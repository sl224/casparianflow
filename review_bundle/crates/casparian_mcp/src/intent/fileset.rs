//! FileSet storage with bounded wire payloads.
//!
//! Large file collections are never returned inline over MCP.
//! They are stored as JSONL manifests and accessed via:
//! - `fileset.sample` - bounded random sample
//! - `fileset.page` - cursor-based paging

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::session::{FileSetEntry, SessionBundle};
use super::types::{FileSetId, FileSetMeta, SamplingMethod};

// ============================================================================
// FileSet Store
// ============================================================================

/// In-memory store for file set metadata.
/// Actual file lists are stored in session bundle JSONL files.
#[derive(Debug, Default)]
pub struct FileSetStore {
    /// Metadata by file set ID
    metadata: HashMap<FileSetId, FileSetMeta>,
}

impl FileSetStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new file set
    pub fn register(&mut self, meta: FileSetMeta) {
        self.metadata.insert(meta.file_set_id, meta);
    }

    /// Get metadata for a file set
    pub fn get_meta(&self, file_set_id: FileSetId) -> Option<&FileSetMeta> {
        self.metadata.get(&file_set_id)
    }

    /// Create a file set from paths
    pub fn create_from_paths(
        &mut self,
        bundle: &SessionBundle,
        paths: Vec<String>,
        sampling_method: SamplingMethod,
        seed: Option<u64>,
    ) -> Result<FileSetMeta, super::session::SessionError> {
        let file_set_id = FileSetId::new();
        let count = paths.len() as u64;

        // Convert to entries
        let entries: Vec<FileSetEntry> = paths
            .into_iter()
            .map(|path| FileSetEntry {
                path,
                size: None,
                content_hash: None,
            })
            .collect();

        // Write to session bundle
        bundle.write_fileset(file_set_id, &entries)?;

        let manifest_ref = format!("corpora/filesets/{}.jsonl", file_set_id);

        let meta = FileSetMeta {
            file_set_id,
            count,
            sampling_method,
            seed,
            manifest_ref,
            created_at: chrono::Utc::now(),
        };

        self.register(meta.clone());

        Ok(meta)
    }

    /// Sample from a file set (bounded)
    pub fn sample(
        &self,
        bundle: &SessionBundle,
        file_set_id: FileSetId,
        n: usize,
        seed: Option<u64>,
    ) -> Result<Vec<FileSetEntry>, super::session::SessionError> {
        let entries = bundle.read_fileset(file_set_id)?;

        if entries.len() <= n {
            return Ok(entries);
        }

        // Deterministic sampling with seed
        let seed = seed.unwrap_or(42);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let mut indices: Vec<usize> = (0..entries.len()).collect();
        indices.shuffle(&mut rng);

        let sampled: Vec<FileSetEntry> = indices
            .into_iter()
            .take(n)
            .map(|i| entries[i].clone())
            .collect();

        Ok(sampled)
    }

    /// Page through a file set
    pub fn page(
        &self,
        bundle: &SessionBundle,
        file_set_id: FileSetId,
        cursor: Option<usize>,
        limit: usize,
    ) -> Result<FileSetPage, super::session::SessionError> {
        let offset = cursor.unwrap_or(0);
        let (items, next_cursor) = bundle.read_fileset_page(file_set_id, offset, limit)?;

        Ok(FileSetPage {
            items,
            next_cursor: next_cursor.map(|c| c.to_string()),
        })
    }

    /// Create a stratified sample from a file set
    pub fn create_stratified_sample(
        &mut self,
        bundle: &SessionBundle,
        source_file_set_id: FileSetId,
        strata_fn: impl Fn(&FileSetEntry) -> String,
        samples_per_stratum: usize,
        seed: u64,
    ) -> Result<FileSetMeta, super::session::SessionError> {
        let entries = bundle.read_fileset(source_file_set_id)?;

        // Group by stratum
        let mut strata: HashMap<String, Vec<&FileSetEntry>> = HashMap::new();
        for entry in &entries {
            let stratum = strata_fn(entry);
            strata.entry(stratum).or_default().push(entry);
        }

        // Sample from each stratum
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut sampled_paths = Vec::new();

        for (_, entries) in strata.iter_mut() {
            entries.shuffle(&mut rng);
            for entry in entries.iter().take(samples_per_stratum) {
                sampled_paths.push(entry.path.clone());
            }
        }

        self.create_from_paths(
            bundle,
            sampled_paths,
            SamplingMethod::StratifiedSample,
            Some(seed),
        )
    }

    /// Create a deterministic sample from a file set
    pub fn create_deterministic_sample(
        &mut self,
        bundle: &SessionBundle,
        source_file_set_id: FileSetId,
        n: usize,
        seed: u64,
    ) -> Result<FileSetMeta, super::session::SessionError> {
        let entries = bundle.read_fileset(source_file_set_id)?;

        if entries.len() <= n {
            // Just reference the source
            return self.create_from_paths(
                bundle,
                entries.into_iter().map(|e| e.path).collect(),
                SamplingMethod::DeterministicSample,
                Some(seed),
            );
        }

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut indices: Vec<usize> = (0..entries.len()).collect();
        indices.shuffle(&mut rng);

        let sampled_paths: Vec<String> = indices
            .into_iter()
            .take(n)
            .map(|i| entries[i].path.clone())
            .collect();

        self.create_from_paths(
            bundle,
            sampled_paths,
            SamplingMethod::DeterministicSample,
            Some(seed),
        )
    }

    /// Create a top-K failures file set
    pub fn create_top_k_failures(
        &mut self,
        bundle: &SessionBundle,
        failures: Vec<(String, u64)>, // (path, failure_count)
        k: usize,
    ) -> Result<FileSetMeta, super::session::SessionError> {
        let mut sorted = failures;
        sorted.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by failure count descending

        let top_paths: Vec<String> = sorted.into_iter().take(k).map(|(path, _)| path).collect();

        self.create_from_paths(bundle, top_paths, SamplingMethod::TopKFailures, None)
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// File set page response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSetPage {
    pub items: Vec<FileSetEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// File set sample response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSetSample {
    pub file_set_id: FileSetId,
    pub examples: Vec<String>,
    pub total_count: u64,
    pub sample_size: usize,
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract extension from path for stratification
pub fn extension_stratum(entry: &FileSetEntry) -> String {
    std::path::Path::new(&entry.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "no_ext".to_string())
}

/// Extract directory prefix for stratification
pub fn dir_prefix_stratum(entry: &FileSetEntry, depth: usize) -> String {
    let path = std::path::Path::new(&entry.path);
    let components: Vec<_> = path.components().take(depth + 1).collect();
    if components.is_empty() {
        return "root".to_string();
    }
    components
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::session::SessionStore;
    use tempfile::TempDir;

    #[test]
    fn test_fileset_create_and_sample() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());
        let bundle = store.create_session("test", None, None).unwrap();

        let mut fs_store = FileSetStore::new();

        let paths: Vec<String> = (0..100).map(|i| format!("/data/file{}.csv", i)).collect();

        let meta = fs_store
            .create_from_paths(&bundle, paths, SamplingMethod::All, None)
            .unwrap();

        assert_eq!(meta.count, 100);

        // Sample with seed
        let sample1 = fs_store
            .sample(&bundle, meta.file_set_id, 10, Some(42))
            .unwrap();
        assert_eq!(sample1.len(), 10);

        // Same seed should give same sample
        let sample2 = fs_store
            .sample(&bundle, meta.file_set_id, 10, Some(42))
            .unwrap();
        assert_eq!(sample1, sample2);

        // Different seed should give different sample
        let sample3 = fs_store
            .sample(&bundle, meta.file_set_id, 10, Some(123))
            .unwrap();
        assert_ne!(sample1, sample3);
    }

    #[test]
    fn test_fileset_paging() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());
        let bundle = store.create_session("test", None, None).unwrap();

        let mut fs_store = FileSetStore::new();

        let paths: Vec<String> = (0..25).map(|i| format!("/data/file{}.csv", i)).collect();

        let meta = fs_store
            .create_from_paths(&bundle, paths, SamplingMethod::All, None)
            .unwrap();

        // Page through
        let page1 = fs_store.page(&bundle, meta.file_set_id, None, 10).unwrap();
        assert_eq!(page1.items.len(), 10);
        assert!(page1.next_cursor.is_some());

        let cursor = page1.next_cursor.unwrap().parse().unwrap();
        let page2 = fs_store
            .page(&bundle, meta.file_set_id, Some(cursor), 10)
            .unwrap();
        assert_eq!(page2.items.len(), 10);
        assert!(page2.next_cursor.is_some());

        let cursor = page2.next_cursor.unwrap().parse().unwrap();
        let page3 = fs_store
            .page(&bundle, meta.file_set_id, Some(cursor), 10)
            .unwrap();
        assert_eq!(page3.items.len(), 5);
        assert!(page3.next_cursor.is_none());
    }

    #[test]
    fn test_stratified_sample() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());
        let bundle = store.create_session("test", None, None).unwrap();

        let mut fs_store = FileSetStore::new();

        // Create files with different extensions
        let mut paths = Vec::new();
        for i in 0..30 {
            paths.push(format!("/data/file{}.csv", i));
        }
        for i in 0..20 {
            paths.push(format!("/data/file{}.parquet", i));
        }
        for i in 0..10 {
            paths.push(format!("/data/file{}.json", i));
        }

        let source_meta = fs_store
            .create_from_paths(&bundle, paths, SamplingMethod::All, None)
            .unwrap();

        // Stratified sample: 5 per extension
        let sampled_meta = fs_store
            .create_stratified_sample(&bundle, source_meta.file_set_id, extension_stratum, 5, 42)
            .unwrap();

        // Should have 15 total (5 csv + 5 parquet + 5 json)
        assert_eq!(sampled_meta.count, 15);
        assert_eq!(
            sampled_meta.sampling_method,
            SamplingMethod::StratifiedSample
        );
    }

    #[test]
    fn test_top_k_failures() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());
        let bundle = store.create_session("test", None, None).unwrap();

        let mut fs_store = FileSetStore::new();

        let failures = vec![
            ("/data/file1.csv".to_string(), 100),
            ("/data/file2.csv".to_string(), 50),
            ("/data/file3.csv".to_string(), 200),
            ("/data/file4.csv".to_string(), 10),
            ("/data/file5.csv".to_string(), 150),
        ];

        let meta = fs_store
            .create_top_k_failures(&bundle, failures, 3)
            .unwrap();

        assert_eq!(meta.count, 3);
        assert_eq!(meta.sampling_method, SamplingMethod::TopKFailures);

        // Verify order (should be top 3 by failure count)
        let entries = bundle.read_fileset(meta.file_set_id).unwrap();
        assert_eq!(entries[0].path, "/data/file3.csv"); // 200
        assert_eq!(entries[1].path, "/data/file5.csv"); // 150
        assert_eq!(entries[2].path, "/data/file1.csv"); // 100
    }
}

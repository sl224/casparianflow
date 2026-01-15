//! Folder Cache - Precomputed folder hierarchy for O(1) navigation
//!
//! Built at scan time and persisted to disk. The TUI loads this cache
//! instead of querying the database, enabling instant folder navigation.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Folder cache version - bump when format changes
const CACHE_VERSION: u32 = 1;

/// Compressed folder cache stored on disk
/// Uses segment interning for memory efficiency:
/// - ~50k unique segments for 1.2M files = ~1MB
/// - zstd compressed = <1MB on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCache {
    /// Cache format version
    pub version: u32,
    /// Source ID this cache is for
    pub source_id: String,
    /// Total file count in source
    pub total_files: usize,
    /// When cache was built (ISO8601)
    pub built_at: String,
    /// Unique path segments (deduplicated via interning)
    pub segments: Vec<String>,
    /// Folder hierarchy - maps prefix to children at that level
    /// Key: prefix (e.g., "", "logs/", "logs/errors/")
    /// Value: list of (segment_idx, file_count, is_file)
    pub folders: HashMap<String, Vec<FolderEntry>>,
}

/// Entry in a folder - references interned segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderEntry {
    /// Index into segments array
    pub segment_idx: u32,
    /// Number of files in this subtree
    pub file_count: u32,
    /// True if this is a file (leaf), false if folder
    pub is_file: bool,
}

impl FolderCache {
    /// Build cache from list of file paths
    /// This is O(n) where n = number of files
    pub fn build(source_id: &str, paths: &[(String,)]) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        // F-008: Use IndexMap for segment interning - maintains insertion order,
        // eliminating the need for a separate Vec during building.
        // PERF: Use get() first to avoid allocation when segment already exists.
        let mut segment_to_idx: IndexMap<String, u32> = IndexMap::new();

        // Build folder structure: prefix -> {segment_idx -> (count, is_file)}
        // PERF: Use u32 segment index as key instead of String to eliminate
        // per-segment allocations in inner loop (was 10M allocs for 1M files).
        let mut folders: HashMap<String, HashMap<u32, (u32, bool)>> = HashMap::new();

        // Reusable buffer for prefix building (O(1) amortized per segment)
        let mut prefix_buf = String::with_capacity(256);

        for (path,) in paths {
            prefix_buf.clear();
            let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let segment_count = path_segments.len();

            for (i, segment) in path_segments.into_iter().enumerate() {
                let is_file = i == segment_count - 1;

                // PERF: Only allocate String when interning a NEW segment.
                // Previously allocated on every segment (5M allocs for 1M files).
                let segment_idx = match segment_to_idx.get(segment) {
                    Some(&idx) => idx,
                    None => {
                        let idx = segment_to_idx.len() as u32;
                        segment_to_idx.insert(segment.to_string(), idx);
                        idx
                    }
                };

                // PERF: Use segment_idx (u32) as key - no allocation.
                // Previously used segment.to_string() (5M allocs for 1M files).
                let level = folders.entry(prefix_buf.clone()).or_default();
                level.entry(segment_idx)
                    .and_modify(|(count, _)| *count += 1)
                    .or_insert((1, is_file));

                // Update prefix for next level (only for folders)
                if !is_file {
                    prefix_buf.push_str(segment);
                    prefix_buf.push('/');
                }
            }
        }

        // Convert to final format - already using segment indices
        let final_folders: HashMap<String, Vec<FolderEntry>> = folders
            .into_iter()
            .map(|(prefix, entries)| {
                let mut folder_entries: Vec<FolderEntry> = entries
                    .into_iter()
                    .map(|(segment_idx, (count, is_file))| {
                        FolderEntry {
                            segment_idx,
                            file_count: count,
                            is_file,
                        }
                    })
                    .collect();
                // Sort by name for consistent ordering
                folder_entries.sort_by(|a, b| {
                    let a_name = segment_to_idx.get_index(a.segment_idx as usize).unwrap().0;
                    let b_name = segment_to_idx.get_index(b.segment_idx as usize).unwrap().0;
                    a_name.cmp(b_name)
                });
                (prefix, folder_entries)
            })
            .collect();

        // F-008: Extract keys from IndexMap to create final segments Vec
        let segments: Vec<String> = segment_to_idx.into_keys().collect();

        Self {
            version: CACHE_VERSION,
            source_id: source_id.to_string(),
            total_files: paths.len(),
            built_at: now,
            segments,
            folders: final_folders,
        }
    }

    /// Get children at a given prefix
    pub fn get_children(&self, prefix: &str) -> Vec<FolderInfo> {
        self.folders
            .get(prefix)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| FolderInfo {
                        name: self.segments[e.segment_idx as usize].clone(),
                        file_count: e.file_count as usize,
                        is_file: e.is_file,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get cache file path for a source
    pub fn cache_path(source_id: &str) -> PathBuf {
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".casparian_flow")
            .join("cache");
        cache_dir.join(format!("folders_{}.bin.zst", source_id))
    }

    /// Save cache to disk (zstd compressed)
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let path = Self::cache_path(&self.source_id);

        // Ensure cache directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize with bincode
        let encoded = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Compress with zstd (level 3 is good balance of speed/size)
        let compressed = zstd::encode_all(encoded.as_slice(), 3)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Write atomically (temp file + rename)
        let temp_path = path.with_extension("bin.zst.tmp");
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        fs::rename(&temp_path, &path)?;

        Ok(path)
    }

    /// Load cache from disk
    pub fn load(source_id: &str) -> std::io::Result<Self> {
        let path = Self::cache_path(source_id);

        // Read compressed file
        let mut file = fs::File::open(&path)?;
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed)?;

        // Decompress
        let decompressed = zstd::decode_all(compressed.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Deserialize
        let cache: FolderCache = bincode::deserialize(&decompressed)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Version check
        if cache.version != CACHE_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Cache version mismatch: expected {}, got {}", CACHE_VERSION, cache.version),
            ));
        }

        Ok(cache)
    }

    /// Check if cache exists for a source
    pub fn exists(source_id: &str) -> bool {
        Self::cache_path(source_id).exists()
    }

    /// Delete cache for a source
    pub fn delete(source_id: &str) -> std::io::Result<()> {
        let path = Self::cache_path(source_id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

/// Folder info for display (resolved from cache)
#[derive(Debug, Clone)]
pub struct FolderInfo {
    pub name: String,
    pub file_count: usize,
    pub is_file: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cache() {
        let paths = vec![
            ("logs/app.log".to_string(),),
            ("logs/errors/crash.log".to_string(),),
            ("logs/errors/fatal.log".to_string(),),
            ("data/users.csv".to_string(),),
        ];

        let cache = FolderCache::build("test_source", &paths);

        assert_eq!(cache.total_files, 4);

        // Root level should have "logs" and "data"
        let root = cache.get_children("");
        assert_eq!(root.len(), 2);

        // Find logs folder
        let logs = root.iter().find(|f| f.name == "logs").unwrap();
        assert_eq!(logs.file_count, 3); // 3 files under logs/
        assert!(!logs.is_file);

        // logs/ level should have "app.log" and "errors"
        let logs_children = cache.get_children("logs/");
        assert_eq!(logs_children.len(), 2);

        // errors/ level should have 2 files
        let errors_children = cache.get_children("logs/errors/");
        assert_eq!(errors_children.len(), 2);
        assert!(errors_children.iter().all(|f| f.is_file));
    }

    #[test]
    fn test_segment_interning() {
        // Same segment appears in multiple paths - should be interned
        let paths = vec![
            ("2024/01/file1.txt".to_string(),),
            ("2024/01/file2.txt".to_string(),),
            ("2024/02/file3.txt".to_string(),),
        ];

        let cache = FolderCache::build("test", &paths);

        // "2024" should appear only once in segments
        let count_2024 = cache.segments.iter().filter(|s| *s == "2024").count();
        assert_eq!(count_2024, 1);

        // "01" should appear only once
        let count_01 = cache.segments.iter().filter(|s| *s == "01").count();
        assert_eq!(count_01, 1);
    }
}

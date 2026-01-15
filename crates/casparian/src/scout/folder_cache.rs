//! Folder Cache - Precomputed folder hierarchy for O(1) navigation
//!
//! Built at scan time and persisted to disk. The TUI loads this cache
//! instead of querying the database, enabling instant folder navigation.
//!
//! ## Data Structure (SoA - Struct of Arrays)
//!
//! Uses struct-of-arrays layout for cache efficiency and fast partial loading:
//! - All strings interned (segments for paths, tag_strings for tags)
//! - Prefixes stored as tree with parent + child pointers
//! - Entries stored as parallel arrays (segment_idx, file_count, is_file)
//!
//! Lookup is O(branching_factor × depth) instead of O(num_prefixes × depth).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Folder cache version - bump when format changes
const CACHE_VERSION: u32 = 3;

/// Sentinel value for "no parent" or "no segment" (root prefix)
const NONE: u32 = u32::MAX;

/// Compressed folder cache stored on disk (SoA layout)
///
/// Memory layout optimized for:
/// - Fast prefix lookup via child pointers: O(branching × depth)
/// - Minimal string duplication via interning
/// - Packed booleans for is_file flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCache {
    // === METADATA ===
    /// Cache format version
    pub version: u32,
    /// Source ID this cache is for
    pub source_id: String,
    /// Total file count in source
    pub total_files: usize,
    /// When cache was built (ISO8601)
    pub built_at: String,

    // === PATH SEGMENTS (intern table 1) ===
    /// Unique path segments - file/folder names
    /// e.g., ["logs", "errors", "app.log", "crash.log", ...]
    pub segments: Vec<String>,

    // === TAG STRINGS (intern table 2) ===
    /// Unique tag names
    /// e.g., ["All files", "csv", "untagged", "medical-records", ...]
    pub tag_strings: Vec<String>,

    // === TAGS (SoA) ===
    /// Index into tag_strings for each tag
    pub tag_name_idx: Vec<u32>,
    /// File count for each tag
    pub tag_counts: Vec<u32>,
    /// Packed booleans: true for special tags ("All files", "untagged")
    pub tag_is_special: Vec<bool>,

    // === PREFIXES (tree structure) ===
    /// Segment index naming this prefix (NONE for root)
    /// e.g., prefix "logs/" has segment "logs"
    pub prefix_segment: Vec<u32>,
    /// Parent prefix index (NONE for root)
    pub prefix_parent: Vec<u32>,
    /// Start index in prefix_children array
    pub prefix_children_start: Vec<u32>,
    /// Number of child prefixes
    pub prefix_children_len: Vec<u16>,
    /// Start index in entry arrays
    pub prefix_entry_start: Vec<u32>,
    /// Number of entries (files + folders) at this prefix
    pub prefix_entry_len: Vec<u16>,

    // === PREFIX CHILDREN (flattened) ===
    /// Child prefix indices, flattened
    /// Allows O(children) lookup instead of O(all_prefixes)
    pub prefix_children: Vec<u32>,

    // === ENTRIES (SoA) ===
    /// Segment index for each entry's name
    pub entry_segment_idx: Vec<u32>,
    /// File count in subtree for each entry
    pub entry_file_count: Vec<u32>,
    /// Packed booleans: true = file, false = folder
    pub entry_is_file: Vec<bool>,
}

/// Tag summary for cache - used during building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSummary {
    /// Tag name ("All files", "untagged", or actual tag)
    pub name: String,
    /// Number of files with this tag
    pub count: usize,
    /// True for "All files" and "untagged" special entries
    pub is_special: bool,
}

/// Temporary entry during building (before flattening to SoA)
#[derive(Debug, Clone)]
struct BuildEntry {
    segment_idx: u32,
    file_count: u32,
    is_file: bool,
}

impl FolderCache {
    /// Build cache from list of file paths
    /// This is O(n) where n = number of files
    pub fn build(source_id: &str, paths: &[(String,)]) -> Self {
        Self::build_with_tags(source_id, paths, Vec::new())
    }

    /// Build cache from file paths with pre-computed tag summaries
    ///
    /// Algorithm:
    /// 1. Intern all path segments
    /// 2. Build folder structure: prefix_string -> entries
    /// 3. Build prefix tree with parent/child indices
    /// 4. Flatten entries into SoA arrays
    pub fn build_with_tags(source_id: &str, paths: &[(String,)], tags: Vec<TagSummary>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        // === PHASE 1: Intern segments and collect folder entries ===
        let mut segment_to_idx: IndexMap<String, u32> = IndexMap::new();
        let mut folders: HashMap<String, HashMap<u32, (u32, bool)>> = HashMap::new();
        let mut prefix_buf = String::with_capacity(256);

        for (path,) in paths {
            prefix_buf.clear();
            let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let segment_count = path_segments.len();

            for (i, segment) in path_segments.into_iter().enumerate() {
                let is_file = i == segment_count - 1;

                let segment_idx = match segment_to_idx.get(segment) {
                    Some(&idx) => idx,
                    None => {
                        let idx = segment_to_idx.len() as u32;
                        segment_to_idx.insert(segment.to_string(), idx);
                        idx
                    }
                };

                let level = folders.entry(prefix_buf.clone()).or_default();
                level
                    .entry(segment_idx)
                    .and_modify(|(count, _)| *count += 1)
                    .or_insert((1, is_file));

                if !is_file {
                    prefix_buf.push_str(segment);
                    prefix_buf.push('/');
                }
            }
        }

        // === PHASE 2: Convert to sorted entries per prefix ===
        let mut prefix_entries: HashMap<String, Vec<BuildEntry>> = folders
            .into_iter()
            .map(|(prefix, entries)| {
                let mut folder_entries: Vec<BuildEntry> = entries
                    .into_iter()
                    .map(|(segment_idx, (count, is_file))| BuildEntry {
                        segment_idx,
                        file_count: count,
                        is_file,
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

        // === PHASE 3: Build prefix tree ===
        // Assign indices: root ("") is always index 0
        let mut prefix_to_idx: HashMap<String, u32> = HashMap::new();
        prefix_to_idx.insert(String::new(), 0);

        // Collect all non-root prefixes and sort for deterministic ordering
        let mut other_prefixes: Vec<String> = prefix_entries
            .keys()
            .filter(|p| !p.is_empty())
            .cloned()
            .collect();
        other_prefixes.sort();

        for prefix in &other_prefixes {
            let idx = prefix_to_idx.len() as u32;
            prefix_to_idx.insert(prefix.clone(), idx);
        }

        let num_prefixes = prefix_to_idx.len();

        // Build prefix arrays
        let mut prefix_segment: Vec<u32> = vec![NONE; num_prefixes];
        let mut prefix_parent: Vec<u32> = vec![NONE; num_prefixes];

        // For each prefix, find its segment name and parent
        for (prefix_str, &prefix_idx) in &prefix_to_idx {
            if prefix_str.is_empty() {
                // Root has no segment and no parent
                prefix_segment[prefix_idx as usize] = NONE;
                prefix_parent[prefix_idx as usize] = NONE;
            } else {
                // Find segment name (last component without trailing /)
                let without_trailing = prefix_str.trim_end_matches('/');
                let last_slash = without_trailing.rfind('/');
                let segment_name = match last_slash {
                    Some(pos) => &without_trailing[pos + 1..],
                    None => without_trailing,
                };

                // Find parent prefix
                let parent_prefix = match last_slash {
                    Some(pos) => format!("{}/", &without_trailing[..pos]),
                    None => String::new(),
                };

                // Look up segment index
                if let Some(&seg_idx) = segment_to_idx.get(segment_name) {
                    prefix_segment[prefix_idx as usize] = seg_idx;
                }

                // Look up parent index
                if let Some(&parent_idx) = prefix_to_idx.get(&parent_prefix) {
                    prefix_parent[prefix_idx as usize] = parent_idx;
                }
            }
        }

        // === PHASE 4: Build child indices ===
        // For each prefix, collect its children
        let mut children_per_prefix: Vec<Vec<u32>> = vec![Vec::new(); num_prefixes];

        for (_, &prefix_idx) in &prefix_to_idx {
            let parent_idx = prefix_parent[prefix_idx as usize];
            if parent_idx != NONE {
                children_per_prefix[parent_idx as usize].push(prefix_idx);
            }
        }

        // Sort children by segment name for consistent ordering
        for children in &mut children_per_prefix {
            children.sort_by(|&a, &b| {
                let seg_a = prefix_segment[a as usize];
                let seg_b = prefix_segment[b as usize];
                if seg_a == NONE || seg_b == NONE {
                    return std::cmp::Ordering::Equal;
                }
                let name_a = segment_to_idx.get_index(seg_a as usize).unwrap().0;
                let name_b = segment_to_idx.get_index(seg_b as usize).unwrap().0;
                name_a.cmp(name_b)
            });
        }

        // Flatten children into single array
        let mut prefix_children: Vec<u32> = Vec::new();
        let mut prefix_children_start: Vec<u32> = vec![0; num_prefixes];
        let mut prefix_children_len: Vec<u16> = vec![0; num_prefixes];

        for (prefix_idx, children) in children_per_prefix.into_iter().enumerate() {
            prefix_children_start[prefix_idx] = prefix_children.len() as u32;
            prefix_children_len[prefix_idx] = children.len() as u16;
            prefix_children.extend(children);
        }

        // === PHASE 5: Flatten entries into SoA ===
        let mut entry_segment_idx: Vec<u32> = Vec::new();
        let mut entry_file_count: Vec<u32> = Vec::new();
        let mut entry_is_file: Vec<bool> = Vec::new();
        let mut prefix_entry_start: Vec<u32> = vec![0; num_prefixes];
        let mut prefix_entry_len: Vec<u16> = vec![0; num_prefixes];

        // Process prefixes in index order
        let mut prefixes_by_idx: Vec<(u32, String)> = prefix_to_idx.into_iter().map(|(s, i)| (i, s)).collect();
        prefixes_by_idx.sort_by_key(|(i, _)| *i);

        for (prefix_idx, prefix_str) in prefixes_by_idx {
            let entries = prefix_entries.remove(&prefix_str).unwrap_or_default();
            prefix_entry_start[prefix_idx as usize] = entry_segment_idx.len() as u32;
            prefix_entry_len[prefix_idx as usize] = entries.len() as u16;

            for entry in entries {
                entry_segment_idx.push(entry.segment_idx);
                entry_file_count.push(entry.file_count);
                entry_is_file.push(entry.is_file);
            }
        }

        // === PHASE 6: Build tag arrays ===
        let mut tag_strings: Vec<String> = Vec::new();
        let mut tag_string_to_idx: HashMap<String, u32> = HashMap::new();
        let mut tag_name_idx: Vec<u32> = Vec::new();
        let mut tag_counts: Vec<u32> = Vec::new();
        let mut tag_is_special: Vec<bool> = Vec::new();

        for tag in tags {
            let str_idx = match tag_string_to_idx.get(&tag.name) {
                Some(&idx) => idx,
                None => {
                    let idx = tag_strings.len() as u32;
                    tag_strings.push(tag.name.clone());
                    tag_string_to_idx.insert(tag.name, idx);
                    idx
                }
            };
            tag_name_idx.push(str_idx);
            tag_counts.push(tag.count as u32);
            tag_is_special.push(tag.is_special);
        }

        // Extract segments from IndexMap
        let segments: Vec<String> = segment_to_idx.into_keys().collect();

        Self {
            version: CACHE_VERSION,
            source_id: source_id.to_string(),
            total_files: paths.len(),
            built_at: now,
            segments,
            tag_strings,
            tag_name_idx,
            tag_counts,
            tag_is_special,
            prefix_segment,
            prefix_parent,
            prefix_children_start,
            prefix_children_len,
            prefix_entry_start,
            prefix_entry_len,
            prefix_children,
            entry_segment_idx,
            entry_file_count,
            entry_is_file,
        }
    }

    /// Find prefix index by path string
    /// O(branching_factor × depth) via tree traversal
    pub fn find_prefix(&self, prefix: &str) -> Option<u32> {
        if prefix.is_empty() {
            return Some(0); // Root is always index 0
        }

        // Walk the tree from root
        let mut current_idx: u32 = 0;
        let segments: Vec<&str> = prefix.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        for target_segment in segments {
            // Find child with matching segment name
            let children_start = self.prefix_children_start[current_idx as usize] as usize;
            let children_len = self.prefix_children_len[current_idx as usize] as usize;

            let mut found = false;
            for i in 0..children_len {
                let child_idx = self.prefix_children[children_start + i];
                let child_segment_idx = self.prefix_segment[child_idx as usize];
                if child_segment_idx != NONE {
                    let child_name = &self.segments[child_segment_idx as usize];
                    if child_name == target_segment {
                        current_idx = child_idx;
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                return None;
            }
        }

        Some(current_idx)
    }

    /// Get entries at a prefix by index
    pub fn get_entries_by_idx(&self, prefix_idx: u32) -> impl Iterator<Item = (u32, u32, bool)> + '_ {
        let start = self.prefix_entry_start[prefix_idx as usize] as usize;
        let len = self.prefix_entry_len[prefix_idx as usize] as usize;

        (0..len).map(move |i| {
            let idx = start + i;
            (
                self.entry_segment_idx[idx],
                self.entry_file_count[idx],
                self.entry_is_file[idx],
            )
        })
    }

    /// Get children at a given prefix (compatibility API)
    pub fn get_children(&self, prefix: &str) -> Vec<FolderInfo> {
        let prefix_idx = match self.find_prefix(prefix) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        self.get_entries_by_idx(prefix_idx)
            .map(|(segment_idx, file_count, is_file)| FolderInfo {
                name: self.segments[segment_idx as usize].clone(),
                file_count: file_count as usize,
                is_file,
            })
            .collect()
    }

    /// Get tag summaries (compatibility API)
    pub fn get_tags(&self) -> Vec<TagSummary> {
        (0..self.tag_name_idx.len())
            .map(|i| TagSummary {
                name: self.tag_strings[self.tag_name_idx[i] as usize].clone(),
                count: self.tag_counts[i] as usize,
                is_special: self.tag_is_special[i],
            })
            .collect()
    }

    /// Get prefix string from prefix index
    /// Walks up the parent chain and builds the full path
    pub fn get_prefix_string(&self, prefix_idx: u32) -> String {
        if prefix_idx == 0 {
            return String::new(); // Root
        }

        // Walk up parent chain collecting segments
        let mut segments: Vec<&str> = Vec::new();
        let mut idx = prefix_idx;

        while idx != NONE && idx != 0 {
            let seg_idx = self.prefix_segment[idx as usize];
            if seg_idx != NONE {
                segments.push(&self.segments[seg_idx as usize]);
            }
            idx = self.prefix_parent[idx as usize];
        }

        // Reverse to get correct order (root-to-leaf)
        segments.reverse();

        // Join with / and add trailing /
        if segments.is_empty() {
            String::new()
        } else {
            format!("{}/", segments.join("/"))
        }
    }

    /// Number of prefixes in the cache
    pub fn num_prefixes(&self) -> usize {
        self.prefix_segment.len()
    }

    /// Convert to HashMap for compatibility with existing code
    /// This builds the old-style HashMap<String, Vec<FolderInfo>> structure
    pub fn to_folder_map(&self) -> HashMap<String, Vec<FolderInfo>> {
        let mut map = HashMap::new();

        for prefix_idx in 0..self.num_prefixes() {
            let prefix_str = self.get_prefix_string(prefix_idx as u32);
            let entries: Vec<FolderInfo> = self
                .get_entries_by_idx(prefix_idx as u32)
                .map(|(segment_idx, file_count, is_file)| FolderInfo {
                    name: self.segments[segment_idx as usize].clone(),
                    file_count: file_count as usize,
                    is_file,
                })
                .collect();
            map.insert(prefix_str, entries);
        }

        map
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

    #[test]
    fn test_prefix_tree_navigation() {
        let paths = vec![
            ("a/b/c/file1.txt".to_string(),),
            ("a/b/d/file2.txt".to_string(),),
            ("a/e/file3.txt".to_string(),),
        ];

        let cache = FolderCache::build("test", &paths);

        // Test find_prefix
        assert_eq!(cache.find_prefix(""), Some(0));
        assert!(cache.find_prefix("a/").is_some());
        assert!(cache.find_prefix("a/b/").is_some());
        assert!(cache.find_prefix("a/b/c/").is_some());
        assert!(cache.find_prefix("nonexistent/").is_none());

        // Test get_prefix_string round-trip
        for idx in 0..cache.num_prefixes() {
            let prefix_str = cache.get_prefix_string(idx as u32);
            let found_idx = cache.find_prefix(&prefix_str);
            assert_eq!(found_idx, Some(idx as u32), "Round-trip failed for prefix '{}'", prefix_str);
        }
    }

    #[test]
    fn test_to_folder_map_compatibility() {
        let paths = vec![
            ("logs/app.log".to_string(),),
            ("logs/errors/crash.log".to_string(),),
            ("data/users.csv".to_string(),),
        ];

        let cache = FolderCache::build("test", &paths);
        let map = cache.to_folder_map();

        // Verify root level
        let root = map.get("").expect("Root should exist");
        assert_eq!(root.len(), 2); // "logs" and "data"

        // Verify logs/ level
        let logs = map.get("logs/").expect("logs/ should exist");
        assert_eq!(logs.len(), 2); // "app.log" and "errors"

        // Verify logs/errors/ level
        let errors = map.get("logs/errors/").expect("logs/errors/ should exist");
        assert_eq!(errors.len(), 1); // "crash.log"
    }

    #[test]
    #[ignore] // Run with: cargo test -p casparian --lib test_large_cache_perf -- --ignored --nocapture
    fn test_large_cache_perf() {
        use std::time::Instant;

        // Generate ~1M paths simulating a realistic file hierarchy (like /Users/shan)
        let mut paths = Vec::with_capacity(1_000_000);

        // Simulate various directory structures
        for year in 2015..2025 {
            for month in 1..=12 {
                for day in 1..=28 {
                    // Logs - deep hierarchy
                    for hour in 0..24 {
                        for file_num in 0..2 {
                            paths.push((format!(
                                "logs/{}/{:02}/{:02}/hour_{:02}/app_{}.log",
                                year, month, day, hour, file_num
                            ),));
                        }
                    }
                    // Downloads - flatter
                    for file_num in 0..5 {
                        paths.push((format!(
                            "Downloads/{}/{:02}/file_{}.pdf",
                            year, month, file_num
                        ),));
                    }
                }
            }
        }

        // Add some more varied paths
        for i in 0..10000 {
            paths.push((format!("Documents/project_{}/src/main.rs", i),));
            paths.push((format!("Documents/project_{}/README.md", i),));
        }

        println!("Generated {} paths", paths.len());

        // Measure build time
        let start = Instant::now();
        let cache = FolderCache::build("perf_test", &paths);
        let build_time = start.elapsed();
        println!("Build time: {:?}", build_time);
        println!("Segments: {}", cache.segments.len());
        println!("Prefixes: {}", cache.num_prefixes());
        println!("Entries: {}", cache.entry_segment_idx.len());

        // Measure save time
        let start = Instant::now();
        let path = cache.save().expect("Save should succeed");
        let save_time = start.elapsed();
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("Save time: {:?}", save_time);
        println!("File size: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1024.0 / 1024.0);

        // Measure load time
        let start = Instant::now();
        let loaded = FolderCache::load("perf_test").expect("Load should succeed");
        let load_time = start.elapsed();
        println!("Load time: {:?}", load_time);

        // Measure to_folder_map time
        let start = Instant::now();
        let _map = loaded.to_folder_map();
        let convert_time = start.elapsed();
        println!("to_folder_map time: {:?}", convert_time);

        // Verify correctness
        assert_eq!(loaded.total_files, paths.len());

        // Cleanup
        let _ = std::fs::remove_file(&path);

        // Performance assertions (adjust based on actual measurements)
        assert!(build_time.as_secs() < 5, "Build should take <5s");
        assert!(load_time.as_secs() < 2, "Load should take <2s");
    }
}

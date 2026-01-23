//! Filesystem scanner with polling-based change detection
//!
//! This module provides the core scanning functionality. It uses polling
//! instead of inotify because inotify doesn't work on network filesystems
//! (SMB, NFS, S3-fuse).
//!
//! # Design
//!
//! - Walk the filesystem using `ignore::WalkParallel` for parallel walking
//! - Compare against SQLite state to detect new/changed/deleted files
//! - Queue pending files for processing
//! - Support progress callbacks for UI updates

use super::db::Database;
use super::error::{Result, ScoutError};
use super::types::{ScanStats, ScannedFile, Source, SourceId};
use chrono::Utc;
use ignore::WalkBuilder;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// GAP-SCAN-003: Normalize a path to use forward slashes consistently.
/// This is critical for Windows compatibility since `split_rel_path()` in types.rs
/// only looks for '/' separators.
fn normalize_path_to_forward_slashes(path: &Path) -> String {
    // Use components() to get a platform-independent path representation,
    // then join with forward slashes
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Configuration for scanning operations
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Number of threads for parallel walking (0 = auto-detect CPU count)
    pub threads: usize,
    /// Batch size for accumulating files before DB operations
    pub batch_size: usize,
    /// Progress update interval (number of files between updates)
    pub progress_interval: usize,
    /// Whether to follow symlinks
    pub follow_symlinks: bool,
    /// Whether to include hidden files/directories
    pub include_hidden: bool,
    /// Directory name patterns to exclude (e.g., "node_modules", ".git")
    /// These are matched against directory names, not full paths
    pub exclude_dir_names: Vec<String>,
    /// Path patterns to exclude (matched against full path)
    /// E.g., "Library/CloudStorage" will exclude any path containing this
    pub exclude_path_patterns: Vec<String>,
}

/// Default directory exclusions to avoid scanning slow/problematic filesystems
///
/// These are excluded by default because they:
/// 1. Use virtual filesystems that can hang on readdir (CloudStorage, Mobile Documents)
/// 2. Are typically not useful for data processing (.Trash, Caches)
/// 3. Can cause infinite loops or excessive I/O (node_modules, .git)
pub const DEFAULT_EXCLUDE_DIR_NAMES: &[&str] = &[
    "node_modules", // Often huge and not useful for data processing
    ".git",         // Git internals
    "__pycache__",  // Python cache
    ".cache",       // Various caches
];

/// Default path patterns to exclude
///
/// These patterns match against the full path and are critical for avoiding
/// cloud storage directories that use FUSE-like virtual filesystems.
/// When the scanner reads these directories, the filesystem driver may need
/// to fetch metadata from the cloud, causing the readdir syscall to block
/// indefinitely (the root cause of scan hangs).
pub const DEFAULT_EXCLUDE_PATH_PATTERNS: &[&str] = &[
    // macOS cloud storage - uses File Provider framework (virtual FS)
    "Library/CloudStorage",     // Google Drive, OneDrive, Dropbox via macOS
    "Library/Mobile Documents", // iCloud Drive
    // Legacy iCloud location
    "iCloud Drive",
    // Common cloud sync folders in home directory
    "Google Drive",
    "OneDrive",
    "Dropbox",
    // macOS system directories that are slow or problematic
    ".Trash",
    "Library/Caches", // Can be huge and changes frequently
];

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            threads: 0,             // Auto-detect CPU count
            batch_size: 10_000,     // Flush to DB every 10k files
            progress_interval: 100, // Progress update every 100 files for responsive TUI
            follow_symlinks: false,
            include_hidden: true,
            exclude_dir_names: DEFAULT_EXCLUDE_DIR_NAMES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            exclude_path_patterns: DEFAULT_EXCLUDE_PATH_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

/// Progress update during a scan
#[derive(Debug, Clone)]
pub struct ScanProgress {
    /// Number of directories scanned
    pub dirs_scanned: usize,
    /// Number of files found (discovered by walker)
    pub files_found: usize,
    /// Number of files persisted to database
    pub files_persisted: usize,
    /// Current directory being scanned (hint)
    pub current_dir: Option<String>,
}

/// Result of a scan operation
#[derive(Debug)]
pub struct ScanResult {
    /// Scan statistics
    pub stats: ScanStats,
    /// Errors encountered during scan
    pub errors: Vec<ScanError>,
}

/// Scan error details
#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

/// Filesystem scanner
pub struct Scanner {
    db: Database,
    config: ScanConfig,
}

impl Scanner {
    /// Create a new scanner with the given database and default config
    pub fn new(db: Database) -> Self {
        Self {
            db,
            config: ScanConfig::default(),
        }
    }

    /// Create a new scanner with custom configuration
    pub fn with_config(db: Database, config: ScanConfig) -> Self {
        Self { db, config }
    }

    /// Scan a source directory and update the database.
    ///
    /// Convenience wrapper for `scan()` with no progress reporting or tagging.
    pub fn scan_source(&self, source: &Source) -> Result<ScanResult> {
        self.scan(source, None, None)
    }

    /// Scan a source with optional progress updates and tagging.
    ///
    /// This is the main scan implementation. Features:
    /// - Parallel filesystem walk using ignore::WalkParallel
    /// - Streaming persist: files are written to DB as batches arrive (O(batch_size) memory)
    /// - Bounded channel with backpressure prevents memory blowup
    /// - Optional progress updates via channel for TUI
    /// - Optional tagging of all discovered files
    pub fn scan(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        tag: Option<&str>,
    ) -> Result<ScanResult> {
        let start = Instant::now();
        info!(source = %source.name, path = %source.path, "Starting streaming scan");

        let source_path = Path::new(&source.path);
        if !source_path.exists() {
            return Err(ScoutError::FileNotFound(source.path.clone()));
        }

        // Send initial progress so UI shows something immediately
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(ScanProgress {
                dirs_scanned: 0,
                files_found: 0,
                files_persisted: 0,
                current_dir: Some(source.path.clone()),
            });
        }

        // Record scan start time for deleted file detection
        let scan_start = Utc::now();

        // Create bounded channel for backpressure (GAP-006)
        // 10 batches in flight = batch_size * 10 files max in memory
        let (batch_tx, batch_rx) = mpsc::sync_channel::<Vec<ScannedFile>>(10);

        // Shared counter for files_persisted (updated by persist task, read by walker for progress)
        let files_persisted_counter = Arc::new(AtomicUsize::new(0));
        let files_persisted_for_persist = files_persisted_counter.clone();

        // Critical fix: Track if all batches succeeded (GAP-SCAN-001)
        // Only mark files as deleted if no batch persist failures occurred
        let scan_ok = Arc::new(AtomicBool::new(true));
        let scan_ok_for_persist = scan_ok.clone();

        // Spawn the walk task; persistence happens on the calling thread
        let db = self.db.clone();
        let tag_owned = tag.map(|t| t.to_string());
        let batch_size = self.config.batch_size;

        // Run the parallel walk in a blocking task, sending batches to channel
        let walk_source_path = source_path.to_path_buf();
        let walk_source_id = source.id.clone();
        let walk_config_batch_size = self.config.batch_size;
        let walk_config_threads = self.config.threads;
        let walk_config_include_hidden = self.config.include_hidden;
        let walk_config_follow_symlinks = self.config.follow_symlinks;
        let walk_progress_interval = self.config.progress_interval;
        let walk_progress_tx = progress_tx.clone();
        let walk_files_persisted = files_persisted_counter.clone();
        let walk_exclude_dir_names = self.config.exclude_dir_names.clone();
        let walk_exclude_path_patterns = self.config.exclude_path_patterns.clone();

        let walk_handle = std::thread::spawn(move || {
            Self::parallel_walk(
                &walk_source_path,
                &walk_source_id,
                batch_tx,
                walk_config_batch_size,
                walk_config_threads,
                walk_config_include_hidden,
                walk_config_follow_symlinks,
                walk_progress_interval,
                walk_progress_tx,
                walk_files_persisted,
                walk_exclude_dir_names,
                walk_exclude_path_patterns,
            )
        });

        let mut persist_stats = ScanStats::default();
        while let Ok(batch) = batch_rx.recv() {
            let batch_len = batch.len();

            // GAP-SCAN-005: Track files_discovered as total files received (including failed)
            // This represents what the walker found, regardless of persist success
            persist_stats.files_discovered += batch_len as u64;

            // GAP-002: Transactional batch persist
            match Self::persist_batch_streaming(&db, batch, tag_owned.as_deref()) {
                Ok(batch_stats) => {
                    persist_stats.files_new += batch_stats.files_new;
                    persist_stats.files_changed += batch_stats.files_changed;
                    persist_stats.files_unchanged += batch_stats.files_unchanged;
                    // GAP-SCAN-005: Track files_persisted separately from files_discovered
                    persist_stats.files_persisted += batch_len as u64;

                    // Update shared counter for progress reporting
                    files_persisted_for_persist.fetch_add(batch_len, Ordering::Relaxed);
                }
                Err(e) => {
                    persist_stats.errors += batch_len as u64;
                    // GAP-SCAN-001: Mark scan as failed so we don't incorrectly mark files as deleted
                    scan_ok_for_persist.store(false, Ordering::Relaxed);
                    tracing::warn!(error = %e, "Batch persist failed - will skip mark_deleted_files");
                }
            }
        }

        let walk_result = walk_handle
            .join()
            .map_err(|_| ScoutError::InvalidState("Walk task panicked".to_string()))?;
        let (walk_stats, walk_errors) = walk_result?;

        // Combine stats
        let mut final_stats = persist_stats;
        final_stats.dirs_scanned = walk_stats.dirs_scanned;
        final_stats.bytes_scanned = walk_stats.bytes_scanned;

        // Send final progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(ScanProgress {
                dirs_scanned: final_stats.dirs_scanned as usize,
                files_found: final_stats.files_discovered as usize,
                files_persisted: files_persisted_counter.load(Ordering::Relaxed),
                current_dir: None,
            });
        }

        // GAP-SCAN-001: Only mark files as deleted if ALL batches were persisted successfully
        // If any batch failed, files in that batch weren't updated with last_seen_at,
        // so marking them deleted would be incorrect (data loss)
        let deleted = if scan_ok.load(Ordering::Relaxed) {
            self.db.mark_deleted_files(&source.id, scan_start)?
        } else {
            tracing::warn!(
                source = %source.name,
                errors = final_stats.errors,
                "Skipping mark_deleted_files due to batch persist failures"
            );
            0
        };
        final_stats.files_deleted = deleted;
        final_stats.duration_ms = start.elapsed().as_millis() as u64;
        final_stats.errors += walk_errors.len() as u64;

        // Update denormalized file_count on source for fast TUI queries
        // GAP-SCAN-005: Use files_persisted (actual count in DB) not files_discovered
        if let Err(e) = self
            .db
            .update_source_file_count(&source.id, final_stats.files_persisted as usize)
        {
            tracing::warn!(source_id = %source.id, error = %e, "Failed to update source file_count");
        }

        // Populate folder cache for O(1) TUI navigation (avoids 20+ second root folder query)
        if let Err(e) = self.db.populate_folder_cache(&source.id) {
            tracing::warn!(source_id = %source.id, error = %e, "Failed to populate folder cache");
        }

        // GAP-SCAN-005: Log both discovered (walker found) and persisted (saved to DB)
        info!(
            source = %source.name,
            discovered = final_stats.files_discovered,
            persisted = final_stats.files_persisted,
            new = final_stats.files_new,
            changed = final_stats.files_changed,
            deleted = final_stats.files_deleted,
            errors = final_stats.errors,
            duration_ms = final_stats.duration_ms,
            batches = batch_size,
            "Streaming scan complete"
        );

        Ok(ScanResult {
            stats: final_stats,
            errors: walk_errors,
        })
    }

    /// Streaming parallel walk - sends batches to channel instead of collecting
    ///
    /// This is the GAP-006 fix: O(batch_size) memory instead of O(file_count).
    /// Walker threads send batches via bounded channel with backpressure.
    ///
    /// GAP-SCAN-011: Excludes cloud storage directories to prevent hangs on FUSE-like
    /// virtual filesystems (Google Drive, iCloud, OneDrive, etc.)
    #[allow(clippy::too_many_arguments)]
    fn parallel_walk(
        source_path: &Path,
        source_id: &SourceId,
        batch_tx: mpsc::SyncSender<Vec<ScannedFile>>,
        batch_size: usize,
        threads: usize,
        include_hidden: bool,
        follow_symlinks: bool,
        progress_interval: usize,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        files_persisted: Arc<AtomicUsize>,
        exclude_dir_names: Vec<String>,
        exclude_path_patterns: Vec<String>,
    ) -> Result<(ScanStats, Vec<ScanError>)> {
        let (error_tx, error_rx) = std::sync::mpsc::channel::<ScanError>();

        // Atomic counters for progress
        // GAP-SCAN-007: Use AtomicU64 for bytes to prevent overflow on 32-bit systems
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_bytes = Arc::new(AtomicU64::new(0));
        let last_progress_at = Arc::new(AtomicUsize::new(0));
        // Counter for skipped directories (for logging)
        let dirs_skipped = Arc::new(AtomicUsize::new(0));

        // GAP-SCAN-011: Build exclusion filter for cloud storage and slow directories
        // This prevents the scanner from hanging on FUSE-like virtual filesystems
        let exclude_dir_names_arc: Arc<[String]> = Arc::from(exclude_dir_names);
        let exclude_path_patterns_arc: Arc<[String]> = Arc::from(exclude_path_patterns);
        let dirs_skipped_clone = dirs_skipped.clone();

        let walker = WalkBuilder::new(source_path)
            .threads(threads)
            .hidden(!include_hidden)
            .follow_links(follow_symlinks)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .filter_entry(move |entry| {
                // Only apply exclusions to directories
                if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    return true;
                }

                let path = entry.path();
                let path_str = path.to_string_lossy();

                // Check directory name exclusions (e.g., "node_modules", ".git")
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    for exclude_name in exclude_dir_names_arc.iter() {
                        if name_str == *exclude_name {
                            dirs_skipped_clone.fetch_add(1, Ordering::Relaxed);
                            return false; // Skip this directory
                        }
                    }
                }

                // Check path pattern exclusions (e.g., "Library/CloudStorage")
                // This is critical for avoiding cloud storage virtual filesystems
                for pattern in exclude_path_patterns_arc.iter() {
                    if path_str.contains(pattern.as_str()) {
                        dirs_skipped_clone.fetch_add(1, Ordering::Relaxed);
                        tracing::debug!(
                            path = %path_str,
                            pattern = %pattern,
                            "Skipping directory matching exclusion pattern"
                        );
                        return false; // Skip this directory
                    }
                }

                true // Include this directory
            })
            .build_parallel();

        let source_id_arc = source_id.clone();
        let source_path_owned = source_path.to_path_buf();
        let batch_tx = batch_tx.clone();

        walker.run(|| {
            let source_path = source_path_owned.clone();
            let source_id = source_id_arc.clone();
            let error_tx = error_tx.clone();
            let total_files = total_files.clone();
            let total_dirs = total_dirs.clone();
            let total_bytes = total_bytes.clone();
            let last_progress_at = last_progress_at.clone();
            let progress_tx = progress_tx.clone();
            let batch_tx = batch_tx.clone();
            let files_persisted = files_persisted.clone();

            // Thread-local batch - sent to channel when full
            // GAP-SCAN-007: Use u64 for byte_count to prevent overflow on 32-bit systems
            // GAP-SCAN-006: dir_count removed - directories counted immediately via atomic
            struct StreamingFlushGuard {
                batch: Vec<ScannedFile>,
                batch_size: usize,
                byte_count: u64,
                batch_tx: mpsc::SyncSender<Vec<ScannedFile>>,
                total_files: Arc<AtomicUsize>,
                total_bytes: Arc<AtomicU64>,
            }

            impl Drop for StreamingFlushGuard {
                fn drop(&mut self) {
                    // GAP-SCAN-006: Always flush byte_count, even if batch is empty
                    if !self.batch.is_empty() {
                        let batch = std::mem::take(&mut self.batch);
                        let batch_len = batch.len();
                        let _ = self.batch_tx.send(batch);
                        self.total_files.fetch_add(batch_len, Ordering::Relaxed);
                    }
                    // Always flush byte count (GAP-SCAN-006)
                    // Note: dir_count is now updated immediately when dirs are encountered
                    if self.byte_count > 0 {
                        self.total_bytes
                            .fetch_add(self.byte_count, Ordering::Relaxed);
                    }
                }
            }

            let mut guard = StreamingFlushGuard {
                batch: Vec::with_capacity(batch_size),
                batch_size,
                byte_count: 0,
                batch_tx: batch_tx.clone(),
                total_files: total_files.clone(),
                total_bytes: total_bytes.clone(),
            };
            let mut current_dir_hint: Option<String> = None;

            Box::new(move |entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = error_tx.send(ScanError {
                            path: "unknown".to_string(),
                            message: e.to_string(),
                        });
                        return ignore::WalkState::Continue;
                    }
                };

                let file_path = entry.path();

                if file_path == source_path {
                    return ignore::WalkState::Continue;
                }

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = error_tx.send(ScanError {
                            path: file_path.display().to_string(),
                            message: e.to_string(),
                        });
                        return ignore::WalkState::Continue;
                    }
                };

                if metadata.is_dir() {
                    // GAP-SCAN-006: Count directories immediately for accurate real-time progress
                    // Previously only flushed on batch send, causing under-reporting
                    total_dirs.fetch_add(1, Ordering::Relaxed);
                    let dir_count = total_dirs.load(Ordering::Relaxed);
                    if dir_count % 100 == 0 {
                        current_dir_hint = file_path
                            .strip_prefix(&source_path)
                            .map(|p| p.display().to_string())
                            .ok();
                    }
                    return ignore::WalkState::Continue;
                }

                // GAP-SCAN-009: Use entry.file_type() for reliable symlink detection
                // entry.metadata() follows symlinks, so metadata.is_symlink() can be false
                // even for symlink entries. entry.file_type() is more reliable.
                if entry.file_type().map_or(false, |ft| ft.is_symlink()) {
                    return ignore::WalkState::Continue;
                }

                // GAP-SCAN-003: Use normalized forward-slash paths for cross-platform compatibility
                let rel_path = file_path
                    .strip_prefix(&source_path)
                    .map(|p| normalize_path_to_forward_slashes(p))
                    .unwrap_or_else(|_| normalize_path_to_forward_slashes(file_path));

                let full_path = file_path.to_string_lossy().into_owned();
                let size = metadata.len();
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                guard.batch.push(ScannedFile::from_parts(
                    source_id.clone(),
                    full_path,
                    rel_path,
                    size,
                    mtime,
                ));
                // GAP-SCAN-007: No cast needed, both are u64
                guard.byte_count += size;

                // GAP-SCAN-008: Send progress updates on every N files, not just batch flush
                // Calculate current total including in-flight batch files
                let current_total = total_files.load(Ordering::Relaxed) + guard.batch.len();
                let last = last_progress_at.load(Ordering::Relaxed);
                if current_total.saturating_sub(last) >= progress_interval {
                    if last_progress_at
                        .compare_exchange(last, current_total, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                    {
                        if let Some(tx) = &progress_tx {
                            let dir_hint = current_dir_hint.clone();
                            let _ = tx.send(ScanProgress {
                                dirs_scanned: total_dirs.load(Ordering::Relaxed),
                                files_found: current_total,
                                files_persisted: files_persisted.load(Ordering::Relaxed),
                                current_dir: dir_hint,
                            });
                        }
                    }
                }

                // Send batch to channel when full
                if guard.batch.len() >= guard.batch_size {
                    let batch =
                        std::mem::replace(&mut guard.batch, Vec::with_capacity(guard.batch_size));
                    let batch_len = batch.len();

                    // send provides backpressure with sync_channel (waits if channel full)
                    if guard.batch_tx.send(batch).is_err() {
                        // Channel closed - persist task exited early
                        return ignore::WalkState::Quit;
                    }

                    let new_total = total_files.fetch_add(batch_len, Ordering::Relaxed) + batch_len;
                    // Note: directories are counted immediately when discovered (not batched)
                    total_bytes.fetch_add(guard.byte_count, Ordering::Relaxed);
                    guard.byte_count = 0;

                    // Send progress update
                    let last = last_progress_at.load(Ordering::Relaxed);
                    if new_total.saturating_sub(last) >= progress_interval {
                        if last_progress_at
                            .compare_exchange(last, new_total, Ordering::Relaxed, Ordering::Relaxed)
                            .is_ok()
                        {
                            if let Some(tx) = &progress_tx {
                                let dir_hint = current_dir_hint.clone();
                                let _ = tx.send(ScanProgress {
                                    dirs_scanned: total_dirs.load(Ordering::Relaxed),
                                    files_found: new_total,
                                    files_persisted: files_persisted.load(Ordering::Relaxed),
                                    current_dir: dir_hint,
                                });
                            }
                        }
                    }
                }

                ignore::WalkState::Continue
            })
        });

        drop(error_tx);
        let errors: Vec<ScanError> = error_rx.into_iter().collect();

        let stats = ScanStats {
            files_discovered: 0, // Will be updated by persist task
            dirs_scanned: total_dirs.load(Ordering::Relaxed) as u64,
            bytes_scanned: total_bytes.load(Ordering::Relaxed) as u64,
            errors: errors.len() as u64,
            ..Default::default()
        };

        Ok((stats, errors))
    }

    /// Persist a batch of files with transactional consistency
    ///
    /// GAP-002: Uses explicit transaction for atomic batch writes.
    /// All files in the batch are persisted in a single transaction,
    /// reducing fsync overhead from O(n) to O(1).
    ///
    /// # Returns
    ///
    /// - `Ok(ScanStats)` on success (includes per-file error counts)
    /// - `Err` on transaction-level failure (BEGIN/COMMIT failed)
    ///
    /// GAP-SCAN-001 FIX: Propagates transaction errors to caller so `scan_ok`
    /// can be set to false, preventing incorrect `mark_deleted_files` calls.
    fn persist_batch_streaming(
        db: &Database,
        files: Vec<ScannedFile>,
        tag: Option<&str>,
    ) -> Result<ScanStats> {
        let mut stats = ScanStats::default();

        // Persist entire batch in one transaction
        // Note: batch_upsert_files handles per-file errors internally
        let result = db.batch_upsert_files(&files, tag)?;

        stats.files_new = result.new;
        stats.files_changed = result.changed;
        stats.files_unchanged = result.unchanged;
        stats.errors = result.errors;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::types::{FileStatus, SourceId, SourceType};
    use filetime::{set_file_mtime, FileTime};
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_env() -> (TempDir, Database, Source) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_in_memory().unwrap();

        let source = Source {
            id: SourceId::new(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().to_string_lossy().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        (temp_dir, db, source)
    }

    fn create_test_file(dir: &Path, name: &str, content: &str) -> std::io::Result<()> {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    #[test]
    fn test_scan_empty_directory() {
        let (_temp_dir, db, source) = create_test_env();
        let scanner = Scanner::new(db);

        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_discovered, 0);
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.errors, 0);
    }

    #[test]
    fn test_scan_discovers_files() {
        let (temp_dir, db, source) = create_test_env();

        // Create some test files
        create_test_file(temp_dir.path(), "file1.csv", "a,b,c\n1,2,3").unwrap();
        create_test_file(temp_dir.path(), "file2.json", "{}").unwrap();
        create_test_file(temp_dir.path(), "subdir/file3.txt", "hello").unwrap();

        let scanner = Scanner::new(db.clone());
        let result = scanner.scan_source(&source).unwrap();

        assert_eq!(result.stats.files_discovered, 3);
        assert_eq!(result.stats.files_new, 3);
        assert_eq!(result.stats.files_changed, 0);
        assert_eq!(result.stats.errors, 0);

        // Verify files are in database
        let pending = db.list_pending_files(&source.id, 10).unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_scan_detects_changes() {
        let (temp_dir, db, source) = create_test_env();

        // Create initial file with explicit old mtime
        let file_path = temp_dir.path().join("data.csv");
        create_test_file(temp_dir.path(), "data.csv", "a,b,c\n1,2,3").unwrap();
        let old_mtime = FileTime::from_unix_time(1000000, 0);
        set_file_mtime(&file_path, old_mtime).unwrap();

        let scanner = Scanner::new(db.clone());

        // First scan
        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_new, 1);

        // Second scan - no changes
        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.files_unchanged, 1);
        assert_eq!(result.stats.files_changed, 0);

        // Modify the file with a newer mtime
        std::fs::write(&file_path, "a,b,c,d\n1,2,3,4").unwrap();
        let new_mtime = FileTime::from_unix_time(2000000, 0);
        set_file_mtime(&file_path, new_mtime).unwrap();

        // Third scan - should detect change
        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.files_changed, 1);
    }

    #[test]
    fn test_scan_detects_deleted_files() {
        let (temp_dir, db, source) = create_test_env();

        // Create files
        create_test_file(temp_dir.path(), "keep.csv", "data").unwrap();
        create_test_file(temp_dir.path(), "delete.csv", "data").unwrap();

        let scanner = Scanner::new(db.clone());

        // First scan
        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_new, 2);

        // Delete one file
        std::fs::remove_file(temp_dir.path().join("delete.csv")).unwrap();

        // Wait 2ms to ensure scan_start timestamp is after last_seen_at
        // (mark_deleted_files uses scan_start time comparison)
        std::thread::sleep(std::time::Duration::from_millis(2));

        // Second scan
        let result = scanner.scan_source(&source).unwrap();
        assert_eq!(result.stats.files_discovered, 1);
        assert_eq!(result.stats.files_deleted, 1);

        // Verify deleted file is marked in database
        let deleted = db.list_files_by_status(FileStatus::Deleted, 10).unwrap();
        assert_eq!(deleted.len(), 1);
        assert!(deleted[0].path.contains("delete.csv"));
    }

    #[test]
    fn test_scan_nonexistent_source() {
        let db = Database::open_in_memory().unwrap();
        let source = Source {
            id: SourceId::new(),
            name: "Missing".to_string(),
            source_type: SourceType::Local,
            path: "/nonexistent/path".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };

        let scanner = Scanner::new(db);
        let result = scanner.scan_source(&source);
        assert!(result.is_err());
    }

    // ========================================================================
    // Streaming scan tests (GAP-006)
    // ========================================================================

    #[test]
    fn test_scan_streaming_discovers_files() {
        let (temp_dir, db, source) = create_test_env();

        // Create some test files
        create_test_file(temp_dir.path(), "file1.csv", "a,b,c\n1,2,3").unwrap();
        create_test_file(temp_dir.path(), "file2.json", "{}").unwrap();
        create_test_file(temp_dir.path(), "subdir/file3.txt", "hello").unwrap();

        let scanner = Scanner::new(db.clone());

        // Use streaming scan
        let result = scanner.scan(&source, None, None).unwrap();

        assert_eq!(result.stats.files_discovered, 3);
        assert_eq!(result.stats.files_new, 3);
        assert_eq!(result.stats.files_changed, 0);
        assert_eq!(result.stats.errors, 0);

        // Verify files are in database
        let pending = db.list_pending_files(&source.id, 10).unwrap();
        assert_eq!(pending.len(), 3);
    }
}

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
use super::types::{ScanStats, ScannedFile, Source};
use chrono::Utc;
use ignore::WalkBuilder;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::info;

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
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            threads: 0,             // Auto-detect CPU count
            batch_size: 1000,       // Flush to DB every 1000 files
            progress_interval: 100, // Progress update every 100 files for responsive TUI
            follow_symlinks: false,
            include_hidden: true,
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
#[allow(dead_code)] // Used in tests and future scheduling
pub struct ScanResult {
    /// Scan statistics
    pub stats: ScanStats,
    /// Errors encountered during scan
    pub errors: Vec<(String, String)>,
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
    pub async fn scan_source(&self, source: &Source) -> Result<ScanResult> {
        self.scan(source, None, None).await
    }

    /// Scan a source with optional progress updates and tagging.
    ///
    /// This is the main scan implementation. Features:
    /// - Parallel filesystem walk using ignore::WalkParallel
    /// - Streaming persist: files are written to DB as batches arrive (O(batch_size) memory)
    /// - Bounded channel with backpressure prevents memory blowup
    /// - Optional progress updates via channel for TUI
    /// - Optional tagging of all discovered files
    pub async fn scan(
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
            let _ = tx.try_send(ScanProgress {
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
        let (batch_tx, mut batch_rx) = mpsc::channel::<Vec<ScannedFile>>(10);

        // Shared counter for files_persisted (updated by persist task, read by walker for progress)
        let files_persisted_counter = Arc::new(AtomicUsize::new(0));
        let files_persisted_for_persist = files_persisted_counter.clone();

        // Spawn the persist task that processes batches as they arrive
        let db = self.db.clone();
        let tag_owned = tag.map(|t| t.to_string());
        let batch_size = self.config.batch_size;

        let persist_handle = tokio::spawn(async move {
            let mut stats = ScanStats::default();

            while let Some(batch) = batch_rx.recv().await {
                let batch_len = batch.len();

                // GAP-002: Transactional batch persist
                match Self::persist_batch_streaming(&db, batch, tag_owned.as_deref()).await {
                    Ok(batch_stats) => {
                        stats.files_new += batch_stats.files_new;
                        stats.files_changed += batch_stats.files_changed;
                        stats.files_unchanged += batch_stats.files_unchanged;
                        stats.files_discovered += batch_len as u64;

                        // Update shared counter for progress reporting
                        files_persisted_for_persist.fetch_add(batch_len, Ordering::Relaxed);
                    }
                    Err(e) => {
                        stats.errors += batch_len as u64;
                        tracing::warn!(error = %e, "Batch persist failed");
                    }
                }
            }

            stats
        });

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

        let walk_handle = tokio::task::spawn_blocking(move || {
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
            )
        });

        // Wait for walk to complete (this drops the sender, signaling persist task)
        let (walk_stats, walk_errors) = walk_handle
            .await
            .map_err(|e| ScoutError::Config(format!("Walk task panicked: {}", e)))??;

        // Wait for persist task to finish processing remaining batches
        let persist_stats = persist_handle
            .await
            .map_err(|e| ScoutError::Config(format!("Persist task panicked: {}", e)))?;

        // Combine stats
        let mut final_stats = persist_stats;
        final_stats.dirs_scanned = walk_stats.dirs_scanned;
        final_stats.bytes_scanned = walk_stats.bytes_scanned;

        // Send final progress
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ScanProgress {
                dirs_scanned: final_stats.dirs_scanned as usize,
                files_found: final_stats.files_discovered as usize,
                files_persisted: files_persisted_counter.load(Ordering::Relaxed),
                current_dir: None,
            });
        }

        // Mark files not seen in this scan as deleted
        let deleted = self.db.mark_deleted_files(&source.id, scan_start).await?;
        final_stats.files_deleted = deleted;
        final_stats.duration_ms = start.elapsed().as_millis() as u64;
        final_stats.errors += walk_errors.len() as u64;

        // Update denormalized file_count on source for fast TUI queries
        if let Err(e) = self.db.update_source_file_count(&source.id, final_stats.files_discovered as usize).await {
            tracing::warn!(source_id = %source.id, error = %e, "Failed to update source file_count");
        }

        info!(
            source = %source.name,
            discovered = final_stats.files_discovered,
            new = final_stats.files_new,
            changed = final_stats.files_changed,
            deleted = final_stats.files_deleted,
            errors = final_stats.errors,
            duration_ms = final_stats.duration_ms,
            batches = batch_size,
            "Streaming scan complete"
        );

        Ok(ScanResult { stats: final_stats, errors: walk_errors })
    }

    /// Streaming parallel walk - sends batches to channel instead of collecting
    ///
    /// This is the GAP-006 fix: O(batch_size) memory instead of O(file_count).
    /// Walker threads send batches via bounded channel with backpressure.
    fn parallel_walk(
        source_path: &Path,
        source_id: &str,
        batch_tx: mpsc::Sender<Vec<ScannedFile>>,
        batch_size: usize,
        threads: usize,
        include_hidden: bool,
        follow_symlinks: bool,
        progress_interval: usize,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        files_persisted: Arc<AtomicUsize>,
    ) -> Result<(ScanStats, Vec<(String, String)>)> {
        // Shared state for errors only (batches go to channel)
        let all_errors: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));

        // Atomic counters for progress
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_bytes = Arc::new(AtomicUsize::new(0));
        let last_progress_at = Arc::new(AtomicUsize::new(0));
        let current_dir_hint: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

        let walker = WalkBuilder::new(source_path)
            .threads(threads)
            .hidden(!include_hidden)
            .follow_links(follow_symlinks)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .build_parallel();

        let source_id_arc: Arc<str> = Arc::from(source_id);
        let source_path_owned = source_path.to_path_buf();
        let batch_tx = Arc::new(batch_tx);

        walker.run(|| {
            let source_path = source_path_owned.clone();
            let source_id = source_id_arc.clone();
            let all_errors = all_errors.clone();
            let total_files = total_files.clone();
            let total_dirs = total_dirs.clone();
            let total_bytes = total_bytes.clone();
            let last_progress_at = last_progress_at.clone();
            let current_dir_hint = current_dir_hint.clone();
            let progress_tx = progress_tx.clone();
            let batch_tx = batch_tx.clone();
            let files_persisted = files_persisted.clone();

            // Thread-local batch - sent to channel when full
            struct StreamingFlushGuard {
                batch: Vec<ScannedFile>,
                batch_size: usize,
                dir_count: usize,
                byte_count: usize,
                batch_tx: Arc<mpsc::Sender<Vec<ScannedFile>>>,
                total_files: Arc<AtomicUsize>,
                total_dirs: Arc<AtomicUsize>,
                total_bytes: Arc<AtomicUsize>,
            }

            impl Drop for StreamingFlushGuard {
                fn drop(&mut self) {
                    if !self.batch.is_empty() {
                        let batch = std::mem::take(&mut self.batch);
                        let batch_len = batch.len();
                        // Use blocking_send for sync context
                        let _ = self.batch_tx.blocking_send(batch);
                        self.total_files.fetch_add(batch_len, Ordering::Relaxed);
                        self.total_dirs.fetch_add(self.dir_count, Ordering::Relaxed);
                        self.total_bytes.fetch_add(self.byte_count, Ordering::Relaxed);
                    }
                }
            }

            let mut guard = StreamingFlushGuard {
                batch: Vec::with_capacity(batch_size),
                batch_size,
                dir_count: 0,
                byte_count: 0,
                batch_tx: batch_tx.clone(),
                total_files: total_files.clone(),
                total_dirs: total_dirs.clone(),
                total_bytes: total_bytes.clone(),
            };

            Box::new(move |entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        if let Ok(mut errors) = all_errors.lock() {
                            errors.push(("unknown".to_string(), e.to_string()));
                        }
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
                        if let Ok(mut errors) = all_errors.lock() {
                            errors.push((file_path.display().to_string(), e.to_string()));
                        }
                        return ignore::WalkState::Continue;
                    }
                };

                if metadata.is_dir() {
                    guard.dir_count += 1;
                    if guard.dir_count % 100 == 0 {
                        if let Ok(mut hint) = current_dir_hint.try_lock() {
                            *hint = file_path
                                .strip_prefix(&source_path)
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                        }
                    }
                    return ignore::WalkState::Continue;
                }

                if metadata.is_symlink() {
                    return ignore::WalkState::Continue;
                }

                let rel_path = file_path
                    .strip_prefix(&source_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file_path.display().to_string());

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
                guard.byte_count += size as usize;

                // Send batch to channel when full
                if guard.batch.len() >= guard.batch_size {
                    let batch = std::mem::replace(
                        &mut guard.batch,
                        Vec::with_capacity(guard.batch_size)
                    );
                    let batch_len = batch.len();

                    // blocking_send provides backpressure - waits if channel full
                    if guard.batch_tx.blocking_send(batch).is_err() {
                        // Channel closed - persist task exited early
                        return ignore::WalkState::Quit;
                    }

                    let new_total = total_files.fetch_add(batch_len, Ordering::Relaxed) + batch_len;
                    total_dirs.fetch_add(guard.dir_count, Ordering::Relaxed);
                    total_bytes.fetch_add(guard.byte_count, Ordering::Relaxed);
                    guard.dir_count = 0;
                    guard.byte_count = 0;

                    // Send progress update
                    let last = last_progress_at.load(Ordering::Relaxed);
                    if new_total.saturating_sub(last) >= progress_interval {
                        if last_progress_at.compare_exchange(
                            last,
                            new_total,
                            Ordering::Relaxed,
                            Ordering::Relaxed
                        ).is_ok() {
                            if let Some(tx) = &progress_tx {
                                let dir_hint = current_dir_hint.try_lock()
                                    .ok()
                                    .map(|h| h.clone());
                                let _ = tx.try_send(ScanProgress {
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

        let errors = match Arc::try_unwrap(all_errors) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

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
    async fn persist_batch_streaming(
        db: &Database,
        files: Vec<ScannedFile>,
        tag: Option<&str>,
    ) -> Result<ScanStats> {
        let mut stats = ScanStats::default();

        // Persist entire batch in one transaction
        // Note: batch_upsert_files handles per-file errors internally
        match db.batch_upsert_files(&files, tag).await {
            Ok(result) => {
                stats.files_new = result.new;
                stats.files_changed = result.changed;
                stats.files_unchanged = result.unchanged;
                stats.errors = result.errors;
            }
            Err(e) => {
                // Transaction-level failure (couldn't BEGIN or COMMIT)
                stats.errors = files.len() as u64;
                tracing::warn!(error = %e, "Batch transaction failed");
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::types::{FileStatus, SourceType};
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;
    use filetime::{FileTime, set_file_mtime};

    async fn create_test_env() -> (TempDir, Database, Source) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_in_memory().await.unwrap();

        let source = Source {
            id: "test-src".to_string(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().to_string_lossy().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

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

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let (_temp_dir, db, source) = create_test_env().await;
        let scanner = Scanner::new(db);

        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_discovered, 0);
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.errors, 0);
    }

    #[tokio::test]
    async fn test_scan_discovers_files() {
        let (temp_dir, db, source) = create_test_env().await;

        // Create some test files
        create_test_file(temp_dir.path(), "file1.csv", "a,b,c\n1,2,3").unwrap();
        create_test_file(temp_dir.path(), "file2.json", "{}").unwrap();
        create_test_file(temp_dir.path(), "subdir/file3.txt", "hello").unwrap();

        let scanner = Scanner::new(db.clone());
        let result = scanner.scan_source(&source).await.unwrap();

        assert_eq!(result.stats.files_discovered, 3);
        assert_eq!(result.stats.files_new, 3);
        assert_eq!(result.stats.files_changed, 0);
        assert_eq!(result.stats.errors, 0);

        // Verify files are in database
        let pending = db.list_pending_files(&source.id, 10).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn test_scan_detects_changes() {
        let (temp_dir, db, source) = create_test_env().await;

        // Create initial file with explicit old mtime
        let file_path = temp_dir.path().join("data.csv");
        create_test_file(temp_dir.path(), "data.csv", "a,b,c\n1,2,3").unwrap();
        let old_mtime = FileTime::from_unix_time(1000000, 0);
        set_file_mtime(&file_path, old_mtime).unwrap();

        let scanner = Scanner::new(db.clone());

        // First scan
        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_new, 1);

        // Second scan - no changes
        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.files_unchanged, 1);
        assert_eq!(result.stats.files_changed, 0);

        // Modify the file with a newer mtime
        std::fs::write(&file_path, "a,b,c,d\n1,2,3,4").unwrap();
        let new_mtime = FileTime::from_unix_time(2000000, 0);
        set_file_mtime(&file_path, new_mtime).unwrap();

        // Third scan - should detect change
        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_new, 0);
        assert_eq!(result.stats.files_changed, 1);
    }

    #[tokio::test]
    async fn test_scan_detects_deleted_files() {
        let (temp_dir, db, source) = create_test_env().await;

        // Create files
        create_test_file(temp_dir.path(), "keep.csv", "data").unwrap();
        create_test_file(temp_dir.path(), "delete.csv", "data").unwrap();

        let scanner = Scanner::new(db.clone());

        // First scan
        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_new, 2);

        // Delete one file
        std::fs::remove_file(temp_dir.path().join("delete.csv")).unwrap();

        // Wait 2ms to ensure scan_start timestamp is after last_seen_at
        // (mark_deleted_files uses scan_start time comparison)
        std::thread::sleep(std::time::Duration::from_millis(2));

        // Second scan
        let result = scanner.scan_source(&source).await.unwrap();
        assert_eq!(result.stats.files_discovered, 1);
        assert_eq!(result.stats.files_deleted, 1);

        // Verify deleted file is marked in database
        let deleted = db.list_files_by_status(FileStatus::Deleted, 10).await.unwrap();
        assert_eq!(deleted.len(), 1);
        assert!(deleted[0].path.contains("delete.csv"));
    }

    #[tokio::test]
    async fn test_scan_nonexistent_source() {
        let db = Database::open_in_memory().await.unwrap();
        let source = Source {
            id: "missing".to_string(),
            name: "Missing".to_string(),
            source_type: SourceType::Local,
            path: "/nonexistent/path".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };

        let scanner = Scanner::new(db);
        let result = scanner.scan_source(&source).await;
        assert!(result.is_err());
    }

    // ========================================================================
    // Streaming scan tests (GAP-006)
    // ========================================================================

    #[tokio::test]
    async fn test_scan_streaming_discovers_files() {
        let (temp_dir, db, source) = create_test_env().await;

        // Create some test files
        create_test_file(temp_dir.path(), "file1.csv", "a,b,c\n1,2,3").unwrap();
        create_test_file(temp_dir.path(), "file2.json", "{}").unwrap();
        create_test_file(temp_dir.path(), "subdir/file3.txt", "hello").unwrap();

        let scanner = Scanner::new(db.clone());

        // Use streaming scan
        let result = scanner.scan(&source, None, None).await.unwrap();

        assert_eq!(result.stats.files_discovered, 3);
        assert_eq!(result.stats.files_new, 3);
        assert_eq!(result.stats.files_changed, 0);
        assert_eq!(result.stats.errors, 0);

        // Verify files are in database
        let pending = db.list_pending_files(&source.id, 10).await.unwrap();
        assert_eq!(pending.len(), 3);
    }
}

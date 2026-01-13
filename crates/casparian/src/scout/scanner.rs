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
use super::types::{FileStatus, ScanStats, ScannedFile, Source};
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
    /// Number of files found
    pub files_found: usize,
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

    /// Scan a source and update the database using parallel walking
    ///
    /// This is the main entry point for scanning. It:
    /// 1. Walks the source path in parallel using ignore::WalkParallel
    /// 2. Collects files into batches per-thread (lock-free hot path)
    /// 3. Persists files to database after walk completes
    /// 4. Marks deleted files
    ///
    /// For progress updates, use `scan_source_with_progress` instead.
    pub async fn scan_source(&self, source: &Source) -> Result<ScanResult> {
        self.scan_source_with_progress(source, None).await
    }

    /// Scan a source with optional progress updates
    ///
    /// If `progress_tx` is provided, progress updates will be sent during the scan.
    /// This is useful for TUI or other interactive contexts.
    pub async fn scan_source_with_progress(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
    ) -> Result<ScanResult> {
        let start = Instant::now();
        info!(source = %source.name, path = %source.path, "Starting parallel scan");

        let source_path = Path::new(&source.path);
        if !source_path.exists() {
            return Err(ScoutError::FileNotFound(source.path.clone()));
        }

        // Send initial progress so UI shows something immediately
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ScanProgress {
                dirs_scanned: 0,
                files_found: 0,
                current_dir: Some(source.path.clone()),
            });
        }

        // Record scan start time for deleted file detection
        let scan_start = Utc::now();

        // Collect files and stats from parallel walker
        let (files, stats, errors) = self.parallel_walk(source_path, &source.id, progress_tx.clone())?;

        // Send final progress with accurate counts
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ScanProgress {
                dirs_scanned: stats.dirs_scanned as usize,
                files_found: stats.files_discovered as usize,
                current_dir: None, // Scan complete
            });
        }

        // Persist all files to database
        let mut final_stats = self.persist_files(&source.id, files, stats).await?;

        // Mark files not seen in this scan as deleted
        let deleted = self.db.mark_deleted_files(&source.id, scan_start).await?;
        final_stats.files_deleted = deleted;
        final_stats.duration_ms = start.elapsed().as_millis() as u64;

        info!(
            source = %source.name,
            discovered = final_stats.files_discovered,
            new = final_stats.files_new,
            changed = final_stats.files_changed,
            deleted = final_stats.files_deleted,
            errors = final_stats.errors,
            duration_ms = final_stats.duration_ms,
            "Scan complete"
        );

        Ok(ScanResult { stats: final_stats, errors })
    }

    /// Parallel filesystem walk - returns collected files and stats
    ///
    /// Uses ignore::WalkParallel for fast parallel traversal.
    /// Each thread accumulates files locally, then flushes to shared state
    /// only when batch is full - minimizing lock contention.
    fn parallel_walk(
        &self,
        source_path: &Path,
        source_id: &str,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
    ) -> Result<(Vec<ScannedFile>, ScanStats, Vec<(String, String)>)> {
        let batch_size = self.config.batch_size;
        let progress_interval = self.config.progress_interval;

        // Shared state - threads only touch when flushing batches
        let all_batches: Arc<Mutex<Vec<Vec<ScannedFile>>>> = Arc::new(Mutex::new(Vec::new()));
        let all_errors: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));

        // Atomic counters - lock-free progress tracking
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_bytes = Arc::new(AtomicUsize::new(0));
        let last_progress_at = Arc::new(AtomicUsize::new(0));

        // Current directory hint - updated infrequently
        let current_dir_hint: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

        let walker = WalkBuilder::new(source_path)
            .threads(self.config.threads)
            .hidden(!self.config.include_hidden)
            .follow_links(self.config.follow_symlinks)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .build_parallel();

        let source_id_owned = source_id.to_string();
        let source_path_owned = source_path.to_path_buf();

        walker.run(|| {
            // Thread-local state - no lock contention in hot path
            let source_path = source_path_owned.clone();
            let source_id = source_id_owned.clone();
            let all_batches = all_batches.clone();
            let all_errors = all_errors.clone();
            let total_files = total_files.clone();
            let total_dirs = total_dirs.clone();
            let total_bytes = total_bytes.clone();
            let last_progress_at = last_progress_at.clone();
            let current_dir_hint = current_dir_hint.clone();
            let progress_tx = progress_tx.clone();

            // Thread-local batch with flush guard
            struct FlushGuard {
                batch: Vec<ScannedFile>,
                dir_count: usize,
                byte_count: usize,
                all_batches: Arc<Mutex<Vec<Vec<ScannedFile>>>>,
                total_files: Arc<AtomicUsize>,
                total_dirs: Arc<AtomicUsize>,
                total_bytes: Arc<AtomicUsize>,
            }

            impl Drop for FlushGuard {
                fn drop(&mut self) {
                    if !self.batch.is_empty() {
                        let batch = std::mem::take(&mut self.batch);
                        let batch_len = batch.len();
                        if let Ok(mut batches) = self.all_batches.lock() {
                            batches.push(batch);
                        }
                        self.total_files.fetch_add(batch_len, Ordering::Relaxed);
                        self.total_dirs.fetch_add(self.dir_count, Ordering::Relaxed);
                        self.total_bytes.fetch_add(self.byte_count, Ordering::Relaxed);
                    }
                }
            }

            let mut guard = FlushGuard {
                batch: Vec::with_capacity(batch_size),
                dir_count: 0,
                byte_count: 0,
                all_batches: all_batches.clone(),
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

                // Skip root
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

                    // Update current_dir hint infrequently
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

                // Skip symlinks unless configured to follow them
                if metadata.is_symlink() {
                    return ignore::WalkState::Continue;
                }

                // Build ScannedFile
                let rel_path = file_path
                    .strip_prefix(&source_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file_path.display().to_string());

                let size = metadata.len();
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                guard.batch.push(ScannedFile::new(
                    &source_id,
                    &file_path.to_string_lossy(),
                    &rel_path,
                    size,
                    mtime,
                ));
                guard.byte_count += size as usize;

                // Flush batch when full
                if guard.batch.len() >= batch_size {
                    let batch = std::mem::replace(
                        &mut guard.batch,
                        Vec::with_capacity(batch_size)
                    );
                    let batch_len = batch.len();

                    if let Ok(mut batches) = all_batches.lock() {
                        batches.push(batch);
                    }

                    let new_total = total_files.fetch_add(batch_len, Ordering::Relaxed) + batch_len;
                    total_dirs.fetch_add(guard.dir_count, Ordering::Relaxed);
                    total_bytes.fetch_add(guard.byte_count, Ordering::Relaxed);
                    guard.dir_count = 0;
                    guard.byte_count = 0;

                    // Send progress using compare_exchange to avoid duplicates
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
                                    current_dir: dir_hint,
                                });
                            }
                        }
                    }
                }

                ignore::WalkState::Continue
            })
        });

        // Collect all files from batches
        let batches = Arc::try_unwrap(all_batches)
            .expect("BUG: Arc still has references after walker completed")
            .into_inner()
            .unwrap_or_default();

        let total_capacity: usize = batches.iter().map(|b| b.len()).sum();
        let mut files = Vec::with_capacity(total_capacity);
        for batch in batches {
            files.extend(batch);
        }

        // Collect errors
        let errors = match Arc::try_unwrap(all_errors) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        let stats = ScanStats {
            files_discovered: files.len() as u64,
            dirs_scanned: total_dirs.load(Ordering::Relaxed) as u64,
            bytes_scanned: total_bytes.load(Ordering::Relaxed) as u64,
            errors: errors.len() as u64,
            ..Default::default()
        };

        Ok((files, stats, errors))
    }

    /// Persist collected files to database
    async fn persist_files(
        &self,
        source_id: &str,
        files: Vec<ScannedFile>,
        mut stats: ScanStats,
    ) -> Result<ScanStats> {
        for file in files {
            match self.db.upsert_file(&file).await {
                Ok(upsert) => {
                    if upsert.is_new {
                        stats.files_new += 1;
                    } else if upsert.is_changed {
                        stats.files_changed += 1;
                    } else {
                        stats.files_unchanged += 1;
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    info!(
                        file = %file.path,
                        error = %e,
                        "Failed to persist file"
                    );
                }
            }
        }
        // Suppress unused variable warning
        let _ = source_id;
        Ok(stats)
    }

    // Methods below are used in tests and will be used for processing integration

    /// Get pending files for a source
    #[allow(dead_code)]
    pub async fn get_pending_files(&self, source_id: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        self.db.list_pending_files(source_id, limit).await
    }

    /// Mark a file as processing
    #[allow(dead_code)]
    pub async fn mark_processing(&self, file_id: i64) -> Result<()> {
        self.db.update_file_status(file_id, FileStatus::Processing, None).await
    }

    /// Mark a file as processed
    #[allow(dead_code)]
    pub async fn mark_processed(&self, file_id: i64) -> Result<()> {
        self.db.update_file_status(file_id, FileStatus::Processed, None).await
    }

    /// Mark a file as failed
    #[allow(dead_code)]
    pub async fn mark_failed(&self, file_id: i64, error: &str) -> Result<()> {
        self.db.update_file_status(file_id, FileStatus::Failed, Some(error)).await
    }

    /// Skip a file
    #[allow(dead_code)]
    pub async fn skip_file(&self, file_id: i64) -> Result<()> {
        self.db.update_file_status(file_id, FileStatus::Skipped, None).await
    }

    /// Get a file by ID
    #[allow(dead_code)]
    pub async fn get_file(&self, file_id: i64) -> Result<Option<ScannedFile>> {
        self.db.get_file(file_id).await
    }
}

/// Scheduler for periodic scanning
#[allow(dead_code)] // Used in tests and future scheduling
pub struct ScanScheduler {
    db: Database,
    sources: Vec<Source>,
}

#[allow(dead_code)] // Used in tests and future scheduling
impl ScanScheduler {
    /// Create a new scheduler
    pub fn new(db: Database) -> Self {
        Self {
            db,
            sources: Vec::new(),
        }
    }

    /// Add a source to monitor
    pub fn add_source(&mut self, source: Source) {
        self.sources.push(source);
    }

    /// Run a single scan cycle for all sources
    pub async fn scan_all(&self) -> Vec<(String, Result<ScanResult>)> {
        let scanner = Scanner::new(self.db.clone());
        let mut results = Vec::new();

        for source in &self.sources {
            let result = scanner.scan_source(source).await;
            results.push((source.id.clone(), result));
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::types::SourceType;
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
    async fn test_file_status_transitions() {
        let (temp_dir, db, source) = create_test_env().await;

        create_test_file(temp_dir.path(), "test.csv", "data").unwrap();

        let scanner = Scanner::new(db.clone());
        scanner.scan_source(&source).await.unwrap();

        // Get the file
        let pending = db.list_pending_files(&source.id, 1).await.unwrap();
        assert_eq!(pending.len(), 1);
        let file_id = pending[0].id.unwrap();

        // Mark as processing
        scanner.mark_processing(file_id).await.unwrap();
        let file = scanner.get_file(file_id).await.unwrap().unwrap();
        assert_eq!(file.status, FileStatus::Processing);

        // Mark as processed
        scanner.mark_processed(file_id).await.unwrap();
        let file = scanner.get_file(file_id).await.unwrap().unwrap();
        assert_eq!(file.status, FileStatus::Processed);
        assert!(file.processed_at.is_some());
    }

    #[tokio::test]
    async fn test_file_failure_handling() {
        let (temp_dir, db, source) = create_test_env().await;

        create_test_file(temp_dir.path(), "bad.csv", "invalid").unwrap();

        let scanner = Scanner::new(db.clone());
        scanner.scan_source(&source).await.unwrap();

        let pending = db.list_pending_files(&source.id, 1).await.unwrap();
        let file_id = pending[0].id.unwrap();

        // Mark as failed
        scanner.mark_failed(file_id, "Parse error: invalid CSV").await.unwrap();
        let file = scanner.get_file(file_id).await.unwrap().unwrap();
        assert_eq!(file.status, FileStatus::Failed);
        assert_eq!(file.error, Some("Parse error: invalid CSV".to_string()));
    }

    #[tokio::test]
    async fn test_scheduler_scan_all() {
        let (temp_dir, db, source) = create_test_env().await;

        create_test_file(temp_dir.path(), "file1.csv", "data").unwrap();
        create_test_file(temp_dir.path(), "file2.csv", "data").unwrap();

        let mut scheduler = ScanScheduler::new(db);
        scheduler.add_source(source);

        let results = scheduler.scan_all().await;
        assert_eq!(results.len(), 1);

        let (source_id, result) = &results[0];
        assert_eq!(source_id, "test-src");
        assert!(result.is_ok());
        assert_eq!(result.as_ref().unwrap().stats.files_new, 2);
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
}

//! Filesystem scanner with polling-based change detection
//!
//! This module provides the core scanning functionality. It uses polling
//! instead of inotify because inotify doesn't work on network filesystems
//! (SMB, NFS, S3-fuse).
//!
//! # Design
//!
//! - Walk the filesystem using `walkdir` (or `jwalk` for parallel walking)
//! - Compare against SQLite state to detect new/changed/deleted files
//! - Queue pending files for processing

use super::db::Database;
use super::error::{Result, ScoutError};
use super::types::{FileStatus, ProcessedEntry, ScanStats, ScannedFile, Source};
use chrono::Utc;
use std::path::Path;
use std::time::Instant;
use tracing::info;
use walkdir::WalkDir;

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
}

impl Scanner {
    /// Create a new scanner with the given database
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Scan a source and update the database
    ///
    /// This is the main entry point for scanning. It:
    /// 1. Walks the source path
    /// 2. Compares files against the database
    /// 3. Upserts new/changed files
    /// 4. Marks deleted files
    pub async fn scan_source(&self, source: &Source) -> Result<ScanResult> {
        let start = Instant::now();
        info!(source = %source.name, path = %source.path, "Starting scan");

        let mut stats = ScanStats::default();
        let mut errors = Vec::new();

        let source_path = Path::new(&source.path);
        if !source_path.exists() {
            return Err(ScoutError::FileNotFound(source.path.clone()));
        }

        // Record scan start time for deleted file detection
        let scan_start = Utc::now();

        // Walk the filesystem
        for entry in WalkDir::new(source_path)
            .follow_links(false)
            .into_iter()
        {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_dir() {
                        stats.dirs_scanned += 1;
                        continue;
                    }

                    // Skip symlinks for now (could add as config option)
                    if entry.file_type().is_symlink() {
                        continue;
                    }

                    match self.process_entry(&source.id, source_path, &entry).await {
                        Ok(result) => {
                            stats.files_discovered += 1;
                            stats.bytes_scanned += result.size;
                            if result.is_new {
                                stats.files_new += 1;
                            } else if result.is_changed {
                                stats.files_changed += 1;
                            } else {
                                stats.files_unchanged += 1;
                            }
                        }
                        Err(e) => {
                            stats.errors += 1;
                            let path = entry.path().to_string_lossy().to_string();
                            errors.push((path, e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    if let Some(path) = e.path() {
                        errors.push((path.to_string_lossy().to_string(), e.to_string()));
                    } else {
                        errors.push(("unknown".to_string(), e.to_string()));
                    }
                }
            }
        }

        // Mark files not seen in this scan as deleted
        // (Only if they haven't been seen since before the scan started)
        let deleted = self.db.mark_deleted_files(&source.id, scan_start).await?;
        stats.files_deleted = deleted;

        stats.duration_ms = start.elapsed().as_millis() as u64;

        info!(
            source = %source.name,
            discovered = stats.files_discovered,
            new = stats.files_new,
            changed = stats.files_changed,
            deleted = stats.files_deleted,
            errors = stats.errors,
            duration_ms = stats.duration_ms,
            "Scan complete"
        );

        Ok(ScanResult { stats, errors })
    }

    /// Process a single directory entry
    async fn process_entry(
        &self,
        source_id: &str,
        source_path: &Path,
        entry: &walkdir::DirEntry,
    ) -> Result<ProcessedEntry> {
        let path = entry.path();
        let rel_path = path
            .strip_prefix(source_path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let meta = entry.metadata()?;
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let file = ScannedFile::new(
            source_id,
            &path.to_string_lossy(),
            &rel_path,
            size,
            mtime,
        );

        let upsert = self.db.upsert_file(&file).await?;
        Ok(ProcessedEntry {
            is_new: upsert.is_new,
            is_changed: upsert.is_changed,
            size,
        })
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

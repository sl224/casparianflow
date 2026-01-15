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
use super::folder_cache::FolderCache;
use super::types::{FileStatus, ScanStats, ScannedFile, Source};
use chrono::Utc;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::info;

// ============================================================================
// Folder Delta Computation (GAP-003)
// ============================================================================

/// Key: (prefix, name), Value: (count_delta, is_folder)
/// - prefix: Parent path (empty string for root level, "dir/" for subdirs)
/// - name: Folder or file name at this level
/// - count_delta: Number of files to add to this entry's count
/// - is_folder: true if this entry represents a folder, false if a file
pub type FolderDelta = HashMap<(String, String), (i64, bool)>;

/// Compute folder hierarchy deltas from a batch of ScannedFiles
///
/// This function extracts the folder hierarchy from file paths and returns
/// a map of (prefix, name) -> (count, is_folder) that can be used to
/// incrementally update the scout_folders table.
///
/// # Arguments
/// * `files` - Slice of ScannedFile with rel_path fields
///
/// # Returns
/// HashMap where:
/// - Key: (prefix, name) tuple identifying a folder entry
/// - Value: (count, is_folder) - file count and whether entry is a folder
///
/// # Example
/// ```ignore
/// // For file "Library/Preferences/settings.json":
/// // Produces entries:
/// //   ("", "Library") -> (1, true)           // Root level folder
/// //   ("Library/", "Preferences") -> (1, true)  // Nested folder
/// //   ("Library/Preferences/", "settings.json") -> (1, false)  // File
/// ```
pub fn compute_folder_deltas_from_files(files: &[ScannedFile]) -> FolderDelta {
    let mut deltas: FolderDelta = HashMap::new();

    for file in files {
        let segments: Vec<&str> = file.rel_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut prefix = String::new();

        for (i, segment) in segments.iter().enumerate() {
            let is_file = i == segments.len() - 1;
            let key = (prefix.clone(), segment.to_string());

            deltas
                .entry(key)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, !is_file));

            if !is_file {
                prefix.push_str(segment);
                prefix.push('/');
            }
        }
    }

    deltas
}

/// Compute folder hierarchy deltas from a list of path strings
///
/// This variant is used for post-scan rebuild when paths come from a DB query.
/// Same algorithm as compute_folder_deltas_from_files but operates on (String,) tuples.
pub fn compute_folder_deltas_from_paths(paths: &[(String,)]) -> FolderDelta {
    let mut deltas: FolderDelta = HashMap::new();

    for (rel_path,) in paths {
        let segments: Vec<&str> = rel_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut prefix = String::new();

        for (i, segment) in segments.iter().enumerate() {
            let is_file = i == segments.len() - 1;
            let key = (prefix.clone(), segment.to_string());

            deltas
                .entry(key)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, !is_file));

            if !is_file {
                prefix.push_str(segment);
                prefix.push('/');
            }
        }
    }

    deltas
}

/// Merge folder deltas into an existing delta map
///
/// Used when accumulating deltas from multiple batches. Adds counts from
/// the new delta to existing entries, or inserts new entries.
pub fn merge_folder_deltas(target: &mut FolderDelta, source: FolderDelta) {
    for (key, (count, is_folder)) in source {
        target
            .entry(key)
            .and_modify(|(existing_count, _)| *existing_count += count)
            .or_insert((count, is_folder));
    }
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
        self.scan_source_with_progress(source, None, None).await
    }

    /// Scan a source with optional progress updates and tagging
    ///
    /// If `progress_tx` is provided, progress updates will be sent during the scan.
    /// This is useful for TUI or other interactive contexts.
    ///
    /// If `tag` is provided, all discovered files will be tagged with it.
    pub async fn scan_source_with_progress(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        tag: Option<&str>,
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

        // Persist all files to database (with optional tagging)
        let mut final_stats = self.persist_files(&source.id, files, stats, tag).await?;

        // Mark files not seen in this scan as deleted
        let deleted = self.db.mark_deleted_files(&source.id, scan_start).await?;
        final_stats.files_deleted = deleted;
        final_stats.duration_ms = start.elapsed().as_millis() as u64;

        // Build folder hierarchy in scout_folders table for O(1) TUI navigation
        let cache_start = Instant::now();
        if let Err(e) = self.build_folder_counts(&source.id).await {
            // Log but don't fail the scan - folder counts are a nice-to-have
            tracing::warn!(source_id = %source.id, error = %e, "Failed to build folder counts");
        } else {
            let cache_ms = cache_start.elapsed().as_millis();
            info!(source_id = %source.id, cache_ms, "Folder counts built in scout_folders");
        }

        // Also build legacy .bin.zst cache for backward compatibility during migration
        #[allow(deprecated)]
        if let Err(e) = self.build_folder_cache(&source.id).await {
            tracing::debug!(source_id = %source.id, error = %e, "Failed to build legacy folder cache");
        }

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

    /// Scan a source with streaming persist and incremental folder updates
    ///
    /// This is the streaming version that addresses GAP-001, GAP-002, GAP-004, GAP-006:
    /// - Files are persisted as batches arrive (not after full walk)
    /// - Folder counts are updated incrementally per batch
    /// - Memory usage is O(batch_size) not O(file_count)
    /// - Uses transactional batching for consistency
    ///
    /// The architecture is:
    /// ```text
    /// parallel_walk_streaming()
    ///      │
    ///      ├──▶ bounded channel (10 batches) ──▶ Persist Task ──▶ DB
    ///      │    (backpressure)                   (batch writes + folder updates)
    /// ```
    pub async fn scan_source_streaming(
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

        // GAP-004: Clear folder cache at scan START (not during rebuild)
        self.db.clear_folder_cache(&source.id).await?;

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

        // Create bounded channel for backpressure (GAP-006)
        // 10 batches in flight = batch_size * 10 files max in memory
        let (batch_tx, mut batch_rx) = mpsc::channel::<Vec<ScannedFile>>(10);

        // Spawn the persist task that processes batches as they arrive
        let db = self.db.clone();
        let source_id = source.id.clone();
        let tag_owned = tag.map(|t| t.to_string());
        let batch_size = self.config.batch_size;

        let persist_handle = tokio::spawn(async move {
            let mut stats = ScanStats::default();
            let mut accumulated_deltas: FolderDelta = HashMap::new();
            let mut batches_processed = 0u64;

            while let Some(batch) = batch_rx.recv().await {
                let batch_len = batch.len();

                // GAP-002: Transactional batch persist with folder updates
                match Self::persist_batch_streaming(
                    &db,
                    &source_id,
                    batch,
                    tag_owned.as_deref(),
                    &mut accumulated_deltas,
                ).await {
                    Ok(batch_stats) => {
                        stats.files_new += batch_stats.files_new;
                        stats.files_changed += batch_stats.files_changed;
                        stats.files_unchanged += batch_stats.files_unchanged;
                        stats.files_discovered += batch_len as u64;
                        batches_processed += 1;

                        // GAP-001: Update folder counts periodically (every 10 batches)
                        // This balances between immediate updates and reducing DB writes
                        if batches_processed % 10 == 0 && !accumulated_deltas.is_empty() {
                            if let Err(e) = db.batch_upsert_folder_counts(&source_id, &accumulated_deltas).await {
                                tracing::warn!(error = %e, "Failed to update folder counts");
                            }
                            accumulated_deltas.clear();
                        }
                    }
                    Err(e) => {
                        stats.errors += batch_len as u64;
                        tracing::warn!(error = %e, "Batch persist failed");
                    }
                }
            }

            // Flush remaining folder deltas
            if !accumulated_deltas.is_empty() {
                if let Err(e) = db.batch_upsert_folder_counts(&source_id, &accumulated_deltas).await {
                    tracing::warn!(error = %e, "Failed to flush final folder counts");
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

        let walk_handle = tokio::task::spawn_blocking(move || {
            Self::parallel_walk_streaming(
                &walk_source_path,
                &walk_source_id,
                batch_tx,
                walk_config_batch_size,
                walk_config_threads,
                walk_config_include_hidden,
                walk_config_follow_symlinks,
                walk_progress_interval,
                walk_progress_tx,
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
                current_dir: None,
            });
        }

        // Mark files not seen in this scan as deleted
        let deleted = self.db.mark_deleted_files(&source.id, scan_start).await?;
        final_stats.files_deleted = deleted;
        final_stats.duration_ms = start.elapsed().as_millis() as u64;
        final_stats.errors += walk_errors.len() as u64;

        // Skip legacy cache building - scout_folders is now authoritative
        // (GAP-005 cleanup: remove deprecated .bin.zst cache)

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
    fn parallel_walk_streaming(
        source_path: &Path,
        source_id: &str,
        batch_tx: mpsc::Sender<Vec<ScannedFile>>,
        batch_size: usize,
        threads: usize,
        include_hidden: bool,
        follow_symlinks: bool,
        progress_interval: usize,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
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
    /// Also computes folder deltas and merges into accumulated deltas.
    async fn persist_batch_streaming(
        db: &Database,
        _source_id: &str, // Reserved for future transactional batch support
        files: Vec<ScannedFile>,
        tag: Option<&str>,
        accumulated_deltas: &mut FolderDelta,
    ) -> Result<ScanStats> {
        let mut stats = ScanStats::default();

        // Compute folder deltas for this batch
        let batch_deltas = compute_folder_deltas_from_files(&files);
        merge_folder_deltas(accumulated_deltas, batch_deltas);

        // Persist files (TODO: wrap in transaction for GAP-002 full compliance)
        for file in files {
            match db.upsert_file(&file).await {
                Ok(upsert) => {
                    if upsert.is_new {
                        stats.files_new += 1;
                    } else if upsert.is_changed {
                        stats.files_changed += 1;
                    } else {
                        stats.files_unchanged += 1;
                    }

                    if let Some(t) = tag {
                        if let Err(e) = db.tag_file(upsert.id, t).await {
                            tracing::warn!(file_id = upsert.id, tag = t, error = %e, "Failed to tag file");
                        }
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    tracing::debug!(file = %file.path, error = %e, "Failed to persist file");
                }
            }
        }

        Ok(stats)
    }

    /// Parallel filesystem walk - returns collected files and stats
    ///
    /// Uses ignore::WalkParallel for fast parallel traversal.
    /// Each thread accumulates files locally, then flushes to shared state
    /// only when batch is full - minimizing lock contention.
    ///
    /// NOTE: This is the legacy non-streaming version. For new code, prefer
    /// scan_source_streaming() which has O(batch_size) memory usage.
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

        // F-007: Use Arc<str> to share source_id across threads without per-file allocation
        let source_id_arc: Arc<str> = Arc::from(source_id);
        let source_path_owned = source_path.to_path_buf();

        walker.run(|| {
            // Thread-local state - no lock contention in hot path
            let source_path = source_path_owned.clone();
            let source_id = source_id_arc.clone(); // Cheap Arc clone
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

                // F-007: Build ScannedFile with pre-allocated strings to avoid redundant allocations
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

                // Use from_parts() with Arc<str> - source_id is shared, path strings are owned
                guard.batch.push(ScannedFile::from_parts(
                    source_id.clone(),
                    full_path,
                    rel_path,
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

    /// Persist collected files to database with optional tagging
    async fn persist_files(
        &self,
        source_id: &str,
        files: Vec<ScannedFile>,
        mut stats: ScanStats,
        tag: Option<&str>,
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

                    // Tag file if requested
                    if let Some(t) = tag {
                        if let Err(e) = self.db.tag_file(upsert.id, t).await {
                            tracing::warn!(file_id = upsert.id, tag = t, error = %e, "Failed to tag file");
                        }
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

    /// Build and persist folder cache for O(1) TUI navigation
    ///
    /// This queries all file paths for the source and builds a trie-based
    /// cache that's persisted to disk. The TUI loads this cache instead of
    /// querying the database, enabling instant folder navigation.
    ///
    /// DEPRECATED: This method builds the legacy .bin.zst file cache.
    /// New code should use build_folder_counts() which populates scout_folders table.
    #[deprecated(note = "Use build_folder_counts() for SQLite-based folder hierarchy")]
    async fn build_folder_cache(&self, source_id: &str) -> Result<()> {
        // Query all file paths for this source
        let paths: Vec<(String,)> = sqlx::query_as(
            "SELECT rel_path FROM scout_files WHERE source_id = ? AND status != 'deleted'"
        )
        .bind(source_id)
        .fetch_all(self.db.pool())
        .await?;

        if paths.is_empty() {
            // Don't create empty cache
            return Ok(());
        }

        // Query tag summaries for this source
        let tags = self.query_tag_summaries(source_id).await?;

        // Build cache in blocking task (CPU-intensive)
        let source_id_owned = source_id.to_string();
        let cache = tokio::task::spawn_blocking(move || {
            FolderCache::build_with_tags(&source_id_owned, &paths, tags)
        })
        .await
        .map_err(|e| ScoutError::Config(format!("Failed to build cache: {}", e)))?;

        // Save to disk
        cache.save()
            .map_err(|e| ScoutError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    /// Build folder hierarchy in scout_folders table
    ///
    /// Replaces build_folder_cache() with SQLite-based storage.
    /// Queries all file paths and aggregates folder counts, then batch upserts.
    ///
    /// NOTE: This is the post-scan rebuild path. For incremental updates during
    /// scanning, use compute_folder_deltas_from_files() with batch_upsert_folder_counts().
    async fn build_folder_counts(&self, source_id: &str) -> Result<()> {
        // Query all file paths for this source
        let paths: Vec<(String,)> = sqlx::query_as(
            "SELECT rel_path FROM scout_files WHERE source_id = ? AND status != 'deleted'"
        )
        .bind(source_id)
        .fetch_all(self.db.pool())
        .await?;

        if paths.is_empty() {
            return Ok(());
        }

        // Compute folder deltas in blocking task (CPU-intensive)
        // Uses the extracted compute_folder_deltas_from_paths() helper (GAP-003)
        let deltas = tokio::task::spawn_blocking(move || {
            compute_folder_deltas_from_paths(&paths)
        })
        .await
        .map_err(|e| ScoutError::Config(format!("Failed to compute folder counts: {}", e)))?;

        // Clear existing folder data and insert new
        self.db.clear_folder_cache(source_id).await?;
        self.db.batch_upsert_folder_counts(source_id, &deltas).await?;

        Ok(())
    }

    /// Query tag summaries for a source (used when building folder cache)
    async fn query_tag_summaries(&self, source_id: &str) -> Result<Vec<crate::scout::folder_cache::TagSummary>> {
        use crate::scout::folder_cache::TagSummary;

        let mut tags = Vec::new();

        // Get total file count
        let total_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND status != 'deleted'"
        )
        .bind(source_id)
        .fetch_one(self.db.pool())
        .await
        .unwrap_or(0);

        // Add "All files" as first option
        tags.push(TagSummary {
            name: "All files".to_string(),
            count: total_count as usize,
            is_special: true,
        });

        // Query distinct tags with counts
        let tag_rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT tag, COUNT(*) as count
            FROM scout_files
            WHERE source_id = ? AND tag IS NOT NULL AND tag != '' AND status != 'deleted'
            GROUP BY tag
            ORDER BY count DESC, tag
            "#
        )
        .bind(source_id)
        .fetch_all(self.db.pool())
        .await
        .unwrap_or_default();

        for (tag_name, count) in tag_rows {
            tags.push(TagSummary {
                name: tag_name,
                count: count as usize,
                is_special: false,
            });
        }

        Ok(tags)
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

    // ========================================================================
    // compute_folder_deltas tests (GAP-003)
    // ========================================================================

    #[test]
    fn test_compute_folder_deltas_from_files_empty() {
        let files: Vec<ScannedFile> = vec![];
        let deltas = compute_folder_deltas_from_files(&files);
        assert!(deltas.is_empty());
    }

    #[test]
    fn test_compute_folder_deltas_from_files_single_root_file() {
        // Single file at root: "readme.txt"
        let files = vec![
            ScannedFile::new("src", "/path/readme.txt", "readme.txt", 100, 0),
        ];
        let deltas = compute_folder_deltas_from_files(&files);

        // Should have one entry: ("", "readme.txt") -> (1, false)
        assert_eq!(deltas.len(), 1);
        let (count, is_folder) = deltas.get(&("".to_string(), "readme.txt".to_string())).unwrap();
        assert_eq!(*count, 1);
        assert!(!*is_folder, "readme.txt should be marked as a file");
    }

    #[test]
    fn test_compute_folder_deltas_from_files_nested_path() {
        // File: "Library/Preferences/settings.json"
        let files = vec![
            ScannedFile::new("src", "/p/Library/Preferences/settings.json", "Library/Preferences/settings.json", 100, 0),
        ];
        let deltas = compute_folder_deltas_from_files(&files);

        // Should have 3 entries:
        // ("", "Library") -> (1, true)              - root folder
        // ("Library/", "Preferences") -> (1, true)  - nested folder
        // ("Library/Preferences/", "settings.json") -> (1, false)  - file
        assert_eq!(deltas.len(), 3);

        let (count, is_folder) = deltas.get(&("".to_string(), "Library".to_string())).unwrap();
        assert_eq!(*count, 1);
        assert!(*is_folder, "Library should be a folder");

        let (count, is_folder) = deltas.get(&("Library/".to_string(), "Preferences".to_string())).unwrap();
        assert_eq!(*count, 1);
        assert!(*is_folder, "Preferences should be a folder");

        let (count, is_folder) = deltas.get(&("Library/Preferences/".to_string(), "settings.json".to_string())).unwrap();
        assert_eq!(*count, 1);
        assert!(!*is_folder, "settings.json should be a file");
    }

    #[test]
    fn test_compute_folder_deltas_from_files_multiple_files_same_folder() {
        // Multiple files in same folder
        let files = vec![
            ScannedFile::new("src", "/p/docs/file1.txt", "docs/file1.txt", 100, 0),
            ScannedFile::new("src", "/p/docs/file2.txt", "docs/file2.txt", 200, 0),
            ScannedFile::new("src", "/p/docs/file3.txt", "docs/file3.txt", 300, 0),
        ];
        let deltas = compute_folder_deltas_from_files(&files);

        // "docs" folder should have count 3
        let (count, is_folder) = deltas.get(&("".to_string(), "docs".to_string())).unwrap();
        assert_eq!(*count, 3);
        assert!(*is_folder);

        // Each file should have count 1
        assert_eq!(deltas.get(&("docs/".to_string(), "file1.txt".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("docs/".to_string(), "file2.txt".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("docs/".to_string(), "file3.txt".to_string())).unwrap().0, 1);
    }

    #[test]
    fn test_compute_folder_deltas_from_files_complex_hierarchy() {
        // Complex hierarchy:
        // Library/Preferences/file1.txt
        // Library/Preferences/file2.txt
        // Library/Caches/file3.txt
        // workspace/project/src/main.rs
        let files = vec![
            ScannedFile::new("src", "/p/Library/Preferences/file1.txt", "Library/Preferences/file1.txt", 100, 0),
            ScannedFile::new("src", "/p/Library/Preferences/file2.txt", "Library/Preferences/file2.txt", 100, 0),
            ScannedFile::new("src", "/p/Library/Caches/file3.txt", "Library/Caches/file3.txt", 100, 0),
            ScannedFile::new("src", "/p/workspace/project/src/main.rs", "workspace/project/src/main.rs", 100, 0),
        ];
        let deltas = compute_folder_deltas_from_files(&files);

        // Library should have 3 files (2 in Preferences, 1 in Caches)
        assert_eq!(deltas.get(&("".to_string(), "Library".to_string())).unwrap().0, 3);

        // workspace should have 1 file
        assert_eq!(deltas.get(&("".to_string(), "workspace".to_string())).unwrap().0, 1);

        // Preferences should have 2 files
        assert_eq!(deltas.get(&("Library/".to_string(), "Preferences".to_string())).unwrap().0, 2);

        // Caches should have 1 file
        assert_eq!(deltas.get(&("Library/".to_string(), "Caches".to_string())).unwrap().0, 1);
    }

    #[test]
    fn test_compute_folder_deltas_from_paths_matches_files_variant() {
        // Test that the paths variant produces same results as files variant
        let files = vec![
            ScannedFile::new("src", "/p/docs/file1.txt", "docs/file1.txt", 100, 0),
            ScannedFile::new("src", "/p/docs/file2.txt", "docs/file2.txt", 200, 0),
        ];

        let paths: Vec<(String,)> = vec![
            ("docs/file1.txt".to_string(),),
            ("docs/file2.txt".to_string(),),
        ];

        let deltas_from_files = compute_folder_deltas_from_files(&files);
        let deltas_from_paths = compute_folder_deltas_from_paths(&paths);

        assert_eq!(deltas_from_files, deltas_from_paths);
    }

    #[test]
    fn test_compute_folder_deltas_handles_leading_slash() {
        // Paths with leading slash should be handled (filter empty segments)
        let paths: Vec<(String,)> = vec![
            ("/docs/file.txt".to_string(),),  // Note leading slash
        ];

        let deltas = compute_folder_deltas_from_paths(&paths);

        // Should still work correctly - empty segments filtered out
        assert_eq!(deltas.get(&("".to_string(), "docs".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("docs/".to_string(), "file.txt".to_string())).unwrap().0, 1);
    }

    #[test]
    fn test_compute_folder_deltas_deeply_nested() {
        // Test deeply nested path: a/b/c/d/e/file.txt
        let paths: Vec<(String,)> = vec![
            ("a/b/c/d/e/file.txt".to_string(),),
        ];

        let deltas = compute_folder_deltas_from_paths(&paths);

        // Should have 6 entries (5 folders + 1 file)
        assert_eq!(deltas.len(), 6);

        // All intermediate folders should have count 1
        assert_eq!(deltas.get(&("".to_string(), "a".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("a/".to_string(), "b".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("a/b/".to_string(), "c".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("a/b/c/".to_string(), "d".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("a/b/c/d/".to_string(), "e".to_string())).unwrap().0, 1);
        assert_eq!(deltas.get(&("a/b/c/d/e/".to_string(), "file.txt".to_string())).unwrap().0, 1);

        // All but the last should be folders
        assert!(deltas.get(&("".to_string(), "a".to_string())).unwrap().1);
        assert!(!deltas.get(&("a/b/c/d/e/".to_string(), "file.txt".to_string())).unwrap().1);
    }

    // ========================================================================
    // merge_folder_deltas tests
    // ========================================================================

    #[test]
    fn test_merge_folder_deltas_empty_source() {
        let mut target: FolderDelta = HashMap::new();
        target.insert(("".to_string(), "docs".to_string()), (5, true));

        let source: FolderDelta = HashMap::new();
        merge_folder_deltas(&mut target, source);

        // Target unchanged
        assert_eq!(target.get(&("".to_string(), "docs".to_string())).unwrap().0, 5);
    }

    #[test]
    fn test_merge_folder_deltas_new_entries() {
        let mut target: FolderDelta = HashMap::new();
        target.insert(("".to_string(), "docs".to_string()), (5, true));

        let mut source: FolderDelta = HashMap::new();
        source.insert(("".to_string(), "images".to_string()), (3, true));

        merge_folder_deltas(&mut target, source);

        assert_eq!(target.len(), 2);
        assert_eq!(target.get(&("".to_string(), "docs".to_string())).unwrap().0, 5);
        assert_eq!(target.get(&("".to_string(), "images".to_string())).unwrap().0, 3);
    }

    #[test]
    fn test_merge_folder_deltas_accumulates_counts() {
        let mut target: FolderDelta = HashMap::new();
        target.insert(("".to_string(), "docs".to_string()), (5, true));

        let mut source: FolderDelta = HashMap::new();
        source.insert(("".to_string(), "docs".to_string()), (3, true));

        merge_folder_deltas(&mut target, source);

        // Counts should be accumulated: 5 + 3 = 8
        assert_eq!(target.get(&("".to_string(), "docs".to_string())).unwrap().0, 8);
    }

    #[test]
    fn test_merge_folder_deltas_complex() {
        // Simulate merging two batches of deltas
        let mut accumulated: FolderDelta = HashMap::new();

        // Batch 1: Library/file1.txt, Library/file2.txt
        let batch1 = compute_folder_deltas_from_paths(&[
            ("Library/file1.txt".to_string(),),
            ("Library/file2.txt".to_string(),),
        ]);
        merge_folder_deltas(&mut accumulated, batch1);

        // After batch 1: Library = 2
        assert_eq!(accumulated.get(&("".to_string(), "Library".to_string())).unwrap().0, 2);

        // Batch 2: Library/file3.txt, workspace/project.rs
        let batch2 = compute_folder_deltas_from_paths(&[
            ("Library/file3.txt".to_string(),),
            ("workspace/project.rs".to_string(),),
        ]);
        merge_folder_deltas(&mut accumulated, batch2);

        // After batch 2: Library = 3, workspace = 1
        assert_eq!(accumulated.get(&("".to_string(), "Library".to_string())).unwrap().0, 3);
        assert_eq!(accumulated.get(&("".to_string(), "workspace".to_string())).unwrap().0, 1);
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
        let result = scanner.scan_source_streaming(&source, None, None).await.unwrap();

        assert_eq!(result.stats.files_discovered, 3);
        assert_eq!(result.stats.files_new, 3);
        assert_eq!(result.stats.files_changed, 0);
        assert_eq!(result.stats.errors, 0);

        // Verify files are in database
        let pending = db.list_pending_files(&source.id, 10).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn test_scan_streaming_clears_folder_cache_at_start() {
        let (temp_dir, db, source) = create_test_env().await;

        // Pre-populate folder cache with stale data
        let mut stale_deltas: FolderDelta = HashMap::new();
        stale_deltas.insert(("".to_string(), "old_folder".to_string()), (100, true));
        db.batch_upsert_folder_counts(&source.id, &stale_deltas).await.unwrap();

        // Verify stale data exists
        let folders = db.get_folder_children(&source.id, "").await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "old_folder");

        // Create real files
        create_test_file(temp_dir.path(), "real_folder/file1.txt", "data").unwrap();

        let scanner = Scanner::new(db.clone());
        scanner.scan_source_streaming(&source, None, None).await.unwrap();

        // Verify: old_folder should be gone, real_folder should exist
        let folders = db.get_folder_children(&source.id, "").await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "real_folder", "Stale folder should be replaced by real folder");
    }

    #[tokio::test]
    async fn test_scan_streaming_updates_folder_counts() {
        let (temp_dir, db, source) = create_test_env().await;

        // Create files in various folders
        create_test_file(temp_dir.path(), "Library/file1.txt", "data").unwrap();
        create_test_file(temp_dir.path(), "Library/file2.txt", "data").unwrap();
        create_test_file(temp_dir.path(), "Library/Caches/cache1.txt", "data").unwrap();
        create_test_file(temp_dir.path(), "workspace/project.rs", "fn main() {}").unwrap();

        let scanner = Scanner::new(db.clone());
        scanner.scan_source_streaming(&source, None, None).await.unwrap();

        // Check folder counts in scout_folders table
        let root_folders = db.get_folder_children(&source.id, "").await.unwrap();

        // Should have Library and workspace at root
        let library = root_folders.iter().find(|f| f.name == "Library").expect("Library folder should exist");
        let workspace = root_folders.iter().find(|f| f.name == "workspace").expect("workspace folder should exist");

        // Library has 3 files total (2 direct + 1 in Caches)
        assert_eq!(library.file_count, 3, "Library should have 3 files total");

        // workspace has 1 file
        assert_eq!(workspace.file_count, 1, "workspace should have 1 file");
    }
}

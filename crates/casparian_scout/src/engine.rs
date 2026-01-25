use crate::db::Database;
use crate::error::{Result, ScoutError};
use crate::scanner::{ScanProgress, ScanResult, Scanner};
use crate::types::{ScanStats, ScannedFile, Source};
use crate::wire::{read_frame, ScanErrorWire, ScanStatsWire, ScannedFileWire, WireMessage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Pluggable scan engine for in-process or subprocess scanning.
pub trait ScanEngine {
    fn scan(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        tag: Option<&str>,
    ) -> Result<ScanResult>;
}

pub struct InProcessEngine {
    scanner: Scanner,
}

impl InProcessEngine {
    pub fn new(db: Database, config: crate::scanner::ScanConfig) -> Self {
        Self {
            scanner: Scanner::with_config(db, config),
        }
    }
}

impl ScanEngine for InProcessEngine {
    fn scan(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        tag: Option<&str>,
    ) -> Result<ScanResult> {
        self.scanner.scan(source, progress_tx, tag)
    }
}

pub struct SubprocessEngine {
    db: Database,
    config: crate::scanner::ScanConfig,
    binary: PathBuf,
}

impl SubprocessEngine {
    pub fn new(db: Database, config: crate::scanner::ScanConfig) -> Self {
        let binary = std::env::var("CASPARIAN_SCOUT_SCAN_BIN")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                std::env::current_exe().ok().and_then(|exe| {
                    exe.parent()
                        .map(|dir| dir.join("casparian-scout-scan"))
                        .filter(|candidate| candidate.exists())
                })
            })
            .unwrap_or_else(|| PathBuf::from("casparian-scout-scan"));
        Self { db, config, binary }
    }

    pub fn with_binary(db: Database, config: crate::scanner::ScanConfig, binary: PathBuf) -> Self {
        Self { db, config, binary }
    }
}

impl ScanEngine for SubprocessEngine {
    fn scan(
        &self,
        source: &Source,
        progress_tx: Option<mpsc::Sender<ScanProgress>>,
        tag: Option<&str>,
    ) -> Result<ScanResult> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("--path")
            .arg(&source.path)
            .arg("--threads")
            .arg(self.config.threads.to_string())
            .arg("--batch-size")
            .arg(self.config.batch_size.to_string())
            .arg("--include-hidden")
            .arg(self.config.include_hidden.to_string())
            .arg("--follow-symlinks")
            .arg(self.config.follow_symlinks.to_string())
            .arg("--source-type-json")
            .arg(serde_json::to_string(&source.source_type)?);

        for dir in &self.config.exclude_dir_names {
            cmd.arg("--exclude-dir").arg(dir);
        }
        for pattern in &self.config.exclude_path_patterns {
            cmd.arg("--exclude-path").arg(pattern);
        }

        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    ScoutError::Config(format!(
                        "Scan subprocess '{}' not found. Build it with `cargo build -p casparian_scout --bin casparian-scout-scan` or set CASPARIAN_SCOUT_SCAN_BIN.",
                        self.binary.display()
                    ))
                } else {
                    ScoutError::InvalidState(format!("Failed to spawn scan subprocess: {e}"))
                }
            })?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ScoutError::InvalidState("Missing subprocess stdout".to_string()))?;

        let scan_start = chrono::Utc::now();
        let start = Instant::now();
        let mut persist_stats = ScanStats::default();
        let mut errors: Vec<crate::scanner::ScanError> = Vec::new();
        let mut folder_cache = FolderCacheAggregator::default();
        let mut done_stats: Option<ScanStatsWire> = None;
        let mut scan_ok = true;
        let mut last_progress_emit = Instant::now();

        while let Some(msg) = read_frame(&mut stdout)? {
            match msg {
                WireMessage::Batch(batch) => {
                    let batch_len = batch.len();
                    if batch_len == 0 {
                        continue;
                    }
                    persist_stats.files_discovered += batch_len as u64;
                    let scanned_files = batch
                        .into_iter()
                        .map(|wire| wire_to_scanned_file(&source, wire))
                        .collect::<Vec<_>>();

                    match self
                        .db
                        .batch_upsert_files(&scanned_files, tag, self.config.compute_stats)
                    {
                        Ok(batch_stats) => {
                            persist_stats.files_new += batch_stats.new;
                            persist_stats.files_changed += batch_stats.changed;
                            persist_stats.files_unchanged += batch_stats.unchanged;
                            persist_stats.files_persisted += batch_len as u64;
                            folder_cache.update_batch(&scanned_files);
                        }
                        Err(err) => {
                            scan_ok = false;
                            persist_stats.errors += batch_len as u64;
                            errors.push(crate::scanner::ScanError {
                                path: source.path.clone(),
                                message: err.to_string(),
                            });
                        }
                    }

                    if let Some(tx) = progress_tx.as_ref() {
                        if last_progress_emit.elapsed() >= Duration::from_secs(1) {
                            last_progress_emit = Instant::now();
                            let elapsed_ms = start.elapsed().as_millis() as u64;
                            let files_per_sec = if elapsed_ms == 0 {
                                0.0
                            } else {
                                persist_stats.files_persisted as f64 / (elapsed_ms as f64 / 1000.0)
                            };
                            let _ = tx.send(ScanProgress {
                                dirs_scanned: done_stats
                                    .as_ref()
                                    .map(|stats| stats.dirs_scanned as usize)
                                    .unwrap_or(0),
                                files_found: persist_stats.files_discovered as usize,
                                files_persisted: persist_stats.files_persisted as usize,
                                current_dir: None,
                                elapsed_ms,
                                files_per_sec,
                                stalled: false,
                            });
                        }
                    }
                }
                WireMessage::Error(err) => {
                    errors.push(scan_error_from_wire(err));
                }
                WireMessage::Done(stats) => {
                    done_stats = Some(stats);
                }
                WireMessage::Progress(_) => {}
            }
        }

        let status = child
            .wait()
            .map_err(|e| ScoutError::InvalidState(format!("Failed to wait for subprocess: {e}")))?;
        if !status.success() {
            return Err(ScoutError::InvalidState(format!(
                "Scan subprocess exited with status {status}"
            )));
        }

        let done_stats = done_stats.unwrap_or(ScanStatsWire {
            dirs_scanned: 0,
            files_discovered: persist_stats.files_discovered,
            bytes_scanned: 0,
            errors: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });

        persist_stats.dirs_scanned = done_stats.dirs_scanned;
        persist_stats.bytes_scanned = done_stats.bytes_scanned;
        persist_stats.duration_ms = start.elapsed().as_millis() as u64;
        persist_stats.errors += done_stats.errors;
        if done_stats.files_discovered > 0 {
            persist_stats.files_discovered = done_stats.files_discovered;
        }

        if scan_ok {
            let deleted = self.db.mark_deleted_files(&source.id, scan_start)?;
            persist_stats.files_deleted = deleted;
        }

        if let Err(e) = self
            .db
            .update_source_file_count(&source.id, persist_stats.files_persisted as usize)
        {
            tracing::warn!(source_id = %source.id, error = %e, "Failed to update source file_count");
        }

        if scan_ok {
            if let Err(e) = self.db.populate_folder_cache_from_aggregates(
                &source.id,
                &folder_cache.root_folder_counts,
                &folder_cache.root_file_names,
            ) {
                tracing::warn!(
                    source_id = %source.id,
                    error = %e,
                    "Failed to populate folder cache from aggregates"
                );
            }
        } else if let Err(e) = self.db.populate_folder_cache(&source.id) {
            tracing::warn!(source_id = %source.id, error = %e, "Failed to populate folder cache");
        }

        Ok(ScanResult {
            stats: persist_stats,
            errors,
        })
    }
}

fn wire_to_scanned_file(source: &Source, wire: ScannedFileWire) -> ScannedFile {
    let rel_trim = wire.rel_path.trim_start_matches('/');
    let full_path = Path::new(&source.path).join(rel_trim);
    let full_path = full_path.to_string_lossy().to_string();
    ScannedFile::from_parts(
        source.workspace_id,
        source.id,
        wire.file_uid,
        full_path,
        wire.rel_path,
        wire.size,
        wire.mtime,
    )
}

fn scan_error_from_wire(err: ScanErrorWire) -> crate::scanner::ScanError {
    crate::scanner::ScanError {
        path: err.path,
        message: err.message,
    }
}

#[derive(Default)]
struct FolderCacheAggregator {
    root_folder_counts: HashMap<String, u64>,
    root_file_names: Vec<String>,
}

impl FolderCacheAggregator {
    fn update_batch(&mut self, batch: &[ScannedFile]) {
        for file in batch {
            if file.parent_path.is_empty() {
                self.root_file_names.push(file.name.clone());
                continue;
            }

            if let Some(root) = file.parent_path.split('/').next() {
                if !root.is_empty() {
                    *self.root_folder_counts.entry(root.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}

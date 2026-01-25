use casparian_scout::file_uid::compute_file_uid;
use casparian_scout::scanner::ScanConfig;
use casparian_scout::types::SourceType;
use casparian_scout::wire::{write_frame, ScanErrorWire, ScanStatsWire, ScannedFileWire, WireMessage};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;

struct Args {
    path: PathBuf,
    config: ScanConfig,
    source_type: SourceType,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    run_scan(args)
}

fn parse_args() -> anyhow::Result<Args> {
    let mut config = ScanConfig::default();
    let mut path: Option<PathBuf> = None;
    let mut source_type_json: Option<String> = None;
    let mut exclude_dir_names: Vec<String> = Vec::new();
    let mut exclude_path_patterns: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => {
                path = Some(PathBuf::from(next_arg(&mut args, "--path")?));
            }
            "--threads" => {
                config.threads = next_arg(&mut args, "--threads")?.parse()?;
            }
            "--batch-size" => {
                config.batch_size = next_arg(&mut args, "--batch-size")?.parse()?;
            }
            "--include-hidden" => {
                config.include_hidden = parse_bool(&next_arg(&mut args, "--include-hidden")?)?;
            }
            "--follow-symlinks" => {
                config.follow_symlinks = parse_bool(&next_arg(&mut args, "--follow-symlinks")?)?;
            }
            "--source-type-json" => {
                source_type_json = Some(next_arg(&mut args, "--source-type-json")?);
            }
            "--exclude-dir" => {
                exclude_dir_names.push(next_arg(&mut args, "--exclude-dir")?);
            }
            "--exclude-path" => {
                exclude_path_patterns.push(next_arg(&mut args, "--exclude-path")?);
            }
            other => {
                return Err(anyhow::anyhow!("Unknown arg: {}", other));
            }
        }
    }

    let path = path.ok_or_else(|| anyhow::anyhow!("Missing --path"))?;
    let source_type = if let Some(json) = source_type_json {
        serde_json::from_str(&json)?
    } else {
        SourceType::Local
    };

    if exclude_dir_names.is_empty() {
        exclude_dir_names = config.exclude_dir_names.clone();
    }
    if exclude_path_patterns.is_empty() {
        exclude_path_patterns = config.exclude_path_patterns.clone();
    }
    config.exclude_dir_names = exclude_dir_names;
    config.exclude_path_patterns = exclude_path_patterns;

    Ok(Args {
        path,
        config,
        source_type,
    })
}

fn next_arg(args: &mut impl Iterator<Item = String>, name: &str) -> anyhow::Result<String> {
    args.next()
        .ok_or_else(|| anyhow::anyhow!("Missing value for {}", name))
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(anyhow::anyhow!("Invalid bool: {}", value)),
    }
}

fn run_scan(args: Args) -> anyhow::Result<()> {
    let start = Instant::now();
    let (tx, rx) = mpsc::sync_channel::<WireMessage>(args.config.batch_size.max(1));

    let writer = std::thread::spawn(move || -> anyhow::Result<()> {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        while let Ok(msg) = rx.recv() {
            write_frame(&mut handle, &msg)?;
        }
        Ok(())
    });

    let total_files = Arc::new(AtomicUsize::new(0));
    let total_dirs = Arc::new(AtomicUsize::new(0));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let total_errors = Arc::new(AtomicUsize::new(0));

    let source_path = args.path.clone();
    let source_type = args.source_type.clone();
    let batch_size = args.config.batch_size.max(1);

    let exclude_dir_names: Arc<[String]> = Arc::from(args.config.exclude_dir_names);
    let exclude_path_patterns: Arc<[String]> = Arc::from(args.config.exclude_path_patterns);

    let walker = WalkBuilder::new(&source_path)
        .threads(args.config.threads)
        .hidden(!args.config.include_hidden)
        .follow_links(args.config.follow_symlinks)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(move |entry| {
            if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                return true;
            }

            let path = entry.path();
            let path_str = path.to_string_lossy();

            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                for exclude_name in exclude_dir_names.iter() {
                    if name_str == *exclude_name {
                        return false;
                    }
                }
            }

            for pattern in exclude_path_patterns.iter() {
                if path_str.contains(pattern.as_str()) {
                    return false;
                }
            }

            true
        })
        .build_parallel();

    let source_path_owned = source_path.clone();
    walker.run(|| {
        let source_path = source_path_owned.clone();
        let source_type = source_type.clone();
        let batch_tx = tx.clone();
        let total_files = total_files.clone();
        let total_dirs = total_dirs.clone();
        let total_bytes = total_bytes.clone();
        let total_errors = total_errors.clone();

        struct StreamingFlushGuard {
            batch: Vec<ScannedFileWire>,
            batch_size: usize,
            byte_count: u64,
            batch_tx: mpsc::SyncSender<WireMessage>,
            total_files: Arc<AtomicUsize>,
            total_bytes: Arc<AtomicU64>,
        }

        impl Drop for StreamingFlushGuard {
            fn drop(&mut self) {
                if !self.batch.is_empty() {
                    let batch = std::mem::take(&mut self.batch);
                    let batch_len = batch.len();
                    let _ = self.batch_tx.send(WireMessage::Batch(batch));
                    self.total_files.fetch_add(batch_len, Ordering::Relaxed);
                }
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

        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    let _ = batch_tx.send(WireMessage::Error(ScanErrorWire {
                        path: "unknown".to_string(),
                        message: e.to_string(),
                    }));
                    total_errors.fetch_add(1, Ordering::Relaxed);
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
                    let _ = batch_tx.send(WireMessage::Error(ScanErrorWire {
                        path: file_path.display().to_string(),
                        message: e.to_string(),
                    }));
                    total_errors.fetch_add(1, Ordering::Relaxed);
                    return ignore::WalkState::Continue;
                }
            };

            if metadata.is_dir() {
                total_dirs.fetch_add(1, Ordering::Relaxed);
                return ignore::WalkState::Continue;
            }

            if entry.file_type().map_or(false, |ft| ft.is_symlink()) {
                return ignore::WalkState::Continue;
            }

            let rel_path = file_path
                .strip_prefix(&source_path)
                .map(|p| normalize_path_to_forward_slashes(p))
                .unwrap_or_else(|_| normalize_path_to_forward_slashes(file_path));

            let size = metadata.len();
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let uid = compute_file_uid(&source_type, file_path, &metadata);
            guard.batch.push(ScannedFileWire {
                rel_path,
                file_uid: uid.value,
                size,
                mtime,
            });
            guard.byte_count += size;

            if guard.batch.len() >= guard.batch_size {
                let batch = std::mem::replace(
                    &mut guard.batch,
                    Vec::with_capacity(guard.batch_size),
                );
                let batch_bytes = guard.byte_count;
                guard.byte_count = 0;
                let batch_len = batch.len();
                let _ = guard.batch_tx.send(WireMessage::Batch(batch));
                guard.total_files.fetch_add(batch_len, Ordering::Relaxed);
                if batch_bytes > 0 {
                    guard.total_bytes.fetch_add(batch_bytes, Ordering::Relaxed);
                }
            }

            ignore::WalkState::Continue
        })
    });

    let stats = ScanStatsWire {
        dirs_scanned: total_dirs.load(Ordering::Relaxed) as u64,
        files_discovered: total_files.load(Ordering::Relaxed) as u64,
        bytes_scanned: total_bytes.load(Ordering::Relaxed),
        errors: total_errors.load(Ordering::Relaxed) as u64,
        duration_ms: start.elapsed().as_millis() as u64,
    };
    tx.send(WireMessage::Done(stats))?;
    drop(tx);

    let _ = writer.join();
    Ok(())
}

fn normalize_path_to_forward_slashes(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if cfg!(windows) {
        path_str.replace('\\', "/")
    } else {
        path_str.into_owned()
    }
}

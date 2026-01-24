use casparian::scout::{
    Database, ScanConfig, ScannedFile, Scanner, Source, SourceId, SourceType, WorkspaceId,
};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use ignore::WalkBuilder;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const FILE_COUNT: usize = 5_000;
const FILE_SIZE_BYTES: usize = 256;
const DEPTH: usize = 3;
const BATCH_SIZES: &[usize] = &[512, 2_048, 10_000];
const WRITE_BATCH_SIZES: &[usize] = &[256, 1_024, 4_096, 10_000];

struct Fixture {
    temp_dir: TempDir,
    file_count: usize,
}

fn build_path(root: &Path, index: usize, depth: usize) -> PathBuf {
    let mut path = root.to_path_buf();
    for level in 0..depth {
        path.push(format!("level{}_{}", level, index % 16));
    }
    path.push(format!("file_{:06}.dat", index));
    path
}

fn create_fixture(file_count: usize, depth: usize, file_size: usize) -> Fixture {
    let temp_dir = TempDir::new().expect("create temp dir");
    let payload = vec![b'x'; file_size];

    for i in 0..file_count {
        let path = build_path(temp_dir.path(), i, depth);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture dirs");
        }
        let mut file = File::create(path).expect("create fixture file");
        file.write_all(&payload).expect("write fixture file");
    }

    Fixture {
        temp_dir,
        file_count,
    }
}

fn create_source(workspace_id: WorkspaceId, path: &Path) -> Source {
    Source {
        workspace_id,
        id: SourceId::new(),
        name: "Bench Source".to_string(),
        source_type: SourceType::Local,
        path: path.to_string_lossy().to_string(),
        poll_interval_secs: 0,
        enabled: true,
    }
}

fn walk_count_bytes(root: &Path, config: &ScanConfig) -> (usize, u64) {
    let total_files = Arc::new(AtomicUsize::new(0));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let exclude_dir_names: Arc<[String]> = Arc::from(config.exclude_dir_names.clone());
    let exclude_path_patterns: Arc<[String]> = Arc::from(config.exclude_path_patterns.clone());

    let walker = WalkBuilder::new(root)
        .threads(config.threads)
        .hidden(!config.include_hidden)
        .follow_links(config.follow_symlinks)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(move |entry| {
            if !entry.file_type().is_some_and(|ft| ft.is_dir()) {
                return true;
            }

            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                for exclude_name in exclude_dir_names.iter() {
                    if name_str == *exclude_name {
                        return false;
                    }
                }
            }

            let path_str = path.to_string_lossy();
            for pattern in exclude_path_patterns.iter() {
                if path_str.contains(pattern.as_str()) {
                    return false;
                }
            }

            true
        })
        .build_parallel();

    let root_owned = root.to_path_buf();
    walker.run(|| {
        let total_files = total_files.clone();
        let total_bytes = total_bytes.clone();
        let errors = errors.clone();
        let root = root_owned.clone();

        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    let mut guard = errors.lock().unwrap();
                    guard.push(e.to_string());
                    return ignore::WalkState::Continue;
                }
            };

            let path = entry.path();
            if path == root {
                return ignore::WalkState::Continue;
            }

            if entry.file_type().is_some_and(|ft| ft.is_symlink()) {
                return ignore::WalkState::Continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    let mut guard = errors.lock().unwrap();
                    guard.push(e.to_string());
                    return ignore::WalkState::Continue;
                }
            };

            if metadata.is_file() {
                total_files.fetch_add(1, Ordering::Relaxed);
                total_bytes.fetch_add(metadata.len(), Ordering::Relaxed);
            }

            ignore::WalkState::Continue
        })
    });

    (
        total_files.load(Ordering::Relaxed),
        total_bytes.load(Ordering::Relaxed),
    )
}

fn build_scanned_files(
    workspace_id: WorkspaceId,
    source_id: &SourceId,
    count: usize,
) -> Vec<ScannedFile> {
    let mut files = Vec::with_capacity(count);
    for i in 0..count {
        let rel_path = format!("level_{}/file_{:06}.dat", i % 16, i);
        let full_path = format!("/bench/{}", rel_path);
        files.push(ScannedFile::new(
            workspace_id,
            *source_id,
            &full_path,
            &rel_path,
            FILE_SIZE_BYTES as u64,
            0,
        ));
    }
    files
}

fn bench_scanner_full_scan(c: &mut Criterion) {
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);

    let mut group = c.benchmark_group("scanner_full_scan");
    group.throughput(Throughput::Elements(fixture.file_count as u64));

    for &batch_size in BATCH_SIZES {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &batch| {
                b.iter_batched(
                    || {
                        let db = Database::open_in_memory().expect("open db");
                        let workspace_id = db.ensure_default_workspace().expect("workspace").id;
                        let source = create_source(workspace_id, fixture.temp_dir.path());
                        db.upsert_source(&source).expect("insert source");
                        (db, source)
                    },
                    |(db, source)| {
                        let config = ScanConfig {
                            batch_size: batch,
                            ..Default::default()
                        };
                        let scanner = Scanner::with_config(db, config);
                        scanner.scan_source(&source).expect("scan");
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_scanner_rescan(c: &mut Criterion) {
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);

    let mut group = c.benchmark_group("scanner_rescan");
    group.throughput(Throughput::Elements(fixture.file_count as u64));

    for &batch_size in BATCH_SIZES {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &batch| {
                b.iter_batched(
                    || {
                        let db = Database::open_in_memory().expect("open db");
                        let workspace_id = db.ensure_default_workspace().expect("workspace").id;
                        let source = create_source(workspace_id, fixture.temp_dir.path());
                        db.upsert_source(&source).expect("insert source");
                        let config = ScanConfig {
                            batch_size: batch,
                            ..Default::default()
                        };
                        let scanner = Scanner::with_config(db.clone(), config);
                        scanner.scan_source(&source).expect("warm scan");
                        (db, source)
                    },
                    |(db, source)| {
                        let config = ScanConfig {
                            batch_size: batch,
                            ..Default::default()
                        };
                        let scanner = Scanner::with_config(db, config);
                        scanner.scan_source(&source).expect("rescan");
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_scanner_walk_only(c: &mut Criterion) {
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);
    let config = ScanConfig::default();

    let mut group = c.benchmark_group("scanner_walk_only");
    group.throughput(Throughput::Elements(fixture.file_count as u64));

    group.bench_function("walk_parallel", |b| {
        b.iter(|| {
            let (files, bytes) = walk_count_bytes(fixture.temp_dir.path(), &config);
            assert!(files > 0, "walk should find files");
            assert!(bytes > 0, "walk should find bytes");
        });
    });

    group.finish();
}

fn bench_scanner_db_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("scanner_db_write");
    group.throughput(Throughput::Elements(FILE_COUNT as u64));

    for &batch_size in WRITE_BATCH_SIZES {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &batch| {
                b.iter_batched(
                    || {
                        let db = Database::open_in_memory().expect("open db");
                        let workspace_id = db.ensure_default_workspace().expect("workspace").id;
                        let source_id = SourceId::new();
                        let source = Source {
                            workspace_id,
                            id: source_id,
                            name: "Bench Source".to_string(),
                            source_type: SourceType::Local,
                            path: "/bench".to_string(),
                            poll_interval_secs: 0,
                            enabled: true,
                        };
                        db.upsert_source(&source).expect("insert source");
                        let files = build_scanned_files(workspace_id, &source_id, FILE_COUNT);
                        (db, files)
                    },
                    |(db, files)| {
                        let mut offset = 0;
                        while offset < files.len() {
                            let end = (offset + batch).min(files.len());
                            db.batch_upsert_files(&files[offset..end], None, true)
                                .expect("batch upsert");
                            offset = end;
                        }
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    scanner_perf,
    bench_scanner_full_scan,
    bench_scanner_rescan,
    bench_scanner_walk_only,
    bench_scanner_db_write
);
criterion_main!(scanner_perf);

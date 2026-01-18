use casparian_mcp::tools::discovery::QuickScanTool;
use casparian_mcp::types::Tool;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const FILE_COUNT: usize = 5_000;
const FILE_SIZE_BYTES: usize = 256;
const DEPTH: usize = 5;
const HIDDEN_COUNT: usize = 200;
const DEPTH_CASES: &[usize] = &[1, 3, 5];
const MAX_FILES_CASES: &[usize] = &[200, 1_000, FILE_COUNT];

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

    for i in 0..HIDDEN_COUNT {
        let hidden_path = temp_dir.path().join(format!(".hidden_{:04}.dat", i));
        let mut file = File::create(hidden_path).expect("create hidden fixture file");
        file.write_all(&payload).expect("write hidden fixture file");
    }

    Fixture { temp_dir, file_count }
}

fn bench_quick_scan_depth(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("create runtime");
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);
    let tool = QuickScanTool::new();

    let mut group = c.benchmark_group("quick_scan_depth");
    group.throughput(Throughput::Elements(fixture.file_count as u64));

    for &depth in DEPTH_CASES {
        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, &depth| {
            b.iter(|| {
                let args = json!({
                    "path": fixture.temp_dir.path().to_string_lossy(),
                    "max_files": fixture.file_count,
                    "max_depth": depth,
                    "include_hidden": false
                });
                rt.block_on(tool.execute(args)).expect("quick_scan");
            });
        });
    }

    group.finish();
}

fn bench_quick_scan_truncate(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("create runtime");
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);
    let tool = QuickScanTool::new();

    let mut group = c.benchmark_group("quick_scan_truncate");

    for &max_files in MAX_FILES_CASES {
        group.throughput(Throughput::Elements(max_files as u64));
        group.bench_with_input(
            BenchmarkId::new("max_files", max_files),
            &max_files,
            |b, &max_files| {
                b.iter(|| {
                    let args = json!({
                        "path": fixture.temp_dir.path().to_string_lossy(),
                        "max_files": max_files,
                        "max_depth": DEPTH,
                        "include_hidden": false
                    });
                    rt.block_on(tool.execute(args)).expect("quick_scan");
                });
            },
        );
    }

    group.finish();
}

fn bench_quick_scan_hidden(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("create runtime");
    let fixture = create_fixture(FILE_COUNT, DEPTH, FILE_SIZE_BYTES);
    let tool = QuickScanTool::new();

    let mut group = c.benchmark_group("quick_scan_hidden");
    group.throughput(Throughput::Elements((fixture.file_count + HIDDEN_COUNT) as u64));

    group.bench_function("include_hidden", |b| {
        b.iter(|| {
            let args = json!({
                "path": fixture.temp_dir.path().to_string_lossy(),
                "max_files": fixture.file_count + HIDDEN_COUNT,
                "max_depth": DEPTH,
                "include_hidden": true
            });
            rt.block_on(tool.execute(args)).expect("quick_scan");
        });
    });

    group.finish();
}

criterion_group!(quick_scan_perf, bench_quick_scan_depth, bench_quick_scan_truncate, bench_quick_scan_hidden);
criterion_main!(quick_scan_perf);

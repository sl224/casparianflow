//! Perf command - benchmarking helpers for scan throughput.

use crate::cli::config::active_db_path;
use crate::cli::error::HelpfulError;
use crate::cli::workspace;
use casparian::scout::scan_path;
use casparian::scout::{
    Database, InProcessEngine, ScanConfig, ScanEngine, Source, SourceId, SourceType,
    SubprocessEngine, WorkspaceId,
};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Subcommands for perf helpers
#[derive(Subcommand, Debug, Clone)]
pub enum PerfAction {
    /// Generate a synthetic fixture with many files
    GenFixture {
        /// Root directory for fixture
        #[arg(long)]
        path: PathBuf,
        /// Number of files to generate
        #[arg(long, default_value = "1000000")]
        files: u64,
        /// Directory fanout depth (levels)
        #[arg(long, default_value = "4")]
        depth: usize,
        /// Size of each file in bytes
        #[arg(long, default_value = "64")]
        size_bytes: u64,
    },
    /// Run a scan benchmark and emit JSON stats
    Scan {
        /// Root directory to scan
        #[arg(long)]
        path: PathBuf,
        /// Database path (defaults to active config db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Batch size for DB persistence
        #[arg(long, default_value = "10000")]
        batch_size: usize,
        /// Thread count for walker (0 = auto)
        #[arg(long, default_value = "0")]
        threads: usize,
        /// Whether to compute per-file stats during upsert
        #[arg(long, default_value_t = true)]
        compute_stats: bool,
        /// Scan engine to use
        #[arg(long, value_enum, default_value_t = ScanEngineKind::InProcess)]
        engine: ScanEngineKind,
        /// Output as JSON (perf scan always outputs JSON)
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScanEngineKind {
    InProcess,
    Subprocess,
}

#[derive(Debug, Serialize, Deserialize)]
struct FixtureMarker {
    files: u64,
    depth: usize,
    size_bytes: u64,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct PerfScanOutput {
    engine: ScanEngineKind,
    path: String,
    file_count_persisted: u64,
    dirs_scanned: u64,
    bytes_scanned: u64,
    duration_ms: u64,
    files_per_sec: f64,
    batch_size: usize,
    threads: usize,
    compute_stats: bool,
    commit: String,
    timestamp: String,
}

pub fn run(action: PerfAction) -> anyhow::Result<()> {
    match action {
        PerfAction::GenFixture {
            path,
            files,
            depth,
            size_bytes,
        } => run_gen_fixture(&path, files, depth, size_bytes),
        PerfAction::Scan {
            path,
            db,
            batch_size,
            threads,
            compute_stats,
            engine,
            json: _,
        } => run_scan(&path, db, batch_size, threads, compute_stats, engine),
    }
}

fn run_gen_fixture(path: &Path, files: u64, depth: usize, size_bytes: u64) -> anyhow::Result<()> {
    if depth == 0 || depth > 8 {
        return Err(HelpfulError::new("depth must be between 1 and 8").into());
    }

    fs::create_dir_all(path)?;
    let marker_path = path.join(".casparian_perf_fixture.json");
    if marker_path.exists() {
        let marker_raw = fs::read_to_string(&marker_path)?;
        let marker: FixtureMarker = serde_json::from_str(&marker_raw)?;
        if marker.files == files && marker.depth == depth && marker.size_bytes == size_bytes {
            println!("Fixture already exists; skipping generation.");
            return Ok(());
        }
        return Err(HelpfulError::new("Fixture exists with different parameters")
            .with_context(format!("Marker: {}", marker_path.display()))
            .with_suggestion("TRY: Delete the fixture directory and re-run".to_string())
            .into());
    }

    let mut last_dir: Option<PathBuf> = None;
    for i in 0..files {
        let rel = fixture_rel_path(i, depth);
        let dir = path.join(rel.parent().unwrap_or(Path::new("")));
        if last_dir.as_ref().map(|d| d != &dir).unwrap_or(true) {
            fs::create_dir_all(&dir)?;
            last_dir = Some(dir.clone());
        }
        let file_path = path.join(&rel);
        let mut file = fs::File::create(&file_path)?;
        if size_bytes > 0 {
            file.set_len(size_bytes)?;
        }
    }

    let marker = FixtureMarker {
        files,
        depth,
        size_bytes,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut marker_file = fs::File::create(&marker_path)?;
    let marker_json = serde_json::to_string_pretty(&marker)?;
    marker_file.write_all(marker_json.as_bytes())?;

    println!(
        "Fixture generated: {} files, depth {}, size {} bytes",
        files, depth, size_bytes
    );

    Ok(())
}

fn fixture_rel_path(index: u64, depth: usize) -> PathBuf {
    let mut path = PathBuf::new();
    for level in 0..depth {
        let shift = (level as u32) * 8;
        let bucket = ((index >> shift) & 0xFF) as u8;
        path.push(format!("l{}_{}", level, bucket));
    }
    path.push(format!("file_{}.dat", index));
    path
}

fn run_scan(
    path: &Path,
    db_override: Option<PathBuf>,
    batch_size: usize,
    threads: usize,
    compute_stats: bool,
    engine: ScanEngineKind,
) -> anyhow::Result<()> {
    let expanded_path = scan_path::expand_scan_path(path);
    if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
        return Err(match err {
            scan_path::ScanPathError::NotFound(path) => HelpfulError::path_not_found(&path),
            scan_path::ScanPathError::NotDirectory(path) => HelpfulError::not_a_directory(&path),
            scan_path::ScanPathError::NotReadable(path) => {
                HelpfulError::new(format!("Cannot read directory: {}", path.display()))
            }
        }
        .into());
    }
    let scan_path = scan_path::canonicalize_scan_path(&expanded_path);

    let db_path = db_override.unwrap_or_else(active_db_path);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
    })?;

    let workspace_id = ensure_workspace_id(&db)?;
    let source = get_or_create_source(&db, &workspace_id, &scan_path)?;

    let mut scan_config = ScanConfig::default();
    scan_config.batch_size = batch_size;
    scan_config.threads = threads;
    scan_config.compute_stats = compute_stats;

    let scan_start = Instant::now();
    let result = match engine {
        ScanEngineKind::InProcess => {
            let engine = InProcessEngine::new(db.clone(), scan_config.clone());
            engine.scan(&source, None, None)?
        }
        ScanEngineKind::Subprocess => {
            let engine = SubprocessEngine::new(db.clone(), scan_config.clone());
            engine.scan(&source, None, None)?
        }
    };
    let elapsed = scan_start.elapsed();

    let stats = result.stats;
    let duration_ms = if stats.duration_ms > 0 {
        stats.duration_ms
    } else {
        elapsed.as_millis() as u64
    };
    let files_per_sec = if duration_ms == 0 {
        0.0
    } else {
        stats.files_persisted as f64 / (duration_ms as f64 / 1000.0)
    };

    let output = PerfScanOutput {
        engine,
        path: scan_path.display().to_string(),
        file_count_persisted: stats.files_persisted,
        dirs_scanned: stats.dirs_scanned,
        bytes_scanned: stats.bytes_scanned,
        duration_ms,
        files_per_sec,
        batch_size,
        threads,
        compute_stats,
        commit: resolve_git_commit(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    println!("{}", serde_json::to_string(&output)?);

    Ok(())
}

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db)
        .map_err(|e| e.with_context("The workspace registry is required for perf scans"))
}

fn get_or_create_source(
    db: &Database,
    workspace_id: &WorkspaceId,
    path: &PathBuf,
) -> anyhow::Result<Source> {
    let path_str = path.display().to_string();

    let sources = db
        .list_sources(workspace_id)
        .map_err(|e| HelpfulError::new(format!("Failed to list sources: {}", e)))?;
    for source in sources {
        if source.path == path_str {
            return Ok(source);
        }
    }

    let id = SourceId::new();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("scan")
        .to_string();

    let source = Source {
        workspace_id: *workspace_id,
        id: id.clone(),
        name,
        source_type: SourceType::Local,
        path: path_str,
        exec_path: None,
        poll_interval_secs: 0,
        enabled: true,
    };

    db.upsert_source(&source)
        .map_err(|e| HelpfulError::new(format!("Failed to create source: {}", e)))?;

    Ok(source)
}

fn resolve_git_commit() -> String {
    if let Ok(commit) = std::env::var("GIT_COMMIT") {
        let trimmed = commit.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    "unknown".to_string()
}

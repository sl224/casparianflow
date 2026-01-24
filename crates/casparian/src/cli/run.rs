//! `casparian run` command - execute a parser against an input file.
//!
//! Standalone mode (no database writes). For production processing,
//! use the sentinel/worker job queue system.
//!
//! # Usage
//!
//! ```bash
//! # Basic usage
//! casparian run parser.py input.csv
//!
//! # With specific sink
//! casparian run parser.py input.csv --sink parquet://./output/
//! casparian run parser.py input.csv --sink duckdb:///data.db
//!
//! # Dry run
//! casparian run parser.py input.csv --whatif
//! ```

use anyhow::{Context, Result};
use casparian::telemetry::TelemetryRecorder;
use casparian_protocol::telemetry as protocol_telemetry;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use uuid::Uuid;

use crate::cli::error::HelpfulError;
use casparian::runner::{DevRunner, LogDestination, ParserRef};
use casparian_security::signing::sha256;
use casparian_sinks::{plan_outputs, write_output_plan, OutputDescriptor};

/// Arguments for the `run` command
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to parser.py file
    #[arg(value_name = "PARSER")]
    pub parser: PathBuf,

    /// Path to input file (CSV, JSON, Parquet)
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Output sink (default: parquet://./output/)
    #[arg(long, short, default_value = "parquet://./output/")]
    pub sink: String,

    /// Force re-processing even if already processed
    #[arg(long)]
    pub force: bool,

    /// Dry run - show what would be processed without writing
    #[arg(long)]
    pub whatif: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct RunOutputInfo {
    name: String,
    table: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunArtifact {
    name: String,
    uri: String,
    rows: u64,
}

#[derive(Debug, Serialize)]
struct RunResult {
    parser: PathBuf,
    input: PathBuf,
    sink: String,
    whatif: bool,
    batches: usize,
    total_rows: usize,
    outputs: Vec<RunOutputInfo>,
    artifacts: Vec<RunArtifact>,
    logs: Option<String>,
}

/// Execute the run command
pub fn cmd_run(args: RunArgs, telemetry: Option<TelemetryRecorder>) -> Result<()> {
    // Validate paths
    if !args.parser.exists() {
        return Err(
            HelpfulError::new(format!("Parser not found: {}", args.parser.display()))
                .with_context("The parser file does not exist")
                .with_suggestion(format!(
                    "TRY: Verify the parser path: ls -la {}",
                    args.parser.display()
                ))
                .into(),
        );
    }
    if !args.input.exists() {
        return Err(
            HelpfulError::new(format!("Input file not found: {}", args.input.display()))
                .with_context("The input file does not exist")
                .with_suggestion(format!(
                    "TRY: Verify the input path: ls -la {}",
                    args.input.display()
                ))
                .into(),
        );
    }

    if args.whatif {
        if args.json {
            let result = RunResult {
                parser: args.parser.clone(),
                input: args.input.clone(),
                sink: args.sink.clone(),
                whatif: true,
                batches: 0,
                total_rows: 0,
                outputs: Vec::new(),
                artifacts: Vec::new(),
                logs: None,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Running parser: {}", args.parser.display());
            println!("Input file: {}", args.input.display());
            println!("Sink: {}", args.sink);
            println!();
            println!("[whatif] Would process file - no output written");
        }
        return Ok(());
    }

    if !args.json {
        println!("Running parser: {}", args.parser.display());
        println!("Input file: {}", args.input.display());
        println!("Sink: {}", args.sink);
    }

    let telemetry_start = Instant::now();
    let telemetry_run_id = telemetry.as_ref().map(|recorder| {
        let run_id = Uuid::new_v4().to_string();
        let payload = protocol_telemetry::RunStarted {
            run_id: run_id.clone(),
            kind: Some("dev_run".to_string()),
            parser_hash: Some(recorder.hasher().hash_path(&args.parser)),
            input_hash: Some(recorder.hasher().hash_path(&args.input)),
            sink_hash: Some(recorder.hasher().hash_str(&args.sink)),
            started_at: chrono::Utc::now(),
        };
        recorder.emit_domain(
            protocol_telemetry::events::RUN_START,
            Some(&run_id),
            None,
            &payload,
        );
        run_id
    });

    let run_result = (|| -> Result<RunResult> {
        let parser_source = std::fs::read_to_string(&args.parser)
            .with_context(|| format!("Failed to read parser: {}", args.parser.display()))?;
        if let Some(venv_path) = ensure_dev_venv(&parser_source)? {
            std::env::set_var("VIRTUAL_ENV", &venv_path);
        }

        // Create dev runner
        let runner = DevRunner::new().context("Failed to initialize runner")?;

        // Execute
        let result = runner.execute(
            ParserRef::Path(args.parser.clone()),
            &args.input,
            LogDestination::Terminal,
        )?;

        let batches = result
            .output_batches
            .iter()
            .map(|group| group.len())
            .sum::<usize>();
        let total_rows = result
            .output_batches
            .iter()
            .flat_map(|group| group.iter())
            .map(|batch| batch.num_rows())
            .sum::<usize>();

        let outputs: Vec<RunOutputInfo> = result
            .output_info
            .iter()
            .map(|info| RunOutputInfo {
                name: info.name.clone(),
                table: info.table.clone(),
            })
            .collect();

        let mut artifacts: Vec<RunArtifact> = Vec::new();

        if !result.output_batches.is_empty() {
            let descriptors: Vec<OutputDescriptor> = result
                .output_info
                .iter()
                .map(|info| OutputDescriptor {
                    name: info.name.clone(),
                    table: info.table.clone(),
                })
                .collect();

            let outputs = plan_outputs(&descriptors, &result.output_batches, "output")?;
            let output_artifacts = write_output_plan(&args.sink, &outputs, "dev", None)?;
            for artifact in output_artifacts {
                let name = artifact.name;
                let uri = artifact.uri;
                let rows = artifact.rows;
                if !args.json {
                    println!("  Wrote {}", uri);
                }
                artifacts.push(RunArtifact { name, uri, rows });
            }
        }

        let logs = if result.logs.is_empty() {
            None
        } else {
            Some(result.logs)
        };

        Ok(RunResult {
            parser: args.parser.clone(),
            input: args.input.clone(),
            sink: args.sink.clone(),
            whatif: false,
            batches,
            total_rows,
            outputs,
            artifacts,
            logs,
        })
    })();

    match run_result {
        Ok(output) => {
            if let (Some(recorder), Some(run_id)) = (telemetry.as_ref(), telemetry_run_id.as_ref())
            {
                let payload = protocol_telemetry::RunCompleted {
                    run_id: run_id.clone(),
                    kind: Some("dev_run".to_string()),
                    duration_ms: telemetry_start.elapsed().as_millis() as u64,
                    total_rows: output.total_rows as u64,
                    outputs: output.outputs.len(),
                };
                recorder.emit_domain(
                    protocol_telemetry::events::RUN_COMPLETE,
                    Some(run_id),
                    None,
                    &payload,
                );
            }

            if args.json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!();
                println!("Execution complete:");
                println!("  Batches: {}", output.batches);
                println!("  Total rows: {}", output.total_rows);
                for info in &output.outputs {
                    println!("  Output '{}'", info.name);
                }
                if let Some(logs) = &output.logs {
                    println!();
                    println!("Parser logs:\n{}", logs);
                }
            }

            Ok(())
        }
        Err(err) => {
            if let (Some(recorder), Some(run_id)) = (telemetry.as_ref(), telemetry_run_id.as_ref())
            {
                let payload = protocol_telemetry::RunFailed {
                    run_id: run_id.clone(),
                    kind: Some("dev_run".to_string()),
                    duration_ms: telemetry_start.elapsed().as_millis() as u64,
                    error_class: classify_run_error(&err),
                };
                recorder.emit_domain(
                    protocol_telemetry::events::RUN_FAIL,
                    Some(run_id),
                    None,
                    &payload,
                );
            }
            Err(err)
        }
    }
}

pub(crate) fn ensure_dev_venv(source: &str) -> Result<Option<PathBuf>> {
    if std::env::var("VIRTUAL_ENV").is_ok() {
        return Ok(None);
    }

    let deps = parse_plugin_dependencies(source);
    let mut all_deps = vec!["pyarrow".to_string(), "pandas".to_string()];
    all_deps.extend(deps);
    all_deps.sort();
    all_deps.dedup();

    let system_python = PathBuf::from("python3");
    if python_supports_modules(&system_python, &all_deps) {
        return Ok(None);
    }

    let deps_str = all_deps.join(",");
    let hash = sha256(deps_str.as_bytes());
    let short_hash = &hash[..16];

    let venv_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("venvs")
        .join(short_hash);
    let interpreter = if cfg!(windows) {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    };

    if !interpreter.exists() {
        std::fs::create_dir_all(&venv_path)?;

        // Use --clear to handle race conditions when multiple processes
        // try to create the same venv concurrently
        let output = Command::new("uv")
            .args(["venv", "--clear", venv_path.to_str().unwrap()])
            .output()
            .context("Failed to create venv with uv")?;

        if !output.status.success() {
            anyhow::bail!(
                "uv venv failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        if !all_deps.is_empty() {
            let mut cmd = Command::new("uv");
            cmd.arg("pip").arg("install");
            for dep in &all_deps {
                cmd.arg(dep);
            }
            cmd.env("VIRTUAL_ENV", &venv_path);

            let output = cmd.output().context("Failed to install dependencies")?;
            if !output.status.success() {
                anyhow::bail!(
                    "uv pip install failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    Ok(Some(venv_path))
}

fn classify_run_error(err: &anyhow::Error) -> String {
    if err.is::<std::io::Error>() {
        "io_error".to_string()
    } else {
        "run_error".to_string()
    }
}

fn python_supports_modules(python: &PathBuf, modules: &[String]) -> bool {
    if modules.is_empty() {
        return true;
    }

    let script = r#"
import importlib.util
import sys

missing = [m for m in sys.argv[1:] if importlib.util.find_spec(m) is None]
sys.exit(0 if not missing else 1)
"#;

    Command::new(python)
        .arg("-c")
        .arg(script)
        .args(modules)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn parse_plugin_dependencies(source: &str) -> Vec<String> {
    let mut deps = vec![];

    for line in source.lines() {
        let line = line.trim();

        // import X or import X as Y
        if let Some(rest) = line.strip_prefix("import ") {
            let module = rest.split_whitespace().next().unwrap_or("");
            let module = module.split('.').next().unwrap_or("");
            if !module.is_empty() && !is_stdlib_module(module) {
                deps.push(module.to_string());
            }
        }
        // from X import Y
        else if let Some(rest) = line.strip_prefix("from ") {
            let module = rest.split_whitespace().next().unwrap_or("");
            let module = module.split('.').next().unwrap_or("");
            if !module.is_empty() && !is_stdlib_module(module) {
                deps.push(module.to_string());
            }
        }
    }

    deps.sort();
    deps.dedup();
    deps
}

fn is_stdlib_module(module: &str) -> bool {
    matches!(
        module,
        "__future__" | "os" | "sys" | "re" | "json" | "math" | "time" | "datetime"
            | "collections" | "itertools" | "functools" | "typing"
            | "pathlib" | "io" | "csv" | "tempfile" | "shutil"
            | "subprocess" | "threading" | "multiprocessing"
            | "hashlib" | "uuid" | "random" | "string" | "struct"
            | "copy" | "enum" | "dataclasses" | "abc" | "contextlib"
            | "decimal" | "stat"  // Used by FIX parser for DECIMAL types and file stat
            | "zoneinfo"  // Python 3.9+ stdlib
            | "casparian_types" // Bridge shim module, not a pip package
    )
}

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
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

use crate::cli::error::HelpfulError;
use crate::runner::{DevRunner, LogDestination, ParserRef, Runner};
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
pub async fn cmd_run(args: RunArgs) -> Result<()> {
    // Validate paths
    if !args.parser.exists() {
        return Err(HelpfulError::new(format!("Parser not found: {}", args.parser.display()))
            .with_context("The parser file does not exist")
            .with_suggestion(format!(
                "TRY: Verify the parser path: ls -la {}",
                args.parser.display()
            ))
            .into());
    }
    if !args.input.exists() {
        return Err(HelpfulError::new(format!("Input file not found: {}", args.input.display()))
            .with_context("The input file does not exist")
            .with_suggestion(format!(
                "TRY: Verify the input path: ls -la {}",
                args.input.display()
            ))
            .into());
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

    // Create dev runner
    let runner = DevRunner::new().context("Failed to initialize runner")?;

    // Execute
    let result = runner
        .execute(
            ParserRef::Path(args.parser.clone()),
            &args.input,
            LogDestination::Terminal,
        )
        .await?;

    let batches = result.batches.len();
    let total_rows = result.batches.iter().map(|b| b.num_rows()).sum::<usize>();

    let outputs: Vec<RunOutputInfo> = result
        .output_info
        .iter()
        .map(|info| RunOutputInfo {
            name: info.name.clone(),
            table: info.table.clone(),
        })
        .collect();

    let mut artifacts: Vec<RunArtifact> = Vec::new();

    if !result.batches.is_empty() {
        let descriptors: Vec<OutputDescriptor> = result
            .output_info
            .iter()
            .map(|info| OutputDescriptor {
                name: info.name.clone(),
                table: info.table.clone(),
            })
            .collect();

        let outputs = plan_outputs(&descriptors, &result.batches, "output")?;
        let output_artifacts = write_output_plan(&args.sink, &outputs, "dev")?;
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

    let output = RunResult {
        parser: args.parser,
        input: args.input,
        sink: args.sink,
        whatif: false,
        batches,
        total_rows,
        outputs,
        artifacts,
        logs,
    };

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

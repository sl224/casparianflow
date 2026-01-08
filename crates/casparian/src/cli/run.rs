//! `casparian run` command - execute a parser against an input file.
//!
//! Dev mode only (no database writes). For production processing,
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
//! casparian run parser.py input.csv --sink sqlite:///data.db
//!
//! # Dry run
//! casparian run parser.py input.csv --whatif
//! ```

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::runner::{DevRunner, LogDestination, ParserRef, Runner};

/// Parsed sink URI
#[derive(Debug, Clone)]
pub enum SinkUri {
    /// Parquet files: parquet://./output/
    Parquet { dir: PathBuf },
    /// CSV files: csv://./output/
    Csv { dir: PathBuf },
    /// SQLite database: sqlite:///path/to/db.sqlite
    Sqlite { path: PathBuf },
}

impl SinkUri {
    /// Parse a sink URI string
    pub fn parse(uri: &str) -> Result<Self> {
        if let Some(path) = uri.strip_prefix("parquet://") {
            Ok(SinkUri::Parquet { dir: PathBuf::from(path) })
        } else if let Some(path) = uri.strip_prefix("csv://") {
            Ok(SinkUri::Csv { dir: PathBuf::from(path) })
        } else if let Some(path) = uri.strip_prefix("sqlite://") {
            // sqlite:///path or sqlite://path
            let path = path.strip_prefix('/').unwrap_or(path);
            Ok(SinkUri::Sqlite { path: PathBuf::from(path) })
        } else {
            anyhow::bail!(
                "Unknown sink URI scheme: {}\n\nSupported schemes:\n  parquet://./output/\n  csv://./output/\n  sqlite:///path/to/db.sqlite",
                uri
            )
        }
    }
}

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
}

/// Execute the run command
pub async fn cmd_run(args: RunArgs) -> Result<()> {
    // Validate paths
    if !args.parser.exists() {
        anyhow::bail!(
            "Parser not found: {}\n\nHint: Make sure the parser file exists and the path is correct.",
            args.parser.display()
        );
    }
    if !args.input.exists() {
        anyhow::bail!(
            "Input file not found: {}\n\nHint: Make sure the input file exists and the path is correct.",
            args.input.display()
        );
    }

    println!("Running parser: {}", args.parser.display());
    println!("Input file: {}", args.input.display());
    println!("Sink: {}", args.sink);

    if args.whatif {
        println!("\n[whatif] Would process file - no output written");
        return Ok(());
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

    println!("\nExecution complete:");
    println!(
        "  Batches: {}",
        result.batches.len()
    );
    println!(
        "  Total rows: {}",
        result.batches.iter().map(|b| b.num_rows()).sum::<usize>()
    );

    // Print output info
    for info in &result.output_info {
        println!("  Output '{}' -> {}", info.name, info.sink);
    }

    // TODO: Write to sink based on args.sink
    // For now, just print info about what would be written
    if !result.batches.is_empty() {
        println!("\nNote: Sink writing not yet implemented in dev mode.");
        println!("      Use `casparian process-job` for production execution.");
    }

    if !result.logs.is_empty() {
        println!("\nParser logs:\n{}", result.logs);
    }

    Ok(())
}

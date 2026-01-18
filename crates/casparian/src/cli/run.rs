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
//! casparian run parser.py input.csv --sink duckdb:///data.db
//!
//! # Dry run
//! casparian run parser.py input.csv --whatif
//! ```

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

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
        println!("  Output '{}'", info.name);
    }

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
        let artifacts = write_output_plan(&args.sink, &outputs, "dev")?;

        for artifact in artifacts {
            println!("  Wrote {}", artifact.uri);
        }
    }

    if !result.logs.is_empty() {
        println!("\nParser logs:\n{}", result.logs);
    }

    Ok(())
}

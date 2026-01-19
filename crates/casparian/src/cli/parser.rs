//! Parser command - Manage parsers
//!
//! Commands:
//! - `parser ls` - List all parsers
//! - `parser show <name>` - Show parser details
//! - `parser test <file.py> --input <data>` - Test a parser against a file
//! - `parser publish <file.py> --topic <topic>` - Deploy a parser
//! - `parser unpublish <name>` - Remove parser from active duty
//! - `parser backtest <name> [--limit N]` - Run parser against all files for its topic

use crate::cli::config;
use crate::cli::error::HelpfulError;
use crate::cli::output::{print_table, print_table_colored};
use chrono::{DateTime, Utc};
use clap::Subcommand;
use comfy_table::Color;
use serde::{Deserialize, Serialize};
use casparian_db::{DbConnection, DbValue};
use std::path::PathBuf;
use std::process::{Command, Stdio};

// Import bundler for register command
use casparian::bundler::bundle_parser;

/// Subcommands for parser management
#[derive(Subcommand, Debug, Clone)]
pub enum ParserAction {
    /// List all parsers
    #[command(name = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show parser details
    Show {
        /// Parser name
        name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Test a parser against a file
    Test {
        /// Path to parser Python file
        parser: PathBuf,
        /// Input data file to test against
        #[arg(long, short)]
        input: PathBuf,
        /// Number of rows to preview
        #[arg(long, short = 'n', default_value = "10")]
        rows: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Publish a parser as a plugin
    Publish {
        /// Path to parser Python file
        parser: PathBuf,
        /// Topic to subscribe to
        #[arg(long)]
        topic: String,
        /// Parser name (defaults to filename without extension)
        #[arg(long)]
        name: Option<String>,
    },
    /// Unpublish a parser (remove from active duty)
    Unpublish {
        /// Parser name
        name: String,
    },
    /// Run parser against all files for its topic
    Backtest {
        /// Parser name
        name: String,
        /// Maximum files to process
        #[arg(long, short = 'n')]
        limit: Option<usize>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Register a parser from a directory (bundles with deterministic hashing)
    Register {
        /// Path to parser directory (must contain uv.lock and parser.py)
        path: PathBuf,
        /// Output bundle as file instead of registering
        #[arg(long)]
        output: Option<PathBuf>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Resume a paused parser (reset circuit breaker)
    Resume {
        /// Parser name to resume
        name: String,
    },
    /// Show health statistics for a parser
    Health {
        /// Parser name
        name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// ============================================================================
// Data Types
// ============================================================================

/// Parser (plugin) info from registry
#[derive(Debug, Clone, Serialize)]
struct Parser {
    name: String,
    version: String,
    status: String,
    source_hash: Option<String>,
    env_hash: Option<String>,
    artifact_hash: Option<String>,
    created_at: DateTime<Utc>,
}

/// Result of testing a parser
#[derive(Debug, Serialize)]
struct ParserTestResult {
    success: bool,
    rows_processed: usize,
    schema: Option<Vec<SchemaColumn>>,
    preview_rows: Vec<Vec<String>>,
    headers: Vec<String>,
    errors: Vec<String>,
    execution_time_ms: u64,
}

/// Schema column information
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchemaColumn {
    name: String,
    dtype: String,
}

/// File info from scout_files
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ScoutFile {
    id: i64,
    path: String,
    tag: Option<String>,
    status: String,
}

/// Backtest result
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct BacktestResult {
    parser_name: String,
    topic: String,
    total_files: usize,
    passed: usize,
    failed: usize,
    failure_analysis: Vec<FailureCategory>,
    schema_variants: Vec<SchemaVariant>,
}

/// Categorized failures
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct FailureCategory {
    error_type: String,
    count: usize,
    sample_files: Vec<String>,
    sample_error: String,
    suggestions: Vec<String>,
}

/// Schema variant found during backtest
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct SchemaVariant {
    columns: Vec<String>,
    file_count: usize,
    sample_files: Vec<String>,
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Execute the parser command
pub fn run(action: ParserAction) -> anyhow::Result<()> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_async(action))
}

async fn run_async(action: ParserAction) -> anyhow::Result<()> {
    match action {
        ParserAction::List { json } => cmd_list(json).await,
        ParserAction::Show { name, json } => cmd_show(&name, json).await,
        ParserAction::Test {
            parser,
            input,
            rows,
            json,
        } => cmd_test(&parser, &input, rows, json),
        ParserAction::Publish {
            parser,
            topic,
            name,
        } => cmd_publish(&parser, &topic, name.as_deref()).await,
        ParserAction::Unpublish { name } => cmd_unpublish(&name).await,
        ParserAction::Backtest { name, limit, json } => {
            cmd_backtest(&name, limit, json).await
        }
        ParserAction::Register { path, output, json } => {
            cmd_register(&path, output.as_deref(), json).await
        }
        ParserAction::Resume { name } => cmd_resume(&name).await,
        ParserAction::Health { name, json } => cmd_health(&name, json).await,
    }
}

// ============================================================================
// Database Connection
// ============================================================================

/// Get database path using config module
fn get_db_path() -> PathBuf {
    config::active_db_path()
}

/// Connect to the database
async fn connect_db() -> anyhow::Result<DbConnection> {
    let db_path = get_db_path();

    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestions([
                "TRY: Run 'casparian start' to initialize the database".to_string(),
                format!("TRY: Check if {} exists", db_path.display()),
            ])
            .into());
    }

    let url = format!("duckdb:{}", db_path.display());
    let conn = DbConnection::open_from_url(&url).await.map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(e.to_string())
            .with_suggestion("TRY: Ensure the database file is not corrupted")
    })?;

    Ok(conn)
}

async fn table_exists(conn: &DbConnection, table: &str) -> anyhow::Result<bool> {
    let row = conn
        .query_optional(
            "SELECT 1 FROM information_schema.tables WHERE table_schema = 'main' AND table_name = ?",
            &[DbValue::from(table)],
        )
        .await?;
    Ok(row.is_some())
}

// ============================================================================
// Command Implementations
// ============================================================================

/// List all parsers
async fn cmd_list(json_output: bool) -> anyhow::Result<()> {
    let conn = connect_db().await?;

    let rows = conn
        .query_all(
            r#"
            SELECT plugin_name, version, status, source_hash, env_hash, artifact_hash, created_at
            FROM cf_plugin_manifest
            ORDER BY created_at DESC
            "#,
            &[],
        )
        .await?;

    let mut parsers = Vec::new();
    for row in rows {
        let source_hash_value: String = row.get_by_name("source_hash").unwrap_or_default();
        let env_hash_value: String = row.get_by_name("env_hash").unwrap_or_default();
        let artifact_hash_value: String = row.get_by_name("artifact_hash").unwrap_or_default();
        let parser = Parser {
            name: row.get_by_name("plugin_name")?,
            version: row.get_by_name("version")?,
            status: row.get_by_name::<String>("status")?,
            source_hash: if source_hash_value.is_empty() {
                None
            } else {
                Some(source_hash_value)
            },
            env_hash: if env_hash_value.is_empty() {
                None
            } else {
                Some(env_hash_value)
            },
            artifact_hash: if artifact_hash_value.is_empty() {
                None
            } else {
                Some(artifact_hash_value)
            },
            created_at: row.get_by_name("created_at")?,
        };
        parsers.push(parser);
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&parsers)?);
        return Ok(());
    }

    if parsers.is_empty() {
        println!("No parsers found.");
        println!();
        println!("To create a parser:");
        println!("  casparian publish <file.py> --version <v>");
        return Ok(());
    }

    println!("Found {} parser(s)", parsers.len());
    println!();

    let headers = &["Name", "Version", "Status", "Created"];
    let table_rows: Vec<Vec<(String, Option<Color>)>> = parsers
        .iter()
        .map(|p| {
            let status = p.status.clone();
            let status_color = match status.as_str() {
                "ACTIVE" | "DEPLOYED" => Some(Color::Green),
                "FAILED" | "ERROR" => Some(Color::Red),
                _ => Some(Color::Yellow),
            };

            vec![
                (p.name.clone(), None),
                (p.version.clone(), Some(Color::Cyan)),
                (status, status_color),
                (format_relative_time(p.created_at), Some(Color::Grey)),
            ]
        })
        .collect();

    print_table_colored(headers, table_rows);

    Ok(())
}

/// Show parser details
async fn cmd_show(name: &str, json_output: bool) -> anyhow::Result<()> {
    let conn = connect_db().await?;

    let row = conn
        .query_optional(
            r#"
            SELECT plugin_name, version, status, source_hash, env_hash, artifact_hash, created_at, source_code
            FROM cf_plugin_manifest
            WHERE plugin_name = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            &[DbValue::from(name)],
        )
        .await?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Parser not found: {}", name))
                .with_context("No plugin with this name exists in the registry")
                .with_suggestions([
                    "TRY: casparian parser ls  (list all parsers)".to_string(),
                    "TRY: casparian publish <file.py> --version <v>".to_string(),
                ])
                .into());
        }
    };

    let source_code: String = row.get_by_name("source_code")?;
    let mut parser = Parser {
        name: row.get_by_name("plugin_name")?,
        version: row.get_by_name("version")?,
        status: row.get_by_name::<String>("status")?,
        source_hash: None,
        env_hash: None,
        artifact_hash: None,
        created_at: row.get_by_name("created_at")?,
    };

    let source_hash_value: String = row.get_by_name("source_hash").unwrap_or_default();
    if !source_hash_value.is_empty() {
        parser.source_hash = Some(source_hash_value);
    }

    let env_hash_value: String = row.get_by_name("env_hash").unwrap_or_default();
    if !env_hash_value.is_empty() {
        parser.env_hash = Some(env_hash_value);
    }

    let artifact_hash_value: String = row.get_by_name("artifact_hash").unwrap_or_default();
    if !artifact_hash_value.is_empty() {
        parser.artifact_hash = Some(artifact_hash_value);
    }

    if json_output {
        let result = serde_json::json!({
            "name": parser.name,
            "version": parser.version,
            "status": parser.status,
            "source_hash": parser.source_hash,
            "env_hash": parser.env_hash,
            "artifact_hash": parser.artifact_hash,
            "created_at": parser.created_at,
            "source_code": source_code,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("Parser: {}", parser.name);
    println!("========================================");
    println!();
    println!("Version:      {}", parser.version);
    println!("Status:       {}", parser.status);
    println!("Created:      {}", parser.created_at);
    if let Some(hash) = &parser.source_hash {
        println!("Source Hash:  {}...", &hash[..hash.len().min(16)]);
    }
    if let Some(hash) = &parser.env_hash {
        println!("Env Hash:     {}...", &hash[..hash.len().min(16)]);
    }
    if let Some(hash) = &parser.artifact_hash {
        println!("Artifact:     {}...", &hash[..hash.len().min(16)]);
    }

    println!();
    println!("Source Code (first 20 lines):");
    println!("----------------------------------------");
    for (i, line) in source_code.lines().take(20).enumerate() {
        println!("{:>4} | {}", i + 1, line);
    }
    let total_lines = source_code.lines().count();
    if total_lines > 20 {
        println!("      ... ({} more lines)", total_lines - 20);
    }

    Ok(())
}

/// Test a parser against a file
fn cmd_test(parser_path: &PathBuf, input_path: &PathBuf, rows: usize, json_output: bool) -> anyhow::Result<()> {
    // Validate parser file exists
    if !parser_path.exists() {
        return Err(HelpfulError::new(format!("Parser file not found: {}", parser_path.display()))
            .with_context("The specified parser file does not exist")
            .with_suggestions([
                format!("TRY: ls -la {}", parser_path.display()),
                "TRY: Provide the full path to the parser file".to_string(),
            ])
            .into());
    }

    // Validate input file exists
    if !input_path.exists() {
        return Err(HelpfulError::new(format!("Input file not found: {}", input_path.display()))
            .with_context("The specified input file does not exist")
            .with_suggestions([
                format!("TRY: ls -la {}", input_path.display()),
                "TRY: Provide the full path to the input file".to_string(),
            ])
            .into());
    }

    // Validate parser is a Python file
    if parser_path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err(HelpfulError::new("Parser must be a Python file")
            .with_context(format!("Got: {}", parser_path.display()))
            .with_suggestion("TRY: Parser files must have .py extension")
            .into());
    }

    // Test the parser by running it
    let start = std::time::Instant::now();
    let (success, rows_processed, schema, preview_rows, headers, errors, _error_code) =
        run_parser_test(parser_path, input_path, rows)?;
    let execution_time_ms = start.elapsed().as_millis() as u64;

    let test_result = ParserTestResult {
        success,
        rows_processed,
        schema,
        preview_rows,
        headers,
        errors,
        execution_time_ms,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&test_result)?);
        return Ok(());
    }

    // Pretty print results
    println!("Parser Test Results");
    println!("========================================");
    println!();
    println!("Parser:      {}", parser_path.display());
    println!("Input:       {}", input_path.display());
    println!("Time:        {}ms", test_result.execution_time_ms);
    println!();

    if test_result.success {
        println!("Status:      PASSED");
        println!("Rows:        {}", test_result.rows_processed);
    } else {
        println!("Status:      FAILED");
        if !test_result.errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &test_result.errors {
                println!("  {}", error);
            }
        }
        return Ok(());
    }

    // Show schema
    if let Some(schema) = &test_result.schema {
        println!();
        println!("Inferred Schema ({} columns):", schema.len());
        let headers = &["Column", "Type"];
        let schema_rows: Vec<Vec<String>> = schema
            .iter()
            .map(|col| vec![col.name.clone(), col.dtype.clone()])
            .collect();
        print_table(headers, schema_rows);
    }

    // Show preview
    if !test_result.preview_rows.is_empty() {
        println!();
        println!("Output Preview (first {} rows):", test_result.preview_rows.len());
        let headers: Vec<&str> = test_result.headers.iter().map(|s| s.as_str()).collect();
        print_table(&headers, test_result.preview_rows.clone());
    }

    Ok(())
}

/// Publish a parser to the registry.
async fn cmd_publish(parser_path: &PathBuf, _topic: &str, _name: Option<&str>) -> anyhow::Result<()> {
    Err(HelpfulError::new("Parser publish is deprecated")
        .with_context("Use the Sentinel publisher workflow for v1")
        .with_suggestions([
            format!(
                "TRY: casparian publish {} --version <v>",
                parser_path.display()
            ),
            "NOTE: --topic is ignored in v1; configure sinks via topic config instead".to_string(),
        ])
        .into())
}

/// Unpublish a parser
async fn cmd_unpublish(name: &str) -> anyhow::Result<()> {
    let conn = connect_db().await?;

    let updated = conn
        .execute(
            "UPDATE cf_plugin_manifest SET status = 'INACTIVE' WHERE plugin_name = ? AND status = 'ACTIVE'",
            &[DbValue::from(name)],
        )
        .await?;

    if updated == 0 {
        return Err(HelpfulError::new(format!("Parser not found or already inactive: {}", name))
            .with_context("No active plugin with this name exists")
            .with_suggestion("TRY: casparian parser ls  (list all parsers)")
            .into());
    }

    println!("Unpublished parser: {}", name);

    Ok(())
}

/// Run backtest against all files for a parser's topic
async fn cmd_backtest(name: &str, limit: Option<usize>, json_output: bool) -> anyhow::Result<()> {
    let _ = (limit, json_output);
    Err(HelpfulError::new(format!("Backtest is not available in v1: {}", name))
        .with_context("The parser lab registry was removed in favor of the plugin manifest")
        .with_suggestions([
            "TRY: casparian run <parser.py> <input> for manual testing".to_string(),
            "TRY: casparian publish <parser.py> --version <v> then run jobs via Sentinel".to_string(),
        ])
        .into())
}

/// Register a parser from a directory by bundling it
async fn cmd_register(
    path: &PathBuf,
    output: Option<&std::path::Path>,
    json_output: bool,
) -> anyhow::Result<()> {
    // Validate directory exists
    if !path.exists() {
        return Err(HelpfulError::new(format!("Parser directory not found: {}", path.display()))
            .with_context("The specified directory does not exist")
            .with_suggestions([
                format!("TRY: ls -la {}", path.display()),
                "TRY: Provide the full path to the parser directory".to_string(),
            ])
            .into());
    }

    if !path.is_dir() {
        return Err(HelpfulError::new(format!("Path is not a directory: {}", path.display()))
            .with_context("The register command expects a directory containing parser files")
            .with_suggestions([
                "TRY: casparian parser register ./my_parser_dir".to_string(),
                "TRY: For single files, use 'casparian publish'".to_string(),
            ])
            .into());
    }

    // Bundle the parser
    let bundle = bundle_parser(path).map_err(|e| {
        HelpfulError::new("Failed to bundle parser")
            .with_context(e.to_string())
            .with_suggestions([
                "TRY: Ensure uv.lock exists: run 'uv lock' in the parser directory".to_string(),
                "TRY: Ensure parser has 'name' and 'version' attributes".to_string(),
            ])
    })?;

    // If output path specified, write bundle to file
    if let Some(output_path) = output {
        std::fs::write(output_path, &bundle.archive).map_err(|e| {
            HelpfulError::new("Failed to write bundle file")
                .with_context(e.to_string())
                .with_suggestion(format!("TRY: Check write permissions for {}", output_path.display()))
        })?;

        if json_output {
            let result = serde_json::json!({
                "name": bundle.name,
                "version": bundle.version,
                "source_hash": bundle.source_hash,
                "lockfile_hash": bundle.lockfile_hash,
                "archive_size": bundle.archive.len(),
                "output_path": output_path.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Bundled {} v{}", bundle.name, bundle.version);
            println!("  Source hash:   {}", &bundle.source_hash[..12]);
            println!("  Lockfile hash: {}", &bundle.lockfile_hash[..12]);
            println!("  Archive size:  {:.1}KB", bundle.archive.len() as f64 / 1024.0);
            println!("  Output:        {}", output_path.display());
        }

        return Ok(());
    }

    Err(HelpfulError::new("Parser register is deprecated")
        .with_context("Use the Sentinel publisher workflow for v1")
        .with_suggestions([
            "TRY: casparian publish <parser.py> --version <v>".to_string(),
            "TRY: casparian parser register --output <bundle.zip> to export an archive".to_string(),
        ])
        .into())
}


/// Resume a paused parser (reset circuit breaker)
async fn cmd_resume(name: &str) -> anyhow::Result<()> {
    let conn = connect_db().await?;

    // Check if cf_parser_health table exists
    if !table_exists(&conn, "cf_parser_health").await? {
        return Err(HelpfulError::new("Parser health table not found")
            .with_context("No circuit breaker data exists yet")
            .with_suggestion("TRY: Run some jobs first to generate health data")
            .into());
    }

    // Check if parser exists in health table
    let health = conn
        .query_optional(
            "SELECT consecutive_failures, paused_at FROM cf_parser_health WHERE parser_name = ?",
            &[DbValue::from(name)],
        )
        .await?;

    match health {
        Some(row) => {
            let failures: i64 = row.get_by_name("consecutive_failures").unwrap_or_default();
            let paused_at: Option<String> = row.get_by_name("paused_at").ok();
            if paused_at.is_none() && failures == 0 {
                println!("Parser '{}' is already healthy (not paused)", name);
                return Ok(());
            }

            // Reset the circuit breaker
            conn.execute(
                "UPDATE cf_parser_health SET paused_at = NULL, consecutive_failures = 0, updated_at = CURRENT_TIMESTAMP WHERE parser_name = ?",
                &[DbValue::from(name)],
            )
            .await?;

            println!("Parser '{}' resumed", name);
            println!("  - Circuit breaker reset");
            println!("  - Consecutive failures cleared");
            println!();
            println!("The parser will now accept new jobs.");
        }
        None => {
            return Err(HelpfulError::new(format!("No health data for parser: {}", name))
                .with_context("This parser has never been executed")
                .with_suggestions([
                    "TRY: casparian jobs create --parser <name> --input <file>".to_string(),
                    "TRY: casparian parser ls  (list all parsers)".to_string(),
                ])
                .into());
        }
    }

    Ok(())
}

/// Show health statistics for a parser
async fn cmd_health(name: &str, json_output: bool) -> anyhow::Result<()> {
    let conn = connect_db().await?;

    // Check if cf_parser_health table exists
    if !table_exists(&conn, "cf_parser_health").await? {
        return Err(HelpfulError::new("Parser health table not found")
            .with_context("No circuit breaker data exists yet")
            .with_suggestion("TRY: Run some jobs first to generate health data")
            .into());
    }

    // Get parser health data
    let health = conn
        .query_optional(
            r#"
            SELECT parser_name, total_executions, successful_executions, consecutive_failures,
                   last_failure_reason, paused_at
            FROM cf_parser_health
            WHERE parser_name = ?
            "#,
            &[DbValue::from(name)],
        )
        .await?;

    match health {
        Some(row) => {
            let parser_name: String = row.get_by_name("parser_name").unwrap_or_default();
            let total: i64 = row.get_by_name("total_executions").unwrap_or_default();
            let success: i64 = row.get_by_name("successful_executions").unwrap_or_default();
            let failures: i64 = row.get_by_name("consecutive_failures").unwrap_or_default();
            let last_error: Option<String> = row.get_by_name("last_failure_reason").ok();
            let paused_at: Option<String> = row.get_by_name("paused_at").ok();
            let success_rate = if total > 0 {
                (success as f64 / total as f64) * 100.0
            } else {
                100.0
            };

            if json_output {
                let result = serde_json::json!({
                    "parser_name": parser_name,
                    "total_executions": total,
                    "successful_executions": success,
                    "consecutive_failures": failures,
                    "success_rate": success_rate,
                    "last_failure_reason": last_error,
                    "paused": paused_at.is_some(),
                    "paused_at": paused_at,
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            println!("Parser Health: {}", parser_name);
            println!("========================================");
            println!();
            println!("Execution Stats:");
            println!("  Total:       {}", total);
            println!("  Successful:  {}", success);
            println!("  Failed:      {}", total - success);
            println!("  Success Rate: {:.1}%", success_rate);
            println!();
            println!("Circuit Breaker:");
            println!("  Consecutive Failures: {}", failures);
            if let Some(paused) = paused_at {
                println!("  Status:               PAUSED (tripped at {})", paused);
                println!();
                println!("To resume this parser:");
                println!("  casparian parser resume {}", parser_name);
            } else if failures > 0 {
                println!("  Status:               WARNING ({}/5 failures)", failures);
            } else {
                println!("  Status:               HEALTHY");
            }

            if let Some(error) = last_error {
                println!();
                println!("Last Failure Reason:");
                // Truncate long error messages
                let error_display = if error.len() > 200 {
                    format!("{}...", &error[..200])
                } else {
                    error
                };
                println!("  {}", error_display);
            }
        }
        None => {
            return Err(HelpfulError::new(format!("No health data for parser: {}", name))
                .with_context("This parser has never been executed")
                .with_suggestions([
                    "TRY: casparian jobs create --parser <name> --input <file>".to_string(),
                    "TRY: casparian parser ls  (list all parsers)".to_string(),
                ])
                .into());
        }
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Run a parser test against an input file
/// Returns: (success, rows, schema, preview_rows, headers, errors)
fn run_parser_test(
    parser_path: &PathBuf,
    input_path: &PathBuf,
    preview_rows: usize,
) -> anyhow::Result<(bool, usize, Option<Vec<SchemaColumn>>, Vec<Vec<String>>, Vec<String>, Vec<String>, Option<String>)> {
    // Create a wrapper script that runs the parser
    let wrapper = format!(
        r#"
import sys
import json
import traceback

# Add parser directory to path
sys.path.insert(0, "{parser_dir}")

try:
    # Try to import polars first, fall back to pandas
    try:
        import polars as pl
        USE_POLARS = True
    except ImportError:
        import pandas as pd
        USE_POLARS = False

    # Import the parser module
    import importlib.util
    spec = importlib.util.spec_from_file_location("parser", "{parser_path}")
    parser_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(parser_module)

    # Find the transform function
    if hasattr(parser_module, 'transform'):
        transform = parser_module.transform
    elif hasattr(parser_module, 'parse'):
        transform = parser_module.parse
    elif hasattr(parser_module, 'process'):
        transform = parser_module.process
    else:
        raise ValueError("Parser must have a 'transform', 'parse', or 'process' function")

    # Read input file
    input_path = "{input_path}"
    if USE_POLARS:
        if input_path.endswith('.csv'):
            df = pl.read_csv(input_path)
        elif input_path.endswith('.json'):
            df = pl.read_json(input_path)
        elif input_path.endswith('.parquet'):
            df = pl.read_parquet(input_path)
        else:
            df = pl.read_csv(input_path)  # Default to CSV
    else:
        if input_path.endswith('.csv'):
            df = pd.read_csv(input_path)
        elif input_path.endswith('.json'):
            df = pd.read_json(input_path)
        elif input_path.endswith('.parquet'):
            df = pd.read_parquet(input_path)
        else:
            df = pd.read_csv(input_path)

    # Run transform
    result = transform(df)

    # Get schema
    if USE_POLARS:
        schema = [
            {{"name": name, "dtype": str(dtype)}}
            for name, dtype in zip(result.columns, result.dtypes)
        ]
        rows = result.head({preview_rows}).to_dicts()
        total_rows = len(result)
    else:
        schema = [
            {{"name": name, "dtype": str(dtype)}}
            for name, dtype in result.dtypes.items()
        ]
        rows = result.head({preview_rows}).to_dict(orient='records')
        total_rows = len(result)

    output = {{
        "success": True,
        "total_rows": total_rows,
        "schema": schema,
        "rows": rows,
        "errors": []
    }}
    print(json.dumps(output))

except Exception as e:
    error_msg = str(e)
    tb = traceback.format_exc()

    # Structured error classification
    error_code = "UNKNOWN_ERROR"
    if isinstance(e, KeyError):
        error_code = "SCHEMA_MISMATCH"
    elif isinstance(e, FileNotFoundError):
        error_code = "FILE_NOT_FOUND"
    elif isinstance(e, PermissionError):
        error_code = "PERMISSION_ERROR"
    elif isinstance(e, UnicodeDecodeError):
        error_code = "ENCODING_ERROR"
    elif isinstance(e, MemoryError):
        error_code = "MEMORY_ERROR"
    elif isinstance(e, ValueError):
        if "convert" in error_msg.lower() or "invalid" in error_msg.lower():
            error_code = "INVALID_DATA"
    elif isinstance(e, TypeError):
        if "convert" in error_msg.lower():
            error_code = "INVALID_DATA"

    output = {{
        "success": False,
        "total_rows": 0,
        "schema": None,
        "rows": [],
        "errors": [error_msg, tb],
        "error_code": error_code
    }}
    print(json.dumps(output))
"#,
        parser_dir = parser_path.parent().unwrap_or(parser_path).display(),
        parser_path = parser_path.display(),
        input_path = input_path.display(),
        preview_rows = preview_rows,
    );

    let output = Command::new("python3")
        .args(["-c", &wrapper])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Ok((
                false,
                0,
                None,
                vec![],
                vec![],
                vec![format!("Failed to run Python: {}", e)],
                Some("EXECUTION_ERROR".to_string()),
            ));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Try to parse JSON output
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let success = result["success"].as_bool().unwrap_or(false);
        let total_rows = result["total_rows"].as_u64().unwrap_or(0) as usize;

        let schema: Option<Vec<SchemaColumn>> = result["schema"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(SchemaColumn {
                            name: v["name"].as_str()?.to_string(),
                            dtype: v["dtype"].as_str()?.to_string(),
                        })
                    })
                    .collect()
            });

        let headers: Vec<String> = schema
            .as_ref()
            .map(|s| s.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();

        let rows: Vec<Vec<String>> = result["rows"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|row| {
                        headers
                            .iter()
                            .map(|h| {
                                row.get(h)
                                    .map(|v| {
                                        if v.is_string() {
                                            v.as_str().unwrap_or("").to_string()
                                        } else {
                                            v.to_string()
                                        }
                                    })
                                    .unwrap_or_default()
                            })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let errors: Vec<String> = result["errors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Extract structured error code from Python
        let error_code: Option<String> = result["error_code"]
            .as_str()
            .map(|s| s.to_string());

        Ok((success, total_rows, schema, rows, headers, errors, error_code))
    } else {
        // Failed to parse output
        let mut errors = vec![];
        if !stdout.is_empty() {
            errors.push(format!("stdout: {}", stdout));
        }
        if !stderr.is_empty() {
            errors.push(format!("stderr: {}", stderr));
        }
        if errors.is_empty() {
            errors.push("No output from parser".to_string());
        }

        Ok((false, 0, None, vec![], vec![], errors, None))
    }
}

/// Categorize an error by type
/// Prefers structured error_code from Python if available, falls back to string matching
#[allow(dead_code)]
fn categorize_error(error_code: Option<&str>, errors: &[String]) -> String {
    // Prefer structured error code from Python (no string matching needed)
    if let Some(code) = error_code {
        return code.to_string();
    }

    // Fallback: string matching for legacy/external errors
    let error_text = errors.join(" ").to_lowercase();

    if error_text.contains("missing column") || error_text.contains("keyerror") {
        "SCHEMA_MISMATCH".to_string()
    } else if error_text.contains("could not convert") || error_text.contains("invalid") {
        "INVALID_DATA".to_string()
    } else if error_text.contains("file not found") || error_text.contains("no such file") {
        "FILE_NOT_FOUND".to_string()
    } else if error_text.contains("permission") {
        "PERMISSION_ERROR".to_string()
    } else if error_text.contains("encoding") || error_text.contains("decode") {
        "ENCODING_ERROR".to_string()
    } else if error_text.contains("memory") || error_text.contains("out of memory") {
        "MEMORY_ERROR".to_string()
    } else {
        "UNKNOWN_ERROR".to_string()
    }
}

/// Get suggestions for an error type
#[allow(dead_code)]
fn get_error_suggestions(error_type: &str) -> Vec<String> {
    match error_type {
        "SCHEMA_MISMATCH" => vec![
            "Re-tag files with different schema: casparian rule add \"pattern\" --topic <new_topic>".to_string(),
            "Update parser to handle missing columns with defaults".to_string(),
        ],
        "INVALID_DATA" => vec![
            "Add data validation in parser to handle edge cases".to_string(),
            "Use try/except to catch and log invalid rows".to_string(),
        ],
        "FILE_NOT_FOUND" => vec![
            "Re-scan sources: casparian scan <path>".to_string(),
            "Check if files were moved or deleted".to_string(),
        ],
        "ENCODING_ERROR" => vec![
            "Specify encoding in parser: pl.read_csv(path, encoding='latin1')".to_string(),
            "Convert files to UTF-8 before processing".to_string(),
        ],
        "MEMORY_ERROR" => vec![
            "Process files in smaller batches".to_string(),
            "Use streaming/chunked reading in parser".to_string(),
        ],
        _ => vec![
            "Check parser logs for more details".to_string(),
            "Test parser manually: python3 <parser.py>".to_string(),
        ],
    }
}

/// Format a datetime as relative time
fn format_relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{}d ago", duration.num_days())
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_categorize_error_with_structured_code() {
        // Structured error_code from Python takes precedence
        assert_eq!(
            categorize_error(Some("SCHEMA_MISMATCH"), &["some error text".to_string()]),
            "SCHEMA_MISMATCH"
        );
        assert_eq!(
            categorize_error(Some("INVALID_DATA"), &["KeyError: would match string".to_string()]),
            "INVALID_DATA"
        );
    }

    #[test]
    fn test_categorize_error_fallback_string_matching() {
        // When no structured error_code, fall back to string matching
        assert_eq!(
            categorize_error(None, &["KeyError: 'missing_column'".to_string()]),
            "SCHEMA_MISMATCH"
        );
        assert_eq!(
            categorize_error(None, &["Could not convert 'abc' to float".to_string()]),
            "INVALID_DATA"
        );
        assert_eq!(
            categorize_error(None, &["FileNotFoundError: No such file".to_string()]),
            "FILE_NOT_FOUND"
        );
        assert_eq!(
            categorize_error(None, &["Some random error".to_string()]),
            "UNKNOWN_ERROR"
        );
    }

    #[test]
    fn test_format_relative_time() {
        let now = Utc::now();

        // Just now
        assert!(format_relative_time(now).ends_with("s ago"));

        // 5 minutes ago
        let five_min_ago = now - chrono::Duration::minutes(5);
        assert!(format_relative_time(five_min_ago).ends_with("m ago"));

        // 2 hours ago
        let two_hours_ago = now - chrono::Duration::hours(2);
        assert!(format_relative_time(two_hours_ago).ends_with("h ago"));

        // 3 days ago
        let three_days_ago = now - chrono::Duration::days(3);
        assert!(format_relative_time(three_days_ago).ends_with("d ago"));
    }

    #[test]
    fn test_parser_test_with_valid_parser() {
        let temp_dir = TempDir::new().unwrap();

        // Create a simple parser
        let parser_path = temp_dir.path().join("test_parser.py");
        let mut parser_file = File::create(&parser_path).unwrap();
        writeln!(
            parser_file,
            r#"
def transform(df):
    return df
"#
        )
        .unwrap();

        // Create a simple CSV
        let input_path = temp_dir.path().join("test_data.csv");
        let mut input_file = File::create(&input_path).unwrap();
        writeln!(input_file, "id,name,value").unwrap();
        writeln!(input_file, "1,foo,100").unwrap();
        writeln!(input_file, "2,bar,200").unwrap();

        // Run test
        let result = run_parser_test(&parser_path, &input_path, 10);
        assert!(result.is_ok());

        let (success, rows, schema, _, _, errors, _error_code) = result.unwrap();
        // Success depends on whether polars/pandas is installed
        // In CI, we may not have these, so we just check the function runs
        if errors.is_empty() {
            assert!(success);
            assert!(rows > 0);
            assert!(schema.is_some());
        }
    }

    #[test]
    fn test_parser_test_missing_file() {
        let temp_dir = TempDir::new().unwrap();

        let parser_path = temp_dir.path().join("parser.py");
        let input_path = temp_dir.path().join("nonexistent.csv");

        // Parser file doesn't exist
        let result = cmd_test(&parser_path, &input_path, 10, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_error_suggestions() {
        let suggestions = get_error_suggestions("SCHEMA_MISMATCH");
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("tag") || suggestions[0].contains("Re-tag"));

        let suggestions = get_error_suggestions("UNKNOWN_ERROR");
        assert!(!suggestions.is_empty());
    }
}

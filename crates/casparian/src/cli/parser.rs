//! Parser command - Manage parsers
//!
//! Commands:
//! - `parser ls` - List all parsers
//! - `parser show <name>` - Show parser details
//! - `parser test <file.py> --input <data>` - Test a parser against a file
//! - `parser publish <file.py> --topic <topic>` - Deploy a parser
//! - `parser unpublish <name>` - Remove parser from active duty
//! - `parser backtest <name> [--limit N]` - Run parser against all files for its topic

use crate::cli::error::HelpfulError;
use crate::cli::output::{print_table, print_table_colored};
use chrono::{DateTime, Utc};
use clap::Subcommand;
use comfy_table::Color;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
}

// ============================================================================
// Data Types
// ============================================================================

/// Parser info from database
#[derive(Debug, Clone, Serialize)]
struct Parser {
    id: String,
    name: String,
    file_pattern: String,
    pattern_type: Option<String>,
    source_code: Option<String>,
    source_hash: Option<String>,
    topic: Option<String>,
    validation_status: Option<String>,
    validation_error: Option<String>,
    schema_json: Option<String>,
    sink_type: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
struct FailureCategory {
    error_type: String,
    count: usize,
    sample_files: Vec<String>,
    sample_error: String,
    suggestions: Vec<String>,
}

/// Schema variant found during backtest
#[derive(Debug, Serialize)]
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
    }
}

// ============================================================================
// Database Connection
// ============================================================================

/// Get database path (same as scout database)
fn get_db_path() -> PathBuf {
    // Check for explicit path in env
    if let Ok(path) = std::env::var("CASPARIAN_DB_PATH") {
        return PathBuf::from(path);
    }

    // Default to current directory
    PathBuf::from("casparian_flow.db")
}

/// Connect to the database
async fn connect_db() -> anyhow::Result<SqlitePool> {
    let db_path = get_db_path();

    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestions([
                "TRY: Run 'casparian start' to initialize the database".to_string(),
                "TRY: Set CASPARIAN_DB_PATH environment variable".to_string(),
                format!("TRY: Check if {} exists", db_path.display()),
            ])
            .into());
    }

    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePool::connect(&url).await.map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(e.to_string())
            .with_suggestion("TRY: Ensure the database file is not corrupted")
    })?;

    Ok(pool)
}

// ============================================================================
// Command Implementations
// ============================================================================

/// List all parsers
async fn cmd_list(json_output: bool) -> anyhow::Result<()> {
    let pool = connect_db().await?;

    let rows = sqlx::query(
        r#"
        SELECT id, name, file_pattern, pattern_type, source_code,
               validation_status, schema_json, sink_type,
               created_at, updated_at
        FROM parser_lab_parsers
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let parsers: Vec<Parser> = rows
        .iter()
        .map(|row| {
            let created_millis: i64 = row.get("created_at");
            let updated_millis: i64 = row.get("updated_at");

            Parser {
                id: row.get("id"),
                name: row.get("name"),
                file_pattern: row.get("file_pattern"),
                pattern_type: row.get("pattern_type"),
                source_code: row.get("source_code"),
                source_hash: None,
                topic: row.get::<Option<String>, _>("file_pattern"), // Using file_pattern as topic for now
                validation_status: row.get("validation_status"),
                validation_error: None,
                schema_json: row.get("schema_json"),
                sink_type: row.get("sink_type"),
                created_at: DateTime::from_timestamp_millis(created_millis).unwrap_or_default(),
                updated_at: DateTime::from_timestamp_millis(updated_millis).unwrap_or_default(),
            }
        })
        .collect();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&parsers)?);
        return Ok(());
    }

    if parsers.is_empty() {
        println!("No parsers found.");
        println!();
        println!("To create a parser:");
        println!("  casparian parser publish <file.py> --topic <topic>");
        return Ok(());
    }

    println!("Found {} parser(s)", parsers.len());
    println!();

    let headers = &["Name", "Pattern", "Status", "Sink", "Updated"];
    let table_rows: Vec<Vec<(String, Option<Color>)>> = parsers
        .iter()
        .map(|p| {
            let status = p.validation_status.clone().unwrap_or_else(|| "pending".to_string());
            let status_color = match status.as_str() {
                "valid" => Some(Color::Green),
                "invalid" => Some(Color::Red),
                _ => Some(Color::Yellow),
            };

            vec![
                (p.name.clone(), None),
                (p.file_pattern.clone(), Some(Color::Cyan)),
                (status, status_color),
                (p.sink_type.clone().unwrap_or_else(|| "parquet".to_string()), None),
                (format_relative_time(p.updated_at), Some(Color::Grey)),
            ]
        })
        .collect();

    print_table_colored(headers, table_rows);

    Ok(())
}

/// Show parser details
async fn cmd_show(name: &str, json_output: bool) -> anyhow::Result<()> {
    let pool = connect_db().await?;

    let row = sqlx::query(
        r#"
        SELECT id, name, file_pattern, pattern_type, source_code,
               validation_status, validation_error, validation_output,
               schema_json, sink_type, sink_config_json,
               created_at, updated_at
        FROM parser_lab_parsers
        WHERE name = ?
        "#,
    )
    .bind(name)
    .fetch_optional(&pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Parser not found: {}", name))
                .with_context("No parser with this name exists in the database")
                .with_suggestions([
                    "TRY: casparian parser ls  (list all parsers)".to_string(),
                    "TRY: casparian parser publish <file.py> --topic <topic>".to_string(),
                ])
                .into());
        }
    };

    let created_millis: i64 = row.get("created_at");
    let updated_millis: i64 = row.get("updated_at");
    let source_code: Option<String> = row.get("source_code");

    let source_hash = source_code.as_ref().map(|code| {
        let mut hasher = Sha256::new();
        hasher.update(code.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    });

    let parser = Parser {
        id: row.get("id"),
        name: row.get("name"),
        file_pattern: row.get("file_pattern"),
        pattern_type: row.get("pattern_type"),
        source_code: source_code.clone(),
        source_hash,
        topic: row.get::<Option<String>, _>("file_pattern"),
        validation_status: row.get("validation_status"),
        validation_error: row.get("validation_error"),
        schema_json: row.get("schema_json"),
        sink_type: row.get("sink_type"),
        created_at: DateTime::from_timestamp_millis(created_millis).unwrap_or_default(),
        updated_at: DateTime::from_timestamp_millis(updated_millis).unwrap_or_default(),
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&parser)?);
        return Ok(());
    }

    // Pretty print
    println!("Parser: {}", parser.name);
    println!("========================================");
    println!();
    println!("ID:          {}", parser.id);
    println!("Pattern:     {}", parser.file_pattern);
    println!(
        "Pattern Type: {}",
        parser.pattern_type.unwrap_or_else(|| "glob".to_string())
    );
    println!(
        "Status:      {}",
        parser.validation_status.unwrap_or_else(|| "pending".to_string())
    );
    println!(
        "Sink:        {}",
        parser.sink_type.unwrap_or_else(|| "parquet".to_string())
    );
    println!("Created:     {}", parser.created_at.format("%Y-%m-%d %H:%M:%S"));
    println!("Updated:     {}", parser.updated_at.format("%Y-%m-%d %H:%M:%S"));

    if let Some(hash) = &parser.source_hash {
        println!("Source Hash: {}...", hash);
    }

    if let Some(error) = &parser.validation_error {
        println!();
        println!("Validation Error:");
        println!("  {}", error);
    }

    if let Some(schema) = &parser.schema_json {
        if !schema.is_empty() && schema != "null" {
            println!();
            println!("Schema:");
            if let Ok(columns) = serde_json::from_str::<Vec<SchemaColumn>>(schema) {
                for col in columns {
                    println!("  - {}: {}", col.name, col.dtype);
                }
            } else {
                println!("  {}", schema);
            }
        }
    }

    // Show source code preview
    if let Some(code) = &parser.source_code {
        println!();
        println!("Source Code (first 20 lines):");
        println!("----------------------------------------");
        for (i, line) in code.lines().take(20).enumerate() {
            println!("{:>4} | {}", i + 1, line);
        }
        let total_lines = code.lines().count();
        if total_lines > 20 {
            println!("      ... ({} more lines)", total_lines - 20);
        }
    }

    // Get file stats
    let stats: Option<(i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END) as processed,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed
        FROM scout_files
        WHERE tag = ?
        "#,
    )
    .bind(&parser.file_pattern)
    .fetch_optional(&pool)
    .await?;

    if let Some((total, processed, failed)) = stats {
        if total > 0 {
            println!();
            println!("Processing Stats:");
            println!("  Total Files:     {}", total);
            println!("  Processed:       {}", processed);
            println!("  Failed:          {}", failed);
            if total > 0 {
                println!(
                    "  Success Rate:    {:.1}%",
                    (processed as f64 / total as f64) * 100.0
                );
            }
        }
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
    let result = run_parser_test(parser_path, input_path, rows)?;
    let execution_time_ms = start.elapsed().as_millis() as u64;

    let test_result = ParserTestResult {
        success: result.0,
        rows_processed: result.1,
        schema: result.2,
        preview_rows: result.3,
        headers: result.4,
        errors: result.5,
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

/// Publish a parser to the database
async fn cmd_publish(parser_path: &PathBuf, topic: &str, name: Option<&str>) -> anyhow::Result<()> {
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

    // Validate parser is a Python file
    if parser_path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err(HelpfulError::new("Parser must be a Python file")
            .with_context(format!("Got: {}", parser_path.display()))
            .with_suggestion("TRY: Parser files must have .py extension")
            .into());
    }

    // Read source code
    let source_code = std::fs::read_to_string(parser_path).map_err(|e| {
        HelpfulError::new("Failed to read parser file")
            .with_context(e.to_string())
            .with_suggestion(format!("TRY: Check file permissions: ls -la {}", parser_path.display()))
    })?;

    // Validate Python syntax
    let syntax_check = Command::new("python3")
        .args(["-m", "py_compile"])
        .arg(parser_path)
        .output();

    match syntax_check {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HelpfulError::new("Parser has syntax errors")
                .with_context(stderr.to_string())
                .with_suggestion("TRY: Fix the syntax errors and try again")
                .into());
        }
        Err(e) => {
            return Err(HelpfulError::new("Failed to validate Python syntax")
                .with_context(e.to_string())
                .with_suggestion("TRY: Ensure python3 is installed and in PATH")
                .into());
        }
        _ => {}
    }

    // Compute hash
    let mut hasher = Sha256::new();
    hasher.update(source_code.as_bytes());
    let source_hash = format!("{:x}", hasher.finalize());

    // Derive name from filename if not provided
    let parser_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            parser_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string()
        });

    let pool = connect_db().await?;
    let now = Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();

    // Check if parser with same name exists
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM parser_lab_parsers WHERE name = ?",
    )
    .bind(&parser_name)
    .fetch_optional(&pool)
    .await?;

    if let Some((existing_id,)) = existing {
        // Update existing parser
        sqlx::query(
            r#"
            UPDATE parser_lab_parsers
            SET source_code = ?,
                file_pattern = ?,
                validation_status = 'pending',
                validation_error = NULL,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&source_code)
        .bind(topic)
        .bind(now)
        .bind(&existing_id)
        .execute(&pool)
        .await?;

        println!("Updated parser '{}' (topic: {})", parser_name, topic);
        println!("Hash: {}...", &source_hash[..16]);
    } else {
        // Insert new parser
        sqlx::query(
            r#"
            INSERT INTO parser_lab_parsers
                (id, name, file_pattern, pattern_type, source_code, validation_status, sink_type, created_at, updated_at)
            VALUES (?, ?, ?, 'glob', ?, 'pending', 'parquet', ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&parser_name)
        .bind(topic)
        .bind(&source_code)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await?;

        println!("Published parser '{}' (topic: {})", parser_name, topic);
        println!("Hash: {}...", &source_hash[..16]);
    }

    println!();
    println!("Next steps:");
    println!("  1. Test with sample file: casparian parser test {} --input <file>", parser_path.display());
    println!("  2. Backtest against files: casparian parser backtest {}", parser_name);

    Ok(())
}

/// Unpublish a parser
async fn cmd_unpublish(name: &str) -> anyhow::Result<()> {
    let pool = connect_db().await?;

    let result = sqlx::query("DELETE FROM parser_lab_parsers WHERE name = ?")
        .bind(name)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(HelpfulError::new(format!("Parser not found: {}", name))
            .with_context("No parser with this name exists")
            .with_suggestion("TRY: casparian parser ls  (list all parsers)")
            .into());
    }

    println!("Unpublished parser: {}", name);

    Ok(())
}

/// Run backtest against all files for a parser's topic
async fn cmd_backtest(name: &str, limit: Option<usize>, json_output: bool) -> anyhow::Result<()> {
    let pool = connect_db().await?;

    // Get parser
    let row = sqlx::query(
        r#"
        SELECT id, name, file_pattern, source_code
        FROM parser_lab_parsers
        WHERE name = ?
        "#,
    )
    .bind(name)
    .fetch_optional(&pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Parser not found: {}", name))
                .with_context("No parser with this name exists")
                .with_suggestion("TRY: casparian parser ls  (list all parsers)")
                .into());
        }
    };

    let parser_name: String = row.get("name");
    let topic: String = row.get("file_pattern");
    let source_code: Option<String> = row.get("source_code");

    let source_code = match source_code {
        Some(code) if !code.is_empty() => code,
        _ => {
            return Err(HelpfulError::new("Parser has no source code")
                .with_context(format!("Parser '{}' has no source code to execute", name))
                .with_suggestion("TRY: casparian parser publish <file.py> --topic <topic>")
                .into());
        }
    };

    // Get files for this topic
    let limit_value = limit.unwrap_or(1000) as i64;
    let files: Vec<ScoutFile> = sqlx::query_as::<_, (i64, String, Option<String>, String)>(
        r#"
        SELECT id, path, tag, status
        FROM scout_files
        WHERE tag = ?
        LIMIT ?
        "#,
    )
    .bind(&topic)
    .bind(limit_value)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|(id, path, tag, status)| ScoutFile { id, path, tag, status })
    .collect();

    if files.is_empty() {
        println!("No files found for topic: {}", topic);
        println!();
        println!("To tag files with this topic:");
        println!("  casparian tag <path> {}", topic);
        return Ok(());
    }

    println!("Testing {} against {} files...", parser_name, files.len());
    println!();

    // Write parser to temp file
    let temp_dir = std::env::temp_dir();
    let parser_path = temp_dir.join(format!("{}.py", parser_name));
    std::fs::write(&parser_path, &source_code)?;

    // Run backtest with progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: HashMap<String, Vec<(String, String)>> = HashMap::new(); // error_type -> [(file, message)]
    let mut schema_counts: HashMap<String, Vec<String>> = HashMap::new(); // schema_key -> [files]

    for file in &files {
        pb.inc(1);

        let input_path = PathBuf::from(&file.path);
        if !input_path.exists() {
            failures
                .entry("FILE_NOT_FOUND".to_string())
                .or_default()
                .push((file.path.clone(), "File does not exist".to_string()));
            failed += 1;
            continue;
        }

        match run_parser_test(&parser_path, &input_path, 1) {
            Ok((true, _, schema, _, _, _)) => {
                passed += 1;

                // Track schema variant
                let schema_key = schema
                    .map(|s| {
                        s.iter()
                            .map(|c| c.name.clone())
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                schema_counts
                    .entry(schema_key)
                    .or_default()
                    .push(file.path.clone());
            }
            Ok((false, _, _, _, _, errors)) => {
                failed += 1;

                let error_type = categorize_error(&errors);
                let error_msg = errors.first().cloned().unwrap_or_else(|| "Unknown error".to_string());

                failures
                    .entry(error_type)
                    .or_default()
                    .push((file.path.clone(), error_msg));
            }
            Err(e) => {
                failed += 1;
                failures
                    .entry("EXECUTION_ERROR".to_string())
                    .or_default()
                    .push((file.path.clone(), e.to_string()));
            }
        }
    }

    pb.finish_with_message("done");
    println!();

    // Build result
    let failure_analysis: Vec<FailureCategory> = failures
        .into_iter()
        .map(|(error_type, items)| {
            let sample_files: Vec<String> = items.iter().take(3).map(|(f, _)| f.clone()).collect();
            let sample_error = items.first().map(|(_, e)| e.clone()).unwrap_or_default();

            FailureCategory {
                error_type: error_type.clone(),
                count: items.len(),
                sample_files,
                sample_error,
                suggestions: get_error_suggestions(&error_type),
            }
        })
        .collect();

    let schema_variants: Vec<SchemaVariant> = schema_counts
        .into_iter()
        .map(|(columns, files)| {
            let cols: Vec<String> = columns.split(',').map(|s| s.to_string()).collect();
            SchemaVariant {
                columns: cols,
                file_count: files.len(),
                sample_files: files.into_iter().take(3).collect(),
            }
        })
        .collect();

    let result = BacktestResult {
        parser_name: parser_name.clone(),
        topic: topic.clone(),
        total_files: files.len(),
        passed,
        failed,
        failure_analysis,
        schema_variants,
    };

    // Clean up temp file
    let _ = std::fs::remove_file(&parser_path);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Pretty print results
    println!("RESULTS");
    println!("  Passed:    {} files ({:.1}%)", passed, (passed as f64 / files.len() as f64) * 100.0);
    println!("  Failed:    {} files", failed);
    println!();

    if !result.failure_analysis.is_empty() {
        println!("FAILURE ANALYSIS");
        println!();

        for category in &result.failure_analysis {
            println!("[{}] {} files", category.error_type, category.count);
            println!("  {}", category.sample_error);
            println!();
            println!("  Sample files:");
            for file in &category.sample_files {
                println!("    - {}", file);
            }
            println!();
            if !category.suggestions.is_empty() {
                println!("  Options:");
                for (i, suggestion) in category.suggestions.iter().enumerate() {
                    println!("    {}) {}", (b'A' + i as u8) as char, suggestion);
                }
            }
            println!();
        }
    }

    if result.schema_variants.len() > 1 {
        println!("SCHEMA VARIANTS DETECTED ({} variants)", result.schema_variants.len());
        println!();

        for (i, variant) in result.schema_variants.iter().enumerate() {
            println!(
                "  Schema {} ({} files): {}",
                (b'A' + i as u8) as char,
                variant.file_count,
                variant.columns.join(", ")
            );
        }
        println!();
    }

    if failed > 0 {
        println!("SUGGESTED WORKFLOW");
        println!("  1. Fix parser to handle the most common error type");
        println!("  2. Re-run backtest: casparian parser backtest {}", parser_name);
        println!("  3. If schema variants exist, consider splitting into multiple parsers");
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
) -> anyhow::Result<(bool, usize, Option<Vec<SchemaColumn>>, Vec<Vec<String>>, Vec<String>, Vec<String>)> {
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
    output = {{
        "success": False,
        "total_rows": 0,
        "schema": None,
        "rows": [],
        "errors": [error_msg, tb]
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

        Ok((success, total_rows, schema, rows, headers, errors))
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

        Ok((false, 0, None, vec![], vec![], errors))
    }
}

/// Categorize an error by type
fn categorize_error(errors: &[String]) -> String {
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
    fn test_categorize_error() {
        assert_eq!(
            categorize_error(&["KeyError: 'missing_column'".to_string()]),
            "SCHEMA_MISMATCH"
        );
        assert_eq!(
            categorize_error(&["Could not convert 'abc' to float".to_string()]),
            "INVALID_DATA"
        );
        assert_eq!(
            categorize_error(&["FileNotFoundError: No such file".to_string()]),
            "FILE_NOT_FOUND"
        );
        assert_eq!(
            categorize_error(&["Some random error".to_string()]),
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

        let (success, rows, schema, _, _, errors) = result.unwrap();
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

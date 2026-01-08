//! Backfill command - Re-process files when parser version changes
//!
//! When a parser is updated to a new version, this command identifies files
//! that were processed with old versions and need re-processing.
//!
//! The backfill workflow:
//! 1. Find the latest version of a parser
//! 2. Find all files tagged with the parser's subscribed topics
//! 3. Identify files that haven't been processed with the latest version
//! 4. Re-process them (or preview what would be processed)
//!
//! Usage:
//!   casparian backfill my_parser              # Preview files to backfill
//!   casparian backfill my_parser --execute    # Actually run backfill
//!   casparian backfill my_parser --limit 10   # Limit to 10 files

use anyhow::{Context, Result};

/// Arguments for the backfill command
#[derive(Debug, Clone)]
pub struct BackfillArgs {
    /// Parser name to backfill
    pub parser_name: String,
    /// Actually execute the backfill (default: preview mode)
    pub execute: bool,
    /// Maximum files to process
    pub limit: Option<usize>,
    /// Output as JSON
    pub json: bool,
    /// Force re-processing even if already processed with this version
    pub force: bool,
}

/// File that needs backfill processing
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillFile {
    /// File path
    pub path: String,
    /// File hash (blake3)
    pub file_hash: String,
    /// Last processed version (if any)
    pub last_version: Option<String>,
    /// Current parser version
    pub target_version: String,
    /// File tag
    pub tag: String,
}

/// Result of backfill operation
#[derive(Debug, serde::Serialize)]
pub struct BackfillResult {
    /// Parser name
    pub parser_name: String,
    /// Parser version
    pub parser_version: String,
    /// Topics subscribed by this parser
    pub topics: Vec<String>,
    /// Files that need backfill
    pub files_to_process: Vec<BackfillFile>,
    /// Files already processed with current version
    pub files_current: usize,
    /// Files processed (if --execute)
    pub files_processed: usize,
    /// Files failed (if --execute)
    pub files_failed: usize,
    /// Was this a dry run?
    pub preview_only: bool,
}

/// Run the backfill command
pub async fn run(args: BackfillArgs) -> Result<()> {
    use super::config::default_db_path;

    let db_path = default_db_path();
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = sqlx::SqlitePool::connect(&db_url)
        .await
        .with_context(|| format!("Failed to connect to database: {}", db_path.display()))?;

    // 1. Get the latest version of the parser
    let parser_info: Option<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT parser_id, name, version
        FROM cf_parsers
        WHERE name = ?
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(&args.parser_name)
    .fetch_optional(&pool)
    .await
    .context("Failed to query cf_parsers")?;

    let (parser_id, parser_name, parser_version) = match parser_info {
        Some(info) => info,
        None => {
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": format!("Parser '{}' not found", args.parser_name),
                        "suggestion": "Run a parser first with: casparian run parser.py input.csv"
                    })
                );
            } else {
                println!("Parser '{}' not found.", args.parser_name);
                println!();
                println!("To register a parser, run it at least once:");
                println!("  casparian run <parser.py> <input.csv>");
            }
            return Ok(());
        }
    };

    // 2. Get topics this parser subscribes to
    let topics: Vec<(String,)> = sqlx::query_as(
        "SELECT topic FROM cf_parser_topics WHERE parser_name = ?",
    )
    .bind(&parser_name)
    .fetch_all(&pool)
    .await
    .context("Failed to query cf_parser_topics")?;

    let topics: Vec<String> = topics.into_iter().map(|(t,)| t).collect();

    if topics.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "error": format!("Parser '{}' has no topic subscriptions", parser_name),
                    "suggestion": "Ensure your parser declares 'topics' attribute"
                })
            );
        } else {
            println!("Parser '{}' has no topic subscriptions.", parser_name);
            println!();
            println!("Ensure your parser class declares topics:");
            println!("  class MyParser:");
            println!("      name = 'my_parser'");
            println!("      version = '1.0.0'");
            println!("      topics = ['my_topic']  # <-- required");
        }
        return Ok(());
    }

    // 3. Find files with matching tags that haven't been processed with this parser version
    //
    // Query logic:
    // - Find scout_files where tag IN (parser's topics)
    // - LEFT JOIN with cf_processing_history on (file hash, parser_id)
    // - WHERE cf_processing_history.job_id IS NULL (not processed with this version)
    //
    // Note: This requires scout_files table and source_hash column

    let topic_placeholders: String = topics.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let query = format!(
        r#"
        SELECT
            f.path,
            f.source_hash,
            f.tag,
            h.job_id as last_job_id,
            p2.version as last_version
        FROM scout_files f
        LEFT JOIN cf_processing_history h ON f.source_hash = h.input_hash AND h.parser_id = ?
        LEFT JOIN cf_parsers p2 ON h.parser_id = p2.parser_id
        WHERE f.tag IN ({})
          AND (h.job_id IS NULL OR ? = 1)
        ORDER BY f.path
        LIMIT ?
        "#,
        topic_placeholders
    );

    let limit = args.limit.unwrap_or(1000) as i64;
    let force_flag = if args.force { 1i32 } else { 0i32 };

    // Build query with bindings
    let mut query_builder = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>)>(&query);
    query_builder = query_builder.bind(&parser_id);
    for topic in &topics {
        query_builder = query_builder.bind(topic);
    }
    query_builder = query_builder.bind(force_flag);
    query_builder = query_builder.bind(limit);

    let files_result = query_builder.fetch_all(&pool).await;

    // Handle case where scout_files table doesn't exist
    let files: Vec<BackfillFile> = match files_result {
        Ok(rows) => rows
            .into_iter()
            .map(|(path, source_hash, tag, _last_job, last_version)| BackfillFile {
                path,
                file_hash: source_hash.unwrap_or_default(),
                last_version,
                target_version: parser_version.clone(),
                tag: tag.unwrap_or_default(),
            })
            .collect(),
        Err(e) => {
            // Table might not exist - check if it's a "no such table" error
            let err_str = e.to_string();
            if err_str.contains("no such table") {
                if args.json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "error": "scout_files table not found",
                            "suggestion": "Run 'casparian scan' to discover files first"
                        })
                    );
                } else {
                    println!("No scout_files table found.");
                    println!();
                    println!("To discover files, run:");
                    println!("  casparian scan <directory> --tag <topic>");
                }
                return Ok(());
            }
            return Err(e).context("Failed to query files for backfill");
        }
    };

    // 4. Count files already processed with current version
    let current_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cf_processing_history WHERE parser_id = ?",
    )
    .bind(&parser_id)
    .fetch_one(&pool)
    .await
    .unwrap_or((0,));

    // 5. Build result
    let mut result = BackfillResult {
        parser_name: parser_name.clone(),
        parser_version: parser_version.clone(),
        topics: topics.clone(),
        files_to_process: files.clone(),
        files_current: current_count.0 as usize,
        files_processed: 0,
        files_failed: 0,
        preview_only: !args.execute,
    };

    // 6. Execute backfill if requested
    if args.execute && !files.is_empty() {
        // Get parser source code to re-run
        let parser_row: Option<(String,)> = sqlx::query_as(
            "SELECT source_hash FROM cf_parsers WHERE parser_id = ?",
        )
        .bind(&parser_id)
        .fetch_optional(&pool)
        .await
        .context("Failed to get parser source")?;

        if parser_row.is_none() {
            anyhow::bail!("Parser '{}' not found in database", parser_name);
        }

        // For now, print instructions - actual execution would require
        // knowing the parser file path
        println!("To execute backfill, run each file with:");
        println!();
        for file in &files {
            println!(
                "  casparian run <parser.py> {} --force",
                file.path
            );
        }
        println!();
        println!("Automatic execution not yet implemented - use the commands above.");
        println!();

        // Mark as preview since we can't auto-execute yet
        result.preview_only = true;
    }

    // 7. Output results
    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Pretty print
    println!("=== Backfill Analysis ===");
    println!();
    println!("Parser:  {} v{}", result.parser_name, result.parser_version);
    println!("Topics:  {}", result.topics.join(", "));
    println!();
    println!(
        "Files already processed with v{}:  {}",
        result.parser_version, result.files_current
    );
    println!("Files to backfill:                {}", result.files_to_process.len());
    println!();

    if result.files_to_process.is_empty() {
        println!("All files are up to date with the current parser version.");
        return Ok(());
    }

    if result.preview_only {
        println!("Files needing backfill:");
        println!();

        let limit_display = 20;
        for (i, file) in result.files_to_process.iter().take(limit_display).enumerate() {
            let version_info = file
                .last_version
                .as_ref()
                .map(|v| format!(" (was v{})", v))
                .unwrap_or_else(|| " (never processed)".to_string());

            println!(
                "  {}. {}{}",
                i + 1,
                file.path,
                version_info
            );
        }

        if result.files_to_process.len() > limit_display {
            println!("  ... and {} more", result.files_to_process.len() - limit_display);
        }

        println!();
        println!("To execute backfill:");
        println!("  casparian backfill {} --execute", args.parser_name);
    }

    Ok(())
}

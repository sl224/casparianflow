//! Tag command - Assign topics to files
//!
//! Two modes:
//! 1. Apply rules: `casparian tag [--dry-run] [--no-queue]`
//! 2. Manual tag: `casparian tag <path> <topic>`

use crate::cli::error::HelpfulError;
use crate::cli::output::format_size;
use casparian::scout::{FileStatus, SourceId, TaggingRuleId};
use casparian_db::{DbConnection, DbValue};
use glob::Pattern;
use std::collections::HashMap;
use std::path::PathBuf;

/// Arguments for the tag command
#[derive(Debug)]
pub struct TagArgs {
    pub path: Option<PathBuf>,
    pub topic: Option<String>,
    pub dry_run: bool,
    pub no_queue: bool,
}

/// Arguments for the untag command
#[derive(Debug)]
pub struct UntagArgs {
    pub path: PathBuf,
}

/// A tagging rule from the database
#[derive(Debug, Clone)]
struct TaggingRule {
    id: TaggingRuleId,
    pattern: String,
    tag: String,
    #[allow(dead_code)]
    priority: i32,
    source_id: SourceId,
}

/// A file from the database
#[derive(Debug, Clone)]
struct ScannedFile {
    id: i64,
    path: String,
    rel_path: String,
    size: i64,
    tag: Option<String>,
    #[allow(dead_code)]
    status: String,
    source_id: SourceId,
}

/// Summary of tagging operation
#[derive(Debug, Default)]
struct TaggingSummary {
    /// Pattern -> (tag, file_count, total_bytes)
    matches: HashMap<String, (String, usize, u64)>,
    /// Files that would be queued
    would_queue: usize,
    /// New files in queue
    new_in_queue: usize,
    /// Untagged files (no pattern matched)
    untagged: usize,
}

/// Get the default database path
fn get_db_path() -> PathBuf {
    crate::cli::config::active_db_path()
}

/// Open database connection with helpful error
fn open_db() -> Result<DbConnection, HelpfulError> {
    let db_path = get_db_path();

    if !db_path.exists() {
        return Err(HelpfulError::new(format!("Database not found: {}", db_path.display()))
            .with_context("The Scout database has not been initialized yet")
            .with_suggestions([
                "TRY: Start the Casparian UI to initialize the database".to_string(),
                "TRY: Run `casparian start` to initialize the system".to_string(),
                format!("TRY: Check the path exists: {}", db_path.display()),
            ]));
    }

    let db_url = format!("duckdb:{}", db_path.display());
    DbConnection::open_from_url(&db_url)
        
        .map_err(|e| {
            let mut err = HelpfulError::new(format!("Cannot open database: {}", e))
                .with_context(format!("Database path: {}", db_path.display()));
            if e.to_string().contains("locked") {
                err = err.with_suggestion(
                    "TRY: Close other Casparian processes holding the database lock".to_string(),
                );
            } else {
                err = err.with_suggestion("TRY: Ensure the database file is not corrupted".to_string());
            }
            err
        })
}

/// Check if a glob pattern matches a path
fn pattern_matches(pattern: &str, path: &str) -> bool {
    // Try both the full pattern and with a leading slash stripped
    Pattern::new(pattern)
        .map(|p| p.matches(path) || p.matches(&path.trim_start_matches('/')))
        .unwrap_or(false)
}

/// Load all enabled tagging rules from the database
fn load_tagging_rules(conn: &DbConnection) -> Result<Vec<TaggingRule>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT id, pattern, tag, priority, source_id \
             FROM scout_tagging_rules \
             WHERE enabled = 1 \
             ORDER BY priority DESC, id",
            &[],
        )
        
        .map_err(|e| {
            HelpfulError::new(format!("Failed to query tagging rules: {}", e))
                .with_context("The scout_tagging_rules table may not exist")
                .with_suggestion("TRY: Ensure the database schema is up to date")
        })?;

    let mut rules = Vec::new();
    for row in rows {
        let id_raw: i64 = row
            .get_by_name("id")
            .map_err(|e| HelpfulError::new(format!("Failed to read rule id: {}", e)))?;
        let source_id_raw: i64 = row
            .get_by_name("source_id")
            .map_err(|e| HelpfulError::new(format!("Failed to read rule source_id: {}", e)))?;
        let id = TaggingRuleId::try_from(id_raw).map_err(|e| {
            HelpfulError::new(format!("Invalid rule id: {}", e))
        })?;
        let source_id = SourceId::try_from(source_id_raw).map_err(|e| {
            HelpfulError::new(format!("Invalid rule source_id: {}", e))
        })?;

        let rule = TaggingRule {
            id,
            pattern: row
                .get_by_name("pattern")
                .map_err(|e| HelpfulError::new(format!("Failed to read rule pattern: {}", e)))?,
            tag: row
                .get_by_name("tag")
                .map_err(|e| HelpfulError::new(format!("Failed to read rule tag: {}", e)))?,
            priority: row
                .get_by_name("priority")
                .map_err(|e| HelpfulError::new(format!("Failed to read rule priority: {}", e)))?,
            source_id,
        };
        rules.push(rule);
    }

    Ok(rules)
}

/// Load untagged files from the database
fn load_untagged_files(conn: &DbConnection) -> Result<Vec<ScannedFile>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT id, path, rel_path, size, tag, status, source_id \
             FROM scout_files \
             WHERE tag IS NULL AND status = ? \
             ORDER BY path",
            &[DbValue::from(FileStatus::Pending.as_str())],
        )
        
        .map_err(|e| {
            HelpfulError::new(format!("Failed to query files: {}", e))
                .with_context("The scout_files table may not exist")
                .with_suggestion("TRY: Run a scan first with `casparian scan`")
        })?;

    let mut files = Vec::new();
    for row in rows {
        let source_id_raw: i64 = row
            .get_by_name("source_id")
            .map_err(|e| HelpfulError::new(format!("Failed to read file source_id: {}", e)))?;
        let source_id = SourceId::try_from(source_id_raw).map_err(|e| {
            HelpfulError::new(format!("Invalid file source_id: {}", e))
        })?;
        let file = ScannedFile {
            id: row
                .get_by_name("id")
                .map_err(|e| HelpfulError::new(format!("Failed to read file id: {}", e)))?,
            path: row
                .get_by_name("path")
                .map_err(|e| HelpfulError::new(format!("Failed to read file path: {}", e)))?,
            rel_path: row
                .get_by_name("rel_path")
                .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
            size: row
                .get_by_name("size")
                .map_err(|e| HelpfulError::new(format!("Failed to read file size: {}", e)))?,
            tag: row
                .get_by_name("tag")
                .map_err(|e| HelpfulError::new(format!("Failed to read file tag: {}", e)))?,
            status: row
                .get_by_name("status")
                .map_err(|e| HelpfulError::new(format!("Failed to read file status: {}", e)))?,
            source_id,
        };
        files.push(file);
    }

    Ok(files)
}

/// Get file by path
fn get_file_by_path(
    conn: &DbConnection,
    path: &str,
) -> Result<Option<ScannedFile>, HelpfulError> {
    let row = conn
        .query_optional(
            "SELECT id, path, rel_path, size, tag, status, source_id \
             FROM scout_files \
             WHERE path = ?",
            &[DbValue::from(path)],
        )
        
        .map_err(|e| HelpfulError::new(format!("Failed to query file: {}", e)))?;

    let row = match row {
        Some(row) => row,
        None => return Ok(None),
    };

    let source_id_raw: i64 = row
        .get_by_name("source_id")
        .map_err(|e| HelpfulError::new(format!("Failed to read file source_id: {}", e)))?;
    let source_id = SourceId::try_from(source_id_raw).map_err(|e| {
        HelpfulError::new(format!("Invalid file source_id: {}", e))
    })?;

    let file = ScannedFile {
        id: row
            .get_by_name("id")
            .map_err(|e| HelpfulError::new(format!("Failed to read file id: {}", e)))?,
        path: row
            .get_by_name("path")
            .map_err(|e| HelpfulError::new(format!("Failed to read file path: {}", e)))?,
        rel_path: row
            .get_by_name("rel_path")
            .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
        size: row
            .get_by_name("size")
            .map_err(|e| HelpfulError::new(format!("Failed to read file size: {}", e)))?,
        tag: row
            .get_by_name("tag")
            .map_err(|e| HelpfulError::new(format!("Failed to read file tag: {}", e)))?,
        status: row
            .get_by_name("status")
            .map_err(|e| HelpfulError::new(format!("Failed to read file status: {}", e)))?,
        source_id,
    };

    Ok(Some(file))
}

/// Apply tag to a file in the database
fn apply_tag(
    conn: &DbConnection,
    file_id: i64,
    tag: &str,
    tag_source: &str,
    rule_id: Option<TaggingRuleId>,
) -> Result<(), HelpfulError> {
    let rule_id_value = match rule_id {
        Some(id) => DbValue::from(id.as_i64()),
        None => DbValue::Null,
    };
    conn.execute(
        "UPDATE scout_files \
         SET tag = ?, tag_source = ?, rule_id = ?, status = ? \
         WHERE id = ?",
        &[
            DbValue::from(tag),
            DbValue::from(tag_source),
            rule_id_value,
            DbValue::from(FileStatus::Tagged.as_str()),
            DbValue::from(file_id),
        ],
    )
    
    .map_err(|e| {
        HelpfulError::new(format!("Failed to update file tag: {}", e))
            .with_context(format!("File ID: {}", file_id))
    })?;

    Ok(())
}

/// Remove tag from a file in the database
fn remove_tag(conn: &DbConnection, file_id: i64) -> Result<(), HelpfulError> {
    conn.execute(
        "UPDATE scout_files \
         SET tag = NULL, tag_source = NULL, rule_id = NULL, status = ?, sentinel_job_id = NULL \
         WHERE id = ?",
        &[
            DbValue::from(FileStatus::Pending.as_str()),
            DbValue::from(file_id),
        ],
    )
    
    .map_err(|e| {
        HelpfulError::new(format!("Failed to remove file tag: {}", e))
            .with_context(format!("File ID: {}", file_id))
    })?;

    Ok(())
}

/// Count total files
fn count_all_files(conn: &DbConnection) -> Result<i64, HelpfulError> {
    conn.query_scalar::<i64>("SELECT COUNT(*) FROM scout_files", &[])
        
        .map_err(|e| HelpfulError::new(format!("Failed to count files: {}", e)))
}

/// Execute the tag command
pub fn run(args: TagArgs) -> anyhow::Result<()> {
    run_with_args(args)
}

fn run_with_args(args: TagArgs) -> anyhow::Result<()> {
    // Determine mode: manual tag or apply rules
    match (&args.path, &args.topic) {
        // Manual tag mode: casparian tag <path> <topic>
        (Some(path), Some(topic)) => run_manual_tag(path, topic),

        // Apply rules mode: casparian tag [--dry-run] [--no-queue]
        (None, None) => run_apply_rules(args.dry_run, args.no_queue),

        // Partial args - invalid
        (Some(_), None) => {
            Err(HelpfulError::new("Missing topic for manual tagging")
                .with_context("When tagging a specific file, you must provide both path and topic")
                .with_suggestions([
                    "TRY: casparian tag /path/to/file.csv my_topic".to_string(),
                    "TRY: casparian tag --dry-run   (to apply rules)".to_string(),
                ])
                .into())
        }
        (None, Some(_)) => {
            Err(HelpfulError::new("Missing path for manual tagging")
                .with_context("When tagging with a specific topic, you must provide the file path")
                .with_suggestions([
                    "TRY: casparian tag /path/to/file.csv my_topic".to_string(),
                    "TRY: casparian tag   (to apply rules to all files)".to_string(),
                ])
                .into())
        }
    }
}

/// Run manual tagging of a single file
fn run_manual_tag(path: &PathBuf, topic: &str) -> anyhow::Result<()> {
    let conn = open_db()?;

    // Normalize path
    let path_str = path.to_string_lossy().to_string();

    // Find file in database
    let file = get_file_by_path(&conn, &path_str)?;

    match file {
        Some(f) => {
            apply_tag(&conn, f.id, topic, "manual", None)?;
            println!("Tagged: {} -> {}", f.path, topic);
            println!();
            println!("File will be processed by plugins subscribed to topic '{}'", topic);
            Ok(())
        }
        None => {
            // Try to find by relative path or partial match
            let similar_rows = conn
                .query_all(
                    "SELECT path FROM scout_files WHERE path LIKE ? OR rel_path LIKE ? LIMIT 5",
                    &[
                        DbValue::from(format!("%{}%", path_str)),
                        DbValue::from(format!("%{}%", path_str)),
                    ],
                )
                
                .map_err(|e| HelpfulError::new(format!("Failed to search for file: {}", e)))?;

            let mut similar = Vec::new();
            for row in similar_rows {
                let path: String = row
                    .get_by_name("path")
                    .map_err(|e| HelpfulError::new(format!("Failed to read file path: {}", e)))?;
                similar.push(path);
            }

            let mut err = HelpfulError::new(format!("File not found in database: {}", path_str))
                .with_context("The file must be discovered by Scout before it can be tagged");

            if !similar.is_empty() {
                err = err.with_suggestion(format!("Did you mean one of these?\n  {}", similar.join("\n  ")));
            }

            err = err.with_suggestions([
                "TRY: casparian files   (to see discovered files)".to_string(),
                "TRY: Add a source in the UI to discover files".to_string(),
            ]);

            Err(err.into())
        }
    }
}

/// Run rule-based tagging on all untagged files
fn run_apply_rules(dry_run: bool, no_queue: bool) -> anyhow::Result<()> {
    let conn = open_db()?;

    // Load tagging rules
    let rules = load_tagging_rules(&conn)?;

    if rules.is_empty() {
        return Err(HelpfulError::new("No tagging rules defined")
            .with_context("Cannot apply rules when no rules exist")
            .with_suggestions([
                "TRY: Add tagging rules in the Casparian UI".to_string(),
                "TRY: Use 'casparian rule add' to create a rule".to_string(),
            ])
            .into());
    }

    // Load untagged files
    let files = load_untagged_files(&conn)?;
    let total_files = count_all_files(&conn)?;

    if files.is_empty() {
        println!("No untagged files to process.");
        println!();
        println!("Total files in database: {}", total_files);
        println!("All files have already been tagged.");
        return Ok(());
    }

    // Match files to rules
    let mut summary = TaggingSummary::default();
    let mut matches: Vec<(ScannedFile, TaggingRule)> = Vec::new();

    for file in &files {
        // Try each rule in priority order
        let mut matched = false;
        for rule in &rules {
            // Only apply rules from the same source
            if rule.source_id != file.source_id {
                continue;
            }

            if pattern_matches(&rule.pattern, &file.rel_path) {
                matches.push((file.clone(), rule.clone()));

                let entry = summary.matches
                    .entry(rule.pattern.clone())
                    .or_insert((rule.tag.clone(), 0, 0));
                entry.1 += 1;
                entry.2 += file.size as u64;

                matched = true;
                break; // First matching rule wins (by priority)
            }
        }

        if !matched {
            summary.untagged += 1;
        }
    }

    summary.would_queue = matches.len();
    summary.new_in_queue = matches.len(); // Simplified: all matches are "new" to queue

    // Output
    if dry_run {
        println!("DRY RUN - No changes");
        println!();
    }

    println!("Applying {} rules to {} untagged files...", rules.len(), files.len());
    println!();

    if !summary.matches.is_empty() {
        if dry_run {
            println!("WOULD TAG:");
        } else {
            println!("TAGGING:");
        }

        // Sort by file count descending
        let mut sorted_matches: Vec<_> = summary.matches.iter().collect();
        sorted_matches.sort_by(|a, b| b.1.1.cmp(&a.1.1));

        for (pattern, (tag, count, bytes)) in sorted_matches {
            println!("  {:<25} -> {:<15} {} files ({})",
                pattern,
                tag,
                count,
                format_size(*bytes)
            );
        }
        println!();
    }

    if !dry_run && !no_queue {
        println!("WOULD QUEUE: {} files ({} new)", summary.would_queue, summary.new_in_queue);
    }

    if summary.untagged > 0 {
        println!("UNTAGGED: {} files (no matching rule)", summary.untagged);
    }

    // Actually apply changes if not dry run
    if !dry_run {
        let mut applied = 0;
        for (file, rule) in &matches {
            apply_tag(&conn, file.id, &rule.tag, "rule", Some(rule.id))?;
            applied += 1;
        }

        println!();
        println!("Applied tags to {} files.", applied);

        if !no_queue {
            println!();
            println!("Files are now ready for processing.");
            println!("Use 'casparian jobs' to monitor processing status.");
        }
    } else {
        println!();
        println!("Run without --dry-run to apply changes.");
    }

    Ok(())
}

/// Execute the untag command
pub fn run_untag(args: UntagArgs) -> anyhow::Result<()> {
    run_untag_with_args(args)
}

fn run_untag_with_args(args: UntagArgs) -> anyhow::Result<()> {
    let conn = open_db()?;

    // Normalize path
    let path_str = args.path.to_string_lossy().to_string();

    // Find file in database
    let file = get_file_by_path(&conn, &path_str)?;

    match file {
        Some(f) => {
            if f.tag.is_none() {
                println!("File is not tagged: {}", f.path);
                return Ok(());
            }

            let old_tag = f.tag.clone().unwrap_or_default();
            remove_tag(&conn, f.id)?;

            println!("Untagged: {} (was: {})", f.path, old_tag);
            println!();
            println!(
                "File status reset to '{}'.",
                FileStatus::Pending.as_str()
            );
            println!("It will not be processed until tagged again.");

            Ok(())
        }
        None => {
            Err(HelpfulError::new(format!("File not found in database: {}", path_str))
                .with_context("The file must be discovered by Scout before it can be untagged")
                .with_suggestions([
                    "TRY: casparian files   (to see discovered files)".to_string(),
                    "TRY: Check the file path is correct".to_string(),
                ])
                .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const SOURCE_ID: i64 = 1;
    const RULE_CSV_ID: i64 = 2;
    const RULE_JSON_ID: i64 = 3;
    const RULE_TXT_ID: i64 = 4;

    fn create_test_db(dir: &TempDir) -> DbConnection {
        let db_path = dir.path().join("test.duckdb");
        let conn = DbConnection::open_duckdb(&db_path).unwrap();

        // Create schema
        let schema = format!(
            r#"
            CREATE TABLE scout_sources (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL,
                source_type TEXT NOT NULL,
                path TEXT NOT NULL,
                poll_interval_secs INTEGER DEFAULT 30,
                enabled INTEGER DEFAULT 1,
                created_at INTEGER,
                updated_at INTEGER
            );

            CREATE TABLE scout_tagging_rules (
                id BIGINT PRIMARY KEY,
                name TEXT,
                source_id BIGINT NOT NULL,
                pattern TEXT NOT NULL,
                tag TEXT NOT NULL,
                priority INTEGER DEFAULT 0,
                enabled INTEGER DEFAULT 1,
                created_at INTEGER,
                updated_at INTEGER
            );

            CREATE TABLE scout_files (
                id BIGINT PRIMARY KEY,
                source_id BIGINT NOT NULL,
                path TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime INTEGER,
                content_hash TEXT,
                status TEXT DEFAULT '{}',
                tag TEXT,
                tag_source TEXT,
                rule_id BIGINT,
                manual_plugin TEXT,
                error TEXT,
                first_seen_at INTEGER,
                last_seen_at INTEGER,
                processed_at INTEGER,
                sentinel_job_id INTEGER
            );
            "#,
            FileStatus::Pending.as_str()
        );
        conn.execute_batch(&schema).unwrap();

        conn
    }

    #[test]
    fn test_pattern_matches() {
        assert!(pattern_matches("*.csv", "data.csv"));
        assert!(pattern_matches("*.csv", "path/to/data.csv"));
        assert!(pattern_matches("data/*.csv", "data/file.csv"));
        assert!(pattern_matches("**/*.json", "deep/nested/file.json"));
        assert!(!pattern_matches("*.csv", "data.json"));
    }

    #[test]
    fn test_load_tagging_rules() {
        let temp_dir = TempDir::new().unwrap();
        let conn = create_test_db(&temp_dir);

        // Insert test source
        conn.execute(
            "INSERT INTO scout_sources (id, name, source_type, path) VALUES (?, ?, ?, ?)",
            &[
                DbValue::from(SOURCE_ID),
                DbValue::from("Test"),
                DbValue::from("local"),
                DbValue::from("/data"),
            ],
        )
        
        .unwrap();

        // Insert test rules
        conn.execute(
            "INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(RULE_CSV_ID),
                DbValue::from(SOURCE_ID),
                DbValue::from("*.csv"),
                DbValue::from("csv_data"),
                DbValue::from(10),
                DbValue::from(1),
            ],
        )
        
        .unwrap();
        conn.execute(
            "INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(RULE_JSON_ID),
                DbValue::from(SOURCE_ID),
                DbValue::from("*.json"),
                DbValue::from("json_data"),
                DbValue::from(5),
                DbValue::from(1),
            ],
        )
        
        .unwrap();
        conn.execute(
            "INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(RULE_TXT_ID),
                DbValue::from(SOURCE_ID),
                DbValue::from("*.txt"),
                DbValue::from("text_data"),
                DbValue::from(0),
                DbValue::from(0),
            ],
        )
        
        .unwrap();

        let rules = load_tagging_rules(&conn).unwrap();

        // Should only get enabled rules, sorted by priority descending
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "*.csv");
        assert_eq!(rules[0].priority, 10);
        assert_eq!(rules[1].pattern, "*.json");
    }

    #[test]
    fn test_apply_and_remove_tag() {
        let temp_dir = TempDir::new().unwrap();
        let conn = create_test_db(&temp_dir);

        // Insert test source and file
        conn.execute(
            "INSERT INTO scout_sources (id, name, source_type, path) VALUES (?, ?, ?, ?)",
            &[
                DbValue::from(SOURCE_ID),
                DbValue::from("Test"),
                DbValue::from("local"),
                DbValue::from("/data"),
            ],
        )
        
        .unwrap();
        conn.execute(
            "INSERT INTO scout_files (id, source_id, path, rel_path, size, status) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(1),
                DbValue::from(SOURCE_ID),
                DbValue::from("/data/test.csv"),
                DbValue::from("test.csv"),
                DbValue::from(1000),
                DbValue::from(FileStatus::Pending.as_str()),
            ],
        )
        
        .unwrap();

        // Apply tag
        apply_tag(&conn, 1, "csv_data", "manual", None)
            
            .unwrap();

        // Verify
        let file = get_file_by_path(&conn, "/data/test.csv")
            
            .unwrap()
            .unwrap();
        assert_eq!(file.tag, Some("csv_data".to_string()));
        assert_eq!(file.status, FileStatus::Tagged.as_str());

        // Remove tag
        remove_tag(&conn, 1).unwrap();

        // Verify
        let file = get_file_by_path(&conn, "/data/test.csv")
            
            .unwrap()
            .unwrap();
        assert!(file.tag.is_none());
        assert_eq!(file.status, FileStatus::Pending.as_str());
    }
}

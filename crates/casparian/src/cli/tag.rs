//! Tag command - Assign topics to files
//!
//! Two modes:
//! 1. Apply rules: `casparian tag [--dry-run] [--no-queue]`
//! 2. Manual tag: `casparian tag <path> <topic>`

use crate::cli::error::HelpfulError;
use crate::cli::output::format_size;
use crate::cli::workspace;
use casparian::scout::{patterns, Database, FileStatus, TagSource, TaggingRuleId, WorkspaceId};
use casparian_db::{DbConnection, DbValue};
use chrono::Utc;
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
}

/// A file from the database
#[derive(Debug, Clone)]
struct ScannedFile {
    id: i64,
    path: String,
    rel_path: String,
    size: i64,
    #[allow(dead_code)]
    status: String,
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
fn open_db() -> Result<Database, HelpfulError> {
    let db_path = get_db_path();

    if !db_path.exists() {
        return Err(
            HelpfulError::new(format!("Database not found: {}", db_path.display()))
                .with_context("The Scout database has not been initialized yet")
                .with_suggestions([
                    "TRY: Start the Casparian UI to initialize the database".to_string(),
                    "TRY: Run `casparian start` to initialize the system".to_string(),
                    format!("TRY: Check the path exists: {}", db_path.display()),
                ]),
        );
    }

    Database::open(&db_path).map_err(|e| {
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

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db).map_err(|e| {
        e.with_context("The workspace registry is required for tagging")
    })
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// Check if a glob pattern matches a path
fn pattern_matches(pattern: &str, path: &str) -> bool {
    patterns::matches(pattern, path).unwrap_or(false)
}

/// Load all enabled tagging rules from the database
fn load_tagging_rules(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
) -> Result<Vec<TaggingRule>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT id, pattern, tag, priority \
             FROM scout_rules \
             WHERE workspace_id = ? AND kind = 'tagging' AND enabled = 1 \
             ORDER BY priority DESC, name",
            &[DbValue::from(workspace_id.to_string())],
        )
        .map_err(|e| {
            HelpfulError::new(format!("Failed to query tagging rules: {}", e))
                .with_context("The scout_rules table may not exist")
                .with_suggestion("TRY: Ensure the database schema is up to date")
        })?;

    let mut rules = Vec::new();
    for row in rows {
        let id_raw: String = row
            .get_by_name("id")
            .map_err(|e| HelpfulError::new(format!("Failed to read rule id: {}", e)))?;
        let id = TaggingRuleId::parse(&id_raw)
            .map_err(|e| HelpfulError::new(format!("Invalid rule id: {}", e)))?;

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
        };
        rules.push(rule);
    }

    Ok(rules)
}

/// Load untagged files from the database
fn load_untagged_files(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
) -> Result<Vec<ScannedFile>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT f.id, f.path, f.rel_path, f.size, f.status \
             FROM scout_files f \
             LEFT JOIN scout_file_tags t \
                ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
             WHERE f.workspace_id = ? AND t.file_id IS NULL AND f.status = ? \
             ORDER BY f.path",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(FileStatus::Pending.as_str()),
            ],
        )
        .map_err(|e| {
            HelpfulError::new(format!("Failed to query files: {}", e))
                .with_context("The scout_files table may not exist")
                .with_suggestion("TRY: Run a scan first with `casparian scan`")
        })?;

    let mut files = Vec::new();
    for row in rows {
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
            status: row
                .get_by_name("status")
                .map_err(|e| HelpfulError::new(format!("Failed to read file status: {}", e)))?,
        };
        files.push(file);
    }

    Ok(files)
}

/// Get file by path
fn get_file_by_path(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    path: &str,
) -> Result<Option<ScannedFile>, HelpfulError> {
    let row = conn
        .query_optional(
            "SELECT id, path, rel_path, size, status \
             FROM scout_files \
             WHERE workspace_id = ? AND path = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(path),
            ],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query file: {}", e)))?;

    let row = match row {
        Some(row) => row,
        None => return Ok(None),
    };

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
        status: row
            .get_by_name("status")
            .map_err(|e| HelpfulError::new(format!("Failed to read file status: {}", e)))?,
    };

    Ok(Some(file))
}

/// Apply tag to a file in the database
fn apply_tag(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    file_id: i64,
    tag: &str,
    rule_id: Option<TaggingRuleId>,
    tag_source: TagSource,
) -> Result<(), HelpfulError> {
    let rule_id_value = rule_id
        .as_ref()
        .map(|id| DbValue::from(id.to_string()))
        .unwrap_or(DbValue::Null);
    conn.execute(
        "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT (workspace_id, file_id, tag) DO UPDATE SET \
            tag_source = excluded.tag_source, \
            rule_id = excluded.rule_id, \
            created_at = excluded.created_at",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from(file_id),
            DbValue::from(tag),
            DbValue::from(tag_source.as_str()),
            rule_id_value,
            DbValue::from(now_millis()),
        ],
    )
    .map_err(|e| {
        HelpfulError::new(format!("Failed to update file tag: {}", e))
            .with_context(format!("File ID: {}", file_id))
    })?;

    let updated = conn.execute(
        "UPDATE scout_files SET status = ? WHERE id = ?",
        &[
            DbValue::from(FileStatus::Tagged.as_str()),
            DbValue::from(file_id),
        ],
    )
    .map_err(|e| {
        HelpfulError::new(format!("Failed to update file status: {}", e))
            .with_context(format!("File ID: {}", file_id))
    })?;

    if updated == 0 {
        return Err(HelpfulError::new("No file updated")
            .with_context(format!("File ID: {}", file_id))
            .with_suggestion("TRY: Re-scan the source to refresh file IDs".to_string()));
    }

    Ok(())
}

fn list_file_tags(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    file_id: i64,
) -> Result<Vec<String>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT tag FROM scout_file_tags WHERE workspace_id = ? AND file_id = ? ORDER BY tag",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(file_id),
            ],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query file tags: {}", e)))?;

    let mut tags = Vec::new();
    for row in rows {
        let tag: String = row
            .get_by_name("tag")
            .map_err(|e| HelpfulError::new(format!("Failed to read tag: {}", e)))?;
        tags.push(tag);
    }
    Ok(tags)
}

/// Remove all tags from a file in the database
fn remove_all_tags(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    file_id: i64,
) -> Result<u64, HelpfulError> {
    let result = conn
        .execute(
            "DELETE FROM scout_file_tags WHERE workspace_id = ? AND file_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(file_id),
            ],
        )
        .map_err(|e| {
            HelpfulError::new(format!("Failed to remove file tags: {}", e))
                .with_context(format!("File ID: {}", file_id))
        })?;

    Ok(result as u64)
}

/// Reset file status after untagging
fn reset_file_status(conn: &DbConnection, file_id: i64) -> Result<(), HelpfulError> {
    let updated = conn.execute(
        "UPDATE scout_files \
         SET status = ?, sentinel_job_id = NULL, manual_plugin = NULL \
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

    if updated == 0 {
        return Err(HelpfulError::new("No file updated")
            .with_context(format!("File ID: {}", file_id))
            .with_suggestion("TRY: Re-scan the source to refresh file IDs".to_string()));
    }

    Ok(())
}

/// Count total files
fn count_all_files(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
) -> Result<i64, HelpfulError> {
    conn.query_scalar::<i64>(
        "SELECT COUNT(*) FROM scout_files WHERE workspace_id = ?",
        &[DbValue::from(workspace_id.to_string())],
    )
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
        (Some(_), None) => Err(HelpfulError::new("Missing topic for manual tagging")
            .with_context("When tagging a specific file, you must provide both path and topic")
            .with_suggestions([
                "TRY: casparian tag /path/to/file.csv my_topic".to_string(),
                "TRY: casparian tag --dry-run   (to apply rules)".to_string(),
            ])
            .into()),
        (None, Some(_)) => Err(HelpfulError::new("Missing path for manual tagging")
            .with_context("When tagging with a specific topic, you must provide the file path")
            .with_suggestions([
                "TRY: casparian tag /path/to/file.csv my_topic".to_string(),
                "TRY: casparian tag   (to apply rules to all files)".to_string(),
            ])
            .into()),
    }
}

/// Run manual tagging of a single file
fn run_manual_tag(path: &PathBuf, topic: &str) -> anyhow::Result<()> {
    let db = open_db()?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    // Normalize path
    let path_str = path.to_string_lossy().to_string();

    // Find file in database
    let file = get_file_by_path(conn, &workspace_id, &path_str)?;

    match file {
        Some(f) => {
            apply_tag(conn, &workspace_id, f.id, topic, None, TagSource::Manual)?;
            println!("Tagged: {} -> {}", f.path, topic);
            println!();
            println!(
                "File will be processed by plugins subscribed to topic '{}'",
                topic
            );
            Ok(())
        }
        None => {
            // Try to find by relative path or partial match
            let similar_rows = conn
                .query_all(
                    "SELECT path FROM scout_files WHERE workspace_id = ? AND (path LIKE ? OR rel_path LIKE ?) LIMIT 5",
                    &[
                        DbValue::from(workspace_id.to_string()),
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
                err = err.with_suggestion(format!(
                    "Did you mean one of these?\n  {}",
                    similar.join("\n  ")
                ));
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
    let db = open_db()?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    // Load tagging rules
    let rules = load_tagging_rules(conn, &workspace_id)?;

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
    let files = load_untagged_files(conn, &workspace_id)?;
    let total_files = count_all_files(conn, &workspace_id)?;

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
            if pattern_matches(&rule.pattern, &file.rel_path) {
                matches.push((file.clone(), rule.clone()));

                let entry =
                    summary
                        .matches
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

    println!(
        "Applying {} rules to {} untagged files...",
        rules.len(),
        files.len()
    );
    println!();

    if !summary.matches.is_empty() {
        if dry_run {
            println!("WOULD TAG:");
        } else {
            println!("TAGGING:");
        }

        // Sort by file count descending
        let mut sorted_matches: Vec<_> = summary.matches.iter().collect();
        sorted_matches.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

        for (pattern, (tag, count, bytes)) in sorted_matches {
            println!(
                "  {:<25} -> {:<15} {} files ({})",
                pattern,
                tag,
                count,
                format_size(*bytes)
            );
        }
        println!();
    }

    if !dry_run && !no_queue {
        println!(
            "WOULD QUEUE: {} files ({} new)",
            summary.would_queue, summary.new_in_queue
        );
    }

    if summary.untagged > 0 {
        println!("UNTAGGED: {} files (no matching rule)", summary.untagged);
    }

    // Actually apply changes if not dry run
    if !dry_run {
        let mut applied = 0;
        for (file, rule) in &matches {
            apply_tag(
                conn,
                &workspace_id,
                file.id,
                &rule.tag,
                Some(rule.id),
                TagSource::Rule,
            )?;
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
    let db = open_db()?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    // Normalize path
    let path_str = args.path.to_string_lossy().to_string();

    // Find file in database
    let file = get_file_by_path(conn, &workspace_id, &path_str)?;

    match file {
        Some(f) => {
            let tags = list_file_tags(conn, &workspace_id, f.id)?;
            if tags.is_empty() {
                println!("File has no tags: {}", f.path);
                return Ok(());
            }

            remove_all_tags(conn, &workspace_id, f.id)?;
            reset_file_status(conn, f.id)?;

            println!("Untagged: {} (removed: {})", f.path, tags.join(", "));
            println!();
            println!("File status reset to '{}'.", FileStatus::Pending.as_str());
            println!("It will not be processed until tagged again.");

            Ok(())
        }
        None => Err(
            HelpfulError::new(format!("File not found in database: {}", path_str))
                .with_context("The file must be discovered by Scout before it can be untagged")
                .with_suggestions([
                    "TRY: casparian files   (to see discovered files)".to_string(),
                    "TRY: Check the file path is correct".to_string(),
                ])
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const SOURCE_ID: i64 = 1;

    fn create_test_db(dir: &TempDir) -> (DbConnection, WorkspaceId) {
        let db_path = dir.path().join("test.duckdb");
        let conn = DbConnection::open_duckdb(&db_path).unwrap();
        let workspace_id = WorkspaceId::new();

        // Create schema
        let schema = format!(
            r#"
            CREATE TABLE cf_workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at BIGINT NOT NULL
            );

            CREATE TABLE scout_sources (
                id BIGINT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                name TEXT NOT NULL,
                source_type TEXT NOT NULL,
                path TEXT NOT NULL,
                poll_interval_secs BIGINT DEFAULT 30,
                enabled BIGINT DEFAULT 1,
                created_at BIGINT,
                updated_at BIGINT
            );

            CREATE TABLE scout_rules (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'tagging',
                pattern TEXT NOT NULL,
                tag TEXT NOT NULL,
                priority BIGINT DEFAULT 0,
                enabled BIGINT DEFAULT 1,
                created_at BIGINT,
                updated_at BIGINT
            );

            CREATE TABLE scout_files (
                id BIGINT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                source_id BIGINT NOT NULL,
                path TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                size BIGINT NOT NULL,
                mtime BIGINT,
                content_hash TEXT,
                status TEXT DEFAULT '{}',
                manual_plugin TEXT,
                error TEXT,
                first_seen_at BIGINT,
                last_seen_at BIGINT,
                processed_at BIGINT,
                sentinel_job_id BIGINT
            );

            CREATE TABLE scout_file_tags (
                workspace_id TEXT NOT NULL,
                file_id BIGINT NOT NULL,
                tag TEXT NOT NULL,
                tag_source TEXT NOT NULL,
                rule_id TEXT,
                created_at BIGINT NOT NULL,
                PRIMARY KEY (workspace_id, file_id, tag)
            );
            "#,
            FileStatus::Pending.as_str()
        );
        conn.execute_batch(&schema).unwrap();

        conn.execute(
            "INSERT INTO cf_workspaces (id, name, created_at) VALUES (?, ?, ?)",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from("Default"),
                DbValue::from(now_millis()),
            ],
        )
        .unwrap();

        (conn, workspace_id)
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
        let (conn, workspace_id) = create_test_db(&temp_dir);

        // Insert test source
        conn.execute(
            "INSERT INTO scout_sources (id, workspace_id, name, source_type, path) VALUES (?, ?, ?, ?, ?)",
            &[
                DbValue::from(SOURCE_ID),
                DbValue::from(workspace_id.to_string()),
                DbValue::from("Test"),
                DbValue::from("local"),
                DbValue::from("/data"),
            ],
        )
        .unwrap();

        // Insert test rules
        conn.execute(
            "INSERT INTO scout_rules (id, workspace_id, name, kind, pattern, tag, priority, enabled) VALUES (?, ?, ?, 'tagging', ?, ?, ?, ?)",
            &[
                DbValue::from(TaggingRuleId::new().to_string()),
                DbValue::from(workspace_id.to_string()),
                DbValue::from("csv"),
                DbValue::from("*.csv"),
                DbValue::from("csv_data"),
                DbValue::from(10),
                DbValue::from(1),
            ],
        )

        .unwrap();
        conn.execute(
            "INSERT INTO scout_rules (id, workspace_id, name, kind, pattern, tag, priority, enabled) VALUES (?, ?, ?, 'tagging', ?, ?, ?, ?)",
            &[
                DbValue::from(TaggingRuleId::new().to_string()),
                DbValue::from(workspace_id.to_string()),
                DbValue::from("json"),
                DbValue::from("*.json"),
                DbValue::from("json_data"),
                DbValue::from(5),
                DbValue::from(1),
            ],
        )

        .unwrap();
        conn.execute(
            "INSERT INTO scout_rules (id, workspace_id, name, kind, pattern, tag, priority, enabled) VALUES (?, ?, ?, 'tagging', ?, ?, ?, ?)",
            &[
                DbValue::from(TaggingRuleId::new().to_string()),
                DbValue::from(workspace_id.to_string()),
                DbValue::from("txt"),
                DbValue::from("*.txt"),
                DbValue::from("text_data"),
                DbValue::from(0),
                DbValue::from(0),
            ],
        )

        .unwrap();

        let rules = load_tagging_rules(&conn, &workspace_id).unwrap();

        // Should only get enabled rules, sorted by priority descending
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "*.csv");
        assert_eq!(rules[0].priority, 10);
        assert_eq!(rules[1].pattern, "*.json");
    }

    #[test]
    fn test_apply_and_remove_tag() {
        let temp_dir = TempDir::new().unwrap();
        let (conn, workspace_id) = create_test_db(&temp_dir);

        // Insert test source and file
        conn.execute(
            "INSERT INTO scout_sources (id, workspace_id, name, source_type, path) VALUES (?, ?, ?, ?, ?)",
            &[
                DbValue::from(SOURCE_ID),
                DbValue::from(workspace_id.to_string()),
                DbValue::from("Test"),
                DbValue::from("local"),
                DbValue::from("/data"),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scout_files (id, workspace_id, source_id, path, rel_path, size, status) VALUES (?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(1),
                DbValue::from(workspace_id.to_string()),
                DbValue::from(SOURCE_ID),
                DbValue::from("/data/test.csv"),
                DbValue::from("test.csv"),
                DbValue::from(1000),
                DbValue::from(FileStatus::Pending.as_str()),
            ],
        )

        .unwrap();

        // Apply tag
        apply_tag(
            &conn,
            &workspace_id,
            1,
            "csv_data",
            None,
            TagSource::Manual,
        )
        .unwrap();

        // Verify
        let file = get_file_by_path(&conn, &workspace_id, "/data/test.csv")
            .unwrap()
            .unwrap();
        let tags = list_file_tags(&conn, &workspace_id, file.id).unwrap();
        assert_eq!(tags, vec!["csv_data".to_string()]);

        // Remove tag
        remove_all_tags(&conn, &workspace_id, 1).unwrap();
        reset_file_status(&conn, 1).unwrap();

        // Verify
        let file = get_file_by_path(&conn, &workspace_id, "/data/test.csv")
            .unwrap()
            .unwrap();
        let tags = list_file_tags(&conn, &workspace_id, file.id).unwrap();
        assert!(tags.is_empty());
        assert_eq!(file.status, FileStatus::Pending.as_str());
    }
}

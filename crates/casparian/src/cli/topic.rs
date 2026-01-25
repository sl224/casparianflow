//! Topic command - Manage topics
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table};
use crate::cli::workspace;
use casparian::scout::{Database, FileStatus, TaggingRuleId, WorkspaceId};
use casparian_db::DbValue;
use clap::Subcommand;
use std::collections::HashMap;
use std::path::PathBuf;

/// Subcommands for topic management
#[derive(Subcommand, Debug, Clone)]
pub enum TopicAction {
    /// List all topics
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a new topic
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Show topic details
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete a topic
    Delete {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// List files for a topic
    Files {
        name: String,
        #[arg(long, default_value = "50")]
        limit: usize,
    },
}

/// Topic statistics
#[derive(Debug, Default, Clone)]
struct TopicStats {
    total: u64,
    processed: u64,
    pending: u64,
    failed: u64,
    total_size: u64,
}

#[derive(Debug, Clone)]
struct TopicFile {
    id: i64,
    path: String,
    rel_path: String,
    size: u64,
    status: FileStatus,
    error: Option<String>,
}

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db)
        .map_err(|e| e.with_context("The workspace registry is required for topics"))
}

/// Get all topics and their statistics from the database
fn get_topic_stats(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
) -> Result<HashMap<String, TopicStats>, HelpfulError> {
    let mut stats: HashMap<String, TopicStats> = HashMap::new();

    let pending = [
        FileStatus::Pending.as_str(),
        FileStatus::Tagged.as_str(),
        FileStatus::Queued.as_str(),
        FileStatus::Processing.as_str(),
    ];

    let rows = conn
        .query_all(
            &format!(
                "SELECT t.tag AS tag, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN f.status = '{processed}' THEN 1 ELSE 0 END) AS processed, \
                        SUM(CASE WHEN f.status IN ('{pending}', '{tagged}', '{queued}', '{processing}') THEN 1 ELSE 0 END) AS pending, \
                        SUM(CASE WHEN f.status = '{failed}' THEN 1 ELSE 0 END) AS failed, \
                        SUM(f.size) AS total_size \
                 FROM scout_file_tags t \
                 JOIN scout_files f ON f.id = t.file_id \
                 WHERE t.workspace_id = ? \
                 GROUP BY t.tag",
                processed = FileStatus::Processed.as_str(),
                pending = pending[0],
                tagged = pending[1],
                queued = pending[2],
                processing = pending[3],
                failed = FileStatus::Failed.as_str(),
            ),
            &[DbValue::from(workspace_id.to_string())],
        )
        .map_err(|e| {
            HelpfulError::new(format!("Failed to query topic stats: {}", e))
                .with_context("The scout_file_tags table may not exist")
        })?;

    for row in rows {
        let tag: String = row
            .get_by_name("tag")
            .map_err(|e| HelpfulError::new(format!("Failed to read tag: {}", e)))?;
        let entry = stats.entry(tag).or_default();
        let total: i64 = row
            .get_by_name("total")
            .map_err(|e| HelpfulError::new(format!("Failed to read total: {}", e)))?;
        let processed: i64 = row
            .get_by_name::<Option<i64>>("processed")
            .map_err(|e| HelpfulError::new(format!("Failed to read processed: {}", e)))?
            .unwrap_or(0);
        let pending: i64 = row
            .get_by_name::<Option<i64>>("pending")
            .map_err(|e| HelpfulError::new(format!("Failed to read pending: {}", e)))?
            .unwrap_or(0);
        let failed: i64 = row
            .get_by_name::<Option<i64>>("failed")
            .map_err(|e| HelpfulError::new(format!("Failed to read failed: {}", e)))?
            .unwrap_or(0);
        let total_size: i64 = row
            .get_by_name::<Option<i64>>("total_size")
            .map_err(|e| HelpfulError::new(format!("Failed to read total_size: {}", e)))?
            .unwrap_or(0);
        entry.total = total as u64;
        entry.processed = processed as u64;
        entry.pending = pending as u64;
        entry.failed = failed as u64;
        entry.total_size = total_size as u64;
    }

    // Untagged files
    let untagged_row = conn
        .query_optional(
            &format!(
                "SELECT COUNT(*) AS total, \
                        SUM(CASE WHEN f.status = '{processed}' THEN 1 ELSE 0 END) AS processed, \
                        SUM(CASE WHEN f.status IN ('{pending}', '{tagged}', '{queued}', '{processing}') THEN 1 ELSE 0 END) AS pending, \
                        SUM(CASE WHEN f.status = '{failed}' THEN 1 ELSE 0 END) AS failed, \
                        SUM(f.size) AS total_size \
                 FROM scout_files f \
                 LEFT JOIN scout_file_tags t \
                    ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
                 WHERE f.workspace_id = ? AND t.file_id IS NULL",
                processed = FileStatus::Processed.as_str(),
                pending = pending[0],
                tagged = pending[1],
                queued = pending[2],
                processing = pending[3],
                failed = FileStatus::Failed.as_str(),
            ),
            &[DbValue::from(workspace_id.to_string())],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query untagged stats: {}", e)))?;

    if let Some(row) = untagged_row {
        let total: i64 = row
            .get_by_name("total")
            .map_err(|e| HelpfulError::new(format!("Failed to read untagged total: {}", e)))?;
        if total > 0 {
            let entry = stats.entry("(untagged)".to_string()).or_default();
            let processed: i64 = row
                .get_by_name::<Option<i64>>("processed")
                .map_err(|e| HelpfulError::new(format!("Failed to read processed: {}", e)))?
                .unwrap_or(0);
            let pending: i64 = row
                .get_by_name::<Option<i64>>("pending")
                .map_err(|e| HelpfulError::new(format!("Failed to read pending: {}", e)))?
                .unwrap_or(0);
            let failed: i64 = row
                .get_by_name::<Option<i64>>("failed")
                .map_err(|e| HelpfulError::new(format!("Failed to read failed: {}", e)))?
                .unwrap_or(0);
            let total_size: i64 = row
                .get_by_name::<Option<i64>>("total_size")
                .map_err(|e| HelpfulError::new(format!("Failed to read total_size: {}", e)))?
                .unwrap_or(0);
            entry.total = total as u64;
            entry.processed = processed as u64;
            entry.pending = pending as u64;
            entry.failed = failed as u64;
            entry.total_size = total_size as u64;
        }
    }

    Ok(stats)
}

fn list_files_for_tag(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    tag: &str,
    limit: usize,
) -> Result<Vec<TopicFile>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT f.id, f.path, f.rel_path, f.size, f.status, f.error \
             FROM scout_files f \
             JOIN scout_file_tags t \
                ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
             WHERE f.workspace_id = ? AND t.tag = ? \
             ORDER BY f.mtime DESC \
             LIMIT ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(tag),
                DbValue::from(limit as i64),
            ],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;

    rows.into_iter()
        .map(|row| {
            let status_raw: String = row
                .get_by_name("status")
                .map_err(|e| HelpfulError::new(format!("Failed to read status: {}", e)))?;
            let status = FileStatus::parse(&status_raw).ok_or_else(|| {
                HelpfulError::new(format!("Invalid status in database: {}", status_raw))
            })?;
            Ok(TopicFile {
                id: row
                    .get_by_name("id")
                    .map_err(|e| HelpfulError::new(format!("Failed to read id: {}", e)))?,
                path: row
                    .get_by_name("path")
                    .map_err(|e| HelpfulError::new(format!("Failed to read path: {}", e)))?,
                rel_path: row
                    .get_by_name("rel_path")
                    .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
                size: {
                    let size_raw: i64 = row
                        .get_by_name("size")
                        .map_err(|e| HelpfulError::new(format!("Failed to read size: {}", e)))?;
                    size_raw as u64
                },
                status,
                error: row
                    .get_by_name("error")
                    .map_err(|e| HelpfulError::new(format!("Failed to read error: {}", e)))?,
            })
        })
        .collect()
}

fn list_untagged_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    limit: usize,
) -> Result<Vec<TopicFile>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT f.id, f.path, f.rel_path, f.size, f.status, f.error \
             FROM scout_files f \
             LEFT JOIN scout_file_tags t \
                ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
             WHERE f.workspace_id = ? AND t.file_id IS NULL \
             ORDER BY f.mtime DESC \
             LIMIT ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(limit as i64),
            ],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;

    rows.into_iter()
        .map(|row| {
            let status_raw: String = row
                .get_by_name("status")
                .map_err(|e| HelpfulError::new(format!("Failed to read status: {}", e)))?;
            let status = FileStatus::parse(&status_raw).ok_or_else(|| {
                HelpfulError::new(format!("Invalid status in database: {}", status_raw))
            })?;
            Ok(TopicFile {
                id: row
                    .get_by_name("id")
                    .map_err(|e| HelpfulError::new(format!("Failed to read id: {}", e)))?,
                path: row
                    .get_by_name("path")
                    .map_err(|e| HelpfulError::new(format!("Failed to read path: {}", e)))?,
                rel_path: row
                    .get_by_name("rel_path")
                    .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
                size: {
                    let size_raw: i64 = row
                        .get_by_name("size")
                        .map_err(|e| HelpfulError::new(format!("Failed to read size: {}", e)))?;
                    size_raw as u64
                },
                status,
                error: row
                    .get_by_name("error")
                    .map_err(|e| HelpfulError::new(format!("Failed to read error: {}", e)))?,
            })
        })
        .collect()
}

fn get_rule_match_counts(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    tag: &str,
) -> Result<HashMap<TaggingRuleId, u64>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT rule_id, COUNT(*) AS matched \
             FROM scout_file_tags \
             WHERE workspace_id = ? AND tag = ? AND rule_id IS NOT NULL \
             GROUP BY rule_id",
            &[DbValue::from(workspace_id.to_string()), DbValue::from(tag)],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to query rule matches: {}", e)))?;

    let mut counts = HashMap::new();
    for row in rows {
        let rule_id_raw: String = row
            .get_by_name("rule_id")
            .map_err(|e| HelpfulError::new(format!("Failed to read rule_id: {}", e)))?;
        let rule_id = TaggingRuleId::parse(&rule_id_raw)
            .map_err(|e| HelpfulError::new(format!("Invalid rule_id: {}", e)))?;
        let matched_raw: i64 = row
            .get_by_name("matched")
            .map_err(|e| HelpfulError::new(format!("Failed to read matched: {}", e)))?;
        counts.insert(rule_id, matched_raw as u64);
    }
    Ok(counts)
}

/// Execute the topic command
pub fn run(action: TopicAction) -> anyhow::Result<()> {
    run_with_action(action)
}

fn run_with_action(action: TopicAction) -> anyhow::Result<()> {
    let db_path = active_db_path();
    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    match action {
        TopicAction::List { json } => list_topics(conn, &workspace_id, json),
        TopicAction::Create {
            name,
            description: _,
        } => create_topic(&db, conn, &workspace_id, &name),
        TopicAction::Show { name, json } => show_topic(&db, conn, &workspace_id, &name, json),
        TopicAction::Delete { name, force } => delete_topic(&db, conn, &workspace_id, &name, force),
        TopicAction::Files { name, limit } => list_topic_files(conn, &workspace_id, &name, limit),
    }
}

fn list_topics(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    json: bool,
) -> anyhow::Result<()> {
    let stats = get_topic_stats(conn, workspace_id)?;

    if stats.is_empty() {
        println!("No topics found.");
        println!();
        println!("Topics are created automatically when files are tagged.");
        println!("Add a tagging rule with:");
        println!("  casparian rule add '*.csv' --topic csv_data");
        return Ok(());
    }

    if json {
        let output: Vec<serde_json::Value> = stats
            .iter()
            .map(|(topic, s)| {
                serde_json::json!({
                    "topic": topic,
                    "files": s.total,
                    "processed": s.processed,
                    "pending": s.pending,
                    "failed": s.failed,
                    "size": s.total_size,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("TOPICS");

    // Sort topics by file count
    let mut topics: Vec<_> = stats.iter().collect();
    topics.sort_by(|a, b| b.1.total.cmp(&a.1.total));

    let rows: Vec<Vec<String>> = topics
        .iter()
        .map(|(topic, s)| {
            vec![
                topic.to_string(),
                format!("{}", s.total),
                format!("{}", s.processed),
                format!("{}", s.pending),
                format!("{}", s.failed),
            ]
        })
        .collect();

    print_table(&["TOPIC", "FILES", "PROCESSED", "PENDING", "FAILED"], rows);

    Ok(())
}

fn create_topic(
    db: &Database,
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    name: &str,
) -> anyhow::Result<()> {
    // Topics are implicit - they exist when files are tagged with them
    // But we can create a rule that uses this topic to make it "real"

    // Check if topic already has files
    let stats = get_topic_stats(conn, workspace_id)?;
    if stats.contains_key(name) {
        return Err(HelpfulError::new(format!("Topic already exists: {}", name))
            .with_context(format!(
                "{} files tagged with this topic",
                stats[name].total
            ))
            .with_suggestion("TRY: Use 'casparian topic show' to see topic details")
            .into());
    }

    // Check if there's already a rule for this topic
    let rules = db.list_tagging_rules(workspace_id)?;
    let has_rule = rules.iter().any(|r| r.tag == name);

    if has_rule {
        println!("Topic '{}' already has rules configured.", name);
        println!("Files will be tagged when they match the rule patterns.");
    } else {
        println!("Created topic '{}'", name);
        println!();
        println!("This is an empty topic (no files tagged yet).");
        println!("Add files by:");
        println!(
            "  1. Creating a rule: casparian rule add '*.csv' --topic {}",
            name
        );
        println!(
            "  2. Manual tagging: casparian tag /path/to/file.csv --topic {}",
            name
        );
    }

    Ok(())
}

fn show_topic(
    db: &Database,
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    name: &str,
    json: bool,
) -> anyhow::Result<()> {
    let stats = get_topic_stats(conn, workspace_id)?;
    let topic_stats = stats.get(name);

    if topic_stats.is_none() && name != "(untagged)" {
        // Check if there's a rule for this topic
        let rules = db.list_tagging_rules(workspace_id)?;
        let has_rule = rules.iter().any(|r| r.tag == name);

        if !has_rule {
            return Err(HelpfulError::new(format!("Topic not found: {}", name))
                .with_suggestion("TRY: Use 'casparian topic ls' to see available topics")
                .with_suggestion("TRY: Create a rule for this topic with 'casparian rule add'")
                .into());
        }
    }

    // Get rules that produce this topic
    let rules = db.list_tagging_rules(workspace_id)?;
    let topic_rules: Vec<_> = rules.iter().filter(|r| r.tag == name).collect();

    // Get files with this topic
    let files = if name == "(untagged)" {
        list_untagged_files(conn, workspace_id, 1000)?
    } else {
        list_files_for_tag(conn, workspace_id, name, 1000)?
    };

    let parser: Option<(String, Option<i64>)> = None;

    // Get recent failures
    let failed_files: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Failed)
        .take(5)
        .collect();

    if json {
        let output = serde_json::json!({
            "topic": name,
            "rules": topic_rules.iter().map(|r| {
                serde_json::json!({
                    "pattern": r.pattern,
                    "priority": r.priority,
                })
            }).collect::<Vec<_>>(),
            "parser": parser.as_ref().map(|(n, _)| n),
            "files": {
                "total": files.len(),
                "processed": files.iter().filter(|f| f.status == FileStatus::Processed).count(),
                "pending": files.iter().filter(|f| matches!(f.status, FileStatus::Pending | FileStatus::Tagged | FileStatus::Queued | FileStatus::Processing)).count(),
                "failed": files.iter().filter(|f| f.status == FileStatus::Failed).count(),
            },
            "failures": failed_files.iter().map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "error": f.error,
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("TOPIC: {}", name);
    println!();

    // Rules section
    println!("RULES");
    if topic_rules.is_empty() {
        println!("  (no rules configured)");
    } else {
        let rule_matches = get_rule_match_counts(conn, workspace_id, name)?;
        for rule in &topic_rules {
            let matched = rule_matches.get(&rule.id).copied().unwrap_or(0);
            println!("  {}     {} files matched", rule.pattern, matched);
        }
    }
    println!();

    // Parser section
    println!("PARSER");
    println!("  (parser subscriptions are not tracked in v1)");
    println!("  TRY: casparian publish <file.py> --version <v>");
    println!();

    // Files section
    let stats = topic_stats.cloned().unwrap_or_default();
    println!("FILES");
    println!("  Total:      {}", stats.total);
    println!("  Processed:  {}", stats.processed);
    println!("  Pending:    {}", stats.pending);
    println!("  Failed:     {}", stats.failed);
    println!();

    // Recent failures
    if !failed_files.is_empty() {
        println!("RECENT FAILURES");
        for file in &failed_files {
            let error = file.error.as_deref().unwrap_or("unknown error");
            // Truncate error to first 50 chars
            let error_short = if error.len() > 50 {
                format!("{}...", &error[..50])
            } else {
                error.to_string()
            };
            println!("  {}    {}", file.rel_path, error_short);
        }
        println!();
    }

    // Output section (if we have processed files)
    if stats.processed > 0 {
        let output_dir = dirs::home_dir()
            .map(|h| h.join(".casparian_flow").join("output").join(name))
            .unwrap_or_else(|| PathBuf::from("output").join(name));

        println!("OUTPUT");
        println!("  {}", output_dir.display());
        if output_dir.exists() {
            // Count parquet files
            let parquet_count = std::fs::read_dir(&output_dir)
                .map(|entries| {
                    entries
                        .filter(|e| {
                            e.as_ref()
                                .map(|e| {
                                    e.path()
                                        .extension()
                                        .map(|ext| ext == "parquet")
                                        .unwrap_or(false)
                                })
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0);
            let total_size: u64 = std::fs::read_dir(&output_dir)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| e.metadata().ok())
                        .map(|m| m.len())
                        .sum()
                })
                .unwrap_or(0);
            println!(
                "  {} parquet files ({})",
                parquet_count,
                format_size(total_size)
            );
        }
        println!();
    }

    // Commands section
    println!("COMMANDS");
    println!(
        "  casparian files --topic {}          # List all files",
        name
    );
    if stats.failed > 0 {
        println!(
            "  casparian files --topic {} --failed # List failures",
            name
        );
    }
    Ok(())
}

fn delete_topic(
    db: &Database,
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    name: &str,
    force: bool,
) -> anyhow::Result<()> {
    let stats = get_topic_stats(conn, workspace_id)?;
    let topic_stats = stats.get(name);

    if topic_stats.is_none() {
        // Check rules
        let rules = db.list_tagging_rules(workspace_id)?;
        let topic_rules: Vec<_> = rules.iter().filter(|r| r.tag == name).collect();

        if topic_rules.is_empty() {
            return Err(HelpfulError::new(format!("Topic not found: {}", name))
                .with_suggestion("TRY: Use 'casparian topic ls' to see available topics")
                .into());
        }

        // Delete rules
        for rule in topic_rules {
            db.delete_tagging_rule(&rule.id).map_err(|e| {
                HelpfulError::new(format!("Failed to delete tagging rule: {}", e))
                    .with_context(format!("Rule ID: {}", rule.id))
            })?;
        }
        println!("Removed rules for topic '{}'", name);
        return Ok(());
    }

    let stats = topic_stats.unwrap();

    if stats.total > 0 && !force {
        return Err(
            HelpfulError::new(format!("Topic '{}' has {} files", name, stats.total))
                .with_context("Deleting this topic will untag all files")
                .with_suggestion("TRY: Use --force to remove anyway")
                .into(),
        );
    }

    // Delete rules for this topic
    let rules = db.list_tagging_rules(workspace_id)?;
    for rule in rules.iter().filter(|r| r.tag == name) {
        db.delete_tagging_rule(&rule.id).map_err(|e| {
            HelpfulError::new(format!("Failed to delete tagging rule: {}", e))
                .with_context(format!("Rule ID: {}", rule.id))
        })?;
    }

    // Untag files (set tag to NULL, status to pending)
    let files = db
        .list_files_by_tag(workspace_id, name, 100000)
        .map_err(|e| {
            HelpfulError::new(format!("Failed to list files for topic: {}", e))
                .with_context(format!("Topic: {}", name))
        })?;
    for file in files {
        if let Some(id) = file.id {
            db.untag_file(id).map_err(|e| {
                HelpfulError::new(format!("Failed to untag file: {}", e))
                    .with_context(format!("File ID: {}", id))
            })?;
        }
    }

    println!("Removed topic '{}'", name);
    println!("  {} files untagged", stats.total);

    Ok(())
}

fn list_topic_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    name: &str,
    limit: usize,
) -> anyhow::Result<()> {
    let files = if name == "(untagged)" {
        list_untagged_files(conn, workspace_id, limit)?
    } else {
        list_files_for_tag(conn, workspace_id, name, limit)?
    };

    if files.is_empty() {
        println!("No files tagged with '{}'", name);
        return Ok(());
    }

    println!("FILES FOR TOPIC: {} ({} shown)", name, files.len());
    println!();

    let rows: Vec<Vec<String>> = files
        .iter()
        .map(|f| {
            vec![
                f.rel_path.clone(),
                format_size(f.size),
                f.status.as_str().to_string(),
            ]
        })
        .collect();

    print_table(&["PATH", "SIZE", "STATUS"], rows);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian::scout::{ScannedFile, Source, SourceId, SourceType, TaggingRule, TaggingRuleId};
    use chrono::Utc;

    #[test]
    fn test_get_topic_stats() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Add files with different topics
        for (i, (name, topic)) in [
            ("test1.csv", "sales"),
            ("test2.csv", "sales"),
            ("data.json", "invoices"),
        ]
        .iter()
        .enumerate()
        {
            let path = format!("/data/{}", name);
            let file_uid = casparian::scout::file_uid::weak_uid_from_path_str(&path);
            let file = ScannedFile::new(
                workspace_id,
                source_id.clone(),
                &file_uid,
                &path,
                name,
                1000,
                12345 + i as i64,
            );
            let result = db.upsert_file(&file).unwrap();
            db.conn()
                .execute(
                    "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at) VALUES (?, ?, ?, 'manual', NULL, ?)",
                    &[
                        DbValue::from(workspace_id.to_string()),
                        DbValue::from(result.id),
                        DbValue::from(*topic),
                        DbValue::from(Utc::now().timestamp_millis()),
                    ],
                )
                .unwrap();
        }

        let stats = get_topic_stats(db.conn(), &workspace_id).unwrap();
        assert_eq!(stats.get("sales").map(|s| s.total), Some(2));
        assert_eq!(stats.get("invoices").map(|s| s.total), Some(1));
    }

    #[test]
    fn test_topic_with_rules() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let rule = TaggingRule {
            id: TaggingRuleId::new(),
            name: "CSV Files".to_string(),
            workspace_id,
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };
        db.upsert_tagging_rule(&rule).unwrap();

        // Check that we can find rules for the topic
        let rules = db.list_tagging_rules(&workspace_id).unwrap();
        let csv_rules: Vec<_> = rules.iter().filter(|r| r.tag == "csv_data").collect();
        assert_eq!(csv_rules.len(), 1);
    }

    #[test]
    fn test_delete_topic_removes_rules() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let rule = TaggingRule {
            id: TaggingRuleId::new(),
            name: "CSV Files".to_string(),
            workspace_id,
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };
        db.upsert_tagging_rule(&rule).unwrap();

        // Delete the rule
        db.delete_tagging_rule(&rule.id).unwrap();

        let rules = db.list_tagging_rules(&workspace_id).unwrap();
        assert!(rules.is_empty());
    }
}

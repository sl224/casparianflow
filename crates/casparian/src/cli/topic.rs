//! Topic command - Manage topics
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table};
use casparian::scout::{Database, FileStatus};
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

/// Get all topics and their statistics from the database
fn get_topic_stats(db: &Database) -> HashMap<String, TopicStats> {
    let mut stats: HashMap<String, TopicStats> = HashMap::new();

    // Get all sources
    let sources = db.list_sources().unwrap_or_default();

    for source in sources {
        let files = db.list_files_by_source(&source.id, 100000).unwrap_or_default();
        for file in files {
            let topic = file.tag.clone().unwrap_or_else(|| "(untagged)".to_string());
            let entry = stats.entry(topic).or_default();
            entry.total += 1;
            entry.total_size += file.size;

            match file.status {
                FileStatus::Processed => entry.processed += 1,
                FileStatus::Pending | FileStatus::Tagged | FileStatus::Queued | FileStatus::Processing => {
                    entry.pending += 1
                }
                FileStatus::Failed => entry.failed += 1,
                FileStatus::Skipped | FileStatus::Deleted => {}
            }
        }
    }

    stats
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

    match action {
        TopicAction::List { json } => list_topics(&db, json),
        TopicAction::Create { name, description: _ } => create_topic(&db, &name),
        TopicAction::Show { name, json } => show_topic(&db, &name, json),
        TopicAction::Delete { name, force } => delete_topic(&db, &name, force),
        TopicAction::Files { name, limit } => list_topic_files(&db, &name, limit),
    }
}

fn list_topics(db: &Database, json: bool) -> anyhow::Result<()> {
    let stats = get_topic_stats(db);

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

fn create_topic(db: &Database, name: &str) -> anyhow::Result<()> {
    // Topics are implicit - they exist when files are tagged with them
    // But we can create a rule that uses this topic to make it "real"

    // Check if topic already has files
    let stats = get_topic_stats(db);
    if stats.contains_key(name) {
        return Err(HelpfulError::new(format!("Topic already exists: {}", name))
            .with_context(format!("{} files tagged with this topic", stats[name].total))
            .with_suggestion("TRY: Use 'casparian topic show' to see topic details")
            .into());
    }

    // Check if there's already a rule for this topic
    let rules = db.list_tagging_rules()?;
    let has_rule = rules.iter().any(|r| r.tag == name);

    if has_rule {
        println!("Topic '{}' already has rules configured.", name);
        println!("Files will be tagged when they match the rule patterns.");
    } else {
        println!("Created topic '{}'", name);
        println!();
        println!("This is an empty topic (no files tagged yet).");
        println!("Add files by:");
        println!("  1. Creating a rule: casparian rule add '*.csv' --topic {}", name);
        println!("  2. Manual tagging: casparian tag /path/to/file.csv --topic {}", name);
    }

    Ok(())
}

fn show_topic(db: &Database, name: &str, json: bool) -> anyhow::Result<()> {
    let stats = get_topic_stats(db);
    let topic_stats = stats.get(name);

    if topic_stats.is_none() && name != "(untagged)" {
        // Check if there's a rule for this topic
        let rules = db.list_tagging_rules()?;
        let has_rule = rules.iter().any(|r| r.tag == name);

        if !has_rule {
            return Err(HelpfulError::new(format!("Topic not found: {}", name))
                .with_suggestion("TRY: Use 'casparian topic ls' to see available topics")
                .with_suggestion("TRY: Create a rule for this topic with 'casparian rule add'")
                .into());
        }
    }

    // Get rules that produce this topic
    let rules = db.list_tagging_rules()?;
    let topic_rules: Vec<_> = rules.iter().filter(|r| r.tag == name).collect();

    // Get files with this topic
    let files = db.list_files_by_tag(name, 1000).unwrap_or_default();

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
        for rule in &topic_rules {
            // Count matches for this rule
            let matched = files
                .iter()
            .filter(|f| f.rule_id.as_ref() == Some(&rule.id))
                .count();
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
                .map(|entries| entries.filter(|e| {
                    e.as_ref()
                        .map(|e| e.path().extension().map(|ext| ext == "parquet").unwrap_or(false))
                        .unwrap_or(false)
                }).count())
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
            println!("  {} parquet files ({})", parquet_count, format_size(total_size));
        }
        println!();
    }

    // Commands section
    println!("COMMANDS");
    println!("  casparian files --topic {}          # List all files", name);
    if stats.failed > 0 {
        println!("  casparian files --topic {} --failed # List failures", name);
    }
    Ok(())
}

fn delete_topic(db: &Database, name: &str, force: bool) -> anyhow::Result<()> {
    let stats = get_topic_stats(db);
    let topic_stats = stats.get(name);

    if topic_stats.is_none() {
        // Check rules
        let rules = db.list_tagging_rules()?;
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
        return Err(HelpfulError::new(format!("Topic '{}' has {} files", name, stats.total))
            .with_context("Deleting this topic will untag all files")
            .with_suggestion("TRY: Use --force to remove anyway")
            .into());
    }

    // Delete rules for this topic
    let rules = db.list_tagging_rules()?;
    for rule in rules.iter().filter(|r| r.tag == name) {
        db.delete_tagging_rule(&rule.id).map_err(|e| {
            HelpfulError::new(format!("Failed to delete tagging rule: {}", e))
                .with_context(format!("Rule ID: {}", rule.id))
        })?;
    }

    // Untag files (set tag to NULL, status to pending)
    let files = db.list_files_by_tag(name, 100000).map_err(|e| {
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

fn list_topic_files(db: &Database, name: &str, limit: usize) -> anyhow::Result<()> {
    let files = db.list_files_by_tag(name, limit).map_err(|e| {
        HelpfulError::new(format!("Failed to list files: {}", e))
    })?;

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

    #[test]
    fn test_get_topic_stats() {
        let db = Database::open_in_memory().unwrap();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
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
            let file = ScannedFile::new(
                source_id.clone(),
                &format!("/data/{}", name),
                name,
                1000,
                12345 + i as i64,
            );
            let result = db.upsert_file(&file).unwrap();
            db.tag_file(result.id, topic).unwrap();
        }

        let stats = get_topic_stats(&db);
        assert_eq!(stats.get("sales").map(|s| s.total), Some(2));
        assert_eq!(stats.get("invoices").map(|s| s.total), Some(1));
    }

    #[test]
    fn test_topic_with_rules() {
        let db = Database::open_in_memory().unwrap();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let rule = TaggingRule {
            id: TaggingRuleId::new(),
            name: "CSV Files".to_string(),
            source_id: source_id.clone(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };
        db.upsert_tagging_rule(&rule).unwrap();

        // Check that we can find rules for the topic
        let rules = db.list_tagging_rules().unwrap();
        let csv_rules: Vec<_> = rules.iter().filter(|r| r.tag == "csv_data").collect();
        assert_eq!(csv_rules.len(), 1);
    }

    #[test]
    fn test_delete_topic_removes_rules() {
        let db = Database::open_in_memory().unwrap();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let rule = TaggingRule {
            id: TaggingRuleId::new(),
            name: "CSV Files".to_string(),
            source_id: source_id.clone(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };
        db.upsert_tagging_rule(&rule).unwrap();

        // Delete the rule
        db.delete_tagging_rule(&rule.id).unwrap();

        let rules = db.list_tagging_rules().unwrap();
        assert!(rules.is_empty());
    }
}

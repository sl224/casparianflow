//! Rule command - Manage tagging rules
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::error::HelpfulError;
use crate::cli::output::print_table;
use casparian::scout::{Database, TaggingRule};
use clap::Subcommand;
use glob::Pattern;

/// Subcommands for rule management
#[derive(Subcommand, Debug, Clone)]
pub enum RuleAction {
    /// List all rules
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a new rule
    Add {
        /// Glob pattern to match files
        pattern: String,
        /// Topic to assign to matching files
        #[arg(long)]
        topic: String,
        /// Rule priority (higher = evaluated first)
        #[arg(long, default_value = "0")]
        priority: i32,
    },
    /// Show rule details
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove a rule
    Remove {
        id: String,
        #[arg(long)]
        force: bool,
    },
    /// Test a rule against a path
    Test {
        id: String,
        path: String,
    },
}

/// Validate a glob pattern
fn validate_pattern(pattern: &str) -> Result<Pattern, HelpfulError> {
    Pattern::new(pattern).map_err(|e| {
        HelpfulError::new(format!("Invalid glob pattern: {}", e))
            .with_context(format!("Pattern: {}", pattern))
            .with_suggestion("TRY: Examples: *.csv, sales/**/*.json, data_????.log")
            .with_suggestion("TRY: Use * for any characters, ** for directories, ? for single character")
    })
}

/// Count how many files in the database match a pattern
async fn count_matching_files(db: &Database, pattern: &str) -> u64 {
    let pat = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    // Get all files and count matches
    let stats = db.get_stats().await.unwrap_or_default();
    if stats.total_files == 0 {
        return 0;
    }

    // Query all sources and their files
    let sources = db.list_sources().await.unwrap_or_default();
    let mut matched = 0u64;

    for source in sources {
        let files = db.list_files_by_source(&source.id, 100000).await.unwrap_or_default();
        for file in files {
            // Match against relative path
            if pat.matches(&file.rel_path) {
                matched += 1;
            }
        }
    }

    matched
}

/// Get matched file count for a specific rule
async fn get_rule_matched_count(db: &Database, rule: &TaggingRule) -> u64 {
    count_matching_files(db, &rule.pattern).await
}

/// Execute the rule command
pub fn run(action: RuleAction) -> anyhow::Result<()> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_async(action))
}

async fn run_async(action: RuleAction) -> anyhow::Result<()> {
    let db_path = active_db_path();
    let db = Database::open(&db_path).await.map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;

    match action {
        RuleAction::List { json } => list_rules(&db, json).await,
        RuleAction::Add { pattern, topic, priority } => add_rule(&db, pattern, topic, priority).await,
        RuleAction::Show { id, json } => show_rule(&db, &id, json).await,
        RuleAction::Remove { id, force } => remove_rule(&db, &id, force).await,
        RuleAction::Test { id, path } => test_rule(&db, &id, &path).await,
    }
}

async fn list_rules(db: &Database, json: bool) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules().await.map_err(|e| {
        HelpfulError::new(format!("Failed to list rules: {}", e))
    })?;

    if rules.is_empty() {
        println!("No tagging rules configured.");
        println!();
        println!("Add a rule with:");
        println!("  casparian rule add '*.csv' --topic csv_data");
        return Ok(());
    }

    if json {
        let mut output = Vec::new();
        for rule in &rules {
            let matched = get_rule_matched_count(db, rule).await;
            output.push(serde_json::json!({
                "id": rule.id,
                "pattern": rule.pattern,
                "topic": rule.tag,
                "priority": rule.priority,
                "matched": matched,
                "enabled": rule.enabled,
            }));
        }
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("RULES (applied in priority order)");

    let mut rows = Vec::new();
    for rule in &rules {
        let matched = get_rule_matched_count(db, rule).await;
        rows.push(vec![
            rule.pattern.clone(),
            rule.tag.clone(),
            format!("{}", rule.priority),
            format!("{}", matched),
        ]);
    }

    print_table(&["PATTERN", "TOPIC", "PRIORITY", "MATCHED"], rows);
    println!();
    println!("{} rules", rules.len());

    Ok(())
}

async fn add_rule(db: &Database, pattern: String, topic: String, priority: i32) -> anyhow::Result<()> {
    // Validate pattern
    validate_pattern(&pattern)?;

    // Check if we have any sources
    let sources = db.list_sources().await?;
    if sources.is_empty() {
        return Err(HelpfulError::new("No sources configured")
            .with_context("Rules require at least one source")
            .with_suggestion("TRY: casparian source add /path/to/data")
            .into());
    }

    // Check if pattern already exists
    let existing = db.list_tagging_rules().await?;
    for rule in &existing {
        if rule.pattern == pattern {
            return Err(HelpfulError::new(format!("Pattern already exists: {}", pattern))
                .with_context(format!("Rule ID: {}, Topic: {}", rule.id, rule.tag))
                .with_suggestion("TRY: Use 'casparian rule rm' to remove the existing rule first")
                .into());
        }
    }

    // Create rules for each source
    // In practice, we typically have one source - but the schema requires source_id
    let source = &sources[0];

    let rule = TaggingRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{} -> {}", pattern, topic),
        source_id: source.id.clone(),
        pattern: pattern.clone(),
        tag: topic.clone(),
        priority,
        enabled: true,
    };

    db.upsert_tagging_rule(&rule).await.map_err(|e| {
        HelpfulError::new(format!("Failed to create rule: {}", e))
    })?;

    // Count existing matches
    let matched = count_matching_files(db, &pattern).await;

    println!("Added rule: {} -> {}", pattern, topic);
    println!("  Priority: {}", priority);
    if matched > 0 {
        println!("  {} existing files would match", matched);
        println!();
        println!("Apply to existing files with:");
        println!("  casparian source sync --all");
    } else {
        println!("  No existing files match this pattern");
    }

    Ok(())
}

async fn show_rule(db: &Database, id: &str, json: bool) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules().await?;
    let rule = rules.iter().find(|r| r.id == id || r.pattern == id);

    let rule = match rule {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Rule not found: {}", id))
                .with_suggestion("TRY: Use 'casparian rule ls' to see available rules")
                .into());
        }
    };

    let matched = get_rule_matched_count(db, rule).await;

    if json {
        let output = serde_json::json!({
            "id": rule.id,
            "name": rule.name,
            "pattern": rule.pattern,
            "topic": rule.tag,
            "priority": rule.priority,
            "source_id": rule.source_id,
            "enabled": rule.enabled,
            "matched": matched,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("RULE: {}", rule.pattern);
    println!();
    println!("  ID:       {}", rule.id);
    println!("  Pattern:  {}", rule.pattern);
    println!("  Topic:    {}", rule.tag);
    println!("  Priority: {}", rule.priority);
    println!("  Enabled:  {}", if rule.enabled { "yes" } else { "no" });
    println!();
    println!("  Matched:  {} files", matched);

    // Show sample matches
    if matched > 0 {
        println!();
        println!("SAMPLE MATCHES (first 5):");
        let pat = Pattern::new(&rule.pattern).unwrap();
        let files = db.list_files_by_source(&rule.source_id, 1000).await.unwrap_or_default();
        let mut count = 0;
        for file in files {
            if pat.matches(&file.rel_path) {
                println!("  {}", file.rel_path);
                count += 1;
                if count >= 5 {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn remove_rule(db: &Database, id: &str, force: bool) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules().await?;
    let rule = rules.iter().find(|r| r.id == id || r.pattern == id);

    let rule = match rule {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Rule not found: {}", id))
                .with_suggestion("TRY: Use 'casparian rule ls' to see available rules")
                .into());
        }
    };

    // Count files that were tagged by this rule
    let files = db.list_files_by_source(&rule.source_id, 100000).await.unwrap_or_default();
    let tagged_by_rule: Vec<_> = files
        .iter()
        .filter(|f| f.rule_id.as_deref() == Some(&rule.id))
        .collect();

    if !tagged_by_rule.is_empty() && !force {
        return Err(HelpfulError::new(format!(
            "Rule has tagged {} files",
            tagged_by_rule.len()
        ))
        .with_context("Removing this rule will leave files without a tag assignment rule")
        .with_suggestion("TRY: Use --force to remove anyway")
        .with_suggestion("TRY: Files will keep their current tags but won't be re-tagged automatically")
        .into());
    }

    let rule_id = rule.id.clone();
    let rule_pattern = rule.pattern.clone();

    db.delete_tagging_rule(&rule_id).await.map_err(|e| {
        HelpfulError::new(format!("Failed to remove rule: {}", e))
    })?;

    println!("Removed rule: {}", rule_pattern);
    if !tagged_by_rule.is_empty() {
        println!("  {} files were tagged by this rule", tagged_by_rule.len());
        println!("  Files keep their current tags");
    }

    Ok(())
}

async fn test_rule(db: &Database, id: &str, path: &str) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules().await?;
    let rule = rules.iter().find(|r| r.id == id || r.pattern == id);

    let rule = match rule {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Rule not found: {}", id))
                .with_suggestion("TRY: Use 'casparian rule ls' to see available rules")
                .into());
        }
    };

    let pat = validate_pattern(&rule.pattern)?;

    if pat.matches(path) {
        println!("MATCH: '{}' matches pattern '{}'", path, rule.pattern);
        println!("  Would be tagged as: {}", rule.tag);
    } else {
        println!("NO MATCH: '{}' does not match pattern '{}'", path, rule.pattern);
        println!();
        println!("Pattern expects:");
        println!("  {}", rule.pattern);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian::scout::{ScannedFile, Source, SourceType};

    #[test]
    fn test_validate_pattern_valid() {
        assert!(validate_pattern("*.csv").is_ok());
        assert!(validate_pattern("sales/**/*.json").is_ok());
        assert!(validate_pattern("data_????.log").is_ok());
        assert!(validate_pattern("[abc]*.txt").is_ok());
    }

    #[test]
    fn test_validate_pattern_invalid() {
        // Empty pattern is actually valid in glob
        assert!(validate_pattern("[invalid").is_err());
    }

    #[test]
    fn test_pattern_matching() {
        let pat = Pattern::new("*.csv").unwrap();
        assert!(pat.matches("test.csv"));
        // Note: glob's default MatchOptions allows * to match /
        // To enforce strict path matching, use MatchOptions { require_literal_separator: true, .. }
        assert!(!pat.matches("test.json"));
    }

    #[test]
    fn test_recursive_pattern_matching() {
        let pat = Pattern::new("**/*.csv").unwrap();
        assert!(pat.matches("test.csv"));
        assert!(pat.matches("data/test.csv"));
        assert!(pat.matches("data/nested/test.csv"));
        assert!(!pat.matches("test.json"));
    }

    #[tokio::test]
    async fn test_add_rule_creates_entry() {
        let db = Database::open_in_memory().await.unwrap();

        // Add a source first
        let source = Source {
            id: "src-1".to_string(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

        // Add a rule
        let rule = TaggingRule {
            id: "rule-1".to_string(),
            name: "CSV Files".to_string(),
            source_id: "src-1".to_string(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };
        db.upsert_tagging_rule(&rule).await.unwrap();

        let rules = db.list_tagging_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "*.csv");
        assert_eq!(rules[0].tag, "csv_data");
    }

    #[tokio::test]
    async fn test_count_matching_files() {
        let db = Database::open_in_memory().await.unwrap();

        let source = Source {
            id: "src-1".to_string(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

        // Add files
        for name in &["test.csv", "data.csv", "info.json"] {
            let file = ScannedFile::new(
                "src-1",
                &format!("/data/{}", name),
                name,
                1000,
                12345,
            );
            db.upsert_file(&file).await.unwrap();
        }

        let csv_matches = count_matching_files(&db, "*.csv").await;
        assert_eq!(csv_matches, 2);

        let json_matches = count_matching_files(&db, "*.json").await;
        assert_eq!(json_matches, 1);

        let all_matches = count_matching_files(&db, "*.*").await;
        assert_eq!(all_matches, 3);
    }
}

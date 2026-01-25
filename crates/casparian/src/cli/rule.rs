//! Rule command - Manage tagging rules
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::error::HelpfulError;
use crate::cli::output::print_table;
use crate::cli::workspace;
use casparian::scout::{Database, TaggingRule, TaggingRuleId, WorkspaceId};
use casparian_db::DbValue;
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
    Test { id: String, path: String },
}

/// Validate a glob pattern
fn validate_pattern(pattern: &str) -> Result<Pattern, HelpfulError> {
    Pattern::new(pattern).map_err(|e| {
        HelpfulError::new(format!("Invalid glob pattern: {}", e))
            .with_context(format!("Pattern: {}", pattern))
            .with_suggestion("TRY: Examples: *.csv, sales/**/*.json, data_????.log")
            .with_suggestion(
                "TRY: Use * for any characters, ** for directories, ? for single character",
            )
    })
}

fn find_rule<'a>(rules: &'a [TaggingRule], input: &str) -> Option<&'a TaggingRule> {
    let parsed_id = TaggingRuleId::parse(input).ok();
    rules
        .iter()
        .find(|r| r.pattern == input || parsed_id.map_or(false, |id| r.id == id))
}

/// Count how many files in the database match a pattern
fn count_matching_files(db: &Database, workspace_id: &WorkspaceId, pattern: &str) -> u64 {
    let pat = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    // Get all files and count matches
    let stats = db.get_stats().unwrap_or_default();
    if stats.total_files == 0 {
        return 0;
    }

    // Query all sources and their files
    let sources = db.list_sources(workspace_id).unwrap_or_default();
    let mut matched = 0u64;

    for source in sources {
        let files = db
            .list_files_by_source(&source.id, 100000)
            .unwrap_or_default();
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
fn get_rule_matched_count(db: &Database, workspace_id: &WorkspaceId, rule: &TaggingRule) -> u64 {
    count_matching_files(db, workspace_id, &rule.pattern)
}

/// Execute the rule command
pub fn run(action: RuleAction) -> anyhow::Result<()> {
    run_with_action(action)
}

fn run_with_action(action: RuleAction) -> anyhow::Result<()> {
    let db_path = active_db_path();
    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;
    let workspace_id = ensure_workspace_id(&db)?;

    match action {
        RuleAction::List { json } => list_rules(&db, &workspace_id, json),
        RuleAction::Add {
            pattern,
            topic,
            priority,
        } => add_rule(&db, &workspace_id, pattern, topic, priority),
        RuleAction::Show { id, json } => show_rule(&db, &workspace_id, &id, json),
        RuleAction::Remove { id, force } => remove_rule(&db, &workspace_id, &id, force),
        RuleAction::Test { id, path } => test_rule(&db, &workspace_id, &id, &path),
    }
}

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db)
        .map_err(|e| e.with_context("The workspace registry is required for tagging rules"))
}

fn list_rules(db: &Database, workspace_id: &WorkspaceId, json: bool) -> anyhow::Result<()> {
    let rules = db
        .list_tagging_rules(workspace_id)
        .map_err(|e| HelpfulError::new(format!("Failed to list rules: {}", e)))?;

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
            let matched = get_rule_matched_count(db, workspace_id, rule);
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
        let matched = get_rule_matched_count(db, workspace_id, rule);
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

fn add_rule(
    db: &Database,
    workspace_id: &WorkspaceId,
    pattern: String,
    topic: String,
    priority: i32,
) -> anyhow::Result<()> {
    // Validate pattern
    validate_pattern(&pattern)?;

    // Check if we have any sources
    let sources = db.list_sources(workspace_id)?;
    if sources.is_empty() {
        return Err(HelpfulError::new("No sources configured")
            .with_context("Rules require at least one source")
            .with_suggestion("TRY: casparian source add /path/to/data")
            .into());
    }

    // Check if pattern already exists
    let existing = db.list_tagging_rules(workspace_id)?;
    for rule in &existing {
        if rule.pattern == pattern {
            return Err(
                HelpfulError::new(format!("Pattern already exists: {}", pattern))
                    .with_context(format!("Rule ID: {}, Topic: {}", rule.id, rule.tag))
                    .with_suggestion(
                        "TRY: Use 'casparian rule rm' to remove the existing rule first",
                    )
                    .into(),
            );
        }
    }

    let rule = TaggingRule {
        id: TaggingRuleId::new(),
        name: format!("{} -> {}", pattern, topic),
        workspace_id: *workspace_id,
        pattern: pattern.clone(),
        tag: topic.clone(),
        priority,
        enabled: true,
    };

    db.upsert_tagging_rule(&rule)
        .map_err(|e| HelpfulError::new(format!("Failed to create rule: {}", e)))?;

    // Count existing matches
    let matched = count_matching_files(db, workspace_id, &pattern);

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

fn show_rule(
    db: &Database,
    workspace_id: &WorkspaceId,
    id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules(workspace_id)?;
    let rule = find_rule(&rules, id);

    let rule = match rule {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Rule not found: {}", id))
                .with_suggestion("TRY: Use 'casparian rule ls' to see available rules")
                .into());
        }
    };

    let matched = get_rule_matched_count(db, workspace_id, rule);

    if json {
        let output = serde_json::json!({
            "id": rule.id,
            "name": rule.name,
            "pattern": rule.pattern,
            "topic": rule.tag,
            "priority": rule.priority,
            "workspace_id": rule.workspace_id,
            "enabled": rule.enabled,
            "matched": matched,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("RULE: {}", rule.pattern);
    println!();
    println!("  ID:       {}", rule.id);
    println!("  Pattern:     {}", rule.pattern);
    println!("  Topic:       {}", rule.tag);
    println!("  Priority:    {}", rule.priority);
    println!("  Workspace:   {}", rule.workspace_id);
    println!("  Enabled:     {}", if rule.enabled { "yes" } else { "no" });
    println!();
    println!("  Matched:  {} files", matched);

    // Show sample matches
    if matched > 0 {
        println!();
        println!("SAMPLE MATCHES (first 5):");
        let pat = Pattern::new(&rule.pattern).unwrap();
        let sources = db.list_sources(workspace_id).unwrap_or_default();
        let mut count = 0;
        for source in sources {
            let files = db
                .list_files_by_source(&source.id, 1000)
                .unwrap_or_default();
            for file in files {
                if pat.matches(&file.rel_path) {
                    println!("  {}", file.rel_path);
                    count += 1;
                    if count >= 5 {
                        break;
                    }
                }
            }
            if count >= 5 {
                break;
            }
        }
    }

    Ok(())
}

fn remove_rule(
    db: &Database,
    workspace_id: &WorkspaceId,
    id: &str,
    force: bool,
) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules(workspace_id)?;
    let rule = find_rule(&rules, id);

    let rule = match rule {
        Some(r) => r,
        None => {
            return Err(HelpfulError::new(format!("Rule not found: {}", id))
                .with_suggestion("TRY: Use 'casparian rule ls' to see available rules")
                .into());
        }
    };

    let tagged_by_rule_count = db
        .conn()
        .query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_file_tags WHERE workspace_id = ? AND rule_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(rule.id.to_string()),
            ],
        )
        .unwrap_or(0);

    if tagged_by_rule_count > 0 && !force {
        return Err(
            HelpfulError::new(format!("Rule has tagged {} files", tagged_by_rule_count))
                .with_context("Removing this rule will leave files without a tag assignment rule")
                .with_suggestion("TRY: Use --force to remove anyway")
                .with_suggestion(
                    "TRY: Files will keep their current tags but won't be re-tagged automatically",
                )
                .into(),
        );
    }

    let rule_id = rule.id.clone();
    let rule_pattern = rule.pattern.clone();

    db.delete_tagging_rule(&rule_id)
        .map_err(|e| HelpfulError::new(format!("Failed to remove rule: {}", e)))?;

    println!("Removed rule: {}", rule_pattern);
    if tagged_by_rule_count > 0 {
        println!("  {} files were tagged by this rule", tagged_by_rule_count);
        println!("  Files keep their current tags");
    }

    Ok(())
}

fn test_rule(
    db: &Database,
    workspace_id: &WorkspaceId,
    id: &str,
    path: &str,
) -> anyhow::Result<()> {
    let rules = db.list_tagging_rules(workspace_id)?;
    let rule = find_rule(&rules, id);

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
        println!(
            "NO MATCH: '{}' does not match pattern '{}'",
            path, rule.pattern
        );
        println!();
        println!("Pattern expects:");
        println!("  {}", rule.pattern);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian::scout::{ScannedFile, Source, SourceId, SourceType, TaggingRuleId};

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

    #[test]
    fn test_add_rule_creates_entry() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        // Add a source first
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

        // Add a rule
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

        let rules = db.list_tagging_rules(&workspace_id).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "*.csv");
        assert_eq!(rules[0].tag, "csv_data");
    }

    #[test]
    fn test_count_matching_files() {
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

        // Add files
        for name in &["test.csv", "data.csv", "info.json"] {
            let path = format!("/data/{}", name);
            let file_uid = crate::scout::file_uid::weak_uid_from_path_str(&path);
            let file = ScannedFile::new(
                workspace_id,
                source_id.clone(),
                &file_uid,
                &path,
                name,
                1000,
                12345,
            );
            db.upsert_file(&file).unwrap();
        }

        let csv_matches = count_matching_files(&db, &workspace_id, "*.csv");
        assert_eq!(csv_matches, 2);

        let json_matches = count_matching_files(&db, &workspace_id, "*.json");
        assert_eq!(json_matches, 1);

        let all_matches = count_matching_files(&db, &workspace_id, "*.*");
        assert_eq!(all_matches, 3);
    }
}

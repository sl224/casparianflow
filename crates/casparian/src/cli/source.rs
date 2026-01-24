//! Source command - Manage data sources
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::context;
use crate::cli::error::HelpfulError;
use crate::cli::workspace;
use crate::cli::output::{format_size, print_table};
use casparian::scout::{Database, Scanner, Source, SourceId, SourceType, WorkspaceId};
use casparian_db::DbValue;
use clap::Subcommand;
use std::path::PathBuf;

/// Subcommands for source management
#[derive(Subcommand, Debug, Clone)]
pub enum SourceAction {
    /// List all sources
    #[command(visible_alias = "ls")]
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a new source
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        recursive: bool,
    },
    /// Show source details
    Show {
        name: String,
        /// Show files in this source
        #[arg(long)]
        files: bool,
        /// Maximum files to display (with --files)
        #[arg(long, default_value = "50")]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Remove a source
    #[command(visible_alias = "rm")]
    Remove {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Sync a source (re-discover files)
    Sync {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Set or show the default source context
    Use {
        /// Source name to set as default
        name: Option<String>,
        /// Clear the default source
        #[arg(long)]
        clear: bool,
    },
}

/// Source statistics for display
struct SourceStats {
    file_count: u64,
    total_size: u64,
}

#[derive(Debug, Clone)]
struct SourceFileRow {
    id: i64,
    rel_path: String,
    size: u64,
    status: String,
}

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db).map_err(|e| {
        e.with_context("The workspace registry is required for sources")
    })
}

fn find_source<'a>(sources: &'a [Source], input: &str) -> Option<&'a Source> {
    let parsed_id = SourceId::parse(input).ok();
    sources
        .iter()
        .find(|s| s.name == input || parsed_id.map_or(false, |id| s.id == id))
}

/// Get stats for a source from the database
fn get_source_stats(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    source_id: &SourceId,
) -> SourceStats {
    let row = conn
        .query_optional(
            "SELECT COUNT(*) as total, COALESCE(SUM(size), 0) as total_size \
             FROM scout_files WHERE workspace_id = ? AND source_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(source_id.as_i64()),
            ],
        )
        .unwrap_or(None);

    if let Some(row) = row {
        let total: i64 = row.get_by_name("total").unwrap_or(0);
        let total_size: i64 = row.get_by_name("total_size").unwrap_or(0);
        SourceStats {
            file_count: total as u64,
            total_size: total_size as u64,
        }
    } else {
        SourceStats {
            file_count: 0,
            total_size: 0,
        }
    }
}

fn list_source_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    source_id: &SourceId,
    limit: usize,
) -> Result<Vec<SourceFileRow>, HelpfulError> {
    let rows = conn
        .query_all(
            "SELECT id, rel_path, size, status \
             FROM scout_files \
             WHERE workspace_id = ? AND source_id = ? \
             ORDER BY mtime DESC \
             LIMIT ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(source_id.as_i64()),
                DbValue::from(limit as i64),
            ],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to list files: {}", e)))?;

    let mut files = Vec::new();
    for row in rows {
        let size_raw: i64 = row
            .get_by_name("size")
            .map_err(|e| HelpfulError::new(format!("Failed to read size: {}", e)))?;
        files.push(SourceFileRow {
            id: row
                .get_by_name("id")
                .map_err(|e| HelpfulError::new(format!("Failed to read id: {}", e)))?,
            rel_path: row
                .get_by_name("rel_path")
                .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
            size: size_raw as u64,
            status: row
                .get_by_name("status")
                .map_err(|e| HelpfulError::new(format!("Failed to read status: {}", e)))?,
        });
    }

    Ok(files)
}

fn fetch_tags_for_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    file_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<String>>, HelpfulError> {
    let mut tags_by_file = std::collections::HashMap::new();
    if file_ids.is_empty() {
        return Ok(tags_by_file);
    }

    let placeholders = std::iter::repeat("?")
        .take(file_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let mut params: Vec<DbValue> = vec![DbValue::from(workspace_id.to_string())];
    for id in file_ids {
        params.push(DbValue::from(*id));
    }

    let sql = format!(
        "SELECT file_id, tag FROM scout_file_tags \
         WHERE workspace_id = ? AND file_id IN ({}) \
         ORDER BY tag",
        placeholders
    );

    let rows = conn
        .query_all(&sql, &params)
        .map_err(|e| HelpfulError::new(format!("Failed to read file tags: {}", e)))?;
    for row in rows {
        let file_id: i64 = row
            .get_by_name("file_id")
            .map_err(|e| HelpfulError::new(format!("Failed to read file_id: {}", e)))?;
        let tag: String = row
            .get_by_name("tag")
            .map_err(|e| HelpfulError::new(format!("Failed to read tag: {}", e)))?;
        tags_by_file.entry(file_id).or_default().push(tag);
    }

    Ok(tags_by_file)
}

/// Execute the source command
pub fn run(action: SourceAction) -> anyhow::Result<()> {
    run_with_action(action)
}

fn run_with_action(action: SourceAction) -> anyhow::Result<()> {
    // Handle `use` command separately - it doesn't need DB for showing current context
    if let SourceAction::Use { name, clear } = action {
        return use_source(name, clear);
    }

    let db_path = active_db_path();
    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    match action {
        SourceAction::List { json } => list_sources(&db, &workspace_id, json),
        SourceAction::Add {
            path,
            name,
            recursive: _,
        } => add_source(&db, &workspace_id, path, name),
        SourceAction::Show {
            name,
            files,
            limit,
            json,
        } => show_source(conn, &workspace_id, &db, &name, files, limit, json),
        SourceAction::Remove { name, force } => remove_source(conn, &workspace_id, &db, &name, force),
        SourceAction::Sync { name, all } => sync_sources(&db, &workspace_id, name, all),
        SourceAction::Use { .. } => unreachable!(), // Handled above
    }
}

fn list_sources(db: &Database, workspace_id: &WorkspaceId, json: bool) -> anyhow::Result<()> {
    let sources = db
        .list_sources(workspace_id)
        .map_err(|e| HelpfulError::new(format!("Failed to list sources: {}", e)))?;

    if sources.is_empty() {
        println!("No sources configured.");
        println!();
        println!("Add a source with:");
        println!("  casparian source add /path/to/data");
        return Ok(());
    }

    if json {
        let output: Vec<serde_json::Value> = {
            let mut result = Vec::new();
            for s in &sources {
                let stats = get_source_stats(db.conn(), workspace_id, &s.id);
                result.push(serde_json::json!({
                    "name": s.name,
                    "workspace_id": s.workspace_id,
                    "path": s.path,
                    "enabled": s.enabled,
                    "files": stats.file_count,
                    "size": stats.total_size,
                }));
            }
            result
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("SOURCES");

    let mut rows = Vec::new();
    let mut total_files = 0u64;
    let mut total_size = 0u64;

    for source in &sources {
        let stats = get_source_stats(db.conn(), workspace_id, &source.id);
        total_files += stats.file_count;
        total_size += stats.total_size;

        rows.push(vec![
            source.name.clone(),
            source.path.clone(),
            format!("{}", stats.file_count),
            format_size(stats.total_size),
        ]);
    }

    print_table(&["NAME", "PATH", "FILES", "SIZE"], rows);
    println!();
    println!(
        "{} sources, {} files, {} total",
        sources.len(),
        total_files,
        format_size(total_size)
    );

    Ok(())
}

fn add_source(
    db: &Database,
    workspace_id: &WorkspaceId,
    path: PathBuf,
    name: Option<String>,
) -> anyhow::Result<()> {
    // Validate path exists and is a directory
    if !path.exists() {
        return Err(HelpfulError::path_not_found(&path).into());
    }

    if !path.is_dir() {
        return Err(
            HelpfulError::new(format!("Not a directory: {}", path.display()))
                .with_context("Sources must be directories")
                .with_suggestion("TRY: Specify a directory path instead of a file")
                .into(),
        );
    }

    // Canonicalize path
    let canonical = path.canonicalize().map_err(|e| {
        HelpfulError::new(format!("Failed to resolve path: {}", e))
            .with_suggestion("TRY: Check the path exists and you have access")
    })?;

    // Generate name if not provided
    let source_name = name.unwrap_or_else(|| {
        canonical
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "source".to_string())
    });

    // Check if source with this path already exists
    let existing = db.list_sources(workspace_id)?;
    for s in &existing {
        if s.path == canonical.display().to_string() {
            return Err(
                HelpfulError::new(format!("Source already exists: {}", s.name))
                    .with_context(format!("Path: {}", s.path))
                    .with_suggestion(
                        "TRY: Use 'casparian source sync' to refresh the existing source",
                    )
                    .into(),
            );
        }
        if s.name == source_name {
            return Err(
                HelpfulError::new(format!("Source name already exists: {}", source_name))
                    .with_suggestion(format!("TRY: Use --name to specify a different name"))
                    .into(),
            );
        }
    }

    // Create the source
    let source = Source {
        workspace_id: *workspace_id,
        id: SourceId::new(),
        name: source_name.clone(),
        source_type: SourceType::Local,
        path: canonical.display().to_string(),
        poll_interval_secs: 30,
        enabled: true,
    };

    db.upsert_source(&source)
        .map_err(|e| HelpfulError::new(format!("Failed to create source: {}", e)))?;

    println!("Added source '{}'", source_name);
    println!("  Path: {}", canonical.display());
    println!("  ID: {}", source.id);
    println!();
    println!("Next steps:");
    println!("  casparian rule add '*.csv' --topic csv_data");
    println!("  casparian source sync {}", source_name);

    Ok(())
}

fn show_source(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    db: &Database,
    name: &str,
    show_files: bool,
    limit: usize,
    json: bool,
) -> anyhow::Result<()> {
    let sources = db.list_sources(workspace_id)?;
    let source = find_source(&sources, name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(HelpfulError::new(format!("Source not found: {}", name))
                .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                .into());
        }
    };

    let stats = get_source_stats(conn, workspace_id, &source.id);

    if json {
        let mut output = serde_json::json!({
            "id": source.id,
            "name": source.name,
            "path": source.path,
            "source_type": format!("{:?}", source.source_type),
            "enabled": source.enabled,
            "poll_interval_secs": source.poll_interval_secs,
            "files": stats.file_count,
            "size": stats.total_size,
        });

        // Include files in JSON output if requested
        if show_files {
            let files = db.list_files_by_source(&source.id, limit)?;
            let file_ids: Vec<i64> = files.iter().filter_map(|f| f.id).collect();
            let tags_by_file = fetch_tags_for_files(conn, workspace_id, &file_ids)?;
            let files_json: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    let tags = f
                        .id
                        .and_then(|id| tags_by_file.get(&id).cloned())
                        .unwrap_or_default();
                    serde_json::json!({
                        "path": f.rel_path,
                        "size": f.size,
                        "status": f.status.as_str(),
                        "tags": tags,
                    })
                })
                .collect();
            output["file_list"] = serde_json::Value::Array(files_json);
        }

        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("SOURCE: {}", source.name);
    println!();
    println!("  ID:       {}", source.id);
    println!("  Path:     {}", source.path);
    println!("  Type:     {:?}", source.source_type);
    println!("  Enabled:  {}", if source.enabled { "yes" } else { "no" });
    println!("  Poll:     {}s", source.poll_interval_secs);
    println!();
    println!("  Files:    {}", stats.file_count);
    println!("  Size:     {}", format_size(stats.total_size));
    println!();

    // Show rules for this source
    let rules = db.list_tagging_rules_for_workspace(workspace_id)?;
    if !rules.is_empty() {
        println!("RULES (WORKSPACE)");
        for rule in &rules {
            println!("  {} -> {}", rule.pattern, rule.tag);
        }
        println!();
    } else {
        println!("No tagging rules configured for this workspace.");
        println!("  TRY: casparian rule add '*.csv' --topic csv_data");
        println!();
    }

    // Show files if requested
    if show_files {
        let files = db.list_files_by_source(&source.id, limit)?;
        if files.is_empty() {
            println!("No files discovered yet.");
            println!("  TRY: casparian source sync {}", source.name);
        } else {
            println!("FILES");
            let file_ids: Vec<i64> = files.iter().filter_map(|f| f.id).collect();
            let tags_by_file = fetch_tags_for_files(conn, workspace_id, &file_ids)?;
            let rows: Vec<Vec<String>> = files
                .iter()
                .map(|f| {
                    let tags = f
                        .id
                        .and_then(|id| tags_by_file.get(&id))
                        .map(|tags| tags.join(", "))
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| "-".to_string());
                    vec![
                        if f.rel_path.len() > 50 {
                            format!("...{}", &f.rel_path[f.rel_path.len().saturating_sub(47)..])
                        } else {
                            f.rel_path.clone()
                        },
                        format_size(f.size),
                        tags,
                        f.status.as_str().to_string(),
                    ]
                })
                .collect();
            print_table(&["PATH", "SIZE", "TAGS", "STATUS"], rows);

            if files.len() >= limit {
                println!();
                println!(
                    "Showing {} of {} files. Use --limit to see more.",
                    limit, stats.file_count
                );
            }
        }
    }

    Ok(())
}

fn remove_source(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    db: &Database,
    name: &str,
    force: bool,
) -> anyhow::Result<()> {
    let sources = db.list_sources(workspace_id)?;
    let source = find_source(&sources, name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(HelpfulError::new(format!("Source not found: {}", name))
                .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                .into());
        }
    };

    let stats = get_source_stats(conn, workspace_id, &source.id);

    if stats.file_count > 0 && !force {
        return Err(HelpfulError::new(format!(
            "Source '{}' has {} files",
            source.name, stats.file_count
        ))
        .with_context("Removing this source will orphan the file records")
        .with_suggestion("TRY: Use --force to remove anyway")
        .with_suggestion("TRY: Remove files first if you want to keep processing records")
        .into());
    }

    let source_id = source.id.clone();
    let source_name = source.name.clone();

    db.delete_source(&source_id)
        .map_err(|e| HelpfulError::new(format!("Failed to remove source: {}", e)))?;

    println!("Removed source '{}'", source_name);
    if stats.file_count > 0 {
        println!("  {} files removed from database", stats.file_count);
    }

    Ok(())
}

fn sync_sources(
    db: &Database,
    workspace_id: &WorkspaceId,
    name: Option<String>,
    all: bool,
) -> anyhow::Result<()> {
    if name.is_none() && !all {
        return Err(HelpfulError::new("No source specified")
            .with_suggestion("TRY: casparian source sync <name>")
            .with_suggestion("TRY: casparian source sync --all")
            .into());
    }

    let sources = db.list_sources(workspace_id)?;

    if sources.is_empty() {
        return Err(HelpfulError::new("No sources configured")
            .with_suggestion("TRY: casparian source add /path/to/data")
            .into());
    }

    let to_sync: Vec<&Source> = if all {
        sources.iter().filter(|s| s.enabled).collect()
    } else {
        let name = name.as_ref().unwrap();
        match find_source(&sources, name) {
            Some(s) => vec![s],
            None => {
                return Err(HelpfulError::new(format!("Source not found: {}", name))
                    .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                    .into());
            }
        }
    };

    if to_sync.is_empty() {
        println!("No enabled sources to sync.");
        return Ok(());
    }

    for source in to_sync {
        println!("Syncing source '{}'...", source.name);

        // Use the scanner from casparian_scout
        let scanner = Scanner::new(db.clone());
        match scanner.scan_source(source) {
            Ok(result) => {
                println!(
                    "  {} files discovered ({} new, {} changed)",
                    result.stats.files_discovered,
                    result.stats.files_new,
                    result.stats.files_changed
                );
            }
            Err(e) => {
                println!("  Error: {}", e);
            }
        }
    }

    Ok(())
}

/// Set or show the default source context
fn use_source(name: Option<String>, clear: bool) -> anyhow::Result<()> {
    // Handle --clear flag
    if clear {
        context::clear_default_source()?;
        println!("Default source cleared.");
        return Ok(());
    }

    // If no name provided, show current context
    if name.is_none() {
        match context::get_default_source() {
            Some(source_name) => {
                println!("Current source: {}", source_name);
            }
            None => {
                println!("No default source set.");
                println!();
                println!("Set a default with:");
                println!("  casparian source use <name>");
            }
        }
        return Ok(());
    }

    // Validate source exists before setting
    let source_name = name.unwrap();
    let db_path = active_db_path();
    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;
    let workspace_id = ensure_workspace_id(&db)?;
    let sources = db.list_sources(&workspace_id)?;
    let source = find_source(&sources, &source_name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(
                HelpfulError::new(format!("Source not found: {}", source_name))
                    .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                    .into(),
            );
        }
    };

    // Set the context
    context::set_default_source(&source.name)?;
    println!("Default source set to: {}", source.name);
    println!();
    println!("Now you can run:");
    println!(
        "  casparian files              # Files from '{}'",
        source.name
    );
    println!("  casparian files --all        # Files from all sources");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian::scout::{ScannedFile, SourceId};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_add_source_path_validation() {
        let temp = TempDir::new().unwrap();
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;

        // Test adding a valid directory
        let test_dir = temp.path().join("test_data");
        fs::create_dir(&test_dir).unwrap();

        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: test_dir.display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let sources = db.list_sources(&workspace_id).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "test");
    }

    #[test]
    fn test_source_stats() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Add some files
        let file = ScannedFile::new(
            workspace_id,
            source_id.clone(),
            "/data/test.csv",
            "test.csv",
            1000,
            12345,
        );
        db.upsert_file(&file).unwrap();

        let stats = get_source_stats(db.conn(), &workspace_id, &source_id);
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.total_size, 1000);
    }

    #[test]
    fn test_remove_source_with_files() {
        let db = Database::open_in_memory().unwrap();
        let workspace_id = db.ensure_default_workspace().unwrap().id;
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Add a file
        let file = ScannedFile::new(
            workspace_id,
            source_id.clone(),
            "/data/test.csv",
            "test.csv",
            1000,
            12345,
        );
        db.upsert_file(&file).unwrap();

        // Delete source should remove files too
        db.delete_source(&source_id).unwrap();
        let sources = db.list_sources(&workspace_id).unwrap();
        assert!(sources.is_empty());
    }
}

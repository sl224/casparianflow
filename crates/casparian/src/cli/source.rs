//! Source command - Manage data sources
//!
//! Data-oriented design: structs for data, functions for behavior.

use crate::cli::config::active_db_path;
use crate::cli::context;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table};
use casparian::scout::{Database, Scanner, Source, SourceType};
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

/// Get stats for a source from the database
async fn get_source_stats(db: &Database, source_id: &str) -> SourceStats {
    // Query file count and total size for this source
    let files = db.list_files_by_source(source_id, 100000).await.unwrap_or_default();
    let file_count = files.len() as u64;
    let total_size = files.iter().map(|f| f.size).sum();
    SourceStats { file_count, total_size }
}

/// Execute the source command
pub fn run(action: SourceAction) -> anyhow::Result<()> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_async(action))
}

async fn run_async(action: SourceAction) -> anyhow::Result<()> {
    // Handle `use` command separately - it doesn't need DB for showing current context
    if let SourceAction::Use { name, clear } = action {
        return use_source(name, clear).await;
    }

    let db_path = active_db_path();
    let db = Database::open(&db_path).await.map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;

    match action {
        SourceAction::List { json } => list_sources(&db, json).await,
        SourceAction::Add { path, name, recursive: _ } => add_source(&db, path, name).await,
        SourceAction::Show { name, files, limit, json } => show_source(&db, &name, files, limit, json).await,
        SourceAction::Remove { name, force } => remove_source(&db, &name, force).await,
        SourceAction::Sync { name, all } => sync_sources(&db, name, all).await,
        SourceAction::Use { .. } => unreachable!(), // Handled above
    }
}

async fn list_sources(db: &Database, json: bool) -> anyhow::Result<()> {
    let sources = db.list_sources().await.map_err(|e| {
        HelpfulError::new(format!("Failed to list sources: {}", e))
    })?;

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
                let stats = get_source_stats(db, &s.id).await;
                result.push(serde_json::json!({
                    "name": s.name,
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
        let stats = get_source_stats(db, &source.id).await;
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

async fn add_source(db: &Database, path: PathBuf, name: Option<String>) -> anyhow::Result<()> {
    // Validate path exists and is a directory
    if !path.exists() {
        return Err(HelpfulError::path_not_found(&path).into());
    }

    if !path.is_dir() {
        return Err(HelpfulError::new(format!("Not a directory: {}", path.display()))
            .with_context("Sources must be directories")
            .with_suggestion("TRY: Specify a directory path instead of a file")
            .into());
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
    let existing = db.list_sources().await?;
    for s in &existing {
        if s.path == canonical.display().to_string() {
            return Err(HelpfulError::new(format!("Source already exists: {}", s.name))
                .with_context(format!("Path: {}", s.path))
                .with_suggestion("TRY: Use 'casparian source sync' to refresh the existing source")
                .into());
        }
        if s.name == source_name {
            return Err(HelpfulError::new(format!("Source name already exists: {}", source_name))
                .with_suggestion(format!("TRY: Use --name to specify a different name"))
                .into());
        }
    }

    // Create the source
    let source = Source {
        id: uuid::Uuid::new_v4().to_string(),
        name: source_name.clone(),
        source_type: SourceType::Local,
        path: canonical.display().to_string(),
        poll_interval_secs: 30,
        enabled: true,
    };

    db.upsert_source(&source).await.map_err(|e| {
        HelpfulError::new(format!("Failed to create source: {}", e))
    })?;

    println!("Added source '{}'", source_name);
    println!("  Path: {}", canonical.display());
    println!("  ID: {}", source.id);
    println!();
    println!("Next steps:");
    println!("  casparian rule add '*.csv' --topic csv_data");
    println!("  casparian source sync {}", source_name);

    Ok(())
}

async fn show_source(db: &Database, name: &str, show_files: bool, limit: usize, json: bool) -> anyhow::Result<()> {
    let sources = db.list_sources().await?;
    let source = sources.iter().find(|s| s.name == name || s.id == name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(HelpfulError::new(format!("Source not found: {}", name))
                .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                .into());
        }
    };

    let stats = get_source_stats(db, &source.id).await;

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
            let files = db.list_files_by_source(&source.id, limit).await?;
            let files_json: Vec<serde_json::Value> = files.iter().map(|f| {
                serde_json::json!({
                    "path": f.rel_path,
                    "size": f.size,
                    "status": f.status.as_str(),
                    "tag": f.tag,
                })
            }).collect();
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
    let rules = db.list_tagging_rules_for_source(&source.id).await?;
    if !rules.is_empty() {
        println!("RULES");
        for rule in &rules {
            println!("  {} -> {}", rule.pattern, rule.tag);
        }
        println!();
    } else {
        println!("No tagging rules configured for this source.");
        println!("  TRY: casparian rule add '*.csv' --topic csv_data");
        println!();
    }

    // Show files if requested
    if show_files {
        let files = db.list_files_by_source(&source.id, limit).await?;
        if files.is_empty() {
            println!("No files discovered yet.");
            println!("  TRY: casparian source sync {}", source.name);
        } else {
            println!("FILES");
            let rows: Vec<Vec<String>> = files.iter().map(|f| {
                vec![
                    if f.rel_path.len() > 50 {
                        format!("...{}", &f.rel_path[f.rel_path.len().saturating_sub(47)..])
                    } else {
                        f.rel_path.clone()
                    },
                    format_size(f.size),
                    f.tag.as_deref().unwrap_or("-").to_string(),
                    f.status.as_str().to_string(),
                ]
            }).collect();
            print_table(&["PATH", "SIZE", "TAG", "STATUS"], rows);

            if files.len() >= limit {
                println!();
                println!("Showing {} of {} files. Use --limit to see more.", limit, stats.file_count);
            }
        }
    }

    Ok(())
}

async fn remove_source(db: &Database, name: &str, force: bool) -> anyhow::Result<()> {
    let sources = db.list_sources().await?;
    let source = sources.iter().find(|s| s.name == name || s.id == name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(HelpfulError::new(format!("Source not found: {}", name))
                .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                .into());
        }
    };

    let stats = get_source_stats(db, &source.id).await;

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

    db.delete_source(&source_id).await.map_err(|e| {
        HelpfulError::new(format!("Failed to remove source: {}", e))
    })?;

    println!("Removed source '{}'", source_name);
    if stats.file_count > 0 {
        println!("  {} files removed from database", stats.file_count);
    }

    Ok(())
}

async fn sync_sources(db: &Database, name: Option<String>, all: bool) -> anyhow::Result<()> {
    if name.is_none() && !all {
        return Err(HelpfulError::new("No source specified")
            .with_suggestion("TRY: casparian source sync <name>")
            .with_suggestion("TRY: casparian source sync --all")
            .into());
    }

    let sources = db.list_sources().await?;

    if sources.is_empty() {
        return Err(HelpfulError::new("No sources configured")
            .with_suggestion("TRY: casparian source add /path/to/data")
            .into());
    }

    let to_sync: Vec<&Source> = if all {
        sources.iter().filter(|s| s.enabled).collect()
    } else {
        let name = name.as_ref().unwrap();
        match sources.iter().find(|s| s.name == *name || s.id == *name) {
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
        match scanner.scan_source(source).await {
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
async fn use_source(name: Option<String>, clear: bool) -> anyhow::Result<()> {
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
    let db = Database::open(&db_path).await.map_err(|e| {
        HelpfulError::new(format!("Failed to open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the directory exists and is writable")
    })?;

    let sources = db.list_sources().await?;
    let source = sources.iter().find(|s| s.name == source_name || s.id == source_name);

    let source = match source {
        Some(s) => s,
        None => {
            return Err(HelpfulError::new(format!("Source not found: {}", source_name))
                .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                .into());
        }
    };

    // Set the context
    context::set_default_source(&source.name)?;
    println!("Default source set to: {}", source.name);
    println!();
    println!("Now you can run:");
    println!("  casparian files              # Files from '{}'", source.name);
    println!("  casparian files --all        # Files from all sources");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian::scout::ScannedFile;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_add_source_path_validation() {
        let temp = TempDir::new().unwrap();
        let db = Database::open_in_memory().await.unwrap();

        // Test adding a valid directory
        let test_dir = temp.path().join("test_data");
        fs::create_dir(&test_dir).unwrap();

        let source = Source {
            id: "test-1".to_string(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: test_dir.display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

        let sources = db.list_sources().await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "test");
    }

    #[tokio::test]
    async fn test_source_stats() {
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

        // Add some files
        let file = ScannedFile::new("src-1", "/data/test.csv", "test.csv", 1000, 12345);
        db.upsert_file(&file).await.unwrap();

        let stats = get_source_stats(&db, "src-1").await;
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.total_size, 1000);
    }

    #[tokio::test]
    async fn test_remove_source_with_files() {
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

        // Add a file
        let file = ScannedFile::new("src-1", "/data/test.csv", "test.csv", 1000, 12345);
        db.upsert_file(&file).await.unwrap();

        // Delete source should remove files too
        db.delete_source("src-1").await.unwrap();
        let sources = db.list_sources().await.unwrap();
        assert!(sources.is_empty());
    }
}

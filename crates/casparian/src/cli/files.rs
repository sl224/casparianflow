//! Files command - List discovered files
//!
//! Query scout_files table with filters:
//! - `--source <source>` - Filter by source (uses default context if not specified)
//! - `--all` - Show files from all sources (override default context)
//! - `--topic <topic>` - Filter by tag/topic
//! - `--status <status>` - Filter by status (pending, processing, done, failed)
//! - `--untagged` - Show only untagged files
//! - `--pattern <pattern>` - Filter by gitignore-style pattern
//! - `--tag <tag>` - Tag matching files
//! - `--limit <n>` - Maximum files to display
//!
//! Uses casparian::scout::Database as the single source of truth.

use crate::cli::context;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table_colored};
use crate::cli::workspace;
use casparian::scout::{Database, FileStatus, Source, SourceId, WorkspaceId};
use casparian_db::DbValue;
use chrono::Utc;
use comfy_table::Color;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Arguments for the files command
#[derive(Debug)]
pub struct FilesArgs {
    pub source: Option<String>,
    pub all: bool,
    pub topic: Option<String>,
    pub status: Option<String>,
    pub untagged: bool,
    pub patterns: Vec<String>,
    pub tag: Option<String>,
    pub limit: usize,
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct FilesOutput {
    files: Vec<FileOutput>,
    summary: FilesSummary,
    filters: FilesFilters,
}

#[derive(Debug, Serialize)]
struct FileOutput {
    id: Option<i64>,
    source_id: SourceId,
    path: String,
    rel_path: String,
    size: u64,
    status: String,
    tags: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct FilesSummary {
    total: usize,
    returned: usize,
    limit: usize,
    tagged: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FilesFilters {
    source_id: Option<SourceId>,
    all_sources: bool,
    topic: Option<String>,
    status: Option<String>,
    untagged: bool,
    patterns: Vec<String>,
    tag: Option<String>,
}

#[derive(Debug, Clone)]
struct FileRow {
    id: i64,
    source_id: SourceId,
    path: String,
    rel_path: String,
    size: u64,
    status: FileStatus,
    error: Option<String>,
}

fn ensure_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    workspace::resolve_active_workspace_id(db)
        .map_err(|e| e.with_context("The workspace registry is required for files"))
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

fn load_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    source_ids: &[SourceId],
    status: Option<FileStatus>,
    topic: Option<&str>,
    untagged: bool,
    limit: usize,
) -> Result<Vec<FileRow>, HelpfulError> {
    let mut sql = String::from(
        "SELECT f.id, f.source_id, f.path, f.rel_path, f.size, f.status, f.error \
         FROM scout_files f ",
    );
    if topic.is_some() {
        sql.push_str(
            "JOIN scout_file_tags t ON t.file_id = f.id AND t.workspace_id = f.workspace_id ",
        );
    } else if untagged {
        sql.push_str(
            "LEFT JOIN scout_file_tags t ON t.file_id = f.id AND t.workspace_id = f.workspace_id ",
        );
    }
    sql.push_str("WHERE f.workspace_id = ? ");

    let mut params: Vec<DbValue> = vec![DbValue::from(workspace_id.to_string())];

    if let Some(tag) = topic {
        sql.push_str("AND t.tag = ? ");
        params.push(DbValue::from(tag));
    } else if untagged {
        sql.push_str("AND t.file_id IS NULL ");
    }

    if !source_ids.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(source_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!("AND f.source_id IN ({}) ", placeholders));
        for source_id in source_ids {
            params.push(DbValue::from(source_id.as_i64()));
        }
    }

    if let Some(status) = status {
        sql.push_str("AND f.status = ? ");
        params.push(DbValue::from(status.as_str()));
    }

    sql.push_str("ORDER BY f.mtime DESC LIMIT ? ");
    params.push(DbValue::from(limit as i64));

    let rows = conn
        .query_all(&sql, &params)
        .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;

    let mut files = Vec::new();
    for row in rows {
        let status_raw: String = row
            .get_by_name("status")
            .map_err(|e| HelpfulError::new(format!("Failed to read status: {}", e)))?;
        let status = FileStatus::parse(&status_raw).ok_or_else(|| {
            HelpfulError::new(format!("Invalid file status in database: {}", status_raw))
        })?;
        let source_id_raw: i64 = row
            .get_by_name("source_id")
            .map_err(|e| HelpfulError::new(format!("Failed to read source_id: {}", e)))?;
        let source_id = SourceId::try_from(source_id_raw)
            .map_err(|e| HelpfulError::new(format!("Invalid source_id: {}", e)))?;
        let size_raw: i64 = row
            .get_by_name("size")
            .map_err(|e| HelpfulError::new(format!("Failed to read size: {}", e)))?;
        files.push(FileRow {
            id: row
                .get_by_name("id")
                .map_err(|e| HelpfulError::new(format!("Failed to read id: {}", e)))?,
            source_id,
            path: row
                .get_by_name("path")
                .map_err(|e| HelpfulError::new(format!("Failed to read path: {}", e)))?,
            rel_path: row
                .get_by_name("rel_path")
                .map_err(|e| HelpfulError::new(format!("Failed to read rel_path: {}", e)))?,
            size: size_raw as u64,
            status,
            error: row
                .get_by_name("error")
                .map_err(|e| HelpfulError::new(format!("Failed to read error: {}", e)))?,
        });
    }

    Ok(files)
}

fn fetch_tags_for_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    file_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, HelpfulError> {
    let mut tags_by_file: HashMap<i64, Vec<String>> = HashMap::new();
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
        .map_err(|e| HelpfulError::new(format!("Failed to query file tags: {}", e)))?;

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

fn apply_manual_tag(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
    file_ids: &[i64],
    tag: &str,
) -> Result<u64, HelpfulError> {
    if file_ids.is_empty() {
        return Ok(0);
    }

    let mut tagged = 0u64;
    for file_id in file_ids {
        let result = conn
            .execute(
                "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at) \
                 VALUES (?, ?, ?, 'manual', NULL, ?) \
                 ON CONFLICT (workspace_id, file_id, tag) DO UPDATE SET \
                    tag_source = excluded.tag_source, \
                    created_at = excluded.created_at",
                &[
                    DbValue::from(workspace_id.to_string()),
                    DbValue::from(*file_id),
                    DbValue::from(tag),
                    DbValue::from(now_millis()),
                ],
            )
            .map_err(|e| HelpfulError::new(format!("Failed to tag files: {}", e)))?;
        tagged += result as u64;
        let updated = conn
            .execute(
                "UPDATE scout_files SET status = ? WHERE id = ?",
                &[
                    DbValue::from(FileStatus::Tagged.as_str()),
                    DbValue::from(*file_id),
                ],
            )
            .map_err(|e| HelpfulError::new(format!("Failed to update file status: {}", e)))?;
        if updated == 0 {
            return Err(HelpfulError::new(format!(
                "Tag applied but file not found (id: {})",
                file_id
            )));
        }
    }

    Ok(tagged)
}

fn count_total_files(
    conn: &casparian_db::DbConnection,
    workspace_id: &WorkspaceId,
) -> Result<u64, HelpfulError> {
    let total = conn
        .query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_files WHERE workspace_id = ?",
            &[DbValue::from(workspace_id.to_string())],
        )
        .map_err(|e| HelpfulError::new(format!("Failed to get stats: {}", e)))?;
    Ok(total as u64)
}

fn valid_statuses_list() -> String {
    FileStatus::ALL
        .iter()
        .map(|status| status.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_source<'a>(sources: &'a [Source], input: &str) -> Option<&'a Source> {
    let parsed_id = SourceId::parse(input).ok();
    sources
        .iter()
        .find(|s| s.name == input || parsed_id.map_or(false, |id| s.id == id))
}

/// Get the active database path
fn get_db_path() -> PathBuf {
    crate::cli::config::state_store_path()
}

/// Validate status filter
fn validate_status(status: &str) -> Result<FileStatus, HelpfulError> {
    let status_lower = status.to_lowercase();

    // Map common aliases
    let normalized = match status_lower.as_str() {
        "done" => "processed",
        s => s,
    };

    let valid_statuses = valid_statuses_list();
    FileStatus::parse(normalized).ok_or_else(|| {
        HelpfulError::new(format!("Invalid status: '{}'", status))
            .with_context("Status must be one of the valid file statuses")
            .with_suggestions([
                format!("TRY: Valid statuses: {}", valid_statuses),
                format!(
                    "TRY: Use 'done' as an alias for '{}'",
                    FileStatus::Processed.as_str()
                ),
            ])
    })
}

/// Get color for status display
fn color_for_status(status: &str) -> Color {
    match FileStatus::parse(status) {
        Some(FileStatus::Pending) => Color::Yellow,
        Some(FileStatus::Tagged) => Color::Blue,
        Some(FileStatus::Queued) => Color::Cyan,
        Some(FileStatus::Processing) => Color::Magenta,
        Some(FileStatus::Processed) => Color::Green,
        Some(FileStatus::Failed) => Color::Red,
        Some(FileStatus::Skipped) => Color::Grey,
        Some(FileStatus::Deleted) => Color::DarkGrey,
        None => Color::White,
    }
}

/// Build a GlobSet from pattern strings
fn build_glob_set(patterns: &[String]) -> anyhow::Result<(GlobSet, GlobSet)> {
    let mut include_builder = GlobSetBuilder::new();
    let mut exclude_builder = GlobSetBuilder::new();

    for pattern in patterns {
        if let Some(stripped) = pattern.strip_prefix('!') {
            exclude_builder.add(Glob::new(stripped)?);
        } else {
            include_builder.add(Glob::new(pattern)?);
        }
    }

    Ok((include_builder.build()?, exclude_builder.build()?))
}

/// Check if a path matches the pattern filters
fn matches_patterns(
    rel_path: &str,
    include_set: &GlobSet,
    exclude_set: &GlobSet,
    has_includes: bool,
) -> bool {
    if exclude_set.is_match(rel_path) {
        return false;
    }
    if !has_includes {
        return true;
    }
    include_set.is_match(rel_path)
}

/// Execute the files command (async version)
pub fn run(args: FilesArgs) -> anyhow::Result<()> {
    // Validate status if provided
    let validated_status = args
        .status
        .as_ref()
        .map(|s| validate_status(s))
        .transpose()?;

    // Open database
    let db_path = get_db_path();
    if !db_path.exists() {
        return Err(
            HelpfulError::new(format!("Database not found: {}", db_path.display()))
                .with_context("The Scout database has not been initialized yet")
                .with_suggestions([
                    "TRY: Run `casparian scan <directory>` to discover files".to_string(),
                    "TRY: Run `casparian source add /path/to/data` to add a source".to_string(),
                ])
                .into(),
        );
    }

    let db = Database::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Cannot open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the database file is not corrupted or locked")
    })?;
    let workspace_id = ensure_workspace_id(&db)?;
    let conn = db.conn();

    // Determine which source(s) to query
    // Priority: explicit --source > --all > default context > all sources (with hint)
    let sources = db
        .list_sources(&workspace_id)
        .map_err(|e| HelpfulError::new(format!("Failed to list sources: {}", e)))?;

    let (source_ids, source_context_name, source_context_msg): (
        Vec<SourceId>,
        Option<String>,
        Option<String>,
    ) = if let Some(ref source_name) = args.source {
        // Explicit --source flag
        let source = find_source(&sources, source_name);
        match source {
            Some(s) => (
                vec![s.id.clone()],
                Some(s.name.clone()),
                Some(format!("[{}]", s.name)),
            ),
            None => {
                return Err(
                    HelpfulError::new(format!("Source not found: {}", source_name))
                        .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                        .into(),
                );
            }
        }
    } else if args.all {
        // --all flag: query all sources
        (sources.iter().map(|s| s.id.clone()).collect(), None, None)
    } else if let Some(default_source) = context::get_default_source().map_err(|e| {
        HelpfulError::new(format!("Failed to load context: {}", e))
            .with_suggestion("TRY: Delete ~/.casparian_flow/context.toml to reset".to_string())
    })? {
        // Use default context
        let source = find_source(&sources, &default_source);
        match source {
            Some(s) => (
                vec![s.id.clone()],
                Some(s.name.clone()),
                Some(format!("[{}]", s.name)),
            ),
            None => {
                // Default source no longer exists - clear it and show all
                context::clear_default_source().map_err(|e| {
                    HelpfulError::new(format!("Failed to clear context: {}", e)).with_suggestion(
                        "TRY: Delete ~/.casparian_flow/context.toml to reset".to_string(),
                    )
                })?;
                (sources.iter().map(|s| s.id.clone()).collect(), None, None)
            }
        }
    } else {
        // No context set - query all sources
        (sources.iter().map(|s| s.id.clone()).collect(), None, None)
    };

    let query_limit = args.limit.max(10_000);
    let untagged = args.untagged && args.topic.is_none();

    // Query files based on filters, restricted to selected sources
    let all_files: Vec<FileRow> = load_files(
        conn,
        &workspace_id,
        &source_ids,
        validated_status,
        args.topic.as_deref(),
        untagged,
        query_limit,
    )?;

    // Apply pattern filtering in memory (limit applied after)
    let filtered_files: Vec<FileRow> = if args.patterns.is_empty() {
        all_files
    } else {
        let (include_set, exclude_set) = build_glob_set(&args.patterns)?;
        let has_includes = args.patterns.iter().any(|p| !p.starts_with('!'));
        all_files
            .into_iter()
            .filter(|f| matches_patterns(&f.rel_path, &include_set, &exclude_set, has_includes))
            .collect()
    };
    let total_matching = filtered_files.len();
    let files: Vec<FileRow> = filtered_files.into_iter().take(args.limit).collect();

    let normalized_status = validated_status
        .as_ref()
        .map(|status| status.as_str().to_string());
    let all_sources = args.all || (args.source.is_none() && source_context_name.is_none());
    let source_id_filter = if all_sources {
        None
    } else {
        source_ids.first().copied()
    };

    // Handle empty results
    if files.is_empty() {
        if args.json {
            let output = FilesOutput {
                files: Vec::new(),
                summary: FilesSummary {
                    total: total_matching,
                    returned: 0,
                    limit: args.limit,
                    tagged: args.tag.as_ref().map(|_| 0),
                },
                filters: FilesFilters {
                    source_id: source_id_filter,
                    all_sources,
                    topic: args.topic.clone(),
                    status: normalized_status.clone(),
                    untagged: args.untagged,
                    patterns: args.patterns.clone(),
                    tag: args.tag.clone(),
                },
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        if let Some(ref ctx) = source_context_msg {
            println!("{} No files found.", ctx);
        } else {
            println!("No files found matching the filters.");
        }
        println!();

        // Show what filters were applied
        let mut applied_filters: Vec<String> = Vec::new();
        if let Some(topic) = &args.topic {
            applied_filters.push(format!("topic={}", topic));
        }
        if let Some(status) = &validated_status {
            applied_filters.push(format!("status={}", status.as_str()));
        }
        if args.untagged {
            applied_filters.push("untagged=true".to_string());
        }

        if !applied_filters.is_empty() {
            println!("Applied filters: {}", applied_filters.join(", "));
        }

        // Get total file count
        let total_files = count_total_files(conn, &workspace_id)?;

        if total_files > 0 {
            println!();
            if source_context_msg.is_some() {
                println!(
                    "Hint: There are {} total files across all sources.",
                    total_files
                );
                println!("TRY: casparian files --all   (to see files from all sources)");
            } else {
                println!(
                    "Hint: There are {} total files in the database.",
                    total_files
                );
                println!("TRY: casparian files   (to see all files)");
            }
        }

        return Ok(());
    }

    // Tag files if requested
    let tagged_count = if let Some(ref new_tag) = args.tag {
        let ids: Vec<i64> = files.iter().map(|f| f.id).collect();
        let tagged = apply_manual_tag(conn, &workspace_id, &ids, new_tag)?;
        if !args.json {
            println!("Tagged {} files with: \x1b[36m{}\x1b[0m", tagged, new_tag);
            println!();
        }
        Some(tagged as usize)
    } else {
        None
    };

    let file_ids: Vec<i64> = files.iter().map(|f| f.id).collect();
    let tags_by_file = fetch_tags_for_files(conn, &workspace_id, &file_ids)?;

    if args.json {
        let files_output: Vec<FileOutput> = files
            .iter()
            .map(|f| FileOutput {
                id: Some(f.id),
                source_id: f.source_id,
                path: f.path.clone(),
                rel_path: f.rel_path.clone(),
                size: f.size,
                status: f.status.as_str().to_string(),
                tags: tags_by_file.get(&f.id).cloned().unwrap_or_default(),
                error: f.error.clone(),
            })
            .collect();

        let output = FilesOutput {
            files: files_output,
            summary: FilesSummary {
                total: total_matching,
                returned: files.len(),
                limit: args.limit,
                tagged: tagged_count,
            },
            filters: FilesFilters {
                source_id: source_id_filter,
                all_sources,
                topic: args.topic.clone(),
                status: normalized_status.clone(),
                untagged: args.untagged,
                patterns: args.patterns.clone(),
                tag: args.tag.clone(),
            },
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Print header with source context and filter summary
    if let Some(ref ctx) = source_context_msg {
        print!("{} ", ctx);
    }

    let mut filter_desc = Vec::new();
    if let Some(topic) = &args.topic {
        filter_desc.push(format!("topic: {}", topic));
    }
    if let Some(status) = &validated_status {
        filter_desc.push(format!("status: {}", status.as_str()));
    }
    if args.untagged {
        filter_desc.push("untagged".to_string());
    }
    if !args.patterns.is_empty() {
        filter_desc.push(format!("patterns: {:?}", args.patterns));
    }

    if !filter_desc.is_empty() {
        println!("Files matching: {}", filter_desc.join(", "));
    } else if source_context_msg.is_some() {
        println!("{} files", files.len());
    }
    println!();

    // Build table rows
    let headers = &["PATH", "SIZE", "TAGS", "STATUS", "ERROR"];
    let rows: Vec<Vec<(String, Option<Color>)>> = files
        .iter()
        .map(|f| {
            let tag_list = tags_by_file.get(&f.id).cloned().unwrap_or_default();
            let topic_display = if tag_list.is_empty() {
                "-".to_string()
            } else {
                tag_list.join(", ")
            };
            let error_display = f.error.as_deref().unwrap_or("-").to_string();
            let status_str = f.status.as_str();

            // Use rel_path for display (shorter and more relevant)
            let path_display = if f.rel_path.len() > 50 {
                format!("...{}", &f.rel_path[f.rel_path.len().saturating_sub(47)..])
            } else {
                f.rel_path.clone()
            };

            let error_truncated = if error_display.len() > 30 {
                format!("{}...", &error_display[..27])
            } else {
                error_display.clone()
            };

            vec![
                (path_display, None),
                (format_size(f.size), None),
                (topic_display, Some(Color::Cyan)),
                (status_str.to_string(), Some(color_for_status(status_str))),
                (
                    error_truncated,
                    if f.error.is_some() {
                        Some(Color::Red)
                    } else {
                        None
                    },
                ),
            ]
        })
        .collect();

    print_table_colored(headers, rows);

    // Print summary
    println!();
    println!("{} files", files.len());

    if files.len() >= args.limit {
        println!();
        println!(
            "Hint: Results limited to {}. Use --limit to see more.",
            args.limit
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_status() {
        let pending = FileStatus::Pending.as_str();
        let pending_upper = pending.to_ascii_uppercase();
        assert!(validate_status(pending).is_ok());
        assert!(validate_status(&pending_upper).is_ok());
        assert!(validate_status("done").is_ok()); // Alias for processed
        assert!(validate_status(FileStatus::Failed.as_str()).is_ok());
        assert!(validate_status("invalid").is_err());
    }

    #[test]
    fn test_color_for_status() {
        assert!(matches!(
            color_for_status(FileStatus::Pending.as_str()),
            Color::Yellow
        ));
        assert!(matches!(
            color_for_status(FileStatus::Failed.as_str()),
            Color::Red
        ));
        assert!(matches!(
            color_for_status(FileStatus::Processed.as_str()),
            Color::Green
        ));
    }

    #[test]
    fn test_pattern_matching() {
        let patterns = vec!["*.csv".to_string(), "!test/**".to_string()];
        let (include, exclude) = build_glob_set(&patterns).unwrap();

        // Test include pattern
        assert!(matches_patterns("data.csv", &include, &exclude, true));
        assert!(!matches_patterns("data.json", &include, &exclude, true));

        // Test exclude pattern
        assert!(!matches_patterns("test/data.csv", &include, &exclude, true));
    }
}

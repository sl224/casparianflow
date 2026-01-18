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
//! Uses crate::scout::Database as the single source of truth.

use crate::cli::context;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table_colored};
use crate::scout::{Database, FileStatus, ScannedFile};
use comfy_table::Color;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
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
    source_id: String,
    path: String,
    rel_path: String,
    size: u64,
    status: String,
    tag: Option<String>,
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
    source: Option<String>,
    all_sources: bool,
    topic: Option<String>,
    status: Option<String>,
    untagged: bool,
    patterns: Vec<String>,
    tag: Option<String>,
}

/// Valid file statuses
const VALID_STATUSES: &[&str] = &[
    "pending",
    "tagged",
    "queued",
    "processing",
    "processed",
    "failed",
    "skipped",
    "deleted",
];

/// Get the active database path
fn get_db_path() -> PathBuf {
    crate::cli::config::active_db_path()
}

/// Validate status filter
fn validate_status(status: &str) -> Result<FileStatus, HelpfulError> {
    let status_lower = status.to_lowercase();

    // Map common aliases
    let normalized = match status_lower.as_str() {
        "done" => "processed",
        s => s,
    };

    FileStatus::parse(normalized).ok_or_else(|| {
        HelpfulError::new(format!("Invalid status: '{}'", status))
            .with_context("Status must be one of the valid file statuses")
            .with_suggestions([
                format!("TRY: Valid statuses: {}", VALID_STATUSES.join(", ")),
                "TRY: Use 'done' as an alias for 'processed'".to_string(),
            ])
    })
}

/// Get color for status display
fn color_for_status(status: &str) -> Color {
    match status {
        "pending" => Color::Yellow,
        "tagged" => Color::Blue,
        "queued" => Color::Cyan,
        "processing" => Color::Magenta,
        "processed" => Color::Green,
        "failed" => Color::Red,
        "skipped" => Color::Grey,
        "deleted" => Color::DarkGrey,
        _ => Color::White,
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
pub async fn run(args: FilesArgs) -> anyhow::Result<()> {
    // Validate status if provided
    let validated_status = args.status
        .as_ref()
        .map(|s| validate_status(s))
        .transpose()?;

    // Open database
    let db_path = get_db_path();
    if !db_path.exists() {
        return Err(HelpfulError::new(format!("Database not found: {}", db_path.display()))
            .with_context("The Scout database has not been initialized yet")
            .with_suggestions([
                "TRY: Run `casparian scan <directory>` to discover files".to_string(),
                "TRY: Run `casparian source add /path/to/data` to add a source".to_string(),
            ])
            .into());
    }

    let db = Database::open(&db_path).await
        .map_err(|e| HelpfulError::new(format!("Cannot open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the database file is not corrupted or locked"))?;

    // Determine which source(s) to query
    // Priority: explicit --source > --all > default context > all sources (with hint)
    let sources = db.list_sources().await
        .map_err(|e| HelpfulError::new(format!("Failed to list sources: {}", e)))?;

    let (source_ids, source_context_name, source_context_msg): (Vec<String>, Option<String>, Option<String>) = if let Some(ref source_name) = args.source {
        // Explicit --source flag
        let source = sources.iter().find(|s| s.name == *source_name || s.id == *source_name);
        match source {
            Some(s) => (
                vec![s.id.clone()],
                Some(s.name.clone()),
                Some(format!("[{}]", s.name)),
            ),
            None => {
                return Err(HelpfulError::new(format!("Source not found: {}", source_name))
                    .with_suggestion("TRY: Use 'casparian source ls' to see available sources")
                    .into());
            }
        }
    } else if args.all {
        // --all flag: query all sources
        (sources.iter().map(|s| s.id.clone()).collect(), None, None)
    } else if let Some(default_source) = context::get_default_source() {
        // Use default context
        let source = sources.iter().find(|s| s.name == default_source || s.id == default_source);
        match source {
            Some(s) => (
                vec![s.id.clone()],
                Some(s.name.clone()),
                Some(format!("[{}]", s.name)),
            ),
            None => {
                // Default source no longer exists - clear it and show all
                let _ = context::clear_default_source();
                (sources.iter().map(|s| s.id.clone()).collect(), None, None)
            }
        }
    } else {
        // No context set - query all sources
        (sources.iter().map(|s| s.id.clone()).collect(), None, None)
    };

    // Query files based on filters, restricted to selected sources
    let all_files: Vec<ScannedFile> = if let Some(topic) = &args.topic {
        // Filter by topic - this queries across all sources, then we filter
        let topic_files = db.list_files_by_tag(topic, 10000).await
            .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;
        topic_files.into_iter()
            .filter(|f| source_ids.iter().any(|s| s.as_str() == &*f.source_id))
            .collect()
    } else if let Some(status) = &validated_status {
        // Filter by status - queries across all sources, then we filter
        let status_files = db.list_files_by_status(*status, 10000).await
            .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;
        status_files.into_iter()
            .filter(|f| source_ids.iter().any(|s| s.as_str() == &*f.source_id))
            .collect()
    } else if args.untagged {
        // Get untagged files from selected sources
        let mut untagged_files = Vec::new();
        for source_id in &source_ids {
            let files = db.list_untagged_files(source_id, 10000).await
                .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;
            untagged_files.extend(files);
        }
        untagged_files
    } else {
        // Get all files from selected sources
        let mut all = Vec::new();
        for source_id in &source_ids {
            let files = db.list_files_by_source(source_id, 10000).await
                .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?;
            all.extend(files);
        }
        all
    };

    let all_files: Vec<ScannedFile> = all_files
        .into_iter()
        .filter(|f| {
            if let Some(topic) = &args.topic {
                if f.tag.as_deref() != Some(topic) {
                    return false;
                }
            }
            if let Some(status) = &validated_status {
                if f.status != *status {
                    return false;
                }
            }
            if args.untagged && f.tag.is_some() {
                return false;
            }
            true
        })
        .collect();

    // Apply pattern filtering in memory (limit applied after)
    let filtered_files: Vec<ScannedFile> = if args.patterns.is_empty() {
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
    let files: Vec<ScannedFile> = filtered_files.into_iter().take(args.limit).collect();

    let normalized_status = validated_status.as_ref().map(|status| status.as_str().to_string());
    let all_sources = args.all || (args.source.is_none() && source_context_name.is_none());

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
                    source: args.source.clone().or(source_context_name.clone()),
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
        let stats = db.get_stats().await
            .map_err(|e| HelpfulError::new(format!("Failed to get stats: {}", e)))?;

        if stats.total_files > 0 {
            println!();
            if source_context_msg.is_some() {
                println!("Hint: There are {} total files across all sources.", stats.total_files);
                println!("TRY: casparian files --all   (to see files from all sources)");
            } else {
                println!("Hint: There are {} total files in the database.", stats.total_files);
                println!("TRY: casparian files   (to see all files)");
            }
        }

        return Ok(());
    }

    // Tag files if requested
    let tagged_count = if let Some(ref new_tag) = args.tag {
        let ids: Vec<i64> = files.iter().filter_map(|f| f.id).collect();
        let tagged = db.tag_files(&ids, new_tag).await
            .map_err(|e| HelpfulError::new(format!("Failed to tag files: {}", e)))?;
        if !args.json {
            println!(
                "Tagged {} files with: \x1b[36m{}\x1b[0m",
                tagged, new_tag
            );
            println!();
        }
        Some(tagged as usize)
    } else {
        None
    };

    if args.json {
        let files_output: Vec<FileOutput> = files
            .iter()
            .map(|f| FileOutput {
                id: f.id,
                source_id: f.source_id.as_ref().to_string(),
                path: f.path.clone(),
                rel_path: f.rel_path.clone(),
                size: f.size,
                status: f.status.as_str().to_string(),
                tag: tagged_count
                    .and_then(|_| args.tag.clone())
                    .or_else(|| f.tag.clone()),
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
                source: args.source.clone().or(source_context_name.clone()),
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
    let headers = &["PATH", "SIZE", "TOPIC", "STATUS", "ERROR"];
    let rows: Vec<Vec<(String, Option<Color>)>> = files
        .iter()
        .map(|f| {
            // Show the new tag if we just tagged, otherwise show existing tag
            let topic_display = if tagged_count.is_some() {
                args.tag.as_deref().unwrap_or("-").to_string()
            } else {
                f.tag.as_deref().unwrap_or("-").to_string()
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
                (error_truncated, if f.error.is_some() { Some(Color::Red) } else { None }),
            ]
        })
        .collect();

    print_table_colored(headers, rows);

    // Print summary
    println!();
    println!("{} files", files.len());

    if files.len() >= args.limit {
        println!();
        println!("Hint: Results limited to {}. Use --limit to see more.", args.limit);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_status() {
        assert!(validate_status("pending").is_ok());
        assert!(validate_status("PENDING").is_ok());
        assert!(validate_status("Pending").is_ok());
        assert!(validate_status("done").is_ok()); // Alias for processed
        assert!(validate_status("failed").is_ok());
        assert!(validate_status("invalid").is_err());
    }

    #[test]
    fn test_color_for_status() {
        assert!(matches!(color_for_status("pending"), Color::Yellow));
        assert!(matches!(color_for_status("failed"), Color::Red));
        assert!(matches!(color_for_status("processed"), Color::Green));
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

//! Files command - List discovered files
//!
//! Query scout_files table with filters:
//! - `--topic <topic>` - Filter by tag/topic
//! - `--status <status>` - Filter by status (pending, processing, done, failed)
//! - `--untagged` - Show only untagged files
//! - `--limit <n>` - Maximum files to display

use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table_colored};
use comfy_table::Color;
use rusqlite::Connection;
use std::path::PathBuf;

/// Arguments for the files command
#[derive(Debug)]
pub struct FilesArgs {
    pub topic: Option<String>,
    pub status: Option<String>,
    pub untagged: bool,
    pub limit: usize,
}

/// A file from the database
#[derive(Debug)]
struct ScannedFile {
    #[allow(dead_code)]
    id: i64,
    path: String,
    size: i64,
    tag: Option<String>,
    status: String,
    error: Option<String>,
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

/// Get the default database path
fn get_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("casparian_flow.sqlite3")
}

/// Open database connection with helpful error
fn open_db() -> Result<Connection, HelpfulError> {
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

    Connection::open(&db_path).map_err(|e| {
        HelpfulError::new(format!("Cannot open database: {}", e))
            .with_context(format!("Database path: {}", db_path.display()))
            .with_suggestion("TRY: Ensure the database file is not corrupted or locked")
    })
}

/// Validate status filter
fn validate_status(status: &str) -> Result<&str, HelpfulError> {
    let status_lower = status.to_lowercase();

    // Map common aliases
    let normalized = match status_lower.as_str() {
        "done" => "processed",
        s => s,
    };

    if VALID_STATUSES.contains(&normalized) {
        // Return the normalized status (static lifetime)
        Ok(match normalized {
            "pending" => "pending",
            "tagged" => "tagged",
            "queued" => "queued",
            "processing" => "processing",
            "processed" => "processed",
            "failed" => "failed",
            "skipped" => "skipped",
            "deleted" => "deleted",
            _ => unreachable!(),
        })
    } else {
        Err(HelpfulError::new(format!("Invalid status: '{}'", status))
            .with_context("Status must be one of the valid file statuses")
            .with_suggestions([
                format!("TRY: Valid statuses: {}", VALID_STATUSES.join(", ")),
                "TRY: Use 'done' as an alias for 'processed'".to_string(),
            ]))
    }
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

/// Build the query based on filters
fn build_query(args: &FilesArgs, validated_status: Option<&str>) -> (String, Vec<String>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    // Topic filter
    if let Some(topic) = &args.topic {
        conditions.push("tag = ?".to_string());
        params.push(topic.clone());
    }

    // Status filter
    if let Some(status) = validated_status {
        conditions.push("status = ?".to_string());
        params.push(status.to_string());
    }

    // Untagged filter
    if args.untagged {
        conditions.push("tag IS NULL".to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        "SELECT id, path, size, tag, status, error
         FROM scout_files
         {}
         ORDER BY path
         LIMIT ?",
        where_clause
    );

    (query, params)
}

/// Execute the files command
pub fn run(args: FilesArgs) -> anyhow::Result<()> {
    // Validate status if provided
    let validated_status = args.status
        .as_ref()
        .map(|s| validate_status(s))
        .transpose()?;

    let conn = open_db()?;

    // Build query
    let (query, params) = build_query(&args, validated_status);

    // Prepare statement
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| {
            HelpfulError::new(format!("Failed to prepare query: {}", e))
                .with_context("The scout_files table may not exist")
                .with_suggestion("TRY: Ensure the database schema is up to date")
        })?;

    // Execute query with parameters
    let limit = args.limit as i64;
    let files: Vec<ScannedFile> = match params.len() {
        0 => stmt
            .query_map([limit], |row| {
                Ok(ScannedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    size: row.get(2)?,
                    tag: row.get(3)?,
                    status: row.get(4)?,
                    error: row.get(5)?,
                })
            })
            .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?
            .filter_map(|r| r.ok())
            .collect(),
        1 => stmt
            .query_map(rusqlite::params![&params[0], limit], |row| {
                Ok(ScannedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    size: row.get(2)?,
                    tag: row.get(3)?,
                    status: row.get(4)?,
                    error: row.get(5)?,
                })
            })
            .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?
            .filter_map(|r| r.ok())
            .collect(),
        2 => stmt
            .query_map(rusqlite::params![&params[0], &params[1], limit], |row| {
                Ok(ScannedFile {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    size: row.get(2)?,
                    tag: row.get(3)?,
                    status: row.get(4)?,
                    error: row.get(5)?,
                })
            })
            .map_err(|e| HelpfulError::new(format!("Failed to query files: {}", e)))?
            .filter_map(|r| r.ok())
            .collect(),
        _ => {
            return Err(HelpfulError::new("Too many filter parameters")
                .with_context("Internal error: unexpected number of parameters")
                .into());
        }
    };

    // Handle empty results
    if files.is_empty() {
        println!("No files found matching the filters.");
        println!();

        // Show what filters were applied
        let mut applied_filters: Vec<String> = Vec::new();
        if let Some(topic) = &args.topic {
            applied_filters.push(format!("topic={}", topic));
        }
        if let Some(status) = validated_status {
            applied_filters.push(format!("status={}", status));
        }
        if args.untagged {
            applied_filters.push("untagged=true".to_string());
        }

        if !applied_filters.is_empty() {
            println!("Applied filters: {}", applied_filters.join(", "));
        }

        // Count total files
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM scout_files", [], |row| row.get(0))
            .unwrap_or(0);

        if total > 0 {
            println!();
            println!("Hint: There are {} total files in the database.", total);
            println!("TRY: casparian files   (to see all files)");
        }

        return Ok(());
    }

    // Print header with filter summary
    let mut filter_desc = Vec::new();
    if let Some(topic) = &args.topic {
        filter_desc.push(format!("topic: {}", topic));
    }
    if let Some(status) = validated_status {
        filter_desc.push(format!("status: {}", status));
    }
    if args.untagged {
        filter_desc.push("untagged".to_string());
    }

    if !filter_desc.is_empty() {
        println!("Files matching: {}", filter_desc.join(", "));
    }
    println!();

    // Build table rows
    let headers = &["PATH", "SIZE", "TOPIC", "STATUS", "ERROR"];
    let rows: Vec<Vec<(String, Option<Color>)>> = files
        .iter()
        .map(|f| {
            let topic_display = f.tag.as_deref().unwrap_or("-").to_string();
            let error_display = f.error.as_deref().unwrap_or("-").to_string();

            // Truncate long paths and errors for display
            let path_display = if f.path.len() > 50 {
                format!("...{}", &f.path[f.path.len() - 47..])
            } else {
                f.path.clone()
            };

            let error_truncated = if error_display.len() > 30 {
                format!("{}...", &error_display[..27])
            } else {
                error_display
            };

            vec![
                (path_display, None),
                (format_size(f.size as u64), None),
                (topic_display, Some(Color::Cyan)),
                (f.status.clone(), Some(color_for_status(&f.status))),
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
        assert_eq!(validate_status("pending").unwrap(), "pending");
        assert_eq!(validate_status("PENDING").unwrap(), "pending");
        assert_eq!(validate_status("Pending").unwrap(), "pending");
        assert_eq!(validate_status("done").unwrap(), "processed");
        assert_eq!(validate_status("failed").unwrap(), "failed");
        assert!(validate_status("invalid").is_err());
    }

    #[test]
    fn test_color_for_status() {
        assert!(matches!(color_for_status("pending"), Color::Yellow));
        assert!(matches!(color_for_status("failed"), Color::Red));
        assert!(matches!(color_for_status("processed"), Color::Green));
    }

    #[test]
    fn test_build_query_no_filters() {
        let args = FilesArgs {
            topic: None,
            status: None,
            untagged: false,
            limit: 50,
        };

        let (query, params) = build_query(&args, None);

        assert!(query.contains("SELECT id, path, size, tag, status, error"));
        assert!(!query.contains("WHERE"));
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_query_with_topic() {
        let args = FilesArgs {
            topic: Some("sales".to_string()),
            status: None,
            untagged: false,
            limit: 50,
        };

        let (query, params) = build_query(&args, None);

        assert!(query.contains("WHERE tag = ?"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], "sales");
    }

    #[test]
    fn test_build_query_with_status() {
        let args = FilesArgs {
            topic: None,
            status: Some("failed".to_string()),
            untagged: false,
            limit: 50,
        };

        let (query, params) = build_query(&args, Some("failed"));

        assert!(query.contains("WHERE status = ?"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], "failed");
    }

    #[test]
    fn test_build_query_untagged() {
        let args = FilesArgs {
            topic: None,
            status: None,
            untagged: true,
            limit: 50,
        };

        let (query, params) = build_query(&args, None);

        assert!(query.contains("WHERE tag IS NULL"));
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_query_combined_filters() {
        let args = FilesArgs {
            topic: Some("sales".to_string()),
            status: Some("failed".to_string()),
            untagged: false,
            limit: 50,
        };

        let (query, params) = build_query(&args, Some("failed"));

        assert!(query.contains("WHERE"));
        assert!(query.contains("tag = ?"));
        assert!(query.contains("AND"));
        assert!(query.contains("status = ?"));
        assert_eq!(params.len(), 2);
    }
}

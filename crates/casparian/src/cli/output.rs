//! Output formatting utilities for CLI commands
//!
//! Provides consistent formatting for:
//! - Tables with column alignment
//! - File sizes (human-readable)
//! - Timestamps (relative and absolute)
//! - Colors for terminal output

use comfy_table::{presets::UTF8_FULL_CONDENSED, Cell, Color, ContentArrangement, Table};
use std::time::{Duration, SystemTime};

/// Format a file size in human-readable form
///
/// Examples:
/// - 500 -> "500 B"
/// - 1024 -> "1.0 KB"
/// - 1536000 -> "1.5 MB"
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Parse a human-readable size string into bytes
///
/// Examples:
/// - "100" -> Ok(100)
/// - "1KB" -> Ok(1024)
/// - "10MB" -> Ok(10485760)
/// - "1.5GB" -> Ok(1610612736)
pub fn parse_size(size_str: &str) -> Result<u64, String> {
    let size_str = size_str.trim().to_uppercase();

    // Try to find where the number ends and unit begins
    let (num_part, unit_part) = split_number_unit(&size_str);

    let num: f64 = num_part
        .parse()
        .map_err(|_| format!("Invalid number: '{}'", num_part))?;

    let multiplier: u64 = match unit_part {
        "" | "B" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        "T" | "TB" => 1024 * 1024 * 1024 * 1024,
        _ => return Err(format!("Unknown unit: '{}'", unit_part)),
    };

    Ok((num * multiplier as f64) as u64)
}

/// Split a size string into number and unit parts
fn split_number_unit(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| i)
        .unwrap_or(s.len());

    (&s[..idx], &s[idx..])
}

/// Format a system time as a human-readable relative time
///
/// Examples:
/// - "2 seconds ago"
/// - "5 minutes ago"
/// - "3 hours ago"
/// - "2024-12-15 14:30" (if older than 24 hours)
pub fn format_time(time: SystemTime) -> String {
    let now = SystemTime::now();

    match now.duration_since(time) {
        Ok(duration) => format_duration_ago(duration),
        Err(_) => {
            // Time is in the future (shouldn't happen, but handle it)
            "just now".to_string()
        }
    }
}

/// Format a duration as "X time ago"
fn format_duration_ago(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs < 60 {
        format!("{} second{} ago", secs, if secs == 1 { "" } else { "s" })
    } else if secs < 3600 {
        let mins = secs / 60;
        format!("{} minute{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if secs < 86400 {
        let hours = secs / 3600;
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else if secs < 604800 {
        let days = secs / 86400;
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else {
        // Format as absolute date for older times
        format_absolute_time(duration)
    }
}

/// Format an absolute timestamp
fn format_absolute_time(duration_ago: Duration) -> String {
    use chrono::Local;

    let now = Local::now();
    let time = now - chrono::Duration::seconds(duration_ago.as_secs() as i64);
    time.format("%Y-%m-%d %H:%M").to_string()
}

/// Format a system time as an absolute timestamp
#[allow(dead_code)]
pub fn format_time_absolute(time: SystemTime) -> String {
    use chrono::{DateTime, Local};

    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Print a table with headers and rows
pub fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add header row with styling
    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| Cell::new(h).fg(Color::Cyan))
        .collect();
    table.set_header(header_cells);

    // Add data rows
    for row in rows {
        table.add_row(row);
    }

    println!("{}", table);
}

/// Print a table with custom column colors
pub fn print_table_colored(headers: &[&str], rows: Vec<Vec<(String, Option<Color>)>>) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add header row
    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| Cell::new(h).fg(Color::Cyan))
        .collect();
    table.set_header(header_cells);

    // Add data rows with colors
    for row in rows {
        let cells: Vec<Cell> = row
            .into_iter()
            .map(|(text, color)| {
                let cell = Cell::new(text);
                if let Some(c) = color {
                    cell.fg(c)
                } else {
                    cell
                }
            })
            .collect();
        table.add_row(cells);
    }

    println!("{}", table);
}

/// Color for file type indicators
pub fn color_for_extension(ext: &str) -> Color {
    match ext.to_lowercase().as_str() {
        "csv" | "tsv" => Color::Green,
        "json" | "jsonl" | "ndjson" => Color::Yellow,
        "parquet" | "pq" => Color::Magenta,
        "txt" | "log" => Color::White,
        "gz" | "zip" | "tar" => Color::Blue,
        _ => Color::Grey,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1099511627776), "1.0 TB");
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100").unwrap(), 100);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1kb").unwrap(), 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5KB").unwrap(), 1536);
    }

    #[test]
    fn test_parse_size_errors() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("1XB").is_err());
    }

    #[test]
    fn test_format_duration_ago() {
        assert_eq!(format_duration_ago(Duration::from_secs(5)), "5 seconds ago");
        assert_eq!(format_duration_ago(Duration::from_secs(1)), "1 second ago");
        assert_eq!(format_duration_ago(Duration::from_secs(120)), "2 minutes ago");
        assert_eq!(format_duration_ago(Duration::from_secs(3600)), "1 hour ago");
        assert_eq!(format_duration_ago(Duration::from_secs(86400)), "1 day ago");
    }

    #[test]
    fn test_split_number_unit() {
        assert_eq!(split_number_unit("100"), ("100", ""));
        assert_eq!(split_number_unit("10KB"), ("10", "KB"));
        assert_eq!(split_number_unit("1.5MB"), ("1.5", "MB"));
    }
}

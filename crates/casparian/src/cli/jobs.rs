//! Jobs command - List processing jobs
//!
//! Lists jobs from the cf_processing_queue table with filtering and formatting.

use crate::cli::config;
use crate::cli::error::HelpfulError;
use crate::cli::output::print_table_colored;
use comfy_table::Color;
use serde::Serialize;
use std::path::PathBuf;

/// Arguments for the jobs command
#[derive(Debug)]
pub struct JobsArgs {
    pub topic: Option<String>,
    pub pending: bool,
    pub running: bool,
    pub failed: bool,
    pub done: bool,
    pub limit: usize,
}

/// Job status (matches database enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Pending,
    Skipped,
}

impl JobStatus {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "QUEUED" => Self::Queued,
            "RUNNING" => Self::Running,
            "COMPLETED" => Self::Completed,
            "FAILED" => Self::Failed,
            "PENDING" => Self::Pending,
            "SKIPPED" => Self::Skipped,
            _ => Self::Pending,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Pending => "PENDING",
            Self::Skipped => "SKIPPED",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Queued => Color::Yellow,
            Self::Running => Color::Cyan,
            Self::Completed => Color::Green,
            Self::Failed => Color::Red,
            Self::Pending => Color::Grey,
            Self::Skipped => Color::DarkGrey,
        }
    }
}

/// A job from the processing queue
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: i64,
    pub file_path: String,
    pub plugin_name: String,
    pub status: JobStatus,
    pub priority: i32,
    pub claim_time: Option<String>,
    pub end_time: Option<String>,
    pub error_message: Option<String>,
    pub result_summary: Option<String>,
    pub retry_count: i32,
}

/// Queue statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct QueueStats {
    pub total: i64,
    pub queued: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
}

/// Execute the jobs command
pub fn run(args: JobsArgs) -> anyhow::Result<()> {
    // Build database path
    let db_path = get_db_path()?;

    // Check database exists
    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestion("TRY: casparian start   # Start the server to create the database")
            .with_suggestion("TRY: Check CASPARIAN_DB environment variable")
            .into());
    }

    // Run the async query
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        run_async(args, &db_path).await
    })
}

async fn run_async(args: JobsArgs, db_path: &PathBuf) -> anyhow::Result<()> {
    let db_url = format!("sqlite:{}", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .map_err(|e| {
            HelpfulError::new("Failed to connect to database")
                .with_context(format!("Database: {}", db_path.display()))
                .with_suggestion(format!("Error: {}", e))
                .with_suggestion("TRY: Check file permissions")
                .with_suggestion("TRY: Ensure database is not locked by another process")
        })?;

    // Get queue statistics
    let stats = get_queue_stats(&pool).await?;

    // Build filter based on flags
    let status_filter = build_status_filter(&args);

    // Get jobs
    let jobs = get_jobs(&pool, &args.topic, &status_filter, args.limit).await?;

    // Output
    print_queue_status(&stats);
    println!();
    print_jobs_table(&jobs, args.limit);

    Ok(())
}

/// Get the database path from environment or default
pub fn get_db_path() -> anyhow::Result<PathBuf> {
    Ok(config::resolve_db_path(None))
}

/// Build status filter from command flags
fn build_status_filter(args: &JobsArgs) -> Vec<&'static str> {
    let mut statuses = Vec::new();

    if args.pending {
        statuses.push("QUEUED");
        statuses.push("PENDING");
    }
    if args.running {
        statuses.push("RUNNING");
    }
    if args.failed {
        statuses.push("FAILED");
    }
    if args.done {
        statuses.push("COMPLETED");
    }

    // If no specific filter, show all
    if statuses.is_empty() {
        statuses.extend(&["QUEUED", "RUNNING", "COMPLETED", "FAILED", "PENDING"]);
    }

    statuses
}

/// Get queue statistics
async fn get_queue_stats(pool: &sqlx::SqlitePool) -> anyhow::Result<QueueStats> {
    // Check if table exists first
    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cf_processing_queue'"
    )
    .fetch_optional(pool)
    .await?;

    if table_exists.is_none() {
        return Ok(QueueStats::default());
    }

    let row: (i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'QUEUED' THEN 1 ELSE 0 END) as queued,
            SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) as running,
            SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as completed,
            SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) as failed
        FROM cf_processing_queue
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0, 0, 0, 0, 0));

    Ok(QueueStats {
        total: row.0,
        queued: row.1,
        running: row.2,
        completed: row.3,
        failed: row.4,
    })
}

/// Get jobs matching filter criteria
async fn get_jobs(
    pool: &sqlx::SqlitePool,
    topic: &Option<String>,
    statuses: &[&str],
    limit: usize,
) -> anyhow::Result<Vec<Job>> {
    // Check if table exists
    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cf_processing_queue'"
    )
    .fetch_optional(pool)
    .await?;

    if table_exists.is_none() {
        return Ok(Vec::new());
    }

    // Build query dynamically based on filters
    let status_placeholders: String = statuses.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

    let base_query = if topic.is_some() {
        format!(
            r#"
            SELECT
                q.id,
                COALESCE(sr.path || '/' || fl.rel_path, 'unknown') as file_path,
                q.plugin_name,
                q.status,
                q.priority,
                q.claim_time,
                q.end_time,
                q.error_message,
                q.result_summary,
                q.retry_count
            FROM cf_processing_queue q
            LEFT JOIN cf_file_version fv ON fv.id = q.file_version_id
            LEFT JOIN cf_file_location fl ON fl.id = fv.location_id
            LEFT JOIN cf_source_root sr ON sr.id = fl.source_root_id
            WHERE q.status IN ({})
              AND q.plugin_name = ?
            ORDER BY q.id DESC
            LIMIT ?
            "#,
            status_placeholders
        )
    } else {
        format!(
            r#"
            SELECT
                q.id,
                COALESCE(sr.path || '/' || fl.rel_path, 'unknown') as file_path,
                q.plugin_name,
                q.status,
                q.priority,
                q.claim_time,
                q.end_time,
                q.error_message,
                q.result_summary,
                q.retry_count
            FROM cf_processing_queue q
            LEFT JOIN cf_file_version fv ON fv.id = q.file_version_id
            LEFT JOIN cf_file_location fl ON fl.id = fv.location_id
            LEFT JOIN cf_source_root sr ON sr.id = fl.source_root_id
            WHERE q.status IN ({})
            ORDER BY q.id DESC
            LIMIT ?
            "#,
            status_placeholders
        )
    };

    // Build and execute query
    let mut query = sqlx::query_as::<_, (i64, String, String, String, i32, Option<String>, Option<String>, Option<String>, Option<String>, i32)>(&base_query);

    for status in statuses {
        query = query.bind(*status);
    }

    if let Some(t) = topic {
        query = query.bind(t);
    }

    query = query.bind(limit as i64);

    let rows = query.fetch_all(pool).await?;

    let jobs: Vec<Job> = rows
        .into_iter()
        .map(|row| Job {
            id: row.0,
            file_path: row.1,
            plugin_name: row.2,
            status: JobStatus::from_str(&row.3),
            priority: row.4,
            claim_time: row.5,
            end_time: row.6,
            error_message: row.7,
            result_summary: row.8,
            retry_count: row.9,
        })
        .collect();

    Ok(jobs)
}

/// Print queue status summary
fn print_queue_status(stats: &QueueStats) {
    println!("QUEUE STATUS");
    println!("  Total:     {:>6} jobs", format_number(stats.total));
    println!("  Pending:   {:>6}", format_number(stats.queued));
    println!("  Running:   {:>6}", format_number(stats.running));
    println!("  Done:      {:>6}", format_number(stats.completed));
    println!("  Failed:    {:>6}", format_number(stats.failed));
}

/// Print jobs table
fn print_jobs_table(jobs: &[Job], limit: usize) {
    if jobs.is_empty() {
        println!("No jobs found matching the filter criteria.");
        return;
    }

    println!("JOBS (last {})", limit.min(jobs.len()));

    let headers = &["ID", "FILE", "TOPIC", "STATUS", "STARTED", "DURATION"];

    let rows: Vec<Vec<(String, Option<Color>)>> = jobs
        .iter()
        .map(|job| {
            // Truncate file path for display
            let file_display = truncate_path(&job.file_path, 40);

            // Calculate duration
            let duration = calculate_duration(&job.claim_time, &job.end_time);

            // Format start time
            let started = job.claim_time.as_ref()
                .map(|t| format_datetime(t))
                .unwrap_or_else(|| "-".to_string());

            vec![
                (job.id.to_string(), None),
                (file_display, None),
                (job.plugin_name.clone(), None),
                (job.status.as_str().to_string(), Some(job.status.color())),
                (started, None),
                (duration, None),
            ]
        })
        .collect();

    print_table_colored(headers, rows);
}

/// Truncate a path for display
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    // Keep the filename and truncate from the left
    let parts: Vec<&str> = path.rsplitn(2, '/').collect();
    if parts.len() == 2 {
        let filename = parts[0];
        let dir = parts[1];

        if filename.len() >= max_len - 4 {
            // Filename itself is too long
            return format!("...{}", &filename[filename.len().saturating_sub(max_len - 3)..]);
        }

        let available = max_len - filename.len() - 4; // 4 for ".../""
        let truncated_dir = &dir[dir.len().saturating_sub(available)..];
        format!("...{}/{}", truncated_dir, filename)
    } else {
        format!("...{}", &path[path.len().saturating_sub(max_len - 3)..])
    }
}

/// Calculate duration between start and end times
fn calculate_duration(start: &Option<String>, end: &Option<String>) -> String {
    match (start, end) {
        (Some(s), Some(e)) => {
            // Parse timestamps and calculate difference
            if let (Ok(start_dt), Ok(end_dt)) = (
                chrono::DateTime::parse_from_rfc3339(s),
                chrono::DateTime::parse_from_rfc3339(e),
            ) {
                let duration = end_dt.signed_duration_since(start_dt);
                format_duration(duration.num_seconds())
            } else {
                "-".to_string()
            }
        }
        (Some(s), None) => {
            // Job is still running
            if let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(s) {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(start_dt);
                format!("{}...", format_duration(duration.num_seconds()))
            } else {
                "...".to_string()
            }
        }
        _ => "-".to_string(),
    }
}

/// Format duration in seconds to human-readable
fn format_duration(secs: i64) -> String {
    if secs < 0 {
        return "-".to_string();
    }

    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format a datetime string for display
fn format_datetime(dt_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        dt_str.to_string()
    }
}

/// Format a number with thousands separators
fn format_number(n: i64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        format!(
            "{},{:03},{:03}",
            n / 1_000_000,
            (n % 1_000_000) / 1000,
            n % 1000
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_from_str() {
        assert_eq!(JobStatus::from_str("QUEUED"), JobStatus::Queued);
        assert_eq!(JobStatus::from_str("RUNNING"), JobStatus::Running);
        assert_eq!(JobStatus::from_str("COMPLETED"), JobStatus::Completed);
        assert_eq!(JobStatus::from_str("FAILED"), JobStatus::Failed);
        assert_eq!(JobStatus::from_str("queued"), JobStatus::Queued);
        assert_eq!(JobStatus::from_str("unknown"), JobStatus::Pending);
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("short.txt", 40), "short.txt");
        // Path is 35 chars, with max 25: keeps filename (8) + ".../X/" prefix
        let truncated = truncate_path("/a/very/long/path/to/some/file.csv", 25);
        assert!(truncated.len() <= 25, "Truncated path too long: {}", truncated);
        assert!(truncated.ends_with("file.csv"), "Should preserve filename");
        assert!(truncated.starts_with("..."), "Should start with ellipsis");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(5), "5s");
        assert_eq!(format_duration(65), "1m 5s");
        assert_eq!(format_duration(3665), "1h 1m");
        assert_eq!(format_duration(-1), "-");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_build_status_filter() {
        let args = JobsArgs {
            topic: None,
            pending: true,
            running: false,
            failed: false,
            done: false,
            limit: 50,
        };
        let filter = build_status_filter(&args);
        assert!(filter.contains(&"QUEUED"));
        assert!(filter.contains(&"PENDING"));
        assert!(!filter.contains(&"RUNNING"));
    }

    #[test]
    fn test_build_status_filter_all() {
        let args = JobsArgs {
            topic: None,
            pending: false,
            running: false,
            failed: false,
            done: false,
            limit: 50,
        };
        let filter = build_status_filter(&args);
        // Should include all statuses when none specified
        assert!(filter.len() >= 4);
    }
}

//! Jobs command - List processing jobs
//!
//! Lists jobs from the cf_processing_queue table with filtering and formatting.

use crate::cli::config;
use crate::cli::error::HelpfulError;
use crate::cli::output::{format_number_signed, print_table_colored};
use casparian_db::{DbConnection, DbTimestamp, DbValue};
use casparian_protocol::{JobId, ProcessingStatus};
use chrono::SecondsFormat;
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
    pub dead_letter: bool,
    pub limit: usize,
    pub json: bool,
}

/// Get display color for a processing status
fn status_color(status: ProcessingStatus) -> Color {
    match status {
        ProcessingStatus::Queued => Color::Yellow,
        ProcessingStatus::Running => Color::Cyan,
        ProcessingStatus::Staged => Color::Blue,
        ProcessingStatus::Completed => Color::Green,
        ProcessingStatus::Failed => Color::Red,
        ProcessingStatus::Pending => Color::Grey,
        ProcessingStatus::Skipped => Color::DarkGrey,
    }
}

/// A job from the processing queue
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: JobId,
    pub file_path: String,
    pub plugin_name: String,
    pub status: ProcessingStatus,
    pub priority: i32,
    pub claim_time: Option<String>,
    pub end_time: Option<String>,
    pub error_message: Option<String>,
    pub result_summary: Option<String>,
    pub retry_count: i32,
    pub quarantine_rows: Option<i64>,
}

/// Queue statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct QueueStats {
    pub total: i64,
    pub queued: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead_letter: i64,
}

/// A dead letter job (from cf_dead_letter table)
#[derive(Debug, Clone, Serialize)]
pub struct DeadLetterJobDisplay {
    pub id: i64,
    pub original_job_id: JobId,
    pub plugin_name: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub moved_at: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct JobsOutput {
    stats: QueueStats,
    filters: JobsFilters,
    limit: usize,
    jobs: Vec<Job>,
    dead_letter: Vec<DeadLetterJobDisplay>,
}

#[derive(Debug, Serialize)]
struct JobsFilters {
    topic: Option<String>,
    status: Vec<String>,
    dead_letter: bool,
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
            .into());
    }

    run_with_db(args, &db_path)
}

fn run_with_db(args: JobsArgs, db_path: &PathBuf) -> anyhow::Result<()> {
    let conn = DbConnection::open_duckdb_readonly(db_path).map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(format!("Database: {}", db_path.display()))
            .with_suggestion(format!("Error: {}", e))
            .with_suggestion("TRY: Check file permissions")
    })?;

    // Get queue statistics
    let stats = get_queue_stats(&conn)?;

    // Build filter based on flags
    let status_filter = build_status_filter(&args);

    if args.json {
        let (jobs, dead_letter) = if args.dead_letter {
            (
                Vec::new(),
                get_dead_letter_jobs(&conn, &args.topic, args.limit)?,
            )
        } else {
            (
                get_jobs(&conn, &args.topic, &status_filter, args.limit)?,
                Vec::new(),
            )
        };

        let output = JobsOutput {
            stats,
            filters: JobsFilters {
                topic: args.topic.clone(),
                status: if args.dead_letter {
                    Vec::new()
                } else {
                    status_filter.iter().map(|s| s.to_string()).collect()
                },
                dead_letter: args.dead_letter,
            },
            limit: args.limit,
            jobs,
            dead_letter,
        };

        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Print status header
    print_queue_status(&stats);
    println!();

    // Handle dead letter mode separately
    if args.dead_letter {
        let dead_letter_jobs = get_dead_letter_jobs(&conn, &args.topic, args.limit)?;
        print_dead_letter_table(&dead_letter_jobs, args.limit);
        return Ok(());
    }

    // Get jobs
    let jobs = get_jobs(&conn, &args.topic, &status_filter, args.limit)?;

    // Output
    print_jobs_table(&jobs, args.limit);

    Ok(())
}

/// Get the database path
pub fn get_db_path() -> anyhow::Result<PathBuf> {
    Ok(config::active_db_path())
}

/// Build status filter from command flags
fn build_status_filter(args: &JobsArgs) -> Vec<&'static str> {
    let mut statuses = Vec::new();

    if args.pending {
        statuses.push(ProcessingStatus::Queued.as_str());
        statuses.push(ProcessingStatus::Pending.as_str());
    }
    if args.running {
        statuses.push(ProcessingStatus::Running.as_str());
    }
    if args.failed {
        statuses.push(ProcessingStatus::Failed.as_str());
    }
    if args.done {
        statuses.push(ProcessingStatus::Completed.as_str());
    }

    // If no specific filter, show all
    if statuses.is_empty() {
        statuses.extend(&[
            ProcessingStatus::Queued.as_str(),
            ProcessingStatus::Running.as_str(),
            ProcessingStatus::Completed.as_str(),
            ProcessingStatus::Failed.as_str(),
            ProcessingStatus::Pending.as_str(),
        ]);
    }

    statuses
}

/// Get queue statistics

pub(crate) fn table_exists(conn: &DbConnection, table: &str) -> anyhow::Result<bool> {
    let row = conn.query_optional(
        "SELECT 1 FROM information_schema.tables WHERE table_schema = 'main' AND table_name = ?",
        &[DbValue::from(table)],
    )?;
    Ok(row.is_some())
}

pub(crate) fn column_exists(
    conn: &DbConnection,
    table: &str,
    column: &str,
) -> anyhow::Result<bool> {
    let row = conn
        .query_optional(
            "SELECT 1 FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ? AND column_name = ?",
            &[DbValue::from(table), DbValue::from(column)],
        )
        ?;
    Ok(row.is_some())
}

fn get_queue_stats(conn: &DbConnection) -> anyhow::Result<QueueStats> {
    if !table_exists(conn, "cf_processing_queue")? {
        return Ok(QueueStats::default());
    }

    let row = conn.query_one(
        &format!(
            r#"
        SELECT
            COUNT(*) as total,
            COALESCE(SUM(CASE WHEN status = '{queued}' THEN 1 ELSE 0 END), 0) as queued,
            COALESCE(SUM(CASE WHEN status = '{running}' THEN 1 ELSE 0 END), 0) as running,
            COALESCE(SUM(CASE WHEN status = '{completed}' THEN 1 ELSE 0 END), 0) as completed,
            COALESCE(SUM(CASE WHEN status = '{failed}' THEN 1 ELSE 0 END), 0) as failed
        FROM cf_processing_queue
        "#,
            queued = ProcessingStatus::Queued.as_str(),
            running = ProcessingStatus::Running.as_str(),
            completed = ProcessingStatus::Completed.as_str(),
            failed = ProcessingStatus::Failed.as_str(),
        ),
        &[],
    )?;

    let total: i64 = row.get(0)?;
    let queued: i64 = row.get(1)?;
    let running: i64 = row.get(2)?;
    let completed: i64 = row.get(3)?;
    let failed: i64 = row.get(4)?;

    let dead_letter_count = if table_exists(conn, "cf_dead_letter")? {
        conn.query_scalar::<i64>("SELECT COUNT(*) FROM cf_dead_letter", &[])
            .unwrap_or(0)
    } else {
        0
    };

    Ok(QueueStats {
        total,
        queued,
        running,
        completed,
        failed,
        dead_letter: dead_letter_count,
    })
}

/// Get dead letter jobs
fn get_dead_letter_jobs(
    conn: &DbConnection,
    topic: &Option<String>,
    limit: usize,
) -> anyhow::Result<Vec<DeadLetterJobDisplay>> {
    if !table_exists(conn, "cf_dead_letter")? {
        return Ok(Vec::new());
    }

    let mut params: Vec<DbValue> = Vec::new();
    let query = if let Some(plugin_name) = topic {
        params.push(DbValue::from(plugin_name.as_str()));
        params.push(DbValue::from(limit as i64));
        r#"
        SELECT id, original_job_id, plugin_name, error_message, retry_count, moved_at, reason
        FROM cf_dead_letter
        WHERE plugin_name = ?
        ORDER BY moved_at DESC
        LIMIT ?
        "#
    } else {
        params.push(DbValue::from(limit as i64));
        r#"
        SELECT id, original_job_id, plugin_name, error_message, retry_count, moved_at, reason
        FROM cf_dead_letter
        ORDER BY moved_at DESC
        LIMIT ?
        "#
    };

    let rows = conn.query_all(query, &params)?;
    let jobs = rows
        .into_iter()
        .map(|row| -> anyhow::Result<DeadLetterJobDisplay> {
            let id = row.get(0)?;
            let raw_job_id: i64 = row.get(1)?;
            let original_job_id = JobId::try_from(raw_job_id).map_err(|err| {
                anyhow::anyhow!("Invalid job id {} in dead letter: {}", raw_job_id, err)
            })?;
            Ok(DeadLetterJobDisplay {
                id,
                original_job_id,
                plugin_name: row.get(2)?,
                error_message: row.get(3).ok(),
                retry_count: row.get(4)?,
                moved_at: row.get(5)?,
                reason: row.get(6).ok(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(jobs)
}

/// Get jobs matching filter criteria
fn get_jobs(
    conn: &DbConnection,
    topic: &Option<String>,
    statuses: &[&str],
    limit: usize,
) -> anyhow::Result<Vec<Job>> {
    if !table_exists(conn, "cf_processing_queue")? {
        return Ok(Vec::new());
    }

    let has_quarantine_column = column_exists(conn, "cf_processing_queue", "quarantine_rows")?;
    let has_quarantine_table = table_exists(conn, "cf_quarantine")?;
    let quarantine_select = if has_quarantine_column {
        ", COALESCE(q.quarantine_rows, 0) as quarantine_rows"
    } else if has_quarantine_table {
        ", COALESCE(qc.quarantine_rows, 0) as quarantine_rows"
    } else {
        ", NULL as quarantine_rows"
    };
    let quarantine_join = if has_quarantine_column {
        ""
    } else if has_quarantine_table {
        r#"
            LEFT JOIN (
                SELECT job_id, COUNT(*) AS quarantine_rows
                FROM cf_quarantine
                GROUP BY job_id
            ) qc ON qc.job_id = q.id
        "#
    } else {
        ""
    };

    // Build query dynamically based on filters
    let status_placeholders: String = statuses.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

    let base_query = if topic.is_some() {
        format!(
            r#"
            SELECT
                q.id,
                COALESCE(sf.path, 'unknown') as file_path,
                q.plugin_name,
                q.status,
                q.priority,
                q.claim_time,
                q.end_time,
                q.error_message,
                q.result_summary,
                q.retry_count{quarantine_select}
            FROM cf_processing_queue q
            LEFT JOIN scout_files sf ON sf.id = q.file_id
            {quarantine_join}
            WHERE q.status IN ({})
              AND q.plugin_name = ?
            ORDER BY q.id DESC
            LIMIT ?
            "#,
            status_placeholders,
            quarantine_select = quarantine_select,
            quarantine_join = quarantine_join
        )
    } else {
        format!(
            r#"
            SELECT
                q.id,
                COALESCE(sf.path, 'unknown') as file_path,
                q.plugin_name,
                q.status,
                q.priority,
                q.claim_time,
                q.end_time,
                q.error_message,
                q.result_summary,
                q.retry_count{quarantine_select}
            FROM cf_processing_queue q
            LEFT JOIN scout_files sf ON sf.id = q.file_id
            {quarantine_join}
            WHERE q.status IN ({})
            ORDER BY q.id DESC
            LIMIT ?
            "#,
            status_placeholders,
            quarantine_select = quarantine_select,
            quarantine_join = quarantine_join
        )
    };

    // Build and execute query
    let mut params: Vec<DbValue> = Vec::new();
    for status in statuses {
        params.push(DbValue::from(*status));
    }
    if let Some(t) = topic {
        params.push(DbValue::from(t.as_str()));
    }
    params.push(DbValue::from(limit as i64));

    let rows = conn.query_all(&base_query, &params)?;

    let jobs = rows
        .into_iter()
        .map(|row| -> anyhow::Result<Job> {
            let raw_id: i64 = row.get(0)?;
            let id = JobId::try_from(raw_id)
                .map_err(|err| anyhow::anyhow!("Invalid job id {} in queue: {}", raw_id, err))?;
            let status_str: String = row.get(3)?;
            let status = status_str.parse::<ProcessingStatus>().map_err(|e| {
                anyhow::anyhow!("Invalid processing status '{}': {}", status_str, e)
            })?;
            Ok(Job {
                id,
                file_path: row.get(1)?,
                plugin_name: row.get(2)?,
                status,
                priority: row.get(4)?,
                claim_time: format_db_timestamp(row.get::<Option<DbTimestamp>>(5).ok().flatten()),
                end_time: format_db_timestamp(row.get::<Option<DbTimestamp>>(6).ok().flatten()),
                error_message: row.get::<Option<String>>(7).ok().flatten(),
                result_summary: row.get::<Option<String>>(8).ok().flatten(),
                retry_count: row.get(9)?,
                quarantine_rows: row.get::<Option<i64>>(10).ok().flatten().and_then(|value| {
                    if value > 0 {
                        Some(value)
                    } else {
                        None
                    }
                }),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(jobs)
}

/// Print queue status summary
fn print_queue_status(stats: &QueueStats) {
    println!("QUEUE STATUS");
    println!(
        "  Total:       {:>6} jobs",
        format_number_signed(stats.total)
    );
    println!("  Pending:     {:>6}", format_number_signed(stats.queued));
    println!("  Running:     {:>6}", format_number_signed(stats.running));
    println!(
        "  Done:        {:>6}",
        format_number_signed(stats.completed)
    );
    println!("  Failed:      {:>6}", format_number_signed(stats.failed));
    if stats.dead_letter > 0 {
        println!(
            "  Dead Letter: {:>6}",
            format_number_signed(stats.dead_letter)
        );
    }
}

/// Print jobs table
fn print_jobs_table(jobs: &[Job], limit: usize) {
    if jobs.is_empty() {
        println!("No jobs found matching the filter criteria.");
        return;
    }

    println!("JOBS (last {})", limit.min(jobs.len()));

    let headers = &[
        "ID", "FILE", "TOPIC", "STATUS", "QUAR", "STARTED", "DURATION",
    ];

    let rows: Vec<Vec<(String, Option<Color>)>> = jobs
        .iter()
        .map(|job| {
            // Truncate file path for display
            let file_display = truncate_path(&job.file_path, 40);

            // Calculate duration
            let duration = calculate_duration(&job.claim_time, &job.end_time);

            // Format start time
            let started = job
                .claim_time
                .as_ref()
                .map(|t| format_datetime(t))
                .unwrap_or_else(|| "-".to_string());
            let quarantine_display = job
                .quarantine_rows
                .map(format_number_signed)
                .unwrap_or_else(|| "-".to_string());
            let quarantine_color = match job.quarantine_rows {
                Some(rows) if rows > 0 => Some(Color::Yellow),
                _ => None,
            };

            vec![
                (job.id.to_string(), None),
                (file_display, None),
                (job.plugin_name.clone(), None),
                (
                    job.status.as_str().to_string(),
                    Some(status_color(job.status)),
                ),
                (quarantine_display, quarantine_color),
                (started, None),
                (duration, None),
            ]
        })
        .collect();

    print_table_colored(headers, rows);
}

/// Print dead letter jobs table
fn print_dead_letter_table(jobs: &[DeadLetterJobDisplay], limit: usize) {
    if jobs.is_empty() {
        println!("No dead letter jobs found.");
        println!();
        println!("Dead letter jobs are jobs that have exhausted all retries.");
        println!("TRY: casparian jobs --failed    # Show failed jobs in the main queue");
        return;
    }

    println!("DEAD LETTER QUEUE (last {})", limit.min(jobs.len()));

    let headers = &["ID", "ORIG_JOB", "PARSER", "RETRIES", "MOVED_AT", "REASON"];

    let rows: Vec<Vec<(String, Option<Color>)>> = jobs
        .iter()
        .map(|job| {
            // Format moved_at time
            let moved_at = format_datetime(&job.moved_at);

            // Truncate reason for display
            let reason = job
                .reason
                .as_ref()
                .map(|r| truncate_string(r, 30))
                .unwrap_or_else(|| "-".to_string());

            vec![
                (job.id.to_string(), None),
                (job.original_job_id.to_string(), None),
                (job.plugin_name.clone(), None),
                (job.retry_count.to_string(), Some(Color::Red)),
                (moved_at, None),
                (reason, None),
            ]
        })
        .collect();

    print_table_colored(headers, rows);

    println!();
    println!("TIP: Use 'casparian job replay <ID>' to retry a dead letter job");
}

/// Truncate a string for display
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
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
            return format!(
                "...{}",
                &filename[filename.len().saturating_sub(max_len - 3)..]
            );
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

fn format_db_timestamp(ts: Option<DbTimestamp>) -> Option<String> {
    ts.map(|value| value.as_chrono().to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Format a datetime string for display
fn format_datetime(dt_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        dt_str.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_status_from_str() {
        assert_eq!(
            ProcessingStatus::Queued
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Queued
        );
        assert_eq!(
            ProcessingStatus::Running
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Running
        );
        assert_eq!(
            ProcessingStatus::Completed
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Completed
        );
        assert_eq!(
            ProcessingStatus::Failed
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Failed
        );
        assert_eq!(
            ProcessingStatus::Queued
                .as_str()
                .to_ascii_lowercase()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Queued
        );
        assert!("unknown".parse::<ProcessingStatus>().is_err());
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("short.txt", 40), "short.txt");
        // Path is 35 chars, with max 25: keeps filename (8) + ".../X/" prefix
        let truncated = truncate_path("/a/very/long/path/to/some/file.csv", 25);
        assert!(
            truncated.len() <= 25,
            "Truncated path too long: {}",
            truncated
        );
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
    fn test_build_status_filter() {
        let args = JobsArgs {
            topic: None,
            pending: true,
            running: false,
            failed: false,
            done: false,
            limit: 50,
            dead_letter: false,
            json: false,
        };
        let filter = build_status_filter(&args);
        assert!(filter.contains(&ProcessingStatus::Queued.as_str()));
        assert!(filter.contains(&ProcessingStatus::Pending.as_str()));
        assert!(!filter.contains(&ProcessingStatus::Running.as_str()));
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
            dead_letter: false,
            json: false,
        };
        let filter = build_status_filter(&args);
        // Should include all statuses when none specified
        assert!(filter.len() >= 4);
    }
}

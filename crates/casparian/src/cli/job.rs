//! Job command - Manage individual jobs
//!
//! Commands for showing, retrying, and cancelling individual jobs.

use crate::cli::error::HelpfulError;
use crate::cli::jobs::{column_exists, get_db_path, table_exists, Job};
use crate::cli::output::format_number_signed;
use casparian_db::{DbConnection, DbValue};
use casparian_protocol::ProcessingStatus;
use clap::Subcommand;
use serde::Serialize;
use std::path::PathBuf;

/// Subcommands for job management
#[derive(Subcommand, Debug, Clone)]
pub enum JobAction {
    /// Show job details
    Show {
        /// Job ID to show
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// View job logs
    Logs {
        /// Job ID to view logs for
        id: String,
        /// Follow logs in real-time
        #[arg(short = 'f', long)]
        follow: bool,
        /// Number of lines to show
        #[arg(long)]
        tail: Option<usize>,
    },
    /// Retry a failed job
    Retry {
        /// Job ID to retry (or --all-failed)
        id: String,
    },
    /// Retry all failed jobs
    #[command(name = "retry-all")]
    RetryAll {
        /// Filter by topic/plugin
        #[arg(long)]
        topic: Option<String>,
    },
    /// Cancel a pending or running job
    Cancel {
        /// Job ID to cancel
        id: String,
    },
}

/// Detailed job information including failure details
#[derive(Debug, Clone, Serialize)]
pub struct JobDetails {
    pub job: Job,
    pub failure: Option<JobFailure>,
    pub timeline: JobTimeline,
}

/// Job failure details
#[derive(Debug, Clone, Serialize)]
pub struct JobFailure {
    pub error_type: Option<String>,
    pub error_message: String,
    pub stack_trace: Option<String>,
    pub line_number: Option<i32>,
    pub context: Option<String>,
}

/// Job timeline
#[derive(Debug, Clone, Serialize)]
pub struct JobTimeline {
    pub created: Option<String>,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub duration_secs: Option<i64>,
}

/// Execute the job command
pub fn run(action: JobAction) -> anyhow::Result<()> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestion("TRY: casparian start   # Start the server to create the database")
            .into());
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        match action {
            JobAction::Show { id, json } => run_show(&db_path, &id, json).await,
            JobAction::Logs { id, follow, tail } => run_logs(&db_path, &id, follow, tail).await,
            JobAction::Retry { id } => run_retry(&db_path, &id).await,
            JobAction::RetryAll { topic } => run_retry_all(&db_path, topic.as_deref()).await,
            JobAction::Cancel { id } => run_cancel(&db_path, &id).await,
        }
    })
}

/// Show detailed job information
async fn run_show(db_path: &PathBuf, id: &str, json: bool) -> anyhow::Result<()> {
    let job_id: i64 = id.parse().map_err(|_| {
        HelpfulError::new(format!("Invalid job ID: '{}'", id))
            .with_context("Job ID must be a positive integer")
            .with_suggestion("TRY: casparian jobs   # List jobs to find valid IDs")
    })?;

    let conn = connect_db(db_path).await?;

    // Get job details
    let job = get_job_by_id(&conn, job_id).await?;
    let Some(job) = job else {
        return Err(HelpfulError::new(format!("Job {} not found", job_id))
            .with_suggestion("TRY: casparian jobs   # List available jobs")
            .into());
    };

    // Get failure details if job failed
    let failure = if job.status == ProcessingStatus::Failed {
        get_job_failure(&conn, job_id).await?
    } else {
        None
    };

    // Build timeline
    let timeline = build_timeline(&job);

    let details = JobDetails {
        job: job.clone(),
        failure: failure.clone(),
        timeline: timeline.clone(),
    };

    if json {
        let output = serde_json::to_string_pretty(&details)?;
        println!("{}", output);
    } else {
        print_job_details(&job, &failure, &timeline);
    }

    Ok(())
}

/// View job logs (not implemented for SQLite backend)
async fn run_logs(
    _db_path: &PathBuf,
    id: &str,
    _follow: bool,
    _tail: Option<usize>,
) -> anyhow::Result<()> {
    println!("JOB #{} LOGS", id);
    println!();
    println!("Log viewing is not yet implemented.");
    println!("Job logs are currently only available in the Sentinel console output.");
    println!();
    println!("TRY:");
    println!("  casparian job show {}   # View job details", id);
    println!("  tail -f /var/log/casparian.log   # View Sentinel logs");

    Ok(())
}

/// Retry a single failed job
async fn run_retry(db_path: &PathBuf, id: &str) -> anyhow::Result<()> {
    let job_id: i64 = id.parse().map_err(|_| {
        HelpfulError::new(format!("Invalid job ID: '{}'", id))
            .with_context("Job ID must be a positive integer")
            .with_suggestion("TRY: casparian jobs --failed   # List failed jobs")
    })?;

    let conn = connect_db(db_path).await?;

    // Check job exists and is failed
    let job = get_job_by_id(&conn, job_id).await?;
    let Some(job) = job else {
        return Err(HelpfulError::new(format!("Job {} not found", job_id))
            .with_suggestion("TRY: casparian jobs --failed   # List failed jobs")
            .into());
    };

    if job.status != ProcessingStatus::Failed {
        return Err(HelpfulError::new(format!(
            "Job {} is {}, not FAILED",
            job_id,
            job.status.as_str()
        ))
        .with_context("Only failed jobs can be retried")
        .with_suggestion("TRY: casparian jobs --failed   # List failed jobs")
        .into());
    }

    // Reset job to QUEUED
    conn.execute(
        r#"
        UPDATE cf_processing_queue
        SET status = 'QUEUED',
            claim_time = NULL,
            end_time = NULL,
            error_message = NULL,
            result_summary = NULL,
            retry_count = retry_count + 1
        WHERE id = ?
        "#,
        &[DbValue::from(job_id)],
    )
    .await?;

    println!("Job {} reset to QUEUED for retry", job_id);
    println!();
    println!("The job will be picked up by the next available worker.");
    println!("TRY: casparian jobs --running   # Monitor job progress");

    Ok(())
}

/// Retry all failed jobs
async fn run_retry_all(db_path: &PathBuf, topic: Option<&str>) -> anyhow::Result<()> {
    let conn = connect_db(db_path).await?;

    let rows_affected = if let Some(t) = topic {
        conn.execute(
            r#"
            UPDATE cf_processing_queue
            SET status = 'QUEUED',
                claim_time = NULL,
                end_time = NULL,
                error_message = NULL,
                result_summary = NULL,
                retry_count = retry_count + 1
            WHERE status = 'FAILED' AND plugin_name = ?
            "#,
            &[DbValue::from(t)],
        )
        .await?
    } else {
        conn.execute(
            r#"
            UPDATE cf_processing_queue
            SET status = 'QUEUED',
                claim_time = NULL,
                end_time = NULL,
                error_message = NULL,
                result_summary = NULL,
                retry_count = retry_count + 1
            WHERE status = 'FAILED'
            "#,
            &[],
        )
        .await?
    };

    if rows_affected == 0 {
        println!("No failed jobs found to retry.");
        if topic.is_some() {
            println!("TRY: casparian jobs --failed   # List all failed jobs");
        }
    } else {
        println!("{} job(s) reset to QUEUED for retry", rows_affected);
        println!();
        println!("The jobs will be picked up by available workers.");
        println!("TRY: casparian jobs   # Monitor queue status");
    }

    Ok(())
}

/// Cancel a pending or running job
async fn run_cancel(db_path: &PathBuf, id: &str) -> anyhow::Result<()> {
    let job_id: i64 = id.parse().map_err(|_| {
        HelpfulError::new(format!("Invalid job ID: '{}'", id))
            .with_context("Job ID must be a positive integer")
    })?;

    let conn = connect_db(db_path).await?;

    // Check job exists
    let job = get_job_by_id(&conn, job_id).await?;
    let Some(job) = job else {
        return Err(HelpfulError::new(format!("Job {} not found", job_id))
            .with_suggestion("TRY: casparian jobs   # List available jobs")
            .into());
    };

    // Only cancel queued or running jobs
    match job.status {
        ProcessingStatus::Queued | ProcessingStatus::Running | ProcessingStatus::Pending => {
            // OK to cancel
        }
        ProcessingStatus::Staged => {
            // Staged jobs can also be cancelled
        }
        ProcessingStatus::Completed => {
            return Err(HelpfulError::new(format!("Job {} already completed", job_id))
                .with_context("Cannot cancel a completed job")
                .into());
        }
        ProcessingStatus::Failed => {
            return Err(HelpfulError::new(format!("Job {} already failed", job_id))
                .with_context("Cannot cancel a failed job")
                .with_suggestion("TRY: casparian job retry {}   # Retry the job instead", )
                .into());
        }
        ProcessingStatus::Skipped => {
            return Err(HelpfulError::new(format!("Job {} was skipped", job_id))
                .with_context("Cannot cancel a skipped job")
                .into());
        }
    }

    // Cancel the job
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        r#"
        UPDATE cf_processing_queue
        SET status = 'FAILED',
            end_time = ?,
            error_message = 'Cancelled by user'
        WHERE id = ?
        "#,
        &[DbValue::from(now), DbValue::from(job_id)],
    )
    .await?;

    println!("Job {} cancelled", job_id);

    // If the job was running, warn about potential side effects
    if job.status == ProcessingStatus::Running {
        println!();
        println!("WARNING: Job was RUNNING when cancelled.");
        println!("The worker may have partially processed the file.");
        println!("Check output files for incomplete data.");
    }

    Ok(())
}

/// Connect to the database
fn db_url_for_path(db_path: &PathBuf) -> String {
    format!("duckdb:{}", db_path.display())
}

async fn connect_db(db_path: &PathBuf) -> anyhow::Result<DbConnection> {
    let db_url = db_url_for_path(db_path);
    DbConnection::open_from_url(&db_url).await.map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(format!("Database: {}", db_path.display()))
            .with_suggestion(format!("Error: {}", e))
            .into()
    })
}

/// Get a single job by ID
async fn get_job_by_id(conn: &DbConnection, job_id: i64) -> anyhow::Result<Option<Job>> {
    let has_quarantine_column =
        column_exists(conn, "cf_processing_queue", "quarantine_rows").await?;
    let has_quarantine_table = table_exists(conn, "cf_quarantine").await?;
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
    let query = format!(
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
        WHERE q.id = ?
        "#,
        quarantine_select = quarantine_select,
        quarantine_join = quarantine_join
    );

    let row = conn
        .query_optional(&query, &[DbValue::from(job_id)])
        .await?;

    Ok(row.map(|r| Job {
        id: r.get(0).unwrap_or_default(),
        file_path: r.get(1).unwrap_or_default(),
        plugin_name: r.get(2).unwrap_or_default(),
        status: r.get::<String>(3).unwrap_or_default().parse().unwrap_or_default(),
        priority: r.get(4).unwrap_or_default(),
        claim_time: r.get(5).ok(),
        end_time: r.get(6).ok(),
        error_message: r.get(7).ok(),
        result_summary: r.get(8).ok(),
        retry_count: r.get(9).unwrap_or_default(),
        quarantine_rows: r.get(10).ok(),
    }))
}

/// Get failure details for a job
async fn get_job_failure(conn: &DbConnection, job_id: i64) -> anyhow::Result<Option<JobFailure>> {
    if !table_exists(conn, "cf_job_failures").await? {
        let error = conn
            .query_optional(
                "SELECT error_message FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .await?;
        return Ok(error.and_then(|row| row.get(0).ok()).map(|msg| JobFailure {
            error_type: None,
            error_message: msg,
            stack_trace: None,
            line_number: None,
            context: None,
        }));
    }

    let row = conn
        .query_optional(
            r#"
            SELECT error_type, error_message, stack_trace, line_number, context
            FROM cf_job_failures
            WHERE job_id = ?
            ORDER BY id DESC
            LIMIT 1
            "#,
            &[DbValue::from(job_id)],
        )
        .await?;

    Ok(row.map(|r| JobFailure {
        error_type: r.get(0).ok(),
        error_message: r.get(1).unwrap_or_default(),
        stack_trace: r.get(2).ok(),
        line_number: r.get(3).ok(),
        context: r.get(4).ok(),
    }))
}

/// Build timeline from job data
fn build_timeline(job: &Job) -> JobTimeline {
    let duration_secs = match (&job.claim_time, &job.end_time) {
        (Some(start), Some(end)) => {
            if let (Ok(s), Ok(e)) = (
                chrono::DateTime::parse_from_rfc3339(start),
                chrono::DateTime::parse_from_rfc3339(end),
            ) {
                Some(e.signed_duration_since(s).num_seconds())
            } else {
                None
            }
        }
        _ => None,
    };

    JobTimeline {
        created: None, // Not tracked in current schema
        started: job.claim_time.clone(),
        ended: job.end_time.clone(),
        duration_secs,
    }
}

/// Print formatted job details
fn print_job_details(job: &Job, failure: &Option<JobFailure>, timeline: &JobTimeline) {
    println!("JOB #{}", job.id);
    println!();
    println!("FILE:      {}", job.file_path);
    println!("TOPIC:     {}", job.plugin_name);
    println!("STATUS:    {}", job.status.as_str());
    println!("PRIORITY:  {}", job.priority);
    println!("RETRIES:   {}", job.retry_count);
    if let Some(rows) = job.quarantine_rows {
        println!("QUARANTINE: {} rows", format_number_signed(rows));
    }

    println!();
    println!("TIMELINE:");
    if let Some(ref started) = timeline.started {
        println!("  Started:   {}", format_datetime(started));
    }
    if let Some(ref ended) = timeline.ended {
        println!("  Ended:     {}", format_datetime(ended));
    }
    if let Some(secs) = timeline.duration_secs {
        println!("  Duration:  {}", format_duration(secs));
    }

    if let Some(ref f) = failure {
        println!();
        println!("ERROR:");
        if let Some(ref error_type) = f.error_type {
            println!("  Type:      {}", error_type);
        }
        println!("  Message:   {}", f.error_message);

        if let Some(ref context) = f.context {
            println!();
            println!("CONTEXT:");
            for line in context.lines() {
                println!("  {}", line);
            }
        }

        if let Some(ref trace) = f.stack_trace {
            println!();
            println!("STACK TRACE:");
            for line in trace.lines().take(20) {
                println!("  {}", line);
            }
            let total_lines = trace.lines().count();
            if total_lines > 20 {
                println!("  ... ({} more lines)", total_lines - 20);
            }
        }
    }

    if let Some(ref summary) = job.result_summary {
        println!();
        println!("RESULT:    {}", summary);
    }

    // Suggestions
    println!();
    println!("TRY:");
    if job.status == ProcessingStatus::Failed {
        println!("  casparian job retry {}            # Retry this job", job.id);
        println!(
            "  casparian preview {}   # Inspect the file",
            job.file_path
        );
    } else if job.status == ProcessingStatus::Queued || job.status == ProcessingStatus::Running {
        println!("  casparian job cancel {}           # Cancel this job", job.id);
    }
    println!("  casparian jobs                    # View all jobs");
}

/// Format a datetime string for display
fn format_datetime(dt_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        dt_str.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(5), "5s");
        assert_eq!(format_duration(65), "1m 5s");
        assert_eq!(format_duration(3665), "1h 1m");
        assert_eq!(format_duration(-1), "-");
    }

    #[test]
    fn test_build_timeline() {
        let job = Job {
            id: 1,
            file_path: "/test/file.csv".to_string(),
            plugin_name: "test".to_string(),
            status: ProcessingStatus::Completed,
            priority: 0,
            claim_time: Some("2024-12-16T10:00:00Z".to_string()),
            end_time: Some("2024-12-16T10:00:05Z".to_string()),
            error_message: None,
            result_summary: None,
            retry_count: 0,
            quarantine_rows: None,
        };

        let timeline = build_timeline(&job);
        assert_eq!(timeline.duration_secs, Some(5));
    }
}

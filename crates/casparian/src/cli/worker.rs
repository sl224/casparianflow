//! Worker command - Manage workers
//!
//! Commands for listing, showing, draining, and removing workers.
//! Workers are tracked in cf_worker_node table and managed via Sentinel.

use crate::cli::error::HelpfulError;
use crate::cli::jobs::get_db_path;
use crate::cli::output::print_table_colored;
use casparian_protocol::WorkerStatus;
use clap::Subcommand;
use comfy_table::Color;
use serde::Serialize;
use std::path::PathBuf;

/// Subcommands for worker management
#[derive(Subcommand, Debug, Clone)]
pub enum WorkerAction {
    /// List all workers
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show worker details
    Show {
        /// Worker ID (hostname or worker_id)
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Drain a worker (stop accepting new jobs)
    Drain {
        /// Worker ID to drain
        id: String,
    },
    /// Remove a worker
    Remove {
        /// Worker ID to remove
        id: String,
        /// Force removal even if worker has active jobs
        #[arg(long)]
        force: bool,
    },
    /// Show worker status summary
    Status,
}

/// Get display color for worker status (CLI display helper)
fn worker_status_color(status: &WorkerStatus) -> Color {
    match status {
        WorkerStatus::Idle => Color::Green,
        WorkerStatus::Busy => Color::Cyan,
        WorkerStatus::Alive => Color::Blue,
        WorkerStatus::Draining => Color::Yellow,
        WorkerStatus::ShuttingDown => Color::Yellow,
        WorkerStatus::Offline => Color::Red,
    }
}

/// Parse worker status from string (handles case-insensitive matching)
fn parse_worker_status(s: &str) -> WorkerStatus {
    s.parse().unwrap_or(WorkerStatus::Offline)
}

/// Worker information
#[derive(Debug, Clone, Serialize)]
pub struct Worker {
    pub hostname: String,
    pub pid: i32,
    pub ip_address: Option<String>,
    pub status: WorkerStatus,
    pub current_job_id: Option<i32>,
    pub started_at: String,
    pub last_heartbeat: String,
    pub env_signature: Option<String>,
}

/// Worker statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkerStats {
    pub total: usize,
    pub idle: usize,
    pub busy: usize,
    pub draining: usize,
    pub offline: usize,
}

/// Execute the worker command
pub fn run(action: WorkerAction) -> anyhow::Result<()> {
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
            WorkerAction::List { json } => run_list(&db_path, json).await,
            WorkerAction::Show { id, json } => run_show(&db_path, &id, json).await,
            WorkerAction::Drain { id } => run_drain(&db_path, &id).await,
            WorkerAction::Remove { id, force } => run_remove(&db_path, &id, force).await,
            WorkerAction::Status => run_status(&db_path).await,
        }
    })
}

/// List all workers
async fn run_list(db_path: &PathBuf, json: bool) -> anyhow::Result<()> {
    let pool = connect_db(db_path).await?;
    let workers = get_all_workers(&pool).await?;

    if json {
        let output = serde_json::to_string_pretty(&workers)?;
        println!("{}", output);
    } else {
        print_workers_table(&workers);
    }

    Ok(())
}

/// Show worker details
async fn run_show(db_path: &PathBuf, id: &str, json: bool) -> anyhow::Result<()> {
    let pool = connect_db(db_path).await?;
    let worker = get_worker_by_id(&pool, id).await?;

    let Some(worker) = worker else {
        return Err(HelpfulError::new(format!("Worker '{}' not found", id))
            .with_suggestion("TRY: casparian worker-cli list   # List all workers")
            .into());
    };

    if json {
        let output = serde_json::to_string_pretty(&worker)?;
        println!("{}", output);
    } else {
        print_worker_details(&worker);
    }

    Ok(())
}

/// Drain a worker (stop accepting new jobs)
async fn run_drain(db_path: &PathBuf, id: &str) -> anyhow::Result<()> {
    let pool = connect_db(db_path).await?;

    // Check worker exists
    let worker = get_worker_by_id(&pool, id).await?;
    let Some(worker) = worker else {
        return Err(HelpfulError::new(format!("Worker '{}' not found", id))
            .with_suggestion("TRY: casparian worker-cli list   # List all workers")
            .into());
    };

    if worker.status == WorkerStatus::Draining {
        println!("Worker '{}' is already draining", id);
        return Ok(());
    }

    if worker.status == WorkerStatus::Offline {
        return Err(HelpfulError::new(format!("Worker '{}' is offline", id))
            .with_context("Cannot drain an offline worker")
            .with_suggestion("TRY: casparian worker-cli remove {}   # Remove the worker", )
            .into());
    }

    // Update worker status to draining
    sqlx::query("UPDATE cf_worker_node SET status = 'draining' WHERE hostname = ?")
        .bind(&worker.hostname)
        .execute(&pool)
        .await?;

    println!("Worker '{}' set to DRAINING", id);
    println!();
    println!("The worker will finish its current job and stop accepting new work.");

    if worker.current_job_id.is_some() {
        println!(
            "Current job: #{}",
            worker.current_job_id.unwrap()
        );
    }

    Ok(())
}

/// Remove a worker
async fn run_remove(db_path: &PathBuf, id: &str, force: bool) -> anyhow::Result<()> {
    let pool = connect_db(db_path).await?;

    // Check worker exists
    let worker = get_worker_by_id(&pool, id).await?;
    let Some(worker) = worker else {
        return Err(HelpfulError::new(format!("Worker '{}' not found", id))
            .with_suggestion("TRY: casparian worker-cli list   # List all workers")
            .into());
    };

    // Check if worker has active job
    if worker.current_job_id.is_some() && !force {
        return Err(HelpfulError::new(format!(
            "Worker '{}' has an active job (#{})",
            id,
            worker.current_job_id.unwrap()
        ))
        .with_context("Cannot remove a worker with an active job")
        .with_suggestion("TRY: casparian worker-cli drain {}   # Drain the worker first", )
        .with_suggestion("TRY: casparian worker-cli remove {} --force   # Force removal", )
        .into());
    }

    // If forcing and has active job, requeue it
    if worker.current_job_id.is_some() && force {
        let job_id = worker.current_job_id.unwrap();
        sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'QUEUED',
                claim_time = NULL,
                worker_host = NULL,
                worker_pid = NULL
            WHERE id = ?
            "#,
        )
        .bind(job_id)
        .execute(&pool)
        .await?;

        println!("Requeued job #{}", job_id);
    }

    // Remove worker
    sqlx::query("DELETE FROM cf_worker_node WHERE hostname = ?")
        .bind(&worker.hostname)
        .execute(&pool)
        .await?;

    println!("Worker '{}' removed", id);

    Ok(())
}

/// Show worker status summary
async fn run_status(db_path: &PathBuf) -> anyhow::Result<()> {
    let pool = connect_db(db_path).await?;

    // Get worker stats
    let workers = get_all_workers(&pool).await?;
    let stats = calculate_stats(&workers);

    // Get queue stats
    let queue_stats = get_queue_stats(&pool).await?;

    println!("WORKER STATUS");
    println!();
    println!("WORKERS:");
    println!("  Total:     {}", stats.total);
    println!("  Idle:      {}", stats.idle);
    println!("  Busy:      {}", stats.busy);
    println!("  Draining:  {}", stats.draining);
    println!("  Offline:   {}", stats.offline);

    println!();
    println!("QUEUE:");
    println!("  Pending:   {}", queue_stats.0);
    println!("  Running:   {}", queue_stats.1);
    println!("  Completed: {}", queue_stats.2);
    println!("  Failed:    {}", queue_stats.3);

    if stats.busy > 0 {
        println!();
        println!("ACTIVE WORKERS:");
        for worker in workers.iter().filter(|w| w.status == WorkerStatus::Busy) {
            if let Some(job_id) = worker.current_job_id {
                println!("  {} (pid {}): Job #{}", worker.hostname, worker.pid, job_id);
            }
        }
    }

    if stats.total == 0 {
        println!();
        println!("No workers are currently registered.");
        println!();
        println!("TRY:");
        println!("  casparian start   # Start the unified server (Sentinel + Worker)");
        println!("  casparian worker --connect <addr>   # Start a standalone worker");
    }

    Ok(())
}

/// Connect to the database
async fn connect_db(db_path: &PathBuf) -> anyhow::Result<sqlx::SqlitePool> {
    let db_url = format!("sqlite:{}", db_path.display());
    sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .map_err(|e| {
            HelpfulError::new("Failed to connect to database")
                .with_context(format!("Database: {}", db_path.display()))
                .with_suggestion(format!("Error: {}", e))
        })
        .map_err(Into::into)
}

/// Get all workers from the database
async fn get_all_workers(pool: &sqlx::SqlitePool) -> anyhow::Result<Vec<Worker>> {
    // Check if table exists
    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cf_worker_node'",
    )
    .fetch_optional(pool)
    .await?;

    if table_exists.is_none() {
        return Ok(Vec::new());
    }

    let rows: Vec<(
        String,
        i32,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        Option<i32>,
    )> = sqlx::query_as(
        r#"
        SELECT
            hostname,
            pid,
            ip_address,
            env_signature,
            started_at,
            last_heartbeat,
            status,
            current_job_id
        FROM cf_worker_node
        ORDER BY last_heartbeat DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let workers: Vec<Worker> = rows
        .into_iter()
        .map(|row| Worker {
            hostname: row.0,
            pid: row.1,
            ip_address: row.2,
            env_signature: row.3,
            started_at: row.4,
            last_heartbeat: row.5,
            status: parse_worker_status(&row.6),
            current_job_id: row.7,
        })
        .collect();

    Ok(workers)
}

/// Get a worker by hostname or worker_id
async fn get_worker_by_id(pool: &sqlx::SqlitePool, id: &str) -> anyhow::Result<Option<Worker>> {
    // Check if table exists
    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cf_worker_node'",
    )
    .fetch_optional(pool)
    .await?;

    if table_exists.is_none() {
        return Ok(None);
    }

    let row: Option<(
        String,
        i32,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        Option<i32>,
    )> = sqlx::query_as(
        r#"
        SELECT
            hostname,
            pid,
            ip_address,
            env_signature,
            started_at,
            last_heartbeat,
            status,
            current_job_id
        FROM cf_worker_node
        WHERE hostname = ? OR hostname LIKE ?
        LIMIT 1
        "#,
    )
    .bind(id)
    .bind(format!("%{}%", id))
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Worker {
        hostname: r.0,
        pid: r.1,
        ip_address: r.2,
        env_signature: r.3,
        started_at: r.4,
        last_heartbeat: r.5,
        status: parse_worker_status(&r.6),
        current_job_id: r.7,
    }))
}

/// Get queue statistics
async fn get_queue_stats(pool: &sqlx::SqlitePool) -> anyhow::Result<(i64, i64, i64, i64)> {
    // Check if table exists
    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cf_processing_queue'",
    )
    .fetch_optional(pool)
    .await?;

    if table_exists.is_none() {
        return Ok((0, 0, 0, 0));
    }

    let row: (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            SUM(CASE WHEN status = 'QUEUED' THEN 1 ELSE 0 END) as pending,
            SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) as running,
            SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as completed,
            SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) as failed
        FROM cf_processing_queue
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0, 0, 0, 0));

    Ok(row)
}

/// Calculate worker statistics
fn calculate_stats(workers: &[Worker]) -> WorkerStats {
    let mut stats = WorkerStats {
        total: workers.len(),
        ..Default::default()
    };

    for worker in workers {
        match worker.status {
            WorkerStatus::Idle => stats.idle += 1,
            WorkerStatus::Busy => stats.busy += 1,
            WorkerStatus::Alive => stats.idle += 1, // Count alive as available
            WorkerStatus::Draining => stats.draining += 1,
            WorkerStatus::ShuttingDown => stats.draining += 1, // Count shutting down with draining
            WorkerStatus::Offline => stats.offline += 1,
        }
    }

    stats
}

/// Print workers table
fn print_workers_table(workers: &[Worker]) {
    if workers.is_empty() {
        println!("No workers registered.");
        println!();
        println!("TRY:");
        println!("  casparian start   # Start the unified server");
        return;
    }

    println!("WORKERS ({})", workers.len());

    let headers = &["HOSTNAME", "PID", "STATUS", "CURRENT JOB", "LAST SEEN"];

    let rows: Vec<Vec<(String, Option<Color>)>> = workers
        .iter()
        .map(|w| {
            let job_display = w
                .current_job_id
                .map(|id| format!("#{}", id))
                .unwrap_or_else(|| "-".to_string());

            let last_seen = format_relative_time(&w.last_heartbeat);

            vec![
                (w.hostname.clone(), None),
                (w.pid.to_string(), None),
                (w.status.as_str().to_string(), Some(worker_status_color(&w.status))),
                (job_display, None),
                (last_seen, None),
            ]
        })
        .collect();

    print_table_colored(headers, rows);
}

/// Print worker details
fn print_worker_details(worker: &Worker) {
    println!("WORKER: {}", worker.hostname);
    println!();
    println!("PID:          {}", worker.pid);
    println!("STATUS:       {}", worker.status.as_str());

    if let Some(ref ip) = worker.ip_address {
        println!("IP ADDRESS:   {}", ip);
    }

    println!();
    println!("TIMELINE:");
    println!("  Started:    {}", format_datetime(&worker.started_at));
    println!("  Last seen:  {} ({})",
        format_datetime(&worker.last_heartbeat),
        format_relative_time(&worker.last_heartbeat)
    );

    if let Some(job_id) = worker.current_job_id {
        println!();
        println!("CURRENT JOB:  #{}", job_id);
    }

    if let Some(ref env) = worker.env_signature {
        println!();
        println!("ENVIRONMENT:  {}", &env[..12.min(env.len())]);
    }

    println!();
    println!("TRY:");
    match worker.status {
        WorkerStatus::Idle | WorkerStatus::Busy | WorkerStatus::Alive => {
            println!("  casparian worker-cli drain {}   # Stop accepting new jobs", worker.hostname);
        }
        WorkerStatus::Draining | WorkerStatus::ShuttingDown => {
            println!("  casparian worker-cli remove {}  # Remove after draining", worker.hostname);
        }
        WorkerStatus::Offline => {
            println!("  casparian worker-cli remove {}  # Remove offline worker", worker.hostname);
        }
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

/// Format relative time (e.g., "5 seconds ago")
fn format_relative_time(dt_str: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt);
        let secs = duration.num_seconds();

        if secs < 0 {
            return "just now".to_string();
        }

        if secs < 60 {
            format!("{}s ago", secs)
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else if secs < 86400 {
            format!("{}h ago", secs / 3600)
        } else {
            format!("{}d ago", secs / 86400)
        }
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_worker_status_from_str() {
        // Protocol's from_str returns Result, unwrap for valid cases
        assert_eq!(WorkerStatus::from_str("idle").unwrap(), WorkerStatus::Idle);
        assert_eq!(WorkerStatus::from_str("BUSY").unwrap(), WorkerStatus::Busy);
        assert_eq!(WorkerStatus::from_str("DRAINING").unwrap(), WorkerStatus::Draining);
        // Unknown returns error, parse_worker_status falls back to Offline
        assert_eq!(parse_worker_status("unknown"), WorkerStatus::Offline);
    }

    #[test]
    fn test_calculate_stats() {
        let workers = vec![
            Worker {
                hostname: "w1".to_string(),
                pid: 1,
                ip_address: None,
                status: WorkerStatus::Idle,
                current_job_id: None,
                started_at: "2024-01-01T00:00:00Z".to_string(),
                last_heartbeat: "2024-01-01T00:00:00Z".to_string(),
                env_signature: None,
            },
            Worker {
                hostname: "w2".to_string(),
                pid: 2,
                ip_address: None,
                status: WorkerStatus::Busy,
                current_job_id: Some(1),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                last_heartbeat: "2024-01-01T00:00:00Z".to_string(),
                env_signature: None,
            },
        ];

        let stats = calculate_stats(&workers);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.idle, 1);
        assert_eq!(stats.busy, 1);
        assert_eq!(stats.draining, 0);
        assert_eq!(stats.offline, 0);
    }
}

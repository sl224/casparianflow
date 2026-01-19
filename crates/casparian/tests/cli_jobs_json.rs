mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json, with_duckdb};
use casparian_db::{DbConnection, DbValue};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct JobsOutput {
    stats: QueueStats,
    filters: JobsFilters,
    limit: usize,
    jobs: Vec<JobOutput>,
    dead_letter: Vec<DeadLetterOutput>,
}

#[derive(Debug, Deserialize)]
struct QueueStats {
    total: i64,
    queued: i64,
    running: i64,
    completed: i64,
    failed: i64,
    dead_letter: i64,
}

#[derive(Debug, Deserialize)]
struct JobsFilters {
    topic: Option<String>,
    status: Vec<String>,
    dead_letter: bool,
}

#[derive(Debug, Deserialize)]
struct JobOutput {
    id: i64,
    file_path: String,
    plugin_name: String,
    status: String,
    priority: i32,
    claim_time: Option<String>,
    end_time: Option<String>,
    error_message: Option<String>,
    result_summary: Option<String>,
    retry_count: i32,
    quarantine_rows: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DeadLetterOutput {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct JobDetails {
    job: JobOutput,
    failure: Option<JobFailure>,
}

#[derive(Debug, Deserialize)]
struct JobFailure {
    error_message: String,
}

#[test]
fn test_jobs_json_filters() {
    let (home_dir, db_path) = setup_jobs_db();
    assert!(db_path.exists());
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let jobs_args = vec!["jobs".to_string(), "--json".to_string()];
    let jobs_output: JobsOutput = run_cli_json(&jobs_args, &envs);
    assert_eq!(jobs_output.stats.total, 4);
    assert_eq!(jobs_output.stats.queued, 1);
    assert_eq!(jobs_output.stats.running, 1);
    assert_eq!(jobs_output.stats.completed, 1);
    assert_eq!(jobs_output.stats.failed, 1);
    assert_eq!(jobs_output.stats.dead_letter, 0);
    assert_eq!(jobs_output.jobs.len(), 4);
    assert!(jobs_output.filters.topic.is_none());
    assert!(jobs_output.filters.status.contains(&"QUEUED".to_string()));
    assert!(!jobs_output.filters.dead_letter);
    let dead_letter_ids: Vec<i64> = jobs_output.dead_letter.iter().map(|job| job.id).collect();
    assert!(dead_letter_ids.is_empty());

    let running_job = jobs_output
        .jobs
        .iter()
        .find(|job| job.id == 1)
        .expect("job 1 present");
    assert_eq!(running_job.file_path, "/data/sales/2024_12.csv");
    assert_eq!(running_job.status, "RUNNING");
    assert_eq!(running_job.priority, 0);
    assert_eq!(
        running_job.claim_time.as_deref(),
        Some("2024-12-16T10:30:05Z")
    );
    assert!(running_job.end_time.is_none());
    assert!(running_job.error_message.is_none());
    assert!(running_job.result_summary.is_none());
    assert_eq!(running_job.retry_count, 0);
    assert!(running_job.quarantine_rows.is_none());

    let completed_job = jobs_output
        .jobs
        .iter()
        .find(|job| job.id == 2)
        .expect("job 2 present");
    assert_eq!(completed_job.end_time.as_deref(), Some("2024-12-16T10:30:05Z"));
    assert_eq!(
        completed_job.result_summary.as_deref(),
        Some("Processed 100 rows")
    );

    let failed_job = jobs_output
        .jobs
        .iter()
        .find(|job| job.id == 3)
        .expect("job 3 present");
    assert_eq!(
        failed_job.error_message.as_deref(),
        Some("Missing field customer_id")
    );

    let queued_job = jobs_output
        .jobs
        .iter()
        .find(|job| job.id == 4)
        .expect("job 4 present");
    assert_eq!(queued_job.status, "QUEUED");
    assert!(queued_job.claim_time.is_none());

    let failed_args = vec![
        "jobs".to_string(),
        "--json".to_string(),
        "--failed".to_string(),
    ];
    let failed_output: JobsOutput = run_cli_json(&failed_args, &envs);
    assert_eq!(failed_output.jobs.len(), 1);
    assert!(failed_output.jobs.iter().all(|job| job.status == "FAILED"));

    let topic_args = vec![
        "jobs".to_string(),
        "--json".to_string(),
        "--topic".to_string(),
        "invoice".to_string(),
    ];
    let topic_output: JobsOutput = run_cli_json(&topic_args, &envs);
    assert!(topic_output.jobs.iter().all(|job| job.plugin_name == "invoice"));

    let limit_args = vec![
        "jobs".to_string(),
        "--json".to_string(),
        "--limit".to_string(),
        "2".to_string(),
    ];
    let limit_output: JobsOutput = run_cli_json(&limit_args, &envs);
    assert_eq!(limit_output.limit, 2);
    assert_eq!(limit_output.jobs.len(), 2);
}

#[test]
fn test_job_actions_update_db() {
    let (home_dir, db_path) = setup_jobs_db();
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let show_args = vec![
        "job".to_string(),
        "show".to_string(),
        "3".to_string(),
        "--json".to_string(),
    ];
    let details: JobDetails = run_cli_json(&show_args, &envs);
    assert_eq!(details.job.status, "FAILED");
    assert!(details
        .failure
        .as_ref()
        .is_some_and(|f| f.error_message.contains("Missing field")));

    let retry_args = vec!["job".to_string(), "retry".to_string(), "3".to_string()];
    assert_cli_success(&run_cli(&retry_args, &envs), &retry_args);
    assert_eq!(job_status(&db_path, 3), "QUEUED");
    assert_eq!(job_retry_count(&db_path, 3), 1);

    let cancel_args = vec!["job".to_string(), "cancel".to_string(), "1".to_string()];
    assert_cli_success(&run_cli(&cancel_args, &envs), &cancel_args);
    assert_eq!(job_status(&db_path, 1), "FAILED");
    assert_eq!(job_error(&db_path, 1), Some("Cancelled by user".to_string()));

    cli_support::with_duckdb(&db_path, |conn| async move {
        conn.execute(
            "UPDATE cf_processing_queue SET status = 'FAILED', error_message = 'Another error' WHERE id = ?",
            &[DbValue::from(2i64)],
        )
        .await
        .expect("reset job 2 to failed");
    });

    let retry_all_args = vec!["job".to_string(), "retry-all".to_string()];
    assert_cli_success(&run_cli(&retry_all_args, &envs), &retry_all_args);
    let failed_count = cli_support::with_duckdb(&db_path, |conn| async move {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE status = 'FAILED'",
            &[],
        )
        .await
        .expect("count failed jobs")
    });
    assert_eq!(failed_count, 0);
}

fn setup_jobs_db() -> (TempDir, PathBuf) {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    with_duckdb(&db_path, |conn| async move {
        insert_source(&conn, "src-1", "test_source", "/data", now).await;

        let files = [
            (1, "/data/sales/2024_12.csv", "sales/2024_12.csv"),
            (2, "/data/sales/2024_11.csv", "sales/2024_11.csv"),
            (3, "/data/invoices/inv_003.json", "invoices/inv_003.json"),
            (4, "/data/sales/2024_10.csv", "sales/2024_10.csv"),
        ];
        for (id, path, rel_path) in files {
            insert_file(
                &conn,
                id,
                "src-1",
                path,
                rel_path,
                1000,
                "pending",
                None,
                None,
                now,
            )
            .await;
        }

        conn.execute_batch(
            r#"
            CREATE TABLE cf_processing_queue (
                id BIGINT PRIMARY KEY,
                file_id BIGINT NOT NULL,
                pipeline_run_id TEXT,
                plugin_name TEXT NOT NULL,
                config_overrides TEXT,
                status TEXT NOT NULL DEFAULT 'QUEUED',
                priority INTEGER DEFAULT 0,
                worker_host TEXT,
                worker_pid INTEGER,
                claim_time TIMESTAMP,
                end_time TIMESTAMP,
                result_summary TEXT,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0,
                quarantine_rows BIGINT DEFAULT 0
            );
            "#,
        )
        .await
        .expect("create processing queue");

        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(1i64),
                DbValue::from(1i64),
                DbValue::from("sales"),
                DbValue::from("RUNNING"),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:30:05Z"),
                DbValue::Null,
                DbValue::Null,
            ],
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(2i64),
                DbValue::from(2i64),
                DbValue::from("sales"),
                DbValue::from("COMPLETED"),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:30:02Z"),
                DbValue::from("2024-12-16T10:30:05Z"),
                DbValue::from("Processed 100 rows"),
            ],
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, error_message)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(3i64),
                DbValue::from(3i64),
                DbValue::from("invoice"),
                DbValue::from("FAILED"),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:29:58Z"),
                DbValue::from("2024-12-16T10:29:59Z"),
                DbValue::from("Missing field customer_id"),
            ],
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
             VALUES (?, ?, ?, ?, ?)",
            &[
                DbValue::from(4i64),
                DbValue::from(4i64),
                DbValue::from("sales"),
                DbValue::from("QUEUED"),
                DbValue::from(0i32),
            ],
        )
        .await
        .unwrap();
    });

    (home_dir, db_path)
}

async fn insert_source(conn: &DbConnection, id: &str, name: &str, path: &str, now: i64) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, 30, 1, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(name),
            DbValue::from(source_type),
            DbValue::from(path),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .await
    .expect("insert source");
}

async fn insert_file(
    conn: &DbConnection,
    id: i64,
    source_id: &str,
    path: &str,
    rel_path: &str,
    size: i64,
    status: &str,
    tag: Option<&str>,
    error: Option<&str>,
    now: i64,
) {
    let (parent_path, name) = split_rel_path(rel_path);
    let extension = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());
    conn.execute(
        "INSERT INTO scout_files (id, source_id, path, rel_path, parent_path, name, extension, size, mtime, status, tag, error, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(source_id),
            DbValue::from(path),
            DbValue::from(rel_path),
            DbValue::from(parent_path),
            DbValue::from(name),
            DbValue::from(extension),
            DbValue::from(size),
            DbValue::from(now),
            DbValue::from(status),
            DbValue::from(tag),
            DbValue::from(error),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .await
    .expect("insert file");
}

fn split_rel_path(rel_path: &str) -> (String, String) {
    match rel_path.rfind('/') {
        Some(idx) => (
            rel_path[..idx].to_string(),
            rel_path[idx + 1..].to_string(),
        ),
        None => ("".to_string(), rel_path.to_string()),
    }
}

fn job_status(db_path: &Path, job_id: i64) -> String {
    with_duckdb(db_path, |conn| async move {
        conn.query_scalar::<String>(
            "SELECT status FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id)],
        )
        .await
        .expect("query job status")
    })
}

fn job_retry_count(db_path: &Path, job_id: i64) -> i32 {
    with_duckdb(db_path, |conn| async move {
        conn.query_scalar::<i32>(
            "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id)],
        )
        .await
        .expect("query retry count")
    })
}

fn job_error(db_path: &Path, job_id: i64) -> Option<String> {
    with_duckdb(db_path, |conn| async move {
        conn.query_optional(
            "SELECT error_message FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id)],
        )
        .await
        .ok()
        .and_then(|row| row.and_then(|r| r.get_by_name::<Option<String>>("error_message").ok()).flatten())
    })
}

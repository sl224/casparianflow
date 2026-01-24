mod cli_support;

use casparian::scout::{FileStatus, WorkspaceId};
use casparian_db::{DbConnection, DbValue};
use casparian_protocol::ProcessingStatus;
use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json, with_duckdb};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SOURCE_ID: i64 = 1;

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
    let envs = [("CASPARIAN_HOME", home_str.as_str()), ("RUST_LOG", "error")];

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
    assert!(jobs_output
        .filters
        .status
        .contains(&ProcessingStatus::Queued.as_str().to_string()));
    assert!(!jobs_output.filters.dead_letter);
    let dead_letter_ids: Vec<i64> = jobs_output.dead_letter.iter().map(|job| job.id).collect();
    assert!(dead_letter_ids.is_empty());

    let running_job = jobs_output
        .jobs
        .iter()
        .find(|job| job.id == 1)
        .expect("job 1 present");
    assert_eq!(running_job.file_path, "/data/sales/2024_12.csv");
    assert_eq!(running_job.status, ProcessingStatus::Running.as_str());
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
    assert_eq!(
        completed_job.end_time.as_deref(),
        Some("2024-12-16T10:30:05Z")
    );
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
    assert_eq!(queued_job.status, ProcessingStatus::Queued.as_str());
    assert!(queued_job.claim_time.is_none());

    let failed_args = vec![
        "jobs".to_string(),
        "--json".to_string(),
        "--failed".to_string(),
    ];
    let failed_output: JobsOutput = run_cli_json(&failed_args, &envs);
    assert_eq!(failed_output.jobs.len(), 1);
    assert!(failed_output
        .jobs
        .iter()
        .all(|job| job.status == ProcessingStatus::Failed.as_str()));

    let topic_args = vec![
        "jobs".to_string(),
        "--json".to_string(),
        "--topic".to_string(),
        "invoice".to_string(),
    ];
    let topic_output: JobsOutput = run_cli_json(&topic_args, &envs);
    assert!(topic_output
        .jobs
        .iter()
        .all(|job| job.plugin_name == "invoice"));

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
    let envs = [("CASPARIAN_HOME", home_str.as_str()), ("RUST_LOG", "error")];

    let show_args = vec![
        "job".to_string(),
        "show".to_string(),
        "3".to_string(),
        "--json".to_string(),
    ];
    let details: JobDetails = run_cli_json(&show_args, &envs);
    assert_eq!(details.job.status, ProcessingStatus::Failed.as_str());
    assert!(details
        .failure
        .as_ref()
        .is_some_and(|f| f.error_message.contains("Missing field")));

    let retry_args = vec!["job".to_string(), "retry".to_string(), "3".to_string()];
    assert_cli_success(&run_cli(&retry_args, &envs), &retry_args);
    assert_eq!(job_status(&db_path, 3), ProcessingStatus::Queued.as_str());
    assert_eq!(job_retry_count(&db_path, 3), 1);

    let cancel_args = vec!["job".to_string(), "cancel".to_string(), "1".to_string()];
    assert_cli_success(&run_cli(&cancel_args, &envs), &cancel_args);
    assert_eq!(job_status(&db_path, 1), ProcessingStatus::Aborted.as_str());
    assert_eq!(
        job_error(&db_path, 1),
        Some("Cancelled by user".to_string())
    );

    cli_support::with_duckdb(&db_path, |conn| {
        conn.execute(
            "UPDATE cf_processing_queue SET status = ?, error_message = 'Another error' WHERE id = ?",
            &[
                DbValue::from(ProcessingStatus::Failed.as_str()),
                DbValue::from(2i64),
            ],
        )
        .expect("reset job 2 to failed");
    });

    let retry_all_args = vec!["job".to_string(), "retry-all".to_string()];
    assert_cli_success(&run_cli(&retry_all_args, &envs), &retry_all_args);
    let failed_count = cli_support::with_duckdb(&db_path, |conn| {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE status = ?",
            &[DbValue::from(ProcessingStatus::Failed.as_str())],
        )
        .expect("count failed jobs")
    });
    assert_eq!(failed_count, 0);
}

fn setup_jobs_db() -> (TempDir, PathBuf) {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    with_duckdb(&db_path, |conn| {
        let workspace_id = insert_workspace(&conn, now);
        insert_source(&conn, &workspace_id, SOURCE_ID, "test_source", "/data", now);

        let files = [
            (1, "/data/sales/2024_12.csv", "sales/2024_12.csv"),
            (2, "/data/sales/2024_11.csv", "sales/2024_11.csv"),
            (3, "/data/invoices/inv_003.json", "invoices/inv_003.json"),
            (4, "/data/sales/2024_10.csv", "sales/2024_10.csv"),
        ];
        for (id, path, rel_path) in files {
            insert_file(
                &conn,
                &workspace_id,
                id,
                SOURCE_ID,
                path,
                rel_path,
                1000,
                FileStatus::Pending.as_str(),
                None,
                None,
                now,
            );
        }

        let queue_schema = format!(
            r#"
            CREATE TABLE cf_processing_queue (
                id BIGINT PRIMARY KEY,
                file_id BIGINT NOT NULL,
                pipeline_run_id TEXT,
                plugin_name TEXT NOT NULL,
                config_overrides TEXT,
                status TEXT NOT NULL DEFAULT '{}',
                completion_status TEXT,
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
            ProcessingStatus::Queued.as_str()
        );
        conn.execute_batch(&queue_schema)
            .expect("create processing queue");

        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(1i64),
                DbValue::from(1i64),
                DbValue::from("sales"),
                DbValue::from(ProcessingStatus::Running.as_str()),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:30:05Z"),
                DbValue::Null,
                DbValue::Null,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(2i64),
                DbValue::from(2i64),
                DbValue::from("sales"),
                DbValue::from(ProcessingStatus::Completed.as_str()),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:30:02Z"),
                DbValue::from("2024-12-16T10:30:05Z"),
                DbValue::from("Processed 100 rows"),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, error_message)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from(3i64),
                DbValue::from(3i64),
                DbValue::from("invoice"),
                DbValue::from(ProcessingStatus::Failed.as_str()),
                DbValue::from(0i32),
                DbValue::from("2024-12-16T10:29:58Z"),
                DbValue::from("2024-12-16T10:29:59Z"),
                DbValue::from("Missing field customer_id"),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
             VALUES (?, ?, ?, ?, ?)",
            &[
                DbValue::from(4i64),
                DbValue::from(4i64),
                DbValue::from("sales"),
                DbValue::from(ProcessingStatus::Queued.as_str()),
                DbValue::from(0i32),
            ],
        )
        .unwrap();
    });

    (home_dir, db_path)
}

fn insert_workspace(conn: &DbConnection, now: i64) -> WorkspaceId {
    let workspace_id = WorkspaceId::new();
    conn.execute(
        "INSERT INTO cf_workspaces (id, name, created_at) VALUES (?, ?, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from("Default"),
            DbValue::from(now),
        ],
    )
    .expect("insert workspace");
    workspace_id
}

fn insert_source(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    id: i64,
    name: &str,
    path: &str,
    now: i64,
) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, workspace_id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 30, 1, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(workspace_id.to_string()),
            DbValue::from(name),
            DbValue::from(source_type),
            DbValue::from(path),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert source");
}

fn insert_file(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    id: i64,
    source_id: i64,
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
        "INSERT INTO scout_files (id, workspace_id, source_id, path, rel_path, parent_path, name, extension, is_dir, size, mtime, status, error, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(workspace_id.to_string()),
            DbValue::from(source_id),
            DbValue::from(path),
            DbValue::from(rel_path),
            DbValue::from(parent_path),
            DbValue::from(name),
            DbValue::from(extension),
            DbValue::from(size),
            DbValue::from(now),
            DbValue::from(status),
            DbValue::from(error),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert file");

    if let Some(tag) = tag {
        insert_tag(conn, workspace_id, id, tag, now);
    }
}

fn insert_tag(conn: &DbConnection, workspace_id: &WorkspaceId, file_id: i64, tag: &str, now: i64) {
    conn.execute(
        "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at)
         VALUES (?, ?, ?, 'manual', NULL, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from(file_id),
            DbValue::from(tag),
            DbValue::from(now),
        ],
    )
    .expect("insert file tag");
}

fn split_rel_path(rel_path: &str) -> (String, String) {
    match rel_path.rfind('/') {
        Some(idx) => (rel_path[..idx].to_string(), rel_path[idx + 1..].to_string()),
        None => ("".to_string(), rel_path.to_string()),
    }
}

fn job_status(db_path: &Path, job_id: i64) -> String {
    let conn = DbConnection::open_duckdb_readonly(db_path).expect("open readonly db");
    conn.query_scalar::<String>(
        "SELECT status FROM cf_processing_queue WHERE id = ?",
        &[DbValue::from(job_id)],
    )
    .expect("query job status")
}

fn job_retry_count(db_path: &Path, job_id: i64) -> i32 {
    let conn = DbConnection::open_duckdb_readonly(db_path).expect("open readonly db");
    conn.query_scalar::<i32>(
        "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
        &[DbValue::from(job_id)],
    )
    .expect("query retry count")
}

fn job_error(db_path: &Path, job_id: i64) -> Option<String> {
    let conn = DbConnection::open_duckdb_readonly(db_path).expect("open readonly db");
    conn.query_optional(
        "SELECT error_message FROM cf_processing_queue WHERE id = ?",
        &[DbValue::from(job_id)],
    )
    .ok()
    .and_then(|row| {
        row.and_then(|r| r.get_by_name::<Option<String>>("error_message").ok())
            .flatten()
    })
}

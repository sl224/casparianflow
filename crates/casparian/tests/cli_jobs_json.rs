mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json};
use rusqlite::{params, Connection};
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
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("CASPARIAN_DB_BACKEND", "sqlite"),
        ("RUST_LOG", "error"),
    ];

    let jobs_args = vec!["jobs".to_string(), "--json".to_string()];
    let jobs_output: JobsOutput = run_cli_json(&jobs_args, &envs);
    assert_eq!(jobs_output.stats.total, 4);
    assert_eq!(jobs_output.jobs.len(), 4);
    assert!(jobs_output.filters.status.contains(&"QUEUED".to_string()));

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
        ("CASPARIAN_DB_BACKEND", "sqlite"),
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

    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "UPDATE cf_processing_queue SET status = 'FAILED', error_message = 'Another error' WHERE id = 2",
        [],
    )
    .expect("reset job 2 to failed");

    let retry_all_args = vec!["job".to_string(), "retry-all".to_string()];
    assert_cli_success(&run_cli(&retry_all_args, &envs), &retry_all_args);
    let failed_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE status = 'FAILED'",
            [],
            |row| row.get(0),
        )
        .expect("count failed jobs");
    assert_eq!(failed_count, 0);
}

fn setup_jobs_db() -> (TempDir, PathBuf) {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.sqlite3");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    let conn = Connection::open(&db_path).expect("open sqlite db");
    insert_source(&conn, "src-1", "test_source", "/data", now);

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
        );
    }

    conn.execute_batch(
        r#"
        CREATE TABLE cf_processing_queue (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL,
            pipeline_run_id TEXT,
            plugin_name TEXT NOT NULL,
            config_overrides TEXT,
            status TEXT NOT NULL DEFAULT 'QUEUED',
            priority INTEGER DEFAULT 0,
            worker_host TEXT,
            worker_pid INTEGER,
            claim_time TEXT,
            end_time TEXT,
            result_summary TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0
        );
        "#,
    )
    .expect("create processing queue");

    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
         VALUES (1, 1, 'sales', 'RUNNING', 0, '2024-12-16T10:30:05Z', NULL, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
         VALUES (2, 2, 'sales', 'COMPLETED', 0, '2024-12-16T10:30:02Z', '2024-12-16T10:30:05Z', 'Processed 100 rows')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, error_message)
         VALUES (3, 3, 'invoice', 'FAILED', 0, '2024-12-16T10:29:58Z', '2024-12-16T10:29:59Z', 'Missing field customer_id')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
         VALUES (4, 4, 'sales', 'QUEUED', 0)",
        [],
    )
    .unwrap();

    (home_dir, db_path)
}

fn insert_source(conn: &Connection, id: &str, name: &str, path: &str, now: i64) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 30, 1, ?5, ?6)",
        params![id, name, source_type, path, now, now],
    )
    .expect("insert source");
}

fn insert_file(
    conn: &Connection,
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
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            source_id,
            path,
            rel_path,
            parent_path,
            name,
            extension,
            size,
            now,
            status,
            tag,
            error,
            now,
            now
        ],
    )
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
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row(
        "SELECT status FROM cf_processing_queue WHERE id = ?1",
        params![job_id],
        |row| row.get(0),
    )
    .expect("query job status")
}

fn job_retry_count(db_path: &Path, job_id: i64) -> i32 {
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row(
        "SELECT retry_count FROM cf_processing_queue WHERE id = ?1",
        params![job_id],
        |row| row.get(0),
    )
    .expect("query retry count")
}

fn job_error(db_path: &Path, job_id: i64) -> Option<String> {
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row(
        "SELECT error_message FROM cf_processing_queue WHERE id = ?1",
        params![job_id],
        |row| row.get(0),
    )
    .unwrap_or(None)
}

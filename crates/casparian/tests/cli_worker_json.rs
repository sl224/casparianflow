mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json, with_duckdb};
use casparian_db::DbValue;
use casparian_protocol::{ProcessingStatus, WorkerStatus};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct WorkerOutput {
    hostname: String,
    status: String,
    current_job_id: Option<i32>,
}

#[test]
fn test_worker_json_and_state_changes() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);
    with_duckdb(&db_path, |conn| {
        conn.execute_batch(
            r#"
            CREATE TABLE cf_worker_node (
                hostname TEXT PRIMARY KEY,
                pid INTEGER,
                ip_address TEXT,
                env_signature TEXT,
                started_at TEXT NOT NULL,
                last_heartbeat TEXT NOT NULL,
                status TEXT,
                current_job_id INTEGER
            );

            CREATE TABLE cf_processing_queue (
                id BIGINT PRIMARY KEY,
                status TEXT,
                claim_time TEXT,
                worker_host TEXT,
                worker_pid INTEGER
            );
            "#,
        )
        .expect("create worker tables");

        conn.execute(
            "INSERT INTO cf_worker_node (hostname, pid, ip_address, env_signature, started_at, last_heartbeat, status, current_job_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from("worker-1"),
                DbValue::from(12345i32),
                DbValue::from("192.168.1.10"),
                DbValue::Null,
                DbValue::from("2024-12-16T08:00:00Z"),
                DbValue::from("2024-12-16T10:30:00Z"),
                DbValue::from(WorkerStatus::Busy.as_str()),
                DbValue::from(42i32),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_worker_node (hostname, pid, ip_address, env_signature, started_at, last_heartbeat, status, current_job_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from("worker-2"),
                DbValue::from(12346i32),
                DbValue::from("192.168.1.11"),
                DbValue::Null,
                DbValue::from("2024-12-16T08:00:00Z"),
                DbValue::from("2024-12-16T10:30:00Z"),
                DbValue::from(WorkerStatus::Idle.as_str()),
                DbValue::Null,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_worker_node (hostname, pid, ip_address, env_signature, started_at, last_heartbeat, status, current_job_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                DbValue::from("worker-3"),
                DbValue::from(12347i32),
                DbValue::from("192.168.1.12"),
                DbValue::Null,
                DbValue::from("2024-12-16T08:00:00Z"),
                DbValue::from("2024-12-16T09:00:00Z"),
                DbValue::from(WorkerStatus::Draining.as_str()),
                DbValue::Null,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cf_processing_queue (id, status, claim_time, worker_host, worker_pid)
             VALUES (?, ?, ?, ?, ?)",
            &[
                DbValue::from(42i64),
                DbValue::from(ProcessingStatus::Running.as_str()),
                DbValue::from("2024-12-16T10:30:05Z"),
                DbValue::from("worker-1"),
                DbValue::from(12345i32),
            ],
        )
        .unwrap();
    });

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let list_args = vec![
        "worker-cli".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let workers: Vec<WorkerOutput> = run_cli_json(&list_args, &envs);
    assert_eq!(workers.len(), 3);
    assert!(workers.iter().any(|w| w.hostname == "worker-1"));

    let show_args = vec![
        "worker-cli".to_string(),
        "show".to_string(),
        "worker-1".to_string(),
        "--json".to_string(),
    ];
    let worker: WorkerOutput = run_cli_json(&show_args, &envs);
    assert_eq!(worker.status, WorkerStatus::Busy.as_str());
    assert_eq!(worker.current_job_id, Some(42));

    let drain_args = vec![
        "worker-cli".to_string(),
        "drain".to_string(),
        "worker-2".to_string(),
    ];
    assert_cli_success(&run_cli(&drain_args, &envs), &drain_args);
    assert_eq!(
        worker_status(&db_path, "worker-2"),
        Some(WorkerStatus::Draining.as_str().to_string())
    );

    let remove_args = vec![
        "worker-cli".to_string(),
        "remove".to_string(),
        "worker-1".to_string(),
        "--force".to_string(),
    ];
    assert_cli_success(&run_cli(&remove_args, &envs), &remove_args);
    assert_eq!(worker_status(&db_path, "worker-1"), None);
    assert_eq!(
        job_status(&db_path, 42),
        Some(ProcessingStatus::Queued.as_str().to_string())
    );
    assert!(job_worker_cleared(&db_path, 42));
}

fn worker_status(db_path: &Path, hostname: &str) -> Option<String> {
    with_duckdb(db_path, |conn| {
        conn.query_optional(
            "SELECT status FROM cf_worker_node WHERE hostname = ?",
            &[DbValue::from(hostname)],
        )
        .ok()
        .and_then(|row| row.and_then(|r| r.get_by_name::<String>("status").ok()))
    })
}

fn job_status(db_path: &Path, job_id: i64) -> Option<String> {
    with_duckdb(db_path, |conn| {
        conn.query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id)],
        )
        .ok()
        .and_then(|row| row.and_then(|r| r.get_by_name::<String>("status").ok()))
    })
}

fn job_worker_cleared(db_path: &Path, job_id: i64) -> bool {
    with_duckdb(db_path, |conn| {
        let row = conn
            .query_optional(
                "SELECT worker_host, worker_pid FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .expect("query job worker");
        row.and_then(|r| {
            let host: Option<String> = r.get_by_name::<Option<String>>("worker_host").ok().flatten();
            let pid: Option<i32> = r.get_by_name::<Option<i32>>("worker_pid").ok().flatten();
            Some(host.is_none() && pid.is_none())
        })
        .unwrap_or(false)
    })
}

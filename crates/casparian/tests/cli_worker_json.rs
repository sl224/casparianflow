mod cli_support;

use cli_support::{assert_cli_success, run_cli, run_cli_json};
use rusqlite::{params, Connection};
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
    let db_path = home_dir.path().join("casparian_flow.sqlite3");
    let conn = Connection::open(&db_path).expect("open sqlite db");

    conn.execute_batch(
        r#"
        CREATE TABLE cf_worker_node (
            hostname TEXT PRIMARY KEY,
            pid INTEGER,
            ip_address TEXT,
            env_signature TEXT,
            started_at TEXT,
            last_heartbeat TEXT,
            status TEXT,
            current_job_id INTEGER
        );

        CREATE TABLE cf_processing_queue (
            id INTEGER PRIMARY KEY,
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
         VALUES ('worker-1', 12345, '192.168.1.10', NULL, '2024-12-16T08:00:00Z', '2024-12-16T10:30:00Z', 'busy', 42)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_worker_node (hostname, pid, ip_address, env_signature, started_at, last_heartbeat, status, current_job_id)
         VALUES ('worker-2', 12346, '192.168.1.11', NULL, '2024-12-16T08:00:00Z', '2024-12-16T10:30:00Z', 'idle', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_worker_node (hostname, pid, ip_address, env_signature, started_at, last_heartbeat, status, current_job_id)
         VALUES ('worker-3', 12347, '192.168.1.12', NULL, '2024-12-16T08:00:00Z', '2024-12-16T09:00:00Z', 'draining', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cf_processing_queue (id, status, claim_time, worker_host, worker_pid)
         VALUES (42, 'RUNNING', '2024-12-16T10:30:05Z', 'worker-1', 12345)",
        [],
    )
    .unwrap();

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("CASPARIAN_DB_BACKEND", "sqlite"),
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
    assert_eq!(worker.status, "BUSY");
    assert_eq!(worker.current_job_id, Some(42));

    let drain_args = vec![
        "worker-cli".to_string(),
        "drain".to_string(),
        "worker-2".to_string(),
    ];
    assert_cli_success(&run_cli(&drain_args, &envs), &drain_args);
    assert_eq!(worker_status(&db_path, "worker-2"), Some("draining".to_string()));

    let remove_args = vec![
        "worker-cli".to_string(),
        "remove".to_string(),
        "worker-1".to_string(),
        "--force".to_string(),
    ];
    assert_cli_success(&run_cli(&remove_args, &envs), &remove_args);
    assert_eq!(worker_status(&db_path, "worker-1"), None);
    assert_eq!(job_status(&db_path, 42), Some("QUEUED".to_string()));
    assert!(job_worker_cleared(&db_path, 42));
}

fn worker_status(db_path: &Path, hostname: &str) -> Option<String> {
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row(
        "SELECT status FROM cf_worker_node WHERE hostname = ?1",
        params![hostname],
        |row| row.get(0),
    )
    .ok()
}

fn job_status(db_path: &Path, job_id: i64) -> Option<String> {
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row(
        "SELECT status FROM cf_processing_queue WHERE id = ?1",
        params![job_id],
        |row| row.get(0),
    )
    .ok()
}

fn job_worker_cleared(db_path: &Path, job_id: i64) -> bool {
    let conn = Connection::open(db_path).expect("open sqlite db");
    let (host, pid): (Option<String>, Option<i32>) = conn
        .query_row(
            "SELECT worker_host, worker_pid FROM cf_processing_queue WHERE id = ?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("query job worker");
    host.is_none() && pid.is_none()
}

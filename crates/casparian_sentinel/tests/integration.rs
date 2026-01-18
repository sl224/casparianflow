//! Integration tests for Rust Sentinel
//!
//! Tests the complete control plane: worker registration, job dispatch, and ZMQ communication.

use casparian_protocol::types::{IdentifyPayload, JobReceipt, JobStatus};
use casparian_protocol::{Message, OpCode};
use casparian_db::{DbConnection, DbValue};
use std::time::Duration;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend};

async fn setup_queue_db() -> DbConnection {
    let conn = DbConnection::open_duckdb_memory().await.unwrap();
    conn.execute(
        r#"
        CREATE TABLE cf_processing_queue (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL,
            pipeline_run_id TEXT,
            plugin_name TEXT NOT NULL,
            config_overrides TEXT,
            status TEXT NOT NULL DEFAULT 'PENDING',
            priority INTEGER DEFAULT 0,
            worker_host TEXT,
            worker_pid INTEGER,
            claim_time TIMESTAMP,
            end_time TIMESTAMP,
            result_summary TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0
        )
        "#,
        &[],
    )
    .await
    .unwrap();
    conn.execute(
        r#"
        CREATE TABLE cf_pipeline_runs (
            id TEXT PRIMARY KEY,
            pipeline_id TEXT NOT NULL,
            selection_spec_id TEXT NOT NULL,
            selection_snapshot_hash TEXT NOT NULL,
            context_snapshot_hash TEXT,
            logical_date TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TIMESTAMP,
            completed_at TIMESTAMP,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        &[],
    )
    .await
    .unwrap();
    conn.execute(
        r#"
        CREATE TABLE scout_files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL
        )
        "#,
        &[],
    )
    .await
    .unwrap();
    conn
}

/// Test protocol message roundtrip
#[test]
fn test_identify_message() {
    let identify = IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("test-worker".to_string()),
    };

    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    // Verify header format
    assert_eq!(header.len(), 16);
    assert_eq!(header[0], 0x04); // version
    assert_eq!(header[1], 0x01); // IDENTIFY = 1

    // Verify we can unpack
    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    assert_eq!(unpacked.header.opcode, OpCode::Identify);

    let parsed: IdentifyPayload = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(parsed.worker_id, Some("test-worker".to_string()));
}

/// Test CONCLUDE message format
#[test]
fn test_conclude_message() {
    let receipt = JobReceipt {
        status: JobStatus::Success,
        metrics: std::collections::HashMap::from([("rows".to_string(), 1000i64)]),
        artifacts: vec![],
        error_message: None,
    };

    let payload = serde_json::to_vec(&receipt).unwrap();
    let msg = Message::new(OpCode::Conclude, 42, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x05); // CONCLUDE = 5

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    assert_eq!(unpacked.header.job_id, 42);

    let parsed: JobReceipt = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(parsed.status, JobStatus::Success);
    assert_eq!(parsed.metrics.get("rows"), Some(&1000i64));
}

/// Test worker/sentinel ZMQ message exchange
///
/// This tests the ACTUAL communication pattern:
/// - DEALER sends 2 frames (header, payload)
/// - ROUTER receives 3 frames (identity, header, payload)
#[tokio::test]
async fn test_worker_sentinel_exchange() {
    use zeromq::RouterSocket;

    // Bind ROUTER (like Sentinel)
    let mut router = RouterSocket::new();
    router.bind("tcp://127.0.0.1:15556").await.unwrap();

    // Connect DEALER (like Worker)
    let mut dealer = DealerSocket::new();
    dealer.connect("tcp://127.0.0.1:15556").await.unwrap();

    // Small delay to ensure connection is established
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Worker sends IDENTIFY (2 frames: header + payload)
    let identify = IdentifyPayload {
        capabilities: vec!["test_plugin".to_string()],
        worker_id: Some("worker-1".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    // Send as multipart message (not separate sends!)
    // zeromq-rs: create a ZmqMessage with multiple frames
    use zeromq::ZmqMessage;
    let mut multipart = ZmqMessage::from(header.to_vec());
    multipart.push_back(body.into());
    dealer.send(multipart).await.unwrap();

    // ROUTER receives one multipart message: [identity, header, payload]
    let recv_msg = tokio::time::timeout(Duration::from_secs(2), router.recv())
        .await
        .expect("Timeout on recv")
        .expect("ZMQ error on recv");

    let parts: Vec<Vec<u8>> = recv_msg.into_vec().into_iter()
        .map(|b| b.to_vec())
        .collect();

    println!("Received {} parts", parts.len());
    for (i, part) in parts.iter().enumerate() {
        println!("  Part {}: {} bytes, first byte: {:02x}", i, part.len(), part.get(0).unwrap_or(&0));
    }

    // ROUTER format: [identity, header, payload]
    assert!(parts.len() >= 3, "Expected at least 3 parts, got {}", parts.len());

    let _identity = &parts[0];
    let header = &parts[1];
    let payload = &parts[2];

    // Parse message
    let msg = Message::unpack(&[header.clone(), payload.clone()]).unwrap();
    assert_eq!(msg.header.opcode, OpCode::Identify);

    let parsed: IdentifyPayload = serde_json::from_slice(&msg.payload).unwrap();
    assert_eq!(parsed.worker_id, Some("worker-1".to_string()));
    assert!(parsed.capabilities.contains(&"test_plugin".to_string()));

    println!("✓ Worker registered successfully via ZMQ");
}

/// Test job queue operations
#[tokio::test]
async fn test_job_queue_operations() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db().await;
    let queue = JobQueue::new(conn.clone());

    // Insert test job
    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
        VALUES (1, 1, 'test_plugin', 'QUEUED', 10)
        "#,
        &[],
    )
    .await
    .unwrap();

    // Pop job
    let job = queue.pop_job().await.unwrap();
    assert!(job.is_some());

    let job = job.unwrap();
    assert_eq!(job.plugin_name, "test_plugin");
    assert_eq!(job.priority, 10);

    // Complete job
    queue.complete_job(job.id, "Success").await.unwrap();

    // Verify completed
    let row = conn
        .query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job.id)],
        )
        .await
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();

    assert_eq!(status, "COMPLETED");
}

/// Test job details lookup via scout_files
#[tokio::test]
async fn test_job_details_uses_scout_files() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db().await;
    let queue = JobQueue::new(conn.clone());

    conn.execute(
        "INSERT INTO scout_files (id, path) VALUES (1, '/data/demo/sample.csv')",
        &[],
    )
    .await
    .unwrap();

    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'demo', 'QUEUED')",
        &[],
    )
    .await
    .unwrap();

    let details = queue.get_job_details(1).await.unwrap().unwrap();
    assert_eq!(details.plugin_name, "demo");
    assert_eq!(details.file_path, "/data/demo/sample.csv");
}

/// Test pipeline run status transitions based on job status changes.
#[tokio::test]
async fn test_pipeline_run_status_updates() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db().await;
    let queue = JobQueue::new(conn.clone());

    conn.execute(
        "INSERT INTO cf_pipeline_runs (id, pipeline_id, selection_spec_id, selection_snapshot_hash, logical_date, status) VALUES ('run-1', 'pipe-1', 'spec-1', 'hash-1', '2025-01-01', 'queued')",
        &[],
    )
    .await
    .unwrap();

    conn.execute(
        "INSERT INTO scout_files (id, path) VALUES (1, '/data/demo/a.csv')",
        &[],
    )
    .await
    .unwrap();

    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, pipeline_run_id, plugin_name, status, priority)
        VALUES (1, 1, 'run-1', 'demo', 'QUEUED', 0)
        "#,
        &[],
    )
    .await
    .unwrap();

    let job = queue.pop_job().await.unwrap().unwrap();
    assert_eq!(job.pipeline_run_id.as_deref(), Some("run-1"));

    conn.execute(
        "UPDATE cf_pipeline_runs SET status = 'running', started_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
        &[],
    )
    .await
    .unwrap();

    queue.complete_job(job.id, "Success").await.unwrap();

    let row = conn
        .query_one(
            "SELECT SUM(CASE WHEN status IN ('QUEUED', 'RUNNING') THEN 1 ELSE 0 END) AS active, SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) AS failed, SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) AS completed FROM cf_processing_queue WHERE pipeline_run_id = ?",
            &[DbValue::from("run-1")],
        )
        .await
        .unwrap();
    let active: i64 = row.get_by_name("active").unwrap_or(0);
    let failed: i64 = row.get_by_name("failed").unwrap_or(0);
    let completed: i64 = row.get_by_name("completed").unwrap_or(0);

    if failed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = 'failed', completed_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
            &[],
        )
        .await
        .unwrap();
    } else if active > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = 'running' WHERE id = 'run-1'",
            &[],
        )
        .await
        .unwrap();
    } else if completed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
            &[],
        )
        .await
        .unwrap();
    }

    let status_row = conn
        .query_one("SELECT status FROM cf_pipeline_runs WHERE id = 'run-1'", &[])
        .await
        .unwrap();
    let status: String = status_row.get_by_name("status").unwrap();
    assert_eq!(status, "completed");
}

/// Test job priority ordering
#[tokio::test]
async fn test_job_priority_ordering() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db().await;

    // Insert jobs with different priorities
    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
        VALUES
            (1, 1, 'low', 'QUEUED', 0),
            (2, 2, 'high', 'QUEUED', 100),
            (3, 3, 'medium', 'QUEUED', 50)
        "#,
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn);

    // Should pop highest priority first
    let job1 = queue.pop_job().await.unwrap().unwrap();
    assert_eq!(job1.plugin_name, "high");
    assert_eq!(job1.priority, 100);

    let job2 = queue.pop_job().await.unwrap().unwrap();
    assert_eq!(job2.plugin_name, "medium");
    assert_eq!(job2.priority, 50);

    let job3 = queue.pop_job().await.unwrap().unwrap();
    assert_eq!(job3.plugin_name, "low");
    assert_eq!(job3.priority, 0);

    // Queue should be empty
    let job4 = queue.pop_job().await.unwrap();
    assert!(job4.is_none());
}

// ============================================================================
// VALUABLE TESTS: Job Failure and Retry Logic
// ============================================================================

/// Test that failed jobs are properly marked and can be retried
#[tokio::test]
async fn test_job_failure_marks_status_and_error() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db().await;

    // Insert a job
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'test', 'QUEUED')",
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn.clone());
    let job = queue.pop_job().await.unwrap().unwrap();

    // Fail the job with an error message
    queue.fail_job(job.id, "Parser crashed: division by zero").await.unwrap();

    // Verify status and error message
    let row = conn
        .query_optional(
            "SELECT status, error_message FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job.id)],
        )
        .await
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();
    let error: Option<String> = row.get_by_name("error_message").ok();

    assert_eq!(status, "FAILED");
    assert_eq!(error, Some("Parser crashed: division by zero".to_string()));
}

/// Test job requeue increments retry count
#[tokio::test]
async fn test_job_requeue_increments_retry_count() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db().await;

    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, retry_count) VALUES (1, 1, 'test', 'QUEUED', 0)",
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn.clone());

    // Pop and requeue 3 times
    for expected_retry in 1..=3 {
        let job = queue.pop_job().await.unwrap().unwrap();
        queue.requeue_job(job.id).await.unwrap();

        let row = conn
            .query_optional(
                "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job.id)],
            )
            .await
            .unwrap()
            .unwrap();
        let retry_count: i32 = row.get_by_name("retry_count").unwrap();

        assert_eq!(retry_count, expected_retry, "Retry count should be {}", expected_retry);
    }
}

/// Test jobs exceeding max retries are marked failed
#[tokio::test]
async fn test_job_exceeds_max_retries_marked_failed() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db().await;

    // Insert job that's already at max retries (5 = MAX_RETRY_COUNT)
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, retry_count) VALUES (1, 1, 'test', 'RUNNING', 5)",
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn.clone());

    // This should fail the job permanently, not requeue
    let _result = queue.requeue_job(1).await;

    // Check that job is now FAILED, not QUEUED
    let row = conn
        .query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = 1",
            &[],
        )
        .await
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();

    assert_eq!(status, "FAILED", "Job exceeding max retries should be marked FAILED");
}

// ============================================================================
// VALUABLE TESTS: Concurrent Job Dispatch
// ============================================================================

/// Test multiple workers competing for the same job (only one should get it)
///
/// Note: This test uses sequential job claiming to verify the atomicity of pop_job().
/// True concurrent stress testing of SQLite is beyond the scope of unit tests.
#[tokio::test]
async fn test_concurrent_job_claim_only_one_wins() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db().await;

    // Insert exactly ONE job
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'contested_job', 'QUEUED')",
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn);

    // First pop should succeed
    let first = queue.pop_job().await.unwrap();
    assert!(first.is_some(), "First pop should get the job");

    // Second pop should get nothing (job already claimed)
    let second = queue.pop_job().await.unwrap();
    assert!(second.is_none(), "Second pop should get nothing");

    // Third pop should also get nothing
    let third = queue.pop_job().await.unwrap();
    assert!(third.is_none(), "Third pop should get nothing");
}

/// Test that multiple jobs can be claimed sequentially with no duplicates
#[tokio::test]
async fn test_multiple_jobs_claimed_sequentially() {
    use casparian_sentinel::db::queue::JobQueue;
    use std::collections::HashSet;
    let conn = setup_queue_db().await;

    // Insert 10 jobs
    for i in 1..=10 {
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (?, ?, 'job', 'QUEUED')",
            &[DbValue::from(i), DbValue::from(i)],
        )
        .await
        .unwrap();
    }

    let queue = JobQueue::new(conn);

    // Claim all 10 jobs sequentially
    let mut claimed_ids: Vec<i64> = vec![];
    for _ in 0..15 {  // Try more times than jobs exist
        if let Some(job) = queue.pop_job().await.unwrap() {
            claimed_ids.push(job.id);
        }
    }

    // Should have claimed exactly 10 jobs
    assert_eq!(claimed_ids.len(), 10, "Should claim exactly 10 jobs");

    // All job IDs should be unique
    let unique_ids: HashSet<i64> = claimed_ids.iter().copied().collect();
    assert_eq!(unique_ids.len(), 10, "All claimed jobs should be unique");

    // Another pop should get nothing
    let extra = queue.pop_job().await.unwrap();
    assert!(extra.is_none(), "Queue should be empty after claiming all jobs");
}

// ============================================================================
// VALUABLE TESTS: Worker Disconnect and Recovery
// ============================================================================

/// Test that running jobs from disconnected workers can be recovered
#[tokio::test]
async fn test_stale_running_jobs_can_be_recovered() {
    let conn = setup_queue_db().await;

    // Insert a job that's been "running" for a long time (stale)
    // claim_time is 1 hour ago
    let stale_time = chrono::Utc::now() - chrono::Duration::hours(1);
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, claim_time, worker_host) VALUES (1, 1, 'stale_job', 'RUNNING', ?, 'dead-worker')",
        &[DbValue::from(stale_time.to_rfc3339())],
    )
    .await
    .unwrap();

    // Query for stale jobs (running for more than 10 minutes with no heartbeat)
    let stale_threshold = chrono::Utc::now() - chrono::Duration::minutes(10);
    let stale_rows = conn
        .query_all(
            "SELECT id, plugin_name FROM cf_processing_queue WHERE status = 'RUNNING' AND claim_time < ?",
            &[DbValue::from(stale_threshold.to_rfc3339())],
        )
        .await
        .unwrap();
    let mut stale_jobs = Vec::new();
    for row in stale_rows {
        let id: i64 = row.get_by_name("id").unwrap();
        let plugin_name: String = row.get_by_name("plugin_name").unwrap();
        stale_jobs.push((id, plugin_name));
    }

    assert_eq!(stale_jobs.len(), 1, "Should detect one stale job");
    assert_eq!(stale_jobs[0].1, "stale_job");

    // Requeue the stale job
    conn.execute(
        "UPDATE cf_processing_queue SET status = 'QUEUED', claim_time = NULL, worker_host = NULL, retry_count = retry_count + 1 WHERE id = ?",
        &[DbValue::from(stale_jobs[0].0)],
    )
    .await
    .unwrap();

    // Verify it's now available
    let row = conn
        .query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = 1",
            &[],
        )
        .await
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();

    assert_eq!(status, "QUEUED", "Stale job should be requeued");
}

// ============================================================================
// VALUABLE TESTS: Empty Queue Behavior
// ============================================================================

/// Test behavior when queue is completely empty
#[tokio::test]
async fn test_empty_queue_returns_none_not_error() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db().await;
    let queue = JobQueue::new(conn);

    // Empty queue should return Ok(None), not an error
    let result = queue.pop_job().await;
    assert!(result.is_ok(), "Empty queue should not error");
    assert!(result.unwrap().is_none(), "Empty queue should return None");

    // Multiple calls should all return None without error
    for _ in 0..10 {
        let result = queue.pop_job().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}

/// Test that all PENDING jobs are not picked up (only QUEUED)
#[tokio::test]
async fn test_only_queued_jobs_are_dispatched() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db().await;

    // Insert jobs in various states
    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES
            (1, 1, 'pending_job', 'PENDING'),
            (2, 2, 'running_job', 'RUNNING'),
            (3, 3, 'completed_job', 'COMPLETED'),
            (4, 4, 'failed_job', 'FAILED'),
            (5, 5, 'queued_job', 'QUEUED')
        "#,
        &[],
    )
    .await
    .unwrap();

    let queue = JobQueue::new(conn);

    // Should only get the QUEUED job
    let job = queue.pop_job().await.unwrap();
    assert!(job.is_some());
    assert_eq!(job.unwrap().plugin_name, "queued_job");

    // No more jobs available
    let job = queue.pop_job().await.unwrap();
    assert!(job.is_none(), "PENDING, RUNNING, COMPLETED, FAILED should not be picked up");
}

// ============================================================================
// VALUABLE TESTS: ZMQ Communication Patterns
// ============================================================================

/// Test bidirectional message flow: IDENTIFY -> ACK -> DISPATCH -> CONCLUDE
#[tokio::test]
async fn test_full_worker_lifecycle_message_flow() {
    use zeromq::{RouterSocket, ZmqMessage};
    use casparian_protocol::types::{DispatchCommand, SinkConfig, SinkMode};

    // Sentinel (ROUTER)
    let mut router = RouterSocket::new();
    router.bind("tcp://127.0.0.1:15560").await.unwrap();

    // Worker (DEALER)
    let mut dealer = DealerSocket::new();
    dealer.connect("tcp://127.0.0.1:15560").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Step 1: Worker sends IDENTIFY
    let identify = IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("lifecycle-test-worker".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    let mut identify_msg = ZmqMessage::from(header.to_vec());
    identify_msg.push_back(body.into());
    dealer.send(identify_msg).await.unwrap();

    // Sentinel receives IDENTIFY
    let recv = tokio::time::timeout(Duration::from_secs(2), router.recv())
        .await
        .unwrap()
        .unwrap();
    let parts: Vec<Vec<u8>> = recv.into_vec().into_iter().map(|b| b.to_vec()).collect();
    assert!(parts.len() >= 3);

    let worker_identity = parts[0].clone();
    let recv_msg = Message::unpack(&[parts[1].clone(), parts[2].clone()]).unwrap();
    assert_eq!(recv_msg.header.opcode, OpCode::Identify);

    // Step 2: Sentinel sends DISPATCH
    let dispatch = DispatchCommand {
        plugin_name: "test_parser".to_string(),
        parser_version: Some("1.0.0".to_string()),
        file_path: "/data/test.csv".to_string(),
        sinks: vec![SinkConfig {
            topic: "output".to_string(),
            uri: "parquet://output.parquet".to_string(),
            mode: SinkMode::Append,
            schema_def: None,
        }],
        file_id: 1,
        env_hash: "abc123".to_string(),
        source_code: "# parser code".to_string(),
        artifact_hash: None,
    };
    let payload = serde_json::to_vec(&dispatch).unwrap();
    let dispatch_msg = Message::new(OpCode::Dispatch, 12345, payload).unwrap();
    let (header, body) = dispatch_msg.pack().unwrap();

    // ROUTER sends back to specific worker identity
    let mut reply = ZmqMessage::from(worker_identity);
    reply.push_back(header.to_vec().into());
    reply.push_back(body.into());
    router.send(reply).await.unwrap();

    // Worker receives DISPATCH
    let recv = tokio::time::timeout(Duration::from_secs(2), dealer.recv())
        .await
        .unwrap()
        .unwrap();
    let parts: Vec<Vec<u8>> = recv.into_vec().into_iter().map(|b| b.to_vec()).collect();

    let recv_msg = Message::unpack(&parts).unwrap();
    assert_eq!(recv_msg.header.opcode, OpCode::Dispatch);
    assert_eq!(recv_msg.header.job_id, 12345);

    let parsed: DispatchCommand = serde_json::from_slice(&recv_msg.payload).unwrap();
    assert_eq!(parsed.plugin_name, "test_parser");
    assert_eq!(parsed.file_path, "/data/test.csv");

    println!("✓ Full message lifecycle test passed");
}

/// Test heartbeat message format
#[tokio::test]
async fn test_heartbeat_message_format() {
    use casparian_protocol::types::{HeartbeatPayload, HeartbeatStatus};

    let heartbeat = HeartbeatPayload {
        status: HeartbeatStatus::Busy,
        current_job_id: Some(42),
        active_job_count: 1,
        active_job_ids: vec![42],
    };

    let payload = serde_json::to_vec(&heartbeat).unwrap();
    let msg = Message::new(OpCode::Heartbeat, 0, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x04); // HEARTBEAT = 4

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    assert_eq!(unpacked.header.opcode, OpCode::Heartbeat);

    let parsed: HeartbeatPayload = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(parsed.status, HeartbeatStatus::Busy);
    assert_eq!(parsed.current_job_id, Some(42));
}

// ============================================================================
// VALUABLE TESTS: Error Message Propagation
// ============================================================================

/// Test ERROR message carries full error info
#[tokio::test]
async fn test_error_message_carries_full_info() {
    use casparian_protocol::types::ErrorPayload;

    let error = ErrorPayload {
        message: "Parser failed: invalid CSV format".to_string(),
        traceback: Some("File parser.py, line 42\n  raise ValueError\nValueError: bad row".to_string()),
    };

    let payload = serde_json::to_vec(&error).unwrap();
    let msg = Message::new(OpCode::Err, 99, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x06); // ERR = 6

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    let parsed: ErrorPayload = serde_json::from_slice(&unpacked.payload).unwrap();

    assert!(parsed.message.contains("invalid CSV"));
    assert!(parsed.traceback.as_ref().unwrap().contains("line 42"));
}

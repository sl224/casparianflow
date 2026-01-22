//! Integration tests for Rust Sentinel
//!
//! Tests the complete control plane: worker registration, job dispatch, and ZMQ communication.

use casparian_protocol::types::{IdentifyPayload, JobReceipt, JobStatus};
use casparian_protocol::{JobId, Message, OpCode, PipelineRunStatus, ProcessingStatus};
use casparian_db::{DbConnection, DbValue};
use std::time::Duration;
use zmq::Context;

fn setup_queue_db() -> DbConnection {
    let conn = DbConnection::open_duckdb_memory().unwrap();
    let queue = casparian_sentinel::db::queue::JobQueue::new(conn.clone());
    queue.init_queue_schema().unwrap();
    queue.init_error_handling_schema().unwrap();
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
    let msg = Message::new(OpCode::Identify, JobId::new(0), payload).unwrap();
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
        diagnostics: None,
    };

    let payload = serde_json::to_vec(&receipt).unwrap();
    let msg = Message::new(OpCode::Conclude, JobId::new(42), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x05); // CONCLUDE = 5

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    assert_eq!(unpacked.header.job_id, JobId::new(42));

    let parsed: JobReceipt = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(parsed.status, JobStatus::Success);
    assert_eq!(parsed.metrics.get("rows"), Some(&1000i64));
}

/// Test worker/sentinel ZMQ message exchange
///
/// This tests the ACTUAL communication pattern:
/// - DEALER sends 2 frames (header, payload)
/// - ROUTER receives 3 frames (identity, header, payload)
#[test]
fn test_worker_sentinel_exchange() {
    let context = Context::new();

    let router = context.socket(zmq::ROUTER).unwrap();
    router.bind("tcp://127.0.0.1:15556").unwrap();
    router.set_rcvtimeo(2000).unwrap();

    let dealer = context.socket(zmq::DEALER).unwrap();
    dealer.connect("tcp://127.0.0.1:15556").unwrap();

    // Small delay to ensure connection is established
    std::thread::sleep(Duration::from_millis(50));

    // Worker sends IDENTIFY (2 frames: header + payload)
    let identify = IdentifyPayload {
        capabilities: vec!["test_plugin".to_string()],
        worker_id: Some("worker-1".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, JobId::new(0), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    let frames = [header.as_slice(), body.as_slice()];
    dealer.send_multipart(&frames, 0).unwrap();

    let parts = router.recv_multipart(0).expect("ZMQ error on recv");

    println!("Received {} parts", parts.len());
    for (i, part) in parts.iter().enumerate() {
        println!("  Part {}: {} bytes, first byte: {:02x}", i, part.len(), part.get(0).unwrap_or(&0));
    }

    // ROUTER format: [identity, header, payload]
    assert!(parts.len() >= 3, "Expected at least 3 parts, got {}", parts.len());

    let _identity = &parts[0];
    let mut cursor = 1;
    if parts.get(1).map(|p| p.is_empty()).unwrap_or(false) {
        cursor += 1;
    }
    let header = &parts[cursor];
    let payload = &parts[cursor + 1];

    // Parse message
    let msg = Message::unpack(&[header.clone(), payload.clone()]).unwrap();
    assert_eq!(msg.header.opcode, OpCode::Identify);

    let parsed: IdentifyPayload = serde_json::from_slice(&msg.payload).unwrap();
    assert_eq!(parsed.worker_id, Some("worker-1".to_string()));
    assert!(parsed.capabilities.contains(&"test_plugin".to_string()));

    println!("✓ Worker registered successfully via ZMQ");
}

/// Test job queue operations
#[test]
fn test_job_queue_operations() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db();
    let queue = JobQueue::new(conn.clone());

    // Insert test job
    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
        VALUES (1, 1, 'test_plugin', ?, 10)
        "#,
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    // Pop job
    let job = queue.pop_job().unwrap();
    assert!(job.is_some());

    let job = job.unwrap();
    assert_eq!(job.plugin_name, "test_plugin");
    assert_eq!(job.priority, 10);

    // Complete job with SUCCESS completion_status using enum helper
    use casparian_protocol::types::JobStatus as ProtocolJobStatus;
    queue.complete_job(job.id, ProtocolJobStatus::Success.as_str(), "Success", None).unwrap();

    // Verify completed with completion_status
    let row = conn
        .query_optional(
            "SELECT status, completion_status FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job.id)],
        )
        
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();
    let completion_status: Option<String> = row.get_by_name("completion_status").unwrap();

    assert_eq!(status, ProcessingStatus::Completed.as_str());
    assert_eq!(completion_status, Some(ProtocolJobStatus::Success.as_str().to_string()));
}

/// Test job details lookup via scout_files
#[test]
fn test_job_details_uses_scout_files() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db();
    let queue = JobQueue::new(conn.clone());

    conn.execute(
        "INSERT INTO scout_files (id, path) VALUES (1, '/data/demo/sample.csv')",
        &[],
    )
    
    .unwrap();

    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'demo', ?)",
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    let details = queue.get_job_details(1).unwrap().unwrap();
    assert_eq!(details.plugin_name, "demo");
    assert_eq!(details.file_path, "/data/demo/sample.csv");
}

/// Test pipeline run status transitions based on job status changes.
#[test]
fn test_pipeline_run_status_updates() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db();
    let queue = JobQueue::new(conn.clone());

    conn.execute(
        "INSERT INTO cf_pipeline_runs (id, pipeline_id, selection_spec_id, selection_snapshot_hash, logical_date, status) VALUES ('run-1', 'pipe-1', 'spec-1', 'hash-1', '2025-01-01', ?)",
        &[DbValue::from(PipelineRunStatus::Queued.as_str())],
    )
    
    .unwrap();

    conn.execute(
        "INSERT INTO scout_files (id, path) VALUES (1, '/data/demo/a.csv')",
        &[],
    )
    
    .unwrap();

    conn.execute(
        r#"
        INSERT INTO cf_processing_queue (id, file_id, pipeline_run_id, plugin_name, status, priority)
        VALUES (1, 1, 'run-1', 'demo', ?, 0)
        "#,
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    let job = queue.pop_job().unwrap().unwrap();
    assert_eq!(job.pipeline_run_id.as_deref(), Some("run-1"));

    conn.execute(
        "UPDATE cf_pipeline_runs SET status = ?, started_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
        &[DbValue::from(PipelineRunStatus::Running.as_str())],
    )
    
    .unwrap();

    use casparian_protocol::types::JobStatus as ProtocolJobStatus;
    queue.complete_job(job.id, ProtocolJobStatus::Success.as_str(), "Success", None).unwrap();

    let row = conn
        .query_one(
            "SELECT SUM(CASE WHEN status IN (?, ?) THEN 1 ELSE 0 END) AS active, SUM(CASE WHEN status = ? THEN 1 ELSE 0 END) AS failed, SUM(CASE WHEN status = ? THEN 1 ELSE 0 END) AS completed FROM cf_processing_queue WHERE pipeline_run_id = ?",
            &[
                DbValue::from(ProcessingStatus::Queued.as_str()),
                DbValue::from(ProcessingStatus::Running.as_str()),
                DbValue::from(ProcessingStatus::Failed.as_str()),
                DbValue::from(ProcessingStatus::Completed.as_str()),
                DbValue::from("run-1"),
            ],
        )
        
        .unwrap();
    let active: i64 = row.get_by_name("active").unwrap_or(0);
    let failed: i64 = row.get_by_name("failed").unwrap_or(0);
    let completed: i64 = row.get_by_name("completed").unwrap_or(0);

    if failed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
            &[DbValue::from(PipelineRunStatus::Failed.as_str())],
        )
        
        .unwrap();
    } else if active > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = ? WHERE id = 'run-1'",
            &[DbValue::from(PipelineRunStatus::Running.as_str())],
        )
        
        .unwrap();
    } else if completed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = 'run-1'",
            &[DbValue::from(PipelineRunStatus::Completed.as_str())],
        )
        
        .unwrap();
    }

    let status_row = conn
        .query_one("SELECT status FROM cf_pipeline_runs WHERE id = 'run-1'", &[])
        
        .unwrap();
    let status: String = status_row.get_by_name("status").unwrap();
    assert_eq!(status, PipelineRunStatus::Completed.as_str());
}

/// Test job priority ordering
#[test]
fn test_job_priority_ordering() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db();

    // Insert jobs with different priorities
    let queued = ProcessingStatus::Queued.as_str();
    conn.execute(
        &format!(
            r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
        VALUES
            (1, 1, 'low', '{queued}', 0),
            (2, 2, 'high', '{queued}', 100),
            (3, 3, 'medium', '{queued}', 50)
        "#,
            queued = queued
        ),
        &[],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn);

    // Should pop highest priority first
    let job1 = queue.pop_job().unwrap().unwrap();
    assert_eq!(job1.plugin_name, "high");
    assert_eq!(job1.priority, 100);

    let job2 = queue.pop_job().unwrap().unwrap();
    assert_eq!(job2.plugin_name, "medium");
    assert_eq!(job2.priority, 50);

    let job3 = queue.pop_job().unwrap().unwrap();
    assert_eq!(job3.plugin_name, "low");
    assert_eq!(job3.priority, 0);

    // Queue should be empty
    let job4 = queue.pop_job().unwrap();
    assert!(job4.is_none());
}

// ============================================================================
// VALUABLE TESTS: Job Failure and Retry Logic
// ============================================================================

/// Test that failed jobs are properly marked and can be retried
#[test]
fn test_job_failure_marks_status_and_error() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db();

    // Insert a job
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'test', ?)",
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn.clone());
    let job = queue.pop_job().unwrap().unwrap();

    // Fail the job with an error message using enum helper
    use casparian_protocol::types::JobStatus as ProtocolJobStatus;
    queue.fail_job(job.id, ProtocolJobStatus::Failed.as_str(), "Parser crashed: division by zero").unwrap();

    // Verify status, completion_status, and error message
    let row = conn
        .query_optional(
            "SELECT status, completion_status, error_message FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job.id)],
        )
        
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();
    let completion_status: Option<String> = row.get_by_name("completion_status").unwrap();
    let error: Option<String> = row.get_by_name("error_message").ok();

    assert_eq!(status, ProcessingStatus::Failed.as_str());
    assert_eq!(completion_status, Some(ProtocolJobStatus::Failed.as_str().to_string()));
    assert_eq!(error, Some("Parser crashed: division by zero".to_string()));
}

/// Test job requeue increments retry count
#[test]
fn test_job_requeue_increments_retry_count() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db();

    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, retry_count) VALUES (1, 1, 'test', ?, 0)",
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn.clone());

    // Pop and requeue 3 times
    for expected_retry in 1..=3 {
        let job = queue.pop_job().unwrap().unwrap();
        queue.requeue_job(job.id).unwrap();

        let row = conn
            .query_optional(
                "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job.id)],
            )
            
            .unwrap()
            .unwrap();
        let retry_count: i32 = row.get_by_name("retry_count").unwrap();

        assert_eq!(retry_count, expected_retry, "Retry count should be {}", expected_retry);
    }
}

/// Test jobs exceeding max retries are marked failed
#[test]
fn test_job_exceeds_max_retries_marked_failed() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db();

    // Insert job that's already at max retries (3 = MAX_RETRY_COUNT)
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, retry_count) VALUES (1, 1, 'test', ?, 3)",
        &[DbValue::from(ProcessingStatus::Running.as_str())],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn.clone());

    // This should fail the job permanently, not requeue
    let _result = queue.requeue_job(1);

    // Check that job is now FAILED, not QUEUED
    let row = conn
        .query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = 1",
            &[],
        )
        
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();

    assert_eq!(
        status,
        ProcessingStatus::Failed.as_str(),
        "Job exceeding max retries should be marked FAILED"
    );
}

// ============================================================================
// VALUABLE TESTS: Concurrent Job Dispatch
// ============================================================================

/// Test multiple workers competing for the same job (only one should get it)
///
/// Note: This test uses sequential job claiming to verify the atomicity of pop_job().
/// True concurrent stress testing of SQLite is beyond the scope of unit tests.
#[test]
fn test_concurrent_job_claim_only_one_wins() {
    use casparian_sentinel::db::queue::JobQueue;
    let conn = setup_queue_db();

    // Insert exactly ONE job
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (1, 1, 'contested_job', ?)",
        &[DbValue::from(ProcessingStatus::Queued.as_str())],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn);

    // First pop should succeed
    let first = queue.pop_job().unwrap();
    assert!(first.is_some(), "First pop should get the job");

    // Second pop should get nothing (job already claimed)
    let second = queue.pop_job().unwrap();
    assert!(second.is_none(), "Second pop should get nothing");

    // Third pop should also get nothing
    let third = queue.pop_job().unwrap();
    assert!(third.is_none(), "Third pop should get nothing");
}

/// Test that multiple jobs can be claimed sequentially with no duplicates
#[test]
fn test_multiple_jobs_claimed_sequentially() {
    use casparian_sentinel::db::queue::JobQueue;
    use std::collections::HashSet;
    let conn = setup_queue_db();

    // Insert 10 jobs
    for i in 1..=10 {
        conn.execute(
            "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (?, ?, 'job', ?)",
            &[
                DbValue::from(i),
                DbValue::from(i),
                DbValue::from(ProcessingStatus::Queued.as_str()),
            ],
        )
        
        .unwrap();
    }

    let queue = JobQueue::new(conn);

    // Claim all 10 jobs sequentially
    let mut claimed_ids: Vec<i64> = vec![];
    for _ in 0..15 {  // Try more times than jobs exist
        if let Some(job) = queue.pop_job().unwrap() {
            claimed_ids.push(job.id);
        }
    }

    // Should have claimed exactly 10 jobs
    assert_eq!(claimed_ids.len(), 10, "Should claim exactly 10 jobs");

    // All job IDs should be unique
    let unique_ids: HashSet<i64> = claimed_ids.iter().copied().collect();
    assert_eq!(unique_ids.len(), 10, "All claimed jobs should be unique");

    // Another pop should get nothing
    let extra = queue.pop_job().unwrap();
    assert!(extra.is_none(), "Queue should be empty after claiming all jobs");
}

// ============================================================================
// VALUABLE TESTS: Worker Disconnect and Recovery
// ============================================================================

/// Test that running jobs from disconnected workers can be recovered
#[test]
fn test_stale_running_jobs_can_be_recovered() {
    let conn = setup_queue_db();

    // Insert a job that's been "running" for a long time (stale)
    // claim_time is 1 hour ago
    let stale_time = chrono::Utc::now() - chrono::Duration::hours(1);
    conn.execute(
        "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, claim_time, worker_host) VALUES (1, 1, 'stale_job', ?, ?, 'dead-worker')",
        &[
            DbValue::from(ProcessingStatus::Running.as_str()),
            DbValue::from(stale_time.to_rfc3339()),
        ],
    )
    
    .unwrap();

    // Query for stale jobs (running for more than 10 minutes with no heartbeat)
    let stale_threshold = chrono::Utc::now() - chrono::Duration::minutes(10);
    let stale_rows = conn
        .query_all(
            "SELECT id, plugin_name FROM cf_processing_queue WHERE status = ? AND claim_time < ?",
            &[
                DbValue::from(ProcessingStatus::Running.as_str()),
                DbValue::from(stale_threshold.to_rfc3339()),
            ],
        )
        
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
        "UPDATE cf_processing_queue SET status = ?, claim_time = NULL, worker_host = NULL, retry_count = retry_count + 1 WHERE id = ?",
        &[
            DbValue::from(ProcessingStatus::Queued.as_str()),
            DbValue::from(stale_jobs[0].0),
        ],
    )
    
    .unwrap();

    // Verify it's now available
    let row = conn
        .query_optional(
            "SELECT status FROM cf_processing_queue WHERE id = 1",
            &[],
        )
        
        .unwrap()
        .unwrap();
    let status: String = row.get_by_name("status").unwrap();

    assert_eq!(
        status,
        ProcessingStatus::Queued.as_str(),
        "Stale job should be requeued"
    );
}

// ============================================================================
// VALUABLE TESTS: Empty Queue Behavior
// ============================================================================

/// Test behavior when queue is completely empty
#[test]
fn test_empty_queue_returns_none_not_error() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db();
    let queue = JobQueue::new(conn);

    // Empty queue should return Ok(None), not an error
    let result = queue.pop_job();
    assert!(result.is_ok(), "Empty queue should not error");
    assert!(result.unwrap().is_none(), "Empty queue should return None");

    // Multiple calls should all return None without error
    for _ in 0..10 {
        let result = queue.pop_job();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}

/// Test that all PENDING jobs are not picked up (only QUEUED)
#[test]
fn test_only_queued_jobs_are_dispatched() {
    use casparian_sentinel::db::queue::JobQueue;

    let conn = setup_queue_db();

    // Insert jobs in various states
    conn.execute(
        &format!(
            r#"
        INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES
            (1, 1, 'pending_job', '{pending}'),
            (2, 2, 'running_job', '{running}'),
            (3, 3, 'completed_job', '{completed}'),
            (4, 4, 'failed_job', '{failed}'),
            (5, 5, 'queued_job', '{queued}')
        "#,
            pending = ProcessingStatus::Pending.as_str(),
            running = ProcessingStatus::Running.as_str(),
            completed = ProcessingStatus::Completed.as_str(),
            failed = ProcessingStatus::Failed.as_str(),
            queued = ProcessingStatus::Queued.as_str(),
        ),
        &[],
    )
    
    .unwrap();

    let queue = JobQueue::new(conn);

    // Should only get the QUEUED job
    let job = queue.pop_job().unwrap();
    assert!(job.is_some());
    assert_eq!(job.unwrap().plugin_name, "queued_job");

    // No more jobs available
    let job = queue.pop_job().unwrap();
    assert!(
        job.is_none(),
        "{} should not be picked up",
        format!(
            "{}, {}, {}, {}",
            ProcessingStatus::Pending.as_str(),
            ProcessingStatus::Running.as_str(),
            ProcessingStatus::Completed.as_str(),
            ProcessingStatus::Failed.as_str(),
        )
    );
}

// ============================================================================
// VALUABLE TESTS: ZMQ Communication Patterns
// ============================================================================

/// Test bidirectional message flow: IDENTIFY -> ACK -> DISPATCH -> CONCLUDE
#[test]
fn test_full_worker_lifecycle_message_flow() {
    use casparian_protocol::types::{DispatchCommand, RuntimeKind, SinkConfig, SinkMode};
    let context = Context::new();

    let router = context.socket(zmq::ROUTER).unwrap();
    router.bind("tcp://127.0.0.1:15560").unwrap();
    router.set_rcvtimeo(2000).unwrap();

    let dealer = context.socket(zmq::DEALER).unwrap();
    dealer.connect("tcp://127.0.0.1:15560").unwrap();
    dealer.set_rcvtimeo(2000).unwrap();
    std::thread::sleep(Duration::from_millis(50));

    // Step 1: Worker sends IDENTIFY
    let identify = IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("lifecycle-test-worker".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, JobId::new(0), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    let identify_frames = [header.as_slice(), body.as_slice()];
    dealer.send_multipart(&identify_frames, 0).unwrap();

    // Sentinel receives IDENTIFY
    let parts = router.recv_multipart(0).unwrap();
    assert!(parts.len() >= 3);

    let worker_identity = parts[0].clone();
    let mut cursor = 1;
    if parts.get(1).map(|p| p.is_empty()).unwrap_or(false) {
        cursor += 1;
    }
    let recv_msg = Message::unpack(&[parts[cursor].clone(), parts[cursor + 1].clone()]).unwrap();
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
            quarantine_config: None,
            schema: None,
        }],
        file_id: 1,
        runtime_kind: RuntimeKind::PythonShim,
        entrypoint: "test_parser.py:Handler".to_string(),
        platform_os: None,
        platform_arch: None,
        signature_verified: false,
        signer_id: None,
        env_hash: Some("abc123".to_string()),
        source_code: Some("# parser code".to_string()),
        artifact_hash: "artifact_hash_test".to_string(),
    };
    let payload = serde_json::to_vec(&dispatch).unwrap();
    let dispatch_msg = Message::new(OpCode::Dispatch, JobId::new(12345), payload).unwrap();
    let (header, body) = dispatch_msg.pack().unwrap();

    // ROUTER sends back to specific worker identity
    let reply = vec![worker_identity, header.to_vec(), body.to_vec()];
    router.send_multipart(&reply, 0).unwrap();

    // Worker receives DISPATCH
    let parts = dealer.recv_multipart(0).unwrap();

    let frames = if parts.len() == 3 && parts[0].is_empty() {
        vec![parts[1].clone(), parts[2].clone()]
    } else {
        parts
    };
    let recv_msg = Message::unpack(&frames).unwrap();
    assert_eq!(recv_msg.header.opcode, OpCode::Dispatch);
    assert_eq!(recv_msg.header.job_id, JobId::new(12345));

    let parsed: DispatchCommand = serde_json::from_slice(&recv_msg.payload).unwrap();
    assert_eq!(parsed.plugin_name, "test_parser");
    assert_eq!(parsed.file_path, "/data/test.csv");

    println!("✓ Full message lifecycle test passed");
}

/// Test heartbeat message format
#[test]
fn test_heartbeat_message_format() {
    use casparian_protocol::types::{HeartbeatPayload, HeartbeatStatus};

    let heartbeat = HeartbeatPayload {
        status: HeartbeatStatus::Busy,
        active_job_count: 1,
        active_job_ids: vec![JobId::new(42)],
    };

    let payload = serde_json::to_vec(&heartbeat).unwrap();
    let msg = Message::new(OpCode::Heartbeat, JobId::new(0), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x04); // HEARTBEAT = 4

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    assert_eq!(unpacked.header.opcode, OpCode::Heartbeat);

    let parsed: HeartbeatPayload = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(parsed.status, HeartbeatStatus::Busy);
}

// ============================================================================
// VALUABLE TESTS: Error Message Propagation
// ============================================================================

/// Test ERROR message carries full error info
#[test]
fn test_error_message_carries_full_info() {
    use casparian_protocol::types::ErrorPayload;

    let error = ErrorPayload {
        message: "Parser failed: invalid CSV format".to_string(),
        traceback: Some("File parser.py, line 42\n  raise ValueError\nValueError: bad row".to_string()),
    };

    let payload = serde_json::to_vec(&error).unwrap();
    let msg = Message::new(OpCode::Err, JobId::new(99), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    assert_eq!(header[1], 0x06); // ERR = 6

    let unpacked = Message::unpack(&[header.to_vec(), body]).unwrap();
    let parsed: ErrorPayload = serde_json::from_slice(&unpacked.payload).unwrap();

    assert!(parsed.message.contains("invalid CSV"));
    assert!(parsed.traceback.as_ref().unwrap().contains("line 42"));
}

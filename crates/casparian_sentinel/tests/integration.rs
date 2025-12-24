//! Integration tests for Rust Sentinel
//!
//! Tests the complete control plane: worker registration, job dispatch, and ZMQ communication.

use cf_protocol::types::{IdentifyPayload, JobReceipt, JobStatus};
use cf_protocol::{Message, OpCode};
use std::time::Duration;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend};

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
    use sqlx::sqlite::SqlitePoolOptions;

    // Create in-memory database
    let pool = SqlitePoolOptions::new()
        .connect(":memory:")
        .await
        .unwrap();

    // Create test table
    sqlx::query(
        r#"
        CREATE TABLE cf_processing_queue (
            id INTEGER PRIMARY KEY,
            file_version_id INTEGER NOT NULL,
            plugin_name TEXT NOT NULL,
            config_overrides TEXT,
            status TEXT NOT NULL DEFAULT 'PENDING',
            priority INTEGER DEFAULT 0,
            worker_host TEXT,
            worker_pid INTEGER,
            claim_time TEXT,
            end_time TEXT,
            result_summary TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let queue = JobQueue::new(pool.clone());

    // Insert test job
    sqlx::query(
        r#"
        INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
        VALUES (1, 'test_plugin', 'QUEUED', 10)
        "#,
    )
    .execute(&pool)
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
    let status: String = sqlx::query_scalar(
        "SELECT status FROM cf_processing_queue WHERE id = ?",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(status, "COMPLETED");
}

/// Test job priority ordering
#[tokio::test]
async fn test_job_priority_ordering() {
    use casparian_sentinel::db::queue::JobQueue;
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .connect(":memory:")
        .await
        .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE cf_processing_queue (
            id INTEGER PRIMARY KEY,
            file_version_id INTEGER NOT NULL,
            plugin_name TEXT NOT NULL,
            config_overrides TEXT,
            status TEXT NOT NULL DEFAULT 'PENDING',
            priority INTEGER DEFAULT 0,
            worker_host TEXT,
            worker_pid INTEGER,
            claim_time TEXT,
            end_time TEXT,
            result_summary TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert jobs with different priorities
    sqlx::query(
        r#"
        INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
        VALUES
            (1, 'low', 'QUEUED', 0),
            (2, 'high', 'QUEUED', 100),
            (3, 'medium', 'QUEUED', 50)
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let queue = JobQueue::new(pool);

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

//! Integration tests for Rust Worker with Python Sentinel
//!
//! These tests verify the Rust worker can communicate correctly
//! with the Python Sentinel over ZMQ.

use casparian_protocol::*;
use std::time::Duration;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend};

/// Test that protocol messages round-trip correctly
#[test]
fn test_protocol_message_roundtrip() {
    // Create a DISPATCH command
    let cmd = types::DispatchCommand {
        plugin_name: "test_plugin".to_string(),
        file_path: "/data/input.csv".to_string(),
        sinks: vec![types::SinkConfig {
            topic: "output".to_string(),
            uri: "parquet://output.parquet".to_string(),
            mode: types::SinkMode::Append,
            schema_def: None,
        }],
        file_id: 1,
        env_hash: "abc123def456".to_string(),
        source_code: "# test plugin".to_string(),
        artifact_hash: None,
    };

    let payload = serde_json::to_vec(&cmd).unwrap();
    let msg = Message::new(OpCode::Dispatch, 12345, payload).unwrap();

    // Pack and unpack
    let (header, body) = msg.pack().unwrap();
    let frames = vec![header.to_vec(), body];
    let unpacked = Message::unpack(&frames).unwrap();

    assert_eq!(unpacked.header.opcode, OpCode::Dispatch);
    assert_eq!(unpacked.header.job_id, 12345);

    // Verify payload
    let unpacked_cmd: types::DispatchCommand = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(unpacked_cmd.plugin_name, "test_plugin");
    assert_eq!(unpacked_cmd.env_hash, "abc123def456");
}

/// Test IDENTIFY message format
#[test]
fn test_identify_message_format() {
    let identify = types::IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("rust-worker-test".to_string()),
    };

    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload).unwrap();

    let (header, body) = msg.pack().unwrap();

    // Header should be 16 bytes
    assert_eq!(header.len(), 16);

    // First byte is version (0x04)
    assert_eq!(header[0], 0x04);

    // Second byte is opcode (IDENTIFY = 1)
    assert_eq!(header[1], 0x01);

    // Verify we can parse body
    let parsed: types::IdentifyPayload = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.worker_id, Some("rust-worker-test".to_string()));
}

/// Test CONCLUDE message format
#[test]
fn test_conclude_message_format() {
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("rows".to_string(), 1500i64);

    let receipt = types::JobReceipt {
        status: types::JobStatus::Success,
        metrics,
        artifacts: vec![],
        error_message: None,
    };

    let payload = serde_json::to_vec(&receipt).unwrap();
    let msg = Message::new(OpCode::Conclude, 99999, payload).unwrap();

    let (header, body) = msg.pack().unwrap();

    // Verify header
    assert_eq!(header[0], 0x04); // version
    assert_eq!(header[1], 0x05); // CONCLUDE = 5

    // Verify job_id is encoded correctly (big endian)
    let frames = vec![header.to_vec(), body];
    let unpacked = Message::unpack(&frames).unwrap();
    assert_eq!(unpacked.header.job_id, 99999);
}

/// Test ENV_READY message format
#[test]
fn test_env_ready_message_format() {
    let payload = types::EnvReadyPayload {
        env_hash: "deadbeef12345678".to_string(),
        interpreter_path: "/home/user/.casparian_flow/venvs/deadbeef/bin/python".to_string(),
        cached: true,
    };

    let json = serde_json::to_vec(&payload).unwrap();
    let msg = Message::new(OpCode::EnvReady, 0, json).unwrap();

    let (header, _) = msg.pack().unwrap();

    // OpCode.ENV_READY = 9
    assert_eq!(header[1], 0x09);
}

/// Test PREPARE_ENV message parsing
#[test]
fn test_prepare_env_parsing() {
    let cmd = types::PrepareEnvCommand {
        env_hash: "abc123".to_string(),
        lockfile_content: "# uv.lock content".to_string(),
        python_version: Some("3.11".to_string()),
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let parsed: types::PrepareEnvCommand = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.env_hash, "abc123");
    assert_eq!(parsed.python_version, Some("3.11".to_string()));
}

/// Test error message format
#[test]
fn test_error_message_format() {
    let err = types::ErrorPayload {
        message: "Something went wrong".to_string(),
        traceback: Some("File foo.py, line 42".to_string()),
    };

    let json = serde_json::to_vec(&err).unwrap();
    let msg = Message::new(OpCode::Err, 123, json).unwrap();

    let (header, body) = msg.pack().unwrap();

    // OpCode.ERR = 6
    assert_eq!(header[1], 0x06);

    let parsed: types::ErrorPayload = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.message, "Something went wrong");
}

// ============================================================================
// ZMQ-based integration test (no Python required)
// ============================================================================

/// Test that we can receive and parse messages from a mock sentinel
#[tokio::test]
async fn test_zmq_message_exchange() {
    use tokio::time::timeout;

    // Create a ROUTER socket (like Python Sentinel)
    let mut router = RouterSocket::new();
    router.bind("tcp://127.0.0.1:15555").await.unwrap();

    // Create a DEALER socket (like Rust Worker)
    let mut dealer = zeromq::DealerSocket::new();
    dealer.connect("tcp://127.0.0.1:15555").await.unwrap();

    // Send IDENTIFY from dealer
    let identify = types::IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("test-worker".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    dealer.send(header.to_vec().into()).await.unwrap();
    dealer.send(body.into()).await.unwrap();

    // Receive on router (with timeout)
    let recv_result = timeout(Duration::from_secs(2), router.recv()).await;

    match recv_result {
        Ok(Ok(frame)) => {
            // Router receives identity frame first, then message
            println!("✓ Received message on router");
            let parts = frame.into_vec();
            assert!(!parts.is_empty(), "Should have received frames");
        }
        Ok(Err(e)) => {
            panic!("ZMQ error: {}", e);
        }
        Err(_) => {
            panic!("Timeout waiting for message");
        }
    }
}

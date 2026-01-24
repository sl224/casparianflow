//! Integration tests for Rust Worker with Python Sentinel
//!
//! These tests verify the Rust worker can communicate correctly
//! with the Python Sentinel over ZMQ.

use casparian_protocol::*;
use zmq::Context;

/// Test that protocol messages round-trip correctly
#[test]
fn test_protocol_message_roundtrip() {
    // Create a DISPATCH command
    let cmd = types::DispatchCommand {
        plugin_name: "test_plugin".to_string(),
        parser_version: Some("1.0.0".to_string()),
        file_path: "/data/input.csv".to_string(),
        sinks: vec![types::SinkConfig {
            topic: "output".to_string(),
            uri: "parquet://output.parquet".to_string(),
            mode: types::SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }],
        file_id: 1,
        runtime_kind: types::RuntimeKind::PythonShim,
        entrypoint: "test_plugin.py:Handler".to_string(),
        platform_os: None,
        platform_arch: None,
        signature_verified: false,
        signer_id: None,
        env_hash: Some("abc123def456".to_string()),
        source_code: Some("# test plugin".to_string()),
        artifact_hash: "artifact_hash_test".to_string(),
    };

    let payload = serde_json::to_vec(&cmd).unwrap();
    let msg = Message::new(OpCode::Dispatch, JobId::new(12345), payload).unwrap();

    // Pack and unpack
    let (header, body) = msg.pack().unwrap();
    let frames = vec![header.to_vec(), body];
    let unpacked = Message::unpack(&frames).unwrap();

    assert_eq!(unpacked.header.opcode, OpCode::Dispatch);
    assert_eq!(unpacked.header.job_id, JobId::new(12345));

    // Verify payload
    let unpacked_cmd: types::DispatchCommand = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(unpacked_cmd.plugin_name, "test_plugin");
    assert_eq!(unpacked_cmd.env_hash.as_deref(), Some("abc123def456"));
}

/// Test IDENTIFY message format
#[test]
fn test_identify_message_format() {
    let identify = types::IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("rust-worker-test".to_string()),
    };

    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, JobId::new(0), payload).unwrap();

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
        diagnostics: None,
        source_hash: Some("abc123def456".to_string()),
    };

    let payload = serde_json::to_vec(&receipt).unwrap();
    let msg = Message::new(OpCode::Conclude, JobId::new(99999), payload).unwrap();

    let (header, body) = msg.pack().unwrap();

    // Verify header
    assert_eq!(header[0], 0x04); // version
    assert_eq!(header[1], 0x05); // CONCLUDE = 5

    // Verify job_id is encoded correctly (big endian)
    let frames = vec![header.to_vec(), body];
    let unpacked = Message::unpack(&frames).unwrap();
    assert_eq!(unpacked.header.job_id, JobId::new(99999));
}

/// Test error message format
#[test]
fn test_error_message_format() {
    let err = types::ErrorPayload {
        message: "Something went wrong".to_string(),
        traceback: Some("File foo.py, line 42".to_string()),
    };

    let json = serde_json::to_vec(&err).unwrap();
    let msg = Message::new(OpCode::Err, JobId::new(123), json).unwrap();

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
#[test]
fn test_zmq_message_exchange() {
    let context = Context::new();

    let router = context.socket(zmq::ROUTER).unwrap();
    router.bind("tcp://127.0.0.1:15555").unwrap();
    router.set_rcvtimeo(2000).unwrap();

    let dealer = context.socket(zmq::DEALER).unwrap();
    dealer.connect("tcp://127.0.0.1:15555").unwrap();

    // Send IDENTIFY from dealer
    let identify = types::IdentifyPayload {
        capabilities: vec!["*".to_string()],
        worker_id: Some("test-worker".to_string()),
    };
    let payload = serde_json::to_vec(&identify).unwrap();
    let msg = Message::new(OpCode::Identify, JobId::new(0), payload).unwrap();
    let (header, body) = msg.pack().unwrap();

    let frames = [header.as_slice(), body.as_slice()];
    dealer.send_multipart(&frames, 0).unwrap();

    let parts = router.recv_multipart(0).unwrap();
    assert!(parts.len() >= 3, "Should have received identity + frames");
}

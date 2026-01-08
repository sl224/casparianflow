//! Protocol Compatibility Tests
//!
//! These tests verify that the Rust protocol implementation maintains
//! compatibility with the documented protocol specification.
//!
//! Note: The Python casparian_flow module has been deprecated. These tests
//! now verify protocol correctness against the documented specification rather
//! than cross-language interop.

use casparian_protocol::*;

/// Verify all OpCode values match the protocol specification
/// These values MUST remain stable for wire protocol compatibility
#[test]
fn test_all_opcodes_compatibility() {
    let opcodes = [
        (OpCode::Unknown, 0u8),
        (OpCode::Identify, 1),
        (OpCode::Dispatch, 2),
        (OpCode::Abort, 3),
        (OpCode::Heartbeat, 4),
        (OpCode::Conclude, 5),
        (OpCode::Err, 6),
        (OpCode::Reload, 7),
        (OpCode::PrepareEnv, 8),
        (OpCode::EnvReady, 9),
        (OpCode::Deploy, 10),
        (OpCode::Ack, 11),
    ];

    for (opcode, expected_value) in opcodes {
        assert_eq!(
            opcode.as_u8(),
            expected_value,
            "OpCode {:?} should have value {}",
            opcode,
            expected_value
        );
        assert_eq!(
            OpCode::from_u8(expected_value).unwrap(),
            opcode,
            "Value {} should parse to OpCode {:?}",
            expected_value,
            opcode
        );
    }
}

/// Verify header format matches protocol specification:
/// - 16 bytes total
/// - Big-endian byte order
/// - Format: [VER:1][OP:1][RES:2][JOB_ID:8][LEN:4]
#[test]
fn test_header_format_specification() {
    let header = Header::new(OpCode::Dispatch, 0x123456789ABCDEF0, 0x12345678);
    let packed = header.pack().unwrap();

    assert_eq!(packed.len(), 16, "Header must be exactly 16 bytes");

    // Verify byte order (big-endian)
    assert_eq!(packed[0], PROTOCOL_VERSION, "Version byte");
    assert_eq!(packed[1], OpCode::Dispatch.as_u8(), "OpCode byte");
    assert_eq!(packed[2], 0, "Reserved high byte");
    assert_eq!(packed[3], 0, "Reserved low byte");

    // Job ID in big-endian
    assert_eq!(packed[4], 0x12, "Job ID byte 0");
    assert_eq!(packed[5], 0x34, "Job ID byte 1");
    assert_eq!(packed[6], 0x56, "Job ID byte 2");
    assert_eq!(packed[7], 0x78, "Job ID byte 3");
    assert_eq!(packed[8], 0x9A, "Job ID byte 4");
    assert_eq!(packed[9], 0xBC, "Job ID byte 5");
    assert_eq!(packed[10], 0xDE, "Job ID byte 6");
    assert_eq!(packed[11], 0xF0, "Job ID byte 7");

    // Payload length in big-endian
    assert_eq!(packed[12], 0x12, "Payload len byte 0");
    assert_eq!(packed[13], 0x34, "Payload len byte 1");
    assert_eq!(packed[14], 0x56, "Payload len byte 2");
    assert_eq!(packed[15], 0x78, "Payload len byte 3");
}

/// Verify protocol version constant
#[test]
fn test_protocol_version() {
    assert_eq!(PROTOCOL_VERSION, 0x04, "Protocol version must be 4");
}

/// Verify message roundtrip works correctly
#[test]
fn test_message_roundtrip_compatibility() {
    use casparian_protocol::types::IdentifyPayload;

    // Create a payload
    let payload = IdentifyPayload {
        capabilities: vec!["plugin_a".to_string(), "plugin_b".to_string()],
        worker_id: Some("rust-worker-001".to_string()),
    };

    let payload_json = serde_json::to_string(&payload).unwrap();
    let msg = Message::new(OpCode::Identify, 42, payload_json.as_bytes().to_vec()).unwrap();

    // Pack and unpack
    let (header_bytes, payload_bytes) = msg.pack().unwrap();
    let frames = vec![header_bytes, payload_bytes];
    let unpacked = Message::unpack(&frames).unwrap();

    // Verify
    assert_eq!(unpacked.header.opcode, OpCode::Identify);
    assert_eq!(unpacked.header.job_id, 42);

    let decoded: IdentifyPayload = serde_json::from_slice(&unpacked.payload).unwrap();
    assert_eq!(decoded.worker_id, Some("rust-worker-001".to_string()));
    assert!(decoded.capabilities.contains(&"plugin_a".to_string()));
    assert!(decoded.capabilities.contains(&"plugin_b".to_string()));
}

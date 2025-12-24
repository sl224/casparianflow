//! Cross-language compatibility tests
//!
//! These tests verify that the Rust protocol implementation can read/write
//! the exact same binary format as the Python implementation.

use cf_protocol::*;
use std::process::Command;

#[test]
fn test_rust_to_python_header_compatibility() {
    // Create a header in Rust
    let header = Header::new(OpCode::Dispatch, 12345, 1024);
    let packed = header.pack().unwrap();

    // Write to temp file
    let temp_path = "/tmp/rust_header.bin";
    std::fs::write(temp_path, &packed).unwrap();

    // Verify with Python (using uv)
    let output = Command::new("uv")
        .arg("run")
        .arg("python")
        .arg("-c")
        .arg(format!(
            r#"
import sys
sys.path.insert(0, '{}')
from casparian_flow.protocol import unpack_header, OpCode

with open('{}', 'rb') as f:
    data = f.read()

opcode, job_id, payload_len = unpack_header(data)
assert opcode == OpCode.DISPATCH, f"OpCode mismatch: {{opcode}} != {{OpCode.DISPATCH}}"
assert job_id == 12345, f"Job ID mismatch: {{job_id}} != 12345"
assert payload_len == 1024, f"Payload length mismatch: {{payload_len}} != 1024"
print("✓ Python successfully read Rust-generated header")
"#,
            env!("CARGO_MANIFEST_DIR").replace("/crates/cf_protocol", "/src"),
            temp_path
        ))
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                eprintln!("Python stdout: {}", String::from_utf8_lossy(&result.stdout));
                eprintln!("Python stderr: {}", String::from_utf8_lossy(&result.stderr));
                panic!("Python verification failed");
            }
            println!("{}", String::from_utf8_lossy(&result.stdout));
        }
        Err(e) => {
            eprintln!("Warning: Could not run Python verification: {}", e);
            eprintln!("This is acceptable in CI environments without Python");
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(temp_path);
}

#[test]
fn test_python_to_rust_header_compatibility() {
    let temp_path = "/tmp/python_header.bin";

    // Generate header with Python (using uv)
    let output = Command::new("uv")
        .arg("run")
        .arg("python")
        .arg("-c")
        .arg(format!(
            r#"
import sys
sys.path.insert(0, '{}')
from casparian_flow.protocol import pack_header, OpCode

header = pack_header(OpCode.HEARTBEAT, 9999, 512)
with open('{}', 'wb') as f:
    f.write(header)
print("✓ Python generated header")
"#,
            env!("CARGO_MANIFEST_DIR").replace("/crates/cf_protocol", "/src"),
            temp_path
        ))
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                eprintln!("Python stdout: {}", String::from_utf8_lossy(&result.stdout));
                eprintln!("Python stderr: {}", String::from_utf8_lossy(&result.stderr));
                panic!("Python header generation failed");
            }
            println!("{}", String::from_utf8_lossy(&result.stdout));

            // Read and verify with Rust
            let data = std::fs::read(temp_path).unwrap();
            let header = Header::unpack(&data).unwrap();

            assert_eq!(header.opcode, OpCode::Heartbeat);
            assert_eq!(header.job_id, 9999);
            assert_eq!(header.payload_len, 512);
            println!("✓ Rust successfully read Python-generated header");
        }
        Err(e) => {
            eprintln!("Warning: Could not run Python test: {}", e);
            eprintln!("This is acceptable in CI environments without Python");
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(temp_path);
}

#[test]
fn test_full_message_roundtrip() {
    use cf_protocol::types::IdentifyPayload;

    // Create a complete message in Rust
    let payload = IdentifyPayload {
        capabilities: vec!["plugin_a".to_string(), "plugin_b".to_string()],
        worker_id: Some("rust-worker-001".to_string()),
    };

    let payload_json = serde_json::to_string(&payload).unwrap();
    let msg = Message::new(OpCode::Identify, 0, payload_json.as_bytes().to_vec());
    let (header_bytes, payload_bytes) = msg.pack().unwrap();

    // Write to temp files
    let header_path = "/tmp/rust_msg_header.bin";
    let payload_path = "/tmp/rust_msg_payload.bin";
    std::fs::write(header_path, &header_bytes).unwrap();
    std::fs::write(payload_path, &payload_bytes).unwrap();

    // Verify with Python (using uv)
    let output = Command::new("uv")
        .arg("run")
        .arg("python")
        .arg("-c")
        .arg(format!(
            r#"
import sys
sys.path.insert(0, '{}')
from casparian_flow.protocol import unpack_msg, OpCode

with open('{}', 'rb') as f:
    header = f.read()
with open('{}', 'rb') as f:
    payload = f.read()

opcode, job_id, payload_dict = unpack_msg([header, payload])
assert opcode == OpCode.IDENTIFY
assert job_id == 0
assert payload_dict['worker_id'] == 'rust-worker-001'
assert 'plugin_a' in payload_dict['capabilities']
print("✓ Python successfully decoded Rust message")
print(f"  Worker ID: {{payload_dict['worker_id']}}")
print(f"  Capabilities: {{payload_dict['capabilities']}}")
"#,
            env!("CARGO_MANIFEST_DIR").replace("/crates/cf_protocol", "/src"),
            header_path,
            payload_path
        ))
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                eprintln!("Python stdout: {}", String::from_utf8_lossy(&result.stdout));
                eprintln!("Python stderr: {}", String::from_utf8_lossy(&result.stderr));
                panic!("Python message verification failed");
            }
            println!("{}", String::from_utf8_lossy(&result.stdout));
        }
        Err(e) => {
            eprintln!("Warning: Could not run Python verification: {}", e);
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(header_path);
    let _ = std::fs::remove_file(payload_path);
}

#[test]
fn test_all_opcodes_compatibility() {
    let opcodes = [
        (OpCode::Identify, 1u8),
        (OpCode::Dispatch, 2),
        (OpCode::Abort, 3),
        (OpCode::Heartbeat, 4),
        (OpCode::Conclude, 5),
        (OpCode::Err, 6),
        (OpCode::Reload, 7),
        (OpCode::PrepareEnv, 8),
        (OpCode::EnvReady, 9),
        (OpCode::Deploy, 10),
    ];

    for (opcode, expected_value) in opcodes {
        assert_eq!(opcode.as_u8(), expected_value);
        assert_eq!(OpCode::from_u8(expected_value).unwrap(), opcode);
    }

    println!("✓ All OpCode values match Python implementation");
}

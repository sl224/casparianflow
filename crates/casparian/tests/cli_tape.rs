//! Tests for CLI tape recording functionality (WS7-02)

use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn casparian_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_casparian"))
}

#[derive(Debug, Deserialize)]
struct TapeEvent {
    schema_version: u32,
    event_id: String,
    seq: u64,
    event_name: TapeEventName,
    payload: serde_json::Value,
    correlation_id: Option<String>,
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "name")]
enum TapeEventName {
    #[serde(rename = "tape_started")]
    TapeStarted,
    #[serde(rename = "tape_stopped")]
    TapeStopped,
    #[serde(rename = "ui_command")]
    UICommand(String),
    #[serde(rename = "system_response")]
    SystemResponse(String),
    #[serde(rename = "error_event")]
    ErrorEvent(String),
    #[serde(rename = "domain_event")]
    DomainEvent(String),
}

fn read_tape_events(tape_path: &std::path::Path) -> Vec<TapeEvent> {
    let content = std::fs::read_to_string(tape_path).expect("Failed to read tape file");
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("Failed to parse tape event"))
        .collect()
}

#[test]
fn test_tape_records_successful_command() {
    let temp_dir = TempDir::new().unwrap();
    let tape_path = temp_dir.path().join("session.tape");
    // Use unique directory name to avoid database constraint violations on reruns
    let unique_name = format!("scan_target_{}", std::process::id());
    let scan_dir = temp_dir.path().join(&unique_name);
    std::fs::create_dir(&scan_dir).unwrap();
    std::fs::write(scan_dir.join("test.csv"), "a,b,c\n1,2,3\n").unwrap();

    let output = Command::new(casparian_bin())
        .env("CASPARIAN_HOME", temp_dir.path())
        .arg("--tape")
        .arg(&tape_path)
        .arg("scan")
        .arg(&scan_dir)
        .arg("--quiet")
        .output()
        .expect("Failed to run casparian");

    assert!(
        output.status.success(),
        "Command should succeed. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tape_path.exists(), "Tape file should be created");

    let events = read_tape_events(&tape_path);

    // Verify event structure
    assert!(events.len() >= 3, "Should have at least TapeStarted, UICommand, and SystemResponse");

    // First event should be TapeStarted
    assert_eq!(events[0].seq, 0);
    assert!(matches!(events[0].event_name, TapeEventName::TapeStarted));

    // UICommand(Scan) should be present near the start
    let ui_index = events
        .iter()
        .position(|event| matches!(event.event_name, TapeEventName::UICommand(ref name) if name == "Scan"))
        .expect("UICommand(Scan) should be recorded");
    let ui_event = &events[ui_index];
    match &ui_event.event_name {
        TapeEventName::UICommand(name) => assert_eq!(name, "Scan"),
        _ => panic!("Expected UICommand, got {:?}", ui_event.event_name),
    }
    // Verify path is redacted (hashed)
    let payload = &ui_event.payload;
    assert!(payload.get("path_hash").is_some(), "Path should be hashed");
    let path_hash = payload["path_hash"].as_str().unwrap();
    assert_eq!(path_hash.len(), 16, "Hash should be 16 chars");
    assert!(!path_hash.contains('/'), "Hash should not contain path characters");

    // Verify correlation_id links events
    let corr_id = ui_event
        .correlation_id
        .as_ref()
        .expect("UICommand should have correlation_id");

    // SystemResponse should appear with matching correlation_id
    let response_event = events.iter().find(|event| {
        matches!(event.event_name, TapeEventName::SystemResponse(ref name) if name == "CommandSucceeded")
            && event.correlation_id.as_ref() == Some(corr_id)
    });
    let response_event = response_event.expect("Expected SystemResponse with matching correlation_id");
    // SystemResponse should have parent_id pointing to UICommand
    assert!(response_event.parent_id.is_some());
}

#[test]
fn test_tape_records_failed_command() {
    let temp_dir = TempDir::new().unwrap();
    let tape_path = temp_dir.path().join("session.tape");
    let nonexistent_dir = temp_dir.path().join("does_not_exist");

    let output = Command::new(casparian_bin())
        .env("CASPARIAN_HOME", temp_dir.path())
        .arg("--tape")
        .arg(&tape_path)
        .arg("scan")
        .arg(&nonexistent_dir)
        .output()
        .expect("Failed to run casparian");

    // Command should fail (nonexistent directory)
    assert!(!output.status.success(), "Command should fail");
    assert!(tape_path.exists(), "Tape file should still be created");

    let events = read_tape_events(&tape_path);

    // Find the ErrorEvent
    let error_event = events.iter().find(|e| matches!(e.event_name, TapeEventName::ErrorEvent(_)));
    assert!(error_event.is_some(), "Should have an ErrorEvent");

    let error_event = error_event.unwrap();
    match &error_event.event_name {
        TapeEventName::ErrorEvent(name) => assert_eq!(name, "CommandFailed"),
        _ => panic!("Expected ErrorEvent"),
    }

    // Verify error payload
    let payload = &error_event.payload;
    assert_eq!(payload["status"], "failed");
    assert!(payload.get("error").is_some(), "Should have error message");
}

#[test]
fn test_tape_schema_version() {
    let temp_dir = TempDir::new().unwrap();
    let tape_path = temp_dir.path().join("session.tape");

    Command::new(casparian_bin())
        .arg("--tape")
        .arg(&tape_path)
        .arg("config")
        .output()
        .expect("Failed to run casparian");

    let events = read_tape_events(&tape_path);

    // All events should have schema_version = 1
    for event in &events {
        assert_eq!(event.schema_version, 1, "Schema version should be 1");
    }
}

#[test]
fn test_tape_monotonic_sequence() {
    let temp_dir = TempDir::new().unwrap();
    let tape_path = temp_dir.path().join("session.tape");

    Command::new(casparian_bin())
        .arg("--tape")
        .arg(&tape_path)
        .arg("config")
        .output()
        .expect("Failed to run casparian");

    let events = read_tape_events(&tape_path);

    // Verify sequence numbers are monotonically increasing
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, i as u64, "Sequence numbers should be monotonic");
    }
}

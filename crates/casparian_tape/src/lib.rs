//! Event recording and replay for Casparian Flow sessions.
//!
//! This crate provides a tape-based event recording system that captures
//! session events in NDJSON format for debugging, replay, and audit purposes.
//!
//! # Key Features
//!
//! - **Schema-versioned events**: All events use `EnvelopeV1` with explicit versioning
//! - **Monotonic sequencing**: Each event has a strictly increasing sequence number
//! - **Redaction by default**: Sensitive values are hashed with a session-specific salt
//! - **NDJSON format**: One JSON object per line for easy streaming and processing
//!
//! # Example
//!
//! ```no_run
//! use casparian_tape::{TapeWriter, EventName};
//! use std::path::Path;
//!
//! let writer = TapeWriter::new(Path::new("/tmp/session.tape")).unwrap();
//!
//! // Emit a domain event
//! writer.emit(
//!     EventName::DomainEvent("JobStateChanged".to_string()),
//!     None,
//!     None,
//!     serde_json::json!({ "job_id": 123, "new_state": "running" }),
//! ).unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

/// Current schema version for event envelopes.
pub const SCHEMA_VERSION: u32 = 1;

/// Errors that can occur during tape operations.
#[derive(Error, Debug)]
pub enum TapeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Failed to acquire lock")]
    LockError,
}

/// Event envelope containing metadata and payload.
///
/// All events are wrapped in this envelope to provide consistent
/// metadata across different event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeV1 {
    /// Schema version for this envelope format (always 1 for now)
    pub schema_version: u32,
    /// Unique identifier for this event (UUID v4)
    pub event_id: String,
    /// Monotonically increasing sequence number within the tape
    pub seq: u64,
    /// When the event was recorded
    pub timestamp: DateTime<Utc>,
    /// Optional correlation ID to group related events
    pub correlation_id: Option<String>,
    /// Optional parent event ID for causal relationships
    pub parent_id: Option<String>,
    /// The type/name of the event
    pub event_name: EventName,
    /// Event-specific payload data
    pub payload: serde_json::Value,
}

/// Classification of events in the tape.
///
/// Events are categorized by their source and purpose to enable
/// filtering and analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "name")]
pub enum EventName {
    /// Tape lifecycle: recording started
    #[serde(rename = "tape_started")]
    TapeStarted,
    /// Tape lifecycle: recording stopped
    #[serde(rename = "tape_stopped")]
    TapeStopped,
    /// UI command (semantic action, not raw input)
    /// Examples: "ExecuteQuery", "CancelJob", "ApproveSchema"
    #[serde(rename = "ui_command")]
    UICommand(String),
    /// Domain event from the system
    /// Examples: "JobStateChanged", "MaterializationRecorded"
    #[serde(rename = "domain_event")]
    DomainEvent(String),
    /// System response to a command or event
    /// Examples: "QuerySucceeded", "JobCancelled"
    #[serde(rename = "system_response")]
    SystemResponse(String),
    /// Error that occurred during processing
    /// Examples: "QueryFailed", "ValidationError"
    #[serde(rename = "error_event")]
    ErrorEvent(String),
}

/// Redaction modes for sensitive data.
///
/// Controls how sensitive values are transformed before recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactionMode {
    /// Replace sensitive values with their salted hash (default)
    #[default]
    Hash,
    /// Remove the field entirely from the payload
    Omit,
    /// Keep the value as-is (requires explicit opt-in)
    Plaintext,
}

/// Reference to an artifact produced by a job.
///
/// Used to record outputs without exposing raw URIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRefV1 {
    /// Type of artifact (e.g., "parquet", "csv", "json")
    pub kind: String,
    /// Name of the output (matches parser output declaration)
    pub output_name: String,
    /// Hashed URI for the artifact location
    pub uri_hash: String,
    /// Number of rows in the artifact, if applicable
    pub rows: Option<u64>,
}

/// Payload for the TapeStarted event.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TapeStartedPayload {
    /// Hash of the redaction salt (for verification, not the salt itself)
    salt_hash: String,
    /// Hostname where the tape was created
    hostname: String,
    /// Working directory at tape start
    cwd: String,
}

/// Writer for recording events to a tape file.
///
/// The writer maintains a monotonically increasing sequence number
/// and a redaction salt for hashing sensitive values.
///
/// Events are written in NDJSON format (one JSON object per line).
pub struct TapeWriter {
    file: Mutex<BufWriter<File>>,
    seq: AtomicU64,
    redaction_salt: [u8; 32],
}

impl TapeWriter {
    /// Create a new tape writer at the given path.
    ///
    /// This will create the file if it doesn't exist, or truncate it if it does.
    /// A `TapeStarted` event is automatically written as the first event.
    pub fn new(path: &Path) -> Result<Self, TapeError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let writer = BufWriter::new(file);

        // Generate a random salt for this session
        let mut redaction_salt = [0u8; 32];
        // Use uuid to generate random bytes (avoiding getrandom dependency)
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        redaction_salt[..16].copy_from_slice(uuid1.as_bytes());
        redaction_salt[16..].copy_from_slice(uuid2.as_bytes());

        let tape = Self {
            file: Mutex::new(writer),
            seq: AtomicU64::new(0),
            redaction_salt,
        };

        // Write the TapeStarted event
        tape.write_tape_started()?;

        Ok(tape)
    }

    /// Create a new tape writer with a specific salt (for testing).
    #[cfg(test)]
    fn new_with_salt(path: &Path, salt: [u8; 32]) -> Result<Self, TapeError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let writer = BufWriter::new(file);

        let tape = Self {
            file: Mutex::new(writer),
            seq: AtomicU64::new(0),
            redaction_salt: salt,
        };

        tape.write_tape_started()?;

        Ok(tape)
    }

    fn write_tape_started(&self) -> Result<String, TapeError> {
        let salt_hash = self.redact_bytes(&self.redaction_salt);
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "unknown".to_string());
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let payload = TapeStartedPayload {
            salt_hash,
            hostname,
            cwd,
        };

        self.emit(
            EventName::TapeStarted,
            None,
            None,
            serde_json::to_value(payload)?,
        )
    }

    /// Emit an event to the tape.
    ///
    /// Returns the event ID of the emitted event.
    ///
    /// # Arguments
    ///
    /// * `event_name` - The type/classification of the event
    /// * `correlation_id` - Optional ID to group related events
    /// * `parent_id` - Optional parent event ID for causal chains
    /// * `payload` - Event-specific data
    pub fn emit(
        &self,
        event_name: EventName,
        correlation_id: Option<&str>,
        parent_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<String, TapeError> {
        let event_id = Uuid::new_v4().to_string();
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);

        let envelope = EnvelopeV1 {
            schema_version: SCHEMA_VERSION,
            event_id: event_id.clone(),
            seq,
            timestamp: Utc::now(),
            correlation_id: correlation_id.map(String::from),
            parent_id: parent_id.map(String::from),
            event_name,
            payload,
        };

        let json = serde_json::to_string(&envelope)?;

        let mut file = self.file.lock().map_err(|_| TapeError::LockError)?;
        writeln!(file, "{}", json)?;
        file.flush()?;

        Ok(event_id)
    }

    /// Hash a string using the session's redaction salt.
    ///
    /// Returns a 16-character hex string (first 8 bytes of the blake3 hash).
    pub fn redact_string(&self, s: &str) -> String {
        self.redact_bytes(s.as_bytes())
    }

    fn redact_bytes(&self, data: &[u8]) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.redaction_salt);
        hasher.update(data);
        let hash = hasher.finalize();
        // Take first 8 bytes (16 hex chars)
        hash.to_hex()[..16].to_string()
    }

    /// Create an artifact reference with a hashed URI.
    pub fn create_artifact_ref(
        &self,
        kind: impl Into<String>,
        output_name: impl Into<String>,
        uri: &str,
        rows: Option<u64>,
    ) -> ArtifactRefV1 {
        ArtifactRefV1 {
            kind: kind.into(),
            output_name: output_name.into(),
            uri_hash: self.redact_string(uri),
            rows,
        }
    }

    /// Get the current sequence number (for testing).
    #[cfg(test)]
    fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_envelope_serialization_roundtrip() {
        let envelope = EnvelopeV1 {
            schema_version: SCHEMA_VERSION,
            event_id: "test-event-id".to_string(),
            seq: 42,
            timestamp: Utc::now(),
            correlation_id: Some("corr-123".to_string()),
            parent_id: None,
            event_name: EventName::DomainEvent("TestEvent".to_string()),
            payload: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: EnvelopeV1 = serde_json::from_str(&json).unwrap();

        assert_eq!(envelope.schema_version, deserialized.schema_version);
        assert_eq!(envelope.event_id, deserialized.event_id);
        assert_eq!(envelope.seq, deserialized.seq);
        assert_eq!(envelope.correlation_id, deserialized.correlation_id);
        assert_eq!(envelope.parent_id, deserialized.parent_id);
        assert_eq!(envelope.event_name, deserialized.event_name);
        assert_eq!(envelope.payload, deserialized.payload);
    }

    #[test]
    fn test_event_name_serialization() {
        let cases = vec![
            (EventName::TapeStarted, r#"{"type":"tape_started"}"#),
            (EventName::TapeStopped, r#"{"type":"tape_stopped"}"#),
            (
                EventName::UICommand("ExecuteQuery".to_string()),
                r#"{"type":"ui_command","name":"ExecuteQuery"}"#,
            ),
            (
                EventName::DomainEvent("JobStarted".to_string()),
                r#"{"type":"domain_event","name":"JobStarted"}"#,
            ),
            (
                EventName::SystemResponse("QuerySucceeded".to_string()),
                r#"{"type":"system_response","name":"QuerySucceeded"}"#,
            ),
            (
                EventName::ErrorEvent("ValidationError".to_string()),
                r#"{"type":"error_event","name":"ValidationError"}"#,
            ),
        ];

        for (event_name, expected_json) in cases {
            let json = serde_json::to_string(&event_name).unwrap();
            assert_eq!(json, expected_json, "Serialization mismatch for {:?}", event_name);

            let deserialized: EventName = serde_json::from_str(&json).unwrap();
            assert_eq!(event_name, deserialized, "Roundtrip mismatch for {:?}", event_name);
        }
    }

    #[test]
    fn test_seq_monotonic_increment() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let writer = TapeWriter::new(&path).unwrap();

        // TapeStarted is seq 0
        assert_eq!(writer.current_seq(), 1);

        // Emit several events and verify seq increments
        for expected_seq in 1..=5 {
            let _event_id = writer
                .emit(
                    EventName::DomainEvent(format!("Event{}", expected_seq)),
                    None,
                    None,
                    serde_json::json!({}),
                )
                .unwrap();
            assert_eq!(writer.current_seq(), expected_seq + 1);
        }

        // Read the file and verify seq numbers in events
        let contents = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 6); // TapeStarted + 5 events

        for (i, line) in lines.iter().enumerate() {
            let envelope: EnvelopeV1 = serde_json::from_str(line).unwrap();
            assert_eq!(envelope.seq, i as u64, "Seq mismatch at line {}", i);
        }
    }

    #[test]
    fn test_redaction_stable_with_same_salt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let salt = [0x42u8; 32]; // Fixed salt for testing
        let writer = TapeWriter::new_with_salt(&path, salt).unwrap();

        let value1 = "sensitive-data";
        let value2 = "other-data";

        // Same value should produce same hash
        let hash1a = writer.redact_string(value1);
        let hash1b = writer.redact_string(value1);
        assert_eq!(hash1a, hash1b, "Same value should produce same hash");

        // Different values should produce different hashes
        let hash2 = writer.redact_string(value2);
        assert_ne!(hash1a, hash2, "Different values should produce different hashes");

        // Hash should be 16 chars (8 bytes hex)
        assert_eq!(hash1a.len(), 16, "Hash should be 16 hex characters");
        assert!(
            hash1a.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hex characters"
        );
    }

    #[test]
    fn test_redaction_different_with_different_salt() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("test1.tape");
        let path2 = dir.path().join("test2.tape");

        let salt1 = [0x42u8; 32];
        let salt2 = [0x43u8; 32];

        let writer1 = TapeWriter::new_with_salt(&path1, salt1).unwrap();
        let writer2 = TapeWriter::new_with_salt(&path2, salt2).unwrap();

        let value = "sensitive-data";

        let hash1 = writer1.redact_string(value);
        let hash2 = writer2.redact_string(value);

        assert_ne!(
            hash1, hash2,
            "Same value with different salts should produce different hashes"
        );
    }

    #[test]
    fn test_tape_started_is_first_event() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let _writer = TapeWriter::new(&path).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let first_line = contents.lines().next().unwrap();
        let envelope: EnvelopeV1 = serde_json::from_str(first_line).unwrap();

        assert_eq!(envelope.seq, 0);
        assert_eq!(envelope.event_name, EventName::TapeStarted);
        assert_eq!(envelope.schema_version, SCHEMA_VERSION);

        // Verify payload contains salt_hash
        let payload: TapeStartedPayload = serde_json::from_value(envelope.payload).unwrap();
        assert_eq!(payload.salt_hash.len(), 16);
    }

    #[test]
    fn test_artifact_ref_creation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let salt = [0x42u8; 32];
        let writer = TapeWriter::new_with_salt(&path, salt).unwrap();

        let artifact = writer.create_artifact_ref(
            "parquet",
            "orders",
            "file:///path/to/orders.parquet",
            Some(1000),
        );

        assert_eq!(artifact.kind, "parquet");
        assert_eq!(artifact.output_name, "orders");
        assert_eq!(artifact.rows, Some(1000));
        assert_eq!(artifact.uri_hash.len(), 16);

        // URI hash should be reproducible
        let expected_hash = writer.redact_string("file:///path/to/orders.parquet");
        assert_eq!(artifact.uri_hash, expected_hash);
    }

    #[test]
    fn test_ndjson_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let writer = TapeWriter::new(&path).unwrap();

        writer
            .emit(
                EventName::UICommand("Test".to_string()),
                None,
                None,
                serde_json::json!({"key": "value"}),
            )
            .unwrap();

        let contents = fs::read_to_string(&path).unwrap();

        // Each line should be valid JSON
        for line in contents.lines() {
            let _: EnvelopeV1 = serde_json::from_str(line)
                .expect("Each line should be valid JSON");
        }

        // File should end with newline
        assert!(contents.ends_with('\n'));
    }

    #[test]
    fn test_correlation_and_parent_ids() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.tape");

        let writer = TapeWriter::new(&path).unwrap();

        let event1_id = writer
            .emit(
                EventName::UICommand("StartJob".to_string()),
                Some("session-123"),
                None,
                serde_json::json!({}),
            )
            .unwrap();

        let _event2_id = writer
            .emit(
                EventName::DomainEvent("JobStarted".to_string()),
                Some("session-123"),
                Some(&event1_id),
                serde_json::json!({}),
            )
            .unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();

        // Skip TapeStarted (line 0)
        let envelope1: EnvelopeV1 = serde_json::from_str(lines[1]).unwrap();
        let envelope2: EnvelopeV1 = serde_json::from_str(lines[2]).unwrap();

        assert_eq!(envelope1.correlation_id, Some("session-123".to_string()));
        assert_eq!(envelope1.parent_id, None);

        assert_eq!(envelope2.correlation_id, Some("session-123".to_string()));
        assert_eq!(envelope2.parent_id, Some(event1_id));
    }
}

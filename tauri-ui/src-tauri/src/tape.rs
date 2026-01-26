//! Tape recording for Tauri commands (WS7-05).
//!
//! This module provides tape instrumentation for the Tauri backend, recording
//! UICommand and SystemResponse events for debugging and audit purposes.
//!
//! # Privacy
//!
//! Sensitive data is automatically redacted:
//! - SQL queries are hashed (not stored in plaintext)
//! - File paths are hashed
//! - Query results are NOT recorded (only row counts)

use casparian_tape::{EventName, TapeWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

/// Tape state shared across Tauri commands.
///
/// The TapeWriter is wrapped in an Option to make tape recording optional.
/// When None, all tape operations are no-ops.
pub struct TapeState {
    writer: Option<TapeWriter>,
}

impl TapeState {
    /// Create a new TapeState without tape recording.
    pub fn disabled() -> Self {
        Self { writer: None }
    }

    /// Create a new TapeState with tape recording enabled.
    ///
    /// The tape file will be created at the specified path.
    pub fn enabled(tape_path: &Path) -> Result<Self, casparian_tape::TapeError> {
        let writer = TapeWriter::new(tape_path)?;
        Ok(Self {
            writer: Some(writer),
        })
    }

    /// Check if tape recording is enabled.
    pub fn is_enabled(&self) -> bool {
        self.writer.is_some()
    }

    /// Hash a string for redaction.
    ///
    /// Returns a 16-character hex hash. If tape is disabled, returns a placeholder.
    pub fn redact(&self, s: &str) -> String {
        match &self.writer {
            Some(writer) => writer.redact_string(s),
            None => "[no-tape]".to_string(),
        }
    }

    /// Emit a UICommand event.
    ///
    /// Returns the event_id and correlation_id for linking with the response.
    pub fn emit_command(
        &self,
        command_name: &str,
        payload: serde_json::Value,
    ) -> Option<(String, String)> {
        let writer = self.writer.as_ref()?;

        let correlation_id = uuid::Uuid::new_v4().to_string();

        match writer.emit(
            EventName::UICommand(command_name.to_string()),
            Some(&correlation_id),
            None,
            payload,
        ) {
            Ok(event_id) => {
                debug!(
                    "Tape: emitted UICommand({}) event_id={} correlation_id={}",
                    command_name, event_id, correlation_id
                );
                Some((event_id, correlation_id))
            }
            Err(e) => {
                warn!("Tape: failed to emit UICommand: {}", e);
                None
            }
        }
    }

    /// Emit a SystemResponse event for a successful command.
    pub fn emit_success(&self, correlation_id: &str, parent_id: &str, payload: serde_json::Value) {
        if let Some(writer) = &self.writer {
            if let Err(e) = writer.emit(
                EventName::SystemResponse("CommandSucceeded".to_string()),
                Some(correlation_id),
                Some(parent_id),
                payload,
            ) {
                warn!("Tape: failed to emit SystemResponse: {}", e);
            }
        }
    }

    /// Emit an ErrorEvent for a failed command.
    pub fn emit_error(
        &self,
        correlation_id: &str,
        parent_id: &str,
        error: &str,
        payload: serde_json::Value,
    ) {
        if let Some(writer) = &self.writer {
            let mut payload = payload;
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("error".to_string(), serde_json::json!(error));
            }

            if let Err(e) = writer.emit(
                EventName::ErrorEvent("CommandFailed".to_string()),
                Some(correlation_id),
                Some(parent_id),
                payload,
            ) {
                warn!("Tape: failed to emit ErrorEvent: {}", e);
            }
        }
    }
}

/// Thread-safe wrapper for TapeState.
pub type SharedTapeState = Arc<RwLock<TapeState>>;

static_assertions::assert_impl_all!(TapeState: Send, Sync);
static_assertions::assert_impl_all!(SharedTapeState: Send, Sync);

/// Create a shared tape state that is disabled (no recording).
pub fn create_disabled_tape() -> SharedTapeState {
    Arc::new(RwLock::new(TapeState::disabled()))
}

/// Create a shared tape state with recording enabled.
pub fn create_enabled_tape(tape_path: &Path) -> Result<SharedTapeState, casparian_tape::TapeError> {
    let state = TapeState::enabled(tape_path)?;
    Ok(Arc::new(RwLock::new(state)))
}

/// Get the default tape directory path.
///
/// Returns `~/.casparian_flow/tapes/`
pub fn default_tape_dir() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return Some(PathBuf::from(override_path).join("tapes"));
    }
    dirs::home_dir().map(|h| h.join(".casparian_flow").join("tapes"))
}

/// Generate a tape file path with timestamp.
///
/// Format: `{tape_dir}/tauri_{timestamp}.tape`
pub fn generate_tape_path(tape_dir: &Path) -> PathBuf {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    tape_dir.join(format!("tauri_{}.tape", timestamp))
}

/// Helper macro for recording commands with automatic tape instrumentation.
///
/// This macro wraps a command execution with tape recording:
/// 1. Emits UICommand before execution
/// 2. Emits SystemResponse or ErrorEvent after execution
/// 3. Links events via correlation_id and parent_id
///
/// # Usage
///
/// ```ignore
/// record_command!(
///     tape_state,
///     "CommandName",
///     json!({"param": value}),
///     || async { /* command logic */ }
/// )
/// ```
#[macro_export]
macro_rules! record_command {
    ($tape:expr, $name:expr, $payload:expr, $body:expr) => {{
        let ids = {
            let tape = $tape.read().ok();
            tape.as_ref()
                .and_then(|t| t.emit_command($name, $payload.clone()))
        };

        let result = $body;

        if let Some((event_id, correlation_id)) = ids {
            let tape = $tape.read().ok();
            if let Some(t) = tape.as_ref() {
                match &result {
                    Ok(_) => t.emit_success(
                        &correlation_id,
                        &event_id,
                        serde_json::json!({"status": "success"}),
                    ),
                    Err(e) => t.emit_error(
                        &correlation_id,
                        &event_id,
                        &e.to_string(),
                        serde_json::json!({"status": "failed"}),
                    ),
                }
            }
        }

        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tape_state_disabled() {
        let state = TapeState::disabled();
        assert!(!state.is_enabled());
        assert_eq!(state.redact("sensitive"), "[no-tape]");

        // emit_command returns None when disabled
        let result = state.emit_command("Test", serde_json::json!({}));
        assert!(result.is_none());
    }

    #[test]
    fn test_tape_state_enabled() {
        let dir = tempdir().unwrap();
        let tape_path = dir.path().join("test.tape");

        let state = TapeState::enabled(&tape_path).unwrap();
        assert!(state.is_enabled());

        // Redaction produces 16-char hash
        let hash = state.redact("sensitive data");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Same value produces same hash
        let hash2 = state.redact("sensitive data");
        assert_eq!(hash, hash2);

        // Different value produces different hash
        let hash3 = state.redact("other data");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_emit_command_creates_tape_file() {
        let dir = tempdir().unwrap();
        let tape_path = dir.path().join("test.tape");

        let state = TapeState::enabled(&tape_path).unwrap();

        let result = state.emit_command("TestCommand", serde_json::json!({"key": "value"}));
        assert!(result.is_some());

        let (event_id, correlation_id) = result.unwrap();
        assert!(!event_id.is_empty());
        assert!(!correlation_id.is_empty());

        // Verify tape file exists and contains the event
        assert!(tape_path.exists());
        let content = std::fs::read_to_string(&tape_path).unwrap();
        assert!(content.contains("TestCommand"));
        assert!(content.contains(&correlation_id));
    }

    #[test]
    fn test_emit_success_and_error() {
        let dir = tempdir().unwrap();
        let tape_path = dir.path().join("test.tape");

        let state = TapeState::enabled(&tape_path).unwrap();

        let (event_id, correlation_id) = state
            .emit_command("TestCommand", serde_json::json!({}))
            .unwrap();

        // Emit success
        state.emit_success(&correlation_id, &event_id, serde_json::json!({"rows": 100}));

        let content = std::fs::read_to_string(&tape_path).unwrap();
        assert!(content.contains("CommandSucceeded"));

        // Emit error on another command
        let (event_id2, correlation_id2) = state
            .emit_command("FailingCommand", serde_json::json!({}))
            .unwrap();

        state.emit_error(
            &correlation_id2,
            &event_id2,
            "Something went wrong",
            serde_json::json!({}),
        );

        let content = std::fs::read_to_string(&tape_path).unwrap();
        assert!(content.contains("CommandFailed"));
        assert!(content.contains("Something went wrong"));
    }

    #[test]
    fn test_generate_tape_path() {
        let dir = tempdir().unwrap();
        let path = generate_tape_path(dir.path());

        assert!(path.to_string_lossy().contains("tauri_"));
        assert!(path.to_string_lossy().ends_with(".tape"));
    }
}

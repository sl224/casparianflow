//! Protocol payload types (Pydantic model equivalents)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Sink Configuration
// ============================================================================

/// Configuration for a single data sink.
/// Worker will use this to instantiate the appropriate sink.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SinkConfig {
    pub topic: String,
    pub uri: String,
    #[serde(default = "default_mode")]
    pub mode: String, // "append" | "replace" | "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_def: Option<String>, // Renamed from schema_json to avoid conflicts
}

fn default_mode() -> String {
    "append".to_string()
}

// ============================================================================
// OpCode.DISPATCH (Sentinel -> Worker)
// ============================================================================

/// Payload for OpCode.DISPATCH.
/// Sentinel -> Worker: "Process this file in isolated venv with Bridge Mode."
///
/// v5.0: Bridge Mode is now mandatory. All execution happens in isolated subprocesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchCommand {
    pub plugin_name: String,
    pub file_path: String,
    pub sinks: Vec<SinkConfig>,
    pub file_version_id: i64, // Required for lineage restoration

    // Bridge Mode fields (now required)
    pub env_hash: String, // SHA256 of lockfile - links to PluginEnvironment
    pub source_code: String, // Plugin source code for subprocess execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>, // For signature verification (optional for legacy manifests)
}

// ============================================================================
// OpCode.CONCLUDE (Worker -> Sentinel)
// ============================================================================

/// Job completion status - type-safe enum instead of stringly-typed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Success,
    Failed,
    Rejected,  // Worker at capacity
    Aborted,   // Cancelled by sentinel
}

impl JobStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, JobStatus::Success)
    }

    pub fn is_failure(&self) -> bool {
        !self.is_success()
    }
}

/// Payload for OpCode.CONCLUDE.
/// Worker -> Sentinel: "Job finished. Here are the results."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobReceipt {
    pub status: JobStatus,
    pub metrics: HashMap<String, i64>, // e.g., {"rows": 1500, "size_bytes": 42000}
    pub artifacts: Vec<HashMap<String, String>>, // e.g., [{"topic": "output", "uri": "s3://..."}]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>, // Populated if status is failure
}

// ============================================================================
// OpCode.IDENTIFY (Worker -> Sentinel)
// ============================================================================

/// Payload for OpCode.IDENTIFY.
/// Worker -> Sentinel: Handshake with capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyPayload {
    pub capabilities: Vec<String>, // List of plugin names this worker can execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>, // Optional stable worker ID
}

// ============================================================================
// OpCode.HEARTBEAT (Worker -> Sentinel)
// ============================================================================

/// Payload for OpCode.HEARTBEAT.
/// Worker -> Sentinel: Status update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub status: String, // "IDLE" | "BUSY" | "ALIVE"
    /// First active job ID (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<i64>,
    /// Number of currently active jobs (0 to MAX_CONCURRENT_JOBS)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub active_job_count: usize,
    /// All active job IDs (for monitoring/debugging)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_job_ids: Vec<i64>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

// ============================================================================
// OpCode.ERR (Bidirectional)
// ============================================================================

/// Payload for OpCode.ERR.
/// Bidirectional: Error notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceback: Option<String>,
}

// ============================================================================
// v5.0 Bridge Mode: Environment Provisioning
// ============================================================================

/// Payload for OpCode.PREPARE_ENV.
/// Sentinel -> Worker: "Provision this environment before execution."
///
/// Enables Eager Provisioning to avoid network blocking during job execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareEnvCommand {
    pub env_hash: String, // SHA256 of lockfile content
    pub lockfile_content: String, // Raw TOML content (uv.lock)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_version: Option<String>, // e.g., "3.11"
}

/// Payload for OpCode.ENV_READY.
/// Worker -> Sentinel: "Environment is ready."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvReadyPayload {
    pub env_hash: String,
    pub interpreter_path: String, // Path to Python interpreter in venv
    #[serde(default)]
    pub cached: bool, // True if environment was already cached
}

// ============================================================================
// v5.0 Bridge Mode: Artifact Deployment
// ============================================================================

/// Payload for OpCode.DEPLOY.
/// CLI -> Sentinel: "Deploy this artifact to the registry."
///
/// Part of the Publisher workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployCommand {
    pub plugin_name: String,
    pub version: String,
    pub source_code: String,
    pub lockfile_content: String, // uv.lock content (empty string for legacy mode)
    pub env_hash: String,         // SHA256(lockfile_content)
    pub artifact_hash: String,    // SHA256(source_code + lockfile_content)
    pub signature: String,        // Ed25519 signature of artifact_hash
    pub publisher_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azure_oid: Option<String>, // For enterprise mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_requirements: Option<Vec<String>>, // e.g., ["glibc_2.31"]
}

/// Response to a DEPLOY command.
/// Sentinel -> CLI: "Deploy succeeded/failed."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sink_config_serialization() {
        let sink = SinkConfig {
            topic: "output".to_string(),
            uri: "s3://bucket/key".to_string(),
            mode: "append".to_string(),
            schema_def: None,
        };

        let json = serde_json::to_string(&sink).unwrap();
        let deserialized: SinkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(sink, deserialized);
    }

    #[test]
    fn test_identify_payload_serialization() {
        let payload = IdentifyPayload {
            capabilities: vec!["plugin_a".to_string(), "plugin_b".to_string()],
            worker_id: Some("worker-001".to_string()),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: IdentifyPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.capabilities, deserialized.capabilities);
        assert_eq!(payload.worker_id, deserialized.worker_id);
    }

    #[test]
    fn test_heartbeat_payload_serialization() {
        let payload = HeartbeatPayload {
            status: "BUSY".to_string(),
            current_job_id: Some(12345),
            active_job_count: 3,
            active_job_ids: vec![12345, 12346, 12347],
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: HeartbeatPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.status, deserialized.status);
        assert_eq!(payload.current_job_id, deserialized.current_job_id);
        assert_eq!(payload.active_job_count, deserialized.active_job_count);
        assert_eq!(payload.active_job_ids, deserialized.active_job_ids);
    }

    #[test]
    fn test_heartbeat_payload_backward_compat() {
        // Old payload without new fields should deserialize with defaults
        let old_json = r#"{"status":"ALIVE","current_job_id":123}"#;
        let payload: HeartbeatPayload = serde_json::from_str(old_json).unwrap();
        assert_eq!(payload.status, "ALIVE");
        assert_eq!(payload.current_job_id, Some(123));
        assert_eq!(payload.active_job_count, 0);
        assert!(payload.active_job_ids.is_empty());
    }

    #[test]
    fn test_job_receipt_serialization() {
        let mut metrics = HashMap::new();
        metrics.insert("rows".to_string(), 1500);
        metrics.insert("size_bytes".to_string(), 42000);

        let receipt = JobReceipt {
            status: JobStatus::Success,
            metrics,
            artifacts: vec![],
            error_message: None,
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: JobReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt.status, deserialized.status);
        assert_eq!(receipt.metrics, deserialized.metrics);
    }

    #[test]
    fn test_job_status_serialization() {
        // Test that JobStatus serializes to SCREAMING_SNAKE_CASE
        assert_eq!(
            serde_json::to_string(&JobStatus::Success).unwrap(),
            "\"SUCCESS\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Failed).unwrap(),
            "\"FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Rejected).unwrap(),
            "\"REJECTED\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Aborted).unwrap(),
            "\"ABORTED\""
        );

        // Test deserialization
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"SUCCESS\"").unwrap(),
            JobStatus::Success
        );
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"FAILED\"").unwrap(),
            JobStatus::Failed
        );
    }

    #[test]
    fn test_job_status_methods() {
        assert!(JobStatus::Success.is_success());
        assert!(!JobStatus::Success.is_failure());

        assert!(!JobStatus::Failed.is_success());
        assert!(JobStatus::Failed.is_failure());

        assert!(!JobStatus::Rejected.is_success());
        assert!(JobStatus::Rejected.is_failure());

        assert!(!JobStatus::Aborted.is_success());
        assert!(JobStatus::Aborted.is_failure());
    }
}

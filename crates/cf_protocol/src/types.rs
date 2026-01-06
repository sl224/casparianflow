//! Protocol payload types (Pydantic model equivalents)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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

// ============================================================================
// Shredder Types (v6.0)
// ============================================================================

/// How to split a multiplexed file into homogeneous shards
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShredStrategy {
    /// Regex with named capture group for shard key
    Regex {
        pattern: String,
        key_group: String, // Named group, e.g., "msg_type"
    },
    /// CSV column value determines shard
    CsvColumn {
        delimiter: u8,
        col_index: usize,
        has_header: bool,
    },
    /// JSON key path (streaming parser)
    JsonKey {
        key_path: String, // e.g., "event.type"
    },
    /// No shredding needed (homogeneous file)
    Passthrough,
}

impl Default for ShredStrategy {
    fn default() -> Self {
        ShredStrategy::Passthrough
    }
}

/// Confidence level of format detection
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DetectionConfidence {
    /// Heuristic is certain (e.g., valid CSV with consistent columns)
    High,
    /// Likely correct but user should verify
    Medium,
    /// Guessing - user MUST review
    Low,
    /// Need LLM assistance or manual specification
    Unknown,
}

impl Default for DetectionConfidence {
    fn default() -> Self {
        DetectionConfidence::Unknown
    }
}

/// Result of analyzing file head
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisResult {
    pub strategy: ShredStrategy,
    pub confidence: DetectionConfidence,
    /// First N unique shard keys found in sample
    pub sample_keys: Vec<String>,
    /// Distinct keys found in sample
    pub estimated_shard_count: usize,
    /// How much of the file we read for analysis
    pub head_bytes: usize,
    /// Human-readable explanation of detection
    pub reasoning: String,
    /// Warning message if high cardinality detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Configuration for shredding operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredConfig {
    pub strategy: ShredStrategy,
    pub output_dir: PathBuf,
    /// Maximum number of open file handles (default: 200)
    #[serde(default = "default_max_handles")]
    pub max_handles: usize,
    /// Number of top keys to get dedicated files; rest go to _MISC (default: 5)
    #[serde(default = "default_top_n_shards")]
    pub top_n_shards: usize,
    /// Buffer size for I/O (default: 64KB)
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Threshold to promote key from _MISC to dedicated file (default: 1000)
    #[serde(default = "default_promotion_threshold")]
    pub promotion_threshold: u64,
}

fn default_max_handles() -> usize {
    200
}
fn default_top_n_shards() -> usize {
    5
}
fn default_buffer_size() -> usize {
    65536 // 64KB
}
fn default_promotion_threshold() -> u64 {
    1000
}

impl Default for ShredConfig {
    fn default() -> Self {
        Self {
            strategy: ShredStrategy::default(),
            output_dir: PathBuf::from("output"),
            max_handles: default_max_handles(),
            top_n_shards: default_top_n_shards(),
            buffer_size: default_buffer_size(),
            promotion_threshold: default_promotion_threshold(),
        }
    }
}

/// Metadata about a generated shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMeta {
    pub path: PathBuf,
    /// The shard key value
    pub key: String,
    pub row_count: u64,
    pub byte_size: u64,
    /// Did we clone header to this shard?
    pub has_header: bool,
    pub first_source_offset: u64,
    pub last_source_offset: u64,
}

/// Result of shredding operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredResult {
    pub shards: Vec<ShardMeta>,
    /// Path to _MISC file for rare types (if created)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freezer_path: Option<PathBuf>,
    /// How many distinct keys ended up in freezer
    pub freezer_key_count: usize,
    pub total_rows: u64,
    pub duration_ms: u64,
    /// Path to sidecar lineage index file
    pub lineage_index_path: PathBuf,
}

/// Block-based lineage record (10KB blocks for efficiency)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageBlock {
    pub block_id: u64,
    pub source_offset_start: u64,
    pub source_offset_end: u64,
    pub shard_key: String,
    pub row_count_in_block: u32,
    pub first_row_number_in_shard: u64,
}

/// Checkpoint for resumable shredding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredCheckpoint {
    pub job_id: i64,
    pub last_source_offset: u64,
    /// key -> bytes written so far
    pub shards_written: HashMap<String, u64>,
    pub checkpointed_at: DateTime<Utc>,
}

/// Parser draft for user review (LLM-generated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserDraft {
    pub shard_key: String,
    pub source_code: String,
    /// First N rows of sample input
    pub sample_input: Vec<String>,
    /// Rendered table output (if validated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_output: Option<String>,
    /// Validation error (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Parser that user has approved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedParser {
    pub id: i64,
    pub shard_key: String,
    pub source_code: String,
    pub source_hash: String,
    pub approved_at: DateTime<Utc>,
    /// "user" or future: username
    pub approved_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtest_results: Option<BacktestResult>,
}

/// Result of running parser against historical files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub files_tested: usize,
    pub total_rows: u64,
    pub success_count: u64,
    pub failure_count: u64,
    /// First 10 failures for inspection
    pub failure_samples: Vec<ParseFailure>,
    pub tested_at: DateTime<Utc>,
}

/// A single parse failure with full lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseFailure {
    pub source_file: PathBuf,
    pub source_offset: u64,
    /// The actual line that failed
    pub raw_line: String,
    pub error_message: String,
    /// Which column failed (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_index: Option<usize>,
}

/// Hop in a lineage chain (for multi-hop tracing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageHop {
    pub file_path: PathBuf,
    pub file_type: LineageFileType,
    pub offset: u64,
    pub row_number: u64,
}

/// Type of file in lineage chain
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageFileType {
    Original,
    Shard,
    Freezer,
    ExtractedShard,
}

/// Full lineage chain from output back to source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageChain {
    pub hops: Vec<LineageHop>,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LlmProvider {
    Anthropic,
    OpenAi,
    Ollama { endpoint: String },
    /// Manual parser writing only (no LLM)
    None,
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::None
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    #[serde(flatten)]
    pub provider: LlmProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_max_tokens() -> usize {
    4096
}
fn default_temperature() -> f32 {
    0.0
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

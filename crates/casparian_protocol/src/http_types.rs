//! HTTP API types for the Local Control Plane API.
//!
//! These types are used by the Sentinel HTTP API server and clients (MCP, CLI, TUI).
//! All types use serde for JSON serialization with strict enum tagging.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use thiserror::Error;

use crate::types::{DataType, ProcessingStatus, SchemaColumnSpec};

// ============================================================================
// Event Types
// ============================================================================

/// Unique identifier for an event within a job.
/// Monotonically increasing per job to ensure ordering.
pub type EventId = u64;

/// Unique identifier for an API job (cf_api_jobs).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord, Default,
)]
#[serde(transparent)]
pub struct ApiJobId(u64);

impl ApiJobId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub fn to_i64(self) -> Result<i64, ApiJobIdError> {
        i64::try_from(self.0).map_err(|_| ApiJobIdError::Overflow(self.0 as u128))
    }
}

impl fmt::Display for ApiJobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for ApiJobId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ApiJobId> for u64 {
    fn from(value: ApiJobId) -> Self {
        value.0
    }
}

impl TryFrom<i64> for ApiJobId {
    type Error = ApiJobIdError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(ApiJobIdError::Negative(value as i128));
        }
        Ok(ApiJobId::new(value as u64))
    }
}

impl TryFrom<i128> for ApiJobId {
    type Error = ApiJobIdError;

    fn try_from(value: i128) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(ApiJobIdError::Negative(value));
        }
        if value > u64::MAX as i128 {
            return Err(ApiJobIdError::Overflow(value as u128));
        }
        Ok(ApiJobId::new(value as u64))
    }
}

impl TryFrom<ApiJobId> for i64 {
    type Error = ApiJobIdError;

    fn try_from(value: ApiJobId) -> Result<Self, Self::Error> {
        value.to_i64()
    }
}

impl std::str::FromStr for ApiJobId {
    type Err = ApiJobIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s
            .trim()
            .parse::<u64>()
            .map_err(|_| ApiJobIdError::Parse(s.to_string()))?;
        Ok(ApiJobId::new(value))
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ApiJobIdError {
    #[error("api job id cannot be negative: {0}")]
    Negative(i128),
    #[error("api job id does not fit in u64: {0}")]
    Overflow(u128),
    #[error("invalid api job id: {0}")]
    Parse(String),
}

/// Event types emitted during job execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventType {
    /// Job has started execution
    JobStarted,
    /// Job entered a new phase (e.g., "parsing", "validation", "writing")
    Phase { name: String },
    /// Progress update with items completed
    Progress {
        items_done: u64,
        items_total: Option<u64>,
        message: Option<String>,
    },
    /// Schema violation detected
    Violation { violations: Vec<ViolationSummary> },
    /// Output materialized to sink
    Output {
        output_name: String,
        sink_uri: String,
        rows: u64,
        bytes: Option<u64>,
    },
    /// Job has finished (success or failure)
    JobFinished {
        status: HttpJobStatus,
        error_message: Option<String>,
    },
    /// Approval required (job is waiting)
    ApprovalRequired { approval_id: String },
}

/// Summary of violations for an event (aggregated).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViolationSummary {
    pub violation_type: ViolationType,
    pub column_name: Option<String>,
    pub count: u64,
    /// Sample values (redacted according to policy)
    pub samples: Vec<String>,
}

/// Type of schema violation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    TypeMismatch,
    NullNotAllowed,
    FormatMismatch,
    ColumnMissing,
    ColumnExtra,
    ColumnOrderMismatch,
}

/// Event record stored in the database and returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: EventId,
    pub job_id: ApiJobId,
    pub timestamp: String, // RFC3339
    #[serde(flatten)]
    pub event_type: EventType,
}

// ============================================================================
// Job Types for HTTP API
// ============================================================================

/// Job type for the HTTP API.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpJobType {
    /// Parser execution on files
    Run,
    /// Multi-file validation backtest
    Backtest,
    /// Preview (no output written)
    Preview,
}

/// Job status for the HTTP API.
/// Maps to ProcessingStatus but with HTTP-friendly naming.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl From<ProcessingStatus> for HttpJobStatus {
    fn from(status: ProcessingStatus) -> Self {
        match status {
            ProcessingStatus::Pending
            | ProcessingStatus::Queued
            | ProcessingStatus::Dispatching => HttpJobStatus::Queued,
            ProcessingStatus::Running | ProcessingStatus::Staged => HttpJobStatus::Running,
            ProcessingStatus::Completed => HttpJobStatus::Completed,
            ProcessingStatus::Aborted => HttpJobStatus::Cancelled,
            ProcessingStatus::Failed => HttpJobStatus::Failed,
            ProcessingStatus::Skipped => HttpJobStatus::Cancelled,
        }
    }
}

/// Job specification for creating a new job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    /// Job type
    pub job_type: HttpJobType,
    /// Plugin name to execute
    pub plugin_name: String,
    /// Plugin version (optional, uses latest if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_version: Option<String>,
    /// Input directory containing files to process
    pub input_dir: String,
    /// Output sink URI (optional, uses default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Schema overrides per output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<HashMap<String, SchemaSpec>>,
}

/// Schema specification for an output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSpec {
    pub columns: Vec<SchemaColumnSpec>,
    #[serde(default)]
    pub mode: SchemaMode,
}

/// Schema validation mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchemaMode {
    /// Fail on any schema mismatch
    #[default]
    Strict,
    /// Allow extra columns in output
    AllowExtra,
    /// Allow missing optional columns
    AllowMissingOptional,
}

/// Full job record returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub job_id: ApiJobId,
    pub job_type: HttpJobType,
    pub status: HttpJobStatus,
    pub plugin_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_version: Option<String>,
    pub input_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub created_at: String, // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>, // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>, // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Progress information (if running)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<JobProgress>,
    /// Result information (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JobResult>,
    /// Serialized job specification (internal use; optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_json: Option<String>,
}

/// Job progress information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub phase: String,
    pub items_done: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Job result information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub rows_processed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_written: Option<u64>,
    pub outputs: Vec<OutputInfo>,
    pub metrics: HashMap<String, i64>,
}

/// Information about a single output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub sink_uri: String,
    pub rows: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

// ============================================================================
// Approval Types
// ============================================================================

/// Approval status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

/// Approval operation type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalOperation {
    /// Parser execution approval
    Run {
        plugin_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin_version: Option<String>,
        input_dir: String,
        file_count: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
    /// Schema promotion approval
    SchemaPromote {
        plugin_name: String,
        output_name: String,
        schema: SchemaSpec,
    },
}

/// Approval request record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub approval_id: String,
    pub status: ApprovalStatus,
    pub operation: ApprovalOperation,
    pub summary: String,
    pub created_at: String, // RFC3339
    pub expires_at: String, // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>, // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<ApiJobId>,
}

/// Decision for an approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub decision: ApprovalDecisionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Approval decision type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionType {
    Approve,
    Reject,
}

// ============================================================================
// Query Types
// ============================================================================

/// Redaction mode for sensitive data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RedactionMode {
    /// No redaction (requires explicit opt-in)
    None,
    /// Truncate strings to max length
    Truncate,
    /// Hash sensitive values (default)
    #[default]
    Hash,
}

/// Redaction policy for query responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPolicy {
    #[serde(default)]
    pub mode: RedactionMode,
    /// Maximum number of sample values to return (default: 5)
    #[serde(default = "default_max_sample_count")]
    pub max_sample_count: usize,
    /// Maximum length of string values (default: 100)
    #[serde(default = "default_max_value_length")]
    pub max_value_length: usize,
}

fn default_max_sample_count() -> usize {
    5
}

fn default_max_value_length() -> usize {
    100
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            mode: RedactionMode::Hash,
            max_sample_count: default_max_sample_count(),
            max_value_length: default_max_value_length(),
        }
    }
}

/// Request body for the query endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// SQL query (SELECT, WITH, EXPLAIN only)
    pub sql: String,
    /// Maximum rows to return (default: 1000)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Query timeout in milliseconds (default: 30000)
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Redaction policy for results
    #[serde(default)]
    pub redaction: RedactionPolicy,
}

fn default_limit() -> usize {
    1000
}

fn default_timeout_ms() -> u64 {
    30000
}

impl Default for QueryRequest {
    fn default() -> Self {
        Self {
            sql: String::new(),
            limit: default_limit(),
            timeout_ms: default_timeout_ms(),
            redaction: RedactionPolicy::default(),
        }
    }
}

/// Response from the query endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Column names
    pub columns: Vec<String>,
    /// Column types
    pub types: Vec<DataType>,
    /// Rows as arrays of JSON values
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Total rows returned
    pub row_count: usize,
    /// Whether results were truncated due to limit
    pub truncated: bool,
    /// Execution time in milliseconds
    pub execution_ms: u64,
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Response for POST /jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobResponse {
    pub job_id: ApiJobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
}

/// Response for GET /jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListJobsResponse {
    pub jobs: Vec<Job>,
    pub total: usize,
}

/// Response for GET /jobs/{job_id}/events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEventsResponse {
    pub events: Vec<Event>,
    /// Last event ID (for polling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_id: Option<EventId>,
}

/// Response for GET /approvals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListApprovalsResponse {
    pub approvals: Vec<Approval>,
    pub total: usize,
}

/// Response for POST /approvals/{id}/decide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecideResponse {
    pub approval_id: String,
    pub status: ApprovalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<ApiJobId>,
}

/// Response for GET /health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Response for GET /version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionResponse {
    pub version: String,
    pub protocol_version: String,
    pub build_info: Option<String>,
}

/// Dataset summary for GET /datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSummary {
    pub name: String,
    pub plugin_name: String,
    pub sink_uri: String,
    pub row_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
    pub last_updated: String, // RFC3339
}

/// Response for GET /datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDatasetsResponse {
    pub datasets: Vec<DatasetSummary>,
}

/// Quarantine summary for GET /quarantine/summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineSummary {
    pub total_rows: u64,
    pub by_plugin: HashMap<String, u64>,
    pub by_violation_type: HashMap<String, u64>,
}

// ============================================================================
// Control Plane Discovery File
// ============================================================================

/// Control plane discovery file written to ~/.casparian_flow/control_plane.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlaneDiscovery {
    /// Protocol version (e.g., "0.1")
    pub protocol_version: String,
    /// Server address (e.g., "127.0.0.1:54321")
    pub address: String,
    /// Bearer token for authentication
    pub token: String,
    /// Server PID for process management
    pub pid: u32,
    /// Server start time (RFC3339)
    pub started_at: String,
}

// ============================================================================
// Error Response
// ============================================================================

/// Standard error response for the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_job_id_json_roundtrip() {
        let id = ApiJobId::new(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");
        let parsed: ApiJobId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_event_type_serialization() {
        let event = EventType::Progress {
            items_done: 100,
            items_total: Some(1000),
            message: Some("Processing files".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"progress\""));
        assert!(json.contains("\"items_done\":100"));

        let deserialized: EventType = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_job_status_from_processing_status() {
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Pending),
            HttpJobStatus::Queued
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Queued),
            HttpJobStatus::Queued
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Running),
            HttpJobStatus::Running
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Completed),
            HttpJobStatus::Completed
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Failed),
            HttpJobStatus::Failed
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Aborted),
            HttpJobStatus::Cancelled
        );
        assert_eq!(
            HttpJobStatus::from(ProcessingStatus::Skipped),
            HttpJobStatus::Cancelled
        );
    }

    #[test]
    fn test_approval_operation_serialization() {
        let op = ApprovalOperation::Run {
            plugin_name: "test_parser".to_string(),
            plugin_version: Some("1.0.0".to_string()),
            input_dir: "/data/input".to_string(),
            file_count: 100,
            output: Some("parquet://./output".to_string()),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"type\":\"run\""));
        assert!(json.contains("\"plugin_name\":\"test_parser\""));

        let deserialized: ApprovalOperation = serde_json::from_str(&json).unwrap();
        match deserialized {
            ApprovalOperation::Run { plugin_name, .. } => {
                assert_eq!(plugin_name, "test_parser");
            }
            _ => panic!("Expected Run operation"),
        }
    }

    #[test]
    fn test_query_request_defaults() {
        let req: QueryRequest = serde_json::from_str(r#"{"sql": "SELECT 1"}"#).unwrap();
        assert_eq!(req.sql, "SELECT 1");
        assert_eq!(req.limit, 1000);
        assert_eq!(req.timeout_ms, 30000);
        assert_eq!(req.redaction.mode, RedactionMode::Hash);
    }

    #[test]
    fn test_redaction_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&RedactionMode::None).unwrap(),
            "\"none\""
        );
        assert_eq!(
            serde_json::to_string(&RedactionMode::Truncate).unwrap(),
            "\"truncate\""
        );
        assert_eq!(
            serde_json::to_string(&RedactionMode::Hash).unwrap(),
            "\"hash\""
        );
    }

    #[test]
    fn test_error_response() {
        let err = ErrorResponse::new("Not found", "NOT_FOUND")
            .with_details(serde_json::json!({"resource": "job", "id": 123}));
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"error\":\"Not found\""));
        assert!(json.contains("\"code\":\"NOT_FOUND\""));
    }

    #[test]
    fn test_control_plane_discovery() {
        let discovery = ControlPlaneDiscovery {
            protocol_version: "0.1".to_string(),
            address: "127.0.0.1:54321".to_string(),
            token: "secret-token".to_string(),
            pid: 12345,
            started_at: "2024-01-15T10:30:00Z".to_string(),
        };
        let json = serde_json::to_string_pretty(&discovery).unwrap();
        let deserialized: ControlPlaneDiscovery = serde_json::from_str(&json).unwrap();
        assert_eq!(discovery.protocol_version, deserialized.protocol_version);
        assert_eq!(discovery.address, deserialized.address);
    }
}

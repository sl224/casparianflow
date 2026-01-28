//! Control Plane API for Casparian Sentinel
//!
//! Provides a ZMQ-based API for UI/CLI to query and mutate sentinel state
//! without direct database access. This enables concurrent access while
//! sentinel is running.
//!
//! # Protocol
//!
//! Uses ZMQ ROUTER socket with JSON request/response protocol:
//! - Request: JSON-encoded `ControlRequest` (ROUTER identity frames handled by sentinel)
//! - Response: JSON-encoded `ControlResponse`
//!
//! # Supported Operations
//!
//! - `ListJobs` / `GetJob` / `CancelJob` / `GetQueueStats`
//! - `ListApprovals` / `CreateApproval` / `GetApproval` / `Approve` / `Reject`
//! - `SetApprovalJobId` / `ExpireApprovals`
//! - `CreateApiJob` / `GetApiJob` / `ListApiJobs`
//! - `UpdateApiJobStatus` / `UpdateApiJobProgress` / `UpdateApiJobResult` / `UpdateApiJobError`
//! - `CancelApiJob`
//! - `CreateSession` / `GetSession` / `ListSessions` / `ListSessionsNeedingInput`
//! - `AdvanceSession` / `CancelSession`

use casparian_protocol::http_types::{
    Approval, ApprovalOperation, ApprovalStatus, HttpJobStatus, HttpJobType, Job as ApiJob,
    JobProgress as ApiJobProgress, JobResult as ApiJobResult,
};
use casparian_protocol::{ApiJobId, JobId, ProcessingStatus};
use casparian_scout::types::{SourceId, SourceType, TagSource, TaggingRuleId, WorkspaceId};
use serde::{Deserialize, Serialize};

use crate::db::{IntentState, Session, SessionId};

/// Default Control API address (TCP loopback).
pub const DEFAULT_CONTROL_ADDR: &str = casparian_protocol::defaults::DEFAULT_CONTROL_ADDR;

/// Control API request envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ControlRequest {
    /// List jobs with optional filter
    ListJobs {
        status: Option<ProcessingStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    },
    /// Get a single job by ID
    GetJob { job_id: JobId },
    /// Request cancellation of a job
    CancelJob { job_id: JobId },
    /// Get queue statistics
    GetQueueStats,
    /// Create an API job (cf_api_jobs)
    CreateApiJob {
        job_type: HttpJobType,
        plugin_name: String,
        plugin_version: Option<String>,
        input_dir: String,
        output: Option<String>,
        approval_id: Option<String>,
        spec_json: Option<String>,
    },
    /// Get a single API job by ID
    GetApiJob { job_id: ApiJobId },
    /// List API jobs with optional status filter
    ListApiJobs {
        status: Option<HttpJobStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    },
    /// Update API job status
    UpdateApiJobStatus {
        job_id: ApiJobId,
        status: HttpJobStatus,
    },
    /// Update API job progress
    UpdateApiJobProgress {
        job_id: ApiJobId,
        progress: ApiJobProgress,
    },
    /// Update API job result
    UpdateApiJobResult {
        job_id: ApiJobId,
        result: ApiJobResult,
    },
    /// Update API job error
    UpdateApiJobError { job_id: ApiJobId, error: String },
    /// Cancel an API job
    CancelApiJob { job_id: ApiJobId },
    /// List approvals with optional status filter
    ListApprovals {
        status: Option<ApprovalStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    },
    /// Create a new approval request
    CreateApproval {
        approval_id: String,
        operation: ApprovalOperation,
        summary: String,
        expires_in_seconds: i64,
    },
    /// Get a single approval by ID
    GetApproval { approval_id: String },
    /// Approve an approval request
    Approve { approval_id: String },
    /// Reject an approval request with reason
    Reject { approval_id: String, reason: String },
    /// Link a job ID to an approval
    SetApprovalJobId {
        approval_id: String,
        job_id: ApiJobId,
    },
    /// Expire pending approvals that are past their expiry
    ExpireApprovals,
    /// Create a new session
    CreateSession {
        intent_text: String,
        input_dir: Option<String>,
    },
    /// Get a session by ID
    GetSession { session_id: SessionId },
    /// List sessions with optional state filter
    ListSessions {
        state: Option<IntentState>,
        limit: Option<i64>,
    },
    /// List sessions that need human input (at gates)
    ListSessionsNeedingInput { limit: Option<i64> },
    /// Advance a session to a new state
    AdvanceSession {
        session_id: SessionId,
        target_state: IntentState,
    },
    /// Cancel a session
    CancelSession { session_id: SessionId },
    // =====================================================================
    // Scout (Sources / Rules / Tags / Scans)
    // =====================================================================
    /// List sources for a workspace
    ListSources { workspace_id: WorkspaceId },
    /// Create or update a source
    UpsertSource { source: ScoutSourceInfo },
    /// Update a source name/path
    UpdateSource {
        source_id: SourceId,
        name: Option<String>,
        path: Option<String>,
    },
    /// Delete a source
    DeleteSource { source_id: SourceId },
    /// Touch source for MRU ordering
    TouchSource { source_id: SourceId },
    /// List tagging rules for a workspace
    ListRules { workspace_id: WorkspaceId },
    /// Create a tagging rule
    CreateRule {
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
        pattern: String,
        tag: String,
    },
    /// Update a rule (enabled flag)
    UpdateRuleEnabled {
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
        enabled: bool,
    },
    /// Delete a rule
    DeleteRule {
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
    },
    /// List tags + counts for a workspace/source
    ListTags {
        workspace_id: WorkspaceId,
        source_id: SourceId,
    },
    /// Apply a tag to a file (manual or rule-based)
    ApplyTag {
        workspace_id: WorkspaceId,
        file_id: i64,
        tag: String,
        tag_source: TagSource,
        rule_id: Option<TaggingRuleId>,
    },
    /// Start a filesystem scan
    StartScan {
        workspace_id: Option<WorkspaceId>,
        path: String,
    },
    /// Get scan status
    GetScan { scan_id: String },
    /// List scans
    ListScans { limit: Option<usize> },
    /// Cancel a running scan
    CancelScan { scan_id: String },
    /// Ping/health check
    Ping,
}

/// Control API response envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ControlResponse {
    /// List of jobs
    Jobs(Vec<JobInfo>),
    /// Single job (None if not found)
    Job(Option<JobInfo>),
    /// Result of cancel operation
    CancelResult { success: bool, message: String },
    /// Queue statistics
    QueueStats(QueueStatsInfo),
    /// Single API job (None if not found)
    ApiJob(Option<ApiJob>),
    /// List of API jobs
    ApiJobs(Vec<ApiJob>),
    /// API job creation result
    ApiJobCreated { job_id: ApiJobId },
    /// API job mutation result
    ApiJobResult { success: bool, message: String },
    /// List of approvals
    Approvals(Vec<Approval>),
    /// Single approval (None if not found)
    Approval(Option<Approval>),
    /// Result of approval decision
    ApprovalResult { success: bool, message: String },
    /// Single session (None if not found)
    Session(Option<Session>),
    /// List of sessions
    Sessions(Vec<Session>),
    /// Session creation result
    SessionCreated { session_id: SessionId },
    /// Result of session update
    SessionResult { success: bool, message: String },
    /// List of sources
    Sources(Vec<ScoutSourceInfo>),
    /// Source mutation result
    SourceResult { success: bool, message: String },
    /// List of rules
    Rules(Vec<ScoutRuleInfo>),
    /// Rule mutation result
    RuleResult { success: bool, message: String },
    /// Tag stats (counts + totals)
    TagStats(ScoutTagStats),
    /// Tag mutation result
    TagResult { success: bool, message: String },
    /// Scan started
    ScanStarted { scan_id: String },
    /// Scan status
    ScanStatus(Option<ScoutScanStatus>),
    /// List of scans
    Scans(Vec<ScoutScanStatus>),
    /// Scan mutation result
    ScanResult { success: bool, message: String },
    /// Pong response
    Pong,
    /// Error response
    Error { code: String, message: String },
}

/// Job information for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: JobId,
    pub file_id: i64,
    pub plugin_name: String,
    pub status: ProcessingStatus,
    pub priority: i32,
    pub retry_count: i32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub error_message: Option<String>,
    pub parser_version: Option<String>,
    pub pipeline_run_id: Option<String>,
    pub quarantine_rows: i64,
}

/// Queue statistics for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatsInfo {
    pub queued: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub aborted: i64,
    pub total: i64,
}

// ============================================================================
// Scout types
// ============================================================================

/// Source summary for Control API (includes file_count).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutSourceInfo {
    pub id: SourceId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub source_type: SourceType,
    pub path: String,
    pub exec_path: Option<String>,
    pub enabled: bool,
    pub poll_interval_secs: u64,
    pub file_count: i64,
}

/// Tagging rule summary for Control API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutRuleInfo {
    pub id: TaggingRuleId,
    pub workspace_id: WorkspaceId,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
    pub enabled: bool,
}

/// Tag stats summary for Control API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutTagStats {
    pub total_files: i64,
    pub untagged_files: i64,
    pub tags: Vec<ScoutTagCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutTagCount {
    pub tag: String,
    pub count: i64,
}

/// Scan lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Scan progress snapshot for Control API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutScanProgress {
    pub dirs_scanned: u64,
    pub files_found: u64,
    pub files_persisted: u64,
    pub current_dir: Option<String>,
    pub elapsed_ms: u64,
    pub files_per_sec: f64,
    pub stalled: bool,
}

/// Scan status for Control API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutScanStatus {
    pub scan_id: String,
    pub workspace_id: WorkspaceId,
    pub source_path: String,
    pub source_id: Option<SourceId>,
    pub state: ScanState,
    pub progress: Option<ScoutScanProgress>,
    pub files_persisted: Option<u64>,
    pub error: Option<String>,
}

impl ControlResponse {
    /// Create an error response
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = ControlRequest::ListJobs {
            status: Some(ProcessingStatus::Queued),
            limit: Some(10),
            offset: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ListJobs"));
        assert!(json.contains("QUEUED"));

        let parsed: ControlRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            ControlRequest::ListJobs { status, limit, .. } => {
                assert_eq!(status, Some(ProcessingStatus::Queued));
                assert_eq!(limit, Some(10));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let resp = ControlResponse::Jobs(vec![JobInfo {
            id: JobId::from(123),
            file_id: 1,
            plugin_name: "test".to_string(),
            status: ProcessingStatus::Running,
            priority: 0,
            retry_count: 0,
            created_at: None,
            updated_at: None,
            error_message: None,
            parser_version: Some("1.0.0".to_string()),
            pipeline_run_id: None,
            quarantine_rows: 0,
        }]);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Jobs"));
        assert!(json.contains("test"));

        let parsed: ControlResponse = serde_json::from_str(&json).unwrap();
        match parsed {
            ControlResponse::Jobs(jobs) => {
                assert_eq!(jobs.len(), 1);
                assert_eq!(jobs[0].plugin_name, "test");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_cancel_job_request() {
        let req = ControlRequest::CancelJob {
            job_id: JobId::from(42),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ControlRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            ControlRequest::CancelJob { job_id } => {
                assert_eq!(job_id, JobId::from(42));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_error_response() {
        let resp = ControlResponse::error("NOT_FOUND", "Job not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("NOT_FOUND"));
    }
}

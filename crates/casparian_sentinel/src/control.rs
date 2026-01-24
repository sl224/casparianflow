//! Control Plane API for Casparian Sentinel
//!
//! Provides a ZMQ-based API for UI/CLI to query and mutate sentinel state
//! without direct database access. This enables concurrent access while
//! sentinel is running.
//!
//! # Protocol
//!
//! Uses ZMQ REP socket with JSON request/response protocol:
//! - Request: JSON-encoded `ControlRequest`
//! - Response: JSON-encoded `ControlResponse`
//!
//! # Supported Operations
//!
//! - `ListJobs` - List jobs with optional status filter
//! - `GetJob` - Get a single job by ID
//! - `CancelJob` - Request cancellation of a job
//! - `GetQueueStats` - Get job counts by status

use casparian_protocol::{JobId, ProcessingStatus};
use serde::{Deserialize, Serialize};

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

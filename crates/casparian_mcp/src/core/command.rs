//! Command types for Core message passing
//!
//! Commands are sent from tool handlers to the Core thread.
//! Each command includes a Responder channel for returning results.

use crate::approvals::{ApprovalId, ApprovalOperation, ApprovalRequest};
use crate::jobs::{Job, JobId, JobProgress, JobSpec};
use crate::types::ApprovalSummary;
use anyhow::Result;
use std::sync::mpsc::Sender;

/// One-shot channel for returning results from Core
pub type Responder<T> = Sender<T>;

/// Commands sent to the Core thread
#[derive(Debug)]
pub enum Command {
    // ========================================================================
    // Job Lifecycle Commands
    // ========================================================================
    /// Create a new job
    CreateJob {
        spec: JobSpec,
        approval_id: Option<String>,
        respond: Responder<Result<Job>>,
    },

    /// Get a job by ID
    GetJob {
        id: JobId,
        respond: Responder<Result<Option<Job>>>,
    },

    /// Start a job (transition from queued to running)
    StartJob {
        id: JobId,
        respond: Responder<Result<()>>,
    },

    /// Update job progress
    UpdateProgress {
        id: JobId,
        progress: JobProgress,
        respond: Responder<Result<()>>,
    },

    /// Complete a job successfully
    CompleteJob {
        id: JobId,
        result: serde_json::Value,
        respond: Responder<Result<()>>,
    },

    /// Fail a job with an error
    FailJob {
        id: JobId,
        error: String,
        respond: Responder<Result<()>>,
    },

    /// Cancel a running or queued job
    CancelJob {
        id: JobId,
        respond: Responder<Result<bool>>,
    },

    /// List jobs with optional status filter
    ListJobs {
        /// Status filter as string: "queued", "running", "completed", "failed", "cancelled"
        status_filter: Option<String>,
        limit: usize,
        respond: Responder<Result<Vec<Job>>>,
    },

    // ========================================================================
    // Approval Lifecycle Commands
    // ========================================================================
    /// Create a new approval request
    CreateApproval {
        operation: ApprovalOperation,
        summary: ApprovalSummary,
        respond: Responder<Result<ApprovalRequest>>,
    },

    /// Get an approval by ID
    GetApproval {
        id: ApprovalId,
        respond: Responder<Result<Option<ApprovalRequest>>>,
    },

    /// Approve a pending request
    ApproveRequest {
        id: ApprovalId,
        respond: Responder<Result<bool>>,
    },

    /// Reject a pending request
    RejectRequest {
        id: ApprovalId,
        reason: Option<String>,
        respond: Responder<Result<bool>>,
    },

    /// Set job ID on an approval (after job creation)
    SetApprovalJobId {
        approval_id: ApprovalId,
        job_id: String,
        respond: Responder<Result<()>>,
    },

    /// List approvals with optional status filter
    ListApprovals {
        status_filter: Option<String>,
        respond: Responder<Result<Vec<ApprovalRequest>>>,
    },

    // ========================================================================
    // Control Commands
    // ========================================================================
    /// Request graceful shutdown
    Shutdown,
}

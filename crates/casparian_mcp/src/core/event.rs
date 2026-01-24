//! Event types emitted by Core
//!
//! Events are broadcast to interested subscribers when state changes occur.
//! This enables reactive patterns without polling.

use crate::approvals::ApprovalId;
use crate::jobs::{JobId, JobProgress};

/// Events emitted by the Core thread
#[derive(Debug, Clone)]
pub enum Event {
    // ========================================================================
    // Job Events
    // ========================================================================
    /// A new job was created
    JobCreated { job_id: JobId },

    /// A job started running
    JobStarted { job_id: JobId },

    /// A job reported progress
    JobProgress {
        job_id: JobId,
        progress: JobProgress,
    },

    /// A job completed successfully
    JobCompleted { job_id: JobId },

    /// A job failed with an error
    JobFailed { job_id: JobId, error: String },

    /// A job was cancelled
    JobCancelled { job_id: JobId },
    // ========================================================================
    // Approval Events (to be added in Commit 2)
    // ========================================================================
    // ApprovalCreated { approval_id: ApprovalId },
    // ApprovalApproved { approval_id: ApprovalId },
    // ApprovalRejected { approval_id: ApprovalId },
    // ApprovalExpired { approval_id: ApprovalId },
}

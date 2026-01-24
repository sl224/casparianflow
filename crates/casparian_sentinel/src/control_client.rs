//! Control API Client
//!
//! A simple synchronous client for communicating with the Sentinel Control API.

use crate::control::{ControlRequest, ControlResponse};
use crate::db::{IntentState, Session, SessionId};
use anyhow::{Context, Result};
use casparian_protocol::http_types::{Approval, ApprovalStatus};
use casparian_protocol::http_types::{
    Job as ApiJob, JobProgress as ApiJobProgress, JobResult as ApiJobResult, HttpJobStatus,
    HttpJobType,
};
use std::time::Duration;
use zmq::{Context as ZmqContext, Socket};

/// Default timeout for control API requests (5 seconds)
const DEFAULT_TIMEOUT_MS: i32 = 5000;

/// Client for the Sentinel Control API
pub struct ControlClient {
    socket: Socket,
    #[allow(dead_code)]
    context: ZmqContext, // Keep context alive
}

impl ControlClient {
    /// Connect to the control API at the given address
    pub fn connect(addr: &str) -> Result<Self> {
        let context = ZmqContext::new();
        let socket = context
            .socket(zmq::REQ)
            .context("Failed to create REQ socket")?;

        socket
            .set_rcvtimeo(DEFAULT_TIMEOUT_MS)
            .context("Failed to set receive timeout")?;
        socket
            .set_sndtimeo(DEFAULT_TIMEOUT_MS)
            .context("Failed to set send timeout")?;
        socket
            .set_linger(0)
            .context("Failed to set linger")?;

        socket
            .connect(addr)
            .with_context(|| format!("Failed to connect to control API at {}", addr))?;

        Ok(Self { socket, context })
    }

    /// Connect with custom timeout
    pub fn connect_with_timeout(addr: &str, timeout: Duration) -> Result<Self> {
        let context = ZmqContext::new();
        let socket = context
            .socket(zmq::REQ)
            .context("Failed to create REQ socket")?;

        let timeout_ms = timeout.as_millis() as i32;
        socket
            .set_rcvtimeo(timeout_ms)
            .context("Failed to set receive timeout")?;
        socket
            .set_sndtimeo(timeout_ms)
            .context("Failed to set send timeout")?;
        socket
            .set_linger(0)
            .context("Failed to set linger")?;

        socket
            .connect(addr)
            .with_context(|| format!("Failed to connect to control API at {}", addr))?;

        Ok(Self { socket, context })
    }

    /// Send a request and receive a response
    pub fn request(&self, req: ControlRequest) -> Result<ControlResponse> {
        let req_bytes = serde_json::to_vec(&req).context("Failed to serialize request")?;

        self.socket
            .send(&req_bytes, 0)
            .context("Failed to send request")?;

        let resp_bytes = self
            .socket
            .recv_bytes(0)
            .context("Failed to receive response (timeout or connection error)")?;

        let resp: ControlResponse =
            serde_json::from_slice(&resp_bytes).context("Failed to parse response")?;

        Ok(resp)
    }

    /// Ping the control API to check if it's alive
    pub fn ping(&self) -> Result<bool> {
        match self.request(ControlRequest::Ping)? {
            ControlResponse::Pong => Ok(true),
            ControlResponse::Error { message, .. } => {
                anyhow::bail!("Ping failed: {}", message)
            }
            _ => anyhow::bail!("Unexpected response to ping"),
        }
    }

    /// List jobs with optional status filter
    pub fn list_jobs(
        &self,
        status: Option<casparian_protocol::ProcessingStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<crate::control::JobInfo>> {
        match self.request(ControlRequest::ListJobs {
            status,
            limit,
            offset,
        })? {
            ControlResponse::Jobs(jobs) => Ok(jobs),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ListJobs failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ListJobs"),
        }
    }

    /// Get a single job by ID
    pub fn get_job(
        &self,
        job_id: casparian_protocol::JobId,
    ) -> Result<Option<crate::control::JobInfo>> {
        match self.request(ControlRequest::GetJob { job_id })? {
            ControlResponse::Job(job) => Ok(job),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("GetJob failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to GetJob"),
        }
    }

    /// Cancel a job
    pub fn cancel_job(&self, job_id: casparian_protocol::JobId) -> Result<(bool, String)> {
        match self.request(ControlRequest::CancelJob { job_id })? {
            ControlResponse::CancelResult { success, message } => Ok((success, message)),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CancelJob failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CancelJob"),
        }
    }

    /// Get queue statistics
    pub fn get_queue_stats(&self) -> Result<crate::control::QueueStatsInfo> {
        match self.request(ControlRequest::GetQueueStats)? {
            ControlResponse::QueueStats(stats) => Ok(stats),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("GetQueueStats failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to GetQueueStats"),
        }
    }

    // =====================================================================
    // API job operations (cf_api_jobs)
    // =====================================================================

    /// Create an API job
    pub fn create_api_job(
        &self,
        job_type: HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output: Option<&str>,
        approval_id: Option<&str>,
        spec_json: Option<&str>,
    ) -> Result<casparian_protocol::JobId> {
        match self.request(ControlRequest::CreateApiJob {
            job_type,
            plugin_name: plugin_name.to_string(),
            plugin_version: plugin_version.map(|s| s.to_string()),
            input_dir: input_dir.to_string(),
            output: output.map(|s| s.to_string()),
            approval_id: approval_id.map(|s| s.to_string()),
            spec_json: spec_json.map(|s| s.to_string()),
        })? {
            ControlResponse::ApiJobCreated { job_id } => Ok(job_id),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CreateApiJob failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CreateApiJob"),
        }
    }

    /// Get an API job by ID
    pub fn get_api_job(&self, job_id: casparian_protocol::JobId) -> Result<Option<ApiJob>> {
        match self.request(ControlRequest::GetApiJob { job_id })? {
            ControlResponse::ApiJob(job) => Ok(job),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("GetApiJob failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to GetApiJob"),
        }
    }

    /// List API jobs with optional status filter
    pub fn list_api_jobs(
        &self,
        status: Option<HttpJobStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ApiJob>> {
        match self.request(ControlRequest::ListApiJobs {
            status,
            limit,
            offset,
        })? {
            ControlResponse::ApiJobs(jobs) => Ok(jobs),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ListApiJobs failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ListApiJobs"),
        }
    }

    /// Update API job status
    pub fn update_api_job_status(
        &self,
        job_id: casparian_protocol::JobId,
        status: HttpJobStatus,
    ) -> Result<()> {
        match self.request(ControlRequest::UpdateApiJobStatus { job_id, status })? {
            ControlResponse::ApiJobResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("UpdateApiJobStatus failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("UpdateApiJobStatus failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to UpdateApiJobStatus"),
        }
    }

    /// Update API job progress
    pub fn update_api_job_progress(
        &self,
        job_id: casparian_protocol::JobId,
        progress: ApiJobProgress,
    ) -> Result<()> {
        match self.request(ControlRequest::UpdateApiJobProgress { job_id, progress })? {
            ControlResponse::ApiJobResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("UpdateApiJobProgress failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("UpdateApiJobProgress failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to UpdateApiJobProgress"),
        }
    }

    /// Update API job result
    pub fn update_api_job_result(
        &self,
        job_id: casparian_protocol::JobId,
        result: ApiJobResult,
    ) -> Result<()> {
        match self.request(ControlRequest::UpdateApiJobResult { job_id, result })? {
            ControlResponse::ApiJobResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("UpdateApiJobResult failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("UpdateApiJobResult failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to UpdateApiJobResult"),
        }
    }

    /// Update API job error
    pub fn update_api_job_error(
        &self,
        job_id: casparian_protocol::JobId,
        error: &str,
    ) -> Result<()> {
        match self.request(ControlRequest::UpdateApiJobError {
            job_id,
            error: error.to_string(),
        })? {
            ControlResponse::ApiJobResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("UpdateApiJobError failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("UpdateApiJobError failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to UpdateApiJobError"),
        }
    }

    /// Cancel an API job
    pub fn cancel_api_job(&self, job_id: casparian_protocol::JobId) -> Result<bool> {
        match self.request(ControlRequest::CancelApiJob { job_id })? {
            ControlResponse::ApiJobResult { success, .. } => Ok(success),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CancelApiJob failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CancelApiJob"),
        }
    }

    /// List approvals with optional status filter
    pub fn list_approvals(
        &self,
        status: Option<ApprovalStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Approval>> {
        match self.request(ControlRequest::ListApprovals {
            status,
            limit,
            offset,
        })? {
            ControlResponse::Approvals(approvals) => Ok(approvals),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ListApprovals failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ListApprovals"),
        }
    }

    /// Create a new approval request
    pub fn create_approval(
        &self,
        approval_id: &str,
        operation: casparian_protocol::http_types::ApprovalOperation,
        summary: &str,
        expires_in_seconds: i64,
    ) -> Result<()> {
        match self.request(ControlRequest::CreateApproval {
            approval_id: approval_id.to_string(),
            operation,
            summary: summary.to_string(),
            expires_in_seconds,
        })? {
            ControlResponse::ApprovalResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("CreateApproval failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CreateApproval failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CreateApproval"),
        }
    }

    /// Get a single approval by ID
    pub fn get_approval(&self, approval_id: &str) -> Result<Option<Approval>> {
        match self.request(ControlRequest::GetApproval {
            approval_id: approval_id.to_string(),
        })? {
            ControlResponse::Approval(approval) => Ok(approval),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("GetApproval failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to GetApproval"),
        }
    }

    /// Approve an approval request
    pub fn approve(&self, approval_id: &str) -> Result<(bool, String)> {
        match self.request(ControlRequest::Approve {
            approval_id: approval_id.to_string(),
        })? {
            ControlResponse::ApprovalResult { success, message } => Ok((success, message)),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("Approve failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to Approve"),
        }
    }

    /// Reject an approval request
    pub fn reject(&self, approval_id: &str, reason: &str) -> Result<(bool, String)> {
        match self.request(ControlRequest::Reject {
            approval_id: approval_id.to_string(),
            reason: reason.to_string(),
        })? {
            ControlResponse::ApprovalResult { success, message } => Ok((success, message)),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("Reject failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to Reject"),
        }
    }

    /// Link a job ID to an approval
    pub fn set_approval_job_id(
        &self,
        approval_id: &str,
        job_id: casparian_protocol::JobId,
    ) -> Result<()> {
        match self.request(ControlRequest::SetApprovalJobId {
            approval_id: approval_id.to_string(),
            job_id,
        })? {
            ControlResponse::ApprovalResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("SetApprovalJobId failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("SetApprovalJobId failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to SetApprovalJobId"),
        }
    }

    /// Expire pending approvals past their expiry
    pub fn expire_approvals(&self) -> Result<()> {
        match self.request(ControlRequest::ExpireApprovals)? {
            ControlResponse::ApprovalResult { success, message } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("ExpireApprovals failed: {}", message)
                }
            }
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ExpireApprovals failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ExpireApprovals"),
        }
    }

    /// Create a new session
    pub fn create_session(&self, intent_text: &str, input_dir: Option<&str>) -> Result<SessionId> {
        match self.request(ControlRequest::CreateSession {
            intent_text: intent_text.to_string(),
            input_dir: input_dir.map(|s| s.to_string()),
        })? {
            ControlResponse::SessionCreated { session_id } => Ok(session_id),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CreateSession failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CreateSession"),
        }
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: SessionId) -> Result<Option<Session>> {
        match self.request(ControlRequest::GetSession { session_id })? {
            ControlResponse::Session(session) => Ok(session),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("GetSession failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to GetSession"),
        }
    }

    /// List sessions with optional state filter
    pub fn list_sessions(
        &self,
        state: Option<IntentState>,
        limit: Option<i64>,
    ) -> Result<Vec<Session>> {
        match self.request(ControlRequest::ListSessions { state, limit })? {
            ControlResponse::Sessions(sessions) => Ok(sessions),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ListSessions failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ListSessions"),
        }
    }

    /// List sessions that need human input (at gates)
    pub fn list_sessions_needing_input(&self, limit: Option<i64>) -> Result<Vec<Session>> {
        match self.request(ControlRequest::ListSessionsNeedingInput { limit })? {
            ControlResponse::Sessions(sessions) => Ok(sessions),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("ListSessionsNeedingInput failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to ListSessionsNeedingInput"),
        }
    }

    /// Advance a session to a new state
    pub fn advance_session(
        &self,
        session_id: SessionId,
        target_state: IntentState,
    ) -> Result<(bool, String)> {
        match self.request(ControlRequest::AdvanceSession {
            session_id,
            target_state,
        })? {
            ControlResponse::SessionResult { success, message } => Ok((success, message)),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("AdvanceSession failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to AdvanceSession"),
        }
    }

    /// Cancel a session
    pub fn cancel_session(&self, session_id: SessionId) -> Result<(bool, String)> {
        match self.request(ControlRequest::CancelSession { session_id })? {
            ControlResponse::SessionResult { success, message } => Ok((success, message)),
            ControlResponse::Error { code, message } => {
                anyhow::bail!("CancelSession failed [{}]: {}", code, message)
            }
            _ => anyhow::bail!("Unexpected response to CancelSession"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running sentinel with control API enabled
    // They are marked as ignore by default

    #[test]
    #[ignore]
    fn test_control_client_ping() {
        let client = ControlClient::connect(crate::DEFAULT_CONTROL_ADDR).unwrap();
        assert!(client.ping().unwrap());
    }

    #[test]
    #[ignore]
    fn test_control_client_list_jobs() {
        let client = ControlClient::connect(crate::DEFAULT_CONTROL_ADDR).unwrap();
        let jobs = client.list_jobs(None, Some(10), None).unwrap();
        println!("Found {} jobs", jobs.len());
    }
}

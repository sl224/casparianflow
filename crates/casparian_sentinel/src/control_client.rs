//! Control API Client
//!
//! A simple synchronous client for communicating with the Sentinel Control API.

use crate::control::{ControlRequest, ControlResponse};
use anyhow::{Context, Result};
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running sentinel with control API enabled
    // They are marked as ignore by default

    #[test]
    #[ignore]
    fn test_control_client_ping() {
        let client = ControlClient::connect("tcp://127.0.0.1:5556").unwrap();
        assert!(client.ping().unwrap());
    }

    #[test]
    #[ignore]
    fn test_control_client_list_jobs() {
        let client = ControlClient::connect("tcp://127.0.0.1:5556").unwrap();
        let jobs = client.list_jobs(None, Some(10), None).unwrap();
        println!("Found {} jobs", jobs.len());
    }
}

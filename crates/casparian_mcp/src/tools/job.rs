//! Job Tools - Job Status, Cancel, List
//!
//! Tools for monitoring and managing jobs.

use super::McpTool;
use crate::approvals::ApprovalManager;
use crate::jobs::{JobId, JobManager};
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// casparian_job_status
// ============================================================================

pub struct JobStatusTool;

#[derive(Debug, Deserialize)]
struct JobStatusArgs {
    job_id: String,
}

#[derive(Debug, Serialize)]
struct JobProgressInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    items_done: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    items_total: Option<u64>,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    eta_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct JobStatusResult {
    job_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<JobProgressInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[async_trait::async_trait]
impl McpTool for JobStatusTool {
    fn name(&self) -> &'static str {
        "casparian_job_status"
    }

    fn description(&self) -> &'static str {
        "Get job progress or result"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "job_id": {
                    "type": "string"
                }
            },
            "required": ["job_id"]
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: JobStatusArgs = serde_json::from_value(args)?;
        let job_id = JobId::from_string(&args.job_id);

        let job_manager = jobs.lock().await;
        let job = job_manager
            .get_job(&job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", args.job_id))?;

        let (progress, result, error) = match &job.state {
            crate::jobs::JobState::Running { progress, .. } => (
                Some(JobProgressInfo {
                    phase: progress.phase.clone(),
                    items_done: progress.items_done,
                    items_total: progress.items_total,
                    elapsed_ms: progress.elapsed_ms,
                    eta_ms: progress.eta_ms,
                }),
                None,
                None,
            ),
            crate::jobs::JobState::Stalled { progress, .. } => (
                Some(JobProgressInfo {
                    phase: progress.phase.clone(),
                    items_done: progress.items_done,
                    items_total: progress.items_total,
                    elapsed_ms: progress.elapsed_ms,
                    eta_ms: progress.eta_ms,
                }),
                None,
                None,
            ),
            crate::jobs::JobState::Completed { result, .. } => (None, Some(result.clone()), None),
            crate::jobs::JobState::Failed { error, .. } => (None, None, Some(error.clone())),
            _ => (None, None, None),
        };

        let result = JobStatusResult {
            job_id: args.job_id,
            status: job.state.status_str().to_string(),
            progress,
            result,
            error,
        };

        Ok(serde_json::to_value(result)?)
    }
}

// ============================================================================
// casparian_job_cancel
// ============================================================================

pub struct JobCancelTool;

#[derive(Debug, Deserialize)]
struct JobCancelArgs {
    job_id: String,
}

#[derive(Debug, Serialize)]
struct JobCancelResult {
    job_id: String,
    status: String,
}

#[async_trait::async_trait]
impl McpTool for JobCancelTool {
    fn name(&self) -> &'static str {
        "casparian_job_cancel"
    }

    fn description(&self) -> &'static str {
        "Cancel a running or queued job"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "job_id": {
                    "type": "string"
                }
            },
            "required": ["job_id"]
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: JobCancelArgs = serde_json::from_value(args)?;
        let job_id = JobId::from_string(&args.job_id);

        let mut job_manager = jobs.lock().await;

        // Check if job exists first
        let exists = job_manager.get_job(&job_id).is_some();
        if !exists {
            return Ok(serde_json::to_value(JobCancelResult {
                job_id: args.job_id,
                status: "not_found".to_string(),
            })?);
        }

        // Check if already completed
        let is_terminal = job_manager
            .get_job(&job_id)
            .map(|j| j.state.is_terminal())
            .unwrap_or(false);

        if is_terminal {
            return Ok(serde_json::to_value(JobCancelResult {
                job_id: args.job_id,
                status: "already_completed".to_string(),
            })?);
        }

        // Cancel
        job_manager.cancel_job(&job_id)?;

        let result = JobCancelResult {
            job_id: args.job_id,
            status: "cancelled".to_string(),
        };

        Ok(serde_json::to_value(result)?)
    }
}

// ============================================================================
// casparian_job_list
// ============================================================================

pub struct JobListTool;

#[derive(Debug, Deserialize)]
struct JobListArgs {
    #[serde(default = "default_status")]
    status: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_status() -> String {
    "all".to_string()
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Serialize)]
struct JobListEntry {
    job_id: String,
    #[serde(rename = "type")]
    job_type: String,
    status: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_ref: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JobListResult {
    jobs: Vec<JobListEntry>,
}

#[async_trait::async_trait]
impl McpTool for JobListTool {
    fn name(&self) -> &'static str {
        "casparian_job_list"
    }

    fn description(&self) -> &'static str {
        "List recent jobs"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["all", "running", "completed", "failed"],
                    "default": "all"
                },
                "limit": {
                    "type": "integer",
                    "default": 20
                }
            }
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: JobListArgs = serde_json::from_value(args).unwrap_or(JobListArgs {
            status: default_status(),
            limit: default_limit(),
        });

        let job_manager = jobs.lock().await;

        let status_filter = if args.status == "all" {
            None
        } else {
            Some(args.status.as_str())
        };

        let jobs = job_manager.list_jobs(status_filter, args.limit);

        let entries: Vec<JobListEntry> = jobs
            .into_iter()
            .map(|j| JobListEntry {
                job_id: j.id.to_string(),
                job_type: j.job_type.to_string(),
                status: j.state.status_str().to_string(),
                created_at: j.created_at.to_rfc3339(),
                plugin_ref: j.plugin_ref.as_ref().map(|pr| serde_json::to_value(pr).unwrap()),
            })
            .collect();

        let result = JobListResult { jobs: entries };

        Ok(serde_json::to_value(result)?)
    }
}

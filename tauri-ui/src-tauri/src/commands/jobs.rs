//! Job management commands.
//!
//! These commands manage background jobs (backtest, run, etc.).
//!
//! Tape instrumentation (WS7-05):
//! - Records job operations with job_id for correlation
//! - Input directories are hashed for privacy

use crate::state::{AppState, CommandError, CommandResult};
use casparian_protocol::{JobId, ProcessingStatus};
use casparian_sentinel::JobQueue;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Job item for list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobItem {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub plugin_name: String,
    pub plugin_version: Option<String>,
    pub input_dir: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    pub progress: Option<JobProgress>,
}

/// Job progress info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub phase: String,
    pub items_done: u64,
    pub items_total: Option<u64>,
    pub message: Option<String>,
}

/// Job cancel response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCancelResponse {
    pub success: bool,
    pub status: String,
}

/// List all jobs.
#[tauri::command]
pub async fn job_list(
    status: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<JobItem>> {
    let status_filter = status.as_deref().and_then(|s| parse_processing_status(s));
    let limit = limit.unwrap_or(100);

    let jobs = if let Some(client) = state.try_control_client() {
        client
            .list_jobs(status_filter, Some(limit as i64), Some(0))
            .map_err(|e| CommandError::Internal(format!("Control API error: {}", e)))?
            .into_iter()
            .map(|job| JobItem {
                id: job.id.as_u64().to_string(),
                job_type: "run".to_string(),
                status: status_to_string(job.status),
                plugin_name: job.plugin_name,
                plugin_version: job.parser_version,
                input_dir: "-".to_string(),
                created_at: job.created_at.unwrap_or_else(|| "-".to_string()),
                started_at: None,
                finished_at: None,
                error_message: job.error_message,
                progress: None,
            })
            .collect()
    } else {
        let conn = state
            .open_rw_connection()
            .map_err(|e| CommandError::Database(e.to_string()))?;
        let queue = JobQueue::new(conn);
        queue
            .init_queue_schema()
            .map_err(|e| CommandError::Database(e.to_string()))?;
        let jobs = queue
            .list_jobs(status_filter, limit, 0)
            .map_err(|e| CommandError::Database(e.to_string()))?;
        jobs.into_iter()
            .map(|job| JobItem {
                id: job.id.as_u64().to_string(),
                job_type: "run".to_string(),
                status: status_to_string(job.status),
                plugin_name: job.plugin_name,
                plugin_version: job.parser_version,
                input_dir: "-".to_string(),
                created_at: job
                    .created_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string()),
                started_at: None,
                finished_at: None,
                error_message: job.error_message,
                progress: None,
            })
            .collect()
    };

    Ok(jobs)
}

/// Get job status by ID.
#[tauri::command]
pub async fn job_status(job_id: String, state: State<'_, AppState>) -> CommandResult<JobItem> {
    let id: u64 = job_id
        .parse()
        .map_err(|_| CommandError::InvalidArgument("Invalid job ID".to_string()))?;

    if let Some(client) = state.try_control_client() {
        let job = client
            .get_job(JobId::new(id))
            .map_err(|e| CommandError::Internal(format!("Control API error: {}", e)))?
            .ok_or_else(|| CommandError::NotFound(format!("Job {} not found", job_id)))?;

        return Ok(JobItem {
            id: job_id,
            job_type: "run".to_string(),
            status: status_to_string(job.status),
            plugin_name: job.plugin_name,
            plugin_version: job.parser_version,
            input_dir: "-".to_string(),
            created_at: job.created_at.unwrap_or_else(|| "-".to_string()),
            started_at: None,
            finished_at: None,
            error_message: job.error_message,
            progress: None,
        });
    }

    let conn = state
        .open_rw_connection()
        .map_err(|e| CommandError::Database(e.to_string()))?;
    let queue = JobQueue::new(conn);
    queue
        .init_queue_schema()
        .map_err(|e| CommandError::Database(e.to_string()))?;
    let job = queue
        .get_job(JobId::new(id))
        .map_err(|e| CommandError::Database(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Job {} not found", job_id)))?;

    Ok(JobItem {
        id: job_id,
        job_type: "run".to_string(),
        status: status_to_string(job.status),
        plugin_name: job.plugin_name,
        plugin_version: job.parser_version,
        input_dir: "-".to_string(),
        created_at: job
            .created_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "-".to_string()),
        started_at: None,
        finished_at: None,
        error_message: job.error_message,
        progress: None,
    })
}

/// Cancel a running job.
///
/// WS4-04: Uses Control API when sentinel is running, falls back to direct DB.
#[tauri::command]
pub async fn job_cancel(
    job_id: String,
    state: State<'_, AppState>,
) -> CommandResult<JobCancelResponse> {
    // Record tape event
    let tape_ids = {
        let tape = state.tape().read().ok();
        tape.as_ref()
            .and_then(|t| t.emit_command("JobCancel", serde_json::json!({ "job_id": job_id })))
    };

    let id: u64 = job_id
        .parse()
        .map_err(|_| CommandError::InvalidArgument("Invalid job ID".to_string()))?;

    // Try Control API first (enables real cancellation of running jobs)
    let (cancelled, message) = if let Some(client) = state.try_control_client() {
        tracing::debug!("Cancelling job {} via Control API", job_id);
        client
            .cancel_job(JobId::new(id))
            .map_err(|e| {
                // Record error
                if let Some((event_id, correlation_id)) = &tape_ids {
                    if let Ok(tape) = state.tape().read() {
                        tape.emit_error(
                            correlation_id,
                            event_id,
                            &e.to_string(),
                            serde_json::json!({"status": "failed", "job_id": job_id, "via": "control_api"}),
                        );
                    }
                }
                CommandError::Internal(format!("Control API error: {}", e))
            })?
    } else {
        // Fall back to direct DB (limited - can only update status, not stop worker)
        tracing::debug!(
            "Cancelling job {} via direct DB (sentinel not available)",
            job_id
        );
        let conn = state
            .open_rw_connection()
            .map_err(|e| CommandError::Database(e.to_string()))?;
        let queue = JobQueue::new(conn);
        queue
            .init_queue_schema()
            .map_err(|e| CommandError::Database(e.to_string()))?;
        let cancelled = queue
            .cancel_job(JobId::new(id))
            .map_err(|e| {
                if let Some((event_id, correlation_id)) = &tape_ids {
                    if let Ok(tape) = state.tape().read() {
                        tape.emit_error(
                            correlation_id,
                            event_id,
                            &e.to_string(),
                            serde_json::json!({"status": "failed", "job_id": job_id, "via": "direct_db"}),
                        );
                    }
                }
                CommandError::Database(e.to_string())
            })?;

        let msg = if cancelled {
            "Cancelled (DB only - worker may still be running)"
        } else {
            "Job not in cancellable state"
        };
        (cancelled, msg.to_string())
    };

    let status = if cancelled { "cancelled" } else { "unchanged" };

    // Record success
    if let Some((event_id, correlation_id)) = tape_ids {
        if let Ok(tape) = state.tape().read() {
            tape.emit_success(
                &correlation_id,
                &event_id,
                serde_json::json!({
                    "status": "success",
                    "job_id": job_id,
                    "cancelled": cancelled,
                    "message": message,
                }),
            );
        }
    }

    Ok(JobCancelResponse {
        success: cancelled,
        status: status.to_string(),
    })
}

fn parse_processing_status(raw: &str) -> Option<ProcessingStatus> {
    match raw.to_lowercase().as_str() {
        "queued" => Some(ProcessingStatus::Queued),
        "pending" => Some(ProcessingStatus::Pending),
        "running" => Some(ProcessingStatus::Running),
        "staged" => Some(ProcessingStatus::Staged),
        "completed" => Some(ProcessingStatus::Completed),
        "failed" => Some(ProcessingStatus::Failed),
        "aborted" | "cancelled" | "canceled" => Some(ProcessingStatus::Aborted),
        "skipped" => Some(ProcessingStatus::Skipped),
        _ => None,
    }
}

fn status_to_string(status: ProcessingStatus) -> String {
    match status {
        ProcessingStatus::Pending => "pending",
        ProcessingStatus::Queued => "queued",
        ProcessingStatus::Running => "running",
        ProcessingStatus::Staged => "staged",
        ProcessingStatus::Completed => "completed",
        ProcessingStatus::Failed => "failed",
        ProcessingStatus::Aborted => "aborted",
        ProcessingStatus::Skipped => "skipped",
    }
    .to_string()
}

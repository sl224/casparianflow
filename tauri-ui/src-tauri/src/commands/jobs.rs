//! Job management commands.
//!
//! These commands manage background jobs (backtest, run, etc.).

use crate::state::{AppState, CommandError, CommandResult};
use casparian_protocol::HttpJobStatus;
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
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let status_filter = status.as_deref().and_then(|s| match s {
        "queued" => Some(HttpJobStatus::Queued),
        "running" => Some(HttpJobStatus::Running),
        "completed" => Some(HttpJobStatus::Completed),
        "failed" => Some(HttpJobStatus::Failed),
        "cancelled" => Some(HttpJobStatus::Cancelled),
        _ => None,
    });

    let jobs = storage
        .list_jobs(status_filter, limit.unwrap_or(100))
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let items: Vec<JobItem> = jobs
        .iter()
        .map(|job| {
            let status = match job.status {
                HttpJobStatus::Queued => "queued",
                HttpJobStatus::Running => "running",
                HttpJobStatus::Completed => "completed",
                HttpJobStatus::Failed => "failed",
                HttpJobStatus::Cancelled => "cancelled",
            };

            let job_type = match job.job_type {
                casparian_protocol::HttpJobType::Run => "run",
                casparian_protocol::HttpJobType::Backtest => "backtest",
                casparian_protocol::HttpJobType::Preview => "preview",
            };

            let progress = job.progress.as_ref().map(|p| JobProgress {
                phase: p.phase.clone(),
                items_done: p.items_done,
                items_total: p.items_total,
                message: p.message.clone(),
            });

            JobItem {
                id: job.job_id.as_u64().to_string(),
                job_type: job_type.to_string(),
                status: status.to_string(),
                plugin_name: job.plugin_name.clone(),
                plugin_version: job.plugin_version.clone(),
                input_dir: job.input_dir.clone(),
                created_at: job.created_at.clone(),
                started_at: job.started_at.clone(),
                finished_at: job.finished_at.clone(),
                error_message: job.error_message.clone(),
                progress,
            }
        })
        .collect();

    Ok(items)
}

/// Get job status by ID.
#[tauri::command]
pub async fn job_status(job_id: String, state: State<'_, AppState>) -> CommandResult<JobItem> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let id: u64 = job_id
        .parse()
        .map_err(|_| CommandError::InvalidArgument("Invalid job ID".to_string()))?;

    let job = storage
        .get_job(casparian_protocol::JobId::new(id))
        .map_err(|e| CommandError::Database(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Job {} not found", job_id)))?;

    let status = match job.status {
        HttpJobStatus::Queued => "queued",
        HttpJobStatus::Running => "running",
        HttpJobStatus::Completed => "completed",
        HttpJobStatus::Failed => "failed",
        HttpJobStatus::Cancelled => "cancelled",
    };

    let job_type = match job.job_type {
        casparian_protocol::HttpJobType::Run => "run",
        casparian_protocol::HttpJobType::Backtest => "backtest",
        casparian_protocol::HttpJobType::Preview => "preview",
    };

    let progress = job.progress.as_ref().map(|p| JobProgress {
        phase: p.phase.clone(),
        items_done: p.items_done,
        items_total: p.items_total,
        message: p.message.clone(),
    });

    Ok(JobItem {
        id: job_id,
        job_type: job_type.to_string(),
        status: status.to_string(),
        plugin_name: job.plugin_name.clone(),
        plugin_version: job.plugin_version.clone(),
        input_dir: job.input_dir.clone(),
        created_at: job.created_at.clone(),
        started_at: job.started_at.clone(),
        finished_at: job.finished_at.clone(),
        error_message: job.error_message.clone(),
        progress,
    })
}

/// Cancel a running job.
#[tauri::command]
pub async fn job_cancel(
    job_id: String,
    state: State<'_, AppState>,
) -> CommandResult<JobCancelResponse> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let id: u64 = job_id
        .parse()
        .map_err(|_| CommandError::InvalidArgument("Invalid job ID".to_string()))?;

    let cancelled = storage
        .cancel_job(casparian_protocol::JobId::new(id))
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let status = if cancelled { "cancelled" } else { "unchanged" };

    Ok(JobCancelResponse {
        success: cancelled,
        status: status.to_string(),
    })
}

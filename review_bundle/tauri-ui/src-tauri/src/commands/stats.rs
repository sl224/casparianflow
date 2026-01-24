//! Dashboard statistics commands.
//!
//! These commands provide aggregate statistics for the home dashboard.

use crate::state::{AppState, CommandError, CommandResult};
use casparian_protocol::HttpJobStatus;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Dashboard statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub ready_outputs: u64,
    pub running_jobs: u64,
    pub quarantined_rows: u64,
    pub failed_jobs: u64,
    pub recent_outputs: Vec<OutputInfo>,
    pub active_runs: Vec<ActiveRun>,
}

/// Output table information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputInfo {
    pub name: String,
    pub rows: String,
    pub updated: String,
}

/// Active run information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRun {
    pub name: String,
    pub progress: u32,
}

/// Get dashboard statistics.
#[tauri::command]
pub async fn dashboard_stats(state: State<'_, AppState>) -> CommandResult<DashboardStats> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // Count jobs by status
    let running = storage
        .list_jobs(Some(HttpJobStatus::Running), 1000)
        .map(|j| j.len() as u64)
        .unwrap_or(0);

    let failed = storage
        .list_jobs(Some(HttpJobStatus::Failed), 1000)
        .map(|j| j.len() as u64)
        .unwrap_or(0);

    let completed = storage
        .list_jobs(Some(HttpJobStatus::Completed), 100)
        .unwrap_or_default();

    // Count completed jobs as "ready outputs"
    let ready_outputs = completed.len() as u64;

    // Get recent completed outputs
    let recent_outputs: Vec<OutputInfo> = completed
        .iter()
        .take(5)
        .map(|job| {
            let rows = job
                .result
                .as_ref()
                .map(|r| format!("{} rows", r.rows_processed))
                .unwrap_or_else(|| "- rows".to_string());

            let updated = job
                .finished_at
                .as_ref()
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| {
                    let now = chrono::Utc::now();
                    let duration = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
                    if duration.num_hours() < 1 {
                        format!("Updated {} min ago", duration.num_minutes())
                    } else if duration.num_hours() < 24 {
                        format!("Updated {} hrs ago", duration.num_hours())
                    } else {
                        format!("Updated {} days ago", duration.num_days())
                    }
                })
                .unwrap_or_else(|| "Updated recently".to_string());

            OutputInfo {
                name: job.plugin_name.clone(),
                rows,
                updated,
            }
        })
        .collect();

    // Get active runs with progress
    let running_jobs = storage
        .list_jobs(Some(HttpJobStatus::Running), 10)
        .unwrap_or_default();

    let active_runs: Vec<ActiveRun> = running_jobs
        .iter()
        .map(|job| {
            let progress = job
                .progress
                .as_ref()
                .map(|p| {
                    if let Some(total) = p.items_total {
                        if total > 0 {
                            ((p.items_done as f64 / total as f64) * 100.0) as u32
                        } else {
                            0
                        }
                    } else {
                        50 // Unknown progress
                    }
                })
                .unwrap_or(0);

            ActiveRun {
                name: job.plugin_name.clone(),
                progress,
            }
        })
        .collect();

    // Quarantined rows would come from job results
    // For now, sum quarantined from completed jobs
    let quarantined_rows: u64 = completed
        .iter()
        .filter_map(|job| {
            job.result
                .as_ref()
                .and_then(|r| r.metrics.get("quarantined_rows").map(|&v| v as u64))
        })
        .sum();

    Ok(DashboardStats {
        ready_outputs,
        running_jobs: running,
        quarantined_rows,
        failed_jobs: failed,
        recent_outputs,
        active_runs,
    })
}

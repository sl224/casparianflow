//! Approval management commands.
//!
//! These commands manage the approval workflow for operations.

use crate::state::{AppState, CommandError, CommandResult};
use casparian_protocol::ApprovalStatus as ProtocolApprovalStatus;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Approval item for list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalItem {
    pub id: String,
    pub operation: String,
    pub plugin: String,
    pub files: String,
    pub expires: String,
    pub urgent: bool,
    pub status: String,
}

/// Approval statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalStats {
    pub pending: u64,
    pub approved: u64,
    pub rejected: u64,
    pub expired: u64,
}

/// Approval decision request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecision {
    pub approval_id: String,
    pub decision: String, // "approve" or "reject"
    pub reason: Option<String>,
}

/// Approval decision response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionResponse {
    pub success: bool,
    pub status: String,
}

/// List all approvals.
#[tauri::command]
pub async fn approval_list(
    status: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<ApprovalItem>> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // First expire any old approvals
    let _ = storage.expire_approvals();

    // Filter by status if provided
    let status_filter = status.as_deref().and_then(|s| match s {
        "pending" => Some(ProtocolApprovalStatus::Pending),
        "approved" => Some(ProtocolApprovalStatus::Approved),
        "rejected" => Some(ProtocolApprovalStatus::Rejected),
        "expired" => Some(ProtocolApprovalStatus::Expired),
        _ => None,
    });

    let approvals = storage
        .list_approvals(status_filter)
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let items: Vec<ApprovalItem> = approvals
        .iter()
        .map(|a| {
            let (operation, plugin, files) = match &a.operation {
                casparian_protocol::ApprovalOperation::Run {
                    plugin_name,
                    input_dir,
                    file_count,
                    ..
                } => (
                    format!("Run parser on {}", input_dir),
                    plugin_name.clone(),
                    file_count.to_string(),
                ),
                casparian_protocol::ApprovalOperation::SchemaPromote {
                    plugin_name,
                    output_name,
                    ..
                } => (
                    format!("Promote schema for {}", output_name),
                    plugin_name.clone(),
                    "-".to_string(),
                ),
            };

            // Calculate time until expiration
            let expires_at = chrono::DateTime::parse_from_rfc3339(&a.expires_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let now = chrono::Utc::now();
            let duration = expires_at.signed_duration_since(now);

            let expires = if duration.num_hours() > 0 {
                format!("in {} hours", duration.num_hours())
            } else if duration.num_minutes() > 0 {
                format!("in {} min", duration.num_minutes())
            } else {
                "expired".to_string()
            };

            let urgent = duration.num_minutes() < 60;

            let status = match a.status {
                ProtocolApprovalStatus::Pending => "pending",
                ProtocolApprovalStatus::Approved => "approved",
                ProtocolApprovalStatus::Rejected => "rejected",
                ProtocolApprovalStatus::Expired => "expired",
            };

            ApprovalItem {
                id: a.approval_id.clone(),
                operation,
                plugin,
                files,
                expires,
                urgent,
                status: status.to_string(),
            }
        })
        .collect();

    Ok(items)
}

/// Decide on an approval (approve or reject).
#[tauri::command]
pub async fn approval_decide(
    decision: ApprovalDecision,
    state: State<'_, AppState>,
) -> CommandResult<ApprovalDecisionResponse> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let success = match decision.decision.as_str() {
        "approve" => storage
            .approve(&decision.approval_id, None)
            .map_err(|e| CommandError::Database(e.to_string()))?,
        "reject" => storage
            .reject(&decision.approval_id, None, decision.reason.as_deref())
            .map_err(|e| CommandError::Database(e.to_string()))?,
        _ => {
            return Err(CommandError::InvalidArgument(
                "Decision must be 'approve' or 'reject'".to_string(),
            ))
        }
    };

    let status = if success {
        decision.decision
    } else {
        "unchanged".to_string()
    };

    Ok(ApprovalDecisionResponse { success, status })
}

/// Get approval statistics.
#[tauri::command]
pub async fn approval_stats(state: State<'_, AppState>) -> CommandResult<ApprovalStats> {
    let storage = state
        .open_api_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // First expire any old approvals
    let _ = storage.expire_approvals();

    // Count approvals by status
    let pending = storage
        .list_approvals(Some(ProtocolApprovalStatus::Pending))
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    let approved = storage
        .list_approvals(Some(ProtocolApprovalStatus::Approved))
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    let rejected = storage
        .list_approvals(Some(ProtocolApprovalStatus::Rejected))
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    let expired = storage
        .list_approvals(Some(ProtocolApprovalStatus::Expired))
        .map(|a| a.len() as u64)
        .unwrap_or(0);

    Ok(ApprovalStats {
        pending,
        approved,
        rejected,
        expired,
    })
}

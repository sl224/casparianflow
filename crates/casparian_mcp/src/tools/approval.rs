//! Approval Tools - Approval Status and List
//!
//! Tools for monitoring approval requests.

use super::McpTool;
use crate::approvals::{ApprovalId, ApprovalManager, ApprovalStatus};
use crate::jobs::JobManager;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// casparian_approval_status
// ============================================================================

pub struct ApprovalStatusTool;

#[derive(Debug, Deserialize)]
struct ApprovalStatusArgs {
    approval_id: String,
}

#[derive(Debug, Serialize)]
struct ApprovalSummaryInfo {
    description: String,
    file_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimated_rows: Option<u64>,
    target_path: String,
}

#[derive(Debug, Serialize)]
struct ApprovalStatusResult {
    approval_id: String,
    status: String,
    summary: ApprovalSummaryInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    expires_at: String,
}

#[async_trait::async_trait]
impl McpTool for ApprovalStatusTool {
    fn name(&self) -> &'static str {
        "casparian_approval_status"
    }

    fn description(&self) -> &'static str {
        "Check status of an approval request"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "approval_id": {
                    "type": "string"
                }
            },
            "required": ["approval_id"]
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _jobs: &Arc<Mutex<JobManager>>,
        approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: ApprovalStatusArgs = serde_json::from_value(args)?;
        let approval_id = ApprovalId::from_string(&args.approval_id);

        let approval_manager = approvals.lock().await;
        let approval = approval_manager
            .get_approval(&approval_id)
            .ok_or_else(|| anyhow::anyhow!("Approval not found: {}", args.approval_id))?;

        let result = ApprovalStatusResult {
            approval_id: args.approval_id,
            status: approval.status.status_str().to_string(),
            summary: ApprovalSummaryInfo {
                description: approval.summary.description.clone(),
                file_count: approval.summary.file_count,
                estimated_rows: approval.summary.estimated_rows,
                target_path: approval.summary.target_path.clone(),
            },
            job_id: approval.job_id.clone(),
            expires_at: approval.expires_at.to_rfc3339(),
        };

        Ok(serde_json::to_value(result)?)
    }
}

// ============================================================================
// casparian_approval_list
// ============================================================================

pub struct ApprovalListTool;

#[derive(Debug, Deserialize)]
struct ApprovalListArgs {
    #[serde(default = "default_status")]
    status: String,
}

fn default_status() -> String {
    "pending".to_string()
}

#[derive(Debug, Serialize)]
struct ApprovalListEntry {
    approval_id: String,
    operation: String,
    summary: ApprovalSummaryInfo,
    created_at: String,
    expires_at: String,
}

#[derive(Debug, Serialize)]
struct ApprovalListResult {
    approvals: Vec<ApprovalListEntry>,
}

#[async_trait::async_trait]
impl McpTool for ApprovalListTool {
    fn name(&self) -> &'static str {
        "casparian_approval_list"
    }

    fn description(&self) -> &'static str {
        "List pending approval requests"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["pending", "all"],
                    "default": "pending"
                }
            }
        })
    }

    async fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _jobs: &Arc<Mutex<JobManager>>,
        approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: ApprovalListArgs = serde_json::from_value(args).unwrap_or(ApprovalListArgs {
            status: default_status(),
        });

        let approval_manager = approvals.lock().await;

        let status_filter = if args.status == "all" {
            None
        } else {
            Some(args.status.as_str())
        };

        let approvals = approval_manager.list_approvals(status_filter);

        let entries: Vec<ApprovalListEntry> = approvals
            .into_iter()
            .map(|a| ApprovalListEntry {
                approval_id: a.approval_id.to_string(),
                operation: a.operation.description(),
                summary: ApprovalSummaryInfo {
                    description: a.summary.description.clone(),
                    file_count: a.summary.file_count,
                    estimated_rows: a.summary.estimated_rows,
                    target_path: a.summary.target_path.clone(),
                },
                created_at: a.created_at.to_rfc3339(),
                expires_at: a.expires_at.to_rfc3339(),
            })
            .collect();

        let result = ApprovalListResult { approvals: entries };

        Ok(serde_json::to_value(result)?)
    }
}

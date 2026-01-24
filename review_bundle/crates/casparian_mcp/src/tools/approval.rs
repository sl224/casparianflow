//! Approval Tools - Approval Status and List
//!
//! Tools for monitoring approval requests.

use super::McpTool;
use crate::approvals::{ApprovalId, ApprovalOperation, ApprovalStatus};
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{ApprovalDecision, ApprovalStatusFilter};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

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

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: ApprovalStatusArgs = serde_json::from_value(args)?;
        let approval_id = ApprovalId::from_string(&args.approval_id);

        let approval = core
            .get_approval(approval_id)?
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
    #[serde(default)]
    status: ApprovalStatusFilter,
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
                    "enum": ["pending", "all", "approved", "rejected", "expired"],
                    "default": "pending"
                }
            }
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        // Parse args with proper error handling (no unwrap_or_default)
        let args: ApprovalListArgs =
            serde_json::from_value(args).map_err(|e| anyhow!("Invalid arguments: {}", e))?;

        let status_filter = args.status.as_filter_str();

        let approvals = core.list_approvals(status_filter)?;

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

// ============================================================================
// casparian_approval_decide
// ============================================================================

pub struct ApprovalDecideTool;

#[derive(Debug, Deserialize)]
struct ApprovalDecideArgs {
    approval_id: String,
    /// Strongly-typed decision enum - no more stringly-typed "approve"/"reject"
    decision: ApprovalDecision,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApprovalDecideResult {
    approval_id: String,
    decision: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl McpTool for ApprovalDecideTool {
    fn name(&self) -> &'static str {
        "casparian_approval_decide"
    }

    fn description(&self) -> &'static str {
        "Approve or reject a pending approval request"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "approval_id": {
                    "type": "string",
                    "description": "The approval ID to decide on"
                },
                "decision": {
                    "type": "string",
                    "enum": ["approve", "reject"],
                    "description": "Whether to approve or reject the request"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional reason (required for rejection)"
                }
            },
            "required": ["approval_id", "decision"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        core: &CoreHandle,
        _config: &McpServerConfig,
        executor: &JobExecutorHandle,
    ) -> Result<Value> {
        // Parse args - serde will validate the decision enum automatically
        let args: ApprovalDecideArgs = serde_json::from_value(args).map_err(|e| {
            anyhow!(
                "Invalid arguments: {}. Decision must be 'approve' or 'reject'.",
                e
            )
        })?;

        let approval_id = ApprovalId::from_string(&args.approval_id);

        // Get approval via Core
        let approval = match core.get_approval(approval_id.clone())? {
            Some(a) => a,
            None => {
                return Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id.clone(),
                    decision: args.decision.to_string(),
                    status: "not_found".to_string(),
                    job_id: None,
                    error: Some("Approval not found".to_string()),
                })?);
            }
        };

        // Check if already decided
        match &approval.status {
            ApprovalStatus::Approved { .. } => {
                return Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id.clone(),
                    decision: args.decision.to_string(),
                    status: "already_approved".to_string(),
                    job_id: approval.job_id.clone(),
                    error: None,
                })?);
            }
            ApprovalStatus::Rejected { .. } => {
                return Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id.clone(),
                    decision: args.decision.to_string(),
                    status: "already_rejected".to_string(),
                    job_id: None,
                    error: None,
                })?);
            }
            ApprovalStatus::Expired => {
                return Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id.clone(),
                    decision: args.decision.to_string(),
                    status: "expired".to_string(),
                    job_id: None,
                    error: Some("Approval has expired".to_string()),
                })?);
            }
            ApprovalStatus::Pending => {
                // Continue with decision
            }
        }

        // Apply decision via Core
        match args.decision {
            ApprovalDecision::Reject => {
                core.reject(approval_id, args.reason.clone())?;
                Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id,
                    decision: args.decision.to_string(),
                    status: "rejected".to_string(),
                    job_id: None,
                    error: None,
                })?)
            }
            ApprovalDecision::Approve => {
                let operation = approval.operation.clone();
                core.approve(approval_id.clone())?;

                // Create job via Core
                let (job_id, job_id_for_enqueue): (Option<String>, Option<crate::jobs::JobId>) =
                    match &operation {
                        ApprovalOperation::Run {
                            plugin_ref,
                            input_dir,
                            output,
                        } => {
                            let job_spec = crate::jobs::JobSpec::Run {
                                plugin_ref: plugin_ref.clone(),
                                input_dir: input_dir.display().to_string(),
                                output_dir: Some(output.clone()),
                                schemas: None,
                            };
                            let approval_id_clone = args.approval_id.clone();
                            let job = core.create_job(job_spec, Some(approval_id_clone))?;
                            let job_id_str = job.id.to_string();
                            (Some(job_id_str), Some(job.id))
                        }
                        ApprovalOperation::SchemaPromote { .. } => {
                            info!(
                                "Schema promotion approved but not yet implemented: {}",
                                args.approval_id
                            );
                            (None, None)
                        }
                    };

                // Enqueue to executor
                if let Some(job_id_to_enqueue) = job_id_for_enqueue {
                    executor.enqueue(job_id_to_enqueue)?;
                    info!(
                        "Enqueued run job {:?} from approval {}",
                        job_id, args.approval_id
                    );
                }

                // Update approval with job_id via Core
                if let Some(ref jid) = job_id {
                    core.set_approval_job_id(approval_id, jid.clone())?;
                }

                Ok(serde_json::to_value(ApprovalDecideResult {
                    approval_id: args.approval_id,
                    decision: args.decision.to_string(),
                    status: "approved".to_string(),
                    job_id,
                    error: None,
                })?)
            }
        }
    }
}

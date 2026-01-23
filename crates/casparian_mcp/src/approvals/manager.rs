//! Approval Manager - Approval Lifecycle Management
//!
//! DB-backed approval manager using casparian_sentinel::ApiStorage.

use super::{ApprovalId, ApprovalOperation, ApprovalRequest, ApprovalStatus, APPROVAL_TTL_DAYS};
use crate::types::ApprovalSummary;
use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_protocol::{
    Approval as ProtocolApproval, ApprovalOperation as ProtocolApprovalOperation,
    ApprovalStatus as ProtocolApprovalStatus, JobId as ProtocolJobId,
};
use casparian_sentinel::ApiStorage;
use chrono::Utc;
use std::path::PathBuf;

/// Approval manager for tracking approval requests (DB-backed, no cache)
pub struct ApprovalManager {
    db_path: PathBuf,
}

impl ApprovalManager {
    /// Create a new approval manager backed by DuckDB at the given path.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = DbConnection::open_duckdb(&db_path)
            .with_context(|| format!("Failed to open DB at {}", db_path.display()))?;
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self { db_path })
    }

    fn storage(&self) -> Result<ApiStorage> {
        let conn = DbConnection::open_duckdb(&self.db_path)
            .with_context(|| format!("Failed to open DB at {}", self.db_path.display()))?;
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;
        Ok(storage)
    }

    /// Create a new approval request.
    pub fn create_approval(
        &self,
        operation: ApprovalOperation,
        summary: ApprovalSummary,
    ) -> Result<ApprovalRequest> {
        let approval = ApprovalRequest::new(operation, summary);
        let protocol_op = to_protocol_operation(&approval.operation);
        let expires_in = approval.expires_at.signed_duration_since(Utc::now());

        let storage = self.storage()?;
        storage.create_approval(
            approval.approval_id.as_ref(),
            &protocol_op,
            &approval.summary.description,
            expires_in,
        )?;

        Ok(approval)
    }

    /// Get an approval by ID.
    pub fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>> {
        let storage = self.storage()?;
        let approval = storage.get_approval(id.as_ref())?;
        approval.map(from_protocol_approval).transpose()
    }

    /// Approve an approval request.
    pub fn approve(&self, id: &ApprovalId) -> Result<bool> {
        let storage = self.storage()?;
        storage.approve(id.as_ref(), None)
    }

    /// Reject an approval request.
    pub fn reject(&self, id: &ApprovalId, reason: Option<String>) -> Result<bool> {
        let storage = self.storage()?;
        storage.reject(id.as_ref(), None, reason.as_deref())
    }

    /// Set the job ID after approval is processed.
    pub fn set_job_id(&self, approval_id: &ApprovalId, job_id: String) -> Result<()> {
        let parsed: ProtocolJobId = job_id
            .parse()
            .with_context(|| format!("Invalid job_id: {}", job_id))?;
        let storage = self.storage()?;
        storage.link_approval_to_job(approval_id.as_ref(), parsed)
    }

    /// List approvals with optional status filter.
    pub fn list_approvals(&self, status_filter: Option<&str>) -> Result<Vec<ApprovalRequest>> {
        let status = match status_filter {
            None => None,
            Some("pending") => Some(ProtocolApprovalStatus::Pending),
            Some("approved") => Some(ProtocolApprovalStatus::Approved),
            Some("rejected") => Some(ProtocolApprovalStatus::Rejected),
            Some("expired") => Some(ProtocolApprovalStatus::Expired),
            Some(other) => anyhow::bail!("Unknown approval status filter: {}", other),
        };

        let storage = self.storage()?;
        let approvals = storage.list_approvals(status)?;
        approvals.into_iter().map(from_protocol_approval).collect()
    }

    /// List pending approvals.
    pub fn list_pending(&self) -> Result<Vec<ApprovalRequest>> {
        self.list_approvals(Some("pending"))
    }

    /// Check and expire any pending approvals past their expiry time.
    pub fn check_expired(&self) -> Result<Vec<ApprovalId>> {
        let pending = self.list_approvals(Some("pending"))?;
        let expired: Vec<ApprovalId> = pending
            .iter()
            .filter(|a| a.is_expired())
            .map(|a| a.approval_id.clone())
            .collect();

        if !expired.is_empty() {
            let storage = self.storage()?;
            storage.expire_approvals()?;
        }

        Ok(expired)
    }

    /// Clean up old terminal approvals (not supported in DB; retained for audit).
    pub fn cleanup_old_approvals(&self) -> Result<usize> {
        let _ = APPROVAL_TTL_DAYS; // retained for compatibility
        Ok(0)
    }
}

// ============================================================================
// Conversion Helpers
// ============================================================================

fn to_protocol_operation(op: &ApprovalOperation) -> ProtocolApprovalOperation {
    match op {
        ApprovalOperation::Run {
            plugin_ref,
            input_dir,
            output,
        } => {
            let (plugin_name, plugin_version) = match plugin_ref {
                crate::types::PluginRef::Registered { plugin, version } => {
                    (plugin.clone(), version.clone())
                }
                crate::types::PluginRef::Path { path } => {
                    (path.to_string_lossy().to_string(), None)
                }
            };
            ProtocolApprovalOperation::Run {
                plugin_name,
                plugin_version,
                input_dir: input_dir.to_string_lossy().to_string(),
                file_count: 0,
                output: Some(output.clone()),
            }
        }
        ApprovalOperation::SchemaPromote {
            ephemeral_id,
            output_path,
        } => ProtocolApprovalOperation::SchemaPromote {
            plugin_name: ephemeral_id.clone(),
            output_name: output_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "default".to_string()),
            schema: casparian_protocol::SchemaSpec {
                columns: vec![],
                mode: casparian_protocol::SchemaMode::Strict,
            },
        },
    }
}

fn from_protocol_operation(op: &ProtocolApprovalOperation) -> Result<ApprovalOperation> {
    match op {
        ProtocolApprovalOperation::Run {
            plugin_name,
            plugin_version,
            input_dir,
            output,
            ..
        } => {
            let output = output
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Run approval missing output"))?;
            Ok(ApprovalOperation::Run {
                plugin_ref: crate::types::PluginRef::Registered {
                    plugin: plugin_name.clone(),
                    version: plugin_version.clone(),
                },
                input_dir: PathBuf::from(input_dir),
                output,
            })
        }
        ProtocolApprovalOperation::SchemaPromote {
            plugin_name,
            output_name,
            ..
        } => Ok(ApprovalOperation::SchemaPromote {
            ephemeral_id: plugin_name.clone(),
            output_path: PathBuf::from(output_name),
        }),
    }
}

fn from_protocol_approval(pa: ProtocolApproval) -> Result<ApprovalRequest> {
    let created_at = pa
        .created_at
        .parse()
        .context("Invalid created_at timestamp")?;
    let expires_at = pa
        .expires_at
        .parse()
        .context("Invalid expires_at timestamp")?;

    let status = match pa.status {
        ProtocolApprovalStatus::Pending => ApprovalStatus::Pending,
        ProtocolApprovalStatus::Approved => {
            let approved_at = pa
                .decided_at
                .as_ref()
                .context("Missing decided_at for approved request")?
                .parse()
                .context("Invalid decided_at timestamp")?;
            ApprovalStatus::Approved { approved_at }
        }
        ProtocolApprovalStatus::Rejected => {
            let rejected_at = pa
                .decided_at
                .as_ref()
                .context("Missing decided_at for rejected request")?
                .parse()
                .context("Invalid decided_at timestamp")?;
            ApprovalStatus::Rejected {
                rejected_at,
                reason: pa.rejection_reason.clone(),
            }
        }
        ProtocolApprovalStatus::Expired => ApprovalStatus::Expired,
    };

    let operation = from_protocol_operation(&pa.operation)?;

    Ok(ApprovalRequest {
        approval_id: ApprovalId::from_string(pa.approval_id),
        operation,
        summary: ApprovalSummary {
            description: pa.summary.clone(),
            file_count: 0,
            estimated_rows: None,
            target_path: String::new(),
        },
        created_at,
        expires_at,
        status,
        job_id: pa.job_id.map(|id| id.as_u64().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PluginRef;

    use tempfile::TempDir;

    fn create_test_manager() -> (ApprovalManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");
        let manager = ApprovalManager::new(db_path).unwrap();
        (manager, temp)
    }

    fn test_operation() -> ApprovalOperation {
        ApprovalOperation::Run {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: PathBuf::from("/data"),
            output: "parquet://./output/".to_string(),
        }
    }

    fn test_summary() -> ApprovalSummary {
        ApprovalSummary {
            description: "Test operation".to_string(),
            file_count: 10,
            estimated_rows: Some(1000),
            target_path: "./output/".to_string(),
        }
    }

    #[test]
    fn test_create_approval() {
        let (manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        assert!(matches!(approval.status, ApprovalStatus::Pending));
        assert!(!approval.is_expired());
    }

    #[test]
    fn test_approve() {
        let (manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        let id = approval.approval_id.clone();

        let approved = manager.approve(&id).unwrap();
        assert!(approved);

        let approval = manager.get_approval(&id).unwrap().unwrap();
        assert!(matches!(approval.status, ApprovalStatus::Approved { .. }));
    }

    #[test]
    fn test_reject() {
        let (manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        let id = approval.approval_id.clone();

        let rejected = manager
            .reject(&id, Some("Not approved".to_string()))
            .unwrap();
        assert!(rejected);

        let approval = manager.get_approval(&id).unwrap().unwrap();
        assert!(matches!(approval.status, ApprovalStatus::Rejected { .. }));
    }

    #[test]
    fn test_list_pending() {
        let (manager, _temp) = create_test_manager();

        manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        let pending = manager.list_pending().unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_approve_command() {
        let (manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        let cmd = approval.approve_command();
        assert!(cmd.contains("casparian mcp approve"));
        assert!(cmd.contains(&approval.approval_id.0));
    }
}

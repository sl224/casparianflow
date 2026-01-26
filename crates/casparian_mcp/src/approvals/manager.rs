//! Approval Manager - Approval Lifecycle Management
//!
//! Control API or DB-backed approval manager using casparian_sentinel.

use super::{ApprovalId, ApprovalOperation, ApprovalRequest, ApprovalStatus, APPROVAL_TTL_DAYS};
use crate::types::ApprovalSummary;
use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_protocol::{
    ApiJobId as ProtocolJobId, Approval as ProtocolApproval,
    ApprovalOperation as ProtocolApprovalOperation, ApprovalStatus as ProtocolApprovalStatus,
};
use casparian_sentinel::{ApiStorage, ControlClient, DEFAULT_CONTROL_ADDR};
use chrono::Utc;
use std::path::PathBuf;
use std::time::Duration;

enum ApprovalBackend {
    Db { db_path: PathBuf },
    Control { control_addr: String },
}

/// Approval manager for tracking approval requests (DB-backed, no cache)
pub struct ApprovalManager {
    backend: ApprovalBackend,
}

impl ApprovalManager {
    /// Create a new approval manager backed by the state store at the given path.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = DbConnection::open_sqlite(&db_path)
            .with_context(|| format!("Failed to open DB at {}", db_path.display()))?;
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self {
            backend: ApprovalBackend::Db { db_path },
        })
    }

    /// Create a new approval manager backed by the Control API.
    pub fn new_control(control_addr: Option<String>) -> Result<Self> {
        let addr = control_addr.unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());
        let client = ControlClient::connect_with_timeout(&addr, Duration::from_millis(500))
            .with_context(|| format!("Failed to connect to Control API at {}", addr))?;
        if !client.ping().unwrap_or(false) {
            anyhow::bail!("Control API did not respond at {}", addr);
        }
        Ok(Self {
            backend: ApprovalBackend::Control { control_addr: addr },
        })
    }

    fn storage(&self) -> Result<ApiStorage> {
        match &self.backend {
            ApprovalBackend::Db { db_path } => {
                let conn = DbConnection::open_sqlite(db_path)
                    .with_context(|| format!("Failed to open DB at {}", db_path.display()))?;
                let storage = ApiStorage::new(conn);
                storage.init_schema().context("Failed to init schema")?;
                Ok(storage)
            }
            ApprovalBackend::Control { .. } => {
                anyhow::bail!("ApprovalManager storage is not available in Control API mode");
            }
        }
    }

    fn control_client(&self) -> Result<ControlClient> {
        match &self.backend {
            ApprovalBackend::Control { control_addr } => {
                ControlClient::connect_with_timeout(control_addr, Duration::from_secs(5))
                    .with_context(|| {
                        format!("Failed to connect to Control API at {}", control_addr)
                    })
            }
            ApprovalBackend::Db { .. } => {
                anyhow::bail!("Control API client is not available in DB mode");
            }
        }
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
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                storage.create_approval(
                    approval.approval_id.as_ref(),
                    &protocol_op,
                    &approval.summary.description,
                    expires_in,
                )?;
                Ok(approval)
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                client.create_approval(
                    approval.approval_id.as_ref(),
                    protocol_op,
                    &approval.summary.description,
                    expires_in.num_seconds(),
                )?;
                Ok(approval)
            }
        }
    }

    /// Get an approval by ID.
    pub fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>> {
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                let approval = storage.get_approval(id.as_ref())?;
                approval.map(from_protocol_approval).transpose()
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                let approval = client.get_approval(id.as_ref())?;
                approval.map(from_protocol_approval).transpose()
            }
        }
    }

    /// Approve an approval request.
    pub fn approve(&self, id: &ApprovalId) -> Result<bool> {
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                storage.approve(id.as_ref(), None)
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                let (success, _message) = client.approve(id.as_ref())?;
                Ok(success)
            }
        }
    }

    /// Reject an approval request.
    pub fn reject(&self, id: &ApprovalId, reason: Option<String>) -> Result<bool> {
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                storage.reject(id.as_ref(), None, reason.as_deref())
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                let reason = reason.unwrap_or_else(|| "Rejected via MCP".to_string());
                let (success, _message) = client.reject(id.as_ref(), &reason)?;
                Ok(success)
            }
        }
    }

    /// Set the job ID after approval is processed.
    pub fn set_job_id(&self, approval_id: &ApprovalId, job_id: String) -> Result<()> {
        let parsed: ProtocolJobId = job_id
            .parse()
            .with_context(|| format!("Invalid job_id: {}", job_id))?;
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                storage.link_approval_to_job(approval_id.as_ref(), parsed)
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                client.set_approval_job_id(approval_id.as_ref(), parsed)
            }
        }
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
        match &self.backend {
            ApprovalBackend::Db { .. } => {
                let storage = self.storage()?;
                let approvals = storage.list_approvals(status)?;
                approvals.into_iter().map(from_protocol_approval).collect()
            }
            ApprovalBackend::Control { .. } => {
                let client = self.control_client()?;
                let approvals = client.list_approvals(status, Some(1000), Some(0))?;
                approvals.into_iter().map(from_protocol_approval).collect()
            }
        }
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
            match &self.backend {
                ApprovalBackend::Db { .. } => {
                    let storage = self.storage()?;
                    storage.expire_approvals()?;
                }
                ApprovalBackend::Control { .. } => {
                    let client = self.control_client()?;
                    client.expire_approvals()?;
                }
            }
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
        let db_path = temp.path().join("test.sqlite");
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

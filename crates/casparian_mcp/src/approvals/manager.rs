//! Approval Manager - Approval Lifecycle Management
//!
//! Manages approval request creation, status tracking, and cleanup.

use super::{
    ApprovalId, ApprovalOperation, ApprovalRequest, ApprovalStatus, ApprovalStore,
    APPROVAL_TTL_DAYS,
};
use crate::types::ApprovalSummary;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Approval manager for tracking approval requests
pub struct ApprovalManager {
    /// Approval store for persistence
    store: ApprovalStore,

    /// In-memory approval cache
    approvals: HashMap<ApprovalId, ApprovalRequest>,
}

impl ApprovalManager {
    /// Create a new approval manager
    pub fn new(approvals_dir: PathBuf) -> Result<Self> {
        let store = ApprovalStore::new(approvals_dir)?;

        // Load existing approvals from store
        let approvals = store.load_all()?;
        let mut approvals_map: HashMap<ApprovalId, ApprovalRequest> =
            approvals.into_iter().map(|a| (a.approval_id.clone(), a)).collect();

        // Auto-expire any pending approvals that have passed their expiry time
        let now = Utc::now();
        for approval in approvals_map.values_mut() {
            if matches!(approval.status, ApprovalStatus::Pending) && approval.expires_at < now {
                approval.mark_expired();
            }
        }

        Ok(Self {
            store,
            approvals: approvals_map,
        })
    }

    /// Create a new approval request
    pub fn create_approval(
        &mut self,
        operation: ApprovalOperation,
        summary: ApprovalSummary,
    ) -> Result<ApprovalRequest> {
        let approval = ApprovalRequest::new(operation, summary);
        self.store.save(&approval)?;
        self.approvals.insert(approval.approval_id.clone(), approval.clone());

        info!(
            "Created approval request: {} ({})",
            approval.approval_id,
            approval.operation.description()
        );

        Ok(approval)
    }

    /// Get an approval by ID
    pub fn get_approval(&self, id: &ApprovalId) -> Option<&ApprovalRequest> {
        self.approvals.get(id)
    }

    /// Get a mutable reference to an approval
    pub fn get_approval_mut(&mut self, id: &ApprovalId) -> Option<&mut ApprovalRequest> {
        self.approvals.get_mut(id)
    }

    /// Approve an approval request
    pub fn approve(&mut self, id: &ApprovalId) -> Result<bool> {
        let approval = match self.approvals.get_mut(id) {
            Some(a) => a,
            None => return Ok(false),
        };

        // Check if already expired
        if approval.is_expired() {
            approval.mark_expired();
            self.store.save(approval)?;
            return Ok(false);
        }

        if !matches!(approval.status, ApprovalStatus::Pending) {
            return Ok(false);
        }

        approval.approve();
        self.store.save(approval)?;

        info!("Approved request: {}", id);
        Ok(true)
    }

    /// Reject an approval request
    pub fn reject(&mut self, id: &ApprovalId, reason: Option<String>) -> Result<bool> {
        let approval = match self.approvals.get_mut(id) {
            Some(a) => a,
            None => return Ok(false),
        };

        if !matches!(approval.status, ApprovalStatus::Pending) {
            return Ok(false);
        }

        approval.reject(reason.clone());
        self.store.save(approval)?;

        info!(
            "Rejected request: {} (reason: {:?})",
            id,
            reason.as_deref().unwrap_or("none")
        );
        Ok(true)
    }

    /// Set the job ID after approval is processed
    pub fn set_job_id(&mut self, approval_id: &ApprovalId, job_id: String) -> Result<()> {
        let approval = self
            .approvals
            .get_mut(approval_id)
            .context("Approval not found")?;

        approval.job_id = Some(job_id);
        self.store.save(approval)?;

        Ok(())
    }

    /// List approvals with optional status filter
    pub fn list_approvals(&self, status_filter: Option<&str>) -> Vec<&ApprovalRequest> {
        let mut approvals: Vec<&ApprovalRequest> = self
            .approvals
            .values()
            .filter(|a| {
                status_filter
                    .map(|s| a.status.status_str() == s)
                    .unwrap_or(true)
            })
            .collect();

        // Sort by created_at descending (newest first)
        approvals.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        approvals
    }

    /// List pending approvals
    pub fn list_pending(&self) -> Vec<&ApprovalRequest> {
        self.list_approvals(Some("pending"))
    }

    /// Check and expire any pending approvals past their expiry time
    pub fn check_expired(&mut self) -> Result<Vec<ApprovalId>> {
        let now = Utc::now();
        let mut expired = Vec::new();

        for (id, approval) in &mut self.approvals {
            if matches!(approval.status, ApprovalStatus::Pending) && approval.expires_at < now {
                approval.mark_expired();
                expired.push(id.clone());
                warn!("Approval {} expired", id);
            }
        }

        // Persist changes
        for id in &expired {
            if let Some(approval) = self.approvals.get(id) {
                self.store.save(approval)?;
            }
        }

        Ok(expired)
    }

    /// Clean up old terminal approvals
    pub fn cleanup_old_approvals(&mut self) -> Result<usize> {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::days(APPROVAL_TTL_DAYS);

        let to_remove: Vec<ApprovalId> = self
            .approvals
            .iter()
            .filter(|(_, a)| a.status.is_terminal() && a.created_at < cutoff)
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();

        for id in to_remove {
            self.approvals.remove(&id);
            self.store.delete(&id)?;
        }

        if count > 0 {
            info!("Cleaned up {} old approvals", count);
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PluginRef;
    use tempfile::TempDir;

    fn create_test_manager() -> (ApprovalManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let manager = ApprovalManager::new(temp.path().to_path_buf()).unwrap();
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
        let (mut manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        assert!(matches!(approval.status, ApprovalStatus::Pending));
        assert!(!approval.is_expired());
    }

    #[test]
    fn test_approve() {
        let (mut manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        let id = approval.approval_id.clone();

        let approved = manager.approve(&id).unwrap();
        assert!(approved);

        let approval = manager.get_approval(&id).unwrap();
        assert!(matches!(approval.status, ApprovalStatus::Approved { .. }));
    }

    #[test]
    fn test_reject() {
        let (mut manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        let id = approval.approval_id.clone();

        let rejected = manager
            .reject(&id, Some("Not approved".to_string()))
            .unwrap();
        assert!(rejected);

        let approval = manager.get_approval(&id).unwrap();
        assert!(matches!(approval.status, ApprovalStatus::Rejected { .. }));
    }

    #[test]
    fn test_list_pending() {
        let (mut manager, _temp) = create_test_manager();

        manager
            .create_approval(test_operation(), test_summary())
            .unwrap();
        manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        let pending = manager.list_pending();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_approve_command() {
        let (mut manager, _temp) = create_test_manager();

        let approval = manager
            .create_approval(test_operation(), test_summary())
            .unwrap();

        let cmd = approval.approve_command();
        assert!(cmd.contains("casparian approvals approve"));
        assert!(cmd.contains(&approval.approval_id.0));
    }
}

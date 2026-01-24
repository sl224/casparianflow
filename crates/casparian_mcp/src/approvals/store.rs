//! Approval Store - Persistent Approval State
//!
//! Stores approval requests in JSON files for persistence and CLI access.
//!
//! # Storage Format
//!
//! ```text
//! ~/.casparian_flow/approvals/
//! ├── {approval_id_1}.json
//! ├── {approval_id_2}.json
//! └── ...
//! ```

use super::{ApprovalId, ApprovalRequest};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::debug;

/// Persistent approval store
pub struct ApprovalStore {
    /// Directory for approval files
    dir: PathBuf,
}

impl ApprovalStore {
    /// Create a new approval store
    pub fn new(dir: PathBuf) -> Result<Self> {
        // Ensure directory exists
        fs::create_dir_all(&dir).with_context(|| {
            format!(
                "Failed to create approval store directory: {}",
                dir.display()
            )
        })?;

        Ok(Self { dir })
    }

    /// Get the file path for an approval
    fn approval_path(&self, id: &ApprovalId) -> PathBuf {
        self.dir.join(format!("{}.json", id.0))
    }

    /// Save an approval to disk
    pub fn save(&self, approval: &ApprovalRequest) -> Result<()> {
        let path = self.approval_path(&approval.approval_id);
        let json = serde_json::to_string_pretty(approval)?;

        atomic_write(&path, json.as_bytes())
            .with_context(|| format!("Failed to write approval file: {}", path.display()))?;

        debug!(
            "Saved approval {} to {}",
            approval.approval_id,
            path.display()
        );
        Ok(())
    }

    /// Load an approval from disk
    pub fn load(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>> {
        let path = self.approval_path(id);

        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read approval file: {}", path.display()))?;

        let approval: ApprovalRequest = serde_json::from_str(&json)
            .with_context(|| format!("Failed to parse approval file: {}", path.display()))?;

        Ok(Some(approval))
    }

    /// Load all approvals from disk
    pub fn load_all(&self) -> Result<Vec<ApprovalRequest>> {
        let mut approvals = Vec::new();

        let entries = fs::read_dir(&self.dir).with_context(|| {
            format!(
                "Failed to read approval store directory: {}",
                self.dir.display()
            )
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let json = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read approval file: {}", path.display()))?;
            let approval: ApprovalRequest = serde_json::from_str(&json)
                .with_context(|| format!("Failed to parse approval file: {}", path.display()))?;
            approvals.push(approval);
        }

        debug!(
            "Loaded {} approvals from {}",
            approvals.len(),
            self.dir.display()
        );
        Ok(approvals)
    }

    /// Delete an approval from disk
    pub fn delete(&self, id: &ApprovalId) -> Result<bool> {
        let path = self.approval_path(id);

        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete approval file: {}", path.display()))?;

        debug!("Deleted approval {} from {}", id, path.display());
        Ok(true)
    }

    /// Get the storage directory
    #[allow(dead_code)]
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

/// Atomic write via temp file + rename
fn atomic_write(path: &PathBuf, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let temp_path = parent.join(format!(".tmp_{}", uuid::Uuid::new_v4()));
    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;
    fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approvals::ApprovalOperation;
    use crate::types::{ApprovalSummary, PluginRef};
    use tempfile::TempDir;

    fn test_approval() -> ApprovalRequest {
        ApprovalRequest::new(
            ApprovalOperation::Run {
                plugin_ref: PluginRef::registered("test"),
                input_dir: PathBuf::from("/data"),
                output: "parquet://./out/".to_string(),
            },
            ApprovalSummary {
                description: "Test".to_string(),
                file_count: 1,
                estimated_rows: None,
                target_path: "./out/".to_string(),
            },
        )
    }

    #[test]
    fn test_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store = ApprovalStore::new(temp.path().to_path_buf()).unwrap();

        let approval = test_approval();
        let id = approval.approval_id.clone();

        store.save(&approval).unwrap();

        let loaded = store.load(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().approval_id, id);
    }

    #[test]
    fn test_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = ApprovalStore::new(temp.path().to_path_buf()).unwrap();

        let id = ApprovalId::from_string("nonexistent");
        let loaded = store.load(&id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_all() {
        let temp = TempDir::new().unwrap();
        let store = ApprovalStore::new(temp.path().to_path_buf()).unwrap();

        // Save multiple approvals
        for _ in 0..3 {
            let approval = test_approval();
            store.save(&approval).unwrap();
        }

        let approvals = store.load_all().unwrap();
        assert_eq!(approvals.len(), 3);
    }

    #[test]
    fn test_delete() {
        let temp = TempDir::new().unwrap();
        let store = ApprovalStore::new(temp.path().to_path_buf()).unwrap();

        let approval = test_approval();
        let id = approval.approval_id.clone();

        store.save(&approval).unwrap();
        assert!(store.load(&id).unwrap().is_some());

        let deleted = store.delete(&id).unwrap();
        assert!(deleted);

        assert!(store.load(&id).unwrap().is_none());
    }
}

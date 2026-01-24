//! Draft management for AI Wizards
//!
//! Drafts are temporary artifacts that await human approval before being
//! committed to the runtime configuration.

use super::types::{Draft, DraftContext, DraftId, DraftStatus, DraftType};
use casparian_db::{BackendError, DbConnection, DbValue, UnifiedDbRow};
use chrono::{Duration, Utc};
use std::path::{Path, PathBuf};

/// Default draft expiry time (24 hours)
const DRAFT_EXPIRY_HOURS: i64 = 24;

/// Error type for draft operations
#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("Draft not found: {0}")]
    NotFound(String),

    #[error("Draft has expired")]
    Expired,

    #[error("Draft is not pending (status: {0})")]
    NotPending(String),

    #[error("Database error: {0}")]
    Database(#[from] BackendError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for draft operations
pub type Result<T> = std::result::Result<T, DraftError>;

/// Manages draft artifacts for AI Wizards
#[derive(Clone)]
pub struct DraftManager {
    conn: DbConnection,
    drafts_dir: PathBuf,
}

impl DraftManager {
    /// Create a new draft manager
    pub fn new(conn: DbConnection, drafts_dir: PathBuf) -> Self {
        Self { conn, drafts_dir }
    }

    /// Get the drafts directory path
    pub fn drafts_dir(&self) -> &Path {
        &self.drafts_dir
    }

    /// Ensure the drafts directory exists
    pub fn ensure_drafts_dir(&self) -> Result<()> {
        if !self.drafts_dir.exists() {
            std::fs::create_dir_all(&self.drafts_dir)?;
        }
        Ok(())
    }

    /// Create a new draft
    ///
    /// The content is written to a file in the drafts directory and metadata
    /// is stored in the database.
    pub fn create_draft(
        &self,
        draft_type: DraftType,
        content: &str,
        context: DraftContext,
        model_name: Option<&str>,
    ) -> Result<Draft> {
        self.ensure_drafts_dir()?;

        let id = DraftId::new();
        let now = Utc::now();
        let expires_at = now + Duration::hours(DRAFT_EXPIRY_HOURS);

        // Determine file extension based on draft type
        let extension = match draft_type {
            DraftType::Extractor | DraftType::SemanticRule => "yaml",
            DraftType::Parser => "py",
            DraftType::Label => "txt",
        };

        // Create the file path
        let filename = format!("{}_{}.{}", draft_type.as_str(), id.as_str(), extension);
        let file_path = self.drafts_dir.join(&filename);

        // Write content to file
        std::fs::write(&file_path, content)?;

        // Serialize context
        let context_json = serde_json::to_string(&context)?;

        // Insert into database
        let file_path_str = file_path.to_string_lossy().to_string();
        self.conn.execute(
            r#"
                INSERT INTO cf_ai_drafts (
                    id, draft_type, file_path, status, source_context_json,
                    model_name, created_at, expires_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            &[
                DbValue::from(id.as_str()),
                DbValue::from(draft_type.as_str()),
                DbValue::from(file_path_str),
                DbValue::from(DraftStatus::Pending.as_str()),
                DbValue::from(context_json),
                DbValue::from(model_name),
                DbValue::from(now.timestamp_millis()),
                DbValue::from(expires_at.timestamp_millis()),
            ],
        )?;

        Ok(Draft {
            id,
            draft_type,
            file_path,
            status: DraftStatus::Pending,
            source_context: context,
            model_name: model_name.map(String::from),
            created_at: now,
            expires_at,
            approved_at: None,
            approved_by: None,
        })
    }

    /// Get a draft by ID
    pub fn get_draft(&self, id: &DraftId) -> Result<Option<Draft>> {
        let row = self.conn.query_optional(
            r#"
                SELECT id, draft_type, file_path, status, source_context_json,
                       model_name, created_at, expires_at, approved_at, approved_by
                FROM cf_ai_drafts
                WHERE id = ?
                "#,
            &[DbValue::from(id.as_str())],
        )?;

        match row {
            Some(row) => Ok(Some(self.row_to_draft(&row)?)),
            None => Ok(None),
        }
    }

    /// List all pending drafts
    pub fn list_pending(&self) -> Result<Vec<Draft>> {
        let now_millis = Utc::now().timestamp_millis();

        let rows = self.conn.query_all(
            r#"
                SELECT id, draft_type, file_path, status, source_context_json,
                       model_name, created_at, expires_at, approved_at, approved_by
                FROM cf_ai_drafts
                WHERE status = ? AND expires_at > ?
                ORDER BY created_at DESC
                "#,
            &[
                DbValue::from(DraftStatus::Pending.as_str()),
                DbValue::from(now_millis),
            ],
        )?;

        let mut drafts = Vec::with_capacity(rows.len());
        for row in rows {
            drafts.push(self.row_to_draft(&row)?);
        }
        Ok(drafts)
    }

    /// List drafts by type
    pub fn list_by_type(&self, draft_type: DraftType) -> Result<Vec<Draft>> {
        let rows = self.conn.query_all(
            r#"
                SELECT id, draft_type, file_path, status, source_context_json,
                       model_name, created_at, expires_at, approved_at, approved_by
                FROM cf_ai_drafts
                WHERE draft_type = ?
                ORDER BY created_at DESC
                "#,
            &[DbValue::from(draft_type.as_str())],
        )?;

        let mut drafts = Vec::with_capacity(rows.len());
        for row in rows {
            drafts.push(self.row_to_draft(&row)?);
        }
        Ok(drafts)
    }

    /// Approve a draft
    ///
    /// This marks the draft as approved and records who approved it.
    /// The actual commit to runtime configuration is handled by the caller.
    pub fn approve_draft(&self, id: &DraftId, approved_by: &str) -> Result<Draft> {
        let draft = self
            .get_draft(id)?
            .ok_or_else(|| DraftError::NotFound(id.to_string()))?;

        if draft.status != DraftStatus::Pending {
            return Err(DraftError::NotPending(draft.status.to_string()));
        }

        if draft.is_expired() {
            return Err(DraftError::Expired);
        }

        let now = Utc::now();
        let now_millis = now.timestamp_millis();

        self.conn.execute(
            r#"
                UPDATE cf_ai_drafts
                SET status = ?, approved_at = ?, approved_by = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(DraftStatus::Approved.as_str()),
                DbValue::from(now_millis),
                DbValue::from(approved_by),
                DbValue::from(id.as_str()),
            ],
        )?;

        Ok(Draft {
            status: DraftStatus::Approved,
            approved_at: Some(now),
            approved_by: Some(approved_by.to_string()),
            ..draft
        })
    }

    /// Reject a draft
    pub fn reject_draft(&self, id: &DraftId) -> Result<()> {
        let draft = self
            .get_draft(id)?
            .ok_or_else(|| DraftError::NotFound(id.to_string()))?;

        if draft.status != DraftStatus::Pending {
            return Err(DraftError::NotPending(draft.status.to_string()));
        }

        self.conn.execute(
            r#"
                UPDATE cf_ai_drafts
                SET status = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(DraftStatus::Rejected.as_str()),
                DbValue::from(id.as_str()),
            ],
        )?;

        // Optionally delete the file
        if draft.file_path.exists() {
            let _ = std::fs::remove_file(&draft.file_path);
        }

        Ok(())
    }

    /// Clean up expired drafts
    ///
    /// Returns the number of drafts cleaned up.
    pub fn cleanup_expired(&self) -> Result<usize> {
        let now_millis = Utc::now().timestamp_millis();

        // Get expired drafts to delete their files
        let expired_rows = self.conn.query_all(
            r#"
                SELECT file_path FROM cf_ai_drafts
                WHERE status = ? AND expires_at <= ?
                "#,
            &[
                DbValue::from(DraftStatus::Pending.as_str()),
                DbValue::from(now_millis),
            ],
        )?;

        // Delete files
        for row in &expired_rows {
            let file_path: String = row.get_by_name("file_path")?;
            let path = PathBuf::from(&file_path);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }

        // Update status in database
        let result = self.conn.execute(
            r#"
                UPDATE cf_ai_drafts
                SET status = ?
                WHERE status = ? AND expires_at <= ?
                "#,
            &[
                DbValue::from(DraftStatus::Expired.as_str()),
                DbValue::from(DraftStatus::Pending.as_str()),
                DbValue::from(now_millis),
            ],
        )?;

        Ok(result as usize)
    }

    /// Delete all drafts (for testing)
    #[cfg(test)]
    pub fn delete_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM cf_ai_drafts", &[])?;
        Ok(())
    }

    /// Convert a database row to a Draft
    fn row_to_draft(&self, row: &UnifiedDbRow) -> Result<Draft> {
        let id: String = row.get_by_name("id")?;
        let draft_type_str: String = row.get_by_name("draft_type")?;
        let file_path: String = row.get_by_name("file_path")?;
        let status_str: String = row.get_by_name("status")?;
        let context_json: Option<String> = row.get_by_name("source_context_json")?;
        let model_name: Option<String> = row.get_by_name("model_name")?;
        let created_at_millis: i64 = row.get_by_name("created_at")?;
        let expires_at_millis: i64 = row.get_by_name("expires_at")?;
        let approved_at_millis: Option<i64> = row.get_by_name("approved_at")?;
        let approved_by: Option<String> = row.get_by_name("approved_by")?;

        let draft_type = DraftType::from_str(&draft_type_str).ok_or_else(|| {
            DraftError::Database(BackendError::TypeConversion(format!(
                "Invalid draft_type: {}",
                draft_type_str
            )))
        })?;

        let status = DraftStatus::from_str(&status_str).ok_or_else(|| {
            DraftError::Database(BackendError::TypeConversion(format!(
                "Invalid status: {}",
                status_str
            )))
        })?;

        let source_context: DraftContext = context_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?
            .unwrap_or_default();

        let created_at =
            chrono::DateTime::from_timestamp_millis(created_at_millis).unwrap_or_else(Utc::now);
        let expires_at =
            chrono::DateTime::from_timestamp_millis(expires_at_millis).unwrap_or_else(Utc::now);
        let approved_at = approved_at_millis.and_then(chrono::DateTime::from_timestamp_millis);

        Ok(Draft {
            id: DraftId::from_str(&id),
            draft_type,
            file_path: PathBuf::from(file_path),
            status,
            source_context,
            model_name,
            created_at,
            expires_at,
            approved_at,
            approved_by,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian_db::DbConnection;
    use tempfile::TempDir;

    fn setup() -> (DraftManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        let conn = DbConnection::open_duckdb(&db_path).unwrap();

        // Create the drafts table
        let schema = format!(
            r#"
            CREATE TABLE IF NOT EXISTS cf_ai_drafts (
                id TEXT PRIMARY KEY,
                draft_type TEXT NOT NULL,
                file_path TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT '{}',
                source_context_json TEXT,
                model_name TEXT,
                created_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                approved_at BIGINT,
                approved_by TEXT
            )
            "#,
            DraftStatus::Pending.as_str()
        );
        conn.execute_batch(&schema).unwrap();

        let drafts_dir = temp_dir.path().join("drafts");
        let manager = DraftManager::new(conn, drafts_dir);
        (manager, temp_dir)
    }

    #[test]
    fn test_create_and_get_draft() {
        let (manager, _temp) = setup();

        let context = DraftContext {
            sample_paths: vec![PathBuf::from("/data/test.csv")],
            user_hints: Some("Extract date from filename".to_string()),
            source_id: Some("src_123".to_string()),
            tag_name: Some("test_tag".to_string()),
        };

        let draft = manager
            .create_draft(
                DraftType::Extractor,
                "name: test_rule\nglob: '**/*.csv'",
                context.clone(),
                Some("qwen2.5-coder:7b"),
            )
            .unwrap();

        assert_eq!(draft.draft_type, DraftType::Extractor);
        assert_eq!(draft.status, DraftStatus::Pending);
        assert!(draft.file_path.exists());

        let retrieved = manager.get_draft(&draft.id).unwrap().unwrap();
        assert_eq!(retrieved.id, draft.id);
        assert_eq!(retrieved.source_context.user_hints, context.user_hints);
    }

    #[test]
    fn test_approve_draft() {
        let (manager, _temp) = setup();

        let draft = manager
            .create_draft(
                DraftType::Parser,
                "def parse(df): return df",
                DraftContext::default(),
                None,
            )
            .unwrap();

        let approved = manager.approve_draft(&draft.id, "test_user").unwrap();

        assert_eq!(approved.status, DraftStatus::Approved);
        assert_eq!(approved.approved_by, Some("test_user".to_string()));
        assert!(approved.approved_at.is_some());
    }

    #[test]
    fn test_reject_draft() {
        let (manager, _temp) = setup();

        let draft = manager
            .create_draft(
                DraftType::Label,
                "sales_data",
                DraftContext::default(),
                None,
            )
            .unwrap();

        assert!(draft.file_path.exists());

        manager.reject_draft(&draft.id).unwrap();

        let retrieved = manager.get_draft(&draft.id).unwrap().unwrap();
        assert_eq!(retrieved.status, DraftStatus::Rejected);
        assert!(!draft.file_path.exists()); // File should be deleted
    }

    #[test]
    fn test_list_pending() {
        let (manager, _temp) = setup();

        // Create multiple drafts
        for i in 0..3 {
            manager
                .create_draft(
                    DraftType::Extractor,
                    &format!("rule_{}", i),
                    DraftContext::default(),
                    None,
                )
                .unwrap();
        }

        let pending = manager.list_pending().unwrap();
        assert_eq!(pending.len(), 3);
    }
}

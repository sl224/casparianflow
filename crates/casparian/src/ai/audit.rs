//! Audit logging for AI Wizards
//!
//! Every LLM interaction is logged for debugging, compliance, and training data collection.

use super::types::{AuditStatus, WizardType};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

/// Error type for audit operations
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for audit operations
pub type Result<T> = std::result::Result<T, AuditError>;

/// An entry in the audit log
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// Unique identifier
    pub id: String,
    /// Which wizard made this call
    pub wizard_type: WizardType,
    /// Model used (e.g., "qwen2.5-coder:7b")
    pub model_name: String,
    /// Type of input (e.g., "paths", "sample", "headers")
    pub input_type: String,
    /// Hash of the input for deduplication
    pub input_hash: String,
    /// Preview of the input (first 500 chars)
    pub input_preview: Option<String>,
    /// Columns that were redacted
    pub redactions: Vec<String>,
    /// Type of output generated
    pub output_type: Option<String>,
    /// Hash of the output
    pub output_hash: Option<String>,
    /// Path to output file (if saved)
    pub output_file: Option<String>,
    /// How long the LLM call took
    pub duration_ms: Option<i64>,
    /// Status of the call
    pub status: AuditStatus,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Attempt number (for retries)
    pub attempt_number: u32,
    /// When this entry was created
    pub created_at: DateTime<Utc>,
}

impl AuditEntry {
    /// Create a new audit entry for a starting call
    pub fn new(wizard_type: WizardType, model_name: &str, input_type: &str, input_hash: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string()[..16].to_string(),
            wizard_type,
            model_name: model_name.to_string(),
            input_type: input_type.to_string(),
            input_hash: input_hash.to_string(),
            input_preview: None,
            redactions: Vec::new(),
            output_type: None,
            output_hash: None,
            output_file: None,
            duration_ms: None,
            status: AuditStatus::Success, // Will be updated
            error_message: None,
            attempt_number: 1,
            created_at: Utc::now(),
        }
    }

    /// Set the input preview
    pub fn with_input_preview(mut self, preview: &str) -> Self {
        self.input_preview = Some(preview.chars().take(500).collect());
        self
    }

    /// Set redacted columns
    pub fn with_redactions(mut self, redactions: Vec<String>) -> Self {
        self.redactions = redactions;
        self
    }

    /// Set output information
    pub fn with_output(mut self, output_type: &str, output_hash: &str) -> Self {
        self.output_type = Some(output_type.to_string());
        self.output_hash = Some(output_hash.to_string());
        self
    }

    /// Set output file path
    pub fn with_output_file(mut self, path: &str) -> Self {
        self.output_file = Some(path.to_string());
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration_ms: i64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set status
    pub fn with_status(mut self, status: AuditStatus) -> Self {
        self.status = status;
        self
    }

    /// Set error message
    pub fn with_error(mut self, message: &str) -> Self {
        self.error_message = Some(message.to_string());
        self
    }

    /// Set attempt number
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt_number = attempt;
        self
    }
}

/// Audit logger for AI wizard operations
#[derive(Clone)]
pub struct AuditLogger {
    pool: SqlitePool,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Log an audit entry
    pub async fn log(&self, entry: &AuditEntry) -> Result<()> {
        let redactions_json = serde_json::to_string(&entry.redactions)?;

        sqlx::query(
            r#"
            INSERT INTO cf_ai_audit_log (
                id, wizard_type, model_name, input_type, input_hash,
                input_preview, redactions_json, output_type, output_hash,
                output_file, duration_ms, status, error_message,
                attempt_number, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry.id)
        .bind(entry.wizard_type.as_str())
        .bind(&entry.model_name)
        .bind(&entry.input_type)
        .bind(&entry.input_hash)
        .bind(&entry.input_preview)
        .bind(&redactions_json)
        .bind(&entry.output_type)
        .bind(&entry.output_hash)
        .bind(&entry.output_file)
        .bind(entry.duration_ms)
        .bind(entry.status.as_str())
        .bind(&entry.error_message)
        .bind(entry.attempt_number as i64)
        .bind(entry.created_at.timestamp_millis())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Query recent audit entries
    pub async fn query_recent(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, wizard_type, model_name, input_type, input_hash,
                   input_preview, redactions_json, output_type, output_hash,
                   output_file, duration_ms, status, error_message,
                   attempt_number, created_at
            FROM cf_ai_audit_log
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.row_to_entry(&row)?);
        }
        Ok(entries)
    }

    /// Query entries by wizard type
    pub async fn query_by_wizard(&self, wizard_type: WizardType, limit: usize) -> Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, wizard_type, model_name, input_type, input_hash,
                   input_preview, redactions_json, output_type, output_hash,
                   output_file, duration_ms, status, error_message,
                   attempt_number, created_at
            FROM cf_ai_audit_log
            WHERE wizard_type = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(wizard_type.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.row_to_entry(&row)?);
        }
        Ok(entries)
    }

    /// Query entries by status
    pub async fn query_by_status(&self, status: AuditStatus, limit: usize) -> Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, wizard_type, model_name, input_type, input_hash,
                   input_preview, redactions_json, output_type, output_hash,
                   output_file, duration_ms, status, error_message,
                   attempt_number, created_at
            FROM cf_ai_audit_log
            WHERE status = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(status.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.row_to_entry(&row)?);
        }
        Ok(entries)
    }

    /// Count entries by status
    pub async fn count_by_status(&self) -> Result<Vec<(AuditStatus, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT status, COUNT(*) as count
            FROM cf_ai_audit_log
            GROUP BY status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut counts = Vec::new();
        for row in rows {
            let status_str: String = row.get("status");
            let count: i64 = row.get("count");
            if let Some(status) = AuditStatus::from_str(&status_str) {
                counts.push((status, count));
            }
        }
        Ok(counts)
    }

    /// Clean up old audit entries
    ///
    /// Keeps entries newer than `retention_days` for success status,
    /// and longer for errors (2x retention for errors, 3x for critical).
    pub async fn cleanup_old(&self, retention_days: u32) -> Result<usize> {
        let now = Utc::now();
        let success_cutoff = (now - chrono::Duration::days(retention_days as i64)).timestamp_millis();
        let error_cutoff = (now - chrono::Duration::days(retention_days as i64 * 2)).timestamp_millis();

        // Delete old success entries
        let result1 = sqlx::query(
            r#"
            DELETE FROM cf_ai_audit_log
            WHERE status = 'success' AND created_at < ?
            "#,
        )
        .bind(success_cutoff)
        .execute(&self.pool)
        .await?;

        // Delete old error entries (longer retention)
        let result2 = sqlx::query(
            r#"
            DELETE FROM cf_ai_audit_log
            WHERE status IN ('error', 'timeout') AND created_at < ?
            "#,
        )
        .bind(error_cutoff)
        .execute(&self.pool)
        .await?;

        Ok((result1.rows_affected() + result2.rows_affected()) as usize)
    }

    /// Convert a database row to an AuditEntry
    fn row_to_entry(&self, row: &sqlx::sqlite::SqliteRow) -> Result<AuditEntry> {
        let id: String = row.get("id");
        let wizard_type_str: String = row.get("wizard_type");
        let model_name: String = row.get("model_name");
        let input_type: String = row.get("input_type");
        let input_hash: String = row.get("input_hash");
        let input_preview: Option<String> = row.get("input_preview");
        let redactions_json: Option<String> = row.get("redactions_json");
        let output_type: Option<String> = row.get("output_type");
        let output_hash: Option<String> = row.get("output_hash");
        let output_file: Option<String> = row.get("output_file");
        let duration_ms: Option<i64> = row.get("duration_ms");
        let status_str: String = row.get("status");
        let error_message: Option<String> = row.get("error_message");
        let attempt_number: i64 = row.get("attempt_number");
        let created_at_millis: i64 = row.get("created_at");

        let wizard_type = WizardType::from_str(&wizard_type_str)
            .unwrap_or(WizardType::Pathfinder); // Fallback

        let redactions: Vec<String> = redactions_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?
            .unwrap_or_default();

        let status = AuditStatus::from_str(&status_str)
            .unwrap_or(AuditStatus::Success); // Fallback

        let created_at = chrono::DateTime::from_timestamp_millis(created_at_millis)
            .unwrap_or_else(Utc::now);

        Ok(AuditEntry {
            id,
            wizard_type,
            model_name,
            input_type,
            input_hash,
            input_preview,
            redactions,
            output_type,
            output_hash,
            output_file,
            duration_ms,
            status,
            error_message,
            attempt_number: attempt_number as u32,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> AuditLogger {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        // Create the audit table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_ai_audit_log (
                id TEXT PRIMARY KEY,
                wizard_type TEXT NOT NULL,
                model_name TEXT NOT NULL,
                input_type TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                input_preview TEXT,
                redactions_json TEXT,
                output_type TEXT,
                output_hash TEXT,
                output_file TEXT,
                duration_ms INTEGER,
                status TEXT NOT NULL,
                error_message TEXT,
                attempt_number INTEGER DEFAULT 1,
                created_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        AuditLogger::new(pool)
    }

    #[tokio::test]
    async fn test_log_and_query() {
        let logger = setup().await;

        let entry = AuditEntry::new(
            WizardType::Pathfinder,
            "qwen2.5-coder:7b",
            "paths",
            "abc123",
        )
        .with_input_preview("/data/test/*.csv")
        .with_output("yaml_rule", "def456")
        .with_duration(1500)
        .with_status(AuditStatus::Success);

        logger.log(&entry).await.unwrap();

        let recent = logger.query_recent(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].wizard_type, WizardType::Pathfinder);
        assert_eq!(recent[0].model_name, "qwen2.5-coder:7b");
    }

    #[tokio::test]
    async fn test_query_by_wizard() {
        let logger = setup().await;

        // Log entries for different wizards
        for wt in [WizardType::Pathfinder, WizardType::ParserLab, WizardType::Pathfinder] {
            let entry = AuditEntry::new(wt, "model", "input", "hash");
            logger.log(&entry).await.unwrap();
        }

        let pathfinder_entries = logger
            .query_by_wizard(WizardType::Pathfinder, 10)
            .await
            .unwrap();
        assert_eq!(pathfinder_entries.len(), 2);

        let parser_entries = logger
            .query_by_wizard(WizardType::ParserLab, 10)
            .await
            .unwrap();
        assert_eq!(parser_entries.len(), 1);
    }

    #[tokio::test]
    async fn test_count_by_status() {
        let logger = setup().await;

        // Log entries with different statuses
        for status in [AuditStatus::Success, AuditStatus::Success, AuditStatus::Error] {
            let entry = AuditEntry::new(WizardType::Pathfinder, "model", "input", "hash")
                .with_status(status);
            logger.log(&entry).await.unwrap();
        }

        let counts = logger.count_by_status().await.unwrap();
        assert!(counts.iter().any(|(s, c)| *s == AuditStatus::Success && *c == 2));
        assert!(counts.iter().any(|(s, c)| *s == AuditStatus::Error && *c == 1));
    }
}

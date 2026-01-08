//! High-failure file tracking for fail-fast optimization
//!
//! Tracks files that have historically failed during backtest iterations.
//! Files with high failure rates are tested first to enable early termination.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use thiserror::Error;
use uuid::Uuid;

use crate::metrics::FailureCategory;

/// Error types for high-failure table operations
#[derive(Error, Debug)]
pub enum HighFailureError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

/// A file that has historically failed during backtest iterations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighFailureFile {
    /// Unique identifier for this entry
    pub file_id: Uuid,
    /// Absolute path to the file
    pub file_path: String,
    /// The scope (parser/pipeline) this failure belongs to
    pub scope_id: Uuid,
    /// Total number of failures across all iterations
    pub failure_count: usize,
    /// Number of consecutive failures (resets on success)
    pub consecutive_failures: usize,
    /// When this file first failed
    pub first_failure_at: DateTime<Utc>,
    /// When this file last failed
    pub last_failure_at: DateTime<Utc>,
    /// When this file was last tested (pass or fail)
    pub last_tested_at: DateTime<Utc>,
    /// History of failures for this file
    pub failure_history: Vec<FailureHistoryEntry>,
}

/// A single failure event in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureHistoryEntry {
    /// Which backtest iteration this failure occurred in
    pub iteration: usize,
    /// Parser version at time of failure
    pub parser_version: usize,
    /// Category of failure
    pub failure_category: FailureCategory,
    /// Human-readable error message
    pub error_message: String,
    /// Whether this failure was later resolved
    pub resolved: bool,
    /// What resolved it (e.g., "parser v3", "schema update")
    pub resolved_by: Option<String>,
    /// When this failure occurred
    pub occurred_at: DateTime<Utc>,
}

impl FailureHistoryEntry {
    /// Create a new failure history entry
    pub fn new(
        iteration: usize,
        parser_version: usize,
        failure_category: FailureCategory,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            iteration,
            parser_version,
            failure_category,
            error_message: error_message.into(),
            resolved: false,
            resolved_by: None,
            occurred_at: Utc::now(),
        }
    }

    /// Mark this failure as resolved
    pub fn mark_resolved(mut self, resolved_by: impl Into<String>) -> Self {
        self.resolved = true;
        self.resolved_by = Some(resolved_by.into());
        self
    }
}

/// File info for backtest ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Unique identifier for this file
    pub id: Uuid,
    /// Absolute path to the file
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Whether this file has been tested before
    pub tested: bool,
    /// Whether this file is in the high-failure list
    pub is_high_failure: bool,
    /// Number of consecutive failures (0 if not high-failure)
    pub consecutive_failures: usize,
}

impl FileInfo {
    /// Create a new file info
    pub fn new(path: impl Into<String>, size: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            path: path.into(),
            size,
            tested: false,
            is_high_failure: false,
            consecutive_failures: 0,
        }
    }

    /// Create from an existing file entry with high-failure info
    pub fn with_high_failure(mut self, consecutive_failures: usize) -> Self {
        self.is_high_failure = true;
        self.consecutive_failures = consecutive_failures;
        self
    }
}

/// Tracks high-failure files for a scope
pub struct HighFailureTable {
    pool: Pool<Sqlite>,
}

impl HighFailureTable {
    /// Create a new high-failure table with the given pool
    pub async fn new(pool: Pool<Sqlite>) -> Result<Self, HighFailureError> {
        let table = Self { pool };
        table.init_schema().await?;
        Ok(table)
    }

    /// Open from a file path
    pub async fn open(path: &str) -> Result<Self, HighFailureError> {
        let db_url = format!("sqlite:{}?mode=rwc", path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        Self::new(pool).await
    }

    /// Create an in-memory table (for testing)
    pub async fn in_memory() -> Result<Self, HighFailureError> {
        let pool = SqlitePoolOptions::new()
            .connect(":memory:")
            .await?;
        Self::new(pool).await
    }

    /// Initialize the database schema
    async fn init_schema(&self) -> Result<(), HighFailureError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS high_failure_files (
                file_id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                failure_count INTEGER NOT NULL DEFAULT 0,
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                first_failure_at TEXT NOT NULL,
                last_failure_at TEXT NOT NULL,
                last_tested_at TEXT NOT NULL,
                failure_history_json TEXT NOT NULL DEFAULT '[]',
                UNIQUE(file_path, scope_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_high_failure_scope ON high_failure_files(scope_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_high_failure_consecutive ON high_failure_files(scope_id, consecutive_failures DESC)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record a failure for a file
    pub async fn record_failure(
        &self,
        file_path: &str,
        scope_id: &Uuid,
        entry: FailureHistoryEntry,
    ) -> Result<HighFailureFile, HighFailureError> {
        let now = Utc::now();
        let file_id = Uuid::new_v4();
        let scope_str = scope_id.to_string();

        // Check if entry exists
        let existing: Option<ExistingRow> = sqlx::query_as(
            r#"
            SELECT file_id, failure_count, consecutive_failures, first_failure_at, failure_history_json
            FROM high_failure_files
            WHERE file_path = ?1 AND scope_id = ?2
            "#,
        )
        .bind(file_path)
        .bind(&scope_str)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing) = existing {
            // Update existing entry
            let mut history: Vec<FailureHistoryEntry> =
                serde_json::from_str(&existing.failure_history_json)?;
            history.push(entry.clone());
            let new_history_json = serde_json::to_string(&history)?;

            sqlx::query(
                r#"
                UPDATE high_failure_files
                SET failure_count = ?1,
                    consecutive_failures = ?2,
                    last_failure_at = ?3,
                    last_tested_at = ?4,
                    failure_history_json = ?5
                WHERE file_path = ?6 AND scope_id = ?7
                "#,
            )
            .bind(existing.failure_count + 1)
            .bind(existing.consecutive_failures + 1)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(&new_history_json)
            .bind(file_path)
            .bind(&scope_str)
            .execute(&self.pool)
            .await?;

            let first_failure_at: DateTime<Utc> = existing
                .first_failure_at
                .parse()
                .unwrap_or(now);

            Ok(HighFailureFile {
                file_id: Uuid::parse_str(&existing.file_id).unwrap_or(file_id),
                file_path: file_path.to_string(),
                scope_id: *scope_id,
                failure_count: (existing.failure_count + 1) as usize,
                consecutive_failures: (existing.consecutive_failures + 1) as usize,
                first_failure_at,
                last_failure_at: now,
                last_tested_at: now,
                failure_history: history,
            })
        } else {
            // Insert new entry
            let history = vec![entry.clone()];
            let history_json = serde_json::to_string(&history)?;

            sqlx::query(
                r#"
                INSERT INTO high_failure_files
                (file_id, file_path, scope_id, failure_count, consecutive_failures,
                 first_failure_at, last_failure_at, last_tested_at, failure_history_json)
                VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(file_id.to_string())
            .bind(file_path)
            .bind(&scope_str)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(&history_json)
            .execute(&self.pool)
            .await?;

            Ok(HighFailureFile {
                file_id,
                file_path: file_path.to_string(),
                scope_id: *scope_id,
                failure_count: 1,
                consecutive_failures: 1,
                first_failure_at: now,
                last_failure_at: now,
                last_tested_at: now,
                failure_history: history,
            })
        }
    }

    /// Record a success for a file (resets consecutive failures)
    pub async fn record_success(
        &self,
        file_path: &str,
        scope_id: &Uuid,
    ) -> Result<(), HighFailureError> {
        let now = Utc::now();
        let scope_str = scope_id.to_string();

        // Get existing history
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT failure_history_json FROM high_failure_files WHERE file_path = ?1 AND scope_id = ?2",
        )
        .bind(file_path)
        .bind(&scope_str)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((history_json,)) = existing {
            let mut history: Vec<FailureHistoryEntry> = serde_json::from_str(&history_json)?;
            for entry in &mut history {
                if !entry.resolved {
                    entry.resolved = true;
                    entry.resolved_by = Some("backtest success".to_string());
                }
            }
            let new_history_json = serde_json::to_string(&history)?;

            sqlx::query(
                r#"
                UPDATE high_failure_files
                SET consecutive_failures = 0,
                    last_tested_at = ?1,
                    failure_history_json = ?2
                WHERE file_path = ?3 AND scope_id = ?4
                "#,
            )
            .bind(now.to_rfc3339())
            .bind(&new_history_json)
            .bind(file_path)
            .bind(&scope_str)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get all active high-failure files for a scope (consecutive_failures > 0)
    pub async fn get_active(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, HighFailureError> {
        let scope_str = scope_id.to_string();

        let rows: Vec<HighFailureRow> = sqlx::query_as(
            r#"
            SELECT file_id, file_path, scope_id, failure_count, consecutive_failures,
                   first_failure_at, last_failure_at, last_tested_at, failure_history_json
            FROM high_failure_files
            WHERE scope_id = ?1 AND consecutive_failures > 0
            ORDER BY consecutive_failures DESC
            "#,
        )
        .bind(&scope_str)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| r.into_high_failure_file())
            .collect()
    }

    /// Get all files for a scope (including resolved)
    pub async fn get_all(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, HighFailureError> {
        let scope_str = scope_id.to_string();

        let rows: Vec<HighFailureRow> = sqlx::query_as(
            r#"
            SELECT file_id, file_path, scope_id, failure_count, consecutive_failures,
                   first_failure_at, last_failure_at, last_tested_at, failure_history_json
            FROM high_failure_files
            WHERE scope_id = ?1
            ORDER BY consecutive_failures DESC, failure_count DESC
            "#,
        )
        .bind(&scope_str)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| r.into_high_failure_file())
            .collect()
    }

    /// Get files ordered for backtest: high-failure first, then resolved, then untested, then passing
    ///
    /// Order priority:
    /// 1. High-failure files (sorted by consecutive_failures DESC)
    /// 2. Resolved files (had failures but now passing)
    /// 3. Untested files (never been tested)
    /// 4. Passing files (tested and passed, never failed)
    pub async fn get_backtest_order(
        &self,
        all_files: &[FileInfo],
        scope_id: &Uuid,
    ) -> Result<Vec<FileInfo>, HighFailureError> {
        // Get all high-failure info
        let high_failure_files = self.get_all(scope_id).await?;
        let high_failure_map: std::collections::HashMap<String, &HighFailureFile> = high_failure_files
            .iter()
            .map(|f| (f.file_path.clone(), f))
            .collect();

        let mut result: Vec<FileInfo> = Vec::with_capacity(all_files.len());

        // Categorize files
        let mut high_failure: Vec<FileInfo> = Vec::new();
        let mut resolved: Vec<FileInfo> = Vec::new();
        let mut untested: Vec<FileInfo> = Vec::new();
        let mut passing: Vec<FileInfo> = Vec::new();

        for file in all_files {
            if let Some(hf) = high_failure_map.get(&file.path) {
                let mut file_with_info = file.clone();
                file_with_info.is_high_failure = hf.consecutive_failures > 0;
                file_with_info.consecutive_failures = hf.consecutive_failures;
                file_with_info.tested = true;

                if hf.consecutive_failures > 0 {
                    high_failure.push(file_with_info);
                } else {
                    // Was in high-failure table but now resolved
                    resolved.push(file_with_info);
                }
            } else if file.tested {
                passing.push(file.clone());
            } else {
                untested.push(file.clone());
            }
        }

        // Sort high-failure by consecutive failures (descending)
        high_failure.sort_by(|a, b| b.consecutive_failures.cmp(&a.consecutive_failures));

        // Combine in order
        result.extend(high_failure);
        result.extend(resolved);
        result.extend(untested);
        result.extend(passing);

        Ok(result)
    }

    /// Clear all entries for a scope (useful for fresh backtest)
    pub async fn clear_scope(&self, scope_id: &Uuid) -> Result<usize, HighFailureError> {
        let scope_str = scope_id.to_string();
        let result = sqlx::query("DELETE FROM high_failure_files WHERE scope_id = ?1")
            .bind(&scope_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as usize)
    }
}

/// Internal row type for existing record lookup
#[derive(sqlx::FromRow)]
struct ExistingRow {
    file_id: String,
    failure_count: i64,
    consecutive_failures: i64,
    first_failure_at: String,
    failure_history_json: String,
}

/// Internal row type for full record
#[derive(sqlx::FromRow)]
struct HighFailureRow {
    file_id: String,
    file_path: String,
    scope_id: String,
    failure_count: i64,
    consecutive_failures: i64,
    first_failure_at: String,
    last_failure_at: String,
    last_tested_at: String,
    failure_history_json: String,
}

impl HighFailureRow {
    fn into_high_failure_file(self) -> Result<HighFailureFile, HighFailureError> {
        let file_id = Uuid::parse_str(&self.file_id)
            .map_err(|e| HighFailureError::Parse(format!("Invalid file_id: {}", e)))?;
        let scope_id = Uuid::parse_str(&self.scope_id)
            .map_err(|e| HighFailureError::Parse(format!("Invalid scope_id: {}", e)))?;
        let first_failure_at = self.first_failure_at.parse()
            .map_err(|e| HighFailureError::Parse(format!("Invalid first_failure_at: {}", e)))?;
        let last_failure_at = self.last_failure_at.parse()
            .map_err(|e| HighFailureError::Parse(format!("Invalid last_failure_at: {}", e)))?;
        let last_tested_at = self.last_tested_at.parse()
            .map_err(|e| HighFailureError::Parse(format!("Invalid last_tested_at: {}", e)))?;
        let failure_history: Vec<FailureHistoryEntry> =
            serde_json::from_str(&self.failure_history_json)?;

        Ok(HighFailureFile {
            file_id,
            file_path: self.file_path,
            scope_id,
            failure_count: self.failure_count as usize,
            consecutive_failures: self.consecutive_failures as usize,
            first_failure_at,
            last_failure_at,
            last_tested_at,
            failure_history,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_table() -> HighFailureTable {
        HighFailureTable::in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn test_record_failure() {
        let table = create_test_table().await;
        let scope_id = Uuid::new_v4();

        let entry = FailureHistoryEntry::new(
            1,
            1,
            FailureCategory::TypeMismatch,
            "Expected Int64, got String",
        );

        let hf = table.record_failure("/path/to/file.csv", &scope_id, entry).await.unwrap();

        assert_eq!(hf.file_path, "/path/to/file.csv");
        assert_eq!(hf.failure_count, 1);
        assert_eq!(hf.consecutive_failures, 1);
        assert_eq!(hf.failure_history.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_failures_increment() {
        let table = create_test_table().await;
        let scope_id = Uuid::new_v4();

        // First failure
        let entry1 = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error 1");
        table.record_failure("/path/to/file.csv", &scope_id, entry1).await.unwrap();

        // Second failure
        let entry2 = FailureHistoryEntry::new(2, 2, FailureCategory::NullNotAllowed, "Error 2");
        let hf = table.record_failure("/path/to/file.csv", &scope_id, entry2).await.unwrap();

        assert_eq!(hf.failure_count, 2);
        assert_eq!(hf.consecutive_failures, 2);
        assert_eq!(hf.failure_history.len(), 2);
    }

    #[tokio::test]
    async fn test_success_resets_consecutive() {
        let table = create_test_table().await;
        let scope_id = Uuid::new_v4();

        // Record failures
        let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/to/file.csv", &scope_id, entry).await.unwrap();

        // Record success
        table.record_success("/path/to/file.csv", &scope_id).await.unwrap();

        // Should have no active high-failure files
        let active = table.get_active(&scope_id).await.unwrap();
        assert!(active.is_empty());

        // But should still be in all (with consecutive = 0)
        let all = table.get_all(&scope_id).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].consecutive_failures, 0);
        assert_eq!(all[0].failure_count, 1); // Still tracked total failures
    }

    #[tokio::test]
    async fn test_backtest_order() {
        let table = create_test_table().await;
        let scope_id = Uuid::new_v4();

        // Record some failures
        let entry1 = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).await.unwrap();
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).await.unwrap();
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).await.unwrap(); // 3 consecutive

        table.record_failure("/path/high2.csv", &scope_id, entry1.clone()).await.unwrap(); // 1 consecutive

        table.record_failure("/path/resolved.csv", &scope_id, entry1.clone()).await.unwrap();
        table.record_success("/path/resolved.csv", &scope_id).await.unwrap(); // resolved

        // Create file list
        let files = vec![
            FileInfo::new("/path/passing.csv", 100),
            FileInfo { tested: true, ..FileInfo::new("/path/passing.csv", 100) },
            FileInfo::new("/path/untested.csv", 100),
            FileInfo::new("/path/high1.csv", 100),
            FileInfo::new("/path/high2.csv", 100),
            FileInfo::new("/path/resolved.csv", 100),
        ];

        let ordered = table.get_backtest_order(&files, &scope_id).await.unwrap();

        // High failure (most consecutive first)
        assert_eq!(ordered[0].path, "/path/high1.csv");
        assert_eq!(ordered[0].consecutive_failures, 3);
        assert_eq!(ordered[1].path, "/path/high2.csv");
        assert_eq!(ordered[1].consecutive_failures, 1);

        // Then resolved
        assert_eq!(ordered[2].path, "/path/resolved.csv");
        assert_eq!(ordered[2].consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_clear_scope() {
        let table = create_test_table().await;
        let scope_id = Uuid::new_v4();
        let other_scope = Uuid::new_v4();

        let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/file1.csv", &scope_id, entry.clone()).await.unwrap();
        table.record_failure("/path/file2.csv", &scope_id, entry.clone()).await.unwrap();
        table.record_failure("/path/other.csv", &other_scope, entry).await.unwrap();

        let cleared = table.clear_scope(&scope_id).await.unwrap();
        assert_eq!(cleared, 2);

        // Other scope unaffected
        let all = table.get_all(&other_scope).await.unwrap();
        assert_eq!(all.len(), 1);
    }
}

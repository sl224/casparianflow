//! High-failure file tracking for fail-fast optimization
//!
//! Tracks files that have historically failed during backtest iterations.
//! Files with high failure rates are tested first to enable early termination.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::metrics::FailureCategory;

/// Error types for high-failure table operations
#[derive(Error, Debug)]
pub enum HighFailureError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("File not found: {0}")]
    FileNotFound(String),
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
    conn: Connection,
}

impl HighFailureTable {
    /// Create a new high-failure table with the given connection
    pub fn new(conn: Connection) -> Result<Self, HighFailureError> {
        let table = Self { conn };
        table.init_schema()?;
        Ok(table)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<(), HighFailureError> {
        self.conn.execute_batch(
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
            );

            CREATE INDEX IF NOT EXISTS idx_high_failure_scope
                ON high_failure_files(scope_id);
            CREATE INDEX IF NOT EXISTS idx_high_failure_consecutive
                ON high_failure_files(scope_id, consecutive_failures DESC);
            "#,
        )?;
        Ok(())
    }

    /// Record a failure for a file
    pub fn record_failure(
        &self,
        file_path: &str,
        scope_id: &Uuid,
        entry: FailureHistoryEntry,
    ) -> Result<HighFailureFile, HighFailureError> {
        let now = Utc::now();
        let file_id = Uuid::new_v4();
        let scope_str = scope_id.to_string();

        // Check if entry exists
        let existing: Option<(String, i64, i64, String, String)> = self
            .conn
            .query_row(
                "SELECT file_id, failure_count, consecutive_failures, first_failure_at, failure_history_json
                 FROM high_failure_files
                 WHERE file_path = ?1 AND scope_id = ?2",
                params![file_path, scope_str],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .ok();

        if let Some((existing_id, failure_count, consecutive, first_failure, history_json)) =
            existing
        {
            // Update existing entry
            let mut history: Vec<FailureHistoryEntry> = serde_json::from_str(&history_json)?;
            history.push(entry.clone());
            let new_history_json = serde_json::to_string(&history)?;

            self.conn.execute(
                "UPDATE high_failure_files
                 SET failure_count = ?1,
                     consecutive_failures = ?2,
                     last_failure_at = ?3,
                     last_tested_at = ?4,
                     failure_history_json = ?5
                 WHERE file_path = ?6 AND scope_id = ?7",
                params![
                    failure_count + 1,
                    consecutive + 1,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    new_history_json,
                    file_path,
                    scope_str
                ],
            )?;

            let first_failure_at: DateTime<Utc> = first_failure
                .parse()
                .unwrap_or(now);

            Ok(HighFailureFile {
                file_id: Uuid::parse_str(&existing_id).unwrap_or(file_id),
                file_path: file_path.to_string(),
                scope_id: *scope_id,
                failure_count: (failure_count + 1) as usize,
                consecutive_failures: (consecutive + 1) as usize,
                first_failure_at,
                last_failure_at: now,
                last_tested_at: now,
                failure_history: history,
            })
        } else {
            // Insert new entry
            let history = vec![entry.clone()];
            let history_json = serde_json::to_string(&history)?;

            self.conn.execute(
                "INSERT INTO high_failure_files
                 (file_id, file_path, scope_id, failure_count, consecutive_failures,
                  first_failure_at, last_failure_at, last_tested_at, failure_history_json)
                 VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, ?6, ?7)",
                params![
                    file_id.to_string(),
                    file_path,
                    scope_str,
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    history_json
                ],
            )?;

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
    pub fn record_success(&self, file_path: &str, scope_id: &Uuid) -> Result<(), HighFailureError> {
        let now = Utc::now();
        let scope_str = scope_id.to_string();

        // Mark all unresolved failures as resolved
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT failure_history_json FROM high_failure_files
                 WHERE file_path = ?1 AND scope_id = ?2",
                params![file_path, scope_str],
                |row| row.get(0),
            )
            .ok();

        if let Some(history_json) = existing {
            let mut history: Vec<FailureHistoryEntry> = serde_json::from_str(&history_json)?;
            for entry in &mut history {
                if !entry.resolved {
                    entry.resolved = true;
                    entry.resolved_by = Some("backtest success".to_string());
                }
            }
            let new_history_json = serde_json::to_string(&history)?;

            self.conn.execute(
                "UPDATE high_failure_files
                 SET consecutive_failures = 0,
                     last_tested_at = ?1,
                     failure_history_json = ?2
                 WHERE file_path = ?3 AND scope_id = ?4",
                params![now.to_rfc3339(), new_history_json, file_path, scope_str],
            )?;
        }

        Ok(())
    }

    /// Get all active high-failure files for a scope (consecutive_failures > 0)
    pub fn get_active(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, HighFailureError> {
        let scope_str = scope_id.to_string();

        let mut stmt = self.conn.prepare(
            "SELECT file_id, file_path, scope_id, failure_count, consecutive_failures,
                    first_failure_at, last_failure_at, last_tested_at, failure_history_json
             FROM high_failure_files
             WHERE scope_id = ?1 AND consecutive_failures > 0
             ORDER BY consecutive_failures DESC",
        )?;

        let files = stmt
            .query_map(params![scope_str], |row| {
                let file_id_str: String = row.get(0)?;
                let file_path: String = row.get(1)?;
                let scope_id_str: String = row.get(2)?;
                let failure_count: i64 = row.get(3)?;
                let consecutive_failures: i64 = row.get(4)?;
                let first_failure_str: String = row.get(5)?;
                let last_failure_str: String = row.get(6)?;
                let last_tested_str: String = row.get(7)?;
                let history_json: String = row.get(8)?;

                Ok((
                    file_id_str,
                    file_path,
                    scope_id_str,
                    failure_count,
                    consecutive_failures,
                    first_failure_str,
                    last_failure_str,
                    last_tested_str,
                    history_json,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(
                |(
                    file_id_str,
                    file_path,
                    scope_id_str,
                    failure_count,
                    consecutive_failures,
                    first_failure_str,
                    last_failure_str,
                    last_tested_str,
                    history_json,
                )| {
                    let file_id = Uuid::parse_str(&file_id_str).ok()?;
                    let scope_id = Uuid::parse_str(&scope_id_str).ok()?;
                    let first_failure_at = first_failure_str.parse().ok()?;
                    let last_failure_at = last_failure_str.parse().ok()?;
                    let last_tested_at = last_tested_str.parse().ok()?;
                    let failure_history: Vec<FailureHistoryEntry> =
                        serde_json::from_str(&history_json).ok()?;

                    Some(HighFailureFile {
                        file_id,
                        file_path,
                        scope_id,
                        failure_count: failure_count as usize,
                        consecutive_failures: consecutive_failures as usize,
                        first_failure_at,
                        last_failure_at,
                        last_tested_at,
                        failure_history,
                    })
                },
            )
            .collect();

        Ok(files)
    }

    /// Get all files for a scope (including resolved)
    pub fn get_all(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, HighFailureError> {
        let scope_str = scope_id.to_string();

        let mut stmt = self.conn.prepare(
            "SELECT file_id, file_path, scope_id, failure_count, consecutive_failures,
                    first_failure_at, last_failure_at, last_tested_at, failure_history_json
             FROM high_failure_files
             WHERE scope_id = ?1
             ORDER BY consecutive_failures DESC, failure_count DESC",
        )?;

        let files = stmt
            .query_map(params![scope_str], |row| {
                let file_id_str: String = row.get(0)?;
                let file_path: String = row.get(1)?;
                let scope_id_str: String = row.get(2)?;
                let failure_count: i64 = row.get(3)?;
                let consecutive_failures: i64 = row.get(4)?;
                let first_failure_str: String = row.get(5)?;
                let last_failure_str: String = row.get(6)?;
                let last_tested_str: String = row.get(7)?;
                let history_json: String = row.get(8)?;

                Ok((
                    file_id_str,
                    file_path,
                    scope_id_str,
                    failure_count,
                    consecutive_failures,
                    first_failure_str,
                    last_failure_str,
                    last_tested_str,
                    history_json,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(
                |(
                    file_id_str,
                    file_path,
                    scope_id_str,
                    failure_count,
                    consecutive_failures,
                    first_failure_str,
                    last_failure_str,
                    last_tested_str,
                    history_json,
                )| {
                    let file_id = Uuid::parse_str(&file_id_str).ok()?;
                    let scope_id = Uuid::parse_str(&scope_id_str).ok()?;
                    let first_failure_at = first_failure_str.parse().ok()?;
                    let last_failure_at = last_failure_str.parse().ok()?;
                    let last_tested_at = last_tested_str.parse().ok()?;
                    let failure_history: Vec<FailureHistoryEntry> =
                        serde_json::from_str(&history_json).ok()?;

                    Some(HighFailureFile {
                        file_id,
                        file_path,
                        scope_id,
                        failure_count: failure_count as usize,
                        consecutive_failures: consecutive_failures as usize,
                        first_failure_at,
                        last_failure_at,
                        last_tested_at,
                        failure_history,
                    })
                },
            )
            .collect();

        Ok(files)
    }

    /// Get files ordered for backtest: high-failure first, then resolved, then untested, then passing
    ///
    /// Order priority:
    /// 1. High-failure files (sorted by consecutive_failures DESC)
    /// 2. Resolved files (had failures but now passing)
    /// 3. Untested files (never been tested)
    /// 4. Passing files (tested and passed, never failed)
    pub fn get_backtest_order(
        &self,
        all_files: &[FileInfo],
        scope_id: &Uuid,
    ) -> Result<Vec<FileInfo>, HighFailureError> {
        // Get all high-failure info
        let high_failure_files = self.get_all(scope_id)?;
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
    pub fn clear_scope(&self, scope_id: &Uuid) -> Result<usize, HighFailureError> {
        let scope_str = scope_id.to_string();
        let count = self.conn.execute(
            "DELETE FROM high_failure_files WHERE scope_id = ?1",
            params![scope_str],
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_table() -> HighFailureTable {
        let conn = Connection::open_in_memory().unwrap();
        HighFailureTable::new(conn).unwrap()
    }

    #[test]
    fn test_record_failure() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        let entry = FailureHistoryEntry::new(
            1,
            1,
            FailureCategory::TypeMismatch,
            "Expected Int64, got String",
        );

        let hf = table.record_failure("/path/to/file.csv", &scope_id, entry).unwrap();

        assert_eq!(hf.file_path, "/path/to/file.csv");
        assert_eq!(hf.failure_count, 1);
        assert_eq!(hf.consecutive_failures, 1);
        assert_eq!(hf.failure_history.len(), 1);
    }

    #[test]
    fn test_multiple_failures_increment() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        // First failure
        let entry1 = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error 1");
        table.record_failure("/path/to/file.csv", &scope_id, entry1).unwrap();

        // Second failure
        let entry2 = FailureHistoryEntry::new(2, 2, FailureCategory::NullNotAllowed, "Error 2");
        let hf = table.record_failure("/path/to/file.csv", &scope_id, entry2).unwrap();

        assert_eq!(hf.failure_count, 2);
        assert_eq!(hf.consecutive_failures, 2);
        assert_eq!(hf.failure_history.len(), 2);
    }

    #[test]
    fn test_success_resets_consecutive() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        // Record failures
        let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/to/file.csv", &scope_id, entry).unwrap();

        // Record success
        table.record_success("/path/to/file.csv", &scope_id).unwrap();

        // Should have no active high-failure files
        let active = table.get_active(&scope_id).unwrap();
        assert!(active.is_empty());

        // But should still be in all (with consecutive = 0)
        let all = table.get_all(&scope_id).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].consecutive_failures, 0);
        assert_eq!(all[0].failure_count, 1); // Still tracked total failures
    }

    #[test]
    fn test_backtest_order() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        // Record some failures
        let entry1 = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).unwrap();
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).unwrap();
        table.record_failure("/path/high1.csv", &scope_id, entry1.clone()).unwrap(); // 3 consecutive

        table.record_failure("/path/high2.csv", &scope_id, entry1.clone()).unwrap(); // 1 consecutive

        table.record_failure("/path/resolved.csv", &scope_id, entry1.clone()).unwrap();
        table.record_success("/path/resolved.csv", &scope_id).unwrap(); // resolved

        // Create file list
        let files = vec![
            FileInfo::new("/path/passing.csv", 100),
            FileInfo { tested: true, ..FileInfo::new("/path/passing.csv", 100) },
            FileInfo::new("/path/untested.csv", 100),
            FileInfo::new("/path/high1.csv", 100),
            FileInfo::new("/path/high2.csv", 100),
            FileInfo::new("/path/resolved.csv", 100),
        ];

        let ordered = table.get_backtest_order(&files, &scope_id).unwrap();

        // High failure (most consecutive first)
        assert_eq!(ordered[0].path, "/path/high1.csv");
        assert_eq!(ordered[0].consecutive_failures, 3);
        assert_eq!(ordered[1].path, "/path/high2.csv");
        assert_eq!(ordered[1].consecutive_failures, 1);

        // Then resolved
        assert_eq!(ordered[2].path, "/path/resolved.csv");
        assert_eq!(ordered[2].consecutive_failures, 0);
    }

    #[test]
    fn test_clear_scope() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();
        let other_scope = Uuid::new_v4();

        let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
        table.record_failure("/path/file1.csv", &scope_id, entry.clone()).unwrap();
        table.record_failure("/path/file2.csv", &scope_id, entry.clone()).unwrap();
        table.record_failure("/path/other.csv", &other_scope, entry).unwrap();

        let cleared = table.clear_scope(&scope_id).unwrap();
        assert_eq!(cleared, 2);

        // Other scope unaffected
        let all = table.get_all(&other_scope).unwrap();
        assert_eq!(all.len(), 1);
    }
}

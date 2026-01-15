//! SQLite state database for Scout
//!
//! Scout is the File Discovery + Tagging layer.
//! All state flows through SQLite:
//! - Sources: filesystem locations to watch
//! - Tagging Rules: pattern → tag mappings
//! - Files: discovered files with their tags and status

use super::error::Result;
use super::types::{
    DbStats, ExtractionLogStatus, ExtractionStatus, Extractor, FileStatus, ScannedFile, Source,
    SourceType, TaggingRule, UpsertResult,
};
use casparian_db::{DbConfig, DbPool, create_pool};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::path::Path;

/// SQLite database schema (v2 - tag-based)
/// Note: All timestamps are stored as INTEGER (milliseconds since Unix epoch)
const SCHEMA_SQL: &str = r#"
-- Sources: filesystem locations to watch
CREATE TABLE IF NOT EXISTS scout_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_type TEXT NOT NULL,
    path TEXT NOT NULL,
    poll_interval_secs INTEGER NOT NULL DEFAULT 30,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Tagging Rules: pattern → tag mappings
CREATE TABLE IF NOT EXISTS scout_tagging_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    pattern TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Settings: key-value store for configuration
CREATE TABLE IF NOT EXISTS scout_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Schema migrations: tracks which migrations have been applied
CREATE TABLE IF NOT EXISTS schema_migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    applied_at INTEGER NOT NULL
);

-- Files: discovered files and their status
CREATE TABLE IF NOT EXISTS scout_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    path TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    content_hash TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    tag TEXT,
    tag_source TEXT,
    rule_id TEXT,
    manual_plugin TEXT,
    error TEXT,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    processed_at INTEGER,
    sentinel_job_id INTEGER,
    -- Extractor metadata (Phase 6)
    metadata_raw TEXT,                           -- JSON blob of extracted metadata
    extraction_status TEXT DEFAULT 'pending',    -- pending, extracted, timeout, crash, stale
    extracted_at INTEGER,                        -- timestamp of last extraction
    UNIQUE(source_id, path)
);

-- Folder hierarchy for O(1) TUI navigation (streaming scanner)
-- Replaces file-based FolderCache (.bin.zst files)
CREATE TABLE IF NOT EXISTS scout_folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES scout_sources(id) ON DELETE CASCADE,
    -- Prefix path, e.g., "" for root, "logs/" for /logs folder
    prefix TEXT NOT NULL,
    -- Folder or file name at this level
    name TEXT NOT NULL,
    -- Count of files in this subtree
    file_count INTEGER NOT NULL DEFAULT 0,
    -- True for folders, false for files (stored as 1/0)
    is_folder INTEGER NOT NULL,
    -- When this row was last updated
    updated_at INTEGER NOT NULL,
    UNIQUE(source_id, prefix, name)
);

-- Extractors: Python extractor registry (Phase 6)
-- Extractors are Python files that extract metadata from file paths
CREATE TABLE IF NOT EXISTS scout_extractors (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_path TEXT NOT NULL,               -- Path to .py file
    source_hash TEXT NOT NULL,               -- SHA-256 of source code
    enabled INTEGER NOT NULL DEFAULT 1,
    timeout_secs INTEGER NOT NULL DEFAULT 5, -- Per-file timeout
    consecutive_failures INTEGER DEFAULT 0,  -- For fail-fast pausing
    paused_at INTEGER,                       -- When auto-paused due to failures
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Extraction Log: execution history for auditing (Phase 6)
CREATE TABLE IF NOT EXISTS scout_extraction_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES scout_files(id),
    extractor_id TEXT NOT NULL REFERENCES scout_extractors(id),
    status TEXT NOT NULL,                    -- success, timeout, crash, error
    duration_ms INTEGER,                     -- Execution time
    error_message TEXT,                      -- Error details if failed
    metadata_snapshot TEXT,                  -- Copy of extracted metadata
    executed_at INTEGER NOT NULL
);

-- Parser Lab parsers (v6 - parser-centric)
-- NOTE: These tables are here for backward compatibility.
-- They should eventually be moved to a separate module.
CREATE TABLE IF NOT EXISTS parser_lab_parsers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    file_pattern TEXT NOT NULL DEFAULT '',
    pattern_type TEXT DEFAULT 'all',
    source_code TEXT,
    validation_status TEXT DEFAULT 'pending',
    validation_error TEXT,
    validation_output TEXT,
    last_validated_at INTEGER,
    messages_json TEXT,
    schema_json TEXT,
    sink_type TEXT DEFAULT 'parquet',
    sink_config_json TEXT,
    published_at INTEGER,
    published_plugin_id INTEGER,
    is_sample INTEGER DEFAULT 0,
    output_mode TEXT DEFAULT 'single',
    detected_topics_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Parser Lab test files
CREATE TABLE IF NOT EXISTS parser_lab_test_files (
    id TEXT PRIMARY KEY,
    parser_id TEXT NOT NULL REFERENCES parser_lab_parsers(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(parser_id, file_path)
);

-- Extraction Rules: glob pattern → field extraction + tagging
-- Created via the Glob Explorer's Rule Editing workflow
CREATE TABLE IF NOT EXISTS extraction_rules (
    id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES scout_sources(id),
    name TEXT NOT NULL,
    glob_pattern TEXT NOT NULL,
    base_tag TEXT,
    priority INTEGER NOT NULL DEFAULT 100,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT 'user',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(source_id, name)
);

-- Extraction Fields: field definitions for extraction rules
CREATE TABLE IF NOT EXISTS extraction_fields (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_value TEXT,
    pattern TEXT,
    type_hint TEXT NOT NULL DEFAULT 'string',
    normalizer TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(rule_id, field_name)
);

-- Extraction Tag Conditions: conditional tagging based on field values
CREATE TABLE IF NOT EXISTS extraction_tag_conditions (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    operator TEXT NOT NULL,
    value TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    created_at INTEGER NOT NULL
);

-- ============================================================================
-- AI Wizards Tables (Layer 2)
-- ============================================================================

-- AI Drafts: temporary artifacts awaiting approval
-- Types: 'extractor' (Pathfinder), 'parser' (Parser Lab), 'label' (Labeling), 'semantic_rule' (Semantic Path)
CREATE TABLE IF NOT EXISTS cf_ai_drafts (
    id TEXT PRIMARY KEY,
    draft_type TEXT NOT NULL,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    source_context_json TEXT,
    model_name TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    approved_at INTEGER,
    approved_by TEXT
);

-- AI Audit Log: tracks all LLM interactions for debugging and compliance
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
);

-- Signature Groups: file groups with same structure (for Labeling Wizard)
CREATE TABLE IF NOT EXISTS cf_signature_groups (
    id TEXT PRIMARY KEY,
    fingerprint_json TEXT NOT NULL,
    file_count INTEGER DEFAULT 0,
    label TEXT,
    labeled_by TEXT,
    labeled_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- AI Training Examples: approved rules for future model improvement
CREATE TABLE IF NOT EXISTS cf_ai_training_examples (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    sample_paths_json TEXT NOT NULL,
    extraction_config_json TEXT NOT NULL,
    approved_by TEXT,
    approved_at INTEGER NOT NULL,
    quality_score REAL,
    created_at INTEGER NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_files_source ON scout_files(source_id);
CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status);
CREATE INDEX IF NOT EXISTS idx_files_tag ON scout_files(tag);
CREATE INDEX IF NOT EXISTS idx_files_mtime ON scout_files(mtime);
CREATE INDEX IF NOT EXISTS idx_files_path ON scout_files(path);
CREATE INDEX IF NOT EXISTS idx_files_last_seen ON scout_files(last_seen_at);
CREATE INDEX IF NOT EXISTS idx_files_tag_source ON scout_files(tag_source);
CREATE INDEX IF NOT EXISTS idx_files_manual_plugin ON scout_files(manual_plugin);
CREATE INDEX IF NOT EXISTS idx_tagging_rules_source ON scout_tagging_rules(source_id);
CREATE INDEX IF NOT EXISTS idx_tagging_rules_priority ON scout_tagging_rules(priority DESC);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_updated ON parser_lab_parsers(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_status ON parser_lab_parsers(validation_status);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_pattern ON parser_lab_parsers(file_pattern);
CREATE INDEX IF NOT EXISTS idx_parser_lab_test_files_parser ON parser_lab_test_files(parser_id);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_source ON extraction_rules(source_id);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_pattern ON extraction_rules(glob_pattern);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_enabled ON extraction_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_extraction_fields_rule ON extraction_fields(rule_id);
CREATE INDEX IF NOT EXISTS idx_extraction_tag_conditions_rule ON extraction_tag_conditions(rule_id);
CREATE INDEX IF NOT EXISTS idx_scout_folders_lookup ON scout_folders(source_id, prefix);

-- Extractor indexes (Phase 6)
CREATE INDEX IF NOT EXISTS idx_files_extraction_status ON scout_files(extraction_status);
CREATE INDEX IF NOT EXISTS idx_files_extracted_at ON scout_files(extracted_at);
CREATE INDEX IF NOT EXISTS idx_extractors_enabled ON scout_extractors(enabled);
CREATE INDEX IF NOT EXISTS idx_extractors_paused ON scout_extractors(paused_at);
CREATE INDEX IF NOT EXISTS idx_extraction_log_file ON scout_extraction_log(file_id);
CREATE INDEX IF NOT EXISTS idx_extraction_log_extractor ON scout_extraction_log(extractor_id);
CREATE INDEX IF NOT EXISTS idx_extraction_log_executed ON scout_extraction_log(executed_at);

-- AI Wizards indexes
CREATE INDEX IF NOT EXISTS idx_ai_drafts_status ON cf_ai_drafts(status);
CREATE INDEX IF NOT EXISTS idx_ai_drafts_type ON cf_ai_drafts(draft_type);
CREATE INDEX IF NOT EXISTS idx_ai_drafts_expires ON cf_ai_drafts(expires_at);
CREATE INDEX IF NOT EXISTS idx_ai_audit_wizard ON cf_ai_audit_log(wizard_type);
CREATE INDEX IF NOT EXISTS idx_ai_audit_created ON cf_ai_audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_ai_audit_status ON cf_ai_audit_log(status);
CREATE INDEX IF NOT EXISTS idx_sig_groups_label ON cf_signature_groups(label);
CREATE INDEX IF NOT EXISTS idx_training_rule ON cf_ai_training_examples(rule_id);
"#;

/// Convert milliseconds since epoch to DateTime
fn millis_to_datetime(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap_or_else(Utc::now)
}

/// Get current time as milliseconds since epoch
fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// Database wrapper with connection pool.
#[derive(Clone)]
pub struct Database {
    pool: DbPool,
}

// Many methods are used in tests and will be used for processing integration
#[allow(dead_code)]
impl Database {
    /// Open or create a database at the given path.
    ///
    /// Opens a SQLite database with optimizations (WAL mode, synchronous=NORMAL).
    /// Optimizations are applied by casparian_db::create_pool.
    pub async fn open(path: &Path) -> Result<Self> {
        let config = DbConfig::sqlite(&path.display().to_string());
        let pool = create_pool(config).await?;

        // Create schema
        sqlx::query(SCHEMA_SQL).execute(&pool).await?;

        Ok(Self { pool })
    }

    /// Create an in-memory database (for testing).
    pub async fn open_in_memory() -> Result<Self> {
        let config = DbConfig::sqlite_memory();
        let pool = create_pool(config).await?;

        // Create schema
        sqlx::query(SCHEMA_SQL).execute(&pool).await?;

        Ok(Self { pool })
    }

    /// Get the underlying pool (for sharing with other code).
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    // ========================================================================
    // Source Operations
    // ========================================================================

    /// Insert or update a source
    pub async fn upsert_source(&self, source: &Source) -> Result<()> {
        let source_type_json = serde_json::to_string(&source.source_type)?;
        let now = now_millis();

        sqlx::query(
            r#"
            INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_type = excluded.source_type,
                path = excluded.path,
                poll_interval_secs = excluded.poll_interval_secs,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&source.id)
        .bind(&source.name)
        .bind(&source_type_json)
        .bind(&source.path)
        .bind(source.poll_interval_secs as i64)
        .bind(source.enabled as i32)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a source by ID
    pub async fn get_source(&self, id: &str) -> Result<Option<Source>> {
        let row = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by name
    pub async fn get_source_by_name(&self, name: &str) -> Result<Option<Source>> {
        let row = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by path
    pub async fn get_source_by_path(&self, path: &str) -> Result<Option<Source>> {
        let row = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// List all sources
    pub async fn list_sources(&self) -> Result<Vec<Source>> {
        let rows = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// List enabled sources
    pub async fn list_enabled_sources(&self) -> Result<Vec<Source>> {
        let rows = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE enabled = 1 ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// Delete a source and all associated data
    pub async fn delete_source(&self, id: &str) -> Result<bool> {
        // Delete associated files and tagging rules first
        sqlx::query("DELETE FROM scout_files WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM scout_tagging_rules WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let result = sqlx::query("DELETE FROM scout_sources WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    fn row_to_source(row: &sqlx::sqlite::SqliteRow) -> Result<Source> {
        let source_type_json: String = row.get(2);
        let source_type: SourceType = serde_json::from_str(&source_type_json)?;
        let poll_interval: i64 = row.get(4);
        let enabled: i32 = row.get(5);

        Ok(Source {
            id: row.get(0),
            name: row.get(1),
            source_type,
            path: row.get(3),
            poll_interval_secs: poll_interval as u64,
            enabled: enabled != 0,
        })
    }

    // ========================================================================
    // Tagging Rule Operations
    // ========================================================================

    /// Insert or update a tagging rule
    pub async fn upsert_tagging_rule(&self, rule: &TaggingRule) -> Result<()> {
        let now = now_millis();

        sqlx::query(
            r#"
            INSERT INTO scout_tagging_rules (id, name, source_id, pattern, tag, priority, enabled, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_id = excluded.source_id,
                pattern = excluded.pattern,
                tag = excluded.tag,
                priority = excluded.priority,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.name)
        .bind(&rule.source_id)
        .bind(&rule.pattern)
        .bind(&rule.tag)
        .bind(rule.priority)
        .bind(rule.enabled as i32)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a tagging rule by ID
    pub async fn get_tagging_rule(&self, id: &str) -> Result<Option<TaggingRule>> {
        let row = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_tagging_rule(&row)?)),
            None => Ok(None),
        }
    }

    /// List all tagging rules
    pub async fn list_tagging_rules(&self) -> Result<Vec<TaggingRule>> {
        let rows = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules ORDER BY priority DESC, name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// List enabled tagging rules for a source (ordered by priority)
    pub async fn list_tagging_rules_for_source(&self, source_id: &str) -> Result<Vec<TaggingRule>> {
        let rows = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE source_id = ? AND enabled = 1 ORDER BY priority DESC, name",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// Delete a tagging rule
    pub async fn delete_tagging_rule(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM scout_tagging_rules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    fn row_to_tagging_rule(row: &sqlx::sqlite::SqliteRow) -> Result<TaggingRule> {
        let enabled: i32 = row.get(6);
        Ok(TaggingRule {
            id: row.get(0),
            name: row.get(1),
            source_id: row.get(2),
            pattern: row.get(3),
            tag: row.get(4),
            priority: row.get(5),
            enabled: enabled != 0,
        })
    }

    // ========================================================================
    // File Operations
    // ========================================================================

    /// Upsert a scanned file
    ///
    /// If the file exists and mtime/size changed, resets status to pending.
    pub async fn upsert_file(&self, file: &ScannedFile) -> Result<UpsertResult> {
        // Check if file exists
        let existing: Option<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT id, size, mtime, status FROM scout_files WHERE source_id = ? AND path = ?",
        )
        .bind(file.source_id.as_ref())  // Arc<str> -> &str for sqlx
        .bind(&file.path)
        .fetch_optional(&self.pool)
        .await?;

        let now = now_millis();
        match existing {
            None => {
                // New file
                let result = sqlx::query(
                    r#"
                    INSERT INTO scout_files (source_id, path, rel_path, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                    VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)
                    "#,
                )
                .bind(file.source_id.as_ref())  // Arc<str> -> &str for sqlx
                .bind(&file.path)
                .bind(&file.rel_path)
                .bind(file.size as i64)
                .bind(file.mtime)
                .bind(&file.content_hash)
                .bind(&file.tag)
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;

                Ok(UpsertResult {
                    id: result.last_insert_rowid(),
                    is_new: true,
                    is_changed: false,
                })
            }
            Some((id, old_size, old_mtime, _status)) => {
                let changed = file.size as i64 != old_size || file.mtime != old_mtime;

                if changed {
                    // File changed - reset to pending, clear tag
                    sqlx::query(
                        r#"
                        UPDATE scout_files SET
                            size = ?,
                            mtime = ?,
                            content_hash = ?,
                            status = 'pending',
                            tag = NULL,
                            error = NULL,
                            sentinel_job_id = NULL,
                            last_seen_at = ?
                        WHERE id = ?
                        "#,
                    )
                    .bind(file.size as i64)
                    .bind(file.mtime)
                    .bind(&file.content_hash)
                    .bind(now)
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                } else {
                    // Just update last_seen_at
                    sqlx::query("UPDATE scout_files SET last_seen_at = ? WHERE id = ?")
                        .bind(now)
                        .bind(id)
                        .execute(&self.pool)
                        .await?;
                }

                Ok(UpsertResult {
                    id,
                    is_new: false,
                    is_changed: changed,
                })
            }
        }
    }

    /// Get a file by ID
    pub async fn get_file(&self, id: i64) -> Result<Option<ScannedFile>> {
        let row = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a file by path
    pub async fn get_file_by_path(&self, source_id: &str, path: &str) -> Result<Option<ScannedFile>> {
        let row = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE source_id = ? AND path = ?
            "#,
        )
        .bind(source_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// List all files for a source (regardless of status)
    pub async fn list_files_by_source(&self, source_id: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE source_id = ?
            ORDER BY mtime DESC
            LIMIT ?
            "#,
        )
        .bind(source_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files with a specific status
    pub async fn list_files_by_status(&self, status: FileStatus, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE status = ?
            ORDER BY mtime DESC
            LIMIT ?
            "#,
        )
        .bind(status.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List pending (untagged) files for a source
    pub async fn list_pending_files(&self, source_id: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Pending, limit).await
    }

    /// List tagged files ready for processing
    pub async fn list_tagged_files(&self, source_id: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Tagged, limit).await
    }

    /// List untagged files (files that have no tag assigned)
    pub async fn list_untagged_files(&self, source_id: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE source_id = ? AND tag IS NULL AND status = 'pending'
            ORDER BY mtime DESC
            LIMIT ?
            "#,
        )
        .bind(source_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files by tag
    pub async fn list_files_by_tag(&self, tag: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE tag = ?
            ORDER BY mtime DESC
            LIMIT ?
            "#,
        )
        .bind(tag)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files for a source with specific status
    pub async fn list_files_by_source_and_status(
        &self,
        source_id: &str,
        status: FileStatus,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                   first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files WHERE source_id = ? AND status = ?
            ORDER BY mtime DESC
            LIMIT ?
            "#,
        )
        .bind(source_id)
        .bind(status.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// Tag a file manually (sets tag_source = 'manual')
    pub async fn tag_file(&self, id: i64, tag: &str) -> Result<()> {
        sqlx::query(
            "UPDATE scout_files SET tag = ?, tag_source = 'manual', rule_id = NULL, status = 'tagged' WHERE id = ?",
        )
        .bind(tag)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Tag multiple files manually (sets tag_source = 'manual')
    pub async fn tag_files(&self, ids: &[i64], tag: &str) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut total = 0u64;
        for id in ids {
            let result = sqlx::query(
                "UPDATE scout_files SET tag = ?, tag_source = 'manual', rule_id = NULL, status = 'tagged' WHERE id = ?",
            )
            .bind(tag)
            .bind(id)
            .execute(&self.pool)
            .await?;
            total += result.rows_affected();
        }
        Ok(total)
    }

    /// Tag a file via a tagging rule (sets tag_source = 'rule')
    pub async fn tag_file_by_rule(&self, id: i64, tag: &str, rule_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE scout_files SET tag = ?, tag_source = 'rule', rule_id = ?, status = 'tagged' WHERE id = ?",
        )
        .bind(tag)
        .bind(rule_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update file status
    pub async fn update_file_status(&self, id: i64, status: FileStatus, error: Option<&str>) -> Result<()> {
        if status == FileStatus::Processed {
            sqlx::query("UPDATE scout_files SET status = ?, error = ?, processed_at = ? WHERE id = ?")
                .bind(status.as_str())
                .bind(error)
                .bind(now_millis())
                .bind(id)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("UPDATE scout_files SET status = ?, error = ? WHERE id = ?")
                .bind(status.as_str())
                .bind(error)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Untag a file (clear tag, tag_source, rule_id, manual_plugin and reset to pending)
    pub async fn untag_file(&self, id: i64) -> Result<()> {
        sqlx::query(
            "UPDATE scout_files SET tag = NULL, tag_source = NULL, rule_id = NULL, \
             manual_plugin = NULL, status = 'pending', sentinel_job_id = NULL WHERE id = ?"
        )
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Mark file as queued for processing
    pub async fn mark_file_queued(&self, id: i64, sentinel_job_id: i64) -> Result<()> {
        sqlx::query("UPDATE scout_files SET status = 'queued', sentinel_job_id = ? WHERE id = ?")
            .bind(sentinel_job_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Mark files as deleted if not seen recently
    pub async fn mark_deleted_files(&self, source_id: &str, seen_before: DateTime<Utc>) -> Result<u64> {
        let seen_before_millis = seen_before.timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE scout_files SET status = 'deleted'
            WHERE source_id = ? AND last_seen_at < ? AND status != 'deleted'
            "#,
        )
        .bind(source_id)
        .bind(seen_before_millis)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    fn row_to_file(row: &sqlx::sqlite::SqliteRow) -> Result<ScannedFile> {
        use super::types::ExtractionStatus;

        let status_str: String = row.get(7);
        let status = FileStatus::parse(&status_str).unwrap_or(FileStatus::Pending);

        let first_seen_millis: i64 = row.get(13);
        let last_seen_millis: i64 = row.get(14);
        let processed_at_millis: Option<i64> = row.get(15);

        // Parse extraction status (Phase 6)
        let extraction_status_str: Option<String> = row.get(18);
        let extraction_status = extraction_status_str
            .as_deref()
            .and_then(ExtractionStatus::parse)
            .unwrap_or(ExtractionStatus::Pending);
        let extracted_at_millis: Option<i64> = row.get(19);

        Ok(ScannedFile {
            id: Some(row.get(0)),
            source_id: std::sync::Arc::from(row.get::<String, _>(1)),  // String -> Arc<str>
            path: row.get(2),
            rel_path: row.get(3),
            size: row.get::<i64, _>(4) as u64,
            mtime: row.get(5),
            content_hash: row.get(6),
            status,
            tag: row.get(8),
            tag_source: row.get(9),
            rule_id: row.get(10),
            manual_plugin: row.get(11),
            error: row.get(12),
            first_seen_at: millis_to_datetime(first_seen_millis),
            last_seen_at: millis_to_datetime(last_seen_millis),
            processed_at: processed_at_millis.map(millis_to_datetime),
            sentinel_job_id: row.get(16),
            // Extractor metadata fields (Phase 6)
            metadata_raw: row.get(17),
            extraction_status,
            extracted_at: extracted_at_millis.map(millis_to_datetime),
        })
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DbStats> {
        let row: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total_files,
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as files_pending,
                SUM(CASE WHEN status = 'tagged' THEN 1 ELSE 0 END) as files_tagged,
                SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END) as files_queued,
                SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) as files_processing,
                SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END) as files_processed,
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as files_failed,
                COALESCE(SUM(CASE WHEN status = 'pending' THEN size ELSE 0 END), 0) as bytes_pending,
                COALESCE(SUM(CASE WHEN status = 'processed' THEN size ELSE 0 END), 0) as bytes_processed
            FROM scout_files
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0));

        let total_sources: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scout_sources")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));

        let total_tagging_rules: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scout_tagging_rules")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));

        Ok(DbStats {
            total_sources: total_sources.0 as u64,
            total_tagging_rules: total_tagging_rules.0 as u64,
            total_files: row.0 as u64,
            files_pending: row.1 as u64,
            files_tagged: row.2 as u64,
            files_queued: row.3 as u64,
            files_processing: row.4 as u64,
            files_processed: row.5 as u64,
            files_failed: row.6 as u64,
            bytes_pending: row.7 as u64,
            bytes_processed: row.8 as u64,
        })
    }

    // ========================================================================
    // Glob Explorer Operations (Hierarchical Browsing)
    // ========================================================================

    /// Get folder counts at a specific depth for hierarchical browsing.
    ///
    /// Returns folders (subdirectories) with file counts, plus leaf files at current level.
    /// This is designed for fast navigation of large sources (400k+ files).
    ///
    /// # Arguments
    /// * `source_id` - The source to query
    /// * `prefix` - Path prefix (empty for root, "folder/" for subfolder)
    /// * `glob_pattern` - Optional glob pattern filter (e.g., "*.csv")
    ///
    /// # Returns
    /// Vec of (folder_name, file_count, is_file) tuples
    pub async fn get_folder_counts(
        &self,
        source_id: &str,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<Vec<(String, i64, bool)>> {
        // Query extracts the immediate child folder or filename from rel_path
        // For paths like "a/b/c.csv" with prefix "a/":
        //   - Extracts "b" (folder containing more files)
        // For paths like "a/file.csv" with prefix "a/":
        //   - Extracts "file.csv" (leaf file)
        let prefix_len = prefix.len() as i32;

        let query = if glob_pattern.is_some() {
            r#"
            SELECT
                CASE
                    WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') > 0
                    THEN SUBSTR(rel_path, ? + 1, INSTR(SUBSTR(rel_path, ? + 1), '/') - 1)
                    ELSE SUBSTR(rel_path, ? + 1)
                END AS item_name,
                COUNT(*) as file_count,
                MAX(CASE WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') = 0 THEN 1 ELSE 0 END) as is_file
            FROM scout_files
            WHERE source_id = ?
              AND rel_path LIKE ? || '%'
              AND rel_path GLOB ?
              AND LENGTH(rel_path) > ?
            GROUP BY item_name
            ORDER BY file_count DESC
            LIMIT 100
            "#
        } else {
            r#"
            SELECT
                CASE
                    WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') > 0
                    THEN SUBSTR(rel_path, ? + 1, INSTR(SUBSTR(rel_path, ? + 1), '/') - 1)
                    ELSE SUBSTR(rel_path, ? + 1)
                END AS item_name,
                COUNT(*) as file_count,
                MAX(CASE WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') = 0 THEN 1 ELSE 0 END) as is_file
            FROM scout_files
            WHERE source_id = ?
              AND rel_path LIKE ? || '%'
              AND LENGTH(rel_path) > ?
            GROUP BY item_name
            ORDER BY file_count DESC
            LIMIT 100
            "#
        };

        let rows: Vec<(String, i64, i32)> = if let Some(pattern) = glob_pattern {
            sqlx::query_as(query)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(source_id)
                .bind(prefix)
                .bind(pattern)
                .bind(prefix_len)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as(query)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(source_id)
                .bind(prefix)
                .bind(prefix_len)
                .fetch_all(&self.pool)
                .await?
        };

        Ok(rows
            .into_iter()
            .filter(|(name, _, _)| !name.is_empty())
            .map(|(name, count, is_file)| (name, count, is_file != 0))
            .collect())
    }

    /// Get sampled preview files for a prefix and optional pattern.
    ///
    /// Returns up to `limit` files matching the criteria, for display in preview pane.
    pub async fn get_preview_files(
        &self,
        source_id: &str,
        prefix: &str,
        glob_pattern: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, i64, i64)>> {
        // Returns (rel_path, size, mtime)
        let rows: Vec<(String, i64, i64)> = if let Some(pattern) = glob_pattern {
            sqlx::query_as(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                  AND rel_path GLOB ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
            )
            .bind(source_id)
            .bind(prefix)
            .bind(pattern)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                ORDER BY mtime DESC
                LIMIT ?
                "#,
            )
            .bind(source_id)
            .bind(prefix)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows)
    }

    /// Get total file count for a prefix and optional pattern.
    pub async fn get_file_count_for_prefix(
        &self,
        source_id: &str,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<i64> {
        let count: (i64,) = if let Some(pattern) = glob_pattern {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                  AND rel_path GLOB ?
                "#,
            )
            .bind(source_id)
            .bind(prefix)
            .bind(pattern)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                "#,
            )
            .bind(source_id)
            .bind(prefix)
            .fetch_one(&self.pool)
            .await?
        };

        Ok(count.0)
    }

    // ========================================================================
    // Settings Operations
    // ========================================================================

    /// Set a setting value
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO scout_settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get a setting value
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let result: Option<(String,)> =
            sqlx::query_as("SELECT value FROM scout_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(result.map(|(v,)| v))
    }

    // ========================================================================
    // Folder Operations (Streaming Scanner)
    // ========================================================================

    /// Get folder children at a given prefix for TUI drill-down
    /// Returns folders first (sorted), then files (sorted)
    pub async fn get_folder_children(
        &self,
        source_id: &str,
        prefix: &str,
    ) -> Result<Vec<FolderEntry>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            r#"
            SELECT name, file_count, is_folder
            FROM scout_folders
            WHERE source_id = ? AND prefix = ?
            ORDER BY is_folder DESC, name ASC
            "#,
        )
        .bind(source_id)
        .bind(prefix)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(name, file_count, is_folder)| FolderEntry {
                name,
                file_count,
                is_folder: is_folder != 0,
            })
            .collect())
    }

    /// Batch upsert folder counts during scan
    /// Called by persist_task with aggregated deltas
    pub async fn batch_upsert_folder_counts(
        &self,
        source_id: &str,
        deltas: &std::collections::HashMap<(String, String), (i64, bool)>,
    ) -> Result<()> {
        let now = now_millis();

        for ((prefix, name), (count, is_folder)) in deltas {
            sqlx::query(
                r#"
                INSERT INTO scout_folders (source_id, prefix, name, file_count, is_folder, updated_at)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(source_id, prefix, name) DO UPDATE
                SET file_count = file_count + excluded.file_count,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(source_id)
            .bind(prefix)
            .bind(name)
            .bind(*count)
            .bind(*is_folder as i32)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Decrement folder counts when files are deleted
    /// Called during rescan cleanup
    pub async fn decrement_folder_counts(
        &self,
        source_id: &str,
        deltas: &std::collections::HashMap<(String, String), (i64, bool)>,
    ) -> Result<()> {
        let now = now_millis();

        for ((prefix, name), (delta, _)) in deltas {
            sqlx::query(
                r#"
                UPDATE scout_folders
                SET file_count = file_count + ?,
                    updated_at = ?
                WHERE source_id = ? AND prefix = ? AND name = ?
                "#,
            )
            .bind(*delta) // negative for decrements
            .bind(now)
            .bind(source_id)
            .bind(prefix)
            .bind(name)
            .execute(&self.pool)
            .await?;
        }

        // Clean up zero-count folders
        sqlx::query(
            r#"
            DELETE FROM scout_folders
            WHERE source_id = ? AND file_count <= 0
            "#,
        )
        .bind(source_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Clear all folder entries for a source (used before rescan)
    pub async fn clear_folder_cache(&self, source_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM scout_folders WHERE source_id = ?")
            .bind(source_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Check if folder data exists for a source
    pub async fn has_folder_data(&self, source_id: &str) -> Result<bool> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM scout_folders WHERE source_id = ? LIMIT 1",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0 > 0)
    }

    // ========================================================================
    // Extractor Operations
    // ========================================================================

    /// Upsert an extractor
    pub async fn upsert_extractor(&self, extractor: &Extractor) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let paused_at = extractor.paused_at.map(|dt| dt.timestamp_millis());

        sqlx::query(
            r#"
            INSERT INTO scout_extractors (id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_path = excluded.source_path,
                source_hash = excluded.source_hash,
                enabled = excluded.enabled,
                timeout_secs = excluded.timeout_secs,
                consecutive_failures = excluded.consecutive_failures,
                paused_at = excluded.paused_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&extractor.id)
        .bind(&extractor.name)
        .bind(&extractor.source_path)
        .bind(&extractor.source_hash)
        .bind(extractor.enabled)
        .bind(extractor.timeout_secs as i64)
        .bind(extractor.consecutive_failures as i64)
        .bind(paused_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get an extractor by ID
    pub async fn get_extractor(&self, id: &str) -> Result<Option<Extractor>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
            FROM scout_extractors WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| row_to_extractor(&r)))
    }

    /// Get all enabled, non-paused extractors
    pub async fn get_enabled_extractors(&self) -> Result<Vec<Extractor>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
            FROM scout_extractors
            WHERE enabled = 1 AND paused_at IS NULL
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_extractor).collect())
    }

    /// List all extractors
    pub async fn list_extractors(&self) -> Result<Vec<Extractor>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
            FROM scout_extractors
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_extractor).collect())
    }

    /// Pause an extractor (set paused_at to now)
    pub async fn pause_extractor(&self, id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        sqlx::query("UPDATE scout_extractors SET paused_at = ?, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Resume a paused extractor (clear paused_at)
    pub async fn resume_extractor(&self, id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        sqlx::query("UPDATE scout_extractors SET paused_at = NULL, consecutive_failures = 0, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Update extractor consecutive failure count
    pub async fn update_extractor_consecutive_failures(
        &self,
        id: &str,
        failures: u32,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "UPDATE scout_extractors SET consecutive_failures = ?, updated_at = ? WHERE id = ?",
        )
        .bind(failures as i64)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete an extractor
    pub async fn delete_extractor(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM scout_extractors WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get files pending extraction (extraction_status = 'pending')
    pub async fn get_files_pending_extraction(&self) -> Result<Vec<ScannedFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_id, path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error, first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
            FROM scout_files
            WHERE extraction_status = 'pending'
            ORDER BY first_seen_at
            LIMIT 1000
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Self::row_to_file).filter_map(|r| r.ok()).collect())
    }

    /// Log an extraction attempt
    pub async fn log_extraction(
        &self,
        file_id: i64,
        extractor_id: &str,
        status: ExtractionLogStatus,
        duration_ms: Option<u64>,
        error_message: Option<&str>,
        metadata_snapshot: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO scout_extraction_log (file_id, extractor_id, status, duration_ms, error_message, metadata_snapshot, executed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(file_id)
        .bind(extractor_id)
        .bind(status.as_str())
        .bind(duration_ms.map(|d| d as i64))
        .bind(error_message)
        .bind(metadata_snapshot)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update file extraction metadata and status
    pub async fn update_file_extraction(
        &self,
        file_id: i64,
        metadata_raw: &str,
        status: ExtractionStatus,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE scout_files
            SET metadata_raw = ?, extraction_status = ?, extracted_at = ?, last_seen_at = ?
            WHERE id = ?
            "#,
        )
        .bind(metadata_raw)
        .bind(status.as_str())
        .bind(now)
        .bind(now)
        .bind(file_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark extraction as stale for files with a given extractor
    pub async fn mark_extractions_stale(&self, extractor_id: &str) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE scout_files
            SET extraction_status = 'stale'
            WHERE id IN (
                SELECT DISTINCT file_id FROM scout_extraction_log WHERE extractor_id = ?
            )
            AND extraction_status = 'extracted'
            "#,
        )
        .bind(extractor_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

/// Entry in folder hierarchy (from scout_folders table)
#[derive(Debug, Clone)]
pub struct FolderEntry {
    pub name: String,
    pub file_count: i64,
    pub is_folder: bool,
}

/// Helper function to convert a database row to an Extractor
fn row_to_extractor(row: &sqlx::sqlite::SqliteRow) -> Extractor {
    let paused_at_millis: Option<i64> = row.get(7);
    let created_at_millis: i64 = row.get(8);
    let updated_at_millis: i64 = row.get(9);

    Extractor {
        id: row.get(0),
        name: row.get(1),
        source_path: row.get(2),
        source_hash: row.get(3),
        enabled: row.get(4),
        timeout_secs: row.get::<i64, _>(5) as u32,
        consecutive_failures: row.get::<i64, _>(6) as u32,
        paused_at: paused_at_millis.map(millis_to_datetime),
        created_at: millis_to_datetime(created_at_millis),
        updated_at: millis_to_datetime(updated_at_millis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_db() -> Database {
        Database::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn test_source_crud() {
        let db = create_test_db().await;

        let source = Source {
            id: "src-1".to_string(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };

        db.upsert_source(&source).await.unwrap();
        let fetched = db.get_source("src-1").await.unwrap().unwrap();
        assert_eq!(fetched.name, "Test Source");
        assert_eq!(fetched.path, "/data");

        let sources = db.list_sources().await.unwrap();
        assert_eq!(sources.len(), 1);

        assert!(db.delete_source("src-1").await.unwrap());
        assert!(db.get_source("src-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_tagging_rule_crud() {
        let db = create_test_db().await;

        let source = Source {
            id: "src-1".to_string(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

        let rule = TaggingRule {
            id: "rule-1".to_string(),
            name: "CSV Files".to_string(),
            source_id: "src-1".to_string(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };

        db.upsert_tagging_rule(&rule).await.unwrap();
        let fetched = db.get_tagging_rule("rule-1").await.unwrap().unwrap();
        assert_eq!(fetched.tag, "csv_data");
        assert_eq!(fetched.priority, 10);

        let rules = db.list_tagging_rules_for_source("src-1").await.unwrap();
        assert_eq!(rules.len(), 1);

        assert!(db.delete_tagging_rule("rule-1").await.unwrap());
        assert!(db.get_tagging_rule("rule-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_file_tagging() {
        let db = create_test_db().await;

        let source = Source {
            id: "src-1".to_string(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).await.unwrap();

        let file = ScannedFile::new("src-1", "/data/test.csv", "test.csv", 1000, 12345);
        let result = db.upsert_file(&file).await.unwrap();

        // File starts untagged
        let fetched = db.get_file(result.id).await.unwrap().unwrap();
        assert!(fetched.tag.is_none());
        assert_eq!(fetched.status, FileStatus::Pending);

        // Tag the file
        db.tag_file(result.id, "csv_data").await.unwrap();
        let fetched = db.get_file(result.id).await.unwrap().unwrap();
        assert_eq!(fetched.tag, Some("csv_data".to_string()));
        assert_eq!(fetched.status, FileStatus::Tagged);

        // List by tag
        let tagged = db.list_files_by_tag("csv_data", 10).await.unwrap();
        assert_eq!(tagged.len(), 1);
    }
}

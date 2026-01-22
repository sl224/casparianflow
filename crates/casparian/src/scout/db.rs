//! State database for Scout
//!
//! Scout is the File Discovery + Tagging layer.
//! All state flows through the configured backend:
//! - Sources: filesystem locations to watch
//! - Tagging Rules: pattern → tag mappings
//! - Files: discovered files with their tags and status

use super::error::{Result, ScoutError};
use super::types::{
    BatchUpsertResult, DbStats, ExtractionLogStatus, ExtractionStatus, Extractor, FileStatus,
    ParserValidationStatus, ScannedFile, Source, SourceId, SourceType, TagSource, TaggingRule,
    TaggingRuleId, UpsertResult,
};
use crate::ai::types::DraftStatus;
use casparian_db::{DbConnection, DbValue};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

/// Database schema (v2 - tag-based)
/// Note: All timestamps are stored as INTEGER (milliseconds since Unix epoch)
const SCHEMA_SQL_TEMPLATE: &str = r#"
-- Sources: filesystem locations to watch
CREATE TABLE IF NOT EXISTS scout_sources (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_type TEXT NOT NULL,
    path TEXT NOT NULL,
    poll_interval_secs INTEGER NOT NULL DEFAULT 30,
    enabled INTEGER NOT NULL DEFAULT 1,
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Tagging Rules: pattern → tag mappings
CREATE TABLE IF NOT EXISTS scout_tagging_rules (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_id BIGINT NOT NULL REFERENCES scout_sources(id),
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
    source_id BIGINT NOT NULL REFERENCES scout_sources(id),
    path TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    parent_path TEXT NOT NULL DEFAULT '',    -- directory containing this file (for O(1) folder nav)
    name TEXT NOT NULL DEFAULT '',           -- filename only (basename of rel_path)
    extension TEXT,                          -- lowercase file extension (e.g., "csv", "json")
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    content_hash TEXT,
    status TEXT NOT NULL DEFAULT '__FILE_STATUS_DEFAULT__'
        CHECK (status IN (__FILE_STATUS_VALUES__)),
    tag TEXT,
    tag_source TEXT,
    rule_id BIGINT,
    manual_plugin TEXT,
    error TEXT,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    processed_at INTEGER,
    sentinel_job_id INTEGER,
    -- Extractor metadata (Phase 6)
    metadata_raw TEXT,                           -- JSON blob of extracted metadata
    extraction_status TEXT DEFAULT '__EXTRACTION_STATUS_DEFAULT__'     -- pending, extracted, timeout, crash, stale, error
        CHECK (extraction_status IN (__EXTRACTION_STATUS_VALUES__)),
    extracted_at INTEGER,                        -- timestamp of last extraction
    UNIQUE(source_id, path)
);

-- Folder hierarchy for O(1) TUI navigation (streaming scanner)
-- Replaces file-based FolderCache (.bin.zst files)
CREATE TABLE IF NOT EXISTS scout_folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id BIGINT NOT NULL REFERENCES scout_sources(id) ON DELETE CASCADE,
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
    status TEXT NOT NULL                     -- success, timeout, crash, error
        CHECK (status IN (__EXTRACTION_LOG_STATUS_VALUES__)),
    duration_ms INTEGER,                     -- Execution time
    error_message TEXT,                      -- Error details if failed
    metadata_snapshot TEXT,                  -- Copy of extracted metadata
    executed_at INTEGER NOT NULL
);

-- Parser Lab parsers (v6 - parser-centric)
-- NOTE: These tables should eventually be moved to a separate module.
CREATE TABLE IF NOT EXISTS parser_lab_parsers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    file_pattern TEXT NOT NULL DEFAULT '',
    pattern_type TEXT DEFAULT 'all',
    source_code TEXT,
    validation_status TEXT DEFAULT '__PARSER_VALIDATION_STATUS_DEFAULT__'
        CHECK (validation_status IN (__PARSER_VALIDATION_STATUS_VALUES__)),
    validation_error TEXT,
    validation_output TEXT,
    last_validated_at INTEGER,
    messages_json TEXT,
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
    source_id BIGINT REFERENCES scout_sources(id),
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
    status TEXT NOT NULL DEFAULT '__DRAFT_STATUS_DEFAULT__',
    source_context_json TEXT,
    model_name TEXT,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    approved_at BIGINT,
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
    created_at BIGINT NOT NULL
);

-- Signature Groups: file groups with same structure (for Labeling Wizard)
CREATE TABLE IF NOT EXISTS cf_signature_groups (
    id TEXT PRIMARY KEY,
    fingerprint_json TEXT NOT NULL,
    file_count INTEGER DEFAULT 0,
    label TEXT,
    labeled_by TEXT,
    labeled_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- AI Training Examples: approved rules for future model improvement
CREATE TABLE IF NOT EXISTS cf_ai_training_examples (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    sample_paths_json TEXT NOT NULL,
    extraction_config_json TEXT NOT NULL,
    approved_by TEXT,
    approved_at BIGINT NOT NULL,
    quality_score REAL,
    created_at BIGINT NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_files_source ON scout_files(source_id);
CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status);
CREATE INDEX IF NOT EXISTS idx_files_tag ON scout_files(tag);
CREATE INDEX IF NOT EXISTS idx_files_extension ON scout_files(source_id, extension);
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

-- O(1) folder navigation index: lookup files by parent directory
CREATE INDEX IF NOT EXISTS idx_files_parent_path ON scout_files(source_id, parent_path);

-- Composite indexes for Rule Builder queries (critical for large sources)
-- Used by: load_scout_files() ORDER BY rel_path
CREATE INDEX IF NOT EXISTS idx_files_source_relpath ON scout_files(source_id, rel_path);
-- Used by: tag count queries GROUP BY tag
CREATE INDEX IF NOT EXISTS idx_files_source_tag ON scout_files(source_id, tag);

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

fn schema_sql_template() -> String {
    let file_status_values = FileStatus::ALL
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let extraction_status_values = ExtractionStatus::ALL
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let extraction_log_status_values = ExtractionLogStatus::ALL
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let parser_validation_status_values = ParserValidationStatus::ALL
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut base = SCHEMA_SQL_TEMPLATE.to_string();
    base = base.replace("__FILE_STATUS_DEFAULT__", FileStatus::Pending.as_str());
    base = base.replace("__FILE_STATUS_VALUES__", &file_status_values);
    base = base.replace(
        "__EXTRACTION_STATUS_DEFAULT__",
        ExtractionStatus::Pending.as_str(),
    );
    base = base.replace("__EXTRACTION_STATUS_VALUES__", &extraction_status_values);
    base = base.replace("__DRAFT_STATUS_DEFAULT__", DraftStatus::Pending.as_str());
    base = base.replace(
        "__EXTRACTION_LOG_STATUS_VALUES__",
        &extraction_log_status_values,
    );
    base = base.replace(
        "__PARSER_VALIDATION_STATUS_DEFAULT__",
        ParserValidationStatus::Pending.as_str(),
    );
    base = base.replace(
        "__PARSER_VALIDATION_STATUS_VALUES__",
        &parser_validation_status_values,
    );
    base
}

fn schema_sql(is_duckdb: bool) -> String {
    if !is_duckdb {
        return schema_sql_template();
    }

    // DuckDB doesn't accept SQLite's AUTOINCREMENT or FK/UNIQUE constraints in DDL.
    let mut base = schema_sql_template();

    let fk_tokens = [
        " REFERENCES scout_sources(id) ON DELETE CASCADE",
        " REFERENCES scout_sources(id)",
        " REFERENCES scout_files(id)",
        " REFERENCES scout_extractors(id)",
        " REFERENCES parser_lab_parsers(id) ON DELETE CASCADE",
        " REFERENCES extraction_rules(id) ON DELETE CASCADE",
    ];
    for token in fk_tokens {
        base = base.replace(token, "");
    }

    let replacements = [
        ("schema_migrations", "seq_schema_migrations"),
        ("scout_files", "seq_scout_files"),
        ("scout_folders", "seq_scout_folders"),
        ("scout_extraction_log", "seq_scout_extraction_log"),
    ];
    for (table, seq) in replacements {
        let needle = format!("CREATE TABLE IF NOT EXISTS {table} (\n    id INTEGER PRIMARY KEY AUTOINCREMENT,");
        let replace = format!(
            "CREATE TABLE IF NOT EXISTS {table} (\n    id BIGINT DEFAULT nextval('{seq}'),"
        );
        base = base.replace(&needle, &replace);
    }

    // DuckDB uses INT32 for INTEGER; promote to BIGINT for timestamps and counts.
    base = base.replace(" INTEGER", " BIGINT");

    let mut cleaned = Vec::new();
    for line in base.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("UNIQUE(") {
            continue;
        }
        let mut line = line.replace(" PRIMARY KEY", "");
        line = line.replace(" UNIQUE", "");
        cleaned.push(line);
    }

    // Remove trailing commas before closing parens.
    let mut output: Vec<String> = Vec::new();
    for line in cleaned {
        if line.trim_start().starts_with(')') {
            if let Some(last) = output.last_mut() {
                if last.trim_end().ends_with(',') {
                    *last = last.trim_end_matches(',').to_string();
                }
            }
        }
        output.push(line);
    }

    let sequences = r#"
CREATE SEQUENCE IF NOT EXISTS seq_schema_migrations;
CREATE SEQUENCE IF NOT EXISTS seq_scout_files;
CREATE SEQUENCE IF NOT EXISTS seq_scout_folders;
CREATE SEQUENCE IF NOT EXISTS seq_scout_extraction_log;
"#;

    let unique_indexes = r#"
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_sources_id ON scout_sources(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_sources_name ON scout_sources(name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_tagging_rules_id ON scout_tagging_rules(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_tagging_rules_name ON scout_tagging_rules(name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_settings_key ON scout_settings(key);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_files_source_path ON scout_files(source_id, path);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_folders_source_prefix_name ON scout_folders(source_id, prefix, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_extractors_id ON scout_extractors(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_extractors_name ON scout_extractors(name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_parser_lab_test_files_parser_path ON parser_lab_test_files(parser_id, file_path);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_extraction_rules_source_name ON extraction_rules(source_id, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_extraction_fields_rule_name ON extraction_fields(rule_id, field_name);
"#;

    format!("{sequences}\n{}\n{unique_indexes}", output.join("\n"))
}

fn column_exists(conn: &DbConnection, table: &str, column: &str) -> Result<bool> {
    let rows = conn
        .query_all(
            "SELECT 1 FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ? AND column_name = ?",
            &[DbValue::from(table), DbValue::from(column)],
        )
        ?;
    Ok(!rows.is_empty())
}

/// Convert milliseconds since epoch to DateTime
fn millis_to_datetime(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap_or_else(Utc::now)
}

/// Get current time as milliseconds since epoch
fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// Convert a glob pattern to SQL LIKE pattern.
///
/// # Examples
/// - `*.csv` → `%.csv`
/// - `data_*` → `data_%`
/// - `**/*.csv` → `%.csv` (recursive match)
/// - `report_?.csv` → `report__.csv` (single char)
fn glob_to_like_pattern(glob: &str) -> String {
    let mut result = String::with_capacity(glob.len() + 4);

    // Handle ** (recursive) by treating as %
    let glob = glob.replace("**/", "");
    let glob = glob.replace("**", "%");

    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => result.push('%'),
            '?' => result.push('_'),
            '%' => result.push('%'), // Already converted from **
            '_' => {
                // Escape literal underscore
                result.push_str("\\_");
            }
            '\\' => {
                // Escape sequence - pass through next char
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(c),
        }
    }

    result
}

/// Database wrapper with unified backend connection.
#[derive(Clone)]
pub struct Database {
    conn: DbConnection,
    _temp_dir: Option<Arc<TempDir>>,
}

impl Database {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        #[cfg(feature = "duckdb")]
        let conn = DbConnection::open_duckdb(path)?;
        #[cfg(not(feature = "duckdb"))]
        {
            return Err(super::error::ScoutError::Config(
                "DuckDB feature not enabled".to_string(),
            ));
        }

        let schema_sql = schema_sql(true);
        conn.execute_batch(&schema_sql)?;
        Self::validate_schema(&conn)?;

        Ok(Self { conn, _temp_dir: None })
    }

    /// Validate schema columns and fail loud if the DB is outdated (pre-v1 policy).
    fn validate_schema(conn: &DbConnection) -> Result<()> {
        let required_columns = [
            "metadata_raw",
            "extraction_status",
            "extracted_at",
            "parent_path",
            "name",
            "extension",
        ];
        let mut missing = Vec::new();
        for col in required_columns {
            if !column_exists(conn, "scout_files", col)? {
                missing.push(col);
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        Err(ScoutError::Config(format!(
            "Database schema for 'scout_files' is missing columns: {}. \
Delete the database (default: ~/.casparian_flow/casparian_flow.duckdb) and restart.",
            missing.join(", ")
        )))
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let temp_dir = Arc::new(TempDir::new()?);
        let db_path = temp_dir.path().join("scout.duckdb");
        let conn = DbConnection::open_duckdb(&db_path)?;
        let schema_sql = schema_sql(true);
        conn.execute_batch(&schema_sql)?;
        Self::validate_schema(&conn)?;
        Ok(Self {
            conn,
            _temp_dir: Some(temp_dir),
        })
    }

    /// Get the underlying connection (for sharing with other code).
    pub fn conn(&self) -> &DbConnection {
        &self.conn
    }

    // ========================================================================
    // Source Operations
    // ========================================================================

    /// Insert or update a source
    pub fn upsert_source(&self, source: &Source) -> Result<()> {
        let source_type_json = serde_json::to_string(&source.source_type)?;
        let now = now_millis();

        self.conn
            .execute(
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
                &[
                    source.id.as_i64().into(),
                    source.name.as_str().into(),
                    source_type_json.into(),
                    source.path.as_str().into(),
                    (source.poll_interval_secs as i64).into(),
                    source.enabled.into(),
                    now.into(),
                    now.into(),
                ],
            )
            ?;

        Ok(())
    }

    /// Update the `updated_at` timestamp for a source (for MRU ordering)
    /// Called when a source is scanned or selected to bring it to the top of the list
    pub fn touch_source(&self, id: &SourceId) -> Result<()> {
        let now = now_millis();
        self.conn
            .execute(
                "UPDATE scout_sources SET updated_at = ? WHERE id = ?",
                &[now.into(), id.as_i64().into()],
            )
            ?;
        Ok(())
    }

    /// Update the file_count for a source (called after scanning)
    /// This is stored directly in scout_sources so listing sources is O(sources) not O(files)
    pub fn update_source_file_count(&self, id: &SourceId, file_count: usize) -> Result<()> {
        self.conn
            .execute(
                "UPDATE scout_sources SET file_count = ? WHERE id = ?",
                &[(file_count as i64).into(), id.as_i64().into()],
            )
            ?;
        Ok(())
    }

    /// Populate scout_folders table for O(1) TUI navigation (called after scanning)
    /// This pre-computes the folder hierarchy so get_folder_counts doesn't need to scan all files
    pub fn populate_folder_cache(&self, source_id: &SourceId) -> Result<()> {
        // Clear existing folder cache for this source
        self.conn
            .execute(
                "DELETE FROM scout_folders WHERE source_id = ?",
                &[source_id.as_i64().into()],
            )
            ?;

        // Compute root-level folders (most expensive query, but run once per scan)
        let root_folders = self.conn.query_all(
            r#"
            SELECT
                CASE
                    WHEN INSTR(parent_path, '/') > 0 THEN SUBSTR(parent_path, 1, INSTR(parent_path, '/') - 1)
                    ELSE parent_path
                END AS folder_name,
                COUNT(*) as file_count
            FROM scout_files
            WHERE source_id = ? AND parent_path <> ''
            GROUP BY folder_name
            ORDER BY file_count DESC
            LIMIT 500
            "#,
            &[source_id.as_i64().into()],
        )?;

        let mut root_folder_rows = Vec::with_capacity(root_folders.len());
        for row in root_folders {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            root_folder_rows.push((name, count));
        }

        let now = now_millis();

        // Insert root-level folders into scout_folders
        for (name, count) in &root_folder_rows {
            self.conn
                .execute(
                    "INSERT INTO scout_folders (source_id, prefix, name, file_count, is_folder, updated_at) VALUES (?, '', ?, ?, 1, ?)",
                    &[source_id.as_i64().into(), name.as_str().into(), (*count).into(), now.into()],
                )
                ?;
        }

        // Also add root-level files (files with empty parent_path)
        let root_files = self
            .conn
            .query_all(
                "SELECT name FROM scout_files WHERE source_id = ? AND parent_path = '' ORDER BY name LIMIT 200",
                &[source_id.as_i64().into()],
            )
            ?;

        let mut root_file_names = Vec::with_capacity(root_files.len());
        for row in root_files {
            let name: String = row.get(0)?;
            root_file_names.push(name);
        }

        for name in &root_file_names {
            self.conn
                .execute(
                    "INSERT INTO scout_folders (source_id, prefix, name, file_count, is_folder, updated_at) VALUES (?, '', ?, 1, 0, ?)",
                    &[source_id.as_i64().into(), name.as_str().into(), now.into()],
                )
                ?;
        }

        tracing::info!(
            source_id = %source_id,
            root_folders = root_folder_rows.len(),
            root_files = root_file_names.len(),
            "Populated folder cache"
        );

        Ok(())
    }

    /// Get folder counts from scout_folders cache (O(1) lookup)
    /// Returns None if cache is not populated for this source
    pub fn get_folder_counts_from_cache(
        &self,
        source_id: &SourceId,
        prefix: &str,
    ) -> Result<Option<Vec<(String, i64, bool)>>> {
        // Check if cache exists for this source/prefix
        let rows = self
            .conn
            .query_all(
                "SELECT name, file_count, is_folder FROM scout_folders WHERE source_id = ? AND prefix = ? ORDER BY is_folder DESC, file_count DESC, name",
                &[source_id.as_i64().into(), prefix.into()],
            )
            ?;

        let mut folder_rows = Vec::with_capacity(rows.len());
        for row in rows {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            let is_folder: i64 = row.get(2)?;
            folder_rows.push((name, count, is_folder));
        }

        if folder_rows.is_empty() && prefix.is_empty() {
            // No cache for root level - return None to trigger live query
            // But first check if source exists at all
            let source_exists = self
                .conn
                .query_optional(
                    "SELECT 1 FROM scout_sources WHERE id = ?",
                    &[source_id.as_i64().into()],
                )
                ?;

            if source_exists.is_some() {
                return Ok(None); // Source exists but no cache
            }
        }

        // Convert to expected format
        let results: Vec<(String, i64, bool)> = folder_rows
            .into_iter()
            .map(|(name, count, is_folder)| (name, count, is_folder == 0))
            .collect();

        Ok(Some(results))
    }

    /// Get a source by ID
    pub fn get_source(&self, id: &SourceId) -> Result<Option<Source>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE id = ?",
                &[id.as_i64().into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by name
    pub fn get_source_by_name(&self, name: &str) -> Result<Option<Source>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE name = ?",
                &[name.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by path
    pub fn get_source_by_path(&self, path: &str) -> Result<Option<Source>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE path = ?",
                &[path.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// List all sources
    pub fn list_sources(&self) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources ORDER BY name",
                &[],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// List enabled sources
    pub fn list_enabled_sources(&self) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE enabled = 1 ORDER BY name",
                &[],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// List enabled sources ordered by most recently used (updated_at DESC)
    /// This is used by the TUI to show recently accessed sources first
    pub fn list_sources_by_mru(&self) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE enabled = 1 ORDER BY updated_at DESC",
                &[],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// Delete a source and all associated data
    pub fn delete_source(&self, id: &SourceId) -> Result<bool> {
        // Delete associated files and tagging rules first
        self.conn
            .execute("DELETE FROM scout_files WHERE source_id = ?", &[id.as_i64().into()])
            ?;
        self.conn
            .execute("DELETE FROM scout_tagging_rules WHERE source_id = ?", &[id.as_i64().into()])
            ?;
        let result = self
            .conn
            .execute("DELETE FROM scout_sources WHERE id = ?", &[id.as_i64().into()])
            ?;

        Ok(result > 0)
    }

    /// Check if a new source path overlaps with any existing sources.
    ///
    /// Returns `Ok(())` if no overlap is detected.
    /// Returns `Err(SourceIsChildOfExisting)` if new path is inside an existing source.
    /// Returns `Err(SourceIsParentOfExisting)` if new path encompasses an existing source.
    ///
    /// # Arguments
    /// * `new_path` - The canonical path of the proposed new source
    ///
    /// # Why This Matters
    /// Overlapping sources cause:
    /// - Duplicate files in database (same file tracked twice)
    /// - Conflicting tags (different rules per source)
    /// - Double processing (parsers run twice)
    /// - Inflated file counts
    pub fn check_source_overlap(&self, new_path: &Path) -> Result<()> {
        use super::error::ScoutError;

        // Canonicalize the new path to resolve symlinks, `.`, `..`, etc.
        let new_canonical = new_path.canonicalize().map_err(|e| {
            ScoutError::Config(format!(
                "Cannot resolve path '{}': {}",
                new_path.display(),
                e
            ))
        })?;

        let existing_sources = self.list_sources()?;

        for source in existing_sources {
            // Canonicalize existing source path
            let existing_path = Path::new(&source.path);
            let existing_canonical = match existing_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    // Existing source path no longer exists - skip it
                    // (User should clean up stale sources separately)
                    continue;
                }
            };

            // Check if new path is a child of existing source
            // e.g., new=/data/projects/medical, existing=/data/projects
            if new_canonical.starts_with(&existing_canonical) && new_canonical != existing_canonical
            {
                return Err(ScoutError::SourceIsChildOfExisting {
                    new_path: new_path.display().to_string(),
                    existing_name: source.name,
                    existing_path: source.path,
                });
            }

            // Check if new path is a parent of existing source
            // e.g., new=/data/projects, existing=/data/projects/medical
            if existing_canonical.starts_with(&new_canonical) && existing_canonical != new_canonical
            {
                return Err(ScoutError::SourceIsParentOfExisting {
                    new_path: new_path.display().to_string(),
                    existing_name: source.name,
                    existing_path: source.path,
                });
            }
        }

        Ok(())
    }

    fn row_to_source(row: &casparian_db::UnifiedDbRow) -> Result<Source> {
        let source_type_json: String = row.get(2)?;
        let source_type: SourceType = serde_json::from_str(&source_type_json)?;
        let poll_interval: i64 = row.get(4)?;
        let enabled: i64 = row.get(5)?;

        let id_raw: i64 = row.get(0)?;
        let id = SourceId::try_from(id_raw)?;

        Ok(Source {
            id,
            name: row.get(1)?,
            source_type,
            path: row.get(3)?,
            poll_interval_secs: poll_interval as u64,
            enabled: enabled != 0,
        })
    }

    // ========================================================================
    // Tagging Rule Operations
    // ========================================================================

    /// Insert or update a tagging rule
    pub fn upsert_tagging_rule(&self, rule: &TaggingRule) -> Result<()> {
        let now = now_millis();

        self.conn
            .execute(
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
                &[
                    rule.id.as_i64().into(),
                    rule.name.as_str().into(),
                    rule.source_id.as_i64().into(),
                    rule.pattern.as_str().into(),
                    rule.tag.as_str().into(),
                    (rule.priority as i64).into(),
                    rule.enabled.into(),
                    now.into(),
                    now.into(),
                ],
            )
            ?;

        Ok(())
    }

    /// Get a tagging rule by ID
    pub fn get_tagging_rule(&self, id: &TaggingRuleId) -> Result<Option<TaggingRule>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE id = ?",
                &[id.as_i64().into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_tagging_rule(&row)?)),
            None => Ok(None),
        }
    }

    /// List all tagging rules
    pub fn list_tagging_rules(&self) -> Result<Vec<TaggingRule>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules ORDER BY priority DESC, name",
                &[],
            )
            ?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// List enabled tagging rules for a source (ordered by priority)
    pub fn list_tagging_rules_for_source(&self, source_id: &SourceId) -> Result<Vec<TaggingRule>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE source_id = ? AND enabled = 1 ORDER BY priority DESC, name",
                &[source_id.as_i64().into()],
            )
            ?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// Delete a tagging rule
    pub fn delete_tagging_rule(&self, id: &TaggingRuleId) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM scout_tagging_rules WHERE id = ?", &[id.as_i64().into()])
            ?;

        Ok(result > 0)
    }

    fn row_to_tagging_rule(row: &casparian_db::UnifiedDbRow) -> Result<TaggingRule> {
        let enabled: i64 = row.get(6)?;
        let id_raw: i64 = row.get(0)?;
        let source_id_raw: i64 = row.get(2)?;
        let id = TaggingRuleId::try_from(id_raw)?;
        let source_id = SourceId::try_from(source_id_raw)?;
        Ok(TaggingRule {
            id,
            name: row.get(1)?,
            source_id,
            pattern: row.get(3)?,
            tag: row.get(4)?,
            priority: row.get(5)?,
            enabled: enabled != 0,
        })
    }

    // ========================================================================
    // File Operations
    // ========================================================================

    /// Upsert a scanned file
    ///
    /// If the file exists and mtime/size changed, resets status to pending.
    pub fn upsert_file(&self, file: &ScannedFile) -> Result<UpsertResult> {
        // Check if file exists
        let existing = self
            .conn
            .query_optional(
                "SELECT id, size, mtime, status FROM scout_files WHERE source_id = ? AND path = ?",
                &[file.source_id.as_i64().into(), file.path.as_str().into()],
            )
            ?;

        let now = now_millis();
        match existing {
            None => {
                // New file
                self.conn
                    .execute(
                    r#"
                    INSERT INTO scout_files (source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                &[
                    file.source_id.as_i64().into(),
                    file.path.as_str().into(),
                    file.rel_path.as_str().into(),
                    file.parent_path.as_str().into(),
                    file.name.as_str().into(),
                    file.extension.as_deref().into(),
                    (file.size as i64).into(),
                    file.mtime.into(),
                    file.content_hash.as_deref().into(),
                    FileStatus::Pending.as_str().into(),
                    file.tag.as_deref().into(),
                    now.into(),
                    now.into(),
                ],
                )
                ?;

                let id: i64 = self
                    .conn
                    .query_scalar(
                        "SELECT id FROM scout_files WHERE source_id = ? AND path = ?",
                        &[file.source_id.as_i64().into(), file.path.as_str().into()],
                    )
                    ?;

                Ok(UpsertResult {
                    id,
                    is_new: true,
                    is_changed: false,
                })
            }
            Some(row) => {
                let id: i64 = row.get(0)?;
                let old_size: i64 = row.get(1)?;
                let old_mtime: i64 = row.get(2)?;

                let changed = file.size as i64 != old_size || file.mtime != old_mtime;

                if changed {
                    // File changed - reset to pending, clear tag
                    self.conn
                        .execute(
                        r#"
                        UPDATE scout_files SET
                            size = ?,
                            mtime = ?,
                            content_hash = ?,
                            status = ?,
                            tag = NULL,
                            error = NULL,
                            sentinel_job_id = NULL,
                            last_seen_at = ?
                        WHERE id = ?
                        "#,
                        &[
                            (file.size as i64).into(),
                            file.mtime.into(),
                            file.content_hash.as_deref().into(),
                            FileStatus::Pending.as_str().into(),
                            now.into(),
                            id.into(),
                        ],
                    )
                    ?;
                } else {
                    // Just update last_seen_at
                    self.conn
                        .execute(
                            "UPDATE scout_files SET last_seen_at = ? WHERE id = ?",
                            &[now.into(), id.into()],
                        )
                        ?;
                }

                Ok(UpsertResult {
                    id,
                    is_new: false,
                    is_changed: changed,
                })
            }
        }
    }

    /// Batch upsert files within a single transaction using bulk INSERT.
    ///
    /// Uses multi-row INSERT for ~10-20x speedup vs individual inserts.
    /// Chunks batches to stay under SQLite's 999 parameter limit.
    ///
    /// First queries existing files to properly track new/changed/unchanged,
    /// then uses bulk INSERT...ON CONFLICT for efficient upserts.
    pub fn batch_upsert_files(
        &self,
        files: &[ScannedFile],
        tag: Option<&str>,
    ) -> Result<BatchUpsertResult> {
        if files.is_empty() {
            return Ok(BatchUpsertResult::default());
        }

        if self.conn.backend_name() == "DuckDB" {
            return self.batch_upsert_files_duckdb(files, tag);
        }

        let now = now_millis();
        let files = files.to_vec();
        let tag = tag.map(|value| value.to_string());
        let source_id = files[0].source_id.clone();

        self.conn
            .transaction(move |tx| {
                let mut stats = BatchUpsertResult::default();

                // Query existing files to determine new vs changed vs unchanged
                // Note: This SELECT also needs chunking for large batches
                let existing = Self::query_existing_files_tx(tx, &source_id, &files)?;

                // Chunk size for bulk inserts. Modern SQLite supports 32766 params (since 3.32.0).
                // 100 rows per chunk is a good balance between fewer round-trips and memory usage.
                const CHUNK_SIZE: usize = 100;

                for chunk in files.chunks(CHUNK_SIZE) {
                    // Pre-compute stats for this chunk (assuming all succeed)
                    let mut chunk_new = 0u64;
                    let mut chunk_changed = 0u64;
                    let mut chunk_unchanged = 0u64;

                    for file in chunk {
                        let is_new = !existing.contains_key(&file.path);
                        let is_changed = existing
                            .get(&file.path)
                            .is_some_and(|(size, mtime)| *size != file.size as i64 || *mtime != file.mtime);

                        if is_new {
                            chunk_new += 1;
                        } else if is_changed {
                            chunk_changed += 1;
                        } else {
                            chunk_unchanged += 1;
                        }
                    }

                    // Try bulk insert
                    match Self::bulk_insert_chunk_tx(tx, chunk, tag.as_deref(), now) {
                        Ok(()) => {
                            stats.new += chunk_new;
                            stats.changed += chunk_changed;
                            stats.unchanged += chunk_unchanged;
                        }
                        Err(e) => {
                            // Bulk failed - fall back to row-by-row to isolate bad row
                            tracing::debug!(error = %e, "Bulk insert failed, falling back to row-by-row");
                            Self::insert_rows_individually_tx(
                                tx,
                                chunk,
                                tag.as_deref(),
                                now,
                                &existing,
                                &mut stats,
                            );
                        }
                    }
                }

                Ok(stats)
            })
            
            .map_err(ScoutError::from)
    }

    fn batch_upsert_files_duckdb(
        &self,
        files: &[ScannedFile],
        tag: Option<&str>,
    ) -> Result<BatchUpsertResult> {
        #[cfg(feature = "duckdb")]
        {
            let now = now_millis();
            let mut stats = BatchUpsertResult::default();
            let existing = Self::query_existing_files_conn(&self.conn, files)?;

            for file in files {
                let is_new = !existing.contains_key(&file.path);
                let is_changed = existing
                    .get(&file.path)
                    .is_some_and(|(size, mtime)| *size != file.size as i64 || *mtime != file.mtime);

                if is_new {
                    stats.new += 1;
                } else if is_changed {
                    stats.changed += 1;
                } else {
                    stats.unchanged += 1;
                }
            }

            let tag_override = tag.map(|t| t.to_string());
            let files_vec = files.to_vec();
            let pending_status = FileStatus::Pending.as_str();

            self.conn
                .execute_duckdb_op(move |conn| {
                    conn.execute_batch(&format!(
                        "CREATE TEMP TABLE IF NOT EXISTS staging_scout_files (
                            source_id BIGINT NOT NULL,
                            path TEXT NOT NULL,
                            rel_path TEXT NOT NULL,
                            parent_path TEXT NOT NULL DEFAULT '',
                            name TEXT NOT NULL DEFAULT '',
                            extension TEXT,
                            size BIGINT NOT NULL,
                            mtime BIGINT NOT NULL,
                            content_hash TEXT,
                            status TEXT NOT NULL DEFAULT '{pending_status}',
                            tag TEXT,
                            first_seen_at BIGINT NOT NULL,
                            last_seen_at BIGINT NOT NULL
                        );
                        DELETE FROM staging_scout_files;",
                        pending_status = pending_status
                    ))?;

                    {
                        let mut appender = conn.appender("staging_scout_files")?;
                        for file in &files_vec {
                            let file_tag = tag_override.as_deref().or(file.tag.as_deref());
                            appender.append_row(duckdb::params![
                                file.source_id.as_i64(),
                                &file.path,
                                &file.rel_path,
                                &file.parent_path,
                                &file.name,
                                file.extension.as_deref(),
                                file.size as i64,
                                file.mtime,
                                file.content_hash.as_deref(),
                                pending_status,
                                file_tag,
                                now,
                                now
                            ])?;
                        }
                        appender.flush()?;
                    }

                    let merge_sql = if tag_override.is_some() {
                        format!(
                            r#"
                        MERGE INTO scout_files AS target
                        USING staging_scout_files AS source
                        ON target.source_id = source.source_id AND target.path = source.path
                        WHEN MATCHED AND (target.size != source.size OR target.mtime != source.mtime) THEN
                            UPDATE SET
                                size = source.size,
                                mtime = source.mtime,
                                content_hash = source.content_hash,
                                parent_path = source.parent_path,
                                name = source.name,
                                extension = source.extension,
                                status = '{pending_status}',
                                tag = source.tag,
                                error = NULL,
                                sentinel_job_id = NULL,
                                last_seen_at = source.last_seen_at
                        WHEN MATCHED THEN
                            UPDATE SET
                                last_seen_at = source.last_seen_at,
                                tag = source.tag
                        WHEN NOT MATCHED THEN
                            INSERT (source_id, path, rel_path, parent_path, name, extension,
                                    size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                            VALUES (source.source_id, source.path, source.rel_path, source.parent_path, source.name, source.extension,
                                    source.size, source.mtime, source.content_hash, source.status, source.tag, source.first_seen_at, source.last_seen_at)
                        "#,
                            pending_status = pending_status
                        )
                    } else {
                        format!(
                            r#"
                        MERGE INTO scout_files AS target
                        USING staging_scout_files AS source
                        ON target.source_id = source.source_id AND target.path = source.path
                        WHEN MATCHED AND (target.size != source.size OR target.mtime != source.mtime) THEN
                            UPDATE SET
                                size = source.size,
                                mtime = source.mtime,
                                content_hash = source.content_hash,
                                parent_path = source.parent_path,
                                name = source.name,
                                extension = source.extension,
                                status = '{pending_status}',
                                error = NULL,
                                sentinel_job_id = NULL,
                                last_seen_at = source.last_seen_at
                        WHEN MATCHED THEN
                            UPDATE SET
                                last_seen_at = source.last_seen_at
                        WHEN NOT MATCHED THEN
                            INSERT (source_id, path, rel_path, parent_path, name, extension,
                                    size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                            VALUES (source.source_id, source.path, source.rel_path, source.parent_path, source.name, source.extension,
                                    source.size, source.mtime, source.content_hash, source.status, source.tag, source.first_seen_at, source.last_seen_at)
                        "#,
                            pending_status = pending_status
                        )
                    };

                    conn.execute_batch(&merge_sql)?;
                    conn.execute_batch("DELETE FROM staging_scout_files;")?;
                    Ok(())
                })
                ?;

            Ok(stats)
        }

        #[cfg(not(feature = "duckdb"))]
        {
            let _ = tag;
            let _ = files;
            Err(super::error::ScoutError::Config(
                "DuckDB feature not enabled".to_string(),
            ))
        }
    }

    fn query_existing_files_conn(
        conn: &DbConnection,
        files: &[ScannedFile],
    ) -> Result<std::collections::HashMap<String, (i64, i64)>> {
        if files.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut existing = std::collections::HashMap::with_capacity(files.len());
        let source_id = files[0].source_id.as_i64();
        const SELECT_CHUNK_SIZE: usize = 500;

        for chunk in files.chunks(SELECT_CHUNK_SIZE) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT path, size, mtime FROM scout_files WHERE source_id = ? AND path IN ({})",
                placeholders
            );

            let mut params = Vec::with_capacity(chunk.len() + 1);
            params.push(DbValue::from(source_id));
            for file in chunk {
                params.push(file.path.as_str().into());
            }

            let rows = conn.query_all(&query, &params)?;
            for row in rows {
                let path: String = row.get(0)?;
                let size: i64 = row.get(1)?;
                let mtime: i64 = row.get(2)?;
                existing.insert(path, (size, mtime));
            }
        }

        Ok(existing)
    }

    /// Query existing files for a batch (chunked to avoid parameter limit)
    fn query_existing_files_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        source_id: &SourceId,
        files: &[ScannedFile],
    ) -> std::result::Result<std::collections::HashMap<String, (i64, i64)>, casparian_db::BackendError> {
        let mut existing = std::collections::HashMap::with_capacity(files.len());

        // Chunk the SELECT query too (999 params, 1 for source_id + N for paths)
        const SELECT_CHUNK_SIZE: usize = 500;

        for chunk in files.chunks(SELECT_CHUNK_SIZE) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT path, size, mtime FROM scout_files WHERE source_id = ? AND path IN ({})",
                placeholders
            );

            let mut params = Vec::with_capacity(chunk.len() + 1);
            params.push(DbValue::from(source_id.as_i64()));
            for file in chunk {
                params.push(file.path.as_str().into());
            }

            let rows = tx.query_all(&query, &params)?;
            for row in rows {
                let path: String = row.get(0)?;
                let size: i64 = row.get(1)?;
                let mtime: i64 = row.get(2)?;
                existing.insert(path, (size, mtime));
            }
        }

        Ok(existing)
    }

    /// Bulk insert a chunk of files using multi-row VALUES
    fn bulk_insert_chunk_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        files: &[ScannedFile],
        tag: Option<&str>,
        now: i64,
    ) -> std::result::Result<(), casparian_db::BackendError> {
        if files.is_empty() {
            return Ok(());
        }

        // Build multi-row VALUES with FileStatus::Pending for status.
        // 13 bind params per row: source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at
        let row_placeholder = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let values: String = (0..files.len())
            .map(|_| row_placeholder)
            .collect::<Vec<_>>()
            .join(", ");

        // Two SQL patterns based on whether tag is provided:
        // - With tag: ON CONFLICT updates tag to excluded.tag
        // - Without tag: ON CONFLICT preserves existing tag
        let sql = if tag.is_some() {
            format!(
                r#"INSERT INTO scout_files
                   (source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                   VALUES {}
                   ON CONFLICT(source_id, path) DO UPDATE SET
                       size = excluded.size,
                       mtime = excluded.mtime,
                       content_hash = excluded.content_hash,
                       parent_path = excluded.parent_path,
                       name = excluded.name,
                       extension = excluded.extension,
                       status = CASE
                           WHEN scout_files.size != excluded.size OR scout_files.mtime != excluded.mtime
                           THEN excluded.status
                           ELSE scout_files.status
                       END,
                       tag = excluded.tag,
                       last_seen_at = excluded.last_seen_at
                "#,
                values
            )
        } else {
            format!(
                r#"INSERT INTO scout_files
                   (source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                   VALUES {}
                   ON CONFLICT(source_id, path) DO UPDATE SET
                       size = excluded.size,
                       mtime = excluded.mtime,
                       content_hash = excluded.content_hash,
                       parent_path = excluded.parent_path,
                       name = excluded.name,
                       extension = excluded.extension,
                       status = CASE
                           WHEN scout_files.size != excluded.size OR scout_files.mtime != excluded.mtime
                           THEN excluded.status
                           ELSE scout_files.status
                       END,
                       last_seen_at = excluded.last_seen_at
                "#,
                values
            )
        };

        let mut params = Vec::with_capacity(files.len() * 13);
        for file in files {
            let file_tag = tag.or(file.tag.as_deref());
            params.push(file.source_id.as_i64().into());
            params.push(file.path.as_str().into());
            params.push(file.rel_path.as_str().into());
            params.push(file.parent_path.as_str().into());
            params.push(file.name.as_str().into());
            params.push(file.extension.clone().into());
            params.push((file.size as i64).into());
            params.push(file.mtime.into());
            params.push(file.content_hash.clone().into());
            params.push(FileStatus::Pending.as_str().into());
            params.push(file_tag.map(|value| value.to_string()).into());
            params.push(now.into());
            params.push(now.into());
        }

        tx.execute(&sql, &params)?;
        Ok(())
    }

    /// Fallback: insert rows one at a time when bulk insert fails
    fn insert_rows_individually_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        files: &[ScannedFile],
        tag: Option<&str>,
        now: i64,
        existing: &std::collections::HashMap<String, (i64, i64)>,
        stats: &mut BatchUpsertResult,
    ) {
        for file in files {
            let is_new = !existing.contains_key(&file.path);
            let is_changed = existing
                .get(&file.path)
                .is_some_and(|(size, mtime)| *size != file.size as i64 || *mtime != file.mtime);

            let file_tag = tag.or(file.tag.as_deref());
            let params = [
                DbValue::from(file.source_id.as_i64()),
                file.path.as_str().into(),
                file.rel_path.as_str().into(),
                file.parent_path.as_str().into(),
                file.name.as_str().into(),
                file.extension.clone().into(),
                (file.size as i64).into(),
                file.mtime.into(),
                file.content_hash.clone().into(),
                FileStatus::Pending.as_str().into(),
                file_tag.map(|value| value.to_string()).into(),
                now.into(),
                now.into(),
                tag.map(|value| value.to_string()).into(),
                tag.map(|value| value.to_string()).into(),
            ];

            let result = tx.execute(
                r#"INSERT INTO scout_files
                   (source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(source_id, path) DO UPDATE SET
                       size = excluded.size,
                       mtime = excluded.mtime,
                       content_hash = excluded.content_hash,
                       parent_path = excluded.parent_path,
                       name = excluded.name,
                       extension = excluded.extension,
                       status = CASE
                           WHEN scout_files.size != excluded.size OR scout_files.mtime != excluded.mtime
                           THEN excluded.status
                           ELSE scout_files.status
                       END,
                       tag = CASE WHEN ? IS NOT NULL THEN ? ELSE scout_files.tag END,
                       last_seen_at = excluded.last_seen_at"#,
                &params,
            );

            match result {
                Ok(_) => {
                    if is_new {
                        stats.new += 1;
                    } else if is_changed {
                        stats.changed += 1;
                    } else {
                        stats.unchanged += 1;
                    }
                }
                Err(e) => {
                    stats.errors += 1;
                    tracing::debug!(file = %file.path, error = %e, "Failed to upsert file");
                }
            }
        }
    }

    /// Get a file by ID
    pub fn get_file(&self, id: i64) -> Result<Option<ScannedFile>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE id = ?
                "#,
                &[id.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a file by path
    pub fn get_file_by_path(&self, source_id: &SourceId, path: &str) -> Result<Option<ScannedFile>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE source_id = ? AND path = ?
                "#,
                &[source_id.as_i64().into(), path.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// List all files for a source (regardless of status)
    pub fn list_files_by_source(&self, source_id: &SourceId, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE source_id = ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[source_id.as_i64().into(), (limit as i64).into()],
            )
            ?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files with a specific status
    pub fn list_files_by_status(&self, status: FileStatus, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE status = ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[status.as_str().into(), (limit as i64).into()],
            )
            ?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List pending (untagged) files for a source
    pub fn list_pending_files(&self, source_id: &SourceId, limit: usize) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Pending, limit)
    }

    /// List tagged files ready for processing
    pub fn list_tagged_files(&self, source_id: &SourceId, limit: usize) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Tagged, limit)
    }

    /// List untagged files (files that have no tag assigned)
    pub fn list_untagged_files(&self, source_id: &SourceId, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE source_id = ? AND tag IS NULL AND status = ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[
                    source_id.as_i64().into(),
                    FileStatus::Pending.as_str().into(),
                    (limit as i64).into(),
                ],
            )
            ?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files by tag
    pub fn list_files_by_tag(&self, tag: &str, limit: usize) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE tag = ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[tag.into(), (limit as i64).into()],
            )
            ?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files for a source with specific status
    pub fn list_files_by_source_and_status(
        &self,
        source_id: &SourceId,
        status: FileStatus,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error,
                       first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files WHERE source_id = ? AND status = ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[source_id.as_i64().into(), status.as_str().into(), (limit as i64).into()],
            )
            ?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// Tag a file manually (sets tag_source = 'manual')
    pub fn tag_file(&self, id: i64, tag: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE scout_files SET tag = ?, tag_source = ?, rule_id = NULL, status = ? WHERE id = ?",
                &[
                    tag.into(),
                    TagSource::Manual.as_str().into(),
                    FileStatus::Tagged.as_str().into(),
                    id.into(),
                ],
            )
            ?;
        Ok(())
    }

    /// Tag multiple files manually (sets tag_source = 'manual')
    pub fn tag_files(&self, ids: &[i64], tag: &str) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let tag_source = TagSource::Manual.as_str();
        let mut total = 0u64;
        for id in ids {
            let result = self
                .conn
                .execute(
                    "UPDATE scout_files SET tag = ?, tag_source = ?, rule_id = NULL, status = ? WHERE id = ?",
                    &[
                        tag.into(),
                        tag_source.into(),
                        FileStatus::Tagged.as_str().into(),
                        (*id).into(),
                    ],
                )
                ?;
            total += result;
        }
        Ok(total)
    }

    /// Tag a file via a tagging rule (sets tag_source = 'rule')
    pub fn tag_file_by_rule(&self, id: i64, tag: &str, rule_id: &TaggingRuleId) -> Result<()> {
        self.conn
            .execute(
                "UPDATE scout_files SET tag = ?, tag_source = ?, rule_id = ?, status = ? WHERE id = ?",
                &[
                    tag.into(),
                    TagSource::Rule.as_str().into(),
                    rule_id.as_i64().into(),
                    FileStatus::Tagged.as_str().into(),
                    id.into(),
                ],
            )
            ?;
        Ok(())
    }

    /// Update file status
    pub fn update_file_status(&self, id: i64, status: FileStatus, error: Option<&str>) -> Result<()> {
        if status == FileStatus::Processed {
            self.conn
                .execute(
                    "UPDATE scout_files SET status = ?, error = ?, processed_at = ? WHERE id = ?",
                    &[
                        status.as_str().into(),
                        error.into(),
                        now_millis().into(),
                        id.into(),
                    ],
                )
                ?;
        } else {
            self.conn
                .execute(
                    "UPDATE scout_files SET status = ?, error = ? WHERE id = ?",
                    &[status.as_str().into(), error.into(), id.into()],
                )
                ?;
        }
        Ok(())
    }

    /// Untag a file (clear tag, tag_source, rule_id, manual_plugin and reset to pending)
    pub fn untag_file(&self, id: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE scout_files SET tag = NULL, tag_source = NULL, rule_id = NULL, \
                 manual_plugin = NULL, status = ?, sentinel_job_id = NULL WHERE id = ?",
                &[FileStatus::Pending.as_str().into(), id.into()],
            )
            ?;
        Ok(())
    }

    /// Mark file as queued for processing
    pub fn mark_file_queued(&self, id: i64, sentinel_job_id: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE scout_files SET status = ?, sentinel_job_id = ? WHERE id = ?",
                &[
                    FileStatus::Queued.as_str().into(),
                    sentinel_job_id.into(),
                    id.into(),
                ],
            )
            ?;
        Ok(())
    }

    /// Mark files as deleted if not seen recently
    pub fn mark_deleted_files(&self, source_id: &SourceId, seen_before: DateTime<Utc>) -> Result<u64> {
        let seen_before_millis = seen_before.timestamp_millis();
        let result = self
            .conn
            .execute(
                r#"
                UPDATE scout_files SET status = ?
                WHERE source_id = ? AND last_seen_at < ? AND status != ?
                "#,
                &[
                    FileStatus::Deleted.as_str().into(),
                    source_id.as_i64().into(),
                    seen_before_millis.into(),
                    FileStatus::Deleted.as_str().into(),
                ],
            )
            ?;

        Ok(result)
    }

    fn row_to_file(row: &casparian_db::UnifiedDbRow) -> Result<ScannedFile> {
        use super::types::ExtractionStatus;

        // Column positions (extension added at position 6):
        // 0:id, 1:source_id, 2:path, 3:rel_path, 4:parent_path, 5:name, 6:extension,
        // 7:size, 8:mtime, 9:content_hash, 10:status, 11:tag, 12:tag_source,
        // 13:rule_id, 14:manual_plugin, 15:error, 16:first_seen_at, 17:last_seen_at,
        // 18:processed_at, 19:sentinel_job_id, 20:metadata_raw, 21:extraction_status, 22:extracted_at

        let status_str: String = row.get(10)?;
        let status = FileStatus::parse(&status_str)
            .ok_or_else(|| ScoutError::InvalidState(format!("Invalid file status: {}", status_str)))?;

        let first_seen_millis: i64 = row.get(16)?;
        let last_seen_millis: i64 = row.get(17)?;
        let processed_at_millis: Option<i64> = row.get(18)?;

        // Parse extraction status (Phase 6)
        let extraction_status_str: Option<String> = row.get(21)?;
        let extraction_status = match extraction_status_str.as_deref() {
            Some(raw) => ExtractionStatus::parse(raw)
                .ok_or_else(|| ScoutError::InvalidState(format!("Invalid extraction status: {}", raw)))?,
            None => ExtractionStatus::Pending,
        };
        let extracted_at_millis: Option<i64> = row.get(22)?;

        let source_id_raw: i64 = row.get(1)?;
        let source_id = SourceId::try_from(source_id_raw)?;

        let tag_source_raw: Option<String> = row.get(12)?;
        let tag_source = match tag_source_raw {
            Some(raw) => Some(
                TagSource::parse(&raw)
                    .ok_or_else(|| ScoutError::InvalidState(format!("Invalid tag source: {}", raw)))?,
            ),
            None => None,
        };

        let rule_id_raw: Option<i64> = row.get(13)?;
        let rule_id = match rule_id_raw {
            Some(raw) => Some(TaggingRuleId::try_from(raw)?),
            None => None,
        };

        Ok(ScannedFile {
            id: Some(row.get(0)?),
            source_id,
            path: row.get(2)?,
            rel_path: row.get(3)?,
            parent_path: row.get(4)?,
            name: row.get(5)?,
            extension: row.get(6)?,
            size: row.get::<i64>(7)? as u64,
            mtime: row.get(8)?,
            content_hash: row.get(9)?,
            status,
            tag: row.get(11)?,
            tag_source,
            rule_id,
            manual_plugin: row.get(14)?,
            error: row.get(15)?,
            first_seen_at: millis_to_datetime(first_seen_millis),
            last_seen_at: millis_to_datetime(last_seen_millis),
            processed_at: processed_at_millis.map(millis_to_datetime),
            sentinel_job_id: row.get(19)?,
            // Extractor metadata fields (Phase 6)
            metadata_raw: row.get(20)?,
            extraction_status,
            extracted_at: extracted_at_millis.map(millis_to_datetime),
        })
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DbStats> {
        let row = self
            .conn
            .query_optional(
                &format!(
                    r#"
                SELECT
                    COUNT(*) as total_files,
                    SUM(CASE WHEN status = '{pending}' THEN 1 ELSE 0 END) as files_pending,
                    SUM(CASE WHEN status = '{tagged}' THEN 1 ELSE 0 END) as files_tagged,
                    SUM(CASE WHEN status = '{queued}' THEN 1 ELSE 0 END) as files_queued,
                    SUM(CASE WHEN status = '{processing}' THEN 1 ELSE 0 END) as files_processing,
                    SUM(CASE WHEN status = '{processed}' THEN 1 ELSE 0 END) as files_processed,
                    SUM(CASE WHEN status = '{failed}' THEN 1 ELSE 0 END) as files_failed,
                    COALESCE(SUM(CASE WHEN status = '{pending}' THEN size ELSE 0 END), 0) as bytes_pending,
                    COALESCE(SUM(CASE WHEN status = '{processed}' THEN size ELSE 0 END), 0) as bytes_processed
                FROM scout_files
                "#,
                    pending = FileStatus::Pending.as_str(),
                    tagged = FileStatus::Tagged.as_str(),
                    queued = FileStatus::Queued.as_str(),
                    processing = FileStatus::Processing.as_str(),
                    processed = FileStatus::Processed.as_str(),
                    failed = FileStatus::Failed.as_str(),
                ),
                &[],
            )
            ?;

        let (total_files, files_pending, files_tagged, files_queued, files_processing, files_processed, files_failed, bytes_pending, bytes_processed) =
            if let Some(row) = row {
                (
                    row.get::<i64>(0)?,
                    row.get::<i64>(1)?,
                    row.get::<i64>(2)?,
                    row.get::<i64>(3)?,
                    row.get::<i64>(4)?,
                    row.get::<i64>(5)?,
                    row.get::<i64>(6)?,
                    row.get::<i64>(7)?,
                    row.get::<i64>(8)?,
                )
            } else {
                (0, 0, 0, 0, 0, 0, 0, 0, 0)
            };

        let total_sources = self
            .conn
            .query_scalar::<i64>("SELECT COUNT(*) FROM scout_sources", &[])
            
            .unwrap_or(0);

        let total_tagging_rules = self
            .conn
            .query_scalar::<i64>("SELECT COUNT(*) FROM scout_tagging_rules", &[])
            
            .unwrap_or(0);

        Ok(DbStats {
            total_sources: total_sources as u64,
            total_tagging_rules: total_tagging_rules as u64,
            total_files: total_files as u64,
            files_pending: files_pending as u64,
            files_tagged: files_tagged as u64,
            files_queued: files_queued as u64,
            files_processing: files_processing as u64,
            files_processed: files_processed as u64,
            files_failed: files_failed as u64,
            bytes_pending: bytes_pending as u64,
            bytes_processed: bytes_processed as u64,
        })
    }

    // ========================================================================
    // Glob Explorer Operations (Hierarchical Browsing)
    // ========================================================================

    /// Get folder counts at a specific depth for hierarchical browsing.
    ///
    /// Uses the indexed `parent_path` column for O(1) folder navigation.
    /// For glob filtering, uses LIKE queries on `rel_path`.
    ///
    /// # Arguments
    /// * `source_id` - The source to query
    /// * `prefix` - Path prefix (empty for root, "folder" for subfolder - no trailing slash)
    /// * `glob_pattern` - Optional glob pattern filter (e.g., "*.csv")
    ///
    /// # Returns
    /// Vec of (folder_name, file_count, is_file) tuples
    pub fn get_folder_counts(
        &self,
        source_id: &SourceId,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<Vec<(String, i64, bool)>> {
        // Normalize prefix: remove trailing slash if present
        let prefix = prefix.trim_end_matches('/');

        if let Some(pattern) = glob_pattern {
            // With glob pattern: search matching files and group by immediate child
            self.get_folder_counts_with_pattern(source_id, prefix, pattern)
        } else {
            // No pattern: O(1) lookup using parent_path index
            self.get_folder_counts_fast(source_id, prefix)
        }
    }

    /// Fast O(1) folder listing using parent_path index (no pattern filtering)
    fn get_folder_counts_fast(
        &self,
        source_id: &SourceId,
        parent_path: &str,
    ) -> Result<Vec<(String, i64, bool)>> {
        // For root level, try the pre-computed cache first (avoids 20+ second GROUP BY)
        if parent_path.is_empty() {
            if let Some(cached) = self.get_folder_counts_from_cache(source_id, "")? {
                if !cached.is_empty() {
                    tracing::debug!(source_id = %source_id, count = cached.len(), "Using cached folder counts");
                    return Ok(cached);
                }
            }
            // Cache not populated - fall through to live query (will be slow for large sources)
            tracing::debug!(source_id = %source_id, "Folder cache miss, using live query");
        }

        let mut results: Vec<(String, i64, bool)> = Vec::new();

        // 1. Get files directly in this folder (O(1) via index)
        let files = self
            .conn
            .query_all(
                "SELECT name, size FROM scout_files WHERE source_id = ? AND parent_path = ? ORDER BY name LIMIT 200",
                &[source_id.as_i64().into(), parent_path.into()],
            )
            ?;

        let mut file_rows = Vec::with_capacity(files.len());
        for row in files {
            let name: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            file_rows.push((name, size));
        }

        // 2. Get immediate subfolders with file counts
        // A folder exists if any file has a parent_path that starts with "current/X"
        let folder_prefix = if parent_path.is_empty() {
            String::new()
        } else {
            format!("{}/", parent_path)
        };

        let subfolders = if parent_path.is_empty() {
            // Root level: find top-level folders by extracting first path component
            // NOTE: This is slow for large sources (20+ seconds). Cache should be used.
            self.conn
                .query_all(
                r#"
                SELECT
                    CASE
                        WHEN INSTR(parent_path, '/') > 0 THEN SUBSTR(parent_path, 1, INSTR(parent_path, '/') - 1)
                        ELSE parent_path
                    END AS folder_name,
                    COUNT(*) as file_count
                FROM scout_files
                WHERE source_id = ? AND parent_path != ''
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[source_id.as_i64().into()],
            )
            ?
        } else {
            // Non-root: find immediate subfolders
            self.conn
                .query_all(
                r#"
                SELECT
                    CASE
                        WHEN INSTR(SUBSTR(parent_path, LENGTH(?) + 1), '/') > 0
                        THEN SUBSTR(parent_path, LENGTH(?) + 1, INSTR(SUBSTR(parent_path, LENGTH(?) + 1), '/') - 1)
                        ELSE SUBSTR(parent_path, LENGTH(?) + 1)
                    END AS folder_name,
                    COUNT(*) as file_count
                FROM scout_files
                WHERE source_id = ? AND parent_path LIKE ? || '%' AND parent_path != ?
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    source_id.as_i64().into(),
                    folder_prefix.as_str().into(),
                    parent_path.into(),
                ],
            )
            ?
        };

        let mut subfolder_rows = Vec::with_capacity(subfolders.len());
        for row in subfolders {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            subfolder_rows.push((name, count));
        }

        // Add folders first (sorted by count desc)
        for (name, count) in subfolder_rows {
            if !name.is_empty() {
                results.push((name, count, false));
            }
        }

        // Add files (sorted by name)
        for (name, _size) in file_rows {
            results.push((name, 1, true));
        }

        Ok(results)
    }

    /// Folder listing with glob pattern filtering using LIKE
    fn get_folder_counts_with_pattern(
        &self,
        source_id: &SourceId,
        prefix: &str,
        pattern: &str,
    ) -> Result<Vec<(String, i64, bool)>> {
        // Convert glob pattern to SQL LIKE pattern
        // *.csv -> %.csv
        // data_* -> data_%
        // **/*.csv -> %/%.csv (recursive)
        let like_pattern = glob_to_like_pattern(pattern);

        // Build the prefix filter
        let path_filter = if prefix.is_empty() {
            like_pattern.clone()
        } else {
            format!("{}/%", prefix)
        };

        // Query matching files and extract immediate child at prefix level
        let prefix_len = if prefix.is_empty() { 0 } else { prefix.len() as i32 + 1 };

        let rows = self.conn.query_all(
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
              AND rel_path LIKE ?
              AND rel_path LIKE ?
              AND LENGTH(rel_path) > ?
            GROUP BY item_name
            ORDER BY file_count DESC
            LIMIT 100
            "#,
            &[
                (prefix_len as i64).into(),
                (prefix_len as i64).into(),
                (prefix_len as i64).into(),
                (prefix_len as i64).into(),
                (prefix_len as i64).into(),
                source_id.as_i64().into(),
                path_filter.as_str().into(),
                like_pattern.as_str().into(),
                (prefix_len as i64).into(),
            ],
        )?;

        let mut results = Vec::new();
        for row in rows {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            let is_file: i64 = row.get(2)?;
            if !name.is_empty() {
                results.push((name, count, is_file != 0));
            }
        }

        Ok(results)
    }

    /// Get sampled preview files for a prefix and optional pattern.
    ///
    /// Returns up to `limit` files matching the criteria, for display in preview pane.
    /// Uses LIKE queries for portable glob-style matching.
    pub fn get_preview_files(
        &self,
        source_id: &SourceId,
        prefix: &str,
        glob_pattern: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, i64, i64)>> {
        // Returns (rel_path, size, mtime)
        let prefix = prefix.trim_end_matches('/');
        let prefix_pattern = if prefix.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", prefix)
        };

        let rows = if let Some(pattern) = glob_pattern {
            let like_pattern = glob_to_like_pattern(pattern);
            self.conn
                .query_all(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ?
                  AND rel_path LIKE ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    like_pattern.as_str().into(),
                    (limit as i64).into(),
                ],
            )
            ?
        } else {
            self.conn
                .query_all(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    (limit as i64).into(),
                ],
            )
            ?
        };

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let rel_path: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            let mtime: i64 = row.get(2)?;
            results.push((rel_path, size, mtime));
        }

        Ok(results)
    }

    /// Get total file count for a prefix and optional pattern.
    /// Uses LIKE queries for portable glob-style matching.
    pub fn get_file_count_for_prefix(
        &self,
        source_id: &SourceId,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<i64> {
        let prefix = prefix.trim_end_matches('/');
        let prefix_pattern = if prefix.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", prefix)
        };

        let count = if let Some(pattern) = glob_pattern {
            let like_pattern = glob_to_like_pattern(pattern);
            self.conn
                .query_scalar::<i64>(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ?
                  AND rel_path LIKE ?
                "#,
                &[
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    like_pattern.as_str().into(),
                ],
            )
            ?
        } else {
            self.conn
                .query_scalar::<i64>(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ?
                "#,
                &[source_id.as_i64().into(), prefix_pattern.as_str().into()],
            )
            ?
        };

        Ok(count)
    }

    // ========================================================================
    // Pattern Search (database-first, using extension index)
    // ========================================================================

    /// Search files by extension and optional path pattern.
    ///
    /// Uses the extension index for fast filtering, then applies LIKE pattern if provided.
    /// Returns (rel_path, size, mtime) tuples for display.
    ///
    /// # Arguments
    /// * `source_id` - The source to search
    /// * `extension` - File extension to filter by (e.g., "rs", "csv"), or None for all
    /// * `path_pattern` - SQL LIKE pattern for path (e.g., "%/src/%"), or None for all
    /// * `limit` - Max results to return
    /// * `offset` - Offset for pagination
    pub fn search_files_by_pattern(
        &self,
        source_id: &SourceId,
        extension: Option<&str>,
        path_pattern: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<(String, i64, i64)>> {
        let rows = match (extension, path_pattern) {
            (Some(ext), Some(path_pat)) => {
                self.conn
                    .query_all(
                        r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE source_id = ? AND extension = ? AND rel_path LIKE ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                        &[
                            source_id.as_i64().into(),
                            ext.into(),
                            path_pat.into(),
                            (limit as i64).into(),
                            (offset as i64).into(),
                        ],
                    )
                    ?
            }
            (Some(ext), None) => {
                self.conn
                    .query_all(
                        r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE source_id = ? AND extension = ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                        &[
                            source_id.as_i64().into(),
                            ext.into(),
                            (limit as i64).into(),
                            (offset as i64).into(),
                        ],
                    )
                    ?
            }
            (None, Some(path_pat)) => {
                self.conn
                    .query_all(
                        r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE source_id = ? AND rel_path LIKE ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                        &[
                            source_id.as_i64().into(),
                            path_pat.into(),
                            (limit as i64).into(),
                            (offset as i64).into(),
                        ],
                    )
                    ?
            }
            (None, None) => {
                self.conn
                    .query_all(
                        r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE source_id = ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                        &[
                            source_id.as_i64().into(),
                            (limit as i64).into(),
                            (offset as i64).into(),
                        ],
                    )
                    ?
            }
        };

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let rel_path: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            let mtime: i64 = row.get(2)?;
            results.push((rel_path, size, mtime));
        }

        Ok(results)
    }

    /// Count files matching extension and optional path pattern.
    ///
    /// Uses the extension index for fast counting.
    pub fn count_files_by_pattern(
        &self,
        source_id: &SourceId,
        extension: Option<&str>,
        path_pattern: Option<&str>,
    ) -> Result<i64> {
        let count = match (extension, path_pattern) {
            (Some(ext), Some(path_pat)) => {
                self.conn
                    .query_scalar::<i64>(
                        r#"SELECT COUNT(*) FROM scout_files
                           WHERE source_id = ? AND extension = ? AND rel_path LIKE ?"#,
                        &[source_id.as_i64().into(), ext.into(), path_pat.into()],
                    )
                    ?
            }
            (Some(ext), None) => {
                self.conn
                    .query_scalar::<i64>(
                        r#"SELECT COUNT(*) FROM scout_files
                           WHERE source_id = ? AND extension = ?"#,
                        &[source_id.as_i64().into(), ext.into()],
                    )
                    ?
            }
            (None, Some(path_pat)) => {
                self.conn
                    .query_scalar::<i64>(
                        r#"SELECT COUNT(*) FROM scout_files
                           WHERE source_id = ? AND rel_path LIKE ?"#,
                        &[source_id.as_i64().into(), path_pat.into()],
                    )
                    ?
            }
            (None, None) => {
                self.conn
                    .query_scalar::<i64>(
                        r#"SELECT COUNT(*) FROM scout_files
                           WHERE source_id = ?"#,
                        &[source_id.as_i64().into()],
                    )
                    ?
            }
        };
        Ok(count)
    }

    // ========================================================================
    // O(1) Folder Navigation (using parent_path index)
    // ========================================================================

    /// Get items (files and subfolders) at a specific folder path using the indexed parent_path column.
    ///
    /// O(1) lookup via index on (source_id, parent_path).
    /// Returns (name, is_folder, size) tuples for rendering in TUI.
    ///
    /// # Arguments
    /// * `source_id` - The source to query
    /// * `parent_path` - Parent directory (empty string "" for root, "a/b" for nested)
    /// * `limit` - Max items to return
    ///
    /// # Returns
    /// Vec of (name, is_folder, size) where:
    /// - name: file or folder name
    /// - is_folder: true if this is a directory (has children with this as parent)
    /// - size: file size (0 for folders)
    pub fn get_folder_contents(
        &self,
        source_id: &SourceId,
        parent_path: &str,
        limit: usize,
    ) -> Result<Vec<(String, bool, u64)>> {
        // Get all files directly in this folder
        let files = self.conn.query_all(
            r#"
            SELECT name, size
            FROM scout_files
            WHERE source_id = ? AND parent_path = ?
            ORDER BY name
            LIMIT ?
            "#,
            &[
                source_id.as_i64().into(),
                parent_path.into(),
                (limit as i64).into(),
            ],
        )?;

        let mut file_rows = Vec::with_capacity(files.len());
        for row in &files {
            let name: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            file_rows.push((name, size));
        }

        // Get unique immediate subfolders by looking at distinct parent_path prefixes
        // For parent_path "a", find all unique "a/X" where X is the immediate child folder
        let subfolder_prefix = if parent_path.is_empty() {
            String::new()
        } else {
            format!("{}/", parent_path)
        };

        let subfolders = self.conn.query_all(
            r#"
            SELECT DISTINCT
                CASE
                    WHEN ? = '' THEN SUBSTR(parent_path, 1, INSTR(parent_path || '/', '/') - 1)
                    ELSE SUBSTR(parent_path, LENGTH(?) + 1, INSTR(SUBSTR(parent_path, LENGTH(?) + 1) || '/', '/') - 1)
                END AS subfolder
            FROM scout_files
            WHERE source_id = ?
              AND parent_path LIKE ? || '%'
              AND parent_path != ?
            ORDER BY subfolder
            LIMIT ?
            "#,
            &[
                parent_path.into(),
                subfolder_prefix.as_str().into(),
                subfolder_prefix.as_str().into(),
                source_id.as_i64().into(),
                subfolder_prefix.as_str().into(),
                parent_path.into(),
                (limit as i64).into(),
            ],
        )?;

        let mut subfolder_rows = Vec::with_capacity(subfolders.len());
        for row in &subfolders {
            let name: String = row.get(0)?;
            subfolder_rows.push(name);
        }

        // Combine results: folders first, then files
        let mut results: Vec<(String, bool, u64)> = Vec::with_capacity(files.len() + subfolders.len());

        for folder_name in subfolder_rows {
            if !folder_name.is_empty() {
                results.push((folder_name, true, 0));
            }
        }

        for (name, size) in file_rows {
            results.push((name, false, size as u64));
        }

        Ok(results)
    }

    /// Count files directly in a folder (not recursive).
    /// O(1) lookup via index on (source_id, parent_path).
    pub fn count_files_in_folder(&self, source_id: &SourceId, parent_path: &str) -> Result<i64> {
        let count = self
            .conn
            .query_scalar::<i64>(
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND parent_path = ?",
                &[source_id.as_i64().into(), parent_path.into()],
            )
            ?;

        Ok(count)
    }

    // ========================================================================
    // Settings Operations
    // ========================================================================

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO scout_settings (key, value) VALUES (?, ?)",
                &[key.into(), value.into()],
            )
            ?;
        Ok(())
    }

    /// Get a setting value
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = self
            .conn
            .query_optional("SELECT value FROM scout_settings WHERE key = ?", &[key.into()])
            ?;
        match row {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    // ========================================================================
    // Extractor Operations
    // ========================================================================

    /// Upsert an extractor
    pub fn upsert_extractor(&self, extractor: &Extractor) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let paused_at = extractor.paused_at.map(|dt| dt.timestamp_millis());

        self.conn
            .execute(
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
                &[
                    extractor.id.as_str().into(),
                    extractor.name.as_str().into(),
                    extractor.source_path.as_str().into(),
                    extractor.source_hash.as_str().into(),
                    extractor.enabled.into(),
                    (extractor.timeout_secs as i64).into(),
                    (extractor.consecutive_failures as i64).into(),
                    paused_at.into(),
                    now.into(),
                    now.into(),
                ],
            )
            ?;

        Ok(())
    }

    /// Get an extractor by ID
    pub fn get_extractor(&self, id: &str) -> Result<Option<Extractor>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
                FROM scout_extractors WHERE id = ?
                "#,
                &[id.into()],
            )
            ?;

        Ok(row.map(|r| row_to_extractor(&r)))
    }

    /// Get all enabled, non-paused extractors
    pub fn get_enabled_extractors(&self) -> Result<Vec<Extractor>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
                FROM scout_extractors
                WHERE enabled = 1 AND paused_at IS NULL
                ORDER BY name
                "#,
                &[],
            )
            ?;

        Ok(rows.iter().map(row_to_extractor).collect())
    }

    /// List all extractors
    pub fn list_extractors(&self) -> Result<Vec<Extractor>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, name, source_path, source_hash, enabled, timeout_secs, consecutive_failures, paused_at, created_at, updated_at
                FROM scout_extractors
                ORDER BY name
                "#,
                &[],
            )
            ?;

        Ok(rows.iter().map(row_to_extractor).collect())
    }

    /// Pause an extractor (set paused_at to now)
    pub fn pause_extractor(&self, id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn
            .execute(
                "UPDATE scout_extractors SET paused_at = ?, updated_at = ? WHERE id = ?",
                &[now.into(), now.into(), id.into()],
            )
            ?;

        Ok(())
    }

    /// Resume a paused extractor (clear paused_at)
    pub fn resume_extractor(&self, id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn
            .execute(
                "UPDATE scout_extractors SET paused_at = NULL, consecutive_failures = 0, updated_at = ? WHERE id = ?",
                &[now.into(), id.into()],
            )
            ?;

        Ok(())
    }

    /// Update extractor consecutive failure count
    pub fn update_extractor_consecutive_failures(
        &self,
        id: &str,
        failures: u32,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn
            .execute(
                "UPDATE scout_extractors SET consecutive_failures = ?, updated_at = ? WHERE id = ?",
                &[(failures as i64).into(), now.into(), id.into()],
            )
            ?;

        Ok(())
    }

    /// Delete an extractor
    pub fn delete_extractor(&self, id: &str) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM scout_extractors WHERE id = ?", &[id.into()])
            ?;

        Ok(result > 0)
    }

    /// Get files pending extraction (ExtractionStatus::Pending)
    pub fn get_files_pending_extraction(&self) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT id, source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, tag_source, rule_id, manual_plugin, error, first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at
                FROM scout_files
                WHERE extraction_status = ?
                ORDER BY first_seen_at
                LIMIT 1000
                "#,
                &[ExtractionStatus::Pending.as_str().into()],
            )
            ?;

        Ok(rows.iter().map(Self::row_to_file).filter_map(|r| r.ok()).collect())
    }

    /// Log an extraction attempt
    pub fn log_extraction(
        &self,
        file_id: i64,
        extractor_id: &str,
        status: ExtractionLogStatus,
        duration_ms: Option<u64>,
        error_message: Option<&str>,
        metadata_snapshot: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn
            .execute(
                r#"
                INSERT INTO scout_extraction_log (file_id, extractor_id, status, duration_ms, error_message, metadata_snapshot, executed_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                &[
                    file_id.into(),
                    extractor_id.into(),
                    status.as_str().into(),
                    duration_ms.map(|d| d as i64).into(),
                    error_message.into(),
                    metadata_snapshot.into(),
                    now.into(),
                ],
            )
            ?;

        Ok(())
    }

    /// Update file extraction metadata and status
    pub fn update_file_extraction(
        &self,
        file_id: i64,
        metadata_raw: &str,
        status: ExtractionStatus,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn
            .execute(
                r#"
                UPDATE scout_files
                SET metadata_raw = ?, extraction_status = ?, extracted_at = ?, last_seen_at = ?
                WHERE id = ?
                "#,
                &[
                    metadata_raw.into(),
                    status.as_str().into(),
                    now.into(),
                    now.into(),
                    file_id.into(),
                ],
            )
            ?;

        Ok(())
    }

    /// Mark extraction as stale for files with a given extractor
    pub fn mark_extractions_stale(&self, extractor_id: &str) -> Result<u64> {
        let result = self
            .conn
            .execute(
                r#"
                UPDATE scout_files
                SET extraction_status = 'stale'
                WHERE id IN (
                    SELECT DISTINCT file_id FROM scout_extraction_log WHERE extractor_id = ?
                )
                AND extraction_status = 'extracted'
                "#,
                &[extractor_id.into()],
            )
            ?;

        Ok(result)
    }
}

/// Helper function to convert a database row to an Extractor
fn row_to_extractor(row: &casparian_db::UnifiedDbRow) -> Extractor {
    let paused_at_millis: Option<i64> = row.get(7).unwrap_or(None);
    let created_at_millis: i64 = row.get(8).unwrap_or(0);
    let updated_at_millis: i64 = row.get(9).unwrap_or(0);

    Extractor {
        id: row.get(0).unwrap_or_default(),
        name: row.get(1).unwrap_or_default(),
        source_path: row.get(2).unwrap_or_default(),
        source_hash: row.get(3).unwrap_or_default(),
        enabled: row.get(4).unwrap_or(false),
        timeout_secs: row.get::<i64>(5).unwrap_or(0) as u32,
        consecutive_failures: row.get::<i64>(6).unwrap_or(0) as u32,
        paused_at: paused_at_millis.map(millis_to_datetime),
        created_at: millis_to_datetime(created_at_millis),
        updated_at: millis_to_datetime(updated_at_millis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn test_source_crud() {
        let db = create_test_db();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };

        db.upsert_source(&source).unwrap();
        let fetched = db.get_source(&source_id).unwrap().unwrap();
        assert_eq!(fetched.name, "Test Source");
        assert_eq!(fetched.path, "/data");

        let sources = db.list_sources().unwrap();
        assert_eq!(sources.len(), 1);

        assert!(db.delete_source(&source_id).unwrap());
        assert!(db.get_source(&source_id).unwrap().is_none());
    }

    #[test]
    fn test_tagging_rule_crud() {
        let db = create_test_db();
        let source_id = SourceId::new();
        let rule_id = TaggingRuleId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        let rule = TaggingRule {
            id: rule_id.clone(),
            name: "CSV Files".to_string(),
            source_id: source_id.clone(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };

        db.upsert_tagging_rule(&rule).unwrap();
        let fetched = db.get_tagging_rule(&rule_id).unwrap().unwrap();
        assert_eq!(fetched.tag, "csv_data");
        assert_eq!(fetched.priority, 10);

        let rules = db.list_tagging_rules_for_source(&source_id).unwrap();
        assert_eq!(rules.len(), 1);

        assert!(db.delete_tagging_rule(&rule_id).unwrap());
        assert!(db.get_tagging_rule(&rule_id).unwrap().is_none());
    }

    #[test]
    fn test_file_tagging() {
        let db = create_test_db();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Direct insert test with explicit NULL
        let now_ms = chrono::Utc::now().timestamp_millis();
        db.conn
            .execute(
                "INSERT INTO scout_files (source_id, path, rel_path, parent_path, name, extension, size, mtime, content_hash, status, tag, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &[
                    source_id.as_i64().into(),
                    "/data/direct.csv".into(),
                    "direct.csv".into(),
                    "".into(),
                    "direct.csv".into(),
                    DbValue::Null, // extension
                    (1000_i64).into(),
                    (12345_i64).into(),
                    DbValue::Null, // content_hash
                    FileStatus::Pending.as_str().into(),
                    DbValue::Null, // tag - THIS IS THE KEY TEST
                    now_ms.into(),
                    now_ms.into(),
                ],
            )
            
            .unwrap();
        let file = ScannedFile::new(source_id.clone(), "/data/test.csv", "test.csv", 1000, 12345);
        let result = db.upsert_file(&file).unwrap();

        // File starts untagged
        let fetched = db.get_file(result.id).unwrap().unwrap();
        assert!(fetched.tag.is_none());
        assert_eq!(fetched.status, FileStatus::Pending);

        // Tag the file
        db.tag_file(result.id, "csv_data").unwrap();
        let fetched = db.get_file(result.id).unwrap().unwrap();
        assert_eq!(fetched.tag, Some("csv_data".to_string()));
        assert_eq!(fetched.status, FileStatus::Tagged);

        // List by tag
        let tagged = db.list_files_by_tag("csv_data", 10).unwrap();
        assert_eq!(tagged.len(), 1);
    }

    /// Test that sources are ordered by most recently used (MRU) and persist across sessions
    #[test]
    fn test_source_mru_ordering_persists() {
        let db = create_test_db();
        let source_a_id = SourceId::new();
        let source_b_id = SourceId::new();
        let source_c_id = SourceId::new();

        // Create three sources with small delays to ensure different timestamps
        let source_a = Source {
            id: source_a_id.clone(),
            name: "Source A".to_string(),
            source_type: SourceType::Local,
            path: "/data/a".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_a).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let source_b = Source {
            id: source_b_id.clone(),
            name: "Source B".to_string(),
            source_type: SourceType::Local,
            path: "/data/b".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_b).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let source_c = Source {
            id: source_c_id.clone(),
            name: "Source C".to_string(),
            source_type: SourceType::Local,
            path: "/data/c".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_c).unwrap();

        // Initial MRU order: C (most recent), B, A (oldest)
        let sources = db.list_sources_by_mru().unwrap();
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].id, source_c_id, "Most recently created should be first");
        assert_eq!(sources[1].id, source_b_id);
        assert_eq!(sources[2].id, source_a_id, "Oldest should be last");

        // Touch source A (simulates user selecting it)
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.touch_source(&source_a_id).unwrap();

        // New MRU order: A (touched), C, B
        let sources = db.list_sources_by_mru().unwrap();
        assert_eq!(sources[0].id, source_a_id, "Touched source should move to top");
        assert_eq!(sources[1].id, source_c_id);
        assert_eq!(sources[2].id, source_b_id);

        // Touch source B
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.touch_source(&source_b_id).unwrap();

        // New MRU order: B, A, C
        let sources = db.list_sources_by_mru().unwrap();
        assert_eq!(sources[0].id, source_b_id, "Most recently touched should be first");
        assert_eq!(sources[1].id, source_a_id);
        assert_eq!(sources[2].id, source_c_id);

        // Simulate "session restart" by creating a new Database instance
        // pointing to the same in-memory pool (in production, this would be
        // reconnecting to the same file-based database)
        // Note: For in-memory SQLite, we can't truly test cross-session persistence,
        // but we verify the data is correct in the current session.
        // The real test is that touch_source updates updated_at in the DB.

        // Verify by querying raw SQL to check actual updated_at values
        let ts_a: i64 = db
            .conn()
            .query_scalar(
                "SELECT updated_at FROM scout_sources WHERE id = ?",
                &[source_a_id.as_i64().into()],
            )
            
            .unwrap_or(0);
        let ts_b: i64 = db
            .conn()
            .query_scalar(
                "SELECT updated_at FROM scout_sources WHERE id = ?",
                &[source_b_id.as_i64().into()],
            )
            
            .unwrap_or(0);
        let ts_c: i64 = db
            .conn()
            .query_scalar(
                "SELECT updated_at FROM scout_sources WHERE id = ?",
                &[source_c_id.as_i64().into()],
            )
            
            .unwrap_or(0);
        assert!(ts_b > ts_a, "B should have newer timestamp than A");
        assert!(ts_a > ts_c, "A should have newer timestamp than C");
    }

    /// Test bulk insert with multiple files
    #[test]
    fn test_batch_upsert_files_bulk() {
        let db = create_test_db();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Create 150 files (tests chunking since limit is 100)
        let files: Vec<ScannedFile> = (0..150)
            .map(|i| ScannedFile::new(source_id.clone(), &format!("/data/file{}.txt", i), &format!("file{}.txt", i), 1000 + i, 12345))
            .collect();

        // First batch insert - all new
        let result = db.batch_upsert_files(&files, Some("test_tag")).unwrap();
        assert_eq!(result.new, 150, "Should have 150 new files");
        assert_eq!(result.changed, 0);
        assert_eq!(result.unchanged, 0);
        assert_eq!(result.errors, 0);

        // Verify files were inserted with tag
        let tagged = db.list_files_by_tag("test_tag", 200).unwrap();
        assert_eq!(tagged.len(), 150, "Should have 150 tagged files");

        // Second batch insert - same files, no changes
        let result = db.batch_upsert_files(&files, Some("test_tag")).unwrap();
        assert_eq!(result.new, 0);
        assert_eq!(result.changed, 0);
        assert_eq!(result.unchanged, 150, "Should have 150 unchanged files");
        assert_eq!(result.errors, 0);

        // Third batch insert - modify some files
        let modified_files: Vec<ScannedFile> = (0..150)
            .map(|i| {
                if i < 50 {
                    // First 50 files: change size
                    ScannedFile::new(source_id.clone(), &format!("/data/file{}.txt", i), &format!("file{}.txt", i), 2000 + i, 12345)
                } else {
                    // Remaining 100 files: unchanged
                    ScannedFile::new(source_id.clone(), &format!("/data/file{}.txt", i), &format!("file{}.txt", i), 1000 + i, 12345)
                }
            })
            .collect();

        let result = db.batch_upsert_files(&modified_files, Some("test_tag")).unwrap();
        assert_eq!(result.new, 0);
        assert_eq!(result.changed, 50, "Should have 50 changed files");
        assert_eq!(result.unchanged, 100, "Should have 100 unchanged files");
        assert_eq!(result.errors, 0);
    }

    /// Test batch upsert without tag (preserves existing tags)
    #[test]
    fn test_batch_upsert_files_no_tag() {
        let db = create_test_db();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Create file and tag it
        let file = ScannedFile::new(source_id.clone(), "/data/test.txt", "test.txt", 1000, 12345);
        let upsert_result = db.upsert_file(&file).unwrap();
        db.tag_file(upsert_result.id, "original_tag").unwrap();

        // Batch upsert with no tag - should preserve existing tag
        let result = db.batch_upsert_files(&[file.clone()], None).unwrap();
        assert_eq!(result.unchanged, 1);

        let fetched = db.get_file_by_path(&source_id, "/data/test.txt").unwrap().unwrap();
        assert_eq!(fetched.tag, Some("original_tag".to_string()), "Tag should be preserved");
    }

    /// Test glob_to_like_pattern conversion
    #[test]
    fn test_glob_to_like_pattern() {
        // Simple wildcards
        assert_eq!(glob_to_like_pattern("*.csv"), "%.csv");
        assert_eq!(glob_to_like_pattern("data*"), "data%");
        assert_eq!(glob_to_like_pattern("report?.csv"), "report_.csv");

        // Underscores are escaped (in glob, _ is literal; in LIKE, _ is wildcard)
        assert_eq!(glob_to_like_pattern("data_*.csv"), "data\\_%.csv"); // _ escaped, then *, then .csv
        assert_eq!(glob_to_like_pattern("report_?.csv"), "report\\__.csv"); // _ escaped, ? -> _, then .csv

        // Recursive patterns
        assert_eq!(glob_to_like_pattern("**/*.csv"), "%.csv");

        // Mixed patterns
        assert_eq!(glob_to_like_pattern("data/*.csv"), "data/%.csv");
    }

    /// Test O(1) folder navigation with parent_path
    #[test]
    fn test_folder_navigation() {
        let db = create_test_db();
        let source_id = SourceId::new();

        let source = Source {
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Create files in nested folder structure:
        // /data/root.txt
        // /data/docs/readme.md
        // /data/docs/api/spec.json
        // /data/logs/2024/jan.log
        // /data/logs/2024/feb.log
        let files = vec![
            ScannedFile::new(source_id.clone(), "/data/root.txt", "root.txt", 100, 1000),
            ScannedFile::new(source_id.clone(), "/data/docs/readme.md", "docs/readme.md", 200, 2000),
            ScannedFile::new(source_id.clone(), "/data/docs/api/spec.json", "docs/api/spec.json", 300, 3000),
            ScannedFile::new(source_id.clone(), "/data/logs/2024/jan.log", "logs/2024/jan.log", 400, 4000),
            ScannedFile::new(source_id.clone(), "/data/logs/2024/feb.log", "logs/2024/feb.log", 500, 5000),
        ];

        db.batch_upsert_files(&files, None).unwrap();

        // Verify parent_path and name are set correctly
        let root_file = db.get_file_by_path(&source_id, "/data/root.txt").unwrap().unwrap();
        assert_eq!(root_file.parent_path, "");
        assert_eq!(root_file.name, "root.txt");

        let readme = db.get_file_by_path(&source_id, "/data/docs/readme.md").unwrap().unwrap();
        assert_eq!(readme.parent_path, "docs");
        assert_eq!(readme.name, "readme.md");

        let spec = db.get_file_by_path(&source_id, "/data/docs/api/spec.json").unwrap().unwrap();
        assert_eq!(spec.parent_path, "docs/api");
        assert_eq!(spec.name, "spec.json");

        // Test O(1) folder listing at root
        let root_contents = db.get_folder_counts(&source_id, "", None).unwrap();
        // Should have: docs folder, logs folder, root.txt file
        assert!(root_contents.iter().any(|(name, _, is_file)| name == "docs" && !is_file));
        assert!(root_contents.iter().any(|(name, _, is_file)| name == "logs" && !is_file));
        assert!(root_contents.iter().any(|(name, _, is_file)| name == "root.txt" && *is_file));

        // Test folder listing at docs/
        let docs_contents = db.get_folder_counts(&source_id, "docs", None).unwrap();
        assert!(docs_contents.iter().any(|(name, _, is_file)| name == "api" && !is_file));
        assert!(docs_contents.iter().any(|(name, _, is_file)| name == "readme.md" && *is_file));

        // Test count files in folder
        let count = db.count_files_in_folder(&source_id, "logs/2024").unwrap();
        assert_eq!(count, 2);
    }

    // ========================================================================
    // Source Overlap Detection Tests
    // ========================================================================

    use crate::scout::error::ScoutError;

    #[test]
    fn test_source_overlap_no_sources() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // No existing sources - should allow any path
        let result = db.check_source_overlap(temp_dir.path());
        assert!(result.is_ok(), "Should allow source when no existing sources");
    }

    #[test]
    fn test_source_overlap_same_path() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create a source at temp_dir
        let source = Source {
            id: SourceId::new(),
            name: "Parent".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Same path is NOT overlap (it's a rescan of existing source)
        // The overlap check should pass because paths are equal, not nested
        let result = db.check_source_overlap(temp_dir.path());
        assert!(result.is_ok(), "Same path should be allowed (rescan scenario)");
    }

    #[test]
    fn test_source_overlap_child_of_existing() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create subdirectory
        let child_dir = temp_dir.path().join("projects").join("medical");
        std::fs::create_dir_all(&child_dir).unwrap();

        // Create parent source first
        let source = Source {
            id: SourceId::new(),
            name: "Projects".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Try to add child - should fail
        let result = db.check_source_overlap(&child_dir);
        assert!(result.is_err(), "Child of existing source should be rejected");

        match result.unwrap_err() {
            ScoutError::SourceIsChildOfExisting {
                new_path,
                existing_name,
                ..
            } => {
                assert!(new_path.contains("medical"));
                assert_eq!(existing_name, "Projects");
            }
            e => panic!("Expected SourceIsChildOfExisting, got: {:?}", e),
        }
    }

    #[test]
    fn test_source_overlap_parent_of_existing() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create subdirectory and make it a source first
        let child_dir = temp_dir.path().join("data").join("medical");
        std::fs::create_dir_all(&child_dir).unwrap();

        let source = Source {
            id: SourceId::new(),
            name: "Medical".to_string(),
            source_type: SourceType::Local,
            path: child_dir.display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Try to add parent - should fail
        let result = db.check_source_overlap(temp_dir.path());
        assert!(result.is_err(), "Parent of existing source should be rejected");

        match result.unwrap_err() {
            ScoutError::SourceIsParentOfExisting {
                existing_name,
                existing_path,
                ..
            } => {
                assert_eq!(existing_name, "Medical");
                assert!(existing_path.contains("medical"));
            }
            e => panic!("Expected SourceIsParentOfExisting, got: {:?}", e),
        }
    }

    #[test]
    fn test_source_overlap_sibling_allowed() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create two sibling directories
        let dir_a = temp_dir.path().join("projects_a");
        let dir_b = temp_dir.path().join("projects_b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();

        // Create source for dir_a
        let source = Source {
            id: SourceId::new(),
            name: "Projects A".to_string(),
            source_type: SourceType::Local,
            path: dir_a.display().to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // dir_b is a sibling, not nested - should be allowed
        let result = db.check_source_overlap(&dir_b);
        assert!(result.is_ok(), "Sibling directories should be allowed");
    }

    #[test]
    fn test_source_overlap_stale_source_skipped() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create a source pointing to non-existent path (stale)
        let source = Source {
            id: SourceId::new(),
            name: "Stale Source".to_string(),
            source_type: SourceType::Local,
            path: "/nonexistent/path/that/does/not/exist".to_string(),
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // New source should be allowed even though stale source exists
        // (stale source can't be canonicalized, so it's skipped)
        let result = db.check_source_overlap(temp_dir.path());
        assert!(result.is_ok(), "Should skip stale sources during overlap check");
    }

    #[test]
    fn test_source_overlap_multiple_existing() {
        let db = create_test_db();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create three separate directories
        let dir_a = temp_dir.path().join("a");
        let dir_b = temp_dir.path().join("b");
        let dir_c = temp_dir.path().join("c");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::create_dir_all(&dir_c).unwrap();

        // Create sources for a and b
        for (name, path) in [
            ("Source A", &dir_a),
            ("Source B", &dir_b),
        ] {
            let source = Source {
                id: SourceId::new(),
                name: name.to_string(),
                source_type: SourceType::Local,
                path: path.display().to_string(),
                poll_interval_secs: 30,
                enabled: true,
            };
            db.upsert_source(&source).unwrap();
        }

        // dir_c is independent - should be allowed
        let result = db.check_source_overlap(&dir_c);
        assert!(result.is_ok(), "Independent directory should be allowed");

        // Parent of all - should fail
        let result = db.check_source_overlap(temp_dir.path());
        assert!(result.is_err(), "Parent of any existing source should be rejected");
    }
}

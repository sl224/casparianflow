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
    FileTag, ParserValidationStatus, ScannedFile, Source, SourceId, SourceType, TagSource,
    TaggingRule, TaggingRuleId, UpsertResult, Workspace, WorkspaceId,
};
use casparian_ai_types::DraftStatus;
use casparian_db::{DbConnection, DbValue};
#[cfg(feature = "duckdb")]
use casparian_db::BackendError;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

/// Database schema (v2 - tag-based)
/// Note: All timestamps are stored as INTEGER (milliseconds since Unix epoch)
const SCHEMA_SQL_TEMPLATE: &str = r#"
-- Workspaces: top-level scope for sources/files/rules
CREATE TABLE IF NOT EXISTS cf_workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    default_scan_config TEXT,
    default_sink_uri TEXT,
    default_redaction TEXT,
    default_concurrency INTEGER
);

-- Sources: filesystem locations to watch (scoped to workspace)
CREATE TABLE IF NOT EXISTS scout_sources (
    id BIGINT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES cf_workspaces(id),
    name TEXT NOT NULL,
    source_type TEXT NOT NULL,
    path TEXT NOT NULL,
    exec_path TEXT,
    poll_interval_secs INTEGER NOT NULL DEFAULT 30,
    enabled INTEGER NOT NULL DEFAULT 1,
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(workspace_id, name),
    UNIQUE(workspace_id, path)
);

-- Tagging Rules: pattern → tag mappings (workspace-wide)
CREATE TABLE IF NOT EXISTS scout_rules (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES cf_workspaces(id),
    name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'tagging',
    pattern TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(workspace_id, name)
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
    workspace_id TEXT NOT NULL REFERENCES cf_workspaces(id),
    source_id BIGINT NOT NULL REFERENCES scout_sources(id),
    file_uid TEXT NOT NULL,
    path TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    parent_path TEXT NOT NULL DEFAULT '',    -- directory containing this file (for O(1) folder nav)
    name TEXT NOT NULL DEFAULT '',           -- filename only (basename of rel_path)
    extension TEXT,                          -- lowercase file extension (e.g., "csv", "json")
    is_dir INTEGER NOT NULL DEFAULT 0,       -- 1 for directory entries, 0 for files
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    content_hash TEXT,
    status TEXT NOT NULL DEFAULT '__FILE_STATUS_DEFAULT__'
        CHECK (status IN (__FILE_STATUS_VALUES__)),
    status_before_delete TEXT
        CHECK (status_before_delete IS NULL OR status_before_delete IN (__FILE_STATUS_VALUES__)),
    manual_plugin TEXT,
    error TEXT,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    missing_scans BIGINT NOT NULL DEFAULT 0,
    deleted_at INTEGER,
    processed_at INTEGER,
    sentinel_job_id INTEGER,
    -- Extractor metadata (Phase 6)
    metadata_raw TEXT,                           -- JSON blob of extracted metadata
    extraction_status TEXT DEFAULT '__EXTRACTION_STATUS_DEFAULT__'     -- pending, extracted, timeout, crash, stale, error
        CHECK (extraction_status IN (__EXTRACTION_STATUS_VALUES__)),
    extracted_at INTEGER,                        -- timestamp of last extraction
    UNIQUE(source_id, path)
);

-- File tags: multi-tag assignments (manual or rule-based)
CREATE TABLE IF NOT EXISTS scout_file_tags (
    workspace_id TEXT NOT NULL REFERENCES cf_workspaces(id),
    file_id BIGINT NOT NULL REFERENCES scout_files(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    tag_source TEXT NOT NULL CHECK (tag_source IN ('rule', 'manual')),
    rule_id TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, file_id, tag)
);

-- Folder hierarchy for O(1) TUI navigation (streaming scanner)
-- Replaces file-based FolderCache (.bin.zst files)
CREATE TABLE IF NOT EXISTS scout_folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL REFERENCES cf_workspaces(id),
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
CREATE INDEX IF NOT EXISTS idx_files_workspace ON scout_files(workspace_id);
CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status);
CREATE INDEX IF NOT EXISTS idx_files_extension ON scout_files(source_id, extension);
CREATE INDEX IF NOT EXISTS idx_files_mtime ON scout_files(mtime);
CREATE INDEX IF NOT EXISTS idx_files_path ON scout_files(path);
CREATE INDEX IF NOT EXISTS idx_files_uid ON scout_files(source_id, file_uid);
CREATE INDEX IF NOT EXISTS idx_files_last_seen ON scout_files(last_seen_at);
CREATE INDEX IF NOT EXISTS idx_files_manual_plugin ON scout_files(manual_plugin);
CREATE INDEX IF NOT EXISTS idx_rules_workspace ON scout_rules(workspace_id);
CREATE INDEX IF NOT EXISTS idx_rules_priority ON scout_rules(priority DESC);
CREATE INDEX IF NOT EXISTS idx_file_tags_workspace_tag ON scout_file_tags(workspace_id, tag);
CREATE INDEX IF NOT EXISTS idx_file_tags_file ON scout_file_tags(file_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_rule ON scout_file_tags(rule_id);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_updated ON parser_lab_parsers(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_status ON parser_lab_parsers(validation_status);
CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_pattern ON parser_lab_parsers(file_pattern);
CREATE INDEX IF NOT EXISTS idx_parser_lab_test_files_parser ON parser_lab_test_files(parser_id);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_source ON extraction_rules(source_id);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_pattern ON extraction_rules(glob_pattern);
CREATE INDEX IF NOT EXISTS idx_extraction_rules_enabled ON extraction_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_extraction_fields_rule ON extraction_fields(rule_id);
CREATE INDEX IF NOT EXISTS idx_extraction_tag_conditions_rule ON extraction_tag_conditions(rule_id);
CREATE INDEX IF NOT EXISTS idx_scout_folders_lookup ON scout_folders(workspace_id, source_id, prefix);

-- O(1) folder navigation index: lookup files by parent directory
CREATE INDEX IF NOT EXISTS idx_files_parent_path ON scout_files(source_id, parent_path);

-- Composite indexes for Rule Builder queries (critical for large sources)
-- Used by: load_scout_files() ORDER BY rel_path
CREATE INDEX IF NOT EXISTS idx_files_source_relpath ON scout_files(source_id, rel_path);

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

const FILE_SELECT_COLUMNS: &str = "id, workspace_id, source_id, file_uid, path, rel_path, parent_path, name, extension, is_dir, size, mtime, content_hash, status, manual_plugin, error, first_seen_at, last_seen_at, processed_at, sentinel_job_id, metadata_raw, extraction_status, extracted_at";

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
        " REFERENCES cf_workspaces(id)",
        " REFERENCES scout_sources(id) ON DELETE CASCADE",
        " REFERENCES scout_sources(id)",
        " REFERENCES scout_files(id) ON DELETE CASCADE",
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
        let needle = format!(
            "CREATE TABLE IF NOT EXISTS {table} (\n    id INTEGER PRIMARY KEY AUTOINCREMENT,"
        );
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
        if trimmed.starts_with("UNIQUE(") || trimmed.starts_with("PRIMARY KEY") {
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
CREATE UNIQUE INDEX IF NOT EXISTS uniq_cf_workspaces_name ON cf_workspaces(name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_sources_id ON scout_sources(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_sources_workspace_name ON scout_sources(workspace_id, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_sources_workspace_path ON scout_sources(workspace_id, path);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_rules_id ON scout_rules(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_rules_workspace_name ON scout_rules(workspace_id, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_settings_key ON scout_settings(key);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_files_source_path ON scout_files(source_id, path);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_folders_source_prefix_name ON scout_folders(workspace_id, source_id, prefix, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_extractors_id ON scout_extractors(id);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_extractors_name ON scout_extractors(name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_parser_lab_test_files_parser_path ON parser_lab_test_files(parser_id, file_path);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_extraction_rules_source_name ON extraction_rules(source_id, name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_extraction_fields_rule_name ON extraction_fields(rule_id, field_name);
CREATE UNIQUE INDEX IF NOT EXISTS uniq_scout_file_tags_workspace_file_tag ON scout_file_tags(workspace_id, file_id, tag);
"#;

    format!("{sequences}\n{}\n{unique_indexes}", output.join("\n"))
}

fn column_exists(conn: &DbConnection, table: &str, column: &str) -> Result<bool> {
    conn.column_exists(table, column)
        .map_err(ScoutError::Database)
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
        let conn = DbConnection::open_sqlite(path)?;

        let schema_sql = schema_sql(false);
        conn.execute_batch(&schema_sql)?;
        Self::validate_schema(&conn)?;

        Ok(Self {
            conn,
            _temp_dir: None,
        })
    }

    /// Open or create a database with a custom busy timeout (milliseconds).
    pub fn open_with_busy_timeout(path: &Path, busy_timeout_ms: u64) -> Result<Self> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(path, busy_timeout_ms)?;

        let schema_sql = schema_sql(false);
        conn.execute_batch(&schema_sql)?;
        Self::validate_schema(&conn)?;

        Ok(Self {
            conn,
            _temp_dir: None,
        })
    }

    /// Validate schema columns and fail loud if the DB is outdated (pre-v1 policy).
    fn validate_schema(conn: &DbConnection) -> Result<()> {
        let required_columns = [
            "workspace_id",
            "metadata_raw",
            "extraction_status",
            "extracted_at",
            "parent_path",
            "name",
            "extension",
            "is_dir",
            "file_uid",
            "missing_scans",
            "status_before_delete",
            "deleted_at",
        ];
        let mut missing = Vec::new();
        for col in required_columns {
            if !column_exists(conn, "scout_files", col)? {
                missing.push(col);
            }
        }

        if missing.is_empty() {
            // fall through to scout_sources validation
        } else {
            return Err(ScoutError::Config(format!(
                "Database schema for 'scout_files' is missing columns: {}. \
Delete the database (default: ~/.casparian_flow/state.sqlite) and restart.",
                missing.join(", ")
            )));
        }

        let required_source_columns = ["exec_path"];
        let mut source_missing = Vec::new();
        for col in required_source_columns {
            if !column_exists(conn, "scout_sources", col)? {
                source_missing.push(col);
            }
        }

        if source_missing.is_empty() {
            return Ok(());
        }

        Err(ScoutError::Config(format!(
            "Database schema for 'scout_sources' is missing columns: {}. \
Delete the database (default: ~/.casparian_flow/state.sqlite) and restart.",
            source_missing.join(", ")
        )))
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let temp_dir = Arc::new(TempDir::new()?);
        let db_path = temp_dir.path().join("scout.sqlite");
        let conn = DbConnection::open_sqlite(&db_path)?;
        let schema_sql = schema_sql(false);
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
    // Workspace Operations
    // ========================================================================

    /// Create a workspace with optional defaults.
    pub fn create_workspace(&self, name: &str) -> Result<Workspace> {
        let workspace = Workspace {
            id: WorkspaceId::new(),
            name: name.to_string(),
            created_at: Utc::now(),
        };

        self.conn.execute(
            r#"
            INSERT INTO cf_workspaces (
                id, name, created_at, default_scan_config, default_sink_uri, default_redaction, default_concurrency
            )
            VALUES (?, ?, ?, NULL, NULL, NULL, NULL)
            "#,
            &[
                DbValue::from(workspace.id.to_string()),
                DbValue::from(workspace.name.as_str()),
                DbValue::from(workspace.created_at.timestamp_millis()),
            ],
        )?;

        Ok(workspace)
    }

    /// Get a workspace by ID.
    pub fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        let row = self.conn.query_optional(
            "SELECT id, name, created_at FROM cf_workspaces WHERE id = ?",
            &[DbValue::from(id.to_string())],
        )?;
        match row {
            Some(row) => Ok(Some(Self::row_to_workspace(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a workspace by name.
    pub fn get_workspace_by_name(&self, name: &str) -> Result<Option<Workspace>> {
        let row = self.conn.query_optional(
            "SELECT id, name, created_at FROM cf_workspaces WHERE name = ?",
            &[DbValue::from(name)],
        )?;
        match row {
            Some(row) => Ok(Some(Self::row_to_workspace(&row)?)),
            None => Ok(None),
        }
    }

    /// List workspaces ordered by creation time.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let rows = self.conn.query_all(
            "SELECT id, name, created_at FROM cf_workspaces ORDER BY created_at ASC",
            &[],
        )?;
        rows.iter().map(Self::row_to_workspace).collect()
    }

    /// Ensure there is at least one workspace. Returns the default workspace.
    pub fn ensure_default_workspace(&self) -> Result<Workspace> {
        if let Some(existing) = self.get_workspace_by_name("Default")? {
            return Ok(existing);
        }
        let existing = self.list_workspaces()?;
        if let Some(first) = existing.into_iter().next() {
            return Ok(first);
        }
        self.create_workspace("Default")
    }

    fn row_to_workspace(row: &casparian_db::UnifiedDbRow) -> Result<Workspace> {
        let id_raw: String = row.get(0)?;
        let id = WorkspaceId::parse(&id_raw)?;
        let created_at_millis: i64 = row.get(2)?;
        Ok(Workspace {
            id,
            name: row.get(1)?,
            created_at: millis_to_datetime(created_at_millis),
        })
    }

    // ========================================================================
    // Source Operations
    // ========================================================================

    /// Insert or update a source
    pub fn upsert_source(&self, source: &Source) -> Result<()> {
        let source_type_json = serde_json::to_string(&source.source_type)?;
        let now = now_millis();

        self.conn.execute(
            r#"
                INSERT INTO scout_sources (
                    id,
                    workspace_id,
                    name,
                    source_type,
                    path,
                    exec_path,
                    poll_interval_secs,
                    enabled,
                    created_at,
                    updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    workspace_id = excluded.workspace_id,
                    name = excluded.name,
                    source_type = excluded.source_type,
                    path = excluded.path,
                    exec_path = excluded.exec_path,
                    poll_interval_secs = excluded.poll_interval_secs,
                    enabled = excluded.enabled,
                    updated_at = excluded.updated_at
                "#,
            &[
                source.id.as_i64().into(),
                DbValue::from(source.workspace_id.to_string()),
                source.name.as_str().into(),
                source_type_json.into(),
                source.path.as_str().into(),
                DbValue::from(Self::normalize_exec_path(&source.exec_path)),
                (source.poll_interval_secs as i64).into(),
                source.enabled.into(),
                now.into(),
                now.into(),
            ],
        )?;

        Ok(())
    }

    /// Update the `updated_at` timestamp for a source (for MRU ordering)
    /// Called when a source is scanned or selected to bring it to the top of the list
    pub fn touch_source(&self, id: &SourceId) -> Result<()> {
        let now = now_millis();
        self.conn.execute(
            "UPDATE scout_sources SET updated_at = ? WHERE id = ?",
            &[now.into(), id.as_i64().into()],
        )?;
        Ok(())
    }

    /// Update the file_count for a source (called after scanning)
    /// This is stored directly in scout_sources so listing sources is O(sources) not O(files)
    pub fn update_source_file_count(&self, id: &SourceId, file_count: usize) -> Result<()> {
        self.conn.execute(
            "UPDATE scout_sources SET file_count = ? WHERE id = ?",
            &[(file_count as i64).into(), id.as_i64().into()],
        )?;
        Ok(())
    }

    fn get_source_workspace_id(&self, id: &SourceId) -> Result<WorkspaceId> {
        let row = self.conn.query_optional(
            "SELECT workspace_id FROM scout_sources WHERE id = ?",
            &[id.as_i64().into()],
        )?;
        let Some(row) = row else {
            return Err(ScoutError::SourceNotFound(id.to_string()));
        };
        let workspace_raw: String = row.get(0)?;
        WorkspaceId::parse(&workspace_raw).map_err(ScoutError::from)
    }

    /// Populate scout_folders table for O(1) TUI navigation (called after scanning)
    /// This pre-computes the folder hierarchy so get_folder_counts doesn't need to scan all files
    pub fn populate_folder_cache(&self, source_id: &SourceId) -> Result<()> {
        let workspace_id = self.get_source_workspace_id(source_id)?;
        // Clear existing folder cache for this source
        self.conn.execute(
            "DELETE FROM scout_folders WHERE workspace_id = ? AND source_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                source_id.as_i64().into(),
            ],
        )?;

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
            WHERE workspace_id = ? AND source_id = ? AND parent_path <> ''
            GROUP BY folder_name
            ORDER BY file_count DESC
            LIMIT 500
            "#,
            &[
                DbValue::from(workspace_id.to_string()),
                source_id.as_i64().into(),
            ],
        )?;

        let mut root_folder_rows = Vec::with_capacity(root_folders.len());
        for row in root_folders {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            root_folder_rows.push((name, count));
        }

        let now = now_millis();
        let source_id_i64 = source_id.as_i64();
        let workspace_id_str = workspace_id.to_string();
        const FOLDER_COLUMNS: [&str; 7] = [
            "workspace_id",
            "source_id",
            "prefix",
            "name",
            "file_count",
            "is_folder",
            "updated_at",
        ];

        let mut folder_rows = Vec::with_capacity(root_folder_rows.len());
        for (name, count) in &root_folder_rows {
            folder_rows.push(vec![
                DbValue::from(workspace_id_str.as_str()),
                DbValue::from(source_id_i64),
                DbValue::from(""),
                DbValue::from(name.as_str()),
                DbValue::from(*count),
                DbValue::from(1_i64),
                DbValue::from(now),
            ]);
        }
        self.conn
            .bulk_insert_rows("scout_folders", &FOLDER_COLUMNS, &folder_rows)?;

        // Also add root-level files (files with empty parent_path)
        let root_files = self
            .conn
            .query_all(
                "SELECT name FROM scout_files WHERE workspace_id = ? AND source_id = ? AND parent_path = '' ORDER BY name LIMIT 200",
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                ],
            )
            ?;

        let mut root_file_names = Vec::with_capacity(root_files.len());
        for row in root_files {
            let name: String = row.get(0)?;
            root_file_names.push(name);
        }

        let mut file_rows = Vec::with_capacity(root_file_names.len());
        for name in &root_file_names {
            file_rows.push(vec![
                DbValue::from(workspace_id_str.as_str()),
                DbValue::from(source_id_i64),
                DbValue::from(""),
                DbValue::from(name.as_str()),
                DbValue::from(1_i64),
                DbValue::from(0_i64),
                DbValue::from(now),
            ]);
        }
        self.conn
            .bulk_insert_rows("scout_folders", &FOLDER_COLUMNS, &file_rows)?;

        tracing::info!(
            source_id = %source_id,
            root_folders = root_folder_rows.len(),
            root_files = root_file_names.len(),
            "Populated folder cache"
        );

        Ok(())
    }

    /// Populate scout_folders cache using aggregates collected during scan.
    /// This avoids a DB-wide GROUP BY after every scan.
    pub fn populate_folder_cache_from_aggregates(
        &self,
        source_id: &SourceId,
        root_folder_counts: &std::collections::HashMap<String, u64>,
        root_file_names: &[String],
    ) -> Result<()> {
        let workspace_id = self.get_source_workspace_id(source_id)?;
        self.conn.execute(
            "DELETE FROM scout_folders WHERE workspace_id = ? AND source_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                source_id.as_i64().into(),
            ],
        )?;

        let mut folder_rows = Vec::with_capacity(root_folder_counts.len());
        for (name, count) in root_folder_counts {
            let count_i64 = i64::try_from(*count).map_err(|_| {
                ScoutError::InvalidState("folder cache count out of range".to_string())
            })?;
            folder_rows.push((name.clone(), count_i64));
        }

        folder_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let folders_truncated = folder_rows.len().saturating_sub(500);
        if folder_rows.len() > 500 {
            folder_rows.truncate(500);
        }

        let mut root_files = root_file_names.to_vec();
        root_files.sort();
        let files_truncated = root_files.len().saturating_sub(200);
        if root_files.len() > 200 {
            root_files.truncate(200);
        }

        let now = now_millis();
        let source_id_i64 = source_id.as_i64();
        let workspace_id_str = workspace_id.to_string();
        let root_folders_len = folder_rows.len();
        let root_files_len = root_files.len();

        const FOLDER_COLUMNS: [&str; 7] = [
            "workspace_id",
            "source_id",
            "prefix",
            "name",
            "file_count",
            "is_folder",
            "updated_at",
        ];

        let mut rows = Vec::with_capacity(folder_rows.len() + root_files.len());
        for (name, count) in &folder_rows {
            rows.push(vec![
                DbValue::from(workspace_id_str.as_str()),
                DbValue::from(source_id_i64),
                DbValue::from(""),
                DbValue::from(name.as_str()),
                DbValue::from(*count),
                DbValue::from(1_i64),
                DbValue::from(now),
            ]);
        }
        for name in &root_files {
            rows.push(vec![
                DbValue::from(workspace_id_str.as_str()),
                DbValue::from(source_id_i64),
                DbValue::from(""),
                DbValue::from(name.as_str()),
                DbValue::from(1_i64),
                DbValue::from(0_i64),
                DbValue::from(now),
            ]);
        }
        self.conn
            .bulk_insert_rows("scout_folders", &FOLDER_COLUMNS, &rows)?;

        tracing::info!(
            source_id = %source_id,
            root_folders = root_folders_len,
            root_files = root_files_len,
            folders_truncated = folders_truncated,
            files_truncated = files_truncated,
            "Populated folder cache from aggregates"
        );

        Ok(())
    }

    /// Get folder counts from scout_folders cache (O(1) lookup)
    /// Returns None if cache is not populated for this source
    pub fn get_folder_counts_from_cache(
        &self,
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        prefix: &str,
    ) -> Result<Option<Vec<(String, i64, bool)>>> {
        // Check if cache exists for this source/prefix
        let rows = self
            .conn
            .query_all(
                "SELECT name, file_count, is_folder FROM scout_folders WHERE workspace_id = ? AND source_id = ? AND prefix = ? ORDER BY is_folder DESC, file_count DESC, name",
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    prefix.into(),
                ],
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
            let source_exists = self.conn.query_optional(
                "SELECT 1 FROM scout_sources WHERE workspace_id = ? AND id = ?",
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                ],
            )?;

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
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE id = ?",
                &[id.as_i64().into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by name within a workspace
    pub fn get_source_by_name(
        &self,
        workspace_id: &WorkspaceId,
        name: &str,
    ) -> Result<Option<Source>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE workspace_id = ? AND name = ?",
                &[DbValue::from(workspace_id.to_string()), name.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a source by path within a workspace
    pub fn get_source_by_path(
        &self,
        workspace_id: &WorkspaceId,
        path: &str,
    ) -> Result<Option<Source>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE workspace_id = ? AND path = ?",
                &[DbValue::from(workspace_id.to_string()), path.into()],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// List all sources in a workspace
    pub fn list_sources(&self, workspace_id: &WorkspaceId) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE workspace_id = ? ORDER BY name",
                &[DbValue::from(workspace_id.to_string())],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// List enabled sources in a workspace
    pub fn list_enabled_sources(&self, workspace_id: &WorkspaceId) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE workspace_id = ? AND enabled = 1 ORDER BY name",
                &[DbValue::from(workspace_id.to_string())],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// List enabled sources ordered by most recently used (updated_at DESC)
    /// This is used by the TUI to show recently accessed sources first
    pub fn list_sources_by_mru(&self, workspace_id: &WorkspaceId) -> Result<Vec<Source>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, workspace_id, name, source_type, path, exec_path, poll_interval_secs, enabled FROM scout_sources WHERE workspace_id = ? AND enabled = 1 ORDER BY updated_at DESC",
                &[DbValue::from(workspace_id.to_string())],
            )
            ?;

        rows.iter().map(Self::row_to_source).collect()
    }

    /// Delete a source and all associated data
    pub fn delete_source(&self, id: &SourceId) -> Result<bool> {
        // Delete associated tags and files first
        self.conn.execute(
            "DELETE FROM scout_file_tags WHERE file_id IN (SELECT id FROM scout_files WHERE source_id = ?)",
            &[id.as_i64().into()],
        )?;
        self.conn.execute(
            "DELETE FROM scout_files WHERE source_id = ?",
            &[id.as_i64().into()],
        )?;
        let result = self.conn.execute(
            "DELETE FROM scout_sources WHERE id = ?",
            &[id.as_i64().into()],
        )?;

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
    pub fn check_source_overlap(&self, workspace_id: &WorkspaceId, new_path: &Path) -> Result<()> {
        use super::error::ScoutError;

        // Canonicalize the new path to resolve symlinks, `.`, `..`, etc.
        let new_canonical = new_path.canonicalize().map_err(|e| {
            ScoutError::Config(format!(
                "Cannot resolve path '{}': {}",
                new_path.display(),
                e
            ))
        })?;

        let existing_sources = self.list_sources(workspace_id)?;

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

    fn normalize_exec_path(exec_path: &Option<String>) -> Option<String> {
        exec_path.as_ref().and_then(|path| {
            if path.trim().is_empty() {
                None
            } else {
                Some(path.clone())
            }
        })
    }

    fn row_to_source(row: &casparian_db::UnifiedDbRow) -> Result<Source> {
        let source_type_json: String = row.get(3)?;
        let source_type: SourceType = serde_json::from_str(&source_type_json)?;
        let exec_path: Option<String> = row.get(5)?;
        let poll_interval: i64 = row.get(6)?;
        let enabled: i64 = row.get(7)?;

        let id_raw: i64 = row.get(0)?;
        let id = SourceId::try_from(id_raw)?;

        let workspace_id_raw: String = row.get(1)?;
        let workspace_id = WorkspaceId::parse(&workspace_id_raw)?;

        Ok(Source {
            workspace_id,
            id,
            name: row.get(2)?,
            source_type,
            path: row.get(4)?,
            exec_path: Self::normalize_exec_path(&exec_path),
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

        self.conn.execute(
            r#"
                INSERT INTO scout_rules (
                    id,
                    workspace_id,
                    name,
                    kind,
                    pattern,
                    tag,
                    priority,
                    enabled,
                    created_at,
                    updated_at
                )
                VALUES (?, ?, ?, 'tagging', ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    workspace_id = excluded.workspace_id,
                    name = excluded.name,
                    kind = excluded.kind,
                    pattern = excluded.pattern,
                    tag = excluded.tag,
                    priority = excluded.priority,
                    enabled = excluded.enabled,
                    updated_at = excluded.updated_at
                "#,
            &[
                DbValue::from(rule.id.to_string()),
                DbValue::from(rule.workspace_id.to_string()),
                rule.name.as_str().into(),
                rule.pattern.as_str().into(),
                rule.tag.as_str().into(),
                (rule.priority as i64).into(),
                rule.enabled.into(),
                now.into(),
                now.into(),
            ],
        )?;

        Ok(())
    }

    /// Get a tagging rule by ID
    pub fn get_tagging_rule(&self, id: &TaggingRuleId) -> Result<Option<TaggingRule>> {
        let row = self
            .conn
            .query_optional(
                "SELECT id, workspace_id, name, pattern, tag, priority, enabled FROM scout_rules WHERE id = ? AND kind = 'tagging'",
                &[DbValue::from(id.to_string())],
            )
            ?;

        match row {
            Some(row) => Ok(Some(Self::row_to_tagging_rule(&row)?)),
            None => Ok(None),
        }
    }

    /// List all tagging rules in a workspace
    pub fn list_tagging_rules(&self, workspace_id: &WorkspaceId) -> Result<Vec<TaggingRule>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, workspace_id, name, pattern, tag, priority, enabled FROM scout_rules WHERE workspace_id = ? AND kind = 'tagging' ORDER BY priority DESC, name",
                &[DbValue::from(workspace_id.to_string())],
            )
            ?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// List enabled tagging rules for a workspace (ordered by priority)
    pub fn list_tagging_rules_for_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<TaggingRule>> {
        let rows = self
            .conn
            .query_all(
                "SELECT id, workspace_id, name, pattern, tag, priority, enabled FROM scout_rules WHERE workspace_id = ? AND kind = 'tagging' AND enabled = 1 ORDER BY priority DESC, name",
                &[DbValue::from(workspace_id.to_string())],
            )
            ?;

        rows.iter().map(Self::row_to_tagging_rule).collect()
    }

    /// Delete a tagging rule
    pub fn delete_tagging_rule(&self, id: &TaggingRuleId) -> Result<bool> {
        let result = self.conn.execute(
            "DELETE FROM scout_rules WHERE id = ?",
            &[DbValue::from(id.to_string())],
        )?;

        Ok(result > 0)
    }

    fn row_to_tagging_rule(row: &casparian_db::UnifiedDbRow) -> Result<TaggingRule> {
        let enabled: i64 = row.get(6)?;
        let id_raw: String = row.get(0)?;
        let id = TaggingRuleId::parse(&id_raw)?;
        let workspace_raw: String = row.get(1)?;
        let workspace_id = WorkspaceId::parse(&workspace_raw)?;
        Ok(TaggingRule {
            id,
            name: row.get(2)?,
            workspace_id,
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
        let existing = self.conn.query_optional(
            "SELECT id, size, mtime, status FROM scout_files WHERE source_id = ? AND file_uid = ?",
            &[
                file.source_id.as_i64().into(),
                DbValue::from(file.file_uid.as_str()),
            ],
        )?;

        let now = now_millis();
        match existing {
            None => {
                // New file
                self.conn.execute(
                    r#"
                    INSERT INTO scout_files (
                        workspace_id,
                        source_id,
                        file_uid,
                        path,
                        rel_path,
                        parent_path,
                        name,
                        extension,
                        is_dir,
                        size,
                        mtime,
                        content_hash,
                        status,
                        first_seen_at,
                        last_seen_at
                    )
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                    &[
                        DbValue::from(file.workspace_id.to_string()),
                        file.source_id.as_i64().into(),
                        DbValue::from(file.file_uid.as_str()),
                        file.path.as_str().into(),
                        file.rel_path.as_str().into(),
                        file.parent_path.as_str().into(),
                        file.name.as_str().into(),
                        file.extension.as_deref().into(),
                        (file.is_dir as i64).into(),
                        (file.size as i64).into(),
                        file.mtime.into(),
                        file.content_hash.as_deref().into(),
                        FileStatus::Pending.as_str().into(),
                        now.into(),
                        now.into(),
                    ],
                )?;

                let id: i64 = self.conn.query_scalar(
                    "SELECT id FROM scout_files WHERE source_id = ? AND file_uid = ?",
                    &[
                        file.source_id.as_i64().into(),
                        DbValue::from(file.file_uid.as_str()),
                    ],
                )?;

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
                    self.conn.execute(
                        r#"
                        UPDATE scout_files SET
                            workspace_id = ?,
                            path = ?,
                            rel_path = ?,
                            parent_path = ?,
                            name = ?,
                            extension = ?,
                            is_dir = ?,
                            size = ?,
                            mtime = ?,
                            content_hash = ?,
                            file_uid = ?,
                            status = ?,
                            error = NULL,
                            sentinel_job_id = NULL,
                            last_seen_at = ?
                        WHERE id = ?
                        "#,
                        &[
                            DbValue::from(file.workspace_id.to_string()),
                            file.path.as_str().into(),
                            file.rel_path.as_str().into(),
                            file.parent_path.as_str().into(),
                            file.name.as_str().into(),
                            file.extension.as_deref().into(),
                            (file.is_dir as i64).into(),
                            (file.size as i64).into(),
                            file.mtime.into(),
                            file.content_hash.as_deref().into(),
                            DbValue::from(file.file_uid.as_str()),
                            FileStatus::Pending.as_str().into(),
                            now.into(),
                            id.into(),
                        ],
                    )?;
                    // Clear any existing tags when file changes (re-tag on next run)
                    self.conn.execute(
                        "DELETE FROM scout_file_tags WHERE file_id = ?",
                        &[id.into()],
                    )?;
                } else {
                    // Just update last_seen_at
                    self.conn.execute(
                        r#"
                        UPDATE scout_files SET
                            workspace_id = ?,
                            path = ?,
                            rel_path = ?,
                            parent_path = ?,
                            name = ?,
                            extension = ?,
                            is_dir = ?,
                            last_seen_at = ?,
                            file_uid = ?
                        WHERE id = ?
                        "#,
                        &[
                            DbValue::from(file.workspace_id.to_string()),
                            file.path.as_str().into(),
                            file.rel_path.as_str().into(),
                            file.parent_path.as_str().into(),
                            file.name.as_str().into(),
                            file.extension.as_deref().into(),
                            (file.is_dir as i64).into(),
                            now.into(),
                            DbValue::from(file.file_uid.as_str()),
                            id.into(),
                        ],
                    )?;
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
        compute_stats: bool,
    ) -> Result<BatchUpsertResult> {
        if files.is_empty() {
            return Ok(BatchUpsertResult::default());
        }

        if self.conn.backend_name() == "DuckDB" {
            return self.batch_upsert_files_duckdb(files, tag, compute_stats);
        }

        let now = now_millis();
        let files = files.to_vec();
        let tag = tag.map(|value| value.to_string());
        let source_id = files[0].source_id;
        let workspace_id = files[0].workspace_id;
        let tag_paths = if tag.is_some() {
            files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let stats = self.conn.transaction(move |tx| {
                let mut stats = BatchUpsertResult::default();

                let existing_by_uid = Self::query_existing_files_by_uid_tx(tx, &source_id, &files)?;
                Self::apply_rename_updates_tx(tx, &files, &existing_by_uid, now)?;

                // Query existing files to determine new vs changed vs unchanged
                // Note: This SELECT also needs chunking for large batches
                let existing = if compute_stats {
                    Some(Self::query_existing_files_tx(tx, &source_id, &files)?)
                } else {
                    None
                };

                // Chunk size for bulk inserts. Modern SQLite supports 32766 params (since 3.32.0).
                // 100 rows per chunk is a good balance between fewer round-trips and memory usage.
                const CHUNK_SIZE: usize = 100;

                for chunk in files.chunks(CHUNK_SIZE) {
                    // Pre-compute stats for this chunk (assuming all succeed)
                    let mut chunk_new = 0u64;
                    let mut chunk_changed = 0u64;
                    let mut chunk_unchanged = 0u64;

                    if let Some(existing) = existing.as_ref() {
                        for file in chunk {
                            let is_new = !existing.contains_key(&file.path);
                            let is_changed = existing.get(&file.path).is_some_and(|(size, mtime)| {
                                *size != file.size as i64 || *mtime != file.mtime
                            });

                            if is_new {
                                chunk_new += 1;
                            } else if is_changed {
                                chunk_changed += 1;
                            } else {
                                chunk_unchanged += 1;
                            }
                        }
                    }

                    // Try bulk insert
                    match Self::bulk_insert_chunk_tx(tx, chunk, now) {
                        Ok(()) => {
                            if compute_stats {
                                stats.new += chunk_new;
                                stats.changed += chunk_changed;
                                stats.unchanged += chunk_unchanged;
                            }
                        }
                        Err(e) => {
                            // Bulk failed - fall back to row-by-row to isolate bad row
                            tracing::debug!(error = %e, "Bulk insert failed, falling back to row-by-row");
                            Self::insert_rows_individually_tx(
                                tx,
                                chunk,
                                now,
                                existing.as_ref(),
                                &mut stats,
                            );
                        }
                    }
                }

                Ok(stats)
            })
            .map_err(ScoutError::from)?;

        if let Some(tag_value) = tag.as_deref() {
            self.tag_paths_for_source(
                &workspace_id,
                &source_id,
                &tag_paths,
                tag_value,
                TagSource::Manual,
                None,
            )?;
        }

        Ok(stats)
    }

    fn batch_upsert_files_duckdb(
        &self,
        files: &[ScannedFile],
        tag: Option<&str>,
        compute_stats: bool,
    ) -> Result<BatchUpsertResult> {
        #[cfg(feature = "duckdb")]
        {
            let now = now_millis();
            let pending_status = FileStatus::Pending.as_str();
            let workspace_id = files[0].workspace_id;
            let source_id = files[0].source_id;
            let tag_paths = if tag.is_some() {
                files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let stats = self
                .conn
                .transaction(|tx| {
                    let mut stats = BatchUpsertResult::default();
                    tx.execute_batch(&format!(
                        "CREATE TEMP TABLE IF NOT EXISTS staging_scout_files (
                            workspace_id TEXT NOT NULL,
                            source_id BIGINT NOT NULL,
                            file_uid TEXT NOT NULL,
                            path TEXT NOT NULL,
                            rel_path TEXT NOT NULL,
                            parent_path TEXT NOT NULL DEFAULT '',
                            name TEXT NOT NULL DEFAULT '',
                            extension TEXT,
                            is_dir BIGINT NOT NULL DEFAULT 0,
                            size BIGINT NOT NULL,
                            mtime BIGINT NOT NULL,
                            content_hash TEXT,
                            status TEXT NOT NULL DEFAULT '{pending_status}',
                            first_seen_at BIGINT NOT NULL,
                            last_seen_at BIGINT NOT NULL
                        );
                        DELETE FROM staging_scout_files;",
                        pending_status = pending_status
                    ))?;

                    const STAGING_COLUMNS: [&str; 15] = [
                        "workspace_id",
                        "source_id",
                        "file_uid",
                        "path",
                        "rel_path",
                        "parent_path",
                        "name",
                        "extension",
                        "is_dir",
                        "size",
                        "mtime",
                        "content_hash",
                        "status",
                        "first_seen_at",
                        "last_seen_at",
                    ];

                    let workspace_id_str = workspace_id.to_string();
                    let mut rows = Vec::with_capacity(files.len());
                    for file in files {
                        rows.push(vec![
                            DbValue::from(workspace_id_str.as_str()),
                            DbValue::from(file.source_id.as_i64()),
                            DbValue::from(file.file_uid.as_str()),
                            DbValue::from(file.path.as_str()),
                            DbValue::from(file.rel_path.as_str()),
                            DbValue::from(file.parent_path.as_str()),
                            DbValue::from(file.name.as_str()),
                            DbValue::from(file.extension.as_deref()),
                            DbValue::from(file.is_dir as i64),
                            DbValue::from(file.size as i64),
                            DbValue::from(file.mtime),
                            DbValue::from(file.content_hash.as_deref()),
                            DbValue::from(pending_status),
                            DbValue::from(now),
                            DbValue::from(now),
                        ]);
                    }

                    tx.bulk_insert_rows("staging_scout_files", &STAGING_COLUMNS, &rows)?;

                    if compute_stats {
                        let row = tx.query_one(
                            r#"
                            SELECT
                                COALESCE(SUM(CASE WHEN target.path IS NULL THEN 1 ELSE 0 END), 0) AS new_count,
                                COALESCE(SUM(CASE
                                    WHEN target.path IS NOT NULL
                                     AND (target.size != source.size OR target.mtime != source.mtime)
                                    THEN 1 ELSE 0 END), 0) AS changed_count,
                                COALESCE(SUM(CASE
                                    WHEN target.path IS NOT NULL
                                     AND target.size = source.size AND target.mtime = source.mtime
                                    THEN 1 ELSE 0 END), 0) AS unchanged_count
                            FROM staging_scout_files AS source
                            LEFT JOIN scout_files AS target
                              ON target.source_id = source.source_id AND target.file_uid = source.file_uid
                            "#,
                            &[],
                        )?;

                        let new_count: i64 = row.get(0)?;
                        let changed_count: i64 = row.get(1)?;
                        let unchanged_count: i64 = row.get(2)?;

                        stats.new = u64::try_from(new_count).map_err(|_| {
                            BackendError::TypeConversion("new_count out of range".to_string())
                        })?;
                        stats.changed = u64::try_from(changed_count).map_err(|_| {
                            BackendError::TypeConversion("changed_count out of range".to_string())
                        })?;
                        stats.unchanged = u64::try_from(unchanged_count).map_err(|_| {
                            BackendError::TypeConversion("unchanged_count out of range".to_string())
                        })?;
                    }

                    let merge_sql = format!(
                        r#"
                        MERGE INTO scout_files AS target
                        USING staging_scout_files AS source
                        ON target.source_id = source.source_id AND target.file_uid = source.file_uid
                        WHEN MATCHED AND (target.size != source.size OR target.mtime != source.mtime) THEN
                            UPDATE SET
                                workspace_id = source.workspace_id,
                                path = source.path,
                                rel_path = source.rel_path,
                                size = source.size,
                                mtime = source.mtime,
                                content_hash = source.content_hash,
                                parent_path = source.parent_path,
                                name = source.name,
                                extension = source.extension,
                                is_dir = source.is_dir,
                                file_uid = source.file_uid,
                                status = '{pending_status}',
                                error = NULL,
                                sentinel_job_id = NULL,
                                last_seen_at = source.last_seen_at
                        WHEN MATCHED THEN
                            UPDATE SET
                                workspace_id = source.workspace_id,
                                path = source.path,
                                rel_path = source.rel_path,
                                parent_path = source.parent_path,
                                name = source.name,
                                extension = source.extension,
                                is_dir = source.is_dir,
                                file_uid = source.file_uid,
                                last_seen_at = source.last_seen_at
                        WHEN NOT MATCHED THEN
                            INSERT (workspace_id, source_id, file_uid, path, rel_path, parent_path, name, extension, is_dir,
                                    size, mtime, content_hash, status, first_seen_at, last_seen_at)
                            VALUES (source.workspace_id, source.source_id, source.file_uid, source.path, source.rel_path, source.parent_path, source.name, source.extension, source.is_dir,
                                    source.size, source.mtime, source.content_hash, source.status, source.first_seen_at, source.last_seen_at)
                        "#,
                        pending_status = pending_status
                    );

                    tx.execute_batch(&merge_sql)?;
                    tx.execute_batch("DELETE FROM staging_scout_files;")?;
                    Ok(stats)
                })
                .map_err(ScoutError::from)?;

            if let Some(tag_value) = tag {
                self.tag_paths_for_source(
                    &workspace_id,
                    &source_id,
                    &tag_paths,
                    tag_value,
                    TagSource::Manual,
                    None,
                )?;
            }

            Ok(stats)
        }

        #[cfg(not(feature = "duckdb"))]
        {
            let _ = compute_stats;
            let _ = tag;
            let _ = files;
            Err(super::error::ScoutError::Config(
                "DuckDB feature not enabled".to_string(),
            ))
        }
    }

    /// Query existing files for a batch (chunked to avoid parameter limit)
    fn query_existing_files_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        source_id: &SourceId,
        files: &[ScannedFile],
    ) -> std::result::Result<
        std::collections::HashMap<String, (i64, i64)>,
        casparian_db::BackendError,
    > {
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

    fn query_existing_files_by_uid_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        source_id: &SourceId,
        files: &[ScannedFile],
    ) -> std::result::Result<
        std::collections::HashMap<String, (String, i64, i64)>,
        casparian_db::BackendError,
    > {
        let mut existing = std::collections::HashMap::with_capacity(files.len());

        const SELECT_CHUNK_SIZE: usize = 500;
        for chunk in files.chunks(SELECT_CHUNK_SIZE) {
            let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT file_uid, path, size, mtime FROM scout_files WHERE source_id = ? AND file_uid IN ({})",
                placeholders
            );

            let mut params = Vec::with_capacity(chunk.len() + 1);
            params.push(DbValue::from(source_id.as_i64()));
            for file in chunk {
                params.push(DbValue::from(file.file_uid.as_str()));
            }

            let rows = tx.query_all(&query, &params)?;
            for row in rows {
                let file_uid: String = row.get(0)?;
                let path: String = row.get(1)?;
                let size: i64 = row.get(2)?;
                let mtime: i64 = row.get(3)?;
                existing.insert(file_uid, (path, size, mtime));
            }
        }

        Ok(existing)
    }

    fn apply_rename_updates_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        files: &[ScannedFile],
        existing_by_uid: &std::collections::HashMap<String, (String, i64, i64)>,
        now: i64,
    ) -> std::result::Result<(), casparian_db::BackendError> {
        for file in files {
            if let Some((existing_path, _, _)) = existing_by_uid.get(file.file_uid.as_str()) {
                if existing_path != &file.path {
                    tx.execute(
                        r#"
                        UPDATE scout_files SET
                            workspace_id = ?,
                            path = ?,
                            rel_path = ?,
                            parent_path = ?,
                            name = ?,
                            extension = ?,
                            is_dir = ?,
                            file_uid = ?,
                            last_seen_at = ?
                        WHERE source_id = ? AND file_uid = ?
                        "#,
                        &[
                            DbValue::from(file.workspace_id.to_string()),
                            file.path.as_str().into(),
                            file.rel_path.as_str().into(),
                            file.parent_path.as_str().into(),
                            file.name.as_str().into(),
                            file.extension.as_deref().into(),
                            (file.is_dir as i64).into(),
                            DbValue::from(file.file_uid.as_str()),
                            now.into(),
                            file.source_id.as_i64().into(),
                            DbValue::from(file.file_uid.as_str()),
                        ],
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Bulk insert a chunk of files using multi-row VALUES
    fn bulk_insert_chunk_tx(
        tx: &mut casparian_db::DbTransaction<'_>,
        files: &[ScannedFile],
        now: i64,
    ) -> std::result::Result<(), casparian_db::BackendError> {
        if files.is_empty() {
            return Ok(());
        }

        // Build multi-row VALUES with FileStatus::Pending for status.
        // 15 bind params per row: workspace_id, source_id, file_uid, path, rel_path, parent_path, name,
        // extension, is_dir, size, mtime, content_hash, status, first_seen_at, last_seen_at
        let row_placeholder = "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let values: String = (0..files.len())
            .map(|_| row_placeholder)
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            r#"INSERT INTO scout_files
               (workspace_id, source_id, file_uid, path, rel_path, parent_path, name, extension, is_dir,
                size, mtime, content_hash, status, first_seen_at, last_seen_at)
               VALUES {}
               ON CONFLICT(source_id, path) DO UPDATE SET
                   workspace_id = excluded.workspace_id,
                   file_uid = excluded.file_uid,
                   size = excluded.size,
                   mtime = excluded.mtime,
                   content_hash = excluded.content_hash,
                   parent_path = excluded.parent_path,
                   name = excluded.name,
                   extension = excluded.extension,
                   is_dir = excluded.is_dir,
                   status = CASE
                       WHEN scout_files.size != excluded.size OR scout_files.mtime != excluded.mtime
                       THEN excluded.status
                       ELSE scout_files.status
                   END,
                   last_seen_at = excluded.last_seen_at
            "#,
            values
        );

        let mut params = Vec::with_capacity(files.len() * 15);
        for file in files {
            params.push(DbValue::from(file.workspace_id.to_string()));
            params.push(file.source_id.as_i64().into());
            params.push(DbValue::from(file.file_uid.as_str()));
            params.push(file.path.as_str().into());
            params.push(file.rel_path.as_str().into());
            params.push(file.parent_path.as_str().into());
            params.push(file.name.as_str().into());
            params.push(file.extension.clone().into());
            params.push((file.is_dir as i64).into());
            params.push((file.size as i64).into());
            params.push(file.mtime.into());
            params.push(file.content_hash.clone().into());
            params.push(FileStatus::Pending.as_str().into());
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
        now: i64,
        existing: Option<&std::collections::HashMap<String, (i64, i64)>>,
        stats: &mut BatchUpsertResult,
    ) {
        for file in files {
            let classification = existing.map(|existing| {
                let is_new = !existing.contains_key(&file.path);
                let is_changed = existing
                    .get(&file.path)
                    .is_some_and(|(size, mtime)| *size != file.size as i64 || *mtime != file.mtime);
                (is_new, is_changed)
            });

            let params = [
                DbValue::from(file.workspace_id.to_string()),
                DbValue::from(file.source_id.as_i64()),
                DbValue::from(file.file_uid.as_str()),
                file.path.as_str().into(),
                file.rel_path.as_str().into(),
                file.parent_path.as_str().into(),
                file.name.as_str().into(),
                file.extension.clone().into(),
                (file.is_dir as i64).into(),
                (file.size as i64).into(),
                file.mtime.into(),
                file.content_hash.clone().into(),
                FileStatus::Pending.as_str().into(),
                now.into(),
                now.into(),
            ];

            let result = tx.execute(
                r#"INSERT INTO scout_files
                   (workspace_id, source_id, file_uid, path, rel_path, parent_path, name, extension, is_dir, size, mtime, content_hash, status, first_seen_at, last_seen_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                   ON CONFLICT(source_id, path) DO UPDATE SET
                       workspace_id = excluded.workspace_id,
                       file_uid = excluded.file_uid,
                       size = excluded.size,
                       mtime = excluded.mtime,
                       content_hash = excluded.content_hash,
                       parent_path = excluded.parent_path,
                       name = excluded.name,
                       extension = excluded.extension,
                       is_dir = excluded.is_dir,
                       status = CASE
                           WHEN scout_files.size != excluded.size OR scout_files.mtime != excluded.mtime
                           THEN excluded.status
                           ELSE scout_files.status
                       END,
                       last_seen_at = excluded.last_seen_at"#,
                &params,
            );

            match result {
                Ok(_) => {
                    if let Some((is_new, is_changed)) = classification {
                        if is_new {
                            stats.new += 1;
                        } else if is_changed {
                            stats.changed += 1;
                        } else {
                            stats.unchanged += 1;
                        }
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
        let row = self.conn.query_optional(
            &format!("SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE id = ?"),
            &[id.into()],
        )?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// Get a file by path
    pub fn get_file_by_path(
        &self,
        source_id: &SourceId,
        path: &str,
    ) -> Result<Option<ScannedFile>> {
        let row = self.conn.query_optional(
            &format!(
                "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE source_id = ? AND path = ?"
            ),
            &[source_id.as_i64().into(), path.into()],
        )?;

        match row {
            Some(row) => Ok(Some(Self::row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// List all files for a source (regardless of status)
    pub fn list_files_by_source(
        &self,
        source_id: &SourceId,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE source_id = ? ORDER BY mtime DESC LIMIT ?"
                ),
                &[source_id.as_i64().into(), (limit as i64).into()],
            )?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files with a specific status
    pub fn list_files_by_status(
        &self,
        workspace_id: &WorkspaceId,
        status: FileStatus,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE workspace_id = ? AND status = ? ORDER BY mtime DESC LIMIT ?"
                ),
                &[
                    DbValue::from(workspace_id.to_string()),
                    status.as_str().into(),
                    (limit as i64).into(),
                ],
            )?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List pending (untagged) files for a source
    pub fn list_pending_files(
        &self,
        source_id: &SourceId,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Pending, limit)
    }

    /// List tagged files ready for processing
    pub fn list_tagged_files(
        &self,
        source_id: &SourceId,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        self.list_files_by_source_and_status(source_id, FileStatus::Tagged, limit)
    }

    /// List untagged files (files that have no tag assigned)
    pub fn list_untagged_files(
        &self,
        source_id: &SourceId,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE source_id = ? AND status = ? AND NOT EXISTS (SELECT 1 FROM scout_file_tags WHERE scout_file_tags.file_id = scout_files.id) ORDER BY mtime DESC LIMIT ?"
                ),
                &[
                    source_id.as_i64().into(),
                    FileStatus::Pending.as_str().into(),
                    (limit as i64).into(),
                ],
            )?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// List files by tag
    pub fn list_files_by_tag(
        &self,
        workspace_id: &WorkspaceId,
        tag: &str,
        limit: usize,
    ) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE workspace_id = ? AND id IN (SELECT file_id FROM scout_file_tags WHERE workspace_id = ? AND tag = ?) ORDER BY mtime DESC LIMIT ?"
                ),
                &[
                    DbValue::from(workspace_id.to_string()),
                    DbValue::from(workspace_id.to_string()),
                    DbValue::from(tag),
                    (limit as i64).into(),
                ],
            )?;

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
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE source_id = ? AND status = ? ORDER BY mtime DESC LIMIT ?"
                ),
                &[source_id.as_i64().into(), status.as_str().into(), (limit as i64).into()],
            )?;

        rows.iter().map(Self::row_to_file).collect()
    }

    /// Tag a file manually (sets tag_source = 'manual')
    pub fn tag_file(&self, id: i64, tag: &str) -> Result<()> {
        self.insert_file_tag(id, tag, TagSource::Manual, None)?;
        self.set_file_status_tagged(id)?;
        Ok(())
    }

    /// Tag multiple files manually (sets tag_source = 'manual')
    pub fn tag_files(&self, ids: &[i64], tag: &str) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut total = 0u64;
        for id in ids {
            total += self.insert_file_tag(*id, tag, TagSource::Manual, None)?;
            self.set_file_status_tagged(*id)?;
        }
        Ok(total)
    }

    /// Tag a file via a tagging rule (sets tag_source = 'rule')
    pub fn tag_file_by_rule(&self, id: i64, tag: &str, rule_id: &TaggingRuleId) -> Result<()> {
        self.insert_file_tag(id, tag, TagSource::Rule, Some(rule_id))?;
        self.set_file_status_tagged(id)?;
        Ok(())
    }

    /// List tags assigned to a file.
    pub fn list_file_tags(&self, file_id: i64) -> Result<Vec<FileTag>> {
        let rows = self.conn.query_all(
            r#"
            SELECT tag, tag_source, rule_id, created_at
            FROM scout_file_tags
            WHERE file_id = ?
            ORDER BY tag
            "#,
            &[file_id.into()],
        )?;

        rows.iter().map(Self::row_to_file_tag).collect()
    }

    /// List distinct tags for a workspace.
    pub fn list_tags(&self, workspace_id: &WorkspaceId) -> Result<Vec<String>> {
        let rows = self.conn.query_all(
            r#"
            SELECT DISTINCT tag
            FROM scout_file_tags
            WHERE workspace_id = ?
            ORDER BY tag
            "#,
            &[DbValue::from(workspace_id.to_string())],
        )?;

        let mut tags = Vec::with_capacity(rows.len());
        for row in rows {
            tags.push(row.get(0)?);
        }
        Ok(tags)
    }

    fn insert_file_tag(
        &self,
        file_id: i64,
        tag: &str,
        tag_source: TagSource,
        rule_id: Option<&TaggingRuleId>,
    ) -> Result<u64> {
        let now = now_millis();
        let rule_id_value = match rule_id {
            Some(id) => DbValue::from(id.to_string()),
            None => DbValue::Null,
        };

        let result = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at)
            SELECT workspace_id, id, ?, ?, ?, ? FROM scout_files WHERE id = ?
            "#,
            &[
                tag.into(),
                tag_source.as_str().into(),
                rule_id_value,
                now.into(),
                file_id.into(),
            ],
        )?;
        Ok(result)
    }

    fn tag_paths_for_source(
        &self,
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        paths: &[String],
        tag: &str,
        tag_source: TagSource,
        rule_id: Option<&TaggingRuleId>,
    ) -> Result<u64> {
        if paths.is_empty() {
            return Ok(0);
        }

        let mut total = 0u64;
        const CHUNK_SIZE: usize = 500;
        let now = now_millis();
        let rule_id_value = match rule_id {
            Some(id) => DbValue::from(id.to_string()),
            None => DbValue::Null,
        };

        for chunk in paths.chunks(CHUNK_SIZE) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "INSERT OR IGNORE INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at)
                 SELECT ?, id, ?, ?, ?, ? FROM scout_files WHERE source_id = ? AND path IN ({})",
                placeholders
            );

            let mut params: Vec<DbValue> = Vec::with_capacity(5 + chunk.len() + 1);
            params.push(DbValue::from(workspace_id.to_string()));
            params.push(DbValue::from(tag));
            params.push(DbValue::from(tag_source.as_str()));
            params.push(rule_id_value.clone());
            params.push(now.into());
            params.push(source_id.as_i64().into());
            params.extend(chunk.iter().map(|p| DbValue::from(p.as_str())));

            total += self.conn.execute(&sql, &params)?;

            let update_sql = format!(
                "UPDATE scout_files SET status = ? WHERE source_id = ? AND path IN ({}) AND status = ?",
                placeholders
            );
            let mut update_params: Vec<DbValue> = Vec::with_capacity(3 + chunk.len());
            update_params.push(FileStatus::Tagged.as_str().into());
            update_params.push(source_id.as_i64().into());
            update_params.extend(chunk.iter().map(|p| DbValue::from(p.as_str())));
            update_params.push(FileStatus::Pending.as_str().into());
            self.conn.execute(&update_sql, &update_params)?;
        }

        Ok(total)
    }

    fn set_file_status_tagged(&self, file_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scout_files SET status = ? WHERE id = ? AND status = ?",
            &[
                FileStatus::Tagged.as_str().into(),
                file_id.into(),
                FileStatus::Pending.as_str().into(),
            ],
        )?;
        Ok(())
    }

    /// Update file status
    pub fn update_file_status(
        &self,
        id: i64,
        status: FileStatus,
        error: Option<&str>,
    ) -> Result<()> {
        if status == FileStatus::Processed {
            self.conn.execute(
                "UPDATE scout_files SET status = ?, error = ?, processed_at = ? WHERE id = ?",
                &[
                    status.as_str().into(),
                    error.into(),
                    now_millis().into(),
                    id.into(),
                ],
            )?;
        } else {
            self.conn.execute(
                "UPDATE scout_files SET status = ?, error = ? WHERE id = ?",
                &[status.as_str().into(), error.into(), id.into()],
            )?;
        }
        Ok(())
    }

    /// Untag a file (clear tag, tag_source, rule_id, manual_plugin and reset to pending)
    pub fn untag_file(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM scout_file_tags WHERE file_id = ?",
            &[id.into()],
        )?;
        self.conn.execute(
            "UPDATE scout_files SET manual_plugin = NULL, status = ?, sentinel_job_id = NULL WHERE id = ?",
            &[FileStatus::Pending.as_str().into(), id.into()],
        )?;
        Ok(())
    }

    /// Mark file as queued for processing
    pub fn mark_file_queued(&self, id: i64, sentinel_job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scout_files SET status = ?, sentinel_job_id = ? WHERE id = ?",
            &[
                FileStatus::Queued.as_str().into(),
                sentinel_job_id.into(),
                id.into(),
            ],
        )?;
        Ok(())
    }

    /// Mark files as deleted if not seen recently
    pub fn mark_deleted_files(
        &self,
        source_id: &SourceId,
        seen_before: DateTime<Utc>,
    ) -> Result<u64> {
        let seen_before_millis = seen_before.timestamp_millis();
        let result = self.conn.execute(
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
        )?;

        Ok(result)
    }

    fn row_to_file(row: &casparian_db::UnifiedDbRow) -> Result<ScannedFile> {
        use super::types::ExtractionStatus;

        // Column positions:
        // 0:id, 1:workspace_id, 2:source_id, 3:file_uid, 4:path, 5:rel_path, 6:parent_path, 7:name,
        // 8:extension, 9:is_dir, 10:size, 11:mtime, 12:content_hash, 13:status,
        // 14:manual_plugin, 15:error, 16:first_seen_at, 17:last_seen_at, 18:processed_at,
        // 19:sentinel_job_id, 20:metadata_raw, 21:extraction_status, 22:extracted_at

        let status_str: String = row.get(13)?;
        let status = FileStatus::parse(&status_str).ok_or_else(|| {
            ScoutError::InvalidState(format!("Invalid file status: {}", status_str))
        })?;

        let first_seen_millis: i64 = row.get(16)?;
        let last_seen_millis: i64 = row.get(17)?;
        let processed_at_millis: Option<i64> = row.get(18)?;

        // Parse extraction status (Phase 6)
        let extraction_status_str: Option<String> = row.get(21)?;
        let extraction_status = match extraction_status_str.as_deref() {
            Some(raw) => ExtractionStatus::parse(raw).ok_or_else(|| {
                ScoutError::InvalidState(format!("Invalid extraction status: {}", raw))
            })?,
            None => ExtractionStatus::Pending,
        };
        let extracted_at_millis: Option<i64> = row.get(22)?;

        let workspace_id_raw: String = row.get(1)?;
        let workspace_id = WorkspaceId::parse(&workspace_id_raw)?;

        let source_id_raw: i64 = row.get(2)?;
        let source_id = SourceId::try_from(source_id_raw)?;

        Ok(ScannedFile {
            id: Some(row.get(0)?),
            workspace_id,
            source_id,
            file_uid: row.get(3)?,
            path: row.get(4)?,
            rel_path: row.get(5)?,
            parent_path: row.get(6)?,
            name: row.get(7)?,
            extension: row.get(8)?,
            is_dir: row.get::<i64>(9)? != 0,
            size: row.get::<i64>(10)? as u64,
            mtime: row.get(11)?,
            content_hash: row.get(12)?,
            status,
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

    fn row_to_file_tag(row: &casparian_db::UnifiedDbRow) -> Result<FileTag> {
        let tag: String = row.get(0)?;
        let tag_source_raw: String = row.get(1)?;
        let tag_source = TagSource::parse(&tag_source_raw).ok_or_else(|| {
            ScoutError::InvalidState(format!("Invalid tag source: {}", tag_source_raw))
        })?;
        let rule_id_raw: Option<String> = row.get(2)?;
        let rule_id = match rule_id_raw {
            Some(value) => Some(TaggingRuleId::parse(&value)?),
            None => None,
        };
        let created_at_millis: i64 = row.get(3)?;

        Ok(FileTag {
            tag,
            tag_source,
            rule_id,
            assigned_at: millis_to_datetime(created_at_millis),
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

        let (
            total_files,
            files_pending,
            files_tagged,
            files_queued,
            files_processing,
            files_processed,
            files_failed,
            bytes_pending,
            bytes_processed,
        ) = if let Some(row) = row {
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

        let total_workspaces = self
            .conn
            .query_scalar::<i64>("SELECT COUNT(*) FROM cf_workspaces", &[])
            .unwrap_or(0);

        let total_tagging_rules = self
            .conn
            .query_scalar::<i64>(
                "SELECT COUNT(*) FROM scout_rules WHERE kind = 'tagging'",
                &[],
            )
            .unwrap_or(0);

        let total_tags = self
            .conn
            .query_scalar::<i64>("SELECT COUNT(*) FROM scout_file_tags", &[])
            .unwrap_or(0);

        Ok(DbStats {
            total_workspaces: total_workspaces as u64,
            total_sources: total_sources as u64,
            total_tagging_rules: total_tagging_rules as u64,
            total_tags: total_tags as u64,
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
    /// * `workspace_id` - Workspace scope
    /// * `source_id` - The source to query
    /// * `prefix` - Path prefix (empty for root, "folder" for subfolder - no trailing slash)
    /// * `glob_pattern` - Optional glob pattern filter (e.g., "*.csv")
    ///
    /// # Returns
    /// Vec of (folder_name, file_count, is_file) tuples
    pub fn get_folder_counts(
        &self,
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<Vec<(String, i64, bool)>> {
        // Normalize prefix: remove trailing slash if present
        let prefix = prefix.trim_end_matches('/');

        if let Some(pattern) = glob_pattern {
            // With glob pattern: search matching files and group by immediate child
            self.get_folder_counts_with_pattern(workspace_id, source_id, prefix, pattern)
        } else {
            // No pattern: O(1) lookup using parent_path index
            self.get_folder_counts_fast(workspace_id, source_id, prefix)
        }
    }

    /// Fast O(1) folder listing using parent_path index (no pattern filtering)
    fn get_folder_counts_fast(
        &self,
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        parent_path: &str,
    ) -> Result<Vec<(String, i64, bool)>> {
        // For root level, try the pre-computed cache first (avoids 20+ second GROUP BY)
        if parent_path.is_empty() {
            if let Some(cached) = self.get_folder_counts_from_cache(workspace_id, source_id, "")? {
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
                "SELECT name, size FROM scout_files WHERE workspace_id = ? AND source_id = ? AND parent_path = ? ORDER BY name LIMIT 200",
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    parent_path.into(),
                ],
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
                WHERE workspace_id = ? AND source_id = ? AND parent_path != ''
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                ],
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
                WHERE workspace_id = ? AND source_id = ? AND parent_path LIKE ? || '%' AND parent_path != ?
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    folder_prefix.as_str().into(),
                    DbValue::from(workspace_id.to_string()),
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
        workspace_id: &WorkspaceId,
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
        let prefix_len = if prefix.is_empty() {
            0
        } else {
            prefix.len() as i32 + 1
        };

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
            WHERE workspace_id = ? AND source_id = ?
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
                DbValue::from(workspace_id.to_string()),
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
        workspace_id: &WorkspaceId,
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
            self.conn.query_all(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE workspace_id = ? AND source_id = ?
                  AND rel_path LIKE ?
                  AND rel_path LIKE ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    like_pattern.as_str().into(),
                    (limit as i64).into(),
                ],
            )?
        } else {
            self.conn.query_all(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE workspace_id = ? AND source_id = ?
                  AND rel_path LIKE ?
                ORDER BY mtime DESC
                LIMIT ?
                "#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    (limit as i64).into(),
                ],
            )?
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
        workspace_id: &WorkspaceId,
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
            self.conn.query_scalar::<i64>(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE workspace_id = ? AND source_id = ?
                  AND rel_path LIKE ?
                  AND rel_path LIKE ?
                "#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                    like_pattern.as_str().into(),
                ],
            )?
        } else {
            self.conn.query_scalar::<i64>(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE workspace_id = ? AND source_id = ?
                  AND rel_path LIKE ?
                "#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    prefix_pattern.as_str().into(),
                ],
            )?
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
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        extension: Option<&str>,
        path_pattern: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<(String, i64, i64)>> {
        let rows = match (extension, path_pattern) {
            (Some(ext), Some(path_pat)) => self.conn.query_all(
                r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND extension = ? AND rel_path LIKE ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    ext.into(),
                    path_pat.into(),
                    (limit as i64).into(),
                    (offset as i64).into(),
                ],
            )?,
            (Some(ext), None) => self.conn.query_all(
                r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND extension = ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    ext.into(),
                    (limit as i64).into(),
                    (offset as i64).into(),
                ],
            )?,
            (None, Some(path_pat)) => self.conn.query_all(
                r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND rel_path LIKE ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    path_pat.into(),
                    (limit as i64).into(),
                    (offset as i64).into(),
                ],
            )?,
            (None, None) => self.conn.query_all(
                r#"SELECT rel_path, size, mtime FROM scout_files
                           WHERE workspace_id = ? AND source_id = ?
                           ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    (limit as i64).into(),
                    (offset as i64).into(),
                ],
            )?,
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
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        extension: Option<&str>,
        path_pattern: Option<&str>,
    ) -> Result<i64> {
        let count = match (extension, path_pattern) {
            (Some(ext), Some(path_pat)) => self.conn.query_scalar::<i64>(
                r#"SELECT COUNT(*) FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND extension = ? AND rel_path LIKE ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    ext.into(),
                    path_pat.into(),
                ],
            )?,
            (Some(ext), None) => self.conn.query_scalar::<i64>(
                r#"SELECT COUNT(*) FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND extension = ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    ext.into(),
                ],
            )?,
            (None, Some(path_pat)) => self.conn.query_scalar::<i64>(
                r#"SELECT COUNT(*) FROM scout_files
                           WHERE workspace_id = ? AND source_id = ? AND rel_path LIKE ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    path_pat.into(),
                ],
            )?,
            (None, None) => self.conn.query_scalar::<i64>(
                r#"SELECT COUNT(*) FROM scout_files
                           WHERE workspace_id = ? AND source_id = ?"#,
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                ],
            )?,
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
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        parent_path: &str,
        limit: usize,
    ) -> Result<Vec<(String, bool, u64)>> {
        // Get all files directly in this folder
        let files = self.conn.query_all(
            r#"
            SELECT name, size
            FROM scout_files
            WHERE workspace_id = ? AND source_id = ? AND parent_path = ?
            ORDER BY name
            LIMIT ?
            "#,
            &[
                DbValue::from(workspace_id.to_string()),
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
            WHERE workspace_id = ? AND source_id = ?
              AND parent_path LIKE ? || '%'
              AND parent_path != ?
            ORDER BY subfolder
            LIMIT ?
            "#,
            &[
                parent_path.into(),
                subfolder_prefix.as_str().into(),
                subfolder_prefix.as_str().into(),
                DbValue::from(workspace_id.to_string()),
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
        let mut results: Vec<(String, bool, u64)> =
            Vec::with_capacity(files.len() + subfolders.len());

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
    pub fn count_files_in_folder(
        &self,
        workspace_id: &WorkspaceId,
        source_id: &SourceId,
        parent_path: &str,
    ) -> Result<i64> {
        let count = self.conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_files WHERE workspace_id = ? AND source_id = ? AND parent_path = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                source_id.as_i64().into(),
                parent_path.into(),
            ],
        )?;

        Ok(count)
    }

    // ========================================================================
    // Settings Operations
    // ========================================================================

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO scout_settings (key, value) VALUES (?, ?)",
            &[key.into(), value.into()],
        )?;
        Ok(())
    }

    /// Get a setting value
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = self.conn.query_optional(
            "SELECT value FROM scout_settings WHERE key = ?",
            &[key.into()],
        )?;
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

        self.conn.execute(
            "UPDATE scout_extractors SET paused_at = ?, updated_at = ? WHERE id = ?",
            &[now.into(), now.into(), id.into()],
        )?;

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
    pub fn update_extractor_consecutive_failures(&self, id: &str, failures: u32) -> Result<()> {
        let now = Utc::now().timestamp_millis();

        self.conn.execute(
            "UPDATE scout_extractors SET consecutive_failures = ?, updated_at = ? WHERE id = ?",
            &[(failures as i64).into(), now.into(), id.into()],
        )?;

        Ok(())
    }

    /// Delete an extractor
    pub fn delete_extractor(&self, id: &str) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM scout_extractors WHERE id = ?", &[id.into()])?;

        Ok(result > 0)
    }

    /// Get files pending extraction (ExtractionStatus::Pending)
    pub fn get_files_pending_extraction(&self) -> Result<Vec<ScannedFile>> {
        let rows = self
            .conn
            .query_all(
                &format!(
                    "SELECT {FILE_SELECT_COLUMNS} FROM scout_files WHERE extraction_status = ? ORDER BY first_seen_at LIMIT 1000"
                ),
                &[ExtractionStatus::Pending.as_str().into()],
            )
            ?;

        Ok(rows
            .iter()
            .map(Self::row_to_file)
            .filter_map(|r| r.ok())
            .collect())
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

        self.conn.execute(
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
        )?;

        Ok(())
    }

    /// Mark extraction as stale for files with a given extractor
    pub fn mark_extractions_stale(&self, extractor_id: &str) -> Result<u64> {
        let result = self.conn.execute(
            r#"
                UPDATE scout_files
                SET extraction_status = 'stale'
                WHERE id IN (
                    SELECT DISTINCT file_id FROM scout_extraction_log WHERE extractor_id = ?
                )
                AND extraction_status = 'extracted'
                "#,
            &[extractor_id.into()],
        )?;

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

    fn default_workspace_id(db: &Database) -> WorkspaceId {
        db.ensure_default_workspace().unwrap().id
    }

    #[test]
    fn test_source_crud() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "Test Source".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };

        db.upsert_source(&source).unwrap();
        let fetched = db.get_source(&source_id).unwrap().unwrap();
        assert_eq!(fetched.name, "Test Source");
        assert_eq!(fetched.path, "/data");

        let sources = db.list_sources(&workspace_id).unwrap();
        assert_eq!(sources.len(), 1);

        assert!(db.delete_source(&source_id).unwrap());
        assert!(db.get_source(&source_id).unwrap().is_none());
    }

    #[test]
    fn test_tagging_rule_crud() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let rule_id = TaggingRuleId::new();

        let rule = TaggingRule {
            id: rule_id.clone(),
            name: "CSV Files".to_string(),
            workspace_id,
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };

        db.upsert_tagging_rule(&rule).unwrap();
        let fetched = db.get_tagging_rule(&rule_id).unwrap().unwrap();
        assert_eq!(fetched.tag, "csv_data");
        assert_eq!(fetched.priority, 10);

        let rules = db.list_tagging_rules_for_workspace(&workspace_id).unwrap();
        assert_eq!(rules.len(), 1);

        assert!(db.delete_tagging_rule(&rule_id).unwrap());
        assert!(db.get_tagging_rule(&rule_id).unwrap().is_none());
    }

    #[test]
    fn test_file_tagging() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Direct insert test with explicit NULL
        let now_ms = chrono::Utc::now().timestamp_millis();
        db.conn
            .execute(
                "INSERT INTO scout_files (workspace_id, source_id, file_uid, path, rel_path, parent_path, name, extension, is_dir, size, mtime, content_hash, status, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &[
                    DbValue::from(workspace_id.to_string()),
                    source_id.as_i64().into(),
                    DbValue::from("path:/data/direct.csv"),
                    "/data/direct.csv".into(),
                    "direct.csv".into(),
                    "".into(),
                    "direct.csv".into(),
                    DbValue::Null, // extension
                    0_i64.into(),   // is_dir
                    (1000_i64).into(),
                    (12345_i64).into(),
                    DbValue::Null, // content_hash
                    FileStatus::Pending.as_str().into(),
                    now_ms.into(),
                    now_ms.into(),
                ],
            )

            .unwrap();
        let file_uid = crate::file_uid::weak_uid_from_path_str("/data/test.csv");
        let file = ScannedFile::new(
            workspace_id,
            source_id.clone(),
            &file_uid,
            "/data/test.csv",
            "test.csv",
            1000,
            12345,
        );
        let result = db.upsert_file(&file).unwrap();

        // File starts untagged
        let fetched = db.get_file(result.id).unwrap().unwrap();
        let tags = db.list_file_tags(result.id).unwrap();
        assert!(tags.is_empty());
        assert_eq!(fetched.status, FileStatus::Pending);

        // Tag the file
        db.tag_file(result.id, "csv_data").unwrap();
        let fetched = db.get_file(result.id).unwrap().unwrap();
        let tags = db.list_file_tags(result.id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "csv_data");
        assert_eq!(fetched.status, FileStatus::Tagged);

        // List by tag
        let tagged = db.list_files_by_tag(&workspace_id, "csv_data", 10).unwrap();
        assert_eq!(tagged.len(), 1);
    }

    /// Test that sources are ordered by most recently used (MRU) and persist across sessions
    #[test]
    fn test_source_mru_ordering_persists() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let source_a_id = SourceId::new();
        let source_b_id = SourceId::new();
        let source_c_id = SourceId::new();

        // Create three sources with small delays to ensure different timestamps
        let source_a = Source {
            workspace_id,
            id: source_a_id.clone(),
            name: "Source A".to_string(),
            source_type: SourceType::Local,
            path: "/data/a".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_a).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let source_b = Source {
            workspace_id,
            id: source_b_id.clone(),
            name: "Source B".to_string(),
            source_type: SourceType::Local,
            path: "/data/b".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_b).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let source_c = Source {
            workspace_id,
            id: source_c_id.clone(),
            name: "Source C".to_string(),
            source_type: SourceType::Local,
            path: "/data/c".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source_c).unwrap();

        // Initial MRU order: C (most recent), B, A (oldest)
        let sources = db.list_sources_by_mru(&workspace_id).unwrap();
        assert_eq!(sources.len(), 3);
        assert_eq!(
            sources[0].id, source_c_id,
            "Most recently created should be first"
        );
        assert_eq!(sources[1].id, source_b_id);
        assert_eq!(sources[2].id, source_a_id, "Oldest should be last");

        // Touch source A (simulates user selecting it)
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.touch_source(&source_a_id).unwrap();

        // New MRU order: A (touched), C, B
        let sources = db.list_sources_by_mru(&workspace_id).unwrap();
        assert_eq!(
            sources[0].id, source_a_id,
            "Touched source should move to top"
        );
        assert_eq!(sources[1].id, source_c_id);
        assert_eq!(sources[2].id, source_b_id);

        // Touch source B
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.touch_source(&source_b_id).unwrap();

        // New MRU order: B, A, C
        let sources = db.list_sources_by_mru(&workspace_id).unwrap();
        assert_eq!(
            sources[0].id, source_b_id,
            "Most recently touched should be first"
        );
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
        let workspace_id = default_workspace_id(&db);
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Create 150 files (tests chunking since limit is 100)
        let files: Vec<ScannedFile> = (0..150)
            .map(|i| {
                let path = format!("/data/file{}.txt", i);
                let file_uid = crate::file_uid::weak_uid_from_path_str(&path);
                ScannedFile::new(
                    workspace_id,
                    source_id.clone(),
                    &file_uid,
                    &path,
                    &format!("file{}.txt", i),
                    1000 + i,
                    12345,
                )
            })
            .collect();

        // First batch insert - all new
        // GAP-SCAN-012: Stats are now approximate (all counted as new) for performance
        let result = db
            .batch_upsert_files(&files, Some("test_tag"), true)
            .unwrap();
        assert_eq!(result.errors, 0);

        // Verify files were inserted with tag
        let tagged = db
            .list_files_by_tag(&workspace_id, "test_tag", 200)
            .unwrap();
        assert_eq!(tagged.len(), 150, "Should have 150 tagged files");

        // Second batch insert - same files (MERGE handles idempotency)
        let result = db
            .batch_upsert_files(&files, Some("test_tag"), true)
            .unwrap();
        assert_eq!(result.errors, 0);

        // Verify still 150 files (no duplicates)
        let tagged = db
            .list_files_by_tag(&workspace_id, "test_tag", 200)
            .unwrap();
        assert_eq!(tagged.len(), 150, "Should still have 150 tagged files");

        // Third batch insert - modify some files (change size)
        let modified_files: Vec<ScannedFile> = (0..150)
            .map(|i| {
                if i < 50 {
                    // First 50 files: change size
                    let path = format!("/data/file{}.txt", i);
                    let file_uid = crate::file_uid::weak_uid_from_path_str(&path);
                    ScannedFile::new(
                        workspace_id,
                        source_id.clone(),
                        &file_uid,
                        &path,
                        &format!("file{}.txt", i),
                        2000 + i,
                        12345,
                    )
                } else {
                    // Remaining 100 files: unchanged
                    let path = format!("/data/file{}.txt", i);
                    let file_uid = crate::file_uid::weak_uid_from_path_str(&path);
                    ScannedFile::new(
                        workspace_id,
                        source_id.clone(),
                        &file_uid,
                        &path,
                        &format!("file{}.txt", i),
                        1000 + i,
                        12345,
                    )
                }
            })
            .collect();

        let result = db
            .batch_upsert_files(&modified_files, Some("test_tag"), true)
            .unwrap();
        assert_eq!(result.errors, 0);

        // Verify size was updated for modified files
        let file0 = db
            .get_file_by_path(&source_id, "/data/file0.txt")
            .unwrap()
            .unwrap();
        assert_eq!(file0.size, 2000, "File size should be updated");

        let file100 = db
            .get_file_by_path(&source_id, "/data/file100.txt")
            .unwrap()
            .unwrap();
        assert_eq!(file100.size, 1100, "Unchanged file size should remain");
    }

    /// Test batch upsert without tag (preserves existing tags)
    #[test]
    fn test_batch_upsert_files_no_tag() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Create file and tag it
        let file_uid = crate::file_uid::weak_uid_from_path_str("/data/test.txt");
        let file = ScannedFile::new(
            workspace_id,
            source_id.clone(),
            &file_uid,
            "/data/test.txt",
            "test.txt",
            1000,
            12345,
        );
        let upsert_result = db.upsert_file(&file).unwrap();
        db.tag_file(upsert_result.id, "original_tag").unwrap();

        // Batch upsert with no tag - should preserve existing tag
        // GAP-SCAN-012: Stats are now approximate for performance
        let result = db
            .batch_upsert_files(std::slice::from_ref(&file), None, true)
            .unwrap();
        assert_eq!(result.errors, 0);

        let fetched = db
            .get_file_by_path(&source_id, "/data/test.txt")
            .unwrap()
            .unwrap();
        let tags = db.list_file_tags(fetched.id.unwrap()).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "original_tag");
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
        let workspace_id = default_workspace_id(&db);
        let source_id = SourceId::new();

        let source = Source {
            workspace_id,
            id: source_id.clone(),
            name: "Test".to_string(),
            source_type: SourceType::Local,
            path: "/data".to_string(),
            exec_path: None,
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
        let make_file = |path: &str, rel_path: &str, size: u64, mtime: i64| {
            let file_uid = crate::file_uid::weak_uid_from_path_str(path);
            ScannedFile::new(
                workspace_id,
                source_id.clone(),
                &file_uid,
                path,
                rel_path,
                size,
                mtime,
            )
        };
        let files = vec![
            make_file("/data/root.txt", "root.txt", 100, 1000),
            make_file("/data/docs/readme.md", "docs/readme.md", 200, 2000),
            make_file("/data/docs/api/spec.json", "docs/api/spec.json", 300, 3000),
            make_file("/data/logs/2024/jan.log", "logs/2024/jan.log", 400, 4000),
            make_file("/data/logs/2024/feb.log", "logs/2024/feb.log", 500, 5000),
        ];

        db.batch_upsert_files(&files, None, true).unwrap();

        // Verify parent_path and name are set correctly
        let root_file = db
            .get_file_by_path(&source_id, "/data/root.txt")
            .unwrap()
            .unwrap();
        assert_eq!(root_file.parent_path, "");
        assert_eq!(root_file.name, "root.txt");

        let readme = db
            .get_file_by_path(&source_id, "/data/docs/readme.md")
            .unwrap()
            .unwrap();
        assert_eq!(readme.parent_path, "docs");
        assert_eq!(readme.name, "readme.md");

        let spec = db
            .get_file_by_path(&source_id, "/data/docs/api/spec.json")
            .unwrap()
            .unwrap();
        assert_eq!(spec.parent_path, "docs/api");
        assert_eq!(spec.name, "spec.json");

        // Test O(1) folder listing at root
        let root_contents = db
            .get_folder_counts(&workspace_id, &source_id, "", None)
            .unwrap();
        // Should have: docs folder, logs folder, root.txt file
        assert!(root_contents
            .iter()
            .any(|(name, _, is_file)| name == "docs" && !is_file));
        assert!(root_contents
            .iter()
            .any(|(name, _, is_file)| name == "logs" && !is_file));
        assert!(root_contents
            .iter()
            .any(|(name, _, is_file)| name == "root.txt" && *is_file));

        // Test folder listing at docs/
        let docs_contents = db
            .get_folder_counts(&workspace_id, &source_id, "docs", None)
            .unwrap();
        assert!(docs_contents
            .iter()
            .any(|(name, _, is_file)| name == "api" && !is_file));
        assert!(docs_contents
            .iter()
            .any(|(name, _, is_file)| name == "readme.md" && *is_file));

        // Test count files in folder
        let count = db
            .count_files_in_folder(&workspace_id, &source_id, "logs/2024")
            .unwrap();
        assert_eq!(count, 2);
    }

    // ========================================================================
    // Source Overlap Detection Tests
    // ========================================================================

    use crate::error::ScoutError;

    #[test]
    fn test_source_overlap_no_sources() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // No existing sources - should allow any path
        let result = db.check_source_overlap(&workspace_id, temp_dir.path());
        assert!(
            result.is_ok(),
            "Should allow source when no existing sources"
        );
    }

    #[test]
    fn test_source_overlap_same_path() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create a source at temp_dir
        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "Parent".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().display().to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Same path is NOT overlap (it's a rescan of existing source)
        // The overlap check should pass because paths are equal, not nested
        let result = db.check_source_overlap(&workspace_id, temp_dir.path());
        assert!(
            result.is_ok(),
            "Same path should be allowed (rescan scenario)"
        );
    }

    #[test]
    fn test_source_overlap_child_of_existing() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create subdirectory
        let child_dir = temp_dir.path().join("projects").join("medical");
        std::fs::create_dir_all(&child_dir).unwrap();

        // Create parent source first
        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "Projects".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().display().to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Try to add child - should fail
        let result = db.check_source_overlap(&workspace_id, &child_dir);
        assert!(
            result.is_err(),
            "Child of existing source should be rejected"
        );

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
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create subdirectory and make it a source first
        let child_dir = temp_dir.path().join("data").join("medical");
        std::fs::create_dir_all(&child_dir).unwrap();

        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "Medical".to_string(),
            source_type: SourceType::Local,
            path: child_dir.display().to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // Try to add parent - should fail
        let result = db.check_source_overlap(&workspace_id, temp_dir.path());
        assert!(
            result.is_err(),
            "Parent of existing source should be rejected"
        );

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
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create two sibling directories
        let dir_a = temp_dir.path().join("projects_a");
        let dir_b = temp_dir.path().join("projects_b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();

        // Create source for dir_a
        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "Projects A".to_string(),
            source_type: SourceType::Local,
            path: dir_a.display().to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // dir_b is a sibling, not nested - should be allowed
        let result = db.check_source_overlap(&workspace_id, &dir_b);
        assert!(result.is_ok(), "Sibling directories should be allowed");
    }

    #[test]
    fn test_source_overlap_stale_source_skipped() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create a source pointing to non-existent path (stale)
        let source = Source {
            workspace_id,
            id: SourceId::new(),
            name: "Stale Source".to_string(),
            source_type: SourceType::Local,
            path: "/nonexistent/path/that/does/not/exist".to_string(),
            exec_path: None,
            poll_interval_secs: 30,
            enabled: true,
        };
        db.upsert_source(&source).unwrap();

        // New source should be allowed even though stale source exists
        // (stale source can't be canonicalized, so it's skipped)
        let result = db.check_source_overlap(&workspace_id, temp_dir.path());
        assert!(
            result.is_ok(),
            "Should skip stale sources during overlap check"
        );
    }

    #[test]
    fn test_source_overlap_multiple_existing() {
        let db = create_test_db();
        let workspace_id = default_workspace_id(&db);
        let temp_dir = tempfile::tempdir().unwrap();

        // Create three separate directories
        let dir_a = temp_dir.path().join("a");
        let dir_b = temp_dir.path().join("b");
        let dir_c = temp_dir.path().join("c");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::create_dir_all(&dir_c).unwrap();

        // Create sources for a and b
        for (name, path) in [("Source A", &dir_a), ("Source B", &dir_b)] {
            let source = Source {
                workspace_id,
                id: SourceId::new(),
                name: name.to_string(),
                source_type: SourceType::Local,
                path: path.display().to_string(),
                exec_path: None,
                poll_interval_secs: 30,
                enabled: true,
            };
            db.upsert_source(&source).unwrap();
        }

        // dir_c is independent - should be allowed
        let result = db.check_source_overlap(&workspace_id, &dir_c);
        assert!(result.is_ok(), "Independent directory should be allowed");

        // Parent of all - should fail
        let result = db.check_source_overlap(&workspace_id, temp_dir.path());
        assert!(
            result.is_err(),
            "Parent of any existing source should be rejected"
        );
    }
}

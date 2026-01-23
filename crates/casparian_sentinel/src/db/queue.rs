//! Job Queue implementation (dbx-compatible).
//!
//! Uses DbConnection for all queries to keep DB backend swappable.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbTimestamp, DbValue};
use casparian_protocol::types::{ObservedDataType, SchemaMismatch};
use casparian_protocol::{JobStatus, PluginStatus, ProcessingStatus};

use super::models::{DeadLetterJob, ParserHealth, ProcessingJob, QuarantinedRow};

/// Maximum number of retries before a job is marked as permanently failed
pub const MAX_RETRY_COUNT: i32 = 3;

/// Job details needed for processing
#[derive(Debug, Clone)]
pub struct JobDetails {
    pub job_id: i64,
    pub plugin_name: String,
    pub file_path: String,
    pub input_file: Option<String>,
}

/// Plugin manifest details needed for execution
#[derive(Debug, Clone)]
pub struct PluginDetails {
    pub source_code: String,
    pub env_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DispatchMetadata {
    pub file_id: i64,
    pub plugin_name: String,
    pub parser_version: Option<String>,
    pub parser_fingerprint: Option<String>,
    pub sink_config_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutputMaterialization {
    pub materialization_key: String,
    pub output_target_key: String,
    pub file_id: i64,
    pub file_mtime: i64,
    pub file_size: i64,
    pub plugin_name: String,
    pub parser_version: Option<String>,
    pub parser_fingerprint: String,
    pub output_name: String,
    pub sink_uri: String,
    pub sink_mode: String,
    pub table_name: Option<String>,
    pub schema_hash: Option<String>,
    pub status: String,
    pub rows: i64,
    pub job_id: i64,
}

/// Job queue for managing processing jobs.
pub struct JobQueue {
    conn: DbConnection,
}

fn now_ts() -> DbTimestamp {
    DbTimestamp::now()
}

fn observed_type_label(observed: &ObservedDataType) -> String {
    match observed {
        ObservedDataType::Canonical { data_type } => data_type.to_string(),
        ObservedDataType::Arrow { name } => format!("arrow:{}", name),
    }
}

impl JobQueue {
    /// Create a JobQueue from an existing connection.
    pub fn new(conn: DbConnection) -> Self {
        Self { conn }
    }

    /// Open a JobQueue from a database URL.
    pub fn open(db_url: &str) -> Result<Self> {
        let conn = DbConnection::open_from_url(db_url)?;
        Ok(Self { conn })
    }

    /// Initialize the processing queue schema (DuckDB v1).
    pub fn init_queue_schema(&self) -> Result<()> {
        let status_values = ProcessingStatus::ALL
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let completion_values = JobStatus::ALL
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let create_sql = format!(
            r#"
            CREATE SEQUENCE IF NOT EXISTS seq_cf_processing_queue;
            CREATE TABLE IF NOT EXISTS cf_processing_queue (
                id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_processing_queue'),
                file_id BIGINT NOT NULL,
                pipeline_run_id TEXT,
                plugin_name TEXT NOT NULL,
                input_file TEXT,
                config_overrides TEXT,
                parser_version TEXT,
                parser_fingerprint TEXT,
                sink_config_json TEXT,
                status TEXT NOT NULL DEFAULT '{default_status}'
                    CHECK (status IN ({status_values})),
                completion_status TEXT DEFAULT NULL
                    CHECK (completion_status IS NULL OR completion_status IN ({completion_values})),
                priority INTEGER DEFAULT 0,
                worker_host TEXT,
                worker_pid INTEGER,
                claim_time TIMESTAMP,
                scheduled_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                end_time TIMESTAMP,
                result_summary TEXT,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0,
                quarantine_rows BIGINT DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS ix_queue_pop ON cf_processing_queue(status, priority, id);

            CREATE TABLE IF NOT EXISTS cf_output_materializations (
                materialization_key TEXT PRIMARY KEY,
                output_target_key TEXT NOT NULL,
                file_id BIGINT NOT NULL,
                file_mtime BIGINT NOT NULL,
                file_size BIGINT NOT NULL,
                plugin_name TEXT NOT NULL,
                parser_version TEXT,
                parser_fingerprint TEXT,
                output_name TEXT NOT NULL,
                sink_uri TEXT NOT NULL,
                sink_mode TEXT NOT NULL,
                table_name TEXT,
                schema_hash TEXT,
                status TEXT NOT NULL
                    CHECK (status IN ('success', 'partial_success', 'no_data')),
                rows BIGINT NOT NULL DEFAULT 0,
                job_id BIGINT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS ix_materializations_file ON cf_output_materializations(file_id);
            CREATE INDEX IF NOT EXISTS ix_materializations_plugin ON cf_output_materializations(plugin_name);
            CREATE INDEX IF NOT EXISTS ix_materializations_target ON cf_output_materializations(output_target_key);
        "#,
            default_status = ProcessingStatus::Queued.as_str(),
            status_values = status_values,
            completion_values = completion_values
        );

        self.conn
            .execute_batch(&create_sql)
            .context("Failed to initialize cf_processing_queue schema")?;
        self.require_columns(
            "cf_processing_queue",
            &[
                "scheduled_at",
                "quarantine_rows",
                "parser_version",
                "parser_fingerprint",
                "sink_config_json",
            ],
        )?;
        Ok(())
    }

    /// Initialize plugin registry and topic configuration tables.
    pub fn init_registry_schema(&self) -> Result<()> {
        let plugin_status_values = PluginStatus::ALL
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let create_sql = format!(
            r#"
            CREATE SEQUENCE IF NOT EXISTS seq_cf_plugin_manifest;
            CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
                id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_plugin_manifest'),
                plugin_name TEXT NOT NULL,
                version TEXT NOT NULL,
                runtime_kind TEXT NOT NULL,
                entrypoint TEXT NOT NULL,
                platform_os TEXT,
                platform_arch TEXT,
                source_code TEXT NOT NULL,
                source_hash TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT '{default_status}'
                    CHECK (status IN ({plugin_status_values})),
                validation_error TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                deployed_at TIMESTAMP,
                env_hash TEXT NOT NULL,
                artifact_hash TEXT NOT NULL,
                manifest_json TEXT NOT NULL,
                protocol_version TEXT NOT NULL,
                schema_artifacts_json TEXT NOT NULL,
                outputs_json TEXT NOT NULL,
                publisher_name TEXT,
                publisher_email TEXT,
                azure_oid TEXT,
                system_requirements TEXT,
                signature_verified BOOLEAN DEFAULT false,
                signer_id TEXT,
                UNIQUE(plugin_name, version, runtime_kind, platform_os, platform_arch),
                UNIQUE(source_hash)
            );

            CREATE TABLE IF NOT EXISTS cf_plugin_environment (
                hash TEXT PRIMARY KEY,
                lockfile_content TEXT NOT NULL,
                size_mb DOUBLE,
                last_used TIMESTAMP,
                created_at TIMESTAMP
            );

            CREATE SEQUENCE IF NOT EXISTS seq_cf_topic_config;
            CREATE TABLE IF NOT EXISTS cf_topic_config (
                id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_topic_config'),
                plugin_name TEXT NOT NULL,
                topic_name TEXT NOT NULL,
                uri TEXT NOT NULL,
                mode TEXT DEFAULT 'append',
                quarantine_allow BOOLEAN,
                quarantine_max_pct DOUBLE,
                quarantine_max_count BIGINT,
                quarantine_dir TEXT
            );
            CREATE INDEX IF NOT EXISTS ix_topic_lookup ON cf_topic_config(plugin_name, topic_name);
            CREATE UNIQUE INDEX IF NOT EXISTS ux_topic_unique ON cf_topic_config(plugin_name, topic_name);
        "#,
            default_status = PluginStatus::Pending.as_str(),
            plugin_status_values = plugin_status_values
        );

        self.conn
            .execute_batch(&create_sql)
            .context("Failed to initialize registry schema")?;
        self.require_columns(
            "cf_plugin_manifest",
            &[
                "manifest_json",
                "protocol_version",
                "schema_artifacts_json",
                "runtime_kind",
                "entrypoint",
                "platform_os",
                "platform_arch",
                "signature_verified",
                "signer_id",
                "outputs_json",
            ],
        )?;
        self.require_columns(
            "cf_topic_config",
            &[
                "quarantine_allow",
                "quarantine_max_pct",
                "quarantine_max_count",
                "quarantine_dir",
            ],
        )?;
        Ok(())
    }

    /// Get job details for processing.
    ///
    /// Tries production path (JOIN through file_id) first,
    /// then falls back to input_file column for CLI/test jobs.
    pub fn get_job_details(&self, job_id: i64) -> Result<Option<JobDetails>> {
        let row = self.conn.query_optional(
            r#"
                SELECT
                    pq.plugin_name,
                    sf.path as full_path
                FROM cf_processing_queue pq
                JOIN scout_files sf ON pq.file_id = sf.id
                WHERE pq.id = ?
                "#,
            &[DbValue::from(job_id)],
        )?;

        if let Some(row) = row {
            return Ok(Some(JobDetails {
                job_id,
                plugin_name: row.get_by_name("plugin_name")?,
                file_path: row.get_by_name("full_path")?,
                input_file: None,
            }));
        }

        let row = self.conn.query_optional(
            r#"
                SELECT plugin_name, input_file
                FROM cf_processing_queue
                WHERE id = ? AND input_file IS NOT NULL
                "#,
            &[DbValue::from(job_id)],
        )?;

        row.map(|row| {
            let plugin_name: String = row
                .get_by_name("plugin_name")
                .context("Failed to read 'plugin_name' from cf_processing_queue")?;
            let input_file: String = row
                .get_by_name("input_file")
                .context("Failed to read 'input_file' from cf_processing_queue")?;
            Ok(JobDetails {
                job_id,
                plugin_name,
                file_path: input_file.clone(),
                input_file: Some(input_file),
            })
        })
        .transpose()
    }

    /// Claim a job by setting status to RUNNING.
    pub fn claim_job(&self, job_id: i64) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "UPDATE cf_processing_queue SET status = ?, claim_time = ? WHERE id = ?",
            &[
                DbValue::from(ProcessingStatus::Running.as_str()),
                DbValue::from(now),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Get plugin source code and env_hash from manifest.
    pub fn get_plugin_details(&self, plugin_name: &str) -> Result<Option<PluginDetails>> {
        let row = self.conn.query_optional(
            r#"
                SELECT source_code, env_hash
                FROM cf_plugin_manifest
                WHERE plugin_name = ? AND status IN (?, ?)
                ORDER BY deployed_at DESC
                LIMIT 1
                "#,
            &[
                DbValue::from(plugin_name),
                DbValue::from(PluginStatus::Active.as_str()),
                DbValue::from(PluginStatus::Deployed.as_str()),
            ],
        )?;

        row.map(|row| {
            Ok(PluginDetails {
                source_code: row
                    .get_by_name("source_code")
                    .context("Failed to read 'source_code' from cf_parsers")?,
                env_hash: row.get_by_name("env_hash").ok(),
            })
        })
        .transpose()
    }

    /// Get lockfile content from plugin environment.
    pub fn get_lockfile(&self, env_hash: &str) -> Result<Option<String>> {
        let row = self.conn.query_optional(
            "SELECT lockfile_content FROM cf_plugin_environment WHERE hash = ?",
            &[DbValue::from(env_hash)],
        )?;
        row.map(|row| {
            row.get_by_name("lockfile_content")
                .context("Failed to read 'lockfile_content' from cf_plugin_environment")
        })
        .transpose()
    }

    /// Peek at the next job without claiming it.
    pub fn peek_job(&self) -> Result<Option<ProcessingJob>> {
        let has_health = self.table_exists("cf_parser_health")?;
        let now = now_ts();
        let (query, params) = if has_health {
            (
                r#"
                SELECT q.*
                FROM cf_processing_queue q
                LEFT JOIN cf_parser_health ph ON ph.parser_name = q.plugin_name
                WHERE q.status = ?
                  AND (q.scheduled_at IS NULL OR q.scheduled_at <= ?)
                  AND (ph.paused_at IS NULL)
                ORDER BY q.priority DESC, q.id ASC
                LIMIT 1
                "#,
                vec![
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                    DbValue::from(now),
                ],
            )
        } else {
            (
                r#"
                SELECT *
                FROM cf_processing_queue
                WHERE status = ?
                  AND (scheduled_at IS NULL OR scheduled_at <= ?)
                ORDER BY priority DESC, id ASC
                LIMIT 1
                "#,
                vec![
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                    DbValue::from(now),
                ],
            )
        };

        let row = self.conn.query_optional(query, &params)?;
        Ok(row.map(|row| ProcessingJob::from_row(&row)).transpose()?)
    }

    /// Atomically pop a job from the queue.
    pub fn pop_job(&self) -> Result<Option<ProcessingJob>> {
        let has_health = self.table_exists("cf_parser_health")?;
        let now = now_ts();
        let (query, params) = if has_health {
            (
                r#"
                UPDATE cf_processing_queue
                SET status = ?, claim_time = ?
                WHERE id = (
                    SELECT q.id
                    FROM cf_processing_queue q
                    LEFT JOIN cf_parser_health ph ON ph.parser_name = q.plugin_name
                    WHERE q.status = ?
                      AND (q.scheduled_at IS NULL OR q.scheduled_at <= ?)
                      AND (ph.paused_at IS NULL)
                    ORDER BY q.priority DESC, q.id ASC
                    LIMIT 1
                )
                RETURNING *
                "#,
                vec![
                    DbValue::from(ProcessingStatus::Running.as_str()),
                    DbValue::from(now.clone()),
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                    DbValue::from(now),
                ],
            )
        } else {
            (
                r#"
                UPDATE cf_processing_queue
                SET status = ?, claim_time = ?
                WHERE id = (
                    SELECT id
                    FROM cf_processing_queue
                    WHERE status = ?
                      AND (scheduled_at IS NULL OR scheduled_at <= ?)
                    ORDER BY priority DESC, id ASC
                    LIMIT 1
                )
                RETURNING *
                "#,
                vec![
                    DbValue::from(ProcessingStatus::Running.as_str()),
                    DbValue::from(now.clone()),
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                    DbValue::from(now),
                ],
            )
        };

        let row = self.conn.query_optional(query, &params)?;

        Ok(row.map(|row| ProcessingJob::from_row(&row)).transpose()?)
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let (query, params) = match self.conn.backend_name() {
            "DuckDB" => (
                "SELECT 1 FROM information_schema.columns WHERE table_name = ? AND column_name = ?"
                    .to_string(),
                vec![DbValue::from(table), DbValue::from(column)],
            ),
            "SQLite" => (
                format!(
                    "SELECT 1 FROM pragma_table_info('{}') WHERE name = ?",
                    table.replace('\'', "''")
                ),
                vec![DbValue::from(column)],
            ),
            _ => (
                "SELECT 1 FROM information_schema.columns WHERE table_name = ? AND column_name = ?"
                    .to_string(),
                vec![DbValue::from(table), DbValue::from(column)],
            ),
        };

        Ok(self.conn.query_optional(&query, &params)?.is_some())
    }

    fn table_exists(&self, table: &str) -> Result<bool> {
        let (query, params) = match self.conn.backend_name() {
            "DuckDB" => (
                "SELECT 1 FROM information_schema.tables WHERE table_name = ?".to_string(),
                vec![DbValue::from(table)],
            ),
            "SQLite" => (
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?".to_string(),
                vec![DbValue::from(table)],
            ),
            _ => (
                "SELECT 1 FROM information_schema.tables WHERE table_name = ?".to_string(),
                vec![DbValue::from(table)],
            ),
        };

        Ok(self.conn.query_optional(&query, &params)?.is_some())
    }

    fn require_columns(&self, table: &str, columns: &[&str]) -> Result<()> {
        let mut missing = Vec::new();
        for column in columns {
            if !self.column_exists(table, column)? {
                missing.push(*column);
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        anyhow::bail!(
            "Database schema for '{}' is missing columns: {}. \
Delete the database (default: ~/.casparian_flow/casparian_flow.duckdb) and restart.",
            table,
            missing.join(", ")
        );
    }

    /// Mark job as complete with outcome details.
    ///
    /// `completion_status` should be one of: SUCCESS, PARTIAL_SUCCESS, COMPLETED_WITH_WARNINGS
    pub fn complete_job(
        &self,
        job_id: i64,
        completion_status: &str,
        summary: &str,
        quarantine_rows: Option<i64>,
    ) -> Result<()> {
        let now = now_ts();
        if let Some(rows) = quarantine_rows {
            self.conn.execute(
                r#"
                    UPDATE cf_processing_queue
                    SET status = ?,
                        completion_status = ?,
                        end_time = ?,
                        result_summary = ?,
                        quarantine_rows = ?
                    WHERE id = ?
                    "#,
                &[
                    DbValue::from(ProcessingStatus::Completed.as_str()),
                    DbValue::from(completion_status),
                    DbValue::from(now),
                    DbValue::from(summary),
                    DbValue::from(rows),
                    DbValue::from(job_id),
                ],
            )?;
        } else {
            self.conn.execute(
                r#"
                    UPDATE cf_processing_queue
                    SET status = ?,
                        completion_status = ?,
                        end_time = ?,
                        result_summary = ?
                    WHERE id = ?
                    "#,
                &[
                    DbValue::from(ProcessingStatus::Completed.as_str()),
                    DbValue::from(completion_status),
                    DbValue::from(now),
                    DbValue::from(summary),
                    DbValue::from(job_id),
                ],
            )?;
        }
        Ok(())
    }

    /// Persist dispatch metadata for a job (parser version/hash + sink config snapshot).
    pub fn record_dispatch_metadata(
        &self,
        job_id: i64,
        parser_version: &str,
        parser_fingerprint: &str,
        sink_config_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET parser_version = ?, parser_fingerprint = ?, sink_config_json = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(parser_version),
                DbValue::from(parser_fingerprint),
                DbValue::from(sink_config_json),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Load dispatch metadata for a job (used for idempotent materialization tracking).
    pub fn get_dispatch_metadata(&self, job_id: i64) -> Result<Option<DispatchMetadata>> {
        let row = self.conn.query_optional(
            r#"
            SELECT file_id, plugin_name, parser_version, parser_fingerprint, sink_config_json
            FROM cf_processing_queue
            WHERE id = ?
            "#,
            &[DbValue::from(job_id)],
        )?;

        row.map(|row| {
            Ok(DispatchMetadata {
                file_id: row
                    .get_by_name("file_id")
                    .context("Failed to read 'file_id' from cf_processing_queue")?,
                plugin_name: row
                    .get_by_name("plugin_name")
                    .context("Failed to read 'plugin_name' from cf_processing_queue")?,
                parser_version: row.get_by_name("parser_version").ok().flatten(),
                parser_fingerprint: row.get_by_name("parser_fingerprint").ok().flatten(),
                sink_config_json: row.get_by_name("sink_config_json").ok().flatten(),
            })
        })
        .transpose()
    }

    /// Record a completed output materialization (idempotent insert).
    pub fn insert_output_materialization(&self, record: &OutputMaterialization) -> Result<()> {
        self.conn.execute(
            r#"
                INSERT INTO cf_output_materializations (
                    materialization_key,
                    output_target_key,
                    file_id,
                    file_mtime,
                    file_size,
                    plugin_name,
                    parser_version,
                    parser_fingerprint,
                    output_name,
                    sink_uri,
                    sink_mode,
                    table_name,
                    schema_hash,
                    status,
                    rows,
                    job_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(materialization_key) DO NOTHING
                "#,
            &[
                DbValue::from(record.materialization_key.as_str()),
                DbValue::from(record.output_target_key.as_str()),
                DbValue::from(record.file_id),
                DbValue::from(record.file_mtime),
                DbValue::from(record.file_size),
                DbValue::from(record.plugin_name.as_str()),
                record
                    .parser_version
                    .as_deref()
                    .map(DbValue::from)
                    .unwrap_or(DbValue::Null),
                DbValue::from(record.parser_fingerprint.as_str()),
                DbValue::from(record.output_name.as_str()),
                DbValue::from(record.sink_uri.as_str()),
                DbValue::from(record.sink_mode.as_str()),
                record
                    .table_name
                    .as_deref()
                    .map(DbValue::from)
                    .unwrap_or(DbValue::Null),
                record
                    .schema_hash
                    .as_deref()
                    .map(DbValue::from)
                    .unwrap_or(DbValue::Null),
                DbValue::from(record.status.as_str()),
                DbValue::from(record.rows),
                DbValue::from(record.job_id),
            ],
        )?;
        Ok(())
    }

    /// Mark job as failed with outcome details.
    ///
    /// `completion_status` should be one of: FAILED, REJECTED, ABORTED
    pub fn fail_job(&self, job_id: i64, completion_status: &str, error: &str) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET status = ?,
                    completion_status = ?,
                    end_time = ?,
                    error_message = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(ProcessingStatus::Failed.as_str()),
                DbValue::from(completion_status),
                DbValue::from(now),
                DbValue::from(error),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Requeue a job.
    /// Clears terminal fields (completion_status, end_time, result_summary, error_message)
    /// when transitioning back to QUEUED state.
    pub fn requeue_job(&self, job_id: i64) -> Result<()> {
        let row = self.conn.query_optional(
            "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id)],
        )?;

        if let Some(row) = row {
            let retry_count: i32 = row.get_by_name("retry_count")?;
            if retry_count >= MAX_RETRY_COUNT {
                self.move_to_dead_letter(job_id, "max_retries_exceeded", "max_retries_exceeded")?;
                return Ok(());
            }
        }

        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET status = ?,
                    completion_status = NULL,
                    claim_time = NULL,
                    end_time = NULL,
                    result_summary = NULL,
                    error_message = NULL,
                    scheduled_at = ?,
                    retry_count = retry_count + 1
                WHERE id = ?
                "#,
            &[
                DbValue::from(ProcessingStatus::Queued.as_str()),
                DbValue::from(now_ts()),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Defer a job without incrementing retry count.
    /// Clears terminal fields when transitioning back to QUEUED state.
    pub fn defer_job(
        &self,
        job_id: i64,
        scheduled_at: DbTimestamp,
        reason: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET status = ?,
                    completion_status = NULL,
                    claim_time = NULL,
                    end_time = NULL,
                    result_summary = NULL,
                    scheduled_at = ?,
                    error_message = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(ProcessingStatus::Queued.as_str()),
                DbValue::from(scheduled_at),
                DbValue::from(reason),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Schedule a retry for a failed job with backoff.
    /// Clears terminal fields (except error_message which stores the retry reason)
    /// when transitioning back to QUEUED state.
    pub fn schedule_retry(
        &self,
        job_id: i64,
        next_retry_count: i32,
        error: &str,
        scheduled_at: DbTimestamp,
    ) -> Result<()> {
        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET status = ?,
                    completion_status = NULL,
                    retry_count = ?,
                    claim_time = NULL,
                    end_time = NULL,
                    result_summary = NULL,
                    scheduled_at = ?,
                    error_message = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(ProcessingStatus::Queued.as_str()),
                DbValue::from(next_retry_count),
                DbValue::from(scheduled_at),
                DbValue::from(error),
                DbValue::from(job_id),
            ],
        )?;
        Ok(())
    }

    /// Queue stats for monitoring.
    pub fn stats(&self) -> Result<QueueStats> {
        let row = self.conn.query_one(
            &format!(
                r#"
                SELECT
                    SUM(CASE WHEN status = '{queued}' THEN 1 ELSE 0 END) AS queued,
                    SUM(CASE WHEN status = '{running}' THEN 1 ELSE 0 END) AS running,
                    SUM(CASE WHEN status = '{completed}' THEN 1 ELSE 0 END) AS completed,
                    SUM(CASE WHEN status = '{failed}' THEN 1 ELSE 0 END) AS failed
                FROM cf_processing_queue
                "#,
                queued = ProcessingStatus::Queued.as_str(),
                running = ProcessingStatus::Running.as_str(),
                completed = ProcessingStatus::Completed.as_str(),
                failed = ProcessingStatus::Failed.as_str(),
            ),
            &[],
        )?;

        Ok(QueueStats {
            queued: row.get_by_name("queued")?,
            running: row.get_by_name("running")?,
            completed: row.get_by_name("completed")?,
            failed: row.get_by_name("failed")?,
        })
    }

    /// Initialize dead-letter, health, quarantine tables.
    pub fn init_error_handling_schema(&self) -> Result<()> {
        let sql = match self.conn.backend_name() {
            "DuckDB" => {
                r#"
                CREATE SEQUENCE IF NOT EXISTS seq_cf_dead_letter;
                CREATE SEQUENCE IF NOT EXISTS seq_cf_quarantine;
                CREATE SEQUENCE IF NOT EXISTS seq_cf_job_schema_mismatch;

                CREATE TABLE IF NOT EXISTS cf_dead_letter (
                    id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_dead_letter'),
                    original_job_id BIGINT NOT NULL,
                    file_id BIGINT,
                    plugin_name TEXT NOT NULL,
                    error_message TEXT,
                    retry_count INTEGER NOT NULL,
                    moved_at TIMESTAMP NOT NULL,
                    reason TEXT
                );

                CREATE TABLE IF NOT EXISTS cf_parser_health (
                    parser_name TEXT PRIMARY KEY,
                    total_executions INTEGER NOT NULL DEFAULT 0,
                    successful_executions INTEGER NOT NULL DEFAULT 0,
                    consecutive_failures INTEGER NOT NULL DEFAULT 0,
                    last_failure_reason TEXT,
                    paused_at TIMESTAMP,
                    created_at TIMESTAMP NOT NULL,
                    updated_at TIMESTAMP NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cf_quarantine (
                    id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_quarantine'),
                    job_id BIGINT NOT NULL,
                    row_index BIGINT NOT NULL,
                    error_reason TEXT NOT NULL,
                    raw_data BLOB,
                    created_at TIMESTAMP NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cf_job_schema_mismatch (
                    id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_job_schema_mismatch'),
                    job_id BIGINT NOT NULL,
                    output_name TEXT NOT NULL,
                    mismatch_kind TEXT NOT NULL,
                    expected_name TEXT,
                    actual_name TEXT,
                    expected_type TEXT,
                    actual_type TEXT,
                    expected_index INTEGER,
                    actual_index INTEGER,
                    created_at TIMESTAMP NOT NULL
                );

                CREATE INDEX IF NOT EXISTS ix_schema_mismatch_job
                    ON cf_job_schema_mismatch(job_id);
                "#
            }
            _ => {
                r#"
                CREATE TABLE IF NOT EXISTS cf_dead_letter (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original_job_id INTEGER NOT NULL,
                    file_id INTEGER,
                    plugin_name TEXT NOT NULL,
                    error_message TEXT,
                    retry_count INTEGER NOT NULL,
                    moved_at TIMESTAMP NOT NULL,
                    reason TEXT
                );

                CREATE TABLE IF NOT EXISTS cf_parser_health (
                    parser_name TEXT PRIMARY KEY,
                    total_executions INTEGER NOT NULL DEFAULT 0,
                    successful_executions INTEGER NOT NULL DEFAULT 0,
                    consecutive_failures INTEGER NOT NULL DEFAULT 0,
                    last_failure_reason TEXT,
                    paused_at TIMESTAMP,
                    created_at TIMESTAMP NOT NULL,
                    updated_at TIMESTAMP NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cf_quarantine (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id INTEGER NOT NULL,
                    row_index INTEGER NOT NULL,
                    error_reason TEXT NOT NULL,
                    raw_data BLOB,
                    created_at TIMESTAMP NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cf_job_schema_mismatch (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id INTEGER NOT NULL,
                    output_name TEXT NOT NULL,
                    mismatch_kind TEXT NOT NULL,
                    expected_name TEXT,
                    actual_name TEXT,
                    expected_type TEXT,
                    actual_type TEXT,
                    expected_index INTEGER,
                    actual_index INTEGER,
                    created_at TIMESTAMP NOT NULL
                );

                CREATE INDEX IF NOT EXISTS ix_schema_mismatch_job
                    ON cf_job_schema_mismatch(job_id);
                "#
            }
        };

        self.conn
            .execute(sql, &[])
            .context("Failed to initialize error handling schema")?;
        Ok(())
    }

    pub fn record_schema_mismatch(&self, job_id: i64, mismatch: &SchemaMismatch) -> Result<()> {
        let now = now_ts();

        for name in &mismatch.missing_columns {
            self.insert_schema_mismatch_row(
                job_id,
                &mismatch.output_name,
                "missing_column",
                Some(name.as_str()),
                None,
                None,
                None,
                None,
                None,
                now.clone(),
            )?;
        }

        for name in &mismatch.extra_columns {
            self.insert_schema_mismatch_row(
                job_id,
                &mismatch.output_name,
                "extra_column",
                None,
                Some(name.as_str()),
                None,
                None,
                None,
                None,
                now.clone(),
            )?;
        }

        for order in &mismatch.order_mismatches {
            let expected_index = i64::try_from(order.index).map_err(|_| {
                anyhow::anyhow!(
                    "schema mismatch index overflow for job {} output '{}'",
                    job_id,
                    mismatch.output_name
                )
            })?;
            self.insert_schema_mismatch_row(
                job_id,
                &mismatch.output_name,
                "order_mismatch",
                Some(order.expected.as_str()),
                Some(order.actual.as_str()),
                None,
                None,
                Some(expected_index),
                None,
                now.clone(),
            )?;
        }

        for type_mismatch in &mismatch.type_mismatches {
            let expected_type = type_mismatch.expected.to_string();
            let actual_type = observed_type_label(&type_mismatch.actual);
            self.insert_schema_mismatch_row(
                job_id,
                &mismatch.output_name,
                "type_mismatch",
                Some(type_mismatch.name.as_str()),
                None,
                Some(expected_type.as_str()),
                Some(actual_type.as_str()),
                None,
                None,
                now.clone(),
            )?;
        }

        Ok(())
    }

    fn insert_schema_mismatch_row(
        &self,
        job_id: i64,
        output_name: &str,
        mismatch_kind: &str,
        expected_name: Option<&str>,
        actual_name: Option<&str>,
        expected_type: Option<&str>,
        actual_type: Option<&str>,
        expected_index: Option<i64>,
        actual_index: Option<i64>,
        created_at: DbTimestamp,
    ) -> Result<()> {
        self.conn
            .execute(
                r#"
                INSERT INTO cf_job_schema_mismatch
                    (job_id, output_name, mismatch_kind, expected_name, actual_name, expected_type, actual_type, expected_index, actual_index, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                &[
                    DbValue::from(job_id),
                    DbValue::from(output_name),
                    DbValue::from(mismatch_kind),
                    DbValue::from(expected_name),
                    DbValue::from(actual_name),
                    DbValue::from(expected_type),
                    DbValue::from(actual_type),
                    DbValue::from(expected_index),
                    DbValue::from(actual_index),
                    DbValue::from(created_at),
                ],
            )
            ?;
        Ok(())
    }

    /// Move a job to dead letter.
    pub fn move_to_dead_letter(&self, job_id: i64, error: &str, reason: &str) -> Result<()> {
        let row = self.conn.query_optional(
            r#"
                SELECT file_id, plugin_name, retry_count
                FROM cf_processing_queue
                WHERE id = ?
                "#,
            &[DbValue::from(job_id)],
        )?;

        let Some(row) = row else {
            return Ok(());
        };

        let file_id: i64 = row.get_by_name("file_id")?;
        let plugin_name: String = row.get_by_name("plugin_name")?;
        let retry_count: i32 = row.get_by_name("retry_count")?;

        let now = now_ts();
        let full_error = format!("{}: {}", reason, error);
        self.conn
            .execute(
                r#"
                INSERT INTO cf_dead_letter (original_job_id, file_id, plugin_name, error_message, retry_count, moved_at, reason)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                &[
                    DbValue::from(job_id),
                    DbValue::from(file_id),
                    DbValue::from(plugin_name),
                    DbValue::from(full_error.as_str()),
                    DbValue::from(retry_count),
                    DbValue::from(now.clone()),
                    DbValue::from(reason),
                ],
            )
            ?;

        // Mark the job as FAILED with FAILED completion_status (dead-lettered)
        self.conn.execute(
            r#"
                UPDATE cf_processing_queue
                SET status = ?,
                    completion_status = ?,
                    end_time = ?,
                    error_message = ?
                WHERE id = ?
                "#,
            &[
                DbValue::from(ProcessingStatus::Failed.as_str()),
                DbValue::from(JobStatus::Failed.as_str()),
                DbValue::from(now_ts()),
                DbValue::from(full_error.as_str()),
                DbValue::from(job_id),
            ],
        )?;

        Ok(())
    }

    pub fn get_dead_letter_jobs(&self, limit: i64) -> Result<Vec<DeadLetterJob>> {
        let rows = self.conn.query_all(
            "SELECT * FROM cf_dead_letter ORDER BY moved_at DESC LIMIT ?",
            &[DbValue::from(limit)],
        )?;
        rows.iter()
            .map(DeadLetterJob::from_row)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    pub fn get_dead_letter_jobs_by_plugin(
        &self,
        plugin: &str,
        limit: i64,
    ) -> Result<Vec<DeadLetterJob>> {
        let rows = self.conn.query_all(
            "SELECT * FROM cf_dead_letter WHERE plugin_name = ? ORDER BY moved_at DESC LIMIT ?",
            &[DbValue::from(plugin), DbValue::from(limit)],
        )?;
        rows.iter()
            .map(DeadLetterJob::from_row)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    pub fn replay_dead_letter(&self, dead_letter_id: i64) -> Result<i64> {
        let row = self.conn.query_optional(
            "SELECT original_job_id, file_id, plugin_name FROM cf_dead_letter WHERE id = ?",
            &[DbValue::from(dead_letter_id)],
        )?;
        let Some(row) = row else {
            return Ok(0);
        };

        let file_id: Option<i64> = row.get_by_name("file_id")?;
        let plugin_name: String = row.get_by_name("plugin_name")?;

        let new_id = self
            .conn
            .query_one(
                r#"
                INSERT INTO cf_processing_queue (file_id, plugin_name, status)
                VALUES (?, ?, ?)
                RETURNING id
                "#,
                &[
                    DbValue::from(file_id.unwrap_or_default()),
                    DbValue::from(plugin_name),
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                ],
            )?
            .get_by_name::<i64>("id")?;

        self.conn.execute(
            "DELETE FROM cf_dead_letter WHERE id = ?",
            &[DbValue::from(dead_letter_id)],
        )?;

        Ok(new_id)
    }

    pub fn count_dead_letter_jobs(&self) -> Result<i64> {
        let row = self
            .conn
            .query_one("SELECT COUNT(*) AS cnt FROM cf_dead_letter", &[])?;
        Ok(row.get_by_name("cnt")?)
    }

    pub fn record_parser_success(&self, parser_name: &str) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                r#"
                INSERT INTO cf_parser_health (parser_name, total_executions, successful_executions, consecutive_failures, created_at, updated_at)
                VALUES (?, 1, 1, 0, ?, ?)
                ON CONFLICT(parser_name) DO UPDATE SET
                    total_executions = total_executions + 1,
                    successful_executions = successful_executions + 1,
                    consecutive_failures = 0,
                    updated_at = ?
                "#,
                &[
                    DbValue::from(parser_name),
                    DbValue::from(now.clone()),
                    DbValue::from(now.clone()),
                    DbValue::from(now),
                ],
            )
            ?;
        Ok(())
    }

    pub fn record_parser_failure(&self, parser_name: &str, reason: &str) -> Result<i32> {
        let now = now_ts();
        self.conn
            .execute(
                r#"
                INSERT INTO cf_parser_health (parser_name, total_executions, successful_executions, consecutive_failures, last_failure_reason, created_at, updated_at)
                VALUES (?, 1, 0, 1, ?, ?, ?)
                ON CONFLICT(parser_name) DO UPDATE SET
                    total_executions = total_executions + 1,
                    consecutive_failures = consecutive_failures + 1,
                    last_failure_reason = ?,
                    updated_at = ?
                "#,
                &[
                    DbValue::from(parser_name),
                    DbValue::from(reason),
                    DbValue::from(now.clone()),
                    DbValue::from(now.clone()),
                    DbValue::from(reason),
                    DbValue::from(now),
                ],
            )
            ?;

        let health = self.get_parser_health(parser_name)?;
        Ok(health.map(|h| h.consecutive_failures).unwrap_or(0))
    }

    pub fn pause_parser(&self, parser_name: &str) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "UPDATE cf_parser_health SET paused_at = ?, updated_at = ? WHERE parser_name = ?",
            &[
                DbValue::from(now.clone()),
                DbValue::from(now),
                DbValue::from(parser_name),
            ],
        )?;
        Ok(())
    }

    pub fn resume_parser(&self, parser_name: &str) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "UPDATE cf_parser_health SET paused_at = NULL, updated_at = ? WHERE parser_name = ?",
            &[DbValue::from(now), DbValue::from(parser_name)],
        )?;
        Ok(())
    }

    pub fn is_parser_paused(&self, parser_name: &str) -> Result<bool> {
        let row = self.conn.query_optional(
            "SELECT paused_at FROM cf_parser_health WHERE parser_name = ?",
            &[DbValue::from(parser_name)],
        )?;
        Ok(row
            .and_then(|r| r.get_by_name::<Option<DbTimestamp>>("paused_at").ok())
            .flatten()
            .is_some())
    }

    pub fn get_parser_health(&self, parser_name: &str) -> Result<Option<ParserHealth>> {
        let row = self.conn.query_optional(
            "SELECT * FROM cf_parser_health WHERE parser_name = ?",
            &[DbValue::from(parser_name)],
        )?;
        Ok(row.map(|row| ParserHealth::from_row(&row)).transpose()?)
    }

    pub fn get_all_parser_health(&self) -> Result<Vec<ParserHealth>> {
        let rows = self.conn.query_all("SELECT * FROM cf_parser_health", &[])?;
        rows.iter()
            .map(ParserHealth::from_row)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    pub fn quarantine_row(
        &self,
        job_id: i64,
        row_index: i32,
        error: &str,
        raw: Option<&[u8]>,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            r#"
                INSERT INTO cf_quarantine (job_id, row_index, error_reason, raw_data, created_at)
                VALUES (?, ?, ?, ?, ?)
                "#,
            &[
                DbValue::from(job_id),
                DbValue::from(row_index),
                DbValue::from(error),
                DbValue::from(raw.map(|v| v.to_vec())),
                DbValue::from(now),
            ],
        )?;
        Ok(())
    }

    pub fn get_quarantined_rows(&self, job_id: i64) -> Result<Vec<QuarantinedRow>> {
        let rows = self.conn.query_all(
            "SELECT * FROM cf_quarantine WHERE job_id = ? ORDER BY row_index",
            &[DbValue::from(job_id)],
        )?;
        rows.iter()
            .map(QuarantinedRow::from_row)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    pub fn count_quarantined_rows(&self, job_id: i64) -> Result<i64> {
        let row = self.conn.query_one(
            "SELECT COUNT(*) AS cnt FROM cf_quarantine WHERE job_id = ?",
            &[DbValue::from(job_id)],
        )?;
        Ok(row.get_by_name("cnt")?)
    }

    pub fn delete_quarantined_rows(&self, job_id: i64) -> Result<u64> {
        let affected = self.conn.execute(
            "DELETE FROM cf_quarantine WHERE job_id = ?",
            &[DbValue::from(job_id)],
        )?;
        Ok(affected)
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub queued: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
}

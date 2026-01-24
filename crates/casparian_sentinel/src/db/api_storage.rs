//! Storage layer for the Control Plane API.
//!
//! Manages jobs, events, and approvals in DuckDB tables.
//! Used directly by casparian_mcp to drive job execution.

use super::schema_version::{ensure_schema_version, SCHEMA_VERSION};
use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbTimestamp, DbValue, UnifiedDbRow};
use casparian_protocol::{
    ApiJobId, Approval, ApprovalOperation, ApprovalStatus, Event, EventId, EventType,
    HttpJobStatus, HttpJobType, Job, JobProgress, JobResult, OutputInfo,
};
use chrono::Duration;
use std::collections::HashMap;
use uuid::Uuid;

/// Storage for Control Plane API data.
///
/// This is the primary interface for MCP to manage jobs, events, and approvals.
/// All operations are synchronous and use the shared DuckDB connection.
pub struct ApiStorage {
    conn: DbConnection,
}

impl ApiStorage {
    /// Create new API storage from a database connection.
    pub fn new(conn: DbConnection) -> Self {
        Self { conn }
    }

    /// Open API storage from a database URL.
    pub fn open(db_url: &str) -> Result<Self> {
        let conn = DbConnection::open_from_url(db_url)?;
        Ok(Self { conn })
    }

    /// Get the underlying connection (for use with other modules).
    pub fn connection(&self) -> &DbConnection {
        &self.conn
    }

    /// Initialize the API schema (DDL).
    pub fn init_schema(&self) -> Result<()> {
        // Pre-v1: reset schema if version mismatched
        let _ = ensure_schema_version(&self.conn, SCHEMA_VERSION)?;
        let job_status_values = "'queued','running','completed','failed','cancelled'";
        let job_type_values = "'run','backtest','preview'";
        let approval_status_values = "'pending','approved','rejected','expired'";
        let event_type_values = "'job_started','phase','progress','violation','output','job_finished','approval_required'";

        let create_sql = format!(
            r#"
            -- API Jobs table
            CREATE SEQUENCE IF NOT EXISTS seq_cf_api_jobs;
            CREATE TABLE IF NOT EXISTS cf_api_jobs (
                job_id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_api_jobs'),
                job_type TEXT NOT NULL CHECK (job_type IN ({job_type_values})),
                status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ({job_status_values})),
                plugin_name TEXT NOT NULL,
                plugin_version TEXT,
                input_dir TEXT NOT NULL,
                output_sink TEXT,
                approval_id TEXT,
                job_spec_json TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                started_at TIMESTAMP,
                finished_at TIMESTAMP,
                error_message TEXT,
                progress_phase TEXT,
                progress_items_done BIGINT DEFAULT 0,
                progress_items_total BIGINT,
                progress_message TEXT,
                result_rows_processed BIGINT,
                result_bytes_written BIGINT,
                result_outputs_json TEXT,
                result_metrics_json TEXT
            );
            CREATE INDEX IF NOT EXISTS ix_api_jobs_status ON cf_api_jobs(status);
            CREATE INDEX IF NOT EXISTS ix_api_jobs_created ON cf_api_jobs(created_at DESC);
            CREATE INDEX IF NOT EXISTS ix_api_jobs_approval ON cf_api_jobs(approval_id);

            -- API Events table
            CREATE SEQUENCE IF NOT EXISTS seq_cf_api_events;
            CREATE TABLE IF NOT EXISTS cf_api_events (
                event_id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_api_events'),
                job_id BIGINT NOT NULL,
                event_type TEXT NOT NULL CHECK (event_type IN ({event_type_values})),
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                payload_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_api_events_job ON cf_api_events(job_id, event_id);

            -- API Approvals table
            CREATE TABLE IF NOT EXISTS cf_api_approvals (
                approval_id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ({approval_status_values})),
                operation_type TEXT NOT NULL,
                operation_json TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                expires_at TIMESTAMP NOT NULL,
                decided_at TIMESTAMP,
                decided_by TEXT,
                rejection_reason TEXT,
                job_id BIGINT
            );
            CREATE INDEX IF NOT EXISTS ix_api_approvals_status ON cf_api_approvals(status);
            CREATE INDEX IF NOT EXISTS ix_api_approvals_expires ON cf_api_approvals(expires_at);
            "#,
            job_type_values = job_type_values,
            job_status_values = job_status_values,
            approval_status_values = approval_status_values,
            event_type_values = event_type_values,
        );

        self.conn
            .execute_batch(&create_sql)
            .context("Failed to initialize API schema")?;

        Ok(())
    }

    // ========================================================================
    // Job Operations
    // ========================================================================

    /// Create a new job.
    pub fn create_job(
        &self,
        job_type: HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output_sink: Option<&str>,
        approval_id: Option<&str>,
        job_spec_json: Option<&str>,
    ) -> Result<ApiJobId> {
        let job_type_str = match job_type {
            HttpJobType::Run => "run",
            HttpJobType::Backtest => "backtest",
            HttpJobType::Preview => "preview",
        };

        let sql = r#"
            INSERT INTO cf_api_jobs (job_type, plugin_name, plugin_version, input_dir, output_sink, approval_id, job_spec_json)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            RETURNING job_id
        "#;

        let job_id_raw: i64 = self.conn.query_scalar(
            sql,
            &[
                DbValue::from(job_type_str),
                DbValue::from(plugin_name),
                DbValue::from(plugin_version),
                DbValue::from(input_dir),
                DbValue::from(output_sink),
                DbValue::from(approval_id),
                DbValue::from(job_spec_json),
            ],
        )?;
        let job_id = ApiJobId::try_from(job_id_raw).context("job_id must be non-negative")?;

        Ok(job_id)
    }

    /// Get a job by ID.
    pub fn get_job(&self, job_id: ApiJobId) -> Result<Option<Job>> {
        let sql = r#"
            SELECT job_id, job_type, status, plugin_name, plugin_version, input_dir, output_sink,
                   approval_id, job_spec_json, created_at, started_at, finished_at, error_message,
                   progress_phase, progress_items_done, progress_items_total, progress_message,
                   result_rows_processed, result_bytes_written, result_outputs_json, result_metrics_json
            FROM cf_api_jobs
            WHERE job_id = ?
        "#;

        let job_id_i64 = job_id.to_i64().context("job_id exceeds i64::MAX")?;
        let row = self
            .conn
            .query_optional(sql, &[DbValue::from(job_id_i64)])?;

        match row {
            Some(r) => Ok(Some(self.row_to_job(&r)?)),
            None => Ok(None),
        }
    }

    /// List jobs with optional status filter.
    pub fn list_jobs(&self, status: Option<HttpJobStatus>, limit: usize) -> Result<Vec<Job>> {
        let (sql, params) = match status {
            Some(s) => {
                let status_str = job_status_to_str(s);
                (
                    r#"
                    SELECT job_id, job_type, status, plugin_name, plugin_version, input_dir, output_sink,
                           approval_id, job_spec_json, created_at, started_at, finished_at, error_message,
                           progress_phase, progress_items_done, progress_items_total, progress_message,
                           result_rows_processed, result_bytes_written, result_outputs_json, result_metrics_json
                    FROM cf_api_jobs
                    WHERE status = ?
                    ORDER BY created_at DESC
                    LIMIT ?
                    "#
                    .to_string(),
                    vec![DbValue::from(status_str), DbValue::from(limit as i64)],
                )
            }
            None => (
                r#"
                SELECT job_id, job_type, status, plugin_name, plugin_version, input_dir, output_sink,
                       approval_id, job_spec_json, created_at, started_at, finished_at, error_message,
                       progress_phase, progress_items_done, progress_items_total, progress_message,
                       result_rows_processed, result_bytes_written, result_outputs_json, result_metrics_json
                FROM cf_api_jobs
                ORDER BY created_at DESC
                LIMIT ?
                "#
                .to_string(),
                vec![DbValue::from(limit as i64)],
            ),
        };

        let rows = self.conn.query_all(&sql, &params)?;
        rows.iter().map(|r| self.row_to_job(r)).collect()
    }

    /// Update job status.
    pub fn update_job_status(&self, job_id: ApiJobId, status: HttpJobStatus) -> Result<()> {
        let status_str = job_status_to_str(status);
        let now = DbTimestamp::now();
        let job_id_i64 = job_id.to_i64().context("job_id exceeds i64::MAX")?;

        let (sql, params) = match status {
            HttpJobStatus::Running => (
                r#"UPDATE cf_api_jobs SET status = ?, started_at = ? WHERE job_id = ?"#,
                vec![
                    DbValue::from(status_str),
                    DbValue::Timestamp(now),
                    DbValue::from(job_id_i64),
                ],
            ),
            HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled => (
                r#"UPDATE cf_api_jobs SET status = ?, finished_at = ? WHERE job_id = ?"#,
                vec![
                    DbValue::from(status_str),
                    DbValue::Timestamp(now),
                    DbValue::from(job_id_i64),
                ],
            ),
            HttpJobStatus::Queued => (
                r#"UPDATE cf_api_jobs SET status = ? WHERE job_id = ?"#,
                vec![DbValue::from(status_str), DbValue::from(job_id_i64)],
            ),
        };

        self.conn.execute(sql, &params)?;

        Ok(())
    }

    /// Update job progress.
    pub fn update_job_progress(
        &self,
        job_id: ApiJobId,
        phase: &str,
        items_done: u64,
        items_total: Option<u64>,
        message: Option<&str>,
    ) -> Result<()> {
        let sql = r#"
            UPDATE cf_api_jobs
            SET progress_phase = ?, progress_items_done = ?, progress_items_total = ?, progress_message = ?
            WHERE job_id = ?
        "#;

        let items_done_i64 = i64::try_from(items_done).context("items_done exceeds i64::MAX")?;
        let items_total_i64 = match items_total {
            Some(v) => Some(i64::try_from(v).context("items_total exceeds i64::MAX")?),
            None => None,
        };

        self.conn.execute(
            sql,
            &[
                DbValue::from(phase),
                DbValue::from(items_done_i64),
                DbValue::from(items_total_i64),
                DbValue::from(message),
                DbValue::from(job_id.to_i64().context("job_id exceeds i64::MAX")?),
            ],
        )?;

        Ok(())
    }

    /// Update job result.
    pub fn update_job_result(&self, job_id: ApiJobId, result: &JobResult) -> Result<()> {
        let outputs_json = serde_json::to_string(&result.outputs)?;
        let metrics_json = serde_json::to_string(&result.metrics)?;

        let sql = r#"
            UPDATE cf_api_jobs
            SET result_rows_processed = ?, result_bytes_written = ?,
                result_outputs_json = ?, result_metrics_json = ?
            WHERE job_id = ?
        "#;

        let rows_processed_i64 =
            i64::try_from(result.rows_processed).context("rows_processed exceeds i64::MAX")?;
        let bytes_written_i64 = match result.bytes_written {
            Some(v) => Some(i64::try_from(v).context("bytes_written exceeds i64::MAX")?),
            None => None,
        };

        self.conn.execute(
            sql,
            &[
                DbValue::from(rows_processed_i64),
                DbValue::from(bytes_written_i64),
                DbValue::from(outputs_json.as_str()),
                DbValue::from(metrics_json.as_str()),
                DbValue::from(job_id.to_i64().context("job_id exceeds i64::MAX")?),
            ],
        )?;

        Ok(())
    }

    /// Update job error message.
    pub fn update_job_error(&self, job_id: ApiJobId, error_message: &str) -> Result<()> {
        let sql = r#"UPDATE cf_api_jobs SET error_message = ? WHERE job_id = ?"#;
        self.conn.execute(
            sql,
            &[
                DbValue::from(error_message),
                DbValue::from(job_id.to_i64().context("job_id exceeds i64::MAX")?),
            ],
        )?;
        Ok(())
    }

    /// Cancel a job (if not in terminal state).
    pub fn cancel_job(&self, job_id: ApiJobId) -> Result<bool> {
        let job = self.get_job(job_id)?;
        match job {
            Some(j) if !is_terminal_status(j.status) => {
                self.update_job_status(job_id, HttpJobStatus::Cancelled)?;
                self.insert_event(
                    job_id,
                    &EventType::JobFinished {
                        status: HttpJobStatus::Cancelled,
                        error_message: Some(
                            casparian_protocol::defaults::CANCELLED_BY_USER_MESSAGE.to_string(),
                        ),
                    },
                )?;
                Ok(true)
            }
            Some(_) => Ok(false), // Already terminal
            None => Ok(false),    // Not found
        }
    }

    fn row_to_job(&self, row: &UnifiedDbRow) -> Result<Job> {
        let job_id_raw: i64 = row.get(0)?;
        let job_type_str: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let plugin_name: String = row.get(3)?;
        let plugin_version: Option<String> = row.get(4)?;
        let input_dir: String = row.get(5)?;
        let output: Option<String> = row.get(6)?;
        let approval_id: Option<String> = row.get(7)?;
        let job_spec_json: Option<String> = row.get(8)?;
        let created_at: DbTimestamp = row.get(9)?;
        let started_at: Option<DbTimestamp> = row.get(10)?;
        let finished_at: Option<DbTimestamp> = row.get(11)?;
        let error_message: Option<String> = row.get(12)?;
        let progress_phase: Option<String> = row.get(13)?;
        let progress_items_done: Option<i64> = row.get(14)?;
        let progress_items_total: Option<i64> = row.get(15)?;
        let progress_message: Option<String> = row.get(16)?;
        let result_rows: Option<i64> = row.get(17)?;
        let result_bytes: Option<i64> = row.get(18)?;
        let result_outputs_json: Option<String> = row.get(19)?;
        let result_metrics_json: Option<String> = row.get(20)?;

        let job_type = str_to_job_type(&job_type_str)?;
        let status = str_to_job_status(&status_str)?;

        let progress = match progress_phase {
            Some(phase) => {
                let items_done = progress_items_done
                    .unwrap_or(0)
                    .try_into()
                    .context("progress_items_done must be non-negative")?;
                let items_total = match progress_items_total {
                    Some(v) => Some(
                        v.try_into()
                            .context("progress_items_total must be non-negative")?,
                    ),
                    None => None,
                };
                Some(JobProgress {
                    phase,
                    items_done,
                    items_total,
                    message: progress_message,
                })
            }
            None => None,
        };

        let result = match result_rows {
            Some(rows) => {
                let rows_processed: u64 = rows
                    .try_into()
                    .context("result_rows_processed must be non-negative")?;
                let outputs: Vec<OutputInfo> = match result_outputs_json.as_ref() {
                    Some(s) => serde_json::from_str(s).context("Invalid result_outputs_json")?,
                    None => Vec::new(),
                };
                let metrics: HashMap<String, i64> = match result_metrics_json.as_ref() {
                    Some(s) => serde_json::from_str(s).context("Invalid result_metrics_json")?,
                    None => HashMap::new(),
                };
                let bytes_written = match result_bytes {
                    Some(b) => Some(
                        b.try_into()
                            .context("result_bytes_written must be non-negative")?,
                    ),
                    None => None,
                };

                Some(JobResult {
                    rows_processed,
                    bytes_written,
                    outputs,
                    metrics,
                })
            }
            None => None,
        };

        let job_id = ApiJobId::try_from(job_id_raw).context("job_id must be non-negative")?;

        Ok(Job {
            job_id,
            job_type,
            status,
            plugin_name,
            plugin_version,
            input_dir,
            output,
            created_at: created_at.to_rfc3339(),
            started_at: started_at.map(|t| t.to_rfc3339()),
            finished_at: finished_at.map(|t| t.to_rfc3339()),
            error_message,
            approval_id,
            progress,
            result,
            spec_json: job_spec_json,
        })
    }

    // ========================================================================
    // Event Operations
    // ========================================================================

    /// Insert an event.
    pub fn insert_event(&self, job_id: ApiJobId, event_type: &EventType) -> Result<EventId> {
        let event_type_str = event_type_to_str(event_type);
        let payload_json = serde_json::to_string(event_type)?;

        let sql = r#"
            INSERT INTO cf_api_events (job_id, event_type, payload_json)
            VALUES (?, ?, ?)
            RETURNING event_id
        "#;

        let event_id_raw: i64 = self.conn.query_scalar(
            sql,
            &[
                DbValue::from(job_id.to_i64().context("job_id exceeds i64::MAX")?),
                DbValue::from(event_type_str),
                DbValue::from(payload_json.as_str()),
            ],
        )?;
        let event_id: EventId = event_id_raw
            .try_into()
            .context("event_id must be non-negative")?;

        Ok(event_id)
    }

    /// List events for a job, optionally after a given event ID (for polling).
    pub fn list_events(
        &self,
        job_id: ApiJobId,
        after_event_id: Option<EventId>,
    ) -> Result<Vec<Event>> {
        let job_id_i64 = job_id.to_i64().context("job_id exceeds i64::MAX")?;
        let after_id_i64 = match after_event_id {
            Some(id) => Some(i64::try_from(id).context("event_id exceeds i64::MAX")?),
            None => None,
        };

        let (sql, params) = match after_id_i64 {
            Some(after_id) => (
                r#"
                SELECT event_id, job_id, timestamp, payload_json
                FROM cf_api_events
                WHERE job_id = ? AND event_id > ?
                ORDER BY event_id ASC
                "#
                .to_string(),
                vec![DbValue::from(job_id_i64), DbValue::from(after_id)],
            ),
            None => (
                r#"
                SELECT event_id, job_id, timestamp, payload_json
                FROM cf_api_events
                WHERE job_id = ?
                ORDER BY event_id ASC
                "#
                .to_string(),
                vec![DbValue::from(job_id_i64)],
            ),
        };

        let rows = self.conn.query_all(&sql, &params)?;
        rows.iter().map(|r| self.row_to_event(r)).collect()
    }

    fn row_to_event(&self, row: &UnifiedDbRow) -> Result<Event> {
        let event_id_raw: i64 = row.get(0)?;
        let job_id_raw: i64 = row.get(1)?;
        let timestamp: DbTimestamp = row.get(2)?;
        let payload_json: String = row.get(3)?;

        let event_type: EventType = serde_json::from_str(&payload_json)?;

        let event_id: EventId = event_id_raw
            .try_into()
            .context("event_id must be non-negative")?;
        let job_id = ApiJobId::try_from(job_id_raw).context("job_id must be non-negative")?;

        Ok(Event {
            event_id,
            job_id,
            timestamp: timestamp.to_rfc3339(),
            event_type,
        })
    }

    // ========================================================================
    // Approval Operations
    // ========================================================================

    /// Generate a new approval ID.
    pub fn generate_approval_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Create a new approval request.
    pub fn create_approval(
        &self,
        approval_id: &str,
        operation: &ApprovalOperation,
        summary: &str,
        expires_in: Duration,
    ) -> Result<()> {
        let operation_type = match operation {
            ApprovalOperation::Run { .. } => "run",
            ApprovalOperation::SchemaPromote { .. } => "schema_promote",
        };
        let operation_json = serde_json::to_string(operation)?;
        let expires_at = chrono::Utc::now() + expires_in;
        let expires_ts = DbTimestamp::from_unix_millis(expires_at.timestamp_millis())?;

        let sql = r#"
            INSERT INTO cf_api_approvals (approval_id, operation_type, operation_json, summary, expires_at)
            VALUES (?, ?, ?, ?, ?)
        "#;

        self.conn.execute(
            sql,
            &[
                DbValue::from(approval_id),
                DbValue::from(operation_type),
                DbValue::from(operation_json.as_str()),
                DbValue::from(summary),
                DbValue::Timestamp(expires_ts),
            ],
        )?;

        Ok(())
    }

    /// Get an approval by ID.
    pub fn get_approval(&self, approval_id: &str) -> Result<Option<Approval>> {
        let sql = r#"
            SELECT approval_id, status, operation_type, operation_json, summary,
                   created_at, expires_at, decided_at, decided_by, rejection_reason, job_id
            FROM cf_api_approvals
            WHERE approval_id = ?
        "#;

        let row = self
            .conn
            .query_optional(sql, &[DbValue::from(approval_id)])?;

        match row {
            Some(r) => Ok(Some(self.row_to_approval(&r)?)),
            None => Ok(None),
        }
    }

    /// List approvals with optional status filter.
    pub fn list_approvals(&self, status: Option<ApprovalStatus>) -> Result<Vec<Approval>> {
        let (sql, params) = match status {
            Some(s) => {
                let status_str = approval_status_to_str(s);
                (
                    r#"
                    SELECT approval_id, status, operation_type, operation_json, summary,
                           created_at, expires_at, decided_at, decided_by, rejection_reason, job_id
                    FROM cf_api_approvals
                    WHERE status = ?
                    ORDER BY created_at DESC
                    "#
                    .to_string(),
                    vec![DbValue::from(status_str)],
                )
            }
            None => (
                r#"
                SELECT approval_id, status, operation_type, operation_json, summary,
                       created_at, expires_at, decided_at, decided_by, rejection_reason, job_id
                FROM cf_api_approvals
                ORDER BY created_at DESC
                "#
                .to_string(),
                vec![],
            ),
        };

        let rows = self.conn.query_all(&sql, &params)?;
        rows.iter().map(|r| self.row_to_approval(r)).collect()
    }

    /// Approve an approval request.
    pub fn approve(&self, approval_id: &str, decided_by: Option<&str>) -> Result<bool> {
        let now = DbTimestamp::now();
        let sql = r#"
            UPDATE cf_api_approvals
            SET status = 'approved', decided_at = ?, decided_by = ?
            WHERE approval_id = ? AND status = 'pending'
        "#;

        let rows = self.conn.execute(
            sql,
            &[
                DbValue::Timestamp(now),
                DbValue::from(decided_by),
                DbValue::from(approval_id),
            ],
        )?;

        Ok(rows > 0)
    }

    /// Reject an approval request.
    pub fn reject(
        &self,
        approval_id: &str,
        decided_by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<bool> {
        let now = DbTimestamp::now();
        let sql = r#"
            UPDATE cf_api_approvals
            SET status = 'rejected', decided_at = ?, decided_by = ?, rejection_reason = ?
            WHERE approval_id = ? AND status = 'pending'
        "#;

        let rows = self.conn.execute(
            sql,
            &[
                DbValue::Timestamp(now),
                DbValue::from(decided_by),
                DbValue::from(reason),
                DbValue::from(approval_id),
            ],
        )?;

        Ok(rows > 0)
    }

    /// Mark expired approvals.
    pub fn expire_approvals(&self) -> Result<usize> {
        let now = DbTimestamp::now();
        let sql = r#"
            UPDATE cf_api_approvals
            SET status = 'expired'
            WHERE status = 'pending' AND expires_at < ?
        "#;

        let count = self.conn.execute(sql, &[DbValue::Timestamp(now)])?;
        Ok(count as usize)
    }

    /// Link a job to an approval.
    pub fn link_approval_to_job(&self, approval_id: &str, job_id: ApiJobId) -> Result<()> {
        let sql = r#"UPDATE cf_api_approvals SET job_id = ? WHERE approval_id = ?"#;
        self.conn.execute(
            sql,
            &[
                DbValue::from(job_id.to_i64().context("job_id exceeds i64::MAX")?),
                DbValue::from(approval_id),
            ],
        )?;
        Ok(())
    }

    fn row_to_approval(&self, row: &UnifiedDbRow) -> Result<Approval> {
        let approval_id: String = row.get(0)?;
        let status_str: String = row.get(1)?;
        let _operation_type: String = row.get(2)?;
        let operation_json: String = row.get(3)?;
        let summary: String = row.get(4)?;
        let created_at: DbTimestamp = row.get(5)?;
        let expires_at: DbTimestamp = row.get(6)?;
        let decided_at: Option<DbTimestamp> = row.get(7)?;
        let decided_by: Option<String> = row.get(8)?;
        let rejection_reason: Option<String> = row.get(9)?;
        let job_id_raw: Option<i64> = row.get(10)?;

        let status = str_to_approval_status(&status_str)?;
        let operation: ApprovalOperation = serde_json::from_str(&operation_json)?;

        Ok(Approval {
            approval_id,
            status,
            operation,
            summary,
            created_at: created_at.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
            decided_at: decided_at.map(|t| t.to_rfc3339()),
            decided_by,
            rejection_reason,
            job_id: match job_id_raw {
                Some(id) => {
                    Some(ApiJobId::try_from(id).context("approval job_id must be non-negative")?)
                }
                None => None,
            },
        })
    }

    // ========================================================================
    // Cleanup Operations
    // ========================================================================

    /// Clean up old jobs and events (TTL enforcement).
    pub fn cleanup_old_data(
        &self,
        job_ttl_hours: i64,
        event_ttl_hours: i64,
    ) -> Result<(usize, usize)> {
        let job_cutoff = chrono::Utc::now() - Duration::hours(job_ttl_hours);
        let event_cutoff = chrono::Utc::now() - Duration::hours(event_ttl_hours);

        let job_cutoff_ts = DbTimestamp::from_unix_millis(job_cutoff.timestamp_millis())?;
        let event_cutoff_ts = DbTimestamp::from_unix_millis(event_cutoff.timestamp_millis())?;

        // Delete old events first (foreign key consideration)
        let events_deleted = self.conn.execute(
            r#"DELETE FROM cf_api_events WHERE timestamp < ?"#,
            &[DbValue::Timestamp(event_cutoff_ts)],
        )?;

        // Delete old jobs with terminal status
        let jobs_deleted = self.conn.execute(
            r#"DELETE FROM cf_api_jobs WHERE created_at < ? AND status IN ('completed', 'failed', 'cancelled')"#,
            &[DbValue::Timestamp(job_cutoff_ts)],
        )?;

        Ok((jobs_deleted as usize, events_deleted as usize))
    }
}

// Helper functions for status conversion

fn job_status_to_str(status: HttpJobStatus) -> &'static str {
    match status {
        HttpJobStatus::Queued => "queued",
        HttpJobStatus::Running => "running",
        HttpJobStatus::Completed => "completed",
        HttpJobStatus::Failed => "failed",
        HttpJobStatus::Cancelled => "cancelled",
    }
}

fn str_to_job_status(s: &str) -> Result<HttpJobStatus> {
    match s {
        "queued" => Ok(HttpJobStatus::Queued),
        "running" => Ok(HttpJobStatus::Running),
        "completed" => Ok(HttpJobStatus::Completed),
        "failed" => Ok(HttpJobStatus::Failed),
        "cancelled" => Ok(HttpJobStatus::Cancelled),
        other => anyhow::bail!("Unknown job status: {}", other),
    }
}

fn str_to_job_type(s: &str) -> Result<HttpJobType> {
    match s {
        "run" => Ok(HttpJobType::Run),
        "backtest" => Ok(HttpJobType::Backtest),
        "preview" => Ok(HttpJobType::Preview),
        other => anyhow::bail!("Unknown job type: {}", other),
    }
}

fn approval_status_to_str(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::Expired => "expired",
    }
}

fn str_to_approval_status(s: &str) -> Result<ApprovalStatus> {
    match s {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "rejected" => Ok(ApprovalStatus::Rejected),
        "expired" => Ok(ApprovalStatus::Expired),
        other => anyhow::bail!("Unknown approval status: {}", other),
    }
}

fn event_type_to_str(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::JobStarted => "job_started",
        EventType::Phase { .. } => "phase",
        EventType::Progress { .. } => "progress",
        EventType::Violation { .. } => "violation",
        EventType::Output { .. } => "output",
        EventType::JobFinished { .. } => "job_finished",
        EventType::ApprovalRequired { .. } => "approval_required",
    }
}

fn is_terminal_status(status: HttpJobStatus) -> bool {
    matches!(
        status,
        HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_storage() -> ApiStorage {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let storage = ApiStorage::new(conn);
        storage.init_schema().unwrap();
        storage
    }

    #[test]
    fn test_create_and_get_job() {
        let storage = setup_storage();

        let job_id = storage
            .create_job(
                HttpJobType::Run,
                "test_parser",
                Some("1.0.0"),
                "/data/input",
                Some("parquet://./output"),
                None,
                None,
            )
            .unwrap();

        let job = storage.get_job(job_id).unwrap().unwrap();
        assert_eq!(job.job_id, job_id);
        assert_eq!(job.plugin_name, "test_parser");
        assert_eq!(job.plugin_version, Some("1.0.0".to_string()));
        assert_eq!(job.status, HttpJobStatus::Queued);
    }

    #[test]
    fn test_update_job_status() {
        let storage = setup_storage();

        let job_id = storage
            .create_job(
                HttpJobType::Backtest,
                "parser",
                None,
                "/input",
                None,
                None,
                None,
            )
            .unwrap();

        storage
            .update_job_status(job_id, HttpJobStatus::Running)
            .unwrap();
        let job = storage.get_job(job_id).unwrap().unwrap();
        assert_eq!(job.status, HttpJobStatus::Running);
        assert!(job.started_at.is_some());

        storage
            .update_job_status(job_id, HttpJobStatus::Completed)
            .unwrap();
        let job = storage.get_job(job_id).unwrap().unwrap();
        assert_eq!(job.status, HttpJobStatus::Completed);
        assert!(job.finished_at.is_some());
    }

    #[test]
    fn test_events_monotonic_ordering() {
        let storage = setup_storage();

        let job_id = storage
            .create_job(HttpJobType::Run, "parser", None, "/input", None, None, None)
            .unwrap();

        let event1_id = storage
            .insert_event(job_id, &EventType::JobStarted)
            .unwrap();
        assert_eq!(event1_id, 1);

        let event2_id = storage
            .insert_event(
                job_id,
                &EventType::Progress {
                    items_done: 10,
                    items_total: Some(100),
                    message: Some("Processing".to_string()),
                },
            )
            .unwrap();
        assert_eq!(event2_id, 2);

        let event3_id = storage
            .insert_event(
                job_id,
                &EventType::JobFinished {
                    status: HttpJobStatus::Completed,
                    error_message: None,
                },
            )
            .unwrap();
        assert_eq!(event3_id, 3);

        let events = storage.list_events(job_id, None).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_id, 1);
        assert_eq!(events[1].event_id, 2);
        assert_eq!(events[2].event_id, 3);

        // Test polling after event_id
        let events_after = storage.list_events(job_id, Some(1)).unwrap();
        assert_eq!(events_after.len(), 2);
        assert_eq!(events_after[0].event_id, 2);
    }

    #[test]
    fn test_approvals_workflow() {
        let storage = setup_storage();

        let approval_id = ApiStorage::generate_approval_id();
        let operation = ApprovalOperation::Run {
            plugin_name: "test_parser".to_string(),
            plugin_version: Some("1.0.0".to_string()),
            input_dir: "/data/input".to_string(),
            file_count: 100,
            output: Some("parquet://./output".to_string()),
        };

        storage
            .create_approval(
                &approval_id,
                &operation,
                "Run test_parser on 100 files",
                Duration::hours(1),
            )
            .unwrap();

        let approval = storage.get_approval(&approval_id).unwrap().unwrap();
        assert_eq!(approval.approval_id, approval_id);
        assert_eq!(approval.status, ApprovalStatus::Pending);

        // Approve
        let approved = storage
            .approve(&approval_id, Some("user@example.com"))
            .unwrap();
        assert!(approved);

        let approval = storage.get_approval(&approval_id).unwrap().unwrap();
        assert_eq!(approval.status, ApprovalStatus::Approved);
        assert!(approval.decided_at.is_some());

        // Can't approve again
        let approved_again = storage.approve(&approval_id, None).unwrap();
        assert!(!approved_again);
    }

    #[test]
    fn test_approval_rejection() {
        let storage = setup_storage();

        let approval_id = ApiStorage::generate_approval_id();
        let operation = ApprovalOperation::Run {
            plugin_name: "dangerous_parser".to_string(),
            plugin_version: None,
            input_dir: "/sensitive".to_string(),
            file_count: 1000,
            output: None,
        };

        storage
            .create_approval(
                &approval_id,
                &operation,
                "Run dangerous_parser",
                Duration::hours(1),
            )
            .unwrap();

        let rejected = storage
            .reject(&approval_id, Some("admin"), Some("Too risky"))
            .unwrap();
        assert!(rejected);

        let approval = storage.get_approval(&approval_id).unwrap().unwrap();
        assert_eq!(approval.status, ApprovalStatus::Rejected);
        assert_eq!(approval.rejection_reason, Some("Too risky".to_string()));
    }

    #[test]
    fn test_list_jobs_by_status() {
        let storage = setup_storage();

        let job1 = storage
            .create_job(
                HttpJobType::Run,
                "parser1",
                None,
                "/input1",
                None,
                None,
                None,
            )
            .unwrap();
        let job2 = storage
            .create_job(
                HttpJobType::Run,
                "parser2",
                None,
                "/input2",
                None,
                None,
                None,
            )
            .unwrap();

        storage
            .update_job_status(job1, HttpJobStatus::Completed)
            .unwrap();

        let queued = storage.list_jobs(Some(HttpJobStatus::Queued), 10).unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].job_id, job2);

        let completed = storage
            .list_jobs(Some(HttpJobStatus::Completed), 10)
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].job_id, job1);

        let all = storage.list_jobs(None, 10).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_cancel_job() {
        let storage = setup_storage();

        let job_id = storage
            .create_job(HttpJobType::Run, "parser", None, "/input", None, None, None)
            .unwrap();

        // Can cancel queued job
        let cancelled = storage.cancel_job(job_id).unwrap();
        assert!(cancelled);

        let job = storage.get_job(job_id).unwrap().unwrap();
        assert_eq!(job.status, HttpJobStatus::Cancelled);

        // Can't cancel again
        let cancelled_again = storage.cancel_job(job_id).unwrap();
        assert!(!cancelled_again);
    }

    #[test]
    fn test_job_progress_and_result() {
        let storage = setup_storage();

        let job_id = storage
            .create_job(
                HttpJobType::Backtest,
                "parser",
                None,
                "/input",
                None,
                None,
                None,
            )
            .unwrap();

        storage
            .update_job_status(job_id, HttpJobStatus::Running)
            .unwrap();
        storage
            .update_job_progress(job_id, "parsing", 50, Some(100), Some("Half done"))
            .unwrap();

        let job = storage.get_job(job_id).unwrap().unwrap();
        assert!(job.progress.is_some());
        let progress = job.progress.unwrap();
        assert_eq!(progress.phase, "parsing");
        assert_eq!(progress.items_done, 50);
        assert_eq!(progress.items_total, Some(100));

        // Complete with result
        let result = JobResult {
            rows_processed: 1000,
            bytes_written: Some(50000),
            outputs: vec![OutputInfo {
                name: "orders".to_string(),
                sink_uri: "parquet://./output/orders.parquet".to_string(),
                rows: 1000,
                bytes: Some(50000),
            }],
            metrics: [("duration_ms".to_string(), 1234)].into_iter().collect(),
        };

        storage.update_job_result(job_id, &result).unwrap();
        storage
            .update_job_status(job_id, HttpJobStatus::Completed)
            .unwrap();

        let job = storage.get_job(job_id).unwrap().unwrap();
        assert!(job.result.is_some());
        let r = job.result.unwrap();
        assert_eq!(r.rows_processed, 1000);
        assert_eq!(r.outputs.len(), 1);
    }
}

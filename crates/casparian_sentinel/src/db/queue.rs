//! Job Queue implementation (dbx-compatible).
//!
//! Uses DbConnection for all queries to keep DB backend swappable.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbTimestamp, DbValue};

use super::models::{DeadLetterJob, ParserHealth, ProcessingJob, QuarantinedRow};

/// Maximum number of retries before a job is marked as permanently failed
pub const MAX_RETRY_COUNT: i32 = 5;

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

/// Job queue for managing processing jobs.
pub struct JobQueue {
    conn: DbConnection,
}

fn now_ts() -> DbTimestamp {
    DbTimestamp::now()
}

impl JobQueue {
    /// Create a JobQueue from an existing connection.
    pub fn new(conn: DbConnection) -> Self {
        Self { conn }
    }

    /// Open a JobQueue from a database URL.
    pub async fn open(db_url: &str) -> Result<Self> {
        let conn = DbConnection::open_from_url(db_url).await?;
        Ok(Self { conn })
    }

    /// Get job details for processing.
    ///
    /// Tries production path (JOIN through file_id) first,
    /// then falls back to input_file column for CLI/test jobs.
    pub async fn get_job_details(&self, job_id: i64) -> Result<Option<JobDetails>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT
                    pq.plugin_name,
                    sf.path as full_path
                FROM cf_processing_queue pq
                JOIN scout_files sf ON pq.file_id = sf.id
                WHERE pq.id = ?
                "#,
                &[DbValue::from(job_id)],
            )
            .await?;

        if let Some(row) = row {
            return Ok(Some(JobDetails {
                job_id,
                plugin_name: row.get_by_name("plugin_name")?,
                file_path: row.get_by_name("full_path")?,
                input_file: None,
            }));
        }

        let row = self
            .conn
            .query_optional(
                r#"
                SELECT plugin_name, input_file
                FROM cf_processing_queue
                WHERE id = ? AND input_file IS NOT NULL
                "#,
                &[DbValue::from(job_id)],
            )
            .await?;

        Ok(row.map(|row| {
            let plugin_name: String = row.get_by_name("plugin_name").unwrap_or_default();
            let input_file: String = row.get_by_name("input_file").unwrap_or_default();
            JobDetails {
                job_id,
                plugin_name,
                file_path: input_file.clone(),
                input_file: Some(input_file),
            }
        }))
    }

    /// Claim a job by setting status to RUNNING.
    pub async fn claim_job(&self, job_id: i64) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                "UPDATE cf_processing_queue SET status = 'RUNNING', claim_time = ? WHERE id = ?",
                &[DbValue::from(now), DbValue::from(job_id)],
            )
            .await?;
        Ok(())
    }

    /// Get plugin source code and env_hash from manifest.
    pub async fn get_plugin_details(&self, plugin_name: &str) -> Result<Option<PluginDetails>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT source_code, env_hash
                FROM cf_plugin_manifest
                WHERE plugin_name = ? AND status IN ('ACTIVE', 'DEPLOYED')
                ORDER BY deployed_at DESC
                LIMIT 1
                "#,
                &[DbValue::from(plugin_name)],
            )
            .await?;

        Ok(row.map(|row| PluginDetails {
            source_code: row.get_by_name("source_code").unwrap_or_default(),
            env_hash: row.get_by_name("env_hash").ok(),
        }))
    }

    /// Get lockfile content from plugin environment.
    pub async fn get_lockfile(&self, env_hash: &str) -> Result<Option<String>> {
        let row = self
            .conn
            .query_optional(
                "SELECT lockfile_content FROM cf_plugin_environment WHERE hash = ?",
                &[DbValue::from(env_hash)],
            )
            .await?;
        Ok(row.map(|row| row.get_by_name("lockfile_content").unwrap_or_default()))
    }

    /// Peek at the next job without claiming it.
    pub async fn peek_job(&self) -> Result<Option<ProcessingJob>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT * FROM cf_processing_queue
                WHERE status = 'QUEUED'
                ORDER BY priority DESC, id ASC
                LIMIT 1
                "#,
                &[],
            )
            .await?;
        Ok(row.map(|row| ProcessingJob::from_row(&row)).transpose()?)
    }

    /// Atomically pop a job from the queue.
    pub async fn pop_job(&self) -> Result<Option<ProcessingJob>> {
        let now = now_ts();
        let row = self
            .conn
            .query_optional(
                r#"
                UPDATE cf_processing_queue
                SET status = 'RUNNING', claim_time = ?
                WHERE id = (
                    SELECT id FROM cf_processing_queue
                    WHERE status = 'QUEUED'
                    ORDER BY priority DESC, id ASC
                    LIMIT 1
                )
                RETURNING *
                "#,
                &[DbValue::from(now)],
            )
            .await?;

        Ok(row.map(|row| ProcessingJob::from_row(&row)).transpose()?)
    }

    /// Mark job as complete.
    pub async fn complete_job(&self, job_id: i64, summary: &str) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                r#"
                UPDATE cf_processing_queue
                SET status = 'COMPLETED', end_time = ?, result_summary = ?
                WHERE id = ?
                "#,
                &[DbValue::from(now), DbValue::from(summary), DbValue::from(job_id)],
            )
            .await?;
        Ok(())
    }

    /// Mark job as failed.
    pub async fn fail_job(&self, job_id: i64, error: &str) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                r#"
                UPDATE cf_processing_queue
                SET status = 'FAILED', end_time = ?, error_message = ?
                WHERE id = ?
                "#,
                &[DbValue::from(now), DbValue::from(error), DbValue::from(job_id)],
            )
            .await?;
        Ok(())
    }

    /// Requeue a job.
    pub async fn requeue_job(&self, job_id: i64) -> Result<()> {
        let row = self
            .conn
            .query_optional(
                "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .await?;

        if let Some(row) = row {
            let retry_count: i32 = row.get_by_name("retry_count")?;
            if retry_count >= MAX_RETRY_COUNT {
                let now = now_ts();
                self.conn
                    .execute(
                        r#"
                        UPDATE cf_processing_queue
                        SET status = 'FAILED', end_time = ?, error_message = 'max_retries_exceeded'
                        WHERE id = ?
                        "#,
                        &[DbValue::from(now), DbValue::from(job_id)],
                    )
                    .await?;
                return Ok(());
            }
        }

        self.conn
            .execute(
                r#"
                UPDATE cf_processing_queue
                SET status = 'QUEUED', claim_time = NULL, retry_count = retry_count + 1
                WHERE id = ?
                "#,
                &[DbValue::from(job_id)],
            )
            .await?;
        Ok(())
    }

    /// Queue stats for monitoring.
    pub async fn stats(&self) -> Result<QueueStats> {
        let row = self
            .conn
            .query_one(
                r#"
                SELECT
                    SUM(CASE WHEN status = 'QUEUED' THEN 1 ELSE 0 END) AS queued,
                    SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) AS running,
                    SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) AS completed,
                    SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) AS failed
                FROM cf_processing_queue
                "#,
                &[],
            )
            .await?;

        Ok(QueueStats {
            queued: row.get_by_name("queued")?,
            running: row.get_by_name("running")?,
            completed: row.get_by_name("completed")?,
            failed: row.get_by_name("failed")?,
        })
    }

    /// Initialize dead-letter, health, quarantine tables.
    pub async fn init_error_handling_schema(&self) -> Result<()> {
        self.conn
            .execute(
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
                "#,
                &[],
            )
            .await
            .context("Failed to initialize error handling schema")?;
        Ok(())
    }

    /// Move a job to dead letter.
    pub async fn move_to_dead_letter(&self, job_id: i64, error: &str, reason: &str) -> Result<()> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT file_id, plugin_name, retry_count
                FROM cf_processing_queue
                WHERE id = ?
                "#,
                &[DbValue::from(job_id)],
            )
            .await?;

        let Some(row) = row else {
            return Ok(());
        };

        let file_id: i32 = row.get_by_name("file_id")?;
        let plugin_name: String = row.get_by_name("plugin_name")?;
        let retry_count: i32 = row.get_by_name("retry_count")?;

        let now = now_ts();
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
                    DbValue::from(error),
                    DbValue::from(retry_count),
                    DbValue::from(now),
                    DbValue::from(reason),
                ],
            )
            .await?;

        self.conn
            .execute("DELETE FROM cf_processing_queue WHERE id = ?", &[DbValue::from(job_id)])
            .await?;

        Ok(())
    }

    pub async fn get_dead_letter_jobs(&self, limit: i64) -> Result<Vec<DeadLetterJob>> {
        let rows = self
            .conn
            .query_all(
                "SELECT * FROM cf_dead_letter ORDER BY moved_at DESC LIMIT ?",
                &[DbValue::from(limit)],
            )
            .await?;
        rows.iter().map(DeadLetterJob::from_row).collect::<Result<_, _>>().map_err(Into::into)
    }

    pub async fn get_dead_letter_jobs_by_plugin(&self, plugin: &str, limit: i64) -> Result<Vec<DeadLetterJob>> {
        let rows = self
            .conn
            .query_all(
                "SELECT * FROM cf_dead_letter WHERE plugin_name = ? ORDER BY moved_at DESC LIMIT ?",
                &[DbValue::from(plugin), DbValue::from(limit)],
            )
            .await?;
        rows.iter().map(DeadLetterJob::from_row).collect::<Result<_, _>>().map_err(Into::into)
    }

    pub async fn replay_dead_letter(&self, dead_letter_id: i64) -> Result<i64> {
        let row = self
            .conn
            .query_optional(
                "SELECT original_job_id, file_id, plugin_name FROM cf_dead_letter WHERE id = ?",
                &[DbValue::from(dead_letter_id)],
            )
            .await?;
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
                VALUES (?, ?, 'QUEUED')
                RETURNING id
                "#,
                &[DbValue::from(file_id.unwrap_or_default()), DbValue::from(plugin_name)],
            )
            .await?
            .get_by_name::<i64>("id")?;

        self.conn
            .execute("DELETE FROM cf_dead_letter WHERE id = ?", &[DbValue::from(dead_letter_id)])
            .await?;

        Ok(new_id)
    }

    pub async fn count_dead_letter_jobs(&self) -> Result<i64> {
        let row = self.conn.query_one("SELECT COUNT(*) AS cnt FROM cf_dead_letter", &[]).await?;
        Ok(row.get_by_name("cnt")?)
    }

    pub async fn record_parser_success(&self, parser_name: &str) -> Result<()> {
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
            .await?;
        Ok(())
    }

    pub async fn record_parser_failure(&self, parser_name: &str, reason: &str) -> Result<i32> {
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
            .await?;

        let health = self.get_parser_health(parser_name).await?;
        Ok(health.map(|h| h.consecutive_failures).unwrap_or(0))
    }

    pub async fn pause_parser(&self, parser_name: &str) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                "UPDATE cf_parser_health SET paused_at = ?, updated_at = ? WHERE parser_name = ?",
                &[
                    DbValue::from(now.clone()),
                    DbValue::from(now),
                    DbValue::from(parser_name),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn resume_parser(&self, parser_name: &str) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
                "UPDATE cf_parser_health SET paused_at = NULL, updated_at = ? WHERE parser_name = ?",
                &[DbValue::from(now), DbValue::from(parser_name)],
            )
            .await?;
        Ok(())
    }

    pub async fn is_parser_paused(&self, parser_name: &str) -> Result<bool> {
        let row = self
            .conn
            .query_optional(
                "SELECT paused_at FROM cf_parser_health WHERE parser_name = ?",
                &[DbValue::from(parser_name)],
            )
            .await?;
        Ok(row
            .and_then(|r| r.get_by_name::<Option<DbTimestamp>>("paused_at").ok())
            .flatten()
            .is_some())
    }

    pub async fn get_parser_health(&self, parser_name: &str) -> Result<Option<ParserHealth>> {
        let row = self
            .conn
            .query_optional(
                "SELECT * FROM cf_parser_health WHERE parser_name = ?",
                &[DbValue::from(parser_name)],
            )
            .await?;
        Ok(row.map(|row| ParserHealth::from_row(&row)).transpose()?)
    }

    pub async fn get_all_parser_health(&self) -> Result<Vec<ParserHealth>> {
        let rows = self.conn.query_all("SELECT * FROM cf_parser_health", &[]).await?;
        rows.iter().map(ParserHealth::from_row).collect::<Result<_, _>>().map_err(Into::into)
    }

    pub async fn quarantine_row(&self, job_id: i64, row_index: i32, error: &str, raw: Option<&[u8]>) -> Result<()> {
        let now = now_ts();
        self.conn
            .execute(
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
            )
            .await?;
        Ok(())
    }

    pub async fn get_quarantined_rows(&self, job_id: i64) -> Result<Vec<QuarantinedRow>> {
        let rows = self
            .conn
            .query_all("SELECT * FROM cf_quarantine WHERE job_id = ? ORDER BY row_index", &[DbValue::from(job_id)])
            .await?;
        rows.iter().map(QuarantinedRow::from_row).collect::<Result<_, _>>().map_err(Into::into)
    }

    pub async fn count_quarantined_rows(&self, job_id: i64) -> Result<i64> {
        let row = self
            .conn
            .query_one("SELECT COUNT(*) AS cnt FROM cf_quarantine WHERE job_id = ?", &[DbValue::from(job_id)])
            .await?;
        Ok(row.get_by_name("cnt")?)
    }

    pub async fn delete_quarantined_rows(&self, job_id: i64) -> Result<u64> {
        let affected = self
            .conn
            .execute("DELETE FROM cf_quarantine WHERE job_id = ?", &[DbValue::from(job_id)])
            .await?;
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

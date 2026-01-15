//! Job Queue implementation
//!
//! Provides atomic job claiming via SQL UPDATE ... WHERE.
//! Ported from Python queue.py with improved type safety.

use anyhow::Result;
use casparian_db::{DbConfig, DbPool, create_pool};
use chrono::Utc;
use tracing::{info, warn};

use super::models::{DeadLetterJob, ParserHealth, ProcessingJob, QuarantinedRow};

/// Maximum number of retries before a job is marked as permanently failed
/// This prevents infinite retry loops for jobs that consistently fail
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
    pool: DbPool,
}

impl JobQueue {
    /// Create a JobQueue from an existing pool.
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Open a JobQueue from a database path (convenience constructor for CLI).
    pub async fn open(db_path: &std::path::Path) -> Result<Self> {
        let config = DbConfig::sqlite(&db_path.display().to_string());
        let pool = create_pool(config).await?;
        Ok(Self { pool })
    }

    /// Get job details for processing
    ///
    /// Tries production path (JOIN through file_version_id) first,
    /// then falls back to input_file column for CLI/test jobs.
    pub async fn get_job_details(&self, job_id: i64) -> Result<Option<JobDetails>> {
        // Try production path first
        let result: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT
                pq.plugin_name,
                sr.path || '/' || fl.rel_path as full_path
            FROM cf_processing_queue pq
            JOIN cf_file_version fv ON pq.file_version_id = fv.id
            JOIN cf_file_location fl ON fv.location_id = fl.id
            JOIN cf_source_root sr ON fl.source_root_id = sr.id
            WHERE pq.id = ?
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((plugin_name, file_path)) = result {
            return Ok(Some(JobDetails {
                job_id,
                plugin_name,
                file_path,
                input_file: None,
            }));
        }

        // Fallback: use input_file directly (for test jobs or CLI-created jobs)
        let result: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT plugin_name, input_file
            FROM cf_processing_queue
            WHERE id = ? AND input_file IS NOT NULL
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(plugin_name, input_file)| JobDetails {
            job_id,
            plugin_name,
            file_path: input_file.clone(),
            input_file: Some(input_file),
        }))
    }

    /// Claim a job by setting status to RUNNING
    pub async fn claim_job(&self, job_id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE cf_processing_queue SET status = 'RUNNING', claim_time = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(job_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get plugin source code and env_hash from manifest
    pub async fn get_plugin_details(&self, plugin_name: &str) -> Result<Option<PluginDetails>> {
        let result: Option<(String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT source_code, env_hash
            FROM cf_plugin_manifest
            WHERE plugin_name = ? AND status IN ('ACTIVE', 'DEPLOYED')
            ORDER BY deployed_at DESC
            LIMIT 1
            "#,
        )
        .bind(plugin_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(source_code, env_hash)| PluginDetails {
            source_code,
            env_hash,
        }))
    }

    /// Get lockfile content from plugin environment
    pub async fn get_lockfile(&self, env_hash: &str) -> Result<Option<String>> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT lockfile_content FROM cf_plugin_environment WHERE hash = ?",
        )
        .bind(env_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(s,)| s))
    }

    /// Peek at the next job without claiming it.
    /// Used to check if a capable worker exists before popping.
    pub async fn peek_job(&self) -> Result<Option<ProcessingJob>> {
        let job: Option<ProcessingJob> = sqlx::query_as(
            r#"
            SELECT * FROM cf_processing_queue
            WHERE status = 'QUEUED'
            ORDER BY priority DESC, id ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    /// Atomically pop a job from the queue (SQLite version)
    ///
    /// Uses UPDATE ... WHERE status = 'QUEUED' with ORDER BY priority DESC, id ASC
    /// to claim the highest priority job atomically.
    pub async fn pop_job(&self) -> Result<Option<ProcessingJob>> {
        let mut tx = self.pool.begin().await?;

        // Find the next job to claim
        let job_id: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT id FROM cf_processing_queue
            WHERE status = 'QUEUED'
            ORDER BY priority DESC, id ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(job_id) = job_id else {
            tx.commit().await?;
            return Ok(None);
        };

        // Claim the job by updating status to RUNNING
        let now = Utc::now().to_rfc3339();
        let rows_affected = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'RUNNING',
                claim_time = ?
            WHERE id = ? AND status = 'QUEUED'
            "#,
        )
        .bind(&now)
        .bind(job_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            // Job was claimed by another worker - race condition
            tx.commit().await?;
            return Ok(None);
        }

        // Fetch the claimed job
        let job: ProcessingJob = sqlx::query_as(
            r#"
            SELECT * FROM cf_processing_queue WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        info!("Claimed job {}: {}", job.id, job.plugin_name);

        Ok(Some(job))
    }

    /// Mark a job as completed
    pub async fn complete_job(&self, job_id: i64, summary: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'COMPLETED',
                end_time = ?,
                result_summary = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(summary)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        info!("Job {} completed: {}", job_id, summary);
        Ok(())
    }

    /// Mark a job as failed
    pub async fn fail_job(&self, job_id: i64, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'FAILED',
                end_time = ?,
                error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(error)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        info!("Job {} failed: {}", job_id, error);
        Ok(())
    }

    /// Requeue a job (move from RUNNING back to QUEUED)
    ///
    /// If the job has exceeded MAX_RETRY_COUNT, it is marked as FAILED instead.
    /// This prevents infinite retry loops for jobs that consistently fail.
    pub async fn requeue_job(&self, job_id: i64) -> Result<()> {
        // First check the current retry count
        let current_retry: Option<i32> = sqlx::query_scalar(
            "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(retry_count) = current_retry else {
            warn!("Cannot requeue job {}: not found in queue", job_id);
            return Ok(());
        };

        if retry_count >= MAX_RETRY_COUNT {
            // Exceeded max retries - fail permanently
            warn!(
                "Job {} exceeded max retries ({}/{}), marking as FAILED",
                job_id, retry_count, MAX_RETRY_COUNT
            );
            self.fail_job(
                job_id,
                &format!("Exceeded maximum retry count ({})", MAX_RETRY_COUNT),
            ).await?;
            return Ok(());
        }

        // Requeue with incremented retry count
        sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'QUEUED',
                claim_time = NULL,
                retry_count = retry_count + 1
            WHERE id = ?
            "#,
        )
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        info!("Job {} requeued (retry {}/{})", job_id, retry_count + 1, MAX_RETRY_COUNT);
        Ok(())
    }

    /// Get queue statistics
    pub async fn stats(&self) -> Result<QueueStats> {
        let stats: QueueStats = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'QUEUED') as queued,
                COUNT(*) FILTER (WHERE status = 'RUNNING') as running,
                COUNT(*) FILTER (WHERE status = 'COMPLETED') as completed,
                COUNT(*) FILTER (WHERE status = 'FAILED') as failed
            FROM cf_processing_queue
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }

    // ========================================================================
    // Error Handling Tables (W5)
    // ========================================================================

    /// Initialize error handling tables (dead letter queue, parser health, quarantine)
    ///
    /// These tables support:
    /// - Dead Letter Queue: Jobs that have exhausted retries
    /// - Parser Health: Circuit breaker state for parsers
    /// - Quarantine: Row-level failures during processing
    pub async fn init_error_handling_schema(&self) -> Result<()> {
        // Dead Letter Queue: Jobs that have exhausted retries
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_dead_letter (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                original_job_id INTEGER NOT NULL,
                file_version_id INTEGER,
                plugin_name TEXT NOT NULL,
                error_message TEXT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                moved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                reason TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Index for querying dead letter jobs by plugin
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS ix_dead_letter_plugin ON cf_dead_letter(plugin_name)"
        )
        .execute(&self.pool)
        .await?;

        // Index for querying dead letter jobs by time
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS ix_dead_letter_moved_at ON cf_dead_letter(moved_at)"
        )
        .execute(&self.pool)
        .await?;

        // Parser Health: Circuit breaker state for parsers
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_parser_health (
                parser_name TEXT PRIMARY KEY,
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                paused_at TEXT,
                last_failure_reason TEXT,
                total_executions INTEGER NOT NULL DEFAULT 0,
                successful_executions INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Quarantine: Row-level failures during processing
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_quarantine (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                row_index INTEGER NOT NULL,
                error_reason TEXT NOT NULL,
                raw_data BLOB,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Index for querying quarantined rows by job
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS ix_quarantine_job ON cf_quarantine(job_id)"
        )
        .execute(&self.pool)
        .await?;

        info!("Initialized error handling schema (dead_letter, parser_health, quarantine)");
        Ok(())
    }

    // ========================================================================
    // Dead Letter Queue Operations
    // ========================================================================

    /// Move a job to the dead letter queue
    ///
    /// This is called when a job has exhausted all retries and should be
    /// permanently removed from the processing queue.
    pub async fn move_to_dead_letter(&self, job_id: i64, error: &str, reason: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Get job details
        let job: Option<(i32, String, i32)> = sqlx::query_as(
            "SELECT file_version_id, plugin_name, retry_count FROM cf_processing_queue WHERE id = ?"
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((file_version_id, plugin_name, retry_count)) = job else {
            warn!("Cannot move job {} to dead letter: not found", job_id);
            return Ok(());
        };

        let now = Utc::now().to_rfc3339();

        // 2. Insert into cf_dead_letter
        sqlx::query(
            r#"
            INSERT INTO cf_dead_letter (original_job_id, file_version_id, plugin_name, error_message, retry_count, moved_at, reason)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(job_id)
        .bind(file_version_id)
        .bind(&plugin_name)
        .bind(error)
        .bind(retry_count)
        .bind(&now)
        .bind(reason)
        .execute(&mut *tx)
        .await?;

        // 3. Delete from cf_processing_queue
        sqlx::query("DELETE FROM cf_processing_queue WHERE id = ?")
            .bind(job_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        info!(
            "Moved job {} ({}) to dead letter queue: {}",
            job_id, plugin_name, reason
        );
        Ok(())
    }

    /// Get all dead letter jobs (limited)
    pub async fn get_dead_letter_jobs(&self, limit: i64) -> Result<Vec<DeadLetterJob>> {
        let jobs: Vec<DeadLetterJob> = sqlx::query_as(
            "SELECT * FROM cf_dead_letter ORDER BY moved_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }

    /// Get dead letter jobs filtered by plugin
    pub async fn get_dead_letter_jobs_by_plugin(
        &self,
        plugin_name: &str,
        limit: i64,
    ) -> Result<Vec<DeadLetterJob>> {
        let jobs: Vec<DeadLetterJob> = sqlx::query_as(
            "SELECT * FROM cf_dead_letter WHERE plugin_name = ? ORDER BY moved_at DESC LIMIT ?"
        )
        .bind(plugin_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }

    /// Replay a dead letter job (move back to queue)
    ///
    /// Returns the new job_id if successful.
    pub async fn replay_dead_letter(&self, dead_letter_id: i64) -> Result<i64> {
        let mut tx = self.pool.begin().await?;

        // 1. Get dead letter job details
        let dlj: Option<DeadLetterJob> = sqlx::query_as(
            "SELECT * FROM cf_dead_letter WHERE id = ?"
        )
        .bind(dead_letter_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(dead_letter) = dlj else {
            anyhow::bail!("Dead letter job {} not found", dead_letter_id);
        };

        // 2. Create new job in cf_processing_queue
        let result = sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority, retry_count)
            VALUES (?, ?, 'QUEUED', 0, 0)
            "#,
        )
        .bind(dead_letter.file_version_id.unwrap_or(0))
        .bind(&dead_letter.plugin_name)
        .execute(&mut *tx)
        .await?;

        let new_job_id = result.last_insert_rowid();

        // 3. Delete from cf_dead_letter
        sqlx::query("DELETE FROM cf_dead_letter WHERE id = ?")
            .bind(dead_letter_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        info!(
            "Replayed dead letter {} as new job {} ({})",
            dead_letter_id, new_job_id, dead_letter.plugin_name
        );
        Ok(new_job_id)
    }

    /// Count dead letter jobs
    pub async fn count_dead_letter_jobs(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cf_dead_letter")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    // ========================================================================
    // Parser Health Operations (Circuit Breaker)
    // ========================================================================

    /// Record a successful parser execution
    pub async fn record_parser_success(&self, parser_name: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cf_parser_health (parser_name, consecutive_failures, total_executions, successful_executions)
            VALUES (?, 0, 1, 1)
            ON CONFLICT(parser_name) DO UPDATE SET
                consecutive_failures = 0,
                paused_at = NULL,
                total_executions = total_executions + 1,
                successful_executions = successful_executions + 1
            "#,
        )
        .bind(parser_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record a parser failure
    ///
    /// Returns the new consecutive failure count.
    pub async fn record_parser_failure(&self, parser_name: &str, reason: &str) -> Result<i32> {
        sqlx::query(
            r#"
            INSERT INTO cf_parser_health (parser_name, consecutive_failures, last_failure_reason, total_executions)
            VALUES (?, 1, ?, 1)
            ON CONFLICT(parser_name) DO UPDATE SET
                consecutive_failures = consecutive_failures + 1,
                last_failure_reason = ?,
                total_executions = total_executions + 1
            "#,
        )
        .bind(parser_name)
        .bind(reason)
        .bind(reason)
        .execute(&self.pool)
        .await?;

        // Get the updated count
        let health: Option<ParserHealth> = sqlx::query_as(
            "SELECT * FROM cf_parser_health WHERE parser_name = ?"
        )
        .bind(parser_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(health.map(|h| h.consecutive_failures).unwrap_or(1))
    }

    /// Pause a parser (circuit breaker open)
    pub async fn pause_parser(&self, parser_name: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE cf_parser_health SET paused_at = ? WHERE parser_name = ?"
        )
        .bind(&now)
        .bind(parser_name)
        .execute(&self.pool)
        .await?;

        warn!("Parser {} has been paused (circuit breaker open)", parser_name);
        Ok(())
    }

    /// Resume a parser (circuit breaker closed)
    pub async fn resume_parser(&self, parser_name: &str) -> Result<()> {
        sqlx::query(
            "UPDATE cf_parser_health SET paused_at = NULL, consecutive_failures = 0 WHERE parser_name = ?"
        )
        .bind(parser_name)
        .execute(&self.pool)
        .await?;

        info!("Parser {} has been resumed (circuit breaker closed)", parser_name);
        Ok(())
    }

    /// Check if a parser is paused
    pub async fn is_parser_paused(&self, parser_name: &str) -> Result<bool> {
        let health: Option<ParserHealth> = sqlx::query_as(
            "SELECT * FROM cf_parser_health WHERE parser_name = ?"
        )
        .bind(parser_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(health.map(|h| h.paused_at.is_some()).unwrap_or(false))
    }

    /// Get parser health status
    pub async fn get_parser_health(&self, parser_name: &str) -> Result<Option<ParserHealth>> {
        let health: Option<ParserHealth> = sqlx::query_as(
            "SELECT * FROM cf_parser_health WHERE parser_name = ?"
        )
        .bind(parser_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(health)
    }

    /// Get all parser health records
    pub async fn get_all_parser_health(&self) -> Result<Vec<ParserHealth>> {
        let health: Vec<ParserHealth> = sqlx::query_as(
            "SELECT * FROM cf_parser_health ORDER BY parser_name"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(health)
    }

    // ========================================================================
    // Quarantine Operations (Row-Level Failures)
    // ========================================================================

    /// Quarantine a row that failed processing
    pub async fn quarantine_row(
        &self,
        job_id: i64,
        row_index: i32,
        error_reason: &str,
        raw_data: Option<&[u8]>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO cf_quarantine (job_id, row_index, error_reason, raw_data, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(job_id)
        .bind(row_index)
        .bind(error_reason)
        .bind(raw_data)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get quarantined rows for a job
    pub async fn get_quarantined_rows(&self, job_id: i64) -> Result<Vec<QuarantinedRow>> {
        let rows: Vec<QuarantinedRow> = sqlx::query_as(
            "SELECT * FROM cf_quarantine WHERE job_id = ? ORDER BY row_index"
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Count quarantined rows for a job
    pub async fn count_quarantined_rows(&self, job_id: i64) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cf_quarantine WHERE job_id = ?"
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }

    /// Delete quarantined rows for a job (e.g., after successful reprocessing)
    pub async fn delete_quarantined_rows(&self, job_id: i64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM cf_quarantine WHERE job_id = ?")
            .bind(job_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct QueueStats {
    pub queued: i32,
    pub running: i32,
    pub completed: i32,
    pub failed: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .connect(":memory:")
            .await
            .unwrap();

        // Create test table
        sqlx::query(
            r#"
            CREATE TABLE cf_processing_queue (
                id INTEGER PRIMARY KEY,
                file_version_id INTEGER NOT NULL,
                plugin_name TEXT NOT NULL,
                config_overrides TEXT,
                status TEXT NOT NULL DEFAULT 'PENDING',
                priority INTEGER DEFAULT 0,
                worker_host TEXT,
                worker_pid INTEGER,
                claim_time TEXT,
                end_time TEXT,
                result_summary TEXT,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_pop_job_empty_queue() {
        let pool = setup_test_db().await;
        let queue = JobQueue::new(pool);

        let job = queue.pop_job().await.unwrap();
        assert!(job.is_none());
    }

    #[tokio::test]
    async fn test_pop_job_priority_order() {
        let pool = setup_test_db().await;

        // Insert jobs with different priorities
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
            VALUES (1, 'low', 'QUEUED', 0), (2, 'high', 'QUEUED', 10), (3, 'medium', 'QUEUED', 5)
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let queue = JobQueue::new(pool);

        // Should pop highest priority first
        let job = queue.pop_job().await.unwrap().unwrap();
        assert_eq!(job.plugin_name, "high");
        assert_eq!(job.priority, 10);
    }

    #[tokio::test]
    async fn test_complete_job() {
        use crate::db::models::StatusEnum;

        let pool = setup_test_db().await;

        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (id, file_version_id, plugin_name, status)
            VALUES (1, 1, 'test', 'RUNNING')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let queue = JobQueue::new(pool.clone());
        queue.complete_job(1, "Success").await.unwrap();

        let job: ProcessingJob = sqlx::query_as("SELECT * FROM cf_processing_queue WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(job.status, StatusEnum::Completed);
        assert_eq!(job.result_summary, Some("Success".to_string()));
    }

    #[tokio::test]
    async fn test_fail_job() {
        use crate::db::models::StatusEnum;

        let pool = setup_test_db().await;

        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (id, file_version_id, plugin_name, status)
            VALUES (1, 1, 'test', 'RUNNING')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let queue = JobQueue::new(pool.clone());
        queue.fail_job(1, "Connection timeout").await.unwrap();

        let job: ProcessingJob = sqlx::query_as("SELECT * FROM cf_processing_queue WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(job.status, StatusEnum::Failed);
        assert_eq!(job.error_message, Some("Connection timeout".to_string()));
        assert!(job.end_time.is_some());
    }

    #[tokio::test]
    async fn test_requeue_job() {
        use crate::db::models::StatusEnum;

        let pool = setup_test_db().await;

        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (id, file_version_id, plugin_name, status, retry_count)
            VALUES (1, 1, 'test', 'RUNNING', 0)
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let queue = JobQueue::new(pool.clone());
        queue.requeue_job(1).await.unwrap();

        let job: ProcessingJob = sqlx::query_as("SELECT * FROM cf_processing_queue WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(job.status, StatusEnum::Queued);
        assert_eq!(job.retry_count, 1);
        assert!(job.claim_time.is_none());
    }

    #[tokio::test]
    async fn test_requeue_exceeds_max_retries() {
        use crate::db::models::StatusEnum;

        let pool = setup_test_db().await;

        // Insert job that has already been retried MAX_RETRY_COUNT times
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (id, file_version_id, plugin_name, status, retry_count)
            VALUES (1, 1, 'test', 'RUNNING', 5)
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let queue = JobQueue::new(pool.clone());
        queue.requeue_job(1).await.unwrap();

        // Job should be marked as FAILED, not requeued
        let job: ProcessingJob = sqlx::query_as("SELECT * FROM cf_processing_queue WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(job.status, StatusEnum::Failed);
        assert!(job.error_message.unwrap().contains("maximum retry count"));
    }
}

//! Job Queue implementation
//!
//! Provides atomic job claiming via SQL UPDATE ... WHERE for both SQLite and PostgreSQL.
//! Ported from Python queue.py with improved type safety.

use anyhow::Result;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use tracing::{info, warn};

use super::models::ProcessingJob;

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

pub struct JobQueue {
    pool: Pool<Sqlite>,
}

impl JobQueue {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Open a JobQueue from a database path (convenience constructor for CLI)
    pub async fn open(db_path: &std::path::Path) -> Result<Self> {
        use sqlx::sqlite::SqlitePoolOptions;

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

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

    async fn setup_test_db() -> Pool<Sqlite> {
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

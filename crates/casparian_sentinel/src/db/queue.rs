//! Job Queue implementation
//!
//! Provides atomic job claiming via SQL UPDATE ... WHERE for both SQLite and PostgreSQL.
//! Ported from Python queue.py with improved type safety.

use anyhow::Result;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use tracing::info;

use super::models::ProcessingJob;

pub struct JobQueue {
    pool: Pool<Sqlite>,
}

impl JobQueue {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Atomically pop a job from the queue (SQLite version)
    ///
    /// Uses UPDATE ... WHERE status = 'QUEUED' with ORDER BY priority DESC, id ASC
    /// to claim the highest priority job atomically.
    pub async fn pop_job(&self) -> Result<Option<ProcessingJob>> {
        let mut tx = self.pool.begin().await?;

        // Find the next job to claim
        let job_id: Option<i32> = sqlx::query_scalar(
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
    pub async fn complete_job(&self, job_id: i32, summary: &str) -> Result<()> {
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
    pub async fn fail_job(&self, job_id: i32, error: &str) -> Result<()> {
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
    pub async fn requeue_job(&self, job_id: i32) -> Result<()> {
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

        info!("Job {} requeued", job_id);
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
}

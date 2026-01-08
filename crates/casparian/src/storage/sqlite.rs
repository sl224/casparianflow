//! SQLite implementations of the storage traits.
//!
//! This module provides SQLite-backed implementations of JobStore,
//! ParserStore, and QuarantineStore using sqlx for async database access.

use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::Path;
use std::time::Duration;

use super::traits::{Job, JobStore, ParserBundle, ParserStore, QuarantinedRow, QuarantineStore};

/// SQLite-backed job store implementation.
pub struct SqliteJobStore {
    pool: SqlitePool,
}

impl SqliteJobStore {
    /// Create a new SQLite job store with the given database path.
    pub async fn new(db_path: &Path) -> Result<Self> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .context("Failed to connect to SQLite database")?;

        let store = Self { pool };
        store.initialize_tables().await?;
        Ok(store)
    }

    /// Create a new SQLite job store from an existing pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize required tables if they don't exist.
    async fn initialize_tables(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_processing_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_version_id INTEGER NOT NULL,
                plugin_name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'QUEUED',
                priority INTEGER NOT NULL DEFAULT 0,
                retry_count INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                worker_id TEXT,
                claim_time TEXT,
                heartbeat_time TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at TEXT,
                output_path TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_processing_queue table")?;

        // Create index for efficient job claiming
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cf_processing_queue_status
            ON cf_processing_queue(status, priority DESC, created_at ASC)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create status index")?;

        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl JobStore for SqliteJobStore {
    async fn claim_next(&self, worker_id: &str) -> Result<Option<Job>> {
        // Use a single atomic UPDATE ... RETURNING to claim the job
        // This prevents race conditions between workers
        let row = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'RUNNING',
                worker_id = ?1,
                claim_time = datetime('now'),
                heartbeat_time = datetime('now')
            WHERE id = (
                SELECT id FROM cf_processing_queue
                WHERE status = 'QUEUED'
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
            )
            RETURNING id, file_version_id, plugin_name, status, retry_count, error_message
            "#,
        )
        .bind(worker_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to claim next job")?;

        match row {
            Some(row) => Ok(Some(Job {
                id: row.get("id"),
                file_version_id: row.get("file_version_id"),
                plugin_name: row.get("plugin_name"),
                status: row.get("status"),
                retry_count: row.get("retry_count"),
                error_message: row.get("error_message"),
            })),
            None => Ok(None),
        }
    }

    async fn heartbeat(&self, job_id: i64) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET heartbeat_time = datetime('now')
            WHERE id = ?1 AND status = 'RUNNING'
            "#,
        )
        .bind(job_id)
        .execute(&self.pool)
        .await
        .context("Failed to update heartbeat")?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Job {} not found or not in RUNNING status", job_id);
        }

        Ok(())
    }

    async fn complete(&self, job_id: i64, output_path: &str) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'COMPLETE',
                completed_at = datetime('now'),
                output_path = ?2
            WHERE id = ?1 AND status = 'RUNNING'
            "#,
        )
        .bind(job_id)
        .bind(output_path)
        .execute(&self.pool)
        .await
        .context("Failed to complete job")?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Job {} not found or not in RUNNING status", job_id);
        }

        Ok(())
    }

    async fn fail(&self, job_id: i64, error: &str, retry_eligible: bool) -> Result<()> {
        let new_status = if retry_eligible { "QUEUED" } else { "FAILED" };

        let result = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = ?2,
                error_message = ?3,
                retry_count = retry_count + 1,
                worker_id = NULL,
                claim_time = NULL,
                heartbeat_time = NULL
            WHERE id = ?1 AND status = 'RUNNING'
            "#,
        )
        .bind(job_id)
        .bind(new_status)
        .bind(error)
        .execute(&self.pool)
        .await
        .context("Failed to mark job as failed")?;

        if result.rows_affected() == 0 {
            anyhow::bail!("Job {} not found or not in RUNNING status", job_id);
        }

        Ok(())
    }

    async fn requeue_stale(&self, stale_threshold: Duration) -> Result<usize> {
        let threshold_seconds = stale_threshold.as_secs() as i64;

        let result = sqlx::query(
            r#"
            UPDATE cf_processing_queue
            SET status = 'QUEUED',
                worker_id = NULL,
                claim_time = NULL,
                heartbeat_time = NULL,
                retry_count = retry_count + 1
            WHERE status = 'RUNNING'
              AND heartbeat_time < datetime('now', ?1 || ' seconds')
            "#,
        )
        .bind(-threshold_seconds)
        .execute(&self.pool)
        .await
        .context("Failed to requeue stale jobs")?;

        Ok(result.rows_affected() as usize)
    }
}

/// SQLite-backed parser store implementation.
pub struct SqliteParserStore {
    pool: SqlitePool,
}

impl SqliteParserStore {
    /// Create a new SQLite parser store with the given database path.
    pub async fn new(db_path: &Path) -> Result<Self> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .context("Failed to connect to SQLite database")?;

        let store = Self { pool };
        store.initialize_tables().await?;
        Ok(store)
    }

    /// Create a new SQLite parser store from an existing pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize required tables if they don't exist.
    async fn initialize_tables(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_parsers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                archive BLOB NOT NULL,
                source_hash TEXT NOT NULL,
                lockfile_hash TEXT NOT NULL,
                lockfile_content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(name, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_parsers table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_parser_topics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parser_name TEXT NOT NULL,
                topic TEXT NOT NULL,
                UNIQUE(parser_name, topic)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_parser_topics table")?;

        // Create index for efficient topic lookups
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cf_parser_topics_parser
            ON cf_parser_topics(parser_name)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create parser topics index")?;

        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl ParserStore for SqliteParserStore {
    async fn get(&self, name: &str, version: &str) -> Result<Option<ParserBundle>> {
        let row = sqlx::query(
            r#"
            SELECT name, version, archive, source_hash, lockfile_hash, lockfile_content
            FROM cf_parsers
            WHERE name = ?1 AND version = ?2
            "#,
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch parser")?;

        match row {
            Some(row) => Ok(Some(ParserBundle {
                name: row.get("name"),
                version: row.get("version"),
                archive: row.get("archive"),
                source_hash: row.get("source_hash"),
                lockfile_hash: row.get("lockfile_hash"),
                lockfile_content: row.get("lockfile_content"),
            })),
            None => Ok(None),
        }
    }

    async fn insert(&self, bundle: ParserBundle) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cf_parsers (name, version, archive, source_hash, lockfile_hash, lockfile_content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&bundle.name)
        .bind(&bundle.version)
        .bind(&bundle.archive)
        .bind(&bundle.source_hash)
        .bind(&bundle.lockfile_hash)
        .bind(&bundle.lockfile_content)
        .execute(&self.pool)
        .await
        .context("Failed to insert parser (name and version may already exist)")?;

        Ok(())
    }

    async fn get_topics(&self, parser_name: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT topic FROM cf_parser_topics
            WHERE parser_name = ?1
            ORDER BY topic
            "#,
        )
        .bind(parser_name)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch parser topics")?;

        Ok(rows.iter().map(|r| r.get("topic")).collect())
    }
}

/// SQLite-backed quarantine store implementation.
pub struct SqliteQuarantineStore {
    pool: SqlitePool,
}

impl SqliteQuarantineStore {
    /// Create a new SQLite quarantine store with the given database path.
    pub async fn new(db_path: &Path) -> Result<Self> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .context("Failed to connect to SQLite database")?;

        let store = Self { pool };
        store.initialize_tables().await?;
        Ok(store)
    }

    /// Create a new SQLite quarantine store from an existing pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize required tables if they don't exist.
    async fn initialize_tables(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_quarantine (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                row_index INTEGER NOT NULL,
                error_reason TEXT NOT NULL,
                raw_data BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_quarantine table")?;

        // Create index for efficient job-based lookups
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cf_quarantine_job
            ON cf_quarantine(job_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create quarantine job index")?;

        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl QuarantineStore for SqliteQuarantineStore {
    async fn quarantine_row(
        &self,
        job_id: i64,
        row_idx: usize,
        error: &str,
        data: &[u8],
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cf_quarantine (job_id, row_index, error_reason, raw_data)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(job_id)
        .bind(row_idx as i64)
        .bind(error)
        .bind(data)
        .execute(&self.pool)
        .await
        .context("Failed to quarantine row")?;

        Ok(())
    }

    async fn get_quarantined(&self, job_id: i64) -> Result<Vec<QuarantinedRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, row_index, error_reason, raw_data
            FROM cf_quarantine
            WHERE job_id = ?1
            ORDER BY row_index
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch quarantined rows")?;

        Ok(rows
            .iter()
            .map(|r| QuarantinedRow {
                id: r.get("id"),
                job_id: r.get("job_id"),
                row_index: r.get::<i64, _>("row_index") as usize,
                error_reason: r.get("error_reason"),
                raw_data: r.get("raw_data"),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_job_store_claim_and_complete() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteJobStore::new(&db_path).await.unwrap();

        // Insert a test job
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
            VALUES (1, 'test_parser', 'QUEUED', 10)
            "#,
        )
        .execute(store.pool())
        .await
        .unwrap();

        // Claim the job
        let job = store.claim_next("worker-1").await.unwrap();
        assert!(job.is_some());
        let job = job.unwrap();
        assert_eq!(job.plugin_name, "test_parser");
        assert_eq!(job.status, "RUNNING");

        // No more jobs to claim
        let job2 = store.claim_next("worker-2").await.unwrap();
        assert!(job2.is_none());

        // Complete the job
        store.complete(job.id, "/output/result.parquet").await.unwrap();

        // Verify completion
        let row = sqlx::query("SELECT status, output_path FROM cf_processing_queue WHERE id = ?1")
            .bind(job.id)
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "COMPLETE");
        assert_eq!(row.get::<String, _>("output_path"), "/output/result.parquet");
    }

    #[tokio::test]
    async fn test_job_store_fail_with_retry() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteJobStore::new(&db_path).await.unwrap();

        // Insert and claim a job
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status)
            VALUES (1, 'test_parser', 'QUEUED')
            "#,
        )
        .execute(store.pool())
        .await
        .unwrap();

        let job = store.claim_next("worker-1").await.unwrap().unwrap();

        // Fail with retry
        store
            .fail(job.id, "Connection timeout", true)
            .await
            .unwrap();

        // Job should be back in QUEUED status
        let row = sqlx::query("SELECT status, retry_count FROM cf_processing_queue WHERE id = ?1")
            .bind(job.id)
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "QUEUED");
        assert_eq!(row.get::<i32, _>("retry_count"), 1);

        // Can claim again
        let job2 = store.claim_next("worker-2").await.unwrap();
        assert!(job2.is_some());
    }

    #[tokio::test]
    async fn test_job_store_fail_without_retry() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteJobStore::new(&db_path).await.unwrap();

        // Insert and claim a job
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status)
            VALUES (1, 'test_parser', 'QUEUED')
            "#,
        )
        .execute(store.pool())
        .await
        .unwrap();

        let job = store.claim_next("worker-1").await.unwrap().unwrap();

        // Fail without retry
        store
            .fail(job.id, "Fatal error", false)
            .await
            .unwrap();

        // Job should be in FAILED status
        let row = sqlx::query("SELECT status FROM cf_processing_queue WHERE id = ?1")
            .bind(job.id)
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("status"), "FAILED");
    }

    #[tokio::test]
    async fn test_job_store_heartbeat() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteJobStore::new(&db_path).await.unwrap();

        // Insert and claim a job
        sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_version_id, plugin_name, status)
            VALUES (1, 'test_parser', 'QUEUED')
            "#,
        )
        .execute(store.pool())
        .await
        .unwrap();

        let job = store.claim_next("worker-1").await.unwrap().unwrap();

        // Heartbeat should succeed
        store.heartbeat(job.id).await.unwrap();

        // Heartbeat on non-existent job should fail
        let result = store.heartbeat(999).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parser_store() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteParserStore::new(&db_path).await.unwrap();

        let bundle = ParserBundle {
            name: "my_parser".to_string(),
            version: "1.0.0".to_string(),
            archive: vec![1, 2, 3, 4],
            source_hash: "abc123".to_string(),
            lockfile_hash: "def456".to_string(),
            lockfile_content: "lockfile contents".to_string(),
        };

        // Insert
        store.insert(bundle.clone()).await.unwrap();

        // Get
        let retrieved = store.get("my_parser", "1.0.0").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "my_parser");
        assert_eq!(retrieved.version, "1.0.0");
        assert_eq!(retrieved.archive, vec![1, 2, 3, 4]);

        // Get non-existent
        let missing = store.get("my_parser", "2.0.0").await.unwrap();
        assert!(missing.is_none());

        // Duplicate insert should fail
        let result = store.insert(bundle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parser_topics() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteParserStore::new(&db_path).await.unwrap();

        // Insert some topics
        sqlx::query("INSERT INTO cf_parser_topics (parser_name, topic) VALUES ('my_parser', 'sales')")
            .execute(store.pool())
            .await
            .unwrap();
        sqlx::query("INSERT INTO cf_parser_topics (parser_name, topic) VALUES ('my_parser', 'inventory')")
            .execute(store.pool())
            .await
            .unwrap();

        let topics = store.get_topics("my_parser").await.unwrap();
        assert_eq!(topics, vec!["inventory", "sales"]); // Sorted alphabetically

        let empty_topics = store.get_topics("other_parser").await.unwrap();
        assert!(empty_topics.is_empty());
    }

    #[tokio::test]
    async fn test_quarantine_store() {
        let tmp_dir = tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let store = SqliteQuarantineStore::new(&db_path).await.unwrap();

        // Quarantine some rows
        store
            .quarantine_row(1, 5, "Invalid date format", b"bad,row,data")
            .await
            .unwrap();
        store
            .quarantine_row(1, 10, "Missing required field", b"incomplete,row")
            .await
            .unwrap();
        store
            .quarantine_row(2, 1, "Other job error", b"other")
            .await
            .unwrap();

        // Get quarantined rows for job 1
        let rows = store.get_quarantined(1).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index, 5);
        assert_eq!(rows[0].error_reason, "Invalid date format");
        assert_eq!(rows[1].row_index, 10);

        // Get quarantined rows for job 2
        let rows = store.get_quarantined(2).await.unwrap();
        assert_eq!(rows.len(), 1);

        // Get quarantined rows for non-existent job
        let rows = store.get_quarantined(999).await.unwrap();
        assert!(rows.is_empty());
    }
}

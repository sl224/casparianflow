//! SQLite implementations of the storage traits.
//!
//! This module provides SQLite-backed implementations of JobStore,
//! ParserStore, and QuarantineStore using sqlx for async database access.

use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::QueryBuilder;
use sqlx::Row;
use std::path::Path;
use std::time::Duration;

use super::traits::{
    Job, JobStore, ParserBundle, ParserStore, PipelineStore, QuarantinedRow, QuarantineStore,
    SelectionFilters, SelectionResolution, WatermarkField,
};
use uuid::Uuid;

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
                file_id INTEGER NOT NULL,
                pipeline_run_id TEXT,
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

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_selection_specs (
                id TEXT PRIMARY KEY,
                spec_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_selection_specs table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_selection_snapshots (
                id TEXT PRIMARY KEY,
                spec_id TEXT NOT NULL,
                snapshot_hash TEXT NOT NULL,
                logical_date TEXT NOT NULL,
                watermark_value TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_selection_snapshots table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_selection_snapshot_files (
                snapshot_id TEXT NOT NULL,
                file_id INTEGER NOT NULL,
                PRIMARY KEY (snapshot_id, file_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_selection_snapshot_files table")?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_snapshot_files_snapshot
            ON cf_selection_snapshot_files(snapshot_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create snapshot_id index")?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_snapshot_files_file
            ON cf_selection_snapshot_files(file_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create file_id index")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_pipelines (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                config_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_pipelines table")?;

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_pipelines_name_version
            ON cf_pipelines(name, version)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create pipeline name/version index")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cf_pipeline_runs (
                id TEXT PRIMARY KEY,
                pipeline_id TEXT NOT NULL,
                selection_spec_id TEXT NOT NULL,
                selection_snapshot_hash TEXT NOT NULL,
                context_snapshot_hash TEXT,
                logical_date TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create cf_pipeline_runs table")?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_pipeline_runs_pipeline
            ON cf_pipeline_runs(pipeline_id, logical_date)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create pipeline runs index")?;

        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl PipelineStore for SqliteJobStore {
    async fn create_selection_spec(&self, spec_json: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO cf_selection_specs (id, spec_json)
            VALUES (?1, ?2)
            "#,
        )
        .bind(&id)
        .bind(spec_json)
        .execute(&self.pool)
        .await
        .context("Failed to insert selection spec")?;
        Ok(id)
    }

    async fn create_selection_snapshot(
        &self,
        spec_id: &str,
        snapshot_hash: &str,
        logical_date: &str,
        watermark_value: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO cf_selection_snapshots (
                id, spec_id, snapshot_hash, logical_date, watermark_value
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&id)
        .bind(spec_id)
        .bind(snapshot_hash)
        .bind(logical_date)
        .bind(watermark_value)
        .execute(&self.pool)
        .await
        .context("Failed to insert selection snapshot")?;
        Ok(id)
    }

    async fn insert_snapshot_files(&self, snapshot_id: &str, file_ids: &[i64]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for file_id in file_ids {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO cf_selection_snapshot_files (snapshot_id, file_id)
                VALUES (?1, ?2)
                "#,
            )
            .bind(snapshot_id)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn create_pipeline(&self, name: &str, version: i64, config_json: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO cf_pipelines (id, name, version, config_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(version)
        .bind(config_json)
        .execute(&self.pool)
        .await
        .context("Failed to insert pipeline")?;
        Ok(id)
    }

    async fn get_latest_pipeline(&self, name: &str) -> Result<Option<super::traits::Pipeline>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, version, config_json, created_at
            FROM cf_pipelines
            WHERE name = ?1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch latest pipeline")?;

        Ok(row.map(|row| super::traits::Pipeline {
            id: row.get("id"),
            name: row.get("name"),
            version: row.get("version"),
            config_json: row.get("config_json"),
            created_at: row.get("created_at"),
        }))
    }

    async fn create_pipeline_run(
        &self,
        pipeline_id: &str,
        selection_spec_id: &str,
        selection_snapshot_hash: &str,
        context_snapshot_hash: Option<&str>,
        logical_date: &str,
        status: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO cf_pipeline_runs (
                id,
                pipeline_id,
                selection_spec_id,
                selection_snapshot_hash,
                context_snapshot_hash,
                logical_date,
                status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&id)
        .bind(pipeline_id)
        .bind(selection_spec_id)
        .bind(selection_snapshot_hash)
        .bind(context_snapshot_hash)
        .bind(logical_date)
        .bind(status)
        .execute(&self.pool)
        .await
        .context("Failed to insert pipeline run")?;
        Ok(id)
    }

    async fn set_pipeline_run_status(&self, run_id: &str, status: &str) -> Result<()> {
        let (set_started, set_completed) = match status {
            "running" => (true, false),
            "completed" | "failed" | "no_op" => (false, true),
            _ => (false, false),
        };

        let mut query = String::from("UPDATE cf_pipeline_runs SET status = ?1");
        if set_started {
            query.push_str(", started_at = datetime('now')");
        }
        if set_completed {
            query.push_str(", completed_at = datetime('now')");
        }
        query.push_str(" WHERE id = ?2");

        sqlx::query(&query)
            .bind(status)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .context("Failed to update pipeline run status")?;
        Ok(())
    }

    async fn pipeline_run_exists(&self, pipeline_id: &str, logical_date: &str) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT 1 FROM cf_pipeline_runs
            WHERE pipeline_id = ?1 AND logical_date = ?2
            LIMIT 1
            "#,
        )
        .bind(pipeline_id)
        .bind(logical_date)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to query pipeline runs")?;

        Ok(row.is_some())
    }

    async fn resolve_selection_files(
        &self,
        filters: &SelectionFilters,
        logical_date_ms: i64,
    ) -> Result<SelectionResolution> {
        let mut builder = QueryBuilder::new(
            "SELECT id, mtime FROM scout_files WHERE status != 'deleted'",
        );

        if let Some(source_id) = &filters.source_id {
            builder.push(" AND source_id = ");
            builder.push_bind(source_id);
        }
        if let Some(tag) = &filters.tag {
            builder.push(" AND tag = ");
            builder.push_bind(tag);
        }
        if let Some(extension) = &filters.extension {
            builder.push(" AND extension = ");
            builder.push_bind(extension);
        }
        if let Some(since_ms) = filters.since_ms {
            let start_ms = logical_date_ms.saturating_sub(since_ms);
            builder.push(" AND mtime >= ");
            builder.push_bind(start_ms);
            builder.push(" AND mtime <= ");
            builder.push_bind(logical_date_ms);
        }

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .context("Failed to resolve selection files")?;

        let mut file_ids = Vec::with_capacity(rows.len());
        let mut watermark_max: Option<i64> = None;
        for row in rows {
            let id: i64 = row.get("id");
            let mtime: i64 = row.get("mtime");
            file_ids.push(id);
            if matches!(filters.watermark, Some(WatermarkField::Mtime)) {
                watermark_max = Some(match watermark_max {
                    Some(current) => current.max(mtime),
                    None => mtime,
                });
            }
        }

        let watermark_value = watermark_max.map(|value| value.to_string());
        Ok(SelectionResolution {
            file_ids,
            watermark_value,
        })
    }
}

#[async_trait]
impl JobStore for SqliteJobStore {
    async fn enqueue_job(&self, file_id: i64, plugin_name: &str, priority: i32) -> Result<i64> {
        let row = sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (file_id, plugin_name, status, priority)
            VALUES (?1, ?2, 'QUEUED', ?3)
            RETURNING id
            "#,
        )
        .bind(file_id)
        .bind(plugin_name)
        .bind(priority)
        .fetch_one(&self.pool)
        .await
        .context("Failed to enqueue job")?;

        Ok(row.get::<i64, _>("id"))
    }

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
            RETURNING id, file_id, plugin_name, status, retry_count, error_message
            "#,
        )
        .bind(worker_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to claim next job")?;

        match row {
            Some(row) => Ok(Some(Job {
                id: row.get("id"),
                file_id: row.get("file_id"),
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
            INSERT INTO cf_processing_queue (file_id, plugin_name, status, priority)
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
            INSERT INTO cf_processing_queue (file_id, plugin_name, status)
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
            INSERT INTO cf_processing_queue (file_id, plugin_name, status)
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
            INSERT INTO cf_processing_queue (file_id, plugin_name, status)
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

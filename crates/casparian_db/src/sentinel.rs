//! Sentinel database operations (job processing)

use crate::error::{DbError, Result};
use crate::types::*;
use crate::CasparianDb;
use sqlx::Row;

impl CasparianDb {
    // ========================================================================
    // Job Queue Operations
    // ========================================================================

    /// Create a new job in the queue
    pub async fn sentinel_create_job(
        &self,
        plugin_name: &str,
        input_file: Option<&str>,
        priority: i32,
    ) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO cf_processing_queue (plugin_name, input_file, status, priority)
            VALUES (?, ?, 'QUEUED', ?)
            "#,
        )
        .bind(plugin_name)
        .bind(input_file)
        .bind(priority)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Pop the next queued job (atomic claim)
    pub async fn sentinel_pop_job(&self) -> Result<Option<Job>> {
        // Start transaction for atomic claim
        let mut tx = self.pool.begin().await?;

        // Find next queued job
        let row = sqlx::query(
            r#"
            SELECT id FROM cf_processing_queue
            WHERE status = 'QUEUED'
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let job_id = match row {
            Some(row) => row.get::<i64, _>("id"),
            None => {
                tx.rollback().await?;
                return Ok(None);
            }
        };

        // Claim the job
        sqlx::query(
            "UPDATE cf_processing_queue SET status = 'RUNNING', claim_time = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Fetch the claimed job
        self.sentinel_get_job(job_id).await
    }

    /// Get a job by ID
    pub async fn sentinel_get_job(&self, id: i64) -> Result<Option<Job>> {
        let row = sqlx::query("SELECT * FROM cf_processing_queue WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_job(&row)?)),
            None => Ok(None),
        }
    }

    /// List jobs with optional filter
    pub async fn sentinel_list_jobs(&self, filter: JobFilter) -> Result<Vec<Job>> {
        let mut sql = String::from("SELECT * FROM cf_processing_queue WHERE 1=1");

        if let Some(ref status) = filter.status {
            sql.push_str(&format!(" AND status = '{}'", status.as_str()));
        }
        if let Some(ref plugin) = filter.plugin_name {
            sql.push_str(&format!(" AND plugin_name = '{}'", plugin));
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        rows.iter().map(|row| self.row_to_job(row)).collect()
    }

    /// Complete a job successfully
    pub async fn sentinel_complete_job(&self, id: i64, summary: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE cf_processing_queue SET
                status = 'COMPLETED',
                end_time = CURRENT_TIMESTAMP,
                completed_at = CURRENT_TIMESTAMP,
                result_summary = ?
            WHERE id = ?
            "#,
        )
        .bind(summary)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fail a job
    pub async fn sentinel_fail_job(&self, id: i64, error: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE cf_processing_queue SET
                status = 'FAILED',
                end_time = CURRENT_TIMESTAMP,
                error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Requeue a failed job (with retry limit)
    pub async fn sentinel_requeue_job(&self, id: i64) -> Result<bool> {
        const MAX_RETRIES: i32 = 5;

        let result = sqlx::query(
            r#"
            UPDATE cf_processing_queue SET
                status = 'QUEUED',
                claim_time = NULL,
                end_time = NULL,
                error_message = NULL,
                retry_count = retry_count + 1
            WHERE id = ? AND retry_count < ?
            "#,
        )
        .bind(id)
        .bind(MAX_RETRIES)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get job queue statistics
    pub async fn sentinel_get_queue_stats(&self) -> Result<QueueStats> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total,
                SUM(CASE WHEN status = 'QUEUED' THEN 1 ELSE 0 END) as queued,
                SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) as running,
                SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as completed,
                SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) as failed
            FROM cf_processing_queue
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(QueueStats {
            total: row.get::<i64, _>("total") as u64,
            queued: row.get::<i64, _>("queued") as u64,
            running: row.get::<i64, _>("running") as u64,
            completed: row.get::<i64, _>("completed") as u64,
            failed: row.get::<i64, _>("failed") as u64,
        })
    }

    fn row_to_job(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Job> {
        let status_str: String = row.get("status");
        let status = JobStatus::parse(&status_str)
            .ok_or_else(|| DbError::invalid_state(format!("Unknown job status: {}", status_str)))?;

        Ok(Job {
            id: row.get("id"),
            file_version_id: row.get("file_version_id"),
            plugin_name: row.get("plugin_name"),
            input_file: row.get("input_file"),
            status,
            priority: row.get("priority"),
            config_overrides: row.get("config_overrides"),
            created_at: row.get("created_at"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            claim_time: row.get("claim_time"),
            end_time: row.get("end_time"),
            result_summary: row.get("result_summary"),
            error_message: row.get("error_message"),
            retry_count: row.get("retry_count"),
            logs: row.get("logs"),
        })
    }

    // ========================================================================
    // Plugin Operations
    // ========================================================================

    /// Get a plugin by name (latest active version)
    pub async fn sentinel_get_plugin(&self, name: &str) -> Result<Option<PluginManifest>> {
        let row = sqlx::query(
            r#"
            SELECT * FROM cf_plugin_manifest
            WHERE plugin_name = ? AND status IN ('ACTIVE', 'DEPLOYED')
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_plugin_manifest(&row)?)),
            None => Ok(None),
        }
    }

    /// List all plugins
    pub async fn sentinel_list_plugins(&self) -> Result<Vec<PluginManifest>> {
        let rows = sqlx::query(
            "SELECT * FROM cf_plugin_manifest WHERE status IN ('ACTIVE', 'DEPLOYED') ORDER BY plugin_name, created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| self.row_to_plugin_manifest(row))
            .collect()
    }

    /// Deploy a plugin
    pub async fn sentinel_deploy_plugin(
        &self,
        name: &str,
        version: &str,
        source_code: &str,
        source_hash: &str,
        env_hash: Option<&str>,
    ) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO cf_plugin_manifest (plugin_name, version, source_code, source_hash, env_hash, status, deployed_at)
            VALUES (?, ?, ?, ?, ?, 'ACTIVE', CURRENT_TIMESTAMP)
            ON CONFLICT(plugin_name, version) DO UPDATE SET
                source_code = excluded.source_code,
                source_hash = excluded.source_hash,
                env_hash = excluded.env_hash,
                status = 'ACTIVE',
                deployed_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(name)
        .bind(version)
        .bind(source_code)
        .bind(source_hash)
        .bind(env_hash)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get plugin config (subscription tags)
    pub async fn sentinel_get_plugin_config(&self, name: &str) -> Result<Option<PluginConfig>> {
        let row = sqlx::query("SELECT * FROM cf_plugin_config WHERE plugin_name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_plugin_config(&row)?)),
            None => Ok(None),
        }
    }

    /// Set plugin subscription tags
    pub async fn sentinel_set_plugin_tags(&self, name: &str, tags: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cf_plugin_config (plugin_name, subscription_tags, enabled)
            VALUES (?, ?, 1)
            ON CONFLICT(plugin_name) DO UPDATE SET
                subscription_tags = excluded.subscription_tags
            "#,
        )
        .bind(name)
        .bind(tags)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find plugin for a tag
    pub async fn sentinel_find_plugin_for_tag(&self, tag: &str) -> Result<Option<String>> {
        // subscription_tags is comma-separated
        let rows = sqlx::query("SELECT plugin_name, subscription_tags FROM cf_plugin_config WHERE enabled = 1")
            .fetch_all(&self.pool)
            .await?;

        for row in rows {
            let plugin_name: String = row.get("plugin_name");
            let tags: String = row.get("subscription_tags");

            for t in tags.split(',') {
                if t.trim() == tag {
                    return Ok(Some(plugin_name));
                }
            }
        }

        Ok(None)
    }

    fn row_to_plugin_manifest(&self, row: &sqlx::sqlite::SqliteRow) -> Result<PluginManifest> {
        Ok(PluginManifest {
            id: row.get("id"),
            plugin_name: row.get("plugin_name"),
            version: row.get("version"),
            source_code: row.get("source_code"),
            source_hash: row.get("source_hash"),
            env_hash: row.get("env_hash"),
            status: row.get("status"),
            created_at: row.get("created_at"),
            deployed_at: row.get("deployed_at"),
        })
    }

    fn row_to_plugin_config(&self, row: &sqlx::sqlite::SqliteRow) -> Result<PluginConfig> {
        Ok(PluginConfig {
            id: row.get("id"),
            plugin_name: row.get("plugin_name"),
            subscription_tags: row.get("subscription_tags"),
            default_parameters: row.get("default_parameters"),
            enabled: row.get("enabled"),
        })
    }
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub total: u64,
    pub queued: u64,
    pub running: u64,
    pub completed: u64,
    pub failed: u64,
}

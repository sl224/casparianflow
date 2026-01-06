//! Scout database operations (file discovery & tagging)

use crate::error::{DbError, Result};
use crate::types::*;
use crate::CasparianDb;
use sqlx::Row;

impl CasparianDb {
    // ========================================================================
    // Source Operations
    // ========================================================================

    /// Insert or update a source
    pub async fn scout_upsert_source(&self, source: &Source) -> Result<()> {
        let source_type_json = serde_json::to_string(&source.source_type)?;
        let now = Self::now_millis();

        sqlx::query(
            r#"
            INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_type = excluded.source_type,
                path = excluded.path,
                poll_interval_secs = excluded.poll_interval_secs,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&source.id)
        .bind(&source.name)
        .bind(&source_type_json)
        .bind(&source.path)
        .bind(source.poll_interval_secs as i64)
        .bind(source.enabled)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a source by ID
    pub async fn scout_get_source(&self, id: &str) -> Result<Option<Source>> {
        let row = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_source(&row)?)),
            None => Ok(None),
        }
    }

    /// List all sources
    pub async fn scout_list_sources(&self) -> Result<Vec<Source>> {
        let rows = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(|row| self.row_to_source(row)).collect()
    }

    /// List enabled sources
    pub async fn scout_list_enabled_sources(&self) -> Result<Vec<Source>> {
        let rows = sqlx::query(
            "SELECT id, name, source_type, path, poll_interval_secs, enabled FROM scout_sources WHERE enabled = 1 ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(|row| self.row_to_source(row)).collect()
    }

    /// Delete a source and all its files
    pub async fn scout_delete_source(&self, id: &str) -> Result<()> {
        // Delete files first (cascade)
        sqlx::query("DELETE FROM scout_files WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Delete tagging rules
        sqlx::query("DELETE FROM scout_tagging_rules WHERE source_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Delete source
        sqlx::query("DELETE FROM scout_sources WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    fn row_to_source(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Source> {
        let source_type_json: String = row.get("source_type");
        let source_type: SourceType = serde_json::from_str(&source_type_json)?;

        Ok(Source {
            id: row.get("id"),
            name: row.get("name"),
            source_type,
            path: row.get("path"),
            poll_interval_secs: row.get::<i64, _>("poll_interval_secs") as u64,
            enabled: row.get("enabled"),
        })
    }

    // ========================================================================
    // Tagging Rule Operations
    // ========================================================================

    /// Insert or update a tagging rule
    pub async fn scout_upsert_tagging_rule(&self, rule: &TaggingRule) -> Result<()> {
        let now = Self::now_millis();

        sqlx::query(
            r#"
            INSERT INTO scout_tagging_rules (id, name, source_id, pattern, tag, priority, enabled, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_id = excluded.source_id,
                pattern = excluded.pattern,
                tag = excluded.tag,
                priority = excluded.priority,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.name)
        .bind(&rule.source_id)
        .bind(&rule.pattern)
        .bind(&rule.tag)
        .bind(rule.priority)
        .bind(rule.enabled)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a tagging rule by ID
    pub async fn scout_get_tagging_rule(&self, id: &str) -> Result<Option<TaggingRule>> {
        let row = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_tagging_rule(&row)?)),
            None => Ok(None),
        }
    }

    /// List all tagging rules
    pub async fn scout_list_tagging_rules(&self) -> Result<Vec<TaggingRule>> {
        let rows = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules ORDER BY priority DESC, name",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(|row| self.row_to_tagging_rule(row)).collect()
    }

    /// List tagging rules for a source
    pub async fn scout_list_tagging_rules_for_source(&self, source_id: &str) -> Result<Vec<TaggingRule>> {
        let rows = sqlx::query(
            "SELECT id, name, source_id, pattern, tag, priority, enabled FROM scout_tagging_rules WHERE source_id = ? ORDER BY priority DESC, name",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(|row| self.row_to_tagging_rule(row)).collect()
    }

    /// Delete a tagging rule
    pub async fn scout_delete_tagging_rule(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM scout_tagging_rules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    fn row_to_tagging_rule(&self, row: &sqlx::sqlite::SqliteRow) -> Result<TaggingRule> {
        Ok(TaggingRule {
            id: row.get("id"),
            name: row.get("name"),
            source_id: row.get("source_id"),
            pattern: row.get("pattern"),
            tag: row.get("tag"),
            priority: row.get("priority"),
            enabled: row.get("enabled"),
        })
    }

    // ========================================================================
    // File Operations
    // ========================================================================

    /// Insert or update a file
    pub async fn scout_upsert_file(&self, file: &ScannedFile) -> Result<UpsertResult> {
        let now = Self::now_millis();

        // Check if file exists and if it changed
        let existing = sqlx::query(
            "SELECT id, mtime, size FROM scout_files WHERE source_id = ? AND path = ?",
        )
        .bind(&file.source_id)
        .bind(&file.path)
        .fetch_optional(&self.pool)
        .await?;

        match existing {
            Some(row) => {
                let id: i64 = row.get("id");
                let old_mtime: i64 = row.get("mtime");
                let old_size: i64 = row.get("size");
                let is_changed = old_mtime != file.mtime || old_size != file.size as i64;

                // Update file - reset status if changed
                let new_status = if is_changed { "pending" } else { file.status.as_str() };

                sqlx::query(
                    r#"
                    UPDATE scout_files SET
                        rel_path = ?,
                        size = ?,
                        mtime = ?,
                        content_hash = ?,
                        status = ?,
                        last_seen_at = ?
                    WHERE id = ?
                    "#,
                )
                .bind(&file.rel_path)
                .bind(file.size as i64)
                .bind(file.mtime)
                .bind(&file.content_hash)
                .bind(new_status)
                .bind(now)
                .bind(id)
                .execute(&self.pool)
                .await?;

                Ok(UpsertResult {
                    id,
                    is_new: false,
                    is_changed,
                })
            }
            None => {
                // Insert new file
                let result = sqlx::query(
                    r#"
                    INSERT INTO scout_files (
                        source_id, path, rel_path, size, mtime, content_hash,
                        status, tag, tag_source, rule_id, manual_plugin, error,
                        first_seen_at, last_seen_at, processed_at, sentinel_job_id
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&file.source_id)
                .bind(&file.path)
                .bind(&file.rel_path)
                .bind(file.size as i64)
                .bind(file.mtime)
                .bind(&file.content_hash)
                .bind(file.status.as_str())
                .bind(&file.tag)
                .bind(&file.tag_source)
                .bind(&file.rule_id)
                .bind(&file.manual_plugin)
                .bind(&file.error)
                .bind(now)
                .bind(now)
                .bind(file.processed_at.map(|dt| dt.timestamp_millis()))
                .bind(file.sentinel_job_id)
                .execute(&self.pool)
                .await?;

                Ok(UpsertResult {
                    id: result.last_insert_rowid(),
                    is_new: true,
                    is_changed: false,
                })
            }
        }
    }

    /// Get a file by ID
    pub async fn scout_get_file(&self, id: i64) -> Result<Option<ScannedFile>> {
        let row = sqlx::query("SELECT * FROM scout_files WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_file(&row)?)),
            None => Ok(None),
        }
    }

    /// List files with optional filters
    pub async fn scout_list_files(&self, filter: FileFilter) -> Result<Vec<ScannedFile>> {
        let mut sql = String::from("SELECT * FROM scout_files WHERE 1=1");

        if let Some(ref source_id) = filter.source_id {
            sql.push_str(&format!(" AND source_id = '{}'", source_id));
        }
        if let Some(ref status) = filter.status {
            sql.push_str(&format!(" AND status = '{}'", status.as_str()));
        }
        if let Some(ref tag) = filter.tag {
            sql.push_str(&format!(" AND tag = '{}'", tag));
        }
        if filter.untagged_only {
            sql.push_str(" AND tag IS NULL");
        }

        sql.push_str(" ORDER BY last_seen_at DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        rows.iter().map(|row| self.row_to_file(row)).collect()
    }

    /// List files by source
    pub async fn scout_list_files_by_source(&self, source_id: &str) -> Result<Vec<ScannedFile>> {
        self.scout_list_files(FileFilter {
            source_id: Some(source_id.to_string()),
            ..Default::default()
        })
        .await
    }

    /// List files by status
    pub async fn scout_list_files_by_status(&self, status: FileStatus) -> Result<Vec<ScannedFile>> {
        self.scout_list_files(FileFilter {
            status: Some(status),
            ..Default::default()
        })
        .await
    }

    /// List files by tag
    pub async fn scout_list_files_by_tag(&self, tag: &str) -> Result<Vec<ScannedFile>> {
        self.scout_list_files(FileFilter {
            tag: Some(tag.to_string()),
            ..Default::default()
        })
        .await
    }

    /// Update file status
    pub async fn scout_update_file_status(
        &self,
        id: i64,
        status: FileStatus,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_millis();
        let processed_at = if status == FileStatus::Processed {
            Some(now)
        } else {
            None
        };

        sqlx::query(
            "UPDATE scout_files SET status = ?, error = ?, processed_at = ? WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(error)
        .bind(processed_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Tag a file manually
    pub async fn scout_tag_file(&self, id: i64, tag: &str) -> Result<()> {
        sqlx::query(
            "UPDATE scout_files SET tag = ?, tag_source = 'manual', status = 'tagged' WHERE id = ?",
        )
        .bind(tag)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Tag a file by rule
    pub async fn scout_tag_file_by_rule(&self, id: i64, tag: &str, rule_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE scout_files SET tag = ?, tag_source = 'rule', rule_id = ?, status = 'tagged' WHERE id = ? AND tag_source IS NULL",
        )
        .bind(tag)
        .bind(rule_id)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get file statistics
    pub async fn scout_get_stats(&self) -> Result<ScoutStats> {
        let row = sqlx::query(
            r#"
            SELECT
                (SELECT COUNT(*) FROM scout_sources) as total_sources,
                (SELECT COUNT(*) FROM scout_tagging_rules) as total_tagging_rules,
                COUNT(*) as total_files,
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as files_pending,
                SUM(CASE WHEN status = 'tagged' THEN 1 ELSE 0 END) as files_tagged,
                SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END) as files_queued,
                SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) as files_processing,
                SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END) as files_processed,
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as files_failed,
                COALESCE(SUM(CASE WHEN status = 'pending' THEN size ELSE 0 END), 0) as bytes_pending,
                COALESCE(SUM(CASE WHEN status = 'processed' THEN size ELSE 0 END), 0) as bytes_processed
            FROM scout_files
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(ScoutStats {
            total_sources: row.get::<i64, _>("total_sources") as u64,
            total_tagging_rules: row.get::<i64, _>("total_tagging_rules") as u64,
            total_files: row.get::<i64, _>("total_files") as u64,
            files_pending: row.get::<i64, _>("files_pending") as u64,
            files_tagged: row.get::<i64, _>("files_tagged") as u64,
            files_queued: row.get::<i64, _>("files_queued") as u64,
            files_processing: row.get::<i64, _>("files_processing") as u64,
            files_processed: row.get::<i64, _>("files_processed") as u64,
            files_failed: row.get::<i64, _>("files_failed") as u64,
            bytes_pending: row.get::<i64, _>("bytes_pending") as u64,
            bytes_processed: row.get::<i64, _>("bytes_processed") as u64,
        })
    }

    fn row_to_file(&self, row: &sqlx::sqlite::SqliteRow) -> Result<ScannedFile> {
        let status_str: String = row.get("status");
        let status = FileStatus::parse(&status_str)
            .ok_or_else(|| DbError::invalid_state(format!("Unknown file status: {}", status_str)))?;

        let first_seen_at: i64 = row.get("first_seen_at");
        let last_seen_at: i64 = row.get("last_seen_at");
        let processed_at: Option<i64> = row.get("processed_at");

        Ok(ScannedFile {
            id: Some(row.get("id")),
            source_id: row.get("source_id"),
            path: row.get("path"),
            rel_path: row.get("rel_path"),
            size: row.get::<i64, _>("size") as u64,
            mtime: row.get("mtime"),
            content_hash: row.get("content_hash"),
            status,
            tag: row.get("tag"),
            tag_source: row.get("tag_source"),
            rule_id: row.get("rule_id"),
            manual_plugin: row.get("manual_plugin"),
            error: row.get("error"),
            first_seen_at: Self::millis_to_datetime(first_seen_at),
            last_seen_at: Self::millis_to_datetime(last_seen_at),
            processed_at: processed_at.map(Self::millis_to_datetime),
            sentinel_job_id: row.get("sentinel_job_id"),
        })
    }
}

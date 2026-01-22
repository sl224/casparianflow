//! DuckDB-backed pipeline storage.
//!
//! Uses DbConnection to keep the actor boundary intact and reuse DuckDB
//! query paths already in casparian_db.

use anyhow::{Context, Result};
use casparian_protocol::{JobId, PipelineRunStatus};
use crate::scout::FileStatus;
use std::path::Path;

use casparian_db::{DbConnection, DbValue};

use super::types::{
    Pipeline, PipelineRun, SelectionFilters, SelectionResolution, SelectionSnapshot, WatermarkField,
};
use uuid::Uuid;

pub struct DuckDbPipelineStore {
    conn: DbConnection,
}

impl DuckDbPipelineStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let db_url = format!("duckdb:{}", db_path.display());
        let conn = DbConnection::open_from_url(&db_url)
            .context("Failed to connect to DuckDB")?;
        let store = Self { conn };
        store.initialize_tables()?;
        Ok(store)
    }

    pub fn from_connection(conn: DbConnection) -> Self {
        Self { conn }
    }

    pub fn connection(&self) -> DbConnection {
        self.conn.clone()
    }

    pub fn get_pipeline_run(&self, run_id: &str) -> Result<Option<PipelineRun>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT
                    id,
                    pipeline_id,
                    selection_spec_id,
                    selection_snapshot_hash,
                    context_snapshot_hash,
                    logical_date,
                    status,
                    started_at,
                    completed_at
                FROM cf_pipeline_runs
                WHERE id = ?
                "#,
                &[DbValue::from(run_id)],
            )
            
            .context("Failed to fetch pipeline run")?;

        let run = match row {
            Some(row) => {
                let status_raw: String = row.get_by_name("status").unwrap_or_default();
                let status = status_raw
                    .parse()
                    .map_err(|err| anyhow::anyhow!("Invalid pipeline run status '{}': {}", status_raw, err))?;
                Some(PipelineRun {
                    id: row.get_by_name("id").unwrap_or_default(),
                    pipeline_id: row.get_by_name("pipeline_id").unwrap_or_default(),
                    selection_spec_id: row.get_by_name("selection_spec_id").unwrap_or_default(),
                    selection_snapshot_hash: row.get_by_name("selection_snapshot_hash").unwrap_or_default(),
                    context_snapshot_hash: row.get_by_name("context_snapshot_hash").ok().flatten(),
                    logical_date: row.get_by_name("logical_date").unwrap_or_default(),
                    status,
                    started_at: row.get_by_name("started_at").ok().flatten(),
                    completed_at: row.get_by_name("completed_at").ok().flatten(),
                })
            }
            None => None,
        };

        Ok(run)
    }

    pub fn get_pipeline_run_for_job(&self, job_id: JobId) -> Result<Option<PipelineRun>> {
        let job_id = job_id.to_i64().map_err(|err| anyhow::anyhow!(err))?;
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT pr.*
                FROM cf_processing_queue q
                JOIN cf_pipeline_runs pr ON pr.id = q.pipeline_run_id
                WHERE q.id = ?
                "#,
                &[DbValue::from(job_id)],
            )
            
            .context("Failed to fetch pipeline run for job")?;

        let run = match row {
            Some(row) => {
                let status_raw: String = row.get_by_name("status").unwrap_or_default();
                let status = status_raw
                    .parse()
                    .map_err(|err| anyhow::anyhow!("Invalid pipeline run status '{}': {}", status_raw, err))?;
                Some(PipelineRun {
                    id: row.get_by_name("id").unwrap_or_default(),
                    pipeline_id: row.get_by_name("pipeline_id").unwrap_or_default(),
                    selection_spec_id: row.get_by_name("selection_spec_id").unwrap_or_default(),
                    selection_snapshot_hash: row.get_by_name("selection_snapshot_hash").unwrap_or_default(),
                    context_snapshot_hash: row.get_by_name("context_snapshot_hash").ok().flatten(),
                    logical_date: row.get_by_name("logical_date").unwrap_or_default(),
                    status,
                    started_at: row.get_by_name("started_at").ok().flatten(),
                    completed_at: row.get_by_name("completed_at").ok().flatten(),
                })
            }
            None => None,
        };

        Ok(run)
    }

    pub fn get_selection_snapshot_by_hash(
        &self,
        snapshot_hash: &str,
    ) -> Result<Option<SelectionSnapshot>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT
                    id,
                    spec_id,
                    snapshot_hash,
                    logical_date,
                    watermark_value,
                    created_at
                FROM cf_selection_snapshots
                WHERE snapshot_hash = ?
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                &[DbValue::from(snapshot_hash)],
            )
            
            .context("Failed to fetch selection snapshot")?;

        Ok(row.map(|row| SelectionSnapshot {
            id: row.get_by_name("id").unwrap_or_default(),
            spec_id: row.get_by_name("spec_id").unwrap_or_default(),
            snapshot_hash: row.get_by_name("snapshot_hash").unwrap_or_default(),
            logical_date: row.get_by_name("logical_date").unwrap_or_default(),
            watermark_value: row.get_by_name("watermark_value").ok().flatten(),
            created_at: row.get_by_name("created_at").unwrap_or_default(),
        }))
    }

    pub fn get_selection_snapshots_for_file(
        &self,
        file_id: i64,
        limit: i64,
    ) -> Result<Vec<SelectionSnapshot>> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT
                    s.id,
                    s.spec_id,
                    s.snapshot_hash,
                    s.logical_date,
                    s.watermark_value,
                    s.created_at
                FROM cf_selection_snapshots s
                JOIN cf_selection_snapshot_files f ON f.snapshot_id = s.id
                WHERE f.file_id = ?
                ORDER BY s.created_at DESC
                LIMIT ?
                "#,
                &[DbValue::from(file_id), DbValue::from(limit)],
            )
            
            .context("Failed to fetch selection snapshots for file")?;

        rows.iter()
            .map(|row| {
                Ok(SelectionSnapshot {
                    id: row.get_by_name("id")?,
                    spec_id: row.get_by_name("spec_id")?,
                    snapshot_hash: row.get_by_name("snapshot_hash")?,
                    logical_date: row.get_by_name("logical_date")?,
                    watermark_value: row.get_by_name("watermark_value").ok().flatten(),
                    created_at: row.get_by_name("created_at")?,
                })
            })
            .collect()
    }

    fn initialize_tables(&self) -> Result<()> {
        let pipeline_status_values = PipelineRunStatus::ALL
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let create_sql = format!(
            r#"
                CREATE TABLE IF NOT EXISTS cf_selection_specs (
                    id TEXT PRIMARY KEY,
                    spec_json TEXT NOT NULL,
                    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS cf_selection_snapshots (
                    id TEXT PRIMARY KEY,
                    spec_id TEXT NOT NULL,
                    snapshot_hash TEXT NOT NULL,
                    logical_date TEXT NOT NULL,
                    watermark_value TEXT,
                    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS cf_selection_snapshot_files (
                    snapshot_id TEXT NOT NULL,
                    file_id BIGINT NOT NULL,
                    PRIMARY KEY (snapshot_id, file_id)
                );

                CREATE INDEX IF NOT EXISTS idx_snapshot_files_snapshot
                ON cf_selection_snapshot_files(snapshot_id);

                CREATE INDEX IF NOT EXISTS idx_snapshot_files_file
                ON cf_selection_snapshot_files(file_id);

                CREATE TABLE IF NOT EXISTS cf_pipelines (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    version BIGINT NOT NULL,
                    config_json TEXT NOT NULL,
                    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE UNIQUE INDEX IF NOT EXISTS idx_pipelines_name_version
                ON cf_pipelines(name, version);

                CREATE TABLE IF NOT EXISTS cf_pipeline_runs (
                    id TEXT PRIMARY KEY,
                    pipeline_id TEXT NOT NULL,
                    selection_spec_id TEXT NOT NULL,
                    selection_snapshot_hash TEXT NOT NULL,
                    context_snapshot_hash TEXT,
                    logical_date TEXT NOT NULL,
                    status TEXT NOT NULL
                        CHECK (status IN ({pipeline_status_values})),
                    started_at TIMESTAMP,
                    completed_at TIMESTAMP,
                    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX IF NOT EXISTS idx_pipeline_runs_pipeline
                ON cf_pipeline_runs(pipeline_id, logical_date);
                "#,
            pipeline_status_values = pipeline_status_values
        );
        self.conn
            .execute_batch(&create_sql)
            
            .context("Failed to initialize pipeline tables")?;

        Ok(())
    }
}

impl DuckDbPipelineStore {
    pub fn create_selection_spec(&self, spec_json: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO cf_selection_specs (id, spec_json) VALUES (?, ?)",
                &[DbValue::from(id.as_str()), DbValue::from(spec_json)],
            )
            
            .context("Failed to insert selection spec")?;
        Ok(id)
    }

    pub fn create_selection_snapshot(
        &self,
        spec_id: &str,
        snapshot_hash: &str,
        logical_date: &str,
        watermark_value: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                r#"
                INSERT INTO cf_selection_snapshots (
                    id, spec_id, snapshot_hash, logical_date, watermark_value
                )
                VALUES (?, ?, ?, ?, ?)
                "#,
                &[
                    DbValue::from(id.as_str()),
                    DbValue::from(spec_id),
                    DbValue::from(snapshot_hash),
                    DbValue::from(logical_date),
                    DbValue::from(watermark_value),
                ],
            )
            
            .context("Failed to insert selection snapshot")?;
        Ok(id)
    }

    pub fn insert_snapshot_files(&self, snapshot_id: &str, file_ids: &[i64]) -> Result<()> {
        for file_id in file_ids {
            self.conn
                .execute(
                    "INSERT INTO cf_selection_snapshot_files (snapshot_id, file_id) VALUES (?, ?)",
                    &[DbValue::from(snapshot_id), DbValue::from(*file_id)],
                )
                
                .context("Failed to insert snapshot file")?;
        }
        Ok(())
    }

    pub fn create_pipeline(&self, name: &str, version: i64, config_json: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO cf_pipelines (id, name, version, config_json) VALUES (?, ?, ?, ?)",
                &[
                    DbValue::from(id.as_str()),
                    DbValue::from(name),
                    DbValue::from(version),
                    DbValue::from(config_json),
                ],
            )
            
            .context("Failed to insert pipeline")?;
        Ok(id)
    }

    pub fn get_latest_pipeline(&self, name: &str) -> Result<Option<Pipeline>> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT id, name, version, config_json, created_at
                FROM cf_pipelines
                WHERE name = ?
                ORDER BY version DESC
                LIMIT 1
                "#,
                &[DbValue::from(name)],
            )
            
            .context("Failed to fetch latest pipeline")?;

        Ok(row.map(|row| Pipeline {
            id: row.get_by_name("id").unwrap_or_default(),
            name: row.get_by_name("name").unwrap_or_default(),
            version: row.get_by_name("version").unwrap_or_default(),
            config_json: row.get_by_name("config_json").unwrap_or_default(),
            created_at: row.get_by_name("created_at").unwrap_or_default(),
        }))
    }

    pub fn create_pipeline_run(
        &self,
        pipeline_id: &str,
        selection_spec_id: &str,
        selection_snapshot_hash: &str,
        context_snapshot_hash: Option<&str>,
        logical_date: &str,
        status: PipelineRunStatus,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
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
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                &[
                    DbValue::from(id.as_str()),
                    DbValue::from(pipeline_id),
                    DbValue::from(selection_spec_id),
                    DbValue::from(selection_snapshot_hash),
                    DbValue::from(context_snapshot_hash),
                    DbValue::from(logical_date),
                    DbValue::from(status.as_str()),
                ],
            )
            
            .context("Failed to insert pipeline run")?;
        Ok(id)
    }

    pub fn set_pipeline_run_status(&self, run_id: &str, status: PipelineRunStatus) -> Result<()> {
        let (set_started, set_completed) = match status {
            PipelineRunStatus::Running => (true, false),
            PipelineRunStatus::Completed | PipelineRunStatus::Failed | PipelineRunStatus::NoOp => {
                (false, true)
            }
            PipelineRunStatus::Queued => (false, false),
        };

        let mut query = String::from("UPDATE cf_pipeline_runs SET status = ?");
        if set_started {
            query.push_str(", started_at = CURRENT_TIMESTAMP");
        }
        if set_completed {
            query.push_str(", completed_at = CURRENT_TIMESTAMP");
        }
        query.push_str(" WHERE id = ?");

        self.conn
            .execute(&query, &[DbValue::from(status.as_str()), DbValue::from(run_id)])
            
            .context("Failed to update pipeline run status")?;
        Ok(())
    }

    pub fn pipeline_run_exists(&self, pipeline_id: &str, logical_date: &str) -> Result<bool> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT 1 FROM cf_pipeline_runs
                WHERE pipeline_id = ? AND logical_date = ?
                LIMIT 1
                "#,
                &[DbValue::from(pipeline_id), DbValue::from(logical_date)],
            )
            
            .context("Failed to query pipeline runs")?;
        Ok(row.is_some())
    }

    pub fn resolve_selection_files(
        &self,
        filters: &SelectionFilters,
        logical_date_ms: i64,
    ) -> Result<SelectionResolution> {
        let mut sql = String::from("SELECT id, mtime FROM scout_files WHERE status != ?");
        let mut params: Vec<DbValue> = vec![DbValue::from(FileStatus::Deleted.as_str())];

        if let Some(source_id) = &filters.source_id {
            sql.push_str(" AND source_id = ?");
            params.push(DbValue::from(source_id.as_i64()));
        }
        if let Some(tag) = &filters.tag {
            sql.push_str(" AND tag = ?");
            params.push(DbValue::from(tag.as_str()));
        }
        if let Some(extension) = &filters.extension {
            sql.push_str(" AND extension = ?");
            params.push(DbValue::from(extension.as_str()));
        }
        if let Some(since_ms) = filters.since_ms {
            let start_ms = logical_date_ms.saturating_sub(since_ms);
            sql.push_str(" AND mtime >= ? AND mtime <= ?");
            params.push(DbValue::from(start_ms));
            params.push(DbValue::from(logical_date_ms));
        }

        let rows = self
            .conn
            .query_all(&sql, &params)
            
            .context("Failed to resolve selection files")?;

        let mut file_ids = Vec::with_capacity(rows.len());
        let mut watermark_max: Option<i64> = None;
        for row in rows {
            let id: i64 = row.get_by_name("id")?;
            let mtime: i64 = row.get_by_name("mtime")?;
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

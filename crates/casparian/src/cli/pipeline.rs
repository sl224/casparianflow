//! Pipeline CLI: apply/run/backfill for deterministic selections.

use crate::cli::config;
use crate::cli::error::HelpfulError;
use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbValue};
use casparian::storage::{DuckDbPipelineStore, Pipeline, SelectionFilters, SelectionResolution, WatermarkField};
use casparian::PipelineStore;
use clap::Subcommand;
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Subcommand, Debug, Clone)]
pub enum PipelineAction {
    /// Apply a pipeline YAML spec
    Apply {
        /// Path to pipeline YAML
        file: PathBuf,
    },
    /// Run a pipeline for a logical date
    Run {
        /// Pipeline name
        name: String,
        /// Logical date (YYYY-MM-DD or RFC3339). Defaults to today (UTC).
        #[arg(long)]
        logical_date: Option<String>,
        /// Preview only (no DB writes)
        #[arg(long)]
        dry_run: bool,
    },
    /// Backfill a pipeline over a date range (inclusive)
    Backfill {
        /// Pipeline name
        name: String,
        /// Start date (YYYY-MM-DD or RFC3339)
        #[arg(long)]
        start: String,
        /// End date (YYYY-MM-DD or RFC3339)
        #[arg(long)]
        end: String,
        /// Preview only (no DB writes)
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run(action: PipelineAction) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        match action {
            PipelineAction::Apply { file } => apply_pipeline(file).await,
            PipelineAction::Run {
                name,
                logical_date,
                dry_run,
            } => run_pipeline(&name, logical_date, dry_run).await,
            PipelineAction::Backfill {
                name,
                start,
                end,
                dry_run,
            } => backfill_pipeline(&name, &start, &end, dry_run).await,
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PipelineFile {
    pipeline: PipelineDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PipelineDefinition {
    name: String,
    #[serde(default)]
    schedule: Option<String>,
    selection: SelectionConfig,
    run: RunConfig,
    #[serde(default)]
    context: Option<ContextConfig>,
    #[serde(default)]
    export: Option<ExportConfig>,
    #[serde(default)]
    selection_spec_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelectionConfig {
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    watermark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunConfig {
    parser: String,
    #[serde(default)]
    output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextConfig {
    #[serde(default)]
    materialize: Option<MaterializeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaterializeConfig {
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportConfig {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    output: Option<String>,
}

struct PipelineStoreHandle {
    store: DuckDbPipelineStore,
}

impl PipelineStoreHandle {
    async fn open() -> Result<Self> {
        let db_path = config::active_db_path();
        let store = DuckDbPipelineStore::open(&db_path).await?;
        Ok(Self { store })
    }

    async fn create_selection_spec(&self, spec_json: &str) -> Result<String> {
        self.store.create_selection_spec(spec_json).await
    }

    async fn create_selection_snapshot(
        &self,
        spec_id: &str,
        snapshot_hash: &str,
        logical_date: &str,
        watermark_value: Option<&str>,
    ) -> Result<String> {
        self.store
            .create_selection_snapshot(spec_id, snapshot_hash, logical_date, watermark_value)
            .await
    }

    async fn insert_snapshot_files(&self, snapshot_id: &str, file_ids: &[i64]) -> Result<()> {
        self.store.insert_snapshot_files(snapshot_id, file_ids).await
    }

    async fn create_pipeline(&self, name: &str, version: i64, config_json: &str) -> Result<String> {
        self.store.create_pipeline(name, version, config_json).await
    }

    async fn get_latest_pipeline(&self, name: &str) -> Result<Option<Pipeline>> {
        self.store.get_latest_pipeline(name).await
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
        self.store
            .create_pipeline_run(
                pipeline_id,
                selection_spec_id,
                selection_snapshot_hash,
                context_snapshot_hash,
                logical_date,
                status,
            )
            .await
    }

    async fn set_pipeline_run_status(&self, run_id: &str, status: &str) -> Result<()> {
        self.store.set_pipeline_run_status(run_id, status).await
    }

    async fn pipeline_run_exists(&self, pipeline_id: &str, logical_date: &str) -> Result<bool> {
        self.store.pipeline_run_exists(pipeline_id, logical_date).await
    }

    async fn resolve_selection_files(
        &self,
        filters: &SelectionFilters,
        logical_date_ms: i64,
    ) -> Result<SelectionResolution> {
        self.store
            .resolve_selection_files(filters, logical_date_ms)
            .await
    }
}

async fn apply_pipeline(file: PathBuf) -> Result<()> {
    let spec = load_pipeline_file(&file)?;
    let store = PipelineStoreHandle::open().await?;

    let selection_spec_json = serde_json::to_string(&spec.pipeline.selection)
        .context("Failed to serialize selection spec")?;
    let selection_spec_id = store.create_selection_spec(&selection_spec_json).await?;

    let mut stored = spec;
    stored.pipeline.selection_spec_id = Some(selection_spec_id.clone());

    let config_json = serde_json::to_string(&stored).context("Failed to serialize pipeline")?;
    let latest = store.get_latest_pipeline(&stored.pipeline.name).await?;
    let next_version = latest.map(|p| p.version + 1).unwrap_or(1);
    let pipeline_id = store
        .create_pipeline(&stored.pipeline.name, next_version, &config_json)
        .await?;

    println!(
        "Applied pipeline '{}' v{} (id: {})",
        stored.pipeline.name, next_version, pipeline_id
    );
    println!("Selection spec: {}", selection_spec_id);
    Ok(())
}

async fn run_pipeline(name: &str, logical_date: Option<String>, dry_run: bool) -> Result<()> {
    let store = PipelineStoreHandle::open().await?;
    let conn = store.open_db_connection().await?;
    let pipeline = store
        .get_latest_pipeline(name)
        .await?
        .ok_or_else(|| HelpfulError::new(format!("Pipeline '{}' not found", name)))?;

    let spec: PipelineFile = serde_json::from_str(&pipeline.config_json)
        .context("Failed to parse pipeline config")?;
    let selection_spec_id = spec
        .pipeline
        .selection_spec_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Pipeline config missing selection_spec_id"))?;

    let (logical_date_str, logical_date_ms) = parse_logical_date(logical_date.as_deref())?;
    if store
        .pipeline_run_exists(&pipeline.id, &logical_date_str)
        .await?
    {
        println!("Pipeline '{}' already ran for {}", name, logical_date_str);
        return Ok(());
    }

    let (filters, since_ms) = build_filters(&spec.pipeline.selection)?;
    let filters = SelectionFilters {
        since_ms,
        ..filters
    };
    let resolution = store
        .resolve_selection_files(&filters, logical_date_ms)
        .await?;
    let snapshot_hash = snapshot_hash(&selection_spec_id, &logical_date_str, &resolution.file_ids);

    if dry_run {
        println!("Pipeline '{}' (dry run)", name);
        println!("Logical date: {}", logical_date_str);
        println!("Files matched: {}", resolution.file_ids.len());
        println!("Snapshot hash: {}", snapshot_hash);
        return Ok(());
    }

    let snapshot_id = store
        .create_selection_snapshot(
            &selection_spec_id,
            &snapshot_hash,
            &logical_date_str,
            resolution.watermark_value.as_deref(),
        )
        .await?;

    store
        .insert_snapshot_files(&snapshot_id, &resolution.file_ids)
        .await?;

    let status = if resolution.file_ids.is_empty() {
        "no_op"
    } else {
        "queued"
    };
    let run_id = store
        .create_pipeline_run(
            &pipeline.id,
            &selection_spec_id,
            &snapshot_hash,
            None,
            &logical_date_str,
            status,
        )
        .await?;

    if status == "no_op" {
        store.set_pipeline_run_status(&run_id, "no_op").await?;
    } else {
        enqueue_jobs(&conn, &run_id, &spec.pipeline.run.parser, &resolution.file_ids).await?;
    }

    println!("Pipeline '{}' queued (run id: {})", name, run_id);
    println!("Logical date: {}", logical_date_str);
    println!("Files matched: {}", resolution.file_ids.len());
    println!("Snapshot hash: {}", snapshot_hash);
    Ok(())
}

async fn backfill_pipeline(name: &str, start: &str, end: &str, dry_run: bool) -> Result<()> {
    let start_dt = parse_logical_date(Some(start))?;
    let end_dt = parse_logical_date(Some(end))?;
    let mut current = chrono::NaiveDate::parse_from_str(&start_dt.0, "%Y-%m-%d")?;
    let end_date = chrono::NaiveDate::parse_from_str(&end_dt.0, "%Y-%m-%d")?;

    while current <= end_date {
        let date_str = current.format("%Y-%m-%d").to_string();
        run_pipeline(name, Some(date_str), dry_run).await?;
        current = current.succ_opt().ok_or_else(|| anyhow::anyhow!("Invalid date increment"))?;
    }
    Ok(())
}

fn load_pipeline_file(path: &PathBuf) -> Result<PipelineFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read pipeline file: {}", path.display()))?;
    serde_yaml::from_str(&contents).context("Failed to parse pipeline YAML")
}

fn parse_logical_date(input: Option<&str>) -> Result<(String, i64)> {
    let dt = if let Some(value) = input {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value) {
            parsed.with_timezone(&chrono::Utc)
        } else {
            let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .with_context(|| format!("Invalid date '{}'", value))?;
            chrono::Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        }
    } else {
        let date = chrono::Utc::now().date_naive();
        chrono::Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
    };

    let date_str = dt.format("%Y-%m-%d").to_string();
    Ok((date_str, dt.timestamp_millis()))
}

fn build_filters(selection: &SelectionConfig) -> Result<(SelectionFilters, Option<i64>)> {
    let watermark = selection
        .watermark
        .as_deref()
        .map(|w| match w {
            "mtime" => Ok(WatermarkField::Mtime),
            other => Err(anyhow::anyhow!("Unsupported watermark '{}'", other)),
        })
        .transpose()?;

    let since_ms = selection
        .since
        .as_deref()
        .map(parse_duration_ms)
        .transpose()?;

    let filters = SelectionFilters {
        source_id: selection.source.clone(),
        tag: selection.tag.clone(),
        extension: selection.ext.clone(),
        since_ms: None,
        watermark,
    };
    Ok((filters, since_ms))
}

fn parse_duration_ms(raw: &str) -> Result<i64> {
    if let Some(days) = raw.strip_prefix('P').and_then(|v| v.strip_suffix('D')) {
        let days: i64 = days.parse()?;
        return Ok(days * 24 * 60 * 60 * 1000);
    }
    if let Some(hours) = raw.strip_prefix("PT").and_then(|v| v.strip_suffix('H')) {
        let hours: i64 = hours.parse()?;
        return Ok(hours * 60 * 60 * 1000);
    }
    if let Some(minutes) = raw.strip_prefix("PT").and_then(|v| v.strip_suffix('M')) {
        let minutes: i64 = minutes.parse()?;
        return Ok(minutes * 60 * 1000);
    }
    if let Some(seconds) = raw.strip_prefix("PT").and_then(|v| v.strip_suffix('S')) {
        let seconds: i64 = seconds.parse()?;
        return Ok(seconds * 1000);
    }

    Err(anyhow::anyhow!(
        "Unsupported duration '{}'. Use PnD, PTnH, PTnM, or PTnS.",
        raw
    ))
}

fn snapshot_hash(spec_id: &str, logical_date: &str, file_ids: &[i64]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(spec_id.as_bytes());
    hasher.update(logical_date.as_bytes());
    for id in file_ids {
        hasher.update(id.to_string().as_bytes());
        hasher.update(b",");
    }
    hasher.finalize().to_hex().to_string()
}

impl PipelineStoreHandle {
    async fn open_db_connection(&self) -> Result<DbConnection> {
        Ok(self.store.connection())
    }
}

async fn enqueue_jobs(
    conn: &DbConnection,
    run_id: &str,
    parser: &str,
    file_ids: &[i64],
) -> Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }
    ensure_queue_schema(conn).await?;
    for file_id in file_ids {
        conn.execute(
            "INSERT INTO cf_processing_queue (file_id, pipeline_run_id, plugin_name, status, priority) VALUES (?, ?, ?, 'QUEUED', 0)",
            &[
                DbValue::from(*file_id),
                DbValue::from(run_id),
                DbValue::from(parser),
            ],
        )
        .await
        .context("Failed to enqueue job")?;
    }
    Ok(())
}

async fn ensure_queue_schema(conn: &DbConnection) -> Result<()> {
    let queue = casparian_sentinel::JobQueue::new(conn.clone());
    queue.init_queue_schema().await?;
    Ok(())
}

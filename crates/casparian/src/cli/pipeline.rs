//! Pipeline CLI: apply/run/backfill for deterministic selections.

use crate::cli::config;
use crate::cli::context;
use crate::cli::error::HelpfulError;
use anyhow::{Context, Result};
use casparian::scout::{SourceId, WorkspaceId};
use casparian::storage::{
    PipelineStore, Pipeline, SelectionFilters, SelectionResolution, WatermarkField,
};
use casparian::telemetry::TelemetryRecorder;
use casparian_db::{DbConnection, DbValue};
use casparian_protocol::telemetry as protocol_telemetry;
use casparian_protocol::types::SchemaDefinition;
use casparian_protocol::{
    defaults, materialization_key, output_target_key, schema_hash, table_name_with_schema,
    PipelineRunStatus, ProcessingStatus, SchemaColumnSpec, SinkConfig, SinkMode,
};
use casparian_schema::approval::derive_scope_id;
use casparian_schema::{SchemaContract, SchemaStorage};
use casparian_sentinel::ExpectedOutputs;
use chrono::{TimeZone, Utc};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tracing::warn;
use uuid::Uuid;

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

pub fn run(action: PipelineAction, telemetry: Option<TelemetryRecorder>) -> Result<()> {
    match action {
        PipelineAction::Apply { file } => apply_pipeline(file),
        PipelineAction::Run {
            name,
            logical_date,
            dry_run,
        } => run_pipeline(
            &name,
            logical_date,
            dry_run,
            telemetry,
            Some("pipeline_run"),
        )
        .map(|_| ()),
        PipelineAction::Backfill {
            name,
            start,
            end,
            dry_run,
        } => backfill_pipeline(
            &name,
            &start,
            &end,
            dry_run,
            telemetry,
            Some("pipeline_backfill"),
        )
        .map(|_| ()),
    }
}

#[derive(Debug, Clone, Copy)]
struct PipelineRunMetrics {
    files_matched: usize,
    queued: u64,
    skipped: u64,
    already_ran: bool,
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
    #[serde(default, alias = "workspace_id")]
    workspace: Option<String>,
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
    materialize: MaterializeConfig,
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
    store: PipelineStore,
}

#[derive(Debug, Default)]
struct EnqueueSummary {
    queued: usize,
    skipped: usize,
}

impl PipelineStoreHandle {
    fn open() -> Result<Self> {
        let db_path = config::state_store_path();
        let store = PipelineStore::open(&db_path)?;
        Ok(Self { store })
    }

    fn create_selection_spec(&self, spec_json: &str) -> Result<String> {
        self.store.create_selection_spec(spec_json)
    }

    fn create_selection_snapshot(
        &self,
        spec_id: &str,
        snapshot_hash: &str,
        logical_date: &str,
        watermark_value: Option<&str>,
    ) -> Result<String> {
        self.store
            .create_selection_snapshot(spec_id, snapshot_hash, logical_date, watermark_value)
    }

    fn insert_snapshot_files(&self, snapshot_id: &str, file_ids: &[i64]) -> Result<()> {
        self.store.insert_snapshot_files(snapshot_id, file_ids)
    }

    fn create_pipeline(&self, name: &str, version: i64, config_json: &str) -> Result<String> {
        self.store.create_pipeline(name, version, config_json)
    }

    fn get_latest_pipeline(&self, name: &str) -> Result<Option<Pipeline>> {
        self.store.get_latest_pipeline(name)
    }

    fn create_pipeline_run(
        &self,
        pipeline_id: &str,
        selection_spec_id: &str,
        selection_snapshot_hash: &str,
        context_snapshot_hash: Option<&str>,
        logical_date: &str,
        status: PipelineRunStatus,
    ) -> Result<String> {
        self.store.create_pipeline_run(
            pipeline_id,
            selection_spec_id,
            selection_snapshot_hash,
            context_snapshot_hash,
            logical_date,
            status,
        )
    }

    fn set_pipeline_run_status(&self, run_id: &str, status: PipelineRunStatus) -> Result<()> {
        self.store.set_pipeline_run_status(run_id, status)
    }

    fn pipeline_run_exists(&self, pipeline_id: &str, logical_date: &str) -> Result<bool> {
        self.store.pipeline_run_exists(pipeline_id, logical_date)
    }

    fn resolve_selection_files(
        &self,
        filters: &SelectionFilters,
        logical_date_ms: i64,
    ) -> Result<SelectionResolution> {
        self.store.resolve_selection_files(filters, logical_date_ms)
    }
}

fn apply_pipeline(file: PathBuf) -> Result<()> {
    let spec = load_pipeline_file(&file)?;
    let store = PipelineStoreHandle::open()?;

    let selection_spec_json = serde_json::to_string(&spec.pipeline.selection)
        .context("Failed to serialize selection spec")?;
    let selection_spec_id = store.create_selection_spec(&selection_spec_json)?;

    let mut stored = spec;
    stored.pipeline.selection_spec_id = Some(selection_spec_id.clone());

    let config_json = serde_json::to_string(&stored).context("Failed to serialize pipeline")?;
    let latest = store.get_latest_pipeline(&stored.pipeline.name)?;
    let next_version = latest.map(|p| p.version + 1).unwrap_or(1);
    let pipeline_id = store.create_pipeline(&stored.pipeline.name, next_version, &config_json)?;

    println!(
        "Applied pipeline '{}' v{} (id: {})",
        stored.pipeline.name, next_version, pipeline_id
    );
    println!("Selection spec: {}", selection_spec_id);
    Ok(())
}

fn run_pipeline(
    name: &str,
    logical_date: Option<String>,
    dry_run: bool,
    telemetry: Option<TelemetryRecorder>,
    telemetry_kind: Option<&str>,
) -> Result<PipelineRunMetrics> {
    let store = PipelineStoreHandle::open()?;
    let conn = store.open_db_connection()?;
    let pipeline = store
        .get_latest_pipeline(name)?
        .ok_or_else(|| HelpfulError::new(format!("Pipeline '{}' not found", name)))?;

    let spec: PipelineFile =
        serde_json::from_str(&pipeline.config_json).context("Failed to parse pipeline config")?;
    let selection_spec_id = spec
        .pipeline
        .selection_spec_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Pipeline config missing selection_spec_id"))?;

    let (logical_date_str, logical_date_ms) = parse_logical_date(logical_date.as_deref())?;
    if store.pipeline_run_exists(&pipeline.id, &logical_date_str)? {
        println!("Pipeline '{}' already ran for {}", name, logical_date_str);
        return Ok(PipelineRunMetrics {
            files_matched: 0,
            queued: 0,
            skipped: 0,
            already_ran: true,
        });
    }

    let telemetry_start = Instant::now();
    let telemetry_run_id = telemetry.as_ref().map(|recorder| {
        let run_id = Uuid::new_v4().to_string();
        let payload = protocol_telemetry::RunStarted {
            run_id: run_id.clone(),
            kind: telemetry_kind.map(|value| value.to_string()),
            parser_hash: Some(recorder.hasher().hash_str(&spec.pipeline.run.parser)),
            input_hash: None,
            sink_hash: spec
                .pipeline
                .run
                .output
                .as_ref()
                .map(|value| recorder.hasher().hash_str(value)),
            started_at: chrono::Utc::now(),
        };
        recorder.emit_domain(
            protocol_telemetry::events::RUN_START,
            Some(&run_id),
            None,
            &payload,
        );
        run_id
    });

    let run_result = (|| -> Result<PipelineRunMetrics> {
        let workspace_id = resolve_workspace_id(&conn, &spec.pipeline.selection)?;
        let (filters, since_ms) = build_filters(&spec.pipeline.selection, workspace_id)?;
        validate_source_scope(&conn, &workspace_id, filters.source_id)?;
        let filters = SelectionFilters {
            since_ms,
            ..filters
        };
        let resolution = store.resolve_selection_files(&filters, logical_date_ms)?;
        let snapshot_hash =
            snapshot_hash(&selection_spec_id, &logical_date_str, &resolution.file_ids);

        if dry_run {
            println!("Pipeline '{}' (dry run)", name);
            println!("Logical date: {}", logical_date_str);
            println!("Files matched: {}", resolution.file_ids.len());
            println!("Snapshot hash: {}", snapshot_hash);
            return Ok(PipelineRunMetrics {
                files_matched: resolution.file_ids.len(),
                queued: 0,
                skipped: 0,
                already_ran: false,
            });
        }

        let snapshot_id = store.create_selection_snapshot(
            &selection_spec_id,
            &snapshot_hash,
            &logical_date_str,
            resolution.watermark_value.as_deref(),
        )?;

        store.insert_snapshot_files(&snapshot_id, &resolution.file_ids)?;

        let status = if resolution.file_ids.is_empty() {
            PipelineRunStatus::NoOp
        } else {
            PipelineRunStatus::Queued
        };
        let run_id = store.create_pipeline_run(
            &pipeline.id,
            &selection_spec_id,
            &snapshot_hash,
            None,
            &logical_date_str,
            status,
        )?;

        let summary = if status == PipelineRunStatus::NoOp {
            store.set_pipeline_run_status(&run_id, PipelineRunStatus::NoOp)?;
            EnqueueSummary::default()
        } else {
            enqueue_jobs(
                &conn,
                &run_id,
                &spec.pipeline.run.parser,
                &resolution.file_ids,
            )?
        };

        println!("Pipeline '{}' queued (run id: {})", name, run_id);
        println!("Logical date: {}", logical_date_str);
        println!("Files matched: {}", resolution.file_ids.len());
        if summary.queued > 0 || summary.skipped > 0 {
            println!(
                "Files queued: {} (skipped {})",
                summary.queued, summary.skipped
            );
        }
        println!("Snapshot hash: {}", snapshot_hash);
        Ok(PipelineRunMetrics {
            files_matched: resolution.file_ids.len(),
            queued: summary.queued as u64,
            skipped: summary.skipped as u64,
            already_ran: false,
        })
    })();

    match run_result {
        Ok(metrics) => {
            if let (Some(recorder), Some(run_id)) = (telemetry.as_ref(), telemetry_run_id.as_ref())
            {
                let payload = protocol_telemetry::RunCompleted {
                    run_id: run_id.clone(),
                    kind: telemetry_kind.map(|value| value.to_string()),
                    duration_ms: telemetry_start.elapsed().as_millis() as u64,
                    total_rows: metrics.files_matched as u64,
                    outputs: metrics.queued as usize,
                };
                recorder.emit_domain(
                    protocol_telemetry::events::RUN_COMPLETE,
                    Some(run_id),
                    None,
                    &payload,
                );
            }
            Ok(metrics)
        }
        Err(err) => {
            if let (Some(recorder), Some(run_id)) = (telemetry.as_ref(), telemetry_run_id.as_ref())
            {
                let payload = protocol_telemetry::RunFailed {
                    run_id: run_id.clone(),
                    kind: telemetry_kind.map(|value| value.to_string()),
                    duration_ms: telemetry_start.elapsed().as_millis() as u64,
                    error_class: classify_pipeline_error(&err),
                };
                recorder.emit_domain(
                    protocol_telemetry::events::RUN_FAIL,
                    Some(run_id),
                    None,
                    &payload,
                );
            }
            Err(err)
        }
    }
}

fn backfill_pipeline(
    name: &str,
    start: &str,
    end: &str,
    dry_run: bool,
    telemetry: Option<TelemetryRecorder>,
    telemetry_kind: Option<&str>,
) -> Result<()> {
    let start_dt = parse_logical_date(Some(start))?;
    let end_dt = parse_logical_date(Some(end))?;
    let mut current = chrono::NaiveDate::parse_from_str(&start_dt.0, "%Y-%m-%d")?;
    let end_date = chrono::NaiveDate::parse_from_str(&end_dt.0, "%Y-%m-%d")?;

    while current <= end_date {
        let date_str = current.format("%Y-%m-%d").to_string();
        run_pipeline(
            name,
            Some(date_str),
            dry_run,
            telemetry.clone(),
            telemetry_kind,
        )?;
        current = current
            .succ_opt()
            .ok_or_else(|| anyhow::anyhow!("Invalid date increment"))?;
    }
    Ok(())
}

fn load_pipeline_file(path: &PathBuf) -> Result<PipelineFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read pipeline file: {}", path.display()))?;
    serde_yaml::from_str(&contents).context("Failed to parse pipeline YAML")
}

fn classify_pipeline_error(err: &anyhow::Error) -> String {
    if err.is::<std::io::Error>() {
        "io_error".to_string()
    } else {
        "pipeline_error".to_string()
    }
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

fn build_filters(
    selection: &SelectionConfig,
    workspace_id: WorkspaceId,
) -> Result<(SelectionFilters, Option<i64>)> {
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

    let source_id = selection
        .source
        .as_deref()
        .map(SourceId::parse)
        .transpose()
        .context("Invalid source ID in selection config")?;

    let filters = SelectionFilters {
        workspace_id: Some(workspace_id),
        source_id,
        tag: selection.tag.clone(),
        extension: selection.ext.clone(),
        since_ms: None,
        watermark,
    };
    Ok((filters, since_ms))
}

fn resolve_workspace_id(conn: &DbConnection, selection: &SelectionConfig) -> Result<WorkspaceId> {
    if let Some(raw) = selection.workspace.as_deref() {
        let workspace_id = resolve_workspace_ref(conn, raw)?;
        return Ok(workspace_id);
    }

    let active = context::get_active_workspace_id().map_err(|err| {
        anyhow::anyhow!(
            "Workspace context error: {}. Delete the context file to reset.",
            err
        )
    })?;

    if let Some(active_id) = active {
        if workspace_exists(conn, &active_id)? {
            return Ok(active_id);
        }
        warn!(workspace_id = %active_id, "Active workspace not found; resetting");
        context::clear_active_workspace()
            .map_err(|err| anyhow::anyhow!("Failed to clear workspace context: {}", err))?;
    }

    let workspace_id = ensure_default_workspace_id(conn)?;
    context::set_active_workspace(&workspace_id)
        .map_err(|err| anyhow::anyhow!("Failed to persist workspace context: {}", err))?;
    Ok(workspace_id)
}

fn resolve_workspace_ref(conn: &DbConnection, raw: &str) -> Result<WorkspaceId> {
    if let Ok(id) = WorkspaceId::parse(raw) {
        if workspace_exists(conn, &id)? {
            return Ok(id);
        }
    }

    let row = conn.query_optional(
        "SELECT id FROM cf_workspaces WHERE name = ?",
        &[DbValue::from(raw)],
    )?;
    if let Some(row) = row {
        let id_raw: String = row.get(0)?;
        return WorkspaceId::parse(&id_raw).map_err(Into::into);
    }

    Err(anyhow::anyhow!("Workspace '{}' not found", raw))
}

fn workspace_exists(conn: &DbConnection, workspace_id: &WorkspaceId) -> Result<bool> {
    let row = conn.query_optional(
        "SELECT 1 FROM cf_workspaces WHERE id = ?",
        &[DbValue::from(workspace_id.to_string())],
    )?;
    Ok(row.is_some())
}

fn ensure_default_workspace_id(conn: &DbConnection) -> Result<WorkspaceId> {
    let row = conn.query_optional(
        "SELECT id FROM cf_workspaces WHERE name = ? LIMIT 1",
        &[DbValue::from("Default")],
    )?;
    if let Some(row) = row {
        let id_raw: String = row.get(0)?;
        return WorkspaceId::parse(&id_raw).map_err(Into::into);
    }

    let row = conn.query_optional(
        "SELECT id FROM cf_workspaces ORDER BY created_at ASC LIMIT 1",
        &[],
    )?;
    if let Some(row) = row {
        let id_raw: String = row.get(0)?;
        return WorkspaceId::parse(&id_raw).map_err(Into::into);
    }

    let workspace_id = WorkspaceId::new();
    let now = Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO cf_workspaces (id, name, created_at) VALUES (?, ?, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from("Default"),
            DbValue::from(now),
        ],
    )?;

    Ok(workspace_id)
}

fn validate_source_scope(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    source_id: Option<SourceId>,
) -> Result<()> {
    let Some(source_id) = source_id else {
        return Ok(());
    };

    let row = conn.query_optional(
        "SELECT workspace_id FROM scout_sources WHERE id = ?",
        &[DbValue::from(source_id.as_i64())],
    )?;
    let Some(row) = row else {
        return Err(anyhow::anyhow!("Source '{}' not found", source_id));
    };

    let workspace_raw: String = row.get(0)?;
    let source_workspace = WorkspaceId::parse(&workspace_raw)?;
    if &source_workspace != workspace_id {
        return Err(anyhow::anyhow!(
            "Source '{}' belongs to workspace '{}', not active workspace '{}'",
            source_id,
            source_workspace,
            workspace_id
        ));
    }

    Ok(())
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

fn is_default_sink(topic: &str) -> bool {
    topic == "*" || topic == defaults::DEFAULT_SINK_TOPIC
}

fn table_exists(conn: &DbConnection, table: &str) -> Result<bool> {
    Ok(conn.table_exists(table)?)
}

struct ParserManifest {
    version: String,
    fingerprint: String,
}

fn load_parser_manifest(conn: &DbConnection, parser: &str) -> Result<ParserManifest> {
    if !table_exists(conn, "cf_plugin_manifest")? {
        return Err(anyhow::anyhow!(
            "Plugin registry table missing; publish '{}' before running pipelines",
            parser
        ));
    }

    let row = conn.query_optional(
        r#"
        SELECT version, artifact_hash
        FROM cf_plugin_manifest
        WHERE plugin_name = ? AND status IN (?, ?)
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        &[
            DbValue::from(parser),
            DbValue::from(casparian_protocol::PluginStatus::Active.as_str()),
            DbValue::from(casparian_protocol::PluginStatus::Deployed.as_str()),
        ],
    )?;

    let Some(row) = row else {
        return Err(anyhow::anyhow!(
            "Parser '{}' not found in registry (publish it first)",
            parser
        ));
    };

    let version: String = row.get_by_name("version")?;
    let artifact_hash: String = row.get_by_name("artifact_hash")?;
    let fingerprint = if artifact_hash.trim().is_empty() {
        version.clone()
    } else {
        artifact_hash
    };

    Ok(ParserManifest {
        version,
        fingerprint,
    })
}

fn schema_definition_from_contract(
    contract: &SchemaContract,
    output_name: &str,
) -> Result<SchemaDefinition> {
    let schema = contract
        .schemas
        .iter()
        .find(|s| s.name == output_name)
        .ok_or_else(|| anyhow::anyhow!("schema contract missing output '{}'", output_name))?;

    let columns = schema
        .columns
        .iter()
        .map(|col| SchemaColumnSpec {
            name: col.name.clone(),
            data_type: col.data_type.clone(),
            nullable: col.nullable,
            format: col.format.clone(),
        })
        .collect();

    Ok(SchemaDefinition { columns })
}

fn apply_contract_overrides(
    storage: &SchemaStorage,
    parser: &str,
    parser_version: &str,
    sinks: Vec<SinkConfig>,
) -> Result<Vec<SinkConfig>> {
    if parser_version.trim().is_empty() {
        return Ok(sinks);
    }

    let mut resolved = Vec::with_capacity(sinks.len());
    for mut sink in sinks {
        if sink.topic == "*" {
            resolved.push(sink);
            continue;
        }

        let scope_id = derive_scope_id(parser, parser_version, &sink.topic);
        if let Some(contract) = storage
            .get_contract_for_scope(&scope_id)
            .map_err(|e| anyhow::anyhow!(e))?
        {
            sink.schema = Some(schema_definition_from_contract(&contract, &sink.topic)?);
            if contract.quarantine_config.is_some() {
                sink.quarantine_config = contract.quarantine_config.clone();
            }
        }

        resolved.push(sink);
    }

    Ok(resolved)
}

fn load_sink_configs(
    conn: &DbConnection,
    parser: &str,
    parser_version: &str,
) -> Result<Vec<SinkConfig>> {
    let mut sinks = Vec::new();
    if table_exists(conn, "cf_topic_config")? {
        let rows = conn.query_all(
            r#"
            SELECT topic_name, uri, mode, quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
            FROM cf_topic_config
            WHERE plugin_name = ?
            ORDER BY id ASC
            "#,
            &[DbValue::from(parser)],
        )?;
        for row in rows {
            let mode_raw: String = row.get_by_name("mode")?;
            let mode = mode_raw
                .parse::<SinkMode>()
                .map_err(|e| anyhow::anyhow!(e))?;
            let allow_quarantine: Option<bool> = row.get_by_name("quarantine_allow")?;
            let max_quarantine_pct: Option<f64> = row.get_by_name("quarantine_max_pct")?;
            let max_quarantine_count: Option<i64> = row.get_by_name("quarantine_max_count")?;
            let quarantine_dir: Option<String> = row.get_by_name("quarantine_dir")?;

            let mut quarantine_config = casparian_protocol::QuarantineConfig::default();
            let mut has_quarantine = false;
            if let Some(value) = allow_quarantine {
                quarantine_config.allow_quarantine = value;
                has_quarantine = true;
            }
            if let Some(value) = max_quarantine_pct {
                quarantine_config.max_quarantine_pct = value;
                has_quarantine = true;
            }
            if let Some(value) = max_quarantine_count {
                let count = u64::try_from(value)
                    .map_err(|_| anyhow::anyhow!("quarantine_max_count out of range"))?;
                quarantine_config.max_quarantine_count = Some(count);
                has_quarantine = true;
            }
            if let Some(value) = quarantine_dir {
                quarantine_config.quarantine_dir = Some(value);
                has_quarantine = true;
            }

            sinks.push(SinkConfig {
                topic: row.get_by_name("topic_name")?,
                uri: row.get_by_name("uri")?,
                mode,
                quarantine_config: if has_quarantine {
                    Some(quarantine_config)
                } else {
                    None
                },
                schema: None,
            });
        }
    }

    if sinks.is_empty() {
        sinks.push(SinkConfig {
            topic: defaults::DEFAULT_SINK_TOPIC.to_string(),
            uri: defaults::DEFAULT_SINK_URI.to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        });
    }

    let storage = SchemaStorage::new(conn.clone()).map_err(|e| anyhow::anyhow!(e))?;
    apply_contract_overrides(&storage, parser, parser_version, sinks)
}

fn load_file_generation(conn: &DbConnection, file_ids: &[i64]) -> Result<HashMap<i64, (i64, i64)>> {
    if file_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut result = HashMap::new();
    const CHUNK_SIZE: usize = 200;
    for chunk in file_ids.chunks(CHUNK_SIZE) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT id, mtime, size FROM scout_files WHERE id IN ({})",
            placeholders
        );
        let params: Vec<DbValue> = chunk.iter().map(|id| DbValue::from(*id)).collect();
        let rows = conn.query_all(&sql, &params)?;
        for row in rows {
            let id: i64 = row.get_by_name("id")?;
            let mtime: i64 = row.get_by_name("mtime")?;
            let size: i64 = row.get_by_name("size")?;
            result.insert(id, (mtime, size));
        }
    }

    Ok(result)
}

fn load_existing_materialization_keys(
    conn: &DbConnection,
    keys: &[String],
) -> Result<HashSet<String>> {
    if keys.is_empty() || !table_exists(conn, "cf_output_materializations")? {
        return Ok(HashSet::new());
    }

    let mut existing = HashSet::new();
    const CHUNK_SIZE: usize = 500;
    for chunk in keys.chunks(CHUNK_SIZE) {
        let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT materialization_key FROM cf_output_materializations WHERE materialization_key IN ({})",
            placeholders
        );
        let params: Vec<DbValue> = chunk.iter().map(|k| DbValue::from(k.as_str())).collect();
        let rows = conn.query_all(&sql, &params)?;
        for row in rows {
            let key: String = row.get_by_name("materialization_key")?;
            existing.insert(key);
        }
    }

    Ok(existing)
}

/// Generate output target keys for the given sink configurations.
///
/// For explicit topics (not `*` or `output`), uses the topic name directly.
/// For default sinks (`*` or `output`), expands to all known outputs from
/// the plugin manifest using `ExpectedOutputs::list_for_plugin()`.
///
/// If the plugin has no declared outputs, returns an empty vec which triggers
/// conservative fallback behavior in the caller (forces reprocessing).
fn output_target_keys_for_sinks(
    conn: &DbConnection,
    sinks: &[SinkConfig],
    parser: &str,
    parser_version: &str,
) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    let mut default_sink: Option<&SinkConfig> = None;

    // Process explicit topics directly
    for sink in sinks {
        if is_default_sink(&sink.topic) {
            // Remember the default sink for later expansion
            default_sink = Some(sink);
        } else {
            // Explicit topic - use directly
            let schema = schema_hash(sink.schema.as_ref());
            let table_name = table_name_with_schema(&sink.topic, schema.as_deref());
            keys.push(output_target_key(
                &sink.topic,
                &sink.uri,
                sink.mode,
                Some(table_name.as_str()),
                schema.as_deref(),
            ));
        }
    }

    // Expand default sink to all known plugin outputs
    if let Some(sink) = default_sink {
        let expected_outputs = ExpectedOutputs::list_for_plugin(
            conn,
            parser,
            if parser_version.is_empty() {
                None
            } else {
                Some(parser_version)
            },
        )?;

        if expected_outputs.is_empty() {
            // Unknown plugin or no declared outputs - log warning
            // Return empty vec to trigger conservative fallback (force reprocessing)
            warn!(
                parser = parser,
                "Plugin has no declared outputs in manifest; using conservative fallback (jobs will be enqueued)"
            );
            return Ok(Vec::new());
        }

        // Expand default sink to each known output
        for output in &expected_outputs {
            let table_name =
                table_name_with_schema(&output.output_name, output.schema_hash.as_deref());
            keys.push(output_target_key(
                &output.output_name,
                &sink.uri,
                sink.mode,
                Some(table_name.as_str()),
                output.schema_hash.as_deref(),
            ));
        }
    }

    Ok(keys)
}

impl PipelineStoreHandle {
    fn open_db_connection(&self) -> Result<DbConnection> {
        Ok(self.store.connection())
    }
}

fn enqueue_jobs(
    conn: &DbConnection,
    run_id: &str,
    parser: &str,
    file_ids: &[i64],
) -> Result<EnqueueSummary> {
    if file_ids.is_empty() {
        return Ok(EnqueueSummary::default());
    }
    ensure_queue_schema(conn)?;

    let manifest = load_parser_manifest(conn, parser)?;
    let sinks = load_sink_configs(conn, parser, &manifest.version)?;
    let output_targets = output_target_keys_for_sinks(conn, &sinks, parser, &manifest.version)?;

    let file_meta = load_file_generation(conn, file_ids)?;
    let mut keys_by_file: HashMap<i64, Vec<String>> = HashMap::new();
    let mut all_keys = Vec::new();

    if !output_targets.is_empty() {
        for file_id in file_ids {
            let Some((mtime, size)) = file_meta.get(file_id) else {
                continue;
            };
            let mut keys = Vec::with_capacity(output_targets.len());
            for target_key in &output_targets {
                let key =
                    materialization_key(*file_id, *mtime, *size, &manifest.fingerprint, target_key);
                keys.push(key.clone());
                all_keys.push(key);
            }
            keys_by_file.insert(*file_id, keys);
        }
    }

    let existing_keys = load_existing_materialization_keys(conn, &all_keys)?;

    let mut summary = EnqueueSummary::default();
    for file_id in file_ids {
        let should_enqueue = if output_targets.is_empty() {
            true
        } else if let Some(keys) = keys_by_file.get(file_id) {
            !keys.iter().all(|key| existing_keys.contains(key))
        } else {
            true
        };

        if should_enqueue {
            conn.execute(
                "INSERT INTO cf_processing_queue (file_id, pipeline_run_id, plugin_name, status, priority) VALUES (?, ?, ?, ?, 0)",
                &[
                    DbValue::from(*file_id),
                    DbValue::from(run_id),
                    DbValue::from(parser),
                    DbValue::from(ProcessingStatus::Queued.as_str()),
                ],
            )

            .context("Failed to enqueue job")?;
            summary.queued += 1;
        } else {
            summary.skipped += 1;
        }
    }

    Ok(summary)
}

fn ensure_queue_schema(conn: &DbConnection) -> Result<()> {
    let queue = casparian_sentinel::JobQueue::new(conn.clone());
    queue.init_queue_schema()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup_db() -> DbConnection {
        let conn = DbConnection::open_sqlite(std::path::Path::new(":memory:")).unwrap();
        // Initialize registry schema for ExpectedOutputs
        let queue = casparian_sentinel::JobQueue::new(conn.clone());
        queue.init_registry_schema().unwrap();
        conn
    }

    fn insert_plugin(conn: &DbConnection, plugin_name: &str, version: &str, outputs_json: &str) {
        let now = now_millis();
        let source_hash = format!("hash_{}_{}", plugin_name, version);
        conn.execute(
            r#"
            INSERT INTO cf_plugin_manifest (
                plugin_name, version, runtime_kind, entrypoint,
                source_code, source_hash, status, env_hash, artifact_hash,
                manifest_json, protocol_version, schema_artifacts_json, outputs_json,
                signature_verified, created_at, deployed_at
            ) VALUES (?, ?, 'python_shim', 'test.py:parse', 'code', ?, 'ACTIVE', '', '',
                      '{}', '1.0', '{}', ?, false, ?, ?)
            "#,
            &[
                DbValue::from(plugin_name),
                DbValue::from(version),
                DbValue::from(source_hash.as_str()),
                DbValue::from(outputs_json),
                DbValue::from(now),
                DbValue::from(now),
            ],
        )
        .unwrap();
    }

    fn now_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH - check system clock")
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX)
    }

    #[test]
    fn test_is_default_sink_wildcard() {
        assert!(is_default_sink("*"));
        assert!(is_default_sink("output"));
        assert!(!is_default_sink("orders"));
        assert!(!is_default_sink("events"));
        assert!(!is_default_sink(""));
    }

    #[test]
    fn test_output_target_keys_explicit_topic() {
        let conn = setup_db();
        insert_plugin(&conn, "test_parser", "1.0.0", r#"{"orders": {}}"#);

        let sinks = vec![SinkConfig {
            topic: "orders".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys = output_target_keys_for_sinks(&conn, &sinks, "test_parser", "1.0.0").unwrap();

        // Should return exactly one key for the explicit topic
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn test_output_target_keys_default_sink_expands_to_plugin_outputs() {
        let conn = setup_db();
        // Plugin declares two outputs: orders and events
        insert_plugin(
            &conn,
            "multi_output_parser",
            "1.0.0",
            r#"{"orders": {}, "events": {}}"#,
        );

        let sinks = vec![SinkConfig {
            topic: "*".to_string(), // Default sink
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys =
            output_target_keys_for_sinks(&conn, &sinks, "multi_output_parser", "1.0.0").unwrap();

        // Should return two keys - one for each declared output
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_output_target_keys_default_sink_with_output_topic() {
        let conn = setup_db();
        insert_plugin(&conn, "single_output_parser", "1.0.0", r#"{"results": {}}"#);

        let sinks = vec![SinkConfig {
            topic: "output".to_string(), // Another default sink variant
            uri: "parquet://./data".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys =
            output_target_keys_for_sinks(&conn, &sinks, "single_output_parser", "1.0.0").unwrap();

        // Should return one key for the declared output
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn test_output_target_keys_unknown_plugin_returns_empty() {
        let conn = setup_db();
        // Don't insert any plugin

        let sinks = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys = output_target_keys_for_sinks(&conn, &sinks, "nonexistent_parser", "").unwrap();

        // Should return empty vec for unknown plugin (triggers conservative fallback)
        assert!(keys.is_empty());
    }

    #[test]
    fn test_output_target_keys_plugin_with_no_outputs_returns_empty() {
        let conn = setup_db();
        // Plugin exists but has empty outputs
        insert_plugin(&conn, "empty_parser", "1.0.0", "{}");

        let sinks = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys = output_target_keys_for_sinks(&conn, &sinks, "empty_parser", "1.0.0").unwrap();

        // Should return empty vec (triggers conservative fallback)
        assert!(keys.is_empty());
    }

    #[test]
    fn test_output_target_keys_mixed_explicit_and_default() {
        let conn = setup_db();
        insert_plugin(
            &conn,
            "mixed_parser",
            "1.0.0",
            r#"{"orders": {}, "events": {}}"#,
        );

        // Mix of explicit topic and default sink
        let sinks = vec![
            SinkConfig {
                topic: "custom_output".to_string(), // Explicit
                uri: "parquet://./custom".to_string(),
                mode: SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
            SinkConfig {
                topic: "*".to_string(), // Default - will expand
                uri: "parquet://./output".to_string(),
                mode: SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
        ];

        let keys = output_target_keys_for_sinks(&conn, &sinks, "mixed_parser", "1.0.0").unwrap();

        // 1 explicit + 2 from expansion = 3 keys
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_output_target_keys_changing_uri_produces_different_keys() {
        let conn = setup_db();
        insert_plugin(&conn, "uri_test_parser", "1.0.0", r#"{"data": {}}"#);

        let sinks1 = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output_v1".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let sinks2 = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output_v2".to_string(), // Different URI
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let keys1 =
            output_target_keys_for_sinks(&conn, &sinks1, "uri_test_parser", "1.0.0").unwrap();
        let keys2 =
            output_target_keys_for_sinks(&conn, &sinks2, "uri_test_parser", "1.0.0").unwrap();

        // Keys should be different due to different URIs
        assert_ne!(keys1, keys2);
    }

    #[test]
    fn test_output_target_keys_changing_mode_produces_different_keys() {
        let conn = setup_db();
        insert_plugin(&conn, "mode_test_parser", "1.0.0", r#"{"data": {}}"#);

        let sinks1 = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let sinks2 = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Replace, // Different mode
            quarantine_config: None,
            schema: None,
        }];

        let keys1 =
            output_target_keys_for_sinks(&conn, &sinks1, "mode_test_parser", "1.0.0").unwrap();
        let keys2 =
            output_target_keys_for_sinks(&conn, &sinks2, "mode_test_parser", "1.0.0").unwrap();

        // Keys should be different due to different modes
        assert_ne!(keys1, keys2);
    }
}

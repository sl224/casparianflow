//! Pipeline CLI: apply/run/backfill for deterministic selections.

use crate::cli::config;
use crate::cli::error::HelpfulError;
use anyhow::{Context, Result};
use casparian::scout::SourceId;
use casparian::storage::{
    DuckDbPipelineStore, Pipeline, SelectionFilters, SelectionResolution, WatermarkField,
};
use casparian_db::{DbConnection, DbValue};
use casparian_protocol::types::SchemaDefinition;
use casparian_protocol::{
    materialization_key, output_target_key, schema_hash, table_name_with_schema, PipelineRunStatus,
    ProcessingStatus, SchemaColumnSpec, SinkConfig, SinkMode,
};
use casparian_schema::approval::derive_scope_id;
use casparian_schema::{SchemaContract, SchemaStorage};
use chrono::TimeZone;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    match action {
        PipelineAction::Apply { file } => apply_pipeline(file),
        PipelineAction::Run {
            name,
            logical_date,
            dry_run,
        } => run_pipeline(&name, logical_date, dry_run),
        PipelineAction::Backfill {
            name,
            start,
            end,
            dry_run,
        } => backfill_pipeline(&name, &start, &end, dry_run),
    }
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
    store: DuckDbPipelineStore,
}

#[derive(Debug, Default)]
struct EnqueueSummary {
    queued: usize,
    skipped: usize,
}

impl PipelineStoreHandle {
    fn open() -> Result<Self> {
        let db_path = config::active_db_path();
        let store = DuckDbPipelineStore::open(&db_path)?;
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

fn run_pipeline(name: &str, logical_date: Option<String>, dry_run: bool) -> Result<()> {
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
        return Ok(());
    }

    let (filters, since_ms) = build_filters(&spec.pipeline.selection)?;
    let filters = SelectionFilters {
        since_ms,
        ..filters
    };
    let resolution = store.resolve_selection_files(&filters, logical_date_ms)?;
    let snapshot_hash = snapshot_hash(&selection_spec_id, &logical_date_str, &resolution.file_ids);

    if dry_run {
        println!("Pipeline '{}' (dry run)", name);
        println!("Logical date: {}", logical_date_str);
        println!("Files matched: {}", resolution.file_ids.len());
        println!("Snapshot hash: {}", snapshot_hash);
        return Ok(());
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
    Ok(())
}

fn backfill_pipeline(name: &str, start: &str, end: &str, dry_run: bool) -> Result<()> {
    let start_dt = parse_logical_date(Some(start))?;
    let end_dt = parse_logical_date(Some(end))?;
    let mut current = chrono::NaiveDate::parse_from_str(&start_dt.0, "%Y-%m-%d")?;
    let end_date = chrono::NaiveDate::parse_from_str(&end_dt.0, "%Y-%m-%d")?;

    while current <= end_date {
        let date_str = current.format("%Y-%m-%d").to_string();
        run_pipeline(name, Some(date_str), dry_run)?;
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

    let source_id = selection
        .source
        .as_deref()
        .map(SourceId::parse)
        .transpose()
        .context("Invalid source ID in selection config")?;

    let filters = SelectionFilters {
        source_id,
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

fn is_default_sink(topic: &str) -> bool {
    topic == "*" || topic == "output"
}

fn table_exists(conn: &DbConnection, table: &str) -> Result<bool> {
    let row = conn.query_optional(
        "SELECT 1 FROM information_schema.tables WHERE table_name = ?",
        &[DbValue::from(table)],
    )?;
    Ok(row.is_some())
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
            topic: "output".to_string(),
            uri: "parquet://./output".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        });
    }

    let storage = SchemaStorage::new(conn.clone()).map_err(|e| anyhow::anyhow!(e))?;
    apply_contract_overrides(&storage, parser, parser_version, sinks)
}

fn load_existing_output_targets(
    conn: &DbConnection,
    parser: &str,
    parser_fingerprint: &str,
) -> Result<Vec<String>> {
    if !table_exists(conn, "cf_output_materializations")? {
        return Ok(Vec::new());
    }
    let rows = conn.query_all(
        r#"
        SELECT DISTINCT output_target_key
        FROM cf_output_materializations
        WHERE plugin_name = ? AND parser_fingerprint = ?
        "#,
        &[DbValue::from(parser), DbValue::from(parser_fingerprint)],
    )?;
    Ok(rows
        .iter()
        .filter_map(|row| row.get_by_name::<String>("output_target_key").ok())
        .collect())
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

fn output_target_keys_for_sinks(sinks: &[SinkConfig]) -> Vec<String> {
    sinks
        .iter()
        .filter(|sink| !is_default_sink(&sink.topic))
        .map(|sink| {
            let schema = schema_hash(sink.schema.as_ref());
            let table_name = table_name_with_schema(&sink.topic, schema.as_deref());
            output_target_key(
                &sink.topic,
                &sink.uri,
                sink.mode,
                Some(table_name.as_str()),
                schema.as_deref(),
            )
        })
        .collect()
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
    let mut output_targets = output_target_keys_for_sinks(&sinks);

    if output_targets.is_empty() {
        output_targets = load_existing_output_targets(conn, parser, &manifest.fingerprint)?;
    }

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

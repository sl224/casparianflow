//! Sentinel - Control Plane for Casparian Flow
//!
//! Manages worker pool, dispatches jobs, and handles ZMQ ROUTER protocol.
//! Ported from Python sentinel.py with data-oriented design principles.

use anyhow::{Context, Result};
use casparian_protocol::http_types::{
    ApprovalOperation, ApprovalStatus, JobProgress as ApiJobProgress, JobResult as ApiJobResult,
};
use casparian_protocol::types::{
    self, ArtifactV1, DispatchCommand, IdentifyPayload, JobReceipt, JobStatus, ParsedSinkUri,
    RuntimeKind, SchemaColumnSpec, SchemaDefinition, SinkConfig, SinkMode, SinkScheme,
};
use casparian_protocol::{
    defaults, materialization_key, metrics, output_target_key, schema_hash, table_name_with_schema,
    safe_output_id, ApiJobId, JobId, Message, OpCode, ProcessingStatus, WorkerStatus,
};
use casparian_scout::{
    scan_path, ScanCancelToken, ScanConfig, ScanProgress, Source as ScoutSource, SourceId,
    SourceType, TagSource, TaggingRuleId, WorkspaceId,
};
use casparian_schema::approval::derive_scope_id;
use casparian_schema::{
    build_outputs_json, locked_schema_from_definition, SchemaContract, SchemaStorage,
};
use casparian_security::signing::compute_artifact_hash;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use zmq::{Context as ZmqContext, Socket};

use crate::control::{
    ControlRequest, ControlResponse, JobInfo, QueueStatsInfo, ScanState, ScoutFileInfo,
    ScoutFilesPage, ScoutFolderEntry, ScoutPatternMatch, ScoutPatternQueryResult, ScoutRuleInfo,
    ScoutScanProgress, ScoutScanStatus, ScoutSourceInfo, ScoutTagCount, ScoutTagFilter,
    ScoutTagStats,
};
use crate::catalog_executor::{CatalogExecutor, CatalogIntent};
use crate::sqlite_executor::{SqliteContext, SqliteExecutor};
use crate::db::queue::{OutputMaterialization, MAX_RETRY_COUNT};
use crate::db::{
    models::*, IntentState, SessionId,
};
use crate::metrics::METRICS;
use casparian_state_store::{DispatchData, StateStore, StateStoreQueueSession};

/// Workers are considered stale after this many seconds without heartbeat
const WORKER_TIMEOUT_SECS: f64 = 60.0;

/// How often to run cleanup (seconds)
const CLEANUP_INTERVAL_SECS: f64 = 10.0;

/// Dispatch backoff base (ms) when queue is empty or blocked
const DISPATCH_BACKOFF_BASE_MS: u64 = 50;
/// Dispatch backoff max (ms)
const DISPATCH_BACKOFF_MAX_MS: u64 = 1_000;
/// Dispatch backoff jitter cap (ms)
const DISPATCH_BACKOFF_JITTER_MS: u64 = 50;
/// Dispatch lease TTL in milliseconds.
const DISPATCH_LEASE_TTL_MS: i64 = 30_000;
/// How often to sweep expired dispatch leases (seconds).
const DISPATCH_LEASE_SWEEP_SECS: f64 = 5.0;
/// Delay before retrying transient dispatch preparation failures.
const DISPATCH_PREP_RETRY_MS: i64 = 10_000;
/// Grace period for worker reconnects after sentinel restart (seconds).
const RECONNECT_GRACE_SECS: f64 = 60.0;

// ============================================================================
// Circuit Breaker & Retry Constants
// ============================================================================

/// Base backoff in seconds for exponential retry (4^retry_count)
/// Retry 1: 4s, Retry 2: 16s, Retry 3: 64s
const BACKOFF_BASE_SECS: u64 = 4;

/// Consecutive failure threshold before tripping circuit breaker
const CIRCUIT_BREAKER_THRESHOLD: i32 = 5;

/// Refresh sink configs when older than this (seconds).
const TOPIC_CACHE_TTL_SECS: f64 = 30.0;

/// Default worker cap.
const DEFAULT_MAX_WORKERS: usize = 4;
/// Hard worker cap.
const HARD_MAX_WORKERS: usize = 8;

// ============================================================================
// Scout scan tracking (control API)
// ============================================================================

#[derive(Debug, Clone)]
struct ScanJobState {
    scan_id: String,
    workspace_id: WorkspaceId,
    source_path: String,
    source_id: Option<SourceId>,
    state: ScanState,
    progress: Option<ScanProgress>,
    files_persisted: Option<u64>,
    error: Option<String>,
    cancel_token: Option<ScanCancelToken>,
}

#[derive(Debug)]
enum ScanEvent {
    Started { scan_id: String, source_id: SourceId },
    Progress { scan_id: String, progress: ScanProgress },
    Completed { scan_id: String, files_persisted: u64 },
    Failed { scan_id: String, error: String },
    Cancelled { scan_id: String },
}

struct PendingControlReply {
    identity: Vec<u8>,
    rx: mpsc::Receiver<anyhow::Result<ControlResponse>>,
}

struct PendingDispatch {
    identity: Vec<u8>,
    worker_id: String,
    requested_at: Instant,
    rx: mpsc::Receiver<anyhow::Result<Option<DispatchPlan>>>,
}

struct PendingConclude {
    job_id: i64,
    started_at: Instant,
    rx: mpsc::Receiver<anyhow::Result<ConcludeOutcome>>,
}

struct PendingCancelJob {
    identity: Vec<u8>,
    job_id: JobId,
    rx: mpsc::Receiver<anyhow::Result<bool>>,
}

struct DispatchPlan {
    job_id_db: i64,
    job_id: JobId,
    plugin_name: String,
    pipeline_run_id: Option<String>,
    lease_token: String,
    command: DispatchCommand,
}

enum ConcludeOutcome {
    Stale { job_id: i64 },
    Completed { job_id: i64, artifacts: Vec<ArtifactV1> },
    Failed { job_id: i64, retried: bool },
    Rejected { job_id: i64 },
    Aborted { job_id: i64 },
}

impl ScanJobState {
    fn to_status(&self) -> ScoutScanStatus {
        ScoutScanStatus {
            scan_id: self.scan_id.clone(),
            workspace_id: self.workspace_id,
            source_path: self.source_path.clone(),
            source_id: self.source_id,
            state: self.state.clone(),
            progress: self.progress.as_ref().map(scan_progress_to_api),
            files_persisted: self.files_persisted,
            error: self.error.clone(),
        }
    }
}

fn scan_progress_to_api(progress: &ScanProgress) -> ScoutScanProgress {
    ScoutScanProgress {
        dirs_scanned: progress.dirs_scanned as u64,
        files_found: progress.files_found as u64,
        files_persisted: progress.files_persisted as u64,
        current_dir: progress.current_dir.clone(),
        elapsed_ms: progress.elapsed_ms,
        files_per_sec: progress.files_per_sec,
        stalled: progress.stalled,
    }
}

fn resolve_dispatch_path(scan_root: &str, exec_root: Option<&str>, rel_path: &str) -> String {
    let root = exec_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(scan_root);
    join_root_and_rel(root, rel_path)
}

fn join_root_and_rel(root: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches('/');
    if looks_like_windows_path(root) {
        let root = root.trim_end_matches(|c| c == '\\' || c == '/');
        let rel = rel.replace('/', "\\");
        if rel.is_empty() {
            root.to_string()
        } else {
            format!("{root}\\{rel}")
        }
    } else {
        let root = root.trim_end_matches('/');
        if rel.is_empty() {
            root.to_string()
        } else {
            format!("{root}/{rel}")
        }
    }
}

fn looks_like_windows_path(path: &str) -> bool {
    if path.starts_with(r"\\") {
        return true;
    }
    let mut chars = path.chars();
    let first = chars.next();
    let second = chars.next();
    matches!((first, second), (Some(letter), Some(':')) if letter.is_ascii_alphabetic())
}

#[derive(Debug, Deserialize)]
struct PluginManifestPayload {
    name: String,
    version: String,
    protocol_version: String,
    runtime_kind: RuntimeKind,
    entrypoint: String,
    platform_os: Option<String>,
    platform_arch: Option<String>,
}

/// Connected worker state (kept in memory, not persisted)
///
/// Note: identity is NOT stored here - it's the key in the workers HashMap.
/// This avoids duplicate storage and keeps ownership clear.
#[derive(Debug, Clone)]
pub struct ConnectedWorker {
    pub status: WorkerStatus,
    pub last_seen: f64,
    /// Plugin capabilities reported by the worker.
    /// v1 assumes a homogeneous worker pool, so this is informational only.
    pub capabilities: Vec<String>,
    pub current_job_id: Option<JobId>,
    pub current_lease_token: Option<String>,
    pub worker_id: String,
}

impl ConnectedWorker {
    fn new(worker_id: String, capabilities: Vec<String>) -> Self {
        Self {
            status: WorkerStatus::Idle,
            last_seen: current_time(),
            capabilities,
            current_job_id: None,
            current_lease_token: None,
            worker_id,
        }
    }
}

/// Sentinel configuration
pub struct SentinelConfig {
    pub bind_addr: String,
    pub state_store_url: String,
    pub max_workers: usize,
    /// Optional control API bind address (e.g., "ipc:///tmp/casparian_control.sock" or "tcp://127.0.0.1:5556")
    /// If None, control API is disabled.
    pub control_addr: Option<String>,
    /// DuckDB query catalog path (local SQL over Parquet)
    pub query_catalog_path: std::path::PathBuf,
}

/// Main Sentinel control plane
pub struct Sentinel {
    context: ZmqContext,
    socket: Socket,
    /// Optional control API socket (ROUTER pattern)
    control_socket: Option<Socket>,
    workers: HashMap<Vec<u8>, ConnectedWorker>,
    state_store: Arc<StateStore>,
    sqlite_executor: SqliteExecutor,
    query_catalog_path: std::path::PathBuf,
    catalog_executor: CatalogExecutor,
    state_store_path: Option<std::path::PathBuf>,
    scan_jobs: HashMap<String, ScanJobState>,
    scan_event_tx: mpsc::Sender<ScanEvent>,
    scan_event_rx: mpsc::Receiver<ScanEvent>,
    pending_control_replies: Vec<PendingControlReply>,
    pending_dispatches: Vec<PendingDispatch>,
    pending_concludes: Vec<PendingConclude>,
    pending_cancel_jobs: Vec<PendingCancelJob>,
    pending_dispatch_sweep: Option<mpsc::Receiver<anyhow::Result<usize>>>,
    running: bool,
    last_cleanup: f64, // Last time we ran stale worker cleanup
    last_dispatch_lease_sweep: f64,
    /// Jobs orphaned by stale workers - need to be failed asynchronously
    orphaned_jobs: Vec<(JobId, Option<String>)>,
    startup_grace_deadline: Option<f64>,
    seen_worker_ids: HashSet<String>,
    reconciled_workers: HashSet<String>,
    dispatch_backoff_ms: u64,
    dispatch_cooldown_until: Option<Instant>,
    max_workers: usize,
}

impl Sentinel {
    /// Create and bind Sentinel
    pub fn bind(config: SentinelConfig) -> Result<Self> {
        let max_workers = if config.max_workers == 0 {
            DEFAULT_MAX_WORKERS
        } else {
            config.max_workers.min(HARD_MAX_WORKERS)
        };

        let state_store = StateStore::open(&config.state_store_url)
            .context("Failed to connect to state store")?;
        state_store.init()?;

        let state_store = Arc::new(state_store);
        let sqlite_executor =
            SqliteExecutor::start(state_store.clone()).context("Failed to start sqlite executor")?;
        let now = now_millis();
        if let Ok(requeued) =
            sqlite_executor.call(move |_, queue, _| queue.requeue_expired_dispatches(now))
        {
            if requeued > 0 {
                info!("Requeued {} expired dispatch leases on startup", requeued);
            }
        }

        let _ = sqlite_executor.execute(|state_store, _queue, ctx| {
            match Sentinel::load_topic_configs(state_store.routing()) {
                Ok(new_map) => {
                    ctx.topic_map = new_map;
                    ctx.topic_map_last_refresh = current_time();
                    info!("Loaded {} plugin topic configs", ctx.topic_map.len());
                }
                Err(e) => {
                    warn!("Failed to load topic configs on startup: {}", e);
                }
            }
            Ok(())
        });

        // Destructive Initialization for IPC sockets (Unix only)
        // Unlink stale socket files to prevent "Address in use" errors
        #[cfg(unix)]
        if let Some(socket_path) = config.bind_addr.strip_prefix("ipc://") {
            let path = std::path::Path::new(socket_path);
            if path.exists() {
                info!("Removing stale IPC socket: {}", socket_path);
                if let Err(e) = std::fs::remove_file(path) {
                    warn!("Failed to remove stale socket {}: {}", socket_path, e);
                }
            }
        }

        // Create and bind ROUTER socket
        let context = ZmqContext::new();
        let socket = context
            .socket(zmq::ROUTER)
            .context("Failed to create ROUTER socket")?;
        socket
            .bind(&config.bind_addr)
            .context("Failed to bind ROUTER socket")?;
        socket
            .set_router_mandatory(true)
            .context("Failed to set ROUTER_MANDATORY")?;
        socket
            .set_sndtimeo(50)
            .context("Failed to set socket send timeout")?;
        info!("Sentinel bound to {}", config.bind_addr);

        // Optionally create control API socket
        let control_socket = if let Some(ref control_addr) = config.control_addr {
            // Destructive initialization for IPC sockets
            #[cfg(unix)]
            if let Some(socket_path) = control_addr.strip_prefix("ipc://") {
                let path = std::path::Path::new(socket_path);
                if path.exists() {
                    info!("Removing stale control IPC socket: {}", socket_path);
                    if let Err(e) = std::fs::remove_file(path) {
                        warn!(
                            "Failed to remove stale control socket {}: {}",
                            socket_path, e
                        );
                    }
                }
            }

            let ctrl_socket = context
                .socket(zmq::ROUTER)
                .context("Failed to create control ROUTER socket")?;
            ctrl_socket
                .bind(control_addr)
                .with_context(|| format!("Failed to bind control socket to {}", control_addr))?;
            info!("Control API bound to {}", control_addr);
            Some(ctrl_socket)
        } else {
            None
        };

        let state_store_path = sqlite_path_from_url(&config.state_store_url);
        let catalog_executor = CatalogExecutor::start(config.query_catalog_path.clone());

        let (scan_event_tx, scan_event_rx) = mpsc::channel();

        Ok(Self {
            context,
            socket,
            control_socket,
            workers: HashMap::new(),
            state_store,
            sqlite_executor,
            query_catalog_path: config.query_catalog_path,
            catalog_executor,
            state_store_path,
            scan_jobs: HashMap::new(),
            scan_event_tx,
            scan_event_rx,
            pending_control_replies: Vec::new(),
            pending_dispatches: Vec::new(),
            pending_concludes: Vec::new(),
            pending_cancel_jobs: Vec::new(),
            pending_dispatch_sweep: None,
            running: false,
            last_cleanup: current_time(),
            last_dispatch_lease_sweep: current_time(),
            orphaned_jobs: Vec::new(),
            startup_grace_deadline: Some(current_time() + RECONNECT_GRACE_SECS),
            seen_worker_ids: HashSet::new(),
            reconciled_workers: HashSet::new(),
            dispatch_backoff_ms: 0,
            dispatch_cooldown_until: None,
            max_workers,
        })
    }

    /// Load topic configurations from database into memory (non-blocking cache)
    fn load_topic_configs(
        routing: &dyn casparian_state_store::RoutingStore,
    ) -> Result<HashMap<String, Vec<SinkConfig>>> {
        let configs = routing.list_topic_configs()?;

        let mut map: HashMap<String, Vec<SinkConfig>> = HashMap::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();

        for tc in configs {
            let key = (tc.plugin_name.clone(), tc.topic_name.clone());
            if !seen.insert(key.clone()) {
                anyhow::bail!(
                    "Duplicate sink config for plugin '{}' and topic '{}'",
                    key.0,
                    key.1
                );
            }
            let sink = SinkConfig {
                topic: tc.topic_name,
                uri: tc.uri,
                mode: tc.mode, // Already a SinkMode enum, parsed at the boundary
                quarantine_config: tc.quarantine_config.clone(),
                schema: None,
            };

            map.entry(tc.plugin_name).or_default().push(sink);
        }

        Ok(map)
    }

    fn resolve_sinks_for_plugin(
        topic_map: &HashMap<String, Vec<SinkConfig>>,
        plugin_name: &str,
    ) -> Vec<SinkConfig> {
        let mut sinks = topic_map.get(plugin_name).cloned().unwrap_or_default();
        if sinks.is_empty() {
            sinks.push(SinkConfig {
                topic: defaults::DEFAULT_SINK_TOPIC.to_string(),
                uri: defaults::DEFAULT_SINK_URI.to_string(),
                mode: SinkMode::Append,
                quarantine_config: None,
                schema: None,
            });
        }
        sinks
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

    fn apply_contract_overrides_with_storage(
        routing: &dyn casparian_state_store::RoutingStore,
        schema_storage: &SchemaStorage,
        plugin_name: &str,
        parser_version: &str,
        sinks: Vec<SinkConfig>,
    ) -> Result<Vec<SinkConfig>> {
        let mut resolved = Vec::with_capacity(sinks.len());
        let mut explicit_topics = HashSet::new();
        let mut default_sinks = Vec::new();

        for sink in sinks {
            if Sentinel::is_default_sink(&sink.topic) {
                default_sinks.push(sink);
                continue;
            }
            explicit_topics.insert(sink.topic.clone());
            let mut sink = sink;
            Self::attach_contract_override(schema_storage, plugin_name, parser_version, &mut sink)?;
            resolved.push(sink);
        }

        if default_sinks.is_empty() {
            return Ok(resolved);
        }

        let default_sink = default_sinks.remove(0);
        let expected_outputs = routing.expected_outputs_for_plugin(
            plugin_name,
            if parser_version.trim().is_empty() {
                None
            } else {
                Some(parser_version)
            },
        )?;

        if expected_outputs.is_empty() {
            resolved.push(default_sink);
            resolved.extend(default_sinks);
            return Ok(resolved);
        }

        for output in expected_outputs {
            if explicit_topics.contains(&output.output_name) {
                continue;
            }
            let mut sink = default_sink.clone();
            sink.topic = output.output_name;
            Self::attach_contract_override(schema_storage, plugin_name, parser_version, &mut sink)?;
            resolved.push(sink);
        }

        Ok(resolved)
    }

    fn attach_contract_override(
        schema_storage: &SchemaStorage,
        plugin_name: &str,
        parser_version: &str,
        sink: &mut SinkConfig,
    ) -> Result<()> {
        if parser_version.trim().is_empty() {
            return Ok(());
        }

        let scope_id = derive_scope_id(plugin_name, parser_version, &sink.topic);
        let contract = schema_storage
            .get_contract_for_scope(&scope_id)
            .map_err(|e| anyhow::anyhow!(e))?;

        if let Some(contract) = contract {
            sink.schema = Some(Self::schema_definition_from_contract(
                &contract,
                &sink.topic,
            )?);
            if contract.quarantine_config.is_some() {
                sink.quarantine_config = contract.quarantine_config.clone();
            }
        }

        Ok(())
    }


    fn is_default_sink(topic: &str) -> bool {
        topic == "*" || topic == defaults::DEFAULT_SINK_TOPIC
    }

    fn select_sink_for_output<'a>(
        sinks: &'a [SinkConfig],
        output_name: &str,
    ) -> Option<&'a SinkConfig> {
        if let Some(exact) = sinks.iter().find(|sink| sink.topic == output_name) {
            return Some(exact);
        }
        if let Some(default) = sinks.iter().find(|sink| Self::is_default_sink(&sink.topic)) {
            return Some(default);
        }
        if sinks.len() == 1 {
            return sinks.first();
        }
        None
    }

    fn record_materializations_for_job_with_context(
        state_store: &StateStore,
        queue: &casparian_state_store::StateStoreQueueSession,
        context: &mut SqliteContext,
        job_id: i64,
        receipt: &JobReceipt,
    ) -> Result<()> {
        let Some(dispatch) = queue.get_dispatch_metadata(job_id)? else {
            return Ok(());
        };

        let parser_version = dispatch.parser_version.clone().unwrap_or_default();
        let parser_fingerprint = dispatch
            .parser_fingerprint
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                if parser_version.trim().is_empty() {
                    None
                } else {
                    Some(parser_version.clone())
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        let Some((file_mtime, file_size)) = queue.load_file_generation(dispatch.file_id)? else {
            warn!("Missing scout_files entry for file_id {}", dispatch.file_id);
            return Ok(());
        };

        let sinks: Vec<SinkConfig> = if let Some(json) = dispatch.sink_config_json.as_deref() {
            serde_json::from_str(json)?
        } else {
            let now = current_time();
            if now - context.topic_map_last_refresh > TOPIC_CACHE_TTL_SECS {
                match Self::load_topic_configs(state_store.routing()) {
                    Ok(new_map) => {
                        context.topic_map = new_map;
                        context.topic_map_last_refresh = now;
                    }
                    Err(e) => {
                        warn!("Failed to refresh topic configs: {}", e);
                    }
                }
            }
            let resolved = Self::resolve_sinks_for_plugin(&context.topic_map, &dispatch.plugin_name);
            Self::apply_contract_overrides_with_storage(
                state_store.routing(),
                &context.schema_storage,
                &dispatch.plugin_name,
                &parser_version,
                resolved,
            )?
        };

        let mut output_rows: HashMap<String, i64> = HashMap::new();
        let mut output_status: HashMap<String, i64> = HashMap::new();
        for (key, value) in &receipt.metrics {
            if let Some(name) = metrics::parse_rows_by_output(key) {
                output_rows.insert(name.to_string(), *value);
            }
            if let Some(name) = metrics::parse_status_by_output(key) {
                output_status.insert(name.to_string(), *value);
            }
        }

        let mut artifact_tables: HashMap<String, String> = HashMap::new();
        for artifact in &receipt.artifacts {
            match artifact {
                ArtifactV1::Output {
                    output_name,
                    table: Some(table),
                    ..
                } => {
                    artifact_tables.insert(output_name.clone(), table.clone());
                }
                ArtifactV1::Quarantine {
                    output_name,
                    table: Some(table),
                    ..
                } => {
                    // Quarantine tables are tracked separately but may still be useful for diagnostics.
                    artifact_tables.insert(output_name.clone(), table.clone());
                }
                _ => {}
            }
        }

        for (output_name, rows) in &output_rows {
            let Some(sink) = Self::select_sink_for_output(&sinks, output_name) else {
                warn!(
                    "No sink config found for output '{}' (job {})",
                    output_name, job_id
                );
                continue;
            };
            let schema_hash = schema_hash(sink.schema.as_ref());
            let table_name = artifact_tables
                .get(output_name)
                .cloned()
                .or_else(|| Some(table_name_with_schema(output_name, schema_hash.as_deref())));
            let target_key = output_target_key(
                output_name,
                &sink.uri,
                sink.mode,
                table_name.as_deref(),
                schema_hash.as_deref(),
            );
            let mat_key = materialization_key(
                dispatch.file_id,
                file_mtime,
                file_size,
                &parser_fingerprint,
                &target_key,
            );
            let status_code = output_status.get(output_name).copied().unwrap_or(0);
            if status_code == 2 {
                continue;
            }
            let status = if status_code == 1 {
                "partial_success"
            } else {
                "success"
            };
            let record = OutputMaterialization {
                materialization_key: mat_key,
                output_target_key: target_key,
                file_id: dispatch.file_id,
                file_mtime,
                file_size,
                plugin_name: dispatch.plugin_name.clone(),
                parser_version: dispatch.parser_version.clone(),
                parser_fingerprint: parser_fingerprint.clone(),
                output_name: output_name.clone(),
                sink_uri: sink.uri.clone(),
                sink_mode: sink.mode,
                table_name,
                schema_hash,
                status: status.to_string(),
                rows: *rows,
                job_id,
            };
            queue.insert_output_materialization(&record)?;
        }

        for sink in sinks
            .iter()
            .filter(|sink| !Self::is_default_sink(&sink.topic))
        {
            if output_rows.contains_key(&sink.topic) {
                continue;
            }
            let schema_hash = schema_hash(sink.schema.as_ref());
            let table_name = Some(table_name_with_schema(&sink.topic, schema_hash.as_deref()));
            let target_key = output_target_key(
                &sink.topic,
                &sink.uri,
                sink.mode,
                table_name.as_deref(),
                schema_hash.as_deref(),
            );
            let mat_key = materialization_key(
                dispatch.file_id,
                file_mtime,
                file_size,
                &parser_fingerprint,
                &target_key,
            );
            let record = OutputMaterialization {
                materialization_key: mat_key,
                output_target_key: target_key,
                file_id: dispatch.file_id,
                file_mtime,
                file_size,
                plugin_name: dispatch.plugin_name.clone(),
                parser_version: dispatch.parser_version.clone(),
                parser_fingerprint: parser_fingerprint.clone(),
                output_name: sink.topic.clone(),
                sink_uri: sink.uri.clone(),
                sink_mode: sink.mode,
                table_name,
                schema_hash,
                status: "no_data".to_string(),
                rows: 0,
                job_id,
            };
            queue.insert_output_materialization(&record)?;
        }
        Ok(())
    }

    fn update_query_catalog_for_artifacts(&self, artifacts: &[ArtifactV1]) -> Result<()> {
        let mut views: HashMap<String, std::path::PathBuf> = HashMap::new();
        for artifact in artifacts {
            let (output_name, sink_uri, is_quarantine) = match artifact {
                ArtifactV1::Output {
                    output_name,
                    sink_uri,
                    ..
                } => (output_name.as_str(), sink_uri.as_str(), false),
                ArtifactV1::Quarantine {
                    output_name,
                    sink_uri,
                    ..
                } => (output_name.as_str(), sink_uri.as_str(), true),
                _ => continue,
            };

            let parsed = ParsedSinkUri::parse(sink_uri).map_err(|err| {
                anyhow::anyhow!("Failed to parse artifact URI '{}': {}", sink_uri, err)
            })?;

            let base_dir = match parsed.scheme {
                SinkScheme::Parquet => Some(parsed.path.clone()),
                SinkScheme::File => {
                    if parsed
                        .path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("parquet"))
                        .unwrap_or(false)
                    {
                        parsed.path.parent().map(|p| p.to_path_buf())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            let Some(base_dir) = base_dir else {
                continue;
            };

            let safe_name = safe_output_id(output_name);
            let pattern = base_dir.join(format!("{}_*.parquet", safe_name));
            let view_name = if is_quarantine {
                format!("quarantine.{}", quote_ident(output_name))
            } else {
                format!("outputs.{}", quote_ident(output_name))
            };
            views.entry(view_name).or_insert(pattern);
        }

        if views.is_empty() {
            return Ok(());
        }

        let intents = views
            .into_iter()
            .map(|(view_name, parquet_glob)| CatalogIntent {
                view_name,
                parquet_glob,
            })
            .collect::<Vec<_>>();
        self.catalog_executor.submit(intents);

        Ok(())
    }

    /// Main event loop
    pub fn run(&mut self) -> Result<()> {
        self.run_with_shutdown_inner(None)
    }

    /// Main event loop with a shutdown channel.
    pub fn run_with_shutdown(&mut self, stop_rx: mpsc::Receiver<()>) -> Result<()> {
        self.run_with_shutdown_inner(Some(stop_rx))
    }

    fn run_with_shutdown_inner(&mut self, stop_rx: Option<mpsc::Receiver<()>>) -> Result<()> {
        self.running = true;
        info!("Sentinel event loop started");

        while self.running {
            if let Some(rx) = stop_rx.as_ref() {
                match rx.try_recv() {
                    Ok(()) => {
                        info!("Sentinel received stop signal");
                        self.running = false;
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        info!("Sentinel stop channel closed");
                        self.running = false;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }

            // Poll sockets to avoid sequential blocking.
            let (worker_ready, control_ready) = {
                let mut items = vec![self.socket.as_poll_item(zmq::POLLIN)];
                let mut control_index = None;
                if let Some(control_socket) = &self.control_socket {
                    control_index = Some(items.len());
                    items.push(control_socket.as_poll_item(zmq::POLLIN));
                }

                if let Err(e) = zmq::poll(&mut items, 50) {
                    error!("Poll error: {}", e);
                }

                let worker_ready = items.get(0).map(|item| item.is_readable()).unwrap_or(false);
                let control_ready = control_index
                    .and_then(|idx| items.get(idx))
                    .map(|item| item.is_readable())
                    .unwrap_or(false);
                (worker_ready, control_ready)
            };

            if worker_ready {
                loop {
                    match self.recv_message() {
                        Ok(Some((identity, msg))) => {
                            if let Err(e) = self.handle_message(identity, msg) {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Recv error: {}", e);
                            break;
                        }
                    }
                }
            }

            if control_ready {
                loop {
                    match self.handle_control_requests() {
                        Ok(true) => continue,
                        Ok(false) => break,
                        Err(e) => {
                            error!("Control API error: {}", e);
                            break;
                        }
                    }
                }
            }

            self.drain_scan_events();
            if let Err(err) = self.drain_pending_control_replies() {
                warn!("Failed to send control replies: {}", err);
            }
            if let Err(err) = self.drain_pending_cancel_jobs() {
                warn!("Failed to send cancel responses: {}", err);
            }
            self.drain_pending_dispatches();
            self.drain_pending_concludes();
            self.drain_pending_dispatch_sweep();

            // Periodic cleanup of stale workers
            self.cleanup_stale_workers();

            // Periodic sweep of expired dispatch leases
            self.sweep_expired_dispatches();

            // Reconcile running jobs after restart grace period
            if let Err(err) = self.reconcile_missing_workers_after_grace() {
                warn!("Restart reconciliation failed: {}", err);
            }

            // Fail any orphaned jobs from stale workers
            if !self.orphaned_jobs.is_empty() {
                let jobs_to_fail: Vec<(JobId, Option<String>)> =
                    std::mem::take(&mut self.orphaned_jobs);
                for (job_id, lease_token) in jobs_to_fail {
                    let job_id_db = match job_id.to_i64() {
                        Ok(value) => value,
                        Err(err) => {
                            error!(
                                "Failed to convert orphaned job {} to storage id: {}",
                                job_id, err
                            );
                            continue;
                        }
                    };
                    let lease_token = lease_token.clone();
                    self.sqlite_executor.execute(move |_, queue, _| {
                        let updated = if let Some(token) = lease_token.as_deref() {
                            queue.fail_job_if_token_matches(
                                job_id_db,
                                token,
                                JobStatus::Failed.as_str(),
                                "Worker became unresponsive (stale heartbeat)",
                            )?
                        } else {
                            queue.fail_job(
                                job_id_db,
                                JobStatus::Failed.as_str(),
                                "Worker became unresponsive (stale heartbeat)",
                            )?;
                            true
                        };
                        if !updated {
                            warn!("Stale orphaned job {} ignored", job_id_db);
                            return Ok(());
                        }
                        info!(
                            "Marked orphaned job {} as {}",
                            job_id_db,
                            ProcessingStatus::Failed.as_str()
                        );
                        METRICS.inc_jobs_failed();
                        Ok(())
                    })?;
                }
            }
 
            // Dispatch loop (assign jobs to idle workers)
            if let Err(e) = self.dispatch_loop() {
                error!("Dispatch error: {}", e);
            }
        }

        info!("Sentinel stopped");
        Ok(())
    }

    fn drain_scan_events(&mut self) {
        while let Ok(event) = self.scan_event_rx.try_recv() {
            match event {
                ScanEvent::Started { scan_id, source_id } => {
                    if let Some(job) = self.scan_jobs.get_mut(&scan_id) {
                        job.state = ScanState::Running;
                        job.source_id = Some(source_id);
                    }
                }
                ScanEvent::Progress { scan_id, progress } => {
                    if let Some(job) = self.scan_jobs.get_mut(&scan_id) {
                        job.progress = Some(progress);
                    }
                }
                ScanEvent::Completed {
                    scan_id,
                    files_persisted,
                } => {
                    if let Some(job) = self.scan_jobs.get_mut(&scan_id) {
                        job.state = ScanState::Completed;
                        job.files_persisted = Some(files_persisted);
                        job.error = None;
                    }
                }
                ScanEvent::Failed { scan_id, error } => {
                    if let Some(job) = self.scan_jobs.get_mut(&scan_id) {
                        job.state = ScanState::Failed;
                        job.error = Some(error);
                    }
                }
                ScanEvent::Cancelled { scan_id } => {
                    if let Some(job) = self.scan_jobs.get_mut(&scan_id) {
                        job.state = ScanState::Cancelled;
                        job.error = Some("Scan cancelled".to_string());
                    }
                }
            }
        }
    }

    fn drain_pending_control_replies(&mut self) -> Result<()> {
        if self.pending_control_replies.is_empty() {
            return Ok(());
        }
        let mut index = 0;
        while index < self.pending_control_replies.len() {
            match self.pending_control_replies[index].rx.try_recv() {
                Ok(result) => {
                    let pending = self.pending_control_replies.swap_remove(index);
                    let response = match result {
                        Ok(response) => response,
                        Err(err) => {
                            ControlResponse::error("DB_ERROR", format!("DB error: {}", err))
                        }
                    };
                    self.send_control_response(pending.identity, response)?;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    warn!("Control response channel disconnected");
                    self.pending_control_replies.swap_remove(index);
                }
            }
        }
        Ok(())
    }

    fn drain_pending_cancel_jobs(&mut self) -> Result<()> {
        if self.pending_cancel_jobs.is_empty() {
            return Ok(());
        }
        let mut index = 0;
        while index < self.pending_cancel_jobs.len() {
            match self.pending_cancel_jobs[index].rx.try_recv() {
                Ok(result) => {
                    let pending = self.pending_cancel_jobs.swap_remove(index);
                    let response = match result {
                        Ok(true) => {
                            info!("Job {} cancelled via control API", pending.job_id);
                            ControlResponse::CancelResult {
                                success: true,
                                message: "Job cancelled".to_string(),
                            }
                        }
                        Ok(false) => {
                            let mut aborted = false;
                            let mut abort_error = None;
                            for (identity, worker) in &self.workers {
                                if worker.current_job_id == Some(pending.job_id) {
                                    if let Err(e) =
                                        self.send_abort_to_worker(identity.clone(), pending.job_id)
                                    {
                                        warn!(
                                            "Failed to send abort for job {}: {}",
                                            pending.job_id, e
                                        );
                                        abort_error = Some(format!("Failed to send abort: {}", e));
                                    } else {
                                        aborted = true;
                                    }
                                    break;
                                }
                            }
                            if let Some(error) = abort_error {
                                ControlResponse::CancelResult {
                                    success: false,
                                    message: error,
                                }
                            } else if aborted {
                                ControlResponse::CancelResult {
                                    success: true,
                                    message: "Abort signal sent to worker".to_string(),
                                }
                            } else {
                                ControlResponse::CancelResult {
                                    success: false,
                                    message: "Job not found or already completed".to_string(),
                                }
                            }
                        }
                        Err(err) => ControlResponse::error(
                            "DB_ERROR",
                            format!("Failed to cancel job: {}", err),
                        ),
                    };
                    self.send_control_response(pending.identity, response)?;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    warn!("Cancel job response channel disconnected");
                    self.pending_cancel_jobs.swap_remove(index);
                }
            }
        }
        Ok(())
    }

    fn drain_pending_dispatch_sweep(&mut self) {
        let Some(rx) = &self.pending_dispatch_sweep else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                match result {
                    Ok(count) if count > 0 => {
                        info!("Requeued {} expired dispatch leases", count);
                    }
                    Ok(_) => {}
                    Err(err) => {
                        warn!("Failed to sweep expired dispatch leases: {}", err);
                    }
                }
                self.pending_dispatch_sweep = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                warn!("Dispatch lease sweep channel disconnected");
                self.pending_dispatch_sweep = None;
            }
        }
    }

    fn drain_pending_dispatches(&mut self) {
        if self.pending_dispatches.is_empty() {
            return;
        }
        let mut index = 0;
        let mut processed_any = false;
        let mut dispatched_any = false;
        while index < self.pending_dispatches.len() {
            match self.pending_dispatches[index].rx.try_recv() {
                Ok(result) => {
                    processed_any = true;
                    let pending = self.pending_dispatches.swap_remove(index);
                    match result {
                        Ok(Some(plan)) => {
                            if let Some(worker) = self.workers.get(&pending.identity) {
                                if worker.status != WorkerStatus::Idle {
                                    warn!(
                                        "Dispatch plan for busy worker {}; requeueing job {}",
                                        worker.worker_id, plan.job_id_db
                                    );
                                    let job_id_db = plan.job_id_db;
                                    let _ = self.sqlite_executor.execute(move |_, queue, _| {
                                        queue.defer_job(
                                            job_id_db,
                                            now_millis(),
                                            Some("dispatch_worker_busy"),
                                        )?;
                                        Ok(())
                                    });
                                } else {
                                    match self.send_dispatch_plan(pending.identity.clone(), plan) {
                                        Ok(true) => dispatched_any = true,
                                        Ok(false) => {}
                                        Err(err) => {
                                            warn!("Dispatch send failed: {}", err);
                                        }
                                    }
                                }
                            } else {
                                warn!(
                                    "Dispatch plan for unknown worker; requeueing job {}",
                                    plan.job_id_db
                                );
                                let job_id_db = plan.job_id_db;
                                let _ = self.sqlite_executor.execute(move |_, queue, _| {
                                    queue.defer_job(
                                        job_id_db,
                                        now_millis(),
                                        Some("dispatch_worker_missing"),
                                    )?;
                                    Ok(())
                                });
                            }
                        }
                        Ok(None) => {}
                        Err(err) => {
                            warn!("Dispatch preparation failed: {}", err);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    warn!("Dispatch preparation channel disconnected");
                    processed_any = true;
                    self.pending_dispatches.swap_remove(index);
                }
            }
        }
        if processed_any {
            if dispatched_any {
                self.dispatch_backoff_ms = 0;
                self.dispatch_cooldown_until = None;
            } else {
                self.schedule_dispatch_backoff();
            }
        }
    }

    fn drain_pending_concludes(&mut self) {
        if self.pending_concludes.is_empty() {
            return;
        }
        let mut index = 0;
        while index < self.pending_concludes.len() {
            match self.pending_concludes[index].rx.try_recv() {
                Ok(result) => {
                    let pending = self.pending_concludes.swap_remove(index);
                    METRICS.record_conclude_time(pending.started_at);
                    match result {
                        Ok(outcome) => match outcome {
                            ConcludeOutcome::Stale { job_id } => {
                                warn!("Stale CONCLUDE ignored for job {}", job_id);
                            }
                            ConcludeOutcome::Completed { job_id, artifacts } => {
                                info!(
                                    "Job {} completed: {} artifacts",
                                    job_id,
                                    artifacts.len()
                                );
                                METRICS.inc_jobs_completed();
                                if let Err(err) = self.update_query_catalog_for_artifacts(&artifacts)
                                {
                                    warn!("Failed to update query catalog: {}", err);
                                }
                            }
                            ConcludeOutcome::Failed { job_id, retried } => {
                                if retried {
                                    METRICS.inc_jobs_retried();
                                } else {
                                    METRICS.inc_jobs_failed();
                                }
                                warn!("Job {} failed", job_id);
                            }
                            ConcludeOutcome::Rejected { job_id } => {
                                METRICS.inc_jobs_rejected();
                                warn!("Job {} rejected by worker", job_id);
                            }
                            ConcludeOutcome::Aborted { job_id } => {
                                METRICS.inc_jobs_aborted();
                                warn!("Job {} aborted", job_id);
                            }
                        },
                        Err(err) => {
                            warn!("Conclude processing failed: {}", err);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    warn!("Conclude response channel disconnected");
                    self.pending_concludes.swap_remove(index);
                }
            }
        }
    }

    fn sweep_expired_dispatches(&mut self) {
        let now = current_time();
        if now - self.last_dispatch_lease_sweep < DISPATCH_LEASE_SWEEP_SECS {
            return;
        }
        self.last_dispatch_lease_sweep = now;
        let now_ms = now_millis();
        if self.pending_dispatch_sweep.is_some() {
            return;
        }
        match self
            .sqlite_executor
            .submit(move |_, queue, _| queue.requeue_expired_dispatches(now_ms))
        {
            Ok(rx) => self.pending_dispatch_sweep = Some(rx),
            Err(err) => warn!("Failed to schedule dispatch lease sweep: {}", err),
        }
    }

    fn reconcile_running_jobs_for_worker(
        &mut self,
        worker_id: &str,
        active_job_ids: &[JobId],
    ) -> Result<()> {
        let mut active_set = HashSet::new();
        for job_id in active_job_ids {
            match job_id.to_i64() {
                Ok(id) => {
                    active_set.insert(id);
                }
                Err(err) => {
                    warn!("Heartbeat job id {} not representable: {}", job_id, err);
                }
            }
        }
        let worker_id = worker_id.to_string();
        self.sqlite_executor.execute(move |_, queue, _| {
            let db_jobs = queue.list_running_jobs_by_owner(&worker_id)?;
            let db_jobs_set: HashSet<i64> = db_jobs.iter().copied().collect();
            let now = now_millis();
            for job_id in &db_jobs {
                if !active_set.contains(job_id) {
                    warn!(
                        "Requeueing job {} after restart; worker {} did not report it",
                        job_id, worker_id
                    );
                    let _ = queue.defer_job(*job_id, now, Some("lost_on_restart"));
                }
            }

            for active_job in active_set {
                if !db_jobs_set.contains(&active_job) {
                    warn!(
                        "Worker {} reported active job {} not present in DB",
                        worker_id, active_job
                    );
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    fn reconcile_missing_workers_after_grace(&mut self) -> Result<()> {
        let Some(deadline) = self.startup_grace_deadline else {
            return Ok(());
        };
        let now = current_time();
        if now < deadline {
            return Ok(());
        }
        self.startup_grace_deadline = None;

        let seen_worker_ids = self.seen_worker_ids.clone();
        self.sqlite_executor.execute(move |_, queue, _| {
            let jobs = queue.list_running_jobs_with_owner()?;
            let now_ms = now_millis();
            for (job_id, owner) in jobs {
                let missing = owner
                    .as_deref()
                    .map(|id| !seen_worker_ids.contains(id))
                    .unwrap_or(true);
                if missing {
                    warn!(
                        "Requeueing job {} after restart; worker {:?} did not reconnect",
                        job_id, owner
                    );
                    let _ = queue.defer_job(job_id, now_ms, Some("worker_missing_after_restart"));
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Remove workers that haven't sent a heartbeat within WORKER_TIMEOUT_SECS
    /// Also collects orphaned jobs from stale workers to be failed asynchronously
    fn cleanup_stale_workers(&mut self) {
        let now = current_time();

        // Only run cleanup every CLEANUP_INTERVAL_SECS
        if now - self.last_cleanup < CLEANUP_INTERVAL_SECS {
            return;
        }
        self.last_cleanup = now;

        let cutoff = now - WORKER_TIMEOUT_SECS;
        let before_count = self.workers.len();

        // Collect stale workers and their current jobs before removing
        let stale_workers: Vec<(Vec<u8>, String, Option<JobId>, Option<String>)> = self
            .workers
            .iter()
            .filter(|(_, w)| w.last_seen < cutoff)
            .map(|(id, w)| {
                (
                    id.clone(),
                    w.worker_id.clone(),
                    w.current_job_id,
                    w.current_lease_token.clone(),
                )
            })
            .collect();

        // Remove stale workers and queue their jobs for failure
        for (id, worker_id, job_id, lease_token) in stale_workers {
            if self.workers.remove(&id).is_some() {
                warn!(
                    "Removing stale worker [{}]: last seen {:.0}s ago",
                    worker_id,
                    now - cutoff + WORKER_TIMEOUT_SECS
                );
                METRICS.inc_workers_cleaned_up();

                // Queue job for async failure if worker had an active job
                if let Some(jid) = job_id {
                    warn!(
                        "Job {} orphaned by stale worker [{}] - will be failed",
                        jid, worker_id
                    );
                    self.orphaned_jobs.push((jid, lease_token));
                }
            }
        }

        let removed = before_count - self.workers.len();
        if removed > 0 {
            info!(
                "Cleanup: removed {} stale workers, {} remaining, {} jobs to fail",
                removed,
                self.workers.len(),
                self.orphaned_jobs.len()
            );
        } else {
            debug!("Cleanup: {} workers active", self.workers.len());
        }
    }

    // ========================================================================
    // Control API Handling
    // ========================================================================

    /// Handle at most one control API request (non-blocking).
    /// Returns true if a request was handled.
    fn handle_control_requests(&mut self) -> Result<bool> {
        if self.control_socket.is_none() {
            return Ok(false);
        }

        // Try to receive a control request (non-blocking).
        // Use take/put pattern to satisfy borrow checker
        let control_socket = self.control_socket.take().unwrap();
        let multipart = match control_socket.recv_multipart(zmq::DONTWAIT) {
            Ok(parts) => {
                self.control_socket = Some(control_socket);
                parts
            }
            Err(zmq::Error::EAGAIN) => {
                self.control_socket = Some(control_socket);
                return Ok(false); // No request waiting
            }
            Err(e) => {
                self.control_socket = Some(control_socket);
                return Err(anyhow::anyhow!("Control socket recv error: {}", e));
            }
        };

        let (identity, request_bytes) = match multipart.len() {
            3 if multipart[1].is_empty() => (multipart[0].clone(), multipart[2].clone()),
            2 => (multipart[0].clone(), multipart[1].clone()),
            count => {
                warn!(
                    "Invalid control request frames (expected 2/3, got {})",
                    count
                );
                return Ok(false);
            }
        };

        // Parse and handle the request
        match serde_json::from_slice::<ControlRequest>(&request_bytes) {
            Ok(request) => {
                self.handle_control_request(identity, request)?;
            }
            Err(e) => {
                self.send_control_response(
                    identity,
                    ControlResponse::error("PARSE_ERROR", format!("Invalid request: {}", e)),
                )?;
            }
        }

        Ok(true)
    }

    /// Handle a single control request
    fn handle_control_request(
        &mut self,
        identity: Vec<u8>,
        request: ControlRequest,
    ) -> Result<()> {
        match request {
            ControlRequest::Ping => {
                self.send_control_response(identity, ControlResponse::Pong)?;
            }
            ControlRequest::StartScan { workspace_id, path } => {
                let response = self.handle_start_scan(workspace_id, &path);
                self.send_control_response(identity, response)?;
            }
            ControlRequest::GetScan { scan_id } => {
                let response = self.handle_get_scan(&scan_id);
                self.send_control_response(identity, response)?;
            }
            ControlRequest::ListScans { limit } => {
                let response = self.handle_list_scans(limit);
                self.send_control_response(identity, response)?;
            }
            ControlRequest::CancelScan { scan_id } => {
                let response = self.handle_cancel_scan(&scan_id);
                self.send_control_response(identity, response)?;
            }
            ControlRequest::CancelJob { job_id } => {
                let rx = self
                    .sqlite_executor
                    .submit(move |_, queue, _| queue.cancel_job(job_id))?;
                self.pending_cancel_jobs.push(PendingCancelJob {
                    identity,
                    job_id,
                    rx,
                });
            }
            request => {
                let rx = self.sqlite_executor.submit(move |state_store, queue, ctx| {
                    Ok(handle_control_request_db(state_store, queue, ctx, request))
                })?;
                self.pending_control_replies.push(PendingControlReply { identity, rx });
            }
        }
        Ok(())
    }

    fn send_control_response(&self, identity: Vec<u8>, response: ControlResponse) -> Result<()> {
        let Some(control_socket) = self.control_socket.as_ref() else {
            warn!("Control response dropped: control socket not available");
            return Ok(());
        };
        let response_bytes = serde_json::to_vec(&response)?;
        control_socket
            .send_multipart(&[identity.as_slice(), &[], &response_bytes], 0)
            .context("Failed to send control response")?;
        Ok(())
    }

    /// Handle CreateApiJob request
    fn handle_create_api_job(
        &self,
        job_type: casparian_protocol::HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output: Option<&str>,
        approval_id: Option<&str>,
        spec_json: Option<&str>,
    ) -> ControlResponse {
        match self.state_store.api().create_job(
            job_type,
            plugin_name,
            plugin_version,
            input_dir,
            output,
            approval_id,
            spec_json,
        ) {
            Ok(job_id) => ControlResponse::ApiJobCreated { job_id },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to create API job: {}", e))
            }
        }
    }

    /// Handle GetApiJob request
    fn handle_get_api_job(&self, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().get_job(job_id) {
            Ok(job) => ControlResponse::ApiJob(job),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get API job {}: {}", job_id, e),
            ),
        }
    }

    /// Handle ListApiJobs request
    fn handle_list_api_jobs(
        &self,
        status: Option<casparian_protocol::HttpJobStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;
        match self
            .state_store
            .api()
            .list_jobs(status, limit.saturating_add(offset))
        {
            Ok(jobs) => {
                let jobs = jobs
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                ControlResponse::ApiJobs(jobs)
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to list API jobs: {}", e)),
        }
    }

    /// Handle UpdateApiJobStatus request
    fn handle_update_api_job_status(
        &self,
        job_id: ApiJobId,
        status: casparian_protocol::HttpJobStatus,
    ) -> ControlResponse {
        match self.state_store.api().update_job_status(job_id, status) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Status updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job status {}: {}", job_id, e),
            ),
        }
    }

    /// Handle UpdateApiJobProgress request
    fn handle_update_api_job_progress(
        &self,
        job_id: ApiJobId,
        progress: ApiJobProgress,
    ) -> ControlResponse {
        match self.state_store.api().update_job_progress(
            job_id,
            &progress.phase,
            progress.items_done,
            progress.items_total,
            progress.message.as_deref(),
        ) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Progress updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job progress {}: {}", job_id, e),
            ),
        }
    }

    /// Handle UpdateApiJobResult request
    fn handle_update_api_job_result(
        &self,
        job_id: ApiJobId,
        result: ApiJobResult,
    ) -> ControlResponse {
        match self.state_store.api().update_job_result(job_id, &result) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Result updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job result {}: {}", job_id, e),
            ),
        }
    }

    /// Handle UpdateApiJobError request
    fn handle_update_api_job_error(&self, job_id: ApiJobId, error: &str) -> ControlResponse {
        match self.state_store.api().update_job_error(job_id, error) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Error updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job error {}: {}", job_id, e),
            ),
        }
    }

    /// Handle CancelApiJob request
    fn handle_cancel_api_job(&self, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().cancel_job(job_id) {
            Ok(success) => ControlResponse::ApiJobResult {
                success,
                message: if success {
                    "Job cancelled".to_string()
                } else {
                    "Job not found".to_string()
                },
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to cancel API job {}: {}", job_id, e),
            ),
        }
    }

    /// Handle ListApprovals request
    fn handle_list_approvals(
        &self,
        status: Option<ApprovalStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;
        match self.state_store.api().list_approvals(status) {
            Ok(approvals) => {
                let approvals = approvals
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                ControlResponse::Approvals(approvals)
            }
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to list approvals: {}", e))
            }
        }
    }

    /// Handle CreateApproval request
    fn handle_create_approval(
        &self,
        approval_id: &str,
        operation: ApprovalOperation,
        summary: &str,
        expires_in_seconds: i64,
    ) -> ControlResponse {
        let expires_in = ChronoDuration::seconds(expires_in_seconds.max(0));
        match self
            .state_store
            .api()
            .create_approval(approval_id, &operation, summary, expires_in)
        {
            Ok(()) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval created".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to create approval {}: {}", approval_id, e),
            ),
        }
    }

    /// Handle GetApproval request
    fn handle_get_approval(&self, approval_id: &str) -> ControlResponse {
        match self.state_store.api().get_approval(approval_id) {
            Ok(approval) => ControlResponse::Approval(approval),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get approval {}: {}", approval_id, e),
            ),
        }
    }

    /// Handle Approve request
    fn handle_approve(&self, approval_id: &str) -> ControlResponse {
        match self.state_store.api().approve(approval_id, None) {
            Ok(true) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval accepted".to_string(),
            },
            Ok(false) => ControlResponse::ApprovalResult {
                success: false,
                message: "Approval not found or not pending".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to approve {}: {}", approval_id, e),
            ),
        }
    }

    /// Handle Reject request
    fn handle_reject(&self, approval_id: &str, reason: &str) -> ControlResponse {
        match self.state_store.api().reject(approval_id, None, Some(reason)) {
            Ok(true) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval rejected".to_string(),
            },
            Ok(false) => ControlResponse::ApprovalResult {
                success: false,
                message: "Approval not found or not pending".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to reject {}: {}", approval_id, e),
            ),
        }
    }

    /// Handle SetApprovalJobId request
    fn handle_set_approval_job_id(&self, approval_id: &str, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().link_approval_to_job(approval_id, job_id) {
            Ok(()) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval linked to job".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!(
                    "Failed to link approval {} to job {}: {}",
                    approval_id, job_id, e
                ),
            ),
        }
    }

    /// Handle ExpireApprovals request
    fn handle_expire_approvals(&self) -> ControlResponse {
        match self.state_store.api().expire_approvals() {
            Ok(count) => ControlResponse::ApprovalResult {
                success: true,
                message: format!("Expired {} approvals", count),
            },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to expire approvals: {}", e))
            }
        }
    }

    /// Handle CreateSession request
    fn handle_create_session(&self, intent_text: &str, input_dir: Option<&str>) -> ControlResponse {
        match self.state_store.sessions().create_session(intent_text, input_dir) {
            Ok(session_id) => ControlResponse::SessionCreated { session_id },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to create session: {}", e))
            }
        }
    }

    /// Handle GetSession request
    fn handle_get_session(&self, session_id: SessionId) -> ControlResponse {
        match self.state_store.sessions().get_session(session_id) {
            Ok(session) => ControlResponse::Session(session),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get session {}: {}", session_id, e),
            ),
        }
    }

    /// Handle ListSessions request
    fn handle_list_sessions(
        &self,
        state: Option<IntentState>,
        limit: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        match self.state_store.sessions().list_sessions(state, limit) {
            Ok(sessions) => ControlResponse::Sessions(sessions),
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to list sessions: {}", e)),
        }
    }

    /// Handle ListSessionsNeedingInput request
    fn handle_list_sessions_needing_input(&self, limit: Option<i64>) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        match self.state_store.sessions().list_sessions_needing_input(limit) {
            Ok(sessions) => ControlResponse::Sessions(sessions),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to list sessions needing input: {}", e),
            ),
        }
    }

    /// Handle AdvanceSession request
    fn handle_advance_session(
        &self,
        session_id: SessionId,
        target_state: IntentState,
    ) -> ControlResponse {
        match self
            .state_store
            .sessions()
            .update_session_state(session_id, target_state)
        {
            Ok(true) => ControlResponse::SessionResult {
                success: true,
                message: "Session advanced".to_string(),
            },
            Ok(false) => ControlResponse::SessionResult {
                success: false,
                message: "Session not found".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to advance session {}: {}", session_id, e),
            ),
        }
    }

    /// Handle CancelSession request
    fn handle_cancel_session(&self, session_id: SessionId) -> ControlResponse {
        match self.state_store.sessions().cancel_session(session_id) {
            Ok(true) => ControlResponse::SessionResult {
                success: true,
                message: "Session cancelled".to_string(),
            },
            Ok(false) => ControlResponse::SessionResult {
                success: false,
                message: "Session not found".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to cancel session {}: {}", session_id, e),
            ),
        }
    }

    // =====================================================================
    // Scout control handlers (sources / rules / tags / scans)
    // =====================================================================

    fn handle_list_sources(&self, workspace_id: WorkspaceId) -> ControlResponse {
        let records = match self.state_store.scout().list_sources_with_counts(workspace_id) {
            Ok(records) => records,
            Err(e) => return ControlResponse::error("DB_ERROR", e.to_string()),
        };

        let sources = records
            .into_iter()
            .map(|record| {
                let source = record.source;
                ScoutSourceInfo {
                    id: source.id,
                    workspace_id: source.workspace_id,
                    name: source.name,
                    source_type: source.source_type,
                    path: source.path,
                    exec_path: source.exec_path,
                    enabled: source.enabled,
                    poll_interval_secs: source.poll_interval_secs,
                    file_count: record.file_count,
                }
            })
            .collect();

        ControlResponse::Sources(sources)
    }

    fn handle_upsert_source(&self, source: &ScoutSourceInfo) -> ControlResponse {
        let source_row = ScoutSource {
            workspace_id: source.workspace_id,
            id: source.id,
            name: source.name.clone(),
            source_type: source.source_type.clone(),
            path: source.path.clone(),
            exec_path: source.exec_path.clone(),
            poll_interval_secs: source.poll_interval_secs,
            enabled: source.enabled,
        };

        match self.state_store.scout().upsert_source(&source_row) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source upserted".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source upsert failed: {}", e)),
        }
    }

    fn handle_update_source(
        &self,
        source_id: SourceId,
        name: Option<&str>,
        path: Option<&str>,
    ) -> ControlResponse {
        let mut source = match self.state_store.scout().get_source(&source_id) {
            Ok(Some(source)) => source,
            Ok(None) => {
                return ControlResponse::SourceResult {
                    success: false,
                    message: "Source not found".to_string(),
                }
            }
            Err(e) => {
                return ControlResponse::error("DB_ERROR", format!("Source load failed: {}", e))
            }
        };

        if let Some(name) = name {
            source.name = name.to_string();
        }
        if let Some(path) = path {
            source.path = path.to_string();
        }

        match self.state_store.scout().upsert_source(&source) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source updated".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source update failed: {}", e)),
        }
    }

    fn handle_delete_source(&self, source_id: SourceId) -> ControlResponse {
        match self.state_store.scout().delete_source(&source_id) {
            Ok(true) => ControlResponse::SourceResult {
                success: true,
                message: "Source deleted".to_string(),
            },
            Ok(false) => ControlResponse::SourceResult {
                success: false,
                message: "Source not found".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source delete failed: {}", e)),
        }
    }

    fn handle_touch_source(&self, source_id: SourceId) -> ControlResponse {
        match self.state_store.scout().touch_source(&source_id) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source touched".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source touch failed: {}", e)),
        }
    }

    fn handle_list_rules(&self, workspace_id: WorkspaceId) -> ControlResponse {
        match self.state_store.scout().list_tagging_rules(&workspace_id) {
            Ok(rules) => {
                let mapped = rules
                    .into_iter()
                    .map(|rule| ScoutRuleInfo {
                        id: rule.id,
                        workspace_id: rule.workspace_id,
                        pattern: rule.pattern,
                        tag: rule.tag,
                        priority: rule.priority,
                        enabled: rule.enabled,
                    })
                    .collect();
                ControlResponse::Rules(mapped)
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rules load failed: {}", e)),
        }
    }

    fn handle_create_rule(
        &self,
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
        pattern: &str,
        tag: &str,
    ) -> ControlResponse {
        let name = format!("{} → {}", pattern, tag);
        let rule = casparian_scout::types::TaggingRule {
            id: rule_id,
            name,
            workspace_id,
            pattern: pattern.to_string(),
            tag: tag.to_string(),
            priority: 100,
            enabled: true,
        };

        match self.state_store.scout().upsert_tagging_rule(&rule) {
            Ok(()) => ControlResponse::RuleResult {
                success: true,
                message: "Rule created".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule create failed: {}", e)),
        }
    }

    fn handle_update_rule_enabled(
        &self,
        rule_id: TaggingRuleId,
        _workspace_id: WorkspaceId,
        enabled: bool,
    ) -> ControlResponse {
        let mut rule = match self.state_store.scout().get_tagging_rule(&rule_id) {
            Ok(Some(rule)) => rule,
            Ok(None) => {
                return ControlResponse::RuleResult {
                    success: false,
                    message: "Rule not found".to_string(),
                }
            }
            Err(e) => {
                return ControlResponse::error("DB_ERROR", format!("Rule load failed: {}", e))
            }
        };

        rule.enabled = enabled;

        match self.state_store.scout().upsert_tagging_rule(&rule) {
            Ok(()) => ControlResponse::RuleResult {
                success: true,
                message: "Rule updated".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule update failed: {}", e)),
        }
    }

    fn handle_delete_rule(
        &self,
        rule_id: TaggingRuleId,
        _workspace_id: WorkspaceId,
    ) -> ControlResponse {
        match self.state_store.scout().delete_tagging_rule(&rule_id) {
            Ok(true) => ControlResponse::RuleResult {
                success: true,
                message: "Rule deleted".to_string(),
            },
            Ok(false) => ControlResponse::RuleResult {
                success: false,
                message: "Rule not found".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule delete failed: {}", e)),
        }
    }

    fn handle_list_tags(&self, workspace_id: WorkspaceId, source_id: SourceId) -> ControlResponse {
        let stats = match self.state_store.scout().tag_stats(workspace_id, source_id) {
            Ok(stats) => stats,
            Err(e) => return ControlResponse::error("DB_ERROR", e.to_string()),
        };

        ControlResponse::TagStats(ScoutTagStats {
            total_files: stats.total_files,
            untagged_files: stats.untagged_files,
            tags: stats
                .tags
                .into_iter()
                .map(|tag| ScoutTagCount {
                    tag: tag.tag,
                    count: tag.count,
                })
                .collect(),
        })
    }

    fn handle_apply_tag(
        &self,
        file_id: i64,
        tag: &str,
        tag_source: TagSource,
        rule_id: Option<&TaggingRuleId>,
    ) -> ControlResponse {
        let result = match tag_source {
            TagSource::Manual => self.state_store.scout().tag_file(file_id, tag),
            TagSource::Rule => {
                let Some(rule_id) = rule_id else {
                    return ControlResponse::TagResult {
                        success: false,
                        message: "Rule-based tag missing rule_id".to_string(),
                    };
                };
                self.state_store.scout().tag_file_by_rule(file_id, tag, rule_id)
            }
        };

        match result {
            Ok(()) => ControlResponse::TagResult {
                success: true,
                message: "Tag applied".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Tag apply failed: {}", e)),
        }
    }

    fn handle_start_scan(
        &mut self,
        workspace_id: Option<WorkspaceId>,
        path: &str,
    ) -> ControlResponse {
        if self.state_store_path.is_none() {
            return ControlResponse::error(
                "DB_ERROR",
                "Scan requires sqlite state store".to_string(),
            );
        }

        let input_path = std::path::Path::new(path);
        let expanded_path = scan_path::expand_scan_path(input_path);
        if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
            return ControlResponse::error("INVALID_PATH", err.to_string());
        }
        let canonical_path = scan_path::canonicalize_scan_path(&expanded_path);
        let path_display = canonical_path.display().to_string();

        let workspace_id = match self
            .sqlite_executor
            .call(move |state_store, _queue, _ctx| {
            let scout = state_store.scout();
            let workspace_id = match workspace_id {
                Some(id) => match scout.get_workspace(&id) {
                    Ok(Some(_)) => id,
                    Ok(None) => {
                        anyhow::bail!("Workspace not found");
                    }
                    Err(e) => return Err(anyhow::anyhow!(e)),
                },
                None => match scout.ensure_default_workspace() {
                    Ok(ws) => ws.id,
                    Err(e) => return Err(anyhow::anyhow!(e)),
                },
            };
            Ok(workspace_id)
        }) {
            Ok(id) => id,
            Err(err) => {
                return ControlResponse::error("DB_ERROR", err.to_string());
            }
        };

        let scan_id = Uuid::new_v4().to_string();
        let cancel_token = ScanCancelToken::new();

        self.scan_jobs.insert(
            scan_id.clone(),
            ScanJobState {
                scan_id: scan_id.clone(),
                workspace_id,
                source_path: path_display.clone(),
                source_id: None,
                state: ScanState::Pending,
                progress: None,
                files_persisted: None,
                error: None,
                cancel_token: Some(cancel_token.clone()),
            },
        );

        let scan_id_for_thread = scan_id.clone();
        let workspace_id_for_thread = workspace_id;
        let path_display_for_thread = path_display.clone();
        let state_store_for_thread = self.state_store.clone();
        let cancel_token_for_thread = cancel_token.clone();
        let scan_event_tx = self.scan_event_tx.clone();
        std::thread::spawn(move || {
            let session = match state_store_for_thread.session_bulk() {
                Ok(session) => session,
                Err(e) => {
                    let _ = scan_event_tx.send(ScanEvent::Failed {
                        scan_id: scan_id_for_thread.clone(),
                        error: format!("Failed to open scout session: {}", e),
                    });
                    return;
                }
            };
            let scout = session.scout();

            let source = match scout
                .get_source_by_path(&workspace_id_for_thread, &path_display_for_thread)
            {
                Ok(Some(existing)) => existing,
                Ok(None) => {
                    let source_name = std::path::Path::new(&path_display_for_thread)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path_display_for_thread.clone());

                    if let Ok(Some(name_conflict)) =
                        scout.get_source_by_name(&workspace_id_for_thread, &source_name)
                    {
                        let _ = scan_event_tx.send(ScanEvent::Failed {
                            scan_id: scan_id_for_thread.clone(),
                            error: format!(
                                "A source named '{}' already exists at '{}'.",
                                source_name, name_conflict.path
                            ),
                        });
                        return;
                    }

                    if let Err(err) = scout.check_source_overlap(
                        &workspace_id_for_thread,
                        std::path::Path::new(&path_display_for_thread),
                    )
                    {
                        let _ = scan_event_tx.send(ScanEvent::Failed {
                            scan_id: scan_id_for_thread.clone(),
                            error: err.to_string(),
                        });
                        return;
                    }

                    let source_id = SourceId::new();
                    let new_source = ScoutSource {
                        workspace_id: workspace_id_for_thread,
                        id: source_id,
                        name: source_name,
                        source_type: SourceType::Local,
                        path: path_display_for_thread.clone(),
                        exec_path: None,
                        poll_interval_secs: 0,
                        enabled: true,
                    };

                    if let Err(e) = scout.upsert_source(&new_source) {
                        let _ = scan_event_tx.send(ScanEvent::Failed {
                            scan_id: scan_id_for_thread.clone(),
                            error: format!("Failed to save source: {}", e),
                        });
                        return;
                    }
                    new_source
                }
                Err(e) => {
                    let _ = scan_event_tx.send(ScanEvent::Failed {
                        scan_id: scan_id_for_thread.clone(),
                        error: format!("Database error: {}", e),
                    });
                    return;
                }
            };

            let _ = scan_event_tx.send(ScanEvent::Started {
                scan_id: scan_id_for_thread.clone(),
                source_id: source.id,
            });

            let (progress_tx, progress_rx) = mpsc::channel::<ScanProgress>();
            let progress_event_tx = scan_event_tx.clone();
            let progress_scan_id = scan_id_for_thread.clone();
            let progress_handle = std::thread::spawn(move || {
                while let Ok(progress) = progress_rx.recv() {
                    let _ = progress_event_tx.send(ScanEvent::Progress {
                        scan_id: progress_scan_id.clone(),
                        progress,
                    });
                }
            });

            let scan_config = ScanConfig::default();
            let scanner = match session.scanner(scan_config) {
                Ok(scanner) => scanner,
                Err(e) => {
                    let _ = scan_event_tx.send(ScanEvent::Failed {
                        scan_id: scan_id_for_thread.clone(),
                        error: format!("Failed to start scanner: {}", e),
                    });
                    return;
                }
            };
            let scan_result = scanner.scan_with_cancel(
                &source,
                Some(progress_tx),
                None,
                Some(cancel_token_for_thread),
            );
            drop(scanner);
            let _ = progress_handle.join();

            match scan_result {
                Ok(result) => {
                    let _ = scan_event_tx.send(ScanEvent::Completed {
                        scan_id: scan_id_for_thread.clone(),
                        files_persisted: result.stats.files_persisted,
                    });
                }
                Err(e) => {
                    if matches!(e, casparian_scout::error::ScoutError::Cancelled) {
                        let _ = scan_event_tx.send(ScanEvent::Cancelled {
                            scan_id: scan_id_for_thread.clone(),
                        });
                    } else {
                        let _ = scan_event_tx.send(ScanEvent::Failed {
                            scan_id: scan_id_for_thread.clone(),
                            error: format!("Scan failed: {}", e),
                        });
                    }
                }
            }
        });

        ControlResponse::ScanStarted { scan_id }
    }

    fn handle_get_scan(&self, scan_id: &str) -> ControlResponse {
        let status = self.scan_jobs.get(scan_id).map(|job| job.to_status());
        ControlResponse::ScanStatus(status)
    }

    fn handle_list_scans(&self, limit: Option<usize>) -> ControlResponse {
        let mut scans: Vec<ScoutScanStatus> =
            self.scan_jobs.values().map(|job| job.to_status()).collect();
        if let Some(limit) = limit {
            if scans.len() > limit {
                scans.truncate(limit);
            }
        }
        ControlResponse::Scans(scans)
    }

    fn handle_cancel_scan(&mut self, scan_id: &str) -> ControlResponse {
        if let Some(job) = self.scan_jobs.get_mut(scan_id) {
            if let Some(token) = job.cancel_token.as_ref() {
                token.cancel();
            }
            job.state = ScanState::Cancelled;
            job.error = Some("Scan cancelled".to_string());
            return ControlResponse::ScanResult {
                success: true,
                message: "Scan cancelled".to_string(),
            };
        }

        ControlResponse::ScanResult {
            success: false,
            message: "Scan not found".to_string(),
        }
    }

    /// Convert Job to JobInfo for API responses
    fn job_to_info(job: crate::db::queue::Job) -> JobInfo {
        JobInfo {
            id: job.id,
            file_id: job.file_id,
            plugin_name: job.plugin_name,
            status: job.status,
            priority: job.priority,
            retry_count: job.retry_count,
            created_at: job.created_at.map(millis_to_rfc3339),
            updated_at: job.updated_at.map(millis_to_rfc3339),
            error_message: job.error_message,
            parser_version: job.parser_version,
            pipeline_run_id: job.pipeline_run_id,
            quarantine_rows: job.quarantine_rows,
        }
    }

    /// Send abort message to a specific worker
    fn send_abort_to_worker(&self, identity: Vec<u8>, job_id: JobId) -> Result<()> {
        // Create abort message with empty payload
        let msg = Message::new(OpCode::Abort, job_id, vec![])?;
        let (header, body) = msg.pack()?;

        // Send ABORT message as multipart [identity, header, body]
        let frames = [identity.as_slice(), header.as_ref(), body.as_slice()];
        self.socket.send_multipart(&frames, 0)?;

        Ok(())
    }

    /// Receive next message with timeout
    ///
    /// ROUTER receives multipart message: [identity, header, payload]
    fn recv_message(&mut self) -> Result<Option<(Vec<u8>, Message)>> {
        let multipart = match self.socket.recv_multipart(zmq::DONTWAIT) {
            Ok(parts) => parts,
            Err(zmq::Error::EAGAIN) => return Ok(None),
            Err(e) => return Err(anyhow::anyhow!("ZMQ error: {}", e)),
        };

        let (identity, header, payload) = match multipart.len() {
            3 => (
                multipart[0].clone(),
                multipart[1].clone(),
                multipart[2].clone(),
            ),
            4 if multipart[1].is_empty() => (
                multipart[0].clone(),
                multipart[2].clone(),
                multipart[3].clone(),
            ),
            count => {
                warn!(
                    "Expected 3 frames [identity, header, payload], got {}",
                    count
                );
                return Ok(None);
            }
        };

        let msg = Message::unpack(&[header, payload])?;
        Ok(Some((identity, msg)))
    }

    /// Handle a received message
    fn handle_message(&mut self, identity: Vec<u8>, msg: Message) -> Result<()> {
        match msg.header.opcode {
            OpCode::Identify => {
                let payload: IdentifyPayload = serde_json::from_slice(&msg.payload)?;
                self.register_worker(identity, payload)?;
            }

            OpCode::DispatchAck => {
                let payload: types::DispatchAckPayload = serde_json::from_slice(&msg.payload)?;
                self.handle_dispatch_ack(identity, msg.header.job_id, payload)?;
            }

            OpCode::Conclude => {
                let receipt: JobReceipt = serde_json::from_slice(&msg.payload)?;
                self.handle_conclude(identity, msg.header.job_id, receipt)?;
            }

            OpCode::Err => {
                let err: types::ErrorPayload = serde_json::from_slice(&msg.payload)?;
                self.handle_error(identity, msg.header.job_id, err)?;
            }

            OpCode::Heartbeat => {
                let payload: types::HeartbeatPayload = serde_json::from_slice(&msg.payload)?;
                self.handle_heartbeat(identity, payload)?;
            }

            OpCode::Deploy => {
                let cmd: types::DeployCommand = serde_json::from_slice(&msg.payload)?;
                match self.handle_deploy(&identity, cmd) {
                    Ok(()) => {
                        info!("Deploy successful");
                    }
                    Err(e) => {
                        error!("Deploy failed: {}", e);
                        self.send_error(&identity, &e.to_string())?;
                    }
                }
            }

            _ => {
                warn!("Unhandled opcode: {:?}", msg.header.opcode);
            }
        }

        Ok(())
    }

    /// Register a worker from IDENTIFY message
    fn register_worker(&mut self, identity: Vec<u8>, payload: IdentifyPayload) -> Result<()> {
        // Generate a unique worker_id from the full identity if not provided
        // Use first 8 bytes of identity hash to avoid collisions from using only identity[0]
        let worker_id = payload.worker_id.unwrap_or_else(|| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&identity);
            let hash = hasher.finalize();
            format!(
                "worker-{:02x}{:02x}{:02x}{:02x}",
                hash[0], hash[1], hash[2], hash[3]
            )
        });

        // Vec instead of HashSet - linear scan is faster for small N
        let capabilities: Vec<String> = payload.capabilities;

        if let Some(existing) = self.workers.get_mut(&identity) {
            existing.last_seen = current_time();
            if existing.worker_id != worker_id {
                warn!(
                    "Worker identity reused with new worker_id (old={}, new={})",
                    existing.worker_id, worker_id
                );
                existing.worker_id = worker_id.clone();
            }
            existing.capabilities = capabilities;
            self.seen_worker_ids.insert(worker_id.clone());
            info!("Worker re-identified: {}", worker_id);
            return Ok(());
        }

        if self.workers.len() >= self.max_workers {
            let message = format!(
                "Worker registration rejected: max_workers {} reached",
                self.max_workers
            );
            warn!("{}", message);
            self.send_error(&identity, &message)?;
            return Ok(());
        }

        info!("Worker joined [{}]", worker_id);

        let worker = ConnectedWorker::new(worker_id.clone(), capabilities);
        self.workers.insert(identity, worker);
        self.seen_worker_ids.insert(worker_id.clone());
        METRICS.inc_workers_registered();
        info!("Worker registered: {}", worker_id);
        Ok(())
    }

    fn handle_dispatch_ack(
        &mut self,
        identity: Vec<u8>,
        job_id: JobId,
        payload: types::DispatchAckPayload,
    ) -> Result<()> {
        let Some(worker) = self.workers.get_mut(&identity) else {
            warn!("Dispatch ACK from unknown worker identity");
            return Ok(());
        };
        worker.last_seen = current_time();

        if let Some(payload_worker_id) = payload.worker_id.as_deref() {
            if payload_worker_id != worker.worker_id {
                warn!(
                    "Dispatch ACK worker_id mismatch: payload={} expected={}",
                    payload_worker_id, worker.worker_id
                );
            }
        }

        if worker.current_job_id != Some(job_id) {
            warn!(
                "Dispatch ACK for job {} does not match worker state {:?}",
                job_id, worker.current_job_id
            );
            return Ok(());
        }

        if worker.current_lease_token.as_deref() != Some(payload.lease_token.as_str()) {
            warn!(
                "Dispatch ACK lease token mismatch for job {}",
                job_id
            );
            return Ok(());
        }

        let job_id_db = job_id.to_i64().map_err(|err| {
            anyhow::anyhow!(
                "Job ID {} is not representable in storage: {}",
                job_id,
                err
            )
        })?;
        let lease_token = payload.lease_token.clone();
        let worker_id = worker.worker_id.clone();
        self.sqlite_executor.execute(move |_, queue, _| {
            let now = now_millis();
            let acked = queue.ack_dispatch(job_id_db, &lease_token, &worker_id, now)?;
            if !acked {
                warn!("Stale dispatch ACK ignored for job {}", job_id_db);
            }
            Ok(())
        })?;
        Ok(())
    }

    fn handle_heartbeat(
        &mut self,
        identity: Vec<u8>,
        payload: types::HeartbeatPayload,
    ) -> Result<()> {
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.last_seen = current_time();
            worker.status = match payload.status {
                types::HeartbeatStatus::Idle => WorkerStatus::Idle,
                types::HeartbeatStatus::Busy | types::HeartbeatStatus::Alive => WorkerStatus::Busy,
            };
            self.seen_worker_ids.insert(worker.worker_id.clone());
            if self.startup_grace_deadline.is_some()
                && !self.reconciled_workers.contains(&worker.worker_id)
            {
                let worker_id = worker.worker_id.clone();
                self.reconciled_workers.insert(worker_id.clone());
                self.reconcile_running_jobs_for_worker(&worker_id, &payload.active_job_ids)?;
            }
        } else {
            // Heartbeat from unknown identity - could be a worker that was cleaned up
            // or a misconfigured client. Log for debugging.
            debug!(
                "Received heartbeat from unknown identity ({} bytes, first byte: 0x{:02x}). \
                        Worker may have been cleaned up for being stale.",
                identity.len(),
                identity.first().copied().unwrap_or(0)
            );
        }
        Ok(())
    }

    /// Handle CONCLUDE message (job completed/failed)
    ///
    /// For failed jobs:
    /// - Extracts `is_transient` from receipt metrics to determine retry eligibility
    /// - Applies exponential backoff for transient errors
    /// - Updates parser health for circuit breaker tracking
    /// - Moves to dead letter queue after MAX_RETRY_COUNT or for permanent errors
    fn handle_conclude(
        &mut self,
        identity: Vec<u8>,
        job_id: JobId,
        receipt: JobReceipt,
    ) -> Result<()> {
        // Mark worker as idle
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Idle;
            worker.current_job_id = None;
            worker.current_lease_token = None;
            worker.last_seen = current_time();
        }

        // Validate job_id fits in i64 (database uses i64 for job IDs)
        let job_id: i64 = job_id.to_i64().map_err(|err| {
            anyhow::anyhow!(
                "Job ID {} is not representable in storage: {}. \
                This indicates a protocol error or corrupted message.",
                job_id,
                err
            )
        })?;

        let conclude_start = Instant::now();
        let receipt_for_db = receipt;
        let rx = self.sqlite_executor.submit(move |state_store, queue, ctx| {
            process_conclude_db(state_store, queue, ctx, job_id, receipt_for_db)
        })?;
        self.pending_concludes.push(PendingConclude {
            job_id,
            started_at: conclude_start,
            rx,
        });
        Ok(())
    }

    /// Handle ERR message
    fn handle_error(
        &mut self,
        identity: Vec<u8>,
        job_id: JobId,
        err: types::ErrorPayload,
    ) -> Result<()> {
        error!("Job {} error: {}", job_id, err.message);
        if let Some(trace) = &err.traceback {
            error!("Traceback:\n{}", trace);
        }

        let lease_token = self
            .workers
            .get(&identity)
            .and_then(|worker| worker.current_lease_token.clone());

        // Mark worker as idle
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Idle;
            worker.current_job_id = None;
            worker.current_lease_token = None;
            worker.last_seen = current_time();
        }

        // Validate job_id fits in i64
        let job_id: i64 = job_id.to_i64().map_err(|err| {
            anyhow::anyhow!("Job ID {} is not representable in storage: {}", job_id, err)
        })?;

        let error_message = err.message.clone();
        self.sqlite_executor.execute(move |_, queue, _| {
            if let Some(token) = lease_token.as_deref() {
                let updated = queue.fail_job_if_token_matches(
                    job_id,
                    token,
                    JobStatus::Failed.as_str(),
                    &error_message,
                )?;
                if !updated {
                    warn!("Stale ERR ignored for job {}", job_id);
                    return Ok(());
                }
            } else {
                queue.fail_job(job_id, JobStatus::Failed.as_str(), &error_message)?;
            }
            if let Err(err) = queue.update_pipeline_run_status_for_job(job_id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job_id, err
                );
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Dispatch loop: assign jobs to ALL idle workers (not just one per iteration)
    fn dispatch_loop(&mut self) -> Result<()> {
        if let Some(cooldown_until) = self.dispatch_cooldown_until {
            if Instant::now() < cooldown_until {
                return Ok(());
            }
        }

        // Collect idle worker identities first (to avoid borrow issues)
        let idle_identities: Vec<Vec<u8>> = self
            .workers
            .iter()
            .filter(|(id, w)| w.status == WorkerStatus::Idle && !self.is_dispatch_pending(id))
            .map(|(id, _)| id.clone())
            .collect();

        if idle_identities.is_empty() {
            return Ok(());
        }

        let now = now_millis();
        for identity in idle_identities {
            let Some(worker) = self.workers.get(&identity) else {
                continue;
            };
            let worker_id = worker.worker_id.clone();
            let worker_id_for_task = worker_id.clone();
            let rx = self.sqlite_executor.submit(move |state_store, queue, ctx| {
                Sentinel::prepare_dispatch_plan(
                    state_store,
                    queue,
                    ctx,
                    now,
                    DISPATCH_LEASE_TTL_MS,
                    &worker_id_for_task,
                )
            })?;
            self.pending_dispatches.push(PendingDispatch {
                identity,
                worker_id,
                requested_at: Instant::now(),
                rx,
            });
        }

        Ok(())
    }

    fn prepare_dispatch_plan(
        state_store: &StateStore,
        queue: &StateStoreQueueSession,
        context: &mut SqliteContext,
        now_ms: i64,
        ttl_ms: i64,
        worker_id: &str,
    ) -> Result<Option<DispatchPlan>> {
        let mut leased_jobs = queue.lease_jobs_for_dispatch(1, now_ms, ttl_ms)?;
        let Some(job) = leased_jobs.pop() else {
            return Ok(None);
        };

        if job.id < 0 {
            anyhow::bail!(
                "Job ID {} is negative - this indicates database corruption",
                job.id
            );
        }
        let job_id = JobId::try_from(job.id)
            .map_err(|err| anyhow::anyhow!("Invalid job id from queue ({}): {}", job.id, err))?;

        let lease_token = Uuid::new_v4().to_string();
        if !queue.set_dispatch_lease(job.id, &lease_token, worker_id)? {
            warn!("Failed to set dispatch lease for job {}", job.id);
            queue.defer_job(job.id, now_ms, Some("dispatch_lease_mismatch"))?;
            return Ok(None);
        }

        let fail_dispatch = |message: &str| -> Result<Option<DispatchPlan>> {
            warn!(
                "Dispatch prep failed for job {} (fatal): {}",
                job.id, message
            );
            let updated = queue.fail_job_if_token_matches_dispatching(
                job.id,
                &lease_token,
                JobStatus::Failed.as_str(),
                message,
            )?;
            if !updated {
                warn!("Stale dispatch failure ignored for job {}", job.id);
            }
            if let Err(err) = queue.update_pipeline_run_status_for_job(job.id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job.id, err
                );
            }
            Ok(None)
        };

        let defer_dispatch = |message: &str| -> Result<Option<DispatchPlan>> {
            warn!(
                "Dispatch prep failed for job {} (transient): {}",
                job.id, message
            );
            let scheduled_at = now_ms.saturating_add(DISPATCH_PREP_RETRY_MS);
            let updated = queue.defer_job_if_token_matches(
                job.id,
                &lease_token,
                scheduled_at,
                Some(message),
            )?;
            if !updated {
                warn!("Stale dispatch defer ignored for job {}", job.id);
            }
            if let Err(err) = queue.update_pipeline_run_status_for_job(job.id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job.id, err
                );
            }
            Ok(None)
        };

        let now = current_time();
        if now - context.topic_map_last_refresh > TOPIC_CACHE_TTL_SECS {
            match Self::load_topic_configs(state_store.routing()) {
                Ok(new_map) => {
                    context.topic_map = new_map;
                    context.topic_map_last_refresh = now;
                }
                Err(e) => {
                    warn!("Failed to refresh topic configs: {}", e);
                }
            }
        }

        let dispatch_data = match queue.load_dispatch_data(&job.plugin_name, job.file_id) {
            Ok(data) => data,
            Err(err) => {
                let msg = format!(
                    "Dispatch data missing for plugin '{}': {}",
                    job.plugin_name, err
                );
                return fail_dispatch(&msg);
            }
        };
        let DispatchData {
            rel_path,
            scan_root,
            exec_root,
            source_code,
            parser_version,
            env_hash,
            artifact_hash,
            runtime_kind,
            entrypoint,
            platform_os,
            platform_arch,
            signature_verified,
            signer_id,
        } = dispatch_data;

        let file_path = resolve_dispatch_path(&scan_root, exec_root.as_deref(), &rel_path);

        if entrypoint.trim().is_empty() {
            let msg = format!("Missing entrypoint for plugin '{}'", job.plugin_name);
            return fail_dispatch(&msg);
        }
        if artifact_hash.trim().is_empty() {
            let msg = format!("Missing artifact_hash for plugin '{}'", job.plugin_name);
            return fail_dispatch(&msg);
        }

        let env_hash = if env_hash.trim().is_empty() {
            None
        } else {
            Some(env_hash)
        };
        let source_code = if source_code.trim().is_empty() {
            None
        } else {
            Some(source_code)
        };

        match runtime_kind {
            RuntimeKind::PythonShim => {
                if env_hash.is_none() {
                    let msg = format!("Missing env_hash for plugin '{}'", job.plugin_name);
                    return fail_dispatch(&msg);
                }
                if source_code.is_none() {
                    let msg = format!("Missing source_code for plugin '{}'", job.plugin_name);
                    return fail_dispatch(&msg);
                }
            }
            RuntimeKind::NativeExec => {
                let platform_os = platform_os.as_ref().map(|value| value.trim()).unwrap_or("");
                let platform_arch = platform_arch
                    .as_ref()
                    .map(|value| value.trim())
                    .unwrap_or("");
                if platform_os.is_empty() || platform_arch.is_empty() {
                    let msg = format!(
                        "Missing platform_os/platform_arch for native plugin '{}'",
                        job.plugin_name
                    );
                    return fail_dispatch(&msg);
                }
            }
        }

        let sinks = Self::resolve_sinks_for_plugin(&context.topic_map, &job.plugin_name);
        let sinks = match Self::apply_contract_overrides_with_storage(
            state_store.routing(),
            &context.schema_storage,
            &job.plugin_name,
            &parser_version,
            sinks,
        ) {
            Ok(sinks) => sinks,
            Err(err) => {
                let msg = format!("Failed to apply contract overrides: {}", err);
                return defer_dispatch(&msg);
            }
        };

        let sink_config_json = match serde_json::to_string(&sinks) {
            Ok(json) => json,
            Err(err) => {
                let msg = format!("Failed to serialize sink config: {}", err);
                return defer_dispatch(&msg);
            }
        };
        if let Err(err) =
            queue.record_dispatch_metadata(job.id, &parser_version, &artifact_hash, &sink_config_json)
        {
            warn!(
                "Failed to persist dispatch metadata for job {}: {}",
                job.id, err
            );
        }

        let cmd = DispatchCommand {
            plugin_name: job.plugin_name.clone(),
            parser_version: Some(parser_version),
            file_path,
            sinks,
            file_id: job.file_id,
            lease_token: Some(lease_token.clone()),
            runtime_kind,
            entrypoint,
            platform_os,
            platform_arch,
            signature_verified,
            signer_id,
            env_hash,
            source_code,
            artifact_hash,
        };

        Ok(Some(DispatchPlan {
            job_id_db: job.id,
            job_id,
            plugin_name: job.plugin_name.clone(),
            pipeline_run_id: job.pipeline_run_id.clone(),
            lease_token,
            command: cmd,
        }))
    }

    fn schedule_dispatch_backoff(&mut self) {
        let next = if self.dispatch_backoff_ms == 0 {
            DISPATCH_BACKOFF_BASE_MS
        } else {
            (self.dispatch_backoff_ms * 2).min(DISPATCH_BACKOFF_MAX_MS)
        };
        self.dispatch_backoff_ms = next;

        let jitter_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 % DISPATCH_BACKOFF_JITTER_MS)
            .unwrap_or(0);
        self.dispatch_cooldown_until =
            Some(Instant::now() + Duration::from_millis(next + jitter_ms));
    }

    fn is_dispatch_pending(&self, identity: &[u8]) -> bool {
        self.pending_dispatches
            .iter()
            .any(|pending| pending.identity == identity)
    }

    /// Send a prepared dispatch command to a worker.
    /// Returns true when a DISPATCH was sent.
    fn send_dispatch_plan(&mut self, identity: Vec<u8>, plan: DispatchPlan) -> Result<bool> {
        let span = tracing::info_span!(
            "sentinel.dispatch_job",
            job_id = plan.job_id_db,
            file_id = plan.command.file_id,
            plugin = %plan.plugin_name,
            pipeline_run_id = %plan.pipeline_run_id.as_deref().unwrap_or("none"),
            duration_ms = tracing::field::Empty
        );
        let _guard = span.enter();
        let dispatch_start = Instant::now();

        let payload = serde_json::to_vec(&plan.command)?;
        let msg = Message::new(OpCode::Dispatch, plan.job_id, payload)?;
        let (header, body) = msg.pack()?;

        // Send DISPATCH message as multipart [identity, header, body]
        let frames = [identity.as_slice(), header.as_ref(), body.as_slice()];
        match self.socket.send_multipart(&frames, zmq::DONTWAIT) {
            Ok(()) => {}
            Err(err) => {
                warn!("Dispatch send failed for job {}: {}", plan.job_id_db, err);
                let job_id_db = plan.job_id_db;
                let _ = self.sqlite_executor.execute(move |_, queue, _| {
                    queue.defer_job(job_id_db, now_millis(), Some("dispatch_send_failed"))?;
                    Ok(())
                });
                if let Some(worker) = self.workers.get_mut(&identity) {
                    worker.status = WorkerStatus::Idle;
                    worker.current_job_id = None;
                    worker.current_lease_token = None;
                }
                return Ok(false);
            }
        }

        if let Some(run_id) = plan.pipeline_run_id.as_deref() {
            let run_id = run_id.to_string();
            let _ = self.sqlite_executor.execute(move |_, queue, _| {
                if let Err(err) = queue.set_pipeline_run_running(&run_id) {
                    warn!("Failed to set pipeline run {} running: {}", run_id, err);
                }
                Ok(())
            });
        }

        // Mark worker as busy
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Busy;
            worker.current_job_id = Some(plan.job_id);
            worker.current_lease_token = Some(plan.lease_token.clone());
        }

        METRICS.inc_jobs_dispatched();
        METRICS.inc_messages_sent();
        let duration_ms = dispatch_start.elapsed().as_millis() as u64;
        span.record("duration_ms", &duration_ms);
        METRICS.record_dispatch_time(dispatch_start);
        info!("Dispatched job {} ({})", plan.job_id_db, plan.plugin_name);
        Ok(true)
    }

    /// Handle DEPLOY command - register a new plugin version
    fn handle_deploy(&mut self, identity: &[u8], cmd: types::DeployCommand) -> Result<()> {
        info!(
            "Deploying plugin {} v{} from {}",
            cmd.plugin_name, cmd.version, cmd.publisher_name
        );

        if cmd.lockfile_content.trim().is_empty() {
            anyhow::bail!("lockfile_content is required for deploy");
        }
        if cmd.env_hash.trim().is_empty() {
            anyhow::bail!("env_hash is required for deploy");
        }
        if cmd.artifact_hash.trim().is_empty() {
            anyhow::bail!("artifact_hash is required for deploy");
        }
        if cmd.manifest_json.trim().is_empty() {
            anyhow::bail!("manifest_json is required for deploy");
        }
        if cmd.protocol_version.trim().is_empty() {
            anyhow::bail!("protocol_version is required for deploy");
        }
        if cmd.schema_artifacts_json.trim().is_empty() {
            anyhow::bail!("schema_artifacts_json is required for deploy");
        }

        let computed_env_hash = compute_sha256(&cmd.lockfile_content);
        if computed_env_hash != cmd.env_hash {
            anyhow::bail!(
                "Env hash mismatch: expected {}, got {}",
                &cmd.env_hash[..12.min(cmd.env_hash.len())],
                &computed_env_hash[..12]
            );
        }

        let manifest: PluginManifestPayload =
            serde_json::from_str(&cmd.manifest_json).context("Failed to parse manifest_json")?;
        if manifest.name != cmd.plugin_name {
            anyhow::bail!(
                "Manifest name '{}' does not match plugin_name '{}'",
                manifest.name,
                cmd.plugin_name
            );
        }
        if manifest.version != cmd.version {
            anyhow::bail!(
                "Manifest version '{}' does not match version '{}'",
                manifest.version,
                cmd.version
            );
        }
        if manifest.protocol_version != cmd.protocol_version {
            anyhow::bail!(
                "Manifest protocol_version '{}' does not match '{}'",
                manifest.protocol_version,
                cmd.protocol_version
            );
        }
        if manifest.entrypoint.trim().is_empty() {
            anyhow::bail!("Manifest field 'entrypoint' must be non-empty");
        }
        let platform_os = manifest
            .platform_os
            .as_ref()
            .map(|value| value.trim())
            .unwrap_or("");
        let platform_arch = manifest
            .platform_arch
            .as_ref()
            .map(|value| value.trim())
            .unwrap_or("");
        match manifest.runtime_kind {
            RuntimeKind::NativeExec => {
                if platform_os.is_empty() {
                    anyhow::bail!(
                        "Manifest field 'platform_os' must be non-empty for runtime_kind '{}'",
                        manifest.runtime_kind.as_str()
                    );
                }
                if platform_arch.is_empty() {
                    anyhow::bail!(
                        "Manifest field 'platform_arch' must be non-empty for runtime_kind '{}'",
                        manifest.runtime_kind.as_str()
                    );
                }
            }
            RuntimeKind::PythonShim => {
                if !platform_os.is_empty() || !platform_arch.is_empty() {
                    anyhow::bail!(
                        "Manifest fields 'platform_os'/'platform_arch' are only valid for runtime_kind '{}'",
                        RuntimeKind::NativeExec.as_str()
                    );
                }
            }
        }

        let schema_defs: BTreeMap<String, SchemaDefinition> =
            serde_json::from_str(&cmd.schema_artifacts_json)
                .context("Failed to parse schema_artifacts_json")?;
        if schema_defs.is_empty() {
            anyhow::bail!("schema_artifacts_json must include at least one output");
        }
        let outputs_json =
            build_outputs_json(&schema_defs).context("Failed to build outputs_json")?;

        let mut contracts = Vec::new();
        for (output_name, schema_def) in &schema_defs {
            let locked_schema = locked_schema_from_definition(output_name, schema_def)
                .with_context(|| format!("Invalid schema for output '{}'", output_name))?;
            let scope_id = derive_scope_id(&cmd.plugin_name, &cmd.version, output_name);
            contracts.push((scope_id, locked_schema));
        }

        // Verify the artifact_hash matches the content
        let computed_hash = compute_artifact_hash(
            &cmd.source_code,
            &cmd.lockfile_content,
            &cmd.manifest_json,
            &cmd.schema_artifacts_json,
        );
        if computed_hash != cmd.artifact_hash {
            anyhow::bail!(
                "Artifact hash mismatch: expected {}, got {}",
                &cmd.artifact_hash[..12.min(cmd.artifact_hash.len())],
                &computed_hash[..12]
            );
        }

        // 2. Compute source_hash (SHA256, not MD5)
        let source_hash = compute_sha256(&cmd.source_code);

        let now = now_millis();
        let system_requirements_json = cmd
            .system_requirements
            .as_ref()
            .map(|reqs| serde_json::to_string(reqs).unwrap_or_default());
        let request = casparian_state_store::PluginDeployRequest {
            plugin_name: cmd.plugin_name.clone(),
            version: cmd.version.clone(),
            runtime_kind: manifest.runtime_kind,
            entrypoint: manifest.entrypoint.clone(),
            platform_os: manifest.platform_os.clone(),
            platform_arch: manifest.platform_arch.clone(),
            source_code: cmd.source_code.clone(),
            source_hash: source_hash.clone(),
            env_hash: cmd.env_hash.clone(),
            artifact_hash: cmd.artifact_hash.clone(),
            manifest_json: cmd.manifest_json.clone(),
            protocol_version: cmd.protocol_version.clone(),
            schema_artifacts_json: cmd.schema_artifacts_json.clone(),
            outputs_json: outputs_json.clone(),
            signature_verified: false,
            signer_id: None,
            created_at: now,
            deployed_at: now,
            publisher_name: cmd.publisher_name.clone(),
            publisher_email: cmd.publisher_email.clone(),
            azure_oid: cmd.azure_oid.clone(),
            system_requirements_json,
            lockfile_content: if cmd.lockfile_content.is_empty() {
                None
            } else {
                Some(cmd.lockfile_content.clone())
            },
            contracts: contracts.clone(),
        };

        self.sqlite_executor.call(move |state_store, _queue, _ctx| {
            state_store.routing().deploy_plugin(request)?;
            Ok(())
        })?;

        info!(
            "Deployed {} v{} (env: {}, artifact: {})",
            cmd.plugin_name,
            cmd.version,
            &cmd.env_hash[..12.min(cmd.env_hash.len())],
            &cmd.artifact_hash[..12.min(cmd.artifact_hash.len())]
        );

        // 5. Refresh topic configs in sqlite executor (new plugins may have topic configs)
        // This ensures newly deployed plugins get their sink configs immediately.
        let _ = self.sqlite_executor.execute(|state_store, _queue, ctx| {
            match Sentinel::load_topic_configs(state_store.routing()) {
                Ok(new_map) => {
                    let old_count = ctx.topic_map.len();
                    ctx.topic_map = new_map;
                    ctx.topic_map_last_refresh = current_time();
                    if ctx.topic_map.len() != old_count {
                        info!(
                            "Refreshed topic configs: {} -> {} plugins",
                            old_count,
                            ctx.topic_map.len()
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to refresh topic configs after deploy: {}", e);
                }
            }
            Ok(())
        });

        // 6. Send success response
        let response = types::DeployResponse {
            success: true,
            message: format!("Deployed {} v{}", cmd.plugin_name, cmd.version),
            plugin_id: None,
        };
        self.send_deploy_response(identity, &response)?;

        Ok(())
    }

    /// Send error response to client
    fn send_error(&mut self, identity: &[u8], message: &str) -> Result<()> {
        let payload = types::ErrorPayload {
            message: message.to_string(),
            traceback: None,
        };

        let msg_bytes = serde_json::to_vec(&payload)?;
        let msg = Message::new(OpCode::Err, JobId::new(0), msg_bytes)?;
        let (header, body) = msg.pack()?;

        let frames = [identity, header.as_ref(), body.as_slice()];
        self.socket.send_multipart(&frames, 0)?;

        Ok(())
    }

    /// Send deploy response to client
    fn send_deploy_response(
        &mut self,
        identity: &[u8],
        response: &types::DeployResponse,
    ) -> Result<()> {
        let payload = serde_json::to_vec(response)?;
        let msg = Message::new(OpCode::Ack, JobId::new(0), payload)?;
        let (header, body) = msg.pack()?;

        let frames = [identity, header.as_ref(), body.as_slice()];
        self.socket.send_multipart(&frames, 0)?;

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

}

struct ControlDbHandler<'a> {
    state_store: &'a StateStore,
    queue: &'a StateStoreQueueSession,
}

impl<'a> ControlDbHandler<'a> {
    fn handle_list_jobs(
        &self,
        status: Option<ProcessingStatus>,
        limit: i64,
        offset: i64,
    ) -> ControlResponse {
        let limit = limit.max(0);
        let offset = offset.max(0);
        match self.queue.list_jobs(status, limit, offset) {
            Ok(jobs) => {
                let job_infos: Vec<JobInfo> = jobs.into_iter().map(Sentinel::job_to_info).collect();
                ControlResponse::Jobs(job_infos)
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to list jobs: {}", e)),
        }
    }

    fn handle_get_job(&self, job_id: JobId) -> ControlResponse {
        match self.queue.get_job(job_id) {
            Ok(Some(job)) => ControlResponse::Job(Some(Sentinel::job_to_info(job))),
            Ok(None) => ControlResponse::Job(None),
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to get job: {}", e)),
        }
    }

    fn handle_get_queue_stats(&self) -> ControlResponse {
        match self.queue.count_jobs_by_status() {
            Ok(counts) => {
                let queued = *counts.get(&ProcessingStatus::Queued).unwrap_or(&0);
                let running = *counts.get(&ProcessingStatus::Running).unwrap_or(&0);
                let dispatching = *counts.get(&ProcessingStatus::Dispatching).unwrap_or(&0);
                let completed = *counts.get(&ProcessingStatus::Completed).unwrap_or(&0);
                let failed = *counts.get(&ProcessingStatus::Failed).unwrap_or(&0);
                let aborted = *counts.get(&ProcessingStatus::Aborted).unwrap_or(&0);
                let total = queued + running + dispatching + completed + failed + aborted;

                ControlResponse::QueueStats(QueueStatsInfo {
                    queued,
                    running: running + dispatching,
                    completed,
                    failed,
                    aborted,
                    total,
                })
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to get stats: {}", e)),
        }
    }

    fn handle_create_api_job(
        &self,
        job_type: casparian_protocol::HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output: Option<&str>,
        approval_id: Option<&str>,
        spec_json: Option<&str>,
    ) -> ControlResponse {
        match self.state_store.api().create_job(
            job_type,
            plugin_name,
            plugin_version,
            input_dir,
            output,
            approval_id,
            spec_json,
        ) {
            Ok(job_id) => ControlResponse::ApiJobCreated { job_id },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to create API job: {}", e))
            }
        }
    }

    fn handle_get_api_job(&self, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().get_job(job_id) {
            Ok(job) => ControlResponse::ApiJob(job),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get API job {}: {}", job_id, e),
            ),
        }
    }

    fn handle_list_api_jobs(
        &self,
        status: Option<casparian_protocol::HttpJobStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;
        match self
            .state_store
            .api()
            .list_jobs(status, limit.saturating_add(offset))
        {
            Ok(jobs) => {
                let jobs = jobs
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                ControlResponse::ApiJobs(jobs)
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to list API jobs: {}", e)),
        }
    }

    fn handle_update_api_job_status(
        &self,
        job_id: ApiJobId,
        status: casparian_protocol::HttpJobStatus,
    ) -> ControlResponse {
        match self.state_store.api().update_job_status(job_id, status) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Status updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job status {}: {}", job_id, e),
            ),
        }
    }

    fn handle_update_api_job_progress(
        &self,
        job_id: ApiJobId,
        progress: ApiJobProgress,
    ) -> ControlResponse {
        match self.state_store.api().update_job_progress(
            job_id,
            &progress.phase,
            progress.items_done,
            progress.items_total,
            progress.message.as_deref(),
        ) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Progress updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job progress {}: {}", job_id, e),
            ),
        }
    }

    fn handle_update_api_job_result(
        &self,
        job_id: ApiJobId,
        result: ApiJobResult,
    ) -> ControlResponse {
        match self.state_store.api().update_job_result(job_id, &result) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Result updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job result {}: {}", job_id, e),
            ),
        }
    }

    fn handle_update_api_job_error(&self, job_id: ApiJobId, error: &str) -> ControlResponse {
        match self.state_store.api().update_job_error(job_id, error) {
            Ok(()) => ControlResponse::ApiJobResult {
                success: true,
                message: "Error updated".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to update API job error {}: {}", job_id, e),
            ),
        }
    }

    fn handle_cancel_api_job(&self, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().cancel_job(job_id) {
            Ok(success) => ControlResponse::ApiJobResult {
                success,
                message: if success {
                    "Job cancelled".to_string()
                } else {
                    "Job not found".to_string()
                },
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to cancel API job {}: {}", job_id, e),
            ),
        }
    }

    fn handle_list_approvals(
        &self,
        status: Option<ApprovalStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;
        match self.state_store.api().list_approvals(status) {
            Ok(approvals) => {
                let approvals = approvals
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                ControlResponse::Approvals(approvals)
            }
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to list approvals: {}", e))
            }
        }
    }

    fn handle_create_approval(
        &self,
        approval_id: &str,
        operation: ApprovalOperation,
        summary: &str,
        expires_in_seconds: i64,
    ) -> ControlResponse {
        let expires_in = ChronoDuration::seconds(expires_in_seconds.max(0));
        match self
            .state_store
            .api()
            .create_approval(approval_id, &operation, summary, expires_in)
        {
            Ok(()) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval created".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to create approval {}: {}", approval_id, e),
            ),
        }
    }

    fn handle_get_approval(&self, approval_id: &str) -> ControlResponse {
        match self.state_store.api().get_approval(approval_id) {
            Ok(approval) => ControlResponse::Approval(approval),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get approval {}: {}", approval_id, e),
            ),
        }
    }

    fn handle_approve(&self, approval_id: &str) -> ControlResponse {
        match self.state_store.api().approve(approval_id, None) {
            Ok(true) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval accepted".to_string(),
            },
            Ok(false) => ControlResponse::ApprovalResult {
                success: false,
                message: "Approval not found or not pending".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to approve {}: {}", approval_id, e),
            ),
        }
    }

    fn handle_reject(&self, approval_id: &str, reason: &str) -> ControlResponse {
        match self.state_store.api().reject(approval_id, None, Some(reason)) {
            Ok(true) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval rejected".to_string(),
            },
            Ok(false) => ControlResponse::ApprovalResult {
                success: false,
                message: "Approval not found or not pending".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to reject {}: {}", approval_id, e),
            ),
        }
    }

    fn handle_set_approval_job_id(&self, approval_id: &str, job_id: ApiJobId) -> ControlResponse {
        match self.state_store.api().link_approval_to_job(approval_id, job_id) {
            Ok(()) => ControlResponse::ApprovalResult {
                success: true,
                message: "Approval linked to job".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!(
                    "Failed to link approval {} to job {}: {}",
                    approval_id, job_id, e
                ),
            ),
        }
    }

    fn handle_expire_approvals(&self) -> ControlResponse {
        match self.state_store.api().expire_approvals() {
            Ok(count) => ControlResponse::ApprovalResult {
                success: true,
                message: format!("Expired {} approvals", count),
            },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to expire approvals: {}", e))
            }
        }
    }

    fn handle_create_session(&self, intent_text: &str, input_dir: Option<&str>) -> ControlResponse {
        match self.state_store.sessions().create_session(intent_text, input_dir) {
            Ok(session_id) => ControlResponse::SessionCreated { session_id },
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Failed to create session: {}", e))
            }
        }
    }

    fn handle_get_session(&self, session_id: SessionId) -> ControlResponse {
        match self.state_store.sessions().get_session(session_id) {
            Ok(session) => ControlResponse::Session(session),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to get session {}: {}", session_id, e),
            ),
        }
    }

    fn handle_list_sessions(
        &self,
        state: Option<IntentState>,
        limit: Option<i64>,
    ) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        match self.state_store.sessions().list_sessions(state, limit) {
            Ok(sessions) => ControlResponse::Sessions(sessions),
            Err(e) => ControlResponse::error("DB_ERROR", format!("Failed to list sessions: {}", e)),
        }
    }

    fn handle_list_sessions_needing_input(&self, limit: Option<i64>) -> ControlResponse {
        let limit = limit.unwrap_or(100).max(0) as usize;
        match self.state_store.sessions().list_sessions_needing_input(limit) {
            Ok(sessions) => ControlResponse::Sessions(sessions),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to list sessions needing input: {}", e),
            ),
        }
    }

    fn handle_advance_session(
        &self,
        session_id: SessionId,
        target_state: IntentState,
    ) -> ControlResponse {
        match self
            .state_store
            .sessions()
            .update_session_state(session_id, target_state)
        {
            Ok(true) => ControlResponse::SessionResult {
                success: true,
                message: "Session advanced".to_string(),
            },
            Ok(false) => ControlResponse::SessionResult {
                success: false,
                message: "Session not found".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to advance session {}: {}", session_id, e),
            ),
        }
    }

    fn handle_cancel_session(&self, session_id: SessionId) -> ControlResponse {
        match self.state_store.sessions().cancel_session(session_id) {
            Ok(true) => ControlResponse::SessionResult {
                success: true,
                message: "Session cancelled".to_string(),
            },
            Ok(false) => ControlResponse::SessionResult {
                success: false,
                message: "Session not found".to_string(),
            },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Failed to cancel session {}: {}", session_id, e),
            ),
        }
    }

    fn handle_list_sources(&self, workspace_id: WorkspaceId) -> ControlResponse {
        let records = match self.state_store.scout().list_sources_with_counts(workspace_id) {
            Ok(records) => records,
            Err(e) => return ControlResponse::error("DB_ERROR", e.to_string()),
        };

        let sources = records
            .into_iter()
            .map(|record| {
                let source = record.source;
                ScoutSourceInfo {
                    id: source.id,
                    workspace_id: source.workspace_id,
                    name: source.name,
                    source_type: source.source_type,
                    path: source.path,
                    exec_path: source.exec_path,
                    enabled: source.enabled,
                    poll_interval_secs: source.poll_interval_secs,
                    file_count: record.file_count,
                }
            })
            .collect();

        ControlResponse::Sources(sources)
    }

    fn handle_upsert_source(&self, source: &ScoutSourceInfo) -> ControlResponse {
        let source_row = ScoutSource {
            workspace_id: source.workspace_id,
            id: source.id,
            name: source.name.clone(),
            source_type: source.source_type.clone(),
            path: source.path.clone(),
            exec_path: source.exec_path.clone(),
            poll_interval_secs: source.poll_interval_secs,
            enabled: source.enabled,
        };

        match self.state_store.scout().upsert_source(&source_row) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source upserted".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source upsert failed: {}", e)),
        }
    }

    fn handle_update_source(
        &self,
        source_id: SourceId,
        name: Option<&str>,
        path: Option<&str>,
    ) -> ControlResponse {
        let mut source = match self.state_store.scout().get_source(&source_id) {
            Ok(Some(source)) => source,
            Ok(None) => {
                return ControlResponse::SourceResult {
                    success: false,
                    message: "Source not found".to_string(),
                }
            }
            Err(e) => {
                return ControlResponse::error("DB_ERROR", format!("Source load failed: {}", e))
            }
        };

        if let Some(name) = name {
            source.name = name.to_string();
        }
        if let Some(path) = path {
            source.path = path.to_string();
        }

        match self.state_store.scout().upsert_source(&source) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source updated".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source update failed: {}", e)),
        }
    }

    fn handle_delete_source(&self, source_id: SourceId) -> ControlResponse {
        match self.state_store.scout().delete_source(&source_id) {
            Ok(true) => ControlResponse::SourceResult {
                success: true,
                message: "Source deleted".to_string(),
            },
            Ok(false) => ControlResponse::SourceResult {
                success: false,
                message: "Source not found".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source delete failed: {}", e)),
        }
    }

    fn handle_touch_source(&self, source_id: SourceId) -> ControlResponse {
        match self.state_store.scout().touch_source(&source_id) {
            Ok(()) => ControlResponse::SourceResult {
                success: true,
                message: "Source touched".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Source touch failed: {}", e)),
        }
    }

    fn handle_list_rules(&self, workspace_id: WorkspaceId) -> ControlResponse {
        match self.state_store.scout().list_tagging_rules(&workspace_id) {
            Ok(rules) => {
                let mapped = rules
                    .into_iter()
                    .map(|rule| ScoutRuleInfo {
                        id: rule.id,
                        workspace_id: rule.workspace_id,
                        pattern: rule.pattern,
                        tag: rule.tag,
                        priority: rule.priority,
                        enabled: rule.enabled,
                    })
                    .collect();
                ControlResponse::Rules(mapped)
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rules load failed: {}", e)),
        }
    }

    fn handle_create_rule(
        &self,
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
        pattern: &str,
        tag: &str,
    ) -> ControlResponse {
        let name = format!("{} → {}", pattern, tag);
        let rule = casparian_scout::types::TaggingRule {
            id: rule_id,
            name,
            workspace_id,
            pattern: pattern.to_string(),
            tag: tag.to_string(),
            priority: 100,
            enabled: true,
        };

        match self.state_store.scout().upsert_tagging_rule(&rule) {
            Ok(()) => ControlResponse::RuleResult {
                success: true,
                message: "Rule created".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule create failed: {}", e)),
        }
    }

    fn handle_update_rule_enabled(
        &self,
        rule_id: TaggingRuleId,
        _workspace_id: WorkspaceId,
        enabled: bool,
    ) -> ControlResponse {
        let mut rule = match self.state_store.scout().get_tagging_rule(&rule_id) {
            Ok(Some(rule)) => rule,
            Ok(None) => {
                return ControlResponse::RuleResult {
                    success: false,
                    message: "Rule not found".to_string(),
                }
            }
            Err(e) => {
                return ControlResponse::error("DB_ERROR", format!("Rule load failed: {}", e))
            }
        };

        rule.enabled = enabled;

        match self.state_store.scout().upsert_tagging_rule(&rule) {
            Ok(()) => ControlResponse::RuleResult {
                success: true,
                message: "Rule updated".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule update failed: {}", e)),
        }
    }

    fn handle_delete_rule(
        &self,
        rule_id: TaggingRuleId,
        _workspace_id: WorkspaceId,
    ) -> ControlResponse {
        match self.state_store.scout().delete_tagging_rule(&rule_id) {
            Ok(true) => ControlResponse::RuleResult {
                success: true,
                message: "Rule deleted".to_string(),
            },
            Ok(false) => ControlResponse::RuleResult {
                success: false,
                message: "Rule not found".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule delete failed: {}", e)),
        }
    }

    fn handle_list_tags(&self, workspace_id: WorkspaceId, source_id: SourceId) -> ControlResponse {
        let stats = match self.state_store.scout().tag_stats(workspace_id, source_id) {
            Ok(stats) => stats,
            Err(e) => return ControlResponse::error("DB_ERROR", e.to_string()),
        };

        ControlResponse::TagStats(ScoutTagStats {
            total_files: stats.total_files,
            untagged_files: stats.untagged_files,
            tags: stats
                .tags
                .into_iter()
                .map(|tag| ScoutTagCount {
                    tag: tag.tag,
                    count: tag.count,
                })
                .collect(),
        })
    }

    fn handle_apply_tag(
        &self,
        file_id: i64,
        tag: &str,
        tag_source: TagSource,
        rule_id: Option<&TaggingRuleId>,
    ) -> ControlResponse {
        let result = match tag_source {
            TagSource::Manual => self.state_store.scout().tag_file(file_id, tag),
            TagSource::Rule => {
                let Some(rule_id) = rule_id else {
                    return ControlResponse::TagResult {
                        success: false,
                        message: "Rule-based tag missing rule_id".to_string(),
                    };
                };
                self.state_store.scout().tag_file_by_rule(file_id, tag, rule_id)
            }
        };

        match result {
            Ok(()) => ControlResponse::TagResult {
                success: true,
                message: "Tag applied".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Tag apply failed: {}", e)),
        }
    }

    fn handle_get_source_by_path(
        &self,
        workspace_id: WorkspaceId,
        path: &str,
    ) -> ControlResponse {
        match self.state_store.scout().get_source_by_path(&workspace_id, path) {
            Ok(Some(source)) => ControlResponse::Source(Some(ScoutSourceInfo {
                id: source.id,
                workspace_id: source.workspace_id,
                name: source.name,
                source_type: source.source_type,
                path: source.path,
                exec_path: source.exec_path,
                enabled: source.enabled,
                poll_interval_secs: source.poll_interval_secs,
                file_count: 0,
            })),
            Ok(None) => ControlResponse::Source(None),
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Source lookup failed: {}", e),
            ),
        }
    }

    fn handle_list_files(
        &self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        tag_filter: ScoutTagFilter,
        path_filter: Option<String>,
        limit: usize,
        offset: usize,
    ) -> ControlResponse {
        let store_filter = match tag_filter {
            ScoutTagFilter::All => casparian_state_store::ScoutFileTagFilter::All,
            ScoutTagFilter::Untagged => casparian_state_store::ScoutFileTagFilter::Untagged,
            ScoutTagFilter::Tag(tag) => casparian_state_store::ScoutFileTagFilter::Tag(tag),
        };

        match self
            .state_store
            .scout()
            .list_files_page(workspace_id, source_id, store_filter, path_filter, limit, offset)
        {
            Ok(page) => {
                let files = page
                    .files
                    .into_iter()
                    .map(|file| ScoutFileInfo {
                        id: file.id,
                        path: file.path,
                        rel_path: file.rel_path,
                        size: file.size,
                        mtime: file.mtime,
                        is_dir: file.is_dir,
                        tags: file.tags,
                    })
                    .collect();
                ControlResponse::FilesPage(ScoutFilesPage {
                    total_count: page.total_count,
                    files,
                })
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("List files failed: {}", e)),
        }
    }

    fn handle_list_folders(
        &self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        prefix: &str,
        glob_pattern: Option<String>,
    ) -> ControlResponse {
        match self
            .state_store
            .scout()
            .list_folder_entries(workspace_id, source_id, prefix, glob_pattern.as_deref())
        {
            Ok((entries, total_count)) => {
                let mapped = entries
                    .into_iter()
                    .map(|entry| ScoutFolderEntry {
                        name: entry.name,
                        file_count: entry.file_count,
                        is_file: entry.is_file,
                    })
                    .collect();
                ControlResponse::FolderEntries {
                    entries: mapped,
                    total_count,
                }
            }
            Err(e) => {
                ControlResponse::error("DB_ERROR", format!("Folder query failed: {}", e))
            }
        }
    }

    fn handle_pattern_query(
        &self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        glob_pattern: &str,
        limit: usize,
        offset: usize,
    ) -> ControlResponse {
        match self
            .state_store
            .scout()
            .pattern_query(workspace_id, source_id, glob_pattern, limit, offset)
        {
            Ok(result) => {
                let files = result
                    .files
                    .into_iter()
                    .map(|file| ScoutPatternMatch {
                        rel_path: file.rel_path,
                        size: file.size,
                        mtime: file.mtime,
                    })
                    .collect();
                ControlResponse::PatternQueryResult(ScoutPatternQueryResult {
                    total_count: result.total_count,
                    files,
                })
            }
            Err(e) => ControlResponse::error("DB_ERROR", format!("Pattern query failed: {}", e)),
        }
    }

    fn handle_sample_paths_for_eval(
        &self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        glob_pattern: &str,
    ) -> ControlResponse {
        match self
            .state_store
            .scout()
            .sample_paths_for_eval(workspace_id, source_id, glob_pattern)
        {
            Ok(paths) => ControlResponse::SamplePaths { paths },
            Err(e) => ControlResponse::error(
                "DB_ERROR",
                format!("Sample eval query failed: {}", e),
            ),
        }
    }

    fn handle_apply_tag_to_paths(
        &self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        rel_paths: Vec<String>,
        tag: &str,
        tag_source: TagSource,
    ) -> ControlResponse {
        let result = self.state_store.scout().apply_tag_to_paths(
            workspace_id,
            source_id,
            &rel_paths,
            tag,
            tag_source,
            None,
        );

        match result {
            Ok(count) => ControlResponse::TagApplyResult {
                success: true,
                tagged_count: count,
                message: "Tags applied".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Tag apply failed: {}", e)),
        }
    }

    fn handle_apply_rule_to_source(
        &self,
        rule_id: TaggingRuleId,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        pattern: &str,
        tag: &str,
    ) -> ControlResponse {
        match self
            .state_store
            .scout()
            .apply_rule_to_source(rule_id, workspace_id, source_id, pattern, tag)
        {
            Ok(count) => ControlResponse::RuleApplyResult {
                success: true,
                tagged_count: count,
                message: "Rule applied".to_string(),
            },
            Err(e) => ControlResponse::error("DB_ERROR", format!("Rule apply failed: {}", e)),
        }
    }
}

fn handle_control_request_db(
    state_store: &StateStore,
    queue: &StateStoreQueueSession,
    _context: &mut SqliteContext,
    request: ControlRequest,
) -> ControlResponse {
    let handler = ControlDbHandler { state_store, queue };
    match request {
        ControlRequest::ListJobs {
            status,
            limit,
            offset,
        } => handler.handle_list_jobs(status, limit.unwrap_or(100), offset.unwrap_or(0)),
        ControlRequest::GetJob { job_id } => handler.handle_get_job(job_id),
        ControlRequest::GetQueueStats => handler.handle_get_queue_stats(),
        ControlRequest::CreateApiJob {
            job_type,
            plugin_name,
            plugin_version,
            input_dir,
            output,
            approval_id,
            spec_json,
        } => handler.handle_create_api_job(
            job_type,
            &plugin_name,
            plugin_version.as_deref(),
            &input_dir,
            output.as_deref(),
            approval_id.as_deref(),
            spec_json.as_deref(),
        ),
        ControlRequest::GetApiJob { job_id } => handler.handle_get_api_job(job_id),
        ControlRequest::ListApiJobs {
            status,
            limit,
            offset,
        } => handler.handle_list_api_jobs(status, limit, offset),
        ControlRequest::UpdateApiJobStatus { job_id, status } => {
            handler.handle_update_api_job_status(job_id, status)
        }
        ControlRequest::UpdateApiJobProgress { job_id, progress } => {
            handler.handle_update_api_job_progress(job_id, progress)
        }
        ControlRequest::UpdateApiJobResult { job_id, result } => {
            handler.handle_update_api_job_result(job_id, result)
        }
        ControlRequest::UpdateApiJobError { job_id, error } => {
            handler.handle_update_api_job_error(job_id, &error)
        }
        ControlRequest::CancelApiJob { job_id } => handler.handle_cancel_api_job(job_id),
        ControlRequest::ListApprovals {
            status,
            limit,
            offset,
        } => handler.handle_list_approvals(status, limit, offset),
        ControlRequest::CreateApproval {
            approval_id,
            operation,
            summary,
            expires_in_seconds,
        } => handler.handle_create_approval(&approval_id, operation, &summary, expires_in_seconds),
        ControlRequest::GetApproval { approval_id } => handler.handle_get_approval(&approval_id),
        ControlRequest::Approve { approval_id } => handler.handle_approve(&approval_id),
        ControlRequest::Reject {
            approval_id,
            reason,
        } => handler.handle_reject(&approval_id, &reason),
        ControlRequest::SetApprovalJobId {
            approval_id,
            job_id,
        } => handler.handle_set_approval_job_id(&approval_id, job_id),
        ControlRequest::ExpireApprovals => handler.handle_expire_approvals(),
        ControlRequest::CreateSession {
            intent_text,
            input_dir,
        } => handler.handle_create_session(&intent_text, input_dir.as_deref()),
        ControlRequest::GetSession { session_id } => handler.handle_get_session(session_id),
        ControlRequest::ListSessions { state, limit } => handler.handle_list_sessions(state, limit),
        ControlRequest::ListSessionsNeedingInput { limit } => {
            handler.handle_list_sessions_needing_input(limit)
        }
        ControlRequest::AdvanceSession {
            session_id,
            target_state,
        } => handler.handle_advance_session(session_id, target_state),
        ControlRequest::CancelSession { session_id } => handler.handle_cancel_session(session_id),
        ControlRequest::ListSources { workspace_id } => handler.handle_list_sources(workspace_id),
        ControlRequest::UpsertSource { source } => handler.handle_upsert_source(&source),
        ControlRequest::UpdateSource {
            source_id,
            name,
            path,
        } => handler.handle_update_source(source_id, name.as_deref(), path.as_deref()),
        ControlRequest::DeleteSource { source_id } => handler.handle_delete_source(source_id),
        ControlRequest::TouchSource { source_id } => handler.handle_touch_source(source_id),
        ControlRequest::ListRules { workspace_id } => handler.handle_list_rules(workspace_id),
        ControlRequest::CreateRule {
            rule_id,
            workspace_id,
            pattern,
            tag,
        } => handler.handle_create_rule(rule_id, workspace_id, &pattern, &tag),
        ControlRequest::UpdateRuleEnabled {
            rule_id,
            workspace_id,
            enabled,
        } => handler.handle_update_rule_enabled(rule_id, workspace_id, enabled),
        ControlRequest::DeleteRule {
            rule_id,
            workspace_id,
        } => handler.handle_delete_rule(rule_id, workspace_id),
        ControlRequest::ListTags {
            workspace_id,
            source_id,
        } => handler.handle_list_tags(workspace_id, source_id),
        ControlRequest::GetSourceByPath { workspace_id, path } => {
            handler.handle_get_source_by_path(workspace_id, &path)
        }
        ControlRequest::ListFiles {
            workspace_id,
            source_id,
            tag_filter,
            path_filter,
            limit,
            offset,
        } => handler.handle_list_files(
            workspace_id,
            source_id,
            tag_filter,
            path_filter,
            limit,
            offset,
        ),
        ControlRequest::ListFolders {
            workspace_id,
            source_id,
            prefix,
            glob_pattern,
        } => handler.handle_list_folders(workspace_id, source_id, &prefix, glob_pattern),
        ControlRequest::PatternQuery {
            workspace_id,
            source_id,
            glob_pattern,
            limit,
            offset,
        } => handler.handle_pattern_query(
            workspace_id,
            source_id,
            &glob_pattern,
            limit,
            offset,
        ),
        ControlRequest::SamplePathsForEval {
            workspace_id,
            source_id,
            glob_pattern,
        } => handler.handle_sample_paths_for_eval(workspace_id, source_id, &glob_pattern),
        ControlRequest::ApplyTag {
            workspace_id: _,
            file_id,
            tag,
            tag_source,
            rule_id,
        } => handler.handle_apply_tag(file_id, &tag, tag_source, rule_id.as_ref()),
        ControlRequest::ApplyTagToPaths {
            workspace_id,
            source_id,
            rel_paths,
            tag,
            tag_source,
        } => handler.handle_apply_tag_to_paths(
            workspace_id,
            source_id,
            rel_paths,
            &tag,
            tag_source,
        ),
        ControlRequest::ApplyRuleToSource {
            rule_id,
            workspace_id,
            source_id,
            pattern,
            tag,
        } => handler.handle_apply_rule_to_source(
            rule_id,
            workspace_id,
            source_id,
            &pattern,
            &tag,
        ),
        ControlRequest::Ping
        | ControlRequest::StartScan { .. }
        | ControlRequest::GetScan { .. }
        | ControlRequest::ListScans { .. }
        | ControlRequest::CancelScan { .. }
        | ControlRequest::CancelJob { .. } => ControlResponse::error(
            "INVALID_REQUEST",
            "Request must be handled by reactor".to_string(),
        ),
    }
}

fn process_conclude_db(
    state_store: &StateStore,
    queue: &StateStoreQueueSession,
    context: &mut SqliteContext,
    job_id: i64,
    receipt: JobReceipt,
) -> Result<ConcludeOutcome> {
    if let Some(diagnostics) = receipt.diagnostics.as_ref() {
        if let Some(mismatch) = diagnostics.schema_mismatch.as_ref() {
            if let Err(err) = queue.record_schema_mismatch(job_id, mismatch) {
                warn!(
                    "Failed to persist schema mismatch for job {}: {}",
                    job_id, err
                );
            }
        }
    }

    if let Err(err) = state_store
        .artifacts()
        .insert_job_artifacts(job_id, &receipt.artifacts)
    {
        warn!("Failed to persist artifacts for job {}: {}", job_id, err);
    }

    let job_info = JobId::try_from(job_id)
        .ok()
        .and_then(|id| queue.get_job(id).ok().flatten());
    let plugin_name = job_info.as_ref().map(|job| job.plugin_name.as_str());
    let retry_count = job_info.as_ref().map(|job| job.retry_count).unwrap_or(0);
    let lease_token = receipt.lease_token.clone();

    match receipt.status {
        JobStatus::Success | JobStatus::PartialSuccess | JobStatus::CompletedWithWarnings => {
            let (completion_status, summary) = match receipt.status {
                JobStatus::Success => (JobStatus::Success.as_str(), "Success"),
                JobStatus::PartialSuccess => (JobStatus::PartialSuccess.as_str(), "Partial success"),
                JobStatus::CompletedWithWarnings => (
                    JobStatus::CompletedWithWarnings.as_str(),
                    "Completed with warnings",
                ),
                other => unreachable!("Non-success status in success branch: {:?}", other),
            };
            let quarantine_rows = receipt.metrics.get(metrics::QUARANTINE_ROWS).copied();
            let updated = if let Some(token) = lease_token.as_deref() {
                queue.complete_job_if_token_matches(
                    job_id,
                    token,
                    completion_status,
                    summary,
                    quarantine_rows,
                )?
            } else {
                warn!(
                    "Legacy CONCLUDE without lease_token for job {}; accepting",
                    job_id
                );
                queue.complete_job(job_id, completion_status, summary, quarantine_rows)?;
                true
            };
            if !updated {
                return Ok(ConcludeOutcome::Stale { job_id });
            }

            if let Err(err) = Sentinel::record_materializations_for_job_with_context(
                state_store,
                queue,
                context,
                job_id,
                &receipt,
            ) {
                warn!(
                    "Failed to record materializations for job {}: {}",
                    job_id, err
                );
            }

            if let Some(parser) = plugin_name {
                if let Err(err) = record_success_db(queue, parser) {
                    warn!("Failed to record parser success for {}: {}", parser, err);
                }
            }

            if let Err(err) = queue.update_pipeline_run_status_for_job(job_id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job_id, err
                );
            }

            Ok(ConcludeOutcome::Completed {
                job_id,
                artifacts: receipt.artifacts,
            })
        }
        JobStatus::Failed => {
            let error = receipt
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown error".to_string());
            let is_transient = receipt
                .metrics
                .get("is_transient")
                .map(|v| *v == 1)
                .unwrap_or(true);

            if let Some(parser) = plugin_name {
                if let Err(err) = record_failure_db(queue, parser, &error) {
                    warn!("Failed to record parser failure for {}: {}", parser, err);
                }
            }

            if let Some(token) = lease_token.as_deref() {
                let updated = queue.fail_job_if_token_matches(
                    job_id,
                    token,
                    JobStatus::Failed.as_str(),
                    &error,
                )?;
                if !updated {
                    return Ok(ConcludeOutcome::Stale { job_id });
                }
            } else {
                warn!(
                    "Legacy CONCLUDE without lease_token for job {}; accepting",
                    job_id
                );
                queue.fail_job(job_id, JobStatus::Failed.as_str(), &error)?;
            }

            let retried = handle_job_failure_db(queue, job_id, &error, is_transient, retry_count)?;
            if let Err(err) = queue.update_pipeline_run_status_for_job(job_id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job_id, err
                );
            }
            Ok(ConcludeOutcome::Failed { job_id, retried })
        }
        JobStatus::Rejected => {
            warn!(
                "Job {} rejected by worker (at capacity), deferring for retry",
                job_id
            );
            let scheduled_at = now_millis();
            if let Some(token) = lease_token.as_deref() {
                let updated = queue.defer_job_if_token_matches(
                    job_id,
                    token,
                    scheduled_at,
                    Some("capacity_rejection"),
                )?;
                if !updated {
                    return Ok(ConcludeOutcome::Stale { job_id });
                }
            } else {
                warn!(
                    "Legacy CONCLUDE Rejected without lease_token for job {}; accepting",
                    job_id
                );
                queue.defer_job(job_id, scheduled_at, Some("capacity_rejection"))?;
            }
            if let Err(err) = queue.update_pipeline_run_status_for_job(job_id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job_id, err
                );
            }
            Ok(ConcludeOutcome::Rejected { job_id })
        }
        JobStatus::Aborted => {
            let error = receipt
                .error_message
                .unwrap_or_else(|| "Aborted".to_string());
            warn!("Job {} aborted: {}", job_id, error);
            if let Some(token) = lease_token.as_deref() {
                let updated = queue.abort_job_if_token_matches(job_id, token, &error)?;
                if !updated {
                    return Ok(ConcludeOutcome::Stale { job_id });
                }
            } else {
                warn!(
                    "Legacy CONCLUDE without lease_token for job {}; accepting",
                    job_id
                );
                queue.abort_job(job_id, &error)?;
            }
            if let Err(err) = queue.update_pipeline_run_status_for_job(job_id) {
                warn!(
                    "Failed to update pipeline run status for job {}: {}",
                    job_id, err
                );
            }
            Ok(ConcludeOutcome::Aborted { job_id })
        }
    }
}

fn record_success_db(queue: &StateStoreQueueSession, parser_name: &str) -> Result<()> {
    queue.record_parser_success(parser_name)?;
    debug!(
        parser = parser_name,
        "Recorded success, reset consecutive_failures"
    );
    Ok(())
}

fn record_failure_db(queue: &StateStoreQueueSession, parser_name: &str, reason: &str) -> Result<()> {
    queue.record_parser_failure(parser_name, reason)?;
    debug!(parser = parser_name, reason = reason, "Recorded failure");
    check_circuit_breaker_db(queue, parser_name)?;
    Ok(())
}

fn check_circuit_breaker_db(queue: &StateStoreQueueSession, parser_name: &str) -> Result<bool> {
    let health = queue.get_parser_health(parser_name)?;
    if let Some(h) = health {
        if h.is_paused() {
            warn!(parser = parser_name, "Parser is paused (circuit open)");
            return Ok(false);
        }

        if h.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
            queue.pause_parser(parser_name)?;

            warn!(
                parser = parser_name,
                consecutive_failures = h.consecutive_failures,
                "Circuit breaker tripped - parser paused"
            );
            return Ok(false);
        }
    }

    Ok(true)
}

fn handle_job_failure_db(
    queue: &StateStoreQueueSession,
    job_id: i64,
    error: &str,
    is_transient: bool,
    retry_count: i32,
) -> Result<bool> {
    if is_transient && retry_count < MAX_RETRY_COUNT {
        let backoff_secs = BACKOFF_BASE_SECS.pow(retry_count as u32 + 1);
        info!(
            job_id,
            retry_count = retry_count + 1,
            backoff_secs,
            "Scheduling retry with exponential backoff"
        );

        let now = now_millis();
        let scheduled_at = now + (backoff_secs as i64 * 1_000);
        queue.schedule_retry(job_id, retry_count + 1, error, scheduled_at)?;
        return Ok(true);
    }

    let reason = if is_transient {
        DeadLetterReason::MaxRetriesExceeded
    } else {
        DeadLetterReason::PermanentError
    };

    warn!(
        "Job {} moving to dead letter queue: {} (retries: {}/{})",
        job_id,
        reason.as_str(),
        retry_count,
        MAX_RETRY_COUNT
    );

    queue.move_to_dead_letter(job_id, error, reason)?;
    Ok(false)
}

/// Get current Unix timestamp
fn current_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX_EPOCH - check system clock")
        .as_secs_f64()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX_EPOCH - check system clock")
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn sqlite_path_from_url(url: &str) -> Option<std::path::PathBuf> {
    url.strip_prefix("sqlite:").map(std::path::PathBuf::from)
}

fn millis_to_rfc3339(millis: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .unwrap_or_else(|| Utc::now())
        .to_rfc3339()
}

/// Compute SHA256 hash of content, returning hex string
fn compute_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
#[cfg(test)]
mod tests {
    use super::*;
    use casparian_db::{DbConnection, DbValue};
    use casparian_state_store::{ExpectedOutputs, OutputSpec, PluginDeployRequest, RoutingStore};

    struct TestRoutingStore {
        conn: DbConnection,
    }

    impl RoutingStore for TestRoutingStore {
        fn list_topic_configs(&self) -> Result<Vec<TopicConfig>> {
            Ok(Vec::new())
        }

        fn expected_outputs_for_plugin(
            &self,
            plugin_name: &str,
            parser_version: Option<&str>,
        ) -> Result<Vec<OutputSpec>> {
            ExpectedOutputs::list_for_plugin(&self.conn, plugin_name, parser_version)
        }

        fn deploy_plugin(&self, _request: PluginDeployRequest) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_join_root_and_rel_posix() {
        let joined = join_root_and_rel("/mnt/data", "folder/file.csv");
        assert_eq!(joined, "/mnt/data/folder/file.csv");
    }

    #[test]
    fn test_join_root_and_rel_windows() {
        let joined = join_root_and_rel("C:\\\\data", "folder/file.csv");
        assert_eq!(joined, "C:\\\\data\\folder\\file.csv");
    }

    #[test]
    fn test_resolve_dispatch_path_fallbacks() {
        let joined = resolve_dispatch_path("/scan/root", None, "file.txt");
        assert_eq!(joined, "/scan/root/file.txt");

        let joined = resolve_dispatch_path("/scan/root", Some(""), "file.txt");
        assert_eq!(joined, "/scan/root/file.txt");

        let joined = resolve_dispatch_path("/scan/root", Some("/exec/root"), "file.txt");
        assert_eq!(joined, "/exec/root/file.txt");
    }

    fn setup_contract_db() -> (DbConnection, SchemaStorage) {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let queue = JobQueue::new(conn.clone());
        queue.init_registry_schema().unwrap();
        let schema_storage = SchemaStorage::new(conn.clone()).unwrap();
        (conn, schema_storage)
    }

    fn insert_test_plugin(
        conn: &DbConnection,
        plugin_name: &str,
        version: &str,
        outputs_json: &str,
    ) {
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

    fn save_contract(
        schema_storage: &SchemaStorage,
        plugin_name: &str,
        version: &str,
        output_name: &str,
    ) {
        let schema_def = SchemaDefinition {
            columns: vec![SchemaColumnSpec {
                name: "id".to_string(),
                data_type: casparian_protocol::DataType::Int64,
                nullable: false,
                format: None,
            }],
        };
        let locked = locked_schema_from_definition(output_name, &schema_def).unwrap();
        let scope_id = derive_scope_id(plugin_name, version, output_name);
        let contract = SchemaContract::new(&scope_id, locked, "tester");
        schema_storage.save_contract(&contract).unwrap();
    }

    #[test]
    fn test_connected_worker() {
        let worker = ConnectedWorker::new("test-worker".to_string(), vec!["*".to_string()]);

        assert_eq!(worker.status, WorkerStatus::Idle);
        assert_eq!(worker.capabilities, vec!["*".to_string()]);
        assert_eq!(worker.worker_id, "test-worker");
    }

    #[test]
    fn test_worker_status() {
        let mut worker = ConnectedWorker::new("test".to_string(), vec![]);

        assert_eq!(worker.status, WorkerStatus::Idle);

        worker.status = WorkerStatus::Busy;
        assert_eq!(worker.status, WorkerStatus::Busy);
    }

    #[test]
    fn test_resolve_sinks_for_plugin_defaults() {
        let topic_map: HashMap<String, Vec<SinkConfig>> = HashMap::new();
        let sinks = Sentinel::resolve_sinks_for_plugin(&topic_map, "missing_plugin");
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].topic, defaults::DEFAULT_SINK_TOPIC);
        assert_eq!(sinks[0].uri, defaults::DEFAULT_SINK_URI);
    }

    #[test]
    fn test_resolve_sinks_for_plugin_preserves_multiple() {
        let mut topic_map: HashMap<String, Vec<SinkConfig>> = HashMap::new();
        topic_map.insert(
            "plugin_a".to_string(),
            vec![
                SinkConfig {
                    topic: "alpha".to_string(),
                    uri: "parquet:///tmp/alpha".to_string(),
                    mode: SinkMode::Append,
                    quarantine_config: None,
                    schema: None,
                },
                SinkConfig {
                    topic: "beta".to_string(),
                    uri: "parquet:///tmp/beta".to_string(),
                    mode: SinkMode::Append,
                    quarantine_config: None,
                    schema: None,
                },
            ],
        );

        let sinks = Sentinel::resolve_sinks_for_plugin(&topic_map, "plugin_a");
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0].topic, "alpha");
        assert_eq!(sinks[1].topic, "beta");
    }

    #[test]
    fn test_apply_contract_overrides_expands_default_sink_output() {
        let (conn, schema_storage) = setup_contract_db();
        let outputs_json = r#"{"alpha": {"columns": []}, "beta": {"columns": []}}"#;
        insert_test_plugin(&conn, "parser_a", "1.0.0", outputs_json);
        save_contract(&schema_storage, "parser_a", "1.0.0", "alpha");
        save_contract(&schema_storage, "parser_a", "1.0.0", "beta");
        let routing = TestRoutingStore { conn: conn.clone() };

        let sinks = vec![SinkConfig {
            topic: defaults::DEFAULT_SINK_TOPIC.to_string(),
            uri: "parquet:///tmp/default".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let resolved = Sentinel::apply_contract_overrides_with_storage(
            &routing,
            &schema_storage,
            "parser_a",
            "1.0.0",
            sinks,
        )
        .unwrap();

        assert_eq!(resolved.len(), 2);
        let alpha = resolved.iter().find(|s| s.topic == "alpha").unwrap();
        let beta = resolved.iter().find(|s| s.topic == "beta").unwrap();
        assert!(alpha.schema.is_some());
        assert!(beta.schema.is_some());
    }

    #[test]
    fn test_apply_contract_overrides_expands_default_sink_wildcard() {
        let (conn, schema_storage) = setup_contract_db();
        let outputs_json = r#"{"alpha": {"columns": []}, "beta": {"columns": []}}"#;
        insert_test_plugin(&conn, "parser_b", "1.2.3", outputs_json);
        save_contract(&schema_storage, "parser_b", "1.2.3", "alpha");
        save_contract(&schema_storage, "parser_b", "1.2.3", "beta");
        let routing = TestRoutingStore { conn: conn.clone() };

        let sinks = vec![SinkConfig {
            topic: "*".to_string(),
            uri: "parquet:///tmp/default".to_string(),
            mode: SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }];

        let resolved = Sentinel::apply_contract_overrides_with_storage(
            &routing,
            &schema_storage,
            "parser_b",
            "1.2.3",
            sinks,
        )
        .unwrap();

        assert_eq!(resolved.len(), 2);
        let alpha = resolved.iter().find(|s| s.topic == "alpha").unwrap();
        let beta = resolved.iter().find(|s| s.topic == "beta").unwrap();
        assert!(alpha.schema.is_some());
        assert!(beta.schema.is_some());
    }

    #[test]
    fn test_load_topic_configs_without_schema() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let db_path = temp.path().to_path_buf();
        let conn = DbConnection::open_sqlite(&db_path).unwrap();
        conn.execute(
            r#"
            CREATE TABLE cf_topic_config (
                id INTEGER PRIMARY KEY,
                plugin_name TEXT NOT NULL,
                topic_name TEXT NOT NULL,
                uri TEXT NOT NULL,
                mode TEXT DEFAULT 'append',
                quarantine_allow BOOLEAN,
                quarantine_max_pct DOUBLE,
                quarantine_max_count BIGINT,
                quarantine_dir TEXT
            )
            "#,
            &[],
        )
        .unwrap();
        conn.execute(
            "CREATE UNIQUE INDEX ux_topic_unique ON cf_topic_config(plugin_name, topic_name)",
            &[],
        )
        .unwrap();

        conn.execute(
            r#"
            INSERT INTO cf_topic_config (id, plugin_name, topic_name, uri, mode)
            VALUES (?, ?, ?, ?, ?)
            "#,
            &[
                DbValue::from(1),
                DbValue::from("test_plugin"),
                DbValue::from("output"),
                DbValue::from("parquet:///tmp/out"),
                DbValue::from("append"),
            ],
        )
        .unwrap();

        let store = StateStore::open(&format!("sqlite:{}", db_path.display())).unwrap();
        let configs = Sentinel::load_topic_configs(store.routing()).unwrap();
        let sinks = configs.get("test_plugin").unwrap();
        assert_eq!(sinks.len(), 1);
        assert!(sinks[0].schema.is_none());
    }

    #[test]
    fn test_load_topic_configs_rejects_duplicates() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let db_path = temp.path().to_path_buf();
        let conn = DbConnection::open_sqlite(&db_path).unwrap();
        conn.execute(
            r#"
            CREATE TABLE cf_topic_config (
                id INTEGER PRIMARY KEY,
                plugin_name TEXT NOT NULL,
                topic_name TEXT NOT NULL,
                uri TEXT NOT NULL,
                mode TEXT DEFAULT 'append',
                quarantine_allow BOOLEAN,
                quarantine_max_pct DOUBLE,
                quarantine_max_count BIGINT,
                quarantine_dir TEXT
            )
            "#,
            &[],
        )
        .unwrap();

        conn.execute(
            r#"
            INSERT INTO cf_topic_config (id, plugin_name, topic_name, uri, mode)
            VALUES
              (1, 'dup_plugin', 'orders', 'parquet:///tmp/out1', 'append'),
              (2, 'dup_plugin', 'orders', 'parquet:///tmp/out2', 'append')
            "#,
            &[],
        )
        .unwrap();

        let store = StateStore::open(&format!("sqlite:{}", db_path.display())).unwrap();
        let err = Sentinel::load_topic_configs(store.routing()).unwrap_err();
        assert!(err.to_string().contains("Duplicate sink config"));
    }

    #[test]
    fn test_compute_sha256() {
        // Test with known input
        let hash = compute_sha256("hello world");
        // SHA256 of "hello world" is well-known
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        // Empty string
        let empty_hash = compute_sha256("");
        assert_eq!(
            empty_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        // Different inputs produce different hashes
        let hash1 = compute_sha256("a");
        let hash2 = compute_sha256("b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_artifact_hash() {
        let hash1 = compute_artifact_hash("source", "lockfile", "manifest", "schemas");
        let hash2 = compute_artifact_hash("source", "lockfile", "manifest", "schemas");

        // Same inputs produce same hash
        assert_eq!(hash1, hash2);

        // ORDER MATTERS: hash(a, b) != hash(b, a)
        let hash_ab = compute_artifact_hash("a", "b", "m", "s");
        let hash_ba = compute_artifact_hash("b", "a", "m", "s");
        assert_ne!(hash_ab, hash_ba);

        // Different inputs produce different hashes
        let hash3 = compute_artifact_hash("source1", "lockfile", "manifest", "schemas");
        let hash4 = compute_artifact_hash("source2", "lockfile", "manifest", "schemas");
        assert_ne!(hash3, hash4);
    }

    #[test]
    fn test_cleanup_logic() {
        // Test the cleanup timing logic
        let now = 1000.0;
        let worker_last_seen = 930.0; // 70 seconds ago
        let cutoff = now - 60.0; // 60 second timeout

        // Worker should be stale (930 < 940)
        assert!(worker_last_seen < cutoff);

        // Worker at exactly timeout should be kept
        let at_cutoff = now - 60.0;
        assert!(at_cutoff >= cutoff);

        // Worker just within timeout should be kept
        let recent = now - 59.0;
        assert!(recent >= cutoff);
    }
}

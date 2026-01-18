//! Worker Node
//!
//! Design principles:
//! - VenvManager created once at startup, reused for all jobs
//! - Socket owned directly (not Option) - created during connect
//! - run() consumes self - can only be called once (enforced at compile time)
//! - Jobs tracked with JoinHandles for cancellation and bounded concurrency
//! - Graceful shutdown via shutdown channel

use anyhow::Result;
use thiserror::Error;
use casparian_protocol::types::{
    self, DispatchCommand, HeartbeatStatus, JobStatus, ParsedSinkUri, PrepareEnvCommand,
};
use casparian_protocol::{Message, OpCode};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::bridge::{self, BridgeConfig};
use crate::schema_validation;
use crate::venv_manager::VenvManager;
use arrow::array::{
    Array, ArrayRef, BooleanArray, Int64Array, LargeStringArray, StringArray, StringBuilder,
    UInt64Array,
};
use arrow::compute::filter_record_batch;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

// ============================================================================
// Error Types
// ============================================================================

/// Worker execution errors with retry classification.
///
/// Exit code conventions for Python parsers:
/// - 0: Success
/// - 1: Permanent error (no retry) - e.g., invalid parser code, schema mismatch
/// - 2: Transient error (retry eligible) - e.g., network timeout, resource unavailable
/// - Other: Treated as transient (may retry)
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Permanent error - retrying will not help (e.g., invalid parser, schema violation)
    #[error("Permanent error (no retry): {message}")]
    Permanent { message: String },

    /// Transient error - may succeed on retry (e.g., network timeout, resource busy)
    #[error("Transient error (retry eligible): {message}")]
    Transient { message: String },

    /// Bridge communication error
    #[error("Bridge error: {0}")]
    Bridge(#[from] anyhow::Error),
}

impl WorkerError {
    /// Check if this error is transient (eligible for retry)
    pub fn is_transient(&self) -> bool {
        matches!(self, WorkerError::Transient { .. })
    }

    /// Check if this error is permanent (no retry)
    pub fn is_permanent(&self) -> bool {
        matches!(self, WorkerError::Permanent { .. })
    }

    /// Create from exit code using the Casparian convention:
    /// - 0: Success (not an error)
    /// - 1: Permanent error
    /// - 2: Transient error
    /// - Other: Transient (default to retry)
    pub fn from_exit_code(code: i32, stderr: &str) -> Self {
        let message = if stderr.is_empty() {
            format!("Parser exited with code {}", code)
        } else {
            // Truncate stderr to avoid huge error messages
            let truncated = if stderr.len() > 500 {
                format!("{}... (truncated)", &stderr[..500])
            } else {
                stderr.to_string()
            };
            format!("Parser exited with code {}: {}", code, truncated)
        };

        match code {
            1 => WorkerError::Permanent { message },
            2 => WorkerError::Transient { message },
            _ => WorkerError::Transient { message },
        }
    }

    /// Create from signal termination
    pub fn from_signal(stderr: &str) -> Self {
        let message = if stderr.is_empty() {
            "Parser terminated by signal".to_string()
        } else {
            let truncated = if stderr.len() > 500 {
                format!("{}... (truncated)", &stderr[..500])
            } else {
                stderr.to_string()
            };
            format!("Parser terminated by signal: {}", truncated)
        };
        WorkerError::Transient { message }
    }
}

// ============================================================================
// Constants
// ============================================================================

/// Maximum concurrent jobs per worker
const MAX_CONCURRENT_JOBS: usize = 4;

/// Heartbeat interval (seconds) - worker sends heartbeat to Sentinel
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Worker configuration (plain data)
pub struct WorkerConfig {
    pub sentinel_addr: String,
    pub parquet_root: PathBuf,
    pub worker_id: String,
    pub shim_path: PathBuf,
    /// Plugin capabilities this worker can handle. "*" means all plugins.
    /// Defaults to ["*"] if empty.
    pub capabilities: Vec<String>,
    /// Custom venvs directory. If None, uses ~/.casparian_flow/venvs.
    /// Useful for testing with isolated temp directories.
    pub venvs_dir: Option<PathBuf>,
}

/// Handle for controlling a running worker
pub struct WorkerHandle {
    shutdown_tx: mpsc::Sender<()>,
    join_handle: JoinHandle<Result<()>>,
}

impl WorkerHandle {
    /// Request graceful shutdown
    pub async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(()).await;
        self.join_handle.await?
    }
}

/// Active worker with connected socket
pub struct Worker {
    config: WorkerConfig,
    socket: DealerSocket,
    venv_manager: Arc<VenvManager>, // VenvManager is now Sync (uses std::sync::Mutex internally)
    result_tx: mpsc::Sender<JobResult>,
    result_rx: mpsc::Receiver<JobResult>,
    shutdown_rx: mpsc::Receiver<()>,
    active_jobs: HashMap<u64, JoinHandle<()>>,
}

/// Result from a completed job
struct JobResult {
    job_id: u64,
    receipt: types::JobReceipt,
}

impl Worker {
    /// Connect to sentinel and create worker.
    /// Returns (Worker, ShutdownHandle) - call run() on Worker, use handle for shutdown.
    pub async fn connect(config: WorkerConfig) -> Result<(Self, mpsc::Sender<()>)> {
        // Initialize VenvManager once (now uses std::sync::Mutex internally)
        let venv_manager = match &config.venvs_dir {
            Some(path) => VenvManager::with_path(path.clone())?,
            None => VenvManager::new()?,
        };
        let (count, bytes) = venv_manager.stats();
        info!("VenvManager: {} cached envs, {} MB", count, bytes / 1_000_000);

        // Create and connect socket
        let mut socket = DealerSocket::new();
        socket.connect(&config.sentinel_addr).await?;

        info!("Connected to sentinel: {}", config.sentinel_addr);

        // Send IDENTIFY with configured capabilities
        let capabilities = if config.capabilities.is_empty() {
            vec!["*".to_string()] // Default to wildcard
        } else {
            config.capabilities.clone()
        };
        let identify = types::IdentifyPayload {
            capabilities,
            worker_id: Some(config.worker_id.clone()),
        };
        send_message(&mut socket, OpCode::Identify, 0, &identify).await?;
        info!("Sent IDENTIFY as {}", config.worker_id);

        // Initialize channels
        let (result_tx, result_rx) = mpsc::channel(MAX_CONCURRENT_JOBS * 2);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Ok((
            Self {
                config,
                socket,
                venv_manager: Arc::new(venv_manager),
                result_tx,
                result_rx,
                shutdown_rx,
                active_jobs: HashMap::new(),
            },
            shutdown_tx,
        ))
    }

    /// Main event loop - consumes self (can only be called once)
    pub async fn run(mut self) -> Result<()> {
        info!("Entering event loop...");

        // Create heartbeat interval timer
        let mut heartbeat_interval = tokio::time::interval(
            Duration::from_secs(HEARTBEAT_INTERVAL_SECS)
        );
        // First tick completes immediately, skip it
        heartbeat_interval.tick().await;

        loop {
            // Clean up completed jobs
            self.reap_completed_jobs();

            // Use biased select to prioritize shutdown and results over new messages
            tokio::select! {
                biased;

                // Branch 1: Shutdown signal (highest priority)
                _ = self.shutdown_rx.recv() => {
                    info!("Shutdown signal received, waiting for {} active jobs...", self.active_jobs.len());
                    self.wait_for_all_jobs().await;
                    break;
                }

                // Branch 2: Job Results from spawned tasks
                Some(result) = self.result_rx.recv() => {
                    info!("Job {} finished, sending CONCLUDE", result.job_id);
                    if let Err(e) = send_message(&mut self.socket, OpCode::Conclude, result.job_id, &result.receipt).await {
                        error!("Failed to send CONCLUDE for job {}: {}", result.job_id, e);
                    }
                }

                // Branch 3: Proactive heartbeat (keep Sentinel informed)
                _ = heartbeat_interval.tick() => {
                    let active_job_ids: Vec<i64> = self.active_jobs.keys()
                        .map(|&id| id as i64)
                        .collect();
                    let status = if active_job_ids.is_empty() { HeartbeatStatus::Idle } else { HeartbeatStatus::Busy };
                    let payload = types::HeartbeatPayload {
                        status,
                        current_job_id: active_job_ids.first().copied(),
                        active_job_count: active_job_ids.len(),
                        active_job_ids,
                    };
                    debug!("Sending heartbeat: {:?} ({} active jobs)", status, payload.active_job_count);
                    if let Err(e) = send_message(&mut self.socket, OpCode::Heartbeat, 0, &payload).await {
                        warn!("Failed to send heartbeat: {}", e);
                    }
                }

                // Branch 4: Control Plane Messages from Sentinel
                // Inline recv logic to avoid borrowing self twice
                recv_result = tokio::time::timeout(Duration::from_millis(100), self.socket.recv()) => {
                    match recv_result {
                        Ok(Ok(multipart)) => {
                            // Extract frames
                            let parts: Vec<Vec<u8>> = multipart
                                .into_vec()
                                .into_iter()
                                .map(|b| b.to_vec())
                                .collect();

                            if parts.len() >= 2 {
                                match Message::unpack(&[parts[0].clone(), parts[1].clone()]) {
                                    Ok(msg) => {
                                        if let Err(e) = self.handle_message(msg).await {
                                            error!("Error handling message: {}", e);
                                        }
                                    }
                                    Err(e) => warn!("Failed to unpack message: {}", e),
                                }
                            } else {
                                warn!("Expected 2 frames [header, payload], got {}", parts.len());
                            }
                        }
                        Ok(Err(e)) => {
                            error!("ZMQ recv error: {}", e);
                            break;
                        }
                        Err(_) => {} // Timeout - continue loop
                    }
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Remove completed job handles from active_jobs map
    fn reap_completed_jobs(&mut self) {
        self.active_jobs.retain(|job_id, handle| {
            if handle.is_finished() {
                debug!("Reaped completed job {}", job_id);
                false
            } else {
                true
            }
        });
    }

    /// Wait for all active jobs to complete and send their CONCLUDE messages (for graceful shutdown)
    ///
    /// This is critical for graceful shutdown - we must:
    /// 1. Wait for all job tasks to complete
    /// 2. Drain any pending results from result_rx
    /// 3. Send CONCLUDE messages for all completed jobs
    ///
    /// Otherwise, the sentinel will never know jobs finished.
    async fn wait_for_all_jobs(&mut self) {
        let job_count = self.active_jobs.len();
        info!("Graceful shutdown: waiting for {} active jobs to complete...", job_count);

        // Wait for all job handles to complete
        for (job_id, handle) in self.active_jobs.drain() {
            debug!("Waiting for job {} to complete...", job_id);
            if let Err(e) = handle.await {
                warn!("Job {} task panicked during shutdown: {:?}", job_id, e);
            }
        }

        // Drain all pending results and send CONCLUDE messages
        // Jobs send results via result_tx, we must receive and forward them
        let mut concluded_count = 0;
        while let Ok(result) = self.result_rx.try_recv() {
            info!(
                "Shutdown: sending CONCLUDE for job {} (status: {:?})",
                result.job_id, result.receipt.status
            );
            if let Err(e) = send_message(
                &mut self.socket,
                OpCode::Conclude,
                result.job_id,
                &result.receipt,
            ).await {
                error!(
                    "Failed to send CONCLUDE for job {} during shutdown: {}",
                    result.job_id, e
                );
            }
            concluded_count += 1;
        }

        info!(
            "Graceful shutdown complete: sent {} CONCLUDE messages",
            concluded_count
        );
    }

    /// Handle a message
    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        match msg.header.opcode {
            OpCode::Dispatch => {
                let cmd: DispatchCommand = serde_json::from_slice(&msg.payload)?;
                let job_id = msg.header.job_id;

                // Check if we're at capacity
                if self.active_jobs.len() >= MAX_CONCURRENT_JOBS {
                    warn!(
                        "At max capacity ({} jobs), rejecting job {}",
                        MAX_CONCURRENT_JOBS, job_id
                    );
                    let receipt = types::JobReceipt {
                        status: JobStatus::Rejected,
                        metrics: HashMap::new(),
                        artifacts: vec![],
                        error_message: Some("Worker at capacity".to_string()),
                    };
                    send_message(&mut self.socket, OpCode::Conclude, job_id, &receipt).await?;
                    return Ok(());
                }

                info!(
                    "DISPATCH job {} -> {} ({} active)",
                    job_id,
                    cmd.plugin_name,
                    self.active_jobs.len() + 1
                );

                // Clone what we need for the spawned task
                let tx = self.result_tx.clone();
                let venv_mgr = self.venv_manager.clone();
                let parquet_root = self.config.parquet_root.clone();
                let shim_path = self.config.shim_path.clone();

                let handle = tokio::spawn(async move {
                    let receipt =
                        execute_job(job_id, cmd, venv_mgr, parquet_root, shim_path).await;
                    // If channel is closed, worker is shutting down - that's fine
                    let _ = tx.send(JobResult { job_id, receipt }).await;
                });

                self.active_jobs.insert(job_id, handle);
            }

            OpCode::PrepareEnv => {
                let cmd: PrepareEnvCommand = serde_json::from_slice(&msg.payload)?;
                info!("PREPARE_ENV {}", truncate_hash(&cmd.env_hash));

                match self.prepare_env(&cmd).await {
                    Ok(interpreter_path) => {
                        let payload = types::EnvReadyPayload {
                            env_hash: cmd.env_hash,
                            interpreter_path: interpreter_path.display().to_string(),
                            cached: true,
                        };
                        send_message(&mut self.socket, OpCode::EnvReady, 0, &payload).await?;
                    }
                    Err(e) => {
                        let payload = types::ErrorPayload {
                            message: e.to_string(),
                            traceback: None,
                        };
                        send_message(&mut self.socket, OpCode::Err, 0, &payload).await?;
                    }
                }
            }

            OpCode::Heartbeat => {
                debug!("Received HEARTBEAT, replying...");
                // Convert job IDs to i64, filtering any that would overflow (shouldn't happen)
                let active_job_ids: Vec<i64> = self.active_jobs.keys()
                    .copied()
                    .filter_map(|id| i64::try_from(id).ok())
                    .collect();
                let active_job_count = self.active_jobs.len(); // Use actual count, not filtered
                let current_job = active_job_ids.first().copied();

                let status = if active_job_count == 0 {
                    HeartbeatStatus::Idle
                } else if active_job_count >= MAX_CONCURRENT_JOBS {
                    HeartbeatStatus::Busy  // At capacity
                } else {
                    HeartbeatStatus::Alive // Working but can accept more
                };

                let payload = types::HeartbeatPayload {
                    status,
                    current_job_id: current_job,
                    active_job_count,
                    active_job_ids,
                };
                send_message(&mut self.socket, OpCode::Heartbeat, 0, &payload).await?;
            }

            OpCode::Abort => {
                let job_id = msg.header.job_id;
                if let Some(handle) = self.active_jobs.remove(&job_id) {
                    warn!("ABORT job {} - cancelling task", job_id);
                    handle.abort();
                    // Send failure receipt
                    let receipt = types::JobReceipt {
                        status: JobStatus::Aborted,
                        metrics: HashMap::new(),
                        artifacts: vec![],
                        error_message: Some("Job aborted by sentinel".to_string()),
                    };
                    send_message(&mut self.socket, OpCode::Conclude, job_id, &receipt).await?;
                } else {
                    warn!("ABORT job {} - not found in active jobs", job_id);
                }
            }

            OpCode::Err => {
                let err: types::ErrorPayload = serde_json::from_slice(&msg.payload)?;
                error!("Received ERR from sentinel: {}", err.message);
            }

            _ => {
                warn!("Unhandled opcode: {:?}", msg.header.opcode);
            }
        }
        Ok(())
    }

    /// Prepare environment (blocking operation)
    async fn prepare_env(&self, cmd: &PrepareEnvCommand) -> Result<PathBuf> {
        let env_hash = cmd.env_hash.clone();
        let lockfile = cmd.lockfile_content.clone();
        let python_version = cmd.python_version.clone();
        let venv_manager = self.venv_manager.clone();

        // VenvManager now uses std::sync::Mutex, so this is safe
        tokio::task::spawn_blocking(move || {
            venv_manager.get_or_create(&env_hash, &lockfile, python_version.as_deref())
        })
        .await?
    }
}

// --- Helper functions ---

/// Truncate hash for display (first 12 chars)
fn truncate_hash(hash: &str) -> &str {
    if hash.len() > 12 {
        &hash[..12]
    } else {
        hash
    }
}

fn compute_source_hash(path: &str) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|e| anyhow::anyhow!("failed to open source file '{}': {}", path, e))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let bytes = file
            .read(&mut buffer)
            .map_err(|e| anyhow::anyhow!("failed to read source file '{}': {}", path, e))?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn batch_has_lineage_columns(batch: &RecordBatch) -> bool {
    let schema = batch.schema();
    schema.index_of("_cf_source_hash").is_ok()
        || schema.index_of("_cf_job_id").is_ok()
        || schema.index_of("_cf_processed_at").is_ok()
        || schema.index_of("_cf_parser_version").is_ok()
}

fn inject_lineage_batches(
    output_name: &str,
    batches: Vec<RecordBatch>,
    source_hash: &str,
    job_id: &str,
    parser_version: &str,
) -> Result<Vec<RecordBatch>> {
    if batches.is_empty() {
        return Ok(batches);
    }

    let total_batches = batches.len();
    let mut with_lineage = Vec::with_capacity(total_batches);
    let existing_count = batches
        .iter()
        .filter(|batch| batch_has_lineage_columns(batch))
        .count();

    if existing_count == 0 {
        for batch in batches {
            let injected =
                casparian_sinks::inject_lineage_columns(&batch, source_hash, job_id, parser_version)?;
            with_lineage.push(injected);
        }
        return Ok(with_lineage);
    }

    if existing_count == total_batches {
        warn!(
            "Output '{}' already includes lineage columns; skipping injection.",
            output_name
        );
        return Ok(batches);
    }

    anyhow::bail!(
        "Output '{}' has inconsistent lineage columns across batches",
        output_name
    );
}

fn normalize_schema_def(schema_def: &str) -> Option<&str> {
    let trimmed = schema_def.trim();
    if trimmed.is_empty() || trimmed == "null" {
        None
    } else {
        Some(trimmed)
    }
}

struct OutputMetrics {
    name: String,
    rows: usize,
    quarantine_rows: usize,
    lineage_unavailable_rows: usize,
}

struct ExecutionMetrics {
    rows: usize,
    quarantine_rows: usize,
    lineage_unavailable_rows: usize,
    outputs: Vec<OutputMetrics>,
}

enum ExecutionOutcome {
    Success {
        metrics: ExecutionMetrics,
        artifacts: Vec<HashMap<String, String>>,
    },
    QuarantineRejected {
        metrics: ExecutionMetrics,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct QuarantineConfig {
    allow_quarantine: bool,
    max_quarantine_pct: f64,
    max_quarantine_count: Option<usize>,
}

impl Default for QuarantineConfig {
    fn default() -> Self {
        Self {
            allow_quarantine: false,
            max_quarantine_pct: 10.0,
            max_quarantine_count: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct QuarantineConfigOverrides {
    allow_quarantine: Option<bool>,
    max_quarantine_pct: Option<f64>,
    max_quarantine_count: Option<usize>,
}

impl QuarantineConfig {
    fn apply(&mut self, overrides: QuarantineConfigOverrides) {
        if let Some(allow) = overrides.allow_quarantine {
            self.allow_quarantine = allow;
        }
        if let Some(max_pct) = overrides.max_quarantine_pct {
            self.max_quarantine_pct = max_pct;
        }
        if let Some(max_count) = overrides.max_quarantine_count {
            self.max_quarantine_count = Some(max_count);
        }
    }
}

impl QuarantineConfigOverrides {
    fn apply(&mut self, overrides: QuarantineConfigOverrides) {
        if overrides.allow_quarantine.is_some() {
            self.allow_quarantine = overrides.allow_quarantine;
        }
        if overrides.max_quarantine_pct.is_some() {
            self.max_quarantine_pct = overrides.max_quarantine_pct;
        }
        if overrides.max_quarantine_count.is_some() {
            self.max_quarantine_count = overrides.max_quarantine_count;
        }
    }
}

fn insert_execution_metrics(metrics: &mut HashMap<String, i64>, exec: &ExecutionMetrics) {
    metrics.insert("rows".to_string(), exec.rows as i64);
    metrics.insert("quarantine_rows".to_string(), exec.quarantine_rows as i64);
    metrics.insert(
        "lineage_unavailable_rows".to_string(),
        exec.lineage_unavailable_rows as i64,
    );
    for output in &exec.outputs {
        metrics.insert(format!("rows.{}", output.name), output.rows as i64);
        metrics.insert(
            format!("quarantine_rows.{}", output.name),
            output.quarantine_rows as i64,
        );
        metrics.insert(
            format!("lineage_unavailable_rows.{}", output.name),
            output.lineage_unavailable_rows as i64,
        );
    }
}

fn find_sink_config<'a>(
    cmd: &'a DispatchCommand,
    output_name: &str,
) -> Option<&'a types::SinkConfig> {
    cmd.sinks
        .iter()
        .find(|sink| sink.topic == output_name)
        .or_else(|| cmd.sinks.first())
}

fn resolve_quarantine_config(
    schema_def: Option<&str>,
    sink_uri: &str,
    output_name: &str,
) -> Result<QuarantineConfig> {
    let mut config = QuarantineConfig::default();
    if let Some(schema_def) = schema_def {
        let overrides = parse_quarantine_overrides_from_schema_def(schema_def, output_name)?;
        config.apply(overrides);
    }

    let sink_overrides = parse_quarantine_overrides_from_sink_uri(sink_uri)?;
    config.apply(sink_overrides);

    validate_quarantine_config(&config)?;
    Ok(config)
}

fn parse_quarantine_overrides_from_schema_def(
    schema_def: &str,
    output_name: &str,
) -> Result<QuarantineConfigOverrides> {
    let value: serde_json::Value = serde_json::from_str(schema_def)
        .map_err(|e| anyhow::anyhow!("schema_def is not valid JSON: {}", e))?;

    let Some(obj) = value.as_object() else {
        return Ok(QuarantineConfigOverrides::default());
    };

    let mut overrides = QuarantineConfigOverrides::default();

    if let Some(config_value) = obj.get("quarantine_config") {
        overrides.apply(parse_quarantine_overrides_value(config_value)?);
    }

    if let Some(schemas) = obj.get("schemas").and_then(|v| v.as_array()) {
        if let Some(schema) = schemas.iter().find(|schema| {
            schema
                .get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|name| name == output_name)
        }) {
            if let Some(config_value) = schema.get("quarantine_config") {
                overrides.apply(parse_quarantine_overrides_value(config_value)?);
            }
        }
    }

    Ok(overrides)
}

fn parse_quarantine_overrides_value(value: &serde_json::Value) -> Result<QuarantineConfigOverrides> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("quarantine_config must be an object"))?;

    let allow_quarantine = match obj.get("allow_quarantine") {
        Some(value) => Some(parse_bool_value(value)?),
        None => None,
    };

    let max_quarantine_pct = match obj.get("max_quarantine_pct") {
        Some(value) => Some(parse_f64_value(value, "max_quarantine_pct")?),
        None => None,
    };

    let max_quarantine_count = match obj.get("max_quarantine_count") {
        Some(value) => Some(parse_usize_value(value, "max_quarantine_count")?),
        None => None,
    };

    Ok(QuarantineConfigOverrides {
        allow_quarantine,
        max_quarantine_pct,
        max_quarantine_count,
    })
}

fn parse_quarantine_overrides_from_sink_uri(uri: &str) -> Result<QuarantineConfigOverrides> {
    let parsed = ParsedSinkUri::parse(uri)
        .map_err(|e| anyhow::anyhow!("invalid sink uri '{}': {}", uri, e))?;

    let mut overrides = QuarantineConfigOverrides::default();

    if let Some(value) = parsed.query.get("allow_quarantine") {
        overrides.allow_quarantine = Some(parse_bool_str(value)?);
    }
    if let Some(value) = parsed.query.get("max_quarantine_pct") {
        overrides.max_quarantine_pct = Some(parse_f64_str(value, "max_quarantine_pct")?);
    }
    if let Some(value) = parsed.query.get("max_quarantine_count") {
        overrides.max_quarantine_count = Some(parse_usize_str(value, "max_quarantine_count")?);
    }

    Ok(overrides)
}

fn validate_quarantine_config(config: &QuarantineConfig) -> Result<()> {
    if !config.max_quarantine_pct.is_finite() {
        anyhow::bail!("max_quarantine_pct must be finite");
    }
    if config.max_quarantine_pct < 0.0 || config.max_quarantine_pct > 100.0 {
        anyhow::bail!("max_quarantine_pct must be between 0 and 100");
    }
    Ok(())
}

fn parse_bool_value(value: &serde_json::Value) -> Result<bool> {
    match value {
        serde_json::Value::Bool(v) => Ok(*v),
        serde_json::Value::Number(num) => {
            if num == &0.into() {
                Ok(false)
            } else if num == &1.into() {
                Ok(true)
            } else {
                anyhow::bail!("invalid boolean number '{}'", num);
            }
        }
        serde_json::Value::String(text) => parse_bool_str(text),
        _ => anyhow::bail!("invalid boolean value '{}'", value),
    }
}

fn parse_f64_value(value: &serde_json::Value, field: &str) -> Result<f64> {
    match value {
        serde_json::Value::Number(num) => num
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("{} must be a number", field)),
        serde_json::Value::String(text) => parse_f64_str(text, field),
        _ => anyhow::bail!("{} must be a number", field),
    }
}

fn parse_usize_value(value: &serde_json::Value, field: &str) -> Result<usize> {
    match value {
        serde_json::Value::Number(num) => num
            .as_u64()
            .map(|v| v as usize)
            .ok_or_else(|| anyhow::anyhow!("{} must be a non-negative integer", field)),
        serde_json::Value::String(text) => parse_usize_str(text, field),
        _ => anyhow::bail!("{} must be a non-negative integer", field),
    }
}

fn parse_bool_str(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        other => anyhow::bail!("invalid boolean '{}'", other),
    }
}

fn parse_f64_str(value: &str, field: &str) -> Result<f64> {
    let parsed: f64 = value
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("{} must be a number", field))?;
    Ok(parsed)
}

fn parse_usize_str(value: &str, field: &str) -> Result<usize> {
    let parsed: usize = value
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("{} must be a non-negative integer", field))?;
    Ok(parsed)
}

fn check_quarantine_policy(
    output_name: &str,
    quarantine_rows: usize,
    total_rows: usize,
    config: &QuarantineConfig,
) -> Option<String> {
    if quarantine_rows == 0 {
        return None;
    }
    if !config.allow_quarantine {
        return Some(format!(
            "quarantine disabled for '{}': {} rows",
            output_name, quarantine_rows
        ));
    }

    if let Some(max_count) = config.max_quarantine_count {
        if quarantine_rows > max_count {
            return Some(format!(
                "quarantine count exceeded for '{}': {} > {}",
                output_name, quarantine_rows, max_count
            ));
        }
    }

    let pct = quarantine_pct(quarantine_rows, total_rows);
    if pct > config.max_quarantine_pct {
        return Some(format!(
            "quarantine pct exceeded for '{}': {:.2}% > {:.2}%",
            output_name, pct, config.max_quarantine_pct
        ));
    }

    None
}

fn quarantine_pct(quarantine_rows: usize, total_rows: usize) -> f64 {
    if total_rows == 0 {
        0.0
    } else {
        (quarantine_rows as f64 / total_rows as f64) * 100.0
    }
}

/// Execute a job and return receipt
///
/// The receipt includes error classification for retry decisions:
/// - `error_message` contains the error details
/// - `metrics["is_transient"]` indicates if the error is retry-eligible (1 = transient, 0 = permanent)
async fn execute_job(
    job_id: u64,
    cmd: DispatchCommand,
    venv_manager: Arc<VenvManager>,
    parquet_root: PathBuf,
    shim_path: PathBuf,
) -> types::JobReceipt {
    match execute_job_inner(job_id, &cmd, &venv_manager, &parquet_root, &shim_path).await {
        Ok(ExecutionOutcome::Success { metrics: exec_metrics, artifacts }) => {
            let mut metrics = HashMap::new();
            insert_execution_metrics(&mut metrics, &exec_metrics);

            types::JobReceipt {
                status: if exec_metrics.quarantine_rows > 0 {
                    JobStatus::PartialSuccess
                } else {
                    JobStatus::Success
                },
                metrics,
                artifacts,
                error_message: None,
            }
        }
        Ok(ExecutionOutcome::QuarantineRejected { metrics: exec_metrics, reason }) => {
            let mut metrics = HashMap::new();
            insert_execution_metrics(&mut metrics, &exec_metrics);
            metrics.insert("is_transient".to_string(), 0);
            metrics.insert("quarantine_rejected".to_string(), 1);

            types::JobReceipt {
                status: JobStatus::Failed,
                metrics,
                artifacts: vec![],
                error_message: Some(reason),
            }
        }
        Err(worker_err) => {
            let is_transient = worker_err.is_transient();
            let error_message = worker_err.to_string();

            if is_transient {
                warn!("Job {} failed (transient, retry eligible): {}", job_id, error_message);
            } else {
                error!("Job {} failed (permanent, no retry): {}", job_id, error_message);
            }

            let mut metrics = HashMap::new();
            // Include error classification in metrics for Sentinel to read
            metrics.insert("is_transient".to_string(), if is_transient { 1 } else { 0 });

            types::JobReceipt {
                status: JobStatus::Failed,
                metrics,
                artifacts: vec![],
                error_message: Some(error_message),
            }
        }
    }
}

/// Execute a job, returning WorkerError with retry classification on failure
async fn execute_job_inner(
    job_id: u64,
    cmd: &DispatchCommand,
    venv_manager: &VenvManager,
    parquet_root: &std::path::Path,
    shim_path: &std::path::Path,
) -> std::result::Result<ExecutionOutcome, WorkerError> {
    // Determine interpreter path
    let interpreter = if cmd.env_hash == "system" {
        // Use system Python for legacy plugins without lockfile
        which::which("python3")
            .or_else(|_| which::which("python"))
            .map_err(|e| WorkerError::Permanent {
                message: format!("No system Python found: {}", e),
            })?
    } else {
        // Use venv interpreter
        let interp = venv_manager.interpreter_path(&cmd.env_hash);
        if !interp.exists() {
            info!(
                "Job {}: environment {} not cached, provisioning...",
                job_id,
                truncate_hash(&cmd.env_hash)
            );
            // Environment not provisioned - this is a transient error, worker can retry
            // after sentinel sends PREPARE_ENV
            return Err(WorkerError::Transient {
                message: format!(
                    "Environment {} not provisioned. Worker cannot auto-provision without lockfile. \
                     Either send PREPARE_ENV first, or include lockfile in DISPATCH.",
                    truncate_hash(&cmd.env_hash)
                ),
            });
        }
        interp
    };

    // Execute bridge
    let config = BridgeConfig {
        interpreter_path: interpreter,
        source_code: cmd.source_code.clone(),
        file_path: cmd.file_path.clone(),
        job_id,
        file_id: cmd.file_id,
        shim_path: shim_path.to_path_buf(),
    };

    let bridge_result = bridge::execute_bridge(config).await.map_err(|e| {
        // Classify bridge errors by examining the error message
        let error_str = e.to_string().to_lowercase();

        // Permanent errors: syntax errors, import errors, schema violations
        if error_str.contains("syntaxerror")
            || error_str.contains("importerror")
            || error_str.contains("modulenotfounderror")
            || error_str.contains("schema")
            || error_str.contains("exited with exit status: 1")
        {
            WorkerError::Permanent {
                message: e.to_string(),
            }
        }
        // Exit code 2 explicitly indicates transient
        else if error_str.contains("exited with exit status: 2") {
            WorkerError::Transient {
                message: e.to_string(),
            }
        }
        // Transient errors: timeouts, network issues, resource unavailable
        else if error_str.contains("timeout")
            || error_str.contains("connection")
            || error_str.contains("resource")
            || error_str.contains("signal")
        {
            WorkerError::Transient {
                message: e.to_string(),
            }
        }
        // Default to transient (conservative - allow retry)
        else {
            WorkerError::Transient {
                message: e.to_string(),
            }
        }
    })?;

    let batches = bridge_result.batches;

    // Log the captured logs for debugging (will be stored in DB in Phase 3)
    if !bridge_result.logs.is_empty() {
        debug!("Job {} logs ({} bytes):\n{}", job_id, bridge_result.logs.len(), bridge_result.logs);
    }

    let default_sink = format!("parquet://{}", parquet_root.display());
    let sink_uri = cmd
        .sinks
        .first()
        .map(|s| s.uri.as_str())
        .unwrap_or(default_sink.as_str());

    let descriptors: Vec<casparian_sinks::OutputDescriptor> = bridge_result
        .output_info
        .iter()
        .map(|info| casparian_sinks::OutputDescriptor {
            name: info.name.clone(),
            table: info.table.clone(),
        })
        .collect();

    let outputs = casparian_sinks::plan_outputs(&descriptors, &batches, "output")
        .map_err(|e| WorkerError::Permanent { message: e.to_string() })?;

    let job_id_str = job_id.to_string();
    let source_hash = match compute_source_hash(&cmd.file_path) {
        Ok(hash) => hash,
        Err(err) => {
            warn!(
                "Job {}: failed to compute source hash for '{}': {}",
                job_id, cmd.file_path, err
            );
            "unknown".to_string()
        }
    };
    let parser_version = cmd.parser_version.as_deref().unwrap_or("unknown");

    let mut total_rows = 0;
    let mut quarantine_rows = 0;
    let mut lineage_unavailable_rows = 0;
    let mut artifacts = Vec::new();
    let mut output_metrics = Vec::new();
    let mut policy_failures = Vec::new();

    let mut valid_owned = Vec::new();
    let mut quarantine_owned = Vec::new();

    for output in outputs {
        let sink_config = find_sink_config(cmd, &output.name);
        let schema_def = sink_config
            .and_then(|sink| sink.schema_def.as_deref())
            .and_then(normalize_schema_def);
        let sink_uri_for_config = sink_config
            .map(|sink| sink.uri.as_str())
            .unwrap_or(sink_uri);

        let mut output_batches: Vec<RecordBatch> =
            output.batches.iter().map(|batch| (*batch).clone()).collect();
        if let Some(schema_def) = schema_def {
            output_batches = schema_validation::enforce_schema_on_batches(
                &output_batches,
                schema_def,
                &output.name,
            )
            .map_err(|e| WorkerError::Permanent {
                message: format!("schema validation failed for '{}': {}", output.name, e),
            })?;
        }

        let output_batch_refs: Vec<&RecordBatch> = output_batches.iter().collect();
        let (valid_batches, quarantine_batches, quarantined, lineage_unavailable) =
            split_output_batches(job_id, &output_batch_refs)
                .map_err(|e| WorkerError::Permanent { message: e.to_string() })?;
        let valid_rows: usize = valid_batches.iter().map(|batch| batch.num_rows()).sum();
        let output_rows = valid_rows + quarantined;
        total_rows += valid_rows;
        quarantine_rows += quarantined;
        lineage_unavailable_rows += lineage_unavailable;
        output_metrics.push(OutputMetrics {
            name: output.name.clone(),
            rows: valid_rows,
            quarantine_rows: quarantined,
            lineage_unavailable_rows: lineage_unavailable,
        });

        let quarantine_config = resolve_quarantine_config(schema_def, sink_uri_for_config, &output.name)
            .map_err(|e| WorkerError::Permanent {
                message: format!("invalid quarantine config for '{}': {}", output.name, e),
            })?;
        if let Some(reason) =
            check_quarantine_policy(&output.name, quarantined, output_rows, &quarantine_config)
        {
            policy_failures.push(reason);
        }

        if !valid_batches.is_empty() {
            let lineage_batches = inject_lineage_batches(
                &output.name,
                valid_batches,
                &source_hash,
                &job_id_str,
                parser_version,
            )
            .map_err(|e| WorkerError::Permanent {
                message: format!("lineage injection failed for '{}': {}", output.name, e),
            })?;
            valid_owned.push(OwnedOutput {
                name: output.name.clone(),
                table: output.table.clone(),
                batches: lineage_batches,
            });
        }
        if !quarantine_batches.is_empty() {
            let quarantine_name = format!("{}_quarantine", output.name);
            let quarantine_table = output
                .table
                .as_ref()
                .map(|t| format!("{}_quarantine", t));
            quarantine_owned.push(OwnedOutput {
                name: quarantine_name,
                table: quarantine_table,
                batches: quarantine_batches,
            });
        }
    }

    let exec_metrics = ExecutionMetrics {
        rows: total_rows,
        quarantine_rows,
        lineage_unavailable_rows,
        outputs: output_metrics,
    };

    if !policy_failures.is_empty() {
        let reason = policy_failures.join("; ");
        return Ok(ExecutionOutcome::QuarantineRejected {
            metrics: exec_metrics,
            reason,
        });
    }

    if !valid_owned.is_empty() {
        let plans = to_output_plans(&valid_owned);
        let written = casparian_sinks::write_output_plan(sink_uri, &plans, &job_id_str)
            .map_err(|e| WorkerError::Transient { message: e.to_string() })?;
        for output in written {
            let mut artifact = HashMap::new();
            artifact.insert("topic".to_string(), output.name);
            artifact.insert("uri".to_string(), output.uri);
            artifacts.push(artifact);
        }
    }

    if !quarantine_owned.is_empty() {
        let plans = to_output_plans(&quarantine_owned);
        let written = casparian_sinks::write_output_plan(sink_uri, &plans, &job_id_str)
            .map_err(|e| WorkerError::Transient { message: e.to_string() })?;
        for output in written {
            let mut artifact = HashMap::new();
            artifact.insert("topic".to_string(), output.name);
            artifact.insert("uri".to_string(), output.uri);
            artifacts.push(artifact);
        }
    }

    info!(
        "Job {} complete: {} rows ({} quarantined)",
        job_id, total_rows, quarantine_rows
    );
    Ok(ExecutionOutcome::Success {
        metrics: exec_metrics,
        artifacts,
    })
}

struct OwnedOutput {
    name: String,
    table: Option<String>,
    batches: Vec<RecordBatch>,
}

fn to_output_plans<'a>(outputs: &'a [OwnedOutput]) -> Vec<casparian_sinks::OutputPlan<'a>> {
    outputs
        .iter()
        .map(|output| casparian_sinks::OutputPlan {
            name: output.name.clone(),
            table: output.table.clone(),
            batches: output.batches.iter().collect(),
        })
        .collect()
}

fn split_output_batches(
    job_id: u64,
    batches: &[&RecordBatch],
) -> Result<(Vec<RecordBatch>, Vec<RecordBatch>, usize, usize)> {
    let mut valid_batches = Vec::new();
    let mut quarantine_batches = Vec::new();
    let mut quarantined = 0;
    let mut lineage_unavailable_rows = 0;

    for batch in batches {
        let Some(error_idx) = batch.schema().index_of("_cf_row_error").ok() else {
            valid_batches.push((*batch).clone());
            continue;
        };

        let error_col = batch.column(error_idx).clone();
        let (valid_mask, invalid_mask) = build_quarantine_masks(&error_col)?;

        let valid_batch = filter_record_batch(batch, &valid_mask)?;
        if valid_batch.num_rows() > 0 {
            valid_batches.push(valid_batch);
        }

        let quarantine_batch = filter_record_batch(batch, &invalid_mask)?;
        if quarantine_batch.num_rows() > 0 {
            let (augmented, batch_lineage_unavailable) =
                augment_quarantine_batch(&quarantine_batch, batch, &invalid_mask, job_id)?;
            quarantined += augmented.num_rows();
            lineage_unavailable_rows += batch_lineage_unavailable;
            quarantine_batches.push(augmented);
        }
    }

    Ok((
        valid_batches,
        quarantine_batches,
        quarantined,
        lineage_unavailable_rows,
    ))
}

enum SourceRowStatus {
    Valid(ArrayRef),
    Invalid(String),
    Missing,
}

fn augment_quarantine_batch(
    batch: &RecordBatch,
    source_batch: &RecordBatch,
    invalid_mask: &BooleanArray,
    job_id: u64,
) -> Result<(RecordBatch, usize)> {
    let mut fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    let mut columns = batch.columns().to_vec();
    let original_len = columns.len();
    let mut lineage_unavailable_rows = 0;

    let error_messages = collect_error_messages(batch)?;

    if batch.schema().index_of("_error_msg").is_err() {
        let error_array = build_error_msg_array(&error_messages);
        fields.push(Field::new("_error_msg", DataType::Utf8, true));
        columns.push(error_array);
    }

    if batch.schema().index_of("_violation_type").is_err() {
        let violation_array = build_violation_type_array(&error_messages);
        fields.push(Field::new("_violation_type", DataType::Utf8, false));
        columns.push(violation_array);
    }

    if batch.schema().index_of("_cf_job_id").is_err() {
        let job_array = build_job_id_array(job_id, batch.num_rows());
        fields.push(Field::new("_cf_job_id", DataType::Utf8, false));
        columns.push(job_array);
    }

    let invalid_indices = invalid_row_indices(invalid_mask);
    if invalid_indices.len() != batch.num_rows() {
        anyhow::bail!(
            "quarantine mask mismatch: {} != {}",
            invalid_indices.len(),
            batch.num_rows()
        );
    }

    if batch.schema().index_of("_source_row").is_err() {
        let output_row_index_exists = batch.schema().index_of("_output_row_index").is_ok();
        match source_row_from_row_id(source_batch, &invalid_indices)? {
            SourceRowStatus::Valid(array) => {
                fields.push(Field::new("_source_row", DataType::Int64, false));
                columns.push(array);
            }
            SourceRowStatus::Invalid(reason) => {
                lineage_unavailable_rows = invalid_indices.len();
                if !output_row_index_exists {
                    warn!(
                        "Job {}: __cf_row_id invalid; {}. Using output row index.",
                        job_id, reason
                    );
                    let output_array = output_row_index_array(&invalid_indices)?;
                    fields.push(Field::new("_output_row_index", DataType::Int64, false));
                    columns.push(output_array);
                }
            }
            SourceRowStatus::Missing => {
                lineage_unavailable_rows = invalid_indices.len();
                if !output_row_index_exists {
                    warn!(
                        "Job {}: __cf_row_id missing; using output row index for quarantine lineage.",
                        job_id
                    );
                    let output_array = output_row_index_array(&invalid_indices)?;
                    fields.push(Field::new("_output_row_index", DataType::Int64, false));
                    columns.push(output_array);
                }
            }
        }
    }

    if columns.len() == original_len {
        return Ok((batch.clone(), lineage_unavailable_rows));
    }

    let schema = Arc::new(Schema::new(fields));
    Ok((RecordBatch::try_new(schema, columns)?, lineage_unavailable_rows))
}

fn invalid_row_indices(mask: &BooleanArray) -> Vec<usize> {
    let mut indices = Vec::new();
    for i in 0..mask.len() {
        if mask.is_valid(i) && mask.value(i) {
            indices.push(i);
        }
    }
    indices
}

fn collect_error_messages(batch: &RecordBatch) -> Result<Vec<Option<String>>> {
    if let Ok(idx) = batch.schema().index_of("_error_msg") {
        return read_string_column(batch.column(idx), "_error_msg");
    }

    let idx = batch
        .schema()
        .index_of("_cf_row_error")
        .map_err(|_| anyhow::anyhow!("_cf_row_error column missing from quarantine batch"))?;
    read_string_column(batch.column(idx), "_cf_row_error")
}

fn read_string_column(array: &ArrayRef, name: &str) -> Result<Vec<Option<String>>> {
    match array.data_type() {
        DataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow::anyhow!("{} column is not Utf8", name))?;
            let mut values = Vec::with_capacity(arr.len());
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    values.push(None);
                } else {
                    values.push(Some(arr.value(i).to_string()));
                }
            }
            Ok(values)
        }
        DataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| anyhow::anyhow!("{} column is not LargeUtf8", name))?;
            let mut values = Vec::with_capacity(arr.len());
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    values.push(None);
                } else {
                    values.push(Some(arr.value(i).to_string()));
                }
            }
            Ok(values)
        }
        _ => {
            warn!(
                "{} column is not Utf8/LargeUtf8 (got {:?}); using placeholder error message.",
                name,
                array.data_type()
            );
            let mut values = Vec::with_capacity(array.len());
            for i in 0..array.len() {
                if array.is_null(i) {
                    values.push(None);
                } else {
                    values.push(Some("non_string_error".to_string()));
                }
            }
            Ok(values)
        }
    }
}

fn build_error_msg_array(messages: &[Option<String>]) -> ArrayRef {
    let mut builder = StringBuilder::new();
    for message in messages {
        if let Some(value) = message.as_deref() {
            builder.append_value(value);
        } else {
            builder.append_null();
        }
    }
    Arc::new(builder.finish())
}

fn build_violation_type_array(messages: &[Option<String>]) -> ArrayRef {
    let mut builder = StringBuilder::new();
    for message in messages {
        let value = violation_type_for_message(message.as_deref());
        builder.append_value(value);
    }
    Arc::new(builder.finish())
}

fn build_job_id_array(job_id: u64, rows: usize) -> ArrayRef {
    let mut builder = StringBuilder::new();
    let job_id_str = job_id.to_string();
    for _ in 0..rows {
        builder.append_value(&job_id_str);
    }
    Arc::new(builder.finish())
}

fn output_row_index_array(indices: &[usize]) -> Result<ArrayRef> {
    let mut values = Vec::with_capacity(indices.len());
    for index in indices {
        let value =
            i64::try_from(*index).map_err(|_| anyhow::anyhow!("output row index overflow"))?;
        values.push(value);
    }
    Ok(Arc::new(Int64Array::from(values)) as ArrayRef)
}

fn source_row_from_row_id(
    source_batch: &RecordBatch,
    invalid_indices: &[usize],
) -> Result<SourceRowStatus> {
    let idx = match source_batch.schema().index_of("__cf_row_id") {
        Ok(idx) => idx,
        Err(_) => return Ok(SourceRowStatus::Missing),
    };
    let array = source_batch.column(idx);

    match array.data_type() {
        DataType::Int64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| anyhow::anyhow!("__cf_row_id column is not Int64"))?;
            let mut values = Vec::with_capacity(invalid_indices.len());
            for &row in invalid_indices {
                if arr.is_null(row) {
                    return Ok(SourceRowStatus::Invalid(
                        "__cf_row_id contains nulls".to_string(),
                    ));
                }
                let value = arr.value(row);
                if value < 0 {
                    return Ok(SourceRowStatus::Invalid(
                        "__cf_row_id contains negative values".to_string(),
                    ));
                }
                values.push(value);
            }
            Ok(SourceRowStatus::Valid(Arc::new(Int64Array::from(values)) as ArrayRef))
        }
        DataType::UInt64 => {
            let arr = array
                .as_any()
                .downcast_ref::<UInt64Array>()
                .ok_or_else(|| anyhow::anyhow!("__cf_row_id column is not UInt64"))?;
            let mut values = Vec::with_capacity(invalid_indices.len());
            for &row in invalid_indices {
                if arr.is_null(row) {
                    return Ok(SourceRowStatus::Invalid(
                        "__cf_row_id contains nulls".to_string(),
                    ));
                }
                let value = arr.value(row);
                if value > i64::MAX as u64 {
                    return Ok(SourceRowStatus::Invalid(
                        "__cf_row_id exceeds Int64 range".to_string(),
                    ));
                }
                values.push(value as i64);
            }
            Ok(SourceRowStatus::Valid(Arc::new(Int64Array::from(values)) as ArrayRef))
        }
        _ => Ok(SourceRowStatus::Invalid(format!(
            "__cf_row_id has non-integer type {:?}",
            array.data_type()
        ))),
    }
}

fn violation_type_for_message(message: Option<&str>) -> &'static str {
    match message.map(str::trim) {
        Some(msg) if msg.is_empty() => "unknown",
        Some(msg) if msg.starts_with("schema:") => {
            if msg.contains("null not allowed") {
                "null_not_allowed"
            } else {
                "schema"
            }
        }
        Some(_) => "parser",
        None => "unknown",
    }
}

fn build_quarantine_masks(error_col: &ArrayRef) -> Result<(BooleanArray, BooleanArray)> {
    let mut valid_flags = Vec::with_capacity(error_col.len());
    let mut invalid_flags = Vec::with_capacity(error_col.len());

    match error_col.data_type() {
        DataType::Utf8 => {
            let arr = error_col
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow::anyhow!("_cf_row_error column is not Utf8"))?;
            for i in 0..arr.len() {
                let is_valid = arr.is_null(i) || arr.value(i).is_empty();
                valid_flags.push(is_valid);
                invalid_flags.push(!is_valid);
            }
        }
        DataType::LargeUtf8 => {
            let arr = error_col
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| anyhow::anyhow!("_cf_row_error column is not LargeUtf8"))?;
            for i in 0..arr.len() {
                let is_valid = arr.is_null(i) || arr.value(i).is_empty();
                valid_flags.push(is_valid);
                invalid_flags.push(!is_valid);
            }
        }
        _ => {
            for i in 0..error_col.len() {
                let is_valid = error_col.is_null(i);
                valid_flags.push(is_valid);
                invalid_flags.push(!is_valid);
            }
        }
    }

    Ok((
        BooleanArray::from(valid_flags),
        BooleanArray::from(invalid_flags),
    ))
}

/// Send a protocol message as multipart (header + body in one ZMQ message)
async fn send_message<T: serde::Serialize>(
    socket: &mut DealerSocket,
    opcode: OpCode,
    job_id: u64,
    payload: &T,
) -> Result<()> {
    let payload_bytes = serde_json::to_vec(payload)?;
    let msg = Message::new(opcode, job_id, payload_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create message: {}", e))?;
    let (header, body) = msg.pack()
        .map_err(|e| anyhow::anyhow!("Failed to pack message: {}", e))?;

    let mut multipart = ZmqMessage::from(header.to_vec());
    multipart.push_back(body.into());
    socket.send(multipart).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_worker_config() {
        let config = WorkerConfig {
            sentinel_addr: "tcp://localhost:5555".to_string(),
            parquet_root: PathBuf::from("/tmp/output"),
            worker_id: "test-worker".to_string(),
            shim_path: PathBuf::from("bridge_shim.py"),
            capabilities: vec!["plugin_a".to_string(), "plugin_b".to_string()],
            venvs_dir: None, // Use default
        };

        assert_eq!(config.sentinel_addr, "tcp://localhost:5555");
        assert_eq!(config.worker_id, "test-worker");
        assert_eq!(config.capabilities.len(), 2);
    }

    #[test]
    fn test_worker_config_default_capabilities() {
        let config = WorkerConfig {
            sentinel_addr: "tcp://localhost:5555".to_string(),
            parquet_root: PathBuf::from("/tmp/output"),
            worker_id: "test-worker".to_string(),
            shim_path: PathBuf::from("bridge_shim.py"),
            capabilities: vec![], // Empty means wildcard "*"
            venvs_dir: None,
        };

        assert!(config.capabilities.is_empty());
    }

    #[test]
    fn test_worker_config_custom_venvs_dir() {
        let config = WorkerConfig {
            sentinel_addr: "tcp://localhost:5555".to_string(),
            parquet_root: PathBuf::from("/tmp/output"),
            worker_id: "test-worker".to_string(),
            shim_path: PathBuf::from("bridge_shim.py"),
            capabilities: vec!["*".to_string()],
            venvs_dir: Some(PathBuf::from("/tmp/custom_venvs")),
        };

        assert_eq!(
            config.venvs_dir,
            Some(PathBuf::from("/tmp/custom_venvs"))
        );
    }

    #[test]
    fn test_truncate_hash() {
        assert_eq!(truncate_hash("abc"), "abc");
        assert_eq!(truncate_hash("123456789012"), "123456789012");
        assert_eq!(truncate_hash("1234567890123"), "123456789012");
        assert_eq!(truncate_hash("abcdefghijklmnop"), "abcdefghijkl");
    }

    // ========================================================================
    // WorkerError tests
    // ========================================================================

    #[test]
    fn test_worker_error_from_exit_code_permanent() {
        // Exit code 1 = permanent error
        let err = WorkerError::from_exit_code(1, "Invalid syntax");
        assert!(err.is_permanent());
        assert!(!err.is_transient());
        assert!(err.to_string().contains("Invalid syntax"));
    }

    #[test]
    fn test_worker_error_from_exit_code_transient() {
        // Exit code 2 = transient error
        let err = WorkerError::from_exit_code(2, "Network timeout");
        assert!(err.is_transient());
        assert!(!err.is_permanent());
        assert!(err.to_string().contains("Network timeout"));
    }

    #[test]
    fn test_worker_error_from_exit_code_other() {
        // Other exit codes default to transient (conservative)
        let err = WorkerError::from_exit_code(137, "");
        assert!(err.is_transient());
        assert!(err.to_string().contains("137"));
    }

    #[test]
    fn test_worker_error_from_signal() {
        let err = WorkerError::from_signal("SIGKILL");
        assert!(err.is_transient());
        assert!(err.to_string().contains("signal"));
    }

    #[test]
    fn test_worker_error_from_exit_code_truncates_long_stderr() {
        // Long stderr should be truncated
        let long_stderr = "x".repeat(1000);
        let err = WorkerError::from_exit_code(1, &long_stderr);
        assert!(err.to_string().len() < 700); // Should be truncated + some overhead
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn test_worker_error_variants() {
        let permanent = WorkerError::Permanent {
            message: "test".to_string(),
        };
        let transient = WorkerError::Transient {
            message: "test".to_string(),
        };

        assert!(permanent.is_permanent());
        assert!(!permanent.is_transient());
        assert!(transient.is_transient());
        assert!(!transient.is_permanent());
    }

    #[test]
    fn test_split_output_batches_quarantine() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("_cf_row_error", DataType::Utf8, true),
        ]));

        let ids = Int64Array::from(vec![1, 2, 3]);
        let errors = StringArray::from(vec![None, Some("bad"), Some("")]);
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(ids) as ArrayRef, Arc::new(errors) as ArrayRef],
        )
        .unwrap();

        let (valid, quarantine, quarantined, lineage_unavailable) =
            split_output_batches(42, &[&batch]).unwrap();
        let valid_rows: usize = valid.iter().map(|b| b.num_rows()).sum();
        let quarantine_rows: usize = quarantine.iter().map(|b| b.num_rows()).sum();

        assert_eq!(quarantined, 1);
        assert_eq!(lineage_unavailable, 1);
        assert_eq!(valid_rows, 2);
        assert_eq!(quarantine_rows, 1);

        let quarantine_batch = &quarantine[0];
        let schema = quarantine_batch.schema();
        assert!(schema.index_of("_error_msg").is_ok());
        assert!(schema.index_of("_violation_type").is_ok());
        assert!(schema.index_of("_cf_job_id").is_ok());
        assert!(schema.index_of("_output_row_index").is_ok());

        let error_idx = schema.index_of("_error_msg").unwrap();
        let errors = quarantine_batch
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(errors.value(0), "bad");

        let violation_idx = schema.index_of("_violation_type").unwrap();
        let violations = quarantine_batch
            .column(violation_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(violations.value(0), "parser");

        let job_id_idx = schema.index_of("_cf_job_id").unwrap();
        let job_ids = quarantine_batch
            .column(job_id_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(job_ids.value(0), "42");

        let index_idx = schema.index_of("_output_row_index").unwrap();
        let output_indices = quarantine_batch
            .column(index_idx)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(output_indices.value(0), 1);
    }

    #[test]
    fn test_split_output_batches_no_error_column() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let ids = Int64Array::from(vec![10, 20, 30]);
        let batch = RecordBatch::try_new(schema, vec![Arc::new(ids) as ArrayRef]).unwrap();

        let (valid, quarantine, quarantined, lineage_unavailable) =
            split_output_batches(1, &[&batch]).unwrap();
        let valid_rows: usize = valid.iter().map(|b| b.num_rows()).sum();

        assert_eq!(quarantined, 0);
        assert_eq!(lineage_unavailable, 0);
        assert_eq!(valid_rows, 3);
        assert!(quarantine.is_empty());
    }

    #[test]
    fn test_quarantine_artifacts_from_schema_def() {
        let schema_def = r#"[{"name":"id","data_type":"int64","nullable":false}]"#;
        let ids = Int64Array::from(vec![Some(1), None]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let validated = schema_validation::enforce_schema_on_batches(&[batch], schema_def, "output")
            .unwrap();
        let validated_refs: Vec<&RecordBatch> = validated.iter().collect();
        let (valid, quarantine, quarantined, _lineage_unavailable) =
            split_output_batches(7, &validated_refs).unwrap();

        assert_eq!(quarantined, 1);
        assert!(!valid.is_empty());
        assert!(!quarantine.is_empty());

        let mut outputs = Vec::new();
        outputs.push(OwnedOutput {
            name: "output".to_string(),
            table: None,
            batches: valid,
        });
        outputs.push(OwnedOutput {
            name: "output_quarantine".to_string(),
            table: None,
            batches: quarantine,
        });

        let plans = to_output_plans(&outputs);
        let dir = tempdir().unwrap();
        let sink_uri = format!("parquet://{}", dir.path().display());
        let artifacts = casparian_sinks::write_output_plan(&sink_uri, &plans, "job-123").unwrap();

        let mut names: Vec<&str> = artifacts.iter().map(|a| a.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["output", "output_quarantine"]);

        for artifact in artifacts {
            assert!(artifact.uri.starts_with("file://"));
            let path = std::path::Path::new(&artifact.uri["file://".len()..]);
            assert!(path.exists());
        }
    }

    #[test]
    fn test_quarantine_policy_disallowed() {
        let config = QuarantineConfig {
            allow_quarantine: false,
            max_quarantine_pct: 10.0,
            max_quarantine_count: None,
        };
        let reason = check_quarantine_policy("output", 1, 10, &config).unwrap();
        assert!(reason.contains("quarantine disabled"));
    }

    #[test]
    fn test_quarantine_source_row_from_cf_row_id() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("__cf_row_id", DataType::Int64, false),
            Field::new("_cf_row_error", DataType::Utf8, true),
        ]));

        let ids = Int64Array::from(vec![1, 2, 3]);
        let row_ids = Int64Array::from(vec![10, 11, 12]);
        let errors = StringArray::from(vec![None, Some("bad"), None]);
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(ids) as ArrayRef,
                Arc::new(row_ids) as ArrayRef,
                Arc::new(errors) as ArrayRef,
            ],
        )
        .unwrap();

        let (_valid, quarantine, quarantined, lineage_unavailable) =
            split_output_batches(9, &[&batch]).unwrap();
        assert_eq!(quarantined, 1);
        assert_eq!(lineage_unavailable, 0);
        assert_eq!(quarantine.len(), 1);

        let quarantine_batch = &quarantine[0];
        let schema = quarantine_batch.schema();
        assert!(schema.index_of("_source_row").is_ok());
        assert!(schema.index_of("_output_row_index").is_err());

        let source_idx = schema.index_of("_source_row").unwrap();
        let source_rows = quarantine_batch
            .column(source_idx)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(source_rows.value(0), 11);
    }

    #[test]
    fn test_quarantine_policy_pct_threshold() {
        let config = QuarantineConfig {
            allow_quarantine: true,
            max_quarantine_pct: 5.0,
            max_quarantine_count: None,
        };
        let reason = check_quarantine_policy("output", 6, 100, &config).unwrap();
        assert!(reason.contains("pct exceeded"));
    }

    #[test]
    fn test_quarantine_policy_count_threshold() {
        let config = QuarantineConfig {
            allow_quarantine: true,
            max_quarantine_pct: 100.0,
            max_quarantine_count: Some(2),
        };
        let reason = check_quarantine_policy("output", 3, 100, &config).unwrap();
        assert!(reason.contains("count exceeded"));
    }

    #[test]
    fn test_quarantine_config_from_sink_uri() {
        let config = resolve_quarantine_config(
            None,
            "parquet:///tmp/out?allow_quarantine=true&max_quarantine_pct=2.5&max_quarantine_count=7",
            "output",
        )
        .unwrap();
        assert!(config.allow_quarantine);
        assert_eq!(config.max_quarantine_pct, 2.5);
        assert_eq!(config.max_quarantine_count, Some(7));
    }

    #[test]
    fn test_quarantine_config_from_schema_def() {
        let schema_def = r#"{"name":"output","columns":[{"name":"id","data_type":"int64"}],"quarantine_config":{"allow_quarantine":true,"max_quarantine_pct":1.0}}"#;
        let config = resolve_quarantine_config(
            Some(schema_def),
            "parquet:///tmp/out",
            "output",
        )
        .unwrap();
        assert!(config.allow_quarantine);
        assert_eq!(config.max_quarantine_pct, 1.0);
    }
}

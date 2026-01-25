//! Worker Node
//!
//! Design principles:
//! - VenvManager created once at startup, reused for all jobs
//! - Socket owned directly (not Option) - created during connect
//! - run() consumes self - can only be called once (enforced at compile time)
//! - Jobs tracked with JoinHandles for cancellation and bounded concurrency
//! - Graceful shutdown via shutdown channel

use anyhow::Result;
use casparian_protocol::types::{
    self, ArtifactV1, DispatchCommand, HeartbeatStatus, JobStatus, ParsedSinkUri, RuntimeKind,
    SinkScheme,
};
use casparian_protocol::{
    metrics, schema_hash, table_name_with_schema, JobId, Message, OpCode, SinkMode,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use zmq::{Context, Socket};

use crate::bridge::BridgeError;
use crate::cancel::CancellationToken;
use crate::native_runtime::NativeSubprocessRuntime;
use crate::runtime::{PluginRuntime, PythonShimRuntime, RunContext};
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

    /// Permanent error with structured diagnostics.
    #[error("Permanent error (no retry): {message}")]
    PermanentWithDiagnostics {
        message: String,
        diagnostics: types::JobDiagnostics,
    },

    /// Transient error - may succeed on retry (e.g., network timeout, resource busy)
    #[error("Transient error (retry eligible): {message}")]
    Transient { message: String },

    /// Bridge communication error
    #[error("Bridge error: {0}")]
    Bridge(#[from] BridgeError),

    /// Internal worker error
    #[error("Worker error: {message}")]
    Internal {
        message: String,
        #[source]
        source: Option<anyhow::Error>,
    },
}

pub type WorkerResult<T> = std::result::Result<T, WorkerError>;

impl From<anyhow::Error> for WorkerError {
    fn from(err: anyhow::Error) -> Self {
        WorkerError::Internal {
            message: err.to_string(),
            source: Some(err),
        }
    }
}

impl WorkerError {
    /// Check if this error is transient (eligible for retry)
    pub fn is_transient(&self) -> bool {
        matches!(self, WorkerError::Transient { .. })
    }

    /// Check if this error is permanent (no retry)
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            WorkerError::Permanent { .. } | WorkerError::PermanentWithDiagnostics { .. }
        )
    }

    pub fn diagnostics(&self) -> Option<&types::JobDiagnostics> {
        match self {
            WorkerError::PermanentWithDiagnostics { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }

    fn internal(err: impl std::fmt::Display) -> Self {
        WorkerError::Internal {
            message: err.to_string(),
            source: None,
        }
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

#[derive(Debug, Deserialize)]
struct BridgeErrorPayload {
    retryable: Option<bool>,
    kind: Option<String>,
}

fn parse_bridge_retryable(message: &str) -> Option<bool> {
    const MARKER: &str = "Guest process error:";
    let payload = if let Some(idx) = message.find(MARKER) {
        message[idx + MARKER.len()..].trim()
    } else {
        message.trim()
    };

    if !payload.starts_with('{') {
        return None;
    }

    let parsed: BridgeErrorPayload = serde_json::from_str(payload).ok()?;
    parsed
        .retryable
        .or_else(|| parsed.kind.as_deref().map(|k| k == "transient"))
}

// ============================================================================
// Constants
// ============================================================================

/// Maximum concurrent jobs per worker
const MAX_CONCURRENT_JOBS: usize = 4;

/// Heartbeat interval (seconds) - worker sends heartbeat to Sentinel
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

/// Grace period before SIGKILL after SIGTERM (seconds)
const KILL_GRACE_PERIOD_SECS: u64 = 3;

/// Tracks an active job's thread handle and cancellation token.
struct ActiveJob {
    handle: JoinHandle<()>,
    cancel_token: CancellationToken,
}

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
    completion_rx: mpsc::Receiver<()>,
}

impl WorkerHandle {
    /// Request immediate shutdown (send signal only).
    pub fn shutdown_now(self) -> WorkerResult<()> {
        self.shutdown_tx.send(()).map_err(WorkerError::internal)
    }

    /// Request graceful shutdown and wait for completion (with timeout).
    pub fn shutdown_gracefully(self, timeout: Duration) -> WorkerResult<()> {
        let Self {
            shutdown_tx,
            completion_rx,
        } = self;
        shutdown_tx.send(()).map_err(WorkerError::internal)?;
        match completion_rx.recv_timeout(timeout) {
            Ok(()) => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Err(WorkerError::internal("worker shutdown timed out"))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(WorkerError::internal("worker shutdown channel closed"))
            }
        }
    }

    /// Request graceful shutdown with default timeout.
    pub fn shutdown(self) -> WorkerResult<()> {
        self.shutdown_gracefully(Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECS))
    }
}

/// Active worker with connected socket
pub struct Worker {
    config: WorkerConfig,
    context: Context,
    socket: Socket,
    venv_manager: Arc<VenvManager>, // VenvManager is now Sync (uses std::sync::Mutex internally)
    result_tx: mpsc::Sender<JobResult>,
    result_rx: mpsc::Receiver<JobResult>,
    shutdown_rx: mpsc::Receiver<()>,
    shutdown_complete_tx: Option<mpsc::Sender<()>>,
    /// Active jobs with their thread handles and cancellation tokens
    active_jobs: HashMap<JobId, ActiveJob>,
}

/// Result from a completed job
struct JobResult {
    job_id: JobId,
    receipt: types::JobReceipt,
}

impl Worker {
    /// Connect to sentinel and create worker.
    /// Returns (Worker, ShutdownHandle) - call run() on Worker, use handle for shutdown.
    pub fn connect(config: WorkerConfig) -> WorkerResult<(Self, WorkerHandle)> {
        Self::connect_inner(config).map_err(WorkerError::internal)
    }

    fn connect_inner(config: WorkerConfig) -> Result<(Self, WorkerHandle)> {
        // Initialize VenvManager once (now uses std::sync::Mutex internally)
        let venv_manager = match &config.venvs_dir {
            Some(path) => VenvManager::with_path(path.clone())?,
            None => VenvManager::new()?,
        };
        let (count, bytes) = venv_manager.stats();
        info!(
            "VenvManager: {} cached envs, {} MB",
            count,
            bytes / 1_000_000
        );

        // Create and connect socket
        let context = Context::new();
        let socket = context
            .socket(zmq::DEALER)
            .map_err(|err| anyhow::anyhow!("Failed to create DEALER socket: {}", err))?;
        socket
            .connect(&config.sentinel_addr)
            .map_err(|err| anyhow::anyhow!("Failed to connect to sentinel: {}", err))?;
        socket
            .set_rcvtimeo(100)
            .map_err(|err| anyhow::anyhow!("Failed to set socket receive timeout: {}", err))?;

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
        send_message(&socket, OpCode::Identify, JobId::new(0), &identify)?;
        info!("Sent IDENTIFY as {}", config.worker_id);

        // Initialize channels
        let (result_tx, result_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();

        let handle = WorkerHandle {
            shutdown_tx,
            completion_rx,
        };

        Ok((
            Self {
                config,
                context,
                socket,
                venv_manager: Arc::new(venv_manager),
                result_tx,
                result_rx,
                shutdown_rx,
                shutdown_complete_tx: Some(completion_tx),
                active_jobs: HashMap::new(),
            },
            handle,
        ))
    }

    /// Main event loop - consumes self (can only be called once)
    pub fn run(mut self) -> WorkerResult<()> {
        let completion_tx = self.shutdown_complete_tx.take();
        let result = self.run_inner().map_err(WorkerError::internal);
        if let Some(tx) = completion_tx {
            let _ = tx.send(());
        }
        result
    }

    fn run_inner(mut self) -> Result<()> {
        info!("Entering event loop...");

        let mut last_heartbeat = Instant::now();

        loop {
            // Clean up completed jobs
            self.reap_completed_jobs();

            match self.shutdown_rx.try_recv() {
                Ok(()) => {
                    info!(
                        "Shutdown signal received, waiting for {} active jobs...",
                        self.active_jobs.len()
                    );
                    self.wait_for_all_jobs();
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    warn!("Shutdown channel closed, stopping worker");
                    self.wait_for_all_jobs();
                    break;
                }
            }

            while let Ok(result) = self.result_rx.try_recv() {
                info!("Job {} finished, sending CONCLUDE", result.job_id);
                if let Err(e) = send_message(
                    &self.socket,
                    OpCode::Conclude,
                    result.job_id,
                    &result.receipt,
                ) {
                    error!("Failed to send CONCLUDE for job {}: {}", result.job_id, e);
                }
            }

            if last_heartbeat.elapsed() >= Duration::from_secs(HEARTBEAT_INTERVAL_SECS) {
                let active_job_ids: Vec<JobId> = self.active_jobs.keys().copied().collect();
                let status = if active_job_ids.is_empty() {
                    HeartbeatStatus::Idle
                } else {
                    HeartbeatStatus::Busy
                };
                let payload = types::HeartbeatPayload {
                    status,
                    active_job_count: active_job_ids.len(),
                    active_job_ids,
                };
                debug!(
                    "Sending heartbeat: {:?} ({} active jobs)",
                    status, payload.active_job_count
                );
                if let Err(e) =
                    send_message(&self.socket, OpCode::Heartbeat, JobId::new(0), &payload)
                {
                    warn!("Failed to send heartbeat: {}", e);
                }
                last_heartbeat = Instant::now();
            }

            match self.socket.recv_multipart(0) {
                Ok(parts) => {
                    let (header, payload) = match parts.len() {
                        2 => (parts[0].clone(), parts[1].clone()),
                        3 if parts[0].is_empty() => (parts[1].clone(), parts[2].clone()),
                        count => {
                            warn!("Expected 2 frames [header, payload], got {}", count);
                            continue;
                        }
                    };

                    match Message::unpack(&[header, payload]) {
                        Ok(msg) => {
                            if let Err(e) = self.handle_message(msg) {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Err(e) => warn!("Failed to unpack message: {}", e),
                    }
                }
                Err(zmq::Error::EAGAIN) => {}
                Err(e) => {
                    error!("ZMQ recv error: {}", e);
                    break;
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Remove completed job handles from active_jobs map
    fn reap_completed_jobs(&mut self) {
        let finished: Vec<JobId> = self
            .active_jobs
            .iter()
            .filter(|(_, active_job)| active_job.handle.is_finished())
            .map(|(job_id, _)| *job_id)
            .collect();

        for job_id in finished {
            if let Some(active_job) = self.active_jobs.remove(&job_id) {
                debug!("Reaped completed job {}", job_id);
                if let Err(err) = active_job.handle.join() {
                    warn!("Job {} thread panicked: {:?}", job_id, err);
                }
            }
        }
    }

    /// Wait for all active jobs to complete and send their CONCLUDE messages (for graceful shutdown)
    ///
    /// This is critical for graceful shutdown - we must:
    /// 1. Signal cancellation to all active jobs
    /// 2. Wait for all job tasks to complete (with timeout)
    /// 3. Drain any pending results from result_rx
    /// 4. Send CONCLUDE messages for all completed jobs
    ///
    /// Otherwise, the sentinel will never know jobs finished.
    /// Jobs that exceed the timeout are aborted; Sentinel's stale-worker cleanup handles them.
    fn wait_for_all_jobs(&mut self) {
        let job_count = self.active_jobs.len();
        info!(
            "Graceful shutdown: waiting for {} active jobs to complete...",
            job_count
        );

        // First, signal cancellation to all active jobs so they stop processing
        for (job_id, active_job) in &self.active_jobs {
            debug!("Signaling cancellation to job {}", job_id);
            active_job.cancel_token.cancel();
        }

        let shutdown_timeout = Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECS);
        let mut timed_out_jobs = Vec::new();

        // Wait for all job handles to complete (with per-job timeout)
        for (job_id, active_job) in self.active_jobs.drain() {
            debug!("Waiting for job {} to complete...", job_id);
            let start = Instant::now();
            loop {
                if active_job.handle.is_finished() {
                    match active_job.handle.join() {
                        Ok(()) => debug!("Job {} completed during shutdown", job_id),
                        Err(e) => warn!("Job {} task panicked during shutdown: {:?}", job_id, e),
                    }
                    break;
                }
                if start.elapsed() >= shutdown_timeout {
                    warn!(
                        "Job {} timed out during shutdown ({}s), aborting",
                        job_id, DEFAULT_SHUTDOWN_TIMEOUT_SECS
                    );
                    timed_out_jobs.push(job_id);
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        // Send explicit Aborted receipts for timed-out jobs so sentinel receives terminal receipts
        for job_id in &timed_out_jobs {
            warn!(
                "Shutdown: sending ABORTED receipt for timed-out job {}",
                job_id
            );
            let receipt = types::JobReceipt {
                status: JobStatus::Aborted,
                metrics: HashMap::new(),
                artifacts: vec![],
                error_message: Some(format!(
                    "Job aborted: shutdown timeout exceeded ({}s)",
                    DEFAULT_SHUTDOWN_TIMEOUT_SECS
                )),
                diagnostics: None,
                source_hash: None, // Not available for timed-out jobs
            };
            if let Err(e) = send_message(&self.socket, OpCode::Conclude, *job_id, &receipt) {
                error!(
                    "Failed to send ABORTED CONCLUDE for job {} during shutdown: {}",
                    job_id, e
                );
            }
        }

        if !timed_out_jobs.is_empty() {
            info!(
                "Shutdown: sent ABORTED receipts for {} timed-out jobs: {:?}",
                timed_out_jobs.len(),
                timed_out_jobs
            );
        }

        // Drain all pending results and send CONCLUDE messages
        // Jobs send results via result_tx, we must receive and forward them
        let mut concluded_count = 0;
        while let Ok(result) = self.result_rx.try_recv() {
            if timed_out_jobs.contains(&result.job_id) {
                debug!(
                    "Shutdown: skipping CONCLUDE for timed-out job {} (already aborted)",
                    result.job_id
                );
                continue;
            }
            info!(
                "Shutdown: sending CONCLUDE for job {} (status: {:?})",
                result.job_id, result.receipt.status
            );
            if let Err(e) = send_message(
                &self.socket,
                OpCode::Conclude,
                result.job_id,
                &result.receipt,
            ) {
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
    fn handle_message(&mut self, msg: Message) -> Result<()> {
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
                        diagnostics: None,
                        source_hash: None, // Not computed before rejection
                    };
                    send_message(&self.socket, OpCode::Conclude, job_id, &receipt)?;
                    return Ok(());
                }

                info!(
                    "DISPATCH job {} -> {} ({} active)",
                    job_id,
                    cmd.plugin_name,
                    self.active_jobs.len() + 1
                );

                // Create cancellation token for this job
                let cancel_token = CancellationToken::new();
                let cancel_token_clone = cancel_token.clone();

                // Clone what we need for the spawned task
                let tx = self.result_tx.clone();
                let venv_mgr = self.venv_manager.clone();
                let parquet_root = self.config.parquet_root.clone();
                let shim_path = self.config.shim_path.clone();

                let handle = std::thread::spawn(move || {
                    let receipt = execute_job(
                        job_id,
                        cmd,
                        venv_mgr,
                        parquet_root,
                        shim_path,
                        cancel_token_clone,
                    );
                    // If channel is closed, worker is shutting down - that's fine
                    let _ = tx.send(JobResult { job_id, receipt });
                });

                self.active_jobs.insert(
                    job_id,
                    ActiveJob {
                        handle,
                        cancel_token,
                    },
                );
            }

            OpCode::Heartbeat => {
                debug!("Received HEARTBEAT, replying...");
                let active_job_ids: Vec<JobId> = self.active_jobs.keys().copied().collect();
                let active_job_count = self.active_jobs.len();

                let status = if active_job_count == 0 {
                    HeartbeatStatus::Idle
                } else if active_job_count >= MAX_CONCURRENT_JOBS {
                    HeartbeatStatus::Busy // At capacity
                } else {
                    HeartbeatStatus::Alive // Working but can accept more
                };

                let payload = types::HeartbeatPayload {
                    status,
                    active_job_count,
                    active_job_ids,
                };
                send_message(&self.socket, OpCode::Heartbeat, JobId::new(0), &payload)?;
            }

            OpCode::Abort => {
                let job_id = msg.header.job_id;
                if let Some(active_job) = self.active_jobs.get(&job_id) {
                    warn!("ABORT job {} - signaling cancellation", job_id);
                    // Signal cancellation to the job - this will trigger subprocess termination
                    active_job.cancel_token.cancel();
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

fn build_schema_hashes(cmd: &DispatchCommand) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    for sink in &cmd.sinks {
        if let Some(schema) = sink.schema.as_ref() {
            if let Some(hash) = schema_hash(Some(schema)) {
                hashes.insert(sink.topic.clone(), hash);
            }
        }
    }
    hashes
}

/// Validate that an entrypoint path is safe (no path traversal).
///
/// Security checks:
/// 1. Reject absolute paths
/// 2. Reject paths with ".." (parent directory traversal)
/// 3. After joining with base, verify the canonicalized path stays within base
fn validate_entrypoint(entrypoint: &Path, base_dir: &Path) -> WorkerResult<PathBuf> {
    use std::path::Component;

    // Reject absolute paths
    if entrypoint.is_absolute() {
        return Err(WorkerError::Permanent {
            message: format!(
                "Entrypoint cannot be an absolute path: {:?}",
                entrypoint.display()
            ),
        });
    }

    // Reject paths with parent directory traversal (..)
    for component in entrypoint.components() {
        if matches!(component, Component::ParentDir) {
            return Err(WorkerError::Permanent {
                message: format!("Entrypoint cannot contain '..': {:?}", entrypoint.display()),
            });
        }
    }

    // Join and canonicalize
    let joined = base_dir.join(entrypoint);
    let canonical = joined.canonicalize().map_err(|e| WorkerError::Permanent {
        message: format!("Failed to resolve entrypoint '{}': {}", joined.display(), e),
    })?;
    let base_canonical = base_dir
        .canonicalize()
        .map_err(|e| WorkerError::Permanent {
            message: format!(
                "Failed to resolve base directory '{}': {}",
                base_dir.display(),
                e
            ),
        })?;

    // Verify the resolved path stays within the base directory
    if !canonical.starts_with(&base_canonical) {
        return Err(WorkerError::Permanent {
            message: format!(
                "Entrypoint escapes plugin directory: resolved '{}' is outside '{}'",
                canonical.display(),
                base_canonical.display()
            ),
        });
    }

    Ok(canonical)
}

fn resolve_entrypoint(cmd: &DispatchCommand) -> WorkerResult<String> {
    match cmd.runtime_kind {
        RuntimeKind::PythonShim => Ok(cmd.entrypoint.clone()),
        RuntimeKind::NativeExec => {
            let version = cmd
                .parser_version
                .as_deref()
                .ok_or_else(|| WorkerError::Permanent {
                    message: "parser_version is required for native plugins".to_string(),
                })?;
            let os = cmd
                .platform_os
                .as_deref()
                .ok_or_else(|| WorkerError::Permanent {
                    message: "platform_os is required for native plugins".to_string(),
                })?;
            let arch = cmd
                .platform_arch
                .as_deref()
                .ok_or_else(|| WorkerError::Permanent {
                    message: "platform_arch is required for native plugins".to_string(),
                })?;
            let base = casparian_home()?
                .join("plugins")
                .join(&cmd.plugin_name)
                .join(version)
                .join(os)
                .join(arch);

            // Validate entrypoint path for security (path traversal protection)
            let validated_path = validate_entrypoint(Path::new(&cmd.entrypoint), &base)?;

            Ok(validated_path.to_string_lossy().to_string())
        }
    }
}

fn allow_unsigned_native() -> WorkerResult<bool> {
    if let Ok(value) = std::env::var("CASPARIAN_ALLOW_UNSIGNED_NATIVE") {
        let normalized = value.trim().to_lowercase();
        return Ok(matches!(normalized.as_str(), "1" | "true" | "yes"));
    }

    let config_path = casparian_home()?.join("config.toml");
    if !config_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&config_path).map_err(WorkerError::internal)?;
    let parsed: toml::Value = toml::from_str(&content).map_err(WorkerError::internal)?;
    Ok(parsed
        .get("trust")
        .and_then(|trust| trust.get("allow_unsigned_native"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

/// Check if unsigned Python plugins are allowed.
///
/// Order of precedence:
/// 1. Environment variable CASPARIAN_ALLOW_UNSIGNED_PYTHON
/// 2. Config file trust.allow_unsigned_python
/// 3. Default: false (opt-in required)
fn allow_unsigned_python() -> WorkerResult<bool> {
    if let Ok(value) = std::env::var("CASPARIAN_ALLOW_UNSIGNED_PYTHON") {
        let normalized = value.trim().to_lowercase();
        return Ok(matches!(normalized.as_str(), "1" | "true" | "yes"));
    }

    let config_path = casparian_home()?.join("config.toml");
    if !config_path.exists() {
        return Ok(false); // Default false unless explicitly allowed
    }
    let content = std::fs::read_to_string(&config_path).map_err(WorkerError::internal)?;
    let parsed: toml::Value = toml::from_str(&content).map_err(WorkerError::internal)?;
    Ok(parsed
        .get("trust")
        .and_then(|trust| trust.get("allow_unsigned_python"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)) // Default false unless explicitly allowed
}

fn casparian_home() -> WorkerResult<PathBuf> {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return Ok(PathBuf::from(override_path));
    }
    dirs::home_dir()
        .map(|home| home.join(".casparian_flow"))
        .ok_or_else(|| WorkerError::internal("Could not determine home directory"))
}

const LINEAGE_COLUMNS: [&str; 4] = [
    "_cf_source_hash",
    "_cf_job_id",
    "_cf_processed_at",
    "_cf_parser_version",
];

fn inject_lineage_batches(
    output_name: &str,
    batches: Vec<RecordBatch>,
    source_hash: &str,
    job_id: &str,
    parser_version: &str,
) -> Result<Vec<casparian_sinks::OutputBatch>> {
    if batches.is_empty() {
        return Ok(Vec::new());
    }

    let total_batches = batches.len();
    let mut with_lineage = Vec::with_capacity(total_batches);
    let mut reserved: Vec<&'static str> = Vec::new();
    for batch in &batches {
        let schema = batch.schema();
        for col in LINEAGE_COLUMNS {
            if schema.index_of(col).is_ok() {
                reserved.push(col);
            }
        }
    }
    if !reserved.is_empty() {
        reserved.sort_unstable();
        reserved.dedup();
        anyhow::bail!(
            "Output '{}' contains reserved runtime lineage columns {:?}; parsers must not emit these columns.",
            output_name,
            reserved
        );
    }

    for batch in batches {
        let wrapped = casparian_sinks::OutputBatch::from_record_batch(batch);
        let injected =
            casparian_sinks::inject_lineage_columns(&wrapped, source_hash, job_id, parser_version)?;
        with_lineage.push(injected);
    }

    Ok(with_lineage)
}

/// Per-output status for multi-output jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStatus {
    /// All rows processed successfully
    Success,
    /// Some rows quarantined but within policy limits
    PartialSuccess,
    /// Output failed (e.g., quarantine policy exceeded)
    Failed,
}

/// Aggregate per-output statuses into a job status.
///
/// Rules:
/// - If ANY output is Failed → JobStatus::Failed
/// - If ANY output is PartialSuccess (and none Failed) → JobStatus::PartialSuccess
/// - If ALL outputs are Success → JobStatus::Success
fn aggregate_job_status(outputs: &[OutputMetrics]) -> JobStatus {
    if outputs.is_empty() {
        return JobStatus::Success;
    }

    let mut has_failed = false;
    let mut has_partial = false;

    for output in outputs {
        match output.status {
            OutputStatus::Failed => {
                has_failed = true;
                break; // Short-circuit: any failure means job fails
            }
            OutputStatus::PartialSuccess => {
                has_partial = true;
            }
            OutputStatus::Success => {}
        }
    }

    if has_failed {
        JobStatus::Failed
    } else if has_partial {
        JobStatus::PartialSuccess
    } else {
        JobStatus::Success
    }
}

struct OutputMetrics {
    name: String,
    rows: usize,
    quarantine_rows: usize,
    lineage_unavailable_rows: usize,
    status: OutputStatus,
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
        artifacts: Vec<ArtifactV1>,
        source_hash: String,
    },
    QuarantineRejected {
        metrics: ExecutionMetrics,
        reason: String,
        source_hash: String,
    },
    /// Job was cancelled during execution
    Cancelled { source_hash: Option<String> },
}

fn insert_execution_metrics(metrics: &mut HashMap<String, i64>, exec: &ExecutionMetrics) {
    metrics.insert(metrics::ROWS.to_string(), exec.rows as i64);
    metrics.insert(
        metrics::QUARANTINE_ROWS.to_string(),
        exec.quarantine_rows as i64,
    );
    metrics.insert(
        metrics::LINEAGE_UNAVAILABLE_ROWS.to_string(),
        exec.lineage_unavailable_rows as i64,
    );
    metrics.insert(metrics::OUTPUT_COUNT.to_string(), exec.outputs.len() as i64);

    // Per-output metrics including status
    for output in &exec.outputs {
        metrics.insert(
            metrics::rows_by_output_key(&output.name),
            output.rows as i64,
        );
        metrics.insert(
            metrics::quarantine_rows_by_output_key(&output.name),
            output.quarantine_rows as i64,
        );
        metrics.insert(
            metrics::lineage_unavailable_rows_by_output_key(&output.name),
            output.lineage_unavailable_rows as i64,
        );
        // Per-output status as numeric: 0=success, 1=partial_success, 2=failed
        let status_code = match output.status {
            OutputStatus::Success => 0,
            OutputStatus::PartialSuccess => 1,
            OutputStatus::Failed => 2,
        };
        metrics.insert(metrics::status_by_output_key(&output.name), status_code);
    }
}

fn is_default_sink(topic: &str) -> bool {
    topic == "*" || topic == "output"
}

fn select_sink_config<'a>(
    cmd: &'a DispatchCommand,
    output_name: &str,
) -> WorkerResult<Option<&'a types::SinkConfig>> {
    if let Some(exact) = cmd.sinks.iter().find(|sink| sink.topic == output_name) {
        return Ok(Some(exact));
    }
    if let Some(default) = cmd.sinks.iter().find(|sink| is_default_sink(&sink.topic)) {
        return Ok(Some(default));
    }
    if cmd.sinks.len() <= 1 {
        return Ok(cmd.sinks.first());
    }

    let topics = cmd
        .sinks
        .iter()
        .map(|sink| sink.topic.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(WorkerError::Permanent {
        message: format!(
            "Output '{}' has no sink config; configured topics: {}",
            output_name, topics
        ),
    })
}

fn resolve_quarantine_config(
    config: Option<&types::QuarantineConfig>,
) -> Result<types::QuarantineConfig> {
    let mut config = config.cloned().unwrap_or_default();
    if let Some(dir) = config.quarantine_dir.as_ref() {
        if dir.trim().is_empty() {
            config.quarantine_dir = None;
        }
    }
    validate_quarantine_config(&config)?;
    Ok(config)
}

fn sink_uri_for_quarantine(sink_uri: &str, quarantine_dir: Option<&str>) -> Result<String> {
    let Some(dir) = quarantine_dir else {
        return Ok(sink_uri.to_string());
    };
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Ok(sink_uri.to_string());
    }

    let query_suffix =
        sink_uri
            .split_once('?')
            .and_then(|(_, query)| if query.is_empty() { None } else { Some(query) });

    let parsed = ParsedSinkUri::parse(sink_uri)
        .map_err(|e| anyhow::anyhow!("invalid sink uri '{}': {}", sink_uri, e))?;

    let is_duckdb_file = matches!(parsed.scheme, SinkScheme::File)
        && parsed
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "duckdb" | "db"))
            .unwrap_or(false);

    if matches!(parsed.scheme, SinkScheme::Duckdb) || is_duckdb_file {
        anyhow::bail!("quarantine_dir is not supported for duckdb sinks");
    }

    let target_path = match parsed.scheme {
        SinkScheme::Parquet | SinkScheme::Csv => PathBuf::from(trimmed),
        SinkScheme::File => {
            let ext = parsed
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("parquet");
            let mut path = PathBuf::from(trimmed);
            path.push(format!("placeholder.{}", ext));
            path
        }
        SinkScheme::Duckdb => {
            unreachable!("duckdb handled above");
        }
    };

    let mut uri = format!("{}://{}", parsed.scheme.as_str(), target_path.display());
    if let Some(query) = query_suffix {
        uri.push('?');
        uri.push_str(query);
    }
    Ok(uri)
}

fn validate_quarantine_config(config: &types::QuarantineConfig) -> Result<()> {
    if !config.max_quarantine_pct.is_finite() {
        anyhow::bail!("max_quarantine_pct must be finite");
    }
    if config.max_quarantine_pct < 0.0 || config.max_quarantine_pct > 100.0 {
        anyhow::bail!("max_quarantine_pct must be between 0 and 100");
    }
    Ok(())
}

fn check_quarantine_policy(
    output_name: &str,
    quarantine_rows: u64,
    total_rows: u64,
    config: &types::QuarantineConfig,
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

fn quarantine_pct(quarantine_rows: u64, total_rows: u64) -> f64 {
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
fn execute_job(
    job_id: JobId,
    cmd: DispatchCommand,
    venv_manager: Arc<VenvManager>,
    parquet_root: PathBuf,
    shim_path: PathBuf,
    cancel_token: CancellationToken,
) -> types::JobReceipt {
    let span = tracing::info_span!(
        "worker.execute_job",
        job_id = %job_id,
        file_id = cmd.file_id,
        plugin = %cmd.plugin_name,
        runtime = ?cmd.runtime_kind,
        duration_ms = tracing::field::Empty
    );
    let _guard = span.enter();
    let start = Instant::now();

    // Check cancellation before starting
    if cancel_token.is_cancelled() {
        let receipt = types::JobReceipt {
            status: JobStatus::Aborted,
            metrics: HashMap::new(),
            artifacts: vec![],
            error_message: Some("Job cancelled before execution".to_string()),
            diagnostics: None,
            source_hash: None,
        };
        let duration_ms = start.elapsed().as_millis() as u64;
        span.record("duration_ms", &duration_ms);
        return receipt;
    }

    match execute_job_inner(
        job_id,
        &cmd,
        &venv_manager,
        &parquet_root,
        &shim_path,
        &cancel_token,
    ) {
        Ok(ExecutionOutcome::Success {
            metrics: exec_metrics,
            artifacts,
            source_hash,
        }) => {
            let mut metrics = HashMap::new();
            insert_execution_metrics(&mut metrics, &exec_metrics);

            // Use per-output status aggregation for multi-output jobs
            let aggregated_status = aggregate_job_status(&exec_metrics.outputs);

            let receipt = types::JobReceipt {
                status: aggregated_status,
                metrics,
                artifacts,
                error_message: None,
                diagnostics: None,
                source_hash: Some(source_hash),
            };
            let duration_ms = start.elapsed().as_millis() as u64;
            span.record("duration_ms", &duration_ms);
            receipt
        }
        Ok(ExecutionOutcome::QuarantineRejected {
            metrics: exec_metrics,
            reason,
            source_hash,
        }) => {
            let mut metrics = HashMap::new();
            insert_execution_metrics(&mut metrics, &exec_metrics);
            metrics.insert(metrics::IS_TRANSIENT.to_string(), 0);
            metrics.insert(metrics::QUARANTINE_REJECTED.to_string(), 1);

            let receipt = types::JobReceipt {
                status: JobStatus::Failed,
                metrics,
                artifacts: vec![],
                error_message: Some(reason),
                diagnostics: None,
                source_hash: Some(source_hash),
            };
            let duration_ms = start.elapsed().as_millis() as u64;
            span.record("duration_ms", &duration_ms);
            receipt
        }
        Ok(ExecutionOutcome::Cancelled { source_hash }) => {
            let receipt = types::JobReceipt {
                status: JobStatus::Aborted,
                metrics: HashMap::new(),
                artifacts: vec![],
                error_message: Some("Job cancelled during execution".to_string()),
                diagnostics: None,
                source_hash,
            };
            let duration_ms = start.elapsed().as_millis() as u64;
            span.record("duration_ms", &duration_ms);
            receipt
        }
        Err(worker_err) => {
            let is_transient = worker_err.is_transient();
            let error_message = worker_err.to_string();
            let diagnostics = worker_err.diagnostics().cloned();

            if is_transient {
                warn!(
                    "Job {} failed (transient, retry eligible): {}",
                    job_id, error_message
                );
            } else {
                error!(
                    "Job {} failed (permanent, no retry): {}",
                    job_id, error_message
                );
            }

            let mut metrics = HashMap::new();
            // Include error classification in metrics for Sentinel to read
            metrics.insert(
                metrics::IS_TRANSIENT.to_string(),
                if is_transient { 1 } else { 0 },
            );

            let receipt = types::JobReceipt {
                status: JobStatus::Failed,
                metrics,
                artifacts: vec![],
                error_message: Some(error_message),
                diagnostics,
                // Hash unavailable on early failure (e.g., file not found, venv setup failure)
                source_hash: None,
            };
            let duration_ms = start.elapsed().as_millis() as u64;
            span.record("duration_ms", &duration_ms);
            receipt
        }
    }
}

/// Execute a job, returning WorkerError with retry classification on failure
fn execute_job_inner(
    job_id: JobId,
    cmd: &DispatchCommand,
    venv_manager: &Arc<VenvManager>,
    parquet_root: &std::path::Path,
    shim_path: &std::path::Path,
    cancel_token: &CancellationToken,
) -> std::result::Result<ExecutionOutcome, WorkerError> {
    // Check cancellation early
    if cancel_token.is_cancelled() {
        return Ok(ExecutionOutcome::Cancelled { source_hash: None });
    }

    // Trust policy enforcement for native plugins
    if cmd.runtime_kind == RuntimeKind::NativeExec && !cmd.signature_verified {
        if !allow_unsigned_native().unwrap_or(false) {
            return Err(WorkerError::Permanent {
                message: "Unsigned native plugin blocked by trust policy".to_string(),
            });
        }
    }

    // Trust policy enforcement for Python plugins
    if cmd.runtime_kind == RuntimeKind::PythonShim && !cmd.signature_verified {
        if !allow_unsigned_python().unwrap_or(false) {
            return Err(WorkerError::Permanent {
                message: "Unsigned Python plugin blocked by trust policy. \
                          Set trust.allow_unsigned_python = true in config.toml to allow."
                    .to_string(),
            });
        }
        // Log warning for unsigned Python plugins in dev mode
        warn!(
            "Running unsigned Python plugin '{}' (dev mode). \
             Set trust.allow_unsigned_python = true to allow (default is false).",
            cmd.plugin_name
        );
    }

    let entrypoint = resolve_entrypoint(cmd)?;
    let schema_hashes = build_schema_hashes(cmd);
    let ctx = RunContext {
        job_id,
        file_id: cmd.file_id,
        entrypoint,
        env_hash: cmd.env_hash.clone(),
        source_code: cmd.source_code.clone(),
        schema_hashes,
    };

    let runtime: Box<dyn PluginRuntime> = match cmd.runtime_kind {
        RuntimeKind::PythonShim => Box::new(PythonShimRuntime::new(
            venv_manager.clone(),
            shim_path.to_path_buf(),
        )),
        RuntimeKind::NativeExec => Box::new(NativeSubprocessRuntime::new()),
    };

    // Check cancellation before running the plugin
    if cancel_token.is_cancelled() {
        return Ok(ExecutionOutcome::Cancelled { source_hash: None });
    }

    let run_outputs = match runtime.run_file(&ctx, Path::new(&cmd.file_path), cancel_token) {
        Ok(outputs) => outputs,
        Err(e) => {
            if cancel_token.is_cancelled() {
                return Ok(ExecutionOutcome::Cancelled { source_hash: None });
            }

            let error_message = e.to_string();
            if let Some(retryable) = parse_bridge_retryable(&error_message) {
                return Err(if retryable {
                    WorkerError::Transient {
                        message: error_message,
                    }
                } else {
                    WorkerError::Permanent {
                        message: error_message,
                    }
                });
            }

            // Classify bridge errors by examining the error message
            let error_str = error_message.to_lowercase();

            // Permanent errors: syntax errors, import errors, schema violations
            let worker_err = if error_str.contains("syntaxerror")
                || error_str.contains("importerror")
                || error_str.contains("modulenotfounderror")
                || error_str.contains("schema")
                || error_str.contains("exited with exit status: 1")
            {
                WorkerError::Permanent {
                    message: error_message,
                }
            }
            // Exit code 2 explicitly indicates transient
            else if error_str.contains("exited with exit status: 2") {
                WorkerError::Transient {
                    message: error_message,
                }
            }
            // Transient errors: timeouts, network issues, resource unavailable
            else if error_str.contains("timeout")
                || error_str.contains("connection")
                || error_str.contains("resource")
                || error_str.contains("signal")
            {
                WorkerError::Transient {
                    message: error_message,
                }
            }
            // Default to transient (conservative - allow retry)
            else {
                WorkerError::Transient {
                    message: error_message,
                }
            };

            return Err(worker_err);
        }
    };

    let output_batches = run_outputs.output_batches;

    // Log the captured logs for debugging (will be stored in DB in Phase 3)
    if !run_outputs.logs.is_empty() {
        debug!(
            "Job {} logs ({} bytes):\n{}",
            job_id,
            run_outputs.logs.len(),
            run_outputs.logs
        );
    }

    // Check cancellation after plugin execution, before writing to sinks
    // This prevents committing outputs for cancelled jobs
    if cancel_token.is_cancelled() {
        let source_hash = compute_source_hash(&cmd.file_path).ok();
        info!(
            "Job {} cancelled after plugin execution, not writing outputs",
            job_id
        );
        return Ok(ExecutionOutcome::Cancelled { source_hash });
    }

    let default_sink = format!("parquet://{}", parquet_root.display());
    let sink_uri = cmd
        .sinks
        .first()
        .map(|s| s.uri.as_str())
        .unwrap_or(default_sink.as_str());

    let descriptors: Vec<casparian_sinks::OutputDescriptor> = run_outputs
        .output_info
        .iter()
        .map(|info| casparian_sinks::OutputDescriptor {
            name: info.name.clone(),
            table: info.table.clone(),
        })
        .collect();

    let outputs =
        casparian_sinks::plan_outputs(&descriptors, &output_batches, "output").map_err(|e| {
            WorkerError::Permanent {
                message: e.to_string(),
            }
        })?;

    let job_id_str = job_id.to_string();
    let source_hash =
        compute_source_hash(&cmd.file_path).map_err(|err| WorkerError::Permanent {
            message: format!(
                "Failed to compute source hash for '{}': {}",
                cmd.file_path, err
            ),
        })?;
    let parser_version = cmd.parser_version.as_deref().unwrap_or("unknown");

    let mut total_rows = 0;
    let mut quarantine_rows = 0;
    let mut lineage_unavailable_rows = 0;
    let mut artifacts: Vec<ArtifactV1> = Vec::new();
    let mut output_metrics = Vec::new();
    let mut policy_failures = Vec::new();

    let mut owned_outputs = Vec::new();

    for output in outputs {
        let output_name = output.name().to_string();
        let mut output_table = output.table().map(|table| table.to_string());
        let sink_config = select_sink_config(cmd, &output_name)?;
        let sink_mode = sink_config
            .map(|sink| sink.mode)
            .unwrap_or(SinkMode::Append);
        let schema_def = sink_config.and_then(|sink| sink.schema.as_ref());
        let schema_hash_value = schema_hash(schema_def);
        if schema_hash_value.is_some() {
            let base = output_table.as_deref().unwrap_or(&output_name);
            output_table = Some(table_name_with_schema(base, schema_hash_value.as_deref()));
        }
        let sink_uri_for_config = sink_config
            .map(|sink| sink.uri.as_str())
            .unwrap_or(sink_uri);
        let sink_uri_for_output = sink_uri_for_config.to_string();

        let mut output_batches: Vec<RecordBatch> = output
            .batches()
            .iter()
            .map(|batch| batch.as_record_batch().clone())
            .collect();
        if let Some(schema_def) = schema_def {
            output_batches = match schema_validation::enforce_schema_on_batches(
                &output_batches,
                schema_def,
                &output_name,
            ) {
                Ok(batches) => batches,
                Err(err) => {
                    return Err(match err {
                        schema_validation::SchemaValidationError::SchemaMismatch {
                            mismatch,
                            ..
                        } => {
                            let summary = schema_validation::summarize_schema_mismatch(&mismatch);
                            WorkerError::PermanentWithDiagnostics {
                                message: summary,
                                diagnostics: types::JobDiagnostics {
                                    schema_mismatch: Some(mismatch),
                                },
                            }
                        }
                        schema_validation::SchemaValidationError::InvalidSchemaDef { message } => {
                            WorkerError::Permanent {
                                message: format!(
                                    "schema validation failed for '{}': {}",
                                    output_name, message
                                ),
                            }
                        }
                    });
                }
            };
        }

        let output_batch_refs: Vec<&RecordBatch> = output_batches.iter().collect();
        let (valid_batches, quarantine_batches, quarantined, lineage_unavailable) =
            split_output_batches(job_id, &output_batch_refs).map_err(|e| {
                WorkerError::Permanent {
                    message: e.to_string(),
                }
            })?;
        let valid_rows: usize = valid_batches.iter().map(|batch| batch.num_rows()).sum();
        let output_rows = valid_rows + quarantined;
        total_rows += valid_rows;
        quarantine_rows += quarantined;
        lineage_unavailable_rows += lineage_unavailable;

        let quarantine_config =
            resolve_quarantine_config(sink_config.and_then(|sink| sink.quarantine_config.as_ref()))
                .map_err(|e| WorkerError::Permanent {
                    message: format!("invalid quarantine config for '{}': {}", output_name, e),
                })?;
        let quarantine_sink_uri = sink_uri_for_quarantine(
            &sink_uri_for_output,
            quarantine_config.quarantine_dir.as_deref(),
        )
        .map_err(|e| WorkerError::Permanent {
            message: format!("invalid quarantine_dir for '{}': {}", output_name, e),
        })?;
        let quarantined_u64 = u64::try_from(quarantined).map_err(|_| WorkerError::Permanent {
            message: format!("quarantine row count overflow for '{}'", output_name),
        })?;
        let output_rows_u64 = u64::try_from(output_rows).map_err(|_| WorkerError::Permanent {
            message: format!("output row count overflow for '{}'", output_name),
        })?;

        // Determine per-output status based on quarantine policy
        let output_status = if let Some(reason) = check_quarantine_policy(
            &output_name,
            quarantined_u64,
            output_rows_u64,
            &quarantine_config,
        ) {
            policy_failures.push(reason);
            OutputStatus::Failed
        } else if quarantined > 0 {
            OutputStatus::PartialSuccess
        } else {
            OutputStatus::Success
        };

        output_metrics.push(OutputMetrics {
            name: output_name.clone(),
            rows: valid_rows,
            quarantine_rows: quarantined,
            lineage_unavailable_rows: lineage_unavailable,
            status: output_status,
        });

        if !valid_batches.is_empty() {
            let lineage_batches = inject_lineage_batches(
                &output_name,
                valid_batches,
                &source_hash,
                &job_id_str,
                parser_version,
            )
            .map_err(|e| WorkerError::Permanent {
                message: format!("lineage injection failed for '{}': {}", output_name, e),
            })?;
            owned_outputs.push(OwnedOutput {
                name: output_name.clone(),
                table: output_table.clone(),
                batches: lineage_batches,
                sink_uri: sink_uri_for_output.clone(),
                sink_mode,
                is_quarantine: false,
                schema_hash: schema_hash_value.clone(),
            });
        }
        if !quarantine_batches.is_empty() {
            let quarantine_name = format!("{}_quarantine", output_name);
            let quarantine_table = output_table
                .as_ref()
                .map(|table| format!("{}_quarantine", table));
            let quarantine_batches = quarantine_batches
                .into_iter()
                .map(casparian_sinks::OutputBatch::from_record_batch)
                .collect();
            owned_outputs.push(OwnedOutput {
                name: quarantine_name,
                table: quarantine_table,
                batches: quarantine_batches,
                sink_uri: quarantine_sink_uri,
                sink_mode: SinkMode::Append,
                is_quarantine: true,
                schema_hash: None,
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
            source_hash,
        });
    }

    if !owned_outputs.is_empty() {
        let output_meta: HashMap<String, (Option<String>, bool, Option<String>)> = owned_outputs
            .iter()
            .map(|output| {
                (
                    output.name.clone(),
                    (
                        output.table.clone(),
                        output.is_quarantine,
                        output.schema_hash.clone(),
                    ),
                )
            })
            .collect();
        let written = match write_outputs_grouped(owned_outputs, &job_id_str, cancel_token) {
            Ok(written) => written,
            Err(err) => {
                if cancel_token.is_cancelled() {
                    return Ok(ExecutionOutcome::Cancelled {
                        source_hash: Some(source_hash),
                    });
                }
                return Err(err);
            }
        };
        for output in written {
            let (table, is_quarantine, schema_hash) = output_meta
                .get(&output.name)
                .cloned()
                .unwrap_or((None, false, None));

            let rows = Some(output.rows);
            if is_quarantine {
                artifacts.push(ArtifactV1::Quarantine {
                    output_name: output.name.clone(),
                    sink_uri: output.uri,
                    table,
                    rows,
                });
            } else {
                artifacts.push(ArtifactV1::Output {
                    output_name: output.name.clone(),
                    sink_uri: output.uri,
                    table,
                    rows,
                    schema_hash,
                });
            }
        }
    }

    info!(
        "Job {} complete: {} rows ({} quarantined)",
        job_id, total_rows, quarantine_rows
    );
    Ok(ExecutionOutcome::Success {
        metrics: exec_metrics,
        artifacts,
        source_hash,
    })
}

struct OwnedOutput {
    name: String,
    table: Option<String>,
    batches: Vec<casparian_sinks::OutputBatch>,
    sink_uri: String,
    sink_mode: SinkMode,
    is_quarantine: bool,
    schema_hash: Option<String>,
}

fn to_output_plans(outputs: &[OwnedOutput]) -> Vec<casparian_sinks::OutputPlan> {
    outputs
        .iter()
        .map(|output| {
            casparian_sinks::OutputPlan::new(
                output.name.clone(),
                output.table.clone(),
                output.batches.clone(),
                output.sink_mode,
            )
        })
        .collect()
}

fn write_outputs_grouped(
    outputs: Vec<OwnedOutput>,
    job_id: &str,
    cancel_token: &CancellationToken,
) -> WorkerResult<Vec<casparian_sinks::OutputArtifact>> {
    let mut grouped: HashMap<String, Vec<OwnedOutput>> = HashMap::new();
    for output in outputs {
        grouped
            .entry(output.sink_uri.clone())
            .or_default()
            .push(output);
    }

    let mut sink_uris: Vec<String> = grouped.keys().cloned().collect();
    sink_uris.sort();

    let mut artifacts = Vec::new();
    for sink_uri in sink_uris {
        let mut group = grouped.remove(&sink_uri).unwrap_or_default();
        group.sort_by(|a, b| a.name.cmp(&b.name));
        let plans = to_output_plans(&group);
        let should_commit = || !cancel_token.is_cancelled();
        let written =
            casparian_sinks::write_output_plan(&sink_uri, &plans, job_id, Some(&should_commit))
                .map_err(|e| WorkerError::Transient {
                    message: e.to_string(),
                })?;
        artifacts.extend(written);
    }

    Ok(artifacts)
}

fn split_output_batches(
    job_id: JobId,
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
    job_id: JobId,
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
    Ok((
        RecordBatch::try_new(schema, columns)?,
        lineage_unavailable_rows,
    ))
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

fn build_job_id_array(job_id: JobId, rows: usize) -> ArrayRef {
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
            Ok(SourceRowStatus::Valid(
                Arc::new(Int64Array::from(values)) as ArrayRef
            ))
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
            Ok(SourceRowStatus::Valid(
                Arc::new(Int64Array::from(values)) as ArrayRef
            ))
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
fn send_message<T: serde::Serialize>(
    socket: &Socket,
    opcode: OpCode,
    job_id: JobId,
    payload: &T,
) -> Result<()> {
    let payload_bytes = serde_json::to_vec(payload)?;
    let msg = Message::new(opcode, job_id, payload_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create message: {}", e))?;
    let (header, body) = msg
        .pack()
        .map_err(|e| anyhow::anyhow!("Failed to pack message: {}", e))?;

    let frames = [header.as_ref(), body.as_slice()];
    socket
        .send_multipart(&frames, 0)
        .map_err(|e| anyhow::anyhow!("ZMQ send error: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_dispatch_command(sinks: Vec<types::SinkConfig>) -> DispatchCommand {
        DispatchCommand {
            plugin_name: "test_plugin".to_string(),
            parser_version: Some("v1".to_string()),
            file_path: "/tmp/input.csv".to_string(),
            sinks,
            file_id: 1,
            runtime_kind: types::RuntimeKind::PythonShim,
            entrypoint: "test_plugin.py:Handler".to_string(),
            platform_os: None,
            platform_arch: None,
            signature_verified: false,
            signer_id: None,
            env_hash: Some("env_hash_test".to_string()),
            source_code: Some("print('ok')".to_string()),
            artifact_hash: "artifact_hash_test".to_string(),
        }
    }

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

        assert_eq!(config.venvs_dir, Some(PathBuf::from("/tmp/custom_venvs")));
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
            split_output_batches(JobId::new(42), &[&batch]).unwrap();
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
            split_output_batches(JobId::new(1), &[&batch]).unwrap();
        let valid_rows: usize = valid.iter().map(|b| b.num_rows()).sum();

        assert_eq!(quarantined, 0);
        assert_eq!(lineage_unavailable, 0);
        assert_eq!(valid_rows, 3);
        assert!(quarantine.is_empty());
    }

    #[test]
    fn conf_t1_worker_rejects_reserved_lineage_columns() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("_cf_source_hash", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![1])) as ArrayRef,
                Arc::new(StringArray::from(vec![Some("spoof")])) as ArrayRef,
            ],
        )
        .unwrap();

        let err = inject_lineage_batches(
            "output",
            vec![batch],
            "src-hash",
            "job-1",
            "1.0.0",
        )
        .expect_err("expected reserved lineage rejection");
        assert!(
            err.to_string().contains("reserved runtime lineage columns"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn conf_t1_quarantine_writes_outputs_and_quarantine() {
        let schema_def = types::SchemaDefinition {
            columns: vec![types::SchemaColumnSpec {
                name: "id".to_string(),
                data_type: casparian_protocol::DataType::Int64,
                nullable: false,
                format: None,
            }],
        };
        let ids = Int64Array::from(vec![Some(1), None]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let validated =
            schema_validation::enforce_schema_on_batches(&[batch], &schema_def, "output").unwrap();
        let validated_refs: Vec<&RecordBatch> = validated.iter().collect();
        let (valid, quarantine, quarantined, _lineage_unavailable) =
            split_output_batches(JobId::new(7), &validated_refs).unwrap();

        assert_eq!(quarantined, 1);
        assert!(!valid.is_empty());
        assert!(!quarantine.is_empty());

        let mut outputs = Vec::new();
        let valid_batches = valid
            .into_iter()
            .map(casparian_sinks::OutputBatch::from_record_batch)
            .collect();
        outputs.push(OwnedOutput {
            name: "output".to_string(),
            table: None,
            batches: valid_batches,
            sink_uri: "parquet://./output".to_string(),
            sink_mode: SinkMode::Append,
            is_quarantine: false,
            schema_hash: None,
        });
        let quarantine_batches = quarantine
            .into_iter()
            .map(casparian_sinks::OutputBatch::from_record_batch)
            .collect();
        outputs.push(OwnedOutput {
            name: "output_quarantine".to_string(),
            table: None,
            batches: quarantine_batches,
            sink_uri: "parquet://./output".to_string(),
            sink_mode: SinkMode::Append,
            is_quarantine: true,
            schema_hash: None,
        });

        let plans = to_output_plans(&outputs);
        let dir = tempdir().unwrap();
        let sink_uri = format!("parquet://{}", dir.path().display());
        let artifacts =
            casparian_sinks::write_output_plan(&sink_uri, &plans, "job-123", None).unwrap();

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
    fn test_write_outputs_grouped_routes_by_sink() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch_one = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef],
        )
        .unwrap();
        let batch_two = RecordBatch::try_new(
            schema,
            vec![Arc::new(Int64Array::from(vec![3])) as ArrayRef],
        )
        .unwrap();

        let dir_one = tempdir().unwrap();
        let dir_two = tempdir().unwrap();
        let sink_one = format!("parquet://{}", dir_one.path().display());
        let sink_two = format!("parquet://{}", dir_two.path().display());

        let outputs = vec![
            OwnedOutput {
                name: "alpha".to_string(),
                table: None,
                batches: vec![casparian_sinks::OutputBatch::from_record_batch(batch_one)],
                sink_uri: sink_one,
                sink_mode: SinkMode::Append,
                is_quarantine: false,
                schema_hash: None,
            },
            OwnedOutput {
                name: "beta".to_string(),
                table: None,
                batches: vec![casparian_sinks::OutputBatch::from_record_batch(batch_two)],
                sink_uri: sink_two,
                sink_mode: SinkMode::Append,
                is_quarantine: false,
                schema_hash: None,
            },
        ];

        let token = CancellationToken::new();
        let artifacts = write_outputs_grouped(outputs, "job-xyz", &token).unwrap();
        assert_eq!(artifacts.len(), 2);

        let mut paths = HashMap::new();
        for artifact in artifacts {
            assert!(artifact.uri.starts_with("file://"));
            let path = std::path::Path::new(&artifact.uri["file://".len()..]).to_path_buf();
            assert!(path.exists());
            paths.insert(artifact.name, path);
        }

        assert!(paths.get("alpha").unwrap().starts_with(dir_one.path()));
        assert!(paths.get("beta").unwrap().starts_with(dir_two.path()));
    }

    #[test]
    fn test_select_sink_config_exact_match() {
        let cmd = make_dispatch_command(vec![
            types::SinkConfig {
                topic: "alpha".to_string(),
                uri: "parquet:///tmp/alpha".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
            types::SinkConfig {
                topic: "beta".to_string(),
                uri: "parquet:///tmp/beta".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
        ]);

        let selected = select_sink_config(&cmd, "beta").unwrap().unwrap();
        assert_eq!(selected.uri, "parquet:///tmp/beta");
    }

    #[test]
    fn test_select_sink_config_default_wildcard() {
        let cmd = make_dispatch_command(vec![
            types::SinkConfig {
                topic: "alpha".to_string(),
                uri: "parquet:///tmp/alpha".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
            types::SinkConfig {
                topic: "*".to_string(),
                uri: "parquet:///tmp/default".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
        ]);

        let selected = select_sink_config(&cmd, "gamma").unwrap().unwrap();
        assert_eq!(selected.uri, "parquet:///tmp/default");
    }

    #[test]
    fn test_select_sink_config_single_sink_fallback() {
        let cmd = make_dispatch_command(vec![types::SinkConfig {
            topic: "alpha".to_string(),
            uri: "parquet:///tmp/alpha".to_string(),
            mode: types::SinkMode::Append,
            quarantine_config: None,
            schema: None,
        }]);

        let selected = select_sink_config(&cmd, "gamma").unwrap().unwrap();
        assert_eq!(selected.uri, "parquet:///tmp/alpha");
    }

    #[test]
    fn test_select_sink_config_requires_explicit_default() {
        let cmd = make_dispatch_command(vec![
            types::SinkConfig {
                topic: "alpha".to_string(),
                uri: "parquet:///tmp/alpha".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
            types::SinkConfig {
                topic: "beta".to_string(),
                uri: "parquet:///tmp/beta".to_string(),
                mode: types::SinkMode::Append,
                quarantine_config: None,
                schema: None,
            },
        ]);

        let err = select_sink_config(&cmd, "gamma").unwrap_err();
        assert!(err.to_string().contains("no sink config"));
    }

    #[test]
    fn test_quarantine_policy_disallowed() {
        let config = types::QuarantineConfig {
            allow_quarantine: false,
            max_quarantine_pct: 10.0,
            max_quarantine_count: None,
            quarantine_dir: None,
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
            split_output_batches(JobId::new(9), &[&batch]).unwrap();
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
        let config = types::QuarantineConfig {
            allow_quarantine: true,
            max_quarantine_pct: 5.0,
            max_quarantine_count: None,
            quarantine_dir: None,
        };
        let reason = check_quarantine_policy("output", 6, 100, &config).unwrap();
        assert!(reason.contains("pct exceeded"));
    }

    #[test]
    fn test_quarantine_policy_count_threshold() {
        let config = types::QuarantineConfig {
            allow_quarantine: true,
            max_quarantine_pct: 100.0,
            max_quarantine_count: Some(2),
            quarantine_dir: None,
        };
        let reason = check_quarantine_policy("output", 3, 100, &config).unwrap();
        assert!(reason.contains("count exceeded"));
    }

    #[test]
    fn test_resolve_quarantine_config_defaults() {
        let config = resolve_quarantine_config(None).unwrap();
        assert!(!config.allow_quarantine);
        assert_eq!(config.max_quarantine_pct, 10.0);
        assert_eq!(config.max_quarantine_count, None);
    }

    #[test]
    fn test_quarantine_dir_overrides_sink_uri() {
        let uri = sink_uri_for_quarantine("parquet:///tmp/out", Some("/tmp/quarantine")).unwrap();
        assert_eq!(uri, "parquet:///tmp/quarantine");
    }

    #[test]
    fn test_quarantine_dir_preserves_query_params() {
        let uri = sink_uri_for_quarantine(
            "parquet:///tmp/out?compression=zstd&row_group_size=1000",
            Some("/tmp/quarantine"),
        )
        .unwrap();
        assert_eq!(
            uri,
            "parquet:///tmp/quarantine?compression=zstd&row_group_size=1000"
        );
    }

    #[test]
    fn test_quarantine_dir_rejects_duckdb() {
        let err = sink_uri_for_quarantine("duckdb:///tmp/out.db", Some("/tmp/quarantine"));
        assert!(err.is_err());
    }

    #[test]
    fn test_quarantine_dir_preserves_file_extension() {
        let uri = sink_uri_for_quarantine("file:///tmp/out.csv", Some("/tmp/quarantine")).unwrap();
        assert!(uri.starts_with("file:///tmp/quarantine/"));
        assert!(uri.ends_with(".csv"));
    }

    // ========================================================================
    // Path traversal security tests (WS6-01)
    // ========================================================================

    #[test]
    fn test_validate_entrypoint_rejects_absolute_path() {
        let temp = tempdir().unwrap();
        let base = temp.path();

        let result = validate_entrypoint(Path::new("/bin/sh"), base);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .to_string()
            .contains("Entrypoint cannot be an absolute path"));
    }

    #[test]
    fn test_validate_entrypoint_rejects_parent_dir_traversal() {
        let temp = tempdir().unwrap();
        let base = temp.path();

        // Try various path traversal patterns
        let traversal_attempts = [
            "../etc/passwd",
            "foo/../../../etc/passwd",
            "subdir/../../other",
        ];

        for attempt in traversal_attempts {
            let result = validate_entrypoint(Path::new(attempt), base);
            assert!(result.is_err(), "Expected rejection for path: {}", attempt);
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("cannot contain '..'"),
                "Wrong error for {}: {}",
                attempt,
                err
            );
        }
    }

    #[test]
    fn test_validate_entrypoint_accepts_valid_relative_path() {
        let temp = tempdir().unwrap();
        let base = temp.path();

        // Create a valid entrypoint file
        let subdir = base.join("bin");
        std::fs::create_dir_all(&subdir).unwrap();
        let entrypoint_file = subdir.join("plugin");
        std::fs::write(&entrypoint_file, "#!/bin/sh\necho hello").unwrap();

        // Should succeed for valid relative path
        let result = validate_entrypoint(Path::new("bin/plugin"), base);
        assert!(
            result.is_ok(),
            "Valid path should be accepted: {:?}",
            result.err()
        );
        let canonical = result.unwrap();
        assert!(canonical.starts_with(base.canonicalize().unwrap()));
    }

    #[test]
    fn test_validate_entrypoint_rejects_symlink_escape() {
        let temp = tempdir().unwrap();
        let base = temp.path().join("plugin_dir");
        std::fs::create_dir_all(&base).unwrap();

        // Create a symlink that points outside the base directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let escape_link = base.join("escape");
            symlink("/tmp", &escape_link).unwrap();

            // Attempting to use the symlink to escape should fail
            // (assuming /tmp exists and is different from our temp dir)
            let result = validate_entrypoint(Path::new("escape"), &base);
            // This will either fail at canonicalize (if /tmp/escape doesn't exist)
            // or fail the starts_with check
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_validate_entrypoint_current_dir_reference() {
        let temp = tempdir().unwrap();
        let base = temp.path();

        // Create entrypoint file
        let entrypoint_file = base.join("plugin.py");
        std::fs::write(&entrypoint_file, "print('hello')").unwrap();

        // Paths with current dir references should work
        let result = validate_entrypoint(Path::new("./plugin.py"), base);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_entrypoint_nonexistent_file() {
        let temp = tempdir().unwrap();
        let base = temp.path();

        // Should fail with canonicalize error for non-existent file
        let result = validate_entrypoint(Path::new("nonexistent.py"), base);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to resolve entrypoint"));
    }
}

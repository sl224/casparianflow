//! Worker Node
//!
//! Design principles:
//! - VenvManager created once at startup, reused for all jobs
//! - Socket owned directly (not Option) - created during connect
//! - run() consumes self - can only be called once (enforced at compile time)
//! - Jobs tracked with JoinHandles for cancellation and bounded concurrency
//! - Graceful shutdown via shutdown channel

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use cf_protocol::types::{self, DispatchCommand, JobStatus, PrepareEnvCommand};
use cf_protocol::{Message, OpCode};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::bridge::{self, BridgeConfig};
use crate::venv_manager::VenvManager;

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
                    let status = if active_job_ids.is_empty() { "IDLE" } else { "BUSY" };
                    let payload = types::HeartbeatPayload {
                        status: status.to_string(),
                        current_job_id: active_job_ids.first().copied(),
                        active_job_count: active_job_ids.len(),
                        active_job_ids,
                    };
                    debug!("Sending heartbeat: {} ({} active jobs)", status, payload.active_job_count);
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
                    "IDLE"
                } else if active_job_count >= MAX_CONCURRENT_JOBS {
                    "BUSY"  // At capacity
                } else {
                    "ALIVE" // Working but can accept more
                };

                let payload = types::HeartbeatPayload {
                    status: status.to_string(),
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

/// Execute a job and return receipt
async fn execute_job(
    job_id: u64,
    cmd: DispatchCommand,
    venv_manager: Arc<VenvManager>,
    parquet_root: PathBuf,
    shim_path: PathBuf,
) -> types::JobReceipt {
    match execute_job_inner(job_id, &cmd, &venv_manager, &parquet_root, &shim_path).await {
        Ok((rows, artifacts)) => {
            let mut metrics = HashMap::new();
            metrics.insert("rows".to_string(), rows as i64);

            types::JobReceipt {
                status: JobStatus::Success,
                metrics,
                artifacts,
                error_message: None,
            }
        }
        Err(e) => {
            error!("Job {} failed: {}", job_id, e);
            types::JobReceipt {
                status: JobStatus::Failed,
                metrics: HashMap::new(),
                artifacts: vec![],
                error_message: Some(e.to_string()),
            }
        }
    }
}

async fn execute_job_inner(
    job_id: u64,
    cmd: &DispatchCommand,
    venv_manager: &VenvManager,
    parquet_root: &std::path::Path,
    shim_path: &std::path::Path,
) -> Result<(usize, Vec<HashMap<String, String>>)> {
    // Determine interpreter path
    let interpreter = if cmd.env_hash == "system" {
        // Use system Python for legacy plugins without lockfile
        which::which("python3")
            .or_else(|_| which::which("python"))
            .context("No system Python found")?
    } else {
        // Use venv interpreter
        let interp = venv_manager.interpreter_path(&cmd.env_hash);
        if !interp.exists() {
            info!(
                "Job {}: environment {} not cached, provisioning...",
                job_id,
                truncate_hash(&cmd.env_hash)
            );
            // Can't provision without lockfile content in DispatchCommand
            anyhow::bail!(
                "Environment {} not provisioned. Worker cannot auto-provision without lockfile. \
                 Either send PREPARE_ENV first, or include lockfile in DISPATCH.",
                truncate_hash(&cmd.env_hash)
            );
        }
        interp
    };

    // Execute bridge
    let config = BridgeConfig {
        interpreter_path: interpreter,
        source_code: cmd.source_code.clone(),
        file_path: cmd.file_path.clone(),
        job_id,
        file_version_id: cmd.file_version_id,
        shim_path: shim_path.to_path_buf(),
    };

    let bridge_result = bridge::execute_bridge(config).await?;
    let batches = bridge_result.batches;

    // Log the captured logs for debugging (will be stored in DB in Phase 3)
    if !bridge_result.logs.is_empty() {
        debug!("Job {} logs ({} bytes):\n{}", job_id, bridge_result.logs.len(), bridge_result.logs);
    }

    // Write to Parquet
    let mut total_rows = 0;
    let mut artifacts = Vec::new();

    for sink_config in &cmd.sinks {
        let output_path = write_parquet(parquet_root, job_id, &sink_config.topic, &batches)?;

        total_rows = batches.iter().map(|b| b.num_rows()).sum();

        let mut artifact = HashMap::new();
        artifact.insert("topic".to_string(), sink_config.topic.clone());
        artifact.insert(
            "uri".to_string(),
            format!("file://{}", output_path.display()),
        );
        artifacts.push(artifact);
    }

    info!("Job {} complete: {} rows", job_id, total_rows);
    Ok((total_rows, artifacts))
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

/// Write Arrow batches to Parquet with collision protection
///
/// Uses job_id + timestamp in filename to prevent overwrites if job is rerun.
/// Format: {job_id}_{topic}_{timestamp}.parquet
fn write_parquet(
    root: &std::path::Path,
    job_id: u64,
    topic: &str,
    batches: &[RecordBatch],
) -> Result<PathBuf> {
    if batches.is_empty() {
        anyhow::bail!("[Job {}] No batches to write for topic '{}'", job_id, topic);
    }

    std::fs::create_dir_all(root)
        .with_context(|| format!("[Job {}] Failed to create output directory: {}", job_id, root.display()))?;

    // Include timestamp to prevent overwrites on job rerun
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let filename = format!("{}_{}_{}.parquet", job_id, topic, timestamp);
    let path = root.join(&filename);

    // Double-check we're not overwriting (should never happen with timestamp)
    if path.exists() {
        warn!(
            "[Job {}] Parquet file already exists (unlikely with timestamp): {}",
            job_id,
            path.display()
        );
    }

    let file = File::create(&path)
        .with_context(|| format!("[Job {}] Failed to create parquet file: {}", job_id, path.display()))?;

    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();

    let schema = batches[0].schema();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))
        .with_context(|| format!(
            "[Job {}] Failed to create Arrow writer for schema {:?}",
            job_id, schema
        ))?;

    let mut total_rows = 0;
    for batch in batches {
        writer.write(batch)
            .with_context(|| format!(
                "[Job {}] Failed to write batch ({} rows) to parquet",
                job_id, batch.num_rows()
            ))?;
        total_rows += batch.num_rows();
    }

    writer.close()
        .with_context(|| format!("[Job {}] Failed to close parquet writer", job_id))?;

    info!(
        "[Job {}] Wrote {} rows to parquet: {}",
        job_id,
        total_rows,
        path.display()
    );

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

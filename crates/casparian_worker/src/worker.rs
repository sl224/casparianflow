//! Worker Node
//!
//! Design principles:
//! - VenvManager created once at startup, reused for all jobs
//! - Socket owned directly (not Option) - created during connect
//! - Sink logic inlined (was only ~100 lines, not worth separate file)
//! - Async only where truly needed (ZMQ recv), sync for blocking I/O

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use cf_protocol::types::{self, DispatchCommand, PrepareEnvCommand};
use cf_protocol::{Message, OpCode};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend};

use crate::bridge::{self, BridgeConfig};
use crate::venv_manager::VenvManager;

/// Worker configuration (plain data)
pub struct WorkerConfig {
    pub sentinel_addr: String,
    pub parquet_root: PathBuf,
    pub worker_id: String,
    pub shim_path: PathBuf,
}

/// Active worker with connected socket
pub struct Worker {
    config: WorkerConfig,
    socket: DealerSocket,
    venv_manager: Arc<Mutex<VenvManager>>, // Shared, mutable (for metadata updates)
    running: bool,
}

impl Worker {
    /// Connect to sentinel and create worker.
    /// VenvManager is created once here, not per-job.
    pub async fn connect(config: WorkerConfig) -> Result<Self> {
        // Initialize VenvManager once
        let venv_manager = VenvManager::new()?;
        let (count, bytes) = venv_manager.stats();
        info!("VenvManager: {} cached envs, {} MB", count, bytes / 1_000_000);

        // Create and connect socket
        let mut socket = DealerSocket::new();
        socket.connect(&config.sentinel_addr).await?;

        info!("Connected to sentinel: {}", config.sentinel_addr);

        // Send IDENTIFY
        let identify = types::IdentifyPayload {
            capabilities: vec!["*".to_string()],
            worker_id: Some(config.worker_id.clone()),
        };
        send_message(&mut socket, OpCode::Identify, 0, &identify).await?;
        info!("Sent IDENTIFY as {}", config.worker_id);

        Ok(Self {
            config,
            socket,
            venv_manager: Arc::new(Mutex::new(venv_manager)),
            running: false,
        })
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        self.running = true;
        info!("Entering event loop...");

        while self.running {
            match self.recv_message().await {
                Ok(Some(msg)) => {
                    if let Err(e) = self.handle_message(msg).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(None) => {
                    // Timeout, no message - continue
                }
                Err(e) => {
                    error!("Recv error: {}", e);
                    break;
                }
            }
        }

        info!("Worker stopped");
        Ok(())
    }

    /// Receive next message with timeout (multipart: [header, payload])
    async fn recv_message(&mut self) -> Result<Option<Message>> {
        let timeout = Duration::from_millis(100);

        // Receive multipart message
        let multipart = match tokio::time::timeout(timeout, self.socket.recv()).await {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => return Err(anyhow::anyhow!("ZMQ error: {}", e)),
            Err(_) => return Ok(None), // Timeout
        };

        // Extract frames
        let parts: Vec<Vec<u8>> = multipart.into_vec().into_iter()
            .map(|b| b.to_vec())
            .collect();

        if parts.len() < 2 {
            warn!("Expected 2 frames [header, payload], got {}", parts.len());
            return Ok(None);
        }

        let msg = Message::unpack(&[parts[0].clone(), parts[1].clone()])?;
        Ok(Some(msg))
    }

    /// Handle a message
    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        match msg.header.opcode {
            OpCode::Dispatch => {
                let cmd: DispatchCommand = serde_json::from_slice(&msg.payload)?;
                info!("DISPATCH job {} -> {}", msg.header.job_id, cmd.plugin_name);

                let receipt = self.execute_job(msg.header.job_id, cmd).await;
                send_message(&mut self.socket, OpCode::Conclude, msg.header.job_id, &receipt).await?;
            }

            OpCode::PrepareEnv => {
                let cmd: PrepareEnvCommand = serde_json::from_slice(&msg.payload)?;
                info!("PREPARE_ENV {}", &cmd.env_hash[..12]);

                match self.prepare_env(cmd.clone()).await {
                    Ok(interpreter_path) => {
                        let payload = types::EnvReadyPayload {
                            env_hash: cmd.env_hash,
                            interpreter_path: interpreter_path.display().to_string(),
                            cached: true, // We just created/verified it
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

            OpCode::Abort => {
                warn!("ABORT job {} (not implemented)", msg.header.job_id);
            }

            OpCode::Err => {
                let err: types::ErrorPayload = serde_json::from_slice(&msg.payload)?;
                error!("Received ERR: {}", err.message);
            }

            _ => {
                warn!("Unhandled opcode: {:?}", msg.header.opcode);
            }
        }
        Ok(())
    }

    /// Execute a job
    async fn execute_job(&self, job_id: u64, cmd: DispatchCommand) -> types::JobReceipt {
        match self.execute_job_inner(job_id, &cmd).await {
            Ok((rows, artifacts)) => {
                let mut metrics = HashMap::new();
                metrics.insert("rows".to_string(), rows as i64);

                types::JobReceipt {
                    status: "SUCCESS".to_string(),
                    metrics,
                    artifacts,
                    error_message: None,
                }
            }
            Err(e) => {
                error!("Job {} failed: {}", job_id, e);
                types::JobReceipt {
                    status: "FAILED".to_string(),
                    metrics: HashMap::new(),
                    artifacts: vec![],
                    error_message: Some(e.to_string()),
                }
            }
        }
    }

    async fn execute_job_inner(
        &self,
        job_id: u64,
        cmd: &DispatchCommand,
    ) -> Result<(usize, Vec<HashMap<String, String>>)> {
        // Check interpreter exists
        let interpreter = {
            let mgr = self.venv_manager.lock().await;
            mgr.interpreter_path(&cmd.env_hash)
        };

        if !interpreter.exists() {
            anyhow::bail!(
                "Environment {} not provisioned. Send PREPARE_ENV first.",
                &cmd.env_hash[..12]
            );
        }

        // Execute bridge (blocking I/O in spawn_blocking)
        let config = BridgeConfig {
            interpreter_path: interpreter,
            source_code: cmd.source_code.clone(),
            file_path: cmd.file_path.clone(),
            job_id,
            file_version_id: cmd.file_version_id,
            shim_path: self.config.shim_path.clone(),
        };

        let batches = bridge::execute_bridge(config).await?;

        // Write to Parquet
        let mut total_rows = 0;
        let mut artifacts = Vec::new();

        for sink_config in &cmd.sinks {
            let output_path = write_parquet(
                &self.config.parquet_root,
                job_id,
                &sink_config.topic,
                &batches,
            )?;

            total_rows = batches.iter().map(|b| b.num_rows()).sum();

            let mut artifact = HashMap::new();
            artifact.insert("topic".to_string(), sink_config.topic.clone());
            artifact.insert("uri".to_string(), format!("file://{}", output_path.display()));
            artifacts.push(artifact);
        }

        info!("Job {} complete: {} rows", job_id, total_rows);
        Ok((total_rows, artifacts))
    }

    /// Prepare environment
    async fn prepare_env(&self, cmd: PrepareEnvCommand) -> Result<PathBuf> {
        let env_hash = cmd.env_hash.clone();
        let lockfile = cmd.lockfile_content.clone();
        let python_version = cmd.python_version.clone();
        let venv_manager = self.venv_manager.clone();

        // Run blocking venv creation in spawn_blocking
        tokio::task::spawn_blocking(move || {
            let mut mgr = futures::executor::block_on(venv_manager.lock());
            mgr.get_or_create(&env_hash, &lockfile, python_version.as_deref())
        })
        .await?
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}

// --- Helper functions ---

/// Send a protocol message as multipart (header + body in one ZMQ message)
async fn send_message<T: serde::Serialize>(
    socket: &mut DealerSocket,
    opcode: OpCode,
    job_id: u64,
    payload: &T,
) -> Result<()> {
    use zeromq::ZmqMessage;

    let payload_bytes = serde_json::to_vec(payload)?;
    let msg = Message::new(opcode, job_id, payload_bytes);
    let (header, body) = msg.pack()?;

    // Send as multipart message so ROUTER receives [identity, header, body]
    let mut multipart = ZmqMessage::from(header.to_vec());
    multipart.push_back(body.into());
    socket.send(multipart).await?;
    Ok(())
}

/// Write Arrow batches to Parquet (sync, call from async context is fine - it's fast)
fn write_parquet(
    root: &PathBuf,
    job_id: u64,
    topic: &str,
    batches: &[RecordBatch],
) -> Result<PathBuf> {
    if batches.is_empty() {
        anyhow::bail!("No batches to write");
    }

    std::fs::create_dir_all(root)?;

    let filename = format!("{}_{}.parquet", job_id, topic);
    let path = root.join(&filename);

    let file = File::create(&path)?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();

    let schema = batches[0].schema();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;

    for batch in batches {
        writer.write(batch)?;
    }

    writer.close()?;
    info!("Wrote Parquet: {}", path.display());

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
        };

        assert_eq!(config.sentinel_addr, "tcp://localhost:5555");
        assert_eq!(config.worker_id, "test-worker");
    }
}

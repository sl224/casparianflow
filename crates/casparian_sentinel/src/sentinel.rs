//! Sentinel - Control Plane for Casparian Flow
//!
//! Manages worker pool, dispatches jobs, and handles ZMQ ROUTER protocol.
//! Ported from Python sentinel.py with data-oriented design principles.

use anyhow::{Context, Result};
use cf_protocol::types::{self, DispatchCommand, IdentifyPayload, JobReceipt, SinkConfig};
use cf_protocol::{Message, OpCode};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{error, info, warn};
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend};

use crate::db::{models::*, JobQueue};

/// Connected worker state (kept in memory, not persisted)
#[derive(Debug, Clone)]
pub struct ConnectedWorker {
    pub identity: Vec<u8>,
    pub status: WorkerStatus,
    pub last_seen: f64,
    pub capabilities: HashSet<String>,
    pub current_job_id: Option<i32>,
    pub worker_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    Idle,
    Busy,
}

impl ConnectedWorker {
    fn new(identity: Vec<u8>, worker_id: String, capabilities: HashSet<String>) -> Self {
        Self {
            identity,
            status: WorkerStatus::Idle,
            last_seen: current_time(),
            capabilities,
            current_job_id: None,
            worker_id,
        }
    }
}

/// Sentinel configuration
pub struct SentinelConfig {
    pub bind_addr: String,
    pub database_url: String,
}

/// Main Sentinel control plane
pub struct Sentinel {
    socket: RouterSocket,
    workers: HashMap<Vec<u8>, ConnectedWorker>,
    queue: JobQueue,
    pool: sqlx::Pool<sqlx::Sqlite>,  // Database pool for queries
    topic_map: HashMap<String, Vec<SinkConfig>>, // Cache: plugin_name -> sinks
    running: bool,
}

impl Sentinel {
    /// Create and bind Sentinel
    pub async fn bind(config: SentinelConfig) -> Result<Self> {
        // Connect to database
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect(&config.database_url)
            .await
            .context("Failed to connect to database")?;

        let queue = JobQueue::new(pool.clone());

        // Create and bind ROUTER socket
        let mut socket = RouterSocket::new();
        socket
            .bind(&config.bind_addr)
            .await
            .context("Failed to bind ROUTER socket")?;

        info!("Sentinel bound to {}", config.bind_addr);

        // Load topic configs into memory
        let topic_map = Self::load_topic_configs(&pool).await?;
        info!("Loaded {} plugin topic configs", topic_map.len());

        Ok(Self {
            socket,
            workers: HashMap::new(),
            queue,
            pool: pool.clone(),
            topic_map,
            running: false,
        })
    }

    /// Load topic configurations from database into memory (non-blocking cache)
    async fn load_topic_configs(
        pool: &sqlx::Pool<sqlx::Sqlite>,
    ) -> Result<HashMap<String, Vec<SinkConfig>>> {
        let configs: Vec<TopicConfig> = sqlx::query_as("SELECT * FROM cf_topic_config")
            .fetch_all(pool)
            .await?;

        let mut map: HashMap<String, Vec<SinkConfig>> = HashMap::new();

        for tc in configs {
            let sink = SinkConfig {
                topic: tc.topic_name,
                uri: tc.uri,
                mode: tc.mode,
                schema_def: tc.schema_json,
            };

            map.entry(tc.plugin_name).or_default().push(sink);
        }

        Ok(map)
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        self.running = true;
        info!("Sentinel event loop started");

        while self.running {
            // Receive message with timeout
            match self.recv_message().await {
                Ok(Some((identity, msg))) => {
                    if let Err(e) = self.handle_message(identity, msg).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(None) => {
                    // Timeout - no message
                }
                Err(e) => {
                    error!("Recv error: {}", e);
                }
            }

            // Dispatch loop (assign jobs to idle workers)
            if let Err(e) = self.dispatch_loop().await {
                error!("Dispatch error: {}", e);
            }
        }

        info!("Sentinel stopped");
        Ok(())
    }

    /// Receive next message with timeout
    ///
    /// ROUTER receives multipart message: [identity, header, payload]
    async fn recv_message(&mut self) -> Result<Option<(Vec<u8>, Message)>> {
        let timeout = Duration::from_millis(100);

        // Receive multipart message
        let multipart = match tokio::time::timeout(timeout, self.socket.recv()).await {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => return Err(anyhow::anyhow!("ZMQ error: {}", e)),
            Err(_) => return Ok(None), // Timeout
        };

        // Extract frames from multipart
        let parts: Vec<Vec<u8>> = multipart.into_vec().into_iter()
            .map(|b| b.to_vec())
            .collect();

        if parts.len() < 3 {
            warn!("Expected 3 frames [identity, header, payload], got {}", parts.len());
            return Ok(None);
        }

        let identity = parts[0].clone();
        let header = parts[1].clone();
        let payload = parts[2].clone();

        let msg = Message::unpack(&[header, payload])?;
        Ok(Some((identity, msg)))
    }

    /// Handle a received message
    async fn handle_message(&mut self, identity: Vec<u8>, msg: Message) -> Result<()> {
        match msg.header.opcode {
            OpCode::Identify => {
                let payload: IdentifyPayload = serde_json::from_slice(&msg.payload)?;
                self.register_worker(identity, payload);
            }

            OpCode::Conclude => {
                let receipt: JobReceipt = serde_json::from_slice(&msg.payload)?;
                self.handle_conclude(identity, msg.header.job_id, receipt)
                    .await?;
            }

            OpCode::Err => {
                let err: types::ErrorPayload = serde_json::from_slice(&msg.payload)?;
                self.handle_error(identity, msg.header.job_id, err).await?;
            }

            OpCode::Heartbeat => {
                if let Some(worker) = self.workers.get_mut(&identity) {
                    worker.last_seen = current_time();
                }
            }

            _ => {
                warn!("Unhandled opcode: {:?}", msg.header.opcode);
            }
        }

        Ok(())
    }

    /// Register a worker from IDENTIFY message
    fn register_worker(&mut self, identity: Vec<u8>, payload: IdentifyPayload) {
        let worker_id = payload
            .worker_id
            .unwrap_or_else(|| format!("worker-{:x}", identity[0]));

        let capabilities: HashSet<String> = payload.capabilities.into_iter().collect();

        let worker = ConnectedWorker::new(identity.clone(), worker_id.clone(), capabilities.clone());

        info!(
            "Worker joined [{}]: {} capabilities",
            worker_id,
            capabilities.len()
        );

        self.workers.insert(identity, worker);
    }

    /// Handle CONCLUDE message (job completed/failed)
    async fn handle_conclude(
        &mut self,
        identity: Vec<u8>,
        job_id: u64,
        receipt: JobReceipt,
    ) -> Result<()> {
        // Mark worker as idle
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Idle;
            worker.current_job_id = None;
            worker.last_seen = current_time();
        }

        let job_id = job_id as i32;

        if receipt.status == "SUCCESS" {
            info!(
                "Job {} completed: {} artifacts",
                job_id,
                receipt.artifacts.len()
            );
            self.queue.complete_job(job_id, "Success").await?;
        } else if receipt.status == "FAILED" {
            let error = receipt.error_message.unwrap_or_else(|| "Unknown error".to_string());
            error!("Job {} failed: {}", job_id, error);
            self.queue.fail_job(job_id, &error).await?;
        }

        Ok(())
    }

    /// Handle ERR message
    async fn handle_error(
        &mut self,
        identity: Vec<u8>,
        job_id: u64,
        err: types::ErrorPayload,
    ) -> Result<()> {
        error!("Job {} error: {}", job_id, err.message);
        if let Some(trace) = &err.traceback {
            error!("Traceback:\n{}", trace);
        }

        // Mark worker as idle
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Idle;
            worker.current_job_id = None;
            worker.last_seen = current_time();
        }

        self.queue.fail_job(job_id as i32, &err.message).await?;
        Ok(())
    }

    /// Dispatch loop: assign jobs to idle workers
    async fn dispatch_loop(&mut self) -> Result<()> {
        // Find idle workers
        let idle_workers: Vec<_> = self
            .workers
            .values()
            .filter(|w| w.status == WorkerStatus::Idle)
            .collect();

        if idle_workers.is_empty() {
            return Ok(());
        }

        // Try to pop a job
        let Some(job) = self.queue.pop_job().await? else {
            return Ok(());
        };

        // Find capable worker
        let candidate = idle_workers
            .iter()
            .find(|w| w.capabilities.contains("*") || w.capabilities.contains(&job.plugin_name));

        if let Some(worker) = candidate {
            self.assign_job(worker.identity.clone(), job).await?;
        } else {
            warn!(
                "No worker capable of {}. Failing job.",
                job.plugin_name
            );
            self.queue
                .fail_job(job.id, "No capable worker available")
                .await?;
        }

        Ok(())
    }

    /// Assign a job to a worker
    async fn assign_job(&mut self, identity: Vec<u8>, job: ProcessingJob) -> Result<()> {
        info!("Assigning job {} to worker", job.id);

        // Get sink configs from cache
        let mut sinks = self.topic_map.get(&job.plugin_name).cloned().unwrap_or_default();

        // Add default output sink if none configured
        if !sinks.iter().any(|s| s.topic == "output") {
            sinks.push(SinkConfig {
                topic: "output".to_string(),
                uri: format!("parquet://{}_output.parquet", job.plugin_name),
                mode: "append".to_string(),
                schema_def: None,
            });
        }

        // Load file path from database
        let file_version: crate::db::models::FileVersion = sqlx::query_as(
            "SELECT * FROM cf_file_version WHERE id = ?"
        )
        .bind(job.file_version_id)
        .fetch_one(&self.pool)
        .await?;

        let file_location: crate::db::models::FileLocation = sqlx::query_as(
            "SELECT * FROM cf_file_location WHERE id = ?"
        )
        .bind(file_version.location_id)
        .fetch_one(&self.pool)
        .await?;

        let source_root: crate::db::models::SourceRoot = sqlx::query_as(
            "SELECT * FROM cf_source_root WHERE id = ?"
        )
        .bind(file_location.source_root_id)
        .fetch_one(&self.pool)
        .await?;

        let file_path = format!("{}/{}", source_root.path, file_location.rel_path);

        // Load plugin manifest (ACTIVE status)
        let manifest: crate::db::models::PluginManifest = sqlx::query_as(
            "SELECT * FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE' ORDER BY created_at DESC LIMIT 1"
        )
        .bind(&job.plugin_name)
        .fetch_one(&self.pool)
        .await?;

        let cmd = DispatchCommand {
            plugin_name: job.plugin_name.clone(),
            file_path,
            sinks,
            file_version_id: job.file_version_id as i64,
            env_hash: manifest.env_hash.unwrap_or_else(|| "system".to_string()),
            source_code: manifest.source_code,
            artifact_hash: manifest.artifact_hash,
        };

        let payload = serde_json::to_vec(&cmd)?;
        let msg = Message::new(OpCode::Dispatch, job.id as u64, payload);
        let (header, body) = msg.pack()?;

        // Send DISPATCH message as multipart [identity, header, body]
        use zeromq::ZmqMessage;
        let mut multipart = ZmqMessage::from(identity.clone());
        multipart.push_back(header.to_vec().into());
        multipart.push_back(body.into());
        self.socket.send(multipart).await?;

        // Mark worker as busy
        if let Some(worker) = self.workers.get_mut(&identity) {
            worker.status = WorkerStatus::Busy;
            worker.current_job_id = Some(job.id);
        }

        info!("Dispatched job {} ({})", job.id, job.plugin_name);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}

/// Get current Unix timestamp
fn current_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connected_worker() {
        let identity = vec![1, 2, 3, 4];
        let worker = ConnectedWorker::new(
            identity.clone(),
            "test-worker".to_string(),
            HashSet::from(["*".to_string()]),
        );

        assert_eq!(worker.identity, identity);
        assert_eq!(worker.status, WorkerStatus::Idle);
        assert!(worker.capabilities.contains("*"));
    }

    #[test]
    fn test_worker_status() {
        let mut worker = ConnectedWorker::new(
            vec![1],
            "test".to_string(),
            HashSet::new(),
        );

        assert_eq!(worker.status, WorkerStatus::Idle);

        worker.status = WorkerStatus::Busy;
        assert_eq!(worker.status, WorkerStatus::Busy);
    }
}

//! Sentinel - Control Plane for Casparian Flow
//!
//! Manages worker pool, dispatches jobs, and handles ZMQ ROUTER protocol.
//! Ported from Python sentinel.py with data-oriented design principles.

use anyhow::{Context, Result};
use casparian_protocol::types::{self, DispatchCommand, IdentifyPayload, JobReceipt, JobStatus, SinkConfig, SinkMode};
use casparian_protocol::{Message, OpCode};
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend};

use crate::db::{models::*, JobQueue};
use casparian_db::{DbConnection, DbValue, UnifiedDbRow};
use crate::metrics::METRICS;

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

// ============================================================================
// Circuit Breaker & Retry Constants
// ============================================================================

/// Maximum number of retries before moving to dead letter queue
const MAX_RETRIES: i32 = 3;

/// Base backoff in seconds for exponential retry (4^retry_count)
/// Retry 1: 4s, Retry 2: 16s, Retry 3: 64s
const BACKOFF_BASE_SECS: u64 = 4;

/// Consecutive failure threshold before tripping circuit breaker
const CIRCUIT_BREAKER_THRESHOLD: i32 = 5;

/// Result of the combined dispatch query (file path + manifest data)
#[derive(Debug)]
struct DispatchQueryResult {
    file_path: String,
    source_code: String,
    parser_version: String,
    env_hash: Option<String>,
    artifact_hash: Option<String>,
}

impl DispatchQueryResult {
    fn from_row(row: &UnifiedDbRow) -> Result<Self> {
        Ok(Self {
            file_path: row.get_by_name("file_path")?,
            source_code: row.get_by_name("source_code")?,
            parser_version: row.get_by_name("parser_version")?,
            env_hash: row.get_by_name("env_hash")?,
            artifact_hash: row.get_by_name("artifact_hash")?,
        })
    }
}

/// Connected worker state (kept in memory, not persisted)
///
/// Note: identity is NOT stored here - it's the key in the workers HashMap.
/// This avoids duplicate storage and keeps ownership clear.
#[derive(Debug, Clone)]
pub struct ConnectedWorker {
    pub status: WorkerStatus,
    pub last_seen: f64,
    /// Plugin capabilities. Vec instead of HashSet - linear scan is faster
    /// for small N (< 50 plugins) due to cache locality.
    pub capabilities: Vec<String>,
    pub current_job_id: Option<i64>,
    pub worker_id: String,
    /// Environments that are provisioned and ready on this worker.
    /// Vec instead of HashSet - linear scan is faster for small N (< 50 envs) due to cache locality.
    ///
    /// NOTE: Currently tracked but NOT used in dispatch decisions. Workers handle
    /// missing envs on-demand via VenvManager. This tracking exists for a future
    /// optimization: preferring workers that already have the required env cached
    /// to avoid network/disk I/O during job execution.
    pub ready_envs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    Idle,
    Busy,
}

impl ConnectedWorker {
    fn new(worker_id: String, capabilities: Vec<String>) -> Self {
        Self {
            status: WorkerStatus::Idle,
            last_seen: current_time(),
            capabilities,
            current_job_id: None,
            worker_id,
            ready_envs: Vec::new(),
        }
    }

    /// Check if this worker has the given environment ready
    fn has_env(&self, env_hash: &str) -> bool {
        self.ready_envs.iter().any(|e| e == env_hash)
    }

    /// Check if this worker can handle the given plugin
    fn can_handle(&self, plugin_name: &str) -> bool {
        self.capabilities.iter().any(|c| c == "*" || c == plugin_name)
    }

    /// Mark an environment as ready on this worker
    fn add_env(&mut self, env_hash: String) {
        if !self.has_env(&env_hash) {
            self.ready_envs.push(env_hash);
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
    conn: DbConnection,  // Database connection for queries
    topic_map: HashMap<String, Vec<SinkConfig>>, // Cache: plugin_name -> sinks
    running: bool,
    last_cleanup: f64, // Last time we ran stale worker cleanup
    /// Jobs orphaned by stale workers - need to be failed asynchronously
    orphaned_jobs: Vec<i64>,
    dispatch_backoff_ms: u64,
    dispatch_cooldown_until: Option<Instant>,
}

impl Sentinel {
    /// Create and bind Sentinel
    pub async fn bind(config: SentinelConfig) -> Result<Self> {
        let conn = DbConnection::open_from_url(&config.database_url)
            .await
            .context("Failed to connect to database")?;

        // Load topic configs into memory (before moving conn)
        let topic_map = Self::load_topic_configs(&conn).await?;
        info!("Loaded {} plugin topic configs", topic_map.len());

        // Clone connection only once for the queue
        let queue = JobQueue::new(conn.clone());

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
        let mut socket = RouterSocket::new();
        socket
            .bind(&config.bind_addr)
            .await
            .context("Failed to bind ROUTER socket")?;

        info!("Sentinel bound to {}", config.bind_addr);

        Ok(Self {
            socket,
            workers: HashMap::new(),
            queue,
            conn,
            topic_map,
            running: false,
            last_cleanup: current_time(),
            orphaned_jobs: Vec::new(),
            dispatch_backoff_ms: 0,
            dispatch_cooldown_until: None,
        })
    }

    /// Load topic configurations from database into memory (non-blocking cache)
    async fn load_topic_configs(
        conn: &DbConnection,
    ) -> Result<HashMap<String, Vec<SinkConfig>>> {
        let rows = conn.query_all("SELECT * FROM cf_topic_config", &[]).await?;
        let mut configs = Vec::with_capacity(rows.len());
        for row in rows {
            configs.push(TopicConfig::from_row(&row)?);
        }

        let mut map: HashMap<String, Vec<SinkConfig>> = HashMap::new();

        for tc in configs {
            // Get mode before moving other fields out of tc
            let mode = tc.sink_mode();
            let sink = SinkConfig {
                topic: tc.topic_name,
                uri: tc.uri,
                mode,
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

            // Periodic cleanup of stale workers
            self.cleanup_stale_workers();

            // Fail any orphaned jobs from stale workers
            if !self.orphaned_jobs.is_empty() {
                let jobs_to_fail: Vec<i64> = std::mem::take(&mut self.orphaned_jobs);
                for job_id in jobs_to_fail {
                    if let Err(e) = self.queue.fail_job(
                        job_id,
                        "Worker became unresponsive (stale heartbeat)"
                    ).await {
                        error!("Failed to mark orphaned job {} as failed: {}", job_id, e);
                    } else {
                        info!("Marked orphaned job {} as FAILED", job_id);
                        METRICS.inc_jobs_failed();
                    }
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
        let stale_workers: Vec<(Vec<u8>, String, Option<i64>)> = self.workers
            .iter()
            .filter(|(_, w)| w.last_seen < cutoff)
            .map(|(id, w)| (id.clone(), w.worker_id.clone(), w.current_job_id))
            .collect();

        // Remove stale workers and queue their jobs for failure
        for (id, worker_id, job_id) in stale_workers {
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
                    self.orphaned_jobs.push(jid);
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
            }

            OpCode::EnvReady => {
                let payload: types::EnvReadyPayload = serde_json::from_slice(&msg.payload)?;
                if let Some(worker) = self.workers.get_mut(&identity) {
                    worker.last_seen = current_time();
                    let env_short = &payload.env_hash[..12.min(payload.env_hash.len())];
                    info!(
                        "Worker [{}] env ready: {} (cached: {})",
                        worker.worker_id, env_short, payload.cached
                    );
                    worker.add_env(payload.env_hash);
                }
            }

            OpCode::Deploy => {
                let cmd: types::DeployCommand = serde_json::from_slice(&msg.payload)?;
                match self.handle_deploy(&identity, cmd).await {
                    Ok(()) => {
                        info!("Deploy successful");
                    }
                    Err(e) => {
                        error!("Deploy failed: {}", e);
                        self.send_error(&identity, &e.to_string()).await?;
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
    fn register_worker(&mut self, identity: Vec<u8>, payload: IdentifyPayload) {
        // Generate a unique worker_id from the full identity if not provided
        // Use first 8 bytes of identity hash to avoid collisions from using only identity[0]
        let worker_id = payload.worker_id.unwrap_or_else(|| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&identity);
            let hash = hasher.finalize();
            format!("worker-{:02x}{:02x}{:02x}{:02x}", hash[0], hash[1], hash[2], hash[3])
        });

        // Vec instead of HashSet - linear scan is faster for small N
        let capabilities: Vec<String> = payload.capabilities;

        info!(
            "Worker joined [{}]: {} capabilities",
            worker_id,
            capabilities.len()
        );

        let worker = ConnectedWorker::new(worker_id.clone(), capabilities);
        self.workers.insert(identity, worker);
        METRICS.inc_workers_registered();
        info!("Worker registered: {}", worker_id);
    }

    /// Handle CONCLUDE message (job completed/failed)
    ///
    /// For failed jobs:
    /// - Extracts `is_transient` from receipt metrics to determine retry eligibility
    /// - Applies exponential backoff for transient errors
    /// - Updates parser health for circuit breaker tracking
    /// - Moves to dead letter queue after MAX_RETRIES or for permanent errors
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

        // Validate job_id fits in i64 (database uses i64 for job IDs)
        let job_id: i64 = job_id.try_into().map_err(|_| {
            anyhow::anyhow!(
                "Job ID {} exceeds maximum supported value ({}). \
                This indicates a protocol error or corrupted message.",
                job_id,
                i64::MAX
            )
        })?;

        // Get plugin_name for health tracking (need to look up from job)
        let plugin_name = self.get_job_plugin_name(job_id).await;

        let conclude_start = Instant::now();
        match receipt.status {
            JobStatus::Success | JobStatus::PartialSuccess | JobStatus::CompletedWithWarnings => {
                info!(
                    "Job {} completed: {} artifacts",
                    job_id,
                    receipt.artifacts.len()
                );
                let summary = if receipt.status == JobStatus::Success {
                    "Success"
                } else {
                    "Partial success"
                };
                self.queue.complete_job(job_id, summary).await?;
                METRICS.inc_jobs_completed();

                // Record success for circuit breaker
                if let Some(ref parser) = plugin_name {
                    self.record_success(parser).await?;
                }
            }
            JobStatus::Failed => {
                let error = receipt.error_message.clone().unwrap_or_else(|| "Unknown error".to_string());

                // Check if error is transient (from receipt metrics)
                let is_transient = receipt.metrics.get("is_transient")
                    .map(|v| *v == 1)
                    .unwrap_or(true); // Default to transient (conservative)

                // Get current retry count
                let retry_count = self.get_job_retry_count(job_id).await.unwrap_or(0);

                // Record failure for circuit breaker
                if let Some(ref parser) = plugin_name {
                    self.record_failure(parser, &error).await?;
                }

                // Apply retry logic
                self.handle_job_failure(job_id, &error, is_transient, retry_count).await?;
            }
            JobStatus::Rejected => {
                // Worker was at capacity - requeue the job (always retry)
                warn!("Job {} rejected by worker (at capacity), requeueing", job_id);
                METRICS.inc_jobs_rejected();
                self.queue.requeue_job(job_id).await?;
            }
            JobStatus::Aborted => {
                let error = receipt.error_message.unwrap_or_else(|| "Aborted".to_string());
                warn!("Job {} aborted: {}", job_id, error);
                self.queue.fail_job(job_id, &error).await?;
                METRICS.inc_jobs_failed();

                // Record failure for circuit breaker
                if let Some(ref parser) = plugin_name {
                    self.record_failure(parser, &error).await?;
                }
            }
        }

        METRICS.record_conclude_time(conclude_start);
        if let Err(err) = self.update_pipeline_run_status_for_job(job_id).await {
            warn!("Failed to update pipeline run status for job {}: {}", job_id, err);
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

        // Validate job_id fits in i64
        let job_id: i64 = job_id.try_into().map_err(|_| {
            anyhow::anyhow!(
                "Job ID {} exceeds maximum supported value ({})",
                job_id,
                i64::MAX
            )
        })?;

        self.queue.fail_job(job_id, &err.message).await?;
        if let Err(err) = self.update_pipeline_run_status_for_job(job_id).await {
            warn!("Failed to update pipeline run status for job {}: {}", job_id, err);
        }
        Ok(())
    }

    /// Dispatch loop: assign jobs to ALL idle workers (not just one per iteration)
    async fn dispatch_loop(&mut self) -> Result<()> {
        if let Some(cooldown_until) = self.dispatch_cooldown_until {
            if Instant::now() < cooldown_until {
                return Ok(());
            }
        }

        // Collect idle worker identities first (to avoid borrow issues)
        let idle_identities: Vec<Vec<u8>> = self
            .workers
            .iter()
            .filter(|(_, w)| w.status == WorkerStatus::Idle)
            .map(|(id, _)| id.clone())
            .collect();

        if idle_identities.is_empty() {
            return Ok(());
        }

        let mut remaining_workers = idle_identities;
        let mut dispatched_any = false;

        // Dispatch jobs to ALL idle workers (batch dispatch)
        while !remaining_workers.is_empty() {
            // Peek at next job without popping
            let Some(job) = self.queue.peek_job().await? else {
                self.schedule_dispatch_backoff();
                break; // No more jobs
            };

            // Find capable worker for THIS job
            let capable_idx = remaining_workers.iter().position(|id| {
                self.workers
                    .get(id)
                    .map(|w| w.can_handle(&job.plugin_name))
                    .unwrap_or(false)
            });

            match capable_idx {
                Some(idx) => {
                    // Pop the job now that we know we can handle it
                    // NOTE: Another sentinel could have claimed it between peek and pop (TOCTOU).
                    // This is expected in multi-sentinel deployments - just continue to next job.
                    let Some(job) = self.queue.pop_job().await? else {
                        debug!("Job claimed by another sentinel between peek and pop - continuing");
                        continue;
                    };
                    if let Some(run_id) = job.pipeline_run_id.as_deref() {
                        if let Err(err) = self.set_pipeline_run_running(run_id).await {
                            warn!("Failed to set pipeline run {} running: {}", run_id, err);
                        }
                    }
                    let identity = remaining_workers.remove(idx);
                    self.assign_job(identity, job).await?;
                    dispatched_any = true;
                }
                None => {
                    // No capable worker for this job - leave it in queue, stop dispatching
                    // Job stays queued for when a capable worker becomes available
                    self.schedule_dispatch_backoff();
                    break;
                }
            }
        }

        if dispatched_any {
            self.dispatch_backoff_ms = 0;
            self.dispatch_cooldown_until = None;
        }

        Ok(())
    }

    fn schedule_dispatch_backoff(&mut self) {
        let next = if self.dispatch_backoff_ms == 0 {
            DISPATCH_BACKOFF_BASE_MS
        } else {
            (self.dispatch_backoff_ms * 2).min(DISPATCH_BACKOFF_MAX_MS)
        };
        self.dispatch_backoff_ms = next;

        let jitter_ms = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 % DISPATCH_BACKOFF_JITTER_MS)
            .unwrap_or(0));
        self.dispatch_cooldown_until = Some(Instant::now() + Duration::from_millis(next + jitter_ms));
    }

    async fn update_pipeline_run_status_for_job(&self, job_id: i64) -> Result<()> {
        let run_id = self
            .conn
            .query_optional(
                "SELECT pipeline_run_id FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .await?
            .and_then(|row| row.get_by_name::<String>("pipeline_run_id").ok());
        let Some(run_id) = run_id else {
            return Ok(());
        };
        self.update_pipeline_run_status(&run_id).await
    }

    async fn set_pipeline_run_running(&self, run_id: &str) -> Result<()> {
        if !self.table_exists("cf_pipeline_runs").await? {
            return Ok(());
        }
        self.conn
            .execute(
                r#"
                UPDATE cf_pipeline_runs
                SET status = 'running',
                    started_at = COALESCE(started_at, CURRENT_TIMESTAMP)
                WHERE id = ?
                "#,
                &[DbValue::from(run_id)],
            )
            .await?;
        Ok(())
    }

    async fn update_pipeline_run_status(&self, run_id: &str) -> Result<()> {
        if !self.table_exists("cf_pipeline_runs").await? {
            return Ok(());
        }

        let row = self
            .conn
            .query_optional(
                r#"
                SELECT
                    SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) AS failed,
                    SUM(CASE WHEN status IN ('QUEUED', 'RUNNING') THEN 1 ELSE 0 END) AS active,
                    SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) AS completed
                FROM cf_processing_queue
                WHERE pipeline_run_id = ?
                "#,
                &[DbValue::from(run_id)],
            )
            .await?;

        let Some(row) = row else {
            return Ok(());
        };

        let failed: i64 = row.get_by_name("failed").unwrap_or(0);
        let active: i64 = row.get_by_name("active").unwrap_or(0);
        let completed: i64 = row.get_by_name("completed").unwrap_or(0);

        if failed > 0 {
            self.conn
                .execute(
                    "UPDATE cf_pipeline_runs SET status = 'failed', completed_at = CURRENT_TIMESTAMP WHERE id = ?",
                    &[DbValue::from(run_id)],
                )
                .await?;
            return Ok(());
        }

        if active > 0 {
            self.set_pipeline_run_running(run_id).await?;
            return Ok(());
        }

        if completed > 0 {
            self.conn
                .execute(
                    "UPDATE cf_pipeline_runs SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = ?",
                    &[DbValue::from(run_id)],
                )
                .await?;
        }

        Ok(())
    }

    async fn table_exists(&self, table: &str) -> Result<bool> {
        let (query, param) = match self.conn.backend_name() {
            "DuckDB" => (
                "SELECT 1 FROM information_schema.tables WHERE table_name = ?",
                DbValue::from(table),
            ),
            "SQLite" => (
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?",
                DbValue::from(table),
            ),
            _ => (
                "SELECT 1 FROM information_schema.tables WHERE table_name = ?",
                DbValue::from(table),
            ),
        };
        Ok(self.conn.query_optional(query, &[param]).await?.is_some())
    }

    /// Assign a job to a worker
    async fn assign_job(&mut self, identity: Vec<u8>, job: ProcessingJob) -> Result<()> {
        let dispatch_start = Instant::now();

        // Validate job.id is non-negative before casting to u64
        // Negative IDs would wrap to huge values, corrupting protocol messages
        if job.id < 0 {
            anyhow::bail!(
                "Job ID {} is negative - this indicates database corruption",
                job.id
            );
        }
        let job_id_u64 = job.id as u64;

        info!("Assigning job {} to worker", job.id);

        // Get sink configs from cache (v1: single job-level sink)
        let mut sinks = self.topic_map.get(&job.plugin_name).cloned().unwrap_or_default();

        if sinks.is_empty() {
            sinks.push(SinkConfig {
                topic: "output".to_string(),
                uri: "parquet://./output".to_string(),
                mode: SinkMode::Append,
                schema_def: None,
            });
        } else if sinks.len() > 1 {
            warn!(
                "Multiple sink configs found for plugin '{}'; v1 uses the first entry only",
                job.plugin_name
            );
            sinks.truncate(1);
        }

        // Normalize to a single job-level sink
        if let Some(first) = sinks.first_mut() {
            first.topic = "output".to_string();
        }

        // Load file path and manifest in single query (was 4 queries, now 1)
        let dispatch_row = self
            .conn
            .query_optional(
                r#"
                SELECT
                    sf.path as file_path,
                    pm.source_code,
                    pm.version as parser_version,
                    pm.env_hash,
                    pm.artifact_hash
                FROM scout_files sf
                JOIN cf_plugin_manifest pm ON pm.plugin_name = ? AND pm.status = 'ACTIVE'
                WHERE sf.id = ?
                ORDER BY pm.created_at DESC
                LIMIT 1
                "#,
                &[DbValue::from(job.plugin_name.as_str()), DbValue::from(job.file_id)],
            )
            .await
            .context("Failed to load dispatch data")?;

        let dispatch_row = dispatch_row.ok_or_else(|| anyhow::anyhow!("Dispatch data missing"))?;
        let dispatch_data = DispatchQueryResult::from_row(&dispatch_row)?;

        let env_hash = dispatch_data.env_hash.clone().unwrap_or_else(|| "system".to_string());

        // NOTE: Eager provisioning removed - it was fire-and-forget with a race condition.
        // The worker's VenvManager handles missing envs on-demand, which is simpler and correct.
        // If we want eager provisioning in the future, it should be request/response with
        // waiting for EnvReady before sending DISPATCH.

        let cmd = DispatchCommand {
            plugin_name: job.plugin_name.clone(),
            parser_version: Some(dispatch_data.parser_version),
            file_path: dispatch_data.file_path,
            sinks,
            file_id: job.file_id as i64,
            env_hash,
            source_code: dispatch_data.source_code,
            artifact_hash: dispatch_data.artifact_hash,
        };

        let payload = serde_json::to_vec(&cmd)?;
        let msg = Message::new(OpCode::Dispatch, job_id_u64, payload)?;
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
            worker.current_job_id = Some(job.id as i64);
        }

        METRICS.inc_jobs_dispatched();
        METRICS.inc_messages_sent();
        METRICS.record_dispatch_time(dispatch_start);
        info!("Dispatched job {} ({})", job.id, job.plugin_name);
        Ok(())
    }

    /// Handle DEPLOY command - register a new plugin version
    async fn handle_deploy(
        &mut self,
        identity: &[u8],
        cmd: types::DeployCommand,
    ) -> Result<()> {
        info!(
            "Deploying plugin {} v{} from {}",
            cmd.plugin_name, cmd.version, cmd.publisher_name
        );

        // Verify the artifact_hash matches the content
        let computed_hash = compute_artifact_hash(&cmd.source_code, &cmd.lockfile_content);
        if computed_hash != cmd.artifact_hash {
            anyhow::bail!(
                "Artifact hash mismatch: expected {}, got {}",
                &cmd.artifact_hash[..12.min(cmd.artifact_hash.len())],
                &computed_hash[..12]
            );
        }

        // 2. Compute source_hash (SHA256, not MD5)
        let source_hash = compute_sha256(&cmd.source_code);

        // 3. Execute all DB operations in a transaction
        self.conn.execute("BEGIN TRANSACTION", &[]).await?;
        let now = chrono::Utc::now();

        // 3a. Upsert the plugin environment (lockfile)
        if !cmd.lockfile_content.is_empty() {
            if let Err(e) = self
                .conn
                .execute(
                    r#"
                    INSERT INTO cf_plugin_environment (hash, lockfile_content, size_mb, last_used, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    ON CONFLICT(hash) DO UPDATE SET last_used = ?
                    "#,
                    &[
                        DbValue::from(cmd.env_hash.as_str()),
                        DbValue::from(cmd.lockfile_content.as_str()),
                        DbValue::from(cmd.lockfile_content.len() as f64 / 1_000_000.0),
                        DbValue::from(now),
                        DbValue::from(now),
                        DbValue::from(now),
                    ],
                )
                .await
            {
                let _ = self.conn.execute("ROLLBACK", &[]).await;
                return Err(e.into());
            }
        }

        // 3b. Insert the plugin manifest
        if let Err(e) = self
            .conn
            .execute(
                r#"
                INSERT INTO cf_plugin_manifest
                (plugin_name, version, source_code, source_hash, status,
                 env_hash, artifact_hash, created_at)
                VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?)
                "#,
                &[
                    DbValue::from(cmd.plugin_name.as_str()),
                    DbValue::from(cmd.version.as_str()),
                    DbValue::from(cmd.source_code.as_str()),
                    DbValue::from(source_hash.as_str()),
                    DbValue::from(cmd.env_hash.as_str()),
                    DbValue::from(cmd.artifact_hash.as_str()),
                    DbValue::from(now),
                ],
            )
            .await
        {
            let _ = self.conn.execute("ROLLBACK", &[]).await;
            return Err(e.into());
        }

        // 3c. Deactivate previous versions
        if let Err(e) = self
            .conn
            .execute(
                r#"
                UPDATE cf_plugin_manifest
                SET status = 'SUPERSEDED'
                WHERE plugin_name = ? AND version != ? AND status = 'ACTIVE'
                "#,
                &[
                    DbValue::from(cmd.plugin_name.as_str()),
                    DbValue::from(cmd.version.as_str()),
                ],
            )
            .await
        {
            let _ = self.conn.execute("ROLLBACK", &[]).await;
            return Err(e.into());
        }

        // 4. Commit transaction
        if let Err(e) = self.conn.execute("COMMIT", &[]).await {
            let _ = self.conn.execute("ROLLBACK", &[]).await;
            return Err(e.into());
        }

        info!(
            "Deployed {} v{} (env: {}, artifact: {})",
            cmd.plugin_name,
            cmd.version,
            &cmd.env_hash[..12.min(cmd.env_hash.len())],
            &cmd.artifact_hash[..12.min(cmd.artifact_hash.len())]
        );

        // 5. Refresh topic_map cache (new plugins may have topic configs)
        // This ensures newly deployed plugins get their sink configs immediately
        match Self::load_topic_configs(&self.conn).await {
            Ok(new_map) => {
                let old_count = self.topic_map.len();
                self.topic_map = new_map;
                if self.topic_map.len() != old_count {
                    info!(
                        "Refreshed topic configs: {} -> {} plugins",
                        old_count,
                        self.topic_map.len()
                    );
                }
            }
            Err(e) => {
                warn!("Failed to refresh topic configs after deploy: {}", e);
                // Non-fatal: existing configs still work, new ones use defaults
            }
        }

        // 6. Send success response
        let response = types::DeployResponse {
            success: true,
            message: format!("Deployed {} v{}", cmd.plugin_name, cmd.version),
            plugin_id: None,
        };
        self.send_deploy_response(identity, &response).await?;

        Ok(())
    }

    /// Send error response to client
    async fn send_error(&mut self, identity: &[u8], message: &str) -> Result<()> {
        let payload = types::ErrorPayload {
            message: message.to_string(),
            traceback: None,
        };

        let msg_bytes = serde_json::to_vec(&payload)?;
        let msg = Message::new(OpCode::Err, 0, msg_bytes)?;
        let (header, body) = msg.pack()?;

        use zeromq::ZmqMessage;
        let mut multipart = ZmqMessage::from(identity.to_vec());
        multipart.push_back(header.to_vec().into());
        multipart.push_back(body.into());
        self.socket.send(multipart).await?;

        Ok(())
    }

    /// Send deploy response to client
    async fn send_deploy_response(
        &mut self,
        identity: &[u8],
        response: &types::DeployResponse,
    ) -> Result<()> {
        let payload = serde_json::to_vec(response)?;
        let msg = Message::new(OpCode::Ack, 0, payload)?;
        let (header, body) = msg.pack()?;

        use zeromq::ZmqMessage;
        let mut multipart = ZmqMessage::from(identity.to_vec());
        multipart.push_back(header.to_vec().into());
        multipart.push_back(body.into());
        self.socket.send(multipart).await?;

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    // ========================================================================
    // Circuit Breaker & Retry Logic
    // ========================================================================

    /// Handle a job failure with retry logic.
    ///
    /// - For transient errors with retries remaining: schedule retry with exponential backoff
    /// - For permanent errors or max retries exceeded: move to dead letter queue
    async fn handle_job_failure(
        &self,
        job_id: i64,
        error: &str,
        is_transient: bool,
        retry_count: i32,
    ) -> Result<()> {
        if is_transient && retry_count < MAX_RETRIES {
            // Exponential backoff: 4^retry_count seconds (4, 16, 64)
            let backoff_secs = BACKOFF_BASE_SECS.pow(retry_count as u32 + 1);
            info!(
                job_id,
                retry_count = retry_count + 1,
                backoff_secs,
                "Scheduling retry with exponential backoff"
            );

            // Update job with incremented retry_count and scheduled_at in future
            self.conn
                .execute(
                    r#"
                    UPDATE cf_processing_queue SET
                        status = 'QUEUED',
                        retry_count = ?,
                        claim_time = NULL,
                        error_message = ?
                    WHERE id = ?
                    "#,
                    &[
                        DbValue::from(retry_count + 1),
                        DbValue::from(error),
                        DbValue::from(job_id),
                    ],
                )
                .await?;

            // Note: In a production system, you'd want to use a scheduled_at column
            // and have the dispatch loop check `scheduled_at <= now()`. For simplicity,
            // we're just requeueing immediately but tracking retry_count.
            // The backoff could be enforced by the worker or by a separate scheduler.

            METRICS.inc_jobs_retried();
        } else {
            // Move to dead letter queue
            let reason = if is_transient {
                "max_retries_exceeded"
            } else {
                "permanent_error"
            };

            warn!(
                "Job {} moving to dead letter queue: {} (retries: {}/{})",
                job_id, reason, retry_count, MAX_RETRIES
            );

            self.move_to_dead_letter(job_id, error, reason).await?;
            METRICS.inc_jobs_failed();
        }

        Ok(())
    }

    /// Move a job to the dead letter queue for manual investigation.
    async fn move_to_dead_letter(&self, job_id: i64, error: &str, reason: &str) -> Result<()> {
        // Update job to FAILED status with reason
        let full_error = format!("{}: {}", reason, error);
        self.conn
            .execute(
                r#"
                UPDATE cf_processing_queue SET
                    status = 'FAILED',
                    end_time = ?,
                    error_message = ?
                WHERE id = ?
                "#,
                &[
                    DbValue::from(chrono::Utc::now()),
                    DbValue::from(full_error.as_str()),
                    DbValue::from(job_id),
                ],
            )
            .await?;

        error!("Job {} moved to dead letter queue: {}", job_id, reason);
        Ok(())
    }

    /// Check if parser is healthy (circuit breaker not tripped).
    ///
    /// Returns true if parser can accept jobs, false if paused.
    pub async fn check_circuit_breaker(&self, parser_name: &str) -> Result<bool> {
        let health = self
            .conn
            .query_optional(
                "SELECT * FROM cf_parser_health WHERE parser_name = ?",
                &[DbValue::from(parser_name)],
            )
            .await?;
        let health = health.map(|row| ParserHealth::from_row(&row)).transpose()?;

        if let Some(h) = health {
            // Already paused
            if h.is_paused() {
                warn!(parser = parser_name, "Parser is paused (circuit open)");
                return Ok(false);
            }

            // Check threshold
            if h.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
                // Trip the circuit breaker
                let now = chrono::Utc::now();
                self.conn
                    .execute(
                        "UPDATE cf_parser_health SET paused_at = ?, updated_at = ? WHERE parser_name = ?",
                        &[
                            DbValue::from(now),
                            DbValue::from(now),
                            DbValue::from(parser_name),
                        ],
                    )
                    .await?;

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

    /// Record successful execution (resets consecutive failures).
    async fn record_success(&self, parser_name: &str) -> Result<()> {
        let now = chrono::Utc::now();
        self.conn
            .execute(
                r#"
                INSERT INTO cf_parser_health (parser_name, total_executions, successful_executions, consecutive_failures, created_at, updated_at)
                VALUES (?, 1, 1, 0, ?, ?)
                ON CONFLICT(parser_name) DO UPDATE SET
                    total_executions = total_executions + 1,
                    successful_executions = successful_executions + 1,
                    consecutive_failures = 0,
                    updated_at = ?
                "#,
                &[
                    DbValue::from(parser_name),
                    DbValue::from(now),
                    DbValue::from(now),
                    DbValue::from(now),
                ],
            )
            .await?;

        debug!(parser = parser_name, "Recorded success, reset consecutive_failures");
        Ok(())
    }

    /// Record failed execution (increments consecutive failures).
    async fn record_failure(&self, parser_name: &str, reason: &str) -> Result<()> {
        let now = chrono::Utc::now();
        self.conn
            .execute(
                r#"
                INSERT INTO cf_parser_health (parser_name, total_executions, successful_executions, consecutive_failures, last_failure_reason, created_at, updated_at)
                VALUES (?, 1, 0, 1, ?, ?, ?)
                ON CONFLICT(parser_name) DO UPDATE SET
                    total_executions = total_executions + 1,
                    consecutive_failures = consecutive_failures + 1,
                    last_failure_reason = ?,
                    updated_at = ?
                "#,
                &[
                    DbValue::from(parser_name),
                    DbValue::from(reason),
                    DbValue::from(now),
                    DbValue::from(now),
                    DbValue::from(reason),
                    DbValue::from(now),
                ],
            )
            .await?;

        debug!(parser = parser_name, reason = reason, "Recorded failure");
        Ok(())
    }

    /// Get plugin name for a job (for health tracking).
    async fn get_job_plugin_name(&self, job_id: i64) -> Option<String> {
        let result = self
            .conn
            .query_optional(
                "SELECT plugin_name FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .await
            .ok()
            .flatten();

        result.and_then(|row| row.get_by_name("plugin_name").ok())
    }

    /// Get retry count for a job.
    async fn get_job_retry_count(&self, job_id: i64) -> Option<i32> {
        let result = self
            .conn
            .query_optional(
                "SELECT retry_count FROM cf_processing_queue WHERE id = ?",
                &[DbValue::from(job_id)],
            )
            .await
            .ok()
            .flatten();

        result.and_then(|row| row.get_by_name("retry_count").ok())
    }
}

/// Get current Unix timestamp
fn current_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX_EPOCH - check system clock")
        .as_secs_f64()
}

/// Compute SHA256 hash of content, returning hex string
fn compute_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute artifact hash from source code and lockfile
fn compute_artifact_hash(source_code: &str, lockfile_content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(source_code.as_bytes());
    hasher.update(lockfile_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casparian_db::{DbConnection, DbValue};

    #[test]
    fn test_connected_worker() {
        let worker = ConnectedWorker::new(
            "test-worker".to_string(),
            vec!["*".to_string()],
        );

        assert_eq!(worker.status, WorkerStatus::Idle);
        assert!(worker.can_handle("any_plugin"));
    }

    #[test]
    fn test_worker_can_handle() {
        // Wildcard capability
        let worker = ConnectedWorker::new("w1".to_string(), vec!["*".to_string()]);
        assert!(worker.can_handle("any_plugin"));
        assert!(worker.can_handle("another_plugin"));

        // Specific capability
        let worker = ConnectedWorker::new("w2".to_string(), vec!["plugin_a".to_string()]);
        assert!(worker.can_handle("plugin_a"));
        assert!(!worker.can_handle("plugin_b"));

        // No capabilities
        let worker = ConnectedWorker::new("w3".to_string(), vec![]);
        assert!(!worker.can_handle("any"));
    }

    #[test]
    fn test_worker_status() {
        let mut worker = ConnectedWorker::new("test".to_string(), vec![]);

        assert_eq!(worker.status, WorkerStatus::Idle);

        worker.status = WorkerStatus::Busy;
        assert_eq!(worker.status, WorkerStatus::Busy);
    }

    #[test]
    fn test_worker_ready_envs() {
        let mut worker = ConnectedWorker::new("test".to_string(), vec![]);

        // Initially no envs ready
        assert!(!worker.has_env("abc123"));
        assert!(worker.ready_envs.is_empty());

        // Add an env
        worker.add_env("abc123".to_string());
        assert!(worker.has_env("abc123"));
        assert_eq!(worker.ready_envs.len(), 1);

        // Adding same env again should not duplicate
        worker.add_env("abc123".to_string());
        assert_eq!(worker.ready_envs.len(), 1);

        // Add different env
        worker.add_env("def456".to_string());
        assert!(worker.has_env("def456"));
        assert_eq!(worker.ready_envs.len(), 2);
    }

    #[tokio::test]
    async fn test_load_topic_configs_with_schema_def() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();
        conn.execute(
            r#"
            CREATE TABLE cf_topic_config (
                id INTEGER PRIMARY KEY,
                plugin_name TEXT NOT NULL,
                topic_name TEXT NOT NULL,
                uri TEXT NOT NULL,
                mode TEXT DEFAULT 'append',
                schema_json TEXT
            )
            "#,
            &[],
        )
        .await
        .unwrap();

        let schema_json = r#"[{"name":"id","data_type":"int64","nullable":false}]"#;
        conn.execute(
            r#"
            INSERT INTO cf_topic_config (id, plugin_name, topic_name, uri, mode, schema_json)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            &[
                DbValue::from(1),
                DbValue::from("test_plugin"),
                DbValue::from("output"),
                DbValue::from("parquet:///tmp/out"),
                DbValue::from("append"),
                DbValue::from(schema_json),
            ],
        )
        .await
        .unwrap();

        let configs = Sentinel::load_topic_configs(&conn).await.unwrap();
        let sinks = configs.get("test_plugin").unwrap();
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].schema_def.as_deref(), Some(schema_json));
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
        let hash1 = compute_artifact_hash("source", "lockfile");
        let hash2 = compute_artifact_hash("source", "lockfile");

        // Same inputs produce same hash
        assert_eq!(hash1, hash2);

        // ORDER MATTERS: hash(a, b) != hash(b, a)
        let hash_ab = compute_artifact_hash("a", "b");
        let hash_ba = compute_artifact_hash("b", "a");
        assert_ne!(hash_ab, hash_ba);

        // Different inputs produce different hashes
        let hash3 = compute_artifact_hash("source1", "lockfile");
        let hash4 = compute_artifact_hash("source2", "lockfile");
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

//! Test Harness for Integration Testing
//!
//! Provides utilities for starting sentinel and worker in test mode,
//! waiting for job completion, and cleanup helpers.
//!
//! # Example
//!
//! ```ignore
//! use harness::{TestHarness, HarnessConfig};
//!
//! let harness = TestHarness::new(HarnessConfig::default())?;
//! harness.init_schema()?;
//! harness.start()?;  // Spawns sentinel and workers
//!
//! // Register plugin and create test file
//! harness.register_plugin("my_plugin", "1.0.0", plugin_source)?;
//! let (file_id, _) = harness.create_test_file("input.txt", "data")?;
//!
//! // Enqueue a job
//! let job_id = harness.enqueue_job("my_plugin", file_id)?;
//!
//! // Wait for completion
//! let result = harness.wait_for_job(job_id, Duration::from_secs(30))?;
//! assert!(result.is_success());
//!
//! // Cleanup happens on drop
//! ```

use casparian_db::{DbConnection, DbValue};
use casparian_protocol::{JobId, ProcessingStatus};
use casparian_sentinel::{Sentinel, SentinelConfig};
use casparian_worker::{Worker, WorkerConfig};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tracing::{info, warn};

/// Result type for test harness operations
pub type HarnessResult<T> = Result<T, HarnessError>;

/// Error type for test harness operations
#[derive(Debug)]
pub struct HarnessError {
    pub message: String,
}

impl HarnessError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HarnessError: {}", self.message)
    }
}

impl std::error::Error for HarnessError {}

impl From<anyhow::Error> for HarnessError {
    fn from(err: anyhow::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl From<casparian_db::BackendError> for HarnessError {
    fn from(err: casparian_db::BackendError) -> Self {
        Self::new(err.to_string())
    }
}

/// Configuration for the test harness
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Bind address for sentinel (will be auto-generated as IPC if None)
    pub sentinel_addr: Option<String>,
    /// Number of workers to spawn (default: 1)
    pub worker_count: usize,
    /// Database path (None = temp directory)
    pub db_path: Option<PathBuf>,
    /// Custom environment variables for workers
    pub env_vars: Vec<(String, String)>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            sentinel_addr: None, // Will be auto-generated as IPC socket
            worker_count: 1,
            db_path: None,
            env_vars: vec![],
        }
    }
}

impl HarnessConfig {
    /// Create config with specific TCP port
    pub fn with_port(port: u16) -> Self {
        Self {
            sentinel_addr: Some(format!("tcp://127.0.0.1:{}", port)),
            ..Default::default()
        }
    }

    /// Create config with specific address
    pub fn with_addr(addr: &str) -> Self {
        Self {
            sentinel_addr: Some(addr.to_string()),
            ..Default::default()
        }
    }

    /// Set environment variable for fixture plugin mode
    pub fn with_fixture_mode(mut self, mode: &str) -> Self {
        self.env_vars.push(("CF_FIXTURE_MODE".to_string(), mode.to_string()));
        self
    }

    /// Set fixture plugin row count
    pub fn with_fixture_rows(mut self, rows: usize) -> Self {
        self.env_vars.push(("CF_FIXTURE_ROWS".to_string(), rows.to_string()));
        self
    }

    /// Set fixture plugin sleep seconds (for slow mode)
    pub fn with_fixture_sleep_secs(mut self, secs: usize) -> Self {
        self.env_vars.push(("CF_FIXTURE_SLEEP_SECS".to_string(), secs.to_string()));
        self
    }
}

/// Job completion result
#[derive(Debug, Clone)]
pub struct JobCompletionResult {
    pub job_id: i64,
    pub status: ProcessingStatus,
    pub completion_status: Option<String>,
    pub error_message: Option<String>,
    pub rows_processed: Option<i64>,
}

impl JobCompletionResult {
    /// Check if job completed successfully
    pub fn is_success(&self) -> bool {
        self.status == ProcessingStatus::Completed
            && self.completion_status.as_deref() == Some("SUCCESS")
    }

    /// Check if job failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, ProcessingStatus::Failed | ProcessingStatus::Aborted)
    }
}

/// Test harness for integration testing
///
/// Manages sentinel, workers, and database lifecycle for testing.
/// Cleanup happens automatically when the harness is dropped.
///
/// Note: The harness releases its database connection when `start()` is called
/// because DuckDB only supports one writer. After `start()`, a read-only
/// connection is used for monitoring job status.
pub struct TestHarness {
    /// Temporary directory for test data
    temp_dir: TempDir,
    /// Database file path (for passing to sentinel/workers)
    db_path: PathBuf,
    /// Sentinel address (auto-generated IPC or user-provided)
    sentinel_addr: String,
    /// Database connection (Some before start, None during run, Some again after stop)
    conn: Option<DbConnection>,
    /// Sentinel shutdown channel
    sentinel_stop_tx: Option<mpsc::Sender<()>>,
    /// Sentinel thread handle
    sentinel_thread: Option<thread::JoinHandle<()>>,
    /// Worker shutdown handles
    worker_handles: Vec<casparian_worker::WorkerHandle>,
    /// Worker thread handles
    worker_threads: Vec<thread::JoinHandle<()>>,
    /// Configuration
    config: HarnessConfig,
}

impl TestHarness {
    /// Create a new test harness with the given configuration.
    ///
    /// This does NOT start sentinel or workers - call `start()` to do that.
    /// Uses a file-based DuckDB in the temp directory so sentinel and workers
    /// can share the same database.
    pub fn new(config: HarnessConfig) -> HarnessResult<Self> {
        let temp_dir = TempDir::new().map_err(|e| HarnessError::new(e.to_string()))?;

        // Always use a file-based database so sentinel/workers can connect
        let db_path = match &config.db_path {
            Some(path) => path.clone(),
            None => temp_dir.path().join("test.duckdb"),
        };

        // Generate sentinel address (IPC by default, or use provided address)
        let sentinel_addr = match &config.sentinel_addr {
            Some(addr) => addr.clone(),
            None => {
                // Use IPC socket in temp directory for test isolation
                let socket_path = temp_dir.path().join("sentinel.ipc");
                format!("ipc://{}", socket_path.display())
            }
        };

        let conn = DbConnection::open_duckdb(&db_path)?;

        Ok(Self {
            temp_dir,
            db_path,
            sentinel_addr,
            conn: Some(conn),
            sentinel_stop_tx: None,
            sentinel_thread: None,
            worker_handles: vec![],
            worker_threads: vec![],
            config,
        })
    }

    /// Get the database connection (panics if connection not available)
    fn get_conn(&self) -> &DbConnection {
        self.conn.as_ref().expect("Database connection not available (harness is running)")
    }

    /// Get mutable database connection (panics if not available)
    fn get_conn_mut(&mut self) -> &mut DbConnection {
        self.conn.as_mut().expect("Database connection not available (harness is running)")
    }

    /// Start the sentinel and workers.
    ///
    /// This spawns the sentinel in a background thread and connects workers to it.
    /// The sentinel and workers share the same database via the file path.
    pub fn start(&mut self) -> HarnessResult<()> {
        if self.sentinel_thread.is_some() {
            return Err(HarnessError::new("Sentinel already started"));
        }

        // Drop the harness's database connection before starting sentinel
        // DuckDB only allows one writer, so sentinel needs exclusive access
        // First checkpoint to ensure all writes are visible
        if let Some(conn) = self.conn.take() {
            conn.execute("CHECKPOINT", &[])
                .map_err(|e| HarnessError::new(format!("Failed to checkpoint database: {}", e)))?;
            // Connection is dropped here, releasing the lock
            drop(conn);
        }

        // Create shutdown channel for sentinel
        let (stop_tx, stop_rx) = mpsc::channel();
        self.sentinel_stop_tx = Some(stop_tx);

        // Build database URL for sentinel
        let db_url = format!("duckdb:{}", self.db_path.display());
        let bind_addr = self.sentinel_addr.clone();

        // Spawn sentinel in a thread
        let sentinel_thread = thread::Builder::new()
            .name("test-sentinel".to_string())
            .spawn(move || {
                let config = SentinelConfig {
                    bind_addr: bind_addr.clone(),
                    database_url: db_url,
                    max_workers: 4,
                    control_addr: None, // No control API for test harness
                };

                match Sentinel::bind(config) {
                    Ok(mut sentinel) => {
                        info!("Test sentinel started on {}", bind_addr);
                        if let Err(e) = sentinel.run_with_shutdown(stop_rx) {
                            warn!("Sentinel error: {}", e);
                        }
                        info!("Test sentinel stopped");
                    }
                    Err(e) => {
                        warn!("Failed to start sentinel: {}", e);
                    }
                }
            })
            .map_err(|e| HarnessError::new(format!("Failed to spawn sentinel thread: {}", e)))?;

        self.sentinel_thread = Some(sentinel_thread);

        // Give sentinel time to bind
        thread::sleep(Duration::from_millis(100));

        // Spawn workers
        let shim_path = casparian_worker::bridge::materialize_bridge_shim()
            .map_err(|e| HarnessError::new(format!("Failed to materialize shim: {}", e)))?;

        let output_dir = self.output_dir();
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| HarnessError::new(format!("Failed to create output dir: {}", e)))?;

        let venvs_dir = self.temp_dir.path().join("venvs");
        std::fs::create_dir_all(&venvs_dir)
            .map_err(|e| HarnessError::new(format!("Failed to create venvs dir: {}", e)))?;

        for i in 0..self.config.worker_count {
            let worker_config = WorkerConfig {
                sentinel_addr: self.sentinel_addr.clone(),
                parquet_root: output_dir.clone(),
                worker_id: format!("test-worker-{}", i),
                shim_path: shim_path.clone(),
                capabilities: vec!["*".to_string()],
                venvs_dir: Some(venvs_dir.clone()),
            };

            // Set environment variables for fixture plugin
            for (key, value) in &self.config.env_vars {
                std::env::set_var(key, value);
            }

            let (worker, handle) = Worker::connect(worker_config)
                .map_err(|e| HarnessError::new(format!("Failed to connect worker {}: {}", i, e)))?;

            self.worker_handles.push(handle);

            let worker_thread = thread::Builder::new()
                .name(format!("test-worker-{}", i))
                .spawn(move || {
                    if let Err(e) = worker.run() {
                        warn!("Worker error: {}", e);
                    }
                })
                .map_err(|e| HarnessError::new(format!("Failed to spawn worker thread: {}", e)))?;

            self.worker_threads.push(worker_thread);
        }

        // Give workers time to connect and register
        thread::sleep(Duration::from_millis(200));

        info!(
            "Test harness started: sentinel on {}, {} workers",
            self.sentinel_addr, self.config.worker_count
        );

        Ok(())
    }

    /// Check if the harness has been started
    pub fn is_started(&self) -> bool {
        self.sentinel_thread.is_some()
    }

    /// Get the database URL for external connections
    pub fn db_url(&self) -> String {
        format!("duckdb:{}", self.db_path.display())
    }

    /// Get the sentinel address
    pub fn sentinel_addr(&self) -> &str {
        &self.sentinel_addr
    }

    /// Get the temporary directory path
    pub fn temp_dir(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get the database connection (only available before start() or after stop())
    pub fn conn(&self) -> &DbConnection {
        self.get_conn()
    }

    /// Initialize the database schema for testing
    pub fn init_schema(&self) -> HarnessResult<()> {
        let conn = self.get_conn();

        // Initialize job queue schema (creates cf_processing_queue, etc.)
        let queue = casparian_sentinel::db::queue::JobQueue::new(conn.clone());
        queue
            .init_queue_schema()
            .map_err(|e| HarnessError::new(e.to_string()))?;
        queue
            .init_registry_schema()
            .map_err(|e| HarnessError::new(e.to_string()))?;
        queue
            .init_error_handling_schema()
            .map_err(|e| HarnessError::new(e.to_string()))?;

        // Create scout_files table (required for job dispatch)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS scout_files (
                id BIGINT PRIMARY KEY,
                source_id BIGINT,
                path TEXT NOT NULL,
                rel_path TEXT,
                size BIGINT DEFAULT 0,
                mtime BIGINT DEFAULT 0,
                content_hash TEXT,
                status TEXT DEFAULT 'PENDING',
                tag TEXT,
                extension TEXT
            )
            "#,
            &[],
        )?;

        Ok(())
    }

    /// Register a plugin in the manifest table
    pub fn register_plugin(
        &self,
        plugin_name: &str,
        version: &str,
        source_code: &str,
    ) -> HarnessResult<()> {
        let conn = self.get_conn();

        // Generate source hash using blake3
        let source_hash = blake3::hash(source_code.as_bytes()).to_hex().to_string();

        // Build a minimal manifest JSON
        let manifest_json = serde_json::json!({
            "name": plugin_name,
            "version": version,
            "protocol_version": "1.0",
            "runtime_kind": "python_shim",
            "entrypoint": format!("{}.py:parse", plugin_name),
        })
        .to_string();

        // Schema artifacts JSON (empty outputs is fine for testing)
        let schema_json = serde_json::json!({
            "fixture_output": {
                "columns": [
                    {"name": "id", "data_type": "int64", "nullable": false},
                    {"name": "value", "data_type": "string", "nullable": true}
                ]
            }
        })
        .to_string();

        conn.execute(
            r#"
            INSERT INTO cf_plugin_manifest (
                plugin_name, version, runtime_kind, entrypoint,
                source_code, source_hash, status, env_hash, artifact_hash,
                manifest_json, protocol_version, schema_artifacts_json, outputs_json
            ) VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?, ?)
            ON CONFLICT (plugin_name, version, runtime_kind, platform_os, platform_arch)
            DO UPDATE SET source_code = EXCLUDED.source_code, source_hash = EXCLUDED.source_hash
            "#,
            &[
                DbValue::from(plugin_name),
                DbValue::from(version),
                DbValue::from("python_shim"), // Must match RuntimeKind::PythonShim.as_str()
                DbValue::from(format!("{}.py:parse", plugin_name)),
                DbValue::from(source_code),
                DbValue::from(source_hash.as_str()),
                DbValue::from(source_hash.as_str()), // env_hash
                DbValue::from(source_hash.as_str()), // artifact_hash
                DbValue::from(manifest_json.as_str()),
                DbValue::from("1.0"),
                DbValue::from(schema_json.as_str()),
                DbValue::from(schema_json.as_str()), // outputs_json
            ],
        )?;

        Ok(())
    }

    /// Create a test file and register it in scout_files
    pub fn create_test_file(&self, filename: &str, content: &str) -> HarnessResult<(i64, PathBuf)> {
        let conn = self.get_conn();
        let file_path = self.temp_dir.path().join(filename);
        std::fs::write(&file_path, content).map_err(|e| HarnessError::new(e.to_string()))?;

        // Get next file ID
        let file_id: i64 = conn.query_scalar(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM scout_files",
            &[],
        )?;

        conn.execute(
            r#"
            INSERT INTO scout_files (id, source_id, path, rel_path, size, status)
            VALUES (?, 1, ?, ?, ?, 'PENDING')
            "#,
            &[
                DbValue::from(file_id),
                DbValue::from(file_path.to_string_lossy().as_ref()),
                DbValue::from(filename),
                DbValue::from(content.len() as i64),
            ],
        )?;

        Ok((file_id, file_path))
    }

    /// Enqueue a job for processing
    pub fn enqueue_job(&self, plugin_name: &str, file_id: i64) -> HarnessResult<JobId> {
        let conn = self.get_conn();

        // Get next job ID
        let job_id: i64 = conn.query_scalar(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM cf_processing_queue",
            &[],
        )?;

        conn.execute(
            r#"
            INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
            VALUES (?, ?, ?, ?, 10)
            "#,
            &[
                DbValue::from(job_id),
                DbValue::from(file_id),
                DbValue::from(plugin_name),
                DbValue::from(ProcessingStatus::Queued.as_str()),
            ],
        )?;

        Ok(JobId::new(job_id as u64))
    }

    /// Open a read-only connection to monitor job status (works while harness is running)
    fn open_readonly_conn(&self) -> HarnessResult<DbConnection> {
        DbConnection::open_duckdb_readonly(&self.db_path)
            .map_err(|e| HarnessError::new(format!("Failed to open read-only connection: {}", e)))
    }

    /// Wait for a job to complete (or fail) with timeout
    ///
    /// This method works while the harness is running by opening read-only connections.
    pub fn wait_for_job(&self, job_id: JobId, timeout: Duration) -> HarnessResult<JobCompletionResult> {
        let start = Instant::now();
        let job_id_i64 = job_id.as_u64() as i64;

        loop {
            // Check if timeout exceeded
            if start.elapsed() > timeout {
                return Err(HarnessError::new(format!(
                    "Timeout waiting for job {} after {:?}",
                    job_id_i64, timeout
                )));
            }

            // Open a read-only connection to check job status
            let conn = self.open_readonly_conn()?;

            // Query job status
            let row = conn.query_optional(
                r#"
                SELECT status, completion_status, error_message
                FROM cf_processing_queue
                WHERE id = ?
                "#,
                &[DbValue::from(job_id_i64)],
            )?;

            if let Some(row) = row {
                let status_str: String = row.get_by_name("status")?;
                let status = ProcessingStatus::from_str(&status_str)
                    .unwrap_or(ProcessingStatus::Pending);

                // Check if terminal state
                if status.is_terminal() {
                    let completion_status: Option<String> = row.get_by_name("completion_status").ok();
                    let error_message: Option<String> = row.get_by_name("error_message").ok();

                    return Ok(JobCompletionResult {
                        job_id: job_id_i64,
                        status,
                        completion_status,
                        error_message,
                        rows_processed: None,
                    });
                }
            }

            // Sleep briefly before next check
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Get the current status of a job
    pub fn get_job_status(&self, job_id: JobId) -> HarnessResult<ProcessingStatus> {
        // Use the main connection if available, otherwise open read-only
        let conn = if let Some(ref conn) = self.conn {
            conn.clone()
        } else {
            self.open_readonly_conn()?
        };

        let job_id_i64 = job_id.as_u64() as i64;

        let status_str: String = conn.query_scalar(
            "SELECT status FROM cf_processing_queue WHERE id = ?",
            &[DbValue::from(job_id_i64)],
        )?;

        Ok(ProcessingStatus::from_str(&status_str).unwrap_or(ProcessingStatus::Pending))
    }

    /// Count jobs in a given status
    pub fn count_jobs_by_status(&self, status: ProcessingStatus) -> HarnessResult<i64> {
        // Use the main connection if available, otherwise open read-only
        let conn = if let Some(ref conn) = self.conn {
            conn.clone()
        } else {
            self.open_readonly_conn()?
        };

        let count: i64 = conn.query_scalar(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE status = ?",
            &[DbValue::from(status.as_str())],
        )?;

        Ok(count)
    }

    /// Get the output directory path
    pub fn output_dir(&self) -> PathBuf {
        self.temp_dir.path().join("output")
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        info!("TestHarness drop: stopping sentinel and workers...");

        // Stop sentinel first (so workers won't get new jobs)
        if let Some(tx) = self.sentinel_stop_tx.take() {
            let _ = tx.send(());
        }

        // Stop workers - send shutdown signal
        for handle in self.worker_handles.drain(..) {
            let _ = handle.shutdown_now();
        }

        // Give a short time for graceful shutdown
        thread::sleep(Duration::from_millis(200));

        // Wait for sentinel thread with timeout
        if let Some(handle) = self.sentinel_thread.take() {
            // Use a timeout approach by spawning a waiter thread
            let waiter = thread::spawn(move || {
                let _ = handle.join();
            });

            // Wait up to 2 seconds for sentinel
            let start = Instant::now();
            loop {
                if waiter.is_finished() {
                    let _ = waiter.join();
                    break;
                }
                if start.elapsed() >= Duration::from_secs(2) {
                    warn!("Sentinel thread did not exit within timeout");
                    // Abandon the waiter thread - it will be cleaned up on process exit
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }

        // Wait for worker threads with timeout
        for handle in self.worker_threads.drain(..) {
            let waiter = thread::spawn(move || {
                let _ = handle.join();
            });

            let start = Instant::now();
            loop {
                if waiter.is_finished() {
                    let _ = waiter.join();
                    break;
                }
                if start.elapsed() >= Duration::from_secs(2) {
                    warn!("Worker thread did not exit within timeout");
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }

        info!("TestHarness drop: cleanup complete");
    }
}

/// Path to the fixture plugin
pub fn fixture_plugin_path() -> PathBuf {
    // Get the workspace root from CARGO_MANIFEST_DIR or infer it
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Navigate up to workspace root and then to fixture plugin
    let mut path = manifest_dir;
    // If we're in crates/casparian, go up two levels
    if path.ends_with("casparian") {
        path = path.parent().unwrap().parent().unwrap().to_path_buf();
    }
    path.join("tests/fixtures/plugins/fixture_plugin.py")
}

/// Read the fixture plugin source code
pub fn fixture_plugin_source() -> String {
    let path = fixture_plugin_path();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture plugin at {:?}: {}", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = TestHarness::new(HarnessConfig::default()).unwrap();
        assert!(harness.temp_dir().exists());
    }

    #[test]
    fn test_fixture_plugin_exists() {
        let path = fixture_plugin_path();
        assert!(path.exists(), "Fixture plugin should exist at {:?}", path);
    }

    #[test]
    fn test_fixture_plugin_source_readable() {
        let source = fixture_plugin_source();
        assert!(source.contains("fixture_plugin"));
        assert!(source.contains("def parse"));
    }

    #[test]
    fn test_harness_config_with_fixture_mode() {
        let config = HarnessConfig::default()
            .with_fixture_mode("slow")
            .with_fixture_rows(100)
            .with_fixture_sleep_secs(5);

        assert!(config.env_vars.iter().any(|(k, v)| k == "CF_FIXTURE_MODE" && v == "slow"));
        assert!(config.env_vars.iter().any(|(k, v)| k == "CF_FIXTURE_ROWS" && v == "100"));
        assert!(config.env_vars.iter().any(|(k, v)| k == "CF_FIXTURE_SLEEP_SECS" && v == "5"));
    }
}

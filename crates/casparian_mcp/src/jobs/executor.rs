//! Job Executor - Background Job Execution Engine (Sync Version)
//!
//! Manages a queue of jobs and executes them serially (single concurrency).
//! Provides cancellation support via cooperative cancel signals.
//!
//! # Design
//!
//! The executor does NOT orchestrate pipelines - it only:
//! - Receives job IDs from the queue
//! - Dispatches to job runners (backtest, run)
//! - Updates progress and handles completion/failure
//! - Supports cooperative cancellation
//!
//! All actual work is delegated to existing crates (casparian_worker, casparian_backtest).
//!
//! # Sync Architecture (Phase 1B Commit 3)
//!
//! This module uses synchronous std channels and threads instead of tokio.
//! Cancellation uses atomic flags instead of watch channels.
//!
//! # Lock Audit (Phase 4)
//!
//! The `cancels` HashMap uses Arc<Mutex<>> because:
//! - JobExecutorHandle is cloned and shared across threads
//! - Executor thread registers/unregisters tokens during job execution
//! - Any thread can call cancel() via the handle
//! - Lock is held only for brief HashMap operations (no I/O)
//! - The CancellationToken itself is lock-free (AtomicBool)
//!
//! The JobManager access was refactored to use CoreHandle (message passing)
//! to avoid holding locks during I/O operations.

use super::{JobId, JobProgress, JobSpec, JobState};
use crate::core::{CancellationToken, CoreHandle};
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Default job timeout (30 minutes)
const DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1000;

/// Handle for interacting with the job executor from tools.
///
/// Can be cloned and shared across threads.
///
/// # Lock Invariant (cancels)
///
/// The `cancels` HashMap is protected by a Mutex because:
/// 1. JobExecutorHandle is Clone + shared across tool-call threads
/// 2. The executor thread registers/unregisters during job lifecycle
/// 3. Any thread can request cancellation via cancel()
/// 4. Lock held only for brief O(1) HashMap ops, never across I/O
/// 5. CancellationToken itself is lock-free (Arc<AtomicBool>)
#[derive(Clone)]
pub struct JobExecutorHandle {
    /// Channel to send job IDs for execution
    tx: Sender<JobId>,
    /// Map of job_id -> cancellation token.
    /// INVARIANT: Lock held only for HashMap insert/remove/get, never across I/O.
    cancels: Arc<Mutex<HashMap<JobId, CancellationToken>>>,
}

impl JobExecutorHandle {
    /// Create a new executor handle
    fn new(tx: Sender<JobId>) -> Self {
        Self {
            tx,
            cancels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Enqueue a job for execution.
    ///
    /// The job must already exist in JobManager with a valid JobSpec.
    /// The executor will start it when capacity is available.
    pub fn enqueue(&self, job_id: JobId) -> Result<()> {
        self.tx
            .send(job_id.clone())
            .context("Executor channel closed")?;
        debug!("Enqueued job for execution: {}", job_id);
        Ok(())
    }

    /// Request cancellation of a job.
    ///
    /// This is cooperative - the job runner must check the cancel signal.
    /// Returns true if a cancel signal was sent, false if job not found.
    pub fn cancel(&self, job_id: &JobId) -> Result<bool> {
        let cancels = self.cancels.lock().expect("Cancel map lock poisoned");
        if let Some(token) = cancels.get(job_id) {
            token.cancel();
            info!("Sent cancel signal to job: {}", job_id);
            Ok(true)
        } else {
            debug!("No active cancel handle for job: {}", job_id);
            Ok(false)
        }
    }

    /// Register a cancellation token for a job (internal use)
    fn register_cancel(&self, job_id: &JobId, token: CancellationToken) {
        let mut cancels = self.cancels.lock().expect("Cancel map lock poisoned");
        cancels.insert(*job_id, token);
    }

    /// Unregister a cancellation token for a job (internal use)
    fn unregister_cancel(&self, job_id: &JobId) {
        let mut cancels = self.cancels.lock().expect("Cancel map lock poisoned");
        cancels.remove(job_id);
    }
}

impl std::fmt::Debug for JobExecutorHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobExecutorHandle")
            .field("channel", &"<std::sync::mpsc::Sender>")
            .finish()
    }
}

/// Job executor - runs in background thread, processes jobs serially.
///
/// Uses CoreHandle for all job state operations (message passing, no locks on JobManager).
pub struct JobExecutor {
    /// Channel to receive job IDs
    rx: Receiver<JobId>,
    /// Shared handle for registering cancels
    handle: JobExecutorHandle,
    /// Core handle for job state operations (replaces Arc<Mutex<JobManager>>)
    core: CoreHandle,
    /// Job timeout in milliseconds
    timeout_ms: u64,
}

impl JobExecutor {
    /// Create a new executor and its handle.
    ///
    /// Returns the executor (to be run in a thread) and the handle (to be passed to tools).
    /// Uses CoreHandle for all job state operations via message passing.
    pub fn new(core: CoreHandle) -> (Self, JobExecutorHandle) {
        let (tx, rx) = mpsc::channel();
        let handle = JobExecutorHandle::new(tx);

        let executor = Self {
            rx,
            handle: handle.clone(),
            core,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };

        (executor, handle)
    }

    /// Spawn the executor in a dedicated thread.
    ///
    /// Returns the thread handle for joining on shutdown.
    pub fn spawn(self) -> JoinHandle<()> {
        thread::Builder::new()
            .name("job-executor".to_string())
            .spawn(move || {
                self.run_loop();
            })
            .expect("Failed to spawn executor thread")
    }

    /// Run the executor loop. Blocks until channel closes.
    pub fn run_loop(self) {
        info!("Job executor started");

        // Use recv() which blocks until a message arrives or channel closes
        while let Ok(job_id) = self.rx.recv() {
            self.execute_job(job_id);
        }

        info!("Job executor stopped (channel closed)");
    }

    /// Execute a single job (synchronously)
    ///
    /// Uses CoreHandle for all job state operations (no locks held during I/O).
    fn execute_job(&self, job_id: JobId) {
        info!("Executor picked up job: {}", job_id);

        // Get job spec and check state (via CoreHandle - no locks)
        let job = match self.core.get_job(job_id) {
            Ok(Some(j)) => j,
            Ok(None) => {
                error!("Job not found: {}", job_id);
                return;
            }
            Err(e) => {
                error!("Failed to load job {}: {}", job_id, e);
                return;
            }
        };

        // Skip if already terminal
        if job.state.is_terminal() {
            debug!("Skipping terminal job: {}", job_id);
            return;
        }

        let spec = match job.spec {
            Some(s) => s,
            None => {
                error!("Job {} has no spec, cannot execute", job_id);
                let _ = self
                    .core
                    .fail_job(job_id, "Job has no execution spec".to_string());
                return;
            }
        };

        // Start the job (via CoreHandle - no locks)
        if let Err(e) = self.core.start_job(job_id) {
            error!("Failed to start job {}: {}", job_id, e);
            return;
        }

        // Create cancellation token
        let cancel_token = CancellationToken::new();
        self.handle.register_cancel(&job_id, cancel_token.clone());

        // Execute with timeout using a wrapper thread
        let timeout = Duration::from_millis(self.timeout_ms);
        let result = self.run_with_timeout(&job_id, spec, cancel_token.clone(), timeout);

        // Unregister cancel token
        self.handle.unregister_cancel(&job_id);

        // Handle result (via CoreHandle - no locks held during I/O)
        match result {
            Ok(result_json) => {
                // Check if cancelled before completing
                match self.core.get_job(job_id) {
                    Ok(Some(job)) if matches!(job.state, JobState::Cancelled { .. }) => {
                        debug!("Job {} was cancelled, not marking complete", job_id);
                        return;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to reload job {}: {}", job_id, e);
                        return;
                    }
                }
                if let Err(e) = self.core.complete_job(job_id, result_json) {
                    error!("Failed to complete job {}: {}", job_id, e);
                }
            }
            Err(e) => {
                // Don't overwrite cancelled state
                match self.core.get_job(job_id) {
                    Ok(Some(job)) if matches!(job.state, JobState::Cancelled { .. }) => return,
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to reload job {}: {}", job_id, e);
                        return;
                    }
                }
                error!("Job {} failed: {}", job_id, e);
                let _ = self.core.fail_job(job_id, format!("{:#}", e));
            }
        }
    }

    /// Run a job with timeout
    fn run_with_timeout(
        &self,
        job_id: &JobId,
        spec: JobSpec,
        cancel_token: CancellationToken,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        // For now, execute synchronously with periodic cancellation checks
        // A more sophisticated approach would use a worker thread with timeout
        // but the current job runners already check cancellation periodically
        self.run_job_spec(job_id, spec, cancel_token)
    }

    /// Dispatch to the appropriate job runner based on spec
    fn run_job_spec(
        &self,
        job_id: &JobId,
        spec: JobSpec,
        cancel_token: CancellationToken,
    ) -> Result<serde_json::Value> {
        match spec {
            JobSpec::Backtest {
                plugin_ref,
                input_dir,
                schemas: _,
                redaction: _,
            } => self.run_backtest(job_id, plugin_ref, input_dir, cancel_token),
            JobSpec::Run {
                plugin_ref,
                input_dir,
                output_dir,
                schemas: _,
            } => self.run_parser(job_id, plugin_ref, input_dir, output_dir, cancel_token),
        }
    }

    /// Run a backtest job
    fn run_backtest(
        &self,
        job_id: &JobId,
        plugin_ref: crate::types::PluginRef,
        input_dir: String,
        cancel_token: CancellationToken,
    ) -> Result<serde_json::Value> {
        use casparian_worker::native_runtime::NativeSubprocessRuntime;
        use casparian_worker::runtime::PluginRuntime;

        // Resolve parser path
        let parser_path = resolve_parser_path(&plugin_ref)?;
        info!("Backtest {} using parser: {:?}", job_id, parser_path);

        // Find input files
        let files = find_input_files(&input_dir)?;
        let total_files = files.len();
        info!("Backtest {} found {} files", job_id, total_files);

        if total_files == 0 {
            return Ok(json!({
                "files_total": 0,
                "files_passed": 0,
                "files_failed": 0,
                "pass_rate": 1.0,
                "outputs": {},
                "errors": ["No files found in input directory"]
            }));
        }

        // Update progress - starting
        self.update_progress(job_id, "scanning", 0, Some(total_files as u64), None);

        // Create runtime
        let runtime = NativeSubprocessRuntime::new();

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut errors = Vec::new();
        let mut all_outputs: HashMap<String, usize> = HashMap::new();

        // Process each file
        for (idx, file_path) in files.iter().enumerate() {
            // Check for cancellation
            if cancel_token.is_cancelled() {
                info!(
                    "Backtest {} cancelled at file {}/{}",
                    job_id, idx, total_files
                );
                // Mark as cancelled via CoreHandle (no locks)
                let _ = self.core.cancel_job(*job_id);
                anyhow::bail!("Job cancelled");
            }

            // Update progress
            self.update_progress(
                job_id,
                "processing",
                idx as u64,
                Some(total_files as u64),
                Some(&format!("Processing {}", file_path.display())),
            );

            // Run parser on file
            let ctx = create_run_context(idx, &parser_path);
            match runtime.run_file(&ctx, file_path, &cancel_token) {
                Ok(outputs) => {
                    passed += 1;
                    for info in outputs.output_info {
                        let count = all_outputs.entry(info.name.clone()).or_insert(0);
                        *count += 1;
                    }
                }
                Err(e) => {
                    failed += 1;
                    let error_msg = format!("{}: {}", file_path.display(), e);
                    if errors.len() < 100 {
                        // Limit stored errors
                        errors.push(error_msg.clone());
                    }
                    warn!(
                        "Backtest {} failed on {}: {}",
                        job_id,
                        file_path.display(),
                        e
                    );
                }
            }
        }

        // Final progress update
        self.update_progress(
            job_id,
            "complete",
            total_files as u64,
            Some(total_files as u64),
            None,
        );

        let pass_rate = if total_files > 0 {
            passed as f64 / total_files as f64
        } else {
            1.0
        };

        info!(
            "Backtest {} complete: {}/{} passed ({:.1}%)",
            job_id,
            passed,
            total_files,
            pass_rate * 100.0
        );

        Ok(json!({
            "files_total": total_files,
            "files_passed": passed,
            "files_failed": failed,
            "pass_rate": pass_rate,
            "outputs": all_outputs,
            "errors": errors
        }))
    }

    /// Run a parser job
    fn run_parser(
        &self,
        job_id: &JobId,
        plugin_ref: crate::types::PluginRef,
        input_dir: String,
        output_dir: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<serde_json::Value> {
        use casparian_worker::native_runtime::NativeSubprocessRuntime;
        use casparian_worker::runtime::PluginRuntime;

        // Resolve parser path
        let parser_path = resolve_parser_path(&plugin_ref)?;
        info!("Run {} using parser: {:?}", job_id, parser_path);

        // Find input files
        let files = find_input_files(&input_dir)?;
        let total_files = files.len();
        info!("Run {} found {} files", job_id, total_files);

        if total_files == 0 {
            return Ok(json!({
                "files_total": 0,
                "files_processed": 0,
                "files_failed": 0,
                "outputs": {},
                "errors": ["No files found in input directory"]
            }));
        }

        self.update_progress(job_id, "starting", 0, Some(total_files as u64), None);

        let runtime = NativeSubprocessRuntime::new();

        let mut processed = 0usize;
        let mut failed = 0usize;
        let mut errors = Vec::new();
        let mut all_outputs: HashMap<String, u64> = HashMap::new();

        for (idx, file_path) in files.iter().enumerate() {
            // Check for cancellation
            if cancel_token.is_cancelled() {
                info!("Run {} cancelled at file {}/{}", job_id, idx, total_files);
                // Mark as cancelled via CoreHandle (no locks)
                let _ = self.core.cancel_job(*job_id);
                anyhow::bail!("Job cancelled");
            }

            self.update_progress(
                job_id,
                "processing",
                idx as u64,
                Some(total_files as u64),
                Some(&format!("Processing {}", file_path.display())),
            );

            let ctx = create_run_context(idx, &parser_path);
            match runtime.run_file(&ctx, file_path, &cancel_token) {
                Ok(outputs) => {
                    processed += 1;
                    for info in outputs.output_info {
                        let count = all_outputs.entry(info.name.clone()).or_insert(0);
                        *count += 1; // Count files processed per output
                    }
                }
                Err(e) => {
                    failed += 1;
                    let error_msg = format!("{}: {}", file_path.display(), e);
                    if errors.len() < 100 {
                        errors.push(error_msg);
                    }
                    warn!("Run {} failed on {}: {}", job_id, file_path.display(), e);
                }
            }
        }

        self.update_progress(
            job_id,
            "complete",
            total_files as u64,
            Some(total_files as u64),
            None,
        );

        info!(
            "Run {} complete: {}/{} processed",
            job_id, processed, total_files
        );

        Ok(json!({
            "files_total": total_files,
            "files_processed": processed,
            "files_failed": failed,
            "outputs": all_outputs,
            "output_dir": output_dir,
            "errors": errors
        }))
    }

    /// Update job progress (via CoreHandle - no locks)
    fn update_progress(
        &self,
        job_id: &JobId,
        phase: &str,
        done: u64,
        total: Option<u64>,
        _message: Option<&str>,
    ) {
        let progress = JobProgress::new().with_phase(phase).with_items(done, total);
        let _ = self.core.update_progress(*job_id, progress);
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn resolve_parser_path(plugin_ref: &crate::types::PluginRef) -> Result<PathBuf> {
    use crate::types::PluginRef;

    match plugin_ref {
        PluginRef::Path { path } => {
            let path = PathBuf::from(path);
            if path.ends_with("evtx_native") || path.to_string_lossy().contains("evtx_native") {
                return find_evtx_native_binary(&path);
            }
            if path.exists() {
                Ok(path)
            } else {
                anyhow::bail!("Parser not found: {}", path.display())
            }
        }
        PluginRef::Registered { plugin, version: _ } => {
            if plugin == "evtx_native" {
                find_evtx_native_binary(&PathBuf::from("parsers/evtx_native"))
            } else {
                anyhow::bail!("Unknown registered parser: {}", plugin)
            }
        }
    }
}

fn find_evtx_native_binary(base_path: &std::path::Path) -> Result<PathBuf> {
    let candidates = vec![
        base_path.join("target/release/evtx_native"),
        PathBuf::from("parsers/evtx_native/target/release/evtx_native"),
        PathBuf::from("target/release/evtx_native"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    // Try to build it
    info!("evtx_native binary not found, attempting to build...");
    let plugin_dir = if base_path.join("Cargo.toml").exists() {
        base_path.to_path_buf()
    } else {
        PathBuf::from("parsers/evtx_native")
    };

    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&plugin_dir)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!(
            "Failed to build evtx_native parser. Ensure the Rust toolchain is installed (rustup + cargo) and retry, or run `cargo build --release` in {}.",
            plugin_dir.display()
        );
    }

    let binary = plugin_dir.join("target/release/evtx_native");
    if binary.exists() {
        Ok(binary)
    } else {
        anyhow::bail!("evtx_native binary not found after build")
    }
}

fn find_input_files(input_dir: &str) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    let input_path = PathBuf::from(input_dir);
    if !input_path.exists() {
        anyhow::bail!("Input directory does not exist: {}", input_dir);
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&input_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if entry.file_type().is_symlink() {
            continue;
        }
        if path.is_file() {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

fn create_run_context(
    file_idx: usize,
    parser_path: &std::path::Path,
) -> casparian_worker::runtime::RunContext {
    use casparian_protocol::JobId as ProtoJobId;

    let proto_job_id = ProtoJobId::new(file_idx as u64);

    // Provide wildcard schema hash - backtest validates outputs but doesn't require
    // exact schema matching (that's what it's testing)
    let mut schema_hashes = HashMap::new();
    // Use wildcard "*" to accept any output in backtest mode
    schema_hashes.insert("*".to_string(), "backtest".to_string());

    casparian_worker::runtime::RunContext {
        job_id: proto_job_id,
        file_id: file_idx as i64,
        entrypoint: parser_path.to_string_lossy().to_string(),
        env_hash: None,
        source_code: None,
        schema_hashes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::spawn_core;

    #[test]
    fn test_executor_handle_enqueue() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");

        // Spawn Core (owns JobManager)
        let (core, _events, _thread) = spawn_core(db_path).unwrap();

        let (executor, handle) = JobExecutor::new(core.clone());

        // Enqueue should work
        let job_id = JobId::new(1);
        handle.enqueue(job_id.clone()).unwrap();

        // Drop executor to close channel
        drop(executor);

        // Shutdown core
        let _ = core.shutdown();
    }

    #[test]
    fn test_executor_handle_cancel_no_job() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");

        // Spawn Core (owns JobManager)
        let (core, _events, _thread) = spawn_core(db_path).unwrap();

        let (_executor, handle) = JobExecutor::new(core.clone());

        // Cancel non-existent job should return false
        let job_id = JobId::new(1);
        let cancelled = handle.cancel(&job_id).unwrap();
        assert!(!cancelled);

        // Shutdown core
        let _ = core.shutdown();
    }
}

//! Bridge Executor: Spawns Python subprocess and streams Arrow IPC data
//!
//! Implements IPC via TCP (127.0.0.1:port) for cross-platform compatibility.
//! All I/O is synchronous and runs in a blocking thread pool.
//!
//! ## Single Binary Distribution
//! The bridge shim Python code is embedded in the binary at compile time.
//! At runtime, it's materialized to `~/.casparian_flow/shim/{version}/bridge_shim.py`.
//!
//! ## Transport
//! Uses TCP on localhost (127.0.0.1) with automatic port allocation for
//! Windows compatibility (Unix sockets not available on Windows).
//! The Python guest connects to BRIDGE_PORT environment variable.
//!
//! ## Timeouts
//! - Connection timeout: 30 seconds for Python to connect to the socket
//! - Read timeout: 60 seconds per read operation
//! - These ensure jobs don't hang indefinitely if Python crashes or hangs

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::StreamReader;
use casparian_protocol::JobId;
use casparian_sinks::OutputBatch;
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::cancel::CancellationToken;
/// Embedded Python bridge shim source code.
/// This is baked into the binary at compile time for single-file distribution.
const BRIDGE_SHIM_SOURCE: &str = include_str!("../shim/bridge_shim.py");

/// Embedded casparian_types.py - the Output NamedTuple contract for parsers.
/// Must be materialized alongside bridge_shim.py so imports work.
const CASPARIAN_TYPES_SOURCE: &str = include_str!("../shim/casparian_types.py");

/// Crate version for shim cache path versioning.
/// When the shim changes, the version bump ensures old cached shims are replaced.
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

const HEADER_SIZE: usize = 4;
const END_OF_STREAM: u32 = 0;
const ERROR_SIGNAL: u32 = 0xFFFF_FFFF;
const LOG_SIGNAL: u32 = 0xFFFF_FFFE; // Sideband logging signal
const OUTPUT_START_SIGNAL: u32 = 0xFFFF_FFFD;
const OUTPUT_END_SIGNAL: u32 = 0xFFFF_FFFC;
const METRICS_SIGNAL: u32 = 0xFFFF_FFFB;

const _: () = {
    assert!(LOG_SIGNAL > 100 * 1024 * 1024);
    assert!(OUTPUT_START_SIGNAL > 100 * 1024 * 1024);
    assert!(OUTPUT_END_SIGNAL > 100 * 1024 * 1024);
};

/// Log levels (must match Python bridge_shim.py)
#[allow(dead_code)]
mod log_level {
    pub const STDOUT: u8 = 0;
    pub const STDERR: u8 = 1;
    pub const DEBUG: u8 = 2;
    pub const INFO: u8 = 3;
    pub const WARNING: u8 = 4;
    pub const ERROR: u8 = 5;
}

/// Maximum size for error messages from guest (1 MB)
/// Prevents OOM from malicious or buggy guest processes
const MAX_ERROR_MESSAGE_SIZE: u32 = 1024 * 1024;
const MAX_METRICS_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Maximum log file size (10 MB) - prevents disk exhaustion
const MAX_LOG_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Timeout for Python guest to connect to Unix socket
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for read operations on the socket
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Poll interval for cancellation checks while reading
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Errors returned by bridge operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("{message}")]
    Message { message: String },
    #[error("{message}")]
    Source {
        message: String,
        #[source]
        source: anyhow::Error,
    },
}

pub type BridgeExecResult<T> = std::result::Result<T, BridgeError>;

impl From<anyhow::Error> for BridgeError {
    fn from(err: anyhow::Error) -> Self {
        BridgeError::Source {
            message: err.to_string(),
            source: err,
        }
    }
}

/// Streaming log writer that writes to a temp file with size cap.
/// Memory usage is O(1) regardless of log volume - key for preventing OOM.
struct JobLogWriter {
    writer: std::io::BufWriter<std::fs::File>,
    path: PathBuf,
    bytes_written: usize,
    truncated: bool,
}

impl JobLogWriter {
    /// Create a new log writer for the given job.
    /// Creates the log directory if needed.
    /// Uses both job_id and process ID to ensure uniqueness when running tests in parallel.
    fn new(job_id: JobId) -> Result<Self> {
        let mut log_dir = std::env::temp_dir();
        log_dir.push("casparian_logs");
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        // Include process ID to avoid collisions when tests run in parallel with same job_id
        let path = log_dir.join(format!("{}_{}.log", job_id, std::process::id()));
        let file = std::fs::File::create(&path)
            .with_context(|| format!("Failed to create log file: {}", path.display()))?;

        Ok(Self {
            writer: std::io::BufWriter::with_capacity(8192, file),
            path,
            bytes_written: 0,
            truncated: false,
        })
    }

    /// Write a log line with level prefix.
    /// Returns silently if file is truncated (over limit).
    fn write_log(&mut self, level: u8, message: &str) {
        if self.truncated {
            return;
        }

        // Check if we would exceed limit
        let prefix = match level {
            log_level::STDOUT => "[STDOUT] ",
            log_level::STDERR => "[STDERR] ",
            log_level::DEBUG => "[DEBUG] ",
            log_level::INFO => "[INFO] ",
            log_level::WARNING => "[WARN] ",
            log_level::ERROR => "[ERROR] ",
            _ => "[LOG] ",
        };

        let line = format!("{}{}\n", prefix, message);
        let line_bytes = line.as_bytes();

        if self.bytes_written + line_bytes.len() > MAX_LOG_FILE_SIZE {
            // Write truncation notice and stop
            let notice = "\n[SYSTEM] Log truncated (exceeded 10MB limit)\n";
            let _ = self.writer.write_all(notice.as_bytes());
            self.truncated = true;
            return;
        }

        if let Err(e) = self.writer.write_all(line_bytes) {
            warn!("Failed to write log line: {}", e);
            return;
        }

        self.bytes_written += line_bytes.len();
    }

    /// Flush and close the log file, returning the path.
    fn finish(mut self) -> Result<PathBuf> {
        self.writer
            .flush()
            .with_context(|| format!("Failed to flush log file: {}", self.path.display()))?;
        debug!(
            "Log file closed: {} ({} bytes)",
            self.path.display(),
            self.bytes_written
        );
        Ok(self.path)
    }

    /// Read the log file contents and delete the file.
    /// Returns the log text (capped at 10MB).
    fn read_and_cleanup(self) -> Result<String> {
        let path = self.finish()?;
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read log file: {}", path.display()))?;

        // Delete temp file
        let _ = std::fs::remove_file(&path);

        Ok(content)
    }
}

/// Bridge execution configuration (plain data, no behavior)
#[derive(Debug)]
pub struct BridgeConfig {
    pub interpreter_path: PathBuf,
    pub source_code: String,
    pub file_path: String,
    pub job_id: JobId,
    pub file_id: i64,
    pub shim_path: PathBuf,
    pub inherit_stdio: bool,
    pub cancel_token: CancellationToken,
}

/// Metadata about a single output from a parser
#[derive(Debug, Clone, Deserialize)]
pub struct OutputInfo {
    /// Output identifier (topic name)
    pub name: String,
    /// Optional table name override (defaults to output name)
    pub table: Option<String>,
}

/// Result of bridge execution including data and logs
#[derive(Debug)]
pub struct BridgeResult {
    /// Arrow record batches grouped by output (per publish call)
    pub output_batches: Vec<Vec<OutputBatch>>,
    /// Captured logs from the plugin (stdout, stderr, logging)
    /// This is O(1) memory during execution but loaded at end (capped at 10MB)
    pub logs: String,
    /// Output metadata from the parser
    pub output_info: Vec<OutputInfo>,
}

struct StreamResult {
    output_batches: Vec<Vec<RecordBatch>>,
    metrics_json: Option<String>,
}

/// Execute a bridge job. This is the only public entry point.
/// Returns both the data batches and captured logs.
pub fn execute_bridge(config: BridgeConfig) -> BridgeExecResult<BridgeResult> {
    execute_bridge_sync(config).map_err(BridgeError::from)
}

/// Synchronous bridge execution - no async lies here
fn execute_bridge_sync(config: BridgeConfig) -> Result<BridgeResult> {
    let job_id = config.job_id;

    // Create log writer for sideband logging (streams to disk, not RAM)
    let mut log_writer = JobLogWriter::new(job_id)
        .with_context(|| format!("[Job {}] Failed to create log writer", job_id))?;

    // Bind TCP listener on localhost with automatic port allocation
    let listener = TcpListener::bind("127.0.0.1:0")
        .with_context(|| "[Job {}] Failed to bind TCP listener on 127.0.0.1:0")?;

    let port = listener
        .local_addr()
        .with_context(|| format!("[Job {}] Failed to get local address", job_id))?
        .port();

    debug!(
        "[Job {}] Bridge TCP listener bound to 127.0.0.1:{}",
        job_id, port
    );

    let mut process = spawn_guest(&config, port)?;
    if config.cancel_token.is_cancelled() {
        cleanup_process(&mut process);
        anyhow::bail!("[Job {}] Cancelled before guest connected", job_id);
    }
    let process_pid = process.id();

    // Accept connection WITH TIMEOUT
    let mut stream = match accept_with_timeout(
        &listener,
        CONNECT_TIMEOUT,
        &mut process,
        job_id,
        &config.cancel_token,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            // Collect stderr for debugging before returning error
            let stderr_output = collect_stderr(&mut process);
            cleanup_process(&mut process);

            if !stderr_output.is_empty() {
                error!(
                    "[Job {}] Guest stderr before connection failure:\n{}",
                    job_id, stderr_output
                );
                log_writer.write_log(log_level::STDERR, &stderr_output);
            }
            // Still return the logs even on failure
            let logs = log_writer.read_and_cleanup().unwrap_or_default();
            return Err(e.context(format!("Logs:\n{}", logs)));
        }
    };

    debug!(
        "[Job {}] Guest process (pid={}) connected",
        job_id, process_pid
    );

    // Set read timeout on the stream (may already be set in accept_with_timeout)
    // On macOS, this can fail with EINVAL if the peer has already closed
    // We try it anyway but don't fail if it doesn't work - the read will still work
    if let Err(e) = stream.set_read_timeout(Some(CANCEL_POLL_INTERVAL)) {
        warn!(
            "[Job {}] Could not set read timeout (may already be set or peer closed): {}",
            job_id, e
        );
    }

    // Read all batches (log_writer receives sideband log messages)
    let stream_result =
        match read_arrow_batches(&mut stream, job_id, &mut log_writer, &config.cancel_token) {
            Ok(result) => result,
            Err(e) => {
                let stderr_output = collect_stderr(&mut process);
                cleanup_process(&mut process);

                if !stderr_output.is_empty() {
                    error!(
                        "[Job {}] Guest stderr during read failure:\n{}",
                        job_id, stderr_output
                    );
                    log_writer.write_log(log_level::STDERR, &stderr_output);
                }
                let logs = log_writer.read_and_cleanup().unwrap_or_default();
                return Err(e.context(format!("Logs:\n{}", logs)));
            }
        };

    // Wait for process to exit
    let status = wait_for_exit(&mut process, job_id, &config.cancel_token)
        .with_context(|| format!("[Job {}] Failed to wait for guest process", job_id))?;

    // Always collect stderr for logging (even on success)
    let stderr_output = collect_stderr(&mut process);

    if !status.success() {
        // TCP socket cleanup is automatic when listener is dropped

        if !stderr_output.is_empty() {
            error!("[Job {}] Guest stderr:\n{}", job_id, stderr_output);
            log_writer.write_log(log_level::STDERR, &stderr_output);
        }
        let logs = log_writer.read_and_cleanup().unwrap_or_default();
        anyhow::bail!(
            "[Job {}] Guest process (pid={}) exited with {}: {}\n\nLogs:\n{}",
            job_id,
            process_pid,
            status,
            if stderr_output.is_empty() {
                "(no stderr output)"
            } else {
                &stderr_output
            },
            logs
        );
    }

    // Log warnings from stderr even on success (append to sideband logs)
    if !stderr_output.is_empty() {
        warn!(
            "[Job {}] Guest stderr (process succeeded but had output):\n{}",
            job_id, stderr_output
        );
        log_writer.write_log(log_level::STDERR, &stderr_output);
    }

    let output_batches = stream_result.output_batches;
    let metrics_json = stream_result
        .metrics_json
        .ok_or_else(|| anyhow::anyhow!("[Job {}] Missing metrics payload from bridge", job_id))?;

    // Parse output_info from JSON metrics
    let output_info = parse_output_info(job_id, "socket", &metrics_json);

    // TCP socket cleanup is automatic when listener goes out of scope

    // Read and cleanup log file
    let logs = log_writer
        .read_and_cleanup()
        .with_context(|| format!("[Job {}] Failed to read logs", job_id))?;

    if !output_info.is_empty() && output_info.len() != output_batches.len() {
        warn!(
            "[Job {}] Output info count ({}) does not match output batches ({})",
            job_id,
            output_info.len(),
            output_batches.len()
        );
    }

    let total_batches: usize = output_batches.iter().map(|batches| batches.len()).sum();
    info!(
        "[Job {}] Bridge execution complete: {} batches, {} outputs, {} bytes logs, {} output_info",
        job_id,
        total_batches,
        output_batches.len(),
        logs.len(),
        output_info.len()
    );

    let output_batches = output_batches
        .into_iter()
        .map(|batches| {
            batches
                .into_iter()
                .map(OutputBatch::from_record_batch)
                .collect()
        })
        .collect();

    Ok(BridgeResult {
        output_batches,
        logs,
        output_info,
    })
}

/// Accept a TCP connection with timeout, checking if process is still alive
fn accept_with_timeout(
    listener: &TcpListener,
    timeout: Duration,
    process: &mut Child,
    job_id: JobId,
    cancel_token: &CancellationToken,
) -> Result<TcpStream> {
    // Use non-blocking mode with polling
    listener.set_nonblocking(true).with_context(|| {
        format!(
            "[Job {}] Failed to set TCP listener to non-blocking",
            job_id
        )
    })?;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        if cancel_token.is_cancelled() {
            cleanup_process(process);
            anyhow::bail!("[Job {}] Cancelled while waiting for guest", job_id);
        }

        // Check if we've exceeded the timeout
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            anyhow::bail!(
                "[Job {}] TIMEOUT: Guest process did not connect to TCP port within {:.1}s. \
                The Python subprocess may have crashed during startup, failed to import dependencies, \
                or the bridge_shim.py may not be connecting to BRIDGE_PORT. \
                Check the guest stderr output above for details.",
                job_id,
                timeout.as_secs_f64()
            );
        }

        // Try to accept connection FIRST - a connection may be pending even if process exited
        match listener.accept() {
            Ok((stream, _)) => {
                // Set read timeout first (while still non-blocking) - this helps on macOS
                // where the order of socket option calls matters
                if let Err(e) = stream.set_read_timeout(Some(CANCEL_POLL_INTERVAL)) {
                    debug!(
                        "[Job {}] Could not set read timeout in accept: {}",
                        job_id, e
                    );
                    // Continue anyway - we'll set it later if needed
                }

                // Switch back to blocking mode for the stream
                stream.set_nonblocking(false).with_context(|| {
                    format!("[Job {}] Failed to set TCP stream to blocking mode", job_id)
                })?;

                debug!(
                    "[Job {}] Guest connected after {:.2}s",
                    job_id,
                    elapsed.as_secs_f64()
                );
                return Ok(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection yet, check if process is still alive
            }
            Err(e) => {
                anyhow::bail!("[Job {}] Failed to accept TCP connection: {}", job_id, e);
            }
        }

        // Only check if process exited when there's no pending connection
        match process.try_wait() {
            Ok(Some(status)) => {
                // Process exited - try once more to accept in case connection was queued
                match listener.accept() {
                    Ok((stream, _)) => {
                        // Try to set timeout, ignore if it fails on dead connection
                        let _ = stream.set_read_timeout(Some(CANCEL_POLL_INTERVAL));
                        stream.set_nonblocking(false).with_context(|| {
                            format!("[Job {}] Failed to set TCP stream to blocking mode", job_id)
                        })?;
                        debug!(
                            "[Job {}] Guest connected after {:.2}s (process already exited)",
                            job_id,
                            elapsed.as_secs_f64()
                        );
                        return Ok(stream);
                    }
                    Err(_) => {
                        anyhow::bail!(
                            "[Job {}] Guest process exited with {} before connecting to TCP port. \
                            The Python subprocess crashed during startup. \
                            Check the guest stderr output above for details.",
                            job_id,
                            status
                        );
                    }
                }
            }
            Ok(None) => {
                // Process still running, sleep and retry
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                anyhow::bail!(
                    "[Job {}] Failed to check guest process status: {}",
                    job_id,
                    e
                );
            }
        }
    }
}

/// Collect stderr from process (consumes the stderr handle)
fn collect_stderr(process: &mut Child) -> String {
    if let Some(mut stderr) = process.stderr.take() {
        let mut output = String::new();
        match stderr.read_to_string(&mut output) {
            Ok(_) => output.trim().to_string(),
            Err(e) => format!("(failed to read stderr: {})", e),
        }
    } else {
        String::new()
    }
}

fn parse_output_info(job_id: JobId, source: &str, json_text: &str) -> Vec<OutputInfo> {
    match serde_json::from_str::<serde_json::Value>(json_text) {
        Ok(json) => {
            if let Some(info_array) = json.get("output_info").and_then(|v| v.as_array()) {
                info_array
                    .iter()
                    .filter_map(|v| serde_json::from_value::<OutputInfo>(v.clone()).ok())
                    .collect()
            } else {
                debug!(
                    "[Job {}] No output_info in metrics JSON ({})",
                    job_id, source
                );
                vec![]
            }
        }
        Err(e) => {
            warn!(
                "[Job {}] Failed to parse metrics JSON ({}): {}",
                job_id, source, e
            );
            vec![]
        }
    }
}

/// Kill process (TCP socket cleanup is automatic)
fn cleanup_process(process: &mut Child) {
    let _ = process.kill();
    let _ = process.wait();
}

/// Wait for process exit, honoring cancellation.
fn wait_for_exit(
    process: &mut Child,
    job_id: JobId,
    cancel_token: &CancellationToken,
) -> Result<std::process::ExitStatus> {
    loop {
        if cancel_token.is_cancelled() {
            cleanup_process(process);
            anyhow::bail!("[Job {}] Cancelled while waiting for guest exit", job_id);
        }
        if let Some(status) = process.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(CANCEL_POLL_INTERVAL);
    }
}

/// Spawn the guest Python process.
///
/// Prefers `uv run` for correct Python environment setup on all platforms.
/// Falls back to spawning the interpreter directly if uv is unavailable.
fn spawn_guest(config: &BridgeConfig, port: u16) -> Result<Child> {
    if let Some(uv_path) = find_uv_path() {
        return spawn_guest_with_uv(config, port, &uv_path);
    }

    if cfg!(target_os = "macos") {
        warn!(
            "[Job {}] uv not found; spawning Python directly. \
            macOS multiprocessing may require __PYVENV_LAUNCHER__. Install uv if you see failures.",
            config.job_id
        );
    } else {
        warn!(
            "[Job {}] uv not found; spawning Python directly.",
            config.job_id
        );
    }

    spawn_guest_direct(config, port)
}

fn spawn_guest_with_uv(config: &BridgeConfig, port: u16, uv_path: &Path) -> Result<Child> {
    use base64::{engine::general_purpose, Engine as _};
    let source_b64 = general_purpose::STANDARD.encode(&config.source_code);

    // Use uv to spawn Python - it knows how to set up the environment correctly
    let mut cmd = Command::new(uv_path);

    // NOTE: Do NOT use env_clear() - macOS Python needs inherited env vars
    // like __PYVENV_LAUNCHER__, LC_CTYPE, etc. for multiprocessing to work.
    // uv will handle passing these through correctly.

    // uv run --frozen --python <interpreter> <script>
    cmd.arg("run")
        .arg("--frozen")
        .arg("--python")
        .arg(&config.interpreter_path)
        .arg(&config.shim_path);

    // Bridge context vars - use BRIDGE_PORT for TCP transport
    cmd.env("BRIDGE_PORT", port.to_string())
        .env("BRIDGE_PLUGIN_CODE", source_b64)
        .env("BRIDGE_FILE_PATH", &config.file_path)
        .env("BRIDGE_JOB_ID", config.job_id.to_string())
        .env("BRIDGE_FILE_ID", config.file_id.to_string())
        .env(
            "BRIDGE_STDIO_MODE",
            if config.inherit_stdio {
                "inherit"
            } else {
                "piped"
            },
        );

    if config.inherit_stdio {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    // NOTE: uv sets VIRTUAL_ENV automatically, no need to do it ourselves

    let child = cmd.spawn().with_context(|| {
        format!(
            "[Job {}] Failed to spawn guest process via 'uv run'. \
            Is uv installed? Interpreter: {}, Shim: {}",
            config.job_id,
            config.interpreter_path.display(),
            config.shim_path.display()
        )
    })?;

    info!(
        "[Job {}] Spawned guest via uv (pid={}) using interpreter {}, port {}",
        config.job_id,
        child.id(),
        config.interpreter_path.display(),
        port
    );

    Ok(child)
}

fn spawn_guest_direct(config: &BridgeConfig, port: u16) -> Result<Child> {
    use base64::{engine::general_purpose, Engine as _};
    let source_b64 = general_purpose::STANDARD.encode(&config.source_code);

    let mut cmd = Command::new(&config.interpreter_path);
    cmd.arg(&config.shim_path);

    // Bridge context vars - use BRIDGE_PORT for TCP transport
    cmd.env("BRIDGE_PORT", port.to_string())
        .env("BRIDGE_PLUGIN_CODE", source_b64)
        .env("BRIDGE_FILE_PATH", &config.file_path)
        .env("BRIDGE_JOB_ID", config.job_id.to_string())
        .env("BRIDGE_FILE_ID", config.file_id.to_string())
        .env(
            "BRIDGE_STDIO_MODE",
            if config.inherit_stdio {
                "inherit"
            } else {
                "piped"
            },
        );

    if let Some(venv_root) = venv_root_for_interpreter(&config.interpreter_path) {
        cmd.env("VIRTUAL_ENV", venv_root);
    }

    if config.inherit_stdio {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    let child = cmd.spawn().with_context(|| {
        format!(
            "[Job {}] Failed to spawn guest process directly. Interpreter: {}, Shim: {}",
            config.job_id,
            config.interpreter_path.display(),
            config.shim_path.display()
        )
    })?;

    info!(
        "[Job {}] Spawned guest directly (pid={}) using interpreter {}, port {}",
        config.job_id,
        child.id(),
        config.interpreter_path.display(),
        port
    );

    Ok(child)
}

fn venv_root_for_interpreter(interpreter: &Path) -> Option<PathBuf> {
    interpreter.parent().and_then(|bin_dir| bin_dir.parent()).map(ToOwned::to_owned)
}

fn find_uv_path() -> Option<PathBuf> {
    if let Ok(path) = which::which("uv") {
        return Some(path);
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.cargo/bin/uv", home),
        format!("{}/.local/bin/uv", home),
        "/usr/local/bin/uv".to_string(),
    ];

    for candidate in candidates {
        let path = PathBuf::from(&candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Read Arrow IPC batches from TCP stream, handling sideband log messages
fn read_arrow_batches(
    stream: &mut TcpStream,
    job_id: JobId,
    log_writer: &mut JobLogWriter,
    cancel_token: &CancellationToken,
) -> Result<StreamResult> {
    let mut outputs: Vec<Vec<RecordBatch>> = Vec::new();
    let mut metrics_json: Option<String> = None;
    let mut current_output: Option<Vec<RecordBatch>> = None;
    let mut current_output_index: Option<u32> = None;
    let mut current_output_schema: Option<SchemaRef> = None;
    let mut batch_count = 0u32;
    let mut log_count = 0u32;
    let mut last_activity = Instant::now();

    loop {
        if cancel_token.is_cancelled() {
            anyhow::bail!("[Job {}] Cancelled during bridge read", job_id);
        }
        // Read 4-byte header
        let mut header_buf = [0u8; HEADER_SIZE];
        match stream.read_exact(&mut header_buf) {
            Ok(_) => {
                last_activity = Instant::now();
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Connection closed cleanly
                debug!("[Job {}] Socket closed by guest (EOF)", job_id);
                break;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if cancel_token.is_cancelled() {
                    anyhow::bail!("[Job {}] Cancelled during bridge read", job_id);
                }
                if last_activity.elapsed() >= READ_TIMEOUT {
                    anyhow::bail!(
                        "[Job {}] TIMEOUT: No data received from guest within {:.0}s. \
                        The Python plugin may be hanging or performing very slow I/O. \
                        Received {} batches before timeout.",
                        job_id,
                        READ_TIMEOUT.as_secs_f64(),
                        batch_count
                    );
                }
                continue;
            }
            Err(e) => {
                anyhow::bail!(
                    "[Job {}] Failed to read header from socket after {} batches: {}",
                    job_id,
                    batch_count,
                    e
                );
            }
        }

        let length = u32::from_be_bytes(header_buf);

        // End of stream signal
        if length == END_OF_STREAM {
            debug!(
                "[Job {}] Received end-of-stream signal after {} batches, {} logs, {} outputs",
                job_id,
                batch_count,
                log_count,
                outputs.len()
            );
            break;
        }

        // Error signal
        if length == ERROR_SIGNAL {
            let error_msg = read_error_message(stream, job_id)?;
            anyhow::bail!("[Job {}] Guest process error: {}", job_id, error_msg);
        }

        // Log signal - sideband logging from Python
        if length == LOG_SIGNAL {
            read_and_write_log(stream, job_id, log_writer)?;
            log_count += 1;
            continue; // Don't treat as data batch
        }

        if length == METRICS_SIGNAL {
            let metrics_payload = read_metrics_payload(stream, job_id)?;
            if metrics_json.is_some() {
                warn!("[Job {}] Received duplicate metrics payload", job_id);
            }
            metrics_json = Some(metrics_payload);
            continue;
        }

        if length == OUTPUT_START_SIGNAL {
            let mut index_buf = [0u8; HEADER_SIZE];
            stream
                .read_exact(&mut index_buf)
                .with_context(|| format!("[Job {}] Failed to read output start index", job_id))?;
            let output_index = u32::from_be_bytes(index_buf);
            if current_output.is_some() {
                anyhow::bail!(
                    "[Job {}] Received OUTPUT_START for output {} while another output is open (index {:?})",
                    job_id,
                    output_index,
                    current_output_index
                );
            }
            current_output = Some(Vec::new());
            current_output_index = Some(output_index);
            current_output_schema = None;
            debug!("[Job {}] Output {} started", job_id, output_index);
            continue;
        }

        if length == OUTPUT_END_SIGNAL {
            let mut index_buf = [0u8; HEADER_SIZE];
            stream
                .read_exact(&mut index_buf)
                .with_context(|| format!("[Job {}] Failed to read output end index", job_id))?;
            let end_index = u32::from_be_bytes(index_buf);

            // Validate index matches the active output
            if let Some(start_index) = current_output_index {
                if start_index != end_index {
                    anyhow::bail!(
                        "[Job {}] OUTPUT_END index {} does not match OUTPUT_START index {} - protocol error",
                        job_id,
                        end_index,
                        start_index
                    );
                }
            }

            if let Some(output_batches) = current_output.take() {
                outputs.push(output_batches);
                current_output_index = None;
                current_output_schema = None;
                debug!("[Job {}] Output {} ended", job_id, end_index);
            } else {
                anyhow::bail!(
                    "[Job {}] Received OUTPUT_END for output {} without active output",
                    job_id,
                    end_index
                );
            }
            continue;
        }

        // Sanity check on payload size (max 100MB per batch to prevent OOM)
        const MAX_BATCH_SIZE: u32 = 100 * 1024 * 1024;
        if length > MAX_BATCH_SIZE {
            anyhow::bail!(
                "[Job {}] Arrow IPC batch size {} bytes exceeds maximum {} bytes. \
                This may indicate a protocol error or corrupted data.",
                job_id,
                length,
                MAX_BATCH_SIZE
            );
        }

        // Read Arrow IPC payload
        let mut ipc_buf = vec![0u8; length as usize];
        stream.read_exact(&mut ipc_buf).with_context(|| {
            format!(
                "[Job {}] Failed to read Arrow IPC payload ({} bytes) after {} batches",
                job_id, length, batch_count
            )
        })?;

        debug!(
            "[Job {}] Received {} bytes of Arrow IPC data",
            job_id, length
        );

        // Parse Arrow IPC stream
        let cursor = std::io::Cursor::new(ipc_buf);
        let reader = StreamReader::try_new(cursor, None).with_context(|| {
            format!(
                "[Job {}] Failed to parse Arrow IPC stream (output {})",
                job_id, batch_count
            )
        })?;

        if current_output.is_some() {
            let schema = reader.schema();
            match &current_output_schema {
                Some(expected) if expected.as_ref() != schema.as_ref() => {
                    anyhow::bail!(
                        "[Job {}] Schema mismatch within output {:?}: expected {:?}, got {:?}",
                        job_id,
                        current_output_index,
                        expected,
                        schema
                    );
                }
                Some(_) => {}
                None => current_output_schema = Some(schema.clone()),
            }
        }

        let mut ipc_batches: Vec<RecordBatch> = Vec::new();
        for batch_result in reader {
            let batch = batch_result.with_context(|| {
                format!(
                    "[Job {}] Failed to read Arrow batch from IPC stream",
                    job_id
                )
            })?;
            ipc_batches.push(batch);
        }

        if let Some(output_batches) = current_output.as_mut() {
            output_batches.extend(ipc_batches);
        } else if !ipc_batches.is_empty() {
            // Legacy mode: no output boundaries, treat each IPC message as a single output
            outputs.push(ipc_batches);
        }

        batch_count += 1;
    }

    if let Some(output_batches) = current_output.take() {
        warn!(
            "[Job {}] Output stream ended without OUTPUT_END (index {:?}); closing open output",
            job_id, current_output_index
        );
        outputs.push(output_batches);
    }

    Ok(StreamResult {
        output_batches: outputs,
        metrics_json,
    })
}

/// Read a log message from the TCP stream and write to the log file.
/// Protocol: [LEVEL:1][LENGTH:4][MESSAGE]
fn read_and_write_log(
    stream: &mut TcpStream,
    job_id: JobId,
    log_writer: &mut JobLogWriter,
) -> Result<()> {
    // Read 1-byte log level
    let mut level_buf = [0u8; 1];
    stream
        .read_exact(&mut level_buf)
        .with_context(|| format!("[Job {}] Failed to read log level", job_id))?;
    let level = level_buf[0];

    // Read 4-byte message length
    let mut len_buf = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut len_buf)
        .with_context(|| format!("[Job {}] Failed to read log message length", job_id))?;
    let msg_len = u32::from_be_bytes(len_buf);

    // Enforce size limit (64KB per message - same as Python side)
    const MAX_LOG_MESSAGE: u32 = 65536;
    if msg_len > MAX_LOG_MESSAGE {
        anyhow::bail!(
            "[Job {}] Log message size {} bytes exceeds maximum {} bytes",
            job_id,
            msg_len,
            MAX_LOG_MESSAGE
        );
    }

    // Read message bytes
    let mut msg_buf = vec![0u8; msg_len as usize];
    stream
        .read_exact(&mut msg_buf)
        .with_context(|| format!("[Job {}] Failed to read log message body", job_id))?;

    // Convert to string (lossy for invalid UTF-8)
    let message = String::from_utf8_lossy(&msg_buf);

    // Write to log file (O(1) memory - streams to disk)
    log_writer.write_log(level, &message);

    Ok(())
}

/// Read error message after ERROR_SIGNAL with size limit
fn read_error_message(stream: &mut TcpStream, job_id: JobId) -> Result<String> {
    let mut len_buf = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut len_buf)
        .with_context(|| format!("[Job {}] Failed to read error message length", job_id))?;

    let error_len = u32::from_be_bytes(len_buf);

    // Enforce size limit to prevent OOM attacks
    if error_len > MAX_ERROR_MESSAGE_SIZE {
        anyhow::bail!(
            "[Job {}] Error message size {} bytes exceeds maximum {} bytes. \
            Possible protocol error or malicious guest.",
            job_id,
            error_len,
            MAX_ERROR_MESSAGE_SIZE
        );
    }

    let mut error_buf = vec![0u8; error_len as usize];
    stream.read_exact(&mut error_buf).with_context(|| {
        format!(
            "[Job {}] Failed to read error message body ({} bytes)",
            job_id, error_len
        )
    })?;

    Ok(String::from_utf8_lossy(&error_buf).to_string())
}

/// Read metrics JSON after METRICS_SIGNAL with size limit
fn read_metrics_payload(stream: &mut TcpStream, job_id: JobId) -> Result<String> {
    let mut len_buf = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut len_buf)
        .with_context(|| format!("[Job {}] Failed to read metrics length", job_id))?;

    let msg_len = u32::from_be_bytes(len_buf);
    if msg_len > MAX_METRICS_MESSAGE_SIZE {
        anyhow::bail!(
            "[Job {}] Metrics payload size {} exceeds maximum {} bytes",
            job_id,
            msg_len,
            MAX_METRICS_MESSAGE_SIZE
        );
    }

    let mut msg_buf = vec![0u8; msg_len as usize];
    stream.read_exact(&mut msg_buf).with_context(|| {
        format!(
            "[Job {}] Failed to read metrics payload ({} bytes)",
            job_id, msg_len
        )
    })?;

    String::from_utf8(msg_buf)
        .with_context(|| format!("[Job {}] Metrics payload is not valid UTF-8", job_id))
}

/// Materialize the embedded bridge shim and casparian_types to the filesystem.
///
/// The files are written to `~/.casparian_flow/shim/{version}/`:
/// - bridge_shim.py - Main bridge execution code
/// - casparian_types.py - Output NamedTuple contract for parsers
///
/// This ensures the single binary can run from any location without
/// needing the source repository.
///
/// The function is idempotent: if files exist and match, they're reused.
/// Version changes cause a new directory to be created.
pub fn materialize_bridge_shim() -> BridgeExecResult<PathBuf> {
    materialize_bridge_shim_inner().map_err(BridgeError::from)
}

fn materialize_bridge_shim_inner() -> Result<PathBuf> {
    // Resolve cache directory: ~/.casparian_flow/shim/{version}/
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory (HOME or USERPROFILE not set)")?;

    let shim_dir = PathBuf::from(home)
        .join(".casparian_flow")
        .join("shim")
        .join(CRATE_VERSION);

    let shim_path = shim_dir.join("bridge_shim.py");
    let types_path = shim_dir.join("casparian_types.py");

    // Check if both files exist and content matches (fast path)
    let shim_ok = shim_path.exists()
        && matches!(
            std::fs::read_to_string(&shim_path),
            Ok(existing) if existing == BRIDGE_SHIM_SOURCE
        );
    let types_ok = types_path.exists()
        && matches!(
            std::fs::read_to_string(&types_path),
            Ok(existing) if existing == CASPARIAN_TYPES_SOURCE
        );

    if shim_ok && types_ok {
        debug!("Using cached bridge shim: {}", shim_path.display());
        return Ok(shim_path);
    }

    // Create directory if needed
    std::fs::create_dir_all(&shim_dir)
        .with_context(|| format!("Failed to create shim directory: {}", shim_dir.display()))?;

    // Helper to write a file atomically
    let write_file_atomic = |name: &str, content: &str, target: &PathBuf| -> Result<()> {
        let unique_id = format!(
            "{}.{:?}.{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let temp_path = shim_dir.join(format!(".{}.{}.tmp", name, unique_id));

        let mut file = std::fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create temp file: {}", temp_path.display()))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write content to: {}", temp_path.display()))?;

        file.sync_all()
            .with_context(|| format!("Failed to sync {} to disk", name))?;

        drop(file);

        // Set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o644);
            std::fs::set_permissions(&temp_path, perms).with_context(|| {
                format!("Failed to set permissions on: {}", temp_path.display())
            })?;
        }

        // Atomic rename
        match std::fs::rename(&temp_path, target) {
            Ok(()) => {
                info!(
                    "Materialized {} v{}: {}",
                    name,
                    CRATE_VERSION,
                    target.display()
                );
            }
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path);
                if !target.exists() {
                    return Err(e).with_context(|| {
                        format!("Failed to rename temp file to: {}", target.display())
                    });
                }
                debug!("Another process materialized {}, using existing", name);
            }
        }

        Ok(())
    };

    // Write both files
    if !shim_ok {
        write_file_atomic("bridge_shim.py", BRIDGE_SHIM_SOURCE, &shim_path)?;
    }
    if !types_ok {
        write_file_atomic("casparian_types.py", CASPARIAN_TYPES_SOURCE, &types_path)?;
    }

    Ok(shim_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;

    #[test]
    fn test_materialize_bridge_shim() {
        // This test verifies both shim files can be materialized
        let path = materialize_bridge_shim().unwrap();
        assert!(path.exists(), "Shim should exist after materialization");
        assert!(
            path.to_string_lossy().contains("bridge_shim.py"),
            "Path should contain bridge_shim.py"
        );
        assert!(
            path.to_string_lossy().contains(CRATE_VERSION),
            "Path should contain version: {}",
            CRATE_VERSION
        );

        // Verify bridge_shim.py content matches
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            content, BRIDGE_SHIM_SOURCE,
            "Content should match embedded source"
        );

        // Verify casparian_types.py also exists alongside
        let types_path = path.parent().unwrap().join("casparian_types.py");
        assert!(
            types_path.exists(),
            "casparian_types.py should exist alongside bridge_shim.py"
        );
        let types_content = std::fs::read_to_string(&types_path).unwrap();
        assert_eq!(
            types_content, CASPARIAN_TYPES_SOURCE,
            "casparian_types content should match"
        );
    }

    #[test]
    fn test_materialize_bridge_shim_idempotent() {
        // Calling materialize twice should return the same path
        let path1 = materialize_bridge_shim().unwrap();
        let path2 = materialize_bridge_shim().unwrap();
        assert_eq!(path1, path2, "Materialization should be idempotent");
    }

    #[test]
    fn test_embedded_shim_not_empty() {
        assert!(
            !BRIDGE_SHIM_SOURCE.is_empty(),
            "Embedded shim source should not be empty"
        );
        assert!(
            BRIDGE_SHIM_SOURCE.contains("BridgeContext"),
            "Shim should contain BridgeContext class"
        );
        assert!(
            BRIDGE_SHIM_SOURCE.contains("def main()"),
            "Shim should contain main function"
        );
        // casparian_types.py checks
        assert!(
            !CASPARIAN_TYPES_SOURCE.is_empty(),
            "Embedded casparian_types source should not be empty"
        );
        assert!(
            CASPARIAN_TYPES_SOURCE.contains("class Output"),
            "casparian_types should contain Output class"
        );
    }

    #[test]
    fn test_protocol_constants() {
        assert_eq!(ERROR_SIGNAL, 0xFFFFFFFF);
        assert_eq!(LOG_SIGNAL, 0xFFFFFFFE);
        assert_eq!(OUTPUT_START_SIGNAL, 0xFFFFFFFD);
        assert_eq!(OUTPUT_END_SIGNAL, 0xFFFFFFFC);
        assert_eq!(END_OF_STREAM, 0);
        // LOG_SIGNAL must be distinct from ERROR_SIGNAL and valid data lengths
        assert_ne!(LOG_SIGNAL, ERROR_SIGNAL);
    }

    #[test]
    fn test_log_levels() {
        assert_eq!(log_level::STDOUT, 0);
        assert_eq!(log_level::STDERR, 1);
        assert_eq!(log_level::DEBUG, 2);
        assert_eq!(log_level::INFO, 3);
        assert_eq!(log_level::WARNING, 4);
        assert_eq!(log_level::ERROR, 5);
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(30));
        assert_eq!(READ_TIMEOUT, Duration::from_secs(60));
        assert_eq!(MAX_ERROR_MESSAGE_SIZE, 1024 * 1024);
    }

    #[test]
    fn test_header_size_constant() {
        // Header is 4 bytes for length prefix (u32 big-endian)
        assert_eq!(HEADER_SIZE, 4);
    }

    #[test]
    fn test_schema_mismatch_within_output() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();

            stream
                .write_all(&OUTPUT_START_SIGNAL.to_be_bytes())
                .unwrap();
            stream.write_all(&1u32.to_be_bytes()).unwrap();

            let batch1 = make_int_batch();
            write_ipc_batch(&mut stream, &batch1);

            let batch2 = make_string_batch();
            write_ipc_batch(&mut stream, &batch2);

            stream.write_all(&OUTPUT_END_SIGNAL.to_be_bytes()).unwrap();
            stream.write_all(&1u32.to_be_bytes()).unwrap();
        });

        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let mut log_writer = JobLogWriter::new(JobId::new(1)).unwrap();
        let cancel_token = CancellationToken::new();
        let result = read_arrow_batches(&mut stream, JobId::new(1), &mut log_writer, &cancel_token);

        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("Schema mismatch"),
            "Expected schema mismatch error, got: {}",
            err
        );

        writer.join().unwrap();
    }

    fn write_ipc_batch(stream: &mut TcpStream, batch: &RecordBatch) {
        let mut sink = Vec::new();
        let mut writer = StreamWriter::try_new(&mut sink, &batch.schema()).unwrap();
        writer.write(batch).unwrap();
        writer.finish().unwrap();

        stream
            .write_all(&(sink.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&sink).unwrap();
    }

    fn make_int_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let array = Int64Array::from(vec![1, 2, 3]);
        RecordBatch::try_new(schema, vec![Arc::new(array)]).unwrap()
    }

    fn make_string_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, false)]));
        let array = StringArray::from(vec!["a", "b", "c"]);
        RecordBatch::try_new(schema, vec![Arc::new(array)]).unwrap()
    }

    #[test]
    fn test_tcp_port_allocation() {
        // Verify TCP port allocation works
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("Should bind to ephemeral port");
        let port = listener.local_addr().expect("Should get local addr").port();
        assert!(port > 0, "Should allocate a valid port");
    }

    #[test]
    fn test_crate_version_defined() {
        assert!(!CRATE_VERSION.is_empty(), "CRATE_VERSION should be defined");
        // Should match semantic versioning pattern
        assert!(
            CRATE_VERSION.contains('.'),
            "Version should contain dots: {}",
            CRATE_VERSION
        );
    }
}

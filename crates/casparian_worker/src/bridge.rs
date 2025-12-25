//! Bridge Executor: Spawns Python subprocess and streams Arrow IPC data
//!
//! Implements IPC via Unix socket for privilege separation.
//! All I/O is synchronous and runs in a blocking thread pool.
//!
//! ## Single Binary Distribution
//! The bridge shim Python code is embedded in the binary at compile time.
//! At runtime, it's materialized to `~/.casparian_flow/shim/{version}/bridge_shim.py`.
//!
//! ## Timeouts
//! - Connection timeout: 30 seconds for Python to connect to the socket
//! - Read timeout: 60 seconds per read operation
//! - These ensure jobs don't hang indefinitely if Python crashes or hangs

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use arrow::ipc::reader::StreamReader;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Embedded Python bridge shim source code.
/// This is baked into the binary at compile time for single-file distribution.
const BRIDGE_SHIM_SOURCE: &str = include_str!("../shim/bridge_shim.py");

/// Crate version for shim cache path versioning.
/// When the shim changes, the version bump ensures old cached shims are replaced.
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

const HEADER_SIZE: usize = 4;
const END_OF_STREAM: u32 = 0;
const ERROR_SIGNAL: u32 = 0xFFFF_FFFF;
const LOG_SIGNAL: u32 = 0xFFFF_FFFE;  // Sideband logging signal

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

/// Maximum log file size (10 MB) - prevents disk exhaustion
const MAX_LOG_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Timeout for Python guest to connect to Unix socket
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for read operations on the socket
const READ_TIMEOUT: Duration = Duration::from_secs(60);

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
    fn new(job_id: u64) -> Result<Self> {
        let log_dir = PathBuf::from("/tmp/casparian_logs");
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

        let path = log_dir.join(format!("{}.log", job_id));
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
        self.writer.flush()
            .with_context(|| format!("Failed to flush log file: {}", self.path.display()))?;
        debug!("Log file closed: {} ({} bytes)", self.path.display(), self.bytes_written);
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
    pub job_id: u64,
    pub file_version_id: i64,
    pub shim_path: PathBuf,
}

/// Result of bridge execution including data and logs
#[derive(Debug)]
pub struct BridgeResult {
    /// Arrow record batches produced by the plugin
    pub batches: Vec<RecordBatch>,
    /// Captured logs from the plugin (stdout, stderr, logging)
    /// This is O(1) memory during execution but loaded at end (capped at 10MB)
    pub logs: String,
}

/// Execute a bridge job. This is the only public entry point.
/// Runs blocking I/O in a separate thread pool.
/// Returns both the data batches and captured logs.
pub async fn execute_bridge(config: BridgeConfig) -> Result<BridgeResult> {
    // Move all blocking work to spawn_blocking
    tokio::task::spawn_blocking(move || execute_bridge_sync(config))
        .await
        .context("Bridge task panicked")?
}

/// Synchronous bridge execution - no async lies here
fn execute_bridge_sync(config: BridgeConfig) -> Result<BridgeResult> {
    let job_id = config.job_id;
    let socket_path = format!("/tmp/bridge_{}.sock", job_id);

    // Cleanup stale socket (TOCTOU is acceptable here - worst case we fail to bind)
    let _ = std::fs::remove_file(&socket_path);

    // Create log writer for sideband logging (streams to disk, not RAM)
    let mut log_writer = JobLogWriter::new(job_id)
        .with_context(|| format!("[Job {}] Failed to create log writer", job_id))?;

    // Create socket and spawn process
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to create Unix socket at {}", socket_path))?;

    debug!("[Job {}] Bridge socket created: {}", job_id, socket_path);

    let mut process = spawn_guest(&config, &socket_path)?;
    let process_pid = process.id();

    // Accept connection WITH TIMEOUT
    let mut stream = match accept_with_timeout(&listener, CONNECT_TIMEOUT, &mut process, job_id) {
        Ok(stream) => stream,
        Err(e) => {
            // Collect stderr for debugging before returning error
            let stderr_output = collect_stderr(&mut process);
            cleanup_process(&mut process, &socket_path);

            if !stderr_output.is_empty() {
                error!("[Job {}] Guest stderr before connection failure:\n{}", job_id, stderr_output);
                log_writer.write_log(log_level::STDERR, &stderr_output);
            }
            // Still return the logs even on failure
            let logs = log_writer.read_and_cleanup().unwrap_or_default();
            return Err(e.context(format!("Logs:\n{}", logs)));
        }
    };

    debug!("[Job {}] Guest process (pid={}) connected", job_id, process_pid);

    // Set read timeout on the stream (may already be set in accept_with_timeout)
    // On macOS, this can fail with EINVAL if the peer has already closed
    // We try it anyway but don't fail if it doesn't work - the read will still work
    if let Err(e) = stream.set_read_timeout(Some(READ_TIMEOUT)) {
        warn!("[Job {}] Could not set read timeout (may already be set or peer closed): {}", job_id, e);
    }

    // Read all batches (log_writer receives sideband log messages)
    let batches = match read_arrow_batches(&mut stream, job_id, &mut log_writer) {
        Ok(batches) => batches,
        Err(e) => {
            let stderr_output = collect_stderr(&mut process);
            cleanup_process(&mut process, &socket_path);

            if !stderr_output.is_empty() {
                error!("[Job {}] Guest stderr during read failure:\n{}", job_id, stderr_output);
                log_writer.write_log(log_level::STDERR, &stderr_output);
            }
            let logs = log_writer.read_and_cleanup().unwrap_or_default();
            return Err(e.context(format!("Logs:\n{}", logs)));
        }
    };

    // Wait for process to exit
    let status = process.wait()
        .with_context(|| format!("[Job {}] Failed to wait for guest process", job_id))?;

    // Always collect stderr for logging (even on success)
    let stderr_output = collect_stderr(&mut process);

    if !status.success() {
        // Cleanup socket
        let _ = std::fs::remove_file(&socket_path);

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
            if stderr_output.is_empty() { "(no stderr output)" } else { &stderr_output },
            logs
        );
    }

    // Log warnings from stderr even on success (append to sideband logs)
    if !stderr_output.is_empty() {
        warn!("[Job {}] Guest stderr (process succeeded but had output):\n{}", job_id, stderr_output);
        log_writer.write_log(log_level::STDERR, &stderr_output);
    }

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);

    // Read and cleanup log file
    let logs = log_writer.read_and_cleanup()
        .with_context(|| format!("[Job {}] Failed to read logs", job_id))?;

    info!("[Job {}] Bridge execution complete: {} batches, {} bytes logs",
          job_id, batches.len(), logs.len());

    Ok(BridgeResult { batches, logs })
}

/// Accept a connection with timeout, checking if process is still alive
fn accept_with_timeout(
    listener: &UnixListener,
    timeout: Duration,
    process: &mut Child,
    job_id: u64,
) -> Result<std::os::unix::net::UnixStream> {
    // Use non-blocking mode with polling
    listener.set_nonblocking(true)
        .with_context(|| format!("[Job {}] Failed to set socket to non-blocking", job_id))?;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        // Check if we've exceeded the timeout
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            anyhow::bail!(
                "[Job {}] TIMEOUT: Guest process did not connect to socket within {:.1}s. \
                The Python subprocess may have crashed during startup, failed to import dependencies, \
                or the bridge_shim.py may not be connecting to BRIDGE_SOCKET. \
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
                if let Err(e) = stream.set_read_timeout(Some(READ_TIMEOUT)) {
                    debug!("[Job {}] Could not set read timeout in accept: {}", job_id, e);
                    // Continue anyway - we'll set it later if needed
                }

                // Switch back to blocking mode for the stream
                stream.set_nonblocking(false)
                    .with_context(|| format!("[Job {}] Failed to set stream to blocking mode", job_id))?;

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
                anyhow::bail!(
                    "[Job {}] Failed to accept connection on socket: {}",
                    job_id,
                    e
                );
            }
        }

        // Only check if process exited when there's no pending connection
        match process.try_wait() {
            Ok(Some(status)) => {
                // Process exited - try once more to accept in case connection was queued
                match listener.accept() {
                    Ok((stream, _)) => {
                        // Try to set timeout, ignore if it fails on dead connection
                        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
                        stream.set_nonblocking(false)
                            .with_context(|| format!("[Job {}] Failed to set stream to blocking mode", job_id))?;
                        debug!(
                            "[Job {}] Guest connected after {:.2}s (process already exited)",
                            job_id,
                            elapsed.as_secs_f64()
                        );
                        return Ok(stream);
                    }
                    Err(_) => {
                        anyhow::bail!(
                            "[Job {}] Guest process exited with {} before connecting to socket. \
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

/// Kill process and cleanup socket
fn cleanup_process(process: &mut Child, socket_path: &str) {
    let _ = process.kill();
    let _ = process.wait();
    let _ = std::fs::remove_file(socket_path);
}

/// Spawn the guest Python process using `uv run`
///
/// Delegates to uv for correct Python environment setup on all platforms.
/// uv reconstructs the macOS-specific env vars (like __PYVENV_LAUNCHER__)
/// that Python's multiprocessing module needs to bootstrap correctly.
fn spawn_guest(config: &BridgeConfig, socket_path: &str) -> Result<Child> {
    use base64::{Engine as _, engine::general_purpose};
    let source_b64 = general_purpose::STANDARD.encode(&config.source_code);

    // Use uv to spawn Python - it knows how to set up the environment correctly
    let mut cmd = Command::new("uv");

    // NOTE: Do NOT use env_clear() - macOS Python needs inherited env vars
    // like __PYVENV_LAUNCHER__, LC_CTYPE, etc. for multiprocessing to work.
    // uv will handle passing these through correctly.

    // uv run --frozen --python <interpreter> <script>
    cmd.arg("run")
        .arg("--frozen")
        .arg("--python")
        .arg(&config.interpreter_path)
        .arg(&config.shim_path);

    // Bridge context vars
    cmd.env("BRIDGE_SOCKET", socket_path)
        .env("BRIDGE_PLUGIN_CODE", source_b64)
        .env("BRIDGE_FILE_PATH", &config.file_path)
        .env("BRIDGE_JOB_ID", config.job_id.to_string())
        .env("BRIDGE_FILE_VERSION_ID", config.file_version_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // NOTE: uv sets VIRTUAL_ENV automatically, no need to do it ourselves

    let child = cmd.spawn()
        .with_context(|| format!(
            "[Job {}] Failed to spawn guest process via 'uv run'. \
            Is uv installed? Interpreter: {}, Shim: {}",
            config.job_id,
            config.interpreter_path.display(),
            config.shim_path.display()
        ))?;

    info!(
        "[Job {}] Spawned guest via uv (pid={}) using interpreter {}",
        config.job_id,
        child.id(),
        config.interpreter_path.display()
    );

    Ok(child)
}

/// Read Arrow IPC batches from socket stream, handling sideband log messages
fn read_arrow_batches(
    stream: &mut std::os::unix::net::UnixStream,
    job_id: u64,
    log_writer: &mut JobLogWriter,
) -> Result<Vec<RecordBatch>> {
    let mut batches = Vec::new();
    let mut batch_count = 0u32;
    let mut log_count = 0u32;

    loop {
        // Read 4-byte header
        let mut header_buf = [0u8; HEADER_SIZE];
        match stream.read_exact(&mut header_buf) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Connection closed cleanly
                debug!("[Job {}] Socket closed by guest (EOF)", job_id);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                   || e.kind() == std::io::ErrorKind::TimedOut => {
                anyhow::bail!(
                    "[Job {}] TIMEOUT: No data received from guest within {:.0}s. \
                    The Python plugin may be hanging or performing very slow I/O. \
                    Received {} batches before timeout.",
                    job_id,
                    READ_TIMEOUT.as_secs_f64(),
                    batch_count
                );
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
            debug!("[Job {}] Received end-of-stream signal after {} batches, {} logs",
                   job_id, batch_count, log_count);
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
            continue;  // Don't treat as data batch
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
        stream.read_exact(&mut ipc_buf)
            .with_context(|| format!(
                "[Job {}] Failed to read Arrow IPC payload ({} bytes) after {} batches",
                job_id, length, batch_count
            ))?;

        debug!("[Job {}] Received {} bytes of Arrow IPC data", job_id, length);

        // Parse Arrow IPC stream
        let cursor = std::io::Cursor::new(ipc_buf);
        let reader = StreamReader::try_new(cursor, None)
            .with_context(|| format!(
                "[Job {}] Failed to parse Arrow IPC stream (batch {})",
                job_id, batch_count
            ))?;

        for batch_result in reader {
            let batch = batch_result
                .with_context(|| format!(
                    "[Job {}] Failed to read Arrow batch from IPC stream",
                    job_id
                ))?;
            debug!("[Job {}] Parsed batch {}: {} rows", job_id, batch_count, batch.num_rows());
            batches.push(batch);
        }

        batch_count += 1;
    }

    Ok(batches)
}

/// Read a log message from the socket and write to the log file.
/// Protocol: [LEVEL:1][LENGTH:4][MESSAGE]
fn read_and_write_log(
    stream: &mut std::os::unix::net::UnixStream,
    job_id: u64,
    log_writer: &mut JobLogWriter,
) -> Result<()> {
    // Read 1-byte log level
    let mut level_buf = [0u8; 1];
    stream.read_exact(&mut level_buf)
        .with_context(|| format!("[Job {}] Failed to read log level", job_id))?;
    let level = level_buf[0];

    // Read 4-byte message length
    let mut len_buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut len_buf)
        .with_context(|| format!("[Job {}] Failed to read log message length", job_id))?;
    let msg_len = u32::from_be_bytes(len_buf);

    // Enforce size limit (64KB per message - same as Python side)
    const MAX_LOG_MESSAGE: u32 = 65536;
    if msg_len > MAX_LOG_MESSAGE {
        anyhow::bail!(
            "[Job {}] Log message size {} bytes exceeds maximum {} bytes",
            job_id, msg_len, MAX_LOG_MESSAGE
        );
    }

    // Read message bytes
    let mut msg_buf = vec![0u8; msg_len as usize];
    stream.read_exact(&mut msg_buf)
        .with_context(|| format!("[Job {}] Failed to read log message body", job_id))?;

    // Convert to string (lossy for invalid UTF-8)
    let message = String::from_utf8_lossy(&msg_buf);

    // Write to log file (O(1) memory - streams to disk)
    log_writer.write_log(level, &message);

    Ok(())
}

/// Read error message after ERROR_SIGNAL with size limit
fn read_error_message(
    stream: &mut std::os::unix::net::UnixStream,
    job_id: u64,
) -> Result<String> {
    let mut len_buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut len_buf)
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
    stream.read_exact(&mut error_buf)
        .with_context(|| format!(
            "[Job {}] Failed to read error message body ({} bytes)",
            job_id, error_len
        ))?;

    Ok(String::from_utf8_lossy(&error_buf).to_string())
}

/// Materialize the embedded bridge shim to the filesystem.
///
/// The shim is written to `~/.casparian_flow/shim/{version}/bridge_shim.py`.
/// This ensures the single binary can run from any location without
/// needing the source repository.
///
/// The function is idempotent: if the file exists and matches, it's reused.
/// Version changes cause a new directory to be created.
pub fn materialize_bridge_shim() -> Result<PathBuf> {
    // Resolve cache directory: ~/.casparian_flow/shim/{version}/
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory (HOME or USERPROFILE not set)")?;

    let shim_dir = PathBuf::from(home)
        .join(".casparian_flow")
        .join("shim")
        .join(CRATE_VERSION);

    let shim_path = shim_dir.join("bridge_shim.py");

    // Check if file exists and content matches (fast path)
    if shim_path.exists() {
        match std::fs::read_to_string(&shim_path) {
            Ok(existing) if existing == BRIDGE_SHIM_SOURCE => {
                debug!("Using cached bridge shim: {}", shim_path.display());
                return Ok(shim_path);
            }
            Ok(_) => {
                info!("Bridge shim content changed, updating: {}", shim_path.display());
            }
            Err(e) => {
                warn!("Failed to read existing shim, will recreate: {}", e);
            }
        }
    }

    // Create directory if needed
    std::fs::create_dir_all(&shim_dir)
        .with_context(|| format!("Failed to create shim directory: {}", shim_dir.display()))?;

    // Write shim atomically (write to temp, then rename)
    // Use PID + thread ID + timestamp to avoid collisions in concurrent scenarios
    let unique_id = format!(
        "{}.{:?}.{}",
        std::process::id(),
        std::thread::current().id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let temp_path = shim_dir.join(format!(".bridge_shim.py.{}.tmp", unique_id));

    let mut file = std::fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp shim file: {}", temp_path.display()))?;

    file.write_all(BRIDGE_SHIM_SOURCE.as_bytes())
        .with_context(|| format!("Failed to write shim content to: {}", temp_path.display()))?;

    file.sync_all()
        .with_context(|| "Failed to sync shim file to disk")?;

    drop(file);

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)
            .with_context(|| format!("Failed to set permissions on: {}", temp_path.display()))?;
    }

    // Atomic rename (handles race: if another process already created it, this succeeds)
    // On Unix, rename is atomic and will overwrite the destination
    match std::fs::rename(&temp_path, &shim_path) {
        Ok(()) => {
            info!("Materialized bridge shim v{}: {}", CRATE_VERSION, shim_path.display());
        }
        Err(e) => {
            // Clean up temp file if rename failed (another process might have won the race)
            let _ = std::fs::remove_file(&temp_path);
            // If the target now exists, we're fine (another process created it)
            if !shim_path.exists() {
                return Err(e).with_context(|| {
                    format!("Failed to rename temp shim to: {}", shim_path.display())
                });
            }
            debug!("Another process materialized shim, using existing: {}", shim_path.display());
        }
    }

    Ok(shim_path)
}

/// Deprecated: Use `materialize_bridge_shim()` instead.
///
/// This function is kept for backward compatibility but now delegates
/// to the new materialization logic.
#[deprecated(since = "0.2.0", note = "Use materialize_bridge_shim() instead")]
pub fn find_bridge_shim() -> Result<PathBuf> {
    materialize_bridge_shim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_materialize_bridge_shim() {
        // This test verifies the shim can be materialized
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

        // Verify content matches
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, BRIDGE_SHIM_SOURCE, "Content should match embedded source");

        // Verify permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&path).unwrap().permissions();
            assert!(
                perms.mode() & 0o111 != 0,
                "Shim should be executable"
            );
        }
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
    }

    #[test]
    fn test_protocol_constants() {
        assert_eq!(ERROR_SIGNAL, 0xFFFFFFFF);
        assert_eq!(LOG_SIGNAL, 0xFFFFFFFE);
        assert_eq!(END_OF_STREAM, 0);
        // LOG_SIGNAL must be distinct from ERROR_SIGNAL and valid data lengths
        assert_ne!(LOG_SIGNAL, ERROR_SIGNAL);
        assert!(LOG_SIGNAL > 100 * 1024 * 1024); // Greater than max batch size
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
    fn test_socket_path_format() {
        // Verify socket path format is as expected
        let job_id = 12345u64;
        let socket_path = format!("/tmp/bridge_{}.sock", job_id);
        assert_eq!(socket_path, "/tmp/bridge_12345.sock");
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

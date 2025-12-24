//! Bridge Executor: Spawns Python subprocess and streams Arrow IPC data
//!
//! Implements IPC via Unix socket for privilege separation.
//! All I/O is synchronous and runs in a blocking thread pool.
//!
//! ## Timeouts
//! - Connection timeout: 30 seconds for Python to connect to the socket
//! - Read timeout: 60 seconds per read operation
//! - These ensure jobs don't hang indefinitely if Python crashes or hangs

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use arrow::ipc::reader::StreamReader;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const HEADER_SIZE: usize = 4;
const END_OF_STREAM: u32 = 0;
const ERROR_SIGNAL: u32 = 0xFFFF_FFFF;

/// Maximum size for error messages from guest (1 MB)
/// Prevents OOM from malicious or buggy guest processes
const MAX_ERROR_MESSAGE_SIZE: u32 = 1024 * 1024;

/// Timeout for Python guest to connect to Unix socket
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for read operations on the socket
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// Bridge execution configuration (plain data, no behavior)
pub struct BridgeConfig {
    pub interpreter_path: PathBuf,
    pub source_code: String,
    pub file_path: String,
    pub job_id: u64,
    pub file_version_id: i64,
    pub shim_path: PathBuf,
}

/// Execute a bridge job. This is the only public entry point.
/// Runs blocking I/O in a separate thread pool.
pub async fn execute_bridge(config: BridgeConfig) -> Result<Vec<RecordBatch>> {
    // Move all blocking work to spawn_blocking
    tokio::task::spawn_blocking(move || execute_bridge_sync(config))
        .await
        .context("Bridge task panicked")?
}

/// Synchronous bridge execution - no async lies here
fn execute_bridge_sync(config: BridgeConfig) -> Result<Vec<RecordBatch>> {
    let job_id = config.job_id;
    let socket_path = format!("/tmp/bridge_{}.sock", job_id);

    // Cleanup stale socket (TOCTOU is acceptable here - worst case we fail to bind)
    let _ = std::fs::remove_file(&socket_path);

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
            }
            return Err(e);
        }
    };

    debug!("[Job {}] Guest process (pid={}) connected", job_id, process_pid);

    // Set read timeout on the stream
    stream.set_read_timeout(Some(READ_TIMEOUT))
        .with_context(|| format!("[Job {}] Failed to set read timeout on socket", job_id))?;

    // Read all batches
    let batches = match read_arrow_batches(&mut stream, job_id) {
        Ok(batches) => batches,
        Err(e) => {
            let stderr_output = collect_stderr(&mut process);
            cleanup_process(&mut process, &socket_path);

            if !stderr_output.is_empty() {
                error!("[Job {}] Guest stderr during read failure:\n{}", job_id, stderr_output);
            }
            return Err(e);
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
        }
        anyhow::bail!(
            "[Job {}] Guest process (pid={}) exited with {}: {}",
            job_id,
            process_pid,
            status,
            if stderr_output.is_empty() { "(no stderr output)" } else { &stderr_output }
        );
    }

    // Log warnings from stderr even on success
    if !stderr_output.is_empty() {
        warn!("[Job {}] Guest stderr (process succeeded but had output):\n{}", job_id, stderr_output);
    }

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);

    info!("[Job {}] Bridge execution complete: {} batches", job_id, batches.len());
    Ok(batches)
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

        // Check if process has exited
        match process.try_wait() {
            Ok(Some(status)) => {
                anyhow::bail!(
                    "[Job {}] Guest process exited with {} before connecting to socket. \
                    The Python subprocess crashed during startup. \
                    Check the guest stderr output above for details.",
                    job_id,
                    status
                );
            }
            Ok(None) => {
                // Process still running, continue waiting
            }
            Err(e) => {
                anyhow::bail!(
                    "[Job {}] Failed to check guest process status: {}",
                    job_id,
                    e
                );
            }
        }

        // Try to accept connection
        match listener.accept() {
            Ok((stream, _)) => {
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
                // No connection yet, sleep and retry
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                anyhow::bail!(
                    "[Job {}] Failed to accept connection on socket: {}",
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

/// Read Arrow IPC batches from socket stream
fn read_arrow_batches(
    stream: &mut std::os::unix::net::UnixStream,
    job_id: u64,
) -> Result<Vec<RecordBatch>> {
    let mut batches = Vec::new();
    let mut batch_count = 0u32;

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
            debug!("[Job {}] Received end-of-stream signal after {} batches", job_id, batch_count);
            break;
        }

        // Error signal
        if length == ERROR_SIGNAL {
            let error_msg = read_error_message(stream, job_id)?;
            anyhow::bail!("[Job {}] Guest process error: {}", job_id, error_msg);
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

/// Find bridge_shim.py - call this once at startup, not per-job
pub fn find_bridge_shim() -> Result<PathBuf> {
    let candidates = [
        // New location in Rust crate
        "crates/casparian_worker/shim/bridge_shim.py",
        "../crates/casparian_worker/shim/bridge_shim.py",
        "../../crates/casparian_worker/shim/bridge_shim.py",
        // Relative to crate directory
        "shim/bridge_shim.py",
        "../shim/bridge_shim.py",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path.canonicalize()?);
        }
    }

    anyhow::bail!(
        "bridge_shim.py not found. Searched: {:?}",
        candidates
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_bridge_shim() {
        // This test only passes when run from repo root
        if PathBuf::from("crates/casparian_worker/shim/bridge_shim.py").exists() {
            let path = find_bridge_shim().unwrap();
            assert!(path.exists());
            assert!(path.to_string_lossy().contains("bridge_shim.py"));
        }
    }

    #[test]
    fn test_error_signal_constant() {
        assert_eq!(ERROR_SIGNAL, 0xFFFFFFFF);
        assert_eq!(END_OF_STREAM, 0);
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
}

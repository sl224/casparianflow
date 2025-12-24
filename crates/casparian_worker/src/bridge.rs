//! Bridge Executor: Spawns Python subprocess and streams Arrow IPC data
//!
//! Implements IPC via Unix socket for privilege separation.
//! All I/O is synchronous and runs in a blocking thread pool.

use anyhow::{Context, Result};
use arrow::array::RecordBatch;
use arrow::ipc::reader::StreamReader;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tracing::{debug, info, warn};

const HEADER_SIZE: usize = 4;
const END_OF_STREAM: u32 = 0;
const ERROR_SIGNAL: u32 = 0xFFFF_FFFF;

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
    let socket_path = format!("/tmp/bridge_{}.sock", config.job_id);

    // Cleanup stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Create socket and spawn process
    let listener = UnixListener::bind(&socket_path)
        .context("Failed to create Unix socket")?;

    debug!("Bridge socket created: {}", socket_path);

    let mut process = spawn_guest(&config, &socket_path)?;

    // Accept connection (blocking - that's fine, we're in spawn_blocking)
    listener.set_nonblocking(false)?;
    let (mut stream, _) = listener.accept()
        .context("Failed to accept connection from guest")?;

    debug!("Guest process connected");

    // Read all batches
    let batches = read_arrow_batches(&mut stream)?;

    // Wait for process to exit
    let status = process.wait()?;
    if !status.success() {
        // Collect stderr for debugging
        if let Some(mut stderr) = process.stderr.take() {
            let mut err_output = String::new();
            let _ = stderr.read_to_string(&mut err_output);
            if !err_output.is_empty() {
                warn!("Guest stderr: {}", err_output);
            }
        }
        anyhow::bail!("Guest process exited with status: {}", status);
    }

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);

    info!("Bridge execution complete: {} batches", batches.len());
    Ok(batches)
}

/// Spawn the guest Python process
fn spawn_guest(config: &BridgeConfig, socket_path: &str) -> Result<Child> {
    use base64::{Engine as _, engine::general_purpose};
    let source_b64 = general_purpose::STANDARD.encode(&config.source_code);

    // Resolve venv root from interpreter path (e.g., /path/to/venv/bin/python -> /path/to/venv)
    // Important: resolve the VENV DIRECTORY symlink, not the python binary
    // (python binary might symlink to uv's base install, but we need the venv's site-packages)
    let venv_root = config.interpreter_path
        .parent()  // bin/
        .and_then(|p| p.parent())  // venv/
        .and_then(|p| p.canonicalize().ok());  // resolve venv symlink

    // Build command with clean environment
    // CRITICAL: env_clear() MUST be called before adding any env vars
    let mut cmd = Command::new(&config.interpreter_path);
    cmd.env_clear();  // Clear ALL inherited env vars first

    // Now add only the vars we need
    cmd.arg(&config.shim_path)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .env("BRIDGE_SOCKET", socket_path)
        .env("BRIDGE_PLUGIN_CODE", source_b64)
        .env("BRIDGE_FILE_PATH", &config.file_path)
        .env("BRIDGE_JOB_ID", config.job_id.to_string())
        .env("BRIDGE_FILE_VERSION_ID", config.file_version_id.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set VIRTUAL_ENV so Python finds correct site-packages (required for uv-managed venvs)
    if let Some(venv) = &venv_root {
        cmd.env("VIRTUAL_ENV", venv);
        info!("VIRTUAL_ENV={} (env_clear applied)", venv.display());
    } else {
        warn!("Could not resolve venv root from interpreter path");
    }

    let child = cmd.spawn()
        .context("Failed to spawn guest process")?;

    info!(
        "Spawned guest process (pid={}) with interpreter {}",
        child.id(),
        config.interpreter_path.display()
    );

    Ok(child)
}

/// Read Arrow IPC batches from socket stream
fn read_arrow_batches(stream: &mut std::os::unix::net::UnixStream) -> Result<Vec<RecordBatch>> {
    let mut batches = Vec::new();

    loop {
        // Read 4-byte header
        let mut header_buf = [0u8; HEADER_SIZE];
        if let Err(e) = stream.read_exact(&mut header_buf) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                // Connection closed cleanly
                break;
            }
            return Err(e.into());
        }

        let length = u32::from_be_bytes(header_buf);

        // End of stream signal
        if length == END_OF_STREAM {
            debug!("Received end-of-stream signal");
            break;
        }

        // Error signal
        if length == ERROR_SIGNAL {
            let error_msg = read_error_message(stream)?;
            anyhow::bail!("Guest process error: {}", error_msg);
        }

        // Read Arrow IPC payload
        let mut ipc_buf = vec![0u8; length as usize];
        stream.read_exact(&mut ipc_buf)?;

        debug!("Received {} bytes of Arrow IPC data", length);

        // Parse Arrow IPC stream
        let cursor = std::io::Cursor::new(ipc_buf);
        let mut reader = StreamReader::try_new(cursor, None)?;

        while let Some(batch_result) = reader.next() {
            let batch = batch_result?;
            debug!("Parsed batch: {} rows", batch.num_rows());
            batches.push(batch);
        }
    }

    Ok(batches)
}

/// Read error message after ERROR_SIGNAL
fn read_error_message(stream: &mut std::os::unix::net::UnixStream) -> Result<String> {
    let mut len_buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut len_buf)?;
    let error_len = u32::from_be_bytes(len_buf);

    let mut error_buf = vec![0u8; error_len as usize];
    stream.read_exact(&mut error_buf)?;

    Ok(String::from_utf8_lossy(&error_buf).to_string())
}

/// Find bridge_shim.py - call this once at startup, not per-job
pub fn find_bridge_shim() -> Result<PathBuf> {
    let candidates = [
        "src/casparian_flow/engine/bridge_shim.py",
        "../src/casparian_flow/engine/bridge_shim.py",
        "../../src/casparian_flow/engine/bridge_shim.py",
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
        if PathBuf::from("src/casparian_flow/engine/bridge_shim.py").exists() {
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
}

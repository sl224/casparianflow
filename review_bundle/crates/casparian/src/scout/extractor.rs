//! Extractor Execution Engine
//!
//! Runs Python extractors as isolated subprocesses with timeout handling,
//! crash isolation, and fail-fast batch semantics.
//!
//! ## Architecture
//!
//! ```text
//! ExtractorRunner (Rust)
//!     │
//!     └── spawn Python subprocess
//!         ├── timeout: 5 seconds (configurable)
//!         ├── input: file_path (via stdin)
//!         └── output: JSON metadata (via stdout)
//! ```
//!
//! ## Error States
//!
//! - `Ok`: Extraction succeeded, metadata returned
//! - `Timeout`: Extractor exceeded time limit
//! - `Crash`: Process exited with non-zero code
//! - `Error`: Other errors (invalid JSON, I/O errors)

use crate::scout::db::Database;
use crate::scout::error::Result;
use crate::scout::types::{ExtractionLogStatus, ExtractionStatus, Extractor, ScannedFile};
use anyhow::Context;
use chrono::Utc;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use std::{process::Command, thread};
use tracing::{debug, error, info, warn};

/// Default timeout for extractor execution (5 seconds)
const DEFAULT_TIMEOUT_SECS: u32 = 5;

/// Maximum number of consecutive failures before pausing an extractor
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Result of running an extractor on a single file
#[derive(Debug, Clone)]
pub enum ExtractorResult {
    /// Extraction succeeded
    Ok {
        /// Extracted metadata as JSON string
        metadata: String,
        /// Duration in milliseconds
        duration_ms: u64,
    },
    /// Extraction timed out
    Timeout {
        /// Timeout duration
        timeout: Duration,
        /// Duration before timeout (approximately equal to timeout)
        duration_ms: u64,
    },
    /// Extractor process crashed
    Crash {
        /// Exit code if available
        exit_code: Option<i32>,
        /// Stderr output
        stderr: String,
        /// Duration in milliseconds
        duration_ms: u64,
    },
    /// Other error (invalid JSON, I/O error, etc.)
    Error {
        /// Error message
        message: String,
        /// Duration in milliseconds
        duration_ms: u64,
    },
}

impl ExtractorResult {
    /// Convert to extraction status for database storage
    pub fn to_status(&self) -> ExtractionStatus {
        match self {
            ExtractorResult::Ok { .. } => ExtractionStatus::Extracted,
            ExtractorResult::Timeout { .. } => ExtractionStatus::Timeout,
            ExtractorResult::Crash { .. } => ExtractionStatus::Crash,
            ExtractorResult::Error { .. } => ExtractionStatus::Error,
        }
    }

    /// Convert to log status for extraction log table
    pub fn to_log_status(&self) -> ExtractionLogStatus {
        match self {
            ExtractorResult::Ok { .. } => ExtractionLogStatus::Success,
            ExtractorResult::Timeout { .. } => ExtractionLogStatus::Timeout,
            ExtractorResult::Crash { .. } => ExtractionLogStatus::Crash,
            ExtractorResult::Error { .. } => ExtractionLogStatus::Error,
        }
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        match self {
            ExtractorResult::Ok { duration_ms, .. } => *duration_ms,
            ExtractorResult::Timeout { duration_ms, .. } => *duration_ms,
            ExtractorResult::Crash { duration_ms, .. } => *duration_ms,
            ExtractorResult::Error { duration_ms, .. } => *duration_ms,
        }
    }

    /// Get error message if any
    pub fn error_message(&self) -> Option<String> {
        match self {
            ExtractorResult::Ok { .. } => None,
            ExtractorResult::Timeout { timeout, .. } => {
                Some(format!("Extraction timed out after {:?}", timeout))
            }
            ExtractorResult::Crash {
                stderr, exit_code, ..
            } => {
                let code_str = exit_code
                    .map(|c| format!(" (exit code: {})", c))
                    .unwrap_or_default();
                if stderr.is_empty() {
                    Some(format!("Extractor crashed{}", code_str))
                } else {
                    Some(format!("Extractor crashed{}: {}", code_str, stderr))
                }
            }
            ExtractorResult::Error { message, .. } => Some(message.clone()),
        }
    }

    /// Get metadata if extraction succeeded
    pub fn metadata(&self) -> Option<&str> {
        match self {
            ExtractorResult::Ok { metadata, .. } => Some(metadata),
            _ => None,
        }
    }

    /// Check if extraction was successful
    pub fn is_ok(&self) -> bool {
        matches!(self, ExtractorResult::Ok { .. })
    }
}

/// Extractor runner configuration
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    /// Timeout per extraction (default: 5 seconds)
    pub timeout: Duration,
    /// Path to Python interpreter (default: "python3")
    pub python_path: PathBuf,
    /// Maximum memory in MB (for future resource limiting)
    pub max_memory_mb: usize,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS as u64),
            python_path: PathBuf::from("python3"),
            max_memory_mb: 256,
        }
    }
}

/// Runs Python extractors in isolated subprocesses
pub struct ExtractorRunner {
    config: ExtractorConfig,
}

impl ExtractorRunner {
    /// Create a new extractor runner with default config
    pub fn new() -> Self {
        Self {
            config: ExtractorConfig::default(),
        }
    }

    /// Create a new extractor runner with custom config
    pub fn with_config(config: ExtractorConfig) -> Self {
        Self { config }
    }

    /// Run an extractor on a single file
    ///
    /// The extractor Python script should:
    /// 1. Read the file path from stdin
    /// 2. Extract metadata from the path
    /// 3. Print JSON metadata to stdout
    /// 4. Exit with code 0 on success, non-zero on failure
    pub fn run_extractor(&self, extractor: &Extractor, file_path: &str) -> ExtractorResult {
        let start = Instant::now();
        let timeout = Duration::from_secs(extractor.timeout_secs as u64);

        debug!(
            "Running extractor '{}' on file: {}",
            extractor.name, file_path
        );

        let duration_ms = start.elapsed().as_millis() as u64;

        match self.spawn_extractor(extractor, file_path, timeout) {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    return ExtractorResult::Crash {
                        exit_code: output.status.code(),
                        stderr,
                        duration_ms,
                    };
                }

                let stdout = match String::from_utf8(output.stdout) {
                    Ok(text) => text,
                    Err(e) => {
                        return ExtractorResult::Error {
                            message: format!("Extractor output is not valid UTF-8: {}", e),
                            duration_ms,
                        };
                    }
                };

                let trimmed = stdout.trim();
                if trimmed.is_empty() {
                    return ExtractorResult::Error {
                        message: "Extractor returned empty output".to_string(),
                        duration_ms,
                    };
                }

                if let Err(err) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    return ExtractorResult::Error {
                        message: format!("Extractor output is not valid JSON: {}", err),
                        duration_ms,
                    };
                }

                debug!(
                    "Extractor '{}' succeeded in {}ms",
                    extractor.name, duration_ms
                );
                ExtractorResult::Ok {
                    metadata: trimmed.to_string(),
                    duration_ms,
                }
            }
            Err(ExtractorSpawnError::Timeout(timeout)) => {
                warn!(
                    "Extractor '{}' timed out after {:?}",
                    extractor.name, timeout
                );
                ExtractorResult::Timeout {
                    timeout,
                    duration_ms,
                }
            }
            Err(ExtractorSpawnError::Io(err)) => ExtractorResult::Error {
                message: err.to_string(),
                duration_ms,
            },
        }
    }

    /// Spawn the extractor subprocess and wait for result with a timeout.
    fn spawn_extractor(
        &self,
        extractor: &Extractor,
        file_path: &str,
        timeout: Duration,
    ) -> SpawnResult<ExtractorProcessOutput> {
        let mut child = Command::new(&self.config.python_path)
            .arg(&extractor.source_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to spawn extractor '{}' at {}",
                    extractor.name, extractor.source_path
                )
            })
            .map_err(ExtractorSpawnError::Io)?;

        // Write file path to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(file_path.as_bytes())
                .with_context(|| "Failed to write file path to extractor stdin")
                .map_err(ExtractorSpawnError::Io)?;
            stdin.flush().map_err(ExtractorSpawnError::io)?;
        }

        wait_for_child(extractor, child, timeout)
    }
}

#[derive(Debug)]
struct ExtractorProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum ExtractorSpawnError {
    Timeout(Duration),
    Io(anyhow::Error),
}

impl ExtractorSpawnError {
    fn io<T: Into<anyhow::Error>>(err: T) -> Self {
        ExtractorSpawnError::Io(err.into())
    }
}

fn wait_for_child(
    _extractor: &Extractor,
    mut child: Child,
    timeout: Duration,
) -> SpawnResult<ExtractorProcessOutput> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExtractorSpawnError::io(anyhow::anyhow!("Missing extractor stdout pipe")))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExtractorSpawnError::io(anyhow::anyhow!("Missing extractor stderr pipe")))?;

    let start = Instant::now();
    loop {
        match child.try_wait().map_err(ExtractorSpawnError::io)? {
            Some(status) => {
                let mut stdout_buf = Vec::new();
                stdout
                    .read_to_end(&mut stdout_buf)
                    .map_err(ExtractorSpawnError::io)?;
                let mut stderr_buf = Vec::new();
                stderr
                    .read_to_end(&mut stderr_buf)
                    .map_err(ExtractorSpawnError::io)?;
                return Ok(ExtractorProcessOutput {
                    status,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                });
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ExtractorSpawnError::Timeout(timeout));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

type SpawnResult<T> = std::result::Result<T, ExtractorSpawnError>;

impl Default for ExtractorRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch extractor with fail-fast semantics
///
/// Runs extractors against multiple files with:
/// - Consecutive failure detection
/// - Automatic extractor pause after N failures
/// - Progress reporting
pub struct BatchExtractor {
    runner: ExtractorRunner,
    db: Database,
}

impl BatchExtractor {
    /// Create a new batch extractor
    pub fn new(db: Database) -> Self {
        Self {
            runner: ExtractorRunner::new(),
            db,
        }
    }

    /// Create a new batch extractor with custom runner
    pub fn with_runner(db: Database, runner: ExtractorRunner) -> Self {
        Self { runner, db }
    }

    /// Run an extractor on a batch of files
    ///
    /// Implements fail-fast semantics:
    /// - Stops after MAX_CONSECUTIVE_FAILURES failures
    /// - Pauses the extractor in the database
    /// - Returns early with partial results
    ///
    /// Returns: (successes, failures, was_paused)
    pub fn run_batch(
        &self,
        extractor: &mut Extractor,
        files: &[ScannedFile],
    ) -> Result<(usize, usize, bool)> {
        if !extractor.enabled || extractor.is_paused() {
            info!("Skipping disabled/paused extractor: {}", extractor.name);
            return Ok((0, 0, false));
        }

        let mut successes = 0;
        let mut failures = 0;
        let mut consecutive_failures = extractor.consecutive_failures;

        info!(
            "Running extractor '{}' on {} files",
            extractor.name,
            files.len()
        );

        for file in files {
            // Check if we should pause due to too many consecutive failures
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                warn!(
                    "Pausing extractor '{}' after {} consecutive failures",
                    extractor.name, consecutive_failures
                );

                // Update extractor state
                extractor.consecutive_failures = consecutive_failures;
                extractor.paused_at = Some(Utc::now());

                // Persist pause state to database
                self.pause_extractor(&extractor.id)?;

                return Ok((successes, failures, true));
            }

            let file_path = &file.path;
            let result = self.runner.run_extractor(extractor, file_path);

            // Log the extraction attempt
            if let Some(file_id) = file.id {
                self.log_extraction(file_id, &extractor.id, &result)?;
            }

            if result.is_ok() {
                // Update file metadata in database
                if let Some(file_id) = file.id {
                    if let Some(metadata) = result.metadata() {
                        self.update_file_metadata(file_id, metadata, ExtractionStatus::Extracted)?;
                    }
                }

                successes += 1;
                consecutive_failures = 0; // Reset on success
            } else {
                failures += 1;
                consecutive_failures += 1;

                // Update file status to reflect extraction failure
                if let Some(file_id) = file.id {
                    self.update_file_metadata(file_id, "{}", result.to_status())?;
                }

                error!(
                    "Extractor '{}' failed on file '{}': {:?}",
                    extractor.name, file_path, result
                );
            }
        }

        // Update consecutive failures in database (but don't pause)
        extractor.consecutive_failures = consecutive_failures;
        self.update_extractor_failures(&extractor.id, consecutive_failures)?;

        info!(
            "Batch complete for '{}': {} successes, {} failures",
            extractor.name, successes, failures
        );

        Ok((successes, failures, false))
    }

    /// Run all enabled extractors on files needing extraction
    ///
    /// Returns: Vec<(extractor_id, successes, failures, was_paused)>
    pub fn run_all_extractors(&self) -> Result<Vec<(String, usize, usize, bool)>> {
        // Get all enabled, non-paused extractors
        let extractors = self.db.get_enabled_extractors()?;
        let mut results = Vec::new();

        for mut extractor in extractors {
            // Get files pending extraction for this extractor
            let files = self.db.get_files_pending_extraction()?;

            if files.is_empty() {
                debug!("No files pending extraction for '{}'", extractor.name);
                continue;
            }

            let (successes, failures, was_paused) = self.run_batch(&mut extractor, &files)?;

            results.push((extractor.id.clone(), successes, failures, was_paused));
        }

        Ok(results)
    }

    /// Pause an extractor in the database
    fn pause_extractor(&self, extractor_id: &str) -> Result<()> {
        self.db.pause_extractor(extractor_id)
    }

    /// Update extractor consecutive failure count
    fn update_extractor_failures(&self, extractor_id: &str, failures: u32) -> Result<()> {
        self.db
            .update_extractor_consecutive_failures(extractor_id, failures)
    }

    /// Log an extraction attempt
    fn log_extraction(
        &self,
        file_id: i64,
        extractor_id: &str,
        result: &ExtractorResult,
    ) -> Result<()> {
        self.db.log_extraction(
            file_id,
            extractor_id,
            result.to_log_status(),
            Some(result.duration_ms()),
            result.error_message().as_deref(),
            result.metadata(),
        )
    }

    /// Update file metadata after extraction
    fn update_file_metadata(
        &self,
        file_id: i64,
        metadata_raw: &str,
        status: ExtractionStatus,
    ) -> Result<()> {
        self.db
            .update_file_extraction(file_id, metadata_raw, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_result_to_status() {
        let ok = ExtractorResult::Ok {
            metadata: "{}".to_string(),
            duration_ms: 100,
        };
        assert_eq!(ok.to_status(), ExtractionStatus::Extracted);

        let timeout = ExtractorResult::Timeout {
            timeout: Duration::from_secs(5),
            duration_ms: 5000,
        };
        assert_eq!(timeout.to_status(), ExtractionStatus::Timeout);

        let crash = ExtractorResult::Crash {
            exit_code: Some(1),
            stderr: "error".to_string(),
            duration_ms: 100,
        };
        assert_eq!(crash.to_status(), ExtractionStatus::Crash);

        let error = ExtractorResult::Error {
            message: "invalid json".to_string(),
            duration_ms: 100,
        };
        assert_eq!(error.to_status(), ExtractionStatus::Error);
    }

    #[test]
    fn test_extractor_result_error_message() {
        let ok = ExtractorResult::Ok {
            metadata: "{}".to_string(),
            duration_ms: 100,
        };
        assert!(ok.error_message().is_none());

        let timeout = ExtractorResult::Timeout {
            timeout: Duration::from_secs(5),
            duration_ms: 5000,
        };
        assert!(timeout.error_message().unwrap().contains("timed out"));

        let crash = ExtractorResult::Crash {
            exit_code: Some(1),
            stderr: "segfault".to_string(),
            duration_ms: 100,
        };
        let msg = crash.error_message().unwrap();
        assert!(msg.contains("crashed"));
        assert!(msg.contains("segfault"));
    }

    #[test]
    fn test_default_config() {
        let config = ExtractorConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.max_memory_mb, 256);
    }
}

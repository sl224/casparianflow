//! Development mode runner - no database, terminal output.
//!
//! Used for local development and testing. Outputs logs to terminal
//! and does not write to any database. Uses job_id 0 for dev mode.

use super::{ExecutionResult, LogDestination, ParserRef, Runner};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

/// Development mode runner that executes parsers without database integration.
///
/// Features:
/// - Logs go to terminal (visible during `pdb` debugging)
/// - No database writes
/// - Uses job_id 0
/// - Resolves Python interpreter from $VIRTUAL_ENV or falls back to system python3
pub struct DevRunner {
    /// Python interpreter path (from $VIRTUAL_ENV/bin/python or system)
    python_path: std::path::PathBuf,
}

impl DevRunner {
    /// Create a new DevRunner.
    ///
    /// Resolves the Python interpreter from $VIRTUAL_ENV if set,
    /// otherwise falls back to system python3.
    pub fn new() -> Result<Self> {
        let python_path = resolve_python()?;
        Ok(Self { python_path })
    }
}

#[async_trait]
impl Runner for DevRunner {
    async fn execute(
        &self,
        parser: ParserRef,
        input: &Path,
        _log_dest: LogDestination, // Dev mode always uses Terminal
    ) -> Result<ExecutionResult> {
        let parser_path = match parser {
            ParserRef::Path(p) => p,
            ParserRef::Bundle { temp_dir, .. } => temp_dir.join("parser.py"),
        };

        // Read parser source
        let source = std::fs::read_to_string(&parser_path)
            .with_context(|| format!("Failed to read parser: {}", parser_path.display()))?;

        // Materialize the bridge shim
        let shim_path = casparian_worker::bridge::materialize_bridge_shim()
            .context("Failed to materialize bridge shim")?;

        // Create bridge config
        let config = casparian_worker::bridge::BridgeConfig {
            interpreter_path: self.python_path.clone(),
            source_code: source,
            file_path: input.to_string_lossy().to_string(),
            job_id: 0, // Dev mode uses job_id 0
            file_version_id: 0,
            shim_path,
        };

        // Execute with terminal output (logs captured by bridge)
        let result = casparian_worker::bridge::execute_bridge(config).await?;

        Ok(ExecutionResult {
            batches: result.batches,
            logs: result.logs,
            output_info: result.output_info,
        })
    }
}

/// Resolve Python interpreter path.
///
/// Priority:
/// 1. $VIRTUAL_ENV/bin/python (if VIRTUAL_ENV is set)
/// 2. System python3
fn resolve_python() -> Result<std::path::PathBuf> {
    // Check $VIRTUAL_ENV first
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let venv_python = std::path::PathBuf::from(&venv).join("bin/python");
        if venv_python.exists() {
            return Ok(venv_python);
        }
    }

    // Fall back to system python3
    Ok(std::path::PathBuf::from("python3"))
}

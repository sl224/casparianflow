//! Runner abstractions for parser execution.
//!
//! Two modes:
//! - DevRunner: Development mode with terminal output, no DB writes
//! - QueuedRunner: Production mode with file logging, DB integration

use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Where to send parser logs
pub enum LogDestination {
    /// Dev mode: logs go to terminal (Stdio::inherit)
    Terminal,
    /// Prod mode: logs go to file
    File(PathBuf),
}

/// Result of parser execution
pub struct ExecutionResult {
    /// Arrow record batches produced by the parser
    pub batches: Vec<arrow::array::RecordBatch>,
    /// Captured logs from the parser (stdout, stderr, logging)
    pub logs: String,
    /// Output metadata from the parser (sink routing info)
    pub output_info: Vec<casparian_worker::bridge::OutputInfo>,
}

/// Reference to a parser - either path (dev) or bundle (prod)
pub enum ParserRef {
    /// Development: path to parser.py
    Path(PathBuf),
    /// Production: extracted bundle in temp dir
    Bundle {
        name: String,
        version: String,
        temp_dir: PathBuf,
    },
}

/// Runner trait for parser execution
#[async_trait]
pub trait Runner: Send + Sync {
    /// Execute a parser against an input file
    async fn execute(
        &self,
        parser: ParserRef,
        input: &Path,
        log_dest: LogDestination,
    ) -> Result<ExecutionResult>;
}

mod dev;
mod queued;

pub use dev::DevRunner;
pub use queued::QueuedRunner;

//! Runner abstractions for parser execution.
//!
//! Two modes:
//! - DevRunner: Development mode with terminal output, no DB writes
//! - QueuedRunner: Production mode with file logging, DB integration

use casparian_sinks::OutputBatch;
use std::path::PathBuf;

/// Where to send parser logs
pub enum LogDestination {
    /// Dev mode: logs go to terminal (Stdio::inherit)
    Terminal,
}

/// Result of parser execution
pub struct ExecutionResult {
    /// Arrow record batches grouped by output (per publish call)
    pub output_batches: Vec<Vec<OutputBatch>>,
    /// Captured logs from the parser (stdout, stderr, logging)
    pub logs: String,
    /// Output metadata from the parser (sink routing info)
    pub output_info: Vec<casparian_worker::bridge::OutputInfo>,
}

/// Reference to a parser by path
pub enum ParserRef {
    /// Path to parser.py
    Path(PathBuf),
}

mod dev;

pub use dev::DevRunner;

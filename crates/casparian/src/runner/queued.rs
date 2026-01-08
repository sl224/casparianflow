//! Production mode runner - with database and file logging.
//!
//! This is a stub - full implementation comes in later phases.
//! The QueuedRunner will integrate with the job queue system.

use super::{ExecutionResult, LogDestination, ParserRef, Runner};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::path::Path;

/// Production mode runner with database integration and file logging.
///
/// This is a stub implementation. Full features will include:
/// - Job queue integration
/// - Database writes for job status
/// - File-based logging instead of terminal
/// - Lineage tracking
/// - Deduplication checks
pub struct QueuedRunner;

impl QueuedRunner {
    /// Create a new QueuedRunner.
    pub fn new() -> Self {
        Self
    }
}

impl Default for QueuedRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Runner for QueuedRunner {
    async fn execute(
        &self,
        _parser: ParserRef,
        _input: &Path,
        _log_dest: LogDestination,
    ) -> Result<ExecutionResult> {
        bail!("QueuedRunner not yet implemented - use DevRunner for now")
    }
}

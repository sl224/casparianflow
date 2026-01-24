//! Job Subsystem
//!
//! Manages job lifecycle for long-running operations (synchronous, no async runtime):
//! - Job creation and tracking
//! - Progress reporting
//! - Cancellation
//! - Persistence across server restarts
//!
//! # Design
//!
//! Long-running operations (backtest, run) return immediately with a job_id.
//! Clients poll for progress via `casparian_job_status`.
//!
//! # Concurrency
//!
//! Default: 1 concurrent job (serialized execution).
//! Jobs are queued and executed in order.

mod executor;
mod manager;
#[cfg(test)]
mod store;

pub use executor::{JobExecutor, JobExecutorHandle};
pub use manager::JobManager;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ============================================================================
// JobSpec - Persisted job execution details
// ============================================================================

/// Job specification - contains all details needed to execute/restart a job.
///
/// Persisted with the job so the executor can run it even after server restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobSpec {
    /// Backtest job specification
    Backtest {
        /// Plugin to run
        plugin_ref: crate::types::PluginRef,
        /// Directory containing input files
        input_dir: String,
        /// Optional per-output schemas for validation
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schemas: Option<crate::types::SchemasMap>,
        /// Redaction policy for sample values
        #[serde(default, skip_serializing_if = "Option::is_none")]
        redaction: Option<crate::types::RedactionPolicy>,
    },
    /// Run job specification
    Run {
        /// Plugin to run
        plugin_ref: crate::types::PluginRef,
        /// Directory containing input files
        input_dir: String,
        /// Output directory (optional)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_dir: Option<String>,
        /// Optional per-output schemas for validation
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schemas: Option<crate::types::SchemasMap>,
    },
}

// ============================================================================
// JobId - Unique job identifier
// ============================================================================

/// Unique job identifier (numeric, DB-backed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);

impl JobId {
    /// Create from an existing numeric ID
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Parse from string (MCP inputs)
    pub fn parse(s: &str) -> Result<Self, std::num::ParseIntError> {
        Ok(Self(u64::from_str(s)?))
    }

    /// Convert to protocol JobId
    pub const fn to_proto(self) -> casparian_protocol::JobId {
        casparian_protocol::JobId::new(self.0)
    }

    /// Convert from protocol JobId
    pub const fn from_proto(id: casparian_protocol::JobId) -> Self {
        Self(id.as_u64())
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Job state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobState {
    /// Job is queued, waiting to run
    Queued { queued_at: DateTime<Utc> },
    /// Job is currently running
    Running {
        started_at: DateTime<Utc>,
        progress: JobProgress,
    },
    /// Job completed successfully
    Completed {
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        result: serde_json::Value,
    },
    /// Job failed with error
    Failed {
        started_at: Option<DateTime<Utc>>,
        failed_at: DateTime<Utc>,
        error: String,
    },
    /// Job was cancelled
    Cancelled { cancelled_at: DateTime<Utc> },
    /// Job appears stalled (no progress for >30s)
    Stalled {
        started_at: DateTime<Utc>,
        last_progress_at: DateTime<Utc>,
        progress: JobProgress,
    },
}

impl JobState {
    /// Get the status string
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Queued { .. } => "queued",
            Self::Running { .. } => "running",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
            Self::Stalled { .. } => "stalled",
        }
    }

    /// Check if the job is terminal (completed, failed, or cancelled)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }

    /// Check if the job is running or stalled
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running { .. } | Self::Stalled { .. })
    }
}

/// Job progress information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobProgress {
    /// Current phase of the job
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// Items processed so far
    pub items_done: u64,

    /// Total items (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_total: Option<u64>,

    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,

    /// Estimated time remaining in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_ms: Option<u64>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Additional phase-specific metrics
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

impl JobProgress {
    /// Create a new progress instance
    pub fn new() -> Self {
        Self {
            updated_at: Utc::now(),
            ..Default::default()
        }
    }

    /// Update items processed
    pub fn with_items(mut self, done: u64, total: Option<u64>) -> Self {
        self.items_done = done;
        self.items_total = total;
        self.updated_at = Utc::now();
        self
    }

    /// Update phase
    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self.updated_at = Utc::now();
        self
    }

    /// Calculate progress percentage (0.0 to 100.0)
    pub fn percentage(&self) -> Option<f64> {
        self.items_total.map(|total| {
            if total == 0 {
                100.0
            } else {
                (self.items_done as f64 / total as f64) * 100.0
            }
        })
    }
}

/// Job type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Backtest job
    Backtest,
    /// Parser run job
    Run,
}

impl fmt::Display for JobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backtest => write!(f, "backtest"),
            Self::Run => write!(f, "run"),
        }
    }
}

/// Full job record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID
    pub id: JobId,

    /// Job type
    pub job_type: JobType,

    /// Current state
    pub state: JobState,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Plugin reference (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_ref: Option<crate::types::PluginRef>,

    /// Input directory or file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,

    /// Approval ID (if this job was created from an approval)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,

    /// Job specification - contains all details needed to execute/restart the job
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<JobSpec>,
}

impl Job {
    /// Create a new queued job with a known ID
    pub fn new(id: JobId, job_type: JobType) -> Self {
        let now = Utc::now();
        Self {
            id,
            job_type,
            state: JobState::Queued { queued_at: now },
            created_at: now,
            plugin_ref: None,
            input: None,
            approval_id: None,
            spec: None,
        }
    }

    /// Set the job specification
    pub fn with_spec(mut self, spec: JobSpec) -> Self {
        self.spec = Some(spec);
        self
    }

    /// Set the plugin reference
    pub fn with_plugin(mut self, plugin_ref: crate::types::PluginRef) -> Self {
        self.plugin_ref = Some(plugin_ref);
        self
    }

    /// Set the input path
    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// Transition to running state
    pub fn start(&mut self) {
        self.state = JobState::Running {
            started_at: Utc::now(),
            progress: JobProgress::new(),
        };
    }

    /// Update progress
    pub fn update_progress(&mut self, progress: JobProgress) {
        if let JobState::Running { started_at, .. } = &self.state {
            self.state = JobState::Running {
                started_at: *started_at,
                progress,
            };
        }
    }

    /// Transition to completed state
    pub fn complete(&mut self, result: serde_json::Value) {
        if let JobState::Running { started_at, .. } = &self.state {
            self.state = JobState::Completed {
                started_at: *started_at,
                completed_at: Utc::now(),
                result,
            };
        }
    }

    /// Transition to failed state
    pub fn fail(&mut self, error: impl Into<String>) {
        let started_at = match &self.state {
            JobState::Running { started_at, .. } => Some(*started_at),
            _ => None,
        };

        self.state = JobState::Failed {
            started_at,
            failed_at: Utc::now(),
            error: error.into(),
        };
    }

    /// Cancel the job
    pub fn cancel(&mut self) {
        if !self.state.is_terminal() {
            self.state = JobState::Cancelled {
                cancelled_at: Utc::now(),
            };
        }
    }
}

/// Stall detection threshold (30 seconds)
pub const STALL_THRESHOLD_MS: u64 = 30_000;

/// Default job timeout (30 minutes)
pub const DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1000;

/// Default max concurrent jobs
pub const DEFAULT_MAX_CONCURRENT: usize = 1;

/// Job retention (24 hours)
pub const JOB_TTL_HOURS: i64 = 24;

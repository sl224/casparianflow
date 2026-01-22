//! Approval Subsystem
//!
//! Manages non-blocking approval requests for write operations.
//! Humans approve out-of-band via CLI, preventing deadlocks in agent loops.
//!
//! # Design
//!
//! Write operations (run, schema_promote) create approval requests that
//! return immediately with an approval_id. The human reviews and approves
//! via CLI commands:
//!
//! ```bash
//! casparian approvals list
//! casparian approvals approve <id>
//! casparian approvals reject <id> --reason "..."
//! ```
//!
//! # Storage
//!
//! Approvals are stored as JSON files in `~/.casparian_flow/approvals/`:
//!
//! ```text
//! approvals/
//! ├── {approval_id_1}.json
//! ├── {approval_id_2}.json
//! └── ...
//! ```

mod manager;
mod store;

pub use manager::ApprovalManager;
pub use store::ApprovalStore;

use crate::types::{ApprovalSummary, PluginRef};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique approval identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApprovalId(pub String);

impl ApprovalId {
    /// Create a new random approval ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for ApprovalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ApprovalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ApprovalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique approval ID
    pub approval_id: ApprovalId,

    /// Operation being requested
    pub operation: ApprovalOperation,

    /// Human-readable summary
    pub summary: ApprovalSummary,

    /// When the request was created
    pub created_at: DateTime<Utc>,

    /// When the request expires
    pub expires_at: DateTime<Utc>,

    /// Current status
    pub status: ApprovalStatus,

    /// Job ID created after approval (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

impl ApprovalRequest {
    /// Create a new approval request
    pub fn new(operation: ApprovalOperation, summary: ApprovalSummary) -> Self {
        let now = Utc::now();
        Self {
            approval_id: ApprovalId::new(),
            operation,
            summary,
            created_at: now,
            expires_at: now + chrono::Duration::hours(DEFAULT_EXPIRY_HOURS),
            status: ApprovalStatus::Pending,
            job_id: None,
        }
    }

    /// Check if the request has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Approve the request
    pub fn approve(&mut self) {
        if matches!(self.status, ApprovalStatus::Pending) && !self.is_expired() {
            self.status = ApprovalStatus::Approved {
                approved_at: Utc::now(),
            };
        }
    }

    /// Reject the request
    pub fn reject(&mut self, reason: Option<String>) {
        if matches!(self.status, ApprovalStatus::Pending) {
            self.status = ApprovalStatus::Rejected {
                rejected_at: Utc::now(),
                reason,
            };
        }
    }

    /// Mark as expired
    pub fn mark_expired(&mut self) {
        if matches!(self.status, ApprovalStatus::Pending) {
            self.status = ApprovalStatus::Expired;
        }
    }

    /// Get the CLI command to approve this request
    pub fn approve_command(&self) -> String {
        format!("casparian approvals approve {}", self.approval_id)
    }
}

/// Operation being requested for approval
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalOperation {
    /// Run parser against files (write output)
    Run {
        /// Plugin reference
        plugin_ref: PluginRef,
        /// Input directory
        input_dir: PathBuf,
        /// Output sink/path
        output: String,
    },
    /// Promote ephemeral schema to code
    SchemaPromote {
        /// Ephemeral schema ID
        ephemeral_id: String,
        /// Output file path
        output_path: PathBuf,
    },
}

impl ApprovalOperation {
    /// Get a short description of the operation
    pub fn description(&self) -> String {
        match self {
            Self::Run { plugin_ref, input_dir, output } => {
                format!(
                    "Run {} on {} -> {}",
                    plugin_ref.display_name(),
                    input_dir.display(),
                    output
                )
            }
            Self::SchemaPromote { ephemeral_id, output_path } => {
                format!(
                    "Promote schema {} to {}",
                    ephemeral_id,
                    output_path.display()
                )
            }
        }
    }
}

/// Approval status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Awaiting human approval
    Pending,
    /// Approved by human
    Approved {
        approved_at: DateTime<Utc>,
    },
    /// Rejected by human
    Rejected {
        rejected_at: DateTime<Utc>,
        reason: Option<String>,
    },
    /// Expired without action
    Expired,
}

impl ApprovalStatus {
    /// Get status string
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved { .. } => "approved",
            Self::Rejected { .. } => "rejected",
            Self::Expired => "expired",
        }
    }

    /// Check if this is a terminal status
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// Default approval expiry (1 hour)
pub const DEFAULT_EXPIRY_HOURS: i64 = 1;

/// Approval retention (7 days for completed approvals)
pub const APPROVAL_TTL_DAYS: i64 = 7;

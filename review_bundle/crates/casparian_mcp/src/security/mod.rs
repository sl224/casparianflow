//! Security Subsystem
//!
//! Provides security controls for the MCP server:
//! - Path allowlist: Validates file paths against configured roots
//! - Output budget: Limits response sizes to prevent OOM
//! - Redaction: Hashes/truncates sensitive sample values
//! - Audit logging: Records all tool invocations
//!
//! # Design Principles
//!
//! Security is P0 (must-ship), not an afterthought. All path operations
//! are validated, all outputs are bounded, and all operations are logged.

mod audit;
mod output_budget;
mod path_allowlist;

pub use audit::AuditLog;
pub use output_budget::OutputBudget;
pub use path_allowlist::PathAllowlist;

/// Combined security configuration
#[derive(Debug)]
pub struct SecurityConfig {
    /// Path validation
    pub path_allowlist: PathAllowlist,

    /// Output size limits
    pub output_budget: OutputBudget,

    /// Audit logging (optional)
    pub audit_log: Option<AuditLog>,
}

impl SecurityConfig {
    /// Validate a path is within allowed roots
    pub fn validate_path(
        &self,
        path: &std::path::Path,
    ) -> Result<std::path::PathBuf, SecurityError> {
        self.path_allowlist.validate(path)
    }
}

/// Security-related errors
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Path not allowed: {path} (not within allowed roots)")]
    PathNotAllowed { path: String },

    #[error("Path traversal attempt detected: {path}")]
    PathTraversal { path: String },

    #[error("Symlink escapes allowed roots: {path}")]
    SymlinkEscape { path: String },

    #[error("Output exceeds budget: {size} bytes > {max} bytes")]
    OutputTooLarge { size: usize, max: usize },

    #[error("Row count exceeds budget: {count} rows > {max} rows")]
    TooManyRows { count: usize, max: usize },

    #[error("Audit log error: {0}")]
    AuditError(String),
}

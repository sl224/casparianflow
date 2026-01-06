//! Error types for the database layer.

use thiserror::Error;

/// Database operation result type.
pub type Result<T> = std::result::Result<T, DbError>;

/// Database errors.
#[derive(Error, Debug)]
pub enum DbError {
    /// SQLx error (connection, query, etc.)
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// IO error (file system operations)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Constraint violation (unique, foreign key, etc.)
    #[error("Constraint violation: {0}")]
    Constraint(String),

    /// Invalid state transition
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl DbError {
    /// Create a not found error.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Create a constraint error.
    pub fn constraint(msg: impl Into<String>) -> Self {
        Self::Constraint(msg.into())
    }

    /// Create an invalid state error.
    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::InvalidState(msg.into())
    }
}

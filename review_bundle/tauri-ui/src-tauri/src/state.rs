//! Application state for Tauri commands.
//!
//! Stores the database path and creates connections on-demand.
//! This avoids thread-safety issues with DuckDB's Rc-based connection.

use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_sentinel::ApiStorage;

use crate::session_storage::SessionStorage;

/// Application state shared across Tauri commands.
///
/// Only stores the database path. Connections are created on-demand
/// because DuckDB's DbConnection uses Rc internally and isn't Send.
pub struct AppState {
    /// Path to the DuckDB database file.
    pub db_path: String,
}

impl AppState {
    /// Create a new AppState with the default database path.
    pub fn new() -> Result<Self> {
        let db_path = Self::default_db_path()?;
        Ok(Self { db_path })
    }

    /// Get the default database path.
    pub fn default_db_path() -> Result<String> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let db_dir = home.join(".casparian_flow");

        // Ensure directory exists
        std::fs::create_dir_all(&db_dir).context("Failed to create database directory")?;

        let db_path = db_dir.join("casparian_flow.duckdb");
        Ok(db_path.to_string_lossy().to_string())
    }

    /// Get the database URL.
    pub fn db_url(&self) -> String {
        format!("duckdb:{}", self.db_path)
    }

    /// Open a new API storage connection.
    ///
    /// Creates a new connection - caller is responsible for cleanup.
    pub fn open_api_storage(&self) -> Result<ApiStorage> {
        let storage = ApiStorage::open(&self.db_url()).context("Failed to open API storage")?;
        storage
            .init_schema()
            .context("Failed to initialize API schema")?;
        Ok(storage)
    }

    /// Open a read-only connection for query operations.
    pub fn open_readonly_connection(&self) -> Result<DbConnection> {
        DbConnection::open_duckdb_readonly(std::path::Path::new(&self.db_path))
            .context("Failed to open read-only connection")
    }

    /// Open session storage for session operations.
    ///
    /// Creates a new connection and initializes the session schema.
    pub fn open_session_storage(&self) -> Result<SessionStorage> {
        let storage =
            SessionStorage::open(&self.db_url()).context("Failed to open session storage")?;
        storage
            .init_schema()
            .context("Failed to initialize session schema")?;
        Ok(storage)
    }
}

// Ensure AppState is Send + Sync (only contains a String)
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

/// Error type for Tauri commands.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<anyhow::Error> for CommandError {
    fn from(err: anyhow::Error) -> Self {
        CommandError::Internal(err.to_string())
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Result type for Tauri commands.
pub type CommandResult<T> = Result<T, CommandError>;

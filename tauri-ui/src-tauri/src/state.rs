//! Application state for Tauri commands.
//!
//! Stores the database path and creates connections on-demand.
//! This avoids thread-safety issues with Rc-based connections.

use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_sentinel::{ApiStorage, ControlClient};

use crate::session_storage::SessionStorage;
use crate::tape::{create_disabled_tape, SharedTapeState};
use std::time::Duration;

/// Default Control API address when sentinel is running.
const DEFAULT_CONTROL_ADDR: &str = casparian_protocol::defaults::DEFAULT_CONTROL_ADDR;

/// Application state shared across Tauri commands.
///
/// Only stores the database path. Connections are created on-demand
/// because DbConnection uses Rc internally and isn't Send.
pub struct AppState {
    /// Path to the state store database file.
    pub db_path: String,
    /// Tape recording state (shared across commands).
    tape: SharedTapeState,
}

impl AppState {
    /// Create a new AppState with the default database path.
    pub fn new() -> Result<Self> {
        let db_path = Self::default_db_path()?;
        let tape = create_disabled_tape();
        Ok(Self { db_path, tape })
    }

    /// Get the default database path.
    pub fn default_db_path() -> Result<String> {
        let db_dir = if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
            std::path::PathBuf::from(override_path)
        } else {
            let home = dirs::home_dir().context("Could not determine home directory")?;
            home.join(".casparian_flow")
        };

        // Ensure directory exists
        std::fs::create_dir_all(&db_dir).context("Failed to create database directory")?;

        let db_path = db_dir.join("state.sqlite");
        Ok(db_path.to_string_lossy().to_string())
    }

    /// Get the database URL.
    pub fn db_url(&self) -> String {
        format!("sqlite:{}", self.db_path)
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
        DbConnection::open_sqlite_readonly(std::path::Path::new(&self.db_path))
            .context("Failed to open read-only connection")
    }

    /// Open a read-write connection for mutation operations (when control API unavailable).
    pub fn open_rw_connection(&self) -> Result<DbConnection> {
        DbConnection::open_sqlite(std::path::Path::new(&self.db_path))
            .context("Failed to open read-write connection")
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

    /// Access the shared tape state.
    pub fn tape(&self) -> &SharedTapeState {
        &self.tape
    }

    /// Attempt to connect to the control API (sentinel mutation authority).
    pub fn try_control_client(&self) -> Option<ControlClient> {
        if std::env::var("CASPARIAN_CONTROL_DISABLED").is_ok() {
            return None;
        }
        let addr =
            std::env::var("CASPARIAN_CONTROL_ADDR").unwrap_or_else(|_| DEFAULT_CONTROL_ADDR.into());
        let timeout = Duration::from_millis(500);
        let client = ControlClient::connect_with_timeout(&addr, timeout).ok()?;
        match client.ping() {
            Ok(true) => Some(client),
            _ => None,
        }
    }
}

static_assertions::assert_impl_all!(AppState: Send, Sync);

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

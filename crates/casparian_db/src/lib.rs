//! Database abstraction layer for Casparian Flow.
//!
//! Provides DuckDB database support (open source).
//!
//! DuckDB is columnar, OLAP optimized, and enforces a single-writer model.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian_db::DbConnection;
//!
//! // DuckDB
//! let conn = DbConnection::open_duckdb(Path::new("./data.duckdb"))?;
//!
//! // Unified query interface
//! conn.execute("INSERT INTO t (id) VALUES (?)", &[1.into()])?;
//! let rows = conn.query_all("SELECT * FROM t", &[])?;
//! ```

pub mod backend;
pub mod dev;
mod license;
pub mod lock;
pub mod sql_guard;

pub use backend::{
    AccessMode, BackendError, DbConnection, DbRow as UnifiedDbRow, DbTimestamp, DbTimestampError,
    DbTransaction, DbValue, FromDbValue,
};
pub use dev::dev_allow_destructive_reset;
pub use license::{License, LicenseError, LicenseTier};
#[cfg(feature = "duckdb")]
pub use lock::{is_locked, lock_exclusive, try_lock_exclusive, try_lock_shared};
pub use lock::{lock_path_for, DbLockGuard, LockError};
pub use sql_guard::{apply_row_limit, validate_read_only, SqlGuardError};
/// Database backend type.
///
/// Pre-v1 defaults to SQLite for state store, with DuckDB for local SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum DatabaseType {
    /// DuckDB - columnar OLAP database (open source)
    DuckDb,
    /// SQLite - embedded transactional database
    Sqlite,
}

impl DatabaseType {
    /// Check if this database type requires an enterprise license.
    pub fn requires_license(&self) -> bool {
        match self {
            Self::DuckDb => false,
            Self::Sqlite => false,
        }
    }

    /// Get the display name for this database type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DuckDb => "DuckDB",
            Self::Sqlite => "SQLite",
        }
    }

    /// Detect database type from a connection URL.
    pub fn from_url(url: &str) -> Option<Self> {
        if url.starts_with("duckdb:") {
            return Some(Self::DuckDb);
        }
        if url.starts_with("sqlite:") {
            return Some(Self::Sqlite);
        }

        None
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

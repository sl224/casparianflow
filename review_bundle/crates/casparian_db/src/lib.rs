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
mod license;
pub mod lock;

pub use backend::{
    AccessMode, BackendError, DbConnection, DbRow as UnifiedDbRow, DbTimestamp, DbTimestampError,
    DbTransaction, DbValue, FromDbValue,
};
pub use license::{License, LicenseError, LicenseTier};
#[cfg(feature = "duckdb")]
pub use lock::{is_locked, lock_exclusive, try_lock_exclusive, try_lock_shared};
pub use lock::{lock_path_for, DbLockGuard, LockError};
/// Database backend type.
///
/// DuckDB is the only supported backend in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum DatabaseType {
    /// DuckDB - columnar OLAP database (open source)
    DuckDb,
}

impl DatabaseType {
    /// Check if this database type requires an enterprise license.
    pub fn requires_license(&self) -> bool {
        match self {
            Self::DuckDb => false,
        }
    }

    /// Get the display name for this database type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DuckDb => "DuckDB",
        }
    }

    /// Detect database type from a connection URL.
    pub fn from_url(url: &str) -> Option<Self> {
        if url.starts_with("duckdb:") {
            return Some(Self::DuckDb);
        }

        None
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

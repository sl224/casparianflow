//! Database abstraction layer for Casparian Flow.
//!
//! Provides feature-gated database support:
//! - `sqlite` (default): SQLite file-based database (open source)
//! - `duckdb`: DuckDB columnar OLAP database (open source)
//! - `postgres`: PostgreSQL server database (enterprise license required)
//! - `mssql`: Microsoft SQL Server (enterprise license required, future)
//!
//! # DuckDB vs SQLite
//!
//! - **SQLite**: Row-oriented, OLTP optimized, multi-process safe
//! - **DuckDB**: Columnar, OLAP optimized (20-50x faster analytics), single-writer
//!
//! # License Gating
//!
//! Enterprise database backends (postgres, mssql) require a valid license.
//! The license is checked at runtime when creating a connection pool.
//!
//! # Example (Legacy sqlx API)
//!
//! ```rust,ignore
//! use casparian_db::{DbConfig, create_pool};
//!
//! let config = DbConfig::sqlite("./data.db");
//! let pool = create_pool(config).await?;
//! ```
//!
//! # Example (New Unified API)
//!
//! ```rust,ignore
//! use casparian_db::backend::DbConnection;
//!
//! // SQLite
//! let conn = DbConnection::open_sqlite(Path::new("./data.db")).await?;
//!
//! // DuckDB
//! let conn = DbConnection::open_duckdb(Path::new("./data.duckdb")).await?;
//!
//! // Unified query interface
//! conn.execute("INSERT INTO t (id) VALUES (?)", &[1.into()]).await?;
//! let rows = conn.query_all("SELECT * FROM t", &[]).await?;
//! ```

pub mod backend;
mod license;
pub mod lock;
mod pool;

pub use backend::{AccessMode, BackendError, DbConnection, DbRow as UnifiedDbRow, DbValue, FromDbValue};
pub use license::{License, LicenseError, LicenseTier};
pub use lock::{lock_path_for, DbLockGuard, LockError};
#[cfg(feature = "duckdb")]
pub use lock::{is_locked, lock_exclusive, try_lock_exclusive, try_lock_shared};
pub use pool::{create_pool, DbConfig, DbError, DbPool, DbRow};

/// Database backend type.
///
/// Only variants for compiled-in features are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum DatabaseType {
    /// SQLite - always available (open source)
    #[cfg(feature = "sqlite")]
    Sqlite,

    /// DuckDB - columnar OLAP database (open source)
    #[cfg(feature = "duckdb")]
    DuckDb,

    /// PostgreSQL - requires enterprise license
    #[cfg(feature = "postgres")]
    Postgres,
    // Future: Mssql
}

impl DatabaseType {
    /// Check if this database type requires an enterprise license.
    pub fn requires_license(&self) -> bool {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite => false,

            #[cfg(feature = "duckdb")]
            Self::DuckDb => false,

            #[cfg(feature = "postgres")]
            Self::Postgres => true,
        }
    }

    /// Get the display name for this database type.
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite => "SQLite",

            #[cfg(feature = "duckdb")]
            Self::DuckDb => "DuckDB",

            #[cfg(feature = "postgres")]
            Self::Postgres => "PostgreSQL",
        }
    }

    /// Detect database type from a connection URL.
    pub fn from_url(url: &str) -> Option<Self> {
        #[cfg(feature = "sqlite")]
        if url.starts_with("sqlite:") {
            return Some(Self::Sqlite);
        }

        #[cfg(feature = "duckdb")]
        if url.starts_with("duckdb:") {
            return Some(Self::DuckDb);
        }

        #[cfg(feature = "postgres")]
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return Some(Self::Postgres);
        }

        None
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

//! Database abstraction layer for Casparian Flow.
//!
//! Provides feature-gated database support:
//! - `sqlite` (default): SQLite file-based database (open source)
//! - `postgres`: PostgreSQL server database (enterprise license required)
//! - `mssql`: Microsoft SQL Server (enterprise license required, future)
//!
//! # License Gating
//!
//! Enterprise database backends (postgres, mssql) require a valid license.
//! The license is checked at runtime when creating a connection pool.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian_db::{DbConfig, create_pool};
//!
//! let config = DbConfig::sqlite("./data.db");
//! let pool = create_pool(config).await?;
//! ```

mod license;
mod pool;

pub use license::{License, LicenseError, LicenseTier};
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

            #[cfg(feature = "postgres")]
            Self::Postgres => true,
        }
    }

    /// Get the display name for this database type.
    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sqlite")]
            Self::Sqlite => "SQLite",

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

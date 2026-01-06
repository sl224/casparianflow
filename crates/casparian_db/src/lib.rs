//! Unified database layer for Casparian Flow
//!
//! This crate provides a single source of truth for all database operations.
//! All interfaces (CLI, Tauri UI, MCP) should use this crate for database access.
//!
//! # Usage
//!
//! ```rust,ignore
//! use casparian_db::{CasparianDb, Result};
//!
//! let db = CasparianDb::open("~/.casparian_flow/casparian_flow.sqlite3").await?;
//!
//! // Scout operations
//! let sources = db.scout_list_sources().await?;
//!
//! // Parser Lab operations
//! let parsers = db.parser_list_all().await?;
//!
//! // Sentinel operations
//! let job = db.sentinel_pop_job().await?;
//! ```

mod error;
mod schema;
mod types;

// Method implementations organized by domain
mod scout;
mod parser_lab;
pub mod sentinel;

pub use error::{DbError, Result};
pub use sentinel::QueueStats;
pub use types::*;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use tracing::info;

/// Unified database for all Casparian Flow operations.
///
/// This is the ONLY way to access the database. Do not use raw sqlx/rusqlite elsewhere.
#[derive(Clone)]
pub struct CasparianDb {
    pool: SqlitePool,
}

impl CasparianDb {
    /// Open or create a database at the given path.
    ///
    /// Creates all tables if they don't exist.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let url = format!("sqlite:{}?mode=rwc", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        let db = Self { pool };

        // Run migrations/schema creation
        db.ensure_schema().await?;

        info!(path = %path.display(), "Database opened");

        Ok(db)
    }

    /// Open an existing database (fails if not exists).
    pub async fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(DbError::NotFound(format!(
                "Database not found: {}",
                path.display()
            )));
        }

        let url = format!("sqlite:{}?mode=rw", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        Ok(Self { pool })
    }

    /// Get the underlying connection pool (escape hatch for complex queries).
    ///
    /// Prefer using the typed methods instead.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Close the database connection.
    pub async fn close(self) {
        self.pool.close().await;
    }
}

// Timestamp utilities
impl CasparianDb {
    /// Current time as milliseconds since Unix epoch.
    pub fn now_millis() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Convert milliseconds to DateTime.
    pub fn millis_to_datetime(millis: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp_millis(millis)
            .unwrap_or_else(chrono::Utc::now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_open_creates_database() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let db = CasparianDb::open(&db_path).await.unwrap();
        assert!(db_path.exists());

        db.close().await;
    }

    #[tokio::test]
    async fn test_open_existing_fails_if_not_exists() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("nonexistent.db");

        let result = CasparianDb::open_existing(&db_path).await;
        assert!(result.is_err());
    }
}

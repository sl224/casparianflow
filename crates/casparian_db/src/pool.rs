//! Database pool creation with license validation.
//!
//! This module provides compile-time database selection via feature flags.
//! Unlike `sqlx::AnyPool`, we use concrete pool types which allows full
//! support for `#[derive(FromRow)]` with custom types like enums and DateTime.
//!
//! # Feature Priority
//!
//! - `postgres` feature: Uses `PgPool` (enterprise)
//! - `sqlite` feature (default): Uses `SqlitePool` (community)
//!
//! If both features are enabled, `postgres` takes priority (enterprise build).

use thiserror::Error;
use tracing::info;

use crate::{DatabaseType, License, LicenseError};

/// Database pool errors.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("License error: {0}")]
    License(#[from] LicenseError),

    #[error("Invalid database URL: {0}")]
    InvalidUrl(String),

    #[error("Database type {0} not compiled in. Rebuild with the '{1}' feature.")]
    NotCompiled(String, String),
}

/// Database pool type alias.
///
/// Uses compile-time feature flags to select the concrete pool type.
/// This allows full support for `#[derive(FromRow)]` with custom types.
///
/// - With `postgres` feature: `PgPool`
/// - With `sqlite` feature (default): `SqlitePool`
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbPool = sqlx::PgPool;

#[cfg(feature = "sqlite")]
pub type DbPool = sqlx::SqlitePool;

/// Database row type for queries.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbRow = sqlx::postgres::PgRow;

#[cfg(feature = "sqlite")]
pub type DbRow = sqlx::sqlite::SqliteRow;

/// Database configuration.
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// Database connection URL
    pub url: String,
    /// Detected database type
    pub db_type: DatabaseType,
    /// Maximum connections in the pool
    pub max_connections: u32,
    /// License (for enterprise features)
    pub license: License,
}

impl DbConfig {
    /// Create SQLite configuration.
    #[cfg(feature = "sqlite")]
    pub fn sqlite(path: impl AsRef<str>) -> Self {
        Self {
            url: format!("sqlite:{}?mode=rwc", path.as_ref()),
            db_type: DatabaseType::Sqlite,
            max_connections: 5,
            license: License::community(),
        }
    }

    /// Create in-memory SQLite configuration (for testing).
    #[cfg(feature = "sqlite")]
    pub fn sqlite_memory() -> Self {
        Self {
            url: "sqlite::memory:".to_string(),
            db_type: DatabaseType::Sqlite,
            max_connections: 1,
            license: License::community(),
        }
    }

    /// Create PostgreSQL configuration.
    ///
    /// Requires a license with Professional or Enterprise tier.
    #[cfg(feature = "postgres")]
    pub fn postgres(url: impl Into<String>, license: License) -> Self {
        Self {
            url: url.into(),
            db_type: DatabaseType::Postgres,
            max_connections: 10,
            license,
        }
    }

    /// Create configuration from a URL, auto-detecting database type.
    pub fn from_url(url: impl Into<String>, license: License) -> Result<Self, DbError> {
        let url = url.into();
        let db_type = DatabaseType::from_url(&url)
            .ok_or_else(|| DbError::InvalidUrl(url.clone()))?;

        Ok(Self {
            url,
            db_type,
            max_connections: match db_type {
                #[cfg(feature = "sqlite")]
                DatabaseType::Sqlite => 5,
                #[cfg(feature = "postgres")]
                DatabaseType::Postgres => 10,
            },
            license,
        })
    }

    /// Set maximum connections.
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set license.
    pub fn with_license(mut self, license: License) -> Self {
        self.license = license;
        self
    }
}

/// Create a database pool from configuration.
///
/// This function:
/// 1. Validates the license allows the requested database type
/// 2. Creates the connection pool
/// 3. Applies database-specific optimizations (e.g., SQLite WAL mode)
///
/// # Errors
///
/// Returns an error if:
/// - The license doesn't allow the database type
/// - The database type isn't compiled in
/// - Connection fails
pub async fn create_pool(config: DbConfig) -> Result<DbPool, DbError> {
    // Check license for enterprise features
    if config.db_type.requires_license() {
        config.license.allows(config.db_type)?;
        info!(
            "License validated for {} (org: {}, tier: {:?})",
            config.db_type, config.license.organization, config.license.tier
        );
    }

    // Create pool based on compiled feature
    #[cfg(feature = "sqlite")]
    {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;

        apply_sqlite_optimizations(&pool).await?;

        info!("Connected to {} database", config.db_type);
        return Ok(pool);
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;

        info!("Connected to {} database", config.db_type);
        return Ok(pool);
    }

    // This should be unreachable if at least one feature is enabled
    #[allow(unreachable_code)]
    Err(DbError::NotCompiled(
        "unknown".to_string(),
        "sqlite or postgres".to_string(),
    ))
}

/// Apply SQLite-specific optimizations.
#[cfg(feature = "sqlite")]
async fn apply_sqlite_optimizations(pool: &DbPool) -> Result<(), DbError> {
    // WAL mode for better concurrent access
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(pool)
        .await?;

    // NORMAL sync for better performance
    sqlx::query("PRAGMA synchronous=NORMAL")
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "sqlite")]
    async fn test_sqlite_pool() {
        let config = DbConfig::sqlite_memory();
        let pool = create_pool(config).await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    #[cfg(feature = "postgres")]
    async fn test_postgres_requires_license() {
        let config = DbConfig::postgres(
            "postgres://localhost/test",
            License::community(), // Community license
        );

        let result = create_pool(config).await;
        assert!(matches!(result, Err(DbError::License(_))));
    }
}

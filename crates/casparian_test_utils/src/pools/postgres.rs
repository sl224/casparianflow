//! PostgreSQL test pool factory.

use crate::config::{DbVersion, TestDbConfig};
use crate::containers::lifecycle::ensure_container_running;
use anyhow::{bail, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;
use tracing::info;

/// A PostgreSQL connection pool for testing.
///
/// Automatically ensures the Docker container is running before connecting.
#[derive(Debug, Clone)]
pub struct TestPgPool {
    /// The underlying sqlx pool
    pub pool: PgPool,
    /// The database version this pool connects to
    pub version: DbVersion,
}

impl TestPgPool {
    /// Create a new test pool for the specified PostgreSQL version.
    ///
    /// This will:
    /// 1. Ensure the Docker container is running
    /// 2. Wait for the database to be healthy
    /// 3. Create a connection pool
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use casparian_test_utils::{DbVersion, TestPgPool};
    ///
    /// #[tokio::test]
    /// async fn test_postgres() {
    ///     let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();
    ///     // Use pool.pool for queries
    /// }
    /// ```
    pub async fn new(version: DbVersion) -> Result<Self> {
        if !version.is_postgres() {
            bail!(
                "Cannot create TestPgPool for non-PostgreSQL version: {:?}",
                version
            );
        }

        // Ensure container is running
        ensure_container_running(version).await?;

        // Create connection pool
        let config = TestDbConfig::new(version);
        let conn_str = config.postgres_connection_string();

        info!(
            "Creating PostgreSQL pool for {} on port {}",
            version,
            version.port()
        );

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&conn_str)
            .await?;

        Ok(Self { pool, version })
    }

    /// Execute a query that doesn't return rows.
    pub async fn execute(&self, query: &str) -> Result<()> {
        sqlx::query(query).execute(&self.pool).await?;
        Ok(())
    }

    /// Get the underlying pool.
    pub fn inner(&self) -> &PgPool {
        &self.pool
    }
}

/// Macro for running a test against all PostgreSQL versions.
///
/// # Example
///
/// ```rust,ignore
/// use casparian_test_utils::test_all_postgres;
///
/// test_all_postgres!(test_insert, |pool: TestPgPool| async move {
///     pool.execute("SELECT 1").await.unwrap();
/// });
/// ```
#[macro_export]
macro_rules! test_all_postgres {
    ($name:ident, |$pool:ident: TestPgPool| $body:expr) => {
        paste::paste! {
            #[tokio::test]
            #[cfg(feature = "docker-tests")]
            async fn [<$name _postgres14>]() {
                let $pool = $crate::TestPgPool::new($crate::DbVersion::Postgres14)
                    .await
                    .unwrap();
                $body
            }

            #[tokio::test]
            #[cfg(feature = "docker-tests")]
            async fn [<$name _postgres15>]() {
                let $pool = $crate::TestPgPool::new($crate::DbVersion::Postgres15)
                    .await
                    .unwrap();
                $body
            }

            #[tokio::test]
            #[cfg(feature = "docker-tests")]
            async fn [<$name _postgres16>]() {
                let $pool = $crate::TestPgPool::new($crate::DbVersion::Postgres16)
                    .await
                    .unwrap();
                $body
            }
        }
    };
}

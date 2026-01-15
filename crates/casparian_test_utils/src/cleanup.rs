//! Test cleanup utilities for database isolation.

use anyhow::Result;
use sqlx::PgPool;
use tracing::{debug, warn};
use uuid::Uuid;

/// RAII guard for PostgreSQL test isolation.
///
/// Creates a unique schema for each test and drops it on Drop.
/// This provides test isolation without requiring separate databases.
///
/// # Example
///
/// ```rust,ignore
/// use casparian_test_utils::{DbVersion, TestPgPool, PostgresTestGuard};
///
/// #[tokio::test]
/// async fn test_isolated() {
///     let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();
///     let guard = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();
///
///     // All operations use the isolated schema
///     guard.execute("CREATE TABLE output (id INT)").await.unwrap();
///     guard.execute("INSERT INTO output VALUES (1)").await.unwrap();
///
///     // Schema is automatically dropped when guard goes out of scope
/// }
/// ```
pub struct PostgresTestGuard {
    pool: PgPool,
    schema_name: String,
}

impl PostgresTestGuard {
    /// Create a new test guard with a unique schema.
    ///
    /// The schema name is generated using a UUID to ensure uniqueness.
    pub async fn new(pool: PgPool) -> Result<Self> {
        let schema_name = format!("test_{}", Uuid::new_v4().simple());

        debug!("Creating test schema: {}", schema_name);

        // Create the schema
        sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
            .execute(&pool)
            .await?;

        // Set the search path for this connection
        sqlx::query(&format!("SET search_path TO {}", schema_name))
            .execute(&pool)
            .await?;

        Ok(Self { pool, schema_name })
    }

    /// Create a test guard with a custom schema name.
    ///
    /// Useful for debugging or when you need a predictable schema name.
    pub async fn with_name(pool: PgPool, schema_name: &str) -> Result<Self> {
        debug!("Creating test schema: {}", schema_name);

        // Create the schema
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name))
            .execute(&pool)
            .await?;

        // Set the search path
        sqlx::query(&format!("SET search_path TO {}", schema_name))
            .execute(&pool)
            .await?;

        Ok(Self {
            pool,
            schema_name: schema_name.to_string(),
        })
    }

    /// Execute a query in the isolated schema.
    ///
    /// Uses a transaction to ensure search_path and query use same connection.
    pub async fn execute(&self, query: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(&format!("SET search_path TO {}", self.schema_name))
            .execute(&mut *tx)
            .await?;
        sqlx::query(query).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Execute a query and return all rows.
    ///
    /// Uses a transaction to ensure search_path and query use same connection.
    pub async fn fetch_all(
        &self,
        query: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(&format!("SET search_path TO {}", self.schema_name))
            .execute(&mut *tx)
            .await?;
        let rows = sqlx::query(query).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(rows)
    }

    /// Get the schema name.
    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }

    /// Get a reference to the underlying pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Manually cleanup the schema.
    ///
    /// This is called automatically on Drop, but can be called manually
    /// if you need to verify cleanup succeeded.
    pub async fn cleanup(&self) -> Result<()> {
        debug!("Dropping test schema: {}", self.schema_name);

        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema_name))
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

impl Drop for PostgresTestGuard {
    fn drop(&mut self) {
        // We can't use async in Drop, so we spawn a blocking task
        let pool = self.pool.clone();
        let schema_name = self.schema_name.clone();

        // Use tokio's block_in_place if we're in an async context,
        // otherwise just log a warning
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) =
                    sqlx::query(&format!("DROP SCHEMA {} CASCADE", schema_name))
                        .execute(&pool)
                        .await
                {
                    warn!("Failed to drop test schema {}: {}", schema_name, e);
                } else {
                    debug!("Dropped test schema: {}", schema_name);
                }
            });
        } else {
            warn!(
                "Not in async context, cannot cleanup schema: {}",
                self.schema_name
            );
        }
    }
}

/// Truncate all tables in a schema.
///
/// Useful for cleaning up between test cases without dropping the schema.
pub async fn truncate_all_tables(pool: &PgPool, schema_name: &str) -> Result<()> {
    // Get all tables in the schema
    let tables: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT tablename::text
        FROM pg_tables
        WHERE schemaname = $1
        "#,
    )
    .bind(schema_name)
    .fetch_all(pool)
    .await?;

    for (table,) in tables {
        debug!("Truncating {}.{}", schema_name, table);
        sqlx::query(&format!(
            "TRUNCATE {}.{} RESTART IDENTITY CASCADE",
            schema_name, table
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Drop all tables in a schema without dropping the schema itself.
pub async fn drop_all_tables(pool: &PgPool, schema_name: &str) -> Result<()> {
    let tables: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT tablename::text
        FROM pg_tables
        WHERE schemaname = $1
        "#,
    )
    .bind(schema_name)
    .fetch_all(pool)
    .await?;

    for (table,) in tables {
        debug!("Dropping {}.{}", schema_name, table);
        sqlx::query(&format!("DROP TABLE {}.{} CASCADE", schema_name, table))
            .execute(pool)
            .await?;
    }

    Ok(())
}

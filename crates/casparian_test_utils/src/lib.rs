//! Casparian Test Utilities
//!
//! Docker-based test infrastructure for PostgreSQL and MSSQL testing.
//!
//! # Features
//!
//! - `docker-tests`: Enable tests that require Docker containers
//! - `mssql`: Enable MSSQL support via tiberius
//!
//! # Usage
//!
//! ```rust,ignore
//! use casparian_test_utils::{DbVersion, TestPgPool, PostgresTestGuard};
//!
//! #[tokio::test]
//! #[cfg(feature = "docker-tests")]
//! async fn test_postgres_sink() {
//!     let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();
//!     let guard = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();
//!
//!     // Test runs in isolated schema
//!     guard.execute("CREATE TABLE output (id INT)").await.unwrap();
//!     // ...
//!
//!     // Cleanup automatic on Drop
//! }
//! ```

pub mod cleanup;
pub mod config;
pub mod containers;
pub mod pools;
pub mod sinks;

// Re-exports for convenience
pub use cleanup::PostgresTestGuard;
pub use config::{DbVersion, TestDbConfig};
pub use containers::lifecycle::{ensure_container_running, wait_for_healthy};
pub use pools::postgres::TestPgPool;

#[cfg(feature = "mssql")]
pub use pools::mssql::TestMssqlPool;

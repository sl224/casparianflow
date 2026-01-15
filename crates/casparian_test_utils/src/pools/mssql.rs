//! MSSQL test pool factory.
//!
//! Requires the `mssql` feature to be enabled.

use crate::config::{DbVersion, TestDbConfig};
use crate::containers::lifecycle::ensure_container_running;
use anyhow::{bail, Result};
use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;
use tracing::info;

/// A MSSQL connection for testing.
///
/// Automatically ensures the Docker container is running before connecting.
pub struct TestMssqlPool {
    /// The database version this pool connects to
    pub version: DbVersion,
    /// Connection configuration
    config: Config,
}

impl TestMssqlPool {
    /// Create a new test pool for the specified MSSQL version.
    ///
    /// This will:
    /// 1. Ensure the Docker container is running
    /// 2. Wait for the database to be healthy
    /// 3. Return a pool that can create connections
    pub async fn new(version: DbVersion) -> Result<Self> {
        if !version.is_mssql() {
            bail!(
                "Cannot create TestMssqlPool for non-MSSQL version: {:?}",
                version
            );
        }

        // Ensure container is running
        ensure_container_running(version).await?;

        let db_config = TestDbConfig::new(version);

        info!(
            "Creating MSSQL pool for {} on port {}",
            version,
            version.port()
        );

        // Build tiberius config
        let mut config = Config::new();
        config.host("localhost");
        config.port(version.port());
        config.authentication(tiberius::AuthMethod::sql_server(
            &db_config.username,
            &db_config.password,
        ));
        config.trust_cert();

        // Test connection
        let tcp = TcpStream::connect(format!("localhost:{}", version.port())).await?;
        tcp.set_nodelay(true)?;
        let _client = Client::connect(config.clone(), tcp.compat_write()).await?;

        Ok(Self { version, config })
    }

    /// Get a new connection from this pool.
    pub async fn get_connection(&self) -> Result<Client<tokio_util::compat::Compat<TcpStream>>> {
        let tcp = TcpStream::connect(format!("localhost:{}", self.version.port())).await?;
        tcp.set_nodelay(true)?;
        let client = Client::connect(self.config.clone(), tcp.compat_write()).await?;
        Ok(client)
    }

    /// Execute a query that doesn't return rows.
    pub async fn execute(&self, query: &str) -> Result<()> {
        let mut client = self.get_connection().await?;
        client.execute(query, &[]).await?;
        Ok(())
    }
}

/// Macro for running a test against all MSSQL versions.
#[macro_export]
macro_rules! test_all_mssql {
    ($name:ident, |$pool:ident: TestMssqlPool| $body:expr) => {
        paste::paste! {
            #[tokio::test]
            #[cfg(all(feature = "docker-tests", feature = "mssql"))]
            async fn [<$name _mssql2019>]() {
                let $pool = $crate::TestMssqlPool::new($crate::DbVersion::Mssql2019)
                    .await
                    .unwrap();
                $body
            }

            #[tokio::test]
            #[cfg(all(feature = "docker-tests", feature = "mssql"))]
            async fn [<$name _mssql2022>]() {
                let $pool = $crate::TestMssqlPool::new($crate::DbVersion::Mssql2022)
                    .await
                    .unwrap();
                $body
            }
        }
    };
}

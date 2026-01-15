//! Database configuration types for test infrastructure.

use std::fmt;

/// Database version enum for multi-version testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DbVersion {
    /// PostgreSQL 14
    Postgres14,
    /// PostgreSQL 15
    Postgres15,
    /// PostgreSQL 16
    Postgres16,
    /// Microsoft SQL Server 2019
    Mssql2019,
    /// Microsoft SQL Server 2022
    Mssql2022,
}

impl DbVersion {
    /// Get the docker-compose service name for this version.
    pub fn service_name(&self) -> &'static str {
        match self {
            DbVersion::Postgres14 => "postgres14",
            DbVersion::Postgres15 => "postgres15",
            DbVersion::Postgres16 => "postgres16",
            DbVersion::Mssql2019 => "mssql2019",
            DbVersion::Mssql2022 => "mssql2022",
        }
    }

    /// Get the host port for this database version.
    pub fn port(&self) -> u16 {
        match self {
            DbVersion::Postgres14 => 15432,
            DbVersion::Postgres15 => 15433,
            DbVersion::Postgres16 => 15434,
            DbVersion::Mssql2019 => 11433,
            DbVersion::Mssql2022 => 11434,
        }
    }

    /// Check if this is a PostgreSQL version.
    pub fn is_postgres(&self) -> bool {
        matches!(
            self,
            DbVersion::Postgres14 | DbVersion::Postgres15 | DbVersion::Postgres16
        )
    }

    /// Check if this is a MSSQL version.
    pub fn is_mssql(&self) -> bool {
        matches!(self, DbVersion::Mssql2019 | DbVersion::Mssql2022)
    }

    /// Get all PostgreSQL versions.
    pub fn all_postgres() -> &'static [DbVersion] {
        &[
            DbVersion::Postgres14,
            DbVersion::Postgres15,
            DbVersion::Postgres16,
        ]
    }

    /// Get all MSSQL versions.
    pub fn all_mssql() -> &'static [DbVersion] {
        &[DbVersion::Mssql2019, DbVersion::Mssql2022]
    }
}

impl fmt::Display for DbVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbVersion::Postgres14 => write!(f, "PostgreSQL 14"),
            DbVersion::Postgres15 => write!(f, "PostgreSQL 15"),
            DbVersion::Postgres16 => write!(f, "PostgreSQL 16"),
            DbVersion::Mssql2019 => write!(f, "SQL Server 2019"),
            DbVersion::Mssql2022 => write!(f, "SQL Server 2022"),
        }
    }
}

/// Configuration for test database connections.
#[derive(Debug, Clone)]
pub struct TestDbConfig {
    /// Database version to use
    pub version: DbVersion,
    /// Host (defaults to localhost)
    pub host: String,
    /// Database name (defaults to casparian_test)
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
}

impl TestDbConfig {
    /// Create a new config for the specified version with default credentials.
    pub fn new(version: DbVersion) -> Self {
        let (username, password) = if version.is_postgres() {
            ("casparian".to_string(), "casparian_test".to_string())
        } else {
            // MSSQL uses SA account
            ("sa".to_string(), "Casparian_Test_123!".to_string())
        };

        Self {
            version,
            host: "localhost".to_string(),
            database: "casparian_test".to_string(),
            username,
            password,
        }
    }

    /// Build a PostgreSQL connection string.
    pub fn postgres_connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username,
            self.password,
            self.host,
            self.version.port(),
            self.database
        )
    }

    /// Build a MSSQL connection string (for tiberius).
    #[cfg(feature = "mssql")]
    pub fn mssql_connection_string(&self) -> String {
        format!(
            "Server={},{};Database={};User Id={};Password={};TrustServerCertificate=true",
            self.host,
            self.version.port(),
            self.database,
            self.username,
            self.password
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_versions() {
        assert!(DbVersion::Postgres14.is_postgres());
        assert!(DbVersion::Postgres15.is_postgres());
        assert!(DbVersion::Postgres16.is_postgres());
        assert!(!DbVersion::Mssql2019.is_postgres());
    }

    #[test]
    fn test_mssql_versions() {
        assert!(DbVersion::Mssql2019.is_mssql());
        assert!(DbVersion::Mssql2022.is_mssql());
        assert!(!DbVersion::Postgres14.is_mssql());
    }

    #[test]
    fn test_ports() {
        assert_eq!(DbVersion::Postgres14.port(), 15432);
        assert_eq!(DbVersion::Postgres15.port(), 15433);
        assert_eq!(DbVersion::Postgres16.port(), 15434);
        assert_eq!(DbVersion::Mssql2019.port(), 11433);
        assert_eq!(DbVersion::Mssql2022.port(), 11434);
    }

    #[test]
    fn test_config_connection_string() {
        let config = TestDbConfig::new(DbVersion::Postgres16);
        assert_eq!(
            config.postgres_connection_string(),
            "postgres://casparian:casparian_test@localhost:15434/casparian_test"
        );
    }
}

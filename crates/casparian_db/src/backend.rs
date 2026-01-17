//! Database backend abstraction layer.
//!
//! Provides a unified interface for different database backends:
//! - SQLite (via sqlx) - row-oriented, OLTP
//! - DuckDB (via async-duckdb) - columnar, OLAP
//! - PostgreSQL (via sqlx) - enterprise

use std::path::Path;
use thiserror::Error;
use tracing::info;

#[cfg(feature = "duckdb")]
use std::sync::Arc;

/// Errors from database backend operations.
#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Database is locked by another process: {0}")]
    Locked(String),

    #[error("Operation requires write access but database is read-only")]
    ReadOnly,

    #[error("Query error: {0}")]
    Query(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Type conversion error: {0}")]
    TypeConversion(String),

    #[error("Backend not available: {0}")]
    NotAvailable(String),

    #[cfg(feature = "sqlite")]
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[cfg(feature = "duckdb")]
    #[error("DuckDB error: {0}")]
    DuckDb(String),
}

#[cfg(feature = "duckdb")]
impl From<duckdb::Error> for BackendError {
    fn from(e: duckdb::Error) -> Self {
        BackendError::DuckDb(e.to_string())
    }
}

#[cfg(feature = "duckdb")]
impl From<async_duckdb::Error> for BackendError {
    fn from(e: async_duckdb::Error) -> Self {
        BackendError::DuckDb(e.to_string())
    }
}

/// Database access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// Read-write access (requires exclusive lock for DuckDB)
    ReadWrite,
    /// Read-only access (can coexist with other readers)
    ReadOnly,
}

/// Value type for query parameters.
#[derive(Debug, Clone)]
pub enum DbValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Boolean(bool),
}

impl From<i32> for DbValue {
    fn from(v: i32) -> Self {
        DbValue::Integer(v as i64)
    }
}

impl From<i64> for DbValue {
    fn from(v: i64) -> Self {
        DbValue::Integer(v)
    }
}

impl From<f64> for DbValue {
    fn from(v: f64) -> Self {
        DbValue::Real(v)
    }
}

impl From<String> for DbValue {
    fn from(v: String) -> Self {
        DbValue::Text(v)
    }
}

impl From<&str> for DbValue {
    fn from(v: &str) -> Self {
        DbValue::Text(v.to_string())
    }
}

impl From<bool> for DbValue {
    fn from(v: bool) -> Self {
        DbValue::Boolean(v)
    }
}

impl From<Vec<u8>> for DbValue {
    fn from(v: Vec<u8>) -> Self {
        DbValue::Blob(v)
    }
}

impl<T: Into<DbValue>> From<Option<T>> for DbValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(val) => val.into(),
            None => DbValue::Null,
        }
    }
}

/// Row data from a query result.
#[derive(Debug, Clone)]
pub struct DbRow {
    columns: Vec<String>,
    values: Vec<DbValue>,
}

impl DbRow {
    /// Create a new row with column names and values.
    pub fn new(columns: Vec<String>, values: Vec<DbValue>) -> Self {
        Self { columns, values }
    }

    /// Get a value by column index.
    pub fn get<T: FromDbValue>(&self, index: usize) -> Result<T, BackendError> {
        self.values
            .get(index)
            .ok_or_else(|| BackendError::TypeConversion(format!("Column index {} out of bounds", index)))
            .and_then(|v| T::from_db_value(v))
    }

    /// Get a value by column name.
    pub fn get_by_name<T: FromDbValue>(&self, name: &str) -> Result<T, BackendError> {
        let index = self
            .columns
            .iter()
            .position(|c| c == name)
            .ok_or_else(|| BackendError::TypeConversion(format!("Column '{}' not found", name)))?;
        self.get(index)
    }

    /// Get the number of columns.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the row is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Trait for converting from DbValue.
pub trait FromDbValue: Sized {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError>;
}

impl FromDbValue for i64 {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Integer(v) => Ok(*v),
            DbValue::Null => Ok(0),
            _ => Err(BackendError::TypeConversion("Expected integer".to_string())),
        }
    }
}

impl FromDbValue for i32 {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Integer(v) => Ok(*v as i32),
            DbValue::Null => Ok(0),
            _ => Err(BackendError::TypeConversion("Expected integer".to_string())),
        }
    }
}

impl FromDbValue for f64 {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Real(v) => Ok(*v),
            DbValue::Integer(v) => Ok(*v as f64),
            DbValue::Null => Ok(0.0),
            _ => Err(BackendError::TypeConversion("Expected real".to_string())),
        }
    }
}

impl FromDbValue for String {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Text(v) => Ok(v.clone()),
            DbValue::Null => Ok(String::new()),
            _ => Err(BackendError::TypeConversion("Expected text".to_string())),
        }
    }
}

impl FromDbValue for bool {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Boolean(v) => Ok(*v),
            DbValue::Integer(v) => Ok(*v != 0),
            DbValue::Null => Ok(false),
            _ => Err(BackendError::TypeConversion("Expected boolean".to_string())),
        }
    }
}

impl<T: FromDbValue> FromDbValue for Option<T> {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Null => Ok(None),
            _ => T::from_db_value(value).map(Some),
        }
    }
}

impl FromDbValue for Vec<u8> {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Blob(v) => Ok(v.clone()),
            DbValue::Null => Ok(Vec::new()),
            _ => Err(BackendError::TypeConversion("Expected blob".to_string())),
        }
    }
}

/// Unified database connection that works with multiple backends.
#[derive(Clone)]
pub struct DbConnection {
    inner: DbConnectionInner,
    access_mode: AccessMode,
    /// Lock guard for DuckDB write mode (ensures single-writer).
    /// Shared via Arc so clones maintain the same lock.
    #[cfg(feature = "duckdb")]
    _lock_guard: Option<Arc<crate::lock::DbLockGuard>>,
}

#[derive(Clone)]
enum DbConnectionInner {
    #[cfg(feature = "sqlite")]
    Sqlite(sqlx::SqlitePool),

    #[cfg(feature = "duckdb")]
    DuckDb(Arc<async_duckdb::Client>),

    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

impl std::fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backend = match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(_) => "SQLite",
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(_) => "DuckDB",
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(_) => "PostgreSQL",
        };
        f.debug_struct("DbConnection")
            .field("backend", &backend)
            .field("access_mode", &self.access_mode)
            .finish()
    }
}

impl DbConnection {
    /// Open a SQLite database.
    #[cfg(feature = "sqlite")]
    pub async fn open_sqlite(path: &Path) -> Result<Self, BackendError> {
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        // Apply optimizations
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA synchronous=NORMAL")
            .execute(&pool)
            .await?;

        info!("Opened SQLite database: {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::Sqlite(pool),
            access_mode: AccessMode::ReadWrite,
            #[cfg(feature = "duckdb")]
            _lock_guard: None,
        })
    }

    /// Open an in-memory SQLite database (for testing).
    #[cfg(feature = "sqlite")]
    pub async fn open_sqlite_memory() -> Result<Self, BackendError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        info!("Opened in-memory SQLite database");
        Ok(Self {
            inner: DbConnectionInner::Sqlite(pool),
            access_mode: AccessMode::ReadWrite,
            #[cfg(feature = "duckdb")]
            _lock_guard: None,
        })
    }

    /// Open a DuckDB database with exclusive write lock.
    ///
    /// DuckDB only allows one writer process at a time. This function acquires
    /// an exclusive lock before opening the database. If another process holds
    /// the lock, returns `BackendError::Locked`.
    ///
    /// For read-only access without locking, use `open_duckdb_readonly()`.
    #[cfg(feature = "duckdb")]
    pub async fn open_duckdb(path: &Path) -> Result<Self, BackendError> {
        use crate::lock::{try_lock_exclusive, LockError};

        // Acquire exclusive lock first (DuckDB single-writer constraint)
        let lock_guard = try_lock_exclusive(path).map_err(|e| match e {
            LockError::Locked(p) => BackendError::Locked(p.display().to_string()),
            LockError::CreateFailed(io) => BackendError::Database(format!("Lock file error: {}", io)),
            LockError::AcquireFailed(io) => BackendError::Database(format!("Lock acquire error: {}", io)),
        })?;

        let path_str = path.to_string_lossy().to_string();
        let client = async_duckdb::ClientBuilder::new()
            .path(&path_str)
            .open()
            .await?;

        info!("Opened DuckDB database with exclusive lock: {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::DuckDb(Arc::new(client)),
            access_mode: AccessMode::ReadWrite,
            _lock_guard: Some(Arc::new(lock_guard)),
        })
    }

    /// Open a DuckDB database in read-only mode (no lock required).
    ///
    /// Multiple processes can open the database in read-only mode simultaneously.
    /// Use this when you only need to query data, not modify it.
    #[cfg(feature = "duckdb")]
    pub async fn open_duckdb_readonly(path: &Path) -> Result<Self, BackendError> {
        use duckdb::{AccessMode as DuckAccessMode, Config};

        let path_str = path.to_string_lossy().to_string();
        let client = async_duckdb::ClientBuilder::new()
            .path(&path_str)
            .flagsfn(|| Config::default().access_mode(DuckAccessMode::ReadOnly))
            .open()
            .await?;

        info!("Opened DuckDB database (read-only): {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::DuckDb(Arc::new(client)),
            access_mode: AccessMode::ReadOnly,
            _lock_guard: None, // No lock needed for read-only
        })
    }

    /// Open an in-memory DuckDB database (for testing).
    ///
    /// In-memory databases don't require locking since they're process-local.
    #[cfg(feature = "duckdb")]
    pub async fn open_duckdb_memory() -> Result<Self, BackendError> {
        let client = async_duckdb::ClientBuilder::new()
            .open()
            .await?;

        info!("Opened in-memory DuckDB database");
        Ok(Self {
            inner: DbConnectionInner::DuckDb(Arc::new(client)),
            access_mode: AccessMode::ReadWrite,
            _lock_guard: None, // No lock needed for in-memory
        })
    }

    /// Open a PostgreSQL database.
    #[cfg(feature = "postgres")]
    pub async fn open_postgres(url: &str) -> Result<Self, BackendError> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await?;

        info!("Opened PostgreSQL database");
        Ok(Self {
            inner: DbConnectionInner::Postgres(pool),
            access_mode: AccessMode::ReadWrite,
            #[cfg(feature = "duckdb")]
            _lock_guard: None,
        })
    }

    /// Get the access mode.
    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    /// Check if this connection has write access.
    pub fn is_writable(&self) -> bool {
        self.access_mode == AccessMode::ReadWrite
    }

    /// Get the backend name.
    pub fn backend_name(&self) -> &'static str {
        match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(_) => "SQLite",
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(_) => "DuckDB",
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(_) => "PostgreSQL",
        }
    }

    /// Execute a SQL statement (no results).
    ///
    /// Returns `BackendError::ReadOnly` if the connection is read-only.
    pub async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        // Enforce read-only mode
        if self.access_mode == AccessMode::ReadOnly {
            return Err(BackendError::ReadOnly);
        }

        match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(pool) => {
                self.execute_sqlite(pool, sql, params).await
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(client) => {
                self.execute_duckdb(client, sql, params).await
            }
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(pool) => {
                self.execute_postgres(pool, sql, params).await
            }
        }
    }

    /// Execute a batch of SQL statements.
    ///
    /// Returns `BackendError::ReadOnly` if the connection is read-only.
    ///
    /// **Note:** This uses naive semicolon splitting for SQLite/PostgreSQL.
    /// Avoid SQL with semicolons in string literals. DuckDB handles this correctly.
    pub async fn execute_batch(&self, sql: &str) -> Result<(), BackendError> {
        // Enforce read-only mode
        if self.access_mode == AccessMode::ReadOnly {
            return Err(BackendError::ReadOnly);
        }

        match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(pool) => {
                // Split by semicolon and execute each
                // WARNING: This breaks SQL with semicolons in string literals
                for stmt in sql.split(';').filter(|s| !s.trim().is_empty()) {
                    sqlx::query(stmt).execute(pool).await?;
                }
                Ok(())
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(client) => {
                let sql = sql.to_string();
                client.conn(move |conn| {
                    conn.execute_batch(&sql)?;
                    Ok(())
                }).await.map_err(BackendError::from)
            }
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(pool) => {
                // WARNING: This breaks SQL with semicolons in string literals
                for stmt in sql.split(';').filter(|s| !s.trim().is_empty()) {
                    sqlx::query(stmt).execute(pool).await?;
                }
                Ok(())
            }
        }
    }

    /// Query and return all rows.
    pub async fn query_all(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(pool) => {
                self.query_sqlite(pool, sql, params).await
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(client) => {
                self.query_duckdb(client, sql, params).await
            }
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(pool) => {
                self.query_postgres(pool, sql, params).await
            }
        }
    }

    /// Query and return the first row, if any.
    pub async fn query_optional(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, BackendError> {
        let rows = self.query_all(sql, params).await?;
        Ok(rows.into_iter().next())
    }

    /// Query and return exactly one row.
    pub async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<DbRow, BackendError> {
        self.query_optional(sql, params)
            .await?
            .ok_or_else(|| BackendError::Query("Expected one row, got none".to_string()))
    }

    /// Query and return a single scalar value.
    pub async fn query_scalar<T: FromDbValue>(&self, sql: &str, params: &[DbValue]) -> Result<T, BackendError> {
        let row = self.query_one(sql, params).await?;
        row.get(0)
    }

    // SQLite implementation
    #[cfg(feature = "sqlite")]
    async fn execute_sqlite(&self, pool: &sqlx::SqlitePool, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
            };
        }
        let result = query.execute(pool).await?;
        Ok(result.rows_affected())
    }

    #[cfg(feature = "sqlite")]
    async fn query_sqlite(&self, pool: &sqlx::SqlitePool, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        use sqlx::{Column, Row};

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
            };
        }

        let rows = query.fetch_all(pool).await?;
        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            let mut values = Vec::with_capacity(columns.len());

            for (i, col) in row.columns().iter().enumerate() {
                let value = match col.type_info().to_string().as_str() {
                    "INTEGER" | "INT" | "BIGINT" | "INT64" => {
                        let v: Option<i64> = row.try_get(i).ok();
                        v.map(DbValue::Integer).unwrap_or(DbValue::Null)
                    }
                    "REAL" | "FLOAT" | "DOUBLE" => {
                        let v: Option<f64> = row.try_get(i).ok();
                        v.map(DbValue::Real).unwrap_or(DbValue::Null)
                    }
                    "BLOB" => {
                        let v: Option<Vec<u8>> = row.try_get(i).ok();
                        v.map(DbValue::Blob).unwrap_or(DbValue::Null)
                    }
                    "BOOLEAN" | "BOOL" => {
                        let v: Option<bool> = row.try_get(i).ok();
                        v.map(DbValue::Boolean).unwrap_or(DbValue::Null)
                    }
                    "TEXT" | "VARCHAR" | "CHAR" => {
                        let v: Option<Option<String>> = row.try_get(i).ok();
                        match v {
                            Some(Some(text)) => DbValue::Text(text),
                            _ => DbValue::Null,
                        }
                    }
                    _ => {
                        let text_value: Option<Option<String>> = row.try_get(i).ok();
                        if let Some(Some(text)) = text_value {
                            DbValue::Text(text)
                        } else if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
                            v.map(DbValue::Integer).unwrap_or(DbValue::Null)
                        } else if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
                            v.map(DbValue::Real).unwrap_or(DbValue::Null)
                        } else if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
                            v.map(DbValue::Boolean).unwrap_or(DbValue::Null)
                        } else if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
                            v.map(DbValue::Blob).unwrap_or(DbValue::Null)
                        } else {
                            DbValue::Null
                        }
                    }
                };
                values.push(value);
            }

            result.push(DbRow::new(columns, values));
        }

        Ok(result)
    }

    // DuckDB implementation
    #[cfg(feature = "duckdb")]
    async fn execute_duckdb(&self, client: &async_duckdb::Client, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        let sql = sql.to_string();
        let params = params.to_vec();

        client.conn(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let duckdb_params = Self::to_duckdb_params(&params);
            let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params.iter()
                .map(|v| v as &dyn duckdb::ToSql)
                .collect();
            let rows = stmt.execute(param_refs.as_slice())?;
            Ok(rows as u64)
        }).await.map_err(BackendError::from)
    }

    #[cfg(feature = "duckdb")]
    async fn query_duckdb(&self, client: &async_duckdb::Client, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        let sql = sql.to_string();
        let params = params.to_vec();

        client.conn(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let duckdb_params = Self::to_duckdb_params(&params);
            let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params.iter()
                .map(|v| v as &dyn duckdb::ToSql)
                .collect();

            // Execute query first - column metadata requires query execution in DuckDB
            let mut rows_iter = stmt.query(param_refs.as_slice())?;

            // Get column info from the Rows via as_ref() to the underlying Statement
            let (column_count, columns) = if let Some(stmt_ref) = rows_iter.as_ref() {
                let count = stmt_ref.column_count();
                let cols: Vec<String> = (0..count)
                    .map(|i| stmt_ref.column_name(i).map(|s| s.to_string()).unwrap_or_else(|_| format!("col{}", i)))
                    .collect();
                (count, cols)
            } else {
                // No statement reference available - return empty result
                return Ok(Vec::new());
            };

            let mut result = Vec::new();

            while let Some(row) = rows_iter.next()? {
                let mut values = Vec::with_capacity(column_count);
                for i in 0..column_count {
                    let value = Self::duckdb_value_to_db_value(&row, i)?;
                    values.push(value);
                }
                result.push(DbRow::new(columns.clone(), values));
            }

            Ok(result)
        }).await.map_err(BackendError::from)
    }

    #[cfg(feature = "duckdb")]
    fn to_duckdb_params(params: &[DbValue]) -> Vec<duckdb::types::Value> {
        params.iter().map(|p| match p {
            DbValue::Null => duckdb::types::Value::Null,
            DbValue::Integer(v) => duckdb::types::Value::BigInt(*v),
            DbValue::Real(v) => duckdb::types::Value::Double(*v),
            DbValue::Text(v) => duckdb::types::Value::Text(v.clone()),
            DbValue::Blob(v) => duckdb::types::Value::Blob(v.clone()),
            DbValue::Boolean(v) => duckdb::types::Value::Boolean(*v),
        }).collect()
    }

    #[cfg(feature = "duckdb")]
    fn duckdb_value_to_db_value(row: &duckdb::Row, index: usize) -> Result<DbValue, duckdb::Error> {
        use duckdb::types::ValueRef;

        match row.get_ref(index)? {
            ValueRef::Null => Ok(DbValue::Null),
            ValueRef::Boolean(v) => Ok(DbValue::Boolean(v)),
            // Integer types
            ValueRef::TinyInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::SmallInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::Int(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::BigInt(v) => Ok(DbValue::Integer(v)),
            ValueRef::HugeInt(v) => Ok(DbValue::Integer(v as i64)), // Lossy for huge values
            ValueRef::UTinyInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::USmallInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::UInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::UBigInt(v) => Ok(DbValue::Integer(v as i64)), // Lossy for huge values
            // Floating point
            ValueRef::Float(v) => Ok(DbValue::Real(v as f64)),
            ValueRef::Double(v) => Ok(DbValue::Real(v)),
            // Text and binary
            ValueRef::Text(v) => Ok(DbValue::Text(String::from_utf8_lossy(v).to_string())),
            ValueRef::Blob(v) => Ok(DbValue::Blob(v.to_vec())),
            // Temporal types - convert to ISO 8601 strings for portability
            ValueRef::Timestamp(unit, v) => {
                // Convert to ISO 8601 timestamp string
                let micros = match unit {
                    duckdb::types::TimeUnit::Second => v * 1_000_000,
                    duckdb::types::TimeUnit::Millisecond => v * 1_000,
                    duckdb::types::TimeUnit::Microsecond => v,
                    duckdb::types::TimeUnit::Nanosecond => v / 1_000,
                };
                // Convert to seconds and format
                let secs = micros / 1_000_000;
                let nanos = ((micros % 1_000_000) * 1_000) as u32;
                if let Some(dt) = chrono::DateTime::from_timestamp(secs, nanos) {
                    Ok(DbValue::Text(dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()))
                } else {
                    Ok(DbValue::Integer(micros)) // Fallback to raw micros
                }
            }
            ValueRef::Date32(days) => {
                // Days since Unix epoch
                if let Some(date) = chrono::NaiveDate::from_num_days_from_ce_opt(719163 + days) {
                    Ok(DbValue::Text(date.format("%Y-%m-%d").to_string()))
                } else {
                    Ok(DbValue::Integer(days as i64))
                }
            }
            ValueRef::Time64(unit, v) => {
                // Time of day
                let micros = match unit {
                    duckdb::types::TimeUnit::Second => v * 1_000_000,
                    duckdb::types::TimeUnit::Millisecond => v * 1_000,
                    duckdb::types::TimeUnit::Microsecond => v,
                    duckdb::types::TimeUnit::Nanosecond => v / 1_000,
                };
                let secs = (micros / 1_000_000) as u32;
                let nanos = ((micros % 1_000_000) * 1_000) as u32;
                if let Some(time) = chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos) {
                    Ok(DbValue::Text(time.format("%H:%M:%S%.6f").to_string()))
                } else {
                    Ok(DbValue::Integer(micros))
                }
            }
            ValueRef::Interval { months, days, nanos } => {
                // Format as ISO 8601 duration (approximate)
                Ok(DbValue::Text(format!("P{}M{}DT{}N", months, days, nanos)))
            }
            // Enum values - use debug format (DuckDB uses Arrow DictionaryArray internally)
            ValueRef::Enum(_, _) => {
                // EnumType uses Arrow's DictionaryArray which is complex to extract
                // Use debug representation for now
                Ok(DbValue::Text(format!("{:?}", row.get_ref(index)?)))
            }
            // List/Array - convert to JSON-like string representation
            ValueRef::List(_, _) => {
                // For now, use debug representation
                // TODO: Proper list serialization if needed
                Ok(DbValue::Text(format!("{:?}", row.get_ref(index)?)))
            }
            // Catch-all for other types (Map, Struct, etc.)
            // Log a warning and return debug string rather than silently corrupt
            other => {
                tracing::warn!(
                    "DuckDB type {:?} at column {} mapped to debug string",
                    std::mem::discriminant(&other),
                    index
                );
                Ok(DbValue::Text(format!("{:?}", other)))
            }
        }
    }

    // PostgreSQL implementation
    #[cfg(feature = "postgres")]
    async fn execute_postgres(&self, pool: &sqlx::PgPool, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
            };
        }
        let result = query.execute(pool).await?;
        Ok(result.rows_affected())
    }

    #[cfg(feature = "postgres")]
    async fn query_postgres(&self, pool: &sqlx::PgPool, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        use sqlx::Row;

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
            };
        }

        let rows = query.fetch_all(pool).await?;
        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            let mut values = Vec::with_capacity(columns.len());

            for i in 0..columns.len() {
                // Try each type in order
                let value = if let Ok(v) = row.try_get::<i64, _>(i) {
                    DbValue::Integer(v)
                } else if let Ok(v) = row.try_get::<f64, _>(i) {
                    DbValue::Real(v)
                } else if let Ok(v) = row.try_get::<bool, _>(i) {
                    DbValue::Boolean(v)
                } else if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
                    DbValue::Blob(v)
                } else if let Ok(v) = row.try_get::<String, _>(i) {
                    DbValue::Text(v)
                } else {
                    DbValue::Null
                };
                values.push(value);
            }

            result.push(DbRow::new(columns, values));
        }

        Ok(result)
    }

    /// Get the underlying SQLite pool (for legacy code during migration).
    #[cfg(feature = "sqlite")]
    pub fn as_sqlite_pool(&self) -> Option<&sqlx::SqlitePool> {
        match &self.inner {
            DbConnectionInner::Sqlite(pool) => Some(pool),
            #[cfg(feature = "duckdb")]
            _ => None,
            #[cfg(feature = "postgres")]
            _ => None,
        }
    }

    /// Get the underlying DuckDB client (for advanced queries).
    #[cfg(feature = "duckdb")]
    pub fn as_duckdb_client(&self) -> Option<&async_duckdb::Client> {
        match &self.inner {
            DbConnectionInner::DuckDb(client) => Some(client),
            #[cfg(feature = "sqlite")]
            _ => None,
            #[cfg(feature = "postgres")]
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "sqlite")]
    async fn test_sqlite_backend() {
        let conn = DbConnection::open_sqlite_memory().await.unwrap();

        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)").await.unwrap();
        conn.execute("INSERT INTO test (id, name) VALUES (?, ?)", &[1.into(), "Alice".into()]).await.unwrap();
        conn.execute("INSERT INTO test (id, name) VALUES (?, ?)", &[2.into(), "Bob".into()]).await.unwrap();

        let rows = conn.query_all("SELECT id, name FROM test ORDER BY id", &[]).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 1);
        assert_eq!(rows[0].get::<String>(1).unwrap(), "Alice");
        assert_eq!(rows[1].get::<i64>(0).unwrap(), 2);
        assert_eq!(rows[1].get::<String>(1).unwrap(), "Bob");
    }

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_backend() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();

        // Use ? placeholders (same as SQLite for consistency)
        conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR)").await.unwrap();
        conn.execute("INSERT INTO test (id, name) VALUES (?, ?)", &[1.into(), "Alice".into()]).await.unwrap();
        conn.execute("INSERT INTO test (id, name) VALUES (?, ?)", &[2.into(), "Bob".into()]).await.unwrap();

        let rows = conn.query_all("SELECT id, name FROM test ORDER BY id", &[]).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<i64>(0).unwrap(), 1);
        assert_eq!(rows[0].get::<String>(1).unwrap(), "Alice");
    }

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_glob() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();

        // Test that glob function exists (returns table function result)
        // Note: glob() returns a table, so we query it with FROM
        let result = conn.query_all("SELECT * FROM glob('/nonexistent/*.csv') LIMIT 1", &[]).await;
        // Should succeed even if no files match (empty result)
        assert!(result.is_ok(), "glob query failed: {:?}", result.err());
    }

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_read_only_enforcement() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // First create the database
        {
            let conn = DbConnection::open_duckdb(&db_path).await.unwrap();
            conn.execute_batch("CREATE TABLE test (id INTEGER)").await.unwrap();
            // Connection drops here, releasing lock
        }

        // Open read-only and try to write
        let conn = DbConnection::open_duckdb_readonly(&db_path).await.unwrap();
        let result = conn.execute("INSERT INTO test (id) VALUES (?)", &[1.into()]).await;
        assert!(matches!(result, Err(BackendError::ReadOnly)));
    }
}

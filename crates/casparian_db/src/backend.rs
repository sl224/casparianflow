//! Database backend abstraction layer.
//!
//! Provides a unified interface for different database backends:
//! - SQLite (via sqlx) - row-oriented, OLTP (actor boundary)
//! - DuckDB (via duckdb) - columnar, OLAP (single-actor async boundary)
//! - PostgreSQL (via sqlx) - enterprise

use std::path::Path;
#[cfg(any(feature = "duckdb", feature = "sqlite"))]
use std::thread;
use thiserror::Error;
use tracing::info;

#[cfg(feature = "sqlite")]
use sqlx::Connection;
#[cfg(feature = "duckdb")]
use std::sync::Arc;
#[cfg(any(feature = "duckdb", feature = "sqlite"))]
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "sqlite")]
use tokio::runtime::Builder as RuntimeBuilder;
use std::any::Any;

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
    Timestamp(chrono::DateTime<chrono::Utc>),
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

impl From<chrono::DateTime<chrono::Utc>> for DbValue {
    fn from(v: chrono::DateTime<chrono::Utc>) -> Self {
        DbValue::Timestamp(v)
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

#[cfg(feature = "duckdb")]
const DUCKDB_ACTOR_QUEUE: usize = 1024;

#[cfg(feature = "sqlite")]
const SQLITE_ACTOR_QUEUE: usize = 1024;

#[cfg(feature = "duckdb")]
#[derive(Clone)]
struct DuckDbActorHandle {
    sender: mpsc::Sender<DuckDbRequest>,
}

#[cfg(feature = "sqlite")]
#[derive(Clone)]
struct SqliteActorHandle {
    sender: mpsc::Sender<SqliteRequest>,
}

#[cfg(feature = "duckdb")]
enum DuckDbRequest {
    Execute {
        sql: String,
        params: Vec<DbValue>,
        resp: oneshot::Sender<Result<u64, BackendError>>,
    },
    ExecuteBatch {
        sql: String,
        resp: oneshot::Sender<Result<(), BackendError>>,
    },
    QueryAll {
        sql: String,
        params: Vec<DbValue>,
        resp: oneshot::Sender<Result<Vec<DbRow>, BackendError>>,
    },
    Op {
        op: Box<dyn DuckDbOp>,
        resp: oneshot::Sender<Result<(), BackendError>>,
    },
    Transaction {
        op: Box<dyn DuckDbTxOp>,
        resp: oneshot::Sender<Result<Box<dyn Any + Send>, BackendError>>,
    },
}

#[cfg(feature = "sqlite")]
enum SqliteRequest {
    Execute {
        sql: String,
        params: Vec<DbValue>,
        resp: oneshot::Sender<Result<u64, BackendError>>,
    },
    ExecuteBatch {
        sql: String,
        resp: oneshot::Sender<Result<(), BackendError>>,
    },
    QueryAll {
        sql: String,
        params: Vec<DbValue>,
        resp: oneshot::Sender<Result<Vec<DbRow>, BackendError>>,
    },
    Transaction {
        op: Box<dyn SqliteTxOp>,
        resp: oneshot::Sender<Result<Box<dyn Any + Send>, BackendError>>,
    },
}

#[cfg(feature = "duckdb")]
trait DuckDbOp: Send {
    fn run(self: Box<Self>, conn: &duckdb::Connection) -> Result<(), BackendError>;
}

#[cfg(feature = "duckdb")]
impl<F> DuckDbOp for F
where
    F: FnOnce(&duckdb::Connection) -> Result<(), BackendError> + Send + 'static,
{
    fn run(self: Box<Self>, conn: &duckdb::Connection) -> Result<(), BackendError> {
        (*self)(conn)
    }
}

trait DuckDbTxOp: Send {
    fn run<'a>(self: Box<Self>, tx: &'a mut DbTransaction<'a>) -> Result<Box<dyn Any + Send>, BackendError>;
}

impl<F, T> DuckDbTxOp for F
where
    F: for<'a> FnOnce(&'a mut DbTransaction<'a>) -> Result<T, BackendError> + Send + 'static,
    T: Send + 'static,
{
    fn run<'a>(self: Box<Self>, tx: &'a mut DbTransaction<'a>) -> Result<Box<dyn Any + Send>, BackendError> {
        (*self)(tx).map(|value| Box::new(value) as Box<dyn Any + Send>)
    }
}

#[cfg(feature = "sqlite")]
trait SqliteTxOp: Send {
    fn run<'a>(self: Box<Self>, tx: &'a mut DbTransaction<'a>) -> Result<Box<dyn Any + Send>, BackendError>;
}

#[cfg(feature = "sqlite")]
impl<F, T> SqliteTxOp for F
where
    F: for<'a> FnOnce(&'a mut DbTransaction<'a>) -> Result<T, BackendError> + Send + 'static,
    T: Send + 'static,
{
    fn run<'a>(self: Box<Self>, tx: &'a mut DbTransaction<'a>) -> Result<Box<dyn Any + Send>, BackendError> {
        (*self)(tx).map(|value| Box::new(value) as Box<dyn Any + Send>)
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

impl FromDbValue for chrono::DateTime<chrono::Utc> {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Timestamp(v) => Ok(v.clone()),
            DbValue::Text(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| BackendError::TypeConversion(format!("Invalid timestamp: {}", e))),
            _ => Err(BackendError::TypeConversion("Expected timestamp".to_string())),
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
    Sqlite(SqliteActorHandle),

    #[cfg(feature = "duckdb")]
    DuckDb(DuckDbActorHandle),

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
    /// Open a database connection from a URL string.
    ///
    /// Supported schemes: sqlite:, duckdb:, postgres://, postgresql://
    pub async fn open_from_url(url: &str) -> Result<Self, BackendError> {
        let db_type = crate::DatabaseType::from_url(url)
            .ok_or_else(|| BackendError::Database(format!("Unsupported database URL: {}", url)))?;

        match db_type {
            #[cfg(feature = "sqlite")]
            crate::DatabaseType::Sqlite => {
                let path = strip_url_prefix(url, "sqlite:")
                    .ok_or_else(|| BackendError::Database(format!("Invalid sqlite URL: {}", url)))?;
                Self::open_sqlite(Path::new(&path)).await
            }
            #[cfg(feature = "duckdb")]
            crate::DatabaseType::DuckDb => {
                let path = strip_url_prefix(url, "duckdb:")
                    .ok_or_else(|| BackendError::Database(format!("Invalid duckdb URL: {}", url)))?;
                Self::open_duckdb(Path::new(&path)).await
            }
            #[cfg(feature = "postgres")]
            crate::DatabaseType::Postgres => Self::open_postgres(url).await,
        }
    }
    /// Open a SQLite database.
    #[cfg(feature = "sqlite")]
    pub async fn open_sqlite(path: &Path) -> Result<Self, BackendError> {
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let actor = Self::spawn_sqlite_actor(url).await?;
        info!("Opened SQLite database: {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::Sqlite(actor),
            access_mode: AccessMode::ReadWrite,
            #[cfg(feature = "duckdb")]
            _lock_guard: None,
        })
    }

    /// Open an in-memory SQLite database (for testing).
    #[cfg(feature = "sqlite")]
    pub async fn open_sqlite_memory() -> Result<Self, BackendError> {
        let actor = Self::spawn_sqlite_actor("sqlite::memory:".to_string()).await?;
        info!("Opened in-memory SQLite database");
        Ok(Self {
            inner: DbConnectionInner::Sqlite(actor),
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

        let path_buf = path.to_path_buf();
        let actor = Self::spawn_duckdb_actor(move || {
            duckdb::Connection::open(path_buf).map_err(BackendError::from)
        })
        .await?;

        info!("Opened DuckDB database with exclusive lock: {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::DuckDb(actor),
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

        let path_buf = path.to_path_buf();
        let actor = Self::spawn_duckdb_actor(move || {
            let config = Config::default()
                .access_mode(DuckAccessMode::ReadOnly)
                .map_err(BackendError::from)?;
            duckdb::Connection::open_with_flags(path_buf, config).map_err(BackendError::from)
        })
        .await?;

        info!("Opened DuckDB database (read-only): {}", path.display());
        Ok(Self {
            inner: DbConnectionInner::DuckDb(actor),
            access_mode: AccessMode::ReadOnly,
            _lock_guard: None, // No lock needed for read-only
        })
    }

    /// Open an in-memory DuckDB database (for testing).
    ///
    /// In-memory databases don't require locking since they're process-local.
    #[cfg(feature = "duckdb")]
    pub async fn open_duckdb_memory() -> Result<Self, BackendError> {
        let actor = Self::spawn_duckdb_actor(|| {
            duckdb::Connection::open_in_memory().map_err(BackendError::from)
        })
        .await?;

        info!("Opened in-memory DuckDB database");
        Ok(Self {
            inner: DbConnectionInner::DuckDb(actor),
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
            DbConnectionInner::Sqlite(handle) => {
                self.execute_sqlite(handle, sql, params).await
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(handle) => {
                self.execute_duckdb(handle, sql, params).await
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
            DbConnectionInner::Sqlite(handle) => {
                self.execute_sqlite_batch(handle, sql).await
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(handle) => {
                self.execute_duckdb_batch(handle, sql).await
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
            DbConnectionInner::Sqlite(handle) => {
                self.query_sqlite(handle, sql, params).await
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(handle) => {
                self.query_duckdb(handle, sql, params).await
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

    /// Execute a DuckDB-specific operation on the actor thread.
    #[cfg(feature = "duckdb")]
    pub async fn execute_duckdb_op<F>(&self, op: F) -> Result<(), BackendError>
    where
        F: FnOnce(&duckdb::Connection) -> Result<(), BackendError> + Send + 'static,
    {
        let handle = match &self.inner {
            DbConnectionInner::DuckDb(handle) => handle.clone(),
            #[cfg(any(feature = "sqlite", feature = "postgres"))]
            _ => {
                return Err(BackendError::NotAvailable(
                    "DuckDB backend not available".to_string(),
                ));
            }
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        let request = DuckDbRequest::Op {
            op: Box::new(op),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("DuckDB actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("DuckDB actor dropped response".to_string()))?
    }

    // SQLite implementation
    #[cfg(feature = "sqlite")]
    async fn execute_sqlite(&self, handle: &SqliteActorHandle, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = SqliteRequest::Execute {
            sql: sql.to_string(),
            params: params.to_vec(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("SQLite actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("SQLite actor dropped response".to_string()))?
    }

    #[cfg(feature = "sqlite")]
    async fn execute_sqlite_batch(&self, handle: &SqliteActorHandle, sql: &str) -> Result<(), BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = SqliteRequest::ExecuteBatch {
            sql: sql.to_string(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("SQLite actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("SQLite actor dropped response".to_string()))?
    }

    #[cfg(feature = "sqlite")]
    async fn query_sqlite(&self, handle: &SqliteActorHandle, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = SqliteRequest::QueryAll {
            sql: sql.to_string(),
            params: params.to_vec(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("SQLite actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("SQLite actor dropped response".to_string()))?
    }

    #[cfg(feature = "sqlite")]
    async fn spawn_sqlite_actor(url: String) -> Result<SqliteActorHandle, BackendError> {
        let (sender, receiver) = mpsc::channel(SQLITE_ACTOR_QUEUE);
        let (init_tx, init_rx) = oneshot::channel();

        thread::spawn(move || {
            let runtime = match RuntimeBuilder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(err) => {
                    let _ = init_tx.send(Err(BackendError::Database(format!(
                        "SQLite runtime init failed: {}",
                        err
                    ))));
                    return;
                }
            };

            let mut conn = match runtime.block_on(sqlx::SqliteConnection::connect(&url)) {
                Ok(conn) => conn,
                Err(err) => {
                    let _ = init_tx.send(Err(BackendError::from(err)));
                    return;
                }
            };

            if let Err(err) = runtime.block_on(Self::apply_sqlite_optimizations_on_conn(&mut conn)) {
                let _ = init_tx.send(Err(err));
                return;
            }

            let _ = init_tx.send(Ok(()));
            Self::run_sqlite_actor(conn, runtime, receiver);
        });

        match init_rx.await {
            Ok(Ok(())) => Ok(SqliteActorHandle { sender }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(BackendError::Database(
                "SQLite actor failed to start".to_string(),
            )),
        }
    }

    #[cfg(feature = "sqlite")]
    fn run_sqlite_actor(
        mut conn: sqlx::SqliteConnection,
        runtime: tokio::runtime::Runtime,
        mut receiver: mpsc::Receiver<SqliteRequest>,
    ) {
        while let Some(request) = receiver.blocking_recv() {
            match request {
                SqliteRequest::Execute { sql, params, resp } => {
                    let result = runtime.block_on(Self::execute_sqlite_on_conn(&mut conn, &sql, &params));
                    let _ = resp.send(result);
                }
                SqliteRequest::ExecuteBatch { sql, resp } => {
                    let result = runtime.block_on(Self::execute_sqlite_batch_on_conn(&mut conn, &sql));
                    let _ = resp.send(result);
                }
                SqliteRequest::QueryAll { sql, params, resp } => {
                    let result = runtime.block_on(Self::query_sqlite_on_conn(&mut conn, &sql, &params));
                    let _ = resp.send(result);
                }
                SqliteRequest::Transaction { op, resp } => {
                    let result = Self::run_sqlite_transaction(&mut conn, &runtime, op);
                    let _ = resp.send(result);
                }
            }
        }
    }

    #[cfg(feature = "sqlite")]
    fn run_sqlite_transaction(
        conn: &mut sqlx::SqliteConnection,
        runtime: &tokio::runtime::Runtime,
        op: Box<dyn SqliteTxOp>,
    ) -> Result<Box<dyn Any + Send>, BackendError> {
        runtime.block_on(async {
            sqlx::query("BEGIN").execute(&mut *conn).await?;
            Ok::<(), BackendError>(())
        })?;

        let mut tx = DbTransaction::sqlite(conn, runtime);
        let result = op.run(&mut tx);

        match result {
            Ok(value) => {
                runtime.block_on(async {
                    sqlx::query("COMMIT").execute(conn).await?;
                    Ok::<(), BackendError>(())
                })?;
                Ok(value)
            }
            Err(err) => {
                let _ = runtime.block_on(async { sqlx::query("ROLLBACK").execute(conn).await });
                Err(err)
            }
        }
    }

    #[cfg(feature = "sqlite")]
    async fn execute_sqlite_on_conn(
        conn: &mut sqlx::SqliteConnection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<u64, BackendError> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
                DbValue::Timestamp(v) => query.bind(v.clone()),
            };
        }
        let result = query.execute(conn).await?;
        Ok(result.rows_affected())
    }

    #[cfg(feature = "sqlite")]
    async fn execute_sqlite_batch_on_conn(
        conn: &mut sqlx::SqliteConnection,
        sql: &str,
    ) -> Result<(), BackendError> {
        // WARNING: This breaks SQL with semicolons in string literals
        for stmt in sql.split(';').filter(|s| !s.trim().is_empty()) {
            sqlx::query(stmt).execute(&mut *conn).await?;
        }
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    async fn query_sqlite_on_conn(
        conn: &mut sqlx::SqliteConnection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<Vec<DbRow>, BackendError> {
        use sqlx::{Column, Row, ValueRef};

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                DbValue::Null => query.bind(Option::<String>::None),
                DbValue::Integer(v) => query.bind(*v),
                DbValue::Real(v) => query.bind(*v),
                DbValue::Text(v) => query.bind(v.as_str()),
                DbValue::Blob(v) => query.bind(v.as_slice()),
                DbValue::Boolean(v) => query.bind(*v),
                DbValue::Timestamp(v) => query.bind(v.clone()),
            };
        }

        let rows = query.fetch_all(conn).await?;
        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
            let mut values = Vec::with_capacity(columns.len());

            for (i, col) in row.columns().iter().enumerate() {
                let type_info = col.type_info().to_string();
                let type_upper = type_info.to_ascii_uppercase();
                let value = match type_upper.as_str() {
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
                    "DATETIME" | "TIMESTAMP" => {
                        let v: Option<chrono::DateTime<chrono::Utc>> = row.try_get(i).ok();
                        v.map(DbValue::Timestamp).unwrap_or(DbValue::Null)
                    }
                    "TEXT" | "VARCHAR" | "CHAR" => {
                        let v: Option<Option<String>> = row.try_get(i).ok();
                        match v {
                            Some(Some(text)) => DbValue::Text(text),
                            _ => DbValue::Null,
                        }
                    }
                    other if other.contains("TEXT")
                        || other.contains("CHAR")
                        || other.contains("CLOB") =>
                    {
                        let v: Option<Option<String>> = row.try_get(i).ok();
                        match v {
                            Some(Some(text)) => DbValue::Text(text),
                            _ => DbValue::Null,
                        }
                    }
                    _ => {
                        if let Ok(raw) = row.try_get_raw(i) {
                            if raw.is_null() {
                                DbValue::Null
                            } else {
                                let raw_type = raw.type_info().to_string();
                                let raw_upper = raw_type.to_ascii_uppercase();
                                match raw_upper.as_str() {
                                    "INTEGER" => {
                                        let v: Option<i64> = row.try_get(i).ok();
                                        v.map(DbValue::Integer).unwrap_or(DbValue::Null)
                                    }
                                    "REAL" => {
                                        let v: Option<f64> = row.try_get(i).ok();
                                        v.map(DbValue::Real).unwrap_or(DbValue::Null)
                                    }
                                    "BLOB" => {
                                        let v: Option<Vec<u8>> = row.try_get(i).ok();
                                        v.map(DbValue::Blob).unwrap_or(DbValue::Null)
                                    }
                                    "TEXT" => {
                                        let v: Option<Option<String>> = row.try_get(i).ok();
                                        match v {
                                            Some(Some(text)) => DbValue::Text(text),
                                            _ => DbValue::Null,
                                        }
                                    }
                                    _ => {
                                        if let Ok(v) = row.try_get::<Option<Option<String>>, _>(i) {
                                            match v {
                                                Some(Some(text)) => DbValue::Text(text),
                                                _ => DbValue::Null,
                                            }
                                        } else if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) {
                                            v.map(DbValue::Timestamp).unwrap_or(DbValue::Null)
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
                                }
                            }
                        } else if let Ok(v) = row.try_get::<Option<Option<String>>, _>(i) {
                            match v {
                                Some(Some(text)) => DbValue::Text(text),
                                _ => DbValue::Null,
                            }
                        } else if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) {
                            v.map(DbValue::Timestamp).unwrap_or(DbValue::Null)
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

    #[cfg(feature = "sqlite")]
    async fn apply_sqlite_optimizations_on_conn(
        conn: &mut sqlx::SqliteConnection,
    ) -> Result<(), BackendError> {
        sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
        sqlx::query("PRAGMA synchronous=NORMAL").execute(&mut *conn).await?;
        Ok(())
    }

    // DuckDB implementation (actor boundary)
    #[cfg(feature = "duckdb")]
    async fn execute_duckdb(&self, handle: &DuckDbActorHandle, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = DuckDbRequest::Execute {
            sql: sql.to_string(),
            params: params.to_vec(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("DuckDB actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("DuckDB actor dropped response".to_string()))?
    }

    #[cfg(feature = "duckdb")]
    async fn execute_duckdb_batch(&self, handle: &DuckDbActorHandle, sql: &str) -> Result<(), BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = DuckDbRequest::ExecuteBatch {
            sql: sql.to_string(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("DuckDB actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("DuckDB actor dropped response".to_string()))?
    }

    #[cfg(feature = "duckdb")]
    async fn query_duckdb(&self, handle: &DuckDbActorHandle, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let request = DuckDbRequest::QueryAll {
            sql: sql.to_string(),
            params: params.to_vec(),
            resp: resp_tx,
        };

        handle
            .sender
            .send(request)
            .await
            .map_err(|_| BackendError::Database("DuckDB actor closed".to_string()))?;

        resp_rx
            .await
            .map_err(|_| BackendError::Database("DuckDB actor dropped response".to_string()))?
    }

    #[cfg(feature = "duckdb")]
    async fn spawn_duckdb_actor<F>(open_fn: F) -> Result<DuckDbActorHandle, BackendError>
    where
        F: FnOnce() -> Result<duckdb::Connection, BackendError> + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel(DUCKDB_ACTOR_QUEUE);
        let (init_tx, init_rx) = oneshot::channel();

        thread::spawn(move || {
            let conn = open_fn();
            match conn {
                Ok(conn) => {
                    let _ = init_tx.send(Ok(()));
                    Self::run_duckdb_actor(conn, receiver);
                }
                Err(err) => {
                    let _ = init_tx.send(Err(err));
                }
            }
        });

        match init_rx.await {
            Ok(Ok(())) => Ok(DuckDbActorHandle { sender }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(BackendError::Database(
                "DuckDB actor failed to start".to_string(),
            )),
        }
    }

    #[cfg(feature = "duckdb")]
    fn run_duckdb_actor(conn: duckdb::Connection, mut receiver: mpsc::Receiver<DuckDbRequest>) {
        while let Some(request) = receiver.blocking_recv() {
            match request {
                DuckDbRequest::Execute { sql, params, resp } => {
                    let result = Self::execute_duckdb_on_conn(&conn, &sql, &params);
                    let _ = resp.send(result);
                }
                DuckDbRequest::ExecuteBatch { sql, resp } => {
                    let result = Self::execute_duckdb_batch_on_conn(&conn, &sql);
                    let _ = resp.send(result);
                }
                DuckDbRequest::QueryAll { sql, params, resp } => {
                    let result = Self::query_duckdb_on_conn(&conn, &sql, &params);
                    let _ = resp.send(result);
                }
                DuckDbRequest::Op { op, resp } => {
                    let result = op.run(&conn);
                    let _ = resp.send(result);
                }
                DuckDbRequest::Transaction { op, resp } => {
                    let result = Self::run_duckdb_transaction(&conn, op);
                    let _ = resp.send(result);
                }
            }
        }
    }

    #[cfg(feature = "duckdb")]
    fn run_duckdb_transaction(
        conn: &duckdb::Connection,
        op: Box<dyn DuckDbTxOp>,
    ) -> Result<Box<dyn Any + Send>, BackendError> {
        conn.execute_batch("BEGIN")?;
        let mut tx = DbTransaction::duckdb(conn);
        let result = op.run(&mut tx);

        match result {
            Ok(value) => {
                conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    #[cfg(feature = "duckdb")]
    fn execute_duckdb_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<u64, BackendError> {
        let mut stmt = conn.prepare(sql)?;
        let duckdb_params = Self::to_duckdb_params(params);
        let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params
            .iter()
            .map(|v| v as &dyn duckdb::ToSql)
            .collect();
        let rows = stmt.execute(param_refs.as_slice())?;
        Ok(rows as u64)
    }

    #[cfg(feature = "duckdb")]
    fn execute_duckdb_batch_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
    ) -> Result<(), BackendError> {
        conn.execute_batch(sql)?;
        Ok(())
    }

    #[cfg(feature = "duckdb")]
    fn query_duckdb_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<Vec<DbRow>, BackendError> {
        let mut stmt = conn.prepare(sql)?;
        let duckdb_params = Self::to_duckdb_params(params);
        let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params
            .iter()
            .map(|v| v as &dyn duckdb::ToSql)
            .collect();

        // Execute query first - column metadata requires query execution in DuckDB
        let mut rows_iter = stmt.query(param_refs.as_slice())?;

        // Get column info from the Rows via as_ref() to the underlying Statement
        let (column_count, columns) = if let Some(stmt_ref) = rows_iter.as_ref() {
            let count = stmt_ref.column_count();
            let cols: Vec<String> = (0..count)
                .map(|i| {
                    stmt_ref
                        .column_name(i)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("col{}", i))
                })
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
            DbValue::Timestamp(v) => {
                let micros = v.timestamp_micros();
                duckdb::types::Value::Timestamp(duckdb::types::TimeUnit::Microsecond, micros)
            }
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
            // Temporal types
            ValueRef::Timestamp(unit, v) => {
                let micros = match unit {
                    duckdb::types::TimeUnit::Second => v * 1_000_000,
                    duckdb::types::TimeUnit::Millisecond => v * 1_000,
                    duckdb::types::TimeUnit::Microsecond => v,
                    duckdb::types::TimeUnit::Nanosecond => v / 1_000,
                };
                let secs = micros / 1_000_000;
                let nanos = ((micros % 1_000_000) * 1_000) as u32;
                if let Some(dt) = chrono::DateTime::from_timestamp(secs, nanos) {
                    Ok(DbValue::Timestamp(dt))
                } else {
                    Ok(DbValue::Integer(micros))
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
                DbValue::Timestamp(v) => query.bind(v.clone()),
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
                DbValue::Timestamp(v) => query.bind(v.clone()),
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
                } else if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) {
                    DbValue::Timestamp(v)
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
    pub async fn transaction<T, F>(&self, op: F) -> Result<T, BackendError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut DbTransaction<'a>) -> Result<T, BackendError> + Send + 'static,
    {
        let mut op = Some(op);
        match &self.inner {
            #[cfg(feature = "sqlite")]
            DbConnectionInner::Sqlite(handle) => {
                let (resp_tx, resp_rx) = oneshot::channel();
                let request = SqliteRequest::Transaction {
                    op: Box::new(op.take().expect("transaction op missing")),
                    resp: resp_tx,
                };

                handle
                    .sender
                    .send(request)
                    .await
                    .map_err(|_| BackendError::Database("SQLite actor closed".to_string()))?;

                let boxed = resp_rx
                    .await
                    .map_err(|_| BackendError::Database("SQLite actor dropped response".to_string()))??;

                boxed
                    .downcast::<T>()
                    .map(|value| *value)
                    .map_err(|_| BackendError::TypeConversion("SQLite transaction result type mismatch".to_string()))
            }
            #[cfg(feature = "duckdb")]
            DbConnectionInner::DuckDb(handle) => {
                let (resp_tx, resp_rx) = oneshot::channel();
                let request = DuckDbRequest::Transaction {
                    op: Box::new(op.take().expect("transaction op missing")),
                    resp: resp_tx,
                };

                handle
                    .sender
                    .send(request)
                    .await
                    .map_err(|_| BackendError::Database("DuckDB actor closed".to_string()))?;

                let boxed = resp_rx
                    .await
                    .map_err(|_| BackendError::Database("DuckDB actor dropped response".to_string()))??;

                boxed
                    .downcast::<T>()
                    .map(|value| *value)
                    .map_err(|_| BackendError::TypeConversion("DuckDB transaction result type mismatch".to_string()))
            }
            #[cfg(feature = "postgres")]
            DbConnectionInner::Postgres(_) => Err(BackendError::NotAvailable(
                "Postgres transactions via DbConnection are not supported yet".to_string(),
            )),
        }
    }
}

/// Transaction wrapper executed within the actor thread.
pub struct DbTransaction<'a> {
    inner: DbTransactionInner<'a>,
}

enum DbTransactionInner<'a> {
    #[cfg(feature = "sqlite")]
    Sqlite {
        conn: &'a mut sqlx::SqliteConnection,
        runtime: &'a tokio::runtime::Runtime,
    },
    #[cfg(feature = "duckdb")]
    DuckDb {
        conn: &'a duckdb::Connection,
    },
}

impl<'a> DbTransaction<'a> {
    #[cfg(feature = "sqlite")]
    fn sqlite(conn: &'a mut sqlx::SqliteConnection, runtime: &'a tokio::runtime::Runtime) -> Self {
        Self {
            inner: DbTransactionInner::Sqlite { conn, runtime },
        }
    }

    #[cfg(feature = "duckdb")]
    fn duckdb(conn: &'a duckdb::Connection) -> Self {
        Self {
            inner: DbTransactionInner::DuckDb { conn },
        }
    }

    pub fn execute(&mut self, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        match &mut self.inner {
            #[cfg(feature = "sqlite")]
            DbTransactionInner::Sqlite { conn, runtime } => {
                runtime.block_on(DbConnection::execute_sqlite_on_conn(conn, sql, params))
            }
            #[cfg(feature = "duckdb")]
            DbTransactionInner::DuckDb { conn } => {
                DbConnection::execute_duckdb_on_conn(conn, sql, params)
            }
        }
    }

    pub fn execute_batch(&mut self, sql: &str) -> Result<(), BackendError> {
        match &mut self.inner {
            #[cfg(feature = "sqlite")]
            DbTransactionInner::Sqlite { conn, runtime } => {
                runtime.block_on(DbConnection::execute_sqlite_batch_on_conn(conn, sql))
            }
            #[cfg(feature = "duckdb")]
            DbTransactionInner::DuckDb { conn } => {
                DbConnection::execute_duckdb_batch_on_conn(conn, sql)
            }
        }
    }

    pub fn query_all(&mut self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        match &mut self.inner {
            #[cfg(feature = "sqlite")]
            DbTransactionInner::Sqlite { conn, runtime } => {
                runtime.block_on(DbConnection::query_sqlite_on_conn(conn, sql, params))
            }
            #[cfg(feature = "duckdb")]
            DbTransactionInner::DuckDb { conn } => {
                DbConnection::query_duckdb_on_conn(conn, sql, params)
            }
        }
    }

    pub fn query_optional(&mut self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, BackendError> {
        let rows = self.query_all(sql, params)?;
        Ok(rows.into_iter().next())
    }

    pub fn query_one(&mut self, sql: &str, params: &[DbValue]) -> Result<DbRow, BackendError> {
        self.query_optional(sql, params)?
            .ok_or_else(|| BackendError::Query("Expected one row, got none".to_string()))
    }

    pub fn query_scalar<T: FromDbValue>(&mut self, sql: &str, params: &[DbValue]) -> Result<T, BackendError> {
        let row = self.query_one(sql, params)?;
        row.get(0)
    }
}

fn strip_url_prefix(url: &str, prefix: &str) -> Option<String> {
    if !url.starts_with(prefix) {
        return None;
    }
    let mut path = &url[prefix.len()..];
    if path.starts_with("//") {
        path = &path[2..];
    }
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::oneshot;

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
    #[cfg(feature = "sqlite")]
    async fn test_sqlite_actor_backpressure_on_full_queue() {
        let handle = DbConnection::spawn_sqlite_actor("sqlite::memory:".to_string())
            .await
            .unwrap();

        struct SleepTx {
            started: Option<oneshot::Sender<()>>,
        }

        impl SqliteTxOp for SleepTx {
            fn run<'a>(
                self: Box<Self>,
                _tx: &'a mut DbTransaction<'a>,
            ) -> Result<Box<dyn Any + Send>, BackendError> {
                if let Some(started) = self.started {
                    let _ = started.send(());
                }
                std::thread::sleep(Duration::from_millis(200));
                Ok(Box::new(()))
            }
        }

        let (started_tx, started_rx) = oneshot::channel();
        let (resp_tx, _resp_rx) = oneshot::channel();
        let request = SqliteRequest::Transaction {
            op: Box::new(SleepTx {
                started: Some(started_tx),
            }),
            resp: resp_tx,
        };

        handle.sender.send(request).await.unwrap();
        let _ = started_rx.await;

        let mut blocked = false;
        for _ in 0..(SQLITE_ACTOR_QUEUE + 10) {
            let (resp_tx, _resp_rx) = oneshot::channel();
            let request = SqliteRequest::Execute {
                sql: "SELECT 1".to_string(),
                params: Vec::new(),
                resp: resp_tx,
            };

            let send = handle.sender.send(request);
            if tokio::time::timeout(Duration::from_millis(10), send)
                .await
                .is_err()
            {
                blocked = true;
                break;
            }
        }

        assert!(blocked, "expected backpressure when queue is full");
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

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_actor_concurrent_writes() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();
        conn.execute_batch("CREATE TABLE test (id INTEGER)").await.unwrap();

        let mut tasks = Vec::new();
        for i in 0..50i64 {
            let conn = conn.clone();
            tasks.push(tokio::spawn(async move {
                conn.execute("INSERT INTO test (id) VALUES (?)", &[i.into()])
                    .await
            }));
        }

        for task in tasks {
            task.await.unwrap().unwrap();
        }

        let count: i64 = conn
            .query_scalar("SELECT COUNT(*) FROM test", &[])
            .await
            .unwrap();
        assert_eq!(count, 50);
    }

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_actor_op_allows_advanced_usage() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();

        conn.execute_duckdb_op(|db| {
            db.execute_batch("CREATE TABLE test (id INTEGER)")?;
            let mut appender = db.appender("test")?;
            appender.append_row(duckdb::params![1])?;
            appender.flush()?;
            Ok(())
        })
        .await
        .unwrap();

        let count: i64 = conn
            .query_scalar("SELECT COUNT(*) FROM test", &[])
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    #[cfg(feature = "duckdb")]
    async fn test_duckdb_actor_dropped_response_does_not_kill_actor() {
        let conn = DbConnection::open_duckdb_memory().await.unwrap();
        conn.execute_batch("CREATE TABLE test (id INTEGER)").await.unwrap();

        let (start_tx, start_rx) = oneshot::channel();
        let conn_clone = conn.clone();

        let handle = tokio::spawn(async move {
            conn_clone
                .execute_duckdb_op(move |_| {
                    let _ = start_tx.send(());
                    std::thread::sleep(Duration::from_millis(100));
                    Ok(())
                })
                .await
                .ok();
        });

        let _ = start_rx.await;
        handle.abort();
        tokio::time::sleep(Duration::from_millis(20)).await;

        let count: i64 = conn
            .query_scalar("SELECT COUNT(*) FROM test", &[])
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

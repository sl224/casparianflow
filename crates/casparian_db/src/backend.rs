//! Database backend abstraction layer.
//!
//! DuckDB-only synchronous backend.
//! - Columnar, OLAP-optimized
//! - Single-writer enforced via file lock

use std::path::Path;
use std::rc::Rc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug_span, info};

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

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Backend not available: {0}")]
    NotAvailable(String),

    #[error("DuckDB error: {0}")]
    DuckDb(#[from] duckdb::Error),
}

/// Database access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// Read-write access (requires exclusive lock for DuckDB)
    ReadWrite,
    /// Read-only access (can coexist with other readers)
    ReadOnly,
}

/// Timestamp wrapper for database values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DbTimestamp {
    inner: chrono::DateTime<chrono::Utc>,
}

/// Errors that can occur when parsing or constructing timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTimestampError {
    message: String,
}

impl DbTimestampError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DbTimestampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DbTimestampError {}

impl DbTimestamp {
    /// Current timestamp in UTC.
    pub fn now() -> Self {
        Self {
            inner: chrono::Utc::now(),
        }
    }

    /// Parse an RFC3339 timestamp string.
    pub fn from_rfc3339(value: &str) -> Result<Self, DbTimestampError> {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|dt| Self {
                inner: dt.with_timezone(&chrono::Utc),
            })
            .map_err(|e| DbTimestampError::new(format!("Invalid timestamp: {}", e)))
    }

    /// Construct from Unix milliseconds.
    pub fn from_unix_millis(ms: i64) -> Result<Self, DbTimestampError> {
        let secs = ms / 1_000;
        let nanos = ((ms % 1_000) * 1_000_000) as u32;
        chrono::DateTime::from_timestamp(secs, nanos)
            .map(|dt| Self { inner: dt })
            .ok_or_else(|| DbTimestampError::new("Invalid Unix milliseconds"))
    }

    /// RFC3339 string representation.
    pub fn to_rfc3339(&self) -> String {
        self.inner.to_rfc3339()
    }

    /// Unix milliseconds since epoch.
    pub fn unix_millis(&self) -> i64 {
        self.inner.timestamp_millis()
    }

    pub(crate) fn from_chrono(value: chrono::DateTime<chrono::Utc>) -> Self {
        Self { inner: value }
    }

    pub fn as_chrono(&self) -> &chrono::DateTime<chrono::Utc> {
        &self.inner
    }
}

impl serde::Serialize for DbTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> serde::Deserialize<'de> for DbTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        DbTimestamp::from_rfc3339(&raw).map_err(serde::de::Error::custom)
    }
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
    Timestamp(DbTimestamp),
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

impl From<DbTimestamp> for DbValue {
    fn from(v: DbTimestamp) -> Self {
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
            .ok_or_else(|| {
                BackendError::TypeConversion(format!("Column index {} out of bounds", index))
            })
            .and_then(|v| T::from_db_value(v))
    }

    /// Get a value by column name.
    pub fn get_by_name<T: FromDbValue>(&self, name: &str) -> Result<T, BackendError> {
        let index =
            self.columns.iter().position(|c| c == name).ok_or_else(|| {
                BackendError::TypeConversion(format!("Column '{}' not found", name))
            })?;
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

    /// Get the column names.
    pub fn column_names(&self) -> &[String] {
        &self.columns
    }

    /// Get the raw DbValue at an index.
    pub fn get_raw(&self, index: usize) -> Option<&DbValue> {
        self.values.get(index)
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
            DbValue::Null => Err(BackendError::TypeConversion(
                "i64 field is NULL - use Option<i64> for nullable columns".to_string(),
            )),
            _ => Err(BackendError::TypeConversion("Expected integer".to_string())),
        }
    }
}

impl FromDbValue for i32 {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Integer(v) => i32::try_from(*v)
                .map_err(|_| BackendError::TypeConversion("Expected i32".to_string())),
            DbValue::Null => Err(BackendError::TypeConversion(
                "i32 field is NULL - use Option<i32> for nullable columns".to_string(),
            )),
            _ => Err(BackendError::TypeConversion("Expected integer".to_string())),
        }
    }
}

impl FromDbValue for f64 {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Real(v) => Ok(*v),
            DbValue::Integer(v) => Ok(*v as f64),
            DbValue::Null => Err(BackendError::TypeConversion(
                "f64 field is NULL - use Option<f64> for nullable columns".to_string(),
            )),
            _ => Err(BackendError::TypeConversion("Expected real".to_string())),
        }
    }
}

impl FromDbValue for DbTimestamp {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Timestamp(v) => Ok(v.clone()),
            DbValue::Text(v) => DbTimestamp::from_rfc3339(v)
                .map_err(|e| BackendError::TypeConversion(e.to_string())),
            DbValue::Null => Err(BackendError::TypeConversion(
                "DbTimestamp field is NULL - use Option<DbTimestamp> for nullable columns"
                    .to_string(),
            )),
            _ => Err(BackendError::TypeConversion(
                "Expected timestamp".to_string(),
            )),
        }
    }
}

impl FromDbValue for String {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Text(v) => Ok(v.clone()),
            DbValue::Null => Err(BackendError::TypeConversion(
                "String field is NULL - use Option<String> for nullable columns".to_string(),
            )),
            _ => Err(BackendError::TypeConversion("Expected text".to_string())),
        }
    }
}

impl FromDbValue for bool {
    fn from_db_value(value: &DbValue) -> Result<Self, BackendError> {
        match value {
            DbValue::Boolean(v) => Ok(*v),
            DbValue::Integer(v) => Ok(*v != 0),
            DbValue::Null => Err(BackendError::TypeConversion(
                "bool field is NULL - use Option<bool> for nullable columns".to_string(),
            )),
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
            DbValue::Null => Err(BackendError::TypeConversion(
                "Vec<u8> field is NULL - use Option<Vec<u8>> for nullable columns".to_string(),
            )),
            _ => Err(BackendError::TypeConversion("Expected blob".to_string())),
        }
    }
}

/// Unified database connection.
#[derive(Clone)]
pub struct DbConnection {
    conn: Rc<duckdb::Connection>,
    access_mode: AccessMode,
    /// Holds the exclusive file lock via RAII - not read, but dropping it releases the lock.
    #[allow(dead_code)]
    lock_guard: Option<Rc<crate::lock::DbLockGuard>>,
}

impl std::fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConnection")
            .field("backend", &"DuckDB")
            .field("access_mode", &self.access_mode)
            .finish()
    }
}

impl DbConnection {
    /// Open a database from a URL.
    ///
    /// Supported scheme: duckdb:
    pub fn open_from_url(url: &str) -> Result<Self, BackendError> {
        if let Some(path) = strip_url_prefix(url, "duckdb:") {
            return Self::open_duckdb(Path::new(&path));
        }

        Err(BackendError::NotAvailable(format!(
            "Unsupported database URL: {}",
            url
        )))
    }

    /// Open a database from a URL in read-only mode.
    ///
    /// Supported scheme: duckdb:
    pub fn open_from_url_readonly(url: &str) -> Result<Self, BackendError> {
        if let Some(path) = strip_url_prefix(url, "duckdb:") {
            return Self::open_duckdb_readonly(Path::new(&path));
        }

        Err(BackendError::NotAvailable(format!(
            "Unsupported database URL: {}",
            url
        )))
    }

    /// Open a DuckDB database with exclusive write lock.
    ///
    /// DuckDB only allows one writer process at a time. This function acquires
    /// an exclusive lock before opening the database.
    pub fn open_duckdb(path: &Path) -> Result<Self, BackendError> {
        use crate::lock::{try_lock_exclusive, LockError};

        let lock_guard = try_lock_exclusive(path).map_err(|e| match e {
            LockError::Locked(p) => BackendError::Locked(p.display().to_string()),
            LockError::CreateFailed(io) => {
                BackendError::Database(format!("Lock file error: {}", io))
            }
            LockError::AcquireFailed(io) => {
                BackendError::Database(format!("Lock acquire error: {}", io))
            }
        })?;

        let conn = Rc::new(duckdb::Connection::open(path)?);
        info!(
            "Opened DuckDB database with exclusive lock: {}",
            path.display()
        );

        Ok(Self {
            conn,
            access_mode: AccessMode::ReadWrite,
            lock_guard: Some(Rc::new(lock_guard)),
        })
    }

    /// Open a DuckDB database in read-only mode (no lock required).
    pub fn open_duckdb_readonly(path: &Path) -> Result<Self, BackendError> {
        use duckdb::{AccessMode as DuckAccessMode, Config};

        let config = Config::default()
            .access_mode(DuckAccessMode::ReadOnly)
            .map_err(BackendError::from)?;
        let conn = Rc::new(duckdb::Connection::open_with_flags(path, config)?);
        info!("Opened DuckDB database (read-only): {}", path.display());

        Ok(Self {
            conn,
            access_mode: AccessMode::ReadOnly,
            lock_guard: None,
        })
    }

    /// Open an in-memory DuckDB database (for testing).
    pub fn open_duckdb_memory() -> Result<Self, BackendError> {
        let conn = Rc::new(duckdb::Connection::open_in_memory()?);
        info!("Opened in-memory DuckDB database");

        Ok(Self {
            conn,
            access_mode: AccessMode::ReadWrite,
            lock_guard: None,
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
        "DuckDB"
    }

    /// Execute a SQL statement (no results).
    pub fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        if self.access_mode == AccessMode::ReadOnly {
            return Err(BackendError::ReadOnly);
        }

        Self::execute_duckdb_on_conn(self.conn.as_ref(), sql, params)
    }

    /// Execute a batch of SQL statements.
    pub fn execute_batch(&self, sql: &str) -> Result<(), BackendError> {
        if self.access_mode == AccessMode::ReadOnly {
            return Err(BackendError::ReadOnly);
        }

        Self::execute_duckdb_batch_on_conn(self.conn.as_ref(), sql)
    }

    /// Bulk insert rows into a table.
    ///
    /// Column order must match the row value order.
    pub fn bulk_insert_rows(
        &self,
        table: &str,
        columns: &[&str],
        rows: &[Vec<DbValue>],
    ) -> Result<u64, BackendError> {
        if self.access_mode == AccessMode::ReadOnly {
            return Err(BackendError::ReadOnly);
        }

        let conn = self.conn.as_ref();
        bulk_insert_rows_internal(
            conn,
            |sql, params| Self::execute_duckdb_on_conn(conn, sql, params),
            table,
            columns,
            rows,
        )
    }

    /// Query and return all rows.
    pub fn query_all(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        Self::query_duckdb_on_conn(self.conn.as_ref(), sql, params)
    }

    /// Query and return the first row, if any.
    pub fn query_optional(
        &self,
        sql: &str,
        params: &[DbValue],
    ) -> Result<Option<DbRow>, BackendError> {
        let rows = self.query_all(sql, params)?;
        Ok(rows.into_iter().next())
    }

    /// Query and return exactly one row.
    pub fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<DbRow, BackendError> {
        self.query_optional(sql, params)?
            .ok_or_else(|| BackendError::Query("Expected one row, got none".to_string()))
    }

    /// Query and return a single scalar value.
    pub fn query_scalar<T: FromDbValue>(
        &self,
        sql: &str,
        params: &[DbValue],
    ) -> Result<T, BackendError> {
        let row = self.query_one(sql, params)?;
        row.get(0)
    }

    /// Execute a transaction using DuckDB.
    pub fn transaction<T, F>(&self, op: F) -> Result<T, BackendError>
    where
        F: for<'a> FnOnce(&'a mut DbTransaction<'a>) -> Result<T, BackendError>,
    {
        self.conn.execute_batch("BEGIN")?;
        let mut tx = DbTransaction::duckdb(self.conn.as_ref());
        let result = op(&mut tx);

        match result {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => match self.conn.execute_batch("ROLLBACK") {
                Ok(()) => Err(err),
                Err(rollback_err) => Err(BackendError::Transaction(format!(
                    "Transaction failed: {}; rollback failed: {}",
                    err, rollback_err
                ))),
            },
        }
    }

    fn execute_duckdb_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<u64, BackendError> {
        let op = sql_op_name(sql);
        let sql_hash = hash_sql(sql);
        let span = debug_span!(
            "db.exec",
            op = op,
            sql_hash = %sql_hash,
            duration_ms = tracing::field::Empty
        );
        let _guard = span.enter();
        let start = Instant::now();

        let mut stmt = conn.prepare(sql)?;
        let duckdb_params = Self::to_duckdb_params(params);
        let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params
            .iter()
            .map(|v| v as &dyn duckdb::ToSql)
            .collect();
        let rows = stmt.execute(param_refs.as_slice())?;
        let duration_ms = start.elapsed().as_millis() as u64;
        span.record("duration_ms", duration_ms);
        Ok(rows as u64)
    }

    fn execute_duckdb_batch_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
    ) -> Result<(), BackendError> {
        let sql_hash = hash_sql(sql);
        let span = debug_span!(
            "db.exec_batch",
            op = "BATCH",
            sql_hash = %sql_hash,
            duration_ms = tracing::field::Empty
        );
        let _guard = span.enter();
        let start = Instant::now();
        conn.execute_batch(sql)?;
        let duration_ms = start.elapsed().as_millis() as u64;
        span.record("duration_ms", duration_ms);
        Ok(())
    }

    fn query_duckdb_on_conn(
        conn: &duckdb::Connection,
        sql: &str,
        params: &[DbValue],
    ) -> Result<Vec<DbRow>, BackendError> {
        let op = sql_op_name(sql);
        let sql_hash = hash_sql(sql);
        let span = debug_span!(
            "db.query",
            op = op,
            sql_hash = %sql_hash,
            duration_ms = tracing::field::Empty
        );
        let _guard = span.enter();
        let start = Instant::now();

        let mut stmt = conn.prepare(sql)?;
        let duckdb_params = Self::to_duckdb_params(params);
        let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params
            .iter()
            .map(|v| v as &dyn duckdb::ToSql)
            .collect();

        let mut rows_iter = stmt.query(param_refs.as_slice())?;

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
            return Ok(Vec::new());
        };

        let mut result = Vec::new();

        while let Some(row) = rows_iter.next()? {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = Self::duckdb_value_to_db_value(row, i)?;
                values.push(value);
            }
            result.push(DbRow::new(columns.clone(), values));
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        span.record("duration_ms", duration_ms);
        Ok(result)
    }

    fn to_duckdb_params(params: &[DbValue]) -> Vec<duckdb::types::Value> {
        params
            .iter()
            .map(|p| match p {
                DbValue::Null => duckdb::types::Value::Null,
                DbValue::Integer(v) => duckdb::types::Value::BigInt(*v),
                DbValue::Real(v) => duckdb::types::Value::Double(*v),
                DbValue::Text(v) => duckdb::types::Value::Text(v.clone()),
                DbValue::Blob(v) => duckdb::types::Value::Blob(v.clone()),
                DbValue::Boolean(v) => duckdb::types::Value::Boolean(*v),
                DbValue::Timestamp(v) => {
                    let micros = v.as_chrono().timestamp_micros();
                    duckdb::types::Value::Timestamp(duckdb::types::TimeUnit::Microsecond, micros)
                }
            })
            .collect()
    }

    fn duckdb_value_to_db_value(row: &duckdb::Row, index: usize) -> Result<DbValue, duckdb::Error> {
        use duckdb::types::ValueRef;

        match row.get_ref(index)? {
            ValueRef::Null => Ok(DbValue::Null),
            ValueRef::Boolean(v) => Ok(DbValue::Boolean(v)),
            ValueRef::TinyInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::SmallInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::Int(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::BigInt(v) => Ok(DbValue::Integer(v)),
            ValueRef::HugeInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::UTinyInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::USmallInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::UInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::UBigInt(v) => Ok(DbValue::Integer(v as i64)),
            ValueRef::Float(v) => Ok(DbValue::Real(v as f64)),
            ValueRef::Double(v) => Ok(DbValue::Real(v)),
            ValueRef::Text(v) => Ok(DbValue::Text(String::from_utf8_lossy(v).to_string())),
            ValueRef::Blob(v) => Ok(DbValue::Blob(v.to_vec())),
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
                    Ok(DbValue::Timestamp(DbTimestamp::from_chrono(dt)))
                } else {
                    Ok(DbValue::Integer(micros))
                }
            }
            ValueRef::Date32(days) => {
                if let Some(date) = chrono::NaiveDate::from_num_days_from_ce_opt(719163 + days) {
                    Ok(DbValue::Text(date.format("%Y-%m-%d").to_string()))
                } else {
                    Ok(DbValue::Integer(days as i64))
                }
            }
            ValueRef::Time64(unit, v) => {
                let micros = match unit {
                    duckdb::types::TimeUnit::Second => v * 1_000_000,
                    duckdb::types::TimeUnit::Millisecond => v * 1_000,
                    duckdb::types::TimeUnit::Microsecond => v,
                    duckdb::types::TimeUnit::Nanosecond => v / 1_000,
                };
                let secs = (micros / 1_000_000) as u32;
                let nanos = ((micros % 1_000_000) * 1_000) as u32;
                if let Some(time) =
                    chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos)
                {
                    Ok(DbValue::Text(time.format("%H:%M:%S%.6f").to_string()))
                } else {
                    Ok(DbValue::Integer(micros))
                }
            }
            ValueRef::Interval {
                months,
                days,
                nanos,
            } => Ok(DbValue::Text(format!("P{}M{}DT{}N", months, days, nanos))),
            ValueRef::Enum(_, _) => Ok(DbValue::Text(format!("{:?}", row.get_ref(index)?))),
            ValueRef::List(_, _) => Ok(DbValue::Text(format!("{:?}", row.get_ref(index)?))),
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
}

/// Transaction wrapper for DuckDB.
pub struct DbTransaction<'a> {
    conn: &'a duckdb::Connection,
}

impl<'a> DbTransaction<'a> {
    fn duckdb(conn: &'a duckdb::Connection) -> Self {
        Self { conn }
    }

    pub fn execute(&mut self, sql: &str, params: &[DbValue]) -> Result<u64, BackendError> {
        DbConnection::execute_duckdb_on_conn(self.conn, sql, params)
    }

    pub fn execute_batch(&mut self, sql: &str) -> Result<(), BackendError> {
        DbConnection::execute_duckdb_batch_on_conn(self.conn, sql)
    }

    pub fn query_all(&mut self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, BackendError> {
        DbConnection::query_duckdb_on_conn(self.conn, sql, params)
    }

    pub fn query_optional(
        &mut self,
        sql: &str,
        params: &[DbValue],
    ) -> Result<Option<DbRow>, BackendError> {
        let rows = self.query_all(sql, params)?;
        Ok(rows.into_iter().next())
    }

    pub fn query_one(&mut self, sql: &str, params: &[DbValue]) -> Result<DbRow, BackendError> {
        self.query_optional(sql, params)?
            .ok_or_else(|| BackendError::Query("Expected one row, got none".to_string()))
    }

    pub fn query_scalar<T: FromDbValue>(
        &mut self,
        sql: &str,
        params: &[DbValue],
    ) -> Result<T, BackendError> {
        let row = self.query_one(sql, params)?;
        row.get(0)
    }

    /// Bulk insert rows into a table within this transaction.
    ///
    /// Column order must match the row value order.
    pub fn bulk_insert_rows(
        &mut self,
        table: &str,
        columns: &[&str],
        rows: &[Vec<DbValue>],
    ) -> Result<u64, BackendError> {
        let conn = self.conn;
        bulk_insert_rows_internal(
            conn,
            |sql, params| DbConnection::execute_duckdb_on_conn(conn, sql, params),
            table,
            columns,
            rows,
        )
    }
}

const DEFAULT_MAX_PARAMS: usize = 999;

fn bulk_insert_rows_internal<F>(
    conn: &duckdb::Connection,
    mut execute: F,
    table: &str,
    columns: &[&str],
    rows: &[Vec<DbValue>],
) -> Result<u64, BackendError>
where
    F: FnMut(&str, &[DbValue]) -> Result<u64, BackendError>,
{
    if rows.is_empty() {
        return Ok(0);
    }
    if columns.is_empty() {
        return Err(BackendError::InvalidInput(
            "bulk_insert_rows requires at least one column".to_string(),
        ));
    }

    for (index, row) in rows.iter().enumerate() {
        if row.len() != columns.len() {
            return Err(BackendError::InvalidInput(format!(
                "Row {} has {} values, expected {}",
                index,
                row.len(),
                columns.len()
            )));
        }
    }

    let total_params = rows.len().saturating_mul(columns.len());
    if total_params <= DEFAULT_MAX_PARAMS {
        return bulk_insert_rows_generic(&mut execute, table, columns, rows, DEFAULT_MAX_PARAMS);
    }

    bulk_insert_rows_duckdb(conn, table, rows)
}

fn bulk_insert_rows_duckdb(
    conn: &duckdb::Connection,
    table: &str,
    rows: &[Vec<DbValue>],
) -> Result<u64, BackendError> {
    let mut appender = conn.appender(table)?;
    for row in rows {
        let duckdb_params = DbConnection::to_duckdb_params(row);
        let param_refs: Vec<&dyn duckdb::ToSql> = duckdb_params
            .iter()
            .map(|v| v as &dyn duckdb::ToSql)
            .collect();
        appender.append_row(param_refs.as_slice())?;
    }
    appender.flush()?;
    Ok(rows.len() as u64)
}

fn bulk_insert_rows_generic<F>(
    execute: &mut F,
    table: &str,
    columns: &[&str],
    rows: &[Vec<DbValue>],
    max_params: usize,
) -> Result<u64, BackendError>
where
    F: FnMut(&str, &[DbValue]) -> Result<u64, BackendError>,
{
    let cols_len = columns.len();
    if cols_len > max_params {
        return Err(BackendError::InvalidInput(format!(
            "Too many columns ({}) for max params ({})",
            cols_len, max_params
        )));
    }

    let rows_per_chunk = max_params / cols_len;
    if rows_per_chunk == 0 {
        return Err(BackendError::InvalidInput(
            "Unable to compute rows per chunk".to_string(),
        ));
    }

    let quoted_table = quote_ident_path(table);
    let quoted_cols = columns
        .iter()
        .map(|col| quote_ident(col))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = vec!["?"; cols_len].join(", ");
    let row_clause = format!("({})", placeholders);

    let mut total = 0;
    for chunk in rows.chunks(rows_per_chunk) {
        let values_clause = std::iter::repeat(row_clause.as_str())
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            quoted_table, quoted_cols, values_clause
        );
        let mut params = Vec::with_capacity(chunk.len() * cols_len);
        for row in chunk {
            for value in row {
                params.push(value.clone());
            }
        }
        execute(&sql, &params)?;
        total += chunk.len() as u64;
    }

    Ok(total)
}

fn quote_ident(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len() + 2);
    escaped.push('"');
    for ch in name.chars() {
        if ch == '"' {
            escaped.push('"');
        }
        escaped.push(ch);
    }
    escaped.push('"');
    escaped
}

fn quote_ident_path(path: &str) -> String {
    path.split('.')
        .map(quote_ident)
        .collect::<Vec<_>>()
        .join(".")
}

fn strip_url_prefix(url: &str, prefix: &str) -> Option<String> {
    url.strip_prefix(prefix).map(|rest| rest.to_string())
}

fn sql_op_name(sql: &str) -> &str {
    sql.split_whitespace().next().unwrap_or("unknown")
}

fn hash_sql(sql: &str) -> String {
    // FNV-1a 64-bit hash for low-cardinality, stable identification.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in sql.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_insert_rows_inserts_expected_rows() {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id BIGINT, name TEXT)")
            .unwrap();

        let rows = vec![
            vec![DbValue::from(1_i64), DbValue::from("alpha")],
            vec![DbValue::from(2_i64), DbValue::from("beta")],
        ];
        let inserted = conn.bulk_insert_rows("t", &["id", "name"], &rows).unwrap();

        assert_eq!(inserted, 2);
        let count: i64 = conn.query_scalar("SELECT COUNT(*) FROM t", &[]).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn bulk_insert_rows_rejects_mismatched_row_len() {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id BIGINT, name TEXT)")
            .unwrap();

        let rows = vec![
            vec![DbValue::from(1_i64)],
            vec![DbValue::from(2_i64), DbValue::from("beta")],
        ];
        let err = conn
            .bulk_insert_rows("t", &["id", "name"], &rows)
            .unwrap_err();
        assert!(matches!(err, BackendError::InvalidInput(_)));
    }

    #[test]
    fn bulk_insert_rows_empty_is_noop() {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id BIGINT)").unwrap();

        let inserted = conn.bulk_insert_rows("t", &["id"], &[]).unwrap();
        assert_eq!(inserted, 0);
    }
}

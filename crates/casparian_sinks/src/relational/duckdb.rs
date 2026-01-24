use anyhow::{bail, Context, Result};
use arrow::array::RecordBatch;
use arrow::datatypes::{DataType, Schema};
use casparian_db::{try_lock_exclusive, DbLockGuard, LockError};
use casparian_protocol::SinkMode;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn stage_table_name(job_id: &str, output_name: &str) -> String {
    let seed = format!("{}:{}", job_id, output_name);
    format!(
        "__cf_stage_{}",
        &blake3::hash(seed.as_bytes()).to_hex()[..16]
    )
}

fn is_control_plane_db_path(db_path: &Path) -> bool {
    if db_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "casparian_flow.duckdb")
    {
        return true;
    }

    if let Ok(home) = std::env::var("CASPARIAN_HOME") {
        let candidate = PathBuf::from(home).join("casparian_flow.duckdb");
        if candidate == db_path {
            return true;
        }
    }

    let home_env = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    if let Ok(home) = home_env {
        let candidate = PathBuf::from(home)
            .join(".casparian_flow")
            .join("casparian_flow.duckdb");
        if candidate == db_path {
            return true;
        }
    }

    false
}

/// DuckDB sink writer
pub struct DuckDbSink {
    db_path: PathBuf,
    table_name: String,
    stage_table: String,
    sink_mode: SinkMode,
    conn: duckdb::Connection,
    rows_written: u64,
    schema: Option<Schema>,
    _lock_guard: DbLockGuard,
}

impl DuckDbSink {
    pub fn new(
        db_path: PathBuf,
        table_name: &str,
        sink_mode: SinkMode,
        job_id: &str,
        output_name: &str,
    ) -> Result<Self> {
        if is_control_plane_db_path(&db_path) {
            bail!(
                "Refusing to write sink output into control-plane database: {}",
                db_path.display()
            );
        }
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create database directory: {}", parent.display())
            })?;
        }

        let lock_guard = try_lock_exclusive(&db_path).map_err(|err| match err {
            LockError::Locked(path) => anyhow::anyhow!(
                "DuckDB sink is locked by another writer: {}",
                path.display()
            ),
            LockError::CreateFailed(io) => {
                anyhow::anyhow!("Failed to create DuckDB lock file: {}", io)
            }
            LockError::AcquireFailed(io) => {
                anyhow::anyhow!("Failed to acquire DuckDB lock: {}", io)
            }
        })?;

        let conn = duckdb::Connection::open(&db_path)
            .with_context(|| format!("Failed to open DuckDB database: {}", db_path.display()))?;

        let stage_table = stage_table_name(job_id, output_name);

        Ok(Self {
            db_path,
            table_name: table_name.to_string(),
            stage_table,
            sink_mode,
            conn,
            rows_written: 0,
            schema: None,
            _lock_guard: lock_guard,
        })
    }

    fn with_conn_mut<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut duckdb::Connection) -> Result<T>,
    {
        f(&mut self.conn)
    }

    fn arrow_to_duckdb_type(dt: &DataType) -> String {
        match dt {
            DataType::Boolean => "BOOLEAN".to_string(),
            DataType::Int8 => "TINYINT".to_string(),
            DataType::Int16 => "SMALLINT".to_string(),
            DataType::Int32 => "INTEGER".to_string(),
            DataType::Int64 => "BIGINT".to_string(),
            DataType::UInt8 => "UTINYINT".to_string(),
            DataType::UInt16 => "USMALLINT".to_string(),
            DataType::UInt32 => "UINTEGER".to_string(),
            DataType::UInt64 => "UBIGINT".to_string(),
            DataType::Float16 | DataType::Float32 => "FLOAT".to_string(),
            DataType::Float64 => "DOUBLE".to_string(),
            DataType::Utf8 | DataType::LargeUtf8 => "VARCHAR".to_string(),
            DataType::Binary | DataType::LargeBinary => "BLOB".to_string(),
            DataType::Date32 => "DATE".to_string(),
            DataType::Date64 => "BIGINT".to_string(),
            DataType::Timestamp(_, tz) => {
                if tz.is_some() {
                    "TIMESTAMPTZ".to_string()
                } else {
                    "TIMESTAMP".to_string()
                }
            }
            DataType::Time32(_) | DataType::Time64(_) => "BIGINT".to_string(),
            DataType::Decimal128(precision, scale) => {
                format!("DECIMAL({}, {})", precision, scale)
            }
            DataType::Decimal256(_, _) => "VARCHAR".to_string(),
            _ => "VARCHAR".to_string(),
        }
    }

    pub fn init(&mut self, schema: &Schema) -> Result<()> {
        info!(
            "Initializing DuckDB sink: {} (table: {}, stage: {})",
            self.db_path.display(),
            self.table_name,
            self.stage_table
        );

        let columns: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| {
                let sql_type = Self::arrow_to_duckdb_type(f.data_type());
                let nullable = if f.is_nullable() { "" } else { " NOT NULL" };
                format!(
                    "\"{}\" {}{}",
                    f.name().replace('"', "\"\""),
                    sql_type,
                    nullable
                )
            })
            .collect();

        let stage_ident = quote_ident(&self.stage_table);
        let drop_sql = format!("DROP TABLE IF EXISTS {}", stage_ident);
        let create_sql = format!("CREATE TABLE {} ({})", stage_ident, columns.join(", "));

        debug!("DROP TABLE: {}", drop_sql);
        debug!("CREATE TABLE: {}", create_sql);
        self.with_conn_mut(|conn| {
            conn.execute(&drop_sql, [])
                .context("Failed to drop DuckDB stage table")?;
            conn.execute(&create_sql, [])
                .context("Failed to create DuckDB stage table")?;
            Ok(())
        })?;

        self.schema = Some(schema.clone());
        Ok(())
    }

    pub fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let num_rows = batch.num_rows();
        let mut appender = self
            .conn
            .appender(&self.stage_table)
            .context("Failed to create DuckDB appender")?;
        appender
            .append_record_batch(batch.clone())
            .context("Failed to append DuckDB record batch")?;

        self.rows_written += num_rows as u64;
        debug!(
            "Wrote {} rows to DuckDB (total: {})",
            num_rows, self.rows_written
        );

        Ok(num_rows as u64)
    }

    pub fn prepare(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        let Some(schema) = self.schema.as_ref() else {
            return Ok(());
        };

        let column_list = schema
            .fields()
            .iter()
            .map(|f| quote_ident(f.name()))
            .collect::<Vec<_>>()
            .join(", ");

        let target = quote_ident(&self.table_name);
        let stage = quote_ident(&self.stage_table);
        let sink_mode = self.sink_mode;
        let table_name = self.table_name.clone();

        self.with_conn_mut(|conn| {
            let tx = conn
                .transaction()
                .context("Failed to begin DuckDB transaction")?;
            match sink_mode {
                SinkMode::Append => {
                    let create_dest = format!(
                        "CREATE TABLE IF NOT EXISTS {} AS SELECT {} FROM {} WHERE 1=0",
                        target, column_list, stage
                    );
                    tx.execute(&create_dest, [])
                        .context("Failed to ensure DuckDB destination table")?;
                    let insert_sql = format!(
                        "INSERT INTO {} ({}) SELECT {} FROM {}",
                        target, column_list, column_list, stage
                    );
                    tx.execute(&insert_sql, [])
                        .context("Failed to append DuckDB stage data")?;
                    let drop_stage = format!("DROP TABLE {}", stage);
                    tx.execute(&drop_stage, [])
                        .context("Failed to drop DuckDB stage table")?;
                }
                SinkMode::Replace => {
                    let drop_target = format!("DROP TABLE IF EXISTS {}", target);
                    tx.execute(&drop_target, [])
                        .context("Failed to drop DuckDB target table")?;
                    let rename_sql = format!("ALTER TABLE {} RENAME TO {}", stage, target);
                    tx.execute(&rename_sql, [])
                        .context("Failed to rename DuckDB stage table")?;
                }
                SinkMode::Error => {
                    let rename_sql = format!("ALTER TABLE {} RENAME TO {}", stage, target);
                    tx.execute(&rename_sql, []).with_context(|| {
                        format!(
                            "DuckDB sink in Error mode: destination table '{}' already exists",
                            table_name
                        )
                    })?;
                }
            }
            tx.commit().context("Failed to commit DuckDB transaction")?;
            Ok(())
        })?;

        self.with_conn_mut(|conn| {
            conn.execute_batch("CHECKPOINT")
                .context("Failed to checkpoint DuckDB database")?;
            Ok(())
        })?;
        info!("Committed DuckDB sink: {} total rows", self.rows_written);
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<()> {
        let stage = quote_ident(&self.stage_table);
        self.with_conn_mut(|conn| {
            conn.execute(&format!("DROP TABLE IF EXISTS {}", stage), [])
                .context("Failed to drop DuckDB stage table")?;
            Ok(())
        })?;
        Ok(())
    }
}

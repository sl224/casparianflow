//! Shared sink writers for dev and worker output.
//!
//! Each sink type receives Arrow RecordBatches and writes them to a destination.
//! Sinks handle:
//! - File/connection management
//! - Schema setup
//! - Batch writing
//! - Lineage column injection

use anyhow::{bail, Context, Result};
use arrow::array::{ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

fn job_prefix(job_id: &str) -> &str {
    if job_id.len() >= 8 {
        &job_id[..8]
    } else {
        job_id
    }
}

pub fn output_filename(output_name: &str, job_id: &str, extension: &str) -> String {
    format!("{}_{}.{}", output_name, job_prefix(job_id), extension)
}

#[derive(Debug, Clone)]
pub struct OutputDescriptor {
    pub name: String,
    pub table: Option<String>,
}

pub struct OutputPlan<'a> {
    pub name: String,
    pub table: Option<String>,
    pub batches: Vec<&'a RecordBatch>,
}

pub struct OutputArtifact {
    pub name: String,
    pub uri: String,
    pub rows: u64,
}

pub fn plan_outputs<'a>(
    descriptors: &[OutputDescriptor],
    batches: &'a [RecordBatch],
    default_name: &str,
) -> Result<Vec<OutputPlan<'a>>> {
    if descriptors.is_empty() {
        return Ok(vec![OutputPlan {
            name: default_name.to_string(),
            table: None,
            batches: batches.iter().collect(),
        }]);
    }

    if descriptors.len() != batches.len() {
        bail!(
            "Output metadata count ({}) does not match batch count ({})",
            descriptors.len(),
            batches.len()
        );
    }

    Ok(descriptors
        .iter()
        .zip(batches.iter())
        .map(|(info, batch)| OutputPlan {
            name: info.name.clone(),
            table: info.table.clone(),
            batches: vec![batch],
        })
        .collect())
}

pub fn artifact_uri_for_output(
    parsed_sink: &casparian_protocol::types::ParsedSinkUri,
    output_name: &str,
    output_table: Option<&str>,
    job_id: &str,
) -> Result<String> {
    use casparian_protocol::types::SinkScheme;

    let table_name = output_table.unwrap_or(output_name);

    let uri = match parsed_sink.scheme {
        SinkScheme::Parquet => {
            let filename = output_filename(output_name, job_id, "parquet");
            let path = parsed_sink.path.join(filename);
            format!("file://{}", path.display())
        }
        SinkScheme::Csv => {
            let filename = output_filename(output_name, job_id, "csv");
            let path = parsed_sink.path.join(filename);
            format!("file://{}", path.display())
        }
        SinkScheme::Duckdb => {
            format!("duckdb://{}?table={}", parsed_sink.path.display(), table_name)
        }
        SinkScheme::File => {
            let ext = parsed_sink
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("parquet");
            let filename = output_filename(output_name, job_id, ext);
            let parent = parsed_sink
                .path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let path = parent.join(filename);
            format!("file://{}", path.display())
        }
    };

    Ok(uri)
}

pub fn write_output_plan(
    sink_uri: &str,
    outputs: &[OutputPlan<'_>],
    job_id: &str,
) -> Result<Vec<OutputArtifact>> {
    let parsed = casparian_protocol::types::ParsedSinkUri::parse(sink_uri)
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut registry = SinkRegistry::new();

    for output in outputs {
        let sink = create_sink_from_uri(
            sink_uri,
            &output.name,
            output.table.as_deref(),
            job_id,
        )?;
        registry.add(&output.name, sink);
    }

    let mut artifacts = Vec::new();

    for output in outputs {
        if output.batches.is_empty() {
            continue;
        }

        registry.init(&output.name, output.batches[0].schema().as_ref())?;
        let mut rows = 0;
        for batch in &output.batches {
            registry.write_batch(&output.name, batch)?;
            rows += batch.num_rows() as u64;
        }

        let uri = artifact_uri_for_output(
            &parsed,
            &output.name,
            output.table.as_deref(),
            job_id,
        )?;

        artifacts.push(OutputArtifact {
            name: output.name.clone(),
            uri,
            rows,
        });
    }

    registry.finish()?;

    Ok(artifacts)
}

/// Trait for sink writers
pub trait SinkWriter: Send {
    /// Initialize the sink with the expected schema
    fn init(&mut self, schema: &Schema) -> Result<()>;

    /// Write a batch of records
    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64>;

    /// Finalize and close the sink
    fn finish(self: Box<Self>) -> Result<()>;

    /// Get the name of this sink
    fn name(&self) -> &str;
}

/// Parquet sink writer
///
/// Partitions output by job_id: {output_name}_{job_id}.parquet
pub struct ParquetSink {
    output_dir: PathBuf,
    output_name: String,
    job_id: String,
    writer: Option<parquet::arrow::arrow_writer::ArrowWriter<std::fs::File>>,
    rows_written: u64,
    /// Temp file path for staging
    temp_path: Option<PathBuf>,
    /// Final file path
    final_path: Option<PathBuf>,
}

impl ParquetSink {
    pub fn new(output_dir: PathBuf, output_name: &str, job_id: &str) -> Result<Self> {
        // Ensure output directory exists
        std::fs::create_dir_all(&output_dir)
            .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

        Ok(Self {
            output_dir,
            output_name: output_name.to_string(),
            job_id: job_id.to_string(),
            writer: None,
            rows_written: 0,
            temp_path: None,
            final_path: None,
        })
    }
}

impl SinkWriter for ParquetSink {
    fn init(&mut self, schema: &Schema) -> Result<()> {
        // Partition by job_id: {output_name}_{job_id}.parquet
        let filename = output_filename(&self.output_name, &self.job_id, "parquet");
        let final_path = self.output_dir.join(&filename);

        // Write to temp file first for atomic rename
        let temp_filename = format!(".{}", filename);
        let temp_filename = format!("{}.tmp", temp_filename);
        let temp_path = self.output_dir.join(&temp_filename);

        info!("Initializing Parquet sink: {} (temp: {})", final_path.display(), temp_path.display());

        let file = std::fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create temp parquet file: {}", temp_path.display()))?;

        let props = parquet::file::properties::WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        let arrow_schema = Arc::new(schema.clone());
        let writer = parquet::arrow::arrow_writer::ArrowWriter::try_new(file, arrow_schema, Some(props))
            .context("Failed to create Parquet writer")?;

        self.writer = Some(writer);
        self.temp_path = Some(temp_path);
        self.final_path = Some(final_path);
        Ok(())
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let writer = self.writer.as_mut()
            .ok_or_else(|| anyhow::anyhow!("Parquet sink not initialized"))?;

        writer.write(batch).context("Failed to write batch to Parquet")?;

        let rows = batch.num_rows() as u64;
        self.rows_written += rows;
        debug!("Wrote {} rows to Parquet (total: {})", rows, self.rows_written);

        Ok(rows)
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.close().context("Failed to close Parquet writer")?;

            // Atomic rename: temp -> final
            if let (Some(temp_path), Some(final_path)) = (&self.temp_path, &self.final_path) {
                std::fs::rename(temp_path, final_path)
                    .with_context(|| format!(
                        "Failed to rename {} -> {}",
                        temp_path.display(),
                        final_path.display()
                    ))?;
                info!("Committed Parquet sink: {} ({} rows)", final_path.display(), self.rows_written);
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        &self.output_name
    }
}

impl Drop for ParquetSink {
    fn drop(&mut self) {
        // Cleanup temp file if we didn't finish properly
        if let Some(temp_path) = &self.temp_path {
            if temp_path.exists() {
                let _ = std::fs::remove_file(temp_path);
                warn!("Cleaned up orphaned temp file: {}", temp_path.display());
            }
        }
    }
}

/// CSV sink writer
///
/// Partitions output by job_id: {output_name}_{job_id}.csv
pub struct CsvSink {
    output_dir: PathBuf,
    output_name: String,
    job_id: String,
    writer: Option<arrow::csv::Writer<std::fs::File>>,
    rows_written: u64,
    /// Temp file path for staging
    temp_path: Option<PathBuf>,
    /// Final file path
    final_path: Option<PathBuf>,
}

impl CsvSink {
    pub fn new(output_dir: PathBuf, output_name: &str, job_id: &str) -> Result<Self> {
        std::fs::create_dir_all(&output_dir)
            .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

        Ok(Self {
            output_dir,
            output_name: output_name.to_string(),
            job_id: job_id.to_string(),
            writer: None,
            rows_written: 0,
            temp_path: None,
            final_path: None,
        })
    }
}

impl SinkWriter for CsvSink {
    fn init(&mut self, _schema: &Schema) -> Result<()> {
        // Partition by job_id: {output_name}_{job_id}.csv
        let filename = output_filename(&self.output_name, &self.job_id, "csv");
        let final_path = self.output_dir.join(&filename);

        // Write to temp file first for atomic rename
        let temp_filename = format!(".{}", filename);
        let temp_filename = format!("{}.tmp", temp_filename);
        let temp_path = self.output_dir.join(&temp_filename);

        info!("Initializing CSV sink: {} (temp: {})", final_path.display(), temp_path.display());

        let file = std::fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create temp CSV file: {}", temp_path.display()))?;

        let writer = arrow::csv::WriterBuilder::new()
            .with_header(true)
            .build(file);

        self.writer = Some(writer);
        self.temp_path = Some(temp_path);
        self.final_path = Some(final_path);
        Ok(())
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let writer = self.writer.as_mut()
            .ok_or_else(|| anyhow::anyhow!("CSV sink not initialized"))?;

        writer.write(batch).context("Failed to write batch to CSV")?;

        let rows = batch.num_rows() as u64;
        self.rows_written += rows;
        debug!("Wrote {} rows to CSV (total: {})", rows, self.rows_written);

        Ok(rows)
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        // Drop writer to flush
        drop(self.writer.take());

        // Atomic rename: temp -> final
        if let (Some(temp_path), Some(final_path)) = (&self.temp_path, &self.final_path) {
            std::fs::rename(temp_path, final_path)
                .with_context(|| format!(
                    "Failed to rename {} -> {}",
                    temp_path.display(),
                    final_path.display()
                ))?;
            info!("Committed CSV sink: {} ({} rows)", final_path.display(), self.rows_written);
        }
        Ok(())
    }

    fn name(&self) -> &str {
        &self.output_name
    }
}

impl Drop for CsvSink {
    fn drop(&mut self) {
        // Cleanup temp file if we didn't finish properly
        if let Some(temp_path) = &self.temp_path {
            if temp_path.exists() {
                let _ = std::fs::remove_file(temp_path);
                warn!("Cleaned up orphaned temp file: {}", temp_path.display());
            }
        }
    }
}

/// SQLite sink writer
pub struct SqliteSink {
    db_path: PathBuf,
    table_name: String,
    conn: Option<rusqlite::Connection>,
    rows_written: u64,
    schema: Option<Schema>,
}

impl SqliteSink {
    pub fn new(db_path: PathBuf, table_name: &str) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
        }

        Ok(Self {
            db_path,
            table_name: table_name.to_string(),
            conn: None,
            rows_written: 0,
            schema: None,
        })
    }

    /// Convert Arrow DataType to SQLite type
    fn arrow_to_sqlite_type(dt: &DataType) -> &'static str {
        match dt {
            DataType::Boolean => "INTEGER",
            DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => "INTEGER",
            DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "INTEGER",
            DataType::Float16 | DataType::Float32 | DataType::Float64 => "REAL",
            DataType::Utf8 | DataType::LargeUtf8 => "TEXT",
            DataType::Binary | DataType::LargeBinary => "BLOB",
            DataType::Date32 | DataType::Date64 => "TEXT",
            DataType::Timestamp(_, _) => "TEXT",
            DataType::Time32(_) | DataType::Time64(_) => "TEXT",
            _ => "TEXT",
        }
    }
}

impl SinkWriter for SqliteSink {
    fn init(&mut self, schema: &Schema) -> Result<()> {
        info!("Initializing SQLite sink: {} (table: {})", self.db_path.display(), self.table_name);

        let conn = rusqlite::Connection::open(&self.db_path)
            .with_context(|| format!("Failed to open SQLite database: {}", self.db_path.display()))?;

        // Build CREATE TABLE statement
        let columns: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| {
                let sql_type = Self::arrow_to_sqlite_type(f.data_type());
                let nullable = if f.is_nullable() { "" } else { " NOT NULL" };
                format!("\"{}\" {}{}", f.name(), sql_type, nullable)
            })
            .collect();

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
            self.table_name,
            columns.join(", ")
        );

        debug!("CREATE TABLE: {}", create_sql);
        conn.execute(&create_sql, [])
            .context("Failed to create table")?;

        self.conn = Some(conn);
        self.schema = Some(schema.clone());
        Ok(())
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let conn = self.conn.as_mut()
            .ok_or_else(|| anyhow::anyhow!("SQLite sink not initialized"))?;

        let schema = self.schema.as_ref().unwrap();
        let num_cols = batch.num_columns();
        let num_rows = batch.num_rows();

        // Build INSERT statement
        let placeholders: Vec<&str> = (0..num_cols).map(|_| "?").collect();
        let columns: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();

        let insert_sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            self.table_name,
            columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", "),
            placeholders.join(", ")
        );

        // Begin transaction for batch insert
        let tx = conn.transaction().context("Failed to begin transaction")?;

        {
            let mut stmt = tx.prepare(&insert_sql).context("Failed to prepare INSERT")?;

            for row_idx in 0..num_rows {
                let params: Vec<Box<dyn rusqlite::ToSql>> = (0..num_cols)
                    .map(|col_idx| {
                        let array = batch.column(col_idx);
                        arrow_value_to_sqlite(array, row_idx)
                    })
                    .collect();

                let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
                stmt.execute(params_refs.as_slice())
                    .context("Failed to execute INSERT")?;
            }
        }

        tx.commit().context("Failed to commit transaction")?;

        self.rows_written += num_rows as u64;
        debug!("Wrote {} rows to SQLite (total: {})", num_rows, self.rows_written);

        Ok(num_rows as u64)
    }

    fn finish(self: Box<Self>) -> Result<()> {
        // Connection closes on drop
        info!("Closed SQLite sink: {} total rows", self.rows_written);
        Ok(())
    }

    fn name(&self) -> &str {
        &self.table_name
    }
}

/// Convert an Arrow array value at index to SQLite parameter
fn arrow_value_to_sqlite(array: &ArrayRef, row: usize) -> Box<dyn rusqlite::ToSql> {
    use arrow::array::*;

    if array.is_null(row) {
        return Box::new(rusqlite::types::Null);
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Box::new(arr.value(row) as i32)
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Box::new(arr.value(row))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            Box::new(arr.value(row) as i64)
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            Box::new(arr.value(row) as f64)
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            Box::new(arr.value(row))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            Box::new(arr.value(row).to_string())
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            Box::new(arr.value(row).to_string())
        }
        _ => {
            // Fallback: convert to string
            Box::new(format!("{:?}", array.slice(row, 1)))
        }
    }
}

/// DuckDB sink writer
pub struct DuckDbSink {
    db_path: PathBuf,
    table_name: String,
    conn: Option<duckdb::Connection>,
    rows_written: u64,
    schema: Option<Schema>,
}

impl DuckDbSink {
    pub fn new(db_path: PathBuf, table_name: &str) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
        }

        Ok(Self {
            db_path,
            table_name: table_name.to_string(),
            conn: None,
            rows_written: 0,
            schema: None,
        })
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
}

impl SinkWriter for DuckDbSink {
    fn init(&mut self, schema: &Schema) -> Result<()> {
        info!("Initializing DuckDB sink: {} (table: {})", self.db_path.display(), self.table_name);

        let conn = duckdb::Connection::open(&self.db_path)
            .with_context(|| format!("Failed to open DuckDB database: {}", self.db_path.display()))?;

        let columns: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| {
                let sql_type = Self::arrow_to_duckdb_type(f.data_type());
                let nullable = if f.is_nullable() { "" } else { " NOT NULL" };
                format!("\"{}\" {}{}", f.name().replace('"', "\"\""), sql_type, nullable)
            })
            .collect();

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
            self.table_name.replace('"', "\"\""),
            columns.join(", ")
        );

        debug!("CREATE TABLE: {}", create_sql);
        conn.execute(&create_sql, [])
            .context("Failed to create DuckDB table")?;

        self.conn = Some(conn);
        self.schema = Some(schema.clone());
        Ok(())
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let conn = self.conn.as_mut()
            .ok_or_else(|| anyhow::anyhow!("DuckDB sink not initialized"))?;

        let schema = self.schema.as_ref().unwrap();
        let num_cols = batch.num_columns();
        let num_rows = batch.num_rows();

        let placeholders: Vec<&str> = (0..num_cols).map(|_| "?").collect();
        let columns: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();

        let insert_sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            self.table_name.replace('"', "\"\""),
            columns
                .iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(", "),
            placeholders.join(", ")
        );

        let tx = conn.transaction().context("Failed to begin DuckDB transaction")?;
        {
            let mut stmt = tx.prepare(&insert_sql).context("Failed to prepare DuckDB INSERT")?;
            for row_idx in 0..num_rows {
                let params: Vec<duckdb::types::Value> = (0..num_cols)
                    .map(|col_idx| {
                        let array = batch.column(col_idx);
                        arrow_value_to_duckdb(array, row_idx)
                    })
                    .collect();
                let param_refs: Vec<&dyn duckdb::ToSql> =
                    params.iter().map(|p| p as &dyn duckdb::ToSql).collect();
                stmt.execute(param_refs.as_slice())
                    .context("Failed to execute DuckDB INSERT")?;
            }
        }
        tx.commit().context("Failed to commit DuckDB transaction")?;

        self.rows_written += num_rows as u64;
        debug!("Wrote {} rows to DuckDB (total: {})", num_rows, self.rows_written);

        Ok(num_rows as u64)
    }

    fn finish(self: Box<Self>) -> Result<()> {
        info!("Closed DuckDB sink: {} total rows", self.rows_written);
        Ok(())
    }

    fn name(&self) -> &str {
        &self.table_name
    }
}

fn arrow_value_to_duckdb(array: &ArrayRef, row: usize) -> duckdb::types::Value {
    use arrow::array::*;
    use duckdb::types::Value;
    use rust_decimal::Decimal;

    if array.is_null(row) {
        return Value::Null;
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Value::Boolean(arr.value(row))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            Value::TinyInt(arr.value(row))
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            Value::SmallInt(arr.value(row))
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            Value::Int(arr.value(row))
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Value::BigInt(arr.value(row))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            Value::UTinyInt(arr.value(row))
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            Value::USmallInt(arr.value(row))
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            Value::UInt(arr.value(row))
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            Value::UBigInt(arr.value(row))
        }
        DataType::Float16 => {
            let arr = array.as_any().downcast_ref::<Float16Array>().unwrap();
            Value::Float(f32::from(arr.value(row)))
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            Value::Float(arr.value(row))
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            Value::Double(arr.value(row))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            Value::Text(arr.value(row).to_string())
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            Value::Text(arr.value(row).to_string())
        }
        DataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            Value::Blob(arr.value(row).to_vec())
        }
        DataType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            Value::Blob(arr.value(row).to_vec())
        }
        DataType::Date32 => {
            let arr = array.as_any().downcast_ref::<Date32Array>().unwrap();
            Value::Date32(arr.value(row))
        }
        DataType::Date64 => {
            let arr = array.as_any().downcast_ref::<Date64Array>().unwrap();
            Value::BigInt(arr.value(row))
        }
        DataType::Time32(unit) => match unit {
            arrow::datatypes::TimeUnit::Second => {
                let arr = array.as_any().downcast_ref::<Time32SecondArray>().unwrap();
                Value::BigInt(arr.value(row) as i64)
            }
            arrow::datatypes::TimeUnit::Millisecond => {
                let arr = array.as_any().downcast_ref::<Time32MillisecondArray>().unwrap();
                Value::BigInt(arr.value(row) as i64)
            }
            _ => Value::Text(format!("{:?}", array.slice(row, 1))),
        },
        DataType::Time64(unit) => match unit {
            arrow::datatypes::TimeUnit::Microsecond => {
                let arr = array.as_any().downcast_ref::<Time64MicrosecondArray>().unwrap();
                Value::BigInt(arr.value(row))
            }
            arrow::datatypes::TimeUnit::Nanosecond => {
                let arr = array.as_any().downcast_ref::<Time64NanosecondArray>().unwrap();
                Value::BigInt(arr.value(row))
            }
            _ => Value::Text(format!("{:?}", array.slice(row, 1))),
        },
        DataType::Timestamp(unit, _) => {
            let (duck_unit, value) = match unit {
                arrow::datatypes::TimeUnit::Second => {
                    let arr = array.as_any().downcast_ref::<TimestampSecondArray>().unwrap();
                    (duckdb::types::TimeUnit::Second, arr.value(row))
                }
                arrow::datatypes::TimeUnit::Millisecond => {
                    let arr = array.as_any().downcast_ref::<TimestampMillisecondArray>().unwrap();
                    (duckdb::types::TimeUnit::Millisecond, arr.value(row))
                }
                arrow::datatypes::TimeUnit::Microsecond => {
                    let arr = array.as_any().downcast_ref::<TimestampMicrosecondArray>().unwrap();
                    (duckdb::types::TimeUnit::Microsecond, arr.value(row))
                }
                arrow::datatypes::TimeUnit::Nanosecond => {
                    let arr = array.as_any().downcast_ref::<TimestampNanosecondArray>().unwrap();
                    (duckdb::types::TimeUnit::Nanosecond, arr.value(row))
                }
            };
            Value::Timestamp(duck_unit, value)
        }
        DataType::Decimal128(_, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            let value = arr.value(row);
            if *scale < 0 {
                Value::Text(format!("{:?}", array.slice(row, 1)))
            } else {
                Value::Decimal(Decimal::from_i128_with_scale(value, *scale as u32))
            }
        }
        DataType::Decimal256(_, _) => Value::Text(format!("{:?}", array.slice(row, 1))),
        _ => Value::Text(format!("{:?}", array.slice(row, 1))),
    }
}

/// Sink registry - manages multiple sinks for a run
pub struct SinkRegistry {
    sinks: HashMap<String, Box<dyn SinkWriter>>,
}

impl SinkRegistry {
    pub fn new() -> Self {
        Self {
            sinks: HashMap::new(),
        }
    }

    /// Add a sink for an output name
    pub fn add(&mut self, name: &str, sink: Box<dyn SinkWriter>) {
        self.sinks.insert(name.to_string(), sink);
    }

    /// Initialize a sink with its schema
    pub fn init(&mut self, name: &str, schema: &Schema) -> Result<()> {
        if let Some(sink) = self.sinks.get_mut(name) {
            sink.init(schema)?;
        } else {
            bail!("No sink registered for output: {}", name);
        }
        Ok(())
    }

    /// Write a batch to a sink
    pub fn write_batch(&mut self, name: &str, batch: &RecordBatch) -> Result<u64> {
        if let Some(sink) = self.sinks.get_mut(name) {
            sink.write_batch(batch)
        } else {
            bail!("No sink registered for output: {}", name);
        }
    }

    /// Finish all sinks
    pub fn finish(self) -> Result<()> {
        for (name, sink) in self.sinks {
            debug!("Finishing sink: {}", name);
            sink.finish()?;
        }
        Ok(())
    }

    /// Get registered sink names
    pub fn sink_names(&self) -> Vec<&str> {
        self.sinks.keys().map(|s| s.as_str()).collect()
    }
}

/// Validate that a batch conforms to a declared schema
///
/// Returns Ok(()) if the batch schema matches, or an error describing the mismatch.
pub fn validate_batch_schema(
    batch: &RecordBatch,
    declared_schema: &Schema,
    sink_name: &str,
) -> Result<()> {
    let batch_schema = batch.schema();

    // Check field count matches
    if batch_schema.fields().len() != declared_schema.fields().len() {
        bail!(
            "Schema mismatch for sink '{}': expected {} columns, got {}",
            sink_name,
            declared_schema.fields().len(),
            batch_schema.fields().len()
        );
    }

    // Check each field
    for (i, (batch_field, declared_field)) in batch_schema.fields().iter().zip(declared_schema.fields().iter()).enumerate() {
        // Check name
        if batch_field.name() != declared_field.name() {
            bail!(
                "Schema mismatch for sink '{}' column {}: expected name '{}', got '{}'",
                sink_name, i, declared_field.name(), batch_field.name()
            );
        }

        // Check data type (allow some compatible type coercions)
        if !types_compatible(batch_field.data_type(), declared_field.data_type()) {
            bail!(
                "Schema mismatch for sink '{}' column '{}': expected type {:?}, got {:?}",
                sink_name, declared_field.name(), declared_field.data_type(), batch_field.data_type()
            );
        }

        // Check nullability (batch can be more restrictive)
        if batch_field.is_nullable() && !declared_field.is_nullable() {
            warn!(
                "Schema warning for sink '{}' column '{}': batch allows nulls but declared schema doesn't",
                sink_name, declared_field.name()
            );
        }
    }

    Ok(())
}

/// Check if two Arrow data types are compatible
fn types_compatible(actual: &DataType, expected: &DataType) -> bool {
    if actual == expected {
        return true;
    }

    // Allow some common compatible conversions
    match (actual, expected) {
        // Integer widening is ok
        (DataType::Int8, DataType::Int16 | DataType::Int32 | DataType::Int64) => true,
        (DataType::Int16, DataType::Int32 | DataType::Int64) => true,
        (DataType::Int32, DataType::Int64) => true,
        (DataType::UInt8, DataType::UInt16 | DataType::UInt32 | DataType::UInt64) => true,
        (DataType::UInt16, DataType::UInt32 | DataType::UInt64) => true,
        (DataType::UInt32, DataType::UInt64) => true,

        // Float widening is ok
        (DataType::Float32, DataType::Float64) => true,

        // String types are compatible
        (DataType::Utf8, DataType::LargeUtf8) => true,
        (DataType::LargeUtf8, DataType::Utf8) => true,

        // Timestamp units can vary
        (DataType::Timestamp(_, tz1), DataType::Timestamp(_, tz2)) => tz1 == tz2,

        _ => false,
    }
}

/// Inject lineage columns into a RecordBatch
///
/// Adds:
/// - _cf_source_hash: Blake3 hash of the source file
/// - _cf_job_id: Unique ID for this processing run
/// - _cf_processed_at: ISO 8601 timestamp of when the record was processed
/// - _cf_parser_version: Parser version that processed this record
pub fn inject_lineage_columns(
    batch: &RecordBatch,
    source_hash: &str,
    job_id: &str,
    parser_version: &str,
) -> Result<RecordBatch> {
    let num_rows = batch.num_rows();

    // Current timestamp in ISO 8601 format
    let processed_at = chrono::Utc::now().to_rfc3339();

    // Create lineage column arrays
    let source_hash_array: ArrayRef = Arc::new(StringArray::from(vec![source_hash; num_rows]));
    let job_id_array: ArrayRef = Arc::new(StringArray::from(vec![job_id; num_rows]));
    let processed_at_array: ArrayRef = Arc::new(StringArray::from(vec![processed_at.as_str(); num_rows]));
    let parser_version_array: ArrayRef = Arc::new(StringArray::from(vec![parser_version; num_rows]));

    // Build new schema with lineage columns
    let mut fields: Vec<Field> = batch.schema().fields().iter().map(|f| f.as_ref().clone()).collect();
    fields.push(Field::new("_cf_source_hash", DataType::Utf8, false));
    fields.push(Field::new("_cf_job_id", DataType::Utf8, false));
    fields.push(Field::new("_cf_processed_at", DataType::Utf8, false));
    fields.push(Field::new("_cf_parser_version", DataType::Utf8, false));

    let new_schema = Arc::new(Schema::new(fields));

    // Build new columns list
    let mut columns: Vec<ArrayRef> = batch.columns().to_vec();
    columns.push(source_hash_array);
    columns.push(job_id_array);
    columns.push(processed_at_array);
    columns.push(parser_version_array);

    RecordBatch::try_new(new_schema, columns)
        .context("Failed to create batch with lineage columns")
}

/// Create a sink from a SinkUri
pub fn create_sink_from_uri(
    uri: &str,
    output_name: &str,
    output_table: Option<&str>,
    job_id: &str,
) -> Result<Box<dyn SinkWriter>> {
    let parsed = casparian_protocol::types::ParsedSinkUri::parse(uri)
        .map_err(|e| anyhow::anyhow!(e))?;
    let table_name = output_table.unwrap_or(output_name);

    match parsed.scheme {
        casparian_protocol::types::SinkScheme::Parquet => {
            Ok(Box::new(ParquetSink::new(parsed.path, output_name, job_id)?))
        }
        casparian_protocol::types::SinkScheme::Csv => {
            Ok(Box::new(CsvSink::new(parsed.path, output_name, job_id)?))
        }
        casparian_protocol::types::SinkScheme::Duckdb => {
            Ok(Box::new(DuckDbSink::new(parsed.path, table_name)?))
        }
        casparian_protocol::types::SinkScheme::File => {
            // File sink: infer by extension
            let ext = parsed.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "parquet" => Ok(Box::new(ParquetSink::new(
                    parsed.path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf(),
                    output_name,
                    job_id,
                )?)),
                "csv" => Ok(Box::new(CsvSink::new(
                    parsed.path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf(),
                    output_name,
                    job_id,
                )?)),
                "duckdb" | "db" => Ok(Box::new(DuckDbSink::new(parsed.path, table_name)?)),
                _ => bail!("Unsupported file sink extension: '{}'", ext),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Decimal128Builder, Int64Array, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{Field, TimeUnit};
    use tempfile::tempdir;

    fn create_test_batch() -> RecordBatch {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]);

        let id_array = Int64Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec![Some("Alice"), Some("Bob"), None]);

        RecordBatch::try_new(
            Arc::new(schema),
            vec![Arc::new(id_array), Arc::new(name_array)],
        )
        .unwrap()
    }

    #[test]
    fn test_parquet_sink() {
        let dir = tempdir().unwrap();
        let job_id = "12345678-abcd-1234-abcd-123456789abc";
        let mut sink = ParquetSink::new(dir.path().to_path_buf(), "test", job_id).unwrap();

        let batch = create_test_batch();
        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        Box::new(sink).finish().unwrap();

        // Verify partitioned file exists
        let output_path = dir.path().join("test_12345678.parquet");
        assert!(output_path.exists());

        // Verify temp file was cleaned up
        let temp_path = dir.path().join(".test_12345678.parquet.tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_csv_sink() {
        let dir = tempdir().unwrap();
        let job_id = "12345678-abcd-1234-abcd-123456789abc";
        let mut sink = CsvSink::new(dir.path().to_path_buf(), "test", job_id).unwrap();

        let batch = create_test_batch();
        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        Box::new(sink).finish().unwrap();

        // Verify partitioned file exists and has content
        let output_path = dir.path().join("test_12345678.csv");
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("id,name"));
        assert!(content.contains("Alice"));

        // Verify temp file was cleaned up
        let temp_path = dir.path().join(".test_12345678.csv.tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_sqlite_sink() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut sink = SqliteSink::new(db_path.clone(), "records").unwrap();

        let batch = create_test_batch();
        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        Box::new(sink).finish().unwrap();

        // Verify data was written
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_duckdb_sink() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let mut sink = DuckDbSink::new(db_path.clone(), "records").unwrap();

        let batch = create_test_batch();
        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        Box::new(sink).finish().unwrap();

        let conn = duckdb::Connection::open(db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_duckdb_sink_decimal_timestamp_tz() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_decimal_tz.duckdb");
        let mut sink = DuckDbSink::new(db_path.clone(), "records").unwrap();

        let mut dec_builder =
            Decimal128Builder::with_capacity(3).with_data_type(DataType::Decimal128(10, 2));
        dec_builder.append_value(12_345);
        dec_builder.append_null();
        dec_builder.append_value(-6_789);
        let dec_array = dec_builder.finish();

        let ts_array = TimestampMicrosecondArray::from(vec![
            Some(1_700_000_000_000_000),
            None,
            Some(1_700_000_100_000_000),
        ])
        .with_timezone("UTC");

        let schema = Schema::new(vec![
            Field::new("amount", DataType::Decimal128(10, 2), true),
            Field::new(
                "event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![Arc::new(dec_array), Arc::new(ts_array)],
        )
        .unwrap();

        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        Box::new(sink).finish().unwrap();

        let conn = duckdb::Connection::open(db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_inject_lineage_columns() {
        let batch = create_test_batch();
        let with_lineage = inject_lineage_columns(&batch, "abc123", "job-456", "1.0.0").unwrap();

        // Original 2 columns + 4 lineage columns
        assert_eq!(with_lineage.num_columns(), 6);
        assert!(with_lineage.schema().field_with_name("_cf_source_hash").is_ok());
        assert!(with_lineage.schema().field_with_name("_cf_job_id").is_ok());
        assert!(with_lineage.schema().field_with_name("_cf_processed_at").is_ok());
        assert!(with_lineage.schema().field_with_name("_cf_parser_version").is_ok());

        // Verify source_hash values
        let hash_col = with_lineage
            .column_by_name("_cf_source_hash")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(hash_col.value(0), "abc123");
        assert_eq!(hash_col.value(1), "abc123");
        assert_eq!(hash_col.value(2), "abc123");

        // Verify job_id values
        let job_col = with_lineage
            .column_by_name("_cf_job_id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(job_col.value(0), "job-456");

        // Verify processed_at is set (ISO 8601 format)
        let ts_col = with_lineage
            .column_by_name("_cf_processed_at")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(ts_col.value(0).contains("T")); // ISO 8601 contains T

        // Verify parser_version values
        let version_col = with_lineage
            .column_by_name("_cf_parser_version")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(version_col.value(0), "1.0.0");
        assert_eq!(version_col.value(1), "1.0.0");
        assert_eq!(version_col.value(2), "1.0.0");
    }
}

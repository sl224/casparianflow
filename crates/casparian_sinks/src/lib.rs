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
use thiserror::Error;
use tracing::{debug, info, warn};

use casparian_protocol::SinkMode;
mod relational;
pub use relational::duckdb::DuckDbSink;
use relational::{RelationalBackend, RelationalSink};

fn job_prefix(job_id: &str) -> String {
    // Use a stable 16-hex blake3 digest prefix to avoid collisions
    blake3::hash(job_id.as_bytes()).to_hex()[..16].to_string()
}

pub fn output_filename(output_name: &str, job_id: &str, extension: &str) -> String {
    format!("{}_{}.{}", output_name, job_prefix(job_id), extension)
}

/// Errors returned by sink planning and writing.
#[derive(Debug, Error)]
pub enum SinkError {
    #[error("{message}")]
    Message { message: String },
    #[error("{message}")]
    Source {
        message: String,
        #[source]
        source: anyhow::Error,
    },
}

pub type SinkResult<T> = std::result::Result<T, SinkError>;

impl SinkError {
    fn message(message: impl Into<String>) -> Self {
        SinkError::Message {
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for SinkError {
    fn from(err: anyhow::Error) -> Self {
        SinkError::Source {
            message: err.to_string(),
            source: err,
        }
    }
}

/// Output batch wrapper to avoid leaking Arrow types in public APIs.
#[derive(Debug, Clone)]
pub struct OutputBatch {
    batch: Arc<RecordBatch>,
}

impl OutputBatch {
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn from_record_batch(batch: RecordBatch) -> Self {
        Self {
            batch: Arc::new(batch),
        }
    }

    #[cfg(not(feature = "internal"))]
    pub(crate) fn from_record_batch(batch: RecordBatch) -> Self {
        Self {
            batch: Arc::new(batch),
        }
    }

    pub fn num_rows(&self) -> usize {
        self.batch.num_rows()
    }

    pub(crate) fn record_batch(&self) -> &RecordBatch {
        &self.batch
    }

    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn as_record_batch(&self) -> &RecordBatch {
        self.record_batch()
    }

    pub(crate) fn schema(&self) -> Arc<Schema> {
        self.batch.schema()
    }
}

#[derive(Debug, Clone)]
pub struct OutputDescriptor {
    pub name: String,
    pub table: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutputPlan {
    name: String,
    table: Option<String>,
    batches: Vec<OutputBatch>,
    sink_mode: SinkMode,
}

impl OutputPlan {
    pub fn new(
        name: impl Into<String>,
        table: Option<String>,
        batches: Vec<OutputBatch>,
        sink_mode: SinkMode,
    ) -> Self {
        Self {
            name: name.into(),
            table,
            batches,
            sink_mode,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn table(&self) -> Option<&str> {
        self.table.as_deref()
    }

    pub fn batches(&self) -> &[OutputBatch] {
        &self.batches
    }

    pub fn sink_mode(&self) -> SinkMode {
        self.sink_mode
    }
}

pub struct OutputArtifact {
    pub name: String,
    pub uri: String,
    pub rows: u64,
}

pub fn plan_outputs(
    descriptors: &[OutputDescriptor],
    output_batches: &[Vec<OutputBatch>],
    default_name: &str,
) -> SinkResult<Vec<OutputPlan>> {
    if descriptors.is_empty() {
        let batches: Vec<OutputBatch> = output_batches
            .iter()
            .flat_map(|group| group.iter().cloned())
            .collect();
        return Ok(vec![OutputPlan::new(
            default_name,
            None,
            batches,
            SinkMode::Append,
        )]);
    }

    if descriptors.len() != output_batches.len() {
        return Err(SinkError::message(format!(
            "Output metadata count ({}) does not match output count ({})",
            descriptors.len(),
            output_batches.len()
        )));
    }

    Ok(descriptors
        .iter()
        .zip(output_batches.iter())
        .map(|(info, batches)| {
            OutputPlan::new(
                info.name.clone(),
                info.table.clone(),
                batches.clone(),
                SinkMode::Append,
            )
        })
        .collect())
}

pub fn artifact_uri_for_output(
    parsed_sink: &casparian_protocol::types::ParsedSinkUri,
    output_name: &str,
    output_table: Option<&str>,
    job_id: &str,
) -> SinkResult<String> {
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
            format!(
                "duckdb://{}?table={}",
                parsed_sink.path.display(),
                table_name
            )
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
    outputs: &[OutputPlan],
    job_id: &str,
    should_commit: Option<&dyn Fn() -> bool>,
) -> SinkResult<Vec<OutputArtifact>> {
    let parsed = casparian_protocol::types::ParsedSinkUri::parse(sink_uri)
        .map_err(|e| SinkError::message(format!("Failed to parse sink URI: {}", e)))?;
    let mut registry = SinkRegistry::new();

    for output in outputs {
        let sink = create_sink_from_uri(
            sink_uri,
            output.name(),
            output.table(),
            output.sink_mode(),
            job_id,
        )?;
        registry.add(output.name(), sink);
    }

    let mut artifacts = Vec::new();

    for output in outputs {
        if output.batches().is_empty() {
            continue;
        }

        let first_schema = output.batches()[0].schema();
        registry.init(output.name(), first_schema.as_ref())?;
        let mut rows = 0;
        for batch in output.batches() {
            validate_batch_schema(batch.record_batch(), first_schema.as_ref(), output.name())?;
            registry.write_batch(output.name(), batch.record_batch())?;
            rows += batch.num_rows() as u64;
        }

        let uri = artifact_uri_for_output(&parsed, output.name(), output.table(), job_id)?;

        artifacts.push(OutputArtifact {
            name: output.name().to_string(),
            uri,
            rows,
        });
    }

    registry.finish_with_guard(should_commit)?;

    Ok(artifacts)
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
    /// True once final file has been promoted
    committed: bool,
}

impl ParquetSink {
    pub fn new(output_dir: PathBuf, output_name: &str, job_id: &str) -> Result<Self> {
        // Ensure output directory exists
        std::fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;

        Ok(Self {
            output_dir,
            output_name: output_name.to_string(),
            job_id: job_id.to_string(),
            writer: None,
            rows_written: 0,
            temp_path: None,
            final_path: None,
            committed: false,
        })
    }
}

impl ParquetSink {
    fn init(&mut self, schema: &Schema) -> Result<()> {
        // Partition by job_id: {output_name}_{job_id}.parquet
        let filename = output_filename(&self.output_name, &self.job_id, "parquet");
        let final_path = self.output_dir.join(&filename);

        // Write to temp file first for atomic rename
        let temp_filename = format!(".{}", filename);
        let temp_filename = format!("{}.tmp", temp_filename);
        let temp_path = self.output_dir.join(&temp_filename);

        info!(
            "Initializing Parquet sink: {} (temp: {})",
            final_path.display(),
            temp_path.display()
        );

        let file = std::fs::File::create(&temp_path).with_context(|| {
            format!(
                "Failed to create temp parquet file: {}",
                temp_path.display()
            )
        })?;

        let props = parquet::file::properties::WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        let arrow_schema = Arc::new(schema.clone());
        let writer =
            parquet::arrow::arrow_writer::ArrowWriter::try_new(file, arrow_schema, Some(props))
                .context("Failed to create Parquet writer")?;

        self.writer = Some(writer);
        self.temp_path = Some(temp_path);
        self.final_path = Some(final_path);
        Ok(())
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Parquet sink not initialized"))?;

        writer
            .write(batch)
            .context("Failed to write batch to Parquet")?;

        let rows = batch.num_rows() as u64;
        self.rows_written += rows;
        debug!(
            "Wrote {} rows to Parquet (total: {})",
            rows, self.rows_written
        );

        Ok(rows)
    }

    fn prepare(&mut self) -> Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.close().context("Failed to close Parquet writer")?;
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if let (Some(temp_path), Some(final_path)) = (&self.temp_path, &self.final_path) {
            std::fs::rename(temp_path, final_path).with_context(|| {
                format!(
                    "Failed to rename {} -> {}",
                    temp_path.display(),
                    final_path.display()
                )
            })?;
            info!(
                "Committed Parquet sink: {} ({} rows)",
                final_path.display(),
                self.rows_written
            );
            self.committed = true;
        }
        self.temp_path = None;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if self.committed {
            if let Some(final_path) = &self.final_path {
                if final_path.exists() {
                    let _ = std::fs::remove_file(final_path);
                    warn!(
                        "Rolled back Parquet committed file: {}",
                        final_path.display()
                    );
                }
            }
        }
        if let Some(temp_path) = &self.temp_path {
            if temp_path.exists() {
                let _ = std::fs::remove_file(temp_path);
                warn!("Rolled back Parquet temp file: {}", temp_path.display());
            }
        }
        self.temp_path = None;
        self.final_path = None;
        self.committed = false;
        Ok(())
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
    /// True once final file has been promoted
    committed: bool,
}

impl CsvSink {
    pub fn new(output_dir: PathBuf, output_name: &str, job_id: &str) -> Result<Self> {
        std::fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;

        Ok(Self {
            output_dir,
            output_name: output_name.to_string(),
            job_id: job_id.to_string(),
            writer: None,
            rows_written: 0,
            temp_path: None,
            final_path: None,
            committed: false,
        })
    }
}

impl CsvSink {
    fn init(&mut self, _schema: &Schema) -> Result<()> {
        // Partition by job_id: {output_name}_{job_id}.csv
        let filename = output_filename(&self.output_name, &self.job_id, "csv");
        let final_path = self.output_dir.join(&filename);

        // Write to temp file first for atomic rename
        let temp_filename = format!(".{}", filename);
        let temp_filename = format!("{}.tmp", temp_filename);
        let temp_path = self.output_dir.join(&temp_filename);

        info!(
            "Initializing CSV sink: {} (temp: {})",
            final_path.display(),
            temp_path.display()
        );

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
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("CSV sink not initialized"))?;

        writer
            .write(batch)
            .context("Failed to write batch to CSV")?;

        let rows = batch.num_rows() as u64;
        self.rows_written += rows;
        debug!("Wrote {} rows to CSV (total: {})", rows, self.rows_written);

        Ok(rows)
    }

    fn prepare(&mut self) -> Result<()> {
        // Drop writer to flush
        drop(self.writer.take());
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if let (Some(temp_path), Some(final_path)) = (&self.temp_path, &self.final_path) {
            std::fs::rename(temp_path, final_path).with_context(|| {
                format!(
                    "Failed to rename {} -> {}",
                    temp_path.display(),
                    final_path.display()
                )
            })?;
            info!(
                "Committed CSV sink: {} ({} rows)",
                final_path.display(),
                self.rows_written
            );
            self.committed = true;
        }
        self.temp_path = None;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if self.committed {
            if let Some(final_path) = &self.final_path {
                if final_path.exists() {
                    let _ = std::fs::remove_file(final_path);
                    warn!("Rolled back CSV committed file: {}", final_path.display());
                }
            }
        }
        if let Some(temp_path) = &self.temp_path {
            if temp_path.exists() {
                let _ = std::fs::remove_file(temp_path);
                warn!("Rolled back CSV temp file: {}", temp_path.display());
            }
        }
        self.temp_path = None;
        self.final_path = None;
        self.committed = false;
        Ok(())
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

enum Sink {
    Parquet(ParquetSink),
    Csv(Box<CsvSink>),
    Relational(RelationalSink),
}

impl Sink {
    fn init(&mut self, schema: &Schema) -> Result<()> {
        match self {
            Sink::Parquet(sink) => sink.init(schema),
            Sink::Csv(sink) => sink.init(schema),
            Sink::Relational(sink) => sink.init(schema),
        }
    }

    fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        match self {
            Sink::Parquet(sink) => sink.write_batch(batch),
            Sink::Csv(sink) => sink.write_batch(batch),
            Sink::Relational(sink) => sink.write_batch(batch),
        }
    }

    fn prepare(&mut self) -> Result<()> {
        match self {
            Sink::Parquet(sink) => sink.prepare(),
            Sink::Csv(sink) => sink.prepare(),
            Sink::Relational(sink) => sink.prepare(),
        }
    }

    fn commit(&mut self) -> Result<()> {
        match self {
            Sink::Parquet(sink) => sink.commit(),
            Sink::Csv(sink) => sink.commit(),
            Sink::Relational(sink) => sink.commit(),
        }
    }

    fn rollback(&mut self) -> Result<()> {
        match self {
            Sink::Parquet(sink) => sink.rollback(),
            Sink::Csv(sink) => sink.rollback(),
            Sink::Relational(sink) => sink.rollback(),
        }
    }
}

/// Sink registry - manages multiple sinks for a run
pub struct SinkRegistry {
    sinks: HashMap<String, Sink>,
}

impl Default for SinkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SinkRegistry {
    pub fn new() -> Self {
        Self {
            sinks: HashMap::new(),
        }
    }

    /// Add a sink for an output name
    pub(crate) fn add(&mut self, name: &str, sink: Sink) {
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

    /// Finish all sinks using prepare/commit with rollback on failure.
    pub fn finish(self) -> Result<()> {
        self.finish_with_guard(None)
    }

    /// Finish all sinks with an optional commit guard.
    ///
    /// If the guard returns false, all sinks are rolled back.
    pub fn finish_with_guard(mut self, should_commit: Option<&dyn Fn() -> bool>) -> Result<()> {
        let mut names: Vec<String> = self.sinks.keys().cloned().collect();
        names.sort();

        for name in &names {
            if let Some(sink) = self.sinks.get_mut(name) {
                debug!("Preparing sink: {}", name);
                sink.prepare()?;
            }
        }

        if let Some(guard) = should_commit {
            if !guard() {
                warn!("Sink commit aborted by guard; rolling back");
                for name in &names {
                    if let Some(sink) = self.sinks.get_mut(name) {
                        let _ = sink.rollback();
                    }
                }
                bail!("Output commit aborted");
            }
        }

        let commit_result: Result<()> = (|| {
            for name in &names {
                if let Some(sink) = self.sinks.get_mut(name) {
                    debug!("Committing sink: {}", name);
                    sink.commit()?;
                }
            }
            Ok(())
        })();

        if let Err(err) = commit_result {
            warn!("Sink commit failed, rolling back: {}", err);
            for name in &names {
                if let Some(sink) = self.sinks.get_mut(name) {
                    let _ = sink.rollback();
                }
            }
            return Err(err);
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
pub(crate) fn validate_batch_schema(
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
    for (i, (batch_field, declared_field)) in batch_schema
        .fields()
        .iter()
        .zip(declared_schema.fields().iter())
        .enumerate()
    {
        // Check name
        if batch_field.name() != declared_field.name() {
            bail!(
                "Schema mismatch for sink '{}' column {}: expected name '{}', got '{}'",
                sink_name,
                i,
                declared_field.name(),
                batch_field.name()
            );
        }

        // Check data type (allow some compatible type coercions)
        if !types_compatible(batch_field.data_type(), declared_field.data_type()) {
            bail!(
                "Schema mismatch for sink '{}' column '{}': expected type {:?}, got {:?}",
                sink_name,
                declared_field.name(),
                declared_field.data_type(),
                batch_field.data_type()
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

/// Inject lineage columns into an OutputBatch
///
/// Adds:
/// - _cf_source_hash: Blake3 hash of the source file
/// - _cf_job_id: Unique ID for this processing run
/// - _cf_processed_at: ISO 8601 timestamp of when the record was processed
/// - _cf_parser_version: Parser version that processed this record
pub fn inject_lineage_columns(
    batch: &OutputBatch,
    source_hash: &str,
    job_id: &str,
    parser_version: &str,
) -> Result<OutputBatch> {
    let batch = batch.record_batch();
    let num_rows = batch.num_rows();

    // Current timestamp in ISO 8601 format
    let processed_at = chrono::Utc::now().to_rfc3339();

    // Create lineage column arrays
    let source_hash_array: ArrayRef = Arc::new(StringArray::from(vec![source_hash; num_rows]));
    let job_id_array: ArrayRef = Arc::new(StringArray::from(vec![job_id; num_rows]));
    let processed_at_array: ArrayRef =
        Arc::new(StringArray::from(vec![processed_at.as_str(); num_rows]));
    let parser_version_array: ArrayRef =
        Arc::new(StringArray::from(vec![parser_version; num_rows]));

    // Build new schema with lineage columns
    let mut fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect();
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

    let batch = RecordBatch::try_new(new_schema, columns)
        .context("Failed to create batch with lineage columns")?;
    Ok(OutputBatch::from_record_batch(batch))
}

/// Create a sink from a SinkUri
pub(crate) fn create_sink_from_uri(
    uri: &str,
    output_name: &str,
    output_table: Option<&str>,
    sink_mode: SinkMode,
    job_id: &str,
) -> Result<Sink> {
    let parsed =
        casparian_protocol::types::ParsedSinkUri::parse(uri).map_err(|e| anyhow::anyhow!(e))?;
    let table_name = output_table.unwrap_or(output_name);

    match parsed.scheme {
        casparian_protocol::types::SinkScheme::Parquet => {
            if sink_mode != SinkMode::Append {
                bail!(
                    "Parquet sink does not support {:?} mode (only Append)",
                    sink_mode
                );
            }
            Ok(Sink::Parquet(ParquetSink::new(
                parsed.path,
                output_name,
                job_id,
            )?))
        }
        casparian_protocol::types::SinkScheme::Csv => {
            if sink_mode != SinkMode::Append {
                bail!(
                    "CSV sink does not support {:?} mode (only Append)",
                    sink_mode
                );
            }
            Ok(Sink::Csv(Box::new(CsvSink::new(
                parsed.path,
                output_name,
                job_id,
            )?)))
        }
        casparian_protocol::types::SinkScheme::Duckdb => Ok(Sink::Relational(RelationalSink::new(
            RelationalBackend::DuckDb(DuckDbSink::new(
                parsed.path,
                table_name,
                sink_mode,
                job_id,
                output_name,
            )?),
        ))),
        casparian_protocol::types::SinkScheme::File => {
            // File sink: infer by extension
            let ext = parsed
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            match ext {
                "parquet" => {
                    if sink_mode != SinkMode::Append {
                        bail!(
                            "Parquet sink does not support {:?} mode (only Append)",
                            sink_mode
                        );
                    }
                    Ok(Sink::Parquet(ParquetSink::new(
                        parsed
                            .path
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_path_buf(),
                        output_name,
                        job_id,
                    )?))
                }
                "csv" => {
                    if sink_mode != SinkMode::Append {
                        bail!(
                            "CSV sink does not support {:?} mode (only Append)",
                            sink_mode
                        );
                    }
                    Ok(Sink::Csv(Box::new(CsvSink::new(
                        parsed
                            .path
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_path_buf(),
                        output_name,
                        job_id,
                    )?)))
                }
                "duckdb" | "db" => Ok(Sink::Relational(RelationalSink::new(
                    RelationalBackend::DuckDb(DuckDbSink::new(
                        parsed.path,
                        table_name,
                        sink_mode,
                        job_id,
                        output_name,
                    )?),
                ))),
                _ => bail!("Unsupported file sink extension: '{}'", ext),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{
        Array, Decimal128Array, Decimal128Builder, Int64Array, StringArray,
        TimestampMicrosecondArray,
    };
    use arrow::datatypes::{Field, TimeUnit};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::fs::File;
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

        sink.prepare().unwrap();
        sink.commit().unwrap();

        // Verify partitioned file exists
        let output_path = dir.path().join(output_filename("test", job_id, "parquet"));
        assert!(output_path.exists());

        // Verify temp file was cleaned up
        let temp_path = dir.path().join(format!(
            ".{}.tmp",
            output_filename("test", job_id, "parquet")
        ));
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_parquet_sink_decimal_timestamp_tz_roundtrip() {
        let dir = tempdir().unwrap();
        let job_id = "12345678-abcd-1234-abcd-123456789abc";
        let mut sink =
            ParquetSink::new(dir.path().to_path_buf(), "test_decimal_tz", job_id).unwrap();

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

        sink.prepare().unwrap();
        sink.commit().unwrap();

        let output_path = dir
            .path()
            .join(output_filename("test_decimal_tz", job_id, "parquet"));
        assert!(output_path.exists());

        let file = File::open(&output_path).unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let arrow_schema = builder.schema().clone();

        assert_eq!(arrow_schema.field(0).name(), "amount");
        assert_eq!(
            arrow_schema.field(0).data_type(),
            &DataType::Decimal128(10, 2)
        );
        assert_eq!(arrow_schema.field(1).name(), "event_time");
        assert_eq!(
            arrow_schema.field(1).data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
        );

        let mut reader = builder.with_batch_size(10).build().unwrap();
        let read_batch = reader.next().unwrap().unwrap();

        let dec_read = read_batch
            .column(0)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap();
        assert_eq!(dec_read.value(0), 12_345);
        assert!(dec_read.is_null(1));
        assert_eq!(dec_read.value(2), -6_789);

        let ts_read = read_batch
            .column(1)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert_eq!(ts_read.value(0), 1_700_000_000_000_000);
        assert!(ts_read.is_null(1));
        assert_eq!(ts_read.value(2), 1_700_000_100_000_000);
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

        sink.prepare().unwrap();
        sink.commit().unwrap();

        // Verify partitioned file exists and has content
        let output_path = dir.path().join(output_filename("test", job_id, "csv"));
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("id,name"));
        assert!(content.contains("Alice"));

        // Verify temp file was cleaned up
        let temp_path = dir
            .path()
            .join(format!(".{}.tmp", output_filename("test", job_id, "csv")));
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_duckdb_sink() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.duckdb");
        let mut sink = DuckDbSink::new(
            db_path.clone(),
            "records",
            SinkMode::Append,
            "job-1",
            "records",
        )
        .unwrap();

        let batch = create_test_batch();
        sink.init(batch.schema().as_ref()).unwrap();
        let rows = sink.write_batch(&batch).unwrap();
        assert_eq!(rows, 3);

        sink.prepare().unwrap();
        sink.commit().unwrap();

        let conn = duckdb::Connection::open(db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_duckdb_sink_lock_conflict() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("locked.duckdb");

        let _sink1 = DuckDbSink::new(
            db_path.clone(),
            "records",
            SinkMode::Append,
            "job-1",
            "records",
        )
        .unwrap();

        let err = match DuckDbSink::new(db_path, "records", SinkMode::Append, "job-2", "records") {
            Ok(_) => panic!("expected lock error, got Ok"),
            Err(err) => err,
        };
        assert!(
            err.to_string().to_lowercase().contains("locked"),
            "expected lock error, got: {}",
            err
        );
    }

    #[test]
    fn test_duckdb_sink_rejects_control_plane_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("casparian_flow.duckdb");

        let err = match DuckDbSink::new(db_path, "records", SinkMode::Append, "job-1", "records") {
            Ok(_) => panic!("expected control-plane rejection, got Ok"),
            Err(err) => err,
        };
        assert!(
            err.to_string().to_lowercase().contains("control-plane"),
            "expected control-plane rejection, got: {}",
            err
        );
    }

    #[test]
    fn test_duckdb_sink_decimal_timestamp_tz() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_decimal_tz.duckdb");
        let mut sink = DuckDbSink::new(
            db_path.clone(),
            "records",
            SinkMode::Append,
            "job-1",
            "records",
        )
        .unwrap();

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

        sink.prepare().unwrap();
        sink.commit().unwrap();

        let conn = duckdb::Connection::open(db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_inject_lineage_columns() {
        let batch = OutputBatch::from_record_batch(create_test_batch());
        let with_lineage = inject_lineage_columns(&batch, "abc123", "job-456", "1.0.0").unwrap();
        let with_lineage = with_lineage.record_batch();

        // Original 2 columns + 4 lineage columns
        assert_eq!(with_lineage.num_columns(), 6);
        assert!(with_lineage
            .schema()
            .field_with_name("_cf_source_hash")
            .is_ok());
        assert!(with_lineage.schema().field_with_name("_cf_job_id").is_ok());
        assert!(with_lineage
            .schema()
            .field_with_name("_cf_processed_at")
            .is_ok());
        assert!(with_lineage
            .schema()
            .field_with_name("_cf_parser_version")
            .is_ok());

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

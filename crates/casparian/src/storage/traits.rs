//! Storage abstraction traits for Casparian Flow.
//!
//! This module defines the core storage interfaces for job management,
//! parser storage, and quarantine handling. These traits enable swapping
//! storage backends (SQLite, PostgreSQL, etc.) without changing application code.

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

/// A job from the processing queue.
#[derive(Debug, Clone)]
pub struct Job {
    /// Unique job identifier.
    pub id: i64,
    /// Reference to the file being processed.
    pub file_id: i64,
    /// Name of the plugin/parser to execute.
    pub plugin_name: String,
    /// Current job status (QUEUED, RUNNING, COMPLETE, FAILED).
    pub status: String,
    /// Number of retry attempts.
    pub retry_count: i32,
    /// Error message if the job failed.
    pub error_message: Option<String>,
}

/// A parser bundle containing the packaged parser artifact.
#[derive(Debug, Clone)]
pub struct ParserBundle {
    /// Logical parser name.
    pub name: String,
    /// Semver version string.
    pub version: String,
    /// ZIP archive containing the parser code.
    pub archive: Vec<u8>,
    /// Blake3 hash of the parser source.
    pub source_hash: String,
    /// Hash of the lockfile for reproducibility.
    pub lockfile_hash: String,
    /// Full lockfile content (uv.lock).
    pub lockfile_content: String,
}

/// A row that was quarantined due to parsing errors.
#[derive(Debug, Clone)]
pub struct QuarantinedRow {
    /// Unique quarantine record identifier.
    pub id: i64,
    /// Reference to the job that produced this quarantine.
    pub job_id: i64,
    /// Zero-based row index in the source file.
    pub row_index: usize,
    /// Reason for quarantine (error message).
    pub error_reason: String,
    /// Raw data of the quarantined row.
    pub raw_data: Vec<u8>,
}

/// A selection specification defining how files are chosen.
#[derive(Debug, Clone)]
pub struct SelectionSpec {
    pub id: String,
    pub spec_json: String,
    pub created_at: String,
}

/// A resolved snapshot of files for a logical execution date.
#[derive(Debug, Clone)]
pub struct SelectionSnapshot {
    pub id: String,
    pub spec_id: String,
    pub snapshot_hash: String,
    pub logical_date: String,
    pub watermark_value: Option<String>,
    pub created_at: String,
}

/// Optional watermark field used for incremental selections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkField {
    Mtime,
}

/// Structured filters for selecting files from the catalog.
#[derive(Debug, Clone, Default)]
pub struct SelectionFilters {
    pub source_id: Option<String>,
    pub tag: Option<String>,
    pub extension: Option<String>,
    pub since_ms: Option<i64>,
    pub watermark: Option<WatermarkField>,
}

/// Result of resolving a selection against a logical execution time.
#[derive(Debug, Clone)]
pub struct SelectionResolution {
    pub file_ids: Vec<i64>,
    pub watermark_value: Option<String>,
}

/// A pipeline configuration with versioning.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub config_json: String,
    pub created_at: String,
}

/// A single pipeline run for a logical execution date.
#[derive(Debug, Clone)]
pub struct PipelineRun {
    pub id: String,
    pub pipeline_id: String,
    pub selection_spec_id: String,
    pub selection_snapshot_hash: String,
    pub context_snapshot_hash: Option<String>,
    pub logical_date: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Job store trait for managing the processing queue.
///
/// Handles job lifecycle: claim, heartbeat, complete, fail, and stale recovery.
#[async_trait]
pub trait JobStore: Send + Sync {
    /// Enqueue a new job for processing.
    async fn enqueue_job(&self, file_id: i64, plugin_name: &str, priority: i32) -> Result<i64>;
    /// Claim the next available job for processing.
    ///
    /// Returns `None` if no jobs are available.
    /// Jobs are claimed atomically to prevent double-processing.
    async fn claim_next(&self, worker_id: &str) -> Result<Option<Job>>;

    /// Send a heartbeat to indicate the job is still being processed.
    ///
    /// Workers should call this periodically to prevent stale detection.
    async fn heartbeat(&self, job_id: i64) -> Result<()>;

    /// Mark a job as complete with the output path.
    async fn complete(&self, job_id: i64, output_path: &str) -> Result<()>;

    /// Mark a job as failed with an error message.
    ///
    /// If `retry_eligible` is true, the job may be retried later.
    async fn fail(&self, job_id: i64, error: &str, retry_eligible: bool) -> Result<()>;

    /// Requeue jobs that have gone stale (no heartbeat within threshold).
    ///
    /// Returns the number of jobs requeued.
    async fn requeue_stale(&self, stale_threshold: Duration) -> Result<usize>;
}

/// Parser store trait for managing parser bundles.
///
/// Handles parser artifact storage and topic subscriptions.
#[async_trait]
pub trait ParserStore: Send + Sync {
    /// Get a parser bundle by name and version.
    ///
    /// Returns `None` if the parser is not found.
    async fn get(&self, name: &str, version: &str) -> Result<Option<ParserBundle>>;

    /// Insert a new parser bundle.
    ///
    /// Returns an error if a parser with the same name and version already exists.
    async fn insert(&self, bundle: ParserBundle) -> Result<()>;

    /// Get all topics that a parser subscribes to.
    async fn get_topics(&self, parser_name: &str) -> Result<Vec<String>>;
}

/// Quarantine store trait for managing failed row data.
///
/// Rows that fail parsing are quarantined for later inspection and reprocessing.
#[async_trait]
pub trait QuarantineStore: Send + Sync {
    /// Quarantine a row that failed parsing.
    async fn quarantine_row(
        &self,
        job_id: i64,
        row_idx: usize,
        error: &str,
        data: &[u8],
    ) -> Result<()>;

    /// Get all quarantined rows for a job.
    async fn get_quarantined(&self, job_id: i64) -> Result<Vec<QuarantinedRow>>;
}

/// Pipeline store trait for selection specs, snapshots, and pipeline runs.
#[async_trait]
pub trait PipelineStore: Send + Sync {
    async fn create_selection_spec(&self, spec_json: &str) -> Result<String>;
    async fn create_selection_snapshot(
        &self,
        spec_id: &str,
        snapshot_hash: &str,
        logical_date: &str,
        watermark_value: Option<&str>,
    ) -> Result<String>;
    async fn insert_snapshot_files(&self, snapshot_id: &str, file_ids: &[i64]) -> Result<()>;
    async fn create_pipeline(&self, name: &str, version: i64, config_json: &str) -> Result<String>;
    async fn get_latest_pipeline(&self, name: &str) -> Result<Option<Pipeline>>;
    async fn create_pipeline_run(
        &self,
        pipeline_id: &str,
        selection_spec_id: &str,
        selection_snapshot_hash: &str,
        context_snapshot_hash: Option<&str>,
        logical_date: &str,
        status: &str,
    ) -> Result<String>;
    async fn set_pipeline_run_status(&self, run_id: &str, status: &str) -> Result<()>;
    async fn pipeline_run_exists(&self, pipeline_id: &str, logical_date: &str) -> Result<bool>;
    async fn resolve_selection_files(
        &self,
        filters: &SelectionFilters,
        logical_date_ms: i64,
    ) -> Result<SelectionResolution>;
}

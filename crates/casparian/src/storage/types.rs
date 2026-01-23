use crate::scout::SourceId;
use casparian_protocol::PipelineRunStatus;

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
    pub source_id: Option<SourceId>,
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
    pub status: PipelineRunStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

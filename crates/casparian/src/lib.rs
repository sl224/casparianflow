//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod ai;
pub mod bundler;
pub mod publish;
pub mod runner;
pub mod scout;
pub mod storage;

#[path = "cli/tui/extraction.rs"]
pub mod tui_extraction;

pub use bundler::{bundle_parser, ParserBundle};
pub use casparian_sinks as sinks;
pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};
pub use storage::{
    Job, JobStore, ParserStore, Pipeline, PipelineRun, PipelineStore, QuarantinedRow,
    QuarantineStore, SelectionFilters, SelectionResolution, SelectionSnapshot, SelectionSpec,
    SqliteJobStore, SqliteParserStore, SqliteQuarantineStore, WatermarkField,
};

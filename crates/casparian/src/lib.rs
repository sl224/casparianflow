//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod ai;
pub mod bundler;
pub mod parser_metadata;
pub mod publish;
pub mod runner;
pub mod scout;
pub mod storage;
pub mod trust;

#[path = "cli/tui/extraction.rs"]
pub mod tui_extraction;

pub use bundler::{bundle_parser, ParserBundle};
pub use casparian_sinks as sinks;
pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};
pub use storage::{
    Pipeline, PipelineRun, SelectionFilters, SelectionResolution, SelectionSnapshot, WatermarkField,
};

// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::single_match)]
#![allow(clippy::type_complexity)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::incompatible_msrv)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::search_is_some)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::infallible_destructuring_match)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::to_string_in_format_args)]
#![allow(clippy::manual_ok_err)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_char_add_str)]
#![allow(clippy::collapsible_str_replace)]

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
pub mod telemetry;
pub mod trust;

#[path = "cli/tui/extraction.rs"]
pub mod tui_extraction;

pub use bundler::{bundle_parser, ParserBundle};
pub use casparian_sinks as sinks;
pub use publish::{
    analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions,
    PublishReceipt,
};
pub use storage::{
    Pipeline, PipelineRun, SelectionFilters, SelectionResolution, SelectionSnapshot, WatermarkField,
};

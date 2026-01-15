//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod ai;
pub mod bundler;
pub mod publish;
pub mod runner;
pub mod scout;
pub mod storage;

pub use bundler::{bundle_parser, ParserBundle};
pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};
pub use storage::{
    Job, JobStore, ParserStore, QuarantinedRow, QuarantineStore,
    SqliteJobStore, SqliteParserStore, SqliteQuarantineStore,
};

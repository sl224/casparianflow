//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod publish;
pub mod storage;

pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};
pub use storage::{
    Job, JobStore, ParserBundle, ParserStore, QuarantinedRow, QuarantineStore,
    SqliteJobStore, SqliteParserStore, SqliteQuarantineStore,
};

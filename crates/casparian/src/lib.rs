//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod publish;

pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};

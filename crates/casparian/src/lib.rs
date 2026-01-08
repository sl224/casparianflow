//! Casparian Flow - Core Library
//!
//! Shared functionality for the unified launcher.

pub mod bundler;
pub mod publish;

pub use bundler::{bundle_parser, ParserBundle};
pub use publish::{analyze_plugin, prepare_publish, PluginAnalysis, PreparedArtifact, PublishOptions, PublishReceipt};

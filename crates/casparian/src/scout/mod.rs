//! Scout - File Discovery & Tagging
//!
//! Scout discovers files from filesystem sources and assigns tags based on patterns.
//! This is the local module, consolidated from the former casparian_scout crate.

pub mod db;
pub mod error;
pub mod scanner;
pub mod tagger;
pub mod types;

// Re-exports for CLI usage
pub use db::Database;
pub use scanner::{ScanConfig, ScanProgress, Scanner};
pub use types::{FileStatus, ScannedFile, Source, SourceType, TaggingRule};

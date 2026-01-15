//! Scout - File Discovery & Tagging
//!
//! Scout discovers files from filesystem sources and assigns tags based on patterns.
//! This is the local module, consolidated from the former casparian_scout crate.

pub mod db;
pub mod error;
pub mod folder_cache;
pub mod scanner;
pub mod tagger;
pub mod types;

// Re-exports for CLI usage
pub use db::{Database, FolderEntry};
pub use folder_cache::FolderCache;
pub use scanner::{
    compute_folder_deltas_from_files, compute_folder_deltas_from_paths,
    merge_folder_deltas, FolderDelta, ScanConfig, ScanProgress, Scanner,
};
pub use types::{FileStatus, ScannedFile, Source, SourceType, TaggingRule};

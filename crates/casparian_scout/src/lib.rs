//! Casparian Scout - File Discovery & Tagging Layer
//!
//! Scout watches filesystem locations, discovers files, and assigns tags based on patterns.
//! Actual file processing happens in Sentinel (Tag → Plugin → Sink).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌───────────────┐
//! │   Source    │     │   SQLite    │     │   Tagger    │     │   Sentinel    │
//! │  (SMB/S3)   │────▶│  (state DB) │────▶│ (pattern →  │────▶│ (Tag→Plugin→  │
//! │             │     │             │     │    tag)     │     │     Sink)     │
//! └─────────────┘     └─────────────┘     └─────────────┘     └───────────────┘
//! ```
//!
//! # Core Concepts
//!
//! - **Source**: A filesystem location to watch (local, SMB, S3)
//! - **TaggingRule**: Pattern → Tag mapping
//! - **File**: Discovered file with status and assigned tag
//! - **Sentinel Handoff**: Tagged files are submitted to Sentinel for processing

pub mod config;
pub mod db;
pub mod error;
pub mod router;
pub mod scanner;
pub mod types;

// Re-exports for convenience
pub use config::ScoutConfig;
pub use db::Database;
pub use error::{Result, ScoutError};
pub use router::Tagger;
pub use scanner::{ScanResult, ScanScheduler, Scanner};
pub use types::{
    DbStats, FileStatus, ScanStats, ScannedFile, Source, SourceType, TaggingRule, UpsertResult,
};

//! Storage abstraction layer for Casparian Flow.
//!
//! This module provides traits for storage operations (JobStore, ParserStore,
//! QuarantineStore) along with SQLite implementations. The abstraction allows
//! swapping storage backends without changing application code.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian::storage::{DuckDbPipelineStore, PipelineStore};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let store = DuckDbPipelineStore::open(Path::new("./data.duckdb")).await?;
//!
//!     let pipeline = store.get_latest_pipeline("daily_report").await?;
//!     println!("Latest pipeline: {:?}", pipeline);
//!     Ok(())
//! }
//! ```

mod duckdb;
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use duckdb::DuckDbPipelineStore;
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteJobStore, SqliteParserStore, SqliteQuarantineStore};
pub use traits::{
    Job, JobStore, ParserBundle, ParserStore, Pipeline, PipelineRun, PipelineStore, QuarantinedRow,
    QuarantineStore, SelectionFilters, SelectionResolution, SelectionSnapshot, SelectionSpec,
    WatermarkField,
};

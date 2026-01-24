//! Storage abstraction layer for Casparian Flow.
//!
//! Pipeline storage utilities for Casparian Flow.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian::storage::DuckDbPipelineStore;
//! use std::path::Path;
//!
//! fn main() -> anyhow::Result<()> {
//!     let store = DuckDbPipelineStore::open(Path::new("./data.duckdb"))?;
//!
//!     let pipeline = store.get_latest_pipeline("daily_report")?;
//!     println!("Latest pipeline: {:?}", pipeline);
//!     Ok(())
//! }
//! ```

mod duckdb;
mod types;

pub use duckdb::DuckDbPipelineStore;
pub use types::{
    Pipeline, PipelineRun, SelectionFilters, SelectionResolution, SelectionSnapshot, WatermarkField,
};

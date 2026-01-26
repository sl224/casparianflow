//! Storage abstraction layer for Casparian Flow.
//!
//! Pipeline storage utilities for Casparian Flow.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian::storage::PipelineStore;
//! use std::path::Path;
//!
//! fn main() -> anyhow::Result<()> {
//!     let store = PipelineStore::open(Path::new("./state.sqlite"))?;
//!
//!     let pipeline = store.get_latest_pipeline("daily_report")?;
//!     println!("Latest pipeline: {:?}", pipeline);
//!     Ok(())
//! }
//! ```

mod pipeline_store;
mod types;

pub use pipeline_store::PipelineStore;
pub use types::{
    Pipeline, PipelineRun, SelectionFilters, SelectionResolution, SelectionSnapshot, WatermarkField,
};

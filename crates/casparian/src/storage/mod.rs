//! Storage abstraction layer for Casparian Flow.
//!
//! This module provides traits for storage operations (JobStore, ParserStore,
//! QuarantineStore) along with SQLite implementations. The abstraction allows
//! swapping storage backends without changing application code.
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian::storage::{SqliteJobStore, JobStore};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let store = SqliteJobStore::new(Path::new("./data.db")).await?;
//!
//!     if let Some(job) = store.claim_next("worker-1").await? {
//!         println!("Claimed job: {} for plugin {}", job.id, job.plugin_name);
//!         // Process the job...
//!         store.complete(job.id, "/output/result.parquet").await?;
//!     }
//!     Ok(())
//! }
//! ```

mod sqlite;
mod traits;

pub use sqlite::{SqliteJobStore, SqliteParserStore, SqliteQuarantineStore};
pub use traits::{Job, JobStore, ParserBundle, ParserStore, QuarantinedRow, QuarantineStore};

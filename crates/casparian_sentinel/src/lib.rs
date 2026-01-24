//! Casparian Flow Sentinel library
//!
//! Exposes database and sentinel modules for testing and library usage.

// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::get_first)]
#![allow(dead_code)]

pub mod control;
pub mod control_client;
pub mod db;
pub mod metrics;
pub mod sentinel;

pub use control::{ControlRequest, ControlResponse, JobInfo, QueueStatsInfo};
pub use control_client::ControlClient;
pub use db::api_storage::ApiStorage;
pub use db::expected_outputs::{ExpectedOutputs, OutputSpec};
pub use db::{
    queue::{Job, JobDetails, PluginDetails, QueueStats},
    JobQueue,
};
pub use metrics::METRICS;
pub use sentinel::{Sentinel, SentinelConfig};

#[derive(clap::Parser, Debug)]
#[command(
    name = "casparian-sentinel",
    about = "Rust Sentinel for Casparian Flow"
)]
pub struct SentinelArgs {
    /// ZMQ bind address for workers
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    pub bind: String,

    /// Database connection string
    #[arg(long, default_value = "duckdb:casparian_flow.duckdb")]
    pub database: String,

    /// Maximum number of workers (default 4, hard cap 8)
    #[arg(long, default_value_t = 4)]
    pub max_workers: usize,

    /// Control API bind address (e.g., "ipc:///tmp/casparian_control.sock" or "tcp://127.0.0.1:5556")
    /// If not specified, control API is disabled.
    #[arg(long)]
    pub control_api: Option<String>,
}

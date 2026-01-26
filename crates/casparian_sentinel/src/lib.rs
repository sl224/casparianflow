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

pub use control::{
    ControlRequest, ControlResponse, JobInfo, QueueStatsInfo, ScoutRuleInfo, ScoutScanProgress,
    ScoutScanStatus, ScoutSourceInfo, ScoutTagCount, ScoutTagStats, ScanState, DEFAULT_CONTROL_ADDR,
};
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
    #[arg(
        long,
        default_value_t = casparian_protocol::defaults::DEFAULT_SENTINEL_BIND_ADDR.to_string()
    )]
    pub bind: String,

    /// State store URL (sqlite:/... | postgres://... | sqlserver://...)
    #[arg(
        long,
        default_value_t = casparian_protocol::defaults::DEFAULT_STATE_STORE_URL.to_string()
    )]
    pub state_store: String,

    /// Query catalog path (DuckDB file over Parquet)
    #[arg(long)]
    pub query_catalog: Option<std::path::PathBuf>,

    /// Maximum number of workers (default 4, hard cap 8)
    #[arg(long, default_value_t = 4)]
    pub max_workers: usize,

    /// Control API bind address (e.g., "ipc:///tmp/casparian_control.sock" or "tcp://127.0.0.1:5556")
    /// If not specified, defaults to tcp://127.0.0.1:5556 unless --no-control-api is set.
    #[arg(long)]
    pub control_addr: Option<String>,

    /// Disable the Control API entirely.
    #[arg(long)]
    pub no_control_api: bool,
}

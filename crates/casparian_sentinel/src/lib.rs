//! Casparian Flow Sentinel library
//!
//! Exposes database and sentinel modules for testing and library usage.

pub mod db;
pub mod metrics;
pub mod sentinel;

pub use db::{JobQueue, queue::{JobDetails, PluginDetails}};
pub use metrics::METRICS;
pub use sentinel::{Sentinel, SentinelConfig};

#[derive(clap::Parser, Debug)]
#[command(name = "casparian-sentinel", about = "Rust Sentinel for Casparian Flow")]
pub struct SentinelArgs {
    /// ZMQ bind address for workers
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    pub bind: String,

    /// Database connection string
    #[arg(long, default_value = "sqlite://casparian_flow.db")]
    pub database: String,
}

// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(clippy::result_large_err)]
#![allow(clippy::if_same_then_else)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(clippy::new_without_default)]
#![allow(clippy::while_let_loop)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::len_zero)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::redundant_guards)]
#![allow(clippy::needless_borrows_for_generic_args)]

pub mod bridge;
pub mod cancel;
pub mod metrics;
pub mod native_runtime;
pub mod runtime;
mod schema_validation;
pub mod type_inference;
pub mod venv_manager;
pub mod worker;

pub use metrics::METRICS;
pub use worker::{Worker, WorkerConfig, WorkerError, WorkerHandle};

#[derive(clap::Parser, Debug)]
#[command(name = "casparian-worker", about = "Rust Worker for Casparian Flow")]
pub struct WorkerArgs {
    /// Sentinel address
    #[arg(
        long,
        default_value_t = casparian_protocol::defaults::DEFAULT_SENTINEL_BIND_ADDR.to_string()
    )]
    pub connect: String,

    /// Parquet output directory
    #[arg(long, default_value = "output")]
    pub output: std::path::PathBuf,

    /// Worker ID (auto-generated if not provided)
    #[arg(long)]
    pub worker_id: Option<String>,
}

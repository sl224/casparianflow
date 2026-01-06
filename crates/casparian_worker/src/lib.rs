pub mod analyzer;
pub mod bridge;
pub mod metrics;
pub mod shredder;
pub mod type_inference;
pub mod venv_manager;
pub mod worker;

pub use metrics::METRICS;
pub use worker::{Worker, WorkerConfig};

#[derive(clap::Parser, Debug)]
#[command(name = "casparian-worker", about = "Rust Worker for Casparian Flow")]
pub struct WorkerArgs {
    /// Sentinel address
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    pub connect: String,

    /// Parquet output directory
    #[arg(long, default_value = "output")]
    pub output: std::path::PathBuf,

    /// Worker ID (auto-generated if not provided)
    #[arg(long)]
    pub worker_id: Option<String>,
}

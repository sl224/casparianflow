//! Casparian Flow Worker (Rust)
//!
//! Usage:
//!     casparian-worker --connect tcp://127.0.0.1:5555 --output ./output

mod bridge;
mod venv_manager;
mod worker;

use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use worker::{Worker, WorkerConfig};

#[derive(Parser, Debug)]
#[command(name = "casparian-worker", about = "Rust Worker for Casparian Flow")]
struct Args {
    /// Sentinel address
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    connect: String,

    /// Parquet output directory
    #[arg(long, default_value = "output")]
    output: PathBuf,

    /// Worker ID (auto-generated if not provided)
    #[arg(long)]
    worker_id: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casparian_worker=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Find bridge_shim.py at startup (fail fast if missing)
    let shim_path = bridge::find_bridge_shim()?;
    tracing::info!("Found bridge_shim.py: {}", shim_path.display());

    // Generate worker ID if not provided
    let worker_id = args.worker_id.unwrap_or_else(|| {
        format!("rust-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap())
    });

    let config = WorkerConfig {
        sentinel_addr: args.connect.clone(),
        parquet_root: args.output.clone(),
        worker_id: worker_id.clone(),
        shim_path,
    };

    tracing::info!("Starting Casparian Rust Worker");
    tracing::info!("  Sentinel: {}", args.connect);
    tracing::info!("  Output: {}", args.output.display());
    tracing::info!("  Worker ID: {}", worker_id);

    // Connect and run
    let mut worker = Worker::connect(config).await?;
    worker.run().await?;

    Ok(())
}

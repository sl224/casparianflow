//! Casparian Flow Sentinel (Rust)
//!
//! Control plane for job orchestration and worker management.
//!
//! Usage:
//!     casparian-sentinel --bind tcp://127.0.0.1:5555 --database sqlite:///path/to/db.sqlite

use casparian_sentinel::{Sentinel, SentinelConfig};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "casparian-sentinel", about = "Rust Sentinel for Casparian Flow")]
struct Args {
    /// ZMQ bind address for workers
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    bind: String,

    /// Database connection string
    #[arg(long, default_value = "sqlite://casparian_flow.db")]
    database: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casparian_sentinel=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    tracing::info!("Starting Casparian Rust Sentinel");
    tracing::info!("  Bind: {}", args.bind);
    tracing::info!("  Database: {}", args.database);

    let config = SentinelConfig {
        bind_addr: args.bind,
        database_url: args.database,
    };

    // Bind and run
    let mut sentinel = Sentinel::bind(config).await?;
    sentinel.run().await?;

    Ok(())
}

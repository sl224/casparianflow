use anyhow::Result;
use casparian_sentinel::{Sentinel, SentinelConfig, SentinelArgs};
use casparian_worker::{Worker, WorkerConfig, WorkerArgs, bridge};
use clap::{Parser, Subcommand};
use tokio::sync::oneshot;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "casparian", about = "Unified Launcher for Casparian Flow")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start both Sentinel and Worker in one process
    Start {
        /// ZMQ bind/connect address
        #[arg(long, default_value = "tcp://127.0.0.1:5555")]
        addr: String,

        /// Database connection string
        #[arg(long, default_value = "sqlite://casparian_flow.db")]
        database: String,

        /// Parquet output directory
        #[arg(long, default_value = "output")]
        output: std::path::PathBuf,
    },
    /// Start only the Sentinel
    Sentinel {
        #[command(flatten)]
        args: SentinelArgs,
    },
    /// Start only the Worker
    Worker {
        #[command(flatten)]
        args: WorkerArgs,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casparian=info,casparian_sentinel=info,casparian_worker=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { addr, database, output } => {
            tracing::info!("Starting Unified Casparian Stack (Sentinel + Worker)");

            // Use oneshot channel for proper synchronization (no sleep race condition)
            let (ready_tx, ready_rx) = oneshot::channel::<()>();

            let sentinel_config = SentinelConfig {
                bind_addr: addr.clone(),
                database_url: database,
            };

            // Start Sentinel in a task
            let sentinel_handle = tokio::spawn(async move {
                let mut sentinel = Sentinel::bind(sentinel_config).await?;
                // Signal that sentinel is ready (socket bound)
                let _ = ready_tx.send(());
                sentinel.run().await
            });

            // Prepare Worker config
            let shim_path = bridge::find_bridge_shim()?;
            let worker_id = format!(
                "rust-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            );
            let worker_config = WorkerConfig {
                sentinel_addr: addr,
                parquet_root: output,
                worker_id,
                shim_path,
                capabilities: vec!["*".to_string()], // Handle all plugins
                venvs_dir: None, // Use default
            };

            // Start Worker in a task
            let worker_handle = tokio::spawn(async move {
                // Wait for sentinel to be ready (proper synchronization, not sleep)
                if ready_rx.await.is_err() {
                    anyhow::bail!("Sentinel failed to start");
                }
                let (worker, _shutdown_tx) = Worker::connect(worker_config).await?;
                worker.run().await
            });

            // Wait for both (or either to crash)
            tokio::select! {
                res = sentinel_handle => {
                    tracing::error!("Sentinel stopped: {:?}", res);
                }
                res = worker_handle => {
                    tracing::error!("Worker stopped: {:?}", res);
                }
            }
        }
        Commands::Sentinel { args } => {
            let config = SentinelConfig {
                bind_addr: args.bind,
                database_url: args.database,
            };
            let mut sentinel = Sentinel::bind(config).await?;
            sentinel.run().await?;
        }
        Commands::Worker { args } => {
            let shim_path = bridge::find_bridge_shim()?;
            let worker_id = args.worker_id.unwrap_or_else(|| {
                format!(
                    "rust-{}",
                    uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
                )
            });

            let config = WorkerConfig {
                sentinel_addr: args.connect,
                parquet_root: args.output,
                worker_id,
                shim_path,
                capabilities: vec!["*".to_string()], // Handle all plugins
                venvs_dir: None, // Use default
            };
            let (worker, _shutdown_tx) = Worker::connect(config).await?;
            worker.run().await?;
        }
    }

    Ok(())
}

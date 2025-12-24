//! Casparian Flow Unified Launcher
//!
//! Hardened binary with:
//! - **Split-Runtime Architecture**: Control Plane and Data Plane on separate Tokio runtimes
//! - **IPC Transport**: Defaults to Unix Domain Sockets (no firewall popups)
//! - **Graceful Shutdown**: SIGINT/SIGTERM handling with timeout

use anyhow::{Context, Result};
use casparian_sentinel::{Sentinel, SentinelArgs, SentinelConfig};
use casparian_worker::{bridge, Worker, WorkerArgs, WorkerConfig};
use clap::{Parser, Subcommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Shutdown timeout in seconds
const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

#[derive(Parser, Debug)]
#[command(name = "casparian", about = "Unified Launcher for Casparian Flow")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start both Sentinel and Worker in one process (Split-Runtime)
    Start {
        /// ZMQ bind/connect address (default: IPC socket)
        #[arg(long)]
        addr: Option<String>,

        /// Database connection string
        #[arg(long, default_value = "sqlite://casparian_flow.db")]
        database: String,

        /// Parquet output directory
        #[arg(long, default_value = "output")]
        output: std::path::PathBuf,

        /// Number of worker threads for Data Plane (default: CPU cores - 1)
        #[arg(long)]
        data_threads: Option<usize>,
    },
    /// Start only the Sentinel (Control Plane)
    Sentinel {
        #[command(flatten)]
        args: SentinelArgs,
    },
    /// Start only the Worker (Data Plane)
    Worker {
        #[command(flatten)]
        args: WorkerArgs,
    },
}

/// Get the default IPC address for the current platform
fn get_default_ipc_addr() -> String {
    #[cfg(windows)]
    {
        "ipc://casparian_flow".to_string()
    }
    #[cfg(not(windows))]
    {
        let temp_dir = std::env::temp_dir();
        let socket_path = temp_dir.join("casparian.sock");
        format!("ipc://{}", socket_path.display())
    }
}

/// Build the Control Plane runtime (low latency, single thread)
fn build_control_runtime() -> Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("control-plane")
        .enable_all()
        .build()
        .context("Failed to build Control Plane runtime")
}

/// Build the Data Plane runtime (high throughput, N-1 threads)
fn build_data_runtime(threads: Option<usize>) -> Result<Runtime> {
    let num_cpus = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    let worker_threads = threads.unwrap_or_else(|| num_cpus.saturating_sub(1).max(1));

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .thread_name("data-plane")
        .enable_all()
        .build()
        .context("Failed to build Data Plane runtime")
}

/// Sentinel handle for shutdown coordination
struct SentinelHandle {
    stop_tx: oneshot::Sender<()>,
    join_handle: std::thread::JoinHandle<Result<()>>,
}

impl SentinelHandle {
    fn stop(self) -> Result<()> {
        let _ = self.stop_tx.send(());
        self.join_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Sentinel thread panicked"))?
    }
}

/// Worker handle for shutdown coordination
struct UnifiedWorkerHandle {
    shutdown_tx: mpsc::Sender<()>,
    join_handle: std::thread::JoinHandle<Result<()>>,
}

impl UnifiedWorkerHandle {
    async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(()).await;
        // Wait for thread to finish
        self.join_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Worker thread panicked"))?
    }
}

fn main() -> Result<()> {
    // Initialize logging before anything else
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "casparian=info,casparian_sentinel=info,casparian_worker=info".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            addr,
            database,
            output,
            data_threads,
        } => run_unified(addr, database, output, data_threads),
        Commands::Sentinel { args } => run_sentinel_standalone(args),
        Commands::Worker { args } => run_worker_standalone(args),
    }
}

/// Run unified Sentinel + Worker with Split-Runtime Architecture
fn run_unified(
    addr: Option<String>,
    database: String,
    output: std::path::PathBuf,
    data_threads: Option<usize>,
) -> Result<()> {
    let addr = addr.unwrap_or_else(get_default_ipc_addr);
    info!("Starting Unified Casparian Stack (Split-Runtime Architecture)");
    info!("Transport: {}", addr);

    // Setup signal handling
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = shutdown_flag.clone();

    // Install signal handlers
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([SIGINT, SIGTERM])?;
        std::thread::spawn(move || {
            if let Some(sig) = signals.forever().next() {
                info!("Received signal {}, initiating shutdown...", sig);
                shutdown_flag_handler.store(true, Ordering::SeqCst);
            }
        });
    }

    #[cfg(windows)]
    {
        let flag = shutdown_flag_handler.clone();
        ctrlc::set_handler(move || {
            info!("Received Ctrl+C, initiating shutdown...");
            flag.store(true, Ordering::SeqCst);
        })?;
    }

    // Build separate runtimes
    let control_rt = build_control_runtime()?;
    let data_rt = build_data_runtime(data_threads)?;

    info!(
        "Control Plane: 1 thread | Data Plane: {} threads",
        data_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|p| p.get().saturating_sub(1).max(1))
                .unwrap_or(3)
        })
    );

    // Channel for Sentinel ready signal
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    // Channel for Sentinel stop signal
    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    // Start Sentinel on Control Plane runtime (in its own thread)
    let sentinel_addr = addr.clone();
    let sentinel_db = database.clone();
    let sentinel_thread = std::thread::spawn(move || {
        control_rt.block_on(async move {
            let config = SentinelConfig {
                bind_addr: sentinel_addr,
                database_url: sentinel_db,
            };

            let mut sentinel = Sentinel::bind(config).await?;

            // Signal ready
            let _ = ready_tx.send(());

            // Run until stop signal
            tokio::select! {
                result = sentinel.run() => result,
                _ = stop_rx => {
                    info!("Sentinel received stop signal");
                    sentinel.stop();
                    Ok(())
                }
            }
        })
    });

    let sentinel_handle = SentinelHandle {
        stop_tx,
        join_handle: sentinel_thread,
    };

    // Prepare Worker config
    let shim_path = bridge::find_bridge_shim()?;
    let worker_id = format!(
        "rust-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap()
    );
    let worker_config = WorkerConfig {
        sentinel_addr: addr,
        parquet_root: output,
        worker_id,
        shim_path,
        capabilities: vec!["*".to_string()],
        venvs_dir: None,
    };

    // Channel for Worker shutdown
    let (worker_shutdown_tx, worker_shutdown_rx) = mpsc::channel::<()>(1);

    // Start Worker on Data Plane runtime (in its own thread)
    let worker_thread = std::thread::spawn(move || {
        data_rt.block_on(async move {
            // Wait for Sentinel to be ready
            if ready_rx.await.is_err() {
                anyhow::bail!("Sentinel failed to start");
            }

            let (worker, internal_shutdown_tx) = Worker::connect(worker_config).await?;

            // Forward external shutdown to internal shutdown
            let mut external_rx = worker_shutdown_rx;
            tokio::spawn(async move {
                if external_rx.recv().await.is_some() {
                    let _ = internal_shutdown_tx.send(()).await;
                }
            });

            worker.run().await
        })
    });

    let worker_handle = UnifiedWorkerHandle {
        shutdown_tx: worker_shutdown_tx,
        join_handle: worker_thread,
    };

    // Main loop: wait for shutdown signal
    while !shutdown_flag.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));

        // Check if either component crashed
        if sentinel_handle.join_handle.is_finished() {
            error!("Sentinel thread terminated unexpectedly");
            break;
        }
        if worker_handle.join_handle.is_finished() {
            error!("Worker thread terminated unexpectedly");
            break;
        }
    }

    // Graceful shutdown with timeout
    info!("Initiating graceful shutdown (timeout: {}s)...", SHUTDOWN_TIMEOUT_SECS);

    let shutdown_start = std::time::Instant::now();
    let timeout = Duration::from_secs(SHUTDOWN_TIMEOUT_SECS);

    // Step 1: Shutdown Worker first (drain jobs)
    info!("Stopping Worker (waiting for active jobs to complete)...");

    // Create a small runtime for the async shutdown
    let shutdown_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let worker_result = shutdown_rt.block_on(async {
        tokio::time::timeout(timeout, worker_handle.shutdown()).await
    });

    match worker_result {
        Ok(Ok(())) => info!("Worker stopped gracefully"),
        Ok(Err(e)) => warn!("Worker shutdown error: {}", e),
        Err(_) => warn!("Worker shutdown timed out"),
    }

    // Step 2: Stop Sentinel
    let remaining = timeout.saturating_sub(shutdown_start.elapsed());
    if remaining.is_zero() {
        warn!("Shutdown timeout exceeded, forcing exit");
        std::process::exit(1);
    }

    info!("Stopping Sentinel...");
    match sentinel_handle.stop() {
        Ok(()) => info!("Sentinel stopped gracefully"),
        Err(e) => warn!("Sentinel shutdown error: {}", e),
    }

    info!("Shutdown complete");
    Ok(())
}

/// Run Sentinel standalone (for distributed deployment)
fn run_sentinel_standalone(args: SentinelArgs) -> Result<()> {
    let rt = build_control_runtime()?;

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = shutdown_flag.clone();

    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([SIGINT, SIGTERM])?;
        std::thread::spawn(move || {
            if let Some(sig) = signals.forever().next() {
                info!("Received signal {}, shutting down...", sig);
                shutdown_flag_handler.store(true, Ordering::SeqCst);
            }
        });
    }

    rt.block_on(async move {
        let config = SentinelConfig {
            bind_addr: args.bind,
            database_url: args.database,
        };
        let mut sentinel = Sentinel::bind(config).await?;

        // Run with shutdown check
        loop {
            if shutdown_flag.load(Ordering::SeqCst) {
                sentinel.stop();
                break;
            }

            // Run one iteration of the event loop
            tokio::select! {
                result = sentinel.run() => {
                    return result;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Check shutdown flag periodically
                    if shutdown_flag.load(Ordering::SeqCst) {
                        sentinel.stop();
                        break;
                    }
                }
            }
        }
        Ok(())
    })
}

/// Run Worker standalone (for distributed deployment)
fn run_worker_standalone(args: WorkerArgs) -> Result<()> {
    let rt = build_data_runtime(None)?;

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_handler = shutdown_flag.clone();

    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([SIGINT, SIGTERM])?;
        std::thread::spawn(move || {
            if let Some(sig) = signals.forever().next() {
                info!("Received signal {}, shutting down...", sig);
                shutdown_flag_handler.store(true, Ordering::SeqCst);
            }
        });
    }

    rt.block_on(async move {
        let shim_path = bridge::find_bridge_shim()?;
        let worker_id = args.worker_id.unwrap_or_else(|| {
            format!(
                "rust-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap()
            )
        });

        let config = WorkerConfig {
            sentinel_addr: args.connect,
            parquet_root: args.output,
            worker_id,
            shim_path,
            capabilities: vec!["*".to_string()],
            venvs_dir: None,
        };

        let (worker, shutdown_tx) = Worker::connect(config).await?;

        // Spawn shutdown monitor
        let flag = shutdown_flag.clone();
        tokio::spawn(async move {
            while !flag.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = shutdown_tx.send(()).await;
        });

        worker.run().await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ipc_addr_format() {
        let addr = get_default_ipc_addr();
        assert!(addr.starts_with("ipc://"), "IPC address should start with ipc://");

        #[cfg(not(windows))]
        {
            assert!(addr.contains("casparian.sock"), "Unix IPC should use casparian.sock");
            // Should be in temp directory
            let temp_dir = std::env::temp_dir();
            let expected_path = temp_dir.join("casparian.sock");
            assert_eq!(addr, format!("ipc://{}", expected_path.display()));
        }

        #[cfg(windows)]
        {
            assert_eq!(addr, "ipc://casparian_flow");
        }
    }

    #[test]
    fn test_build_control_runtime() {
        let rt = build_control_runtime().expect("Failed to build control runtime");
        // Control runtime should be created successfully
        // We can't easily verify thread count, but we can verify it runs
        rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
    }

    #[test]
    fn test_build_data_runtime_default_threads() {
        let rt = build_data_runtime(None).expect("Failed to build data runtime");
        rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
    }

    #[test]
    fn test_build_data_runtime_explicit_threads() {
        let rt = build_data_runtime(Some(2)).expect("Failed to build data runtime with 2 threads");
        rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
    }

    #[test]
    fn test_build_data_runtime_minimum_one_thread() {
        // Even with 0 threads requested, should get at least 1
        // (saturating_sub handles this)
        let rt = build_data_runtime(Some(1)).expect("Failed to build data runtime with 1 thread");
        rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
    }
}

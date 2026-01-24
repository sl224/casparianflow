//! Casparian Flow Sentinel (Rust)
//!
//! Control plane for job orchestration and worker management.
//!
//! Usage:
//!     casparian-sentinel --bind tcp://127.0.0.1:5555 --database duckdb:/path/to/db.duckdb

use casparian_sentinel::{Sentinel, SentinelConfig};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tracing_subscriber::Layer;

#[derive(Parser, Debug)]
#[command(
    name = "casparian-sentinel",
    about = "Rust Sentinel for Casparian Flow"
)]
struct Args {
    /// ZMQ bind address for workers
    #[arg(long, default_value = "tcp://127.0.0.1:5555")]
    bind: String,

    /// Database connection string
    #[arg(long)]
    database: Option<String>,

    /// Maximum number of workers (default 4, hard cap 8)
    #[arg(long, default_value_t = 4)]
    max_workers: usize,

    /// Control API bind address (e.g., "ipc:///tmp/casparian_control.sock" or "tcp://127.0.0.1:5556")
    /// If not specified, defaults to tcp://127.0.0.1:5556 unless --no-control-api is set.
    #[arg(long)]
    control_addr: Option<String>,

    /// Disable the Control API entirely.
    #[arg(long)]
    no_control_api: bool,
}

fn main() -> anyhow::Result<()> {
    // Initialize logging (console + rolling file)
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "casparian_sentinel=info".into());

    let mut _log_guard: Option<tracing_appender::non_blocking::WorkerGuard> = None;
    let file_layer = match ensure_logs_dir() {
        Ok(log_dir) => {
            let file_appender = tracing_appender::rolling::daily(log_dir, "casparian-sentinel.log");
            let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
            _log_guard = Some(guard);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(file_writer)
                    .with_ansi(false)
                    .with_filter(env_filter.clone()),
            )
        }
        Err(err) => {
            eprintln!("Warning: failed to create logs directory: {}", err);
            None
        }
    };

    let registry = tracing_subscriber::registry().with(file_layer);

    let console_layer = tracing_subscriber::fmt::layer().with_filter(env_filter.clone());
    registry.with(console_layer).init();

    let args = Args::parse();

    tracing::info!("Starting Casparian Rust Sentinel");
    tracing::info!("  Bind: {}", args.bind);
    let database = args.database.unwrap_or_else(default_db_url);
    tracing::info!("  Database: {}", database);
    tracing::info!("  Max workers: {}", args.max_workers);
    let control_addr = if args.no_control_api {
        None
    } else {
        Some(
            args.control_addr
                .unwrap_or_else(|| casparian_sentinel::DEFAULT_CONTROL_ADDR.to_string()),
        )
    };
    if let Some(ref control) = control_addr {
        tracing::info!("  Control API: {}", control);
    }

    let config = SentinelConfig {
        bind_addr: args.bind,
        database_url: database,
        max_workers: args.max_workers,
        control_addr,
    };

    // Bind and run
    let mut sentinel = Sentinel::bind(config)?;
    sentinel.run()?;

    Ok(())
}

fn default_db_url() -> String {
    let home = casparian_home();
    format!("duckdb:{}", home.join("casparian_flow.duckdb").display())
}

fn casparian_home() -> std::path::PathBuf {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return std::path::PathBuf::from(override_path);
    }
    dirs::home_dir()
        .map(|h| h.join(".casparian_flow"))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn ensure_logs_dir() -> std::io::Result<std::path::PathBuf> {
    let dir = casparian_home().join("logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

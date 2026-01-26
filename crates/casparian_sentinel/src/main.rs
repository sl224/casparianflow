//! Casparian Flow Sentinel (Rust)
//!
//! Control plane for job orchestration and worker management.
//!
//! Usage:
//!     casparian-sentinel --bind tcp://127.0.0.1:5555 --state-store sqlite:/path/to/state.sqlite

use casparian_sentinel::{Sentinel, SentinelConfig};
use clap::Parser;
use tracing_subscriber::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "casparian-sentinel",
    about = "Rust Sentinel for Casparian Flow"
)]
struct Args {
    /// ZMQ bind address for workers
    #[arg(
        long,
        default_value_t = casparian_protocol::defaults::DEFAULT_SENTINEL_BIND_ADDR.to_string()
    )]
    bind: String,

    /// State store URL (sqlite:/... | postgres://... | sqlserver://...)
    #[arg(long = "state-store")]
    state_store: Option<String>,

    /// Query catalog path (DuckDB file over Parquet)
    #[arg(long = "query-catalog")]
    query_catalog: Option<std::path::PathBuf>,

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
    let state_store_arg = args
        .state_store
        .unwrap_or_else(|| casparian_protocol::defaults::DEFAULT_STATE_STORE_URL.to_string());
    let state_store_url = resolve_state_store_url(&state_store_arg);
    tracing::info!("  State Store: {}", state_store_url);
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
        state_store_url,
        max_workers: args.max_workers,
        control_addr,
        query_catalog_path: args
            .query_catalog
            .unwrap_or_else(default_query_catalog_path),
    };

    // Bind and run
    let mut sentinel = Sentinel::bind(config)?;
    sentinel.run()?;

    Ok(())
}

fn resolve_state_store_url(state_store: &str) -> String {
    if state_store == casparian_protocol::defaults::DEFAULT_STATE_STORE_URL {
        let home = casparian_home();
        format!("sqlite:{}", home.join("state.sqlite").display())
    } else {
        normalize_state_store_url(state_store)
    }
}

fn normalize_state_store_url(raw: &str) -> String {
    if looks_like_url(raw) {
        raw.to_string()
    } else {
        format!("sqlite:{}", raw)
    }
}

fn looks_like_url(raw: &str) -> bool {
    raw.contains("://")
        || raw.starts_with("sqlite:")
        || raw.starts_with("duckdb:")
        || raw.starts_with("postgres:")
        || raw.starts_with("postgresql:")
        || raw.starts_with("sqlserver:")
}

fn default_query_catalog_path() -> std::path::PathBuf {
    casparian_home().join("query.duckdb")
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

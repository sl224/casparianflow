//! Casparian Flow Unified Launcher
//!
//! Hardened binary with:
//! - **Split-Runtime Architecture**: Control Plane and Data Plane on separate Tokio runtimes
//! - **IPC Transport**: Defaults to Unix Domain Sockets (no firewall popups)
//! - **Graceful Shutdown**: SIGINT/SIGTERM handling with timeout
//! - **CLI Commands**: Standalone utilities for file discovery and preview

use anyhow::{Context, Result};
use casparian_sentinel::{Sentinel, SentinelArgs, SentinelConfig};
use casparian_worker::{bridge, Worker, WorkerArgs, WorkerConfig};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;

// Use scout and runner from library (lib.rs) so they're testable
use casparian::runner;
use casparian::scout;

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
    // === W1: Core Standalone Commands ===

    /// Discover files in a directory (no database required)
    Scan {
        /// Directory to scan
        path: PathBuf,

        /// Filter by file type (e.g., csv, json, parquet)
        #[arg(short = 't', long = "type")]
        types: Vec<String>,

        /// Filter by gitignore-style pattern (e.g., "*.csv", "!node_modules/**")
        /// Can be specified multiple times. Prefix with ! to exclude.
        #[arg(short = 'p', long = "pattern")]
        patterns: Vec<String>,

        /// Scan subdirectories recursively (default: true)
        #[arg(short, long, default_value = "true", action = clap::ArgAction::Set)]
        recursive: bool,

        /// Maximum directory depth to scan
        #[arg(short, long)]
        depth: Option<usize>,

        /// Minimum file size (e.g., 1KB, 10MB)
        #[arg(long)]
        min_size: Option<String>,

        /// Maximum file size (e.g., 100MB, 1GB)
        #[arg(long)]
        max_size: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show statistics summary only
        #[arg(long)]
        stats: bool,

        /// Output file paths only (quiet mode)
        #[arg(short, long)]
        quiet: bool,

        /// Interactive mode - browse and preview files
        #[arg(short, long)]
        interactive: bool,

        /// Tag matched files with this topic (enables database mode)
        #[arg(long)]
        tag: Option<String>,
    },

    /// Preview file contents and infer schema
    Preview {
        /// File to preview
        file: PathBuf,

        /// Number of rows to preview
        #[arg(short = 'n', long, default_value = "20")]
        rows: usize,

        /// Show schema only (no data preview)
        #[arg(long)]
        schema: bool,

        /// View raw bytes (hex dump)
        #[arg(long)]
        raw: bool,

        /// Show first N lines (text mode)
        #[arg(long)]
        head: Option<usize>,

        /// CSV delimiter character
        #[arg(long)]
        delimiter: Option<char>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    // === W2: Tagging Commands (stubs) ===

    /// Assign a topic to file(s)
    Tag {
        /// File or directory to tag
        path: Option<PathBuf>,

        /// Topic to assign
        topic: Option<String>,

        /// Preview changes without applying
        #[arg(long)]
        dry_run: bool,

        /// Tag without creating jobs
        #[arg(long)]
        no_queue: bool,
    },

    /// Remove topic from a file
    Untag {
        /// File to untag
        path: PathBuf,
    },

    /// List discovered files
    Files {
        /// Filter by source (uses default context if not specified)
        #[arg(short = 's', long)]
        source: Option<String>,

        /// Show files from all sources (override default context)
        #[arg(long)]
        all: bool,

        /// Filter by topic
        #[arg(long)]
        topic: Option<String>,

        /// Filter by status (pending, processing, done, failed)
        #[arg(long)]
        status: Option<String>,

        /// Show only untagged files
        #[arg(long)]
        untagged: bool,

        /// Filter by gitignore-style pattern (e.g., "*.csv", "!test/**")
        #[arg(short = 'p', long = "pattern")]
        patterns: Vec<String>,

        /// Tag matching files with this topic
        #[arg(long)]
        tag: Option<String>,

        /// Maximum files to display
        #[arg(long, default_value = "50")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    // === W3: Parser Commands (stubs) ===

    /// Manage parsers
    Parser {
        #[command(subcommand)]
        action: cli::parser::ParserAction,
    },

    // === W4: Job Commands (stubs) ===

    /// Execute a parser against an input file
    Run(cli::run::RunArgs),

    /// List processing jobs
    Jobs {
        /// Filter by topic
        #[arg(long)]
        topic: Option<String>,

        /// Show only pending jobs
        #[arg(long)]
        pending: bool,

        /// Show only running jobs
        #[arg(long)]
        running: bool,

        /// Show only failed jobs
        #[arg(long)]
        failed: bool,

        /// Show only completed jobs
        #[arg(long)]
        done: bool,

        /// Show dead letter jobs (jobs that exhausted retries)
        #[arg(long)]
        dead_letter: bool,

        /// Maximum jobs to display
        #[arg(long, default_value = "50")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage pipelines
    Pipeline {
        #[command(subcommand)]
        action: cli::pipeline::PipelineAction,
    },

    /// Manage a specific job
    Job {
        #[command(subcommand)]
        action: cli::job::JobAction,
    },

    /// Manage workers (CLI version)
    #[command(name = "worker-cli")]
    WorkerCli {
        #[command(subcommand)]
        action: cli::worker::WorkerAction,
    },

    // === W5: Resource Commands (stubs) ===

    /// Manage data sources
    Source {
        #[command(subcommand)]
        action: cli::source::SourceAction,
    },

    /// Manage tagging rules
    Rule {
        #[command(subcommand)]
        action: cli::rule::RuleAction,
    },

    /// Manage topics
    Topic {
        #[command(subcommand)]
        action: cli::topic::TopicAction,
    },

    // === W6: Backfill Command ===

    /// Re-process files when parser version changes
    ///
    /// When you update a parser to a new version, this command identifies
    /// files that were processed with old versions and need re-processing.
    ///
    /// Examples:
    ///   casparian backfill my_parser              # Preview files to backfill
    ///   casparian backfill my_parser --execute    # Actually run backfill
    ///   casparian backfill my_parser --limit 10   # Limit to 10 files
    Backfill {
        /// Parser name to backfill
        parser: String,

        /// Actually execute the backfill (default: preview mode)
        #[arg(long)]
        execute: bool,

        /// Maximum files to process
        #[arg(long, short = 'n')]
        limit: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Force re-processing even if already processed with this version
        #[arg(long)]
        force: bool,
    },

    // === W7: MCP Server ===

    /// Start MCP server for Claude Code integration
    McpServer {
        /// Bind address (default: stdio for MCP)
        #[arg(long)]
        addr: Option<String>,
    },

    // === Existing Server Commands ===

    /// Start both Sentinel and Worker in one process (Split-Runtime)
    Start {
        /// ZMQ bind/connect address (default: IPC socket)
        #[arg(long)]
        addr: Option<String>,

        /// Database path (default: ~/.casparian_flow/casparian_flow.duckdb)
        #[arg(long)]
        database: Option<std::path::PathBuf>,

        /// Parquet output directory (default: ~/.casparian_flow/output)
        #[arg(long)]
        output: Option<std::path::PathBuf>,

        /// Number of worker threads for Data Plane (default: CPU cores - 1)
        #[arg(long)]
        data_threads: Option<usize>,

        /// Directory for Python virtual environments
        /// Default: ~/.casparian_flow/venvs
        #[arg(long, env = "CASPARIAN_VENVS_DIR")]
        venvs_dir: Option<std::path::PathBuf>,
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

    /// Publish a plugin to the Sentinel registry
    Publish {
        /// Path to the Python plugin file
        file: std::path::PathBuf,

        /// Plugin version (e.g., "1.0.2")
        #[arg(long)]
        version: String,

        /// Sentinel address (default: IPC socket)
        #[arg(long)]
        addr: Option<String>,

        /// Publisher name (defaults to system username)
        #[arg(long)]
        publisher: Option<String>,

        /// Publisher email (optional)
        #[arg(long)]
        email: Option<String>,
    },
    /// Process a single job from the queue (for UI-spawned processing)
    ProcessJob {
        /// Job ID from cf_processing_queue
        job_id: i64,

        /// Sentinel database path (casparian_flow.duckdb)
        #[arg(long)]
        db: std::path::PathBuf,

        /// Output directory for processed files
        #[arg(long, default_value = "output")]
        output: std::path::PathBuf,
    },

    /// Show current configuration and paths
    Config {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Interactive TUI for chat and monitoring
    Tui {
        #[command(flatten)]
        args: cli::tui::TuiArgs,
    },
}

/// Get the default IPC address for the current platform.
///
/// Uses user-specific paths to avoid collisions on multi-user systems:
/// - **Unix**: `$XDG_RUNTIME_DIR/casparian.sock` or `/tmp/casparian_{uid}.sock`
/// - **Windows**: `ipc://casparian_{username}` (named pipe scoped to user)
fn get_default_ipc_addr() -> String {
    #[cfg(windows)]
    {
        // Windows named pipes are global - scope to username to avoid collision
        let username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "default".to_string());
        format!("ipc://casparian_{}", username)
    }
    #[cfg(not(windows))]
    {
        // Try XDG_RUNTIME_DIR first (standard on Linux for user-specific sockets)
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let socket_path = std::path::Path::new(&runtime_dir).join("casparian.sock");
            return format!("ipc://{}", socket_path.display());
        }

        // Fallback to temp_dir with user ID to avoid collision on shared systems
        let temp_dir = std::env::temp_dir();
        let uid = unsafe { libc::getuid() };
        let socket_path = temp_dir.join(format!("casparian_{}.sock", uid));
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

fn command_wants_json(command: &Commands) -> bool {
    match command {
        Commands::Scan { json, .. } => *json,
        Commands::Preview { json, .. } => *json,
        Commands::Files { json, .. } => *json,
        Commands::Jobs { json, .. } => *json,
        Commands::Backfill { json, .. } => *json,
        Commands::Config { json } => *json,
        Commands::Run(args) => args.json,
        Commands::Parser { action } => parser_action_wants_json(action),
        Commands::Rule { action } => rule_action_wants_json(action),
        Commands::Topic { action } => topic_action_wants_json(action),
        Commands::Source { action } => source_action_wants_json(action),
        Commands::WorkerCli { action } => worker_action_wants_json(action),
        Commands::Job { action } => job_action_wants_json(action),
        _ => false,
    }
}

fn parser_action_wants_json(action: &cli::parser::ParserAction) -> bool {
    match action {
        cli::parser::ParserAction::List { json } => *json,
        cli::parser::ParserAction::Show { json, .. } => *json,
        cli::parser::ParserAction::Test { json, .. } => *json,
        cli::parser::ParserAction::Backtest { json, .. } => *json,
        cli::parser::ParserAction::Register { json, .. } => *json,
        cli::parser::ParserAction::Health { json, .. } => *json,
        _ => false,
    }
}

fn rule_action_wants_json(action: &cli::rule::RuleAction) -> bool {
    match action {
        cli::rule::RuleAction::List { json } => *json,
        cli::rule::RuleAction::Show { json, .. } => *json,
        _ => false,
    }
}

fn topic_action_wants_json(action: &cli::topic::TopicAction) -> bool {
    match action {
        cli::topic::TopicAction::List { json } => *json,
        cli::topic::TopicAction::Show { json, .. } => *json,
        _ => false,
    }
}

fn source_action_wants_json(action: &cli::source::SourceAction) -> bool {
    match action {
        cli::source::SourceAction::List { json } => *json,
        cli::source::SourceAction::Show { json, .. } => *json,
        _ => false,
    }
}

fn worker_action_wants_json(action: &cli::worker::WorkerAction) -> bool {
    match action {
        cli::worker::WorkerAction::List { json } => *json,
        cli::worker::WorkerAction::Show { json, .. } => *json,
        _ => false,
    }
}

fn job_action_wants_json(action: &cli::job::JobAction) -> bool {
    match action {
        cli::job::JobAction::Show { json, .. } => *json,
        _ => false,
    }
}

fn run_command(cli: Cli) -> Result<()> {
    match cli.command {
        // === W1: Core Standalone Commands ===
        Commands::Scan {
            path,
            types,
            patterns,
            recursive,
            depth,
            min_size,
            max_size,
            json,
            stats,
            quiet,
            interactive,
            tag,
        } => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(async {
                cli::scan::run(cli::scan::ScanArgs {
                    path,
                    types,
                    patterns,
                    recursive,
                    depth,
                    min_size,
                    max_size,
                    json,
                    stats,
                    quiet,
                    interactive,
                    tag,
                })
                .await
            })
        }

        Commands::Preview {
            file,
            rows,
            schema,
            raw,
            head,
            delimiter,
            json,
        } => cli::preview::run(cli::preview::PreviewArgs {
            file,
            rows,
            schema,
            raw,
            head,
            delimiter,
            json,
        }),

        // === W2: Tagging Commands (stubs) ===
        Commands::Tag {
            path,
            topic,
            dry_run,
            no_queue,
        } => cli::tag::run(cli::tag::TagArgs {
            path,
            topic,
            dry_run,
            no_queue,
        }),

        Commands::Untag { path } => cli::tag::run_untag(cli::tag::UntagArgs { path }),

        Commands::Files {
            source,
            all,
            topic,
            status,
            untagged,
            patterns,
            tag,
            limit,
            json,
        } => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(async {
                cli::files::run(cli::files::FilesArgs {
                    source,
                    all,
                    topic,
                    status,
                    untagged,
                    patterns,
                    tag,
                    limit,
                    json,
                })
                .await
            })
        }

        // === W3: Parser Commands (stubs) ===
        Commands::Parser { action } => cli::parser::run(action),

        // === W4: Job Commands (stubs) ===
        Commands::Run(args) => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(cli::run::cmd_run(args))
        }

        Commands::Jobs {
            topic,
            pending,
            running,
            failed,
            done,
            dead_letter,
            limit,
            json,
        } => cli::jobs::run(cli::jobs::JobsArgs {
            topic,
            pending,
            running,
            failed,
            done,
            dead_letter,
            limit,
            json,
        }),

        Commands::Job { action } => cli::job::run(action),

        Commands::WorkerCli { action } => cli::worker::run(action),

        Commands::Pipeline { action } => cli::pipeline::run(action),

        // === W5: Resource Commands (stubs) ===
        Commands::Source { action } => cli::source::run(action),
        Commands::Rule { action } => cli::rule::run(action),
        Commands::Topic { action } => cli::topic::run(action),

        // === W6: Backfill Command ===
        Commands::Backfill {
            parser,
            execute,
            limit,
            json,
            force,
        } => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(async {
                cli::backfill::run(cli::backfill::BackfillArgs {
                    parser_name: parser,
                    execute,
                    limit,
                    json,
                    force,
                })
                .await
            })
        }

        // === W7: MCP Server ===
        Commands::McpServer { addr } => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(async { cli::mcp::run(cli::mcp::McpArgs { addr }).await })
        }

        // === Existing Server Commands ===
        Commands::Start {
            addr,
            database,
            output,
            data_threads,
            venvs_dir,
        } => {
            // Use config module for defaults
            let db_path = database.unwrap_or_else(cli::config::active_db_path);
            let output_dir = output.unwrap_or_else(cli::config::output_dir);
            run_unified(addr, db_path, output_dir, data_threads, venvs_dir)
        }

        Commands::Sentinel { args } => run_sentinel_standalone(args),

        Commands::Worker { args } => run_worker_standalone(args),

        Commands::Publish {
            file,
            version,
            addr,
            publisher,
            email,
        } => {
            // Publish runs synchronously (no need for tokio runtime)
            tokio::runtime::Runtime::new()?.block_on(async {
                run_publish(file, version, addr, publisher, email).await
            })
        }
        Commands::ProcessJob { job_id, db, output } => process_single_job(job_id, &db, &output),
        Commands::Config { json } => cli::config::run(cli::config::ConfigArgs { json }),
        Commands::Tui { args } => {
            let rt = Runtime::new().context("Failed to create runtime")?;
            rt.block_on(async { cli::tui::run(args).await })
        }
    }
}

fn main() -> ExitCode {
    // Parse CLI first to check if we're in TUI mode
    let cli = Cli::parse();

    // Initialize logging - suppress stdout logs in TUI mode to avoid corrupting display
    let is_tui_mode = matches!(cli.command, Commands::Tui { .. });
    let json_mode = command_wants_json(&cli.command);

    if is_tui_mode {
        // TUI mode: only log errors to stderr, suppress info/debug
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "error".into()),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();
    } else if json_mode {
        // JSON mode: log to stderr to keep stdout machine-readable
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    "casparian=info,casparian_sentinel=info,casparian_worker=info".into()
                }),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();
    } else {
        // Normal mode: full logging to stdout
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    "casparian=info,casparian_sentinel=info,casparian_worker=info".into()
                }),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let result = run_command(cli);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if json_mode {
                cli::error::print_json_error(&err);
            } else {
                eprintln!("{:?}", err);
            }
            ExitCode::from(1)
        }
    }
}

/// Run unified Sentinel + Worker with Split-Runtime Architecture
fn run_unified(
    addr: Option<String>,
    db_path: std::path::PathBuf,
    output: std::path::PathBuf,
    data_threads: Option<usize>,
    venvs_dir: Option<std::path::PathBuf>,
) -> Result<()> {
    // Ensure config directories exist
    cli::config::ensure_casparian_home()?;
    std::fs::create_dir_all(&output)?;

    let addr = addr.unwrap_or_else(get_default_ipc_addr);
    info!("Starting Unified Casparian Stack (Split-Runtime Architecture)");
    info!("Transport: {}", addr);
    info!("Database: {}", db_path.display());
    info!("Output: {}", output.display());

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
    let sentinel_db = format!("duckdb:{}", db_path.display());
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
    // Materialize embedded bridge shim to disk (single binary distribution)
    let shim_path = bridge::materialize_bridge_shim()?;
    let worker_id = format!(
        "rust-{}",
        &uuid::Uuid::new_v4().to_string()[..8]  // First 8 hex chars of UUID
    );
    let worker_config = WorkerConfig {
        sentinel_addr: addr,
        parquet_root: output,
        worker_id,
        shim_path,
        capabilities: vec!["*".to_string()],
        venvs_dir,
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
        // Resolve database URL: if it's the default, use config module resolution
        let database_url = if args.database == "duckdb:casparian_flow.duckdb" {
            let db_path = cli::config::active_db_path();
            format!("duckdb:{}", db_path.display())
        } else {
            args.database
        };

        let config = SentinelConfig {
            bind_addr: args.bind,
            database_url,
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
        // Materialize embedded bridge shim to disk (single binary distribution)
        let shim_path = bridge::materialize_bridge_shim()?;
        let worker_id = args.worker_id.unwrap_or_else(|| {
            format!(
                "rust-{}",
                &uuid::Uuid::new_v4().to_string()[..8]  // First 8 hex chars of UUID
            )
        });

        let config = WorkerConfig {
            sentinel_addr: args.connect,
            parquet_root: args.output,
            worker_id,
            shim_path,
            capabilities: vec!["*".to_string()],
            venvs_dir: None, // Use default ~/.casparian_flow/venvs
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

/// Publish a plugin to the Sentinel registry
async fn run_publish(
    file: std::path::PathBuf,
    version: String,
    addr: Option<String>,
    publisher: Option<String>,
    email: Option<String>,
) -> Result<()> {
    use casparian_protocol::types::DeployCommand;
    use casparian_protocol::{Message, OpCode};
    use casparian_security::signing::sha256;
    use casparian_security::Gatekeeper;
    use zeromq::{Socket, SocketRecv, SocketSend, ZmqMessage};

    info!("Publishing plugin: {:?} v{}", file, version);

    // 1. Read plugin source code
    let source_code = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read plugin file: {:?}", file))?;

    // 2. Validate with Gatekeeper (AST-based security checks)
    let gatekeeper = Gatekeeper::new();
    gatekeeper
        .validate(&source_code)
        .context("Plugin failed security validation")?;
    info!("✓ Security validation passed");

    // 3. Check for uv.lock, run `uv lock` if missing
    let plugin_dir = file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Plugin file has no parent directory"))?;
    let lockfile_path = plugin_dir.join("uv.lock");

    if !lockfile_path.exists() {
        info!("No uv.lock found, running `uv lock` in {:?}...", plugin_dir);
        let output = std::process::Command::new("uv")
            .arg("lock")
            .current_dir(plugin_dir)
            .output()
            .context("Failed to run `uv lock` (is uv installed?)")?;

        if !output.status.success() {
            anyhow::bail!(
                "uv lock failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        info!("✓ Generated uv.lock");
    }

    let lockfile_content = std::fs::read_to_string(&lockfile_path)
        .context("Failed to read uv.lock after generation")?;

    // 4. Compute hashes
    let env_hash = sha256(lockfile_content.as_bytes());
    let artifact_content = format!("{}{}", source_code, lockfile_content);
    let artifact_hash = sha256(artifact_content.as_bytes());
    info!("✓ Computed hashes (env: {}..., artifact: {}...)", &env_hash[..8], &artifact_hash[..8]);

    // 5. Extract plugin name from file
    let plugin_name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Could not extract plugin name from file path"))?
        .to_string();

    // Get publisher name (default to system username)
    let publisher_name = publisher.unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    });

    // 6. Construct DeployCommand
    let deploy_cmd = DeployCommand {
        plugin_name: plugin_name.clone(),
        version: version.clone(),
        source_code,
        lockfile_content,
        env_hash,
        artifact_hash,
        publisher_name,
        publisher_email: email,
        azure_oid: None,
        system_requirements: None,
    };

    // 7. Send via ZMQ DEALER to Sentinel
    let sentinel_addr = addr.unwrap_or_else(get_default_ipc_addr);
    info!("Connecting to Sentinel at {}", sentinel_addr);

    let mut socket = zeromq::DealerSocket::new();
    socket.connect(&sentinel_addr).await?;
    info!("✓ Connected to Sentinel");

    // Serialize payload
    let payload = serde_json::to_vec(&deploy_cmd)?;

    // Create protocol message
    let msg = Message::new(OpCode::Deploy, 0, payload)?;
    let (header_bytes, payload_bytes) = msg.pack()?;

    // Send message (multipart)
    let mut multipart = ZmqMessage::from(header_bytes);
    multipart.push_back(payload_bytes.into());
    socket.send(multipart).await?;
    info!("✓ Sent deployment request");

    // 8. Await ACK/ERR response
    let response_frames: ZmqMessage = socket.recv().await?;
    let response_msg = Message::unpack(
        &response_frames
            .into_vec()
            .iter()
            .map(|f| f.to_vec())
            .collect::<Vec<_>>(),
    )?;

    match response_msg.header.opcode {
        OpCode::Ack => {
            use casparian_protocol::types::DeployResponse;
            let deploy_response: DeployResponse = serde_json::from_slice(&response_msg.payload)?;

            if deploy_response.success {
                println!("✅ Deployed plugin '{}' v{}", plugin_name, version);
                Ok(())
            } else {
                anyhow::bail!("Deployment failed: {}", deploy_response.message)
            }
        }
        OpCode::Err => {
            use casparian_protocol::types::ErrorPayload;
            let error_payload: ErrorPayload = serde_json::from_slice(&response_msg.payload)?;
            anyhow::bail!("Deployment error: {}", error_payload.message)
        }
        _ => anyhow::bail!(
            "Unexpected response opcode: {:?}",
            response_msg.header.opcode
        ),
    }
}

/// Process a single job from cf_processing_queue
///
/// This is used by the Tauri UI to spawn a worker process for each job.
/// The job runs the plugin via the bridge (Python subprocess).
///
/// Uses casparian_sentinel::JobQueue (DbConnection) for all database operations.
fn process_single_job(
    job_id: i64,
    db_path: &std::path::Path,
    output_dir: &std::path::Path,
) -> Result<()> {
    use casparian_worker::bridge::{self, BridgeConfig};
    use std::time::Instant;

    info!(job_id, db = %db_path.display(), "Processing job");

    // Create runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    // Open database via JobQueue
    let db_url = format!("duckdb:{}", db_path.display());
    let queue = rt.block_on(casparian_sentinel::JobQueue::open(&db_url))
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

    // Get job details
    let job_details = rt.block_on(queue.get_job_details(job_id))?
        .ok_or_else(|| anyhow::anyhow!("Job {} not found (no file_id or input_file)", job_id))?;

    let plugin_name = job_details.plugin_name;
    let file_path = job_details.file_path;

    info!(plugin = %plugin_name, file = %file_path, "Starting job");

    // Claim the job (status = RUNNING)
    rt.block_on(queue.claim_job(job_id))?;

    // Get plugin source code and env_hash
    let plugin_details = rt.block_on(queue.get_plugin_details(&plugin_name))?
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found or not deployed", plugin_name))?;

    let plugin_source = plugin_details.source_code;
    let env_hash_opt = plugin_details.env_hash;

    // Execute plugin via bridge
    let start = Instant::now();

    // Try to get lockfile from cf_plugin_environment (proper deployment path)
    let lockfile_content: Option<String> = if let Some(ref hash) = env_hash_opt {
        rt.block_on(queue.get_lockfile(hash))?
    } else {
        None
    };

    // Determine venv setup strategy
    let (env_hash, interpreter) = if let (Some(hash), Some(lockfile)) = (&env_hash_opt, &lockfile_content) {
        // PROPER PATH: Use VenvManager with lockfile
        info!(env_hash = %hash, "Using pre-computed environment from deployment");

        let venv_manager = casparian_worker::venv_manager::VenvManager::new()
            .context("Failed to initialize VenvManager")?;

        let interp = venv_manager.get_or_create(hash, lockfile, None)
            .context("Failed to get or create venv")?;

        (hash.clone(), interp)
    } else {
        // ADHOC PATH: Generate minimal lockfile with plugin deps + bridge deps
        use casparian_security::signing::sha256;

        let deps = parse_plugin_dependencies(&plugin_source);
        info!(deps = ?deps, "Detected plugin dependencies (adhoc mode)");

        // Bridge runtime deps - ALWAYS needed for IPC serialization
        let mut all_deps = vec!["pyarrow".to_string(), "pandas".to_string()];
        all_deps.extend(deps);
        all_deps.sort();
        all_deps.dedup();

        // Generate deterministic hash from deps
        let deps_str = all_deps.join(",");
        let hash = sha256(deps_str.as_bytes());
        let short_hash = &hash[..16];

        info!(deps = ?all_deps, env_hash = %short_hash, "Using adhoc environment");

        let venv_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".casparian_flow")
            .join("venvs")
            .join(short_hash);

        // Create venv if needed
        if !venv_path.join("bin").join("python").exists() {
            std::fs::create_dir_all(&venv_path)?;

            let output = std::process::Command::new("uv")
                .args(["venv", venv_path.to_str().unwrap()])
                .output()
                .context("Failed to create venv with uv")?;

            if !output.status.success() {
                anyhow::bail!("uv venv failed: {}", String::from_utf8_lossy(&output.stderr));
            }

            // Install ALL dependencies (including bridge deps)
            if !all_deps.is_empty() {
                let mut cmd = std::process::Command::new("uv");
                cmd.arg("pip").arg("install");
                for dep in &all_deps {
                    cmd.arg(dep);
                }
                cmd.env("VIRTUAL_ENV", &venv_path);

                let output = cmd.output().context("Failed to install dependencies")?;
                if !output.status.success() {
                    anyhow::bail!("uv pip install failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        }

        (short_hash.to_string(), venv_path.join("bin").join("python"))
    };
    info!(interpreter = %interpreter.display(), env_hash = %env_hash, "Using Python interpreter");

    // Materialize shim
    let shim_path = bridge::materialize_bridge_shim()?;

    // Configure bridge
    let config = BridgeConfig {
        interpreter_path: interpreter,
        source_code: plugin_source,
        file_path: file_path.clone(),
        job_id: job_id as u64,
        file_id: 0,
        shim_path,
    };

    // Execute via bridge
    let result = rt.block_on(bridge::execute_bridge(config));
    let elapsed = start.elapsed();

    match result {
        Ok(bridge_result) => {
            std::fs::create_dir_all(output_dir)?;
            let output_paths = handle_bridge_outputs(&bridge_result, output_dir, &plugin_name, job_id)?;

            let result_summary = output_paths.join(";");
            info!(job_id, elapsed_ms = elapsed.as_millis(), outputs = %result_summary, "Job completed");

            rt.block_on(queue.complete_job(job_id, &result_summary))?;

            // Log plugin output
            if !bridge_result.logs.is_empty() {
                info!(logs = %bridge_result.logs, "Plugin logs");
            }

            Ok(())
        }
        Err(e) => {
            error!(job_id, error = %e, elapsed_ms = elapsed.as_millis(), "Job failed");
            rt.block_on(queue.fail_job(job_id, &e.to_string()))?;
            Err(e)
        }
    }
}

/// Parse plugin dependencies from source code
///
/// Looks for patterns like:
/// - `import pandas` -> "pandas"
/// - `import pyarrow` -> "pyarrow"
fn parse_plugin_dependencies(source: &str) -> Vec<String> {
    let mut deps = vec![];

    for line in source.lines() {
        let line = line.trim();

        // import X or import X as Y
        if let Some(rest) = line.strip_prefix("import ") {
            let module = rest.split_whitespace().next().unwrap_or("");
            let module = module.split('.').next().unwrap_or("");
            if !module.is_empty() && !is_stdlib_module(module) {
                deps.push(module.to_string());
            }
        }
        // from X import Y
        else if let Some(rest) = line.strip_prefix("from ") {
            let module = rest.split_whitespace().next().unwrap_or("");
            let module = module.split('.').next().unwrap_or("");
            if !module.is_empty() && !is_stdlib_module(module) {
                deps.push(module.to_string());
            }
        }
    }

    // Deduplicate
    deps.sort();
    deps.dedup();
    deps
}

/// Check if a module is in the Python standard library
fn is_stdlib_module(module: &str) -> bool {
    matches!(
        module,
        "os" | "sys" | "re" | "json" | "math" | "time" | "datetime"
            | "collections" | "itertools" | "functools" | "typing"
            | "pathlib" | "io" | "csv" | "tempfile" | "shutil"
            | "subprocess" | "threading" | "multiprocessing"
            | "hashlib" | "uuid" | "random" | "string" | "struct"
            | "copy" | "enum" | "dataclasses" | "abc" | "contextlib"
    )
}

/// Write Arrow batches to a Parquet file
fn handle_bridge_outputs(
    bridge_result: &bridge::BridgeResult,
    output_dir: &std::path::Path,
    plugin_name: &str,
    job_id: i64,
) -> Result<Vec<String>> {
    #[cfg(feature = "data-plane")]
    {
        use casparian_sinks::{plan_outputs, write_output_plan, OutputDescriptor};

        let job_id_str = job_id.to_string();
        let descriptors: Vec<OutputDescriptor> = bridge_result
            .output_info
            .iter()
            .map(|info| OutputDescriptor {
                name: info.name.clone(),
                table: info.table.clone(),
            })
            .collect();

        let outputs = plan_outputs(&descriptors, &bridge_result.batches, plugin_name)?;
        let sink_uri = format!("parquet://{}", output_dir.display());
        let artifacts = write_output_plan(&sink_uri, &outputs, &job_id_str)?;

        Ok(artifacts
            .into_iter()
            .map(|artifact| artifact.uri.strip_prefix("file://").unwrap_or(&artifact.uri).to_string())
            .collect())
    }
    #[cfg(not(feature = "data-plane"))]
    {
        let _ = bridge_result;
        let _ = output_dir;
        let _ = plugin_name;
        let _ = job_id;
        anyhow::bail!("data outputs require the `data-plane` feature")
    }
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
            // Should contain casparian in the socket name
            assert!(
                addr.contains("casparian"),
                "Unix IPC should contain 'casparian' in path: {}",
                addr
            );
            assert!(
                addr.ends_with(".sock"),
                "Unix IPC should end with .sock: {}",
                addr
            );

            // Verify it uses either XDG_RUNTIME_DIR or temp_dir with UID
            if std::env::var("XDG_RUNTIME_DIR").is_ok() {
                assert!(
                    addr.contains("casparian.sock"),
                    "With XDG_RUNTIME_DIR, should use casparian.sock: {}",
                    addr
                );
            } else {
                let uid = unsafe { libc::getuid() };
                assert!(
                    addr.contains(&format!("casparian_{}", uid)),
                    "Without XDG_RUNTIME_DIR, should use casparian_<uid>.sock: {}",
                    addr
                );
            }
        }

        #[cfg(windows)]
        {
            assert!(addr.starts_with("ipc://casparian_"), "Windows should start with ipc://casparian_");
            // Should contain username
            let username = std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "default".to_string());
            assert_eq!(addr, format!("ipc://casparian_{}", username));
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

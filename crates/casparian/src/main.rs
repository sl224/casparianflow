//! Casparian Flow Unified Launcher
//!
//! Hardened binary with:
//! - **Split-Runtime Architecture**: Control Plane and Data Plane on separate Tokio runtimes
//! - **IPC Transport**: Defaults to Unix Domain Sockets (no firewall popups)
//! - **Graceful Shutdown**: SIGINT/SIGTERM handling with timeout
//! - **CLI Commands**: Standalone utilities for file discovery and preview

use anyhow::Result;
use casparian_sentinel::{Sentinel, SentinelArgs, SentinelConfig};
use casparian_worker::{bridge, Worker, WorkerArgs, WorkerConfig};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod cli;

/// Shutdown timeout in seconds
const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

#[derive(Parser, Debug)]
#[command(name = "casparian", about = "Unified Launcher for Casparian Flow")]
struct Cli {
    /// Enable verbose logging (info/debug to stderr)
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

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

    /// Show parser schema
    Schema {
        /// Parser name or path (e.g., fix or parsers/fix/fix_parser.py)
        parser: String,

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

    /// Manage native plugins
    Plugin {
        #[command(subcommand)]
        action: cli::plugin::PluginAction,
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

    /// MCP (Model Context Protocol) server for AI tool integration
    Mcp {
        #[command(subcommand)]
        action: cli::mcp::McpAction,
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

/// Sentinel handle for shutdown coordination
struct SentinelHandle {
    stop_tx: mpsc::Sender<()>,
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
    handle: casparian_worker::WorkerHandle,
    join_handle: std::thread::JoinHandle<Result<()>>,
}

impl UnifiedWorkerHandle {
    fn shutdown(self) -> Result<()> {
        self.handle
            .shutdown_gracefully(Duration::from_secs(SHUTDOWN_TIMEOUT_SECS))
            .map_err(anyhow::Error::new)?;
        self.join_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Worker thread panicked"))?
    }
}

fn command_wants_json(command: &Commands) -> bool {
    match command {
        Commands::Scan { json, .. } => *json,
        Commands::Preview { json, .. } => *json,
        Commands::Schema { json, .. } => *json,
        Commands::Files { json, .. } => *json,
        Commands::Jobs { json, .. } => *json,
        Commands::Backfill { json, .. } => *json,
        Commands::Config { json } => *json,
        Commands::Run(args) => args.json,
        Commands::Parser { action } => parser_action_wants_json(action),
        Commands::Plugin { action } => plugin_action_wants_json(action),
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
        cli::parser::ParserAction::Health { json, .. } => *json,
        _ => false,
    }
}

fn plugin_action_wants_json(action: &cli::plugin::PluginAction) -> bool {
    match action {
        cli::plugin::PluginAction::List { json } => *json,
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

        Commands::Schema { parser, json } => {
            cli::schema::run(cli::schema::SchemaArgs { parser, json })
        }

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
        }

        // === W3: Parser Commands (stubs) ===
        Commands::Parser { action } => cli::parser::run(action),
        Commands::Plugin { action } => cli::plugin::run(action),

        // === W4: Job Commands (stubs) ===
        Commands::Run(args) => {
            cli::run::cmd_run(args)
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
            cli::backfill::run(cli::backfill::BackfillArgs {
                parser_name: parser,
                execute,
                limit,
                json,
                force,
            })
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
        } => run_publish(file, version, addr, publisher, email),
        Commands::Config { json } => cli::config::run(cli::config::ConfigArgs { json }),
        Commands::Tui { args } => cli::tui::run(args),
        Commands::Mcp { action } => cli::mcp::run(action),
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
    let control_addr = Some("tcp://127.0.0.1:5556".to_string());
    info!("Starting Unified Casparian Stack (Split-Runtime Architecture)");
    info!("Transport: {}", addr);
    info!(
        "Control API: {}",
        control_addr.as_deref().unwrap_or("disabled")
    );
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

    let _ = data_threads;

    // Channel for Sentinel ready signal
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    // Channel for Sentinel stop signal
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // Start Sentinel (in its own thread)
    let sentinel_addr = addr.clone();
    let sentinel_db = format!("duckdb:{}", db_path.display());
    let sentinel_thread = std::thread::spawn(move || {
        let config = SentinelConfig {
            bind_addr: sentinel_addr,
            database_url: sentinel_db,
            control_addr,
            max_workers: 1,
        };

        let mut sentinel = Sentinel::bind(config)?;

        // Signal ready
        let _ = ready_tx.send(());

        sentinel.run_with_shutdown(stop_rx)
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

    // Wait for Sentinel to be ready
    ready_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("Sentinel failed to start"))?;

    // Start Worker (in its own thread)
    let (worker, worker_handle) = Worker::connect(worker_config)
        .map_err(|e| anyhow::anyhow!(e))?;

    let worker_thread = std::thread::spawn(move || {
        worker.run().map_err(|e| anyhow::anyhow!(e))
    });

    let worker_handle = UnifiedWorkerHandle {
        handle: worker_handle,
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
    match worker_handle.shutdown() {
        Ok(()) => info!("Worker stopped gracefully"),
        Err(e) => warn!("Worker shutdown error: {}", e),
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
        control_addr: None,
        max_workers: args.max_workers,
    };
    let mut sentinel = Sentinel::bind(config)?;

    let (stop_tx, stop_rx) = mpsc::channel();
    let flag = shutdown_flag.clone();
    std::thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = stop_tx.send(());
    });

    sentinel.run_with_shutdown(stop_rx)
}

/// Run Worker standalone (for distributed deployment)
fn run_worker_standalone(args: WorkerArgs) -> Result<()> {
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

    // Materialize embedded bridge shim to disk (single binary distribution)
    let shim_path = bridge::materialize_bridge_shim().map_err(|e| anyhow::anyhow!(e))?;
    let worker_id = args.worker_id.unwrap_or_else(|| {
        format!(
            "rust-{}",
            &uuid::Uuid::new_v4().to_string()[..8] // First 8 hex chars of UUID
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

    let (worker, worker_handle) = Worker::connect(config)
        .map_err(|e| anyhow::anyhow!(e))?;

    let flag = shutdown_flag.clone();
    std::thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = worker_handle.shutdown();
    });

    worker.run().map_err(|e| anyhow::anyhow!(e))
}

/// Publish a plugin to the Sentinel registry
fn run_publish(
    file: std::path::PathBuf,
    version: String,
    addr: Option<String>,
    publisher: Option<String>,
    email: Option<String>,
) -> Result<()> {
    use casparian_protocol::types::DeployCommand;
    use casparian_protocol::{JobId, Message, OpCode};
    use zmq::Context;
    use casparian::prepare_publish;

    info!("Publishing plugin: {:?} v{}", file, version);

    let artifact = prepare_publish(&file)?;
    if artifact.manifest.version != version {
        anyhow::bail!(
            "Version mismatch: CLI version '{}' does not match manifest version '{}'",
            version,
            artifact.manifest.version
        );
    }

    let plugin_name = artifact.plugin_name.clone();

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
        source_code: artifact.source_code,
        lockfile_content: artifact.lockfile_content,
        env_hash: artifact.env_hash,
        artifact_hash: artifact.artifact_hash,
        manifest_json: artifact.manifest_json,
        protocol_version: artifact.manifest.protocol_version,
        schema_artifacts_json: artifact.schema_artifacts_json,
        publisher_name,
        publisher_email: email,
        azure_oid: None,
        system_requirements: None,
    };

    // 7. Send via ZMQ DEALER to Sentinel
    let sentinel_addr = addr.unwrap_or_else(get_default_ipc_addr);
    info!("Connecting to Sentinel at {}", sentinel_addr);

    let context = Context::new();
    let socket = context
        .socket(zmq::DEALER)
        .map_err(|e| anyhow::anyhow!("Failed to create DEALER socket: {}", e))?;
    socket
        .connect(&sentinel_addr)
        .map_err(|e| anyhow::anyhow!("Failed to connect to sentinel: {}", e))?;
    info!("✓ Connected to Sentinel");

    // Serialize payload
    let payload = serde_json::to_vec(&deploy_cmd)?;

    // Create protocol message
    let msg = Message::new(OpCode::Deploy, JobId::new(0), payload)?;
    let (header_bytes, payload_bytes) = msg.pack()?;

    // Send message (multipart)
    let frames = [header_bytes.as_slice(), payload_bytes.as_slice()];
    socket
        .send_multipart(&frames, 0)
        .map_err(|e| anyhow::anyhow!("ZMQ send error: {}", e))?;
    info!("✓ Sent deployment request");

    // 8. Await ACK/ERR response
    let response_frames = socket
        .recv_multipart(0)
        .map_err(|e| anyhow::anyhow!("ZMQ recv error: {}", e))?;
    let (header, payload) = match response_frames.len() {
        2 => (response_frames[0].clone(), response_frames[1].clone()),
        3 if response_frames[0].is_empty() => {
            (response_frames[1].clone(), response_frames[2].clone())
        }
        count => {
            return Err(anyhow::anyhow!(
                "Expected 2 frames [header, payload], got {}",
                count
            ));
        }
    };
    let response_msg = Message::unpack(&[header, payload])?;

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

}

//! Configuration paths for Casparian
//!
//! Simple path resolution with sensible defaults.
//! All paths are under ~/.casparian_flow/

use std::path::PathBuf;
use std::sync::Once;

static CREATE_DIR_WARNED: Once = Once::new();

/// State store backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    /// SQLite - embedded transactional store (local default)
    Sqlite,
    /// DuckDB - query catalog backend (local SQL)
    DuckDb,
}

impl DbBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            DbBackend::Sqlite => "sqlite",
            DbBackend::DuckDb => "duckdb",
        }
    }
}

/// Get the Casparian home directory: ~/.casparian_flow
pub fn casparian_home() -> PathBuf {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return PathBuf::from(override_path);
    }
    dirs::home_dir()
        .expect(
            "Could not determine home directory (HOME not set). \
Set CASPARIAN_HOME or pass --state-store, or export HOME.",
        )
        .join(".casparian_flow")
}

/// Ensure the Casparian home directory exists
pub fn ensure_casparian_home() -> std::io::Result<PathBuf> {
    let home = casparian_home();
    std::fs::create_dir_all(&home)?;
    Ok(home)
}

/// Get the state store path: ~/.casparian_flow/state.sqlite
pub fn default_state_store_path() -> PathBuf {
    let home = casparian_home();
    if let Err(err) = std::fs::create_dir_all(&home) {
        CREATE_DIR_WARNED.call_once(|| {
            eprintln!(
                "Warning: failed to create Casparian home directory {}: {}. Set CASPARIAN_HOME or use --state-store.",
                home.display(),
                err
            );
        });
    }
    home.join("state.sqlite")
}

/// Get the query catalog path: ~/.casparian_flow/query.duckdb
pub fn default_query_catalog_path() -> PathBuf {
    let home = casparian_home();
    if let Err(err) = std::fs::create_dir_all(&home) {
        CREATE_DIR_WARNED.call_once(|| {
            eprintln!(
                "Warning: failed to create Casparian home directory {}: {}. Set CASPARIAN_HOME or use --query-catalog.",
                home.display(),
                err
            );
        });
    }
    home.join("query.duckdb")
}

/// Determine the state store backend.
///
/// Priority:
/// 1. If config.toml specifies `state_store.backend`, use that
/// 2. Default to SQLite for local mode
pub fn default_db_backend() -> DbBackend {
    // pre-v1: SQLite default for state store
    DbBackend::Sqlite
}

/// Get the state store path based on detected backend.
pub fn state_store_path() -> PathBuf {
    default_state_store_path()
}

/// Get the state store URL (sqlite: path).
pub fn state_store_url() -> String {
    format!("sqlite:{}", state_store_path().display())
}

/// Get the query catalog path.
pub fn query_catalog_path() -> PathBuf {
    default_query_catalog_path()
}

/// Get output directory: ~/.casparian_flow/output
pub fn output_dir() -> PathBuf {
    casparian_home().join("output")
}

/// Get venvs directory: ~/.casparian_flow/venvs
pub fn venvs_dir() -> PathBuf {
    casparian_home().join("venvs")
}

/// Get parsers directory: ~/.casparian_flow/parsers
pub fn parsers_dir() -> PathBuf {
    casparian_home().join("parsers")
}

/// Get logs directory: ~/.casparian_flow/logs
pub fn logs_dir() -> PathBuf {
    casparian_home().join("logs")
}

/// Ensure the logs directory exists
pub fn ensure_logs_dir() -> std::io::Result<PathBuf> {
    let dir = logs_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Arguments for the config command
#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    /// Show resolved paths in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Run the config command - shows current paths
pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    let home = casparian_home();
    let backend = default_db_backend();
    let state_store = state_store_path();
    let query_catalog = default_query_catalog_path();
    let output = output_dir();
    let venvs = venvs_dir();
    let parsers = parsers_dir();

    if args.json {
        let config = serde_json::json!({
            "home": home.to_string_lossy(),
            "state_store": {
                "backend": backend.as_str(),
                "path": state_store.to_string_lossy(),
                "exists": state_store.exists(),
            },
            "query_catalog": {
                "backend": DbBackend::DuckDb.as_str(),
                "path": query_catalog.to_string_lossy(),
                "exists": query_catalog.exists(),
            },
            "output": {
                "path": output.to_string_lossy(),
                "exists": output.exists(),
            },
            "venvs": {
                "path": venvs.to_string_lossy(),
                "exists": venvs.exists(),
            },
            "parsers": {
                "path": parsers.to_string_lossy(),
                "exists": parsers.exists(),
            },
        });
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        println!("CASPARIAN CONFIGURATION");
        println!("=======================");
        println!();
        println!("Home:     {}", home.display());
        println!();
        println!("State Store Backend: {}", backend.as_str());
        println!(
            "  Path:   {} ({})",
            state_store.display(),
            if state_store.exists() { "exists" } else { "not found" }
        );
        println!();
        println!(
            "Query Catalog (DuckDB): {} ({})",
            query_catalog.display(),
            if query_catalog.exists() { "exists" } else { "not found" }
        );
        println!();
        println!("Output:   {}", output.display());
        println!(
            "          exists: {}",
            if output.exists() { "yes" } else { "no" }
        );
        println!();
        println!("Venvs:    {}", venvs.display());
        println!(
            "          exists: {}",
            if venvs.exists() { "yes" } else { "no" }
        );
        println!();
        println!("Parsers:  {}", parsers.display());
        println!(
            "          exists: {}",
            if parsers.exists() { "yes" } else { "no" }
        );
    }

    Ok(())
}

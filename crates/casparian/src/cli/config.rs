//! Configuration paths for Casparian
//!
//! Simple path resolution with sensible defaults.
//! All paths are under ~/.casparian_flow/

use std::path::PathBuf;
use std::sync::Once;

static CREATE_DIR_WARNED: Once = Once::new();

/// Database backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    /// DuckDB - columnar OLAP (v1 default)
    DuckDb,
}

impl DbBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
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
        .expect("Could not determine home directory. Set CASPARIAN_HOME or pass --db-path.")
        .join(".casparian_flow")
}

/// Ensure the Casparian home directory exists
pub fn ensure_casparian_home() -> std::io::Result<PathBuf> {
    let home = casparian_home();
    std::fs::create_dir_all(&home)?;
    Ok(home)
}

/// Get the DuckDB database path: ~/.casparian_flow/casparian_flow.duckdb
pub fn default_duckdb_path() -> PathBuf {
    let home = casparian_home();
    if let Err(err) = std::fs::create_dir_all(&home) {
        CREATE_DIR_WARNED.call_once(|| {
            eprintln!(
                "Warning: failed to create Casparian home directory {}: {}. Set CASPARIAN_HOME or use --db-path.",
                home.display(),
                err
            );
        });
    }
    home.join("casparian_flow.duckdb")
}

/// Determine the active database backend.
///
/// Priority:
/// 1. If config.toml specifies `database.backend`, use that
/// 2. Default to DuckDB when available, otherwise SQLite
pub fn default_db_backend() -> DbBackend {
    // v1: DuckDB-only
    DbBackend::DuckDb
}

/// Get the active database path based on detected backend.
pub fn active_db_path() -> PathBuf {
    default_duckdb_path()
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
    let active_db = active_db_path();
    let duckdb_db = default_duckdb_path();
    let output = output_dir();
    let venvs = venvs_dir();
    let parsers = parsers_dir();

    if args.json {
        let config = serde_json::json!({
            "home": home.to_string_lossy(),
            "database": {
                "backend": backend.as_str(),
                "active_path": active_db.to_string_lossy(),
                "duckdb_path": duckdb_db.to_string_lossy(),
                "duckdb_exists": duckdb_db.exists(),
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
        println!("Database Backend: {}", backend.as_str());
        println!("  Active:  {}", active_db.display());
        println!("  DuckDB:  {} ({})", duckdb_db.display(), if duckdb_db.exists() { "exists" } else { "not found" });
        println!();
        println!("Output:   {}", output.display());
        println!("          exists: {}", if output.exists() { "yes" } else { "no" });
        println!();
        println!("Venvs:    {}", venvs.display());
        println!("          exists: {}", if venvs.exists() { "yes" } else { "no" });
        println!();
        println!("Parsers:  {}", parsers.display());
        println!("          exists: {}", if parsers.exists() { "yes" } else { "no" });
    }

    Ok(())
}

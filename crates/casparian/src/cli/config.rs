//! Configuration paths for Casparian
//!
//! Simple path resolution with sensible defaults.
//! All paths are under ~/.casparian_flow/

use std::path::PathBuf;

/// Database backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    /// SQLite - fallback, always available
    Sqlite,
    /// DuckDB - columnar OLAP, preferred when available
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
        .expect("Could not determine home directory")
        .join(".casparian_flow")
}

/// Ensure the Casparian home directory exists
pub fn ensure_casparian_home() -> std::io::Result<PathBuf> {
    let home = casparian_home();
    std::fs::create_dir_all(&home)?;
    Ok(home)
}

/// Get the SQLite database path: ~/.casparian_flow/casparian_flow.sqlite3
///
/// This is the canonical path for the SQLite database.
/// Ensures the directory exists before returning.
pub fn default_db_path() -> PathBuf {
    let home = casparian_home();
    // Ensure directory exists
    let _ = std::fs::create_dir_all(&home);
    home.join("casparian_flow.sqlite3")
}

/// Get the DuckDB database path: ~/.casparian_flow/casparian_flow.duckdb
pub fn default_duckdb_path() -> PathBuf {
    let home = casparian_home();
    let _ = std::fs::create_dir_all(&home);
    home.join("casparian_flow.duckdb")
}

/// Get the config file path: ~/.casparian_flow/config.toml
pub fn config_file_path() -> PathBuf {
    casparian_home().join("config.toml")
}

/// Determine the active database backend.
///
/// Priority:
/// 1. If config.toml specifies `database.backend`, use that
/// 2. Default to DuckDB when available, otherwise SQLite
pub fn default_db_backend() -> DbBackend {
    if let Ok(backend) = std::env::var("CASPARIAN_DB_BACKEND") {
        let backend = backend.to_lowercase();
        if backend == "duckdb" {
            return DbBackend::DuckDb;
        }
        if backend == "sqlite" {
            return DbBackend::Sqlite;
        }
    }
    // Check config.toml first
    let config_path = config_file_path();
    if config_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            // Simple TOML parsing for database.backend
            // Look for: [database] then backend = "duckdb" or backend = "sqlite"
            let mut in_database_section = false;
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') {
                    in_database_section = trimmed == "[database]";
                } else if in_database_section && trimmed.starts_with("backend") {
                    if trimmed.contains("duckdb") {
                        return DbBackend::DuckDb;
                    } else if trimmed.contains("sqlite") {
                        return DbBackend::Sqlite;
                    }
                }
            }
        }
    }

    // Default to DuckDB if compiled in
    #[cfg(feature = "duckdb")]
    {
        return DbBackend::DuckDb;
    }

    // Fallback to SQLite
    #[cfg(not(feature = "duckdb"))]
    {
        return DbBackend::Sqlite;
    }
}

/// Get the active database path based on detected backend.
pub fn active_db_path() -> PathBuf {
    match default_db_backend() {
        DbBackend::Sqlite => default_db_path(),
        DbBackend::DuckDb => default_duckdb_path(),
    }
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
    let sqlite_db = default_db_path();
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
                "sqlite_path": sqlite_db.to_string_lossy(),
                "sqlite_exists": sqlite_db.exists(),
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
        println!("  SQLite:  {} ({})", sqlite_db.display(), if sqlite_db.exists() { "exists" } else { "not found" });
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

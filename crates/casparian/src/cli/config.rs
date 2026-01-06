//! Configuration paths for Casparian
//!
//! Simple path resolution with sensible defaults.
//! No config files - just env vars and defaults.
//!
//! Resolution order:
//! 1. CLI flag (if provided)
//! 2. Environment variable
//! 3. Default (~/.casparian_flow/...)

use std::path::PathBuf;

/// Get the Casparian home directory: ~/.casparian_flow
pub fn casparian_home() -> PathBuf {
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

/// Get database path: CLI flag > CASPARIAN_DB env > ~/.casparian_flow/casparian.db
pub fn resolve_db_path(cli_override: Option<PathBuf>) -> PathBuf {
    // 1. CLI flag
    if let Some(p) = cli_override {
        return p;
    }

    // 2. Environment variable
    if let Ok(p) = std::env::var("CASPARIAN_DB") {
        return PathBuf::from(p);
    }

    // 3. Default
    casparian_home().join("casparian.db")
}

/// Get output directory: CLI flag > CASPARIAN_OUTPUT env > ~/.casparian_flow/output
pub fn resolve_output_dir(cli_override: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli_override {
        return p;
    }

    if let Ok(p) = std::env::var("CASPARIAN_OUTPUT") {
        return PathBuf::from(p);
    }

    casparian_home().join("output")
}

/// Get venvs directory: ~/.casparian_flow/venvs (rarely needs override)
pub fn venvs_dir() -> PathBuf {
    casparian_home().join("venvs")
}

/// Arguments for the config command
#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    /// Show resolved paths in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Run the config command - shows current resolved paths
pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    let home = casparian_home();
    let db = resolve_db_path(None);
    let output = resolve_output_dir(None);
    let venvs = venvs_dir();

    // Check what's from env vs default
    let db_source = if std::env::var("CASPARIAN_DB").is_ok() { "CASPARIAN_DB" } else { "default" };
    let output_source = if std::env::var("CASPARIAN_OUTPUT").is_ok() { "CASPARIAN_OUTPUT" } else { "default" };

    if args.json {
        let config = serde_json::json!({
            "home": home.to_string_lossy(),
            "database": {
                "path": db.to_string_lossy(),
                "source": db_source,
                "exists": db.exists(),
            },
            "output": {
                "path": output.to_string_lossy(),
                "source": output_source,
                "exists": output.exists(),
            },
            "venvs": {
                "path": venvs.to_string_lossy(),
                "exists": venvs.exists(),
            },
        });
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        println!("CASPARIAN CONFIGURATION");
        println!("=======================");
        println!();
        println!("Home:     {}", home.display());
        println!();
        println!("Database: {}", db.display());
        println!("          source: {}", db_source);
        println!("          exists: {}", if db.exists() { "yes" } else { "no" });
        println!();
        println!("Output:   {}", output.display());
        println!("          source: {}", output_source);
        println!("          exists: {}", if output.exists() { "yes" } else { "no" });
        println!();
        println!("Venvs:    {}", venvs.display());
        println!("          exists: {}", if venvs.exists() { "yes" } else { "no" });
        println!();
        println!("ENVIRONMENT VARIABLES");
        println!("---------------------");
        println!("CASPARIAN_DB     = {}", std::env::var("CASPARIAN_DB").unwrap_or_else(|_| "(not set)".into()));
        println!("CASPARIAN_OUTPUT = {}", std::env::var("CASPARIAN_OUTPUT").unwrap_or_else(|_| "(not set)".into()));
    }

    Ok(())
}

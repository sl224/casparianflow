//! Configuration paths for Casparian
//!
//! Simple path resolution with sensible defaults.
//! All paths are under ~/.casparian_flow/

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

/// Get the database path: ~/.casparian_flow/casparian_flow.sqlite3
///
/// This is the canonical path for the single Casparian database.
/// Ensures the directory exists before returning.
pub fn default_db_path() -> PathBuf {
    let home = casparian_home();
    // Ensure directory exists
    let _ = std::fs::create_dir_all(&home);
    home.join("casparian_flow.sqlite3")
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
    let db = default_db_path();
    let output = output_dir();
    let venvs = venvs_dir();
    let parsers = parsers_dir();

    if args.json {
        let config = serde_json::json!({
            "home": home.to_string_lossy(),
            "database": {
                "path": db.to_string_lossy(),
                "exists": db.exists(),
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
        println!("Database: {}", db.display());
        println!("          exists: {}", if db.exists() { "yes" } else { "no" });
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

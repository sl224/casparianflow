use std::path::PathBuf;
use std::sync::Once;

static CREATE_DIR_WARNED: Once = Once::new();

/// Resolve Casparian home directory.
///
/// Priority:
/// 1) CASPARIAN_HOME
/// 2) HOME/USERPROFILE
/// 3) ./.casparian_flow
pub fn casparian_home() -> PathBuf {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return PathBuf::from(override_path);
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".casparian_flow");
    }
    PathBuf::from(".").join(".casparian_flow")
}

fn ensure_home_dir(home: &PathBuf) {
    if let Err(err) = std::fs::create_dir_all(home) {
        CREATE_DIR_WARNED.call_once(|| {
            eprintln!(
                "Warning: failed to create Casparian home directory {}: {}. Set CASPARIAN_HOME or pass --state-store.",
                home.display(),
                err
            );
        });
    }
}

/// Default state store path: ~/.casparian_flow/state.sqlite
pub fn default_state_store_path() -> PathBuf {
    let home = casparian_home();
    ensure_home_dir(&home);
    home.join("state.sqlite")
}

/// Default query catalog path: ~/.casparian_flow/query.duckdb
pub fn default_query_catalog_path() -> PathBuf {
    let home = casparian_home();
    ensure_home_dir(&home);
    home.join("query.duckdb")
}

/// Default logs directory: ~/.casparian_flow/logs
pub fn default_logs_dir() -> PathBuf {
    let home = casparian_home();
    ensure_home_dir(&home);
    home.join("logs")
}

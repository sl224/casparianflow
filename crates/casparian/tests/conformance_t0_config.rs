mod cli_support;

use cli_support::{init_scout_schema, run_cli_json_value};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn conf_t0_config_json_paths() {
    let home_dir = TempDir::new().expect("create temp home");
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [("CASPARIAN_HOME", home_str.as_str()), ("RUST_LOG", "error")];

    let config_args = vec!["config".to_string(), "--json".to_string()];
    let config = run_cli_json_value(&config_args, &envs);

    let home = config
        .get("home")
        .and_then(|value| value.as_str())
        .expect("config.home present");
    assert_eq!(Path::new(home), home_dir.path());

    let database = config
        .get("database")
        .and_then(|value| value.as_object())
        .expect("config.database present");
    let backend = database
        .get("backend")
        .and_then(|value| value.as_str())
        .expect("database.backend present");
    assert_eq!(backend, "duckdb");

    let active_path = database
        .get("active_path")
        .and_then(|value| value.as_str())
        .expect("database.active_path present");
    let duckdb_path = database
        .get("duckdb_path")
        .and_then(|value| value.as_str())
        .expect("database.duckdb_path present");
    let expected_db_path = home_dir.path().join("casparian_flow.duckdb");
    assert_eq!(Path::new(active_path), expected_db_path);
    assert_eq!(Path::new(duckdb_path), expected_db_path);

    let duckdb_exists = database
        .get("duckdb_exists")
        .and_then(|value| value.as_bool())
        .expect("database.duckdb_exists present");
    assert!(!duckdb_exists, "duckdb should not exist yet");

    init_scout_schema(&expected_db_path);
    assert!(expected_db_path.exists(), "duckdb file created");

    let config_after = run_cli_json_value(&config_args, &envs);
    let database_after = config_after
        .get("database")
        .and_then(|value| value.as_object())
        .expect("config.database present");
    let duckdb_exists_after = database_after
        .get("duckdb_exists")
        .and_then(|value| value.as_bool())
        .expect("database.duckdb_exists present");
    assert!(duckdb_exists_after, "duckdb should exist after init");

    let active_path_after = database_after
        .get("active_path")
        .and_then(|value| value.as_str())
        .expect("database.active_path present");
    let duckdb_path_after = database_after
        .get("duckdb_path")
        .and_then(|value| value.as_str())
        .expect("database.duckdb_path present");
    assert_eq!(Path::new(active_path_after), expected_db_path);
    assert_eq!(Path::new(duckdb_path_after), expected_db_path);
}

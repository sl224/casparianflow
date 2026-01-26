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

    let state_store = config
        .get("state_store")
        .and_then(|value| value.as_object())
        .expect("config.state_store present");
    let backend = state_store
        .get("backend")
        .and_then(|value| value.as_str())
        .expect("state_store.backend present");
    assert_eq!(backend, "sqlite");

    let state_path = state_store
        .get("path")
        .and_then(|value| value.as_str())
        .expect("state_store.path present");
    let expected_db_path = home_dir.path().join("state.sqlite");
    assert_eq!(Path::new(state_path), expected_db_path);

    let state_exists = state_store
        .get("exists")
        .and_then(|value| value.as_bool())
        .expect("state_store.exists present");
    assert!(!state_exists, "state store should not exist yet");

    let query_catalog = config
        .get("query_catalog")
        .and_then(|value| value.as_object())
        .expect("config.query_catalog present");
    let query_path = query_catalog
        .get("path")
        .and_then(|value| value.as_str())
        .expect("query_catalog.path present");
    let query_exists = query_catalog
        .get("exists")
        .and_then(|value| value.as_bool())
        .expect("query_catalog.exists present");
    assert!(!query_exists, "query catalog should not exist yet");
    let expected_query_path = home_dir.path().join("query.duckdb");
    assert_eq!(Path::new(query_path), expected_query_path);

    init_scout_schema(&expected_db_path);
    assert!(expected_db_path.exists(), "state store file created");

    let config_after = run_cli_json_value(&config_args, &envs);
    let state_store_after = config_after
        .get("state_store")
        .and_then(|value| value.as_object())
        .expect("config.state_store present");
    let state_exists_after = state_store_after
        .get("exists")
        .and_then(|value| value.as_bool())
        .expect("state_store.exists present");
    assert!(state_exists_after, "state store should exist after init");

    let state_path_after = state_store_after
        .get("path")
        .and_then(|value| value.as_str())
        .expect("state_store.path present");
    assert_eq!(Path::new(state_path_after), expected_db_path);
}

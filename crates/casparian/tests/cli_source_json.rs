mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json, with_duckdb};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct SourceListItem {
    name: String,
    path: String,
    enabled: bool,
    files: u64,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct SourceDetails {
    name: String,
    path: String,
    enabled: bool,
    files: u64,
    size: u64,
    file_list: Option<Vec<SourceFile>>,
}

#[derive(Debug, Deserialize)]
struct SourceFile {
    path: String,
}

#[test]
fn test_source_json_and_sync() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let data_dir = TempDir::new().expect("create data dir");
    fs::write(data_dir.path().join("sample.csv"), "id,name\n1,A\n").unwrap();
    fs::write(data_dir.path().join("sample.json"), "{\"ok\":true}\n").unwrap();

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let add_args = vec![
        "source".to_string(),
        "add".to_string(),
        data_dir.path().to_string_lossy().to_string(),
        "--name".to_string(),
        "test_source".to_string(),
    ];
    assert_cli_success(&run_cli(&add_args, &envs), &add_args);

    let list_args = vec![
        "source".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let sources: Vec<SourceListItem> = run_cli_json(&list_args, &envs);
    let source = sources
        .iter()
        .find(|s| s.name == "test_source")
        .expect("source present");
    let canonical = data_dir
        .path()
        .canonicalize()
        .expect("canonicalize");
    assert_eq!(Path::new(&source.path), canonical);
    assert!(source.enabled);
    assert_eq!(source.files, 0);
    assert_eq!(source.size, 0);

    let sync_args = vec![
        "source".to_string(),
        "sync".to_string(),
        "test_source".to_string(),
    ];
    assert_cli_success(&run_cli(&sync_args, &envs), &sync_args);

    let show_args = vec![
        "source".to_string(),
        "show".to_string(),
        "test_source".to_string(),
        "--files".to_string(),
        "--json".to_string(),
    ];
    let details: SourceDetails = run_cli_json(&show_args, &envs);
    assert_eq!(details.name, "test_source");
    assert_eq!(Path::new(&details.path), canonical);
    assert!(details.enabled);
    assert!(details.files >= 2);
    assert!(details.size > 0);
    let file_list = details.file_list.expect("file list included");
    assert_eq!(file_list.len(), 2);
    assert!(file_list.iter().any(|f| f.path.ends_with("sample.csv")));
    assert!(file_list.iter().any(|f| f.path.ends_with("sample.json")));

    let dup_args = vec![
        "source".to_string(),
        "add".to_string(),
        data_dir.path().to_string_lossy().to_string(),
    ];
    let dup_output = run_cli(&dup_args, &envs);
    assert!(!dup_output.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&dup_output.stdout),
        String::from_utf8_lossy(&dup_output.stderr)
    );
    assert!(
        combined.contains("Source already exists") || combined.contains("name already exists"),
        "unexpected error output: {}",
        combined
    );

    let remove_args = vec![
        "source".to_string(),
        "remove".to_string(),
        "test_source".to_string(),
        "--force".to_string(),
    ];
    assert_cli_success(&run_cli(&remove_args, &envs), &remove_args);
    assert_eq!(source_count(&db_path), 0);
}

fn source_count(db_path: &Path) -> i64 {
    with_duckdb(db_path, |conn| {
        conn.query_scalar::<i64>("SELECT COUNT(*) FROM scout_sources", &[])
            .expect("count sources")
    })
}

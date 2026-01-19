use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn casparian_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_casparian"))
}

fn run_cli(args: &[String], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(casparian_bin());
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("failed to execute casparian CLI")
}

fn parse_json_output(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout
        .find(|c| c == '{' || c == '[')
        .unwrap_or_else(|| {
            panic!(
                "no JSON payload found in output\nstdout:\n{}\nstderr:\n{}",
                stdout,
                String::from_utf8_lossy(&output.stderr)
            )
        });
    let json_text = &stdout[json_start..];
    let mut deserializer = serde_json::Deserializer::from_str(json_text);
    serde_json::Value::deserialize(&mut deserializer).unwrap_or_else(|err| {
        panic!(
            "failed to parse JSON output: {}\nstdout:\n{}\nstderr:\n{}",
            err,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_cli_json<T: DeserializeOwned>(args: &[String], envs: &[(&str, &str)]) -> T {
    let output = run_cli(args, envs);
    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = parse_json_output(&output);
    serde_json::from_value(value).unwrap_or_else(|err| {
        panic!(
            "failed to deserialize JSON output: {}\nstdout:\n{}\nstderr:\n{}",
            err,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_cli_json_error(args: &[String], envs: &[(&str, &str)]) -> serde_json::Value {
    let output = run_cli(args, envs);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_json_output(&output)
}

#[derive(Debug, Deserialize)]
struct ScanResult {
    files: Vec<ScanFile>,
    summary: ScanSummary,
}

#[derive(Debug, Deserialize)]
struct ScanFile {
    path: PathBuf,
    extension: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScanSummary {
    total_files: usize,
    files_by_type: HashMap<String, usize>,
}

#[test]
fn test_scan_json_filters() {
    let home_dir = TempDir::new().expect("create temp home");
    let data_dir = TempDir::new().expect("create scan dir");
    let nested_dir = data_dir.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");

    fs::write(data_dir.path().join("root.csv"), "id,name\n1,A\n2,B\n").unwrap();
    fs::write(data_dir.path().join("root.json"), "{\"key\": 1}\n").unwrap();
    fs::write(data_dir.path().join("root.txt"), "hello\n").unwrap();
    fs::write(nested_dir.join("deep.csv"), "id\n1\n").unwrap();
    fs::write(data_dir.path().join("big.bin"), vec![0u8; 120_000]).unwrap();

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let scan_csv_args = vec![
        "scan".to_string(),
        data_dir.path().to_string_lossy().to_string(),
        "--json".to_string(),
        "--type".to_string(),
        "csv".to_string(),
    ];
    let scan_csv: ScanResult = run_cli_json(&scan_csv_args, &envs);
    assert_eq!(scan_csv.summary.total_files, 2);
    assert_eq!(scan_csv.summary.files_by_type.get("csv"), Some(&2));
    assert!(scan_csv
        .files
        .iter()
        .all(|f| f.extension.as_deref() == Some("csv")));

    let scan_min_size_args = vec![
        "scan".to_string(),
        data_dir.path().to_string_lossy().to_string(),
        "--json".to_string(),
        "--min-size".to_string(),
        "50KB".to_string(),
    ];
    let scan_min_size: ScanResult = run_cli_json(&scan_min_size_args, &envs);
    assert_eq!(scan_min_size.summary.total_files, 1);
    assert_eq!(
        scan_min_size.summary.files_by_type.get("bin"),
        Some(&1)
    );

    let scan_depth_args = vec![
        "scan".to_string(),
        data_dir.path().to_string_lossy().to_string(),
        "--json".to_string(),
        "--depth".to_string(),
        "1".to_string(),
    ];
    let scan_depth: ScanResult = run_cli_json(&scan_depth_args, &envs);
    assert_eq!(scan_depth.summary.total_files, 4);
    assert!(scan_depth
        .files
        .iter()
        .all(|f| !path_ends_with(&f.path, "deep.csv")));
}

#[test]
fn test_scan_json_error_invalid_path() {
    let home_dir = TempDir::new().expect("create temp home");
    let missing_path = home_dir.path().join("missing");
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let args = vec![
        "scan".to_string(),
        missing_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let value = run_cli_json_error(&args, &envs);
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("Path not found"),
        "unexpected error message: {}",
        message
    );
}

#[derive(Debug, Deserialize)]
struct PreviewResult {
    file_type: String,
    schema: Option<Vec<ColumnSchema>>,
    row_count: usize,
    headers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ColumnSchema {
    name: String,
    inferred_type: String,
}

#[test]
fn test_preview_json_outputs() {
    let data_dir = TempDir::new().expect("create preview dir");

    let csv_path = data_dir.path().join("sample.csv");
    fs::write(
        &csv_path,
        "id,name,price,active\n1,Widget,19.99,true\n2,Gadget,29.50,false\n3,Device,99.00,true\n",
    )
    .unwrap();

    let json_path = data_dir.path().join("sample.json");
    fs::write(
        &json_path,
        r#"[{"id":1,"name":"Alice","age":30},{"id":2,"name":"Bob","age":25},{"id":3,"name":"Cara","age":35}]"#,
    )
    .unwrap();

    let ndjson_path = data_dir.path().join("sample.jsonl");
    fs::write(
        &ndjson_path,
        r#"{"event":"login","user":"alice","ts":1704067200}
{"event":"click","user":"bob","ts":1704067300}
{"event":"logout","user":"alice","ts":1704067400}
"#,
    )
    .unwrap();

    let csv_args = vec![
        "preview".to_string(),
        csv_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let csv_result: PreviewResult = run_cli_json(&csv_args, &[]);
    assert_eq!(csv_result.file_type, "Csv");
    assert_eq!(csv_result.row_count, 3);
    assert!(csv_result.headers.contains(&"id".to_string()));
    let csv_schema = csv_result.schema.expect("csv schema");
    let csv_types: HashMap<String, String> = csv_schema
        .into_iter()
        .map(|col| (col.name, col.inferred_type))
        .collect();
    assert_eq!(csv_types.get("id").map(String::as_str), Some("integer"));
    assert_eq!(
        csv_types.get("price").map(String::as_str),
        Some("float")
    );
    assert_eq!(
        csv_types.get("active").map(String::as_str),
        Some("boolean")
    );
    assert_eq!(
        csv_types.get("name").map(String::as_str),
        Some("string")
    );

    let json_args = vec![
        "preview".to_string(),
        json_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let json_result: PreviewResult = run_cli_json(&json_args, &[]);
    assert_eq!(json_result.file_type, "Json");
    assert_eq!(json_result.row_count, 3);
    let json_schema = json_result.schema.expect("json schema");
    assert!(json_schema.iter().any(|col| {
        col.name == "name" && col.inferred_type == "string"
    }));

    let ndjson_args = vec![
        "preview".to_string(),
        ndjson_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let ndjson_result: PreviewResult = run_cli_json(&ndjson_args, &[]);
    assert_eq!(ndjson_result.file_type, "NdJson");
    assert_eq!(ndjson_result.row_count, 3);
}

#[test]
fn test_preview_json_error_missing_file() {
    let data_dir = TempDir::new().expect("create preview dir");
    let missing_path = data_dir.path().join("missing.csv");
    let args = vec![
        "preview".to_string(),
        missing_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let value = run_cli_json_error(&args, &[]);
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("File not found"),
        "unexpected error message: {}",
        message
    );
}

#[test]
fn test_run_json_error_missing_parser() {
    let data_dir = TempDir::new().expect("create run dir");
    let input_path = data_dir.path().join("input.csv");
    fs::write(&input_path, "id,name\n1,A\n").unwrap();
    let parser_path = data_dir.path().join("missing_parser.py");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        input_path.to_string_lossy().to_string(),
        "--json".to_string(),
    ];
    let value = run_cli_json_error(&args, &[]);
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("Parser not found"),
        "unexpected error message: {}",
        message
    );
}

fn path_ends_with(path: &Path, tail: &str) -> bool {
    path.to_string_lossy().ends_with(tail)
}

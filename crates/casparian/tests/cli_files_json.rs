mod cli_support;

use cli_support::{init_scout_schema, run_cli, run_cli_json, with_duckdb};
use casparian_db::{DbConnection, DbValue};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

const SOURCE_ID: i64 = 1;

#[derive(Debug, Deserialize)]
struct FilesOutput {
    files: Vec<FileOutput>,
    summary: FilesSummary,
    filters: FilesFilters,
}

#[derive(Debug, Deserialize)]
struct FileOutput {
    status: String,
    tag: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FilesSummary {
    total: usize,
    returned: usize,
    limit: usize,
    tagged: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct FilesFilters {
    status: Option<String>,
}

#[test]
fn test_files_json_filters_and_limits() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;

    with_duckdb(&db_path, |conn| {
        insert_source(&conn, SOURCE_ID, "test_source", "/data", now);

        let files = [
            (1, "/data/sales/report.csv", "sales/report.csv", 10_000, "pending", None, None),
            (2, "/data/sales/q1.csv", "sales/q1.csv", 5_000, "tagged", Some("sales"), None),
            (3, "/data/sales/q2.csv", "sales/q2.csv", 6_000, "processed", Some("sales"), None),
            (4, "/data/invoices/jan.json", "invoices/jan.json", 2_500, "processed", Some("invoices"), None),
            (5, "/data/invoices/corrupt.json", "invoices/corrupt.json", 1_200, "failed", Some("invoices"), None),
            (6, "/data/sales/bad.csv", "sales/bad.csv", 800, "failed", Some("sales"), Some("Row 15: invalid date format")),
            (7, "/data/logs/access.log", "logs/access.log", 50_000, "pending", None, None),
            (8, "/data/logs/error.log", "logs/error.log", 25_000, "pending", None, None),
        ];
        for (id, path, rel_path, size, status, tag, error) in files {
            insert_file(
                &conn,
                id,
                SOURCE_ID,
                path,
                rel_path,
                size,
                status,
                tag,
                error,
                now,
            );
        }
    });

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let base_args = vec!["files".to_string(), "--json".to_string()];
    let all_output: FilesOutput = run_cli_json(&base_args, &envs);
    assert_eq!(all_output.summary.total, 8);
    assert_eq!(all_output.summary.returned, 8);
    assert!(all_output.summary.tagged.is_none());

    let sales_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--topic".to_string(),
        "sales".to_string(),
    ];
    let sales_output: FilesOutput = run_cli_json(&sales_args, &envs);
    assert_eq!(sales_output.summary.total, 3);
    assert!(sales_output.files.iter().all(|f| f.tag.as_deref() == Some("sales")));

    let failed_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--status".to_string(),
        "failed".to_string(),
    ];
    let failed_output: FilesOutput = run_cli_json(&failed_args, &envs);
    assert_eq!(failed_output.summary.total, 2);
    assert!(failed_output.files.iter().all(|f| f.status == "failed"));
    assert!(failed_output.files.iter().any(|f| f.error.is_some()));

    let untagged_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--untagged".to_string(),
    ];
    let untagged_output: FilesOutput = run_cli_json(&untagged_args, &envs);
    assert_eq!(untagged_output.summary.total, 3);
    assert!(untagged_output.files.iter().all(|f| f.tag.is_none()));

    let combined_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--topic".to_string(),
        "sales".to_string(),
        "--status".to_string(),
        "failed".to_string(),
    ];
    let combined_output: FilesOutput = run_cli_json(&combined_args, &envs);
    assert_eq!(combined_output.summary.total, 1);

    let done_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--status".to_string(),
        "done".to_string(),
    ];
    let done_output: FilesOutput = run_cli_json(&done_args, &envs);
    assert_eq!(done_output.summary.total, 2);
    assert_eq!(done_output.filters.status.as_deref(), Some("processed"));

    let limit_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--limit".to_string(),
        "2".to_string(),
    ];
    let limit_output: FilesOutput = run_cli_json(&limit_args, &envs);
    assert_eq!(limit_output.summary.total, 8);
    assert_eq!(limit_output.summary.returned, 2);
    assert_eq!(limit_output.summary.limit, 2);

    let invalid_args = vec![
        "files".to_string(),
        "--json".to_string(),
        "--status".to_string(),
        "invalid".to_string(),
    ];
    let invalid_output = run_cli(&invalid_args, &envs);
    assert!(!invalid_output.status.success());
    let stdout = String::from_utf8_lossy(&invalid_output.stdout);
    let stderr = String::from_utf8_lossy(&invalid_output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("Invalid status") || combined.contains("TRY:"),
        "unexpected error output: {}",
        combined
    );
}

fn insert_source(conn: &DbConnection, id: i64, name: &str, path: &str, now: i64) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, 30, 1, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(name),
            DbValue::from(source_type),
            DbValue::from(path),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert source");
}

fn insert_file(
    conn: &DbConnection,
    id: i64,
    source_id: i64,
    path: &str,
    rel_path: &str,
    size: i64,
    status: &str,
    tag: Option<&str>,
    error: Option<&str>,
    now: i64,
) {
    let (parent_path, name) = split_rel_path(rel_path);
    let extension = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());
    conn.execute(
        "INSERT INTO scout_files (id, source_id, path, rel_path, parent_path, name, extension, size, mtime, status, tag, error, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(source_id),
            DbValue::from(path),
            DbValue::from(rel_path),
            DbValue::from(parent_path),
            DbValue::from(name),
            DbValue::from(extension),
            DbValue::from(size),
            DbValue::from(now),
            DbValue::from(status),
            DbValue::from(tag),
            DbValue::from(error),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert file");
}

fn split_rel_path(rel_path: &str) -> (String, String) {
    match rel_path.rfind('/') {
        Some(idx) => (
            rel_path[..idx].to_string(),
            rel_path[idx + 1..].to_string(),
        ),
        None => ("".to_string(), rel_path.to_string()),
    }
}

mod cli_support;

use cli_support::{init_scout_schema, run_cli_json, with_duckdb};
use casparian_db::{DbConnection, DbValue};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct TopicListItem {
    topic: String,
    files: u64,
    failed: u64,
}

#[derive(Debug, Deserialize)]
struct TopicShow {
    topic: String,
    files: TopicFiles,
    failures: Vec<TopicFailure>,
}

#[derive(Debug, Deserialize)]
struct TopicFiles {
    total: usize,
    failed: usize,
}

#[derive(Debug, Deserialize)]
struct TopicFailure {
    path: String,
    error: Option<String>,
}

#[test]
fn test_topic_json_outputs() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    with_duckdb(&db_path, |conn| async move {
        insert_source(&conn, "src-1", "test_source", "/data", now).await;

        let files = [
            (1, "/data/sales/a.csv", "sales/a.csv", 100, "processed", Some("sales"), None),
            (2, "/data/sales/b.csv", "sales/b.csv", 120, "failed", Some("sales"), Some("bad row")),
            (3, "/data/invoices/one.json", "invoices/one.json", 90, "pending", Some("invoices"), None),
            (4, "/data/invoices/two.json", "invoices/two.json", 95, "processed", Some("invoices"), None),
            (5, "/data/logs/untagged.log", "logs/untagged.log", 80, "pending", None, None),
        ];
        for (id, path, rel_path, size, status, tag, error) in files {
            insert_file(
                &conn,
                id,
                "src-1",
                path,
                rel_path,
                size,
                status,
                tag,
                error,
                now,
            )
            .await;
        }
    });

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let list_args = vec![
        "topic".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let topics: Vec<TopicListItem> = run_cli_json(&list_args, &envs);
    let sales = topics.iter().find(|t| t.topic == "sales").expect("sales topic");
    let invoices = topics
        .iter()
        .find(|t| t.topic == "invoices")
        .expect("invoices topic");
    assert_eq!(sales.files, 2);
    assert_eq!(sales.failed, 1);
    assert_eq!(invoices.files, 2);

    let show_args = vec![
        "topic".to_string(),
        "show".to_string(),
        "sales".to_string(),
        "--json".to_string(),
    ];
    let details: TopicShow = run_cli_json(&show_args, &envs);
    assert_eq!(details.topic, "sales");
    assert_eq!(details.files.total, 2);
    assert_eq!(details.files.failed, 1);
    assert_eq!(details.failures.len(), 1);
    assert!(details
        .failures
        .iter()
        .any(|f| f.path.ends_with("b.csv") && f.error.as_deref() == Some("bad row")));
}

async fn insert_source(conn: &DbConnection, id: &str, name: &str, path: &str, now: i64) {
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
    .await
    .expect("insert source");
}

async fn insert_file(
    conn: &DbConnection,
    id: i64,
    source_id: &str,
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
    .await
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

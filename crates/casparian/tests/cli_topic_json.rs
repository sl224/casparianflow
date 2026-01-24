mod cli_support;

use casparian::scout::WorkspaceId;
use casparian_db::{DbConnection, DbValue};
use cli_support::{init_scout_schema, run_cli_json, with_duckdb};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

const SOURCE_ID: i64 = 1;

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
    with_duckdb(&db_path, |conn| {
        let workspace_id = insert_workspace(&conn, now);
        insert_source(&conn, &workspace_id, SOURCE_ID, "test_source", "/data", now);

        let files = [
            (
                1,
                "/data/sales/a.csv",
                "sales/a.csv",
                100,
                "processed",
                Some("sales"),
                None,
            ),
            (
                2,
                "/data/sales/b.csv",
                "sales/b.csv",
                120,
                "failed",
                Some("sales"),
                Some("bad row"),
            ),
            (
                3,
                "/data/invoices/one.json",
                "invoices/one.json",
                90,
                "pending",
                Some("invoices"),
                None,
            ),
            (
                4,
                "/data/invoices/two.json",
                "invoices/two.json",
                95,
                "processed",
                Some("invoices"),
                None,
            ),
            (
                5,
                "/data/logs/untagged.log",
                "logs/untagged.log",
                80,
                "pending",
                None,
                None,
            ),
        ];
        for (id, path, rel_path, size, status, tag, error) in files {
            insert_file(
                &conn,
                &workspace_id,
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
    let envs = [("CASPARIAN_HOME", home_str.as_str()), ("RUST_LOG", "error")];

    let list_args = vec![
        "topic".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let topics: Vec<TopicListItem> = run_cli_json(&list_args, &envs);
    let sales = topics
        .iter()
        .find(|t| t.topic == "sales")
        .expect("sales topic");
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

fn insert_workspace(conn: &DbConnection, now: i64) -> WorkspaceId {
    let workspace_id = WorkspaceId::new();
    conn.execute(
        "INSERT INTO cf_workspaces (id, name, created_at) VALUES (?, ?, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from("Default"),
            DbValue::from(now),
        ],
    )
    .expect("insert workspace");
    workspace_id
}

fn insert_source(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    id: i64,
    name: &str,
    path: &str,
    now: i64,
) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, workspace_id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 30, 1, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(workspace_id.to_string()),
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
    workspace_id: &WorkspaceId,
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
        "INSERT INTO scout_files (id, workspace_id, source_id, path, rel_path, parent_path, name, extension, is_dir, size, mtime, status, error, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?)",
        &[
            DbValue::from(id),
            DbValue::from(workspace_id.to_string()),
            DbValue::from(source_id),
            DbValue::from(path),
            DbValue::from(rel_path),
            DbValue::from(parent_path),
            DbValue::from(name),
            DbValue::from(extension),
            DbValue::from(size),
            DbValue::from(now),
            DbValue::from(status),
            DbValue::from(error),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert file");

    if let Some(tag) = tag {
        insert_tag(conn, workspace_id, id, tag, now);
    }
}

fn insert_tag(conn: &DbConnection, workspace_id: &WorkspaceId, file_id: i64, tag: &str, now: i64) {
    conn.execute(
        "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at)
         VALUES (?, ?, ?, 'manual', NULL, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from(file_id),
            DbValue::from(tag),
            DbValue::from(now),
        ],
    )
    .expect("insert file tag");
}

fn split_rel_path(rel_path: &str) -> (String, String) {
    match rel_path.rfind('/') {
        Some(idx) => (rel_path[..idx].to_string(), rel_path[idx + 1..].to_string()),
        None => ("".to_string(), rel_path.to_string()),
    }
}

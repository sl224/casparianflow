mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json, with_duckdb};
use casparian_db::{DbConnection, DbValue};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

const SOURCE_ID: i64 = 1;

#[derive(Debug, Deserialize)]
struct RuleListItem {
    pattern: String,
    topic: String,
    priority: i32,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct RuleShow {
    pattern: String,
    topic: String,
    priority: i32,
    enabled: bool,
    matched: usize,
}

#[test]
fn test_rule_json_and_lifecycle() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    with_duckdb(&db_path, |conn| {
        insert_source(&conn, SOURCE_ID, "test_source", "/data", now);
        insert_file(
            &conn,
            1,
            SOURCE_ID,
            "/data/sample.csv",
            "sample.csv",
            100,
            "pending",
            None,
            None,
            now,
        );
    });

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("RUST_LOG", "error"),
    ];

    let add_args = vec![
        "rule".to_string(),
        "add".to_string(),
        "*.csv".to_string(),
        "--topic".to_string(),
        "sales".to_string(),
        "--priority".to_string(),
        "10".to_string(),
    ];
    assert_cli_success(&run_cli(&add_args, &envs), &add_args);

    let list_args = vec![
        "rule".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let rules: Vec<RuleListItem> = run_cli_json(&list_args, &envs);
    let rule = rules
        .iter()
        .find(|r| r.pattern == "*.csv")
        .expect("rule present");
    assert_eq!(rule.topic, "sales");
    assert_eq!(rule.priority, 10);
    assert!(rule.enabled);

    let show_args = vec![
        "rule".to_string(),
        "show".to_string(),
        "*.csv".to_string(),
        "--json".to_string(),
    ];
    let details: RuleShow = run_cli_json(&show_args, &envs);
    assert_eq!(details.pattern, "*.csv");
    assert_eq!(details.topic, "sales");
    assert_eq!(details.priority, 10);
    assert!(details.enabled);
    assert_eq!(details.matched, 1);

    let remove_args = vec![
        "rule".to_string(),
        "remove".to_string(),
        "*.csv".to_string(),
        "--force".to_string(),
    ];
    assert_cli_success(&run_cli(&remove_args, &envs), &remove_args);
    assert_eq!(rule_count(&db_path), 0);
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

fn rule_count(db_path: &Path) -> i64 {
    with_duckdb(db_path, |conn| {
        conn.query_scalar::<i64>("SELECT COUNT(*) FROM scout_tagging_rules", &[])
            .expect("count rules")
    })
}

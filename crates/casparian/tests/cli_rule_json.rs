mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, run_cli_json};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::Path;
use tempfile::TempDir;

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
    let db_path = home_dir.path().join("casparian_flow.sqlite3");
    init_scout_schema(&db_path);

    let now = 1_737_187_200_000i64;
    let conn = Connection::open(&db_path).expect("open sqlite db");
    insert_source(&conn, "src-1", "test_source", "/data", now);
    insert_file(
        &conn,
        1,
        "src-1",
        "/data/sample.csv",
        "sample.csv",
        100,
        "pending",
        None,
        None,
        now,
    );

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
        ("CASPARIAN_DB_BACKEND", "sqlite"),
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
    assert_eq!(details.topic, "sales");
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

fn insert_source(conn: &Connection, id: &str, name: &str, path: &str, now: i64) {
    let source_type = serde_json::json!({ "type": "local" }).to_string();
    conn.execute(
        "INSERT INTO scout_sources (id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 30, 1, ?5, ?6)",
        params![id, name, source_type, path, now, now],
    )
    .expect("insert source");
}

fn insert_file(
    conn: &Connection,
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
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            source_id,
            path,
            rel_path,
            parent_path,
            name,
            extension,
            size,
            now,
            status,
            tag,
            error,
            now,
            now
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
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.query_row("SELECT COUNT(*) FROM scout_tagging_rules", [], |row| row.get(0))
        .expect("count rules")
}

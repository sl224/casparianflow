mod cli_support;

use casparian::scout::{FileStatus, TaggingRuleId, WorkspaceId};
use casparian_db::{DbConnection, DbValue};
use casparian_sentinel::JobQueue;
use cli_support::{init_scout_schema, with_duckdb};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const SOURCE_ID: i64 = 1;

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

fn assert_cli_success(output: Output, args: &[String]) {
    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_tag_and_untag_update_sqlite_db() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [("CASPARIAN_HOME", home_str.as_str())];
    let now = 1_737_187_200_000i64;
    let workspace_id = WorkspaceId::new();

    {
        with_duckdb(&db_path, |conn| {
            insert_workspace(&conn, &workspace_id, "Default", now);
            insert_source(&conn, &workspace_id, SOURCE_ID, "test_source", "/data", now);
            insert_rule(
                &conn,
                &workspace_id,
                "csv_rule",
                "*.csv",
                "csv_data",
                10,
                now,
            );
            insert_rule(
                &conn,
                &workspace_id,
                "json_rule",
                "*.json",
                "json_data",
                5,
                now,
            );

            let files = [
                (1i64, "/data/sales.csv", "sales.csv", 1000),
                (2i64, "/data/invoices.csv", "invoices.csv", 2000),
                (3i64, "/data/config.json", "config.json", 500),
                (4i64, "/data/readme.txt", "readme.txt", 100),
                (5i64, "/data/unknown.xyz", "unknown.xyz", 50),
            ];
            for (id, path, rel_path, size) in files {
                insert_file(
                    &conn,
                    &workspace_id,
                    id,
                    SOURCE_ID,
                    path,
                    rel_path,
                    size,
                    FileStatus::Pending.as_str(),
                    now,
                );
            }
        });
    }

    let tag_args = vec!["tag".to_string()];
    assert_cli_success(run_cli(&tag_args, &envs), &tag_args);

    {
        let (csv_count, json_count, untagged_count) = with_duckdb(&db_path, |conn| {
            let csv_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) FROM scout_file_tags WHERE workspace_id = ? AND tag = 'csv_data'",
                    &[DbValue::from(workspace_id.to_string())],
                )
                .unwrap();
            let json_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) FROM scout_file_tags WHERE workspace_id = ? AND tag = 'json_data'",
                    &[DbValue::from(workspace_id.to_string())],
                )
                .unwrap();
            let untagged_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) \
                     FROM scout_files f \
                     LEFT JOIN scout_file_tags t \
                        ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
                     WHERE f.workspace_id = ? AND t.file_id IS NULL",
                    &[DbValue::from(workspace_id.to_string())],
                )
                .unwrap();
            (csv_count, json_count, untagged_count)
        });
        assert_eq!(csv_count, 2);
        assert_eq!(json_count, 1);
        assert_eq!(untagged_count, 2);
    }

    let manual_args = vec![
        "tag".to_string(),
        "/data/unknown.xyz".to_string(),
        "custom_tag".to_string(),
    ];
    assert_cli_success(run_cli(&manual_args, &envs), &manual_args);

    {
        let (tag, tag_source) = with_duckdb(&db_path, |conn| {
            let row = conn
                .query_optional(
                    "SELECT t.tag, t.tag_source \
                     FROM scout_file_tags t \
                     JOIN scout_files f ON f.id = t.file_id \
                     WHERE f.workspace_id = ? AND f.path = ?",
                    &[
                        DbValue::from(workspace_id.to_string()),
                        DbValue::from("/data/unknown.xyz"),
                    ],
                )
                .unwrap();
            row.and_then(|r| {
                let tag: Option<String> = r.get_by_name("tag").ok().flatten();
                let tag_source: Option<String> = r.get_by_name("tag_source").ok().flatten();
                Some((tag, tag_source))
            })
            .unwrap_or((None, None))
        });
        assert_eq!(tag.as_deref(), Some("custom_tag"));
        assert_eq!(tag_source.as_deref(), Some("manual"));
    }

    let untag_args = vec!["untag".to_string(), "/data/unknown.xyz".to_string()];
    assert_cli_success(run_cli(&untag_args, &envs), &untag_args);

    {
        let (tag_count, status) = with_duckdb(&db_path, |conn| {
            let tag_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) \
                     FROM scout_file_tags t \
                     JOIN scout_files f ON f.id = t.file_id \
                     WHERE f.workspace_id = ? AND f.path = ?",
                    &[
                        DbValue::from(workspace_id.to_string()),
                        DbValue::from("/data/unknown.xyz"),
                    ],
                )
                .unwrap();
            let status: String = conn
                .query_scalar(
                    "SELECT status FROM scout_files WHERE workspace_id = ? AND path = ?",
                    &[
                        DbValue::from(workspace_id.to_string()),
                        DbValue::from("/data/unknown.xyz"),
                    ],
                )
                .unwrap_or_default();
            (tag_count, status)
        });
        assert_eq!(tag_count, 0);
        assert_eq!(status, FileStatus::Pending.as_str());
    }
}

#[test]
fn test_pipeline_run_enqueues_jobs() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [("CASPARIAN_HOME", home_str.as_str())];
    let now = 1_737_187_200_000i64;
    let workspace_id = WorkspaceId::new();

    {
        with_duckdb(&db_path, |conn| {
            insert_workspace(&conn, &workspace_id, "Default", now);
            insert_source(&conn, &workspace_id, SOURCE_ID, "demo_source", "/data/demo", now);

            let queue = JobQueue::new(conn.clone());
            queue.init_queue_schema().unwrap();
            queue.init_registry_schema().unwrap();
            insert_plugin_manifest(&conn, "demo_parser", "1.0.0");

            let files = [
                (1i64, "/data/demo/a.csv", "a.csv"),
                (2i64, "/data/demo/b.csv", "b.csv"),
                (3i64, "/data/demo/c.csv", "c.csv"),
            ];
            for (id, path, rel_path) in files {
                insert_file(
                    &conn,
                    &workspace_id,
                    id,
                    SOURCE_ID,
                    path,
                    rel_path,
                    100,
                    FileStatus::Pending.as_str(),
                    now,
                );
                insert_tag(&conn, &workspace_id, id, "demo", now);
            }
        });
    }

    let pipeline_file = home_dir.path().join("pipeline.yaml");
    std::fs::write(
        &pipeline_file,
        r#"pipeline:
  name: demo_pipeline
  selection:
    tag: demo
  run:
    parser: demo_parser
"#,
    )
    .expect("write pipeline file");

    let apply_args = vec![
        "pipeline".to_string(),
        "apply".to_string(),
        pipeline_file.to_string_lossy().to_string(),
    ];
    assert_cli_success(run_cli(&apply_args, &envs), &apply_args);

    let run_args = vec![
        "pipeline".to_string(),
        "run".to_string(),
        "demo_pipeline".to_string(),
        "--logical-date".to_string(),
        "2025-01-01".to_string(),
    ];
    assert_cli_success(run_cli(&run_args, &envs), &run_args);

    let queued_count = with_duckdb(&db_path, |conn| {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE plugin_name = 'demo_parser'",
            &[],
        )
        .unwrap()
    });
    assert_eq!(queued_count, 3);
}

fn insert_workspace(conn: &DbConnection, workspace_id: &WorkspaceId, name: &str, now: i64) {
    conn.execute(
        "INSERT INTO cf_workspaces (id, name, created_at) VALUES (?, ?, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from(name),
            DbValue::from(now),
        ],
    )
    .expect("insert workspace");
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
        "INSERT INTO scout_sources (id, workspace_id, name, source_type, path, poll_interval_secs, enabled, created_at, updated_at)\n         VALUES (?, ?, ?, ?, ?, 30, 1, ?, ?)",
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

fn insert_rule(
    conn: &DbConnection,
    workspace_id: &WorkspaceId,
    name: &str,
    pattern: &str,
    tag: &str,
    priority: i64,
    now: i64,
) {
    let rule_id = TaggingRuleId::new();
    conn.execute(
        "INSERT INTO scout_rules (id, workspace_id, name, kind, pattern, tag, priority, enabled, created_at, updated_at)\n         VALUES (?, ?, ?, 'tagging', ?, ?, ?, 1, ?, ?)",
        &[
            DbValue::from(rule_id.to_string()),
            DbValue::from(workspace_id.to_string()),
            DbValue::from(name),
            DbValue::from(pattern),
            DbValue::from(tag),
            DbValue::from(priority),
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert rule");
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
    now: i64,
) {
    let (parent_path, name) = split_rel_path(rel_path);
    let extension = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());
    conn.execute(
        "INSERT INTO scout_files (id, workspace_id, source_id, path, rel_path, parent_path, name, extension, is_dir, size, mtime, status, error, first_seen_at, last_seen_at)\n         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, NULL, ?, ?)",
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
            DbValue::from(now),
            DbValue::from(now),
        ],
    )
    .expect("insert file");
}

fn insert_tag(conn: &DbConnection, workspace_id: &WorkspaceId, file_id: i64, tag: &str, now: i64) {
    conn.execute(
        "INSERT INTO scout_file_tags (workspace_id, file_id, tag, tag_source, rule_id, created_at)\n         VALUES (?, ?, ?, 'manual', NULL, ?)",
        &[
            DbValue::from(workspace_id.to_string()),
            DbValue::from(file_id),
            DbValue::from(tag),
            DbValue::from(now),
        ],
    )
    .expect("insert file tag");
}

fn insert_plugin_manifest(conn: &DbConnection, plugin_name: &str, version: &str) {
    let source_hash = format!("hash_{}_{}", plugin_name, version);
    let manifest_json = serde_json::json!({
        "name": plugin_name,
        "version": version,
        "protocol_version": "1.0",
        "runtime_kind": "python_shim",
        "entrypoint": format!("{}.py:parse", plugin_name),
    })
    .to_string();
    let outputs_json = "{}";
    conn.execute(
        "INSERT INTO cf_plugin_manifest (\n            plugin_name, version, runtime_kind, entrypoint,\n            source_code, source_hash, status, env_hash, artifact_hash,\n            manifest_json, protocol_version, schema_artifacts_json, outputs_json\n        ) VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?, ?)",
        &[
            DbValue::from(plugin_name),
            DbValue::from(version),
            DbValue::from("python_shim"),
            DbValue::from(format!("{}.py:parse", plugin_name)),
            DbValue::from("code"),
            DbValue::from(source_hash.as_str()),
            DbValue::from(source_hash.as_str()),
            DbValue::from(source_hash.as_str()),
            DbValue::from(manifest_json.as_str()),
            DbValue::from("1.0"),
            DbValue::from(outputs_json),
            DbValue::from(outputs_json),
        ],
    )
    .expect("insert plugin manifest");
}

fn split_rel_path(rel_path: &str) -> (String, String) {
    match rel_path.rfind('/') {
        Some(idx) => (rel_path[..idx].to_string(), rel_path[idx + 1..].to_string()),
        None => ("".to_string(), rel_path.to_string()),
    }
}

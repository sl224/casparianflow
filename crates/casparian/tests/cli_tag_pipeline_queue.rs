mod cli_support;

use casparian_db::{DbConnection, DbValue};
use cli_support::with_duckdb;
use std::path::PathBuf;
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

fn assert_cli_success(output: Output, args: &[String]) {
    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn create_tag_schema(conn: &DbConnection) {
    conn.execute_batch(
        r#"
        CREATE TABLE scout_tagging_rules (
            id TEXT PRIMARY KEY,
            name TEXT,
            source_id TEXT NOT NULL,
            pattern TEXT NOT NULL,
            tag TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            enabled INTEGER DEFAULT 1,
            created_at INTEGER,
            updated_at INTEGER
        );

        CREATE TABLE scout_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            size INTEGER NOT NULL,
            mtime INTEGER,
            content_hash TEXT,
            status TEXT DEFAULT 'pending',
            tag TEXT,
            tag_source TEXT,
            rule_id TEXT,
            manual_plugin TEXT,
            error TEXT,
            first_seen_at INTEGER,
            last_seen_at INTEGER,
            processed_at INTEGER,
            sentinel_job_id INTEGER
        );
        "#,
    )
    .await
    .expect("create tag schema");
}

#[test]
fn test_tag_and_untag_update_sqlite_db() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
    ];

    {
        with_duckdb(&db_path, |conn| async move {
            create_tag_schema(&conn).await;

            conn.execute(
                "INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled)
                 VALUES (?, ?, ?, ?, ?, 1)",
                &[
                    DbValue::from("r1"),
                    DbValue::from("src-1"),
                    DbValue::from("*.csv"),
                    DbValue::from("csv_data"),
                    DbValue::from(10i32),
                ],
            )
            .await
            .unwrap();
            conn.execute(
                "INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled)
                 VALUES (?, ?, ?, ?, ?, 1)",
                &[
                    DbValue::from("r2"),
                    DbValue::from("src-1"),
                    DbValue::from("*.json"),
                    DbValue::from("json_data"),
                    DbValue::from(5i32),
                ],
            )
            .await
            .unwrap();

            let files = [
                ("/data/sales.csv", "sales.csv", 1000),
                ("/data/invoices.csv", "invoices.csv", 2000),
                ("/data/config.json", "config.json", 500),
                ("/data/readme.txt", "readme.txt", 100),
                ("/data/unknown.xyz", "unknown.xyz", 50),
            ];
            for (path, rel_path, size) in files {
                conn.execute(
                    "INSERT INTO scout_files (source_id, path, rel_path, size, status)
                     VALUES ('src-1', ?, ?, ?, 'pending')",
                    &[
                        DbValue::from(path),
                        DbValue::from(rel_path),
                        DbValue::from(size),
                    ],
                )
                .await
                .unwrap();
            }
        });
    }

    let tag_args = vec!["tag".to_string()];
    assert_cli_success(run_cli(&tag_args, &envs), &tag_args);

    {
        let (csv_count, json_count, untagged_count) = with_duckdb(&db_path, |conn| async move {
            let csv_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) FROM scout_files WHERE tag = 'csv_data'",
                    &[],
                )
                .await
                .unwrap();
            let json_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) FROM scout_files WHERE tag = 'json_data'",
                    &[],
                )
                .await
                .unwrap();
            let untagged_count = conn
                .query_scalar::<i64>(
                    "SELECT COUNT(*) FROM scout_files WHERE tag IS NULL",
                    &[],
                )
                .await
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
        let (tag, tag_source) = with_duckdb(&db_path, |conn| async move {
            let row = conn
                .query_optional(
                    "SELECT tag, tag_source FROM scout_files WHERE path = ?",
                    &[DbValue::from("/data/unknown.xyz")],
                )
                .await
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
        let (final_tag, status) = with_duckdb(&db_path, |conn| async move {
            let row = conn
                .query_optional(
                    "SELECT tag, status FROM scout_files WHERE path = ?",
                    &[DbValue::from("/data/unknown.xyz")],
                )
                .await
                .unwrap();
            row.and_then(|r| {
                let final_tag: Option<String> = r.get_by_name("tag").ok().flatten();
                let status: String = r.get_by_name("status").unwrap_or_default();
                Some((final_tag, status))
            })
            .unwrap_or((None, String::new()))
        });
        assert!(final_tag.is_none());
        assert_eq!(status, "pending");
    }
}

#[test]
fn test_pipeline_run_enqueues_jobs() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [
        ("CASPARIAN_HOME", home_str.as_str()),
    ];

    {
        with_duckdb(&db_path, |conn| async move {
            conn.execute_batch(
                r#"
                CREATE TABLE scout_files (
                    id BIGINT,
                    source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    rel_path TEXT NOT NULL,
                    size BIGINT NOT NULL,
                    mtime BIGINT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    tag TEXT,
                    extension TEXT
                );
                "#,
            )
            .await
            .unwrap();

            let files = [
                ("/data/demo/a.csv", "a.csv"),
                ("/data/demo/b.csv", "b.csv"),
                ("/data/demo/c.csv", "c.csv"),
            ];
            for (path, rel_path) in files {
                conn.execute(
                    "INSERT INTO scout_files (source_id, path, rel_path, size, mtime, status, tag, extension)
                     VALUES ('src-1', ?, ?, 100, 1737187200000, 'pending', 'demo', 'csv')",
                    &[DbValue::from(path), DbValue::from(rel_path)],
                )
                .await
                .unwrap();
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

    let queued_count = with_duckdb(&db_path, |conn| async move {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM cf_processing_queue WHERE plugin_name = 'demo_parser'",
            &[],
        )
        .await
        .unwrap()
    });
    assert_eq!(queued_count, 3);
}

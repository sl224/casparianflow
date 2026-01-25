mod cli_support;

use cli_support::{assert_cli_success, init_scout_schema, run_cli, with_duckdb};
use casparian_db::DbValue;
use std::fs;
use tempfile::TempDir;

#[test]
fn conf_t1_rename_preserves_file_id_and_tags() {
    let home_dir = TempDir::new().expect("create temp home");
    let db_path = home_dir.path().join("casparian_flow.duckdb");
    init_scout_schema(&db_path);

    let data_dir = TempDir::new().expect("create data dir");
    let a_path = data_dir.path().join("a.txt");
    fs::write(&a_path, "alpha").unwrap();
    let a_canon = a_path.canonicalize().expect("canonicalize a.txt");

    let home_str = home_dir.path().to_string_lossy().to_string();
    let envs = [("CASPARIAN_HOME", home_str.as_str()), ("RUST_LOG", "error")];

    let add_args = vec![
        "source".to_string(),
        "add".to_string(),
        data_dir.path().to_string_lossy().to_string(),
        "--name".to_string(),
        "test_source".to_string(),
    ];
    assert_cli_success(&run_cli(&add_args, &envs), &add_args);

    let sync_args = vec![
        "source".to_string(),
        "sync".to_string(),
        "test_source".to_string(),
    ];
    assert_cli_success(&run_cli(&sync_args, &envs), &sync_args);

    let tag_args = vec![
        "tag".to_string(),
        a_canon.to_string_lossy().to_string(),
        "topic1".to_string(),
    ];
    assert_cli_success(&run_cli(&tag_args, &envs), &tag_args);

    let file_id: i64 = with_duckdb(&db_path, |conn| {
        conn.query_scalar(
            "SELECT id FROM scout_files WHERE path = ?",
            &[DbValue::from(a_canon.to_string_lossy().as_ref())],
        )
        .expect("file id")
    });

    let b_path = data_dir.path().join("b.txt");
    fs::rename(&a_path, &b_path).expect("rename file");
    let b_canon = b_path.canonicalize().expect("canonicalize b.txt");

    assert_cli_success(&run_cli(&sync_args, &envs), &sync_args);

    let renamed_id: i64 = with_duckdb(&db_path, |conn| {
        conn.query_scalar(
            "SELECT id FROM scout_files WHERE path = ?",
            &[DbValue::from(b_canon.to_string_lossy().as_ref())],
        )
        .expect("renamed id")
    });
    assert_eq!(renamed_id, file_id);

    let old_count: i64 = with_duckdb(&db_path, |conn| {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_files WHERE path = ?",
            &[DbValue::from(a_canon.to_string_lossy().as_ref())],
        )
        .expect("old path count")
    });
    assert_eq!(old_count, 0);

    let tag_count: i64 = with_duckdb(&db_path, |conn| {
        conn.query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_file_tags WHERE file_id = ? AND tag = ?",
            &[DbValue::from(file_id), DbValue::from("topic1")],
        )
        .expect("tag count")
    });
    assert_eq!(tag_count, 1);
}

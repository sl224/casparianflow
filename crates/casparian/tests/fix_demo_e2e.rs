use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn casparian_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_casparian"))
}

fn python_supports_fix_demo() -> bool {
    let check = r#"
import importlib.util
import sys

has_pandas = importlib.util.find_spec("pandas") is not None
has_pyarrow = importlib.util.find_spec("pyarrow") is not None
sys.exit(0 if (has_pandas or has_pyarrow) else 1)
"#;
    Command::new("python3")
        .arg("-c")
        .arg(check)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_fix_demo_lifecycle_query_by_cl_ord_id() {
    if !python_supports_fix_demo() {
        println!("SKIP: python3 with pandas or pyarrow is required for FIX demo parser");
        return;
    }

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("docs/demo/fix/fix_demo.fix");
    let parser_path = workspace_root.join("docs/demo/fix/fix_lifecycle_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_demo.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    let output = Command::new(casparian_bin())
        .args(&args)
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");
    let total_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_order_lifecycle", [], |row| row.get(0))
        .expect("query fix_order_lifecycle");
    assert!(total_rows > 0, "expected rows in fix_order_lifecycle");

    let mut stmt = conn
        .prepare(
            "SELECT msg_type, exec_type FROM fix_order_lifecycle \
             WHERE cl_ord_id = ? ORDER BY msg_seq_num",
        )
        .expect("prepare cl_ord_id query");

    let rows: Vec<(String, Option<String>)> = stmt
        .query_map(["CLORD1"], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query cl_ord_id")
        .map(|row| row.expect("row result"))
        .collect();

    let expected = vec![
        ("D".to_string(), None),
        ("8".to_string(), Some("0".to_string())),
        ("8".to_string(), Some("1".to_string())),
        ("8".to_string(), Some("2".to_string())),
    ];
    assert_eq!(rows, expected, "unexpected lifecycle rows for CLORD1");
}

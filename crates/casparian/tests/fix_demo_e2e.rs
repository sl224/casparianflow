use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn casparian_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_casparian"))
}

/// Wait for DuckDB file handles to be fully released after subprocess exits.
/// This prevents "table does not exist" errors caused by race conditions.
fn wait_for_duckdb_release() {
    std::thread::sleep(std::time::Duration::from_millis(200));
}

#[test]
fn test_fix_demo_lifecycle_query_by_cl_ord_id() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("docs/demo/fix/fix_demo.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
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
        .env("FIX_TZ", "UTC")
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

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

/// Test the first-class FIX parser with multi-output
#[test]
fn test_first_class_fix_parser_multi_output() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/mixed_messages.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_multi.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    // FIX_TZ is required
    let output = Command::new(casparian_bin())
        .args(&args)
        .env("FIX_TZ", "UTC")
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");

    // Verify fix_order_lifecycle table exists and has rows
    let order_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_order_lifecycle", [], |row| row.get(0))
        .expect("query fix_order_lifecycle");
    assert!(order_rows > 0, "expected rows in fix_order_lifecycle, got {}", order_rows);

    // Verify fix_session_events table exists and has rows
    let session_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_session_events", [], |row| row.get(0))
        .expect("query fix_session_events");
    assert!(session_rows > 0, "expected rows in fix_session_events, got {}", session_rows);

    // Verify context columns exist
    let has_source_path: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM fix_order_lifecycle WHERE source_path IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("check source_path");
    assert!(has_source_path, "source_path should be populated");

    // Verify __cf_row_id equals line_number
    let mismatched: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_order_lifecycle WHERE __cf_row_id != line_number",
            [],
            |row| row.get(0),
        )
        .expect("check __cf_row_id");
    assert_eq!(mismatched, 0, "__cf_row_id should equal line_number");

    // Verify raw_line_hash is populated
    let has_hash: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM fix_order_lifecycle WHERE raw_line_hash IS NOT NULL AND LENGTH(raw_line_hash) = 64",
            [],
            |row| row.get(0),
        )
        .expect("check raw_line_hash");
    assert!(has_hash, "raw_line_hash should be 64-char SHA256 hex");
}

/// Test FIX parser fails when FIX_TZ is not set
#[test]
fn test_fix_parser_requires_fix_tz() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/order_lifecycle.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let output_path = temp_dir.path().join("output");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("parquet://{}", output_path.display()),
    ];

    // Run WITHOUT FIX_TZ - should fail
    let output = Command::new(casparian_bin())
        .args(&args)
        .env_remove("FIX_TZ")
        .output()
        .expect("run casparian");

    assert!(
        !output.status.success(),
        "expected failure without FIX_TZ, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FIX_TZ") || stderr.contains("timezone"),
        "expected error about FIX_TZ, got: {}",
        stderr
    );
}

/// Test FIX_TAGS_ALLOWLIST enables fix_tags output
#[test]
fn test_fix_tags_allowlist() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/order_lifecycle.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_tags.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    // Run WITH FIX_TAGS_ALLOWLIST - should create fix_tags table
    let output = Command::new(casparian_bin())
        .args(&args)
        .env("FIX_TZ", "UTC")
        .env("FIX_TAGS_ALLOWLIST", "55,54,38,44")  // symbol, side, qty, price
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");

    // Verify fix_tags table exists and has rows
    let tag_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_tags", [], |row| row.get(0))
        .expect("query fix_tags");
    assert!(tag_rows > 0, "expected rows in fix_tags, got {}", tag_rows);

    // Verify only allowed tags are captured
    let disallowed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_tags WHERE tag NOT IN ('55', '54', '38', '44')",
            [],
            |row| row.get(0),
        )
        .expect("check allowed tags");
    assert_eq!(disallowed, 0, "fix_tags should only contain allowed tags");
}

/// Test FIX parser handles prefixed log lines (gateway headers, syslog, etc.)
#[test]
fn test_fix_parser_prefixed_logs() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/prefixed_logs.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_prefixed.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    let output = Command::new(casparian_bin())
        .args(&args)
        .env("FIX_TZ", "UTC")
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");

    // Verify fix_order_lifecycle captured messages despite prefixes
    let order_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_order_lifecycle", [], |row| row.get(0))
        .expect("query fix_order_lifecycle");
    assert!(order_rows > 0, "expected prefixed order messages, got {}", order_rows);

    // Verify session events captured
    let session_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_session_events", [], |row| row.get(0))
        .expect("query fix_session_events");
    assert!(session_rows > 0, "expected prefixed session messages, got {}", session_rows);

    // Verify message_index disambiguates multi-message lines
    let multi_message_lines: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ( \
             SELECT line_number, COUNT(DISTINCT message_index) AS cnt \
             FROM fix_session_events GROUP BY line_number HAVING cnt > 1 \
             ) t",
            [],
            |row| row.get(0),
        )
        .expect("check multi-message line message_index");
    assert!(
        multi_message_lines > 0,
        "expected at least one line with multiple message_index values"
    );

    // Verify source_fingerprint is populated (16-char hash)
    let has_fingerprint: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM fix_order_lifecycle WHERE source_fingerprint IS NOT NULL AND LENGTH(source_fingerprint) = 16",
            [],
            |row| row.get(0),
        )
        .expect("check source_fingerprint");
    assert!(has_fingerprint, "source_fingerprint should be 16-char hash");

    // Verify fix_parse_errors table exists (may be empty or have errors from malformed line)
    let errors_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_parse_errors", [], |row| row.get(0))
        .expect("fix_parse_errors should exist");
    // Table exists if we can query it (count may be 0)
    assert!(errors_count >= 0, "fix_parse_errors table should be queryable");

    // Check for the specific prefixed order - PREFIXED001
    let prefixed_order_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_order_lifecycle WHERE cl_ord_id = 'PREFIXED001'",
            [],
            |row| row.get(0),
        )
        .expect("query prefixed order");
    assert!(prefixed_order_count > 0, "expected to find PREFIXED001 order from prefixed logs");
}

/// Test source_fingerprint, message_index, and DECIMAL types in v1.2 schema
#[test]
fn test_fix_parser_v1_2_schema_features() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/mixed_messages.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_v11.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    let output = Command::new(casparian_bin())
        .args(&args)
        .env("FIX_TZ", "UTC")
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");

    // Verify source_fingerprint in all outputs
    let order_fp: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_order_lifecycle WHERE source_fingerprint IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("check order source_fingerprint");
    assert!(order_fp > 0, "fix_order_lifecycle should have source_fingerprint");

    let session_fp: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_session_events WHERE source_fingerprint IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("check session source_fingerprint");
    assert!(session_fp > 0, "fix_session_events should have source_fingerprint");

    // Verify message_index is populated
    let has_message_index: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM fix_order_lifecycle WHERE message_index IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("check message_index");
    assert!(has_message_index, "message_index should be populated");

    // Verify DECIMAL columns are present (they should contain numeric values)
    // Check that order_qty and price have values
    let has_decimals: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM fix_order_lifecycle WHERE order_qty IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("check DECIMAL order_qty");
    assert!(has_decimals, "DECIMAL order_qty should be populated");

    // Verify fix_parse_errors table exists (may be empty)
    let errors_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM fix_parse_errors", [], |row| row.get(0))
        .expect("fix_parse_errors should exist");
    assert!(errors_count >= 0, "fix_parse_errors table should be queryable");
}

/// Test decimal rounding emits parse error warnings
#[test]
fn test_fix_decimal_rounding_records_error() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let fixture_path = workspace_root.join("tests/fixtures/fix/decimal_rounding.fix");
    let parser_path = workspace_root.join("parsers/fix/fix_parser.py");
    assert!(fixture_path.exists(), "Missing fixture: {}", fixture_path.display());
    assert!(parser_path.exists(), "Missing parser: {}", parser_path.display());

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("fix_rounding.duckdb");

    let args = vec![
        "run".to_string(),
        parser_path.to_string_lossy().to_string(),
        fixture_path.to_string_lossy().to_string(),
        "--sink".to_string(),
        format!("duckdb://{}", db_path.display()),
    ];

    let output = Command::new(casparian_bin())
        .args(&args)
        .env("FIX_TZ", "UTC")
        .output()
        .expect("run casparian");

    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    wait_for_duckdb_release();

    let conn = duckdb::Connection::open(&db_path).expect("open duckdb output");

    let rounded_errors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fix_parse_errors WHERE error_reason LIKE '%Decimal rounded%'",
            [],
            |row| row.get(0),
        )
        .expect("query decimal rounding errors");
    assert!(rounded_errors > 0, "expected decimal rounding warnings");
}

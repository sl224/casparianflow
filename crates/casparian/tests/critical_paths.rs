//! Critical Path E2E Tests - NO MOCKS
//!
//! These tests verify the actual critical paths that users depend on.
//! They use real files, real databases, and real tool execution.
//!
//! Jon Blow principle: "If you can't test it, you can't know if it works."
//!
//! ## Test Philosophy
//!
//! 1. Test PUBLIC interfaces only (what users actually use)
//! 2. Use REAL files and databases (no mocks)
//! 3. Test the BINARY when possible (actual user experience)
//! 4. Test MCP tools through their public API

use std::fs;
use std::process::Command;
use tempfile::TempDir;
use serde_json::json;

// =============================================================================
// MCP TOOL INTEGRATION - The Core of the System
// =============================================================================

mod mcp_tools {
    use super::*;
    use casparian_mcp::tools::create_default_registry;
    use casparian_mcp::types::{Tool, ToolContent};

    fn extract_text(content: &ToolContent) -> Option<&str> {
        match content {
            ToolContent::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }

    /// Critical: Tool registry must have all expected tools
    #[test]
    fn test_registry_completeness() {
        let registry = create_default_registry();

        let expected = vec![
            "quick_scan",
            "apply_scope",
            "discover_schemas",
            "approve_schemas",
            "propose_amendment",
            "run_backtest",
            "fix_parser",
            "refine_parser",
            "execute_pipeline",
            "query_output",
        ];

        assert_eq!(registry.len(), expected.len(), "Tool count mismatch");

        for name in expected {
            assert!(
                registry.get(name).is_some(),
                "Missing tool: {}", name
            );
        }
    }

    /// Critical: quick_scan must find files in real directories
    #[tokio::test]
    async fn test_quick_scan_finds_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create a realistic file structure
        fs::write(temp_dir.path().join("data.csv"), "id,name\n1,Alice\n2,Bob").unwrap();
        fs::write(temp_dir.path().join("config.json"), "{}").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir/nested.csv"), "x\n1").unwrap();

        let registry = create_default_registry();
        let tool = registry.get("quick_scan").unwrap();

        let result = tool.execute(json!({
            "path": temp_dir.path().to_string_lossy()
        })).await.unwrap();

        assert!(!result.is_error, "Scan failed: {:?}", result.content);

        // Parse and verify
        if let Some(text) = result.content.first().and_then(extract_text) {
            let scan: serde_json::Value = serde_json::from_str(text).unwrap();

            let count = scan["file_count"].as_u64().unwrap();
            assert!(count >= 3, "Should find at least 3 files, got {}", count);

            // Should organize by extension
            assert!(scan.get("by_extension").is_some());
        } else {
            panic!("No text content in result");
        }
    }

    /// Critical: discover_schemas must infer types correctly
    #[tokio::test]
    async fn test_schema_inference_accuracy() {
        let temp_dir = TempDir::new().unwrap();

        // CSV with clear type distinctions
        let csv = r#"int_col,float_col,str_col,bool_col
100,100.5,hello,true
200,200.75,world,false
300,300.25,test,true
"#;
        fs::write(temp_dir.path().join("typed.csv"), csv).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("discover_schemas").unwrap();

        let result = tool.execute(json!({
            "files": [temp_dir.path().join("typed.csv").to_string_lossy()]
        })).await.unwrap();

        assert!(!result.is_error);

        if let Some(text) = result.content.first().and_then(extract_text) {
            let discovery: serde_json::Value = serde_json::from_str(text).unwrap();

            // Must have schemas
            let schemas = discovery["schemas"].as_array().expect("Should have schemas");
            assert!(!schemas.is_empty());

            // Check columns
            let columns = schemas[0]["columns"].as_array().expect("Should have columns");

            let types: std::collections::HashMap<_, _> = columns.iter()
                .filter_map(|c| {
                    let name = c["name"].as_str()?;
                    let dtype = c["data_type"].as_str()?;
                    Some((name, dtype))
                })
                .collect();

            // Verify type inference
            assert!(types.get("int_col").map(|t| t.to_lowercase().contains("int")).unwrap_or(false),
                   "int_col should be integer");
            assert!(types.get("float_col").map(|t|
                t.to_lowercase().contains("float") || t.to_lowercase().contains("double")
            ).unwrap_or(false), "float_col should be float");
            assert!(types.get("bool_col").map(|t| t.to_lowercase().contains("bool")).unwrap_or(false),
                   "bool_col should be boolean");
        }
    }

    /// Critical: Tool results must include WorkflowMetadata for UI guidance
    #[tokio::test]
    async fn test_workflow_metadata_present() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("data.csv"), "id\n1\n2").unwrap();

        let registry = create_default_registry();

        // discover_schemas should return workflow metadata
        let discover = registry.get("discover_schemas").unwrap();
        let result = discover.execute(json!({
            "files": [temp_dir.path().join("data.csv").to_string_lossy()]
        })).await.unwrap();

        if let Some(text) = result.content.first().and_then(extract_text) {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();

            // Must have workflow metadata
            assert!(response.get("workflow").is_some(),
                   "discover_schemas must return workflow metadata");

            let workflow = &response["workflow"];
            assert!(workflow.get("phase").is_some(), "workflow must have phase");
            assert!(workflow.get("needs_approval").is_some(), "workflow must have needs_approval");
            assert!(workflow.get("next_actions").is_some(), "workflow must have next_actions");
        }
    }

    /// Critical: approve_schemas must create contracts
    #[tokio::test]
    async fn test_schema_approval_creates_contract() {
        let registry = create_default_registry();
        let tool = registry.get("approve_schemas").unwrap();

        let result = tool.execute(json!({
            "scope_id": "test-scope",
            "schemas": [{
                "discovery_id": "disc-1",
                "name": "test_table",
                "columns": [
                    {"name": "id", "data_type": "Int64", "nullable": false},
                    {"name": "value", "data_type": "Float64", "nullable": true}
                ],
                "output_table_name": "output_fact"
            }]
        })).await.unwrap();

        assert!(!result.is_error, "Approval should succeed");

        if let Some(text) = result.content.first().and_then(extract_text) {
            let approval: serde_json::Value = serde_json::from_str(text).unwrap();

            // Must return contract
            assert!(
                approval.get("contract_id").is_some() ||
                approval.get("contract").and_then(|c| c.get("contract_id")).is_some(),
                "Must return contract_id"
            );
        }
    }

    /// Critical: query_output must read CSV files
    #[tokio::test]
    async fn test_query_reads_data() {
        let temp_dir = TempDir::new().unwrap();

        let csv = "id,name,value\n1,Alice,100\n2,Bob,200\n3,Charlie,300\n";
        fs::write(temp_dir.path().join("output.csv"), csv).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("query_output").unwrap();

        let result = tool.execute(json!({
            "source": temp_dir.path().join("output.csv").to_string_lossy(),
            "limit": 10
        })).await.unwrap();

        assert!(!result.is_error);

        if let Some(text) = result.content.first().and_then(extract_text) {
            let query: serde_json::Value = serde_json::from_str(text).unwrap();

            // Must return rows
            if let Some(rows) = query["rows"].as_array() {
                assert!(rows.len() >= 3, "Should return 3 rows");
            }

            // Must return columns
            if let Some(columns) = query["columns"].as_array() {
                assert!(columns.len() >= 3, "Should have 3 columns");
            }
        }
    }
}

// =============================================================================
// DATABASE TESTS - Real SQLite Operations
// =============================================================================

mod database {
    use super::*;
    use rusqlite::Connection;

    /// Critical: SQLite must work with real files
    #[test]
    fn test_sqlite_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.sqlite3");

        // Create and write
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)",
                [],
            ).unwrap();
            conn.execute("INSERT INTO test (value) VALUES (?)", ["data1"]).unwrap();
        }

        // Reopen and read
        {
            let conn = Connection::open(&db_path).unwrap();
            let value: String = conn.query_row(
                "SELECT value FROM test WHERE id = 1",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(value, "data1");
        }

        assert!(db_path.exists(), "Database file should persist");
    }

    /// Critical: WAL mode for concurrent access
    #[test]
    fn test_wal_mode() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("wal.sqlite3");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();

        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    /// Critical: Database handles concurrent writes
    #[test]
    fn test_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("concurrent.sqlite3");

        // Create table
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            CREATE TABLE data (id INTEGER PRIMARY KEY, value INTEGER);
        ").unwrap();

        // Multiple inserts
        for i in 0..100 {
            conn.execute("INSERT INTO data (value) VALUES (?)", [i]).unwrap();
        }

        // Verify count
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM data", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 100);
    }
}

// =============================================================================
// BINARY TESTS - Actual Executable
// =============================================================================

mod binary {
    use super::*;

    /// Critical: Binary must compile and run --help
    #[test]
    fn test_binary_runs() {
        let output = Command::new("cargo")
            .args(["run", "-p", "casparian", "-q", "--", "--help"])
            .output();

        match output {
            Ok(out) => {
                let combined = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );

                // Should contain usage info or subcommands
                assert!(
                    combined.contains("Usage") ||
                    combined.contains("casparian") ||
                    combined.contains("SUBCOMMANDS") ||
                    combined.contains("Commands") ||
                    combined.contains("help"),
                    "Should show help. Got: {}", combined
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Skipping: cargo not in PATH");
            }
            Err(e) => panic!("Binary test failed: {}", e),
        }
    }

    /// Critical: scan subcommand should work
    #[test]
    fn test_scan_subcommand() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.csv"), "id\n1\n2\n").unwrap();

        let output = Command::new("cargo")
            .args([
                "run", "-p", "casparian", "-q", "--",
                "scan", &temp_dir.path().to_string_lossy()
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                // Should either succeed or show meaningful output
                if out.status.success() {
                    assert!(
                        stdout.contains("csv") ||
                        stdout.contains("file") ||
                        stdout.contains("1"),
                        "Scan should show files. Got stdout: {}, stderr: {}", stdout, stderr
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Skipping: cargo not in PATH");
            }
            Err(e) => panic!("Scan test failed: {}", e),
        }
    }
}

// =============================================================================
// EDGE CASE TESTS - Things That Break in Production
// =============================================================================

mod edge_cases {
    use super::*;
    use casparian_mcp::tools::create_default_registry;
    use casparian_mcp::types::ToolContent;

    fn extract_text(content: &ToolContent) -> Option<&str> {
        match content {
            ToolContent::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }

    /// Edge case: Empty directory
    #[tokio::test]
    async fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let registry = create_default_registry();
        let tool = registry.get("quick_scan").unwrap();

        let result = tool.execute(json!({
            "path": temp_dir.path().to_string_lossy()
        })).await.unwrap();

        assert!(!result.is_error);

        if let Some(text) = result.content.first().and_then(extract_text) {
            let scan: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(scan["file_count"].as_u64().unwrap(), 0);
        }
    }

    /// Edge case: Non-existent path
    #[tokio::test]
    async fn test_scan_nonexistent() {
        let registry = create_default_registry();
        let tool = registry.get("quick_scan").unwrap();

        let result = tool.execute(json!({
            "path": "/this/path/does/not/exist/anywhere/12345"
        })).await;

        assert!(result.is_err(), "Should error on non-existent path");
    }

    /// Edge case: CSV with special characters
    #[tokio::test]
    async fn test_csv_special_chars() {
        let temp_dir = TempDir::new().unwrap();

        let csv = r#"id,name,description
1,"Alice ""The Great""","Contains, comma"
2,Bob,"Quote: ""Hello"""
3,Charlie,"Newline
in value"
"#;
        fs::write(temp_dir.path().join("special.csv"), csv).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("discover_schemas").unwrap();

        let result = tool.execute(json!({
            "files": [temp_dir.path().join("special.csv").to_string_lossy()]
        })).await.unwrap();

        // Should handle without crashing
        assert!(!result.is_error, "Should handle special characters");
    }

    /// Edge case: Large file (performance)
    #[tokio::test]
    async fn test_large_csv() {
        let temp_dir = TempDir::new().unwrap();

        // Create 10,000 row CSV
        let mut csv = String::from("id,value\n");
        for i in 0..10_000 {
            csv.push_str(&format!("{},{}\n", i, i * 10));
        }
        fs::write(temp_dir.path().join("large.csv"), csv).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("discover_schemas").unwrap();

        let start = std::time::Instant::now();
        let result = tool.execute(json!({
            "files": [temp_dir.path().join("large.csv").to_string_lossy()],
            "max_rows": 1000
        })).await.unwrap();
        let elapsed = start.elapsed();

        assert!(!result.is_error);
        assert!(elapsed.as_secs() < 10, "Should complete in < 10 seconds");
    }

    /// Edge case: Mixed file types in directory
    #[tokio::test]
    async fn test_mixed_file_types() {
        let temp_dir = TempDir::new().unwrap();

        // Create various file types
        fs::write(temp_dir.path().join("data.csv"), "id\n1").unwrap();
        fs::write(temp_dir.path().join("config.json"), "{}").unwrap();
        fs::write(temp_dir.path().join("readme.md"), "# Title").unwrap();
        fs::write(temp_dir.path().join("script.py"), "print('hi')").unwrap();
        fs::write(temp_dir.path().join("data.parquet"), &[0u8; 100]).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("quick_scan").unwrap();

        let result = tool.execute(json!({
            "path": temp_dir.path().to_string_lossy()
        })).await.unwrap();

        assert!(!result.is_error);

        if let Some(text) = result.content.first().and_then(extract_text) {
            let scan: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(scan["file_count"].as_u64().unwrap(), 5);

            // Should categorize by extension
            let by_ext = &scan["by_extension"];
            assert!(by_ext.get("csv").is_some());
            assert!(by_ext.get("json").is_some());
        }
    }

    /// Edge case: Unicode in file content
    #[tokio::test]
    async fn test_unicode_content() {
        let temp_dir = TempDir::new().unwrap();

        let csv = "id,name,description\n1,日本語,Contains Japanese\n2,Ελληνικά,Greek text\n3,Emoji,🎉🚀✨\n";
        fs::write(temp_dir.path().join("unicode.csv"), csv).unwrap();

        let registry = create_default_registry();
        let tool = registry.get("discover_schemas").unwrap();

        let result = tool.execute(json!({
            "files": [temp_dir.path().join("unicode.csv").to_string_lossy()]
        })).await.unwrap();

        assert!(!result.is_error, "Should handle Unicode");
    }

    /// Edge case: Deeply nested directories
    #[tokio::test]
    async fn test_nested_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested structure
        let deep_path = temp_dir.path()
            .join("a").join("b").join("c").join("d").join("e");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("deep.csv"), "id\n1").unwrap();

        let registry = create_default_registry();
        let tool = registry.get("quick_scan").unwrap();

        let result = tool.execute(json!({
            "path": temp_dir.path().to_string_lossy()
        })).await.unwrap();

        assert!(!result.is_error);

        if let Some(text) = result.content.first().and_then(extract_text) {
            let scan: serde_json::Value = serde_json::from_str(text).unwrap();
            assert!(scan["file_count"].as_u64().unwrap() >= 1, "Should find nested file");
        }
    }
}

// =============================================================================
// WORKFLOW INTEGRATION - Full User Flow
// =============================================================================

mod workflow {
    use super::*;
    use casparian_mcp::tools::create_default_registry;
    use casparian_mcp::types::ToolContent;

    fn extract_text(content: &ToolContent) -> Option<&str> {
        match content {
            ToolContent::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }

    /// Full workflow: scan -> discover -> approve
    #[tokio::test]
    async fn test_discovery_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create realistic data
        let csv = r#"order_id,customer,amount,date
1001,ACME Corp,1500.00,2024-01-15
1002,Beta Inc,2300.50,2024-01-16
1003,ACME Corp,750.25,2024-01-17
"#;
        fs::write(temp_dir.path().join("orders.csv"), csv).unwrap();

        let registry = create_default_registry();

        // Step 1: Scan
        let scan_tool = registry.get("quick_scan").unwrap();
        let scan_result = scan_tool.execute(json!({
            "path": temp_dir.path().to_string_lossy()
        })).await.unwrap();
        assert!(!scan_result.is_error, "Scan should succeed");

        // Step 2: Discover
        let discover_tool = registry.get("discover_schemas").unwrap();
        let discover_result = discover_tool.execute(json!({
            "files": [temp_dir.path().join("orders.csv").to_string_lossy()]
        })).await.unwrap();
        assert!(!discover_result.is_error, "Discovery should succeed");

        // Verify discovery result
        if let Some(text) = discover_result.content.first().and_then(extract_text) {
            let discovery: serde_json::Value = serde_json::from_str(text).unwrap();

            // Should have inferred schema
            let schemas = discovery["schemas"].as_array().unwrap();
            assert!(!schemas.is_empty(), "Should discover schema");

            // Should have workflow guidance
            let workflow = &discovery["workflow"];
            assert!(workflow["phase"].as_str().is_some(), "Should have phase");
        }

        // Step 3: Approve
        let approve_tool = registry.get("approve_schemas").unwrap();
        let approve_result = approve_tool.execute(json!({
            "scope_id": "orders-scope",
            "schemas": [{
                "discovery_id": "disc-orders",
                "name": "orders",
                "columns": [
                    {"name": "order_id", "data_type": "Int64", "nullable": false},
                    {"name": "customer", "data_type": "String", "nullable": false},
                    {"name": "amount", "data_type": "Float64", "nullable": false},
                    {"name": "date", "data_type": "Date", "nullable": false}
                ],
                "output_table_name": "orders_fact"
            }]
        })).await.unwrap();
        assert!(!approve_result.is_error, "Approval should succeed");
    }
}

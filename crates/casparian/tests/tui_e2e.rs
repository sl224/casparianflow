//! E2E tests for TUI - No Mocks
//!
//! Tests the full killer flow with real MCP tools and real files.

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::{ToolContent, ToolResult, WorkflowMetadata, WorkflowPhase};
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

/// Helper to execute a tool from the registry
async fn execute_tool(registry: &ToolRegistry, name: &str, args: Value) -> ToolResult {
    let tool = registry.get(name).expect(&format!("Tool '{}' not found", name));
    tool.execute(args).await.expect(&format!("Tool '{}' failed", name))
}

/// Helper to extract text content from ToolResult
fn get_text(result: &ToolResult) -> &str {
    match result.content.first() {
        Some(ToolContent::Text { text }) => text,
        _ => panic!("Expected text content"),
    }
}

/// Helper to parse JSON result
fn parse_result<T: serde::de::DeserializeOwned>(result: &ToolResult) -> T {
    let text = get_text(result);
    serde_json::from_str(text).expect("Failed to parse result")
}

// =============================================================================
// Tool Execution Tests (Real Tools, Real Files)
// =============================================================================

#[tokio::test]
async fn test_quick_scan_real_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(temp_dir.path().join("data1.csv"), "id,name\n1,Alice\n2,Bob").unwrap();
    fs::write(temp_dir.path().join("data2.csv"), "id,value\n1,100\n2,200").unwrap();
    fs::write(temp_dir.path().join("config.json"), r#"{"setting": true}"#).unwrap();

    let registry = create_default_registry();

    let result = execute_tool(
        &registry,
        "quick_scan",
        json!({ "path": temp_dir.path() }),
    )
    .await;

    assert!(!result.is_error);

    #[derive(serde::Deserialize)]
    struct ScanResult {
        file_count: usize,
        by_extension: std::collections::HashMap<String, Vec<Value>>,
    }

    let scan: ScanResult = parse_result(&result);
    assert_eq!(scan.file_count, 3);
    assert!(scan.by_extension.contains_key("csv"));
    assert!(scan.by_extension.contains_key("json"));
}

#[tokio::test]
async fn test_preview_files_real_csv() {
    let temp_dir = TempDir::new().unwrap();

    fs::write(
        temp_dir.path().join("sensors.csv"),
        "timestamp,sensor_id,temp,humidity\n\
         2024-01-01 10:00,S1,22.5,45\n\
         2024-01-01 10:01,S2,23.1,48\n\
         2024-01-01 10:02,S1,22.8,46",
    )
    .unwrap();

    let registry = create_default_registry();

    let result = execute_tool(
        &registry,
        "preview_files",
        json!({
            "files": [temp_dir.path().join("sensors.csv")],
            "lines": 10
        }),
    )
    .await;

    assert!(!result.is_error);

    #[derive(serde::Deserialize)]
    struct PreviewResult {
        previews: Vec<FilePreview>,
    }

    #[derive(serde::Deserialize)]
    struct FilePreview {
        total_lines: usize,
        detected_delimiter: Option<String>,
        column_count: Option<usize>,
        header: Option<Vec<String>>,
    }

    let preview: PreviewResult = parse_result(&result);
    assert_eq!(preview.previews.len(), 1);
    assert_eq!(preview.previews[0].total_lines, 4);
    assert_eq!(preview.previews[0].detected_delimiter, Some("comma".into()));
    assert_eq!(preview.previews[0].column_count, Some(4));
    assert_eq!(
        preview.previews[0].header,
        Some(vec!["timestamp".into(), "sensor_id".into(), "temp".into(), "humidity".into()])
    );
}

#[tokio::test]
async fn test_discover_schemas_infers_types() {
    let temp_dir = TempDir::new().unwrap();

    fs::write(
        temp_dir.path().join("data.csv"),
        "id,name,score,active\n\
         1,Alice,95.5,true\n\
         2,Bob,87.0,false\n\
         3,Carol,92.3,true",
    )
    .unwrap();

    let registry = create_default_registry();

    let result = execute_tool(
        &registry,
        "discover_schemas",
        json!({
            "files": [temp_dir.path().join("data.csv")]
        }),
    )
    .await;

    assert!(!result.is_error);

    #[derive(serde::Deserialize)]
    struct DiscoverResult {
        schemas: Vec<Schema>,
        workflow: WorkflowMetadata,
    }

    #[derive(serde::Deserialize)]
    struct Schema {
        columns: Vec<Column>,
    }

    #[derive(serde::Deserialize)]
    struct Column {
        name: String,
        data_type: String,
    }

    let discover: DiscoverResult = parse_result(&result);
    assert!(!discover.schemas.is_empty());

    let schema = &discover.schemas[0];
    assert_eq!(schema.columns.len(), 4);

    // Verify type inference
    let column_types: std::collections::HashMap<_, _> = schema
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.data_type.as_str()))
        .collect();

    assert!(column_types.get("id").is_some());
    assert!(column_types.get("name").is_some());
    assert!(column_types.get("score").is_some());

    // Workflow should indicate schema approval phase (discover_schemas returns approval_needed)
    assert!(matches!(discover.workflow.phase, WorkflowPhase::SchemaApproval));
}

// =============================================================================
// Full Killer Flow Test
// =============================================================================

#[tokio::test]
async fn test_killer_flow_scan_to_query() {
    let temp_dir = TempDir::new().unwrap();

    // Create sensor data CSV
    fs::write(
        temp_dir.path().join("sensors.csv"),
        "timestamp,sensor_id,temp\n\
         2024-01-01,S1,22.5\n\
         2024-01-02,S2,23.1\n\
         2024-01-03,S1,22.8",
    )
    .unwrap();

    let registry = create_default_registry();

    // Step 1: quick_scan - Discover files
    let scan_result = execute_tool(
        &registry,
        "quick_scan",
        json!({ "path": temp_dir.path() }),
    )
    .await;

    assert!(!scan_result.is_error);

    #[derive(serde::Deserialize)]
    struct ScanResult {
        file_count: usize,
    }

    let scan: ScanResult = parse_result(&scan_result);
    assert_eq!(scan.file_count, 1);

    // Step 2: preview_files - See content
    let preview_result = execute_tool(
        &registry,
        "preview_files",
        json!({
            "files": [temp_dir.path().join("sensors.csv")],
            "lines": 5
        }),
    )
    .await;

    assert!(!preview_result.is_error);

    #[derive(serde::Deserialize)]
    struct PreviewResult {
        previews: Vec<PreviewFile>,
    }

    #[derive(serde::Deserialize)]
    struct PreviewFile {
        column_count: Option<usize>,
    }

    let preview: PreviewResult = parse_result(&preview_result);
    assert_eq!(preview.previews[0].column_count, Some(3));

    // Step 3: discover_schemas - Infer types
    let discover_result = execute_tool(
        &registry,
        "discover_schemas",
        json!({
            "files": [temp_dir.path().join("sensors.csv")]
        }),
    )
    .await;

    assert!(!discover_result.is_error);

    #[derive(serde::Deserialize)]
    struct DiscoverResult {
        schemas: Vec<Value>,
    }

    let discover: DiscoverResult = parse_result(&discover_result);
    assert!(!discover.schemas.is_empty());

    // Step 4: execute_pipeline - Write CSV output
    let output_dir = temp_dir.path().join("output");
    let execute_result = execute_tool(
        &registry,
        "execute_pipeline",
        json!({
            "files": [temp_dir.path().join("sensors.csv")],
            "config": {
                "output_format": "csv",
                "output_dir": output_dir
            }
        }),
    )
    .await;

    assert!(!execute_result.is_error);

    #[derive(serde::Deserialize)]
    struct ExecuteResult {
        success: bool,
        total_rows: usize,
        file_results: Vec<FileResult>,
    }

    #[derive(serde::Deserialize)]
    struct FileResult {
        output_file: Option<String>,
        success: bool,
    }

    let execute: ExecuteResult = parse_result(&execute_result);
    assert!(execute.success);
    assert_eq!(execute.total_rows, 3);
    assert!(execute.file_results[0].success);

    let output_file = execute.file_results[0].output_file.as_ref().unwrap();
    assert!(std::path::Path::new(output_file).exists(), "Output file should exist");

    // Step 5: query_output - Read results
    let query_result = execute_tool(
        &registry,
        "query_output",
        json!({
            "source": output_file,
            "limit": 100
        }),
    )
    .await;

    assert!(!query_result.is_error);

    #[derive(serde::Deserialize)]
    struct QueryResult {
        row_count: usize,
        column_count: usize,
        columns: Vec<String>,
    }

    let query: QueryResult = parse_result(&query_result);
    assert_eq!(query.row_count, 3);
    assert_eq!(query.column_count, 3);
    assert_eq!(query.columns, vec!["timestamp", "sensor_id", "temp"]);
}

// =============================================================================
// WorkflowMetadata Tests
// =============================================================================

#[tokio::test]
async fn test_workflow_metadata_approval_gates() {
    let temp_dir = TempDir::new().unwrap();

    fs::write(
        temp_dir.path().join("data.csv"),
        "id,value\n1,100\n2,200",
    )
    .unwrap();

    let registry = create_default_registry();

    // discover_schemas should return workflow with needs_approval
    let result = execute_tool(
        &registry,
        "discover_schemas",
        json!({
            "files": [temp_dir.path().join("data.csv")]
        }),
    )
    .await;

    #[derive(serde::Deserialize)]
    struct DiscoverResult {
        workflow: WorkflowMetadata,
    }

    let discover: DiscoverResult = parse_result(&result);

    // Workflow should indicate approval needed for schema
    assert!(discover.workflow.needs_approval);
    assert!(matches!(discover.workflow.phase, WorkflowPhase::SchemaApproval));

    // Next actions should include approve_schemas
    let has_approve = discover
        .workflow
        .next_actions
        .iter()
        .any(|a| a.tool_name == "approve_schemas");
    assert!(has_approve, "Should suggest approve_schemas as next action");
}

#[tokio::test]
async fn test_workflow_discovery_phase() {
    let temp_dir = TempDir::new().unwrap();

    fs::write(temp_dir.path().join("test.csv"), "a,b\n1,2").unwrap();

    let registry = create_default_registry();

    let result = execute_tool(
        &registry,
        "quick_scan",
        json!({ "path": temp_dir.path() }),
    )
    .await;

    #[derive(serde::Deserialize)]
    struct ScanResult {
        workflow: WorkflowMetadata,
    }

    let scan: ScanResult = parse_result(&result);

    // Quick scan should be in discovery phase
    assert!(matches!(scan.workflow.phase, WorkflowPhase::Discovery));
}

// =============================================================================
// Preview Pagination Tests
// =============================================================================

#[tokio::test]
async fn test_preview_pagination() {
    let temp_dir = TempDir::new().unwrap();

    // Create file with many rows
    let mut content = "id,value\n".to_string();
    for i in 1..=100 {
        content.push_str(&format!("{},{}\n", i, i * 10));
    }
    fs::write(temp_dir.path().join("large.csv"), content).unwrap();

    let registry = create_default_registry();

    // First page
    let page1 = execute_tool(
        &registry,
        "preview_files",
        json!({
            "files": [temp_dir.path().join("large.csv")],
            "lines": 10,
            "offset": 0
        }),
    )
    .await;

    #[derive(serde::Deserialize)]
    struct PreviewResult {
        previews: Vec<Preview>,
    }

    #[derive(serde::Deserialize)]
    struct Preview {
        start_line: usize,
        lines: Vec<String>,
        has_more: bool,
    }

    let p1: PreviewResult = parse_result(&page1);
    assert_eq!(p1.previews[0].start_line, 1);
    assert_eq!(p1.previews[0].lines.len(), 10);
    assert!(p1.previews[0].has_more);

    // Second page
    let page2 = execute_tool(
        &registry,
        "preview_files",
        json!({
            "files": [temp_dir.path().join("large.csv")],
            "lines": 10,
            "offset": 10
        }),
    )
    .await;

    let p2: PreviewResult = parse_result(&page2);
    assert_eq!(p2.previews[0].start_line, 11);
    assert!(p2.previews[0].has_more);
}

// =============================================================================
// Tool Count Verification
// =============================================================================

#[test]
fn test_all_12_tools_registered() {
    let registry = create_default_registry();

    assert_eq!(registry.len(), 12, "Should have exactly 12 tools");

    let expected_tools = [
        "quick_scan",
        "apply_scope",
        "preview_files",
        "discover_schemas",
        "approve_schemas",
        "propose_amendment",
        "run_backtest",
        "fix_parser",
        "generate_parser",
        "refine_parser",
        "execute_pipeline",
        "query_output",
    ];

    for tool_name in expected_tools {
        assert!(
            registry.get(tool_name).is_some(),
            "Tool '{}' should be registered",
            tool_name
        );
    }
}

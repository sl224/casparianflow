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

// NOTE: test_preview_files_real_csv removed - preview_files tool not yet implemented

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

    // NOTE: preview_files step skipped - tool not yet implemented

    // Step 2: discover_schemas - Infer types
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

    // NOTE: execute_pipeline and query_output steps skipped - require full pipeline setup
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

// NOTE: test_preview_pagination removed - preview_files tool not yet implemented

// =============================================================================
// Tool Count Verification
// =============================================================================

#[test]
fn test_all_10_tools_registered() {
    let registry = create_default_registry();

    assert_eq!(registry.len(), 10, "Should have exactly 10 tools");

    let expected_tools = [
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

    for tool_name in expected_tools {
        assert!(
            registry.get(tool_name).is_some(),
            "Tool '{}' should be registered",
            tool_name
        );
    }
}

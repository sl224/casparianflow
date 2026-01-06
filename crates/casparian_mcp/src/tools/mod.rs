//! MCP Tool implementations
//!
//! This module provides the ToolRegistry for managing available tools
//! and implementations for all MCP tools:
//!
//! - **Discovery**: quick_scan, apply_scope
//! - **Schema**: discover_schemas, approve_schemas, propose_amendment
//! - **Backtest**: run_backtest, fix_parser
//! - **Codegen**: refine_parser
//! - **Execution**: execute_pipeline, query_output

pub mod backtest;
pub mod codegen;
pub mod discovery;
pub mod execution;
pub mod schema;

use std::collections::HashMap;
use std::sync::Arc;

use crate::types::Tool;

/// Registry of available tools
///
/// Tools are registered by name and can be looked up for execution.
pub struct ToolRegistry {
    /// Map of tool name to tool implementation
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    ///
    /// If a tool with the same name already exists, it will be replaced.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a tool (Arc wrapped)
    ///
    /// Useful when the tool is already wrapped in Arc.
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Check if a tool is registered
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// List all registered tools
    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Get the number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Remove a tool by name
    pub fn remove(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.remove(name)
    }

    /// Clear all registered tools
    pub fn clear(&mut self) {
        self.tools.clear();
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register all available MCP tools with the registry
///
/// This function registers all 10 tools:
/// - Discovery: quick_scan, apply_scope
/// - Schema: discover_schemas, approve_schemas, propose_amendment
/// - Backtest: run_backtest, fix_parser
/// - Codegen: refine_parser
/// - Execution: execute_pipeline, query_output
pub fn register_all_tools(registry: &mut ToolRegistry) {
    // Discovery tools
    registry.register(discovery::QuickScanTool::new());
    registry.register(discovery::ApplyScopeTool::new());

    // Schema tools
    registry.register(schema::DiscoverSchemasTool::new());
    registry.register(schema::ApproveSchemasTool::new());
    registry.register(schema::ProposeAmendmentTool::new());

    // Backtest tools
    registry.register(backtest::RunBacktestTool::new());
    registry.register(backtest::FixParserTool::new());

    // Codegen tools
    registry.register(codegen::RefineParserTool::new());

    // Execution tools
    registry.register(execution::ExecutePipelineTool::new());
    registry.register(execution::QueryOutputTool::new());
}

/// Create a new registry with all tools pre-registered
pub fn create_default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_all_tools(&mut registry);
    registry
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ToolError, ToolInputSchema, ToolResult};
    use async_trait::async_trait;
    use serde_json::Value;

    struct DummyTool {
        name: String,
    }

    impl DummyTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "A dummy tool for testing"
        }

        fn input_schema(&self) -> ToolInputSchema {
            ToolInputSchema::new()
        }

        async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
            Ok(ToolResult::text(format!("Executed {}", self.name)))
        }
    }

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool::new("test_tool"));

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("test_tool"));

        let tool = registry.get("test_tool");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "test_tool");
    }

    #[test]
    fn test_register_replaces() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool::new("tool"));
        registry.register(DummyTool::new("tool"));

        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool::new("tool1"));
        registry.register(DummyTool::new("tool2"));
        registry.register(DummyTool::new("tool3"));

        let tools = registry.list();
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_remove_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool::new("tool"));

        let removed = registry.remove("tool");
        assert!(removed.is_some());
        assert!(registry.is_empty());

        let not_found = registry.remove("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_clear_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool::new("tool1"));
        registry.register(DummyTool::new("tool2"));

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_arc() {
        let mut registry = ToolRegistry::new();
        let tool: Arc<dyn Tool> = Arc::new(DummyTool::new("arc_tool"));
        registry.register_arc(tool);

        assert!(registry.contains("arc_tool"));
    }

    #[test]
    fn test_register_all_tools() {
        let registry = create_default_registry();

        // Should have all 10 tools
        assert_eq!(registry.len(), 10);

        // Check each tool is registered
        assert!(registry.contains("quick_scan"));
        assert!(registry.contains("apply_scope"));
        assert!(registry.contains("discover_schemas"));
        assert!(registry.contains("approve_schemas"));
        assert!(registry.contains("propose_amendment"));
        assert!(registry.contains("run_backtest"));
        assert!(registry.contains("fix_parser"));
        assert!(registry.contains("refine_parser"));
        assert!(registry.contains("execute_pipeline"));
        assert!(registry.contains("query_output"));
    }

    #[test]
    fn test_tool_descriptions_not_empty() {
        let registry = create_default_registry();

        for tool in registry.list() {
            assert!(
                !tool.description().is_empty(),
                "Tool {} has empty description",
                tool.name()
            );
        }
    }

    #[test]
    fn test_tool_schemas_valid() {
        let registry = create_default_registry();

        for tool in registry.list() {
            let schema = tool.input_schema();
            assert_eq!(
                schema.schema_type, "object",
                "Tool {} has invalid schema type",
                tool.name()
            );
        }
    }
}

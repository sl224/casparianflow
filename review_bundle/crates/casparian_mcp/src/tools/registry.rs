//! Tool Registry - Tool Discovery and Dispatch
//!
//! Maintains the list of available tools and dispatches calls by name.

use super::*;
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::protocol::ToolDefinition;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

/// Registry of available MCP tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn McpTool>>,
}

impl ToolRegistry {
    /// Create a new tool registry with all tools registered
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register all tools
        registry.register(Box::new(plugins::PluginsTool));
        registry.register(Box::new(scan::ScanTool));
        registry.register(Box::new(preview::PreviewTool));
        registry.register(Box::new(query::QueryTool));
        registry.register(Box::new(backtest::BacktestStartTool));
        registry.register(Box::new(run::RunRequestTool));
        registry.register(Box::new(job::JobStatusTool));
        registry.register(Box::new(job::JobCancelTool));
        registry.register(Box::new(job::JobListTool));
        registry.register(Box::new(approval::ApprovalStatusTool));
        registry.register(Box::new(approval::ApprovalListTool));
        registry.register(Box::new(approval::ApprovalDecideTool));

        // Intent pipeline tools (§7.1-7.9)
        // Session lifecycle
        registry.register(Box::new(intent_session::SessionCreateTool));
        registry.register(Box::new(intent_session::SessionStatusTool));
        registry.register(Box::new(intent_session::SessionListTool));

        // FileSet tools
        registry.register(Box::new(intent_fileset::FileSetSampleTool));
        registry.register(Box::new(intent_fileset::FileSetPageTool));
        registry.register(Box::new(intent_fileset::FileSetInfoTool));

        // Selection tools
        registry.register(Box::new(intent_select::SelectProposeTool));
        registry.register(Box::new(intent_select::SelectApproveTool));

        // Tag rules tools
        registry.register(Box::new(intent_tags::TagsProposeRulesTool));
        registry.register(Box::new(intent_tags::TagsApplyRulesTool));

        // Path fields tools
        registry.register(Box::new(intent_path_fields::PathFieldsProposeTool));
        registry.register(Box::new(intent_path_fields::PathFieldsApplyTool));

        // Schema intent tools
        registry.register(Box::new(intent_schema::SchemaInferIntentTool));
        registry.register(Box::new(intent_schema::SchemaResolveAmbiguityTool));

        // Backtest loop tools
        registry.register(Box::new(intent_backtest::ParserGenerateDraftTool));
        registry.register(Box::new(intent_backtest::IntentBacktestStartTool));
        registry.register(Box::new(intent_backtest::IntentBacktestStatusTool));
        registry.register(Box::new(intent_backtest::IntentBacktestReportTool));
        registry.register(Box::new(intent_backtest::PatchApplyTool));

        // Publish/run tools
        registry.register(Box::new(intent_publish::SchemaPromoteTool));
        registry.register(Box::new(intent_publish::PublishPlanTool));
        registry.register(Box::new(intent_publish::PublishExecuteTool));
        registry.register(Box::new(intent_publish::RunPlanTool));
        registry.register(Box::new(intent_publish::RunExecuteTool));

        debug!("Registered {} tools", registry.tools.len());

        registry
    }

    /// Register a tool
    fn register(&mut self, tool: Box<dyn McpTool>) {
        let name = tool.name().to_string();
        debug!("Registering tool: {}", name);
        self.tools.insert(name, tool);
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Call a tool by name (synchronous)
    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        security: &SecurityConfig,
        core: &CoreHandle,
        config: &McpServerConfig,
        executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow!("Unknown tool: {}", name))?;

        tool.execute(args, security, core, config, executor)
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&dyn McpTool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Check if a tool exists
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_core_tools() {
        let registry = ToolRegistry::new();

        assert!(registry.has_tool("casparian_plugins"));
        assert!(registry.has_tool("casparian_scan"));
        assert!(registry.has_tool("casparian_preview"));
        assert!(registry.has_tool("casparian_query"));
        assert!(registry.has_tool("casparian_backtest_start"));
        assert!(registry.has_tool("casparian_run_request"));
        assert!(registry.has_tool("casparian_job_status"));
        assert!(registry.has_tool("casparian_job_cancel"));
        assert!(registry.has_tool("casparian_job_list"));
        assert!(registry.has_tool("casparian_approval_status"));
        assert!(registry.has_tool("casparian_approval_list"));
        assert!(registry.has_tool("casparian_approval_decide"));
    }

    #[test]
    fn test_list_tools() {
        let registry = ToolRegistry::new();
        let tools = registry.list_tools();

        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "casparian_scan"));
    }
}

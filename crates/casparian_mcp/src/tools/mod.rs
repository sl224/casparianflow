//! MCP Tool Implementations
//!
//! Each tool exposes a specific Casparian capability via the MCP protocol.
//! Tools are registered in the ToolRegistry and dispatched by name.
//!
//! # Tool Categories
//!
//! - **Discovery**: scan, plugins
//! - **Preview**: preview (read-only)
//! - **Jobs**: backtest_start, run_request, job_*
//! - **Query**: query (read-only sandbox)
//! - **Approvals**: approval_status, approval_list
//!
//! # Human Gates
//!
//! Some tools require human approval before execution:
//! - `run_request`: Creates approval request, human must approve
//! - `schema_promote`: Creates approval request, human must approve

mod registry;

// Tool implementations
mod approval;
mod backtest;
mod job;
mod plugins;
mod preview;
mod query;
mod run;
mod scan;

// Intent pipeline tools (§7.1-7.9)
mod intent_backtest;
mod intent_fileset;
mod intent_path_fields;
mod intent_publish;
mod intent_schema;
mod intent_select;
mod intent_session;
mod intent_tags;

pub use registry::ToolRegistry;

use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::protocol::ToolDefinition;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::Result;
use serde_json::Value;

/// Trait for MCP tools
///
/// All tool execution is synchronous. Tools receive a `CoreHandle` for
/// state operations via message passing to the Core thread.
pub trait McpTool: Send + Sync {
    /// Tool name (e.g., "casparian_scan")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// JSON Schema for input parameters
    fn input_schema(&self) -> Value;

    /// Execute the tool (synchronous)
    ///
    /// Tools receive a `CoreHandle` for all state operations (jobs, approvals).
    /// State changes flow through the Core via message passing.
    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        core: &CoreHandle,
        config: &McpServerConfig,
        executor: &JobExecutorHandle,
    ) -> Result<Value>;

    /// Get the tool definition for tools/list
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.input_schema(),
        }
    }
}

//! MCP Server CLI launcher
//!
//! Launches the Model Context Protocol server for Claude Code integration.
//! The server communicates via stdio using JSON-RPC.

use anyhow::Result;
use casparian_mcp::{create_default_registry, McpServer};

/// Arguments for the MCP server command
pub struct McpArgs {
    /// Bind address (currently unused, MCP uses stdio)
    #[allow(dead_code)]
    pub addr: Option<String>,
}

/// Run the MCP server
///
/// Starts the MCP server with all 9 tools registered:
/// - Discovery: quick_scan, apply_scope
/// - Schema: discover_schemas, approve_schemas, propose_amendment
/// - Backtest: run_backtest, fix_parser
/// - Execution: execute_pipeline, query_output
pub async fn run(_args: McpArgs) -> Result<()> {
    // Create a registry with all tools pre-registered
    let registry = create_default_registry();

    // Create server with the registry
    let mut server = McpServer::with_registry(registry);

    tracing::info!(
        "MCP Server starting via stdio ({} tools registered)",
        server.registry().len()
    );

    // Run the server (blocks until stdin closes)
    server.run().await.map_err(|e| anyhow::anyhow!("{}", e))
}

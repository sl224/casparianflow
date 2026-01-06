//! MCP Server implementation
//!
//! Main server that handles MCP protocol and dispatches to registered tools.

use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::protocol::{
    ContentBlock, InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, McpProtocol, RequestId, ServerCapabilities, ServerInfo, ToolDefinition,
    ToolsCallParams, ToolsCallResult, ToolsCapability, ToolsListResult,
};
use crate::tools::ToolRegistry;
use crate::types::{Tool, ToolContent, ToolError};

/// MCP Server version
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP Server name
pub const SERVER_NAME: &str = "casparian-mcp";

/// Supported protocol version
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP Server
///
/// Handles MCP protocol communication and dispatches tool calls
/// to registered tool implementations.
pub struct McpServer {
    /// Protocol handler for JSON-RPC over stdio
    protocol: McpProtocol,

    /// Registry of available tools
    tool_registry: ToolRegistry,

    /// Whether the server has been initialized
    initialized: bool,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new() -> Self {
        Self {
            protocol: McpProtocol::new(),
            tool_registry: ToolRegistry::new(),
            initialized: false,
        }
    }

    /// Create a new MCP server with a pre-configured tool registry
    pub fn with_registry(tool_registry: ToolRegistry) -> Self {
        Self {
            protocol: McpProtocol::new(),
            tool_registry,
            initialized: false,
        }
    }

    /// Register a tool with the server
    pub fn register_tool<T: Tool + 'static>(&mut self, tool: T) {
        self.tool_registry.register(tool);
    }

    /// Register a tool (Arc wrapped) with the server
    pub fn register_tool_arc(&mut self, tool: Arc<dyn Tool>) {
        self.tool_registry.register_arc(tool);
    }

    /// Get the tool registry (for inspection)
    pub fn registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    /// Run the server, processing requests until stdin closes
    pub async fn run(&mut self) -> Result<(), ToolError> {
        info!("Starting {} v{}", SERVER_NAME, SERVER_VERSION);

        loop {
            match self.protocol.read_request().await? {
                Some(request) => {
                    self.handle_request(request).await?;
                }
                None => {
                    info!("Connection closed, shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single JSON-RPC request
    async fn handle_request(&mut self, request: JsonRpcRequest) -> Result<(), ToolError> {
        let id = request.id.clone().unwrap_or(RequestId::Null);
        let method = request.method.as_str();

        debug!("Handling request: method={}, id={:?}", method, id);

        // Dispatch based on method
        let response = match method {
            "initialize" => self.handle_initialize(id.clone(), request.params).await,
            "initialized" => {
                // Notification, no response needed
                debug!("Client sent initialized notification");
                return Ok(());
            }
            "shutdown" => {
                info!("Shutdown requested");
                JsonRpcResponse::success(id.clone(), Value::Null)
            }
            "tools/list" => self.handle_tools_list(id.clone()).await,
            "tools/call" => self.handle_tools_call(id.clone(), request.params).await,
            _ => {
                warn!("Unknown method: {}", method);
                JsonRpcResponse::error(id.clone(), JsonRpcError::method_not_found(method))
            }
        };

        // Send response (unless it was a notification)
        if request.id.is_some() {
            self.protocol.write_response(&response).await?;
        }

        Ok(())
    }

    /// Handle initialize request
    async fn handle_initialize(
        &mut self,
        id: RequestId,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        // Parse params
        let _init_params: InitializeParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    error!("Failed to parse initialize params: {}", e);
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(e.to_string()),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing initialize params"),
                );
            }
        };

        self.initialized = true;
        info!("Server initialized");

        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: true }),
                resources: None,
                prompts: None,
            },
            server_info: ServerInfo {
                name: SERVER_NAME.to_string(),
                version: SERVER_VERSION.to_string(),
            },
        };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(id, v),
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string())),
        }
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, id: RequestId) -> JsonRpcResponse {
        let tools: Vec<ToolDefinition> = self
            .tool_registry
            .list()
            .iter()
            .map(|t| {
                let schema = t.input_schema();
                ToolDefinition {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    input_schema: serde_json::to_value(schema).unwrap_or(Value::Object(
                        serde_json::Map::new(),
                    )),
                }
            })
            .collect();

        let result = ToolsListResult { tools };

        match serde_json::to_value(result) {
            Ok(v) => JsonRpcResponse::success(id, v),
            Err(e) => JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string())),
        }
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, id: RequestId, params: Option<Value>) -> JsonRpcResponse {
        // Parse params
        let call_params: ToolsCallParams = match params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    error!("Failed to parse tools/call params: {}", e);
                    return JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params(e.to_string()),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_params("Missing tools/call params"),
                );
            }
        };

        debug!("Calling tool: {}", call_params.name);

        // Find and execute tool
        match self.tool_registry.get(&call_params.name) {
            Some(tool) => {
                match tool.execute(call_params.arguments).await {
                    Ok(result) => {
                        // Convert ToolResult to ToolsCallResult
                        let content: Vec<ContentBlock> = result
                            .content
                            .into_iter()
                            .map(|c| match c {
                                ToolContent::Text { text } => ContentBlock::Text { text },
                                ToolContent::Image { data, mime_type } => {
                                    ContentBlock::Image { data, mime_type }
                                }
                                ToolContent::Resource { uri, mime_type } => {
                                    ContentBlock::Resource { uri, mime_type }
                                }
                            })
                            .collect();

                        let call_result = ToolsCallResult {
                            content,
                            is_error: result.is_error,
                        };

                        match serde_json::to_value(call_result) {
                            Ok(v) => JsonRpcResponse::success(id, v),
                            Err(e) => {
                                JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string()))
                            }
                        }
                    }
                    Err(e) => {
                        error!("Tool execution failed: {}", e);
                        // Return error as tool result, not JSON-RPC error
                        let call_result = ToolsCallResult {
                            content: vec![ContentBlock::Text {
                                text: e.to_string(),
                            }],
                            is_error: true,
                        };
                        match serde_json::to_value(call_result) {
                            Ok(v) => JsonRpcResponse::success(id, v),
                            Err(e) => {
                                JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string()))
                            }
                        }
                    }
                }
            }
            None => {
                warn!("Tool not found: {}", call_params.name);
                let call_result = ToolsCallResult {
                    content: vec![ContentBlock::Text {
                        text: format!("Tool not found: {}", call_params.name),
                    }],
                    is_error: true,
                };
                match serde_json::to_value(call_result) {
                    Ok(v) => JsonRpcResponse::success(id, v),
                    Err(e) => {
                        JsonRpcResponse::error(id, JsonRpcError::internal_error(e.to_string()))
                    }
                }
            }
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolInputSchema;
    use async_trait::async_trait;

    struct TestTool;

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn input_schema(&self) -> ToolInputSchema {
            ToolInputSchema::new()
        }

        async fn execute(&self, _args: Value) -> Result<crate::types::ToolResult, ToolError> {
            Ok(crate::types::ToolResult::text("test result"))
        }
    }

    #[test]
    fn test_server_creation() {
        let server = McpServer::new();
        assert!(!server.initialized);
        assert_eq!(server.registry().list().len(), 0);
    }

    #[test]
    fn test_register_tool() {
        let mut server = McpServer::new();
        server.register_tool(TestTool);
        assert_eq!(server.registry().list().len(), 1);
    }

    #[test]
    fn test_server_constants() {
        assert_eq!(SERVER_NAME, "casparian-mcp");
        assert_eq!(PROTOCOL_VERSION, "2024-11-05");
    }
}

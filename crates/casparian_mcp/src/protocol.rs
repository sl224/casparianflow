//! MCP Protocol implementation
//!
//! Handles JSON-RPC 2.0 over stdio for MCP communication.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};
use tracing::{debug, error, trace};

use crate::ToolError;

// =============================================================================
// JSON-RPC Types
// =============================================================================

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Request ID (null for notifications)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,

    /// Method name
    pub method: String,

    /// Method parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Request ID this response corresponds to
    pub id: RequestId,

    /// Result (present on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Error (present on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a success response
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: RequestId, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,

    /// Error message
    pub message: String,

    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    /// Parse error (-32700)
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: message.into(),
            data: None,
        }
    }

    /// Invalid request (-32600)
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
            data: None,
        }
    }

    /// Method not found (-32601)
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {}", method.into()),
            data: None,
        }
    }

    /// Invalid params (-32602)
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    /// Internal error (-32603)
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }

    /// Create from ToolError
    pub fn from_tool_error(err: &ToolError) -> Self {
        Self {
            code: err.error_code(),
            message: err.to_string(),
            data: None,
        }
    }
}

/// Request ID (can be string, number, or null)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
    Null,
}

impl Default for RequestId {
    fn default() -> Self {
        RequestId::Null
    }
}

// =============================================================================
// MCP-Specific Types
// =============================================================================

/// MCP Initialize request params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Protocol version
    pub protocol_version: String,

    /// Client capabilities
    pub capabilities: ClientCapabilities,

    /// Client info
    pub client_info: ClientInfo,
}

/// Client capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Roots capability (file system access)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,

    /// Sampling capability (LLM access)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

/// Roots capability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    pub list_changed: bool,
}

/// Client info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// MCP Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Protocol version
    pub protocol_version: String,

    /// Server capabilities
    pub capabilities: ServerCapabilities,

    /// Server info
    pub server_info: ServerInfo,
}

/// Server capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tools capability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,

    /// Resources capability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,

    /// Prompts capability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Value>,
}

/// Tools capability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    pub list_changed: bool,
}

/// Server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Tool definition for tools/list response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// tools/list result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<ToolDefinition>,
}

/// tools/call params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallParams {
    pub name: String,

    #[serde(default)]
    pub arguments: Value,
}

/// tools/call result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCallResult {
    pub content: Vec<ContentBlock>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// Content block in tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { uri: String, mime_type: Option<String> },
}

// =============================================================================
// Protocol Handler
// =============================================================================

/// MCP Protocol handler for JSON-RPC over stdio
pub struct McpProtocol {
    /// Buffered stdin reader
    stdin: BufReader<Stdin>,

    /// Stdout writer
    stdout: Stdout,
}

impl McpProtocol {
    /// Create a new protocol handler
    pub fn new() -> Self {
        Self {
            stdin: BufReader::new(tokio::io::stdin()),
            stdout: tokio::io::stdout(),
        }
    }

    /// Read a JSON-RPC request from stdin
    ///
    /// Returns None if stdin is closed (EOF)
    pub async fn read_request(&mut self) -> Result<Option<JsonRpcRequest>, ToolError> {
        loop {
            let mut line = String::new();

            match self.stdin.read_line(&mut line).await {
                Ok(0) => {
                    // EOF
                    debug!("stdin closed (EOF)");
                    return Ok(None);
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        // Empty line, continue reading
                        continue;
                    }

                    trace!("Received: {}", trimmed);

                    let request: JsonRpcRequest = serde_json::from_str(trimmed).map_err(|e| {
                        error!("Failed to parse request: {}", e);
                        ToolError::Serialization(e)
                    })?;

                    debug!("Parsed request: method={}", request.method);
                    return Ok(Some(request));
                }
                Err(e) => {
                    error!("Failed to read from stdin: {}", e);
                    return Err(ToolError::Io(e));
                }
            }
        }
    }

    /// Write a JSON-RPC response to stdout
    pub async fn write_response(&mut self, response: &JsonRpcResponse) -> Result<(), ToolError> {
        let json = serde_json::to_string(response)?;
        trace!("Sending: {}", json);

        self.stdout
            .write_all(json.as_bytes())
            .await
            .map_err(ToolError::Io)?;
        self.stdout
            .write_all(b"\n")
            .await
            .map_err(ToolError::Io)?;
        self.stdout.flush().await.map_err(ToolError::Io)?;

        debug!("Sent response for id={:?}", response.id);
        Ok(())
    }

    /// Write a notification (no response expected)
    pub async fn write_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), ToolError> {
        let notification = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&notification)?;
        trace!("Sending notification: {}", json);

        self.stdout
            .write_all(json.as_bytes())
            .await
            .map_err(ToolError::Io)?;
        self.stdout
            .write_all(b"\n")
            .await
            .map_err(ToolError::Io)?;
        self.stdout.flush().await.map_err(ToolError::Io)?;

        debug!("Sent notification: {}", method);
        Ok(())
    }
}

impl Default for McpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Synchronous Protocol Handler (for testing)
// =============================================================================

/// Synchronous protocol handler for testing
pub struct SyncProtocol<R: BufRead, W: Write> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> SyncProtocol<R, W> {
    /// Create a new sync protocol handler
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Read a JSON-RPC request
    pub fn read_request(&mut self) -> Result<Option<JsonRpcRequest>, ToolError> {
        loop {
            let mut line = String::new();

            match self.reader.read_line(&mut line) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let request: JsonRpcRequest = serde_json::from_str(trimmed)?;
                    return Ok(Some(request));
                }
                Err(e) => return Err(ToolError::Io(e)),
            }
        }
    }

    /// Write a JSON-RPC response
    pub fn write_response(&mut self, response: &JsonRpcResponse) -> Result<(), ToolError> {
        let json = serde_json::to_string(response)?;
        writeln!(self.writer, "{}", json).map_err(ToolError::Io)?;
        self.writer.flush().map_err(ToolError::Io)?;
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "tools/list".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
    }

    #[test]
    fn test_response_success() {
        let response = JsonRpcResponse::success(
            RequestId::Number(1),
            serde_json::json!({"tools": []}),
        );

        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[test]
    fn test_response_error() {
        let response = JsonRpcResponse::error(
            RequestId::Number(1),
            JsonRpcError::method_not_found("unknown"),
        );

        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn test_sync_protocol_roundtrip() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(42)),
            method: "test/method".to_string(),
            params: Some(serde_json::json!({"arg": "value"})),
        };

        let input = format!("{}\n", serde_json::to_string(&request).unwrap());
        let reader = Cursor::new(input.as_bytes().to_vec());
        let mut output = Vec::new();

        let mut protocol = SyncProtocol::new(reader, &mut output);
        let parsed = protocol.read_request().unwrap().unwrap();

        assert_eq!(parsed.method, "test/method");
        assert_eq!(parsed.id, Some(RequestId::Number(42)));
    }

    #[test]
    fn test_initialize_params_deserialize() {
        let json = r#"{
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": {"listChanged": true}
            },
            "clientInfo": {
                "name": "claude-code",
                "version": "1.0.0"
            }
        }"#;

        let params: InitializeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.protocol_version, "2024-11-05");
        assert_eq!(params.client_info.name, "claude-code");
    }

    #[test]
    fn test_tools_call_params_deserialize() {
        let json = r#"{
            "name": "create_scope",
            "arguments": {"name": "test-scope"}
        }"#;

        let params: ToolsCallParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.name, "create_scope");
        assert_eq!(params.arguments["name"], "test-scope");
    }
}

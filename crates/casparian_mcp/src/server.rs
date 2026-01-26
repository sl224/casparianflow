//! MCP Server Implementation
//!
//! JSON-RPC 2.0 server over stdio for the Model Context Protocol.
//!
//! # Architecture
//!
//! The server runs in a single process, reading JSON-RPC requests from stdin
//! and writing responses to stdout. Long-running operations return immediately
//! with a job_id; progress is polled via separate tool calls.
//!
//! # Example
//!
//! ```ignore
//! let config = McpServerConfig::default();
//! let mut server = McpServer::new(config)?;
//! server.run()?; // Blocking, no async runtime required
//! ```

use crate::core::{spawn_core_with_config, CoreConfig, CoreHandle, Event};
use crate::jobs::{JobExecutor, JobExecutorHandle};
use crate::protocol::{
    methods, ContentBlock, InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, ServerCapabilities, ServerInfo, ToolCallParams, ToolCallResult,
    ToolsCapability, ToolsListResult, JSONRPC_VERSION, MCP_PROTOCOL_VERSION,
};
use crate::security::{AuditLog, OutputBudget, PathAllowlist, SecurityConfig};
use crate::tools::ToolRegistry;
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use tracing::{debug, error, info, warn};

/// MCP Server configuration
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Server name (reported in initialize)
    pub server_name: String,

    /// Server version (reported in initialize)
    pub server_version: String,

    /// Allowed paths for file operations
    pub allowed_paths: Vec<PathBuf>,

    /// Maximum response size in bytes
    pub max_response_bytes: usize,

    /// Maximum rows returned from queries
    pub max_rows: usize,

    /// Path to audit log file
    pub audit_log_path: Option<PathBuf>,

    /// Database path
    pub db_path: PathBuf,

    /// Query catalog path (DuckDB)
    pub query_catalog_path: PathBuf,

    /// Control API address (preferred backend for mutations)
    pub control_addr: Option<String>,

    /// Allow standalone DB writer mode (no Control API)
    pub standalone_db_writer: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let casparian_dir = home.join(".casparian_flow");

        Self {
            server_name: "casparian-mcp".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            allowed_paths: vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
            max_response_bytes: 1024 * 1024, // 1MB
            max_rows: 10_000,
            audit_log_path: Some(casparian_dir.join("mcp_audit.ndjson")),
            db_path: casparian_dir.join("state.sqlite"),
            query_catalog_path: casparian_dir.join("query.duckdb"),
            control_addr: Some(casparian_sentinel::DEFAULT_CONTROL_ADDR.to_string()),
            standalone_db_writer: false,
        }
    }
}

/// MCP Server
///
/// # Lock-Free Architecture (Phase 4)
///
/// The server no longer holds any `Arc<Mutex<>>` for job/approval management.
/// All state is owned by the Core thread and accessed via message passing (CoreHandle).
/// The executor uses CoreHandle for all job state operations.
pub struct McpServer {
    config: McpServerConfig,
    security: SecurityConfig,
    /// Handle to the Core thread for all state operations
    core: CoreHandle,
    /// Event receiver for state change notifications (not used yet, for future TUI)
    #[allow(dead_code)]
    events: Receiver<Event>,
    /// Core thread handle (joined on drop)
    #[allow(dead_code)]
    core_thread: JoinHandle<()>,
    tools: ToolRegistry,
    executor_handle: JobExecutorHandle,
    /// Executor thread handle (joined on drop)
    #[allow(dead_code)]
    executor_thread: JoinHandle<()>,
    initialized: bool,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new(config: McpServerConfig) -> Result<Self> {
        // Initialize security subsystem
        let path_allowlist = PathAllowlist::new(config.allowed_paths.clone());
        let output_budget = OutputBudget::new(config.max_response_bytes, config.max_rows);
        let audit_log = config
            .audit_log_path
            .as_ref()
            .map(|p| AuditLog::new(p.clone()))
            .transpose()?;

        let security = SecurityConfig {
            path_allowlist,
            output_budget,
            audit_log,
        };

        // Spawn the Core thread (owns JobManager, ApprovalManager - single owner, no locks)
        let core_config = CoreConfig {
            db_path: config.db_path.clone(),
            control_addr: config.control_addr.clone(),
            standalone_db_writer: config.standalone_db_writer,
        };
        let (core, events, core_thread) = spawn_core_with_config(core_config)?;

        // Initialize job executor (sync, runs in dedicated thread)
        // Uses CoreHandle for all job state operations via message passing (no locks)
        let (executor, executor_handle) = JobExecutor::new(core.clone());

        // Spawn executor loop in dedicated thread (no tokio)
        let executor_thread = executor.spawn();

        // Initialize tool registry
        let tools = ToolRegistry::new();

        Ok(Self {
            config,
            security,
            core,
            events,
            core_thread,
            tools,
            executor_handle,
            executor_thread,
            initialized: false,
        })
    }

    /// Run the server (blocking, reads from stdin, writes to stdout)
    ///
    /// This is a synchronous blocking loop - no async runtime required.
    pub fn run(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        info!("MCP server starting");

        for line in stdin.lock().lines() {
            let line = line.context("Failed to read from stdin")?;

            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            // Parse request
            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let response = JsonRpcResponse::error(
                        None,
                        JsonRpcError::new(
                            crate::protocol::ErrorCode::ParseError,
                            format!("Invalid JSON: {}", e),
                        ),
                    );
                    self.write_response(&mut stdout, &response)?;
                    continue;
                }
            };

            // Log to audit
            if let Some(ref mut audit) = self.security.audit_log {
                audit.log_request(&request)?;
            }

            // Handle request (synchronous)
            let response = self.handle_request(request);

            // Skip response for notifications (no id, no result, no error)
            if response.id.is_none() && response.result.is_none() && response.error.is_none() {
                continue;
            }

            // Log response to audit
            if let Some(ref mut audit) = self.security.audit_log {
                audit.log_response(&response)?;
            }

            // Write response
            self.write_response(&mut stdout, &response)?;
        }

        info!("MCP server shutting down");
        Ok(())
    }

    /// Handle a single JSON-RPC request (synchronous)
    fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        // Validate JSON-RPC version
        if request.jsonrpc != JSONRPC_VERSION {
            return JsonRpcResponse::error(
                request.id,
                JsonRpcError::new(
                    crate::protocol::ErrorCode::InvalidRequest,
                    format!("Invalid JSON-RPC version: {}", request.jsonrpc),
                ),
            );
        }

        match request.method.as_str() {
            methods::INITIALIZE => self.handle_initialize(request),
            methods::INITIALIZED => {
                // JSON-RPC notifications (no id) should not receive a response
                // If this is a notification (id is None), we skip writing a response
                // by returning early from the handler
                if request.id.is_none() {
                    // Return a dummy response that will be skipped in write path
                    return JsonRpcResponse {
                        jsonrpc: JSONRPC_VERSION.to_string(),
                        id: None,
                        result: None,
                        error: None,
                    };
                }
                // If it has an id (unusual but valid), respond with empty object
                JsonRpcResponse::success(request.id, Value::Null)
            }
            methods::TOOLS_LIST => self.handle_tools_list(request),
            methods::TOOLS_CALL => self.handle_tools_call(request),
            methods::PING => {
                JsonRpcResponse::success(request.id, Value::Object(Default::default()))
            }
            _ => JsonRpcResponse::error(
                request.id,
                JsonRpcError::new(
                    crate::protocol::ErrorCode::MethodNotFound,
                    format!("Unknown method: {}", request.method),
                ),
            ),
        }
    }

    /// Handle initialize request
    fn handle_initialize(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: InitializeParams = match request.params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    return JsonRpcResponse::error(
                        request.id,
                        JsonRpcError::new(
                            crate::protocol::ErrorCode::InvalidParams,
                            format!("Invalid initialize params: {}", e),
                        ),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    request.id,
                    JsonRpcError::new(
                        crate::protocol::ErrorCode::InvalidParams,
                        "Missing initialize params",
                    ),
                );
            }
        };

        info!(
            "Initialize from {} v{} (protocol {})",
            params.client_info.name, params.client_info.version, params.protocol_version
        );

        self.initialized = true;

        let result = InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: false,
                }),
                resources: None,
                prompts: None,
                logging: None,
            },
            server_info: ServerInfo {
                name: self.config.server_name.clone(),
                version: self.config.server_version.clone(),
            },
        };

        JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle tools/list request
    fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tools = self.tools.list_tools();

        let result = ToolsListResult { tools };

        JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle tools/call request
    fn handle_tools_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        if !self.initialized {
            return JsonRpcResponse::error(
                request.id,
                JsonRpcError::new(
                    crate::protocol::ErrorCode::ServerError(-32002),
                    "Server not initialized",
                ),
            );
        }

        let params: ToolCallParams = match request.params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    return JsonRpcResponse::error(
                        request.id,
                        JsonRpcError::new(
                            crate::protocol::ErrorCode::InvalidParams,
                            format!("Invalid tool call params: {}", e),
                        ),
                    );
                }
            },
            None => {
                return JsonRpcResponse::error(
                    request.id,
                    JsonRpcError::new(
                        crate::protocol::ErrorCode::InvalidParams,
                        "Missing tool call params",
                    ),
                );
            }
        };

        info!("Tool call: {}", params.name);

        // Execute the tool (synchronous)
        let result = self.tools.call_tool(
            &params.name,
            params.arguments,
            &self.security,
            &self.core,
            &self.config,
            &self.executor_handle,
        );

        match result {
            Ok(value) => {
                // Check output budget - return structured JSON with truncation flag
                let json = match serde_json::to_string(&value) {
                    Ok(j) => j,
                    Err(e) => {
                        error!("Failed to serialize tool result: {}", e);
                        let tool_result = ToolCallResult {
                            content: vec![ContentBlock::text(format!(
                                "{{\"error\": \"Serialization failed: {}\"}}",
                                e
                            ))],
                            is_error: true,
                        };
                        let tool_value = match serde_json::to_value(tool_result) {
                            Ok(value) => value,
                            Err(e) => {
                                error!("Failed to serialize tool error response: {}", e);
                                return JsonRpcResponse::error(
                                    request.id,
                                    JsonRpcError::new(
                                        crate::protocol::ErrorCode::InternalError,
                                        "Failed to serialize tool error response",
                                    ),
                                );
                            }
                        };
                        return JsonRpcResponse::success(request.id, tool_value);
                    }
                };

                let (content, was_truncated) = if json.len() > self.config.max_response_bytes {
                    warn!(
                        "Response truncated from {} to {} bytes",
                        json.len(),
                        self.config.max_response_bytes
                    );
                    // Create a valid JSON response that indicates truncation
                    // instead of breaking the JSON by cutting mid-string
                    let truncated_response = serde_json::json!({
                        "truncated": true,
                        "max_bytes": self.config.max_response_bytes,
                        "original_bytes": json.len(),
                        "message": "Response exceeded size limit. Use pagination or filters to reduce output.",
                        "partial_data": null
                    });
                    (serde_json::to_string(&truncated_response).unwrap_or_else(|_|
                        r#"{"truncated":true,"error":"Failed to create truncation response"}"#.to_string()
                    ), true)
                } else {
                    (json, false)
                };

                let tool_result = ToolCallResult {
                    content: vec![ContentBlock::text(content)],
                    is_error: was_truncated, // Mark truncated responses as errors so agent knows to paginate
                };

                let tool_value = match serde_json::to_value(tool_result) {
                    Ok(value) => value,
                    Err(e) => {
                        error!("Failed to serialize tool response: {}", e);
                        return JsonRpcResponse::error(
                            request.id,
                            JsonRpcError::new(
                                crate::protocol::ErrorCode::InternalError,
                                "Failed to serialize tool response",
                            ),
                        );
                    }
                };

                JsonRpcResponse::success(request.id, tool_value)
            }
            Err(e) => {
                error!("Tool error: {}", e);
                let tool_result = ToolCallResult {
                    content: vec![ContentBlock::text(format!("Error: {}", e))],
                    is_error: true,
                };
                let tool_value = match serde_json::to_value(tool_result) {
                    Ok(value) => value,
                    Err(e) => {
                        error!("Failed to serialize tool error response: {}", e);
                        return JsonRpcResponse::error(
                            request.id,
                            JsonRpcError::new(
                                crate::protocol::ErrorCode::InternalError,
                                "Failed to serialize tool error response",
                            ),
                        );
                    }
                };

                JsonRpcResponse::success(request.id, tool_value)
            }
        }
    }

    /// Write a response to stdout
    fn write_response<W: Write>(&self, writer: &mut W, response: &JsonRpcResponse) -> Result<()> {
        let json = serde_json::to_string(response)?;
        debug!("Sending: {}", json);
        writeln!(writer, "{}", json)?;
        writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = McpServerConfig::default();
        assert_eq!(config.server_name, "casparian-mcp");
        assert_eq!(config.max_response_bytes, 1024 * 1024);
        assert_eq!(config.max_rows, 10_000);
        assert!(!config.standalone_db_writer);
    }
}

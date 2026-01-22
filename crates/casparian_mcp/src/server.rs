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
//! let server = McpServer::new(config)?;
//! server.run().await?;
//! ```

use crate::approvals::ApprovalManager;
use crate::jobs::JobManager;
use crate::protocol::{
    methods, ContentBlock, InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, RequestId, ServerCapabilities, ServerInfo, ToolCallParams, ToolCallResult,
    ToolDefinition, ToolsCapability, ToolsListResult, MCP_PROTOCOL_VERSION, JSONRPC_VERSION,
};
use crate::security::{AuditLog, OutputBudget, PathAllowlist, SecurityConfig};
use crate::tools::ToolRegistry;
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
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

    /// Directory for approval files
    pub approvals_dir: PathBuf,

    /// Directory for job state
    pub jobs_dir: PathBuf,
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
            audit_log_path: Some(casparian_dir.join("mcp_audit.log")),
            db_path: casparian_dir.join("casparian_flow.duckdb"),
            approvals_dir: casparian_dir.join("approvals"),
            jobs_dir: casparian_dir.join("mcp_jobs"),
        }
    }
}

/// MCP Server
pub struct McpServer {
    config: McpServerConfig,
    security: SecurityConfig,
    jobs: Arc<Mutex<JobManager>>,
    approvals: Arc<Mutex<ApprovalManager>>,
    tools: ToolRegistry,
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

        // Initialize job manager
        let jobs = JobManager::new(config.jobs_dir.clone())?;

        // Initialize approval manager
        let approvals = ApprovalManager::new(config.approvals_dir.clone())?;

        // Initialize tool registry
        let tools = ToolRegistry::new();

        Ok(Self {
            config,
            security,
            jobs: Arc::new(Mutex::new(jobs)),
            approvals: Arc::new(Mutex::new(approvals)),
            tools,
            initialized: false,
        })
    }

    /// Run the server (blocking, reads from stdin, writes to stdout)
    pub async fn run(&mut self) -> Result<()> {
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

            // Handle request
            let response = self.handle_request(request).await;

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

    /// Handle a single JSON-RPC request
    async fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
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
            methods::INITIALIZE => self.handle_initialize(request).await,
            methods::INITIALIZED => {
                // Notification, no response needed but we return empty for consistency
                JsonRpcResponse::success(request.id, Value::Null)
            }
            methods::TOOLS_LIST => self.handle_tools_list(request).await,
            methods::TOOLS_CALL => self.handle_tools_call(request).await,
            methods::PING => JsonRpcResponse::success(request.id, Value::Object(Default::default())),
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
    async fn handle_initialize(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
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
                tools: Some(ToolsCapability { list_changed: false }),
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
    async fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tools = self.tools.list_tools();

        let result = ToolsListResult { tools };

        JsonRpcResponse::success(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
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

        // Execute the tool
        let result = self
            .tools
            .call_tool(
                &params.name,
                params.arguments,
                &self.security,
                &self.jobs,
                &self.approvals,
                &self.config,
            )
            .await;

        match result {
            Ok(value) => {
                // Check output budget
                let json = serde_json::to_string(&value).unwrap_or_default();
                let (truncated_value, was_truncated) =
                    self.security.output_budget.enforce_size(&json);

                let content = if was_truncated {
                    warn!(
                        "Response truncated from {} to {} bytes",
                        json.len(),
                        truncated_value.len()
                    );
                    format!(
                        "{}... [TRUNCATED: response exceeded {} byte limit]",
                        truncated_value, self.config.max_response_bytes
                    )
                } else {
                    json
                };

                let tool_result = ToolCallResult {
                    content: vec![ContentBlock::text(content)],
                    is_error: false,
                };

                JsonRpcResponse::success(request.id, serde_json::to_value(tool_result).unwrap())
            }
            Err(e) => {
                error!("Tool error: {}", e);
                let tool_result = ToolCallResult {
                    content: vec![ContentBlock::text(format!("Error: {}", e))],
                    is_error: true,
                };

                JsonRpcResponse::success(request.id, serde_json::to_value(tool_result).unwrap())
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
    }
}

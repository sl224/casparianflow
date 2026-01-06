//! Core types for MCP server
//!
//! Includes wrapper types for domain IDs and the Tool trait definition.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

// =============================================================================
// Wrapper Types for Domain IDs
// =============================================================================

/// Unique identifier for a Scope (data context)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScopeId(pub Uuid);

impl ScopeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ScopeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ScopeId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a Contract (transformation definition)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractId(pub Uuid);

impl ContractId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ContractId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ContractId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a Backtest run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BacktestId(pub Uuid);

impl BacktestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for BacktestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BacktestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for BacktestId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

// =============================================================================
// Tool Error Types
// =============================================================================

/// Errors that can occur during tool execution
#[derive(Debug, Error)]
pub enum ToolError {
    /// Invalid parameters provided to the tool
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Resource not found (scope, contract, backtest, etc.)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Tool execution failed
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ToolError {
    /// Get the JSON-RPC error code for this error type
    pub fn error_code(&self) -> i32 {
        match self {
            ToolError::InvalidParams(_) => -32602, // Invalid params
            ToolError::NotFound(_) => -32001,      // Custom: not found
            ToolError::ExecutionFailed(_) => -32002, // Custom: execution failed
            ToolError::Internal(_) => -32603,      // Internal error
            ToolError::Serialization(_) => -32700, // Parse error
            ToolError::Io(_) => -32603,            // Internal error
        }
    }
}

// =============================================================================
// Tool Trait
// =============================================================================

/// JSON Schema for tool input parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    /// Schema type (always "object" for MCP tools)
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,

    /// Required property names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ToolInputSchema {
    /// Create a new schema with object type
    pub fn new() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        }
    }

    /// Create a schema with properties
    pub fn with_properties(properties: serde_json::Value, required: Vec<String>) -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
        }
    }
}

impl Default for ToolInputSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Content blocks returned by the tool
    pub content: Vec<ToolContent>,

    /// Whether this result indicates an error
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful text result
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: text.into(),
            }],
            is_error: false,
        }
    }

    /// Create a successful JSON result
    pub fn json<T: Serialize>(value: &T) -> Result<Self, ToolError> {
        let text = serde_json::to_string_pretty(value)?;
        Ok(Self {
            content: vec![ToolContent::Text { text }],
            is_error: false,
        })
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }
}

/// Content types that can be returned by tools
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    /// Text content
    Text { text: String },

    /// Image content (base64 encoded)
    Image { data: String, mime_type: String },

    /// Resource reference
    Resource { uri: String, mime_type: Option<String> },
}

/// Trait for implementing MCP tools
///
/// Each tool must provide:
/// - A unique name
/// - A description for Claude to understand when to use it
/// - An input schema defining expected parameters
/// - An async execute method that performs the tool's action
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does
    fn description(&self) -> &str;

    /// JSON Schema for input parameters
    fn input_schema(&self) -> ToolInputSchema;

    /// Execute the tool with the given arguments
    ///
    /// # Arguments
    /// * `args` - JSON object containing tool parameters
    ///
    /// # Returns
    /// * `Ok(ToolResult)` - Tool execution result
    /// * `Err(ToolError)` - Error during execution
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_id_creation() {
        let id1 = ScopeId::new();
        let id2 = ScopeId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_scope_id_serialization() {
        let id = ScopeId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ScopeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_scope_id_from_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id: ScopeId = uuid_str.parse().unwrap();
        assert_eq!(id.to_string(), uuid_str);
    }

    #[test]
    fn test_tool_result_text() {
        let result = ToolResult::text("Hello, world!");
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn test_tool_result_json() {
        #[derive(Serialize)]
        struct Data {
            value: i32,
        }
        let result = ToolResult::json(&Data { value: 42 }).unwrap();
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("Something went wrong");
        assert!(result.is_error);
    }

    #[test]
    fn test_tool_error_codes() {
        assert_eq!(ToolError::InvalidParams("".into()).error_code(), -32602);
        assert_eq!(ToolError::NotFound("".into()).error_code(), -32001);
        assert_eq!(ToolError::ExecutionFailed("".into()).error_code(), -32002);
        assert_eq!(ToolError::Internal("".into()).error_code(), -32603);
    }
}

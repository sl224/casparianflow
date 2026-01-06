//! LLM Provider Abstraction
//!
//! This module provides a trait-based abstraction for LLM providers,
//! enabling support for multiple backends (Claude, OpenAI, Ollama, etc.).
//!
//! The design prioritizes:
//! - **Streaming**: All responses stream token-by-token for responsive UI
//! - **Tool Calling**: Full support for MCP tool definitions and execution
//! - **Extensibility**: Easy to add new providers via the trait

pub mod claude;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use thiserror::Error;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during LLM operations
#[derive(Debug, Error)]
pub enum LlmError {
    /// API key not found or invalid
    #[error("API key error: {0}")]
    ApiKey(String),

    /// HTTP request failed
    #[error("HTTP error: {0}")]
    Http(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: retry after {retry_after_ms}ms")]
    RateLimit { retry_after_ms: u64 },

    /// Invalid response from provider
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Stream error
    #[error("Stream error: {0}")]
    Stream(String),

    /// Provider-specific error
    #[error("{provider} error: {message}")]
    Provider { provider: String, message: String },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for LlmError {
    fn from(e: serde_json::Error) -> Self {
        LlmError::Serialization(e.to_string())
    }
}

// =============================================================================
// Message Types
// =============================================================================

/// Role of a message participant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// User message
    User,
    /// Assistant (LLM) message
    Assistant,
    /// System prompt
    System,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::System => write!(f, "system"),
        }
    }
}

/// Content block within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },

    /// Tool use request from the assistant
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },

    /// Tool result from execution
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl ContentBlock {
    /// Create a text content block
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }

    /// Create a tool use content block
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        ContentBlock::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// Create a tool result content block
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Create a tool error result content block
    pub fn tool_error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: error.into(),
            is_error: true,
        }
    }

    /// Check if this is a text block
    pub fn is_text(&self) -> bool {
        matches!(self, ContentBlock::Text { .. })
    }

    /// Check if this is a tool use block
    pub fn is_tool_use(&self) -> bool {
        matches!(self, ContentBlock::ToolUse { .. })
    }

    /// Get text content if this is a text block
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,

    /// Content blocks in the message
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a new user message with text content
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
        }
    }

    /// Create a new assistant message with text content
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::text(text)],
        }
    }

    /// Create an assistant message with multiple content blocks
    pub fn assistant_with_content(content: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }

    /// Create a user message with tool results
    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::User,
            content: results,
        }
    }

    /// Get all text from the message
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Check if the message contains tool use requests
    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(|c| c.is_tool_use())
    }

    /// Get all tool use blocks from the message
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|c| c.is_tool_use()).collect()
    }
}

// =============================================================================
// Tool Definition Types
// =============================================================================

/// JSON Schema for tool parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Schema type (always "object")
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,

    /// Required property names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl Default for ToolSchema {
    fn default() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        }
    }
}

/// Definition of a tool that the LLM can use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique name of the tool
    pub name: String,

    /// Description of what the tool does
    pub description: String,

    /// Input parameter schema
    pub input_schema: ToolSchema,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: ToolSchema,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Create a tool definition with properties
    pub fn with_properties(
        name: impl Into<String>,
        description: impl Into<String>,
        properties: Value,
        required: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: ToolSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: if required.is_empty() {
                    None
                } else {
                    Some(required)
                },
            },
        }
    }
}

// =============================================================================
// Stream Types
// =============================================================================

/// A chunk of streamed response from the LLM
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text delta (partial text)
    Text(String),

    /// Tool call started
    ToolCallStart {
        /// Index of the tool call (for parallel calls)
        index: usize,
        /// Unique ID for this tool call
        id: String,
        /// Name of the tool being called
        name: String,
    },

    /// Tool call arguments delta (partial JSON)
    ToolCallDelta {
        /// Index of the tool call
        index: usize,
        /// Partial arguments JSON
        arguments_delta: String,
    },

    /// Tool call completed (full arguments available)
    ToolCall {
        /// Unique ID for this tool call
        id: String,
        /// Name of the tool
        name: String,
        /// Complete arguments as JSON
        arguments: Value,
    },

    /// Stream completed successfully
    Done {
        /// Stop reason (e.g., "end_turn", "tool_use")
        stop_reason: Option<String>,
    },

    /// Error occurred during streaming
    Error(String),
}

impl StreamChunk {
    /// Check if this is a done chunk
    pub fn is_done(&self) -> bool {
        matches!(self, StreamChunk::Done { .. })
    }

    /// Check if this is an error chunk
    pub fn is_error(&self) -> bool {
        matches!(self, StreamChunk::Error(_))
    }

    /// Check if this indicates a tool use stop reason
    pub fn is_tool_use_stop(&self) -> bool {
        matches!(
            self,
            StreamChunk::Done {
                stop_reason: Some(reason)
            } if reason == "tool_use"
        )
    }
}

// =============================================================================
// Provider Trait
// =============================================================================

/// Configuration for LLM requests
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Model identifier (e.g., "claude-sonnet-4-20250514")
    pub model: String,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Temperature for sampling (0.0 - 1.0)
    pub temperature: Option<f32>,

    /// System prompt
    pub system: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: None,
            system: None,
        }
    }
}

impl LlmConfig {
    /// Create config with a specific model
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set the system prompt
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// Trait for LLM providers
///
/// Implementations must be thread-safe and support async streaming.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name (e.g., "Claude", "OpenAI")
    fn name(&self) -> &str;

    /// Get the current model being used
    fn model(&self) -> &str;

    /// Check if the provider is configured and ready
    fn is_ready(&self) -> bool;

    /// Send messages and stream the response
    ///
    /// # Arguments
    /// * `messages` - Conversation history
    /// * `tools` - Available tools the LLM can use
    /// * `config` - Optional config overrides
    ///
    /// # Returns
    /// A stream of response chunks
    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError>;

    /// Simple chat without tools (convenience method)
    async fn chat(
        &self,
        messages: &[Message],
        config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        self.chat_stream(messages, &[], config).await
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Convert MCP tool schemas to LLM tool definitions
pub fn mcp_tools_to_definitions(tools: &[&dyn casparian_mcp::types::Tool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|tool| {
            let schema = tool.input_schema();
            ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: ToolSchema {
                    schema_type: schema.schema_type.clone(),
                    properties: schema.properties.clone(),
                    required: schema.required.clone(),
                },
            }
        })
        .collect()
}

/// Convert ToolRegistry tools to definitions
pub fn registry_to_definitions(
    registry: &casparian_mcp::tools::ToolRegistry,
) -> Vec<ToolDefinition> {
    registry
        .list()
        .iter()
        .map(|tool| {
            let schema = tool.input_schema();
            ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: ToolSchema {
                    schema_type: schema.schema_type.clone(),
                    properties: schema.properties.clone(),
                    required: schema.required.clone(),
                },
            }
        })
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_serialization() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text(), "Hello");
        assert!(!msg.has_tool_use());
    }

    #[test]
    fn test_message_with_tool_use() {
        let content = vec![
            ContentBlock::text("Let me help"),
            ContentBlock::tool_use("123", "quick_scan", serde_json::json!({"path": "/tmp"})),
        ];
        let msg = Message::assistant_with_content(content);

        assert!(msg.has_tool_use());
        assert_eq!(msg.tool_uses().len(), 1);
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition::with_properties(
            "test_tool",
            "A test tool",
            serde_json::json!({
                "path": {"type": "string", "description": "File path"}
            }),
            vec!["path".to_string()],
        );

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.input_schema.schema_type, "object");
        assert!(tool.input_schema.required.is_some());
    }

    #[test]
    fn test_content_block_text() {
        let block = ContentBlock::text("Hello");
        assert!(block.is_text());
        assert!(!block.is_tool_use());
        assert_eq!(block.as_text(), Some("Hello"));
    }

    #[test]
    fn test_tool_result_blocks() {
        let success = ContentBlock::tool_result("123", "Success!");
        let error = ContentBlock::tool_error("456", "Failed!");

        match success {
            ContentBlock::ToolResult { is_error, .. } => assert!(!is_error),
            _ => panic!("Expected ToolResult"),
        }

        match error {
            ContentBlock::ToolResult { is_error, .. } => assert!(is_error),
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_llm_config_builder() {
        let config = LlmConfig::with_model("claude-opus-4-20250514")
            .system("You are helpful")
            .max_tokens(8192)
            .temperature(0.7);

        assert_eq!(config.model, "claude-opus-4-20250514");
        assert_eq!(config.system, Some("You are helpful".to_string()));
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.temperature, Some(0.7));
    }

    #[test]
    fn test_stream_chunk_variants() {
        let text = StreamChunk::Text("Hello".to_string());
        assert!(!text.is_done());
        assert!(!text.is_error());

        let done = StreamChunk::Done {
            stop_reason: Some("end_turn".to_string()),
        };
        assert!(done.is_done());

        let tool_done = StreamChunk::Done {
            stop_reason: Some("tool_use".to_string()),
        };
        assert!(tool_done.is_tool_use_stop());

        let error = StreamChunk::Error("Oops".to_string());
        assert!(error.is_error());
    }
}

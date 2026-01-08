//! LLM Provider Abstraction
//!
//! This module provides a trait-based abstraction for LLM providers.
//!
//! ## Available Providers
//!
//! - `claude_code`: Spawns `claude` CLI (uses claude-code's auth, no API key needed)
//! - `mock`: Mock provider for deterministic testing (test only)

pub mod claude_code;

#[cfg(test)]
pub mod mock;

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
#[allow(dead_code)]
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

    /// Get all text from the message
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("")
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
    /// Create a tool definition with properties (used for tests)
    #[cfg(test)]
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
    /// Text content
    Text(String),

    /// Tool call (full arguments available)
    #[allow(dead_code)]
    ToolCall {
        /// Name of the tool
        name: String,
        /// Complete arguments as JSON
        arguments: Value,
    },

    /// Stream completed successfully
    Done {
        /// Stop reason (e.g., "end_turn", "tool_use")
        #[allow(dead_code)]
        stop_reason: Option<String>,
    },

    /// Error occurred during streaming
    Error(String),
}

// =============================================================================
// Provider Trait
// =============================================================================

/// Configuration for LLM requests
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Model identifier (e.g., "claude-sonnet-4-20250514")
    #[allow(dead_code)]
    pub model: String,

    /// Maximum tokens to generate
    #[allow(dead_code)]
    pub max_tokens: u32,

    /// Temperature for sampling (0.0 - 1.0)
    #[allow(dead_code)]
    pub temperature: Option<f32>,

    /// System prompt
    #[allow(dead_code)]
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


/// Trait for LLM providers
///
/// Implementations must be thread-safe and support async streaming.
#[allow(dead_code)]
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
        assert!(matches!(block, ContentBlock::Text { .. }));
        assert_eq!(block.as_text(), Some("Hello"));
    }

    #[test]
    fn test_stream_chunk_variants() {
        let text = StreamChunk::Text("Hello".to_string());
        assert!(matches!(text, StreamChunk::Text(_)));

        let done = StreamChunk::Done {
            stop_reason: Some("end_turn".to_string()),
        };
        assert!(matches!(done, StreamChunk::Done { .. }));

        let error = StreamChunk::Error("Oops".to_string());
        assert!(matches!(error, StreamChunk::Error(_)));
    }
}

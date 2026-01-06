//! Claude Provider Implementation
//!
//! Implements the LlmProvider trait for Anthropic's Claude API.
//! Supports streaming responses via Server-Sent Events (SSE).
//!
//! # Configuration
//!
//! - API key: Set via `ANTHROPIC_API_KEY` environment variable or passed directly
//! - Model: Defaults to claude-sonnet-4-20250514, configurable via constructor
//!
//! # Example
//!
//! ```rust,ignore
//! use casparian::cli::tui::llm::claude::ClaudeProvider;
//! use casparian::cli::tui::llm::{Message, LlmProvider};
//!
//! let provider = ClaudeProvider::from_env()?;
//! let messages = vec![Message::user("Hello!")];
//! let mut stream = provider.chat(&messages, None).await?;
//! ```

use super::{
    ContentBlock, LlmConfig, LlmError, LlmProvider, Message, Role, StreamChunk, ToolDefinition,
};
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// Anthropic API base URL
const API_BASE_URL: &str = "https://api.anthropic.com/v1";

/// Default model to use
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// API version header
const API_VERSION: &str = "2023-06-01";

// =============================================================================
// API Request/Response Types
// =============================================================================

/// Request body for the Messages API
#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Message format for the API
#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: Vec<ApiContent>,
}

/// Content block for API messages
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

/// Tool definition for the API
#[derive(Debug, Serialize)]
struct ApiTool {
    name: String,
    description: String,
    input_schema: Value,
}

// =============================================================================
// SSE Event Types
// =============================================================================

/// Server-Sent Event types from Claude streaming API
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseEvent {
    MessageStart {
        message: SseMessage,
    },
    ContentBlockStart {
        index: usize,
        content_block: SseContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: SseDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: SseMessageDelta,
    },
    MessageStop,
    Ping,
    Error {
        error: SseError,
    },
}

#[derive(Debug, Deserialize)]
struct SseMessage {
    id: String,
    #[serde(rename = "type")]
    _type: String,
    role: String,
    model: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SseDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct SseMessageDelta {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

// =============================================================================
// Claude Provider
// =============================================================================

/// Claude API provider
pub struct ClaudeProvider {
    /// HTTP client
    client: Client,
    /// API key
    api_key: String,
    /// Default model
    model: String,
    /// Default config
    default_config: LlmConfig,
}

impl ClaudeProvider {
    /// Create a new Claude provider with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            default_config: LlmConfig::default(),
        }
    }

    /// Create a new Claude provider from environment variable
    ///
    /// Reads `ANTHROPIC_API_KEY` from environment
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            LlmError::ApiKey(
                "ANTHROPIC_API_KEY environment variable not set. \
                 Please set it or use --api-key argument."
                    .to_string(),
            )
        })?;

        if api_key.is_empty() {
            return Err(LlmError::ApiKey("ANTHROPIC_API_KEY is empty".to_string()));
        }

        Ok(Self::new(api_key))
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self.default_config.model = self.model.clone();
        self
    }

    /// Set the default system prompt
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.default_config.system = Some(system.into());
        self
    }

    /// Set default max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.default_config.max_tokens = max_tokens;
        self
    }

    /// Convert internal Message to API format
    fn to_api_message(msg: &Message) -> ApiMessage {
        let content = msg
            .content
            .iter()
            .map(|c| match c {
                ContentBlock::Text { text } => ApiContent::Text { text: text.clone() },
                ContentBlock::ToolUse { id, name, input } => ApiContent::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => ApiContent::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                },
            })
            .collect();

        ApiMessage {
            role: msg.role.to_string(),
            content,
        }
    }

    /// Convert tool definitions to API format
    fn to_api_tools(tools: &[ToolDefinition]) -> Vec<ApiTool> {
        tools
            .iter()
            .map(|t| ApiTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: serde_json::json!({
                    "type": t.input_schema.schema_type,
                    "properties": t.input_schema.properties,
                    "required": t.input_schema.required,
                }),
            })
            .collect()
    }

    /// Build the request body
    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: Option<&LlmConfig>,
    ) -> MessagesRequest {
        let config = config.unwrap_or(&self.default_config);

        MessagesRequest {
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            system: config.system.clone().or_else(|| self.default_config.system.clone()),
            messages: messages.iter().map(Self::to_api_message).collect(),
            tools: Self::to_api_tools(tools),
            stream: true,
            temperature: config.temperature,
        }
    }

    /// Parse an SSE line into an event
    fn parse_sse_line(line: &str) -> Option<SseEvent> {
        // SSE format: "data: {...}"
        if !line.starts_with("data: ") {
            return None;
        }

        let json_str = &line[6..]; // Skip "data: "
        if json_str.is_empty() || json_str == "[DONE]" {
            return None;
        }

        serde_json::from_str(json_str).ok()
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "Claude"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn is_ready(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        let request = self.build_request(messages, tools, config);

        // Send the request
        let response = self
            .client
            .post(format!("{}/messages", API_BASE_URL))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        // Check for errors
        let status = response.status();
        if !status.is_success() {
            // Try to read error body
            let error_text = response.text().await.unwrap_or_default();

            // Check for rate limiting
            if status.as_u16() == 429 {
                // Try to parse retry-after
                let retry_ms = 60000; // Default 1 minute
                return Err(LlmError::RateLimit {
                    retry_after_ms: retry_ms,
                });
            }

            return Err(LlmError::Provider {
                provider: "Claude".to_string(),
                message: format!("HTTP {}: {}", status, error_text),
            });
        }

        // Create a channel for streaming
        let (tx, rx) = mpsc::channel::<StreamChunk>(100);

        // Spawn task to process SSE stream
        let byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut current_tool_calls: Vec<ToolCallAccumulator> = Vec::new();

            let mut stream = Box::pin(byte_stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            if let Some(event) = Self::parse_sse_line(&line) {
                                match event {
                                    SseEvent::ContentBlockStart { index, content_block } => {
                                        match content_block {
                                            SseContentBlock::Text { text } => {
                                                if !text.is_empty() {
                                                    let _ = tx.send(StreamChunk::Text(text)).await;
                                                }
                                            }
                                            SseContentBlock::ToolUse { id, name, .. } => {
                                                // Start accumulating tool call
                                                while current_tool_calls.len() <= index {
                                                    current_tool_calls.push(ToolCallAccumulator::default());
                                                }
                                                current_tool_calls[index] = ToolCallAccumulator {
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                    arguments_json: String::new(),
                                                };

                                                let _ = tx
                                                    .send(StreamChunk::ToolCallStart { index, id, name })
                                                    .await;
                                            }
                                        }
                                    }
                                    SseEvent::ContentBlockDelta { index, delta } => {
                                        match delta {
                                            SseDelta::TextDelta { text } => {
                                                let _ = tx.send(StreamChunk::Text(text)).await;
                                            }
                                            SseDelta::InputJsonDelta { partial_json } => {
                                                // Accumulate tool arguments
                                                if index < current_tool_calls.len() {
                                                    current_tool_calls[index]
                                                        .arguments_json
                                                        .push_str(&partial_json);
                                                }

                                                let _ = tx
                                                    .send(StreamChunk::ToolCallDelta {
                                                        index,
                                                        arguments_delta: partial_json,
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                    SseEvent::ContentBlockStop { index } => {
                                        // If this was a tool call, emit the complete tool call
                                        if index < current_tool_calls.len() {
                                            let acc = &current_tool_calls[index];
                                            if !acc.id.is_empty() {
                                                let arguments: Value =
                                                    serde_json::from_str(&acc.arguments_json)
                                                        .unwrap_or(Value::Object(
                                                            serde_json::Map::new(),
                                                        ));

                                                let _ = tx
                                                    .send(StreamChunk::ToolCall {
                                                        id: acc.id.clone(),
                                                        name: acc.name.clone(),
                                                        arguments,
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                    SseEvent::MessageDelta { delta } => {
                                        // Message is complete
                                        let _ = tx
                                            .send(StreamChunk::Done {
                                                stop_reason: delta.stop_reason,
                                            })
                                            .await;
                                    }
                                    SseEvent::MessageStop => {
                                        // Redundant with MessageDelta, but handle anyway
                                    }
                                    SseEvent::Error { error } => {
                                        let _ = tx
                                            .send(StreamChunk::Error(format!(
                                                "{}: {}",
                                                error.error_type, error.message
                                            )))
                                            .await;
                                    }
                                    SseEvent::MessageStart { .. } | SseEvent::Ping => {
                                        // Ignore
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(e.to_string())).await;
                        break;
                    }
                }
            }
        });

        // Convert receiver to stream
        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

/// Helper to accumulate tool call arguments
#[derive(Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments_json: String,
}

/// Wrapper to convert mpsc::Receiver into a Stream
struct ReceiverStream {
    rx: mpsc::Receiver<StreamChunk>,
}

impl ReceiverStream {
    fn new(rx: mpsc::Receiver<StreamChunk>) -> Self {
        Self { rx }
    }
}

impl Stream for ReceiverStream {
    type Item = StreamChunk;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = ClaudeProvider::new("test-key");
        assert_eq!(provider.name(), "Claude");
        assert_eq!(provider.model(), DEFAULT_MODEL);
        assert!(provider.is_ready());
    }

    #[test]
    fn test_provider_with_model() {
        let provider = ClaudeProvider::new("test-key").with_model("claude-opus-4-20250514");
        assert_eq!(provider.model(), "claude-opus-4-20250514");
    }

    #[test]
    fn test_empty_key_not_ready() {
        let provider = ClaudeProvider::new("");
        assert!(!provider.is_ready());
    }

    #[test]
    fn test_to_api_message() {
        let msg = Message::user("Hello");
        let api_msg = ClaudeProvider::to_api_message(&msg);

        assert_eq!(api_msg.role, "user");
        assert_eq!(api_msg.content.len(), 1);
    }

    #[test]
    fn test_to_api_tools() {
        let tools = vec![ToolDefinition::with_properties(
            "test",
            "Test tool",
            serde_json::json!({"arg": {"type": "string"}}),
            vec!["arg".to_string()],
        )];

        let api_tools = ClaudeProvider::to_api_tools(&tools);
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0].name, "test");
    }

    #[test]
    fn test_parse_sse_line() {
        // Valid text delta
        let line = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event = ClaudeProvider::parse_sse_line(line);
        assert!(event.is_some());

        // Empty data
        let empty = "data: ";
        assert!(ClaudeProvider::parse_sse_line(empty).is_none());

        // DONE marker
        let done = "data: [DONE]";
        assert!(ClaudeProvider::parse_sse_line(done).is_none());

        // Non-data line
        let other = "event: message";
        assert!(ClaudeProvider::parse_sse_line(other).is_none());
    }

    #[test]
    fn test_build_request() {
        let provider = ClaudeProvider::new("test-key").with_system("You are helpful");
        let messages = vec![Message::user("Hello")];
        let tools = vec![];

        let request = provider.build_request(&messages, &tools, None);

        assert_eq!(request.model, DEFAULT_MODEL);
        assert!(request.stream);
        assert_eq!(request.system, Some("You are helpful".to_string()));
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn test_build_request_with_config() {
        let provider = ClaudeProvider::new("test-key");
        let messages = vec![Message::user("Hello")];
        let config = LlmConfig::with_model("claude-opus-4-20250514")
            .max_tokens(8192)
            .temperature(0.5);

        let request = provider.build_request(&messages, &[], Some(&config));

        assert_eq!(request.model, "claude-opus-4-20250514");
        assert_eq!(request.max_tokens, 8192);
        assert_eq!(request.temperature, Some(0.5));
    }

    #[test]
    fn test_tool_call_accumulator() {
        let mut acc = ToolCallAccumulator::default();
        assert!(acc.id.is_empty());

        acc.id = "123".to_string();
        acc.name = "test_tool".to_string();
        acc.arguments_json.push_str(r#"{"arg":"#);
        acc.arguments_json.push_str(r#""value"}"#);

        let args: Value = serde_json::from_str(&acc.arguments_json).unwrap();
        assert_eq!(args["arg"], "value");
    }
}

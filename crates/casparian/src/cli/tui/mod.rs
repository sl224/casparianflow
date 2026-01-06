//! TUI Module for Casparian Flow
//!
//! Provides an interactive terminal user interface with LLM integration.
//! The TUI enables users to interact with Casparian Flow through natural
//! language conversations powered by Claude.
//!
//! # Architecture
//!
//! ```text
//! +------------------+
//! |     TUI App      |
//! +------------------+
//!         |
//!         v
//! +------------------+     +------------------+
//! |  LLM Provider    |<--->|   Tool Registry  |
//! | (Claude/OpenAI)  |     |   (MCP Tools)    |
//! +------------------+     +------------------+
//! ```
//!
//! # Features
//!
//! - Natural language interaction with Claude
//! - Streaming responses for responsive UI
//! - Tool calling for MCP tool execution
//! - Conversation history management

pub mod llm;

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::ToolError;
use futures::StreamExt;
use llm::{
    claude::ClaudeProvider, ContentBlock, LlmConfig, LlmError, LlmProvider, Message, StreamChunk,
    ToolDefinition,
};
use std::sync::Arc;
use tokio::sync::RwLock;

// =============================================================================
// App State
// =============================================================================

/// Chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Message role (user/assistant/system)
    pub role: String,
    /// Message content
    pub content: String,
    /// Whether this is a tool result
    pub is_tool_result: bool,
    /// Tool name if this is a tool call
    pub tool_name: Option<String>,
}

impl ChatMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            is_tool_result: false,
            tool_name: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            is_tool_result: false,
            tool_name: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            is_tool_result: true,
            tool_name: Some(tool_name.into()),
        }
    }
}

/// TUI Application state
pub struct App {
    /// LLM provider for chat
    llm: Box<dyn LlmProvider>,

    /// Tool registry for MCP tools
    registry: ToolRegistry,

    /// Tool definitions for LLM
    tools: Vec<ToolDefinition>,

    /// Conversation history for LLM API
    messages: Arc<RwLock<Vec<Message>>>,

    /// Chat messages for display
    chat_history: Arc<RwLock<Vec<ChatMessage>>>,

    /// LLM configuration
    config: LlmConfig,

    /// Whether we're currently processing
    is_processing: Arc<RwLock<bool>>,
}

impl App {
    /// Create a new App with the given LLM provider
    pub fn new(llm: Box<dyn LlmProvider>) -> Self {
        let registry = create_default_registry();
        let tools = llm::registry_to_definitions(&registry);

        Self {
            llm,
            registry,
            tools,
            messages: Arc::new(RwLock::new(Vec::new())),
            chat_history: Arc::new(RwLock::new(Vec::new())),
            config: LlmConfig::default().system(DEFAULT_SYSTEM_PROMPT.to_string()),
            is_processing: Arc::new(RwLock::new(false)),
        }
    }

    /// Create App with Claude provider from environment
    pub fn with_claude_from_env() -> Result<Self, LlmError> {
        let provider = ClaudeProvider::from_env()?;
        Ok(Self::new(Box::new(provider)))
    }

    /// Create App with Claude provider and custom API key
    pub fn with_claude(api_key: impl Into<String>) -> Self {
        let provider = ClaudeProvider::new(api_key);
        Self::new(Box::new(provider))
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set the system prompt
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.config.system = Some(system.into());
        self
    }

    /// Get the LLM provider name
    pub fn provider_name(&self) -> &str {
        self.llm.name()
    }

    /// Get the current model
    pub fn model(&self) -> &str {
        self.llm.model()
    }

    /// Check if the LLM is ready
    pub fn is_ready(&self) -> bool {
        self.llm.is_ready()
    }

    /// Check if currently processing a message
    pub async fn is_processing(&self) -> bool {
        *self.is_processing.read().await
    }

    /// Get chat history for display
    pub async fn chat_history(&self) -> Vec<ChatMessage> {
        self.chat_history.read().await.clone()
    }

    /// Get available tools
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    /// Send a user message and process the response
    ///
    /// This method:
    /// 1. Adds the user message to history
    /// 2. Calls the LLM with streaming
    /// 3. Processes any tool calls
    /// 4. Returns when the conversation turn is complete
    ///
    /// # Arguments
    /// * `message` - User's input message
    /// * `on_chunk` - Callback for each streamed chunk
    pub async fn send_message<F>(
        &self,
        message: impl Into<String>,
        mut on_chunk: F,
    ) -> Result<(), LlmError>
    where
        F: FnMut(StreamChunk) + Send,
    {
        let message_text = message.into();

        // Set processing flag
        {
            *self.is_processing.write().await = true;
        }

        // Add user message to history
        {
            let mut messages = self.messages.write().await;
            messages.push(Message::user(&message_text));
        }
        {
            let mut chat_history = self.chat_history.write().await;
            chat_history.push(ChatMessage::user(&message_text));
        }

        // Process conversation (may loop for tool calls)
        let result = self.process_conversation(&mut on_chunk).await;

        // Clear processing flag
        {
            *self.is_processing.write().await = false;
        }

        result
    }

    /// Process the conversation, handling tool calls
    async fn process_conversation<F>(&self, on_chunk: &mut F) -> Result<(), LlmError>
    where
        F: FnMut(StreamChunk) + Send,
    {
        loop {
            // Get messages
            let messages = {
                self.messages.read().await.clone()
            };

            // Call LLM
            let mut stream = self
                .llm
                .chat_stream(&messages, &self.tools, Some(&self.config))
                .await?;

            // Collect response
            let mut text_content = String::new();
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
            let mut stop_reason: Option<String> = None;

            while let Some(chunk) = stream.next().await {
                // Forward chunk to callback
                on_chunk(chunk.clone());

                match chunk {
                    StreamChunk::Text(text) => {
                        text_content.push_str(&text);
                    }
                    StreamChunk::ToolCall { id, name, arguments } => {
                        tool_calls.push((id, name, arguments));
                    }
                    StreamChunk::Done { stop_reason: reason } => {
                        stop_reason = reason;
                    }
                    StreamChunk::Error(e) => {
                        return Err(LlmError::Stream(e));
                    }
                    _ => {}
                }
            }

            // Build assistant content blocks
            let mut content_blocks = Vec::new();
            if !text_content.is_empty() {
                content_blocks.push(ContentBlock::text(&text_content));
            }
            for (id, name, input) in &tool_calls {
                content_blocks.push(ContentBlock::tool_use(id, name, input.clone()));
            }

            // Add assistant message to history
            if !content_blocks.is_empty() {
                let mut messages = self.messages.write().await;
                messages.push(Message::assistant_with_content(content_blocks));
            }

            // Add to chat history for display
            if !text_content.is_empty() {
                let mut chat_history = self.chat_history.write().await;
                chat_history.push(ChatMessage::assistant(&text_content));
            }

            // If there were tool calls, execute them and continue
            if !tool_calls.is_empty() {
                let tool_results = self.execute_tools(tool_calls).await;

                // Add tool results to messages
                {
                    let mut messages = self.messages.write().await;
                    messages.push(Message::tool_results(tool_results.clone()));
                }

                // Add to chat history for display
                {
                    let mut chat_history = self.chat_history.write().await;
                    for result in &tool_results {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = result
                        {
                            chat_history.push(ChatMessage::tool_result(tool_use_id, content));
                        }
                    }
                }

                // Continue the loop to get the next response
                continue;
            }

            // No tool calls, we're done
            break;
        }

        Ok(())
    }

    /// Execute tool calls and return results
    async fn execute_tools(
        &self,
        tool_calls: Vec<(String, String, serde_json::Value)>,
    ) -> Vec<ContentBlock> {
        let mut results = Vec::new();

        for (id, name, arguments) in tool_calls {
            let result = self.execute_tool(&name, arguments).await;

            match result {
                Ok(content) => {
                    results.push(ContentBlock::tool_result(&id, content));
                }
                Err(e) => {
                    results.push(ContentBlock::tool_error(&id, e.to_string()));
                }
            }
        }

        results
    }

    /// Execute a single tool
    async fn execute_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, ToolError> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| ToolError::NotFound(format!("Tool '{}' not found", name)))?;

        let result = tool.execute(arguments).await?;

        // Extract text from result
        let text = result
            .content
            .iter()
            .filter_map(|c| match c {
                casparian_mcp::types::ToolContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(text)
    }

    /// Clear conversation history
    pub async fn clear_history(&self) {
        self.messages.write().await.clear();
        self.chat_history.write().await.clear();
    }
}

/// Default system prompt for the TUI
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are an AI assistant for Casparian Flow, a data processing platform.

You help users:
- Discover and scan files in directories
- Analyze file schemas and structures
- Create and test data parsers
- Run backtests against sample files
- Execute data pipelines

You have access to tools for these operations. When users ask about their data, use the appropriate tools to help them.

Be concise but helpful. Use tools proactively when they would provide useful information.

When displaying tool results, summarize the key findings rather than showing raw JSON.
"#;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "Hello");
        assert!(!user.is_tool_result);

        let assistant = ChatMessage::assistant("Hi there");
        assert_eq!(assistant.role, "assistant");

        let tool = ChatMessage::tool_result("quick_scan", r#"{"files": 10}"#);
        assert!(tool.is_tool_result);
        assert_eq!(tool.tool_name, Some("quick_scan".to_string()));
    }

    #[test]
    fn test_default_system_prompt() {
        assert!(!DEFAULT_SYSTEM_PROMPT.is_empty());
        assert!(DEFAULT_SYSTEM_PROMPT.contains("Casparian Flow"));
    }
}

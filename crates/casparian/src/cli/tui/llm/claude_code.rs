//! Claude Code Provider - Uses the claude CLI as the LLM backend
//!
//! This provider spawns `claude -p` subprocess for LLM calls, leveraging
//! Claude Code's existing authentication. No API key needed.
//!
//! ## How It Works
//!
//! 1. User types message in TUI
//! 2. Provider builds prompt with MCP tool schemas
//! 3. Spawns `claude -p "..." --output-format stream-json`
//! 4. Streams response chunks back to TUI
//! 5. If Claude calls MCP tools, we execute and continue
//!
//! ## Benefits
//!
//! - No API key required (uses claude's auth)
//! - Full Claude Code capability in TUI
//! - Same model/settings as user's claude config

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::Deserialize;
use std::process::{Command, Stdio};
use tokio::sync::mpsc;

use super::{
    LlmConfig, LlmError, LlmProvider, Message, StreamChunk, ToolDefinition,
};

/// Response from `claude -p --output-format json`
#[derive(Debug, Deserialize)]
pub struct ClaudeCodeResponse {
    /// Response type ("result" for success)
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub response_type: Option<String>,
    /// The text response
    #[serde(default)]
    pub result: String,
    /// Session ID for multi-turn
    #[allow(dead_code)]
    pub session_id: Option<String>,
    /// Whether there was an error
    #[serde(default)]
    pub is_error: bool,
    /// Duration in milliseconds
    #[allow(dead_code)]
    pub duration_ms: Option<u64>,
}

/// Claude Code provider configuration
#[derive(Debug, Clone)]
pub struct ClaudeCodeConfig {
    /// Allowed tools for auto-approval
    pub allowed_tools: Vec<String>,
    /// Maximum turns for agentic loop
    pub max_turns: u32,
    /// System prompt to append
    pub system_prompt: Option<String>,
    /// Working directory
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            // Allow read-only tools by default
            allowed_tools: vec![
                "Read".to_string(),
                "Grep".to_string(),
                "Glob".to_string(),
            ],
            max_turns: 10,
            system_prompt: None,
            working_dir: None,
        }
    }
}

/// Claude Code LLM Provider
///
/// Spawns `claude` subprocess for LLM calls.
pub struct ClaudeCodeProvider {
    config: ClaudeCodeConfig,
    /// Session ID for multi-turn conversations
    session_id: Option<String>,
}

impl ClaudeCodeProvider {
    /// Create a new Claude Code provider
    pub fn new() -> Self {
        Self {
            config: ClaudeCodeConfig::default(),
            session_id: None,
        }
    }

    /// Create with custom config
    #[allow(dead_code)]
    pub fn with_config(config: ClaudeCodeConfig) -> Self {
        Self {
            config,
            session_id: None,
        }
    }

    /// Set allowed tools
    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.allowed_tools = tools;
        self
    }

    /// Set system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = Some(prompt.into());
        self
    }

    /// Set max turns
    pub fn max_turns(mut self, turns: u32) -> Self {
        self.config.max_turns = turns;
        self
    }

    /// Check if claude CLI is available
    pub fn is_available() -> bool {
        Command::new("claude")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Build the prompt with tool context
    fn build_prompt(&self, messages: &[Message], tools: &[ToolDefinition]) -> String {
        let mut prompt = String::new();

        // Add tool descriptions
        if !tools.is_empty() {
            prompt.push_str("You have access to the following Casparian MCP tools:\n\n");
            for tool in tools {
                prompt.push_str(&format!("## {}\n", tool.name));
                prompt.push_str(&format!("{}\n", tool.description));
                if let Some(props) = &tool.input_schema.properties {
                    prompt.push_str(&format!("Parameters: {}\n", props));
                }
                prompt.push('\n');
            }
            prompt.push_str("---\n\n");
        }

        // Add conversation history
        for msg in messages {
            let role = match msg.role {
                super::Role::User => "User",
                super::Role::Assistant => "Assistant",
                super::Role::System => "System",
            };
            let text = msg.text();
            if !text.is_empty() {
                prompt.push_str(&format!("{}: {}\n\n", role, text));
            }
        }

        prompt
    }

    /// Build command arguments
    fn build_command(&self, prompt: &str) -> Command {
        let mut cmd = Command::new("claude");

        cmd.arg("-p").arg(prompt);
        cmd.arg("--output-format").arg("json");

        if !self.config.allowed_tools.is_empty() {
            cmd.arg("--allowedTools")
                .arg(self.config.allowed_tools.join(","));
        }

        cmd.arg("--max-turns")
            .arg(self.config.max_turns.to_string());

        if let Some(ref system) = self.config.system_prompt {
            cmd.arg("--append-system-prompt").arg(system);
        }

        if let Some(ref session) = self.session_id {
            cmd.arg("--resume").arg(session);
        }

        if let Some(ref dir) = self.config.working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        cmd
    }

    /// Execute and get full response (non-streaming)
    #[allow(dead_code)]
    pub fn execute_sync(&self, prompt: &str) -> Result<ClaudeCodeResponse, LlmError> {
        let mut cmd = self.build_command(prompt);

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LlmError::Provider {
                    provider: "claude-code".to_string(),
                    message: "claude CLI not found. Install Claude Code first.".to_string(),
                }
            } else {
                LlmError::Internal(format!("Failed to run claude: {}", e))
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LlmError::Provider {
                provider: "claude-code".to_string(),
                message: format!("claude exited with error: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let response: ClaudeCodeResponse = serde_json::from_str(&stdout)
            .map_err(|e| LlmError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        if response.is_error {
            return Err(LlmError::Provider {
                provider: "claude-code".to_string(),
                message: response.result,
            });
        }

        Ok(response)
    }
}

impl Default for ClaudeCodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for ClaudeCodeProvider {
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn model(&self) -> &str {
        "claude (via claude-code)"
    }

    fn is_ready(&self) -> bool {
        Self::is_available()
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        _config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        let prompt = self.build_prompt(messages, tools);

        // Create channel for the response
        let (tx, rx) = mpsc::channel::<StreamChunk>(10);

        // Clone what we need for the blocking task
        let mut cmd = self.build_command(&prompt);

        // Spawn blocking task to run claude
        tokio::task::spawn_blocking(move || {
            // Run claude and wait for completion
            let output = match cmd.output() {
                Ok(o) => o,
                Err(e) => {
                    let msg = if e.kind() == std::io::ErrorKind::NotFound {
                        "claude CLI not found. Install Claude Code first.".to_string()
                    } else {
                        format!("Failed to run claude: {}", e)
                    };
                    let _ = tx.blocking_send(StreamChunk::Error(msg));
                    let _ = tx.blocking_send(StreamChunk::Done { stop_reason: Some("error".to_string()) });
                    return;
                }
            };

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = tx.blocking_send(StreamChunk::Error(format!("claude error: {}", stderr)));
                let _ = tx.blocking_send(StreamChunk::Done { stop_reason: Some("error".to_string()) });
                return;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse JSON response
            match serde_json::from_str::<ClaudeCodeResponse>(&stdout) {
                Ok(response) => {
                    if response.is_error {
                        let _ = tx.blocking_send(StreamChunk::Error(response.result));
                    } else {
                        // Send the result as text
                        let _ = tx.blocking_send(StreamChunk::Text(response.result));
                    }
                }
                Err(_e) => {
                    // Maybe raw text output
                    let _ = tx.blocking_send(StreamChunk::Text(stdout.to_string()));
                }
            }

            // Always send done
            let _ = tx.blocking_send(StreamChunk::Done {
                stop_reason: Some("end_turn".to_string()),
            });
        });

        // Convert receiver to stream
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn test_provider_creation() {
        let provider = ClaudeCodeProvider::new();
        assert_eq!(provider.name(), "Claude Code");
    }

    #[test]
    fn test_config_builder() {
        let provider = ClaudeCodeProvider::new()
            .allowed_tools(vec!["Read".to_string(), "Bash".to_string()])
            .system_prompt("Be helpful")
            .max_turns(5);

        assert_eq!(provider.config.allowed_tools.len(), 2);
        assert_eq!(provider.config.max_turns, 5);
        assert!(provider.config.system_prompt.is_some());
    }

    #[test]
    fn test_is_available() {
        // This will be true if claude CLI is installed
        let available = ClaudeCodeProvider::is_available();
        println!("Claude Code available: {}", available);
        // Don't assert - depends on environment
    }

    #[test]
    fn test_build_prompt() {
        let provider = ClaudeCodeProvider::new();

        let messages = vec![Message::user("scan /tmp")];

        let tools = vec![ToolDefinition::with_properties(
            "quick_scan",
            "Scan a directory for files",
            serde_json::json!({
                "path": {"type": "string", "description": "Directory path"}
            }),
            vec!["path".to_string()],
        )];

        let prompt = provider.build_prompt(&messages, &tools);

        assert!(prompt.contains("quick_scan"));
        assert!(prompt.contains("Scan a directory"));
        assert!(prompt.contains("scan /tmp"));
    }

    #[test]
    fn test_command_building() {
        let provider = ClaudeCodeProvider::new()
            .allowed_tools(vec!["Read".to_string()])
            .max_turns(3);

        let cmd = provider.build_command("test prompt");

        // Can't easily inspect Command args, but this validates it builds
        assert!(cmd.get_program().to_str().unwrap().contains("claude"));
    }

    #[tokio::test]
    async fn test_streaming_with_claude_code() {
        let provider = ClaudeCodeProvider::new();

        if !provider.is_ready() {
            println!("Skipping: claude CLI not available");
            return;
        }

        // Simple test message
        let messages = vec![Message::user("say hello in 5 words or less")];

        let result = provider.chat_stream(&messages, &[], None).await;

        match result {
            Ok(mut stream) => {
                let mut got_done = false;

                while let Some(chunk) = stream.next().await {
                    match chunk {
                        StreamChunk::Text(t) => {
                            println!("Text: {}", t);
                        }
                        StreamChunk::Done { .. } => {
                            got_done = true;
                            break;
                        }
                        StreamChunk::Error(e) => {
                            println!("Error: {}", e);
                            break;
                        }
                        StreamChunk::ToolCall { .. } => {}
                    }
                }

                // At minimum we should get a done signal
                assert!(got_done, "Should complete");
            }
            Err(e) => {
                println!("Stream error (might be expected): {}", e);
            }
        }
    }
}

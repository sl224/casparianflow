//! Application state for the TUI

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::{ToolResult, WorkflowMetadata};
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use serde_json::Value;

use super::llm::claude_code::ClaudeCodeProvider;
use super::llm::{registry_to_definitions, LlmProvider, StreamChunk};
use super::TuiArgs;

/// Maximum number of messages to keep in input history
const MAX_INPUT_HISTORY: usize = 50;

/// Current view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Chat,
    Monitor,
    Help,
}

/// Chat message
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Local>,
}

impl Message {
    pub fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: Local::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Input history for recalling previous messages
#[derive(Debug, Default)]
pub struct InputHistory {
    /// Previous inputs
    entries: Vec<String>,
    /// Current position in history (None = new input)
    position: Option<usize>,
    /// Draft input being typed (preserved when browsing history)
    draft: String,
}

impl InputHistory {
    /// Add an entry to history
    pub fn push(&mut self, input: String) {
        if !input.is_empty() {
            // Don't add duplicates of the last entry
            if self.entries.last() != Some(&input) {
                self.entries.push(input);
                // Keep history bounded
                if self.entries.len() > MAX_INPUT_HISTORY {
                    self.entries.remove(0);
                }
            }
        }
        self.position = None;
        self.draft.clear();
    }

    /// Move up in history (older entries)
    pub fn up(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.position {
            None => {
                // Save current input as draft before browsing history
                self.draft = current_input.to_string();
                self.position = Some(self.entries.len() - 1);
                Some(&self.entries[self.entries.len() - 1])
            }
            Some(pos) if pos > 0 => {
                self.position = Some(pos - 1);
                Some(&self.entries[pos - 1])
            }
            Some(_) => {
                // Already at oldest entry
                Some(&self.entries[0])
            }
        }
    }

    /// Move down in history (newer entries)
    pub fn down(&mut self) -> Option<&str> {
        match self.position {
            None => None, // Already at current input
            Some(pos) => {
                if pos < self.entries.len() - 1 {
                    self.position = Some(pos + 1);
                    Some(&self.entries[pos + 1])
                } else {
                    // Return to draft input
                    self.position = None;
                    Some(&self.draft)
                }
            }
        }
    }

    /// Reset position (when user starts typing)
    pub fn reset_position(&mut self) {
        self.position = None;
    }
}

/// Chat state
#[derive(Debug)]
pub struct ChatState {
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Current input buffer (supports multi-line)
    pub input: String,
    /// Cursor position in input (byte offset)
    pub cursor: usize,
    /// Scroll offset for message list (in lines)
    pub scroll: usize,
    /// Whether waiting for LLM response
    pub awaiting_response: bool,
    /// Current workflow metadata from last tool
    pub workflow: Option<WorkflowMetadata>,
    /// Input history
    pub input_history: InputHistory,
    /// Whether in history browsing mode
    pub browsing_history: bool,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            messages: vec![Message::new(
                MessageRole::System,
                "Welcome to Casparian TUI. Type a message to chat with Claude about your data pipelines.\n\nKeyboard shortcuts:\n  Shift+Enter: New line\n  Enter: Send message\n  Up/Down: Browse input history (when input is single-line)\n  Ctrl+Up/Down: Scroll messages\n  Esc: Clear input".into(),
            )],
            input: String::new(),
            cursor: 0,
            scroll: 0,
            awaiting_response: false,
            workflow: None,
            input_history: InputHistory::default(),
            browsing_history: false,
        }
    }
}

impl ChatState {
    /// Get current line and column from cursor position
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.input[..self.cursor];
        let line = before_cursor.matches('\n').count();
        let col = before_cursor.rfind('\n').map_or(self.cursor, |pos| self.cursor - pos - 1);
        (line, col)
    }

    /// Get the number of lines in the input
    pub fn input_line_count(&self) -> usize {
        self.input.matches('\n').count() + 1
    }

    /// Check if input is single-line (for history browsing)
    pub fn is_single_line(&self) -> bool {
        !self.input.contains('\n')
    }

    /// Insert a newline at cursor position
    pub fn insert_newline(&mut self) {
        self.input.insert(self.cursor, '\n');
        self.cursor += 1;
    }

    /// Move cursor up one line
    pub fn cursor_up(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line == 0 {
            return;
        }

        // Find the start of current line
        let current_line_start = self.input[..self.cursor]
            .rfind('\n')
            .map_or(0, |pos| pos + 1);

        // Find the start of previous line
        let prev_line_start = if current_line_start == 0 {
            0
        } else {
            self.input[..current_line_start - 1]
                .rfind('\n')
                .map_or(0, |pos| pos + 1)
        };

        // Find the end of previous line (or use the column)
        let prev_line_len = current_line_start - 1 - prev_line_start;
        let new_col = col.min(prev_line_len);
        self.cursor = prev_line_start + new_col;
    }

    /// Move cursor down one line
    pub fn cursor_down(&mut self) {
        let (line, col) = self.cursor_line_col();
        let total_lines = self.input_line_count();
        if line >= total_lines - 1 {
            return;
        }

        // Find the end of current line
        let current_line_end = self.input[self.cursor..]
            .find('\n')
            .map(|pos| self.cursor + pos)
            .unwrap_or(self.input.len());

        if current_line_end >= self.input.len() {
            return;
        }

        // Next line starts after the newline
        let next_line_start = current_line_end + 1;

        // Find the end of next line
        let next_line_end = self.input[next_line_start..]
            .find('\n')
            .map(|pos| next_line_start + pos)
            .unwrap_or(self.input.len());

        let next_line_len = next_line_end - next_line_start;
        let new_col = col.min(next_line_len);
        self.cursor = next_line_start + new_col;
    }
}

/// Monitor state
#[derive(Debug, Default)]
pub struct MonitorState {
    /// Selected job index
    pub selected: usize,
    /// Placeholder job count (will be fetched from DB)
    pub job_count: usize,
}

/// Main application state
pub struct App {
    /// Whether app is running
    pub running: bool,
    /// Current view
    pub view: View,
    /// Chat state
    pub chat: ChatState,
    /// Monitor state
    pub monitor: MonitorState,
    /// Tool registry for executing MCP tools
    pub tools: ToolRegistry,
    /// LLM provider (Claude Code if available)
    pub llm: Option<ClaudeCodeProvider>,
    /// Configuration
    pub config: TuiArgs,
    /// Last error message
    pub error: Option<String>,
}

impl App {
    /// Create new app with given args
    pub fn new(args: TuiArgs) -> Self {
        // Check if Claude Code is available
        let llm = if ClaudeCodeProvider::is_available() {
            Some(
                ClaudeCodeProvider::new()
                    .allowed_tools(vec![
                        "Read".to_string(),
                        "Grep".to_string(),
                        "Glob".to_string(),
                        "Bash".to_string(),
                    ])
                    .system_prompt(
                        "You are helping the user with Casparian Flow, a data pipeline tool. \
                         You have access to MCP tools for scanning files, discovering schemas, \
                         and building data pipelines. Be concise and helpful.",
                    )
                    .max_turns(5),
            )
        } else {
            None
        };

        Self {
            running: true,
            view: View::Chat,
            chat: ChatState::default(),
            monitor: MonitorState::default(),
            tools: create_default_registry(),
            llm,
            config: args,
            error: None,
        }
    }

    /// Create app with custom tool registry (for testing)
    pub fn new_with_registry(registry: ToolRegistry, args: TuiArgs) -> Self {
        Self {
            running: true,
            view: View::Chat,
            chat: ChatState::default(),
            monitor: MonitorState::default(),
            tools: registry,
            llm: None,
            config: args,
            error: None,
        }
    }

    /// Handle key event
    pub async fn handle_key(&mut self, key: KeyEvent) {
        // Global keys
        match key.code {
            KeyCode::F(1) => {
                self.view = View::Chat;
                return;
            }
            KeyCode::F(2) => {
                self.view = View::Monitor;
                return;
            }
            KeyCode::F(3) => {
                self.view = View::Help;
                return;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
                return;
            }
            KeyCode::Char('q') if self.view == View::Help => {
                self.view = View::Chat;
                return;
            }
            _ => {}
        }

        // View-specific keys
        match self.view {
            View::Chat => self.handle_chat_key(key).await,
            View::Monitor => self.handle_monitor_key(key),
            View::Help => {} // Handled above
        }
    }

    /// Handle chat view keys
    async fn handle_chat_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                // Any typing resets history browsing
                self.chat.input_history.reset_position();
                self.chat.browsing_history = false;
                self.chat.input.insert(self.cursor_byte_pos(), c);
                self.chat.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if self.chat.cursor > 0 {
                    // Find the previous character boundary
                    let mut new_cursor = self.chat.cursor - 1;
                    while new_cursor > 0 && !self.chat.input.is_char_boundary(new_cursor) {
                        new_cursor -= 1;
                    }
                    self.chat.input.remove(new_cursor);
                    self.chat.cursor = new_cursor;
                }
            }
            KeyCode::Delete => {
                if self.chat.cursor < self.chat.input.len() {
                    self.chat.input.remove(self.chat.cursor);
                }
            }
            KeyCode::Left => {
                if self.chat.cursor > 0 {
                    // Move to previous character boundary
                    self.chat.cursor -= 1;
                    while self.chat.cursor > 0 && !self.chat.input.is_char_boundary(self.chat.cursor) {
                        self.chat.cursor -= 1;
                    }
                }
            }
            KeyCode::Right => {
                if self.chat.cursor < self.chat.input.len() {
                    // Move to next character boundary
                    self.chat.cursor += 1;
                    while self.chat.cursor < self.chat.input.len()
                        && !self.chat.input.is_char_boundary(self.chat.cursor)
                    {
                        self.chat.cursor += 1;
                    }
                }
            }
            KeyCode::Home => {
                // Move to start of current line
                let line_start = self.chat.input[..self.chat.cursor]
                    .rfind('\n')
                    .map_or(0, |pos| pos + 1);
                self.chat.cursor = line_start;
            }
            KeyCode::End => {
                // Move to end of current line
                let line_end = self.chat.input[self.chat.cursor..]
                    .find('\n')
                    .map(|pos| self.chat.cursor + pos)
                    .unwrap_or(self.chat.input.len());
                self.chat.cursor = line_end;
            }
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Enter: Insert newline
                    self.chat.insert_newline();
                } else if !self.chat.input.is_empty() && !self.chat.awaiting_response {
                    // Enter: Send message
                    self.send_message().await;
                }
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Up: Scroll messages up
                    if self.chat.scroll > 0 {
                        self.chat.scroll -= 1;
                    }
                } else if self.chat.is_single_line() && self.chat.input.is_empty() {
                    // Up in empty single-line input: Browse history
                    if let Some(prev) = self.chat.input_history.up(&self.chat.input) {
                        self.chat.input = prev.to_string();
                        self.chat.cursor = self.chat.input.len();
                        self.chat.browsing_history = true;
                    }
                } else if !self.chat.is_single_line() {
                    // Up in multi-line: Move cursor up
                    self.chat.cursor_up();
                } else if self.chat.is_single_line() {
                    // Up in non-empty single-line: Browse history
                    if let Some(prev) = self.chat.input_history.up(&self.chat.input) {
                        self.chat.input = prev.to_string();
                        self.chat.cursor = self.chat.input.len();
                        self.chat.browsing_history = true;
                    }
                }
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Down: Scroll messages down
                    self.chat.scroll += 1;
                } else if self.chat.browsing_history {
                    // Down while browsing history: Move forward
                    if let Some(next) = self.chat.input_history.down() {
                        self.chat.input = next.to_string();
                        self.chat.cursor = self.chat.input.len();
                    }
                } else if !self.chat.is_single_line() {
                    // Down in multi-line: Move cursor down
                    self.chat.cursor_down();
                }
            }
            KeyCode::Esc => {
                self.chat.input.clear();
                self.chat.cursor = 0;
                self.chat.browsing_history = false;
                self.chat.input_history.reset_position();
            }
            KeyCode::PageUp => {
                // Page up: Scroll messages up by multiple lines
                self.chat.scroll = self.chat.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                // Page down: Scroll messages down by multiple lines
                self.chat.scroll += 10;
            }
            _ => {}
        }
    }

    /// Get byte position for cursor (handles UTF-8)
    fn cursor_byte_pos(&self) -> usize {
        self.chat.cursor.min(self.chat.input.len())
    }

    /// Insert newline at cursor
    fn insert_newline(&mut self) {
        self.chat.insert_newline();
    }

    /// Handle monitor view keys
    fn handle_monitor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.monitor.selected < self.monitor.job_count.saturating_sub(1) {
                    self.monitor.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.monitor.selected > 0 {
                    self.monitor.selected -= 1;
                }
            }
            KeyCode::Char('r') => {
                // TODO: Retry selected job
            }
            KeyCode::Char('c') => {
                // TODO: Cancel selected job
            }
            _ => {}
        }
    }

    /// Send user message
    async fn send_message(&mut self) {
        let content = std::mem::take(&mut self.chat.input);
        self.chat.cursor = 0;

        // Add to history
        self.chat.input_history.push(content.clone());
        self.chat.browsing_history = false;

        // Add user message
        self.chat.messages.push(Message::new(
            MessageRole::User,
            content.clone(),
        ));

        // Auto-scroll to bottom
        self.chat.scroll = 0;

        // Mark as awaiting response
        self.chat.awaiting_response = true;

        // Try to use Claude Code if available
        if let Some(ref provider) = self.llm {
            // Build LLM messages from our chat messages
            let llm_messages: Vec<super::llm::Message> = self
                .chat
                .messages
                .iter()
                .filter_map(|m| match m.role {
                    MessageRole::User => Some(super::llm::Message::user(&m.content)),
                    MessageRole::Assistant => Some(super::llm::Message::assistant(&m.content)),
                    _ => None,
                })
                .collect();

            // Get tool definitions
            let tool_defs = registry_to_definitions(&self.tools);

            // Stream response
            match provider.chat_stream(&llm_messages, &tool_defs, None).await {
                Ok(mut stream) => {
                    let mut response_text = String::new();

                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            StreamChunk::Text(text) => {
                                response_text.push_str(&text);
                            }
                            StreamChunk::ToolCall { name, arguments, .. } => {
                                // Execute MCP tool
                                response_text.push_str(&format!("\n[Calling {}...]\n", name));

                                match self.execute_tool(&name, arguments.clone()).await {
                                    Ok(result) => {
                                        if let Some(content) = result.content.first() {
                                            if let casparian_mcp::types::ToolContent::Text { text } = content {
                                                response_text.push_str(&format!("[Result: {}]\n",
                                                    if text.len() > 200 {
                                                        format!("{}...", &text[..200])
                                                    } else {
                                                        text.clone()
                                                    }
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        response_text.push_str(&format!("[Tool error: {}]\n", e));
                                    }
                                }
                            }
                            StreamChunk::Done { .. } => break,
                            StreamChunk::Error(e) => {
                                response_text.push_str(&format!("\n[Error: {}]\n", e));
                                break;
                            }
                            _ => {}
                        }
                    }

                    if response_text.is_empty() {
                        response_text = "(No response from Claude Code)".to_string();
                    }

                    self.chat.messages.push(Message::new(
                        MessageRole::Assistant,
                        response_text,
                    ));
                }
                Err(e) => {
                    self.chat.messages.push(Message::new(
                        MessageRole::System,
                        format!("LLM Error: {}", e),
                    ));
                }
            }
        } else {
            // No Claude Code available - show helpful message
            self.chat.messages.push(Message::new(
                MessageRole::System,
                "Claude Code not available. Install Claude Code (`npm install -g @anthropic-ai/claude-code`) \
                 to enable AI chat.\n\nFor now, you can use the MCP tools directly or try:\n  \
                 F2 - Monitor view\n  F3 - Help".to_string(),
            ));
        }

        self.chat.awaiting_response = false;
    }

    /// Execute a tool directly
    pub async fn execute_tool(&self, name: &str, args: Value) -> Result<ToolResult, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool '{}' not found", name))?;

        tool.execute(args).await.map_err(|e| e.to_string())
    }

    /// Periodic tick for updates
    pub async fn tick(&mut self) {
        // TODO: Poll job status, refresh metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        }
    }

    #[test]
    fn test_view_switching() {
        let mut app = App::new(test_args());

        assert!(matches!(app.view, View::Chat));

        // Simulate F2 press
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.view, View::Monitor));

        // Simulate F1 press
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.view, View::Chat));
    }

    #[test]
    fn test_chat_input() {
        let mut app = App::new(test_args());

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Type "hello"
            for c in "hello".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });

        assert_eq!(app.chat.input, "hello");
        assert_eq!(app.chat.cursor, 5);
    }

    #[test]
    fn test_chat_backspace() {
        let mut app = App::new(test_args());
        app.chat.input = "hello".into();
        app.chat.cursor = 5;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });

        assert_eq!(app.chat.input, "hell");
        assert_eq!(app.chat.cursor, 4);
    }

    #[test]
    fn test_multiline_input() {
        let mut app = App::new(test_args());

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Type "line1"
            for c in "line1".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
            // Shift+Enter for newline
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))
                .await;
            // Type "line2"
            for c in "line2".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });

        assert_eq!(app.chat.input, "line1\nline2");
        assert_eq!(app.chat.input_line_count(), 2);
    }

    #[test]
    fn test_input_history() {
        let mut history = InputHistory::default();

        // Add some entries
        history.push("first".into());
        history.push("second".into());
        history.push("third".into());

        // Browse up
        assert_eq!(history.up(""), Some("third"));
        assert_eq!(history.up(""), Some("second"));
        assert_eq!(history.up(""), Some("first"));
        assert_eq!(history.up(""), Some("first")); // At oldest

        // Browse down
        assert_eq!(history.down(), Some("second"));
        assert_eq!(history.down(), Some("third"));
        assert_eq!(history.down(), Some("")); // Back to draft
    }

    #[test]
    fn test_input_history_preserves_draft() {
        let mut history = InputHistory::default();
        history.push("old".into());

        // Start typing, then browse history
        let draft = "typing something";
        assert_eq!(history.up(draft), Some("old"));

        // Return to draft
        assert_eq!(history.down(), Some(draft));
    }

    #[test]
    fn test_cursor_line_col() {
        let mut state = ChatState::default();
        state.input = "line1\nline2\nline3".into();

        // Start of first line
        state.cursor = 0;
        assert_eq!(state.cursor_line_col(), (0, 0));

        // End of first line
        state.cursor = 5;
        assert_eq!(state.cursor_line_col(), (0, 5));

        // Start of second line
        state.cursor = 6;
        assert_eq!(state.cursor_line_col(), (1, 0));

        // Middle of second line
        state.cursor = 8;
        assert_eq!(state.cursor_line_col(), (1, 2));
    }

    #[tokio::test]
    async fn test_execute_tool() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.csv"), "id,name\n1,Alice").unwrap();

        let app = App::new(test_args());

        let result = app
            .execute_tool(
                "quick_scan",
                serde_json::json!({ "path": temp_dir.path() }),
            )
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.is_error);
    }

    #[test]
    fn test_ctrl_c_quits() {
        let mut app = App::new(test_args());
        assert!(app.running);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
                .await;
        });

        assert!(!app.running);
    }

    #[test]
    fn test_esc_clears_input() {
        let mut app = App::new(test_args());
        app.chat.input = "some text".into();
        app.chat.cursor = 9;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });

        assert_eq!(app.chat.input, "");
        assert_eq!(app.chat.cursor, 0);
    }
}

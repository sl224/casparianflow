//! Application state for the TUI

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::{ToolResult, WorkflowMetadata};
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use super::llm::claude_code::ClaudeCodeProvider;
use super::llm::{registry_to_definitions, LlmProvider, StreamChunk};
use super::TuiArgs;

/// Current TUI mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiMode {
    #[default]
    Home,     // Home hub with 4 cards
    Discover, // File discovery and tagging
    Process,  // Parser execution
    Inspect,  // Output inspection
    Jobs,     // Job queue management
}

/// Statistics shown on home hub cards
#[derive(Debug, Clone, Default)]
pub struct HomeStats {
    pub file_count: usize,
    pub source_count: usize,
    pub running_jobs: usize,
    pub pending_jobs: usize,
    pub failed_jobs: usize,
    pub completed_jobs: usize,
    pub parser_count: usize,
    pub paused_parsers: usize,
}

/// State for the home hub screen
#[derive(Debug, Clone, Default)]
pub struct HomeState {
    /// Currently selected card index (0-3)
    pub selected_card: usize,
    /// Statistics displayed on cards
    pub stats: HomeStats,
}

/// Result from a pending Claude response
pub enum PendingResponse {
    /// Response text received with tool calls info
    Text {
        content: String,
        tools_used: Vec<String>,
    },
    /// Error occurred
    Error(String),
}

/// Maximum number of messages to keep in input history
const MAX_INPUT_HISTORY: usize = 50;

/// Format tool call for display
fn format_tool_call(name: &str, arguments: &Value) -> String {
    let mut result = format!("\n[Tool: {}]\n", name);

    // Format arguments in a readable way
    if let Value::Object(obj) = arguments {
        for (key, value) in obj {
            let value_str = match value {
                Value::String(s) => {
                    // Truncate long strings
                    if s.len() > 100 {
                        format!("\"{}...\"", &s[..97])
                    } else {
                        format!("\"{}\"", s)
                    }
                }
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                Value::Array(arr) => {
                    if arr.len() <= 3 {
                        format!("{:?}", arr)
                    } else {
                        format!("[{} items]", arr.len())
                    }
                }
                Value::Object(_) => "{...}".to_string(),
            };
            result.push_str(&format!("  {}: {}\n", key, value_str));
        }
    }

    result
}

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
    #[allow(dead_code)]
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
    /// Recent tools used (for display in context pane)
    pub recent_tools: Vec<String>,
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
            recent_tools: Vec::new(),
        }
    }
}

impl ChatState {
    /// Estimate total line count for scroll bounds
    /// This is a rough estimate used to prevent unbounded scroll growth
    pub fn estimated_total_lines(&self) -> usize {
        // Estimate ~5 lines per message on average (header + wrapped content)
        // This is intentionally generous to allow scrolling
        self.messages.iter().map(|m| {
            // 2 lines for header + at least 1 for content, plus estimate based on content length
            2 + (m.content.len() / 50).max(1)
        }).sum()
    }

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
    /// Current TUI mode/screen
    pub mode: TuiMode,
    /// Current view (within mode - for Discover mode's chat/monitor/help)
    pub view: View,
    /// Home hub state
    pub home: HomeState,
    /// Chat state
    pub chat: ChatState,
    /// Monitor state
    pub monitor: MonitorState,
    /// Tool registry for executing MCP tools
    pub tools: ToolRegistry,
    /// LLM provider (Claude Code if available)
    pub llm: Option<ClaudeCodeProvider>,
    /// Injected LLM provider (for testing with mock providers)
    #[cfg(test)]
    pub llm_provider: Option<std::sync::Arc<dyn super::llm::LlmProvider + Send + Sync>>,
    /// Configuration
    #[allow(dead_code)]
    pub config: TuiArgs,
    /// Last error message
    #[allow(dead_code)]
    pub error: Option<String>,
    /// Pending response from Claude (non-blocking)
    pending_response: Option<mpsc::Receiver<PendingResponse>>,
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
            mode: TuiMode::Home,
            view: View::Chat,
            home: HomeState::default(),
            chat: ChatState::default(),
            monitor: MonitorState::default(),
            tools: create_default_registry(),
            llm,
            #[cfg(test)]
            llm_provider: None,
            config: args,
            error: None,
            pending_response: None,
        }
    }

    /// Create app with custom tool registry (for testing)
    #[cfg(test)]
    pub fn new_with_registry(registry: ToolRegistry, args: TuiArgs) -> Self {
        Self {
            running: true,
            mode: TuiMode::Home,
            view: View::Chat,
            home: HomeState::default(),
            chat: ChatState::default(),
            monitor: MonitorState::default(),
            tools: registry,
            llm: None,
            llm_provider: None,
            config: args,
            error: None,
            pending_response: None,
        }
    }

    /// Create app with injected LLM provider (for testing with mock providers)
    #[cfg(test)]
    pub fn new_with_provider(
        args: TuiArgs,
        provider: std::sync::Arc<dyn super::llm::LlmProvider + Send + Sync>,
    ) -> Self {
        Self {
            running: true,
            mode: TuiMode::Home,
            view: View::Chat,
            home: HomeState::default(),
            chat: ChatState::default(),
            monitor: MonitorState::default(),
            tools: create_default_registry(),
            llm: None,
            llm_provider: Some(provider),
            config: args,
            error: None,
            pending_response: None,
        }
    }

    /// Handle key event
    pub async fn handle_key(&mut self, key: KeyEvent) {
        // Global keys - always active
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
                return;
            }
            // Alt+D: Switch to Discover mode
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.mode = TuiMode::Discover;
                return;
            }
            // Alt+P: Switch to Process mode
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.mode = TuiMode::Process;
                return;
            }
            // Alt+I: Switch to Inspect mode
            KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.mode = TuiMode::Inspect;
                return;
            }
            // Alt+J: Switch to Jobs mode
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.mode = TuiMode::Jobs;
                return;
            }
            // Alt+H: Return to Home
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.mode = TuiMode::Home;
                return;
            }
            // Esc: Return to Home from any mode (except Home itself)
            KeyCode::Esc if self.mode != TuiMode::Home => {
                self.mode = TuiMode::Home;
                return;
            }
            _ => {}
        }

        // Mode-specific keys
        match self.mode {
            TuiMode::Home => self.handle_home_key(key),
            TuiMode::Discover => {
                // F-keys for view switching within Discover mode
                match key.code {
                    KeyCode::F(1) => self.view = View::Chat,
                    KeyCode::F(2) => self.view = View::Monitor,
                    KeyCode::F(3) => self.view = View::Help,
                    KeyCode::Char('q') if self.view == View::Help => self.view = View::Chat,
                    _ => match self.view {
                        View::Chat => self.handle_chat_key(key).await,
                        View::Monitor => self.handle_monitor_key(key),
                        View::Help => {}
                    },
                }
            }
            TuiMode::Process | TuiMode::Inspect | TuiMode::Jobs => {
                // Placeholder modes - just show placeholder screen
                // No additional key handling for now
            }
        }
    }

    /// Handle home hub keys
    fn handle_home_key(&mut self, key: KeyEvent) {
        match key.code {
            // Arrow key navigation
            KeyCode::Left | KeyCode::Char('h') => {
                if self.home.selected_card % 2 != 0 {
                    self.home.selected_card -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.home.selected_card % 2 == 0 && self.home.selected_card < 3 {
                    self.home.selected_card += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.home.selected_card >= 2 {
                    self.home.selected_card -= 2;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.home.selected_card < 2 {
                    self.home.selected_card += 2;
                }
            }
            // Enter: Navigate to selected mode
            KeyCode::Enter => {
                self.mode = match self.home.selected_card {
                    0 => TuiMode::Discover,
                    1 => TuiMode::Process,
                    2 => TuiMode::Inspect,
                    3 => TuiMode::Jobs,
                    _ => TuiMode::Home,
                };
            }
            // Number keys for quick access
            KeyCode::Char('1') => {
                self.mode = TuiMode::Discover;
            }
            KeyCode::Char('2') => {
                self.mode = TuiMode::Process;
            }
            KeyCode::Char('3') => {
                self.mode = TuiMode::Inspect;
            }
            KeyCode::Char('4') => {
                self.mode = TuiMode::Jobs;
            }
            _ => {}
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
                    // Ctrl+Down: Scroll messages down (with upper bound)
                    let max_scroll = self.chat.estimated_total_lines();
                    if self.chat.scroll < max_scroll {
                        self.chat.scroll += 1;
                    }
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
                // Page down: Scroll messages down by multiple lines (with upper bound)
                let max_scroll = self.chat.estimated_total_lines();
                self.chat.scroll = (self.chat.scroll + 10).min(max_scroll);
            }
            _ => {}
        }
    }

    /// Get byte position for cursor (handles UTF-8)
    fn cursor_byte_pos(&self) -> usize {
        self.chat.cursor.min(self.chat.input.len())
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

    /// Send user message (non-blocking - spawns Claude in background)
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

        // Check for injected provider first (test mode)
        #[cfg(test)]
        if let Some(ref provider) = self.llm_provider {
            let (tx, rx) = mpsc::channel::<PendingResponse>(1);
            self.pending_response = Some(rx);

            let provider = provider.clone();
            tokio::spawn(async move {
                Self::run_llm_request(provider, llm_messages, tool_defs, tx).await;
            });

            self.chat.messages.push(Message::new(
                MessageRole::Assistant,
                "Thinking...".to_string(),
            ));
            return;
        }

        // Try to use Claude Code if available
        if self.llm.is_some() {
            // Create channel for response
            let (tx, rx) = mpsc::channel::<PendingResponse>(1);
            self.pending_response = Some(rx);

            // Spawn background task to get Claude response
            // Note: We create a new provider instance since ClaudeCodeProvider is cheap
            let provider = ClaudeCodeProvider::new()
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
                .max_turns(5);

            tokio::spawn(async move {
                match provider.chat_stream(&llm_messages, &tool_defs, None).await {
                    Ok(mut stream) => {
                        let mut response_text = String::new();
                        let mut tools_used = Vec::new();

                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                StreamChunk::Text(text) => {
                                    response_text.push_str(&text);
                                }
                                StreamChunk::ToolCall { name, arguments } => {
                                    // Track tool name for context pane
                                    tools_used.push(name.clone());
                                    // Show tool name and arguments
                                    response_text.push_str(&format_tool_call(&name, &arguments));
                                }
                                StreamChunk::Done { .. } => {
                                    break;
                                }
                                StreamChunk::Error(e) => {
                                    response_text.push_str(&format!("\n[Error: {}]\n", e));
                                    break;
                                }
                            }
                        }

                        if response_text.is_empty() {
                            let _ = tx.send(PendingResponse::Error(
                                "(No response from Claude Code)".to_string()
                            )).await;
                        } else {
                            let _ = tx.send(PendingResponse::Text {
                                content: response_text,
                                tools_used,
                            }).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(PendingResponse::Error(format!("LLM Error: {}", e))).await;
                    }
                }
            });

            // Add a "thinking" message that will be replaced
            self.chat.messages.push(Message::new(
                MessageRole::Assistant,
                "Thinking...".to_string(),
            ));
        } else {
            // No Claude Code available - show helpful message
            self.chat.messages.push(Message::new(
                MessageRole::System,
                "Claude Code not available. Install Claude Code (`npm install -g @anthropic-ai/claude-code`) \
                 to enable AI chat.\n\nFor now, you can use the MCP tools directly or try:\n  \
                 F2 - Monitor view\n  F3 - Help".to_string(),
            ));
            self.chat.awaiting_response = false;
        }
    }

    /// Helper to run LLM request in background (used by tests with injected provider)
    #[cfg(test)]
    async fn run_llm_request(
        provider: std::sync::Arc<dyn super::llm::LlmProvider + Send + Sync>,
        llm_messages: Vec<super::llm::Message>,
        tool_defs: Vec<super::llm::ToolDefinition>,
        tx: mpsc::Sender<PendingResponse>,
    ) {
        match provider.chat_stream(&llm_messages, &tool_defs, None).await {
            Ok(mut stream) => {
                let mut response_text = String::new();
                let mut tools_used = Vec::new();

                while let Some(chunk) = stream.next().await {
                    match chunk {
                        StreamChunk::Text(text) => {
                            response_text.push_str(&text);
                        }
                        StreamChunk::ToolCall { name, arguments } => {
                            tools_used.push(name.clone());
                            response_text.push_str(&format_tool_call(&name, &arguments));
                        }
                        StreamChunk::Done { .. } => {
                            break;
                        }
                        StreamChunk::Error(e) => {
                            response_text.push_str(&format!("\n[Error: {}]\n", e));
                            break;
                        }
                    }
                }

                if response_text.is_empty() {
                    let _ = tx
                        .send(PendingResponse::Error(
                            "(No response from LLM)".to_string(),
                        ))
                        .await;
                } else {
                    let _ = tx.send(PendingResponse::Text {
                        content: response_text,
                        tools_used,
                    }).await;
                }
            }
            Err(e) => {
                let _ = tx
                    .send(PendingResponse::Error(format!("LLM Error: {}", e)))
                    .await;
            }
        }
    }

    /// Execute a tool directly
    #[allow(dead_code)]
    pub async fn execute_tool(&self, name: &str, args: Value) -> Result<ToolResult, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool '{}' not found", name))?;

        tool.execute(args).await.map_err(|e| e.to_string())
    }

    /// Periodic tick for updates
    pub async fn tick(&mut self) {
        // Poll for pending Claude response
        if let Some(ref mut rx) = self.pending_response {
            // Try to receive without blocking
            match rx.try_recv() {
                Ok(response) => {
                    // Replace the "Thinking..." message with actual response
                    // Note: Use starts_with() because animation changes dots count
                    if let Some(last_msg) = self.chat.messages.last_mut() {
                        if last_msg.role == MessageRole::Assistant && last_msg.content.starts_with("Thinking") {
                            match response {
                                PendingResponse::Text { content, tools_used } => {
                                    last_msg.content = content;
                                    // Update recent tools for context pane
                                    if !tools_used.is_empty() {
                                        self.chat.recent_tools = tools_used;
                                    }
                                }
                                PendingResponse::Error(err) => {
                                    last_msg.content = err;
                                    last_msg.role = MessageRole::System;
                                }
                            }
                        }
                    }
                    self.chat.awaiting_response = false;
                    self.pending_response = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still waiting, update thinking animation
                    if let Some(last_msg) = self.chat.messages.last_mut() {
                        if last_msg.role == MessageRole::Assistant && last_msg.content.starts_with("Thinking") {
                            // Cycle through animation: Thinking... -> Thinking.... -> Thinking.....
                            let dots = last_msg.content.matches('.').count();
                            if dots >= 5 {
                                last_msg.content = "Thinking.".to_string();
                            } else {
                                last_msg.content.push('.');
                            }
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed without response
                    if let Some(last_msg) = self.chat.messages.last_mut() {
                        if last_msg.role == MessageRole::Assistant && last_msg.content.starts_with("Thinking") {
                            last_msg.content = "(Claude process ended unexpectedly)".to_string();
                            last_msg.role = MessageRole::System;
                        }
                    }
                    self.chat.awaiting_response = false;
                    self.pending_response = None;
                }
            }
        }

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
        // Must be in Discover mode for view switching to work
        app.mode = TuiMode::Discover;

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
    fn test_mode_switching() {
        let mut app = App::new(test_args());
        assert!(matches!(app.mode, TuiMode::Home));

        // Alt+D should switch to Discover
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Discover));

        // Alt+P should switch to Process
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Process));

        // Esc should return to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Home));
    }

    #[test]
    fn test_home_card_navigation() {
        let mut app = App::new(test_args());
        assert_eq!(app.home.selected_card, 0);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Right arrow should move to card 1
            app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.home.selected_card, 1);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Down arrow should move to card 3
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.home.selected_card, 3);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Enter should navigate to Jobs mode (card 3)
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Jobs));
    }

    #[test]
    fn test_chat_input() {
        let mut app = App::new(test_args());
        // Must be in Discover mode for chat input to work
        app.mode = TuiMode::Discover;

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
        // Must be in Discover mode for chat input to work
        app.mode = TuiMode::Discover;
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
        // Must be in Discover mode for chat input to work
        app.mode = TuiMode::Discover;

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
    fn test_esc_returns_to_home() {
        let mut app = App::new(test_args());
        // Start in Discover mode
        app.mode = TuiMode::Discover;
        app.chat.input = "some text".into();
        app.chat.cursor = 9;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });

        // Esc returns to Home mode from Discover
        assert!(matches!(app.mode, TuiMode::Home));
    }

    #[tokio::test]
    async fn test_pending_response_polling() {
        // Test the non-blocking response mechanism
        let mut app = App::new(test_args());

        // Simulate adding a "Thinking..." message and pending response
        app.chat.messages.push(Message::new(
            MessageRole::Assistant,
            "Thinking...".to_string(),
        ));
        app.chat.awaiting_response = true;

        // Create channel and send a response
        let (tx, rx) = mpsc::channel::<PendingResponse>(1);
        app.pending_response = Some(rx);

        // Send response in background
        tokio::spawn(async move {
            tx.send(PendingResponse::Text {
                content: "Hello from Claude!".to_string(),
                tools_used: vec![],
            })
                .await
                .unwrap();
        });

        // Give time for the message to be sent
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Call tick to poll the response
        app.tick().await;

        // Verify the message was updated
        let last_msg = app.chat.messages.last().unwrap();
        assert_eq!(last_msg.content, "Hello from Claude!");
        assert!(!app.chat.awaiting_response);
        assert!(app.pending_response.is_none());
    }

    #[tokio::test]
    async fn test_pending_response_thinking_animation() {
        // Test the thinking animation when no response yet
        let mut app = App::new(test_args());

        // Simulate adding a "Thinking..." message
        app.chat.messages.push(Message::new(
            MessageRole::Assistant,
            "Thinking...".to_string(),
        ));
        app.chat.awaiting_response = true;

        // Create channel but don't send anything
        let (_tx, rx) = mpsc::channel::<PendingResponse>(1);
        app.pending_response = Some(rx);

        // Call tick - should animate the dots
        app.tick().await;

        // Verify the message got more dots
        let last_msg = app.chat.messages.last().unwrap();
        assert!(last_msg.content.starts_with("Thinking"));
        assert!(last_msg.content.len() > "Thinking...".len());
    }

    /// CRITICAL TEST: Catches the animation bug where response wasn't applied
    /// because the check used == "Thinking..." instead of starts_with()
    #[tokio::test]
    async fn test_response_replaces_animated_thinking() {
        let mut app = App::new(test_args());

        // Setup initial "Thinking..." message
        app.chat.messages.push(Message::new(
            MessageRole::Assistant,
            "Thinking...".to_string(),
        ));
        app.chat.awaiting_response = true;

        // Create channel but DON'T send response yet
        let (tx, rx) = mpsc::channel::<PendingResponse>(1);
        app.pending_response = Some(rx);

        // Run multiple ticks to animate (no response yet)
        for i in 0..4 {
            app.tick().await;
            let content = &app.chat.messages.last().unwrap().content;
            println!("Tick {}: {}", i + 1, content);
            assert!(content.starts_with("Thinking"), "Should still be thinking");
        }

        // Verify animation actually ran (not still exactly "Thinking...")
        let animated_content = app.chat.messages.last().unwrap().content.clone();
        assert_ne!(
            animated_content, "Thinking...",
            "Animation should have changed the dots"
        );

        // NOW send the response
        tx.send(PendingResponse::Text {
            content: "Response from Claude".to_string(),
            tools_used: vec![],
        })
            .await
            .unwrap();

        // Run tick to process response
        app.tick().await;

        // CRITICAL: This is where the bug manifested
        // With == "Thinking..." check, this would fail because animation changed dots
        let final_msg = app.chat.messages.last().unwrap();
        assert_eq!(
            final_msg.content, "Response from Claude",
            "Response should replace animated message - BUG if this fails!"
        );
        assert!(!app.chat.awaiting_response);
        assert!(app.pending_response.is_none());
    }

    /// TRUE END-TO-END TEST: Full flow from user input to Claude response
    ///
    /// This test catches bugs that mocked tests miss by:
    /// 1. Actually calling send_message() (not injecting mock channel)
    /// 2. Using real ClaudeCodeProvider.chat_stream()
    /// 3. Polling tick() like the real TUI event loop
    /// 4. Verifying response replaces "Thinking..."
    ///
    /// This is the test that would have caught the "no response" bug.
    #[tokio::test]
    async fn test_full_chat_flow_with_real_claude() {
        // Skip if Claude CLI not available
        if !ClaudeCodeProvider::is_available() {
            println!("Skipping test_full_chat_flow_with_real_claude: claude CLI not installed");
            return;
        }

        let mut app = App::new(test_args());
        // Must be in Discover mode for chat to work
        app.mode = TuiMode::Discover;

        // Verify Claude Code is available in the app
        if app.llm.is_none() {
            println!("Skipping: app.llm is None despite Claude being available");
            return;
        }

        // Step 1: Type a simple message
        app.chat.input = "say hello".to_string();
        app.chat.cursor = app.chat.input.len();

        // Step 2: Press Enter to send
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Verify "Thinking..." was added
        assert!(app.chat.awaiting_response, "Should be awaiting response");
        assert!(
            app.chat
                .messages
                .last()
                .map(|m| m.content.starts_with("Thinking"))
                .unwrap_or(false),
            "Should have Thinking... message"
        );
        assert!(
            app.pending_response.is_some(),
            "Should have pending_response channel"
        );

        // Step 3: Poll tick() until response arrives (max 60 seconds)
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(60);
        let mut got_response = false;
        let mut saw_animation = false;
        let mut last_content = String::new();

        while start.elapsed() < timeout {
            app.tick().await;

            let current_content = app
                .chat
                .messages
                .last()
                .map(|m| m.content.clone())
                .unwrap_or_default();

            // Track if animation ran
            if current_content.starts_with("Thinking") && current_content != last_content {
                println!("Animation frame: {}", current_content);
                if current_content != "Thinking..." {
                    saw_animation = true;
                }
            }

            // Check if we got a real response
            if !current_content.starts_with("Thinking")
                && !current_content.is_empty()
                && app.chat.messages.len() >= 2
            {
                // Not "Thinking..." anymore = got response!
                got_response = true;
                println!("Got response: {}", current_content.chars().take(100).collect::<String>());
                break;
            }

            last_content = current_content;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Step 4: Assert the test results
        let final_msg = app.chat.messages.last().unwrap();

        // This is the critical check that would have caught the bug
        assert!(
            !final_msg.content.starts_with("Thinking"),
            "FAILURE: Response never arrived! Content is still: '{}'.\n\
             This means the spawned task didn't send a response through the channel.\n\
             Check: 1) Is chat_stream() returning? 2) Is the channel send working?",
            final_msg.content
        );

        assert!(got_response, "Should have received response from Claude");
        assert!(!app.chat.awaiting_response, "Should no longer be awaiting");
        assert!(app.pending_response.is_none(), "Channel should be consumed");

        println!(
            "TRUE E2E TEST PASSED: Animation ran: {}, Final content length: {}",
            saw_animation,
            final_msg.content.len()
        );
    }

    /// Test that channel disconnection is handled gracefully
    #[tokio::test]
    async fn test_channel_disconnection_handling() {
        let mut app = App::new(test_args());

        // Setup "Thinking..." message
        app.chat.messages.push(Message::new(
            MessageRole::Assistant,
            "Thinking...".to_string(),
        ));
        app.chat.awaiting_response = true;

        // Create channel and immediately drop the sender
        let (tx, rx) = mpsc::channel::<PendingResponse>(1);
        app.pending_response = Some(rx);
        drop(tx); // Simulate task ending without sending

        // Call tick - should handle disconnection
        app.tick().await;

        // Should show error message
        let last_msg = app.chat.messages.last().unwrap();
        assert!(
            last_msg.content.contains("unexpectedly") || last_msg.role == MessageRole::System,
            "Should show disconnection error. Got: {}",
            last_msg.content
        );
        assert!(!app.chat.awaiting_response);
    }
}

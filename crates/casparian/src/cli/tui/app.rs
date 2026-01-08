//! Application state for the TUI

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::ToolResult;
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
    pub parser_count: usize,
    pub paused_parsers: usize,
}

/// State for data inspection mode
#[derive(Debug, Clone, Default)]
pub struct InspectState {
    /// List of output tables
    pub tables: Vec<TableInfo>,
    /// Currently selected table index
    pub selected_table: usize,
    /// SQL query input
    pub query_input: String,
    /// Query result preview
    pub query_result: Option<String>,
    /// Is query input focused
    pub query_focused: bool,
}

/// Information about an output table
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub row_count: u64,
    pub column_count: usize,
    pub size_bytes: u64,
    pub last_updated: DateTime<Local>,
}

/// State for job queue mode
#[derive(Debug, Clone, Default)]
pub struct JobsState {
    /// List of jobs
    pub jobs: Vec<JobInfo>,
    /// Currently selected job index (into filtered list)
    pub selected_index: usize,
    /// Filter: show only specific status
    pub status_filter: Option<JobStatus>,
}

impl JobsState {
    /// Get filtered jobs based on current status filter
    pub fn filtered_jobs(&self) -> Vec<&JobInfo> {
        self.jobs.iter()
            .filter(|j| match self.status_filter {
                Some(status) => j.status == status,
                None => true,
            })
            .collect()
    }

    /// Clamp selected_index to valid range for filtered list
    pub fn clamp_selection(&mut self) {
        let count = self.filtered_jobs().len();
        if count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= count {
            self.selected_index = count - 1;
        }
    }

    /// Set status filter and clamp selection
    pub fn set_filter(&mut self, filter: Option<JobStatus>) {
        self.status_filter = filter;
        self.clamp_selection();
    }
}

/// Information about a job
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: i64,
    pub file_path: String,
    pub parser_name: String,
    pub status: JobStatus,
    pub retry_count: i32,
    pub error_message: Option<String>,
    pub created_at: DateTime<Local>,
}

/// Job status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl JobStatus {
    /// Get display symbol for this status
    pub fn symbol(&self) -> &'static str {
        match self {
            JobStatus::Pending => "⏳",
            JobStatus::Running => "▶",
            JobStatus::Completed => "✓",
            JobStatus::Failed => "✗",
        }
    }

    /// Get display text for this status
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "Pending",
            JobStatus::Running => "Running",
            JobStatus::Completed => "Completed",
            JobStatus::Failed => "Failed",
        }
    }
}

/// State for the home hub screen
#[derive(Debug, Clone, Default)]
pub struct HomeState {
    /// Currently selected card index (0-3)
    pub selected_card: usize,
    /// Statistics displayed on cards
    pub stats: HomeStats,
}

/// Focus areas for input handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppFocus {
    #[default]
    Main,
    Chat,
}

/// Focus areas within Discover mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoverFocus {
    #[default]
    Files,
    Sources,
    Tags,  // Renamed from Rules - users browse by tag category
}

/// Source information for Discover mode sidebar
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    #[allow(dead_code)] // Will be used for displaying full path in details view
    pub path: String,
    pub file_count: usize,
}

/// Tag with file count (for Tags dropdown in sidebar)
/// Tags are derived from files, showing what categories exist
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,        // Tag name, "All files", or "untagged"
    pub count: usize,        // Number of files with this tag
    pub is_special: bool,    // True for "All files" and "untagged"
}

/// Tagging rule (for Rules Manager dialog)
/// Rules are the mechanism that applies tags to files
#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: i64,
    pub pattern: String,
    pub tag: String,
    #[allow(dead_code)] // Used in Rules Manager sorting
    pub priority: i32,
    pub enabled: bool,
}

/// State for the Discover mode (File Explorer)
#[derive(Debug, Clone, Default)]
pub struct DiscoverState {
    pub files: Vec<FileInfo>,
    pub selected: usize,
    pub filter: String,
    pub is_filtering: bool,
    pub preview_open: bool,
    /// Path input for scan dialog
    pub scan_path_input: String,
    /// Whether the scan path input is active
    pub is_entering_path: bool,
    /// Error message from last scan attempt
    pub scan_error: Option<String>,
    /// Whether data has been loaded from Scout DB
    pub data_loaded: bool,
    /// Whether the tag dialog is open
    pub is_tagging: bool,
    /// Tag input for new tag
    pub tag_input: String,
    /// Available tags from DB for autocomplete
    pub available_tags: Vec<String>,
    /// Status message (success/error) for user feedback
    pub status_message: Option<(String, bool)>, // (message, is_error)
    /// Whether the create source dialog is open
    pub is_creating_source: bool,
    /// Source name input
    pub source_name_input: String,
    /// Directory path for the source being created
    pub pending_source_path: Option<String>,
    /// Whether bulk tag dialog is open
    pub is_bulk_tagging: bool,
    /// Tag input for bulk tagging
    pub bulk_tag_input: String,
    /// Whether to save bulk tag as a rule
    pub bulk_tag_save_as_rule: bool,

    // --- Sidebar state ---
    /// Current focus within Discover mode
    pub focus: DiscoverFocus,
    /// Available sources from DB
    pub sources: Vec<SourceInfo>,
    /// Currently selected source index
    pub selected_source: usize,
    /// Whether sources have been loaded
    pub sources_loaded: bool,

    // --- Tags dropdown (sidebar panel 2) ---
    /// Tags derived from files (for dropdown navigation)
    pub tags: Vec<TagInfo>,
    /// Currently selected tag index (None = "All files")
    pub selected_tag: Option<usize>,
    /// Whether tags dropdown is expanded
    pub tags_dropdown_open: bool,
    /// Filter text for tags dropdown
    pub tags_filter: String,
    /// Temporary tag index while navigating dropdown (for preview)
    pub preview_tag: Option<usize>,

    // --- Sources dropdown state ---
    /// Whether sources dropdown is expanded
    pub sources_dropdown_open: bool,
    /// Filter text for sources dropdown
    pub sources_filter: String,
    /// Temporary source index while navigating dropdown (for preview)
    pub preview_source: Option<usize>,

    // --- Rules Manager dialog ---
    /// Whether Rules Manager dialog is open
    pub rules_manager_open: bool,
    /// Tagging rules for the selected source (for Rules Manager)
    pub rules: Vec<RuleInfo>,
    /// Currently selected rule in Rules Manager
    pub selected_rule: usize,

    // --- Rule creation/edit dialog ---
    /// Whether the create/edit rule dialog is open
    pub is_creating_rule: bool,
    /// Tag input for new/edited rule
    pub rule_tag_input: String,
    /// Pattern input for new/edited rule
    pub rule_pattern_input: String,
    /// Rule being edited (None = creating new)
    pub editing_rule_id: Option<i64>,
}

/// File information for Discover mode
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    /// Relative path from source root (for display)
    pub rel_path: String,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub is_dir: bool,
    pub tags: Vec<String>,
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

/// Main application state
pub struct App {
    /// Whether app is running
    pub running: bool,
    /// Current TUI mode/screen
    pub mode: TuiMode,
    /// Whether the AI chat sidebar is visible
    pub show_chat_sidebar: bool,
    /// Current input focus (Main vs Chat)
    pub focus: AppFocus,
    /// Home hub state
    pub home: HomeState,
    /// Discover mode state
    pub discover: DiscoverState,
    /// Chat state
    pub chat: ChatState,
    /// Inspect mode state
    pub inspect: InspectState,
    /// Jobs mode state
    pub jobs_state: JobsState,
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
            show_chat_sidebar: false,
            focus: AppFocus::Main,
            home: HomeState::default(),
            discover: DiscoverState::default(),
            chat: ChatState::default(),
            inspect: InspectState::default(),
            jobs_state: JobsState::default(),
            tools: create_default_registry(),
            llm,
            #[cfg(test)]
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
            show_chat_sidebar: false,
            focus: AppFocus::Main,
            home: HomeState::default(),
            discover: DiscoverState::default(),
            chat: ChatState::default(),
            inspect: InspectState::default(),
            jobs_state: JobsState::default(),
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
            // Alt+A: Toggle AI Chat Sidebar
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.show_chat_sidebar = !self.show_chat_sidebar;
                // If opening, focus chat? Maybe not by default, user can Tab
                // If closing and focus was Chat, move back to Main
                if !self.show_chat_sidebar && self.focus == AppFocus::Chat {
                    self.focus = AppFocus::Main;
                }
                return;
            }
            // Tab: Cycle Focus (if sidebar is open)
            KeyCode::Tab => {
                if self.show_chat_sidebar {
                    self.focus = match self.focus {
                        AppFocus::Main => AppFocus::Chat,
                        AppFocus::Chat => AppFocus::Main,
                    };
                    return;
                }
            }
            // Esc: Return to Home from any mode (except Home itself)
            // But NOT if we're in a focused state (e.g., query input in Inspect mode, or Chat focus)
            KeyCode::Esc if self.mode != TuiMode::Home => {
                // If Chat is focused, Esc moves focus back to Main first
                if self.focus == AppFocus::Chat {
                    self.focus = AppFocus::Main;
                    return;
                }
                // Skip global Esc handling if mode-specific handler should handle it
                // Discover mode has layered Esc: dialog -> filter -> sidebar -> Home
                let discover_needs_local_esc = self.mode == TuiMode::Discover && (
                    self.discover.is_filtering ||
                    self.discover.is_entering_path ||
                    self.discover.is_tagging ||
                    self.discover.is_creating_source ||
                    self.discover.is_bulk_tagging ||
                    self.discover.is_creating_rule ||
                    !self.discover.filter.is_empty() ||
                    self.discover.focus != DiscoverFocus::Files
                );
                let inspect_has_dialog = self.mode == TuiMode::Inspect && self.inspect.query_focused;

                if !discover_needs_local_esc && !inspect_has_dialog {
                    self.mode = TuiMode::Home;
                    return;
                }
            }
            _ => {}
        }

        // If Focus is Chat, route keys to Chat handler
        if self.focus == AppFocus::Chat {
            self.handle_chat_key(key).await;
            return;
        }

        // Mode-specific keys (Main Focus)
        match self.mode {
            TuiMode::Home => self.handle_home_key(key),
            TuiMode::Discover => self.handle_discover_key(key),
            TuiMode::Process => {
                // Placeholder mode - just show placeholder screen
                // No additional key handling for now
            }
            TuiMode::Inspect => self.handle_inspect_key(key),
            TuiMode::Jobs => self.handle_jobs_key(key),
        }
    }

    /// Handle Discover mode keys
    fn handle_discover_key(&mut self, key: KeyEvent) {
        // Clear status message on any key press
        if self.discover.status_message.is_some() && key.code != KeyCode::Esc {
            self.discover.status_message = None;
        }

        // If entering scan path, handle text input
        if self.discover.is_entering_path {
            match key.code {
                KeyCode::Enter => {
                    // Execute scan with the entered path
                    let path = self.discover.scan_path_input.clone();
                    self.discover.is_entering_path = false;
                    if !path.is_empty() {
                        self.scan_directory(&path);
                    }
                }
                KeyCode::Esc => {
                    self.discover.is_entering_path = false;
                    self.discover.scan_path_input.clear();
                    self.discover.scan_error = None;
                }
                KeyCode::Char(c) => {
                    self.discover.scan_path_input.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.scan_path_input.pop();
                }
                _ => {}
            }
            return;
        }

        // If creating source, handle source name input
        if self.discover.is_creating_source {
            match key.code {
                KeyCode::Enter => {
                    // Create source with the entered name
                    let name = self.discover.source_name_input.trim().to_string();
                    if !name.is_empty() {
                        if let Some(path) = self.discover.pending_source_path.take() {
                            self.create_source(&path, &name);
                        }
                    }
                    self.discover.is_creating_source = false;
                    self.discover.source_name_input.clear();
                }
                KeyCode::Esc => {
                    self.discover.is_creating_source = false;
                    self.discover.source_name_input.clear();
                    self.discover.pending_source_path = None;
                }
                KeyCode::Char(c) => {
                    self.discover.source_name_input.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.source_name_input.pop();
                }
                _ => {}
            }
            return;
        }

        // If bulk tagging, handle bulk tag input
        if self.discover.is_bulk_tagging {
            match key.code {
                KeyCode::Enter => {
                    // Apply tag to all filtered files
                    let tag = self.discover.bulk_tag_input.trim().to_string();
                    if !tag.is_empty() {
                        let file_paths: Vec<String> = self.filtered_files()
                            .iter()
                            .map(|f| f.path.clone())
                            .collect();
                        let count = file_paths.len();

                        for path in file_paths {
                            self.apply_tag_to_file(&path, &tag);
                        }

                        // Show result (overwrite the per-file messages)
                        let rule_msg = if self.discover.bulk_tag_save_as_rule {
                            " (rule saved)"
                        } else {
                            ""
                        };
                        self.discover.status_message = Some((
                            format!("Tagged {} files with '{}'{}", count, tag, rule_msg),
                            false,
                        ));
                    }
                    self.discover.is_bulk_tagging = false;
                    self.discover.bulk_tag_input.clear();
                    self.discover.bulk_tag_save_as_rule = false;
                }
                KeyCode::Esc => {
                    self.discover.is_bulk_tagging = false;
                    self.discover.bulk_tag_input.clear();
                    self.discover.bulk_tag_save_as_rule = false;
                }
                KeyCode::Char(' ') => {
                    // Space toggles "save as rule" option
                    self.discover.bulk_tag_save_as_rule = !self.discover.bulk_tag_save_as_rule;
                }
                KeyCode::Char(c) => {
                    self.discover.bulk_tag_input.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.bulk_tag_input.pop();
                }
                _ => {}
            }
            return;
        }

        // If creating rule, handle rule tag input
        if self.discover.is_creating_rule {
            match key.code {
                KeyCode::Enter => {
                    // Save rule with pattern from filter and entered tag
                    let tag = self.discover.rule_tag_input.trim().to_string();
                    let pattern = self.discover.rule_pattern_input.clone();
                    if !tag.is_empty() && !pattern.is_empty() {
                        // TODO: Actually save to database
                        self.discover.status_message = Some((
                            format!("Created rule: {} -> {}", pattern, tag),
                            false,
                        ));
                        // Add to local rules list for immediate feedback
                        self.discover.rules.push(RuleInfo {
                            id: -1, // Temporary ID until saved to DB
                            pattern,
                            tag,
                            priority: 100,
                            enabled: true,
                        });
                    }
                    self.discover.is_creating_rule = false;
                    self.discover.rule_tag_input.clear();
                    self.discover.rule_pattern_input.clear();
                }
                KeyCode::Esc => {
                    self.discover.is_creating_rule = false;
                    self.discover.rule_tag_input.clear();
                    self.discover.rule_pattern_input.clear();
                    self.discover.editing_rule_id = None;
                }
                KeyCode::Char(c) => {
                    self.discover.rule_tag_input.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.rule_tag_input.pop();
                }
                _ => {}
            }
            return;
        }

        // If tagging, handle tag input
        if self.discover.is_tagging {
            match key.code {
                KeyCode::Enter => {
                    // Apply tag to selected file
                    let tag = self.discover.tag_input.trim().to_string();
                    if !tag.is_empty() {
                        if let Some(file) = self.filtered_files().get(self.discover.selected) {
                            let file_path = file.path.clone();
                            self.apply_tag_to_file(&file_path, &tag);
                        }
                    }
                    self.discover.is_tagging = false;
                    self.discover.tag_input.clear();
                }
                KeyCode::Esc => {
                    self.discover.is_tagging = false;
                    self.discover.tag_input.clear();
                }
                KeyCode::Char(c) => {
                    self.discover.tag_input.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.tag_input.pop();
                }
                KeyCode::Tab => {
                    // Autocomplete from available tags
                    if !self.discover.tag_input.is_empty() {
                        let input_lower = self.discover.tag_input.to_lowercase();
                        if let Some(matching_tag) = self.discover.available_tags.iter()
                            .find(|t| t.to_lowercase().starts_with(&input_lower))
                        {
                            self.discover.tag_input = matching_tag.clone();
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        // If filtering, handle text input
        if self.discover.is_filtering {
            match key.code {
                KeyCode::Enter => {
                    self.discover.is_filtering = false;
                }
                KeyCode::Esc => {
                    self.discover.is_filtering = false;
                    self.discover.filter.clear();
                }
                KeyCode::Char(c) => {
                    self.discover.filter.push(c);
                }
                KeyCode::Backspace => {
                    self.discover.filter.pop();
                }
                _ => {}
            }
            return;
        }

        // Handle dropdown input FIRST when open (captures all keys including numbers)
        if self.discover.sources_dropdown_open {
            self.handle_sources_dropdown_key(key);
            return;
        }

        if self.discover.tags_dropdown_open {
            self.handle_tags_dropdown_key(key);
            return;
        }

        // Rules Manager dialog intercepts all keys when open
        if self.discover.rules_manager_open {
            self.handle_rules_manager_key(key);
            return;
        }

        // Number keys: Open dropdowns (only when no dropdown is open)
        match key.code {
            KeyCode::Char('1') => {
                self.discover.focus = DiscoverFocus::Sources;
                self.discover.sources_dropdown_open = true;
                self.discover.sources_filter.clear();
                self.discover.preview_source = Some(self.discover.selected_source);
                return;
            }
            KeyCode::Char('2') => {
                self.discover.focus = DiscoverFocus::Tags;
                self.discover.tags_dropdown_open = true;
                self.discover.tags_filter.clear();
                self.discover.preview_tag = self.discover.selected_tag;
                return;
            }
            KeyCode::Char('3') => {
                self.discover.focus = DiscoverFocus::Files;
                return;
            }
            KeyCode::Char('R') => {
                // Open Rules Manager dialog
                self.discover.rules_manager_open = true;
                self.discover.selected_rule = 0;
                return;
            }
            _ => {}
        }

        // Tab: Toggle preview when in Files focus
        if key.code == KeyCode::Tab {
            if self.discover.focus == DiscoverFocus::Files {
                self.discover.preview_open = !self.discover.preview_open;
            }
            return;
        }

        // Esc: Layered escape behavior
        if key.code == KeyCode::Esc {
            // 1. Clear filter if active
            if !self.discover.filter.is_empty() {
                self.discover.filter.clear();
                self.discover.selected = 0;
                return;
            }
            // 2. Go to Home (handled by global Esc handler)
            return;
        }

        // Focus-specific navigation and actions
        match self.discover.focus {
            DiscoverFocus::Files => self.handle_discover_files_key(key),
            DiscoverFocus::Sources => self.handle_discover_sources_key(key),
            DiscoverFocus::Tags => self.handle_discover_tags_key(key),
        }
    }

    /// Handle keys when Files panel is focused
    fn handle_discover_files_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.discover.selected < self.filtered_files().len().saturating_sub(1) {
                    self.discover.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.discover.selected > 0 {
                    self.discover.selected -= 1;
                }
            }
            KeyCode::Char('/') => {
                self.discover.is_filtering = true;
            }
            KeyCode::Char('p') => {
                self.discover.preview_open = !self.discover.preview_open;
            }
            KeyCode::Char('s') => {
                // Open scan path input
                self.discover.is_entering_path = true;
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
            }
            KeyCode::Char('r') => {
                // Reload from Scout DB
                self.discover.data_loaded = false;
                self.discover.sources_loaded = false;
            }
            KeyCode::Char('t') => {
                // Open tag dialog for selected file (or bulk tag if filter active)
                if !self.discover.filter.is_empty() {
                    // With filter active, 't' tags all filtered files
                    let count = self.filtered_files().len();
                    if count > 0 {
                        self.discover.is_bulk_tagging = true;
                        self.discover.bulk_tag_input.clear();
                        self.discover.bulk_tag_save_as_rule = false;
                    }
                } else if !self.filtered_files().is_empty() {
                    self.discover.is_tagging = true;
                    self.discover.tag_input.clear();
                }
            }
            KeyCode::Char('R') => {
                // Create rule from current filter
                if !self.discover.filter.is_empty() {
                    self.discover.is_creating_rule = true;
                    self.discover.rule_tag_input.clear();
                } else {
                    self.discover.status_message = Some((
                        "Enter a filter pattern first (press /)".to_string(),
                        true,
                    ));
                }
            }
            KeyCode::Char('S') => {
                // Create source from selected directory
                let file_info = self.filtered_files().get(self.discover.selected)
                    .map(|f| (f.is_dir, f.path.clone()));

                if let Some((is_dir, path)) = file_info {
                    if is_dir {
                        self.discover.is_creating_source = true;
                        self.discover.source_name_input.clear();
                        self.discover.pending_source_path = Some(path);
                    } else {
                        self.discover.status_message = Some((
                            "Select a directory to create a source".to_string(),
                            true,
                        ));
                    }
                }
            }
            KeyCode::Char('T') => {
                // Bulk tag all filtered/visible files (explicit T)
                let count = self.filtered_files().len();
                if count > 0 {
                    self.discover.is_bulk_tagging = true;
                    self.discover.bulk_tag_input.clear();
                    self.discover.bulk_tag_save_as_rule = false;
                } else {
                    self.discover.status_message = Some((
                        "No files to tag".to_string(),
                        true,
                    ));
                }
            }
            _ => {}
        }
    }

    /// Get filtered sources based on dropdown filter
    fn filtered_sources(&self) -> Vec<(usize, &SourceInfo)> {
        let filter = self.discover.sources_filter.to_lowercase();
        self.discover.sources
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                filter.is_empty() || s.name.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// Get filtered tags based on dropdown filter
    fn filtered_tags(&self) -> Vec<(usize, &TagInfo)> {
        let filter = self.discover.tags_filter.to_lowercase();
        self.discover.tags
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                filter.is_empty() || t.name.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// Handle keys when Sources dropdown is open
    /// Arrow keys navigate, all other chars go to filter
    fn handle_sources_dropdown_key(&mut self, key: KeyEvent) {
        let filtered = self.filtered_sources();

        match key.code {
            KeyCode::Down => {
                if let Some(preview_idx) = self.discover.preview_source {
                    // Find current position in filtered list
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos + 1 < filtered.len() {
                            self.discover.preview_source = Some(filtered[pos + 1].0);
                            self.discover.data_loaded = false; // Trigger file preview reload
                        }
                    }
                } else if !filtered.is_empty() {
                    self.discover.preview_source = Some(filtered[0].0);
                    self.discover.data_loaded = false;
                }
            }
            KeyCode::Up => {
                if let Some(preview_idx) = self.discover.preview_source {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos > 0 {
                            self.discover.preview_source = Some(filtered[pos - 1].0);
                            self.discover.data_loaded = false;
                        }
                    }
                }
            }
            KeyCode::Enter => {
                // Confirm selection, close dropdown, focus Files
                if let Some(preview_idx) = self.discover.preview_source {
                    self.discover.selected_source = preview_idx;
                    self.discover.data_loaded = false; // Reload files for confirmed source
                    // Clear tag selection when source changes
                    self.discover.selected_tag = None;
                    self.discover.filter.clear();
                }
                self.discover.sources_dropdown_open = false;
                self.discover.sources_filter.clear();
                self.discover.preview_source = None;
                self.discover.focus = DiscoverFocus::Files;
            }
            KeyCode::Esc => {
                // Close dropdown without changing selection
                self.discover.sources_dropdown_open = false;
                self.discover.sources_filter.clear();
                self.discover.preview_source = None;
                self.discover.focus = DiscoverFocus::Files;
            }
            KeyCode::Backspace => {
                self.discover.sources_filter.pop();
                // Reset preview to first match if current preview is filtered out
                let filtered = self.filtered_sources();
                if let Some(preview_idx) = self.discover.preview_source {
                    if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                        self.discover.preview_source = filtered.first().map(|(i, _)| *i);
                        if self.discover.preview_source.is_some() {
                            self.discover.data_loaded = false;
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                // All characters go to filter (including numbers, j, k, etc.)
                self.discover.sources_filter.push(c);
                // Reset preview to first match if current preview is filtered out
                let filtered = self.filtered_sources();
                if let Some(preview_idx) = self.discover.preview_source {
                    if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                        self.discover.preview_source = filtered.first().map(|(i, _)| *i);
                        if self.discover.preview_source.is_some() {
                            self.discover.data_loaded = false;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle keys when Sources panel is focused (dropdown closed)
    fn handle_discover_sources_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('n') => {
                // Create new source (open scan dialog)
                self.discover.is_entering_path = true;
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
            }
            _ => {}
        }
    }

    /// Handle keys when Tags dropdown is open
    /// Arrow keys navigate, all other chars go to filter
    fn handle_tags_dropdown_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                let filtered = self.filtered_tags();
                if let Some(preview_idx) = self.discover.preview_tag {
                    // Find current position in filtered list
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos + 1 < filtered.len() {
                            let new_idx = filtered[pos + 1].0;
                            self.discover.preview_tag = Some(new_idx);
                            // Trigger file reload with tag filter
                            self.discover.data_loaded = false;
                        }
                    }
                } else if !filtered.is_empty() {
                    // Select first tag
                    let new_idx = filtered[0].0;
                    self.discover.preview_tag = Some(new_idx);
                    self.discover.data_loaded = false;
                }
            }
            KeyCode::Up => {
                let filtered = self.filtered_tags();
                if let Some(preview_idx) = self.discover.preview_tag {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos > 0 {
                            let new_idx = filtered[pos - 1].0;
                            self.discover.preview_tag = Some(new_idx);
                            self.discover.data_loaded = false;
                        } else {
                            // At top of list, select "All files" (None)
                            self.discover.preview_tag = None;
                            self.discover.data_loaded = false;
                        }
                    }
                }
            }
            KeyCode::Enter => {
                // Confirm selection, close dropdown, focus Files
                self.discover.selected_tag = self.discover.preview_tag;
                self.discover.tags_dropdown_open = false;
                self.discover.tags_filter.clear();
                self.discover.preview_tag = None;
                self.discover.focus = DiscoverFocus::Files;
                self.discover.data_loaded = false; // Reload files with selected tag
            }
            KeyCode::Esc => {
                // Close dropdown, show all files
                self.discover.selected_tag = None;
                self.discover.tags_dropdown_open = false;
                self.discover.tags_filter.clear();
                self.discover.preview_tag = None;
                self.discover.focus = DiscoverFocus::Files;
                self.discover.data_loaded = false;
            }
            KeyCode::Backspace => {
                if self.discover.tags_filter.is_empty() {
                    // Empty filter + backspace: move to "All files" or close
                    if self.discover.preview_tag.is_some() {
                        self.discover.preview_tag = None;
                        self.discover.data_loaded = false;
                    } else {
                        // Already at "all files", close dropdown
                        self.discover.selected_tag = None;
                        self.discover.tags_dropdown_open = false;
                        self.discover.focus = DiscoverFocus::Files;
                        self.discover.data_loaded = false;
                    }
                } else {
                    self.discover.tags_filter.pop();
                    // Reset preview if filtered out
                    let filtered = self.filtered_tags();
                    if let Some(preview_idx) = self.discover.preview_tag {
                        if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                            self.discover.preview_tag = filtered.first().map(|(i, _)| *i);
                            self.discover.data_loaded = false;
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                // All characters go to filter (including numbers, j, k, etc.)
                self.discover.tags_filter.push(c);
                // Reset preview to first match if current is filtered out
                let filtered = self.filtered_tags();
                if let Some(preview_idx) = self.discover.preview_tag {
                    if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                        self.discover.preview_tag = filtered.first().map(|(i, _)| *i);
                        self.discover.data_loaded = false;
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle keys when Tags panel is focused (dropdown closed)
    fn handle_discover_tags_key(&mut self, key: KeyEvent) {
        // Tags panel doesn't have specific keybindings when dropdown is closed
        // Press 2 to open dropdown, R to manage rules
        match key.code {
            _ => {}
        }
    }

    /// Handle keys when Rules Manager dialog is open
    fn handle_rules_manager_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.discover.selected_rule < self.discover.rules.len().saturating_sub(1) {
                    self.discover.selected_rule += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.discover.selected_rule > 0 {
                    self.discover.selected_rule -= 1;
                }
            }
            KeyCode::Char('n') => {
                // Create new rule
                self.discover.is_creating_rule = true;
                self.discover.rule_tag_input.clear();
                self.discover.rule_pattern_input.clear();
                self.discover.editing_rule_id = None;
            }
            KeyCode::Char('e') => {
                // Edit selected rule
                if let Some(rule) = self.discover.rules.get(self.discover.selected_rule) {
                    self.discover.is_creating_rule = true;
                    self.discover.rule_pattern_input = rule.pattern.clone();
                    self.discover.rule_tag_input = rule.tag.clone();
                    self.discover.editing_rule_id = Some(rule.id);
                }
            }
            KeyCode::Char('d') => {
                // Delete selected rule (TODO: add confirmation)
                if !self.discover.rules.is_empty() {
                    self.discover.rules.remove(self.discover.selected_rule);
                    if self.discover.selected_rule >= self.discover.rules.len() && self.discover.selected_rule > 0 {
                        self.discover.selected_rule -= 1;
                    }
                    // TODO: Delete from DB
                }
            }
            KeyCode::Enter => {
                // Toggle rule enabled/disabled
                if let Some(rule) = self.discover.rules.get_mut(self.discover.selected_rule) {
                    rule.enabled = !rule.enabled;
                    // TODO: Update in DB
                }
            }
            KeyCode::Esc => {
                // Close Rules Manager
                self.discover.rules_manager_open = false;
            }
            _ => {}
        }
    }

    /// Create a source from a directory path
    fn create_source(&mut self, path: &str, name: &str) {
        // Note: Actual DB persistence would be done async via tick() or a separate task
        // For now, just show a success message
        self.discover.status_message = Some((
            format!("Created source '{}' from {}", name,
                std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string())
            ),
            false,
        ));
    }

    /// Apply a tag to a file (stores in local state, async save happens on tick)
    fn apply_tag_to_file(&mut self, file_path: &str, tag: &str) {
        // Find the file in our list and add the tag locally
        for file in &mut self.discover.files {
            if file.path == file_path {
                if !file.tags.contains(&tag.to_string()) {
                    file.tags.push(tag.to_string());
                    self.discover.status_message = Some((
                        format!("Tagged '{}' with '{}'",
                            std::path::Path::new(file_path)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| file_path.to_string()),
                            tag
                        ),
                        false,
                    ));
                    // Add to available tags if new
                    if !self.discover.available_tags.contains(&tag.to_string()) {
                        self.discover.available_tags.push(tag.to_string());
                    }
                } else {
                    self.discover.status_message = Some((
                        format!("File already has tag '{}'", tag),
                        true,
                    ));
                }
                break;
            }
        }

        // Note: Actual DB persistence would be done async via tick() or a separate task
        // For now this updates local state immediately
    }

    /// Get files filtered by current filter
    ///
    /// Supports gitignore-style patterns:
    /// - `foo` matches any path containing "foo"
    /// - `*foo*` matches paths with "foo" anywhere (wildcard)
    /// - `*.py` matches files ending in .py
    pub fn filtered_files(&self) -> Vec<&FileInfo> {
        if self.discover.filter.is_empty() {
            self.discover.files.iter().collect()
        } else {
            // Check if filter contains wildcard characters
            let has_wildcards = self.discover.filter.contains('*')
                || self.discover.filter.contains('?');

            if has_wildcards {
                // Use globset with case-insensitive matching
                use globset::GlobBuilder;

                // Wrap pattern to match anywhere in path if not already a path pattern
                let pattern = if self.discover.filter.contains('/') {
                    self.discover.filter.clone()
                } else {
                    format!("**/{}", self.discover.filter)
                };

                match GlobBuilder::new(&pattern)
                    .case_insensitive(true)
                    .build()
                    .map(|g| g.compile_matcher())
                {
                    Ok(matcher) => {
                        self.discover.files
                            .iter()
                            .filter(|f| {
                                // Strip leading / for glob matching (glob ** doesn't match leading /)
                                let path = f.path.strip_prefix('/').unwrap_or(&f.path);
                                matcher.is_match(path)
                            })
                            .collect()
                    }
                    Err(_) => {
                        // Invalid pattern, fall back to substring match
                        let filter_lower = self.discover.filter.to_lowercase();
                        self.discover.files
                            .iter()
                            .filter(|f| f.path.to_lowercase().contains(&filter_lower))
                            .collect()
                    }
                }
            } else {
                // Simple substring match (case insensitive)
                let filter_lower = self.discover.filter.to_lowercase();
                self.discover.files
                    .iter()
                    .filter(|f| f.path.to_lowercase().contains(&filter_lower))
                    .collect()
            }
        }
    }

    /// Scan a directory recursively and add files to the discover list
    fn scan_directory(&mut self, path: &str) {
        use std::path::Path;
        use walkdir::WalkDir;

        let path = Path::new(path);

        // Expand ~ to home directory
        let expanded_path = if path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                home.join(path.strip_prefix("~").unwrap_or(path))
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        if !expanded_path.exists() {
            self.discover.scan_error = Some(format!("Path not found: {}", expanded_path.display()));
            return;
        }

        if !expanded_path.is_dir() {
            self.discover.scan_error = Some(format!("Not a directory: {}", expanded_path.display()));
            return;
        }

        // Scan directory recursively using walkdir
        let mut new_files = Vec::new();
        for entry in WalkDir::new(&expanded_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let file_path = entry.path();
            if let Ok(metadata) = entry.metadata() {
                let modified = metadata.modified()
                    .map(|t| chrono::DateTime::<chrono::Local>::from(t))
                    .unwrap_or_else(|_| chrono::Local::now());

                // Skip the root directory itself
                if file_path == expanded_path {
                    continue;
                }

                // Compute relative path from scan root
                let rel_path = file_path
                    .strip_prefix(&expanded_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| file_path.display().to_string());

                new_files.push(FileInfo {
                    path: file_path.display().to_string(),
                    rel_path,
                    size: metadata.len(),
                    modified,
                    is_dir: metadata.is_dir(),
                    tags: vec![],
                });
            }
        }

        // Sort by path for consistent ordering
        new_files.sort_by(|a, b| a.path.cmp(&b.path));

        self.discover.files = new_files;
        self.discover.selected = 0;
        self.discover.scan_error = None;
    }

    /// Load files from Scout database for the selected source (async using sqlx)
    ///
    /// Uses the scout_files schema from scout/db.rs:
    /// - path: TEXT
    /// - size: INTEGER
    /// - mtime: INTEGER (milliseconds since epoch)
    /// - tag: TEXT (single tag per file, NULL if untagged)
    ///
    /// Files are filtered by the currently selected source. If no source is
    /// selected, the file list will be empty with a helpful message.
    /// When sources dropdown is open, uses preview_source for live preview.
    async fn load_scout_files(&mut self) {
        use sqlx::SqlitePool;

        // Use preview source if dropdown is open, otherwise use selected source
        let source_idx = if self.discover.sources_dropdown_open {
            self.discover.preview_source.unwrap_or(self.discover.selected_source)
        } else {
            self.discover.selected_source
        };

        // Check if we have a selected source - source-first workflow
        let selected_source_id = match self.discover.sources.get(source_idx) {
            Some(source) => source.id.clone(),
            None => {
                // No source selected - show empty list with guidance
                self.discover.files.clear();
                self.discover.selected = 0;
                self.discover.data_loaded = true;
                self.discover.scan_error = if self.discover.sources.is_empty() {
                    Some("No sources found. Press 's' to scan a folder.".to_string())
                } else {
                    Some("Press 1 to select a source".to_string())
                };
                return;
            }
        };

        // Get database path
        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            self.discover.scan_error = Some("No Scout database found. Press 's' to scan a folder.".to_string());
            self.discover.data_loaded = true;
            return;
        }

        // Connect and query - filter by selected source
        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        match SqlitePool::connect(&db_url).await {
            Ok(pool) => {
                // Query matches scout_files schema, filtered by source_id
                let query = r#"
                    SELECT path, rel_path, source_id, size, mtime, tag
                    FROM scout_files
                    WHERE source_id = ?
                    ORDER BY rel_path
                    LIMIT 1000
                "#;

                match sqlx::query_as::<_, (String, String, String, i64, i64, Option<String>)>(query)
                    .bind(&selected_source_id)
                    .fetch_all(&pool)
                    .await
                {
                    Ok(rows) => {
                        let files: Vec<FileInfo> = rows
                            .into_iter()
                            .map(|(path, rel_path, _source_id, size, mtime_millis, tag)| {
                                // Convert mtime from milliseconds to DateTime
                                let modified = chrono::DateTime::from_timestamp_millis(mtime_millis)
                                    .map(|dt| dt.with_timezone(&chrono::Local))
                                    .unwrap_or_else(chrono::Local::now);

                                // Single tag becomes a vec (for UI compatibility)
                                let tags = tag.into_iter().collect();

                                let is_dir = std::path::Path::new(&path).is_dir();

                                FileInfo {
                                    path,
                                    rel_path,
                                    size: size as u64,
                                    modified,
                                    is_dir,
                                    tags,
                                }
                            })
                            .collect();

                        self.discover.files = files;
                        self.discover.selected = 0;
                        self.discover.data_loaded = true;
                        self.discover.scan_error = None;
                    }
                    Err(e) => {
                        self.discover.scan_error = Some(format!("Query failed: {}", e));
                        self.discover.data_loaded = true;
                    }
                }
            }
            Err(e) => {
                self.discover.scan_error = Some(format!("DB connection failed: {}", e));
                self.discover.data_loaded = true;
            }
        }
    }

    /// Load sources from Scout database
    async fn load_sources(&mut self) {
        use sqlx::SqlitePool;

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            self.discover.sources_loaded = true;
            return;
        }

        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        if let Ok(pool) = SqlitePool::connect(&db_url).await {
            let query = r#"
                SELECT s.id, s.name, s.path,
                       (SELECT COUNT(*) FROM scout_files WHERE source_id = s.id) as file_count
                FROM scout_sources s
                WHERE s.enabled = 1
                ORDER BY s.name
            "#;

            if let Ok(rows) = sqlx::query_as::<_, (String, String, String, i64)>(query)
                .fetch_all(&pool)
                .await
            {
                self.discover.sources = rows
                    .into_iter()
                    .map(|(id, name, path, file_count)| SourceInfo {
                        id,
                        name,
                        path,
                        file_count: file_count as usize,
                    })
                    .collect();

                // Auto-select first source if none selected and sources exist
                if !self.discover.sources.is_empty() && self.discover.selected_source >= self.discover.sources.len() {
                    self.discover.selected_source = 0;
                }
            }
        }
        self.discover.sources_loaded = true;
    }

    /// Load tags from files for the selected source
    /// Tags are derived from actual file tags, not from rules
    async fn load_tags_for_source(&mut self) {
        use sqlx::SqlitePool;

        // Get source ID for selected source
        let source_id = match self.discover.sources.get(self.discover.selected_source) {
            Some(source) => source.id.clone(),
            None => {
                self.discover.tags.clear();
                return;
            }
        };

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            return;
        }

        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        if let Ok(pool) = SqlitePool::connect(&db_url).await {
            // Get total file count
            let total_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ?"
            )
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

            // Get distinct tags with counts
            let query = r#"
                SELECT tag, COUNT(*) as count
                FROM scout_files
                WHERE source_id = ? AND tag IS NOT NULL AND tag != ''
                GROUP BY tag
                ORDER BY count DESC, tag
            "#;

            let mut tags = Vec::new();

            // Add "All files" as first option
            tags.push(TagInfo {
                name: "All files".to_string(),
                count: total_count as usize,
                is_special: true,
            });

            if let Ok(rows) = sqlx::query_as::<_, (String, i64)>(query)
                .bind(&source_id)
                .fetch_all(&pool)
                .await
            {
                for (tag_name, count) in rows {
                    tags.push(TagInfo {
                        name: tag_name,
                        count: count as usize,
                        is_special: false,
                    });
                }
            }

            // Get untagged count
            let untagged_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND (tag IS NULL OR tag = '')"
            )
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

            if untagged_count > 0 {
                tags.push(TagInfo {
                    name: "untagged".to_string(),
                    count: untagged_count as usize,
                    is_special: true,
                });
            }

            self.discover.tags = tags;

            // Clamp selected tag if it's out of bounds
            if let Some(idx) = self.discover.selected_tag {
                if idx >= self.discover.tags.len() {
                    self.discover.selected_tag = None; // Reset to "All files"
                }
            }
        }
    }

    /// Load tagging rules for the Rules Manager dialog
    async fn load_rules_for_manager(&mut self) {
        use sqlx::SqlitePool;

        // Get source ID for selected source
        let source_id = match self.discover.sources.get(self.discover.selected_source) {
            Some(source) => source.id.clone(),
            None => {
                self.discover.rules.clear();
                return;
            }
        };

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            return;
        }

        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        if let Ok(pool) = SqlitePool::connect(&db_url).await {
            let query = r#"
                SELECT id, pattern, tag, priority, enabled
                FROM scout_tagging_rules
                WHERE source_id = ?
                ORDER BY priority DESC, pattern
            "#;

            if let Ok(rows) = sqlx::query_as::<_, (i64, String, String, i32, bool)>(query)
                .bind(&source_id)
                .fetch_all(&pool)
                .await
            {
                self.discover.rules = rows
                    .into_iter()
                    .map(|(id, pattern, tag, priority, enabled)| RuleInfo {
                        id,
                        pattern,
                        tag,
                        priority,
                        enabled,
                    })
                    .collect();

                // Clamp selected rule if it's out of bounds
                if self.discover.selected_rule >= self.discover.rules.len() && !self.discover.rules.is_empty() {
                    self.discover.selected_rule = 0;
                }
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
                    if self.chat.scroll == usize::MAX {
                        self.chat.scroll = self.chat.estimated_total_lines();
                    }
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
                if self.chat.scroll == usize::MAX {
                    self.chat.scroll = self.chat.estimated_total_lines();
                }
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

    /// Handle inspect mode keys
    fn handle_inspect_key(&mut self, key: KeyEvent) {
        match key.code {
            // Query input when focused - must come before specific char matches!
            KeyCode::Char(c) if self.inspect.query_focused => {
                self.inspect.query_input.push(c);
            }
            KeyCode::Backspace if self.inspect.query_focused => {
                self.inspect.query_input.pop();
            }
            KeyCode::Enter if self.inspect.query_focused => {
                // Execute query (placeholder - would call query_output tool)
                if !self.inspect.query_input.is_empty() {
                    self.inspect.query_result = Some(format!(
                        "Query executed: {}\n(Query results would appear here)",
                        self.inspect.query_input
                    ));
                }
            }
            KeyCode::Esc if self.inspect.query_focused => {
                self.inspect.query_focused = false;
            }
            // Table navigation (only when query not focused)
            KeyCode::Char('j') | KeyCode::Down => {
                if self.inspect.selected_table < self.inspect.tables.len().saturating_sub(1) {
                    self.inspect.selected_table += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.inspect.selected_table > 0 {
                    self.inspect.selected_table -= 1;
                }
            }
            // Toggle query input focus
            KeyCode::Char('/') => {
                self.inspect.query_focused = !self.inspect.query_focused;
            }
            // Export selected table
            KeyCode::Char('e') => {
                // TODO: Export table to file
            }
            // Filter mode
            KeyCode::Char('f') => {
                // TODO: Open filter dialog
            }
            _ => {}
        }
    }

    /// Handle jobs mode keys
    fn handle_jobs_key(&mut self, key: KeyEvent) {
        let filtered_count = self.jobs_state.filtered_jobs().len();

        match key.code {
            // Job navigation (within filtered list)
            KeyCode::Char('j') | KeyCode::Down => {
                if self.jobs_state.selected_index < filtered_count.saturating_sub(1) {
                    self.jobs_state.selected_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.jobs_state.selected_index > 0 {
                    self.jobs_state.selected_index -= 1;
                }
            }
            // Retry failed job
            KeyCode::Char('r') => {
                let jobs = self.jobs_state.filtered_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if job.status == JobStatus::Failed {
                        // TODO: Actually retry the job
                    }
                }
            }
            // Cancel running job
            KeyCode::Char('c') => {
                let jobs = self.jobs_state.filtered_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if job.status == JobStatus::Running {
                        // TODO: Actually cancel the job
                    }
                }
            }
            // Filter by status (uses set_filter to clamp selection)
            KeyCode::Char('1') => {
                self.jobs_state.set_filter(Some(JobStatus::Pending));
            }
            KeyCode::Char('2') => {
                self.jobs_state.set_filter(Some(JobStatus::Running));
            }
            KeyCode::Char('3') => {
                self.jobs_state.set_filter(Some(JobStatus::Completed));
            }
            KeyCode::Char('4') => {
                self.jobs_state.set_filter(Some(JobStatus::Failed));
            }
            KeyCode::Char('0') => {
                self.jobs_state.set_filter(None); // Show all
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
        self.chat.scroll = usize::MAX;

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
        // Load Scout data if in Discover mode
        if self.mode == TuiMode::Discover {
            // Load sources for sidebar
            if !self.discover.sources_loaded {
                self.load_sources().await;
            }
            // Load files for selected source (also reloads tags when source changes)
            if !self.discover.data_loaded {
                self.load_scout_files().await;
                // Reload tags for the (possibly new) selected source
                self.load_tags_for_source().await;
            }
            // Load rules for Rules Manager if it's open
            if self.discover.rules_manager_open && self.discover.rules.is_empty() {
                self.load_rules_for_manager().await;
            }
        }

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
        // Enable chat sidebar and focus on it
        app.show_chat_sidebar = true;
        app.focus = AppFocus::Chat;

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
        // Enable chat sidebar and focus on it
        app.show_chat_sidebar = true;
        app.focus = AppFocus::Chat;
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
        // Enable chat sidebar and focus on it
        app.show_chat_sidebar = true;
        app.focus = AppFocus::Chat;

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
        app.show_chat_sidebar = true;
        app.focus = AppFocus::Chat;

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

    // =========================================================================
    // Jobs Mode Tests - Critical Path Coverage
    // =========================================================================

    fn create_test_jobs() -> Vec<JobInfo> {
        vec![
            JobInfo {
                id: 1,
                file_path: "/data/a.csv".into(),
                parser_name: "parser_a".into(),
                status: JobStatus::Pending,
                retry_count: 0,
                error_message: None,
                created_at: Local::now(),
            },
            JobInfo {
                id: 2,
                file_path: "/data/b.csv".into(),
                parser_name: "parser_b".into(),
                status: JobStatus::Running,
                retry_count: 0,
                error_message: None,
                created_at: Local::now(),
            },
            JobInfo {
                id: 3,
                file_path: "/data/c.csv".into(),
                parser_name: "parser_c".into(),
                status: JobStatus::Failed,
                retry_count: 2,
                error_message: Some("Parse error".into()),
                created_at: Local::now(),
            },
            JobInfo {
                id: 4,
                file_path: "/data/d.csv".into(),
                parser_name: "parser_d".into(),
                status: JobStatus::Completed,
                retry_count: 0,
                error_message: None,
                created_at: Local::now(),
            },
        ]
    }

    #[test]
    fn test_jobs_filtered_jobs() {
        let mut state = JobsState::default();
        state.jobs = create_test_jobs();

        // No filter - all 4 jobs
        assert_eq!(state.filtered_jobs().len(), 4);

        // Filter to Pending - 1 job
        state.status_filter = Some(JobStatus::Pending);
        assert_eq!(state.filtered_jobs().len(), 1);
        assert_eq!(state.filtered_jobs()[0].id, 1);

        // Filter to Failed - 1 job
        state.status_filter = Some(JobStatus::Failed);
        assert_eq!(state.filtered_jobs().len(), 1);
        assert_eq!(state.filtered_jobs()[0].id, 3);
    }

    #[test]
    fn test_jobs_filter_clamps_selection() {
        let mut state = JobsState::default();
        state.jobs = create_test_jobs();
        state.selected_index = 3; // Last job (Completed)

        // Filter to Pending - only 1 job exists, selection must clamp to 0
        state.set_filter(Some(JobStatus::Pending));
        assert_eq!(state.selected_index, 0);

        // Filter to nothing (no matches) - selection stays 0
        state.jobs = vec![]; // Clear jobs
        state.selected_index = 5;
        state.clamp_selection();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_jobs_navigation_bounds() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;
        app.jobs_state.jobs = create_test_jobs();
        app.jobs_state.selected_index = 0;

        // Navigate down to last item
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for _ in 0..10 {
                app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                    .await;
            }
        });
        // Should stop at last valid index (3)
        assert_eq!(app.jobs_state.selected_index, 3);

        // Navigate up past beginning
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for _ in 0..10 {
                app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
                    .await;
            }
        });
        // Should stop at 0
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    #[test]
    fn test_jobs_navigation_respects_filter() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;
        app.jobs_state.jobs = create_test_jobs();

        // Filter to show only Pending and Failed (2 jobs total won't work, let's just use Pending)
        // Actually, with our test data, Pending has 1 job
        app.jobs_state.set_filter(Some(JobStatus::Pending));
        assert_eq!(app.jobs_state.filtered_jobs().len(), 1);

        // Try to navigate - should stay at 0 since only 1 item
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    // =========================================================================
    // Inspect Mode Tests - Critical Path Coverage
    // =========================================================================

    fn create_test_tables() -> Vec<TableInfo> {
        vec![
            TableInfo {
                name: "orders".into(),
                row_count: 1000,
                column_count: 5,
                size_bytes: 50000,
                last_updated: Local::now(),
            },
            TableInfo {
                name: "customers".into(),
                row_count: 500,
                column_count: 8,
                size_bytes: 25000,
                last_updated: Local::now(),
            },
        ]
    }

    #[test]
    fn test_inspect_navigation() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Inspect;
        app.inspect.tables = create_test_tables();
        app.inspect.selected_table = 0;

        // Navigate down
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.inspect.selected_table, 1);

        // Try to go past end
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.inspect.selected_table, 1); // Stays at last

        // Navigate up
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.inspect.selected_table, 0);
    }

    #[test]
    fn test_inspect_query_focus_toggle() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Inspect;
        app.inspect.tables = create_test_tables();
        assert!(!app.inspect.query_focused);

        // Press / to focus query
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
                .await;
        });
        assert!(app.inspect.query_focused);

        // Press Esc to unfocus
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.inspect.query_focused);
    }

    #[test]
    fn test_inspect_query_input() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Inspect;
        app.inspect.tables = create_test_tables();
        app.inspect.query_focused = true;

        // Type a query
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for c in "SELECT".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });
        assert_eq!(app.inspect.query_input, "SELECT");

        // Backspace
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.inspect.query_input, "SELEC");
    }

    #[test]
    fn test_inspect_navigation_blocked_when_query_focused() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Inspect;
        app.inspect.tables = create_test_tables();
        app.inspect.selected_table = 0;
        app.inspect.query_focused = true;

        // Try to navigate with j - should NOT move selection (j goes to query)
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        // Selection unchanged, but 'j' was typed into query
        assert_eq!(app.inspect.selected_table, 0);
        assert_eq!(app.inspect.query_input, "j");
    }

    // =========================================================================
    // JobStatus Display Method Tests
    // =========================================================================

    #[test]
    fn test_job_status_symbol() {
        assert_eq!(JobStatus::Pending.symbol(), "⏳");
        assert_eq!(JobStatus::Running.symbol(), "▶");
        assert_eq!(JobStatus::Completed.symbol(), "✓");
        assert_eq!(JobStatus::Failed.symbol(), "✗");
    }

    #[test]
    fn test_job_status_as_str() {
        assert_eq!(JobStatus::Pending.as_str(), "Pending");
        assert_eq!(JobStatus::Running.as_str(), "Running");
        assert_eq!(JobStatus::Completed.as_str(), "Completed");
        assert_eq!(JobStatus::Failed.as_str(), "Failed");
    }

    // =========================================================================
    // Edge Case / Failure Mode Tests
    // =========================================================================

    #[test]
    fn test_jobs_empty_list_navigation() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;
        // Empty jobs list
        app.jobs_state.jobs = vec![];
        app.jobs_state.selected_index = 0;

        // Try to navigate - should not panic
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
            app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    #[test]
    fn test_inspect_empty_tables_navigation() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Inspect;
        // Empty tables list
        app.inspect.tables = vec![];
        app.inspect.selected_table = 0;

        // Try to navigate - should not panic
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
            app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.inspect.selected_table, 0);
    }

    #[test]
    fn test_filter_to_empty_result() {
        let mut state = JobsState::default();
        state.jobs = create_test_jobs(); // Has Pending, Running, Failed, Completed
        state.selected_index = 2;

        // Remove all jobs and filter - should handle gracefully
        state.jobs.clear();
        state.set_filter(Some(JobStatus::Pending));
        assert_eq!(state.filtered_jobs().len(), 0);
        assert_eq!(state.selected_index, 0);
    }

    // =========================================================================
    // Discover Mode Dialog Tests - New TUI Features
    // =========================================================================

    fn create_test_files() -> Vec<FileInfo> {
        vec![
            FileInfo {
                path: "/data/sales.csv".into(),
                rel_path: "sales.csv".into(),
                size: 1024,
                modified: Local::now(),
                tags: vec!["sales".into()],
                is_dir: false,
            },
            FileInfo {
                path: "/data/orders.csv".into(),
                rel_path: "orders.csv".into(),
                size: 2048,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                path: "/data/archives".into(),
                rel_path: "archives".into(),
                size: 0,
                modified: Local::now(),
                tags: vec![],
                is_dir: true,
            },
        ]
    }

    #[test]
    fn test_discover_filter_mode() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        assert!(!app.discover.is_filtering);
        assert!(app.discover.filter.is_empty());

        // Press / to enter filter mode
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
                .await;
        });
        assert!(app.discover.is_filtering);

        // Type filter text
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for c in "sales".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });
        assert_eq!(app.discover.filter, "sales");

        // Verify filtering works
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].path.contains("sales"));
    }

    #[test]
    fn test_discover_filter_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.is_filtering = true;
        app.discover.filter = "test".to_string();

        // Esc should exit filter mode, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.is_filtering);
        assert!(app.discover.filter.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_tag_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.selected = 1; // Select orders.csv
        assert!(!app.discover.is_tagging);

        // Press 't' to open tag dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
                .await;
        });
        assert!(app.discover.is_tagging);
        assert!(app.discover.tag_input.is_empty());

        // Type tag name
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for c in "important".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });
        assert_eq!(app.discover.tag_input, "important");
    }

    #[test]
    fn test_discover_tag_dialog_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.is_tagging = true;
        app.discover.tag_input = "partial".to_string();

        // Esc should close tag dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.is_tagging);
        assert!(app.discover.tag_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_scan_path_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        assert!(!app.discover.is_entering_path);

        // Press 's' to open scan path dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
                .await;
        });
        assert!(app.discover.is_entering_path);

        // Type path
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for c in "/tmp".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .await;
            }
        });
        assert_eq!(app.discover.scan_path_input, "/tmp");
    }

    #[test]
    fn test_discover_scan_path_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.is_entering_path = true;
        app.discover.scan_path_input = "/some/path".to_string();

        // Esc should close scan dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.is_entering_path);
        assert!(app.discover.scan_path_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_bulk_tag_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        assert!(!app.discover.is_bulk_tagging);

        // Press 'T' (Shift+t) to open bulk tag dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT))
                .await;
        });
        assert!(app.discover.is_bulk_tagging);
        assert!(app.discover.bulk_tag_input.is_empty());
        assert!(!app.discover.bulk_tag_save_as_rule);
    }

    #[test]
    fn test_discover_bulk_tag_toggle_save_as_rule() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.is_bulk_tagging = true;
        assert!(!app.discover.bulk_tag_save_as_rule);

        // Press Space to toggle save-as-rule
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
                .await;
        });
        assert!(app.discover.bulk_tag_save_as_rule);

        // Press Space again to toggle back
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.bulk_tag_save_as_rule);
    }

    #[test]
    fn test_discover_bulk_tag_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.is_bulk_tagging = true;
        app.discover.bulk_tag_input = "batch".to_string();
        app.discover.bulk_tag_save_as_rule = true;

        // Esc should close bulk tag dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.is_bulk_tagging);
        assert!(app.discover.bulk_tag_input.is_empty());
        assert!(!app.discover.bulk_tag_save_as_rule);
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_create_source_on_directory() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.selected = 2; // Select archives directory
        assert!(!app.discover.is_creating_source);

        // Press 'S' (Shift+s) on a directory to create source
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT))
                .await;
        });
        assert!(app.discover.is_creating_source);
        assert!(app.discover.pending_source_path.is_some());
        assert!(app.discover.pending_source_path.as_ref().unwrap().contains("archives"));
    }

    #[test]
    fn test_discover_create_source_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.is_creating_source = true;
        app.discover.source_name_input = "my_source".to_string();
        app.discover.pending_source_path = Some("/data/archives".to_string());

        // Esc should close create source dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(!app.discover.is_creating_source);
        assert!(app.discover.source_name_input.is_empty());
        assert!(app.discover.pending_source_path.is_none());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_esc_goes_home_when_no_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        // No dialogs open
        assert!(!app.discover.is_filtering);
        assert!(!app.discover.is_entering_path);
        assert!(!app.discover.is_tagging);
        assert!(!app.discover.is_creating_source);
        assert!(!app.discover.is_bulk_tagging);

        // Esc should go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Home));
    }

    #[test]
    fn test_discover_navigation_with_files() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.selected = 0;

        // Navigate down with j
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 1);

        // Navigate down again
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 2);

        // Try to navigate past end
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 2); // Stays at last

        // Navigate up
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 1);
    }

    #[test]
    fn test_discover_filter_glob_pattern() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        // Use realistic absolute paths like real scans produce
        app.discover.files = vec![
            FileInfo {
                path: "/Users/test/workspace/blog/myproject/manage.py".into(),
                rel_path: "manage.py".into(),
                size: 1024,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                path: "/Users/test/workspace/blog/myproject/manifest.json".into(),
                rel_path: "manifest.json".into(),
                size: 2048,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                path: "/Users/test/workspace/blog/myproject/other.txt".into(),
                rel_path: "other.txt".into(),
                size: 512,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                path: "/Users/test/workspace/blog/myproject/subdir/commands.py".into(),
                rel_path: "subdir/commands.py".into(),
                size: 256,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
        ];

        // Test wildcard pattern *man*
        app.discover.filter = "*man*".to_string();
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 3, "Should match manage.py, manifest.json, commands.py");
        assert!(filtered.iter().any(|f| f.path.contains("manage")));
        assert!(filtered.iter().any(|f| f.path.contains("manifest")));
        assert!(filtered.iter().any(|f| f.path.contains("commands")));

        // Test wildcard pattern *.py
        app.discover.filter = "*.py".to_string();
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 2, "Should match both .py files");
        assert!(filtered.iter().all(|f| f.path.ends_with(".py")));

        // Test wildcard pattern **/*man* (gitignore style) - critical test for absolute paths
        app.discover.filter = "**/*man*".to_string();
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 3, "Should match manage.py, manifest.json, commands.py with absolute paths");
    }

    #[test]
    fn test_discover_filter_substring_still_works() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();

        // Simple substring filter (no glob chars)
        app.discover.filter = "sales".to_string();
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].path.contains("sales"));

        // Case insensitive
        app.discover.filter = "SALES".to_string();
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_discover_backspace_in_dialogs() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Test backspace in scan path
        app.discover.is_entering_path = true;
        app.discover.scan_path_input = "/tmp/test".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.scan_path_input, "/tmp/tes");

        // Reset and test backspace in tag input
        app.discover.is_entering_path = false;
        app.discover.is_tagging = true;
        app.discover.tag_input = "mytag".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.tag_input, "myta");

        // Reset and test backspace in bulk tag input
        app.discover.is_tagging = false;
        app.discover.is_bulk_tagging = true;
        app.discover.bulk_tag_input = "bulktag".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.bulk_tag_input, "bulkta");
    }
}

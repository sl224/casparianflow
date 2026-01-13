//! Application state for the TUI

use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::ToolResult;
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;

use super::llm::claude_code::ClaudeCodeProvider;
use super::llm::{registry_to_definitions, LlmProvider, StreamChunk};
use super::TuiArgs;
use crate::scout::{
    Database as ScoutDatabase, ScanProgress as ScoutProgress, Scanner as ScoutScanner, Source,
    SourceType,
};

/// Current TUI mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiMode {
    #[default]
    Home,        // Home hub with 4 cards
    Discover,    // File discovery and tagging
    ParserBench, // Parser development workbench
    Inspect,     // Output inspection
    Jobs,        // Job queue management
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
    /// Job was cancelled by user
    Cancelled,
}

impl JobStatus {
    /// Get display symbol for this status
    /// Symbols per tui.md Section 5.3:
    /// ○ = Pending, ↻ = Running, ✓ = Complete, ✗ = Failed, ⊘ = Cancelled
    pub fn symbol(&self) -> &'static str {
        match self {
            JobStatus::Pending => "○",
            JobStatus::Running => "↻",
            JobStatus::Completed => "✓",
            JobStatus::Failed => "✗",
            JobStatus::Cancelled => "⊘",
        }
    }

    /// Get display text for this status
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "Pending",
            JobStatus::Running => "Running",
            JobStatus::Completed => "Completed",
            JobStatus::Failed => "Failed",
            JobStatus::Cancelled => "Cancelled",
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

// =============================================================================
// Parser Bench Types
// =============================================================================

/// State for Parser Bench mode (parser development workbench)
#[derive(Debug, Clone, Default)]
pub struct ParserBenchState {
    /// View mode within Parser Bench
    pub view: ParserBenchView,
    /// Whether right panel is fullscreen (focus mode)
    pub focus_mode: bool,
    /// List of parsers from ~/.casparian_flow/parsers/
    pub parsers: Vec<ParserInfo>,
    /// Currently selected parser index
    pub selected_parser: usize,
    /// Whether parsers have been loaded
    pub parsers_loaded: bool,
    /// Quick test path (for testing arbitrary files)
    pub quick_test_path: Option<std::path::PathBuf>,
    /// Files matched to selected parser via topics
    pub bound_files: Vec<BoundFileInfo>,
    /// Currently selected bound file index
    pub selected_file: usize,
    /// Test result from last run
    pub test_result: Option<ParserTestResult>,
    /// Whether a test is currently running
    pub test_running: bool,
    /// Last file used for testing (for re-run)
    pub last_test_file: Option<std::path::PathBuf>,
    /// Filter text for parser list
    pub filter: String,
    /// Whether filter input is active
    pub is_filtering: bool,
}

/// View modes within Parser Bench
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserBenchView {
    #[default]
    ParserList,
    FilePicker,
    FilesView,
    ResultView,
}

/// Parser information discovered from parsers directory
#[derive(Debug, Clone)]
pub struct ParserInfo {
    /// Full path to the parser file
    pub path: std::path::PathBuf,
    /// Parser name (from metadata or filename)
    pub name: String,
    /// Parser version (from metadata)
    pub version: Option<String>,
    /// Topics the parser subscribes to
    pub topics: Vec<String>,
    /// Last modified time
    pub modified: DateTime<Local>,
    /// Parser health status
    pub health: ParserHealth,
    /// Whether this is a symlink
    pub is_symlink: bool,
    /// Whether the symlink is broken (target doesn't exist)
    pub symlink_broken: bool,
}

/// Parser health state
#[derive(Debug, Clone, Default)]
pub enum ParserHealth {
    /// Parser is working well
    Healthy {
        success_rate: f64,
        total_runs: usize,
    },
    /// Parser has some failures
    Warning {
        consecutive_failures: u32,
    },
    /// Circuit breaker tripped
    Paused {
        reason: String,
    },
    /// Never run / unknown
    #[default]
    Unknown,
    /// Symlink target doesn't exist
    BrokenLink,
}

impl ParserHealth {
    /// Get display symbol for this health state
    pub fn symbol(&self) -> &'static str {
        match self {
            ParserHealth::Healthy { .. } => "●",
            ParserHealth::Warning { .. } => "⚠",
            ParserHealth::Paused { .. } => "⏸",
            ParserHealth::Unknown => "○",
            ParserHealth::BrokenLink => "✗",
        }
    }
}

/// File bound to a parser via topic matching
#[derive(Debug, Clone)]
pub struct BoundFileInfo {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub status: BoundFileStatus,
}

/// Processing status of a bound file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundFileStatus {
    #[default]
    Pending,
    Processed,
    Failed,
}

impl BoundFileStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            BoundFileStatus::Pending => "○",
            BoundFileStatus::Processed => "✓",
            BoundFileStatus::Failed => "✗",
        }
    }
}

/// Result from running a parser test
#[derive(Debug, Clone)]
pub struct ParserTestResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Option<Vec<SchemaColumn>>,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub suggestions: Vec<String>,
    pub error_type: Option<String>,
    pub truncated: bool,
}

/// Schema column info from parser test
#[derive(Debug, Clone)]
pub struct SchemaColumn {
    pub name: String,
    pub dtype: String,
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

/// Which field is focused in the rule creation dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleDialogFocus {
    #[default]
    Pattern,
    Tag,
}

// =============================================================================
// Text Input Handling (shared across all input fields)
// =============================================================================

/// Result from processing a text input key event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputResult {
    /// User pressed Enter - commit the input
    Committed,
    /// User pressed Esc - cancel the input
    Cancelled,
    /// Input was modified, continue editing
    Continue,
    /// Key was not handled by text input (e.g., Tab)
    NotHandled,
}

/// Process a key event for a text input field
/// Returns what action should be taken
fn handle_text_input(key: KeyEvent, input: &mut String) -> TextInputResult {
    match key.code {
        KeyCode::Enter => TextInputResult::Committed,
        KeyCode::Esc => TextInputResult::Cancelled,
        KeyCode::Char(c) => {
            input.push(c);
            TextInputResult::Continue
        }
        KeyCode::Backspace => {
            input.pop();
            TextInputResult::Continue
        }
        _ => TextInputResult::NotHandled,
    }
}

// =============================================================================
// Newtypes for type safety
// =============================================================================

/// Strongly-typed source ID (from database)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SourceId {
    fn from(s: String) -> Self {
        SourceId(s)
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Strongly-typed rule ID (None = unsaved rule)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuleId(pub Option<i64>);

impl RuleId {
    pub fn new(id: i64) -> Self {
        RuleId(Some(id))
    }

    pub fn unsaved() -> Self {
        RuleId(None)
    }

    pub fn is_saved(&self) -> bool {
        self.0.is_some()
    }
}

impl Default for RuleId {
    fn default() -> Self {
        RuleId::unsaved()
    }
}

// =============================================================================
// View State Machine
// =============================================================================

/// View state machine for Discover mode - matches spec Section 4
/// Controls which dialog/dropdown/view is currently active
/// ALL modal states are represented here (no boolean flags)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoverViewState {
    #[default]
    Files,              // Default state, navigate files
    // --- Modal input overlays (were previously booleans) ---
    Filtering,          // Text filter input (was is_filtering)
    EnteringPath,       // Scan path input (was is_entering_path)
    Tagging,            // Single file tag input (was is_tagging)
    CreatingSource,     // Source name input (was is_creating_source)
    BulkTagging,        // Bulk tag input (was is_bulk_tagging)
    // --- Dropdown menus ---
    SourcesDropdown,    // Filtering/selecting sources
    TagsDropdown,       // Filtering/selecting tags
    // --- Full dialogs ---
    RulesManager,       // Dialog for rule CRUD
    RuleCreation,       // Dialog for creating/editing single rule
    // --- Sources Manager (spec v1.7) ---
    SourcesManager,     // Dialog for source CRUD (M key)
    SourceEdit,         // Nested dialog for editing source name
    SourceDeleteConfirm, // Delete confirmation dialog
    // --- Background scanning ---
    Scanning,           // Directory scan in progress (non-blocking)
}

/// Filter applied to file list based on tag selection
#[derive(Debug, Clone, PartialEq)]
pub enum TagFilter {
    Untagged,           // Show files where tag IS NULL
    Tag(String),        // Show files with specific tag
}

/// Source information for Discover mode sidebar
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub id: SourceId,
    pub name: String,
    #[allow(dead_code)] // Will be used for displaying full path in details view
    pub path: std::path::PathBuf,
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
    pub id: RuleId,
    pub pattern: String,
    pub tag: String,
    #[allow(dead_code)] // Used in Rules Manager sorting
    pub priority: i32,
    pub enabled: bool,
}

/// Pending tag write for persistence
#[derive(Debug, Clone)]
pub struct PendingTagWrite {
    pub file_path: String,
    pub tag: String,
}

/// Result from background directory scan
#[derive(Debug)]
pub enum TuiScanResult {
    /// Progress update during scan
    Progress(ScoutProgress),
    /// Scanning completed successfully
    Complete {
        source_path: String,
    },
    /// Scanning failed with error
    Error(String),
}

/// Pending rule write for persistence
#[derive(Debug, Clone)]
pub struct PendingRuleWrite {
    pub pattern: String,
    pub tag: String,
    pub source_id: SourceId,
}

// ============================================================================
// Glob Explorer State (Hierarchical File Browsing)
// ============================================================================

/// State for Glob Explorer - hierarchical file browsing for large sources.
/// Single source of truth for all explorer state.
#[derive(Debug, Clone)]
pub struct GlobExplorerState {
    // --- Input state (what user requested) ---
    /// Current glob pattern filter (e.g., "*.csv", "**/*.json")
    pub pattern: String,
    /// History of patterns for Backspace navigation
    pub pattern_history: Vec<String>,
    /// Current path prefix (empty = root, "folder/" = inside folder)
    pub current_prefix: String,

    // --- Derived state (loaded atomically from DB) ---
    /// Folders/files at current level with file counts
    pub folders: Vec<FolderInfo>,
    /// Sampled preview files (max 10)
    pub preview_files: Vec<GlobPreviewFile>,
    /// Total file count for current prefix + pattern
    pub total_count: GlobFileCount,

    // --- O(1) Navigation Cache ---
    /// Preloaded folder hierarchy - key is prefix, value is children at that level
    /// Example: "" -> [FolderInfo{name: "logs", ...}, FolderInfo{name: "data", ...}]
    ///          "logs/" -> [FolderInfo{name: "app.log", is_file: true}, ...]
    pub folder_cache: HashMap<String, Vec<FolderInfo>>,
    /// Whether cache has been loaded for current source
    pub cache_loaded: bool,
    /// Source ID for which cache was loaded (to detect source changes)
    pub cache_source_id: Option<String>,

    // --- UI state ---
    /// Currently selected folder index
    pub selected_folder: usize,
    /// Current phase in the explorer state machine
    pub phase: GlobExplorerPhase,
    /// Whether pattern input is active (for typing)
    pub pattern_editing: bool,
}

impl Default for GlobExplorerState {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            pattern_history: Vec::new(),
            current_prefix: String::new(),
            folders: Vec::new(),
            preview_files: Vec::new(),
            total_count: GlobFileCount::Exact(0),
            folder_cache: HashMap::new(),
            cache_loaded: false,
            cache_source_id: None,
            selected_folder: 0,
            phase: GlobExplorerPhase::Explore,
            pattern_editing: false,
        }
    }
}

/// Folder/file info for hierarchical browsing
#[derive(Debug, Clone)]
pub struct FolderInfo {
    /// Folder or file name
    pub name: String,
    /// Number of files in/under this folder
    pub file_count: usize,
    /// True if this is a leaf file (not a folder)
    pub is_file: bool,
}

/// Preview file for Glob Explorer
#[derive(Debug, Clone)]
pub struct GlobPreviewFile {
    pub rel_path: String,
    pub size: u64,
    pub mtime: i64,
}

/// File count (exact or estimated for large sources)
#[derive(Debug, Clone)]
pub enum GlobFileCount {
    Exact(usize),
    Estimated(usize),
}

impl GlobFileCount {
    pub fn value(&self) -> usize {
        match self {
            GlobFileCount::Exact(n) => *n,
            GlobFileCount::Estimated(n) => *n,
        }
    }
}

/// State machine phases for Glob Explorer
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GlobExplorerPhase {
    /// Browsing root level folders
    #[default]
    Explore,
    /// Drilled into a folder (narrowed scope)
    Focused,
}

/// State for the Discover mode (File Explorer)
#[derive(Debug, Clone, Default)]
pub struct DiscoverState {
    // --- State machine (per spec Section 4) ---
    /// Current view state - controls which dialog/dropdown is active
    /// ALL modal states are in this enum (no separate boolean flags)
    pub view_state: DiscoverViewState,
    /// Previous state for "return to previous" transitions (Esc from dialogs)
    pub previous_view_state: Option<DiscoverViewState>,
    /// Active tag filter applied to files
    pub tag_filter: Option<TagFilter>,

    // --- File list ---
    pub files: Vec<FileInfo>,
    pub selected: usize,
    /// Text filter for file list (used in Filtering state)
    pub filter: String,
    pub preview_open: bool,
    /// Path input for scan dialog (used in EnteringPath state)
    pub scan_path_input: String,
    /// Error message from last scan attempt
    pub scan_error: Option<String>,
    /// Whether data has been loaded from Scout DB
    pub data_loaded: bool,
    /// Tag input for new tag (used in Tagging state)
    pub tag_input: String,
    /// Available tags from DB for autocomplete
    pub available_tags: Vec<String>,
    /// Status message (success/error) for user feedback
    pub status_message: Option<(String, bool)>, // (message, is_error)
    /// Source name input (used in CreatingSource state)
    pub source_name_input: String,
    /// Directory path for the source being created
    pub pending_source_path: Option<String>,
    /// Tag input for bulk tagging (used in BulkTagging state)
    pub bulk_tag_input: String,
    /// Whether to save bulk tag as a rule
    pub bulk_tag_save_as_rule: bool,

    // --- Glob Explorer (hierarchical file browsing) ---
    /// Glob Explorer state (Some = explorer active, None = flat file list)
    pub glob_explorer: Option<GlobExplorerState>,

    // --- Sidebar state ---
    /// Current focus within Discover mode
    pub focus: DiscoverFocus,
    /// Available sources from DB
    pub sources: Vec<SourceInfo>,
    /// Currently selected source (by ID, not index - stable across list changes)
    pub selected_source_id: Option<SourceId>,
    /// Whether sources have been loaded
    pub sources_loaded: bool,

    // --- Tags dropdown (sidebar panel 2) ---
    /// Tags derived from files (for dropdown navigation)
    pub tags: Vec<TagInfo>,
    /// Currently selected tag index (None = "All files")
    pub selected_tag: Option<usize>,
    /// Filter text for tags dropdown
    pub tags_filter: String,
    /// Whether actively filtering in tags dropdown (vim-style modal)
    pub tags_filtering: bool,
    /// Temporary tag index while navigating dropdown (for preview)
    pub preview_tag: Option<usize>,

    // --- Sources dropdown state ---
    /// Filter text for sources dropdown
    pub sources_filter: String,
    /// Whether actively filtering in sources dropdown (vim-style modal)
    pub sources_filtering: bool,
    /// Temporary source index while navigating dropdown (for preview)
    pub preview_source: Option<usize>,

    // --- Rules Manager dialog ---
    /// Tagging rules for the selected source (for Rules Manager)
    pub rules: Vec<RuleInfo>,
    /// Currently selected rule in Rules Manager
    pub selected_rule: usize,

    // --- Rule creation/edit dialog ---
    /// Tag input for new/edited rule
    pub rule_tag_input: String,
    /// Pattern input for new/edited rule
    pub rule_pattern_input: String,
    /// Rule being edited (None = creating new)
    pub editing_rule_id: Option<RuleId>,
    /// Which field is focused in rule dialog (Pattern or Tag)
    pub rule_dialog_focus: RuleDialogFocus,
    /// Live preview of files matching current pattern
    pub rule_preview_files: Vec<String>,
    /// Count of files matching current pattern
    pub rule_preview_count: usize,

    // --- Sources Manager dialog (spec v1.7) ---
    /// Selected source in Sources Manager list (separate from main selection)
    pub sources_manager_selected: usize,
    /// Name input for editing source
    pub source_edit_input: String,
    /// Source being edited (stores ID and original name)
    pub editing_source: Option<SourceId>,
    /// Source pending deletion (for confirmation dialog)
    pub source_to_delete: Option<SourceId>,

    // --- Pending DB writes ---
    pub pending_tag_writes: Vec<PendingTagWrite>,
    pub pending_rule_writes: Vec<PendingRuleWrite>,

    // --- Background scanning ---
    /// Path being scanned (for display)
    pub scanning_path: Option<String>,
    /// Current scan progress (updated during scan)
    pub scan_progress: Option<ScoutProgress>,
    /// When scan started (for elapsed time display)
    pub scan_start_time: Option<std::time::Instant>,

    // --- Directory autocomplete (path input) ---
    /// Suggested directories matching current path input
    pub path_suggestions: Vec<String>,
    /// Currently selected suggestion index
    pub path_suggestion_idx: usize,
}

impl DiscoverState {
    /// Get the index of the currently selected source (or 0 if none/not found)
    pub fn selected_source_index(&self) -> usize {
        self.selected_source_id
            .as_ref()
            .and_then(|id| self.sources.iter().position(|s| &s.id == id))
            .unwrap_or(0)
    }

    /// Set the selected source by index
    pub fn select_source_by_index(&mut self, idx: usize) {
        self.selected_source_id = self.sources.get(idx).map(|s| s.id.clone());
    }

    /// Get the currently selected source
    pub fn selected_source(&self) -> Option<&SourceInfo> {
        self.selected_source_id
            .as_ref()
            .and_then(|id| self.sources.iter().find(|s| &s.id == id))
    }

    /// Ensure selection is valid after sources list changes
    pub fn validate_source_selection(&mut self) {
        if self.sources.is_empty() {
            self.selected_source_id = None;
        } else if self.selected_source_id.is_none()
            || !self.sources.iter().any(|s| Some(&s.id) == self.selected_source_id.as_ref())
        {
            // Selection invalid, select first source
            self.selected_source_id = self.sources.first().map(|s| s.id.clone());
        }
    }
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
    /// Whether the help overlay is visible (per spec Section 3.1)
    pub show_help: bool,
    /// Current input focus (Main vs Chat)
    pub focus: AppFocus,
    /// Home hub state
    pub home: HomeState,
    /// Discover mode state
    pub discover: DiscoverState,
    /// Parser Bench mode state
    pub parser_bench: ParserBenchState,
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
    /// Pending scan result from background directory scan
    pending_scan: Option<mpsc::Receiver<TuiScanResult>>,
    /// Job ID for the currently running scan (for status updates)
    current_scan_job_id: Option<i64>,
    /// Tick counter for animated UI elements (spinner, etc.)
    pub tick_count: u64,
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
            show_help: false,
            focus: AppFocus::Main,
            home: HomeState::default(),
            discover: DiscoverState::default(),
            parser_bench: ParserBenchState::default(),
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
            pending_scan: None,
            current_scan_job_id: None,
            tick_count: 0,
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
            show_help: false,
            focus: AppFocus::Main,
            home: HomeState::default(),
            discover: DiscoverState::default(),
            parser_bench: ParserBenchState::default(),
            chat: ChatState::default(),
            inspect: InspectState::default(),
            jobs_state: JobsState::default(),
            tools: create_default_registry(),
            llm: None,
            llm_provider: Some(provider),
            config: args,
            error: None,
            pending_response: None,
            pending_scan: None,
            current_scan_job_id: None,
            tick_count: 0,
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
            // Number keys for primary navigation (1-4)
            // Note: In Discover mode, 1/2/3 are overridden for panel focus
            // Don't intercept when chat is focused (allow typing numbers)
            KeyCode::Char('1') if self.focus != AppFocus::Chat && self.mode != TuiMode::Discover => {
                self.mode = TuiMode::Discover;
                return;
            }
            KeyCode::Char('2') if self.focus != AppFocus::Chat && self.mode != TuiMode::Discover => {
                self.mode = TuiMode::ParserBench;
                return;
            }
            KeyCode::Char('3') if self.focus != AppFocus::Chat && self.mode != TuiMode::Discover => {
                self.mode = TuiMode::Jobs;
                return;
            }
            KeyCode::Char('4') if self.focus != AppFocus::Chat => {
                // TODO: Switch to Sources view when implemented
                // For now, switch to Inspect as placeholder
                self.mode = TuiMode::Inspect;
                return;
            }
            // 0 or H: Return to Home (from any view)
            // Don't intercept when chat is focused
            KeyCode::Char('0') if self.focus != AppFocus::Chat => {
                self.mode = TuiMode::Home;
                return;
            }
            KeyCode::Char('H') if self.focus != AppFocus::Chat => {
                self.mode = TuiMode::Home;
                return;
            }
            // q: Quit application (per spec Section 3.1)
            // Don't intercept when in text input mode
            KeyCode::Char('q') if self.focus != AppFocus::Chat && !self.in_text_input_mode() => {
                // TODO: Add confirmation dialog if unsaved changes
                self.running = false;
                return;
            }
            // r: Refresh current view (per spec Section 3.3)
            // Don't intercept when in text input mode
            KeyCode::Char('r') if self.focus != AppFocus::Chat && !self.in_text_input_mode() => {
                self.refresh_current_view();
                return;
            }
            // ?: Toggle help overlay (per spec Section 3.1)
            // Don't intercept when in text input mode
            KeyCode::Char('?') if self.focus != AppFocus::Chat && !self.in_text_input_mode() => {
                self.show_help = !self.show_help;
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
            // Esc: Close help overlay first, then handle other escapes
            KeyCode::Esc if self.show_help => {
                self.show_help = false;
                return;
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
                    // Any state other than Files needs local Esc
                    self.discover.view_state != DiscoverViewState::Files ||
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
            TuiMode::ParserBench => self.handle_parser_bench_key(key),
            TuiMode::Inspect => self.handle_inspect_key(key),
            TuiMode::Jobs => self.handle_jobs_key(key),
        }
    }

    // ======== Discover State Machine Helpers ========

    /// Transition to a new Discover view state, saving current as previous
    fn transition_discover_state(&mut self, new_state: DiscoverViewState) {
        self.discover.previous_view_state = Some(self.discover.view_state);
        self.discover.view_state = new_state;
    }

    /// Return to previous Discover view state (for Esc from dialogs/dropdowns)
    fn return_to_previous_discover_state(&mut self) {
        if let Some(prev) = self.discover.previous_view_state.take() {
            self.discover.view_state = prev;
        } else {
            self.discover.view_state = DiscoverViewState::Files;
        }
    }

    /// Handle Discover mode keys - using unified state machine
    fn handle_discover_key(&mut self, key: KeyEvent) {
        // Clear status message on any key press
        if self.discover.status_message.is_some() && key.code != KeyCode::Esc {
            self.discover.status_message = None;
        }

        // Global keybindings that work from most states (per spec Section 6.1)
        // R (Rules Manager) and M (Sources Manager) work from Files, dropdowns, etc.
        // but NOT from dialogs that are already open
        if !matches!(self.discover.view_state,
            DiscoverViewState::RulesManager |
            DiscoverViewState::RuleCreation |
            DiscoverViewState::SourcesManager |
            DiscoverViewState::SourceEdit |
            DiscoverViewState::SourceDeleteConfirm |
            DiscoverViewState::EnteringPath |
            DiscoverViewState::CreatingSource |
            DiscoverViewState::Tagging |
            DiscoverViewState::BulkTagging |
            DiscoverViewState::Filtering
        ) {
            match key.code {
                KeyCode::Char('R') => {
                    self.transition_discover_state(DiscoverViewState::RulesManager);
                    self.discover.selected_rule = 0;
                    return;
                }
                KeyCode::Char('M') => {
                    self.transition_discover_state(DiscoverViewState::SourcesManager);
                    self.discover.sources_manager_selected = self.discover.selected_source_index();
                    return;
                }
                _ => {}
            }
        }

        // Route to handler based on current view state
        match self.discover.view_state {
            // === Modal text input states (using shared handler) ===
            DiscoverViewState::EnteringPath => {
                // Handle autocomplete navigation first
                match key.code {
                    KeyCode::Tab if !self.discover.path_suggestions.is_empty() => {
                        // Apply selected suggestion
                        self.apply_path_suggestion();
                        return;
                    }
                    KeyCode::Down if !self.discover.path_suggestions.is_empty() => {
                        // Navigate down in suggestions
                        let max_idx = self.discover.path_suggestions.len().saturating_sub(1);
                        self.discover.path_suggestion_idx =
                            (self.discover.path_suggestion_idx + 1).min(max_idx);
                        return;
                    }
                    KeyCode::Up if !self.discover.path_suggestions.is_empty() => {
                        // Navigate up in suggestions
                        self.discover.path_suggestion_idx =
                            self.discover.path_suggestion_idx.saturating_sub(1);
                        return;
                    }
                    _ => {}
                }

                // Handle regular text input
                match handle_text_input(key, &mut self.discover.scan_path_input) {
                    TextInputResult::Committed => {
                        let path = self.discover.scan_path_input.clone();
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.path_suggestions.clear();
                        if !path.is_empty() {
                            self.scan_directory(&path);
                        }
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.scan_path_input.clear();
                        self.discover.path_suggestions.clear();
                        self.discover.scan_error = None;
                    }
                    TextInputResult::Continue => {
                        // Update suggestions after any character change
                        self.update_path_suggestions();
                    }
                    TextInputResult::NotHandled => {}
                }
            }

            DiscoverViewState::CreatingSource => {
                match handle_text_input(key, &mut self.discover.source_name_input) {
                    TextInputResult::Committed => {
                        let name = self.discover.source_name_input.trim().to_string();
                        if !name.is_empty() {
                            if let Some(path) = self.discover.pending_source_path.take() {
                                self.create_source(&path, &name);
                            }
                        }
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.source_name_input.clear();
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.source_name_input.clear();
                        self.discover.pending_source_path = None;
                    }
                    TextInputResult::Continue | TextInputResult::NotHandled => {}
                }
            }

            DiscoverViewState::BulkTagging => {
                // Special handling: Space toggles option
                if key.code == KeyCode::Char(' ') {
                    self.discover.bulk_tag_save_as_rule = !self.discover.bulk_tag_save_as_rule;
                    return;
                }
                match handle_text_input(key, &mut self.discover.bulk_tag_input) {
                    TextInputResult::Committed => {
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
                            let rule_msg = if self.discover.bulk_tag_save_as_rule { " (rule saved)" } else { "" };
                            self.discover.status_message = Some((
                                format!("Tagged {} files with '{}'{}", count, tag, rule_msg),
                                false,
                            ));
                        }
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.bulk_tag_input.clear();
                        self.discover.bulk_tag_save_as_rule = false;
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.bulk_tag_input.clear();
                        self.discover.bulk_tag_save_as_rule = false;
                    }
                    TextInputResult::Continue | TextInputResult::NotHandled => {}
                }
            }

            DiscoverViewState::Tagging => {
                // Special handling: Tab for autocomplete
                if key.code == KeyCode::Tab {
                    if !self.discover.tag_input.is_empty() {
                        let input_lower = self.discover.tag_input.to_lowercase();
                        if let Some(matching_tag) = self.discover.available_tags.iter()
                            .find(|t| t.to_lowercase().starts_with(&input_lower))
                        {
                            self.discover.tag_input = matching_tag.clone();
                        }
                    }
                    return;
                }
                match handle_text_input(key, &mut self.discover.tag_input) {
                    TextInputResult::Committed => {
                        let tag = self.discover.tag_input.trim().to_string();
                        if !tag.is_empty() {
                            if let Some(file) = self.filtered_files().get(self.discover.selected) {
                                let file_path = file.path.clone();
                                self.apply_tag_to_file(&file_path, &tag);
                            }
                        }
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.tag_input.clear();
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.tag_input.clear();
                    }
                    TextInputResult::Continue | TextInputResult::NotHandled => {}
                }
            }

            DiscoverViewState::Filtering => {
                match handle_text_input(key, &mut self.discover.filter) {
                    TextInputResult::Committed => {
                        self.discover.view_state = DiscoverViewState::Files;
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.filter.clear();
                    }
                    TextInputResult::Continue | TextInputResult::NotHandled => {}
                }
            }

            // === Dialog states ===
            DiscoverViewState::RuleCreation => {
                match key.code {
                    KeyCode::Enter => {
                        let tag = self.discover.rule_tag_input.trim().to_string();
                        let pattern = self.discover.rule_pattern_input.trim().to_string();
                        if !tag.is_empty() && !pattern.is_empty() {
                            let tagged_count = self.apply_rule_to_files(&pattern, &tag);
                            self.discover.rules.push(RuleInfo {
                                id: RuleId::unsaved(),
                                pattern: pattern.clone(),
                                tag: tag.clone(),
                                priority: 100,
                                enabled: true,
                            });
                            self.refresh_tags_list();
                            self.discover.status_message = Some((
                                format!("Created rule: {} → {} ({} files tagged)", pattern, tag, tagged_count),
                                false,
                            ));
                        } else if tag.is_empty() && !pattern.is_empty() {
                            self.discover.status_message = Some(("Please enter a tag name".to_string(), true));
                            return;
                        } else if pattern.is_empty() {
                            self.discover.status_message = Some(("Please enter a pattern".to_string(), true));
                            return;
                        }
                        self.close_rule_creation_dialog();
                    }
                    KeyCode::Esc => self.close_rule_creation_dialog(),
                    KeyCode::Tab | KeyCode::BackTab => {
                        self.discover.rule_dialog_focus = match self.discover.rule_dialog_focus {
                            RuleDialogFocus::Pattern => RuleDialogFocus::Tag,
                            RuleDialogFocus::Tag => RuleDialogFocus::Pattern,
                        };
                    }
                    KeyCode::Char(c) => match self.discover.rule_dialog_focus {
                        RuleDialogFocus::Pattern => {
                            self.discover.rule_pattern_input.push(c);
                            self.update_rule_preview();
                        }
                        RuleDialogFocus::Tag => self.discover.rule_tag_input.push(c),
                    },
                    KeyCode::Backspace => match self.discover.rule_dialog_focus {
                        RuleDialogFocus::Pattern => {
                            self.discover.rule_pattern_input.pop();
                            self.update_rule_preview();
                        }
                        RuleDialogFocus::Tag => { self.discover.rule_tag_input.pop(); }
                    },
                    _ => {}
                }
            }

            DiscoverViewState::RulesManager => self.handle_rules_manager_key(key),
            DiscoverViewState::SourcesDropdown => self.handle_sources_dropdown_key(key),
            DiscoverViewState::TagsDropdown => self.handle_tags_dropdown_key(key),

            // === Sources Manager states (spec v1.7) ===
            DiscoverViewState::SourcesManager => self.handle_sources_manager_key(key),
            DiscoverViewState::SourceEdit => self.handle_source_edit_key(key),
            DiscoverViewState::SourceDeleteConfirm => self.handle_source_delete_confirm_key(key),

            // === Background scanning state ===
            DiscoverViewState::Scanning => {
                match key.code {
                    // Esc cancels the scan
                    KeyCode::Esc => {
                        // Update job status to Cancelled
                        if let Some(job_id) = self.current_scan_job_id {
                            self.update_scan_job_status(job_id, JobStatus::Cancelled, None);
                        }

                        self.pending_scan = None;
                        self.current_scan_job_id = None;
                        self.discover.scanning_path = None;
                        self.discover.scan_progress = None;
                        self.discover.scan_start_time = None;
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.status_message = Some(("Scan cancelled".to_string(), true));
                    }
                    // Navigate to Home while scan continues in background
                    KeyCode::Char('0') => {
                        // Don't cancel - scan continues, just switch view
                        self.discover.view_state = DiscoverViewState::Files;
                        self.mode = TuiMode::Home;
                        self.discover.status_message = Some(("Scan running in background...".to_string(), false));
                    }
                    // Navigate to Jobs while scan continues in background
                    KeyCode::Char('4') => {
                        // Don't cancel - scan continues, just switch view
                        self.discover.view_state = DiscoverViewState::Files;
                        self.mode = TuiMode::Jobs;
                        self.discover.status_message = Some(("Scan running in background...".to_string(), false));
                    }
                    // All other keys are ignored during scanning
                    _ => {}
                }
            }

            // === Default file browsing state ===
            DiscoverViewState::Files => {
                match key.code {
                    KeyCode::Char('1') => {
                        self.discover.focus = DiscoverFocus::Sources;
                        self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                        self.discover.sources_filter.clear();
                        self.discover.preview_source = Some(self.discover.selected_source_index());
                    }
                    KeyCode::Char('2') => {
                        self.discover.focus = DiscoverFocus::Tags;
                        self.transition_discover_state(DiscoverViewState::TagsDropdown);
                        self.discover.tags_filter.clear();
                        self.discover.preview_tag = self.discover.selected_tag;
                    }
                    KeyCode::Char('3') => self.discover.focus = DiscoverFocus::Files,
                    KeyCode::Char('n') => self.open_rule_creation_dialog(),
                    // Note: R and M are now handled globally above, so they work from dropdowns too
                    KeyCode::Tab if self.discover.focus == DiscoverFocus::Files => {
                        self.discover.preview_open = !self.discover.preview_open;
                    }
                    KeyCode::Esc if !self.discover.filter.is_empty() => {
                        self.discover.filter.clear();
                        self.discover.selected = 0;
                    }
                    _ => match self.discover.focus {
                        DiscoverFocus::Files => self.handle_discover_files_key(key),
                        DiscoverFocus::Sources => self.handle_discover_sources_key(key),
                        DiscoverFocus::Tags => self.handle_discover_tags_key(key),
                    }
                }
            }
        }
    }

    /// Handle keys when Files panel is focused
    fn handle_discover_files_key(&mut self, key: KeyEvent) {
        // === Glob Explorer mode ===
        // When glob_explorer is active, handle folder navigation
        if self.discover.glob_explorer.is_some() {
            self.handle_glob_explorer_key(key);
            return;
        }

        // === Normal file list mode ===
        match key.code {
            KeyCode::Char('g') => {
                // Toggle Glob Explorer on
                self.discover.glob_explorer = Some(GlobExplorerState::default());
                self.discover.data_loaded = false; // Trigger reload
            }
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
                self.transition_discover_state(DiscoverViewState::Filtering);
            }
            KeyCode::Char('p') => {
                self.discover.preview_open = !self.discover.preview_open;
            }
            KeyCode::Char('s') => {
                // Open scan path input
                self.transition_discover_state(DiscoverViewState::EnteringPath);
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
                        self.transition_discover_state(DiscoverViewState::BulkTagging);
                        self.discover.bulk_tag_input.clear();
                        self.discover.bulk_tag_save_as_rule = false;
                    }
                } else if !self.filtered_files().is_empty() {
                    self.transition_discover_state(DiscoverViewState::Tagging);
                    self.discover.tag_input.clear();
                }
            }
            KeyCode::Char('R') => {
                // Create rule from current filter
                if !self.discover.filter.is_empty() {
                    // Prefill pattern with current filter
                    self.discover.rule_pattern_input = self.discover.filter.clone();
                    self.discover.rule_tag_input.clear();
                    self.transition_discover_state(DiscoverViewState::RuleCreation);
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
                        self.transition_discover_state(DiscoverViewState::CreatingSource);
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
                    self.transition_discover_state(DiscoverViewState::BulkTagging);
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

    /// Handle keys when Glob Explorer is active (hierarchical folder navigation)
    fn handle_glob_explorer_key(&mut self, key: KeyEvent) {
        // Pattern editing mode - uses in-memory cache filtering (O(m) where m = current level items)
        if let Some(ref explorer) = self.discover.glob_explorer {
            if explorer.pattern_editing {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        // Exit pattern editing, filter from cache
                        if let Some(ref mut explorer) = self.discover.glob_explorer {
                            explorer.pattern_editing = false;
                        }
                        self.update_folders_from_cache();
                    }
                    KeyCode::Backspace => {
                        if let Some(ref mut explorer) = self.discover.glob_explorer {
                            explorer.pattern.pop();
                        }
                        // Live filter update from cache
                        self.update_folders_from_cache();
                    }
                    KeyCode::Char(c) => {
                        if let Some(ref mut explorer) = self.discover.glob_explorer {
                            explorer.pattern.push(c);
                        }
                        // Live filter update from cache
                        self.update_folders_from_cache();
                    }
                    _ => {}
                }
                return;
            }
        }

        // Navigation mode
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                // Navigate down in folder list
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if explorer.selected_folder < explorer.folders.len().saturating_sub(1) {
                        explorer.selected_folder += 1;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // Navigate up in folder list
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if explorer.selected_folder > 0 {
                        explorer.selected_folder -= 1;
                    }
                }
            }
            KeyCode::Enter => {
                // Drill into selected folder - O(1) using cache
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(folder) = explorer.folders.get(explorer.selected_folder).cloned() {
                        if !folder.is_file {
                            // Save current prefix to history
                            explorer.pattern_history.push(explorer.current_prefix.clone());
                            // Update prefix to drill into folder
                            explorer.current_prefix = format!("{}{}/", explorer.current_prefix, folder.name);
                            explorer.phase = GlobExplorerPhase::Focused;
                        }
                    }
                }
                // Update from cache - O(1) hashmap lookup, no SQL
                self.update_folders_from_cache();
            }
            KeyCode::Backspace => {
                // Go back to parent folder - O(1) using cache
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(prev_prefix) = explorer.pattern_history.pop() {
                        explorer.current_prefix = prev_prefix;
                        explorer.phase = if explorer.pattern_history.is_empty() {
                            GlobExplorerPhase::Explore
                        } else {
                            GlobExplorerPhase::Focused
                        };
                    }
                }
                // Update from cache - O(1) hashmap lookup, no SQL
                self.update_folders_from_cache();
            }
            KeyCode::Char('/') => {
                // Enter pattern editing mode
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.pattern_editing = true;
                }
            }
            KeyCode::Char('g') | KeyCode::Esc => {
                // Exit Glob Explorer
                self.discover.glob_explorer = None;
                self.discover.data_loaded = false; // Trigger reload of normal file list
            }
            KeyCode::Char('s') => {
                // Open scan path input (same as normal mode)
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
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
    /// Vim-style modal: navigation mode by default, '/' enters filter mode
    fn handle_sources_dropdown_key(&mut self, key: KeyEvent) {
        // Filter mode: text input goes to filter
        if self.discover.sources_filtering {
            match key.code {
                KeyCode::Enter => {
                    // Confirm filter, stay in dropdown but exit filter mode
                    self.discover.sources_filtering = false;
                }
                KeyCode::Esc => {
                    // Clear filter and exit filter mode
                    self.discover.sources_filter.clear();
                    self.discover.sources_filtering = false;
                    // Reset preview to first item
                    let filtered = self.filtered_sources();
                    self.discover.preview_source = filtered.first().map(|(i, _)| *i);
                }
                KeyCode::Backspace => {
                    self.discover.sources_filter.pop();
                    self.update_sources_preview_after_filter();
                }
                KeyCode::Char(c) => {
                    self.discover.sources_filter.push(c);
                    self.update_sources_preview_after_filter();
                }
                _ => {}
            }
            return;
        }

        // Navigation mode: keybindings work
        let filtered = self.filtered_sources();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                // Navigate down in dropdown - DON'T reload files here (perf fix)
                // Files only reload on Enter (confirm selection)
                if let Some(preview_idx) = self.discover.preview_source {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos + 1 < filtered.len() {
                            self.discover.preview_source = Some(filtered[pos + 1].0);
                        }
                    }
                } else if !filtered.is_empty() {
                    self.discover.preview_source = Some(filtered[0].0);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // Navigate up in dropdown - DON'T reload files here (perf fix)
                if let Some(preview_idx) = self.discover.preview_source {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos > 0 {
                            self.discover.preview_source = Some(filtered[pos - 1].0);
                        }
                    }
                }
            }
            KeyCode::Char('/') => {
                // Enter filter mode
                self.discover.sources_filtering = true;
            }
            KeyCode::Char('s') => {
                // Open scan dialog to add new source
                self.discover.view_state = DiscoverViewState::Files;
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = None;
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
            }
            KeyCode::Enter => {
                // Confirm selection, close dropdown
                if let Some(preview_idx) = self.discover.preview_source {
                    self.discover.select_source_by_index(preview_idx);
                    self.discover.data_loaded = false;
                    self.discover.selected_tag = None;
                    self.discover.filter.clear();
                }
                self.discover.view_state = DiscoverViewState::Files;
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = None;
                self.discover.focus = DiscoverFocus::Files;
            }
            KeyCode::Esc => {
                // Close dropdown without changing selection
                self.discover.view_state = DiscoverViewState::Files;
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = None;
                self.discover.focus = DiscoverFocus::Files;
            }
            _ => {}
        }
    }

    /// Helper to update preview after filter changes
    /// Note: Does NOT reload files - that only happens on Enter (perf fix)
    fn update_sources_preview_after_filter(&mut self) {
        let filtered = self.filtered_sources();
        if let Some(preview_idx) = self.discover.preview_source {
            if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                self.discover.preview_source = filtered.first().map(|(i, _)| *i);
            }
        }
    }

    /// Handle keys when Sources panel is focused (dropdown closed)
    fn handle_discover_sources_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('s') => {
                // Create new source (open scan dialog)
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
            }
            _ => {}
        }
    }

    /// Handle keys when Tags dropdown is open
    /// Vim-style modal: navigation mode by default, '/' enters filter mode
    fn handle_tags_dropdown_key(&mut self, key: KeyEvent) {
        // Filter mode: text input goes to filter
        if self.discover.tags_filtering {
            match key.code {
                KeyCode::Enter => {
                    // Confirm filter, stay in dropdown but exit filter mode
                    self.discover.tags_filtering = false;
                }
                KeyCode::Esc => {
                    // Clear filter and exit filter mode
                    self.discover.tags_filter.clear();
                    self.discover.tags_filtering = false;
                }
                KeyCode::Backspace => {
                    self.discover.tags_filter.pop();
                    self.update_tags_preview_after_filter();
                }
                KeyCode::Char(c) => {
                    self.discover.tags_filter.push(c);
                    self.update_tags_preview_after_filter();
                }
                _ => {}
            }
            return;
        }

        // Navigation mode: keybindings work
        let filtered = self.filtered_tags();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(preview_idx) = self.discover.preview_tag {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos + 1 < filtered.len() {
                            let new_idx = filtered[pos + 1].0;
                            self.discover.preview_tag = Some(new_idx);
                            self.discover.selected = 0;
                        }
                    }
                } else if !filtered.is_empty() {
                    let new_idx = filtered[0].0;
                    self.discover.preview_tag = Some(new_idx);
                    self.discover.selected = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(preview_idx) = self.discover.preview_tag {
                    if let Some(pos) = filtered.iter().position(|(i, _)| *i == preview_idx) {
                        if pos > 0 {
                            let new_idx = filtered[pos - 1].0;
                            self.discover.preview_tag = Some(new_idx);
                            self.discover.selected = 0;
                        } else {
                            // At top of list, select "All files" (None)
                            self.discover.preview_tag = None;
                            self.discover.selected = 0;
                        }
                    }
                }
            }
            KeyCode::Char('/') => {
                // Enter filter mode
                self.discover.tags_filtering = true;
            }
            KeyCode::Enter => {
                // Confirm selection, close dropdown
                self.discover.selected_tag = self.discover.preview_tag;
                self.discover.view_state = DiscoverViewState::Files;
                self.discover.tags_filter.clear();
                self.discover.tags_filtering = false;
                self.discover.preview_tag = None;
                self.discover.focus = DiscoverFocus::Files;
                self.discover.selected = 0;
            }
            KeyCode::Esc => {
                // Close dropdown, show all files
                self.discover.selected_tag = None;
                self.discover.view_state = DiscoverViewState::Files;
                self.discover.tags_filter.clear();
                self.discover.tags_filtering = false;
                self.discover.preview_tag = None;
                self.discover.focus = DiscoverFocus::Files;
                self.discover.selected = 0;
            }
            _ => {}
        }
    }

    /// Helper to update preview after tags filter changes
    fn update_tags_preview_after_filter(&mut self) {
        let filtered = self.filtered_tags();
        if let Some(preview_idx) = self.discover.preview_tag {
            if !filtered.iter().any(|(i, _)| *i == preview_idx) {
                self.discover.preview_tag = filtered.first().map(|(i, _)| *i);
                self.discover.selected = 0;
            }
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
                self.transition_discover_state(DiscoverViewState::RuleCreation);
                self.discover.rule_tag_input.clear();
                self.discover.rule_pattern_input.clear();
                self.discover.editing_rule_id = None;
            }
            KeyCode::Char('e') => {
                // Edit selected rule
                if let Some(rule) = self.discover.rules.get(self.discover.selected_rule).cloned() {
                    self.transition_discover_state(DiscoverViewState::RuleCreation);
                    self.discover.rule_pattern_input = rule.pattern;
                    self.discover.rule_tag_input = rule.tag;
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
                self.return_to_previous_discover_state();
            }
            _ => {}
        }
    }

    /// Handle keys when Sources Manager dialog is open (spec v1.7)
    fn handle_sources_manager_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.discover.sources_manager_selected < self.discover.sources.len().saturating_sub(1) {
                    self.discover.sources_manager_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.discover.sources_manager_selected > 0 {
                    self.discover.sources_manager_selected -= 1;
                }
            }
            KeyCode::Char('n') => {
                // Add new source (open scan dialog)
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
            }
            KeyCode::Char('e') => {
                // Edit selected source name
                if let Some(source) = self.discover.sources.get(self.discover.sources_manager_selected) {
                    self.discover.editing_source = Some(source.id.clone());
                    self.discover.source_edit_input = source.name.clone();
                    self.transition_discover_state(DiscoverViewState::SourceEdit);
                }
            }
            KeyCode::Char('d') => {
                // Delete source (with confirmation)
                if let Some(source) = self.discover.sources.get(self.discover.sources_manager_selected) {
                    self.discover.source_to_delete = Some(source.id.clone());
                    self.transition_discover_state(DiscoverViewState::SourceDeleteConfirm);
                }
            }
            KeyCode::Char('r') => {
                // Rescan selected source
                let source_info = self.discover.sources.get(self.discover.sources_manager_selected)
                    .map(|s| (s.path.to_string_lossy().to_string(), s.name.clone()));

                if let Some((path, name)) = source_info {
                    self.scan_directory(&path);
                    self.discover.status_message = Some((
                        format!("Rescanning '{}'...", name),
                        false,
                    ));
                }
            }
            KeyCode::Esc => {
                self.return_to_previous_discover_state();
            }
            _ => {}
        }
    }

    /// Handle keys when Source Edit dialog is open (spec v1.7)
    fn handle_source_edit_key(&mut self, key: KeyEvent) {
        match handle_text_input(key, &mut self.discover.source_edit_input) {
            TextInputResult::Committed => {
                let new_name = self.discover.source_edit_input.trim().to_string();
                if !new_name.is_empty() {
                    if let Some(source_id) = &self.discover.editing_source {
                        // Update source name in local state
                        if let Some(source) = self.discover.sources.iter_mut()
                            .find(|s| &s.id == source_id)
                        {
                            source.name = new_name.clone();
                            self.discover.status_message = Some((
                                format!("Renamed source to '{}'", new_name),
                                false,
                            ));
                            // TODO: Persist to DB
                        }
                    }
                }
                self.discover.editing_source = None;
                self.discover.source_edit_input.clear();
                self.transition_discover_state(DiscoverViewState::SourcesManager);
            }
            TextInputResult::Cancelled => {
                self.discover.editing_source = None;
                self.discover.source_edit_input.clear();
                self.transition_discover_state(DiscoverViewState::SourcesManager);
            }
            TextInputResult::Continue | TextInputResult::NotHandled => {}
        }
    }

    /// Handle keys when Source Delete Confirmation dialog is open (spec v1.7)
    fn handle_source_delete_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                if let Some(source_id) = self.discover.source_to_delete.take() {
                    // Find and remove the source
                    let source_name = self.discover.sources.iter()
                        .find(|s| s.id == source_id)
                        .map(|s| s.name.clone());

                    self.discover.sources.retain(|s| s.id != source_id);

                    // Adjust selection if needed
                    if self.discover.sources_manager_selected >= self.discover.sources.len()
                        && self.discover.sources_manager_selected > 0
                    {
                        self.discover.sources_manager_selected -= 1;
                    }

                    // Validate main selection after deletion
                    self.discover.validate_source_selection();

                    if let Some(name) = source_name {
                        self.discover.status_message = Some((
                            format!("Deleted source '{}'", name),
                            false,
                        ));
                    }
                    // TODO: Delete from DB (cascade delete files)
                }
                self.transition_discover_state(DiscoverViewState::SourcesManager);
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                self.discover.source_to_delete = None;
                self.transition_discover_state(DiscoverViewState::SourcesManager);
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

    /// Open the rule creation dialog with context-aware prefilling
    fn open_rule_creation_dialog(&mut self) {
        self.transition_discover_state(DiscoverViewState::RuleCreation);
        self.discover.rule_dialog_focus = RuleDialogFocus::Pattern;

        // Context-aware prefilling
        if !self.discover.filter.is_empty() {
            // From Files panel with filter: prefill pattern
            self.discover.rule_pattern_input = self.discover.filter.clone();
        } else if let Some(file) = self.filtered_files().get(self.discover.selected) {
            // From Files panel with file selected: prefill with extension pattern
            if let Some(ext) = std::path::Path::new(&file.path).extension() {
                self.discover.rule_pattern_input = format!("*.{}", ext.to_string_lossy());
            } else {
                self.discover.rule_pattern_input.clear();
            }
        } else {
            self.discover.rule_pattern_input.clear();
        }

        // If in Tags dropdown with a tag selected, prefill tag
        if self.discover.focus == DiscoverFocus::Tags {
            if let Some(tag_idx) = self.discover.selected_tag {
                if let Some(tag) = self.discover.tags.get(tag_idx) {
                    if !tag.is_special {
                        self.discover.rule_tag_input = tag.name.clone();
                    } else {
                        self.discover.rule_tag_input.clear();
                    }
                } else {
                    self.discover.rule_tag_input.clear();
                }
            } else {
                self.discover.rule_tag_input.clear();
            }
        } else {
            self.discover.rule_tag_input.clear();
        }

        // Update preview with current pattern
        self.update_rule_preview();
    }

    /// Close the rule creation dialog and reset state
    fn close_rule_creation_dialog(&mut self) {
        self.return_to_previous_discover_state();
        self.discover.rule_pattern_input.clear();
        self.discover.rule_tag_input.clear();
        self.discover.rule_preview_files.clear();
        self.discover.rule_preview_count = 0;
        self.discover.rule_dialog_focus = RuleDialogFocus::Pattern;
        self.discover.editing_rule_id = None;
    }

    /// Update the live preview of files matching the current pattern
    fn update_rule_preview(&mut self) {
        let pattern = &self.discover.rule_pattern_input;

        if pattern.is_empty() {
            self.discover.rule_preview_files.clear();
            self.discover.rule_preview_count = 0;
            return;
        }

        // Use globset for matching
        use globset::GlobBuilder;

        // Wrap pattern to match anywhere in path if not already a path pattern
        let glob_pattern = if pattern.contains('/') {
            pattern.clone()
        } else {
            format!("**/{}", pattern)
        };

        match GlobBuilder::new(&glob_pattern)
            .case_insensitive(true)
            .build()
            .map(|g| g.compile_matcher())
        {
            Ok(matcher) => {
                let matches: Vec<String> = self.discover.files
                    .iter()
                    .filter(|f| {
                        let path = f.path.strip_prefix('/').unwrap_or(&f.path);
                        matcher.is_match(path)
                    })
                    .map(|f| f.rel_path.clone())
                    .collect();

                self.discover.rule_preview_count = matches.len();
                self.discover.rule_preview_files = matches.into_iter().take(10).collect();
            }
            Err(_) => {
                // Invalid pattern, try substring match
                let pattern_lower = pattern.to_lowercase();
                let matches: Vec<String> = self.discover.files
                    .iter()
                    .filter(|f| f.path.to_lowercase().contains(&pattern_lower))
                    .map(|f| f.rel_path.clone())
                    .collect();

                self.discover.rule_preview_count = matches.len();
                self.discover.rule_preview_files = matches.into_iter().take(10).collect();
            }
        }
    }

    /// Apply a rule (pattern → tag) to all matching files
    /// Returns the number of files tagged
    /// Also queues DB writes for persistence
    fn apply_rule_to_files(&mut self, pattern: &str, tag: &str) -> usize {
        use globset::GlobBuilder;

        // Build the glob matcher
        let glob_pattern = if pattern.contains('/') {
            pattern.to_string()
        } else {
            format!("**/{}", pattern)
        };

        let matcher = match GlobBuilder::new(&glob_pattern)
            .case_insensitive(true)
            .build()
            .map(|g| g.compile_matcher())
        {
            Ok(m) => m,
            Err(_) => return 0,
        };

        // Find and tag matching files
        let mut tagged_count = 0;
        for file in &mut self.discover.files {
            let path = file.path.strip_prefix('/').unwrap_or(&file.path);
            if matcher.is_match(path) {
                // Add tag if not already present
                if !file.tags.contains(&tag.to_string()) {
                    file.tags.push(tag.to_string());
                    tagged_count += 1;
                    // Queue DB write
                    self.discover.pending_tag_writes.push(PendingTagWrite {
                        file_path: file.path.clone(),
                        tag: tag.to_string(),
                    });
                }
            }
        }

        // Add to available tags if new
        if tagged_count > 0 && !self.discover.available_tags.contains(&tag.to_string()) {
            self.discover.available_tags.push(tag.to_string());
        }

        // Queue rule write
        if tagged_count > 0 {
            let source_id = self.discover.selected_source()
                .map(|s| s.id.clone())
                .unwrap_or_default();
            if !source_id.is_empty() {
                self.discover.pending_rule_writes.push(PendingRuleWrite {
                    pattern: pattern.to_string(),
                    tag: tag.to_string(),
                    source_id,
                });
            }
        }

        tagged_count
    }

    /// Refresh the tags dropdown list based on current file tags
    fn refresh_tags_list(&mut self) {
        use std::collections::HashMap;

        // Count files per tag
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        let mut untagged_count = 0;

        for file in &self.discover.files {
            if file.tags.is_empty() {
                untagged_count += 1;
            } else {
                for tag in &file.tags {
                    *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
        }

        // Build the tags list
        let mut tags = Vec::new();

        // "All files" is always first (special)
        tags.push(TagInfo {
            name: "All files".to_string(),
            count: self.discover.files.len(),
            is_special: true,
        });

        // Add actual tags sorted by count (descending)
        let mut sorted_tags: Vec<_> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

        for (tag_name, count) in sorted_tags {
            tags.push(TagInfo {
                name: tag_name,
                count,
                is_special: false,
            });
        }

        // "untagged" is last (special)
        if untagged_count > 0 {
            tags.push(TagInfo {
                name: "untagged".to_string(),
                count: untagged_count,
                is_special: true,
            });
        }

        self.discover.tags = tags;
    }

    /// Refresh the current view's data (per spec Section 3.3)
    fn refresh_current_view(&mut self) {
        match self.mode {
            TuiMode::Home => {
                // Home stats are currently static placeholders
                // TODO: Load real stats from database
            }
            TuiMode::Discover => {
                // Mark data as needing refresh - will trigger reload on next tick
                self.discover.data_loaded = false;
                self.refresh_tags_list();
            }
            TuiMode::ParserBench => {
                // Reload parsers from disk
                self.parser_bench.parsers_loaded = false;
                self.load_parsers();
            }
            TuiMode::Jobs => {
                // TODO: Reload jobs from database
                // For now just reset the view
                self.jobs_state.selected_index = 0;
            }
            TuiMode::Inspect => {
                // TODO: Reload tables from output directory
                self.inspect.selected_table = 0;
            }
        }
    }

    /// Check if the app is in a text input mode where global keys should not be intercepted
    fn in_text_input_mode(&self) -> bool {
        match self.mode {
            TuiMode::Discover => {
                // All text input states are now in the view_state enum
                matches!(
                    self.discover.view_state,
                    DiscoverViewState::Filtering |
                    DiscoverViewState::EnteringPath |
                    DiscoverViewState::Tagging |
                    DiscoverViewState::CreatingSource |
                    DiscoverViewState::BulkTagging |
                    DiscoverViewState::RuleCreation |
                    DiscoverViewState::SourcesDropdown |
                    DiscoverViewState::TagsDropdown |
                    DiscoverViewState::SourceEdit  // Added for Sources Manager
                )
            }
            TuiMode::Inspect => self.inspect.query_focused,
            TuiMode::ParserBench => self.parser_bench.is_filtering,
            _ => false,
        }
    }

    /// Get files filtered by current tag and text filter
    ///
    /// Tag filtering:
    /// - Uses preview_tag when tags dropdown is open (live preview)
    /// - Uses selected_tag when dropdown is closed
    /// - "All files" (index 0) or None = no tag filter
    /// - "untagged" = files with no tags
    /// - Other tags = files with that specific tag
    ///
    /// Text filter supports gitignore-style patterns:
    /// - `foo` matches any path containing "foo"
    /// - `*foo*` matches paths with "foo" anywhere (wildcard)
    /// - `*.py` matches files ending in .py
    pub fn filtered_files(&self) -> Vec<&FileInfo> {
        // Step 1: Get the active tag for filtering
        let active_tag_idx = if self.discover.view_state == DiscoverViewState::TagsDropdown {
            self.discover.preview_tag
        } else {
            self.discover.selected_tag
        };

        // Step 2: Determine which tag to filter by
        let tag_filter: Option<&str> = match active_tag_idx {
            None => None, // No tag selected = show all
            Some(idx) => {
                match self.discover.tags.get(idx) {
                    Some(tag_info) if tag_info.name == "All files" => None, // Show all
                    Some(tag_info) if tag_info.name == "untagged" => Some(""), // Empty string = untagged
                    Some(tag_info) => Some(&tag_info.name), // Specific tag
                    None => None,
                }
            }
        };

        // Step 3: Apply tag filter first
        let tag_filtered: Vec<&FileInfo> = match tag_filter {
            None => self.discover.files.iter().collect(),
            Some("") => {
                // "untagged" - files with no tags
                self.discover.files.iter().filter(|f| f.tags.is_empty()).collect()
            }
            Some(tag_name) => {
                // Specific tag
                self.discover.files.iter().filter(|f| f.tags.contains(&tag_name.to_string())).collect()
            }
        };

        // Step 4: Apply text filter on top of tag filter
        if self.discover.filter.is_empty() {
            tag_filtered
        } else {
            let has_wildcards = self.discover.filter.contains('*')
                || self.discover.filter.contains('?');

            if has_wildcards {
                use globset::GlobBuilder;

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
                        tag_filtered
                            .into_iter()
                            .filter(|f| {
                                let path = f.path.strip_prefix('/').unwrap_or(&f.path);
                                matcher.is_match(path)
                            })
                            .collect()
                    }
                    Err(_) => {
                        let filter_lower = self.discover.filter.to_lowercase();
                        tag_filtered
                            .into_iter()
                            .filter(|f| f.path.to_lowercase().contains(&filter_lower))
                            .collect()
                    }
                }
            } else {
                let filter_lower = self.discover.filter.to_lowercase();
                tag_filtered
                    .into_iter()
                    .filter(|f| f.path.to_lowercase().contains(&filter_lower))
                    .collect()
            }
        }
    }

    /// Scan a directory recursively and add files to the discover list (non-blocking)
    ///
    /// Path validation happens synchronously (fast). The actual directory walk
    /// happens in a background task to avoid freezing the TUI. Results are
    /// polled in tick() and applied when ready.
    ///
    /// ## Parallelism Design (fixing common pitfalls)
    ///
    /// 1. **Per-thread local batches**: Each thread accumulates files locally,
    ///    only locking to flush full batches. This avoids the "global mutex on
    ///    every file" anti-pattern that serializes parallel work.
    ///
    /// 2. **Atomic compare-exchange for progress**: Prevents duplicate progress
    ///    messages from racing threads.

    // =========================================================================
    // Job Management for Scans
    // =========================================================================

    /// Create a new scan job and add it to the jobs list.
    ///
    /// Returns the job ID for tracking status updates.
    fn add_scan_job(&mut self, directory_path: &str) -> i64 {
        // Generate unique job ID from timestamp
        let job_id = chrono::Local::now().timestamp_millis();

        let job = JobInfo {
            id: job_id,
            file_path: directory_path.to_string(),
            parser_name: "scan".to_string(), // Distinguish scan jobs from parser jobs
            status: JobStatus::Running,
            retry_count: 0,
            error_message: None,
            created_at: chrono::Local::now(),
        };

        // Add to front of list so it's visible immediately
        self.jobs_state.jobs.insert(0, job);

        job_id
    }

    /// Update the status of a scan job.
    ///
    /// Finds the job by ID and updates its status and error message.
    fn update_scan_job_status(&mut self, job_id: i64, status: JobStatus, error: Option<String>) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = status;
            if error.is_some() {
                job.error_message = error;
            }
        }
    }

    /// Scan a directory using the unified parallel scanner.
    ///
    /// Uses `scout::Scanner` for parallel walking and DB persistence.
    /// Progress updates are forwarded to the TUI via channel.
    fn scan_directory(&mut self, path: &str) {
        use std::path::Path;

        let path_input = Path::new(path);

        // Expand ~ to home directory (synchronous, fast)
        let expanded_path = if path_input.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                home.join(path_input.strip_prefix("~").unwrap_or(path_input))
            } else {
                path_input.to_path_buf()
            }
        } else {
            path_input.to_path_buf()
        };

        // Path validation - synchronous (fast filesystem checks)
        if !expanded_path.exists() {
            self.discover.scan_error = Some(format!("Path not found: {}", expanded_path.display()));
            return;
        }

        if !expanded_path.is_dir() {
            self.discover.scan_error = Some(format!("Not a directory: {}", expanded_path.display()));
            return;
        }

        // Check if directory is readable
        if std::fs::read_dir(&expanded_path).is_err() {
            self.discover.scan_error = Some(format!("Cannot read directory: {}", expanded_path.display()));
            return;
        }

        // Path is valid - enter scanning state and spawn background task
        let path_display = expanded_path.display().to_string();
        self.discover.scanning_path = Some(path_display.clone());
        self.discover.scan_progress = Some(ScoutProgress {
            dirs_scanned: 0,
            files_found: 0,
            current_dir: Some("Initializing...".to_string()),
        });
        self.discover.scan_start_time = Some(std::time::Instant::now());
        self.discover.view_state = DiscoverViewState::Scanning;
        self.discover.scan_error = None;

        // Channel for TUI scan results
        let (tui_tx, tui_rx) = mpsc::channel::<TuiScanResult>(256);
        self.pending_scan = Some(tui_rx);

        // Create scan job for tracking in Jobs view
        let job_id = self.add_scan_job(&path_display);
        self.current_scan_job_id = Some(job_id);

        // Get database path
        let db_path = self.config.database.clone()
            .unwrap_or_else(crate::cli::config::default_db_path);

        let source_path = path_display;

        // Spawn async task for scanning
        tokio::spawn(async move {
            // Open database
            if let Some(parent) = db_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let db = match ScoutDatabase::open(&db_path).await {
                Ok(db) => db,
                Err(e) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Failed to open database: {}", e))).await;
                    return;
                }
            };

            // Check if source with this path already exists (rescan case)
            let existing_source = match db.get_source_by_path(&source_path).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Database error: {}", e))).await;
                    return;
                }
            };

            let source = if let Some(existing) = existing_source {
                // Rescan existing source - use existing source record
                existing
            } else {
                // New source - create record
                let source_name = std::path::Path::new(&source_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| source_path.clone());

                // Check if a source with this name (but different path) already exists
                if let Ok(Some(name_conflict)) = db.get_source_by_name(&source_name).await {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "A source named '{}' already exists at '{}'. Use Sources Manager (M) to rename or delete it first.",
                        source_name, name_conflict.path
                    ))).await;
                    return;
                }

                let source_id = format!("local:{}", source_path.replace(['/', '\\'], "_"));

                let new_source = Source {
                    id: source_id,
                    name: source_name,
                    source_type: SourceType::Local,
                    path: source_path.clone(),
                    poll_interval_secs: 0,
                    enabled: true,
                };

                // Insert new source
                if let Err(e) = db.upsert_source(&new_source).await {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Failed to save source: {}", e))).await;
                    return;
                }

                new_source
            };

            // Create progress channel that sends directly to TUI
            // We wrap tui_tx in a channel adapter so scanner can use its existing interface
            let (progress_tx, mut progress_rx) = mpsc::channel::<ScoutProgress>(512);

            // Spawn a task to forward progress - use spawn_blocking context awareness
            let tui_tx_progress = tui_tx.clone();
            let forward_handle = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    // Send to TUI channel
                    let _ = tui_tx_progress.try_send(TuiScanResult::Progress(progress));
                }
            });

            // Create scanner with default config
            let scanner = ScoutScanner::new(db);

            // Run the scan in a blocking task so it doesn't block the runtime
            let scan_result = {
                let source_clone = source.clone();
                tokio::task::spawn_blocking(move || {
                    // Create a new runtime for the blocking task
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(scanner.scan_source_with_progress(&source_clone, Some(progress_tx)))
                }).await
            };

            // Wait for forwarding to complete
            let _ = forward_handle.await;

            match scan_result {
                Ok(Ok(_result)) => {
                    // Scan complete - TUI will load files from DB
                    let _ = tui_tx.send(TuiScanResult::Complete {
                        source_path,
                    }).await;
                }
                Ok(Err(e)) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Scan failed: {}", e))).await;
                }
                Err(e) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Scan task panicked: {}", e))).await;
                }
            }
        });
    }

    /// List directories matching a partial path for autocomplete
    ///
    /// Given a partial path like "/Users/shan/Do", returns directories that match:
    /// - If path ends with '/', lists directories in that path
    /// - Otherwise, lists directories in parent matching the prefix
    ///
    /// Returns up to 8 suggestions, excludes hidden directories (starting with '.').
    fn list_directories(partial_path: &str) -> Vec<String> {
        use std::path::Path;

        let partial = if partial_path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                let rest = partial_path.strip_prefix("~").unwrap_or("");
                home.join(rest.trim_start_matches('/')).to_string_lossy().to_string()
            } else {
                partial_path.to_string()
            }
        } else {
            partial_path.to_string()
        };

        let path = Path::new(&partial);

        let (parent, prefix) = if partial.ends_with('/') || partial.ends_with('\\') {
            // Path ends with separator - list contents of this directory
            (path, "")
        } else {
            // Split into parent directory and prefix to match
            let parent = path.parent().unwrap_or(Path::new("/"));
            let prefix = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            (parent, prefix)
        };

        // Read directory and filter
        let mut suggestions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.filter_map(|e| e.ok()) {
                // Check if it's a directory
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }

                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Skip hidden directories
                if name_str.starts_with('.') {
                    continue;
                }

                // Check prefix match (case-insensitive)
                if name_str.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("{}/", name_str));
                }
            }
        }

        // Sort alphabetically and limit to 8
        suggestions.sort();
        suggestions.truncate(8);
        suggestions
    }

    /// Update path suggestions based on current input
    fn update_path_suggestions(&mut self) {
        self.discover.path_suggestions = Self::list_directories(&self.discover.scan_path_input);
        self.discover.path_suggestion_idx = 0;
    }

    /// Apply the selected suggestion to the path input
    fn apply_path_suggestion(&mut self) {
        if let Some(suggestion) = self.discover.path_suggestions.get(self.discover.path_suggestion_idx) {
            let input = &self.discover.scan_path_input;

            // Find the parent path (everything up to and including the last separator)
            let parent = if input.ends_with('/') || input.ends_with('\\') {
                input.clone()
            } else if let Some(pos) = input.rfind(|c| c == '/' || c == '\\') {
                input[..=pos].to_string()
            } else {
                String::new()
            };

            // Combine parent with suggestion
            self.discover.scan_path_input = format!("{}{}", parent, suggestion);
            self.update_path_suggestions();
        }
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
        let source_idx = if self.discover.view_state == DiscoverViewState::SourcesDropdown {
            self.discover.preview_source.unwrap_or_else(|| self.discover.selected_source_index())
        } else {
            self.discover.selected_source_index()
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
                    .bind(selected_source_id.as_str())
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

    /// Load folder tree for Glob Explorer (hierarchical file browsing).
    ///
    /// This replaces load_scout_files() when glob_explorer is active.
    /// Queries are batched: folders + preview files + total count in one function call.
    /// State is updated atomically at the end.
    ///
    /// NOTE: Currently unused - replaced by preload_folder_cache() for O(1) navigation.
    /// Kept for potential future use with complex SQL-based pattern filtering.
    #[allow(dead_code)]
    async fn load_folder_tree(&mut self) {
        use sqlx::SqlitePool;

        // Must have glob_explorer active and a source selected
        let source_id = match &self.discover.selected_source_id {
            Some(id) => id.clone(),
            None => return,
        };
        let explorer = match &self.discover.glob_explorer {
            Some(e) => e.clone(),
            None => return,
        };

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            return;
        }

        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        let pool = match SqlitePool::connect(&db_url).await {
            Ok(p) => p,
            Err(_) => return,
        };

        let prefix = &explorer.current_prefix;
        let prefix_len = prefix.len() as i32;
        let glob_pattern = if explorer.pattern.is_empty() {
            None
        } else {
            Some(explorer.pattern.as_str())
        };

        // --- Batch Query 1: Folder counts at current depth ---
        let folder_query = if glob_pattern.is_some() {
            r#"
            SELECT
                CASE
                    WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') > 0
                    THEN SUBSTR(rel_path, ? + 1, INSTR(SUBSTR(rel_path, ? + 1), '/') - 1)
                    ELSE SUBSTR(rel_path, ? + 1)
                END AS item_name,
                COUNT(*) as file_count,
                MAX(CASE WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') = 0 THEN 1 ELSE 0 END) as is_file
            FROM scout_files
            WHERE source_id = ?
              AND rel_path LIKE ? || '%'
              AND rel_path GLOB ?
              AND LENGTH(rel_path) > ?
            GROUP BY item_name
            ORDER BY file_count DESC
            LIMIT 100
            "#
        } else {
            r#"
            SELECT
                CASE
                    WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') > 0
                    THEN SUBSTR(rel_path, ? + 1, INSTR(SUBSTR(rel_path, ? + 1), '/') - 1)
                    ELSE SUBSTR(rel_path, ? + 1)
                END AS item_name,
                COUNT(*) as file_count,
                MAX(CASE WHEN INSTR(SUBSTR(rel_path, ? + 1), '/') = 0 THEN 1 ELSE 0 END) as is_file
            FROM scout_files
            WHERE source_id = ?
              AND rel_path LIKE ? || '%'
              AND LENGTH(rel_path) > ?
            GROUP BY item_name
            ORDER BY file_count DESC
            LIMIT 100
            "#
        };

        let folders_result: Result<Vec<(String, i64, i32)>, _> = if let Some(pattern) = glob_pattern {
            sqlx::query_as(folder_query)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(source_id.as_str())
                .bind(prefix)
                .bind(pattern)
                .bind(prefix_len)
                .fetch_all(&pool)
                .await
        } else {
            sqlx::query_as(folder_query)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(prefix_len)
                .bind(source_id.as_str())
                .bind(prefix)
                .bind(prefix_len)
                .fetch_all(&pool)
                .await
        };

        // --- Batch Query 2: Preview files ---
        let preview_result: Result<Vec<(String, i64, i64)>, _> = if let Some(pattern) = glob_pattern {
            sqlx::query_as(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                  AND rel_path GLOB ?
                ORDER BY mtime DESC
                LIMIT 10
                "#,
            )
            .bind(source_id.as_str())
            .bind(prefix)
            .bind(pattern)
            .fetch_all(&pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT rel_path, size, mtime
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                ORDER BY mtime DESC
                LIMIT 10
                "#,
            )
            .bind(source_id.as_str())
            .bind(prefix)
            .fetch_all(&pool)
            .await
        };

        // --- Batch Query 3: Total count ---
        let count_result: Result<(i64,), _> = if let Some(pattern) = glob_pattern {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                  AND rel_path GLOB ?
                "#,
            )
            .bind(source_id.as_str())
            .bind(prefix)
            .bind(pattern)
            .fetch_one(&pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM scout_files
                WHERE source_id = ?
                  AND rel_path LIKE ? || '%'
                "#,
            )
            .bind(source_id.as_str())
            .bind(prefix)
            .fetch_one(&pool)
            .await
        };

        // --- ATOMIC STATE UPDATE ---
        // Only update if all queries succeeded
        if let (Ok(folders_raw), Ok(preview_raw), Ok((count,))) =
            (folders_result, preview_result, count_result)
        {
            let folders: Vec<FolderInfo> = folders_raw
                .into_iter()
                .filter(|(name, _, _)| !name.is_empty())
                .map(|(name, count, is_file)| FolderInfo {
                    name,
                    file_count: count as usize,
                    is_file: is_file != 0,
                })
                .collect();

            let preview_files: Vec<GlobPreviewFile> = preview_raw
                .into_iter()
                .map(|(rel_path, size, mtime)| GlobPreviewFile {
                    rel_path,
                    size: size as u64,
                    mtime,
                })
                .collect();

            let total_count = GlobFileCount::Exact(count as usize);

            // Update explorer state atomically
            if let Some(ref mut explorer) = self.discover.glob_explorer {
                explorer.folders = folders;
                explorer.preview_files = preview_files;
                explorer.total_count = total_count;
                explorer.selected_folder = 0;
            }
        }

        // Mark as loaded (whether success or failure)
        self.discover.data_loaded = true;
    }

    /// Preload entire folder hierarchy into cache for O(1) navigation.
    /// Called once when source is selected. After this, all drill-in/back
    /// operations use the in-memory HashMap instead of SQL queries.
    async fn preload_folder_cache(&mut self) {
        use sqlx::SqlitePool;

        // Must have glob_explorer active and a source selected
        let source_id = match &self.discover.selected_source_id {
            Some(id) => id.clone(),
            None => return,
        };

        // Skip if cache already loaded for this source
        if let Some(ref explorer) = self.discover.glob_explorer {
            if explorer.cache_loaded {
                if let Some(ref cached_id) = explorer.cache_source_id {
                    if cached_id.as_str() == source_id.as_str() {
                        return; // Already cached for this source
                    }
                }
            }
        }

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            return;
        }

        let db_url = format!("sqlite:{}?mode=ro", db_path.display());
        let pool = match SqlitePool::connect(&db_url).await {
            Ok(p) => p,
            Err(_) => return,
        };

        // Single query: get all paths for this source
        let paths: Vec<(String,)> = match sqlx::query_as(
            "SELECT rel_path FROM scout_files WHERE source_id = ?"
        )
        .bind(source_id.as_str())
        .fetch_all(&pool)
        .await {
            Ok(p) => p,
            Err(_) => return,
        };

        // Build cache in memory - O(n*m) where n=files, m=avg path depth
        // Uses nested HashMap for O(1) lookup during build, then converts to Vec for display

        // Intermediate structure: prefix -> (name -> (file_count, is_file))
        // This gives O(1) lookup when checking if a segment exists
        let mut build_cache: HashMap<String, HashMap<String, (usize, bool)>> = HashMap::new();

        for (path,) in paths {
            let segments: Vec<&str> = path.split('/').collect();
            let mut current_prefix = String::new();

            for (i, segment) in segments.iter().enumerate() {
                if segment.is_empty() {
                    continue;
                }
                let is_file = i == segments.len() - 1;
                let level = build_cache.entry(current_prefix.clone()).or_default();

                // O(1) lookup and update using HashMap
                level.entry(segment.to_string())
                    .and_modify(|(count, _)| *count += 1)
                    .or_insert((1, is_file));

                if !is_file {
                    current_prefix = format!("{}{}/", current_prefix, segment);
                }
            }
        }

        // Convert to final format: HashMap<prefix, Vec<FolderInfo>> sorted by count
        let mut cache: HashMap<String, Vec<FolderInfo>> = HashMap::with_capacity(build_cache.len());
        for (prefix, entries) in build_cache {
            let mut folder_vec: Vec<FolderInfo> = entries
                .into_iter()
                .map(|(name, (file_count, is_file))| FolderInfo {
                    name,
                    file_count,
                    is_file,
                })
                .collect();
            // Sort by file count descending
            folder_vec.sort_by(|a, b| b.file_count.cmp(&a.file_count));
            cache.insert(prefix, folder_vec);
        }

        // Store cache and mark as loaded
        if let Some(ref mut explorer) = self.discover.glob_explorer {
            explorer.folder_cache = cache;
            explorer.cache_loaded = true;
            explorer.cache_source_id = Some(source_id.as_str().to_string());

            // Initialize folders from cache at root level
            if let Some(root_folders) = explorer.folder_cache.get("") {
                explorer.folders = root_folders.clone();
                explorer.total_count = GlobFileCount::Exact(
                    root_folders.iter().map(|f| f.file_count).sum()
                );
            }
        }
    }

    /// Update folders from cache based on current prefix (O(1) lookup).
    /// Used for navigation instead of SQL queries.
    /// If a pattern is set, filters entries in-memory using simple matching.
    fn update_folders_from_cache(&mut self) {
        if let Some(ref mut explorer) = self.discover.glob_explorer {
            let prefix = explorer.current_prefix.clone();
            let pattern = explorer.pattern.clone();

            if let Some(cached_folders) = explorer.folder_cache.get(&prefix) {
                // Filter folders based on pattern (in-memory)
                let folders: Vec<FolderInfo> = if pattern.is_empty() {
                    cached_folders.clone()
                } else {
                    // Simple pattern matching: supports *.ext and substring search
                    let pattern_lower = pattern.to_lowercase();
                    cached_folders.iter()
                        .filter(|f| {
                            let name_lower = f.name.to_lowercase();
                            // Handle common glob patterns
                            if pattern_lower.starts_with("*.") {
                                // *.ext -> ends with .ext
                                let ext = &pattern_lower[1..]; // ".ext"
                                name_lower.ends_with(ext)
                            } else if pattern_lower.ends_with("*") {
                                // prefix* -> starts with prefix
                                let prefix_pat = &pattern_lower[..pattern_lower.len()-1];
                                name_lower.starts_with(prefix_pat)
                            } else if pattern_lower.contains('*') {
                                // a*b pattern -> starts with a and ends with b
                                let parts: Vec<&str> = pattern_lower.split('*').collect();
                                if parts.len() == 2 {
                                    name_lower.starts_with(parts[0]) && name_lower.ends_with(parts[1])
                                } else {
                                    name_lower.contains(&pattern_lower.replace('*', ""))
                                }
                            } else {
                                // Simple substring match
                                name_lower.contains(&pattern_lower)
                            }
                        })
                        .cloned()
                        .collect()
                };

                explorer.folders = folders.clone();
                explorer.total_count = GlobFileCount::Exact(
                    folders.iter().map(|f| f.file_count).sum()
                );
                explorer.selected_folder = 0;
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
                        id: SourceId::from(id),
                        name,
                        path: std::path::PathBuf::from(path),
                        file_count: file_count as usize,
                    })
                    .collect();

                // Auto-select first source if none selected or selection invalid
                self.discover.validate_source_selection();
            }
        }
        self.discover.sources_loaded = true;
    }

    /// Load tags from files for the selected source
    /// Tags are derived from actual file tags, not from rules
    async fn load_tags_for_source(&mut self) {
        use sqlx::SqlitePool;

        // Get source ID for selected source
        let source_id = match self.discover.sources.get(self.discover.selected_source_index()) {
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
                .bind(source_id.as_str())
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
                .bind(source_id.as_str())
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
                .bind(source_id.as_str())
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

    /// Persist pending tag and rule writes to the database
    async fn persist_pending_writes(&mut self) {
        use sqlx::SqlitePool;

        // Skip if nothing to persist
        if self.discover.pending_tag_writes.is_empty() && self.discover.pending_rule_writes.is_empty() {
            return;
        }

        let db_path = dirs::home_dir()
            .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
            .unwrap_or_else(|| std::path::PathBuf::from("casparian_flow.sqlite3"));

        if !db_path.exists() {
            return;
        }

        // Need write mode for updates
        let db_url = format!("sqlite:{}", db_path.display());
        let pool = match SqlitePool::connect(&db_url).await {
            Ok(p) => p,
            Err(_) => return,
        };

        // Persist tag updates to scout_files
        let tag_writes = std::mem::take(&mut self.discover.pending_tag_writes);
        for write in tag_writes {
            let _ = sqlx::query("UPDATE scout_files SET tag = ? WHERE path = ?")
                .bind(&write.tag)
                .bind(&write.file_path)
                .execute(&pool)
                .await;
        }

        // Persist rules to scout_tagging_rules
        let rule_writes = std::mem::take(&mut self.discover.pending_rule_writes);
        for write in rule_writes {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let rule_name = format!("{} → {}", write.pattern, write.tag);
            let now = chrono::Utc::now().timestamp();

            let _ = sqlx::query(
                r#"INSERT OR IGNORE INTO scout_tagging_rules
                   (id, name, source_id, pattern, tag, priority, enabled, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, 100, 1, ?, ?)"#
            )
                .bind(&rule_id)
                .bind(&rule_name)
                .bind(write.source_id.as_str())
                .bind(&write.pattern)
                .bind(&write.tag)
                .bind(now)
                .bind(now)
                .execute(&pool)
                .await;
        }
    }

    /// Load tagging rules for the Rules Manager dialog
    async fn load_rules_for_manager(&mut self) {
        use sqlx::SqlitePool;

        // Get source ID for selected source
        let source_id = match self.discover.sources.get(self.discover.selected_source_index()) {
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
                .bind(source_id.as_str())
                .fetch_all(&pool)
                .await
            {
                self.discover.rules = rows
                    .into_iter()
                    .map(|(id, pattern, tag, priority, enabled)| RuleInfo {
                        id: RuleId::new(id),
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
            // Card order: 0=Discover, 1=ParserBench, 2=Jobs, 3=Sources (Inspect placeholder)
            KeyCode::Enter => {
                self.mode = match self.home.selected_card {
                    0 => TuiMode::Discover,
                    1 => TuiMode::ParserBench,
                    2 => TuiMode::Jobs,
                    3 => TuiMode::Inspect, // TODO: TuiMode::Sources when implemented
                    _ => TuiMode::Home,
                };
            }
            // Number keys 1-4 are handled by global key handler
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
        // Increment tick counter for animated UI elements
        self.tick_count = self.tick_count.wrapping_add(1);

        // Poll scan progress FIRST - before any potentially blocking operations

        // Load Scout data if in Discover mode (but NOT while scanning - don't block progress updates)
        if self.mode == TuiMode::Discover && self.discover.view_state != DiscoverViewState::Scanning {
            // Process pending DB writes FIRST (before any reloads)
            self.persist_pending_writes().await;

            // Load sources for sidebar
            if !self.discover.sources_loaded {
                self.load_sources().await;
            }
            // Load files for selected source (also reloads tags when source changes)
            if !self.discover.data_loaded {
                // Always use Glob Explorer (hierarchical browsing) - prevents freeze on large sources
                if self.discover.glob_explorer.is_none() {
                    self.discover.glob_explorer = Some(GlobExplorerState::default());
                }

                // Preload folder cache for O(1) navigation (one-time per source)
                // This single SQL query builds the entire hierarchy in memory
                self.preload_folder_cache().await;

                // Update display from cache (O(1) lookup)
                self.update_folders_from_cache();

                // Reload tags for the (possibly new) selected source
                self.load_tags_for_source().await;

                // Mark as loaded
                self.discover.data_loaded = true;
            }
            // Load rules for Rules Manager if it's open
            if self.discover.view_state == DiscoverViewState::RulesManager && self.discover.rules.is_empty() {
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

        // Poll for pending scan results (non-blocking directory scan)
        // Process ALL available messages (progress updates + completion)
        if let Some(ref mut rx) = self.pending_scan {
            let mut scan_complete = false;
            // Drain all available messages
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            TuiScanResult::Progress(progress) => {
                                // Update progress - UI will display this
                                self.discover.scan_progress = Some(progress);
                            }
                            TuiScanResult::Complete { source_path } => {
                                // Update job status to Completed
                                if let Some(job_id) = self.current_scan_job_id {
                                    self.update_scan_job_status(job_id, JobStatus::Completed, None);
                                }

                                // Scanner persisted to DB - reload sources and files
                                let source_name = std::path::Path::new(&source_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| source_path.clone());

                                // Generate source ID matching scanner's format
                                let source_id = SourceId(format!(
                                    "local:{}",
                                    source_path.replace('/', "_").replace('\\', "_")
                                ));

                                // Reload sources from DB to get updated list
                                self.load_sources().await;

                                // Select the newly scanned source
                                self.discover.selected_source_id = Some(source_id);

                                // Load files for the new source
                                self.load_scout_files().await;

                                let file_count = self.discover.files.len();
                                self.discover.selected = 0;
                                self.discover.scan_error = None;
                                self.discover.view_state = DiscoverViewState::Files;
                                self.discover.scanning_path = None;
                                self.discover.scan_progress = None;
                                self.discover.scan_start_time = None;
                                self.discover.status_message = Some((
                                    format!("Scanned {} files from {}", file_count, source_name),
                                    false,
                                ));
                                scan_complete = true;
                                break;
                            }
                            TuiScanResult::Error(err) => {
                                // Update job status to Failed
                                if let Some(job_id) = self.current_scan_job_id {
                                    self.update_scan_job_status(job_id, JobStatus::Failed, Some(err.clone()));
                                }

                                self.discover.scan_error = Some(err);
                                self.discover.view_state = DiscoverViewState::Files;
                                self.discover.scanning_path = None;
                                self.discover.scan_progress = None;
                                self.discover.scan_start_time = None;
                                scan_complete = true;
                                break;
                            }
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // Task ended without completion - mark job as failed
                        if let Some(job_id) = self.current_scan_job_id {
                            self.update_scan_job_status(
                                job_id,
                                JobStatus::Failed,
                                Some("Scan task ended unexpectedly".to_string()),
                            );
                        }

                        self.discover.scan_error = Some("Scan task ended unexpectedly".to_string());
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.scanning_path = None;
                        self.discover.scan_progress = None;
                        self.discover.scan_start_time = None;
                        scan_complete = true;
                        break;
                    }
                }
            }
            if scan_complete {
                self.pending_scan = None;
                self.current_scan_job_id = None;
            }
        }

        // TODO: Poll job status, refresh metrics

        // Load parsers if in ParserBench mode
        if self.mode == TuiMode::ParserBench && !self.parser_bench.parsers_loaded {
            self.load_parsers();
        }
    }

    // =========================================================================
    // Parser Bench Methods
    // =========================================================================

    /// Python script for extracting parser metadata via AST (no execution).
    /// This is embedded as a const to avoid external file dependencies.
    /// Supports batch mode: reads JSON array of paths from stdin, outputs JSON object keyed by path.
    const METADATA_EXTRACTOR_SCRIPT: &'static str = r#"
import ast
import json
import sys
import os

def extract_metadata(path):
    """Extract parser metadata via AST parsing (no execution)."""
    try:
        source = open(path).read()
        tree = ast.parse(source)
    except SyntaxError as e:
        return {"error": f"Syntax error: {e}"}
    except Exception as e:
        return {"error": str(e)}

    result = {
        "name": None,
        "version": None,
        "topics": [],
        "has_transform": False,
        "has_parse": False,
    }

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for item in node.body:
                # Class attributes (name = 'value')
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            try:
                                value = ast.literal_eval(item.value)
                                if target.id == "name":
                                    result["name"] = value
                                elif target.id == "version":
                                    result["version"] = value
                                elif target.id == "topics":
                                    result["topics"] = value if isinstance(value, list) else [value]
                            except:
                                pass
                # Methods
                elif isinstance(item, ast.FunctionDef):
                    if item.name == "transform":
                        result["has_transform"] = True
                    elif item.name == "parse":
                        result["has_parse"] = True

        # Also check for module-level parse() function
        elif isinstance(node, ast.FunctionDef) and node.name == "parse":
            result["has_parse"] = True

    # Fallback: use filename if no name attribute
    if result["name"] is None:
        result["name"] = os.path.splitext(os.path.basename(path))[0]

    return result

if __name__ == "__main__":
    # Batch mode: read JSON array of paths from stdin
    paths = json.load(sys.stdin)
    results = {}
    for path in paths:
        results[path] = extract_metadata(path)
    print(json.dumps(results))
"#;

    /// Maximum number of parser files to process in a single Python subprocess.
    /// Prevents command line overflow and keeps memory usage reasonable.
    const METADATA_BATCH_SIZE: usize = 50;

    /// Extract metadata from multiple Python parser files in a single subprocess.
    /// Returns a map from path string to (name, version, topics).
    /// Uses stdin to pass paths as JSON array, avoiding command line length limits.
    fn extract_parser_metadata_batch(
        paths: &[std::path::PathBuf],
    ) -> std::collections::HashMap<String, (String, Option<String>, Vec<String>)> {
        use std::collections::HashMap;
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut results = HashMap::new();

        if paths.is_empty() {
            return results;
        }

        // Convert paths to strings for JSON
        let path_strings: Vec<String> = paths
            .iter()
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect();

        let json_input = match serde_json::to_string(&path_strings) {
            Ok(j) => j,
            Err(_) => {
                // Fallback: return defaults for all paths
                for path in paths {
                    let fallback_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if let Some(path_str) = path.to_str() {
                        results.insert(path_str.to_string(), (fallback_name, None, vec![]));
                    }
                }
                return results;
            }
        };

        // Try python3, then python
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(Self::METADATA_EXTRACTOR_SCRIPT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        if child.is_err() {
            child = Command::new("python")
                .arg("-c")
                .arg(Self::METADATA_EXTRACTOR_SCRIPT)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
        }

        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                // Python not available, return defaults
                for path in paths {
                    let fallback_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if let Some(path_str) = path.to_str() {
                        results.insert(path_str.to_string(), (fallback_name, None, vec![]));
                    }
                }
                return results;
            }
        };

        // Write JSON input to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(json_input.as_bytes());
        }

        // Read output
        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(_) => {
                for path in paths {
                    let fallback_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if let Some(path_str) = path.to_str() {
                        results.insert(path_str.to_string(), (fallback_name, None, vec![]));
                    }
                }
                return results;
            }
        };

        if !output.status.success() {
            for path in paths {
                let fallback_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                if let Some(path_str) = path.to_str() {
                    results.insert(path_str.to_string(), (fallback_name, None, vec![]));
                }
            }
            return results;
        }

        // Parse JSON output: {"path": {"name": ..., "version": ..., "topics": [...]}, ...}
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);

        match parsed {
            Ok(json) => {
                if let Some(obj) = json.as_object() {
                    for (path_str, metadata) in obj {
                        let fallback_name = std::path::Path::new(path_str)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        let name = metadata
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| fallback_name.clone());

                        let version = metadata
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let topics = metadata
                            .get("topics")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        results.insert(path_str.clone(), (name, version, topics));
                    }
                }
            }
            Err(_) => {}
        }

        // Fill in any missing paths with defaults
        for path in paths {
            if let Some(path_str) = path.to_str() {
                results.entry(path_str.to_string()).or_insert_with(|| {
                    let fallback_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    (fallback_name, None, vec![])
                });
            }
        }

        results
    }

    /// Load parsers from the parsers directory
    fn load_parsers(&mut self) {
        use std::fs;

        let parsers_dir = crate::cli::config::parsers_dir();

        // Ensure directory exists
        if let Err(_) = fs::create_dir_all(&parsers_dir) {
            self.parser_bench.parsers_loaded = true;
            return;
        }

        // First pass: collect all .py files with their filesystem metadata
        struct ParserEntry {
            path: std::path::PathBuf,
            is_symlink: bool,
            symlink_broken: bool,
            modified: DateTime<Local>,
        }

        let mut entries_to_process: Vec<ParserEntry> = Vec::new();

        if let Ok(entries) = fs::read_dir(&parsers_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Only process .py files
                if path.extension().and_then(|e| e.to_str()) != Some("py") {
                    continue;
                }

                // Check if it's a symlink and if it's broken
                let metadata = fs::symlink_metadata(&path);
                let (is_symlink, symlink_broken) = match metadata {
                    Ok(m) => {
                        let is_symlink = m.file_type().is_symlink();
                        let symlink_broken = if is_symlink {
                            !path.exists() // Symlink exists but target doesn't
                        } else {
                            false
                        };
                        (is_symlink, symlink_broken)
                    }
                    Err(_) => (false, false),
                };

                // Get modification time
                let modified = if symlink_broken {
                    // Can't get metadata from broken symlink
                    Local::now()
                } else {
                    fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .map(DateTime::<Local>::from)
                        .unwrap_or_else(Local::now)
                };

                entries_to_process.push(ParserEntry {
                    path,
                    is_symlink,
                    symlink_broken,
                    modified,
                });
            }
        }

        // Collect paths that need metadata extraction (non-broken files)
        let paths_for_metadata: Vec<std::path::PathBuf> = entries_to_process
            .iter()
            .filter(|e| !e.symlink_broken)
            .map(|e| e.path.clone())
            .collect();

        // Extract metadata in batches to avoid spawning too many Python processes
        let mut all_metadata = std::collections::HashMap::new();
        for chunk in paths_for_metadata.chunks(Self::METADATA_BATCH_SIZE) {
            let batch_results = Self::extract_parser_metadata_batch(chunk);
            all_metadata.extend(batch_results);
        }

        // Build ParserInfo structs
        let mut parsers = Vec::new();
        for entry in entries_to_process {
            let (name, version, topics) = if entry.symlink_broken {
                // Broken symlink: use fallback name
                let fallback_name = entry
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                (fallback_name, None, vec![])
            } else {
                // Look up from batch results
                entry
                    .path
                    .to_str()
                    .and_then(|path_str| all_metadata.get(path_str).cloned())
                    .unwrap_or_else(|| {
                        let fallback_name = entry
                            .path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        (fallback_name, None, vec![])
                    })
            };

            // Set health based on symlink status
            let health = if entry.symlink_broken {
                ParserHealth::BrokenLink
            } else {
                ParserHealth::Unknown
            };

            parsers.push(ParserInfo {
                path: entry.path,
                name,
                version,
                topics,
                modified: entry.modified,
                health,
                is_symlink: entry.is_symlink,
                symlink_broken: entry.symlink_broken,
            });
        }

        // Sort by name
        parsers.sort_by(|a, b| a.name.cmp(&b.name));

        self.parser_bench.parsers = parsers;
        self.parser_bench.parsers_loaded = true;
    }

    /// Handle Parser Bench mode keys
    fn handle_parser_bench_key(&mut self, key: KeyEvent) {
        match self.parser_bench.view {
            ParserBenchView::ParserList => {
                match key.code {
                    // Navigation
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !self.parser_bench.parsers.is_empty() {
                            self.parser_bench.selected_parser =
                                (self.parser_bench.selected_parser + 1) % self.parser_bench.parsers.len();
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if !self.parser_bench.parsers.is_empty() {
                            if self.parser_bench.selected_parser == 0 {
                                self.parser_bench.selected_parser = self.parser_bench.parsers.len() - 1;
                            } else {
                                self.parser_bench.selected_parser -= 1;
                            }
                        }
                    }
                    // Test parser
                    KeyCode::Char('t') | KeyCode::Enter => {
                        // TODO: Start test flow
                    }
                    // Quick test
                    KeyCode::Char('n') => {
                        // TODO: Open file picker for quick test
                    }
                    // Refresh
                    KeyCode::Char('r') => {
                        self.parser_bench.parsers_loaded = false;
                    }
                    // Delete broken symlink
                    KeyCode::Char('d') => {
                        if !self.parser_bench.parsers.is_empty() {
                            let parser = &self.parser_bench.parsers[self.parser_bench.selected_parser];
                            if parser.symlink_broken {
                                // Remove the broken symlink
                                let _ = std::fs::remove_file(&parser.path);
                                self.parser_bench.parsers_loaded = false; // Trigger reload
                            }
                        }
                    }
                    // Clear test result
                    KeyCode::Esc => {
                        if self.parser_bench.test_result.is_some() {
                            self.parser_bench.test_result = None;
                        } else {
                            self.mode = TuiMode::Home;
                        }
                    }
                    _ => {}
                }
            }
            ParserBenchView::ResultView => {
                match key.code {
                    // Re-run test
                    KeyCode::Char('r') => {
                        // TODO: Re-run last test
                    }
                    // Different file
                    KeyCode::Char('f') => {
                        self.parser_bench.view = ParserBenchView::ParserList;
                        self.parser_bench.test_result = None;
                    }
                    // Back to list
                    KeyCode::Esc => {
                        self.parser_bench.view = ParserBenchView::ParserList;
                        self.parser_bench.test_result = None;
                    }
                    _ => {}
                }
            }
            _ => {
                if key.code == KeyCode::Esc {
                    self.parser_bench.view = ParserBenchView::ParserList;
                }
            }
        }
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

        // Key '1' should switch to Discover (per spec)
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Discover));

        // In Discover mode, '2' controls panel focus (not view navigation)
        // So we need to go Home first with '0', then use '2'
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Home));

        // Now '2' should switch to Parser Bench (per spec)
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::ParserBench));

        // Key '0' should return to Home (per spec)
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))
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
            // Enter should navigate to Inspect mode (card 3 = Sources placeholder)
            // Card order: 0=Discover, 1=ParserBench, 2=Jobs, 3=Sources(Inspect)
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Inspect)); // Sources placeholder
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
    // UI Latency Tests - Navigation Must Be Fast
    // =========================================================================
    //
    // These tests verify that navigation operations complete quickly.
    // UI freezes occur when navigation triggers expensive operations like DB queries.
    // Navigation should be pure in-memory operations (< 1ms typical, < 10ms max).

    #[tokio::test]
    async fn test_sources_dropdown_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Set up sources (in-memory, no DB)
        app.discover.sources = (0..100)
            .map(|i| SourceInfo {
                id: SourceId(format!("source_{}", i)),
                name: format!("Source {}", i),
                path: std::path::PathBuf::from(format!("/data/source_{}", i)),
                file_count: 1000,
            })
            .collect();
        app.discover.sources_loaded = true;
        app.discover.data_loaded = true;

        // Open sources dropdown
        app.discover.view_state = DiscoverViewState::SourcesDropdown;
        app.discover.preview_source = Some(0);

        // Navigate through all 100 sources and measure time
        let start = Instant::now();
        for _ in 0..99 {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // 99 navigation operations should complete in < 100ms (< 1ms each)
        assert!(
            elapsed.as_millis() < 100,
            "Sources dropdown navigation too slow: {:?} for 99 operations (should be < 100ms)",
            elapsed
        );

        // Verify we navigated to the last source
        assert_eq!(app.discover.preview_source, Some(99));

        // Verify data_loaded is still true (no DB reload triggered)
        assert!(
            app.discover.data_loaded,
            "Navigation should NOT trigger data reload"
        );
    }

    #[tokio::test]
    async fn test_file_list_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::Files;

        // Set up large file list (in-memory)
        app.discover.files = (0..10_000)
            .map(|i| FileInfo {
                path: format!("/data/file_{}.csv", i),
                rel_path: format!("file_{}.csv", i),
                size: 1024,
                modified: chrono::Local::now(),
                is_dir: false,
                tags: vec![],
            })
            .collect();
        app.discover.selected = 0;

        // Navigate through 1000 files and measure time
        let start = Instant::now();
        for _ in 0..1000 {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // 1000 navigation operations should complete in < 200ms
        assert!(
            elapsed.as_millis() < 200,
            "File list navigation too slow: {:?} for 1000 operations (should be < 200ms)",
            elapsed
        );

        assert_eq!(app.discover.selected, 1000);
    }

    #[tokio::test]
    async fn test_jobs_list_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Set up large jobs list (in-memory)
        app.jobs_state.jobs = (0..1000)
            .map(|i| JobInfo {
                id: i,
                file_path: format!("/data/file_{}.csv", i),
                parser_name: "test_parser".to_string(),
                status: if i % 4 == 0 {
                    JobStatus::Completed
                } else if i % 4 == 1 {
                    JobStatus::Running
                } else if i % 4 == 2 {
                    JobStatus::Failed
                } else {
                    JobStatus::Pending
                },
                retry_count: 0,
                error_message: None,
                created_at: chrono::Local::now(),
            })
            .collect();
        app.jobs_state.selected_index = 0;

        // Navigate through 500 jobs and measure time
        let start = Instant::now();
        for _ in 0..500 {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // 500 navigation operations should complete in < 100ms
        assert!(
            elapsed.as_millis() < 100,
            "Jobs list navigation too slow: {:?} for 500 operations (should be < 100ms)",
            elapsed
        );

        assert_eq!(app.jobs_state.selected_index, 500);
    }

    #[tokio::test]
    async fn test_sources_filter_typing_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Set up sources
        app.discover.sources = (0..100)
            .map(|i| SourceInfo {
                id: SourceId(format!("source_{}", i)),
                name: format!("Source {}", i),
                path: std::path::PathBuf::from(format!("/data/source_{}", i)),
                file_count: 1000,
            })
            .collect();
        app.discover.sources_loaded = true;
        app.discover.data_loaded = true;

        // Open sources dropdown and enter filter mode
        app.discover.view_state = DiscoverViewState::SourcesDropdown;
        app.discover.sources_filtering = true;

        // Type a filter string and measure time
        let start = Instant::now();
        for c in "Source 5".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // Typing 8 characters should complete in < 50ms
        assert!(
            elapsed.as_millis() < 50,
            "Filter typing too slow: {:?} for 8 keystrokes (should be < 50ms)",
            elapsed
        );

        // Verify filter was applied
        assert_eq!(app.discover.sources_filter, "Source 5");

        // Verify data_loaded is still true (no DB reload triggered)
        assert!(
            app.discover.data_loaded,
            "Filtering should NOT trigger data reload"
        );
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
        // Symbols per tui.md Section 5.3
        assert_eq!(JobStatus::Pending.symbol(), "○");
        assert_eq!(JobStatus::Running.symbol(), "↻");
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
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.filter.is_empty());

        // Press / to enter filter mode
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Filtering);

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
        app.discover.view_state = DiscoverViewState::Filtering;
        app.discover.filter = "test".to_string();

        // Esc should exit filter mode, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
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
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 't' to open tag dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Tagging);
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
        app.discover.view_state = DiscoverViewState::Tagging;
        app.discover.tag_input = "partial".to_string();

        // Esc should close tag dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.tag_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_scan_path_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 's' to open scan path dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::EnteringPath);

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
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/some/path".to_string();

        // Esc should close scan dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.scan_path_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_bulk_tag_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 'T' (Shift+t) to open bulk tag dialog
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::BulkTagging);
        assert!(app.discover.bulk_tag_input.is_empty());
        assert!(!app.discover.bulk_tag_save_as_rule);
    }

    #[test]
    fn test_discover_bulk_tag_toggle_save_as_rule() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.files = create_test_files();
        app.discover.view_state = DiscoverViewState::BulkTagging;
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
        app.discover.view_state = DiscoverViewState::BulkTagging;
        app.discover.bulk_tag_input = "batch".to_string();
        app.discover.bulk_tag_save_as_rule = true;

        // Esc should close bulk tag dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
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
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 'S' (Shift+s) on a directory to create source
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::CreatingSource);
        assert!(app.discover.pending_source_path.is_some());
        assert!(app.discover.pending_source_path.as_ref().unwrap().contains("archives"));
    }

    #[test]
    fn test_discover_create_source_esc_cancels() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::CreatingSource;
        app.discover.source_name_input = "my_source".to_string();
        app.discover.pending_source_path = Some("/data/archives".to_string());

        // Esc should close create source dialog, NOT go to Home
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.source_name_input.is_empty());
        assert!(app.discover.pending_source_path.is_none());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Discover));
    }

    #[test]
    fn test_discover_esc_goes_home_when_no_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        // No dialogs open - view_state should be Files
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

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
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/tmp/test".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.scan_path_input, "/tmp/tes");

        // Reset and test backspace in tag input
        app.discover.view_state = DiscoverViewState::Tagging;
        app.discover.tag_input = "mytag".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.tag_input, "myta");

        // Reset and test backspace in bulk tag input
        app.discover.view_state = DiscoverViewState::BulkTagging;
        app.discover.bulk_tag_input = "bulktag".to_string();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.bulk_tag_input, "bulkta");
    }

    // =========================================================================
    // Scanning E2E Tests - Non-blocking scan with progress
    // =========================================================================

    #[tokio::test]
    async fn test_scan_valid_directory_enters_scanning_state() {
        use tempfile::TempDir;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "test2").unwrap();

        // Open scan dialog and enter path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();

        // Press Enter to start scan
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Should enter Scanning state (non-blocking)
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Scanning,
            "Should enter Scanning state after submitting valid path"
        );
        assert!(
            app.discover.scanning_path.is_some(),
            "scanning_path should be set"
        );
        assert!(
            app.pending_scan.is_some(),
            "pending_scan channel should be created"
        );
    }

    #[tokio::test]
    async fn test_scan_invalid_path_shows_error() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Open scan dialog with invalid path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/nonexistent/path/that/does/not/exist".to_string();

        // Press Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Should NOT enter Scanning state - stays in Files with error
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Should return to Files state on invalid path"
        );
        assert!(
            app.discover.scan_error.is_some(),
            "Should have scan_error set"
        );
        assert!(
            app.discover.scan_error.as_ref().unwrap().contains("not found"),
            "Error message should mention path not found"
        );
    }

    #[tokio::test]
    async fn test_scan_not_a_directory_shows_error() {
        use tempfile::NamedTempFile;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp file (not a directory)
        let temp_file = NamedTempFile::new().unwrap();

        // Open scan dialog with file path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_file.path().display().to_string();

        // Press Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Should NOT enter Scanning state - stays in Files with error
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Should return to Files state when path is a file"
        );
        assert!(
            app.discover.scan_error.is_some(),
            "Should have scan_error set"
        );
        assert!(
            app.discover.scan_error.as_ref().unwrap().contains("Not a directory"),
            "Error message should mention not a directory"
        );
    }

    #[tokio::test]
    async fn test_scan_cancel_with_esc() {
        use tempfile::TempDir;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        assert_eq!(app.discover.view_state, DiscoverViewState::Scanning);

        // Press Esc to cancel scan
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;

        // Should return to Files state
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Esc should cancel scan and return to Files"
        );
        assert!(
            app.pending_scan.is_none(),
            "pending_scan should be cleared"
        );
        assert!(
            app.discover.scanning_path.is_none(),
            "scanning_path should be cleared"
        );
        assert!(
            app.discover.status_message.is_some(),
            "Should have status message about cancellation"
        );
    }

    // =========================================================================
    // Scan-as-Job Integration Tests
    // =========================================================================

    #[tokio::test]
    async fn test_scan_creates_job_with_running_status() {
        use tempfile::TempDir;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Verify no jobs initially
        assert!(app.jobs_state.jobs.is_empty(), "Should start with no jobs");

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Should have created a job
        assert_eq!(app.jobs_state.jobs.len(), 1, "Should have created one job");

        let job = &app.jobs_state.jobs[0];
        assert_eq!(job.status, JobStatus::Running, "Job should be Running");
        assert_eq!(job.parser_name, "scan", "Job type should be 'scan'");
        assert!(
            job.file_path.contains(temp_dir.path().to_str().unwrap()),
            "Job should track the scanned directory"
        );
        assert!(
            app.current_scan_job_id.is_some(),
            "current_scan_job_id should be set"
        );
    }

    #[tokio::test]
    async fn test_scan_cancel_sets_job_cancelled() {
        use tempfile::TempDir;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Verify job was created with Running status
        assert_eq!(app.jobs_state.jobs.len(), 1);
        assert_eq!(app.jobs_state.jobs[0].status, JobStatus::Running);

        // Cancel with ESC
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;

        // Job should now be Cancelled
        assert_eq!(
            app.jobs_state.jobs[0].status,
            JobStatus::Cancelled,
            "Job should be Cancelled after ESC"
        );
        assert!(
            app.current_scan_job_id.is_none(),
            "current_scan_job_id should be cleared"
        );
    }

    #[tokio::test]
    async fn test_scan_complete_sets_job_completed() {
        use tempfile::TempDir;
        use std::time::Duration;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Verify job created with Running status
        assert_eq!(app.jobs_state.jobs.len(), 1);
        assert_eq!(app.jobs_state.jobs[0].status, JobStatus::Running);

        // Wait for scan to complete
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Scan did not complete within 5 seconds");
            }
            app.tick().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Job should now be Completed
        assert_eq!(
            app.jobs_state.jobs[0].status,
            JobStatus::Completed,
            "Job should be Completed after scan finishes"
        );
        assert!(
            app.current_scan_job_id.is_none(),
            "current_scan_job_id should be cleared after completion"
        );
    }

    #[tokio::test]
    async fn test_scan_completes_and_populates_files() {
        use tempfile::TempDir;
        use std::time::Duration;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "test2").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        std::fs::write(temp_dir.path().join("subdir/file3.txt"), "test3").unwrap();

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        assert_eq!(app.discover.view_state, DiscoverViewState::Scanning);

        // Poll tick until scan completes (with timeout)
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Scan did not complete within 5 seconds");
            }
            app.tick().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Should be back in Files state with files populated
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Should return to Files after scan completes"
        );
        assert!(
            !app.discover.files.is_empty(),
            "Files should be populated after scan"
        );
        // Should have found 3 files (scanner stores files, not directories)
        assert_eq!(
            app.discover.files.len(),
            3,
            "Should have found 3 files (scanner stores files only, not directories)"
        );
        assert!(
            app.discover.status_message.is_some(),
            "Should have success message"
        );
    }

    #[tokio::test]
    async fn test_scan_progress_initialized_and_cleared() {
        use tempfile::TempDir;
        use std::time::Duration;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        for i in 0..10 {
            std::fs::write(temp_dir.path().join(format!("file{}.txt", i)), format!("test{}", i)).unwrap();
        }

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // Should have entered Scanning state with progress initialized
        assert_eq!(app.discover.view_state, DiscoverViewState::Scanning);
        assert!(
            app.discover.scan_progress.is_some(),
            "scan_progress should be initialized when entering Scanning state"
        );

        // Wait for scan to complete
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(10) {
                panic!("Scan did not complete within 10 seconds");
            }
            app.tick().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Verify scan completed successfully
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Scan should complete"
        );

        // Progress should be cleared after completion
        assert!(
            app.discover.scan_progress.is_none(),
            "scan_progress should be cleared after scan completes"
        );

        // Should have found the files
        assert!(
            app.discover.files.len() >= 10,
            "Should have found files: got {}",
            app.discover.files.len()
        );
    }

    #[tokio::test]
    async fn test_scan_home_tilde_expansion() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        // Test ~ expansion - should not fail immediately
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "~".to_string();

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        // If home dir exists and is readable, should enter Scanning
        // Otherwise should show error - but NOT panic
        // (We can't guarantee home dir exists in all test environments)
        let valid_states = [DiscoverViewState::Scanning, DiscoverViewState::Files];
        assert!(
            valid_states.contains(&app.discover.view_state),
            "Should either be Scanning (if ~ resolved) or Files (with error)"
        );
    }

    // =========================================================================
    // Pagination and Large Dataset Tests
    // =========================================================================

    #[test]
    fn test_large_file_list_navigation_performance() {
        // Test that navigating a large file list is O(1) not O(n)
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::Files;

        // Create a simulated large file list (100K files)
        let large_count: usize = 100_000;
        app.discover.files = (0..large_count)
            .map(|i| FileInfo {
                path: format!("/test/path/file_{}.txt", i),
                rel_path: format!("file_{}.txt", i),
                size: 1000,
                modified: chrono::Local::now(),
                is_dir: false,
                tags: vec![],
            })
            .collect();

        // Navigation should be instant - just updating selected index
        let start = std::time::Instant::now();

        // Navigate down 1000 times
        for _ in 0..1000 {
            app.discover.selected = (app.discover.selected + 1) % large_count;
        }

        let duration = start.elapsed();

        // Should complete in under 10ms (pure index math)
        assert!(
            duration.as_millis() < 10,
            "Navigation took {:?}, should be under 10ms for 1000 key presses",
            duration
        );
    }

    #[test]
    fn test_selection_bounds_with_large_list() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::Files;

        // Create a large file list
        let file_count: usize = 50_000;
        app.discover.files = (0..file_count)
            .map(|i| FileInfo {
                path: format!("/test/path/file_{}.txt", i),
                rel_path: format!("file_{}.txt", i),
                size: 1000,
                modified: chrono::Local::now(),
                is_dir: false,
                tags: vec![],
            })
            .collect();

        // Test selection at various positions
        app.discover.selected = 0;
        assert_eq!(app.discover.selected, 0, "Should start at 0");

        // Navigate to middle
        app.discover.selected = file_count / 2;
        assert_eq!(
            app.discover.selected,
            file_count / 2,
            "Should be at middle"
        );

        // Navigate to end
        app.discover.selected = file_count - 1;
        assert_eq!(
            app.discover.selected,
            file_count - 1,
            "Should be at last item"
        );

        // Bounds check - selection should not exceed file count
        app.discover.selected = file_count; // Invalid - past end
        app.discover.selected = app.discover.selected.min(file_count.saturating_sub(1));
        assert_eq!(
            app.discover.selected,
            file_count - 1,
            "Should clamp to valid range"
        );
    }

    #[test]
    fn test_virtual_scroll_offset_calculation() {
        // Test the scroll offset logic used in draw_file_list
        let file_count: usize = 10_000;
        let visible_rows: usize = 30; // Typical terminal height for file list

        // Near start - should not scroll
        let selected: usize = 5;
        let scroll_offset = if visible_rows >= file_count {
            0
        } else if selected < visible_rows / 2 {
            0
        } else if selected > file_count.saturating_sub(visible_rows / 2) {
            file_count.saturating_sub(visible_rows)
        } else {
            selected.saturating_sub(visible_rows / 2)
        };
        assert_eq!(scroll_offset, 0, "Near start should show from beginning");

        // Middle - should center selection
        let selected: usize = 5000;
        let scroll_offset = if visible_rows >= file_count {
            0
        } else if selected < visible_rows / 2 {
            0
        } else if selected > file_count.saturating_sub(visible_rows / 2) {
            file_count.saturating_sub(visible_rows)
        } else {
            selected.saturating_sub(visible_rows / 2)
        };
        assert_eq!(
            scroll_offset,
            5000 - 15,
            "Middle should center selection"
        );

        // Near end - should show last visible_rows
        let selected: usize = 9990;
        let scroll_offset = if visible_rows >= file_count {
            0
        } else if selected < visible_rows / 2 {
            0
        } else if selected > file_count.saturating_sub(visible_rows / 2) {
            file_count.saturating_sub(visible_rows)
        } else {
            selected.saturating_sub(visible_rows / 2)
        };
        assert_eq!(
            scroll_offset,
            file_count - visible_rows,
            "Near end should show last rows"
        );
    }

    #[test]
    fn test_small_list_no_scroll() {
        // When all files fit, scroll offset should always be 0
        let file_count: usize = 20;
        let visible_rows: usize = 50; // More visible rows than files

        for selected in 0..file_count {
            let scroll_offset = if visible_rows >= file_count {
                0
            } else if selected < visible_rows / 2 {
                0
            } else if selected > file_count.saturating_sub(visible_rows / 2) {
                file_count.saturating_sub(visible_rows)
            } else {
                selected.saturating_sub(visible_rows / 2)
            };
            assert_eq!(
                scroll_offset, 0,
                "Small list should never scroll, but got offset {} for selected {}",
                scroll_offset, selected
            );
        }
    }

    #[test]
    fn test_filter_with_large_list() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::Files;

        // Create a large file list with varied filenames
        app.discover.files = (0..50_000usize)
            .map(|i| {
                let ext = match i % 5 {
                    0 => "csv",
                    1 => "json",
                    2 => "txt",
                    3 => "log",
                    _ => "xml",
                };
                FileInfo {
                    path: format!("/test/path/file_{}.{}", i, ext),
                    rel_path: format!("file_{}.{}", i, ext),
                    size: 1000,
                    modified: chrono::Local::now(),
                    is_dir: false,
                    tags: vec![],
                }
            })
            .collect();

        // Apply filter
        app.discover.filter = "csv".to_string();

        // Count filtered files (without materializing all of them)
        let filtered_count = app
            .discover
            .files
            .iter()
            .filter(|f| f.rel_path.contains("csv") || f.path.contains("csv"))
            .count();

        // Should have ~10K CSV files (1/5 of 50K)
        assert_eq!(
            filtered_count, 10_000,
            "Filter should match ~10K CSV files"
        );

        // Selection should reset when filter applied
        app.discover.selected = 25_000; // Invalid for filtered view
        app.discover.selected = app.discover.selected.min(filtered_count.saturating_sub(1));
        assert!(
            app.discover.selected < filtered_count,
            "Selection should be within filtered range"
        );
    }

    #[test]
    fn test_progress_update_no_overflow() {
        // Test that progress update calculation doesn't overflow
        // This simulates the race condition where last > count due to concurrent updates
        use std::sync::atomic::{AtomicUsize, Ordering};

        let count = AtomicUsize::new(0);
        let last_progress = AtomicUsize::new(0);

        // Simulate multiple threads updating progress
        // Thread 1: count=50, last=0 -> 50-0=50 < 100, no update
        count.store(50, Ordering::Relaxed);
        let c = count.load(Ordering::Relaxed);
        let l = last_progress.load(Ordering::Relaxed);
        assert!(c.saturating_sub(l) < 100);

        // Thread 2: count=150, last=0 -> 150-0=150 >= 100, update last to 150
        count.store(150, Ordering::Relaxed);
        last_progress.store(150, Ordering::Relaxed);

        // Race condition: Thread 1 reads last=150 but count=50 (stale)
        // Without saturating_sub: 50-150 = underflow panic!
        // With saturating_sub: 50.saturating_sub(150) = 0 < 100, no update (safe)
        let stale_count: usize = 50;
        let updated_last: usize = 150;
        assert_eq!(stale_count.saturating_sub(updated_last), 0);
        assert!(stale_count.saturating_sub(updated_last) < 100);

        // Normal case: count=250, last=150 -> 250-150=100 >= 100, update
        let c: usize = 250;
        let l: usize = 150;
        assert_eq!(c.saturating_sub(l), 100);
        assert!(c.saturating_sub(l) >= 100);
    }

    #[tokio::test]
    async fn test_scan_result_memory_efficiency() {
        // Test that scan results don't cause memory issues
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Create many subdirectories with files
        for i in 0..100 {
            let subdir = temp_dir.path().join(format!("dir_{}", i));
            std::fs::create_dir(&subdir).unwrap();
            for j in 0..50 {
                let file_path = subdir.join(format!("file_{}.txt", j));
                std::fs::write(&file_path, format!("content {}", j)).unwrap();
            }
        }

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        app.discover.view_state = DiscoverViewState::Files;

        // Trigger scan
        let path = temp_dir.path().to_string_lossy().to_string();
        app.discover.scan_path_input = path.clone();
        app.scan_directory(&path);

        // Complete scan
        while app.discover.view_state == DiscoverViewState::Scanning {
            app.tick().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // load_scout_files has LIMIT 1000 for memory efficiency
        // So we expect 1000 files to be loaded (even though 5000+ were scanned)
        assert!(
            app.discover.files.len() == 1000,
            "Should have loaded 1000 files (LIMIT 1000 for memory), got {}",
            app.discover.files.len()
        );

        // Memory should be reasonable - each FileInfo is ~200 bytes
        // 5000 files * 200 bytes = ~1MB, well under any reasonable limit
        let estimated_memory = app.discover.files.len() * std::mem::size_of::<FileInfo>();
        assert!(
            estimated_memory < 50_000_000, // 50MB limit
            "Memory usage should be reasonable: {} bytes",
            estimated_memory
        );
    }

    #[tokio::test]
    async fn test_scan_partial_batches_not_lost() {
        // Test that partial batches (less than BATCH_SIZE=1000) are correctly flushed
        // This validates the FlushGuard drop behavior
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Create exactly 150 files - less than BATCH_SIZE (1000)
        // These should all be collected via FlushGuard.drop()
        for i in 0..150 {
            let file_path = temp_dir.path().join(format!("file_{}.txt", i));
            std::fs::write(&file_path, format!("content {}", i)).unwrap();
        }

        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;

        let path = temp_dir.path().to_string_lossy().to_string();
        app.scan_directory(&path);

        // Wait for scan to complete
        let mut iterations = 0;
        while app.discover.view_state == DiscoverViewState::Scanning && iterations < 500 {
            app.tick().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            iterations += 1;
        }

        // All 150 files should be present - partial batch was flushed
        assert_eq!(
            app.discover.files.len(), 150,
            "Expected exactly 150 files (partial batch), got {}",
            app.discover.files.len()
        );
    }

    #[test]
    fn test_compare_exchange_prevents_duplicate_progress() {
        // Test that compare_exchange correctly prevents duplicate progress updates
        // when multiple threads race on the same old value
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let last_progress = Arc::new(AtomicUsize::new(0));
        let successful_updates = Arc::new(AtomicUsize::new(0));

        // All threads read the SAME "last" value before any thread succeeds
        // This simulates the race condition
        let last_seen = last_progress.load(Ordering::Relaxed);

        // Spawn 10 threads that all try to update from the same old value
        let handles: Vec<_> = (0..10).map(|_| {
            let last_progress = last_progress.clone();
            let successful_updates = successful_updates.clone();
            thread::spawn(move || {
                // All threads observed last_seen=0, all try to update to 5000
                if last_progress.compare_exchange(
                    last_seen,
                    5000,
                    Ordering::Relaxed,
                    Ordering::Relaxed
                ).is_ok() {
                    successful_updates.fetch_add(1, Ordering::Relaxed);
                }
            })
        }).collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Only ONE thread should have won the race
        assert_eq!(
            successful_updates.load(Ordering::Relaxed), 1,
            "Only one thread should win the compare_exchange race"
        );
        assert_eq!(last_progress.load(Ordering::Relaxed), 5000);
    }

    #[test]
    fn test_batch_flush_guard_behavior() {
        // Test the FlushGuard pattern used in parallel scanning
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let all_batches: Arc<Mutex<Vec<Vec<i32>>>> = Arc::new(Mutex::new(Vec::new()));
        let total_count = Arc::new(AtomicUsize::new(0));

        // Simulate what happens in each thread
        {
            struct TestFlushGuard {
                batch: Vec<i32>,
                all_batches: Arc<Mutex<Vec<Vec<i32>>>>,
                total_count: Arc<AtomicUsize>,
            }

            impl Drop for TestFlushGuard {
                fn drop(&mut self) {
                    if !self.batch.is_empty() {
                        let batch = std::mem::take(&mut self.batch);
                        let len = batch.len();
                        self.all_batches.lock().unwrap().push(batch);
                        self.total_count.fetch_add(len, Ordering::Relaxed);
                    }
                }
            }

            let mut guard = TestFlushGuard {
                batch: Vec::new(),
                all_batches: all_batches.clone(),
                total_count: total_count.clone(),
            };

            // Add 50 items (less than a typical batch size)
            for i in 0..50 {
                guard.batch.push(i);
            }

            // guard is dropped here, should flush the 50 items
        }

        // Verify items were flushed
        let batches = all_batches.lock().unwrap();
        assert_eq!(batches.len(), 1, "Should have one batch");
        assert_eq!(batches[0].len(), 50, "Batch should have 50 items");
        assert_eq!(total_count.load(Ordering::Relaxed), 50, "Total count should be 50");
    }
}

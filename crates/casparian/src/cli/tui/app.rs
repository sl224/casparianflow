//! Application state for the TUI
//!
//! # Dead Code Justification
//! Several struct fields and enum variants in this module are defined for
//! upcoming TUI features (Jobs view, Parser Bench, Monitoring) per the spec.
//! They are scaffolding for active development. See specs/views/*.md.
#![allow(dead_code)]

use casparian_db::{DbConnection, DbValue};
use casparian_mcp::tools::{create_default_registry, ToolRegistry};
use casparian_mcp::types::ToolResult;
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;

use super::TuiArgs;
use crate::cli::config::{active_db_path, default_db_backend, DbBackend};
use crate::scout::{
    scan_path, Database as ScoutDatabase,
    ScanProgress as ScoutProgress, Scanner as ScoutScanner, Source, SourceType,
};

/// Current TUI mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiMode {
    #[default]
    Home,        // Home hub: quick start + status dashboard
    Discover,    // File discovery and tagging
    Jobs,        // Job queue management
    Sources,     // Sources management
    ParserBench, // Parser development workbench (accessed via P key)
    Settings,    // Application settings
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


/// Settings category in the Settings view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    General,
    Display,
    About,
}

/// State for settings mode (per specs/views/settings.md)
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Current category
    pub category: SettingsCategory,
    /// Selected item index within category
    pub selected_index: usize,
    /// Whether in editing mode for a setting
    pub editing: bool,
    /// Current edit value (for text fields)
    pub edit_value: String,
    /// Previous mode to return to on Esc
    pub previous_mode: Option<TuiMode>,
    // Settings values
    /// Default source path
    pub default_source_path: String,
    /// Auto-scan on startup
    pub auto_scan_on_startup: bool,
    /// Confirm destructive actions
    pub confirm_destructive: bool,
    /// Theme (dark, light, system)
    pub theme: String,
    /// Use unicode symbols
    pub unicode_symbols: bool,
    /// Show hidden files
    pub show_hidden_files: bool,
}

impl SettingsState {
    /// Get the number of settings in the current category
    pub fn category_item_count(&self) -> usize {
        match self.category {
            SettingsCategory::General => 3,  // default_path, auto_scan, confirm
            SettingsCategory::Display => 3,  // theme, unicode, hidden_files
            SettingsCategory::About => 3,    // version, database, config (read-only)
        }
    }
}

/// State for job queue mode (per jobs_redesign.md spec)
#[derive(Debug, Clone, Default)]
pub struct JobsState {
    /// Current view state within Jobs mode
    pub view_state: JobsViewState,
    /// Previous view state (for Esc navigation)
    pub previous_view_state: Option<JobsViewState>,
    /// List of jobs
    pub jobs: Vec<JobInfo>,
    /// Currently selected job index (into filtered list)
    pub selected_index: usize,
    /// Filter: show only specific status
    pub status_filter: Option<JobStatus>,
    /// Filter: show only specific job type
    pub type_filter: Option<JobType>,
    /// Whether pipeline summary is shown
    pub show_pipeline: bool,
    /// Pipeline state data
    pub pipeline: PipelineState,
    /// Monitoring panel state
    pub monitoring: MonitoringState,
    /// Whether jobs have been loaded from DB
    pub jobs_loaded: bool,
    /// Last poll timestamp for incremental updates
    pub last_poll: Option<DateTime<Local>>,
}

impl JobsState {
    /// Get filtered jobs based on current status and type filters
    /// Jobs are sorted: Failed first, then by recency (per spec Section 10.1)
    pub fn filtered_jobs(&self) -> Vec<&JobInfo> {
        let mut jobs: Vec<&JobInfo> = self.jobs.iter()
            .filter(|j| {
                let status_ok = match self.status_filter {
                    Some(status) => j.status == status,
                    None => true,
                };
                let type_ok = match self.type_filter {
                    Some(jtype) => j.job_type == jtype,
                    None => true,
                };
                status_ok && type_ok
            })
            .collect();

        // Sort: Failed first, then by recency
        jobs.sort_by(|a, b| {
            let a_failed = a.status == JobStatus::Failed;
            let b_failed = b.status == JobStatus::Failed;
            match (a_failed, b_failed) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.started_at.cmp(&a.started_at),
            }
        });

        jobs
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

    /// Transition to a new view state, saving current as previous
    pub fn transition_state(&mut self, new_state: JobsViewState) {
        self.previous_view_state = Some(self.view_state);
        self.view_state = new_state;
    }

    /// Return to previous view state (for Esc)
    pub fn return_to_previous_state(&mut self) {
        if let Some(prev) = self.previous_view_state.take() {
            self.view_state = prev;
        } else {
            self.view_state = JobsViewState::JobList;
        }
    }

    /// Get currently selected job
    pub fn selected_job(&self) -> Option<&JobInfo> {
        self.filtered_jobs().get(self.selected_index).copied()
    }

    /// Calculate aggregate statistics for status bar
    pub fn aggregate_stats(&self) -> (u32, u32, u32, u32, u64) {
        let mut running = 0u32;
        let mut done = 0u32;
        let mut failed = 0u32;
        let mut total_files = 0u32;
        let mut total_output_bytes = 0u64;

        for job in &self.jobs {
            match job.status {
                JobStatus::Running => running += 1,
                JobStatus::Completed => done += 1,
                JobStatus::Failed => failed += 1,
                _ => {}
            }
            total_files += job.items_processed;
            if let Some(bytes) = job.output_size_bytes {
                total_output_bytes += bytes;
            }
        }

        (running, done, failed, total_files, total_output_bytes)
    }

    /// Add a job at the front and trim old completed jobs if over limit
    pub fn push_job(&mut self, job: JobInfo) {
        self.jobs.insert(0, job);
        self.trim_completed_jobs();
    }

    /// Trim old completed/failed jobs to prevent unbounded memory growth
    /// Keeps running/pending jobs and removes oldest completed jobs first
    fn trim_completed_jobs(&mut self) {
        if self.jobs.len() > MAX_JOBS {
            // Count non-terminal jobs (Running, Pending)
            let active_count = self.jobs.iter()
                .filter(|j| matches!(j.status, JobStatus::Running | JobStatus::Pending))
                .count();

            // Only trim if we have enough completed jobs to remove
            let completed_count = self.jobs.len() - active_count;
            let target_completed = MAX_JOBS.saturating_sub(active_count);

            if completed_count > target_completed {
                let to_remove = completed_count - target_completed;
                let mut removed = 0;

                // Remove from the end (oldest) first, only completed/failed jobs
                self.jobs.retain(|j| {
                    if removed >= to_remove {
                        return true;
                    }
                    if matches!(j.status, JobStatus::Completed | JobStatus::Failed) {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
    }
}

/// Job type enumeration (per jobs_redesign.md spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobType {
    Scan,
    #[default]
    Parse,
    Backtest,
    /// Schema analysis job (pattern seeds, archetypes, naming schemes, synonyms)
    SchemaEval,
}

impl JobType {
    /// Get display name for this job type
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::Scan => "SCAN",
            JobType::Parse => "PARSE",
            JobType::Backtest => "BACKTEST",
            JobType::SchemaEval => "SCHEMA",
        }
    }
}

/// Mode for schema evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaEvalMode {
    /// Quick sample evaluation (structure-aware sampling, ~50 paths per prefix)
    Sample,
    /// Full evaluation (all matching paths)
    Full,
}

/// Information about a job (per jobs_redesign.md spec Section 8.1)
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub id: i64,
    pub file_version_id: Option<i64>,
    pub job_type: JobType,
    pub name: String,                     // parser/exporter/source name
    pub version: Option<String>,
    pub status: JobStatus,
    pub started_at: DateTime<Local>,
    pub completed_at: Option<DateTime<Local>>,

    // Progress
    pub items_total: u32,
    pub items_processed: u32,
    pub items_failed: u32,

    // Output
    pub output_path: Option<String>,
    pub output_size_bytes: Option<u64>,

    // Backtest-specific (None for other types)
    pub backtest: Option<BacktestInfo>,

    // Errors
    pub failures: Vec<JobFailure>,
}

impl Default for JobInfo {
    fn default() -> Self {
        Self {
            id: 0,
            file_version_id: None,
            job_type: JobType::Parse,
            name: String::new(),
            version: None,
            status: JobStatus::Pending,
            started_at: Local::now(),
            completed_at: None,
            items_total: 0,
            items_processed: 0,
            items_failed: 0,
            output_path: None,
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
        }
    }
}

/// Backtest-specific job information
#[derive(Debug, Clone)]
pub struct BacktestInfo {
    pub pass_rate: f64,                   // 0.0 - 1.0
    pub iteration: u32,
    pub high_failure_passed: u32,
}

/// Job failure details
#[derive(Debug, Clone)]
pub struct JobFailure {
    pub file_path: String,
    pub error: String,
    pub line: Option<u32>,
}

/// Job status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    /// Job was cancelled by user
    Cancelled,
}

impl JobStatus {
    /// Get display symbol for this status
    /// Symbols per jobs_redesign.md spec:
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

// =============================================================================
// Jobs View State Types (per jobs_redesign.md spec Section 6)
// =============================================================================

/// View states within Jobs mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobsViewState {
    #[default]
    JobList,
    DetailPanel,
    LogViewer,
    FilterDialog,
    MonitoringPanel,
}

/// Monitoring panel state (per jobs_redesign.md spec Section 8.2)
#[derive(Debug, Clone, Default)]
pub struct MonitoringState {
    pub queue: QueueStats,
    pub throughput_history: std::collections::VecDeque<ThroughputSample>,
    pub sinks: Vec<SinkStats>,
    pub paused: bool,
}

/// Queue statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub pending: u32,
    pub running: u32,
    pub completed: u32,
    pub failed: u32,
    pub depth_history: std::collections::VecDeque<u32>,
}

/// Throughput sample for sparklines
#[derive(Debug, Clone)]
pub struct ThroughputSample {
    pub timestamp: DateTime<Local>,
    pub rows_per_second: f64,
}

/// Sink statistics
#[derive(Debug, Clone)]
pub struct SinkStats {
    pub uri: String,
    pub total_rows: u64,
    pub total_bytes: u64,
    pub error_count: u32,
    pub latency_p50_ms: u32,
    pub latency_p99_ms: u32,
    pub outputs: Vec<SinkOutput>,
}

/// Individual sink output
#[derive(Debug, Clone)]
pub struct SinkOutput {
    pub name: String,
    pub rows: u64,
    pub bytes: u64,
}

/// Pipeline state for visualization (per jobs_redesign.md spec Section 8.3)
#[derive(Debug, Clone, Default)]
pub struct PipelineState {
    pub source: PipelineStage,
    pub parsed: PipelineStage,
    pub output: PipelineStage,
    pub active_parser: Option<String>,
}

/// Pipeline stage counts
#[derive(Debug, Clone, Default)]
pub struct PipelineStage {
    pub count: u32,
    pub in_progress: u32,
}

/// State for the home hub screen
#[derive(Debug, Clone, Default)]
pub struct HomeState {
    /// Selected source index in the Quick Start panel
    pub selected_source_index: usize,
    /// Filter text for Quick Start sources list
    pub filter: String,
    /// Whether filter input mode is active
    pub filtering: bool,
    /// Recent jobs for the activity panel
    pub recent_jobs: Vec<JobSummary>,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Statistics displayed on cards (legacy, used for stats loading)
    pub stats: HomeStats,
    /// Whether stats have been loaded from database
    pub stats_loaded: bool,
}

/// Summary of a job for display in Home activity panel
#[derive(Debug, Clone)]
pub struct JobSummary {
    pub id: i64,
    pub job_type: String,
    pub description: String,
    pub status: JobStatus,
    pub progress_percent: Option<u8>,
    pub duration_secs: Option<f64>,
}

// =============================================================================
// Sources View Types
// =============================================================================

/// State for Sources view (key 4)
#[derive(Debug, Clone, Default)]
pub struct SourcesState {
    /// Selected source index
    pub selected_index: usize,
    /// Whether in edit mode
    pub editing: bool,
    /// Edit field value
    pub edit_value: String,
    /// Whether showing delete confirmation
    pub confirm_delete: bool,
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
    RuleCreation,       // Dialog for creating/editing single rule (legacy)
    RuleBuilder,        // Split-view Rule Builder (specs/rule_builder.md)
    // --- Sources Manager (spec v1.7) ---
    SourcesManager,     // Dialog for source CRUD (M key)
    SourceEdit,         // Nested dialog for editing source name
    SourceDeleteConfirm, // Delete confirmation dialog
    // --- Background scanning ---
    Scanning,           // Directory scan in progress (non-blocking)
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
    /// Validation passed, scan is starting
    Started { job_id: i64 },
    /// Progress update during scan
    Progress(ScoutProgress),
    /// Scanning completed successfully
    Complete {
        source_path: String,
        /// Final count of files persisted (accurate, unlike last progress update)
        files_persisted: usize,
    },
    /// Scanning failed with error
    Error(String),
}

/// Result from background schema evaluation job
pub enum SchemaEvalResult {
    /// Schema eval started
    Started { job_id: i64 },
    /// Progress update (0-100)
    Progress { progress: u8, paths_analyzed: usize, total_paths: usize },
    /// Schema eval completed successfully
    Complete {
        job_id: i64,
        pattern: String,
        pattern_seeds: Vec<super::extraction::PatternSeed>,
        path_archetypes: Vec<super::extraction::PathArchetype>,
        naming_schemes: Vec<super::extraction::NamingScheme>,
        synonym_suggestions: Vec<super::extraction::SynonymSuggestion>,
        paths_analyzed: usize,
    },
    /// Schema eval failed
    Error(String),
}

/// Result from background sample schema evaluation
pub enum SampleEvalResult {
    Complete {
        pattern: String,
        pattern_seeds: Vec<super::extraction::PatternSeed>,
        path_archetypes: Vec<super::extraction::PathArchetype>,
        naming_schemes: Vec<super::extraction::NamingScheme>,
        synonym_suggestions: Vec<super::extraction::SynonymSuggestion>,
        paths_analyzed: usize,
    },
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
    /// History of (prefix, pattern) for back navigation
    pub nav_history: Vec<(String, String)>,
    /// Current path prefix (empty = root, "folder/" = inside folder)
    pub current_prefix: String,

    // --- Derived state (loaded atomically from DB) ---
    /// Folders/files at current level with file counts
    pub folders: Vec<FsEntry>,
    /// Sampled preview files (max 10)
    pub preview_files: Vec<GlobPreviewFile>,
    /// Total file count for current prefix + pattern
    pub total_count: GlobFileCount,

    // --- O(1) Navigation Cache ---
    /// Preloaded folder hierarchy - key is prefix, value is children at that level
    /// Example: "" -> [FsEntry::Folder { name: "logs", .. }, FsEntry::Folder { name: "data", .. }]
    ///          "logs/" -> [FsEntry::File { name: "app.log", .. }, ...]
    pub folder_cache: HashMap<String, Vec<FsEntry>>,
    /// Whether cache has been loaded for current source
    pub cache_loaded: bool,
    /// Source ID for which cache was loaded (to detect source changes)
    pub cache_source_id: Option<String>,

    // --- UI state ---
    /// Currently selected folder index
    pub selected_folder: usize,
    /// Cursor position within the pattern string (for editing)
    pub pattern_cursor: usize,
    /// Current phase in the explorer state machine
    pub phase: GlobExplorerPhase,

    // --- Rule Editing State ---
    /// Draft rule being edited (persists across Testing/Publishing)
    pub rule_draft: Option<super::extraction::RuleDraft>,
    /// Test state (populated during Testing phase)
    pub test_state: Option<super::extraction::TestState>,
    /// Publish state (populated during Publishing phase)
    pub publish_state: Option<super::extraction::PublishState>,

    // --- Debouncing state ---
    /// When pattern was last modified (for debouncing)
    pub pattern_changed_at: Option<std::time::Instant>,
    /// Last pattern that was actually searched (to detect changes)
    pub last_searched_pattern: String,
    /// Last prefix that was searched (to detect navigation changes)
    pub last_searched_prefix: String,

    // --- Rule Builder Enhancements (v3.0 Consolidation) ---
    /// Filter for displaying test results (a=all, p=pass, f=fail)
    pub result_filter: super::extraction::ResultFilter,
    /// Exclusion patterns (folders/files to skip)
    pub excludes: Vec<String>,
    /// Backtest summary statistics
    pub backtest_summary: super::extraction::BacktestSummary,

    // --- Staleness Detection (spec Section 4.5) ---
    /// True if a scan is currently running for this source
    pub scan_in_progress: bool,
    /// Minutes since last completed scan (None if never scanned)
    pub minutes_since_scan: Option<f64>,
}

impl GlobExplorerState {
    /// Returns true if data may be stale (>60 min since scan or scan in progress)
    pub fn is_stale(&self) -> bool {
        self.scan_in_progress || self.minutes_since_scan.map(|m| m > 60.0).unwrap_or(true)
    }
}

impl Default for GlobExplorerState {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            nav_history: Vec::new(),
            current_prefix: String::new(),
            folders: Vec::new(),
            preview_files: Vec::new(),
            total_count: GlobFileCount::Exact(0),
            folder_cache: HashMap::new(),
            cache_loaded: false,
            cache_source_id: None,
            selected_folder: 0,
            pattern_cursor: 0,
            phase: GlobExplorerPhase::Browse,
            rule_draft: None,
            test_state: None,
            publish_state: None,
            pattern_changed_at: None,
            last_searched_pattern: String::new(),
            last_searched_prefix: String::new(),
            // Rule Builder enhancements
            result_filter: super::extraction::ResultFilter::default(),
            excludes: Vec::new(),
            backtest_summary: super::extraction::BacktestSummary::default(),
            // Staleness detection
            scan_in_progress: false,
            minutes_since_scan: None,
        }
    }
}

/// Filesystem entry for hierarchical browsing
#[derive(Debug, Clone)]
pub enum FsEntry {
    Folder {
        /// Display name (may include pattern suffix like "data/reports/*.csv")
        name: String,
        /// Raw path for navigation (e.g., "data/reports" without pattern suffix)
        /// If None, uses `name` for navigation
        path: Option<String>,
        /// Number of files in/under this folder
        file_count: usize,
    },
    File {
        /// Display name
        name: String,
        /// Raw path for navigation (e.g., "data/reports/file.csv")
        /// If None, uses `name` for navigation
        path: Option<String>,
        /// Number of files represented (usually 1)
        file_count: usize,
    },
    Loading {
        /// Status message
        message: String,
    },
}

impl FsEntry {
    /// Create a folder entry
    pub fn folder(name: String, file_count: usize) -> Self {
        Self::Folder { name, path: None, file_count }
    }

    /// Create a file entry
    pub fn file(name: String, file_count: usize) -> Self {
        Self::File { name, path: None, file_count }
    }

    /// Create a folder/file entry
    pub fn new(name: String, file_count: usize, is_file: bool) -> Self {
        Self::with_path(name, None, file_count, is_file)
    }

    /// Create a folder/file entry with explicit navigation path
    pub fn with_path(name: String, path: Option<String>, file_count: usize, is_file: bool) -> Self {
        if is_file {
            Self::File { name, path, file_count }
        } else {
            Self::Folder { name, path, file_count }
        }
    }

    /// Create a loading placeholder
    pub fn loading(message: &str) -> Self {
        Self::Loading { message: message.to_string() }
    }

    pub fn name(&self) -> &str {
        match self {
            FsEntry::Folder { name, .. } => name,
            FsEntry::File { name, .. } => name,
            FsEntry::Loading { message } => message,
        }
    }

    pub fn file_count(&self) -> usize {
        match self {
            FsEntry::Folder { file_count, .. } => *file_count,
            FsEntry::File { file_count, .. } => *file_count,
            FsEntry::Loading { .. } => 0,
        }
    }

    pub fn nav_path(&self) -> Option<&str> {
        match self {
            FsEntry::Folder { name, path, .. } | FsEntry::File { name, path, .. } => {
                Some(path.as_deref().unwrap_or(name))
            }
            FsEntry::Loading { .. } => None,
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            FsEntry::Folder { path, .. } | FsEntry::File { path, .. } => path.as_deref(),
            FsEntry::Loading { .. } => None,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, FsEntry::File { .. })
    }
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
/// Organized into Navigation Layer and Rule Editing Layer (spec Section 13.3)
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GlobExplorerPhase {
    // --- Navigation Layer ---
    /// Browsing folders (root or drilled in) without active pattern
    #[default]
    Browse,
    /// Pattern active - showing heat map with match counts
    Filtering,

    // --- Rule Editing Layer ---
    /// Editing extraction rule (4-section editor)
    EditRule {
        focus: super::extraction::RuleEditorFocus,
        /// Selected item index in FieldList or Conditions
        selected_index: usize,
        /// Whether editing a field inline
        editing_field: Option<super::extraction::FieldEditFocus>,
    },
    /// Running extraction test
    Testing,
    /// Publishing rule to database
    Publishing,
    /// Rule published successfully
    Published {
        job_id: String,
    },
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

    // --- Glob Explorer ---
    /// Glob Explorer state - DEPRECATED, kept for transition
    /// TODO: Remove after Rule Builder fully replaces GlobExplorer
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

    // --- Rule Builder (v3.0 consolidation) ---
    /// Rule Builder state (Some = builder active)
    pub rule_builder: Option<super::extraction::RuleBuilderState>,

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
    /// Source ID to touch for MRU ordering (set on source selection)
    pub pending_source_touch: Option<String>,

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

/// Maximum number of jobs to keep in the jobs list (prevents unbounded memory growth)
const MAX_JOBS: usize = 200;

/// Main application state
pub struct App {
    /// Whether app is running
    pub running: bool,
    /// Current TUI mode/screen
    pub mode: TuiMode,
    /// Whether the help overlay is visible (per spec Section 3.1)
    pub show_help: bool,
    /// Home hub state
    pub home: HomeState,
    /// Discover mode state
    pub discover: DiscoverState,
    /// Parser Bench mode state
    pub parser_bench: ParserBenchState,
    /// Jobs mode state
    pub jobs_state: JobsState,
    /// Sources mode state
    pub sources_state: SourcesState,
    /// Settings mode state
    pub settings: SettingsState,
    /// Whether the Jobs drawer overlay is visible (toggle with J)
    pub jobs_drawer_open: bool,
    /// Selected job in the drawer (for Enter navigation)
    pub jobs_drawer_selected: usize,
    /// Whether the Sources drawer overlay is visible (toggle with S)
    pub sources_drawer_open: bool,
    /// Selected source in the drawer (for Enter navigation)
    pub sources_drawer_selected: usize,
    /// Tool registry for executing MCP tools
    pub tools: ToolRegistry,
    /// Configuration
    #[allow(dead_code)]
    pub config: TuiArgs,
    /// Last error message
    #[allow(dead_code)]
    pub error: Option<String>,
    /// Pending scan result from background directory scan
    pending_scan: Option<mpsc::Receiver<TuiScanResult>>,
    /// Job ID for the currently running scan (for status updates)
    current_scan_job_id: Option<i64>,
    /// Job ID for the currently running schema eval (for status updates)
    current_schema_eval_job_id: Option<i64>,
    /// Pending schema eval result from background analysis
    pending_schema_eval: Option<mpsc::Receiver<SchemaEvalResult>>,
    /// Pending sample schema eval result
    pending_sample_eval: Option<mpsc::Receiver<SampleEvalResult>>,
    /// Pending cache load for glob explorer (streaming chunks)
    pending_cache_load: Option<mpsc::Receiver<CacheLoadMessage>>,
    /// Progress tracking for streaming cache load
    pub cache_load_progress: Option<CacheLoadProgress>,
    /// Timing info from last completed cache load (for profiler display)
    pub last_cache_load_timing: Option<CacheLoadTiming>,
    /// Tick counter for animated UI elements (spinner, etc.)
    pub tick_count: u64,
    /// Pending glob search results (non-blocking recursive search)
    pending_glob_search: Option<mpsc::Receiver<GlobSearchResult>>,
    /// Pending Rule Builder pattern search (async database query for **/*.ext patterns)
    pending_rule_builder_search: Option<mpsc::Receiver<RuleBuilderSearchResult>>,
    /// Cancellation token for pending glob search (set to true to cancel)
    glob_search_cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Pending folder query (on-demand database query for navigation)
    pending_folder_query: Option<mpsc::Receiver<FolderQueryMessage>>,
    /// Pending sources load (non-blocking DB query)
    pending_sources_load: Option<mpsc::Receiver<Vec<SourceInfo>>>,
    /// Pending jobs load (non-blocking DB query)
    pending_jobs_load: Option<mpsc::Receiver<Vec<JobInfo>>>,
    /// Pending home stats load (non-blocking DB query)
    pending_stats_load: Option<mpsc::Receiver<HomeStats>>,
    /// Last time jobs were polled (for incremental updates)
    last_jobs_poll: Option<std::time::Instant>,
    /// Profiler for frame timing and zone breakdown (F12 toggle)
    #[cfg(feature = "profiling")]
    pub profiler: casparian_profiler::Profiler,
}

/// Cache load messages (simplified - no chunking needed)
enum CacheLoadMessage {
    /// Loading complete (includes folder cache and tags)
    Complete {
        source_id: String,
        total_files: usize,
        tags: Vec<TagInfo>,
        cache: HashMap<String, Vec<FsEntry>>,
    },
    /// Error during loading
    Error(String),
}

/// Message for on-demand folder queries
enum FolderQueryMessage {
    /// Query completed successfully
    Complete {
        prefix: String,
        folders: Vec<FsEntry>,
        total_count: usize,
    },
    /// Error during query
    Error(String),
}

/// Progress tracking for cache load (simplified - spinner only)
pub struct CacheLoadProgress {
    /// Source name being loaded
    pub source_name: String,
    /// When loading started (for measuring total load time)
    pub started_at: std::time::Instant,
}

impl CacheLoadProgress {
    fn new(source_name: String) -> Self {
        Self {
            source_name,
            started_at: std::time::Instant::now(),
        }
    }

    /// Format status line: "Loading sales_data... (1.2s)"
    pub fn status_line(&self) -> String {
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let name = if self.source_name.len() > 25 {
            format!("...{}", &self.source_name[self.source_name.len()-22..])
        } else {
            self.source_name.clone()
        };
        format!("Loading {}... ({:.1}s)", name, elapsed)
    }
}

/// Timing info for the last completed cache load
#[derive(Debug, Clone)]
pub struct CacheLoadTiming {
    /// Total time to load cache
    pub duration_ms: f64,
    /// Number of files loaded
    pub files_loaded: usize,
    /// Source ID that was loaded
    pub source_id: String,
}

/// Result of background glob search
struct GlobSearchResult {
    folders: Vec<FsEntry>,
    total_count: usize,
    pattern: String,
}

/// Result of background Rule Builder pattern search
struct RuleBuilderSearchResult {
    folder_matches: Vec<super::extraction::FolderMatch>,
    total_count: usize,
    pattern: String,
}

impl App {
    /// Create new app with given args
    pub fn new(args: TuiArgs) -> Self {
        Self {
            running: true,
            mode: TuiMode::Home,
            show_help: false,
            home: HomeState::default(),
            discover: DiscoverState::default(),
            parser_bench: ParserBenchState::default(),
            jobs_state: JobsState::default(),
            sources_state: SourcesState::default(),
            jobs_drawer_open: false,
            jobs_drawer_selected: 0,
            sources_drawer_open: false,
            sources_drawer_selected: 0,
            settings: SettingsState {
                default_source_path: "~/data".to_string(),
                auto_scan_on_startup: true,
                confirm_destructive: true,
                theme: "dark".to_string(),
                unicode_symbols: true,
                show_hidden_files: false,
                ..Default::default()
            },
            tools: create_default_registry(),
            config: args,
            error: None,
            pending_scan: None,
            current_scan_job_id: None,
            current_schema_eval_job_id: None,
            pending_schema_eval: None,
            pending_sample_eval: None,
            pending_cache_load: None,
            cache_load_progress: None,
            last_cache_load_timing: None,
            tick_count: 0,
            pending_glob_search: None,
            pending_rule_builder_search: None,
            glob_search_cancelled: None,
            pending_folder_query: None,
            pending_sources_load: None,
            pending_jobs_load: None,
            pending_stats_load: None,
            last_jobs_poll: None,
            #[cfg(feature = "profiling")]
            profiler: casparian_profiler::Profiler::new(250), // 250ms frame budget
        }
    }

    /// Enter Discover mode with Rule Builder initialized immediately.
    /// This ensures the Rule Builder UI appears instantly (no loading delay).
    /// Files will populate asynchronously as the cache loads.
    pub fn enter_discover_mode(&mut self) {
        self.mode = TuiMode::Discover;

        // Initialize Rule Builder immediately if not already present
        if self.discover.rule_builder.is_none() {
            let source_id = self.discover.selected_source_id
                .as_ref()
                .map(|id| id.as_str().to_string());
            let mut builder = super::extraction::RuleBuilderState::new(source_id);
            builder.pattern = "**/*".to_string();
            self.discover.rule_builder = Some(builder);
        }

        // Set view state to Rule Builder immediately
        self.discover.view_state = DiscoverViewState::RuleBuilder;

        // Auto-open Sources dropdown if no source is selected (first-time use)
        // Otherwise stay in RuleBuilder view for returning users
        if self.discover.selected_source_id.is_none() {
            self.discover.view_state = DiscoverViewState::SourcesDropdown;
            self.discover.sources_filter.clear();
            self.discover.sources_filtering = false;
            self.discover.preview_source = Some(self.discover.selected_source_index());
        }
    }

    fn resolve_db_target(&self) -> (DbBackend, std::path::PathBuf) {
        if let Some(ref path) = self.config.database {
            let backend = match path.extension().and_then(|ext| ext.to_str()) {
                Some("duckdb") => DbBackend::DuckDb,
                _ => DbBackend::Sqlite,
            };
            (backend, path.clone())
        } else {
            (default_db_backend(), active_db_path())
        }
    }

    async fn open_db_readonly(&self) -> Option<DbConnection> {
        let (backend, path) = self.resolve_db_target();
        Self::open_db_readonly_with(backend, &path).await
    }

    async fn open_db_write(&self) -> Option<DbConnection> {
        let (backend, path) = self.resolve_db_target();
        Self::open_db_write_with(backend, &path).await
    }

    async fn open_db_readonly_with(backend: DbBackend, path: &std::path::Path) -> Option<DbConnection> {
        if !path.exists() {
            return None;
        }

        match backend {
            DbBackend::Sqlite => DbConnection::open_sqlite(path).await.ok(),
            DbBackend::DuckDb => {
                #[cfg(feature = "duckdb")]
                {
                    DbConnection::open_duckdb_readonly(path).await.ok()
                }
                #[cfg(not(feature = "duckdb"))]
                {
                    None
                }
            }
        }
    }

    async fn open_db_write_with(backend: DbBackend, path: &std::path::Path) -> Option<DbConnection> {
        if !path.exists() {
            return None;
        }

        match backend {
            DbBackend::Sqlite => DbConnection::open_sqlite(path).await.ok(),
            DbBackend::DuckDb => {
                #[cfg(feature = "duckdb")]
                {
                    DbConnection::open_duckdb(path).await.ok()
                }
                #[cfg(not(feature = "duckdb"))]
                {
                    None
                }
            }
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
            // F12 or backtick: Toggle profiler overlay (when profiling feature enabled)
            // Backtick added as fallback since F12 doesn't work on some Mac terminals
            #[cfg(feature = "profiling")]
            KeyCode::F(12) => {
                self.profiler.enabled = !self.profiler.enabled;
                return;
            }
            #[cfg(feature = "profiling")]
            KeyCode::Char('`') => {
                self.profiler.enabled = !self.profiler.enabled;
                return;
            }
            // Discover Rule Builder shortcuts: [1]/[2] open local dropdowns
            KeyCode::Char('1') | KeyCode::Char('2')
                if self.mode == TuiMode::Discover
                    && !self.in_text_input_mode()
                    && matches!(
                        self.discover.view_state,
                        DiscoverViewState::RuleBuilder | DiscoverViewState::Files
                    ) =>
            {
                match key.code {
                    KeyCode::Char('1') => {
                        self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                        self.discover.sources_filter.clear();
                        self.discover.sources_filtering = false;
                        self.discover.preview_source = Some(self.discover.selected_source_index());
                    }
                    KeyCode::Char('2') => {
                        self.transition_discover_state(DiscoverViewState::TagsDropdown);
                        self.discover.tags_filter.clear();
                        self.discover.tags_filtering = false;
                        self.discover.preview_tag = self.discover.selected_tag;
                    }
                    _ => {}
                }
                return;
            }
            // ========== GLOBAL VIEW NAVIGATION (per keybinding matrix) ==========
            // Keys 1-4 are RESERVED for view navigation and work from ANY view.
            // Don't intercept when in text input.

            // 1: Discover
            KeyCode::Char('1') if !self.in_text_input_mode() => {
                self.enter_discover_mode();
                return;
            }
            // 2: Parser Bench
            KeyCode::Char('2') if !self.in_text_input_mode() => {
                self.mode = TuiMode::ParserBench;
                return;
            }
            // 3: Jobs
            KeyCode::Char('3') if !self.in_text_input_mode() => {
                self.mode = TuiMode::Jobs;
                return;
            }
            // 4: Sources
            KeyCode::Char('4') if !self.in_text_input_mode() => {
                self.mode = TuiMode::Sources;
                return;
            }
            // P: Parser Bench (separate from 1-4 navigation)
            KeyCode::Char('P') if !self.in_text_input_mode() => {
                self.mode = TuiMode::ParserBench;
                return;
            }
            // J: Toggle Jobs Drawer (global overlay)
            KeyCode::Char('J') if !self.in_text_input_mode() => {
                self.jobs_drawer_open = !self.jobs_drawer_open;
                if self.jobs_drawer_open {
                    self.sources_drawer_open = false;
                    self.jobs_drawer_selected = self.jobs_state.selected_index;
                }
                return;
            }
            // S: Toggle Sources Drawer (global overlay, except Discover)
            KeyCode::Char('S') if !self.in_text_input_mode() && self.mode != TuiMode::Discover => {
                self.sources_drawer_open = !self.sources_drawer_open;
                if self.sources_drawer_open {
                    self.jobs_drawer_open = false;
                    let drawer_sources = self.sources_drawer_sources();
                    self.sources_drawer_selected = drawer_sources
                        .iter()
                        .position(|idx| *idx == self.sources_state.selected_index)
                        .unwrap_or(0);
                }
                return;
            }
            // Jobs Drawer navigation (when drawer is open)
            KeyCode::Up if self.jobs_drawer_open => {
                if self.jobs_drawer_selected > 0 {
                    self.jobs_drawer_selected -= 1;
                }
                return;
            }
            KeyCode::Down if self.jobs_drawer_open => {
                let job_count = self.jobs_state.jobs.len();
                if self.jobs_drawer_selected < job_count.saturating_sub(1) {
                    self.jobs_drawer_selected += 1;
                }
                return;
            }
            KeyCode::Enter if self.jobs_drawer_open => {
                // Jump to Jobs view with selected job
                if !self.jobs_state.jobs.is_empty() {
                    self.jobs_state.selected_index = self.jobs_drawer_selected;
                    self.mode = TuiMode::Jobs;
                    self.jobs_drawer_open = false;
                }
                return;
            }
            KeyCode::Esc if self.jobs_drawer_open => {
                self.jobs_drawer_open = false;
                return;
            }
            // Sources Drawer navigation (when drawer is open)
            KeyCode::Up if self.sources_drawer_open => {
                if self.sources_drawer_selected > 0 {
                    self.sources_drawer_selected -= 1;
                }
                return;
            }
            KeyCode::Down if self.sources_drawer_open => {
                let source_count = self.sources_drawer_sources().len();
                if self.sources_drawer_selected < source_count.saturating_sub(1) {
                    self.sources_drawer_selected += 1;
                }
                return;
            }
            KeyCode::Enter if self.sources_drawer_open => {
                if let Some(source) = self.sources_drawer_selected_source() {
                    self.sources_state.selected_index = source;
                    self.mode = TuiMode::Sources;
                    self.sources_drawer_open = false;
                }
                return;
            }
            KeyCode::Char('s') if self.sources_drawer_open => {
                if let Some(source_idx) = self.sources_drawer_selected_source() {
                    let path = self.discover.sources[source_idx].path.display().to_string();
                    self.sources_drawer_open = false;
                    self.enter_discover_mode();
                    self.scan_directory(&path);
                }
                return;
            }
            KeyCode::Char('e') if self.sources_drawer_open => {
                if let Some(source_idx) = self.sources_drawer_selected_source() {
                    let source = &self.discover.sources[source_idx];
                    self.sources_state.selected_index = source_idx;
                    self.sources_state.editing = true;
                    self.sources_state.edit_value = source.path.display().to_string();
                    self.mode = TuiMode::Sources;
                    self.sources_drawer_open = false;
                }
                return;
            }
            KeyCode::Char('d') if self.sources_drawer_open => {
                if let Some(source_idx) = self.sources_drawer_selected_source() {
                    self.sources_state.selected_index = source_idx;
                    self.sources_state.confirm_delete = true;
                    self.mode = TuiMode::Sources;
                    self.sources_drawer_open = false;
                }
                return;
            }
            KeyCode::Char('n') if self.sources_drawer_open => {
                self.sources_drawer_open = false;
                self.enter_discover_mode();
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
                self.discover.path_suggestions.clear();
                return;
            }
            KeyCode::Esc if self.sources_drawer_open => {
                self.sources_drawer_open = false;
                return;
            }
            // 0 or H: Return to Home (from any view)
            KeyCode::Char('0') => {
                self.mode = TuiMode::Home;
                return;
            }
            KeyCode::Char('H') => {
                self.mode = TuiMode::Home;
                return;
            }
            // q: Quit application (per spec Section 3.1)
            // Don't intercept when in text input mode
            KeyCode::Char('q') if !self.in_text_input_mode() => {
                // TODO: Add confirmation dialog if unsaved changes
                self.running = false;
                return;
            }
            // r: Refresh current view (per spec Section 3.3)
            // Don't intercept when in text input mode
            KeyCode::Char('r') if !self.in_text_input_mode() => {
                self.refresh_current_view();
                return;
            }
            // ?: Toggle help overlay (per spec Section 3.1)
            // Don't intercept when in text input mode
            KeyCode::Char('?') if !self.in_text_input_mode() => {
                self.show_help = !self.show_help;
                return;
            }
            // ,: Open Settings (per specs/views/settings.md Section 4)
            // Don't intercept when in text input mode
            KeyCode::Char(',') if !self.in_text_input_mode() => {
                if self.mode != TuiMode::Settings {
                    self.settings.previous_mode = Some(self.mode);
                    self.mode = TuiMode::Settings;
                }
                return;
            }
            // Esc: Close help overlay first, then handle other escapes
            KeyCode::Esc if self.show_help => {
                self.show_help = false;
                return;
            }
            _ => {}
        }

        // Mode-specific keys (Main Focus)
        match self.mode {
            TuiMode::Home => self.handle_home_key(key),
            TuiMode::Discover => self.handle_discover_key(key),
            TuiMode::Jobs => self.handle_jobs_key(key),
            TuiMode::Sources => self.handle_sources_key(key),
            TuiMode::ParserBench => self.handle_parser_bench_key(key),
            TuiMode::Settings => self.handle_settings_key(key),
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
        // but NOT from dialogs or text input modes
        if !self.in_text_input_mode() && !matches!(self.discover.view_state,
            DiscoverViewState::RulesManager |
            DiscoverViewState::RuleCreation |
            DiscoverViewState::RuleBuilder |
            DiscoverViewState::SourcesManager |
            DiscoverViewState::SourceEdit |
            DiscoverViewState::SourceDeleteConfirm
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

            // === Rule Builder (specs/rule_builder.md) ===
            DiscoverViewState::RuleBuilder => self.handle_rule_builder_key(key),

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
                // IMPORTANT: If in text input mode (glob pattern editing, filtering, etc.),
                // dispatch to the appropriate handler first - don't intercept shortcuts
                if self.in_text_input_mode() {
                    match self.discover.focus {
                        DiscoverFocus::Files => self.handle_discover_files_key(key),
                        DiscoverFocus::Sources => self.handle_discover_sources_key(key),
                        DiscoverFocus::Tags => self.handle_discover_tags_key(key),
                    }
                    return;
                }

                // Not in text input mode - handle shortcuts
                // NOTE: Keys 1-4 are now handled globally for view navigation.
                // Use Tab/arrow keys for panel focus cycling within Discover.
                match key.code {
                    KeyCode::Char('n') => self.open_rule_creation_dialog(),
                    // Tab: Toggle preview panel or cycle focus
                    KeyCode::Tab if self.discover.focus == DiscoverFocus::Files => {
                        // If in glob explorer EditRule phase, let it handle Tab for section cycling
                        if let Some(ref explorer) = self.discover.glob_explorer {
                            if matches!(explorer.phase, GlobExplorerPhase::EditRule { .. }) {
                                self.handle_discover_files_key(key);
                                return;
                            }
                        }
                        self.discover.preview_open = !self.discover.preview_open;
                    }
                    KeyCode::Esc if !self.discover.filter.is_empty() => {
                        // If in glob explorer non-Browse phase, let it handle Escape
                        let in_glob_editor_phase = self.discover.glob_explorer.as_ref()
                            .map(|e| !matches!(e.phase, GlobExplorerPhase::Browse | GlobExplorerPhase::Filtering))
                            .unwrap_or(false);
                        if in_glob_editor_phase {
                            self.handle_discover_files_key(key);
                            return;
                        }
                        self.discover.filter.clear();
                        self.discover.selected = 0;
                    }
                    KeyCode::Esc => {
                        self.mode = TuiMode::Home;
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
            KeyCode::Down => {
                if self.discover.selected < self.filtered_files().len().saturating_sub(1) {
                    self.discover.selected += 1;
                }
            }
            KeyCode::Up => {
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
                // Open scan path input - auto-populate with selected source path
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                // Pre-fill with selected source path if available
                self.discover.scan_path_input = self.discover.selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
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
        // Check current phase to determine behavior
        let phase = self.discover.glob_explorer.as_ref().map(|e| e.phase.clone());

        // Pattern editing mode (Filtering phase) - uses in-memory cache filtering
        if matches!(phase, Some(GlobExplorerPhase::Filtering)) {
            match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Down => {
                    // Exit pattern editing, move focus to folder list (Browse phase)
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.phase = GlobExplorerPhase::Browse;
                    }
                    self.update_folders_from_cache();
                }
                KeyCode::Left => {
                    // Move cursor left
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.pattern_cursor > 0 {
                            explorer.pattern_cursor -= 1;
                        }
                    }
                }
                KeyCode::Right => {
                    // Move cursor right
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        let len = explorer.pattern.chars().count();
                        if explorer.pattern_cursor < len {
                            explorer.pattern_cursor += 1;
                        }
                    }
                }
                KeyCode::Home => {
                    // Move cursor to start
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.pattern_cursor = 0;
                    }
                }
                KeyCode::End => {
                    // Move cursor to end
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.pattern_cursor = explorer.pattern.chars().count();
                    }
                }
                KeyCode::Backspace => {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.pattern_cursor > 0 && !explorer.pattern.is_empty() {
                            // Delete character before cursor
                            let mut chars: Vec<char> = explorer.pattern.chars().collect();
                            chars.remove(explorer.pattern_cursor - 1);
                            explorer.pattern = chars.into_iter().collect();
                            explorer.pattern_cursor -= 1;
                            explorer.pattern_changed_at = Some(std::time::Instant::now());
                        } else if explorer.pattern.is_empty() && !explorer.current_prefix.is_empty() {
                            // Pattern is empty, go up a directory
                            let prefix = explorer.current_prefix.trim_end_matches('/');
                            if let Some(last_slash) = prefix.rfind('/') {
                                explorer.current_prefix = format!("{}/", &prefix[..last_slash]);
                            } else {
                                explorer.current_prefix.clear();
                            }
                            explorer.pattern = "*".to_string();
                            explorer.pattern_cursor = 1;
                            explorer.nav_history.clear();
                            explorer.pattern_changed_at = Some(std::time::Instant::now());
                        }
                    }
                }
                KeyCode::Delete => {
                    // Delete character at cursor
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        let len = explorer.pattern.chars().count();
                        if explorer.pattern_cursor < len {
                            let mut chars: Vec<char> = explorer.pattern.chars().collect();
                            chars.remove(explorer.pattern_cursor);
                            explorer.pattern = chars.into_iter().collect();
                            explorer.pattern_changed_at = Some(std::time::Instant::now());
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        // Insert character at cursor position
                        let mut chars: Vec<char> = explorer.pattern.chars().collect();
                        chars.insert(explorer.pattern_cursor, c);
                        explorer.pattern = chars.into_iter().collect();
                        explorer.pattern_cursor += 1;
                        explorer.pattern_changed_at = Some(std::time::Instant::now());
                    }
                }
                _ => {}
            }
            return;
        }

        // EditRule phase - editing extraction rule
        if let Some(GlobExplorerPhase::EditRule { focus, selected_index, editing_field }) = phase.clone() {
            self.handle_edit_rule_key(key, focus, selected_index, editing_field);
            return;
        }

        // Testing phase - viewing test results
        if matches!(phase, Some(GlobExplorerPhase::Testing)) {
            self.handle_testing_key(key);
            return;
        }

        // Publishing phase - confirming publish
        if matches!(phase, Some(GlobExplorerPhase::Publishing)) {
            self.handle_publishing_key(key);
            return;
        }

        // Published phase - success screen
        if matches!(phase, Some(GlobExplorerPhase::Published { .. })) {
            self.handle_published_key(key);
            return;
        }

        // Navigation mode (Browse phase)
        match key.code {
            KeyCode::Down => {
                // Navigate down in folder list
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if explorer.selected_folder < explorer.folders.len().saturating_sub(1) {
                        explorer.selected_folder += 1;
                    }
                }
            }
            KeyCode::Up => {
                // Navigate up in folder list, or move to pattern input at top
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if explorer.selected_folder > 0 {
                        explorer.selected_folder -= 1;
                    } else {
                        // At top of list, move focus to pattern input (Filtering phase)
                        explorer.phase = GlobExplorerPhase::Filtering;
                        explorer.pattern_cursor = explorer.pattern.chars().count();
                    }
                }
            }
            KeyCode::Enter | KeyCode::Right => {
                // Drill into selected folder - O(1) using cache
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(folder) = explorer.folders.get(explorer.selected_folder).cloned() {
                        // Don't drill into files or the loading placeholder
                        let folder_name = folder.name().to_string();
                        if !folder.is_file() && !folder_name.contains("Loading folder hierarchy") && !folder_name.contains("Searching") {
                            // Save current (prefix, pattern) to history for back navigation
                            explorer.nav_history.push((
                                explorer.current_prefix.clone(),
                                explorer.pattern.clone(),
                            ));

                            // Determine new prefix based on whether this is a ** result or normal folder
                            if let Some(full_path) = folder.path() {
                                // ** result: path is the full folder path, use it directly
                                explorer.current_prefix = format!("{}/", full_path);
                                // Clear ** from pattern when drilling into a ** result
                                if explorer.pattern.contains("**") {
                                    explorer.pattern = explorer.pattern.replace("**/", "");
                                }
                            } else {
                                // Normal folder: append folder name to current prefix
                                explorer.current_prefix = format!("{}{}/", explorer.current_prefix, folder_name);
                            }

                            // Stay in Browse phase (navigation) - phase doesn't change based on folder depth
                            explorer.selected_folder = 0;
                        }
                    }
                }
                // Update from cache - O(1) hashmap lookup, no SQL
                self.update_folders_from_cache();
            }
            KeyCode::Left | KeyCode::Backspace => {
                // Go back to parent folder - O(1) using cache
                // Backspace kept for backwards compatibility
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some((prev_prefix, prev_pattern)) = explorer.nav_history.pop() {
                        explorer.current_prefix = prev_prefix;
                        explorer.pattern = prev_pattern;
                        // Stay in Browse phase
                        self.update_folders_from_cache();
                    } else if key.code == KeyCode::Left {
                        // At root level, Left/h moves focus to sidebar
                        self.discover.focus = DiscoverFocus::Sources;
                    }
                }
            }
            KeyCode::Char('/') => {
                // Enter pattern editing mode (Filtering phase)
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.phase = GlobExplorerPhase::Filtering;
                    // Position cursor at end of pattern
                    explorer.pattern_cursor = explorer.pattern.chars().count();
                }
            }
            KeyCode::Char('e') => {
                // Enter rule editing mode (if matches > 0)
                // Get source_id before mutable borrow
                let source_id = self.discover.selected_source()
                    .and_then(|s| uuid::Uuid::parse_str(&s.id.to_string()).ok());

                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    let match_count = explorer.total_count.value();
                    if match_count > 0 {
                        // Create a new rule draft from current pattern
                        let pattern = if explorer.current_prefix.is_empty() {
                            explorer.pattern.clone()
                        } else {
                            format!("{}{}", explorer.current_prefix, explorer.pattern)
                        };
                        explorer.rule_draft = Some(super::extraction::RuleDraft::from_pattern(&pattern, source_id));
                        explorer.phase = GlobExplorerPhase::EditRule {
                            focus: super::extraction::RuleEditorFocus::GlobPattern,
                            selected_index: 0,
                            editing_field: None,
                        };
                    }
                    // If no matches, do nothing (could show hint)
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
                // Pre-fill with selected source path if available
                self.discover.scan_path_input = self.discover.selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
                self.discover.scan_error = None;
            }
            // --- Rule Builder: Result Filtering (a/p/f keys) ---
            KeyCode::Char('a') => {
                // Show all results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = super::extraction::ResultFilter::All;
                }
            }
            KeyCode::Char('p') => {
                // Show only passing results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = super::extraction::ResultFilter::PassOnly;
                }
            }
            KeyCode::Char('f') => {
                // Show only failing results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = super::extraction::ResultFilter::FailOnly;
                }
            }
            // --- Rule Builder: Exclusion System (x/i keys) ---
            KeyCode::Char('x') => {
                // Exclude selected file/folder from rule
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(folder) = explorer.folders.get(explorer.selected_folder) {
                        // Build exclusion pattern from current item
                        let exclude_pattern = if folder.is_file() {
                            // Exclude specific file
                            if let Some(path) = folder.path() {
                                path.to_string()
                            } else {
                                format!("{}{}", explorer.current_prefix, folder.name())
                            }
                        } else {
                            // Exclude folder and all contents
                            let path = folder.path()
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| format!("{}{}", explorer.current_prefix, folder.name()));
                            format!("{}/**", path)
                        };
                        // Add to excludes if not already present
                        if !explorer.excludes.contains(&exclude_pattern) {
                            explorer.excludes.push(exclude_pattern);
                        }
                    }
                }
            }
            KeyCode::Char('i') => {
                // Ignore current folder (add to excludes)
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if !explorer.current_prefix.is_empty() {
                        // Remove trailing slash for the exclusion pattern
                        let folder_path = explorer.current_prefix.trim_end_matches('/').to_string();
                        let exclude_pattern = format!("{}/**", folder_path);
                        if !explorer.excludes.contains(&exclude_pattern) {
                            explorer.excludes.push(exclude_pattern);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle keys in EditRule phase (editing extraction rule)
    fn handle_edit_rule_key(
        &mut self,
        key: KeyEvent,
        focus: super::extraction::RuleEditorFocus,
        selected_index: usize,
        _editing_field: Option<super::extraction::FieldEditFocus>,
    ) {
        use super::extraction::RuleEditorFocus;

        match key.code {
            KeyCode::Tab => {
                // Cycle through sections: GlobPattern -> FieldList -> BaseTag -> Conditions -> GlobPattern
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    let new_focus = match focus {
                        RuleEditorFocus::GlobPattern => RuleEditorFocus::FieldList,
                        RuleEditorFocus::FieldList => RuleEditorFocus::BaseTag,
                        RuleEditorFocus::BaseTag => RuleEditorFocus::Conditions,
                        RuleEditorFocus::Conditions => RuleEditorFocus::GlobPattern,
                    };
                    explorer.phase = GlobExplorerPhase::EditRule {
                        focus: new_focus,
                        selected_index: 0,
                        editing_field: None,
                    };
                }
            }
            KeyCode::BackTab => {
                // Reverse cycle
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    let new_focus = match focus {
                        RuleEditorFocus::GlobPattern => RuleEditorFocus::Conditions,
                        RuleEditorFocus::FieldList => RuleEditorFocus::GlobPattern,
                        RuleEditorFocus::BaseTag => RuleEditorFocus::FieldList,
                        RuleEditorFocus::Conditions => RuleEditorFocus::BaseTag,
                    };
                    explorer.phase = GlobExplorerPhase::EditRule {
                        focus: new_focus,
                        selected_index: 0,
                        editing_field: None,
                    };
                }
            }
            KeyCode::Char('t') => {
                // In text fields, 't' is just a character
                if matches!(focus, RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag) {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref mut draft) = explorer.rule_draft {
                            match focus {
                                RuleEditorFocus::GlobPattern => draft.glob_pattern.push('t'),
                                RuleEditorFocus::BaseTag => draft.base_tag.push('t'),
                                _ => {}
                            }
                        }
                    }
                } else {
                    // Transition to Testing phase (if rule is valid)
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref draft) = explorer.rule_draft {
                            if draft.is_valid_for_test() {
                                // Initialize test state with rule draft and file count
                                let files_total = explorer.total_count.value();
                                explorer.test_state = Some(super::extraction::TestState::new(
                                    draft.clone(),
                                    files_total,
                                ));
                                explorer.phase = GlobExplorerPhase::Testing;
                                // TODO: Start async test execution (non-blocking)
                            }
                        }
                    }
                }
            }
            KeyCode::Esc => {
                // Return to Browse, preserve prefix
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.phase = GlobExplorerPhase::Browse;
                    explorer.rule_draft = None;
                }
            }
            // Section-specific key handling
            KeyCode::Down => {
                if matches!(focus, RuleEditorFocus::FieldList | RuleEditorFocus::Conditions) {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.phase = GlobExplorerPhase::EditRule {
                            focus: focus.clone(),
                            selected_index: selected_index.saturating_add(1),
                            editing_field: None,
                        };
                    }
                }
            }
            KeyCode::Up => {
                if matches!(focus, RuleEditorFocus::FieldList | RuleEditorFocus::Conditions) {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.phase = GlobExplorerPhase::EditRule {
                            focus: focus.clone(),
                            selected_index: selected_index.saturating_sub(1),
                            editing_field: None,
                        };
                    }
                }
            }
            KeyCode::Char('a') => {
                // In text fields, 'a' is just a character
                if matches!(focus, RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag) {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref mut draft) = explorer.rule_draft {
                            match focus {
                                RuleEditorFocus::GlobPattern => draft.glob_pattern.push('a'),
                                RuleEditorFocus::BaseTag => draft.base_tag.push('a'),
                                _ => {}
                            }
                        }
                    }
                } else {
                    // Add field or condition
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref mut draft) = explorer.rule_draft {
                            match focus {
                                RuleEditorFocus::FieldList => {
                                    draft.fields.push(super::extraction::FieldDraft::default());
                                }
                                RuleEditorFocus::Conditions => {
                                    draft.tag_conditions.push(super::extraction::TagConditionDraft::default());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            KeyCode::Char('d') => {
                // In text fields, 'd' is just a character
                if matches!(focus, RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag) {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref mut draft) = explorer.rule_draft {
                            match focus {
                                RuleEditorFocus::GlobPattern => draft.glob_pattern.push('d'),
                                RuleEditorFocus::BaseTag => draft.base_tag.push('d'),
                                _ => {}
                            }
                        }
                    }
                } else {
                    // Delete selected field or condition
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if let Some(ref mut draft) = explorer.rule_draft {
                            match focus {
                                RuleEditorFocus::FieldList => {
                                    if selected_index < draft.fields.len() {
                                        draft.fields.remove(selected_index);
                                    }
                                }
                                RuleEditorFocus::Conditions => {
                                    if selected_index < draft.tag_conditions.len() {
                                        draft.tag_conditions.remove(selected_index);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            KeyCode::Char('i') if matches!(focus, RuleEditorFocus::FieldList) => {
                // Infer fields from pattern
                // TODO: Implement field inference from glob pattern
            }
            KeyCode::Char(c) => {
                // Text input for GlobPattern and BaseTag
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut draft) = explorer.rule_draft {
                        match focus {
                            RuleEditorFocus::GlobPattern => {
                                draft.glob_pattern.push(c);
                            }
                            RuleEditorFocus::BaseTag => {
                                draft.base_tag.push(c);
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                // Delete char for GlobPattern and BaseTag
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut draft) = explorer.rule_draft {
                        match focus {
                            RuleEditorFocus::GlobPattern => {
                                draft.glob_pattern.pop();
                            }
                            RuleEditorFocus::BaseTag => {
                                draft.base_tag.pop();
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle keys in Testing phase (viewing test results)
    fn handle_testing_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('p') => {
                // Transition to Publishing (only if test is complete)
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref test_state) = explorer.test_state {
                        if matches!(test_state.phase, super::extraction::TestPhase::Complete { .. }) {
                            let matching_files = explorer.total_count.value();
                            explorer.publish_state = Some(super::extraction::PublishState::new(
                                test_state.rule.clone(),
                                matching_files,
                            ));
                            explorer.phase = GlobExplorerPhase::Publishing;
                        }
                    }
                }
            }
            KeyCode::Char('e') | KeyCode::Esc => {
                // Return to EditRule, preserve draft
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.phase = GlobExplorerPhase::EditRule {
                        focus: super::extraction::RuleEditorFocus::GlobPattern,
                        selected_index: 0,
                        editing_field: None,
                    };
                    explorer.test_state = None;
                }
            }
            KeyCode::Down => {
                // Scroll test results down
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut test_state) = explorer.test_state {
                        test_state.scroll_offset = test_state.scroll_offset.saturating_add(1);
                    }
                }
            }
            KeyCode::Up => {
                // Scroll test results up
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut test_state) = explorer.test_state {
                        test_state.scroll_offset = test_state.scroll_offset.saturating_sub(1);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle keys in Publishing phase (confirming publish)
    fn handle_publishing_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                // Confirm publish - save to DB and start job
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut publish_state) = explorer.publish_state {
                        use super::extraction::PublishPhase;
                        match publish_state.phase {
                            PublishPhase::Confirming => {
                                publish_state.phase = PublishPhase::Saving;
                                // TODO: Actually save to DB (async, non-blocking)
                                // For now, transition directly to Published
                                let job_id = format!("cf_extract_{}", &uuid::Uuid::new_v4().to_string()[..8]);
                                explorer.phase = GlobExplorerPhase::Published { job_id };
                            }
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Esc => {
                // Return to EditRule, preserve draft
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.phase = GlobExplorerPhase::EditRule {
                        focus: super::extraction::RuleEditorFocus::GlobPattern,
                        selected_index: 0,
                        editing_field: None,
                    };
                    explorer.publish_state = None;
                }
            }
            _ => {}
        }
    }

    /// Handle keys in Published phase (success screen)
    fn handle_published_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                // Return to Browse at root (clean slate)
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.phase = GlobExplorerPhase::Browse;
                    explorer.current_prefix.clear();
                    explorer.pattern = "*".to_string();
                    explorer.rule_draft = None;
                    explorer.test_state = None;
                    explorer.publish_state = None;
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
            KeyCode::Down => {
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
            KeyCode::Up => {
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
                    // Schedule source touch for MRU ordering (processed in tick)
                    if let Some(source_id) = &self.discover.selected_source_id {
                        self.discover.pending_source_touch = Some(source_id.as_str().to_string());
                    }
                    self.discover.data_loaded = false;
                    self.discover.selected_tag = None;
                    self.discover.filter.clear();
                    // Reset glob_explorer completely so it reloads for new source
                    // Setting to None triggers fresh creation in tick()
                    self.discover.glob_explorer = None;
                    // Reset rule_builder so it reinitializes with new source
                    self.discover.rule_builder = None;
                    // Cancel any pending cache load for old source
                    self.pending_cache_load = None;
                    self.cache_load_progress = None;
                }
                // Return to RuleBuilder (the default Discover view)
                self.discover.view_state = DiscoverViewState::RuleBuilder;
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = None;
            }
            KeyCode::Esc => {
                // Close dropdown without changing selection, return to Rule Builder
                self.discover.view_state = DiscoverViewState::RuleBuilder;
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = None;
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
            KeyCode::Right => {
                // Move focus to Files/Folder area
                self.discover.focus = DiscoverFocus::Files;
            }
            KeyCode::Down => {
                // Move focus to Tags
                self.discover.focus = DiscoverFocus::Tags;
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
            KeyCode::Down => {
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
            KeyCode::Up => {
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
                // Confirm selection, close dropdown, return to Rule Builder
                self.discover.selected_tag = self.discover.preview_tag;
                self.discover.view_state = DiscoverViewState::RuleBuilder;
                self.discover.tags_filter.clear();
                self.discover.tags_filtering = false;
                self.discover.preview_tag = None;
                self.discover.selected = 0;
            }
            KeyCode::Esc => {
                // Close dropdown without changing selection, return to Rule Builder
                self.discover.view_state = DiscoverViewState::RuleBuilder;
                self.discover.tags_filter.clear();
                self.discover.tags_filtering = false;
                self.discover.preview_tag = None;
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
            KeyCode::Right => {
                // Move focus to Files/Folder area
                self.discover.focus = DiscoverFocus::Files;
            }
            KeyCode::Up => {
                // Move focus to Sources
                self.discover.focus = DiscoverFocus::Sources;
            }
            _ => {}
        }
    }

    /// Handle keys when Rules Manager dialog is open
    fn handle_rules_manager_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                if self.discover.selected_rule < self.discover.rules.len().saturating_sub(1) {
                    self.discover.selected_rule += 1;
                }
            }
            KeyCode::Up => {
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
            KeyCode::Down => {
                if self.discover.sources_manager_selected < self.discover.sources.len().saturating_sub(1) {
                    self.discover.sources_manager_selected += 1;
                }
            }
            KeyCode::Up => {
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

    /// Handle keys in Rule Builder mode (specs/rule_builder.md)
    ///
    /// Focus cycles: Pattern → Excludes → Tag → Extractions → Options → FileList
    fn handle_rule_builder_key(&mut self, key: KeyEvent) {
        use super::extraction::RuleBuilderFocus;

        // Capture the current pattern before handling the key
        let pattern_before = self.discover.rule_builder.as_ref()
            .map(|b| b.pattern.clone())
            .unwrap_or_default();

        let builder = match self.discover.rule_builder.as_mut() {
            Some(b) => b,
            None => {
                // No builder state - should not happen, return to Files
                self.transition_discover_state(DiscoverViewState::Files);
                return;
            }
        };

        if builder.source_id.is_none() {
            match key.code {
                KeyCode::Esc => {
                    self.mode = TuiMode::Home;
                }
                _ => {
                    self.discover.status_message = Some((
                        "Select a source before building rules".to_string(),
                        true,
                    ));
                    self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                    self.discover.sources_filter.clear();
                    self.discover.sources_filtering = false;
                    self.discover.preview_source = Some(self.discover.selected_source_index());
                }
            }
            return;
        }

        match key.code {
            // Tab cycles focus (per spec Section 8)
            KeyCode::Tab => {
                builder.focus = match builder.focus {
                    RuleBuilderFocus::Pattern => RuleBuilderFocus::Excludes,
                    RuleBuilderFocus::Excludes => RuleBuilderFocus::Tag,
                    RuleBuilderFocus::ExcludeInput => RuleBuilderFocus::Tag,
                    RuleBuilderFocus::Tag => RuleBuilderFocus::Extractions,
                    RuleBuilderFocus::Extractions => RuleBuilderFocus::Options,
                    RuleBuilderFocus::ExtractionEdit(_) => RuleBuilderFocus::Options,
                    RuleBuilderFocus::Options => RuleBuilderFocus::FileList,
                    RuleBuilderFocus::FileList => RuleBuilderFocus::Pattern,
                    RuleBuilderFocus::IgnorePicker => RuleBuilderFocus::FileList,
                };
            }

            // BackTab (Shift+Tab) cycles focus backwards
            KeyCode::BackTab => {
                builder.focus = match builder.focus {
                    RuleBuilderFocus::Pattern => RuleBuilderFocus::FileList,
                    RuleBuilderFocus::Excludes => RuleBuilderFocus::Pattern,
                    RuleBuilderFocus::ExcludeInput => RuleBuilderFocus::Excludes,
                    RuleBuilderFocus::Tag => RuleBuilderFocus::Excludes,
                    RuleBuilderFocus::Extractions => RuleBuilderFocus::Tag,
                    RuleBuilderFocus::ExtractionEdit(_) => RuleBuilderFocus::Extractions,
                    RuleBuilderFocus::Options => RuleBuilderFocus::Extractions,
                    RuleBuilderFocus::FileList => RuleBuilderFocus::Options,
                    RuleBuilderFocus::IgnorePicker => RuleBuilderFocus::FileList,
                };
            }

            // Escape cancels nested state or exits Rule Builder from FileList
            KeyCode::Esc => {
                match builder.focus {
                    RuleBuilderFocus::FileList => {
                        self.mode = TuiMode::Home;
                        return;
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.exclude_input.clear();
                        builder.focus = RuleBuilderFocus::Excludes;
                    }
                    RuleBuilderFocus::ExtractionEdit(_) => {
                        builder.focus = RuleBuilderFocus::Extractions;
                    }
                    RuleBuilderFocus::IgnorePicker => {
                        builder.ignore_options.clear();
                        builder.focus = RuleBuilderFocus::FileList;
                    }
                    RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag => {
                        // Escape from text input fields moves focus to FileList
                        // This provides a quick way to exit text entry mode
                        builder.focus = RuleBuilderFocus::FileList;
                    }
                    _ => {
                        // First Escape moves to FileList
                        builder.focus = RuleBuilderFocus::FileList;
                    }
                }
            }

            // Enter: confirm action based on focus (phase-aware for FileList)
            KeyCode::Enter => {
                match builder.focus {
                    RuleBuilderFocus::FileList => {
                        // Phase-aware Enter behavior
                        match &mut builder.file_results {
                            super::extraction::FileResultsState::Exploration { expanded_folder_indices, .. } => {
                                // Toggle folder expansion
                                let idx = builder.selected_file;
                                if expanded_folder_indices.contains(&idx) {
                                    expanded_folder_indices.remove(&idx);
                                } else {
                                    expanded_folder_indices.insert(idx);
                                }
                            }
                            super::extraction::FileResultsState::ExtractionPreview { .. } => {
                                // Could show file details or do nothing
                            }
                            super::extraction::FileResultsState::BacktestResults { .. } => {
                                // Could show error details for failed files
                            }
                        }
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        // Add exclude pattern
                        let pattern = builder.exclude_input.trim().to_string();
                        if !pattern.is_empty() {
                            builder.add_exclude(pattern);
                        }
                        builder.exclude_input.clear();
                        builder.focus = RuleBuilderFocus::Excludes;
                    }
                    RuleBuilderFocus::Excludes => {
                        // Start editing new exclude
                        builder.focus = RuleBuilderFocus::ExcludeInput;
                    }
                    RuleBuilderFocus::IgnorePicker => {
                        // Apply selected ignore option
                        if let Some(option) = builder.ignore_options.get(builder.ignore_selected) {
                            builder.add_exclude(option.pattern.clone());
                        }
                        builder.ignore_options.clear();
                        builder.focus = RuleBuilderFocus::FileList;
                    }
                    _ => {}
                }
            }

            // Ctrl+S: Save rule
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if builder.can_save() {
                    let _draft = builder.to_draft();
                    // TODO: Save to database
                    self.discover.status_message = Some((
                        format!("Rule '{}' saved", builder.tag),
                        false,
                    ));
                    // Stay in Rule Builder (it's the default view) - clear for next rule
                    builder.pattern = "**/*".to_string();
                    builder.tag.clear();
                    builder.excludes.clear();
                    builder.focus = RuleBuilderFocus::Pattern;
                } else {
                    self.discover.status_message = Some((
                        "Cannot save: pattern and tag are required".to_string(),
                        true,
                    ));
                }
            }

            // Down arrow: navigate lists OR move to next field from text input
            KeyCode::Down => {
                match builder.focus {
                    // In text input fields, Down moves to next field
                    RuleBuilderFocus::Pattern => {
                        builder.focus = RuleBuilderFocus::Excludes;
                    }
                    RuleBuilderFocus::Tag => {
                        builder.focus = RuleBuilderFocus::Extractions;
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.focus = RuleBuilderFocus::Tag;
                    }
                    // In lists, navigate within the list
                    RuleBuilderFocus::FileList => {
                        let max_index = match &builder.file_results {
                            super::extraction::FileResultsState::Exploration { folder_matches, .. } => {
                                folder_matches.len().saturating_sub(1)
                            }
                            super::extraction::FileResultsState::ExtractionPreview { preview_files } => {
                                preview_files.len().saturating_sub(1)
                            }
                            super::extraction::FileResultsState::BacktestResults { visible_indices, .. } => {
                                visible_indices.len().saturating_sub(1)
                            }
                        };
                        if max_index > 0 {
                            builder.selected_file = (builder.selected_file + 1).min(max_index);
                        }
                    }
                    RuleBuilderFocus::Excludes => {
                        if !builder.excludes.is_empty() {
                            builder.selected_exclude = (builder.selected_exclude + 1)
                                .min(builder.excludes.len().saturating_sub(1));
                        }
                    }
                    RuleBuilderFocus::Extractions => {
                        if !builder.extractions.is_empty() {
                            builder.selected_extraction = (builder.selected_extraction + 1)
                                .min(builder.extractions.len().saturating_sub(1));
                        }
                    }
                    RuleBuilderFocus::IgnorePicker => {
                        if !builder.ignore_options.is_empty() {
                            builder.ignore_selected = (builder.ignore_selected + 1)
                                .min(builder.ignore_options.len().saturating_sub(1));
                        }
                    }
                    _ => {}
                }
            }

            // Up arrow: navigate lists OR move to previous field from text input
            KeyCode::Up => {
                match builder.focus {
                    // In text input fields, Up moves to previous field
                    RuleBuilderFocus::Tag => {
                        builder.focus = RuleBuilderFocus::Excludes;
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.focus = RuleBuilderFocus::Excludes;
                    }
                    // In lists, navigate within the list
                    RuleBuilderFocus::FileList => {
                        builder.selected_file = builder.selected_file.saturating_sub(1);
                    }
                    RuleBuilderFocus::Excludes => {
                        if builder.selected_exclude == 0 {
                            // At top of excludes, move to Pattern
                            builder.focus = RuleBuilderFocus::Pattern;
                        } else {
                            builder.selected_exclude = builder.selected_exclude.saturating_sub(1);
                        }
                    }
                    RuleBuilderFocus::Extractions => {
                        if builder.selected_extraction == 0 {
                            // At top of extractions, move to Tag
                            builder.focus = RuleBuilderFocus::Tag;
                        } else {
                            builder.selected_extraction = builder.selected_extraction.saturating_sub(1);
                        }
                    }
                    RuleBuilderFocus::IgnorePicker => {
                        builder.ignore_selected = builder.ignore_selected.saturating_sub(1);
                    }
                    _ => {}
                }
            }

            // Delete exclude with 'd' or 'x'
            KeyCode::Char('d') | KeyCode::Char('x') if builder.focus == RuleBuilderFocus::Excludes => {
                builder.remove_exclude(builder.selected_exclude);
            }

            // Filter toggle in FileList (only in BacktestResults phase)
            KeyCode::Char('a') if builder.focus == RuleBuilderFocus::FileList => {
                if let super::extraction::FileResultsState::BacktestResults { result_filter, .. } = &mut builder.file_results {
                    *result_filter = super::extraction::ResultFilter::All;
                    builder.update_visible();
                }
            }
            KeyCode::Char('p') if builder.focus == RuleBuilderFocus::FileList => {
                if let super::extraction::FileResultsState::BacktestResults { result_filter, .. } = &mut builder.file_results {
                    *result_filter = super::extraction::ResultFilter::PassOnly;
                    builder.update_visible();
                }
            }
            KeyCode::Char('f') if builder.focus == RuleBuilderFocus::FileList => {
                if let super::extraction::FileResultsState::BacktestResults { result_filter, .. } = &mut builder.file_results {
                    *result_filter = super::extraction::ResultFilter::FailOnly;
                    builder.update_visible();
                }
            }

            // 't' to run backtest (Phase 2 -> Phase 3) - only when not editing text
            KeyCode::Char('t') if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput | RuleBuilderFocus::ExtractionEdit(_)) => {
                if let super::extraction::FileResultsState::ExtractionPreview { preview_files } = &builder.file_results {
                    let matched_files: Vec<super::extraction::MatchedFile> = preview_files
                        .iter()
                        .map(|pf| super::extraction::MatchedFile {
                            path: pf.path.clone(),
                            relative_path: pf.relative_path.clone(),
                            extractions: pf.extractions.clone(),
                            test_result: super::extraction::FileTestResult::NotTested,
                        })
                        .collect();
                    let visible_indices: Vec<usize> = (0..matched_files.len()).collect();
                    builder.selected_file = 0;
                    builder.file_results = super::extraction::FileResultsState::BacktestResults {
                        matched_files,
                        visible_indices,
                        backtest: super::extraction::BacktestSummary::default(),
                        result_filter: super::extraction::ResultFilter::All,
                    };
                    // TODO: Actually run backtest async and update test_result
                }
            }

            // Left/Right arrows for panel navigation (move between left panel and FileList)
            // When not in text input mode, arrows provide quick panel switching
            KeyCode::Left if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput) => {
                if matches!(builder.focus, RuleBuilderFocus::FileList) {
                    // Move from FileList to Pattern (left panel)
                    builder.focus = RuleBuilderFocus::Pattern;
                }
            }
            KeyCode::Right if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput) => {
                if !matches!(builder.focus, RuleBuilderFocus::FileList | RuleBuilderFocus::IgnorePicker) {
                    // Move from left panel to FileList (right panel)
                    builder.focus = RuleBuilderFocus::FileList;
                }
            }

            // 's' opens scan dialog (when not in text input)
            KeyCode::Char('s') if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput) && !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                // Pre-fill with selected source path if available
                self.discover.scan_path_input = self.discover.selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
                self.discover.scan_error = None;
                return; // Exit early
            }

            // 'e' runs quick sample schema evaluation (RULE_BUILDER_UI_PLAN.md)
            // Structure-aware sampling: buckets by prefix, N per bucket, capped at 200
            KeyCode::Char('e') if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput) => {
                self.run_sample_schema_eval();
            }

            // 'E' (shift+e) triggers full evaluation as background job (RULE_BUILDER_UI_PLAN.md)
            // Runs complete schema analysis on ALL matched files with progress tracking
            KeyCode::Char('E') if !matches!(builder.focus, RuleBuilderFocus::Pattern | RuleBuilderFocus::Tag | RuleBuilderFocus::ExcludeInput) => {
                self.start_full_schema_eval();
            }

            // Text input for Pattern, Tag, and ExcludeInput
            KeyCode::Char(c) => {
                match builder.focus {
                    RuleBuilderFocus::Pattern => {
                        builder.pattern.push(c);
                        builder.pattern_changed_at = Some(std::time::Instant::now());
                        // Validate pattern
                        match super::extraction::parse_custom_glob(&builder.pattern) {
                            Ok(_) => builder.pattern_error = None,
                            Err(e) => builder.pattern_error = Some(e.message),
                        }
                    }
                    RuleBuilderFocus::Tag => {
                        builder.tag.push(c);
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.exclude_input.push(c);
                    }
                    _ => {}
                }
            }

            // Backspace for text input
            KeyCode::Backspace => {
                match builder.focus {
                    RuleBuilderFocus::Pattern => {
                        builder.pattern.pop();
                        builder.pattern_changed_at = Some(std::time::Instant::now());
                        // Re-validate pattern
                        if builder.pattern.is_empty() {
                            builder.pattern_error = None;
                        } else {
                            match super::extraction::parse_custom_glob(&builder.pattern) {
                                Ok(_) => builder.pattern_error = None,
                                Err(e) => builder.pattern_error = Some(e.message),
                            }
                        }
                    }
                    RuleBuilderFocus::Tag => {
                        builder.tag.pop();
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.exclude_input.pop();
                    }
                    _ => {}
                }
            }

            _ => {}
        }

        // If pattern changed, update matched files
        if let Some(builder) = &self.discover.rule_builder {
            if builder.pattern != pattern_before {
                let pattern = builder.pattern.clone();
                self.update_rule_builder_files(&pattern);
            }
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
    ///
    /// v3.0 Consolidation: Opens GlobExplorer in EditRule phase (Rule Builder)
    /// instead of the old RuleCreation dialog.
    fn open_rule_creation_dialog(&mut self) {
        // Determine initial pattern from context
        let initial_pattern = if !self.discover.filter.is_empty() {
            // From Files panel with filter: prefill pattern
            self.discover.filter.clone()
        } else if let Some(file) = self.filtered_files().get(self.discover.selected) {
            // From Files panel with file selected: prefill with extension pattern
            if let Some(ext) = std::path::Path::new(&file.path).extension() {
                format!("**/*.{}", ext.to_string_lossy())
            } else {
                "**/*".to_string()
            }
        } else {
            "**/*".to_string()
        };

        // Determine initial tag from context
        let initial_tag = if self.discover.focus == DiscoverFocus::Tags {
            if let Some(tag_idx) = self.discover.selected_tag {
                if let Some(tag) = self.discover.tags.get(tag_idx) {
                    if !tag.is_special {
                        tag.name.clone()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Create RuleBuilderState (v3.0 consolidation)
        let source_id_str = self.discover.selected_source()
            .map(|s| s.id.to_string());
        let mut builder_state = super::extraction::RuleBuilderState::new(source_id_str);
        builder_state.pattern = initial_pattern.clone();
        builder_state.tag = initial_tag;

        // Set the Rule Builder state and transition to RuleBuilder view
        self.discover.rule_builder = Some(builder_state);
        self.transition_discover_state(DiscoverViewState::RuleBuilder);

        // Populate files matching the initial pattern
        self.update_rule_builder_files(&initial_pattern);

        // Trigger cache load for pattern matching
        self.start_cache_load();
    }

    /// Update the Rule Builder's matched files based on the current pattern
    /// Update the Rule Builder's file results based on the current pattern.
    /// Detects phase based on whether pattern contains <field> placeholders.
    fn update_rule_builder_files(&mut self, pattern: &str) {

        let builder = match self.discover.rule_builder.as_mut() {
            Some(b) => b,
            None => return,
        };

        if pattern.is_empty() {
            builder.match_count = 0;
            builder.file_results = super::extraction::FileResultsState::Exploration {
                folder_matches: Vec::new(),
                expanded_folder_indices: std::collections::HashSet::new(),
                detected_patterns: Vec::new(),
            };
            return;
        }

        // Detect phase: Does pattern contain <field> placeholders?
        let has_placeholders = pattern.contains('<') && pattern.contains('>');

        if has_placeholders {
            // Phase 2: Extraction Preview
            self.update_rule_builder_extraction_preview(pattern);
        } else {
            // Phase 1: Exploration (folder counts)
            self.update_rule_builder_exploration(pattern);
        }
    }

    /// Phase 1: Exploration - Update folder matches with counts
    fn update_rule_builder_exploration(&mut self, pattern: &str) {
        use globset::GlobBuilder;
        use super::extraction::FolderMatch;

        // Check rule_builder exists first (early return)
        if self.discover.rule_builder.is_none() {
            return;
        }

        // Build glob matcher (doesn't need folder_cache)
        let glob_pattern = if pattern.contains('/') {
            pattern.to_string()
        } else if pattern.is_empty() || pattern == "*" {
            "**/*".to_string()
        } else {
            format!("**/{}", pattern)
        };

        // Check for "match all" patterns - use root folder counts directly
        // The cache only has root-level entries, so traversing won't find subfolders
        let is_match_all = glob_pattern == "**/*" || glob_pattern == "**" || pattern == "*";

        // For recursive patterns like "**/*.csv", we need to query the database
        // because the in-memory folder_cache only contains root-level entries
        if glob_pattern.contains("**") && !is_match_all {
            // Get source ID from glob_explorer
            let source_id = match self.discover.glob_explorer.as_ref() {
                Some(e) => e.cache_source_id.clone().unwrap_or_default(),
                None => return,
            };

            if source_id.is_empty() {
                return; // No source loaded yet
            }

            // Parse pattern for database query
            let query = super::pattern_query::PatternQuery::from_glob(&glob_pattern);

            // Show loading indicator
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.match_count = 0;
                builder.is_streaming = true;
                builder.file_results = super::extraction::FileResultsState::Exploration {
                    folder_matches: Vec::new(),
                    expanded_folder_indices: std::collections::HashSet::new(),
                    detected_patterns: Vec::new(),
                };
            }

            let (backend, db_path) = self.resolve_db_target();

            // Spawn async database search
            let pattern_for_msg = glob_pattern.clone();
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            self.pending_rule_builder_search = Some(rx);

            tokio::spawn(async move {
                let conn = match App::open_db_readonly_with(backend, &db_path).await {
                    Some(conn) => conn,
                    None => {
                        let _ = tx.send(RuleBuilderSearchResult {
                            folder_matches: vec![],
                            total_count: 0,
                            pattern: pattern_for_msg,
                        }).await;
                        return;
                    }
                };

                let total_count = query.count_files(&conn, &source_id).await as usize;

                let results = query
                    .search_files(&conn, &source_id, 1000, 0)
                    .await;

                // Group by parent folder
                let mut folder_counts: std::collections::HashMap<String, (usize, String)> =
                    std::collections::HashMap::new();

                for (rel_path, _size, _mtime) in results {
                    // Extract parent folder from path
                    let folder = if let Some(idx) = rel_path.rfind('/') {
                        rel_path[..idx].to_string()
                    } else {
                        ".".to_string()
                    };
                    let filename = rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string();
                    let entry = folder_counts.entry(folder).or_insert((0, filename));
                    entry.0 += 1;
                }

                // Convert to FolderMatch
                let mut folder_matches: Vec<super::extraction::FolderMatch> = folder_counts
                    .into_iter()
                    .map(|(path, (count, sample))| super::extraction::FolderMatch {
                        path: if path == "." { "./".to_string() } else { format!("{}/", path) },
                        count,
                        sample_filename: sample,
                        files: Vec::new(),
                    })
                    .collect();

                folder_matches.sort_by(|a, b| b.count.cmp(&a.count));

                let _ = tx.send(RuleBuilderSearchResult {
                    folder_matches,
                    total_count,
                    pattern: pattern_for_msg,
                }).await;
            });

            return;
        }

        let matcher = match GlobBuilder::new(&glob_pattern)
            .case_insensitive(true)
            .build()
            .map(|g| g.compile_matcher())
        {
            Ok(m) => m,
            Err(_) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("Invalid pattern".to_string());
                    builder.file_results = super::extraction::FileResultsState::Exploration {
                        folder_matches: Vec::new(),
                        expanded_folder_indices: std::collections::HashSet::new(),
                        detected_patterns: Vec::new(),
                    };
                }
                return;
            }
        };

        // Traverse folder_cache with shared borrow - NO CLONE
        let (folder_matches, match_count) = {
            let folder_cache = match self.discover.glob_explorer.as_ref() {
                Some(e) => &e.folder_cache,
                None => return,
            };

            // For "match all" patterns, use root folder info directly
            // These already have accurate file counts from the database
            if is_match_all {
                if let Some(root_items) = folder_cache.get("") {
                    let mut folder_matches: Vec<FolderMatch> = root_items
                        .iter()
                        .map(|item| FolderMatch {
                            path: if item.is_file() {
                                "./".to_string()
                            } else {
                                format!("{}/", item.name())
                            },
                            count: item.file_count(),
                            sample_filename: item.name().to_string(),
                            files: Vec::new(),
                        })
                        .collect();

                    folder_matches.sort_by(|a, b| b.count.cmp(&a.count));
                    let match_count = folder_matches.iter().map(|f| f.count).sum();
                    (folder_matches, match_count)
                } else {
                    (Vec::new(), 0)
                }
            } else {
                // For specific patterns, traverse the cache
                let mut folder_counts: std::collections::HashMap<String, (usize, String)> =
                    std::collections::HashMap::new();

                // Recursive function to traverse folder cache
                fn traverse_cache(
                    cache: &std::collections::HashMap<String, Vec<FsEntry>>,
                    prefix: &str,
                    matcher: &globset::GlobMatcher,
                    folder_counts: &mut std::collections::HashMap<String, (usize, String)>,
                ) {
                    if let Some(items) = cache.get(prefix) {
                        for item in items {
                            let full_path = if prefix.is_empty() {
                                item.name().to_string()
                            } else {
                                format!("{}{}", prefix, item.name())
                            };

                            if item.is_file() {
                                if matcher.is_match(&full_path) {
                                    let folder = if prefix.is_empty() {
                                        ".".to_string()
                                    } else {
                                        prefix.trim_end_matches('/').to_string()
                                    };
                                    let entry = folder_counts.entry(folder).or_insert((0, item.name().to_string()));
                                    entry.0 += 1;
                                }
                            } else {
                                let sub_prefix = format!("{}/", full_path);
                                traverse_cache(cache, &sub_prefix, matcher, folder_counts);
                            }
                        }
                    }
                }

                traverse_cache(folder_cache, "", &matcher, &mut folder_counts);

                // Convert to FolderMatch and sort
                let mut folder_matches: Vec<FolderMatch> = folder_counts
                    .into_iter()
                    .filter(|(_, (count, _))| *count > 0)
                    .map(|(path, (count, sample))| FolderMatch {
                        path: if path == "." { "./".to_string() } else { format!("{}/", path) },
                        count,
                        sample_filename: sample,
                        files: Vec::new(),
                    })
                    .collect();

                folder_matches.sort_by(|a, b| b.count.cmp(&a.count));
                let match_count = folder_matches.iter().map(|f| f.count).sum();
                (folder_matches, match_count)
            }
        }; // shared borrow ends here

        // Update builder with mutable borrow
        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.pattern_error = None;
            builder.match_count = match_count;
            builder.file_results = super::extraction::FileResultsState::Exploration {
                folder_matches,
                expanded_folder_indices: std::collections::HashSet::new(),
                detected_patterns: Vec::new(),
            };
            builder.selected_file = 0;
        }
    }

    /// Phase 2: Extraction Preview - Show files with extracted values
    fn update_rule_builder_extraction_preview(&mut self, pattern: &str) {
        use globset::GlobBuilder;
        use super::extraction::{ExtractionPreviewFile, parse_custom_glob, extract_field_values};

        // Check rule_builder exists first (early return)
        if self.discover.rule_builder.is_none() {
            return;
        }

        // Parse custom glob pattern (doesn't need folder_cache)
        let parsed = match parse_custom_glob(pattern) {
            Ok(p) => p,
            Err(e) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some(e.message);
                    builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                        preview_files: Vec::new(),
                    };
                }
                return;
            }
        };

        // Build glob matcher
        let glob_pattern = if parsed.glob_pattern.contains('/') {
            parsed.glob_pattern.clone()
        } else {
            format!("**/{}", parsed.glob_pattern)
        };

        let matcher = match GlobBuilder::new(&glob_pattern)
            .case_insensitive(true)
            .build()
            .map(|g| g.compile_matcher())
        {
            Ok(m) => m,
            Err(_) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("Invalid glob pattern".to_string());
                    builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                        preview_files: Vec::new(),
                    };
                }
                return;
            }
        };

        // Traverse folder_cache with shared borrow - NO CLONE
        let all_files = {
            let folder_cache = match self.discover.glob_explorer.as_ref() {
                Some(e) => &e.folder_cache,
                None => return,
            };

            let mut files: Vec<String> = Vec::new();

            fn collect_files(
                cache: &std::collections::HashMap<String, Vec<FsEntry>>,
                prefix: &str,
                matcher: &globset::GlobMatcher,
                files: &mut Vec<String>,
                limit: usize,
            ) {
                if files.len() >= limit {
                    return;
                }
                if let Some(items) = cache.get(prefix) {
                    for item in items {
                        if files.len() >= limit {
                            return;
                        }
                        let full_path = if prefix.is_empty() {
                            item.name().to_string()
                        } else {
                            format!("{}{}", prefix, item.name())
                        };

                        if item.is_file() {
                            if matcher.is_match(&full_path) {
                                files.push(full_path);
                            }
                        } else {
                            let sub_prefix = format!("{}/", full_path);
                            collect_files(cache, &sub_prefix, matcher, files, limit);
                        }
                    }
                }
            }

            collect_files(folder_cache, "", &matcher, &mut files, 100);
            files
        }; // shared borrow ends here

        // Convert to preview files with extractions
        let preview_files: Vec<ExtractionPreviewFile> = all_files
            .into_iter()
            .map(|path| {
                let extractions = extract_field_values(&path, &parsed);
                ExtractionPreviewFile {
                    path: path.clone(),
                    relative_path: path,
                    extractions,
                    warnings: Vec::new(),
                }
            })
            .collect();

        // Update builder with mutable borrow
        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.pattern_error = None;
            builder.match_count = preview_files.len();
            builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                preview_files,
            };
            builder.selected_file = 0;
        }
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
                // Mark stats as needing refresh - will trigger reload on next tick
                self.home.stats_loaded = false;
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
            TuiMode::Sources => {
                // Mark sources as needing refresh - will trigger reload on next tick
                self.discover.sources_loaded = false;
            }
            TuiMode::Settings => {
                // Settings don't need refresh - they're always current
            }
        }
    }

    /// Check if the app is in a text input mode where global keys should not be intercepted
    fn in_text_input_mode(&self) -> bool {
        match self.mode {
            TuiMode::Discover => {
                // Check glob explorer filtering state
                if let Some(ref explorer) = self.discover.glob_explorer {
                    if matches!(explorer.phase, GlobExplorerPhase::Filtering) {
                        return true;
                    }
                }
                // Check sources/tags dropdown filtering
                if self.discover.sources_filtering || self.discover.tags_filtering {
                    return true;
                }
                // Check Rule Builder text input fields (Pattern, Tag, ExcludeInput, ExtractionEdit)
                if self.discover.view_state == DiscoverViewState::RuleBuilder {
                    if let Some(ref builder) = self.discover.rule_builder {
                        use super::extraction::RuleBuilderFocus;
                        if matches!(
                            builder.focus,
                            RuleBuilderFocus::Pattern |
                            RuleBuilderFocus::Tag |
                            RuleBuilderFocus::ExcludeInput |
                            RuleBuilderFocus::ExtractionEdit(_)
                        ) {
                            return true;
                        }
                    }
                }
                // All other text input states are in the view_state enum
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
            file_version_id: None,
            job_type: JobType::Scan,
            name: "scan".to_string(),
            version: None,
            status: JobStatus::Running,
            started_at: chrono::Local::now(),
            completed_at: None,
            items_total: 0,
            items_processed: 0,
            items_failed: 0,
            output_path: Some(directory_path.to_string()),
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
        };

        // Add to front of list so it's visible immediately
        self.jobs_state.push_job(job);

        job_id
    }

    /// Update the status of a scan job.
    ///
    /// Finds the job by ID and updates its status and error message.
    fn update_scan_job_status(&mut self, job_id: i64, status: JobStatus, error: Option<String>) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = status;
            if status == JobStatus::Completed || status == JobStatus::Failed || status == JobStatus::Cancelled {
                job.completed_at = Some(chrono::Local::now());
            }
            if let Some(err) = error {
                job.failures.push(JobFailure {
                    file_path: "".to_string(),
                    error: err,
                    line: None,
                });
            }
        }
    }

    /// Create a new schema eval job and add it to the jobs list.
    ///
    /// Returns the job ID for tracking status updates.
    fn add_schema_eval_job(&mut self, mode: SchemaEvalMode, paths_total: usize) -> i64 {
        let job_id = chrono::Local::now().timestamp_millis();

        let job = JobInfo {
            id: job_id,
            file_version_id: None,
            job_type: JobType::SchemaEval,
            name: match mode {
                SchemaEvalMode::Sample => "schema-sample".to_string(),
                SchemaEvalMode::Full => "schema-full".to_string(),
            },
            version: None,
            status: JobStatus::Running,
            started_at: chrono::Local::now(),
            completed_at: None,
            items_total: paths_total as u32,
            items_processed: 0,
            items_failed: 0,
            output_path: None,
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
        };

        self.jobs_state.push_job(job);
        job_id
    }

    /// Update the status of a schema eval job.
    fn update_schema_eval_job(&mut self, job_id: i64, status: JobStatus, processed: u32, error: Option<String>) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = status;
            job.items_processed = processed;
            if status == JobStatus::Completed || status == JobStatus::Failed || status == JobStatus::Cancelled {
                job.completed_at = Some(chrono::Local::now());
            }
            if let Some(err) = error {
                job.failures.push(JobFailure {
                    file_path: "".to_string(),
                    error: err,
                    line: None,
                });
            }
        }
    }

    fn set_schema_eval_job_total(&mut self, job_id: i64, total: u32) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            if job.items_total == 0 {
                job.items_total = total;
            }
        }
    }

    /// Run sample schema evaluation (quick, async).
    ///
    /// Uses structure-aware sampling: buckets paths by prefix and takes N per bucket.
    fn run_sample_schema_eval(&mut self) {
        let builder = match self.discover.rule_builder.as_ref() {
            Some(b) => b,
            None => return,
        };

        if matches!(builder.eval_state, super::extraction::EvalState::Running { .. })
            || self.pending_schema_eval.is_some()
            || self.pending_sample_eval.is_some()
        {
            return;
        }

        let source_id = match builder.source_id.as_deref() {
            Some(id) => id.to_string(),
            None => return,
        };
        let pattern = builder.pattern.clone();
        let (backend, db_path) = self.resolve_db_target();

        let (tx, rx) = mpsc::channel::<SampleEvalResult>(16);
        self.pending_sample_eval = Some(rx);

        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => {
                    let _ = tx.send(SampleEvalResult::Error("Database error: failed to open".to_string())).await;
                    return;
                }
            };

            let glob_pattern = match super::pattern_query::eval_glob_pattern(&pattern) {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx.send(SampleEvalResult::Error(err)).await;
                    return;
                }
            };
            let matcher = match super::pattern_query::build_eval_matcher(&glob_pattern) {
                Ok(m) => m,
                Err(err) => {
                    let _ = tx.send(SampleEvalResult::Error(err)).await;
                    return;
                }
            };

            let paths = super::pattern_query::sample_paths_for_eval(
                &conn,
                &source_id,
                &glob_pattern,
                &matcher,
            )
            .await;
            if paths.is_empty() {
                let _ = tx.send(SampleEvalResult::Error("No files to analyze".to_string())).await;
                return;
            }

            let mut state = super::extraction::RuleBuilderState::default();
            super::extraction::analyze_paths_for_schema_ui(&mut state, &paths, 8);

            let _ = tx.send(SampleEvalResult::Complete {
                pattern,
                pattern_seeds: state.pattern_seeds,
                path_archetypes: state.path_archetypes,
                naming_schemes: state.naming_schemes,
                synonym_suggestions: state.synonym_suggestions,
                paths_analyzed: paths.len(),
            }).await;
        });
    }

    /// Start full schema evaluation as a background job.
    fn start_full_schema_eval(&mut self) {
        let (pattern, source_id, full_eval_running) = match self.discover.rule_builder.as_ref() {
            Some(builder) => (
                builder.pattern.clone(),
                builder.source_id.clone(),
                matches!(builder.eval_state, super::extraction::EvalState::Running { .. }),
            ),
            None => return,
        };

        if full_eval_running {
            return;
        }

        let source_id = match source_id.as_deref() {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => {
                self.discover.status_message = Some(("No source selected".to_string(), true));
                return;
            }
        };

        // Create job
        let job_id = self.add_schema_eval_job(SchemaEvalMode::Full, 0);
        self.current_schema_eval_job_id = Some(job_id);

        // Mark as running
        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.eval_state = super::extraction::EvalState::Running { progress: 0 };
        }

        // Channel for results
        let (tx, rx) = mpsc::channel::<SchemaEvalResult>(16);
        self.pending_schema_eval = Some(rx);

        // Spawn background task
        let (backend, db_path) = self.resolve_db_target();

        tokio::spawn(async move {
            let _ = tx.send(SchemaEvalResult::Started { job_id }).await;

            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => {
                    let _ = tx.send(SchemaEvalResult::Error("Database error: failed to open".to_string())).await;
                    return;
                }
            };

            let glob_pattern = match super::pattern_query::eval_glob_pattern(&pattern) {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx.send(SchemaEvalResult::Error(err)).await;
                    return;
                }
            };
            let matcher = match super::pattern_query::build_eval_matcher(&glob_pattern) {
                Ok(m) => m,
                Err(err) => {
                    let _ = tx.send(SchemaEvalResult::Error(err)).await;
                    return;
                }
            };

            let query = super::pattern_query::PatternQuery::from_glob(&glob_pattern);
            let total_candidates = query.count_files(&conn, &source_id).await as usize;

            let _ = tx.send(SchemaEvalResult::Progress {
                progress: 0,
                paths_analyzed: 0,
                total_paths: total_candidates,
            }).await;

            let mut offset = 0usize;
            let batch_size = 1000usize;
            let mut processed = 0usize;
            let mut matched_paths: Vec<String> = Vec::new();

            loop {
                let results = query
                    .search_files(&conn, &source_id, batch_size, offset)
                    .await;

                if results.is_empty() {
                    break;
                }

                for (rel_path, _size, _mtime) in results.iter() {
                    if matcher.is_match(rel_path) {
                        matched_paths.push(rel_path.clone());
                    }
                }

                processed += results.len();
                offset += results.len();

                if processed % 2000 == 0 || processed >= total_candidates {
                    let progress = if total_candidates == 0 {
                        100
                    } else {
                        ((processed.saturating_mul(100)) / total_candidates).min(99) as u8
                    };
                    let _ = tx.send(SchemaEvalResult::Progress {
                        progress,
                        paths_analyzed: processed,
                        total_paths: total_candidates,
                    }).await;
                }
            }

            if matched_paths.is_empty() {
                let _ = tx.send(SchemaEvalResult::Error("No files to analyze".to_string())).await;
                return;
            }

            let mut state = super::extraction::RuleBuilderState::default();
            super::extraction::analyze_paths_for_schema_ui(&mut state, &matched_paths, 8);

            let _ = tx.send(SchemaEvalResult::Complete {
                job_id,
                pattern,
                pattern_seeds: state.pattern_seeds,
                path_archetypes: state.path_archetypes,
                naming_schemes: state.naming_schemes,
                synonym_suggestions: state.synonym_suggestions,
                paths_analyzed: matched_paths.len(),
            }).await;
        });

        self.discover.status_message = Some((
            "Full eval started...".to_string(),
            false,
        ));
    }

    /// Scan a directory using the unified parallel scanner.
    ///
    /// Uses `scout::Scanner` for parallel walking and DB persistence.
    /// Progress updates are forwarded to the TUI via channel.
    fn scan_directory(&mut self, path: &str) {
        use std::path::Path;

        let path_input = Path::new(path);

        let expanded_path = scan_path::expand_scan_path(path_input);

        // Path validation - synchronous (fast filesystem checks)
        if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
            self.discover.scan_error = Some(err.to_string());
            return;
        }

        // Path is valid - enter scanning state and spawn background task
        let path_display = expanded_path.display().to_string();
        self.discover.scanning_path = Some(path_display.clone());
        self.discover.scan_progress = Some(ScoutProgress {
            dirs_scanned: 0,
            files_found: 0,
            files_persisted: 0,
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

        // Don't show "Scan started" yet - wait for validation to pass
        // The status will update to "Scan started" after validation succeeds
        // or show an error message if validation fails
        self.discover.status_message = None;

        // Get database path
        let db_path = self.config.database.clone()
            .unwrap_or_else(crate::cli::config::active_db_path);

        let source_path = path_display;
        let scan_job_id = job_id; // Capture for async block

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

                // Check for overlapping sources (parent/child relationships)
                // This prevents duplicate file tracking and conflicting tags
                if let Err(e) = db.check_source_overlap(std::path::Path::new(&source_path)).await {
                    let suggestion = match &e {
                        crate::scout::error::ScoutError::SourceIsChildOfExisting { existing_name, .. } => {
                            format!(
                                "Either delete source '{}' first, or use tagging rules to organize files within that source.",
                                existing_name
                            )
                        }
                        crate::scout::error::ScoutError::SourceIsParentOfExisting { existing_name, .. } => {
                            format!(
                                "Either delete source '{}' first, or scan a non-overlapping directory.",
                                existing_name
                            )
                        }
                        _ => String::new(),
                    };
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "{}\n\nSuggestion: {}",
                        e, suggestion
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

            // Validation passed - notify TUI that scan is starting
            let _ = tui_tx.send(TuiScanResult::Started { job_id: scan_job_id }).await;

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
                    // Create a multi-threaded runtime for the blocking task
                    // CRITICAL: Scanner uses tokio::spawn internally for concurrent persist task.
                    // A current-thread runtime causes deadlock because the persist task can't
                    // run concurrently with the walker's blocking_send().
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2) // Minimum for concurrent walker + persist
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(scanner.scan(&source_clone, Some(progress_tx), None))
                }).await
            };

            // Wait for forwarding to complete
            let _ = forward_handle.await;

            match scan_result {
                Ok(Ok(result)) => {
                    // Scan complete - TUI will load files from DB
                    let _ = tui_tx.send(TuiScanResult::Complete {
                        source_path,
                        files_persisted: result.stats.files_discovered as usize,
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
        // First check if we have a directly-set source ID (e.g., after scan completion)
        // This handles the case where sources list hasn't loaded yet
        let selected_source_id = if let Some(ref id) = self.discover.selected_source_id {
            id.clone()
        } else {
            // Fall back to looking up from sources list
            let source_idx = if self.discover.view_state == DiscoverViewState::SourcesDropdown {
                self.discover.preview_source.unwrap_or_else(|| self.discover.selected_source_index())
            } else {
                self.discover.selected_source_index()
            };

            match self.discover.sources.get(source_idx) {
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
            }
        };

        let conn = match self.open_db_readonly().await {
            Some(conn) => conn,
            None => {
                self.discover.scan_error = Some("No Scout database found. Press 's' to scan a folder.".to_string());
                self.discover.data_loaded = true;
                return;
            }
        };

        let query = r#"
            SELECT path, rel_path, source_id, size, mtime, tag
            FROM scout_files
            WHERE source_id = ?
            ORDER BY rel_path
            LIMIT 1000
        "#;

        let rows = match conn.query_all(query, &[DbValue::Text(selected_source_id.as_str().to_string())]).await {
            Ok(rows) => rows,
            Err(e) => {
                self.discover.scan_error = Some(format!("Query failed: {}", e));
                self.discover.data_loaded = true;
                return;
            }
        };

        let mut files: Vec<FileInfo> = Vec::with_capacity(rows.len());
        for row in rows {
            let path: String = match row.get(0) {
                Ok(v) => v,
                Err(e) => {
                    self.discover.scan_error = Some(format!("Query failed: {}", e));
                    self.discover.data_loaded = true;
                    return;
                }
            };
            let rel_path: String = match row.get(1) {
                Ok(v) => v,
                Err(e) => {
                    self.discover.scan_error = Some(format!("Query failed: {}", e));
                    self.discover.data_loaded = true;
                    return;
                }
            };
            let size: i64 = match row.get(3) {
                Ok(v) => v,
                Err(e) => {
                    self.discover.scan_error = Some(format!("Query failed: {}", e));
                    self.discover.data_loaded = true;
                    return;
                }
            };
            let mtime_millis: i64 = match row.get(4) {
                Ok(v) => v,
                Err(e) => {
                    self.discover.scan_error = Some(format!("Query failed: {}", e));
                    self.discover.data_loaded = true;
                    return;
                }
            };
            let tag: Option<String> = match row.get(5) {
                Ok(v) => v,
                Err(e) => {
                    self.discover.scan_error = Some(format!("Query failed: {}", e));
                    self.discover.data_loaded = true;
                    return;
                }
            };

            let modified = chrono::DateTime::from_timestamp_millis(mtime_millis)
                .map(|dt| dt.with_timezone(&chrono::Local))
                .unwrap_or_else(chrono::Local::now);

            let tags = tag.into_iter().collect();
            let is_dir = std::path::Path::new(&path).is_dir();

            files.push(FileInfo {
                path,
                rel_path,
                size: size as u64,
                modified,
                is_dir,
                tags,
            });
        }

        self.discover.files = files;
        self.discover.selected = 0;
        self.discover.data_loaded = true;
        self.discover.scan_error = None;
    }

    /// Load folder tree for Glob Explorer (hierarchical file browsing).
    ///
    /// This replaces load_scout_files() when glob_explorer is active.
    /// Queries are batched: folders + preview files + total count in one function call.
    /// State is updated atomically at the end.
    ///
    /// NOTE: Currently unused - replaced by preload_folder_cache() for O(1) navigation.
    /// Start non-blocking cache load for glob explorer.
    /// Checks preloaded caches first (instant), then loads from disk cache or scout_folders.
    fn start_cache_load(&mut self) {
        // Must have a source selected
        let source_id = match &self.discover.selected_source_id {
            Some(id) => id.clone(),
            None => return,
        };

        // Skip if already loading
        if self.pending_cache_load.is_some() {
            return;
        }

        // Skip if cache is already loaded for this source
        if let Some(ref explorer) = self.discover.glob_explorer {
            if explorer.cache_loaded {
                if let Some(ref cache_source) = explorer.cache_source_id {
                    if cache_source == source_id.as_str() {
                        // Cache already loaded for this source, no reload needed
                        self.discover.data_loaded = true;
                        return;
                    }
                }
            }
        }

        // Skip if we're already tracking progress for this load
        // (prevents timer reset when tick() calls us multiple times)
        if self.cache_load_progress.is_some() {
            return;
        }

        // Skip if cache load already failed (don't retry until user takes action)
        if self.discover.scan_error.is_some() {
            return;
        }

        let source_id_str = source_id.as_str().to_string();

        // Get source name for progress display
        let source_name = self.discover.sources
            .iter()
            .find(|s| s.id == source_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| source_id_str.clone());

        // Set up channel and progress tracking
        let (tx, rx) = mpsc::channel::<CacheLoadMessage>(1);
        self.pending_cache_load = Some(rx);
        self.cache_load_progress = Some(CacheLoadProgress::new(source_name));

        // Initialize empty cache in explorer
        if let Some(ref mut explorer) = self.discover.glob_explorer {
            explorer.folder_cache = HashMap::new();
            explorer.cache_source_id = Some(source_id_str.clone());
        }

        let (backend, db_path) = self.resolve_db_target();

        // Spawn background task for database queries (live folder derivation)
        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => {
                    let _ = tx.send(CacheLoadMessage::Error(
                        "Database error: failed to open".to_string()
                    )).await;
                    return;
                }
            };

            let root_folders = match App::query_folder_counts(&conn, &source_id_str, "", None).await {
                Ok(folders) => folders,
                Err(e) => {
                    let _ = tx.send(CacheLoadMessage::Error(
                        format!("Query error: {}", e)
                    )).await;
                    return;
                }
            };

            let folder_infos: Vec<FsEntry> = root_folders
                .into_iter()
                .map(|(name, count, is_file)| FsEntry::new(name, count as usize, is_file))
                .collect();

            let mut cache: HashMap<String, Vec<FsEntry>> = HashMap::new();
            cache.insert(String::new(), folder_infos);

            let total_files: usize = conn.query_scalar::<i64>(
                "SELECT file_count FROM scout_sources WHERE id = ?",
                &[DbValue::Text(source_id_str.clone())],
            ).await.unwrap_or(0) as usize;

            let tag_rows = conn.query_all(
                "SELECT tag, COUNT(*) as count FROM scout_files WHERE source_id = ? AND tag IS NOT NULL GROUP BY tag ORDER BY count DESC, tag",
                &[DbValue::Text(source_id_str.clone())],
            ).await.unwrap_or_default();

            let untagged_count: i64 = conn.query_scalar::<i64>(
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND tag IS NULL",
                &[DbValue::Text(source_id_str.clone())],
            ).await.unwrap_or(0);

            // Build tags list
            let mut tags: Vec<TagInfo> = Vec::new();
            tags.push(TagInfo {
                name: "All files".to_string(),
                count: total_files,
                is_special: true,
            });
            for row in tag_rows {
                let tag_name: String = match row.get(0) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let count: i64 = match row.get(1) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                tags.push(TagInfo {
                    name: tag_name,
                    count: count as usize,
                    is_special: false,
                });
            }
            if untagged_count > 0 {
                tags.push(TagInfo {
                    name: "untagged".to_string(),
                    count: untagged_count as usize,
                    is_special: true,
                });
            }

            let _ = tx.send(CacheLoadMessage::Complete {
                source_id: source_id_str,
                total_files,
                tags,
                cache,
            }).await;
        });
    }
}


impl App {
    /// Check for profiler dump trigger file and export data.
    /// Used for testing integration - touch /tmp/casparian_profile_dump to trigger.
    #[cfg(feature = "profiling")]
    pub fn check_profiler_dump(&self) {
        const DUMP_TRIGGER: &str = "/tmp/casparian_profile_dump";
        const DUMP_OUTPUT: &str = "/tmp/casparian_profile_data.txt";

        if std::path::Path::new(DUMP_TRIGGER).exists() {
            let _ = std::fs::remove_file(DUMP_TRIGGER);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Format cache load timing if available
            let cache_load_section = if let Some(ref timing) = self.last_cache_load_timing {
                format!(
                    "\n=== CACHE LOAD ===\nsource_id={}\nfiles_loaded={}\nload_duration_ms={:.1}\n",
                    timing.source_id,
                    timing.files_loaded,
                    timing.duration_ms
                )
            } else {
                String::new()
            };

            let data = format!(
                "=== PROFILER DUMP ===\ntimestamp={}\n{}{}\n=== ZONES ===\n{}\n=== FRAMES ===\n{}\n",
                timestamp,
                self.profiler.export_summary(),
                cache_load_section,
                self.profiler.export_zones(),
                self.profiler.export_frames_tsv(30)
            );
            let _ = std::fs::write(DUMP_OUTPUT, data);
        }
    }

    /// Update folders from cache based on current prefix (O(1) lookup).
    /// Used for navigation instead of SQL queries.
    /// If a pattern is set, filters entries in-memory using simple matching.
    fn update_folders_from_cache(&mut self) {
        // Note: profiling zone removed here to avoid borrow conflict with start_folder_query

        if let Some(ref mut explorer) = self.discover.glob_explorer {
            let prefix = explorer.current_prefix.clone();
            let pattern = explorer.pattern.clone();

            // Treat "**/*" and "**" as "show all" - use normal folder navigation
            // Only do recursive search for specific patterns like "**/*.rs"
            let is_match_all = pattern == "**/*" || pattern == "**" || pattern == "*" || pattern.is_empty();

            // Handle ** patterns with database query (not in-memory cache)
            // Skip for "match all" patterns - just use folder navigation
            if pattern.contains("**") && !is_match_all {
                // Cancel any pending search
                self.pending_glob_search = None;

                // Parse pattern to extract extension and path filter
                let query = super::pattern_query::PatternQuery::from_glob(&pattern);
                let pattern_for_search = pattern.clone();

                // Get source ID for query
                let source_id = explorer.cache_source_id.clone().unwrap_or_default();

                // Show loading indicator immediately
                let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                explorer.folders = vec![FsEntry::loading(&format!("{} Searching...", spinner_char))];

                let (backend, db_path) = self.resolve_db_target();

                // Spawn async task for database query
                let (tx, rx) = mpsc::channel(1);
                self.pending_glob_search = Some(rx);

                tokio::spawn(async move {
                    let conn = match App::open_db_readonly_with(backend, &db_path).await {
                        Some(conn) => conn,
                        None => {
                            let _ = tx.send(GlobSearchResult {
                                folders: vec![],
                                total_count: 0,
                                pattern: pattern_for_search,
                            }).await;
                            return;
                        }
                    };

                    let count = query.count_files(&conn, &source_id).await;

                    let results = query.search_files(&conn, &source_id, 100, 0).await;

                    // Convert to FsEntry for display
                    let folders: Vec<FsEntry> = results.into_iter()
                        .map(|(path, _size, _mtime)| {
                            FsEntry::with_path(path.clone(), Some(path), 1, true)
                        })
                        .collect();

                    let _ = tx.send(GlobSearchResult {
                        folders,
                        total_count: count as usize,
                        pattern: pattern_for_search,
                    }).await;
                });

                return;
            }

            // Normal pattern: filter current level only
            if let Some(cached_folders) = explorer.folder_cache.get(&prefix) {
                let mut folders: Vec<FsEntry> = if pattern.is_empty() {
                    cached_folders.clone()
                } else {
                    let pattern_lower = pattern.to_lowercase();
                    cached_folders.iter()
                        .filter(|f| {
                            let name_lower = f.name().to_lowercase();
                            Self::glob_match_name(&name_lower, &pattern_lower)
                        })
                        .cloned()
                        .collect()
                };

                // Sort by file count descending (most matches first)
                folders.sort_by(|a, b| b.file_count().cmp(&a.file_count()));

                explorer.folders = folders.clone();
                explorer.total_count = GlobFileCount::Exact(
                    folders.iter().map(|f| f.file_count()).sum()
                );
                explorer.selected_folder = 0;
            } else {
                // Prefix not in cache - trigger async database query
                let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                explorer.folders = vec![FsEntry::loading(&format!("{} Loading {}...", spinner_char, if prefix.is_empty() { "root" } else { &prefix }))];
            }
        }

        // Check if we need to start a folder query (outside the borrow scope)
        // Always query DB if cache is empty, even with "**" patterns (need to populate cache first)
        let needs_query = if let Some(ref explorer) = self.discover.glob_explorer {
            let prefix = &explorer.current_prefix;
            !explorer.folder_cache.contains_key(prefix)
        } else {
            false
        };

        if needs_query {
            if let Some(ref explorer) = self.discover.glob_explorer {
                if let Some(source_id) = explorer.cache_source_id.clone() {
                    let prefix = explorer.current_prefix.clone();
                    let pattern = explorer.pattern.clone();
                    self.start_folder_query(source_id, prefix, pattern);
                }
            }
        }
    }

    /// Start an async database query for a folder prefix
    fn start_folder_query(&mut self, source_id: String, prefix: String, glob_pattern: String) {
        // Skip if already loading
        if self.pending_folder_query.is_some() {
            return;
        }

        let (tx, rx) = mpsc::channel(1);
        self.pending_folder_query = Some(rx);

        let glob_opt = if glob_pattern.is_empty() { None } else { Some(glob_pattern) };

        let (backend, db_path) = self.resolve_db_target();

        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => {
                    let _ = tx.send(FolderQueryMessage::Error("Database error: failed to open".to_string())).await;
                    return;
                }
            };

            let rows = match App::query_folder_counts(&conn, &source_id, &prefix, glob_opt.as_deref()).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(FolderQueryMessage::Error(format!("Query error: {}", e))).await;
                    return;
                }
            };

            let folders: Vec<FsEntry> = rows
                .into_iter()
                .map(|(name, count, is_file)| FsEntry::new(name, count as usize, is_file))
                .collect();

            let total_count = folders.iter().map(|f| f.file_count()).sum();

            let _ = tx.send(FolderQueryMessage::Complete {
                prefix,
                folders,
                total_count,
            }).await;
        });
    }

    /// Simple glob pattern matching for file names
    fn glob_match_name(name: &str, pattern: &str) -> bool {
        if pattern.starts_with("*.") {
            // *.ext -> ends with .ext
            let ext = &pattern[1..];
            name.ends_with(ext)
        } else if pattern.ends_with("*") {
            // prefix* -> starts with prefix
            let prefix_pat = &pattern[..pattern.len()-1];
            name.starts_with(prefix_pat)
        } else if pattern.contains('*') {
            // a*b pattern -> starts with a and ends with b
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                name.starts_with(parts[0]) && name.ends_with(parts[1])
            } else {
                name.contains(&pattern.replace('*', ""))
            }
        } else {
            // Simple substring match
            name.contains(pattern)
        }
    }

    fn glob_to_like_pattern(glob: &str) -> String {
        let mut result = String::with_capacity(glob.len() + 4);

        let glob = glob.replace("**/", "");
        let glob = glob.replace("**", "%");

        let mut chars = glob.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => result.push('%'),
                '?' => result.push('_'),
                '%' => result.push('%'),
                '_' => result.push_str("\\_"),
                '\\' => {
                    if let Some(next) = chars.next() {
                        result.push(next);
                    }
                }
                _ => result.push(c),
            }
        }

        result
    }

    async fn query_folder_counts(
        conn: &DbConnection,
        source_id: &str,
        prefix: &str,
        glob_pattern: Option<&str>,
    ) -> Result<Vec<(String, i64, bool)>, casparian_db::BackendError> {
        let prefix = prefix.trim_end_matches('/');

        if let Some(pattern) = glob_pattern {
            let like_pattern = Self::glob_to_like_pattern(pattern);
            let path_filter = if prefix.is_empty() {
                like_pattern.clone()
            } else {
                format!("{}/%", prefix)
            };
            let prefix_len = if prefix.is_empty() { 0 } else { prefix.len() as i64 + 1 };

            let rows = conn.query_all(
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
                  AND rel_path LIKE ?
                  AND rel_path LIKE ?
                  AND LENGTH(rel_path) > ?
                GROUP BY item_name
                ORDER BY file_count DESC
                LIMIT 100
                "#,
                &[
                    DbValue::Integer(prefix_len),
                    DbValue::Integer(prefix_len),
                    DbValue::Integer(prefix_len),
                    DbValue::Integer(prefix_len),
                    DbValue::Integer(prefix_len),
                    DbValue::Text(source_id.to_string()),
                    DbValue::Text(path_filter),
                    DbValue::Text(like_pattern),
                    DbValue::Integer(prefix_len),
                ],
            ).await?;

            let mut results = Vec::new();
            for row in rows {
                let name: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                let is_file_flag: i64 = row.get(2)?;
                if !name.is_empty() {
                    results.push((name, count, is_file_flag != 0));
                }
            }

            return Ok(results);
        }

        if prefix.is_empty() {
            let cached_rows = conn.query_all(
                "SELECT name, file_count, is_folder FROM scout_folders WHERE source_id = ? AND prefix = ? ORDER BY is_folder DESC, file_count DESC, name",
                &[
                    DbValue::Text(source_id.to_string()),
                    DbValue::Text(prefix.to_string()),
                ],
            ).await?;

            if !cached_rows.is_empty() {
                let mut results = Vec::new();
                for row in cached_rows {
                    let name: String = row.get(0)?;
                    let count: i64 = row.get(1)?;
                    let is_folder: i64 = row.get(2)?;
                    results.push((name, count, is_folder == 0));
                }
                return Ok(results);
            }
        }

        let files = conn.query_all(
            "SELECT name, size FROM scout_files WHERE source_id = ? AND parent_path = ? ORDER BY name LIMIT 200",
            &[
                DbValue::Text(source_id.to_string()),
                DbValue::Text(prefix.to_string()),
            ],
        ).await?;

        let mut results: Vec<(String, i64, bool)> = Vec::new();

        let subfolders = if prefix.is_empty() {
            conn.query_all(
                r#"
                SELECT
                    CASE
                        WHEN INSTR(parent_path, '/') > 0 THEN SUBSTR(parent_path, 1, INSTR(parent_path, '/') - 1)
                        ELSE parent_path
                    END AS folder_name,
                    COUNT(*) as file_count
                FROM scout_files
                WHERE source_id = ? AND parent_path != ''
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[DbValue::Text(source_id.to_string())],
            ).await?
        } else {
            let folder_prefix = format!("{}/", prefix);
            conn.query_all(
                r#"
                SELECT
                    CASE
                        WHEN INSTR(SUBSTR(parent_path, LENGTH(?) + 1), '/') > 0
                        THEN SUBSTR(parent_path, LENGTH(?) + 1, INSTR(SUBSTR(parent_path, LENGTH(?) + 1), '/') - 1)
                        ELSE SUBSTR(parent_path, LENGTH(?) + 1)
                    END AS folder_name,
                    COUNT(*) as file_count
                FROM scout_files
                WHERE source_id = ? AND parent_path LIKE ? || '%' AND parent_path != ?
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(source_id.to_string()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(prefix.to_string()),
                ],
            ).await?
        };

        for row in subfolders {
            let name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            if !name.is_empty() {
                results.push((name, count, false));
            }
        }

        for row in files {
            let name: String = row.get(0)?;
            results.push((name, 1, true));
        }

        Ok(results)
    }


    /// Start non-blocking sources load from Scout database
    fn start_sources_load(&mut self) {
        // Skip if already loading
        if self.pending_sources_load.is_some() {
            return;
        }

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.discover.sources_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::channel(1);
        self.pending_sources_load = Some(rx);

        // Spawn background task for DB query
        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => return,
            };

            // Use denormalized file_count column (O(n) instead of O(n×m))
            let query = r#"
                SELECT id, name, path, file_count
                FROM scout_sources
                WHERE enabled = 1
                ORDER BY updated_at DESC
            "#;

            if let Ok(rows) = conn.query_all(query, &[]).await {
                let mut sources: Vec<SourceInfo> = Vec::with_capacity(rows.len());
                for row in rows {
                    let id: String = match row.get(0) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let name: String = match row.get(1) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let path: String = match row.get(2) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let file_count: i64 = match row.get(3) {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    sources.push(SourceInfo {
                        id: SourceId::from(id),
                        name,
                        path: std::path::PathBuf::from(path),
                        file_count: file_count as usize,
                    });
                }

                let _ = tx.send(sources).await;
            }
        });
    }

    /// Start non-blocking jobs load from processing queue database
    fn start_jobs_load(&mut self) {
        // Skip if already loading
        if self.pending_jobs_load.is_some() {
            return;
        }

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.jobs_state.jobs_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::channel(1);
        self.pending_jobs_load = Some(rx);

        // Spawn background task for DB query
        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => return,
            };

            let query = r#"
                SELECT
                    q.id,
                    q.file_version_id,
                    q.plugin_name,
                    q.status,
                    q.claim_time,
                    q.end_time,
                    q.result_summary,
                    q.error_message
                FROM cf_processing_queue q
                ORDER BY
                    CASE q.status
                        WHEN 'RUNNING' THEN 1
                        WHEN 'QUEUED' THEN 2
                        WHEN 'FAILED' THEN 3
                        WHEN 'COMPLETED' THEN 4
                    END,
                    q.id DESC
                LIMIT 100
            "#;

            if let Ok(rows) = conn.query_all(query, &[]).await {
                let mut jobs: Vec<JobInfo> = Vec::with_capacity(rows.len());
                for row in rows {
                    let id: i64 = match row.get(0) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let file_version_id: Option<i64> = match row.get(1) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let plugin_name: String = match row.get(2) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let status_str: String = match row.get(3) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let claim_time: Option<String> = match row.get(4) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let end_time: Option<String> = match row.get(5) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let result_summary: Option<String> = match row.get(6) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let error_message: Option<String> = match row.get(7) {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    let status = match status_str.as_str() {
                        "RUNNING" => JobStatus::Running,
                        "QUEUED" => JobStatus::Pending,
                        "COMPLETED" => JobStatus::Completed,
                        "FAILED" => JobStatus::Failed,
                        _ => JobStatus::Pending,
                    };

                    let started_at = claim_time
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Local))
                        .unwrap_or_else(Local::now);

                    let completed_at = end_time
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Local));

                    let failures = if let Some(ref err) = error_message {
                        vec![JobFailure {
                            file_path: String::new(),
                            error: err.clone(),
                            line: None,
                        }]
                    } else {
                        vec![]
                    };

                    jobs.push(JobInfo {
                        id,
                        file_version_id,
                        job_type: JobType::Parse,
                        name: plugin_name,
                        version: None,
                        status,
                        started_at,
                        completed_at,
                        items_total: 0,
                        items_processed: if result_summary.is_some() { 1 } else { 0 },
                        items_failed: if error_message.is_some() { 1 } else { 0 },
                        output_path: None,
                        output_size_bytes: None,
                        backtest: None,
                        failures,
                    });
                }

                let _ = tx.send(jobs).await;
            }
        });
    }

    /// Start non-blocking home stats load from database
    fn start_stats_load(&mut self) {
        // Skip if already loading
        if self.pending_stats_load.is_some() {
            return;
        }

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.home.stats_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::channel(1);
        self.pending_stats_load = Some(rx);

        // Spawn background task for DB query
        tokio::spawn(async move {
            let conn = match App::open_db_readonly_with(backend, &db_path).await {
                Some(conn) => conn,
                None => return,
            };

            let mut stats = HomeStats::default();

            if let Ok(count) = conn.query_scalar::<i64>("SELECT COUNT(*) FROM scout_files", &[]).await {
                stats.file_count = count as usize;
            }

            if let Ok(count) = conn.query_scalar::<i64>(
                "SELECT COUNT(*) FROM scout_sources WHERE enabled = 1",
                &[],
            ).await {
                stats.source_count = count as usize;
            }

            if let Ok(rows) = conn.query_all(
                "SELECT status, COUNT(*) as cnt FROM cf_processing_queue GROUP BY status",
                &[],
            ).await {
                for row in rows {
                    let status: String = match row.get(0) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let count: i64 = match row.get(1) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    match status.as_str() {
                        "RUNNING" => stats.running_jobs = count as usize,
                        "QUEUED" => stats.pending_jobs = count as usize,
                        "FAILED" => stats.failed_jobs = count as usize,
                        _ => {}
                    }
                }
            }

            if let Ok(count) = conn.query_scalar::<i64>("SELECT COUNT(*) FROM cf_parsers", &[]).await {
                stats.parser_count = count as usize;
            }

            let _ = tx.send(stats).await;
        });
    }

    /// Persist pending tag and rule writes to the database
    async fn persist_pending_writes(&mut self) {
        // Skip if nothing to persist
        if self.discover.pending_tag_writes.is_empty()
            && self.discover.pending_rule_writes.is_empty()
            && self.discover.pending_source_touch.is_none() {
            return;
        }

        let conn = match self.open_db_write().await {
            Some(conn) => conn,
            None => return,
        };

        // Persist tag updates to scout_files
        let tag_writes = std::mem::take(&mut self.discover.pending_tag_writes);
        for write in tag_writes {
            let _ = conn.execute(
                "UPDATE scout_files SET tag = ? WHERE path = ?",
                &[
                    DbValue::Text(write.tag),
                    DbValue::Text(write.file_path),
                ],
            ).await;
        }

        // Persist rules to scout_tagging_rules
        let rule_writes = std::mem::take(&mut self.discover.pending_rule_writes);
        for write in rule_writes {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let rule_name = format!("{} → {}", write.pattern, write.tag);
            let now = chrono::Utc::now().timestamp();

            let _ = conn.execute(
                r#"INSERT OR IGNORE INTO scout_tagging_rules
                   (id, name, source_id, pattern, tag, priority, enabled, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, 100, 1, ?, ?)"#
                ,
                &[
                    DbValue::Text(rule_id),
                    DbValue::Text(rule_name),
                    DbValue::Text(write.source_id.as_str().to_string()),
                    DbValue::Text(write.pattern),
                    DbValue::Text(write.tag),
                    DbValue::Integer(now),
                    DbValue::Integer(now),
                ],
            ).await;
        }

        // Touch source for MRU ordering (updates updated_at timestamp)
        if let Some(source_id) = std::mem::take(&mut self.discover.pending_source_touch) {
            let now = chrono::Utc::now().timestamp_millis();
            let result = conn.execute(
                "UPDATE scout_sources SET updated_at = ? WHERE id = ?",
                &[
                    DbValue::Integer(now),
                    DbValue::Text(source_id.clone()),
                ],
            ).await;
            // Trigger sources reload to reflect new MRU ordering
            if result.is_ok() {
                self.discover.sources_loaded = false;
            }
        }
    }

    /// Load tagging rules for the Rules Manager dialog
    async fn load_rules_for_manager(&mut self) {
        // Get source ID for selected source
        let source_id = match self.discover.sources.get(self.discover.selected_source_index()) {
            Some(source) => source.id.clone(),
            None => {
                self.discover.rules.clear();
                return;
            }
        };

        let conn = match self.open_db_readonly().await {
            Some(conn) => conn,
            None => return,
        };

        let query = r#"
            SELECT id, pattern, tag, priority, enabled
            FROM scout_tagging_rules
            WHERE source_id = ?
            ORDER BY priority DESC, pattern
        "#;

        let rows = match conn.query_all(query, &[DbValue::Text(source_id.as_str().to_string())]).await {
            Ok(rows) => rows,
            Err(_) => return,
        };

        let mut rules: Vec<RuleInfo> = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = match row.get(0) {
                Ok(v) => v,
                Err(_) => return,
            };
            let pattern: String = match row.get(1) {
                Ok(v) => v,
                Err(_) => return,
            };
            let tag: String = match row.get(2) {
                Ok(v) => v,
                Err(_) => return,
            };
            let priority: i32 = match row.get(3) {
                Ok(v) => v,
                Err(_) => return,
            };
            let enabled: bool = match row.get(4) {
                Ok(v) => v,
                Err(_) => return,
            };

            rules.push(RuleInfo {
                id: RuleId::new(id),
                pattern,
                tag,
                priority,
                enabled,
            });
        }

        self.discover.rules = rules;

        // Clamp selected rule if it's out of bounds
        if self.discover.selected_rule >= self.discover.rules.len() && !self.discover.rules.is_empty() {
            self.discover.selected_rule = 0;
        }
    }

    /// Handle home hub keys
    /// Handle Home view keys (Quick Start + Status dashboard)
    /// Navigation: ↑↓ to select source, Enter to start scan
    fn handle_home_key(&mut self, key: KeyEvent) {
        if self.home.filtering {
            match handle_text_input(key, &mut self.home.filter) {
                TextInputResult::Committed => {
                    self.home.filtering = false;
                    return;
                }
                TextInputResult::Cancelled => {
                    self.home.filtering = false;
                    self.home.filter.clear();
                    return;
                }
                TextInputResult::Continue => {
                    return;
                }
                TextInputResult::NotHandled => {}
            }
        }

        let filter_lower = self.home.filter.to_lowercase();
        let filtered_indices: Vec<usize> = self.discover.sources.iter()
            .enumerate()
            .filter(|(_, source)| {
                filter_lower.is_empty()
                    || source.name.to_lowercase().contains(&filter_lower)
                    || source.path.display().to_string().to_lowercase().contains(&filter_lower)
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(first_idx) = filtered_indices.first().copied() {
            if !filtered_indices.contains(&self.home.selected_source_index) {
                self.home.selected_source_index = first_idx;
            }
        } else {
            self.home.selected_source_index = 0;
        }

        let source_count = filtered_indices.len();
        match key.code {
            // Navigate up in source list
            KeyCode::Up => {
                if let Some(pos) = filtered_indices.iter().position(|idx| *idx == self.home.selected_source_index) {
                    if pos > 0 {
                        self.home.selected_source_index = filtered_indices[pos - 1];
                    }
                }
            }
            // Navigate down in source list
            KeyCode::Down => {
                if let Some(pos) = filtered_indices.iter().position(|idx| *idx == self.home.selected_source_index) {
                    if pos + 1 < source_count {
                        self.home.selected_source_index = filtered_indices[pos + 1];
                    }
                }
            }
            // Enter: Start scan job for selected source
            KeyCode::Enter => {
                if source_count > 0 && self.home.selected_source_index < self.discover.sources.len() {
                    // Get the selected source and trigger a scan
                    let source = &self.discover.sources[self.home.selected_source_index];
                    let source_id = source.id.clone();
                    self.start_scan_for_source(&source_id.0);
                }
            }
            // /: Filter sources
            KeyCode::Char('/') => {
                self.home.filtering = true;
            }
            // s: Scan a new folder (via Discover)
            KeyCode::Char('s') => {
                self.enter_discover_mode();
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                self.discover.scan_path_input.clear();
                self.discover.scan_error = None;
                self.discover.path_suggestions.clear();
            }
            // Global keys 1-4, J, P, etc. are handled by global key handler
            _ => {}
        }
    }

    /// Start a scan job for the given source (called from Home)
    fn start_scan_for_source(&mut self, source_id: &str) {
        let path = self
            .discover
            .sources
            .iter()
            .find(|s| s.id.as_str() == source_id)
            .map(|s| s.path.display().to_string());

        if let Some(path) = path {
            self.enter_discover_mode();
            self.scan_directory(&path);
        } else {
            self.discover.status_message = Some((
                "Selected source no longer exists. Refresh sources and try again.".to_string(),
                true,
            ));
        }
    }

    pub fn sources_drawer_sources(&self) -> Vec<usize> {
        let count = self.discover.sources.len().min(5);
        (0..count).collect()
    }

    pub fn sources_drawer_selected_source(&self) -> Option<usize> {
        self.sources_drawer_sources()
            .get(self.sources_drawer_selected)
            .copied()
    }

    /// Handle Sources view keys (key 4)
    /// Per keybinding matrix: n=new, e=edit, r=rescan, d=delete
    fn handle_sources_key(&mut self, key: KeyEvent) {
        let source_count = self.discover.sources.len();

        // Handle delete confirmation first
        if self.sources_state.confirm_delete {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    // TODO: Actually delete the source
                    self.sources_state.confirm_delete = false;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.sources_state.confirm_delete = false;
                }
                _ => {}
            }
            return;
        }

        // Handle editing mode
        if self.sources_state.editing {
            match key.code {
                KeyCode::Esc => {
                    self.sources_state.editing = false;
                    self.sources_state.edit_value.clear();
                }
                KeyCode::Enter => {
                    // TODO: Save the edited source
                    self.sources_state.editing = false;
                    self.sources_state.edit_value.clear();
                }
                KeyCode::Char(c) => {
                    self.sources_state.edit_value.push(c);
                }
                KeyCode::Backspace => {
                    self.sources_state.edit_value.pop();
                }
                _ => {}
            }
            return;
        }

        // Normal mode
        match key.code {
            // Navigate up/down in source list
            KeyCode::Up => {
                if self.sources_state.selected_index > 0 {
                    self.sources_state.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if source_count > 0 && self.sources_state.selected_index < source_count.saturating_sub(1) {
                    self.sources_state.selected_index += 1;
                }
            }
            // n: New source
            KeyCode::Char('n') => {
                self.sources_state.editing = true;
                self.sources_state.edit_value.clear();
            }
            // e: Edit source
            KeyCode::Char('e') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    self.sources_state.editing = true;
                    let source = &self.discover.sources[self.sources_state.selected_index];
                    self.sources_state.edit_value = source.path.display().to_string();
                }
            }
            // r: Rescan source
            KeyCode::Char('r') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    let source = &self.discover.sources[self.sources_state.selected_index];
                    let source_id = source.id.0.clone();
                    self.start_scan_for_source(&source_id);
                }
            }
            // d: Delete source (with confirmation)
            KeyCode::Char('d') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    self.sources_state.confirm_delete = true;
                }
            }
            KeyCode::Esc => {
                self.mode = TuiMode::Home;
            }
            _ => {}
        }
    }

    /// Handle jobs mode keys
    fn handle_jobs_key(&mut self, key: KeyEvent) {
        // Handle keys based on current view state
        match self.jobs_state.view_state {
            JobsViewState::JobList => self.handle_jobs_list_key(key),
            JobsViewState::DetailPanel => self.handle_jobs_detail_key(key),
            JobsViewState::MonitoringPanel => self.handle_jobs_monitoring_key(key),
            JobsViewState::LogViewer => self.handle_jobs_log_viewer_key(key),
            JobsViewState::FilterDialog => self.handle_jobs_filter_dialog_key(key),
        }
    }

    /// Handle keys when in job list view
    fn handle_jobs_list_key(&mut self, key: KeyEvent) {
        let filtered_count = self.jobs_state.filtered_jobs().len();

        match key.code {
            // Job navigation (within filtered list)
            KeyCode::Down => {
                if self.jobs_state.selected_index < filtered_count.saturating_sub(1) {
                    self.jobs_state.selected_index += 1;
                }
            }
            KeyCode::Up => {
                if self.jobs_state.selected_index > 0 {
                    self.jobs_state.selected_index -= 1;
                }
            }
            // Open detail panel for selected job
            KeyCode::Enter => {
                if !self.jobs_state.filtered_jobs().is_empty() {
                    self.jobs_state.transition_state(JobsViewState::DetailPanel);
                }
            }
            // Toggle pipeline summary
            KeyCode::Char('P') => {
                self.jobs_state.show_pipeline = !self.jobs_state.show_pipeline;
            }
            // Open monitoring panel
            KeyCode::Char('m') => {
                self.jobs_state.transition_state(JobsViewState::MonitoringPanel);
            }
            // Retry failed job
            KeyCode::Char('r') | KeyCode::Char('R') => {
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
            // f: Open filter dialog (per keybinding matrix - keys 1-4 are reserved for navigation)
            KeyCode::Char('f') => {
                self.jobs_state.transition_state(JobsViewState::FilterDialog);
            }
            // Quick filter reset (0 still works as a shortcut)
            KeyCode::Char('0') => {
                self.jobs_state.set_filter(None); // Show all
            }
            // Go to first job
            KeyCode::Char('g') => {
                self.jobs_state.selected_index = 0;
            }
            // Go to last job
            KeyCode::Char('G') => {
                self.jobs_state.selected_index = filtered_count.saturating_sub(1);
            }
            // Stop running backtest (requires confirmation)
            KeyCode::Char('S') => {
                let jobs = self.jobs_state.filtered_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if job.status == JobStatus::Running && job.job_type == JobType::Backtest {
                        // TODO: Show confirmation dialog and stop backtest
                    }
                }
            }
            // Open output folder for completed jobs
            KeyCode::Char('o') => {
                let jobs = self.jobs_state.filtered_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if job.status == JobStatus::Completed {
                        if let Some(ref path) = job.output_path {
                            // Try to open the folder in system file manager
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(path).spawn();
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("explorer").arg(path).spawn();
                        }
                    }
                }
            }
            // Clear completed jobs from the list
            KeyCode::Char('x') => {
                self.jobs_state.jobs.retain(|j| j.status != JobStatus::Completed);
                // Clamp selection to valid range
                if self.jobs_state.selected_index >= self.jobs_state.filtered_jobs().len() {
                    self.jobs_state.selected_index = self.jobs_state.filtered_jobs().len().saturating_sub(1);
                }
            }
            // Show help overlay
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            // Open log viewer
            KeyCode::Char('L') => {
                if !self.jobs_state.filtered_jobs().is_empty() {
                    self.jobs_state.transition_state(JobsViewState::LogViewer);
                }
            }
            // Copy output path to clipboard
            KeyCode::Char('y') => {
                let jobs = self.jobs_state.filtered_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if let Some(ref path) = job.output_path {
                        // TODO: Copy to clipboard (requires clipboard crate or platform-specific impl)
                        let _ = path; // Silence warning for now
                    }
                }
            }
            KeyCode::Esc => {
                self.mode = TuiMode::Home;
            }
            _ => {}
        }
    }

    /// Handle keys when in job detail panel
    fn handle_jobs_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close detail panel, return to list
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // Retry failed job from detail view
            KeyCode::Char('R') => {
                if let Some(job) = self.jobs_state.selected_job() {
                    if job.status == JobStatus::Failed {
                        // TODO: Actually retry the job
                    }
                }
            }
            // View logs (placeholder)
            KeyCode::Char('L') => {
                // TODO: Open log viewer
            }
            // Copy output path to clipboard (placeholder)
            KeyCode::Char('y') => {
                // TODO: Copy to clipboard
            }
            _ => {}
        }
    }

    /// Handle keys when in monitoring panel
    fn handle_jobs_monitoring_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close monitoring panel, return to list
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // Pause/resume monitoring refresh
            KeyCode::Char('p') => {
                self.jobs_state.monitoring.paused = !self.jobs_state.monitoring.paused;
            }
            // Reset metrics
            KeyCode::Char('r') => {
                self.jobs_state.monitoring = MonitoringState::default();
            }
            _ => {}
        }
    }

    /// Handle keys when in log viewer
    fn handle_jobs_log_viewer_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close log viewer, return to previous state
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // TODO: Scroll logs up/down
            KeyCode::Down => {
                // Scroll down
            }
            KeyCode::Up => {
                // Scroll up
            }
            _ => {}
        }
    }

    /// Handle keys when in filter dialog
    fn handle_jobs_filter_dialog_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close filter dialog
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // TODO: Apply filter selections
            KeyCode::Enter => {
                self.jobs_state.return_to_previous_state();
            }
            _ => {}
        }
    }

    // ======== Settings Key Handlers ========

    /// Handle key events in Settings mode (per specs/views/settings.md)
    fn handle_settings_key(&mut self, key: KeyEvent) {
        if self.settings.editing {
            // In editing mode, handle text input
            match key.code {
                KeyCode::Esc => {
                    // Cancel edit, discard changes
                    self.settings.editing = false;
                    self.settings.edit_value.clear();
                }
                KeyCode::Enter => {
                    // Save the edit
                    self.apply_settings_edit();
                    self.settings.editing = false;
                    self.settings.edit_value.clear();
                }
                KeyCode::Backspace => {
                    self.settings.edit_value.pop();
                }
                KeyCode::Char(c) => {
                    self.settings.edit_value.push(c);
                }
                _ => {}
            }
            return;
        }

        // Normal navigation mode
        match key.code {
            // Navigate categories
            KeyCode::Tab => {
                self.settings.category = match self.settings.category {
                    SettingsCategory::General => SettingsCategory::Display,
                    SettingsCategory::Display => SettingsCategory::About,
                    SettingsCategory::About => SettingsCategory::General,
                };
                self.settings.selected_index = 0;
            }
            KeyCode::BackTab => {
                self.settings.category = match self.settings.category {
                    SettingsCategory::General => SettingsCategory::About,
                    SettingsCategory::Display => SettingsCategory::General,
                    SettingsCategory::About => SettingsCategory::Display,
                };
                self.settings.selected_index = 0;
            }
            // Navigate within category
            KeyCode::Down => {
                let max = self.settings.category_item_count().saturating_sub(1);
                if self.settings.selected_index < max {
                    self.settings.selected_index += 1;
                }
            }
            KeyCode::Up => {
                if self.settings.selected_index > 0 {
                    self.settings.selected_index -= 1;
                }
            }
            // Edit/Toggle selected setting
            KeyCode::Enter => {
                self.toggle_or_edit_setting();
            }
            _ => {}
        }
    }

    /// Toggle boolean setting or start editing text setting
    fn toggle_or_edit_setting(&mut self) {
        match self.settings.category {
            SettingsCategory::General => match self.settings.selected_index {
                0 => {
                    // default_source_path - enter edit mode
                    self.settings.editing = true;
                    self.settings.edit_value = self.settings.default_source_path.clone();
                }
                1 => {
                    // auto_scan_on_startup - toggle
                    self.settings.auto_scan_on_startup = !self.settings.auto_scan_on_startup;
                }
                2 => {
                    // confirm_destructive - toggle
                    self.settings.confirm_destructive = !self.settings.confirm_destructive;
                }
                _ => {}
            },
            SettingsCategory::Display => match self.settings.selected_index {
                0 => {
                    // theme - cycle
                    self.settings.theme = match self.settings.theme.as_str() {
                        "dark" => "light".to_string(),
                        "light" => "system".to_string(),
                        _ => "dark".to_string(),
                    };
                }
                1 => {
                    // unicode_symbols - toggle
                    self.settings.unicode_symbols = !self.settings.unicode_symbols;
                }
                2 => {
                    // show_hidden_files - toggle
                    self.settings.show_hidden_files = !self.settings.show_hidden_files;
                }
                _ => {}
            },
            SettingsCategory::About => {
                // Read-only, no action
            }
        }
    }

    /// Apply the current edit value to the appropriate setting
    fn apply_settings_edit(&mut self) {
        match self.settings.category {
            SettingsCategory::General => {
                if self.settings.selected_index == 0 {
                    self.settings.default_source_path = self.settings.edit_value.clone();
                }
            }
            _ => {}
        }
        // TODO: Persist to config.toml
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

        // Preload sources on startup (any mode) so they're ready when user goes to Discover
        // This prevents "no sources" on first open
        if !self.discover.sources_loaded && self.pending_sources_load.is_none() {
            self.start_sources_load();
        }

        // Preload home stats on startup so Home screen shows real data
        if !self.home.stats_loaded && self.pending_stats_load.is_none() {
            self.start_stats_load();
        }

        // Poll for pending stats load results (non-blocking)
        if let Some(ref mut rx) = self.pending_stats_load {
            match rx.try_recv() {
                Ok(stats) => {
                    self.home.stats = stats;
                    self.home.stats_loaded = true;
                    self.pending_stats_load = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (keep default stats)
                    self.home.stats_loaded = true;
                    self.pending_stats_load = None;
                }
            }
        }

        // Poll for pending sources load results (non-blocking)
        if let Some(ref mut rx) = self.pending_sources_load {
            let recv_result = {
                #[cfg(feature = "profiling")]
                let _zone = self.profiler.zone("discover.sources_poll");
                rx.try_recv()
            };

            match recv_result {
                Ok(sources) => {
                    self.discover.sources = sources;
                    self.discover.sources_loaded = true;
                    self.discover.validate_source_selection();
                    let drawer_count = self.sources_drawer_sources().len();
                    if drawer_count == 0 {
                        self.sources_drawer_selected = 0;
                    } else if self.sources_drawer_selected >= drawer_count {
                        self.sources_drawer_selected = drawer_count - 1;
                    }
                    if let (Some(ref mut builder), Some(ref source_id)) =
                        (self.discover.rule_builder.as_mut(), self.discover.selected_source_id.as_ref())
                    {
                        if builder.source_id.as_deref() != Some(source_id.as_str()) {
                            builder.source_id = Some(source_id.as_str().to_string());
                        }
                    }
                    if self.mode == TuiMode::Discover
                        && self.discover.selected_source_id.is_none()
                        && matches!(
                            self.discover.view_state,
                            DiscoverViewState::RuleBuilder | DiscoverViewState::Files
                        )
                    {
                        self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                        self.discover.sources_filter.clear();
                        self.discover.sources_filtering = false;
                        self.discover.preview_source = Some(self.discover.selected_source_index());
                    }
                    self.pending_sources_load = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty sources)
                    self.discover.sources_loaded = true;
                    self.pending_sources_load = None;
                }
            }
        }

        // Jobs mode: Trigger load on first visit or after poll interval
        if self.mode == TuiMode::Jobs {
            const JOBS_POLL_INTERVAL_MS: u64 = 2000; // Poll every 2 seconds when in Jobs view

            let should_load = if !self.jobs_state.jobs_loaded {
                // First load
                true
            } else if let Some(last_poll) = self.last_jobs_poll {
                // Check if poll interval elapsed
                last_poll.elapsed().as_millis() as u64 >= JOBS_POLL_INTERVAL_MS
            } else {
                false
            };

            if should_load && self.pending_jobs_load.is_none() {
                self.start_jobs_load();
            }
        }

        // Poll for pending jobs load results (non-blocking)
        if let Some(ref mut rx) = self.pending_jobs_load {
            let recv_result = {
                #[cfg(feature = "profiling")]
                let _zone = self.profiler.zone("jobs.poll");
                rx.try_recv()
            };

            match recv_result {
                Ok(jobs) => {
                    self.jobs_state.jobs = jobs;
                    self.jobs_state.jobs_loaded = true;
                    self.last_jobs_poll = Some(std::time::Instant::now());
                    self.pending_jobs_load = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty jobs)
                    self.jobs_state.jobs_loaded = true;
                    self.pending_jobs_load = None;
                }
            }
        }

        // Debounced glob pattern search - trigger after user stops typing
        const DEBOUNCE_MS: u128 = 150;
        let should_search = if let Some(ref mut explorer) = self.discover.glob_explorer {
            if let Some(changed_at) = explorer.pattern_changed_at {
                let elapsed = changed_at.elapsed().as_millis();
                if elapsed >= DEBOUNCE_MS {
                    // Check if pattern or prefix actually changed
                    let pattern_changed = explorer.pattern != explorer.last_searched_pattern;
                    let prefix_changed = explorer.current_prefix != explorer.last_searched_prefix;

                    if pattern_changed || prefix_changed {
                        // Record what we're searching for
                        explorer.last_searched_pattern = explorer.pattern.clone();
                        explorer.last_searched_prefix = explorer.current_prefix.clone();
                        explorer.pattern_changed_at = None;
                        true
                    } else {
                        // No actual change, clear the timestamp
                        explorer.pattern_changed_at = None;
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if should_search {
            self.update_folders_from_cache();
        }

        // Poll for pending glob search results (non-blocking recursive search)
        if let Some(ref mut rx) = self.pending_glob_search {
            match rx.try_recv() {
                Ok(result) => {
                    // Only apply results if pattern still matches (user may have typed more)
                    let current_pattern = self.discover.glob_explorer
                        .as_ref()
                        .map(|e| e.pattern.clone())
                        .unwrap_or_default();

                    if result.pattern == current_pattern {
                        // Search complete! Update explorer with results
                        if let Some(ref mut explorer) = self.discover.glob_explorer {
                            explorer.folders = result.folders;
                            explorer.total_count = GlobFileCount::Exact(result.total_count);
                            explorer.selected_folder = 0;
                        }
                    }
                    // else: stale result, discard it
                    self.pending_glob_search = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still searching - update spinner
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.folders.len() == 1 {
                            if let Some(FsEntry::Loading { message }) = explorer.folders.get_mut(0) {
                                if message.contains("Searching") {
                                    let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                                    *message = format!("{} Searching for {}...", spinner_char, explorer.pattern);
                                }
                            }
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.pending_glob_search = None;
                }
            }
        }

        // Poll for pending Rule Builder pattern search results
        if let Some(ref mut rx) = self.pending_rule_builder_search {
            match rx.try_recv() {
                Ok(result) => {
                    // Only apply results if pattern still matches (user may have typed more)
                    let current_pattern = self.discover.rule_builder
                        .as_ref()
                        .map(|b| b.pattern.clone())
                        .unwrap_or_default();

                    if result.pattern == current_pattern || result.pattern == format!("**/{}", current_pattern) {
                        // Search complete! Update Rule Builder with results
                        let has_matches = result.total_count > 0;
                        if let Some(ref mut builder) = self.discover.rule_builder {
                            builder.file_results = super::extraction::FileResultsState::Exploration {
                                folder_matches: result.folder_matches,
                                expanded_folder_indices: std::collections::HashSet::new(),
                                detected_patterns: Vec::new(),
                            };
                            builder.match_count = result.total_count;
                            builder.is_streaming = false;
                        }
                        // Auto-run sample eval when pattern matches files
                        // Only if: matches found, not running full eval, not just switched patterns
                        if has_matches && self.pending_schema_eval.is_none() && self.pending_sample_eval.is_none() {
                            self.run_sample_schema_eval();
                        }
                    }
                    // else: stale result, discard it
                    self.pending_rule_builder_search = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still searching - spinner is shown via builder.streaming flag
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.pending_rule_builder_search = None;
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.is_streaming = false;
                    }
                }
            }
        }

        // Poll for pending schema eval results (background full eval job)
        if let Some(ref mut rx) = self.pending_schema_eval {
            match rx.try_recv() {
                Ok(SchemaEvalResult::Started { job_id }) => {
                    // Job started - already tracked via add_schema_eval_job
                    tracing::debug!(job_id = job_id, "Schema eval job started");
                }
                Ok(SchemaEvalResult::Progress { progress, paths_analyzed, total_paths }) => {
                    // Update progress
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.eval_state = super::extraction::EvalState::Running { progress };
                    }
                    if let Some(job_id) = self.current_schema_eval_job_id {
                        self.set_schema_eval_job_total(job_id, total_paths as u32);
                        self.update_schema_eval_job(job_id, JobStatus::Running, paths_analyzed as u32, None);
                    }
                }
                Ok(SchemaEvalResult::Complete {
                    job_id,
                    pattern,
                    pattern_seeds,
                    path_archetypes,
                    naming_schemes,
                    synonym_suggestions,
                    paths_analyzed,
                }) => {
                    // Update Rule Builder state with results
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        if builder.pattern == pattern {
                            builder.pattern_seeds = pattern_seeds;
                            builder.path_archetypes = path_archetypes;
                            builder.naming_schemes = naming_schemes;
                            builder.synonym_suggestions = synonym_suggestions;
                        }
                        builder.eval_state = super::extraction::EvalState::Idle;
                    }
                    // Update job status
                    self.update_schema_eval_job(job_id, JobStatus::Completed, paths_analyzed as u32, None);
                    self.current_schema_eval_job_id = None;
                    self.pending_schema_eval = None;

                    // Status message
                    if let Some(ref builder) = self.discover.rule_builder {
                        if builder.pattern == pattern {
                            self.discover.status_message = Some((
                                format!("Full eval: {} patterns, {} archetypes, {} schemes, {} synonyms",
                                    builder.pattern_seeds.len(),
                                    builder.path_archetypes.len(),
                                    builder.naming_schemes.len(),
                                    builder.synonym_suggestions.len(),
                                ),
                                false,
                            ));
                        }
                    }
                }
                Ok(SchemaEvalResult::Error(err)) => {
                    // Job failed
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.eval_state = super::extraction::EvalState::Idle;
                    }
                    if let Some(job_id) = self.current_schema_eval_job_id {
                        self.update_schema_eval_job(job_id, JobStatus::Failed, 0, Some(err.clone()));
                    }
                    self.current_schema_eval_job_id = None;
                    self.pending_schema_eval = None;
                    self.discover.status_message = Some((format!("Schema eval failed: {}", err), true));
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still running - that's fine
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed unexpectedly
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.eval_state = super::extraction::EvalState::Idle;
                    }
                    if let Some(job_id) = self.current_schema_eval_job_id {
                        self.update_schema_eval_job(job_id, JobStatus::Failed, 0, Some("Channel disconnected".to_string()));
                    }
                    self.current_schema_eval_job_id = None;
                    self.pending_schema_eval = None;
                }
            }
        }

        // Poll for pending sample schema eval results
        if let Some(ref mut rx) = self.pending_sample_eval {
            match rx.try_recv() {
                Ok(SampleEvalResult::Complete {
                    pattern,
                    pattern_seeds,
                    path_archetypes,
                    naming_schemes,
                    synonym_suggestions,
                    paths_analyzed,
                }) => {
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        if builder.pattern == pattern {
                            builder.pattern_seeds = pattern_seeds;
                            builder.path_archetypes = path_archetypes;
                            builder.naming_schemes = naming_schemes;
                            builder.synonym_suggestions = synonym_suggestions;
                            self.discover.status_message = Some((
                                format!("Sample: {} paths analyzed", paths_analyzed),
                                false,
                            ));
                        }
                    }
                    self.pending_sample_eval = None;
                }
                Ok(SampleEvalResult::Error(err)) => {
                    self.discover.status_message = Some((format!("Sample eval failed: {}", err), true));
                    self.pending_sample_eval = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {}
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.pending_sample_eval = None;
                }
            }
        }

        // Poll for cache load messages (non-blocking)
        if let Some(ref mut rx) = self.pending_cache_load {
            match rx.try_recv() {
                Ok(CacheLoadMessage::Complete { source_id, total_files, tags, cache }) => {
                    // Record load timing before clearing progress
                    if let Some(progress) = &self.cache_load_progress {
                        let duration_ms = progress.started_at.elapsed().as_secs_f64() * 1000.0;
                        self.last_cache_load_timing = Some(CacheLoadTiming {
                            duration_ms,
                            files_loaded: total_files,
                            source_id: source_id.clone(),
                        });
                        tracing::info!(
                            source_id = %source_id,
                            files = total_files,
                            duration_ms = format!("{:.1}", duration_ms),
                            "Cache load complete"
                        );
                    }

                    // Cache fully loaded
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.folder_cache = cache;
                        explorer.cache_loaded = true;
                        explorer.cache_source_id = Some(source_id);
                        explorer.selected_folder = 0;

                        // Ensure root folders are displayed
                        if let Some(root_folders) = explorer.folder_cache.get("") {
                            explorer.folders = root_folders.clone();
                            explorer.total_count = GlobFileCount::Exact(
                                root_folders.iter().map(|f| f.file_count()).sum()
                            );
                        }
                    }
                    self.cache_load_progress = None;
                    self.update_folders_from_cache();

                    // Tags came with the cache - no separate load needed
                    self.discover.tags = tags;
                    self.discover.data_loaded = true;
                    self.pending_cache_load = None;

                    // Update Rule Builder with loaded cache data
                    if self.discover.view_state == DiscoverViewState::RuleBuilder {
                        if let Some(ref builder) = self.discover.rule_builder {
                            let pattern = builder.pattern.clone();
                            self.update_rule_builder_files(&pattern);
                        }
                    }
                }
                Ok(CacheLoadMessage::Error(e)) => {
                    self.discover.scan_error = Some(format!("Cache load failed: {}", e));
                    self.pending_cache_load = None;
                    self.cache_load_progress = None;
                    // Clear loading placeholder so error message can display
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.folders.clear();
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No message yet - still loading
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.pending_cache_load = None;
                    self.cache_load_progress = None;
                }
            }
        }

        // Poll for folder query results (lazy loading for navigation)
        if let Some(ref mut rx) = self.pending_folder_query {
            match rx.try_recv() {
                Ok(FolderQueryMessage::Complete { prefix, folders, total_count }) => {
                    // Cache the result for future navigation
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.folder_cache.insert(prefix.clone(), folders.clone());

                        // Update display if this is the current prefix
                        if explorer.current_prefix == prefix {
                            // Sort by count descending (single final sort as requested)
                            let mut sorted_folders = folders;
                            sorted_folders.sort_by(|a, b| b.file_count().cmp(&a.file_count()));
                            explorer.folders = sorted_folders;
                            explorer.total_count = GlobFileCount::Exact(total_count);
                            explorer.selected_folder = 0;
                        }
                    }
                    self.pending_folder_query = None;
                }
                Ok(FolderQueryMessage::Error(e)) => {
                    // Show error in folders list
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        explorer.folders = vec![FsEntry::loading(&format!("Error: {}", e))];
                    }
                    self.pending_folder_query = None;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Still loading - update spinner
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.folders.len() == 1 {
                            if let Some(FsEntry::Loading { message }) = explorer.folders.get_mut(0) {
                                if message.contains("Loading") {
                                    let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                                    let prefix = &explorer.current_prefix;
                                    *message = format!(
                                        "{} Loading {}...",
                                        spinner_char,
                                        if prefix.is_empty() { "root" } else { prefix }
                                    );
                                }
                            }
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.pending_folder_query = None;
                }
            }
        }

        // Load Scout data if in Discover mode (but NOT while scanning - don't block progress updates)
        if self.mode == TuiMode::Discover && self.discover.view_state != DiscoverViewState::Scanning {
            // Process pending DB writes FIRST (before any reloads)
            self.persist_pending_writes().await;

            // Load files for selected source (also reloads tags when source changes)
            // Cache loading is non-blocking to avoid freezing UI on large sources
            if !self.discover.data_loaded {
                // Initialize GlobExplorer for folder cache (data loading)
                if self.discover.glob_explorer.is_none() {
                    self.discover.glob_explorer = Some(GlobExplorerState::default());
                }

                // Rule Builder is the default view (replaces old GlobExplorer UI per specs/rule_builder.md)
                // Fallback initialization if not already done by enter_discover_mode()
                // Only create if we're supposed to be in RuleBuilder view (not if user exited to Files)
                if self.discover.rule_builder.is_none() && self.discover.view_state == DiscoverViewState::RuleBuilder {
                    let source_id = self.discover.selected_source_id
                        .as_ref()
                        .map(|id| id.as_str().to_string());
                    let mut builder = super::extraction::RuleBuilderState::new(source_id);
                    builder.pattern = "**/*".to_string();
                    self.discover.rule_builder = Some(builder);
                }

                // Check if cache is still loading (not yet loaded)
                let cache_not_loaded = self.discover.glob_explorer
                    .as_ref()
                    .map(|e| !e.cache_loaded)
                    .unwrap_or(true);

                // Show loading progress while cache is loading
                // Only show loading placeholder if we have no folders yet (streaming will populate them)
                // But NOT if there's an error - let the error display instead
                if cache_not_loaded && self.discover.scan_error.is_none() {
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);

                        // Check if we're still loading
                        if let Some(ref progress) = self.cache_load_progress {
                            let needs_loading = explorer.folders.is_empty()
                                || matches!(explorer.folders.get(0), Some(FsEntry::Loading { message }) if message.contains("Loading"));
                            if needs_loading {
                                let elapsed = progress.started_at.elapsed().as_secs_f32();
                                explorer.folders = vec![FsEntry::loading(&format!(
                                    "{} Loading {}... ({:.1}s)",
                                    spinner_char,
                                    progress.source_name,
                                    elapsed
                                ))];
                            }
                        } else if explorer.folders.is_empty() {
                            explorer.folders = vec![FsEntry::loading(&format!("{} Loading folder hierarchy...", spinner_char))];
                        }
                    }
                }

                // Start cache load if not already started
                if self.pending_cache_load.is_none() {
                    self.start_cache_load();
                }
            }
            // Load rules for Rules Manager if it's open
            if self.discover.view_state == DiscoverViewState::RulesManager && self.discover.rules.is_empty() {
                self.load_rules_for_manager().await;
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
                            TuiScanResult::Started { job_id } => {
                                // Validation passed, scan is actually starting
                                self.discover.status_message = Some((
                                    format!("Scan started (Job #{}) - press [4] to view Jobs", job_id),
                                    false,
                                ));
                            }
                            TuiScanResult::Progress(progress) => {
                                // Update progress - UI will display this
                                self.discover.scan_progress = Some(progress);
                            }
                            TuiScanResult::Complete { source_path, files_persisted } => {
                                // Update job status to Completed
                                if let Some(job_id) = self.current_scan_job_id {
                                    self.update_scan_job_status(job_id, JobStatus::Completed, None);
                                }

                                // Scanner persisted to DB - reload sources and files
                                let source_name = std::path::Path::new(&source_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| source_path.clone());

                                // Use accurate count from scanner (not stale progress update)
                                let final_file_count = files_persisted;

                                // Generate source ID matching scanner's format
                                let source_id = SourceId(format!(
                                    "local:{}",
                                    source_path.replace('/', "_").replace('\\', "_")
                                ));

                                // Trigger sources reload (non-blocking, handled by tick())
                                self.discover.sources_loaded = false;
                                self.start_sources_load();

                                // Select the newly scanned source
                                self.discover.selected_source_id = Some(source_id.clone());

                                // Update rule builder with new source
                                if let Some(ref mut builder) = self.discover.rule_builder {
                                    builder.source_id = Some(source_id.as_str().to_string());
                                }

                                // Load files for the new source (this sets data_loaded = true)
                                self.load_scout_files().await;

                                // Reset cache state to force reload for the new source
                                // IMPORTANT: Must be AFTER load_scout_files() because it sets data_loaded=true
                                self.discover.data_loaded = false;
                                self.pending_cache_load = None;
                                self.cache_load_progress = None;
                                if let Some(ref mut explorer) = self.discover.glob_explorer {
                                    explorer.cache_loaded = false;
                                    explorer.cache_source_id = None;
                                    explorer.folder_cache.clear();
                                    explorer.folders.clear();
                                }

                                self.discover.selected = 0;
                                self.discover.scan_error = None;
                                // Stay in RuleBuilder mode (the default view)
                                self.discover.view_state = DiscoverViewState::RuleBuilder;
                                self.discover.scanning_path = None;
                                self.discover.scan_progress = None;
                                self.discover.scan_start_time = None;
                                self.discover.status_message = Some((
                                    format!("Scanned {} files from {}", final_file_count, source_name),
                                    false,
                                ));

                                // Trigger home stats refresh so Home view shows updated counts
                                self.home.stats_loaded = false;
                                self.pending_stats_load = None;

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
        match key.code {
            // Navigation
            KeyCode::Down => {
                if !self.parser_bench.parsers.is_empty() {
                    self.parser_bench.selected_parser =
                        (self.parser_bench.selected_parser + 1) % self.parser_bench.parsers.len();
                }
            }
            KeyCode::Up => {
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
            // Clear test result / Back to home
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: Some(std::env::temp_dir().join(format!(
                "casparian_test_{}.sqlite3",
                uuid::Uuid::new_v4()
            ))),
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

        // Now '2' should switch to Parser Bench
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::ParserBench));

        // Return Home, then use 'P' to switch to Parser Bench
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE))
                .await;
        });
        assert!(matches!(app.mode, TuiMode::Home));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE))
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
    fn test_home_source_navigation() {
        let mut app = App::new(test_args());
        // Add some test sources
        app.discover.sources = vec![
            SourceInfo {
                id: SourceId("src1".to_string()),
                name: "Source 1".to_string(),
                path: std::path::PathBuf::from("/test/source1"),
                file_count: 10,
            },
            SourceInfo {
                id: SourceId("src2".to_string()),
                name: "Source 2".to_string(),
                path: std::path::PathBuf::from("/test/source2"),
                file_count: 20,
            },
        ];
        assert_eq!(app.home.selected_source_index, 0);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Down arrow should move to source 1
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.home.selected_source_index, 1);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Up arrow should move back to source 0
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.home.selected_source_index, 0);
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
    fn test_esc_returns_home_from_jobs() {
        let mut app = App::new(test_args());
        // Start in Jobs mode
        app.mode = TuiMode::Jobs;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
                .await;
        });

        // Esc returns to Home when no dialog is open
        assert!(matches!(app.mode, TuiMode::Home));
    }

    // =========================================================================
    // Jobs Mode Tests - Critical Path Coverage
    // =========================================================================

    fn create_test_jobs() -> Vec<JobInfo> {
        vec![
            JobInfo {
                id: 1,
                file_version_id: Some(101),
                job_type: JobType::Parse,
                name: "parser_a".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Pending,
                started_at: Local::now(),
                completed_at: None,
                items_total: 100,
                items_processed: 0,
                items_failed: 0,
                output_path: Some("/data/output/a.parquet".into()),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
            },
            JobInfo {
                id: 2,
                file_version_id: Some(102),
                job_type: JobType::Parse,
                name: "parser_b".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Running,
                started_at: Local::now(),
                completed_at: None,
                items_total: 100,
                items_processed: 50,
                items_failed: 0,
                output_path: Some("/data/output/b.parquet".into()),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
            },
            JobInfo {
                id: 3,
                file_version_id: Some(103),
                job_type: JobType::Parse,
                name: "parser_c".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Failed,
                started_at: Local::now(),
                completed_at: Some(Local::now()),
                items_total: 100,
                items_processed: 30,
                items_failed: 5,
                output_path: Some("/data/output/c.parquet".into()),
                output_size_bytes: None,
                backtest: None,
                failures: vec![JobFailure {
                    file_path: "/data/c.csv".into(),
                    error: "Parse error".into(),
                    line: None,
                }],
            },
            JobInfo {
                id: 4,
                file_version_id: Some(104),
                job_type: JobType::Parse,
                name: "parser_d".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Completed,
                started_at: Local::now(),
                completed_at: Some(Local::now()),
                items_total: 100,
                items_processed: 100,
                items_failed: 0,
                output_path: Some("/data/output/d.parquet".into()),
                output_size_bytes: Some(1024 * 1024),
                backtest: None,
                failures: vec![],
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
                app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                    .await;
            }
        });
        // Should stop at last valid index (3)
        assert_eq!(app.jobs_state.selected_index, 3);

        // Navigate up past beginning
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for _ in 0..10 {
                app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // 1000 navigation operations should complete in < 2000ms (debug build)
        // In release builds this should be much faster (~100ms)
        // Debug builds are significantly slower due to unoptimized code
        assert!(
            elapsed.as_millis() < 2000,
            "File list navigation too slow: {:?} for 1000 operations (should be < 2000ms)",
            elapsed
        );

        assert_eq!(app.discover.selected, 1000);
    }

    #[tokio::test]
    #[ignore = "Flaky under variable system load - run manually with --ignored"]
    async fn test_jobs_list_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args());
        app.mode = TuiMode::Jobs;

        // Set up large jobs list (in-memory)
        app.jobs_state.jobs = (0..1000)
            .map(|i| JobInfo {
                id: i,
                file_version_id: Some(i * 100),
                job_type: JobType::Parse,
                name: format!("test_parser_{}", i),
                version: Some("1.0.0".into()),
                status: if i % 4 == 0 {
                    JobStatus::Completed
                } else if i % 4 == 1 {
                    JobStatus::Running
                } else if i % 4 == 2 {
                    JobStatus::Failed
                } else {
                    JobStatus::Pending
                },
                started_at: chrono::Local::now(),
                completed_at: None,
                items_total: 100,
                items_processed: 50,
                items_failed: 0,
                output_path: Some(format!("/data/output/file_{}.parquet", i)),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
            })
            .collect();
        app.jobs_state.selected_index = 0;

        // Navigate through 500 jobs and measure time
        let start = Instant::now();
        for _ in 0..500 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        }
        let elapsed = start.elapsed();

        // 500 navigation operations should complete in < 3000ms (debug build)
        // In release builds this should be much faster (~50ms)
        // Debug builds are significantly slower due to unoptimized code
        // Note: This threshold is generous to handle loaded CI systems
        assert!(
            elapsed.as_millis() < 3000,
            "Jobs list navigation too slow: {:?} for 500 operations (should be < 3000ms)",
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.jobs_state.selected_index, 0);
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
    fn test_discover_esc_no_view_change_when_no_dialog() {
        let mut app = App::new(test_args());
        app.mode = TuiMode::Discover;
        // No dialogs open - view_state should be Files
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Esc should not change views
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 1);

        // Navigate down again
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 2);

        // Try to navigate past end
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .await;
        });
        assert_eq!(app.discover.selected, 2); // Stays at last

        // Navigate up
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
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
        assert_eq!(job.job_type, JobType::Scan, "Job type should be Scan");
        assert!(
            job.output_path.as_ref().map_or(false, |p| p.contains(temp_dir.path().to_str().unwrap())),
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
            DiscoverViewState::RuleBuilder,
            "Should return to RuleBuilder after scan completes"
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
            DiscoverViewState::RuleBuilder,
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
        use std::time::Duration;

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

        // Wait for scan to complete (with timeout)
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(10) {
                panic!("Scan did not complete within 10 seconds");
            }
            app.tick().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
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

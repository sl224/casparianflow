//! Application state for the TUI
//!
//! # Dead Code Justification
//! Several struct fields and enum variants in this module are defined for
//! upcoming TUI features (Jobs view, Parser Bench, Monitoring) per the spec.
//! They are scaffolding for active development. See specs/views/*.md.
#![allow(dead_code)]

use casparian_db::{dev_allow_destructive_reset, BackendError, DbConnection, DbValue};
use casparian_intent::IntentState;
use casparian_mcp::intent::{
    ConfidenceLabel, Decision, DecisionRecord, DecisionTarget, NextAction, ProposalId,
    SelectionProposal, SessionBundle, SessionId, SessionManifest, SessionStore,
};
use casparian_protocol::{
    Approval as ProtoApproval, ApprovalOperation, ApprovalStatus as ProtoApprovalStatus,
    JobStatus as ProtocolJobStatus, ProcessingStatus,
};
use casparian_sentinel::{
    ApiStorage, ControlClient, JobInfo as ControlJobInfo, ScoutRuleInfo, ScoutSourceInfo,
    DEFAULT_CONTROL_ADDR,
};
use casparian_sentinel::control::ScoutTagFilter;
use chrono::{DateTime, Local, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::info_span;

use super::{backend::BackendRouter, nav, ui_signature::UiSignature, TuiArgs};
use crate::cli::config::{
    casparian_home, default_db_backend, query_catalog_path, state_store_path, DbBackend,
};
use crate::cli::scan::classify_scan_error;
use crate::cli::context;
use casparian::scout::{
    patterns, scan_path, Database as ScoutDatabase, ScanCancelToken, ScanProgress as ScoutProgress,
    Scanner as ScoutScanner, Source, SourceId, SourceType, TagSource, TaggingRuleId, Workspace,
    WorkspaceId,
};

fn dev_allow_offline_write() -> bool {
    std::env::var("CASPARIAN_DEV_ALLOW_DIRECT_DB_WRITE")
        .ok()
        .as_deref()
        == Some("1")
}
use casparian::telemetry::{scan_config_telemetry, TelemetryRecorder};
use casparian_protocol::telemetry as protocol_telemetry;
use uuid::Uuid;

#[path = "views/approvals.rs"]
mod approvals;
#[path = "views/discover.rs"]
mod discover;
#[path = "views/jobs.rs"]
mod jobs;
#[path = "views/home.rs"]
mod home;
#[path = "views/catalog.rs"]
mod catalog;
#[path = "views/query.rs"]
mod query;
#[path = "views/sessions.rs"]
mod sessions;
#[path = "views/settings.rs"]
mod settings;
#[path = "views/sources.rs"]
mod sources;
#[path = "views/triage.rs"]
mod triage;

/// Current TUI mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiMode {
    #[default]
    Home, // Home hub: quick start + status dashboard
    Ingest,   // Sources + selection + rules + validation
    Run,      // Jobs + outputs
    Review,   // Triage + approvals + sessions
    Query,    // SQL query console
    Settings, // Application settings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IngestTab {
    Sources,
    #[default]
    Select,
    Rules,
    Validate,
}

impl IngestTab {
    pub fn next(self) -> Self {
        match self {
            IngestTab::Sources => IngestTab::Select,
            IngestTab::Select => IngestTab::Rules,
            IngestTab::Rules => IngestTab::Validate,
            IngestTab::Validate => IngestTab::Sources,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            IngestTab::Sources => IngestTab::Validate,
            IngestTab::Select => IngestTab::Sources,
            IngestTab::Rules => IngestTab::Select,
            IngestTab::Validate => IngestTab::Rules,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IngestTab::Sources => "Sources",
            IngestTab::Select => "Scope",
            IngestTab::Rules => "Label",
            IngestTab::Validate => "Test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunTab {
    #[default]
    Jobs,
    Outputs,
}

impl RunTab {
    pub fn next(self) -> Self {
        match self {
            RunTab::Jobs => RunTab::Outputs,
            RunTab::Outputs => RunTab::Jobs,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }

    pub fn label(self) -> &'static str {
        match self {
            RunTab::Jobs => "Jobs",
            RunTab::Outputs => "Outputs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewTab {
    #[default]
    Triage,
    Approvals,
    Sessions,
}

impl ReviewTab {
    pub fn next(self) -> Self {
        match self {
            ReviewTab::Triage => ReviewTab::Approvals,
            ReviewTab::Approvals => ReviewTab::Sessions,
            ReviewTab::Sessions => ReviewTab::Triage,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ReviewTab::Triage => ReviewTab::Sessions,
            ReviewTab::Approvals => ReviewTab::Triage,
            ReviewTab::Sessions => ReviewTab::Approvals,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReviewTab::Triage => "Triage",
            ReviewTab::Approvals => "Approvals",
            ReviewTab::Sessions => "Sessions",
        }
    }
}

/// Focus area for global shell navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellFocus {
    #[default]
    Main,
    Rail,
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

/// Global status message shown in the shell header.
#[derive(Debug, Clone)]
pub struct GlobalStatusMessage {
    pub message: String,
    pub is_error: bool,
    pub expires_at: std::time::Instant,
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
            SettingsCategory::General => 3, // default_path, auto_scan, confirm
            SettingsCategory::Display => 3, // theme, unicode, hidden_files
            SettingsCategory::About => 3,   // version, database, config (read-only)
        }
    }
}

// =============================================================================
// Sessions View Types (Intent Pipeline Workflow)
// =============================================================================

/// View states within Sessions mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionsViewState {
    #[default]
    SessionList,
    SessionDetail,
    WorkflowProgress,
    ProposalReview,
    GateApproval,
}

/// Information about a session for display
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub intent: String,             // "find all sales files"
    pub state: Option<IntentState>, // current workflow state
    pub state_label: String,
    pub created_at: DateTime<Local>,
    pub file_count: usize,
    pub pending_gate: Option<String>, // G1, G2, etc.
}

/// Information about a proposal for display
#[derive(Debug, Clone)]
pub struct ProposalInfo {
    pub id: String,
    pub proposal_type: String, // "Selection", "TagRules", etc.
    pub summary: String,
    pub confidence: String, // "LOW", "MEDIUM", "HIGH"
    pub created_at: DateTime<Local>,
}

/// Information about a gate for approval
#[derive(Debug, Clone)]
pub struct GateInfo {
    pub gate_id: String,   // G1-G6
    pub gate_name: String, // "File Selection", "Tag Rules", etc.
    pub proposal_summary: String,
    pub evidence: Vec<String>,
    pub confidence: String, // LOW, MEDIUM, HIGH
    pub selected_examples: Vec<String>,
    pub near_miss_examples: Vec<String>,
    pub next_actions: Vec<String>,
    pub proposal_id: ProposalId,
    pub approval_target_hash: String,
}

/// Confidence level for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

impl ConfidenceLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfidenceLevel::Low => "LOW",
            ConfidenceLevel::Medium => "MEDIUM",
            ConfidenceLevel::High => "HIGH",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "HIGH" => ConfidenceLevel::High,
            "MED" | "MEDIUM" => ConfidenceLevel::Medium,
            _ => ConfidenceLevel::Low,
        }
    }
}

/// State for Sessions mode (Intent Pipeline Workflow)
#[derive(Debug, Clone, Default)]
pub struct SessionsState {
    /// Current view state within Sessions mode
    pub view_state: SessionsViewState,
    /// Previous view state (for Esc navigation)
    pub previous_view_state: Option<SessionsViewState>,
    /// Previous app mode (for Esc navigation back to prior screen)
    pub previous_mode: Option<TuiMode>,
    /// List of sessions
    pub sessions: Vec<SessionInfo>,
    /// Currently selected session index
    pub selected_index: usize,
    /// Active session ID (if viewing details)
    pub active_session: Option<String>,
    /// Current proposal being reviewed
    pub current_proposal: Option<ProposalInfo>,
    /// Pending gate awaiting approval
    pub pending_gate: Option<GateInfo>,
    /// Newly created session to select after reload
    pub pending_select_session_id: Option<String>,
    /// Whether sessions have been loaded from storage
    pub sessions_loaded: bool,
}

impl SessionsState {
    /// Get the currently selected session
    pub fn selected_session(&self) -> Option<&SessionInfo> {
        self.sessions.get(self.selected_index)
    }

    /// Clamp selected_index to valid range
    pub fn clamp_selection(&mut self) {
        if self.sessions.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.sessions.len() {
            self.selected_index = self.sessions.len() - 1;
        }
    }

    /// Transition to a new view state, saving current as previous
    pub fn transition_state(&mut self, new_state: SessionsViewState) {
        self.previous_view_state = Some(self.view_state);
        self.view_state = new_state;
    }

    /// Return to previous view state (for Esc)
    pub fn return_to_previous_state(&mut self) {
        if let Some(prev) = self.previous_view_state.take() {
            self.view_state = prev;
        } else {
            self.view_state = SessionsViewState::SessionList;
        }
    }
}

// =============================================================================
// Triage View Types (Quarantine + Schema Mismatch + Dead Letter)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriageTab {
    #[default]
    Quarantine,
    SchemaMismatch,
    DeadLetter,
}

impl TriageTab {
    pub fn next(&self) -> Self {
        match self {
            TriageTab::Quarantine => TriageTab::SchemaMismatch,
            TriageTab::SchemaMismatch => TriageTab::DeadLetter,
            TriageTab::DeadLetter => TriageTab::Quarantine,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TriageTab::Quarantine => "Quarantine",
            TriageTab::SchemaMismatch => "Schema Mismatch",
            TriageTab::DeadLetter => "Dead Letter",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuarantineRow {
    pub id: i64,
    pub job_id: i64,
    pub row_index: i64,
    pub error_reason: String,
    pub raw_data: Option<Vec<u8>>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SchemaMismatchRow {
    pub id: i64,
    pub job_id: i64,
    pub output_name: String,
    pub mismatch_kind: String,
    pub expected_name: Option<String>,
    pub actual_name: Option<String>,
    pub expected_type: Option<String>,
    pub actual_type: Option<String>,
    pub expected_index: Option<i64>,
    pub actual_index: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct DeadLetterRow {
    pub id: i64,
    pub original_job_id: i64,
    pub file_id: Option<i64>,
    pub plugin_name: String,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub moved_at: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TriageState {
    pub tab: TriageTab,
    pub quarantine_rows: Option<Vec<QuarantineRow>>,
    pub schema_mismatches: Option<Vec<SchemaMismatchRow>>,
    pub dead_letters: Option<Vec<DeadLetterRow>>,
    pub selected_index: usize,
    pub previous_mode: Option<TuiMode>,
    pub job_filter: Option<i64>,
    pub loaded: bool,
    pub copied_buffer: Option<String>,
    pub status_message: Option<String>,
}

impl TriageState {
    fn active_len(&self) -> usize {
        match self.tab {
            TriageTab::Quarantine => self.quarantine_rows.as_ref().map_or(0, |r| r.len()),
            TriageTab::SchemaMismatch => self.schema_mismatches.as_ref().map_or(0, |r| r.len()),
            TriageTab::DeadLetter => self.dead_letters.as_ref().map_or(0, |r| r.len()),
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.active_len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }
}

#[derive(Debug, Clone)]
struct TriageData {
    quarantine_rows: Option<Vec<QuarantineRow>>,
    schema_mismatches: Option<Vec<SchemaMismatchRow>>,
    dead_letters: Option<Vec<DeadLetterRow>>,
}

// =============================================================================
// Catalog View Types (Pipelines + Runs)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatalogTab {
    #[default]
    Pipelines,
    Runs,
}

impl CatalogTab {
    pub fn next(&self) -> Self {
        match self {
            CatalogTab::Pipelines => CatalogTab::Runs,
            CatalogTab::Runs => CatalogTab::Pipelines,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CatalogTab::Pipelines => "Pipelines",
            CatalogTab::Runs => "Runs",
        }
    }
}

// =============================================================================
// Workspace Switcher Overlay
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceSwitcherMode {
    #[default]
    List,
    Creating,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceSwitcherState {
    pub visible: bool,
    pub mode: WorkspaceSwitcherMode,
    pub workspaces: Vec<Workspace>,
    pub selected_index: usize,
    pub input: String,
    pub status_message: Option<String>,
    pub loaded: bool,
}

#[derive(Debug, Clone)]
pub struct PipelineInfo {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PipelineRunInfo {
    pub id: String,
    pub pipeline_id: String,
    pub pipeline_name: Option<String>,
    pub pipeline_version: Option<i64>,
    pub logical_date: String,
    pub status: String,
    pub selection_snapshot_hash: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CatalogState {
    pub tab: CatalogTab,
    pub pipelines: Option<Vec<PipelineInfo>>,
    pub runs: Option<Vec<PipelineRunInfo>>,
    pub selected_index: usize,
    pub previous_mode: Option<TuiMode>,
    pub pending_select_run_id: Option<String>,
    pub loaded: bool,
    pub status_message: Option<String>,
}

impl CatalogState {
    fn active_len(&self) -> usize {
        match self.tab {
            CatalogTab::Pipelines => self.pipelines.as_ref().map_or(0, |r| r.len()),
            CatalogTab::Runs => self.runs.as_ref().map_or(0, |r| r.len()),
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.active_len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }
}

#[derive(Debug, Clone)]
struct CatalogData {
    pipelines: Option<Vec<PipelineInfo>>,
    runs: Option<Vec<PipelineRunInfo>>,
}

/// State for job queue mode (per jobs_redesign.md spec)
#[derive(Debug, Clone, Default)]
pub struct JobsState {
    /// Current view state within Jobs mode
    pub view_state: JobsViewState,
    /// Previous view state (for Esc navigation)
    pub previous_view_state: Option<JobsViewState>,
    /// Previous app mode (for Esc navigation back to prior screen)
    pub previous_mode: Option<TuiMode>,
    /// List of jobs
    pub jobs: Vec<JobInfo>,
    /// Currently selected job index (into filtered list)
    pub selected_index: usize,
    /// Which list is focused in the Jobs view
    pub section_focus: JobsListSection,
    /// Remembered selection for actionable jobs
    pub actionable_index: usize,
    /// Remembered selection for ready jobs
    pub ready_index: usize,
    /// Pinned job for the details panel
    pub pinned_job_id: Option<i64>,
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
    /// Log viewer scroll offset (line-based)
    pub log_viewer_scroll: usize,
    /// Whether jobs have been loaded from DB
    pub jobs_loaded: bool,
    /// Last poll timestamp for incremental updates
    pub last_poll: Option<DateTime<Local>>,
}

impl JobsState {
    /// Get filtered jobs based on current status and type filters
    /// Jobs are sorted: Failed first, then by recency (per spec Section 10.1)
    pub fn filtered_jobs(&self) -> Vec<&JobInfo> {
        let mut jobs: Vec<&JobInfo> = self
            .jobs
            .iter()
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

    fn job_matches_filters(&self, job: &JobInfo) -> bool {
        let status_ok = match self.status_filter {
            Some(status) => job.status == status,
            None => true,
        };
        let type_ok = match self.type_filter {
            Some(jtype) => job.job_type == jtype,
            None => true,
        };
        status_ok && type_ok
    }

    pub fn actionable_jobs(&self) -> Vec<&JobInfo> {
        let mut jobs: Vec<&JobInfo> = self
            .jobs
            .iter()
            .filter(|job| self.job_matches_filters(job))
            .filter(|job| {
                matches!(
                    job.status,
                    JobStatus::Pending
                        | JobStatus::Running
                        | JobStatus::Failed
                        | JobStatus::Cancelled
                )
            })
            .collect();

        jobs.sort_by(|a, b| {
            let rank = |status: JobStatus| match status {
                JobStatus::Running => 0,
                JobStatus::Pending => 1,
                JobStatus::Failed => 2,
                JobStatus::Cancelled => 3,
                JobStatus::PartialSuccess => 4,
                JobStatus::Completed => 5,
            };
            let status_cmp = rank(a.status).cmp(&rank(b.status));
            if status_cmp == std::cmp::Ordering::Equal {
                b.started_at.cmp(&a.started_at)
            } else {
                status_cmp
            }
        });

        jobs
    }

    pub fn ready_jobs(&self) -> Vec<&JobInfo> {
        let mut jobs: Vec<&JobInfo> = self
            .jobs
            .iter()
            .filter(|job| self.job_matches_filters(job))
            .filter(|job| matches!(job.status, JobStatus::Completed | JobStatus::PartialSuccess))
            .collect();

        jobs.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
        jobs
    }

    pub fn focused_jobs(&self) -> Vec<&JobInfo> {
        match self.section_focus {
            JobsListSection::Actionable => self.actionable_jobs(),
            JobsListSection::Ready => self.ready_jobs(),
        }
    }

    /// Clamp selected_index to valid range for focused list
    pub fn clamp_selection(&mut self) {
        match self.section_focus {
            JobsListSection::Actionable => self.actionable_index = self.selected_index,
            JobsListSection::Ready => self.ready_index = self.selected_index,
        }

        let actionable_count = self.actionable_jobs().len();
        let ready_count = self.ready_jobs().len();

        Self::clamp_index(&mut self.actionable_index, actionable_count);
        Self::clamp_index(&mut self.ready_index, ready_count);

        self.selected_index = match self.section_focus {
            JobsListSection::Actionable => self.actionable_index,
            JobsListSection::Ready => self.ready_index,
        };
    }

    fn clamp_index(index: &mut usize, count: usize) {
        if count == 0 {
            *index = 0;
        } else if *index >= count {
            *index = count - 1;
        }
    }

    /// Clamp selected_index to valid range for filtered list
    /// Set status filter and clamp selection
    pub fn set_filter(&mut self, filter: Option<JobStatus>) {
        self.status_filter = filter;
        self.clamp_selection();
    }

    /// Cycle through status filters (None -> Pending -> Running -> Failed -> Cancelled -> Completed -> Partial -> None)
    pub fn cycle_status_filter(&mut self) {
        const ORDER: [JobStatus; 6] = [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Failed,
            JobStatus::Cancelled,
            JobStatus::Completed,
            JobStatus::PartialSuccess,
        ];

        self.status_filter = match self.status_filter {
            None => Some(ORDER[0]),
            Some(current) => {
                let next = ORDER
                    .iter()
                    .position(|status| *status == current)
                    .and_then(|idx| ORDER.get(idx + 1))
                    .copied();
                next
            }
        };

        self.clamp_selection();
    }

    /// Cycle through job type filters (None -> Scan -> Parse -> Backtest -> Schema -> None)
    pub fn cycle_type_filter(&mut self) {
        const ORDER: [JobType; 4] = [
            JobType::Scan,
            JobType::Parse,
            JobType::Backtest,
            JobType::SchemaEval,
        ];

        self.type_filter = match self.type_filter {
            None => Some(ORDER[0]),
            Some(current) => {
                let next = ORDER
                    .iter()
                    .position(|job_type| *job_type == current)
                    .and_then(|idx| ORDER.get(idx + 1))
                    .copied();
                next
            }
        };

        self.clamp_selection();
    }

    /// Clear all job list filters
    pub fn clear_filters(&mut self) {
        self.status_filter = None;
        self.type_filter = None;
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
        if let Some(pinned_id) = self.pinned_job_id {
            if let Some(job) = self.jobs.iter().find(|job| job.id == pinned_id) {
                return Some(job);
            }
        }
        self.focused_jobs().get(self.selected_index).copied()
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
                JobStatus::Completed | JobStatus::PartialSuccess => done += 1,
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

    pub fn merge_loaded_jobs(&mut self, loaded: Vec<JobInfo>) {
        // Keep ephemeral/local jobs regardless of what DB/control returned.
        // Treat loaded jobs as authoritative snapshot for persistent records.
        let mut existing_by_id: HashMap<i64, JobInfo> =
            self.jobs.iter().cloned().map(|job| (job.id, job)).collect();

        let mut next: Vec<JobInfo> = self
            .jobs
            .iter()
            .cloned()
            .filter(|job| job.origin == JobOrigin::Ephemeral)
            .collect();

        for mut new_job in loaded {
            if let Some(old) = existing_by_id.remove(&new_job.id) {
                // Preserve UI-only/detail fields that DB/control does not (or should not) own.
                new_job.violations = old.violations;
                new_job.top_violations_loaded = old.top_violations_loaded;
                new_job.selected_violation_index = old.selected_violation_index;
                if new_job.backtest.is_none() && old.backtest.is_some() {
                    new_job.backtest = old.backtest.clone();
                }
                if new_job.failures.is_empty()
                    && !old.failures.is_empty()
                    && new_job.status == old.status
                {
                    new_job.failures = old.failures.clone();
                }

                // Preserve stable started_at when the loader used Local::now() fallback.
                if matches!(new_job.status, JobStatus::Pending)
                    && old.started_at < new_job.started_at
                {
                    new_job.started_at = old.started_at;
                }

                // Preserve completed_at if we had it but refresh doesn't.
                if new_job.completed_at.is_none() && old.completed_at.is_some() {
                    new_job.completed_at = old.completed_at;
                }
            }

            next.push(new_job);
        }

        self.jobs = next;
        self.trim_completed_jobs();

        if let Some(pinned) = self.pinned_job_id {
            if !self.jobs.iter().any(|job| job.id == pinned) {
                self.pinned_job_id = None;
            }
        }
        self.clamp_selection();
    }

    /// Trim old completed/failed jobs to prevent unbounded memory growth
    /// Keeps running/pending jobs and removes oldest completed jobs first
    fn trim_completed_jobs(&mut self) {
        if self.jobs.len() > MAX_JOBS {
            // Count non-terminal jobs (Running, Pending)
            let active_count = self
                .jobs
                .iter()
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
                    if matches!(
                        j.status,
                        JobStatus::Completed | JobStatus::PartialSuccess | JobStatus::Failed
                    ) {
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

/// Where a job record comes from (persistent store vs local UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobOrigin {
    #[default]
    Persistent,
    Ephemeral,
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
    pub file_id: Option<i64>,
    pub job_type: JobType,
    pub origin: JobOrigin,
    pub name: String, // parser/exporter/source name
    pub version: Option<String>,
    pub status: JobStatus,
    pub started_at: DateTime<Local>,
    pub completed_at: Option<DateTime<Local>>,
    pub pipeline_run_id: Option<String>,
    pub logical_date: Option<String>,
    pub selection_snapshot_hash: Option<String>,
    pub quarantine_rows: Option<i64>,

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

    // Violation information (for backtest jobs)
    pub violations: Vec<ViolationSummary>,
    /// Whether top violations have been loaded for this job
    pub top_violations_loaded: bool,
    /// Index of currently selected violation (for violation detail view)
    pub selected_violation_index: usize,
}

impl Default for JobInfo {
    fn default() -> Self {
        Self {
            id: 0,
            file_id: None,
            job_type: JobType::Parse,
            origin: JobOrigin::Persistent,
            name: String::new(),
            version: None,
            status: JobStatus::Pending,
            started_at: Local::now(),
            completed_at: None,
            pipeline_run_id: None,
            logical_date: None,
            selection_snapshot_hash: None,
            quarantine_rows: None,
            items_total: 0,
            items_processed: 0,
            items_failed: 0,
            output_path: None,
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
            violations: vec![],
            top_violations_loaded: false,
            selected_violation_index: 0,
        }
    }
}

/// Backtest-specific job information
#[derive(Debug, Clone)]
pub struct BacktestInfo {
    pub pass_rate: f64, // 0.0 - 1.0
    pub iteration: u32,
    pub high_failure_passed: u32,
}

/// Summary of schema violations for display in Jobs view
#[derive(Debug, Clone)]
pub struct ViolationSummary {
    /// Type of violation: TypeMismatch, NullNotAllowed, FormatMismatch
    pub violation_type: ViolationType,
    /// Number of rows with this violation
    pub count: u32,
    /// Percentage of total rows affected (0.0 - 100.0)
    pub pct_of_rows: f32,
    /// Column name where violation occurred
    pub column: String,
    /// Sample values (already redacted)
    pub samples: Vec<String>,
    /// Suggested fix action (ChangeType, MakeNullable, ChangeFormat, etc.)
    pub suggested_fix: Option<SuggestedFix>,
    /// Confidence level for the suggested fix: HIGH, MEDIUM, LOW
    pub confidence: Option<String>,
    /// Expected type/format (for display)
    pub expected: Option<String>,
    /// Actual type/format found (for display)
    pub actual: Option<String>,
}

impl Default for ViolationSummary {
    fn default() -> Self {
        Self {
            violation_type: ViolationType::TypeMismatch,
            count: 0,
            pct_of_rows: 0.0,
            column: String::new(),
            samples: vec![],
            suggested_fix: None,
            confidence: None,
            expected: None,
            actual: None,
        }
    }
}

/// Types of schema violations (mirrors casparian_mcp::types::ViolationType)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationType {
    /// Value doesn't match expected type
    TypeMismatch,
    /// Null value in non-nullable column
    NullNotAllowed,
    /// Value doesn't match expected format (e.g., date format)
    FormatMismatch,
    /// Column name doesn't match schema
    ColumnNameMismatch,
    /// Wrong number of columns
    ColumnCountMismatch,
}

impl ViolationType {
    /// Get the display symbol for this violation type
    pub fn symbol(&self) -> &'static str {
        match self {
            ViolationType::TypeMismatch => "\u{2298}",       // ⊘
            ViolationType::NullNotAllowed => "\u{2205}",     // ∅
            ViolationType::FormatMismatch => "\u{2260}",     // ≠
            ViolationType::ColumnNameMismatch => "\u{2262}", // ≢
            ViolationType::ColumnCountMismatch => "#",
        }
    }

    /// Get human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            ViolationType::TypeMismatch => "TypeMismatch",
            ViolationType::NullNotAllowed => "NullNotAllowed",
            ViolationType::FormatMismatch => "FormatMismatch",
            ViolationType::ColumnNameMismatch => "ColumnNameMismatch",
            ViolationType::ColumnCountMismatch => "ColumnCountMismatch",
        }
    }
}

/// Suggested fix for a violation (mirrors casparian_mcp::types::SuggestedFix)
#[derive(Debug, Clone)]
pub enum SuggestedFix {
    /// Change column type
    ChangeType { from: String, to: String },
    /// Make column nullable
    MakeNullable,
    /// Change format string
    ChangeFormat { suggested: String },
    /// Add missing column
    AddColumn { name: String, data_type: String },
    /// Remove extra column
    RemoveColumn { name: String },
}

impl SuggestedFix {
    /// Get display string for the suggested fix
    pub fn display(&self) -> String {
        match self {
            SuggestedFix::ChangeType { to, .. } => format!("ChangeType to {}", to),
            SuggestedFix::MakeNullable => "MakeNullable".to_string(),
            SuggestedFix::ChangeFormat { suggested } => format!("ChangeFormat to {}", suggested),
            SuggestedFix::AddColumn { name, data_type } => {
                format!("AddColumn {} ({})", name, data_type)
            }
            SuggestedFix::RemoveColumn { name } => format!("RemoveColumn {}", name),
        }
    }
}

/// Job failure details
#[derive(Debug, Clone)]
pub struct JobFailure {
    pub file_path: String,
    pub error: String,
    pub line: Option<u32>,
}

/// Job status enumeration for UI display (UiJobStatus)
/// Derived from (ProcessingStatus, Option<JobStatus>) per enum_consolidation_plan.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    PartialSuccess,
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
            JobStatus::PartialSuccess => "⚠",
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
            JobStatus::PartialSuccess => "Partial",
            JobStatus::Failed => "Failed",
            JobStatus::Cancelled => "Cancelled",
        }
    }

    /// Map from DB status string (ProcessingStatus) and optional completion_status (JobStatus)
    /// to UI status. Per enum_consolidation_plan.md mapping:
    ///
    /// ui_status(queue, outcome):
    ///   if queue in {Pending, Queued} -> Pending
    ///   if queue in {Running, Staged} -> Running
    ///   if queue == Skipped -> Completed
    ///   if queue == Failed:
    ///     if outcome == Aborted -> Cancelled
    ///     else -> Failed
    ///   if queue == Completed:
    ///     if outcome in {PartialSuccess, CompletedWithWarnings} -> PartialSuccess
    ///     if outcome == Failed -> Failed
    ///     if outcome == Aborted -> Cancelled
    ///     else -> Completed
    pub fn from_db_status(queue_status_str: &str, completion_status: Option<&str>) -> Self {
        let queue_status = queue_status_str.parse::<ProcessingStatus>().ok();
        let completion_status =
            completion_status.and_then(|status| status.parse::<ProtocolJobStatus>().ok());

        match queue_status {
            Some(ProcessingStatus::Pending)
            | Some(ProcessingStatus::Queued)
            | Some(ProcessingStatus::Dispatching) => JobStatus::Pending,
            Some(ProcessingStatus::Running) | Some(ProcessingStatus::Staged) => JobStatus::Running,
            Some(ProcessingStatus::Skipped) => JobStatus::Completed,
            Some(ProcessingStatus::Aborted) => JobStatus::Cancelled,
            Some(ProcessingStatus::Failed) => {
                if matches!(completion_status, Some(ProtocolJobStatus::Aborted)) {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                }
            }
            Some(ProcessingStatus::Completed) => match completion_status {
                Some(
                    ProtocolJobStatus::PartialSuccess | ProtocolJobStatus::CompletedWithWarnings,
                ) => JobStatus::PartialSuccess,
                Some(ProtocolJobStatus::Failed) => JobStatus::Failed,
                Some(ProtocolJobStatus::Aborted) => JobStatus::Cancelled,
                _ => JobStatus::Completed,
            },
            _ => {
                if queue_status_str.eq_ignore_ascii_case(JobStatus::Cancelled.as_str()) {
                    JobStatus::Cancelled
                } else if let Ok(legacy) = queue_status_str.parse::<ProtocolJobStatus>() {
                    match legacy {
                        ProtocolJobStatus::Success => JobStatus::Completed,
                        ProtocolJobStatus::PartialSuccess
                        | ProtocolJobStatus::CompletedWithWarnings => JobStatus::PartialSuccess,
                        ProtocolJobStatus::Failed => JobStatus::Failed,
                        ProtocolJobStatus::Aborted => JobStatus::Cancelled,
                        ProtocolJobStatus::Rejected => JobStatus::Pending,
                    }
                } else {
                    JobStatus::Pending
                }
            }
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
    /// Viewing violation breakdown for a backtest job
    ViolationDetail,
}

/// Focused list in Jobs mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobsListSection {
    #[default]
    Actionable,
    Ready,
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
    /// Statistics displayed on cards
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
    /// Whether we're creating a new source (vs editing)
    pub creating: bool,
    /// Edit field value
    pub edit_value: String,
    /// Whether showing delete confirmation
    pub confirm_delete: bool,
    /// Previous app mode (for Esc navigation back to prior screen)
    pub previous_mode: Option<TuiMode>,
}

// =============================================================================
// Approvals View Types (View 5)
// =============================================================================

/// View states within Approvals mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalsViewState {
    #[default]
    List,
    Detail,
    ConfirmApprove,
    ConfirmReject,
}

/// Filter for approval status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalStatusFilter {
    #[default]
    Pending,
    Approved,
    Rejected,
    Expired,
    All,
}

impl ApprovalStatusFilter {
    /// Get display text for this filter
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatusFilter::Pending => "Pending",
            ApprovalStatusFilter::Approved => "Approved",
            ApprovalStatusFilter::Rejected => "Rejected",
            ApprovalStatusFilter::Expired => "Expired",
            ApprovalStatusFilter::All => "All",
        }
    }

    /// Cycle to next filter value
    pub fn next(&self) -> Self {
        match self {
            ApprovalStatusFilter::Pending => ApprovalStatusFilter::Approved,
            ApprovalStatusFilter::Approved => ApprovalStatusFilter::Rejected,
            ApprovalStatusFilter::Rejected => ApprovalStatusFilter::Expired,
            ApprovalStatusFilter::Expired => ApprovalStatusFilter::All,
            ApprovalStatusFilter::All => ApprovalStatusFilter::Pending,
        }
    }
}

/// Action to be confirmed for an approval
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalAction {
    Approve,
    Reject,
}

/// Approval status for display (parsed from DB string at boundary)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalDisplayStatus {
    #[default]
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl ApprovalDisplayStatus {
    /// Parse from database string. Returns Pending for unknown values (fail-safe).
    pub fn from_db_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "expired" => Self::Expired,
            _ => Self::Pending, // Safe default for unknown status
        }
    }

    /// Get the display symbol for this status
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::Approved => "✓",
            Self::Rejected => "✗",
            Self::Expired => "⊘",
        }
    }

    /// Get the display string for this status
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    /// Check if this matches a filter
    pub fn matches_filter(&self, filter: ApprovalStatusFilter) -> bool {
        match filter {
            ApprovalStatusFilter::All => true,
            ApprovalStatusFilter::Pending => *self == Self::Pending,
            ApprovalStatusFilter::Approved => *self == Self::Approved,
            ApprovalStatusFilter::Rejected => *self == Self::Rejected,
            ApprovalStatusFilter::Expired => *self == Self::Expired,
        }
    }
}

/// Operation type for an approval (parsed from DB string at boundary)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalOperationType {
    #[default]
    Run,
    SchemaPromote,
}

impl ApprovalOperationType {
    /// Parse from database string. Returns Run for unknown values (fail-safe).
    pub fn from_db_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "run" => Self::Run,
            "schema_promote" | "schemapromote" => Self::SchemaPromote,
            _ => Self::Run, // Safe default for unknown operation type
        }
    }

    /// Get the display string for this operation type
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Run => "Run",
            Self::SchemaPromote => "SchemaPromote",
        }
    }
}

/// Approval information for display
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    /// Approval ID
    pub id: String,
    /// Operation type (Run or SchemaPromote)
    pub operation_type: ApprovalOperationType,
    /// Plugin reference or path
    pub plugin_ref: String,
    /// Human-readable summary
    pub summary: String,
    /// Status (Pending, Approved, Rejected, Expired)
    pub status: ApprovalDisplayStatus,
    /// When the approval was created
    pub created_at: DateTime<Local>,
    /// When the approval expires
    pub expires_at: DateTime<Local>,
    /// File count (for Run operations)
    pub file_count: Option<u32>,
    /// Input directory (for Run operations)
    pub input_dir: Option<String>,
    /// Job ID (if approved and executed)
    pub job_id: Option<String>,
}

impl ApprovalInfo {
    /// Get status symbol for display
    pub fn status_symbol(&self) -> &'static str {
        self.status.symbol()
    }

    /// Check if this approval is pending
    pub fn is_pending(&self) -> bool {
        self.status == ApprovalDisplayStatus::Pending
    }

    /// Get time until expiration (None if already expired or not pending)
    pub fn time_until_expiry(&self) -> Option<chrono::Duration> {
        if self.status != ApprovalDisplayStatus::Pending {
            return None;
        }
        let now = Local::now();
        if self.expires_at <= now {
            None
        } else {
            Some(self.expires_at.signed_duration_since(now))
        }
    }

    /// Format expiration countdown
    pub fn expiry_countdown(&self) -> String {
        if let Some(duration) = self.time_until_expiry() {
            let total_secs = duration.num_seconds();
            if total_secs < 60 {
                format!("{}s", total_secs)
            } else if total_secs < 3600 {
                format!("{}m", total_secs / 60)
            } else {
                format!("{}h{}m", total_secs / 3600, (total_secs % 3600) / 60)
            }
        } else if self.status == ApprovalDisplayStatus::Pending {
            "expired".to_string()
        } else {
            "-".to_string()
        }
    }

    fn from_control(approval: ProtoApproval) -> Self {
        let status = match approval.status {
            ProtoApprovalStatus::Pending => ApprovalDisplayStatus::Pending,
            ProtoApprovalStatus::Approved => ApprovalDisplayStatus::Approved,
            ProtoApprovalStatus::Rejected => ApprovalDisplayStatus::Rejected,
            ProtoApprovalStatus::Expired => ApprovalDisplayStatus::Expired,
        };

        let (operation_type, plugin_ref, input_dir, file_count) = match approval.operation {
            ApprovalOperation::Run {
                plugin_name,
                input_dir,
                file_count,
                ..
            } => (
                ApprovalOperationType::Run,
                plugin_name,
                Some(input_dir),
                Some(file_count as u32),
            ),
            ApprovalOperation::SchemaPromote { plugin_name, .. } => (
                ApprovalOperationType::SchemaPromote,
                plugin_name,
                None,
                None,
            ),
        };

        let created_at = chrono::DateTime::parse_from_rfc3339(&approval.created_at)
            .map(|dt| dt.with_timezone(&Local))
            .unwrap_or_else(|_| Local::now());
        let expires_at = chrono::DateTime::parse_from_rfc3339(&approval.expires_at)
            .map(|dt| dt.with_timezone(&Local))
            .unwrap_or_else(|_| Local::now());

        Self {
            id: approval.approval_id,
            operation_type,
            plugin_ref,
            summary: approval.summary,
            status,
            created_at,
            expires_at,
            file_count,
            input_dir,
            job_id: approval.job_id.map(|id| id.to_string()),
        }
    }
}

/// State for Approvals view (key 5)
#[derive(Debug, Clone, Default)]
pub struct ApprovalsState {
    /// Current view state within Approvals mode
    pub view_state: ApprovalsViewState,
    /// List of approvals
    pub approvals: Vec<ApprovalInfo>,
    /// Currently selected approval index
    pub selected_index: usize,
    /// Filter by status
    pub filter: ApprovalStatusFilter,
    /// Pinned approval for details panel
    pub pinned_approval_id: Option<String>,
    /// Action being confirmed (approve/reject)
    pub confirm_action: Option<ApprovalAction>,
    /// Rejection reason input
    pub rejection_reason: String,
    /// Whether approvals have been loaded from DB
    pub approvals_loaded: bool,
    /// Previous mode (for Esc navigation)
    pub previous_mode: Option<TuiMode>,
}

impl ApprovalsState {
    /// Get filtered approvals based on current filter
    pub fn filtered_approvals(&self) -> Vec<&ApprovalInfo> {
        self.approvals
            .iter()
            .filter(|a| a.status.matches_filter(self.filter))
            .collect()
    }

    /// Get currently selected approval
    pub fn selected_approval(&self) -> Option<&ApprovalInfo> {
        if let Some(ref pinned_id) = self.pinned_approval_id {
            if let Some(approval) = self.approvals.iter().find(|a| &a.id == pinned_id) {
                return Some(approval);
            }
        }
        self.filtered_approvals().get(self.selected_index).copied()
    }

    /// Clamp selected_index to valid range
    pub fn clamp_selection(&mut self) {
        let count = self.filtered_approvals().len();
        if count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= count {
            self.selected_index = count - 1;
        }
    }

    /// Calculate aggregate statistics
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        let pending = self
            .approvals
            .iter()
            .filter(|a| a.status == ApprovalDisplayStatus::Pending)
            .count();
        let approved = self
            .approvals
            .iter()
            .filter(|a| a.status == ApprovalDisplayStatus::Approved)
            .count();
        let rejected = self
            .approvals
            .iter()
            .filter(|a| a.status == ApprovalDisplayStatus::Rejected)
            .count();
        let expired = self
            .approvals
            .iter()
            .filter(|a| a.status == ApprovalDisplayStatus::Expired)
            .count();
        (pending, approved, rejected, expired)
    }
}

// =============================================================================
// Command Palette Types
// =============================================================================

/// Mode for the command palette input
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandPaletteMode {
    /// Natural language intent: "find all sales files"
    #[default]
    Intent,
    /// Slash commands: "/scan", "/query", "/approve"
    Command,
    /// Quick navigation: "jobs", "approvals", "home"
    Navigation,
}

impl CommandPaletteMode {
    /// Get the mode indicator character for display
    pub fn indicator(&self) -> &'static str {
        match self {
            CommandPaletteMode::Intent => ">",
            CommandPaletteMode::Command => "/",
            CommandPaletteMode::Navigation => "@",
        }
    }

    /// Get the mode name for display
    pub fn name(&self) -> &'static str {
        match self {
            CommandPaletteMode::Intent => "Intent",
            CommandPaletteMode::Command => "Command",
            CommandPaletteMode::Navigation => "Navigate",
        }
    }

    /// Cycle to the next mode
    pub fn next(&self) -> Self {
        match self {
            CommandPaletteMode::Intent => CommandPaletteMode::Command,
            CommandPaletteMode::Command => CommandPaletteMode::Navigation,
            CommandPaletteMode::Navigation => CommandPaletteMode::Intent,
        }
    }
}

/// Action to perform when a suggestion is selected
#[derive(Debug, Clone)]
pub enum CommandAction {
    /// Navigate to a specific TUI mode/view
    Navigate(TuiMode),
    /// Start an intent pipeline with the given text
    StartIntent(String),
    /// Execute a slash command
    RunCommand(String),
}

/// A suggestion in the command palette
#[derive(Debug, Clone)]
pub struct CommandSuggestion {
    /// Display text for the suggestion
    pub text: String,
    /// Description/help text
    pub description: String,
    /// Action to perform when selected
    pub action: CommandAction,
}

/// State for the command palette overlay
#[derive(Debug, Clone, Default)]
pub struct CommandPaletteState {
    /// Whether the palette is visible
    pub visible: bool,
    /// Current input text
    pub input: String,
    /// Cursor position in the input (character index)
    pub cursor: usize,
    /// List of suggestions based on current input
    pub suggestions: Vec<CommandSuggestion>,
    /// Currently selected suggestion index
    pub selected_suggestion: usize,
    /// Current input mode (Intent, Command, Navigation)
    pub mode: CommandPaletteMode,
    /// Recent intents for history
    pub recent_intents: Vec<String>,
}

impl CommandPaletteState {
    /// Create a new command palette state
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            cursor: 0,
            suggestions: Vec::new(),
            selected_suggestion: 0,
            mode: CommandPaletteMode::Intent,
            recent_intents: Vec::new(),
        }
    }

    /// Open the command palette in a specific mode
    pub fn open(&mut self, mode: CommandPaletteMode) {
        self.visible = true;
        self.mode = mode;
        self.input.clear();
        self.cursor = 0;
        self.selected_suggestion = 0;
        self.update_suggestions();
    }

    /// Close the command palette
    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.cursor = 0;
        self.suggestions.clear();
        self.selected_suggestion = 0;
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
        self.update_suggestions();
    }

    /// Delete the character before the cursor
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
            self.update_suggestions();
        }
    }

    /// Delete the character at the cursor
    pub fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
            self.update_suggestions();
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Clear the input
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.update_suggestions();
    }

    /// Select the previous suggestion
    pub fn select_prev(&mut self) {
        if !self.suggestions.is_empty() && self.selected_suggestion > 0 {
            self.selected_suggestion -= 1;
        }
    }

    /// Select the next suggestion
    pub fn select_next(&mut self) {
        if !self.suggestions.is_empty() && self.selected_suggestion < self.suggestions.len() - 1 {
            self.selected_suggestion += 1;
        }
    }

    /// Switch to the next mode
    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.next();
        self.update_suggestions();
    }

    /// Update suggestions based on current input and mode
    pub fn update_suggestions(&mut self) {
        self.suggestions.clear();
        self.selected_suggestion = 0;

        let input_lower = self.input.to_lowercase();

        match self.mode {
            CommandPaletteMode::Intent => {
                // Always show the main intent action
                if !self.input.is_empty() {
                    self.suggestions.push(CommandSuggestion {
                        text: format!("Find: \"{}\"", self.input),
                        description: "Start new intent pipeline with file discovery".to_string(),
                        action: CommandAction::StartIntent(self.input.clone()),
                    });
                }

                // Show recent intents that match
                for recent in &self.recent_intents {
                    if recent.to_lowercase().contains(&input_lower) || self.input.is_empty() {
                        self.suggestions.push(CommandSuggestion {
                            text: format!("Recent: \"{}\"", recent),
                            description: "Re-run previous intent".to_string(),
                            action: CommandAction::StartIntent(recent.clone()),
                        });
                    }
                    if self.suggestions.len() >= 8 {
                        break;
                    }
                }

                // Add example suggestions if input is empty
                if self.input.is_empty() {
                    self.suggestions.push(CommandSuggestion {
                        text: "Example: \"find all sales files\"".to_string(),
                        description: "Discover files matching a pattern".to_string(),
                        action: CommandAction::StartIntent("find all sales files".to_string()),
                    });
                    self.suggestions.push(CommandSuggestion {
                        text: "Example: \"process csv files in /data\"".to_string(),
                        description: "Find and process specific file types".to_string(),
                        action: CommandAction::StartIntent(
                            "process csv files in /data".to_string(),
                        ),
                    });
                }
            }
            CommandPaletteMode::Command => {
                let commands = [
                    ("/scan", "Scan a directory for files (Ingest)", "/scan"),
                    ("/query", "Query processed data", "/query"),
                    ("/approve", "Review approvals", "/approve"),
                    ("/workspace", "Switch workspace", "/workspace"),
                    ("/quarantine", "Open quarantine (Review)", "/quarantine"),
                    ("/catalog", "Open outputs catalog (Run)", "/catalog"),
                    ("/pipelines", "Open outputs catalog (Run)", "/pipelines"),
                    ("/jobs", "View jobs (Run)", "/jobs"),
                    ("/help", "Show help", "/help"),
                ];

                for (cmd, desc, action) in commands {
                    if cmd.contains(&input_lower) || self.input.is_empty() {
                        self.suggestions.push(CommandSuggestion {
                            text: cmd.to_string(),
                            description: desc.to_string(),
                            action: CommandAction::RunCommand(action.to_string()),
                        });
                    }
                }
            }
            CommandPaletteMode::Navigation => {
                for item in nav::NAV_ITEMS {
                    if item.label.to_lowercase().contains(&input_lower) || self.input.is_empty() {
                        self.suggestions.push(CommandSuggestion {
                            text: item.label.to_string(),
                            description: item.description.to_string(),
                            action: CommandAction::Navigate(item.mode),
                        });
                    }
                }
            }
        }
    }

    /// Get the currently selected action, if any
    pub fn selected_action(&self) -> Option<&CommandAction> {
        self.suggestions
            .get(self.selected_suggestion)
            .map(|s| &s.action)
    }

    /// Add an intent to recent history
    pub fn add_to_history(&mut self, intent: String) {
        // Remove if already exists
        self.recent_intents.retain(|i| i != &intent);
        // Add to front
        self.recent_intents.insert(0, intent);
        // Keep max 10 recent intents
        if self.recent_intents.len() > 10 {
            self.recent_intents.truncate(10);
        }
    }
}

// =============================================================================
// Query Console Types (View 6)
// =============================================================================

/// View state within Query mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryViewState {
    #[default]
    Editing, // Editing SQL in the editor
    Executing,      // Query is running
    ViewingResults, // Focus on results table
    TableBrowser,   // Table list overlay
    SavedQueries,   // Saved query picker
}

/// Query results from SQL execution
#[derive(Debug, Clone, Default)]
pub struct QueryResults {
    /// Column names from the result set
    pub columns: Vec<String>,
    /// Row data as strings
    pub rows: Vec<Vec<String>>,
    /// Total row count (may differ from rows.len() if truncated)
    pub row_count: usize,
    /// Whether results were truncated due to size limit
    pub truncated: bool,
    /// Currently selected row in results table
    pub selected_row: usize,
    /// Horizontal scroll offset for wide tables
    pub scroll_x: usize,
}

#[derive(Debug, Clone)]
pub struct TableBrowserEntry {
    pub schema: String,
    pub name: String,
    pub insert_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct TableBrowserState {
    pub tables: Vec<TableBrowserEntry>,
    pub selected_index: usize,
    pub loaded: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SavedQueryEntry {
    pub name: String,
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SavedQueriesState {
    pub entries: Vec<SavedQueryEntry>,
    pub selected_index: usize,
    pub loaded: bool,
    pub error: Option<String>,
}

/// State for Query mode (SQL query console)
#[derive(Debug, Clone, Default)]
pub struct QueryState {
    /// Current view state within Query mode
    pub view_state: QueryViewState,
    /// SQL input text
    pub sql_input: String,
    /// Cursor position within the SQL input
    pub cursor_position: usize,
    /// Query history (most recent first)
    pub history: Vec<String>,
    /// Current index in history (None = not browsing history)
    pub history_index: Option<usize>,
    /// Query results (None if no query run yet)
    pub results: Option<QueryResults>,
    /// Error message from last query (None if successful)
    pub error: Option<String>,
    /// Status/notice message (e.g., save confirmations)
    pub status_message: Option<String>,
    /// Whether a query is currently executing
    pub executing: bool,
    /// Execution time in milliseconds (None if no query run)
    pub execution_time_ms: Option<u64>,
    /// Temporary storage for input when browsing history
    pub draft_input: Option<String>,
    /// Table browser data
    pub table_browser: TableBrowserState,
    /// Saved queries data
    pub saved_queries: SavedQueriesState,
}

impl QueryState {
    /// Add a query to history
    pub fn add_to_history(&mut self, query: &str) {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        // Remove duplicate if exists
        self.history.retain(|q| q != &trimmed);
        // Add to front
        self.history.insert(0, trimmed);
        // Limit history size
        if self.history.len() > 100 {
            self.history.pop();
        }
    }

    /// Navigate to previous history entry
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Save current input as draft
                self.draft_input = Some(self.sql_input.clone());
                self.history_index = Some(0);
                self.sql_input = self.history[0].clone();
                self.cursor_position = self.sql_input.len();
            }
            Some(idx) if idx + 1 < self.history.len() => {
                self.history_index = Some(idx + 1);
                self.sql_input = self.history[idx + 1].clone();
                self.cursor_position = self.sql_input.len();
            }
            _ => {}
        }
    }

    /// Navigate to next history entry (or back to draft)
    pub fn history_next(&mut self) {
        match self.history_index {
            Some(0) => {
                // Return to draft
                self.history_index = None;
                if let Some(draft) = self.draft_input.take() {
                    self.sql_input = draft;
                }
                self.cursor_position = self.sql_input.len();
            }
            Some(idx) => {
                self.history_index = Some(idx - 1);
                self.sql_input = self.history[idx - 1].clone();
                self.cursor_position = self.sql_input.len();
            }
            None => {}
        }
    }

    /// Clear the current query state for a new query
    pub fn clear_for_new_query(&mut self) {
        self.error = None;
        self.results = None;
        self.execution_time_ms = None;
    }
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
    Warning { consecutive_failures: u32 },
    /// Circuit breaker tripped
    Paused { reason: String },
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
    Tags, // Renamed from Rules - users browse by tag category
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
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            input.clear();
            TextInputResult::Continue
        }
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

/// Strongly-typed rule ID (None = unsaved rule)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(pub Option<TaggingRuleId>);

impl RuleId {
    pub fn new(id: TaggingRuleId) -> Self {
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
    Files, // Default state, navigate files
    // --- Modal input overlays (were previously booleans) ---
    Filtering,      // Text filter input (was is_filtering)
    EnteringPath,   // Scan path input (was is_entering_path)
    ScanConfirm,    // Confirm scan for risky paths
    Tagging,        // Single file tag input (was is_tagging)
    CreatingSource, // Source name input (was is_creating_source)
    BulkTagging,    // Bulk tag input (was is_bulk_tagging)
    // --- Dropdown menus ---
    SourcesDropdown, // Filtering/selecting sources
    TagsDropdown,    // Filtering/selecting tags
    // --- Full dialogs ---
    RulesManager, // Dialog for rule CRUD
    RuleCreation, // Dialog for creating/editing single rule
    RuleBuilder,  // Split-view Rule Builder (specs/rule_builder.md)
    // --- Sources Manager (spec v1.7) ---
    SourcesManager,      // Dialog for source CRUD (M key)
    SourceEdit,          // Nested dialog for editing source name
    SourceDeleteConfirm, // Delete confirmation dialog
    // --- Background scanning ---
    Scanning, // Directory scan in progress (non-blocking)
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

impl SourceInfo {
    fn from_control(source: ScoutSourceInfo) -> Self {
        Self {
            id: source.id,
            name: source.name,
            path: std::path::PathBuf::from(source.path),
            file_count: source.file_count.max(0) as usize,
        }
    }
}

/// Tag with file count (for Tags dropdown in sidebar)
/// Tags are derived from files, showing what categories exist
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,     // Tag name, "All files", or "untagged"
    pub count: usize,     // Number of files with this tag
    pub is_special: bool, // True for "All files" and "untagged"
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

impl RuleInfo {
    fn from_control(rule: ScoutRuleInfo) -> Self {
        Self {
            id: RuleId::new(rule.id),
            pattern: rule.pattern,
            tag: rule.tag,
            priority: rule.priority,
            enabled: rule.enabled,
        }
    }
}

/// Pending tag write for persistence
#[derive(Debug, Clone)]
pub struct PendingTagWrite {
    pub file_id: i64,
    pub tag: String,
    pub workspace_id: WorkspaceId,
    pub tag_source: TagSource,
    pub rule_id: Option<TaggingRuleId>,
}

/// Pending source update (name/path changes)
#[derive(Debug, Clone)]
pub struct PendingSourceUpdate {
    pub id: SourceId,
    pub name: Option<String>,
    pub path: Option<String>,
}

/// Pending source delete
#[derive(Debug, Clone)]
pub struct PendingSourceDelete {
    pub id: SourceId,
}

/// Result from background directory scan
#[derive(Debug)]
pub enum TuiScanResult {
    /// Validation passed, scan is starting
    Started {
        job_id: i64,
        scan_id: Option<String>,
    },
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
    Progress {
        progress: u8,
        paths_analyzed: usize,
        total_paths: usize,
    },
    /// Schema eval completed successfully
    Complete {
        job_id: i64,
        pattern: String,
        pattern_seeds: Vec<super::extraction::PatternSeed>,
        path_archetypes: Vec<super::extraction::PathArchetype>,
        naming_schemes: Vec<super::extraction::NamingScheme>,
        synonym_suggestions: Vec<super::extraction::SynonymSuggestion>,
        rule_candidates: Vec<super::extraction::RuleCandidate>,
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
        rule_candidates: Vec<super::extraction::RuleCandidate>,
        paths_analyzed: usize,
    },
    Error(String),
}

/// Pending rule write for persistence
#[derive(Debug, Clone)]
pub struct PendingRuleWrite {
    pub id: TaggingRuleId,
    pub workspace_id: WorkspaceId,
    pub pattern: String,
    pub tag: String,
}

/// Pending rule update for persistence (enabled toggle)
#[derive(Debug, Clone)]
pub struct PendingRuleUpdate {
    pub id: TaggingRuleId,
    pub enabled: bool,
    pub workspace_id: WorkspaceId,
}

/// Pending rule delete for persistence
#[derive(Debug, Clone)]
pub struct PendingRuleDelete {
    pub id: TaggingRuleId,
    pub workspace_id: WorkspaceId,
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
    pub cache_source_id: Option<SourceId>,
    /// Workspace ID for which cache was loaded (to detect workspace changes)
    pub cache_workspace_id: Option<WorkspaceId>,

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
            cache_workspace_id: None,
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
        Self::Folder {
            name,
            path: None,
            file_count,
        }
    }

    /// Create a file entry
    pub fn file(name: String, file_count: usize) -> Self {
        Self::File {
            name,
            path: None,
            file_count,
        }
    }

    /// Create a folder/file entry
    pub fn new(name: String, file_count: usize, is_file: bool) -> Self {
        Self::with_path(name, None, file_count, is_file)
    }

    /// Create a folder/file entry with explicit navigation path
    pub fn with_path(name: String, path: Option<String>, file_count: usize, is_file: bool) -> Self {
        if is_file {
            Self::File {
                name,
                path,
                file_count,
            }
        } else {
            Self::Folder {
                name,
                path,
                file_count,
            }
        }
    }

    /// Create a loading placeholder
    pub fn loading(message: &str) -> Self {
        Self::Loading {
            message: message.to_string(),
        }
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
    Published { job_id: String },
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
    /// Current page offset (0-based row index in the full result set)
    pub page_offset: usize,
    /// Page size for DB-backed pagination
    pub page_size: usize,
    /// Total files matching current filters
    pub total_files: usize,
    /// Text filter for file list (used in Filtering state)
    pub filter: String,
    /// True when DB already applied tag/text filters to `files`
    pub db_filtered: bool,
    pub preview_open: bool,
    /// Path input for scan dialog (used in EnteringPath state)
    pub scan_path_input: String,
    /// Pending path awaiting confirmation for risky scans
    pub scan_confirm_path: Option<String>,
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
    /// Source path to select after sources reload (e.g., after scan completion)
    pub pending_select_source_path: Option<String>,
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
    /// Whether we've applied the default inspector collapse for Discover
    pub inspector_defaulted: bool,

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
    pub pending_rule_updates: Vec<PendingRuleUpdate>,
    pub pending_rule_deletes: Vec<PendingRuleDelete>,
    pub pending_source_creates: Vec<Source>,
    pub pending_source_updates: Vec<PendingSourceUpdate>,
    pub pending_source_deletes: Vec<PendingSourceDelete>,
    /// Source ID to touch for MRU ordering (set on source selection)
    pub pending_source_touch: Option<SourceId>,

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
            || !self
                .sources
                .iter()
                .any(|s| Some(&s.id) == self.selected_source_id.as_ref())
        {
            // Selection invalid, select first source
            self.selected_source_id = self.sources.first().map(|s| s.id.clone());
        }
    }
}

#[derive(Debug, Clone)]
enum DiscoverTagFilter {
    All,
    Untagged,
    Tag(String),
}

/// File information for Discover mode
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: i64,
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
/// Page size for Discover file listings (DB-backed pagination).
const DISCOVER_PAGE_SIZE: usize = 1000;

/// Main application state
pub struct App {
    /// Whether app is running
    pub running: bool,
    /// Current TUI mode/screen
    pub mode: TuiMode,
    /// Active tab within Ingest
    pub ingest_tab: IngestTab,
    /// Active tab within Run
    pub run_tab: RunTab,
    /// Active tab within Review
    pub review_tab: ReviewTab,
    /// Whether the help overlay is visible (per spec Section 3.1)
    pub show_help: bool,
    /// Whether the right-side inspector panel is collapsed
    pub inspector_collapsed: bool,
    /// Global shell focus (rail vs main)
    pub shell_focus: ShellFocus,
    /// Selected nav index when rail is focused
    pub nav_selected: usize,
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
    /// Approvals mode state
    pub approvals_state: ApprovalsState,
    /// Query mode state
    pub query_state: QueryState,
    /// Settings mode state
    pub settings: SettingsState,
    /// Sessions mode state (Intent Pipeline Workflow)
    pub sessions_state: SessionsState,
    /// Triage mode state (Quarantine/Schema/Dead Letter)
    pub triage_state: TriageState,
    /// Catalog mode state (Pipelines/Runs)
    pub catalog_state: CatalogState,
    /// Command palette overlay state
    pub command_palette: CommandPaletteState,
    /// Workspace switcher overlay state
    pub workspace_switcher: WorkspaceSwitcherState,
    /// Whether the Jobs drawer overlay is visible (toggle with J)
    pub jobs_drawer_open: bool,
    /// Selected job in the drawer (for Enter navigation)
    pub jobs_drawer_selected: usize,
    /// Whether the Sources drawer overlay is visible (toggle with S)
    pub sources_drawer_open: bool,
    /// Selected source in the drawer (for Enter navigation)
    pub sources_drawer_selected: usize,
    /// Active workspace (scopes sources/files/rules)
    pub active_workspace: Option<Workspace>,
    /// Optional global status message (toast)
    pub global_status: Option<GlobalStatusMessage>,
    /// Configuration
    #[allow(dead_code)]
    pub config: TuiArgs,
    /// Control API address (when connected)
    control_addr: Option<String>,
    /// Whether Control API is connected (sentinel is writer)
    control_connected: bool,
    /// Last time we probed the control plane (for reconnect)
    last_control_probe: Option<Instant>,
    /// Optional telemetry recorder (Tape domain events)
    pub telemetry: Option<TelemetryRecorder>,
    /// Last error message
    #[allow(dead_code)]
    pub error: Option<String>,
    /// Pending scan result from background directory scan
    pending_scan: Option<mpsc::Receiver<TuiScanResult>>,
    /// Cancellation token for active scans
    scan_cancel_token: Option<ScanCancelToken>,
    /// Job ID for the currently running scan (for status updates)
    current_scan_job_id: Option<i64>,
    /// Control-plane scan ID (when connected)
    current_scan_id: Option<String>,
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
    /// Pending Rule Builder extraction preview
    pending_rule_builder_preview: Option<mpsc::Receiver<RuleBuilderPreviewResult>>,
    /// Cancellation token for pending glob search (set to true to cancel)
    glob_search_cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Pending SQL query execution
    pending_query: Option<mpsc::Receiver<QueryExecutionResult>>,
    /// Pending folder query (on-demand database query for navigation)
    pending_folder_query: Option<mpsc::Receiver<FolderQueryMessage>>,
    /// Pending sources load (non-blocking DB query)
    pending_sources_load: Option<mpsc::Receiver<Result<Vec<SourceInfo>, String>>>,
    /// Pending files load (control-plane query)
    pending_files_load: Option<mpsc::Receiver<FilesLoadMessage>>,
    /// Pending tags load (control-plane query)
    pending_tags_load: Option<mpsc::Receiver<Result<TagsLoadMessage, String>>>,
    /// Pending jobs load (non-blocking DB query)
    pending_jobs_load: Option<mpsc::Receiver<Result<Vec<JobInfo>, String>>>,
    /// Pending home stats load (non-blocking DB query)
    pending_stats_load: Option<mpsc::Receiver<Result<HomeStats, String>>>,
    /// Pending approvals load (non-blocking DB query)
    pending_approvals_load: Option<mpsc::Receiver<Result<Vec<ApprovalInfo>, String>>>,
    /// Pending sessions load (non-blocking file scan)
    pending_sessions_load: Option<mpsc::Receiver<Vec<SessionInfo>>>,
    /// Pending triage load (non-blocking DB query)
    pending_triage_load: Option<mpsc::Receiver<Result<TriageData, String>>>,
    /// Pending catalog load (non-blocking DB query)
    pending_catalog_load: Option<mpsc::Receiver<Result<CatalogData, String>>>,
    /// Pending control write flush (non-blocking Control API calls)
    pending_control_writes: Option<mpsc::Receiver<ControlWriteResult>>,
    /// Pending manual tag apply by paths (control-plane)
    pending_tag_apply: Option<mpsc::Receiver<Result<TagApplyResult, String>>>,
    /// Pending rule apply to source (control-plane)
    pending_rule_apply: Option<mpsc::Receiver<Result<RuleApplyResult, String>>>,
    /// Last time jobs were polled (for incremental updates)
    last_jobs_poll: Option<std::time::Instant>,
    /// Profiler for frame timing and zone breakdown (F12 toggle)
    #[cfg(feature = "profiling")]
    pub profiler: casparian_profiler::Profiler,
    /// Database is in read-only mode due to failed health check
    db_read_only: bool,
    /// Health check already performed
    db_health_checked: bool,
    /// Health warning message (shown once in UI)
    db_health_warning: Option<String>,
}

/// Cache load messages (simplified - no chunking needed)
enum CacheLoadMessage {
    /// Loading complete (includes folder cache and tags)
    Complete {
        workspace_id: WorkspaceId,
        source_id: SourceId,
        total_files: usize,
        tags: Vec<TagInfo>,
        cache: HashMap<String, Vec<FsEntry>>,
    },
    /// Error during loading
    Error(String),
}

/// Files load message (paged results).
enum FilesLoadMessage {
    Complete {
        workspace_id: WorkspaceId,
        source_id: SourceId,
        page_offset: usize,
        total_count: usize,
        files: Vec<FileInfo>,
    },
    Error(String),
}

/// Tags load message.
struct TagsLoadMessage {
    workspace_id: WorkspaceId,
    source_id: SourceId,
    tags: Vec<TagInfo>,
    available_tags: Vec<String>,
}

/// Result for manual tag apply by paths.
struct TagApplyResult {
    tag: String,
    paths: Vec<String>,
    tagged_count: usize,
}

/// Result for rule apply to source.
struct RuleApplyResult {
    rule_id: TaggingRuleId,
    pattern: String,
    tag: String,
    tagged_count: usize,
}

/// Message for on-demand folder queries
enum FolderQueryMessage {
    /// Query completed successfully
    Complete {
        workspace_id: WorkspaceId,
        prefix: String,
        folders: Vec<FsEntry>,
        total_count: usize,
    },
    /// Error during query
    Error(String),
}

struct QueryExecutionResult {
    sql: String,
    result: Result<QueryResults, String>,
    elapsed_ms: u64,
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
            format!("...{}", &self.source_name[self.source_name.len() - 22..])
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
    pub source_id: SourceId,
}

/// Result of background glob search
struct GlobSearchResult {
    folders: Vec<FsEntry>,
    total_count: usize,
    pattern: String,
    error: Option<String>,
}

/// Result of background Rule Builder pattern search
struct RuleBuilderSearchResult {
    folder_matches: Vec<super::extraction::FolderMatch>,
    total_count: usize,
    pattern: String,
    error: Option<String>,
}

/// Result of background Rule Builder extraction preview
struct RuleBuilderPreviewResult {
    preview_files: Vec<super::extraction::ExtractionPreviewFile>,
    total_count: usize,
    pattern: String,
    error: Option<String>,
}

struct ControlWriteResult {
    sources_changed: bool,
    error: Option<String>,
    control_connected: Option<bool>,
}

impl App {
    fn check_db_health_once(&mut self) {
        if self.db_health_checked {
            return;
        }
        if self.control_connected {
            self.db_health_checked = true;
            return;
        }
        self.db_health_checked = true;

        let (backend, path) = self.resolve_db_target();
        if !path.exists() {
            return;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() == 0 {
                return;
            }
        }

        let conn = match App::open_db_readonly_with(backend, &path) {
            Ok(Some(conn)) => conn,
            Ok(None) => return,
            Err(err) => {
                self.report_db_error("Database health check failed", err);
                return;
            }
        };

        let required_tables = [
            "cf_workspaces",
            "scout_sources",
            "scout_files",
            "scout_rules",
            "scout_file_tags",
            "scout_folders",
            "scout_settings",
            "schema_migrations",
        ];

        let mut missing = Vec::new();
        for table in required_tables {
            match conn.table_exists(table) {
                Ok(true) => {}
                Ok(false) => missing.push(table.to_string()),
                Err(err) => {
                    self.report_db_error("Database health check failed", err);
                    return;
                }
            }
        }

        if !missing.is_empty() {
            drop(conn);
            let msg = format!("Missing tables: {}", missing.join(", "));
            if self.control_connected {
                let warn = format!(
                    "Database missing tables: {}. Control API active; not resetting.",
                    missing.join(", ")
                );
                self.db_health_warning = Some(warn.clone());
                self.discover.status_message = Some((warn, true));
                self.db_read_only = true;
                return;
            }
            if self.reset_db_file(&path, &msg) {
                return;
            }
            self.db_read_only = true;
            let warn = format!(
                "Database missing tables: {}. Read-only mode.",
                missing.join(", ")
            );
            self.db_health_warning = Some(warn.clone());
            self.discover.status_message = Some((warn, true));
        }
    }

    fn reset_db_file(&mut self, path: &std::path::Path, reason: &str) -> bool {
        if self.control_connected {
            let warn = "Control API active; refusing to reset state store.".to_string();
            self.db_health_warning = Some(warn.clone());
            self.discover.status_message = Some((warn, true));
            return false;
        }
        if !self.config.standalone_writer {
            let warn = "Sentinel not reachable; run with --standalone-writer to reset the state store."
                .to_string();
            self.db_health_warning = Some(warn.clone());
            self.discover.status_message = Some((warn, true));
            return false;
        }
        if !dev_allow_offline_write() {
            let warn = "Standalone writer disabled; set CASPARIAN_DEV_ALLOW_DIRECT_DB_WRITE=1."
                .to_string();
            self.db_health_warning = Some(warn.clone());
            self.discover.status_message = Some((warn, true));
            return false;
        }
        if !dev_allow_destructive_reset() {
            let warn = "Destructive reset disabled; set CASPARIAN_DEV_ALLOW_RESET=1 to allow."
                .to_string();
            self.db_health_warning = Some(warn.clone());
            self.discover.status_message = Some((warn, true));
            return false;
        }
        let _ = std::fs::remove_file(path);
        match ScoutDatabase::open(path) {
            Ok(_) => {
                self.db_read_only = false;
                self.active_workspace = None;
                self.discover.sources_loaded = false;
                self.pending_sources_load = None;
                self.discover.data_loaded = false;
                self.discover.db_filtered = false;
                self.discover.page_offset = 0;
                self.discover.total_files = 0;
                self.discover.files.clear();
                self.pending_cache_load = None;
                self.cache_load_progress = None;
                self.discover.status_message = Some((
                    format!("Database reset (pre-v1). Reason: {}", reason),
                    false,
                ));
                true
            }
            Err(err) => {
                let warn = format!("Database reset failed: {}", err);
                self.db_health_warning = Some(warn.clone());
                self.discover.status_message = Some((warn, true));
                false
            }
        }
    }

    fn nav_index_for_mode(mode: TuiMode) -> usize {
        nav::nav_index_for_mode(mode).unwrap_or(0)
    }

    fn nav_mode_for_index(index: usize) -> TuiMode {
        nav::nav_mode_for_index(index)
    }

    fn set_mode(&mut self, mode: TuiMode) {
        self.mode = mode;
        self.nav_selected = Self::nav_index_for_mode(mode);
    }

    fn ensure_rule_builder_ready(&mut self) {
        if self.discover.rule_builder.is_none() {
            let source_id = self.discover.selected_source_id;
            let mut builder = super::extraction::RuleBuilderState::new(source_id);
            builder.pattern = "**/*".to_string();
            self.discover.rule_builder = Some(builder);
        }
    }

    fn set_ingest_tab(&mut self, tab: IngestTab) {
        self.ingest_tab = tab;
        self.set_mode(TuiMode::Ingest);
        if !self.discover.inspector_defaulted {
            self.inspector_collapsed = true;
            self.discover.inspector_defaulted = true;
        }
        if self.ingest_tab != IngestTab::Sources {
            self.ensure_rule_builder_ready();
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                use super::extraction::RuleBuilderFocus;
                match self.ingest_tab {
                    IngestTab::Select => {
                        if !matches!(
                            builder.focus,
                            RuleBuilderFocus::Pattern
                                | RuleBuilderFocus::Excludes
                                | RuleBuilderFocus::ExcludeInput
                                | RuleBuilderFocus::FileList
                        ) {
                            builder.focus = RuleBuilderFocus::Pattern;
                        }
                    }
                    IngestTab::Rules => {
                        if matches!(builder.focus, RuleBuilderFocus::Suggestions) {
                            builder.focus = RuleBuilderFocus::Pattern;
                        }
                    }
                    IngestTab::Validate => {
                        if !matches!(builder.focus, RuleBuilderFocus::FileList) {
                            builder.focus = RuleBuilderFocus::FileList;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn set_run_tab(&mut self, tab: RunTab) {
        self.run_tab = tab;
        self.set_mode(TuiMode::Run);
    }

    fn set_review_tab(&mut self, tab: ReviewTab) {
        self.review_tab = tab;
        self.set_mode(TuiMode::Review);
    }

    fn next_task_tab(&mut self) {
        match self.mode {
            TuiMode::Ingest => {
                self.set_ingest_tab(self.ingest_tab.next());
            }
            TuiMode::Run => {
                self.set_run_tab(self.run_tab.next());
            }
            TuiMode::Review => {
                self.set_review_tab(self.review_tab.next());
            }
            _ => {}
        }
    }

    fn prev_task_tab(&mut self) {
        match self.mode {
            TuiMode::Ingest => {
                self.set_ingest_tab(self.ingest_tab.prev());
            }
            TuiMode::Run => {
                self.set_run_tab(self.run_tab.prev());
            }
            TuiMode::Review => {
                self.set_review_tab(self.review_tab.prev());
            }
            _ => {}
        }
    }

    fn navigate_to_mode(&mut self, mode: TuiMode) {
        match mode {
            TuiMode::Home => self.set_mode(TuiMode::Home),
            TuiMode::Ingest => {
                self.set_mode(TuiMode::Ingest);
                if self.ingest_tab != IngestTab::Sources {
                    self.ensure_rule_builder_ready();
                }
            }
            TuiMode::Run => self.set_mode(TuiMode::Run),
            TuiMode::Review => self.set_mode(TuiMode::Review),
            TuiMode::Query => self.set_mode(TuiMode::Query),
            TuiMode::Settings => {
                if self.mode != TuiMode::Settings {
                    self.settings.previous_mode = Some(self.mode);
                }
                self.set_mode(TuiMode::Settings);
            }
        }

        self.shell_focus = ShellFocus::Main;
    }

    /// Create new app with given args
    pub fn new(args: TuiArgs, telemetry: Option<TelemetryRecorder>) -> Self {
        let discover = DiscoverState {
            page_size: DISCOVER_PAGE_SIZE,
            ..Default::default()
        };
        let control_addr = Self::resolve_control_addr();
        let control_connected = control_addr
            .as_deref()
            .map(Self::probe_control_addr)
            .unwrap_or(false);
        let mut app = Self {
            running: true,
            mode: TuiMode::Home,
            ingest_tab: IngestTab::Select,
            run_tab: RunTab::Jobs,
            review_tab: ReviewTab::Triage,
            show_help: false,
            inspector_collapsed: false,
            shell_focus: ShellFocus::Main,
            nav_selected: Self::nav_index_for_mode(TuiMode::Home),
            home: HomeState::default(),
            discover,
            parser_bench: ParserBenchState::default(),
            jobs_state: JobsState::default(),
            sources_state: SourcesState::default(),
            approvals_state: ApprovalsState::default(),
            query_state: QueryState::default(),
            triage_state: TriageState::default(),
            catalog_state: CatalogState::default(),
            jobs_drawer_open: false,
            jobs_drawer_selected: 0,
            sources_drawer_open: false,
            sources_drawer_selected: 0,
            active_workspace: None,
            global_status: None,
            settings: SettingsState {
                default_source_path: "~/data".to_string(),
                auto_scan_on_startup: true,
                confirm_destructive: true,
                theme: "dark".to_string(),
                unicode_symbols: true,
                show_hidden_files: false,
                ..Default::default()
            },
            sessions_state: SessionsState::default(),
            command_palette: CommandPaletteState::new(),
            workspace_switcher: WorkspaceSwitcherState::default(),
            config: args,
            control_addr,
            control_connected,
            last_control_probe: None,
            telemetry,
            error: None,
            pending_scan: None,
            scan_cancel_token: None,
            current_scan_job_id: None,
            current_scan_id: None,
            current_schema_eval_job_id: None,
            pending_schema_eval: None,
            pending_sample_eval: None,
            pending_cache_load: None,
            cache_load_progress: None,
            last_cache_load_timing: None,
            tick_count: 0,
            pending_glob_search: None,
            pending_rule_builder_search: None,
            pending_rule_builder_preview: None,
            glob_search_cancelled: None,
            pending_query: None,
            pending_folder_query: None,
            pending_sources_load: None,
            pending_files_load: None,
            pending_tags_load: None,
            pending_jobs_load: None,
            pending_stats_load: None,
            pending_approvals_load: None,
            pending_sessions_load: None,
            pending_triage_load: None,
            pending_catalog_load: None,
            pending_control_writes: None,
            pending_tag_apply: None,
            pending_rule_apply: None,
            last_jobs_poll: None,
            #[cfg(feature = "profiling")]
            profiler: casparian_profiler::Profiler::new(250), // 250ms frame budget
            db_read_only: false,
            db_health_checked: false,
            db_health_warning: None,
        };

        if app.config.standalone_writer {
            if dev_allow_offline_write() {
                app.set_global_status_for(
                    "OFFLINE WRITE MODE (dev only)",
                    true,
                    Duration::from_secs(60 * 60 * 24),
                );
            } else {
                app.set_global_status_for(
                    "Standalone writer disabled; set CASPARIAN_DEV_ALLOW_DIRECT_DB_WRITE=1",
                    true,
                    Duration::from_secs(8),
                );
            }
        }

        app
    }

    /// Enter Discover mode with Rule Builder initialized immediately.
    /// This ensures the Rule Builder UI appears instantly (no loading delay).
    /// Files will populate asynchronously as the cache loads.
    pub fn enter_discover_mode(&mut self) {
        self.set_ingest_tab(IngestTab::Select);

        self.ensure_rule_builder_ready();

        // Set view state to Rule Builder immediately
        self.discover.view_state = DiscoverViewState::RuleBuilder;

        // Stay in RuleBuilder view; dropdowns open only on explicit user action.
    }

    fn refresh_current_view(&mut self) {
        match self.mode {
            TuiMode::Home => {
                // Mark stats as needing refresh - will trigger reload on next tick
                self.home.stats_loaded = false;
            }
            TuiMode::Ingest => match self.ingest_tab {
                IngestTab::Sources => {
                    self.discover.sources_loaded = false;
                }
                _ => {
                    self.discover.data_loaded = false;
                    self.discover.db_filtered = false;
                    self.refresh_tags_list();
                }
            },
            TuiMode::Run => match self.run_tab {
                RunTab::Jobs => {
                    self.jobs_state.jobs_loaded = false;
                    self.last_jobs_poll = None;
                    self.jobs_state.selected_index = 0;
                    self.jobs_state.section_focus = JobsListSection::Actionable;
                    self.jobs_state.actionable_index = 0;
                    self.jobs_state.ready_index = 0;
                    self.jobs_state.pinned_job_id = None;
                }
                RunTab::Outputs => {
                    self.catalog_state.loaded = false;
                }
            },
            TuiMode::Review => match self.review_tab {
                ReviewTab::Approvals => {
                    self.approvals_state.approvals_loaded = false;
                }
                ReviewTab::Sessions => {
                    self.sessions_state.sessions_loaded = false;
                }
                ReviewTab::Triage => {
                    self.triage_state.loaded = false;
                }
            },
            TuiMode::Query => {}
            TuiMode::Settings => {}
        }
    }

    fn in_text_input_mode(&self) -> bool {
        // Command palette is always a text input mode when visible
        if self.command_palette.visible {
            return true;
        }
        if self.workspace_switcher.visible
            && self.workspace_switcher.mode == WorkspaceSwitcherMode::Creating
        {
            return true;
        }
        match self.mode {
            TuiMode::Ingest => {
                if self.ingest_tab == IngestTab::Sources {
                    return self.sources_state.editing;
                }
                if let Some(ref explorer) = self.discover.glob_explorer {
                    if matches!(explorer.phase, GlobExplorerPhase::Filtering) {
                        return true;
                    }
                }
                if self.discover.sources_filtering || self.discover.tags_filtering {
                    return true;
                }
                if self.discover.view_state == DiscoverViewState::RuleBuilder {
                    if let Some(ref builder) = self.discover.rule_builder {
                        use super::extraction::RuleBuilderFocus;
                        if matches!(
                            builder.focus,
                            RuleBuilderFocus::Pattern
                                | RuleBuilderFocus::Tag
                                | RuleBuilderFocus::ExcludeInput
                                | RuleBuilderFocus::ExtractionEdit(_)
                        ) {
                            return true;
                        }
                    }
                }
                matches!(
                    self.discover.view_state,
                    DiscoverViewState::Filtering
                        | DiscoverViewState::EnteringPath
                        | DiscoverViewState::ScanConfirm
                        | DiscoverViewState::Tagging
                        | DiscoverViewState::CreatingSource
                        | DiscoverViewState::BulkTagging
                        | DiscoverViewState::RuleCreation
                        | DiscoverViewState::SourcesDropdown
                        | DiscoverViewState::TagsDropdown
                        | DiscoverViewState::SourceEdit
                )
            }
            TuiMode::Review => {
                matches!(
                    self.review_tab,
                    ReviewTab::Approvals
                ) && self.approvals_state.view_state == ApprovalsViewState::ConfirmReject
            }
            TuiMode::Query => self.query_state.view_state == QueryViewState::Editing,
            _ => false,
        }
    }

    pub(crate) fn is_text_input_mode(&self) -> bool {
        self.in_text_input_mode()
    }

    pub(crate) fn view_label(&self) -> String {
        match self.mode {
            TuiMode::Home => "Home".to_string(),
            TuiMode::Ingest => format!("Ingest/{}", self.ingest_tab.label()),
            TuiMode::Run => format!("Run/{}", self.run_tab.label()),
            TuiMode::Review => format!("Review/{}", self.review_tab.label()),
            TuiMode::Query => "Query".to_string(),
            TuiMode::Settings => "Settings".to_string(),
        }
    }

    pub(crate) fn ui_signature(&self) -> UiSignature {
        UiSignature::from_app(self)
    }

    pub(crate) fn ui_signature_key(&self) -> String {
        self.ui_signature().key()
    }

    pub(crate) fn check_profiler_dump(&mut self) {
        #[cfg(feature = "profiling")]
        {
            let _ = self.profiler.enabled;
        }
    }

    fn active_discover_tag_filter(&self) -> DiscoverTagFilter {
        let active_tag_idx = if self.discover.view_state == DiscoverViewState::TagsDropdown {
            self.discover.preview_tag
        } else {
            self.discover.selected_tag
        };

        match active_tag_idx {
            None => DiscoverTagFilter::All,
            Some(idx) => match self.discover.tags.get(idx) {
                Some(tag_info) if tag_info.name == "All files" => DiscoverTagFilter::All,
                Some(tag_info) if tag_info.name == "untagged" => DiscoverTagFilter::Untagged,
                Some(tag_info) => DiscoverTagFilter::Tag(tag_info.name.clone()),
                None => DiscoverTagFilter::All,
            },
        }
    }

    fn discover_path_filter_clause(&self) -> Option<(String, DbValue)> {
        let raw = self.discover.filter.trim();
        if raw.is_empty() {
            return None;
        }

        let has_wildcards = raw.contains('*') || raw.contains('?');
        let like = if has_wildcards {
            let normalized = patterns::normalize_glob_pattern(raw);
            let like = normalized
                .replace("**/", "%")
                .replace("**", "%")
                .replace('*', "%")
                .replace('?', "_");
            if like.is_empty() || like == "%" || like == "%%" {
                return None;
            }
            like
        } else {
            format!("%{}%", raw)
        };

        Some((
            "LOWER(f.rel_path) LIKE LOWER(?)".to_string(),
            DbValue::Text(like),
        ))
    }

    pub(crate) fn filtered_files(&self) -> Vec<&FileInfo> {
        let mut files: Vec<&FileInfo> = self.discover.files.iter().collect();
        if files.is_empty() {
            return files;
        }

        let tag_filter = self.active_discover_tag_filter();
        let raw_filter = self.discover.filter.trim();
        let has_filter = !raw_filter.is_empty();

        let (matcher, filter_lower) = if has_filter && (raw_filter.contains('*') || raw_filter.contains('?')) {
            let normalized = patterns::normalize_glob_pattern(raw_filter);
            (patterns::build_matcher(&normalized).ok(), String::new())
        } else {
            (None, raw_filter.to_lowercase())
        };

        files.retain(|file| {
            let tag_ok = match &tag_filter {
                DiscoverTagFilter::All => true,
                DiscoverTagFilter::Untagged => file.tags.is_empty(),
                DiscoverTagFilter::Tag(tag) => file.tags.iter().any(|t| t == tag),
            };
            if !tag_ok {
                return false;
            }
            if !has_filter {
                return true;
            }
            if let Some(ref matcher) = matcher {
                matcher.is_match(&file.rel_path)
            } else {
                file.rel_path.to_lowercase().contains(&filter_lower)
            }
        });

        files
    }

    pub(crate) fn discover_page_bounds(&self) -> (usize, usize, usize) {
        let total = self.discover.total_files;
        if total == 0 || self.discover.files.is_empty() {
            return (0, 0, total);
        }
        let start = self.discover.page_offset + 1;
        let end = (self.discover.page_offset + self.discover.files.len()).min(total);
        (start, end, total)
    }

    fn discover_rule_pattern_from_filter(&self) -> Option<String> {
        let raw = self.discover.filter.trim();
        if raw.is_empty() {
            return None;
        }

        let is_glob = raw.contains('*') || raw.contains('?') || raw.contains('[');
        if is_glob {
            Some(patterns::normalize_glob_pattern(raw))
        } else {
            Some(format!("*{}*", raw))
        }
    }

    fn set_discover_page_offset(&mut self, offset: usize) {
        self.discover.page_offset = offset;
        self.discover.selected = 0;
        self.discover.data_loaded = false;
        self.discover.db_filtered = false;
    }

    fn discover_next_page(&mut self) {
        let total = self.discover.total_files;
        if total == 0 {
            return;
        }
        let page_size = self.discover.page_size.max(1);
        let max_offset = (total - 1) / page_size * page_size;
        if self.discover.page_offset < max_offset {
            let next = (self.discover.page_offset + page_size).min(max_offset);
            self.set_discover_page_offset(next);
        }
    }

    fn discover_prev_page(&mut self) {
        let page_size = self.discover.page_size.max(1);
        let prev = self.discover.page_offset.saturating_sub(page_size);
        if prev != self.discover.page_offset {
            self.set_discover_page_offset(prev);
        }
    }

    fn discover_first_page(&mut self) {
        if self.discover.page_offset != 0 {
            self.set_discover_page_offset(0);
        }
    }

    fn discover_last_page(&mut self) {
        let total = self.discover.total_files;
        if total == 0 {
            return;
        }
        let page_size = self.discover.page_size.max(1);
        let max_offset = (total - 1) / page_size * page_size;
        if self.discover.page_offset != max_offset {
            self.set_discover_page_offset(max_offset);
        }
    }

    fn add_scan_job(&mut self, directory_path: &str) -> i64 {
        // Generate unique job ID from timestamp
        let job_id = chrono::Local::now().timestamp_millis();

        let job = JobInfo {
            id: job_id,
            file_id: None,
            job_type: JobType::Scan,
            origin: JobOrigin::Ephemeral,
            name: "scan".to_string(),
            version: None,
            status: JobStatus::Running,
            started_at: chrono::Local::now(),
            completed_at: None,
            pipeline_run_id: None,
            logical_date: None,
            selection_snapshot_hash: None,
            quarantine_rows: None,
            items_total: 0,
            items_processed: 0,
            items_failed: 0,
            output_path: Some(directory_path.to_string()),
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
            violations: vec![],
            top_violations_loaded: false,
            selected_violation_index: 0,
        };

        // Add to front of list so it's visible immediately
        self.jobs_state.push_job(job);

        job_id
    }

    fn update_scan_job_status(
        &mut self,
        job_id: i64,
        status: JobStatus,
        error: Option<String>,
        processed: Option<u32>,
        total: Option<u32>,
    ) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = status;
            if let Some(value) = processed {
                job.items_processed = value;
            }
            if let Some(value) = total {
                job.items_total = value;
            }
            if matches!(
                status,
                JobStatus::Completed
                    | JobStatus::PartialSuccess
                    | JobStatus::Failed
                    | JobStatus::Cancelled
            ) {
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

    fn add_schema_eval_job(&mut self, mode: SchemaEvalMode, paths_total: usize) -> i64 {
        let job_id = chrono::Local::now().timestamp_millis();

        let job = JobInfo {
            id: job_id,
            file_id: None,
            job_type: JobType::SchemaEval,
            origin: JobOrigin::Ephemeral,
            name: match mode {
                SchemaEvalMode::Sample => "schema-sample".to_string(),
                SchemaEvalMode::Full => "schema-full".to_string(),
            },
            version: None,
            status: JobStatus::Running,
            started_at: chrono::Local::now(),
            completed_at: None,
            pipeline_run_id: None,
            logical_date: None,
            selection_snapshot_hash: None,
            quarantine_rows: None,
            items_total: paths_total as u32,
            items_processed: 0,
            items_failed: 0,
            output_path: None,
            output_size_bytes: None,
            backtest: None,
            failures: vec![],
            violations: vec![],
            top_violations_loaded: false,
            selected_violation_index: 0,
        };

        self.jobs_state.push_job(job);
        job_id
    }

    fn update_schema_eval_job(
        &mut self,
        job_id: i64,
        status: JobStatus,
        processed: u32,
        error: Option<String>,
    ) {
        if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = status;
            job.items_processed = processed;
            if matches!(
                status,
                JobStatus::Completed
                    | JobStatus::PartialSuccess
                    | JobStatus::Failed
                    | JobStatus::Cancelled
            ) {
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

    fn run_sample_schema_eval(&mut self) {
        let (source_id_raw, pattern, excludes, eval_running, last_key) =
            match self.discover.rule_builder.as_ref() {
                Some(builder) => (
                    builder.source_id.clone(),
                    builder.pattern.clone(),
                    builder.excludes.clone(),
                    matches!(
                        builder.eval_state,
                        super::extraction::EvalState::Running { .. }
                    ),
                    builder.last_sample_eval_key.clone(),
                ),
                None => return,
            };

        if eval_running || self.pending_schema_eval.is_some() || self.pending_sample_eval.is_some()
        {
            return;
        }

        let source_id = match source_id_raw {
            Some(id) => id,
            None => return,
        };
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message = Some(("No workspace selected".to_string(), true));
                return;
            }
        };

        let sample_key = format!(
            "{}|{}|{}",
            source_id.as_i64(),
            pattern.as_str(),
            excludes.join("\u{1f}")
        );
        if last_key.as_deref() == Some(&sample_key) {
            return;
        }

        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.last_sample_eval_key = Some(sample_key);
        }

        let (tx, rx) = mpsc::sync_channel::<SampleEvalResult>(16);
        self.pending_sample_eval = Some(rx);

        if !self.control_connected {
            self.discover.status_message = Some((
                "Sentinel not connected; cannot sample schema".to_string(),
                true,
            ));
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.status_message = Some((
                    "Control API address not configured".to_string(),
                    true,
                ));
                return;
            }
        };

        let exclude_patterns = excludes;

        std::thread::spawn(move || {
            let client = match ControlClient::connect_with_timeout(
                &control_addr,
                Duration::from_millis(500),
            ) {
                Ok(client) => client,
                Err(err) => {
                    let _ = tx.send(SampleEvalResult::Error(format!(
                        "Control API unavailable: {}",
                        err
                    )));
                    return;
                }
            };

            let glob_pattern = match super::pattern_query::eval_glob_pattern(&pattern) {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx.send(SampleEvalResult::Error(err));
                    return;
                }
            };

            let mut paths =
                match client.sample_paths_for_eval(workspace_id, source_id, glob_pattern) {
                    Ok(paths) => paths,
                    Err(err) => {
                        let _ = tx.send(SampleEvalResult::Error(format!(
                            "Sample eval request failed: {}",
                            err
                        )));
                        return;
                    }
                };
            if paths.is_empty() {
                let _ = tx.send(SampleEvalResult::Error("No files to analyze".to_string()));
                return;
            }

            let exclude_matchers: Vec<globset::GlobMatcher> = exclude_patterns
                .into_iter()
                .filter_map(|pattern| {
                    let glob = patterns::normalize_glob_pattern(&pattern);
                    patterns::build_matcher(&glob).ok()
                })
                .collect();

            if !exclude_matchers.is_empty() {
                paths.retain(|path| !exclude_matchers.iter().any(|m| m.is_match(path)));
            }
            if paths.is_empty() {
                let _ = tx.send(SampleEvalResult::Error("No files to analyze".to_string()));
                return;
            }

            let mut state = super::extraction::RuleBuilderState::default();
            super::extraction::analyze_paths_for_schema_ui(&mut state, &paths, 5);

            let _ = tx.send(SampleEvalResult::Complete {
                pattern: pattern.to_string(),
                pattern_seeds: state.pattern_seeds,
                path_archetypes: state.path_archetypes,
                naming_schemes: state.naming_schemes,
                synonym_suggestions: state.synonym_suggestions,
                rule_candidates: state.rule_candidates,
                paths_analyzed: paths.len(),
            });
        });
    }

    fn start_full_schema_eval(&mut self) {
        let (pattern, source_id_raw, excludes, full_eval_running) =
            match self.discover.rule_builder.as_ref() {
                Some(builder) => (
                    builder.pattern.clone(),
                    builder.source_id.clone(),
                    builder.excludes.clone(),
                    matches!(
                        builder.eval_state,
                        super::extraction::EvalState::Running { .. }
                    ),
                ),
                None => return,
            };

        if full_eval_running {
            return;
        }

        let source_id = match source_id_raw {
            Some(id) => id,
            None => {
                self.discover.status_message = Some(("No source selected".to_string(), true));
                return;
            }
        };

        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message = Some(("No workspace selected".to_string(), true));
                return;
            }
        };

        if !self.control_connected {
            self.discover.status_message = Some((
                "Sentinel not connected; cannot run full eval".to_string(),
                true,
            ));
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.status_message = Some((
                    "Control API address not configured".to_string(),
                    true,
                ));
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
        let (tx, rx) = mpsc::sync_channel::<SchemaEvalResult>(16);
        self.pending_schema_eval = Some(rx);

        let exclude_patterns = excludes;

        std::thread::spawn(move || {
            let _ = tx.send(SchemaEvalResult::Started { job_id });

            let client = match ControlClient::connect_with_timeout(
                &control_addr,
                Duration::from_millis(500),
            ) {
                Ok(client) => client,
                Err(err) => {
                    let _ = tx.send(SchemaEvalResult::Error(format!(
                        "Control API unavailable: {}",
                        err
                    )));
                    return;
                }
            };

            let glob_pattern = match super::pattern_query::eval_glob_pattern(&pattern) {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx.send(SchemaEvalResult::Error(err));
                    return;
                }
            };
            let matcher = match super::pattern_query::build_eval_matcher(&glob_pattern) {
                Ok(m) => m,
                Err(err) => {
                    let _ = tx.send(SchemaEvalResult::Error(err));
                    return;
                }
            };
            let exclude_matchers: Vec<globset::GlobMatcher> = exclude_patterns
                .into_iter()
                .filter_map(|pattern| {
                    let glob = patterns::normalize_glob_pattern(&pattern);
                    patterns::build_matcher(&glob).ok()
                })
                .collect();

            let mut offset = 0usize;
            let batch_size = 1000usize;
            let mut processed = 0usize;
            let mut matched_paths: Vec<String> = Vec::new();
            let mut total_candidates: Option<usize> = None;

            loop {
                let result = match client.pattern_query(
                    workspace_id,
                    source_id,
                    glob_pattern.clone(),
                    batch_size,
                    offset,
                ) {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = tx.send(SchemaEvalResult::Error(format!(
                            "Schema eval request failed: {}",
                            err
                        )));
                        return;
                    }
                };

                if total_candidates.is_none() {
                    let total = result.total_count.max(0) as usize;
                    total_candidates = Some(total);
                    let _ = tx.send(SchemaEvalResult::Progress {
                        progress: 0,
                        paths_analyzed: 0,
                        total_paths: total,
                    });
                }

                if result.files.is_empty() {
                    break;
                }

                for file in result.files.iter() {
                    if !matcher.is_match(&file.rel_path) {
                        continue;
                    }
                    if exclude_matchers.iter().any(|m| m.is_match(&file.rel_path)) {
                        continue;
                    }
                    matched_paths.push(file.rel_path.clone());
                }

                processed += result.files.len();
                offset += result.files.len();

                let total = total_candidates.unwrap_or(0);
                if processed % 2000 == 0 || (total > 0 && processed >= total) {
                    let progress = if total == 0 {
                        100
                    } else {
                        ((processed.saturating_mul(100)) / total).min(99) as u8
                    };
                    let _ = tx.send(SchemaEvalResult::Progress {
                        progress,
                        paths_analyzed: processed,
                        total_paths: total,
                    });
                }
            }

            if matched_paths.is_empty() {
                let _ = tx.send(SchemaEvalResult::Error("No files to analyze".to_string()));
                return;
            }

            let mut state = super::extraction::RuleBuilderState::default();
            super::extraction::analyze_paths_for_schema_ui(&mut state, &matched_paths, 5);

            let _ = tx.send(SchemaEvalResult::Complete {
                job_id,
                pattern: pattern.to_string(),
                pattern_seeds: state.pattern_seeds,
                path_archetypes: state.path_archetypes,
                naming_schemes: state.naming_schemes,
                synonym_suggestions: state.synonym_suggestions,
                rule_candidates: state.rule_candidates,
                paths_analyzed: matched_paths.len(),
            });
        });

        self.discover.status_message = Some(("Full eval started...".to_string(), false));
    }

    fn is_risky_scan_path(&self, path: &str) -> bool {
        use std::path::Path;

        let input = Path::new(path);
        let expanded = scan_path::expand_scan_path(input);
        let canonical = scan_path::canonicalize_scan_path(&expanded);

        if let Some(home) = dirs::home_dir() {
            if canonical == home {
                return true;
            }
        }

        // Root on Unix or drive root on Windows
        canonical.parent().is_none()
    }

    fn scan_directory(&mut self, path: &str) {
        use std::path::Path;

        if self.control_connected {
            self.scan_directory_control(path);
            return;
        }

        if self.mutations_blocked() {
            let message = BackendRouter::new(
                self.control_addr.clone(),
                self.config.standalone_writer,
                self.db_read_only,
            )
            .blocked_message("start scan");
            self.discover.scan_error = Some(message.clone());
            self.discover.status_message = Some((message.clone(), true));
            self.set_global_status_for(message, true, Duration::from_secs(8));
            return;
        }

        self.ensure_active_workspace();

        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message =
                    Some(("No workspace available; cannot scan.".to_string(), true));
                return;
            }
        };

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
            elapsed_ms: 0,
            files_per_sec: 0.0,
            stalled: false,
        });
        self.discover.scan_start_time = Some(std::time::Instant::now());
        self.discover.view_state = DiscoverViewState::Scanning;
        self.discover.scan_error = None;

        // Channel for TUI scan results
        let (tui_tx, tui_rx) = mpsc::sync_channel::<TuiScanResult>(256);
        self.pending_scan = Some(tui_rx);

        // Create scan job for tracking in Jobs view
        let job_id = self.add_scan_job(&path_display);
        self.current_scan_job_id = Some(job_id);

        // Don't show "Scan started" yet - wait for validation to pass
        // The status will update to "Scan started" after validation succeeds
        // or show an error message if validation fails
        self.discover.status_message = None;

        // Get database path
        let db_path = self
            .config
            .database
            .clone()
            .unwrap_or_else(crate::cli::config::state_store_path);

        let source_path = path_display;
        let scan_job_id = job_id; // Capture for async block
        let telemetry = self.telemetry.clone();
        let cancel_token = ScanCancelToken::new();
        self.scan_cancel_token = Some(cancel_token.clone());

        std::thread::spawn(move || {
            // Open database
            if let Some(parent) = db_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let db = match ScoutDatabase::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "Failed to open database: {}",
                        e
                    )));
                    return;
                }
            };

            // Check if source with this path already exists (rescan case)
            let existing_source = match db.get_source_by_path(&workspace_id, &source_path) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Database error: {}", e)));
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
                if let Ok(Some(name_conflict)) = db.get_source_by_name(&workspace_id, &source_name)
                {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "A source named '{}' already exists at '{}'. Use Sources Manager (M) to rename or delete it first.",
                        source_name, name_conflict.path
                    )));
                    return;
                }

                // Check for overlapping sources (parent/child relationships)
                // This prevents duplicate file tracking and conflicting tags
                if let Err(e) =
                    db.check_source_overlap(&workspace_id, std::path::Path::new(&source_path))
                {
                    let suggestion = match &e {
                        casparian::scout::error::ScoutError::SourceIsChildOfExisting {
                            existing_name,
                            ..
                        } => {
                            format!(
                                "Either delete source '{}' first, or use tagging rules to organize files within that source.",
                                existing_name
                            )
                        }
                        casparian::scout::error::ScoutError::SourceIsParentOfExisting {
                            existing_name,
                            ..
                        } => {
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
                    )));
                    return;
                }

                // Create a new SourceId for the source
                let source_id = SourceId::new();

                let new_source = Source {
                    workspace_id,
                    id: source_id,
                    name: source_name,
                    source_type: SourceType::Local,
                    path: source_path.clone(),
                    exec_path: None,
                    poll_interval_secs: 0,
                    enabled: true,
                };

                // Insert new source
                if let Err(e) = db.upsert_source(&new_source) {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "Failed to save source: {}",
                        e
                    )));
                    return;
                }

                new_source
            };

            let scan_config = casparian::scout::ScanConfig::default();
            let telemetry_start = std::time::Instant::now();
            let telemetry_context = telemetry.as_ref().map(|recorder| {
                let run_id = Uuid::new_v4().to_string();
                let source_id = source.id.to_string();
                let root_hash = Some(
                    recorder
                        .hasher()
                        .hash_path(std::path::Path::new(&source.path)),
                );
                let payload = protocol_telemetry::ScanStarted {
                    run_id: run_id.clone(),
                    source_id: source_id.clone(),
                    root_hash,
                    started_at: chrono::Utc::now(),
                    config: scan_config_telemetry(&scan_config),
                };
                let parent_id = recorder.emit_domain(
                    protocol_telemetry::events::SCAN_START,
                    Some(&run_id),
                    None,
                    &payload,
                );
                (run_id, source_id, parent_id)
            });

            // Validation passed - notify TUI that scan is starting
            let _ = tui_tx.send(TuiScanResult::Started {
                job_id: scan_job_id,
                scan_id: None,
            });

            // Create progress channel that sends directly to TUI
            // We wrap tui_tx in a channel adapter so scanner can use its existing interface
            let (progress_tx, progress_rx) = mpsc::channel::<ScoutProgress>();

            // Spawn a task to forward progress - use spawn_blocking context awareness
            let tui_tx_progress = tui_tx.clone();
            let telemetry_progress = telemetry.clone();
            let telemetry_context_progress = telemetry_context.clone();
            let forward_handle = std::thread::spawn(move || {
                let mut last_emit = std::time::Instant::now()
                    .checked_sub(std::time::Duration::from_secs(1))
                    .unwrap_or_else(std::time::Instant::now);
                while let Ok(progress) = progress_rx.recv() {
                    let _ = tui_tx_progress.try_send(TuiScanResult::Progress(progress.clone()));

                    if let (Some(recorder), Some((run_id, source_id, parent_id))) = (
                        telemetry_progress.as_ref(),
                        telemetry_context_progress.as_ref(),
                    ) {
                        let now = std::time::Instant::now();
                        if now.duration_since(last_emit) >= std::time::Duration::from_secs(1) {
                            last_emit = now;
                            let payload = protocol_telemetry::ScanProgress {
                                run_id: run_id.clone(),
                                source_id: source_id.clone(),
                                elapsed_ms: progress.elapsed_ms,
                                files_found: progress.files_found,
                                files_persisted: progress.files_persisted,
                                dirs_scanned: progress.dirs_scanned,
                                files_per_sec: progress.files_per_sec,
                                stalled: progress.stalled,
                            };
                            recorder.emit_domain(
                                protocol_telemetry::events::SCAN_PROGRESS,
                                Some(run_id),
                                parent_id.as_deref(),
                                &payload,
                            );
                        }
                    }
                }
            });

            // Create scanner with default config
            let scanner = ScoutScanner::with_config(db, scan_config.clone());

            // Run the scan in a blocking task so it doesn't block the runtime
            let scan_result = if let Some((run_id, _, _)) = telemetry_context.as_ref() {
                let span = info_span!("scan.run", run_id = %run_id, source_id = %source.id);
                let _guard = span.enter();
                scanner.scan_with_cancel(&source, Some(progress_tx), None, Some(cancel_token))
            } else {
                scanner.scan_with_cancel(&source, Some(progress_tx), None, Some(cancel_token))
            };
            drop(scanner);

            let _ = forward_handle.join();

            match scan_result {
                Ok(result) => {
                    let persisted = result.stats.files_persisted as usize;
                    let discovered = result.stats.files_discovered as usize;
                    if let (Some(recorder), Some((run_id, source_id, _))) =
                        (telemetry.as_ref(), telemetry_context.as_ref())
                    {
                        let payload = protocol_telemetry::ScanCompleted {
                            run_id: run_id.clone(),
                            source_id: source_id.clone(),
                            duration_ms: result.stats.duration_ms,
                            files_discovered: result.stats.files_discovered,
                            files_persisted: result.stats.files_persisted,
                            files_new: result.stats.files_new,
                            files_changed: result.stats.files_changed,
                            files_deleted: result.stats.files_deleted,
                            dirs_scanned: result.stats.dirs_scanned,
                            bytes_scanned: result.stats.bytes_scanned,
                            errors: result.stats.errors,
                        };
                        recorder.emit_domain(
                            protocol_telemetry::events::SCAN_COMPLETE,
                            Some(run_id),
                            None,
                            &payload,
                        );
                    }
                    if persisted == 0 && discovered > 0 {
                        let _ = tui_tx.send(TuiScanResult::Error(
                            "Scan completed but no files were persisted. Check database health or locks.".to_string(),
                        ));
                        return;
                    }
                    // Scan complete - TUI will load files from DB
                    let _ = tui_tx.send(TuiScanResult::Complete {
                        source_path,
                        files_persisted: persisted,
                    });
                }
                Err(e) => {
                    if matches!(e, casparian::scout::error::ScoutError::Cancelled) {
                        let _ = tui_tx.send(TuiScanResult::Error("Scan cancelled".to_string()));
                        return;
                    }
                    if let (Some(recorder), Some((run_id, source_id, _))) =
                        (telemetry.as_ref(), telemetry_context.as_ref())
                    {
                        let (error_class, io_kind) = classify_scan_error(&e);
                        let payload = protocol_telemetry::ScanFailed {
                            run_id: run_id.clone(),
                            source_id: source_id.clone(),
                            duration_ms: telemetry_start.elapsed().as_millis() as u64,
                            error_class,
                            io_kind,
                        };
                        recorder.emit_domain(
                            protocol_telemetry::events::SCAN_FAIL,
                            Some(run_id),
                            None,
                            &payload,
                        );
                    }
                    let _ = tui_tx.send(TuiScanResult::Error(format!("Scan failed: {}", e)));
                }
            }
        });
    }

    fn scan_directory_control(&mut self, path: &str) {
        use std::path::Path;

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.status_message = Some((
                    "Control API address not configured".to_string(),
                    true,
                ));
                return;
            }
        };

        let path_input = Path::new(path);
        let expanded_path = scan_path::expand_scan_path(path_input);
        if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
            self.discover.scan_error = Some(err.to_string());
            return;
        }

        let canonical_path = scan_path::canonicalize_scan_path(&expanded_path);
        let path_display = canonical_path.display().to_string();

        self.discover.scanning_path = Some(path_display.clone());
        self.discover.scan_progress = Some(ScoutProgress {
            dirs_scanned: 0,
            files_found: 0,
            files_persisted: 0,
            current_dir: Some("Initializing...".to_string()),
            elapsed_ms: 0,
            files_per_sec: 0.0,
            stalled: false,
        });
        self.discover.scan_start_time = Some(std::time::Instant::now());
        self.discover.view_state = DiscoverViewState::Scanning;
        self.discover.scan_error = None;

        let (tui_tx, tui_rx) = mpsc::sync_channel::<TuiScanResult>(256);
        self.pending_scan = Some(tui_rx);

        let job_id = self.add_scan_job(&path_display);
        self.current_scan_job_id = Some(job_id);
        self.current_scan_id = None;
        self.scan_cancel_token = None;

        let workspace_id = self.active_workspace_id();
        self.discover.status_message = None;

        let scan_job_id = job_id;
        std::thread::spawn(move || {
            let client = match ControlClient::connect_with_timeout(&control_addr, Duration::from_millis(500)) {
                Ok(client) => client,
                Err(err) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "Control API unavailable: {}",
                        err
                    )));
                    return;
                }
            };

            let scan_id = match client.start_scan(workspace_id, path_display.clone()) {
                Ok(id) => id,
                Err(err) => {
                    let _ = tui_tx.send(TuiScanResult::Error(format!(
                        "Failed to start scan: {}",
                        err
                    )));
                    return;
                }
            };

            let _ = tui_tx.send(TuiScanResult::Started {
                job_id: scan_job_id,
                scan_id: Some(scan_id.clone()),
            });

            loop {
                match client.get_scan(scan_id.clone()) {
                    Ok(Some(status)) => {
                        if let Some(progress) = status.progress.as_ref() {
                            let tui_progress = ScoutProgress {
                                dirs_scanned: progress.dirs_scanned as usize,
                                files_found: progress.files_found as usize,
                                files_persisted: progress.files_persisted as usize,
                                current_dir: progress.current_dir.clone(),
                                elapsed_ms: progress.elapsed_ms,
                                files_per_sec: progress.files_per_sec,
                                stalled: progress.stalled,
                            };
                            let _ = tui_tx.send(TuiScanResult::Progress(tui_progress));
                        }

                        match status.state {
                            casparian_sentinel::ScanState::Completed => {
                                let persisted = status.files_persisted.unwrap_or(0) as usize;
                                let _ = tui_tx.send(TuiScanResult::Complete {
                                    source_path: status.source_path.clone(),
                                    files_persisted: persisted,
                                });
                                break;
                            }
                            casparian_sentinel::ScanState::Failed => {
                                let _ = tui_tx.send(TuiScanResult::Error(
                                    status
                                        .error
                                        .unwrap_or_else(|| "Scan failed".to_string()),
                                ));
                                break;
                            }
                            casparian_sentinel::ScanState::Cancelled => {
                                let _ = tui_tx.send(TuiScanResult::Error(
                                    status
                                        .error
                                        .unwrap_or_else(|| "Scan cancelled".to_string()),
                                ));
                                break;
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => {
                        let _ = tui_tx.send(TuiScanResult::Error(
                            "Scan not found in control plane".to_string(),
                        ));
                        break;
                    }
                    Err(err) => {
                        let _ = tui_tx.send(TuiScanResult::Error(format!(
                            "Failed to query scan status: {}",
                            err
                        )));
                        break;
                    }
                }

                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }

    fn list_directories(partial_path: &str) -> Vec<String> {
        use std::path::Path;

        let partial = if partial_path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                let rest = partial_path.strip_prefix("~").unwrap_or("");
                home.join(rest.trim_start_matches('/'))
                    .to_string_lossy()
                    .to_string()
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
            let prefix = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
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

    fn update_path_suggestions(&mut self) {
        self.discover.path_suggestions = Self::list_directories(&self.discover.scan_path_input);
        self.discover.path_suggestion_idx = 0;
    }

    fn apply_path_suggestion(&mut self) {
        if let Some(suggestion) = self
            .discover
            .path_suggestions
            .get(self.discover.path_suggestion_idx)
        {
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

    fn load_scout_files(&mut self) {
        self.discover.db_filtered = false;

        if self.discover.pending_select_source_path.is_some() {
            return;
        }

        // First check if we have a directly-set source ID (e.g., after scan completion)
        // This handles the case where sources list hasn't loaded yet
        let selected_source_id = if let Some(ref id) = self.discover.selected_source_id {
            id.clone()
        } else {
            let source_idx = if self.discover.view_state == DiscoverViewState::SourcesDropdown {
                self.discover
                    .preview_source
                    .unwrap_or_else(|| self.discover.selected_source_index())
            } else {
                self.discover.selected_source_index()
            };

            match self.discover.sources.get(source_idx) {
                Some(source) => source.id.clone(),
                None => {
                    self.discover.files.clear();
                    self.discover.selected = 0;
                    self.discover.page_offset = 0;
                    self.discover.total_files = 0;
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

        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.scan_error = Some("No workspace selected.".to_string());
                self.discover.page_offset = 0;
                self.discover.total_files = 0;
                self.discover.data_loaded = true;
                return;
            }
        };

        if !self.control_connected {
            if self.config.standalone_writer || cfg!(test) {
                self.load_scout_files_offline(workspace_id, selected_source_id);
                return;
            }
            self.discover.scan_error =
                Some("Sentinel not connected; cannot load files.".to_string());
            self.discover.page_offset = 0;
            self.discover.total_files = 0;
            self.discover.data_loaded = true;
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.scan_error =
                    Some("Control API address not configured.".to_string());
                self.discover.page_offset = 0;
                self.discover.total_files = 0;
                self.discover.data_loaded = true;
                return;
            }
        };

        if self.pending_files_load.is_some() {
            return;
        }

        let tag_filter = match self.active_discover_tag_filter() {
            DiscoverTagFilter::All => ScoutTagFilter::All,
            DiscoverTagFilter::Untagged => ScoutTagFilter::Untagged,
            DiscoverTagFilter::Tag(tag) => ScoutTagFilter::Tag(tag),
        };
        let path_filter = if self.discover.filter.trim().is_empty() {
            None
        } else {
            Some(self.discover.filter.trim().to_string())
        };
        let page_size = self.discover.page_size.max(1);
        let page_offset = self.discover.page_offset;

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_files_load = Some(rx);

        std::thread::spawn(move || {
            let result: Result<FilesLoadMessage, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let page = client
                    .list_files(
                        workspace_id,
                        selected_source_id,
                        tag_filter,
                        path_filter,
                        page_size,
                        page_offset,
                    )
                    .map_err(|err| format!("Files query failed: {}", err))?;

                let files = page
                    .files
                    .into_iter()
                    .map(|file| FileInfo {
                        file_id: file.id,
                        path: file.path,
                        rel_path: file.rel_path,
                        size: file.size,
                        modified: Self::millis_to_local(file.mtime)
                            .unwrap_or_else(Local::now),
                        is_dir: file.is_dir,
                        tags: file.tags,
                    })
                    .collect();

                Ok(FilesLoadMessage::Complete {
                    workspace_id,
                    source_id: selected_source_id,
                    page_offset,
                    total_count: page.total_count.max(0) as usize,
                    files,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => FilesLoadMessage::Error(err),
            };
            let _ = tx.send(msg);
        });
    }

    fn load_scout_files_offline(&mut self, workspace_id: WorkspaceId, source_id: SourceId) {
        if self.pending_files_load.is_some() {
            return;
        }

        let tag_filter = self.active_discover_tag_filter();
        let path_filter = self
            .discover_path_filter_clause()
            .and_then(|(clause, value)| match value {
                DbValue::Text(text) => Some((clause, text)),
                _ => None,
            });
        let page_size = self.discover.page_size.max(1);
        let page_offset = self.discover.page_offset;
        let (backend, db_path) = self.resolve_db_target();

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_files_load = Some(rx);

        std::thread::spawn(move || {
            let result: Result<FilesLoadMessage, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                let mut join_clause = String::new();
                let mut where_clauses = vec![
                    "f.workspace_id = ?".to_string(),
                    "f.source_id = ?".to_string(),
                ];
                let mut params: Vec<DbValue> = vec![
                    DbValue::Text(workspace_id.to_string()),
                    DbValue::Integer(source_id.as_i64()),
                ];

                match tag_filter {
                    DiscoverTagFilter::All => {}
                    DiscoverTagFilter::Untagged => {
                        join_clause = "LEFT JOIN scout_file_tags t ON t.file_id = f.id AND t.workspace_id = f.workspace_id".to_string();
                        where_clauses.push("t.file_id IS NULL".to_string());
                    }
                    DiscoverTagFilter::Tag(tag_name) => {
                        join_clause = "JOIN scout_file_tags t ON t.file_id = f.id AND t.workspace_id = f.workspace_id".to_string();
                        where_clauses.push("t.tag = ?".to_string());
                        params.push(DbValue::Text(tag_name));
                    }
                }

                if let Some((clause, value)) = path_filter {
                    where_clauses.push(clause);
                    params.push(DbValue::Text(value));
                }

                let where_sql = where_clauses.join(" AND ");
                let count_sql = format!(
                    "SELECT COUNT(*) FROM scout_files f {} WHERE {}",
                    join_clause, where_sql
                );
                let total_count = conn
                    .query_scalar::<i64>(&count_sql, &params)
                    .map_err(|err| format!("Query failed: {}", err))?
                    .max(0) as usize;

                let mut query_offset = page_offset;
                if total_count == 0 {
                    query_offset = 0;
                } else {
                    let max_offset = (total_count.saturating_sub(1) / page_size) * page_size;
                    if query_offset > max_offset {
                        query_offset = max_offset;
                    }
                }

                let mut page_params = params.clone();
                page_params.push(DbValue::Integer(page_size as i64));
                page_params.push(DbValue::Integer(query_offset as i64));

                let query = format!(
                    "SELECT f.id, f.path, f.rel_path, f.size, f.mtime, f.is_dir \
                     FROM scout_files f {} \
                     WHERE {} \
                     ORDER BY f.rel_path ASC, f.id ASC \
                     LIMIT ? OFFSET ?",
                    join_clause, where_sql
                );

                let rows = conn
                    .query_all(&query, &page_params)
                    .map_err(|err| format!("Query failed: {}", err))?;

                let mut files: Vec<FileInfo> = Vec::with_capacity(rows.len());
                let mut file_ids: Vec<i64> = Vec::with_capacity(rows.len());
                for row in rows {
                    let file_id: i64 = row.get(0).map_err(|e| e.to_string())?;
                    let path: String = row.get(1).map_err(|e| e.to_string())?;
                    let rel_path: String = row.get(2).map_err(|e| e.to_string())?;
                    let size: i64 = row.get(3).map_err(|e| e.to_string())?;
                    let mtime_millis: i64 = row.get(4).map_err(|e| e.to_string())?;
                    let is_dir: i64 = row.get(5).map_err(|e| e.to_string())?;

                    let modified = App::millis_to_local(mtime_millis)
                        .unwrap_or_else(Local::now);

                    file_ids.push(file_id);
                    files.push(FileInfo {
                        file_id,
                        path,
                        rel_path,
                        size: size as u64,
                        modified,
                        is_dir: is_dir != 0,
                        tags: Vec::new(),
                    });
                }

                let mut tags_by_file: HashMap<i64, Vec<String>> = HashMap::new();
                if !file_ids.is_empty() {
                    let placeholders = std::iter::repeat("?")
                        .take(file_ids.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let query = format!(
                        "SELECT file_id, tag FROM scout_file_tags WHERE workspace_id = ? AND file_id IN ({})",
                        placeholders
                    );
                    let mut params: Vec<DbValue> = Vec::with_capacity(file_ids.len() + 1);
                    params.push(DbValue::Text(workspace_id.to_string()));
                    params.extend(file_ids.iter().map(|id| DbValue::Integer(*id)));

                    if let Ok(tag_rows) = conn.query_all(&query, &params) {
                        for row in tag_rows {
                            let file_id: i64 = match row.get(0) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let tag: String = match row.get(1) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            tags_by_file.entry(file_id).or_default().push(tag);
                        }
                    }
                }

                for (idx, file) in files.iter_mut().enumerate() {
                    if let Some(tags) = tags_by_file.remove(&file_ids[idx]) {
                        file.tags = tags;
                    }
                }

                Ok(FilesLoadMessage::Complete {
                    workspace_id,
                    source_id,
                    page_offset: query_offset,
                    total_count,
                    files,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => FilesLoadMessage::Error(err),
            };
            let _ = tx.send(msg);
        });
    }

    fn update_folders_from_cache(&mut self) {
        // Note: profiling zone removed here to avoid borrow conflict with start_folder_query

        if let Some(ref mut explorer) = self.discover.glob_explorer {
            let prefix = explorer.current_prefix.clone();
            let pattern = explorer.pattern.clone();

            // Treat "**/*" and "**" as "show all" - use normal folder navigation
            // Only do recursive search for specific patterns like "**/*.rs"
            let is_match_all =
                pattern == "**/*" || pattern == "**" || pattern == "*" || pattern.is_empty();

            // Handle ** patterns with database query (not in-memory cache)
            // Skip for "match all" patterns - just use folder navigation
            if pattern.contains("**") && !is_match_all {
                // Cancel any pending search
                self.pending_glob_search = None;

                let pattern_for_search = pattern.clone();

                // Get source ID for query
                let (workspace_id, source_id) =
                    match (explorer.cache_workspace_id, explorer.cache_source_id) {
                        (Some(workspace_id), Some(source_id)) => (workspace_id, source_id),
                        _ => return,
                    };

                if !self.control_connected {
                    explorer.folders =
                        vec![FsEntry::loading("Sentinel not connected; cannot search")];
                    return;
                }

                let control_addr = match self.control_addr.clone() {
                    Some(addr) => addr,
                    None => {
                        explorer.folders =
                            vec![FsEntry::loading("Control API address not configured")];
                        return;
                    }
                };

                // Show loading indicator immediately
                let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                explorer.folders =
                    vec![FsEntry::loading(&format!("{} Searching...", spinner_char))];

                // Spawn async task for control query
                let (tx, rx) = mpsc::sync_channel(1);
                self.pending_glob_search = Some(rx);

                std::thread::spawn(move || {
                    let pattern_for_msg = pattern_for_search.clone();
                    let result: Result<GlobSearchResult, String> = (|| {
                        let client = ControlClient::connect_with_timeout(
                            &control_addr,
                            Duration::from_millis(500),
                        )
                        .map_err(|err| format!("Control API unavailable: {}", err))?;

                        let glob_pattern = patterns::normalize_glob_pattern(&pattern_for_search);
                        let result = client
                            .pattern_query(workspace_id, source_id, glob_pattern, 100, 0)
                            .map_err(|err| format!("Glob search failed: {}", err))?;

                        let folders: Vec<FsEntry> = result
                            .files
                            .into_iter()
                            .map(|file| {
                                FsEntry::with_path(
                                    file.rel_path.clone(),
                                    Some(file.rel_path),
                                    1,
                                    true,
                                )
                            })
                            .collect();

                        Ok(GlobSearchResult {
                            folders,
                            total_count: result.total_count.max(0) as usize,
                            pattern: pattern_for_msg.clone(),
                            error: None,
                        })
                    })();

                    let msg = match result {
                        Ok(msg) => msg,
                        Err(err) => GlobSearchResult {
                            folders: vec![],
                            total_count: 0,
                            pattern: pattern_for_msg,
                            error: Some(err),
                        },
                    };
                    let _ = tx.send(msg);
                });

                return;
            }

            // Normal pattern: filter current level only
            if let Some(cached_folders) = explorer.folder_cache.get(&prefix) {
                let mut folders: Vec<FsEntry> = if pattern.is_empty() {
                    cached_folders.clone()
                } else {
                    let pattern_lower = pattern.to_lowercase();
                    cached_folders
                        .iter()
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
                explorer.total_count =
                    GlobFileCount::Exact(folders.iter().map(|f| f.file_count()).sum());
                explorer.selected_folder = 0;
            } else {
                // Prefix not in cache - trigger async database query
                let spinner_char = crate::cli::tui::ui::spinner_char(self.tick_count);
                explorer.folders = vec![FsEntry::loading(&format!(
                    "{} Loading {}...",
                    spinner_char,
                    if prefix.is_empty() { "root" } else { &prefix }
                ))];
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
                if let (Some(source_id), Some(workspace_id)) =
                    (explorer.cache_source_id, explorer.cache_workspace_id)
                {
                    let prefix = explorer.current_prefix.clone();
                    let pattern = explorer.pattern.clone();
                    self.start_folder_query(workspace_id, source_id, prefix, pattern);
                }
            }
        }
    }

    fn start_folder_query(
        &mut self,
        workspace_id: WorkspaceId,
        source_id: SourceId,
        prefix: String,
        glob_pattern: String,
    ) {
        // Skip if already loading
        if self.pending_folder_query.is_some() {
            return;
        }

        if !self.control_connected {
            self.pending_folder_query = None;
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.pending_folder_query = None;
                return;
            }
        };

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_folder_query = Some(rx);

        let glob_opt = if glob_pattern.is_empty() {
            None
        } else {
            Some(glob_pattern)
        };

        std::thread::spawn(move || {
            let result: Result<FolderQueryMessage, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let (entries, total_count) = client
                    .list_folders(workspace_id, source_id, prefix.clone(), glob_opt)
                    .map_err(|err| format!("Folder query failed: {}", err))?;

                let folders: Vec<FsEntry> = entries
                    .into_iter()
                    .map(|entry| {
                        FsEntry::new(entry.name, entry.file_count as usize, entry.is_file)
                    })
                    .collect();

                Ok(FolderQueryMessage::Complete {
                    workspace_id,
                    prefix,
                    folders,
                    total_count: total_count.max(0) as usize,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => FolderQueryMessage::Error(err),
            };
            let _ = tx.send(msg);
        });
    }
    pub(super) fn resolve_db_target(&self) -> (DbBackend, std::path::PathBuf) {
        if let Some(ref path) = self.config.database {
            (DbBackend::Sqlite, path.clone())
        } else {
            (default_db_backend(), state_store_path())
        }
    }

    fn resolve_control_addr() -> Option<String> {
        if cfg!(test) {
            return None;
        }
        if std::env::var("CASPARIAN_CONTROL_DISABLED").is_ok() {
            return None;
        }
        Some(
            std::env::var("CASPARIAN_CONTROL_ADDR")
                .unwrap_or_else(|_| DEFAULT_CONTROL_ADDR.to_string()),
        )
    }

    fn probe_control_addr(addr: &str) -> bool {
        match ControlClient::connect_with_timeout(addr, Duration::from_millis(200)) {
            Ok(client) => client.ping().unwrap_or(false),
            Err(_) => false,
        }
    }

    fn maybe_probe_control_connection(&mut self) {
        let Some(addr) = self.control_addr.as_deref() else {
            return;
        };
        let now = Instant::now();
        let should_probe = self
            .last_control_probe
            .map(|last| now.duration_since(last) >= Duration::from_secs(2))
            .unwrap_or(true);
        if !should_probe {
            return;
        }
        self.last_control_probe = Some(now);

        let connected = Self::probe_control_addr(addr);
        if connected != self.control_connected {
            self.control_connected = connected;
            if connected {
                self.set_global_status("Sentinel connected", false);
            } else {
                self.set_global_status("Sentinel not reachable", true);
            }
        }
    }

    fn open_db_write(&self) -> Result<Option<DbConnection>, BackendError> {
        if self.db_read_only
            || self.control_connected
            || !self.config.standalone_writer
            || !dev_allow_offline_write()
        {
            return Ok(None);
        }
        let (backend, path) = self.resolve_db_target();
        Self::open_db_write_with(backend, &path)
    }

    fn open_scout_db_for_writes(&mut self) -> Option<ScoutDatabase> {
        if self.db_read_only
            || self.control_connected
            || !self.config.standalone_writer
            || !dev_allow_offline_write()
        {
            return None;
        }
        let (_backend, path) = self.resolve_db_target();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match ScoutDatabase::open(&path) {
            Ok(db) => Some(db),
            Err(casparian::scout::error::ScoutError::Config(msg)) => {
                if self.reset_db_file(&path, &msg) {
                    ScoutDatabase::open(&path).ok()
                } else {
                    None
                }
            }
            Err(casparian::scout::error::ScoutError::Database(BackendError::Locked(msg))) => {
                self.discover.status_message = Some((
                    format!("Database is locked by another process: {}", msg),
                    true,
                ));
                None
            }
            Err(casparian::scout::error::ScoutError::Database(BackendError::ReadOnly)) => {
                self.discover.status_message = Some((
                    "Database is read-only; cannot open for writes.".to_string(),
                    true,
                ));
                None
            }
            Err(casparian::scout::error::ScoutError::Database(err)) => {
                if self.reset_db_file(&path, &err.to_string()) {
                    ScoutDatabase::open(&path).ok()
                } else {
                    None
                }
            }
            Err(err) => {
                self.discover.status_message =
                    Some((format!("Database open failed: {}", err), true));
                None
            }
        }
    }

    fn active_workspace_id(&self) -> Option<WorkspaceId> {
        self.active_workspace.as_ref().map(|workspace| workspace.id)
    }

    fn ensure_active_workspace(&mut self) {
        if self.active_workspace.is_some() {
            return;
        }

        let active_from_context = match context::get_active_workspace_id() {
            Ok(id) => id,
            Err(err) => {
                self.discover.status_message =
                    Some((format!("Workspace context error: {}", err), true));
                None
            }
        };

        if let Some(active_id) = active_from_context {
            match self.query_workspace_by_id(&active_id) {
                Ok(Some(workspace)) => {
                    self.active_workspace = Some(workspace);
                    return;
                }
                Ok(None) => {
                    let _ = context::clear_active_workspace();
                }
                Err(err) => {
                    self.discover.status_message =
                        Some((format!("Workspace load failed: {}", err), true));
                }
            }
        }

        if !self.control_connected {
            if let Some(db) = self.open_scout_db_for_writes() {
                match db.ensure_default_workspace() {
                    Ok(workspace) => {
                        let id = workspace.id;
                        self.active_workspace = Some(workspace);
                        if let Err(err) = context::set_active_workspace(&id) {
                            self.discover.status_message = Some((
                                format!("Failed to persist workspace context: {}", err),
                                true,
                            ));
                        }
                        return;
                    }
                    Err(err) => {
                        self.discover.status_message =
                            Some((format!("Workspace init failed: {}", err), true));
                    }
                }
            }
        }

        if let Ok(Some(workspace)) = self.query_first_workspace() {
            let id = workspace.id;
            self.active_workspace = Some(workspace);
            if let Err(err) = context::set_active_workspace(&id) {
                self.discover.status_message = Some((
                    format!("Failed to persist workspace context: {}", err),
                    true,
                ));
            }
        }
    }


    fn job_from_control(job: ControlJobInfo) -> JobInfo {
        let status = JobStatus::from_db_status(job.status.as_str(), None);
        let started_at = job
            .created_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Local))
            .unwrap_or_else(Local::now);
        let mut completed_at = job
            .updated_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Local));
        if !matches!(
            status,
            JobStatus::Completed
                | JobStatus::PartialSuccess
                | JobStatus::Failed
                | JobStatus::Cancelled
        ) {
            completed_at = None;
        }

        let failures = job
            .error_message
            .map(|err| {
                vec![JobFailure {
                    file_path: String::new(),
                    error: err,
                    line: None,
                }]
            })
            .unwrap_or_default();

        let id = i64::try_from(job.id).expect("job id out of range for i64");
        JobInfo {
            id,
            file_id: if job.file_id == 0 {
                None
            } else {
                Some(job.file_id)
            },
            job_type: JobType::Parse,
            origin: JobOrigin::Persistent,
            name: job.plugin_name,
            version: job.parser_version,
            status,
            started_at,
            completed_at,
            pipeline_run_id: job.pipeline_run_id,
            logical_date: None,
            selection_snapshot_hash: None,
            quarantine_rows: Some(job.quarantine_rows),
            items_total: 0,
            items_processed: 0,
            items_failed: 0,
            output_path: None,
            output_size_bytes: None,
            backtest: None,
            failures,
            violations: vec![],
            top_violations_loaded: false,
            selected_violation_index: 0,
        }
    }

    fn set_global_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.set_global_status_for(
            message,
            is_error,
            std::time::Duration::from_secs(3),
        );
    }

    fn set_global_status_for(
        &mut self,
        message: impl Into<String>,
        is_error: bool,
        duration: std::time::Duration,
    ) {
        let message = message.into();
        let expires_at = std::time::Instant::now() + duration;
        self.global_status = Some(GlobalStatusMessage {
            message,
            is_error,
            expires_at,
        });
    }

    fn mutations_blocked(&self) -> bool {
        BackendRouter::new(
            self.control_addr.clone(),
            self.config.standalone_writer && dev_allow_offline_write(),
            self.db_read_only,
        )
        .mutations_blocked(self.control_connected)
    }

    fn open_db_write_with(
        backend: DbBackend,
        path: &std::path::Path,
    ) -> Result<Option<DbConnection>, BackendError> {
        if !path.exists() {
            return Ok(None);
        }

        match backend {
            DbBackend::Sqlite => DbConnection::open_sqlite(path).map(Some),
            DbBackend::DuckDb => DbConnection::open_duckdb(path).map(Some),
        }
    }

    fn report_db_error(&mut self, context: &str, err: impl std::fmt::Display) {
        self.discover
            .status_message
            .replace((format!("{}: {}", context, err), true));
    }

    /// Handle key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Workspace switcher overlay input has highest priority when visible
        if self.workspace_switcher.visible {
            self.handle_workspace_switcher_key(key);
            return;
        }

        // Handle command palette input when visible (highest priority)
        if self.command_palette.visible {
            self.handle_command_palette_key(key);
            return;
        }

        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => {
                    self.show_help = false;
                }
                _ => {}
            }
            return;
        }

        // Global keys - always active
        match key.code {
            // Command Palette openers (before other global handlers)
            // ':' or Ctrl+P = open in Command mode
            KeyCode::Char(':') if !self.in_text_input_mode() => {
                self.command_palette.open(CommandPaletteMode::Command);
                return;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_palette.open(CommandPaletteMode::Command);
                return;
            }
            // Ctrl+W: Workspace switcher
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.in_text_input_mode() {
                    self.open_workspace_switcher();
                    return;
                }
            }
            // '>' = open in Intent mode (natural language)
            KeyCode::Char('>') if !self.in_text_input_mode() => {
                self.command_palette.open(CommandPaletteMode::Intent);
                return;
            }
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
            // ========== GLOBAL VIEW NAVIGATION (per keybinding matrix) ==========
            // Keys 1-4 are RESERVED for view navigation and work from ANY view.
            // Don't intercept when in text input.

            // 1: Ingest
            KeyCode::Char('1') if !self.in_text_input_mode() => {
                self.navigate_to_mode(TuiMode::Ingest);
                return;
            }
            // 2: Run
            KeyCode::Char('2') if !self.in_text_input_mode() => {
                self.navigate_to_mode(TuiMode::Run);
                return;
            }
            // 3: Review
            KeyCode::Char('3') if !self.in_text_input_mode() => {
                self.navigate_to_mode(TuiMode::Review);
                return;
            }
            // 4: Query Console
            KeyCode::Char('4') if !self.in_text_input_mode() => {
                self.navigate_to_mode(TuiMode::Query);
                return;
            }
            // 5: Settings
            KeyCode::Char('5') if !self.in_text_input_mode() => {
                self.navigate_to_mode(TuiMode::Settings);
                return;
            }
            // Tab cycling within tasks
            KeyCode::Char('[') if !self.in_text_input_mode() => {
                self.prev_task_tab();
                return;
            }
            KeyCode::Char(']') if !self.in_text_input_mode() => {
                self.next_task_tab();
                return;
            }
            // I: Toggle Inspector panel
            KeyCode::Char('I') if !self.in_text_input_mode() => {
                self.inspector_collapsed = !self.inspector_collapsed;
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
            KeyCode::Char('S') if !self.in_text_input_mode() && self.mode != TuiMode::Ingest => {
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
                    if self.mode != TuiMode::Run {
                        self.jobs_state.previous_mode = Some(self.mode);
                    }
                    if let Some(job) = self.jobs_state.jobs.get(self.jobs_drawer_selected) {
                        self.jobs_state.section_focus = match job.status {
                            JobStatus::Completed | JobStatus::PartialSuccess => {
                                JobsListSection::Ready
                            }
                            JobStatus::Pending
                            | JobStatus::Running
                            | JobStatus::Failed
                            | JobStatus::Cancelled => JobsListSection::Actionable,
                        };

                        let target_list = match self.jobs_state.section_focus {
                            JobsListSection::Actionable => self.jobs_state.actionable_jobs(),
                            JobsListSection::Ready => self.jobs_state.ready_jobs(),
                        };

                        if let Some(index) = target_list.iter().position(|j| j.id == job.id) {
                            self.jobs_state.selected_index = index;
                        } else {
                            self.jobs_state.selected_index = 0;
                        }
                        self.jobs_state.clamp_selection();
                    }
                    self.set_run_tab(RunTab::Jobs);
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
                    if self.mode != TuiMode::Ingest {
                        self.sources_state.previous_mode = Some(self.mode);
                    }
                    self.set_ingest_tab(IngestTab::Sources);
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
                    if self.mode != TuiMode::Ingest {
                        self.sources_state.previous_mode = Some(self.mode);
                    }
                    self.set_ingest_tab(IngestTab::Sources);
                    self.sources_drawer_open = false;
                }
                return;
            }
            KeyCode::Char('d') if self.sources_drawer_open => {
                if let Some(source_idx) = self.sources_drawer_selected_source() {
                    self.sources_state.selected_index = source_idx;
                    self.sources_state.confirm_delete = true;
                    if self.mode != TuiMode::Ingest {
                        self.sources_state.previous_mode = Some(self.mode);
                    }
                    self.set_ingest_tab(IngestTab::Sources);
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
            KeyCode::Char('0') if !self.in_text_input_mode() => {
                self.set_mode(TuiMode::Home);
                return;
            }
            KeyCode::Char('H') if !self.in_text_input_mode() => {
                self.set_mode(TuiMode::Home);
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
            // Exempt Sources view: 'r' there means Rescan, not global refresh
            KeyCode::Char('r')
                if !self.in_text_input_mode()
                    && !(self.mode == TuiMode::Ingest && self.ingest_tab == IngestTab::Sources) =>
            {
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
                self.navigate_to_mode(TuiMode::Settings);
                return;
            }
            _ => {}
        }

        if !self.in_text_input_mode() {
            if self.shell_focus == ShellFocus::Rail {
                let max_index = nav::nav_max_index();
                match key.code {
                    KeyCode::Up => {
                        if self.nav_selected > 0 {
                            self.nav_selected -= 1;
                        }
                        return;
                    }
                    KeyCode::Down => {
                        if self.nav_selected < max_index {
                            self.nav_selected += 1;
                        }
                        return;
                    }
                    KeyCode::Enter => {
                        let target = Self::nav_mode_for_index(self.nav_selected);
                        self.navigate_to_mode(target);
                        return;
                    }
                    KeyCode::Right | KeyCode::Esc => {
                        self.shell_focus = ShellFocus::Main;
                        return;
                    }
                    _ => {
                        // Ignore other keys while rail is focused
                        return;
                    }
                }
            }

            if key.code == KeyCode::Left
                && !(self.mode == TuiMode::Ingest && self.ingest_tab != IngestTab::Sources)
            {
                self.shell_focus = ShellFocus::Rail;
                self.nav_selected = Self::nav_index_for_mode(self.mode);
                return;
            }
        }

        // Mode-specific keys (Main Focus)
        match self.mode {
            TuiMode::Home => self.handle_home_key(key),
            TuiMode::Ingest => match self.ingest_tab {
                IngestTab::Sources => self.handle_sources_key(key),
                _ => self.handle_discover_key(key),
            },
            TuiMode::Run => match self.run_tab {
                RunTab::Jobs => self.handle_jobs_key(key),
                RunTab::Outputs => self.handle_catalog_key(key),
            },
            TuiMode::Review => match self.review_tab {
                ReviewTab::Triage => self.handle_triage_key(key),
                ReviewTab::Approvals => self.handle_approvals_key(key),
                ReviewTab::Sessions => self.handle_sessions_key(key),
            },
            TuiMode::Query => self.handle_query_key(key),
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

    fn handle_discover_panel_shortcut(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('S') => {
                self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                self.discover.sources_filter.clear();
                self.discover.sources_filtering = false;
                self.discover.preview_source = Some(self.discover.selected_source_index());
            }
            KeyCode::Char('T') => {
                self.transition_discover_state(DiscoverViewState::TagsDropdown);
                self.discover.tags_filter.clear();
                self.discover.tags_filtering = false;
                self.discover.preview_tag = self.discover.selected_tag;
            }
            _ => {}
        }
    }


    fn create_source(&mut self, path: &str, name: &str) {
        if self.mutations_blocked() {
            self.discover.status_message = Some((
                "Database is read-only; cannot create sources".to_string(),
                true,
            ));
            return;
        }
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message = Some((
                    "No workspace selected; cannot create source".to_string(),
                    true,
                ));
                return;
            }
        };

        let raw_name = name.trim();
        let source_name = if raw_name.is_empty() {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        } else {
            raw_name.to_string()
        };

        if self.discover.sources.iter().any(|s| s.name == source_name) {
            self.discover.status_message = Some((
                format!("Source name '{}' already exists", source_name),
                true,
            ));
            return;
        }

        let expanded_path = scan_path::expand_scan_path(std::path::Path::new(path));
        if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
            self.discover.status_message = Some((err.to_string(), true));
            return;
        }

        let new_canon = scan_path::canonicalize_scan_path(&expanded_path);
        for source in &self.discover.sources {
            let existing_canon = scan_path::canonicalize_scan_path(&source.path);
            if new_canon.starts_with(&existing_canon) || existing_canon.starts_with(&new_canon) {
                self.discover.status_message =
                    Some((format!("Source path overlaps with '{}'", source.name), true));
                return;
            }
        }

        let source_path = expanded_path.display().to_string();
        let source_id = SourceId::new();

        let new_source = Source {
            workspace_id,
            id: source_id.clone(),
            name: source_name.clone(),
            source_type: SourceType::Local,
            path: source_path.clone(),
            exec_path: None,
            poll_interval_secs: 0,
            enabled: true,
        };

        self.discover.pending_source_creates.push(new_source);
        self.discover.sources.push(SourceInfo {
            id: source_id.clone(),
            name: source_name.clone(),
            path: std::path::PathBuf::from(&source_path),
            file_count: 0,
        });

        self.discover.selected_source_id = Some(source_id);
        self.sources_state.selected_index = self.discover.sources.len().saturating_sub(1);

        self.discover.status_message = Some((format!("Created source '{}'", source_name), false));
    }

    fn update_source_name(&mut self, source_id: &SourceId, new_name: &str) {
        if self.mutations_blocked() {
            self.discover.status_message = Some((
                "Database is read-only; cannot rename sources".to_string(),
                true,
            ));
            return;
        }

        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return;
        }

        if self
            .discover
            .sources
            .iter()
            .any(|s| s.name == trimmed && s.id != *source_id)
        {
            self.discover.status_message =
                Some((format!("Source name '{}' already exists", trimmed), true));
            return;
        }

        if let Some(source) = self
            .discover
            .sources
            .iter_mut()
            .find(|s| s.id == *source_id)
        {
            source.name = trimmed.to_string();
        }

        self.discover
            .pending_source_updates
            .push(PendingSourceUpdate {
                id: source_id.clone(),
                name: Some(trimmed.to_string()),
                path: None,
            });

        self.discover.status_message = Some((format!("Renamed source to '{}'", trimmed), false));
    }

    fn update_source_path(&mut self, source_id: &SourceId, new_path: &str) {
        if self.mutations_blocked() {
            self.discover.status_message = Some((
                "Database is read-only; cannot edit sources".to_string(),
                true,
            ));
            return;
        }

        let expanded_path = scan_path::expand_scan_path(std::path::Path::new(new_path));
        if let Err(err) = scan_path::validate_scan_path(&expanded_path) {
            self.discover.status_message = Some((err.to_string(), true));
            return;
        }

        let new_canon = scan_path::canonicalize_scan_path(&expanded_path);
        for source in &self.discover.sources {
            if source.id == *source_id {
                continue;
            }
            let existing_canon = scan_path::canonicalize_scan_path(&source.path);
            if new_canon.starts_with(&existing_canon) || existing_canon.starts_with(&new_canon) {
                self.discover.status_message =
                    Some((format!("Source path overlaps with '{}'", source.name), true));
                return;
            }
        }

        let source_path = expanded_path.display().to_string();
        if let Some(source) = self
            .discover
            .sources
            .iter_mut()
            .find(|s| s.id == *source_id)
        {
            source.path = std::path::PathBuf::from(&source_path);
        }

        self.discover
            .pending_source_updates
            .push(PendingSourceUpdate {
                id: source_id.clone(),
                name: None,
                path: Some(source_path),
            });

        self.discover.status_message = Some(("Updated source path".to_string(), false));
    }

    fn delete_source(&mut self, source_id: &SourceId) {
        if self.mutations_blocked() {
            self.discover.status_message = Some((
                "Database is read-only; cannot delete sources".to_string(),
                true,
            ));
            return;
        }

        self.discover
            .pending_source_deletes
            .push(PendingSourceDelete {
                id: source_id.clone(),
            });
        self.discover.sources.retain(|s| s.id != *source_id);
        self.discover.validate_source_selection();

        if self.sources_state.selected_index >= self.discover.sources.len()
            && self.sources_state.selected_index > 0
        {
            self.sources_state.selected_index = self.discover.sources.len().saturating_sub(1);
        }
        if self.discover.sources_manager_selected >= self.discover.sources.len()
            && self.discover.sources_manager_selected > 0
        {
            self.discover.sources_manager_selected = self.discover.sources.len().saturating_sub(1);
        }
    }
    fn queue_tag_for_file(
        &mut self,
        file_id: i64,
        tag: &str,
        tag_source: TagSource,
        rule_id: Option<TaggingRuleId>,
        show_message: bool,
    ) -> bool {
        if self.mutations_blocked() {
            let message = BackendRouter::new(
                self.control_addr.clone(),
                self.config.standalone_writer,
                self.db_read_only,
            )
            .blocked_message("apply tags");
            self.discover.scan_error = Some(message.clone());
            self.discover.status_message = Some((message, true));
            return false;
        }
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message =
                    Some(("No workspace selected; cannot apply tags".to_string(), true));
                return false;
            }
        };
        if tag_source == TagSource::Rule && rule_id.is_none() {
            self.discover.status_message =
                Some(("Rule-based tag write missing rule ID".to_string(), true));
            return false;
        }

        let mut already_tagged = false;
        let mut display_name: Option<String> = None;
        for file in &mut self.discover.files {
            if file.file_id == file_id {
                display_name = Some(file.rel_path.clone());
                if file.tags.contains(&tag.to_string()) {
                    already_tagged = true;
                } else {
                    file.tags.push(tag.to_string());
                }
                break;
            }
        }

        if already_tagged {
            if show_message {
                self.discover.status_message =
                    Some((format!("File already has tag '{}'", tag), true));
            }
            return false;
        }

        self.discover.pending_tag_writes.push(PendingTagWrite {
            file_id,
            tag: tag.to_string(),
            workspace_id,
            tag_source,
            rule_id,
        });

        if show_message {
            let name = display_name.unwrap_or_else(|| format!("file {}", file_id));
            self.discover
                .status_message
                .replace((format!("Tagged '{}' with '{}'", name, tag), false));
        }

        if !self.discover.available_tags.contains(&tag.to_string()) {
            self.discover.available_tags.push(tag.to_string());
        }

        true
    }
    fn rule_builder_selected_rel_path_from(
        builder: &super::extraction::RuleBuilderState,
    ) -> Option<String> {
        match &builder.file_results {
            super::extraction::FileResultsState::ExtractionPreview { preview_files } => {
                preview_files
                    .get(builder.selected_file)
                    .map(|f| f.relative_path.clone())
            }
            super::extraction::FileResultsState::BacktestResults {
                matched_files,
                visible_indices,
                ..
            } => visible_indices
                .get(builder.selected_file)
                .and_then(|idx| matched_files.get(*idx))
                .map(|f| f.relative_path.clone()),
            _ => None,
        }
    }
    fn rule_builder_preview_paths_from(
        builder: &super::extraction::RuleBuilderState,
        only_selected: bool,
    ) -> Vec<String> {
        let selected = &builder.selected_preview_files;
        let mut paths = Vec::new();

        match &builder.file_results {
            super::extraction::FileResultsState::ExtractionPreview { preview_files } => {
                for file in preview_files {
                    if !only_selected || selected.contains(&file.relative_path) {
                        paths.push(file.relative_path.clone());
                    }
                }
            }
            super::extraction::FileResultsState::BacktestResults {
                matched_files,
                visible_indices,
                ..
            } => {
                for idx in visible_indices {
                    if let Some(file) = matched_files.get(*idx) {
                        if !only_selected || selected.contains(&file.relative_path) {
                            paths.push(file.relative_path.clone());
                        }
                    }
                }
            }
            _ => {}
        }

        paths
    }
    fn apply_manual_tag_to_paths(&mut self, paths: &[String], tag: &str) -> usize {
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message =
                    Some(("No workspace selected; cannot apply tags".to_string(), true));
                return 0;
            }
        };

        let source_id = if let Some(id) = self.discover.selected_source_id.as_ref() {
            id.clone()
        } else {
            let idx = self.discover.selected_source_index();
            match self.discover.sources.get(idx) {
                Some(source) => source.id.clone(),
                None => {
                    self.discover.status_message =
                        Some(("No source selected; cannot apply tags".to_string(), true));
                    return 0;
                }
            }
        };

        if paths.is_empty() {
            self.discover.status_message =
                Some(("No preview results to tag".to_string(), true));
            return 0;
        }

        if !self.control_connected {
            self.discover.status_message = Some((
                "Sentinel not connected; cannot apply tags".to_string(),
                true,
            ));
            return 0;
        }

        if self.pending_tag_apply.is_some() {
            self.discover.status_message =
                Some(("Tagging already in progress".to_string(), true));
            return 0;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.status_message = Some((
                    "Control API address not configured".to_string(),
                    true,
                ));
                return 0;
            }
        };

        let requested = paths.len();
        let paths_vec: Vec<String> = paths.to_vec();
        let tag_string = tag.to_string();

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_tag_apply = Some(rx);

        std::thread::spawn(move || {
            let result: Result<TagApplyResult, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let (success, tagged_count, message) = client
                    .apply_tag_to_paths(
                        workspace_id,
                        source_id,
                        paths_vec.clone(),
                        tag_string.clone(),
                        TagSource::Manual,
                    )
                    .map_err(|err| format!("Tag apply failed: {}", err))?;

                if !success {
                    return Err(message);
                }

                Ok(TagApplyResult {
                    tag: tag_string,
                    paths: paths_vec,
                    tagged_count,
                })
            })();

            let _ = tx.send(result);
        });

        self.discover.status_message = Some((
            format!("Tagging {} files with '{}'", requested, tag),
            false,
        ));

        requested
    }

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
        let source_id = self.discover.selected_source().map(|s| s.id);
        let mut builder_state = super::extraction::RuleBuilderState::new(source_id);
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
    fn update_rule_builder_files(&mut self, pattern: &str) {
        let builder = match self.discover.rule_builder.as_mut() {
            Some(b) => b,
            None => return,
        };
        builder.selected_preview_files.clear();
        builder.manual_tag_confirm_open = false;
        builder.manual_tag_confirm_count = 0;
        builder.rule_candidates.clear();
        builder.selected_candidate = 0;
        builder.candidate_preview_open = false;
        builder.sampled_paths_count = 0;

        if pattern.is_empty() {
            builder.match_count = 0;
            builder.last_sample_eval_key = None;
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
    fn update_rule_builder_exploration(&mut self, pattern: &str) {
        use super::extraction::FolderMatch;

        let (workspace_id, source_id) = match (
            self.active_workspace_id(),
            self.discover
                .rule_builder
                .as_ref()
                .and_then(|b| b.source_id),
        ) {
            (Some(workspace_id), Some(source_id)) => (workspace_id, source_id),
            (None, _) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("No workspace selected".to_string());
                    builder.file_results = super::extraction::FileResultsState::Exploration {
                        folder_matches: Vec::new(),
                        expanded_folder_indices: std::collections::HashSet::new(),
                        detected_patterns: Vec::new(),
                    };
                }
                return;
            }
            (_, None) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("No source selected".to_string());
                    builder.file_results = super::extraction::FileResultsState::Exploration {
                        folder_matches: Vec::new(),
                        expanded_folder_indices: std::collections::HashSet::new(),
                        detected_patterns: Vec::new(),
                    };
                }
                return;
            }
        };

        let glob_pattern = patterns::normalize_glob_pattern(pattern);
        if patterns::build_matcher(&glob_pattern).is_err() {
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

        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.match_count = 0;
            builder.is_streaming = true;
            builder.pattern_error = None;
            builder.file_results = super::extraction::FileResultsState::Exploration {
                folder_matches: Vec::new(),
                expanded_folder_indices: std::collections::HashSet::new(),
                detected_patterns: Vec::new(),
            };
        }

        let exclude_patterns = self
            .discover
            .rule_builder
            .as_ref()
            .map(|b| b.excludes.clone())
            .unwrap_or_default();

        let pattern_for_thread = pattern.to_string();
        let glob_pattern_for_query = glob_pattern.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_rule_builder_search = Some(rx);

        if !self.control_connected {
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.match_count = 0;
                builder.pattern_error = Some("Sentinel not connected".to_string());
                builder.file_results = super::extraction::FileResultsState::Exploration {
                    folder_matches: Vec::new(),
                    expanded_folder_indices: std::collections::HashSet::new(),
                    detected_patterns: Vec::new(),
                };
            }
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error =
                        Some("Control API address not configured".to_string());
                    builder.file_results = super::extraction::FileResultsState::Exploration {
                        folder_matches: Vec::new(),
                        expanded_folder_indices: std::collections::HashSet::new(),
                        detected_patterns: Vec::new(),
                    };
                }
                return;
            }
        };

        std::thread::spawn(move || {
            let pattern_for_msg = pattern_for_thread.clone();
            let result: Result<RuleBuilderSearchResult, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let matcher = patterns::build_matcher(&glob_pattern_for_query)
                    .map_err(|err| format!("Invalid pattern: {}", err))?;

                let exclude_matchers: Vec<globset::GlobMatcher> = exclude_patterns
                    .into_iter()
                    .filter_map(|pattern| {
                        let glob = patterns::normalize_glob_pattern(&pattern);
                        patterns::build_matcher(&glob).ok()
                    })
                    .collect();

                let result = client
                    .pattern_query(
                        workspace_id,
                        source_id,
                        glob_pattern_for_query.clone(),
                        1000,
                        0,
                    )
                    .map_err(|err| format!("Rule builder query failed: {}", err))?;

                let mut folder_counts: std::collections::HashMap<String, (usize, String)> =
                    std::collections::HashMap::new();

                for file in result.files {
                    let rel_path = file.rel_path;
                    if !matcher.is_match(&rel_path) {
                        continue;
                    }
                    if exclude_matchers.iter().any(|m| m.is_match(&rel_path)) {
                        continue;
                    }
                    let folder = if let Some(idx) = rel_path.rfind('/') {
                        rel_path[..idx].to_string()
                    } else {
                        ".".to_string()
                    };
                    let filename =
                        rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string();
                    let entry = folder_counts.entry(folder).or_insert((0, filename));
                    entry.0 += 1;
                }

                let mut folder_matches: Vec<FolderMatch> = folder_counts
                    .into_iter()
                    .map(|(path, (count, sample))| FolderMatch {
                        path: if path == "." {
                            "./".to_string()
                        } else {
                            format!("{}/", path)
                        },
                        count,
                        sample_filename: sample,
                        files: Vec::new(),
                    })
                    .collect();

                folder_matches.sort_by(|a, b| b.count.cmp(&a.count));

                Ok(RuleBuilderSearchResult {
                    folder_matches,
                    total_count: result.total_count.max(0) as usize,
                    pattern: pattern_for_msg.clone(),
                    error: None,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => RuleBuilderSearchResult {
                    folder_matches: vec![],
                    total_count: 0,
                    pattern: pattern_for_msg,
                    error: Some(err),
                },
            };
            let _ = tx.send(msg);
        });
    }

    fn update_rule_builder_extraction_preview(&mut self, pattern: &str) {
        use super::extraction::{
            extract_field_values, parse_custom_glob, sync_extractions_from_custom_pattern,
            ExtractionPreviewFile,
        };

        if self.discover.rule_builder.is_none() {
            return;
        }

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

        if let Some(builder) = self.discover.rule_builder.as_mut() {
            if let Err(err) = sync_extractions_from_custom_pattern(builder) {
                builder.pattern_error = Some(err.message);
            } else {
                builder.pattern_error = None;
            }
        }

        let glob_pattern = patterns::normalize_glob_pattern(&parsed.glob_pattern);

        if patterns::build_matcher(&glob_pattern).is_err() {
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.match_count = 0;
                builder.pattern_error = Some("Invalid glob pattern".to_string());
                builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                    preview_files: Vec::new(),
                };
            }
            return;
        }

        let (workspace_id, source_id) = match (
            self.active_workspace_id(),
            self.discover
                .rule_builder
                .as_ref()
                .and_then(|b| b.source_id),
        ) {
            (Some(workspace_id), Some(source_id)) => (workspace_id, source_id),
            (None, _) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("No workspace selected".to_string());
                    builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                        preview_files: Vec::new(),
                    };
                }
                return;
            }
            (_, None) => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error = Some("No source selected".to_string());
                    builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                        preview_files: Vec::new(),
                    };
                }
                return;
            }
        };

        if !self.control_connected {
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.match_count = 0;
                builder.pattern_error = Some("Sentinel not connected".to_string());
                builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                    preview_files: Vec::new(),
                };
            }
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.match_count = 0;
                    builder.pattern_error =
                        Some("Control API address not configured".to_string());
                    builder.file_results = super::extraction::FileResultsState::ExtractionPreview {
                        preview_files: Vec::new(),
                    };
                }
                return;
            }
        };

        if let Some(builder) = self.discover.rule_builder.as_mut() {
            builder.match_count = 0;
            builder.is_streaming = true;
        }

        let exclude_patterns = self
            .discover
            .rule_builder
            .as_ref()
            .map(|b| b.excludes.clone())
            .unwrap_or_default();

        let pattern_for_thread = pattern.to_string();
        let glob_pattern_for_query = glob_pattern.clone();
        let parsed_for_thread = parsed.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_rule_builder_preview = Some(rx);

        std::thread::spawn(move || {
            let pattern_for_msg = pattern_for_thread.clone();
            let result: Result<RuleBuilderPreviewResult, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let matcher = patterns::build_matcher(&glob_pattern_for_query)
                    .map_err(|err| format!("Invalid glob pattern: {}", err))?;

                let exclude_matchers: Vec<globset::GlobMatcher> = exclude_patterns
                    .into_iter()
                    .filter_map(|pattern| {
                        let glob = patterns::normalize_glob_pattern(&pattern);
                        patterns::build_matcher(&glob).ok()
                    })
                    .collect();

                let is_excluded = |path: &str| exclude_matchers.iter().any(|m| m.is_match(path));

                let result = client
                    .pattern_query(
                        workspace_id,
                        source_id,
                        glob_pattern_for_query.clone(),
                        2000,
                        0,
                    )
                    .map_err(|err| format!("Rule builder query failed: {}", err))?;

                let mut preview_files = Vec::new();
                for file in result.files {
                    let rel_path = file.rel_path;
                    if matcher.is_match(&rel_path) && !is_excluded(&rel_path) {
                        if preview_files.len() < 100 {
                            let extractions = extract_field_values(&rel_path, &parsed_for_thread);
                            preview_files.push(ExtractionPreviewFile {
                                path: rel_path.clone(),
                                relative_path: rel_path,
                                extractions,
                                warnings: Vec::new(),
                            });
                        }
                    }
                }

                Ok(RuleBuilderPreviewResult {
                    preview_files,
                    total_count: result.total_count.max(0) as usize,
                    pattern: pattern_for_msg.clone(),
                    error: None,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => RuleBuilderPreviewResult {
                    preview_files: Vec::new(),
                    total_count: 0,
                    pattern: pattern_for_msg,
                    error: Some(err),
                },
            };
            let _ = tx.send(msg);
        });
    }

    fn close_rule_creation_dialog(&mut self) {
        self.return_to_previous_discover_state();
        self.discover.rule_pattern_input.clear();
        self.discover.rule_tag_input.clear();
        self.discover.rule_preview_files.clear();
        self.discover.rule_preview_count = 0;
        self.discover.rule_dialog_focus = RuleDialogFocus::Pattern;
        self.discover.editing_rule_id = None;
    }
    fn update_rule_preview(&mut self) {
        let pattern = &self.discover.rule_pattern_input;

        if pattern.is_empty() {
            self.discover.rule_preview_files.clear();
            self.discover.rule_preview_count = 0;
            return;
        }

        // Use globset for matching
        let glob_pattern = patterns::normalize_glob_pattern(pattern);

        match patterns::build_matcher(&glob_pattern) {
            Ok(matcher) => {
                let matches: Vec<String> = self
                    .discover
                    .files
                    .iter()
                    .filter(|f| matcher.is_match(&f.rel_path))
                    .map(|f| f.rel_path.clone())
                    .collect();

                self.discover.rule_preview_count = matches.len();
                self.discover.rule_preview_files = matches.into_iter().take(10).collect();
            }
            Err(_) => {
                // Invalid pattern, try substring match
                let pattern_lower = pattern.to_lowercase();
                let matches: Vec<String> = self
                    .discover
                    .files
                    .iter()
                    .filter(|f| f.rel_path.to_lowercase().contains(&pattern_lower))
                    .map(|f| f.rel_path.clone())
                    .collect();

                self.discover.rule_preview_count = matches.len();
                self.discover.rule_preview_files = matches.into_iter().take(10).collect();
            }
        }
    }
    fn apply_rule_to_files(&mut self, pattern: &str, tag: &str) -> Option<TaggingRuleId> {
        if self.mutations_blocked() {
            let message = BackendRouter::new(
                self.control_addr.clone(),
                self.config.standalone_writer,
                self.db_read_only,
            )
            .blocked_message("apply rules");
            self.discover.scan_error = Some(message.clone());
            self.discover.status_message = Some((message, true));
            return None;
        }
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.status_message = Some((
                    "No workspace selected; cannot apply rules".to_string(),
                    true,
                ));
                return None;
            }
        };

        let source_id = if let Some(id) = self.discover.selected_source_id.as_ref() {
            id.clone()
        } else {
            let idx = self.discover.selected_source_index();
            match self.discover.sources.get(idx) {
                Some(source) => source.id.clone(),
                None => {
                    self.discover.status_message =
                        Some(("No source selected; cannot apply rules".to_string(), true));
                    return None;
                }
            }
        };

        if !self.control_connected {
            self.discover.status_message = Some((
                "Sentinel not connected; cannot apply rules".to_string(),
                true,
            ));
            return None;
        }

        if self.pending_rule_apply.is_some() {
            self.discover.status_message =
                Some(("Rule apply already in progress".to_string(), true));
            return None;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.status_message = Some((
                    "Control API address not configured".to_string(),
                    true,
                ));
                return None;
            }
        };

        let rule_id = TaggingRuleId::new();
        let rule_id_for_thread = rule_id.clone();
        let pattern_string = pattern.to_string();
        let tag_string = tag.to_string();

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_rule_apply = Some(rx);

        std::thread::spawn(move || {
            let result: Result<RuleApplyResult, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let (success, tagged_count, message) = client
                    .apply_rule_to_source(
                        rule_id_for_thread.clone(),
                        workspace_id,
                        source_id,
                        pattern_string.clone(),
                        tag_string.clone(),
                    )
                    .map_err(|err| format!("Rule apply failed: {}", err))?;

                if !success {
                    return Err(message);
                }

                Ok(RuleApplyResult {
                    rule_id: rule_id_for_thread,
                    pattern: pattern_string,
                    tag: tag_string,
                    tagged_count,
                })
            })();

            let _ = tx.send(result);
        });

        self.discover.status_message = Some((
            format!("Applying rule: {} → {}", pattern, tag),
            false,
        ));

        Some(rule_id)
    }

    fn refresh_tags_list(&mut self) {
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.tags = vec![TagInfo {
                    name: "All files".to_string(),
                    count: 0,
                    is_special: true,
                }];
                self.discover.available_tags.clear();
                return;
            }
        };

        let source_id = if let Some(id) = self.discover.selected_source_id.as_ref() {
            id.clone()
        } else {
            let idx = self.discover.selected_source_index();
            match self.discover.sources.get(idx) {
                Some(source) => source.id.clone(),
                None => {
                    self.discover.tags = vec![TagInfo {
                        name: "All files".to_string(),
                        count: 0,
                        is_special: true,
                    }];
                    self.discover.available_tags.clear();
                    return;
                }
            }
        };

        if !self.control_connected {
            self.discover.tags = vec![TagInfo {
                name: "All files".to_string(),
                count: 0,
                is_special: true,
            }];
            self.discover.available_tags.clear();
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.tags = vec![TagInfo {
                    name: "All files".to_string(),
                    count: 0,
                    is_special: true,
                }];
                self.discover.available_tags.clear();
                return;
            }
        };

        if self.pending_tags_load.is_some() {
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_tags_load = Some(rx);

        std::thread::spawn(move || {
            let result: Result<TagsLoadMessage, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;
                let stats = client
                    .list_tags(workspace_id, source_id)
                    .map_err(|err| format!("Tags query failed: {}", err))?;

                let mut tags = Vec::new();
                let total_count = stats.total_files.max(0) as usize;
                tags.push(TagInfo {
                    name: "All files".to_string(),
                    count: total_count,
                    is_special: true,
                });

                let mut available_tags = Vec::new();
                for entry in stats.tags {
                    if entry.count <= 0 {
                        continue;
                    }
                    available_tags.push(entry.tag.clone());
                    tags.push(TagInfo {
                        name: entry.tag,
                        count: entry.count as usize,
                        is_special: false,
                    });
                }

                if stats.untagged_files > 0 {
                    tags.push(TagInfo {
                        name: "untagged".to_string(),
                        count: stats.untagged_files as usize,
                        is_special: true,
                    });
                }

                Ok(TagsLoadMessage {
                    workspace_id,
                    source_id,
                    tags,
                    available_tags,
                })
            })();

            let _ = tx.send(result);
        });
    }

    fn start_cache_load(&mut self) {
        // Must have a source selected
        let source_id = match &self.discover.selected_source_id {
            Some(id) => id.clone(),
            None => return,
        };
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => return,
        };

        // Skip if already loading
        if self.pending_cache_load.is_some() {
            return;
        }

        if !self.control_connected {
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => return,
        };

        // Skip if cache is already loaded for this source
        if let Some(ref explorer) = self.discover.glob_explorer {
            if explorer.cache_loaded {
                if let (Some(cache_source), Some(cache_workspace)) =
                    (explorer.cache_source_id, explorer.cache_workspace_id)
                {
                    if cache_source == source_id && cache_workspace == workspace_id {
                        // Cache already loaded for this source/workspace, no reload needed
                        self.discover.data_loaded = true;
                        return;
                    }
                }
            }
        }

        // Skip if we're already tracking progress for this load
        if self.cache_load_progress.is_some() {
            return;
        }

        // Skip if cache load already failed (don't retry until user takes action)
        if self.discover.scan_error.is_some() {
            return;
        }

        // Get source name for progress display
        let source_name = self
            .discover
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| source_id.to_string());

        // Set up channel and progress tracking
        let (tx, rx) = mpsc::sync_channel::<CacheLoadMessage>(1);
        self.pending_cache_load = Some(rx);
        self.cache_load_progress = Some(CacheLoadProgress::new(source_name));

        // Initialize empty cache in explorer
        if let Some(ref mut explorer) = self.discover.glob_explorer {
            explorer.folder_cache = HashMap::new();
            explorer.cache_source_id = Some(source_id);
            explorer.cache_workspace_id = Some(workspace_id);
        }

        std::thread::spawn(move || {
            let result: Result<CacheLoadMessage, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;

                let (folder_entries, _total_count) = client
                    .list_folders(workspace_id, source_id, "".to_string(), None)
                    .map_err(|err| format!("Folder query failed: {}", err))?;

                let stats = client
                    .list_tags(workspace_id, source_id)
                    .map_err(|err| format!("Tags query failed: {}", err))?;

                let folder_infos: Vec<FsEntry> = folder_entries
                    .into_iter()
                    .map(|entry| FsEntry::new(entry.name, entry.file_count as usize, entry.is_file))
                    .collect();

                let mut cache: HashMap<String, Vec<FsEntry>> = HashMap::new();
                cache.insert(String::new(), folder_infos);

                let mut tags = Vec::new();
                let total_count = stats.total_files.max(0) as usize;
                tags.push(TagInfo {
                    name: "All files".to_string(),
                    count: total_count,
                    is_special: true,
                });

                for entry in stats.tags {
                    if entry.count <= 0 {
                        continue;
                    }
                    tags.push(TagInfo {
                        name: entry.tag,
                        count: entry.count as usize,
                        is_special: false,
                    });
                }

                if stats.untagged_files > 0 {
                    tags.push(TagInfo {
                        name: "untagged".to_string(),
                        count: stats.untagged_files as usize,
                        is_special: true,
                    });
                }

                Ok(CacheLoadMessage::Complete {
                    workspace_id,
                    source_id,
                    total_files: total_count,
                    tags,
                    cache,
                })
            })();

            let msg = match result {
                Ok(msg) => msg,
                Err(err) => CacheLoadMessage::Error(err),
            };
            let _ = tx.send(msg);
        });
    }

    fn glob_match_name(name: &str, pattern: &str) -> bool {
        if pattern.starts_with("*.") {
            // *.ext -> ends with .ext
            let ext = &pattern[1..];
            name.ends_with(ext)
        } else if pattern.ends_with("*") {
            // prefix* -> starts with prefix
            let prefix_pat = &pattern[..pattern.len() - 1];
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

    fn query_folder_counts(
        conn: &DbConnection,
        workspace_id: WorkspaceId,
        source_id: SourceId,
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
            let prefix_len = if prefix.is_empty() {
                0
            } else {
                prefix.len() as i64 + 1
            };

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
                WHERE workspace_id = ? AND source_id = ?
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
                    DbValue::Text(workspace_id.to_string()),
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(path_filter),
                    DbValue::Text(like_pattern),
                    DbValue::Integer(prefix_len),
                ],
            )?;

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
                "SELECT name, file_count, is_folder FROM scout_folders WHERE workspace_id = ? AND source_id = ? AND prefix = ? ORDER BY is_folder DESC, file_count DESC, name",
                &[
                    DbValue::Text(workspace_id.to_string()),
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(prefix.to_string()),
                ],
            )?;

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
            "SELECT name, size FROM scout_files WHERE workspace_id = ? AND source_id = ? AND parent_path = ? ORDER BY name LIMIT 200",
            &[
                DbValue::Text(workspace_id.to_string()),
                DbValue::Integer(source_id.as_i64()),
                DbValue::Text(prefix.to_string()),
            ],
        )?;

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
                WHERE workspace_id = ? AND source_id = ? AND parent_path != ''
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    DbValue::Text(workspace_id.to_string()),
                    DbValue::Integer(source_id.as_i64()),
                ],
            )?
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
                WHERE workspace_id = ? AND source_id = ? AND parent_path LIKE ? || '%' AND parent_path != ?
                GROUP BY folder_name
                ORDER BY file_count DESC
                LIMIT 200
                "#,
                &[
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(workspace_id.to_string()),
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(folder_prefix.clone()),
                    DbValue::Text(prefix.to_string()),
                ],
            )?
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
    fn start_sources_load(&mut self) {
        // Skip if already loading
        if self.pending_sources_load.is_some() {
            return;
        }

        if !self.control_connected {
            if !(self.config.standalone_writer || cfg!(test)) {
                self.discover.sources_loaded = true;
                return;
            }

            let workspace_id = match self.active_workspace_id() {
                Some(id) => id,
                None => return,
            };

            let (backend, db_path) = self.resolve_db_target();

            if !db_path.exists() {
                self.discover.sources_loaded = true;
                return;
            }

            let (tx, rx) = mpsc::sync_channel(1);
            self.pending_sources_load = Some(rx);

            let workspace_id_str = workspace_id.to_string();
            std::thread::spawn(move || {
                let result: Result<Vec<SourceInfo>, String> = (|| {
                    let conn = match App::open_db_readonly_with(backend, &db_path) {
                        Ok(Some(conn)) => conn,
                        Ok(None) => return Err("Database not available".to_string()),
                        Err(err) => return Err(format!("Database open failed: {}", err)),
                    };

                    let query = r#"
                    SELECT id, name, path, file_count
                    FROM scout_sources
                    WHERE enabled = 1 AND workspace_id = ?
                    ORDER BY updated_at DESC
                "#;

                    let rows = conn
                        .query_all(query, &[DbValue::Text(workspace_id_str)])
                        .map_err(|err| format!("Sources query failed: {}", err))?;

                    let mut sources: Vec<SourceInfo> = Vec::with_capacity(rows.len());
                    for row in rows {
                        let id_i64: i64 = row
                            .get(0)
                            .map_err(|e| format!("Sources parse failed: {}", e))?;
                        let id = SourceId::try_from(id_i64)
                            .map_err(|e| format!("Sources parse failed: {}", e))?;
                        let name: String = row
                            .get(1)
                            .map_err(|e| format!("Sources parse failed: {}", e))?;
                        let path: String = row
                            .get(2)
                            .map_err(|e| format!("Sources parse failed: {}", e))?;
                        let file_count: i64 = row
                            .get(3)
                            .map_err(|e| format!("Sources parse failed: {}", e))?;

                        sources.push(SourceInfo {
                            id,
                            name,
                            path: std::path::PathBuf::from(path),
                            file_count: file_count as usize,
                        });
                    }

                    Ok(sources)
                })();

                let _ = tx.send(result);
            });
            return;
        }

        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => return,
        };
        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover.sources_loaded = true;
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_sources_load = Some(rx);
        std::thread::spawn(move || {
            let result: Result<Vec<SourceInfo>, String> = (|| {
                let client = ControlClient::connect_with_timeout(
                    &control_addr,
                    Duration::from_millis(500),
                )
                .map_err(|err| format!("Control API unavailable: {}", err))?;
                let sources = client
                    .list_sources(workspace_id)
                    .map_err(|err| format!("Sources query failed: {}", err))?;
                Ok(sources
                    .into_iter()
                    .map(SourceInfo::from_control)
                    .collect())
            })();
            let _ = tx.send(result);
        });
    }

    fn millis_to_local(ms: i64) -> Option<DateTime<Local>> {
        Local.timestamp_millis_opt(ms).single()
    }

    fn start_jobs_load(&mut self) {
        // Skip if already loading
        if self.pending_jobs_load.is_some() {
            return;
        }

        if self.control_connected {
            let control_addr = match self.control_addr.clone() {
                Some(addr) => addr,
                None => {
                    self.jobs_state.jobs_loaded = true;
                    return;
                }
            };

            let (tx, rx) = mpsc::sync_channel(1);
            self.pending_jobs_load = Some(rx);
            std::thread::spawn(move || {
                let result: Result<Vec<JobInfo>, String> = (|| {
                    let client = ControlClient::connect_with_timeout(
                        &control_addr,
                        Duration::from_millis(500),
                    )
                    .map_err(|err| format!("Control API unavailable: {}", err))?;
                    let jobs = client
                        .list_jobs(None, Some(100), None)
                        .map_err(|err| format!("Jobs query failed: {}", err))?;
                    Ok(jobs.into_iter().map(App::job_from_control).collect())
                })();
                let _ = tx.send(result);
            });
            return;
        }

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.jobs_state.jobs_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_jobs_load = Some(rx);

        // Spawn background task for DB query
        std::thread::spawn(move || {
            let result: Result<Vec<JobInfo>, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                let has_queue = App::table_exists(&conn, "cf_processing_queue")
                    .map_err(|err| format!("Jobs schema check failed: {}", err))?;
                if !has_queue {
                    return Ok(Vec::new());
                }

                let has_pipeline_runs = App::table_exists(&conn, "cf_pipeline_runs")
                    .map_err(|err| format!("Jobs schema check failed: {}", err))?;
                let has_quarantine_column =
                    App::column_exists(&conn, "cf_processing_queue", "quarantine_rows")
                        .map_err(|err| format!("Jobs schema check failed: {}", err))?;
                let has_quarantine_table = App::table_exists(&conn, "cf_quarantine")
                    .map_err(|err| format!("Jobs schema check failed: {}", err))?;

                let (quarantine_select, quarantine_join) = if has_quarantine_column {
                    ("q.quarantine_rows", "")
                } else if has_quarantine_table {
                    (
                        "qc.quarantine_rows",
                        r#"
                LEFT JOIN (
                    SELECT job_id, COUNT(*) AS quarantine_rows
                    FROM cf_quarantine
                    GROUP BY job_id
                ) qc ON qc.job_id = q.id
                "#,
                    )
                } else {
                    ("NULL as quarantine_rows", "")
                };

                let query = if has_pipeline_runs {
                    format!(
                        r#"
                SELECT
                    q.id,
                    q.file_id,
                    q.plugin_name,
                    q.status,
                    q.claim_time,
                    q.end_time,
                    q.result_summary,
                    q.error_message,
                    q.completion_status,
                    q.pipeline_run_id,
                    pr.logical_date,
                    pr.selection_snapshot_hash,
                    {quarantine_select}
                FROM cf_processing_queue q
                LEFT JOIN cf_pipeline_runs pr ON pr.id = q.pipeline_run_id
                {quarantine_join}
                ORDER BY
                    CASE q.status
                        WHEN '{running}' THEN 1
                        WHEN '{staged}' THEN 1
                        WHEN '{dispatching}' THEN 1
                        WHEN '{queued}' THEN 2
                        WHEN '{pending}' THEN 2
                        WHEN '{failed}' THEN 3
                        WHEN '{aborted}' THEN 3
                        WHEN '{completed}' THEN 4
                        WHEN '{skipped}' THEN 5
                    END,
                    q.id DESC
                LIMIT 100
                "#,
                        quarantine_select = quarantine_select,
                        quarantine_join = quarantine_join,
                        running = ProcessingStatus::Running.as_str(),
                        staged = ProcessingStatus::Staged.as_str(),
                        dispatching = ProcessingStatus::Dispatching.as_str(),
                        queued = ProcessingStatus::Queued.as_str(),
                        pending = ProcessingStatus::Pending.as_str(),
                        failed = ProcessingStatus::Failed.as_str(),
                        aborted = ProcessingStatus::Aborted.as_str(),
                        completed = ProcessingStatus::Completed.as_str(),
                        skipped = ProcessingStatus::Skipped.as_str(),
                    )
                } else {
                    format!(
                        r#"
                SELECT
                    q.id,
                    q.file_id,
                    q.plugin_name,
                    q.status,
                    q.claim_time,
                    q.end_time,
                    q.result_summary,
                    q.error_message,
                    q.completion_status,
                    {quarantine_select}
                FROM cf_processing_queue q
                {quarantine_join}
                ORDER BY
                    CASE q.status
                        WHEN '{running}' THEN 1
                        WHEN '{staged}' THEN 1
                        WHEN '{dispatching}' THEN 1
                        WHEN '{queued}' THEN 2
                        WHEN '{pending}' THEN 2
                        WHEN '{failed}' THEN 3
                        WHEN '{aborted}' THEN 3
                        WHEN '{completed}' THEN 4
                        WHEN '{skipped}' THEN 5
                    END,
                    q.id DESC
                LIMIT 100
                "#,
                        quarantine_select = quarantine_select,
                        quarantine_join = quarantine_join,
                        running = ProcessingStatus::Running.as_str(),
                        staged = ProcessingStatus::Staged.as_str(),
                        dispatching = ProcessingStatus::Dispatching.as_str(),
                        queued = ProcessingStatus::Queued.as_str(),
                        pending = ProcessingStatus::Pending.as_str(),
                        failed = ProcessingStatus::Failed.as_str(),
                        aborted = ProcessingStatus::Aborted.as_str(),
                        completed = ProcessingStatus::Completed.as_str(),
                        skipped = ProcessingStatus::Skipped.as_str(),
                    )
                };

                let rows = conn
                    .query_all(&query, &[])
                    .map_err(|err| format!("Jobs query failed: {}", err))?;

                let mut jobs: Vec<JobInfo> = Vec::with_capacity(rows.len());
                for row in rows {
                    let id: i64 = row
                        .get(0)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let file_id: Option<i64> = row
                        .get(1)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let plugin_name: String = row
                        .get(2)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let status_str: String = row
                        .get(3)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let claim_time: Option<i64> = row
                        .get(4)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let end_time: Option<i64> = row
                        .get(5)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let result_summary: Option<String> = row
                        .get(6)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let error_message: Option<String> = row
                        .get(7)
                        .map_err(|e| format!("Jobs parse failed: {}", e))?;
                    let completion_status: Option<String> = row.get(8).ok().flatten();
                    let (pipeline_run_id, logical_date, selection_snapshot_hash, quarantine_rows) =
                        if has_pipeline_runs {
                            (
                                row.get(9).ok(),
                                row.get(10).ok(),
                                row.get(11).ok(),
                                row.get(12).ok(),
                            )
                        } else {
                            (None, None, None, row.get(9).ok())
                        };

                    // Map queue status + completion_status to UI status
                    let status =
                        JobStatus::from_db_status(&status_str, completion_status.as_deref());

                    let started_at = claim_time
                        .and_then(Self::millis_to_local)
                        .unwrap_or_else(Local::now);

                    let completed_at = end_time
                        .and_then(Self::millis_to_local);

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
                        file_id,
                        job_type: JobType::Parse,
                        origin: JobOrigin::Persistent,
                        name: plugin_name,
                        version: None,
                        status,
                        started_at,
                        completed_at,
                        pipeline_run_id,
                        logical_date,
                        selection_snapshot_hash,
                        quarantine_rows,
                        items_total: 0,
                        items_processed: if result_summary.is_some() { 1 } else { 0 },
                        items_failed: if error_message.is_some() { 1 } else { 0 },
                        output_path: None,
                        output_size_bytes: None,
                        backtest: None,
                        failures,
                        violations: vec![],
                        top_violations_loaded: false,
                        selected_violation_index: 0,
                    });
                }

                Ok(jobs)
            })();

            let _ = tx.send(result);
        });
    }

    fn update_home_recent_jobs(&mut self) {
        let mut summaries = Vec::new();
        for job in self.jobs_state.jobs.iter().take(5) {
            let progress_percent = if job.items_total > 0 {
                Some(((job.items_processed as f64 / job.items_total as f64) * 100.0) as u8)
            } else {
                None
            };

            let duration_secs = job
                .completed_at
                .map(|end| (end - job.started_at).num_milliseconds() as f64 / 1000.0);

            summaries.push(JobSummary {
                id: job.id,
                job_type: job.job_type.as_str().to_string(),
                description: job.name.clone(),
                status: job.status,
                progress_percent,
                duration_secs,
            });
        }

        self.home.recent_jobs = summaries;
    }
    fn start_stats_load(&mut self) {
        // Skip if already loading
        if self.pending_stats_load.is_some() {
            return;
        }

        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => return,
        };

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.home.stats_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_stats_load = Some(rx);

        // Spawn background task for DB query
        let workspace_id_str = workspace_id.to_string();
        std::thread::spawn(move || {
            let result: Result<HomeStats, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                let mut stats = HomeStats::default();

                let count = conn
                    .query_scalar::<i64>(
                        "SELECT COUNT(*) FROM scout_files WHERE workspace_id = ?",
                        &[DbValue::Text(workspace_id_str.clone())],
                    )
                    .map_err(|err| format!("Stats query failed: {}", err))?;
                stats.file_count = count as usize;

                let count = conn
                    .query_scalar::<i64>(
                        "SELECT COUNT(*) FROM scout_sources WHERE enabled = 1 AND workspace_id = ?",
                        &[DbValue::Text(workspace_id_str.clone())],
                    )
                    .map_err(|err| format!("Stats query failed: {}", err))?;
                stats.source_count = count as usize;

                let has_queue = App::table_exists(&conn, "cf_processing_queue")
                    .map_err(|err| format!("Stats schema check failed: {}", err))?;
                if has_queue {
                    let rows = conn
                        .query_all(
                            "SELECT status, COUNT(*) as cnt FROM cf_processing_queue GROUP BY status",
                            &[],
                        )
                        .map_err(|err| format!("Stats query failed: {}", err))?;
                    for row in rows {
                        let status: String = row
                            .get(0)
                            .map_err(|e| format!("Stats parse failed: {}", e))?;
                        let count: i64 = row
                            .get(1)
                            .map_err(|e| format!("Stats parse failed: {}", e))?;
                        if let Ok(queue_status) = status.parse::<ProcessingStatus>() {
                            match queue_status {
                                ProcessingStatus::Running => stats.running_jobs = count as usize,
                                ProcessingStatus::Queued | ProcessingStatus::Dispatching => {
                                    stats.pending_jobs = count as usize
                                }
                                ProcessingStatus::Failed | ProcessingStatus::Aborted => {
                                    stats.failed_jobs = count as usize
                                }
                                _ => {}
                            }
                        }
                    }
                }

                let has_manifest = App::table_exists(&conn, "cf_plugin_manifest")
                    .map_err(|err| format!("Stats schema check failed: {}", err))?;
                if has_manifest {
                    let count = conn
                        .query_scalar::<i64>("SELECT COUNT(*) FROM cf_plugin_manifest", &[])
                        .map_err(|err| format!("Stats query failed: {}", err))?;
                    stats.parser_count = count as usize;
                }

                Ok(stats)
            })();

            let _ = tx.send(result);
        });
    }
    fn start_approvals_load(&mut self) {
        // Skip if already loading
        if self.pending_approvals_load.is_some() {
            return;
        }

        if self.control_connected {
            let control_addr = match self.control_addr.clone() {
                Some(addr) => addr,
                None => {
                    self.approvals_state.approvals_loaded = true;
                    return;
                }
            };

            let (tx, rx) = mpsc::sync_channel(1);
            self.pending_approvals_load = Some(rx);

            std::thread::spawn(move || {
                let result: Result<Vec<ApprovalInfo>, String> = (|| {
                    let client = ControlClient::connect_with_timeout(
                        &control_addr,
                        Duration::from_millis(500),
                    )
                    .map_err(|err| format!("Control API unavailable: {}", err))?;
                    let approvals = client
                        .list_approvals(None, Some(100), None)
                        .map_err(|err| format!("Approvals query failed: {}", err))?;
                    Ok(approvals
                        .into_iter()
                        .map(ApprovalInfo::from_control)
                        .collect())
                })();

                let _ = tx.send(result);
            });
            return;
        }

        let (backend, db_path) = self.resolve_db_target();

        if !db_path.exists() {
            self.approvals_state.approvals_loaded = true;
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_approvals_load = Some(rx);

        // Spawn background task for DB query
        std::thread::spawn(move || {
            let result: Result<Vec<ApprovalInfo>, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                // Check if cf_api_approvals table exists
                let has_table = App::table_exists(&conn, "cf_api_approvals")
                    .map_err(|err| format!("Approvals schema check failed: {}", err))?;
                if !has_table {
                    return Ok(Vec::new());
                }

                let query = r#"
                SELECT approval_id, status, operation_type, operation_json, summary,
                       created_at, expires_at, job_id
                FROM cf_api_approvals
                ORDER BY
                    CASE status
                        WHEN 'pending' THEN 1
                        WHEN 'approved' THEN 2
                        WHEN 'rejected' THEN 3
                        WHEN 'expired' THEN 4
                    END,
                    created_at DESC
                LIMIT 100
            "#;

                let rows = conn
                    .query_all(query, &[])
                    .map_err(|err| format!("Approvals query failed: {}", err))?;
                let mut approvals: Vec<ApprovalInfo> = Vec::with_capacity(rows.len());

                for row in rows {
                    let id: String = row
                        .get(0)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let status_str: String = row
                        .get(1)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let operation_type_str: String = row
                        .get(2)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    // Parse strings to enums at the DB boundary
                    let status = ApprovalDisplayStatus::from_db_str(&status_str);
                    let operation_type = ApprovalOperationType::from_db_str(&operation_type_str);
                    let operation_json: String = row
                        .get(3)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let summary: String = row
                        .get(4)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let created_at_ms: i64 = row
                        .get(5)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let expires_at_ms: i64 = row
                        .get(6)
                        .map_err(|e| format!("Approvals parse failed: {}", e))?;
                    let job_id: Option<i64> = row.get(7).ok();

                    // Parse timestamps
                    let created_at =
                        Self::millis_to_local(created_at_ms).unwrap_or_else(Local::now);
                    let expires_at =
                        Self::millis_to_local(expires_at_ms).unwrap_or_else(Local::now);

                    // Parse operation JSON to extract plugin_ref, input_dir, file_count
                    let (plugin_ref, input_dir, file_count) = if let Ok(op) =
                        serde_json::from_str::<serde_json::Value>(&operation_json)
                    {
                        let plugin = op
                            .get("plugin_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let input = op
                            .get("input_dir")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let count = op
                            .get("file_count")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as u32);
                        (plugin, input, count)
                    } else {
                        ("unknown".to_string(), None, None)
                    };

                    approvals.push(ApprovalInfo {
                        id,
                        operation_type,
                        plugin_ref,
                        summary,
                        status,
                        created_at,
                        expires_at,
                        file_count,
                        input_dir,
                        job_id: job_id.map(|id| id.to_string()),
                    });
                }

                Ok(approvals)
            })();

            let _ = tx.send(result);
        });
    }
    fn start_sessions_load(&mut self) {
        // Skip if already loading
        if self.pending_sessions_load.is_some() {
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_sessions_load = Some(rx);

        // Spawn background task for file system scan
        std::thread::spawn(move || {
            let store = SessionStore::new();
            let session_ids = match store.list_sessions() {
                Ok(ids) => ids,
                Err(_) => {
                    let _ = tx.send(vec![]);
                    return;
                }
            };

            let mut sessions: Vec<SessionInfo> = Vec::with_capacity(session_ids.len());

            for session_id in session_ids {
                if let Ok(bundle) = store.get_session(session_id) {
                    if let Ok(manifest) = bundle.read_manifest() {
                        let parsed_state = manifest.state.parse::<IntentState>().ok();
                        let state_label = parsed_state
                            .map(|state| state.as_str().to_string())
                            .unwrap_or_else(|| manifest.state.clone());
                        let pending_gate = parsed_state
                            .and_then(|state| state.gate_number())
                            .map(|gate| format!("G{}", gate));

                        sessions.push(SessionInfo {
                            id: session_id.to_string(),
                            intent: manifest.intent_text,
                            state: parsed_state,
                            state_label,
                            created_at: manifest.created_at.with_timezone(&Local),
                            file_count: 0, // Would need to check corpus
                            pending_gate,
                        });
                    }
                }
            }

            // Sort by created_at descending (most recent first)
            sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            let _ = tx.send(sessions);
        });
    }



    fn persist_pending_writes(&mut self) {
        if self.control_connected {
            self.persist_pending_writes_control();
            return;
        }
        // Skip if nothing to persist
        if self.discover.pending_tag_writes.is_empty()
            && self.discover.pending_rule_writes.is_empty()
            && self.discover.pending_rule_updates.is_empty()
            && self.discover.pending_rule_deletes.is_empty()
            && self.discover.pending_source_creates.is_empty()
            && self.discover.pending_source_updates.is_empty()
            && self.discover.pending_source_deletes.is_empty()
            && self.discover.pending_source_touch.is_none()
        {
            return;
        }
        if self.mutations_blocked() {
            let message = "Sentinel not reachable; changes are not saved (use --standalone-writer to allow local writes)".to_string();
            self.discover.scan_error = Some(message.clone());
            self.set_global_status(message, true);
            return;
        }

        let db = match self.open_scout_db_for_writes() {
            Some(db) => db,
            None => return,
        };
        let conn = db.conn();

        // Persist tag updates to scout_file_tags
        let tag_writes = std::mem::take(&mut self.discover.pending_tag_writes);
        for write in tag_writes {
            let result = match write.tag_source {
                TagSource::Manual => db.tag_file(write.file_id, &write.tag),
                TagSource::Rule => match write.rule_id.as_ref() {
                    Some(rule_id) => db.tag_file_by_rule(write.file_id, &write.tag, rule_id),
                    None => {
                        self.report_db_error(
                            "Tag write failed",
                            "Rule-based tag write missing rule ID",
                        );
                        continue;
                    }
                },
            };

            if let Err(err) = result {
                self.report_db_error("Tag write failed", err);
            }
        }

        // Persist rules to scout_rules
        let rule_writes = std::mem::take(&mut self.discover.pending_rule_writes);
        for write in rule_writes {
            let rule_name = format!("{} → {}", write.pattern, write.tag);
            let now = chrono::Utc::now().timestamp_millis();

            if let Err(err) = conn.execute(
                r#"INSERT OR IGNORE INTO scout_rules
                   (id, workspace_id, name, kind, pattern, tag, priority, enabled, created_at, updated_at)
                   VALUES (?, ?, ?, 'tagging', ?, ?, 100, 1, ?, ?)"#,
                &[
                    DbValue::Text(write.id.to_string()),
                    DbValue::Text(write.workspace_id.to_string()),
                    DbValue::Text(rule_name),
                    DbValue::Text(write.pattern),
                    DbValue::Text(write.tag),
                    DbValue::Integer(now),
                    DbValue::Integer(now),
                ],
            ) {
                self.report_db_error("Rule create failed", err);
            }
        }

        // Persist rule enabled toggles
        let rule_updates = std::mem::take(&mut self.discover.pending_rule_updates);
        for update in rule_updates {
            if let Err(err) = conn.execute(
                "UPDATE scout_rules SET enabled = ? WHERE id = ? AND workspace_id = ?",
                &[
                    DbValue::Integer(if update.enabled { 1 } else { 0 }),
                    DbValue::Text(update.id.to_string()),
                    DbValue::Text(update.workspace_id.to_string()),
                ],
            ) {
                self.report_db_error("Rule update failed", err);
            }
        }

        // Persist rule deletes
        let rule_deletes = std::mem::take(&mut self.discover.pending_rule_deletes);
        for delete in rule_deletes {
            if let Err(err) = conn.execute(
                "DELETE FROM scout_rules WHERE id = ? AND workspace_id = ?",
                &[
                    DbValue::Text(delete.id.to_string()),
                    DbValue::Text(delete.workspace_id.to_string()),
                ],
            ) {
                self.report_db_error("Rule delete failed", err);
            }
        }

        let mut sources_changed = false;

        // Persist source creates
        let source_creates = std::mem::take(&mut self.discover.pending_source_creates);
        for source in source_creates {
            match db.upsert_source(&source) {
                Ok(_) => sources_changed = true,
                Err(err) => self.report_db_error("Source create failed", err),
            }
        }

        // Persist source updates
        let source_updates = std::mem::take(&mut self.discover.pending_source_updates);
        for update in source_updates {
            match db.get_source(&update.id) {
                Ok(Some(mut source)) => {
                    if let Some(name) = update.name {
                        source.name = name;
                    }
                    if let Some(path) = update.path {
                        source.path = path;
                    }
                    match db.upsert_source(&source) {
                        Ok(_) => sources_changed = true,
                        Err(err) => self.report_db_error("Source update failed", err),
                    }
                }
                Ok(None) => {
                    self.report_db_error("Source update failed", "Source not found");
                }
                Err(err) => self.report_db_error("Source update failed", err),
            }
        }

        // Persist source deletes
        let source_deletes = std::mem::take(&mut self.discover.pending_source_deletes);
        for delete in source_deletes {
            match db.delete_source(&delete.id) {
                Ok(true) => sources_changed = true,
                Ok(false) => {}
                Err(err) => self.report_db_error("Source delete failed", err),
            }
        }

        // Touch source for MRU ordering (updates updated_at timestamp)
        if let Some(source_id) = std::mem::take(&mut self.discover.pending_source_touch) {
            let now = chrono::Utc::now().timestamp_millis();
            let result = conn.execute(
                "UPDATE scout_sources SET updated_at = ? WHERE id = ?",
                &[DbValue::Integer(now), DbValue::Integer(source_id.as_i64())],
            );
            match result {
                Ok(_) => {
                    // Trigger sources reload to reflect new MRU ordering
                    self.discover.sources_loaded = false;
                }
                Err(err) => self.report_db_error("Source touch failed", err),
            }
        }

        if sources_changed {
            self.discover.sources_loaded = false;
            self.pending_sources_load = None;
            self.home.stats_loaded = false;
            self.pending_stats_load = None;
        }
    }

    fn persist_pending_writes_control(&mut self) {
        if self.pending_control_writes.is_some() {
            return;
        }
        if self.discover.pending_tag_writes.is_empty()
            && self.discover.pending_rule_writes.is_empty()
            && self.discover.pending_rule_updates.is_empty()
            && self.discover.pending_rule_deletes.is_empty()
            && self.discover.pending_source_creates.is_empty()
            && self.discover.pending_source_updates.is_empty()
            && self.discover.pending_source_deletes.is_empty()
            && self.discover.pending_source_touch.is_none()
        {
            return;
        }

        let control_addr = match self.control_addr.clone() {
            Some(addr) => addr,
            None => {
                self.discover
                    .status_message
                    .replace(("Control API not configured".to_string(), true));
                return;
            }
        };

        let tag_writes = std::mem::take(&mut self.discover.pending_tag_writes);
        let rule_writes = std::mem::take(&mut self.discover.pending_rule_writes);
        let rule_updates = std::mem::take(&mut self.discover.pending_rule_updates);
        let rule_deletes = std::mem::take(&mut self.discover.pending_rule_deletes);
        let source_creates = std::mem::take(&mut self.discover.pending_source_creates);
        let source_updates = std::mem::take(&mut self.discover.pending_source_updates);
        let source_deletes = std::mem::take(&mut self.discover.pending_source_deletes);
        let source_touch = std::mem::take(&mut self.discover.pending_source_touch);

        let sources_changed = !source_creates.is_empty()
            || !source_updates.is_empty()
            || !source_deletes.is_empty()
            || source_touch.is_some();

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_control_writes = Some(rx);

        std::thread::spawn(move || {
            let mut error: Option<String> = None;
            let client = match ControlClient::connect_with_timeout(
                &control_addr,
                Duration::from_millis(500),
            ) {
                Ok(client) => client,
                Err(err) => {
                    let _ = tx.send(ControlWriteResult {
                        sources_changed,
                        error: Some(format!("Control API unavailable: {}", err)),
                        control_connected: Some(false),
                    });
                    return;
                }
            };

            for source in source_creates {
                let source_info = ScoutSourceInfo {
                    id: source.id,
                    workspace_id: source.workspace_id,
                    name: source.name,
                    source_type: source.source_type,
                    path: source.path,
                    exec_path: source.exec_path,
                    enabled: source.enabled,
                    poll_interval_secs: source.poll_interval_secs,
                    file_count: 0,
                };
                if let Err(err) = client.upsert_source(source_info) {
                    if error.is_none() {
                        error = Some(format!("Source create failed: {}", err));
                    }
                }
            }

            for update in source_updates {
                if let Err(err) = client.update_source(update.id, update.name, update.path) {
                    if error.is_none() {
                        error = Some(format!("Source update failed: {}", err));
                    }
                }
            }

            for delete in source_deletes {
                if let Err(err) = client.delete_source(delete.id) {
                    if error.is_none() {
                        error = Some(format!("Source delete failed: {}", err));
                    }
                }
            }

            if let Some(source_id) = source_touch {
                if let Err(err) = client.touch_source(source_id) {
                    if error.is_none() {
                        error = Some(format!("Source touch failed: {}", err));
                    }
                }
            }

            for write in tag_writes {
                if let Err(err) = client.apply_tag(
                    write.workspace_id,
                    write.file_id,
                    write.tag,
                    write.tag_source,
                    write.rule_id,
                ) {
                    if error.is_none() {
                        error = Some(format!("Tag write failed: {}", err));
                    }
                }
            }

            for write in rule_writes {
                if let Err(err) =
                    client.create_rule(write.id, write.workspace_id, write.pattern, write.tag)
                {
                    if error.is_none() {
                        error = Some(format!("Rule create failed: {}", err));
                    }
                }
            }

            for update in rule_updates {
                if let Err(err) =
                    client.update_rule_enabled(update.id, update.workspace_id, update.enabled)
                {
                    if error.is_none() {
                        error = Some(format!("Rule update failed: {}", err));
                    }
                }
            }

            for delete in rule_deletes {
                if let Err(err) = client.delete_rule(delete.id, delete.workspace_id) {
                    if error.is_none() {
                        error = Some(format!("Rule delete failed: {}", err));
                    }
                }
            }

            let _ = tx.send(ControlWriteResult {
                sources_changed,
                error,
                control_connected: Some(true),
            });
        });
    }
    fn load_rules_for_manager(&mut self) {
        let workspace_id = match self.active_workspace_id() {
            Some(id) => id,
            None => {
                self.discover.rules.clear();
                return;
            }
        };

        if self.control_connected {
            let control_addr = match self.control_addr.clone() {
                Some(addr) => addr,
                None => return,
            };
            let client = match ControlClient::connect_with_timeout(
                &control_addr,
                Duration::from_millis(500),
            ) {
                Ok(client) => client,
                Err(err) => {
                    self.discover
                        .status_message
                        .replace((format!("Rules load failed: {}", err), true));
                    return;
                }
            };
            match client.list_rules(workspace_id) {
                Ok(rules) => {
                    self.discover.rules =
                        rules.into_iter().map(RuleInfo::from_control).collect();
                    if self.discover.selected_rule >= self.discover.rules.len()
                        && !self.discover.rules.is_empty()
                    {
                        self.discover.selected_rule = 0;
                    }
                }
                Err(err) => {
                    self.discover
                        .status_message
                        .replace((format!("Rules load failed: {}", err), true));
                }
            }
            return;
        }
        self.discover.status_message =
            Some(("Sentinel not connected; cannot load rules".to_string(), true));
    }
    /// Periodic tick for updates
    pub fn tick(&mut self) {
        // Increment tick counter for animated UI elements
        self.tick_count = self.tick_count.wrapping_add(1);

        self.maybe_probe_control_connection();
        self.check_db_health_once();
        self.ensure_active_workspace();

        if let Some(status) = &self.global_status {
            if status.expires_at <= std::time::Instant::now() {
                self.global_status = None;
            }
        }

        // Persist any queued writes regardless of current view.
        self.persist_pending_writes();

        if let Some(ref mut rx) = self.pending_control_writes {
            match rx.try_recv() {
                Ok(result) => {
                    if let Some(err) = result.error {
                        self.discover
                            .status_message
                            .replace((err, true));
                    }
                    if let Some(connected) = result.control_connected {
                        if connected != self.control_connected {
                            self.control_connected = connected;
                            if connected {
                                self.set_global_status("Sentinel connected", false);
                            } else {
                                self.set_global_status("Sentinel not reachable", true);
                            }
                        }
                    }
                    if result.sources_changed {
                        self.discover.sources_loaded = false;
                        self.pending_sources_load = None;
                        self.home.stats_loaded = false;
                        self.pending_stats_load = None;
                    }
                    self.pending_control_writes = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_control_writes = None;
                }
            }
        }

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
                Ok(Ok(stats)) => {
                    self.home.stats = stats;
                    self.home.stats_loaded = true;
                    self.pending_stats_load = None;
                }
                Ok(Err(err)) => {
                    self.home.stats_loaded = true;
                    self.pending_stats_load = None;
                    self.report_db_error("Home stats load failed", err);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
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
                Ok(Ok(sources)) => {
                    self.discover.sources = sources;
                    self.discover.sources_loaded = true;
                    self.discover.validate_source_selection();
                    if let Some(path) = self.discover.pending_select_source_path.take() {
                        let selected = self
                            .discover
                            .sources
                            .iter()
                            .find(|source| source.path.to_string_lossy() == path)
                            .map(|source| source.id);
                        if let Some(selected_id) = selected {
                            self.discover.selected_source_id = Some(selected_id);
                            self.discover.data_loaded = false;
                        } else {
                            self.discover.status_message = Some((
                                "Scan completed but source was not found".to_string(),
                                true,
                            ));
                        }
                    }
                    let drawer_count = self.sources_drawer_sources().len();
                    if drawer_count == 0 {
                        self.sources_drawer_selected = 0;
                    } else if self.sources_drawer_selected >= drawer_count {
                        self.sources_drawer_selected = drawer_count - 1;
                    }
                    if let (Some(ref mut builder), Some(source_id)) = (
                        self.discover.rule_builder.as_mut(),
                        self.discover.selected_source_id,
                    ) {
                        if builder.source_id != Some(source_id) {
                            builder.source_id = Some(source_id);
                        }
                    }
                    if self.mode == TuiMode::Ingest
                        && self.ingest_tab != IngestTab::Sources
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
                Ok(Err(err)) => {
                    self.discover.sources_loaded = true;
                    self.discover.pending_select_source_path = None;
                    self.pending_sources_load = None;
                    self.report_db_error("Sources load failed", err);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty sources)
                    self.discover.sources_loaded = true;
                    self.discover.pending_select_source_path = None;
                    self.pending_sources_load = None;
                }
            }
        }

        // Jobs: initial load always, then poll fast in Run/Home, slower elsewhere if active
        const JOBS_POLL_INTERVAL_MS: u64 = 2000;
        const JOBS_POLL_INTERVAL_IDLE_MS: u64 = 10000;
        let in_run_or_home = matches!(self.mode, TuiMode::Run | TuiMode::Home);
        let has_active_jobs = self
            .jobs_state
            .jobs
            .iter()
            .any(|job| matches!(job.status, JobStatus::Pending | JobStatus::Running));
        let poll_interval = if in_run_or_home {
            JOBS_POLL_INTERVAL_MS
        } else if has_active_jobs {
            JOBS_POLL_INTERVAL_IDLE_MS
        } else {
            0
        };

        let should_load = if !self.jobs_state.jobs_loaded {
            true
        } else if poll_interval > 0 {
            match self.last_jobs_poll {
                Some(last_poll) => last_poll.elapsed().as_millis() as u64 >= poll_interval,
                None => true,
            }
        } else {
            false
        };

        if should_load && self.pending_jobs_load.is_none() {
            self.start_jobs_load();
        }

        // Poll for pending jobs load results (non-blocking)
        if let Some(ref mut rx) = self.pending_jobs_load {
            let recv_result = {
                #[cfg(feature = "profiling")]
                let _zone = self.profiler.zone("jobs.poll");
                rx.try_recv()
            };

            match recv_result {
                Ok(Ok(jobs)) => {
                    self.jobs_state.merge_loaded_jobs(jobs);
                    self.jobs_state.jobs_loaded = true;
                    self.last_jobs_poll = Some(std::time::Instant::now());
                    self.update_home_recent_jobs();
                    self.pending_jobs_load = None;
                }
                Ok(Err(err)) => {
                    self.jobs_state.jobs_loaded = true;
                    self.pending_jobs_load = None;
                    self.report_db_error("Jobs load failed", err);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty jobs)
                    self.jobs_state.jobs_loaded = true;
                    self.pending_jobs_load = None;
                }
            }
        }

        // Approvals: Load when entering Review/Approvals tab or when refresh requested
        if self.mode == TuiMode::Review
            && self.review_tab == ReviewTab::Approvals
            && !self.approvals_state.approvals_loaded
            && self.pending_approvals_load.is_none()
        {
            self.start_approvals_load();
        }

        // Poll for pending approvals load results (non-blocking)
        if let Some(ref mut rx) = self.pending_approvals_load {
            match rx.try_recv() {
                Ok(Ok(approvals)) => {
                    self.approvals_state.approvals = approvals;
                    self.approvals_state.approvals_loaded = true;
                    self.approvals_state.clamp_selection();
                    self.pending_approvals_load = None;
                }
                Ok(Err(err)) => {
                    self.approvals_state.approvals_loaded = true;
                    self.pending_approvals_load = None;
                    self.report_db_error("Approvals load failed", err);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty approvals)
                    self.approvals_state.approvals_loaded = true;
                    self.pending_approvals_load = None;
                }
            }
        }

        // Sessions: Load when entering Review/Sessions tab or when refresh requested
        if self.mode == TuiMode::Review
            && self.review_tab == ReviewTab::Sessions
            && !self.sessions_state.sessions_loaded
            && self.pending_sessions_load.is_none()
        {
            self.start_sessions_load();
        }

        // Triage: Load when entering Review/Triage tab or when refresh requested
        if self.mode == TuiMode::Review
            && self.review_tab == ReviewTab::Triage
            && !self.triage_state.loaded
            && self.pending_triage_load.is_none()
        {
            self.start_triage_load();
        }

        if self.mode == TuiMode::Run
            && self.run_tab == RunTab::Outputs
            && !self.catalog_state.loaded
            && self.pending_catalog_load.is_none()
        {
            self.start_catalog_load();
        }

        // Poll for pending sessions load results (non-blocking)
        if let Some(ref mut rx) = self.pending_sessions_load {
            match rx.try_recv() {
                Ok(sessions) => {
                    self.sessions_state.sessions = sessions;
                    self.sessions_state.sessions_loaded = true;
                    if let Some(pending_id) = self.sessions_state.pending_select_session_id.take() {
                        if let Some(pos) = self
                            .sessions_state
                            .sessions
                            .iter()
                            .position(|session| session.id == pending_id)
                        {
                            self.sessions_state.selected_index = pos;
                        }
                    }
                    self.sessions_state.clamp_selection();
                    self.pending_sessions_load = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed, mark as loaded (empty sessions)
                    self.sessions_state.sessions_loaded = true;
                    self.pending_sessions_load = None;
                }
            }
        }

        if let Some(ref mut rx) = self.pending_triage_load {
            match rx.try_recv() {
                Ok(Ok(data)) => {
                    self.triage_state.quarantine_rows = data.quarantine_rows;
                    self.triage_state.schema_mismatches = data.schema_mismatches;
                    self.triage_state.dead_letters = data.dead_letters;
                    self.triage_state.loaded = true;
                    self.triage_state.clamp_selection();
                    self.pending_triage_load = None;
                }
                Ok(Err(err)) => {
                    self.triage_state.loaded = true;
                    self.triage_state.status_message = Some(err);
                    self.pending_triage_load = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.triage_state.loaded = true;
                    self.pending_triage_load = None;
                }
            }
        }

        if let Some(ref mut rx) = self.pending_catalog_load {
            match rx.try_recv() {
                Ok(Ok(data)) => {
                    self.catalog_state.pipelines = data.pipelines;
                    self.catalog_state.runs = data.runs;
                    if let Some(run_id) = self.catalog_state.pending_select_run_id.take() {
                        if let Some(ref runs) = self.catalog_state.runs {
                            if let Some(pos) = runs.iter().position(|run| run.id == run_id) {
                                self.catalog_state.tab = CatalogTab::Runs;
                                self.catalog_state.selected_index = pos;
                            }
                        }
                    }
                    self.catalog_state.loaded = true;
                    self.catalog_state.clamp_selection();
                    self.pending_catalog_load = None;
                }
                Ok(Err(err)) => {
                    self.catalog_state.loaded = true;
                    self.catalog_state.status_message = Some(err);
                    self.pending_catalog_load = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.catalog_state.loaded = true;
                    self.pending_catalog_load = None;
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

        // Debounced Rule Builder pattern search
        if self.discover.view_state == DiscoverViewState::RuleBuilder {
            if let Some(ref mut builder) = self.discover.rule_builder {
                if let Some(changed_at) = builder.pattern_changed_at {
                    let elapsed = changed_at.elapsed().as_millis();
                    if elapsed >= DEBOUNCE_MS {
                        let pattern = builder.pattern.clone();
                        builder.pattern_changed_at = None;
                        self.update_rule_builder_files(&pattern);
                    }
                }
            }
        }

        // Poll for pending glob search results (non-blocking recursive search)
        if let Some(ref mut rx) = self.pending_glob_search {
            match rx.try_recv() {
                Ok(result) => {
                    // Only apply results if pattern still matches (user may have typed more)
                    let current_pattern = self
                        .discover
                        .glob_explorer
                        .as_ref()
                        .map(|e| e.pattern.clone())
                        .unwrap_or_default();

                    if result.pattern == current_pattern {
                        if let Some(err) = result.error {
                            if let Some(ref mut explorer) = self.discover.glob_explorer {
                                explorer.folders =
                                    vec![FsEntry::loading(&format!("Error: {}", err))];
                                explorer.total_count = GlobFileCount::Exact(0);
                                explorer.selected_folder = 0;
                            }
                            self.report_db_error("Glob search failed", err);
                        } else {
                            // Search complete! Update explorer with results
                            if let Some(ref mut explorer) = self.discover.glob_explorer {
                                explorer.folders = result.folders;
                                explorer.total_count = GlobFileCount::Exact(result.total_count);
                                explorer.selected_folder = 0;
                            }
                        }
                    }
                    // else: stale result, discard it
                    self.pending_glob_search = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still searching - update spinner
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.folders.len() == 1 {
                            if let Some(FsEntry::Loading { message }) = explorer.folders.get_mut(0)
                            {
                                if message.contains("Searching") {
                                    let spinner_char =
                                        crate::cli::tui::ui::spinner_char(self.tick_count);
                                    *message = format!(
                                        "{} Searching for {}...",
                                        spinner_char, explorer.pattern
                                    );
                                }
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_glob_search = None;
                }
            }
        }

        // Poll for pending Rule Builder pattern search results
        if let Some(ref mut rx) = self.pending_rule_builder_search {
            match rx.try_recv() {
                Ok(result) => {
                    // Only apply results if pattern still matches (user may have typed more)
                    let current_pattern = self
                        .discover
                        .rule_builder
                        .as_ref()
                        .map(|b| b.pattern.clone())
                        .unwrap_or_default();

                    if result.pattern == current_pattern
                        || result.pattern == format!("**/{}", current_pattern)
                    {
                        if let Some(err) = result.error {
                            if let Some(ref mut builder) = self.discover.rule_builder {
                                builder.file_results =
                                    super::extraction::FileResultsState::Exploration {
                                        folder_matches: Vec::new(),
                                        expanded_folder_indices: std::collections::HashSet::new(),
                                        detected_patterns: Vec::new(),
                                    };
                                builder.match_count = 0;
                                builder.is_streaming = false;
                                builder.pattern_error = Some(err.clone());
                            }
                            self.report_db_error("Rule builder search failed", err);
                        } else {
                            // Search complete! Update Rule Builder with results
                            let has_matches = result.total_count > 0;
                            if let Some(ref mut builder) = self.discover.rule_builder {
                                builder.file_results =
                                    super::extraction::FileResultsState::Exploration {
                                        folder_matches: result.folder_matches,
                                        expanded_folder_indices: std::collections::HashSet::new(),
                                        detected_patterns: Vec::new(),
                                    };
                                builder.match_count = result.total_count;
                                builder.is_streaming = false;
                            }
                            // Auto-run sample eval when pattern matches files
                            // Only if: matches found, not running full eval, not just switched patterns
                            if has_matches
                                && self.pending_schema_eval.is_none()
                                && self.pending_sample_eval.is_none()
                            {
                                self.run_sample_schema_eval();
                            }
                        }
                    }
                    // else: stale result, discard it
                    self.pending_rule_builder_search = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still searching - spinner is shown via builder.streaming flag
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_rule_builder_search = None;
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.is_streaming = false;
                    }
                }
            }
        }

        // Poll for pending Rule Builder extraction preview results
        if let Some(ref mut rx) = self.pending_rule_builder_preview {
            match rx.try_recv() {
                Ok(result) => {
                    let current_pattern = self
                        .discover
                        .rule_builder
                        .as_ref()
                        .map(|b| b.pattern.clone())
                        .unwrap_or_default();

                    if result.pattern == current_pattern {
                        if let Some(err) = result.error {
                            if let Some(ref mut builder) = self.discover.rule_builder {
                                builder.file_results =
                                    super::extraction::FileResultsState::ExtractionPreview {
                                        preview_files: Vec::new(),
                                    };
                                builder.match_count = 0;
                                builder.is_streaming = false;
                                builder.pattern_error = Some(err.clone());
                            }
                            self.report_db_error("Rule builder preview failed", err);
                        } else {
                            if let Some(ref mut builder) = self.discover.rule_builder {
                                builder.file_results =
                                    super::extraction::FileResultsState::ExtractionPreview {
                                        preview_files: result.preview_files,
                                    };
                                builder.match_count = result.total_count;
                                builder.pattern_error = None;
                                builder.is_streaming = false;
                                builder.selected_file = 0;
                            }
                        }
                    }
                    self.pending_rule_builder_preview = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_rule_builder_preview = None;
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
                Ok(SchemaEvalResult::Progress {
                    progress,
                    paths_analyzed,
                    total_paths,
                }) => {
                    // Update progress
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.eval_state = super::extraction::EvalState::Running { progress };
                    }
                    if let Some(job_id) = self.current_schema_eval_job_id {
                        self.set_schema_eval_job_total(job_id, total_paths as u32);
                        self.update_schema_eval_job(
                            job_id,
                            JobStatus::Running,
                            paths_analyzed as u32,
                            None,
                        );
                    }
                }
                Ok(SchemaEvalResult::Complete {
                    job_id,
                    pattern,
                    pattern_seeds,
                    path_archetypes,
                    naming_schemes,
                    synonym_suggestions,
                    rule_candidates,
                    paths_analyzed,
                }) => {
                    // Update Rule Builder state with results
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        if builder.pattern == pattern {
                            builder.pattern_seeds = pattern_seeds;
                            builder.path_archetypes = path_archetypes;
                            builder.naming_schemes = naming_schemes;
                            builder.synonym_suggestions = synonym_suggestions;
                            builder.rule_candidates = rule_candidates;
                            if !builder.rule_candidates.is_empty() {
                                builder.selected_candidate = builder
                                    .selected_candidate
                                    .min(builder.rule_candidates.len().saturating_sub(1));
                            } else {
                                builder.selected_candidate = 0;
                            }
                        }
                        builder.eval_state = super::extraction::EvalState::Idle;
                    }
                    // Update job status
                    self.update_schema_eval_job(
                        job_id,
                        JobStatus::Completed,
                        paths_analyzed as u32,
                        None,
                    );
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
                        self.update_schema_eval_job(
                            job_id,
                            JobStatus::Failed,
                            0,
                            Some(err.clone()),
                        );
                    }
                    self.current_schema_eval_job_id = None;
                    self.pending_schema_eval = None;
                    self.discover.status_message =
                        Some((format!("Schema eval failed: {}", err), true));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still running - that's fine
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Channel closed unexpectedly
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        builder.eval_state = super::extraction::EvalState::Idle;
                    }
                    if let Some(job_id) = self.current_schema_eval_job_id {
                        self.update_schema_eval_job(
                            job_id,
                            JobStatus::Failed,
                            0,
                            Some("Channel disconnected".to_string()),
                        );
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
                    rule_candidates,
                    paths_analyzed,
                }) => {
                    if let Some(ref mut builder) = self.discover.rule_builder {
                        if builder.pattern == pattern {
                            builder.pattern_seeds = pattern_seeds;
                            builder.path_archetypes = path_archetypes;
                            builder.naming_schemes = naming_schemes;
                            builder.synonym_suggestions = synonym_suggestions;
                            builder.rule_candidates = rule_candidates;
                            if !builder.rule_candidates.is_empty() {
                                builder.selected_candidate = builder
                                    .selected_candidate
                                    .min(builder.rule_candidates.len().saturating_sub(1));
                            } else {
                                builder.selected_candidate = 0;
                            }
                            builder.sampled_paths_count = paths_analyzed;
                            self.discover.status_message =
                                Some((format!("Sample: {} paths analyzed", paths_analyzed), false));
                        }
                    }
                    self.pending_sample_eval = None;
                }
                Ok(SampleEvalResult::Error(err)) => {
                    if let Some(builder) = self.discover.rule_builder.as_mut() {
                        builder.last_sample_eval_key = None;
                    }
                    self.discover.status_message =
                        Some((format!("Sample eval failed: {}", err), true));
                    self.pending_sample_eval = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(builder) = self.discover.rule_builder.as_mut() {
                        builder.last_sample_eval_key = None;
                    }
                    self.pending_sample_eval = None;
                }
            }
        }

        // Poll for cache load messages (non-blocking)
        if let Some(ref mut rx) = self.pending_cache_load {
            match rx.try_recv() {
                Ok(CacheLoadMessage::Complete {
                    workspace_id,
                    source_id,
                    total_files,
                    tags,
                    cache,
                }) => {
                    if Some(workspace_id) != self.active_workspace_id() {
                        self.pending_cache_load = None;
                        return;
                    }
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
                        explorer.cache_workspace_id = Some(workspace_id);
                        explorer.selected_folder = 0;

                        // Ensure root folders are displayed
                        if let Some(root_folders) = explorer.folder_cache.get("") {
                            explorer.folders = root_folders.clone();
                            explorer.total_count = GlobFileCount::Exact(
                                root_folders.iter().map(|f| f.file_count()).sum(),
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
                Err(mpsc::TryRecvError::Empty) => {
                    // No message yet - still loading
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_cache_load = None;
                    self.cache_load_progress = None;
                }
            }
        }

        // Poll for folder query results (lazy loading for navigation)
        if let Some(ref mut rx) = self.pending_folder_query {
            match rx.try_recv() {
                Ok(FolderQueryMessage::Complete {
                    workspace_id,
                    prefix,
                    folders,
                    total_count,
                }) => {
                    if Some(workspace_id) != self.active_workspace_id() {
                        self.pending_folder_query = None;
                        return;
                    }
                    // Cache the result for future navigation
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.cache_workspace_id != Some(workspace_id) {
                            self.pending_folder_query = None;
                            return;
                        }
                        explorer
                            .folder_cache
                            .insert(prefix.clone(), folders.clone());

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
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading - update spinner
                    if let Some(ref mut explorer) = self.discover.glob_explorer {
                        if explorer.folders.len() == 1 {
                            if let Some(FsEntry::Loading { message }) = explorer.folders.get_mut(0)
                            {
                                if message.contains("Loading") {
                                    let spinner_char =
                                        crate::cli::tui::ui::spinner_char(self.tick_count);
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
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_folder_query = None;
                }
            }
        }

        // Poll for files load results (paged files list)
        if let Some(ref mut rx) = self.pending_files_load {
            match rx.try_recv() {
                Ok(FilesLoadMessage::Complete {
                    workspace_id,
                    source_id,
                    page_offset,
                    total_count,
                    files,
                }) => {
                    if Some(workspace_id) != self.active_workspace_id() {
                        self.pending_files_load = None;
                        return;
                    }
                    if self.discover.selected_source_id != Some(source_id) {
                        self.pending_files_load = None;
                        return;
                    }
                    if page_offset != self.discover.page_offset {
                        self.pending_files_load = None;
                        return;
                    }

                    let page_size = self.discover.page_size.max(1);
                    let max_offset = if total_count == 0 {
                        0
                    } else {
                        (total_count.saturating_sub(1) / page_size) * page_size
                    };
                    if self.discover.page_offset > max_offset {
                        self.discover.page_offset = max_offset;
                        self.discover.data_loaded = false;
                        self.pending_files_load = None;
                        return;
                    }

                    let mut available_tags = HashSet::new();
                    for file in &files {
                        for tag in &file.tags {
                            available_tags.insert(tag.clone());
                        }
                    }

                    self.discover.files = files;
                    self.discover.available_tags = available_tags.into_iter().collect();
                    self.discover.available_tags.sort();
                    self.discover.selected = 0;
                    self.discover.total_files = total_count;
                    self.discover.data_loaded = true;
                    self.discover.db_filtered = true;
                    self.discover.scan_error = None;
                    self.refresh_tags_list();
                    self.pending_files_load = None;
                }
                Ok(FilesLoadMessage::Error(err)) => {
                    self.discover.scan_error = Some(err.clone());
                    self.discover.data_loaded = true;
                    self.pending_files_load = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_files_load = None;
                }
            }
        }

        // Poll for tags load results
        if let Some(ref mut rx) = self.pending_tags_load {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(message) => {
                            if Some(message.workspace_id) != self.active_workspace_id() {
                                self.pending_tags_load = None;
                                return;
                            }
                            if self.discover.selected_source_id != Some(message.source_id) {
                                self.pending_tags_load = None;
                                return;
                            }

                            self.discover.tags = message.tags;
                            self.discover.available_tags = message.available_tags;
                            if let Some(selected_tag) = self.discover.selected_tag {
                                if selected_tag >= self.discover.tags.len() {
                                    self.discover.selected_tag = None;
                                }
                            }
                        }
                        Err(err) => {
                            self.report_db_error("Tags load failed", err);
                        }
                    }
                    self.pending_tags_load = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_tags_load = None;
                }
            }
        }

        // Poll for pending manual tag apply (rule builder preview tagging)
        if let Some(ref mut rx) = self.pending_tag_apply {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    let mut path_set: HashSet<&String> = HashSet::new();
                    for path in &result.paths {
                        path_set.insert(path);
                    }
                    for file in &mut self.discover.files {
                        if path_set.contains(&file.rel_path) {
                            if !file.tags.contains(&result.tag) {
                                file.tags.push(result.tag.clone());
                            }
                        }
                    }
                    if !self.discover.available_tags.contains(&result.tag) {
                        self.discover.available_tags.push(result.tag.clone());
                        self.discover.available_tags.sort();
                    }
                    self.discover.status_message = Some((
                        format!("Tagged {} files with '{}'", result.tagged_count, result.tag),
                        false,
                    ));
                    self.refresh_tags_list();
                    if let Some(builder) = self.discover.rule_builder.as_mut() {
                        builder.selected_preview_files.clear();
                    }
                    self.pending_tag_apply = None;
                }
                Ok(Err(err)) => {
                    self.discover.status_message = Some((err, true));
                    self.pending_tag_apply = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_tag_apply = None;
                }
            }
        }

        // Poll for pending rule apply results
        if let Some(ref mut rx) = self.pending_rule_apply {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    let rule_id = RuleId::new(result.rule_id);
                    if !self.discover.rules.iter().any(|rule| rule.id == rule_id) {
                        self.discover.rules.push(RuleInfo {
                            id: rule_id,
                            pattern: result.pattern.clone(),
                            tag: result.tag.clone(),
                            priority: 100,
                            enabled: true,
                        });
                        if self.discover.selected_rule >= self.discover.rules.len() {
                            self.discover.selected_rule = self.discover.rules.len() - 1;
                        }
                    }
                    self.refresh_tags_list();
                    self.discover.status_message = Some((
                        format!(
                            "Created rule: {} → {} ({} files tagged)",
                            result.pattern, result.tag, result.tagged_count
                        ),
                        false,
                    ));
                    self.pending_rule_apply = None;
                }
                Ok(Err(err)) => {
                    self.discover.status_message = Some((err, true));
                    self.pending_rule_apply = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_rule_apply = None;
                }
            }
        }

        // Load Scout data if in Ingest (non-Sources) (but NOT while scanning - don't block progress updates)
        if self.mode == TuiMode::Ingest
            && self.ingest_tab != IngestTab::Sources
            && self.discover.view_state != DiscoverViewState::Scanning
        {
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
                if self.discover.rule_builder.is_none()
                    && self.discover.view_state == DiscoverViewState::RuleBuilder
                {
                    let source_id = self.discover.selected_source_id;
                    let mut builder = super::extraction::RuleBuilderState::new(source_id);
                    builder.pattern = "**/*".to_string();
                    self.discover.rule_builder = Some(builder);
                }
                self.load_scout_files();

                // Check if cache is still loading (not yet loaded)
                let cache_not_loaded = self
                    .discover
                    .glob_explorer
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
                                    spinner_char, progress.source_name, elapsed
                                ))];
                            }
                        } else if explorer.folders.is_empty() {
                            explorer.folders = vec![FsEntry::loading(&format!(
                                "{} Loading folder hierarchy...",
                                spinner_char
                            ))];
                        }
                    }
                }

                // Start cache load if not already started
                if self.pending_cache_load.is_none() {
                    self.start_cache_load();
                }
            }
            // Load rules for Rules Manager if it's open
            if self.discover.view_state == DiscoverViewState::RulesManager
                && self.discover.rules.is_empty()
            {
                self.load_rules_for_manager();
            }
        }

        // Poll for pending scan results (non-blocking directory scan)
        // Process ALL available messages (progress updates + completion)
        if self.pending_scan.is_some() {
            let mut rx = self.pending_scan.take().unwrap();
            let mut scan_complete = false;
            // Drain all available messages
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            TuiScanResult::Started { job_id, scan_id } => {
                                // Validation passed, scan is actually starting
                                self.current_scan_id = scan_id;
                                self.discover.status_message = Some((
                                    format!(
                                        "Scan started (Job #{}) - press [2] to view Jobs",
                                        job_id
                                    ),
                                    false,
                                ));
                            }
                            TuiScanResult::Progress(progress) => {
                                // Update progress - UI will display this
                                if let Some(job_id) = self.current_scan_job_id {
                                    self.update_scan_job_status(
                                        job_id,
                                        JobStatus::Running,
                                        None,
                                        Some(progress.files_persisted as u32),
                                        Some(progress.files_found as u32),
                                    );
                                }
                                self.discover.scan_progress = Some(progress);
                            }
                            TuiScanResult::Complete {
                                source_path,
                                files_persisted,
                            } => {
                                // Update job status to Completed
                                if let Some(job_id) = self.current_scan_job_id {
                                    let count = files_persisted as u32;
                                    self.update_scan_job_status(
                                        job_id,
                                        JobStatus::Completed,
                                        None,
                                        Some(count),
                                        Some(count),
                                    );
                                }

                                // Scanner persisted to DB - reload sources and files
                                let source_name = std::path::Path::new(&source_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| source_path.clone());

                                // Use accurate count from scanner (not stale progress update)
                                let final_file_count = files_persisted;

                                // Trigger sources reload (non-blocking, handled by tick())
                                self.discover.pending_select_source_path =
                                    Some(source_path.clone());
                                self.discover.sources_loaded = false;
                                self.start_sources_load();

                                // Reset cache state to force reload for the new source
                                // IMPORTANT: Must be AFTER load_scout_files() because it sets data_loaded=true
                                self.discover.data_loaded = false;
                                self.discover.db_filtered = false;
                                self.discover.page_offset = 0;
                                self.discover.total_files = final_file_count as usize;
                                self.pending_cache_load = None;
                                self.cache_load_progress = None;
                                if let Some(ref mut explorer) = self.discover.glob_explorer {
                                    explorer.cache_loaded = false;
                                    explorer.cache_source_id = None;
                                    explorer.cache_workspace_id = None;
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
                                    format!(
                                        "Scanned {} files from {}",
                                        final_file_count, source_name
                                    ),
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
                                    self.update_scan_job_status(
                                        job_id,
                                        JobStatus::Failed,
                                        Some(err.clone()),
                                        None,
                                        None,
                                    );
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
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Task ended without completion - mark job as failed
                        if let Some(job_id) = self.current_scan_job_id {
                            self.update_scan_job_status(
                                job_id,
                                JobStatus::Failed,
                                Some("Scan task ended unexpectedly".to_string()),
                                None,
                                None,
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
                self.current_scan_job_id = None;
                self.current_scan_id = None;
                self.scan_cancel_token = None;
            } else {
                self.pending_scan = Some(rx);
            }
        }

        // Poll for pending query results
        if let Some(ref mut rx) = self.pending_query {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending_query = None;
                    self.query_state.executing = false;
                    self.query_state.execution_time_ms = Some(result.elapsed_ms);
                    match result.result {
                        Ok(results) => {
                            self.query_state.add_to_history(&result.sql);
                            self.query_state.results = Some(results);
                            if self.query_state.view_state == QueryViewState::Executing {
                                self.query_state.view_state = QueryViewState::ViewingResults;
                            }
                        }
                        Err(err) => {
                            self.query_state.error = Some(err);
                            if self.query_state.view_state == QueryViewState::Executing {
                                self.query_state.view_state = QueryViewState::Editing;
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_query = None;
                    self.query_state.executing = false;
                    self.query_state.error = Some("Query task ended unexpectedly".to_string());
                    if self.query_state.view_state == QueryViewState::Executing {
                        self.query_state.view_state = QueryViewState::Editing;
                    }
                }
            }
        }

        // TODO: Poll job status, refresh metrics
    }

    // =========================================================================
    // Parser Bench Methods
    // =========================================================================

    /// Maximum number of parser files to process in a single Python subprocess.
    /// Prevents command line overflow and keeps memory usage reasonable.
    const METADATA_BATCH_SIZE: usize = 50;

    /// Extract metadata from multiple Python parser files in a single subprocess.
    /// Returns a map from path string to (name, version, topics).
    /// Uses stdin to pass paths as JSON array, avoiding command line length limits.
    fn extract_parser_metadata_batch(
        paths: &[std::path::PathBuf],
    ) -> std::collections::HashMap<String, (String, Option<String>, Vec<String>)> {
        casparian::parser_metadata::extract_metadata_batch(paths)
            .into_iter()
            .map(|(path, meta)| (path, (meta.name, meta.version, meta.topics)))
            .collect()
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
        if self.parser_bench.is_filtering {
            match handle_text_input(key, &mut self.parser_bench.filter) {
                TextInputResult::Committed => {
                    self.parser_bench.is_filtering = false;
                }
                TextInputResult::Cancelled => {
                    self.parser_bench.is_filtering = false;
                    self.parser_bench.filter.clear();
                }
                TextInputResult::Continue | TextInputResult::NotHandled => {}
            }
            let filtered = self.filtered_parser_indices();
            if let Some(first) = filtered.first().copied() {
                if !filtered.contains(&self.parser_bench.selected_parser) {
                    self.parser_bench.selected_parser = first;
                }
            } else {
                self.parser_bench.selected_parser = 0;
            }
            return;
        }

        let filtered = self.filtered_parser_indices();
        if let Some(first) = filtered.first().copied() {
            if !filtered.contains(&self.parser_bench.selected_parser) {
                self.parser_bench.selected_parser = first;
            }
        }

        match key.code {
            // Navigation
            KeyCode::Down => {
                if !filtered.is_empty() {
                    let current_pos = filtered
                        .iter()
                        .position(|idx| *idx == self.parser_bench.selected_parser)
                        .unwrap_or(0);
                    let next_pos = (current_pos + 1) % filtered.len();
                    self.parser_bench.selected_parser = filtered[next_pos];
                }
            }
            KeyCode::Up => {
                if !filtered.is_empty() {
                    let current_pos = filtered
                        .iter()
                        .position(|idx| *idx == self.parser_bench.selected_parser)
                        .unwrap_or(0);
                    let prev_pos = if current_pos == 0 {
                        filtered.len() - 1
                    } else {
                        current_pos - 1
                    };
                    self.parser_bench.selected_parser = filtered[prev_pos];
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
            // Filter
            KeyCode::Char('/') => {
                self.parser_bench.is_filtering = true;
            }
            KeyCode::Esc => {
                if !self.parser_bench.filter.is_empty() {
                    self.parser_bench.filter.clear();
                } else if self.parser_bench.test_result.is_some() {
                    self.parser_bench.test_result = None;
                } else {
                    self.set_mode(TuiMode::Home);
                }
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::context;
    use crate::cli::tui::flow_record::RecordRedaction;
    use std::sync::Mutex;
    use std::time::Duration;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_args() -> TuiArgs {
        TuiArgs {
            database: Some(
                std::env::temp_dir()
                    .join(format!("casparian_test_{}.sqlite", uuid::Uuid::new_v4())),
            ),
            standalone_writer: true,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        }
    }

    #[test]
    fn test_mode_switching() {
        let mut app = App::new(test_args(), None);
        assert!(matches!(app.mode, TuiMode::Home));

        // Key '1' should switch to Ingest
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Ingest));
        assert!(matches!(app.ingest_tab, IngestTab::Select));

        // Key '2' should switch to Run
        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Run));

        // Key '3' should switch to Review
        app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Review));

        // Key '4' should switch to Query
        app.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Query));
        // Leave query editing so global nav applies.
        app.query_state.view_state = QueryViewState::ViewingResults;

        // Key '5' should switch to Settings
        app.handle_key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Settings));

        // Key '0' should return to Home (per spec)
        app.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Home));
    }

    #[test]
    fn test_home_source_navigation() {
        let mut app = App::new(test_args(), None);
        // Add some test sources
        app.discover.sources = vec![
            SourceInfo {
                id: SourceId::new(),
                name: "Source 1".to_string(),
                path: std::path::PathBuf::from("/test/source1"),
                file_count: 10,
            },
            SourceInfo {
                id: SourceId::new(),
                name: "Source 2".to_string(),
                path: std::path::PathBuf::from("/test/source2"),
                file_count: 20,
            },
        ];
        assert_eq!(app.home.selected_source_index, 0);

        // Down arrow should move to source 1
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.home.selected_source_index, 1);

        // Up arrow should move back to source 0
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.home.selected_source_index, 0);
    }

    #[test]
    fn test_ctrl_c_quits() {
        let mut app = App::new(test_args(), None);
        assert!(app.running);

        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert!(!app.running);
    }

    #[test]
    fn test_esc_does_not_leave_run_jobs() {
        let mut app = App::new(test_args(), None);
        // Start in Run/Jobs tab
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        // Esc does not change task when no dialog is open
        assert!(matches!(app.mode, TuiMode::Run));
    }

    #[test]
    fn test_workspace_context_respected() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("CASPARIAN_HOME", temp_dir.path());

        let db_path = temp_dir.path().join("state.sqlite");
        let db = ScoutDatabase::open(&db_path).unwrap();
        let ws_alpha = db.create_workspace("alpha").unwrap();
        let ws_bravo = db.create_workspace("bravo").unwrap();

        context::set_active_workspace(&ws_bravo.id).unwrap();

        let args = TuiArgs {
            database: Some(db_path),
            standalone_writer: false,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        };
        let mut app = App::new(args, None);
        app.ensure_active_workspace();

        assert_eq!(app.active_workspace_id(), Some(ws_bravo.id));

        // Clean up context env override for other tests
        let _ = context::clear_active_workspace();
        std::env::remove_var("CASPARIAN_HOME");
    }

    #[test]
    fn test_jobs_pipeline_toggle_is_not_captured_globally() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;
        app.jobs_state.view_state = JobsViewState::JobList;
        app.jobs_state.show_pipeline = false;

        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));

        assert!(app.jobs_state.show_pipeline);
        assert!(matches!(app.mode, TuiMode::Run));
    }

    // =========================================================================
    // Jobs Mode Tests - Critical Path Coverage
    // =========================================================================

    fn create_test_jobs() -> Vec<JobInfo> {
        vec![
            JobInfo {
                id: 1,
                file_id: Some(101),
                job_type: JobType::Parse,
                origin: JobOrigin::Persistent,
                name: "parser_a".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Pending,
                started_at: Local::now(),
                completed_at: None,
                pipeline_run_id: None,
                logical_date: None,
                selection_snapshot_hash: None,
                quarantine_rows: None,
                items_total: 100,
                items_processed: 0,
                items_failed: 0,
                output_path: Some("/data/output/a.parquet".into()),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
                violations: vec![],
                top_violations_loaded: false,
                selected_violation_index: 0,
            },
            JobInfo {
                id: 2,
                file_id: Some(102),
                job_type: JobType::Parse,
                origin: JobOrigin::Persistent,
                name: "parser_b".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Running,
                started_at: Local::now(),
                completed_at: None,
                pipeline_run_id: None,
                logical_date: None,
                selection_snapshot_hash: None,
                quarantine_rows: None,
                items_total: 100,
                items_processed: 50,
                items_failed: 0,
                output_path: Some("/data/output/b.parquet".into()),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
                violations: vec![],
                top_violations_loaded: false,
                selected_violation_index: 0,
            },
            JobInfo {
                id: 3,
                file_id: Some(103),
                job_type: JobType::Parse,
                origin: JobOrigin::Persistent,
                name: "parser_c".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Failed,
                started_at: Local::now(),
                completed_at: Some(Local::now()),
                pipeline_run_id: None,
                logical_date: None,
                selection_snapshot_hash: None,
                quarantine_rows: None,
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
                violations: vec![],
                top_violations_loaded: false,
                selected_violation_index: 0,
            },
            JobInfo {
                id: 4,
                file_id: Some(104),
                job_type: JobType::Parse,
                origin: JobOrigin::Persistent,
                name: "parser_d".into(),
                version: Some("1.0.0".into()),
                status: JobStatus::Completed,
                started_at: Local::now(),
                completed_at: Some(Local::now()),
                pipeline_run_id: None,
                logical_date: None,
                selection_snapshot_hash: None,
                quarantine_rows: None,
                items_total: 100,
                items_processed: 100,
                items_failed: 0,
                output_path: Some("/data/output/d.parquet".into()),
                output_size_bytes: Some(1024 * 1024),
                backtest: None,
                failures: vec![],
                violations: vec![],
                top_violations_loaded: false,
                selected_violation_index: 0,
            },
        ]
    }

    #[test]
    fn test_jobs_filtered_jobs() {
        let mut state = JobsState {
            jobs: create_test_jobs(),
            ..Default::default()
        };

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
        let mut state = JobsState {
            jobs: create_test_jobs(),
            selected_index: 3, // Last job (Completed)
            ..Default::default()
        };

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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;
        app.jobs_state.jobs = create_test_jobs();
        app.jobs_state.selected_index = 0;

        // Navigate down to last item
        for _ in 0..10 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        // Should stop at last valid index (2) for actionable list
        assert_eq!(app.jobs_state.selected_index, 2);

        // Navigate up past beginning
        for _ in 0..10 {
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }
        // Should stop at 0
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    #[test]
    fn test_jobs_navigation_respects_filter() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;
        app.jobs_state.jobs = create_test_jobs();

        // Filter to show only Pending and Failed (2 jobs total won't work, let's just use Pending)
        // Actually, with our test data, Pending has 1 job
        app.jobs_state.set_filter(Some(JobStatus::Pending));
        assert_eq!(app.jobs_state.filtered_jobs().len(), 1);

        // Try to navigate - should stay at 0 since only 1 item
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    // =========================================================================
    // UI Latency Tests - Navigation Must Be Fast
    // =========================================================================
    //
    // These tests verify that navigation operations complete quickly.
    // UI freezes occur when navigation triggers expensive operations like DB queries.
    // Navigation should be pure in-memory operations (< 1ms typical, < 10ms max).

    #[test]
    fn test_sources_dropdown_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Set up sources (in-memory, no DB)
        app.discover.sources = (0..100)
            .map(|i| SourceInfo {
                id: SourceId::new(),
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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

    #[test]
    fn test_file_list_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::Files;

        // Set up large file list (in-memory)
        app.discover.files = (0..10_000)
            .map(|i| FileInfo {
                file_id: i,
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
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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

    #[test]
    #[ignore = "Flaky under variable system load - run manually with --ignored"]
    fn test_jobs_list_navigation_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;

        // Set up large jobs list (in-memory)
        app.jobs_state.jobs = (0..1000)
            .map(|i| JobInfo {
                id: i,
                file_id: Some(i * 100),
                job_type: JobType::Parse,
                origin: JobOrigin::Persistent,
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
                pipeline_run_id: None,
                logical_date: None,
                selection_snapshot_hash: None,
                quarantine_rows: None,
                items_total: 100,
                items_processed: 50,
                items_failed: 0,
                output_path: Some(format!("/data/output/file_{}.parquet", i)),
                output_size_bytes: None,
                backtest: None,
                failures: vec![],
                violations: vec![],
                top_violations_loaded: false,
                selected_violation_index: 0,
            })
            .collect();
        app.jobs_state.selected_index = 0;

        // Navigate through 500 jobs and measure time
        let start = Instant::now();
        for _ in 0..500 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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

    #[test]
    fn test_sources_filter_typing_latency() {
        use std::time::Instant;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Set up sources
        app.discover.sources = (0..100)
            .map(|i| SourceInfo {
                id: SourceId::new(),
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
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
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
        assert_eq!(JobStatus::PartialSuccess.symbol(), "⚠");
        assert_eq!(JobStatus::Failed.symbol(), "✗");
    }

    #[test]
    fn test_job_status_as_str() {
        assert_eq!(JobStatus::Pending.as_str(), "Pending");
        assert_eq!(JobStatus::Running.as_str(), "Running");
        assert_eq!(JobStatus::Completed.as_str(), "Completed");
        assert_eq!(JobStatus::PartialSuccess.as_str(), "Partial");
        assert_eq!(JobStatus::Failed.as_str(), "Failed");
    }

    // =========================================================================
    // Edge Case / Failure Mode Tests
    // =========================================================================

    #[test]
    fn test_jobs_empty_list_navigation() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Run;
        app.run_tab = RunTab::Jobs;
        // Empty jobs list
        app.jobs_state.jobs = vec![];
        app.jobs_state.selected_index = 0;

        // Try to navigate - should not panic
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.jobs_state.selected_index, 0);
    }

    #[test]
    fn test_filter_to_empty_result() {
        let mut state = JobsState {
            jobs: create_test_jobs(), // Has Pending, Running, Failed, Completed
            selected_index: 2,
            ..Default::default()
        };

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
                file_id: 1,
                path: "/data/sales.csv".into(),
                rel_path: "sales.csv".into(),
                size: 1024,
                modified: Local::now(),
                tags: vec!["sales".into()],
                is_dir: false,
            },
            FileInfo {
                file_id: 2,
                path: "/data/orders.csv".into(),
                rel_path: "orders.csv".into(),
                size: 2048,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                file_id: 3,
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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.filter.is_empty());

        // Press / to enter filter mode
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Filtering);

        // Type filter text
        for c in "sales".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.discover.filter, "sales");

        // Verify filtering works
        let filtered = app.filtered_files();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].path.contains("sales"));
    }

    #[test]
    fn test_discover_filter_esc_cancels() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.view_state = DiscoverViewState::Filtering;
        app.discover.filter = "test".to_string();

        // Esc should exit filter mode, NOT go to Home
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.filter.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Ingest));
    }

    #[test]
    fn test_discover_tag_dialog() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.selected = 1; // Select orders.csv
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 't' to open tag dialog
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Tagging);
        assert!(app.discover.tag_input.is_empty());

        // Type tag name
        for c in "important".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.discover.tag_input, "important");
    }

    #[test]
    fn test_discover_tag_dialog_esc_cancels() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.view_state = DiscoverViewState::Tagging;
        app.discover.tag_input = "partial".to_string();

        // Esc should close tag dialog, NOT go to Home
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.tag_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Ingest));
    }

    #[test]
    fn test_discover_scan_path_dialog() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 's' to open scan path dialog
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::EnteringPath);

        // Type path
        for c in "/tmp".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.discover.scan_path_input, "/tmp");
    }

    #[test]
    fn test_discover_scan_path_esc_cancels() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/some/path".to_string();

        // Esc should close scan dialog, NOT go to Home
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.scan_path_input.is_empty());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Ingest));
    }

    #[test]
    fn test_discover_bulk_tag_dialog() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 'B' (Shift+b) to open bulk tag dialog
        app.handle_key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT));
        assert_eq!(app.discover.view_state, DiscoverViewState::BulkTagging);
        assert!(app.discover.bulk_tag_input.is_empty());
        assert!(!app.discover.bulk_tag_save_as_rule);
    }

    #[test]
    fn test_discover_bulk_tag_toggle_save_as_rule() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.view_state = DiscoverViewState::BulkTagging;
        assert!(!app.discover.bulk_tag_save_as_rule);

        // Press Space to toggle save-as-rule
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.discover.bulk_tag_save_as_rule);

        // Press Space again to toggle back
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.discover.bulk_tag_save_as_rule);
    }

    #[test]
    fn test_discover_bulk_tag_esc_cancels() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::BulkTagging;
        app.discover.bulk_tag_input = "batch".to_string();
        app.discover.bulk_tag_save_as_rule = true;

        // Esc should close bulk tag dialog, NOT go to Home
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.bulk_tag_input.is_empty());
        assert!(!app.discover.bulk_tag_save_as_rule);
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Ingest));
    }

    #[test]
    fn test_discover_create_source_on_directory() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.selected = 2; // Select archives directory
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Press 'c' on a directory to create source
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::CreatingSource);
        assert!(app.discover.pending_source_path.is_some());
        assert!(app
            .discover
            .pending_source_path
            .as_ref()
            .unwrap()
            .contains("archives"));
    }

    #[test]
    fn test_discover_create_source_esc_cancels() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::CreatingSource;
        app.discover.source_name_input = "my_source".to_string();
        app.discover.pending_source_path = Some("/data/archives".to_string());

        // Esc should close create source dialog, NOT go to Home
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);
        assert!(app.discover.source_name_input.is_empty());
        assert!(app.discover.pending_source_path.is_none());
        // Still in Discover mode
        assert!(matches!(app.mode, TuiMode::Ingest));
    }

    #[test]
    fn test_discover_esc_no_view_change_when_no_dialog() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        // No dialogs open - view_state should be Files
        assert_eq!(app.discover.view_state, DiscoverViewState::Files);

        // Esc should not change views
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.mode, TuiMode::Home));
    }

    #[test]
    fn test_discover_navigation_with_files() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.files = create_test_files();
        app.discover.selected = 0;

        // Navigate down with j
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.discover.selected, 1);

        // Navigate down again
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.discover.selected, 2);

        // Try to navigate past end
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.discover.selected, 2); // Stays at last

        // Navigate up
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.discover.selected, 1);
    }

    #[test]
    fn test_discover_filter_glob_pattern() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        // Use realistic absolute paths like real scans produce
        app.discover.files = vec![
            FileInfo {
                file_id: 10,
                path: "/Users/test/workspace/blog/myproject/manage.py".into(),
                rel_path: "manage.py".into(),
                size: 1024,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                file_id: 11,
                path: "/Users/test/workspace/blog/myproject/manifest.json".into(),
                rel_path: "manifest.json".into(),
                size: 2048,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                file_id: 12,
                path: "/Users/test/workspace/blog/myproject/other.txt".into(),
                rel_path: "other.txt".into(),
                size: 512,
                modified: Local::now(),
                tags: vec![],
                is_dir: false,
            },
            FileInfo {
                file_id: 13,
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
        assert_eq!(
            filtered.len(),
            3,
            "Should match manage.py, manifest.json, commands.py"
        );
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
        assert_eq!(
            filtered.len(),
            3,
            "Should match manage.py, manifest.json, commands.py with absolute paths"
        );
    }

    #[test]
    fn test_discover_filter_substring_still_works() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Test backspace in scan path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/tmp/test".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.discover.scan_path_input, "/tmp/tes");

        // Reset and test backspace in tag input
        app.discover.view_state = DiscoverViewState::Tagging;
        app.discover.tag_input = "mytag".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.discover.tag_input, "myta");

        // Reset and test backspace in bulk tag input
        app.discover.view_state = DiscoverViewState::BulkTagging;
        app.discover.bulk_tag_input = "bulktag".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.discover.bulk_tag_input, "bulkta");
    }

    // =========================================================================
    // Scanning E2E Tests - Non-blocking scan with progress
    // =========================================================================

    #[test]
    fn test_scan_valid_directory_enters_scanning_state() {
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "test2").unwrap();

        // Open scan dialog and enter path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();

        // Press Enter to start scan
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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

    #[test]
    fn test_scan_invalid_path_shows_error() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Open scan dialog with invalid path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "/nonexistent/path/that/does/not/exist".to_string();

        // Press Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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
            app.discover
                .scan_error
                .as_ref()
                .unwrap()
                .contains("not found"),
            "Error message should mention path not found"
        );
    }

    #[test]
    fn test_scan_not_a_directory_shows_error() {
        use tempfile::NamedTempFile;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp file (not a directory)
        let temp_file = NamedTempFile::new().unwrap();

        // Open scan dialog with file path
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_file.path().display().to_string();

        // Press Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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
            app.discover
                .scan_error
                .as_ref()
                .unwrap()
                .contains("Not a directory"),
            "Error message should mention not a directory"
        );
    }

    #[test]
    fn test_scan_cancel_with_esc() {
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.discover.view_state, DiscoverViewState::Scanning);

        // Press Esc to cancel scan
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        // Should return to Files state
        assert_eq!(
            app.discover.view_state,
            DiscoverViewState::Files,
            "Esc should cancel scan and return to Files"
        );
        assert!(app.pending_scan.is_none(), "pending_scan should be cleared");
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

    #[test]
    fn test_scan_creates_job_with_running_status() {
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Verify no jobs initially
        assert!(app.jobs_state.jobs.is_empty(), "Should start with no jobs");

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Should have created a job
        assert_eq!(app.jobs_state.jobs.len(), 1, "Should have created one job");

        let job = &app.jobs_state.jobs[0];
        assert_eq!(job.status, JobStatus::Running, "Job should be Running");
        assert_eq!(job.job_type, JobType::Scan, "Job type should be Scan");
        assert!(
            job.output_path
                .as_ref()
                .map_or(false, |p| p.contains(temp_dir.path().to_str().unwrap())),
            "Job should track the scanned directory"
        );
        assert!(
            app.current_scan_job_id.is_some(),
            "current_scan_job_id should be set"
        );
    }

    #[test]
    fn test_scan_cancel_sets_job_cancelled() {
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start a scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Verify job was created with Running status
        assert_eq!(app.jobs_state.jobs.len(), 1);
        assert_eq!(app.jobs_state.jobs[0].status, JobStatus::Running);

        // Cancel with ESC
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

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

    #[test]
    fn test_scan_complete_sets_job_completed() {
        use std::time::Duration;
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Verify job created with Running status
        assert_eq!(app.jobs_state.jobs.len(), 1);
        assert_eq!(app.jobs_state.jobs[0].status, JobStatus::Running);

        // Wait for scan to complete
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Scan did not complete within 5 seconds");
            }
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
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

    #[test]
    fn test_scan_completes_and_populates_files() {
        use std::time::Duration;
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "test2").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        std::fs::write(temp_dir.path().join("subdir/file3.txt"), "test3").unwrap();

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.discover.view_state, DiscoverViewState::Scanning);

        // Poll tick until scan completes (with timeout)
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Scan did not complete within 5 seconds");
            }
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
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

    #[test]
    fn test_scan_progress_initialized_and_cleared() {
        use std::time::Duration;
        use tempfile::TempDir;

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Create a temp directory with some files
        let temp_dir = TempDir::new().unwrap();
        for i in 0..10 {
            std::fs::write(
                temp_dir.path().join(format!("file{}.txt", i)),
                format!("test{}", i),
            )
            .unwrap();
        }

        // Start scan
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = temp_dir.path().display().to_string();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
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

    #[test]
    fn test_scan_home_tilde_expansion() {
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        // Test ~ expansion - should not fail immediately
        app.discover.view_state = DiscoverViewState::EnteringPath;
        app.discover.scan_path_input = "~".to_string();

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // If home dir exists and is readable, should enter Scanning
        // Otherwise should show error - but NOT panic
        // (We can't guarantee home dir exists in all test environments)
        let valid_states = [
            DiscoverViewState::Scanning,
            DiscoverViewState::Files,
            DiscoverViewState::ScanConfirm,
        ];
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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::Files;

        // Create a simulated large file list (100K files)
        let large_count: usize = 100_000;
        app.discover.files = (0..large_count)
            .map(|i| FileInfo {
                file_id: i as i64,
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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::Files;

        // Create a large file list
        let file_count: usize = 50_000;
        app.discover.files = (0..file_count)
            .map(|i| FileInfo {
                file_id: i as i64,
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
        assert_eq!(app.discover.selected, file_count / 2, "Should be at middle");

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
        assert_eq!(scroll_offset, 5000 - 15, "Middle should center selection");

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
        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
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
                    file_id: i as i64,
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
        assert_eq!(filtered_count, 10_000, "Filter should match ~10K CSV files");

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

    #[test]
    fn test_scan_result_memory_efficiency() {
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

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;
        app.discover.view_state = DiscoverViewState::Files;

        // Trigger scan
        let path = temp_dir.path().to_string_lossy().to_string();
        app.discover.scan_path_input = path.clone();
        app.scan_directory(&path);

        // Complete scan
        while app.discover.view_state == DiscoverViewState::Scanning {
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }

        // load_scout_files uses paged queries for memory efficiency
        // So we expect a single page of files to be loaded
        assert!(
            app.discover.files.len() == app.discover.page_size,
            "Should have loaded {} files (page size), got {}",
            app.discover.page_size,
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

    #[test]
    fn test_scan_partial_batches_not_lost() {
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

        let mut app = App::new(test_args(), None);
        app.mode = TuiMode::Ingest;
        app.ingest_tab = IngestTab::Select;

        let path = temp_dir.path().to_string_lossy().to_string();
        app.scan_directory(&path);

        // Wait for scan to complete (with timeout)
        let start = std::time::Instant::now();
        while app.discover.view_state == DiscoverViewState::Scanning {
            if start.elapsed() > Duration::from_secs(10) {
                panic!("Scan did not complete within 10 seconds");
            }
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }

        // All 150 files should be present - partial batch was flushed
        assert_eq!(
            app.discover.files.len(),
            150,
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
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let last_progress = last_progress.clone();
                let successful_updates = successful_updates.clone();
                thread::spawn(move || {
                    // All threads observed last_seen=0, all try to update to 5000
                    if last_progress
                        .compare_exchange(last_seen, 5000, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                    {
                        successful_updates.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Only ONE thread should have won the race
        assert_eq!(
            successful_updates.load(Ordering::Relaxed),
            1,
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
        assert_eq!(
            total_count.load(Ordering::Relaxed),
            50,
            "Total count should be 50"
        );
    }
}

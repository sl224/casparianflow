//! Canonical snapshot states for TUI rendering tests and exports.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};

use casparian::scout::{SourceId, TaggingRuleId, Workspace, WorkspaceId};
use casparian_intent::IntentState;
use casparian_mcp::intent::ProposalId;
use uuid::Uuid;

use super::app::{
    App, ApprovalDisplayStatus, ApprovalInfo, ApprovalOperationType, ApprovalStatusFilter,
    ApprovalsViewState, BacktestInfo, CatalogTab,
    CommandPaletteMode, CommandPaletteState, DeadLetterRow, DiscoverFocus, DiscoverViewState,
    FileInfo, GateInfo, HomeStats, IngestTab, JobInfo, JobStatus, JobSummary, JobType,
    JobsListSection, JobsViewState, MonitoringState, PipelineInfo, PipelineRunInfo,
    PipelineStage, PipelineState, ProposalInfo, QuarantineRow, QueryResults, QueryState,
    QueryViewState, QueueStats, ReviewTab, RuleDialogFocus, RuleId, RuleInfo, RunTab,
    SavedQueriesState, SavedQueryEntry, SchemaMismatchRow, SessionInfo, SessionsViewState,
    SettingsCategory, SettingsState, SinkOutput, SinkStats, SourceInfo, SuggestedFix,
    TableBrowserState, TagInfo, ThroughputSample, TriageTab, TuiMode, ViolationSummary,
    ViolationType, WorkspaceSwitcherMode,
};
use super::extraction::{
    BacktestSummary, FieldSource, FieldType, FileResultsState, FileTestResult, FolderMatch,
    MatchedFile, NamingScheme, PathArchetype, PatternSeed, ResultFilter, RuleBuilderField,
    RuleBuilderFocus, RuleBuilderState, SuggestionSection, SynonymConfidence, SynonymSuggestion,
};
use super::flow_record::RecordRedaction;
use super::TuiArgs;

pub const DEFAULT_SNAPSHOT_SIZES: &[(u16, u16)] = &[(80, 24), (100, 30), (120, 40), (160, 50)];

const SNAPSHOT_DB_PATH: &str = "SNAPSHOT_DB.duckdb";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapshotCoverage {
    HomeDefault,
    HomeFiltering,
    DiscoverEmptyNoSources,
    DiscoverRuleBuilder,
    DiscoverBacktestResults,
    DiscoverScanningProgress,
    DiscoverSourcesDropdown,
    DiscoverTagsDropdown,
    DiscoverRulesManagerDialog,
    DiscoverRuleCreationDialog,
    DiscoverSourcesManagerDialog,
    DiscoverSourceEditDialog,
    DiscoverSourceDeleteConfirmDialog,
    DiscoverEnteringPathDialog,
    DiscoverScanConfirmDialog,
    DiscoverFilteringDialog,
    DiscoverTaggingDialog,
    DiscoverBulkTagDialog,
    DiscoverCreateSourceDialog,
    DiscoverSuggestionsHelpOverlay,
    DiscoverSuggestionsDetailOverlay,
    DiscoverManualTagConfirmOverlay,
    DiscoverConfirmExitOverlay,
    JobsList,
    JobsListNoPipeline,
    JobsMonitoringPanel,
    JobsViolationDetail,
    JobsDrawer,
    SourcesDrawer,
    SourcesList,
    SourcesEditOverlay,
    SourcesCreateOverlay,
    SourcesDeleteConfirmOverlay,
    ApprovalsList,
    ApprovalsConfirmApprove,
    ApprovalsConfirmReject,
    ApprovalsDetail,
    QueryEditing,
    QueryExecuting,
    QueryResults,
    QueryTableBrowser,
    QuerySavedQueries,
    SettingsGeneral,
    SettingsDisplay,
    SettingsAbout,
    SettingsEditing,
    SessionsList,
    SessionsDetail,
    SessionsWorkflowProgress,
    SessionsProposalReview,
    SessionsGateApproval,
    TriageQuarantine,
    TriageSchemaMismatch,
    TriageDeadLetter,
    CatalogPipelines,
    CatalogRuns,
    WorkspaceSwitcherList,
    WorkspaceSwitcherCreate,
    CommandPaletteIntent,
    CommandPaletteCommand,
    CommandPaletteNavigation,
    HelpOverlayDiscover,
    HelpOverlayJobs,
    HelpOverlayDefault,
}

impl SnapshotCoverage {
    pub const ALL: &'static [SnapshotCoverage] = &[
        SnapshotCoverage::HomeDefault,
        SnapshotCoverage::HomeFiltering,
        SnapshotCoverage::DiscoverEmptyNoSources,
        SnapshotCoverage::DiscoverRuleBuilder,
        SnapshotCoverage::DiscoverBacktestResults,
        SnapshotCoverage::DiscoverScanningProgress,
        SnapshotCoverage::DiscoverSourcesDropdown,
        SnapshotCoverage::DiscoverTagsDropdown,
        SnapshotCoverage::DiscoverRulesManagerDialog,
        SnapshotCoverage::DiscoverRuleCreationDialog,
        SnapshotCoverage::DiscoverSourcesManagerDialog,
        SnapshotCoverage::DiscoverSourceEditDialog,
        SnapshotCoverage::DiscoverSourceDeleteConfirmDialog,
        SnapshotCoverage::DiscoverEnteringPathDialog,
        SnapshotCoverage::DiscoverScanConfirmDialog,
        SnapshotCoverage::DiscoverFilteringDialog,
        SnapshotCoverage::DiscoverTaggingDialog,
        SnapshotCoverage::DiscoverBulkTagDialog,
        SnapshotCoverage::DiscoverCreateSourceDialog,
        SnapshotCoverage::DiscoverSuggestionsHelpOverlay,
        SnapshotCoverage::DiscoverSuggestionsDetailOverlay,
        SnapshotCoverage::DiscoverManualTagConfirmOverlay,
        SnapshotCoverage::DiscoverConfirmExitOverlay,
        SnapshotCoverage::JobsList,
        SnapshotCoverage::JobsListNoPipeline,
        SnapshotCoverage::JobsMonitoringPanel,
        SnapshotCoverage::JobsViolationDetail,
        SnapshotCoverage::JobsDrawer,
        SnapshotCoverage::SourcesDrawer,
        SnapshotCoverage::SourcesList,
        SnapshotCoverage::SourcesEditOverlay,
        SnapshotCoverage::SourcesCreateOverlay,
        SnapshotCoverage::SourcesDeleteConfirmOverlay,
        SnapshotCoverage::ApprovalsList,
        SnapshotCoverage::ApprovalsConfirmApprove,
        SnapshotCoverage::ApprovalsConfirmReject,
        SnapshotCoverage::ApprovalsDetail,
        SnapshotCoverage::QueryEditing,
        SnapshotCoverage::QueryExecuting,
        SnapshotCoverage::QueryResults,
        SnapshotCoverage::QueryTableBrowser,
        SnapshotCoverage::QuerySavedQueries,
        SnapshotCoverage::SettingsGeneral,
        SnapshotCoverage::SettingsDisplay,
        SnapshotCoverage::SettingsAbout,
        SnapshotCoverage::SettingsEditing,
        SnapshotCoverage::SessionsList,
        SnapshotCoverage::SessionsDetail,
        SnapshotCoverage::SessionsWorkflowProgress,
        SnapshotCoverage::SessionsProposalReview,
        SnapshotCoverage::SessionsGateApproval,
        SnapshotCoverage::TriageQuarantine,
        SnapshotCoverage::TriageSchemaMismatch,
        SnapshotCoverage::TriageDeadLetter,
        SnapshotCoverage::CatalogPipelines,
        SnapshotCoverage::CatalogRuns,
        SnapshotCoverage::WorkspaceSwitcherList,
        SnapshotCoverage::WorkspaceSwitcherCreate,
        SnapshotCoverage::CommandPaletteIntent,
        SnapshotCoverage::CommandPaletteCommand,
        SnapshotCoverage::CommandPaletteNavigation,
        SnapshotCoverage::HelpOverlayDiscover,
        SnapshotCoverage::HelpOverlayJobs,
        SnapshotCoverage::HelpOverlayDefault,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotCoverage::HomeDefault => "home_default",
            SnapshotCoverage::HomeFiltering => "home_filtering",
            SnapshotCoverage::DiscoverEmptyNoSources => "discover_empty_no_sources",
            SnapshotCoverage::DiscoverRuleBuilder => "discover_rule_builder",
            SnapshotCoverage::DiscoverBacktestResults => "discover_backtest_results",
            SnapshotCoverage::DiscoverScanningProgress => "discover_scanning_progress",
            SnapshotCoverage::DiscoverSourcesDropdown => "discover_sources_dropdown",
            SnapshotCoverage::DiscoverTagsDropdown => "discover_tags_dropdown",
            SnapshotCoverage::DiscoverRulesManagerDialog => "discover_rules_manager_dialog",
            SnapshotCoverage::DiscoverRuleCreationDialog => "discover_rule_creation_dialog",
            SnapshotCoverage::DiscoverSourcesManagerDialog => "discover_sources_manager_dialog",
            SnapshotCoverage::DiscoverSourceEditDialog => "discover_source_edit_dialog",
            SnapshotCoverage::DiscoverSourceDeleteConfirmDialog => {
                "discover_source_delete_confirm_dialog"
            }
            SnapshotCoverage::DiscoverEnteringPathDialog => "discover_entering_path_dialog",
            SnapshotCoverage::DiscoverScanConfirmDialog => "discover_scan_confirm_dialog",
            SnapshotCoverage::DiscoverFilteringDialog => "discover_filtering_dialog",
            SnapshotCoverage::DiscoverTaggingDialog => "discover_tagging_dialog",
            SnapshotCoverage::DiscoverBulkTagDialog => "discover_bulk_tag_dialog",
            SnapshotCoverage::DiscoverCreateSourceDialog => "discover_create_source_dialog",
            SnapshotCoverage::DiscoverSuggestionsHelpOverlay => "discover_suggestions_help_overlay",
            SnapshotCoverage::DiscoverSuggestionsDetailOverlay => {
                "discover_suggestions_detail_overlay"
            }
            SnapshotCoverage::DiscoverManualTagConfirmOverlay => {
                "discover_manual_tag_confirm_overlay"
            }
            SnapshotCoverage::DiscoverConfirmExitOverlay => "discover_confirm_exit_overlay",
            SnapshotCoverage::JobsList => "jobs_list",
            SnapshotCoverage::JobsListNoPipeline => "jobs_list_no_pipeline",
            SnapshotCoverage::JobsMonitoringPanel => "jobs_monitoring_panel",
            SnapshotCoverage::JobsViolationDetail => "jobs_violation_detail",
            SnapshotCoverage::JobsDrawer => "jobs_drawer_open",
            SnapshotCoverage::SourcesDrawer => "sources_drawer_open",
            SnapshotCoverage::SourcesList => "sources_screen",
            SnapshotCoverage::SourcesEditOverlay => "sources_edit_overlay",
            SnapshotCoverage::SourcesCreateOverlay => "sources_create_overlay",
            SnapshotCoverage::SourcesDeleteConfirmOverlay => "sources_delete_confirm",
            SnapshotCoverage::ApprovalsList => "approvals_list_mixed",
            SnapshotCoverage::ApprovalsConfirmApprove => "approvals_confirm_approve",
            SnapshotCoverage::ApprovalsConfirmReject => "approvals_confirm_reject",
            SnapshotCoverage::ApprovalsDetail => "approvals_detail",
            SnapshotCoverage::QueryEditing => "query_editor_focused",
            SnapshotCoverage::QueryExecuting => "query_executing",
            SnapshotCoverage::QueryResults => "query_results_table",
            SnapshotCoverage::QueryTableBrowser => "query_table_browser",
            SnapshotCoverage::QuerySavedQueries => "query_saved_queries",
            SnapshotCoverage::SettingsGeneral => "settings_general",
            SnapshotCoverage::SettingsDisplay => "settings_display",
            SnapshotCoverage::SettingsAbout => "settings_about",
            SnapshotCoverage::SettingsEditing => "settings_editing",
            SnapshotCoverage::SessionsList => "sessions_list",
            SnapshotCoverage::SessionsDetail => "sessions_detail",
            SnapshotCoverage::SessionsWorkflowProgress => "sessions_workflow_progress",
            SnapshotCoverage::SessionsProposalReview => "sessions_proposal_review",
            SnapshotCoverage::SessionsGateApproval => "sessions_gate_approval",
            SnapshotCoverage::TriageQuarantine => "triage_quarantine_list",
            SnapshotCoverage::TriageSchemaMismatch => "triage_schema_mismatch_list",
            SnapshotCoverage::TriageDeadLetter => "triage_dead_letter_list",
            SnapshotCoverage::CatalogPipelines => "catalog_pipelines_list",
            SnapshotCoverage::CatalogRuns => "catalog_runs_list",
            SnapshotCoverage::WorkspaceSwitcherList => "workspace_switcher_open",
            SnapshotCoverage::WorkspaceSwitcherCreate => "workspace_switcher_create",
            SnapshotCoverage::CommandPaletteIntent => "command_palette_intent",
            SnapshotCoverage::CommandPaletteCommand => "command_palette_open",
            SnapshotCoverage::CommandPaletteNavigation => "command_palette_navigation",
            SnapshotCoverage::HelpOverlayDiscover => "help_overlay_open",
            SnapshotCoverage::HelpOverlayJobs => "help_overlay_jobs",
            SnapshotCoverage::HelpOverlayDefault => "help_overlay_default",
        }
    }
}

pub struct SnapshotCase {
    pub name: &'static str,
    pub notes: &'static str,
    pub focus_hint: &'static str,
    pub coverage: SnapshotCoverage,
    pub build: fn() -> App,
}

pub fn snapshot_cases() -> &'static [SnapshotCase] {
    &SNAPSHOT_CASES
}

static SNAPSHOT_CASES: &[SnapshotCase] = &[
    SnapshotCase {
        name: "home_default",
        notes: "Home hub with seeded sources and recent jobs.",
        focus_hint: "Quick Start list",
        coverage: SnapshotCoverage::HomeDefault,
        build: case_home_default,
    },
    SnapshotCase {
        name: "home_filtering",
        notes: "Home Quick Start filter input active.",
        focus_hint: "Quick Start list",
        coverage: SnapshotCoverage::HomeFiltering,
        build: case_home_filtering,
    },
    SnapshotCase {
        name: "discover_empty_no_sources",
        notes: "Discover mode with no sources configured.",
        focus_hint: "Rule Builder header",
        coverage: SnapshotCoverage::DiscoverEmptyNoSources,
        build: case_discover_empty_no_sources,
    },
    SnapshotCase {
        name: "discover_rule_builder",
        notes: "Rule Builder with schema suggestions populated.",
        focus_hint: "Suggestions panel",
        coverage: SnapshotCoverage::DiscoverRuleBuilder,
        build: case_discover_rule_builder,
    },
    SnapshotCase {
        name: "discover_backtest_results",
        notes: "Rule Builder showing backtest results and tag focus.",
        focus_hint: "Results list",
        coverage: SnapshotCoverage::DiscoverBacktestResults,
        build: case_discover_files_list_with_filters_and_tags,
    },
    SnapshotCase {
        name: "discover_scanning_progress",
        notes: "Discover mode with scanning overlay and progress counters.",
        focus_hint: "Scanning dialog",
        coverage: SnapshotCoverage::DiscoverScanningProgress,
        build: case_discover_scanning_progress,
    },
    SnapshotCase {
        name: "discover_sources_dropdown",
        notes: "Sources dropdown open with filter input.",
        focus_hint: "Sources dropdown",
        coverage: SnapshotCoverage::DiscoverSourcesDropdown,
        build: case_discover_sources_dropdown,
    },
    SnapshotCase {
        name: "discover_tags_dropdown",
        notes: "Tags dropdown open with filter input.",
        focus_hint: "Tags dropdown",
        coverage: SnapshotCoverage::DiscoverTagsDropdown,
        build: case_discover_tags_dropdown,
    },
    SnapshotCase {
        name: "discover_rules_manager_dialog",
        notes: "Rules Manager overlay with multiple rules.",
        focus_hint: "Rules dialog",
        coverage: SnapshotCoverage::DiscoverRulesManagerDialog,
        build: case_discover_rules_manager_dialog,
    },
    SnapshotCase {
        name: "discover_rule_creation_dialog",
        notes: "Rule creation dialog with live preview.",
        focus_hint: "Pattern input",
        coverage: SnapshotCoverage::DiscoverRuleCreationDialog,
        build: case_discover_rule_creation_dialog,
    },
    SnapshotCase {
        name: "discover_sources_manager_dialog",
        notes: "Sources Manager dialog listing sources.",
        focus_hint: "Sources list",
        coverage: SnapshotCoverage::DiscoverSourcesManagerDialog,
        build: case_discover_sources_manager_dialog,
    },
    SnapshotCase {
        name: "discover_source_edit_dialog",
        notes: "Source edit dialog open.",
        focus_hint: "Name field",
        coverage: SnapshotCoverage::DiscoverSourceEditDialog,
        build: case_discover_source_edit_dialog,
    },
    SnapshotCase {
        name: "discover_source_delete_confirm_dialog",
        notes: "Source delete confirmation dialog.",
        focus_hint: "Confirm dialog",
        coverage: SnapshotCoverage::DiscoverSourceDeleteConfirmDialog,
        build: case_discover_source_delete_confirm_dialog,
    },
    SnapshotCase {
        name: "discover_entering_path_dialog",
        notes: "Add source path dialog with suggestions.",
        focus_hint: "Path input",
        coverage: SnapshotCoverage::DiscoverEnteringPathDialog,
        build: case_discover_entering_path_dialog,
    },
    SnapshotCase {
        name: "discover_scan_confirm_dialog",
        notes: "Scan confirmation warning dialog.",
        focus_hint: "Confirm scan",
        coverage: SnapshotCoverage::DiscoverScanConfirmDialog,
        build: case_discover_scan_confirm_dialog,
    },
    SnapshotCase {
        name: "discover_filtering_dialog",
        notes: "Filter dialog with active input.",
        focus_hint: "Filter input",
        coverage: SnapshotCoverage::DiscoverFilteringDialog,
        build: case_discover_filtering_dialog,
    },
    SnapshotCase {
        name: "discover_tagging_dialog",
        notes: "Tagging dialog with suggestions.",
        focus_hint: "Tag input",
        coverage: SnapshotCoverage::DiscoverTaggingDialog,
        build: case_discover_tagging_dialog,
    },
    SnapshotCase {
        name: "discover_bulk_tag_dialog",
        notes: "Bulk tag dialog with rule toggle.",
        focus_hint: "Tag input",
        coverage: SnapshotCoverage::DiscoverBulkTagDialog,
        build: case_discover_bulk_tag_dialog,
    },
    SnapshotCase {
        name: "discover_create_source_dialog",
        notes: "Create source dialog with path and name input.",
        focus_hint: "Name input",
        coverage: SnapshotCoverage::DiscoverCreateSourceDialog,
        build: case_discover_create_source_dialog,
    },
    SnapshotCase {
        name: "discover_suggestions_help_overlay",
        notes: "Suggestions help overlay open.",
        focus_hint: "Help overlay",
        coverage: SnapshotCoverage::DiscoverSuggestionsHelpOverlay,
        build: case_discover_suggestions_help_overlay,
    },
    SnapshotCase {
        name: "discover_suggestions_detail_overlay",
        notes: "Suggestions detail overlay open.",
        focus_hint: "Detail overlay",
        coverage: SnapshotCoverage::DiscoverSuggestionsDetailOverlay,
        build: case_discover_suggestions_detail_overlay,
    },
    SnapshotCase {
        name: "discover_manual_tag_confirm_overlay",
        notes: "Manual tag confirm overlay open.",
        focus_hint: "Confirm dialog",
        coverage: SnapshotCoverage::DiscoverManualTagConfirmOverlay,
        build: case_discover_manual_tag_confirm_overlay,
    },
    SnapshotCase {
        name: "discover_confirm_exit_overlay",
        notes: "Confirm exit dialog open.",
        focus_hint: "Confirm dialog",
        coverage: SnapshotCoverage::DiscoverConfirmExitOverlay,
        build: case_discover_confirm_exit_overlay,
    },
    SnapshotCase {
        name: "jobs_list",
        notes: "Jobs view with mixed job statuses and pipeline summary.",
        focus_hint: "Actionable list",
        coverage: SnapshotCoverage::JobsList,
        build: case_jobs_list_mixed_status,
    },
    SnapshotCase {
        name: "jobs_list_no_pipeline",
        notes: "Jobs view with pipeline summary hidden.",
        focus_hint: "Actionable list",
        coverage: SnapshotCoverage::JobsListNoPipeline,
        build: case_jobs_list_no_pipeline,
    },
    SnapshotCase {
        name: "jobs_monitoring_panel",
        notes: "Jobs monitoring panel with queue and throughput.",
        focus_hint: "Monitoring panel",
        coverage: SnapshotCoverage::JobsMonitoringPanel,
        build: case_jobs_monitoring_panel,
    },
    SnapshotCase {
        name: "jobs_violation_detail",
        notes: "Backtest violations detail panel.",
        focus_hint: "Violations list",
        coverage: SnapshotCoverage::JobsViolationDetail,
        build: case_jobs_violation_detail,
    },
    SnapshotCase {
        name: "jobs_drawer_open",
        notes: "Global Jobs drawer overlay open.",
        focus_hint: "Jobs drawer",
        coverage: SnapshotCoverage::JobsDrawer,
        build: case_jobs_drawer_open,
    },
    SnapshotCase {
        name: "sources_drawer_open",
        notes: "Global Sources drawer overlay open.",
        focus_hint: "Sources drawer",
        coverage: SnapshotCoverage::SourcesDrawer,
        build: case_sources_drawer_open,
    },
    SnapshotCase {
        name: "sources_screen",
        notes: "Sources list with selection and inspector.",
        focus_hint: "Sources list",
        coverage: SnapshotCoverage::SourcesList,
        build: case_sources_screen,
    },
    SnapshotCase {
        name: "sources_edit_overlay",
        notes: "Sources edit dialog open.",
        focus_hint: "Edit dialog",
        coverage: SnapshotCoverage::SourcesEditOverlay,
        build: case_sources_edit_overlay,
    },
    SnapshotCase {
        name: "sources_create_overlay",
        notes: "Sources add dialog open.",
        focus_hint: "Path input",
        coverage: SnapshotCoverage::SourcesCreateOverlay,
        build: case_sources_create_overlay,
    },
    SnapshotCase {
        name: "sources_delete_confirm",
        notes: "Sources delete confirmation open.",
        focus_hint: "Confirm dialog",
        coverage: SnapshotCoverage::SourcesDeleteConfirmOverlay,
        build: case_sources_delete_confirm,
    },
    SnapshotCase {
        name: "approvals_list_mixed",
        notes: "Approvals view with mixed statuses.",
        focus_hint: "Approvals list",
        coverage: SnapshotCoverage::ApprovalsList,
        build: case_approvals_list_mixed,
    },
    SnapshotCase {
        name: "approvals_confirm_approve",
        notes: "Approve confirmation dialog.",
        focus_hint: "Confirm dialog",
        coverage: SnapshotCoverage::ApprovalsConfirmApprove,
        build: case_approvals_confirm_approve,
    },
    SnapshotCase {
        name: "approvals_confirm_reject",
        notes: "Reject confirmation dialog with reason input.",
        focus_hint: "Reason input",
        coverage: SnapshotCoverage::ApprovalsConfirmReject,
        build: case_approvals_confirm_reject,
    },
    SnapshotCase {
        name: "approvals_detail",
        notes: "Approval detail overlay open.",
        focus_hint: "Approval detail",
        coverage: SnapshotCoverage::ApprovalsDetail,
        build: case_approvals_detail,
    },
    SnapshotCase {
        name: "query_editor_focused",
        notes: "Query console with editor focused and history.",
        focus_hint: "SQL editor",
        coverage: SnapshotCoverage::QueryEditing,
        build: case_query_editor_focused,
    },
    SnapshotCase {
        name: "query_executing",
        notes: "Query console in executing state.",
        focus_hint: "Status bar",
        coverage: SnapshotCoverage::QueryExecuting,
        build: case_query_executing,
    },
    SnapshotCase {
        name: "query_results_table",
        notes: "Query console with results table focused.",
        focus_hint: "Results table",
        coverage: SnapshotCoverage::QueryResults,
        build: case_query_results_table,
    },
    SnapshotCase {
        name: "query_table_browser",
        notes: "Query table browser overlay open.",
        focus_hint: "Tables list",
        coverage: SnapshotCoverage::QueryTableBrowser,
        build: case_query_table_browser,
    },
    SnapshotCase {
        name: "query_saved_queries",
        notes: "Saved queries overlay open.",
        focus_hint: "Saved queries list",
        coverage: SnapshotCoverage::QuerySavedQueries,
        build: case_query_saved_queries,
    },
    SnapshotCase {
        name: "settings_general",
        notes: "Settings view on General section.",
        focus_hint: "General settings",
        coverage: SnapshotCoverage::SettingsGeneral,
        build: case_settings_general,
    },
    SnapshotCase {
        name: "settings_display",
        notes: "Settings view on Display section.",
        focus_hint: "Display settings",
        coverage: SnapshotCoverage::SettingsDisplay,
        build: case_settings_display,
    },
    SnapshotCase {
        name: "settings_about",
        notes: "Settings view on About section.",
        focus_hint: "About panel",
        coverage: SnapshotCoverage::SettingsAbout,
        build: case_settings_about,
    },
    SnapshotCase {
        name: "settings_editing",
        notes: "Settings editing mode active.",
        focus_hint: "Edit field",
        coverage: SnapshotCoverage::SettingsEditing,
        build: case_settings_editing,
    },
    SnapshotCase {
        name: "sessions_list",
        notes: "Sessions list with pending gate.",
        focus_hint: "Sessions list",
        coverage: SnapshotCoverage::SessionsList,
        build: case_sessions_list,
    },
    SnapshotCase {
        name: "sessions_detail",
        notes: "Session detail view active.",
        focus_hint: "Details panel",
        coverage: SnapshotCoverage::SessionsDetail,
        build: case_sessions_detail,
    },
    SnapshotCase {
        name: "sessions_workflow_progress",
        notes: "Workflow progress view active.",
        focus_hint: "Workflow panel",
        coverage: SnapshotCoverage::SessionsWorkflowProgress,
        build: case_sessions_workflow_progress,
    },
    SnapshotCase {
        name: "sessions_proposal_review",
        notes: "Proposal review state active.",
        focus_hint: "Details panel",
        coverage: SnapshotCoverage::SessionsProposalReview,
        build: case_sessions_proposal_review,
    },
    SnapshotCase {
        name: "sessions_gate_approval",
        notes: "Gate approval panel open.",
        focus_hint: "Gate details",
        coverage: SnapshotCoverage::SessionsGateApproval,
        build: case_sessions_gate_approval,
    },
    SnapshotCase {
        name: "triage_quarantine_list",
        notes: "Quarantine triage list with raw data preview.",
        focus_hint: "Quarantine list",
        coverage: SnapshotCoverage::TriageQuarantine,
        build: case_triage_quarantine_list,
    },
    SnapshotCase {
        name: "triage_schema_mismatch_list",
        notes: "Schema mismatch triage tab.",
        focus_hint: "Schema mismatch list",
        coverage: SnapshotCoverage::TriageSchemaMismatch,
        build: case_triage_schema_mismatch_list,
    },
    SnapshotCase {
        name: "triage_dead_letter_list",
        notes: "Dead letter triage tab.",
        focus_hint: "Dead letter list",
        coverage: SnapshotCoverage::TriageDeadLetter,
        build: case_triage_dead_letter_list,
    },
    SnapshotCase {
        name: "catalog_pipelines_list",
        notes: "Pipeline catalog view.",
        focus_hint: "Pipelines list",
        coverage: SnapshotCoverage::CatalogPipelines,
        build: case_catalog_pipelines_list,
    },
    SnapshotCase {
        name: "catalog_runs_list",
        notes: "Pipeline runs catalog view.",
        focus_hint: "Catalog list",
        coverage: SnapshotCoverage::CatalogRuns,
        build: case_catalog_runs_list,
    },
    SnapshotCase {
        name: "workspace_switcher_open",
        notes: "Workspace switcher overlay open.",
        focus_hint: "Workspace list",
        coverage: SnapshotCoverage::WorkspaceSwitcherList,
        build: case_workspace_switcher_open,
    },
    SnapshotCase {
        name: "workspace_switcher_create",
        notes: "Workspace switcher create mode.",
        focus_hint: "Name input",
        coverage: SnapshotCoverage::WorkspaceSwitcherCreate,
        build: case_workspace_switcher_create,
    },
    SnapshotCase {
        name: "command_palette_intent",
        notes: "Command palette in Intent mode.",
        focus_hint: "Command palette",
        coverage: SnapshotCoverage::CommandPaletteIntent,
        build: case_command_palette_intent,
    },
    SnapshotCase {
        name: "command_palette_open",
        notes: "Command palette overlay in Command mode.",
        focus_hint: "Command palette",
        coverage: SnapshotCoverage::CommandPaletteCommand,
        build: case_command_palette_open,
    },
    SnapshotCase {
        name: "command_palette_navigation",
        notes: "Command palette in Navigation mode.",
        focus_hint: "Command palette",
        coverage: SnapshotCoverage::CommandPaletteNavigation,
        build: case_command_palette_navigation,
    },
    SnapshotCase {
        name: "help_overlay_open",
        notes: "Help overlay open on Ingest.",
        focus_hint: "Help overlay",
        coverage: SnapshotCoverage::HelpOverlayDiscover,
        build: case_help_overlay_open,
    },
    SnapshotCase {
        name: "help_overlay_jobs",
        notes: "Help overlay open on Run (Jobs).",
        focus_hint: "Help overlay",
        coverage: SnapshotCoverage::HelpOverlayJobs,
        build: case_help_overlay_jobs,
    },
    SnapshotCase {
        name: "help_overlay_default",
        notes: "Help overlay open on Home mode.",
        focus_hint: "Help overlay",
        coverage: SnapshotCoverage::HelpOverlayDefault,
        build: case_help_overlay_default,
    },
];

fn case_home_default() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.home.selected_source_index = 1;
    app.home.recent_jobs = sample_home_jobs();
    app.home.stats = sample_home_stats();
    app.home.stats_loaded = true;
    app
}

fn case_home_filtering() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.home.filtering = true;
    app.home.filter = "alpha".to_string();
    app.home.selected_source_index = 0;
    app.home.recent_jobs = sample_home_jobs();
    app.home.stats = sample_home_stats();
    app.home.stats_loaded = true;
    app
}

fn case_discover_empty_no_sources() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.sources.clear();
    app.discover.tags.clear();
    app.discover.selected_source_id = None;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_empty());
    app
}

fn case_discover_scanning_progress() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::Scanning;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app.discover.scanning_path = Some("/data/alpha".to_string());
    app.discover.scan_progress = Some(casparian::scout::ScanProgress {
        dirs_scanned: 42,
        files_found: 380,
        files_persisted: 245,
        current_dir: Some("/data/alpha/2024/Q1".to_string()),
        elapsed_ms: 12_000,
        files_per_sec: 180.0,
        stalled: false,
    });
    app.discover.scan_start_time = Some(std::time::Instant::now());
    app.tick_count = 3;
    app
}

fn case_discover_files_list_with_filters_and_tags() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.selected_tag = Some(2);
    app.discover.rule_builder = Some(sample_rule_builder_backtest());
    app
}

fn case_discover_rule_builder() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_with_suggestions());
    app
}

fn case_discover_sources_dropdown() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::SourcesDropdown;
    app.discover.sources_filtering = true;
    app.discover.sources_filter = "alpha".to_string();
    app.discover.preview_source = Some(1);
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_tags_dropdown() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::TagsDropdown;
    app.discover.focus = DiscoverFocus::Tags;
    app.discover.tags_filtering = true;
    app.discover.tags_filter = "rep".to_string();
    app.discover.preview_tag = Some(2);
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_rules_manager_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RulesManager;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app.discover.rules = sample_rules();
    app.discover.selected_rule = 1;
    app
}

fn case_discover_rule_creation_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleCreation;
    app.discover.rule_dialog_focus = RuleDialogFocus::Pattern;
    app.discover.rule_pattern_input = "**/reports/**/*.csv".to_string();
    app.discover.rule_tag_input = "report.financial".to_string();
    app.discover.rule_preview_files = vec![
        "reports/2024/Q3/report_2024-09-30_us.csv".to_string(),
        "reports/2024/Q4/report_2024-10-01_eu.csv".to_string(),
    ];
    app.discover.rule_preview_count = 128;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_sources_manager_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::SourcesManager;
    app.discover.sources_manager_selected = 1;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_source_edit_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::SourceEdit;
    app.discover.editing_source = app.discover.sources.get(1).map(|s| s.id.clone());
    app.discover.source_edit_input = "bravo-share".to_string();
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_source_delete_confirm_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::SourceDeleteConfirm;
    app.discover.source_to_delete = app.discover.sources.get(2).map(|s| s.id.clone());
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_entering_path_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::EnteringPath;
    app.discover.scan_path_input = "/data/alpha/2024".to_string();
    app.discover.path_suggestions = vec![
        "/data/alpha/2024/Q1".to_string(),
        "/data/alpha/2024/Q2".to_string(),
        "/data/alpha/2024/Q3".to_string(),
    ];
    app.discover.path_suggestion_idx = 1;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_scan_confirm_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::ScanConfirm;
    app.discover.scan_confirm_path = Some("/".to_string());
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_filtering_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::Filtering;
    app.discover.filter = "reports/**/*.csv".to_string();
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_tagging_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::Tagging;
    app.discover.files = sample_discover_files();
    app.discover.selected = 1;
    app.discover.tag_input = "rep".to_string();
    app.discover.available_tags = sample_available_tags();
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_bulk_tag_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::BulkTagging;
    app.discover.files = sample_discover_files();
    app.discover.filter = "2024".to_string();
    app.discover.bulk_tag_input = "report".to_string();
    app.discover.bulk_tag_save_as_rule = true;
    app.discover.available_tags = sample_available_tags();
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_create_source_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::CreatingSource;
    app.discover.pending_source_path = Some("/data/alpha/2024".to_string());
    app.discover.source_name_input = "alpha-2024".to_string();
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app
}

fn case_discover_suggestions_help_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    let mut builder = sample_rule_builder_with_suggestions();
    builder.suggestions_help_open = true;
    app.discover.rule_builder = Some(builder);
    app
}

fn case_discover_suggestions_detail_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    let mut builder = sample_rule_builder_with_suggestions();
    builder.suggestions_section = SuggestionSection::Synonyms;
    builder.selected_synonym = 1;
    builder.suggestions_detail_open = true;
    app.discover.rule_builder = Some(builder);
    app
}

fn case_discover_manual_tag_confirm_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    let mut builder = sample_rule_builder_backtest();
    builder.manual_tag_confirm_open = true;
    builder.manual_tag_confirm_count = 12;
    app.discover.rule_builder = Some(builder);
    app
}

fn case_discover_confirm_exit_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    let mut builder = sample_rule_builder_basic();
    builder.confirm_exit_open = true;
    builder.dirty = true;
    app.discover.rule_builder = Some(builder);
    app
}

fn case_jobs_list_mixed_status() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Jobs;
    app.jobs_state.view_state = JobsViewState::JobList;
    app.jobs_state.section_focus = JobsListSection::Actionable;
    app.jobs_state.selected_index = 1;
    app.jobs_state.show_pipeline = true;
    app.jobs_state.pipeline = sample_pipeline();
    app
}

fn case_jobs_list_no_pipeline() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Jobs;
    app.jobs_state.view_state = JobsViewState::JobList;
    app.jobs_state.section_focus = JobsListSection::Actionable;
    app.jobs_state.selected_index = 1;
    app.jobs_state.show_pipeline = false;
    app
}

fn case_jobs_monitoring_panel() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Jobs;
    app.jobs_state.view_state = JobsViewState::MonitoringPanel;
    app.jobs_state.monitoring = sample_monitoring_state();
    app
}

fn case_jobs_violation_detail() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Jobs;
    app.jobs_state.view_state = JobsViewState::ViolationDetail;
    let mut jobs = sample_jobs();
    if let Some(job) = jobs.get_mut(2) {
        job.violations = sample_violation_summaries();
        job.top_violations_loaded = true;
        job.selected_violation_index = 1;
        app.jobs_state.pinned_job_id = Some(job.id);
    }
    app.jobs_state.jobs = jobs;
    app
}

fn case_jobs_drawer_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.jobs_drawer_open = true;
    app.jobs_drawer_selected = 2;
    app
}

fn case_sources_drawer_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.sources_drawer_open = true;
    app.sources_drawer_selected = 1;
    app
}

fn case_approvals_list_mixed() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Approvals;
    app.approvals_state.view_state = ApprovalsViewState::List;
    app.approvals_state.filter = ApprovalStatusFilter::All;
    app.approvals_state.approvals = sample_approvals();
    app.approvals_state.approvals_loaded = true;
    app.approvals_state.selected_index = 0;
    app
}

fn case_approvals_confirm_approve() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Approvals;
    app.approvals_state.view_state = ApprovalsViewState::ConfirmApprove;
    app.approvals_state.approvals = sample_approvals();
    app.approvals_state.approvals_loaded = true;
    app.approvals_state.selected_index = 0;
    app
}

fn case_approvals_confirm_reject() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Approvals;
    app.approvals_state.view_state = ApprovalsViewState::ConfirmReject;
    app.approvals_state.approvals = sample_approvals();
    app.approvals_state.approvals_loaded = true;
    app.approvals_state.selected_index = 0;
    app.approvals_state.rejection_reason = "Policy requires review".to_string();
    app
}

fn case_approvals_detail() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Approvals;
    app.approvals_state.view_state = ApprovalsViewState::Detail;
    app.approvals_state.approvals = sample_approvals();
    app.approvals_state.approvals_loaded = true;
    app.approvals_state.selected_index = 1;
    app
}

fn case_sessions_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Sessions;
    app.sessions_state.view_state = SessionsViewState::SessionList;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 0;
    app
}

fn case_sessions_detail() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Sessions;
    app.sessions_state.view_state = SessionsViewState::SessionDetail;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 1;
    app
}

fn case_sessions_workflow_progress() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Sessions;
    app.sessions_state.view_state = SessionsViewState::WorkflowProgress;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 1;
    app
}

fn case_sessions_proposal_review() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Sessions;
    app.sessions_state.view_state = SessionsViewState::ProposalReview;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 2;
    app.sessions_state.current_proposal = Some(sample_proposal_info());
    app
}

fn case_sessions_gate_approval() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Sessions;
    app.sessions_state.view_state = SessionsViewState::GateApproval;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 0;
    app.sessions_state.pending_gate = Some(sample_gate_info());
    app
}

fn case_triage_quarantine_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Triage;
    app.triage_state.tab = TriageTab::Quarantine;
    app.triage_state.quarantine_rows = Some(sample_quarantine_rows());
    app.triage_state.schema_mismatches = Some(sample_schema_mismatches());
    app.triage_state.dead_letters = Some(sample_dead_letters());
    app.triage_state.selected_index = 1;
    app.triage_state.loaded = true;
    app
}

fn case_triage_schema_mismatch_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Triage;
    app.triage_state.tab = TriageTab::SchemaMismatch;
    app.triage_state.quarantine_rows = Some(sample_quarantine_rows());
    app.triage_state.schema_mismatches = Some(sample_schema_mismatches());
    app.triage_state.dead_letters = Some(sample_dead_letters());
    app.triage_state.selected_index = 0;
    app.triage_state.loaded = true;
    app
}

fn case_triage_dead_letter_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Review;
    app.review_tab = ReviewTab::Triage;
    app.triage_state.tab = TriageTab::DeadLetter;
    app.triage_state.quarantine_rows = Some(sample_quarantine_rows());
    app.triage_state.schema_mismatches = Some(sample_schema_mismatches());
    app.triage_state.dead_letters = Some(sample_dead_letters());
    app.triage_state.selected_index = 0;
    app.triage_state.loaded = true;
    app
}

fn case_catalog_runs_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Outputs;
    app.catalog_state.tab = CatalogTab::Runs;
    app.catalog_state.pipelines = Some(sample_pipelines());
    app.catalog_state.runs = Some(sample_pipeline_runs());
    app.catalog_state.selected_index = 0;
    app.catalog_state.loaded = true;
    app
}

fn case_catalog_pipelines_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Outputs;
    app.catalog_state.tab = CatalogTab::Pipelines;
    app.catalog_state.pipelines = Some(sample_pipelines());
    app.catalog_state.runs = Some(sample_pipeline_runs());
    app.catalog_state.selected_index = 0;
    app.catalog_state.loaded = true;
    app
}

fn case_workspace_switcher_open() -> App {
    let mut app = base_app();
    let workspaces = sample_workspace_list();
    app.mode = TuiMode::Home;
    app.active_workspace = workspaces.get(1).cloned();
    app.workspace_switcher.visible = true;
    app.workspace_switcher.mode = WorkspaceSwitcherMode::List;
    app.workspace_switcher.workspaces = workspaces;
    app.workspace_switcher.selected_index = 1;
    app.workspace_switcher.loaded = true;
    app
}

fn case_workspace_switcher_create() -> App {
    let mut app = base_app();
    let workspaces = sample_workspace_list();
    app.mode = TuiMode::Home;
    app.active_workspace = workspaces.first().cloned();
    app.workspace_switcher.visible = true;
    app.workspace_switcher.mode = WorkspaceSwitcherMode::Creating;
    app.workspace_switcher.workspaces = workspaces;
    app.workspace_switcher.input = "delta-lab".to_string();
    app.workspace_switcher.status_message = Some("Name available".to_string());
    app.workspace_switcher.loaded = true;
    app
}

fn case_query_editor_focused() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Query;
    app.query_state = QueryState {
        view_state: QueryViewState::Editing,
        sql_input: "SELECT source, tag, COUNT(*) AS count\nFROM scout_file_tags\nWHERE tag LIKE 'report.%'\nGROUP BY source, tag\nORDER BY count DESC".to_string(),
        cursor_position: 64,
        history: vec![
            "SELECT * FROM scout_files LIMIT 50".to_string(),
            "SELECT tag, COUNT(*) FROM scout_file_tags GROUP BY tag".to_string(),
        ],
        history_index: None,
        results: None,
        error: None,
        status_message: None,
        executing: false,
        execution_time_ms: None,
        draft_input: None,
        table_browser: TableBrowserState::default(),
        saved_queries: SavedQueriesState::default(),
    };
    app
}

fn case_query_executing() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Query;
    app.query_state = QueryState {
        view_state: QueryViewState::Executing,
        sql_input: "SELECT * FROM scout_jobs WHERE status = 'FAILED'".to_string(),
        cursor_position: 0,
        history: vec![
            "SELECT * FROM scout_jobs LIMIT 50".to_string(),
            "SELECT tag, COUNT(*) FROM scout_file_tags GROUP BY tag".to_string(),
        ],
        history_index: None,
        results: None,
        error: None,
        status_message: Some("Executing query...".to_string()),
        executing: true,
        execution_time_ms: None,
        draft_input: None,
        table_browser: TableBrowserState::default(),
        saved_queries: SavedQueriesState::default(),
    };
    app
}

fn case_query_results_table() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Query;
    app.query_state = QueryState {
        view_state: QueryViewState::ViewingResults,
        sql_input:
            "SELECT id, name, status, created_at FROM scout_jobs ORDER BY created_at DESC LIMIT 20"
                .to_string(),
        cursor_position: 0,
        history: vec![
            "SELECT * FROM scout_jobs WHERE status = 'FAILED'".to_string(),
            "SELECT tag, COUNT(*) FROM scout_file_tags GROUP BY tag".to_string(),
        ],
        history_index: None,
        results: Some(sample_query_results()),
        error: None,
        status_message: None,
        executing: false,
        execution_time_ms: Some(128),
        draft_input: None,
        table_browser: TableBrowserState::default(),
        saved_queries: SavedQueriesState::default(),
    };
    app
}

fn case_query_table_browser() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Query;
    app.query_state = QueryState {
        view_state: QueryViewState::TableBrowser,
        sql_input: "SELECT * FROM scout_jobs".to_string(),
        cursor_position: 0,
        history: vec![],
        history_index: None,
        results: None,
        error: None,
        status_message: None,
        executing: false,
        execution_time_ms: None,
        draft_input: None,
        table_browser: TableBrowserState {
            tables: sample_query_tables(),
            selected_index: 1,
            loaded: true,
            error: None,
        },
        saved_queries: SavedQueriesState::default(),
    };
    app
}

fn case_query_saved_queries() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Query;
    app.query_state = QueryState {
        view_state: QueryViewState::SavedQueries,
        sql_input: "SELECT * FROM scout_jobs".to_string(),
        cursor_position: 0,
        history: vec![],
        history_index: None,
        results: None,
        error: None,
        status_message: None,
        executing: false,
        execution_time_ms: None,
        draft_input: None,
        table_browser: TableBrowserState::default(),
        saved_queries: SavedQueriesState {
            entries: sample_saved_queries(),
            selected_index: 0,
            loaded: true,
            error: None,
        },
    };
    app
}

fn case_settings_general() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Settings;
    let mut settings = sample_settings_state();
    settings.category = SettingsCategory::General;
    settings.selected_index = 0;
    settings.editing = false;
    app.settings = settings;
    app
}

fn case_settings_display() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Settings;
    let mut settings = sample_settings_state();
    settings.category = SettingsCategory::Display;
    settings.selected_index = 1;
    settings.editing = false;
    app.settings = settings;
    app
}

fn case_settings_about() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Settings;
    let mut settings = sample_settings_state();
    settings.category = SettingsCategory::About;
    settings.selected_index = 0;
    settings.editing = false;
    app.settings = settings;
    app
}

fn case_settings_editing() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Settings;
    let mut settings = sample_settings_state();
    settings.category = SettingsCategory::General;
    settings.selected_index = 0;
    settings.editing = true;
    settings.edit_value = "/data/alpha".to_string();
    app.settings = settings;
    app
}

fn case_command_palette_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    let mut palette = CommandPaletteState::new();
    palette.visible = true;
    palette.mode = CommandPaletteMode::Command;
    palette.input = "scan /data".to_string();
    palette.cursor = palette.input.len();
    palette.recent_intents = vec![
        "find all csv files in /data".to_string(),
        "process sales reports".to_string(),
    ];
    palette.update_suggestions();
    app.command_palette = palette;
    app
}

fn case_command_palette_intent() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    let mut palette = CommandPaletteState::new();
    palette.visible = true;
    palette.mode = CommandPaletteMode::Intent;
    palette.input = "find all trades".to_string();
    palette.cursor = palette.input.len();
    palette.recent_intents = vec![
        "process sales reports".to_string(),
        "find all csv files in /data".to_string(),
    ];
    palette.update_suggestions();
    app.command_palette = palette;
    app
}

fn case_command_palette_navigation() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    let mut palette = CommandPaletteState::new();
    palette.visible = true;
    palette.mode = CommandPaletteMode::Navigation;
    palette.input = "run".to_string();
    palette.cursor = palette.input.len();
    palette.update_suggestions();
    app.command_palette = palette;
    app
}

fn case_help_overlay_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Select;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app.show_help = true;
    app
}

fn case_help_overlay_jobs() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Run;
    app.run_tab = RunTab::Jobs;
    app.jobs_state.view_state = JobsViewState::JobList;
    app.jobs_state.section_focus = JobsListSection::Actionable;
    app.show_help = true;
    app
}

fn case_help_overlay_default() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.show_help = true;
    app
}

fn case_sources_screen() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Sources;
    app.sources_state.selected_index = 1;
    app
}

fn case_sources_edit_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Sources;
    app.sources_state.selected_index = 0;
    app.sources_state.editing = true;
    app.sources_state.creating = false;
    app.sources_state.edit_value = "/data/alpha".to_string();
    app
}

fn case_sources_create_overlay() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Sources;
    app.sources_state.selected_index = 0;
    app.sources_state.editing = true;
    app.sources_state.creating = true;
    app.sources_state.edit_value = "/data/new_source".to_string();
    app
}

fn case_sources_delete_confirm() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Ingest;
    app.ingest_tab = IngestTab::Sources;
    app.sources_state.selected_index = 1;
    app.sources_state.confirm_delete = true;
    app
}

fn base_app() -> App {
    let args = TuiArgs {
        database: Some(PathBuf::from(SNAPSHOT_DB_PATH)),
        record_flow: None,
        record_redaction: RecordRedaction::Plaintext,
        record_checkpoint_every: None,
    };
    let mut app = App::new(args, None);
    app.tick_count = 4;
    app.active_workspace = Some(sample_workspace());

    app.discover.sources = sample_sources();
    app.discover.tags = sample_tags();
    app.discover.sources_loaded = true;
    app.discover.select_source_by_index(0);
    app.discover.focus = DiscoverFocus::Files;
    app.discover.rule_dialog_focus = RuleDialogFocus::Pattern;

    app.jobs_state.jobs = sample_jobs();
    app.jobs_state.jobs_loaded = true;
    app.jobs_state.show_pipeline = true;
    app.jobs_state.pipeline = sample_pipeline();

    app
}

fn sample_workspace() -> Workspace {
    Workspace {
        id: WorkspaceId::parse("11111111-1111-1111-1111-111111111111")
            .expect("snapshot workspace id"),
        name: "alpha-case".to_string(),
        created_at: base_utc(),
    }
}

fn sample_workspace_list() -> Vec<Workspace> {
    vec![
        Workspace {
            id: WorkspaceId::parse("11111111-1111-1111-1111-111111111111")
                .expect("snapshot workspace id"),
            name: "alpha-case".to_string(),
            created_at: base_utc(),
        },
        Workspace {
            id: WorkspaceId::parse("22222222-2222-2222-2222-222222222222")
                .expect("snapshot workspace id"),
            name: "bravo-ops".to_string(),
            created_at: base_utc() + Duration::hours(2),
        },
        Workspace {
            id: WorkspaceId::parse("33333333-3333-3333-3333-333333333333")
                .expect("snapshot workspace id"),
            name: "charlie-archive".to_string(),
            created_at: base_utc() + Duration::hours(4),
        },
    ]
}

fn base_utc() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 10, 1, 9, 30, 0)
        .single()
        .expect("snapshot base timestamp")
}

fn local_at(offset_minutes: i64) -> DateTime<Local> {
    let dt = base_utc() + Duration::minutes(offset_minutes);
    DateTime::<Local>::from(dt)
}

fn source_info(id: i64, name: &str, path: &str, file_count: usize) -> SourceInfo {
    SourceInfo {
        id: SourceId::try_from(id).expect("snapshot source id"),
        name: name.to_string(),
        path: PathBuf::from(path),
        file_count,
    }
}

fn sample_sources() -> Vec<SourceInfo> {
    vec![
        source_info(1, "alpha-lake", "/data/alpha", 1280),
        source_info(2, "bravo-share", "/mnt/bravo/share", 342),
        source_info(
            3,
            "charlie-archive-long-name",
            "/Volumes/archive/2024/charlie",
            9050,
        ),
    ]
}

fn sample_tags() -> Vec<TagInfo> {
    vec![
        TagInfo {
            name: "All files".to_string(),
            count: 10345,
            is_special: true,
        },
        TagInfo {
            name: "untagged".to_string(),
            count: 120,
            is_special: true,
        },
        TagInfo {
            name: "report".to_string(),
            count: 840,
            is_special: false,
        },
        TagInfo {
            name: "trade".to_string(),
            count: 410,
            is_special: false,
        },
        TagInfo {
            name: "raw".to_string(),
            count: 92,
            is_special: false,
        },
    ]
}

fn sample_available_tags() -> Vec<String> {
    vec![
        "report".to_string(),
        "report.financial".to_string(),
        "trade".to_string(),
        "raw".to_string(),
    ]
}

fn sample_discover_files() -> Vec<FileInfo> {
    vec![
        FileInfo {
            file_id: 1001,
            path: "/data/alpha/reports/2024/Q1/report_2024-01.csv".to_string(),
            rel_path: "reports/2024/Q1/report_2024-01.csv".to_string(),
            size: 1_048_576,
            modified: local_at(15),
            is_dir: false,
            tags: vec!["report".to_string()],
        },
        FileInfo {
            file_id: 1002,
            path: "/data/alpha/reports/2024/Q2/report_2024-04.csv".to_string(),
            rel_path: "reports/2024/Q2/report_2024-04.csv".to_string(),
            size: 1_092_000,
            modified: local_at(35),
            is_dir: false,
            tags: vec!["report".to_string()],
        },
        FileInfo {
            file_id: 1003,
            path: "/data/alpha/trades/2024/trade_2024-02.csv".to_string(),
            rel_path: "trades/2024/trade_2024-02.csv".to_string(),
            size: 824_320,
            modified: local_at(55),
            is_dir: false,
            tags: vec!["trade".to_string()],
        },
        FileInfo {
            file_id: 1004,
            path: "/data/alpha/raw/2023/ingest_2023-12-31.json".to_string(),
            rel_path: "raw/2023/ingest_2023-12-31.json".to_string(),
            size: 2_097_152,
            modified: local_at(75),
            is_dir: false,
            tags: vec!["raw".to_string()],
        },
        FileInfo {
            file_id: 1005,
            path: "/data/alpha/notes/readme.txt".to_string(),
            rel_path: "notes/readme.txt".to_string(),
            size: 12_288,
            modified: local_at(95),
            is_dir: false,
            tags: vec![],
        },
    ]
}

fn sample_home_stats() -> HomeStats {
    HomeStats {
        file_count: 10345,
        source_count: 3,
        running_jobs: 2,
        pending_jobs: 1,
        failed_jobs: 1,
        parser_count: 7,
        paused_parsers: 1,
    }
}

fn sample_settings_state() -> SettingsState {
    SettingsState {
        category: SettingsCategory::General,
        selected_index: 0,
        editing: false,
        edit_value: String::new(),
        previous_mode: Some(TuiMode::Home),
        default_source_path: "~/data".to_string(),
        auto_scan_on_startup: true,
        confirm_destructive: true,
        theme: "dark".to_string(),
        unicode_symbols: true,
        show_hidden_files: false,
    }
}

fn sample_home_jobs() -> Vec<JobSummary> {
    vec![
        JobSummary {
            id: 201,
            job_type: "SCAN".to_string(),
            description: "alpha-lake".to_string(),
            status: JobStatus::Completed,
            progress_percent: Some(100),
            duration_secs: Some(52.2),
        },
        JobSummary {
            id: 202,
            job_type: "PARSE".to_string(),
            description: "trades_parser".to_string(),
            status: JobStatus::Running,
            progress_percent: Some(48),
            duration_secs: None,
        },
        JobSummary {
            id: 203,
            job_type: "BACKTEST".to_string(),
            description: "report_rules".to_string(),
            status: JobStatus::PartialSuccess,
            progress_percent: Some(100),
            duration_secs: Some(31.0),
        },
        JobSummary {
            id: 204,
            job_type: "PARSE".to_string(),
            description: "broken_parser".to_string(),
            status: JobStatus::Failed,
            progress_percent: Some(100),
            duration_secs: Some(4.1),
        },
    ]
}

fn sample_jobs() -> Vec<JobInfo> {
    let mut jobs = vec![
        job_info(
            9101,
            JobType::Scan,
            "alpha-lake",
            JobStatus::Running,
            0,
            None,
            Some(12),
        ),
        job_info(
            9102,
            JobType::Parse,
            "trades_parser",
            JobStatus::Pending,
            -30,
            None,
            None,
        ),
        job_info(
            9103,
            JobType::Backtest,
            "report_rules",
            JobStatus::PartialSuccess,
            -120,
            Some(45),
            Some(4),
        ),
        job_info(
            9104,
            JobType::Parse,
            "broken_parser",
            JobStatus::Failed,
            -240,
            Some(12),
            Some(0),
        ),
        job_info(
            9105,
            JobType::SchemaEval,
            "schema_seeds",
            JobStatus::Completed,
            -360,
            Some(18),
            None,
        ),
    ];

    if let Some(job) = jobs.first_mut() {
        job.completed_at = Some(job.started_at + Duration::minutes(15));
        job.items_processed = 0;
    }

    jobs
}

fn sample_violation_summaries() -> Vec<ViolationSummary> {
    vec![
        ViolationSummary {
            violation_type: ViolationType::TypeMismatch,
            count: 42,
            pct_of_rows: 3.5,
            column: "amount".to_string(),
            samples: vec!["12.34".to_string(), "N/A".to_string(), "null".to_string()],
            suggested_fix: Some(SuggestedFix::ChangeType {
                from: "VARCHAR".to_string(),
                to: "DECIMAL(10,2)".to_string(),
            }),
            confidence: Some("HIGH".to_string()),
            expected: Some("DECIMAL(10,2)".to_string()),
            actual: Some("VARCHAR".to_string()),
        },
        ViolationSummary {
            violation_type: ViolationType::NullNotAllowed,
            count: 19,
            pct_of_rows: 1.2,
            column: "trade_id".to_string(),
            samples: vec!["".to_string(), "null".to_string()],
            suggested_fix: Some(SuggestedFix::MakeNullable),
            confidence: Some("MEDIUM".to_string()),
            expected: Some("NOT NULL".to_string()),
            actual: Some("NULL".to_string()),
        },
        ViolationSummary {
            violation_type: ViolationType::FormatMismatch,
            count: 11,
            pct_of_rows: 0.7,
            column: "trade_date".to_string(),
            samples: vec!["2024/10/01".to_string(), "10-01-2024".to_string()],
            suggested_fix: Some(SuggestedFix::ChangeFormat {
                suggested: "YYYY-MM-DD".to_string(),
            }),
            confidence: Some("LOW".to_string()),
            expected: Some("YYYY-MM-DD".to_string()),
            actual: Some("MM/DD/YYYY".to_string()),
        },
    ]
}

fn job_info(
    id: i64,
    job_type: JobType,
    name: &str,
    status: JobStatus,
    start_offset_minutes: i64,
    duration_minutes: Option<i64>,
    quarantine_rows: Option<i64>,
) -> JobInfo {
    let started_at = local_at(start_offset_minutes);
    let completed_at = duration_minutes.map(|mins| local_at(start_offset_minutes + mins));
    let mut job = JobInfo {
        id,
        file_id: Some(id * 10),
        job_type,
        name: name.to_string(),
        version: Some("1.2.3".to_string()),
        status,
        started_at,
        completed_at,
        pipeline_run_id: Some(format!("run-{}", id)),
        logical_date: Some("2024-10-01".to_string()),
        selection_snapshot_hash: Some("ab12cd34".to_string()),
        quarantine_rows,
        items_total: 1200,
        items_processed: 540,
        items_failed: 3,
        output_path: Some(format!("/data/output/{}.parquet", name)),
        output_size_bytes: Some(42_000_000),
        backtest: None,
        failures: vec![],
        violations: vec![],
        top_violations_loaded: false,
        selected_violation_index: 0,
    };

    if matches!(job_type, JobType::Backtest) {
        job.backtest = Some(BacktestInfo {
            pass_rate: 0.92,
            iteration: 3,
            high_failure_passed: 2,
        });
    }

    job
}

fn sample_pipeline() -> PipelineState {
    PipelineState {
        source: PipelineStage {
            count: 1200,
            in_progress: 2,
        },
        parsed: PipelineStage {
            count: 980,
            in_progress: 1,
        },
        output: PipelineStage {
            count: 410,
            in_progress: 1,
        },
        active_parser: Some("trades_parser".to_string()),
    }
}

fn sample_monitoring_state() -> MonitoringState {
    let queue = QueueStats {
        pending: 12,
        running: 4,
        completed: 320,
        failed: 5,
        depth_history: VecDeque::from(vec![2, 4, 8, 6, 10, 12, 9, 7, 6, 5, 8, 9, 11, 10, 7]),
    };

    let mut throughput_history = VecDeque::new();
    for i in 0..20 {
        let rows = 800.0 + (i as f64 * 75.0);
        throughput_history.push_back(ThroughputSample {
            timestamp: local_at(-5 + i as i64),
            rows_per_second: rows,
        });
    }

    let sinks = vec![
        SinkStats {
            uri: "duckdb://local".to_string(),
            total_rows: 1_240_000,
            total_bytes: 280_000_000,
            error_count: 2,
            latency_p50_ms: 38,
            latency_p99_ms: 210,
            outputs: vec![
                SinkOutput {
                    name: "trades".to_string(),
                    rows: 620_000,
                    bytes: 140_000_000,
                },
                SinkOutput {
                    name: "reports".to_string(),
                    rows: 420_000,
                    bytes: 110_000_000,
                },
            ],
        },
        SinkStats {
            uri: "s3://archive".to_string(),
            total_rows: 480_000,
            total_bytes: 90_000_000,
            error_count: 0,
            latency_p50_ms: 120,
            latency_p99_ms: 480,
            outputs: vec![SinkOutput {
                name: "archive".to_string(),
                rows: 480_000,
                bytes: 90_000_000,
            }],
        },
    ];

    MonitoringState {
        queue,
        throughput_history,
        sinks,
        paused: false,
    }
}

fn sample_approvals() -> Vec<ApprovalInfo> {
    vec![
        ApprovalInfo {
            id: "apr-001".to_string(),
            operation_type: ApprovalOperationType::Run,
            plugin_ref: "parsers/trades_v1".to_string(),
            summary: "Run trades parser on /data/trades".to_string(),
            status: ApprovalDisplayStatus::Pending,
            created_at: local_at(-15),
            expires_at: local_at(45),
            file_count: Some(1200),
            input_dir: Some("/data/trades".to_string()),
            job_id: None,
        },
        ApprovalInfo {
            id: "apr-002".to_string(),
            operation_type: ApprovalOperationType::SchemaPromote,
            plugin_ref: "parsers/orders_v2".to_string(),
            summary: "Promote schema v2 for orders".to_string(),
            status: ApprovalDisplayStatus::Approved,
            created_at: local_at(-120),
            expires_at: local_at(-60),
            file_count: None,
            input_dir: None,
            job_id: Some("job-4421".to_string()),
        },
        ApprovalInfo {
            id: "apr-003".to_string(),
            operation_type: ApprovalOperationType::Run,
            plugin_ref: "parsers/hl7_v1".to_string(),
            summary: "Backfill HL7 messages".to_string(),
            status: ApprovalDisplayStatus::Rejected,
            created_at: local_at(-300),
            expires_at: local_at(-240),
            file_count: Some(3400),
            input_dir: Some("/data/hl7".to_string()),
            job_id: None,
        },
    ]
}

fn sample_sessions() -> Vec<SessionInfo> {
    vec![
        SessionInfo {
            id: "8f3b2c7a-1111-4d22-9a1b-acde00000001".to_string(),
            intent: "Ingest trades from /data/trades".to_string(),
            state: Some(IntentState::AwaitingSelectionApproval),
            state_label: IntentState::AwaitingSelectionApproval.as_str().to_string(),
            created_at: local_at(-5),
            file_count: 1200,
            pending_gate: Some("G1".to_string()),
        },
        SessionInfo {
            id: "8f3b2c7a-1111-4d22-9a1b-acde00000002".to_string(),
            intent: "Tag invoices by region".to_string(),
            state: Some(IntentState::ProposeTagRules),
            state_label: IntentState::ProposeTagRules.as_str().to_string(),
            created_at: local_at(-45),
            file_count: 340,
            pending_gate: None,
        },
        SessionInfo {
            id: "8f3b2c7a-1111-4d22-9a1b-acde00000003".to_string(),
            intent: "Publish orders parser".to_string(),
            state: Some(IntentState::Completed),
            state_label: IntentState::Completed.as_str().to_string(),
            created_at: local_at(-240),
            file_count: 780,
            pending_gate: None,
        },
    ]
}

fn sample_proposal_info() -> ProposalInfo {
    ProposalInfo {
        id: "prop-01".to_string(),
        proposal_type: "Selection".to_string(),
        summary: "Select trades and reports from /data/alpha".to_string(),
        confidence: "MEDIUM".to_string(),
        created_at: local_at(-20),
    }
}

fn sample_gate_info() -> GateInfo {
    GateInfo {
        gate_id: "G1".to_string(),
        gate_name: "File Selection".to_string(),
        proposal_summary: "Select trades and reports from /data/alpha".to_string(),
        evidence: vec![
            "Matched 1,240 files".to_string(),
            "Filtered out 312 hidden files".to_string(),
        ],
        confidence: "HIGH".to_string(),
        selected_examples: vec![
            "reports/2024/Q4/report_2024-10-01_us.csv".to_string(),
            "trades/2024/10/trade_2024-10-01.parquet".to_string(),
        ],
        near_miss_examples: vec!["reports/2024/Q4/README.txt".to_string()],
        next_actions: vec![
            "Approve selection to begin tagging".to_string(),
            "Review ignored file types".to_string(),
        ],
        proposal_id: ProposalId::from_uuid(
            Uuid::parse_str("11111111-2222-3333-4444-555555555555").expect("proposal id"),
        ),
        approval_target_hash: "hash-abc123".to_string(),
    }
}

fn sample_quarantine_rows() -> Vec<QuarantineRow> {
    vec![
        QuarantineRow {
            id: 101,
            job_id: 9104,
            row_index: 42,
            error_reason: "Invalid decimal in amount".to_string(),
            raw_data: Some(b"{\"order_id\":42,\"amount\":\"oops\"}".to_vec()),
            created_at: "2026-01-25T12:03:14Z".to_string(),
        },
        QuarantineRow {
            id: 102,
            job_id: 9104,
            row_index: 108,
            error_reason: "Missing required field: customer_id".to_string(),
            raw_data: Some(b"{\"order_id\":108,\"amount\":19.5}".to_vec()),
            created_at: "2026-01-25T12:03:19Z".to_string(),
        },
    ]
}

fn sample_schema_mismatches() -> Vec<SchemaMismatchRow> {
    vec![SchemaMismatchRow {
        id: 201,
        job_id: 9103,
        output_name: "orders".to_string(),
        mismatch_kind: "type_mismatch".to_string(),
        expected_name: Some("amount".to_string()),
        actual_name: Some("amount".to_string()),
        expected_type: Some("DECIMAL(10,2)".to_string()),
        actual_type: Some("VARCHAR".to_string()),
        expected_index: Some(3),
        actual_index: Some(3),
        created_at: "2026-01-25T12:05:00Z".to_string(),
    }]
}

fn sample_dead_letters() -> Vec<DeadLetterRow> {
    vec![DeadLetterRow {
        id: 301,
        original_job_id: 9102,
        file_id: Some(5541),
        plugin_name: "trades_parser".to_string(),
        error_message: Some("Worker crash during parse".to_string()),
        retry_count: 2,
        moved_at: "2026-01-25T12:06:00Z".to_string(),
        reason: Some("Exceeded retries".to_string()),
    }]
}

fn sample_pipelines() -> Vec<PipelineInfo> {
    vec![
        PipelineInfo {
            id: "pipe-001".to_string(),
            name: "trades_daily".to_string(),
            version: 3,
            created_at: "2026-01-20T12:00:00Z".to_string(),
        },
        PipelineInfo {
            id: "pipe-002".to_string(),
            name: "orders_weekly".to_string(),
            version: 1,
            created_at: "2026-01-18T08:15:00Z".to_string(),
        },
    ]
}

fn sample_pipeline_runs() -> Vec<PipelineRunInfo> {
    vec![
        PipelineRunInfo {
            id: "run-7781".to_string(),
            pipeline_id: "pipe-001".to_string(),
            pipeline_name: Some("trades_daily".to_string()),
            pipeline_version: Some(3),
            logical_date: "2026-01-25".to_string(),
            status: "COMPLETED".to_string(),
            selection_snapshot_hash: Some("ab12cd34".to_string()),
            started_at: Some("2026-01-25T05:00:00Z".to_string()),
            completed_at: Some("2026-01-25T05:06:12Z".to_string()),
        },
        PipelineRunInfo {
            id: "run-7780".to_string(),
            pipeline_id: "pipe-001".to_string(),
            pipeline_name: Some("trades_daily".to_string()),
            pipeline_version: Some(3),
            logical_date: "2026-01-24".to_string(),
            status: "FAILED".to_string(),
            selection_snapshot_hash: Some("de45fa11".to_string()),
            started_at: Some("2026-01-24T05:00:00Z".to_string()),
            completed_at: Some("2026-01-24T05:02:44Z".to_string()),
        },
        PipelineRunInfo {
            id: "run-6601".to_string(),
            pipeline_id: "pipe-002".to_string(),
            pipeline_name: Some("orders_weekly".to_string()),
            pipeline_version: Some(1),
            logical_date: "2026-01-19".to_string(),
            status: "RUNNING".to_string(),
            selection_snapshot_hash: None,
            started_at: Some("2026-01-19T02:00:00Z".to_string()),
            completed_at: None,
        },
    ]
}

fn sample_rules() -> Vec<RuleInfo> {
    vec![
        RuleInfo {
            id: RuleId::new(
                TaggingRuleId::parse("22222222-2222-2222-2222-222222222222").expect("rule id"),
            ),
            pattern: "reports/**/*.csv".to_string(),
            tag: "report".to_string(),
            priority: 10,
            enabled: true,
        },
        RuleInfo {
            id: RuleId::new(
                TaggingRuleId::parse("33333333-3333-3333-3333-333333333333").expect("rule id"),
            ),
            pattern: "trades/**/*.parquet".to_string(),
            tag: "trade".to_string(),
            priority: 5,
            enabled: true,
        },
        RuleInfo {
            id: RuleId::new(
                TaggingRuleId::parse("44444444-4444-4444-4444-444444444444").expect("rule id"),
            ),
            pattern: "archive/**/*.zip".to_string(),
            tag: "archive".to_string(),
            priority: 1,
            enabled: false,
        },
    ]
}

fn sample_rule_builder_empty() -> RuleBuilderState {
    let mut builder = RuleBuilderState::new(None);
    builder.pattern = "".to_string();
    builder.tag = "".to_string();
    builder.focus = RuleBuilderFocus::Pattern;
    builder.file_results = FileResultsState::Exploration {
        folder_matches: Vec::new(),
        expanded_folder_indices: HashSet::new(),
        detected_patterns: Vec::new(),
    };
    builder
}

fn sample_rule_builder_basic() -> RuleBuilderState {
    let mut builder = RuleBuilderState::new(Some(SourceId::new()));
    builder.pattern = "**/reports/<date>_<region>.csv".to_string();
    builder.tag = "report.financial".to_string();
    builder.excludes = vec!["**/archive/**".to_string(), "**/*.tmp".to_string()];
    builder.extractions = vec![
        RuleBuilderField {
            name: "date".to_string(),
            source: FieldSource::Filename,
            field_type: FieldType::Date,
            pattern: Some(r"(\\d{4}-\\d{2}-\\d{2})".to_string()),
            sample_values: vec!["2024-09-30".to_string(), "2024-10-01".to_string()],
            enabled: true,
        },
        RuleBuilderField {
            name: "region".to_string(),
            source: FieldSource::Segment(-1),
            field_type: FieldType::String,
            pattern: None,
            sample_values: vec!["us-east".to_string(), "eu-west".to_string()],
            enabled: true,
        },
    ];
    builder.match_count = 128;
    builder.focus = RuleBuilderFocus::Pattern;
    builder.file_results = FileResultsState::Exploration {
        folder_matches: vec![
            FolderMatch {
                path: "reports/2024/Q3/".to_string(),
                count: 42,
                sample_filename: "report_2024-09-30_us.csv".to_string(),
                files: vec!["report_2024-09-30_us.csv".to_string()],
            },
            FolderMatch {
                path: "reports/2024/Q4/".to_string(),
                count: 68,
                sample_filename: "report_2024-10-01_eu.csv".to_string(),
                files: vec!["report_2024-10-01_eu.csv".to_string()],
            },
        ],
        expanded_folder_indices: HashSet::from([0usize]),
        detected_patterns: vec!["report_<date>_<region>.csv".to_string()],
    };
    builder
}

fn sample_rule_builder_with_suggestions() -> RuleBuilderState {
    let mut builder = sample_rule_builder_basic();
    builder.focus = RuleBuilderFocus::Suggestions;
    builder.suggestions_section = SuggestionSection::Structures;
    builder.pattern_seeds = vec![
        PatternSeed {
            pattern: "**/*.csv".to_string(),
            match_count: 612,
            is_extension: true,
        },
        PatternSeed {
            pattern: "**/reports/**/*.csv".to_string(),
            match_count: 128,
            is_extension: false,
        },
    ];
    builder.path_archetypes = vec![
        PathArchetype {
            template: "reports/<year>/<quarter>/".to_string(),
            file_count: 412,
            folder_count: 12,
            sample_paths: vec!["reports/2024/Q4/report_2024-10-01_us.csv".to_string()],
            depth: 3,
        },
        PathArchetype {
            template: "exports/<region>/<date>/".to_string(),
            file_count: 200,
            folder_count: 8,
            sample_paths: vec!["exports/us-east/2024-09-30/summary.csv".to_string()],
            depth: 3,
        },
    ];
    builder.naming_schemes = vec![
        NamingScheme {
            template: "report_<date>_<region>.csv".to_string(),
            file_count: 128,
            example: "report_2024-09-30_us.csv".to_string(),
            fields: vec!["date".to_string(), "region".to_string()],
        },
        NamingScheme {
            template: "summary_<date>.csv".to_string(),
            file_count: 84,
            example: "summary_2024-10-01.csv".to_string(),
            fields: vec!["date".to_string()],
        },
    ];
    builder.synonym_suggestions = vec![
        SynonymSuggestion {
            short_form: "rpt".to_string(),
            canonical_form: "report".to_string(),
            confidence: SynonymConfidence::High,
            reason: "Abbreviation match".to_string(),
            score: 92,
            applied: false,
        },
        SynonymSuggestion {
            short_form: "acct".to_string(),
            canonical_form: "account".to_string(),
            confidence: SynonymConfidence::Medium,
            reason: "Edit distance".to_string(),
            score: 71,
            applied: false,
        },
    ];
    builder
}

fn sample_rule_builder_backtest() -> RuleBuilderState {
    let mut builder = sample_rule_builder_basic();
    builder.focus = RuleBuilderFocus::FileList;

    let matched_files = vec![
        matched_file(
            "reports/2024/Q4/report_2024-10-01_us.csv",
            "report_2024-10-01_us.csv",
            FileTestResult::Pass,
        ),
        matched_file(
            "reports/2024/Q4/report_2024-10-02_eu.csv",
            "report_2024-10-02_eu.csv",
            FileTestResult::Fail {
                error: "region missing".to_string(),
                hint: Some("Check naming scheme".to_string()),
            },
        ),
        matched_file(
            "reports/2024/Q4/report_2024-10-03_apac.csv",
            "report_2024-10-03_apac.csv",
            FileTestResult::Fail {
                error: "date format".to_string(),
                hint: Some("Expected YYYY-MM-DD".to_string()),
            },
        ),
        matched_file(
            "reports/2024/Q4/report_2024-10-04_us.csv",
            "report_2024-10-04_us.csv",
            FileTestResult::Excluded {
                pattern: "**/archive/**".to_string(),
            },
        ),
    ];

    builder.file_results = FileResultsState::BacktestResults {
        matched_files,
        visible_indices: Vec::new(),
        backtest: BacktestSummary {
            total_matched: 4,
            pass_count: 1,
            fail_count: 2,
            excluded_count: 1,
            is_running: false,
        },
        result_filter: ResultFilter::FailOnly,
    };

    builder.selected_preview_files = HashSet::from([
        "report_2024-10-02_eu.csv".to_string(),
        "report_2024-10-03_apac.csv".to_string(),
    ]);
    builder.update_visible();
    builder
}

fn matched_file(path: &str, rel: &str, result: FileTestResult) -> MatchedFile {
    MatchedFile {
        path: path.to_string(),
        relative_path: rel.to_string(),
        extractions: HashMap::from([
            ("date".to_string(), "2024-10-01".to_string()),
            ("region".to_string(), "us".to_string()),
        ]),
        test_result: result,
    }
}

fn sample_query_results() -> QueryResults {
    let columns = vec![
        "id".to_string(),
        "name".to_string(),
        "status".to_string(),
        "created_at".to_string(),
    ];
    let rows = vec![
        vec![
            "9105".to_string(),
            "schema_seeds".to_string(),
            "Completed".to_string(),
            "2024-10-01 09:42".to_string(),
        ],
        vec![
            "9104".to_string(),
            "broken_parser".to_string(),
            "Failed".to_string(),
            "2024-10-01 09:30".to_string(),
        ],
        vec![
            "9103".to_string(),
            "report_rules".to_string(),
            "PartialSuccess".to_string(),
            "2024-10-01 09:20".to_string(),
        ],
        vec![
            "9102".to_string(),
            "trades_parser".to_string(),
            "Pending".to_string(),
            "2024-10-01 09:10".to_string(),
        ],
        vec![
            "9101".to_string(),
            "alpha-lake".to_string(),
            "Running".to_string(),
            "2024-10-01 09:05".to_string(),
        ],
    ];

    QueryResults {
        columns,
        rows,
        row_count: 42,
        truncated: true,
        selected_row: 2,
        scroll_x: 0,
    }
}

fn sample_query_tables() -> Vec<String> {
    vec![
        "scout_files".to_string(),
        "scout_jobs".to_string(),
        "scout_file_tags".to_string(),
        "scout_rules".to_string(),
    ]
}

fn sample_saved_queries() -> Vec<SavedQueryEntry> {
    vec![
        SavedQueryEntry {
            name: "failed_jobs".to_string(),
            path: PathBuf::from("/Users/demo/.casparian_flow/queries/failed_jobs.sql"),
        },
        SavedQueryEntry {
            name: "tag_counts".to_string(),
            path: PathBuf::from("/Users/demo/.casparian_flow/queries/tag_counts.sql"),
        },
        SavedQueryEntry {
            name: "recent_runs".to_string(),
            path: PathBuf::from("/Users/demo/.casparian_flow/queries/recent_runs.sql"),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_cases_cover_all_required() {
        let mut names = HashSet::new();
        let mut coverage = HashSet::new();

        for case in snapshot_cases() {
            assert!(
                names.insert(case.name),
                "duplicate snapshot name: {}",
                case.name
            );
            assert!(
                coverage.insert(case.coverage),
                "duplicate snapshot coverage: {}",
                case.coverage.as_str()
            );
            assert_eq!(
                case.name,
                case.coverage.as_str(),
                "snapshot name mismatch for coverage {}",
                case.coverage.as_str()
            );
        }

        let missing: Vec<_> = SnapshotCoverage::ALL
            .iter()
            .filter(|cov| !coverage.contains(cov))
            .collect();
        assert!(
            missing.is_empty(),
            "missing snapshot coverage: {:?}",
            missing
        );
    }
}

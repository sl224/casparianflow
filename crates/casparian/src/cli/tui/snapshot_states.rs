//! Canonical snapshot states for TUI rendering tests and exports.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Local, TimeZone, Utc};

use casparian::scout::{SourceId, TaggingRuleId, Workspace, WorkspaceId};
use casparian_intent::IntentState;

use super::app::{
    ApprovalDisplayStatus, ApprovalInfo, ApprovalOperationType, ApprovalStatusFilter,
    ApprovalsViewState, App, BacktestInfo, BoundFileInfo, BoundFileStatus, CatalogTab,
    CommandPaletteMode, CommandPaletteState, DeadLetterRow, DiscoverFocus, DiscoverViewState,
    HomeStats, JobInfo, JobStatus, JobSummary, JobType, JobsListSection, JobsViewState,
    ParserBenchState, ParserHealth, ParserInfo, PipelineInfo, PipelineRunInfo, PipelineStage,
    PipelineState, QuarantineRow, QueryResults, QueryState, QueryViewState, RuleDialogFocus,
    RuleId, RuleInfo, SavedQueriesState, SchemaMismatchRow, SessionInfo, SessionsViewState,
    SettingsCategory, SettingsState, SourceInfo, TableBrowserState, TagInfo, TriageTab, TuiMode,
    WorkspaceSwitcherMode,
};
use super::extraction::{
    BacktestSummary, FieldSource, FieldType, FileResultsState, FileTestResult, FolderMatch,
    MatchedFile, NamingScheme, PathArchetype, PatternSeed, ResultFilter, RuleBuilderField,
    RuleBuilderFocus, RuleBuilderState, SuggestionSection, SynonymConfidence, SynonymSuggestion,
};
use super::TuiArgs;

pub const DEFAULT_SNAPSHOT_SIZES: &[(u16, u16)] = &[(80, 24), (100, 30), (120, 40), (160, 50)];

const SNAPSHOT_DB_PATH: &str = "SNAPSHOT_DB.duckdb";

pub struct SnapshotCase {
    pub name: &'static str,
    pub notes: &'static str,
    pub focus_hint: &'static str,
    pub build: fn() -> App,
}

pub fn snapshot_cases() -> &'static [SnapshotCase] {
    &SNAPSHOT_CASES
}

const SNAPSHOT_CASES: [SnapshotCase; 20] = [
    SnapshotCase {
        name: "home_default",
        notes: "Home hub with seeded sources and recent jobs.",
        focus_hint: "Quick Start list",
        build: case_home_default,
    },
    SnapshotCase {
        name: "discover_empty_no_sources",
        notes: "Discover mode with no sources configured.",
        focus_hint: "Rule Builder header",
        build: case_discover_empty_no_sources,
    },
    SnapshotCase {
        name: "discover_scanning_progress",
        notes: "Discover mode with scanning overlay and progress counters.",
        focus_hint: "Scanning dialog",
        build: case_discover_scanning_progress,
    },
    SnapshotCase {
        name: "discover_files_list_with_filters_and_tags",
        notes: "Rule Builder showing backtest results and tag focus.",
        focus_hint: "Results list",
        build: case_discover_files_list_with_filters_and_tags,
    },
    SnapshotCase {
        name: "discover_rule_builder",
        notes: "Rule Builder with schema suggestions populated.",
        focus_hint: "Suggestions panel",
        build: case_discover_rule_builder,
    },
    SnapshotCase {
        name: "discover_rules_manager_dialog",
        notes: "Rules Manager overlay with multiple rules.",
        focus_hint: "Rules dialog",
        build: case_discover_rules_manager_dialog,
    },
    SnapshotCase {
        name: "jobs_list_mixed_status",
        notes: "Jobs view with mixed job statuses and pipeline summary.",
        focus_hint: "Actionable list",
        build: case_jobs_list_mixed_status,
    },
    SnapshotCase {
        name: "jobs_drawer_open",
        notes: "Global Jobs drawer overlay open.",
        focus_hint: "Jobs drawer",
        build: case_jobs_drawer_open,
    },
    SnapshotCase {
        name: "approvals_list_mixed",
        notes: "Approvals view with mixed statuses.",
        focus_hint: "Approvals list",
        build: case_approvals_list_mixed,
    },
    SnapshotCase {
        name: "sessions_list_pending_gate",
        notes: "Sessions view showing pending gate workflows.",
        focus_hint: "Sessions list",
        build: case_sessions_list_pending_gate,
    },
    SnapshotCase {
        name: "triage_quarantine_list",
        notes: "Quarantine triage list with raw data preview.",
        focus_hint: "Quarantine list",
        build: case_triage_quarantine_list,
    },
    SnapshotCase {
        name: "catalog_runs_list",
        notes: "Pipeline runs catalog view.",
        focus_hint: "Catalog list",
        build: case_catalog_runs_list,
    },
    SnapshotCase {
        name: "workspace_switcher_open",
        notes: "Workspace switcher overlay open.",
        focus_hint: "Workspace list",
        build: case_workspace_switcher_open,
    },
    SnapshotCase {
        name: "query_editor_focused",
        notes: "Query console with editor focused and history.",
        focus_hint: "SQL editor",
        build: case_query_editor_focused,
    },
    SnapshotCase {
        name: "query_results_table",
        notes: "Query console with results table focused.",
        focus_hint: "Results table",
        build: case_query_results_table,
    },
    SnapshotCase {
        name: "settings_about",
        notes: "Settings view on About section.",
        focus_hint: "About panel",
        build: case_settings_about,
    },
    SnapshotCase {
        name: "command_palette_open",
        notes: "Command palette overlay with suggestions.",
        focus_hint: "Command palette",
        build: case_command_palette_open,
    },
    SnapshotCase {
        name: "help_overlay_open",
        notes: "Help overlay open on Discover mode.",
        focus_hint: "Help overlay",
        build: case_help_overlay_open,
    },
    SnapshotCase {
        name: "sources_screen",
        notes: "Sources list with selection and inspector.",
        focus_hint: "Sources list",
        build: case_sources_screen,
    },
    SnapshotCase {
        name: "parser_bench_list",
        notes: "Parser Bench with parser list and details.",
        focus_hint: "Parser list",
        build: case_parser_bench_list,
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

fn case_discover_empty_no_sources() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Discover;
    app.discover.sources.clear();
    app.discover.tags.clear();
    app.discover.selected_source_id = None;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_empty());
    app
}

fn case_discover_scanning_progress() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Discover;
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
    app.mode = TuiMode::Discover;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.selected_tag = Some(2);
    app.discover.rule_builder = Some(sample_rule_builder_backtest());
    app
}

fn case_discover_rule_builder() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Discover;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_with_suggestions());
    app
}

fn case_discover_rules_manager_dialog() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Discover;
    app.discover.view_state = DiscoverViewState::RulesManager;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app.discover.rules = sample_rules();
    app.discover.selected_rule = 1;
    app
}

fn case_jobs_list_mixed_status() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Jobs;
    app.jobs_state.view_state = JobsViewState::JobList;
    app.jobs_state.section_focus = JobsListSection::Actionable;
    app.jobs_state.selected_index = 1;
    app.jobs_state.show_pipeline = true;
    app.jobs_state.pipeline = sample_pipeline();
    app
}

fn case_jobs_drawer_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Home;
    app.jobs_drawer_open = true;
    app.jobs_drawer_selected = 2;
    app
}

fn case_approvals_list_mixed() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Approvals;
    app.approvals_state.view_state = ApprovalsViewState::List;
    app.approvals_state.filter = ApprovalStatusFilter::All;
    app.approvals_state.approvals = sample_approvals();
    app.approvals_state.approvals_loaded = true;
    app.approvals_state.selected_index = 0;
    app
}

fn case_sessions_list_pending_gate() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Sessions;
    app.sessions_state.view_state = SessionsViewState::SessionList;
    app.sessions_state.sessions = sample_sessions();
    app.sessions_state.sessions_loaded = true;
    app.sessions_state.selected_index = 0;
    app
}

fn case_triage_quarantine_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Triage;
    app.triage_state.tab = TriageTab::Quarantine;
    app.triage_state.quarantine_rows = Some(sample_quarantine_rows());
    app.triage_state.schema_mismatches = Some(sample_schema_mismatches());
    app.triage_state.dead_letters = Some(sample_dead_letters());
    app.triage_state.selected_index = 1;
    app.triage_state.loaded = true;
    app
}

fn case_catalog_runs_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Catalog;
    app.catalog_state.tab = CatalogTab::Runs;
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

fn case_settings_about() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Settings;
    app.settings = SettingsState {
        category: SettingsCategory::About,
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
    };
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

fn case_help_overlay_open() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Discover;
    app.discover.view_state = DiscoverViewState::RuleBuilder;
    app.discover.rule_builder = Some(sample_rule_builder_basic());
    app.show_help = true;
    app
}

fn case_sources_screen() -> App {
    let mut app = base_app();
    app.mode = TuiMode::Sources;
    app.sources_state.selected_index = 1;
    app
}

fn case_parser_bench_list() -> App {
    let mut app = base_app();
    app.mode = TuiMode::ParserBench;
    app.parser_bench = ParserBenchState {
        parsers: sample_parsers(),
        selected_parser: 1,
        parsers_loaded: true,
        bound_files: sample_bound_files(),
        ..ParserBenchState::default()
    };
    app
}

fn base_app() -> App {
    let args = TuiArgs {
        database: Some(PathBuf::from(SNAPSHOT_DB_PATH)),
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

fn sample_parsers() -> Vec<ParserInfo> {
    vec![
        ParserInfo {
            path: PathBuf::from("/Users/demo/.casparian_flow/parsers/trades_parser.py"),
            name: "trades_parser".to_string(),
            version: Some("1.4.2".to_string()),
            topics: vec!["trade".to_string(), "audit".to_string()],
            modified: local_at(-180),
            health: ParserHealth::Healthy {
                success_rate: 0.98,
                total_runs: 128,
            },
            is_symlink: false,
            symlink_broken: false,
        },
        ParserInfo {
            path: PathBuf::from("/Users/demo/.casparian_flow/parsers/report_parser.py"),
            name: "report_parser".to_string(),
            version: Some("0.9.1".to_string()),
            topics: vec!["report".to_string()],
            modified: local_at(-240),
            health: ParserHealth::Warning {
                consecutive_failures: 3,
            },
            is_symlink: false,
            symlink_broken: false,
        },
        ParserInfo {
            path: PathBuf::from("/Users/demo/.casparian_flow/parsers/legacy_parser.py"),
            name: "legacy_parser".to_string(),
            version: None,
            topics: vec!["legacy".to_string()],
            modified: local_at(-360),
            health: ParserHealth::Paused {
                reason: "Circuit breaker".to_string(),
            },
            is_symlink: true,
            symlink_broken: true,
        },
    ]
}

fn sample_bound_files() -> Vec<BoundFileInfo> {
    vec![
        BoundFileInfo {
            path: PathBuf::from("/data/alpha/trades/2024/09/report_2024-09-30.csv"),
            size: 2_400_000,
            status: BoundFileStatus::Processed,
        },
        BoundFileInfo {
            path: PathBuf::from("/data/alpha/trades/2024/10/report_2024-10-01.csv"),
            size: 2_100_000,
            status: BoundFileStatus::Pending,
        },
        BoundFileInfo {
            path: PathBuf::from("/data/alpha/trades/2024/10/report_2024-10-02.csv"),
            size: 2_200_000,
            status: BoundFileStatus::Failed,
        },
    ]
}

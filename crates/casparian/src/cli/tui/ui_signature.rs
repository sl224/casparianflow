//! UI topology signature for deterministic state exploration.

use serde::Serialize;

use super::app::{
    App, ApprovalStatusFilter, ApprovalsViewState, CatalogTab, CommandPaletteMode, DiscoverFocus,
    DiscoverViewState, JobsListSection, JobsViewState, ParserBenchView, QueryViewState,
    SessionsViewState, SettingsCategory, ShellFocus, SourcesState, TriageTab, TuiMode,
    WorkspaceSwitcherMode,
};
use super::extraction::{FileResultsState, RuleBuilderFocus};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct UiSignature {
    pub mode: UiMode,
    pub shell_focus: UiShellFocus,
    pub nav_selected: Option<usize>,
    pub overlays: UiOverlays,
    pub home: Option<HomeSignature>,
    pub discover: Option<DiscoverSignature>,
    pub jobs: Option<JobsSignature>,
    pub sources: Option<SourcesSignature>,
    pub approvals: Option<ApprovalsSignature>,
    pub parser_bench: Option<ParserBenchSignature>,
    pub query: Option<QuerySignature>,
    pub settings: Option<SettingsSignature>,
    pub sessions: Option<SessionsSignature>,
    pub triage: Option<TriageSignature>,
    pub catalog: Option<CatalogSignature>,
}

impl UiSignature {
    pub fn from_app(app: &App) -> Self {
        let mode = UiMode::from(app.mode);
        let shell_focus = UiShellFocus::from(app.shell_focus);
        let nav_selected = match app.shell_focus {
            ShellFocus::Rail => Some(app.nav_selected),
            ShellFocus::Main => None,
        };

        let overlays = UiOverlays {
            show_help: app.show_help,
            inspector_collapsed: app.inspector_collapsed,
            command_palette_visible: app.command_palette.visible,
            command_palette_mode: if app.command_palette.visible {
                Some(UiCommandPaletteMode::from(app.command_palette.mode))
            } else {
                None
            },
            workspace_switcher_visible: app.workspace_switcher.visible,
            workspace_switcher_mode: if app.workspace_switcher.visible {
                Some(UiWorkspaceSwitcherMode::from(app.workspace_switcher.mode))
            } else {
                None
            },
            jobs_drawer_open: app.jobs_drawer_open,
            sources_drawer_open: app.sources_drawer_open,
        };

        let home = match app.mode {
            TuiMode::Home => Some(HomeSignature {
                filtering: app.home.filtering,
            }),
            _ => None,
        };

        let discover = match app.mode {
            TuiMode::Discover => {
                let rule_builder = if app.discover.view_state == DiscoverViewState::RuleBuilder {
                    app.discover
                        .rule_builder
                        .as_ref()
                        .map(|builder| RuleBuilderSignature {
                            focus: RuleBuilderFocusKey::from(builder.focus.clone()),
                            file_results: FileResultsKind::from(&builder.file_results),
                            suggestions_help_open: builder.suggestions_help_open,
                            suggestions_detail_open: builder.suggestions_detail_open,
                            manual_tag_confirm_open: builder.manual_tag_confirm_open,
                            confirm_exit_open: builder.confirm_exit_open,
                        })
                } else {
                    None
                };

                Some(DiscoverSignature {
                    view_state: DiscoverViewStateKey::from(app.discover.view_state),
                    focus: DiscoverFocusKey::from(app.discover.focus),
                    preview_open: app.discover.preview_open,
                    rule_builder,
                })
            }
            _ => None,
        };

        let jobs = match app.mode {
            TuiMode::Jobs => Some(JobsSignature {
                view_state: JobsViewStateKey::from(app.jobs_state.view_state),
                section_focus: JobsListSectionKey::from(app.jobs_state.section_focus),
                show_pipeline: app.jobs_state.show_pipeline,
            }),
            _ => None,
        };

        let sources = match app.mode {
            TuiMode::Sources => Some(SourcesSignature::from_state(&app.sources_state)),
            _ => None,
        };

        let approvals = match app.mode {
            TuiMode::Approvals => Some(ApprovalsSignature {
                view_state: ApprovalsViewStateKey::from(app.approvals_state.view_state),
                filter: ApprovalStatusFilterKey::from(app.approvals_state.filter),
            }),
            _ => None,
        };

        let parser_bench = match app.mode {
            TuiMode::ParserBench => Some(ParserBenchSignature {
                view: ParserBenchViewKey::from(app.parser_bench.view),
                focus_mode: app.parser_bench.focus_mode,
                filtering: app.parser_bench.is_filtering,
                test_running: app.parser_bench.test_running,
                has_test_result: app.parser_bench.test_result.is_some(),
            }),
            _ => None,
        };

        let query = match app.mode {
            TuiMode::Query => Some(QuerySignature {
                view_state: QueryViewStateKey::from(app.query_state.view_state),
            }),
            _ => None,
        };

        let settings = match app.mode {
            TuiMode::Settings => Some(SettingsSignature {
                category: SettingsCategoryKey::from(app.settings.category),
                editing: app.settings.editing,
            }),
            _ => None,
        };

        let sessions = match app.mode {
            TuiMode::Sessions => Some(SessionsSignature {
                view_state: SessionsViewStateKey::from(app.sessions_state.view_state),
            }),
            _ => None,
        };

        let triage = match app.mode {
            TuiMode::Triage => Some(TriageSignature {
                tab: TriageTabKey::from(app.triage_state.tab),
            }),
            _ => None,
        };

        let catalog = match app.mode {
            TuiMode::Catalog => Some(CatalogSignature {
                tab: CatalogTabKey::from(app.catalog_state.tab),
            }),
            _ => None,
        };

        Self {
            mode,
            shell_focus,
            nav_selected,
            overlays,
            home,
            discover,
            jobs,
            sources,
            approvals,
            parser_bench,
            query,
            settings,
            sessions,
            triage,
            catalog,
        }
    }

    pub fn key(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!("mode={}", self.mode.as_str()));
        parts.push(format!("shell_focus={}", self.shell_focus.as_str()));
        match self.nav_selected {
            Some(idx) => parts.push(format!("nav={}", idx)),
            None => parts.push("nav=none".to_string()),
        }
        parts.push(format!("help={}", bool_key(self.overlays.show_help)));
        parts.push(format!(
            "inspector={}",
            bool_key(self.overlays.inspector_collapsed)
        ));
        parts.push(format!("palette={}", self.overlays.command_palette_label()));
        parts.push(format!(
            "workspace_switcher={}",
            self.overlays.workspace_switcher_label()
        ));
        parts.push(format!(
            "jobs_drawer={}",
            bool_key(self.overlays.jobs_drawer_open)
        ));
        parts.push(format!(
            "sources_drawer={}",
            bool_key(self.overlays.sources_drawer_open)
        ));

        if let Some(ref home) = self.home {
            parts.push(format!("home.filtering={}", bool_key(home.filtering)));
        }

        if let Some(ref discover) = self.discover {
            parts.push(format!("discover.view={}", discover.view_state.as_str()));
            parts.push(format!("discover.focus={}", discover.focus.as_str()));
            parts.push(format!(
                "discover.preview={}",
                bool_key(discover.preview_open)
            ));
            if let Some(ref builder) = discover.rule_builder {
                parts.push(format!("rule_builder.focus={}", builder.focus.as_str()));
                parts.push(format!(
                    "rule_builder.results={}",
                    builder.file_results.as_str()
                ));
                parts.push(format!(
                    "rule_builder.help={}",
                    bool_key(builder.suggestions_help_open)
                ));
                parts.push(format!(
                    "rule_builder.detail={}",
                    bool_key(builder.suggestions_detail_open)
                ));
                parts.push(format!(
                    "rule_builder.manual_confirm={}",
                    bool_key(builder.manual_tag_confirm_open)
                ));
                parts.push(format!(
                    "rule_builder.confirm_exit={}",
                    bool_key(builder.confirm_exit_open)
                ));
            }
        }

        if let Some(ref jobs) = self.jobs {
            parts.push(format!("jobs.view={}", jobs.view_state.as_str()));
            parts.push(format!("jobs.section={}", jobs.section_focus.as_str()));
            parts.push(format!("jobs.pipeline={}", bool_key(jobs.show_pipeline)));
        }

        if let Some(ref sources) = self.sources {
            parts.push(format!("sources.editing={}", bool_key(sources.editing)));
            parts.push(format!("sources.creating={}", bool_key(sources.creating)));
            parts.push(format!(
                "sources.confirm_delete={}",
                bool_key(sources.confirm_delete)
            ));
        }

        if let Some(ref approvals) = self.approvals {
            parts.push(format!("approvals.view={}", approvals.view_state.as_str()));
            parts.push(format!("approvals.filter={}", approvals.filter.as_str()));
        }

        if let Some(ref bench) = self.parser_bench {
            parts.push(format!("parser_bench.view={}", bench.view.as_str()));
            parts.push(format!(
                "parser_bench.focus_mode={}",
                bool_key(bench.focus_mode)
            ));
            parts.push(format!(
                "parser_bench.filtering={}",
                bool_key(bench.filtering)
            ));
            parts.push(format!(
                "parser_bench.test_running={}",
                bool_key(bench.test_running)
            ));
            parts.push(format!(
                "parser_bench.has_result={}",
                bool_key(bench.has_test_result)
            ));
        }

        if let Some(ref query) = self.query {
            parts.push(format!("query.view={}", query.view_state.as_str()));
        }

        if let Some(ref settings) = self.settings {
            parts.push(format!("settings.category={}", settings.category.as_str()));
            parts.push(format!("settings.editing={}", bool_key(settings.editing)));
        }

        if let Some(ref sessions) = self.sessions {
            parts.push(format!("sessions.view={}", sessions.view_state.as_str()));
        }

        if let Some(ref triage) = self.triage {
            parts.push(format!("triage.tab={}", triage.tab.as_str()));
        }

        if let Some(ref catalog) = self.catalog {
            parts.push(format!("catalog.tab={}", catalog.tab.as_str()));
        }

        parts.join("|")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct UiOverlays {
    pub show_help: bool,
    pub inspector_collapsed: bool,
    pub command_palette_visible: bool,
    pub command_palette_mode: Option<UiCommandPaletteMode>,
    pub workspace_switcher_visible: bool,
    pub workspace_switcher_mode: Option<UiWorkspaceSwitcherMode>,
    pub jobs_drawer_open: bool,
    pub sources_drawer_open: bool,
}

impl UiOverlays {
    fn command_palette_label(&self) -> String {
        if !self.command_palette_visible {
            return "off".to_string();
        }
        match self.command_palette_mode {
            Some(mode) => format!("on:{}", mode.as_str()),
            None => "on:unknown".to_string(),
        }
    }

    fn workspace_switcher_label(&self) -> String {
        if !self.workspace_switcher_visible {
            return "off".to_string();
        }
        match self.workspace_switcher_mode {
            Some(mode) => format!("on:{}", mode.as_str()),
            None => "on:unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct HomeSignature {
    pub filtering: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DiscoverSignature {
    pub view_state: DiscoverViewStateKey,
    pub focus: DiscoverFocusKey,
    pub preview_open: bool,
    pub rule_builder: Option<RuleBuilderSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RuleBuilderSignature {
    pub focus: RuleBuilderFocusKey,
    pub file_results: FileResultsKind,
    pub suggestions_help_open: bool,
    pub suggestions_detail_open: bool,
    pub manual_tag_confirm_open: bool,
    pub confirm_exit_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct JobsSignature {
    pub view_state: JobsViewStateKey,
    pub section_focus: JobsListSectionKey,
    pub show_pipeline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SourcesSignature {
    pub editing: bool,
    pub creating: bool,
    pub confirm_delete: bool,
}

impl SourcesSignature {
    fn from_state(state: &SourcesState) -> Self {
        Self {
            editing: state.editing,
            creating: state.creating,
            confirm_delete: state.confirm_delete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ApprovalsSignature {
    pub view_state: ApprovalsViewStateKey,
    pub filter: ApprovalStatusFilterKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ParserBenchSignature {
    pub view: ParserBenchViewKey,
    pub focus_mode: bool,
    pub filtering: bool,
    pub test_running: bool,
    pub has_test_result: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct QuerySignature {
    pub view_state: QueryViewStateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SettingsSignature {
    pub category: SettingsCategoryKey,
    pub editing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SessionsSignature {
    pub view_state: SessionsViewStateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct TriageSignature {
    pub tab: TriageTabKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct CatalogSignature {
    pub tab: CatalogTabKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    Home,
    Discover,
    Jobs,
    Sources,
    Approvals,
    ParserBench,
    Query,
    Settings,
    Sessions,
    Triage,
    Catalog,
}

impl UiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UiMode::Home => "home",
            UiMode::Discover => "discover",
            UiMode::Jobs => "jobs",
            UiMode::Sources => "sources",
            UiMode::Approvals => "approvals",
            UiMode::ParserBench => "parser_bench",
            UiMode::Query => "query",
            UiMode::Settings => "settings",
            UiMode::Sessions => "sessions",
            UiMode::Triage => "triage",
            UiMode::Catalog => "catalog",
        }
    }
}

impl From<TuiMode> for UiMode {
    fn from(value: TuiMode) -> Self {
        match value {
            TuiMode::Home => UiMode::Home,
            TuiMode::Discover => UiMode::Discover,
            TuiMode::Jobs => UiMode::Jobs,
            TuiMode::Sources => UiMode::Sources,
            TuiMode::Approvals => UiMode::Approvals,
            TuiMode::ParserBench => UiMode::ParserBench,
            TuiMode::Query => UiMode::Query,
            TuiMode::Settings => UiMode::Settings,
            TuiMode::Sessions => UiMode::Sessions,
            TuiMode::Triage => UiMode::Triage,
            TuiMode::Catalog => UiMode::Catalog,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiShellFocus {
    Main,
    Rail,
}

impl UiShellFocus {
    pub fn as_str(self) -> &'static str {
        match self {
            UiShellFocus::Main => "main",
            UiShellFocus::Rail => "rail",
        }
    }
}

impl From<ShellFocus> for UiShellFocus {
    fn from(value: ShellFocus) -> Self {
        match value {
            ShellFocus::Main => UiShellFocus::Main,
            ShellFocus::Rail => UiShellFocus::Rail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiCommandPaletteMode {
    Intent,
    Command,
    Navigation,
}

impl UiCommandPaletteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UiCommandPaletteMode::Intent => "intent",
            UiCommandPaletteMode::Command => "command",
            UiCommandPaletteMode::Navigation => "navigation",
        }
    }
}

impl From<CommandPaletteMode> for UiCommandPaletteMode {
    fn from(value: CommandPaletteMode) -> Self {
        match value {
            CommandPaletteMode::Intent => UiCommandPaletteMode::Intent,
            CommandPaletteMode::Command => UiCommandPaletteMode::Command,
            CommandPaletteMode::Navigation => UiCommandPaletteMode::Navigation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiWorkspaceSwitcherMode {
    List,
    Creating,
}

impl UiWorkspaceSwitcherMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UiWorkspaceSwitcherMode::List => "list",
            UiWorkspaceSwitcherMode::Creating => "creating",
        }
    }
}

impl From<WorkspaceSwitcherMode> for UiWorkspaceSwitcherMode {
    fn from(value: WorkspaceSwitcherMode) -> Self {
        match value {
            WorkspaceSwitcherMode::List => UiWorkspaceSwitcherMode::List,
            WorkspaceSwitcherMode::Creating => UiWorkspaceSwitcherMode::Creating,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverViewStateKey {
    Files,
    Filtering,
    EnteringPath,
    ScanConfirm,
    Tagging,
    CreatingSource,
    BulkTagging,
    SourcesDropdown,
    TagsDropdown,
    RulesManager,
    RuleCreation,
    RuleBuilder,
    SourcesManager,
    SourceEdit,
    SourceDeleteConfirm,
    Scanning,
}

impl DiscoverViewStateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            DiscoverViewStateKey::Files => "files",
            DiscoverViewStateKey::Filtering => "filtering",
            DiscoverViewStateKey::EnteringPath => "entering_path",
            DiscoverViewStateKey::ScanConfirm => "scan_confirm",
            DiscoverViewStateKey::Tagging => "tagging",
            DiscoverViewStateKey::CreatingSource => "creating_source",
            DiscoverViewStateKey::BulkTagging => "bulk_tagging",
            DiscoverViewStateKey::SourcesDropdown => "sources_dropdown",
            DiscoverViewStateKey::TagsDropdown => "tags_dropdown",
            DiscoverViewStateKey::RulesManager => "rules_manager",
            DiscoverViewStateKey::RuleCreation => "rule_creation",
            DiscoverViewStateKey::RuleBuilder => "rule_builder",
            DiscoverViewStateKey::SourcesManager => "sources_manager",
            DiscoverViewStateKey::SourceEdit => "source_edit",
            DiscoverViewStateKey::SourceDeleteConfirm => "source_delete_confirm",
            DiscoverViewStateKey::Scanning => "scanning",
        }
    }
}

impl From<DiscoverViewState> for DiscoverViewStateKey {
    fn from(value: DiscoverViewState) -> Self {
        match value {
            DiscoverViewState::Files => DiscoverViewStateKey::Files,
            DiscoverViewState::Filtering => DiscoverViewStateKey::Filtering,
            DiscoverViewState::EnteringPath => DiscoverViewStateKey::EnteringPath,
            DiscoverViewState::ScanConfirm => DiscoverViewStateKey::ScanConfirm,
            DiscoverViewState::Tagging => DiscoverViewStateKey::Tagging,
            DiscoverViewState::CreatingSource => DiscoverViewStateKey::CreatingSource,
            DiscoverViewState::BulkTagging => DiscoverViewStateKey::BulkTagging,
            DiscoverViewState::SourcesDropdown => DiscoverViewStateKey::SourcesDropdown,
            DiscoverViewState::TagsDropdown => DiscoverViewStateKey::TagsDropdown,
            DiscoverViewState::RulesManager => DiscoverViewStateKey::RulesManager,
            DiscoverViewState::RuleCreation => DiscoverViewStateKey::RuleCreation,
            DiscoverViewState::RuleBuilder => DiscoverViewStateKey::RuleBuilder,
            DiscoverViewState::SourcesManager => DiscoverViewStateKey::SourcesManager,
            DiscoverViewState::SourceEdit => DiscoverViewStateKey::SourceEdit,
            DiscoverViewState::SourceDeleteConfirm => DiscoverViewStateKey::SourceDeleteConfirm,
            DiscoverViewState::Scanning => DiscoverViewStateKey::Scanning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverFocusKey {
    Files,
    Sources,
    Tags,
}

impl DiscoverFocusKey {
    pub fn as_str(self) -> &'static str {
        match self {
            DiscoverFocusKey::Files => "files",
            DiscoverFocusKey::Sources => "sources",
            DiscoverFocusKey::Tags => "tags",
        }
    }
}

impl From<DiscoverFocus> for DiscoverFocusKey {
    fn from(value: DiscoverFocus) -> Self {
        match value {
            DiscoverFocus::Files => DiscoverFocusKey::Files,
            DiscoverFocus::Sources => DiscoverFocusKey::Sources,
            DiscoverFocus::Tags => DiscoverFocusKey::Tags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleBuilderFocusKey {
    Pattern,
    Excludes,
    ExcludeInput,
    Tag,
    Extractions,
    ExtractionEdit,
    Options,
    Suggestions,
    FileList,
    IgnorePicker,
}

impl RuleBuilderFocusKey {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleBuilderFocusKey::Pattern => "pattern",
            RuleBuilderFocusKey::Excludes => "excludes",
            RuleBuilderFocusKey::ExcludeInput => "exclude_input",
            RuleBuilderFocusKey::Tag => "tag",
            RuleBuilderFocusKey::Extractions => "extractions",
            RuleBuilderFocusKey::ExtractionEdit => "extraction_edit",
            RuleBuilderFocusKey::Options => "options",
            RuleBuilderFocusKey::Suggestions => "suggestions",
            RuleBuilderFocusKey::FileList => "file_list",
            RuleBuilderFocusKey::IgnorePicker => "ignore_picker",
        }
    }
}

impl From<RuleBuilderFocus> for RuleBuilderFocusKey {
    fn from(value: RuleBuilderFocus) -> Self {
        match value {
            RuleBuilderFocus::Pattern => RuleBuilderFocusKey::Pattern,
            RuleBuilderFocus::Excludes => RuleBuilderFocusKey::Excludes,
            RuleBuilderFocus::ExcludeInput => RuleBuilderFocusKey::ExcludeInput,
            RuleBuilderFocus::Tag => RuleBuilderFocusKey::Tag,
            RuleBuilderFocus::Extractions => RuleBuilderFocusKey::Extractions,
            RuleBuilderFocus::ExtractionEdit(_) => RuleBuilderFocusKey::ExtractionEdit,
            RuleBuilderFocus::Options => RuleBuilderFocusKey::Options,
            RuleBuilderFocus::Suggestions => RuleBuilderFocusKey::Suggestions,
            RuleBuilderFocus::FileList => RuleBuilderFocusKey::FileList,
            RuleBuilderFocus::IgnorePicker => RuleBuilderFocusKey::IgnorePicker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileResultsKind {
    Exploration,
    ExtractionPreview,
    BacktestResults,
}

impl FileResultsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileResultsKind::Exploration => "exploration",
            FileResultsKind::ExtractionPreview => "extraction_preview",
            FileResultsKind::BacktestResults => "backtest_results",
        }
    }
}

impl From<&FileResultsState> for FileResultsKind {
    fn from(value: &FileResultsState) -> Self {
        match value {
            FileResultsState::Exploration { .. } => FileResultsKind::Exploration,
            FileResultsState::ExtractionPreview { .. } => FileResultsKind::ExtractionPreview,
            FileResultsState::BacktestResults { .. } => FileResultsKind::BacktestResults,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobsViewStateKey {
    JobList,
    DetailPanel,
    LogViewer,
    FilterDialog,
    MonitoringPanel,
    ViolationDetail,
}

impl JobsViewStateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            JobsViewStateKey::JobList => "job_list",
            JobsViewStateKey::DetailPanel => "detail_panel",
            JobsViewStateKey::LogViewer => "log_viewer",
            JobsViewStateKey::FilterDialog => "filter_dialog",
            JobsViewStateKey::MonitoringPanel => "monitoring_panel",
            JobsViewStateKey::ViolationDetail => "violation_detail",
        }
    }
}

impl From<JobsViewState> for JobsViewStateKey {
    fn from(value: JobsViewState) -> Self {
        match value {
            JobsViewState::JobList => JobsViewStateKey::JobList,
            JobsViewState::DetailPanel => JobsViewStateKey::DetailPanel,
            JobsViewState::LogViewer => JobsViewStateKey::LogViewer,
            JobsViewState::FilterDialog => JobsViewStateKey::FilterDialog,
            JobsViewState::MonitoringPanel => JobsViewStateKey::MonitoringPanel,
            JobsViewState::ViolationDetail => JobsViewStateKey::ViolationDetail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobsListSectionKey {
    Actionable,
    Ready,
}

impl JobsListSectionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            JobsListSectionKey::Actionable => "actionable",
            JobsListSectionKey::Ready => "ready",
        }
    }
}

impl From<JobsListSection> for JobsListSectionKey {
    fn from(value: JobsListSection) -> Self {
        match value {
            JobsListSection::Actionable => JobsListSectionKey::Actionable,
            JobsListSection::Ready => JobsListSectionKey::Ready,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalsViewStateKey {
    List,
    Detail,
    ConfirmApprove,
    ConfirmReject,
}

impl ApprovalsViewStateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalsViewStateKey::List => "list",
            ApprovalsViewStateKey::Detail => "detail",
            ApprovalsViewStateKey::ConfirmApprove => "confirm_approve",
            ApprovalsViewStateKey::ConfirmReject => "confirm_reject",
        }
    }
}

impl From<ApprovalsViewState> for ApprovalsViewStateKey {
    fn from(value: ApprovalsViewState) -> Self {
        match value {
            ApprovalsViewState::List => ApprovalsViewStateKey::List,
            ApprovalsViewState::Detail => ApprovalsViewStateKey::Detail,
            ApprovalsViewState::ConfirmApprove => ApprovalsViewStateKey::ConfirmApprove,
            ApprovalsViewState::ConfirmReject => ApprovalsViewStateKey::ConfirmReject,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatusFilterKey {
    Pending,
    Approved,
    Rejected,
    Expired,
    All,
}

impl ApprovalStatusFilterKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalStatusFilterKey::Pending => "pending",
            ApprovalStatusFilterKey::Approved => "approved",
            ApprovalStatusFilterKey::Rejected => "rejected",
            ApprovalStatusFilterKey::Expired => "expired",
            ApprovalStatusFilterKey::All => "all",
        }
    }
}

impl From<ApprovalStatusFilter> for ApprovalStatusFilterKey {
    fn from(value: ApprovalStatusFilter) -> Self {
        match value {
            ApprovalStatusFilter::Pending => ApprovalStatusFilterKey::Pending,
            ApprovalStatusFilter::Approved => ApprovalStatusFilterKey::Approved,
            ApprovalStatusFilter::Rejected => ApprovalStatusFilterKey::Rejected,
            ApprovalStatusFilter::Expired => ApprovalStatusFilterKey::Expired,
            ApprovalStatusFilter::All => ApprovalStatusFilterKey::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserBenchViewKey {
    ParserList,
}

impl ParserBenchViewKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ParserBenchViewKey::ParserList => "parser_list",
        }
    }
}

impl From<ParserBenchView> for ParserBenchViewKey {
    fn from(value: ParserBenchView) -> Self {
        match value {
            ParserBenchView::ParserList => ParserBenchViewKey::ParserList,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryViewStateKey {
    Editing,
    Executing,
    ViewingResults,
    TableBrowser,
    SavedQueries,
}

impl QueryViewStateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryViewStateKey::Editing => "editing",
            QueryViewStateKey::Executing => "executing",
            QueryViewStateKey::ViewingResults => "viewing_results",
            QueryViewStateKey::TableBrowser => "table_browser",
            QueryViewStateKey::SavedQueries => "saved_queries",
        }
    }
}

impl From<QueryViewState> for QueryViewStateKey {
    fn from(value: QueryViewState) -> Self {
        match value {
            QueryViewState::Editing => QueryViewStateKey::Editing,
            QueryViewState::Executing => QueryViewStateKey::Executing,
            QueryViewState::ViewingResults => QueryViewStateKey::ViewingResults,
            QueryViewState::TableBrowser => QueryViewStateKey::TableBrowser,
            QueryViewState::SavedQueries => QueryViewStateKey::SavedQueries,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsCategoryKey {
    General,
    Display,
    About,
}

impl SettingsCategoryKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SettingsCategoryKey::General => "general",
            SettingsCategoryKey::Display => "display",
            SettingsCategoryKey::About => "about",
        }
    }
}

impl From<SettingsCategory> for SettingsCategoryKey {
    fn from(value: SettingsCategory) -> Self {
        match value {
            SettingsCategory::General => SettingsCategoryKey::General,
            SettingsCategory::Display => SettingsCategoryKey::Display,
            SettingsCategory::About => SettingsCategoryKey::About,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionsViewStateKey {
    SessionList,
    SessionDetail,
    WorkflowProgress,
    ProposalReview,
    GateApproval,
}

impl SessionsViewStateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionsViewStateKey::SessionList => "session_list",
            SessionsViewStateKey::SessionDetail => "session_detail",
            SessionsViewStateKey::WorkflowProgress => "workflow_progress",
            SessionsViewStateKey::ProposalReview => "proposal_review",
            SessionsViewStateKey::GateApproval => "gate_approval",
        }
    }
}

impl From<SessionsViewState> for SessionsViewStateKey {
    fn from(value: SessionsViewState) -> Self {
        match value {
            SessionsViewState::SessionList => SessionsViewStateKey::SessionList,
            SessionsViewState::SessionDetail => SessionsViewStateKey::SessionDetail,
            SessionsViewState::WorkflowProgress => SessionsViewStateKey::WorkflowProgress,
            SessionsViewState::ProposalReview => SessionsViewStateKey::ProposalReview,
            SessionsViewState::GateApproval => SessionsViewStateKey::GateApproval,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageTabKey {
    Quarantine,
    SchemaMismatch,
    DeadLetter,
}

impl TriageTabKey {
    pub fn as_str(self) -> &'static str {
        match self {
            TriageTabKey::Quarantine => "quarantine",
            TriageTabKey::SchemaMismatch => "schema_mismatch",
            TriageTabKey::DeadLetter => "dead_letter",
        }
    }
}

impl From<TriageTab> for TriageTabKey {
    fn from(value: TriageTab) -> Self {
        match value {
            TriageTab::Quarantine => TriageTabKey::Quarantine,
            TriageTab::SchemaMismatch => TriageTabKey::SchemaMismatch,
            TriageTab::DeadLetter => TriageTabKey::DeadLetter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogTabKey {
    Pipelines,
    Runs,
}

impl CatalogTabKey {
    pub fn as_str(self) -> &'static str {
        match self {
            CatalogTabKey::Pipelines => "pipelines",
            CatalogTabKey::Runs => "runs",
        }
    }
}

impl From<CatalogTab> for CatalogTabKey {
    fn from(value: CatalogTab) -> Self {
        match value {
            CatalogTab::Pipelines => CatalogTabKey::Pipelines,
            CatalogTab::Runs => CatalogTabKey::Runs,
        }
    }
}

fn bool_key(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tui::snapshot_states::snapshot_cases;

    fn find_case(name: &str) -> &'static super::super::snapshot_states::SnapshotCase {
        snapshot_cases()
            .iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("missing snapshot case {}", name))
    }

    #[test]
    fn signature_ignores_tick() {
        let case = find_case("home_default");
        let mut app = (case.build)();
        let sig1 = UiSignature::from_app(&app);
        app.tick();
        app.tick();
        let sig2 = UiSignature::from_app(&app);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn signature_changes_on_help_toggle() {
        let case = find_case("home_default");
        let mut app = (case.build)();
        let sig1 = UiSignature::from_app(&app);
        app.show_help = true;
        let sig2 = UiSignature::from_app(&app);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn signature_changes_on_view_state() {
        let case = find_case("discover_rule_builder");
        let mut app = (case.build)();
        let sig1 = UiSignature::from_app(&app);
        app.discover.view_state = DiscoverViewState::RulesManager;
        let sig2 = UiSignature::from_app(&app);
        assert_ne!(sig1, sig2);
    }
}

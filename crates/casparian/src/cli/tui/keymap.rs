use super::app::{
    App, ApprovalsViewState, DiscoverViewState, IngestTab, JobsViewState, QueryViewState,
    ReviewTab, RunTab, SessionsViewState, TuiMode,
};
use super::components::action_bar::ActionHint;

fn ordered_hints(items: &[(&'static str, &'static str)]) -> Vec<ActionHint> {
    let mut hints = Vec::with_capacity(items.len());
    let mut priority: i16 = 100;
    for (key, label) in items {
        let prio = priority.max(1) as u8;
        hints.push(ActionHint::new(*key, *label, prio));
        priority -= 5;
    }
    hints
}

fn discover_actions(app: &App) -> Vec<ActionHint> {
    let mut items = vec![
        ("e", "Sample"),
        ("E", "Full"),
        ("t", "Apply Tag"),
        ("b", "Backtest"),
        ("s", "Scan"),
        ("Ctrl+S", "Save"),
        ("Ctrl+N", "Clear"),
        ("I", "Inspector"),
        ("?", "Help"),
    ];

    if app.is_text_input_mode() && app.discover.view_state == DiscoverViewState::RuleBuilder {
        items.insert(4, ("Esc", "Exit input"));
    }

    ordered_hints(&items)
}

fn home_actions() -> Vec<ActionHint> {
    ordered_hints(&[
        ("Enter", "Open"),
        ("s", "Scan"),
        ("/", "Filter"),
        ("r", "Refresh"),
        ("I", "Inspector"),
        ("?", "Help"),
    ])
}

fn sources_actions() -> Vec<ActionHint> {
    ordered_hints(&[
        ("Up/Down", "Navigate"),
        ("n", "New"),
        ("e", "Edit"),
        ("r", "Rescan"),
        ("d", "Delete"),
        ("I", "Inspector"),
        ("?", "Help"),
    ])
}

fn approvals_actions(app: &App) -> Vec<ActionHint> {
    if app.approvals_state.view_state != ApprovalsViewState::List {
        return Vec::new();
    }

    ordered_hints(&[
        ("j/k", "Navigate"),
        ("a", "Approve"),
        ("r", "Reject"),
        ("f", "Filter"),
        ("Enter", "Pin"),
        ("d", "Details"),
        ("R", "Refresh"),
        ("I", "Inspector"),
    ])
}

fn jobs_actions(app: &App) -> Vec<ActionHint> {
    match app.jobs_state.view_state {
        JobsViewState::MonitoringPanel => ordered_hints(&[
            ("Esc", "Back"),
            ("p", "Pause updates"),
            ("x", "Reset stats"),
            ("I", "Inspector"),
        ]),
        JobsViewState::LogViewer => ordered_hints(&[
            ("Esc", "Back"),
            ("Up/Down", "Scroll"),
            ("y", "Copy path"),
            ("?", "Help"),
        ]),
        JobsViewState::FilterDialog => ordered_hints(&[
            ("s", "Status"),
            ("t", "Type"),
            ("x", "Clear"),
            ("Enter", "Apply"),
            ("Esc", "Back"),
        ]),
        JobsViewState::ViolationDetail => ordered_hints(&[
            ("Up/Down", "Select"),
            ("a", "Apply fix"),
            ("v/Esc", "Back"),
            ("?", "Help"),
        ]),
        _ => ordered_hints(&[
            ("Up/Down", "Select"),
            ("Tab", "Section"),
            ("Enter", "Pin"),
            ("p", "Pipeline"),
            ("f", "Filter"),
            ("Del", "Clear"),
            ("Q", "Quarantine"),
            ("C", "Catalog"),
            ("v", "Violations"),
            ("L", "Logs"),
            ("O", "Open"),
            ("y", "Copy path"),
            ("I", "Inspector"),
            ("?", "Help"),
        ]),
    }
}

fn query_actions(app: &App) -> Vec<ActionHint> {
    match app.query_state.view_state {
        QueryViewState::Editing => {
            if app.query_state.executing {
                ordered_hints(&[("Tab", "Results")])
            } else {
                ordered_hints(&[
                    ("Ctrl+Enter", "Execute"),
                    ("Ctrl+L", "Clear"),
                    ("Ctrl+T", "Tables"),
                    ("Ctrl+S", "Save"),
                    ("Ctrl+O", "Open"),
                    ("Tab", "Results"),
                ])
            }
        }
        QueryViewState::Executing => ordered_hints(&[("Esc", "Detach")]),
        QueryViewState::ViewingResults => ordered_hints(&[
            ("Tab/Esc", "Editor"),
            ("Up/Down", "Navigate"),
            ("Left/Right", "Scroll"),
            ("PgUp/PgDn", "Page"),
        ]),
        QueryViewState::TableBrowser => ordered_hints(&[("Enter", "Insert"), ("Esc", "Close")]),
        QueryViewState::SavedQueries => ordered_hints(&[("Enter", "Load"), ("Esc", "Close")]),
    }
}

fn settings_actions(app: &App) -> Vec<ActionHint> {
    if app.settings.editing {
        ordered_hints(&[("Enter", "Save"), ("Esc", "Cancel"), ("I", "Inspector")])
    } else {
        ordered_hints(&[
            ("Up/Down", "Navigate"),
            ("Tab", "Category"),
            ("Enter", "Edit/Toggle"),
            ("I", "Inspector"),
        ])
    }
}

fn sessions_actions(app: &App) -> Vec<ActionHint> {
    match app.sessions_state.view_state {
        SessionsViewState::GateApproval => {
            ordered_hints(&[("a", "Approve"), ("r", "Reject"), ("Esc", "Back"), ("I", "Inspector")])
        }
        SessionsViewState::SessionList => ordered_hints(&[
            ("Enter", "View"),
            ("n", "New Session"),
            ("r", "Refresh"),
            ("Esc", "Back"),
            ("I", "Inspector"),
        ]),
        SessionsViewState::SessionDetail => ordered_hints(&[
            ("w", "Workflow"),
            ("j", "Jobs"),
            ("q", "Query"),
            ("d", "Discover"),
            ("Esc", "Back"),
            ("I", "Inspector"),
        ]),
        _ => ordered_hints(&[("Esc", "Back"), ("I", "Inspector")]),
    }
}

fn triage_actions(app: &App) -> Vec<ActionHint> {
    let mut items = vec![
        ("Tab", "Next Tab"),
        ("Up/Down", "Select"),
        ("j", "Jobs"),
        ("y", "Copy"),
        ("r", "Refresh"),
        ("Esc", "Back"),
    ];
    if app.triage_state.job_filter.is_some() {
        items.insert(4, ("Del", "Clear filter"));
    }
    ordered_hints(&items)
}

fn catalog_actions() -> Vec<ActionHint> {
    ordered_hints(&[
        ("Tab", "Next Tab"),
        ("Up/Down", "Select"),
        ("Enter", "Runs"),
        ("r", "Refresh"),
        ("Esc", "Back"),
    ])
}

impl App {
    pub fn effective_actions(&self) -> Vec<ActionHint> {
        match self.mode {
            TuiMode::Home => home_actions(),
            TuiMode::Ingest => match self.ingest_tab {
                IngestTab::Sources => sources_actions(),
                _ => discover_actions(self),
            },
            TuiMode::Run => match self.run_tab {
                RunTab::Jobs => jobs_actions(self),
                RunTab::Outputs => catalog_actions(),
            },
            TuiMode::Review => match self.review_tab {
                ReviewTab::Triage => triage_actions(self),
                ReviewTab::Approvals => approvals_actions(self),
                ReviewTab::Sessions => sessions_actions(self),
            },
            TuiMode::Query => query_actions(self),
            TuiMode::Settings => settings_actions(self),
        }
    }

    pub fn global_actions(&self) -> Vec<ActionHint> {
        let mut actions = ordered_hints(&[
            ("?", "Help"),
            ("q", "Quit"),
            ("r", "Refresh"),
            ("I", "Inspector"),
            ("Ctrl+W", "Workspaces"),
            (":", "Command"),
            (">", "Intent"),
            ("[", "Prev Tab"),
            ("]", "Next Tab"),
            ("0", "Home"),
            ("1", "Ingest"),
            ("2", "Run"),
            ("3", "Review"),
            ("4", "Query"),
            ("5", "Settings"),
        ]);

        if self.is_text_input_mode() {
            actions.insert(0, ActionHint::new("Esc", "Exit input", 110));
        }

        actions
    }
}

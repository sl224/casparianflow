use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {

    pub(super) fn handle_command_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.command_palette.close();
            }
            KeyCode::Enter => {
                // Execute the selected action
                if let Some(action) = self.command_palette.selected_action().cloned() {
                    match action {
                        CommandAction::Navigate(mode) => {
                            self.command_palette.close();
                            self.navigate_to_mode(mode);
                        }
                        CommandAction::StartIntent(intent) => {
                            // Add to history before closing
                            self.command_palette.add_to_history(intent.clone());
                            self.command_palette.close();
                            match self.create_intent_session(&intent) {
                                Ok(session_id) => {
                                    self.sessions_state.pending_select_session_id =
                                        Some(session_id);
                                    self.sessions_state.sessions_loaded = false;
                                    self.sessions_state.view_state = SessionsViewState::SessionList;
                                    self.sessions_state.active_session = None;
                                    self.set_review_tab(ReviewTab::Sessions);
                                }
                                Err(err) => {
                                    tracing::error!("{}", err);
                                }
                            }
                        }
                        CommandAction::RunCommand(cmd) => {
                            self.command_palette.close();
                            // Handle slash commands
                            match cmd.as_str() {
                                "/jobs" => self.set_run_tab(RunTab::Jobs),
                                "/approve" => self.set_review_tab(ReviewTab::Approvals),
                                "/query" => self.navigate_to_mode(TuiMode::Query),
                                "/workspace" => self.open_workspace_switcher(),
                                "/quarantine" => self.open_triage(None),
                                "/catalog" | "/pipelines" => self.open_catalog(None),
                                "/scan" => {
                                    self.navigate_to_mode(TuiMode::Ingest);
                                    self.set_ingest_tab(IngestTab::Select);
                                    self.transition_discover_state(DiscoverViewState::EnteringPath);
                                }
                                "/help" => {
                                    self.show_help = true;
                                }
                                _ => {
                                    // Unknown command - show in status
                                    self.discover.status_message =
                                        Some((format!("Unknown command: {}", cmd), true));
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Tab => {
                // Cycle through modes
                self.command_palette.cycle_mode();
            }
            KeyCode::Up => {
                self.command_palette.select_prev();
            }
            KeyCode::Down => {
                self.command_palette.select_next();
            }
            KeyCode::Left => {
                self.command_palette.cursor_left();
            }
            KeyCode::Right => {
                self.command_palette.cursor_right();
            }
            KeyCode::Home => {
                self.command_palette.cursor_home();
            }
            KeyCode::End => {
                self.command_palette.cursor_end();
            }
            KeyCode::Backspace => {
                self.command_palette.backspace();
            }
            KeyCode::Delete => {
                self.command_palette.delete();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+U clears input
                self.command_palette.clear_input();
            }
            KeyCode::Char(c) => {
                self.command_palette.insert_char(c);
            }
            _ => {}
        }
    }

    pub(super) fn open_workspace_switcher(&mut self) {
        self.ensure_active_workspace();
        self.workspace_switcher.visible = true;
        self.workspace_switcher.mode = WorkspaceSwitcherMode::List;
        self.workspace_switcher.input.clear();
        self.workspace_switcher.status_message = None;
        self.load_workspace_switcher_list();
    }

    fn close_workspace_switcher(&mut self) {
        self.workspace_switcher.visible = false;
        self.workspace_switcher.mode = WorkspaceSwitcherMode::List;
        self.workspace_switcher.input.clear();
        self.workspace_switcher.status_message = None;
    }

    fn load_workspace_switcher_list(&mut self) {
        self.workspace_switcher.workspaces.clear();
        self.workspace_switcher.loaded = false;
        match self.query_workspaces() {
            Ok(workspaces) => {
                self.workspace_switcher.workspaces = workspaces;
                self.workspace_switcher.loaded = true;
                if let Some(active_id) = self.active_workspace_id() {
                    if let Some(idx) = self
                        .workspace_switcher
                        .workspaces
                        .iter()
                        .position(|ws| ws.id == active_id)
                    {
                        self.workspace_switcher.selected_index = idx;
                    } else {
                        self.workspace_switcher.selected_index = 0;
                    }
                } else {
                    self.workspace_switcher.selected_index = 0;
                }
            }
            Err(err) => {
                self.workspace_switcher.status_message = Some(err);
            }
        }
    }

    pub(super) fn handle_workspace_switcher_key(&mut self, key: KeyEvent) {
        match self.workspace_switcher.mode {
            WorkspaceSwitcherMode::List => match key.code {
                KeyCode::Esc => {
                    self.close_workspace_switcher();
                }
                KeyCode::Up => {
                    if self.workspace_switcher.selected_index > 0 {
                        self.workspace_switcher.selected_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.workspace_switcher.selected_index + 1
                        < self.workspace_switcher.workspaces.len()
                    {
                        self.workspace_switcher.selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(workspace) = self
                        .workspace_switcher
                        .workspaces
                        .get(self.workspace_switcher.selected_index)
                        .cloned()
                    {
                        self.apply_active_workspace(workspace);
                        self.close_workspace_switcher();
                    }
                }
                KeyCode::Char('n') => {
                    if self.mutations_blocked() {
                        self.workspace_switcher.status_message =
                            Some("Sentinel not reachable; cannot create workspace".to_string());
                    } else {
                        self.workspace_switcher.mode = WorkspaceSwitcherMode::Creating;
                        self.workspace_switcher.input.clear();
                        self.workspace_switcher.status_message = None;
                    }
                }
                KeyCode::Char('r') => {
                    self.load_workspace_switcher_list();
                }
                _ => {}
            },
            WorkspaceSwitcherMode::Creating => {
                match handle_text_input(key, &mut self.workspace_switcher.input) {
                    TextInputResult::Committed => {
                        let name = self.workspace_switcher.input.trim().to_string();
                        if name.is_empty() {
                            self.workspace_switcher.status_message =
                                Some("Workspace name is required".to_string());
                            return;
                        }
                        match self.create_workspace(&name) {
                            Ok(workspace) => {
                                self.apply_active_workspace(workspace);
                                self.close_workspace_switcher();
                            }
                            Err(err) => {
                                self.workspace_switcher.status_message = Some(err);
                            }
                        }
                    }
                    TextInputResult::Cancelled => {
                        self.workspace_switcher.mode = WorkspaceSwitcherMode::List;
                        self.workspace_switcher.input.clear();
                    }
                    TextInputResult::Continue => {}
                    TextInputResult::NotHandled => {}
                }
            }
        }
    }

    fn create_workspace(&mut self, name: &str) -> Result<Workspace, String> {
        let db = self
            .open_scout_db_for_writes()
            .ok_or_else(|| "Database unavailable for writes".to_string())?;
        if let Ok(Some(existing)) = db.get_workspace_by_name(name) {
            return Err(format!("Workspace '{}' already exists", existing.name));
        }
        db.create_workspace(name)
            .map_err(|err| format!("Create workspace failed: {}", err))
    }

    fn apply_active_workspace(&mut self, workspace: Workspace) {
        let name = workspace.name.clone();
        let id = workspace.id;
        let was_ingest = self.mode == TuiMode::Ingest && self.ingest_tab != IngestTab::Sources;
        self.reset_workspace_scoped_state();
        self.active_workspace = Some(workspace);
        if let Err(err) = context::set_active_workspace(&id) {
            self.workspace_switcher
                .status_message
                .replace(format!("Failed to persist workspace context: {}", err));
        } else {
            self.set_global_status(format!("Switched workspace to {}", name), false);
        }
        if was_ingest {
            self.enter_discover_mode();
        }
    }

    fn reset_workspace_scoped_state(&mut self) {
        // Discover caches and selections
        self.discover = DiscoverState {
            page_size: DISCOVER_PAGE_SIZE,
            ..Default::default()
        };

        // Cancel any pending workspace-scoped async work.
        self.pending_cache_load = None;
        self.pending_folder_query = None;
        self.pending_glob_search = None;
        self.pending_rule_builder_search = None;
        self.cache_load_progress = None;
        self.last_cache_load_timing = None;
        self.glob_search_cancelled = None;

        // Sources view state
        self.sources_state = SourcesState::default();

        // Home stats should reload for the new workspace
        self.home.stats_loaded = false;
        self.home.stats = HomeStats::default();
        self.home.recent_jobs.clear();
        self.home.selected_source_index = 0;
        self.home.filter.clear();
        self.home.filtering = false;

        // Jobs may be workspace-scoped; reload and clear filters.
        self.jobs_state.jobs.clear();
        self.jobs_state.jobs_loaded = false;
        self.jobs_state.status_filter = None;
        self.jobs_state.type_filter = None;
        self.jobs_state.selected_index = 0;
        self.jobs_state.pinned_job_id = None;

        // Invalidate cached loads.
        self.pending_sources_load = None;
        self.pending_stats_load = None;
        self.pending_jobs_load = None;
    }

    fn create_intent_session(&self, intent: &str) -> Result<String, String> {
        let store = SessionStore::with_root(casparian_home().join("sessions"));
        match store.create_session(intent, Some("tui"), Some("tui")) {
            Ok(bundle) => Ok(bundle.session_id.to_string()),
            Err(err) => Err(format!("Session create failed: {}", err)),
        }
    }
    pub(super) fn handle_home_key(&mut self, key: KeyEvent) {
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
        let filtered_indices: Vec<usize> = self
            .discover
            .sources
            .iter()
            .enumerate()
            .filter(|(_, source)| {
                filter_lower.is_empty()
                    || source.name.to_lowercase().contains(&filter_lower)
                    || source
                        .path
                        .display()
                        .to_string()
                        .to_lowercase()
                        .contains(&filter_lower)
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
                if let Some(pos) = filtered_indices
                    .iter()
                    .position(|idx| *idx == self.home.selected_source_index)
                {
                    if pos > 0 {
                        self.home.selected_source_index = filtered_indices[pos - 1];
                    }
                }
            }
            // Navigate down in source list
            KeyCode::Down => {
                if let Some(pos) = filtered_indices
                    .iter()
                    .position(|idx| *idx == self.home.selected_source_index)
                {
                    if pos + 1 < source_count {
                        self.home.selected_source_index = filtered_indices[pos + 1];
                    }
                }
            }
            // Enter: Start scan job for selected source
            KeyCode::Enter => {
                if source_count > 0 && self.home.selected_source_index < self.discover.sources.len()
                {
                    // Clone the source_id to avoid borrow conflict
                    let source_id = self.discover.sources[self.home.selected_source_index].id;
                    self.start_scan_for_source(source_id);
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
}

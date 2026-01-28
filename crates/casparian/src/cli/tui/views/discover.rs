use super::*;
use casparian::scout::{TagSource, TaggingRuleId};
use casparian_sentinel::ControlClient;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::cli::tui::extraction;
use std::time::Duration;

impl App {
    /// Handle Discover mode keys - using unified state machine
    pub(super) fn handle_discover_key(&mut self, key: KeyEvent) {
        // Clear status message on any key press
        if self.discover.status_message.is_some() && key.code != KeyCode::Esc {
            self.discover.status_message = None;
        }

        // Global keybindings that work from most states (per spec Section 6.1)
        // R (Rules Manager), M (Sources Manager), S/T (Sources/Tags dropdowns)
        // work from Files, dropdowns, etc.
        // but NOT from dialogs or text input modes
        if !self.in_text_input_mode()
            && !matches!(
                self.discover.view_state,
                DiscoverViewState::RulesManager
                    | DiscoverViewState::RuleCreation
                    | DiscoverViewState::SourcesManager
                    | DiscoverViewState::SourceEdit
                    | DiscoverViewState::SourceDeleteConfirm
            )
        {
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
                KeyCode::Char('S') => {
                    self.handle_discover_panel_shortcut(KeyCode::Char('S'));
                    return;
                }
                KeyCode::Char('T') => {
                    self.handle_discover_panel_shortcut(KeyCode::Char('T'));
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
                        self.discover.path_suggestions.clear();
                        if path.is_empty() {
                            self.discover.view_state = DiscoverViewState::Files;
                            return;
                        }
                        if self.is_risky_scan_path(&path) {
                            self.discover.scan_confirm_path = Some(path);
                            self.discover.view_state = DiscoverViewState::ScanConfirm;
                            self.discover.status_message = Some((
                                "Confirm scan of a risky path (Enter to proceed, Esc to cancel)"
                                    .to_string(),
                                true,
                            ));
                        } else {
                            self.discover.view_state = DiscoverViewState::Files;
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

            DiscoverViewState::ScanConfirm => match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(path) = self.discover.scan_confirm_path.take() {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.status_message = None;
                        self.scan_directory(&path);
                    } else {
                        self.discover.view_state = DiscoverViewState::Files;
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.discover.scan_confirm_path = None;
                    self.discover.view_state = DiscoverViewState::Files;
                    self.discover
                        .status_message
                        .replace(("Scan cancelled".to_string(), true));
                }
                _ => {}
            },

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
                            if self.mutations_blocked() {
                                let message = BackendRouter::new(
                                    self.control_addr.clone(),
                                    self.config.standalone_writer,
                                    self.db_read_only,
                                )
                                .blocked_message("apply tags");
                                self.discover.scan_error = Some(message.clone());
                                self.discover.status_message = Some((message, true));
                                self.discover.view_state = DiscoverViewState::Files;
                                self.discover.bulk_tag_input.clear();
                                self.discover.bulk_tag_save_as_rule = false;
                                return;
                            }
                            let workspace_id = match self.active_workspace_id() {
                                Some(id) => id,
                                None => {
                                    let message =
                                        "No workspace selected; cannot apply tags".to_string();
                                    self.discover.scan_error = Some(message.clone());
                                    self.discover.status_message = Some((message, true));
                                    self.discover.view_state = DiscoverViewState::Files;
                                    self.discover.bulk_tag_input.clear();
                                    self.discover.bulk_tag_save_as_rule = false;
                                    return;
                                }
                            };

                            let file_ids: Vec<i64> =
                                self.filtered_files().iter().map(|f| f.file_id).collect();
                            let mut tagged_count = 0;
                            for file_id in file_ids {
                                if self.queue_tag_for_file(
                                    file_id,
                                    &tag,
                                    TagSource::Manual,
                                    None,
                                    false,
                                ) {
                                    tagged_count += 1;
                                }
                            }

                            let mut rule_suffix = String::new();
                            let mut rule_error = false;
                            if self.discover.bulk_tag_save_as_rule {
                                match self.discover_rule_pattern_from_filter() {
                                    Some(pattern) => {
                                        let rule_id = TaggingRuleId::new();
                                        self.discover
                                            .pending_rule_writes
                                            .push(PendingRuleWrite {
                                                id: rule_id,
                                                workspace_id,
                                                pattern: pattern.clone(),
                                                tag: tag.clone(),
                                            });
                                        self.discover.rules.push(RuleInfo {
                                            id: RuleId::new(rule_id),
                                            pattern,
                                            tag: tag.clone(),
                                            priority: 100,
                                            enabled: true,
                                        });
                                        rule_suffix.push_str(" (rule saved)");
                                    }
                                    None => {
                                        let message =
                                            "Save as rule requires a filter pattern (press /)"
                                                .to_string();
                                        self.discover.scan_error = Some(message.clone());
                                        rule_suffix.push_str(" (rule not saved: add a filter)");
                                        rule_error = true;
                                    }
                                }
                            }
                            self.discover.status_message = Some((
                                format!(
                                    "Tagged {} files with '{}'{}",
                                    tagged_count, tag, rule_suffix
                                ),
                                rule_error,
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
                        if let Some(matching_tag) = self
                            .discover
                            .available_tags
                            .iter()
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
                                self.queue_tag_for_file(
                                    file.file_id,
                                    &tag,
                                    TagSource::Manual,
                                    None,
                                    true,
                                );
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
                        self.discover.page_offset = 0;
                        self.discover.data_loaded = false;
                        self.discover.db_filtered = false;
                    }
                    TextInputResult::Cancelled => {
                        self.discover.view_state = DiscoverViewState::Files;
                        self.discover.filter.clear();
                        self.discover.page_offset = 0;
                        self.discover.data_loaded = false;
                        self.discover.db_filtered = false;
                    }
                    TextInputResult::Continue | TextInputResult::NotHandled => {
                        self.discover.db_filtered = false;
                    }
                }
            }

            // === Dialog states ===
            DiscoverViewState::RuleCreation => match key.code {
                KeyCode::Enter => {
                    let tag = self.discover.rule_tag_input.trim().to_string();
                    let pattern = self.discover.rule_pattern_input.trim().to_string();
                    if !tag.is_empty() && !pattern.is_empty() {
                        if self.apply_rule_to_files(&pattern, &tag).is_none() {
                            return;
                        }
                    } else if tag.is_empty() && !pattern.is_empty() {
                        self.discover.status_message =
                            Some(("Please enter a tag name".to_string(), true));
                        return;
                    } else if pattern.is_empty() {
                        self.discover.status_message =
                            Some(("Please enter a pattern".to_string(), true));
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
                    RuleDialogFocus::Tag => {
                        self.discover.rule_tag_input.pop();
                    }
                },
                _ => {}
            },

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
                            self.update_scan_job_status(
                                job_id,
                                JobStatus::Cancelled,
                                None,
                                None,
                                None,
                            );
                        }

                        if self.control_connected {
                            if let (Some(scan_id), Some(control_addr)) =
                                (self.current_scan_id.clone(), self.control_addr.clone())
                            {
                                std::thread::spawn(move || {
                                    if let Ok(client) = ControlClient::connect_with_timeout(
                                        &control_addr,
                                        Duration::from_millis(200),
                                    ) {
                                        let _ = client.cancel_scan(scan_id);
                                    }
                                });
                            }
                        } else if let Some(token) = self.scan_cancel_token.take() {
                            token.cancel();
                        }

                        self.pending_scan = None;
                        self.current_scan_job_id = None;
                        self.current_scan_id = None;
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
                        self.set_mode(TuiMode::Home);
                        self.discover.status_message =
                            Some(("Scan running in background...".to_string(), false));
                    }
                    // Navigate to Run (Jobs) while scan continues in background
                    KeyCode::Char('2') => {
                        // Don't cancel - scan continues, just switch view
                        self.discover.view_state = DiscoverViewState::Files;
                        self.set_run_tab(RunTab::Jobs);
                        self.discover.status_message =
                            Some(("Scan running in background...".to_string(), false));
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
                        let in_glob_editor_phase = self
                            .discover
                            .glob_explorer
                            .as_ref()
                            .map(|e| {
                                !matches!(
                                    e.phase,
                                    GlobExplorerPhase::Browse | GlobExplorerPhase::Filtering
                                )
                            })
                            .unwrap_or(false);
                        if in_glob_editor_phase {
                            self.handle_discover_files_key(key);
                            return;
                        }
                        self.discover.filter.clear();
                        self.discover.selected = 0;
                        self.discover.page_offset = 0;
                        self.discover.data_loaded = false;
                        self.discover.db_filtered = false;
                    }
                    KeyCode::Esc => {
                        self.set_mode(TuiMode::Home);
                    }
                    _ => match self.discover.focus {
                        DiscoverFocus::Files => self.handle_discover_files_key(key),
                        DiscoverFocus::Sources => self.handle_discover_sources_key(key),
                        DiscoverFocus::Tags => self.handle_discover_tags_key(key),
                    },
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
                self.discover.db_filtered = false;
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
            KeyCode::PageDown => {
                self.discover_next_page();
            }
            KeyCode::PageUp => {
                self.discover_prev_page();
            }
            KeyCode::Home => {
                self.discover_first_page();
            }
            KeyCode::End => {
                self.discover_last_page();
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
                self.discover.scan_path_input = self
                    .discover
                    .selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
                self.discover.scan_error = None;
            }
            KeyCode::Char('r') => {
                // Reload from Scout DB
                self.discover.data_loaded = false;
                self.discover.db_filtered = false;
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
                    self.discover.status_message =
                        Some(("Enter a filter pattern first (press /)".to_string(), true));
                }
            }
            KeyCode::Char('c') => {
                // Create source from selected directory
                let file_info = self
                    .filtered_files()
                    .get(self.discover.selected)
                    .map(|f| (f.is_dir, f.path.clone()));

                if let Some((is_dir, path)) = file_info {
                    if is_dir {
                        self.transition_discover_state(DiscoverViewState::CreatingSource);
                        self.discover.source_name_input.clear();
                        self.discover.pending_source_path = Some(path);
                    } else {
                        self.discover.status_message =
                            Some(("Select a directory to create a source".to_string(), true));
                    }
                }
            }
            KeyCode::Char('B') => {
                // Bulk tag all filtered/visible files (explicit B)
                let count = self.filtered_files().len();
                if count > 0 {
                    self.transition_discover_state(DiscoverViewState::BulkTagging);
                    self.discover.bulk_tag_input.clear();
                    self.discover.bulk_tag_save_as_rule = false;
                } else {
                    self.discover.status_message = Some(("No files to tag".to_string(), true));
                }
            }
            _ => {}
        }
    }

    /// Handle keys when Glob Explorer is active (hierarchical folder navigation)
    fn handle_glob_explorer_key(&mut self, key: KeyEvent) {
        // Check current phase to determine behavior
        let phase = self
            .discover
            .glob_explorer
            .as_ref()
            .map(|e| e.phase.clone());

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
                        } else if explorer.pattern.is_empty() && !explorer.current_prefix.is_empty()
                        {
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
        if let Some(GlobExplorerPhase::EditRule {
            focus,
            selected_index,
            editing_field,
        }) = phase.clone()
        {
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
                        if !folder.is_file()
                            && !folder_name.contains("Loading folder hierarchy")
                            && !folder_name.contains("Searching")
                        {
                            // Save current (prefix, pattern) to history for back navigation
                            explorer
                                .nav_history
                                .push((explorer.current_prefix.clone(), explorer.pattern.clone()));

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
                                explorer.current_prefix =
                                    format!("{}{}/", explorer.current_prefix, folder_name);
                            }

                            // Stay in Browse phase (navigation) - phase doesn't change based on folder depth
                            explorer.selected_folder = 0;
                        }
                    }
                }
                // Update from cache - O(1) hashmap lookup, no SQL
                self.update_folders_from_cache();
            }
            KeyCode::Left => {
                // Go back to parent folder - O(1) using cache
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
                let source_id = self.discover.selected_source().map(|s| s.id);

                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    let match_count = explorer.total_count.value();
                    if match_count > 0 {
                        // Create a new rule draft from current pattern
                        let pattern = if explorer.current_prefix.is_empty() {
                            explorer.pattern.clone()
                        } else {
                            format!("{}{}", explorer.current_prefix, explorer.pattern)
                        };
                        explorer.rule_draft = Some(extraction::RuleDraft::from_pattern(
                            &pattern, source_id,
                        ));
                        explorer.phase = GlobExplorerPhase::EditRule {
                            focus: extraction::RuleEditorFocus::GlobPattern,
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
                self.discover.db_filtered = false;
            }
            KeyCode::Char('s') => {
                // Open scan path input (same as normal mode)
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                // Pre-fill with selected source path if available
                self.discover.scan_path_input = self
                    .discover
                    .selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
                self.discover.scan_error = None;
            }
            // --- Rule Builder: Result Filtering (a/p/f keys) ---
            KeyCode::Char('a') => {
                // Show all results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = extraction::ResultFilter::All;
                }
            }
            KeyCode::Char('p') => {
                // Show only passing results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = extraction::ResultFilter::PassOnly;
                }
            }
            KeyCode::Char('f') => {
                // Show only failing results
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    explorer.result_filter = extraction::ResultFilter::FailOnly;
                }
            }
            // --- Rule Builder: Exclusion System (x/i keys) ---
            KeyCode::Char('x') => {
                // Exclude selected file/folder from rule
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(folder) = explorer.folders.get(explorer.selected_folder) {
                        // Build exclusion pattern from current item
                        let exclude_pattern =
                            if folder.is_file() {
                                // Exclude specific file
                                if let Some(path) = folder.path() {
                                    path.to_string()
                                } else {
                                    format!("{}{}", explorer.current_prefix, folder.name())
                                }
                            } else {
                                // Exclude folder and all contents
                                let path =
                                    folder.path().map(|value| value.to_string()).unwrap_or_else(
                                        || format!("{}{}", explorer.current_prefix, folder.name()),
                                    );
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
        focus: extraction::RuleEditorFocus,
        selected_index: usize,
        _editing_field: Option<extraction::FieldEditFocus>,
    ) {
        use extraction::RuleEditorFocus;

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
                if matches!(
                    focus,
                    RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag
                ) {
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
                                explorer.test_state = Some(extraction::TestState::new(
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
                if matches!(
                    focus,
                    RuleEditorFocus::FieldList | RuleEditorFocus::Conditions
                ) {
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
                if matches!(
                    focus,
                    RuleEditorFocus::FieldList | RuleEditorFocus::Conditions
                ) {
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
                if matches!(
                    focus,
                    RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag
                ) {
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
                                    draft.fields.push(extraction::FieldDraft::default());
                                }
                                RuleEditorFocus::Conditions => {
                                    draft
                                        .tag_conditions
                                        .push(extraction::TagConditionDraft::default());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            KeyCode::Char('d') => {
                // In text fields, 'd' is just a character
                if matches!(
                    focus,
                    RuleEditorFocus::GlobPattern | RuleEditorFocus::BaseTag
                ) {
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
                        if matches!(
                            test_state.phase,
                            extraction::TestPhase::Complete { .. }
                        ) {
                            let matching_files = explorer.total_count.value();
                            explorer.publish_state = Some(extraction::PublishState::new(
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
                        focus: extraction::RuleEditorFocus::GlobPattern,
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
                if self.mutations_blocked() {
                    let message = BackendRouter::new(
                        self.control_addr.clone(),
                        self.config.standalone_writer,
                        self.db_read_only,
                    )
                    .blocked_message("publish rules");
                    self.discover.scan_error = Some(message.clone());
                    self.discover.status_message = Some((message, true));
                    return;
                }
                // Confirm publish - save to DB and start job
                if let Some(ref mut explorer) = self.discover.glob_explorer {
                    if let Some(ref mut publish_state) = explorer.publish_state {
                        use extraction::PublishPhase;
                        match publish_state.phase {
                            PublishPhase::Confirming => {
                                publish_state.phase = PublishPhase::Saving;
                                // TODO: Actually save to DB (async, non-blocking)
                                // For now, transition directly to Published
                                let job_id = format!(
                                    "cf_extract_{}",
                                    &uuid::Uuid::new_v4().to_string()[..8]
                                );
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
                        focus: extraction::RuleEditorFocus::GlobPattern,
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
        self.discover
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| filter.is_empty() || s.name.to_lowercase().contains(&filter))
            .collect()
    }

    /// Get filtered tags based on dropdown filter
    fn filtered_tags(&self) -> Vec<(usize, &TagInfo)> {
        let filter = self.discover.tags_filter.to_lowercase();
        self.discover
            .tags
            .iter()
            .enumerate()
            .filter(|(_, t)| filter.is_empty() || t.name.to_lowercase().contains(&filter))
            .collect()
    }

    /// Get parser indices filtered by the current Parser Bench filter
    pub fn filtered_parser_indices(&self) -> Vec<usize> {
        let filter = self.parser_bench.filter.to_lowercase();
        self.parser_bench
            .parsers
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                filter.is_empty()
                    || p.name.to_lowercase().contains(&filter)
                    || p.path
                        .display()
                        .to_string()
                        .to_lowercase()
                        .contains(&filter)
            })
            .map(|(idx, _)| idx)
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
                    // Clear filter and close dropdown
                    self.discover.sources_filter.clear();
                    self.discover.sources_filtering = false;
                    self.discover.preview_source = None;
                    self.discover.view_state = DiscoverViewState::RuleBuilder;
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
                        self.discover.pending_source_touch = Some(source_id.clone());
                    }
                    self.discover.data_loaded = false;
                    self.discover.db_filtered = false;
                    self.discover.page_offset = 0;
                    self.discover.total_files = 0;
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
                    // Clear filter and close dropdown
                    self.discover.tags_filter.clear();
                    self.discover.tags_filtering = false;
                    self.discover.preview_tag = None;
                    self.discover.view_state = DiscoverViewState::RuleBuilder;
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
                self.discover.page_offset = 0;
                self.discover.data_loaded = false;
                self.discover.db_filtered = false;
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
                if let Some(rule) = self
                    .discover
                    .rules
                    .get(self.discover.selected_rule)
                    .cloned()
                {
                    self.transition_discover_state(DiscoverViewState::RuleCreation);
                    self.discover.rule_pattern_input = rule.pattern;
                    self.discover.rule_tag_input = rule.tag;
                    self.discover.editing_rule_id = Some(rule.id);
                }
            }
            KeyCode::Char('d') => {
                // Delete selected rule (TODO: add confirmation)
                if !self.discover.rules.is_empty() {
                    if self.mutations_blocked() {
                        self.discover.status_message = Some((
                            "Database is read-only; cannot delete rules".to_string(),
                            true,
                        ));
                        return;
                    }
                    let workspace_id = match self.active_workspace_id() {
                        Some(id) => id,
                        None => {
                            self.discover.status_message = Some((
                                "No workspace selected; cannot delete rule".to_string(),
                                true,
                            ));
                            return;
                        }
                    };
                    if let Some(rule) = self.discover.rules.get(self.discover.selected_rule) {
                        if let Some(id) = rule.id.0 {
                            self.discover
                                .pending_rule_deletes
                                .push(PendingRuleDelete { id, workspace_id });
                        }
                    }
                    self.discover.rules.remove(self.discover.selected_rule);
                    if self.discover.selected_rule >= self.discover.rules.len()
                        && self.discover.selected_rule > 0
                    {
                        self.discover.selected_rule -= 1;
                    }
                }
            }
            KeyCode::Enter => {
                // Toggle rule enabled/disabled
                let workspace_id = match self.active_workspace_id() {
                    Some(id) => id,
                    None => {
                        self.discover.status_message = Some((
                            "No workspace selected; cannot update rule".to_string(),
                            true,
                        ));
                        return;
                    }
                };
                if self.mutations_blocked() {
                    self.discover.status_message = Some((
                        "Sentinel not reachable; cannot update rules".to_string(),
                        true,
                    ));
                    return;
                }
                if let Some(rule) = self.discover.rules.get_mut(self.discover.selected_rule) {
                    rule.enabled = !rule.enabled;
                    if let Some(id) = rule.id.0 {
                        self.discover.pending_rule_updates.push(PendingRuleUpdate {
                            id,
                            enabled: rule.enabled,
                            workspace_id,
                        });
                    }
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
                if self.discover.sources_manager_selected
                    < self.discover.sources.len().saturating_sub(1)
                {
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
                if let Some(source) = self
                    .discover
                    .sources
                    .get(self.discover.sources_manager_selected)
                {
                    self.discover.editing_source = Some(source.id.clone());
                    self.discover.source_edit_input = source.name.clone();
                    self.transition_discover_state(DiscoverViewState::SourceEdit);
                }
            }
            KeyCode::Char('d') => {
                // Delete source (with confirmation)
                if let Some(source) = self
                    .discover
                    .sources
                    .get(self.discover.sources_manager_selected)
                {
                    self.discover.source_to_delete = Some(source.id.clone());
                    self.transition_discover_state(DiscoverViewState::SourceDeleteConfirm);
                }
            }
            KeyCode::Char('r') => {
                // Rescan selected source
                let source_info = self
                    .discover
                    .sources
                    .get(self.discover.sources_manager_selected)
                    .map(|s| (s.path.to_string_lossy().to_string(), s.name.clone()));

                if let Some((path, name)) = source_info {
                    self.scan_directory(&path);
                    self.discover.status_message =
                        Some((format!("Rescanning '{}'...", name), false));
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
                    if let Some(source_id) = self.discover.editing_source.clone() {
                        self.update_source_name(&source_id, &new_name);
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
                    let source_name = self
                        .discover
                        .sources
                        .iter()
                        .find(|s| s.id == source_id)
                        .map(|s| s.name.clone());

                    self.delete_source(&source_id);

                    if let Some(name) = source_name {
                        self.discover.status_message =
                            Some((format!("Deleted source '{}'", name), false));
                    }
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
    /// Focus cycles: Pattern → Excludes → Tag → Extractions → Options → Suggestions → FileList
    fn handle_rule_builder_key(&mut self, key: KeyEvent) {
        use extraction::RuleBuilderFocus;

        // Capture the current pattern before handling the key
        let pattern_before = self
            .discover
            .rule_builder
            .as_ref()
            .map(|b| b.pattern.clone())
            .unwrap_or_default();
        let active_workspace_id = self.active_workspace_id();
        let control_connected = self.control_connected;
        let db_read_only = self.db_read_only;
        let standalone_writer = self.config.standalone_writer;
        let mutations_blocked = !control_connected && (db_read_only || !standalone_writer);
        let blocked_message = |action: &str| {
            if db_read_only {
                format!("Database is read-only; cannot {}", action)
            } else {
                format!("Sentinel not reachable; cannot {}", action)
            }
        };

        let builder = match self.discover.rule_builder.as_mut() {
            Some(b) => b,
            None => {
                // No builder state - should not happen, return to Files
                self.transition_discover_state(DiscoverViewState::Files);
                return;
            }
        };
        let mut refresh_needed = false;
        let mut refresh_pattern: Option<String> = None;
        let mut switch_to_label = false;
        let mut skip_main_handling = false;
        let mut pattern_after = builder.pattern.clone();
        let mut pattern_debounced = builder.pattern_changed_at.is_some();
        let mut pending_manual_tag: Option<(Vec<String>, String, bool)> = None;
        let mut pending_sample_eval = false;
        let mut pending_full_eval = false;
        let mut return_after_action = false;

        if builder.candidate_preview_open {
            skip_main_handling = true;
            match key.code {
                KeyCode::Esc => {
                    builder.candidate_preview_open = false;
                }
                KeyCode::Char('a') | KeyCode::Enter => {
                    if let Some(candidate) =
                        builder.rule_candidates.get(builder.selected_candidate).cloned()
                    {
                        builder.pattern = candidate.custom_pattern.clone();
                        builder.extractions = candidate.fields.clone();
                        builder.pattern_error = None;
                        builder.dirty = true;
                        builder.focus = RuleBuilderFocus::Tag;
                        builder.candidate_preview_open = false;
                        builder.selected_preview_files.clear();
                        builder.pattern_changed_at = Some(std::time::Instant::now());
                        refresh_pattern = Some(candidate.custom_pattern);
                        switch_to_label = true;
                    } else {
                        builder.candidate_preview_open = false;
                    }
                }
                _ => {}
            }
            pattern_after = builder.pattern.clone();
            pattern_debounced = builder.pattern_changed_at.is_some();
        }

        if !skip_main_handling {
            if builder.confirm_exit_open {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        builder.confirm_exit_open = false;
                        builder.dirty = false;
                        self.set_mode(TuiMode::Home);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        builder.confirm_exit_open = false;
                    }
                    _ => {}
                }
                return;
            }

            if builder.manual_tag_confirm_open {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let (tag, paths) = {
                            builder.manual_tag_confirm_open = false;
                            (
                                builder.tag.clone(),
                                Self::rule_builder_preview_paths_from(builder, false),
                            )
                        };
                        if !tag.is_empty() && !paths.is_empty() {
                            pending_manual_tag = Some((paths, tag, true));
                        }
                        return_after_action = true;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        builder.manual_tag_confirm_open = false;
                        return_after_action = true;
                    }
                    _ => {}
                }
                skip_main_handling = true;
            }

            if !skip_main_handling {
                if builder.source_id.is_none() {
                    match key.code {
                        KeyCode::Esc => {
                            self.set_mode(TuiMode::Home);
                        }
                        KeyCode::Char('S') => {
                            self.transition_discover_state(DiscoverViewState::SourcesDropdown);
                            self.discover.sources_filter.clear();
                            self.discover.sources_filtering = false;
                            self.discover.preview_source =
                                Some(self.discover.selected_source_index());
                        }
                        KeyCode::Char('s') => {
                            self.transition_discover_state(DiscoverViewState::EnteringPath);
                            self.discover.scan_path_input.clear();
                            self.discover.scan_error = None;
                        }
                        _ => {
                            self.discover.status_message = Some((
                                "Select a source before building rules".to_string(),
                                true,
                            ));
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
                    RuleBuilderFocus::Options => {
                        if self.ingest_tab == IngestTab::Select {
                            RuleBuilderFocus::Suggestions
                        } else {
                            RuleBuilderFocus::FileList
                        }
                    }
                    RuleBuilderFocus::Suggestions => RuleBuilderFocus::FileList,
                    RuleBuilderFocus::FileList => RuleBuilderFocus::Pattern,
                    RuleBuilderFocus::IgnorePicker => RuleBuilderFocus::FileList,
                };
            }

            // BackTab (Shift+Tab) cycles focus in reverse
            KeyCode::BackTab => {
                builder.focus = match builder.focus {
                    RuleBuilderFocus::Pattern => RuleBuilderFocus::FileList,
                    RuleBuilderFocus::Excludes => RuleBuilderFocus::Pattern,
                    RuleBuilderFocus::ExcludeInput => RuleBuilderFocus::Excludes,
                    RuleBuilderFocus::Tag => RuleBuilderFocus::Excludes,
                    RuleBuilderFocus::Extractions => RuleBuilderFocus::Tag,
                    RuleBuilderFocus::ExtractionEdit(_) => RuleBuilderFocus::Extractions,
                    RuleBuilderFocus::Options => RuleBuilderFocus::Extractions,
                    RuleBuilderFocus::Suggestions => RuleBuilderFocus::Options,
                    RuleBuilderFocus::FileList => {
                        if self.ingest_tab == IngestTab::Select {
                            RuleBuilderFocus::Suggestions
                        } else {
                            RuleBuilderFocus::Options
                        }
                    }
                    RuleBuilderFocus::IgnorePicker => RuleBuilderFocus::FileList,
                };
            }

            // Escape cancels nested state or exits Rule Builder from FileList
            KeyCode::Esc => {
                match builder.focus {
                    RuleBuilderFocus::FileList => {
                        if builder.dirty {
                            builder.confirm_exit_open = true;
                        } else {
                            self.set_mode(TuiMode::Home);
                        }
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
                    RuleBuilderFocus::Suggestions => {
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
                            extraction::FileResultsState::Exploration {
                                folder_matches,
                                ..
                            } => {
                                if let Some(folder) = folder_matches.get(builder.selected_file) {
                                    let folder_path = folder.path.trim_end_matches('/');
                                    if !folder_path.is_empty()
                                        && folder_path != "."
                                        && folder_path != "./"
                                    {
                                        let pattern_suffix = if builder.pattern.starts_with("**/")
                                        {
                                            builder.pattern.trim_start_matches("**/").to_string()
                                        } else if builder.pattern.starts_with("./") {
                                            builder.pattern.trim_start_matches("./").to_string()
                                        } else {
                                            builder.pattern.clone()
                                        };
                                        let new_pattern =
                                            format!("{}/{}", folder_path, pattern_suffix);
                                        builder.pattern = new_pattern;
                                        builder.pattern_changed_at =
                                            Some(std::time::Instant::now());
                                        builder.dirty = true;
                                        builder.selected_file = 0;
                                        refresh_needed = true;
                                    }
                                }
                            }
                            extraction::FileResultsState::ExtractionPreview { .. } => {
                                // Could show file details or do nothing
                            }
                            extraction::FileResultsState::BacktestResults { .. } => {
                                // Could show error details for failed files
                            }
                        }
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        // Add exclude pattern
                        let pattern = builder.exclude_input.trim().to_string();
                        if !pattern.is_empty() {
                            builder.add_exclude(pattern);
                            builder.dirty = true;
                            builder.pattern_changed_at = Some(std::time::Instant::now());
                            refresh_needed = true;
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
                            builder.dirty = true;
                            builder.pattern_changed_at = Some(std::time::Instant::now());
                            refresh_needed = true;
                        }
                        builder.ignore_options.clear();
                        builder.focus = RuleBuilderFocus::FileList;
                    }
                    RuleBuilderFocus::Suggestions => {
                        if !builder.rule_candidates.is_empty() {
                            builder.candidate_preview_open = true;
                        }
                    }
                    _ => {}
                }
            }

            // Ctrl+S: Save rule
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if mutations_blocked {
                    let message = blocked_message("save rules");
                    self.discover.scan_error = Some(message.clone());
                    self.discover.status_message = Some((message, true));
                    return;
                }
                let workspace_id = match active_workspace_id {
                    Some(id) => id,
                    None => {
                        self.discover.status_message =
                            Some(("No workspace selected; cannot save rule".to_string(), true));
                        return;
                    }
                };
                if builder.can_save() {
                    let mut pattern_to_save = builder.pattern.clone();
                    if builder.pattern.contains('<') && builder.pattern.contains('>') {
                        match extraction::parse_custom_glob(&builder.pattern) {
                            Ok(parsed) => {
                                pattern_to_save = parsed.glob_pattern;
                            }
                            Err(err) => {
                                self.discover.status_message = Some((
                                    format!("Cannot save: {}", err.message),
                                    true,
                                ));
                                return;
                            }
                        }
                    }
                    if builder.source_id.is_some() {
                        let rule_id = TaggingRuleId::new();
                        self.discover.pending_rule_writes.push(PendingRuleWrite {
                            id: rule_id,
                            workspace_id,
                            pattern: pattern_to_save.clone(),
                            tag: builder.tag.clone(),
                        });
                        if pattern_to_save != builder.pattern {
                            self.discover.status_message = Some((
                                format!(
                                    "Saved rule using glob: {}",
                                    pattern_to_save
                                ),
                                false,
                            ));
                        } else {
                            self.discover.status_message =
                                Some((format!("Rule '{}' saved", builder.tag), false));
                        }
                        builder.dirty = false;
                        // Stay in Rule Builder (it's the default view) - clear for next rule
                        builder.pattern = "**/*".to_string();
                        builder.tag.clear();
                        builder.excludes.clear();
                        builder.focus = RuleBuilderFocus::Pattern;
                    } else {
                        self.discover.status_message =
                            Some(("Cannot save: no source selected".to_string(), true));
                    }
                } else {
                    self.discover.status_message = Some((
                        "Cannot save: pattern and tag are required".to_string(),
                        true,
                    ));
                }
            }

            // Ctrl+N: Clear form (new rule)
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                builder.pattern = "**/*".to_string();
                builder.pattern_error = None;
                builder.excludes.clear();
                builder.exclude_input.clear();
                builder.tag.clear();
                builder.extractions.clear();
                builder.editing_rule_id = None;
                builder.selected_preview_files.clear();
                builder.manual_tag_confirm_open = false;
                builder.manual_tag_confirm_count = 0;
                builder.dirty = false;
                builder.focus = RuleBuilderFocus::Pattern;
                refresh_needed = true;
                self.discover.status_message = Some(("Cleared rule builder".to_string(), false));
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
                            extraction::FileResultsState::Exploration {
                                folder_matches,
                                ..
                            } => folder_matches.len().saturating_sub(1),
                            extraction::FileResultsState::ExtractionPreview {
                                preview_files,
                            } => preview_files.len().saturating_sub(1),
                            extraction::FileResultsState::BacktestResults {
                                visible_indices,
                                ..
                            } => visible_indices.len().saturating_sub(1),
                        };
                        if max_index > 0 {
                            builder.selected_file = (builder.selected_file + 1).min(max_index);
                        }
                    }
                    RuleBuilderFocus::Excludes => {
                        if builder.excludes.is_empty() {
                            builder.focus = RuleBuilderFocus::Tag;
                        } else if builder.selected_exclude + 1 >= builder.excludes.len() {
                            builder.focus = RuleBuilderFocus::Tag;
                        } else {
                            builder.selected_exclude = (builder.selected_exclude + 1)
                                .min(builder.excludes.len().saturating_sub(1));
                        }
                    }
                    RuleBuilderFocus::Extractions => {
                        if builder.extractions.is_empty() {
                            builder.focus = RuleBuilderFocus::Options;
                        } else if builder.selected_extraction + 1 >= builder.extractions.len() {
                            builder.focus = RuleBuilderFocus::Options;
                        } else {
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
                    RuleBuilderFocus::Options => {
                        builder.focus = RuleBuilderFocus::Suggestions;
                    }
                    RuleBuilderFocus::Suggestions => {
                        if !builder.rule_candidates.is_empty() {
                            builder.selected_candidate = (builder.selected_candidate + 1)
                                .min(builder.rule_candidates.len().saturating_sub(1));
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
                            builder.selected_extraction =
                                builder.selected_extraction.saturating_sub(1);
                        }
                    }
                    RuleBuilderFocus::Options => {
                        builder.focus = RuleBuilderFocus::Extractions;
                    }
                    RuleBuilderFocus::Suggestions => {
                        builder.selected_candidate = builder.selected_candidate.saturating_sub(1);
                    }
                    RuleBuilderFocus::IgnorePicker => {
                        builder.ignore_selected = builder.ignore_selected.saturating_sub(1);
                    }
                    _ => {}
                }
            }

            // Delete exclude with 'd' or 'x'
            KeyCode::Char('d') | KeyCode::Char('x')
                if builder.focus == RuleBuilderFocus::Excludes =>
            {
                builder.remove_exclude(builder.selected_exclude);
                builder.dirty = true;
                builder.pattern_changed_at = Some(std::time::Instant::now());
                refresh_needed = true;
            }

            // Apply suggested rule (from list)
            KeyCode::Char('a') if builder.focus == RuleBuilderFocus::Suggestions => {
                if let Some(candidate) = builder.rule_candidates.get(builder.selected_candidate).cloned() {
                    builder.pattern = candidate.custom_pattern.clone();
                    builder.extractions = candidate.fields.clone();
                    builder.pattern_error = None;
                    builder.dirty = true;
                    builder.focus = RuleBuilderFocus::Tag;
                    builder.pattern_changed_at = Some(std::time::Instant::now());
                    builder.selected_preview_files.clear();
                    refresh_pattern = Some(candidate.custom_pattern);
                    switch_to_label = true;
                }
            }

            // Filter toggle in FileList (only in BacktestResults phase)
            KeyCode::Char('a') if builder.focus == RuleBuilderFocus::FileList => {
                if let extraction::FileResultsState::BacktestResults {
                    result_filter, ..
                } = &mut builder.file_results
                {
                    *result_filter = extraction::ResultFilter::All;
                    builder.update_visible();
                }
            }
            KeyCode::Char('p') if builder.focus == RuleBuilderFocus::FileList => {
                if let extraction::FileResultsState::BacktestResults {
                    result_filter, ..
                } = &mut builder.file_results
                {
                    *result_filter = extraction::ResultFilter::PassOnly;
                    builder.update_visible();
                }
            }
            KeyCode::Char('f') if builder.focus == RuleBuilderFocus::FileList => {
                if let extraction::FileResultsState::BacktestResults {
                    result_filter, ..
                } = &mut builder.file_results
                {
                    *result_filter = extraction::ResultFilter::FailOnly;
                    builder.update_visible();
                }
            }

            // Suggested rule list navigation
            KeyCode::Char('j') if builder.focus == RuleBuilderFocus::Suggestions => {
                if !builder.rule_candidates.is_empty() {
                    builder.selected_candidate = (builder.selected_candidate + 1)
                        .min(builder.rule_candidates.len().saturating_sub(1));
                }
            }
            KeyCode::Char('k') if builder.focus == RuleBuilderFocus::Suggestions => {
                builder.selected_candidate = builder.selected_candidate.saturating_sub(1);
            }

            // 'b' backtest stub removed (no-op with guidance)
            KeyCode::Char('b')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                        | RuleBuilderFocus::ExtractionEdit(_)
                ) =>
            {
                self.discover.status_message =
                    Some(("Backtest is not available yet".to_string(), true));
            }

            // Space toggles selection in preview/results list
            KeyCode::Char(' ') if builder.focus == RuleBuilderFocus::FileList => {
                if let Some(path) = Self::rule_builder_selected_rel_path_from(builder) {
                    if builder.selected_preview_files.contains(&path) {
                        builder.selected_preview_files.remove(&path);
                    } else {
                        builder.selected_preview_files.insert(path);
                    }
                }
            }

            // Left/Right arrows for panel navigation (move between left panel and FileList)
            // When not in text input mode, arrows provide quick panel switching
            KeyCode::Left
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                ) =>
            {
                if matches!(builder.focus, RuleBuilderFocus::FileList) {
                    // Move from FileList to Pattern (left panel)
                    builder.focus = RuleBuilderFocus::Pattern;
                }
            }
            KeyCode::Right
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                ) =>
            {
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::FileList | RuleBuilderFocus::IgnorePicker
                ) {
                    // Move from left panel to FileList (right panel)
                    builder.focus = RuleBuilderFocus::FileList;
                }
            }
            // Quick pane jump with [ and ]
            KeyCode::Char('[')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                        | RuleBuilderFocus::ExtractionEdit(_)
                ) =>
            {
                builder.focus = RuleBuilderFocus::Pattern;
            }
            KeyCode::Char(']')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                        | RuleBuilderFocus::ExtractionEdit(_)
                ) =>
            {
                builder.focus = RuleBuilderFocus::FileList;
            }

            // 't' applies manual tag to preview (selection-aware)
            KeyCode::Char('t') if builder.focus == RuleBuilderFocus::FileList => {
                if mutations_blocked {
                    let message = blocked_message("apply tags");
                    self.discover.scan_error = Some(message.clone());
                    self.discover.status_message = Some((message, true));
                    return;
                }
                if builder.tag.trim().is_empty() {
                    self.discover.status_message =
                        Some(("Enter a tag before applying".to_string(), true));
                    return;
                }
                let selected = Self::rule_builder_preview_paths_from(builder, true);
                let total = if selected.is_empty() {
                    Self::rule_builder_preview_paths_from(builder, false).len()
                } else {
                    0
                };
                let tag = builder.tag.clone();
                if selected.is_empty() {
                    if total == 0 {
                        self.discover.status_message =
                            Some(("No preview results to tag".to_string(), true));
                        return;
                    }
                    if let Some(builder) = self.discover.rule_builder.as_mut() {
                        builder.manual_tag_confirm_open = true;
                        builder.manual_tag_confirm_count = total;
                    }
                    return;
                }
                pending_manual_tag = Some((selected, tag, true));
            }

            // 's' opens scan dialog (when not in text input)
            KeyCode::Char('s')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                ) && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.transition_discover_state(DiscoverViewState::EnteringPath);
                // Pre-fill with selected source path if available
                self.discover.scan_path_input = self
                    .discover
                    .selected_source()
                    .map(|s| s.path.display().to_string())
                    .unwrap_or_default();
                self.discover.scan_error = None;
                return; // Exit early
            }

            // 'e' runs quick sample schema evaluation (RULE_BUILDER_UI_PLAN.md)
            // Structure-aware sampling: buckets by prefix, N per bucket, capped at 200
            KeyCode::Char('e')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                ) =>
            {
                pending_sample_eval = true;
            }

            // 'E' (shift+e) triggers full evaluation as background job (RULE_BUILDER_UI_PLAN.md)
            // Runs complete schema analysis on ALL matched files with progress tracking
            KeyCode::Char('E')
                if !matches!(
                    builder.focus,
                    RuleBuilderFocus::Pattern
                        | RuleBuilderFocus::Tag
                        | RuleBuilderFocus::ExcludeInput
                ) =>
            {
                pending_full_eval = true;
            }

            // Text input for Pattern, Tag, and ExcludeInput
            KeyCode::Char(c) => {
                match builder.focus {
                    RuleBuilderFocus::Pattern => {
                        builder.pattern.push(c);
                        builder.dirty = true;
                        builder.pattern_changed_at = Some(std::time::Instant::now());
                        // Validate pattern
                        match extraction::parse_custom_glob(&builder.pattern) {
                            Ok(_) => builder.pattern_error = None,
                            Err(e) => builder.pattern_error = Some(e.message),
                        }
                    }
                    RuleBuilderFocus::Tag => {
                        builder.tag.push(c);
                        builder.dirty = true;
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.exclude_input.push(c);
                        builder.dirty = true;
                    }
                    _ => {}
                }
            }

            // Backspace for text input
            KeyCode::Backspace => {
                match builder.focus {
                    RuleBuilderFocus::Pattern => {
                        builder.pattern.pop();
                        builder.dirty = true;
                        builder.pattern_changed_at = Some(std::time::Instant::now());
                        // Re-validate pattern
                        if builder.pattern.is_empty() {
                            builder.pattern_error = None;
                        } else {
                            match extraction::parse_custom_glob(&builder.pattern) {
                                Ok(_) => builder.pattern_error = None,
                                Err(e) => builder.pattern_error = Some(e.message),
                            }
                        }
                    }
                    RuleBuilderFocus::Tag => {
                        builder.tag.pop();
                        builder.dirty = true;
                    }
                    RuleBuilderFocus::ExcludeInput => {
                        builder.exclude_input.pop();
                        builder.dirty = true;
                    }
                    _ => {}
                }
            }

            _ => {}
        }
            }
            pattern_after = builder.pattern.clone();
            pattern_debounced = builder.pattern_changed_at.is_some();
        }

        let _ = builder;

        if let Some((paths, tag, clear_selection)) = pending_manual_tag.take() {
            let requested = self.apply_manual_tag_to_paths(&paths, &tag);
            if requested > 0 && clear_selection {
                if let Some(builder) = self.discover.rule_builder.as_mut() {
                    builder.selected_preview_files.clear();
                }
            }
        }

        if pending_sample_eval {
            self.run_sample_schema_eval();
        }
        if pending_full_eval {
            self.start_full_schema_eval();
        }

        if return_after_action {
            return;
        }

        if let Some(pattern) = refresh_pattern {
            if switch_to_label {
                self.set_ingest_tab(IngestTab::Rules);
            }
            self.update_rule_builder_files(&pattern);
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.pattern_changed_at = None;
            }
            return;
        }

        if skip_main_handling {
            return;
        }

        // If pattern changed, update matched files
        let mut needs_refresh = refresh_needed || (pattern_after != pattern_before && !pattern_debounced);
        if needs_refresh {
            self.update_rule_builder_files(&pattern_after);
            if let Some(builder) = self.discover.rule_builder.as_mut() {
                builder.pattern_changed_at = None;
            }
        }
    }

}

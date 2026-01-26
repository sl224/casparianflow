use super::{App, DiscoverViewState, SourceId, TuiMode};
use crate::cli::tui::backend::BackendRouter;
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Start a scan job for the given source (called from Home)
    pub(super) fn start_scan_for_source(&mut self, source_id: SourceId) {
        if self.mutations_blocked() {
            let message = BackendRouter::new(
                self.control_addr.clone(),
                self.config.standalone_writer,
                self.db_read_only,
            )
            .blocked_message("start scan");
            self.set_global_status_for(message, true, std::time::Duration::from_secs(8));
            return;
        }
        let path = self
            .discover
            .sources
            .iter()
            .find(|s| s.id == source_id)
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
    pub(super) fn handle_sources_key(&mut self, key: KeyEvent) {
        let source_count = self.discover.sources.len();

        // Handle delete confirmation first
        if self.sources_state.confirm_delete {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Some(source_id) = self
                        .discover
                        .sources
                        .get(self.sources_state.selected_index)
                        .map(|s| s.id.clone())
                    {
                        self.delete_source(&source_id);
                    }
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
                    self.sources_state.creating = false;
                    self.sources_state.edit_value.clear();
                }
                KeyCode::Enter => {
                    let value = self.sources_state.edit_value.trim().to_string();
                    if !value.is_empty() {
                        if self.sources_state.creating {
                            self.create_source(&value, "");
                        } else if let Some(source_id) = self
                            .discover
                            .sources
                            .get(self.sources_state.selected_index)
                            .map(|s| s.id.clone())
                        {
                            self.update_source_path(&source_id, &value);
                        }
                    }
                    self.sources_state.editing = false;
                    self.sources_state.creating = false;
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
                if source_count > 0
                    && self.sources_state.selected_index < source_count.saturating_sub(1)
                {
                    self.sources_state.selected_index += 1;
                }
            }
            // n: New source
            KeyCode::Char('n') => {
                self.sources_state.editing = true;
                self.sources_state.creating = true;
                self.sources_state.edit_value.clear();
            }
            // e: Edit source
            KeyCode::Char('e') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    self.sources_state.editing = true;
                    self.sources_state.creating = false;
                    let source = &self.discover.sources[self.sources_state.selected_index];
                    self.sources_state.edit_value = source.path.display().to_string();
                }
            }
            // r: Rescan source
            KeyCode::Char('r') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    // Clone the source_id to avoid borrow conflict
                    let source_id = self.discover.sources[self.sources_state.selected_index].id;
                    self.start_scan_for_source(source_id);
                }
            }
            // d: Delete source (with confirmation)
            KeyCode::Char('d') => {
                if source_count > 0 && self.sources_state.selected_index < source_count {
                    self.sources_state.confirm_delete = true;
                }
            }
            KeyCode::Esc => {
                if let Some(prev_mode) = self.sources_state.previous_mode.take() {
                    self.set_mode(prev_mode);
                } else {
                    self.set_mode(TuiMode::Home);
                }
            }
            _ => {}
        }
    }
}

use super::*;
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    pub(super) fn handle_settings_key(&mut self, key: KeyEvent) {
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

}

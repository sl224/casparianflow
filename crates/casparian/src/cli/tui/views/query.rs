use super::*;
use casparian_db::DbConnection;
use chrono::Local;
use crate::cli::config::{casparian_home, query_catalog_path};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc;

impl App {
    // ======== Query Mode Key Handler ========

    /// Handle Query mode key events
    pub(super) fn handle_query_key(&mut self, key: KeyEvent) {
        match self.query_state.view_state {
            QueryViewState::Editing => self.handle_query_editing_key(key),
            QueryViewState::Executing => {
                // Esc detaches (query keeps running in background)
                if key.code == KeyCode::Esc {
                    self.query_state.view_state = QueryViewState::Editing;
                }
            }
            QueryViewState::ViewingResults => self.handle_query_results_key(key),
            QueryViewState::TableBrowser => self.handle_query_table_browser_key(key),
            QueryViewState::SavedQueries => self.handle_query_saved_queries_key(key),
        }
    }

    /// Handle keys when in query editing mode
    fn handle_query_editing_key(&mut self, key: KeyEvent) {
        if self.query_state.status_message.is_some() {
            self.query_state.status_message = None;
        }
        match key.code {
            // Ctrl+Enter = execute query
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.execute_query();
            }
            // Ctrl+T = open table browser
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.load_table_browser();
                self.query_state.view_state = QueryViewState::TableBrowser;
            }
            // Ctrl+S = save query
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_query_to_disk();
            }
            // Ctrl+O = open saved query list
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.load_saved_queries();
                self.query_state.view_state = QueryViewState::SavedQueries;
            }
            // Ctrl+L = clear input
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query_state.sql_input.clear();
                self.query_state.cursor_position = 0;
                self.query_state.history_index = None;
                self.query_state.draft_input = None;
            }
            // Up arrow = history navigation (when not in middle of text)
            KeyCode::Up
                if self.query_state.cursor_position == 0
                    || self.query_state.sql_input.is_empty() =>
            {
                self.query_state.history_prev();
            }
            // Down arrow = history navigation (when browsing history)
            KeyCode::Down if self.query_state.history_index.is_some() => {
                self.query_state.history_next();
            }
            // Tab = toggle focus to results (if results exist)
            KeyCode::Tab if self.query_state.results.is_some() => {
                self.query_state.view_state = QueryViewState::ViewingResults;
            }
            // Esc = clear results/error or do nothing
            KeyCode::Esc => {
                if self.query_state.error.is_some() {
                    self.query_state.error = None;
                } else if self.query_state.results.is_some() {
                    self.query_state.results = None;
                }
            }
            // Regular text input
            KeyCode::Char(c) => {
                self.query_state
                    .sql_input
                    .insert(self.query_state.cursor_position, c);
                self.query_state.cursor_position += 1;
                self.query_state.history_index = None;
            }
            KeyCode::Backspace if self.query_state.cursor_position > 0 => {
                self.query_state.cursor_position -= 1;
                self.query_state
                    .sql_input
                    .remove(self.query_state.cursor_position);
                self.query_state.history_index = None;
            }
            KeyCode::Delete
                if self.query_state.cursor_position < self.query_state.sql_input.len() =>
            {
                self.query_state
                    .sql_input
                    .remove(self.query_state.cursor_position);
                self.query_state.history_index = None;
            }
            KeyCode::Left if self.query_state.cursor_position > 0 => {
                self.query_state.cursor_position -= 1;
            }
            KeyCode::Right
                if self.query_state.cursor_position < self.query_state.sql_input.len() =>
            {
                self.query_state.cursor_position += 1;
            }
            KeyCode::Home => {
                self.query_state.cursor_position = 0;
            }
            KeyCode::End => {
                self.query_state.cursor_position = self.query_state.sql_input.len();
            }
            KeyCode::Enter => {
                // Regular Enter adds newline for multi-line queries
                self.query_state
                    .sql_input
                    .insert(self.query_state.cursor_position, '\n');
                self.query_state.cursor_position += 1;
                self.query_state.history_index = None;
            }
            _ => {}
        }
    }

    /// Handle keys when viewing query results
    fn handle_query_results_key(&mut self, key: KeyEvent) {
        match key.code {
            // Tab = toggle focus back to editor
            KeyCode::Tab | KeyCode::Esc => {
                self.query_state.view_state = QueryViewState::Editing;
            }
            // Up/Down = navigate result rows
            KeyCode::Up => {
                if let Some(ref mut results) = self.query_state.results {
                    if results.selected_row > 0 {
                        results.selected_row -= 1;
                    }
                }
            }
            KeyCode::Down => {
                if let Some(ref mut results) = self.query_state.results {
                    if results.selected_row + 1 < results.rows.len() {
                        results.selected_row += 1;
                    }
                }
            }
            // Left/Right = horizontal scroll for wide tables
            KeyCode::Left => {
                if let Some(ref mut results) = self.query_state.results {
                    if results.scroll_x > 0 {
                        results.scroll_x -= 1;
                    }
                }
            }
            KeyCode::Right => {
                if let Some(ref mut results) = self.query_state.results {
                    if results.scroll_x + 1 < results.columns.len() {
                        results.scroll_x += 1;
                    }
                }
            }
            // Page navigation
            KeyCode::PageUp => {
                if let Some(ref mut results) = self.query_state.results {
                    results.selected_row = results.selected_row.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(ref mut results) = self.query_state.results {
                    let max = results.rows.len().saturating_sub(1);
                    results.selected_row = (results.selected_row + 10).min(max);
                }
            }
            KeyCode::Home => {
                if let Some(ref mut results) = self.query_state.results {
                    results.selected_row = 0;
                }
            }
            KeyCode::End => {
                if let Some(ref mut results) = self.query_state.results {
                    results.selected_row = results.rows.len().saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    fn handle_query_table_browser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.query_state.view_state = QueryViewState::Editing;
            }
            KeyCode::Up => {
                if self.query_state.table_browser.selected_index > 0 {
                    self.query_state.table_browser.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.query_state.table_browser.selected_index + 1
                    < self.query_state.table_browser.tables.len()
                {
                    self.query_state.table_browser.selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(table) = self
                    .query_state
                    .table_browser
                    .tables
                    .get(self.query_state.table_browser.selected_index)
                {
                    self.query_state
                        .sql_input
                        .insert_str(self.query_state.cursor_position, table.insert_text.as_str());
                    self.query_state.cursor_position += table.insert_text.len();
                }
                self.query_state.view_state = QueryViewState::Editing;
            }
            _ => {}
        }
    }

    fn handle_query_saved_queries_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.query_state.view_state = QueryViewState::Editing;
            }
            KeyCode::Up => {
                if self.query_state.saved_queries.selected_index > 0 {
                    self.query_state.saved_queries.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.query_state.saved_queries.selected_index + 1
                    < self.query_state.saved_queries.entries.len()
                {
                    self.query_state.saved_queries.selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = self
                    .query_state
                    .saved_queries
                    .entries
                    .get(self.query_state.saved_queries.selected_index)
                {
                    match std::fs::read_to_string(&entry.path) {
                        Ok(sql) => {
                            self.query_state.sql_input = sql;
                            self.query_state.cursor_position = self.query_state.sql_input.len();
                            self.query_state.error = None;
                            self.query_state.results = None;
                            self.query_state.view_state = QueryViewState::Editing;
                        }
                        Err(err) => {
                            self.query_state.status_message = Some(format!("Load failed: {}", err));
                        }
                    }
                } else {
                    self.query_state.view_state = QueryViewState::Editing;
                }
            }
            _ => {}
        }
    }

    fn load_table_browser(&mut self) {
        self.query_state.table_browser.tables.clear();
        self.query_state.table_browser.selected_index = 0;
        self.query_state.table_browser.error = None;

        let db_path = query_catalog_path();
        if !db_path.exists() {
            self.query_state.table_browser.error = Some("Query catalog not found".to_string());
            return;
        }

        let conn = match DbConnection::open_duckdb_readonly(&db_path) {
            Ok(conn) => conn,
            Err(err) => {
                self.query_state.table_browser.error =
                    Some(format!("Query catalog open failed: {}", err));
                return;
            }
        };

        let rows = match conn.query_all(
            "SELECT table_schema, table_name FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY table_schema, table_name",
            &[],
        ) {
            Ok(rows) => rows,
            Err(err) => {
                self.query_state.table_browser.error = Some(format!("Table list failed: {}", err));
                return;
            }
        };

        for row in rows {
            let schema = match row.get::<String>(0) {
                Ok(schema) => schema,
                Err(_) => continue,
            };
            let name = match row.get::<String>(1) {
                Ok(name) => name,
                Err(_) => continue,
            };
            let insert_text = format!("{}.{}", quote_ident(&schema), quote_ident(&name));
            self.query_state.table_browser.tables.push(TableBrowserEntry {
                schema,
                name,
                insert_text,
            });
        }
        self.query_state.table_browser.loaded = true;
    }

    fn load_saved_queries(&mut self) {
        self.query_state.saved_queries.entries.clear();
        self.query_state.saved_queries.selected_index = 0;
        self.query_state.saved_queries.error = None;

        let dir = casparian_home().join("queries");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.query_state.saved_queries.error =
                Some(format!("Create queries dir failed: {}", err));
            return;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.query_state.saved_queries.error =
                    Some(format!("Read queries dir failed: {}", err));
                return;
            }
        };

        let mut list: Vec<SavedQueryEntry> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|e| e == "sql")
                    .unwrap_or(false)
            })
            .map(|entry| SavedQueryEntry {
                name: entry
                    .path()
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("query.sql")
                    .to_string(),
                path: entry.path(),
            })
            .collect();

        list.sort_by(|a, b| a.name.cmp(&b.name));
        self.query_state.saved_queries.entries = list;
        self.query_state.saved_queries.loaded = true;
    }

    fn save_query_to_disk(&mut self) {
        let sql = self.query_state.sql_input.trim();
        if sql.is_empty() {
            self.query_state.status_message = Some("Nothing to save".to_string());
            return;
        }

        let dir = casparian_home().join("queries");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.query_state.status_message = Some(format!("Save failed: {}", err));
            return;
        }

        let filename = format!("query_{}.sql", Local::now().format("%Y%m%d_%H%M%S"));
        let path = dir.join(&filename);
        if let Err(err) = std::fs::write(&path, sql) {
            self.query_state.status_message = Some(format!("Save failed: {}", err));
            return;
        }

        self.query_state.status_message = Some(format!("Saved {}", filename));
        self.query_state.saved_queries.loaded = false;
    }

    /// Execute the current SQL query
    fn execute_query(&mut self) {
        // Clone the SQL to avoid borrow issues
        let sql = self.query_state.sql_input.trim().to_string();
        if sql.is_empty() {
            return;
        }

        if self.query_state.executing {
            self.query_state.error =
                Some("Query already running. Press Esc to detach.".to_string());
            return;
        }

        self.query_state.clear_for_new_query();
        self.query_state.view_state = QueryViewState::Executing;
        self.query_state.executing = true;

        let db_path = query_catalog_path();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_query = Some(rx);

        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let result = match DbConnection::open_duckdb_readonly(&db_path) {
                Ok(conn) => App::run_query_with_conn(&conn, &sql),
                Err(err) => Err(format!("Query catalog open failed: {}", err)),
            };
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let _ = tx.send(QueryExecutionResult {
                sql,
                result,
                elapsed_ms,
            });
        });
    }

    /// Run a SQL query and return results
    fn run_query_with_conn(conn: &DbConnection, sql: &str) -> Result<QueryResults, String> {
        use casparian_db::DbValue;

        let rows_result = conn.query_all(sql, &[]);

        match rows_result {
            Ok(rows) => {
                // Extract column names from first row if available
                let columns: Vec<String> = if rows.is_empty() {
                    vec![]
                } else {
                    rows[0].column_names().to_vec()
                };

                const MAX_ROWS: usize = 1000;
                let truncated = rows.len() > MAX_ROWS;
                let row_count = rows.len();

                let result_rows: Vec<Vec<String>> = rows
                    .into_iter()
                    .take(MAX_ROWS)
                    .map(|row| {
                        (0..row.len())
                            .map(|i| {
                                // Convert DbValue to display string
                                match row.get_raw(i) {
                                    Some(DbValue::Null) => "NULL".to_string(),
                                    Some(DbValue::Integer(v)) => v.to_string(),
                                    Some(DbValue::Real(v)) => v.to_string(),
                                    Some(DbValue::Text(v)) => v.clone(),
                                    Some(DbValue::Boolean(v)) => v.to_string(),
                                    Some(DbValue::Blob(v)) => format!("<blob {} bytes>", v.len()),
                                    Some(DbValue::Timestamp(t)) => t.to_rfc3339(),
                                    None => "NULL".to_string(),
                                }
                            })
                            .collect()
                    })
                    .collect();

                Ok(QueryResults {
                    columns,
                    rows: result_rows,
                    row_count,
                    truncated,
                    selected_row: 0,
                    scroll_x: 0,
                })
            }
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

}

fn quote_ident(ident: &str) -> String {
    let escaped = ident.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

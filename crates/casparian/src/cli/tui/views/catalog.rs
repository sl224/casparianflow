use super::*;
use casparian_db::DbValue;
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::mpsc;

impl App {
    pub(super) fn start_catalog_load(&mut self) {
        if self.pending_catalog_load.is_some() {
            return;
        }

        let (backend, db_path) = self.resolve_db_target();
        if !db_path.exists() {
            self.catalog_state.loaded = true;
            return;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_catalog_load = Some(rx);

        std::thread::spawn(move || {
            let result: Result<CatalogData, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                let pipelines = if App::table_exists(&conn, "cf_pipelines")
                    .map_err(|err| format!("Catalog schema check failed: {}", err))?
                {
                    let mut rows_out = Vec::new();
                    let rows = conn
                        .query_all(
                            "SELECT id, name, version, created_at FROM cf_pipelines \
                             ORDER BY created_at DESC LIMIT 200",
                            &[],
                        )
                        .map_err(|err| format!("Pipelines query failed: {}", err))?;
                    for row in rows {
                        let id: String = row.get(0).map_err(|e| e.to_string())?;
                        let name: String = row.get(1).map_err(|e| e.to_string())?;
                        let version: i64 = row.get(2).map_err(|e| e.to_string())?;
                        let created_at: Option<String> = row.get(3).ok().flatten();
                        rows_out.push(PipelineInfo {
                            id,
                            name,
                            version,
                            created_at: created_at.unwrap_or_else(|| "-".to_string()),
                        });
                    }
                    Some(rows_out)
                } else {
                    None
                };

                let runs = if App::table_exists(&conn, "cf_pipeline_runs")
                    .map_err(|err| format!("Catalog schema check failed: {}", err))?
                {
                    let mut rows_out = Vec::new();
                    let rows = conn
                        .query_all(
                            "SELECT pr.id, pr.pipeline_id, p.name, p.version, pr.logical_date, \
                             pr.status, pr.selection_snapshot_hash, pr.started_at, pr.completed_at \
                             FROM cf_pipeline_runs pr \
                             LEFT JOIN cf_pipelines p ON p.id = pr.pipeline_id \
                             ORDER BY pr.created_at DESC LIMIT 200",
                            &[],
                        )
                        .map_err(|err| format!("Pipeline runs query failed: {}", err))?;
                    for row in rows {
                        let id: String = row.get(0).map_err(|e| e.to_string())?;
                        let pipeline_id: String = row.get(1).map_err(|e| e.to_string())?;
                        let pipeline_name: Option<String> = row.get(2).ok().flatten();
                        let pipeline_version: Option<i64> = row.get(3).ok().flatten();
                        let logical_date: String = row.get(4).map_err(|e| e.to_string())?;
                        let status: String = row.get(5).map_err(|e| e.to_string())?;
                        let selection_snapshot_hash: Option<String> = row.get(6).ok().flatten();
                        let started_at: Option<String> = row.get(7).ok().flatten();
                        let completed_at: Option<String> = row.get(8).ok().flatten();
                        rows_out.push(PipelineRunInfo {
                            id,
                            pipeline_id,
                            pipeline_name,
                            pipeline_version,
                            logical_date,
                            status,
                            selection_snapshot_hash,
                            started_at,
                            completed_at,
                        });
                    }
                    Some(rows_out)
                } else {
                    None
                };

                Ok(CatalogData { pipelines, runs })
            })();

            let _ = tx.send(result);
        });
    }
    pub(super) fn handle_catalog_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if let Some(prev_mode) = self.catalog_state.previous_mode.take() {
                    self.set_mode(prev_mode);
                } else {
                    self.set_mode(TuiMode::Home);
                }
            }
            KeyCode::Tab => {
                self.catalog_state.tab = self.catalog_state.tab.next();
                self.catalog_state.selected_index = 0;
                self.catalog_state.clamp_selection();
            }
            KeyCode::Down => {
                let len = self.catalog_state.active_len();
                if len > 0 && self.catalog_state.selected_index + 1 < len {
                    self.catalog_state.selected_index += 1;
                }
            }
            KeyCode::Up => {
                if self.catalog_state.selected_index > 0 {
                    self.catalog_state.selected_index -= 1;
                }
            }
            KeyCode::Char('r') => {
                self.catalog_state.loaded = false;
            }
            KeyCode::Enter => {
                if self.catalog_state.tab == CatalogTab::Pipelines {
                    self.catalog_state.tab = CatalogTab::Runs;
                    self.catalog_state.selected_index = 0;
                    self.catalog_state.clamp_selection();
                }
            }
            _ => {}
        }
    }

    pub(super) fn open_catalog(&mut self, run_id: Option<String>) {
        self.catalog_state.pending_select_run_id = run_id;
        self.catalog_state.tab = CatalogTab::Pipelines;
        if self.catalog_state.pending_select_run_id.is_some() {
            self.catalog_state.tab = CatalogTab::Runs;
        }
        self.catalog_state.selected_index = 0;
        self.catalog_state.loaded = false;
        self.catalog_state.status_message = None;
        self.set_run_tab(RunTab::Outputs);
    }

}

use super::*;
use casparian_db::DbValue;
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::mpsc;

impl App {
    pub(super) fn start_triage_load(&mut self) {
        if self.pending_triage_load.is_some() {
            return;
        }

        let (backend, db_path) = self.resolve_db_target();
        if !db_path.exists() {
            self.triage_state.loaded = true;
            return;
        }

        let job_filter = self.triage_state.job_filter;
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending_triage_load = Some(rx);

        std::thread::spawn(move || {
            let result: Result<TriageData, String> = (|| {
                let conn = match App::open_db_readonly_with(backend, &db_path) {
                    Ok(Some(conn)) => conn,
                    Ok(None) => return Err("Database not available".to_string()),
                    Err(err) => return Err(format!("Database open failed: {}", err)),
                };

                let quarantine_rows = if App::table_exists(&conn, "cf_quarantine")
                    .map_err(|err| format!("Triage schema check failed: {}", err))?
                {
                    let mut rows = Vec::new();
                    let (query, params) = if let Some(job_id) = job_filter {
                        (
                            "SELECT id, job_id, row_index, error_reason, raw_data, created_at \
                             FROM cf_quarantine WHERE job_id = ? ORDER BY created_at DESC LIMIT 500",
                            vec![DbValue::from(job_id)],
                        )
                    } else {
                        (
                            "SELECT id, job_id, row_index, error_reason, raw_data, created_at \
                             FROM cf_quarantine ORDER BY created_at DESC LIMIT 500",
                            Vec::new(),
                        )
                    };
                    let result_rows = conn
                        .query_all(query, &params)
                        .map_err(|err| format!("Quarantine query failed: {}", err))?;
                    for row in result_rows {
                        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
                        let job_id: i64 = row.get(1).map_err(|e| e.to_string())?;
                        let row_index: i64 = row.get(2).map_err(|e| e.to_string())?;
                        let error_reason: String = row.get(3).map_err(|e| e.to_string())?;
                        let raw_data: Option<Vec<u8>> = row.get(4).ok().flatten();
                        let created_at: Option<String> = row.get(5).ok().flatten();
                        rows.push(QuarantineRow {
                            id,
                            job_id,
                            row_index,
                            error_reason,
                            raw_data,
                            created_at: created_at.unwrap_or_else(|| "-".to_string()),
                        });
                    }
                    Some(rows)
                } else {
                    None
                };

                let schema_mismatches = if App::table_exists(&conn, "cf_job_schema_mismatch")
                    .map_err(|err| format!("Triage schema check failed: {}", err))?
                {
                    let mut rows = Vec::new();
                    let (query, params) = if let Some(job_id) = job_filter {
                        (
                            "SELECT id, job_id, output_name, mismatch_kind, expected_name, actual_name, \
                             expected_type, actual_type, expected_index, actual_index, created_at \
                             FROM cf_job_schema_mismatch WHERE job_id = ? ORDER BY created_at DESC LIMIT 500",
                            vec![DbValue::from(job_id)],
                        )
                    } else {
                        (
                            "SELECT id, job_id, output_name, mismatch_kind, expected_name, actual_name, \
                             expected_type, actual_type, expected_index, actual_index, created_at \
                             FROM cf_job_schema_mismatch ORDER BY created_at DESC LIMIT 500",
                            Vec::new(),
                        )
                    };
                    let result_rows = conn
                        .query_all(query, &params)
                        .map_err(|err| format!("Schema mismatch query failed: {}", err))?;
                    for row in result_rows {
                        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
                        let job_id: i64 = row.get(1).map_err(|e| e.to_string())?;
                        let output_name: String = row.get(2).map_err(|e| e.to_string())?;
                        let mismatch_kind: String = row.get(3).map_err(|e| e.to_string())?;
                        let expected_name: Option<String> = row.get(4).ok().flatten();
                        let actual_name: Option<String> = row.get(5).ok().flatten();
                        let expected_type: Option<String> = row.get(6).ok().flatten();
                        let actual_type: Option<String> = row.get(7).ok().flatten();
                        let expected_index: Option<i64> = row.get(8).ok().flatten();
                        let actual_index: Option<i64> = row.get(9).ok().flatten();
                        let created_at: Option<String> = row.get(10).ok().flatten();
                        rows.push(SchemaMismatchRow {
                            id,
                            job_id,
                            output_name,
                            mismatch_kind,
                            expected_name,
                            actual_name,
                            expected_type,
                            actual_type,
                            expected_index,
                            actual_index,
                            created_at: created_at.unwrap_or_else(|| "-".to_string()),
                        });
                    }
                    Some(rows)
                } else {
                    None
                };

                let dead_letters = if App::table_exists(&conn, "cf_dead_letter")
                    .map_err(|err| format!("Triage schema check failed: {}", err))?
                {
                    let mut rows = Vec::new();
                    let (query, params) = if let Some(job_id) = job_filter {
                        (
                            "SELECT id, original_job_id, file_id, plugin_name, error_message, retry_count, moved_at, reason \
                             FROM cf_dead_letter WHERE original_job_id = ? ORDER BY moved_at DESC LIMIT 500",
                            vec![DbValue::from(job_id)],
                        )
                    } else {
                        (
                            "SELECT id, original_job_id, file_id, plugin_name, error_message, retry_count, moved_at, reason \
                             FROM cf_dead_letter ORDER BY moved_at DESC LIMIT 500",
                            Vec::new(),
                        )
                    };
                    let result_rows = conn
                        .query_all(query, &params)
                        .map_err(|err| format!("Dead letter query failed: {}", err))?;
                    for row in result_rows {
                        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
                        let original_job_id: i64 = row.get(1).map_err(|e| e.to_string())?;
                        let file_id: Option<i64> = row.get(2).ok().flatten();
                        let plugin_name: String = row.get(3).map_err(|e| e.to_string())?;
                        let error_message: Option<String> = row.get(4).ok().flatten();
                        let retry_count: i64 = row.get(5).map_err(|e| e.to_string())?;
                        let moved_at: Option<String> = row.get(6).ok().flatten();
                        let reason: Option<String> = row.get(7).ok().flatten();
                        rows.push(DeadLetterRow {
                            id,
                            original_job_id,
                            file_id,
                            plugin_name,
                            error_message,
                            retry_count,
                            moved_at: moved_at.unwrap_or_else(|| "-".to_string()),
                            reason,
                        });
                    }
                    Some(rows)
                } else {
                    None
                };

                Ok(TriageData {
                    quarantine_rows,
                    schema_mismatches,
                    dead_letters,
                })
            })();

            let _ = tx.send(result);
        });
    }


    pub(super) fn handle_triage_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if let Some(prev_mode) = self.triage_state.previous_mode.take() {
                    self.set_mode(prev_mode);
                } else {
                    self.set_mode(TuiMode::Home);
                }
            }
            KeyCode::Tab => {
                self.triage_state.tab = self.triage_state.tab.next();
                self.triage_state.selected_index = 0;
                self.triage_state.clamp_selection();
            }
            KeyCode::Down => {
                let len = self.triage_state.active_len();
                if len > 0 && self.triage_state.selected_index + 1 < len {
                    self.triage_state.selected_index += 1;
                }
            }
            KeyCode::Up => {
                if self.triage_state.selected_index > 0 {
                    self.triage_state.selected_index -= 1;
                }
            }
            KeyCode::Char('r') => {
                self.triage_state.loaded = false;
            }
            KeyCode::Char('j') => {
                if let Some(job_id) = self.triage_selected_job_id() {
                    self.select_job_by_id(job_id);
                    self.set_run_tab(RunTab::Jobs);
                }
            }
            KeyCode::Char('y') => {
                if let Some(detail) = self.triage_selected_detail() {
                    self.triage_state.copied_buffer = Some(detail);
                    self.triage_state.status_message =
                        Some("Copied diagnostics to buffer".to_string());
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if self.triage_state.job_filter.is_some() {
                    self.triage_state.job_filter = None;
                    self.triage_state.loaded = false;
                    self.triage_state.selected_index = 0;
                }
            }
            _ => {}
        }
    }

    fn triage_selected_job_id(&self) -> Option<i64> {
        match self.triage_state.tab {
            TriageTab::Quarantine => self
                .triage_state
                .quarantine_rows
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| row.job_id),
            TriageTab::SchemaMismatch => self
                .triage_state
                .schema_mismatches
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| row.job_id),
            TriageTab::DeadLetter => self
                .triage_state
                .dead_letters
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| row.original_job_id),
        }
    }

    fn triage_selected_detail(&self) -> Option<String> {
        match self.triage_state.tab {
            TriageTab::Quarantine => self
                .triage_state
                .quarantine_rows
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| {
                    format!(
                        "Quarantine Row {}\nJob: {}\nRow: {}\nReason: {}\nCreated: {}\n",
                        row.id, row.job_id, row.row_index, row.error_reason, row.created_at
                    )
                }),
            TriageTab::SchemaMismatch => self
                .triage_state
                .schema_mismatches
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| {
                    format!(
                        "Schema Mismatch {}\nJob: {}\nOutput: {}\nKind: {}\nExpected: {:?} ({:?}) idx {:?}\nActual: {:?} ({:?}) idx {:?}\nCreated: {}\n",
                        row.id,
                        row.job_id,
                        row.output_name,
                        row.mismatch_kind,
                        row.expected_name,
                        row.expected_type,
                        row.expected_index,
                        row.actual_name,
                        row.actual_type,
                        row.actual_index,
                        row.created_at
                    )
                }),
            TriageTab::DeadLetter => self
                .triage_state
                .dead_letters
                .as_ref()
                .and_then(|rows| rows.get(self.triage_state.selected_index))
                .map(|row| {
                    format!(
                        "Dead Letter {}\nOriginal Job: {}\nFile: {:?}\nPlugin: {}\nError: {:?}\nRetry: {}\nMoved: {}\nReason: {:?}\n",
                        row.id,
                        row.original_job_id,
                        row.file_id,
                        row.plugin_name,
                        row.error_message,
                        row.retry_count,
                        row.moved_at,
                        row.reason
                    )
                }),
        }
    }

    fn select_job_by_id(&mut self, job_id: i64) {
        if self.jobs_state.jobs.is_empty() {
            return;
        }

        if let Some(job) = self.jobs_state.jobs.iter().find(|job| job.id == job_id) {
            self.jobs_state.section_focus = match job.status {
                JobStatus::Completed | JobStatus::PartialSuccess => JobsListSection::Ready,
                JobStatus::Pending
                | JobStatus::Running
                | JobStatus::Failed
                | JobStatus::Cancelled => JobsListSection::Actionable,
            };

            let list = match self.jobs_state.section_focus {
                JobsListSection::Actionable => self.jobs_state.actionable_jobs(),
                JobsListSection::Ready => self.jobs_state.ready_jobs(),
            };
            if let Some(pos) = list.iter().position(|job| job.id == job_id) {
                self.jobs_state.selected_index = pos;
                self.jobs_state.clamp_selection();
            }
        }
    }

    pub(super) fn open_triage(&mut self, job_filter: Option<i64>) {
        self.triage_state.job_filter = job_filter;
        self.triage_state.tab = TriageTab::Quarantine;
        self.triage_state.selected_index = 0;
        self.triage_state.loaded = false;
        self.triage_state.status_message = None;
        self.set_review_tab(ReviewTab::Triage);
    }
}

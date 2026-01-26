use super::{App, JobStatus, JobType, JobsListSection, JobsState, JobsViewState, MonitoringState};
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Handle jobs mode keys
    pub(super) fn handle_jobs_key(&mut self, key: KeyEvent) {
        // Handle keys based on current view state
        match self.jobs_state.view_state {
            JobsViewState::JobList => self.handle_jobs_list_key(key),
            JobsViewState::DetailPanel => self.handle_jobs_detail_key(key),
            JobsViewState::MonitoringPanel => self.handle_jobs_monitoring_key(key),
            JobsViewState::LogViewer => self.handle_jobs_log_viewer_key(key),
            JobsViewState::FilterDialog => self.handle_jobs_filter_dialog_key(key),
            JobsViewState::ViolationDetail => self.handle_jobs_violation_detail_key(key),
        }
    }

    /// Handle keys when in job list view
    fn handle_jobs_list_key(&mut self, key: KeyEvent) {
        let focused_count = self.jobs_state.focused_jobs().len();

        let sync_focus_index = |state: &mut JobsState| match state.section_focus {
            JobsListSection::Actionable => state.actionable_index = state.selected_index,
            JobsListSection::Ready => state.ready_index = state.selected_index,
        };

        match key.code {
            // Job navigation (within filtered list)
            KeyCode::Down => {
                if self.jobs_state.selected_index < focused_count.saturating_sub(1) {
                    self.jobs_state.selected_index += 1;
                    sync_focus_index(&mut self.jobs_state);
                }
            }
            KeyCode::Up => {
                if self.jobs_state.selected_index > 0 {
                    self.jobs_state.selected_index -= 1;
                    sync_focus_index(&mut self.jobs_state);
                }
            }
            // Pin details panel to selected job
            KeyCode::Enter => {
                let jobs = self.jobs_state.focused_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if self.jobs_state.pinned_job_id == Some(job.id) {
                        self.jobs_state.pinned_job_id = None;
                    } else {
                        self.jobs_state.pinned_job_id = Some(job.id);
                    }
                }
            }
            // Switch list focus
            KeyCode::Tab => {
                sync_focus_index(&mut self.jobs_state);
                self.jobs_state.section_focus = match self.jobs_state.section_focus {
                    JobsListSection::Actionable => JobsListSection::Ready,
                    JobsListSection::Ready => JobsListSection::Actionable,
                };
                self.jobs_state.clamp_selection();
            }
            // Toggle pipeline summary
            KeyCode::Char('p') => {
                self.jobs_state.show_pipeline = !self.jobs_state.show_pipeline;
            }
            // Open quarantine/triage view for selected job
            KeyCode::Char('Q') => {
                let jobs = self.jobs_state.focused_jobs();
                let job_id = jobs.get(self.jobs_state.selected_index).map(|job| job.id);
                self.open_triage(job_id);
            }
            // Open monitoring panel
            KeyCode::Char('m') => {
                self.jobs_state
                    .transition_state(JobsViewState::MonitoringPanel);
            }
            // Open pipeline catalog for selected run
            KeyCode::Char('C') => {
                let jobs = self.jobs_state.focused_jobs();
                let run_id = jobs
                    .get(self.jobs_state.selected_index)
                    .and_then(|job| job.pipeline_run_id.clone());
                self.open_catalog(run_id);
            }
            // f: Open filter dialog (per keybinding matrix - keys 1-4 are reserved for navigation)
            KeyCode::Char('f') => {
                self.jobs_state
                    .transition_state(JobsViewState::FilterDialog);
            }
            // Clear filters when active
            KeyCode::Backspace | KeyCode::Delete => {
                if self.jobs_state.status_filter.is_some() || self.jobs_state.type_filter.is_some()
                {
                    self.jobs_state.status_filter = None;
                    self.jobs_state.type_filter = None;
                    self.jobs_state.clamp_selection();
                }
            }
            // Go to first job
            KeyCode::Char('g') => {
                self.jobs_state.selected_index = 0;
                sync_focus_index(&mut self.jobs_state);
            }
            // Go to last job
            KeyCode::Char('G') => {
                self.jobs_state.selected_index = focused_count.saturating_sub(1);
                sync_focus_index(&mut self.jobs_state);
            }
            // Open output folder for completed jobs
            KeyCode::Char('o') | KeyCode::Char('O') => {
                let jobs = self.jobs_state.focused_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if let Some(ref path) = job.output_path {
                        // Try to open the folder in system file manager
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(path).spawn();
                        #[cfg(target_os = "linux")]
                        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("explorer").arg(path).spawn();
                    }
                }
            }
            // Clear completed jobs from the list
            KeyCode::Char('x') => {
                self.jobs_state.jobs.retain(|j| {
                    !matches!(j.status, JobStatus::Completed | JobStatus::PartialSuccess)
                });
                // Clamp selection to valid range
                self.jobs_state.clamp_selection();
            }
            // Show help overlay
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            // Open log viewer
            KeyCode::Char('L') => {
                if !self.jobs_state.focused_jobs().is_empty() {
                    self.jobs_state.transition_state(JobsViewState::LogViewer);
                }
            }
            // Copy output path to clipboard
            KeyCode::Char('y') => {
                let jobs = self.jobs_state.focused_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if let Some(ref path) = job.output_path {
                        // TODO: Copy to clipboard (requires clipboard crate or platform-specific impl)
                        let _ = path; // Silence warning for now
                    }
                }
            }
            // Toggle violation detail view (for backtest jobs)
            KeyCode::Char('v') => {
                let jobs = self.jobs_state.focused_jobs();
                if let Some(job) = jobs.get(self.jobs_state.selected_index) {
                    if job.job_type == JobType::Backtest && !job.violations.is_empty() {
                        self.jobs_state
                            .transition_state(JobsViewState::ViolationDetail);
                    }
                }
            }
            KeyCode::Esc => {}
            _ => {}
        }
    }

    /// Handle keys when in job detail panel
    fn handle_jobs_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close detail panel, return to list
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // Retry failed job from detail view
            KeyCode::Char('R') => {
                if let Some(job) = self.jobs_state.selected_job() {
                    if job.status == JobStatus::Failed {
                        // TODO: Actually retry the job
                    }
                }
            }
            // View logs (placeholder)
            KeyCode::Char('L') => {
                // TODO: Open log viewer
            }
            // Copy output path to clipboard (placeholder)
            KeyCode::Char('y') => {
                // TODO: Copy to clipboard
            }
            _ => {}
        }
    }

    /// Handle keys when in monitoring panel
    fn handle_jobs_monitoring_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close monitoring panel, return to list
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // Pause/resume monitoring refresh
            KeyCode::Char('p') => {
                self.jobs_state.monitoring.paused = !self.jobs_state.monitoring.paused;
            }
            // Reset metrics
            KeyCode::Char('r') => {
                self.jobs_state.monitoring = MonitoringState::default();
            }
            _ => {}
        }
    }

    /// Handle keys when in log viewer
    fn handle_jobs_log_viewer_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close log viewer, return to previous state
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // TODO: Scroll logs up/down
            KeyCode::Down => {
                // Scroll down
            }
            KeyCode::Up => {
                // Scroll up
            }
            _ => {}
        }
    }

    /// Handle keys when in filter dialog
    fn handle_jobs_filter_dialog_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close filter dialog
            KeyCode::Esc => {
                self.jobs_state.return_to_previous_state();
            }
            // TODO: Apply filter selections
            KeyCode::Enter => {
                self.jobs_state.return_to_previous_state();
            }
            _ => {}
        }
    }

    /// Handle keys when in violation detail view
    fn handle_jobs_violation_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            // Close violation detail view, return to job list
            KeyCode::Esc | KeyCode::Char('v') => {
                self.jobs_state.return_to_previous_state();
            }
            // Navigate violations
            KeyCode::Down => {
                if let Some(job_id) = self.jobs_state.selected_job().map(|j| j.id) {
                    if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
                        if job.selected_violation_index < job.violations.len().saturating_sub(1) {
                            job.selected_violation_index += 1;
                        }
                    }
                }
            }
            KeyCode::Up => {
                if let Some(job_id) = self.jobs_state.selected_job().map(|j| j.id) {
                    if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
                        if job.selected_violation_index > 0 {
                            job.selected_violation_index -= 1;
                        }
                    }
                }
            }
            // Apply suggested fix (creates approval request)
            KeyCode::Char('a') => {
                if let Some(job) = self.jobs_state.selected_job() {
                    if let Some(violation) = job.violations.get(job.selected_violation_index) {
                        if violation.suggested_fix.is_some() {
                            // TODO: Create approval request for the suggested fix
                            // This would integrate with the approval workflow
                            // For now, just log that we want to apply the fix
                            let _ = (job.id, job.selected_violation_index);
                        }
                    }
                }
            }
            // Go to first violation
            KeyCode::Char('g') => {
                if let Some(job_id) = self.jobs_state.selected_job().map(|j| j.id) {
                    if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
                        job.selected_violation_index = 0;
                    }
                }
            }
            // Go to last violation
            KeyCode::Char('G') => {
                if let Some(job_id) = self.jobs_state.selected_job().map(|j| j.id) {
                    if let Some(job) = self.jobs_state.jobs.iter_mut().find(|j| j.id == job_id) {
                        job.selected_violation_index = job.violations.len().saturating_sub(1);
                    }
                }
            }
            // Show help
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            _ => {}
        }
    }
}

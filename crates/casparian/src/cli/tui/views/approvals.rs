use super::{App, ApprovalAction, ApprovalDisplayStatus, ApprovalsViewState, TuiMode};
use casparian_sentinel::{ApiStorage, ControlClient};
use crossterm::event::{KeyCode, KeyEvent};
use std::time::Duration;

impl App {
    /// Handle Approvals view keys (key 5)
    /// Per keybinding matrix: a=approve, r=reject, Enter=details, f=filter
    pub(super) fn handle_approvals_key(&mut self, key: KeyEvent) {
        let filtered_count = self.approvals_state.filtered_approvals().len();

        // Handle confirm dialogs first
        match self.approvals_state.view_state {
            ApprovalsViewState::ConfirmApprove => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Enter => {
                        // Approve the selected approval
                        if let Some(approval) = self.approvals_state.selected_approval() {
                            let approval_id = approval.id.clone();
                            self.approve_approval(&approval_id);
                        }
                        self.approvals_state.view_state = ApprovalsViewState::List;
                        self.approvals_state.confirm_action = None;
                    }
                    KeyCode::Char('n') | KeyCode::Esc => {
                        self.approvals_state.view_state = ApprovalsViewState::List;
                        self.approvals_state.confirm_action = None;
                    }
                    _ => {}
                }
                return;
            }
            ApprovalsViewState::ConfirmReject => {
                match key.code {
                    KeyCode::Enter => {
                        // Reject with reason
                        if let Some(approval) = self.approvals_state.selected_approval() {
                            let approval_id = approval.id.clone();
                            let reason = if self.approvals_state.rejection_reason.is_empty() {
                                None
                            } else {
                                Some(self.approvals_state.rejection_reason.clone())
                            };
                            self.reject_approval(&approval_id, reason);
                        }
                        self.approvals_state.view_state = ApprovalsViewState::List;
                        self.approvals_state.confirm_action = None;
                        self.approvals_state.rejection_reason.clear();
                    }
                    KeyCode::Esc => {
                        self.approvals_state.view_state = ApprovalsViewState::List;
                        self.approvals_state.confirm_action = None;
                        self.approvals_state.rejection_reason.clear();
                    }
                    KeyCode::Char(c) => {
                        self.approvals_state.rejection_reason.push(c);
                    }
                    KeyCode::Backspace => {
                        self.approvals_state.rejection_reason.pop();
                    }
                    _ => {}
                }
                return;
            }
            ApprovalsViewState::Detail => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.approvals_state.view_state = ApprovalsViewState::List;
                    }
                    KeyCode::Char('a') => {
                        if let Some(approval) = self.approvals_state.selected_approval() {
                            if approval.is_pending() {
                                self.approvals_state.view_state =
                                    ApprovalsViewState::ConfirmApprove;
                                self.approvals_state.confirm_action = Some(ApprovalAction::Approve);
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(approval) = self.approvals_state.selected_approval() {
                            if approval.is_pending() {
                                self.approvals_state.view_state = ApprovalsViewState::ConfirmReject;
                                self.approvals_state.confirm_action = Some(ApprovalAction::Reject);
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
            ApprovalsViewState::List => {}
        }

        // Normal list mode
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.approvals_state.selected_index > 0 {
                    self.approvals_state.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if filtered_count > 0
                    && self.approvals_state.selected_index < filtered_count.saturating_sub(1)
                {
                    self.approvals_state.selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(approval) = self
                    .approvals_state
                    .filtered_approvals()
                    .get(self.approvals_state.selected_index)
                {
                    let approval_id = approval.id.clone();
                    if self.approvals_state.pinned_approval_id == Some(approval_id.clone()) {
                        self.approvals_state.pinned_approval_id = None;
                    } else {
                        self.approvals_state.pinned_approval_id = Some(approval_id);
                    }
                }
            }
            KeyCode::Char('a') => {
                if let Some(approval) = self
                    .approvals_state
                    .filtered_approvals()
                    .get(self.approvals_state.selected_index)
                {
                    if approval.is_pending() {
                        self.approvals_state.view_state = ApprovalsViewState::ConfirmApprove;
                        self.approvals_state.confirm_action = Some(ApprovalAction::Approve);
                    }
                }
            }
            KeyCode::Char('r') => {
                if let Some(approval) = self
                    .approvals_state
                    .filtered_approvals()
                    .get(self.approvals_state.selected_index)
                {
                    if approval.is_pending() {
                        self.approvals_state.view_state = ApprovalsViewState::ConfirmReject;
                        self.approvals_state.confirm_action = Some(ApprovalAction::Reject);
                        self.approvals_state.rejection_reason.clear();
                    }
                }
            }
            KeyCode::Char('f') => {
                self.approvals_state.filter = self.approvals_state.filter.next();
                self.approvals_state.clamp_selection();
            }
            KeyCode::Char('d') => {
                if filtered_count > 0 {
                    self.approvals_state.view_state = ApprovalsViewState::Detail;
                }
            }
            KeyCode::Char('R') => {
                self.approvals_state.approvals_loaded = false;
            }
            KeyCode::Esc => {
                if let Some(prev_mode) = self.approvals_state.previous_mode.take() {
                    self.set_mode(prev_mode);
                } else {
                    self.set_mode(TuiMode::Home);
                }
            }
            _ => {}
        }
    }

    /// Approve an approval request (stub - actual implementation connects to MCP)
    fn approve_approval(&mut self, approval_id: &str) {
        // Update in-memory state immediately for UI feedback
        if let Some(approval) = self
            .approvals_state
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
        {
            approval.status = ApprovalDisplayStatus::Approved;
        }

        // Call backend to persist the approval
        let approval_id_owned = approval_id.to_string();
        if self.control_connected {
            if let Some(control_addr) = self.control_addr.clone() {
                std::thread::spawn(move || {
                    if let Ok(client) =
                        ControlClient::connect_with_timeout(&control_addr, Duration::from_millis(500))
                    {
                        let _ = client.approve(&approval_id_owned);
                    }
                });
            }
        } else {
            if self.mutations_blocked() {
                self.set_global_status("Sentinel not reachable; cannot approve", true);
                return;
            }
            let (backend, db_path) = self.resolve_db_target();
            std::thread::spawn(move || {
                if let Ok(Some(conn)) = App::open_db_write_with(backend, &db_path) {
                    let storage = ApiStorage::new(conn);
                    if let Err(e) = storage.init_schema() {
                        tracing::error!("Failed to init schema for approval: {}", e);
                        return;
                    }
                    if let Err(e) = storage.approve(&approval_id_owned, None) {
                        tracing::error!("Failed to approve {}: {}", approval_id_owned, e);
                    }
                }
            });
        }

        // Mark approvals as needing refresh to pick up any job_id changes
        self.approvals_state.approvals_loaded = false;
    }

    /// Reject an approval request
    fn reject_approval(&mut self, approval_id: &str, reason: Option<String>) {
        // Update in-memory state immediately for UI feedback
        if let Some(approval) = self
            .approvals_state
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
        {
            approval.status = ApprovalDisplayStatus::Rejected;
        }

        // Call backend to persist the rejection
        let approval_id_owned = approval_id.to_string();
        let reason_owned = reason.clone();
        if self.control_connected {
            if let Some(control_addr) = self.control_addr.clone() {
                std::thread::spawn(move || {
                    if let Ok(client) =
                        ControlClient::connect_with_timeout(&control_addr, Duration::from_millis(500))
                    {
                        let reason = reason_owned.as_deref().unwrap_or("");
                        let _ = client.reject(&approval_id_owned, reason);
                    }
                });
            }
        } else {
            if self.mutations_blocked() {
                self.set_global_status("Sentinel not reachable; cannot reject", true);
                return;
            }
            let (backend, db_path) = self.resolve_db_target();
            std::thread::spawn(move || {
                if let Ok(Some(conn)) = App::open_db_write_with(backend, &db_path) {
                    let storage = ApiStorage::new(conn);
                    if let Err(e) = storage.init_schema() {
                        tracing::error!("Failed to init schema for rejection: {}", e);
                        return;
                    }
                    if let Err(e) = storage.reject(&approval_id_owned, None, reason_owned.as_deref()) {
                        tracing::error!("Failed to reject {}: {}", approval_id_owned, e);
                    }
                }
            });
        }

        // Mark approvals as needing refresh
        self.approvals_state.approvals_loaded = false;
    }
}

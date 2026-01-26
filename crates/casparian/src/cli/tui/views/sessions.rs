use super::*;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    fn load_pending_gate_info(&self, session_id: &str, gate_id: &str) -> Result<GateInfo, String> {
        let session_id: SessionId = session_id
            .parse()
            .map_err(|err| format!("Invalid session id: {}", err))?;
        let store = SessionStore::with_root(casparian_home().join("sessions"));
        let bundle = store
            .get_session(session_id)
            .map_err(|err| format!("Failed to load session: {}", err))?;
        let manifest = bundle
            .read_manifest()
            .map_err(|err| format!("Failed to read manifest: {}", err))?;

        match gate_id {
            "G1" => self.build_selection_gate_info(&bundle, &manifest),
            _ => Err(format!("Gate {} not supported yet", gate_id)),
        }
    }

    fn build_selection_gate_info(
        &self,
        bundle: &SessionBundle,
        manifest: &SessionManifest,
    ) -> Result<GateInfo, String> {
        let artifact = manifest
            .artifacts
            .iter()
            .rev()
            .find(|artifact| artifact.kind == "selection")
            .ok_or_else(|| "No selection proposal found".to_string())?;
        let path = bundle.session_dir().join(&artifact.reference);
        let content =
            std::fs::read_to_string(&path).map_err(|err| format!("Read proposal: {}", err))?;
        let proposal: SelectionProposal =
            serde_json::from_str(&content).map_err(|err| format!("Parse proposal: {}", err))?;

        let selected_examples = proposal.preview.selected_examples.clone();
        let near_miss_examples = proposal.preview.near_miss_examples.clone();
        let proposal_summary = format!(
            "Selected {} examples, {} near misses",
            selected_examples.len(),
            near_miss_examples.len()
        );
        let evidence = self.selection_evidence_lines(&proposal);
        let confidence = Self::confidence_label_string(proposal.confidence.label);
        let next_actions = proposal
            .next_actions
            .iter()
            .map(|action| Self::next_action_label(action.clone()))
            .collect();

        Ok(GateInfo {
            gate_id: "G1".to_string(),
            gate_name: "File Selection".to_string(),
            proposal_summary,
            evidence,
            confidence,
            selected_examples,
            near_miss_examples,
            next_actions,
            proposal_id: proposal.proposal_id,
            approval_target_hash: proposal.proposal_hash,
        })
    }

    fn selection_evidence_lines(&self, proposal: &SelectionProposal) -> Vec<String> {
        let mut evidence = Vec::new();

        for item in proposal.evidence.top_dir_prefixes.iter().take(3) {
            evidence.push(format!("Dir {} ({})", item.prefix, item.count));
        }
        for item in proposal.evidence.extensions.iter().take(3) {
            evidence.push(format!("Ext {} ({})", item.ext, item.count));
        }
        for item in proposal.evidence.semantic_tokens.iter().take(3) {
            evidence.push(format!("Token {} ({})", item.token, item.count));
        }
        for item in proposal
            .evidence
            .collision_with_existing_tags
            .iter()
            .take(3)
        {
            evidence.push(format!("Tag collision {} ({})", item.tag, item.count));
        }
        for reason in proposal.confidence.reasons.iter().take(3) {
            evidence.push(format!("Reason: {}", reason));
        }

        if evidence.is_empty() {
            evidence.push("No evidence recorded".to_string());
        }

        evidence
    }

    fn confidence_label_string(label: ConfidenceLabel) -> String {
        match label {
            ConfidenceLabel::High => "HIGH".to_string(),
            ConfidenceLabel::Med => "MEDIUM".to_string(),
            ConfidenceLabel::Low => "LOW".to_string(),
        }
    }

    fn next_action_label(action: NextAction) -> String {
        let raw = format!("{:?}", action);
        let mut out = String::new();
        for (idx, ch) in raw.chars().enumerate() {
            if idx > 0 && ch.is_uppercase() {
                out.push(' ');
            }
            out.push(ch);
        }
        out
    }

    fn apply_gate_decision(&mut self, decision: Decision) -> Result<(), String> {
        let session_id = self
            .sessions_state
            .active_session
            .clone()
            .ok_or_else(|| "No active session selected".to_string())?;
        let gate = self
            .sessions_state
            .pending_gate
            .clone()
            .ok_or_else(|| "No pending gate loaded".to_string())?;

        let next_state = match (gate.gate_id.as_str(), &decision) {
            ("G1", Decision::Approve) => IntentState::ProposeTagRules,
            ("G1", Decision::Reject) => IntentState::ProposeSelection,
            _ => return Err(format!("Decision for {} not supported yet", gate.gate_id)),
        };

        let session_id_copy = session_id.clone();
        self.record_gate_decision(&session_id, &gate, decision, next_state)?;
        self.sessions_state.pending_select_session_id = Some(session_id_copy);
        self.sessions_state.sessions_loaded = false;
        Ok(())
    }

    fn record_gate_decision(
        &self,
        session_id: &str,
        gate: &GateInfo,
        decision: Decision,
        next_state: IntentState,
    ) -> Result<(), String> {
        let session_id: SessionId = session_id
            .parse()
            .map_err(|err| format!("Invalid session id: {}", err))?;
        let store = SessionStore::with_root(casparian_home().join("sessions"));
        let bundle = store
            .get_session(session_id)
            .map_err(|err| format!("Failed to load session: {}", err))?;

        let notes = format!("{} {:?} via TUI", gate.gate_id, decision);
        let decision_record = DecisionRecord {
            timestamp: Utc::now(),
            actor: "tui".to_string(),
            decision: decision.clone(),
            target: DecisionTarget {
                proposal_id: gate.proposal_id,
                approval_target_hash: gate.approval_target_hash.clone(),
            },
            choice_payload: serde_json::json!({}),
            notes: Some(notes),
        };
        bundle
            .append_decision(&decision_record)
            .map_err(|err| format!("Append decision failed: {}", err))?;
        bundle
            .update_state(next_state)
            .map_err(|err| format!("Update state failed: {}", err))?;

        Ok(())
    }
    pub(super) fn handle_sessions_key(&mut self, key: KeyEvent) {
        match self.sessions_state.view_state {
            SessionsViewState::SessionList => self.handle_sessions_list_key(key),
            SessionsViewState::SessionDetail => self.handle_session_detail_key(key),
            SessionsViewState::WorkflowProgress => self.handle_workflow_progress_key(key),
            SessionsViewState::ProposalReview => self.handle_proposal_review_key(key),
            SessionsViewState::GateApproval => self.handle_gate_approval_key(key),
        }
    }
    fn handle_sessions_list_key(&mut self, key: KeyEvent) {
        match key.code {
            // Navigate list
            KeyCode::Down => {
                if self.sessions_state.selected_index
                    < self.sessions_state.sessions.len().saturating_sub(1)
                {
                    self.sessions_state.selected_index += 1;
                }
            }
            KeyCode::Up => {
                if self.sessions_state.selected_index > 0 {
                    self.sessions_state.selected_index -= 1;
                }
            }
            // View session details
            KeyCode::Enter => {
                // Extract needed values first to avoid borrow conflicts
                let session_info = self
                    .sessions_state
                    .selected_session()
                    .map(|s| (s.id.clone(), s.pending_gate.clone()));
                if let Some((session_id, pending_gate)) = session_info {
                    self.sessions_state.active_session = Some(session_id.clone());
                    // If there's a pending gate, go to gate approval, otherwise session detail
                    if let Some(gate_id) = pending_gate {
                        match self.load_pending_gate_info(&session_id, &gate_id) {
                            Ok(gate) => {
                                self.sessions_state.pending_gate = Some(gate);
                                self.sessions_state
                                    .transition_state(SessionsViewState::GateApproval);
                            }
                            Err(err) => {
                                tracing::error!("{}", err);
                                self.sessions_state.pending_gate = None;
                                self.sessions_state
                                    .transition_state(SessionsViewState::SessionDetail);
                            }
                        }
                    } else {
                        self.sessions_state.pending_gate = None;
                        self.sessions_state
                            .transition_state(SessionsViewState::SessionDetail);
                    }
                }
            }
            // New session (would open command palette in full implementation)
            KeyCode::Char('n') => {
                self.command_palette.open(CommandPaletteMode::Intent);
            }
            // Escape returns to previous mode
            KeyCode::Esc => {
                if let Some(prev_mode) = self.sessions_state.previous_mode {
                    self.set_mode(prev_mode);
                    self.sessions_state.previous_mode = None;
                } else {
                    self.set_mode(TuiMode::Home);
                }
            }
            // Refresh sessions list
            KeyCode::Char('r') => {
                self.sessions_state.sessions_loaded = false;
                // TODO: Trigger sessions reload
            }
            _ => {}
        }
    }
    fn handle_session_detail_key(&mut self, key: KeyEvent) {
        match key.code {
            // View workflow progress
            KeyCode::Char('w') => {
                self.sessions_state
                    .transition_state(SessionsViewState::WorkflowProgress);
            }
            // Jump to Jobs view
            KeyCode::Char('j') => {
                self.set_run_tab(RunTab::Jobs);
            }
            // Jump to Query view with a template
            KeyCode::Char('q') => {
                if let Some(session) = self.sessions_state.selected_session() {
                    let template = format!(
                        "-- Session {}\nSELECT * FROM cf_pipeline_runs ORDER BY started_at DESC LIMIT 50;",
                        session.id
                    );
                    self.query_state.sql_input = template;
                    self.query_state.cursor_position = self.query_state.sql_input.len();
                    self.query_state.view_state = QueryViewState::Editing;
                    self.query_state.error = None;
                    self.query_state.results = None;
                }
                self.navigate_to_mode(TuiMode::Query);
            }
            // Jump to Discover view
            KeyCode::Char('d') => {
                self.set_ingest_tab(IngestTab::Select);
            }
            // Back to session list
            KeyCode::Esc => {
                self.sessions_state.return_to_previous_state();
                self.sessions_state.active_session = None;
            }
            _ => {}
        }
    }
    fn handle_workflow_progress_key(&mut self, key: KeyEvent) {
        match key.code {
            // Back to session detail
            KeyCode::Esc => {
                self.sessions_state.return_to_previous_state();
            }
            _ => {}
        }
    }
    fn handle_proposal_review_key(&mut self, key: KeyEvent) {
        match key.code {
            // Back to previous view
            KeyCode::Esc => {
                self.sessions_state.return_to_previous_state();
                self.sessions_state.current_proposal = None;
            }
            _ => {}
        }
    }
    fn handle_gate_approval_key(&mut self, key: KeyEvent) {
        match key.code {
            // Approve gate
            KeyCode::Char('a') | KeyCode::Enter => {
                match self.apply_gate_decision(Decision::Approve) {
                    Ok(()) => {
                        self.sessions_state.pending_gate = None;
                        self.sessions_state.return_to_previous_state();
                    }
                    Err(err) => {
                        tracing::error!("{}", err);
                    }
                }
            }
            // Reject gate
            KeyCode::Char('r') => match self.apply_gate_decision(Decision::Reject) {
                Ok(()) => {
                    self.sessions_state.pending_gate = None;
                    self.sessions_state.return_to_previous_state();
                }
                Err(err) => {
                    tracing::error!("{}", err);
                }
            },
            // Back to session list without action
            KeyCode::Esc => {
                self.sessions_state.pending_gate = None;
                self.sessions_state.return_to_previous_state();
            }
            _ => {}
        }
    }
}

//! Intent pipeline core types and state machine.
//!
//! Canonical definitions for SessionId, IntentState, and workflow transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Session ID - Newtype to prevent mixing with other IDs
// ============================================================================

/// Session identifier (UUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

// ============================================================================
// Intent State - The core state machine
// ============================================================================

/// Intent pipeline states (S0-S12) with gates (G1-G6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntentState {
    // ========== Processing States (S0-S12) ==========
    /// S0: Interpret the user's intent
    InterpretIntent,
    /// S1: Scan the corpus for files
    ScanCorpus,
    /// S2: Propose file selection
    ProposeSelection,
    /// S3: Propose tagging rules
    ProposeTagRules,
    /// S4: Propose path-derived fields
    ProposePathFields,
    /// S5: Infer schema intent
    InferSchemaIntent,
    /// S6: Generate parser draft
    GenerateParserDraft,
    /// S7: Backtest with fail-fast loop
    BacktestFailFast,
    /// S8: Promote schema (ephemeral -> schema-as-code)
    PromoteSchema,
    /// S9: Create publish plan
    PublishPlan,
    /// S10: Execute publish
    PublishExecute,
    /// S11: Create run plan
    RunPlan,
    /// S12: Execute run
    RunExecute,

    // ========== Gates (G1-G6) - Human approval required ==========
    /// G1: Human approves selection + corpus snapshot
    AwaitingSelectionApproval,
    /// G2: Human approves enabling persistent tagging rules
    AwaitingTagRulesApproval,
    /// G3: Human approves derived fields + namespacing + collision resolutions
    AwaitingPathFieldsApproval,
    /// G4: Human resolves ambiguities / approves safe defaults
    AwaitingSchemaApproval,
    /// G5: Human approves publish (schema + parser)
    AwaitingPublishApproval,
    /// G6: Human approves run/backfill scope
    AwaitingRunApproval,

    // ========== Terminal States ==========
    /// Terminal: Completed successfully
    Completed,
    /// Terminal: Failed
    Failed,
    /// Terminal: Cancelled by user
    Cancelled,
}

impl IntentState {
    pub const ALL: [IntentState; 22] = [
        IntentState::InterpretIntent,
        IntentState::ScanCorpus,
        IntentState::ProposeSelection,
        IntentState::AwaitingSelectionApproval,
        IntentState::ProposeTagRules,
        IntentState::AwaitingTagRulesApproval,
        IntentState::ProposePathFields,
        IntentState::AwaitingPathFieldsApproval,
        IntentState::InferSchemaIntent,
        IntentState::AwaitingSchemaApproval,
        IntentState::GenerateParserDraft,
        IntentState::BacktestFailFast,
        IntentState::PromoteSchema,
        IntentState::PublishPlan,
        IntentState::AwaitingPublishApproval,
        IntentState::PublishExecute,
        IntentState::RunPlan,
        IntentState::AwaitingRunApproval,
        IntentState::RunExecute,
        IntentState::Completed,
        IntentState::Failed,
        IntentState::Cancelled,
    ];

    pub const GATES: [IntentState; 6] = [
        IntentState::AwaitingSelectionApproval,
        IntentState::AwaitingTagRulesApproval,
        IntentState::AwaitingPathFieldsApproval,
        IntentState::AwaitingSchemaApproval,
        IntentState::AwaitingPublishApproval,
        IntentState::AwaitingRunApproval,
    ];

    /// Get the canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentState::InterpretIntent => "S0_INTERPRET_INTENT",
            IntentState::ScanCorpus => "S1_SCAN_CORPUS",
            IntentState::ProposeSelection => "S2_PROPOSE_SELECTION",
            IntentState::AwaitingSelectionApproval => "G1_AWAITING_SELECTION_APPROVAL",
            IntentState::ProposeTagRules => "S3_PROPOSE_TAG_RULES",
            IntentState::AwaitingTagRulesApproval => "G2_AWAITING_TAG_RULES_APPROVAL",
            IntentState::ProposePathFields => "S4_PROPOSE_PATH_FIELDS",
            IntentState::AwaitingPathFieldsApproval => "G3_AWAITING_PATH_FIELDS_APPROVAL",
            IntentState::InferSchemaIntent => "S5_INFER_SCHEMA_INTENT",
            IntentState::AwaitingSchemaApproval => "G4_AWAITING_SCHEMA_APPROVAL",
            IntentState::GenerateParserDraft => "S6_GENERATE_PARSER_DRAFT",
            IntentState::BacktestFailFast => "S7_BACKTEST_FAIL_FAST",
            IntentState::PromoteSchema => "S8_PROMOTE_SCHEMA",
            IntentState::PublishPlan => "S9_PUBLISH_PLAN",
            IntentState::AwaitingPublishApproval => "G5_AWAITING_PUBLISH_APPROVAL",
            IntentState::PublishExecute => "S10_PUBLISH_EXECUTE",
            IntentState::RunPlan => "S11_RUN_PLAN",
            IntentState::AwaitingRunApproval => "G6_AWAITING_RUN_APPROVAL",
            IntentState::RunExecute => "S12_RUN_EXECUTE",
            IntentState::Completed => "COMPLETED",
            IntentState::Failed => "FAILED",
            IntentState::Cancelled => "CANCELLED",
        }
    }

    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            IntentState::Completed | IntentState::Failed | IntentState::Cancelled
        )
    }

    /// Check if this is a gate (awaiting human approval).
    pub fn is_gate(&self) -> bool {
        matches!(
            self,
            IntentState::AwaitingSelectionApproval
                | IntentState::AwaitingTagRulesApproval
                | IntentState::AwaitingPathFieldsApproval
                | IntentState::AwaitingSchemaApproval
                | IntentState::AwaitingPublishApproval
                | IntentState::AwaitingRunApproval
        )
    }

    /// Get the gate number if this is a gate (1-6).
    pub fn gate_number(&self) -> Option<u8> {
        match self {
            IntentState::AwaitingSelectionApproval => Some(1),
            IntentState::AwaitingTagRulesApproval => Some(2),
            IntentState::AwaitingPathFieldsApproval => Some(3),
            IntentState::AwaitingSchemaApproval => Some(4),
            IntentState::AwaitingPublishApproval => Some(5),
            IntentState::AwaitingRunApproval => Some(6),
            _ => None,
        }
    }

    /// Get valid transitions from this state.
    pub fn valid_transitions(&self) -> &'static [IntentState] {
        match self {
            IntentState::InterpretIntent => &[
                IntentState::ScanCorpus,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::ScanCorpus => &[
                IntentState::ProposeSelection,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::ProposeSelection => &[
                IntentState::AwaitingSelectionApproval,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingSelectionApproval => &[
                IntentState::ProposeTagRules,
                IntentState::ProposeSelection,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::ProposeTagRules => &[
                IntentState::AwaitingTagRulesApproval,
                IntentState::ProposePathFields,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingTagRulesApproval => &[
                IntentState::ProposePathFields,
                IntentState::ProposeTagRules,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::ProposePathFields => &[
                IntentState::AwaitingPathFieldsApproval,
                IntentState::InferSchemaIntent,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingPathFieldsApproval => &[
                IntentState::InferSchemaIntent,
                IntentState::ProposePathFields,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::InferSchemaIntent => &[
                IntentState::AwaitingSchemaApproval,
                IntentState::GenerateParserDraft,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingSchemaApproval => &[
                IntentState::GenerateParserDraft,
                IntentState::InferSchemaIntent,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::GenerateParserDraft => &[
                IntentState::BacktestFailFast,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::BacktestFailFast => &[
                IntentState::PromoteSchema,
                IntentState::GenerateParserDraft,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::PromoteSchema => &[
                IntentState::PublishPlan,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::PublishPlan => &[
                IntentState::AwaitingPublishApproval,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingPublishApproval => &[
                IntentState::PublishExecute,
                IntentState::PublishPlan,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::PublishExecute => &[
                IntentState::RunPlan,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::RunPlan => &[
                IntentState::AwaitingRunApproval,
                IntentState::Completed,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::AwaitingRunApproval => &[
                IntentState::RunExecute,
                IntentState::RunPlan,
                IntentState::Completed,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::RunExecute => &[
                IntentState::Completed,
                IntentState::Failed,
                IntentState::Cancelled,
            ],
            IntentState::Completed | IntentState::Failed | IntentState::Cancelled => &[],
        }
    }

    /// Check if a transition to the target state is valid.
    pub fn can_transition_to(&self, target: IntentState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

impl fmt::Display for IntentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error when parsing an IntentState from string.
#[derive(Debug, Error, Clone)]
#[error("invalid state: {0}")]
pub struct StateParseError(String);

impl std::str::FromStr for IntentState {
    type Err = StateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "S0_INTERPRET_INTENT" => Ok(IntentState::InterpretIntent),
            "S1_SCAN_CORPUS" => Ok(IntentState::ScanCorpus),
            "S2_PROPOSE_SELECTION" => Ok(IntentState::ProposeSelection),
            "G1_AWAITING_SELECTION_APPROVAL" => Ok(IntentState::AwaitingSelectionApproval),
            "S3_PROPOSE_TAG_RULES" => Ok(IntentState::ProposeTagRules),
            "G2_AWAITING_TAG_RULES_APPROVAL" => Ok(IntentState::AwaitingTagRulesApproval),
            "S4_PROPOSE_PATH_FIELDS" => Ok(IntentState::ProposePathFields),
            "G3_AWAITING_PATH_FIELDS_APPROVAL" => Ok(IntentState::AwaitingPathFieldsApproval),
            "S5_INFER_SCHEMA_INTENT" => Ok(IntentState::InferSchemaIntent),
            "G4_AWAITING_SCHEMA_APPROVAL" => Ok(IntentState::AwaitingSchemaApproval),
            "S6_GENERATE_PARSER_DRAFT" => Ok(IntentState::GenerateParserDraft),
            "S7_BACKTEST_FAIL_FAST" => Ok(IntentState::BacktestFailFast),
            "S8_PROMOTE_SCHEMA" => Ok(IntentState::PromoteSchema),
            "S9_PUBLISH_PLAN" => Ok(IntentState::PublishPlan),
            "G5_AWAITING_PUBLISH_APPROVAL" => Ok(IntentState::AwaitingPublishApproval),
            "S10_PUBLISH_EXECUTE" => Ok(IntentState::PublishExecute),
            "S11_RUN_PLAN" => Ok(IntentState::RunPlan),
            "G6_AWAITING_RUN_APPROVAL" => Ok(IntentState::AwaitingRunApproval),
            "S12_RUN_EXECUTE" => Ok(IntentState::RunExecute),
            "COMPLETED" => Ok(IntentState::Completed),
            "FAILED" => Ok(IntentState::Failed),
            "CANCELLED" => Ok(IntentState::Cancelled),
            _ => Err(StateParseError(s.to_string())),
        }
    }
}

// ============================================================================
// State Transition
// ============================================================================

/// A state transition event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: IntentState,
    pub to: IntentState,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl StateTransition {
    pub fn new(from: IntentState, to: IntentState) -> Self {
        Self {
            from,
            to,
            timestamp: Utc::now(),
            reason: None,
            actor: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }
}

// ============================================================================
// State Machine Manager
// ============================================================================

/// Errors for state machine operations.
#[derive(Debug, Error)]
pub enum StateMachineError {
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: IntentState, to: IntentState },

    #[error("state is terminal: {0}")]
    TerminalState(IntentState),
}

/// State machine manager for a session.
#[derive(Debug)]
pub struct StateMachine {
    current: IntentState,
    history: Vec<StateTransition>,
}

impl StateMachine {
    /// Create a new state machine starting at InterpretIntent.
    pub fn new() -> Self {
        Self {
            current: IntentState::InterpretIntent,
            history: Vec::new(),
        }
    }

    /// Create a state machine from a known state.
    pub fn from_state(state: IntentState) -> Self {
        Self {
            current: state,
            history: Vec::new(),
        }
    }

    /// Get the current state.
    pub fn current(&self) -> IntentState {
        self.current
    }

    /// Get the transition history.
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// Attempt to transition to a new state.
    pub fn transition(&mut self, to: IntentState) -> Result<StateTransition, StateMachineError> {
        self.transition_with_reason(to, None, None)
    }

    /// Attempt to transition with reason and actor.
    pub fn transition_with_reason(
        &mut self,
        to: IntentState,
        reason: Option<String>,
        actor: Option<String>,
    ) -> Result<StateTransition, StateMachineError> {
        if self.current.is_terminal() {
            return Err(StateMachineError::TerminalState(self.current));
        }

        if !self.current.can_transition_to(to) {
            return Err(StateMachineError::InvalidTransition {
                from: self.current,
                to,
            });
        }

        let mut transition = StateTransition::new(self.current, to);
        if let Some(r) = reason {
            transition = transition.with_reason(r);
        }
        if let Some(a) = actor {
            transition = transition.with_actor(a);
        }

        self.current = to;
        self.history.push(transition.clone());

        Ok(transition)
    }

    /// Force a transition (for recovery/admin use).
    pub fn force_transition(&mut self, to: IntentState, reason: &str) -> StateTransition {
        let transition =
            StateTransition::new(self.current, to).with_reason(format!("FORCED: {}", reason));
        self.current = to;
        self.history.push(transition.clone());
        transition
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Session - The core session record
// ============================================================================

/// A Session in the Intent Pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub session_id: SessionId,
    /// User's original intent text
    pub intent_text: String,
    /// Current state (typed enum, not string)
    pub state: IntentState,
    /// Number of files in the current selection (0 if not yet selected)
    pub files_selected: u64,
    /// When the session was created (RFC3339 string)
    pub created_at: String,
    /// When the session was last updated (RFC3339 string)
    pub updated_at: String,
    /// Optional input directory
    pub input_dir: Option<String>,
    /// Optional error message (only set when state is Failed)
    pub error_message: Option<String>,
    /// ID of pending question if at a gate (only set when state is a gate)
    pub pending_question_id: Option<String>,
}

impl Session {
    /// Check if the session is at a gate requiring human input.
    pub fn needs_human_input(&self) -> bool {
        self.state.is_gate()
    }

    /// Check if the session is complete (success or failure).
    pub fn is_complete(&self) -> bool {
        self.state.is_terminal()
    }
}

// ============================================================================
// Session Question - Type-safe question for gates
// ============================================================================

/// Kind of question at a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuestionKind {
    /// G1: Confirm file selection
    ConfirmSelection,
    /// G2: Confirm tag rules
    ConfirmTagRules,
    /// G3: Confirm path fields
    ConfirmPathFields,
    /// G4: Resolve schema ambiguity
    ResolveSchemaAmbiguity,
    /// G5: Confirm publish
    ConfirmPublish,
    /// G6: Confirm run
    ConfirmRun,
}

impl fmt::Display for QuestionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            QuestionKind::ConfirmSelection => "CONFIRM_SELECTION",
            QuestionKind::ConfirmTagRules => "CONFIRM_TAG_RULES",
            QuestionKind::ConfirmPathFields => "CONFIRM_PATH_FIELDS",
            QuestionKind::ResolveSchemaAmbiguity => "RESOLVE_SCHEMA_AMBIGUITY",
            QuestionKind::ConfirmPublish => "CONFIRM_PUBLISH",
            QuestionKind::ConfirmRun => "CONFIRM_RUN",
        };
        write!(f, "{}", s)
    }
}

impl QuestionKind {
    /// Get the gate number this question kind corresponds to.
    pub fn gate_number(&self) -> u8 {
        match self {
            QuestionKind::ConfirmSelection => 1,
            QuestionKind::ConfirmTagRules => 2,
            QuestionKind::ConfirmPathFields => 3,
            QuestionKind::ResolveSchemaAmbiguity => 4,
            QuestionKind::ConfirmPublish => 5,
            QuestionKind::ConfirmRun => 6,
        }
    }

    /// Get the state this question should be asked at.
    pub fn expected_state(&self) -> IntentState {
        match self {
            QuestionKind::ConfirmSelection => IntentState::AwaitingSelectionApproval,
            QuestionKind::ConfirmTagRules => IntentState::AwaitingTagRulesApproval,
            QuestionKind::ConfirmPathFields => IntentState::AwaitingPathFieldsApproval,
            QuestionKind::ResolveSchemaAmbiguity => IntentState::AwaitingSchemaApproval,
            QuestionKind::ConfirmPublish => IntentState::AwaitingPublishApproval,
            QuestionKind::ConfirmRun => IntentState::AwaitingRunApproval,
        }
    }
}

/// A question option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// Unique option ID
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Description of what this option does
    pub description: String,
    /// Whether this is the default/recommended option
    pub is_default: bool,
}

/// A question requiring human input at a gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionQuestion {
    /// Unique question ID
    pub id: String,
    /// The kind of question (typed, not string)
    pub kind: QuestionKind,
    /// The question text
    pub prompt: String,
    /// Available options
    pub options: Vec<QuestionOption>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_roundtrip() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed: SessionId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_state_roundtrip() {
        for state in [
            IntentState::InterpretIntent,
            IntentState::ScanCorpus,
            IntentState::ProposeSelection,
            IntentState::AwaitingSelectionApproval,
            IntentState::Completed,
            IntentState::Failed,
        ] {
            let s = state.as_str();
            let parsed: IntentState = s.parse().unwrap();
            assert_eq!(state, parsed);
        }
    }

    #[test]
    fn test_state_serde_roundtrip() {
        let state = IntentState::InferSchemaIntent;
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: IntentState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(state, decoded);
    }

    #[test]
    fn test_gate_detection() {
        assert!(!IntentState::InterpretIntent.is_gate());
        assert!(!IntentState::ScanCorpus.is_gate());
        assert!(IntentState::AwaitingSelectionApproval.is_gate());
        assert!(IntentState::AwaitingTagRulesApproval.is_gate());
        assert!(IntentState::AwaitingPublishApproval.is_gate());
        assert!(!IntentState::Completed.is_gate());
    }

    #[test]
    fn test_terminal_detection() {
        assert!(!IntentState::InterpretIntent.is_terminal());
        assert!(!IntentState::BacktestFailFast.is_terminal());
        assert!(IntentState::Completed.is_terminal());
        assert!(IntentState::Failed.is_terminal());
        assert!(IntentState::Cancelled.is_terminal());
    }

    #[test]
    fn test_valid_transitions() {
        assert!(IntentState::InterpretIntent.can_transition_to(IntentState::ScanCorpus));
        assert!(IntentState::InterpretIntent.can_transition_to(IntentState::Failed));
        assert!(!IntentState::InterpretIntent.can_transition_to(IntentState::Completed));

        assert!(
            IntentState::AwaitingSelectionApproval.can_transition_to(IntentState::ProposeTagRules)
        );

        assert!(
            IntentState::AwaitingSelectionApproval.can_transition_to(IntentState::ProposeSelection)
        );

        assert!(IntentState::Completed.valid_transitions().is_empty());
    }

    #[test]
    fn test_question_kind_display() {
        assert_eq!(
            QuestionKind::ConfirmSelection.to_string(),
            "CONFIRM_SELECTION"
        );
        assert_eq!(QuestionKind::ConfirmRun.to_string(), "CONFIRM_RUN");
    }

    #[test]
    fn test_state_machine_transitions() {
        let mut sm = StateMachine::new();
        assert_eq!(sm.current(), IntentState::InterpretIntent);

        sm.transition(IntentState::ScanCorpus).unwrap();
        assert_eq!(sm.current(), IntentState::ScanCorpus);

        assert_eq!(sm.history()[0].from, IntentState::InterpretIntent);
        assert_eq!(sm.history()[0].to, IntentState::ScanCorpus);
    }

    #[test]
    fn test_state_machine_invalid_transition() {
        let mut sm = StateMachine::new();
        let result = sm.transition(IntentState::BacktestFailFast);
        assert!(matches!(
            result,
            Err(StateMachineError::InvalidTransition { .. })
        ));
        assert_eq!(sm.current(), IntentState::InterpretIntent);
    }

    #[test]
    fn test_state_machine_terminal_transition() {
        let mut sm = StateMachine::from_state(IntentState::Completed);
        let result = sm.transition(IntentState::InterpretIntent);
        assert!(matches!(result, Err(StateMachineError::TerminalState(_))));
    }

    #[test]
    fn test_state_machine_force_transition() {
        let mut sm = StateMachine::new();
        let transition = sm.force_transition(IntentState::BacktestFailFast, "admin override");
        assert_eq!(sm.current(), IntentState::BacktestFailFast);
        assert_eq!(transition.from, IntentState::InterpretIntent);
        assert_eq!(transition.to, IntentState::BacktestFailFast);
    }
}

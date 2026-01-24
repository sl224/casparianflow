//! Session management commands.
//!
//! These commands manage Intent Pipeline sessions with proper type safety.
//! Uses IntentState enum - no stringly-typed state representation.

use crate::session_types::{IntentState, QuestionKind, SessionId};
use crate::state::{AppState, CommandError, CommandResult};
use serde::{Deserialize, Serialize};
use tauri::State;

// ============================================================================
// Response Types - For JSON serialization to frontend
// ============================================================================

/// Session summary for list view.
///
/// Note: `state` is serialized as the canonical string (e.g., "S1_SCAN_CORPUS")
/// but the backend uses the typed IntentState enum internally.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub intent: String,
    pub state: String,
    pub files_selected: u64,
    pub created_at: String,
    pub has_question: bool,
    pub is_at_gate: bool,
    pub is_terminal: bool,
}

/// Full session status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub id: String,
    pub intent: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub input_dir: Option<String>,
    pub files_selected: u64,
    pub error_message: Option<String>,
    pub current_question: Option<SessionQuestionResponse>,
    pub is_at_gate: bool,
    pub is_terminal: bool,
    pub gate_number: Option<u8>,
    pub valid_next_states: Vec<String>,
}

/// Session question response for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuestionResponse {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub options: Vec<QuestionOptionResponse>,
}

/// Question option response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOptionResponse {
    pub id: String,
    pub label: String,
    pub description: String,
    pub is_default: bool,
}

// ============================================================================
// Request Types
// ============================================================================

/// Create session request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub intent: String,
    pub input_dir: Option<String>,
}

/// Create session response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    pub session_id: String,
}

/// Advance session request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceSessionRequest {
    /// The target state (must be a valid transition from current state)
    pub target_state: String,
    /// Optional answer to a gate question
    pub answer: Option<String>,
}

/// Advance session response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceSessionResponse {
    pub success: bool,
    pub new_state: String,
    pub error: Option<String>,
}

// ============================================================================
// Commands
// ============================================================================

/// List all sessions.
///
/// Returns a list of session summaries for the sessions list view.
#[tauri::command]
pub async fn session_list(state: State<'_, AppState>) -> CommandResult<Vec<SessionSummary>> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let sessions = storage
        .list_sessions(None, 100)
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let summaries: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|session| SessionSummary {
            id: session.session_id.to_string(),
            intent: session.intent_text,
            state: session.state.as_str().to_string(),
            files_selected: session.files_selected,
            created_at: session.created_at, // Already RFC3339 string
            has_question: session.pending_question_id.is_some(),
            is_at_gate: session.state.is_gate(),
            is_terminal: session.state.is_terminal(),
        })
        .collect();

    Ok(summaries)
}

/// Create a new session.
#[tauri::command]
pub async fn session_create(
    request: CreateSessionRequest,
    state: State<'_, AppState>,
) -> CommandResult<CreateSessionResponse> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let session_id = storage
        .create_session(&request.intent, request.input_dir.as_deref())
        .map_err(|e| CommandError::Database(e.to_string()))?;

    Ok(CreateSessionResponse {
        session_id: session_id.to_string(),
    })
}

/// Get session status by ID.
#[tauri::command]
pub async fn session_status(
    session_id: String,
    state: State<'_, AppState>,
) -> CommandResult<SessionStatus> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // Parse session ID using the typed SessionId
    let parsed_id: SessionId = session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", session_id))
    })?;

    let session = storage
        .get_session(parsed_id)
        .map_err(|e| CommandError::Database(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(format!("Session {} not found", session_id)))?;

    // Build the current question if at a gate
    let current_question = if session.state.is_gate() {
        session.pending_question_id.as_ref().map(|q_id| {
            // Build a placeholder question based on the gate
            let (kind, text) = gate_to_question(&session.state);
            SessionQuestionResponse {
                id: q_id.clone(),
                kind: kind.to_string(),
                text,
                options: vec![
                    QuestionOptionResponse {
                        id: "approve".to_string(),
                        label: "Approve".to_string(),
                        description: "Accept the proposal and proceed".to_string(),
                        is_default: true,
                    },
                    QuestionOptionResponse {
                        id: "reject".to_string(),
                        label: "Reject".to_string(),
                        description: "Reject and go back".to_string(),
                        is_default: false,
                    },
                ],
            }
        })
    } else {
        None
    };

    // Get valid next states
    let valid_next_states: Vec<String> = session
        .state
        .valid_transitions()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();

    Ok(SessionStatus {
        id: session.session_id.to_string(),
        intent: session.intent_text,
        state: session.state.as_str().to_string(),
        created_at: session.created_at, // Already RFC3339 string
        updated_at: session.updated_at, // Already RFC3339 string
        input_dir: session.input_dir,
        files_selected: session.files_selected,
        error_message: session.error_message,
        current_question,
        is_at_gate: session.state.is_gate(),
        is_terminal: session.state.is_terminal(),
        gate_number: session.state.gate_number(),
        valid_next_states,
    })
}

/// Advance a session to the next state.
///
/// This validates that the transition is allowed before applying it.
#[tauri::command]
pub async fn session_advance(
    session_id: String,
    request: AdvanceSessionRequest,
    state: State<'_, AppState>,
) -> CommandResult<AdvanceSessionResponse> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // Parse session ID
    let parsed_id: SessionId = session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", session_id))
    })?;

    // Parse target state - this validates the state string
    let target_state: IntentState = request.target_state.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid target state: {}", request.target_state))
    })?;

    // Attempt the transition - the storage layer validates it
    match storage.update_session_state(parsed_id, target_state) {
        Ok(_) => Ok(AdvanceSessionResponse {
            success: true,
            new_state: target_state.as_str().to_string(),
            error: None,
        }),
        Err(e) => Ok(AdvanceSessionResponse {
            success: false,
            new_state: request.target_state,
            error: Some(e.to_string()),
        }),
    }
}

/// Cancel a session.
#[tauri::command]
pub async fn session_cancel(
    session_id: String,
    state: State<'_, AppState>,
) -> CommandResult<AdvanceSessionResponse> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let parsed_id: SessionId = session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", session_id))
    })?;

    match storage.cancel_session(parsed_id) {
        Ok(_) => Ok(AdvanceSessionResponse {
            success: true,
            new_state: IntentState::Cancelled.as_str().to_string(),
            error: None,
        }),
        Err(e) => Ok(AdvanceSessionResponse {
            success: false,
            new_state: "".to_string(),
            error: Some(e.to_string()),
        }),
    }
}

/// List sessions that need human input (at gates).
#[tauri::command]
pub async fn session_list_pending(
    state: State<'_, AppState>,
) -> CommandResult<Vec<SessionSummary>> {
    let storage = state
        .open_session_storage()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let sessions = storage
        .list_sessions_needing_input(50)
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let summaries: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|session| SessionSummary {
            id: session.session_id.to_string(),
            intent: session.intent_text,
            state: session.state.as_str().to_string(),
            files_selected: session.files_selected,
            created_at: session.created_at, // Already RFC3339 string
            has_question: session.pending_question_id.is_some(),
            is_at_gate: true,
            is_terminal: false,
        })
        .collect();

    Ok(summaries)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Map a gate state to a question kind and default prompt.
fn gate_to_question(state: &IntentState) -> (QuestionKind, String) {
    match state {
        IntentState::AwaitingSelectionApproval => (
            QuestionKind::ConfirmSelection,
            "Review the proposed file selection. Approve to proceed or reject to refine."
                .to_string(),
        ),
        IntentState::AwaitingTagRulesApproval => (
            QuestionKind::ConfirmTagRules,
            "Review the proposed tagging rules. These will be used for file routing.".to_string(),
        ),
        IntentState::AwaitingPathFieldsApproval => (
            QuestionKind::ConfirmPathFields,
            "Review the path-derived fields. These will be extracted from file paths.".to_string(),
        ),
        IntentState::AwaitingSchemaApproval => (
            QuestionKind::ResolveSchemaAmbiguity,
            "Review the inferred schema. Resolve any ambiguities in column types.".to_string(),
        ),
        IntentState::AwaitingPublishApproval => (
            QuestionKind::ConfirmPublish,
            "Review the publish plan. This will register the parser and schema.".to_string(),
        ),
        IntentState::AwaitingRunApproval => (
            QuestionKind::ConfirmRun,
            "Review the run plan. This will process files and write to the sink.".to_string(),
        ),
        _ => (
            QuestionKind::ConfirmSelection,
            "Unexpected state for question".to_string(),
        ),
    }
}

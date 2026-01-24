//! MCP tools for session lifecycle (§7.1)
//!
//! - `casp.session.create` → Create a new intent pipeline session
//! - `casp.session.status` → Get session status, pending questions, active jobs

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::CoreHandle;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{ArtifactRef, HumanQuestion, SessionId};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Session Create Tool
// ============================================================================

/// Tool: casp.session.create
pub struct SessionCreateTool;

#[derive(Debug, Deserialize)]
struct SessionCreateArgs {
    /// The user's intent text (e.g., "process all sales files")
    intent: String,
    /// Optional actor identifier
    #[serde(default)]
    actor: Option<String>,
    /// Optional client identifier
    #[serde(default)]
    client: Option<String>,
}

#[derive(Debug, Serialize)]
struct SessionCreateResponse {
    session_id: SessionId,
    state: String,
    created_at: String,
}

impl McpTool for SessionCreateTool {
    fn name(&self) -> &'static str {
        "casp_session_create"
    }

    fn description(&self) -> &'static str {
        "Create a new intent pipeline session. Returns a session_id for tracking the workflow."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "description": "The user's intent text (e.g., 'process all sales files')"
                },
                "actor": {
                    "type": "string",
                    "description": "Optional actor identifier (e.g., email)"
                },
                "client": {
                    "type": "string",
                    "description": "Optional client identifier (e.g., 'cli', 'tui', 'api')"
                }
            },
            "required": ["intent"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: SessionCreateArgs = serde_json::from_value(args)?;

        // Get session store path from config or use default
        let session_store = SessionStore::new();

        let bundle = session_store.create_session(
            &args.intent,
            args.actor.as_deref(),
            args.client.as_deref(),
        )?;

        let manifest = bundle.read_manifest()?;

        let response = SessionCreateResponse {
            session_id: manifest.session_id,
            state: manifest.state,
            created_at: manifest.created_at.to_rfc3339(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Session Status Tool
// ============================================================================

/// Tool: casp.session.status
pub struct SessionStatusTool;

#[derive(Debug, Deserialize)]
struct SessionStatusArgs {
    /// Session ID to query
    session_id: SessionId,
}

#[derive(Debug, Serialize)]
struct SessionStatusResponse {
    session_id: SessionId,
    state: String,
    is_gate: bool,
    gate_number: Option<u8>,
    is_terminal: bool,
    intent_text: String,
    created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pending_questions: Vec<HumanQuestion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    active_jobs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<ArtifactRef>,
}

impl McpTool for SessionStatusTool {
    fn name(&self) -> &'static str {
        "casp_session_status"
    }

    fn description(&self) -> &'static str {
        "Get the status of an intent pipeline session, including state, pending questions, and active jobs."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID to query"
                }
            },
            "required": ["session_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: SessionStatusArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;
        let manifest = bundle.read_manifest()?;

        // Parse state
        let state: IntentState = manifest
            .state
            .parse()
            .unwrap_or(IntentState::InterpretIntent);

        // TODO: Load pending questions from the current proposal
        let pending_questions = Vec::new();

        // TODO: Load active jobs for this session
        let active_jobs = Vec::new();

        let response = SessionStatusResponse {
            session_id: manifest.session_id,
            state: manifest.state,
            is_gate: state.is_gate(),
            gate_number: state.gate_number(),
            is_terminal: state.is_terminal(),
            intent_text: manifest.intent_text,
            created_at: manifest.created_at.to_rfc3339(),
            pending_questions,
            active_jobs,
            artifacts: manifest.artifacts,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Session List Tool
// ============================================================================

/// Tool: casp.session.list
pub struct SessionListTool;

#[derive(Debug, Deserialize)]
struct SessionListArgs {
    /// Maximum number of sessions to return
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Serialize)]
struct SessionSummary {
    session_id: SessionId,
    state: String,
    intent_text: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct SessionListResponse {
    sessions: Vec<SessionSummary>,
    total: usize,
}

impl McpTool for SessionListTool {
    fn name(&self) -> &'static str {
        "casp_session_list"
    }

    fn description(&self) -> &'static str {
        "List all intent pipeline sessions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of sessions to return (default: 20)"
                }
            }
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: SessionListArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let session_ids = session_store.list_sessions()?;

        let mut sessions = Vec::new();
        for session_id in session_ids.into_iter().take(args.limit) {
            if let Ok(bundle) = session_store.get_session(session_id) {
                if let Ok(manifest) = bundle.read_manifest() {
                    sessions.push(SessionSummary {
                        session_id: manifest.session_id,
                        state: manifest.state,
                        intent_text: manifest.intent_text,
                        created_at: manifest.created_at.to_rfc3339(),
                    });
                }
            }
        }

        let total = sessions.len();

        let response = SessionListResponse { sessions, total };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_create_args_deserialize() {
        let json = json!({
            "intent": "process sales files",
            "actor": "user@example.com"
        });

        let args: SessionCreateArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.intent, "process sales files");
        assert_eq!(args.actor, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_session_status_args_deserialize() {
        let session_id = SessionId::new();
        let json = json!({
            "session_id": session_id.to_string()
        });

        let args: SessionStatusArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id, session_id);
    }
}

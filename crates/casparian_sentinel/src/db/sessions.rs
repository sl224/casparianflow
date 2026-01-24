//! Session storage for the Intent Pipeline.
//!
//! Ported from tauri-ui session storage to keep schema and semantics aligned.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbTimestamp, DbValue, UnifiedDbRow};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ============================================================================
// Session ID - Newtype to prevent mixing with other IDs
// ============================================================================

/// Session identifier (UUID).
///
/// Newtype wrapper prevents accidentally passing a job_id where session_id is expected.
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
///
/// This is an exhaustive enum - every possible state is represented.
/// The state machine validates transitions at runtime but the type
/// system ensures we can only be in ONE state at a time.
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
    /// Terminal states cannot transition to any other state.
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
    ///
    /// This enforces the state machine rules:
    /// - Terminal states have no valid transitions
    /// - Gates can backtrack to their preceding proposal state
    /// - Processing states progress forward or fail/cancel
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
#[derive(Debug, Clone)]
pub struct StateParseError(pub String);

impl fmt::Display for StateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid state: {}", self.0)
    }
}

impl std::error::Error for StateParseError {}

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
// Session - The core session record
// ============================================================================

/// A Session in the Intent Pipeline.
///
/// This is the database record - all fields are concrete, no Option<Option<>>.
/// State is stored as an enum, not a string.
/// Timestamps are stored as RFC3339 strings for simplicity.
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

/// Storage for Intent Pipeline sessions.
///
/// All operations are synchronous and use the provided DuckDB connection.
pub struct SessionStorage {
    conn: DbConnection,
}

impl SessionStorage {
    /// Create session storage from a database connection.
    pub fn new(conn: DbConnection) -> Self {
        Self { conn }
    }

    /// Open session storage from a database URL.
    pub fn open(db_url: &str) -> Result<Self> {
        let conn = DbConnection::open_from_url(db_url)?;
        Ok(Self { conn })
    }

    /// Initialize the sessions schema (DDL).
    ///
    /// Creates the cf_sessions table with proper CHECK constraints
    /// that match the IntentState enum values.
    pub fn init_schema(&self) -> Result<()> {
        // All valid state strings - matches IntentState::as_str() exactly
        let state_values = r#"'S0_INTERPRET_INTENT','S1_SCAN_CORPUS','S2_PROPOSE_SELECTION','G1_AWAITING_SELECTION_APPROVAL','S3_PROPOSE_TAG_RULES','G2_AWAITING_TAG_RULES_APPROVAL','S4_PROPOSE_PATH_FIELDS','G3_AWAITING_PATH_FIELDS_APPROVAL','S5_INFER_SCHEMA_INTENT','G4_AWAITING_SCHEMA_APPROVAL','S6_GENERATE_PARSER_DRAFT','S7_BACKTEST_FAIL_FAST','S8_PROMOTE_SCHEMA','S9_PUBLISH_PLAN','G5_AWAITING_PUBLISH_APPROVAL','S10_PUBLISH_EXECUTE','S11_RUN_PLAN','G6_AWAITING_RUN_APPROVAL','S12_RUN_EXECUTE','COMPLETED','FAILED','CANCELLED'"#;

        let create_sql = format!(
            r#"
            -- Intent Pipeline Sessions table
            CREATE TABLE IF NOT EXISTS cf_sessions (
                session_id TEXT PRIMARY KEY,
                intent_text TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'S0_INTERPRET_INTENT' CHECK (state IN ({state_values})),
                files_selected BIGINT NOT NULL DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                input_dir TEXT,
                error_message TEXT,
                pending_question_id TEXT
            );
            CREATE INDEX IF NOT EXISTS ix_sessions_state ON cf_sessions(state);
            CREATE INDEX IF NOT EXISTS ix_sessions_created ON cf_sessions(created_at DESC);
            "#,
            state_values = state_values,
        );

        self.conn
            .execute_batch(&create_sql)
            .context("Failed to initialize sessions schema")?;

        Ok(())
    }

    // ========================================================================
    // Session Operations
    // ========================================================================

    /// Create a new session.
    pub fn create_session(&self, intent_text: &str, input_dir: Option<&str>) -> Result<SessionId> {
        let session_id = SessionId::new();
        let state = IntentState::InterpretIntent;

        let sql = r#"
            INSERT INTO cf_sessions (session_id, intent_text, state, input_dir)
            VALUES (?, ?, ?, ?)
        "#;

        self.conn.execute(
            sql,
            &[
                DbValue::from(session_id.to_string()),
                DbValue::from(intent_text),
                DbValue::from(state.as_str()),
                DbValue::from(input_dir),
            ],
        )?;

        Ok(session_id)
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: SessionId) -> Result<Option<Session>> {
        let sql = r#"
            SELECT session_id, intent_text, state, files_selected, created_at, updated_at,
                   input_dir, error_message, pending_question_id
            FROM cf_sessions
            WHERE session_id = ?
        "#;

        let row = self
            .conn
            .query_optional(sql, &[DbValue::from(session_id.to_string())])?;

        match row {
            Some(r) => Ok(Some(self.row_to_session(&r)?)),
            None => Ok(None),
        }
    }

    /// List sessions with optional state filter.
    pub fn list_sessions(&self, state: Option<IntentState>, limit: usize) -> Result<Vec<Session>> {
        let (sql, params) = match state {
            Some(s) => (
                r#"
                    SELECT session_id, intent_text, state, files_selected, created_at, updated_at,
                           input_dir, error_message, pending_question_id
                    FROM cf_sessions
                    WHERE state = ?
                    ORDER BY created_at DESC
                    LIMIT ?
                    "#
                .to_string(),
                vec![DbValue::from(s.as_str()), DbValue::from(limit as i64)],
            ),
            None => (
                r#"
                    SELECT session_id, intent_text, state, files_selected, created_at, updated_at,
                           input_dir, error_message, pending_question_id
                    FROM cf_sessions
                    ORDER BY created_at DESC
                    LIMIT ?
                    "#
                .to_string(),
                vec![DbValue::from(limit as i64)],
            ),
        };

        let rows = self.conn.query_all(&sql, &params)?;
        rows.iter().map(|r| self.row_to_session(r)).collect()
    }

    /// List sessions that need human input (at gates).
    pub fn list_sessions_needing_input(&self, limit: usize) -> Result<Vec<Session>> {
        // Gates are G1-G6
        let sql = r#"
            SELECT session_id, intent_text, state, files_selected, created_at, updated_at,
                   input_dir, error_message, pending_question_id
            FROM cf_sessions
            WHERE state IN (
                'G1_AWAITING_SELECTION_APPROVAL',
                'G2_AWAITING_TAG_RULES_APPROVAL',
                'G3_AWAITING_PATH_FIELDS_APPROVAL',
                'G4_AWAITING_SCHEMA_APPROVAL',
                'G5_AWAITING_PUBLISH_APPROVAL',
                'G6_AWAITING_RUN_APPROVAL'
            )
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let rows = self.conn.query_all(sql, &[DbValue::from(limit as i64)])?;
        rows.iter().map(|r| self.row_to_session(r)).collect()
    }

    /// Update session state with validation.
    ///
    /// Returns error if the transition is invalid.
    pub fn update_session_state(
        &self,
        session_id: SessionId,
        new_state: IntentState,
    ) -> Result<bool> {
        // First, get the current session to validate transition
        let session = self.get_session(session_id)?.context("Session not found")?;

        // Validate transition using the type system
        if !session.state.can_transition_to(new_state) {
            anyhow::bail!(
                "Invalid state transition from {} to {}",
                session.state,
                new_state
            );
        }

        let sql = r#"
            UPDATE cf_sessions
            SET state = ?, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self.conn.execute(
            sql,
            &[
                DbValue::from(new_state.as_str()),
                DbValue::from(session_id.to_string()),
            ],
        )?;

        Ok(affected > 0)
    }

    /// Update files selected count.
    pub fn update_files_selected(
        &self,
        session_id: SessionId,
        files_selected: u64,
    ) -> Result<bool> {
        let sql = r#"
            UPDATE cf_sessions
            SET files_selected = ?, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self.conn.execute(
            sql,
            &[
                DbValue::from(files_selected as i64),
                DbValue::from(session_id.to_string()),
            ],
        )?;

        Ok(affected > 0)
    }

    /// Set pending question for a session at a gate.
    pub fn set_pending_question(&self, session_id: SessionId, question_id: &str) -> Result<bool> {
        let sql = r#"
            UPDATE cf_sessions
            SET pending_question_id = ?, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self.conn.execute(
            sql,
            &[
                DbValue::from(question_id),
                DbValue::from(session_id.to_string()),
            ],
        )?;

        Ok(affected > 0)
    }

    /// Clear pending question for a session.
    pub fn clear_pending_question(&self, session_id: SessionId) -> Result<bool> {
        let sql = r#"
            UPDATE cf_sessions
            SET pending_question_id = NULL, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self
            .conn
            .execute(sql, &[DbValue::from(session_id.to_string())])?;

        Ok(affected > 0)
    }

    /// Fail a session with an error message.
    pub fn fail_session(&self, session_id: SessionId, error_message: &str) -> Result<bool> {
        // First validate the transition is allowed
        let session = self.get_session(session_id)?.context("Session not found")?;

        if !session.state.can_transition_to(IntentState::Failed) {
            anyhow::bail!("Cannot fail session in state {}", session.state);
        }

        let sql = r#"
            UPDATE cf_sessions
            SET state = 'FAILED', error_message = ?, updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self.conn.execute(
            sql,
            &[
                DbValue::from(error_message),
                DbValue::from(session_id.to_string()),
            ],
        )?;

        Ok(affected > 0)
    }

    /// Cancel a session.
    pub fn cancel_session(&self, session_id: SessionId) -> Result<bool> {
        // First validate the transition is allowed
        let session = self.get_session(session_id)?.context("Session not found")?;

        if !session.state.can_transition_to(IntentState::Cancelled) {
            anyhow::bail!("Cannot cancel session in state {}", session.state);
        }

        let sql = r#"
            UPDATE cf_sessions
            SET state = 'CANCELLED', updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self
            .conn
            .execute(sql, &[DbValue::from(session_id.to_string())])?;

        Ok(affected > 0)
    }

    /// Complete a session.
    pub fn complete_session(&self, session_id: SessionId) -> Result<bool> {
        // First validate the transition is allowed
        let session = self.get_session(session_id)?.context("Session not found")?;

        if !session.state.can_transition_to(IntentState::Completed) {
            anyhow::bail!("Cannot complete session in state {}", session.state);
        }

        let sql = r#"
            UPDATE cf_sessions
            SET state = 'COMPLETED', updated_at = CURRENT_TIMESTAMP
            WHERE session_id = ?
        "#;

        let affected = self
            .conn
            .execute(sql, &[DbValue::from(session_id.to_string())])?;

        Ok(affected > 0)
    }

    // ========================================================================
    // Row Conversion - Type-safe deserialization
    // ========================================================================

    /// Parse a database row into a Session.
    ///
    /// Column order must match the SELECT statement:
    /// 0: session_id, 1: intent_text, 2: state, 3: files_selected,
    /// 4: created_at, 5: updated_at, 6: input_dir, 7: error_message,
    /// 8: pending_question_id
    fn row_to_session(&self, row: &UnifiedDbRow) -> Result<Session> {
        // Column 0: session_id
        let session_id_str: String = row.get(0)?;
        let session_id: SessionId = session_id_str
            .parse()
            .context("Invalid session_id format")?;

        // Column 1: intent_text
        let intent_text: String = row.get(1)?;

        // Column 2: state - parse using FromStr to validate
        let state_str: String = row.get(2)?;
        let state: IntentState = state_str
            .parse()
            .map_err(|e: StateParseError| {
                anyhow::anyhow!("Invalid state in database: {} - {}", state_str, e)
            })?;

        // Column 3: files_selected
        let files_selected: i64 = row.get(3).unwrap_or(0);

        // Column 4: created_at - convert to RFC3339 string
        let created_at_ts: DbTimestamp = row.get(4)?;
        let created_at = created_at_ts.to_rfc3339();

        // Column 5: updated_at - convert to RFC3339 string
        let updated_at_ts: DbTimestamp = row.get(5)?;
        let updated_at = updated_at_ts.to_rfc3339();

        // Column 6: input_dir (optional)
        let input_dir: Option<String> = row.get(6).ok();

        // Column 7: error_message (optional)
        let error_message: Option<String> = row.get(7).ok();

        // Column 8: pending_question_id (optional)
        let pending_question_id: Option<String> = row.get(8).ok();

        Ok(Session {
            session_id,
            intent_text,
            state,
            files_selected: files_selected as u64,
            created_at,
            updated_at,
            input_dir,
            error_message,
            pending_question_id,
        })
    }
}

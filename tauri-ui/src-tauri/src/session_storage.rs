//! Session Storage Layer.
//!
//! Manages Intent Pipeline sessions in DuckDB.
//! Uses typed IntentState enum - no stringly-typed state representation.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbTimestamp, DbValue, UnifiedDbRow};

use crate::session_types::{IntentState, Session, SessionId};

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
        let state_values = IntentState::ALL
            .iter()
            .map(|state| format!("'{}'", state.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let default_state = IntentState::InterpretIntent.as_str();

        let create_sql = format!(
            r#"
            -- Intent Pipeline Sessions table
            CREATE TABLE IF NOT EXISTS cf_sessions (
                session_id TEXT PRIMARY KEY,
                intent_text TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT '{default_state}' CHECK (state IN ({state_values})),
                files_selected BIGINT NOT NULL DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                input_dir TEXT,
                error_message TEXT
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
                   input_dir, error_message
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
                           input_dir, error_message
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
                           input_dir, error_message
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
        let gate_values = IntentState::GATES
            .iter()
            .map(|state| format!("'{}'", state.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
            SELECT session_id, intent_text, state, files_selected, created_at, updated_at,
                   input_dir, error_message
            FROM cf_sessions
            WHERE state IN ({gate_values})
            ORDER BY created_at DESC
            LIMIT ?
        "#,
            gate_values = gate_values
        );

        let rows = self.conn.query_all(&sql, &[DbValue::from(limit as i64)])?;
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
    /// 4: created_at, 5: updated_at, 6: input_dir, 7: error_message
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
        let state: IntentState =
            state_str
                .parse()
                .map_err(|e: crate::session_types::StateParseError| {
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

        Ok(Session {
            session_id,
            intent_text,
            state,
            files_selected: files_selected as u64,
            created_at,
            updated_at,
            input_dir,
            error_message,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use casparian_db::DbConnection;

    fn setup_storage() -> SessionStorage {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let storage = SessionStorage::new(conn);
        storage.init_schema().unwrap();
        storage
    }

    #[test]
    fn test_create_and_get_session() {
        let storage = setup_storage();

        let session_id = storage
            .create_session("Process CSV files", Some("/data/input"))
            .unwrap();

        let session = storage.get_session(session_id).unwrap().unwrap();

        assert_eq!(session.session_id, session_id);
        assert_eq!(session.intent_text, "Process CSV files");
        assert_eq!(session.state, IntentState::InterpretIntent);
        assert_eq!(session.input_dir, Some("/data/input".to_string()));
        assert_eq!(session.files_selected, 0);
    }

    #[test]
    fn test_list_sessions() {
        let storage = setup_storage();

        storage.create_session("Session 1", None).unwrap();
        storage.create_session("Session 2", None).unwrap();
        storage.create_session("Session 3", None).unwrap();

        let sessions = storage.list_sessions(None, 10).unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_update_state_valid_transition() {
        let storage = setup_storage();
        let session_id = storage.create_session("Test", None).unwrap();

        // Valid: S0 -> S1
        assert!(storage
            .update_session_state(session_id, IntentState::ScanCorpus)
            .is_ok());

        let session = storage.get_session(session_id).unwrap().unwrap();
        assert_eq!(session.state, IntentState::ScanCorpus);
    }

    #[test]
    fn test_update_state_invalid_transition() {
        let storage = setup_storage();
        let session_id = storage.create_session("Test", None).unwrap();

        // Invalid: S0 -> Completed (must go through all states)
        let result = storage.update_session_state(session_id, IntentState::Completed);
        assert!(result.is_err());

        // State should be unchanged
        let session = storage.get_session(session_id).unwrap().unwrap();
        assert_eq!(session.state, IntentState::InterpretIntent);
    }

    #[test]
    fn test_fail_session() {
        let storage = setup_storage();
        let session_id = storage.create_session("Test", None).unwrap();

        storage
            .fail_session(session_id, "Something went wrong")
            .unwrap();

        let session = storage.get_session(session_id).unwrap().unwrap();
        assert_eq!(session.state, IntentState::Failed);
        assert_eq!(
            session.error_message,
            Some("Something went wrong".to_string())
        );
    }

    #[test]
    fn test_list_sessions_needing_input() {
        let storage = setup_storage();

        // Create sessions at different states
        let s1 = storage.create_session("Session 1", None).unwrap();
        let _s2 = storage.create_session("Session 2", None).unwrap();

        // Move s1 to a gate
        storage
            .update_session_state(s1, IntentState::ScanCorpus)
            .unwrap();
        storage
            .update_session_state(s1, IntentState::ProposeSelection)
            .unwrap();
        storage
            .update_session_state(s1, IntentState::AwaitingSelectionApproval)
            .unwrap();

        // s2 stays at S0

        let needing_input = storage.list_sessions_needing_input(10).unwrap();
        assert_eq!(needing_input.len(), 1);
        assert_eq!(needing_input[0].session_id, s1);
        assert!(needing_input[0].state.is_gate());
    }

    #[test]
    fn test_cannot_transition_from_terminal() {
        let storage = setup_storage();
        let session_id = storage.create_session("Test", None).unwrap();

        // Fail the session
        storage.fail_session(session_id, "Error").unwrap();

        // Cannot transition from Failed
        let result = storage.update_session_state(session_id, IntentState::ScanCorpus);
        assert!(result.is_err());
    }
}

//! Intent Pipeline: Non-brittle MCP orchestration for intent → pipeline workflows.
//!
//! This module implements the Intent Pipeline workflow as specified in
//! `docs/intent_pipeline_workflow.md`. It provides:
//!
//! - Session management with full decision history
//! - FileSet storage with bounded wire payloads
//! - Deterministic confidence scoring
//! - Approval token binding for irreversible actions
//! - State machine for workflow progression

pub mod confidence;
pub mod fileset;
pub mod session;
pub mod state;
pub mod types;

pub use confidence::ConfidenceScore;
pub use fileset::FileSetStore;
pub use session::FileSetEntry;
pub use session::{SessionBundle, SessionStore};
pub use state::{IntentState, StateMachine, StateTransition};
pub use types::ConfidenceLabel;
pub use types::*;

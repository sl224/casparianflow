//! Casparian Flow state store (control-plane persistence).
//!
//! Provides semantic storage APIs for queues, routing, sessions, and scans.

// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::get_first)]
#![allow(dead_code)]

pub mod api_storage;
pub mod expected_outputs;
pub mod legacy_models;
pub mod models;
pub mod queue;
pub mod schema_version;
pub mod sessions;
pub mod state_store;

pub use api_storage::ApiStorage;
pub use casparian_intent::{
    IntentState, QuestionKind, QuestionOption, Session, SessionId, SessionQuestion,
};
pub use expected_outputs::{ExpectedOutputs, OutputSpec};
pub use queue::{Job, JobQueue, QueueStats};
pub use schema_version::{ensure_schema_version, SCHEMA_VERSION};
pub use sessions::SessionStorage;
pub use state_store::{
    ApiStore, ArtifactStore, DispatchData, JobArtifactRecord, PluginDeployRequest, QueueStore,
    RoutingStore, ScoutStore, ScoutTagCount, ScoutTagStats, SessionStore, StateStore,
    StateStoreBackend, StateStoreQueueSession, StateStoreScoutSession, StateStoreUrl,
};

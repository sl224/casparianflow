// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unwrap_or_default)]

//! MCP (Model Context Protocol) Server for Casparian Flow
//!
//! This crate implements an MCP server that exposes Casparian's core capabilities
//! as MCP tools, enabling AI assistants (Claude, etc.) to interact with Casparian
//! programmatically.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    AI Assistant (Claude)                         │
//! └─────────────────────────────────────────────────────────────────┘
//!                               │
//!                               │ MCP Protocol (JSON-RPC over stdio)
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                 crates/casparian_mcp/                            │
//! │                                                                  │
//! │  ┌──────────────────────────────────────────────────────────┐   │
//! │  │                    Core Subsystems                        │   │
//! │  ├──────────────────────────────────────────────────────────┤   │
//! │  │  Server       │ JSON-RPC stdio, tool dispatch            │   │
//! │  │  Jobs         │ Async job lifecycle (start/status/cancel)│   │
//! │  │  Approvals    │ Non-blocking approval requests           │   │
//! │  │  Security     │ Path allowlist, output budgets, redaction│   │
//! │  └──────────────────────────────────────────────────────────┘   │
//! │                                                                  │
//! │  ┌──────────────────────────────────────────────────────────┐   │
//! │  │                    Tool Implementations                   │   │
//! │  ├──────────────────────────────────────────────────────────┤   │
//! │  │  Discovery    │ scan, plugins                            │   │
//! │  │  Preview      │ preview (read-only)                      │   │
//! │  │  Jobs         │ backtest_start, run_request, job_*       │   │
//! │  │  Query        │ query (read-only sandbox)                │   │
//! │  │  Schema       │ schema_propose, schema_promote           │   │
//! │  │  Approvals    │ approval_status, approval_list           │   │
//! │  └──────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Design Principles
//!
//! 1. **Job-first architecture:** Long-running operations return immediately with
//!    a `job_id`; progress is polled via separate tools.
//!
//! 2. **Non-blocking approvals:** Write operations create approval requests;
//!    humans approve out-of-band via CLI.
//!
//! 3. **Read-only by default:** Query tool uses read-only DuckDB connection;
//!    samples are redacted by default.
//!
//! 4. **Security from day one:** Path allowlists, output budgets, and audit
//!    logging are P0, not afterthoughts.
//!
//! 5. **Per-output schemas:** Multi-output parsers are first-class; schemas
//!    are always keyed by output name.

pub mod protocol;
pub mod server;
pub mod types;

pub mod approvals;
pub mod db_store;
pub mod jobs;
pub mod redaction;
pub mod security;
pub mod tools;

// Sync Core - single-owner state management (Phase 1B)
pub mod core;

// Intent Pipeline (non-brittle MCP orchestration)
pub mod intent;

// Re-exports for convenience
pub use approvals::{ApprovalId, ApprovalManager, ApprovalRequest, ApprovalStatus};
pub use db_store::{DbApprovalStore, DbJobStore};
pub use jobs::{JobId, JobManager, JobProgress, JobState};

// Core module re-exports (Phase 1B - sync architecture)
pub use core::{
    spawn_core, spawn_core_with_config, CancellationToken, Command, Core, CoreConfig, CoreHandle,
    Event, Responder,
};
pub use protocol::{ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use security::{OutputBudget, PathAllowlist, SecurityConfig};
pub use server::{McpServer, McpServerConfig};
pub use types::{
    ApprovalDecision, ApprovalStatusFilter, ColumnDefinition, DataType, JobStatusFilter, PluginRef,
    RedactionPolicy, SchemaDefinition, SchemaMode, SimpleDataType, ViolationContext,
};

// Intent pipeline re-exports
pub use intent::{
    ConfidenceScore, FileSetId, FileSetStore, IntentState, ProposalId, SessionBundle, SessionId,
    SessionStore, StateMachine,
};

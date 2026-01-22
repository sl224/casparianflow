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

pub mod security;
pub mod jobs;
pub mod approvals;
pub mod tools;

// Re-exports for convenience
pub use protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError, ErrorCode};
pub use server::{McpServer, McpServerConfig};
pub use types::{PluginRef, RedactionPolicy, ViolationContext};
pub use security::{SecurityConfig, PathAllowlist, OutputBudget};
pub use jobs::{JobManager, JobId, JobState, JobProgress};
pub use approvals::{ApprovalManager, ApprovalId, ApprovalRequest, ApprovalStatus};

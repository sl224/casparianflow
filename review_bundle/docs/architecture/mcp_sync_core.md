# MCP Sync Core Architecture

## Overview

This document describes the architecture for removing async/tokio from the MCP subsystem and replacing it with a synchronous, message-passing core using explicit threads and channels (Jon Blow model).

## Current State (Problems)

The current MCP implementation has several issues:

1. **Async/Tokio throughout**: The server, job executor, and tools use async functions and tokio runtime
2. **Arc<Mutex<...>> pattern**: JobManager and ApprovalManager are wrapped in Arc<Mutex<>> for sharing
3. **Tokio channels**: tokio::sync::mpsc and tokio::sync::watch for job queuing and cancellation
4. **Lock contention**: Multiple async tasks lock the same managers

### Current Architecture
```
┌─────────────────────────────────────────────────────────────┐
│                     McpServer                                │
│                                                             │
│  ┌─────────────────┐   ┌────────────────────────────────┐  │
│  │ Arc<Mutex<      │   │ Arc<Mutex<                     │  │
│  │   JobManager>>  │   │   ApprovalManager>>            │  │
│  └─────────────────┘   └────────────────────────────────┘  │
│           │                       │                         │
│           ├───────────────────────┴─────────────────────┐  │
│           ▼                                             ▼  │
│  ┌─────────────────────────────────────────────────────────┐
│  │              Tokio Runtime (async .await)                │
│  │                                                         │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │  │ Tool Task 1 │  │ Tool Task 2 │  │ JobExecutor     │ │
│  │  │ (async)     │  │ (async)     │  │ (tokio::spawn)  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────┘ │
│  └─────────────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────────┘
```

## Target State (Solution)

Replace with a single-owner Core thread that owns all mutable state, receiving Commands and emitting Events via bounded channels.

### Core Invariants

1. **One thread owns mutable state**: The Core thread exclusively owns JobManager, ApprovalManager, and DB access
2. **Workers do compute; Core does state transitions**: Job execution happens in worker threads, state changes flow through Core
3. **Cancellation is token-based**: No "cancel by lock" - use CancellationToken pattern
4. **No async/await in MCP path**: All MCP code is synchronous

### Target Architecture
```
┌─────────────────────────────────────────────────────────────┐
│                     McpServer                                │
│                                                             │
│  ┌─────────────────────────────────────────────────────────┐
│  │                    Core Thread                           │
│  │  (single-owner of JobManager, ApprovalManager, DB)      │
│  │                                                         │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │  │ JobManager  │  │ ApprovalMgr │  │ DbConnection│     │
│  │  │ (owned)     │  │ (owned)     │  │ (owned)     │     │
│  │  └─────────────┘  └─────────────┘  └─────────────┘     │
│  └─────────────────────────────────────────────────────────┘
│           ▲                              │                  │
│           │ Commands                     │ Events           │
│           │ (mpsc::channel)              │ (mpsc::channel)  │
│           │                              ▼                  │
│  ┌─────────────────────────────────────────────────────────┐
│  │              Request Handler Thread                      │
│  │  (stdin reader, dispatches Commands, returns Responses) │
│  └─────────────────────────────────────────────────────────┘
│                                                             │
│  ┌─────────────────────────────────────────────────────────┐
│  │              Worker Threads (fixed pool)                 │
│  │                                                         │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │  │ Worker 1    │  │ Worker 2    │  │ Worker N    │     │
│  │  │ (compute)   │  │ (compute)   │  │ (compute)   │     │
│  │  └─────────────┘  └─────────────┘  └─────────────┘     │
│  └─────────────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────────┘
```

## Message Types

### Command (from tool calls / RPC → Core)
```rust
pub enum Command {
    // Job lifecycle
    CreateJob { spec: JobSpec, respond: Responder<Result<JobId>> },
    GetJob { id: JobId, respond: Responder<Result<Option<Job>>> },
    StartJob { id: JobId, respond: Responder<Result<()>> },
    UpdateProgress { id: JobId, progress: JobProgress, respond: Responder<Result<()>> },
    CompleteJob { id: JobId, result: JobResult, respond: Responder<Result<()>> },
    FailJob { id: JobId, error: String, respond: Responder<Result<()>> },
    CancelJob { id: JobId, respond: Responder<Result<bool>> },
    ListJobs { status: Option<JobState>, limit: usize, respond: Responder<Result<Vec<Job>>> },

    // Approval lifecycle
    CreateApproval { request: ApprovalRequest, respond: Responder<Result<ApprovalId>> },
    GetApproval { id: ApprovalId, respond: Responder<Result<Option<Approval>>> },
    ApproveRequest { id: ApprovalId, by: Option<String>, respond: Responder<Result<bool>> },
    RejectRequest { id: ApprovalId, by: Option<String>, reason: Option<String>, respond: Responder<Result<bool>> },
    ListApprovals { status: Option<ApprovalStatus>, limit: usize, respond: Responder<Result<Vec<Approval>>> },

    // Query (read-only)
    Query { sql: String, limit: usize, respond: Responder<Result<QueryResult>> },

    // Shutdown
    Shutdown,
}

/// One-shot channel for returning results
pub type Responder<T> = std::sync::mpsc::Sender<T>;
```

### Event (from Core → subscribers)
```rust
pub enum Event {
    JobCreated { job_id: JobId },
    JobStarted { job_id: JobId },
    JobProgress { job_id: JobId, progress: JobProgress },
    JobCompleted { job_id: JobId },
    JobFailed { job_id: JobId, error: String },
    JobCancelled { job_id: JobId },
    ApprovalCreated { approval_id: ApprovalId },
    ApprovalDecided { approval_id: ApprovalId, approved: bool },
}
```

## Job Lifecycle State Machine

```
                    ┌──────────┐
                    │  Queued  │ ──(timeout)─── ▶ Failed
                    └────┬─────┘
                         │ start
                         ▼
                    ┌──────────┐
         ┌──────────│ Running  │──────────┐
         │          └────┬─────┘          │
         │ cancel        │ progress       │ error
         │               ▼                │
         │          ┌──────────┐          │
         │          │ Running  │          │
         │          │(progress)│          │
         │          └────┬─────┘          │
         │               │ complete       │
         ▼               ▼                ▼
    ┌──────────┐   ┌──────────┐    ┌──────────┐
    │Cancelled │   │Completed │    │  Failed  │
    └──────────┘   └──────────┘    └──────────┘
         │               │                │
         └───────────────┴────────────────┘
                  Terminal States
```

### State Transitions (enforced in Core)
```rust
impl JobState {
    pub fn can_transition_to(&self, target: &JobState) -> bool {
        match (self, target) {
            (Queued, Running) => true,
            (Queued, Cancelled) => true,
            (Queued, Failed) => true,  // timeout
            (Running, Completed) => true,
            (Running, Failed) => true,
            (Running, Cancelled) => true,
            _ => false,  // Terminal states cannot transition
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Completed | Failed | Cancelled)
    }
}
```

## Core Implementation

```rust
pub struct Core {
    // Owned state (no Arc, no Mutex)
    job_manager: JobManager,
    approval_manager: ApprovalManager,
    db_conn: DbConnection,

    // Channels
    commands: Receiver<Command>,
    events: Sender<Event>,

    // Cancellation tokens for active jobs
    cancel_tokens: HashMap<JobId, CancellationToken>,
}

impl Core {
    pub fn run(&mut self) {
        loop {
            match self.commands.recv() {
                Ok(Command::Shutdown) => break,
                Ok(cmd) => self.handle_command(cmd),
                Err(_) => break, // Channel closed
            }
        }
    }

    fn handle_command(&mut self, cmd: Command) -> Vec<Effect> {
        match cmd {
            Command::CreateJob { spec, respond } => {
                let result = self.job_manager.create_job(spec);
                if let Ok(job_id) = &result {
                    let _ = self.events.send(Event::JobCreated { job_id: *job_id });
                }
                let _ = respond.send(result);
            }
            // ... other commands
        }
    }
}
```

## Commit Sequence

### Commit 1 - Introduce Core Types (no behavioral changes)
**Files:**
- `crates/casparian_mcp/src/core/mod.rs` (new)
- `crates/casparian_mcp/src/core/command.rs` (new)
- `crates/casparian_mcp/src/core/event.rs` (new)

**Content:**
- Define `Core` struct with owned fields
- Define `Command` enum with all variants
- Define `Event` enum
- Define `Responder<T>` type alias
- No changes to existing code

### Commit 2 - Move State Ownership into Core
**Files:**
- `crates/casparian_mcp/src/core/mod.rs`
- `crates/casparian_mcp/src/server.rs`
- `crates/casparian_mcp/src/tools/*.rs`

**Changes:**
- Core takes ownership of JobManager, ApprovalManager
- Server sends Commands to Core instead of locking managers
- Tools use Command channel for all state operations
- Remove Arc<Mutex<...>> wrappers

### Commit 3 - Replace Tokio with Blocking Loop
**Files:**
- `crates/casparian_mcp/src/server.rs`
- `crates/casparian_mcp/src/jobs/executor.rs`
- `crates/casparian_mcp/src/core/mod.rs`

**Changes:**
- Replace `tokio::spawn` with `std::thread::spawn`
- Replace `tokio::sync::mpsc` with `std::sync::mpsc` (bounded)
- Replace `tokio::sync::watch` with `CancellationToken` pattern
- Server main loop: `stdin.lines()` instead of async read
- Worker pool: fixed threads with crossbeam_channel

### Commit 4 - Delete Tokio Dependencies
**Files:**
- `crates/casparian_mcp/Cargo.toml`
- Verify: `rg "(tokio::|async fn|\.await)" crates/casparian_mcp` → 0 matches

**Changes:**
- Remove `tokio` from dependencies
- Remove `async-trait` if no longer needed
- Update any remaining async function signatures

## Acceptance Criteria

1. **No async/await in MCP path**
   - `rg "(tokio::|async fn|\.await)" crates/casparian_mcp` → 0 matches

2. **Locks eliminated or minimized**
   - No `Arc<Mutex<JobManager>>` or `Arc<Mutex<ApprovalManager>>`
   - Any remaining locks have documented invariants

3. **Tests pass**
   - `cargo test -p casparian_mcp`
   - Existing E2E tests in `tests/e2e/mcp/`

4. **No behavioral changes**
   - MCP tools work identically from Claude's perspective
   - Job lifecycle unchanged
   - Approval workflow unchanged

## Risks and Mitigations

### Risk: Deadlock in Command/Response
**Mitigation:** Use bounded channels with reasonable capacity. Timeout on response wait.

### Risk: Worker thread starvation
**Mitigation:** Fixed worker pool size based on CPU cores. Queue depth monitoring.

### Risk: Breaking API contracts
**Mitigation:** Keep all tool interfaces identical. Only internal architecture changes.

## No Migrations Note

Per pre-v1 rules: if the job/approval persistence schema needs changes, delete the DB. No migration code.

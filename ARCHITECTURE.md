# Casparian Flow Architecture Guide (v7.0 - MCP-First)

A comprehensive mental model for the Casparian Flow system.

---

## High-Level Overview

Casparian Flow is a **data processing platform** that transforms "dark data" (files on disk) into structured, queryable datasets (SQL/Parquet). The system is designed for AI-assisted workflows via the Model Context Protocol (MCP).

### Core Principles

1. **Schema = Intent, then Contract**: Approved schemas become immutable contracts
2. **Elimination-Based Type Inference**: Prove types by eliminating impossibilities
3. **Fail-Fast Backtest**: Test high-failure files first for rapid feedback
4. **Bridge Mode Execution**: Host/Guest privilege separation for security
5. **MCP-First Integration**: AI-assisted data processing via Claude Code

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           CLAUDE CODE (MCP Client)                              │
│                     "Scan these CSV files and create a parser"                  │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        │ MCP Protocol (JSON-RPC)
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           CASPARIAN MCP SERVER                                  │
│                                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐                  │
│  │   DISCOVERY     │  │     SCHEMA      │  │    BACKTEST     │                  │
│  │  quick_scan     │  │ discover_schemas│  │  run_backtest   │                  │
│  │  apply_scope    │  │ approve_schemas │  │  fix_parser     │                  │
│  │                 │  │ propose_amend   │  │                 │                  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘                  │
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐    │
│  │                          EXECUTION                                      │    │
│  │              execute_pipeline     query_output                          │    │
│  └─────────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
              ┌─────────────────────────┼─────────────────────────┐
              ▼                         ▼                         ▼
┌─────────────────────────┐ ┌─────────────────────────┐ ┌─────────────────────────┐
│   SCHEMA CONTRACT       │ │   BACKTEST ENGINE       │ │   TYPE INFERENCE        │
│   SYSTEM                │ │                         │ │   ENGINE                │
│                         │ │                         │ │                         │
│  - LockedSchema         │ │  - High-failure table   │ │  - ConstraintSolver     │
│  - SchemaContract       │ │  - Fail-fast ordering   │ │  - Elimination logic    │
│  - Approval workflow    │ │  - Plateau detection    │ │  - Date format detect   │
│  - Amendment process    │ │  - Iteration loop       │ │  - Streaming inference  │
└─────────────────────────┘ └─────────────────────────┘ └─────────────────────────┘
              │                         │                         │
              └─────────────────────────┼─────────────────────────┘
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              WORKER (BRIDGE MODE)                               │
│                                                                                 │
│  ┌─────────────────────────────────┐    ┌─────────────────────────────────────┐ │
│  │         HOST PROCESS            │    │         GUEST PROCESS               │ │
│  │                                 │    │         (isolated venv)             │ │
│  │  - Credentials, secrets         │◀──▶│  - Plugin code only                 │ │
│  │  - Heavy drivers (pyodbc)       │    │  - pandas, pyarrow                  │ │
│  │  - Sink writers                 │    │  - No credentials                   │ │
│  │                                 │    │                                     │ │
│  └─────────────────────────────────┘    └─────────────────────────────────────┘ │
│                    │                                   │                        │
│                    │ AF_UNIX Socket                    │                        │
│                    │ Arrow IPC Batches                 │                        │
└────────────────────┼───────────────────────────────────┼────────────────────────┘
                     ▼                                   │
          ┌────────────────────┐                         │
          │    OUTPUT SINKS    │◀────────────────────────┘
          │  Parquet / SQLite  │
          │  CSV / PostgreSQL  │
          └────────────────────┘
```

---

## Directory Structure

```
casparian-flow/
├── CLAUDE.md                 # Entry point for LLM context
├── ARCHITECTURE.md           # This file
├── README.md                 # Quick start
│
├── crates/                   # Rust core
│   ├── casparian/            # Unified CLI binary
│   │   └── src/
│   │       ├── main.rs       # CLI entry (start, publish, scout)
│   │       ├── runtime.rs    # Split Tokio runtime
│   │       └── server.rs     # Sentinel server
│   │
│   ├── casparian_mcp/        # MCP Server
│   │   ├── CLAUDE.md
│   │   └── src/
│   │       ├── tools/        # 9 MCP tools
│   │       ├── server.rs     # JSON-RPC server
│   │       ├── protocol.rs   # MCP protocol types
│   │       └── types.rs      # Shared types
│   │
│   ├── casparian_schema/     # Schema Contracts
│   │   ├── CLAUDE.md
│   │   └── src/
│   │       ├── contract.rs   # LockedSchema, SchemaContract
│   │       ├── approval.rs   # Approval workflow
│   │       ├── amendment.rs  # Schema evolution
│   │       └── storage.rs    # SQLite persistence
│   │
│   ├── casparian_backtest/   # Backtest Engine
│   │   ├── CLAUDE.md
│   │   └── src/
│   │       ├── high_failure.rs  # Failure tracking
│   │       ├── failfast.rs      # Early termination
│   │       ├── loop_.rs         # Iteration loop
│   │       └── metrics.rs       # Pass rate, plateau
│   │
│   ├── casparian_worker/     # Worker + Type Inference
│   │   ├── CLAUDE.md
│   │   └── src/
│   │       ├── type_inference/  # Constraint solver
│   │       ├── bridge.rs        # Host/Guest comm
│   │       ├── venv_manager.rs  # UV environments
│   │       └── worker.rs        # Job execution
│   │
│   ├── casparian_scout/      # File Discovery
│   │   ├── CLAUDE.md
│   │   └── src/
│   │       ├── db.rs         # SQLite state
│   │       ├── scanner.rs    # Filesystem walking
│   │       └── router.rs     # Pattern → tag
│   │
│   ├── cf_security/          # Auth + Signing
│   │   └── src/
│   │       ├── azure.rs      # Azure AD integration
│   │       ├── signing.rs    # Ed25519 signatures
│   │       └── gatekeeper.rs # AST validation
│   │
│   └── cf_protocol/          # Binary Protocol
│       └── src/
│           └── lib.rs        # OpCodes, serialization
│
├── ui/                       # Tauri Desktop App
│   ├── CLAUDE.md
│   ├── src/                  # SvelteKit frontend
│   │   ├── routes/
│   │   └── lib/
│   │       ├── components/
│   │       │   ├── parser-lab/
│   │       │   ├── scout/
│   │       │   └── shredder/  # deprecated
│   │       └── stores/
│   └── src-tauri/            # Rust backend
│       └── src/
│           ├── lib.rs
│           └── scout.rs
│
└── demo/                     # Examples
    ├── plugins/
    ├── scout/
    └── samples/
```

---

## Core Subsystems

### 1. MCP Server (casparian_mcp)

Provides 9 tools for Claude Code integration:

| Category | Tools | Purpose |
|----------|-------|---------|
| Discovery | `quick_scan`, `apply_scope` | Find and group files |
| Schema | `discover_schemas`, `approve_schemas`, `propose_amendment` | Schema lifecycle |
| Backtest | `run_backtest`, `fix_parser` | Validate parsers |
| Execution | `execute_pipeline`, `query_output` | Run and query |

**Protocol**: JSON-RPC 2.0 over stdio

```json
// Tool invocation
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "quick_scan",
    "arguments": { "path": "/data", "extensions": ["csv"] }
  }
}
```

### 2. Schema Contract System (casparian_schema)

**Lifecycle:**
```
Discovery → Review → APPROVAL → Contract (Immutable)
                                    │
                                    ├── Enforcement (violations = failures)
                                    │
                                    └── Amendment (controlled evolution)
```

**Key Types:**
- `SchemaContract`: Binding agreement with version tracking
- `LockedSchema`: Immutable schema definition
- `LockedColumn`: Column with type, nullability, format
- `SchemaViolation`: Failure when contract is broken

**Principle:** No silent fallbacks. Violations are failures.

### 3. Type Inference Engine (casparian_worker)

**Algorithm:** Constraint-based elimination

```
Start with all possible types
For each value:
    Eliminate impossible interpretations
Until:
    One type remains (resolved)
    OR all eliminated (contradiction)
    OR data ends (ambiguous)
```

**Example:**
```
"15/06/24" → Could be DD/MM/YY or MM/DD/YY
"31/05/24" → PROVES DD/MM/YY (31 > 12, can't be month)
```

**Key Types:**
- `ConstraintSolver`: Per-column type resolver
- `DataType`: Null, Boolean, Integer, Float, Date, DateTime, Time, Duration, String
- `EliminationEvidence`: Why a type was ruled out

### 4. Backtest Engine (casparian_backtest)

**Algorithm:** Fail-fast with high-failure tracking

```
1. Get all files in scope
2. Sort: high-failure → resolved → untested → passing
3. For each file:
   - Run parser
   - Update high-failure table
   - Check early termination
4. Calculate metrics
5. Continue or stop based on:
   - Pass rate achieved
   - Plateau detected
   - Max iterations reached
```

**Key Types:**
- `HighFailureTable`: SQLite-backed failure tracking
- `FailureHistoryEntry`: Individual failure with context
- `BacktestResult`: Complete or EarlyStopped with metrics

### 5. Bridge Mode Execution (casparian_worker)

**Architecture:**
```
Worker (Host)         Guest Process
     │                     │
     │ AF_UNIX Socket      │
     │ ─────────────────── │
     │                     │
     │ Credentials         │ Plugin code
     │ Heavy drivers       │ pandas, pyarrow
     │ Sink writers        │ No secrets
     │                     │
     ▼                     ▼
 Write to Sinks      Stream Arrow IPC
```

**Why?**
- Security: Guest has no credentials
- Isolation: Plugin crashes don't affect host
- Dependencies: Each plugin has its own venv

### 6. Virtual Environment Management

All Python environments use [UV](https://github.com/astral-sh/uv):

```
~/.casparian_flow/venvs/
├── {env_hash_1}/    # Content-addressable by lockfile
├── {env_hash_2}/
└── ...
```

**Features:**
- Reproducible: `uv.lock` is the source of truth
- Fast: UV is much faster than pip
- Cached: Identical lockfiles share venvs
- LRU Eviction: Old venvs cleaned up

---

## Data Flow: Complete Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 1. DISCOVERY                                                                    │
│    quick_scan("/data", extensions=["csv"]) → 42 files                           │
│    apply_scope(files, "transactions") → scope_id                                │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 2. SCHEMA DISCOVERY                                                             │
│    discover_schemas(scope_id) → [LockedSchema(columns=[id, amount, date])]      │
│                                                                                 │
│    Type Inference:                                                              │
│    - id: values [1, 2, 3] → Integer                                             │
│    - amount: values [100, 150.50] → Float (decimal eliminated Integer)          │
│    - date: values [31/05/24] → Date, format DD/MM/YY (31 > 12 proves it)        │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 3. APPROVAL                                                                     │
│    User reviews schema, clicks "Approve"                                        │
│    approve_schemas(scope_id, approved_by="user@example.com")                    │
│                                                                                 │
│    → SchemaContract created (immutable, versioned)                              │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 4. BACKTEST                                                                     │
│    run_backtest(scope_id, pass_threshold=0.95)                                  │
│                                                                                 │
│    Order: high_failure_1.csv → high_failure_2.csv → untested.csv → passing.csv  │
│                                                                                 │
│    If high-failure files still fail → EARLY STOP                                │
│    If pass rate achieved → COMPLETE                                             │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 5. EXECUTION                                                                    │
│    execute_pipeline(scope_id, sink="parquet:///output/transactions.parquet")    │
│                                                                                 │
│    Bridge Mode:                                                                 │
│    Host → spawn Guest in venv                                                   │
│    Guest → read files, stream Arrow batches                                     │
│    Host → receive batches, write to sink                                        │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ 6. QUERY                                                                        │
│    query_output(scope_id, sql="SELECT * FROM output WHERE amount > 100")        │
│                                                                                 │
│    → Results ready for analysis                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Database Architecture

### Single Database Rule

**All data in ONE database:** `~/.casparian_flow/casparian_flow.sqlite3`

```
~/.casparian_flow/
├── casparian_flow.sqlite3    # THE database
├── venvs/                    # Content-addressable venv cache
├── parsers/                  # Deployed .py files
├── output/                   # Parquet, CSV output
└── samples/                  # Demo files
```

### Table Prefixes

| Prefix | Purpose |
|--------|---------|
| `parser_lab_*` | Parser development |
| `scout_*` | File discovery, tagging |
| `cf_*` | Sentinel, execution |
| `schema_*` | Schema contracts |
| `backtest_*` | High-failure tracking |

---

## Protocol v5 (OpCodes)

Binary header: `!BBHQI` (16 bytes)

| OpCode | Name | Direction | Purpose |
|--------|------|-----------|---------|
| 1 | IDENTIFY | Worker → Sentinel | Handshake |
| 2 | DISPATCH | Sentinel → Worker | Job command |
| 3 | ABORT | Sentinel → Worker | Cancel |
| 4 | HEARTBEAT | Worker → Sentinel | Status |
| 5 | CONCLUDE | Worker → Sentinel | Job done |
| 6 | ERR | Both | Error |
| 8 | PREPARE_ENV | Sentinel → Worker | Setup venv |
| 9 | ENV_READY | Worker → Sentinel | Venv ready |
| 10 | DEPLOY | Publisher → Sentinel | Deploy artifact |

---

## Security Architecture

### Local Mode (Development)
- Zero friction, auto-generated Ed25519 keys
- Implicit trust for rapid iteration

### Enterprise Mode (Production)
- Azure AD integration (raw HTTP, no SDK)
- JWT validation, audit trails
- Device Code Flow for CLI auth

### Artifact Security
1. **Signing**: Ed25519 signatures on all artifacts
2. **Hashing**: SHA-256 of source + lockfile
3. **Gatekeeper**: AST validation (banned imports)
4. **Isolation**: Guest has no credentials

---

## Testing Strategy

### E2E Tests (No Mocks)

All E2E tests use real databases and real files:

```bash
# Type Inference (25 tests)
cargo test --package casparian_worker --test e2e_type_inference

# Schema Contracts (24 tests)
cargo test --package casparian_schema --test e2e_contracts

# Backtest Engine (14 tests)
cargo test --package casparian_backtest --test e2e_backtest

# MCP Tools (20 tests)
cargo test --package casparian_mcp --test e2e_tools
```

### UI E2E Tests

```bash
cd ui && bun run test:e2e
```

---

## Key Decisions

### ADR-001: Schema as Contract
Approved schemas are immutable. Violations fail the job.
*Why:* Data quality and trust.

### ADR-002: Elimination-Based Inference
Prove types by eliminating impossibilities, not by voting.
*Why:* Certainty when possible, explicit ambiguity otherwise.

### ADR-003: Fail-Fast Backtest
Test high-failure files first.
*Why:* Rapid feedback during development.

### ADR-004: Bridge Mode Only
All plugins in isolated subprocesses.
*Why:* Security and reproducibility.

### ADR-005: MCP-First Integration
Claude Code integration via standard protocol.
*Why:* AI-assisted workflows without custom tooling.

### ADR-006: UV for Environments
UV instead of pip for venv management.
*Why:* Speed and reproducibility.

---

## Glossary

| Term | Definition |
|------|------------|
| **MCP** | Model Context Protocol - LLM tool standard |
| **Schema Contract** | Immutable, approved schema definition |
| **High-Failure File** | File that historically fails during backtest |
| **Constraint Solver** | Type inference via elimination |
| **Bridge Mode** | Host/Guest execution for isolation |
| **Scope** | Group of files for processing |
| **Amendment** | Controlled schema evolution |
| **Sentinel** | Job orchestration service |
| **Guest** | Isolated subprocess running plugin |
| **Host** | Worker process with credentials |

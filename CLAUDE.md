# Claude Code Instructions for Casparian Flow

## Quick Orientation

**What is Casparian Flow?** A data processing platform that transforms "dark data" (files on disk) into queryable datasets. Users discover files, approve schemas, and execute pipelines - all with AI assistance via MCP (Model Context Protocol).

**Start Here:**
1. Read this file for high-level architecture
2. See `ARCHITECTURE.md` for detailed system design
3. Check crate-specific `CLAUDE.md` files for component details

---

## The North Star

**Transform "dark data" into queryable datasets with zero friction.**

Users have files (CSVs, JSON, logs) scattered across their systems. They want to:
1. **Discover** files automatically (Scout)
2. **Parse** them into structured data (Parser Lab + Plugins)
3. **Query** the results (SQL/Parquet)

The entire system should feel like "drag and drop your messy files, get clean data."

---

## System Architecture (v7.0 - MCP-First)

```
                              ┌─────────────────────────────────────┐
                              │        Claude Code (MCP Client)     │
                              │   "Scan these files for me"         │
                              └──────────────────┬──────────────────┘
                                                 │ MCP Protocol
                                                 ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                           CASPARIAN MCP SERVER                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ quick_scan  │  │  discover   │  │   approve   │  │    run_backtest         │  │
│  │ apply_scope │  │  _schemas   │  │   _schemas  │  │    fix_parser           │  │
│  │             │  │             │  │  propose_   │  │    execute_pipeline     │  │
│  │             │  │             │  │  amendment  │  │    query_output         │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
│     Discovery          Schema          Approval           Execution              │
└──────────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
            ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
            │   Schema    │ │   Backtest  │ │    Type     │
            │  Contract   │ │   Engine    │ │  Inference  │
            │   System    │ │             │ │   Engine    │
            └─────────────┘ └─────────────┘ └─────────────┘
```

### Crate Architecture

| Crate | Purpose | Key Concepts |
|-------|---------|--------------|
| `casparian_mcp` | MCP server for Claude Code | 9 tools, JSON-RPC protocol |
| `casparian_schema` | Schema contracts | Approval workflow, amendments |
| `casparian_backtest` | Multi-file validation | Fail-fast, high-failure tracking |
| `casparian_worker` | Parser execution + type inference | Constraint-based inference |
| `casparian_scout` | File discovery + tagging | Sources, patterns, tags |
| `casparian` | Unified CLI binary | start, publish, scout commands |
| `cf_security` | Auth + signing | Ed25519, Azure AD |
| `cf_protocol` | Binary protocol | OpCodes, serialization |

---

## Core Concepts

### 1. Schema = Intent, then Contract

The most important architectural principle:

```
Discovery → User Reviews → APPROVAL → Contract (Immutable)
```

- **Before approval**: Schema is a proposal, can be changed freely
- **After approval**: Schema is a CONTRACT - parser must conform
- **Violations**: Hard failures, not silent coercion

Why? Data quality. Users know exactly what they're getting.

### 2. Constraint-Based Type Inference

Traditional inference: "70% look like dates, so it's a date" (voting)

Our approach: "31/05/24 PROVES DD/MM/YY format because 31 > 12" (elimination)

```rust
// Value "31/05/24" eliminates MM/DD/YY as a possibility
// because month cannot be 31
solver.add_value("31/05/24");
assert!(!solver.possible_formats().contains("MM/DD/YY"));
```

Key insight: One constraining value can resolve ambiguity with certainty.

### 3. Fail-Fast Backtest

When validating a parser against many files:
1. Test **high-failure files first** (files that failed in previous iterations)
2. If they still fail, stop early (parser not ready)
3. If they pass, continue with remaining files

This provides rapid feedback during parser development.

### 4. Tags, Not Routes

Old model: `pattern → transform → sink` (coupled)

New model:
```
Scout: pattern → TAG
Sentinel: TAG → plugin subscription → execute → sink
```

Decoupling enables:
- Manual tag override
- Multiple plugins per tag
- Tag assignment via pattern OR manual OR API

### 5. Bridge Mode Execution

All plugins run in isolated subprocesses:

```
Worker (Host)  <──AF_UNIX──>  Guest Process (isolated venv)
     │                              │
     │ Credentials, Sinks           │ Plugin code only
     ▼                              ▼
  Write to DB/Parquet         Stream Arrow IPC batches
```

Host holds secrets. Guest is sandboxed.

### 6. UV for Environment Management

All Python environments use [uv](https://github.com/astral-sh/uv):
- `uv.lock` files for reproducible dependencies
- Fast, cross-platform environment creation
- Content-addressable venv caching (`~/.casparian_flow/venvs/{env_hash}/`)

---

## Directory Structure

```
casparian-flow/
├── CLAUDE.md                 # YOU ARE HERE
├── ARCHITECTURE.md           # Detailed system design
├── README.md                 # Quick start
│
├── crates/                   # Rust core
│   ├── casparian/            # Unified binary (CLI)
│   │   └── src/main.rs       # CLI entry point
│   ├── casparian_mcp/        # MCP server
│   │   ├── CLAUDE.md         # MCP-specific docs
│   │   └── src/
│   │       ├── tools/        # 9 MCP tools
│   │       ├── server.rs     # JSON-RPC server
│   │       └── protocol.rs   # MCP protocol types
│   ├── casparian_schema/     # Schema contracts
│   │   ├── CLAUDE.md         # Schema-specific docs
│   │   └── src/
│   │       ├── contract.rs   # LockedSchema, SchemaContract
│   │       ├── approval.rs   # Approval workflow
│   │       ├── amendment.rs  # Schema evolution
│   │       └── storage.rs    # SQLite persistence
│   ├── casparian_backtest/   # Backtest engine
│   │   ├── CLAUDE.md         # Backtest-specific docs
│   │   └── src/
│   │       ├── high_failure.rs  # Failure tracking
│   │       ├── failfast.rs      # Early termination
│   │       ├── loop_.rs         # Iteration loop
│   │       └── metrics.rs       # Pass rate, plateau detection
│   ├── casparian_worker/     # Worker + type inference
│   │   ├── CLAUDE.md         # Worker-specific docs
│   │   └── src/
│   │       ├── type_inference/  # Constraint solver
│   │       ├── bridge.rs        # Host/Guest communication
│   │       └── worker.rs        # Job execution
│   ├── casparian_scout/      # File discovery
│   │   ├── CLAUDE.md         # Scout-specific docs
│   │   └── src/
│   │       ├── db.rs         # SQLite state
│   │       └── scanner.rs    # Filesystem walking
│   ├── cf_security/          # Auth + signing
│   └── cf_protocol/          # Binary protocol
│
├── ui/                       # Tauri desktop app
│   ├── CLAUDE.md             # UI-specific docs
│   ├── src/                  # SvelteKit frontend
│   └── src-tauri/            # Rust backend
│
└── demo/                     # Example files and plugins
```

---

## MCP Tools Reference

The MCP server exposes 9 tools for Claude Code integration:

### Discovery Tools
| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `quick_scan` | Fast metadata scan | `path`, `extensions`, `max_depth` |
| `apply_scope` | Group files for processing | `files`, `scope_name` |

### Schema Tools
| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `discover_schemas` | Infer schema from files | `scope_id`, `sample_rows` |
| `approve_schemas` | Create locked contracts | `scope_id`, `approved_by` |
| `propose_amendment` | Modify existing contract | `scope_id`, `changes` |

### Backtest Tools
| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `run_backtest` | Validate parser | `scope_id`, `pass_threshold` |
| `fix_parser` | Generate fixes | `scope_id`, `failures` |

### Execution Tools
| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `execute_pipeline` | Run full pipeline | `scope_id`, `sink_config` |
| `query_output` | Query processed data | `scope_id`, `sql` |

---

## Development Workflow

### After Any Code Change

```bash
# 1. Type check everything
cargo check                    # Rust
cd ui && bun run check         # TypeScript/Svelte

# 2. Build
cargo build                    # Rust
cd ui && bun run build         # UI

# 3. Test
cargo test                     # All Rust tests
cargo test -p casparian_mcp    # Specific crate
cd ui && bun run test:e2e      # UI E2E tests
```

### Running E2E Tests

```bash
# All E2E tests for new crates
cargo test --package casparian_worker --test e2e_type_inference
cargo test --package casparian_schema --test e2e_contracts
cargo test --package casparian_backtest --test e2e_backtest
cargo test --package casparian_mcp --test e2e_tools

# UI E2E tests
cd ui && bun run test:e2e
```

### Key Commands

```bash
# Start the system
./target/release/casparian start

# Publish a plugin
./target/release/casparian publish my_plugin.py --version 1.0.0

# Scout operations
./target/release/casparian scout init
./target/release/casparian scout run --config scout.toml
```

---

## Database Architecture (CRITICAL)

### Single Database Rule

**Everything uses ONE database: `~/.casparian_flow/casparian_flow.sqlite3`**

```
~/.casparian_flow/
├── casparian_flow.sqlite3    # THE ONLY DATABASE
├── venvs/                    # Content-addressable venv cache
├── parsers/                  # Deployed parser .py files
├── output/                   # Parser output (parquet, csv)
└── samples/                  # Sample files for demos
```

### Table Prefixes

| Prefix | Tables | Purpose |
|--------|--------|---------|
| `parser_lab_*` | parsers, test_files | Parser development |
| `scout_*` | sources, files, tagging_rules | File discovery |
| `cf_*` | plugin_manifest, processing_queue | Sentinel/execution |
| `schema_*` | contracts, amendments | Schema contracts |
| `backtest_*` | high_failure_files | Backtest tracking |

---

## Common Tasks

### Add a New MCP Tool

1. Create tool module in `crates/casparian_mcp/src/tools/`
2. Implement `McpTool` trait
3. Register in `create_default_registry()`
4. Add E2E test in `tests/e2e_tools.rs`
5. Run `cargo test -p casparian_mcp`

### Add Schema Support for New Type

1. Add variant to `DataType` in `casparian_schema/src/contract.rs`
2. Implement validation in `DataType::validate_string()`
3. Add Arrow type mapping in `DataType::arrow_type_name()`
4. Update type inference in `casparian_worker/src/type_inference/`
5. Add E2E tests

### Debug Type Inference

```rust
// Get elimination evidence
let solver = ConstraintSolver::new("column_name");
solver.add_value("31/05/24");
for evidence in solver.elimination_evidence() {
    println!("Eliminated {:?} because: {}", evidence.eliminated_type, evidence.reason);
}
```

---

## Code Style Guidelines

### Rust
- Use `rustfmt` defaults
- Prefer `Result<T, String>` for Tauri commands
- Add doc comments for public functions
- Comprehensive error types with `thiserror`

### TypeScript/Svelte
- Svelte 5 runes: `$state`, `$derived`, `$props`
- No emojis in code or UI unless requested
- Semantic HTML for accessibility

### Testing
- E2E tests for all new features
- No mocks for core functionality - use real databases
- Test happy path AND error cases

---

## Architecture Decision Records

### ADR-001: Parser Lab Redesign (Jan 2025)
**Decision:** Parser is the top-level entity. No project wrapper.
**Consequence:** Simpler mental model, direct file → parser → output flow.

### ADR-002: Tags, Not Routes (Jan 2025)
**Decision:** Scout only discovers and tags. Sentinel handles processing.
**Consequence:** Clean separation, multiple plugins per tag, manual override.

### ADR-003: Constraint-Based Type Inference (Jan 2025)
**Decision:** Use elimination, not voting, for type inference.
**Consequence:** Certainty when possible, explicit ambiguity when not.

### ADR-004: Schema as Contract (Jan 2025)
**Decision:** Approved schemas become immutable contracts.
**Consequence:** Hard failures on violation, no silent coercion.

### ADR-005: Fail-Fast Backtest (Jan 2025)
**Decision:** Test high-failure files first.
**Consequence:** Rapid feedback during parser development.

### ADR-006: MCP-First Integration (Jan 2025)
**Decision:** Claude Code integration via MCP protocol.
**Consequence:** AI-assisted data processing workflow.

---

## Glossary

| Term | Definition |
|------|------------|
| **MCP** | Model Context Protocol - LLM tool integration standard |
| **Scout** | File discovery + tagging layer |
| **Sentinel** | Job orchestration + worker management |
| **Schema Contract** | Approved schema that parser must conform to |
| **High-Failure File** | File that has historically failed during backtest |
| **Constraint Solver** | Type inference via elimination |
| **Bridge Mode** | Host/Guest execution model for isolation |
| **Scope** | Group of files for processing (parser, pipeline, tag) |
| **Amendment** | Controlled schema evolution after approval |

---

## Getting Help

- **Component docs**: Check crate-specific `CLAUDE.md` files
- **Architecture**: See `ARCHITECTURE.md`
- **UI development**: See `ui/CLAUDE.md`
- **MCP tools**: See `crates/casparian_mcp/CLAUDE.md`

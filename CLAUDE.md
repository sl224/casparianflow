# Claude Code Instructions for Casparian Flow

## Quick Orientation

**What is Casparian Flow?** A data processing platform that transforms "dark data" (files on disk) into queryable datasets. Users discover files, approve schemas, and execute pipelines.

**Start Here:**
1. This file → high-level architecture
2. `code_execution_workflow.md` → **coding standards and testing**
3. `ARCHITECTURE.md` → detailed system design
4. Crate-specific `CLAUDE.md` files → component details

---

## Pre-v1 Development Rules

Until v1 ships: NO production users, NO data to preserve. Do NOT implement:
- Database migrations (just delete `~/.casparian_flow/casparian_flow.duckdb`)
- Backwards compatibility / API versioning / gradual rollouts
- Data preservation during refactors

**Instead:** Change schemas directly, update all call sites, break fast and fix fast.

---

## Engineering Ethos

Follow "make illegal states unrepresentable" (Jon Blow, Casey Muratori, John Carmack style). Prefer compile-time guarantees over runtime patching.

### Core Principles

| Principle | Meaning |
|-----------|---------|
| **Parse, don't validate** | Convert unstructured → structured at boundaries |
| **Data dominates** | Right data structures first, algorithms follow |
| **State is liability** | Minimize state, derive what you can compute |
| **Boundaries do heavy lifting** | Defensive code at edges; core trusts inputs |
| **Boring code > clever code** | Junior-readable in 30 seconds |
| **Fail loud, not silent** | Errors impossible to ignore |
| **Delete unused code** | Dead code misleads and hides bugs |

### Anti-Patterns (Reference by Name)

| Pattern | Problem | Fix |
|---------|---------|-----|
| **Silent Corruption** | `.unwrap_or_default()` hides bad DB data | Use `?` with typed error |
| **Stringly Typed** | `match status.as_str()` misses typos | Use enum matching |
| **Shotgun Validation** | Same check in 10 places | Parse once, use validated type |
| **Zombie Object** | Struct needs `.init()` after `new()` | Valid from construction |
| **Primitive Obsession** | `fn f(file_id: i64, job_id: i64)` swappable | Use newtypes |
| **Dual Source of Truth** | Rust enum vs SQL CHECK diverge | Single authoritative source |
| **Boolean Blindness** | `editing: bool, creating: bool` both true | Use enum for exclusive states |
| **Lossy Cast** | `x as i32` silently truncates | Use `try_from` |
| **Magic String Literals** | `"PENDING"` in 40 files | Centralized constants |

### Pre-Commit Checklist

- [ ] No `.unwrap_or_default()` on parsed enums
- [ ] Status/state checks use enums, not strings
- [ ] Unstructured data converted to types at boundaries
- [ ] No duplicated constants between Rust and SQL
- [ ] Structs valid from construction
- [ ] No `as i32` on potentially large values
- [ ] Multiple bools aren't encoding exclusive states

---

## System Architecture

```
CLI/TUI → Schema Contracts → Sentinel/Job Queue → Worker (Bridge) → Sinks
```

### Crate Architecture

| Crate | Purpose |
|-------|---------|
| `casparian` | Unified CLI binary |
| `casparian_schema` | Schema contracts, approval workflow |
| `casparian_backtest` | Multi-file validation, fail-fast |
| `casparian_worker` | Parser execution, type inference |
| `casparian_scout` | File discovery, tagging |
| `casparian_db` | Database abstraction, license gating |
| `casparian_security` | Auth, Ed25519 signing |
| `casparian_protocol` | Binary protocol, serialization |

---

## Core Concepts

### 1. Schema = Intent, then Contract
```
Discovery → User Reviews → APPROVAL → Contract (Immutable)
```
Before approval: proposal. After: CONTRACT with hard failures on violation.

### 2. Constraint-Based Type Inference
Elimination, not voting: "31/05/24" PROVES DD/MM/YY because 31 > 12.

### 3. Fail-Fast Backtest
Test high-failure files first → early stop if they still fail.

### 4. Tags, Not Routes
Scout: `pattern → TAG`. Sentinel: `TAG → plugin → execute → sink`.

### 5. Bridge Mode Execution
Plugins run in isolated subprocesses. Host holds secrets; guest is sandboxed.

### 6. Parser Execution

```python
class MyParser:
    name = 'my_parser'           # Required
    version = '1.0.0'            # Required
    topics = ['sales_data']      # Required
    outputs = {'orders': pa.schema([...])}

    def parse(self, ctx):
        yield ('orders', dataframe)  # (sink_name, data) tuples
```

**Key features:** Parser versioning, deduplication by (input_hash, parser_name, version), lineage columns (`_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version`), atomic writes.

---

## Database Architecture

### Single Database Rule
Everything uses: `~/.casparian_flow/casparian_flow.duckdb`

### Table Prefixes
| Prefix | Purpose |
|--------|---------|
| `cf_parsers`, `cf_parser_topics` | Parser registry, topic routing |
| `cf_job_status`, `cf_processing_history` | Job tracking, deduplication |
| `scout_*` | File discovery, tagging rules |
| `schema_*` | Schema contracts, amendments |
| `backtest_*` | High-failure tracking |

### Database Abstraction
- **NEVER** use `sqlx::Sqlite*` in library code → use generic `Pool<DB>`
- **NEVER** hardcode `anthropic::*` → use `LlmProvider` trait
- **OK** to use concrete types in: CLI entry points, tests

---

## Development Workflow

```bash
# After any change
cargo check && cargo build --release && cargo test

# Key commands
casparian run parser.py input.csv [--sink parquet://./] [--force]
casparian backfill my_parser [--execute] [--limit 10]
casparian tui
casparian scan <dir> --tag my_topic
casparian files --tag my_topic
casparian jobs --status pending
```

### Code Quality Requirements

| Requirement | Details |
|-------------|---------|
| Zero warnings | `cargo check` + `cargo clippy` clean |
| Use sqlx | Never `rusqlite` |
| No unwrap in lib | Use `?` or `expect()` with context |
| Channels over locks | `tokio::sync::mpsc` or `std::sync::mpsc` |

---

## Documentation Structure

| Master Doc | Sub-Docs | Focus |
|------------|----------|-------|
| `spec.md` | `specs/*.md` | What to build |
| `STRATEGY.md` | `strategies/*.md` | How to win |

**When to create subspec:** TUI mode with complex state, >50 lines of spec, multiple impl phases.

---

## TUI Development

**Use TMux for ALL TUI work.** Scripts in `scripts/`:
- `tui-debug.sh start|stop|restart` - Session management
- `tui-send.sh "key"` - Send keystrokes
- `tui-capture.sh` - Capture screen
- `tui-test.sh all` - Run test scenarios

**Workflow:** Reproduce → Capture after EACH keystroke → Compare to spec → Fix → Verify in fresh session.

---

## Workflow Manager

Meta-workflows for specs, code quality, data models. See `specs/meta/workflow_manager.md`.

| Workflow | Trigger |
|----------|---------|
| `feature_workflow` | "add", "implement", "fix" |
| `spec_refinement_workflow` | "refine spec", "gaps in spec" |
| `spec_maintenance_workflow` | "audit specs" |
| `memory_audit_workflow` | "memory audit", "allocation" |
| `data_model_maintenance_workflow` | "dead types", "type cleanup" |
| `abstraction_audit_workflow` | "abstraction", "db coupling" |
| `tui_testing_workflow` | "test TUI" |

---

## Architecture Decision Records

| ADR | Decision | Consequence |
|-----|----------|-------------|
| 001 | Parser is top-level entity | Direct file → parser → output flow |
| 002 | Tags, not routes | Scout discovers+tags; Sentinel processes |
| 003 | Constraint-based type inference | Elimination, not voting |
| 004 | Schema as contract | Hard failures on violation |
| 005 | Fail-fast backtest | Test high-failure files first |
| 006 | MCP-first integration (v1) | AI-assisted workflow with human approval gates |
| 007 | CLI-first architecture | No Tauri; CLI + TUI only |
| 008 | Parser as tuple yielder | `(sink_name, data)` tuples |
| 009 | Content-based parser identity | blake3(content), not path |
| 010 | Partitioned output by job | `{output}_{job_id}.parquet` |
| 011 | CLI sink override | `--sink` overrides parser defaults |
| 012 | Parser versioning | `name`, `version`, `topics` required; dedup by (hash, name, ver) |
| 013 | Topic subscriptions | Files → Tags → Topics → Parsers chain |
| 014 | Structured error codes | Python bridge emits `error_code` field |
| 015 | Dual parser patterns | `transform(df)` for test; `parse(path)` for run |
| 016 | Split runtime architecture | Control plane + data plane separate |
| 021 | AI agentic iteration (v1) | Ephemeral contracts for iteration; AST for publish |

**ADR-006 details:** See `docs/v1_scope.md` MCP section, `docs/execution_plan_mcp.md`.
**ADR-021 details:** See `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`.

---

## Glossary

| Term | Definition |
|------|------------|
| **Scout** | File discovery + tagging |
| **Sentinel** | Job orchestration |
| **Schema Contract** | Approved schema parser must conform to |
| **High-Failure File** | Historically failed during backtest |
| **Bridge Mode** | Host/guest isolation model |
| **Lineage Columns** | `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Deduplication** | Skip if (input_hash, parser_name, version) seen |
| **Backfill** | Re-process files when parser version changes |
| **EphemeralSchemaContract** | Temporary contract for AI iteration |
| **ViolationContext** | Machine-readable error context for AI learning |

---

## Getting Help

- Component docs: crate-specific `CLAUDE.md` files
- Architecture: `ARCHITECTURE.md`
- CLI usage: `casparian --help`

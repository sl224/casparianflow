# Claude Code Instructions for Casparian Flow

## Quick Orientation

**What is Casparian Flow?** A data processing platform that transforms "dark data" (files on disk) into queryable datasets. Users discover files, approve schemas, and execute pipelines - all with AI assistance via MCP (Model Context Protocol).

**Start Here:**
1. Read this file for high-level architecture
2. See `code_execution_workflow.md` for **coding standards and testing requirements**
3. See `ARCHITECTURE.md` for detailed system design
4. Check crate-specific `CLAUDE.md` files for component details

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
| `casparian_security` | Auth + signing | Ed25519, Azure AD |
| `casparian_protocol` | Binary protocol | OpCodes, serialization |

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

### 7. Parser Execution (`casparian run`)

The `run` command executes parsers with ZMQ-based IPC:

```
┌─────────────────────────────────────────────────────────────────┐
│                      casparian run                              │
│                        (Rust CLI)                               │
└─────────────────────────────────────────────────────────────────┘
        │                                          ▲
        │ 1. Spawn worker                          │ 4. ZMQ messages
        │ 2. Pass: parser, input, endpoint         │    (Arrow IPC batches)
        ▼                                          │
┌─────────────────────────────────────────────────────────────────┐
│                    Python Worker Shim                           │
│  - Loads parser                                                 │
│  - Extracts name, version, topics from parser class             │
│  - Executes parse() method                                      │
│  - Yields (sink_name, arrow_batch) tuples                       │
│  - Serializes to Arrow IPC, sends via ZMQ                       │
└─────────────────────────────────────────────────────────────────┘
```

**Parser class requirements:**

```python
import pyarrow as pa

class MyParser:
    name = 'my_parser'           # Required: logical parser name
    version = '1.0.0'            # Required: semver version
    topics = ['sales_data']      # Required: topics to subscribe to
    outputs = {
        'orders': pa.schema([
            ('id', pa.int64()),
            ('amount', pa.float64()),
        ])
    }

    def parse(self, ctx):
        # ctx.input_path, ctx.source_hash, ctx.job_id available
        yield ('orders', dataframe)  # Yield (sink_name, data) tuples
```

**Key features:**
- **Parser versioning**: Parsers must declare `name`, `version`, `topics`
- **Version conflict detection**: Same (name, version) with different source hash = ERROR
- **Deduplication**: Skip if (input_hash, parser_name, parser_version) already processed
- **Lineage columns**: `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version`
- **Atomic writes**: Temp file → rename for parquet/csv
- **Partitioned output**: `{output}_{job_id}.parquet` per run
- **Topic subscriptions**: Files → Tags → Topics → Parsers

**Version bump flow:**

When you change parser code:
1. Bump the `version` attribute (e.g., `1.0.0` → `1.0.1`)
2. Run `casparian backfill my_parser` to see files needing re-processing
3. Use `--execute` to actually re-process them

**Supported sinks:**
- `parquet://./output/` - Parquet files (default)
- `sqlite:///data.db` - SQLite database
- `csv://./output/` - CSV files

---

## Directory Structure

```
casparian-flow/
├── CLAUDE.md                 # YOU ARE HERE
├── README.md                 # Quick start
│
├── spec.md                   # MASTER: Product specification
├── specs/                    # SUBSPECS: Feature implementation details
│   ├── discover.md           # Discover mode TUI spec
│   ├── parser_bench.md       # Parser Bench mode TUI spec
│   └── hl7_parser.md         # HL7 v2.x parser technical spec
│
├── STRATEGY.md               # MASTER: Business strategy (platform-level)
├── strategies/               # SUBSTRATEGIES: Vertical-specific GTM
│   └── healthcare_hl7.md     # Healthcare/HL7 market strategy
│
├── docs/                     # Technical decisions, ADRs
├── archive/                  # Historical documents
│
├── crates/                   # Rust core
│   ├── casparian/            # Unified binary (CLI + TUI)
│   │   └── src/
│   │       ├── main.rs       # CLI entry point
│   │       └── cli/
│   │           └── tui/      # Terminal UI (ratatui)
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
└── demo/                     # Example files and plugins
```

### Documentation Organization Pattern

This project uses a **master/sub-document pattern** for both specifications and strategy:

| Master Doc | Sub-Docs | Focus |
|------------|----------|-------|
| `spec.md` | `specs/*.md` | **What to build** (features, schemas, TUI) |
| `STRATEGY.md` | `strategies/*.md` | **How to win** (markets, competitors, GTM) |

---

### Specification Organization (Subspecs)

Complex components have detailed specifications in `specs/` subdirectory. This keeps `spec.md` readable while providing comprehensive documentation for implementation.

**Pattern:**
- **`spec.md`**: Master specification with high-level summaries
- **`specs/*.md`**: Detailed subspecs for complex components

**Bidirectional References:**
```
spec.md                          specs/discover.md
─────────                        ─────────────────
#### Mode: Discover              # Discover Mode - TUI Subspec
> Full Specification:
> See specs/discover.md    ◄────► Parent: spec.md Section 5.3
```

**Subspec Structure:**
| Section | Purpose |
|---------|---------|
| Header | Status, Parent reference, Version |
| Overview | Philosophy, core entities |
| User Workflows | Step-by-step user journeys |
| Layout Specification | ASCII diagrams, component descriptions |
| State Machine | State transitions, definitions |
| Data Model | Rust struct definitions |
| Keybindings | Key → action tables by context |
| Implementation Phases | Checkbox task lists |
| Decisions Made | Decision → choice → rationale |
| Revision History | Date → version → changes |

**When to Create a Subspec:**
- TUI mode with complex state machine
- Component with >50 lines of specification
- Feature requiring detailed layout diagrams
- Anything with multiple implementation phases

---

### Strategy Organization (Substrategies)

Vertical-specific market strategies live in `strategies/` subdirectory. This keeps `STRATEGY.md` focused on platform-level strategy while enabling deep dives into specific markets.

**Pattern:**
- **`STRATEGY.md`**: Master business strategy (vision, ICP, pricing, GTM phases)
- **`strategies/*.md`**: Vertical-specific go-to-market strategies

**Bidirectional References:**
```
STRATEGY.md                           strategies/healthcare_hl7.md
───────────                           ────────────────────────────
| Healthcare IT | ... |               # Healthcare HL7 Market Strategy
[→ Deep Dive](strategies/...)   ◄────► Parent: STRATEGY.md Section 2
                                       Related Spec: specs/hl7_parser.md
```

**Substrategy Structure:**
| Section | Purpose |
|---------|---------|
| Header | Status, Parent reference, Related Spec, Version |
| Market Overview | Size, players, regulatory environment |
| Target Personas | Buyer, user, influencer profiles |
| Pain Points | What hurts today, current alternatives |
| Competitive Positioning | Incumbents, our differentiation |
| Attack Strategies | Multiple approaches to win |
| Product Roadmap Implications | What features this market needs |
| Go-to-Market | Channels, pricing, messaging |
| Success Metrics | How we measure winning |
| Risks & Mitigations | What could go wrong |

**When to Create a Substrategy:**
- Entering a new vertical market (healthcare, defense, finance)
- Facing a major competitor requiring specific positioning
- Market with unique regulatory or technical requirements
- Vertical needing dedicated pricing or GTM approach

**Linking Specs and Strategies:**
Technical specs and market strategies are **related but separate**:
- `specs/hl7_parser.md` → How to build the HL7 parser (technical)
- `strategies/healthcare_hl7.md` → How to win the healthcare market (business)

Both should cross-reference each other in their headers.

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
# 1. Type check
cargo check

# 2. Build
cargo build --release

# 3. Test
cargo test                     # All Rust tests
cargo test -p casparian_mcp    # Specific crate
```

### Running E2E Tests

```bash
# All E2E tests for crates
cargo test --package casparian_worker --test e2e_type_inference
cargo test --package casparian_schema --test e2e_contracts
cargo test --package casparian_backtest --test e2e_backtest
cargo test --package casparian_mcp --test e2e_tools

# E2E test script
./tests/e2e/run_e2e_test.sh
```

### Key Commands

```bash
# Run a parser against an input file
./target/release/casparian run parser.py input.csv
./target/release/casparian run parser.py input.csv --sink parquet://./output/
./target/release/casparian run parser.py input.csv --sink sqlite:///data.db
./target/release/casparian run parser.py input.csv --force  # Skip dedup check
./target/release/casparian run parser.py input.csv --whatif # Dry run

# Backfill: re-process files when parser version changes
./target/release/casparian backfill my_parser              # Preview what would be processed
./target/release/casparian backfill my_parser --execute    # Actually process
./target/release/casparian backfill my_parser --limit 10   # Limit to 10 files
./target/release/casparian backfill my_parser --force      # Force re-process all

# Interactive TUI
./target/release/casparian tui

# Publish a plugin
./target/release/casparian publish my_plugin.py --version 1.0.0

# Scout operations
./target/release/casparian scan <directory> --tag my_topic
./target/release/casparian files --tag my_topic
./target/release/casparian jobs --status pending
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
| `cf_parsers` | Parser registry with name, version, source_hash | Parser versioning |
| `cf_parser_topics` | Parser → topic subscriptions | Topic routing |
| `cf_job_status` | Job lifecycle (running/staged/complete/failed) | Job tracking |
| `cf_processing_history` | Dedup by (input_hash, parser_name, version) | Skip unchanged |
| `scout_*` | sources, files, tagging_rules | File discovery |
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

## Code Quality Workflow

> **Full documentation:** See [`code_execution_workflow.md`](./code_execution_workflow.md)

**IMPORTANT:** Follow the workflow for all coding tasks. Key points:

### Before You Code
1. **Search for existing modules** - If similar exists, ask before creating new
2. **Identify related components** - Reuse shared types
3. **Plan concurrency** - Channels over locks, document strategy
4. **Verify state machines** - Check spec matches code, understand hierarchy, reconcile if inconsistent

### While You Code
- **No stringly types** - Newtypes for IDs, enums for states, structs for config
- **No race conditions** - Channels (`mpsc`) over `Arc<Mutex<T>>`, no locks across `.await`
- **Propagate errors** - Use `?`, not `unwrap()` in library code
- **No migrations** - Alpha app: change schema directly, users delete DB if needed

### Testing
- Test critical paths with real DBs (no mocks)
- Test error cases and edge cases
- Don't test implementation details

### Before You Commit
- `cargo check` + `cargo clippy` pass
- Critical path has test coverage
- Public APIs have doc comments

---

## Code Style Guidelines

### Rust
- Use `rustfmt` defaults
- Add doc comments for public functions
- Comprehensive error types with `thiserror`
- Helpful CLI error messages with suggestions
- **Database access: Use `sqlx`, NOT `rusqlite`** - sqlx is async and the project standard

### Testing
- E2E tests for all new features
- No mocks for core functionality - use real databases
- Test happy path AND error cases
- See [`code_execution_workflow.md`](./code_execution_workflow.md) for detailed testing patterns and requirements

### CLI Design Principles

**Core Principles:**
1. **Verb-First Commands** - Action before noun: `casparian scan` not `casparian folder scan`
2. **Fast Feedback** - <1s for typical operations, streaming output for long ops
3. **Helpful Errors** - Every error includes what went wrong AND how to fix it
4. **Type-Preserving Interchange** - Binary formats (Arrow IPC), not text streams
5. **No Hidden State** - No project files, no server (unless explicitly started)
6. **Discoverable** - Help system teaches the tool organically

**Anti-Patterns to Avoid:**
- No interactive wizards (use flags instead)
- No "press enter to continue"
- No spinners without information
- No silent failures
- No config files required for basic usage

**Output Modes:**

| Context | Format | Reason |
|---------|--------|--------|
| Terminal (human) | Pretty tables | Readable, truncated, colored |
| `-o file.parquet` | Parquet | Compressed columnar storage |
| `-o file.sqlite` | SQLite | Ad-hoc SQL queries |
| `--sink` override | Per command | Flexible output destination |

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

### ADR-007: CLI-First Architecture (Jan 2025)
**Decision:** Remove Tauri desktop app, focus on CLI + TUI.
**Consequence:** Simpler architecture, faster iteration, stability first.

### ADR-008: Parser as Tuple Yielder (Jan 2025)
**Decision:** Parsers yield `(sink_name, data)` tuples, not Record objects.
**Consequence:** Simpler protocol, no wrapper classes, data is just data.

### ADR-009: Content-Based Parser Identity (Jan 2025)
**Decision:** Parser identity is blake3(parser_content), not file path.
**Consequence:** Same parser = same ID regardless of location. Parser changes trigger reprocessing.

### ADR-010: Partitioned Output by Job (Jan 2025)
**Decision:** Each run creates `{output}_{job_id}.parquet`, no appending.
**Consequence:** Atomic writes, no corruption risk, query with glob patterns.

### ADR-011: CLI Sink Override (Jan 2025)
**Decision:** CLI `--sink` overrides parser-defined sinks.
**Consequence:** Flexibility for users, parser author defines defaults.

### ADR-012: Parser Versioning (Jan 2025)
**Decision:** Parsers must declare `name`, `version`, and `topics` attributes.
**Consequence:**
- Same (name, version) with different source hash = ERROR (must bump version)
- Dedup key is (input_hash, parser_name, parser_version) not just parser_id
- Backfill command enables re-processing when version changes
- Lineage includes `_cf_parser_version` for traceability

### ADR-013: Topic Subscriptions (Jan 2025)
**Decision:** Parsers declare topics they subscribe to; files are routed by tag→topic match.
**Consequence:**
- Files → Tags → Topics → Parsers chain enables backfill queries
- Parser can subscribe to multiple topics
- Topic is decoupled from file pattern (one indirection layer)

### ADR-014: Structured Error Codes (Jan 2025)
**Decision:** Python bridge_shim emits structured `error_code` field based on exception type.
**Consequence:**
- Rust can parse error codes directly instead of string matching
- Fallback string matching preserved for backwards compatibility
- Error classification is deterministic and testable

### ADR-015: Dual Parser Patterns (Jan 2025)
**Decision:** Keep `transform(df)` and `parse(file_path)` as separate parser patterns.
**Rationale:**
- `transform(df)`: Test harness reads file, passes DataFrame - used by `casparian test`
- `parse(file_path)`: Parser handles its own file reading - used by `casparian run`
- Unifying would require changing parser interface (breaking change)
**Consequence:** `run_parser_test` remains separate from `DevRunner` code path.

### ADR-016: Split Runtime Architecture (Jan 2025)
**Decision:** Keep separate Control Plane and Data Plane runtimes for now.
**Rationale:**
- Current pattern: Control (1 thread) + Data (N-1 threads) in separate `block_on` calls
- Unifying to single runtime with `spawn_blocking` is significant refactor
- Current architecture works and is tested
**Future:** Consider unification when adding more complex async coordination.
**Consequence:** Accept "uncanny valley" complexity until clear benefit emerges.

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
| **parser_id** | UUID identifying a specific parser (name + version + source_hash) |
| **parser_name** | Logical parser name (e.g., "sales_parser") |
| **parser_version** | Semver version (e.g., "1.0.0") |
| **job_id** | UUID identifying a single processing run |
| **Lineage Columns** | `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Deduplication** | Skip processing if (input_hash, parser_name, parser_version) seen before |
| **Backfill** | Re-process files when parser version changes |
| **Topic Subscription** | Parser declares topics it handles; files with matching tags are routed |

---

## Getting Help

- **Component docs**: Check crate-specific `CLAUDE.md` files
- **Architecture**: See `ARCHITECTURE.md`
- **MCP tools**: See `crates/casparian_mcp/CLAUDE.md`
- **CLI usage**: `./target/release/casparian --help`

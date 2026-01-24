# Casparian Flow

A **local-first ingestion and governance runtime** for file artifacts.

## What is Casparian Flow?

Casparian Flow turns messy file corpuses into typed, queryable tables with incremental
ingestion, per-row lineage, quarantine semantics, and schema contracts. v1 targets
DFIR / Incident Response: parse Windows artifacts (EVTX as flagship) into auditable,
reproducible datasets.

**Core promise:** If you can point Casparian at a directory of files and a parser, you can
reliably produce tables you can trust—and you can prove how you got them.

**Core capabilities:**

1. **Discovers** files across your systems (case folders, evidence bundles)
2. **Validates** against schema contracts (authoritative in Rust, not Python)
3. **Quarantines** invalid rows with error context (no silent coercion)
4. **Tracks lineage** per-row: source hash, job id, timestamp, parser version
5. **Outputs** clean, queryable datasets (DuckDB, Parquet)

**Trust primitives:**
- Same inputs + same parser bundle hash → identical outputs (reproducibility)
- Invalid rows go to quarantine, not silent coercion (safe partial success)
- Every output row has lineage metadata (chain of custody)
- Content-addressed parser identity (changes trigger re-processing)

## Quick Start

```bash
# Build
cargo build --release

# Scan a folder (discovery)
./target/release/casparian scan tests/fixtures/fix --tag fix-data

# Preview a file
./target/release/casparian preview tests/fixtures/fix/order_lifecycle.fix --head 3

# Run the FIX parser (multi-output: order_lifecycle, session_events, optional fix_tags)
FIX_TZ=America/New_York ./target/release/casparian run \
  parsers/fix/fix_parser.py \
  tests/fixtures/fix/mixed_messages.fix \
  --sink duckdb://./output/fix_demo.duckdb

# Start the system (Sentinel + Worker)
./target/release/casparian start

# Interactive TUI
./target/release/casparian tui
```

## FIX Protocol Demo

The included FIX parser demonstrates multi-output parsing for trade break analysis:

```bash
# Required: Set timezone for FIX timestamp parsing
export FIX_TZ=America/New_York

# Parse FIX messages into structured tables
./target/release/casparian run \
  parsers/fix/fix_parser.py \
  tests/fixtures/fix/order_lifecycle.fix \
  --sink parquet://./output/

# Outputs:
#   - fix_order_lifecycle: Orders, executions, cancels with ClOrdID lineage
#   - fix_session_events: Logon, heartbeat, test requests

# Optional: Enable fix_tags output with an allowlist
FIX_TAGS_ALLOWLIST=35,49,56,11 ./target/release/casparian run \
  parsers/fix/fix_parser.py \
  tests/fixtures/fix/order_lifecycle.fix \
  --sink parquet://./output/
```

See [docs/fix_schema.md](docs/fix_schema.md) for the complete schema specification.

## Core Concepts

### Schema = Intent, then Contract

- **Before approval**: Schema is a proposal
- **After approval**: Schema is a CONTRACT - parser must conform
- **Violations**: Invalid rows quarantined with context; no silent coercion

### Trust Primitives

| Guarantee | Description |
|-----------|-------------|
| **Reproducibility** | Same inputs + parser hash → identical outputs |
| **Per-row lineage** | `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Quarantine** | Invalid rows isolated with error context; partial success is safe |
| **Content-addressed** | Parser identity = blake3(content + lockfile) |

### Constraint-Based Type Inference

Traditional: "70% look like dates, so it's a date" (voting)

Casparian: "31/05/24 PROVES DD/MM/YY because 31 > 12" (elimination)

### Fail-Fast Backtest

Test high-failure files first. If they still fail, stop early.

### Bridge Mode Execution

Plugins run in isolated subprocesses. Host has credentials. Guest has only code.
Worker execution is non-interactive (no `pdb`).

## Architecture

```
┌─────────────────────────────────────────┐
│         FRONTENDS (Clients)             │
│     CLI / TUI / Tauri UI / MCP          │
│   • Mutations via Control API (IPC)     │
│   • Read-only DB for queries            │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│      CONTROL PLANE (Sentinel)           │
│   • Single mutation authority           │
│   • Job queue + state machine           │
│   • Materializations (incremental)      │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│      EXECUTION PLANE (Worker)           │
│   • Stateless executor                  │
│   • True cancellation                   │
│   • Atomic outputs (stage → promote)    │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│           PERSISTENCE                   │
│   DuckDB / Parquet / CSV sinks          │
└─────────────────────────────────────────┘
```

See **[ARCHITECTURE.md](ARCHITECTURE.md)** for detailed system design.

### Crates

| Crate | Purpose |
|-------|---------|
| `casparian` | Unified CLI binary |
| `casparian_sentinel` | Control plane + dispatch + Control API |
| `casparian_worker` | Execution + validation |
| `casparian_sinks` | Sink implementations + lineage |
| `casparian_protocol` | Binary protocol + types + idempotency |
| `casparian_db` | DuckDB actor + DB API |
| `casparian_tape` | Event recording for replay/debugging |
| `casparian_backtest` | Multi-file validation |

## Development

```bash
# Type check
cargo check

# Build
cargo build --release

# Test (E2E, no mocks)
cargo test --package casparian_worker --test e2e_type_inference
cargo test --package casparian_schema --test e2e_contracts
cargo test --package casparian_backtest --test e2e_backtest
# E2E test script
./tests/e2e/run_e2e_test.sh
```

## Documentation

- **[CLAUDE.md](CLAUDE.md)** - Entry point for LLM context + coding standards
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Detailed system design + invariants
- **[docs/v1_scope.md](docs/v1_scope.md)** - v1 scope and success metrics
- **[docs/trust_guarantees.md](docs/trust_guarantees.md)** - Trust model and security
- **[docs/schema_rfc.md](docs/schema_rfc.md)** - Schema contract system
- **[docs/fix_schema.md](docs/fix_schema.md)** - FIX protocol schema specification
- **Crate docs**: Each crate has its own `CLAUDE.md`

## Requirements

- Rust 1.75+
- Python 3 with `pyarrow` available in the worker environment
- [uv](https://github.com/astral-sh/uv) optional for provisioning plugin envs

## License

Proprietary

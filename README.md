# Casparian Flow

Transform "dark data" into queryable datasets with AI assistance.

## What is Casparian Flow?

Casparian Flow is a data processing platform that:

1. **Discovers** files (CSVs, JSON, logs) across your systems
2. **Infers** schemas using constraint-based type detection
3. **Validates** parsers against real data with fail-fast testing
4. **Executes** pipelines in isolated environments
5. **Outputs** clean, queryable datasets (Parquet, SQLite, CSV)

AI assistance is optional and out of the critical execution path for v1.

## Quick Start

```bash
# Build
cargo build --release

# Start the system
./target/release/casparian start

# Interactive TUI
./target/release/casparian tui

# Initialize Scout (file discovery)
./target/release/casparian scout init

# Run file discovery
./target/release/casparian scout run --config scout.toml
```

## Core Concepts

### Schema = Intent, then Contract

- **Before approval**: Schema is a proposal
- **After approval**: Schema is a CONTRACT - parser must conform
- **Violations**: Hard failures, not silent coercion

### Constraint-Based Type Inference

Traditional: "70% look like dates, so it's a date" (voting)

Casparian: "31/05/24 PROVES DD/MM/YY because 31 > 12" (elimination)

### Fail-Fast Backtest

Test high-failure files first. If they still fail, stop early.

### Bridge Mode Execution

Plugins run in isolated subprocesses. Host has credentials. Guest has only code.

## Architecture

```
Casparian CLI / TUI
        │
        ▼
  Schema Contracts
        │
        ▼
 Sentinel / Job Queue
        │
        ▼
 Worker (Bridge Mode)
        │
        ▼
  Output Sinks
```

### Crates

| Crate | Purpose |
|-------|---------|
| `casparian` | Unified CLI binary |
| `casparian_schema` | Schema contracts |
| `casparian_backtest` | Multi-file validation |
| `casparian_worker` | Type inference + execution |
| `casparian_scout` | File discovery |
| `cf_security` | Auth + signing |
| `cf_protocol` | Binary protocol |

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

- **[CLAUDE.md](CLAUDE.md)** - Entry point for LLM context
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Detailed system design
- **Crate docs**: Each crate has its own `CLAUDE.md`

## Requirements

- Rust 1.75+
- [uv](https://github.com/astral-sh/uv) for Python environment management

## License

Proprietary

# Review Context: Casparian Flow

## What This App Does (Summary)
Casparian Flow is a **deterministic, governed data build system** for file artifacts.
It transforms "dark data" (files on disk, e.g., Windows EVTX logs) into queryable datasets
with strict schema contracts, quarantine semantics, and per-row lineage tracking.

## Primary User Flow
1. **Discover**: Scan a directory to find files (`casparian scan <dir> --tag mydata`)
2. **Preview**: Examine file structure and sample data (`casparian preview <file>`)
3. **Define Schema**: Create/approve schema contracts that define expected output
4. **Run Parser**: Execute a parser on files (`casparian run parser.py input.file`)
5. **Validate**: Rust-side validation against schema contracts; invalid rows go to quarantine
6. **Query**: Output is written to DuckDB/Parquet for downstream analysis

## Key Technical Details
- **Backend**: Rust (Cargo workspace with ~14 crates)
- **Frontend**: Two interfaces - CLI/TUI (terminal) + Tauri desktop app (TypeScript/React)
- **Database**: DuckDB (single file at `~/.casparian_flow/casparian_flow.duckdb`)
- **Parser Runtime**: Python parsers run in isolated subprocesses via "Bridge Mode"
- **MCP Server**: Model Context Protocol server for AI-assisted workflows

## How to Build & Run

### Backend (Rust)
```bash
cargo build --release
./target/release/casparian --help
./target/release/casparian tui  # Terminal UI
```

### Frontend (Tauri)
```bash
cd tauri-ui
npm install
npm run dev       # Web dev server
npm run tauri dev # Full Tauri app
```

### Tests
```bash
cargo test                                    # All Rust tests
cd tauri-ui && npm test                       # Playwright e2e tests
./tests/e2e/cli_scan_test.sh                  # CLI e2e tests
```

## Code Entrypoints

| Component | Path |
|-----------|------|
| CLI main | `crates/casparian/src/main.rs` |
| TUI app | `crates/casparian/src/cli/tui/` |
| MCP server | `crates/casparian_mcp/src/server.rs` |
| Tauri backend | `tauri-ui/src-tauri/src/main.rs` |
| Tauri frontend | `tauri-ui/src/App.tsx` |
| Parser worker | `crates/casparian_worker/src/worker.rs` |
| Schema contracts | `crates/casparian_schema/src/` |

## Core Domain Logic Locations
- **Schema validation**: `crates/casparian_schema/`
- **Type inference**: `crates/casparian_worker/src/type_inference/`
- **Job orchestration**: `crates/casparian_sentinel/`
- **File discovery**: `crates/casparian/src/scout/`
- **Intent pipeline** (AI workflow): `crates/casparian_mcp/src/intent/`

## Architecture Overview
```
CLI/TUI/Tauri
      │
      ▼
  MCP Server (optional AI assist)
      │
      ▼
Schema Contracts ─────────────────┐
      │                           │
      ▼                           ▼
Sentinel (job queue)         Approval Gates
      │                           │
      ▼                           │
Worker (bridge mode)  ◄───────────┘
      │
      ▼
Output Sinks (DuckDB/Parquet)
```

## Known Quirks / Reviewer Notes
- Pre-v1: No migrations - DB schema changes require deleting local DB
- The `rust` branch has significant uncommitted changes (see `REVIEW_META.txt`)
- ADR-007 says "CLI-first, no Tauri" but Tauri UI exists (ADR-020 added it back)
- MCP tools have approval gates (G1-G6) for human-in-the-loop AI workflows
- Python parsers must define `name`, `version`, `topics` class attributes
- Output has lineage columns: `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version`

## Key Documentation Files
- `CLAUDE.md` - Engineering ethos and architecture overview
- `docs/v1_scope.md` - What's in/out of scope for v1
- `docs/execution_plan_mcp.md` - MCP integration plan
- `docs/intent_pipeline_workflow.md` - AI-assisted workflow design
- `specs/tui.md` - Terminal UI specification
- `specs/tauri_ui.md` - Desktop UI specification

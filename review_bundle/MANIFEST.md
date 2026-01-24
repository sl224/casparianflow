# Bundle Manifest

## What's Included

### Product & Intent Documentation
- `README.md` - Project overview and quickstart
- `spec.md` - Product specification v1
- `CLAUDE.md` - Engineering ethos and architecture
- `AGENTS.md` - Multi-agent workflow instructions
- `STRATEGY.md` - Go-to-market strategy
- `code_execution_workflow.md` - Development workflow guide
- `MANUAL_TESTING_GUIDE.md` - Manual testing procedures
- `docs/` - Full documentation directory (27 files)
  - `v1_scope.md`, `execution_plan*.md`, `schema_rfc.md`
  - `decisions/` - Architecture Decision Records (ADR-017 through ADR-021)
  - `product/` - Pricing, personas, domain intelligence docs
  - `validation/` - Interview scripts, outreach, pilot terms
- `specs/` - Detailed specifications (24 files)
  - Feature specs, TUI specs, meta-workflows
  - View specifications for UI screens
- `strategies/` - Market strategies (15 files)
- `roadmap/` - Future planning docs
- `claude_docs/` - Additional LLM context files

### Source Code

#### Backend (Rust)
- `Cargo.toml`, `Cargo.lock` - Workspace manifest and lockfile
- `crates/` - 14 Rust crates:
  - `casparian/` - Main CLI binary (~30 source files)
  - `casparian_mcp/` - MCP server (~25 source files)
  - `casparian_worker/` - Parser execution (~15 source files)
  - `casparian_schema/` - Schema contracts (~6 source files)
  - `casparian_sentinel/` - Job orchestration (~8 source files)
  - `casparian_backtest/` - Multi-file validation (~6 source files)
  - `casparian_db/` - Database abstraction (~4 source files)
  - `casparian_protocol/` - Binary protocol (~5 source files)
  - `casparian_sinks/` - Output sinks (~1 source file)
  - `casparian_security/` - Auth/signing (~3 source files)
  - `casparian_profiler/` - Performance profiling (~1 source file)
  - `casparian_ids/` - ID type wrappers (~1 source file)
  - `casparian_logging/` - Logging (~1 source file)
  - `casparian_test_utils/` - Test utilities

#### Frontend (Tauri Desktop App)
- `tauri-ui/` - Full Tauri application
  - `package.json`, `package-lock.json` - NPM dependencies
  - `vite.config.ts`, `tsconfig.json` - Build configuration
  - `src/` - React frontend (TypeScript)
    - `App.tsx`, `main.tsx` - Entry points
    - `screens/` - 9 screen components
    - `components/` - Shared components including intent workflow steps
    - `api/` - Tauri command bindings
  - `src-tauri/` - Rust Tauri backend
    - `main.rs` - Tauri entry point
    - `commands/` - IPC command handlers
  - `tests/` - Playwright e2e tests (7 test files)

#### Parsers
- `parsers/fix/` - FIX protocol parser (Python)
- `parsers/evtx_native/` - EVTX parser (Rust native plugin)
- `raw_lines_parser.py` - Simple line-based parser

### Tests & Validation
- `tests/` - Test suite
  - `e2e/` - End-to-end CLI tests (shell scripts)
  - `e2e/mcp/` - MCP server integration tests
  - `fixtures/` - Test fixtures (EVTX, FIX, etc.)
- Per-crate test directories in `crates/*/tests/`

### Demo & Examples
- `demo/` - Demo setup with sample parsers and data
- `scripts/` - Development scripts (TUI testing, profiling)
- `templates/` - Execution templates

### Build & Configuration
- `Cargo.toml` / `Cargo.lock` - Rust workspace
- `tauri-ui/package.json` - NPM dependencies
- `uv.lock` - Python lockfile
- `.gitignore` - Git ignore patterns
- `.mcp.json` - MCP server config (local dev only)
- `playwright.config.js` - E2E test config

## What's Excluded

### Secrets (none found in tracked files)
- `.sesskey` - Removed (session key file)
- No `.env` files were tracked

### Build Outputs
- `target/` - 23GB Rust build directory
- `node_modules/` - 333MB NPM packages
- `.venv/` - 374MB Python virtual environment
- `dist/`, `build/`, `test-results/` - Build outputs

### Caches & Transient
- `.pytest_cache/`, `.ruff_cache/` - Python tool caches
- `__pycache__/` - Python bytecode
- `.coverage` - Coverage data
- `.DS_Store` - macOS metadata
- `.claude/` - Claude Code local state

### Large Binaries
- `*.evtx` in fixtures kept (small, needed for understanding)
- No large media files found

### Database Files
- `casparian_flow.db` - Local SQLite/DuckDB files
- `*.duckdb` - Output database files

## Redactions Performed
| File | Action | Reason |
|------|--------|--------|
| `.sesskey` | Removed | Session key/secret material |

## File Statistics
- **Total files**: 501
- **Total directories**: 109
- **Bundle size**: ~9MB (uncompressed)

## Notes for Reviewer
- Git status shows many uncommitted changes on `rust` branch (development in progress)
- Some tracked JSON files contain test/sample data (e.g., `actionable_findings.json`)
- The bundle preserves exact paths for easy reference

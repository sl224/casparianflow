# Documentation Index

**Status**: canonical
**Last verified against code**: 2026-01-24
**Key code references**: `Cargo.toml`, `crates/*/CLAUDE.md`

This index organizes Casparian Flow documentation by status. Canonical docs must match code; archived docs are historical.

---

## Header Convention

All canonical docs should include a header block:

```
Status: canonical|plan|archived
Last verified against code: YYYY-MM-DD
Key code references: <paths>
```

---

## Canonical (Must Match Code)

These docs are the source of truth and must accurately reflect the current implementation.

### Root Documentation
| Document | Purpose |
|----------|---------|
| [CLAUDE.md](../CLAUDE.md) | LLM entry point: architecture, invariants, coding standards |
| [ARCHITECTURE.md](../ARCHITECTURE.md) | Detailed system design, protocol, crate map |
| [README.md](../README.md) | Repository overview and quickstart |

### Reference Documentation
| Document | Purpose | Key Code References |
|----------|---------|---------------------|
| [docs/trust_guarantees.md](trust_guarantees.md) | Trust model and security | `crates/casparian_worker/src/worker.rs`, `crates/casparian/src/trust/config.rs` |
| [docs/v1_scope.md](v1_scope.md) | v1 scope and success metrics | - |
| [docs/v1_checklist.md](v1_checklist.md) | v1 delivery checklist | - |
| [docs/schema_rfc.md](schema_rfc.md) | Schema contract system | `crates/casparian_schema/` |
| [docs/fix_schema.md](fix_schema.md) | FIX protocol schema spec | `parsers/fix/` |

### Crate-Level Documentation
Each crate has its own `CLAUDE.md` with implementation details:
- `crates/casparian/CLAUDE.md` - Main CLI binary
- `crates/casparian_sentinel/CLAUDE.md` - Control plane
- `crates/casparian_worker/CLAUDE.md` - Execution plane
- `crates/casparian_db/CLAUDE.md` - Database abstraction
- `crates/casparian_protocol/CLAUDE.md` - Binary protocol
- `crates/casparian_schema/CLAUDE.md` - Schema contracts
- `crates/casparian_mcp/CLAUDE.md` - MCP integration
- `crates/casparian_backtest/CLAUDE.md` - Backtest engine

---

## ADRs / Decisions

Architecture Decision Records document key design choices.

| ADR | Title | Location |
|-----|-------|----------|
| ADR-017 | Tagging vs Extraction Rules | [docs/decisions/ADR-017-tagging-vs-extraction-rules.md](decisions/ADR-017-tagging-vs-extraction-rules.md) |
| ADR-018 | Worker Cap Simplifications | [docs/decisions/ADR-018-worker-cap-simplifications.md](decisions/ADR-018-worker-cap-simplifications.md) |
| ADR-019 | Multi-Output Sink Routing | [docs/decisions/ADR-019-multi-output-sink-routing.md](decisions/ADR-019-multi-output-sink-routing.md) |
| ADR-020 | Tauri GUI | [docs/decisions/ADR-020-tauri-gui.md](decisions/ADR-020-tauri-gui.md) |
| ADR-021 | AI Agentic Iteration | [docs/decisions/ADR-021-ai-agentic-iteration-workflow.md](decisions/ADR-021-ai-agentic-iteration-workflow.md) |

See also: ADR summary table in [CLAUDE.md](../CLAUDE.md#architecture-decision-records)

---

## Plans (Future / In-Progress)

These docs describe planned or in-progress work that may not yet match code.

| Document | Purpose |
|----------|---------|
| [docs/execution_plan.md](execution_plan.md) | Current implementation plan |
| [docs/execution_plan_mcp.md](execution_plan_mcp.md) | MCP integration plan |
| [docs/local_control_plane_api_plan.md](local_control_plane_api_plan.md) | Control API design |
| [specs/tauri_ui.md](../specs/tauri_ui.md) | Tauri UI specification |
| [specs/tauri_mvp.md](../specs/tauri_mvp.md) | Tauri MVP specification |

### Specs (Feature Plans)
| Document | Purpose |
|----------|---------|
| [specs/features/export.md](../specs/features/export.md) | Export functionality |
| [specs/features/streaming_scanner.md](../specs/features/streaming_scanner.md) | Streaming scanner |
| [specs/features/variant_grouping.md](../specs/features/variant_grouping.md) | Variant grouping |

### Meta-Workflows
| Document | Purpose |
|----------|---------|
| [specs/meta/workflow_manager.md](../specs/meta/workflow_manager.md) | Workflow orchestration |
| [specs/meta/feature_workflow.md](../specs/meta/feature_workflow.md) | Feature development workflow |
| [specs/meta/tui_testing_workflow.md](../specs/meta/tui_testing_workflow.md) | TUI testing workflow |

---

## Archive (Historical)

These docs are outdated and kept for historical reference only. **Do not rely on archived docs for current implementation details.**

### docs/archive/
| Document | Why Archived |
|----------|--------------|
| [DUCKDB_MIGRATION_PLAN.md](archive/DUCKDB_MIGRATION_PLAN.md) | Migration complete; historical notes only |
| [PR_STATUS.md](archive/status/PR_STATUS.md) | Point-in-time status snapshot |
| [claude_docs_legacy/](archive/claude_docs_legacy/) | Outdated LLM context (SQLite references, old UI) |

### specs/archive/
| Document | Why Archived |
|----------|--------------|
| [db.md](../specs/archive/db.md) | Outdated: assumed async actor model with SQLite |
| [db_actor.md](../specs/archive/db_actor.md) | Outdated: async actor design superseded by DuckDB |

---

## Product / Business Documents

| Document | Purpose |
|----------|---------|
| [docs/product/README.md](product/README.md) | Product documentation index |
| [docs/product/pricing_v2_refined.md](product/pricing_v2_refined.md) | Pricing system |
| [docs/product/validated_personas.md](product/validated_personas.md) | ICP and workflows |
| [docs/product/domain_intelligence.md](product/domain_intelligence.md) | File format catalog |

---

## Agent / LLM Documentation

| Document | Purpose |
|----------|---------|
| [docs/agent/AGENTS_CHECKLIST.md](agent/AGENTS_CHECKLIST.md) | Agent development checklist |
| [docs/agent/BASELINE.md](agent/BASELINE.md) | Baseline configuration |
| [docs/agent/SCAN_REPORT.md](agent/SCAN_REPORT.md) | Scan report format |

---

## Verification Notes

Docs marked "canonical" were verified against code on 2026-01-24:
- Protocol opcodes match `crates/casparian_protocol/src/lib.rs`
- Crate map matches `Cargo.toml` workspace members
- Python plugin contract matches `crates/casparian_worker/shim/bridge_shim.py`
- Trust defaults match `crates/casparian_worker/src/worker.rs`
- Database is DuckDB-only (no SQLite/sqlx)
- Bridge transport is TCP loopback (not AF_UNIX)
- Scout is a module in `crates/casparian/src/scout/`, not a separate crate

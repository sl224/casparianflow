# V1 Scope: DFIR Artifact Workbench

Status: Active
Date: January 2026
Owner: Product

## Goal
Deliver a **deterministic, governed data build system** for DFIR artifact parsing.
Turn Windows artifacts (EVTX as flagship) into queryable datasets with strict
schema contracts, quarantine semantics, and per-row lineage for chain of custody.

**Core value proposition:** "Turn DFIR parsing into an auditable, repeatable,
backfillable dataset build process." NOT "another EVTX parser."

## Target ICP
Primary: DFIR / Incident Response artifact parsing teams

| Title | Role | Technical Level |
|-------|------|-----------------|
| **DFIR Engineer** | Parse and analyze digital forensic artifacts | Writes Python; CLI-comfortable |
| **Forensic Engineer** | Build evidence timelines; court-defensible analysis | Python/PowerShell; evidence handling |
| **IR Engineer** | Incident response; rapid triage | Python; artifact parsing |
| **Detection Engineer** (consumer) | Consume parsed outputs for detection logic | SQL; queries outputs |

**Environment:** Air-gapped evidence servers; offline collection workflows; chain-of-custody requirements

## Trust Primitives / Integrity Guarantees

| Guarantee | Description |
|-----------|-------------|
| **Reproducibility** | Same inputs + same parser bundle hash → identical outputs |
| **Per-row lineage** | Every row: `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Authoritative validation** | Schema contracts enforced in Rust; invalid rows never silently coerce |
| **Quarantine semantics** | Invalid rows go to quarantine with error context; partial success is safe |
| **Evidence-grade manifests** | Export includes: inputs + hashes + parser IDs + outputs + timestamps |

## Core User Journey (v1)

### Primary Workflow: Case Folder Ingestion
1. Point Casparian at a directory of EVTX files (or extracted artifacts).
2. Select the EVTX parser (auto-detect is a stretch goal).
3. Parse into `evtx_events` and `evtx_events_quarantine`.
4. Query by time range, host, event_id, or user to build a timeline.
5. Review quarantine summary and lineage metadata.

### Additional DFIR Workflows
| Workflow | Description |
|----------|-------------|
| **Evidence bundle ingestion** | Normalize offline collection zip into tagged inputs |
| **Offline collector zip ingestion** | Extract and tag folder tree from collector output |
| **Quarantine triage loop** | Review violations by type; sample rows; trace to source |
| **Backfill planning** | When parser version changes, see exactly what needs reprocessing |

### What v1 is NOT
- NOT streaming (batch files at rest only)
- NOT an orchestrator/scheduler
- NOT a BI tool
- NOT "no-code"
- NOT AI-dependent (AI assistance is optional; MCP enables it but doesn't require it)

## In Scope
- EVTX parser with `evtx_events` table.
- Rust-side schema validation with quarantine split.
- Quarantine policy controls (allow/threshold/lineage).
- Decimal + timezone-aware timestamps in schema types.
- Lineage columns injected into outputs.
- Output sinks: Parquet and DuckDB with per-output routing.
- CLI quickstart + demo dataset for EVTX timeline walkthrough.
- Productized onboarding sprint kit (scope, acceptance criteria, handoff checklist).
- Minimal TUI support: Parser Bench + Jobs view.
- **MCP Server for AI-assisted workflows** (see MCP section below).

## Out of Scope
- Additional DFIR parsers (Shimcache, Amcache, Prefetch, $MFT) beyond v1 demo.
- Additional vertical parsers (HL7, PST, CoT, etc.).
- Postgres/MSSQL sinks.
- Streaming/Kafka/Redpanda integration.
- Multi-node scheduling or server deployment.
- Advanced approval UI.
- Autonomous AI parser authoring (AI proposes, human approves via MCP).

## MCP Server (AI Integration)

### What is MCP?

Model Context Protocol (MCP) is Anthropic's open standard for AI tool integration. An MCP
server exposes tools that AI assistants (Claude, etc.) can invoke programmatically.

### Why MCP for v1?

**Value proposition:** "Let Claude iterate on your parser while you watch."

DFIR workflows benefit from AI assistance because:
1. **Schema inference** is tedious but pattern-matchable.
2. **Edge case discovery** via backtest loops benefits from rapid iteration.
3. **Quarantine triage** often requires proposing schema/parser changes.

MCP makes Casparian a **force multiplier** rather than just another CLI tool.

### MCP Tools (v1 Scope)

| Tool | Description | Human Gate |
|------|-------------|------------|
| `casparian_scan` | Scan directory, return file metadata | None (read-only) |
| `casparian_preview` | Preview parser output on sample files | None (read-only) |
| `casparian_backtest` | Run parser against corpus, return summary | None (read-only) |
| `casparian_run` | Execute parser, write outputs | **Requires approval** |
| `casparian_schema_propose` | Propose schema changes (ephemeral) | None (ephemeral) |
| `casparian_schema_promote` | Generate schema-as-code for publish | **Requires approval** |
| `casparian_quarantine_summary` | Get violation summary with samples | None (read-only) |
| `casparian_query` | Run SQL on DuckDB outputs | None (read-only) |

### MCP Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    AI Assistant (Claude)                         │
│  "Parse these EVTX files and build a timeline query"            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ MCP Protocol (JSON-RPC over stdio)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Casparian MCP Server                          │
│  • Exposes tools as MCP resources                               │
│  • Translates MCP calls → CLI commands                          │
│  • Returns structured results (JSON/Arrow)                      │
│  • Enforces human gates on write operations                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Casparian Core                                │
│  • Parser execution (Python bridge / native runtime)            │
│  • Schema validation (Rust)                                     │
│  • Output sinks (Parquet / DuckDB)                              │
└─────────────────────────────────────────────────────────────────┘
```

### AI Iteration Workflow (via MCP)

```
1. User: "Parse these EVTX files"
   └─ Claude calls casparian_scan → returns file list

2. Claude: Proposes parser + schema
   └─ Claude calls casparian_preview → sees sample output

3. Claude: Runs backtest
   └─ Claude calls casparian_backtest → gets pass/fail + quarantine %

4. Claude: Adjusts schema based on violations
   └─ Claude calls casparian_schema_propose → ephemeral contract
   └─ Claude calls casparian_backtest → improved pass rate

5. Claude: Ready to write outputs
   └─ Claude calls casparian_run → **BLOCKED: requires human approval**
   └─ Human approves → outputs written

6. Claude: Queries results
   └─ Claude calls casparian_query → returns timeline data
```

### MCP Success Criteria

- AI can iterate on parser/schema without human intervention (read-only ops).
- Write operations (run, promote) require explicit human approval.
- Backtest results include machine-readable violation context.
- Time-to-first-query via MCP: <10 minutes for demo dataset.

### MCP Dependencies

- `EphemeralSchemaContract` for fast iteration (ADR-021).
- `ViolationContext` with samples/distributions for AI learning.
- Progress streaming for long-running backtests.

## Success Metrics

### Adoption Metrics
- 3 DFIR pilots completed.
- 2 pilots willing to pay >= $2K/month (or equivalent per-engagement).

### Performance Metrics
- Timeline query returns results in <= 5 minutes on demo dataset.
- Time-to-first-query on case folder: <15 minutes.

### Integrity Metrics
- Reproducibility check: same inputs + parser hash → identical outputs (100%).
- Backfill accuracy: files/jobs selected correctly for reprocessing (100%).
- % rows quarantined per parser version (track per parser).

### UX Metrics
- Quarantine and lineage visible in CLI/TUI.
- Quarantine summary shows: violation types, sample rows, pointers to source.

## Dependencies
- Canonical DataType supports Decimal and timestamp_tz.
- Rust validator + quarantine policy enforcement.
- EVTX parser with stable schema.
- Scout detection signature for EVTX files.

## Risks
- Access to real DFIR artifacts for testing.
- Trust/validation requirements for evidence handling.
- Performance on large EVTX archives (multi-GB).

## Next Milestones
- M1: EVTX parser + events table shipped.
- M2: Rust validation + quarantine policy shipped.
- M3: Demo + pilot onboarding kit shipped.

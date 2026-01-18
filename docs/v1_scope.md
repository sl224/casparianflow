# V1 Scope: Trade Break Workbench (Finance)

Status: Proposed
Date: TBD
Owner: Product

## Goal
Deliver a finance-first, local-first workflow that lets trade support teams
turn FIX logs into queryable SQL and resolve trade breaks in minutes.

## Target ICP
- Trade Support Analyst / Middle Office / FIX Ops
- SQL-capable, not Python-heavy
- Operates under T+1 settlement pressure

## Core User Journey (v1)
1. Point Casparian at a FIX log directory.
2. Auto-detect FIX and suggest the FIX parser.
3. Parse logs into `fix_order_lifecycle` and related tables.
4. Query by `cl_ord_id` to reconstruct a full order lifecycle.
5. Review quarantine summary and lineage metadata.

## In Scope
- FIX log parser with `fix_order_lifecycle` table.
- Rust-side schema validation with quarantine split.
- Quarantine policy controls (allow/threshold/lineage).
- Decimal + timezone-aware timestamps in schema types.
- Lineage columns injected into outputs.
- Output sinks: Parquet and DuckDB.
- CLI quickstart + demo dataset for trade break walkthrough.
- Minimal TUI support: Parser Bench + Jobs view.

## Out of Scope
- AI-assisted parser generation (MCP tools).
- Additional vertical parsers (HL7, PST, CoT, etc.).
- Postgres/MSSQL sinks.
- Streaming/Kafka/Redpanda integration.
- Multi-node scheduling or server deployment.
- Per-output sinks and advanced approval UI.

## Success Metrics
- 3 finance pilots completed.
- 2 pilots willing to pay >= $2K/month.
- Break resolution time <= 5 minutes in demo.
- Time-to-first-value <= 15 minutes on first run.
- Quarantine and lineage visible in CLI/TUI.

## Dependencies
- Canonical DataType supports Decimal and timestamp_tz.
- Rust validator + quarantine policy enforcement.
- FIX parser with lifecycle reconstruction.
- Scout detection signature for FIX logs.

## Risks
- Access to real FIX logs for testing.
- Performance on multi-million row logs.
- Governance UX friction slows onboarding.

## Next Milestones
- M1: FIX parser + lifecycle table shipped.
- M2: Rust validation + quarantine policy shipped.
- M3: Demo + pilot onboarding kit shipped.

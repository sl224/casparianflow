# State Store Architecture (Pre-v1)

## Control Plane vs Data Plane

Casparian Flow splits persistence into two planes:

- **Control plane**: authoritative state for jobs, sessions, approvals, routing, and scans.
- **Data plane**: produced outputs (e.g., Parquet) and query-facing catalog state.

Control plane data is small, mutation-heavy, and must be deterministic. Data plane outputs are large, append-heavy, and optimized for reads.

## Local Mode Defaults

**Control plane: SQLite**

- Deterministic, single-file, predictable locking.
- Boring and ubiquitous.
- Well-suited for sentinel’s single-writer authority.

**Data plane: Parquet + DuckDB query catalog**

- Outputs are stored as Parquet for durability and interoperability.
- DuckDB provides a fast, local SQL surface over Parquet.
- The DuckDB catalog is read-facing only and derived from artifacts.

## Sentinel as the Single Writer

Even if the underlying DB supports concurrent writes, **the sentinel remains the single logical writer**. CLI/TUI only mutate state through the control plane. This avoids split-brain behavior and keeps invariants centralized.

## Semantic Boundary

Callers do **not** execute raw SQL. They call semantic operations:

- `claim_next_job`
- `persist_scan_batch`
- `record_artifacts`

The store layer enforces invariants and owns the schema so the rest of the codebase can remain data-focused and type-safe.

## Pre-v1 Schema Policy

Pre-v1 development explicitly favors **destructive resets over migrations**. When schema changes, the state store is wiped and recreated (per project rules). Migration tooling and backward compatibility are deferred until v1.

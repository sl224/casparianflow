# Third-Party Wrapping Policy

## Context

Casparian Flow is local-first and must be stable for regulated workflows
(finance, healthcare, legal, defense). The system emphasizes explicit contracts,
deterministic behavior, and clear failure modes. At the same time, the product
prioritizes simple, direct interfaces (CLI/TUI first) and fast iteration.

Relevant decisions and specs:
- Parser interface favors simplicity: "data is just data" (no wrapper classes).
- CLI/TUI are the primary interface; avoid SDK sprawl.
- Database layer already plans a unified wrapper to isolate backend differences.

This creates a tension between stability at public boundaries and avoiding
unnecessary abstraction inside the data path.

## Rationale

Eskil Steenberg's principle ("anything we don't own we should wrap") is about
controlling change and meaning:
- Dependency upgrades should not force widespread API changes.
- External semantics should be translated once, in one place.
- Swapping vendors should be possible without rewriting callers.

However, applying this everywhere increases boilerplate and slows iteration.
For Casparian, the highest value is at public, long-lived boundaries. Internal
modules can use third-party types freely as long as they do not leak into
public APIs.

## Proposal

Adopt a selective rule:

1. Wrap at public boundaries that must be stable
   - Public crate APIs (types exposed in `pub` structs/enums/traits).
   - External protocols and config contracts (CLI config files, API schemas).
   - Storage boundaries that must survive dependency changes (DB backends).

2. Allow third-party types internally
   - Data path and implementation details can use third-party types if the
     public API is insulated.
   - This keeps iteration speed while protecting external users.

3. Prefer minimal, purposeful wrappers
   - Provide conversion functions at the boundary.
   - Avoid wrapper classes when the product spec explicitly favors direct data.

## What To Wrap (Concrete Targets)

High-ROI boundaries in this codebase:
- Database backend abstraction (DbConnection/DbTransaction wrapper).
- Public error types (avoid embedding third-party error enums directly).
- Public ID/timestamp types used in contracts (stable across crates).
- Tool result and input schema types exposed to clients.

Likely OK to keep third-party types internal:
- Arrow RecordBatch in internal runner implementations.
- sqlx pools inside DB adapter implementations.
- chrono/uuid inside internal storage structs that are not public API.

## Examples

Bad (leaks dependency into public API):
- `ExecutionResult` exposes `arrow::array::RecordBatch` directly.
- `DbPool` and `DbRow` type aliases expose `sqlx` types in a public crate.
- Public tuple wrappers still expose `uuid::Uuid` directly.

Good (wrapped boundary, internal use still allowed):
- Public `DbConnection` type with internal `sqlx`/DuckDB pools.
- Public `ScopeId` type with private inner and conversion helpers.
- Public error enum with `#[source]` for third-party errors, not public variants.

## Decision Summary

We will not enforce "wrap everything." We will:
- Enforce wrapping at public, external, or long-lived boundaries.
- Allow third-party types internally to preserve speed and simplicity.
- Use wrapper decisions to align with existing product principles and specs.

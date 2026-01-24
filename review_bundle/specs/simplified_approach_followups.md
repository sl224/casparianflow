# Simplified Approach Followups

## Purpose
Capture where we want to apply the "concrete first, abstract later" pattern
across the codebase, so we can address it after the DB actor work lands.

## Guiding Principles
- Prefer one concrete implementation until a second backend is real.
- Avoid trait-object adapters and capability flags unless needed now.
- Keep async boundaries explicit and owned (actor or dedicated thread).
- Preserve stable public APIs; change internals first.

## Candidate Areas (Review Later)
1) Runner abstraction (Dev vs Prod)
- Check for unnecessary trait hierarchies or mode-specific interfaces.
- Prefer a single concrete runner with a mode flag if divergence is small.

2) Output sinks (Parquet/SQLite/CSV)
- Prefer concrete sinks per format.
- Add a simple enum dispatcher only when multiple sinks are in use together.

3) Storage layer (`storage/sqlite.rs` -> `storage/db.rs`)
- Keep one concrete DB implementation until Postgres is real.
- Avoid adapter layers that mirror hypothetical backends.

4) AI assistance wiring (future)
- Ensure assistive features are thin wrappers over concrete services.
- Avoid generalized registries unless we need dynamic loading.

## Evaluation Checklist
- Is there more than one real backend today?
- Does the abstraction hide thread affinity or async boundaries?
- Are there capability flags that signal a leaky abstraction?
- Would a concrete type + enum backend be simpler?
- Can we keep API stable while simplifying internals?

## Next Steps
- After DB actor implementation: scan modules above for over-abstraction.
- Document any simplifications as small refactors (one per module).
- Keep changes incremental; avoid cross-cutting rewrites.

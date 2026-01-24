## Pre-v1 Development Rules

Until v1 is released, this project has NO production users and NO data to preserve. Do NOT propose or implement:

1. **Database migrations** - Just delete and recreate tables. No ALTER TABLE, no migration scripts, no versioned schema changes.

2. **Backwards compatibility** - No deprecation warnings, no dual-write patterns, no "keep old field for compatibility."

3. **API versioning** - No `/v1/`, `/v2/` endpoints. No deprecated parameters. Just change the API.

4. **Gradual rollouts** - No feature flags for safe rollout. No A/B testing infrastructure.

5. **Data preservation during refactors** - If schema changes, wipe and restart. No data migration scripts.

**What to do instead:**
- Change schemas directly in code
- Delete `~/.casparian_flow/casparian_flow.duckdb` when schema changes
- Update all call sites when APIs change
- Break things fast, fix them fast

**When this changes:** These rules expire at v1 release. After v1, backwards compatibility and migrations become required.

---

## Engineering Ethos (Pre-v1 and Beyond)

We follow the "make illegal states unrepresentable" ethos in the spirit of
world-class codebases like Doom and engineers like Jon Blow, Casey Muratori,
and John Carmack. Prefer compile-time guarantees and type-driven design over
stringly-typed logic or runtime patching. If a bug class can be prevented by
structure, choose the structural approach.

### Core Principles

- **Parse, don't validate** — Convert unstructured → structured at boundaries. Core never sees invalid data.
- **Data dominates** — Right data structures first; algorithms follow.
- **State is liability** — Minimize fields. Derive what you can compute.
- **Boundaries do the heavy lifting** — Defensive code at edges; core trusts its inputs.
- **Total functions over partial** — Prefer functions that always succeed.
- **Boring code > clever code** — Explicit beats implicit. Juniors should understand it.
- **Fail loud, not silent** — No default fallbacks that mask corruption.
- **The loaded gun** — Avoid APIs where misuse compiles successfully.

### Anti-Pattern Catalog (Reference by Name)

| Anti-Pattern | Description | Fix |
|--------------|-------------|-----|
| **Silent Corruption** | `.unwrap_or_default()` hiding bad DB data | Return `Result`, fail at boundary |
| **Stringly Typed** | `match status.as_str()` with string literals | Use enum matching |
| **Shotgun Validation** | Same `if x.is_empty()` check in 10 places | Newtype that guarantees validity |
| **Zombie Object** | Struct needs `.init()` after `new()` | Return ready-to-use or `Result` |
| **Primitive Obsession** | `i64` for both `file_id` and `job_id` | Newtypes: `FileId(i64)`, `JobId(i64)` |
| **Dual Source of Truth** | Enum in Rust, different CHECK in SQL | `enum.as_str()` is single source |
| **Boolean Blindness** | `editing: bool, creating: bool` (mutex) | `enum Mode { Editing, Creating }` |
| **Lossy Cast** | `file_id as i32` truncating silently | `i32::try_from(file_id)?` |
| **Magic String Literals** | `"PENDING"` hardcoded in 40 files | `Status::Pending.as_str()` |

### Pre-Commit Checklist

- [ ] No `.unwrap_or_default()` on parsed enums
- [ ] Status/state uses enums, not string comparison
- [ ] Unstructured data parsed to types at boundaries
- [ ] No duplicated constants between Rust and SQL
- [ ] Structs valid from construction (no `.init()`)
- [ ] No `as i32/i64` on values that could overflow; use `try_from`
- [ ] Multiple bools aren't encoding mutually exclusive states

---

## Scan + Persist Line-Rate Plan (codex/scan_persist_line_rate)

Working agreements:
- Always read and update: `codex/scan_persist_line_rate/PROGRESS.md`
- Make changes in small milestones; keep diffs reviewable.
- Prefer data-oriented reductions: fewer allocations, fewer copies, fewer passes over data.
- Avoid new dependencies unless explicitly justified by benchmark wins.
- After each milestone:
  - run: `cargo test -p casparian`
  - run: `cargo bench -p casparian --bench scanner_perf` (or at least the relevant group)
  - record results + next step in `PROGRESS.md`

Debuggability:
- Add tracing timings around: walk, batch persist, and any post-scan work.
- If behavior changes, add or adjust tests near the changed module.

Exit criteria:
- If scanner_full_scan is within ~1.2–1.5x walk_only on the criterion fixture (and db_write no longer dominates),
  stop and write up final results in `PROGRESS.md`.

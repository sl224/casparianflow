# AGENTS/CLAUDE.md Checklist

## Pre-v1 Rules (Non-Negotiable)
- [ ] No database migrations - delete `~/.casparian_flow/casparian_flow.duckdb` on schema changes
- [ ] No backwards compatibility / API versioning / gradual rollouts
- [ ] No data preservation during refactors

## Engineering Principles
- [ ] "Parse, don't validate" - Convert unstructured → structured at boundaries
- [ ] "Data dominates" - Right data structures first, algorithms follow
- [ ] "State is liability" - Minimize state, derive what you can compute
- [ ] "Boundaries do heavy lifting" - Defensive code at edges; core trusts inputs
- [ ] "Boring code > clever code" - Junior-readable in 30 seconds
- [ ] "Fail loud, not silent" - Errors impossible to ignore
- [ ] "Delete unused code" - Dead code misleads and hides bugs

## Anti-Patterns to Fix
| Pattern | Problem | Fix |
|---------|---------|-----|
| Silent Corruption | `.unwrap_or_default()` hides bad DB data | Use `?` with typed error |
| Stringly Typed | `match status.as_str()` misses typos | Use enum matching |
| Shotgun Validation | Same check in 10 places | Parse once, use validated type |
| Zombie Object | Struct needs `.init()` after `new()` | Valid from construction |
| Primitive Obsession | `fn f(file_id: i64, job_id: i64)` swappable | Use newtypes |
| Dual Source of Truth | Rust enum vs SQL CHECK diverge | Single authoritative source |
| Boolean Blindness | `editing: bool, creating: bool` both true | Use enum for exclusive states |
| Lossy Cast | `x as i32` silently truncates | Use `try_from` |
| Magic String Literals | `"PENDING"` in 40 files | Centralized constants |

## Pre-Commit Checklist
- [ ] No `.unwrap_or_default()` on parsed enums
- [ ] Status/state checks use enums, not strings
- [ ] Unstructured data converted to types at boundaries
- [ ] No duplicated constants between Rust and SQL
- [ ] Structs valid from construction
- [ ] No `as i32` on potentially large values
- [ ] Multiple bools aren't encoding exclusive states

## Code Quality Requirements
- [ ] Zero warnings: `cargo check` + `cargo clippy` clean
- [ ] Use DuckDB via `casparian_db::DbConnection` - no other database libraries
- [ ] No unwrap in lib - use `?` or `expect()` with context
- [ ] Channels over locks: `tokio::sync::mpsc` or `std::sync::mpsc`

## Concurrency Model (Jon Blow style)
- [ ] No async/await in MCP core
- [ ] No Tokio runtime in MCP path
- [ ] Use explicit threads + channels + job-system style
- [ ] Single-owner state + message passing over locks

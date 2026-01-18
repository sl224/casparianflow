# Code Quality Workflow

**Type:** Meta-specification (Code Quality Standards)
**Version:** 1.1
**Purpose:** Hard requirements for all coding tasks in Casparian Flow
**Related:** CLAUDE.md (references this), feature_workflow.md (applies this)
**Last Updated:** 2026-01-14

---

**IMPORTANT:** Follow this workflow for all coding tasks. These are hard requirements, not suggestions.

---

## Pre-Implementation Checklist

Before writing any code, verify these:

### 1. Check for Existing Modules

- Search codebase for similar functionality (`Grep`, `Glob`)
- If similar module exists: **STOP and ask** whether to extend, merge, or replace
- Never create duplicate implementations silently

```bash
# Example searches before implementing a new scanner
rg "fn scan" --type rust
rg "WalkParallel\|walkdir" --type rust
```

### 2. Identify Related Components

- What other modules will this interact with?
- Are there shared types that should be reused?
- Will this change affect existing callers?

### 3. Concurrency Analysis

For any async/threaded code:

- Identify all shared state
- Document synchronization strategy before coding
- Prefer channels (`mpsc`, `broadcast`) over shared mutable state (`Mutex`, `RwLock`)

### 4. State Machine Analysis (REQUIRED)

Before modifying any component with state transitions:

#### Step 1: Find the State Machine Definition

- Check the relevant spec file (e.g., `specs/views/discover.md` for Discover mode)
- Look for state machine diagrams, state enums, transition tables
- If working on TUI, also check `specs/tui.md` for global app state machine

#### Step 2: Verify Spec-Code Consistency

```bash
# Find state enum in code
rg "enum.*State|enum.*View|enum.*Mode" --type rust -A 10

# Compare with spec definition
```

**If spec and code are inconsistent:** STOP. Reconcile before proceeding:
1. Determine which is correct (spec or code)
2. Update the outdated one
3. Document the change in revision history

#### Step 3: Understand State Hierarchy

```
Global App State (View enum)
    └── Mode State (e.g., DiscoverViewState)
        └── Dialog State (e.g., EnteringPath, SourcesManager)
```

- How does this component's state fit into the hierarchy?
- What parent states must be active for this state to be reachable?
- What state transitions affect parent/child states?

#### When State Machine Documentation Is Required

| Scenario | Spec Required? | Location |
|----------|----------------|----------|
| TUI mode (user-facing states) | **Yes** | `specs/views/{mode}.md` |
| Background process states (>3 states) | **Yes** | Inline in module doc or `specs/` |
| Parser/iterator states | No | Code is the spec |
| Error variants | No | - |
| Simple binary state (on/off) | No | - |

**Rule of thumb:** If a user could observe the state (UI) or if transitions have complex rules (>3 states), document it.

#### Step 4: If No State Machine Exists

If the component has stateful behavior but no defined state machine:

1. Check if documentation is required (see table above)
2. If required: **STOP coding** and define the state machine:
   - State enum with all possible states
   - Transition diagram (ASCII or description)
   - Entry/exit conditions for each state
3. Get confirmation before implementing

#### State Machine Checklist

- [ ] State machine is defined in spec (or just created it)
- [ ] Code matches spec exactly (states, transitions)
- [ ] Understand how it fits into global state hierarchy
- [ ] New states/transitions documented in spec before coding

---

## Coding Standards (Hard Requirements)

### Alpha Development: No Migrations, No Deprecations

This is an **alpha application**. Breaking changes are expected.

| Don't Do | Do Instead |
|----------|------------|
| Schema migrations (`CREATE TABLE xxx_new`, `ALTER TABLE`) | Change schema directly, users delete DB |
| Backwards compatibility shims | Just change the code |
| Deprecation warnings | Remove old code entirely |
| Feature flags for old behavior | Replace old behavior |

**Why:** Migration code is complexity debt. In alpha, the cost of maintaining backwards compatibility exceeds the cost of users resetting state. Users can delete `~/.casparian_flow/` and start fresh.

**When this changes:** After 1.0 release, migrations become necessary.

### No Race Conditions or Concurrency Bugs

This project uses BOTH sync (`rayon`, `ignore`) and async (`tokio`, `sqlx`) patterns. Choose the right primitives for your context.

#### Async Context (Tokio runtime)

| Do | Don't |
|----|-------|
| `tokio::sync::mpsc` for channels | `std::sync::mpsc` (blocks runtime) |
| `tokio::sync::Mutex` when needed | `std::sync::Mutex` (blocks runtime) |
| `spawn_blocking` for CPU work | Long CPU work in async task |
| `tokio::time::sleep` | `std::thread::sleep` (blocks runtime) |

#### Sync Context (CPU parallelism)

| Do | Don't |
|----|-------|
| `rayon` for data parallelism | Manual `std::thread::spawn` |
| `ignore::WalkParallel` for filesystem | `walkdir` + manual threads |
| `std::sync::mpsc` for thread comms | Async channels in sync code |
| `crossbeam_channel` for complex patterns | Rolling your own sync |

#### General Patterns

| Pattern | Use | Avoid |
|---------|-----|-------|
| Cross-thread data | Channels (async or sync per context) | `Arc<Mutex<T>>` for complex state |
| Progress updates | Channel with `try_send` | Shared counters with locks |
| Atomic counters | `AtomicUsize` for simple counts | `Mutex<usize>` |

**When using locks is unavoidable:**

- Hold locks for minimal duration
- Never hold locks across `.await` points
- Document lock ordering to prevent deadlocks

```rust
// BAD: Lock held across await
let guard = self.state.lock().await;
self.do_async_work().await;  // Still holding lock!
drop(guard);

// GOOD: Lock released before await
let data = {
    let guard = self.state.lock().await;
    guard.clone()
};
self.do_async_work().await;
```

### Data-Oriented Design (No Stringly Types)

```rust
// GOOD: Type-safe, compiler-verified
pub struct SourceId(pub String);
pub enum FileStatus { Pending, Scanned, Failed }
pub struct ScanConfig { threads: usize, batch_size: usize }

// BAD: Stringly-typed, error-prone
fn scan(source_id: String, status: String, config: HashMap<String, String>)
```

**Rules:**

| Pattern | Example |
|---------|---------|
| Newtypes for IDs | `SourceId(String)`, `JobId(Uuid)` |
| Enums for states | `FileStatus`, `ExtractionStatus` |
| Structs for config | `ScanConfig`, not `HashMap<String, Value>` |
| Parse at boundaries | Convert strings to typed values at entry points |

### Module Boundaries

- Public API should be minimal (`pub use` specific items)
- Internal helpers stay private
- Cross-module communication via well-defined types
- No circular dependencies between crates

### Database Access Standard

**Use `sqlx`, NOT `rusqlite`.** This is a project-wide standard.

| Aspect | `sqlx` (Required) | `rusqlite` (Don't Use) |
|--------|-------------------|------------------------|
| Async | Native async/await | Sync only, blocks runtime |
| Compile-time checks | Query validation at compile time | Runtime errors |
| Connection pooling | Built-in | Manual |

All database operations go through the single database at `~/.casparian_flow/casparian_flow.duckdb`.

### No Dead Code

Dead code is technical debt. It confuses readers, bloats binaries, and decays over time.

| Rule | Enforcement |
|------|-------------|
| No unused functions | `cargo check` must pass without `dead_code` warnings |
| No unused imports | `cargo check` must pass without `unused_imports` warnings |
| No unused variables | Prefix with `_` only if intentionally unused AND documented |
| No commented-out code | Delete it; git has history if you need it back |

**Exceptions (must be documented):**

```rust
// KEEP: Used by external crate via #[no_mangle] FFI
#[allow(dead_code)]
pub fn ffi_entry_point() { }

// KEEP: Test helper used only in #[cfg(test)]
#[cfg(test)]
fn create_test_fixture() { }

// KEEP: Placeholder for imminent feature (link to issue/spec)
// TODO(#123): Will be used when extraction wizard lands
#[allow(dead_code)]
pub struct ExtractionConfig { }
```

**Not valid exceptions:**
- "Might need this later" - delete it, add back when needed
- "Works in other codebase" - delete it if not used here
- "Too hard to delete" - refactor to make it deletable

### No Compiler Warnings

Warnings indicate potential bugs or code smell. A clean build must have zero warnings.

| Requirement | Command |
|-------------|---------|
| No warnings in library code | `cargo check 2>&1 \| grep -c warning` must be 0 |
| No warnings in tests | `cargo test 2>&1 \| grep -c warning` must be 0 |
| Clippy clean | `cargo clippy -- -D warnings` must pass |

**Allowed suppressions (must be documented):**

```rust
// Clippy false positive: we intentionally use this pattern for X reason
#[allow(clippy::needless_return)]
fn complex_control_flow() -> Result<()> {
    // ... justified reason in comment
}

// Suppress in Cargo.toml for crate-wide issues only when truly unavoidable
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(coverage)"] }
```

**Process for new warnings after compiler/clippy upgrade:**

1. Fix the warning (preferred)
2. If unfixable, add `#[allow(...)]` with justification comment
3. Track in issue if it's a known clippy bug

### Enforcement Mechanisms

Rules without enforcement become suggestions. This section specifies HOW standards are enforced.

#### Automated Checks (Local)

| Check | Command | When |
|-------|---------|------|
| Compile | `cargo check` | Before every commit |
| Clippy | `cargo clippy -- -D warnings` | Before every commit |
| Tests | `cargo test` | Before commits touching critical paths |

**Note:** No CI pipeline currently exists. All enforcement is local. This may change post-1.0.

#### Manual Checks (Code Review)

| Check | Reviewer Responsibility |
|-------|------------------------|
| Dead code justifications | Verify `#[allow(dead_code)]` has explanatory comment |
| State machine sync | Compare code enum to spec diagram |
| Concurrency strategy | Verify channels vs locks rationale documented |
| Database standard | Verify `sqlx` is used, not `rusqlite` |

#### Failure Consequences

| Violation | Action |
|-----------|--------|
| `cargo check` fails | Fix immediately, cannot commit broken code |
| `cargo clippy` warning | Fix or add `#[allow(...)]` with comment |
| Review finding | Address feedback before merge |
| Post-merge violation | Next developer to touch the code must fix |

---

## Testing Requirements

### What to Test

| Category | Required | Example |
|----------|----------|---------|
| Critical path | **Yes** | `scan_source()` actually discovers files |
| Error cases | **Yes** | Invalid path returns proper error |
| Edge cases | **Yes** | Empty directory, permission denied |
| Integration | **Yes** | Scanner + DB persistence together |
| Performance | When relevant | Scan 10K files in <5s |

### What NOT to Test

- Don't mock core functionality (use real DBs, real filesystems)
- Don't test private implementation details
- Don't write tests that just echo the implementation
- Don't test third-party library behavior

**TUI Testing:** For TUI-specific testing requirements (TMux-based testing), see `CLAUDE.md` section "TUI Development & Debugging".

### Test Pattern

```rust
#[tokio::test]
async fn test_failure_recording() {
    // SETUP: Real in-memory database (actual codebase pattern)
    let table = HighFailureTable::in_memory().await.unwrap();
    let scope_id = ScopeId("test-scope".to_string());

    // EXECUTE: The actual critical path
    let entry = FailureHistoryEntry {
        file_path: "/path/file.csv".to_string(),
        error_message: "Parse failed".to_string(),
        iteration: 1,
        timestamp: chrono::Utc::now(),
    };
    table.record_failure(&scope_id, entry).await.unwrap();

    // VERIFY: Observable outcomes, not internals
    let failures = table.get_failures(&scope_id).await.unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].file_path, "/path/file.csv");
}
```

### Test Naming Convention

```rust
// Pattern: test_<action>_<expected_outcome>
fn test_scan_empty_dir_returns_zero_files() { }
fn test_scan_invalid_path_returns_error() { }
fn test_scan_with_symlinks_follows_when_configured() { }
```

---

## Post-Implementation Checklist

Before marking a task complete:

- [ ] `cargo check` passes (no errors, **no warnings**)
- [ ] `cargo clippy` passes (no warnings in changed code)
- [ ] **Zero dead code** - no `#[allow(dead_code)]` without documented justification
- [ ] **Zero compiler warnings** - fix or document with `#[allow(...)]` + comment
- [ ] Critical path has test coverage
- [ ] No new `unwrap()` in production code (use `?` or proper error handling)
- [ ] No `clone()` where borrow would suffice
- [ ] Async code doesn't hold locks across await points
- [ ] Public APIs have doc comments
- [ ] State machine spec updated if states/transitions changed
- [ ] Code state enums match spec exactly

### When Checks Fail

| Failure | Recovery |
|---------|----------|
| `cargo clippy` on unchanged code | Fix anyway (broken window), or track in issue if large scope |
| Test flaky | Mark `#[ignore]` with issue link, don't skip silently |
| State machine spec outdated | If code is correct: update spec. If spec is correct: fix code. Never leave mismatched. |
| Pre-existing dead code found | Add justification comment or delete, don't leave unresolved |
| Build fails on dependency | Check `Cargo.lock`, pin version if upstream broke |

---

## Additional Guidelines

### Explicit State Machines

For complex state (TUI modes, scan states), use explicit enums:

```rust
// GOOD: Compiler enforces valid transitions
pub enum ScanState {
    Idle,
    Scanning { progress: ScanProgress },
    Complete { stats: ScanStats },
    Failed { error: ScanError },
}

impl ScanState {
    fn can_start(&self) -> bool {
        matches!(self, ScanState::Idle | ScanState::Complete { .. } | ScanState::Failed { .. })
    }
}

// BAD: Implicit state via booleans
pub struct Scanner {
    is_scanning: bool,
    is_complete: bool,
    is_failed: bool,
    // What if is_scanning AND is_complete are both true?
}
```

### Error Handling

- Use `thiserror` for error types
- Errors should be actionable (what went wrong + how to fix)
- Propagate with `?`, don't `unwrap()` in library code
- Log at boundaries, not deep in call stack

#### Unwrap Policy

| Context | `unwrap()` OK? | Alternative |
|---------|----------------|-------------|
| Library code (`src/`) | **No** | `?` or `expect()` with context |
| CLI entry points (`main.rs`) | Minimal | `expect()` with user-friendly message |
| Tests (`tests/`, `#[cfg(test)]`) | **Yes** | - |
| Examples (`examples/`) | **Yes** | - |
| Build scripts (`build.rs`) | **Yes** | - |

**Rationale:** Test failures should panic with clear location. Library code must propagate errors for proper handling upstream.

```rust
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Source path does not exist: {path}")]
    PathNotFound { path: String },

    #[error("Permission denied reading {path}: {source}")]
    PermissionDenied {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

### Performance Awareness

| Principle | Do | Don't |
|-----------|-----|-------|
| Batch operations | Insert 1000 rows per transaction | Insert 1 row at a time |
| Stream large data | Use iterators, process chunks | Load 1M files into `Vec` |
| Parallel CPU work | Use `rayon` for data parallelism | Spawn threads manually |
| Measure first | Profile with `cargo flamegraph` | Guess at bottlenecks |

#### Project-Specific Guidelines

| Operation | Threshold | Rationale |
|-----------|-----------|-----------|
| SQLite batch inserts | 1000 rows | Transaction overhead amortization |
| TUI progress updates | 100ms min interval | Avoid render thrashing |
| File streaming | >10MB | Below this, memory is fine |

**Note:** Exact thresholds should be validated via benchmarks. When in doubt, measure.

### Clone Guidelines

The rule "no `clone()` where borrow would suffice" has nuance:

| Pattern | Clone OK? | Notes |
|---------|-----------|-------|
| Thread ownership (`std::thread::spawn`) | **Yes** | Required for `'static` bound |
| Async task (`tokio::spawn`) | **Yes** | Required for `'static` bound |
| Small Copy types (`i32`, `bool`) | N/A | Use Copy, not Clone |
| Small String (<100 chars, cold path) | **Yes** | Simplicity > micro-optimization |
| Large data (>1KB, hot path) | **No** | Use `Arc` or redesign |
| Inside loop body | **Audit** | Often indicates design issue |

**When in doubt:** If clone is in a tight loop or on large data, consider `Arc`. Otherwise, clarity often beats avoiding a clone.

### Common Patterns in This Codebase

**Progress reporting via channels:**

```rust
let (tx, mut rx) = mpsc::channel::<Progress>(256);

// Producer (background task)
tokio::spawn(async move {
    for item in items {
        process(item);
        let _ = tx.try_send(Progress { count });  // Non-blocking
    }
});

// Consumer (TUI)
while let Some(progress) = rx.recv().await {
    update_ui(progress);
}
```

**Database transactions for batch inserts:**

```rust
let mut tx = pool.begin().await?;
for chunk in files.chunks(1000) {
    sqlx::query("INSERT INTO scout_files ...")
        .execute(&mut *tx)
        .await?;
}
tx.commit().await?;
```

**Parallel filesystem walking:**

```rust
use ignore::WalkBuilder;

WalkBuilder::new(path)
    .threads(num_cpus::get())
    .build_parallel()
    .run(|| {
        Box::new(|entry| {
            // Process entry
            WalkState::Continue
        })
    });
```

---

## Quick Reference

### Before You Code

1. Search for existing similar code
2. Identify shared types to reuse
3. Plan concurrency strategy
4. **Verify state machines** (spec ↔ code consistency)

### While You Code

- Newtypes for IDs
- Enums for states
- Channels over locks
- `?` over `unwrap()`

### Before You Commit

- `cargo check` (zero errors, **zero warnings**)
- `cargo clippy` (zero warnings)
- **No dead code** without documented justification
- Tests for critical path
- No locks across awaits
- State machine spec updated if changed

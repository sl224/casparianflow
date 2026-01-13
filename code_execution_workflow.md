# Code Quality Workflow

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

#### Step 4: If No State Machine Exists

If the component has stateful behavior but no defined state machine:

1. **STOP coding**
2. Define the state machine in the appropriate spec file:
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

| Pattern | Use | Avoid |
|---------|-----|-------|
| Cross-thread data | `mpsc::channel`, `broadcast` | `Arc<Mutex<T>>` for complex state |
| Progress updates | Channel with `try_send` | Shared counters with locks |
| Parallel collection | `rayon` or `ignore::WalkParallel` | Manual thread spawning |
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

### Test Pattern

```rust
#[tokio::test]
async fn test_scan_persists_files() {
    // SETUP: Real database, real temp directory
    let db = Database::open_test().await;
    let temp_dir = create_test_files(100);
    let source = Source {
        id: "test-source".to_string(),
        path: temp_dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };

    // EXECUTE: The actual critical path
    let scanner = Scanner::new(db.clone());
    let result = scanner.scan_source(&source).await.unwrap();

    // VERIFY: Observable outcomes, not internals
    assert_eq!(result.files_discovered, 100);
    let files = db.list_files_for_source(&source.id).await.unwrap();
    assert_eq!(files.len(), 100);
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

- [ ] `cargo check` passes (no errors)
- [ ] `cargo clippy` passes (no warnings in changed code)
- [ ] Critical path has test coverage
- [ ] No new `unwrap()` in production code (use `?` or proper error handling)
- [ ] No `clone()` where borrow would suffice
- [ ] Async code doesn't hold locks across await points
- [ ] Public APIs have doc comments
- [ ] State machine spec updated if states/transitions changed
- [ ] Code state enums match spec exactly

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

- `cargo check`
- `cargo clippy`
- Tests for critical path
- No locks across awaits
- State machine spec updated if changed

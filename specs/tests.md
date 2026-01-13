# Test Specification

**Status:** ACTIVE
**Version:** 1.1
**Last Updated:** 2026-01-12

---

## 1. Philosophy

Inspired by Jon Blow and Casey Muratori's pragmatic approach:

1. **Test the real thing** - No mocks. If something is slow, fix the code, don't mock it.
2. **Tests catch bugs, not satisfy coverage** - Every test should have a plausible failure scenario.
3. **Fail fast** - The most common failures should be caught in seconds, not minutes.
4. **Simple over clever** - Direct assertions over test framework abstractions.
5. **Integration over isolation** - "Does the system work?" beats "does each piece work alone?"

### 1.1 The "Would This Catch a Real Bug?" Test

Before writing a test, ask:
- What bug would this catch?
- Has this bug happened before?
- If this test fails, is it because something is actually broken?

If you can't answer these, don't write the test.

### 1.2 Anti-Patterns

| Anti-Pattern | Why It's Bad | Instead |
|--------------|--------------|---------|
| Mocking the database | Hides real SQL bugs | Use real SQLite in-memory |
| Mocking file system | Hides path handling bugs | Use temp directories |
| Snapshot tests for UI | Break on any cosmetic change | Semantic verification |
| Testing private functions | Couples tests to implementation | Test through public API |
| `sleep()` in tests | Flaky, slow | Use conditions/channels |
| Testing getters/setters | Zero value | Test behavior |

---

## 2. Test Tiers

### 2.1 Fast Suite (Default)

**Target:** <30 seconds
**Command:** `cargo test`
**Scope:** Everything that can run without network, PTY, or LLM

```
cargo test                    # Runs fast suite
```

**Includes:**
- Unit tests for pure logic
- Integration tests with in-memory SQLite
- Key handling logic (no rendering)
- Parser execution (real files, real Python)
- State machine transitions

**Excludes:**
- PTY-based TUI tests
- LLM integration tests
- Network-dependent tests
- Tests marked `#[ignore]`

### 2.2 Full Suite

**Target:** <5 minutes
**Command:** `cargo test --features=full`
**Scope:** Everything including slow tests

```
cargo test --features=full    # Runs everything
```

**Includes everything in Fast, plus:**
- PTY-based TUI interaction tests
- Real terminal rendering tests
- Multi-process integration tests

### 2.3 Performance Suite

**Target:** Enforces regression budgets
**Command:** `cargo test --features=perf`
**Scope:** Performance regression tests

```
cargo test --features=perf    # Performance tests with thresholds
```

**Tests:**
- App startup time
- View switching latency
- Database query times
- File scanning throughput

---

## 3. Critical Paths

These paths MUST work. A failure here blocks release.

### 3.1 Crown Jewels

| Path | Description | Test Coverage |
|------|-------------|---------------|
| **Startup → Home** | App launches, shows home screen | `test_app_starts_shows_home` |
| **Navigation 1-4** | Number keys switch views | `test_view_navigation` |
| **Discover → Scan** | Can scan a directory | `test_discover_scan_directory` |
| **Parser → Run** | Can execute a parser on a file | `test_parser_executes` |
| **Chat → Response** | Can send message, get response | `test_chat_round_trip` (full only) |

### 3.2 Functional Requirements

Every view must:
- [ ] Render without panic
- [ ] Accept keyboard input
- [ ] Navigate away via 0/H or Esc
- [ ] Show correct title/breadcrumb

### 3.3 Data Integrity

- [ ] Database migrations complete without error
- [ ] Parser output written atomically
- [ ] No data loss on crash (use WAL mode)

---

## 4. Performance Budgets

Hard limits that fail the build if exceeded.

### 4.1 Startup Performance

| Metric | Budget | Measured From |
|--------|--------|---------------|
| Cold start to first frame | <500ms | `main()` to first `draw()` |
| Warm start (cached venvs) | <200ms | Same |
| Database connection | <50ms | `open()` to ready |

### 4.2 Interaction Latency

| Metric | Budget | Notes |
|--------|--------|-------|
| View switch (1/2/3/4) | <16ms | Must feel instant |
| List scroll (j/k) | <8ms | 120fps target |
| Dialog open | <32ms | Acceptable slight delay |
| Refresh (r) | <100ms | DB query + render |

### 4.3 Data Operations

| Metric | Budget | Notes |
|--------|--------|-------|
| Scan 1000 files | <2s | Metadata only |
| Load 100 parsers | <500ms | From disk |
| Query job history | <100ms | Last 1000 jobs |

### 4.4 Precomputation Strategy

To meet budgets, precompute:
- Home screen stats (cache, refresh on demand)
- Parser metadata (cache on load, invalidate on file change)
- File counts per tag (incremental update)

---

## 5. Test Categories

### 5.1 Logic Tests (Fast)

Pure functions with no I/O. Should be <1ms each.

```rust
#[test]
fn test_job_status_symbols() {
    assert_eq!(JobStatus::Pending.symbol(), "○");
    assert_eq!(JobStatus::Running.symbol(), "↻");
}
```

### 5.2 State Machine Tests (Fast)

Verify state transitions without rendering.

```rust
#[test]
fn test_discover_state_transitions() {
    let mut state = DiscoverState::default();

    // Idle -> Scanning
    state.start_scan("/tmp");
    assert!(matches!(state.phase, DiscoverPhase::Scanning));

    // Scanning -> Loaded
    state.complete_scan(vec![file1, file2]);
    assert!(matches!(state.phase, DiscoverPhase::Loaded));
}
```

### 5.3 Key Handling Tests (Fast)

Test key → action mapping without rendering.

```rust
#[test]
fn test_navigation_keys() {
    let mut app = App::new(test_args());

    // From Home, '1' goes to Discover
    app.handle_key(key('1')).await;
    assert_eq!(app.mode, TuiMode::Discover);

    // From Discover, '0' goes to Home
    app.handle_key(key('0')).await;
    assert_eq!(app.mode, TuiMode::Home);
}
```

### 5.4 Database Integration Tests (Fast)

Real SQLite, in-memory.

```rust
#[test]
fn test_job_persistence() {
    let db = Database::in_memory().unwrap();

    let job_id = db.create_job("parser.py", "/input.csv").unwrap();
    let job = db.get_job(job_id).unwrap();

    assert_eq!(job.status, JobStatus::Pending);
}
```

### 5.5 Parser Execution Tests (Fast)

Real Python, real files, temp directories.

```rust
#[test]
fn test_parser_produces_output() {
    let temp = TempDir::new().unwrap();
    let parser = temp.child("parser.py");
    parser.write_str(MINIMAL_PARSER).unwrap();

    let input = temp.child("input.csv");
    input.write_str("a,b\n1,2\n").unwrap();

    let result = run_parser(&parser, &input).unwrap();
    assert!(result.output_path.exists());
    assert!(result.rows_processed > 0);
}
```

### 5.6 TUI Rendering Tests (Full)

Semantic verification of rendered output.

```rust
#[test]
#[cfg(feature = "full")]
fn test_home_screen_renders_correctly() {
    let app = App::new(test_args());
    let output = render_to_string(&app);

    // Semantic check: verify structure, not exact characters
    assert!(semantic_verify(&output, "home screen with 4 navigation cards"));
}
```

### 5.7 PTY Integration Tests (Full)

Real terminal interaction.

```rust
#[test]
#[cfg(feature = "full")]
fn test_tui_keyboard_navigation() {
    let mut pty = spawn_tui().unwrap();

    pty.send_key('1');  // Go to Discover
    pty.wait_for_text("Discover", Duration::from_secs(2)).unwrap();

    pty.send_key('0');  // Go to Home
    pty.wait_for_text("Home", Duration::from_secs(2)).unwrap();
}
```

### 5.8 Performance Regression Tests (Perf)

```rust
#[test]
#[cfg(feature = "perf")]
fn test_startup_time() {
    let start = Instant::now();
    let _app = App::new(test_args());
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(500),
        "Startup took {:?}, budget is 500ms", elapsed);
}

#[test]
#[cfg(feature = "perf")]
fn test_view_switch_latency() {
    let mut app = App::new(test_args());

    let start = Instant::now();
    app.handle_key(key('1')).await;
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(16),
        "View switch took {:?}, budget is 16ms", elapsed);
}
```

---

## 6. Failure Tracking

Track which tests fail most often to optimize test ordering.

### 6.1 Failure Log

Update this table when tests fail in CI or local development:

| Test | Failures (30d) | Last Failure | Root Cause |
|------|----------------|--------------|------------|
| `test_tui_pty_navigation` | 3 | 2026-01-10 | Timing flake |
| `test_parser_executes` | 2 | 2026-01-08 | Python not found |
| `test_chat_round_trip` | 1 | 2026-01-05 | Network timeout |

### 6.2 Flaky Test Policy

If a test fails intermittently:
1. First occurrence: Note in failure log
2. Second occurrence: Add to `#[flaky]` list, investigate
3. Third occurrence: Fix or delete. No permanently flaky tests.

### 6.3 Test Ordering

Fast suite should run tests in this order:
1. **Most-failed tests first** - Catch common breaks early
2. **Fastest tests next** - Quick wins
3. **Slowest tests last** - Only if everything else passes

---

## 7. TUI Testing with LLM Semantic Verification

### 7.1 Concept

Instead of brittle exact-match assertions, use LLM to verify semantic correctness:

```rust
fn semantic_verify(output: &str, expectation: &str) -> bool {
    let prompt = format!(
        "You are verifying TUI output. Answer only YES or NO.\n\n\
         OUTPUT:\n```\n{}\n```\n\n\
         EXPECTATION: {}\n\n\
         Does the output match the expectation?",
        output, expectation
    );

    let response = claude_api::complete(&prompt).unwrap();
    response.trim().eq_ignore_ascii_case("yes")
}
```

### 7.2 Example Expectations

| Expectation | What It Catches |
|-------------|-----------------|
| "home screen with 4 navigation cards" | Missing cards, wrong count |
| "discover view showing file list" | Wrong view rendered |
| "dialog asking for tag name" | Dialog not appearing |
| "error message visible" | Errors swallowed silently |
| "job status shows 3 running" | Wrong counts displayed |

### 7.3 When to Use

- **Use** for complex UI states that are hard to assert programmatically
- **Don't use** for simple logic (use direct assertions)
- **Don't use** in fast suite (too slow)

### 7.4 Fallback Heuristics

For fast suite, use simple heuristics instead of LLM:

```rust
fn quick_verify_home_screen(output: &str) -> bool {
    output.contains("Discover") &&
    output.contains("Parser") &&
    output.contains("Jobs") &&
    output.contains("Sources") &&
    output.contains("[1]") &&
    output.contains("[2]")
}
```

---

## 8. Test Fixtures & Compression

### 8.1 Shared Fixtures

```rust
// tests/fixtures/mod.rs

pub fn test_args() -> TuiArgs {
    TuiArgs {
        db_path: None,  // Uses in-memory
        no_llm: true,
        model: "test".into(),
    }
}

pub fn test_db() -> Database {
    Database::in_memory().unwrap()
}

pub fn test_files() -> Vec<FileInfo> {
    vec![
        FileInfo { path: "data.csv".into(), size: 1024, .. },
        FileInfo { path: "orders.json".into(), size: 2048, .. },
    ]
}

pub const MINIMAL_PARSER: &str = r#"
class Parser:
    name = 'test'
    version = '1.0.0'
    topics = ['test']

    def parse(self, ctx):
        yield ('output', [{'a': 1}])
"#;
```

### 8.2 Test Helpers

```rust
// tests/helpers/mod.rs

pub fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

pub fn key_ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

pub async fn press_keys(app: &mut App, keys: &str) {
    for c in keys.chars() {
        app.handle_key(key(c)).await;
    }
}
```

### 8.3 Parameterized Tests

```rust
#[test_case('1', TuiMode::Discover ; "1 goes to Discover")]
#[test_case('2', TuiMode::ParserBench ; "2 goes to Parser Bench")]
#[test_case('3', TuiMode::Jobs ; "3 goes to Jobs")]
#[test_case('4', TuiMode::Inspect ; "4 goes to Sources")]
fn test_navigation_key(key: char, expected_mode: TuiMode) {
    let mut app = App::new(test_args());
    block_on(app.handle_key(key(key)));
    assert_eq!(app.mode, expected_mode);
}
```

---

## 9. CI Configuration

### 9.1 Fast Suite (Every Push)

```yaml
test-fast:
  runs-on: ubuntu-latest
  timeout-minutes: 5
  steps:
    - uses: actions/checkout@v4
    - run: cargo test
```

### 9.2 Full Suite (PR Merge)

```yaml
test-full:
  runs-on: ubuntu-latest
  timeout-minutes: 15
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --features=full
```

### 9.3 Performance Suite (Nightly)

```yaml
test-perf:
  runs-on: ubuntu-latest
  schedule:
    - cron: '0 0 * * *'
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --features=perf
```

---

## 10. Implementation Checklist

### Phase 1: Fast Suite Optimization ✓
- [x] Add `#[cfg(feature = "full")]` to PTY tests
- [x] Add `#[cfg(feature = "full")]` to LLM tests
- [x] Measure current fast suite time (~30s achieved)
- [x] Identify slowest tests, optimize or move to full

**Gated Files/Modules:**
- `tui_pty_e2e.rs` - entire file (~180s)
- `tui_chat_e2e.rs` - entire file (~60s)
- `crown_jewel_e2e.rs` - entire file (~60s)
- `ultrathink_e2e.rs` - `binary_tui`, `tui_state` modules + `cargo run` tests
- `critical_paths.rs` - `binary` module

### Phase 2: Performance Tests
- [ ] Add startup time test
- [ ] Add view switch latency test
- [ ] Add database query time tests
- [ ] Set up perf regression CI

### Phase 3: Semantic Verification
- [ ] Implement `semantic_verify()` helper
- [ ] Add heuristic fallbacks for fast suite
- [ ] Write TUI semantic tests for each view

### Phase 4: Failure Tracking
- [ ] Set up failure logging in CI
- [ ] Create dashboard/report
- [ ] Implement test ordering by failure frequency

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Initial specification |
| 2026-01-12 | 1.1 | Phase 1 complete: fast suite <30s, slow tests gated behind `full` feature |

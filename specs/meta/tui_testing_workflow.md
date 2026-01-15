# TUI Testing Workflow

> **Status**: Active
> **Version**: 1.2
> **Category**: Analysis workflow (per workflow_manager.md Section 3.3.1)
> **Last Updated**: 2026-01-14
> **Related**: `specs/views/discover.md`, `CLAUDE.md` (TUI Development section)

## Overview

This document formalizes the process for testing Terminal User Interfaces (TUI) using tmux. Unlike unit tests or PTY-based testing, tmux provides real terminal rendering that catches issues invisible to programmatic tests.

**Why tmux over PTY tests?**

| Aspect | PTY Tests | tmux Testing |
|--------|-----------|--------------|
| Visibility | Pattern matching on byte streams | Clean rendered text, exactly what user sees |
| Debugging | Pass/fail with no context | Visual inspection at each step |
| Escape sequences | Must parse `\x1b[?25h` etc | `capture-pane -p` strips them |
| State inspection | Implicit in assertions | Explicit screen capture |
| Edge cases | Often missed | Visually apparent |
| Layout issues | Hard to detect | Immediately visible |

## Core Principles

### 1. Test What Users See
The goal is to verify the actual rendered output, not internal state. If a user would see a bug, the test should catch it.

### 2. Capture After Every Action
Never batch keystrokes then capture. The bug may be in transition states:
```bash
# WRONG - batched
tmux send-keys -t tui "abc" && tmux send-keys -t tui Enter && tmux capture-pane -t tui -p

# RIGHT - incremental
tmux send-keys -t tui "a" && sleep 0.2 && tmux capture-pane -t tui -p
tmux send-keys -t tui "b" && sleep 0.2 && tmux capture-pane -t tui -p
# ... etc
```

### 3. Fresh Sessions for Verification
After fixing a bug, always restart the tmux session. Stale state masks regressions:
```bash
tmux kill-session -t tui 2>/dev/null
tmux new-session -d -s tui -x 120 -y 40 "./target/release/casparian tui"
```

### 4. Document Bugs Immediately
Use TodoWrite to track issues as they're discovered. Don't rely on memory.

---

## Success Criteria

A test session is considered **successful** when:
1. All items in the Testing Checklist are marked complete
2. No new bugs discovered, OR all discovered bugs are documented
3. Smoke tests pass with exit code 0
4. No regressions in previously fixed bugs

A **smoke test** passes when:
- All grep assertions match expected strings
- Exit code is 0
- No timeout (default 30s per test)

A **regression** is detected when:
- A previously passing test now fails
- A previously fixed bug reappears

---

## Test Fixtures

### Initial State Requirements

For consistent testing, start with a clean database state:

```bash
# Reset to clean state (WARNING: deletes all data)
rm -f ~/.casparian_flow/casparian_flow.sqlite3
```

### Sample Data Setup

For testing Discover mode with data:

```bash
# Create test directory with sample files
mkdir -p /tmp/tui-test-data
echo "col1,col2" > /tmp/tui-test-data/test.csv
echo '{"key": "value"}' > /tmp/tui-test-data/test.json

# Add source and scan (requires binary built)
./target/release/casparian scan /tmp/tui-test-data --tag test
```

After setup, the TUI should show files in the test source.

---

## Testing Phases

### Phase 1: Session Setup

```bash
# Kill any existing session
tmux kill-session -t tui 2>/dev/null

# Wait for cleanup
sleep 0.2

# Start fresh session with consistent dimensions
tmux new-session -d -s tui -x 120 -y 40 "./target/release/casparian tui"

# Wait for app to initialize
sleep 1

# Verify app started
tmux capture-pane -t tui -p
```

**Key parameters:**
- `-x 120 -y 40`: Consistent terminal size (important for layout testing)
- `-d`: Detached mode (runs in background)
- `-s tui`: Named session for easy reference

### Phase 2: Systematic State Testing

Test each view state systematically. For Discover mode:

```
Home Hub
  └─[1]→ Discover (Files view)
           ├─[1]→ Sources dropdown
           ├─[2]→ Tags dropdown
           ├─[n]→ Rule Creation dialog
           ├─[R]→ Rules Manager
           ├─[M]→ Sources Manager
           ├─[/]→ Filter mode
           └─[g]→ Glob Explorer toggle
```

**Testing checklist for each state:**
- [ ] Entry: Does correct UI render?
- [ ] Focus: Is the right element focused?
- [ ] Keybindings: Do documented keys work? (See `specs/views/discover.md` Section 6 for keybinding tables)
- [ ] Navigation: Can user move between elements?
- [ ] Exit: Does Esc/back work correctly?
- [ ] Data: Is content displayed correctly?

### Phase 3: Dialog Testing

For each dialog (Rule Creation, Sources Manager, etc.):

1. **Open dialog**
   ```bash
   tmux send-keys -t tui "n" && sleep 0.3 && tmux capture-pane -t tui -p
   ```

2. **Test each field**
   ```bash
   # Type in field
   tmux send-keys -t tui "test_input" && sleep 0.3 && tmux capture-pane -t tui -p

   # Navigate to next field
   tmux send-keys -t tui Tab && sleep 0.2 && tmux capture-pane -t tui -p
   ```

3. **Test field interactions**
   - Character input (including special chars)
   - Backspace/delete
   - Cursor movement (if supported)
   - Field validation feedback

4. **Test actions**
   ```bash
   # Test submit
   tmux send-keys -t tui Enter && sleep 0.3 && tmux capture-pane -t tui -p

   # Test cancel
   tmux send-keys -t tui Escape && sleep 0.3 && tmux capture-pane -t tui -p
   ```

5. **Verify data persistence**
   ```bash
   # After creating a rule, verify in database
   sqlite3 ~/.casparian_flow/casparian_flow.sqlite3 \
     "SELECT * FROM scout_tagging_rules ORDER BY created_at DESC LIMIT 1"
   ```

   Expected: New rule visible with correct pattern and tag.

### Phase 4: Edge Case Testing

**Empty states:**
- No data loaded
- Empty search results
- No sources configured

**Boundary conditions:**
- Very long text input
- Many items in lists (scrolling)
- Special characters in input
- Unicode content

**Error states:**
- Invalid input
- Network/DB errors
- Permission issues

### Phase 5: Visual Verification

Check for:
- [ ] Layout alignment (borders line up)
- [ ] Text truncation (ellipsis where expected)
- [ ] Color/style correctness
- [ ] Focus indicators visible
- [ ] No rendering artifacts
- [ ] Footer matches current context
- [ ] Title reflects current state

### Phase 6: Async Operation Testing

TUI operations that involve background work need special handling.

**Glob Explorer Scanning:**
```bash
# Start glob explorer
tmux send-keys -t tui "g" && sleep 0.3 && tmux capture-pane -t tui -p

# Enter a path with many files
tmux send-keys -t tui -l "/**/*.rs" && sleep 0.3 && tmux capture-pane -t tui -p

# Wait for scan to complete (watch for loading indicator to disappear)
for i in {1..10}; do
    OUTPUT=$(tmux capture-pane -t tui -p)
    if ! echo "$OUTPUT" | grep -q "Loading\|Scanning"; then
        break
    fi
    sleep 0.5
done
tmux capture-pane -t tui -p  # Final state
```

**Loading State Checklist:**
- [ ] Loading indicator visible during async operation
- [ ] UI remains responsive (can cancel with Escape)
- [ ] Completion updates UI correctly
- [ ] Error states display appropriately

### Phase 7: Performance Validation

For performance-sensitive test scenarios, capture profiler data to detect regressions.

**Prerequisites:**
- Build with profiling feature: `cargo build -p casparian --release --features profiling`

**Performance Testing Flow:**

```bash
# 1. Cleanup stale profiler data
rm -f /tmp/casparian_profile_dump /tmp/casparian_profile_data.txt

# 2. Run test scenario (example: Discover mode navigation)
tmux send-keys -t tui -l "1" && sleep 0.5  # Navigate to Discover
for i in {1..10}; do
    tmux send-keys -t tui Down && sleep 0.1
done

# 3. Trigger profiler dump with polling
touch /tmp/casparian_profile_dump
for i in {1..20}; do
    sleep 0.25
    if [ -f /tmp/casparian_profile_data.txt ]; then
        break
    fi
done

# 4. Validate against thresholds
MAX_MS=$(grep "max_ms" /tmp/casparian_profile_data.txt | cut -d= -f2)
MAX_INT=${MAX_MS%.*}

if [ "$MAX_INT" -gt 250 ]; then
    echo "FAIL: Frame budget exceeded (${MAX_MS}ms > 250ms)"
    grep "^zone\." /tmp/casparian_profile_data.txt  # Show zone breakdown
    exit 1
fi
echo "PASS: Performance within budget (max_ms=$MAX_MS)"

# 5. Save performance data to artifacts (optional)
cp /tmp/casparian_profile_data.txt "screenshots/$(date +%Y%m%d_%H%M%S)_perf.txt"
```

**Performance Thresholds:**

| Threshold | Value | Action |
|-----------|-------|--------|
| OK | < 150ms | Silent pass |
| WARNING | 150-250ms | Log warning, don't fail |
| FAIL | > 250ms | Fail test |
| CRITICAL | > 500ms | Fail test, dump zone breakdown |

**Performance Checklist:**
- [ ] Built with `--features profiling`
- [ ] No frames exceeded budget (max_ms < 250)
- [ ] Average frame time acceptable (avg_ms < 150 for typical operations)
- [ ] No single zone dominates unexpectedly (check zone breakdown)
- [ ] Performance data captured to artifacts on failure

**Detection:**
If profiling is not enabled, the dump file won't be created. Tests should gracefully skip:
```bash
if [ ! -f /tmp/casparian_profile_data.txt ]; then
    echo "SKIP: Profiling not enabled in build"
    exit 0
fi
```

> **Related:** `specs/profiler.md` Section 8 (Testing Integration)

---

## Bug Documentation Pattern

When a bug is found, document it immediately:

```
TodoWrite([
  {
    "content": "Bug: [Component] - [Brief description]",
    "status": "pending",
    "activeForm": "Fixing [component] [issue type]"
  }
])
```

**Bug categories:**

| Category | Example |
|----------|---------|
| Key conflict | 't' triggers action when should type character |
| State sync | Title shows old state after transition |
| Layout | Double footer, misaligned borders |
| Data | Preview shows wrong count, stale data |
| Navigation | Can't exit dialog, focus stuck |
| Rendering | Artifacts, missing elements |

---

## Fix Verification Process

### Step 1: Make the fix
Edit the code to address the bug.

### Step 2: Rebuild
```bash
cargo build -p casparian --release 2>&1 | tail -10
```

### Step 3: Fresh session test
```bash
# Kill old session
tmux kill-session -t tui 2>/dev/null
sleep 0.2

# Start fresh
tmux new-session -d -s tui -x 120 -y 40 "./target/release/casparian tui"
sleep 1

# Navigate to the bug location
tmux send-keys -t tui "1" && sleep 0.5  # Go to Discover
# ... reproduce the bug scenario

# Capture and verify
tmux capture-pane -t tui -p
```

### Step 4: Regression check
Re-run the full test sequence to ensure fix didn't break other things.

### Screenshot Management

For regression tracking, save captures with descriptive names:

```bash
# Create screenshots directory
mkdir -p screenshots

# Save timestamped capture
tmux capture-pane -t tui -p > "screenshots/$(date +%Y%m%d_%H%M%S)_discover_main.txt"

# Save with bug reference
tmux capture-pane -t tui -p > "screenshots/BUG-001_before_fix.txt"

# Compare captures
diff screenshots/baseline_home.txt <(tmux capture-pane -t tui -p)
```

**Naming convention:** `{date}_{context}_{state}.txt` or `{BUG-ID}_{description}.txt`

---

## Common Bug Patterns

### 1. Key Conflicts
**Symptom:** Pressing a key triggers wrong action
**Cause:** Key handler matches before text input handler
**Fix:** Add guard conditions to restrict when key triggers action

```rust
// WRONG - always triggers
KeyCode::Char('t') => { trigger_action(); }

// RIGHT - only when in specific focus
KeyCode::Char('t') if self.focus == Focus::Options => { trigger_action(); }
```

### 2. Stale Data Display
**Symptom:** UI shows old data after action
**Cause:** State update happens but UI reads from different source
**Fix:** Ensure all data sources are synced, or use single source of truth

### 3. Missing Focus Indicators
**Symptom:** Can't tell which element is focused
**Cause:** Focus state not reflected in render
**Fix:** Add visual distinction (border color, cursor, highlight)

### 4. Double Elements
**Symptom:** Footer/header appears twice
**Cause:** Both dialog and parent view render same element
**Fix:** Conditionally hide parent element when dialog is active

### 5. State Transition Bugs
**Symptom:** Wrong view after action
**Cause:** State machine transition incorrect
**Fix:** Trace state transitions, verify each path

---

## Helper Commands Reference

### Session Management
```bash
# Start session
tmux new-session -d -s tui -x 120 -y 40 "./target/release/casparian tui"

# Kill session
tmux kill-session -t tui

# List sessions
tmux list-sessions

# Attach to session (for manual inspection)
tmux attach -t tui
```

### Sending Input
```bash
# Single character
tmux send-keys -t tui "a"

# String
tmux send-keys -t tui "hello world"

# Special keys
tmux send-keys -t tui Enter
tmux send-keys -t tui Escape
tmux send-keys -t tui Tab
tmux send-keys -t tui Down
tmux send-keys -t tui Up

# Ctrl combinations
tmux send-keys -t tui C-c

# Literal (prevents F1 interpretation of "1")
tmux send-keys -t tui -l "1"
```

### Capturing Output
```bash
# Current visible content
tmux capture-pane -t tui -p

# With scrollback
tmux capture-pane -t tui -p -S -100

# To file
tmux capture-pane -t tui -p > screenshot.txt
```

### Timing
```bash
# Always add delays between send and capture
tmux send-keys -t tui "x" && sleep 0.3 && tmux capture-pane -t tui -p

# Longer delays for async operations
tmux send-keys -t tui Enter && sleep 1 && tmux capture-pane -t tui -p
```

---

## Testing Checklist Template

Copy and use for each testing session:

```markdown
## TUI Test Session - [Date]

### Setup
- [ ] Built release binary
- [ ] Started fresh tmux session
- [ ] App initialized correctly

### Home Hub
- [ ] All 4 quadrants render
- [ ] Number keys navigate correctly
- [ ] Stats display correctly

### Discover Mode
- [ ] Files panel renders
- [ ] Sources dropdown works
- [ ] Tags dropdown works
- [ ] Filter mode works
- [ ] Rule Creation dialog works
- [ ] Rules Manager works
- [ ] Sources Manager works

### Dialog Testing: [Dialog Name]
- [ ] Opens correctly
- [ ] All fields accessible
- [ ] Tab navigation works
- [ ] Input accepted
- [ ] Validation feedback
- [ ] Submit works
- [ ] Cancel works
- [ ] No visual artifacts

### Performance (if profiling enabled)
- [ ] Built with `--features profiling`
- [ ] max_ms < 250 (frame budget)
- [ ] avg_ms < 150 (typical operations)
- [ ] Zone breakdown reasonable

### Bugs Found
1. [Description] - Status: [pending/fixed]
2. ...

### Regressions Checked
- [ ] Previous bug fixes still work
- [ ] Core navigation unaffected
```

---

## Integration with CI

While tmux testing is primarily manual/interactive, the helper script provides automation:

```bash
# Use the helper script (scripts/tui-test-workflow.sh)
./scripts/tui-test-workflow.sh smoke          # Run all smoke tests
./scripts/tui-test-workflow.sh test-home      # Test home hub only
./scripts/tui-test-workflow.sh test-discover  # Test discover mode only
./scripts/tui-test-workflow.sh test-rule-dialog  # Test rule creation dialog
```

The smoke tests verify:
1. Home Hub renders with all quadrants
2. Discover mode entry works
3. Rule Creation dialog opens and accepts input

For custom CI integration, individual assertions can be scripted:

```bash
#!/bin/bash
# Example: Custom CI test
set -e

tmux kill-session -t test 2>/dev/null || true
tmux new-session -d -s test -x 120 -y 40 "./target/release/casparian tui"
sleep 1

OUTPUT=$(tmux capture-pane -t test -p)
if ! echo "$OUTPUT" | grep -q "Discover"; then
    echo "FAIL: Home Hub missing Discover"
    exit 1
fi

tmux kill-session -t test
echo "PASS"
```

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.2 | Added Phase 7: Performance Validation for profiler integration, performance checklist in template, thresholds table, polling-based dump trigger pattern |
| 2025-01-14 | 1.1 | Added: Success Criteria, Test Fixtures, Async Testing (Phase 6), Database Verification, Screenshot Management, fixed CI script references |
| 2025-01-14 | 1.0 | Initial workflow based on Rule Creation dialog testing |

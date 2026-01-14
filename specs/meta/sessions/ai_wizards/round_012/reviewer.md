# Reviewer Assessment: GAP-TUI-001 Resolution

**Engineer Proposal:** `round_012/engineer.md`
**Gap:** GAP-TUI-001 - $EDITOR subprocess handling for TUI (terminal handoff, crash recovery, fallbacks)

---

## Overall Assessment: APPROVED

The Engineer's proposal is architecturally sound and follows the established pattern for terminal handoff in ratatui/crossterm applications. The approach of full terminal suspension is correct for supporting both terminal-based editors (vim, nano) and GUI editors (VS Code, Sublime). The fallback chain is reasonable and matches Unix conventions.

---

## Issues Identified

### ISSUE-R12-001: Timeout Check Occurs After Process Exit (Low)

**Location:** Section 2.1, `spawn_editor` method, lines 172-179

**Problem:** The timeout check occurs after `cmd.spawn()?.wait()` returns, which means the timeout is never actually enforced during execution - it only logs after the fact. The `wait()` call blocks indefinitely.

```rust
let start = std::time::Instant::now();
let status = cmd.spawn()?.wait()?;  // Blocks forever
let elapsed = start.elapsed();

// Check timeout - but process already exited!
if elapsed >= self.editor_config.timeout {
    return Err(EditorError::Timeout { ... });
}
```

**Recommendation:** The timeout is informational only (user left editor open a long time), which is acceptable. However, document this clearly:

```rust
// NOTE: Timeout is advisory only. We cannot forcibly terminate
// the user's editor - that would cause data loss. This timeout
// is logged for diagnostics but does not interrupt editing.
```

Alternatively, if actual enforcement is desired, spawn a separate monitoring thread that sends SIGTERM after timeout - but this is likely undesirable UX.

**Severity:** Low - Current behavior is safe (never kills user's work), just mislabeled.

---

### ISSUE-R12-002: Missing Cursor Visibility Handling (Medium)

**Location:** Section 2.1, `run` method

**Problem:** The terminal handoff code handles raw mode and alternate screen, but does not explicitly manage cursor visibility. Many TUI apps hide the cursor, and this state may not be correctly restored.

Current code:
```rust
disable_raw_mode()?;
execute!(std::io::stdout(), LeaveAlternateScreen)?;
// ... spawn editor ...
execute!(std::io::stdout(), EnterAlternateScreen)?;
enable_raw_mode()?;
```

Missing: `crossterm::cursor::Show` before handing off, `crossterm::cursor::Hide` after.

**Recommendation:** Add cursor management:
```rust
use crossterm::cursor::{Show, Hide};

disable_raw_mode()?;
execute!(std::io::stdout(), LeaveAlternateScreen, Show)?;
// ... spawn editor ...
execute!(std::io::stdout(), EnterAlternateScreen, Hide)?;
enable_raw_mode()?;
```

---

### ISSUE-R12-003: Race Window in Terminal Restoration (Medium)

**Location:** Section 2.1, `run` method

**Problem:** If `spawn_editor()` panics or returns an error, the terminal restoration code in step 3 may not execute, leaving the terminal in a corrupted state.

**Example scenario:**
```rust
pub fn run(&self) -> Result<EditorResult, EditorError> {
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    let result = self.spawn_editor();  // If this panics...

    // ... this never runs, terminal is broken
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;

    result
}
```

**Recommendation:** Use a guard pattern to ensure restoration:

```rust
struct TerminalGuard;

impl TerminalGuard {
    fn suspend() -> io::Result<Self> {
        disable_raw_mode()?;
        execute!(std::io::stdout(), LeaveAlternateScreen, Show)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), EnterAlternateScreen, Hide);
        let _ = enable_raw_mode();
    }
}

pub fn run(&self) -> Result<EditorResult, EditorError> {
    let _guard = TerminalGuard::suspend()?;
    self.spawn_editor()
    // Guard restores terminal even on panic/early return
}
```

---

### ISSUE-R12-004: open -t on macOS Fails for Non-Text Files (Low)

**Location:** Section 1.2, macOS fallback

**Problem:** The `open -t -W` command on macOS opens files in the default **plain text** editor (TextEdit in plain text mode). If the file has a recognized extension like `.yaml` or `.py`, the system may launch a different application that does not honor `-W`.

**Example:**
```bash
$ open -t -W draft.yaml  # May open in Xcode or another app, won't wait
```

**Recommendation:** Use `open -e -W` instead, which explicitly opens in TextEdit and always waits:
```rust
#[cfg(target_os = "macos")]
{
    return Some(EditorConfig {
        command: "open".to_string(),
        args: vec!["-e".to_string(), "-W".to_string()],  // -e not -t
        timeout: Duration::from_secs(3600),
    });
}
```

Or document that users should set `$EDITOR` for reliable behavior.

---

### ISSUE-R12-005: ensure_wait_flag Contains Check is Fragile (Low)

**Location:** Section 4.1, `ensure_wait_flag`

**Problem:** The check `config.command.contains(editor_name)` is overly broad and could match unintended strings:

```rust
if config.command.contains("code") {  // Matches "vscode", "barcode", "encode"
```

**Recommendation:** Match on the basename of the command:

```rust
fn ensure_wait_flag(config: &mut EditorConfig) {
    let basename = std::path::Path::new(&config.command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&config.command);

    let wait_flags: &[(&str, &str)] = &[
        ("code", "--wait"),
        ("subl", "--wait"),
        ("atom", "--wait"),
        ("mate", "-w"),
        ("zed", "--wait"),
    ];

    for (editor_name, flag) in wait_flags {
        if basename == *editor_name && !config.args.contains(&flag.to_string()) {
            config.args.push(flag.to_string());
        }
    }
}
```

---

### ISSUE-R12-006: temp_path Argument Position for GUI Editors (Low)

**Location:** Section 2.1, `spawn_editor`

**Problem:** The file path is always added as the last argument. For `open -t -W`, the path should come last, but for some editors the wait flag may need to come after the path.

Current:
```rust
for arg in &self.editor_config.args {
    cmd.arg(arg);
}
cmd.arg(&self.temp_path);  // Path always last
```

**Example issue:** `zed --wait path.txt` works, but what if an editor needs `editor path.txt --wait`?

**Recommendation:** This is likely fine for all listed editors, but document the assumption:
```rust
// NOTE: File path is always added as final argument.
// All supported editors (vim, nano, code, subl, zed) accept this ordering.
```

---

### ISSUE-R12-007: No SIGINT Handler During Editor Execution (Medium)

**Location:** Section 2 generally

**Problem:** If the user presses Ctrl+C while the external editor is running, the signal may be delivered to the TUI process (which is suspended but still exists), potentially causing unclean shutdown without terminal restoration.

**Recommendation:** Document that editors run in the same process group, so Ctrl+C goes to the editor first. If this becomes an issue, consider:

```rust
// Before spawn
#[cfg(unix)]
unsafe {
    // Create new process group for editor
    libc::setpgid(0, 0);
}
```

However, this may be over-engineering for v1.

---

### ISSUE-R12-008: Draft File Cleanup on Success Should Be Deferred (Low)

**Location:** Section 3.2, `cleanup_on_success`

**Problem:** The cleanup function deletes the temp file immediately after success. If validation subsequently fails in VALIDATING state, the user has lost their edits.

**Recommendation:** Keep temp file until APPROVED state, not just editor success:

```rust
// In wizard state machine, not EditorSession:
match self.state {
    PathfinderState::Approved(_) => {
        self.cleanup_temp_files();
    }
    _ => {
        // Keep temp file for potential recovery
    }
}
```

---

## Positive Observations

1. **Terminal handoff approach is correct** - LeaveAlternateScreen + disable_raw_mode is the standard pattern and matches existing TUI code in `crates/casparian/src/cli/tui/mod.rs`
2. **Fallback chain follows Unix conventions** - $VISUAL > $EDITOR > platform-specific matches git, less, and other tools
3. **Error types are comprehensive** - Timeout, NonZeroExit, SpawnFailed, Killed cover the important cases
4. **Temp file preservation is thoughtful** - Keeping drafts on error enables manual recovery
5. **GUI editor wait flag auto-detection** - Practical solution for common VS Code/Sublime frustration
6. **State machine integration is clean** - WizardAction::RunEditor pattern keeps TUI loop simple

---

## Required Changes for Approval

| Issue | Severity | Required? |
|-------|----------|-----------|
| ISSUE-R12-001 | Low | No - document as advisory timeout |
| ISSUE-R12-002 | Medium | Yes - add cursor show/hide handling |
| ISSUE-R12-003 | Medium | Yes - use guard pattern for restoration |
| ISSUE-R12-004 | Low | No - document limitation |
| ISSUE-R12-005 | Low | Yes - easy fix, prevents false positives |
| ISSUE-R12-006 | Low | No - document assumption |
| ISSUE-R12-007 | Medium | No - document for v1, revisit if issues arise |
| ISSUE-R12-008 | Low | Yes - defer cleanup to APPROVED state |

---

## Summary

The proposal correctly identifies full terminal suspension as the only viable approach for supporting both terminal and GUI editors. The crossterm-based implementation matches the existing TUI codebase and follows established patterns.

**No blocking issues.** The identified issues are all improvements rather than correctness bugs:
- ISSUE-R12-002 and ISSUE-R12-003 are robustness improvements
- ISSUE-R12-005 and ISSUE-R12-008 are edge case fixes

The proposal can proceed to implementation with the recommended changes incorporated.

---

**Reviewer:** Spec Refinement Workflow
**Date:** 2026-01-13
**Status:** APPROVED (with minor improvements recommended)

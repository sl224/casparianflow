# Engineer Resolution: GAP-TUI-001

## $EDITOR Subprocess Handling for TUI

**Gap:** The wizard states reference opening files in $EDITOR (e.g., EDITING state in Pathfinder), but the spec doesn't specify:
1. How to detect when $EDITOR closes
2. What happens if $EDITOR crashes
3. TUI thread management during editor session
4. Fallback if $EDITOR is not set

**Confidence:** HIGH

---

## 1. Editor Resolution Algorithm

### 1.1 Environment Variable Priority

```rust
/// Resolve which editor command to use
pub fn resolve_editor() -> Option<EditorConfig> {
    // Priority order (matches git, less, and other Unix tools)
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()?;

    if editor.is_empty() {
        return None;
    }

    // Parse editor command (may include args, e.g., "code --wait")
    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (command, args) = parts.split_first()?;

    Some(EditorConfig {
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        timeout: Duration::from_secs(3600), // 1 hour default
    })
}

pub struct EditorConfig {
    pub command: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}
```

### 1.2 Fallback Behavior

When `$VISUAL` and `$EDITOR` are both unset:

| Platform | Fallback Order | Rationale |
|----------|----------------|-----------|
| macOS | `open -t -W`, `nano`, `vi` | `open -t -W` uses default text editor, waits |
| Linux | `sensible-editor`, `nano`, `vi` | Debian/Ubuntu standard, then ubiquitous |
| Windows | `notepad.exe` | Always available |

```rust
fn platform_fallback() -> Option<EditorConfig> {
    #[cfg(target_os = "macos")]
    {
        // Try `open -t -W` (opens in default text editor, waits for close)
        if which::which("open").is_ok() {
            return Some(EditorConfig {
                command: "open".to_string(),
                args: vec!["-t".to_string(), "-W".to_string()],
                timeout: Duration::from_secs(3600),
            });
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Debian/Ubuntu's sensible-editor
        if which::which("sensible-editor").is_ok() {
            return Some(EditorConfig {
                command: "sensible-editor".to_string(),
                args: vec![],
                timeout: Duration::from_secs(3600),
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        return Some(EditorConfig {
            command: "notepad.exe".to_string(),
            args: vec![],
            timeout: Duration::from_secs(3600),
        });
    }

    // Universal fallbacks
    for editor in ["nano", "vi"] {
        if which::which(editor).is_ok() {
            return Some(EditorConfig {
                command: editor.to_string(),
                args: vec![],
                timeout: Duration::from_secs(3600),
            });
        }
    }

    None
}
```

---

## 2. TUI Suspension Model

### 2.1 Approach: Full Terminal Handoff

The TUI must **fully suspend** during editor execution because:
1. Terminal-based editors (vim, nano) need raw mode control
2. GUI editors with `--wait` (VS Code, Sublime) block the shell
3. Partial TUI rendering would corrupt terminal state

**Implementation Pattern:**

```rust
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, LeaveAlternateScreen, EnterAlternateScreen},
};

pub struct EditorSession {
    temp_path: PathBuf,
    original_content: String,
    editor_config: EditorConfig,
}

impl EditorSession {
    /// Open editor and wait for it to close.
    /// Returns Ok(modified_content) or Err if editor failed/cancelled.
    pub fn run(&self) -> Result<EditorResult, EditorError> {
        // 1. Leave TUI alternate screen, restore normal terminal
        disable_raw_mode()?;
        execute!(std::io::stdout(), LeaveAlternateScreen)?;

        // 2. Spawn editor process
        let result = self.spawn_editor();

        // 3. Restore TUI alternate screen
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        enable_raw_mode()?;

        // 4. Force full redraw (TUI state unchanged, just screen)
        // This is handled by the TUI event loop on next tick

        result
    }

    fn spawn_editor(&self) -> Result<EditorResult, EditorError> {
        let mut cmd = std::process::Command::new(&self.editor_config.command);

        // Add configured args
        for arg in &self.editor_config.args {
            cmd.arg(arg);
        }

        // Add file path as final argument
        cmd.arg(&self.temp_path);

        // Inherit stdio for terminal editors
        cmd.stdin(std::process::Stdio::inherit())
           .stdout(std::process::Stdio::inherit())
           .stderr(std::process::Stdio::inherit());

        // Spawn and wait
        let start = std::time::Instant::now();
        let status = cmd.spawn()?.wait()?;
        let elapsed = start.elapsed();

        // Check timeout
        if elapsed >= self.editor_config.timeout {
            return Err(EditorError::Timeout {
                duration: elapsed,
                limit: self.editor_config.timeout,
            });
        }

        // Check exit status
        if !status.success() {
            return Err(EditorError::NonZeroExit {
                code: status.code(),
            });
        }

        // Read modified content
        let new_content = std::fs::read_to_string(&self.temp_path)?;

        Ok(EditorResult {
            content: new_content.clone(),
            modified: new_content != self.original_content,
            elapsed,
        })
    }
}

pub struct EditorResult {
    pub content: String,
    pub modified: bool,
    pub elapsed: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("Editor timed out after {duration:?} (limit: {limit:?})")]
    Timeout { duration: Duration, limit: Duration },

    #[error("Editor exited with code {code:?}")]
    NonZeroExit { code: Option<i32> },

    #[error("Failed to spawn editor: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("No editor available. Set $EDITOR or $VISUAL environment variable")]
    NoEditorAvailable,

    #[error("Editor was killed by signal")]
    Killed,
}
```

### 2.2 State Machine Integration

The EDITING state in wizard state machines should be a **blocking operation**, not a concurrent state:

```rust
pub enum PathfinderState {
    // ... other states ...

    /// External editor active - TUI is suspended.
    /// This state is transient: we enter it, block on editor, then transition.
    Editing(EditingData),
}

impl PathfinderWizard {
    pub fn handle_edit_key(&mut self) -> WizardAction {
        let result_data = match &self.state {
            PathfinderState::YamlResult(data) |
            PathfinderState::PythonResult(data) => data.clone(),
            _ => return WizardAction::None,
        };

        // Check editor availability before entering EDITING state
        let editor_config = resolve_editor()
            .or_else(|| platform_fallback())
            .ok_or(EditorError::NoEditorAvailable)?;

        // Create temp file with current content
        let temp_path = self.create_temp_file(&result_data.generated_content)?;

        // Transition to EDITING state
        self.state = PathfinderState::Editing(EditingData {
            temp_file_path: temp_path.clone(),
            original_content: result_data.generated_content.clone(),
            previous_state: Box::new(self.state.clone()),
        });

        // Signal TUI to suspend and run editor
        WizardAction::RunEditor(EditorSession {
            temp_path,
            original_content: result_data.generated_content,
            editor_config,
        })
    }

    /// Called after editor returns
    pub fn handle_editor_result(&mut self, result: Result<EditorResult, EditorError>) {
        match result {
            Ok(EditorResult { content, modified: true, .. }) => {
                // Content changed - transition to VALIDATING (Parser Lab)
                // or REGENERATING (Pathfinder)
                self.state = PathfinderState::Regenerating(RegeneratingData {
                    hints: vec![],
                    sample_paths: self.sample_paths.clone(),
                    manual_content: Some(content),
                });
            }
            Ok(EditorResult { modified: false, .. }) => {
                // Content unchanged - return to previous result state
                if let PathfinderState::Editing(data) = &self.state {
                    self.state = *data.previous_state.clone();
                }
            }
            Err(e) => {
                // Editor failed - show error, return to previous state
                self.show_error(format!("Editor error: {}", e));
                if let PathfinderState::Editing(data) = &self.state {
                    self.state = *data.previous_state.clone();
                }
            }
        }
    }
}
```

### 2.3 TUI Event Loop Integration

The TUI main loop must handle the `WizardAction::RunEditor` action:

```rust
// In TUI event loop
loop {
    // ... render frame ...

    // Handle input
    if let Some(action) = app.handle_input(event)? {
        match action {
            AppAction::Wizard(WizardAction::RunEditor(session)) => {
                // Suspend TUI, run editor, resume
                let result = session.run();

                // Process result back in wizard
                app.active_wizard_mut()?.handle_editor_result(result);

                // Force full redraw on next frame
                app.force_redraw = true;
            }
            // ... other actions ...
        }
    }
}
```

---

## 3. Error Handling

### 3.1 Editor Crash / Kill

| Scenario | Detection | User Experience |
|----------|-----------|-----------------|
| SIGKILL/SIGTERM | `status.success()` returns false | "Editor was terminated. Changes may be lost." |
| SIGINT (Ctrl+C) | Exit code 130 on Unix | "Editor cancelled. Returning to previous state." |
| Crash (segfault) | Exit code non-zero | "Editor crashed (code X). Check temp file at: {path}" |
| File deleted | `read_to_string` fails | "Draft file missing. Returning to previous state." |

```rust
fn interpret_exit_status(status: ExitStatus) -> EditorOutcome {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return match signal {
                2 => EditorOutcome::Cancelled,   // SIGINT
                9 => EditorOutcome::Killed,      // SIGKILL
                15 => EditorOutcome::Killed,     // SIGTERM
                _ => EditorOutcome::Crashed { signal },
            };
        }
    }

    match status.code() {
        Some(0) => EditorOutcome::Success,
        Some(code) => EditorOutcome::ExitedWithError { code },
        None => EditorOutcome::Killed,
    }
}

enum EditorOutcome {
    Success,
    Cancelled,
    Killed,
    Crashed { signal: i32 },
    ExitedWithError { code: i32 },
}
```

### 3.2 Temp File Cleanup

Temp files should persist on error for recovery:

```rust
impl EditorSession {
    fn create_temp_file(content: &str, extension: &str) -> io::Result<PathBuf> {
        let dir = std::env::temp_dir().join("casparian_drafts");
        std::fs::create_dir_all(&dir)?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("draft_{}.{}", timestamp, extension);
        let path = dir.join(filename);

        std::fs::write(&path, content)?;
        Ok(path)
    }

    fn cleanup_on_success(&self) {
        // Only delete temp file if editor succeeded and content was processed
        let _ = std::fs::remove_file(&self.temp_path);
    }

    fn preserve_on_error(&self) -> PathBuf {
        // Keep temp file, return path for error message
        self.temp_path.clone()
    }
}
```

---

## 4. Special Editor Handling

### 4.1 GUI Editors Without Wait

Some editors return immediately without `--wait` flag:

| Editor | Wait Flag | Detection |
|--------|-----------|-----------|
| VS Code | `--wait` | Check for `code` in command |
| Sublime Text | `--wait` | Check for `subl` in command |
| Atom | `--wait` | Check for `atom` in command |
| TextMate | `-w` | Check for `mate` in command |
| Zed | `--wait` | Check for `zed` in command |

```rust
fn ensure_wait_flag(config: &mut EditorConfig) {
    let wait_flags: &[(&str, &str)] = &[
        ("code", "--wait"),
        ("subl", "--wait"),
        ("atom", "--wait"),
        ("mate", "-w"),
        ("zed", "--wait"),
    ];

    for (editor_name, flag) in wait_flags {
        if config.command.contains(editor_name) && !config.args.contains(&flag.to_string()) {
            config.args.push(flag.to_string());
            tracing::info!("Added {} flag for {}", flag, editor_name);
        }
    }
}
```

### 4.2 Terminal Multiplexer Detection

If running inside tmux/screen, consider using a split pane instead of full suspension (optional enhancement):

```rust
fn detect_multiplexer() -> Option<Multiplexer> {
    if std::env::var("TMUX").is_ok() {
        Some(Multiplexer::Tmux)
    } else if std::env::var("STY").is_ok() {
        Some(Multiplexer::Screen)
    } else {
        None
    }
}

// Future enhancement: open editor in new tmux pane
// tmux split-window -h "vim /tmp/draft.yaml"
```

---

## 5. Configuration

### 5.1 Config Schema Addition

```toml
# ~/.casparian_flow/config.toml

[tui.editor]
# Override $EDITOR/$VISUAL (optional)
# command = "code --wait"

# Timeout for editor session (default: 3600 seconds / 1 hour)
timeout_seconds = 3600

# Auto-add --wait flag for known GUI editors (default: true)
auto_wait_flag = true

# Preserve temp files on error (default: true)
preserve_on_error = true
```

### 5.2 Rust Config Struct

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct EditorConfig {
    /// Override $EDITOR/$VISUAL
    pub command: Option<String>,
    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// Auto-add --wait for GUI editors
    #[serde(default = "default_true")]
    pub auto_wait_flag: bool,
    /// Keep temp files on error
    #[serde(default = "default_true")]
    pub preserve_on_error: bool,
}

fn default_timeout() -> u64 { 3600 }
fn default_true() -> bool { true }
```

---

## 6. TUI User Feedback

### 6.1 Pre-Editor Message

Before suspending TUI, show brief message:

```
Opening editor...
Temp file: /tmp/casparian_drafts/draft_20260113_143022.yaml
(Press any key in TUI after editor closes if screen doesn't restore)
```

### 6.2 Post-Editor Feedback

After editor closes:

| Outcome | Message | Next State |
|---------|---------|------------|
| Modified | "Changes detected. Validating..." | VALIDATING or REGENERATING |
| Unmodified | "No changes made." | Previous result state |
| Error | "Editor error: {details}. Temp file: {path}" | Previous result state |
| Timeout | "Editor timed out after 1 hour." | Previous result state |

---

## 7. Implementation Checklist

### Phase 1: Core Editor Support (1 day)
- [ ] Implement `resolve_editor()` with $VISUAL/$EDITOR priority
- [ ] Implement `platform_fallback()` for all platforms
- [ ] Implement `EditorSession::run()` with terminal handoff
- [ ] Add temp file creation/cleanup

### Phase 2: Error Handling (0.5 day)
- [ ] Implement exit status interpretation
- [ ] Add signal handling for Unix
- [ ] Preserve temp files on error
- [ ] Add error messages with recovery path

### Phase 3: State Integration (0.5 day)
- [ ] Add `WizardAction::RunEditor` to wizard action enum
- [ ] Integrate into TUI event loop
- [ ] Add force-redraw after editor returns
- [ ] Test with vim, nano, code --wait

### Phase 4: Configuration (0.5 day)
- [ ] Add `[tui.editor]` config section
- [ ] Implement auto-wait-flag for GUI editors
- [ ] Add timeout configuration
- [ ] Document in CLAUDE.md

---

## 8. Spec Updates Required

Add to `specs/ai_wizards.md` after Section 5.1.1 transitions table:

```markdown
#### 5.1.2 External Editor Handling

**EDITING State Behavior:**

The EDITING state suspends the TUI and hands off terminal control to the user's editor:

1. **Editor Resolution:** `$VISUAL` > `$EDITOR` > platform fallback (see below)
2. **Terminal Handoff:** TUI leaves alternate screen, disables raw mode
3. **Blocking Wait:** Process blocks until editor exits
4. **Resume:** TUI re-enters alternate screen, restores raw mode, forces redraw

**Platform Fallbacks:**

| Platform | Fallback Chain |
|----------|----------------|
| macOS | `open -t -W` (default text editor), `nano`, `vi` |
| Linux | `sensible-editor`, `nano`, `vi` |
| Windows | `notepad.exe` |

**Error Handling:**

| Scenario | Behavior |
|----------|----------|
| No editor available | Transition blocked; show error: "Set $EDITOR or $VISUAL" |
| Editor crashes | Return to previous state; preserve temp file |
| Editor timeout (1hr) | Return to previous state; show timeout message |
| File unchanged | Return to previous state silently |

**Temp File Location:** `/tmp/casparian_drafts/draft_{timestamp}.{yaml|py}`

**GUI Editor Support:** The `--wait` flag is auto-added for known GUI editors (VS Code, Sublime, etc.) to ensure blocking behavior.

See `specs/meta/sessions/ai_wizards/round_012/engineer.md` for full specification.
```

---

## 9. Trade-offs

**Pros:**
1. **Standard behavior** - Matches git, less, and other Unix tools
2. **Works everywhere** - Terminal editors (vim) and GUI editors (VS Code)
3. **Preserves data** - Temp files survive crashes for manual recovery
4. **Configurable** - Users can override via config or env vars

**Cons:**
1. **Full suspension** - TUI is unresponsive during edit (unavoidable for terminal editors)
2. **GUI editor complexity** - Need --wait flag detection
3. **Timeout necessary** - Prevents hung sessions; 1 hour default is arbitrary

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|------------------|
| Inline editing in TUI | Poor UX for multi-line code; no syntax highlighting |
| Background editor with polling | Breaks terminal editors completely |
| Tmux pane split | Adds tmux dependency; complex |
| Embedded editor widget | Significant implementation effort; worse than vim/VS Code |

---

## 10. New Gaps Introduced

None. This resolution is self-contained.

---

## 11. References

- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine, EDITING state)
- `specs/ai_wizards.md` Section 5.2.1 (Parser Lab state machine, EDITING state)
- `specs/ai_wizards.md` Section 5.4 (Semantic Path Wizard, EDITING state)
- Git source code: `editor.c` for editor resolution pattern
- crossterm crate documentation for terminal mode handling

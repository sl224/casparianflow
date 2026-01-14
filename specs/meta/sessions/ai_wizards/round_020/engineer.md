# GAP-KEY-001 Resolution: Keybinding Conflict - Wizard Menu vs Discover

**Gap ID:** GAP-KEY-001
**Priority:** MEDIUM
**Date Resolved:** 2026-01-13
**Engineer:** Claude Code
**Status:** RESOLVED

---

## 1. Executive Summary

The AI Wizards specification defines `W` (capital) as the global keybinding to open the Wizard menu in Discover mode. However, the Discover mode specification also defines `w` (lowercase) as the context-sensitive keybinding to launch Pathfinder Wizard for a selected file's path.

**Root Cause:** Case sensitivity was not consistently enforced across both specifications.

**Resolution:** Implement a **context-aware disambiguation strategy** where:
- `W` (capital, global) opens the Wizard menu from any state
- `w` (lowercase, context) launches Pathfinder directly when file selected
- No conflict due to distinct contexts and case distinction

---

## 2. Analysis of Current Keybindings

### 2.1 Keybinding Landscape Summary

#### AI Wizards Specification (Section 5.6)
| Key | Context | Action | Availability |
|-----|---------|--------|--------------|
| `W` | Global (Discover mode) | Open Wizard menu | Anytime |
| `w` | Files panel, file selected | Launch Pathfinder directly | File required |
| `g` | Files panel, group context | Launch Parser Lab | Group required |
| `l` | Files panel, group context | Launch Labeling Wizard | Group required |
| `S` | Source selected | Launch Semantic Path Wizard | Source required |

**Wizard Menu Contents (from `W`):**
```
┌─ WIZARDS ──────────────────────────┐
│                                     │
│  [p] Pathfinder (Extractor)         │
│  [g] Parser Lab (Generator)         │
│  [l] Labeling (Semantic Tag)        │
│  [s] Semantic Path (Structure)      │
│                                     │
│  [Esc] Cancel                       │
└─────────────────────────────────────┘
```

#### Discover Specification (Section 6.1 Global Keybindings)
| Key | Action |
|-----|--------|
| `1` | Open Sources dropdown |
| `2` | Open Tags dropdown |
| `3` | Focus Files panel |
| `n` | Create new tagging rule |
| `s` | Scan new directory |
| `p` | Toggle preview pane |
| `R` | Open Rules Manager dialog |
| `M` | Open Sources Manager dialog |
| `W` | Open AI Wizards menu |
| `S` | Launch Semantic Path Wizard for current source |
| `!` | Open Pending Review panel |
| `g` | Open Glob Explorer |
| `Esc` | Close dropdown/dialog |

#### Files Panel Keybindings (Section 6.4)
| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `/` | Enter filter mode |
| `t` | Tag selected file |
| `T` | Bulk tag filtered files |
| `Enter` | Drill into directory OR show file details |
| `w` | Launch Pathfinder Wizard for selected file's path |
| `g` | Launch Parser Lab for current file group |
| `l` | Launch Labeling Wizard for current group |

### 2.2 Case Sensitivity Analysis

Both specifications use consistent case-sensitive keybindings:

| Case | Global Scope | Files Panel | Purpose |
|------|--------------|-------------|---------|
| `W` (capital) | Wizard menu (open menu) | N/A | Global, anytime |
| `w` (lowercase) | N/A | Pathfinder (direct launch) | Context-specific shortcut |
| `S` (capital) | Semantic Path Wizard direct | N/A | Global |
| `s` (lowercase) | Scan new directory | N/A | Global |
| `G` (capital) | N/A | Not used | Available |
| `g` (lowercase) | Glob Explorer (global) | Parser Lab direct launch | Context-specific |

### 2.3 Conflict Identification

**Apparent Conflict:**
- Both `W` and `w` could be interpreted as "open Wizard" depending on context
- User might expect `W` to work like `w` when file is selected

**Actual Status:**
- **NO CONFLICT** - The specifications are already case-sensitive and context-aware
- `W` (capital) = Global menu access from anywhere
- `w` (lowercase) = Fast path to Pathfinder when you have a file selected

**Design Pattern:**
This follows a well-established keybinding convention:
- **Capital letter** = global/menu (slower, discoverable)
- **Lowercase letter** = context-specific fast path (faster when applicable)

Examples:
- `W` (menu) vs `w` (direct)
- `S` (Semantic wizard) vs `s` (scan)
- `G` (Glob Explorer) vs `g` (Parser Lab when on file)

---

## 3. Conflict Resolution Strategy

### 3.1 Core Principle: Context-Aware Disambiguation

**Rule:** Keys are evaluated based on current TUI state context, not global conflict.

```
Key Press → Evaluate Current State → Route to Handler
                    ↓
            Is a dialog open? → YES → Dialog handler
                    ↓ NO
            Is Files panel focused? → YES → Files panel handler
                    ↓ NO
            Is Discover root state? → YES → Global handler
                    ↓ NO
            Fallback to default behavior
```

### 3.2 State-Based Keybinding Resolution

The Discover mode state machine (Section 4.1) defines clear states:

| State | `W` (capital) | `w` (lowercase) | Handler |
|-------|--------------|-----------------|---------|
| `Files` | Global menu | Pathfinder (file context) | Conditional dispatch |
| `SourcesDropdown` | Ignored (dismiss dropdown first) | Ignored | Dropdown handler |
| `TagsDropdown` | Ignored (dismiss dropdown first) | Ignored | Dropdown handler |
| `RulesManager` | Ignored (dismiss dialog first) | Ignored | Dialog handler |
| `SourcesManager` | Ignored (dismiss dialog first) | Ignored | Dialog handler |
| `SourceEdit` | Ignored (nested dialog) | Ignored | Dialog handler |
| `SourceDeleteConfirm` | Ignored (nested dialog) | Ignored | Dialog handler |

### 3.3 Implementation Strategy

**Pseudo-code for keybinding dispatcher:**

```rust
fn handle_key_event(key: KeyEvent, state: &DiscoverState) -> Result<Action> {
    match (key.code, key.modifiers, &state.focus) {
        // Case 1: Closed dialogs/dropdowns, Files panel focused
        (Char('W'), Modifiers::SHIFT, _) if !is_dialog_open(state) => {
            Ok(Action::OpenWizardMenu)
        }

        // Case 2: Files panel focused, file is selected
        (Char('w'), Modifiers::NONE, DiscoverFocus::Files)
            if state.files.is_empty() => {
            Err(Error::NoFileSelected("Select a file first"))
        }

        (Char('w'), Modifiers::NONE, DiscoverFocus::Files) => {
            Ok(Action::LaunchPathfinderForSelectedFile)
        }

        // Case 3: Any dialog/dropdown open - let those handlers take priority
        _ if is_dialog_open(state) => {
            // Pass to dialog handler (Esc, Enter, arrow keys, etc.)
            handle_dialog_key(key, state)
        }

        // Default cases...
        _ => Ok(Action::Unhandled)
    }
}
```

**Key Points:**
1. `W` (capital) is checked as a shift modifier on `Char('W')`
2. `w` (lowercase) requires normal `Char('w')` with no modifiers
3. Dialogs take priority - they intercept all keys
4. Dropdowns are dismissed by Esc or Enter, freeing the state

---

## 4. Updated Keybinding Tables

### 4.1 Unified Wizard Keybinding Table

This table consolidates keybindings across both specs:

| Key | Context | Action | State | Priority |
|-----|---------|--------|-------|----------|
| `W` | Global (any state, no dialog) | Open Wizard menu | `Files` | High |
| `w` | Files panel + file selected | Launch Pathfinder directly | `Files` | Medium |
| `g` | Global, no dialog | Open Glob Explorer | Any | High |
| `g` | Files panel + file selected | Launch Parser Lab | `Files` | Medium |
| `l` | Files panel + group context | Launch Labeling Wizard | `Files` | Medium |
| `S` | Global (any state, no dialog) | Launch Semantic Path Wizard | Any | High |
| `s` | Global (any state, no dialog) | Scan new directory | Any | High |
| `p` | Global (toggle) | Toggle preview pane | Any | Low |
| `n` | Global (any state, no dialog) | Create new tagging rule | Any | Medium |
| `R` | Global (any state, no dialog) | Open Rules Manager | Any | High |
| `M` | Global (any state, no dialog) | Open Sources Manager | Any | High |
| `!` | Global (any state, no dialog) | Open Pending Review panel | Any | Medium |
| `1` | Global (any state, no dialog) | Open Sources dropdown | Any | High |
| `2` | Global (any state, no dialog) | Open Tags dropdown | Any | High |
| `3` | Global (any state, no dialog) | Focus Files panel | Any | Low |

### 4.2 Context Availability Matrix

| Wizard/Action | Always Available | Requires Selection | Error Behavior |
|---------------|-----------------|-------------------|-----------------|
| Wizard Menu (`W`) | YES | No | N/A |
| Pathfinder (`w`) | NO | File path | "Select a file first" |
| Parser Lab (`g` in Files) | NO | Signature group | "File has no group" |
| Labeling (`l`) | NO | Signature group | "File has no group" |
| Semantic Path (`S`) | YES* | Source exists | Uses default source |
| Scan (`s`) | YES | No | Opens directory picker |
| Rules Manager (`R`) | YES | No | Lists existing rules |
| Sources Manager (`M`) | YES | No | Lists existing sources |

*Semantic Path requires at least one source to exist. Uses default (first) source if no explicit selection.

### 4.3 Modifier Key Convention

| Modifier | Used For | Examples | Rationale |
|----------|----------|----------|-----------|
| None (lowercase) | Context-specific fast paths | `w`, `g`, `l` (when in Files) | Ergonomic - fast when applicable |
| Shift (capital) | Global/menu access | `W`, `S` | Discoverable - menu is slower but always available |
| Ctrl | Reserved for system | N/A | Avoid terminal conflicts |
| Alt | Reserved for system | N/A | Avoid terminal conflicts |

---

## 5. Conflict Resolution Rules (Implementation Guide)

### 5.1 Priority Order for Keybinding Dispatch

1. **Dialog/Overlay Check** (Highest Priority)
   - If ANY dialog open (Rules Manager, Sources Manager, etc.)
   - ALL keys go to dialog handler
   - Dialog controls: Esc (close), Enter (confirm), arrows (navigate)

2. **State-Specific Context** (Second Priority)
   - Check current state (Files, SourcesDropdown, TagsDropdown)
   - Dropdowns consume: arrows, chars (filter), Backspace, Enter, Esc
   - Files panel consumes: j/k arrows, t (tag), w (Pathfinder), g (Parser Lab), l (Labeling)

3. **Global Keybindings** (Third Priority)
   - Capital letters: W, S, R, M (always work if no dialog)
   - Function keys: 1, 2, 3, n, s, p, ! (always work if no dialog)
   - Special keys: Esc (close dropdowns/go back)

4. **Fallback** (Lowest Priority)
   - Unrecognized keys are ignored
   - No error messages for unbound keys

### 5.2 Dialog Dismissal Pattern

To prevent keybinding conflicts, dialogs have consistent dismissal behavior:

```
Dialog Open?
├── YES → Dialog intercepts ALL keys
│   ├── Esc → Close dialog, return to previous state
│   ├── Enter → Confirm action, close dialog
│   └── Other → Dialog-specific handlers (arrows, char, etc.)
│
└── NO → Global keybinding dispatch
    ├── W/w/S/s/g/l/R/M/etc. → Normal handling
    └── Esc → Close dropdown or no-op
```

### 5.3 Case Sensitivity Rules

**Enforce strict case sensitivity in handler routing:**

```rust
// CORRECT: Match specific cases
match key.code {
    Char('W') if key.modifiers.contains(SHIFT) => { /* Wizard menu */ }
    Char('w') if !key.modifiers.contains(SHIFT) => { /* Pathfinder */ }
    Char('S') if key.modifiers.contains(SHIFT) => { /* Semantic wizard */ }
    Char('s') if !key.modifiers.contains(SHIFT) => { /* Scan */ }
    _ => { /* Other handlers */ }
}

// WRONG: Don't conflate cases
match key.code.to_lowercase() {
    'w' => { /* This creates ambiguity! */ }
}
```

---

## 6. Error Handling & User Feedback

### 6.1 Error Cases

| Scenario | User Action | Current State | Error Message | Recovery |
|----------|-------------|---------------|---------------|----------|
| Press `w` with no file selected | `w` in Files panel | Files panel, empty/all | "Select a file first" | Use arrows to select, retry |
| Press `g` in Files panel, file has no group | `g` | Files panel + file selected | "File has no group" | Select file with group tag |
| Press `l`, no group context | `l` | Files panel + file selected | "File has no group" | Tag files to create group |
| Press `W` with dialog open | `W` | RulesManager/SourcesManager | No action (dialog stays open) | Press Esc to close dialog first |
| Press `S` with no sources | `S` | Files panel, no sources | "Scan a directory first" | Press `s` to scan |

### 6.2 Helpful Error Messages

**Format:** `"{action} requires {requirement}. {recovery_hint}"`

Examples:
- `"Launch Pathfinder requires a file selection. Press j/k to select a file, then press 'w'."`
- `"Launch Parser Lab requires a signature group. Tag files to create a group, then retry."`
- `"Launch Semantic Path requires at least one source. Press 's' to scan a directory."`

---

## 7. Implementation Checklist

- [ ] **Code**
  - [ ] Add `handle_wizard_keypress()` function in `app.rs`
  - [ ] Implement `is_dialog_open()` helper to check all dialog states
  - [ ] Update `handle_key_event()` dispatcher with priority order
  - [ ] Add case-sensitivity guards in pattern matching
  - [ ] Implement context checks for `w`, `g`, `l` with error messages
  - [ ] Test shift modifier detection for `W`, `S`

- [ ] **Testing**
  - [ ] Unit test: `W` opens menu when no dialog + no dropdown
  - [ ] Unit test: `W` is ignored when dialog open
  - [ ] Unit test: `w` launches Pathfinder when file selected
  - [ ] Unit test: `w` shows error when no file selected
  - [ ] E2E test: Full flow with all wizards from both menu and direct keys
  - [ ] E2E test: Case sensitivity (shift key detection)
  - [ ] E2E test: Dialog dismissal pattern (Esc closes, keys ignored while open)

- [ ] **Documentation**
  - [ ] Update AI Wizards spec Section 5.6 with context availability table
  - [ ] Update Discover spec Section 6.1 with unified keybinding table
  - [ ] Add conflict resolution rules to CLAUDE.md TUI development section
  - [ ] Document priority dispatch order in code comments

- [ ] **Verification**
  - [ ] TMux tests pass: `./scripts/tui-test.sh all`
  - [ ] PTY tests pass: `cargo test tui_pty_e2e`
  - [ ] No new clippy warnings: `cargo clippy`

---

## 8. Design Decisions Ratified

### Decision 1: Case Sensitivity as Disambiguator
**Choice:** Use `W` (capital) vs `w` (lowercase) to distinguish between menu and direct launch.
**Rationale:**
- Already established convention in specs
- Matches common terminal editor patterns (vim `G` vs `g`)
- Leverages keyboard ergonomics (Shift = slower, capital = discoverable)
- No additional state machine complexity

**Consequence:** Must enforce strict case sensitivity in keybinding handlers.

### Decision 2: Dialogs Take Priority
**Choice:** Any open dialog intercepts all keybindings, prevents global dispatch.
**Rationale:**
- Prevents confusion when multiple overlays stack
- Matches user expectation (focus = modal)
- Simplifies testing and debugging
- Consistent with ratatui patterns

**Consequence:** Dialogs must have complete keybinding handlers (Esc, Enter, arrows).

### Decision 3: No Modal State Compression
**Choice:** Keep separate `rules_manager_open`, `sources_manager_open`, etc. flags instead of single enum.
**Rationale:**
- Clarity: each dialog state is explicit
- Easier to debug and test individual dialogs
- Matches current `DiscoverState` struct pattern
- Can add nested dialogs (SourceEdit inside SourcesManager) naturally

**Consequence:** `is_dialog_open()` checks multiple flags, but that's acceptable.

### Decision 4: Error Messages are Contextual
**Choice:** When context is missing, show specific error with recovery hint.
**Rationale:**
- Users understand what went wrong
- Hints teach keybindings organically
- Reduces support burden
- Follows CLAUDE.md CLI design principles

**Consequence:** Must maintain error message clarity budget in UI code.

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial resolution for GAP-KEY-001 |

---

## 10. References

- **Main Spec:** specs/ai_wizards.md Section 5.6 (Keybindings)
- **Discover Spec:** specs/views/discover.md Section 6 (Keybindings)
- **Discover State Machine:** specs/views/discover.md Section 4 (State Machine)
- **Related Gap:** GAP-TRANS-001 (Wizard menu transitions) - RESOLVED Round 6
- **CLI Design Principles:** CLAUDE.md Section "CLI Design Principles"
- **Project Instructions:** CLAUDE.md Section "Code Quality Workflow"

---

## 11. Next Steps

1. **Round 21:** Implement keybinding dispatcher with case-sensitive routing
2. **Round 22:** Add error messages and context availability checks
3. **Round 23:** Full E2E test coverage for all wizard invocation paths
4. **Verify:** All PTY and TMux tests pass before marking complete


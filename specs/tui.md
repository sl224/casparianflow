# TUI Master Specification

**Status:** READY FOR IMPLEMENTATION
**Version:** 1.1
**Parent:** spec.md
**Last Updated:** 2026-01-13

---

## 1. Overview

The Casparian Flow TUI provides a keyboard-driven interface for data discovery, parser development, and pipeline monitoring. This master spec defines global patterns; individual views are specified in `specs/views/`.

### 1.1 Design Principles

1. **Keyboard-first**: Every action has a key binding
2. **Progressive disclosure**: Simple by default, power on demand
3. **Live feedback**: Changes reflect immediately
4. **Consistent patterns**: Same keys do same things everywhere
5. **No mode confusion**: Current context always visible
6. **Discoverability**: Every keybinding must be discoverable through the UI (see Section 6)

### 1.2 View Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         HOME HUB                                │
│                                                                 │
│   [1] Discover    [2] Parser Bench    [3] Jobs    [4] Sources   │
└─────────────────────────────────────────────────────────────────┘
         │                  │               │            │
         ▼                  ▼               ▼            ▼
    ┌─────────┐      ┌───────────┐    ┌─────────┐  ┌──────────┐
    │Discover │      │Parser     │    │ Jobs    │  │ Sources  │
    │         │      │Bench      │    │         │  │          │
    └─────────┘      └───────────┘    └─────────┘  └──────────┘
         │                │
         ▼                ▼
    ┌─────────┐      ┌───────────┐
    │Extraction│     │Test       │
    │Rules    │      │Results    │
    └─────────┘      └───────────┘
```

### 1.3 View Specs

| View | Spec | Purpose |
|------|------|---------|
| Home | `views/home.md` | Navigation hub, status overview |
| Discover | `views/discover.md` | File scanning, tagging |
| Parser Bench | `views/parser_bench.md` | Parser development, testing |
| Jobs | `views/jobs.md` | Job monitoring, logs |
| Settings | `views/settings.md` | Configuration |
| Extraction Rules | `views/extraction_rules.md` | Rule creation, testing (Draft) |

---

## 2. Global Navigation

### 2.1 Primary Navigation (Always Available)

| Key | Action | Context |
|-----|--------|---------|
| `1` | Go to Discover | From any view |
| `2` | Go to Parser Bench | From any view |
| `3` | Go to Jobs | From any view |
| `4` | Go to Sources | From any view |
| `0` or `H` | Go to Home | From any view |
| `Esc` | Back / Close dialog | Context-dependent |
| `q` | Quit (with confirmation if unsaved) | From any view |

**View-level overrides:**
- **Discover:** `1`, `2`, `3` control panel focus (Sources/Tags/Files) instead of view navigation.
  Use `0`/`H` or `4` to navigate away. See `views/discover.md` Section 6.1.
- Views may override keys when contextually appropriate; overrides are documented in view specs.

### 2.4 Jobs View Signals
Jobs view emphasizes batch throughput over topology:
- Throughput, % complete, and ETA for active batches.
- Stragglers and quarantine counts for wide fan-out parsing.
- Schema contract status and logical execution date when available.
See `specs/views/jobs.md`.

### 2.2 Navigation Stack

Views maintain a navigation stack for `Esc` to go back:

```
Home → Discover → Extraction Rules → [Esc] → Discover → [Esc] → Home
```

**Stack rules:**
- Max depth: 5
- Primary views (1-4) reset stack
- Dialogs don't push to stack
- `Esc` at root returns to Home

### 2.3 Breadcrumb Display

```
┌─ Casparian Flow ─────────────────────────────────────────────────┐
│ Home > Discover > Extraction Rules                    [?] Help   │
├──────────────────────────────────────────────────────────────────┤
```

---

## 3. Global Keybindings

### 3.1 Universal Keys (Work Everywhere)

| Key | Action |
|-----|--------|
| `?` | Toggle help overlay |
| `q` | Quit application |
| `Esc` | Back / Cancel / Close |
| `Tab` | Next focus area |
| `Shift+Tab` | Previous focus area |
| `Enter` | Confirm / Select |
| `Space` | Toggle / Check |

### 3.2 List Navigation (Any List/Table)

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `PgUp` / `Ctrl+u` | Page up |
| `PgDn` / `Ctrl+d` | Page down |
| `Home` / `g` | Go to first |
| `End` / `G` | Go to last |
| `/` | Start filter/search |
| `n` | Next search result (only after `/` search active) |
| `N` | Previous search result (only after `/` search active) |
| `Esc` | Clear search, return to normal mode |

**Search mode behavior:**
- After pressing `/`, user types search query
- `Enter` confirms search, highlights matches
- `n`/`N` navigate between matches (vim convention)
- `Esc` clears search and exits search mode
- **Important:** `n` only means "next result" when search is active. Otherwise, `n` triggers view-specific action (see 3.3).

**For view specs:** Do NOT repeat these list navigation bindings. Only document:
- View-specific additions (e.g., `Enter` opens detail)
- Context-specific behavior variations
- Reference: "List navigation per tui.md Section 3.2"

### 3.3 Common Actions

| Key | Action | Views | Note |
|-----|--------|-------|------|
| `n` | New / Create | Discover, Parser Bench, Sources, Extraction | Only when search NOT active |
| `e` | Edit selected | Discover, Extraction, Sources | |
| `d` | Delete selected | Views with deletable items | |
| `r` | Refresh / Reload | All views | |
| `t` | Test / Run | Parser Bench, Extraction | |
| `Enter` | Open / Drill down | All views | |

**Key override rules:**
- When search is active (after `/`), `n`/`N` always mean next/previous result
- When search is NOT active, `n` means "New/Create" in applicable views
- Views may document additional overrides (e.g., Jobs log viewer uses `n/N` for search)

**View-specific key semantics (acceptable variance):**

Some keys have different meanings across views based on context:

| Key | Jobs | Extraction | Sources | Parser Bench |
|-----|------|------------|---------|--------------|
| `c` | Cancel job | Coverage report | Manage class | — |
| `r` | — (use `R` for retry) | — | — | Re-run test (in ResultView) |
| `R` | Retry failed job | — | — | Resume paused parser |

These variances are intentional and documented in respective view specs. The key semantics
make sense within each view's context. Implementers should ensure footer hints update
to show the correct action for the current view.

---

## 4. Layout Patterns

### 4.1 Standard Layout

```
┌─ Header ─────────────────────────────────────────────────────────┐
│ View Title                                    Status  [?] Help   │
├─ Sidebar ──────────────┬─ Main Content ──────────────────────────┤
│                        │                                         │
│  Navigation            │  Primary content area                   │
│  or filters            │                                         │
│                        │                                         │
│                        │                                         │
│                        │                                         │
├────────────────────────┴─────────────────────────────────────────┤
│ Footer: Context-sensitive keybindings                            │
└──────────────────────────────────────────────────────────────────┘
```

### 4.2 Panel Ratios

| Layout | Sidebar | Main | Use Case |
|--------|---------|------|----------|
| Default | 25% | 75% | Browse views |
| Collapsed | 0% | 100% | Focus mode (`Ctrl+\`) |
| Wide sidebar | 40% | 60% | Complex filters |

### 4.3 Focus Cycling

`Tab` cycles through focusable areas:

```
Header (if interactive)
    ↓
Sidebar
    ↓
Main Content
    ↓
Footer (if interactive)
    ↓
(back to Header)
```

### 4.4 Dialog Pattern

Dialogs appear centered, dimming background:

```
┌─────────────────────────────────────────────────────────────────┐
│                         (dimmed)                                │
│        ┌─ Dialog Title ────────────────────────┐                │
│        │                                        │                │
│        │  Content                               │                │
│        │                                        │                │
│        │  [Enter] Confirm    [Esc] Cancel      │                │
│        └────────────────────────────────────────┘                │
│                         (dimmed)                                │
└─────────────────────────────────────────────────────────────────┘
```

**Dialog rules:**
- Always show how to close (`Esc`)
- Focusable elements: `Tab` between them
- `Enter` on primary action
- Trap focus inside dialog

### 4.5 Confirmation Dialog Pattern

Destructive actions (delete, cancel, clear) require confirmation:

```
┌─ [Action] [Item] ───────────────────────────────────────────┐
│                                                              │
│   [Action] "item_name"?                                      │
│                                                              │
│   This will:                                                 │
│   • [Consequence 1]                                          │
│   • [Consequence 2]                                          │
│                                                              │
│   [Enter] Confirm  [Esc] Cancel                              │
└──────────────────────────────────────────────────────────────┘
```

**Confirmation keybindings:**
| Key | Action |
|-----|--------|
| `Enter` | Execute action |
| `Esc` | Cancel, return to previous state |
| `Tab` | Switch focus between buttons (if multiple) |

**Actions requiring confirmation:**
- Deleting items (rules, sources)
- Cancelling running operations
- Clearing completed items
- Any irreversible state change

**Impact display:**
Always show what will happen (file counts, affected items, data loss).
View specs reference this pattern for their specific confirmation dialogs.

---

## 5. Visual Patterns

### 5.1 Colors (Semantic)

| Element | Color | ANSI |
|---------|-------|------|
| Primary action | Blue | `\x1b[34m` |
| Success | Green | `\x1b[32m` |
| Warning | Yellow | `\x1b[33m` |
| Error | Red | `\x1b[31m` |
| Muted/disabled | Gray | `\x1b[90m` |
| Selected/focused | Inverted | `\x1b[7m` |

### 5.2 Borders

| Context | Style |
|---------|-------|
| Panel | Single line `─│┌┐└┘` |
| Dialog | Double line `═║╔╗╚╝` |
| Focus | Bold/highlighted border |
| Disabled | Dotted `┄┆` |

### 5.3 Status Indicators

| Indicator | Meaning | Color | Use Cases |
|-----------|---------|-------|-----------|
| `✓` | Complete / Success | Green | Job complete, extraction complete, test passed |
| `●` | Active / Healthy | Green | Source healthy, parser healthy, system OK |
| `○` | Inactive / Pending / Queued | Gray | Job queued, source stale, not started |
| `↻` | Running / In Progress | Blue | Job running, scan in progress, loading |
| `⚠` | Warning / Partial | Yellow | Partial extraction, approaching limit |
| `✗` | Error / Failed | Red | Job failed, source error, test failed |
| `⏸` | Paused | Yellow | Circuit breaker tripped, user paused |
| `⊘` | Cancelled | Gray | Job cancelled by user |

**Key distinction:**
- `✓` = **Finished successfully** (terminal state)
- `●` = **Currently OK** (ongoing health)
- `↻` = **In progress** (actively running)

Views should use `✓` for completed items, `●` for healthy status, and `↻` for running operations.

### 5.4 Progress Display

```
Processing files... ████████████░░░░░░░░ 62% (1,247 / 2,012)
```

For indeterminate: `Processing... ▓▒░▒▓▒░▒`

---

## 6. Discoverability & Help System

> **MANDATORY RULE:** Every keybinding must be discoverable through at least one of:
> 1. Footer hints (primary actions for current context)
> 2. Help overlay (`?` key)
> 3. Dialog footer (for dialog-specific keys)
>
> **Implementation Checklist for New Features:**
> - [ ] All keybindings documented in spec's keybinding table
> - [ ] Primary actions visible in footer for relevant states
> - [ ] Secondary actions accessible via `?` help
> - [ ] Dialog-specific keys shown in dialog footer
>
> **Violation = Bug:** If a keybinding exists but isn't discoverable, that's a UX bug.

### 6.1 Discoverability Mechanisms

| Mechanism | What to Show | When |
|-----------|--------------|------|
| **Footer** | 4-6 most relevant actions | Always visible, context-aware |
| **Help Overlay** (`?`) | Complete keybinding reference | On demand |
| **Dialog Footer** | Dialog-specific actions | While dialog is open |
| **Empty States** | "Press X to do Y" hints | When list is empty |
| **Tooltips** | Extended descriptions | On hover (2s) or `Ctrl+?` |

### 6.2 Footer Design Rules

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [primary] [primary] [primary]  │  [dialog keys]  │  [navigation hints]  │
└──────────────────────────────────────────────────────────────────────────┘
```

**Footer must include:**
1. **Context-primary actions** (left): Actions for current focus/state
2. **Manager shortcuts** (middle): Keys to open management dialogs (e.g., `[R] Rules [M] Sources`)
3. **Navigation hints** (right): How to change context (e.g., `1:Source 2:Tags`)

**Footer must update when:**
- View state changes (dialog opens/closes)
- Focus changes (different panel)
- Mode changes (filter active, search active)

### 6.3 Spec Author Requirements

When adding a new keybinding to a view spec:

1. **Add to keybinding table** (Section 6.x of view spec)
2. **Update footer text** for all relevant states
3. **Add to help overlay content** (or reference tui.md defaults)
4. **If global key**: Add to tui.md Section 3 AND view footer

**Template for view specs:**
```markdown
### X.Y [Feature] Keybindings

| Key | Action | Footer Text |
|-----|--------|-------------|
| `M` | Open Manager | `[M] Manager` |

**Footer updates required:**
- State A: Add `[M] Manager` to footer
- State B: Add `[M] Manager` to footer
- State C: N/A (dialog open, M disabled)
```

### 6.4 Help Overlay (`?`)

Pressing `?` shows context-sensitive help:

```
┌─ Help: Discover Mode ───────────────────────────────────────────┐
│                                                                  │
│  Navigation                      Actions                         │
│  ──────────                      ───────                         │
│  ↑/↓ or j/k  Move selection      n  New tagging rule             │
│  Enter       Open/select         e  Edit selected rule           │
│  Tab         Switch panel        d  Delete selected              │
│  Esc         Back to Home        r  Refresh file list            │
│  1-4         Jump to view        /  Filter files                 │
│                                                                  │
│  Press ? again or Esc to close                                   │
└──────────────────────────────────────────────────────────────────┘
```

### 6.5 Footer Hints

Footer always shows most relevant actions:

```
│ [n] New rule  [e] Edit  [d] Delete  [/] Filter  [?] Help  [q] Quit │
```

### 6.6 Contextual Tooltips

On long hover (2s) or `Ctrl+?`, show tooltip for focused element.

---

## 7. State Management

### 7.1 View State Persistence

Each view maintains state that survives navigation:

```rust
struct ViewState {
    scroll_position: usize,
    selected_index: Option<usize>,
    filter_text: String,
    expanded_items: HashSet<String>,
}
```

State persists:
- Within session: Always
- Across sessions: Optional (saved to DB)

### 7.2 Unsaved Changes

If view has unsaved changes:
- Show `[*]` in header
- Confirm on `Esc` or navigation away
- Auto-save drafts every 30s

### 7.3 Loading States

| State | Display |
|-------|---------|
| Initial load | Full-screen spinner |
| Refresh | Inline spinner, content visible |
| Background | Footer indicator only |
| Error | Inline error with retry option |

### 7.4 Refresh Strategy

Views implement consistent refresh behavior:

**Automatic Refresh Intervals:**
| Context | Interval | Examples |
|---------|----------|----------|
| Default | 5 seconds | Dashboard stats, list updates |
| Active operation | 500ms | Progress bars, running jobs |
| Background data | 30 seconds | Coverage stats, secondary metrics |

**Refresh Triggers:**
| Trigger | Behavior |
|---------|----------|
| Timer | Automatic refresh at interval |
| Manual (`r` key) | Immediate refresh |
| Event-driven | After action completes (create, update, delete) |
| View enter | Refresh on `on_enter()` |

**Pause Conditions:**
- Dialogs open (avoid background updates during user input)
- Log viewer active (preserve scroll position)
- User is typing (debounce until idle)

**Debouncing:**
Multiple refresh triggers within 100ms coalesce into single refresh.
View specs reference this pattern rather than repeating refresh logic.

---

## 8. Input Handling

### 8.1 Text Input Pattern

```
┌─ Field Label ───────────────────────────────────────┐
│ user input here█                                     │
└─────────────────────────────────────────────────────┘
  Hint: Enter pattern like *.csv or sales/**
```

| Key | Action |
|-----|--------|
| `Ctrl+a` | Select all |
| `Ctrl+u` | Clear line |
| `Ctrl+w` | Delete word |
| `←` / `→` | Move cursor |
| `Ctrl+←/→` | Move by word |

### 8.2 Dropdown Pattern (Telescope-style)

```
┌─ Select Tag ────────────────────────────────────────┐
│ > sales█                                             │
├─────────────────────────────────────────────────────┤
│   sales (142 files)                                  │
│   sales_2024 (89 files)                              │
│   sales_archive (23 files)                           │
└─────────────────────────────────────────────────────┘
```

- Type to filter
- `↑/↓` to navigate
- `Enter` to select
- `Esc` to cancel

### 8.3 Multi-Select Pattern

```
│ [ ] Item 1                                           │
│ [✓] Item 2                                           │
│ [✓] Item 3                                           │
│ [ ] Item 4                                           │
```

| Key | Action |
|-----|--------|
| `Space` | Toggle current |
| `a` | Select all |
| `A` | Deselect all |
| `Enter` | Confirm selection |

---

## 9. Error Handling

### 9.1 Error Display Levels

| Level | Display |
|-------|---------|
| Field error | Inline below field, red |
| Action error | Toast notification, dismissable |
| View error | Inline banner with retry |
| Fatal error | Full-screen with diagnostic info |

### 9.2 Toast Notifications

```
                    ┌─ Error ─────────────────────────┐
                    │ ✗ Failed to save rule           │
                    │   Network timeout. Retry?       │
                    │   [r] Retry  [Esc] Dismiss      │
                    └─────────────────────────────────┘
```

Auto-dismiss after 5s (errors stay until dismissed).

### 9.3 Retry Pattern

All network/IO operations support retry:
- Show error with `[r] Retry` option
- Exponential backoff for auto-retry
- Max 3 retries before giving up

---

## 10. Accessibility

### 10.1 Screen Reader Support

- All interactive elements have labels
- Focus changes announced
- Error states announced immediately
- Progress updates announced periodically

### 10.2 Contrast Requirements

- Minimum 4.5:1 contrast ratio for text
- 3:1 for large text and icons
- Focus indicators always visible

### 10.3 Keyboard-Only Operation

- No mouse-only features
- Tab order follows visual order
- Skip links for repetitive content

---

## 11. Performance

### 11.1 Targets

| Operation | Target |
|-----------|--------|
| View switch | < 50ms |
| List scroll | 60fps |
| Filter update | < 100ms |
| Full refresh | < 500ms |

### 11.2 Virtualization

Lists with > 100 items use virtual scrolling:
- Render only visible rows + buffer
- Maintain scroll position on filter
- Smooth scrolling animation

### 11.3 Background Loading

Large data loads happen in background:
- Show immediate results (first 50)
- Load rest progressively
- "Loading more..." indicator

---

## 12. Implementation

### 12.1 Technology

- **Framework**: ratatui (Rust TUI library)
- **Event loop**: crossterm for input handling
- **State**: Custom state machine per view
- **Async**: tokio for background operations

### 12.2 File Structure

```
crates/casparian/src/cli/tui/
├── mod.rs              # TUI entry point
├── app.rs              # Application state
├── event.rs            # Event handling
├── ui.rs               # Rendering coordinator
├── theme.rs            # Colors, styles
├── widgets/            # Reusable widgets
│   ├── dropdown.rs
│   ├── table.rs
│   ├── dialog.rs
│   └── progress.rs
└── views/              # View implementations
    ├── home.rs
    ├── discover.rs
    ├── parser_bench.rs
    ├── jobs.rs
    └── sources.rs
```

### 12.3 View Trait

```rust
pub trait View {
    fn name(&self) -> &'static str;
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_event(&mut self, event: Event) -> ViewAction;
    fn help_text(&self) -> Vec<(&'static str, &'static str)>;
    fn on_enter(&mut self);  // Called when view becomes active
    fn on_leave(&mut self);  // Called when navigating away
}

pub enum ViewAction {
    None,
    Navigate(ViewId),
    Back,
    Quit,
    ShowDialog(Box<dyn Dialog>),
    Refresh,
}
```

---

## Appendix A: Keybinding Quick Reference

### Global

| Key | Action |
|-----|--------|
| `1-4` | Jump to view |
| `0` / `H` | Home |
| `?` | Help |
| `q` | Quit |
| `Esc` | Back |
| `Tab` | Next focus |
| `/` | Search |

### Lists

| Key | Action |
|-----|--------|
| `j` / `↓` | Down |
| `k` / `↑` | Up |
| `g` | First |
| `G` | Last |
| `Enter` | Select |
| `Space` | Toggle |

### Actions

| Key | Action |
|-----|--------|
| `n` | New |
| `e` | Edit |
| `d` | Delete |
| `r` | Refresh |
| `t` | Test |

---

## Appendix B: View Spec Template

Each view spec in `specs/views/` should follow this structure:

```markdown
# [View Name] - TUI View Spec

**Status:** [Draft | Approved | Implemented]
**Parent:** specs/tui.md
**Version:** X.Y

---

## 1. Overview
[Purpose and user goals]

## 2. User Workflows
[Step-by-step user journeys]

## 3. Layout
[ASCII wireframes]

## 4. State Machine
[States and transitions]

## 5. View-Specific Keybindings
[Keys unique to this view]

## 6. Data Model
[Rust structs]

## 7. Implementation Notes
[View-specific considerations]
```

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Initial master TUI spec. Extracted common patterns from discover.md and parser_bench.md. |
| 2026-01-13 | 1.1 | **Discoverability Rule (Section 6)**: Added mandatory requirement that all keybindings must be discoverable via footer, help overlay, or dialog footer. Added spec author checklist and footer design rules. Added Design Principle #6. |

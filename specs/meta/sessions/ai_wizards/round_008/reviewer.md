# Reviewer Round 008: GAP-FLOW-001

## Review: GAP-FLOW-001 (Wizard Invocation from Files Panel)

### Summary

The Engineer proposes a comprehensive resolution for wizard invocation from the Files panel in Discover mode. The proposal covers context determination algorithms for all four wizards (Pathfinder, Parser Lab, Labeling, Semantic Path), focus management during wizard open/close, error handling with actionable messages, visual feedback through status bar hints, and a complete entry point inventory.

### Critical Issues

No critical issues identified. The proposal is thorough and well-structured.

---

### High Priority

**ISSUE-R8-001: Context determination for Pathfinder has edge case gap**

The Pathfinder context algorithm (Section 2.2) has three priority levels:

1. File selected in Files panel -> SingleFile
2. Filter active with filtered files -> MultipleFiles
3. In Pending Review > Unmatched Paths -> MultipleFiles

However, there is an unspecified case: What if the user has focus on Files panel, no file selected (e.g., empty file list due to filter), but the filter is also empty?

```rust
// Priority 1 fails: selected_file >= state.files.len() (no files match)
// Priority 2 fails: filter.is_empty()
// Priority 3 fails: not in Pending Review
// Result: None
```

This is correctly handled (returns `None`, shows error), but the behavior when `state.files.is_empty()` should be explicitly documented in the context determination table. Currently, it's implicitly covered by "No file selected, no filter" but the distinction between "no selection in non-empty list" vs "empty list" could help debugging.

**Suggested fix:** Add a row to the table in Section 2.2:
```
| Empty file list (after filter) | None | Error: "No files match current filter" |
```

This provides a more specific error message that guides the user to adjust their filter.

---

**ISSUE-R8-002: Semantic Path context algorithm uses incorrect precedence**

Section 2.5 defines Semantic Path context determination:

```rust
fn determine_semantic_path_context(state: &DiscoverState) -> SemanticPathContext {
    // Priority 1: Explicitly selected source
    if state.selected_source < state.sources.len() {
        // ...
    }
    SemanticPathContext::None
}
```

The algorithm only checks `selected_source < sources.len()`. However, in Discover mode:
- Sources dropdown is collapsed by default
- `selected_source` may be 0 (first source) without user explicitly selecting it

Per `specs/views/discover.md` Section 5 (Data Model), `selected_source: usize` defaults to the first source. This means Semantic Path wizard would always have context as long as at least one source exists, even if the user hasn't consciously selected one.

**Question:** Is this intentional (always use current/default source) or should there be a guard like `sources_dropdown_interacted` or similar?

**Suggested fix:** Clarify intent. If always-available is desired, document it explicitly:
> "Semantic Path always uses the currently selected source. Since a source is always selected when sources exist, this wizard is always available in Discover mode with at least one source."

If explicit selection is required, add a guard or require the user to have opened the Sources dropdown at least once in the session.

---

**ISSUE-R8-003: Focus restoration after wizard close is incomplete**

Section 3.2 specifies focus restoration:

```rust
WizardOutcome::Canceled | WizardOutcome::Error => {
    app.discover.focus = app.wizard_previous_focus.take()
        .unwrap_or(DiscoverFocus::Files);
}
```

This handles return to the previous panel focus, but does not restore:
1. `selected_file` index (if file list changed during wizard)
2. Scroll position within the Files panel
3. `preview_tag` / `preview_source` if a dropdown was in preview state

Per `specs/views/discover.md` Section 4.2, dropdowns have "Preview vs Selection" (two-stage). If the wizard was invoked while a dropdown was in preview state (navigating but not confirmed), the preview state would be lost.

**Suggested fix:** The `WizardDialogState` struct (Section 3.3) should include:
```rust
pub struct WizardDialogState {
    // ... existing fields ...

    // Scroll/selection state preservation
    pub previous_selected_file: usize,
    pub previous_scroll_offset: usize,

    // Dropdown preview state (if any)
    pub sources_dropdown_was_open: bool,
    pub tags_dropdown_was_open: bool,
    pub preview_source_idx: Option<usize>,
    pub preview_tag_idx: Option<usize>,
}
```

---

### Medium Priority

**ISSUE-R8-004: Entry point precedence conflicts with keybinding behavior**

Section 6.2 defines entry point precedence:

```
1. Pending Review panel (if open) - uses panel's selected item
2. Wizard Menu (W) - uses current selection in Discover
3. Direct shortcut (w, g, l, S) - uses current selection
```

However, the keybinding table in Section 8.1 shows:

```
| w | Files panel | File selected | Launch Pathfinder if context valid |
| w | Pending Review | Unmatched Paths focused | Launch Pathfinder for selected paths |
```

If the user is in Pending Review with Unmatched Paths focused and presses `w`, precedence rule #1 says "use panel's selected item." But what if Pending Review is open but a different category is focused (e.g., "Parser Warnings")? Does `w` still launch Pathfinder using... what context?

The keybinding table implies context-specific behavior (only launches from Unmatched Paths), but precedence rule #1 implies Pending Review always wins.

**Suggested fix:** Clarify that keybinding context guards AND precedence rules work together:
> "Precedence determines WHICH context to use. Keybinding guards determine IF the wizard can launch. If Pending Review is open but the focused category doesn't provide valid context for the wizard, the wizard shows an error."

---

**ISSUE-R8-005: Wizard Menu keybindings conflict with direct shortcuts**

Section 8.1 shows:

| Key | Context | Action |
|-----|---------|--------|
| `g` | Files panel | Launch Parser Lab if context valid |
| `g` | Wizard menu | Launch Parser Lab |

This is correct, but the state machine (Section 7) shows:

```
WIZARD_MENU ──────────────────────┤
   p/g/l/s                        │
```

Within the Wizard Menu, pressing `g` selects Parser Lab. But per `specs/ai_wizards.md` Section 5.6.1, the Wizard Menu is a modal overlay. When Wizard Menu is open, all Discover keybindings are suspended.

**Question:** Is the `g` key within Wizard Menu supposed to respect the same context guards as the direct `g` shortcut?

Looking at Section 5.2 in the Wizard Menu rendering:

```rust
MenuItem {
    key: 'g',
    label: "Parser Lab (Generator)",
    enabled: parser_lab_ctx.is_some(),
    // ...
}
```

This suggests the guard IS applied within the menu. But Section 8.1 states:

```
| g | Wizard menu | Parser Lab enabled | Launch Parser Lab |
```

The "Parser Lab enabled" guard is not the same as "Parser Lab context valid." The menu item could be enabled but context determination could still fail (race condition if file list changes during menu display).

**Suggested fix:** Add note that context is re-checked at launch time:
> "Menu items show availability based on context at menu open time. Context is re-validated when the wizard actually launches. If context becomes invalid (e.g., file list refreshed), an error toast is shown."

---

**ISSUE-R8-006: Missing error case for "No headers detected" in Labeling**

Section 4.1 (Error Message Table) includes:

| Labeling | No headers detected | "Cannot label files without headers" | - |

But the recovery action is blank (`-`). All other errors have recovery actions. Users need guidance on what to do.

**Suggested fix:** Add recovery guidance:
```
| Labeling | No headers detected | "Cannot label files without headers" | Select CSV/tabular file, or use Parser Lab first |
```

---

### Low Priority / Nits

**ISSUE-R8-007: Status bar wizard hint line is verbose**

Section 5.1 shows status bar with wizard availability:

```
Wizards: [W]menu [w]Pathfinder [g]Parser [l]Label [S]Semantic
```

This is 59 characters. Combined with the main keybindings line:
```
[1]Sources [2]Tags [3]Files [n]Rule [R]Rules [M]Manage
```

That's 56 characters. Total: 115 characters minimum width required.

Per `scripts/tui-debug.sh`, the TUI test size is 120x40. This leaves only 5 characters margin.

**Suggested fix:** Consider abbreviating:
```
Wizards: [W]menu [w]Path [g]Parse [l]Label [S]Semantic
```
Or move to a separate line that only appears when wizards are relevant.

---

**ISSUE-R8-008: Example 10.4 references "23 unmatched files selected"**

```
User state:
  - Pending Review panel open (!)
  - Unmatched Paths section focused
  - 23 unmatched files selected
```

But per `specs/views/discover.md` Section 8.9, Pending Review shows items in a list. The spec doesn't define multi-select in Pending Review. Is this bulk selection a new concept?

**Question:** Does Pending Review support multi-select? If so, define the selection model. If not, example should say "23 unmatched files in category" (not "selected").

---

**ISSUE-R8-009: Data structures use `Box<dyn WizardState>`**

Section 3.3:
```rust
pub wizard_state: Box<dyn WizardState>,
```

This implies a trait object pattern, but `WizardState` trait is not defined in the proposal. This is acceptable as an implementation detail, but for spec completeness, either:
1. Define the `WizardState` trait, or
2. Use a concrete enum: `pub wizard_state: WizardStateKind`

Not blocking, but implementers will need to make this decision.

---

**ISSUE-R8-010: Keybinding table shows `S` as global but `s` in Wizard Menu**

Section 8.1:
```
| S | Global (Discover) | Source selected | Launch Semantic Path if context valid |
```

Section 5.6 in `specs/ai_wizards.md`:
```
| s | Wizard menu | Semantic Path enabled | Launch Semantic Path |
```

The case difference (`S` vs `s`) is intentional but worth noting explicitly:
> "Direct shortcut uses uppercase `S` (Shift+S). Wizard Menu uses lowercase `s`. This follows the pattern where uppercase indicates a 'global' action."

---

### Integration Assessment

**With specs/ai_wizards.md Section 5.6:**

The proposal correctly extends Section 5.6 with subsections 5.6.2-5.6.6. The state machine diagram (Section 7) integrates cleanly with the existing 5.6.1 Wizard Menu State Machine. The keybinding table (Section 8.1) is consistent with the existing keybindings in ai_wizards.md.

**With specs/views/discover.md:**

The proposal correctly references:
- Files panel keybindings (Section 6.4)
- `DiscoverFocus` enum (Section 5)
- Pending Review panel (Section 8.9)

The suggested cross-reference note (Section 9.2) is appropriate and follows the existing pattern.

---

### Entry Point Inventory Completeness

Section 6.1 provides comprehensive entry points for all four wizards. The "Future Entry Points" section (6.3) appropriately defers:
- Right-click context menu (Phase 3)
- Command palette (Phase 3)
- MCP tool `invoke_wizard` (Phase 2)
- CLI `casparian wizard pathfinder <path>` (Phase 2)

This is consistent with the implementation phasing in `specs/ai_wizards.md` Section 11.

**One missing entry point:** The Sources Manager (`M` key) is listed for Semantic Path, but the Files panel detailed view (accessed via `Enter` on a file) could also be an entry point for Pathfinder/Parser Lab. This may be intentional exclusion for v1.0.

---

### Verdict

**ACCEPT_WITH_MINOR**

### Recommendation

The proposal provides a thorough and implementable specification for wizard invocation from the Files panel. The context determination algorithms are well-defined with clear priority ordering. The focus management and error handling approaches are consistent with TUI conventions.

**Required before implementation:**
1. Clarify Semantic Path "always available" vs "explicit selection required" (ISSUE-R8-002)
2. Add error guidance for "No headers detected" case (ISSUE-R8-006)

**Recommended (non-blocking):**
3. Add "empty file list" error message variant (ISSUE-R8-001)
4. Consider fuller state preservation during wizard (ISSUE-R8-003)
5. Clarify precedence vs keybinding guard interaction (ISSUE-R8-004)
6. Add re-validation note for Wizard Menu context (ISSUE-R8-005)

The proposal can proceed to implementation after addressing ISSUE-R8-002 and ISSUE-R8-006. Other issues can be addressed during implementation or in a follow-up revision.

---

**Minor revision requested before implementation.**

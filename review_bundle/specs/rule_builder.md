# Rule Builder - TUI Spec

**Status:** Redesign proposal
**Version:** 4.2
**Date:** 2026-01-15
**Related:** specs/extraction.md
**Last Updated:** 2026-01-18

> **Note:** This is the canonical spec for Discover mode and Rule Builder.
> Previous: `specs/views/discover.md` (archived as `archive/specs/views/discover_v3_archived.md`).

---

## 1. Overview

The **Rule Builder** is the Discover mode workbench for **Scope → Compose → Validate**.
It uses the global **Audit-First Shell** and centers on a step band with a
tabbed results panel and persistent inspector.

It subsumes the previous Files view; preview, backtest, and tagging live in the
Validate panel.

### 1.1 Current Behavior Summary

- Rule Builder is the default Discover view.
- No source is required to enter Rule Builder; it prompts on actions instead.
- Source dropdown opens with `[1]` and applies selection on `Enter` (no live reload).
- Tags dropdown opens with `[2]` and previews tags while open.
- Rule saving (`Ctrl+S`) persists to DB (scoped to selected source).

---

## 2. Layout

```
┌─ Casparian Flow | Mode: Dev | Contract: DRAFT | Scan: 12,410 files ───────┐
├─ Rail ───────────┬─ Rule Builder (Scope → Compose → Validate) ─┬─ Inspector┤
│ [0] Home         │ [Scope] [Pattern] [Extract] [Validate]      │ Rule: fx   │
│ [1] Discover     │                                            │ Source: ix │
│ [2] Parser Bench │ PATTERN       EXCLUDES     TAG             │ Matches: 82│
│ [3] Jobs         │ EXTRACTIONS   OPTIONS      SUGGESTIONS     │ Dirty: yes │
│ [4] Sources      │                                            │            │
│ Source: <name> ▾ │ VALIDATE: [Preview] [Backtest] [Coverage]   │            │
│ Tags: All ▾      │ ▸ file/path/...  id=123  date=2026-01-12    │            │
│ Rules: <rule> ▾  │   file/path/...  id=124  date=2026-01-12    │            │
├──────────────────┴────────────────────────────────────────────┴───────────┤
│ [s] Scan  [Ctrl+S] Save  [b] Backtest  [t] Tag  [Tab] Next  [I] Inspector │
└──────────────────────────────────────────────────────────────────────────┘
```

**Split ratio:** 40% left (Rule Builder) / 60% right (File Results)

---

## 3. Keybindings (Current)

### 3.1 Global in Discover

| Key | Action | Notes |
|-----|--------|-------|
| `1` | Open Sources dropdown | Applies on `Enter` |
| `2` | Open Tags dropdown | Previews tags while open |
| `3` | Focus Files panel | Rule Builder only |
| `M` | Sources Manager dialog | From Rule Builder |
| `R` | Rules dropdown | Opens rules list overlay (CRUD) |
| `s` | Scan new directory | Only when not in text input |
| `Esc` | Back to Home | When focus is FileList |
| `I` | Toggle inspector | Collapse/expand details |

### 3.2 Focus and Editing

| Key | Action | Notes |
|-----|--------|-------|
| `Tab` / `Shift+Tab` | Cycle focus | Pattern -> Excludes -> Tag -> Extractions -> Options -> FileList |
| `[` / `]` | Jump focus | Left panel / FileList |
| `Enter` | Contextual | Expand folder in FileList, accept exclude input, etc. |
| `Space` | Toggle selection | FileList only (preview/results) |
| `t` | Apply tag | Selection-aware; prompts if none selected |
| `b` | Backtest | From preview to results |
| `Ctrl+S` | Save rule | Persists to DB |
| `Ctrl+N` | Clear form | Resets to new rule |

### 3.3 Dropdowns

| Key | Action | Notes |
|-----|--------|-------|
| `/` | Filter list | Filter input mode |
| `Enter` | Select item | Sources applies to state |
| `Esc` | Close dropdown | One press closes, even in filter mode |

---

## 4. Interaction Rules

### 4.1 No Source Selected

- Rule Builder stays visible.
- Pressing `[1]` opens Sources dropdown.
- Pressing `[s]` opens scan dialog.
- Other actions surface a status hint: "Select a source before building rules".

### 4.2 Tags Preview

- Tags dropdown uses `preview_tag` while open.
- Closing the dropdown applies the chosen tag filter.

### 4.3 Sources Selection

- Sources dropdown does **not** live preview file results.
- Selection is applied only on `Enter`.

### 4.4 Rules CRUD (Interaction Spec)

**Scope:** Rules are scoped to the currently selected source. The rules list is
filtered to that source only.

**Entry Points:**
- `R` opens the Rules dropdown overlay from Rule Builder.
- Rules are also accessible from Files view via `R` (same overlay).

**Rules Dropdown (Overlay)**
- List shows: pattern, tag, enabled state, priority (compact), and last updated.
- `/` starts filter mode (searches pattern + tag + name).
- `Enter` opens the selected rule in Rule Builder (edit mode).
- `d` prompts delete confirmation for the selected rule.
- `Space` toggles enabled/disabled.
- `Esc` closes overlay.

**Edit Flow**
- Selecting a rule loads it into Rule Builder with `editing_rule_id` set.
- Saving (`Ctrl+S`) updates the existing rule row instead of creating a new one.
- Rule Builder header shows an inline "Editing: <rule name>" hint while active.

**Delete Flow**
- `d` opens a confirm dialog: "Delete rule '<name>'? [y/N]".
- On confirm, rule is removed from DB and from the overlay list.

**Create Flow (Existing)**
- Build a rule in Rule Builder and `Ctrl+S` to persist.
- New rule appears at top of the Rules dropdown (sorted by priority, then name).

**Non-Goals (V1)**
- No global rule list across sources.
- No bulk edit or priority reordering in the overlay (future enhancement).

### 4.5 Manual Tagging (Persistent)

- `t` applies the current tag to the selected preview results (or prompts to apply to all).
- Tags are persisted to `scout_files.tag` on the next tick.
- This action does **not** create or update a rule; it only tags the selected files.

---

## 5. Notes / Planned

- Rules are persisted to `scout_tagging_rules` with unique `id`.
- Validate tabs (Preview/Backtest/Coverage) are planned; Preview/Backtest are
  already implemented, Coverage is a stub.
- **Implementation gap:** Glob Explorer publish flow does not persist extraction rules
  or enqueue jobs yet; it transitions directly to a Published screen.

---

## 6. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-18 | 4.2 | Redesign: audit-first shell + step band layout |
| 2026-01-15 | 4.1 | Updated to match current Rule Builder implementation |

# Rule Builder - TUI Spec

**Status:** Updated for current implementation
**Version:** 4.1
**Date:** 2026-01-15
**Related:** specs/extraction.md
**Last Updated:** 2026-01-15

> **Note:** This is the canonical spec for Discover mode and Rule Builder.
> Previous: `specs/views/discover.md` (archived as `discover_v3_archived.md`).

---

## 1. Overview

The **Rule Builder** is the Discover mode UI for scanning sources, tagging files,
and drafting rules. It uses a split layout with a left rule editor and a right
file results panel.

### 1.1 Current Behavior Summary

- Rule Builder is the default Discover view.
- No source is required to enter Rule Builder; it prompts on actions instead.
- Source dropdown opens with `[1]` and applies selection on `Enter` (no live reload).
- Tags dropdown opens with `[2]` and previews tags while open.
- Rule saving (`Ctrl+S`) is a stub and does not persist to DB.

---

## 2. Layout

```
┌ Rule Builder - [1] Source: <name> ▾  [2] Tags: All ▾ ─────────────────────┐
├─────────────────────────────────────────┬────────────────────────────────┤
│ PATTERN                                 │ FILE RESULTS                   │
│ EXCLUDES                                │                                │
│ TAG                                     │                                │
│ EXTRACTIONS                             │                                │
│ OPTIONS                                 │                                │
│ (schema suggestions)                    │                                │
├─────────────────────────────────────────┴────────────────────────────────┤
│ [e] Sample  [E] Full  [s] Scan  [Tab] Nav  [Ctrl+S] Save  [Esc] Back       │
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
| `R` | Rules Manager dialog | From Rule Builder |
| `s` | Scan new directory | Only when not in text input |
| `Esc` | Back to Home | When focus is FileList |

### 3.2 Focus and Editing

| Key | Action | Notes |
|-----|--------|-------|
| `Tab` / `Shift+Tab` | Cycle focus | Pattern -> Excludes -> Tag -> Extractions -> Options -> FileList |
| `Enter` | Contextual | Expand folder in FileList, accept exclude input, etc. |
| `Ctrl+S` | Save rule | Stub only (no persistence) |

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

---

## 5. Notes / Planned

- Persisting rules to the database is not implemented.
- Schema suggestions are present but not fully wired to backtest execution.

---

## 6. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-15 | 4.1 | Updated to match current Rule Builder implementation |

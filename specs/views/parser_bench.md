# Parser Bench - TUI View Spec

**Status:** Partial implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.3
**Related:** specs/views/jobs.md, specs/rule_builder.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Parser Bench** view is a filesystem-driven list of parser files with a details panel.
It supports discovery, basic metadata display, and filtering. Test execution is not yet wired.

### 1.1 Parsers Directory

```
~/.casparian_flow/
├── parsers/                    # Parser plugins directory
│   ├── sales_parser.py
│   └── invoice_parser.py
└── casparian_flow.duckdb
```

**Rules (current):**
- Only `.py` files directly under `parsers/` are discovered.
- Broken symlinks are shown as error state.
- Files are sorted by name.

---

## 2. Layout

```
┌─ Parser Bench ───────────────────────────────────────────────────────────┐
│ Parsers (~/.casparian_flow/parsers)   | Details                          │
│ /filter_text                          | Name, version, topics, path      │
│ ▸ ● parser_a            v1.0.0        | Health + modified time           │
│   ✗ parser_b            —             | Bound files (if any)             │
├──────────────────────────────────────────────────────────────────────────┤
│ [↑/↓] Navigate  [/] Filter  [r] Refresh  [Esc] Back  [0] Home             │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Metadata Discovery (Implemented)

Metadata extraction is done via a Python subprocess that parses AST in batch mode.
This is implemented in `crates/casparian/src/cli/tui/app.rs`.

- Batch size: 50 paths per subprocess
- Fallback name: filename without extension
- Version: `—` if missing
- Topics: empty list if missing

---

## 4. Keybindings (Current)

| Key | Action | Notes |
|-----|--------|-------|
| `↑/↓` | Navigate parser list | Honors filter |
| `/` | Start filter input | Filter is local to list |
| `r` | Refresh list | Reloads parser files |
| `Esc` | Back to Home | Clears filter first if present |
| `0` / `H` | Back to Home | Global |

**Not implemented yet:** parser testing (`t`), quick test (`n`), backtest (`b`), watcher (`w`).

---

## 5. Notes / Planned

- Testing flow, quick test picker, and results views are planned but not wired.
- Health metrics are currently derived from file status (broken symlink vs unknown).
- Filtering is implemented on name and path.

---

## 6. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.3 | Updated to match current Parser Bench implementation |

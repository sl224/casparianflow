# Home - TUI View Spec

**Status:** Updated for current implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.1
**Related:** specs/rule_builder.md, specs/views/jobs.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Home** view is a lightweight hub with two panels:
- **Quick Start**: a selectable list of sources for fast scanning.
- **Recent Activity**: a compact summary of recent job statuses.

It is intentionally minimal: no modal popups on entry, and no tile-based navigation.

### 1.1 Data Sources

```
~/.casparian_flow/casparian_flow.sqlite3

Tables queried:
├── scout_sources        # Source list + file_count
├── scout_files          # File count (stats)
├── cf_processing_queue  # Recent jobs + status counts
└── cf_parsers           # Parser count (stats)
```

### 1.2 User Goals (Current)

| Goal | How Home Helps |
|------|-----------------|
| "Scan a source quickly" | Select a source and press Enter |
| "See job status" | Recent Activity panel shows running/pending/failed |
| "Jump to a view" | Global keys 1-4, J/S drawers, ? help |

---

## 2. Layout

```
┌─ Home ───────────────────────────────────────────────────────────────────┐
│ Quick Start: Scan a Source                                               │
│  ▸ source_a (120 files)                                                  │
│    source_b (42 files)                                                   │
│                                                                          │
│ Recent Activity                                                          │
│  2 running, 0 pending, 1 failed                                          │
│  [PARSE] parser_x failed                                                 │
│  [PARSE] parser_y running                                                │
│                                                                          │
│ Tip: Press [J] to open Jobs drawer                                       │
├──────────────────────────────────────────────────────────────────────────┤
│ [↑↓] Navigate  [Enter] Scan  [/] Filter  [s] New Scan  [1-4] Views  [?]   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Quick Start Panel

- Lists configured sources (name + file_count).
- Selection is tracked separately from Discover mode.
- Empty state text: "No sources configured. Press [s] to scan a folder."

### 2.2 Recent Activity Panel

- Displays aggregated job counts and the most recent jobs.
- Backed by `cf_processing_queue` in the current implementation.
- If no jobs, shows: "No recent jobs."

---

## 3. Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `↑/↓` | Move selection | Quick Start list |
| `Enter` | Scan selected source | Quick Start list |
| `/` | Filter sources | Quick Start list |
| `s` | Add/scan new folder | Home |
| `1-4` | Switch view | Global |
| `J` | Jobs drawer | Global |
| `S` | Sources drawer | Global |
| `?` | Help | Global |
| `0` / `H` | Return Home | Global |

---

## 4. Notes / Planned

- Tile-based layout, quick test, and recent files panels are not implemented.
- First-time welcome banner is not implemented.
- Home does not currently offer direct navigation to Parser Bench or Jobs via tiles; use global keys instead.

---

## 5. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.1 | Updated to match current Home view implementation |

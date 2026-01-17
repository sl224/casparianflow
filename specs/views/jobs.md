# Jobs View - Specification

**Status:** Partial implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 2.1
**Related:** specs/rule_builder.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-14

---

## 1. Overview

The **Jobs** view shows processing activity sourced from `cf_processing_queue`.
Current rendering supports a list, a detail panel, a monitoring panel stub, and
filter/log panels without full data wiring.

### 1.1 Data Source

```
~/.casparian_flow/casparian_flow.sqlite3

Table queried:
└── cf_processing_queue
```

**Current mapping:**
- All jobs are displayed as `PARSE` type.
- `error_message` becomes a failure entry.
- `result_summary` is used to mark completion.

---

## 2. Layout (Current)

```
┌─ JOBS ────────────────────────────────────────────────────────────────┐
│ ↻ 1 running   ✓ 0 done   ✗ 1 failed   0/0 files • 0 B output           │
│                                                                       │
│▸ ✗ PARSE   parser_x                                  2m ago           │
│           1 files failed • <error message>                          │
│           First failure: <error message or file>                     │
│                                                                       │
│  ↻ PARSE   parser_y                                  5m ago           │
│           1/10 files • ETA 3m                                         │
├───────────────────────────────────────────────────────────────────────┤
│ [↑/↓] Navigate  [Enter] Details  [P] Pipeline  [m] Monitor  [f] Filter │
└───────────────────────────────────────────────────────────────────────┘
```

---

## 3. Keybindings (Current)

| Key | Action | Notes |
|-----|--------|-------|
| `↑/↓` | Navigate jobs | List view |
| `Enter` | Detail panel | List view |
| `P` | Toggle pipeline | Layout only |
| `m` | Monitoring panel | Metrics not wired |
| `f` | Filter dialog | Dialog is placeholder |
| `g` / `G` | First / last job | List view |
| `r` / `R` | Retry failed | Placeholder |
| `c` | Cancel running | Placeholder |
| `x` | Clear completed | Removes from list | 
| `L` | Log viewer | Placeholder |
| `y` | Copy output path | Placeholder |
| `Esc` | Back | Returns to previous mode if set |

---

## 4. Notes / Planned

- Backtest/export/scan job types are not wired in the current Jobs data path.
- Pipeline and monitoring panels render, but metrics are not populated.
- Filter dialog does not apply any filters yet.

---

## 5. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 2.1 | Updated to match current Jobs implementation |

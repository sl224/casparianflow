# Jobs View - Specification

**Status:** Redesign proposal
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 2.5
**Related:** specs/rule_builder.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-18

---

## 1. Overview

The **Jobs** view is **output-first**: it tells users what is ready to query,
what is running, and what needs action. It uses the **Audit-First Shell** with
a persistent inspector panel for details.

Users should answer within seconds:
- Is my data ready to query?
- What is running or blocked, and what is impacted?
- What failed and what should I do next?

### 1.1 Data Source

```
~/.casparian_flow/casparian_flow.duckdb

Table queried:
└── cf_processing_queue
```

**Current mapping:**
- Jobs use `JobInfo` fields from the TUI state (see Section 5).
- If only parse jobs are wired, they still render in ACTIONABLE/READY buckets.
- `failures` render as the failure summary and detail in the ACTIONABLE section.

---

## 2. Layout (Proposed)

```
┌─ Casparian Flow | View: Jobs | DB: DuckDB | Run: 2 | Fail: 1 | Quarantine: 14 ─┐
├─ Rail ───────────┬─ Jobs (Output-First) ───────────────┬─ Inspector ────────────┤
│ [0] Home         │ READY OUTPUTS                      │ Job #1842  [FAIL]       │
│ [1] Discover     │ ▸ [READY] hl7_oru_obs 1.2M rows     │ Parser: finance_ap v2.1│
│ [2] Parser Bench │   [READY] fix_trades 420k rows      │ Output: /data/...      │
│ [3] Jobs         │                                    │ Contract: strict       │
│ [4] Sources      │ ACTIONABLE                         │ Quarantine: 14 rows    │
│                  │   [RUN]  finance_ap  62% ETA 5m     │ Failure: row 9912      │
│                  │   [FAIL] hl7_daily   2m ago         │ Logs: /logs/1842.log   │
│                  │   [QUEUE] citi_backfill (5 files)   │                        │
├──────────────────┴────────────────────────────────────┴──────────────────────────┤
│ [Tab] Section  [Enter] Pin  [R] Retry  [c] Cancel  [L] Logs  [I] Inspector  [?]   │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Keybindings (Proposed)

| Key | Action | Notes |
|-----|--------|-------|
| `↑/↓` | Navigate jobs | List view |
| `Tab` | Switch section | Cycles ACTIONABLE → READY |
| `Enter` | Pin details | Detail panel reflects selected job |
| `f` | Filter dialog | Filter by status/type/name |
| `g` / `G` | First / last job | List view |
| `r` / `R` | Retry failed | Failed jobs only |
| `c` | Cancel running | Active jobs only |
| `x` | Clear completed | Removes READY rows |
| `L` | Log viewer | Selected job |
| `y` | Copy output path | Selected job |
| `O` | Open output | Selected job if output_path present |
| `I` | Toggle inspector | Collapse/expand details |
| `Esc` | Back | Returns to previous mode if set |

---

## 4. Notes / Planned

- READY/ACTIONABLE are derived from `JobStatus`:
  - READY: `Completed`, `PartialSuccess` (`CompletedWithWarnings` compat alias)
  - ACTIONABLE: `Pending`, `Running`, `Failed`, `Cancelled`
- Inspector is always visible unless toggled with `I`.
- Schema contract status, quarantine count, and logical date are shown when
  available from job metadata (may be blank for V1).
- Offline indicator is shown when network access is unavailable or disabled.
- Phase line uses fixed topology: Context → Parse → Export (no DAG graph).
- **Top bar alignment:** Current UI shows View/DB/Run/Fail/Quarantine totals. If Mode/Contract
  should be surfaced, add them without dropping operational totals.

---

## 5. Field Mapping (TUI JobInfo → UI)

| UI Element | JobInfo Field(s) | Notes |
|-----------|------------------|-------|
| Job ID | `id` | Shown in details |
| Type | `job_type.as_str()` | SCAN/PARSE/BACKTEST/SCHEMA |
| Name | `name` | Parser/source name |
| Version | `version` | Parser version (optional) |
| Status | `status` | Drives section + symbols |
| Time | `started_at`, `completed_at` | Relative age |
| Progress | `items_processed`, `items_total` | Percent + counts |
| Failed count | `items_failed` | Failed summary |
| Logical date | `logical_date` | Pipeline runs only |
| Snapshot hash | `selection_snapshot_hash` | Pipeline runs only |
| Quarantine | `quarantine_rows` | Parsed output metadata |
| Output path | `output_path` | Copy/open |
| Output size | `output_size_bytes` | Summary header |
| Failure detail | `failures` | First failure shows inline |

---

## 6. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-18 | 2.5 | Redesign: output-first shell + inspector layout |
| 2026-01-18 | 2.3 | Merge active/failed into ACTIONABLE + add output-first details focus |
| 2026-01-17 | 2.2 | Jobs view reframed around READY/ACTIVE/FAILED + output-first details |

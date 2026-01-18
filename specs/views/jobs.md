# Jobs View - Specification

**Status:** Partial implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 2.4
**Related:** specs/rule_builder.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-18

---

## 1. Overview

The **Jobs** view shows processing activity sourced from `cf_processing_queue`,
prioritizing **actionable work** and **output readiness** over raw job history.
Users should be able to answer, within seconds:
- Is my data ready to query?
- If not, what is running or blocked, and why?
- What failed, what is affected, and what should I do next?
- How fast is the batch moving and what are the stragglers?

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
┌─ JOBS ───────────────────────────────────────────────────────────────────────────────┐
│ READY: 22  •  ACTIVE: 3  •  FAILED: 1  •  Queue: 5  •  420 f/m  • ETA 05:10        │
├─ ACTIONABLE (running, queued, or failed) ───────────────────────────────────────────┤
│ ▶ PARSE  hl7_oru_daily   62%  2.3k/3.7k  schema ✓  quarantine 14                     │
│   Phase: Context ✓  Parse 62%  Export —   Stragglers: file_9912.hl7 (8m)             │
│ ✗ PARSE  finance_ap_aging   2m ago   14 failed • timeout at row 9912               │
├─ READY (outputs you can query now) ─────────────────────────────────────────────────┤
│ ✓ PARSE  hl7_oru_daily   5m ago   tables: hl7_oru_obs (1.2M rows)                  │
│ ✓ EXPORT hl7_oru_fhir    5m ago   files: 42 bundles                                 │
├─ DETAILS ───────────────────────────────────────────────────────────────────────────┤
│ Job #1842  PARSE  finance_ap_aging   Failed   parser v2.1.4                         │
│ Pipeline: hl7_oru_daily  Logical date: 2026-01-18                                   │
│ Snapshot: 9b3f...a12e  Contract: strict  Quarantine: 14 rows                        │
│ Output: /data/casparian/outputs/finance_ap_aging/                                    │
│ Error: delimiter mismatch at row 9912                                                │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ [↑/↓] Select  [Tab] Actionable/Ready  [Enter] Pin  [f] Filter  [R] Retry            │
│ [c] Cancel  [L] Logs  [y] Copy output  [O] Open output  [Esc] Back  [0] Home        │
└─────────────────────────────────────────────────────────────────────────────────────┘
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
| `Esc` | Back | Returns to previous mode if set |

---

## 4. Notes / Planned

- ACTIONABLE/READY are derived from `JobStatus`:
  - READY: `Completed`, `CompletedWithWarnings`
  - ACTIONABLE: `Pending`, `Running`, `Failed`, `Cancelled`
- Detail panel should always be visible; if vertical space is tight, collapse
  READY list to the latest N entries.
- Schema contract status, quarantine count, and logical date are shown when
  available from job metadata (may be blank for V1).
- Offline indicator is shown when network access is unavailable or disabled.
- Phase line uses fixed topology: Context → Parse → Export (no DAG graph).

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
| 2026-01-18 | 2.3 | Merge active/failed into ACTIONABLE + add output-first details focus |
| 2026-01-17 | 2.2 | Jobs view reframed around READY/ACTIVE/FAILED + output-first details |

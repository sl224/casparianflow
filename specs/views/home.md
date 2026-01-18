# Home - TUI View Spec

**Status:** Redesign proposal
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.2
**Related:** specs/rule_builder.md, specs/views/jobs.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-18

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Home** view is a **Readiness Board** for output-first triage:
- **Ready Outputs**: datasets you can query now.
- **Active Runs**: running/backlogged work with ETA and stragglers.
- **Warnings**: quarantine counts and last failures.

It uses the global **Audit-First Shell** (Top Bar + Rail + Main + Inspector + Action Bar).

### 1.1 Data Sources

```
~/.casparian_flow/casparian_flow.duckdb

Tables queried:
├── scout_sources        # Source list + file_count
├── scout_files          # File count (stats)
├── cf_processing_queue  # Recent jobs + status counts
└── cf_parsers           # Parser count (stats)
```

### 1.2 User Goals (Current)

| Goal | How Home Helps |
|------|-----------------|
| "See what is ready to query" | Ready Outputs list is first |
| "See what is running and risky" | Active Runs + Warnings sections |
| "Scan a new source quickly" | [s] Scan from the action bar |
| "Jump to a view" | Global keys 1-4 and Rail |

---

## 2. Layout

```
┌─ Casparian Flow | Mode: Dev | Contract: STRICT | Quarantine: 14 ─────────┐
├─ Rail ───────────┬─ Readiness Board ─────────────────┬─ Inspector ───────┤
│ [0] Home         │ READY OUTPUTS                     │ Output: hl7_oru    │
│ [1] Discover     │ ▸ [READY] hl7_oru_obs  1.2M rows  │ Path: /data/...    │
│ [2] Parser Bench │   [READY] fix_trades   420k rows  │ Tables: 3          │
│ [3] Jobs         │                                  │ Contract: strict   │
│ [4] Sources      │ ACTIVE RUNS                      │ Quarantine: 14     │
│                  │   [RUN]  finance_ap  62%  ETA 5m │ Last run: 5m ago    │
│ Sources (MRU)    │   [RUN]  hl7_daily    4%  ETA 1h │ Errors: none        │
│ ▸ inbox/         │                                  │                    │
│   trades/        │ WARNINGS                          │                    │
│                  │   [QUAR] hl7_oru_obs 14 rows     │                    │
├──────────────────┴───────────────────────────────────┴────────────────────┤
│ [Enter] Open  [s] Scan  [r] Refresh  [I] Inspector  [?] Help             │
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
| `↑/↓` | Move selection | Main lists (Ready/Active/Warnings) |
| `Tab` | Cycle focus | Rail → Main → Inspector |
| `Enter` | Open selected item | Main lists |
| `s` | Scan new folder | Home |
| `r` | Refresh | Home |
| `I` | Toggle inspector | Home |
| `1-4` | Switch view | Global |
| `?` | Help | Global |
| `0` / `H` | Return Home | Global |

---

## 4. Notes / Planned

- Ready Outputs uses `cf_processing_queue` (Completed/Warnings) and output metadata.
- Active Runs shows ETA + straggler when available.
- Warnings section surfaces quarantine counts and last failure per output.

---

## 5. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-18 | 1.2 | Redesign: output-first Readiness Board layout |
| 2026-01-14 | 1.1 | Updated to match current Home view implementation |

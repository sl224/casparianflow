---
title: "EVTX to SQL: Building a Repeatable Windows Event Log Timeline (DuckDB + Parquet)"
description: "A practical DFIR workflow: turn a folder of Windows Event Logs (EVTX) into typed tables you can query locally. The key is reproducibility: per-row lineage, quarantine semantics, and deterministic reruns."
pubDate: 2026-01-24
---

Windows Event Logs (EVTX) are one of the richest sources of forensic truth—and one of the easiest ways to get lost.

A typical workflow still looks like:

- export or parse EVTX into CSV
- load into Excel or a SIEM export view
- hope the parsing didn’t silently drop records
- struggle to reproduce results later

In DFIR, that’s not good enough. You often need:

- **repeatability** (same inputs → same outputs)
- **provenance** (trace each event to the source log file)
- **safe failure modes** (corrupted records shouldn’t destroy the whole run)

This tutorial walks through a local-first workflow using Casparian Flow to produce evidence-grade SQL tables from EVTX.

> **Note:** EVTX is the flagship v1 DFIR parser target for Casparian Flow. If you’re evaluating with another artifact type today, the workflow is the same: scan → run parser → query outputs + quarantine.

---

## 0) Prereqs

You’ll need:

- a directory containing `.evtx` files (case folder, triage collection, etc.)
- Casparian Flow installed locally
- DuckDB installed (optional but recommended)

---

## 1) Scan the Case Folder (Discovery + Hashing)

First, point Casparian at the directory to build a file catalog.

```bash
casparian scan /cases/ACME-incident-2026-01 --tag evtx
```

What this step gives you:

- a list of discovered artifacts
- stable identifiers via content hashing
- tags you can use for routing (no hardcoded folder assumptions)

---

## 2) Run the EVTX Parser to DuckDB (Bronze as Tables)

Now run the EVTX parser and write outputs to a local DuckDB database:

```bash
casparian run parsers/evtx/evtx_parser.py --tag evtx --sink duckdb://./acme_case.duckdb
```

A typical parser emits at least two outputs:

- `evtx_events` — clean, typed events table
- `evtx_events_quarantine` — rows that violated the schema contract + error context

Casparian will also inject per-row lineage columns like:

- `_cf_source_hash`
- `_cf_job_id`
- `_cf_processed_at`
- `_cf_parser_version`

Those are what make the outputs defensible.

---

## 3) Query Events in DuckDB

Open DuckDB:

```bash
duckdb ./acme_case.duckdb
```

Start with a sanity check:

```sql
SELECT COUNT(*) AS events FROM evtx_events;
SELECT COUNT(*) AS quarantined FROM evtx_events_quarantine;
```

If quarantine is non-zero, that’s not necessarily a failure—it’s a signal to inspect.

---

## 4) Build a First Timeline Query

A simple timeline query might look like:

```sql
SELECT
  timestamp,
  host,
  channel,
  event_id,
  user,
  message,
  _cf_source_hash,
  _cf_processed_at
FROM evtx_events
ORDER BY timestamp
LIMIT 1000;
```

Even in this first pass, you already have:
- structured events
- traceability
- deterministic run identity

---

## 5) Focus on High-Signal Security Events

Depending on your environment, you might filter by event IDs commonly used in investigations.

Examples include (not exhaustive):

- **4624** — successful logon
- **4625** — failed logon
- **4688** — process creation (when enabled)
- **4697 / 7045** — service installation/creation (depending on channel/logging)
- **1102** — audit log cleared

Here’s a quick filter:

```sql
SELECT
  timestamp,
  host,
  event_id,
  user,
  message,
  _cf_source_hash
FROM evtx_events
WHERE event_id IN (4624, 4625, 4688, 1102)
ORDER BY timestamp;
```

Now you’re doing real DFIR analysis in SQL—not in a CSV viewer.

---

## 6) Trace a Suspicious Event Back to Its Source Log

Let’s say you find a suspicious `1102` (audit log cleared). You can trace it:

```sql
SELECT
  timestamp,
  host,
  event_id,
  _cf_source_hash,
  _cf_job_id,
  _cf_parser_version
FROM evtx_events
WHERE event_id = 1102
ORDER BY timestamp DESC
LIMIT 20;
```

Now you have the `source_hash`. If you keep a file catalog table, you can join to it to find the path of the originating EVTX file.

That’s the difference between “I saw it in the output” and “I can point to the original artifact that produced it.”

---

## 7) Inspect Quarantine (Don’t Ignore It)

Quarantine is where you find:

- corrupted records
- edge cases the parser didn’t anticipate
- schema assumptions that don’t hold in the wild

Start with a breakdown:

```sql
SELECT
  violation_type,
  COUNT(*) AS rows
FROM evtx_events_quarantine
GROUP BY 1
ORDER BY rows DESC;
```

And sample examples:

```sql
SELECT
  _cf_source_hash,
  _cf_row_error
FROM evtx_events_quarantine
LIMIT 50;
```

In DFIR, quarantine is often an investigative lead:
- corrupted logs can be accidental
- or they can be intentional tampering

Either way, you want those records preserved—not silently dropped.

---

## 8) Export to Parquet for Sharing (Optional)

DuckDB is great locally. Parquet is great for portability.

Casparian can write Parquet directly:

```bash
casparian run parsers/evtx/evtx_parser.py --tag evtx --sink parquet://./outputs/
```

Now you can hand the dataset to another analyst, load it into another tool, or archive it with the case.

---

## Why This Workflow Is Different

This isn’t “EVTX parsing with another tool.”

It’s EVTX parsing with the properties DFIR actually needs:

- deterministic identity (source hashes, parser bundle identity)
- per-row lineage (traceability)
- quarantine semantics (no silent loss)
- atomic outputs (no partial artifacts)

In other words: **a trustworthy Bronze layer for forensic artifacts.**

---

## Next in the Series

Next up: constraint-based type inference—why “voting” on types fails in messy corpuses, and why elimination-based inference is safer.

<!-- TODO: update CTA link to your site -->
If you want to run this workflow on a real case folder, reach out for a pilot: `/contact`

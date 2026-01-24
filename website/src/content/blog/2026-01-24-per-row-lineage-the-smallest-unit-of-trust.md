---
title: "Per-Row Lineage: The Smallest Unit of Trust"
description: "Dataset-level lineage is useful. In DFIR and regulated workflows, it isn’t enough. Per-row lineage lets you trace every output record back to a specific source artifact, job, and parser version—so you can actually defend your data."
pubDate: 2026-01-24
---

“Lineage” is one of those words that gets used a lot and implemented very little.

Most tools mean something like:

> “Table A came from job B, which ran yesterday.”

That’s a start. But in high-stakes environments, it doesn’t answer the question people actually ask:

> “This *specific row*—where did it come from?”

In DFIR, a single record might become evidence.  
In pharma, a single measurement might be audited.  
In finance, a single execution record might be reconciled.

**Trust doesn’t live at the dataset level. It lives at the record level.**

That’s why Casparian Flow treats **per-row lineage** as a first-class primitive.

---

## The Problem With “Good Enough” Lineage

Dataset-level lineage breaks down when:

- a job processes many inputs and writes one combined output
- a bug affects only a subset of records
- someone asks you to reproduce results months later
- a single anomalous record needs to be traced and explained

Without row-level provenance, you end up with brittle answers:

- “It probably came from that folder…”
- “We reran it at some point…”
- “The parser might have been updated…”

That’s not defensible.

---

## What Per-Row Lineage Looks Like in Casparian

Casparian injects lineage metadata into every output row using a reserved namespace (`_cf_*`).

A minimal set includes:

- `_cf_source_hash` — content hash of the input artifact
- `_cf_job_id` — stable job/run identity
- `_cf_processed_at` — when this row was produced
- `_cf_parser_version` — content-addressed parser identity/version

That means lineage isn’t “some log file.”

It’s queryable, filterable metadata sitting next to your data.

---

## Why Content Hashing Matters

File paths are not identity.

Paths move. Evidence folders are renamed. Bundles are extracted in different locations.

A **content hash** is stable.

When your lineage points to a `source_hash`, you can prove:

- exactly which bytes were processed
- whether the artifact changed later
- whether two runs used identical inputs

In DFIR workflows, this maps naturally to chain-of-custody thinking: evidence is tracked by immutable identity, not “whatever someone named the folder.”

---

## Tracing a Row Back to Its Source

With per-row lineage, you can answer real questions quickly.

### Example: “Which file produced this event?”

```sql
SELECT
  _cf_source_hash,
  host,
  event_id,
  timestamp
FROM evtx_events
WHERE event_record_id = 1234567;
```

Now you have `_cf_source_hash`.

If you keep a file catalog (Casparian does), you can join back to it:

```sql
SELECT
  e.timestamp,
  e.host,
  e.event_id,
  f.path,
  f.size_bytes
FROM evtx_events e
JOIN scout_files f
  ON e._cf_source_hash = f.source_hash
WHERE e.event_record_id = 1234567;
```

Now you can point to the exact artifact on disk that produced the row.

That’s provenance you can defend.

---

## Lineage + Quarantine = A Safe Debugging Loop

Per-row lineage becomes even more powerful when combined with quarantine semantics.

When a row violates a schema contract, it doesn’t disappear.

It lands in a quarantine table **with the same lineage keys**.

That lets you:

- group failures by source artifact
- identify “bad” files quickly
- reproduce issues deterministically
- decide whether it’s corruption, parser logic, or schema strictness

Example:

```sql
-- Which files generate the most quarantined rows?
SELECT
  _cf_source_hash,
  COUNT(*) AS quarantined_rows
FROM evtx_events_quarantine
GROUP BY 1
ORDER BY quarantined_rows DESC
LIMIT 20;
```

---

## “But We Already Have Lineage in Our Orchestrator…”

Orchestrator lineage (Airflow, Dagster, etc.) typically tells you:

- which task ran
- which dataset was written

That’s useful for operations.

But it’s not enough for evidence-grade work because it usually can’t tell you:

- which input artifact produced which record
- which subset of records was affected by a bug
- which records were produced by which parser version

Per-row lineage is the missing layer that turns a pipeline into a system you can audit.

---

## A Simple Rule: Lineage Must Be Unavoidable

One of the easiest ways lineage gets broken is when it’s “optional.”

- someone forgets to add it
- a new parser emits columns that collide with metadata
- a downstream transform drops it

Casparian treats lineage as **system-reserved**:

- `_cf_*` columns are injected automatically
- collisions are rejected
- schema contracts ensure lineage fields are present and correctly typed

That “boring” rigor is the difference between a nice idea and a reliable primitive.

---

## The Takeaway

If your Bronze layer is the foundation of your data stack, per-row lineage is the rebar.

It gives you:

- traceability
- reproducibility
- defensibility
- real debugging power

And it makes the rest of the system honest, because it’s impossible to hide from provenance.

---

## Next in the Series

Next up: **Schema contracts as code**—how to enforce structure without turning into an enterprise bureaucracy.

<!-- TODO: update CTA link to your site -->
If you need evidence-grade lineage for file-based ingestion, reach out for a pilot: `/contact`

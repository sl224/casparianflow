---
title: "Quarantine Semantics: Stop Letting One Bad Row Poison Your Pipeline"
description: "Real datasets are messy. The question isn’t whether you’ll see invalid records—it’s whether you’ll lose them, silently coerce them, or quarantine them with context. Here’s the quarantine pattern Casparian Flow uses to make partial success safe."
pubDate: 2026-01-24
---

There’s a classic ingestion failure mode everyone eventually meets:

> 9,999,999 rows parse fine.  
> Row 10,000,000 has a malformed timestamp.  
> The entire job fails… or worse, “succeeds” by coercing the bad value into a default.

Both outcomes are unacceptable in high-stakes environments:

- In **DFIR**, silently dropping or coercing a row can mean **lost evidence**.
- In **finance**, coercing a numeric field can introduce a rounding error that looks like a reconciliation issue.
- In **healthcare**, malformed timestamps can shift clinical events across days.

If you can’t trust your failure modes, you can’t trust your data.

This post explains the **quarantine semantics** Casparian Flow is built around: a pattern that preserves correctness *and* keeps the pipeline moving.

---

## Three Ways Pipelines Handle Invalid Data

Most ingestion systems fall into one of these buckets:

### 1) Fail-fast (all-or-nothing)
If any record is invalid, the job fails.

**Pros**
- No bad data makes it downstream
- Simple to reason about

**Cons**
- One bad row blocks millions of good ones
- Creates operational “pager fatigue”
- Forces risky hotfixes under pressure

Fail-fast is appropriate when *any* invalid output is catastrophic. But most real corpuses contain some noise.

### 2) Coerce / default / “best effort”
Invalid values get “fixed” silently: nulls become empty strings, dates become `1970-01-01`, etc.

**Pros**
- Pipelines keep running
- Downstream tables look “complete”

**Cons**
- You just shipped corruption
- You can’t tell what was wrong later
- Debugging becomes archaeology

This is the worst failure mode because it creates **false confidence**.

### 3) Quarantine (partial success, explicit)
Valid data continues. Invalid records are preserved separately with context.

**Pros**
- Correctness is preserved
- You don’t lose data
- You can debug systematically
- Processing is resilient to edge cases

**Cons**
- Requires a deliberate design (not an afterthought)

Casparian Flow is designed around option 3.

---

## What “Quarantine” Means in Casparian

In Casparian Flow, a parser doesn’t get to silently decide what “valid” means.

Instead:

1. A **schema contract** defines what the output must be.
2. The worker validates parser outputs **authoritatively in Rust**.
3. Rows are split into:

- ✅ **clean output** (contract-compliant rows)
- 🚧 **quarantine output** (violations + error context)

The key point is that quarantine is **not data loss**.

It’s explicit, queryable, and reversible.

---

## The Quarantine Table Pattern

For every output table, you can think in pairs:

- `events` — valid rows only
- `events_quarantine` — invalid rows + metadata for triage

A quarantine table typically contains:

- the original row payload (as columns, or as a raw representation)
- **error code / message**
- the specific violated constraint (type mismatch, nullability, range, enum, etc.)
- a pointer to where the record came from (lineage)

This makes it possible to answer:

- *“How many records failed and why?”*
- *“Is this a new upstream issue or a parser bug?”*
- *“Can we safely ignore this class of violations?”*
- *“What do we need to change to get these rows clean?”*

---

## Why Quarantine Is Essential for DFIR

In DFIR, “invalid data” is often not “bad data”—it’s **hostile or corrupted artifacts**.

Logs are truncated. Records are partially written. Malware intentionally damages sources.

If your ingestion strategy is fail-fast, you get a pipeline that fails exactly when you need it most.

If your strategy is silent coercion, you get “results” you can’t defend.

Quarantine gives you a third option:

- You keep processing and get usable timelines fast.
- You preserve exceptions with enough context to justify them later.
- You can iterate safely, without destroying the original evidence.

---

## What Makes Quarantine Work (And What Breaks It)

Quarantine isn’t just “write bad rows somewhere.” It only works if:

### 1) The schema is explicit and enforced
If there’s no contract, “invalid” is subjective.

Casparian contracts define column types, nullability, and constraints. Validation happens outside the plugin runtime so plugin code can’t accidentally “paper over” failures.

### 2) The quarantine row is traceable
A quarantine table without provenance is just a junk drawer.

Casparian injects lineage columns into both clean and quarantine outputs (source hash, job id, parser version, etc.) so you can trace each row to the input artifact and the run that produced it.

### 3) Partial success is *safe*
Many systems claim partial success, but still leave you with half-written outputs.

Casparian writes outputs in a staging area and only promotes them on success. Cancellation means no commit.

That way:
- “partial success” is a deliberate outcome, not a corrupted state.

### 4) You can reprocess quarantine later
Quarantine should create a feedback loop:

- tighten parser logic
- revise schema contract (through approvals)
- re-run only what needs reprocessing (incremental materializations)

---

## A Practical Quarantine Workflow

Here’s a workflow we’ve seen repeatedly across verticals:

1. **Ingest a real corpus**
2. **Inspect quarantine summary**
3. **Classify failures** into buckets:
   - input corruption / upstream noise
   - parser logic bugs
   - schema contract too strict
4. **Decide policy** per violation class:
   - keep quarantined forever (expected noise)
   - fix parser to accept valid variants
   - evolve schema contract (with approvals)
5. **Backfill / reprocess** only the affected materializations

This is the core of “serious” ingestion: you aren’t hoping the data is clean—you’re building a system that can handle reality.

---

## Example: Querying Quarantine

Once quarantine is a table, it becomes *operationally useful*:

```sql
-- What are the top violation types?
SELECT
  violation_type,
  COUNT(*) AS rows
FROM evtx_events_quarantine
GROUP BY 1
ORDER BY rows DESC;
```

```sql
-- Show examples of a specific failure
SELECT
  _cf_source_hash,
  _cf_parser_version,
  _cf_row_error
FROM evtx_events_quarantine
WHERE violation_type = 'timestamp_parse_error'
LIMIT 20;
```

This is the difference between “my script crashed” and “I have a measurable, triageable set of exceptions.”

---

## The Takeaway

A mature ingestion system doesn’t pretend invalid data won’t happen.

It makes three promises:

1. **You won’t lose data.**
2. **You won’t silently corrupt data.**
3. **You can explain what happened.**

Quarantine semantics are how we keep those promises.

---

## Next in the Series

Next up: **Per-row lineage**—because quarantine without provenance is just a junk drawer.

<!-- TODO: update CTA link to your site -->
Want to see quarantine semantics on your data? Reach out for a pilot: `/contact`

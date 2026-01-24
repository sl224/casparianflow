---
title: "Deterministic Ingestion: Content-Addressed Parsers and Idempotency Keys"
description: "Reruns are where pipelines lie. If you can’t prove what code ran on what inputs, you can’t trust your outputs. Casparian Flow uses content-addressed parser identity plus idempotency keys to make incremental ingestion deterministic by default."
pubDate: 2026-01-24
---

Most ingestion systems are *fine* on the first run.

They fail on the second run—when something changes and you need to answer:

- Did we reprocess the right files?
- Did we double-write anything?
- Which outputs came from which version of the parser?
- Can we reproduce last month’s results exactly?

This is where a lot of “data pipeline” tooling quietly becomes a pile of scripts.

Casparian Flow treats ingestion like a **build system**. That requires two things:

1. **Deterministic identity** for inputs and toolchain  
2. **Idempotent output planning** so reruns are safe

This post explains how Casparian does both.

---

## The Hidden Problem: “parser.py” Is Not an Identity

A common pattern looks like:

- You have a parser at `parsers/evtx/evtx_parser.py`
- Someone edits it
- You rerun ingestion

Now you have an unanswerable question:

> Did the output come from the old logic or the new logic?

Git hashes help, but only if:
- everyone commits perfectly
- the execution environment is pinned
- nobody runs local edits

In regulated and investigative environments, “we think it was commit X” isn’t good enough.

---

## Casparian’s Answer: Content-Addressed Parser Identity

Casparian treats a parser artifact as something you can hash.

In practice, the identity is derived from:

- plugin source code content
- dependency lockfile (environment)
- explicit parser version metadata

The key idea:

> **Parser identity is content-based, not path-based.**

If the parser code changes, its hash changes. If the dependencies change, the environment hash changes. The system sees a new toolchain and can plan reprocessing accordingly.

This makes provenance real:

- same inputs + same parser bundle hash → identical outputs
- different parser bundle hash → you can’t pretend it’s “the same run”

---

## Deterministic Inputs: Source Hashes

On the input side, Casparian identifies files by content hash.

Not by path. Not by filename. By bytes.

That gives you a stable input identity even when:

- evidence bundles are re-extracted
- folders are renamed
- file shares reorganize

And it enables the strongest form of reproducibility:

> If the content hash matches, you processed the same artifact.

---

## The Glue: Idempotency Keys

To make reruns safe, the system needs stable keys for “what is this output?” and “have we already produced it?”

Casparian uses two core keys:

### 1) Output Target Key
Represents *where* data is being written.

It includes things like:

- sink URI (DuckDB path, Parquet folder, etc.)
- table name
- schema hash
- sink mode (append/replace/error)

If any of those change, you get a different output target key.

### 2) Materialization Key
Represents a specific “build artifact”:

> output target + source hash + parser artifact hash

That means:

- same file processed by same parser artifact to same output target  
  → same materialization key  
  → safe to skip (already done)

- parser artifact changes  
  → materialization key changes  
  → system knows it must reprocess

This is how you get **deterministic incremental ingestion** without guessing.

---

## Atomic Outputs: Stage → Promote

Idempotency keys protect you from double-processing.

Atomic output commits protect you from half-written results.

Casparian writes outputs in a staging area and only promotes them when the job concludes successfully.

That way:

- “completed” means committed
- “failed” means nothing was promoted
- “aborted” means no commit (cancel actually means stop)

This sounds obvious until you’ve lived through a pipeline that leaves corrupted partial artifacts behind.

---

## Why This Matters for “Seriousness”

When someone evaluates a data tool, they’ll ask about features.

But robustness is mostly about boring questions like:

- What happens on rerun?
- What happens on cancellation?
- What happens when code changes?
- What happens when schemas drift?

Content-addressed toolchain identity + idempotency keys are how you answer those questions without hand-waving.

This is the difference between:

- “it usually works”
- “it’s reproducible, and we can prove it”

---

## A Simple Mental Model

Treat your ingestion run like a compilation:

- **source files** → input artifacts (content hashes)
- **compiler** → parser artifact (content-addressed)
- **build outputs** → materializations (tracked and atomic)

Suddenly, “incremental ingestion” becomes obvious:

- if nothing changed, nothing rebuilds
- if code changed, only affected artifacts rebuild
- if outputs changed, identity keys prevent collisions

---

## Next in the Series

Next up: **Local-first ingestion**—why running in-place matters for regulated and air-gapped environments.

<!-- TODO: update CTA link to your site -->
If you need deterministic reruns for messy file corpuses, reach out for a pilot: `/contact`

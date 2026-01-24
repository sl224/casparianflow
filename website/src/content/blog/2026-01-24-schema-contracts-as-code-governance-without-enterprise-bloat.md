---
title: "Schema Contracts as Code: Governance Without Enterprise Bloat"
description: "Schema-on-read works—until it doesn’t. Casparian Flow treats schemas as proposals first, then immutable contracts after approval. The result: no silent drift, explicit violations, and controlled evolution you can audit."
pubDate: 2026-01-24
---

Schema governance has a reputation problem.

On one side, you have “schema-on-read” chaos:

- inferred types change week to week
- columns appear/disappear
- downstream models break quietly
- people stop trusting dashboards

On the other side, you have heavyweight enterprise governance:

- committees
- tickets
- spreadsheets
- long lead times

Most teams end up in the middle: **no real contracts**, plus a bunch of brittle assumptions.

Casparian Flow takes a different stance:

> **Schema is intent first, then contract.**

This post explains what that means, why it matters in Bronze, and how we keep it lightweight enough to be practical.

---

## Why Schema-On-Read Fails in the Bronze Layer

Schema-on-read is attractive because it lets you get started fast:

- parse what you can
- clean later
- “we’ll handle edge cases downstream”

But the Bronze layer isn’t just a staging area. It’s the foundation of your downstream trust.

Two failure modes show up repeatedly:

### 1) Silent drift
A new variation appears in the source, and types start changing:

- `"00123"` becomes `123`
- `"31/05/24"` flips from `MM/DD/YY` to `DD/MM/YY`
- decimals become floats
- timestamps parse “most of the time”

Your pipeline still runs. Your results are now wrong.

### 2) Tribal schemas
The “schema” exists in someone’s head, or in a notebook, or in a parser function.

When that person leaves—or the format evolves—you lose the schema.

Contracts exist to prevent both failure modes.

---

## Casparian’s Model: Proposal → Approval → Contract

Casparian separates schema lifecycle into two phases:

### Before approval: schema is a proposal
You’re exploring. You’re iterating.

Schema changes should be cheap.

### After approval: schema is a contract
Once you promote a schema, it becomes an **auditable agreement**:

- the parser must conform
- violations are explicit
- evolution is controlled (new contract/amendment, not silent mutation)

This matches how real teams work:

- experimentation first
- defensibility second

---

## What a Schema Contract Actually Enforces

A Casparian contract is not just a list of columns.

It includes:

- data types (including Decimal and timestamp-with-timezone where needed)
- nullability constraints
- output identity (which parser + version + output name the contract applies to)
- quarantine policy (how violations are handled)

And—crucially—

**validation is authoritative in Rust**, not left to plugin code.

That means:

- plugins can be fast and focused on parsing
- validation rules don’t drift across environments
- “helpful” coercions don’t sneak in

---

## Explicit Violations, Not Silent Fixes

When a record violates a contract, the system must do something explicit.

Casparian supports the “quarantine pattern” by default:

- valid rows continue to `table`
- invalid rows go to `table_quarantine` with error context

This gives you safe partial success without hiding the truth.

It also creates an iteration loop:

- inspect violation types
- decide if it’s a parser bug, source corruption, or contract strictness
- update parser or propose a new contract
- rerun incrementally

---

## Contract Evolution Without Bureaucracy

The goal isn’t to recreate enterprise workflow tools.

The goal is to make schema change:

- **explicit**
- **reviewable**
- **auditable**

A lightweight evolution model looks like:

1. **Propose** a new schema version (based on real corpus behavior)
2. **Backtest** against a representative file set
3. **Approve** the contract when it’s stable
4. **Promote** to “production contract”
5. **Reprocess** only what changed (incrementally)

If your schema changes and you *don’t* backfill, you’re lying to yourself about consistency.

Contracts force you to face that reality.

---

## Why This Matters for DFIR

DFIR workflows have a special constraint: **evidence must be defensible**.

A common “fragile scripts” failure looks like:

- parser crashes on corrupted artifacts
- developer adds a try/except and default values
- the script “works” again
- nobody can explain what was defaulted vs. real

Schema contracts + quarantine avoid this:

- you can process quickly
- you don’t lose evidence
- you can explain exactly what violated the contract, and why

---

## A Practical Workflow

Even without a fancy UI, the workflow is simple:

1) Preview output on a few files  
2) Propose a schema  
3) Backtest on the corpus  
4) Approve + lock the schema  
5) Run ingestion and trust the outputs

The important part is cultural:

> Schema is not an implementation detail. It’s an explicit agreement.

---

## The Takeaway

In the Bronze layer, schema is not about “making SQL happy.”

It’s about making trust possible.

Schema contracts give you:

- a stable foundation for Silver/Gold
- explicit failure modes (quarantine vs silent coercion)
- controlled evolution you can audit
- defensible outputs in regulated environments

And because the contract system is built into the runtime, it doesn’t require enterprise bureaucracy to be effective.

---

## Next in the Series

Next up: **Deterministic ingestion**—how content-addressed parser identities and idempotency keys make reruns safe.

<!-- TODO: update CTA link to your site -->
If you’re tired of silent schema drift in file-based ingestion, reach out for a pilot: `/contact`

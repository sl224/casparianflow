# Casparian Flow: Product Strategy

**Status:** Canonical
**Last updated:** January 2026

---

## Executive Summary

Casparian Flow is a **local-first ingestion and governance runtime** that transforms industry-specific file formats into queryable SQL/Parquet datasets with per-row lineage, quarantine semantics, and reproducibility manifests.

**v1 Focus:** DFIR / Incident Response artifact parsing. EVTX as flagship parser.

**Core Promise:** If you can point Casparian at a directory of files and a parser, you can reliably produce tables you can trust—and you can prove how you got them.

**Key Differentiators:**
1. **Deterministic execution** - Schema contracts enforced in Rust; no silent coercion
2. **Local-first execution** - Data never leaves the machine; air-gapped and sovereignty-friendly
3. **Trust primitives** - Per-row lineage, quarantine semantics, reproducibility manifests
4. **Evidence-grade outputs** - Chain of custody for regulated industries

---

## Vertical Priority

### Why These Orderings

| Constraint | How It Shapes Priority |
|------------|----------------------|
| **No warm leads** | Must target self-serve segments with short sales cycles |
| **Minimal support** | Avoid high-touch personas who expect custom parser development |
| **Annual OK** | Can price annually (vs. monthly); fits project-based engagements |
| **Paid pilots** | Filter tire-kickers; credit toward annual purchase |
| **Avoid custom parser trap** | Don't promise custom parser dev as part of sale |

### Priority Stack

| Priority | Vertical | Wedge | Why This Slot |
|----------|----------|-------|---------------|
| **P0** | DFIR | EVTX artifact parsing | Cashflow wedge. Urgent buyers. Short sales cycle. Self-serve. Annual engagements. |
| **P1** | eDiscovery | Production preflight (DAT/OPT/LFP) | Same trust primitives. Billable engagements. Legal tech network. |
| **P2** | Defense | Flight test returned media (CH10/TF10) | High LTV. Longer cycle but worth it. Air-gapped requirement fits. |
| **P3** | Finance | Trade break workbench | Consultant-delivered only. Avoid custom parser trap. No warm leads. |
| **Deferred** | Pharma, Healthcare | Various | Long enterprise cycles. Post-traction. |

### Vertical Strategy Docs

| Vertical | Strategy Doc | Status |
|----------|--------------|--------|
| DFIR | `strategies/dfir.md` | Canonical (P0) |
| eDiscovery | `strategies/legal_ediscovery.md` | Canonical (P1) |
| Defense Flight Test | `strategies/defense_flight_test_returned_media.md` | Canonical (P2) |
| Defense Tactical Edge | `strategies/defense_tactical.md` | Canonical (separate track) |
| Pharma Clinical | `strategies/pharma_clinical_submission.md` | Deferred |
| Pharma R&D | `strategies/pharma.md` | Deferred |
| Healthcare X12 | `strategies/healthcare_x12_claims_ledger.md` | Deferred |
| Healthcare HL7 | `strategies/healthcare_hl7.md` | Deferred |

---

## Why DFIR Is P0 (The Winner)

DFIR consultants are the **only customer** with "network drive data" that is both **urgent** (active breach) and **legally mandated** to have a perfect audit trail.

| Factor | DFIR | eDiscovery (P1) | Finance (P3) |
|--------|------|-----------------|--------------|
| **Urgency** | EXTREME (active breach) | High (court deadlines) | Medium (T+1) |
| **Audit Trail** | LEGALLY MANDATED (evidence chain) | Required (court) | Nice-to-have |
| **Writes Python?** | YES | Mixed | NO |
| **Sales Cycle** | FAST (boutiques) | Medium (firms) | Long (enterprise) |
| **Why They Pay** | Speed + Liability | Error prevention | Time savings |

### The DFIR Value Proposition

> "If my script deletes a row, I destroy evidence. Casparian's lineage and quarantine is my insurance policy."

**We are NOT "another EVTX parser."** We are: "turn DFIR parsing into an auditable, repeatable, backfillable dataset build process."

### DFIR Core Value Story

Casparian is a **deterministic, governed "data build system"** for file artifacts:
- Schema contracts enforced authoritatively (Rust validation) — no silent coercion
- Quarantine invalid/out-of-range rows — partial success is safe
- Per-row lineage: source hash + job id + processed timestamp + parser version
- Reproducible run identity (content-addressed parser bundle)
- Incremental ingest: version-dedup + backfill planning when parser versions change
- CLI-first; minimal TUI for discovery/bench/jobs/quarantine summary

---

## Trust Primitives

| Primitive | Description | DFIR Value |
|-----------|-------------|------------|
| **Manifest** | Complete inventory of inputs + outputs | Evidence bundle documentation |
| **Hash Inventory** | blake3 hash per input file | Prove processing matches source |
| **Quarantine** | Invalid rows isolated with error context | No silent data loss |
| **Lineage Columns** | `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` | Trace any row to source |
| **Diffs** | Compare run outputs | "What changed" for re-processing |
| **Reproducibility** | Same inputs + parser hash → identical outputs | Court-defensible |

---

## Pricing Strategy

**System of Record:** `docs/product/pricing.md`

### Core Principles

1. **Annual-first** - Predictable revenue; fits project-based engagements
2. **Paid pilots** - $1,000 creditable to annual; filter tire-kickers
3. **Minimal support** - Email SLAs, no custom parser dev, no dedicated CSM below Enterprise

### Pricing Summary (DFIR)

| Tier | Annual Price | Users | Support |
|------|--------------|-------|---------|
| **Solo** | $1,200/year | 1 | Community |
| **Team** | $4,800/year | Up to 5 | Email (48hr) |
| **Enterprise Lite** | $12,000/year | Up to 15 | Priority email, monthly call |

See `docs/product/pricing.md` for all verticals.

---

## Go-to-Market

### Phase 1: DFIR Cash Flow (Months 1-6)

**Target:** DFIR boutiques, IR consultants, forensic engineers

**Channels:**
- SANS community, DFIR Discord, LinkedIn
- Boutique IR firm partnerships
- Conference presence (SANS, DFRWS)

**Demo:** "Corrupted EVTX files that crash other tools → Casparian quarantines gracefully"

**Success Metrics:**
- 10 Solo + 5 Team licenses
- $30K ARR from DFIR segment

### Phase 2: eDiscovery Expansion (Months 6-12)

**Target:** Litigation support technologists, eDiscovery processing analysts

**Channels:**
- ILTA, ACC, ACEDS communities
- Legal tech consultant partnerships
- LinkedIn outreach

**Demo:** "Validate production load files before they fail on import"

**Success Metrics:**
- 8 Solo + 4 Team licenses
- $50K ARR combined (DFIR + eDiscovery)

### Phase 3: Defense Entry (Months 12-18)

**Target:** Flight test data processors, telemetry data engineers

**Channels:**
- SBIR/STTR applications
- Defense tech partnerships
- Test range relationships

**Success Metrics:**
- 1 SBIR Phase I award
- 2 Tactical tier licenses
- $100K ARR combined

---

## What We Don't Do

| Trap | Why We Avoid It |
|------|-----------------|
| **Custom parser development** | Becomes services business; support costs exceed revenue |
| **Free extended pilots** | Attracts tire-kickers; delays buying decision |
| **Enterprise sales before cash flow** | Long cycles burn runway |
| **Finance direct sales** | High-touch, no warm leads, custom parser expectations |

### Do Not Target (Explicitly Cut)

| Persona | Why Cut |
|---------|---------|
| **Trade Support Analyst** | Want an *answer*, not a database. Don't write parsers. Excel users. |
| **eDiscovery Analyst** | Click "Process" in Relativity. Expect vendor support. File tickets. |
| **General IT Admin** | Use Splunk/Cribl. Want search bars, not schema definitions. |

### The Qualifying Question

> "When a weird file format fails to parse, do they (a) write a Python script, or (b) email a vendor?"

- **(a) Write a script** → Valid target
- **(b) Email a vendor** → DO NOT TARGET

---

## Product Architecture

### Core Components (v1)

| Component | Purpose |
|-----------|---------|
| **Scout** | File discovery + tagging by pattern |
| **Sentinel** | Control plane: job orchestration, materializations |
| **Worker** | Execution plane: parser execution, schema validation |
| **Schema Contracts** | Governance + validation; violations = hard failures |
| **Sinks** | Output persistence (DuckDB, Parquet, CSV) |

### Not v1

| Excluded | Why |
|----------|-----|
| **Streaming** | Focus on files-at-rest |
| **Orchestration** | Not a DAG runner |
| **BI/Analytics** | Outputs go to external tools |
| **Cloud control plane** | Local-first is the point |

---

## Competitive Landscape

### DFIR Competitors

| Competitor | What They Do | Gap |
|------------|--------------|-----|
| **Plaso/log2timeline** | Timeline creation | No governance, no quarantine |
| **Velociraptor** | Live forensics + artifacts | Collection focus, not governance |
| **Autopsy** | Forensic analysis platform | GUI-focused, less programmable |
| **Custom Python** | Ad-hoc parsing | No lineage, no reproducibility |
| **Casparian** | Governed artifact ingestion | Lineage + quarantine + reproducibility |

### eDiscovery Competitors

| Competitor | What They Do | Gap |
|------------|--------------|-----|
| **Relativity** | Full eDiscovery platform | Overkill for preflight |
| **Nuix** | Forensic processing | Enterprise pricing |
| **Vendors** | Processing services | $5-15K per matter |
| **Casparian** | Production preflight | Validation layer, not full platform |

---

## Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| **DFIR market smaller than expected** | Medium | eDiscovery as P1 backup; same trust primitives |
| **Enterprise sales required** | High | Stay self-serve; productized onboarding |
| **Custom parser expectations** | High | Explicit scope in pilot terms; no promises |
| **Competitor adds governance** | Medium | Execution speed; first-mover advantage |

---

## Success Metrics

### Year 1 Targets

| Metric | Target |
|--------|--------|
| DFIR ARR | $50K |
| eDiscovery ARR | $30K |
| Defense ARR | $48K |
| Total ARR | $150K |
| Paid customers | 30 |
| Quarantine catch rate | 95%+ |

### Health Metrics

| Metric | Target |
|--------|--------|
| Time-to-first-query | <15 minutes |
| Activation rate (first parser) | >50% |
| 7-day retention | >40% |
| Support tickets per customer | <2/month |

---

## Decision Log

| Decision | Rationale |
|----------|-----------|
| DFIR first | Urgent + legally mandated audit trail; short sales cycle |
| Annual-first pricing | Predictable revenue; fits project engagements |
| Paid pilots | Filter tire-kickers; avoid support burden |
| Finance = P3 | High-touch, no warm leads; consultant-delivered only |
| No custom parser dev | Avoid services trap; focus on product |
| eDiscovery = preflight only | Don't compete with Relativity; narrow scope |

---

## Related Documentation

| Doc | Purpose |
|-----|---------|
| `docs/v1_scope.md` | Detailed v1 scope and success metrics |
| `docs/product/pricing.md` | System of record for pricing |
| `ARCHITECTURE.md` | Technical architecture |
| `CLAUDE.md` | Developer context |
| `strategies/*.md` | Vertical-specific strategies |

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 5.0 | Major rewrite: DFIR-first narrative; updated vertical priority stack; removed finance-first remnants; added "Why These Orderings" section with constraints; references new pricing doc |

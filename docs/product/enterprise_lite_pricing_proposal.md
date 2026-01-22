# Enterprise Lite: Pricing & Packaging Proposal (v1)

**Purpose:** Fastest path to cash flow with a single-node, local-first offering.
**Status:** Active
**Date:** 2026-01-21

---

## 1. Goals

1. Ship a paid tier that closes in weeks, not quarters.
2. Preserve local-first, air-gapped positioning.
3. Defer multi-node/DB integrations until revenue exists.
4. **Product-first:** Services delivered as productized onboarding (fixed scope), not bespoke.

---

## 2. Target Customer #1

**Vertical:** DFIR / Incident Response

**Why first:**
- Urgent + legally mandated audit trail (chain of custody requirements).
- Budget exists for tooling that reduces evidence handling risk.
- Fast buyer loop: boutique firms, practitioners decide.
- Technical users who write Python (core Casparian value).

**Buyer profile:**
- DFIR consultants, forensic engineers, IR engineers
- Boutique IR firms (5-50 people)
- Enterprise SOC/CIRT teams

**Why NOT Finance first:**
- Trade Support Analysts don't write parsers (core Casparian value)
- Risk of "Service Trap" where Casparian maintains parsers for non-technical users
- Finance moved to P3 (consultant-delivered only)

---

## 3. Packaging: Enterprise Lite (v1)

### Core Product (Single-Node, Local-First)
- CLI + TUI for parser publish and job monitoring.
- Local data store: DuckDB.
- Output: Parquet/DuckDB.
- Schema contracts and quarantine (governance built in).
- **Trust primitives:** per-row lineage, reproducibility, evidence-grade manifests.
- Air-gapped capable, no mandatory cloud services.
- **Windows-friendly:** EVTX as flagship parser pack.

### Exclusions (Explicitly NOT in v1)
- Postgres or MSSQL output targets.
- Multi-node scheduling or server deployment.
- SSO/SAML.
- Multi-tenant admin dashboard.
- Streaming (batch only).
- AI-dependent features.

---

## 4. Productized Onboarding SKUs

**We remain product-first.** Services are fixed-scope productized onboarding, not bespoke.

| SKU | Scope | Deliverables |
|-----|-------|--------------|
| **DFIR Starter Pack** | Fixed scope, short engagement | Deploy on workstation/server (offline/air-gap); ingest one real case corpus; EVTX → governed DuckDB/Parquet + quarantine; evidence-grade manifest template + runbook |
| **Custom Artifact Pack** | Fixed scope | Implement 1–2 custom artifacts as Casparian parsers; regression tests; deliver as internal bundle |
| **Maintenance Subscription** | Recurring | Parser pack updates; regression suite; backfill planning support |

---

## 5. Pricing (DFIR-First, Land Then Expand)

**Stage 1: Land**
- **Team**: $2,000/month
- 5 users, single node, EVTX parser pack, priority support.
- Goal: first 3-5 paying teams in 90 days.

**Stage 2: Expand**
- **IR Firm**: $6,000/month
- Unlimited users, multi-engagement, custom artifact packs, SLA.
- Trigger: proven ROI + expanded usage.

**Stage 3: Enterprise**
- $15,000+/month
- Adds SSO, compliance deliverables, evidence-grade export templates, dedicated success.

---

## 6. Delivery Plan (90 Days)

**Weeks 1-4: Validation**
- 20 conversations with DFIR practitioners.
- Convert 3 pilots to paid Team tier.

**Weeks 5-8: Ship Enterprise Lite**
- Harden single-node flow.
- Add evidence-grade manifest export.
- Focus on end-to-end time-to-value: case folder → query in <15 minutes.

**Weeks 9-12: Expand**
- Push 1-2 customers into IR Firm tier.
- Collect ROI proof points and reference quotes.
- Validate productized onboarding SKUs.

---

## 7. Migration Path (Postgres/MSSQL Later)

**Phase 2: Postgres**
- Target: 3-6 months after first $50K ARR.
- Rationale: most common on-prem target for regulated customers.

**Phase 3: MSSQL**
- Target: after Postgres proves demand.
- Rationale: enterprise Microsoft stack requirement.

**Cloud Sinks (Optional Extension):**
- Local-first is core; cloud is optional output destination only.
- Supported: Write Parquet to S3, load into cloud SQL.
- No cloud control plane; no SaaS dependency.

---

## 8. Key Risks & Mitigations

**Risk:** Team tier seen as "too small" for enterprise buyers.
- Mitigation: sell as entry point; position IR Firm tier as expansion.

**Risk:** Requests for shared DB access block deals.
- Mitigation: Parquet export + DuckDB for local SQL now; commit Postgres timeline for qualified deals.

**Risk:** Too many verticals dilute messaging.
- Mitigation: **DFIR only until $50K ARR.**

---

## 9. Success Metrics

- 3 paying customers at $2K/month in 90 days.
- 1 expansion to $6K/month within 120 days.
- <2 week average sales cycle for Team tier.
- 50%+ weekly active usage in pilots.
- Time-to-first-query on case folder: <15 minutes.

---

## 10. Positioning Statement (Enterprise Lite)

"Deterministic, governed artifact parsing for DFIR. Evidence-grade lineage, quarantine semantics, reproducibility. Single-node, air-gapped, Windows-friendly, ready to deploy in a day." 

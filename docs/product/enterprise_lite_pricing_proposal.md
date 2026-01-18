# Enterprise Lite: Pricing & Packaging Proposal (v1)

**Purpose:** Fastest path to cash flow with a single-node, local-first offering.
**Status:** Draft proposal
**Date:** 2026-01-XX

---

## 1. Goals

1. Ship a paid tier that closes in weeks, not quarters.
2. Preserve local-first, air-gapped positioning.
3. Defer multi-node/DB integrations until revenue exists.

---

## 2. Target Customer #1

**Vertical:** Finance (Trade Support / FIX operations)

**Why first:**
- Strong, validated pain around manual log parsing and trade break resolution.
- Budget exists for tooling that saves analyst hours.
- Fast buyer loop compared to healthcare/defense.

**Buyer profile:**
- Technical ops teams who already use SQL, grep, Unix tools.
- Mid-market and enterprise desks with recurring break volume.

---

## 3. Packaging: Enterprise Lite (v1)

### Core Product (Single-Node, Local-First)
- CLI + TUI for parser registration and job monitoring.
- Local data store: DuckDB/SQLite.
- Output: Parquet/CSV/SQLite.
- Schema contracts and quarantine (governance built in).
- Audit trail: parser version, inputs, outputs.
- Air-gapped capable, no mandatory cloud services.

### Exclusions (Explicitly NOT in v1)
- Postgres or MSSQL output targets.
- Multi-node scheduling or server deployment.
- SSO/SAML.
- Multi-tenant admin dashboard.

---

## 4. Pricing (Finance-First, Land Then Expand)

**Stage 1: Land**
- **Team**: $2,000/month
- 5 users, single node, FIX parsing, priority support.
- Goal: first 3-5 paying teams in 90 days.

**Stage 2: Expand**
- **Trading Desk**: $6,000/month
- Unlimited users, higher throughput, custom tags, SLA.
- Trigger: proven ROI + expanded usage in the same org.

**Stage 3: Enterprise**
- $15,000+/month
- Adds SSO, compliance deliverables, dedicated success.

---

## 5. Delivery Plan (90 Days)

**Weeks 1-4: Validation**
- 20 conversations with finance ops teams.
- Convert 3 pilots to paid Team tier.

**Weeks 5-8: Ship Enterprise Lite**
- Harden single-node flow.
- Add basic compliance/exportable audit report.
- Focus on end-to-end time-to-value: file -> query in minutes.

**Weeks 9-12: Expand**
- Push 1-2 customers into Trading Desk tier.
- Collect ROI proof points and reference quotes.

---

## 6. Migration Path (Postgres/MSSQL Later)

**Phase 2: Postgres**
- Target: 3-6 months after first $50K ARR.
- Rationale: most common on-prem target for regulated customers.

**Phase 3: MSSQL**
- Target: after Postgres proves demand.
- Rationale: enterprise Microsoft stack requirement.

---

## 7. Key Risks & Mitigations

**Risk:** Team tier seen as "too small" for enterprise buyers.
- Mitigation: sell as entry point; position Trading Desk as expansion.

**Risk:** Requests for shared DB access block deals.
- Mitigation: Parquet export + DuckDB for local SQL now; commit Postgres timeline for qualified deals.

**Risk:** Too many verticals dilute messaging.
- Mitigation: Finance only until $50K ARR.

---

## 8. Success Metrics

- 3 paying customers at $2K/month in 90 days.
- 1 expansion to $6K/month within 120 days.
- <2 week average sales cycle for Team tier.
- 50%+ weekly active usage in pilots.

---

## 9. Positioning Statement (Enterprise Lite)

"Local-first FIX log parsing with schema contracts and audit trails. Single-node, air-gapped, ready to deploy in a day." 

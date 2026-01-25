# Casparian Flow - Pricing Strategy

**Status:** Canonical (System of Record)
**Date:** January 2026
**Parent:** STRATEGY.md

---

## 1. Executive Summary

This document is the single source of truth for Casparian Flow pricing. All other docs reference this.

**Pricing Philosophy:** Annual-first, paid pilots, minimal support overhead, self-serve where possible.

### Vertical Priority & Pricing Rationale

| Priority | Vertical | Wedge Use Case | Pricing Model | Why This Order |
|----------|----------|----------------|---------------|----------------|
| **P0** | DFIR | EVTX artifact parsing | Annual, self-serve | Cashflow wedge. Urgent buyers. Short sales cycle. |
| **P1** | eDiscovery | Production preflight (DAT/OPT/LFP) | Annual, self-serve | Same trust primitives. Billable engagements. |
| **P2** | Defense | Flight test returned media (CH10/TF10) | Annual, per-deployment | High LTV. Longer sales cycle. |
| **P3** | Finance | Trade break workbench | Consultant-delivered only | Avoid custom parser trap. Not self-serve. |
| **Deferred** | Pharma, Healthcare | Various | TBD | Long enterprise cycles. Post-traction. |

---

## 2. Core Pricing Principles

### 2.1 Annual-First

All tiers are priced **annually**. Monthly is exception, not default.

**Rationale:**
- Predictable revenue
- Lower churn risk
- Aligns with DFIR engagement budgets (project-based)
- Reduces billing overhead

### 2.2 Paid Pilots (No Free Tier for Commercial Use)

| Pilot Type | Price | Duration | Terms |
|------------|-------|----------|-------|
| **Evaluation** | Free | 14 days | Single user, demo data only, no production use |
| **Paid Pilot** | $1,000 | 30 days | Full product, production data, creditable to annual |

**Why paid pilots:**
- Filters tire-kickers from real buyers
- Credits toward annual = no "sunk cost" objection
- Avoids "free pilot + custom parser development" trap
- Time-boxed: 30 days max, explicit scope caps

### 2.3 Minimal Support Overhead

| Tier | Support Level | Response SLA |
|------|---------------|--------------|
| **Solo** | Community (Discord/GitHub) | Best effort |
| **Team** | Email | 48 hours |
| **Enterprise Lite** | Email + monthly call | 24 hours |

**Explicit exclusions (all tiers):**
- No custom parser development
- No on-call support
- No dedicated CSM below Enterprise Lite
- No phone support

---

## 3. DFIR Pricing (P0 - Primary Vertical)

**Target:** DFIR boutiques, IR consultants, forensic engineers

**Wedge:** EVTX artifact parsing with chain-of-custody lineage

### 3.1 Tier Structure

| Tier | Annual Price | Users | Features |
|------|--------------|-------|----------|
| **Solo** | $1,200/year | 1 | CLI + TUI, all parsers, 10GB/month, community support |
| **Team** | $4,800/year | Up to 5 | + MCP integration, unlimited volume, email support |
| **Enterprise Lite** | $12,000/year | Up to 15 | + SSO, priority email, monthly call, SLA (99.5%) |

### 3.2 What's Included (All Tiers)

- All premade parsers (EVTX, registry, prefetch, etc.)
- Per-row lineage (`_cf_source_hash`, `_cf_job_id`, etc.)
- Quarantine semantics with violation context
- Manifest export for chain of custody
- Local-first execution (air-gapped OK)

### 3.3 What's NOT Included

- Custom parser development (write your own or hire a consultant)
- 24x7 support (we're not an MSP)
- Hosted/cloud deployment (local-first only)
- Training beyond docs

---

## 4. eDiscovery Pricing (P1)

**Target:** Litigation support technologists, eDiscovery processing analysts

**Wedge:** Production preflight - validate load files before delivery

### 4.1 Tier Structure

| Tier | Annual Price | Users | Features |
|------|--------------|-------|----------|
| **Solo** | $1,800/year | 1 | CLI + TUI, load file parsers (DAT/OPT/LFP), 25GB/month |
| **Team** | $7,200/year | Up to 5 | + Unlimited volume, SFTP/UNC source support, email support |
| **Enterprise Lite** | $18,000/year | Up to 15 | + SSO, priority email, monthly call |

### 4.2 Why Higher Than DFIR

- Directly tied to billable engagements ($$$)
- Cost of production errors is high (court deadlines)
- Load file validation is specialized

---

## 5. Defense Pricing (P2)

**Target:** Flight test data processors, telemetry engineers

**Wedge:** Returned media ingest - CH10/TF10/DF10 from removable cartridges

### 5.1 Tier Structure

| Tier | Annual Price | Deployment Scope | Features |
|------|--------------|------------------|----------|
| **Tactical** | $24,000/year | Per flight test program | All parsers, 5 users, air-gapped, email support |
| **Mission** | $60,000/year | Multi-program | + 15 users, priority support, quarterly reviews |

### 5.2 Why Annual Per-Deployment

- Defense procurement expects annual contracts
- Flight test programs have defined timelines
- Air-gapped deployment = higher support complexity

---

## 6. Finance Pricing (P3 - Consultant-Delivered Only)

**Status:** Not self-serve. Delivered through consulting partnerships only.

**Rationale:**
- High-touch requirements (custom FIX tags, venue-specific formats)
- Enterprise sales cycle (6-12 months)
- Risk of "custom parser trap" where support costs exceed revenue
- No warm leads currently

### 6.1 Consulting Partner Model

| Engagement | Price | Scope |
|------------|-------|-------|
| **Implementation** | $25,000+ | Parser customization, schema setup, training |
| **Annual License** | $15,000/year | Platform access for deployed solution |

**We do not:**
- Sell directly to finance firms
- Offer free pilots
- Develop custom parsers in-house for finance

---

## 7. Deferred Verticals

### 7.1 Pharma (Clinical Submission)

**When:** After DFIR + eDiscovery traction (Month 12+)

**Wedge:** XPT + define.xml validation for clinical submissions

**Expected pricing:** $12,000-24,000/year (enterprise buyers, compliance-driven)

### 7.2 Healthcare (X12 Claims Ledger)

**When:** After Pharma exploration (Month 18+)

**Wedge:** 837/835/999 reconciliation

**Expected pricing:** $6,000-18,000/year

---

## 8. Pilot Program Terms

### 8.1 Paid Pilot ($1,000 / 30 Days)

**What's included:**
- Full product access (all tiers' features for evaluation)
- Production data allowed
- Email support during pilot
- $1,000 credited toward annual purchase

**What's NOT included:**
- Custom parser development
- Extended pilot duration (30 days max)
- Scope expansion beyond agreed use case
- Refunds (credit is forward-only)

### 8.2 Pilot Success Criteria

Define upfront:
- Number of files to process
- Expected output format
- Specific parsers to use
- Timeline for go/no-go decision

### 8.3 Kill Criteria

End pilot early if:
- Scope creep beyond agreed use case
- Customer requests custom parser development
- No engagement after 14 days
- Customer expects free extension

---

## 9. Discounts & Promotions

### 9.1 Standard Discounts

| Type | Discount | Conditions |
|------|----------|------------|
| **Multi-year** | 15% | 2-year commitment |
| **Nonprofit** | 20% | 501(c)(3) or equivalent |
| **Academic** | Free (Solo) | .edu email, non-commercial research |

### 9.2 No Discounts

- No "first year free"
- No volume discounts below 5 users
- No competitive displacement discounts
- No "we'll pay later" arrangements

---

## 10. Revenue Targets (Conservative)

| Quarter | DFIR | eDiscovery | Defense | Total ARR |
|---------|------|------------|---------|-----------|
| Q1 | 5 Solo ($6K) | 0 | 0 | $6K |
| Q2 | 10 Solo + 3 Team ($26K) | 2 Solo ($4K) | 0 | $30K |
| Q3 | 15 Solo + 5 Team ($42K) | 5 Solo + 2 Team ($23K) | 1 Tactical ($24K) | $89K |
| Q4 | 20 Solo + 8 Team ($62K) | 8 Solo + 4 Team ($43K) | 2 Tactical ($48K) | $153K |

**Year 1 Target:** $150-200K ARR

---

## 11. Constraints This Pricing Respects

| Constraint | How Pricing Respects It |
|------------|------------------------|
| **No warm leads** | Self-serve tiers, no enterprise sales dependency |
| **Minimal support** | Explicit SLA caps, no custom parser dev |
| **Annual OK** | Annual-first pricing model |
| **Paid pilots** | $1,000 creditable pilot, no free extended trials |
| **Avoid custom parser trap** | Finance = consultant-delivered only |

---

## 12. Decision Log

| Decision | Rationale |
|----------|-----------|
| DFIR first | Urgent buyers, short sales cycle, cashflow wedge |
| Annual-first | Predictable revenue, lower churn |
| Paid pilots | Filter tire-kickers, avoid custom parser trap |
| Finance = P3 | High-touch, no warm leads, consultant-delivered |
| No free tier | Community support is still a cost; free evaluation only |

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Initial canonical pricing doc (DFIR-first, annual-first) |

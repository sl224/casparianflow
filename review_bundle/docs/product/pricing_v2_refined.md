# Casparian Flow - Pricing Strategy v2.0 (Refined)

**Version:** 2.0
**Status:** Refined through Adversarial Review
**Date:** January 13, 2026
**Parent:** specs/pricing.md (v1.0)
**Review Process:** Bull/Bear adversarial debate

---

## 1. Executive Summary: Post-Debate Synthesis

This document refines the original pricing strategy after rigorous adversarial review. Two reviewers with opposing perspectives debated the strategy, surfacing critical assumptions and risks.

### Key Findings from the Debate

| Issue | Bullish Position | Bearish Position | **Synthesis** |
|-------|------------------|------------------|---------------|
| Value capture % | 10-15% is conservative | 1-5% is realistic for startups | **Start at 5-8%, earn 10-15%** |
| Pricing tier | $15K/year is the floor | $15K is death zone | **Staged pricing: Land at $3K/mo, expand to $15K+** |
| Validation | Hypothesis worth testing | Zero evidence | **90-day validation sprint required** |
| Moat durability | Unreplicable architecture | 6-month feature copy | **12-24 month window, not permanent** |
| Vertical focus | Five verticals in parallel | One vertical only | **Lead with Finance, expand with traction** |

### The Central Truth

**Both reviewers agreed on one thing:** The original pricing strategy is 100% unvalidated theory.

The path forward is not to defend or attack the pricing—it's to **test it rapidly with real customers** and adjust based on data, not debate.

---

## 2. Revised Pricing Philosophy

### 2.1 From "Value Capture" to "Earned Trust Pricing"

**Original assumption:** Capture 10-15% of value from Day 1.

**Revised assumption:** Earn the right to capture 10-15% through demonstrated value.

| Stage | Value Capture | What You've Earned |
|-------|---------------|-------------------|
| **Pilot** | 0% (free) | Nothing yet |
| **Land** | 3-5% | Product works; solves immediate pain |
| **Expand** | 5-8% | Trusted; integrated into workflow |
| **Enterprise** | 10-15% | Strategic; compliance-critical; switching costs high |

This is not "underpricing"—it's **pricing that matches relationship depth**.

### 2.2 Staged Pricing Model

**The $15K problem:** The Bearish reviewer correctly identified that $15K/year is "no-man's land"—too expensive for self-serve, too cheap for enterprise sales.

**Solution:** Three-stage pricing that moves customers up a ladder:

```
Stage 1: LAND ($2-3K/month)
         ↓
         Prove value, build trust, get reference
         ↓
Stage 2: EXPAND ($6-10K/month)
         ↓
         Add users, sources, verticals within org
         ↓
Stage 3: ENTERPRISE ($15K+/month)
         ↓
         Full platform, SSO, SLA, dedicated success
```

This matches pricing to where the customer IS, not where you wish they were.

---

## 3. Revised Pricing Tiers (All Verticals)

**System of record:** This document is the single source of truth for pricing tiers, unit definitions, and vertical mappings. Other strategy docs should reference this section instead of restating tier tables.

### 3.1 Universal Tier Structure

| Tier | Monthly | Annual | Target | Sales Motion |
|------|---------|--------|--------|--------------|
| **Free** | $0 | $0 | Evaluation, individuals | Self-serve |
| **Team** | $500/user/month | $6,000/user/year | Small teams (2-5 users) | Self-serve + support |
| **Department** | $2,500/month | $30,000/year | Department-level (5-20 users) | Inside sales |
| **Enterprise** | $8,000+/month | $96,000+/year | Organization-wide | Field sales |

### 3.1.1 Unified Tier Mapping (Universal → Vertical)

Use this mapping to keep vertical tiers consistent with the universal ladder. Defense remains an exception due to deployment-based procurement.

| Universal Tier | Finance | Legal | Healthcare | Defense |
|---------------|---------|-------|------------|---------|
| **Free** | Free | Free | Community | Open Source |
| **Team** | Analyst | Solo | Clinic | Tactical (annual, per deployment) |
| **Department** | Team | Firm | Department | Mission (annual, per deployment) |
| **Enterprise** | Trading Desk / Enterprise | Litigation Team / Enterprise | Hospital / Health System | Program (annual, per program) |

### 3.1.2 Vertical Addenda (What Changes by Vertical)

| Vertical | Pricing Unit | Primary Exception | Notes |
|----------|--------------|-------------------|-------|
| **Finance** | Per month (team/desk) | None | Trading desk pricing anchors expansion. |
| **Legal** | Per month (firm/team) | None | Volume limits define Solo/Firm tiers. |
| **Healthcare** | Per month (clinic/department/hospital) | None | BAA and audit features gate Hospital+. |
| **Defense** | Annual (per deployment/program) | Deployment-based procurement | Defense stays on deployment/program pricing. |

### 3.2 Finance Vertical (Revised)

**Original:** $15,000/desk/year (10% value capture)

**Revised:**

| Tier | Price | Value Capture | Sales Motion |
|------|-------|---------------|--------------|
| **Free** | $0 | 0% | EDGAR parser, 5 files/day |
| **Analyst** | $300/user/month | ~2% | FIX parsing, email support |
| **Team** | $2,000/month | ~4% | 5 users, multi-venue, priority support |
| **Trading Desk** | $6,000/month | ~8% | Unlimited users, custom tags, SLA |
| **Enterprise** | $15,000+/month | ~15% | Multi-desk, SSO, dedicated success |

**Rationale:** Start at $2K/month to prove value. Earn the right to $15K through demonstrated ROI and expanded usage.

### 3.3 Legal Vertical (Revised)

**Original:** $20,000/litigation team/year (10% value capture)

**Revised:**

| Tier | Price | Value Capture | Sales Motion |
|------|-------|---------------|--------------|
| **Free** | $0 | 0% | 3 parsers, 1GB/month |
| **Solo** | $200/month | ~2% | PST + load files, 10GB/month |
| **Firm** | $1,500/month | ~5% | Multi-custodian, 100GB/month |
| **Litigation Team** | $5,000/month | ~10% | Unlimited volume, export, support |
| **Enterprise** | $10,000+/month | ~15% | Multi-office, SSO, white-label |

**Rationale:** Law firms are conservative buyers. Lower entry price reduces friction; expand with case volume.

### 3.4 Defense Vertical (Revised)

**Original:** $50,000-150,000/deployment/year

**Revised:** Keep original pricing. Defense is the exception.

| Tier | Price | Rationale |
|------|-------|-----------|
| **Open Source** | Free | Community, evaluation |
| **Tactical** | $50,000/deployment/year | Air-gapped is unique; defense expects enterprise pricing |
| **Mission** | $150,000/deployment/year | 24x7 support, custom parsers |
| **Program** | $500,000+/year | SBIR Phase III, multi-site |

**Rationale:** The Bullish reviewer was right here—defense buyers expect and budget for enterprise pricing. Low prices signal "not serious" to DoD procurement.

### 3.5 Healthcare Vertical (Revised)

**Original:** $25,000/hospital/year

**Revised:**

| Tier | Price | Value Capture | Sales Motion |
|------|-------|---------------|--------------|
| **Community** | $0 | 0% | HL7 ADT parser, 1K messages/day |
| **Clinic** | $500/month | ~3% | All HL7 parsers, 10K messages/day |
| **Department** | $2,000/month | ~6% | Unlimited volume, audit logs |
| **Hospital** | $6,000/month | ~12% | HIPAA BAA, schema governance |
| **Health System** | $15,000+/month | ~20% | Multi-facility, SSO, dedicated team |

**Rationale:** Healthcare sales cycles are 12-18 months. Lower entry pricing reduces time-to-close for department-level pilots.

---

## 4. The Validation Sprint (90 Days)

### 4.1 The Central Problem

**Both reviewers agreed:** This strategy has zero customer validation. Every number is theory.

**Solution:** 90-day validation sprint before committing to pricing.

### 4.2 Sprint Structure

| Week | Activity | Success Criteria |
|------|----------|------------------|
| **1-4** | 20 cold conversations (Finance only) | 10+ completed conversations |
| **5-6** | 3 pilot installations (free) | 3 design partners committed |
| **7-10** | Pilots running, feedback collection | Weekly usage + feedback calls |
| **11-12** | Conversion attempt (50% discount) | 2+ pilots willing to pay |

### 4.3 What We're Testing

| Hypothesis | Test | Kill Criteria |
|------------|------|---------------|
| Pain is real | "Tell me about file chaos" | <5/10 conversations show pain |
| Buyer exists | "Who would buy this?" | No clear budget owner identified |
| Price is viable | "Would you pay $2K/month?" | <3/10 say yes |
| Value is delivered | Pilot usage data | <50% weekly active in pilots |

### 4.4 Decision Matrix at Day 90

| Outcome | Action |
|---------|--------|
| 0 design partners | Kill or major pivot |
| 1-2 design partners, no willingness to pay | Lower price, extend validation |
| 3+ design partners, 2+ willing to pay | Proceed with staged pricing |
| 5+ willing to pay at full price | Consider raising prices |

---

## 5. Concessions from the Debate

### 5.1 Bearish Arguments We Accept

| Argument | Our Response |
|----------|--------------|
| "Zero validation" | Agreed. 90-day sprint required. |
| "Pricing no-man's land" | Agreed. Staged pricing solves this. |
| "Pricing out early adopters" | Agreed. $200-500/month tier added. |
| "Five-front war" | Agreed. Finance first, others follow. |
| "Moat is not permanent" | Agreed. 12-24 month window, not forever. |

### 5.2 Bullish Arguments We Accept

| Argument | Our Response |
|----------|--------------|
| "Underpricing signals desperation" | Agreed. Free tier exists, but paid tiers are premium. |
| "Air-gapped IS differentiated" | Agreed. Especially for Defense. |
| "Enterprise buyers expect enterprise pricing" | Agreed. Defense keeps original pricing. |
| "Price is adjustable" | Agreed. Start lower, earn higher. |
| "Parsers are IP" | Agreed. But IP doesn't automatically = high prices. |

---

## 6. Risk Mitigation (Updated)

### 6.1 Kill Risks (from Bearish Review)

| Risk | Probability | Mitigation |
|------|-------------|------------|
| **Cash runway exhaustion** | 40% | Staged pricing = faster closes = better cash flow |
| **Price-quality signal mismatch** | 30% | Match expectations: lower price = lower expectations |
| **Incumbent bundling** | 30% | Speed. Ship faster than they can copy. |
| **Pricing out early adopters** | 20% | $200-500/month tier for individuals |
| **Vertical strategy paralysis** | 40% | Finance ONLY for first 6 months |

### 6.2 Mitigation: Single Vertical Focus

**Original plan:** Five verticals in 24 months.

**Revised plan:** Finance for 6 months. Legal at Month 6 if Finance works. Others follow.

| Month | Vertical | Criteria to Add Next |
|-------|----------|---------------------|
| 0-6 | Finance only | $50K ARR from Finance |
| 6-12 | + Legal | $100K ARR combined |
| 12-18 | + Healthcare OR Defense | $250K ARR combined |
| 18-24 | + Manufacturing | $500K ARR combined |

---

## 7. Revenue Projections (Revised)

### 7.1 Conservative (Staged Pricing, Finance Focus)

| Quarter | Customers | Avg MRR | Total ARR |
|---------|-----------|---------|-----------|
| Q1 | 3 pilots (free) | $0 | $0 |
| Q2 | 5 paid (Team) | $2,000 | $120,000 |
| Q3 | 10 paid (mix) | $2,500 | $300,000 |
| Q4 | 20 paid (mix) | $3,000 | $720,000 |

**Year 1 ARR (revised):** $300,000-500,000 (vs. $1M original estimate)

### 7.2 Why Lower is Better

The original $1M ARR target required:
- 67 customers at $15K each
- Enterprise sales motion (6-month cycles)
- $400K+ sales team investment

The revised target requires:
- 20-30 customers at $2-5K/month
- Inside sales + self-serve motion
- $100-150K sales investment

**Lower revenue target, higher probability of achievement.**

---

## 8. Updated Pricing Page Strategy

### 8.1 What to Show

```
FREE          TEAM           DEPARTMENT      ENTERPRISE
$0            $500/user/mo   $2,500/mo       Custom
              $5K/user/yr    $25K/yr

- 3 parsers   - Unlimited    - All Team      - All Dept
- 100 files   - Priority     - 20 users      - SSO/SAML
- Community   - Email        - SLA           - Dedicated CSM
                                             - Custom parsers
```

### 8.2 What NOT to Show

- Don't show Defense pricing (custom conversations only)
- Don't show Enterprise pricing (call us)
- Don't apologize for pricing
- Don't compare to competitors (let them discover we're cheaper)

---

## 9. Decision Log

| Decision | Original | Revised | Rationale |
|----------|----------|---------|-----------|
| Value capture target | 10-15% | 5-8% (earn to 10-15%) | Match relationship depth |
| Entry price (Finance) | $15K/year | $2K/month | Reduce friction, prove value |
| Vertical focus | 5 verticals parallel | Finance first | Avoid strategy paralysis |
| Defense pricing | $50K-150K | Keep original | Defense is exception |
| Validation | Implicit | 90-day sprint | Address zero-evidence problem |
| Revenue target (Y1) | $1M+ | $300-500K | Lower target, higher probability |

---

## 10. Open Questions (Remaining)

1. **What's the minimum viable support for paid tiers?** Can we do email-only at Team tier?
2. **Should Annual discounts be 15% or 20%?** Need to test.
3. **Is $500/user/month too high for Team?** May need $300/user/month tier.
4. **What triggers upsell from Team to Department?** Usage thresholds? Manual?
5. **How do we handle pilot-to-paid conversion?** Auto-bill or re-contract?

---

## 11. Next Steps

### Immediate (This Week)
- [ ] Finalize pilot program terms (free, 30 days)
- [ ] Create landing page with revised pricing
- [ ] Draft outreach email for Finance persona
- [ ] Schedule 20 cold conversations

### Week 1-4
- [ ] Execute 20 conversations
- [ ] Document willingness-to-pay feedback
- [ ] Identify 3 pilot candidates
- [ ] Adjust pricing based on conversation feedback

### Week 5-12
- [ ] Run pilots
- [ ] Collect usage data
- [ ] Attempt conversion at 50% discount
- [ ] Make final pricing decision based on conversion data

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Original pricing spec |
| 2026-01-13 | 2.0 | Post-adversarial review refinement: staged pricing model, single vertical focus, 90-day validation sprint, revised revenue targets |

---

## Appendix A: Debate Summary

### Bullish Reviewer's Strongest Points
1. Underpricing is more dangerous than overpricing (cash flow, signals)
2. Air-gapped + local-first is genuinely differentiated
3. Defense buyers expect and budget for enterprise pricing
4. "Theory requires validation" is not an argument against the theory

### Bearish Reviewer's Strongest Points
1. Zero validation makes all numbers fiction
2. $15K is "no-man's land" between self-serve and enterprise
3. Early adopters priced out = no references, no testimonials
4. Five-front vertical war is unwinnable for a startup

### Synthesis
Both reviewers agreed the strategy is worth testing. The refinement:
- Keeps the value-based philosophy
- Adds staged pricing to reduce friction
- Requires validation before commitment
- Focuses on one vertical to start

**The market will tell us who's right.**

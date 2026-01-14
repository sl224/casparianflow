# Casparian Flow - Value-Based Pricing Specification

**Version:** 1.0
**Status:** Draft - Pending Refinement
**Date:** January 13, 2026
**Parent:** STRATEGY.md (Value-Based Pricing Strategy section)

---

## 1. Executive Summary

This specification defines Casparian Flow's value-based pricing strategy, grounded in Marc Andreessen's framework: **price by the value created for the customer, not by cost.** Enterprise software should capture **10-20% of the value created**.

**Core Problem:** Initial pricing ($50-100/user/month) captured <2% of value created, leaving 90%+ on the table and signaling "not enterprise-grade" to buyers.

**Solution:** Vertical-specific pricing tiers that capture 10-15% of quantifiable value, validated through customer willingness to pay.

**Key Principle:** Higher prices are better for customers because they:
1. Fund R&D for faster product improvement
2. Enable dedicated customer success
3. Signal enterprise-grade quality
4. Prove the product has a real moat

---

## 2. Pricing Philosophy

### 2.1 The Andreessen Framework

Marc Andreessen's pricing principles applied to Casparian:

| Principle | Application |
|-----------|-------------|
| **Price by value, not cost** | Base prices on customer outcomes (time saved, risk reduced, costs avoided) |
| **Higher prices = faster growth** | Higher margins fund sales, R&D, and customer success |
| **Pricing proves the moat** | If customers pay $15K/year, differentiation is real |
| **Signal quality** | Enterprise buyers are suspicious of cheap software |
| **Fund the virtuous cycle** | Revenue → Better product → More value → Higher prices |

### 2.2 Value Capture Target

**Target:** Capture **10-15% of quantifiable value** created for customers.

**Rationale:**
- <5%: Leaving too much on the table; signaling low quality
- 10-15%: Fair exchange; sustainable business; premium positioning
- >20%: Risk of customer pushback; creates competitor opportunity

### 2.3 Anti-Patterns to Avoid

| Anti-Pattern | Problem | Alternative |
|--------------|---------|-------------|
| **Cost-plus pricing** | Ignores customer value; race to bottom | Value-based pricing |
| **Competitor matching** | Assumes competitors priced correctly | Price by our unique value |
| **"Affordable" positioning** | Signals low quality to enterprise | Premium positioning with free tier for evaluation |
| **Per-seat for all tiers** | Doesn't scale with value for teams | Per-desk/per-deployment for enterprise |
| **One price fits all** | Different verticals have different value | Vertical-specific pricing |

---

## 3. Value Analysis by Vertical

### 3.1 Finance (Trade Operations)

**Primary Value:** Trade break resolution time reduction

| Value Component | Quantification | Annual Value |
|-----------------|----------------|--------------|
| Trade Support Engineer time | 6+ hours/day × $75/hour × 250 days | **$112,500/year** |
| Settlement risk reduction | 1 failed settlement = $10K-100K | **$50,000+/year** (avoided) |
| Knowledge retention | Departing employee knowledge loss | **$50,000+** (avoided) |
| Audit trail compliance | Regulatory fine avoidance | **$100,000+** (avoided) |

**Total quantifiable value per trading desk:** $150,000-500,000/year

**Target price:** $15,000/desk/year (10% of conservative value estimate)

### 3.2 Legal (eDiscovery)

**Primary Value:** Vendor processing cost reduction

| Value Component | Quantification | Annual Value |
|-----------------|----------------|--------------|
| Vendor processing fees | $5-15K/matter × 20-50 matters | **$100,000-750,000/year** |
| Turnaround time | Days → hours (opportunity cost) | **$25,000+/year** |
| Data control | Sensitive data stays in-house | **Risk reduction** |
| Small matter profitability | Can take $5K matters profitably | **Revenue enablement** |

**Total quantifiable value per firm:** $100,000-800,000/year

**Target price:** $20,000/litigation team/year (10% of conservative value estimate)

### 3.3 Defense (Tactical Edge)

**Primary Value:** Unique capability (no alternative exists)

| Value Component | Quantification | Annual Value |
|-----------------|----------------|--------------|
| Alternative (Palantir) | $1M+/year; requires server | **N/A (can't run on laptop)** |
| Alternative (ArcGIS Enterprise) | $100K+/year; requires server | **N/A (can't run air-gapped)** |
| Alternative (custom Python) | Fragile; single point of failure | **$100K+/year** (hidden cost) |
| Mission-critical capability | Analyst productivity | **Incalculable** |

**Total value:** Mission-critical capability where no laptop-deployable alternative exists.

**Target price:** $50,000-150,000/deployment/year (captures unique capability premium)

### 3.4 Healthcare (HL7 Analytics)

**Primary Value:** IT backlog bypass + compliance automation

| Value Component | Quantification | Annual Value |
|-----------------|----------------|--------------|
| Interface project cost | $50,000-150,000 per project | **$100,000+/year** (avoided) |
| Interface Team wait time | 6+ months opportunity cost | **$50,000+** (avoided) |
| Compliance audit prep | $20,000-50,000/audit | **$20,000-50,000/year** |
| Research data extraction | $10,000-30,000/project | **$30,000+/year** |

**Total quantifiable value per hospital:** $150,000-300,000/year

**Target price:** $25,000/hospital/year (10-15% of conservative value estimate)

### 3.5 Manufacturing

**Primary Value:** Historian license displacement + analyst productivity

| Value Component | Quantification | Annual Value |
|-----------------|----------------|--------------|
| OSIsoft PI license | $100,000+ per plant | **$100,000+/year** (avoided) |
| Seeq analytics add-on | $50,000+/year | **$50,000+/year** (avoided) |
| Plant engineer time | 10+ hours/week × $80/hour | **$40,000+/year** |
| Downtime analysis | 1 hour reduced downtime | **$10,000+** (per incident) |

**Total quantifiable value per plant:** $150,000-300,000/year

**Target price:** $25,000/plant/year (10-15% of conservative value estimate)

---

## 4. Pricing Tiers by Vertical

### 4.1 Finance Vertical

| Tier | Price | Value Capture | Features |
|------|-------|---------------|----------|
| **Starter** | Free | N/A | EDGAR parser, 5 files/day, evaluation |
| **Professional** | $500/user/month | ~3% | FIX parsing, unlimited files, email support |
| **Trading Desk** | $15,000/desk/year | ~10% | Unlimited users per desk, multi-venue, priority support |
| **Enterprise** | $50,000+/year | Custom | Multi-desk, ISO 20022, SSO, dedicated success manager |

**Key insight:** "Trading Desk" pricing aligns with how operations budget, not per-seat.

### 4.2 Legal Vertical

| Tier | Price | Value Capture | Features |
|------|-------|---------------|----------|
| **Solo** | Free | N/A | 3 parsers, 1GB/month, evaluation |
| **Firm** | $500/user/month | ~5% | PST + load file, 100GB/month, email support |
| **Litigation Team** | $20,000/year | ~10% | Unlimited matters, multi-custodian, priority support |
| **Enterprise** | $75,000+/year | Custom | Multi-office, SSO, white-label, dedicated success |

**Key insight:** Fixed annual fee for Litigation Team eliminates per-matter anxiety.

### 4.3 Defense Vertical

| Tier | Price | Value Capture | Features |
|------|-------|---------------|----------|
| **Open Source** | Free | N/A | Core parsers, CLI, community support |
| **Tactical** | $50,000/deployment/year | ~5% | Air-gapped bundle, 5 formats, 8x5 support |
| **Mission** | $150,000/deployment/year | ~10% | All formats, 24x7 support, custom parsers |
| **Program** | $500,000+/year | Custom | Multi-site, dedicated team, SBIR Phase III |

**Key insight:** Defense buyers expect enterprise pricing. Low prices signal "not serious."

### 4.4 Healthcare Vertical

| Tier | Price | Value Capture | Features |
|------|-------|---------------|----------|
| **Community** | Free | N/A | HL7 ADT parser, 1,000 messages/day |
| **Clinic** | $250/month | ~3% | All HL7 parsers, 10K messages/day |
| **Hospital** | $25,000/year | ~10% | Unlimited volume, HIPAA BAA, audit logs |
| **Health System** | $100,000+/year | Custom | Multi-facility, SSO, on-prem, dedicated team |

**Key insight:** HIPAA BAA required for Hospital+ tiers; factor into cost structure.

### 4.5 MSP/SMB Channel

| Tier | Price | MSP Markup | End Client Cost |
|------|-------|------------|-----------------|
| **Per-Client** | $25-50/client/month | 3-5x | $75-200/month |
| **White-Label** | $2,500/month base | Full control | MSP sets price |

**Key insight:** MSP channel uses volume economics; different from enterprise direct.

---

## 5. Pricing Validation Framework

### 5.1 Validation Methodology

For each tier, validate pricing through:

1. **Willingness-to-pay interviews** (5-10 target customers per vertical)
2. **A/B test pricing pages** (if web-based signup)
3. **Sales conversation feedback** (track objections)
4. **Win/loss analysis** (why did we win or lose?)
5. **Churn correlation** (do higher prices = higher churn?)

### 5.2 Validation Questions

| Question | What It Reveals |
|----------|-----------------|
| "What would you pay for this?" | Anchoring (usually too low) |
| "What are you paying for alternatives?" | Reference price |
| "At what price would this be a no-brainer?" | Value perception |
| "At what price would you hesitate?" | Price ceiling |
| "How much time/money does this save?" | Value quantification |

### 5.3 Price Adjustment Triggers

**Raise prices when:**
- Win rate > 80% (too cheap)
- Customers say "that's it?" when told price
- Sales cycle < 30 days for enterprise
- NRR > 120% (customers expanding rapidly)

**Lower prices when:**
- Win rate < 30% (too expensive for value delivered)
- Sales cycle > 12 months for mid-market
- Churn > 15% annual (not delivering value)
- Customers reference cheaper alternatives winning

**Red flag:** If price objections disappear, you're underpriced.

---

## 6. Competitive Positioning

### 6.1 Price Position by Vertical

| Vertical | Competitor | Their Price | Our Price | Position |
|----------|-----------|-------------|-----------|----------|
| **Finance** | Bloomberg Terminal | $32,000/seat | $15,000/desk | 50% cheaper, more flexible |
| **Finance** | Enterprise TCA | $50,000+/year | $15,000/desk | 70% cheaper, different use case |
| **Legal** | Relativity | $150,000+/year | $20,000/year | 85% cheaper, pre-processing |
| **Legal** | Vendor processing | $100,000+/year | $20,000/year | 80% cheaper, in-house |
| **Defense** | Palantir | $1M+/year | $150,000/year | 85% cheaper, laptop-capable |
| **Healthcare** | Rhapsody | $100,000+/year | $25,000/year | 75% cheaper, analytics focus |
| **Manufacturing** | OSIsoft PI | $100,000+/plant | $25,000/year | 75% cheaper, no lock-in |

**Key insight:** Even at 10x current prices, we're still the "cheap" option vs. enterprise alternatives.

### 6.2 Moat Validation

Per Andreessen: "If you have a moat, customers will still buy, because they have to."

**Test the moat:** Raise prices and see if customers still buy.

| Moat Component | Validation Test |
|----------------|-----------------|
| **Premade parsers** | Will customers pay $15K for FIX parsing they'd otherwise build? |
| **Local-first** | Will defense buyers pay $50K for air-gapped capability? |
| **Schema contracts** | Will compliance teams pay premium for audit trails? |
| **Parser IP** | Do customers resist switching once parsers are tuned? |

---

## 7. Revenue Model

### 7.1 Revenue Projections (Value-Based Pricing)

**Year 1 Target:** $1,050,000 ARR (vs. $250,000 with original pricing)

| Vertical | Customers | Avg Price | ARR |
|----------|-----------|-----------|-----|
| Finance | 20 desks | $15,000 | $300,000 |
| Legal | 30 firms | $10,000 | $300,000 |
| Defense | 3 deployments | $50,000 | $150,000 |
| Healthcare | 10 hospitals | $15,000 | $150,000 |
| MSP/SMB | 50 accounts | $3,000 | $150,000 |
| **Total** | | | **$1,050,000** |

### 7.2 Unit Economics

| Metric | Original Pricing | Value-Based Pricing |
|--------|------------------|---------------------|
| **Average Contract Value** | $1,200/year | $12,000/year |
| **CAC (enterprise sales)** | $5,000 | $5,000 |
| **CAC Payback** | 4+ years | 5 months |
| **LTV (3-year retention)** | $3,600 | $36,000 |
| **LTV:CAC** | 0.7:1 | 7.2:1 |

**Key insight:** Value-based pricing makes enterprise sales motion viable.

### 7.3 Gross Margin Analysis

| Cost Component | Per-Customer Cost | At $1,200/year | At $12,000/year |
|----------------|-------------------|----------------|-----------------|
| Infrastructure | $50/month | 50% margin | 95% margin |
| Support (shared) | $20/month | 30% margin | 93% margin |
| Customer success | $200/month | -100% margin | 80% margin |

**Key insight:** Premium pricing enables customer success investment that drives retention.

---

## 8. Implementation Roadmap

### 8.1 Phase 1: Finance Validation (Months 1-3)

**Goal:** Validate $15,000/desk pricing with 5 trading desks

**Actions:**
- [ ] Identify 10 target trading desks (prop firms, broker-dealers)
- [ ] Conduct 5 willingness-to-pay interviews
- [ ] Offer $15,000/desk to 5 prospects
- [ ] Track close rate, objections, sales cycle

**Success criteria:**
- 3+ customers at $15,000/desk
- <60-day sales cycle
- Objection rate <50%

**Adjustment:** If close rate <20%, test $10,000/desk tier.

### 8.2 Phase 2: Legal Validation (Months 3-6)

**Goal:** Validate $20,000/litigation team pricing with 5 firms

**Actions:**
- [ ] Identify 15 target litigation support teams
- [ ] Conduct 5 willingness-to-pay interviews
- [ ] Offer $20,000/year to 5 prospects
- [ ] Track cost-comparison discussions (vs. vendors)

**Success criteria:**
- 3+ customers at $20,000/year
- Clear cost-savings narrative resonates
- Objection rate <50%

### 8.3 Phase 3: Defense Positioning (Months 6-12)

**Goal:** Position for $50,000+ deployments via SBIR

**Actions:**
- [ ] Submit SBIR Phase I at $150,000+ budget
- [ ] Position commercial pricing at $50,000/deployment
- [ ] Develop capability statement with pricing
- [ ] Track DoD buyer reaction to pricing

**Success criteria:**
- SBIR Phase I award
- No pushback on commercial pricing in discussions
- 2+ pilot users willing to budget $50,000/year

### 8.4 Phase 4: Healthcare Entry (Months 12-24)

**Goal:** Validate $25,000/hospital pricing with 5 hospitals

**Actions:**
- [ ] Defer until Finance + Legal revenue covers runway
- [ ] Identify 15 target hospitals
- [ ] Develop HIPAA BAA template
- [ ] Conduct willingness-to-pay interviews

**Success criteria:**
- 3+ customers at $25,000/year
- BAA process doesn't block sales
- <12-month sales cycle

---

## 9. Pricing Communication

### 9.1 Sales Positioning

**Finance:**
> "Casparian is $15,000/desk/year. That's the cost of one trade break that escalates to settlement failure. Most desks see 10+ breaks per day. You'll save that in the first week."

**Legal:**
> "Casparian is $20,000/year. Your last 100GB matter cost $15,000 in vendor processing alone. With Casparian, that's $0. The tool pays for itself on your first matter."

**Defense:**
> "Casparian is $50,000/deployment. There is no other tool that runs on a laptop, works air-gapped, and structures CoT/NITF/PCAP into SQL. The alternative is $100K+ custom development that breaks when the contractor leaves."

**Healthcare:**
> "Casparian is $25,000/year. Your last interface project cost $75,000 and took 8 months. Analysts can now query HL7 archives themselves, in hours. No Interface Team backlog."

### 9.2 Objection Handling

| Objection | Response |
|-----------|----------|
| "Too expensive" | "What's the cost of NOT having this? [Calculate value]" |
| "Competitor X is cheaper" | "What does X give you? [Feature comparison]" |
| "We need to try before buying" | "Free tier available. Let's set up a pilot." |
| "Our budget is $X" | "Let's find the tier that fits. [Offer appropriate tier]" |
| "Can you do per-seat?" | "Our value scales with the team, not individuals. Here's why..." |

### 9.3 Pricing Page Strategy

**Do:**
- Show value created before showing price
- Offer free tier for evaluation
- Use vertical-specific pricing pages
- Include ROI calculator

**Don't:**
- Hide pricing (enterprise buyers hate this)
- Only show per-seat pricing
- Compare to competitors directly on pricing page
- Apologize for pricing

---

## 10. Success Metrics

### 10.1 Pricing Health Metrics

| Metric | Target | Action if Off |
|--------|--------|---------------|
| **Win rate (qualified)** | 40-60% | <30%: lower price; >70%: raise price |
| **Sales cycle** | 30-90 days (mid-market) | >120 days: simplify, add free trial |
| **Price objection rate** | 30-50% | <20%: raise price; >60%: lower or add tier |
| **Discount frequency** | <20% of deals | >30%: pricing too high or wrong tier structure |
| **Avg discount depth** | <15% | >25%: raise list price, formalize discount policy |

### 10.2 Revenue Metrics

| Metric | 6-Month | 12-Month | 24-Month |
|--------|---------|----------|----------|
| **ARR** | $250,000 | $1,000,000 | $3,000,000 |
| **Avg Contract Value** | $8,000 | $12,000 | $15,000 |
| **Net Revenue Retention** | 100% | 110% | 120% |
| **Gross Margin** | 85% | 88% | 90% |
| **CAC Payback** | 8 months | 6 months | 4 months |

---

## 11. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Price too high for market** | High | Free tier for evaluation; start at lower end of range |
| **Enterprise sales cycle too long** | High | MSP channel as faster alternative |
| **Competitors undercut** | Medium | Differentiate on value (features competitors don't have) |
| **Customers anchor on old pricing** | Medium | New verticals only see new pricing |
| **Sales team discounts too aggressively** | Medium | Formal discount policy; approval for >15% |
| **Free tier cannibalizes paid** | Low | Clear feature differentiation; usage limits |

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Pricing model** | Value-based, 10-15% capture | Andreessen framework; sustainable margins |
| **Vertical-specific pricing** | Yes | Different value = different price |
| **Free tier** | Yes, all verticals | Reduces friction; proves value |
| **Per-seat vs. per-desk** | Per-desk for enterprise | Aligns with customer value, not headcount |
| **Annual vs. monthly** | Annual for mid-market+ | Reduces churn; improves cash flow |
| **Published pricing** | Yes | Enterprise buyers prefer transparency |

---

## 13. Open Questions for Refinement

1. **Packaging:** Should "priority support" be separate add-on or bundled?
2. **Discounts:** What's the formal discount policy? (Annual prepay, multi-year, etc.)
3. **Grandfathering:** How do we handle existing customers at old pricing?
4. **Usage limits:** What are the specific limits for each tier?
5. **Overage pricing:** What happens when customers exceed tier limits?
6. **Implementation fees:** Should enterprise tiers include implementation or charge separately?
7. **Training:** Include training in tier or separate SKU?
8. **Custom parser development:** Price as fixed fee or time & materials?

---

## 14. Glossary

| Term | Definition |
|------|------------|
| **ACV** | Average Contract Value - average annual revenue per customer |
| **ARR** | Annual Recurring Revenue - total annual subscription revenue |
| **CAC** | Customer Acquisition Cost - cost to acquire one customer |
| **CAC Payback** | Months to recover CAC from customer revenue |
| **Gross Margin** | Revenue minus cost of goods sold, as percentage |
| **LTV** | Lifetime Value - total revenue from a customer over relationship |
| **LTV:CAC** | Ratio of lifetime value to acquisition cost; >3:1 is healthy |
| **MRR** | Monthly Recurring Revenue - total monthly subscription revenue |
| **NRR** | Net Revenue Retention - revenue retained + expansions from existing customers |
| **Value Capture** | Percentage of customer value captured in pricing |
| **Win Rate** | Percentage of qualified opportunities that convert to customers |

---

## 15. References

- Marc Andreessen on pricing (Elad Gil's High Growth Handbook)
- Marc Andreessen podcast quotes on value-based pricing
- [STRATEGY.md](../STRATEGY.md) - Parent strategy document
- [strategies/finance.md](../strategies/finance.md) - Finance vertical strategy
- [strategies/defense_tactical.md](../strategies/defense_tactical.md) - Defense vertical strategy
- [strategies/legal_ediscovery.md](../strategies/legal_ediscovery.md) - Legal vertical strategy
- [strategies/healthcare_hl7.md](../strategies/healthcare_hl7.md) - Healthcare vertical strategy

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial draft based on Andreessen framework analysis |

---

*This specification is pending refinement through the spec refinement workflow.*

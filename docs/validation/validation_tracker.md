# Casparian Flow - Validation Tracker

**Status:** Canonical
**Sprint Start:** _____________
**Sprint End:** _____________

---

## Vertical Tracks

| Track | Status | Target |
|-------|--------|--------|
| **DFIR (P0)** | Primary | 15 conversations, 5 pilots, $30K ARR |
| **eDiscovery (P1)** | Secondary | 8 conversations, 3 pilots, $20K ARR |
| **Defense (P2)** | Tertiary | 5 conversations, 1 pilot, SBIR application |

---

## Dashboard (Update Weekly)

### Week _____ Status

| Metric | DFIR Target | DFIR Actual | eDiscovery Target | eDiscovery Actual |
|--------|-------------|-------------|-------------------|-------------------|
| Outreach sent | 30 | | 15 | |
| Conversations completed | 4/week | | 2/week | |
| Pilots started | - | | - | |
| Pilots converting | - | | - | |

### Cumulative Progress

| Metric | Target | Week 1 | Week 2 | Week 3 | Week 4 | Total |
|--------|--------|--------|--------|--------|--------|-------|
| **DFIR Conversations** | 15 | | | | | /15 |
| DFIR Pain validated (3+) | 12 | | | | | /15 |
| DFIR Buyer validated (3+) | 8 | | | | | /15 |
| DFIR WTP validated (3+) | 8 | | | | | /15 |
| DFIR Pilot candidates | 5 | | | | | |
| **eDiscovery Conversations** | 8 | | | | | /8 |
| eDiscovery Pilot candidates | 3 | | | | | |

---

## Kill/Continue Criteria

### Week 4 Decision Point

| Outcome | Criteria | Action |
|---------|----------|--------|
| **KILL DFIR** | <8 conversations completed | Market misalignment |
| **KILL DFIR** | <4 pain scores of 3+ | Pain hypothesis wrong |
| **KILL DFIR** | 0 pilot candidates | No buyer exists |
| **PIVOT PRICING** | Pain exists but WTP <$500/year | Reprice dramatically |
| **CONTINUE** | 3+ pilot candidates, WTP >$1K/year | Start pilots |

### Week 12 Decision Point

| Outcome | Criteria | Action |
|---------|----------|--------|
| **KILL** | 0 pilots willing to pay | Pricing hypothesis wrong |
| **PIVOT** | WTP <$1K/year average | Lower price tier needed |
| **CONTINUE** | 2+ pilots convert at $1.2K+/year | Expand outreach |
| **ACCELERATE** | 4+ pilots convert | Add eDiscovery focus |

---

## Kill Criteria (Aligned with Constraints)

### "Minimal Support" Kill Signals

| Signal | Threshold | Action |
|--------|-----------|--------|
| Prospect expects custom parser dev | >30% of conversations | Messaging failure or wrong audience |
| Support requests per pilot | >5 per 30 days | Product gaps or wrong persona |
| "Can you just do it for me" | >20% of conversations | Wrong audience |

### "Paid Pilots" Kill Signals

| Signal | Threshold | Action |
|--------|-----------|--------|
| Refuses $1K pilot fee | >50% of qualified prospects | Reprice pilot or validate value |
| Expects free extended trial | >30% of conversations | Messaging failure |

---

## DFIR Conversation Log

### Conversation #1
```
Date: _______________
Name: _______________
Company: _______________
Title: _______________
Source: [ ] Cold email [ ] LinkedIn [ ] Referral [ ] DFIR Discord [ ] Other

SCORES:
Pain: ___/5    Buyer: ___/5    WTP: ___/5    Fit: ___/5

KEY DATA:
- Cases per month: _____
- Time per case on parsing: _____ hours
- Evidence documentation: [ ] Systematic [ ] Ad-hoc [ ] None
- Budget authority: [ ] Yes [ ] Influence [ ] No
- Unprompted WTP: $_____/year
- Reaction to $1.2-4.8K: [ ] High [ ] Fair [ ] Low

OUTCOME: [ ] Pilot candidate [ ] Follow up later [ ] Not a fit

NOTES:
_______________________________________________

KEY QUOTE:
"_______________________________________________"
```

*(Copy for conversations #2-15)*

---

## eDiscovery Conversation Log

### Conversation #1
```
Date: _______________
Name: _______________
Company: _______________
Title: _______________
Source: [ ] Cold email [ ] LinkedIn [ ] Referral [ ] ILTA [ ] Other

SCORES:
Pain: ___/5    Buyer: ___/5    WTP: ___/5    Fit: ___/5

KEY DATA:
- Productions per month: _____
- Production errors per month: _____
- Budget authority: [ ] Yes [ ] Influence [ ] No
- Unprompted WTP: $_____/year
- Reaction to $1.8-7.2K: [ ] High [ ] Fair [ ] Low

OUTCOME: [ ] Pilot candidate [ ] Follow up later [ ] Not a fit

NOTES:
_______________________________________________

KEY QUOTE:
"_______________________________________________"
```

*(Copy for conversations #2-8)*

---

## Pilot Tracker

### DFIR Pilot #1
```
Company: _______________
Contact: _______________
Start Date: _______________
Pilot Fee: [ ] Paid $1,000

TIMELINE:
[ ] Day 0: Onboarding complete
[ ] Day 14: Check-in complete
[ ] Day 25: Pricing conversation complete
[ ] Day 30: Decision made

USAGE METRICS:
- Files processed: _____
- Parsers used: _____
- Active days: _____/30
- Quarantine rows handled: _____

PRICING CONVERSATION:
- Value articulated: $_____/year
- Price presented: $_____/year
- Reaction: _______________
- Objections: _______________

OUTCOME:
[ ] Converted at $_____/year
[ ] Lost - reason: _______________

EXIT INTERVIEW (if lost):
- Primary blocker: _______________
- Price they would pay: $_____
- Feature gap: _______________
```

*(Copy for DFIR pilots #2-5 and eDiscovery pilots #1-3)*

---

## Pattern Recognition

### Pain Patterns (Update as you hear them)

| Pattern | DFIR Frequency | eDiscovery Frequency | Quote |
|---------|----------------|----------------------|-------|
| Scripts crash on edge cases | /15 | | "" |
| No chain of custody documentation | /15 | | "" |
| Silently dropped rows | /15 | | "" |
| Can't reproduce old runs | /15 | | "" |
| Production validation failures | | /8 | "" |
| Missing native files | | /8 | "" |
| BATES sequence errors | | /8 | "" |

### Objection Patterns

| Objection | Frequency | Response That Worked |
|-----------|-----------|---------------------|
| "I can write my own scripts" | | |
| "Plaso/Velociraptor already does this" | | |
| "Too expensive" | | |
| "Need IT approval" | | |
| "Expects custom parser dev" | | |

### Willingness-to-Pay Distribution

| Range | DFIR Count | eDiscovery Count |
|-------|------------|------------------|
| "Free only" | /15 | /8 |
| $500-1,000/yr | /15 | /8 |
| $1,000-2,500/yr | /15 | /8 |
| $2,500-5,000/yr | /15 | /8 |
| $5,000+/yr | /15 | /8 |

**DFIR Median WTP:** $_____/year
**eDiscovery Median WTP:** $_____/year

---

## Weekly Retrospective

### Week _____

**What worked:**
-
-
-

**What didn't work:**
-
-
-

**Surprises:**
-
-
-

**Adjustments for next week:**
-
-
-

**Confidence level (1-10):** _____

---

## Final Sprint Report (Week 12)

### Summary Statistics

| Metric | DFIR Result | eDiscovery Result |
|--------|-------------|-------------------|
| Total outreach sent | | |
| Total conversations | /15 | /8 |
| Conversation → pilot rate | % | % |
| Pilots started | | |
| Pilots completed | | |
| Pilots converted | | |
| Conversion rate | % | % |
| Average WTP | $/yr | $/yr |
| Total ARR signed | $ | $ |

### Hypothesis Validation

| Hypothesis | Status | Evidence |
|------------|--------|----------|
| DFIR pain is real | [ ] Validated [ ] Invalidated | |
| DFIR buyer exists | [ ] Validated [ ] Invalidated | |
| $1.2K/year DFIR is viable | [ ] Validated [ ] Invalidated | |
| $4.8K/year DFIR is viable | [ ] Validated [ ] Invalidated | |
| eDiscovery preflight pain exists | [ ] Validated [ ] Invalidated | |
| Paid pilot filters tire-kickers | [ ] Validated [ ] Invalidated | |

### Next Steps

**If DFIR validated:**
- [ ] Expand DFIR outreach
- [ ] Add eDiscovery focus
- [ ] Build [feature]

**If DFIR invalidated:**
- [ ] Pivot to eDiscovery primary
- [ ] Lower prices to [amount]
- [ ] Re-evaluate vertical priority

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 2.0 | Rewritten for DFIR-first with eDiscovery secondary track |

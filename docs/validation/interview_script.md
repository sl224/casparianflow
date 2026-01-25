# Casparian Flow - Validation Interview Script

**Status:** Canonical
**Date:** January 2026
**Duration:** 15-20 minutes
**Purpose:** Validate pain, buyer, and willingness-to-pay for DFIR vertical

---

## Pre-Call Prep (2 min)

- [ ] Research company (size, type, recent news)
- [ ] Check LinkedIn (prospect's role, tenure)
- [ ] Note any warm intro context
- [ ] Have pricing sheet ready (don't show unless asked)
- [ ] Recording permission ready

---

## Opening (2 min)

### Start Recording
> "Do you mind if I record this for my notes? Won't be shared externally."

### Set Context
> "Thanks for taking the time. I'm [name], building Casparian Flow.
> We help IR teams add governance to artifact parsing - lineage, quarantine, reproducibility.
>
> Before we go further - I'm not here to pitch you. I'm genuinely trying
> to understand how teams handle evidence processing today.
>
> Is it okay if I ask a few questions about your workflow?"

---

## Section 1: Pain Discovery (5 min)

### Q1: Current State
> "Walk me through what happens when you get a case folder with EVTX files, registry hives, and other artifacts.
> Step by step, what do you do?"

**Listen for:**
- Tools used (Python scripts, Plaso, Velociraptor, custom tools)
- Pain points (crashes, silent failures, documentation)
- Frequency (cases per week/month)
- Time spent on parsing vs. analysis

### Q2: Evidence Integrity
> "How do you document what you parsed and with what tool version?
> If opposing counsel asked 'prove this output came from that file,' what would you show them?"

**Listen for:**
- Ad-hoc documentation vs. systematic tracking
- Confidence level in reproducibility
- Prior issues with chain of custody

### Q3: Edge Cases
> "What happens when a parsing script crashes on a corrupted file?
> Do you have examples of artifacts that caused problems?"

**Listen for:**
- Lost data stories
- Workarounds used
- Time lost to debugging

### Q4: Prior Solutions
> "Have you tried anything to improve this? Other tools, processes, scripts?"

**Listen for:**
- What they've tried (and why it failed)
- Budget spent previously
- Appetite for new tools

---

## Section 2: Buyer Identification (3 min)

### Q5: Decision Process
> "If you found a tool that added governance to artifact parsing - lineage, quarantine, reproducibility -
> what would the process look like to get it approved?"

**Listen for:**
- Their authority level
- Who else is involved
- Budget cycle
- Approval timeline

### Q6: Budget Reality
> "Is there a budget for forensic tooling, or does it come out of general IT?"

> "Roughly what range - under $5K/year, $5-15K, or above $15K?"

**Listen for:**
- Budget owner
- Budget size
- Procurement process

---

## Section 3: Solution Validation (3 min)

### Q7: Feature Relevance
> "Let me describe what we built in 30 seconds and you tell me if it's
> relevant to your workflow...
>
> We add governance to artifact parsing. Every output row has source hash,
> job ID, and parser version. Quarantine catches malformed records.
> Same inputs + same parser = identical outputs, guaranteed.
>
> Does that sound like it would help, or is the problem somewhere else?"

**Listen for:**
- "Yes, that's exactly it" (strong signal)
- "Sort of, but we also need X" (feature gap)
- "Our problem is different" (pivot needed)

### Q8: Differentiation Check
> "Is there anything like this you're using today?"

**Listen for:**
- Competitor names
- DIY solutions
- "Nothing" (good sign)

---

## Section 4: Willingness to Pay (5 min)

### Q9: Value Framing
> "You mentioned your team handles [X] cases per month.
> If a tool saved you [X hours] per case in parsing time and eliminated
> the 'prove your work' documentation burden, what would that be worth?"

**Let them do the math. Don't anchor.**

### Q10: Direct WTP Question
> "If this worked as described, what would you expect to pay for it?"

**Listen for:**
- Specific number (great)
- "I don't know" (probe further)
- "It depends" (on what?)

### Q11: Price Reaction (if they give a number)
> "Interesting. We're thinking about pricing around [$1,200-4,800/year]
> for a Solo to Team license. Does that feel high, low, or about right?"

**Listen for:**
- "That's reasonable" (good)
- "That's high" (probe: what would be fair?)
- "That's cheap" (consider raising)

### Q12: Budget Fit
> "At that price point, is that something you could approve, or would
> it need to go higher?"

---

## Section 5: Close (2 min)

### If Strong Interest
> "Based on what you've shared, I think we might be able to help.
>
> We're running a paid pilot program - $1,000 for 30 days, credits toward
> annual if you convert. You get full access, we get your feedback.
>
> Would you be open to being one of our pilot partners?"

### If Lukewarm
> "Thanks for the honest feedback. It sounds like [summarize their situation].
>
> Would it be okay if I followed up in [3-6 months] to see if anything
> has changed?"

### If Not a Fit
> "Thanks for the time. It sounds like your workflow is different from
> what we're solving for.
>
> Is there anyone else in your network who deals with artifact parsing
> that I should talk to?"

---

## Post-Call (5 min)

### Capture Immediately

```
Date: _______________
Prospect: _______________
Company: _______________
Title: _______________

PAIN SCORE (1-5): _____
- Cases per month: _____
- Time per case on parsing: _____ hours
- Evidence documentation: [ ] Systematic [ ] Ad-hoc [ ] None

BUYER SCORE (1-5): _____
- Budget authority: [ ] Yes [ ] Influence [ ] No
- Budget range: $_____
- Decision timeline: _____

WTP SCORE (1-5): _____
- Unprompted WTP: $_____/year
- Reaction to $1.2-4.8K: [ ] High [ ] Fair [ ] Low
- Budget fit: [ ] Yes [ ] Maybe [ ] No

SOLUTION FIT (1-5): _____
- Feature relevance: [ ] High [ ] Medium [ ] Low
- Competitors mentioned: _____
- Missing features: _____

OUTCOME:
[ ] Pilot candidate (schedule onboarding)
[ ] Maybe later (follow up in ___ months)
[ ] Not a fit (reason: _____________)
[ ] Referral given (name: _____________)

KEY QUOTES:
"_________________________________"
"_________________________________"
"_________________________________"
```

---

## Scoring Rubric

### Pain Score
| Score | Criteria |
|-------|----------|
| 5 | "Scripts crash regularly," no evidence documentation, high stakes cases |
| 4 | Some crashes, ad-hoc documentation, concerned about chain of custody |
| 3 | Occasional issues, documentation exists but tedious |
| 2 | Rare problems, mostly satisfied with current workflow |
| 1 | "Not really a problem for us" |

### Buyer Score
| Score | Criteria |
|-------|----------|
| 5 | Direct budget authority, $5K+ available now |
| 4 | Influence + knows budget owner, $1-5K range |
| 3 | Can champion internally, budget unclear |
| 2 | Individual contributor, limited influence |
| 1 | No authority, no path to authority |

### WTP Score
| Score | Criteria |
|-------|----------|
| 5 | Unprompted >$3K/year, "that's reasonable" at $4.8K |
| 4 | Unprompted $1-3K/year, accepts $1.2-2.4K range |
| 3 | "Depends on value," open to $1K+ range |
| 2 | Pushback on any price, wants free |
| 1 | "We'd never pay for this" |

### Pilot Candidate Threshold
**Minimum scores to proceed:**
- Pain: 3+
- Buyer: 3+
- WTP: 3+
- Solution Fit: 4+

---

## Red Flags (Disqualify)

- [ ] "We already solved this" (no pain)
- [ ] "I just want to see a demo" (tire-kicker)
- [ ] "Send me a proposal" without engagement (not real)
- [ ] Can't articulate their workflow (doesn't own the problem)
- [ ] "Price doesn't matter" (not a real buyer)
- [ ] Keeps rescheduling (not prioritized)
- [ ] Expects custom parser development (support trap)

---

## Question Bank (If Time Permits)

### Deeper Pain Questions
- "What's the worst case you've dealt with in terms of parsing problems?"
- "How does your team feel about the current evidence documentation workflow?"
- "What would you do with the time back if parsing was more reliable?"

### Competitive Questions
- "How did you evaluate [competitor] and why didn't you go with them?"
- "What's stopping you from building better tooling internally?"

### Expansion Questions
- "Are there other people in your firm with similar problems?"
- "Would this need to work across multiple case types?"

### Reference Questions
- "If this works, would you be willing to be a reference?"
- "Do you know others in the industry dealing with this?"

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 2.0 | Rewritten for DFIR-first (file-based ingest, manifest, quarantine, reproducibility) |

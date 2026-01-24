# Casparian Flow - Validation Interview Script

**Version:** 1.0
**Date:** January 13, 2026
**Duration:** 15-20 minutes
**Purpose:** Validate pain, buyer, and willingness-to-pay

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
> We help trading ops teams query FIX logs with SQL.
>
> Before we go further - I'm not here to pitch you. I'm genuinely trying
> to understand how teams handle trade break resolution today.
>
> Is it okay if I ask a few questions about your workflow?"

---

## Section 1: Pain Discovery (5 min)

### Q1: Current State
> "Walk me through what happens when you get a trade break alert today.
> Step by step, what do you do?"

**Listen for:**
- Time spent (target: 30+ min per break)
- Tools used (grep, Excel, custom scripts)
- People involved (just them, or escalations)
- Frequency (per day, per week)

### Q2: Quantify the Pain
> "Roughly how many trade breaks does your team handle per day/week?"

> "And each one takes about how long to resolve?"

**Calculate silently:** breaks × time × hourly rate = annual cost

### Q3: Impact
> "What happens when a break takes too long to resolve?"

**Listen for:**
- Settlement failures
- Client complaints
- Regulatory issues
- Personal stress

### Q4: Prior Solutions
> "Have you tried anything to speed this up? Other tools, scripts, processes?"

**Listen for:**
- What they've tried (and why it failed)
- Budget spent previously
- Appetite for new tools

---

## Section 2: Buyer Identification (3 min)

### Q5: Decision Process
> "If you found a tool that cut trade break resolution from 45 min to 5 min,
> what would the process look like to get it approved?"

**Listen for:**
- Their authority level
- Who else is involved
- Budget cycle
- Approval timeline

### Q6: Budget Reality
> "Is there a budget for ops productivity tools, or does it come from IT?"

> "Roughly what range - under $10K/year, $10-50K, or above $50K?"

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
> We take your FIX logs and turn them into a SQL table called
> `fix_order_lifecycle`. You can query by ClOrdID, symbol, or time range.
> See the full order lifecycle - new order, fills, rejects, cancels -
> in one query.
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
> "You mentioned your team spends [X hours/week] on trade breaks.
> At a rough rate of [$75-100/hour], that's about [$X/year] in time.
>
> If a tool cut that by 80%, what would that be worth to you?"

**Let them do the math. Don't anchor.**

### Q10: Direct WTP Question
> "If this worked as described, what would you expect to pay for it?"

**Listen for:**
- Specific number (great)
- "I don't know" (probe further)
- "It depends" (on what?)

### Q11: Price Reaction (if they give a number)
> "Interesting. We're thinking about pricing around [$2,000-6,000/month]
> for a trading desk. Does that feel high, low, or about right?"

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
> We're running a pilot program - 30 days free, we help you get set up,
> you tell us what works and what doesn't.
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
> Is there anyone else in your network who deals with FIX log chaos
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
- Time per break: _____ min
- Breaks per week: _____
- Calculated annual cost: $_____

BUYER SCORE (1-5): _____
- Budget authority: [ ] Yes [ ] Influence [ ] No
- Budget range: $_____
- Decision timeline: _____

WTP SCORE (1-5): _____
- Unprompted WTP: $_____/month
- Reaction to $2-6K: [ ] High [ ] Fair [ ] Low
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
| 5 | >1 hour/break, daily occurrence, high stakes |
| 4 | 30-60 min/break, multiple per week |
| 3 | 15-30 min/break, weekly occurrence |
| 2 | <15 min/break, occasional |
| 1 | "Not really a problem for us" |

### Buyer Score
| Score | Criteria |
|-------|----------|
| 5 | Direct budget authority, $10K+ available now |
| 4 | Influence + knows budget owner, $5-10K range |
| 3 | Can champion internally, budget unclear |
| 2 | Individual contributor, limited influence |
| 1 | No authority, no path to authority |

### WTP Score
| Score | Criteria |
|-------|----------|
| 5 | Unprompted >$3K/mo, "that's reasonable" at $6K |
| 4 | Unprompted $1-3K/mo, accepts $2-4K range |
| 3 | "Depends on value," open to $1-2K range |
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

---

## Question Bank (If Time Permits)

### Deeper Pain Questions
- "What's the worst trade break you've dealt with recently?"
- "How does your team feel about the current workflow?"
- "What would you do with the time back if this was solved?"

### Competitive Questions
- "How did you evaluate [competitor] and why didn't you go with them?"
- "What's stopping you from building this internally?"

### Expansion Questions
- "Are there other teams in [Company] with similar problems?"
- "Would this need to work across multiple desks/offices?"

### Reference Questions
- "If this works, would you be willing to be a reference?"
- "Do you know others in the industry dealing with this?"

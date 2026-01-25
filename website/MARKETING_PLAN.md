# Casparian Flow - Marketing & Launch Plan

**Status:** Canonical
**Last Updated:** January 2026
**Purpose:** Marketing strategy for DFIR-first launch

---

## Executive Summary

**Product:** CLI tool that transforms DFIR artifacts into queryable SQL with governance (lineage, quarantine, reproducibility).

**Go-To-Market Strategy:** Vertical-by-vertical launch starting with DFIR (Incident Response), followed by eDiscovery preflight, and Defense flight test data.

**Target Buyers:**

| Priority | Vertical | Buyer Persona | Price Point | Status |
|----------|----------|---------------|-------------|--------|
| P0 | DFIR | Forensic Consultant / IR Lead | $1,200-$12,000/yr | **LAUNCH** |
| P1 | eDiscovery | Litigation Support Technologist | $1,800-$18,000/yr | Q2 2026 |
| P2 | Defense | Flight Test Data Processor | $24,000-$60,000/yr | Q3 2026 |
| P3 | Finance | Trade Support (via consultants) | Consultant-delivered | Deferred |

---

## Part 1: DFIR Vertical Launch (NOW)

### Target Persona: DFIR Consultant

**From validated strategy:**
- Job title: Forensic Engineer, IR Consultant, DFIR Lead, Detection Engineer
- Salary: $100K-$200K+ (boutique firms, consulting)
- Skills: Python, CLI, artifact parsing (Plaso, Velociraptor, custom scripts)
- Pain: Fragile scripts, no chain of custody, silent failures

**Decision maker:** Principal, Practice Lead, Managing Partner (boutique firms)

### Value Proposition

**Short (for ads/headlines):**
> "Evidence-grade artifact parsing. Lineage + Quarantine + Reproducibility."

**Long (for landing page):**
> "Turn your case folders into governed, queryable tables. Every row has source hash, job ID, and parser version. Quarantine catches the edge cases. Prove your work."

**Quantified ROI:**
- 2 hours/case × 20 cases/month = 40 hours on parsing
- Reduced to 0.5 hours/case = 10 hours
- **Time saved: 30 hours/month**
- At $150/hour consulting rate = **$4,500/month saved**
- Solo pricing ($1,200/year) pays for itself in <1 week

### Marketing Channels

#### 1. DFIR Community (Primary)

**Target communities:**
- DFIR Discord
- SANS DFIR Summit
- DFRWS attendees
- r/computerforensics
- r/dfir

**Community post format (not salesy):**
```
Title: "How do you document your artifact parsing for chain of custody?"

Body:
Working on case documentation and realized my current workflow
(Python scripts + manual notes) doesn't give me reproducible runs.

Curious how others handle:
- Tracking which parser version processed which file
- Documenting when malformed records are skipped
- Proving outputs match source files

(disclosure: building a tool for this, but genuinely curious
what workflows exist)
```

#### 2. LinkedIn Outreach (Secondary)

**Target search:**
- "DFIR Consultant"
- "Forensic Engineer"
- "Incident Response"
- "Digital Forensics"

**Connection message template:**
```
Hi [Name],

I noticed you're in DFIR at [Company]. We built a tool that adds
governance to artifact parsing — source hashes, lineage, quarantine.

Every output row traces back to the exact input file and parser version.

Would a 10-minute demo be useful? Runs locally, works air-gapped.

Best,
[Your name]
```

**Post-connection message (if accepted):**
```
Thanks for connecting!

Quick context: We built Casparian Flow for IR teams who want
reproducible, evidence-grade artifact parsing.

The pitch: instead of fragile Python scripts, you get governed
pipelines with lineage on every row and quarantine for edge cases.

Here's a 60-second demo: [VIDEO_LINK]

If "prove your parsing" matters for your practice, happy to chat.
```

#### 3. Direct Email to Pilot Prospects

**Cold email template:**
```
Subject: Evidence-grade artifact parsing for IR

Hi [Name],

I'm reaching out because [Company] likely deals with artifact
processing and chain of custody documentation.

We built Casparian Flow: a local CLI that adds governance to
artifact parsing.

- Every row has source hash, job ID, parser version
- Quarantine catches malformed records (no silent drops)
- Same inputs + same parser = identical outputs (reproducible)
- Works air-gapped on evidence servers

Would a 15-minute demo be useful?

Best,
[Your name]
Casparian Flow
```

**Follow-up (3 days later, no response):**
```
Subject: Re: Evidence-grade artifact parsing for IR

Hi [Name],

Following up — I know IR teams are busy with active engagements.

If timing is bad, here's a 60-second video showing the workflow:
[VIDEO_LINK]

Happy to connect whenever makes sense.

Best,
[Your name]
```

#### 4. Hacker News (Launch Moment)

**Only do this when:**
- [ ] Demo video is ready
- [ ] Download actually works
- [ ] You can monitor HN for 4-6 hours straight

**Post title options:**
- "Show HN: Governed artifact parsing for DFIR — lineage, quarantine, reproducibility"
- "Show HN: Casparian Flow – evidence-grade parsing for incident response"

**Post body:**
```
I built this for DFIR teams who need to prove their artifact processing.

The problem: Most artifact parsing uses Python scripts that crash on
edge cases, silently drop rows, and have no audit trail.

Casparian Flow adds governance:
- Source hash per input file
- Lineage columns on every output row
- Quarantine for malformed records
- Reproducible runs (same inputs + parser = identical outputs)

Runs locally (data never leaves your machine). Works air-gapped.

Demo: [VIDEO_LINK]
Download: [GITHUB_RELEASES_LINK]

I'm the developer. Happy to answer questions about DFIR workflows,
the architecture, or evidence handling.
```

### Launch Checklist: DFIR

**Pre-launch (before any outreach):**
```
[ ] Demo video recorded (60-90 seconds)
    - Show: scan case folder → run parser → query outputs → show lineage
    - Record with OBS or Loom
    - No fancy editing needed

[ ] GitHub Releases set up
    - macOS ARM64, macOS x64, Linux x64, Windows x64
    - Download links work

[ ] Stripe Payment Links created
    - Solo Monthly: $100/mo → Annual: $1,200/yr
    - Team Monthly: $400/mo → Annual: $4,800/yr
    - Enterprise Lite: $1,000/mo → Annual: $12,000/yr
    - Paid Pilot: $1,000 (30 days, credits to annual)

[ ] License key delivery process documented
    - Manual for now: receive Stripe notification → generate key → email

[ ] Plausible goals configured
    - Download click
    - Start Pilot click
    - Demo watched
```

**Launch week:**
```
Day 1: DFIR Discord introduction post
Day 2: LinkedIn outreach to 15 DFIR consultants
Day 3: LinkedIn outreach to 15 more
Day 4: Email pilot prospects (any warm leads)
Day 5: Review analytics, adjust messaging
```

**Post-launch:**
```
Week 2: Follow up with anyone who downloaded but didn't convert
Week 3: Collect feedback from first users
Week 4: If 5+ paying customers, consider HN Show launch
```

---

## Part 2: eDiscovery Preflight (Q2 2026)

### Target Persona: Litigation Support Technologist

**From validated strategy:**
- Job title: Litigation Support Tech, eDiscovery Processing Analyst
- Salary: $75K-$130K
- Skills: Load file formats (DAT/OPT/LFP), Relativity, SQL
- Pain: Production validation errors, vendor back-and-forth

**Decision maker:** Director of Litigation Support, Practice Group Leader

### Go-Live Criteria

eDiscovery goes live when:
```
[ ] Load file parsers tested on real DAT/OPT/LFP files
[ ] 30+ waitlist signups
[ ] Pricing validated with 3+ prospects
[ ] Demo video for preflight workflow recorded
[ ] DFIR ARR > $30K (cash flow established)
```

### Marketing Channels

**LinkedIn targets:**
- "Litigation Support"
- "eDiscovery Processing"
- "Legal Technology"

**Industry forums:**
- ILTA (International Legal Technology Association)
- ACEDS (Association of Certified E-Discovery Specialists)
- ACC (Association of Corporate Counsel)

**Angle:** "Validate productions before they fail on import. Catch errors before they cost you."

---

## Part 3: Defense Flight Test (Q3 2026)

### Target Persona: Flight Test Data Processor

**From validated strategy:**
- Job title: Telemetry Data Engineer, Flight Test Data Processor
- Clearance: May require SECRET or higher
- Salary: $90K-$160K
- Pain: Manual extraction, no governance, no audit trail

**Decision maker:** Data Processing Lead, Program Manager

### Go-Live Criteria

Defense goes live when:
```
[ ] CH10/TF10 parsers tested
[ ] Air-gapped deployment mode verified
[ ] 10+ waitlist signups from defense contacts
[ ] SBIR Phase I application submitted
[ ] DFIR + eDiscovery ARR > $80K
```

### Marketing Channels

**This vertical is different:**
- SBIR/STTR applications (AFWERX, Army Futures Command)
- Defense tech partnerships (smaller integrators)
- Test range relationships (Edwards, Pax River)
- Conference presence (ITEA, STC)

**Angle:** "Governed ingestion for returned media. Chain of custody for flight test data."

---

## Part 4: Finance (P3 - Consultant-Delivered Only)

### Status: NOT SELF-SERVE

Finance is explicitly **not** a direct sales target. Reasons:
- High-touch requirements (custom FIX tags, venue-specific formats)
- Expects custom parser development as part of deal
- Long enterprise sales cycle with no warm leads
- Risk of "custom parser trap" (support costs exceed revenue)

### Consultant Delivery Model

If finance opportunities arise:
- Delivered through consulting partners only
- $25,000+ implementation fee
- $15,000+/year platform license
- No direct sales outreach

---

## Pricing Strategy

### DFIR (P0)

| Tier | Annual Price | Target |
|------|--------------|--------|
| Solo | $1,200/year | Individual consultant |
| Team | $4,800/year | Small practice (up to 5) |
| Enterprise Lite | $12,000/year | Mid-size firm (up to 15) |

**Paid Pilot:** $1,000 for 30 days, credits to annual.

### eDiscovery (P1)

| Tier | Annual Price | Target |
|------|--------------|--------|
| Solo | $1,800/year | Individual tech |
| Team | $7,200/year | Lit support team (up to 5) |
| Enterprise Lite | $18,000/year | Large department (up to 15) |

### Defense (P2)

| Tier | Annual Price | Target |
|------|--------------|--------|
| Tactical | $24,000/year | Per flight test program |
| Mission | $60,000/year | Multi-program deployment |

---

## Content Calendar

### Month 1 (DFIR Launch)

| Week | Content | Channel |
|------|---------|---------|
| 1 | Demo video | Website, LinkedIn, DFIR Discord |
| 2 | "Evidence-Grade Parsing" post | LinkedIn article |
| 3 | LinkedIn outreach (30 profiles) | LinkedIn DM |
| 4 | Community engagement | r/dfir, r/computerforensics |

### Month 2-3 (DFIR Growth)

| Week | Content | Channel |
|------|---------|---------|
| 5-6 | Customer testimonial (if available) | Website, LinkedIn |
| 7-8 | HN Show launch (if ready) | Hacker News |
| 9-12 | Iterate based on feedback | All |

### Month 4+ (eDiscovery Prep)

| Week | Content | Channel |
|------|---------|---------|
| 13 | eDiscovery demo video | Website |
| 14 | "Production Preflight" post | LinkedIn |
| 15 | Waitlist → launch email | Email |
| 16 | eDiscovery live | All |

---

## Analytics & Goals

### Plausible Goals to Configure

```
DFIR:
- "Download" click
- "Start Pilot" click
- "View Pricing" click
- "Watch Demo" click

eDiscovery/Defense:
- "Join Waitlist" submit
```

### Weekly Metrics to Track

| Metric | Target (Month 1) |
|--------|------------------|
| DFIR page views | 300+ |
| Downloads | 30+ |
| Pilot starts | 5+ |
| Paid conversions | 3+ |
| Waitlist signups (other verticals) | 15+ |

---

## Competitive Positioning

### DFIR: vs. Existing Tools

| Competitor | Their Weakness | Our Angle |
|------------|----------------|-----------|
| Plaso/log2timeline | No governance, crashes on edge cases | "Governed, reproducible" |
| Velociraptor | Collection focus, not parsing governance | "Evidence-grade outputs" |
| Custom Python scripts | No lineage, no quarantine | "Every row traceable" |
| Autopsy | GUI-focused, less programmable | "CLI-first, scriptable" |

### eDiscovery: vs. Relativity/Vendors

| Their Weakness | Our Angle |
|----------------|-----------|
| $150K+/year (Relativity) | "Preflight layer, not replacement" |
| $5-15K per matter (vendors) | "Catch errors before import" |

### Defense: vs. Custom Tools

| Their Weakness | Our Angle |
|----------------|-----------|
| Program-specific, no governance | "Governed across programs" |
| Brittle extraction scripts | "Reproducible, auditable" |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| No demo video | Record this week — blocks all outreach |
| Stripe not set up | Can't accept money — do before outreach |
| GitHub releases broken | Test download on fresh machine |
| No early customers | Start with paid pilots, validate value |
| Competitor emerges | Move fast, own the DFIR governance messaging |

---

## Next Actions

### This Week
```
[ ] Record 60-second demo video for DFIR
[ ] Set up Stripe Payment Links (4 links + pilot)
[ ] Set up GitHub Releases with CLI binaries
[ ] Create 2 Tally forms (eDiscovery, Defense waitlists)
[ ] Configure Plausible goals
```

### Next Week
```
[ ] DFIR Discord introduction post
[ ] LinkedIn outreach: 15 DFIR consultants
[ ] Test full purchase flow end-to-end
[ ] Monitor analytics daily
```

### This Month
```
[ ] 30+ downloads
[ ] 5+ paid pilots
[ ] 3+ annual conversions
[ ] Collect feedback for iteration
```

---

## Appendix: Email/Message Templates

### LinkedIn Connection Request (DFIR)
```
DFIR + artifact parsing? We built governance for evidence processing —
lineage, quarantine, reproducibility. Would love to show you a 60-second demo.
```

### Cold Email (Short Version)
```
Subject: Evidence-grade artifact parsing

[Name] — When opposing counsel asks "prove this output came from that file,"
what do you show them?

We built a CLI that adds governance to artifact parsing. Every row has
source hash, job ID, parser version.

60-second demo: [LINK]

Worth a look?
```

### Waitlist Follow-Up (When Vertical Goes Live)
```
Subject: Casparian Flow for [eDiscovery/Defense] is live

Hi [Name],

You signed up for early access to Casparian Flow for [vertical].

It's live now: [LINK]

Early access: First 10 customers get 20% off annual pricing.

Questions? Reply to this email.

Best,
[Your name]
```

---

## Revision History

| Date | Change |
|------|--------|
| 2026-01 | Rewritten for DFIR-first launch (removed finance-first content) |

# Casparian Flow - Marketing & Launch Plan

**Last Updated:** January 2026
**Purpose:** Marketing strategy aligned with validated personas and website structure.

---

## Executive Summary

**Product:** CLI tool that transforms industry-specific file formats into queryable SQL.

**Go-To-Market Strategy:** Vertical-by-vertical launch starting with Finance (Trade Support), followed by Healthcare, Legal, and Defense based on waitlist demand and product readiness.

**Target Buyers:**
| Vertical | Buyer Persona | Price Point | Status |
|----------|---------------|-------------|--------|
| Finance | Trade Support Analyst / Manager | $300-$6,000/mo | **LIVE** |
| Healthcare | HL7 Integration Analyst / Manager | TBD | Q2 2026 |
| Legal | Litigation Support Specialist | TBD | Q2 2026 |
| Defense | Intelligence Analyst (cleared) | TBD | 2026 |

---

## Part 1: Finance Vertical Launch (NOW)

### Target Persona: Trade Support Analyst

**From validated research:**
- Job title: Trade Support Analyst, FIX Connectivity Analyst, Middle Office Analyst
- Salary: $73K-$118K (mid-range); hedge funds pay $200K+
- Skills: SQL, Excel, Unix/Linux log parsing, VBA — NOT Python experts
- Pain: 30-45 minutes per trade break investigation
- Work hours: 7am start (handle overnight breaks)

**Decision maker:** Manager of Operations, Head of Trade Support

### Value Proposition

**Short (for ads/headlines):**
> "Debug trade breaks in 5 minutes, not 45. T+1 ready."

**Long (for landing page):**
> "Turn your file chaos into a structured database. Includes a battle-tested FIX parser, or bring your own Python scripts. We handle the lineage, errors, and quarantine."

**Quantified ROI:**
- 40 min/break × 10 breaks/day = 6.5 hours lost daily
- Reduced to 10 min/break = 1.7 hours
- **Time saved: 4.8 hours/day per analyst**
- At $50/hour loaded cost = **$24K/year saved per analyst**
- Team pricing ($2K/mo) pays for itself with 1 analyst in <1 month

### Marketing Channels

#### 1. LinkedIn Outreach (Primary)

**Target search:**
- "Trade Support Analyst"
- "FIX Connectivity"
- "Middle Office" + "Trading"
- "Trade Operations"

**Connection message template:**
```
Hi [Name],

I noticed you're in Trade Support at [Company]. We built a tool that turns FIX log grep sessions into SQL queries — goes from 45-min investigations to 5 minutes.

No cloud, runs locally, full audit trail.

Would a 10-minute demo be useful? Happy to show it on your logs.

Best,
[Your name]
```

**Post-connection message (if accepted):**
```
Thanks for connecting!

Quick context: We're a small team that built Casparian Flow specifically for Trade Support.

The pitch is simple: instead of grep + Excel for trade breaks, you run SQL against a structured order_lifecycle table.

Here's a 60-second demo: [VIDEO_LINK]

If T+1 pressure is real for your team, happy to do a quick call.
```

#### 2. Reddit (Secondary)

**Target subreddits:**
- r/financialcareers (career discussion, can mention tools)
- r/algotrading (quant-adjacent, tech-savvy)
- r/ExperiencedDevs (if framing as infra/tooling)

**Post format (not salesy):**
```
Title: "Anyone have good FIX log analysis tooling?"

Body:
Been dealing with T+1 settlement pressure and our trade break workflow
is still grep + Excel.

Curious what others use for FIX log investigation — we need better
order lifecycle reconstruction than manual grep.

(disclosure: I'm working on a tool for this, but genuinely curious
what else exists)
```

#### 3. Direct Email to Pilot Prospects

**Target:** Companies you've identified or that reach out via website.

**Cold email template:**
```
Subject: FIX log analysis for Trade Support

Hi [Name],

I'm reaching out because [Company] likely deals with trade break
investigations — and T+1 makes that pain worse.

We built Casparian Flow: a local CLI that turns FIX logs into a
queryable SQL table. Instead of grep + Excel, you query
fix_order_lifecycle by ClOrdID.

- Runs on your server (data never leaves)
- Full audit trail for compliance
- Works with FIX 4.2/4.4/5.0 + custom tags

Would a 15-minute demo on your logs be useful?

Best,
[Your name]
Casparian Flow
```

**Follow-up (3 days later, no response):**
```
Subject: Re: FIX log analysis for Trade Support

Hi [Name],

Following up — I know Trade Support teams are slammed in the morning
with overnight breaks.

If timing is bad, no worries. Here's a 60-second video showing the
workflow: [VIDEO_LINK]

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
- "Show HN: SQL for FIX logs – debug trade breaks in 5 minutes"
- "Show HN: Casparian Flow – local-first FIX log analysis for Trade Support"

**Post body:**
```
I built this for Trade Support teams dealing with T+1 settlement pressure.

The problem: Trade breaks require reconstructing order lifecycles from
FIX logs. Most teams grep through logs and paste into Excel. Takes
30-45 minutes per break.

Casparian Flow scans your FIX logs, builds a structured order_lifecycle
table, and lets you query by ClOrdID in SQL.

- Runs locally (data never leaves your machine)
- Full audit trail for compliance
- Works offline

Demo: [VIDEO_LINK]
Download: [GITHUB_RELEASES_LINK]

I'm the solo developer. Happy to answer questions about FIX parsing,
the architecture, or trade support workflows.
```

### Launch Checklist: Finance

**Pre-launch (before any outreach):**
```
[ ] Demo video recorded (60-90 seconds)
    - Show: scan logs → query order → see lifecycle
    - Record with OBS or Loom
    - No fancy editing needed

[ ] GitHub Releases set up
    - macOS ARM64, macOS x64, Linux x64, Windows x64
    - Download links in finance.html work

[ ] Stripe Payment Links created
    - Analyst Monthly: $300/mo
    - Analyst Annual: $3,000/yr
    - Team Monthly: $2,000/mo
    - Team Annual: $20,000/yr

[ ] License key delivery process documented
    - Manual for now: receive Stripe notification → generate key → email

[ ] Plausible goals configured
    - Download click
    - Start Trial click
    - Demo watched
```

**Launch week:**
```
Day 1: LinkedIn outreach to 20 Trade Support profiles
Day 2: LinkedIn outreach to 20 more
Day 3: Post in relevant Reddit thread (if organic opportunity)
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

## Part 2: Healthcare Vertical (Q2 2026)

### Target Persona: HL7 Integration Analyst

**From validated research:**
- Job title: HL7 Interface Analyst, Integration Analyst, Mirth Administrator
- Salary: $65K-$119K; Mirth specialists up to $200K
- Skills: HL7 v2.x (ADT, ORU, ORM), Mirth Connect, SQL, JavaScript
- Pain: Archive analysis gap — Mirth routes, doesn't analyze

**Decision maker:** Director of IT, Integration Manager

### Go-Live Criteria

Healthcare goes live when:
```
[ ] HL7 parser tested on real ADT/ORU/ORM files
[ ] 50+ waitlist signups
[ ] Pricing validated with 3+ prospects
[ ] Demo video for healthcare workflow recorded
[ ] Tally waitlist converted to Stripe payment flow
```

### Marketing Channels

**LinkedIn targets:**
- "HL7 Interface Analyst"
- "Healthcare Integration"
- "Mirth Connect"
- "Epic Integration"

**Industry forums:**
- HL7.org community
- Mirth Community Forums (forum.mirthproject.io)
- HIMSS community

**Angle:** "Mirth went commercial. Get more value from your HL7 archives."

---

## Part 3: Legal Vertical (Q2 2026)

### Target Persona: Litigation Support Specialist

**From validated research:**
- Job title: Litigation Support Specialist, eDiscovery Analyst
- Salary: $82K-$132K
- Pain: Relativity too expensive, vendor processing costs $5-15K per matter
- Skills: Relativity, SQL, Excel, load file formats (DAT/OPT)

**Decision maker:** Director of Litigation Support, Managing Partner (small firms)

### Go-Live Criteria

Legal goes live when:
```
[ ] PST parser tested on real archives
[ ] Load file (DAT/OPT) export working
[ ] 30+ waitlist signups
[ ] Pricing validated with 2+ law firms
[ ] Demo video for eDiscovery workflow recorded
```

### Marketing Channels

**LinkedIn targets:**
- "Litigation Support"
- "eDiscovery"
- "Legal Technology"

**Industry forums:**
- ACEDS (Association of Certified E-Discovery Specialists)
- Above the Law (legal industry publication)

**Angle:** "Process PSTs in-house. Save $5-15K per matter."

---

## Part 4: Defense Vertical (2026)

### Target Persona: Intelligence Analyst (DDIL/Edge)

**From validated research:**
- Job title: Intelligence Analyst, SIGINT Analyst, GEOINT Analyst
- Clearance: TS/SCI required
- Salary: $77K median; $175K+ for senior cleared roles
- Pain: Closed systems (Palantir), DDIL constraints

**Decision maker:** Program Manager, Contracting Officer

### Go-Live Criteria

Defense goes live when:
```
[ ] CoT/PCAP/NITF parsers tested
[ ] Air-gapped deployment mode (no network calls)
[ ] Security review completed
[ ] 20+ waitlist signups from .mil/.gov
[ ] Pricing structure for government contracts defined
```

### Marketing Channels

**This vertical is different:**
- Direct outreach to defense contractors (Palantir alumni, SAIC, Leidos)
- Conference presence (classified)
- Word of mouth in cleared community

**Angle:** "SQL for tactical data on your laptop. Works offline."

---

## Pricing Strategy

### Current (Finance)

| Tier | Price | Target |
|------|-------|--------|
| Free | $0 | Evaluators, hobbyists |
| Analyst | $300/mo ($3K/yr) | Individual analyst |
| Team | $2,000/mo ($20K/yr) | Trade Support team (up to 5) |
| Trading Desk | $6,000/mo | Enterprise (unlimited) |

### Why These Prices

- **$300/mo** = $3,600/year = less than 1 month of analyst salary saved
- **$2,000/mo** = $24K/year = exactly the ROI for one analyst
- B2B pricing, not prosumer — Trade Support teams have budget

### Future Verticals

| Vertical | Expected Pricing | Rationale |
|----------|------------------|-----------|
| Healthcare | $200-$500/mo | Smaller IT budgets than finance |
| Legal | $100-$300/mo | Per-matter or monthly; cost-sensitive |
| Defense | $500-$2,000/mo | Government contracts, longer sales cycle |

---

## Content Calendar

### Month 1 (Finance Launch)

| Week | Content | Channel |
|------|---------|---------|
| 1 | Demo video | Website, LinkedIn |
| 2 | "T+1 and Trade Breaks" blog post | LinkedIn article |
| 3 | LinkedIn outreach (40 profiles) | LinkedIn DM |
| 4 | Reddit engagement | r/financialcareers |

### Month 2-3 (Finance Growth)

| Week | Content | Channel |
|------|---------|---------|
| 5-6 | Customer case study (if available) | Website, LinkedIn |
| 7-8 | HN Show launch (if ready) | Hacker News |
| 9-12 | Iterate based on feedback | All |

### Month 4+ (Healthcare Prep)

| Week | Content | Channel |
|------|---------|---------|
| 13 | Healthcare demo video | Website |
| 14 | "HL7 Archive Analysis" post | LinkedIn |
| 15 | Waitlist → launch email | Email |
| 16 | Healthcare live | All |

---

## Analytics & Goals

### Plausible Goals to Configure

```
Finance:
- "Download Free" click
- "Start Trial" (Analyst) click
- "Start Trial" (Team) click
- "Contact Sales" (Trading Desk) click
- "Watch Demo" click

Healthcare/Legal/Defense:
- "Join Waitlist" submit
```

### Weekly Metrics to Track

| Metric | Target (Month 1) |
|--------|------------------|
| Finance page views | 500+ |
| Downloads | 50+ |
| Trial starts | 10+ |
| Paid conversions | 2-3 |
| Waitlist signups (other verticals) | 20+ |

---

## Competitive Positioning

### Finance: vs. Existing Tools

| Competitor | Their Weakness | Our Angle |
|------------|----------------|-----------|
| Grep + Excel | Manual, no audit trail | "SQL instead of grep" |
| Databricks | Different team, requires credentials | "Runs on YOUR log server" |
| Internal scripts | Knowledge walks out the door | "Standardized, documented" |

### Healthcare: vs. Mirth

| Their Weakness | Our Angle |
|----------------|-----------|
| Routes, doesn't analyze archives | "Analytics that Mirth can't do" |
| No SQL query capability | "Query 5 years of archives" |

### Legal: vs. Relativity/Vendors

| Their Weakness | Our Angle |
|----------------|-----------|
| $150K+/year | "Process PSTs in-house" |
| $5-15K per matter (vendors) | "Save $5-15K per matter" |

### Defense: vs. Palantir

| Their Weakness | Our Angle |
|----------------|-----------|
| Closed system, requires engineers | "Open, analysts control" |
| Cloud/server-based | "Runs on your laptop, offline" |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| No demo video | Record this week — blocks all outreach |
| Stripe not set up | Can't accept money — do before outreach |
| GitHub releases broken | Test download on fresh machine |
| No early customers | Start with free tier, upgrade later |
| Competitor emerges | Move fast, own the vertical messaging |

---

## Next Actions

### This Week
```
[ ] Record 60-second demo video for Finance
[ ] Set up Stripe Payment Links (4 links)
[ ] Set up GitHub Releases with CLI binaries
[ ] Create 3 Tally forms (Healthcare, Legal, Defense waitlists)
[ ] Configure Plausible goals
```

### Next Week
```
[ ] LinkedIn outreach: 20 Trade Support profiles
[ ] Test full purchase flow end-to-end
[ ] Monitor analytics daily
```

### This Month
```
[ ] 50+ downloads
[ ] 2-3 paying customers
[ ] Collect feedback for iteration
```

---

## Appendix: Email/Message Templates

### LinkedIn Connection Request
```
Trade Support + FIX logs? We built a tool that turns grep sessions into
SQL queries. Would love to show you a 60-second demo.
```

### Cold Email (Short Version)
```
Subject: FIX log analysis

[Name] — T+1 is live and trade breaks still take 45 minutes to investigate.

We built a CLI that turns FIX logs into a SQL table. Query by ClOrdID instead of grep.

60-second demo: [LINK]

Worth a look?
```

### Waitlist Follow-Up (When Vertical Goes Live)
```
Subject: Casparian Flow for [Healthcare/Legal/Defense] is live

Hi [Name],

You signed up for early access to Casparian Flow for [vertical].

It's live now: [LINK]

Early access pricing: [X% off] for first 30 days.

Questions? Reply to this email.

Best,
[Your name]
```

---

## Revision History

| Date | Change |
|------|--------|
| 2026-01 | Initial marketing plan aligned with validated personas |

# Casparian Flow - Outreach Email Templates

**Status:** Canonical
**Date:** January 2026
**Target:** DFIR / Incident Response vertical

---

## Email 1: Cold Outreach (LinkedIn/Email)

**Subject lines to A/B test:**
- "DFIR artifact parsing - fragile scripts vs. governed pipelines?"
- "Quick question about your evidence processing workflow"
- "Chain of custody for your EVTX parsing?"

---

### Version A: Pain-First

```
Subject: DFIR artifact parsing - fragile scripts vs. governed pipelines?

Hi [First Name],

Quick question: How confident are you in your artifact parsing scripts?

I've talked to IR consultants who rely on Python scripts that:
- Crash on corrupted EVTX files
- Silently drop rows on edge cases
- Have no audit trail for chain of custody

We built something that turns artifact parsing into a governed, reproducible pipeline.
Every row has source hash, job ID, and parser version. Quarantine catches the edge cases.

Would you be open to a 15-minute call to see if this is relevant to your practice?

[Your name]

P.S. - If you're not the right person, who handles artifact tooling at [Company]?
```

---

### Version B: Evidence-First

```
Subject: Chain of custody for your EVTX parsing?

Hi [First Name],

I'm [Your name], founder of Casparian Flow. We help DFIR practitioners
build evidence-grade artifact parsing pipelines.

Before we launch publicly, I'm talking to 20 IR consultants to understand
how teams handle the "prove your parsing" problem.

Would you have 15 minutes this week for a quick call? Not a sales pitch -
genuinely trying to learn.

[Your name]
```

---

### Version C: Tool-First

```
Subject: Quick question about your evidence processing workflow

Hi [First Name],

When you process case folders - EVTX files, registry hives, prefetch -
how do you track what you parsed and with what tool version?

We built a tool that adds governance to artifact parsing:
- Source hash per file
- Lineage columns on every output row
- Quarantine for malformed records
- Reproducible runs

Curious if chain of custody documentation is a pain point for you?

15 minutes to compare notes?

[Your name]
```

---

## Email 2: Follow-Up (Day 3-4)

```
Subject: Re: [Original subject]

Hi [First Name],

Following up on my note from [Day].

I know IR work is time-sensitive, so I'll keep this short:

We're running 30-day paid pilots ($1K, credits to annual) with DFIR
boutiques who want to test governed artifact parsing.

If "prove your parsing" or "script crashed on weird file" are real problems,
might be worth a quick look.

Open to a 15-minute call this week?

[Your name]
```

---

## Email 3: Break-Up Email (Day 7-10)

```
Subject: Closing the loop

Hi [First Name],

I've reached out a couple times about artifact parsing governance for IR work.

I'll assume the timing isn't right and close this out.

If chain of custody or reproducibility become priorities, feel free to reach out.

Best,
[Your name]

P.S. - If there's someone else at [Company] I should talk to about this,
I'd appreciate the intro.
```

---

## LinkedIn Connection Request

```
Hi [First Name] - I'm researching how IR consultants handle artifact parsing
tooling (EVTX, registry, etc). Would love to hear how [Company] approaches
evidence-grade workflows. Open to a quick chat?
```

---

## LinkedIn InMail (If Not Connected)

```
Hi [First Name],

I'm building a tool that adds governance to DFIR artifact parsing -
lineage, quarantine, reproducibility.

Before we launch, I'm talking to 20 IR practitioners to understand
what's working and what's not in evidence processing workflows.

Would you have 15 minutes for a call? Not a sales pitch - genuinely
trying to learn.

Thanks,
[Your name]
```

---

## Response Templates

### If They Say Yes

```
Great! Here's my Calendly: [link]

Pick any 15-minute slot that works. Looking forward to learning about
your artifact workflow.

[Your name]
```

### If They Say "Not Me, Try X"

```
Thanks for the redirect! I'll reach out to [X].

Quick question before I do - what's the biggest tooling headache you're
seeing in IR work right now? Just curious.

[Your name]
```

### If They Say "We Don't Have This Problem"

```
That's great to hear! Most IR teams I talk to have some "parsing script
crashed on weird file" stories.

What's your secret? I'd love to understand what's working for [Company].

[Your name]
```

### If They Say "Send Me Info"

```
Sure thing. Here's a 2-minute overview:

[Loom video link OR one-pager PDF]

The TL;DR: We add governance to artifact parsing. Every output row has
source hash, job ID, parser version. Quarantine catches malformed records.
Reproducible runs for chain of custody.

Worth a 15-minute call to see if it fits your workflow?

[Your name]
```

---

## Target List Criteria

### Where to Find Prospects

1. **LinkedIn Sales Navigator**
   - Title: "Forensic" OR "Incident Response" OR "DFIR"
   - Industry: Computer & Network Security, IT Services
   - Company size: 5-100 (boutiques = faster decisions)

2. **Community Presence**
   - SANS DFIR Summit attendees
   - DFRWS attendees
   - DFIR Discord / Reddit

3. **Warm Intros**
   - Ask existing network: "Know anyone in IR/forensics?"

### Ideal Company Profile

| Attribute | Target |
|-----------|--------|
| Size | 5-100 employees |
| Type | DFIR boutique, IR consulting, forensic services |
| Tech | Uses Python for artifact parsing |
| Pain signal | Mentions "evidence integrity" or "reproducibility" |

---

## Tracking

### Per-Outreach Record

```
Date: _______________
Prospect: _______________
Company: _______________
Title: _______________
Channel: [ ] Email [ ] LinkedIn [ ] Intro
Email Version: [ ] A [ ] B [ ] C
Status: [ ] Sent [ ] Opened [ ] Replied [ ] Scheduled [ ] No Response
Outcome: _______________
Notes: _______________
```

### Metrics to Track

| Metric | Target |
|--------|--------|
| Outreach sent | 30+/week |
| Open rate (email) | >40% |
| Reply rate | >15% |
| Call scheduled rate | >8% |
| Conversations completed | 15 in 4 weeks |

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 2.0 | Rewritten for DFIR-first targeting |

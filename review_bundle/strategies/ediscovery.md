# eDiscovery Market Strategy

**Status:** Heavily Qualified (One Persona Only)
**Parent:** STRATEGY.md Section 2 (Target Market → eDiscovery)
**Priority:** P3 (Bridge Market - "Maybe")
**Version:** 0.3
**Date:** January 20, 2026

---

> **Critical Realization (January 2026):** "eDiscovery" is a **business process**, not a job title. The market splits into three distinct personas—only ONE survives the technical user test.

## 1. Executive Summary: The eDiscovery Segmentation

### 1.1 What Happened to eDiscovery?

We realized the critical flaw: **"If they're opening Excel, they probably won't know how to use Python."**

This forced us to split the eDiscovery market:

| Persona | Technical? | Verdict | Why |
|---------|------------|---------|-----|
| **eDiscovery Analyst** | NO (Excel) | **DEAD END** | Treats Casparian as "Magic Converter"; files support tickets |
| **Litigation Support Technologist** | Reluctant (Python) | **MAYBE** | Can code but hates it; bridge market only |
| **DFIR Consultant** | YES (Python daily) | **WINNER** | → Moved to [strategies/dfir.md](dfir.md) |

### 1.2 The Dead End: eDiscovery Analyst

| Attribute | Detail |
|-----------|--------|
| **Who they are** | Project Managers, Review Managers at law firms |
| **Their workflow** | Drag 50GB into Relativity/Nuix/Disco, click "Process" |
| **When parsing fails** | Mark as "Exception" and ignore, OR email a vendor |
| **The trap** | They treat Casparian as a **Magic Converter**. When it fails, they file a support ticket, not write code. **You become their free IT support.** |

**VERDICT: CUT.** Do not target. Do not demo to. Do not accept money from.

### 1.3 The Survivor: Litigation Support Technologist (This Document)

| Attribute | Detail |
|-----------|--------|
| **Who they are** | Technical back-office staff who support analysts |
| **Their workflow** | Write SQL/Python to massage data into "Load Files" (custom CSVs) for platform ingestion |
| **Why "Maybe"** | They **can** code, but they **hate** it. View it as necessary evil to meet court deadlines. |
| **Market status** | Good **bridge market**, but not core. Secondary to DFIR. |

**VERDICT: P3 (MAYBE).** Valid if they find us, but don't prioritize outreach.

### 1.4 The Pivot: DFIR Consultant (Separate Strategy)

The "eDiscovery idea" actually landed on **Forensic Engineers** who work for legal teams.

| Attribute | Detail |
|-----------|--------|
| **Who they are** | Incident Responders, Digital Forensic experts |
| **Their workflow** | Hunt through raw disk bytes for evidence of hackers |
| **Why they fit** | Write Python daily (pytsk3, construct); Need the backtest loop; Data is literally disk images |

**VERDICT: WINNER.** See [strategies/dfir.md](dfir.md).

---

## 2. Bottom Line

> **You aren't selling to "Legal Tech" anymore. You are selling to "Cybersecurity & Forensics."**
>
> Same budgets, much more technical users.

### 2.1 What This Strategy Now Covers

This document covers the **Litigation Support Technologist** persona only—the "Maybe" bridge market.

- Do NOT use this strategy to target eDiscovery Analysts (Excel users)
- For DFIR Consultants (Python users), see [strategies/dfir.md](dfir.md)

---

## 3. The Litigation Support Technologist (LST)

**The Pitch:**
> "Stop writing throwaway scripts for Load File generation. Get audit trails for court."

### 3.1 Who They Are

| Attribute | Description |
|-----------|-------------|
| **Role** | Litigation Support Technologist, Legal IT, eDiscovery Engineer |
| **Department** | Back-office at law firm or eDiscovery vendor |
| **Technical skill** | **Reluctant Python/SQL**; can code but views it as necessary evil |
| **Pain** | Forced to write scripts when platforms can't ingest weird formats |
| **Goal** | Meet court deadline; make data ingestible |
| **Buying power** | Per-matter budget; firm can approve $500-2K/matter |

### 3.2 Why They're "Maybe" Not "Core"

| Factor | LST | DFIR (contrast) |
|--------|-----|-----------------|
| **Attitude toward coding** | Necessary evil | Daily practice |
| **When parser fails** | Escalate or workaround | Debug and fix |
| **Parser iteration** | Minimal (deadline-driven) | Extensive (evidence-driven) |
| **Uses full platform value?** | Partial | Full |

### 3.3 Current Workflow

1. Receive data dump from client (PST, Slack, proprietary)
2. Try to load into Relativity → **fails on weird format**
3. **Reluctantly** write Python script to convert to Load File
4. Script works → move on (no governance, no tests)
5. Script fails → panic, deadline pressure
6. No audit trail for court if challenged

### 3.4 Casparian Value (Limited)

| Feature | Value to LST |
|---------|--------------|
| **Lineage** | Court defensibility (valuable) |
| **Quarantine** | Evidence preservation (valuable) |
| **Backtest** | Less valuable (one-off scripts) |
| **Parser versioning** | Less valuable (don't iterate) |

---

## 2. Market Overview

### 2.1 eDiscovery Industry Size

| Metric | Value | Source |
|--------|-------|--------|
| Global eDiscovery market | ~$15B (2024) | Various |
| Projected (2030) | ~$30B+ | CAGR 12-15% |
| US legal services market | $350B+ | ABA |
| Law firms in US | 450,000+ | ABA |
| Law firms <10 attorneys | 80,000+ | ABA |

### 2.2 The Processing Tier (Our Target)

The eDiscovery workflow has three tiers:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     COLLECTION TIER                                  │
│  • Forensic imaging                                                  │
│  • Data preservation                                                 │
│  • Chain of custody                                                  │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     PROCESSING TIER  ◄── OUR TARGET                 │
│  • Parse data dumps (PST, Slack, proprietary)                       │
│  • Extract text, metadata, attachments                              │
│  • Load into review platform (Relativity, Logikcull)                │
│  • MUST have audit trail for court                                  │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     REVIEW TIER                                      │
│  • Document review (attorney time)                                   │
│  • Privilege review                                                  │
│  • Production                                                        │
└─────────────────────────────────────────────────────────────────────┘
```

**Key Insight:** We target the **Processing Tier**, not Collection (forensic tools) or Review (Relativity, Logikcull).

### 2.3 The Pain Point

**Scenario:** Law firm receives 2TB hard drive from client in lawsuit. Contains:
- 10 years of PST email archives
- Slack JSON exports
- Proprietary chat logs from old system
- Weird accounting system dumps
- Excel files with macros
- Old Access databases

**Current Workflow:**
1. Litigation support technologist writes Python script to parse each format
2. Script runs overnight
3. Missing data discovered during review ("Where are the 2018 chats?")
4. Re-run, re-parse, repeat
5. No audit trail of what was parsed, when, by which script version
6. If evidence is missed → **spoliation risk** (legal liability)

**The Quantified Pain:**
- $300-500/hour billable rate for eDiscovery technologists
- 20-40 hours per matter on parsing/processing
- $6K-20K in processing time per matter
- Spoliation sanctions: $10K to $1M+ in penalties

### 2.4 Regulatory Environment

| Requirement | What It Means |
|-------------|---------------|
| **FRCP Rule 26** | Parties must disclose evidence; production must be defensible |
| **Chain of Custody** | Must document what was done to evidence, when, by whom |
| **Spoliation** | Failure to preserve/produce evidence = sanctions |
| **Load File Standards** | Standard formats (DAT, OPT) for loading into review platforms |

---

## 4. Target Persona (Narrow)

> **Only ONE persona survives:** The Litigation Support Technologist who is forced to write code.

### 4.1 The Litigation Support Technologist (LST)

| Attribute | Description |
|-----------|-------------|
| **Role** | Litigation Support Technologist, Legal IT Engineer |
| **Technical skill** | **Reluctant Python/SQL**; can code but doesn't love it |
| **Pain** | Platforms can't ingest weird formats; forced to write scripts |
| **Goal** | Meet court deadline; make data ingestible |
| **Buying power** | Per-matter budget; firm can approve $500-2K/matter |

### 4.2 Do NOT Target (Explicitly Cut)

| Persona | Why Cut |
|---------|---------|
| **eDiscovery Analyst** | Excel user; clicks "Process" in Relativity; files support tickets when things fail |
| **Project Manager** | Non-technical; manages timelines, not data |
| **Review Manager** | Attorney-adjacent; reads documents, doesn't parse them |
| **eDiscovery Consultant (general)** | Too broad—split into LST (maybe) and DFIR (winner) |

### 4.3 If Confused, Ask This Question

> "When a weird file format fails to parse, do they (a) write a Python script, or (b) email a vendor?"

- **(a) Write a script** → Valid target (LST or DFIR)
- **(b) Email a vendor** → **DO NOT TARGET** (eDiscovery Analyst)

---

## 4. Competitive Landscape

### 4.1 Direct Competitors (Processing Tier)

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| [Relativity Processing](https://www.relativity.com/) | Native processing for Relativity | Included/expensive | Locked to Relativity; limited custom formats |
| [Nuix](https://www.nuix.com/) | Enterprise processing | $50K+/year | Enterprise only; complex |
| [Exterro](https://www.exterro.com/) | End-to-end eDiscovery | Enterprise | Large firm focus |
| [Logikcull](https://www.logikcull.com/) | Cloud processing | $250-500/GB | Per-GB expensive at scale; cloud |
| [GoldFynch](https://goldfynch.com/) | Cloud PST processing | Per-GB | Cloud; limited formats |
| **DIY Python scripts** | Custom per-matter | "Free" | No governance, no audit trail |

### 4.2 Why These Aren't Enough

| Competitor | Why Casparian Wins |
|------------|-------------------|
| Relativity | Only works for Relativity output; we work with any review platform |
| Nuix | Enterprise pricing; we're accessible to boutiques |
| Logikcull/GoldFynch | Per-GB gets expensive; cloud; limited formats |
| DIY scripts | No audit trail; no versioning; spoliation risk |

### 4.3 The Market Gap

```
┌─────────────────────────────────────────────────────────────────────┐
│                     ENTERPRISE TIER                                  │
│  Relativity, Nuix, Exterro ($50K-$500K/year)                        │
│  → Am Law 100, Fortune 500 legal departments                        │
└─────────────────────────────────────────────────────────────────────┘
                          ↑
                    MARKET GAP
              (We target this gap)
                          ↓
┌─────────────────────────────────────────────────────────────────────┐
│                     DIY TIER                                         │
│  Python scripts, manual processing, vendor outsourcing              │
│  → Boutique firms, solo practitioners, consultants                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Why Casparian Fits

### 5.1 Core Platform Features → eDiscovery Value

| Casparian Feature | eDiscovery Value |
|-------------------|------------------|
| **Lineage tracking** | Chain of custody; prove what was parsed, when, by which parser |
| **Quarantine** | Bad rows isolated; no evidence lost; auditable |
| **Backtest** | Validate parser against sample before production run |
| **Schema contracts** | Output matches expected format; no surprises |
| **Parser versioning** | Know exactly which parser version produced which output |
| **Local-first** | Privileged data stays on-prem; no cloud exposure |

### 5.2 The Defensible Parsing Story

```
BEFORE CASPARIAN                          WITH CASPARIAN
────────────────                          ──────────────
"We ran a Python script"                  "Parser v2.3.1 processed file X at
                                           timestamp Y, producing N rows with
                                           M quarantined. Full lineage attached."

Court: "Can you prove this is complete?"  Court: "The audit trail is comprehensive."
```

### 5.3 Spoliation Protection

| Spoliation Risk | How Casparian Helps |
|-----------------|---------------------|
| Parser skips rows silently | Quarantine captures bad rows; nothing lost |
| Can't prove what was processed | Lineage columns on every row |
| Parser version unknown | Version recorded in `_cf_parser_version` |
| Re-run produces different results | Deterministic; same input → same output |

---

## 6. Go-to-Market

### 6.1 Positioning

**Primary:** "Defensible parsing infrastructure for eDiscovery"

**Secondary:** "The audit trail your throwaway scripts don't have"

### 6.2 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **ACEDS (eDiscovery Association)** | Community outreach, webinars | Month 1-3 |
| **Relativity Fest** | Conference presence | Annual |
| **LinkedIn** | Direct outreach to Litigation Support titles | Month 1-6 |
| **Legal tech blogs** | Content marketing | Month 1-6 |
| **Boutique eDiscovery firms** | Direct sales | Month 3-9 |

### 6.3 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Spoliation-proof your parsing" blog | SEO, thought leadership | High |
| "Parse PST with audit trail" tutorial | Developer education | High |
| "Why your Python scripts are liability" | Pain agitation | High |
| Demo video: Messy data → Relativity-ready | Proof of value | High |
| Case study: [Boutique firm] saved X hours | Social proof | Medium |

### 6.4 Demo Script (60 seconds)

```
[0:00] "You just received a 500GB hard drive from opposing counsel.
       Let me show you how Casparian makes your processing defensible."

[0:10] *Point Casparian at data dump*
       casparian scan ./evidence --tag matter_12345

[0:20] "Casparian discovers and tags all files."

[0:25] *Run parser*
       casparian run pst_parser.py ./evidence/inbox.pst

[0:35] "Every row has lineage: source file, parser version, timestamp.
       Bad rows go to quarantine, not the void."

[0:45] *Show audit report*
       casparian report --matter matter_12345

[0:55] "Chain of custody documentation for the court."

[1:00] "That's defensible processing in 60 seconds."
```

---

## 7. Premade Parsers (Starter Kits)

Ship these as **examples**, not products. eDiscovery technologists will customize:

### 7.1 PST Parser (`pst_parser.py`)

**Input:** Microsoft Outlook PST files

**Output Tables:**

| Table | Description |
|-------|-------------|
| `pst_messages` | Email headers, body text, metadata |
| `pst_attachments` | Attachment metadata, extracted text |
| `pst_folders` | Folder hierarchy |

**Library:** libpff (via pypff) or readpst

### 7.2 Slack Export Parser (`slack_parser.py`)

**Input:** Slack JSON export

**Output Tables:**

| Table | Description |
|-------|-------------|
| `slack_messages` | Messages with timestamps, users |
| `slack_channels` | Channel metadata |
| `slack_users` | User directory |
| `slack_files` | File attachments |

### 7.3 Generic Load File Parser (`loadfile_parser.py`)

**Input:** DAT, OPT files (standard eDiscovery formats)

**Output:** Validated load file ready for review platform import

---

## 8. Pricing

### 8.1 Value Analysis

| Role | Hourly Rate | Time Saved | Value Created |
|------|-------------|------------|---------------|
| eDiscovery Technologist | $300-500/hr | 10-20 hrs/matter | $3K-10K/matter |
| Boutique firm (per matter) | N/A | 40+ hrs/matter | $12K-20K/matter |

**Additional value:** Spoliation protection (avoid $10K-$1M+ sanctions)

### 8.2 Pricing Tiers

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Pro** | $100/user/month | Full platform, example parsers | Individual technologists |
| **Team** | $400/month | Multi-matter, shared parsers | Small firms |
| **Consultant** | $600/month | White-label, multi-client | eDiscovery consultants |
| **Enterprise** | Custom | SSO, audit reports, support | Am Law 100 |

### 8.3 Alternative: Per-Matter Pricing

For firms that prefer project-based:

| Matter Size | Price | Features |
|-------------|-------|----------|
| Small (<10GB) | $500/matter | Full processing |
| Medium (10-100GB) | $1,500/matter | Full processing |
| Large (100GB+) | Custom | Dedicated support |

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| eDiscovery users | 25 | 100 |
| Matters processed | 50 | 250 |
| Files processed | 100K | 1M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| eDiscovery MRR | $5K | $25K |
| Paying customers | 15 | 50 |
| Average deal size | $400/mo | $500/mo |

### 9.3 Validation Metrics

| Metric | Target |
|--------|--------|
| "Backtest is valuable" feedback | 5+ users |
| "Lineage saved us" stories | 3+ case studies |
| Platform feedback (not domain) | 10+ requests |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Long sales cycle (law firms) | Medium | Target boutiques, consultants first |
| Relativity dominance | Medium | Position as pre-Relativity processing |
| PST format complexity | Low | Use mature libraries (libpff) |
| Spoliation liability concern | Medium | Clear documentation that we're tools, not legal advice |
| Enterprise competitor moves down-market | Medium | Speed advantage; local-first; boutique focus |

---

## 11. References

- [ACEDS (Association of Certified E-Discovery Specialists)](https://aceds.org/)
- [Relativity Documentation](https://relativity.com/resources/)
- [EDRM (Electronic Discovery Reference Model)](https://edrm.net/)
- [FRCP Rule 26](https://www.law.cornell.edu/rules/frcp/rule_26)
- [libpff (PST parsing library)](https://github.com/libyal/libpff)

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial draft based on strategic fork evaluation |
| 2026-01-20 | 0.2 | Demoted from P0 to P2; DFIR has higher urgency; Now parallel track with DFIR |
| 2026-01-20 | 0.3 | **Major revision:** Segmented eDiscovery into 3 personas; Cut "eDiscovery Analyst" (Excel user, support ticket filer); Narrowed to "Litigation Support Technologist" only (reluctant Python); Clarified DFIR is where the idea actually landed; Demoted to P3 "Bridge Market" |

# DFIR (Digital Forensics & Incident Response) Market Strategy

**Status:** Active
**Parent:** STRATEGY.md Section 2 (Target Market → DFIR)
**Priority:** #1 (The Winner - Immediate Cash)
**Version:** 0.2
**Date:** January 20, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the Digital Forensics & Incident Response (DFIR) market as the **#1 priority target** for immediate cash flow.

**Why DFIR Is #1 (The Winner):**

DFIR consultants are the **only customer** with "network drive data" that is both **urgent** (active breach) and **legally mandated** to have a perfect audit trail.

| Factor | DFIR | Pharma (#2 contrast) | Trade Desk (cut) |
|--------|------|---------------------|------------------|
| **The Data** | Disk images, Amcache, Shimcache, $MFT on air-gapped evidence servers | Instrument files on lab network drives | FIX logs on trading servers |
| **Current Tool** | "Fragile" Python scripts (construct, kaitai) | ETL scripts to Snowflake | grep + Excel |
| **Writes Python?** | YES (binary artifact parsing) | YES | NO |
| **Urgency** | **EXTREME** (stop breach NOW) | Medium (nightly batch) | High (T+1) |
| **Audit Trail** | **LEGALLY MANDATED** (chain of custody) | FDA REQUIRED | Nice-to-have |
| **Why They Pay** | **Speed + Liability** ("script deletes row = destroyed evidence") | Compliance | Time savings |
| **Sales Cycle** | **FAST** (boutique firms, practitioners decide) | Slow (enterprise) | Medium |

**The Pitch:**
> *"The first IDE for forensic artifact parsing. Stop trusting fragile scripts for evidence."*

**Key Value Proposition:** Your **Lineage/Quarantine** feature is their **insurance policy**. If their script deletes a row, they destroy evidence. Casparian quarantines bad rows—nothing is lost.

### Trust Primitives for DFIR

| Guarantee | DFIR Value |
|-----------|------------|
| **Reproducibility** | Same evidence + same parser = identical outputs (court-defensible) |
| **Per-row lineage** | Chain of custody: `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Quarantine** | Corrupted artifacts don't crash; bad rows isolated with error context |
| **Content-addressed identity** | Know exactly which parser version produced which output |
| **Backfill planning** | When parser improves, see exactly what needs reprocessing |

---

## 2. Market Overview

### 2.1 DFIR Industry Size

| Metric | Value | Source |
|--------|-------|--------|
| Global cybersecurity services market | ~$150B (2024) | Various |
| Incident response services | ~$15-20B | Subset |
| Average data breach cost | $4.45M (2023) | IBM |
| Median ransom payment | $1.5M (2023) | Various |
| IR retainer market | Growing 15-20% YoY | Industry |

### 2.2 The IR Workflow

```
┌─────────────────────────────────────────────────────────────────────┐
│                     DETECTION / TRIAGE                               │
│  • Alert received (SOC, EDR, user report)                           │
│  • Initial scoping                                                   │
│  • Evidence collection initiated                                     │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     EVIDENCE COLLECTION                              │
│  • Disk imaging                                                      │
│  • Memory capture                                                    │
│  • Log aggregation                                                   │
│  • Network capture                                                   │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     ARTIFACT ANALYSIS  ◄── OUR TARGET                │
│  • Parse Windows Event Logs                                          │
│  • Parse Shimcache, Amcache, Prefetch                               │
│  • Parse browser history, registry                                   │
│  • Build timeline of attacker activity                               │
│  • MUST handle corrupted/partial data                                │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     REPORTING / REMEDIATION                          │
│  • Timeline report                                                   │
│  • Indicators of Compromise (IOCs)                                   │
│  • Remediation recommendations                                       │
│  • Legal/law enforcement handoff                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**Key Insight:** We target **Artifact Analysis**, where responders parse system artifacts to build attacker timelines.

### 2.3 The Pain Point

**Scenario:** Client is hacked. Responder gets disk images from 50 workstations. Each contains:
- Windows Event Logs (EVTX) - some corrupted
- Shimcache (program execution artifacts)
- Amcache (application compatibility)
- Prefetch files (program execution)
- Registry hives
- Browser artifacts
- Custom application logs

**Current Workflow:**
1. Use standard tools (log2timeline, Plaso, RegRipper)
2. Tool crashes on corrupted artifact
3. Write custom Python script to handle edge case
4. Script works on this case, breaks on next
5. No audit trail of what was parsed
6. If evidence is missed → attacker escapes → client is re-compromised
7. If prosecution fails → chain of custody questioned

**The Quantified Pain:**
- $300-500/hour billable rate for senior IR consultant
- 40-100 hours per incident on artifact analysis
- $12K-50K per incident in analysis time
- Active breach = every hour matters (attacker still in network)
- Chain of custody failure = criminal case dismissed

### 2.4 Why Standard Tools Aren't Enough

| Tool | Strength | Weakness |
|------|----------|----------|
| **Plaso/log2timeline** | Comprehensive | Crashes on corrupted data; hard to customize |
| **RegRipper** | Registry parsing | Perl-based; hard to extend |
| **Eric Zimmerman tools** | Windows artifacts | GUI-focused; limited batch processing |
| **Velociraptor** | Collection + hunting | Collection focus, not parsing infrastructure |
| **Custom scripts** | Flexible | No governance, no versioning, fragile |

### 2.5 The "Fragile Scripts" Problem

DFIR practitioners currently write Python scripts using:
- **`construct`** - Binary parsing library for custom structures
- **`kaitai struct`** - Binary format parser generator
- **Direct struct unpacking** - Manual byte parsing

**Why these are fragile:**
```python
# Typical DFIR script (fragile)
def parse_shimcache(data):
    header = struct.unpack("<I", data[0:4])[0]  # Crashes on truncated file
    entries = []
    offset = 0x80
    while offset < len(data):
        entry = parse_entry(data[offset:])      # Crashes on corrupted entry
        entries.append(entry)
        offset += entry_size
    return entries                               # Returns nothing if any step fails
```

**What happens in practice:**
1. Script works on clean test data
2. Runs on real evidence → corrupted artifact → **crash**
3. Entire timeline generation fails
4. Responder spends hours debugging script instead of finding attacker

**Casparian solution:**
- **Quarantine** bad records, continue processing
- **Lineage** proves what was processed
- **Backtest** catches edge cases before production

**The Gap:** There's no "parser development infrastructure" for DFIR. Teams either use monolithic tools (that crash) or write fragile scripts (no governance).

---

## 3. Target Personas

### 3.1 Primary: DFIR Consultant

| Attribute | Description |
|-----------|-------------|
| **Role** | Digital Forensics Analyst, Incident Responder |
| **Technical skill** | **Python, PowerShell, forensic tools**; highly technical |
| **Pain** | Parsers crash on corrupted data; no governance on custom scripts |
| **Goal** | Build timeline without losing evidence |
| **Buying power** | Can expense tools; firm approves $500-2K/engagement |

### 3.2 Secondary: IR Boutique Firm

| Attribute | Description |
|-----------|-------------|
| **Role** | Small DFIR consultancy (5-20 people) |
| **Technical skill** | Very high |
| **Pain** | Rebuilding infrastructure per engagement; staff turnover |
| **Goal** | Reusable, robust parsing infrastructure |
| **Buying power** | Monthly subscription; passes to clients |

### 3.3 Tertiary: Enterprise SOC / IR Team

| Attribute | Description |
|-----------|-------------|
| **Role** | Internal security team, CIRT |
| **Technical skill** | High |
| **Pain** | Inconsistent artifact handling; no audit trail |
| **Goal** | Standardized, defensible analysis pipeline |
| **Buying power** | Security budget; $25K-100K/year tools |

---

## 4. Competitive Landscape

### 4.1 DFIR Tools Ecosystem

| Category | Tools | Our Relationship |
|----------|-------|------------------|
| **Collection** | Velociraptor, KAPE, FTK Imager | Complementary (we parse what they collect) |
| **Timeline** | Plaso, log2timeline | Alternative (we're more robust + custom) |
| **Artifact** | Eric Zimmerman, RegRipper | Complementary (we add governance) |
| **Platform** | Magnet AXIOM, X-Ways | Different tier (we're infrastructure) |

### 4.2 Why Casparian Wins

| Competitor | Why Casparian Wins |
|------------|-------------------|
| Plaso | Casparian doesn't crash on corrupted data (quarantine) |
| Custom scripts | Casparian adds governance, versioning, audit trail |
| AXIOM/X-Ways | Casparian is infrastructure, not monolithic tool; $300/mo vs $5K+ |
| DIY | Casparian saves 20+ hours per engagement on infrastructure |

### 4.3 Positioning

We are **NOT** replacing forensic tools. We are providing **infrastructure for custom parsing** that the ecosystem lacks.

```
FORENSIC TOOL STACK                    WHERE CASPARIAN FITS
────────────────────                   ─────────────────────

[Collection Tools]                     (We parse what they collect)
  Velociraptor, KAPE
        │
        ▼
[Parsing Tools]      ◄── FRAGILE      [CASPARIAN]
  Plaso, scripts     (crash on          • Robust parsing
        │             corrupted data)   • Quarantine bad rows
        ▼                               • Versioning
[Analysis Tools]                        • Audit trail
  Timesketch, ELK                       • Custom parsers
        │
        ▼
[Reporting]
  Word, custom reports
```

---

## 5. Why Casparian Fits

### 5.1 Core Platform Features → DFIR Value

| Casparian Feature | DFIR Value |
|-------------------|------------|
| **Quarantine** | Corrupted artifacts don't crash pipeline; bad rows isolated |
| **Backtest** | Validate parser against sample artifacts before full run |
| **Lineage tracking** | Chain of custody; prove what was parsed, when, by which parser |
| **Parser versioning** | Know exactly which parser version produced which output |
| **Local-first** | Evidence never leaves the analysis machine; air-gapped OK |
| **Schema contracts** | Output matches expected format; timeline tools can consume |

### 5.2 The Robustness Story

**The Problem:** Attackers don't leave clean logs. Standard parsers assume well-formed input.

```
REAL-WORLD ARTIFACTS                  STANDARD PARSER
──────────────────                    ────────────────
• Corrupted EVTX files                  → CRASH
• Partial disk images                   → CRASH
• Timestomped files                     → Wrong timeline
• Truncated logs                        → CRASH
• Encoding issues                       → CRASH or garbage

                                      CASPARIAN
                                      ─────────
• Corrupted EVTX files                  → Quarantine bad records, parse rest
• Partial disk images                   → Parse what's available, flag gaps
• Timestomped files                     → Flag suspicious timestamps
• Truncated logs                        → Quarantine, continue
• Encoding issues                       → Explicit handling, no silent fail
```

### 5.3 Chain of Custody

| Requirement | How Casparian Helps |
|-------------|---------------------|
| What was processed? | Lineage columns on every row |
| When was it processed? | `_cf_processed_at` timestamp |
| What parser version? | `_cf_parser_version` on every row |
| What was skipped? | Quarantine table with reasons |
| Reproducible? | Same input → same output (deterministic) |

---

## 6. Go-to-Market

### 6.1 Positioning

**Primary:** "Robust artifact parsing infrastructure for incident response"

**Secondary:** "The parser that doesn't crash on corrupted evidence"

### 6.2 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **SANS Community** | Webinars, blog posts, FOR508/FOR500 adjacent | Month 1-3 |
| **DFIR Discord** | Community engagement | Month 1-3 |
| **LinkedIn** | Direct outreach to DFIR titles | Month 1-6 |
| **DFIR Summit / conferences** | Conference presence | Annual |
| **Boutique IR firms** | Direct sales | Month 3-9 |

### 6.3 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Parsing artifacts that crash Plaso" blog | Pain agitation | High |
| "EVTX parser with quarantine" tutorial | Developer education | High |
| "Chain of custody for artifact analysis" | Compliance angle | High |
| Demo video: Corrupted disk → Timeline | Proof of value | High |
| Case study: IR firm saved X hours | Social proof | Medium |

### 6.4 Demo Script (60 seconds)

```
[0:00] "You've got a disk image with corrupted Event Logs.
       Watch Plaso crash, then watch Casparian handle it."

[0:10] *Run Plaso*
       $ plaso.py --source disk.E01
       ERROR: Failed to parse EventLog record...

[0:20] "Plaso crashed. Let's try Casparian."

[0:25] *Run Casparian*
       $ casparian run evtx_parser.py ./disk.E01/Windows/System32/winevt/

[0:35] "Casparian parsed 50,000 events, quarantined 127 corrupted records.
       Nothing lost. Here's the timeline."

[0:45] *Show timeline output*
       SELECT timestamp, event_id, description FROM evtx_events
       ORDER BY timestamp;

[0:55] "And here's the quarantine for review."
       SELECT * FROM evtx_events_quarantine;

[1:00] "Robust parsing. Full audit trail. That's Casparian."
```

---

## 7. Premade Parsers (Starter Kits)

Ship these as **examples**, not products. DFIR teams will customize:

### 7.1 Windows Event Log Parser (`evtx_parser.py`)

**Input:** EVTX files (Windows Event Logs)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `evtx_events` | Parsed events with timestamp, event_id, channel, data |
| `evtx_events_quarantine` | Corrupted records with error reason |

**Library:** python-evtx

### 7.2 Shimcache Parser (`shimcache_parser.py`)

**Input:** SYSTEM registry hive

**Output Tables:**

| Table | Description |
|-------|-------------|
| `shimcache_entries` | Program execution artifacts |

**Library:** Custom (AppCompatCache format)

### 7.3 Prefetch Parser (`prefetch_parser.py`)

**Input:** Prefetch files (*.pf)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `prefetch_entries` | Program execution with timestamps, run count |

**Library:** prefetch (PyPI)

### 7.4 Browser History Parser (`browser_parser.py`)

**Input:** Chrome/Firefox/Edge history databases

**Output Tables:**

| Table | Description |
|-------|-------------|
| `browser_history` | URLs, timestamps, visit counts |
| `browser_downloads` | Downloaded files |

**Library:** browserhistory or direct SQLite

---

## 8. Pricing & Productized Onboarding

### 8.1 Value Analysis

| Role | Hourly Rate | Time Saved | Value Created |
|------|-------------|------------|---------------|
| Senior IR Consultant | $400-500/hr | 20-40 hrs/engagement | $8K-20K/engagement |
| IR Boutique (per incident) | N/A | 40+ hrs/incident | $16K-40K/incident |

**Additional value:**
- Don't miss evidence (avoid re-compromise)
- Chain of custody (criminal prosecution success)
- Speed (contain breach faster = less damage)
- **Reproducibility** (re-run same parser on same evidence = identical output)

### 8.2 Productized Onboarding SKUs

**We remain product-first.** Services are delivered as fixed-scope productized onboarding, not bespoke engagements.

| SKU | Scope | Deliverables | Target |
|-----|-------|--------------|--------|
| **DFIR Starter Pack** | Fixed scope, short engagement | Deploy on workstation/server (offline/air-gap friendly); ingest one real case corpus (or redacted); EVTX → governed DuckDB/Parquet + quarantine workflow; evidence-grade manifest template + runbook | Individual consultants, boutique firms |
| **Custom Artifact Pack** | Fixed scope | Implement 1–2 custom artifacts as Casparian parsers (Shimcache, Amcache, Prefetch, etc.); include regression tests against corpus; deliver as internal parser bundle | IR firms with specific artifact needs |
| **Maintenance Subscription** | Recurring | Parser pack updates as Windows artifacts evolve; regression suite + compatibility guarantees; support for backfill planning and controlled upgrades | All tiers |

### 8.3 Pricing Guidance

- Do NOT "race to the bottom" just because AI can generate first-draft code
- Price around: risk/time-to-trust/time-to-maintain and cost of being wrong (silent drift)
- Pricing axes to explore: license, subscription, per-engagement, maintenance

### 8.4 Platform Tiers

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Pro** | $100/user/month | Full platform, example parsers | Individual consultants |
| **Team** | $400/month | Multi-engagement, shared parsers, registry access | Small IR firms |
| **Consultant** | $600/month | White-label, multi-client, priority support | IR consultancies |
| **Enterprise** | Custom | SSO, audit reports, evidence-grade manifest exports | Enterprise SOC/CIRT |

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| DFIR users | 20 | 75 |
| Engagements processed | 30 | 150 |
| Artifacts processed | 500K | 2M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| DFIR MRR | $4K | $20K |
| Paying customers | 12 | 40 |
| Average deal size | $350/mo | $500/mo |

### 9.3 Validation Metrics

| Metric | Target |
|--------|--------|
| "Quarantine saved evidence" stories | 3+ |
| "Faster than Plaso" testimonials | 5+ |
| Platform feedback (not domain) | 10+ requests |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Plaso improves robustness | Medium | Our value is governance + custom, not just robustness |
| Big vendor enters market | Medium | Speed advantage; community-focused |
| Artifact format complexity | Medium | Ship examples, let users customize |
| Forensic validation requirements | Low | Clear documentation on validation |
| Enterprise IR teams prefer suites | Medium | Position as infrastructure layer under suites |

---

## 11. Integration Points

### 11.1 Complementary Tools

| Tool | Integration |
|------|-------------|
| **Velociraptor** | Parse Velociraptor collection outputs |
| **KAPE** | Parse KAPE triage outputs |
| **Timesketch** | Output to Timesketch-compatible format |
| **ELK/Splunk** | Output to JSON for ingestion |

### 11.2 Output Formats

| Format | Use Case |
|--------|----------|
| Parquet | Large-scale analysis |
| CSV | Excel review, simple tools |
| JSON | Timesketch, SIEM ingestion |
| DuckDB | SQL analysis on analyst laptop |

---

## 12. Parser Ecosystem / Registry

> **TAM Analysis:** See [STRATEGY.md Appendix: TAM Expansion via Parser Ecosystem](../STRATEGY.md#appendix-tam-expansion-via-parser-ecosystem-vault-strategy) for full market sizing and ecosystem precedent analysis (Velociraptor, KAPE, Sigma, Airbyte). Key insight: The ecosystem approach is **pragmatic and TAM-expanding** — it turns "parser coverage" into a compounding asset (community contributions for breadth, Vault-certified packs for regulated buyers) without becoming a services shop.

### Why Ecosystem Works for DFIR

DFIR practitioners already share reusable content:
- **Velociraptor**: Over 60% of users develop their own artifacts ([Rapid7 survey](https://www.rapid7.com/blog/post/2023/05/10/the-velociraptor-2023-annual-community-survey/))
- **KAPE**: GitHub repo (KapeFiles) is community-updatable content ([GitHub](https://github.com/EricZimmerman/KapeFiles))
- **Sigma**: 3000+ detection rules as collaboration hub ([GitHub](https://github.com/SigmaHQ/sigma))

The behavioral bet ("security practitioners will contribute long-tail logic") is validated by these ecosystems.

### Open Core Model

**Open (public repos):**
- Casparian Parser Protocol + SDK
- Standard Tables for DFIR artifacts (EVTX, Shimcache, Amcache, Prefetch, etc.)
- Community parser library

**Closed (commercial engine):**
- Authoritative validation (Rust-side schema enforcement)
- Quarantine management + retention policies
- Reproducibility manifests + evidence-grade exports
- Backfill planning + version migration

### Registry Trust Tiers

| Tier | Label | Criteria |
|------|-------|----------|
| **Verified / Gold** | ✓ Verified | Casparian-maintained; regression tested against real artifacts; schema contracts published |
| **Community / Silver** | Community | Tests required; schema contract required; versioning required |
| **Experimental / Bronze** | Experimental | Basic functionality; community-contributed |

---

## 14. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial draft based on strategic fork evaluation |
| 2026-01-20 | 0.2 | Elevated to #1 priority; Added specific pitch; Added "fragile scripts" problem detail |
| 2026-01-21 | 0.3 | **Major update:** Added trust primitives section; Added productized onboarding SKUs; Added parser ecosystem / registry with trust tiers; Updated pricing to reflect product-first approach |
| 2026-01-21 | 0.4 | **Ecosystem validation:** Added TAM analysis cross-reference; Added ecosystem precedent evidence (Velociraptor 60%+ artifact development, KAPE community targets, Sigma rules); Linked to STRATEGY.md appendix for full market sizing |

---

## 15. References

- [SANS DFIR Resources](https://www.sans.org/digital-forensics-incident-response/)
- [DFIR Discord](https://discord.gg/dfir)
- [python-evtx](https://github.com/williballenthin/python-evtx)
- [Eric Zimmerman's Tools](https://ericzimmerman.github.io/)
- [Plaso/log2timeline](https://plaso.readthedocs.io/)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial draft based on strategic fork evaluation |
| 2026-01-20 | 0.2 | Elevated to #1 priority; Added specific pitch ("first IDE for forensic artifact parsing"); Added "fragile scripts" problem detail (construct, kaitai); Emphasized Lineage/Quarantine as "insurance policy" |

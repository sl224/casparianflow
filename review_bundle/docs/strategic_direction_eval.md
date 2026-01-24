# Casparian Flow: Strategic Direction Evaluation Document

**Purpose:** This document provides complete context for evaluating Casparian Flow's strategic direction. It contains product background, market research, pivot history, and the current proposed strategy.

**Date:** January 21, 2026
**Stage:** Pre-launch, Pre-revenue
**Founder Context:** Solo technical founder, bootstrapped

---

## Table of Contents

1. [Product Overview](#1-product-overview)
2. [The Core Problem](#2-the-core-problem)
3. [Technical Architecture](#3-technical-architecture)
4. [Pivot History & Lessons](#4-pivot-history--lessons)
5. [Target Market Evolution](#5-target-market-evolution)
6. [Current Strategic Model](#6-current-strategic-model)
7. [Key Assumptions](#7-key-assumptions)
8. [Financial Projections](#8-financial-projections)
9. [Risks & Mitigations](#9-risks--mitigations)
10. [Open Questions for Evaluation](#10-open-questions-for-evaluation)

---

## 1. Product Overview

### What is Casparian Flow?

Casparian Flow is a **local-first data processing platform** that transforms unstructured/semi-structured files (logs, exports, binary formats) into queryable SQL/Parquet datasets.

**Core Workflow:**
```
Raw Files → Discovery → Parsing → Validation → Clean Data
   │           │           │          │            │
   │     (casparian     (Python    (Schema      (Parquet/
   │       scan)        parsers)  Contracts)    DuckDB)
   │
  Network drives, evidence servers, lab instruments
```

### Key Technical Differentiators

| Feature | Description | Why It Matters |
|---------|-------------|----------------|
| **Schema Contracts** | Declared schemas that parsers must conform to; violations are hard failures | Data quality guarantee; compliance requirement |
| **Quarantine** | Invalid/out-of-range rows isolated, not dropped | Audit trail; no silent data loss |
| **Lineage Tracking** | Every row has `_cf_source_hash`, `_cf_parser_version`, `_cf_job_id` | Traceability; reproducibility |
| **Backtest Loop** | Test parsers against file corpus before production | Rapid parser development; catch edge cases |
| **Local-First** | Runs entirely on local machine; no cloud required | Air-gapped environments; data sovereignty |
| **Source Hash** | Blake3 hash of source file attached to every output row | Prove processed data matches original file |

### What Casparian Is NOT

- **Not a real-time streaming tool** — Batch processing of files at rest
- **Not an ETL orchestrator** — Focuses on the "E" (Extract/parse), not orchestration
- **Not a BI/visualization tool** — Outputs data for other tools to consume
- **Not a no-code tool** — Parsers are Python; users must be technical

---

## 2. The Core Problem

### The "Dark Data" Problem

Organizations have terabytes of files sitting on network drives that they can't query:

```
Typical Enterprise Network Drive:
├── logs/
│   ├── server_2024.log      (custom format)
│   ├── firewall_export.csv  (vendor-specific columns)
│   └── audit_trail.json     (nested, inconsistent)
├── exports/
│   ├── crm_dump.xml         (legacy system)
│   ├── erp_extract.txt      (fixed-width, undocumented)
│   └── vendor_feed.dat      (binary, proprietary)
└── archives/
    ├── email_backup.pst     (Outlook)
    └── old_database.mdb     (Access)
```

**The current options are bad:**

| Option | Cost | Problem |
|--------|------|---------|
| Enterprise ETL (Databricks, Informatica) | $50K-500K/year | Overkill; requires data team; cloud-focused |
| Hire data engineer | $150K+/year | One person; knowledge leaves when they leave |
| Outsource to consultants | $5-15K per project | Slow; recurring cost; no internal capability |
| DIY Python scripts | "Free" | No governance; breaks silently; tribal knowledge |
| Manual (grep, Excel) | Labor | 30-45 min per query; error-prone; doesn't scale |

### Why This Problem Is Hard

1. **Format Diversity:** Every organization has unique formats (vendor exports, legacy systems, custom logs)
2. **No Standards:** Unlike APIs (REST, GraphQL), file formats have no universal schema
3. **Edge Cases:** Real files have corruption, encoding issues, schema drift
4. **Governance Gap:** DIY scripts work but have no audit trail, versioning, or validation

### The Market Opportunity

- Unstructured data management market: $156B by 2034
- 80%+ of enterprise data is unstructured/semi-structured
- Growing compliance requirements (FDA, SOX, legal discovery)

---

## 3. Technical Architecture

### System Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI / TUI                                │
│  casparian scan | casparian run | casparian backtest            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Scout (Discovery)                           │
│  • Scan directories for files                                   │
│  • Tag files by pattern (*.log → "server_logs")                 │
│  • Hash files for deduplication                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Schema Contracts                              │
│  • Define expected output schema                                │
│  • Validate parser output against schema                        │
│  • Hard failure on violation (no silent coercion)               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Worker (Parser Execution)                      │
│  • Run Python parsers in isolated subprocess                    │
│  • Stream Arrow IPC batches                                     │
│  • Add lineage columns to every row                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Output Sinks                                │
│  • Parquet files (default)                                      │
│  • DuckDB database                                              │
│  • CSV (for compatibility)                                      │
└─────────────────────────────────────────────────────────────────┘
```

### Parser Structure

```python
import pyarrow as pa

class MyParser:
    name = 'my_parser'           # Logical name
    version = '1.0.0'            # Semver version
    topics = ['server_logs']     # Subscribe to tagged files

    outputs = {
        'parsed_logs': pa.schema([
            ('timestamp', pa.timestamp('us')),
            ('level', pa.string()),
            ('message', pa.string()),
        ])
    }

    def parse(self, ctx):
        # ctx.input_path - path to source file
        # ctx.source_hash - blake3 hash of source
        # ctx.job_id - unique job identifier

        df = self.process_file(ctx.input_path)
        yield ('parsed_logs', df)
```

### Lineage Columns (Added Automatically)

Every output row includes:

| Column | Type | Description |
|--------|------|-------------|
| `_cf_source_hash` | string | Blake3 hash of source file |
| `_cf_job_id` | string | UUID of processing job |
| `_cf_processed_at` | timestamp | When row was processed |
| `_cf_parser_version` | string | Parser version that produced row |

### Technology Stack

- **Core:** Rust (Tokio async runtime)
- **Database:** DuckDB (default), PostgreSQL (enterprise)
- **Data Format:** Apache Arrow / Parquet
- **Parser Runtime:** Python (isolated subprocess)
- **IPC:** ZeroMQ (Arrow IPC batches)
- **Environment:** UV for Python dependency management

---

## 4. Pivot History & Lessons

### Timeline of Strategic Pivots

```
2025 Q1: "AI-Native Data Platform"
    │
    │   Lesson: AI is enhancement, not core value
    ▼
2025 Q2: "Universal Local ETL"
    │
    │   Lesson: Need vertical focus, not horizontal
    ▼
2025 Q3: "Vertical-First Strategy"
    │
    │   Lesson: Some verticals are traps
    ▼
2026 Q1: "Platform + Productized Onboarding"
    │
    │   Current direction
    ▼
```

### Pivot 1: AI-Native → Local-First (2025 Q1-Q2)

**Original Thesis:** "Claude Code integration via MCP will be the killer feature."

**What We Learned:**
- AI is good at generating parser boilerplate (80%)
- AI fails on edge cases, binary formats, corrupted data (20%)
- Customers want reliability, not AI magic
- Core value is the governance layer (Schema Contracts, Lineage, Quarantine)

**Pivot:** AI moved to "Phase 2 enhancement." Core product works without AI.

### Pivot 2: Horizontal → Vertical Focus (2025 Q2-Q3)

**Original Thesis:** "Universal tool for all data processing."

**What We Learned:**
- "Universal" means competing with Databricks, Fivetran
- Different verticals have different pain points
- Sales messaging must be specific to resonate

**Pivot:** Identified specific verticals where local-first + governance is a must-have.

### Pivot 3: Vertical Cuts (2025 Q4 - 2026 Q1)

**Evaluated Verticals:**

| Vertical | Initial Priority | Final Priority | Why Changed |
|----------|------------------|----------------|-------------|
| Finance (Trade Support) | P0 | **CUT** | They want answers, not databases. Excel users. |
| eDiscovery (Analyst) | P0 | **CUT** | Click "Process" in Relativity. Email vendors when it fails. |
| DFIR (Forensics) | P1 | **#1** | Write Python daily. Legally mandated audit trail. Urgent. |
| Pharma R&D | P2 | **#2** | FDA 21 CFR Part 11 compliance. Source Hash = killer feature. |
| IIoT/OT | P2 | **#3** | Billions of rows in proprietary historians. Data lake initiatives. |
| Satellite/Space | Not considered | **#4** | 50TB/hour telemetry. Binary parsing. Python infrastructure. |
| Defense/GEOINT | P1 | **#5** | Perfect fit, but slow sales. Requires clearances. |

**Key Lesson: The Qualifying Question**

> "When a file fails to parse, do they (a) write a Python script, or (b) email a vendor?"

- (a) → Valid target
- (b) → DO NOT TARGET (they become support burden)

### Pivot 4: Product-Only → Platform + Productized Onboarding (2026 Q1)

**Original Thesis:** "Sell SaaS subscriptions to technical users."

**The Problem:**
- Technical users (DFIR, Pharma engineers) can use the tool
- But they have to write parsers from scratch ("Empty Box" problem)
- High friction to first value
- Enterprise buyers want guaranteed outcomes, not potential

**The Insight:**
- A short, fixed-scope onboarding sprint gets faster revenue and first value
- Open-ended services is a trap (1-2x valuation, linear scaling)
- Hybrid model: Platform + Productized Onboarding

**Current Direction:** See Section 6.

---

## 5. Target Market Evolution

### Current Validated Segments

| Rank | Segment | Technical Buyer | Audit Trail Need | Sales Cycle |
|------|---------|-----------------|------------------|-------------|
| **#1** | DFIR (Incident Response) | Forensic Consultant | Legally mandated | Fast (2-4 weeks) |
| **#2** | Pharma R&D | Lab Data Engineer | FDA required | Slow (3-6 months) |
| **#3** | IIoT/OT | Industrial Data Engineer | Data quality | Medium |
| **#4** | Satellite/Space | Ground Systems Engineer | Mission-critical | Medium |
| **#5** | Defense/GEOINT | Intel Contractor | Classified | Very slow |

### Why DFIR Is #1

| Factor | DFIR | Others |
|--------|------|--------|
| **Urgency** | EXTREME (active breach) | Medium-Low |
| **Audit Trail** | Legally mandated (evidence) | Nice-to-have or compliance |
| **Technical Skill** | Writes Python daily | Varies |
| **Budget Authority** | Partner/Principal decides | Enterprise procurement |
| **Sales Cycle** | 2-4 weeks | 3-12 months |
| **Current Pain** | "Fragile scripts" (Plaso crashes) | Vendor lock-in, manual work |

**The DFIR Value Proposition:**

> "The first IDE for forensic artifact parsing. Stop trusting fragile scripts for evidence."

**Key Insight:** Casparian's **Lineage + Quarantine** is their **insurance policy**. If their script silently drops a row, they've destroyed evidence.

### Why Pharma Is #2 (Highest LTV)

| Factor | Detail |
|--------|--------|
| **The Data** | Mass spec XML, HPLC binary, instrument logs on lab network drives |
| **Compliance** | FDA 21 CFR Part 11: Must prove database matches raw file |
| **Casparian Value** | **Source Hash** proves data integrity |
| **Sales Cycle** | Slow (enterprise), but sticky forever |
| **LTV** | $50K-200K/year; multi-year contracts |

### Explicitly Cut Segments

| Segment | Why Cut |
|---------|---------|
| **Trade Support Analyst** | Want an answer, not a database. Don't write parsers. Excel users. |
| **eDiscovery Analyst** | Click "Process" in Relativity. Email vendor when fails. Would treat us as "Magic Converter" and file support tickets. |
| **General IT Admin** | Use Splunk/Cribl. Want search bars, not schema definitions. |
| **Marketing Agencies** | Data is in APIs (Facebook/Google), not files on network drives. |

---

## 6. Current Strategic Model

### The "Platform + Productized Onboarding Sprint" Model

This model resolves the tension between:
- **Product economics** (high margin, recurring, scalable)
- **First-value friction** (customers need working parsers on day one)

```
┌─────────────────────────────────────────────────────────────────┐
│                      CASPARIAN FLOW                              │
│                        (Platform)                                │
│                                                                  │
│  Schema Contracts │ Backtest │ Lineage │ Quarantine             │
│                                                                  │
│  Customer owns: Platform license + Parser assets                │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Enabled by
                              │
┌─────────────────────────────────────────────────────────────────┐
│                ONBOARDING SPRINT (Fixed Scope)                   │
│                                                                  │
│  • 1-2 week engagement, fixed price, fixed deliverables          │
│  • Build first parsers + demo queries + runbook                  │
│  • Train a named customer engineer (owner)                       │
│  • Hard acceptance criteria; no open-ended support               │
└─────────────────────────────────────────────────────────────────┘
```

### Why This Model Works

**1. Solves the "Empty Box" Problem**

| SaaS-Only | Platform + Onboarding Sprint |
|-----------|------------------------------|
| Customer opens empty IDE | Customer opens working system |
| Must write code to get value | First parsers already run |
| High friction | Low friction |

**2. Preserves Product Economics**

The sprint is capped in scope and time. Ongoing value is driven by the
platform license, not open-ended services.

**3. Parser Library Growth Is Conditional**

Reusable parser IP only accumulates when contracts explicitly grant reuse
rights. Default assumption is "client-specific" unless proven otherwise.

### Onboarding Intensity by Segment

| Segment | Mode | Intensity | Why |
|---------|------|-----------|-----|
| **DFIR** | Accelerator sprint | Low | Technical users; need environment + first hard parsers. |
| **Pharma** | Full glove implementation | High | Validation docs and compliance sign-off. |
| **IIoT** | Migration sprint | Medium | Large exports; lots of data hygiene. |
| **Satellite** | Accelerator sprint | Low | Technical teams; complex CCSDS setup. |
| **Defense** | Partner channel | 0% direct | Clearance required; integrator-led. |

### The Handoff Requirement

**Critical:** Onboarding must lead to self-sufficiency, not dependency.

| Wrong | Right |
|-------|-------|
| Alerts route to us | Alerts route to customer |
| We log in to debug | They log in, we advise |
| Success = they call us | Success = they stop calling |

**Implementation:** Casparian's notifications go to the customer by default.
The platform should nudge ownership and make dependency uncomfortable.

### Pricing Structure (Illustrative)

**DFIR (Accelerator Sprint):**

| Component | Price |
|-----------|-------|
| Platform | $200/user/month |
| Onboarding sprint (3 days) | $5,000 |
| Complex parser add-on (each) | $2,500 |
| **Year 1 (3 users + sprint + 2 parsers)** | **~$17K** |

**Pharma (Full Glove):**

| Component | Price |
|-----------|-------|
| Platform | $50,000/year |
| Implementation sprint | $75,000 |
| Parsers (5 × $5K) | $25,000 |
| Validation docs | Included |
| **Year 1** | **$150,000** |

### Transition Plan

| Phase | Timeline | Focus | Revenue Mix |
|-------|----------|-------|-------------|
| **1. Founder-Led** | Months 0-12 | DFIR accelerator sprints | 60% onboarding / 40% platform |
| **2. Standardize** | Months 12-24 | Repeatable sprint playbooks | 50% onboarding / 50% platform |
| **3. Platform-Dominant** | Months 24+ | Self-serve + enterprise | 30% onboarding / 70% platform |

**Goal:** Onboarding decreases as % of revenue over time. Platform adoption and
parser reuse (where allowed) increase.

---

## 7. Key Assumptions

### Product Assumptions

| Assumption | Evidence | Risk if Wrong |
|------------|----------|---------------|
| Lineage/Quarantine is valuable for compliance | DFIR legal requirements, FDA 21 CFR Part 11 | Core value prop fails |
| Schema Contracts prevent data quality issues | Standard practice in data engineering | Differentiation weakens |
| Local-first matters for target segments | Air-gapped environments (DFIR, Defense) | Could be SaaS instead |
| Python parsers are acceptable | All target segments use Python | Would need no-code option |

### Market Assumptions

| Assumption | Evidence | Risk if Wrong |
|------------|----------|---------------|
| DFIR consultants will pay for tools | They buy forensic software (X-Ways, EnCase) | Need different segment |
| Pharma will pay $50K+ for compliance | They pay $100K+ for validation consultants | Lower price point needed |
| Technical users prefer tools over ongoing services | DFIR writes own scripts today | Services-only business |
| Boutique firms have budget authority | Small firms, fast decisions | Need enterprise only |

### Onboarding Assumptions

| Assumption | Evidence | Risk if Wrong |
|------------|----------|---------------|
| AI provides meaningful leverage on parser boilerplate | GPT-4/Claude good at scaffolding | Margins compress |
| Customers assign an owner and accept handoff | DFIR teams already run tooling | Stuck doing support forever |
| Parser IP accumulates only with explicit reuse rights | Some formats are common | One-off projects, no leverage |
| Founder can deliver productized sprints + product | Common at early stage | Burnout, slow product progress |

### Financial Assumptions

| Assumption | Evidence | Risk if Wrong |
|------------|----------|---------------|
| DFIR Year 1 deal: ~$15-30K | Boutique firm budgets | Too low for sustainability |
| Pharma Year 1 deal: ~$150K | Enterprise compliance budgets | Overstated, need more deals |
| Onboarding margin: 60-70% | Tech-enabled leverage | Need 5x more deals |
| Time to first DFIR deal: 2-4 weeks | Fast sales cycle | Longer runway needed |

---

## 8. Financial Projections

### Year 1 Targets

| Metric | Target | Assumptions |
|--------|--------|-------------|
| DFIR customers | 10 | $20K average deal |
| Pharma customers | 2 | $100K average deal (slower ramp) |
| Total Revenue | $400K | $200K DFIR + $200K Pharma |
| Onboarding % | 55% | ~$220K onboarding |
| Platform % | 45% | ~$180K recurring |
| Gross Margin | 65% | Blended onboarding + platform |

### Year 2 Targets

| Metric | Target | Assumptions |
|--------|--------|-------------|
| DFIR customers | 25 | Including renewals |
| Pharma customers | 5 | Enterprise ramp |
| IIoT customers | 3 | New segment |
| Total Revenue | $1.2M | |
| Onboarding % | 40% | Decreasing |
| Platform % | 60% | Growing |

### Path to $1M ARR

```
Month 1-3:   3 DFIR deals × $20K = $60K
Month 4-6:   4 DFIR deals × $20K = $80K + 1 Pharma × $75K = $155K
Month 7-9:   3 DFIR deals × $25K = $75K + 1 Pharma × $100K = $175K
Month 10-12: 4 DFIR deals × $25K = $100K + 1 Pharma × $100K = $200K

Year 1 Total: ~$600K (revised up from initial conservative estimate)
Run-rate ARR: ~$300K (platform portion)
```

### Unit Economics

| Metric | DFIR | Pharma |
|--------|------|--------|
| Year 1 Revenue | $20K | $150K |
| Year 2 Revenue | $12K (renewal) | $60K (renewal) |
| Gross Margin | 70% | 60% |
| CAC | ~$2K (founder time) | ~$10K (sales cycle) |
| Payback | 1 month | 2 months |
| LTV (3-year) | $44K | $270K |
| LTV:CAC | 22x | 27x |

---

## 9. Risks & Mitigations

### Strategic Risks

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| Onboarding expands into custom services | High | Medium | Fixed scope; hard acceptance criteria; no open-ended support |
| Parser reuse blocked by IP rights | High | Medium | Explicit reuse clauses; track reuse rate |
| DFIR market too small | Medium | Low | Bridge to Pharma/IIoT |
| Founder burnout | High | Medium | Cap at 2-3 concurrent sprints; reserve product weeks |

### Competitive Risks

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| Palantir moves downmarket | Medium | Low | Local-first differentiation; they're cloud-focused |
| Databricks adds governance | Medium | Medium | Vertical-specific parsers; local-first |
| Open-source alternative emerges | Medium | Medium | Onboarding playbook; parser library where allowed |
| Big vendor acquires competitor | Low | Low | Focus on underserved segments |

### Execution Risks

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| AI leverage doesn't materialize | High | Medium | Validate on first 3 deals; adjust pricing |
| Long sales cycles | Medium | Medium | Start with DFIR (fast); use for references |
| Customer churn | Medium | Low | Focus on sticky use cases (compliance) |
| Technical debt | Medium | Medium | Product sprints between engagements |

### Market Risks

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| DFIR firms consolidate | Low | Low | Diversify to Pharma/IIoT |
| Regulatory changes | Low | Low | Regulations usually increase compliance needs |
| Economic downturn | Medium | Medium | Compliance is non-discretionary |

---

## 10. Open Questions for Evaluation

### Strategic Questions

1. **Is the "Platform + Productized Onboarding Sprint" model the right approach for a solo founder?** Or should we be pure product?

2. **Is the DFIR → Pharma → IIoT sequencing correct?** Should we start with a different segment?

3. **Is the "Asset Accumulation Flywheel" realistic?** Will parser IP actually compound, or is every engagement unique?

4. **What's the right onboarding/platform mix target?** We said <30% onboarding by Year 3—is that achievable? Desirable?

5. **Should Defense be deprioritized entirely?** Given clearance requirements and slow sales cycles.

### Tactical Questions

6. **Is $15-30K the right DFIR price point?** Too low for sustainability? Too high for boutique firms?

7. **Is $150K realistic for Pharma Year 1?** Or should we expect smaller initial deals?

8. **How many concurrent onboarding sprints can a solo founder handle?** We assumed 2-3. Is that right?

9. **When should first hire happen?** We said $200K ARR. Too early? Too late?

10. **Should we pursue grants/non-dilutive funding?** NIH SBIR for HL7, Sovereign Tech Fund for open-source Rust.

### Product Questions

11. **Is the "Empty Box" problem real?** Or will technical users (DFIR) self-serve with good docs?

12. **Should we ship premade parsers in v1?** Or let onboarding engagements build the library first?

13. **Is local-first a real differentiator?** Or would a cloud option accelerate adoption?

14. **Is Python the right parser runtime?** Or should we support other languages?

### Market Questions

15. **Is there demand for DFIR tooling beyond Plaso/Autopsy?** Validated pain, but unvalidated willingness to pay for alternatives.

16. **Will Pharma buy from an unknown startup?** Or do we need partnerships/certifications first?

17. **Is IIoT a real opportunity or a distraction?** Historian escape is compelling, but different from DFIR/Pharma.

18. **Should Satellite be on the list at all?** Small market, but perfect technical fit.

### Existential Questions

19. **Can a bootstrapped solo founder compete in enterprise compliance software?** Or does this require VC funding and a team?

20. **Is this a venture-scale business?** Or a profitable lifestyle business? Both are valid, but strategy differs.

21. **What's the exit path?** Acquisition by Palantir/Databricks? IPO (unlikely at this scale)? Profitable independence?

---

## Appendix A: Competitive Landscape

### By Segment

**DFIR:**
| Tool | Price | Gap |
|------|-------|-----|
| Plaso/log2timeline | Free | Crashes on large E01s; no governance |
| X-Ways | $1K+ | Forensic focus, not data engineering |
| Magnet AXIOM | $3K+ | Point-and-click, not programmable |
| Custom Python | Free | No audit trail; fragile |

**Pharma:**
| Tool | Price | Gap |
|------|-------|-----|
| LabWare LIMS | $100K+ | Limited instrument integration |
| Tetra Data Platform | $200K+ | Enterprise only |
| Custom scripts | Free | No compliance features |

**IIoT:**
| Tool | Price | Gap |
|------|-------|-----|
| OSIsoft PI | $50-500K | Proprietary lock-in |
| Palantir Foundry | $1M+ | Overkill |
| Databricks | $50K+ | No historian expertise |

---

## Appendix B: Key Documents

| Document | Location | Purpose |
|----------|----------|---------|
| Master Strategy | `STRATEGY.md` | Overall business strategy |
| DFIR Strategy | `strategies/dfir.md` | DFIR go-to-market |
| Pharma Strategy | `strategies/pharma.md` | Pharma go-to-market |
| IIoT Strategy | `strategies/iiot.md` | IIoT go-to-market |
| Satellite Strategy | `strategies/satellite.md` | Satellite go-to-market |
| eDiscovery Strategy | `strategies/ediscovery.md` | eDiscovery (deprioritized) |
| Architecture | `ARCHITECTURE.md` | Technical architecture |
| V1 Scope | `docs/v1_scope.md` | Product scope for v1 |

---

## Appendix C: Glossary

| Term | Definition |
|------|------------|
| **Schema Contract** | Declared schema that parser output must conform to; violations are hard failures |
| **Quarantine** | Storage for rows that fail validation; isolated but not lost |
| **Lineage** | Metadata tracking (source hash, parser version, job ID) attached to every row |
| **Backtest** | Running a parser against a corpus of files to validate before production |
| **Source Hash** | Blake3 hash of original file; attached to every output row |
| **Onboarding Sprint** | Fixed-scope engagement to deliver first parsers, runbook, and handoff |
| **Asset Accumulation** | Building reusable parser IP through onboarding engagements |
| **Empty Box Problem** | User opens tool with no content; must create everything from scratch |
| **Accelerator Model** | Light onboarding (setup + hard parsers) for technical customers |
| **Full Glove Model** | Heavy onboarding (implementation + validation docs) for enterprise |

---

*End of document. Ready for external evaluation.*

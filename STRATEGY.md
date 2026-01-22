# Casparian Flow: Product Strategy

> Last updated: January 20, 2026

## Executive Summary

Casparian Flow is a **local-first data platform** that transforms industry-specific file formats (FIX logs, HL7 messages, CoT tracks, PST archives) into queryable SQL/Parquet datasets. Unlike cloud ETL tools that require data to leave premises, Casparian runs entirely on local machines—critical for regulated industries with compliance, air-gap, or data sovereignty requirements.

**Core insight:** The bronze layer (raw file → structured data) for industry-specific formats is underserved. Enterprise tools (Databricks, Relativity, Palantir) are overkill for many use cases, while DIY Python lacks governance.

**Key differentiators:**
1. **Deterministic execution** - schema contracts enforced in Rust, not Python; no silent coercion
2. **Local-first execution** - data never leaves the machine; air-gapped and sovereignty-friendly
3. **Trust primitives** - per-row lineage, quarantine semantics, reproducibility manifests
4. **Premade parsers** for arcane formats (EVTX, FIX, HL7, CoT) with DFIR-first focus

**v1 Focus:** DFIR / Incident Response artifact parsing. Case folder ingestion, evidence bundle workflows, Windows-first (EVTX as flagship parser). NOT streaming, NOT orchestration, NOT BI.

V1 scope and success metrics are defined in `docs/v1_scope.md`.

---

## Vision

### North Star
**"Query your dark data in SQL. Locally. With full audit trails."**

### The Problem

Teams with industry-specific file formats face a painful choice:

| Option | Problem |
|--------|---------|
| **Enterprise platforms** (Databricks, Relativity, Palantir) | $50K-$150K+/year; cloud-only; requires data team |
| **DIY Python scripts** | No governance; knowledge lost when author leaves; no audit trail |
| **Vendor services** | $5-15K per engagement; slow turnaround; recurring cost |
| **Manual analysis** (grep, Excel) | 30-45 minutes per query; error-prone; doesn't scale |

**The format gap:** ETL tools (Fivetran, Airbyte) support APIs and standard formats. Industry formats (FIX, HL7, CoT, PST) = DIY.

### The Solution

Casparian provides:
1. **Premade parsers** for industry formats - FIX logs, HL7 messages, CoT tracks, PST archives, load files
2. **Local-first execution** - Data never leaves the machine; works air-gapped
3. **Schema contracts** - Governance layer for compliance; violations are hard failures
4. **SQL/Parquet output** - Query results with familiar tools
5. **Full traceability** - Parser versions, schema history, processing lineage

**AI Enhancement (Phase 2):**
- Claude-assisted custom parser development for formats we don't ship
- AI proposes, humans approve - no AI in the execution hot path

### Core Philosophy

**What We Believe:**
1. **Determinism over convenience.** Execution is reproducible; same inputs + same parser bundle hash = identical outputs.
2. **Local-first, always.** Data sovereignty isn't negotiable. Cloud is optional, local is default.
3. **Governance built-in.** Schema contracts, audit trails, and versioning aren't enterprise add-ons—they're core.
4. **Fail loud, not silent.** Invalid rows go to quarantine with context; no silent coercion into clean tables.
5. **Parsers are the product.** The core value is transforming arcane formats to SQL. Everything else is infrastructure.

**What We Don't Believe:**
- "Cloud is always better" - Regulated industries need local options
- "AI can figure it out" - Premade parsers for known formats beat AI improvisation
- "AI makes tools obsolete" - AI makes first-draft code cheap; operational guarantees remain valuable
- "One tool fits all" - Different verticals have different competitors (see below)

### AI Era: Why We're Defensible

AI makes first-draft code cheaper (one-off parsers, glue, demo UIs). Buyers still pay for operational guarantees:
- **Repeatability** across cases and over time
- **Auditability/defensibility** (what ran, on which inputs, with what versions)
- **Safe failure modes** (quarantine vs silent coercion)
- **Maintenance** as formats evolve
- **Packaging** in constrained environments (offline/air-gap/Windows)
- **Accountability** when something breaks under pressure

Casparian's moat is "integrity as a system" (deterministic runs + schema contracts + quarantine + lineage + reproducibility + backfill planning), not a thin wrapper around AI.

---

## Target Market

### Primary ICP: "Technical Teams Without Data Engineering Infrastructure"

**Critical insight:** The buyer must have technical capability. SMB practice owners (dental, legal, tax) are NOT viable direct buyers - they lack technical staff (79% of sub-50 employee businesses have no full-time IT) and want turnkey solutions.

| Segment | Viable? | Why |
|---------|---------|-----|
| **SMB practice owners** | **NO** | No technical staff; buy from MSPs; want turnkey |
| **MSPs serving SMBs** | **YES** | Have technical staff; serve many clients; can white-label |
| **Mid-market (100-500 employees)** | **YES** | Have IT teams; can't afford enterprise tools |
| **Enterprise (500+)** | **YES** | Have data needs; compliance requirements |
| **Technical consultants** | **YES** | Build for clients; need productivity tools |

### Validated Target Segments (Refined January 2026)

> **Final Prioritization:** Based on constraints analysis—**data at rest on network drives**, **needs schematization**, **technical Python users**, **high willingness to pay**—segments ranked by speed to revenue + product fit.

| Rank | Segment | Technical Buyer | Format Examples | Why They Win |
|------|---------|-----------------|-----------------|--------------|
| **#1** | **DFIR (Incident Response)** | Forensic Consultants | Disk images, Amcache, Shimcache, $MFT, memory dumps | **Urgent + legally mandated audit trail**; air-gapped; writes "fragile scripts" today | [→ Deep Dive](strategies/dfir.md) |
| **#2** | **Pharma R&D Data Engineers** | Lab Data Engineers | Mass spec XML, HPLC binary, instrument logs | **FDA 21 CFR Part 11 compliance**; Source Hash = compliance feature; highest LTV | [→ Deep Dive](strategies/pharma.md) |
| **#3** | **IIoT/OT Data Engineers** | Industrial Data Engineers | Historian exports (PI/IP21), PLC logs, SCADA telemetry | **Billions of rows locked in proprietary historians**; bespoke ETL to Parquet; data lakes initiative | [→ Deep Dive](strategies/iiot.md) |
| **#4** | **Satellite/Space Data Engineers** | Space Data Engineers | CCSDS telemetry, TLE files, binary downlinks | **50TB/hour downlinks**; custom binary formats; Python infra (COSMOS, SatNOGS) | [→ Deep Dive](strategies/satellite.md) |
| **#5** | **Defense/GEOINT** | Intel Contractors | NITF imagery, drone logs, CoT tracks | **Perfect product fit**; air-gapped classified networks; slow sales (target subcontractors) | [→ Deep Dive](strategies/defense_tactical.md) |
| **P3** | **eDiscovery (LST only)** | Litigation Support Technologist | PST, Slack exports, Load Files | "Maybe" bridge market; reluctant Python users; DFIR is where idea landed | [→ Deep Dive](strategies/ediscovery.md) |
| **P3** | **Data Consultants** | Themselves | Client-specific formats | Uses full platform; fast sales; platform feedback | |

### Do Not Target (Explicitly Cut)

| Segment | Why Cut |
|---------|---------|
| **Trade Support Analyst** | They want an *answer*, not a database. Don't write parsers. Excel users. |
| **eDiscovery Analyst** | Click "Process" in Relativity. When fails, email vendor. Treat Casparian as "Magic Converter." **You become free IT support.** |
| **General IT Admin** | Use Splunk/Cribl. Want search bars, not schema definitions. |
| **Marketing Agencies** | Data is in APIs (Facebook/Google), not files on network drives. |

> **Key Insight:** "eDiscovery" is a business process, not a job title. The DFIR Consultant (Python daily) survived; the eDiscovery Analyst (Excel, support tickets) was cut.

### Why DFIR Is #1 (The Winner)

**The v1 wedge:** DFIR / Incident Response artifact parsing teams. The product value story:

Casparian is a **deterministic, governed "data build system"** for file artifacts:
- Schema contracts enforced authoritatively (Rust validation) — no silent coercion
- Quarantine invalid/out-of-range rows — partial success is safe
- Per-row lineage: source hash + job id + processed timestamp + parser version
- Reproducible run identity (content-addressed parser bundle: code + lockfile hash)
- Incremental ingest primitives: version-dedup + backfill planning when parser versions change
- CLI-first; minimal TUI for discovery/bench/jobs/quarantine summary

**We are NOT "another EVTX parser."** We are: "turn DFIR parsing into an auditable, repeatable, backfillable dataset build process."

DFIR consultants are the **only customer** with "network drive data" that is both **urgent** (active breach) and **legally mandated** to have a perfect audit trail:

| Factor | DFIR | Pharma (contrast) | Trade Desk (cut) |
|--------|------|-------------------|------------------|
| **Data Location** | Disk images on air-gapped evidence server | Instrument files on lab network drives | FIX logs on trading servers |
| **Current Tool** | "Fragile" Python scripts (construct, kaitai) | ETL scripts to Snowflake | grep + Excel |
| **Writes Python?** | YES (binary artifact parsing) | YES (instrument data munging) | NO |
| **Urgency** | EXTREME (stop breach NOW) | Medium (nightly batch) | High (T+1) |
| **Audit Trail** | **LEGALLY MANDATED** (evidence chain) | **FDA REQUIRED** (21 CFR Part 11) | Nice-to-have |
| **Why They Pay** | Speed + Liability ("script deletes row = destroyed evidence") | Compliance + Traceability | Time savings |
| **Sales Cycle** | FAST (boutique firms, practitioners decide) | Slow (enterprise) | Medium |

**Key Insight:** Your **Lineage/Quarantine** feature is their insurance policy. "If my script deletes a row, I destroy evidence."

### Why Pharma R&D Is #2 (Highest LTV)

Pharma has the deepest pockets and the most permanent problem:

| Attribute | Detail |
|-----------|--------|
| **The Data** | Terabytes of XML, JSON, binary from Mass Spectrometers & HPLC on shared lab network drives |
| **The Workflow** | Scripts sweep drives nightly, push structured data to Snowflake/Databricks for scientists |
| **Why They Pay** | **FDA 21 CFR Part 11**: Must prove DB data matches raw file on drive |
| **Casparian Value** | **Source Hash** + **Schema Contract** = compliance features |
| **Sales Cycle** | Slower (enterprise), but sticky forever |

### Strategic Positioning: "Universal Local ETL"

**Core Positioning:** Casparian is the **"Cold Data Browser"** - a Splunk alternative for data that can't go to the cloud.

```
Hot Data (real-time) → Splunk, Datadog, cloud SIEM
                           ↑
                     (network connection)
                           ↓
Cold Data (historical) → [CASPARIAN] → Local SQL/Parquet
```

**Why "Universal Local ETL" wins:**
1. **Horizontal appeal:** Every industry has "dark data" on local drives
2. **Cloud cost savings:** Pre-filter before sending to expensive cloud storage
3. **Air-gap compatible:** Works where cloud tools can't
4. **Compliance friendly:** Data never leaves premises

**Strategic Grid (Refined January 2026):**

| | **Urgent + Mandated Audit** | **High LTV + Compliance** | **Massive Data Volume** | **Good Fit, Slow Sales** |
|---|---|---|---|---|
| **Technical (writes Python)** | **DFIR (#1)** | **Pharma R&D (#2)** | **IIoT/OT (#3)**, **Satellite (#4)** | Defense/GEOINT (#5) |
| **Semi-Technical** | eDiscovery (P3) | — | — | Data Consultants |
| **Non-Technical** | ❌ Cut | ❌ Cut | ❌ Cut | ❌ Cut (Trade Desk, IT Admin) |

**The Attack Plan:**

| Phase | Target | Pitch | Goal | Timeline |
|-------|--------|-------|------|----------|
| **Immediate Cash** | Boutique DFIR Firms | *"The first IDE for forensic artifact parsing. Stop trusting fragile scripts for evidence."* | 5-10 consulting licenses; validate Parser Dev Loop | Months 1-3 |
| **Enterprise Growth** | Biotech/Pharma R&D | *"Automated, compliant ingestion for instrument data. 21 CFR Part 11 ready out of the box."* | Large annual contracts ($50K+) | Months 6+ |
| **Industrial Expansion** | IIoT/OT Data Teams | *"Escape your historian. Query decades of PLC data with SQL."* | Data lake modernization contracts ($25K+) | Months 6+ |
| **Space Sector** | Satellite Operators | *"Parse 50TB/hour downlinks. Schema contracts for mission-critical telemetry."* | Ground station contracts | Months 9+ |
| **Dark Horse** | Defense Subcontractors | *"Process classified telemetry locally. Air-gapped, auditable, Python-native."* | Gov contracts via smaller integrators | Months 12+ |

### MSP Channel: B2B2B Opportunity (Researched)

**Why MSPs matter:** MSPs are the path to SMB data without selling directly to non-technical buyers.

**Market Size (2025):**
- Global managed services market: ~$390-440B in 2025, growing 10-15% CAGR
- U.S. MSP market: ~$70B in 2025, projected $116B by 2030
- 220,000+ SMBs use MSPs in the U.S. alone
- SMBs grew MSP usage 26% YoY

**MSP Profile:**

| Tier | Size | Clients | Endpoints | Revenue |
|------|------|---------|-----------|---------|
| Tier 1 (Small) | 1-5 employees | <10 | <500 | <$1.5M |
| Tier 2 (Mid) | 5-20 employees | 10-50 | 500-2,500 | $1.5-5M |
| Tier 3+ (Large) | 20+ employees | 50+ | 2,500+ | $5M+ |

**Why MSPs Would Buy Casparian:**

| Pain | How Casparian Helps |
|------|---------------------|
| Clients have messy data exports (practice management, EHR, accounting) | AI-assisted parser development |
| Can't afford data engineer ($150K+) | Tool cost vs. headcount |
| Client data must stay local (HIPAA, compliance) | Local-first architecture |
| Need differentiation from commodity IT | "Data services" as premium add-on |
| Clients ask for reports/analytics | Bronze layer enables BI downstream |

**MSP Economics:**
- MSPs target 50-70% gross margins
- White-label resellers typically earn 50-70% margins
- Standard reseller programs offer 20-40% margins
- MSPs charge clients per-user ($50-150/user/month) or per-endpoint ($5-20/endpoint/month)

**Casparian MSP Pricing Models:**

| Model | Casparian Price | MSP Markup | End Client Cost | Casparian Revenue |
|-------|-----------------|------------|-----------------|-------------------|
| **Per-MSP (flat)** | $200-500/month | N/A (internal use) | Bundled | Predictable |
| **Per-Client** | $15-30/client/month | 3-5x | $50-100/month | Scales with MSP |
| **White-Label OEM** | $1,000-2,500/month | Full control | MSP sets price | Premium tier |

### The Cost Savings Angle (Validated)

**For organizations that HAVE technical staff:**

| Alternative | Annual Cost | What You Get |
|-------------|-------------|--------------|
| Hire data engineer | $150-200K | One person, might leave |
| Consulting engagement | $50-100K | Project-based, no ownership |
| Enterprise ETL (Fivetran) | $50-200K | Overkill for custom formats |
| **Casparian** | **$2-25K** | AI + infrastructure, team scales |

**ROI example:** A hospital IT team spending 20 hours/week on manual data wrangling → $50K/year in labor. Casparian at $500/month saves $44K/year.

### Why "No Cloud" Matters (Local-First Core)

| Concern | Who Cares | Casparian Answer |
|---------|-----------|------------------|
| **Security classification** | Defense, government | Air-gapped deployment |
| **HIPAA/data sovereignty** | Healthcare, finance | Data never leaves premises |
| **Network restrictions** | Manufacturing, critical infrastructure | Works without internet |
| **Cost control** | Budget-conscious IT teams | No cloud compute bills |

**Cloud Sinks (Optional Extension):**
- Local-first is the **default and core value**
- Cloud is **optional output destination only** (pluggable sinks)
- Supported: Write Parquet to S3, load into cloud SQL (Snowflake, BigQuery, etc.)
- **No cloud control plane** - Casparian runs locally; cloud is just where outputs go
- **No SaaS dependency** - Product works fully offline; cloud connectivity is user-initiated

### Invalidated Hypotheses

| Hypothesis | Finding | Source |
|------------|---------|--------|
| "Dental offices have IT staff" | FALSE - 79% of <50 employee businesses have no full-time IT | Fit Small Business |
| "SMBs would buy Python tools" | FALSE - Practice owners want turnkey; they buy from MSPs | McKinsey via Fit Small Business |
| "No solutions exist for SMB data" | PARTIAL - Industry-specific embedded analytics filling gap | Research on Dentrix, Clio, etc. |

### Anti-Targets (Not For)

- **SMB practice owners** (dental, legal, tax) - no technical staff
- **Teams processing only standard formats** - use Pandas directly
- **Organizations wanting fully managed SaaS** - use Fivetran
- **One-time projects** - hire a consultant instead
- **Non-technical buyers** - need at least Python comfort

---

## Value Proposition

### By Vertical

| Vertical | Value Proposition | Quantified ROI |
|----------|-------------------|----------------|
| **Finance (Trade Ops)** | "Debug trade breaks in 5 minutes, not 45" | 6+ hours/day saved per analyst |
| **Legal (Litigation Support)** | "Process PSTs in-house, stop paying vendors" | $5-15K saved per matter |
| **Healthcare (IT)** | "Query 5 years of HL7 archives with SQL - analytics Mirth can't do" | SQL access to historical data |
| **Defense (Analysts)** | "SQL for CoT/NITF/PCAP on your laptop, air-gapped" | Mission-critical; no alternative |
| **Manufacturing (Engineers)** | "Analyze historian exports without PI licenses" | $50K+/year vs. enterprise |

### Generic Value Props

**For Operations Teams:**
> "Turn file formats your vendors create into SQL you can actually query. Locally. In minutes."

**For Compliance Officers:**
> "Full audit trail: who processed what, when, with which parser version. Schema contracts prevent silent data drift."

**For CTOs:**
> "Process industry-specific formats without $150K/year platform licenses or building a data team."

---

## Product Architecture

### Phase 1: Core Product (No AI Required)

| Component | Purpose | Status |
|-----------|---------|--------|
| **Premade Parsers** | FIX, HL7, CoT, PST, load files → SQL/Parquet | Core value |
| **Scout** | File discovery + tagging by pattern | Core |
| **Schema Contracts** | Governance + validation; violations = hard failures | Core |
| **`casparian run`** | Execute parser against files | Core |
| **`casparian scan`** | Discover and tag files | Core |
| **SQL/Parquet Output** | Query results with DuckDB, pandas, etc. | Core |
| **Parser Versioning** | Audit trail; lineage columns | Core |

### Phase 2: AI Enhancement (Future)

| Component | Purpose | Status |
|-----------|---------|--------|
| **AI Integration (TBD)** | AI-assisted parser development workflows | Future |
| **Parser Lab** | AI-assisted parser generation | Future |
| **`discover_schemas`** | AI proposes schema from sample data | Future |
| **`fix_parser`** | AI suggests fixes for failing parsers | Future |

**Key Principle:** AI helps BUILD parsers, but execution is deterministic. No AI in the production data path.

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Local-first (CLI)** | Defense/healthcare can't use cloud; data never leaves machine |
| **Premade parsers first** | Known formats don't need AI; faster time-to-value |
| **Schema contracts are immutable** | Prevents silent schema drift; compliance requirement |
| **Parser versioning** | Audit trail; rollback capability |
| **SQL/Parquet output** | Users query with familiar tools |
| **SQLite default, Postgres for enterprise** | Simple start, enterprise scalability |
| **AI in Phase 2** | Core value doesn't depend on AI; AI is productivity multiplier |
| **Streaming deferred** | SQLite + ZMQ sufficient at current scale; Redpanda evaluated for future [→ Deep Dive](strategies/streaming_redpanda.md) |

---

## Competitive Landscape

### Key Insight: Competitors Differ by Vertical

Generic comparisons (Fivetran, Airbyte) are misleading. A Trade Support Engineer debugging FIX logs doesn't compare us to Fivetran—they compare us to manual grep, Excel, or enterprise TCA tools.

### Finance: FIX Log Analysis

| Competitor | What It Does | Price | Why It's Not Enough |
|------------|--------------|-------|---------------------|
| [QuantScope](https://quantscopeapp.com/) | FIX log viewer/analyzer | Free | Visualization focus; no SQL output; no batch processing |
| [OnixS FIX Analyser](https://www.onixs.biz/fix-analyser.html) | Query builder for FIX | Commercial | Testing/QA focus, not operations |
| [B2BITS FIXEye](https://www.b2bits.com/trading_solutions/fix_log_analyzers) | Enterprise log search | Enterprise | $$$; monitoring focus, not break resolution |
| [Databricks](https://www.databricks.com/solutions/industries/financial-services) | Data lakehouse | $50K+/year | For data teams, not operations; requires cloud |
| Manual grep + Excel | DIY | "Free" | 30-45 min per trade break; no governance |
| **Casparian** | FIX → SQL locally | $75-400/mo | **Trade lifecycle in 5 minutes; local-first** |

**Why Databricks isn't the answer:** Trade Support Engineers don't have Databricks access. They need tools that work NOW, not data pipelines that take weeks to build.

### Legal: PST/eDiscovery Processing

| Competitor | What It Does | Price | Why It's Not Enough |
|------------|--------------|-------|---------------------|
| [Relativity](https://www.relativity.com/) | Full eDiscovery platform | $150K+/year | Overkill for small firms |
| [GoldFynch](https://goldfynch.com/) | Cloud PST processing | Per-GB | Requires cloud; adds up at scale |
| [Logikcull](https://www.logikcull.com/) | Cloud eDiscovery | $250-500/GB | Per-GB expensive on large matters |
| [Aid4Mail](https://www.aid4mail.com/) | Desktop PST processing | License | Processing only; no SQL output |
| Vendor outsourcing | Processing service | $5-15K/matter | Slow; recurring cost per matter |
| **Casparian** | PST → SQL locally | $75-300/mo | **Process in-house; fixed cost; local** |

**Why small firms can't use Relativity:** 80,000+ law firms with <10 attorneys can't justify $150K/year. They outsource or use manual methods.

### Healthcare: HL7 Archive Analysis

| Competitor | What It Does | Price | Why It's Not Enough |
|------------|--------------|-------|---------------------|
| [Mirth Connect](https://github.com/nextgenhealthcare/connect) | Integration engine | **Commercial (Mar 2025)** | Real-time focus; archive analysis is DIY |
| [Rhapsody](https://www.orionhealth.com/global/platform/rhapsody/) | Enterprise integration | $150K+/year | Enterprise scale; overkill for archives |
| [HL7 Soup](https://www.integrationsoup.com/) | HL7 viewer/editor | License | Viewing, not analytics; no SQL |
| Manual Python | DIY parsing | "Free" | No governance; knowledge lost |
| **Casparian** | HL7 archives → SQL | $X | **Query historical data; local-first** |

**Market context:** Mirth Connect went commercial in March 2025. Organizations paying $20-30K/year want more value from their HL7 data. Casparian is **complementary** - we analyze Mirth's archives, not replace its routing function.

### Defense: Tactical Edge Data

| Competitor | What It Does | Price | Why It's Not Enough |
|------------|--------------|-------|---------------------|
| [Palantir](https://www.palantir.com/) | AI/ML analytics | $10B contract | Requires STRUCTURED data as input |
| ArcGIS Enterprise | GIS platform | $100K+/year | Server required; not edge-capable |
| [NetworkMiner](https://www.netresec.com/?page=NetworkMiner) | PCAP forensics | Free/Commercial | Interactive; no batch pipeline |
| Wireshark/tshark | Packet analysis | Free | Interactive; no SQL output |
| Custom Python | DIY | "Free" | Brittle; no governance |
| **Casparian** | CoT/NITF/PCAP → SQL | $X | **Runs on laptop; air-gapped; upstream of Palantir** |

**Key insight:** We are UPSTREAM of Palantir. Palantir analyzes structured data. Casparian structures the raw files so ANY downstream tool can use them.

### Why Not Just Write Python?

| Casparian Adds | DIY Alternative | Delta |
|----------------|-----------------|-------|
| Premade parsers | You build from scratch | **Weeks → minutes** |
| Schema governance | None | **Required for compliance** |
| Automatic versioning | Git (manual) | **Built-in audit trail** |
| SQL/Parquet output | Custom code | **Query with any tool** |
| File discovery | glob + watchdog | **Pattern-based tagging** |

### Moat Analysis

| Asset | Defensibility | Duration |
|-------|---------------|----------|
| Premade parsers for arcane formats | High | Domain expertise required |
| Local-first + governance | High | Rare combination |
| Schema contract system | Medium | Architecture is novel |
| Parser IP accumulation | High | Switching cost increases over time |
| (Phase 2) AI integration | Medium | 12-18 months to replicate |

### The Awareness Gap (Validated Jan 2026)

Casparian sits in a market gap that buyers don't know exists:

```
┌─────────────────────────────────────────────────────────────┐
│                     ENTERPRISE TIER                          │
│  Databricks, Relativity, Palantir ($50K-$500K/year)         │
│  → Data teams know these exist                               │
└─────────────────────────────────────────────────────────────┘
                          ↑
                    AWARENESS GAP
              (Buyers don't know tools exist here)
                          ↓
┌─────────────────────────────────────────────────────────────┐
│                     DIY / MANUAL TIER                        │
│  grep + Excel + Python scripts ("free")                      │
│  → Operations teams think this is the only alternative      │
└─────────────────────────────────────────────────────────────┘
```

**Why this matters:**

| Observation | Implication |
|-------------|-------------|
| The pain is real | 6+ hours/day lost to manual FIX log analysis |
| The market is large | $156B by 2034 for unstructured data management |
| But buyers won't come to us | They don't know a $300/month solution can exist |
| Category awareness is zero | They think options are: manual (free) or enterprise ($150K+) |

**GTM implication:** We must create category awareness. Buyers need to be shown that mid-market local data tooling exists before they can evaluate it.

**Sales motion:**
1. **Find them** - LinkedIn outreach to Trade Support analysts
2. **Show them** - 60-second demo: "Watch me solve a trade break"
3. **Prove ROI** - Calculator: 40 min × 10 breaks × $75/hr = $X saved

**Why "begging" doesn't happen:** The people who feel "bronze layer" pain (data engineers) already have enterprise tools. The people without tools (Trade Support, Operations) don't frame it as a "bronze layer problem" - they call it "why does this take 45 minutes?"

---

## Go-to-Market Strategy

### GTM Philosophy: Technical Users with Data at Rest

> **Final Insight (Jan 2026):** Target **technical Python users** with **data at rest on network drives** that **requires schematization** and has **high willingness to pay**.

The path keeps focus on users who value the specific architecture (Local-First + Python + Governance) and avoids the support nightmare of non-technical analysts.

**Priority Order:** DFIR (#1) → Pharma R&D (#2) → IIoT/OT (#3) → Satellite/Space (#4) → Defense/GEOINT (#5)

---

### Commercialization: Product-First + Productized Onboarding

**Critical:** We remain **product-first**. We sell adoption via productized onboarding SKUs (fixed scope), not bespoke services-led ingestion. Services are used only as repeatable onboarding + enablement, delivered inside the product framework.

**Productized SKUs:**

| SKU | Scope | Deliverables |
|-----|-------|--------------|
| **DFIR Starter Pack** | Fixed scope, short engagement | Deploy on workstation/server (offline/air-gap friendly); ingest one real case corpus (or redacted); EVTX → governed DuckDB/Parquet + quarantine workflow; evidence-grade manifest template + runbook |
| **Custom Artifact Pack** | Fixed scope | Implement 1–2 custom artifacts as Casparian parsers; include regression tests against corpus; deliver as internal parser bundle |
| **Maintenance Subscription** | Recurring | Parser pack updates as artifacts evolve; regression suite + compatibility guarantees; support for backfill planning and controlled upgrades |

**Pricing Guidance:**
- Do NOT "race to the bottom" on price just because code is promptable
- Price around risk/time-to-trust/time-to-maintain and the cost of being wrong (silent drift)
- Offer pricing axes options (license/subscription/usage) without committing to exact figures until validation

---

### Phase 1: DFIR - Immediate Cash (Months 1-3)

**Why DFIR First (The Winner):**

They are the **only customer** with network drive data that is both **urgent** (active breach) and **legally mandated** for audit trail:

| Factor | Detail |
|--------|--------|
| **The Data** | Disk images, memory dumps, obscure system logs (Amcache, Shimcache, $MFT) on air-gapped evidence servers |
| **Current Tool** | "Fragile" Python scripts using `construct` or `kaitai` to parse binary artifacts |
| **Why They Pay** | **Speed:** Parse 1 hour faster = stop breach sooner. **Liability:** Script deletes row = destroyed evidence |
| **Our Value** | **Lineage/Quarantine** = their insurance policy |
| **Sales Cycle** | FAST - Boutique firms (5-50 people) have decision-makers who are practitioners |

**Target Persona:** DFIR Consultant at boutique firm

**The Pitch:**
> *"The first IDE for forensic artifact parsing. Stop trusting fragile scripts for evidence."*

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Target SANS community, DFIR Discord, LinkedIn | Direct outreach |
| Ship example parsers: Amcache, Shimcache, $MFT, EVTX | Starter kits |
| Demo: "Corrupted artifacts that crash Plaso" | Pain agitation |
| Partner with boutique IR firms | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Pro** | $100/user/month | Full platform, example parsers |
| **Team** | $400/month | Multi-engagement, shared parsers |
| **Consultant** | $600/month | White-label, multi-client |

**Success Metrics:**
- 5-10 consulting licenses in first 3 months
- Validate Parser Dev Loop with real users
- $5K MRR from DFIR segment

---

### Phase 2: Pharma R&D - Enterprise Growth (Months 6+)

**Why Pharma Second (Highest LTV):**

Deepest pockets and most permanent problem:

| Factor | Detail |
|--------|--------|
| **The Data** | Terabytes of XML, JSON, binary from Mass Spectrometers & HPLC on **shared lab network drives** |
| **Current Tool** | Scripts to sweep drives nightly, push to Snowflake/Databricks for scientists |
| **Why They Pay** | **FDA 21 CFR Part 11**: Must prove DB data matches raw file on drive |
| **Our Value** | **Source Hash** + **Schema Contract** = compliance features |
| **Sales Cycle** | Slower (enterprise), but **sticky forever** |

**Target Persona:** Lab Data Engineer at Biotech/Pharma

**The Pitch:**
> *"Automated, compliant ingestion for instrument data. 21 CFR Part 11 ready out of the box."*

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Target lab informatics communities, LinkedIn | Direct outreach |
| Content: "FDA-compliant data pipelines from instrument files" | Compliance angle |
| Partner with lab automation vendors | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Team** | $1,000/month | Multi-instrument, compliance reports |
| **Enterprise** | $50K+/year | SSO, validation packages, support |

**Success Metrics:**
- 2-3 enterprise pilots in first year
- $50K+ annual contracts
- FDA compliance validation documentation

---

### Phase 3: IIoT/OT - Industrial Expansion (Months 6+)

**Why IIoT/OT Third:**

Massive data volumes locked in proprietary historians:

| Factor | Detail |
|--------|--------|
| **The Data** | Billions of rows in OSIsoft PI, AspenTech IP21, Wonderware on **industrial networks** |
| **Current Tool** | Bespoke Python ETL pipelines to convert historian exports to Parquet for data lakes |
| **Why They Pay** | **Data lake modernization**: Escape $100K+/year historian licenses; ML on operational data |
| **Our Value** | **Schema Contracts** = data quality for ML pipelines; **Quarantine** = handle sensor noise |
| **Sales Cycle** | Medium (mid-market manufacturing) |

**Target Persona:** Industrial Data Engineer at manufacturing or utility company

**The Pitch:**
> *"Escape your historian. Query decades of PLC data with SQL."*

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Target industrial data communities, LinkedIn | Direct outreach |
| Content: "From PI to Parquet: modernizing historian data" | Data lake angle |
| Partner with IIoT platform vendors (Uptake, Samsara) | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Team** | $1,000/month | Multi-site, historian parsers |
| **Enterprise** | $25K+/year | Integration support, custom formats |

**Success Metrics:**
- 3-5 manufacturing/utility pilots
- $25K+ annual contracts
- Historian escape velocity: 10B+ rows migrated

---

### Phase 4: Satellite/Space - New Frontier (Months 9+)

**Why Satellite Fourth:**

Emerging sector with perfect technical fit:

| Factor | Detail |
|--------|--------|
| **The Data** | CCSDS telemetry, TLE files, binary downlinks generating **50TB/hour** from satellite constellations |
| **Current Tool** | Python scripts using COSMOS, SatNOGS, custom binary parsers |
| **Why They Pay** | **Mission-critical data integrity**: One parsing bug = lost science; **Volume**: Can't manually review |
| **Our Value** | **Schema Contracts** = validate telemetry before storage; **Lineage** = trace anomalies to source |
| **Sales Cycle** | Medium (startup-friendly sector) |

**Target Persona:** Ground Systems Data Engineer at satellite operator

**The Pitch:**
> *"Parse 50TB/hour downlinks. Schema contracts for mission-critical telemetry."*

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Target NewSpace communities, SmallSat conferences | Direct outreach |
| Content: "CCSDS telemetry to Parquet at scale" | Technical demo |
| Partner with ground station providers | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Mission** | $2,000/month | Multi-satellite, telemetry parsers |
| **Constellation** | $50K+/year | High-volume, custom formats, priority support |

**Success Metrics:**
- 2-3 satellite operator pilots
- Integration with COSMOS or SatNOGS ecosystem
- 1TB+ telemetry processed in demo

---

### Phase 5: Defense/GEOINT - Dark Horse (Months 12+)

**Why Defense Fifth:**

Perfect product fit, but painful sales cycle:

| Factor | Detail |
|--------|--------|
| **The Data** | Satellite telemetry, NITF imagery, drone logs on **air-gapped classified networks** (SIPRNet/JWICS) |
| **Current Tool** | Python (GDAL, NumPy) to "munge" data before analysts see it |
| **Why They Pay** | **National security** - process petabytes of custom binary locally |
| **Cloud Illegal** | Cloud tools are literally illegal on classified networks |
| **Sales Cycle** | Very slow (gov procurement) |

**Strategy:** Target **subcontractors** (smaller defense tech firms) rather than Raytheon directly.

**Target Persona:** Data Engineer at defense subcontractor

**The Pitch:**
> *"Process classified telemetry locally. Air-gapped, auditable, Python-native."*

**Go-to-Market:**
| Action | Target |
|--------|--------|
| SBIR applications (A254-011 "AI for Interoperability") | Non-dilutive funding |
| Target smaller integrators, not primes | Faster decision cycle |
| `casparian bundle` for air-gapped deployment | Required feature |

> **SBIR Status:** Programs expired Sept 30, 2025; awaiting reauthorization. See [strategies/dod_sbir_opportunities.md](strategies/dod_sbir_opportunities.md).

**Success Metrics:**
- 1 SBIR Phase I award
- 3-5 subcontractor pilots

---

---

## Parser Ecosystem / Registry Strategy

### Open Core Model: Open Parsers + Proprietary Engine

**Open (public repos):**
- Casparian Parser Protocol + SDK
- Standard Tables (schema definitions for common artifacts: EVTX, Shimcache, Amcache, etc.)
- Community parser library (parser implementations)

**Closed (commercial engine):**
- Incremental state + deduplication
- Authoritative validation (Rust-side schema enforcement)
- Quarantine management + retention policies
- Reproducibility manifests + evidence-grade exports
- Enterprise governance controls (approvals, audit logs)
- Backfill planning + version migration

**Principle:** "Logic is open; execution guarantees are in the engine."

### Registry Trust Tiers

| Tier | Label | Criteria | Maintenance |
|------|-------|----------|-------------|
| **Verified / Gold** | ✓ Verified | Casparian-maintained; regression tested against real artifacts; schema contracts published | Casparian team |
| **Community / Silver** | Community | Tests required; schema contract required; versioning required; error taxonomy required | Community + Casparian review |
| **Experimental / Bronze** | Experimental | Basic functionality; may lack full test coverage | Community |

### Contribution Guidelines

Parsers submitted to the registry must:
1. Include regression tests against sample artifacts
2. Declare schema contracts for all outputs
3. Follow semantic versioning
4. Use standardized error taxonomy for quarantine classification
5. Pass CI validation (schema validation + sample run)

---

### Phase 4: eDiscovery - Bridge Market Only (P3)

> **Critical Insight:** "eDiscovery" is a business process, not a job title. We segmented it into three personas:

| Persona | Verdict | Why |
|---------|---------|-----|
| **eDiscovery Analyst** | **CUT** | Excel user. Clicks "Process" in Relativity. Files support tickets. |
| **Litigation Support Technologist** | **MAYBE** | Reluctant Python. Can code but hates it. Bridge market. |
| **DFIR Consultant** | **WINNER** | Python daily. This is where the eDiscovery idea actually landed. |

**Bottom Line:** We aren't selling to "Legal Tech" anymore. We're selling to "Cybersecurity & Forensics." Same budgets, much more technical users.

**Approach:** If Litigation Support Technologists find us organically, great. But don't prioritize outreach—focus on DFIR. See [strategies/ediscovery.md](strategies/ediscovery.md) for the narrow LST-only strategy.

---

### Explicitly Cut: Do Not Target

| Segment | Why Cut |
|---------|---------|
| **Trade Support Analyst** | Want an *answer*, not a database. Don't write parsers. Excel users. Service Trap risk. |
| **eDiscovery Analyst** | Click "Process" in Relativity. When parsing fails, email vendor or mark as "Exception." Will treat Casparian as "Magic Converter" and file support tickets. **You become their free IT support.** |
| **General IT Admin** | Use Splunk/Cribl. Want search bars, not schema definitions. |
| **Marketing Agencies** | Data in APIs (Facebook/Google), not files on network drives. |
| **Healthcare IT** | 12-18 month sales cycle, HIPAA requirements. Defer until revenue covers runway. |
| **Bioinformatics** | Lower budget (academic), slower cycle. |

### The Qualifying Question

When evaluating any prospect, ask:

> "When a weird file format fails to parse, do they (a) write a Python script, or (b) email a vendor?"

- **(a) Write a script** → Valid target
- **(b) Email a vendor** → **DO NOT TARGET**

### MSP Channel (Parallel, Lower Priority)

MSP channel is viable for mid-market business formats (QuickBooks, Salesforce), but NOT primary focus. Vertical-specific enterprise sales come first.

**When to activate:** After $50K MRR from direct vertical sales.

**Enterprise Features (required for large deals):**

| Feature | Enterprise Need |
|---------|-----------------|
| SOC2 certification | Security requirement |
| SSO/SAML | IT requirement |
| On-prem deployment | Air-gapped environments |
| Custom SLAs | Support requirement |
| Postgres/MSSQL sinks | Enterprise databases |

**MSP Features (required for scale):**

| Feature | MSP Need |
|---------|----------|
| Multi-tenant dashboard | Manage all clients in one view |
| White-label option | Remove Casparian branding from outputs |
| Usage reporting | Bill clients accurately |
| Bulk parser deployment | Same parser across 50 clients |

**Success metric:** $8K MRR ($100K ARR run rate)

### Business Model: BYOK (Bring Your Own Key)

Users provide their own LLM API keys (Anthropic, OpenAI, local models). Casparian does not proxy or markup API calls.

| Implication | Benefit |
|-------------|---------|
| No API costs for Casparian | Pure SaaS margin (~80-90%) |
| Users control LLM spend | No surprise bills from us |
| Not locked to single provider | Works with any LLM API or local model |
| Air-gapped compatible | Local models (Ollama, vLLM) work |

---

## Value-Based Pricing Strategy

> **Full Specification:** See [specs/pricing.md](specs/pricing.md) for detailed pricing framework.

### Pricing Philosophy: Value Capture, Not Cost-Plus

**Core Principle (Andreessen Framework):** Price by the value created for the customer, not by our costs. Enterprise software should capture **10-20% of the value created**.

**Current Problem:** Initial pricing ($50-100/user/month) captures **<2% of value**—leaving 90%+ on the table and signaling "not enterprise-grade."

### Value Created by Vertical

| Vertical | Value Created | Current Pricing | Gap |
|----------|--------------|-----------------|-----|
| **Finance (Trade Ops)** | $50-200K/desk/year (labor savings + risk reduction) | $900/user/year | **Capturing <2%** |
| **Legal (eDiscovery)** | $50-750K/year (processing cost savings) | $900/user/year | **Capturing <1%** |
| **Defense** | Mission-critical; no alternative exists | $1,200/user/year | **Capturing <1%** |
| **Healthcare** | $100K+ (IT backlog bypass + compliance) | $600/user/year | **Capturing <1%** |
| **Manufacturing** | $100K+ (historian license displacement) | $900/user/year | **Capturing <1%** |

### Why Higher Prices Are Better

Per Marc Andreessen's framework:

1. **Proves the moat:** If customers pay $6K/month, it proves differentiation is real
2. **Funds growth:** Higher margins enable enterprise sales investment
3. **Signals quality:** Defense/healthcare buyers are suspicious of cheap software
4. **Enables customer success:** Higher-paying customers get white-glove onboarding
5. **Faster iteration:** Revenue funds R&D for better product

### Pricing Strategy (System of Record)

Pricing tiers, units, and vertical mappings live in `docs/product/pricing_v2_refined.md`. Strategy docs should not restate tier tables. Use the universal ladder plus the vertical mapping in that doc, with Defense as the only pricing-unit exception.

### Pricing Implementation Roadmap

**Phase 1: Finance Validation (Months 1-3)**
- Offer the Trading Desk tier and test willingness-to-pay
- If accepted: moat validated, continue at this price
- If rejected: gather feedback, adjust

**Phase 2: Legal Testing (Months 3-6)**
- Offer the Litigation Team tier
- Position against vendor processing spend and turnaround speed

**Phase 3: Defense SBIR (Parallel)**
- Offer Tactical/Mission tiers from day one
- Defense expects enterprise procurement signals

### Competitive Pricing Context

| Competitor | Annual Price | Casparian Position |
|------------|-------------|-------------------|
| Bloomberg Terminal | $32,000/seat | 50% of price, more flexibility |
| Relativity | $150,000+ | 15% of price, local-first |
| OSIsoft PI | $100,000+ license | 25% of price, no lock-in |
| Palantir | $1M+/year | Upstream enabler, different value |
| Fivetran (at scale) | $500,000+/year | 10% of price, industry formats |

Even at ~$96K/year (Enterprise tier), Casparian is **the cheap option** compared to enterprise alternatives.

### Revenue Modeling

Revenue scenarios depend on the pricing ladder in `docs/product/pricing_v2_refined.md` and should live in a dedicated financial model to avoid drift. Keep narrative strategy here; maintain numbers in the model.

### Non-Dilutive Funding: Open Core Strategy

> **Full Analysis:** See [strategies/non_dilutive_funding.md](strategies/non_dilutive_funding.md) for detailed grant opportunities.

Casparian can pursue grants to fund core infrastructure while maintaining commercial freedom.

**The Model:**
- **Grants fund:** Open-source Rust crates, parsers, security audits
- **Commercial revenue funds:** Pro/Enterprise features, support, connectors

| Funding Source | Amount | Fit | Deadline |
|----------------|--------|-----|----------|
| **Sovereign Tech Fund** | €50K-€1M | Rust infrastructure | Rolling |
| **NLNet Foundation** | €5K-€50K | Data sovereignty | Feb 1, 2026 |
| **NIH SBIR** | $314K | HL7 interoperability | April 5, 2026 |
| **Open Tech Fund** | $50K-$900K | Journalism/privacy | Rolling |

**Why This Works:**
- Grant committees see commercial path (Finance sales) as sustainability
- Commercial customers see open-source core as trust signal
- No equity dilution; no military entanglements

**Estimated Year 1:** ~$165K in grant funding (in addition to commercial revenue)

---

## Key Metrics

### Product Metrics

| Metric | Definition | Target (6 months) |
|--------|------------|-------------------|
| **WAU** | Weekly active users | 500 |
| **Parsers created** | Total parsers in production | 1,000 |
| **Files processed** | Files through Sentinel | 100K/month |
| **Backtests run** | Parser validation runs | 5K/month |

### Business Metrics

| Metric | Definition | Target (6 months) |
|--------|------------|-------------------|
| **MRR** | Monthly recurring revenue | $10K |
| **Paying customers** | Teams on paid plans | 100 |
| **NRR** | Net revenue retention | >100% |
| **CAC payback** | Months to recover acquisition cost | <6 |

### Health Metrics

| Metric | Definition | Target |
|--------|------------|--------|
| **Activation rate** | % who create first parser | >50% |
| **7-day retention** | % who return in week 2 | >40% |
| **Time to first value** | Minutes to process first file | <15 |
| **Schema approval rate** | % of AI proposals approved | >80% |

---

## Risks and Mitigations

### Technical Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| LLM API churn | Medium | Keep provider adapters thin; AI remains optional |
| Claude improves, needs fewer tools | Medium | Tools provide state/persistence Claude lacks |
| Performance at scale | Low | Rust/Arrow architecture is sound |

### Market Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| AI adoption is slow | High | Standalone CLI/TUI value; hedge bet |
| Anthropic builds this | Medium | First-mover advantage; deeper integration |
| Enterprise sales is hard | High | MSP channel as alternative; shorter sales cycle |
| Target market is smaller than expected | Medium | MSP channel expands reach to SMBs indirectly |
| MSPs don't adopt | Medium | Direct sales track runs in parallel |

### Execution Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Too complex for users | Medium | AI handles complexity; better onboarding |
| Distribution is unclear | High | Developer/data ops community; direct outreach |
| Pricing wrong | Medium | Start low; increase with value proof |

---

## Open Questions

### Product Questions
1. **What's the "holy shit" demo?** The 30-second moment that sells the product.
2. **Standalone vs. AI-only?** How much value without Claude Code?
3. **Schema contracts in v1?** Ship governance now or add later?

### Market Questions
1. **Who are the first 10 customers?** Actual companies, not personas.
2. **What's the most painful format?** Defense logs? HL7? SEC filings?
3. **What would make compliance officers buy?** Specific requirements.

### Business Questions
1. **Pricing model?** Per-seat? Per-file? Per-parser? Per-client (for MSPs)?
2. **Sales motion?** Self-serve? Inside sales? MSP partner program? Enterprise sales?
3. **Partnership with Anthropic?** Co-marketing?

### MSP Channel Questions
1. **Which MSP verticals first?** Healthcare MSPs? Accounting MSPs? Generalist?
2. **Will MSPs pay $20/client/month?** Need pilot data to validate.
3. **MSP sales cycle?** Estimated 30-90 days - is this accurate?
4. **Multi-tenant requirements?** What does "manage all clients" actually look like?
5. **White-label priority?** Is this a deal-breaker or nice-to-have?

---

## Success Criteria

### 6-Month Milestones

| Milestone | Target | Status |
|-----------|--------|--------|
| Demo video live | 2-minute CLI/TUI walkthrough | Not started |
| 10 design partners | Active weekly users | Not started |
| **5 MSP pilots** | Processing client data | Not started |
| 100 free users | WAU on free tier | Not started |
| 10 paying customers | Any paid plan | Not started |
| $5K MRR | Monthly recurring revenue | Not started |
| 1 case study published | "How X uses Casparian" | Not started |
| **MSP pricing validated** | Confirm $20/client works | Not started |

### 12-Month Milestones

| Milestone | Target | Status |
|-----------|--------|--------|
| $50K MRR | Monthly recurring revenue | Not started |
| **25 MSP partners** | Active, paying | Not started |
| **500 end clients via MSPs** | Indirect SMB reach | Not started |
| 1,000 WAU | Weekly active users | Not started |
| 3 enterprise contracts | >$20K/year each | Not started |
| SOC2 certification | Security compliance | Not started |
| AI ecosystem listing | Anthropic/OpenAI communities | Not started |
| **Multi-tenant dashboard** | MSP requirement | Not started |

---

## Appendix: Segments Explicitly Not v1

We are NOT ignoring other markets. We are explicitly sequencing. The following segments are attractive but have gating constraints that make them unsuitable for v1:

| Segment | Why It's Attractive | Why It's NOT v1 (Gating Reason) |
|---------|---------------------|--------------------------------|
| **Healthcare HL7** (batch/file drops) | Compliance + PHI, recurring ingestion | Often message-transport/integration-engine domain (MLLP); incumbents already provide file polling connectors; hospital procurement is slow (12-18 months); needs sharper wedge than "parse HL7" |
| **eDiscovery** | Chain-of-custody, massive corpora | End-to-end platforms dominate (Relativity, Nuix); expectations include OCR/review workflows beyond v1 parsing scope |
| **Payments** (ISO8583/FIX/SWIFT) | High governance + budgets | Often real-time/stateful; offline logs possible but not core wedge; long procurement cycles with entrenched incumbents |
| **Genomics / Sequencing** | Huge file volumes, recurring drops | Success depends on orchestration/compute workflows beyond v1 scope; we are explicitly NOT an orchestrator |
| **GIS / Weather / LiDAR** (Shapefile, NetCDF, GRIB, LAS) | Real file formats | Commoditized OSS exists (GDAL, xarray); lower governance urgency; differentiation harder |
| **Automotive / Semicon / Life Sciences** | Technically strong fit | Longer procurement cycles; revisit post-wedge validation |
| **DICOM / PACS** | Compliance + on-prem | Deeply integrated clinical systems; buying unit expects clinical workflow integration |
| **Utilities / CIM** | Critical infra, standards | Utility procurement + program-driven integration; slow cycles |
| **Defense / Space Telemetry** | Air-gap, integrity | Procurement + bespoke programs; long cycles (target via subcontractors) |

**Note:** Cloud output sinks (S3, cloud SQL) don't change segment ranking. Main blockers are: protocol-vs-file nature, orchestration dependency, and procurement/incumbent lock-in.

---

## Appendix: Trust Primitives / Integrity Guarantees

Casparian provides the following guarantees (these are the "trust primitives" that justify the product):

| Guarantee | Description |
|-----------|-------------|
| **Reproducibility** | Same inputs + same parser bundle hash → identical outputs |
| **Per-row lineage** | Every output row has: `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Authoritative validation** | Schema contracts are enforced in Rust; invalid rows never silently coerce into clean tables |
| **Quarantine semantics** | Invalid rows go to quarantine with error context; runs may succeed partially (PartialSuccess) |
| **Content-addressed identity** | Parser identity = blake3(parser_content + lockfile); changes trigger re-processing |
| **Backfill planning** | When parser version changes, backfill command shows exactly what needs reprocessing |
| **Evidence-grade manifests** | Export includes: inputs + hashes + parser IDs + outputs + timestamps |

---

## Appendix: Success Metrics (DFIR-First)

| Metric | Definition | Target |
|--------|------------|--------|
| **Time-to-first-query** | Minutes from case folder to SQL query | <15 minutes |
| **Quarantine rate** | % rows quarantined per parser version | Track per parser |
| **Reproducibility check** | Same inputs + parser hash → identical outputs | 100% |
| **Backfill accuracy** | Files/jobs selected correctly for reprocessing | 100% |
| **Silent drift incidents** | Parsers that changed output without version bump | 0 |

---

## Appendix: TAM Expansion via Parser Ecosystem (Vault Strategy)

### Executive Take

The "Vault + community contributions + minimal stable protocol" direction is **pragmatic and TAM-expanding**, *as long as we keep the DFIR wedge and don't market Casparian as "a general data processing engine" in the Spark/dbt/Airflow sense.*

It **doesn't magically make v1 horizontal overnight**, but it **materially increases the addressable surface area over time** by turning "parser coverage" into a compounding asset rather than an internal services burden.

### What This Direction Changes (And Why It's Sane)

We're moving from:
- "we ship a tool + we write parsers"

to:
- "we ship an integrity-focused engine + we standardize a parser ecosystem + we sell *trust* (Vault)"

This is aligned with how DFIR ecosystems already work: practitioners share reusable "content" (artifacts/rules/targets), while the execution environment provides operational leverage.

**Concrete Precedent Signals:**

| Ecosystem | Evidence |
|-----------|----------|
| **Velociraptor** | Uses shareable "Artifacts" and Rapid7 reports that most users use built-ins + artifact exchange, and **over 60% of users develop their own artifacts**. [Source](https://www.rapid7.com/blog/post/2023/05/10/the-velociraptor-2023-annual-community-survey/) |
| **KAPE** | Has a GitHub repo (KapeFiles) that "contains all the Targets and Modules" used by KAPE and is positioned as community-updatable content. [Source](https://github.com/EricZimmerman/KapeFiles) |
| **Sigma** | The main repo explicitly says it offers **3000+ detection rules** and is a collaboration hub for defenders. [Source](https://github.com/SigmaHQ/sigma) |

The *behavioral* bet ("security practitioners will contribute long-tail logic") is not speculative.

**Why the "Vault" Layer Is Important:**

A pure community registry creates adoption but also creates **trust problems** (quality, regressions, blame, supply-chain risk). Airbyte's approach is instructive: it introduced explicit connector classifications (Certified vs Community) and documents different support expectations. [Source](https://airbyte.com/blog/introducing-certified-community-connectors)

Our Vault is basically "Certified connectors, but for parsers," plus stronger integrity demands (fixtures, determinism checks, signing, offline bundles).

### TAM Impact

**First: a precision point.** TAM is "how much spending exists for the problem you solve." This direction **doesn't change the existence of demand**, but it **does change what we can credibly claim to serve** and therefore how big our *addressable* market becomes.

**The Quantifiable Context: Two Big Adjacent Markets**

| Market | 2024 Size | 2030 Projection | Source |
|--------|-----------|-----------------|--------|
| **Digital Forensics** | ~$11.45B | ~$26.15B | [Grand View Research](https://www.grandviewresearch.com/horizon/outlook/digital-forensics-market-size/global) |
| **Data Integration** | ~$15.19B | ~$30.27B | [Grand View Research](https://www.grandviewresearch.com/industry-analysis/data-integration-market-report) |

*Caveat: these market numbers include categories we won't serve (services, hardware, streaming, full-stack platforms). But they show we're not in a tiny niche.*

**What the Ecosystem Direction Does to "Potential TAM" in Practice:**

Think of it as a **coverage multiplier**, not a "new market":

| Without Community + Vault | With Community + Vault |
|---------------------------|------------------------|
| Effective addressable market limited by: how many parsers we can build/maintain, how many formats we can support with confidence, and how many "weird" customer formats we can absorb without becoming services | Can credibly expand to many more verticals because: **long-tail formats become supportable** (community writes parser → we don't staff it), **regulated buyers become winnable** (Vault provides trust: tests, signing, offline bundle, support), **standard tables become sticky** across orgs and tools |

This is exactly the ecosystem flywheel in DFIR content systems (Velociraptor artifacts, Sigma rules, KAPE targets).

**What It Does NOT Automatically Do:**

It does **not** automatically make us a credible replacement for:
- Orchestrators (Dagster/Airflow)
- Warehouse ELT (Fivetran-esque)
- Streaming (Kafka/Flink)
- General compute engines (Spark)
- Interface engines in healthcare

Our differentiator remains **governed parsing of files-at-rest** + integrity guarantees.

### The "General Data Processing Engine" Claim

**It IS an exaggeration** if we mean "general-purpose compute platform." If we market that, buyers will assume: distributed compute/cluster execution, orchestration & scheduling, connectors & transports, transformations beyond parsing (joins, incremental models), and SLAs typical of data platforms. Casparian (by v1 constraints) is **not** that.

**It is NOT an exaggeration** if we mean: Casparian is a **general-purpose governed execution engine for parsers that turn file artifacts into typed tables**.

That is real because:
- Our engine is format-agnostic (anything that can be parsed into Arrow)
- We have job identity, lineage, strict validation, quarantine semantics, reproducibility, and corpus backtesting
- The Vault direction strengthens "trust + regression" as a first-class product

**Pragmatic Wording That Won't Backfire:**

| Avoid | Use Instead |
|-------|-------------|
| "General data processing engine" | **"Governed artifact-to-table build system"** |
| "Data platform" | **"Local-first governed ingestion runtime for dark files"** |
| "ETL tool" | **"Deterministic parser execution engine with lineage + quarantine + reproducibility"** |

These claim what we actually do, and still support broad TAM narratives later.

### Risks of the Ecosystem Direction

| Risk | Description | Mitigation |
|------|-------------|------------|
| **Ecosystem trust collapse** | If community parsers are unreliable, "addressable market" shrinks because enterprises won't trust the registry | Vault tiers modeled after connector ecosystems (Airbyte Certified vs Community is a concrete precedent) |
| **"Forkable runner" commoditization** | If our open protocol makes it easy for someone to write a competitor runner, we lose pricing power | Keep "hard stuff" proprietary: incremental state + backfill planning, quarantine tooling, attestations/signing workflows, enterprise policy controls |
| **License/community backlash** | If we build a big ecosystem and later change the rules, we invite a fork (HashiCorp Terraform → OpenTofu cautionary tale). [Source](https://opentofu.org/manifesto/) | Decide up front what is open forever (protocol + SDK + standard tables) and what is closed (engine), and stick to it |
| **Messaging dilution** | A platform narrative can reduce near-term sales because buyers can't map it to a concrete pain | Keep DFIR-first messaging; treat ecosystem/Vault as how we scale coverage, not why the first customer buys |

### Net Impact on TAM

**The "honest" TAM story after this change:**

1. **Near-term SAM stays DFIR-first**, because that's where we can win with EVTX + evidence-grade manifests + quarantine + reproducibility.

2. **Potential TAM expands materially** because we can now plausibly serve many more file-format domains without staffing each parser, by combining:
   - Community contributions for breadth (like Velociraptor artifacts / Sigma rules / KAPE targets)
   - Vault-certified packs for regulated buyers (like Airbyte's Certified vs Community connectors + support tiers)

3. The big "category TAM" anchors we can reference (carefully) are:
   - Digital forensics spend (large and growing)
   - Eventually parts of data integration spend (also large)
   - But we should be explicit that we're addressing the **file-at-rest / dark data / governed parsing** slice

### Bottom Line

- **Not too high in the sky** as an architecture + ecosystem plan.
- **Too high in the sky** only if we start selling it as a generic "data processing engine" today.
- The pragmatic win is: **DFIR wedge + ecosystem/Vault as the scaling mechanism** that expands what we can address over time without becoming a services shop

---

## Appendix: Technical Stack

### Backend (Rust)
- Tokio (async runtime)
- SQLx (database)
- Arrow/Parquet (data format)
- ZeroMQ (worker messaging)

### CLI + TUI
- Ratatui (terminal UI)
- Clap (CLI framework)
- Crossterm (terminal handling)

### Python Runtime
- Polars/Pandas (dataframes)
- PyArrow (IPC)
- UV (environment management)

### Database
- SQLite (default)
- PostgreSQL (enterprise)
- MSSQL (enterprise)

---

## Version History

| Date | Version | Changes |
|------|---------|---------|
| 2025-01 | 1.0 | Initial strategy document |
| 2025-01 | 1.1 | Added MSP channel analysis, revised revenue projections, updated GTM phases |
| 2026-01 | 1.2 | Gap analysis integration: Universal Local ETL positioning; Strategic Grid; Vertical priority; Added Legal eDiscovery vertical |
| 2026-01 | 2.0 | **Major revision:** Repositioned from "AI-native" to "local-first format parser"; Added vertical-specific competitor analysis; Restructured GTM to vertical-first (Finance → Legal → Defense → Healthcare); Clarified AI as Phase 2 enhancement, not core requirement; Added research-backed competitor tables |
| 2026-01 | 2.1 | **Healthcare positioning fix:** Clarified Casparian is complementary to Mirth (archive analytics), not a replacement for real-time routing |
| 2026-01 | 2.2 | **SBIR research:** Added program status note (expired Sept 2025, pending reauth); Identified best-fit topic A254-011; Created [dod_sbir_opportunities.md](strategies/dod_sbir_opportunities.md) |
| 2026-01 | 2.3 | **Non-dilutive funding:** Added Open Core grant strategy; Created [non_dilutive_funding.md](strategies/non_dilutive_funding.md) with verified civilian funding sources (STF, NLNet, OTF, NIH) |
| 2026-01 | 2.4 | **Streaming strategy:** Evaluated Redpanda/ADP for architecture; Documented phased approach (differentiate → complement → integrate); Created [streaming_redpanda.md](strategies/streaming_redpanda.md) |
| 2026-01 | 3.0 | **Major strategic pivot:** Reprioritized target segments based on strategic fork evaluation; P0 = eDiscovery + DFIR (audit trails legally required, writes Python); P1 = Data Consultants; Trade Desk deprioritized to P3 (consultant-delivered only); Added "Consultant-First" GTM strategy; See [strategic_fork_evaluation.md](docs/strategic_fork_evaluation.md) |
| 2026-01 | 3.1 | **Refined prioritization:** DFIR confirmed as #1 (urgent + legally mandated); Added Pharma R&D as #2 (highest LTV, FDA 21 CFR Part 11); Defense/GEOINT as #3 (dark horse); eDiscovery demoted to P2; Explicitly cut Trade Desk, IT Admin, Marketing; Added "Attack Plan" with specific pitches |
| 2026-01 | 3.2 | **eDiscovery segmentation:** Split into 3 personas (Analyst=CUT, LST=Maybe, DFIR=Winner); Added eDiscovery Analyst to "Do Not Target" list; Demoted eDiscovery to P3 bridge market; Added "Qualifying Question" (write script vs email vendor); Clarified "Legal Tech → Cybersecurity & Forensics" pivot |
| 2026-01 | 3.3 | **Expanded target segments:** Added IIoT/OT Data Engineers as #3 (historian escape, data lake modernization); Added Satellite/Space Data Engineers as #4 (CCSDS telemetry, NewSpace boom); Defense/GEOINT moved to #5; Created strategies/iiot.md and strategies/satellite.md; Updated attack plan with Industrial Expansion and Space Sector phases |
| 2026-01-21 | 4.0 | **Major update per authoritative decisions:** Added AI-era defensibility narrative; Added open ecosystem / parser registry strategy with trust tiers; Added productized onboarding SKUs (DFIR Starter Pack, Custom Artifact Pack, Maintenance); Added Trust Primitives / Integrity Guarantees appendix; Added Segments Not v1 appendix with gating reasons; Added cloud sinks as optional extension; Emphasized DFIR-first v1 focus; Clarified NOT streaming/orchestrator/BI/no-code/AI-dependent |
| 2026-01-21 | 4.1 | **TAM validation appendix:** Added "TAM Expansion via Parser Ecosystem (Vault Strategy)" appendix with market size context (Digital Forensics ~$11.45B, Data Integration ~$15.19B), ecosystem precedents (Velociraptor, KAPE, Sigma, Airbyte), risk analysis (trust collapse, forkable runner, license backlash, messaging dilution), and positioning guidance (avoid "general data processing engine" claim) |

---

*This document should be reviewed and updated monthly as the product evolves.*

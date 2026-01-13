# Casparian Flow: Product Strategy

> Last updated: January 2026

## Executive Summary

Casparian Flow is a **local-first data platform** that transforms industry-specific file formats (FIX logs, HL7 messages, CoT tracks, PST archives) into queryable SQL/Parquet datasets. Unlike cloud ETL tools that require data to leave premises, Casparian runs entirely on local machines—critical for regulated industries with compliance, air-gap, or data sovereignty requirements.

**Core insight:** The bronze layer (raw file → structured data) for industry-specific formats is underserved. Enterprise tools (Databricks, Relativity, Palantir) are overkill for many use cases, while DIY Python lacks governance.

**Key differentiators:**
1. **Premade parsers** for arcane formats (FIX, HL7, CoT, PST, load files)
2. **Local-first execution** - data never leaves the machine
3. **Schema contracts** - governance and audit trails for compliance
4. **AI-assisted development** (Phase 2) - Claude helps build custom parsers

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
1. **Parsers are the product.** The core value is transforming arcane formats to SQL. Everything else is infrastructure.
2. **Local-first, always.** Data sovereignty isn't negotiable. Cloud is optional, local is default.
3. **Governance built-in.** Schema contracts, audit trails, and versioning aren't enterprise add-ons—they're core.
4. **AI enhances, humans decide.** AI can help build parsers, but execution is deterministic. No AI in production data paths.
5. **Show results, not code.** Users care about output tables, not parser implementation.

**What We Don't Believe:**
- "Cloud is always better" - Regulated industries need local options
- "AI can figure it out" - Premade parsers for known formats beat AI improvisation
- "One tool fits all" - Different verticals have different competitors (see below)

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

### Validated Target Segments

| Segment | Technical Buyer | Format Examples | Why Casparian |
|---------|-----------------|-----------------|---------------|
| **Financial Services** | Trade ops, quant teams | FIX logs, SEC filings, ISO 20022 | Trade break resolution; audit trails | [→ Deep Dive](strategies/finance.md) |
| **Defense/Aerospace** | In-house dev teams | CoT, NITF, PCAP, KLV telemetry | Air-gapped; compliance; custom formats | [→ Deep Dive](strategies/defense_tactical.md) |
| **Healthcare IT** | Hospital IT departments | HL7 exports, EHR dumps, lab systems | HIPAA; can't use cloud; legacy formats | [→ Deep Dive](strategies/healthcare_hl7.md) |
| **Legal Tech/eDiscovery** | Litigation support | PST archives, load files, Slack | Pre-processing tier; cost reduction | [→ Deep Dive](strategies/legal_ediscovery.md) |
| **Manufacturing (mid-market+)** | Plant IT teams | Historian exports, MTConnect, SPC | No cloud on factory floor; proprietary formats | [→ Deep Dive](strategies/manufacturing.md) |
| **Mid-Size Business** | FP&A analysts, IT generalists | QuickBooks, Salesforce, ERP exports | Can't afford enterprise ETL; Excel hell | [→ Deep Dive](strategies/midsize_business.md) |
| **Government/Public Sector** | Agency IT teams | Permit systems, legacy databases | Budget constraints; data must stay local |
| **Technical Consultants** | Themselves | Client data projects | 10x productivity; reusable parsers |

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

**Strategic Grid:**

| | **Low Complexity** | **High Complexity** |
|---|---|---|
| **High $$$** | **FINANCE** (Trade Break Workbench) | Defense (SBIR pathway) |
| **Low $$$** | Mid-size Business (PLG) | Healthcare (long sales cycle) |

**Recommended Priority:**
1. **Phase 1:** Finance (Trade Break Workbench) - Fastest path to revenue
2. **Phase 2:** Legal eDiscovery - Adjacent to finance, similar buyers
3. **Phase 3:** Defense - SBIR pathway, longer timeline
4. **Phase 4:** Healthcare - Requires compliance certifications

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

### Why "No Cloud" Matters

| Concern | Who Cares | Casparian Answer |
|---------|-----------|------------------|
| **Security classification** | Defense, government | Air-gapped deployment |
| **HIPAA/data sovereignty** | Healthcare, finance | Data never leaves premises |
| **Network restrictions** | Manufacturing, critical infrastructure | Works without internet |
| **Cost control** | Budget-conscious IT teams | No cloud compute bills |

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
| **MCP Tools** | Claude Code integration for custom parser development | Future |
| **Parser Lab** | AI-assisted parser generation | Future |
| **`discover_schemas`** | AI proposes schema from sample data | Future |
| **`fix_parser`** | AI suggests fixes for failing parsers | Future |

**Key Principle:** AI helps BUILD parsers, but execution is deterministic. No AI in the production data path.

### MCP Tools (Phase 2)

When AI features ship, 9 MCP tools will enable Claude Code integration:

| Category | Tools | Human Approval Required |
|----------|-------|-------------------------|
| **Discovery** | `quick_scan`, `apply_scope` | No |
| **Schema** | `discover_schemas`, `approve_schemas`, `propose_amendment` | **Yes** |
| **Validation** | `run_backtest`, `fix_parser` | No (3 iteration limit) |
| **Execution** | `execute_pipeline`, `query_output` | No |

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
| (Phase 2) MCP integration | Medium | 12-18 months to replicate |

---

## Go-to-Market Strategy

### GTM Philosophy: Vertical-First, Not Horizontal

Generic "data platform" positioning fails. Each vertical has different:
- **Competitors** (Databricks vs. Relativity vs. Palantir)
- **Buyers** (Trade Support vs. Litigation Support vs. Intel Analyst)
- **Sales cycles** (30 days vs. 18 months)
- **Pricing tolerance** ($75/mo vs. $150K/year)

**Priority Order:** Finance → Legal → Defense → Healthcare

### Phase 1: Finance / Trade Break Workbench (Months 0-6)

**Why Finance First:**
- Highest willingness to pay
- Fastest sales cycle (operations budget, not IT budget)
- Quantifiable ROI (40 min saved × 10 breaks/day = 6+ hours)
- T+1 settlement pressure creates urgency

**Target Persona:** Trade Support Engineer / Middle Office

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Ship FIX parser with `fix_order_lifecycle` table | Core product |
| Demo: "Debug trade break in 5 minutes" | 2-minute video |
| Direct outreach to prop trading firms, broker-dealers | 50 companies |
| Content: "T+1 is killing your Trade Support team" | SEO + thought leadership |
| Partner with FIX protocol consultants | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Pro** | $75/user/month | FIX parser, unlimited files |
| **Trading Team** | $400/month | Multi-venue, custom tags |
| **Enterprise** | Custom | Audit trails, SSO, support |

**Success Metrics:**
- 10 trading desks using weekly
- $10K MRR from finance vertical
- 3 case studies published

### Phase 2: Legal / eDiscovery (Months 6-12)

**Why Legal Second:**
- Adjacent to finance (similar buyer profile)
- Clear ROI ($5-15K saved per matter)
- Small firm market underserved (80K+ firms)

**Target Persona:** Litigation Support Specialist

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Ship PST parser with email/attachment extraction | Core product |
| Demo: "Process 50GB PST in 30 minutes" | Cost savings angle |
| Target litigation support communities (ACEDS) | Direct outreach |
| Partner with legal tech consultants | Channel |

**Pricing:**
| Tier | Price | Features |
|------|-------|----------|
| **Pro** | $75/user/month | PST + load file parsers |
| **Team** | $300/month | Multi-custodian, export |
| **Consultant** | $500/month | White-label, multi-client |

**Success Metrics:**
- 25 legal customers
- $5K MRR from legal vertical

### Phase 3: Defense / SBIR Pathway (Months 6-18)

**Why Defense Third:**
- Longer sales cycle (SBIR process)
- Requires air-gap features (bundle mode)
- High value but slow to close

**Target:** AFWERX, DIU, SOCOM SBIR topics

> **SBIR Program Status (Jan 2026):** Programs expired Sept 30, 2025; awaiting congressional reauthorization (expected late Jan 2026). Best-fit topic identified: **A254-011 "AI for Interoperability"** - almost a verbatim description of Casparian's capabilities. See [strategies/dod_sbir_opportunities.md](strategies/dod_sbir_opportunities.md) for detailed analysis.

**Go-to-Market:**
| Action | Target |
|--------|--------|
| Ship CoT + PCAP + NITF parsers | Core product |
| `casparian bundle` for air-gapped deployment | Required feature |
| Register in SAM.gov + DSIP Portal | Required for federal work |
| SBIR Phase I application | $50-250K funding |
| Demo to DoD stakeholders | Build relationships |

**Success Metrics:**
- 1 SBIR Phase I award
- 5 DoD pilot users

### Phase 4: Healthcare / HL7 Archive Analytics (Months 12-24)

**Why Healthcare Fourth:**
- Long sales cycle (12-18 months)
- Requires compliance certifications (HIPAA)
- Mirth went commercial (Mar 2025) - organizations want more value from HL7 data

**Defer until:** Finance + Legal revenue covers runway

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
| Not locked to single provider | Works with any MCP-compatible LLM |
| Air-gapped compatible | Local models (Ollama, vLLM) work |

### Revenue Projections: Scenario Analysis

**Caveat:** These are modeled scenarios, not projections. Actual revenue depends on validated pricing and conversion rates.

#### Scenario A: Direct Sales Only (No MSP Channel)

| Segment | # Customers | Avg MRR | Annual Revenue |
|---------|-------------|---------|----------------|
| Pro (individuals) | 200 | $50 | $120K |
| Team (mid-market) | 50 | $400 | $240K |
| Enterprise | 10 | $2,000 | $240K |
| **Total** | **260** | | **$600K ARR** |

*Assumption: 1-2% conversion from free tier, 18-month enterprise sales cycle*

#### Scenario B: MSP Channel Focus

| MSP Tier | # MSPs | Avg Clients | Price/Client | Annual Revenue |
|----------|--------|-------------|--------------|----------------|
| Tier 1 (small) | 100 | 5 | $20/mo | $120K |
| Tier 2 (mid) | 50 | 25 | $20/mo | $300K |
| Tier 3 (large) | 10 | 75 | $25/mo | $225K |
| White-label OEM | 5 | N/A | $1,500/mo | $90K |
| **Total** | **165 MSPs** | **~2,000 end clients** | | **$735K ARR** |

*Assumption: Per-client pricing; MSP marks up 3-5x to end clients ($60-100/client)*

#### Scenario C: Hybrid (MSP + Direct)

| Channel | Annual Revenue | % of Total |
|---------|----------------|------------|
| MSP channel | $500K | 45% |
| Direct mid-market | $300K | 27% |
| Enterprise | $300K | 27% |
| **Total** | | **$1.1M ARR** |

*This is the most likely scenario at 24-month maturity*

#### Why MSP Channel Changes the Math

| Factor | Direct to SMB | Via MSP Channel |
|--------|---------------|-----------------|
| Technical buyer? | NO (practice owner) | YES (MSP technician) |
| Sales motion | Impossible (no IT staff) | B2B (MSP is the customer) |
| Support burden | High (non-technical users) | Low (MSP handles end-user) |
| Price sensitivity | High | Lower (MSP bundles) |
| Scalability | Linear (1 customer = 1 SMB) | Leverage (1 MSP = 10-50 SMBs) |

**Key insight:** 100 MSP customers could represent 1,000-5,000 end SMB clients. The MSP channel provides leverage that direct SMB sales cannot.

#### Revenue Confidence Levels

| Scenario | 12-Month | 24-Month | Confidence |
|----------|----------|----------|------------|
| Conservative | $100K | $400K | High |
| Base case | $250K | $800K | Medium |
| Optimistic | $500K | $1.5M | Low |

**What we still don't know:**
1. Will MSPs pay $20/client/month? (Need 5 pilot MSPs to validate)
2. MSP sales cycle length (estimated 30-90 days vs. 12-18 months enterprise)
3. Churn rate (MSPs may drop tools faster than enterprises)
4. White-label demand (requires more product maturity)

**Recommendation:** Target 5 MSP pilots in first 90 days to validate per-client pricing model.

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
| **MCP tool calls** | Claude → Casparian interactions | 10K/month |

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
| MCP protocol changes | Medium | Abstract MCP layer; track Anthropic roadmap |
| Claude improves, needs fewer tools | Medium | Tools provide state/persistence Claude lacks |
| Performance at scale | Low | Rust/Arrow architecture is sound |

### Market Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| MCP adoption is slow | High | Standalone CLI/TUI value; hedge bet |
| Anthropic builds this | Medium | First-mover advantage; deeper integration |
| Enterprise sales is hard | High | MSP channel as alternative; shorter sales cycle |
| Target market is smaller than expected | Medium | MSP channel expands reach to SMBs indirectly |
| MSPs don't adopt | Medium | Direct sales track runs in parallel |

### Execution Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Too complex for users | Medium | AI handles complexity; better onboarding |
| Distribution is unclear | High | Claude Code community; direct outreach |
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
3. **Partnership with Anthropic?** MCP directory listing? Co-marketing?

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
| Demo video live | 2-minute Claude + Casparian | Not started |
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
| MCP directory listing | Anthropic ecosystem | Not started |
| **Multi-tenant dashboard** | MSP requirement | Not started |

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

---

*This document should be reviewed and updated monthly as the product evolves.*

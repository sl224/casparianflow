# Casparian Flow: Product Strategy

> Last updated: January 2025

## Executive Summary

Casparian Flow is an **AI-native data platform** that transforms unstructured files into production-quality datasets through conversational interaction with Claude Code. Unlike traditional ETL tools, Casparian provides the infrastructure, safety rails, and audit trails needed for AI-assisted data engineering in regulated industries.

**Core insight:** The bronze layer (raw file → structured data) for custom formats is underserved. Teams with Python knowledge but no data engineering infrastructure struggle to operationalize custom parsers.

**Key differentiator:** Full MCP integration with human-in-the-loop governance. Users have a conversation with Claude; Casparian ensures safety, traceability, and compliance.

---

## Vision

### North Star
**"Have a conversation with Claude, get production data pipelines with full governance."**

### The Problem

Teams with proprietary file formats (defense logs, medical exports, financial reports) face a painful reality:

| Current State | Pain |
|---------------|------|
| Write parser scripts | No monitoring, retry, versioning |
| Run manually or via cron | Failures go unnoticed |
| Schema changes break everything | No drift detection |
| Original author leaves | Knowledge lost |
| Compliance asks "who changed what?" | No audit trail |

**The bronze layer gap:** Tools like Fivetran/Airbyte assume standard formats and API sources. Custom formats = DIY.

### The Solution

Casparian provides:
1. **AI-assisted parser development** - Claude writes parsers, humans approve
2. **Schema contracts** - Immutable after approval, violations are hard failures
3. **File discovery (Scout)** - Automatic scanning and tagging
4. **Isolated execution (Bridge Mode)** - AI-generated code runs safely
5. **Full traceability** - Parser versions, schema history, processing logs

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
| **Defense/Aerospace** | In-house dev teams | Mission logs, telemetry, sensor data | Air-gapped; compliance; custom formats |
| **Healthcare IT** | Hospital IT departments | HL7 exports, EHR dumps, lab systems | HIPAA; can't use cloud; legacy formats |
| **Financial Services** | Quant teams, compliance IT | Trading logs, SEC filings, risk data | Audit trails; data sovereignty |
| **Manufacturing (mid-market+)** | Plant IT teams | Equipment logs, quality data, SCADA | No cloud on factory floor; proprietary formats |
| **Government/Public Sector** | Agency IT teams | Permit systems, legacy databases | Budget constraints; data must stay local |
| **Technical Consultants** | Themselves | Client data projects | 10x productivity; reusable parsers |

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

### For Data Teams
> "10x productivity on custom formats. Claude writes the parsers, you approve the schemas, Casparian handles the infrastructure."

### For Compliance Officers
> "AI-assisted data processing with full governance. Every schema change requires human approval. Complete audit trail."

### For CTOs
> "An AI data engineer that costs $200/month instead of $200K/year. Production-quality pipelines from day one."

---

## Product Architecture

### Core Subsystems

| Subsystem | Purpose | AI Integration |
|-----------|---------|----------------|
| **Scout** | File discovery + tagging | `quick_scan`, `apply_scope` |
| **Parser Lab** | Parser development + testing | `discover_schemas`, `fix_parser` |
| **Schema Contracts** | Governance + validation | `approve_schemas`, `propose_amendment` |
| **Backtest Engine** | Multi-file validation | `run_backtest` |
| **Sentinel** | Job orchestration | `execute_pipeline` |
| **Query Layer** | Result exploration | `query_output` |

### MCP Tools (9 Total)

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
| **Schema contracts are immutable** | AI safety; prevents silent schema drift |
| **Human approval for schema changes** | Governance requirement for regulated industries |
| **Bridge Mode (isolated execution)** | AI-generated code runs in subprocess; security |
| **Parser versioning** | Audit trail; rollback capability |
| **SQLite default, Postgres for enterprise** | Simple start, enterprise scalability |

---

## Competitive Landscape

### Category: AI-Native Data Operations

Casparian is creating a new category. Traditional comparisons don't apply.

| Competitor | Custom Parsers | AI Integration | Human-in-Loop | Local-First |
|------------|----------------|----------------|---------------|-------------|
| Fivetran | No | None | No | No |
| Airbyte | Limited | Basic | No | No |
| Databricks | Yes | Copilot | No | No |
| dbt | No (SQL) | None | No | No |
| Manual Python | Yes | None | N/A | Yes |
| **Casparian** | **Yes** | **Full MCP** | **Yes** | **Yes** |

### Moat Analysis

| Asset | Defensibility | Duration |
|-------|---------------|----------|
| MCP integration (9 tools) | High | 12-18 months to replicate |
| Schema contract system | Medium | Architecture is novel |
| Local-first + governance | High | Rare combination |
| Parser IP accumulation | High | Switching cost increases over time |

### Why Not Just Write Python?

| Casparian Adds | DIY Alternative | Delta |
|----------------|-----------------|-------|
| AI writes parsers | You write parsers | **10x faster** |
| Schema governance | None | **Required for compliance** |
| Automatic versioning | Git (manual) | **Built-in audit trail** |
| Isolated execution | subprocess (manual) | **Security by default** |
| File discovery | glob + watchdog | **Pattern-based tagging** |
| Human approval workflow | None | **AI safety** |

---

## Go-to-Market Strategy

### Phase 1: Prove Value (0-3 months)

**Goal:** 10 design partners actively using the product

| Action | Target |
|--------|--------|
| Create 2-minute demo video | Claude → data pipeline in action |
| Direct outreach | 50 target companies in defense/healthcare/finance |
| Claude Code community | Power users building MCP tools |
| Document case studies | "How X processes Y format with Casparian" |

**Success metric:** 5 teams using Casparian weekly

### Phase 2: MSP Channel Development (3-6 months)

**Goal:** 5 MSP partners, $15K MRR

| Action | Target |
|--------|--------|
| Identify 50 Tier 2 MSPs (healthcare, accounting focus) | LinkedIn, MSP communities |
| Build MSP demo: "Process 10 client data exports in 1 hour" | Operational efficiency angle |
| Create per-client pricing model | $15-25/client/month |
| Partner onboarding: 30-minute setup to first client | Low-friction trial |
| MSP-specific features: multi-tenant dashboard, white-label option | Roadmap items |

**MSP Value Prop:**
> "Offer data services to every client without hiring a data engineer. Casparian + your technician = premium analytics offering."

| Pricing Tier | Price | For |
|--------------|-------|-----|
| **MSP Starter** | $200/month flat | Up to 10 clients |
| **MSP Growth** | $20/client/month | 10+ clients, volume discount |
| **MSP White-Label** | $1,500/month | Full rebrand, priority support |

**Success metric:** 5 MSPs processing client data weekly

### Phase 2b: Direct Sales (Parallel Track)

**Goal:** $5K MRR from non-MSP customers

| Pricing Tier | Price | Features |
|--------------|-------|----------|
| **Free** | $0 | Parser Lab, 3 parsers, local only |
| **Pro** | $50/user/month | Unlimited parsers, Scout, team sharing |
| **Team** | $200/month + $20/user | Sentinel orchestration, audit logs |

**Success metric:** 50 paying direct users

### Phase 3: Scale Both Channels (6-12 months)

**Goal:** $100K ARR

| Channel | Target | Revenue |
|---------|--------|---------|
| MSP partners | 25 MSPs, 500 end clients | $60K ARR |
| Direct mid-market | 20 Team accounts | $25K ARR |
| Enterprise | 2 contracts | $40K ARR |

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

---

*This document should be reviewed and updated monthly as the product evolves.*

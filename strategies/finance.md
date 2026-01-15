# Financial Services Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → Financial Services)
**Version:** 0.3
**Date:** January 14, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the financial services data analytics market by positioning against expensive enterprise solutions (Bloomberg Terminal, Refinitiv) and complex ETL tooling (Fivetran, Airbyte).

**Core Insight:** Financial data is trapped in expensive terminals, proprietary formats, and regulatory filings. Teams with Python skills can't easily operationalize parsing for FIX, XBRL, or ISO 20022 without significant engineering investment.

**Primary Attack Vector:** "The Trade Break Workbench" - enable Trade Support Engineers to debug trade breaks in 5 minutes instead of 45 by reconstructing trade lifecycles from FIX logs into queryable SQL.

**Secondary Attack Vector:** "Quant Liberation" - give quantitative analysts and compliance teams direct access to alternative data, SEC filings, and trade data without $30K/year terminal subscriptions.

**Strategic Positioning:** Casparian is a **"Liability Shield"** (compliance/audit trail) and **"Risk Reducer"** (faster trade break resolution = less settlement risk).

---

## 2. Market Overview

### 2.1 Key Format Landscapes

| Format | Domain | Volume | Complexity |
|--------|--------|--------|------------|
| **FIX Protocol** | Trade execution, market data | 100M+ msgs/day industry-wide | Medium |
| **SEC EDGAR/XBRL** | Public company filings | 18M+ filings since 1993 | Medium-High |
| **ISO 20022 (MX)** | Cross-border payments (SWIFT) | $2.5 quadrillion settled annually | High |
| **Alternative Data** | Satellite, credit card, sentiment | Growing 40%+ CAGR | Variable |
| **CAT (Consolidated Audit Trail)** | Trade surveillance | 350M+ txns/year | High |

### 2.2 Market Size

| Segment | Size | Growth |
|---------|------|--------|
| Financial data & analytics | $45B+ | 8-10% CAGR |
| Alternative data market | $7B (2024) → $135B (2030) | 40%+ CAGR |
| RegTech (compliance) | $12B+ | 15%+ CAGR |
| Bloomberg Terminal revenue | $12B/year | Stable |

### 2.3 T+1 Settlement Reality (Live Since May 2024)

The SEC moved US markets to T+1 settlement on May 28, 2024. The operational pressure is now permanent:
- **Trade breaks must be resolved faster** - Less time to reconcile discrepancies
- **Trade Support teams are overwhelmed** - Manual FIX log analysis is too slow
- **Settlement risk is financial risk** - Failed settlements cost real money

**The Pain Point:** When a trade breaks, analysts must grep through FIX logs to find:
1. The original order (35=D NewOrderSingle)
2. Partial fills (35=8 ExecutionReport)
3. Final fill or rejection
4. Any amendments or cancels

This "trade lifecycle reconstruction" takes 30-45 minutes per break. With T+1, there's no time.

**Global Expansion (2025-2027):**
- **EU/UK:** Transitioning to T+1 by October 11, 2027
- **Switzerland:** Aligned with EU/UK timeline
- **Hong Kong:** Consultation underway for T+1 transition

**Ongoing Opportunity:** Tools that accelerate trade break resolution have immediate, quantifiable ROI - and the global market is expanding.

### 2.4 ISO 20022: Post-Migration Reality (November 2025 Complete)

SWIFT's ISO 20022 (MX format) mandate took effect November 22, 2025. The coexistence period has ended:
- **Legacy MT messages no longer accepted** on SWIFT FIN network
- **January 2026:** Additional charges for contingency/translation services
- **November 2026:** Unstructured address formats retired; only structured/hybrid allowed
- **2027-2028:** Full statement/reporting message adoption expected

**The "Translation Trap":** Banks that used MT-to-MX translators checked the compliance box but forfeited the strategic benefits of rich ISO 20022 data. They have MX messages but no tools to analyze them.

**Current Opportunity:** Organizations now have mandatory ISO 20022 data but lack tooling to analyze it. The November 2026 structured address requirement creates another compliance wave - banks need to audit their data quality.

---

## 3. Where Financial Data Lives (Domain Intelligence)

### 3.1 Trade Execution Logs (FIX)

FIX messages are everywhere in trading infrastructure:

```
FIX logs on trading desks:
/var/log/fix/
├── gateway_20260108.log       # FIX 4.4 messages, pipe-delimited
├── execution_20260108.log     # Order fills, timestamps
└── drop_copy_20260108.log     # Regulatory copies
```

**Characteristics:**
- Tag=value format: `8=FIX.4.4|9=148|35=D|49=SENDER|56=TARGET|...`
- High volume (millions of messages/day for active desks)
- Time-sensitive (latency matters for analysis)
- Custom tags (tag 5000+ for proprietary fields)

**Implications for Casparian:**
- Parser must handle custom tag dictionaries
- Streaming parsing for large log files
- Schema must accommodate venue-specific extensions

### 3.2 SEC EDGAR Filings

Public company financials in structured (XBRL) and unstructured (HTML/TXT) formats:

```
SEC EDGAR structure:
https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&CIK=0000320193
├── 10-K/                      # Annual reports
│   └── 0000320193-24-000081/  # Apple FY2024
│       ├── aapl-20240928.htm  # Human-readable
│       └── aapl-20240928_htm.xml  # XBRL data
├── 10-Q/                      # Quarterly reports
└── 8-K/                       # Current events
```

**Characteristics:**
- 150+ filing types, but 10-K/10-Q/8-K are 80% of value
- XBRL is XML with complex taxonomies
- Historical data back to 1993
- Free, public, well-documented API

**Implications for Casparian:**
- Ship with premade 10-K, 10-Q parsers
- Extract standardized financial statements (Income, Balance Sheet, Cash Flow)
- Handle taxonomy version differences across years

### 3.3 Alternative Data Sources

Hedge funds spend $15B+ annually on non-traditional data:

| Source Type | Example | Format | Access |
|-------------|---------|--------|--------|
| Satellite imagery | Retail parking lots, oil tanks | GeoTIFF, COG | Orbital Insight, SpaceKnow |
| Credit card data | Consumer spending panels | CSV, Parquet | Second Measure, Earnest |
| Web scraping | Pricing, job postings | JSON, CSV | Custom pipelines |
| Sentiment | Social media, news | JSON, JSONL | Twitter API, NewsAPI |
| Shipping/AIS | Vessel tracking | CSV, binary | MarineTraffic, VesselFinder |

**Implications for Casparian:**
- Focus on parsing outputs from data vendors (CSV, JSON)
- Don't compete on data collection (Scrapy, etc.)
- Add value by normalizing and joining disparate sources

### 3.4 Payment Messages (ISO 20022)

Post-November 2025, MX messages dominate:

```xml
<!-- pacs.008 - FI to FI Customer Credit Transfer -->
<FIToFICstmrCdtTrf>
  <GrpHdr>
    <MsgId>MSGID123</MsgId>
    <CreDtTm>2026-01-08T10:30:00Z</CreDtTm>
    <NbOfTxs>1</NbOfTxs>
  </GrpHdr>
  <CdtTrfTxInf>
    <PmtId><EndToEndId>E2E123</EndToEndId></PmtId>
    <IntrBkSttlmAmt Ccy="USD">50000.00</IntrBkSttlmAmt>
    <!-- Complex nested structures... -->
  </CdtTrfTxInf>
</FIToFICstmrCdtTrf>
```

**Characteristics:**
- Complex XML schemas (pacs, pain, camt families)
- Rich metadata (vs. legacy MT messages)
- Validation rules in XSD schemas
- Multiple message types for different use cases

**Implications for Casparian:**
- Use pyiso20022 library for parsing
- Focus on common message types: pacs.008, pacs.009, camt.053
- Extract to flat Parquet for SQL analysis

---

## 4. Target Personas

### 4.1 Primary: Trade Support Engineer (Tier 2 Support)

| Attribute | Description |
|-----------|-------------|
| **Role** | Trade Support Analyst, Middle Office, Operations |
| **Technical skill** | SQL, grep, basic scripting; NOT Python experts |
| **Pain** | Trade breaks take 30-45 minutes to debug; T+1 pressure |
| **Goal** | Resolve trade breaks before settlement deadline |
| **Buying power** | Operations budget; can approve tools that reduce risk |

**Current Workflow (painful):**
1. Receive alert: "Trade break on order 12345"
2. SSH into log server, grep for ClOrdID
3. Manually piece together order lifecycle across multiple log files
4. Copy-paste into Excel to see the timeline
5. 30-45 minutes later: "Ah, the counterparty sent a reject at 14:32:05"
6. Repeat 10-20 times per day

**Casparian Workflow:**
1. `casparian scan /var/log/fix --tag fix_logs`
2. `casparian process --tag fix_logs`
3. Query: `SELECT * FROM fix_order_lifecycle WHERE cl_ord_id = '12345' ORDER BY timestamp`
4. 5 minutes: Full trade lifecycle visible

**Value Proposition:** "Resolve trade breaks in 5 minutes instead of 45."

### 4.2 Secondary: Quantitative Analyst / Data Scientist

| Attribute | Description |
|-----------|-------------|
| **Role** | Quant Analyst, Data Scientist, Research Associate |
| **Technical skill** | Python, SQL, Pandas/Polars expert |
| **Pain** | Can't afford Bloomberg ($32K/yr); XBRL parsing is painful |
| **Goal** | Extract alpha signals from filings, alternative data |
| **Buying power** | Influences tool selection; small discretionary budget |

**Current Workflow (painful):**
1. Download SEC filings manually or via API
2. Parse XBRL with EdgarTools or sec-api (limited)
3. Wrangle into usable format (days of work)
4. Repeat for each filing type

**Casparian Workflow:**
1. `casparian scan ./filings --tag sec_10k`
2. `casparian process --tag sec_10k`
3. Query normalized financials in SQL/Parquet

### 4.3 Tertiary: Compliance / Surveillance Analyst

| Attribute | Description |
|-----------|-------------|
| **Role** | Compliance Officer, Trade Surveillance, AML Analyst |
| **Technical skill** | SQL, Excel; some Python |
| **Pain** | CAT reporting complexity; manual reconciliation |
| **Goal** | Automate surveillance reports; reduce manual review |
| **Buying power** | Budget for compliance tools; risk-averse buyer |

### 4.4 Tertiary: Trading Technology / Quant Dev

| Attribute | Description |
|-----------|-------------|
| **Role** | Quant Developer, Trading Systems Engineer |
| **Technical skill** | Python, C++, FIX protocol expert |
| **Pain** | Custom FIX parsing for each venue; maintenance burden |
| **Goal** | Standardize FIX log analysis across venues |
| **Buying power** | Technical decision maker; budget for tools |

---

## 5. Competitive Positioning

### 5.1 Bloomberg Terminal vs Casparian Flow

| Feature | Bloomberg Terminal | Casparian Flow |
|---------|-------------------|----------------|
| **Cost** | $32,000/year/seat | **$50-200/month** |
| **Data Access** | Comprehensive, real-time | **User brings data** |
| **Custom Parsing** | Limited | **Unlimited (Python)** |
| **Audit Trail** | None | **Full lineage** |
| **Deployment** | Cloud/Desktop | **Local-first** |
| **AI Integration** | Limited | **Full MCP** |

**Positioning:** "Bloomberg is the data. Casparian is the plumbing."

### 5.2 Where We Fight

**DO NOT** compete on Day 1:
- Real-time market data feeds
- Trading execution
- Portfolio management
- Pre-built dashboards

**DO** compete on:
- **Custom format parsing** - FIX, XBRL, ISO 20022
- **Alternative data normalization** - Join disparate sources
- **Compliance data pipelines** - CAT, trade reconciliation
- **Cost** - 10-100x cheaper than enterprise solutions

### 5.3 Python Ecosystem Comparison

| Tool | Strength | Weakness | Our Angle |
|------|----------|----------|-----------|
| **EdgarTools** | SEC filing access | No pipeline infrastructure | Add Scout + Sentinel |
| **sec-api** | API wrapper | Paid API, limited parsing | Free, local-first |
| **pyfixmsg** (Morgan Stanley) | FIX parsing | Library only | Complete solution |
| **simplefix** | Lightweight FIX | No analytics | Add SQL output |
| **pyiso20022** | ISO 20022 parsing | Complex API | Batteries included |
| **OpenBB** | Terminal alternative | Analysis, not ETL | Complementary |

---

## 6. Attack Strategies

### 6.1 Strategy A: "SEC Filings Democratization" (Open Data Play)

**Positioning:** "Bloomberg-quality financial data for Python developers."

**How it works:**
1. Ship 10-K, 10-Q, 8-K parsers with Casparian
2. User downloads filings from EDGAR (free)
3. Casparian extracts standardized financials to Parquet

**Value proposition:**
- "Why pay $32K/year for data that's free?"
- Normalized financial statements across 8,000+ companies
- Historical data back to 2009 (XBRL mandate)

**Why we win:**
- EdgarTools is library, not product
- sec-api charges per API call
- Casparian is local, free tier, complete

**Revenue model:**
- Free: 10 companies, basic financials
- Pro: Unlimited companies, full statement coverage
- Team: Shared datasets, audit trails

**Best for:** Quant researchers, indie traders, fintech startups

### 6.2 Strategy B: "The Trade Break Workbench" (Operations Play) ⭐ PRIMARY RECOMMENDED

**Positioning:** "Debug trade breaks in 5 minutes, not 45."

**How it works:**
1. Point Casparian at FIX log directory
2. Parser reconstructs **trade lifecycles** (New Order → Fills → Final State)
3. Query by ClOrdID, Symbol, or time range
4. See full order history in seconds

**Value proposition:**
- "T+1 means no time for manual grep. Casparian gives you instant trade forensics."
- Quantifiable ROI: 40 minutes saved per trade break × 10 breaks/day = 6+ hours/day
- Risk reduction: Faster resolution = lower settlement risk

**Why we win:**
- pyfixmsg is Morgan Stanley's library (testing focus, not operations)
- Enterprise TCA tools ($50-100K+) are for analytics, not break resolution
- No existing "Trade Break Workbench" product in the market

**Revenue model:**
- Pro: FIX parser + lifecycle reconstruction
- Team: Multi-venue normalization, alert integration
- Enterprise: Compliance audit trails, settlement risk dashboards

**Best for:** Broker-dealers, prop trading firms, hedge fund operations

### 6.3 Strategy C: "FIX Log Analytics" (Trading Tech Play)

**Positioning:** "Turn your FIX logs into insights."

**How it works:**
1. Point Casparian at FIX log directory
2. Parser handles venue-specific tag dictionaries
3. Query execution quality, latency, fill rates in SQL

**Value proposition:**
- "Every trading desk has FIX logs. Nobody queries them effectively."
- Execution quality analysis without expensive TCA tools
- Custom tag support for proprietary extensions

**Why we win:**
- pyfixmsg is Morgan Stanley's library (testing focus, not analytics)
- Enterprise TCA tools cost $50-100K+
- Casparian bridges the gap

**Revenue model:**
- Pro: FIX parser + custom tag support
- Team: Multi-venue normalization
- Enterprise: Compliance audit trails

**Best for:** Quant teams, trading technology groups

### 6.4 Strategy D: "ISO 20022 Analytics" (Payments Play)

**Positioning:** "Analyze your SWIFT MX messages."

**How it works:**
1. Export MX messages from payment system
2. Casparian parses pacs/pain/camt families
3. Query payment flows, exceptions, timing in SQL

**Value proposition:**
- "November 2025 gave you ISO 20022. Now analyze it."
- Extract nested structures to flat tables
- Compliance-ready audit trails

**Why we win:**
- pyiso20022 is complex (xsdata dependency)
- Banks have MX data but no easy analysis tools
- Casparian simplifies the developer experience

**Revenue model:**
- Team: ISO 20022 parser suite
- Enterprise: Multi-format (MX + legacy MT)

**Best for:** Banks, payment processors, treasury departments

---

## 7. Premade Parsers

### 7.1 FIX Protocol Parser (`fix_parser.py`)

**Input:** FIX 4.2/4.4/5.0 log files

**Output Tables:**

| Table | Description |
|-------|-------------|
| `fix_messages` | All messages with header fields |
| `fix_orders` | Order details (NewOrderSingle, etc.) |
| `fix_executions` | Execution reports, fills |
| `fix_market_data` | Market data snapshots/updates |
| `fix_order_lifecycle` | **Trade Break Workbench table** - reconstructed order lifecycle |

**`fix_order_lifecycle` Schema (Trade Break Workbench):**

| Column | Description |
|--------|-------------|
| `cl_ord_id` | Client order ID (primary key for trade lookup) |
| `symbol` | Instrument symbol |
| `side` | Buy/Sell |
| `order_qty` | Original order quantity |
| `cum_qty` | Cumulative filled quantity |
| `leaves_qty` | Remaining quantity |
| `avg_px` | Average fill price |
| `order_status` | Final status (Filled, PartiallyFilled, Canceled, Rejected) |
| `first_seen` | First message timestamp |
| `last_update` | Last message timestamp |
| `lifecycle_duration_ms` | Time from first to last message |
| `message_count` | Number of messages in lifecycle |
| `venue` | Execution venue (from TargetCompID) |
| `reject_reason` | If rejected, the reason text |
| `lifecycle_json` | Full message history as JSON array |

**Use Case:** Query a single table to see the complete lifecycle of any order:
```sql
SELECT * FROM fix_order_lifecycle
WHERE cl_ord_id = 'ORD12345'
ORDER BY last_update DESC;
```

**Key Fields:**
- `msg_type` (35): Message type code
- `sender_comp_id` (49), `target_comp_id` (56)
- `sending_time` (52): Timestamp
- `order_id` (37), `cl_ord_id` (11): Order identifiers
- `symbol` (55), `side` (54), `qty` (38), `price` (44)

**Library:** pyfixmsg or simplefix (user choice)

### 7.2 SEC EDGAR XBRL Parser (`edgar_xbrl.py`)

**Input:** 10-K, 10-Q XBRL filings

**Output Tables:**

| Table | Description |
|-------|-------------|
| `edgar_filings` | Filing metadata (CIK, date, type) |
| `edgar_income_statement` | Revenue, expenses, net income |
| `edgar_balance_sheet` | Assets, liabilities, equity |
| `edgar_cash_flow` | Operating, investing, financing |
| `edgar_facts` | Raw XBRL facts for custom analysis |

**Library:** EdgarTools (open source, well-maintained)

### 7.3 ISO 20022 Parser (`iso20022_parser.py`)

**Input:** MX messages (pacs, pain, camt families)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `iso_payments` | Payment instructions (pacs.008) |
| `iso_returns` | Payment returns (pacs.004) |
| `iso_statements` | Account statements (camt.053) |
| `iso_status` | Status reports (pacs.002) |

**Library:** pyiso20022

---

## 8. Go-to-Market

### 8.1 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **Quant communities** | QuantConnect, Quantopian alumni, r/algotrading | Month 1-3 |
| **Fintech developers** | Dev.to, Medium, GitHub | Month 1-6 |
| **Compliance conferences** | SIFMA, FIA, RegTech events | Month 6-12 |
| **Trading tech firms** | Direct outreach to hedge funds | Month 3-9 |

### 8.2 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Parse SEC 10-K in 5 minutes" video | Top-of-funnel | High |
| "FIX log analysis tutorial" | Developer education | High |
| "ISO 20022 for analysts" guide | Post-migration audience | Medium |
| "Bloomberg alternatives" blog | SEO, cost-conscious buyers | Medium |

### 8.3 Pricing (Finance Vertical) - Value-Based

> **Pricing Philosophy:** Price by the value created, not by cost. See [STRATEGY.md](../STRATEGY.md#value-based-pricing-strategy) for framework.

#### Value Analysis

| Role | Fully Loaded Cost | Time Saved | Value Created |
|------|-------------------|------------|---------------|
| Trade Support Engineer | $150,000/year | 6+ hrs/day | **$50-100K/year** |
| Quant Analyst | $200,000/year | 10+ hrs/week | **$40-80K/year** |
| Compliance Officer | $175,000/year | 5+ hrs/week | **$20-40K/year** |

**Additional value:** Settlement risk reduction (T+1 pressure), audit trail for compliance, knowledge retention when staff leaves.

#### Pricing Tiers (Capturing 10-15% of Value)

| Tier | Price | Value Capture | Features | Target |
|------|-------|---------------|----------|--------|
| **Starter** | Free | N/A | EDGAR parser, 10 companies | Students, evaluation |
| **Professional** | $500/user/month | ~5% | FIX parser, unlimited companies, email support | Individual analysts |
| **Trading Desk** | $15,000/desk/year | ~10-15% | Unlimited users per desk, multi-venue FIX, custom tags, priority support | Operations teams |
| **Enterprise** | $50,000+/year | Custom | Multi-desk, ISO 20022, audit trails, SSO, dedicated success manager | Banks, compliance |

#### Pricing Justification

**Trading Desk tier ($15,000/year):**
- Current workflow: 40 min/break × 10 breaks/day = 6.7 hours/day wasted
- At $75/hour (Trade Support Engineer rate): **$125K/year in labor**
- $15K captures 12% of quantifiable labor savings alone
- Settlement risk reduction adds unquantifiable additional value

**Comparison to alternatives:**
- Manual grep + Excel: "Free" but costs $125K+ in labor
- Enterprise TCA tools: $50-100K+ but wrong use case (analytics, not break resolution)
- Bloomberg Terminal: $32K/seat but for data access, not parsing
- **Casparian at $15K: 10-50% of alternatives, purpose-built**

#### Why Not Price Lower?

Per Andreessen's framework:
1. **$400/month doesn't prove the moat** - Too cheap to test if product is must-have
2. **$400/month doesn't fund sales** - Need enterprise sales team for trading desks
3. **$400/month signals "not enterprise-grade"** - Operations managers skeptical of cheap tools
4. **$400/month can't fund customer success** - Trading desks need white-glove onboarding

#### Revenue Projection (Finance Vertical)

| Metric | 6-Month | 12-Month | 24-Month |
|--------|---------|----------|----------|
| Trading Desk customers | 10 | 30 | 75 |
| Avg contract value | $15,000 | $15,000 | $18,000 |
| Finance MRR | $12,500 | $37,500 | $112,500 |
| Finance ARR | $150,000 | $450,000 | $1,350,000 |

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| EDGAR parser users | 500 | 2,500 |
| FIX parser users | 100 | 500 |
| Files processed (finance) | 500K | 5M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Finance vertical MRR | $10K | $50K |
| Finance customers | 50 | 250 |
| Enterprise deals | 2 | 10 |

### 9.3 Competitive Metrics

| Metric | Target |
|--------|--------|
| "XBRL parser Python" search ranking | Top 5 |
| "FIX log analysis" search ranking | Top 10 |
| OpenBB integration | Partnership in Year 1 |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| SEC changes XBRL format | Low | EdgarTools community maintains |
| FIX 5.0 adoption changes landscape | Low | Support multiple versions |
| Bloomberg launches competing analytics | Medium | Cost advantage; local-first |
| Compliance sales cycle too long | High | Bottom-up quant adoption |
| Alternative data vendors don't cooperate | Medium | Focus on user-provided data |

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Primary attack vector** | **Trade Break Workbench (B)** | Operations budget approval faster than quant; T+1 urgency; quantifiable ROI |
| Secondary attack vector | SEC Filings (A) | Good PLG funnel; free data source; developer audience |
| Positioning | "Liability Shield" + "Risk Reducer" | Appeals to operations/compliance budget, not just tech budget |
| Primary persona | Trade Support Engineer | SQL-capable, urgent pain, budget authority, shorter sales cycle |
| Parser library (FIX) | pyfixmsg + simplefix | Morgan Stanley backing; lightweight option |
| Parser library (XBRL) | EdgarTools | Best maintained; active community |
| Parser library (ISO 20022) | pyiso20022 | Python-native; good documentation |
| Real-time data | Out of scope | Focus on analytics, not feeds |
| ISO 20022 priority | Phase 2 (after Trade Break traction) | Market still digesting migration; tools not mature |

---

## 12. References

- [pyfixmsg (Morgan Stanley)](https://github.com/morganstanley/pyfixmsg)
- [simplefix](https://github.com/da4089/simplefix)
- [EdgarTools](https://edgartools.readthedocs.io/)
- [pyiso20022](https://github.com/phoughton/pyiso20022)
- [SEC EDGAR](https://www.sec.gov/edgar)
- [ISO 20022 Migration](https://www.swift.com/standards/iso-20022)
- [OpenBB](https://openbb.co/)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft |
| 2026-01-08 | 0.2 | Gap analysis integration: Trade Break Workbench as primary attack; T+1 urgency section; Trade Support Engineer persona; fix_order_lifecycle table; updated positioning |
| 2026-01-14 | 0.3 | Maintenance workflow: Updated T+1 to reflect live status (May 2024); added global T+1 expansion (EU/UK Oct 2027); updated ISO 20022 to post-migration reality with Nov 2026 structured address deadline |

# Healthcare HL7 Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → Healthcare IT)
**Related Spec:** [specs/hl7_parser.md](../specs/hl7_parser.md)
**Version:** 0.3
**Date:** January 14, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the healthcare data **analytics** market by filling a gap that integration engines like Mirth Connect don't address: **querying historical HL7 archives**.

**Core Insight:** Mirth Connect routes live messages. Casparian queries archived messages. These are **complementary, not competitive** products.

```
Live HL7 traffic → [MIRTH CONNECT] → Routes to destinations (EHR, Lab, etc.)
                         ↓
                  Archives to disk
                         ↓
                   [CASPARIAN] → SQL/Parquet for analytics
```

**Primary Attack Vector:** "Archive Analytics" - give analysts SQL access to historical HL7 data without waiting for the Interface Team to build custom reports.

**What We Are NOT:** A Mirth Connect alternative. We don't do real-time routing, ACK handling, or message forwarding. That's Mirth's job.

---

## 2. Market Overview

### 2.1 HL7 v2.x Prevalence

- **~95% of US hospitals** use HL7 v2.x for internal data exchange
- **ADT messages** (Admit/Discharge/Transfer) are the highest volume
- **ORU messages** (Observation Results) carry lab/clinical data
- HL7 v2.x will remain dominant for **10+ years** due to legacy system inertia

### 2.2 Market Size

| Segment | Size | Growth |
|---------|------|--------|
| Healthcare IT spending (US) | $200B+ annually | 8-10% CAGR |
| Interface engine market | $500M+ | Consolidating |
| Healthcare analytics | $50B+ | 15%+ CAGR |

### 2.3 The Mirth Connect Licensing Change (March 2025 - Complete)

NextGen Healthcare transitioned Mirth Connect from open-source to **commercial-only** on March 19, 2025:
- **Version 4.6+** requires paid license (source code no longer public)
- **Version 4.5.2** is the last open-source release (no security patches)
- **New Enterprise features:** SSL Manager, Channel History, Mirth Command Center (not available in EU/UK)
- **Community response:** Forks emerged (Open Integration Engine, BridgeLink) offering open-source alternatives

**Market Impact:**
- Organizations face upgrade decision: pay for 4.6+ or stay on unsupported 4.5.2
- Healthcare IT budgets now allocate Mirth licensing costs
- Some orgs exploring fork migrations (creates integration uncertainty)

**Implication for Casparian:** Organizations now paying for Mirth will want to extract more value from their HL7 data. Casparian enables **analytics** on the archives Mirth creates - complementing their Mirth investment rather than replacing it. For orgs on forks, Casparian remains compatible (same archive format).

---

## 3. Where HL7 Data Lives (Domain Intelligence)

Understanding the "habitat" of HL7 data informs our product design:

### 3.1 The "Network Share" Graveyard (90% of cases)

Historical HL7 archives sit on massive NAS devices (SMB/NFS), mapped as network drives.

```
\\hospital-nas-01\interface_archives\
├── ADT_Inbound\
│   ├── 2024\
│   │   ├── 01\
│   │   │   ├── 20240101_00.hl7  (Midnight to 1AM)
│   │   │   ├── 20240101_01.hl7
│   │   │   └── ...
```

**Implications for Casparian:**
- Scanner must handle network latency (polling, not inotify)
- Parser must tolerate IO timeouts (SMB flakiness)
- Millions of small files = stress test for file discovery

### 3.2 The "Shadow IT" Data Dump

Researchers can't get database access, so IT dumps zip files of HL7 messages to secure folders.

```
\\research-share\Dr_Smith_Projects\
├── data_dump_final_v2.zip
├── NEW_DATA_DO_NOT_DELETE\
│   ├── export_1.txt
│   └── export_2.txt
```

**Implications for Casparian:**
- Zero-ceremony parsing (chaotic naming conventions)
- Works on analyst's laptop (no enterprise deployment)
- Handles zip files, mixed formats

### 3.3 The "Modern" Data Lake (Azure Blob / S3)

Forward-thinking health systems move archives to cloud object storage.

```
s3://hospital-data-lake/raw/hl7/adt/year=2024/month=01/day=08/data.json
```

**Implications for Casparian:**
- Future: Scout sources for `az://` and `s3://` URIs
- Not Day 1 priority (network shares are more common)

### 3.4 Why NOT SharePoint

SharePoint is document-centric and unsuitable for HL7:
- **Throughput:** APIs throttle at thousands of files/second
- **File Extensions:** SharePoint doesn't understand `.hl7` files
- **HIPAA:** Access controls harder to manage than dedicated storage

---

## 4. Target Personas

### 4.1 Primary: Healthcare Data Analyst

| Attribute | Description |
|-----------|-------------|
| **Role** | Data Analyst, Clinical Informaticist, Research Coordinator |
| **Technical skill** | SQL, maybe Python, NOT an HL7 expert |
| **Pain** | 6-month wait for Interface Team; regex parsing in Python |
| **Goal** | Query HL7 data in SQL without learning HL7 internals |
| **Buying power** | Influences, doesn't decide |

**Current Workflow (painful):**
1. Request HL7 export from IT
2. Receive files (thousands of `.hl7` messages)
3. Google "how to parse HL7" → find python-hl7
4. Write custom scripts, handle edge cases
5. Load into database/Excel
6. Repeat for every project

**Casparian Workflow:**
1. Mount network drive
2. `casparian scan Z:/archives --tag hl7_adt`
3. `casparian process --tag hl7_adt`
4. Query in SQL

### 4.2 Secondary: Interface Engineer

| Attribute | Description |
|-----------|-------------|
| **Role** | Integration Engineer, HL7 Analyst |
| **Technical skill** | Mirth/Rhapsody expert, JavaScript, some Python |
| **Pain** | Mirth is now expensive; no visibility into data quality |
| **Goal** | Monitor data flows, debug issues faster |
| **Buying power** | Recommends tools to IT leadership |

### 4.3 Tertiary: IT Leadership (Buyer)

| Attribute | Description |
|-----------|-------------|
| **Role** | CIO, IT Director, CISO |
| **Pain** | Mirth licensing costs; compliance requirements |
| **Goal** | Reduce costs, maintain compliance |
| **Buying power** | Decision maker |

---

## 5. Competitive Positioning

### 5.1 Mirth Connect + Casparian Flow (Complementary Stack)

Mirth and Casparian solve **different problems**:

| Aspect | Mirth Connect | Casparian Flow |
|--------|---------------|----------------|
| **Problem** | "Route this ADT to Epic" | "What ADT patterns exist in our archives?" |
| **Data State** | Transient (in-flight) | Persistent (archived) |
| **Time Domain** | Real-time (ms latency) | Batch (historical analysis) |
| **User** | Interface Engineer (Ops) | Data Analyst / Researcher |
| **Output** | Message forwarded to EHR | Parquet/SQL for analytics |

**The Stack:**
```
Sending System → [MIRTH] → Receiving System (Epic, Lab, etc.)
                    ↓
              Archive folder
                    ↓
              [CASPARIAN] → Data Lake / Analytics
```

### 5.2 Where We Add Value (Not Where We Fight)

**Mirth's job (we don't touch this):**
- TCP listeners (MLLP routing)
- ACK handling
- Message retries
- Real-time routing

**Casparian's job (downstream of Mirth):**
- **Archive analytics** - Query 5 years of ADT history
- **Data quality audits** - Find invalid DOBs across all historical messages
- **Research datasets** - Turn HL7 archives into SQL-queryable tables
- **Compliance reporting** - Audit who sent what, when

### 5.3 Other Competitors

| Competitor | Strength | Weakness | Our Angle |
|------------|----------|----------|-----------|
| **Rhapsody** | Enterprise features | Expensive ($100K+) | 10x cheaper |
| **Qvera QIE** | User-friendly | Still enterprise-priced | Free tier |
| **HAPI (Java)** | Open source | Library, not product | Complete solution |
| **python-hl7** | Free | DIY, no infrastructure | Batteries included |

---

## 6. Attack Strategies

### 6.1 Strategy A: "The Sidecar" (Observability Play)

**Positioning:** "Don't replace Mirth. Audit it."

**How it works:**
1. Configure Mirth to archive messages to network share
2. Casparian watches that folder
3. Instant queryable data lake (Parquet)

**Value proposition:**
- "Mirth moves your data. Casparian observes it."
- Alerting: "Mirth won't tell you Dr. Smith sends invalid DOBs. Casparian will."
- Non-threatening to Interface Team (complementary, not competitive)

**Revenue model:**
- Sell "Observability Dashboard" on top of free parser
- Enterprise tier: alerting, anomaly detection

**Best for:** Organizations with existing Mirth investment

### 6.2 Strategy B: "Analyst Liberation" (Shadow IT Play) ⭐ RECOMMENDED

**Positioning:** "Bypass the Interface Team."

**How it works:**
1. Analyst gets zip file of HL7 messages from IT
2. Drag and drop into Casparian
3. 5 minutes later → SQL-queryable tables

**Value proposition:**
- "Don't wait 6 months for an interface. Parse it yourself."
- Works on analyst's laptop (no enterprise deployment)
- Zero HL7 knowledge required

**Why we win:**
- Mirth is too heavy for laptops
- Casparian (Rust binary) runs anywhere
- Once analysts use CF, they refuse to go back

**Revenue model:**
- Free tier captures analysts
- Pro tier for teams
- Enterprise when IT notices adoption

**Best for:** Initial market entry; bottom-up adoption

### 6.3 Strategy C: "GitOps Attack" (Developer Play)

**Positioning:** "HL7 integration for software engineers."

**How it works:**
- Parsers are Python files → Git-friendly
- Branch, PR, pytest, CI/CD
- Infrastructure-as-Code (not Click-Ops)

**Value proposition:**
- "Treat your hospital data pipeline like code."
- Version control that actually works (vs. Mirth's XML blob)
- Unit tests for healthcare integrations

**Target:** Modern health tech startups (hate Mirth's GUI)

**Revenue model:**
- Team tier with collaboration features
- Enterprise tier with compliance features

**Best for:** Greenfield projects; health tech startups

---

## 7. Product Roadmap Implications

### Phase 1: Offline Parser (Current)

**Focus:** Files on disk, batch processing

**Features:**
- `hl7_adt.py`, `hl7_oru.py` parsers (ships with Casparian)
- Parquet/SQL output
- TUI for testing and inspection

**Strategic purpose:** Win the analytics use case. Mirth can't touch us here.

### Phase 2: "Fake" Listener (Future)

**Focus:** Simple MLLP (TCP) listener

**Features:**
- Accept message → Send ACK → Write to disk
- Standard Casparian scanner picks up files
- No routing logic (that's Mirth's job)

**Strategic purpose:** Replace Mirth for "inbound" feeds without full interface engine complexity.

**Technical approach:**
```rust
// Simple MLLP listener - accept, ACK, persist
async fn handle_mllp_connection(stream: TcpStream, output_dir: PathBuf) {
    // 1. Read MLLP frame
    // 2. Parse MSH to get message ID
    // 3. Write to {output_dir}/{timestamp}_{msg_id}.hl7
    // 4. Send ACK
}
```

### Phase 3: Reverse ETL (Future)

**Focus:** Generate HL7 messages from SQL/Parquet

**Features:**
- Read structured data → Generate HL7 v2.x messages
- Send via MLLP to downstream systems

**Strategic purpose:** Full bi-directional engine, but built on modern data engineering principles (Python/Rust) rather than legacy Java.

---

## 8. Go-to-Market

### 8.1 Channels

| Channel | Approach |
|---------|----------|
| **Direct to analysts** | Content marketing, "how to parse HL7" SEO |
| **Health IT communities** | HIMSS, HL7 forums, Reddit r/healthIT |
| **Healthcare MSPs** | Partner program (see STRATEGY.md) |
| **Interface teams** | "Add analytics to your Mirth archives" positioning |

### 8.2 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Parse HL7 in 5 minutes" video | Top-of-funnel | High |
| "Query your HL7 archives with SQL" blog | SEO, analytics use case | High |
| "HL7 ADT explained for analysts" | Education, trust | Medium |
| "Add analytics to your Mirth archives" | Complementary positioning | Medium |
| Case study: Hospital X | Social proof | Medium |

### 8.3 Pricing (Healthcare Vertical) - Value-Based

> **Pricing Philosophy:** Price by the value created, not by cost. See [STRATEGY.md](../STRATEGY.md#value-based-pricing-strategy) for framework.

#### Value Analysis

| Pain Point | Current Cost | Casparian Value |
|------------|--------------|-----------------|
| Interface Team wait time | 6+ months opportunity cost | Days instead |
| Custom integration project | $50,000-150,000 | Self-service |
| Interface analyst salary | $80,000-120,000/year | Time savings |
| Mirth Connect license (post-March 2025) | $20,000-50,000/year | Complementary analytics |
| Compliance audit prep | $20,000-50,000 | Automated reports |
| Research data extraction | $10,000-30,000/project | Self-service |

**Additional value:** HIPAA compliance (data stays local), audit trails, researcher enablement, IT backlog reduction.

#### Pricing Tiers (Capturing 10-15% of Value)

| Tier | Price | Value Capture | Features | Target |
|------|-------|---------------|----------|--------|
| **Community** | Free | N/A | HL7 ADT parser, 1,000 messages/day | Individual analysts, evaluation |
| **Clinic** | $250/month | ~3% | All HL7 parsers, 10K messages/day, email support | Small clinics, research groups |
| **Hospital** | $25,000/year | ~10% | Unlimited volume, HIPAA BAA, audit logs, schema governance, priority support | Single facility |
| **Health System** | $100,000+/year | Custom | Multi-facility, SSO, on-prem deployment, SLA, dedicated team, custom parser development | Health systems |

#### Pricing Justification

**Hospital tier ($25,000/year):**
- Interface project cost: $50,000-150,000 and 6+ months
- Casparian enables analysts to bypass Interface Team backlog
- Compliance audit prep savings: $20,000-50,000
- $25K captures 10-25% of first-year value
- Plus ongoing savings on every subsequent project

**Comparison to alternatives:**
- Rhapsody: $100K+/year (overkill for analytics)
- Mirth Connect (post-2025): $20K-50K/year (routing, not analytics)
- Custom Python development: $100K+ one-time, fragile
- **Casparian at $25K/year: Analytics layer that complements Mirth**

#### Why Not Price Lower?

Per Andreessen's framework:
1. **$300/month ($3,600/year) signals "not HIPAA-ready"** - Healthcare buyers expect enterprise pricing
2. **$300/month can't fund BAA support** - HIPAA compliance requires legal and operational investment
3. **$300/month can't fund healthcare sales** - HIMSS presence, compliance certifications, long sales cycles
4. **$300/month triggers "shadow IT" concerns** - IT leadership worried about unauthorized tools

**Note:** Healthcare vertical has longer sales cycles. Budget for 12-18 months to close Hospital/Health System deals.

#### Revenue Projection (Healthcare Vertical)

| Metric | Year 1 | Year 2 | Year 3 |
|--------|--------|--------|--------|
| Community (free) users | 500 | 2,000 | 5,000 |
| Clinic customers | 30 | 100 | 250 |
| Hospital customers | 5 | 20 | 50 |
| Health System customers | 0 | 2 | 8 |
| **Healthcare ARR** | **$215,000** | **$830,000** | **$2,350,000** |

Note: Year 1 assumes Phase 4 timing (starts Month 12+). Revenue projections begin when healthcare vertical is actively pursued.

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| HL7 parser downloads | 1,000 | 5,000 |
| WAU (HL7 users) | 100 | 500 |
| Files processed (HL7) | 1M | 10M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Healthcare MRR | $5K | $25K |
| Healthcare customers | 20 | 100 |
| Enterprise deals (healthcare) | 1 | 5 |

### 9.3 Market Position Metrics

| Metric | Target |
|--------|--------|
| "HL7 analytics" search ranking | Top 5 |
| "Parse HL7 with SQL" search ranking | Top 3 |
| Mirth-complementary deployments | 50 in Year 1 |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Mirth releases competitive analytics | Medium | Move fast; establish market position |
| Healthcare sales cycle too long | High | Bottom-up analyst adoption; MSP channel |
| HIPAA concerns about AI | Medium | Local-first; no PHI leaves machine |
| HL7 complexity underestimated | Medium | Start with ADT/ORU; expand based on demand |

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Initial attack vector | Analyst Liberation (B) | Fastest path to users; bottom-up |
| Day 1 scope | Batch processing only | Don't compete with Mirth on routing |
| Parser distribution | Ships with Casparian | Zero friction for healthcare users |
| MLLP listener | Phase 2 | Prove value with files first |
| Reverse ETL | Phase 3 | Requires established user base |

---

## 12. References

- [HL7 Parser Technical Spec](../specs/hl7_parser.md)
- [Mirth Connect Pricing](https://www.nextgen.com/products-and-services/mirth-connect)
- [Healthcare IT Market Research](https://www.grandviewresearch.com/industry-analysis/healthcare-it-market)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft |
| 2026-01-08 | 0.2 | **Positioning fix:** Clarified Casparian is complementary to Mirth (archive analytics), not a replacement; Reframed competitive positioning as "Mirth + Casparian Stack"; Removed "Mirth alternative" language throughout |
| 2026-01-14 | 0.3 | Maintenance workflow: Updated Mirth licensing section to reflect March 2025 change is complete; added community forks (OIE, BridgeLink); noted Mirth Command Center EU/UK unavailability |


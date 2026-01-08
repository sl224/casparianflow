# Manufacturing Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → Manufacturing)
**Version:** 0.1
**Date:** January 8, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the manufacturing data analytics market by positioning against expensive historian systems (OSIsoft PI, AVEVA) and complex industrial integration platforms.

**Core Insight:** Manufacturing data is trapped in proprietary historians, SCADA systems, and equipment logs. Plant engineers with Python skills can't easily build analytics without $100K+ historian licenses or complex OPC-UA integrations.

**Primary Attack Vector:** "Plant Engineer Liberation" - give engineers direct access to equipment data without going through IT/OT gatekeepers or purchasing enterprise historian licenses.

---

## 2. Market Overview

### 2.1 Key Format Landscapes

| Format | Domain | Prevalence | Complexity |
|--------|--------|------------|------------|
| **OPC-UA** | Universal industrial connectivity | Standard for new equipment | Medium |
| **MTConnect** | CNC machine tools | ~30% of CNC machines | Low-Medium |
| **SCADA exports** | Control system data | Every plant | Variable |
| **Historian exports** | Time-series archives | PI, AVEVA customers | Low (CSV/Parquet) |
| **SPC/Quality data** | Statistical process control | Quality departments | Low |
| **MES exports** | Manufacturing execution | Enterprise plants | Medium |

### 2.2 Market Size

| Segment | Size | Growth |
|---------|------|--------|
| Industrial data management | $18B+ (2026) | 12%+ CAGR |
| OSIsoft PI / AVEVA historian | $2B+ installed base | Consolidating |
| Manufacturing analytics | $15B+ | 15%+ CAGR |
| OPC-UA market | $18B+ (projected 2026) | 8%+ CAGR |

### 2.3 The AVEVA/OSIsoft Disruption

AVEVA's acquisition of OSIsoft created market turbulence:
- PI ProcessBook end-of-life: December 2024
- License consolidation: Many customers reviewing alternatives
- Cloud push: Conflicts with air-gapped OT environments
- Cost increases: License audits and renewals driving alternatives

**Window of Opportunity:** Organizations evaluating historian alternatives need modern analytics without vendor lock-in. Many have data in CSV/Parquet exports they can't effectively analyze.

---

## 3. Where Manufacturing Data Lives (Domain Intelligence)

### 3.1 Historian Exports (Primary Target)

Plants export time-series data from historians for analysis:

```
Plant data exports:
\\plant-share\engineering\data_exports\
├── line_1_temperatures_202601.csv     # 1-second intervals
├── quality_spc_weekly.xlsx            # SPC charts data
├── oee_report_202601.parquet          # OEE calculations
└── downtime_log_202601.csv            # Manual entries
```

**Characteristics:**
- High-frequency time-series (sub-second for some sensors)
- Tag-based naming (cryptic: `LI-1234.PV`, `TT-5678.SP`)
- Multiple export formats (CSV, Parquet, Excel)
- Historical archives can be TB-scale

**Implications for Casparian:**
- Parser must handle tag name mapping (human-readable aliases)
- Streaming for large time-series files
- Schema needs timestamp + tag + value + quality flag

### 3.2 SCADA/HMI Exports

Operators export data from SCADA systems:

```
SCADA exports:
/var/scada_exports/
├── alarms_20260108.csv                # Alarm history
├── trends_reactor_1.csv               # Trend data
├── batch_report_12345.xml             # Batch records
└── audit_trail_202601.csv             # Operator actions
```

**Characteristics:**
- Alarm data is event-based (timestamp, tag, state, priority)
- Trend data is time-series (sampled at various rates)
- Batch records are hierarchical (phases, steps, parameters)
- Audit trails required for FDA/compliance

**Implications for Casparian:**
- Multiple parser types: alarms, trends, batches
- Handle vendor-specific formats (Wonderware, Ignition, FactoryTalk)
- Compliance-friendly audit trail output

### 3.3 CNC/Machine Tool Data (MTConnect)

MTConnect provides standardized CNC data:

```xml
<!-- MTConnect Streams response -->
<MTConnectStreams>
  <Streams>
    <DeviceStream name="Haas-VF2">
      <ComponentStream component="Controller">
        <Samples>
          <PathPosition timestamp="2026-01-08T10:30:00Z" dataItemId="Xact">123.456</PathPosition>
          <PathFeedrate timestamp="2026-01-08T10:30:00Z" dataItemId="Frt">500.0</PathFeedrate>
        </Samples>
        <Events>
          <Execution timestamp="2026-01-08T10:30:00Z" dataItemId="exec">ACTIVE</Execution>
          <ControllerMode timestamp="2026-01-08T10:30:00Z" dataItemId="mode">AUTOMATIC</ControllerMode>
        </Events>
      </ComponentStream>
    </DeviceStream>
  </Streams>
</MTConnectStreams>
```

**Characteristics:**
- XML-based, HTTP REST API
- Standardized data model (no proprietary tags)
- Event-driven (conditions, events) + sampled (samples)
- Agent-based architecture (adapter → agent → client)

**Implications for Casparian:**
- XML parser with MTConnect schema awareness
- Handle both archived XML and live polling
- Machine-agnostic output (normalize across vendors)

### 3.4 SPC/Quality Data

Quality departments track statistical process control:

```
Quality data:
\\quality-share\spc_data\
├── cmm_measurements_20260108.csv      # CMM inspection results
├── control_charts_line_1.xlsx         # X-bar, R charts
├── capability_studies/
│   ├── cpk_study_12345.csv            # Process capability
│   └── gage_rr_study.xlsx             # Gage R&R
└── defect_log.csv                     # Defect tracking
```

**Characteristics:**
- Measurement data (actual vs. nominal vs. tolerance)
- Statistical summaries (Cp, Cpk, Pp, Ppk)
- Traceability (part number, lot, operator, timestamp)
- Often Excel-based (engineers love Excel)

**Implications for Casparian:**
- Excel parser with multi-sheet support
- SPC-specific output tables (measurements, limits, stats)
- Integration with quality standards (AQDEF, QIF)

---

## 4. Target Personas

### 4.1 Primary: Plant/Process Engineer

| Attribute | Description |
|-----------|-------------|
| **Role** | Process Engineer, Controls Engineer, Reliability Engineer |
| **Technical skill** | Python basics, Excel expert, some SQL |
| **Pain** | Historian queries are slow; can't get data without IT help |
| **Goal** | Analyze process data to improve yield, reduce downtime |
| **Buying power** | Small discretionary budget; influences larger purchases |

**Current Workflow (painful):**
1. Request data export from IT/OT team
2. Wait days for historian query to complete
3. Receive massive CSV, crash Excel trying to open it
4. Write Python script to parse, fight encoding issues
5. Repeat for every analysis project

**Casparian Workflow:**
1. IT exports data to shared folder (one-time setup)
2. `casparian scan \\plant-share\exports --tag historian_data`
3. `casparian process --tag historian_data`
4. Query in SQL, visualize in existing tools

### 4.2 Secondary: Data Scientist / Analytics Team

| Attribute | Description |
|-----------|-------------|
| **Role** | Manufacturing Data Scientist, Analytics Engineer |
| **Technical skill** | Python expert, ML/AI experience |
| **Pain** | Data wrangling consumes 80% of time; model deployment is hard |
| **Goal** | Build predictive models for quality, maintenance |
| **Buying power** | Budget for tools; influences platform decisions |

### 4.3 Tertiary: IT/OT Manager (Buyer)

| Attribute | Description |
|-----------|-------------|
| **Role** | IT Manager, OT Manager, Plant IT Director |
| **Technical skill** | Infrastructure focus, limited coding |
| **Pain** | Historian licenses are expensive; security concerns |
| **Goal** | Enable analytics without compromising OT security |
| **Buying power** | Decision maker for plant software |

---

## 5. Competitive Positioning

### 5.1 OSIsoft PI / AVEVA vs Casparian Flow

| Feature | OSIsoft PI / AVEVA | Casparian Flow |
|---------|-------------------|----------------|
| **Cost** | $100K+ (perpetual) + maintenance | **$200-500/month** |
| **Data Collection** | Direct OPC-UA/DA connection | **Works with exports** |
| **Deployment** | Complex server infrastructure | **Single binary, local** |
| **Query Language** | PI AF SQL, proprietary | **Standard SQL** |
| **AI Integration** | Limited, add-on | **Full MCP** |
| **Air-Gap Support** | Yes | **Yes** |

**Positioning:** "PI collects. Casparian analyzes."

### 5.2 Where We Fight

**DO NOT** compete on Day 1:
- Real-time data collection from PLCs
- Historian database (time-series storage)
- SCADA/HMI visualization
- Control system integration

**DO** compete on:
- **Export analysis** - Turn historian CSV dumps into insights
- **Cross-system joins** - Combine quality + process + downtime
- **AI-assisted analytics** - Anomaly detection without data science team
- **Cost** - 10-100x cheaper than historian analytics add-ons

### 5.3 Other Competitors

| Competitor | Strength | Weakness | Our Angle |
|------------|----------|----------|-----------|
| **Seeq** | Advanced analytics | Expensive ($50K+/yr) | 10x cheaper |
| **TrendMiner** | Pattern recognition | Historian-dependent | Works with any export |
| **dataPARC** | PI alternative | Still enterprise-priced | Open core |
| **Ignition** | Modern SCADA | Steep learning curve | Simpler, focused |
| **Python + Pandas** | Free, flexible | No infrastructure | Batteries included |

---

## 6. Attack Strategies

### 6.1 Strategy A: "Historian Export Analytics" (Sidecar Play)

**Positioning:** "Keep your PI. Analyze your exports."

**How it works:**
1. Configure PI/AVEVA to export to shared folder (IT does once)
2. Casparian watches folder, parses new exports
3. Engineers query in SQL without touching historian

**Value proposition:**
- "Don't fight IT for historian access. Use the exports."
- Works alongside existing infrastructure
- No OT network access required

**Revenue model:**
- Free: Basic CSV parsing
- Pro: Tag mapping, multi-format support
- Team: Cross-plant normalization

**Best for:** Large plants with existing PI investment

### 6.2 Strategy B: "Plant Engineer Liberation" ⭐ RECOMMENDED

**Positioning:** "Python analytics for manufacturing without the hassle."

**How it works:**
1. Engineer gets data dump (CSV, Excel, Parquet)
2. Casparian parses with AI-assisted schema discovery
3. Query in SQL, export to ML pipelines

**Value proposition:**
- "Data wrangling is 80% of your job. Make it 10%."
- No historian license required
- Works on engineer's laptop

**Why we win:**
- PI/AVEVA is IT-controlled; Casparian is engineering-controlled
- Seeq/TrendMiner cost $50K+; Casparian is $50/month
- Python + Pandas requires infrastructure work

**Revenue model:**
- Free: 3 parsers, local only
- Pro: Unlimited parsers, Scout
- Team: Collaboration, audit trails

**Best for:** Mid-size manufacturers without analytics infrastructure

### 6.3 Strategy C: "MTConnect Analytics" (Industry 4.0 Play)

**Positioning:** "Turn your MTConnect data into insights."

**How it works:**
1. Point Casparian at MTConnect agent or XML archives
2. Parser normalizes events, samples, conditions
3. Query OEE, cycle times, utilization in SQL

**Value proposition:**
- "Every modern CNC speaks MTConnect. Now analyze it."
- Machine-agnostic (Haas, Mazak, DMG MORI all work)
- Standardized output across machines

**Why we win:**
- MTConnect is open standard but raw XML is hard to analyze
- Commercial MTConnect analytics are expensive
- Casparian bridges the gap

**Revenue model:**
- Pro: MTConnect parser
- Team: Multi-machine normalization
- Enterprise: Shop floor dashboard

**Best for:** CNC machine shops, discrete manufacturing

---

## 7. Premade Parsers

### 7.1 Historian Export Parser (`historian_export.py`)

**Input:** PI/AVEVA/Canary CSV/Parquet exports

**Output Tables:**

| Table | Description |
|-------|-------------|
| `historian_tags` | Tag metadata (name, description, units, range) |
| `historian_values` | Time-series values (timestamp, tag, value, quality) |
| `historian_hourly` | Pre-aggregated hourly rollups |
| `historian_daily` | Pre-aggregated daily rollups |

**Key Fields:**
- `timestamp`: ISO 8601 timestamp
- `tag_name`: Raw tag identifier (e.g., `LI-1234.PV`)
- `tag_alias`: Human-readable name (optional mapping)
- `value`: Numeric or string value
- `quality`: Data quality flag (Good, Bad, Uncertain)

**Features:**
- Tag alias mapping file support
- Configurable aggregation windows
- Gap detection and interpolation options

### 7.2 MTConnect Parser (`mtconnect_parser.py`)

**Input:** MTConnect XML streams/archives

**Output Tables:**

| Table | Description |
|-------|-------------|
| `mtc_devices` | Device metadata (name, uuid, manufacturer) |
| `mtc_samples` | Continuous measurements (position, feedrate, load) |
| `mtc_events` | State changes (execution, mode, program) |
| `mtc_conditions` | Fault/alarm conditions |
| `mtc_oee` | Calculated OEE metrics (availability, performance, quality) |

**Key Fields:**
- `device_uuid`: Unique device identifier
- `timestamp`: ISO 8601 timestamp
- `data_item_id`: MTConnect data item identifier
- `value`: Measurement or state value
- `sequence`: MTConnect sequence number

**Library:** XML parsing with lxml, MTConnect schema validation

### 7.3 SPC/Quality Parser (`spc_parser.py`)

**Input:** CMM exports, SPC software exports, Excel quality data

**Output Tables:**

| Table | Description |
|-------|-------------|
| `spc_measurements` | Individual measurements (actual, nominal, tolerance) |
| `spc_parts` | Part metadata (part number, lot, serial) |
| `spc_control_limits` | Control chart limits (UCL, LCL, CL) |
| `spc_capability` | Process capability indices (Cp, Cpk, Pp, Ppk) |

**Key Fields:**
- `part_number`: Part identifier
- `feature`: Measured characteristic
- `actual`: Measured value
- `nominal`: Target value
- `usl`, `lsl`: Upper/lower specification limits
- `pass`: Boolean pass/fail

### 7.4 Alarm/Downtime Parser (`alarm_parser.py`)

**Input:** SCADA alarm exports, downtime logs

**Output Tables:**

| Table | Description |
|-------|-------------|
| `alarms` | Alarm events (timestamp, tag, message, priority) |
| `downtime_events` | Downtime periods (start, end, duration, reason) |
| `downtime_summary` | Aggregated downtime by reason code |

---

## 8. Go-to-Market

### 8.1 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **Engineering communities** | r/PLC, Control.com, LinkedIn groups | Month 1-3 |
| **Manufacturing conferences** | IMTS, Automate, SME events | Month 6-12 |
| **System integrators** | Partner with PI/SCADA integrators | Month 3-9 |
| **Industry associations** | AMT (MTConnect), ISA, SME | Month 3-6 |

### 8.2 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "Parse PI exports in 5 minutes" video | Top-of-funnel | High |
| "MTConnect analytics tutorial" | Developer education | High |
| "OSIsoft PI alternatives" blog | SEO, cost-conscious buyers | Medium |
| "OEE calculation from raw data" | Practical value | Medium |

### 8.3 Pricing (Manufacturing Vertical)

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Free** | $0 | Historian CSV parser, 3 custom parsers | Individual engineers |
| **Pro** | $75/user/month | MTConnect, tag mapping, unlimited parsers | Engineering teams |
| **Plant Team** | $400/month | Multi-format, cross-system joins | Plant-wide |
| **Enterprise** | Custom | Air-gap deployment, audit trails, SSO | Large manufacturers |

---

## 9. Success Metrics

### 9.1 Adoption Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Historian parser users | 200 | 1,000 |
| MTConnect parser users | 50 | 250 |
| Files processed (manufacturing) | 100K | 1M |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Manufacturing MRR | $5K | $30K |
| Manufacturing customers | 25 | 150 |
| Enterprise deals | 1 | 5 |

### 9.3 Competitive Metrics

| Metric | Target |
|--------|--------|
| "OSIsoft PI alternative" search ranking | Top 10 |
| "MTConnect analytics" search ranking | Top 5 |
| AMT/MTConnect partnership | In Year 1 |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| OT security concerns block adoption | High | Air-gapped, no network required |
| PI/AVEVA releases competing analytics | Medium | Cost advantage; flexibility |
| MTConnect adoption slower than expected | Medium | Focus on historian exports first |
| Engineering buyers have no budget | Medium | Bottom-up adoption; prove ROI |
| Complex tag naming conventions | Medium | AI-assisted tag mapping |

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Initial attack vector | Plant Engineer Liberation (B) | Clearest pain point; bottom-up |
| Day 1 scope | Exports only (no real-time) | Simpler; no OT network access needed |
| MTConnect support | Phase 1 | Open standard; growing adoption |
| OPC-UA direct connection | Deferred | Requires OT network access; security concerns |
| Seeq integration | Future consideration | Could be complementary |

---

## 12. References

- [opcua-asyncio (Python OPC-UA)](https://github.com/FreeOpcUa/opcua-asyncio)
- [MTConnect Standard](https://www.mtconnect.org/)
- [dataPARC (PI alternative)](https://www.dataparc.com/)
- [Seeq](https://www.seeq.com/)
- [AVEVA PI System](https://www.aveva.com/en/products/aveva-pi-system/)
- [Ignition SCADA](https://inductiveautomation.com/)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft |

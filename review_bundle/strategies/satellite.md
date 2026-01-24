# Satellite/Space Data Engineering Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → Satellite/Space)
**Priority:** #4 (New Frontier)
**Version:** 0.1
**Date:** January 20, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the Satellite/Space data engineering market by positioning as **"Mission-Critical Telemetry Parsing"**.

**Core Insight:** The NewSpace boom has created hundreds of satellite operators generating 50TB+ per hour of binary telemetry. They're all building Python-based ground systems and need schema validation before data reaches mission databases.

**Why Satellite/Space Is #4:**

| Factor | Satellite | DFIR (#1 contrast) | Pharma (#2 contrast) |
|--------|-----------|-------------------|---------------------|
| **The Data** | CCSDS telemetry, TLE, binary downlinks on ground stations | Disk images on evidence servers | Instrument files on lab drives |
| **Writes Python?** | YES (COSMOS, SatNOGS, custom) | YES (artifact parsers) | YES (data munging) |
| **Audit Trail** | Mission-critical (anomaly investigation) | **LEGALLY MANDATED** | **FDA REQUIRED** |
| **Why They Pay** | Data integrity for $500M satellites | Speed + liability | Compliance + traceability |
| **Sales Cycle** | Medium (startup-friendly) | Fast (boutiques) | Slow (enterprise) |
| **Urgency** | High (real-time downlinks) | EXTREME (active breach) | Medium (batch) |

**The Pitch:**
> *"Parse 50TB/hour downlinks. Schema contracts for mission-critical telemetry."*

**Why After IIoT:** Emerging sector with perfect technical fit, but smaller total addressable market. IIoT first for manufacturing scale; Satellite for technical differentiation.

---

## 2. Market Overview

### 2.1 Space Industry Size

| Metric | Value | Source |
|--------|-------|--------|
| Global space economy | $469B (2024) | Space Foundation |
| Satellite services | $130B (2024) | SIA |
| Ground segment market | $40B (2024) | NSR |
| NewSpace funding | $15B (2024) | Space Capital |
| Active satellites | 10,000+ (2025) | UCS Satellite Database |

### 2.2 The Telemetry Problem

Every satellite generates continuous telemetry:

```
Ground Station (typical)
├── downlink_20260120/
│   ├── pass_001.ccsds       (binary, CCSDS frames)
│   ├── pass_001.tle         (Two-Line Element)
│   ├── pass_002.ccsds
│   └── ...
├── decoded/
│   ├── housekeeping.csv     (decoded telemetry)
│   ├── payload.bin          (mission-specific binary)
│   └── ...
└── anomaly_logs/
    └── anomaly_20260120.json
```

**The Problem:** This data must flow to:
1. **Mission Database** for operations (real-time and historical)
2. **Anomaly Detection** systems (ML-based)
3. **Science Teams** (payload data analysis)
4. **Regulatory Bodies** (spectrum compliance)

### 2.3 Why Telemetry Parsing Is Hard

| Challenge | Description | Impact |
|-----------|-------------|--------|
| **Binary formats** | CCSDS, custom binary, vendor-specific | Need specialized parsers |
| **Volume** | 50TB+/hour for constellations | Can't manually review |
| **Real-time** | Downlink windows are short | Must process immediately |
| **Integrity** | One parsing bug = lost science | Mission-critical |
| **Versioning** | Telemetry formats change with firmware | Schema drift |

### 2.4 Current State of Ground Systems

| Approach | Prevalence | Problem |
|----------|------------|---------|
| **COSMOS** | 30% | Open-source, but no schema validation |
| **Custom Python** | 40% | No governance, fragile scripts |
| **Vendor systems (KSAT, SSC)** | 20% | Expensive, vendor lock-in |
| **Commercial GS software** | 10% | $500K+, enterprise only |

**The Gap:** No tool specifically addresses "telemetry → validated mission database" with schema contracts and quarantine. Everyone writes custom Python.

---

## 3. Target Personas

### 3.1 Primary: Ground Systems Data Engineer

| Attribute | Description |
|-----------|-------------|
| **Role** | Ground Systems Engineer, Telemetry Engineer, Mission Data Engineer |
| **Technical skill** | **Python, C, binary parsing**; comfortable with CCSDS |
| **Pain** | Writing parsers for every new satellite; no validation; anomaly investigation is painful |
| **Goal** | Reliable, validated telemetry pipeline |
| **Buying power** | Recommender; mission lead can approve $10K-50K |

**Current Workflow (painful):**
1. Satellite downlinks during pass window
2. Engineer writes Python script to decode CCSDS frames
3. Script runs, pushes to mission database
4. Anomaly detected: "Battery voltage dropped unexpectedly"
5. Engineer spends days tracing back through raw telemetry
6. Discovers: Parser bug interpreted bytes wrong for two weeks

**Casparian Workflow:**
1. `casparian scan /ground/downlinks --tag ccsds_raw`
2. `casparian run ccsds_parser.py --sink duckdb://mission.db`
3. Schema contracts validate: battery_voltage between 25-35V
4. Out-of-range values go to quarantine with source reference
5. Anomaly investigation: "Quarantine shows 47 voltage anomalies from pass_023"

### 3.2 Secondary: Mission Operations Lead

| Attribute | Description |
|-----------|-------------|
| **Role** | Mission Operations Manager, Constellation Manager |
| **Technical skill** | Medium-High; manages ground systems team |
| **Pain** | Data quality issues affect mission decisions; can't trust historical data |
| **Goal** | Reliable mission database for operations |
| **Buying power** | Mission budget; can approve $50K-200K/year |

### 3.3 Tertiary: Payload Data Scientist

| Attribute | Description |
|-----------|-------------|
| **Role** | Data Scientist, Science Team Lead |
| **Technical skill** | High (Python, ML, domain science) |
| **Pain** | Payload data quality varies; can't reproduce analysis |
| **Goal** | Clean, validated payload data for science |
| **Buying power** | Influences data engineering decisions |

---

## 4. Competitive Landscape

### 4.1 Ground Station Software

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| **Ball Aerospace COSMOS** | Open-source command/control | Free | No schema validation; no quarantine |
| **Kratos quantumGS** | Commercial ground system | $500K+/year | Enterprise only; overkill |
| **Kongsberg** | Commercial ground | Enterprise | Same |
| **SatNOGS** | Open-source ground network | Free | Focus on receive, not parsing |

### 4.2 Data Platforms

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| **Loft Orbital** | Satellite-as-a-Service | Per-mission | Owns the whole stack |
| **Spire** | Data-as-a-Service | Subscription | Competes with operators |
| **Custom Python** | DIY | "Free" | No validation, no governance |

### 4.3 Why These Aren't Enough

| Competitor | Why Casparian Wins |
|------------|-------------------|
| COSMOS | We add schema validation + quarantine on top |
| Commercial GS | $500K+ overkill; we're accessible to NewSpace |
| Custom Python | No validation, no lineage, no anomaly tracing |
| SatNOGS | Different focus (RF, not data engineering) |

### 4.4 The Market Gap

```
┌─────────────────────────────────────────────────────────────────┐
│                     ENTERPRISE TIER                              │
│  Kratos, Kongsberg, L3Harris ($500K-$5M/year)                   │
│  → Traditional satellite operators (Intelsat, SES)              │
└─────────────────────────────────────────────────────────────────┘
                          ↑
                    MARKET GAP
              (We target this gap)
                          ↓
┌─────────────────────────────────────────────────────────────────┐
│                     DIY / COSMOS TIER                            │
│  Python scripts, COSMOS, SatNOGS ("free")                        │
│  → NewSpace startups, university missions, smallsat operators   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. Why Casparian Fits

### 5.1 Core Platform Features → Satellite Value

| Casparian Feature | Satellite/Space Value |
|-------------------|----------------------|
| **Schema Contracts** | **Telemetry Validation**: Define valid ranges; catch anomalies early |
| **Quarantine** | **Anomaly Investigation**: Bad packets isolated with source reference |
| **Lineage tracking** | **Traceability**: Trace anomaly back to exact downlink pass |
| **Parser versioning** | **Firmware Updates**: Track which parser version processed which data |
| **Local-first** | **Ground Station**: Works on isolated ground networks |
| **Source Hash** | **Integrity**: Prove processed data matches raw downlink |

### 5.2 The Anomaly Investigation Story

```
MISSION OPS QUESTION                   CASPARIAN ANSWER
────────────────────                   ─────────────────
"Battery voltage dropped.              "Quarantine shows 47 readings
 When did this start?"                  below threshold starting pass_023."

"Is this a parsing bug or             "Source hash verified. Parser v2.1.0
 real anomaly?"                         matched contract. Real anomaly."

"What data was affected?"              "Lineage shows pass_023 through
                                        pass_031 during firmware v4.2."

"Can we reproduce the                  "Parser v2.1.0 + schema v3.0 =
 analysis?"                             deterministic reproduction."
```

### 5.3 Why "Schema Contracts" Is The Killer Feature

For mission-critical telemetry, one parsing bug can:
- Corrupt months of science data
- Miss a satellite failure warning
- Cause regulatory compliance issues

**The Schema Contract Guarantee:**
```
CCSDS Telemetry Contract:
| Field | Type | Min | Max | Description |
|-------|------|-----|-----|-------------|
| battery_v | float32 | 25.0 | 35.0 | Battery voltage (V) |
| solar_w | float32 | 0.0 | 500.0 | Solar array power (W) |
| temp_c | float32 | -40.0 | 85.0 | Internal temp (C) |

If battery_v = 24.3:
- Value goes to quarantine (below min)
- Alert: "Battery voltage anomaly detected"
- Lineage: "Pass 023, frame 4521, byte offset 47"
```

---

## 6. Go-to-Market

### 6.1 Positioning

**Primary:** "Mission-critical telemetry parsing with schema contracts."

**Secondary:** "Anomaly investigation in seconds, not days."

### 6.2 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **NewSpace communities** | SmallSat Symposium, LinkedIn | Month 9-15 |
| **COSMOS community** | GitHub, OpenC3 forum | Month 9-15 |
| **SatNOGS network** | Open-source collaboration | Month 12-18 |
| **Ground station providers** | AWS Ground Station, Azure Orbital | Year 2 |

### 6.3 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "CCSDS telemetry to Parquet at scale" | Technical SEO | High |
| "Schema contracts for satellite data" | Mission-critical angle | High |
| "Anomaly investigation with lineage" | Operations angle | Medium |
| Case study: [NewSpace company] traced anomaly in 5 minutes | Social proof | Medium |

### 6.4 Demo Script (90 seconds)

```
[0:00] "Your constellation generates 50TB/hour.
       Here's how to validate it before it hits your mission database."

[0:10] *Point Casparian at downlink data*
       $ casparian scan /ground/downlinks --tag ccsds_raw

[0:20] "Casparian discovers all pass files."

[0:30] *Run parser with schema contract*
       $ casparian run ccsds_parser.py --sink duckdb://mission.db

[0:45] "Schema contract validates telemetry ranges.
       Out-of-spec readings go to quarantine."

[0:55] *Show anomaly investigation*
       $ casparian quarantine show --source pass_023
       "Battery voltage anomaly started at frame 4521."

[1:10] "Lineage traces to exact source byte.
       Investigation in seconds, not days."

[1:20] "That's mission-critical telemetry parsing."
```

---

## 7. Premade Parsers (Starter Kits)

Ship these as **examples** to demonstrate the pattern:

### 7.1 CCSDS Telemetry Parser (`ccsds_parser.py`)

**Input:** CCSDS Space Packet frames (binary)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `ccsds_packets` | Decoded packets with APID, sequence, timestamp |
| `telemetry_values` | Extracted telemetry points by parameter ID |

### 7.2 TLE Parser (`tle_parser.py`)

**Input:** Two-Line Element sets (text)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `tle_entries` | Parsed TLE with orbital elements |
| `tle_metadata` | Satellite identifiers, epoch |

### 7.3 Housekeeping Telemetry Parser (`housekeeping_parser.py`)

**Input:** Generic housekeeping CSV exports

**Output Tables:**

| Table | Description |
|-------|-------------|
| `hk_readings` | Timestamped subsystem readings |
| `hk_anomalies` | Out-of-range values with context |

---

## 8. Pricing

### 8.1 Value Analysis

| Role | Salary | Time on Manual Parsing | Value of Automation |
|------|--------|----------------------|---------------------|
| Ground Systems Engineer | $140K/year | 30% (parser development) | $42K/year |
| Mission Ops | $130K/year | 20% (anomaly investigation) | $26K/year |

**Additional value:**
- Mission-critical data integrity: Priceless ($500M satellites)
- Anomaly investigation speed: Hours → seconds
- Regulatory compliance: Spectrum + safety documentation

### 8.2 Pricing Tiers

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Mission** | $2,000/month | Full platform, CCSDS parsers, 1 constellation | NewSpace startup |
| **Constellation** | $50K+/year | Multi-constellation, custom formats, priority support | Constellation operator |

### 8.3 Why This Pricing Works

Satellite operators spend:
- $100K-500K on ground station software
- $500M+ on satellites (data is the product)
- $150K+/year on ground systems engineering

Casparian at $50K/year is **cheap** for mission-critical data integrity.

---

## 9. Success Metrics

### 9.1 Adoption Metrics (Year 1)

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| NewSpace pilots | 1 | 3 |
| Ground Systems Engineers using | 3 | 10 |
| Telemetry processed | 1TB | 100TB |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Satellite MRR | $2K | $10K |
| Constellation contracts | 0 | 1 |
| Annual contract value | - | $50K+ |

### 9.3 Validation Metrics

| Metric | Target |
|--------|--------|
| "Schema contracts caught real anomaly" | 1+ pilot |
| COSMOS integration | Working demo |
| CCSDS parser library | Complete |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Small total market | Medium | High LTV per customer; expand to defense space |
| Binary format complexity | High | Focus on CCSDS standard; partner with COSMOS |
| Long mission development cycles | Medium | Target operational missions, not development |
| Competition from COSMOS | Low | We're complementary (validation layer on top) |
| Requires space domain expertise | High | Partner with space consultants; hire from industry |

---

## 11. Strategic Synergies

### 11.1 Defense/GEOINT Connection

Satellite/Space expertise directly transfers to Defense/GEOINT (#5):

| Satellite Skill | Defense Application |
|-----------------|---------------------|
| CCSDS parsing | Military satellite telemetry |
| Binary format expertise | Classified sensor data |
| Ground station workflows | Tactical ground processing |
| Air-gapped deployment | SIPR/JWICS networks |

**Strategy:** Win NewSpace customers first (faster sales), then leverage expertise for defense subcontracts.

### 11.2 COSMOS Ecosystem

Ball Aerospace's COSMOS is the de facto open-source ground system. Integration strategy:

| Phase | Goal |
|-------|------|
| **Phase 1** | Casparian as COSMOS telemetry sink |
| **Phase 2** | Schema contract plugin for COSMOS |
| **Phase 3** | Joint marketing with OpenC3 community |

---

## 12. References

- [CCSDS Standards](https://public.ccsds.org/default.aspx) - Space data system standards
- [Ball COSMOS / OpenC3](https://github.com/OpenC3/cosmos) - Open-source ground system
- [SatNOGS](https://satnogs.org/) - Open-source ground station network
- [SpaceNews](https://spacenews.com/) - Industry news
- [SmallSat Symposium](https://www.smallsatshow.com/) - Key conference

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial draft based on strategic research |

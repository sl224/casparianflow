# IIoT/OT Data Engineering Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → IIoT/OT)
**Priority:** #3 (Industrial Expansion)
**Version:** 0.1
**Date:** January 20, 2026

---

## 1. Executive Summary

This substrategy details how Casparian Flow captures the Industrial IoT / Operational Technology data engineering market by positioning as **"Escape Your Historian"**.

**Core Insight:** Manufacturing and utility companies have billions of rows of sensor data locked in proprietary historians (OSIsoft PI, AspenTech IP21, Wonderware). They're building data lakes for ML/AI initiatives but struggle to extract and validate this data at scale.

**Why IIoT/OT Is #3:**

| Factor | IIoT/OT | DFIR (#1 contrast) | Pharma (#2 contrast) |
|--------|---------|-------------------|---------------------|
| **The Data** | Historian exports (CSV, binary) on industrial networks | Disk images on evidence servers | Instrument files on lab drives |
| **Writes Python?** | YES (ETL pipelines) | YES (artifact parsers) | YES (data munging) |
| **Audit Trail** | Nice-to-have (data quality) | **LEGALLY MANDATED** | **FDA REQUIRED** |
| **Why They Pay** | Historian escape + ML enablement | Speed + liability | Compliance + traceability |
| **Sales Cycle** | Medium (mid-market) | Fast (boutiques) | Slow (enterprise) |
| **LTV** | High ($25K-100K/year) | High (per-engagement) | Highest (sticky) |

**The Pitch:**
> *"Escape your historian. Query decades of PLC data with SQL."*

**Why After Pharma:** Data lake initiatives are hot, but sales cycle is medium. Pharma first for enterprise credibility; IIoT for volume.

---

## 2. Market Overview

### 2.1 Industrial Data Market Size

| Metric | Value | Source |
|--------|-------|--------|
| Global IIoT market | $110B (2024) | MarketsandMarkets |
| Industrial analytics | $25B (2024) | Mordor Intelligence |
| Historian software market | $2.5B | Various |
| Data lake adoption in manufacturing | 45% planning | Deloitte |

### 2.2 The Historian Problem

Every manufacturing facility has decades of sensor data locked in historians:

```
Historian Server (typical)
├── OSIsoft PI Archive/
│   ├── pi_2010.arc     (proprietary binary)
│   ├── pi_2011.arc
│   └── ...20 years...
├── IP21 Export/
│   ├── export_20260120.csv   (millions of rows)
│   └── ...
└── Wonderware/
    └── historian.mdf         (SQL Server)
```

**The Problem:** This data must flow to:
1. **Data Lake** (Databricks, Snowflake) for ML/AI
2. **Analytics** (Power BI, Tableau) for operations
3. **Predictive Maintenance** systems

### 2.3 Why Historians Are Prisons

| Historian | Vendor | Lock-In Mechanism | Annual Cost |
|-----------|--------|-------------------|-------------|
| **OSIsoft PI** | AVEVA (Schneider) | Proprietary binary format, AF structure | $50K-500K |
| **AspenTech IP21** | AspenTech | Proprietary compression, licensing | $100K+ |
| **Wonderware** | AVEVA | SQL Server dependency, licensing | $50K-200K |
| **GE Proficy** | GE Digital | Proprietary format | $50K-300K |

**The Escape:** Companies are migrating to open formats (Parquet, Delta Lake) but need to:
1. Extract data from proprietary formats
2. Validate data quality (sensor noise, gaps)
3. Maintain lineage for troubleshooting

### 2.4 Current State of Industrial Data Engineering

| Approach | Prevalence | Problem |
|----------|------------|---------|
| **Vendor connectors** | 40% | Expensive, vendor lock-in continues |
| **Custom Python scripts** | 35% | No validation, schema drift, breaks silently |
| **OSIsoft PI to Parquet** | 15% | Manual, no governance |
| **Palantir Foundry** | 10% | $1M+/year, overkill |

**The Gap:** No tool specifically addresses "historian exports → validated data lake" with schema contracts and quarantine.

---

## 3. Target Personas

### 3.1 Primary: Industrial Data Engineer

| Attribute | Description |
|-----------|-------------|
| **Role** | Data Engineer, OT Data Analyst, Industrial Analytics Engineer |
| **Technical skill** | **Python, SQL, Spark**; comfortable with ETL |
| **Pain** | Writing scripts to parse historian exports; no validation; ML models fail on bad data |
| **Goal** | Reliable, validated data pipeline from historian to data lake |
| **Buying power** | Recommender; team lead can approve $10K-50K |

**Current Workflow (painful):**
1. Historian exports data to CSV (or worse, proprietary binary)
2. Engineer writes Python script to parse and clean
3. Script runs in cron, pushes to Databricks
4. ML model fails: "Why is there a spike at 3am on Tuesdays?"
5. Engineer spends days tracing bad data back through pipeline
6. Discovers: sensor was offline, historian logged zeros as valid

**Casparian Workflow:**
1. `casparian scan /historian/exports --tag sensor_data`
2. `casparian run historian_parser.py --sink parquet://datalake/`
3. Schema contracts validate expected ranges (temp > -40, < 150)
4. Bad readings go to quarantine, not production
5. ML team: "Data is clean. Model works."

### 3.2 Secondary: OT/IT Convergence Lead

| Attribute | Description |
|-----------|-------------|
| **Role** | Director of Digital Transformation, OT/IT Integration Manager |
| **Technical skill** | Medium-High; manages technical teams |
| **Pain** | Data lake initiatives stall on data quality; historian vendor lock-in |
| **Goal** | Unified data platform for analytics and ML |
| **Buying power** | Department budget; can approve $50K-200K/year |

### 3.3 Tertiary: Data Science Team

| Attribute | Description |
|-----------|-------------|
| **Role** | Data Scientist, ML Engineer |
| **Technical skill** | High (Python, ML frameworks) |
| **Pain** | Models fail on bad data; can't trust historian exports |
| **Goal** | Clean, validated training data |
| **Buying power** | Influences data engineering decisions |

---

## 4. Competitive Landscape

### 4.1 Historian Vendors

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| **OSIsoft PI to Parquet** | AVEVA's own connector | Expensive | Still locked to AVEVA; limited validation |
| **AspenTech Connect** | IP21 connectors | Enterprise | Vendor lock-in; minimal transformation |
| **HighByte Intelligence Hub** | OT data modeling | $50K+/year | Focus on modeling, not migration |

### 4.2 Data Lake Platforms

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| **Databricks** | Data lake analytics | $50K+/year | Expects clean data; garbage in, garbage out |
| **Snowflake** | Cloud data warehouse | Per-usage | Same; no historian expertise |
| **Palantir Foundry** | Industrial analytics | $1M+/year | Overkill for data migration |

### 4.3 Why These Aren't Enough

| Competitor | Why Casparian Wins |
|------------|-------------------|
| Historian vendors | Their connectors maintain lock-in; we enable escape |
| Databricks/Snowflake | They assume clean data; we MAKE data clean |
| Palantir | $1M+ overkill; we're accessible to mid-market |
| Custom scripts | No validation, no quarantine, no lineage |

### 4.4 The Market Gap

```
┌─────────────────────────────────────────────────────────────────┐
│                     ENTERPRISE TIER                              │
│  Palantir, Seeq, Uptake ($500K-$5M/year)                        │
│  → Fortune 500 manufacturing                                     │
└─────────────────────────────────────────────────────────────────┘
                          ↑
                    MARKET GAP
              (We target this gap)
                          ↓
┌─────────────────────────────────────────────────────────────────┐
│                     DIY TIER                                     │
│  Python scripts, manual Excel, tribal knowledge                  │
│  → Mid-market manufacturing, utilities                           │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. Why Casparian Fits

### 5.1 Core Platform Features → IIoT Value

| Casparian Feature | IIoT/OT Value |
|-------------------|---------------|
| **Schema Contracts** | **Data Quality**: Define valid sensor ranges; reject outliers |
| **Quarantine** | **Sensor Noise**: Bad readings isolated, not silently dropped |
| **Lineage tracking** | **Traceability**: Trace ML failures back to source sensor/file |
| **Parser versioning** | **Reproducibility**: Know exactly which version processed data |
| **Local-first** | **OT Network**: Industrial networks are often air-gapped |
| **Source Hash** | **Integrity**: Prove data lake matches historian export |

### 5.2 The Data Quality Story

```
DATA SCIENTIST QUESTION                CASPARIAN ANSWER
────────────────────────               ─────────────────
"Why did my model spike              "Sensor 47 was offline 3am-4am.
 on Tuesday at 3am?"                  Those readings are in quarantine."

"Is this data the same as            "Source hash matches. This is
 what was in the historian?"          exactly what was exported."

"What processing was done?"           "Parser v2.1.0 with schema
                                      temp_min=-40, temp_max=150."

"Why are there gaps?"                 "Historian export had 47 null
                                      rows. All in quarantine."
```

### 5.3 Why "Quarantine" Is The Killer Feature

For ML/AI initiatives, data quality is everything. One bad row can:
- Corrupt a model's training
- Cause production failures
- Waste weeks debugging

**The Quarantine Guarantee:**
```
Historian Export: sensor_data_20260120.csv
Total Rows: 1,000,000
Valid Rows: 998,523 → data_lake/sensor_data.parquet
Quarantine: 1,477 → quarantine/sensor_data_20260120.parquet

Quarantine includes:
| timestamp | sensor_id | value | reason |
|-----------|-----------|-------|--------|
| 03:00:00  | 47        | NULL  | Missing value |
| 03:15:00  | 47        | -999  | Below min (-40) |
| 03:30:00  | 47        | 0     | Sensor offline flag |

Data scientist can inspect quarantine, understand why rows were excluded.
```

---

## 6. Go-to-Market

### 6.1 Positioning

**Primary:** "Escape your historian. Query decades of sensor data with SQL."

**Secondary:** "Data lake ready: validated, clean, with full lineage."

### 6.2 Channels

| Channel | Approach | Timeline |
|---------|----------|----------|
| **Industrial data communities** | LinkedIn, IoT conferences | Month 6-12 |
| **Manufacturing analytics forums** | AWS Industrial, Azure IoT | Month 6-12 |
| **SI partners** | Accenture Industry X, Deloitte | Month 9-18 |
| **IIoT platform vendors** | Uptake, Samsara, Tulip | Year 2 |

### 6.3 Content Strategy

| Content | Purpose | Priority |
|---------|---------|----------|
| "From PI to Parquet: historian data migration" | Technical SEO | High |
| "Data quality for predictive maintenance" | ML angle | High |
| "Escape historian vendor lock-in" | Business angle | Medium |
| Case study: [Manufacturer] cut data prep by X% | Social proof | Medium |

### 6.4 Demo Script (90 seconds)

```
[0:00] "Your historian has 20 years of sensor data.
       Here's how to get it to your data lake, validated."

[0:10] *Point Casparian at historian exports*
       $ casparian scan /historian/exports --tag sensor_raw

[0:20] "Casparian discovers all export files."

[0:30] *Run parser with schema contract*
       $ casparian run historian_parser.py --sink parquet://datalake/

[0:45] "Schema contract validates: temp between -40 and 150.
       Bad readings go to quarantine, not your model."

[0:55] *Show quarantine*
       $ casparian quarantine list
       "1,477 rows quarantined. Sensor 47 was offline at 3am."

[1:10] "Clean data in your lake. Full lineage. No surprises."

[1:20] "That's historian escape with data quality."
```

---

## 7. Premade Parsers (Starter Kits)

Ship these as **examples** to demonstrate the pattern:

### 7.1 OSIsoft PI Export Parser (`pi_export_parser.py`)

**Input:** PI DataLink exports (CSV), PI Web API JSON

**Output Tables:**

| Table | Description |
|-------|-------------|
| `pi_values` | Timestamped sensor values with quality flags |
| `pi_tags` | Tag metadata (units, ranges, descriptions) |

### 7.2 Historian CSV Parser (`historian_csv_parser.py`)

**Input:** Generic historian CSV exports (timestamp, tag, value)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `sensor_readings` | Normalized readings with validation |
| `sensor_metadata` | Tag configuration and thresholds |

### 7.3 OPC-UA Export Parser (`opcua_parser.py`)

**Input:** OPC-UA historical data exports (JSON, CSV)

**Output Tables:**

| Table | Description |
|-------|-------------|
| `opc_values` | Node values with timestamps |
| `opc_nodes` | Node hierarchy and metadata |

---

## 8. Pricing

### 8.1 Value Analysis

| Role | Salary | Time on Manual ETL | Value of Automation |
|------|--------|-------------------|---------------------|
| Data Engineer | $130K/year | 40% (historian wrangling) | $52K/year |
| Data Scientist | $150K/year | 20% (debugging bad data) | $30K/year |

**Additional value:**
- Historian license escape: $50-500K/year
- ML model reliability: Priceless for predictive maintenance

### 8.2 Pricing Tiers

| Tier | Price | Features | Target |
|------|-------|----------|--------|
| **Team** | $1,000/month | Full platform, 10 users, historian parsers | Mid-size manufacturing |
| **Enterprise** | $25K+/year | Unlimited users, custom formats, priority support | Large manufacturing |

### 8.3 Why This Pricing Works

Manufacturing companies spend:
- $50-500K/year on historian licenses
- $500K+ on failed data lake initiatives
- $150K+/year on data engineering headcount

Casparian at $25K/year is **cheap** for validated data migration.

---

## 9. Success Metrics

### 9.1 Adoption Metrics (Year 1)

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| Manufacturing pilots | 2 | 5 |
| Data Engineers using | 5 | 20 |
| Rows processed | 1B | 10B |

### 9.2 Revenue Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| IIoT MRR | $5K | $20K |
| Enterprise contracts | 0 | 3 |
| Annual contract value | - | $25K+ each |

### 9.3 Validation Metrics

| Metric | Target |
|--------|--------|
| "Quarantine is valuable" feedback | 3+ pilots |
| Successful data lake migration | 1 (10B+ rows) |
| Historian parser library | 5+ formats |

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Historian format complexity | High | Focus on exports (CSV, JSON), not proprietary binary |
| Long enterprise sales cycle | Medium | Target mid-market first; reference customers |
| Competition from historian vendors | Medium | Position as escape, not replacement |
| IT/OT convergence is slow | Medium | Target companies with active data lake initiatives |
| Requires OT domain expertise | Medium | Partner with industrial SIs |

---

## 11. References

- [OSIsoft PI](https://www.aveva.com/en/products/aveva-pi-system/) - Market leader historian
- [AspenTech IP21](https://www.aspentech.com/en/products/msc/aspen-infoplus-21) - Process industries historian
- [HighByte Intelligence Hub](https://highbyte.com/) - OT data modeling
- [Industrial Data Space](https://industrialdataspace.fraunhofer.de/) - European initiative

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial draft based on strategic research |

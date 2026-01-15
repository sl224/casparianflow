# Competitive Matrix

**Status:** Reference Document
**Parent:** STRATEGY.md
**Version:** 0.1
**Date:** January 14, 2026

---

## Overview

This document provides a consolidated view of competitive alternatives across all verticals. Use this to understand positioning, differentiation, and pricing comparisons.

**Casparian's Universal Position:** Transform dark data (files on disk) into queryable datasets. We are the **plumbing layer** that structures raw files so downstream tools can use them.

---

## 1. By Vertical

### 1.1 Financial Services

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **Bloomberg Terminal** | Comprehensive data, real-time | $32K/year/seat, locked ecosystem | "Bloomberg is data, we're plumbing" | 10-50x cheaper |
| **Refinitiv Eikon** | Enterprise integration | Expensive, complex | Same as Bloomberg | 10-50x cheaper |
| **Fivetran/Airbyte** | SaaS ETL, connectors | Monthly fees, cloud-only | Local-first, custom formats | Comparable |
| **EdgarTools** | SEC filing access | Library only, no pipeline | Complete solution with Scout | Free vs Free |
| **pyfixmsg** | FIX parsing (Morgan Stanley) | Testing focus, not operations | Trade Break Workbench | N/A |

**Key Differentiator:** FIX log → SQL in minutes for trade break resolution (T+1 urgency)

### 1.2 Healthcare / HL7

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **Mirth Connect** | Industry standard routing | Now commercial (v4.6+), routing not analytics | **Complementary** - we query archives | N/A (different function) |
| **Mirth Forks (OIE, BridgeLink)** | Open source continuation | Uncertain roadmap | Compatible with archives | N/A |
| **python-hl7** | Simple HL7 parsing | Library only, manual pipeline | Complete solution | Free vs Free |
| **Rhapsody** | Enterprise integration | Expensive, complex | Analyst self-service | 10x cheaper |

**Key Differentiator:** Analysts query HL7 archives directly without waiting for Interface Team

### 1.3 Defense / Tactical

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **Palantir Gotham** | Powerful analytics, DoD contracts | $10B contracts, requires server infrastructure | Laptop-deployable, air-gapped | 100x cheaper |
| **ArcGIS Enterprise** | Geospatial standard | $100K+/year, requires server | Works disconnected | 2-10x cheaper |
| **Custom Python** | Free, flexible | Brittle, no governance, single point of failure | Production-grade, memory-safe | Similar |
| **Manual Analysis** | No software cost | Hours per file, analyst burnout | Automated parsing | N/A |

**Key Differentiator:** Only laptop-deployable, air-gapped data structuring tool that exists

### 1.4 Legal / eDiscovery

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **Relativity** | Industry standard | $150K+/year, requires hosting | PST processing at fraction of cost | 10-50x cheaper |
| **Logikcull** | Cloud-native, easy | Per-GB pricing adds up | Flat rate, local processing | Comparable at scale |
| **Nuix** | Powerful processing | $100K+, complex | Analyst-accessible | 5-10x cheaper |
| **pst-extractor (Python)** | Free, open source | Library only, PST handling complex | Complete solution | Free vs Free |

**Key Differentiator:** Small law firms can process PST/OST in-house without $100K+ tools

### 1.5 Manufacturing

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **OSIsoft PI / AVEVA** | Industry standard historian | Expensive licensing, vendor lock-in | Analyze historian exports | 10x cheaper |
| **Seeq** | Modern analytics on PI | Still requires PI license | Works with exports only | Comparable |
| **InfluxDB** | Time-series database | Requires migration | Parse existing exports | Different category |
| **Grafana** | Visualization | Not for file parsing | Complementary | N/A |

**Key Differentiator:** Analyze historian CSV/Parquet exports without additional licensing

### 1.6 Mid-Size Business

| Competitor | Strength | Weakness | Our Angle | Price Comparison |
|------------|----------|----------|-----------|------------------|
| **Fivetran** | Easy SaaS connectors | $2K+/month, cloud-only | Local files, one-time exports | 5-10x cheaper |
| **Airbyte** | Open source, many connectors | Complexity, cloud focus | Zero-ceremony local | Free vs Free |
| **Stitch Data** | Simple, affordable | Limited transformations | Full parsing capability | Comparable |
| **Excel/Power Query** | Everyone knows it | Breaks at scale, no automation | Scales to millions of rows | N/A |

**Key Differentiator:** "Data team of one" can parse QuickBooks/Salesforce exports without engineering

---

## 2. Cross-Cutting Competitors

These competitors appear across multiple verticals:

### 2.1 Fivetran / Airbyte (ETL)

| Aspect | Fivetran/Airbyte | Casparian |
|--------|------------------|-----------|
| **Focus** | API connectors, SaaS-to-warehouse | File parsing, dark data |
| **Deployment** | Cloud-first | Local-first |
| **Pricing** | Per-connector, usage-based | Flat rate |
| **Custom formats** | Limited | Unlimited (Python) |
| **Air-gapped** | No | Yes |

**When they win:** Live SaaS data sync, enterprise cloud environments
**When we win:** Files on disk, custom formats, air-gapped, cost-sensitive

### 2.2 Palantir (Analytics)

| Aspect | Palantir | Casparian |
|--------|----------|-----------|
| **Focus** | Data fusion, analytics, visualization | Data structuring (upstream) |
| **Deployment** | Server infrastructure, cloud | Laptop, CLI |
| **Pricing** | $1M+/year enterprise | $50K-500K/year |
| **Target** | Executives, analysts with training | Self-service analysts |
| **Air-gapped** | Possible but complex | Native |

**When they win:** Enterprise-wide analytics platform, government mega-contracts
**When we win:** Tactical edge, laptop deployment, budget-constrained, self-service

### 2.3 Databricks / Snowflake (Data Platforms)

| Aspect | Databricks/Snowflake | Casparian |
|--------|----------------------|-----------|
| **Focus** | Data lakehouse, SQL analytics | File parsing, ingestion |
| **Deployment** | Cloud | Local-first, optional cloud |
| **Pricing** | Compute-based, scales with usage | Flat rate |
| **Relationship** | **Complementary** | Feeds into these platforms |

**Position:** We're **upstream** of Databricks/Snowflake. We parse files into Parquet that can be loaded into these platforms.

---

## 3. Positioning Summary

### 3.1 Where We Fight

| Battle | Our Advantage |
|--------|---------------|
| Custom file formats | Python extensibility, Parser Lab |
| Cost-sensitive buyers | 10-50x cheaper than enterprise tools |
| Air-gapped environments | Native support, bundle deployment |
| Self-service analytics | No IT dependency, analyst-friendly |
| Compliance/audit trail | Full lineage, schema contracts |

### 3.2 Where We Don't Fight

| Battle | Why We Avoid |
|--------|--------------|
| Real-time streaming | Different architecture, not our focus |
| Live SaaS connectors | Fivetran/Airbyte do this well |
| Visualization/dashboards | Metabase/Grafana do this well |
| Enterprise analytics | Palantir/Tableau have this market |

### 3.3 Universal Pitch

> "We're not replacing your analytics tools. We're making your files usable by them."

---

## 4. Pricing Philosophy Comparison

| Vertical | Our Entry Price | Typical Alternative | Savings |
|----------|-----------------|---------------------|---------|
| Finance | $15K/desk/year | $50-100K (TCA tools) | 70-85% |
| Healthcare | $15K/year | $50K+ (analytics platforms) | 70% |
| Defense | $50K/deployment | $1M+ (Palantir) | 95% |
| Legal | $10K/firm/year | $150K+ (Relativity) | 93% |
| Manufacturing | $15K/plant/year | $100K+ (historian licenses) | 85% |
| Mid-Size | $200/user/month | $2K/month (Fivetran) | 90% |

---

## 5. Competitive Response Playbook

### 5.1 If Competitor Says...

| Objection | Response |
|-----------|----------|
| "We have more connectors" | "We handle files they can't parse - custom formats, legacy exports" |
| "We're enterprise-grade" | "So are we - schema contracts, audit trails, memory-safe Rust core" |
| "We're cheaper/free" | "Are they? Calculate total cost including engineering time for custom parsing" |
| "We integrate with everything" | "We output Parquet/SQL - works with everything too" |
| "We're the industry standard" | "We complement standards, not replace them (Mirth + Casparian)" |

### 5.2 Competitive Alerts

Monitor these for changes:
- [ ] Mirth Connect pricing changes
- [ ] Palantir small business offerings
- [ ] Fivetran file connector expansion
- [ ] OSIsoft/AVEVA pricing audits
- [ ] New open-source parsing tools

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 0.1 | Initial competitive matrix created from strategy maintenance workflow |

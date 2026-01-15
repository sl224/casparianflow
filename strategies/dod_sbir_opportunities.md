# DoD SBIR/STTR Opportunities Analysis

**Status:** Research Complete
**Parent:** [strategies/defense_tactical.md](defense_tactical.md) Section 7.2
**Purpose:** Track relevant DoD funding opportunities for Casparian Flow
**Version:** 1.1
**Date:** January 8, 2026
**Last Updated:** January 14, 2026
**Next Review:** February 1, 2026 (post-funding deadline check)

---

## 1. Program Status (CRITICAL)

### Current Situation (Updated January 14, 2026)

**SBIR/STTR programs expired September 30, 2025** and remain in limbo. Congress has not passed reauthorization despite multiple attempts.

| Status | Detail |
|--------|--------|
| **Program Authority** | Expired Sept 30, 2025 |
| **New Solicitations** | Paused - no new applications accepted |
| **Existing Awards** | Continue under current terms |
| **Congressional Status** | Impasse between clean extension vs. reform bills |
| **Next Window** | January 30, 2026 funding deadline |

### Congressional Activity

| Bill | Status | Description |
|------|--------|-------------|
| [H.R. 5100](https://www.congress.gov/bill/119th-congress/house-bill/5100) | Passed House, **blocked in Senate** | One-year clean extension |
| [S.1573](https://www.congress.gov/bill/119th-congress/senate-bill/1573) | Senate (Ernst) | Full reauthorization with reforms |
| [H.R. 3169](https://www.congress.gov/bill/119th-congress/house-bill/3169) | House | SBIR/STTR Reauthorization Act of 2025 |

**The Impasse:** Sen. Ernst is blocking the clean 1-year extension (H.R. 5100) in favor of her reform bill (S.1573). The "six corners" (House/Senate Small Business + Science committees from both parties) have not aligned.

**Key Date:** January 30, 2026 - Government funding deadline; SBIR reauthorization may be attached to continuing resolution or appropriations bill.

### What This Means for Casparian

1. **No new proposals accepted** until reauthorization
2. **Prepare now** - topics will reopen quickly once authorized
3. **Monitor weekly** - [defensesbirsttr.mil](https://www.defensesbirsttr.mil) and [DSIP Portal](https://www.dodsbirsttr.mil/topics-app/)
4. **Topics announced previously** indicate DoD priorities (will likely return)

---

## 2. Best-Fit Topics Identified

### 2.1 A254-011: Artificial Intelligence for Interoperability ⭐⭐⭐

**This is the closest match to Casparian's capabilities.**

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Army |
| **Topic Number** | A254-011 |
| **Phase** | Phase I (Feasibility Study) |
| **Funding** | Up to $250,000 |
| **Duration** | 6 months |
| **Status** | Closed Feb 26, 2025 (expect similar topic to return) |

**Objective (verbatim from solicitation):**
> "Apply Large Language Models (LLMs) and/or other Artificial Intelligence (AI) approaches to support and automate warfighter's system's integrations. This pertains to problems with data unification and interoperability **regardless of the target system, source system, or data format**, with focus on usage in **tactical environments**."

**Why Casparian Fits:**
- Transforms disparate formats (CoT, PCAP, NITF) into standardized SQL/Parquet
- Works in tactical (DDIL) environments
- Local-first, no cloud required
- Python-based parser development (accessible to warfighters)

**Positioning for Proposal:**
```
Problem: Tactical data exists in incompatible formats (CoT XML, PCAP, NITF, KLV)
Solution: Casparian provides format-agnostic transformation to SQL-queryable data
Differentiator: Runs on laptop, air-gapped, analyst can modify parsers
Outcome: Ubiquitous data access regardless of source format
```

**Phase I Deliverables Required:**
- Research and document AI/ML techniques for system integration
- Identify approach addressing "API, data model, and message mapping"
- Performance metrics aligned with interoperability goals
- Technology Readiness Level 2 (TRL-2) demonstration

---

### 2.2 Generative AI Enabled Tactical Network

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Army |
| **Focus** | NGC2 (data-centric C2 architecture) |
| **Funding** | Up to $250,000 (Phase I) |
| **Status** | Closed March 2025 |

**Objective:**
> "Create realistic tactical data streams... The environment should reflect a realistic tactical network **(DDIL environment)** with multiple data access and delivery demands in real time."

**Why Casparian Fits:**
- Designed for DDIL environments
- Processes tactical data streams (CoT, tracks, telemetry)
- Single Rust binary, no server infrastructure
- Enables data access without connectivity

**Note:** Required xTechIgnite white paper submission as prerequisite.

---

### 2.3 xTechOverwatch Open Topic

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Army |
| **Type** | Prize competition + Direct to Phase II |
| **Funding** | Up to $1M prizes; $2M DP2 SBIR |
| **Status** | Finals Oct 2025; SBIR proposals Nov 2025 |

**Keywords from solicitation:**
- **Edge Computing**
- **Sensor Fusion**
- **Secure Communications**
- **Human-Machine Teaming**
- **Modular Integration**

**Technical Requirements:**
> "Plug-and-play hardware/software interfaces following Army IOP, NATO STANAGs, and other interoperability standards... including **standardized data protocols and API-based integration**."

**Why Casparian Fits:**
- Edge computing architecture (single binary)
- Sensor data fusion (NITF + CoT + PCAP)
- Standardized output (SQL/Parquet)
- API-based (MCP integration for AI assistance)

---

### 2.4 Context-Aware Decision Support

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Army |
| **Focus** | AI-driven capabilities for military planning |
| **Phase II Deliverables** | Real-time data processing algorithms |

**Objective:**
> "Develop innovative solutions leveraging generative AI to create interoperable, AI-driven capabilities that will consolidate, synthesize, and prioritize **real-time data** to support military planning and tactical decision-making."

**Casparian Angle:**
- Pre-processor for decision support systems
- Structures raw data for AI/ML consumption
- Schema contracts ensure data quality

---

### 2.5 Distributed Electromagnetic Sensing (USASOC)

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Army / USASOC |
| **Focus** | Electronic Warfare / spectrum awareness |
| **Funding** | $250K (Phase I) / $2M (Direct to Phase II) |

**Priority Areas:**
> "AI/ML-enabled **edge processing**, automated signal detection and characterization, operationally relevant outputs for users not trained in EW systems, **near-real-time data availability**, and integration with tactical situational awareness."

**Casparian Fit:**
- Edge processing architecture
- Transforms complex data for non-expert users
- SQL output is "operationally relevant" for analysts
- Local processing without cloud dependency

---

### 2.6 Space Force: Satellite Data Analytics (Upcoming)

| Attribute | Detail |
|-----------|--------|
| **Agency** | U.S. Space Force |
| **Focus** | Proliferated Warfighter Space Architecture (PWSA) |
| **Status** | Was scheduled Sept 24, 2025 - delayed pending reauth |

**Objective:**
> "Create a secure, adaptable software environment capable of **ingesting, integrating, and analyzing high-volume, low-latency data streams** from diverse space-based sources, enhancing real-time situational awareness and advanced analytics."

**Casparian Fit:**
- High-volume file processing
- Multi-format ingestion
- SQL-based analytics
- Secure, local-first architecture

---

## 3. Topic Alignment Matrix

| Topic | Data Interop | Edge/DDIL | Tactical | AI/ML | Fit Score |
|-------|--------------|-----------|----------|-------|-----------|
| **AI for Interoperability** | ✅ Core | ✅ Required | ✅ Focus | ✅ Required | ⭐⭐⭐ |
| **GenAI Tactical Network** | ✅ Yes | ✅ Core | ✅ Core | ✅ Yes | ⭐⭐⭐ |
| **xTechOverwatch** | ✅ Yes | ✅ Yes | ✅ Yes | Optional | ⭐⭐ |
| **Context-Aware Decision** | ✅ Yes | Partial | ✅ Yes | ✅ Core | ⭐⭐ |
| **USASOC EW Sensing** | Partial | ✅ Core | ✅ Yes | ✅ Yes | ⭐⭐ |
| **Space Force Analytics** | ✅ Yes | Partial | Partial | Optional | ⭐ |

---

## 4. Competitive Positioning for SBIR

### 4.1 Casparian Differentiators

| Differentiator | Evidence | SBIR Value |
|----------------|----------|------------|
| **Rust Core** | Memory-safe, NSA/CISA recommended | Security posture |
| **Local-First** | No cloud, no server required | DDIL compliance |
| **Single Binary** | ~50MB static build | Sneakernet deployment |
| **Python Parsers** | Analyst-modifiable | Extensibility |
| **Schema Contracts** | Immutable after approval | Data governance |
| **AI-Assisted** | MCP integration for Claude | Phase 2 differentiator |

### 4.2 Positioning Statement for Proposals

**Problem:**
> Tactical operators have raw data files (CoT, PCAP, NITF, KLV) scattered across disconnected systems. Enterprise analytics tools (Palantir, ArcGIS) require server infrastructure and connectivity unavailable at the tactical edge. Analysts resort to manual file inspection or brittle ad-hoc scripts.

**Solution:**
> Casparian Flow is a local-first data transformation engine that converts tactical file formats into SQL-queryable datasets. A single Rust binary runs on any laptop, processes 50,000+ files in minutes, and outputs standardized Parquet/SQLite for downstream analysis.

**Unique Value:**
> Unlike cloud-based ETL or heavy enterprise platforms, Casparian:
> - Deploys via sneakernet (single file, no `pip install`)
> - Operates fully offline (no license server, no telemetry)
> - Is built in Rust (memory-safe per NSA guidance)
> - Enables analysts to modify parsers (Python) without vendor dependence

**Outcome:**
> Tactical users gain SQL access to all their raw data in minutes, enabling queries like "show me everywhere we've been in the last 30 days" or "find all network traffic to suspicious IPs" - without infrastructure, without connectivity, without waiting for IT.

---

## 5. Direct to Phase II (DP2) Opportunity

Some Army topics accept Direct to Phase II proposals for companies with existing technology.

| Requirement | Casparian Status |
|-------------|------------------|
| Working prototype | ✅ Functional CLI + parsers |
| Prior funding/development | ✅ Self-funded development |
| Technical documentation | ✅ Specs, CLAUDE.md, architecture docs |
| Demonstration capability | ✅ Can demo on sample data |

**DP2 Benefits:**
- Skip Phase I ($250K, 6 months)
- Go directly to Phase II ($2M, 18 months)
- Faster path to production

**Action:** Monitor for DP2-eligible topics when program resumes.

---

## 6. Submission Checklist

### Before Reauthorization (Now)

- [ ] Register company in SAM.gov (required for federal contracting)
- [ ] Register in DSIP Portal ([dodsbirsttr.mil](https://www.dodsbirsttr.mil))
- [ ] Obtain DUNS number (now UEI - Unique Entity ID)
- [ ] Prepare company capability statement (2-page PDF)
- [ ] Document technical approach for "AI for Interoperability" topic
- [ ] Identify SBIR consultant/advisor (optional but helpful)
- [ ] Prepare demo environment with sample tactical data

### When Program Resumes

- [ ] Monitor [DSIP Portal](https://www.dodsbirsttr.mil/topics-app/) daily
- [ ] Subscribe to SBIR/STTR mailing lists
- [ ] Review new topics within 24 hours of release
- [ ] Contact TPOCs during pre-release period (questions allowed)
- [ ] Submit proposal before deadline (typically 4-week window)

### Proposal Components (Typical)

| Section | Content |
|---------|---------|
| Technical Volume | Problem, approach, schedule, deliverables |
| Cost Volume | Labor, materials, ODCs, fee |
| Company Qualifications | Past performance, team bios |
| Commercialization Plan | Path to Phase III, market analysis |

---

## 7. Alternative Funding Paths

If SBIR reauthorization is further delayed, consider:

### 7.1 Other Transaction Authority (OTA)

| Program | Agency | Focus |
|---------|--------|-------|
| [DIU](https://www.diu.mil/) | DoD-wide | Commercial solutions for defense |
| [AFWERX](https://afwerx.com/) | Air Force | Innovation challenges |
| [NavalX](https://www.secnav.navy.mil/agility/Pages/NavalX.aspx) | Navy | Tech acceleration |

**OTA Benefits:**
- Faster than traditional contracting
- Less paperwork than SBIR
- Can lead to production contracts

### 7.2 STRATFI/TACFI (Air Force)

| Program | Funding | Purpose |
|---------|---------|---------|
| STRATFI | Up to $15M | Strategic technology acceleration |
| TACFI | Up to $3M | Tactical technology acceleration |

**Requirement:** Must have existing SBIR Phase II or commercial traction.

### 7.3 Defense Contractor Partnerships

Large primes (Palantir, Leidos, SAIC, Booz Allen) have SBIR mentorship programs and may partner on proposals.

**Approach:** Position Casparian as "upstream data structuring" that feeds their analytics platforms.

---

## 8. Key Contacts & Resources

### Official Portals

| Resource | URL |
|----------|-----|
| DoD SBIR/STTR | [defensesbirsttr.mil](https://www.defensesbirsttr.mil) |
| DSIP (Proposal Submission) | [dodsbirsttr.mil](https://www.dodsbirsttr.mil) |
| Army SBIR | [armysbir.army.mil](https://armysbir.army.mil) |
| DARPA SBIR | [darpa.mil/sbir-sttr-topics](https://www.darpa.mil/work-with-us/communities/small-business/sbir-sttr-topics) |
| SBIR.gov (All Agencies) | [sbir.gov](https://www.sbir.gov) |
| AFWERX | [afwerx.com](https://afwerx.com) |

### News & Updates

| Resource | URL |
|----------|-----|
| SBIR Reauthorization Status | [E.B. Howard Consulting](https://www.ebhoward.com/as-of-january-2026-where-things-stand-with-sbir-sttr-reauthorization/) |
| Federal News Network | [federalnewsnetwork.com](https://federalnewsnetwork.com) |
| SpaceNews (Space Force) | [spacenews.com](https://spacenews.com) |

---

## 9. Timeline & Next Steps

### Immediate (January 2026)

| Action | Priority | Owner |
|--------|----------|-------|
| Monitor reauthorization news | High | Weekly check |
| Complete SAM.gov registration | High | Required for any federal work |
| Prepare capability statement | High | 2-page PDF |
| Draft technical approach for A254-011-style topic | Medium | When time permits |

### When Program Resumes (Expected Feb 2026)

| Action | Timeline | Notes |
|--------|----------|-------|
| Review new topics | Within 24 hours | Check DSIP daily |
| Contact TPOCs | Pre-release period only | Technical questions |
| Submit Phase I proposal | Within 4 weeks | Typical window |
| Prepare DP2 if eligible | Same timeline | Higher funding, faster |

### Post-Award (If Successful)

| Phase | Duration | Funding | Deliverables |
|-------|----------|---------|--------------|
| Phase I | 6 months | $250K | Feasibility study, prototype |
| Phase II | 18-24 months | $750K-$2M | Working prototype, testing |
| Phase III | Ongoing | Unlimited | Production, deployment |

---

## 10. References

### Congressional

- [H.R.5100 - One Year Extension](https://www.congress.gov/bill/119th-congress/house-bill/5100)
- [S.1573 - SBIR/STTR Reauthorization Act](https://www.congress.gov/bill/119th-congress/senate-bill/1573)
- [H.R.3169 - SBIR/STTR Reauthorization Act](https://www.congress.gov/bill/119th-congress/house-bill/3169)

### Topics

- [Army SBIR: AI for Interoperability](https://armysbir.army.mil/topics/artificial-intelligence-interoperability/)
- [Army SBIR: GenAI Tactical Network](https://armysbir.army.mil/topics/generative-ai-enabled-tactical-network/)
- [Army SBIR: xTechOverwatch](https://armysbir.army.mil/topics/xtechoverwatch-open-topic/)
- [Army SBIR: Context-Aware Decision Support](https://armysbir.army.mil/topics/context-aware-decision-support/)

### Analysis

- [SBIR Reauthorization Status (Jan 2026)](https://www.ebhoward.com/as-of-january-2026-where-things-stand-with-sbir-sttr-reauthorization/)
- [NSBA: SBIR/STTR Lapsed](https://www.nsbaadvocate.org/post/news-sbir-sttr-programs-still-lapsed-as-ndaa-week-lands-on-capitol-big-questions-for-small-busine)
- [Space Force SBIR Held Up](https://www.satellitetoday.com/government-military/2025/12/14/space-rco-solicitation-for-agile-space-effort-held-up-by-sbir-sttr-funding-issue/)

---

## 11. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 1.0 | Initial research and analysis |
| 2026-01-14 | 1.1 | Maintenance workflow: Updated congressional status (Ernst blocking H.R. 5100); added next review date; clarified impasse situation |


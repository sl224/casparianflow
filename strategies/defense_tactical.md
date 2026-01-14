# Defense & Tactical Edge Market Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 2 (Target Market → Defense/Aerospace)
**Related Specs:** (future) specs/nitf_parser.md, specs/cot_parser.md, specs/pcap_parser.md
**Version:** 0.2
**Date:** January 8, 2026
**Codename:** Casparian Edge (formerly "Casparian Sentinel" - renamed to avoid conflicts)

---

## 1. Executive Summary

The Department of Defense is drowning in raw file data at the "Tactical Edge" - laptops in tents, disconnected ships, drone ground stations. Enterprise tools (Palantir, ArcGIS Enterprise) require cloud connectivity and server infrastructure that simply doesn't exist in DDIL (Denied, Disrupted, Intermittent, Limited) environments.

**The Opportunity:**
Casparian Flow fills a critical gap: **The disconnected, laptop-based data structuring engine.**

An intelligence analyst can drag-and-drop a folder of raw sensor data (NITF imagery headers, CoT tracks, KLV telemetry) and instantly query it via SQL—without a server farm, without internet, without waiting for IT.

**Key Insight:**
We are **upstream** of tools like Palantir. Palantir makes sense of structured data. Casparian structures the raw data so ANY downstream tool can use it.

---

## 2. Market Context

### 2.1 The "Dark Data" Problem in Defense

Just as healthcare has HL7 files on network shares, defense has:
- **Imagery archives** (NITF files from satellites, drones, reconnaissance)
- **Track logs** (CoT XML from ATAK/TAK ecosystem)
- **Video telemetry** (KLV metadata from Full Motion Video)
- **Message archives** (tactical data links, orders, reports)

This data sits on hard drives, disconnected systems, and classified networks—unqueryable, unsearchable, unusable for rapid decision-making.

### 2.2 Market Size & Opportunity

| Segment | Size | Growth | Casparian Relevance |
|---------|------|--------|---------------------|
| Defense IT spending (US) | $50B+ annually | 5-7% CAGR | Infrastructure |
| Tactical edge computing | $5B+ | 15%+ CAGR | Direct target |
| GEOINT analytics | $10B+ | 8% CAGR | NITF/imagery |
| C4ISR systems | $40B+ | 6% CAGR | Data integration |

### 2.3 The TAK Ecosystem Explosion

The [Tactical Assault Kit (TAK)](https://www.offgridweb.com/gear/tactical-awareness-kit-tak-ultimate-guide/) has become ubiquitous:
- **500,000+ users globally** (Pentagon, DHS, coalition partners)
- Open source civilian version (ATAK-CIV)
- Foundation of Army's Nett Warrior program
- Generates massive CoT (Cursor on Target) data

**Implication:** CoT parsing is high-value, large-market, low-complexity.

### 2.4 Government Push for Memory Safety

[NSA and CISA have urged](https://www.theregister.com/2025/06/27/cisa_nsa_call_formemory_safe_languages/) adoption of memory-safe languages:
- 2022: NSA guidance on memory-safe languages
- 2024: White House technical report
- 2025: Joint NSA/CISA updated guidance

**Marketing Angle:** "Casparian is built in Rust—immune to 70% of vulnerabilities that plague C/C++ legacy defense tools."

---

## 3. Where Defense Data Lives

### 3.1 The Tactical Hard Drive (Most Common)

In a TOC (Tactical Operations Center) or field unit, data accumulates on portable drives:

```
/mnt/mission_data/
├── imagery/
│   ├── 20240115_satellite_pass/
│   │   ├── img_001.ntf    # NITF files
│   │   ├── img_002.ntf
│   │   └── ...
│   └── drone_feed_20240115/
│       └── mission.ts      # STANAG 4609 video
├── tracks/
│   ├── patrol_alpha.cot    # CoT XML logs
│   └── patrol_bravo.cot
└── reports/
    └── sitrep_20240115.txt
```

**Current workflow:** Open files one by one. Try to remember what's where. Miss critical intelligence because it's buried.

**Casparian workflow:** Scan folder → Query everything via SQL.

### 3.2 The Disconnected SCIF

Classified networks (SIPRNet, JWICS) have no internet access. Software arrives via "sneakernet" (physical media).

**Requirements:**
- Single-file deployment (no `pip install`)
- No network calls (no telemetry, no license checks)
- No dynamic library downloads
- Works on government-furnished equipment (often outdated)

### 3.3 The Ship/Aircraft/Vehicle

Deployed platforms have intermittent or no connectivity:
- Navy vessels at sea
- Aircraft on mission
- Ground vehicles in contested areas

**DDIL Reality:** Must process data locally, then sync when connectivity returns.

---

## 4. Target Formats (Priority Order)

### 4.1 Priority 1: CoT (Cursor on Target) - Highest Value, Lowest Complexity

**What:** XML-based standard for "What, Where, When." The [heartbeat of the TAK ecosystem](https://hackaday.com/2022/09/08/the-tak-ecosystem-military-coordination-goes-open-source/).

**Example:**
```xml
<event version="2.0" uid="ATAK-Device-1" type="a-f-G-U-C"
       time="2024-01-15T14:30:00Z" start="2024-01-15T14:30:00Z">
  <point lat="32.1234" lon="-110.5678" hae="100" ce="10" le="10"/>
  <detail>
    <contact callsign="ALPHA-1"/>
    <status battery="85"/>
  </detail>
</event>
```

**Output Schema: `cot_tracks`**

| Column | Source | Description |
|--------|--------|-------------|
| uid | event/@uid | Unique device ID |
| callsign | detail/contact/@callsign | Human-readable name |
| event_type | event/@type | MIL-STD-2525 symbol code |
| lat | point/@lat | Latitude |
| lon | point/@lon | Longitude |
| altitude | point/@hae | Height above ellipsoid |
| timestamp | event/@time | Event time |
| battery | detail/status/@battery | Device battery % |

**Implementation:** Standard `xml.etree` or `lxml`. No special libraries needed.

**Why Priority 1:**
- Largest user base (500K+ TAK users)
- Simple XML parsing
- Immediate value (mission replay, track analysis)
- Unclassified samples available (ATAK-CIV community)

### 4.2 Priority 2: PCAP Network Capture - High Value, Low-Medium Complexity

**What:** Packet capture files from network analysis tools (Wireshark, tcpdump). Standard format across military, enterprise IT, and cybersecurity.

**Why PCAP is Strategic:**
1. **Bridge market:** Defense analysts AND enterprise IT/SOC teams use PCAP
2. **Incident response:** Every security incident generates PCAPs
3. **Air-gapped environments:** PCAPs accumulate on disconnected systems for later analysis
4. **Existing pain:** Wireshark is interactive-only; no good batch analysis pipeline

**Example Use Cases:**
- Post-mission network traffic analysis
- Forensic investigation of compromised systems
- Malware communication pattern detection
- Protocol compliance verification

**Output Schema: `pcap_flows`**

| Column | Source | Description |
|--------|--------|-------------|
| flow_id | Generated | Unique flow identifier (5-tuple hash) |
| src_ip | IP header | Source IP address |
| dst_ip | IP header | Destination IP address |
| src_port | TCP/UDP header | Source port |
| dst_port | TCP/UDP header | Destination port |
| protocol | IP header | Protocol (TCP, UDP, ICMP) |
| first_seen | Packet timestamp | First packet in flow |
| last_seen | Packet timestamp | Last packet in flow |
| packet_count | Aggregated | Number of packets in flow |
| byte_count | Aggregated | Total bytes transferred |
| flags | TCP header | TCP flags seen (SYN, FIN, RST) |

**Output Schema: `pcap_dns`**

| Column | Source | Description |
|--------|--------|-------------|
| timestamp | Packet | Query/response time |
| query_name | DNS payload | Domain queried |
| query_type | DNS payload | A, AAAA, MX, TXT, etc. |
| response_ip | DNS payload | Resolved IP (if response) |
| src_ip | IP header | Querying host |

**Output Schema: `pcap_http`**

| Column | Source | Description |
|--------|--------|-------------|
| timestamp | Packet | Request time |
| method | HTTP | GET, POST, etc. |
| host | HTTP header | Target host |
| uri | HTTP | Request URI |
| user_agent | HTTP header | Browser/client string |
| response_code | HTTP | Status code |
| content_type | HTTP header | Response MIME type |

**Implementation:** [scapy](https://scapy.net/) or [pyshark](https://github.com/KimiNewt/pyshark) (Wireshark wrapper)

**Why Priority 2:**
- Bridges defense + enterprise IT markets (dual-use)
- Well-documented format, mature libraries
- Immediate value (flow analysis, DNS visibility, HTTP inspection)
- Complements NITF/CoT for full tactical picture (network + geospatial + tracks)

### 4.3 Priority 3: NITF Metadata - High Value, Medium Complexity

**What:** [National Imagery Transmission Format](https://en.wikipedia.org/wiki/National_Imagery_Transmission_Format). Standard for satellite/aerial imagery. Used by NGA, NRO, all imagery intel.

**The Insight:** Analysts rarely need pixel data. They need **metadata**:
- When was this image taken?
- What area does it cover (lat/lon bounding box)?
- What sensor captured it?
- What's the classification?

**Strategy:** Read headers only. Skip image segments (too large, too slow).

**Output Schema: `nitf_images`**

| Column | Source | Description |
|--------|--------|-------------|
| image_id | IID1 | Image identifier |
| timestamp | IDATIM | Image date/time |
| sensor | ISORCE | Sensor/platform |
| classification | ISCLAS | Security classification |
| corner_ul_lat | IGEOLO[0:6] | Upper-left latitude |
| corner_ul_lon | IGEOLO[6:13] | Upper-left longitude |
| corner_ur_lat | IGEOLO[13:19] | Upper-right latitude |
| ... | ... | Bounding box corners |
| file_path | - | Source file |

**Implementation:** [GDAL Python bindings](https://gdal.org/en/stable/drivers/raster/nitf.html) (industry standard).

**Why Priority 2:**
- Critical for GEOINT workflows
- GDAL is mature, well-documented
- NASA provides unclassified NITF samples
- Immediate value (image cataloging, coverage analysis)

### 4.4 Priority 4: STANAG 4609 KLV - High Value, Medium Complexity

**What:** [Metadata embedded in Full Motion Video](https://impleotv.com/2025/03/11/stanag-4609-isr-video/) from drones (Predator, Reaper, Gray Eagle).

**The Insight:** We don't play the video. We extract the telemetry:
- Where was the camera pointing?
- What was the drone's position?
- What time was this frame captured?

**Output Schema: `fmv_telemetry`**

| Column | KLV Tag | Description |
|--------|---------|-------------|
| timestamp | Tag 2 | Precision timestamp |
| platform_lat | Tag 13 | Aircraft latitude |
| platform_lon | Tag 14 | Aircraft longitude |
| platform_alt | Tag 15 | Aircraft altitude |
| sensor_lat | Tag 23 | Sensor target latitude |
| sensor_lon | Tag 24 | Sensor target longitude |
| slant_range | Tag 21 | Distance to target |
| frame_center_lat | Tag 23 | Frame center latitude |
| frame_center_lon | Tag 24 | Frame center longitude |

**Implementation:** [klvdata Python library](https://pypi.org/project/klvdata/).

**Why Priority 3:**
- High-value for ISR analysts
- Python library exists
- Clear output schema (MISB 0601 standard)

### 4.5 Future: VMF/USMTF/Link 16 - High Complexity, Defer

**What:** Binary tactical data link formats. [MIL-STD-6017 (VMF)](https://en.wikipedia.org/wiki/Variable_Message_Format), MIL-STD-6016 (Link 16), MIL-STD-6040 (USMTF).

**Reality Check:** These are **extremely complex**:
- Thousands of message types
- Binary encoding with variable-length fields
- Classified specifications
- Requires domain expertise to parse correctly

**Recommendation:** Defer to Phase 3+. Focus on CoT/NITF/KLV first to prove value and build credibility.

### 4.6 Also Consider: GeoTIFF, GeoJSON, KML/KMZ

| Format | Use Case | Complexity |
|--------|----------|------------|
| GeoTIFF | Commercial/civil geospatial imagery | Low (GDAL) |
| GeoJSON | TAK ecosystem interchange | Trivial |
| KML/KMZ | Google Earth, mission planning exports | Trivial-Low |

**KML/KMZ Deep Dive:**

KML (Keyhole Markup Language) is ubiquitous in military/civilian geospatial workflows:
- **Mission planning exports** from TAK, Google Earth, ArcGIS
- **Route files** (patrol routes, flight paths, convoy tracks)
- **Overlay archives** (historical positions, area of interest)
- **KMZ = ZIP-compressed KML** with embedded imagery/icons

**Output Schema: `kml_placemarks`**

| Column | Source | Description |
|--------|--------|-------------|
| name | Placemark/name | Feature name |
| description | Placemark/description | Description text |
| lat | coordinates | Latitude |
| lon | coordinates | Longitude |
| altitude | coordinates | Altitude (optional) |
| feature_type | Geometry type | Point, LineString, Polygon |
| style_url | styleUrl | Reference to style |
| folder_path | Folder hierarchy | /Mission/Alpha/Points |
| timestamp | TimeStamp/when | Time (if present) |

**Output Schema: `kml_paths`**

| Column | Source | Description |
|--------|--------|-------------|
| name | Placemark/name | Path name |
| coordinates_json | LineString/coordinates | Full coordinate array |
| point_count | Calculated | Number of points in path |
| total_distance_m | Calculated | Path length in meters |
| start_point | First coordinate | Starting lat/lon |
| end_point | Last coordinate | Ending lat/lon |

**Implementation:** Standard XML parsing (`xml.etree`) or [pykml](https://pythonhosted.org/pykml/)

**Why KML matters:**
- Everyone has KML files (Google Earth is universal)
- Natural bridge between civilian and military workflows
- Useful for "show me everywhere we've been" type queries

These are easy wins that complement the core formats.

---

## 5. Competitive Positioning

### 5.1 The Landscape

| Player | What They Do | Price | Gap |
|--------|--------------|-------|-----|
| [Palantir](https://www.cnbc.com/2025/08/01/palantir-lands-10-billion-army-software-and-data-contract.html) | AI/ML, link analysis, decision support | $10B contract | Requires structured data as INPUT |
| ArcGIS Enterprise | Mapping, geospatial analysis | $100K+/year | Heavy, requires server |
| Custom scripts | Ad-hoc Python parsing | "Free" | Brittle, no governance |
| **Casparian** | Raw file → structured data | $X | Enables everything downstream |

### 5.2 Correct Positioning

**Wrong:** "We compete with Palantir."

**Right:** "We are upstream of Palantir."

```
Raw Files → [CASPARIAN] → Structured Data → [Palantir/ArcGIS/Any Tool]
              ↑                                        ↑
         WE ARE HERE                          THEY ARE HERE
```

**Value Proposition:**
> "Palantir makes sense of data you already have structured. Casparian structures the raw data so you can use ANY tool downstream—including Palantir."

### 5.3 Why We Win at the Tactical Edge

| Requirement | Palantir | ArcGIS | Casparian |
|-------------|----------|--------|-----------|
| Runs on laptop | ❌ | ❌ | ✅ |
| Works offline | ❌ | Limited | ✅ |
| Single-file deploy | ❌ | ❌ | ✅ |
| No license server | ❌ | ❌ | ✅ |
| Memory-safe (Rust) | ❌ | ❌ | ✅ |
| Analyst can modify parsers | ❌ | ❌ | ✅ |

---

## 6. Architecture: Air-Gapped Deployment

### 6.1 The "Sneakernet" Requirement

In a SCIF, there is no `pip install`. Software arrives on approved media.

**Feature: `casparian bundle`**

```bash
# On connected system (UNCLASS)
$ casparian bundle --output casparian_edge_v1.0.zip

# Creates self-contained archive:
# - Rust binary (statically linked)
# - Python runtime (embedded)
# - All parsers (~/.casparian_flow/parsers/)
# - All dependencies (vendored wheels)

# On disconnected system (SIPR/JWICS)
$ unzip casparian_edge_v1.0.zip
$ ./casparian scan /mnt/mission_data
```

**Requirements:**
- Static linking (no dynamic library dependencies)
- Vendored Python wheels (no network fetch)
- No telemetry, no phone-home, no license checks
- Works on RHEL 7/8 (common DoD baseline)

### 6.2 Security Posture

| Control | Implementation |
|---------|----------------|
| Memory safety | Rust core (no buffer overflows) |
| Input validation | Strict parsing, quarantine malformed |
| No network access | `--offline` flag disables all network |
| Audit trail | Full lineage in SQLite |
| Sandboxed execution | Bridge Mode (isolated subprocess) |

### 6.3 Classification Levels

| Level | Network | Deployment |
|-------|---------|------------|
| UNCLASS | Internet | Normal install |
| CUI/FOUO | Isolated | Bundle deploy |
| SECRET (SIPRNet) | Air-gapped | Bundle + security review |
| TS/SCI (JWICS) | Air-gapped | Bundle + TS review |

**Note:** Higher classification levels require security reviews and potentially ATO (Authority to Operate). This is a business/compliance matter, not a technical one.

---

## 7. Go-to-Market Strategy

### 7.1 Target Organizations (Priority Order)

| Organization | Why | Contact Path |
|--------------|-----|--------------|
| **AFWERX** | Air Force innovation arm, fast procurement | SBIR/STTR |
| **DIU** | Defense Innovation Unit, commercial solutions | Other Transaction (OT) |
| **Army Futures Command** | Modernization, JADC2 | SBIR/STTR |
| **SOCOM** | Special ops, tactical edge focus | [SOCOM SBIR](https://team-80.com/blog/socom-sbir/) |
| **NGA** | Geospatial intel, NITF expertise | Contractor relationships |

### 7.2 SBIR/STTR Pathway (Primary)

> **Full Analysis:** See [strategies/dod_sbir_opportunities.md](dod_sbir_opportunities.md) for detailed topic research.

**Program Status (as of Jan 2026):** SBIR/STTR programs expired Sept 30, 2025 and await congressional reauthorization. Expected resolution by late January 2026.

| Phase | Funding | Duration | Goal |
|-------|---------|----------|------|
| Phase I | $50-250K | 6-12 months | Feasibility study |
| Phase II | $750K-2M | 18-24 months | Prototype development |
| Phase III | Unlimited | Ongoing | Production/deployment |

**Best-Fit Topic Identified:**

| Topic | Agency | Fit | Status |
|-------|--------|-----|--------|
| **A254-011: AI for Interoperability** | Army | ⭐⭐⭐ | Closed Feb 2025; expect similar |
| GenAI Enabled Tactical Network | Army | ⭐⭐⭐ | Closed Mar 2025 |
| xTechOverwatch Open Topic | Army | ⭐⭐ | Finals Oct 2025 |
| Space Force Data Analytics | USSF | ⭐ | Delayed pending reauth |

**Why A254-011 is Perfect Fit:**
> "Apply LLMs and AI to support warfighter system integrations... data unification **regardless of target system, source system, or data format**, with focus on **tactical environments**."

This is almost a verbatim description of Casparian's capabilities.

**Immediate Actions:**
1. Register in [SAM.gov](https://sam.gov) (required for federal contracting)
2. Register in [DSIP Portal](https://www.dodsbirsttr.mil)
3. Prepare capability statement (2-page PDF)
4. Monitor [defensesbirsttr.mil](https://www.defensesbirsttr.mil) weekly for reauthorization news
5. Draft technical approach for interoperability-focused topic

### 7.3 Pricing Strategy - Value-Based

> **Pricing Philosophy:** Defense buyers expect enterprise pricing. Low prices signal "not serious." See [STRATEGY.md](../STRATEGY.md#value-based-pricing-strategy) for framework.

#### Value Analysis

**The reality:** No laptop-deployable, air-gapped data structuring tool exists. The alternatives are:
- **Palantir:** $10B Army contract; requires server infrastructure
- **ArcGIS Enterprise:** $100K+/year; requires server
- **Custom Python:** "Free" but brittle, no governance, single point of failure
- **Manual analysis:** Hours per file; analysts overwhelmed

**Casparian's unique value:**
1. **Runs on laptop** - No server, no cloud, no IT dependency
2. **Works air-gapped** - Critical for DDIL environments
3. **Memory-safe (Rust)** - Meets NSA/CISA guidance
4. **Analyst-modifiable** - No contractor dependency for parser updates
5. **SQL output** - Works with any downstream tool

**Value created:** Mission-critical capability where no alternative exists. Value is effectively infinite for the right use case.

#### Pricing Tiers

| Tier | Price | Value Capture | Features | Target |
|------|-------|---------------|----------|--------|
| **Open Source** | Free | N/A | Core parsers, CLI, community support | Evaluation, ATAK-CIV users |
| **Tactical** | $50,000/deployment/year | ~5% | Air-gapped bundle, 5 formats (CoT, PCAP, NITF, KML, KLV), 8x5 support | Single-site tactical |
| **Mission** | $150,000/deployment/year | ~10% | All formats, 24x7 support, custom parser development, on-site training | Multi-format, high-criticality |
| **Program** | $500,000+/year | Custom | Multi-site, program-level support, SBIR Phase III, dedicated team | Major programs, IDIQ |

#### Pricing Justification

**Tactical tier ($50,000/year):**
- Palantir alternative: $1M+/year and can't run on laptop
- ArcGIS Enterprise: $100K+/year and requires server
- $50K is **5-10% of alternative cost** while providing unique capability
- Defense budgets allocate millions for data tools; $50K is noise

**Mission tier ($150,000/year):**
- Includes 24x7 support (critical for deployed operations)
- Custom parser development for unit-specific formats
- On-site training for analysts
- This is what enterprise defense contracts look like

**Program tier ($500K+):**
- Multi-site deployments (multiple TOCs, ships, aircraft)
- Dedicated Casparian team for the program
- Natural fit for SBIR Phase III or OT contract

#### Why Not Price Lower?

**$100/user/month signals "consumer tool":**
1. DoD procurement officers won't take it seriously
2. No budget for dedicated support at that price
3. No budget for security reviews/ATO assistance
4. Competitors will use low price against you ("they can't be serious")

**$50K/year is still cheap for defense:**
- 0.05% of a $100M program budget
- 0.5% of typical annual IT spend for a unit
- Rounding error compared to contractor costs

#### Revenue Projection (Defense Vertical)

| Metric | Year 1 | Year 2 | Year 3 |
|--------|--------|--------|--------|
| SBIR Phase I awards | 1 ($150K) | - | - |
| SBIR Phase II awards | - | 1 ($1.5M) | - |
| Tactical deployments | 2 | 8 | 20 |
| Mission deployments | 0 | 2 | 5 |
| Program contracts | 0 | 0 | 1 |
| **Defense ARR** | **$250K** | **$1.0M** | **$3.5M** |

Note: SBIR funding counted separately from commercial ARR.

### 7.4 Compliance Roadmap

| Milestone | Timeline | Purpose |
|-----------|----------|---------|
| FedRAMP Ready | 6-12 months | Cloud offering baseline |
| IL4 | 12-18 months | CUI/FOUO data |
| IL5 | 18-24 months | Unclassified national security |
| IL6+ | Future | Classified (requires sponsor) |

---

## 8. The "Killer App" Use Case

### Scenario: Mission Planning with Historical Data

A Mission Planner needs to route a convoy. They have a hard drive containing:
- 50,000 CoT files from previous patrols
- 5,000 NITF files from satellite/drone imagery
- 100 hours of FMV with KLV metadata

**Without Casparian:**
1. Open Google Earth, try to drag files in
2. Crash after 500 files
3. Open files one by one in specialized viewers
4. Miss critical patterns because data is siloed
5. Fly blind

**With Casparian:**
1. `casparian scan /mnt/mission_data --recursive`
2. Wait 5 minutes (50K files processed)
3. Query:
```sql
-- Find safe routes (places we drove before without incident)
SELECT lat, lon, COUNT(*) as passage_count
FROM cot_tracks
WHERE event_type LIKE 'a-f-G%'  -- Friendly ground units
  AND timestamp > date('now', '-30 days')
GROUP BY ROUND(lat, 3), ROUND(lon, 3)
HAVING passage_count > 5;
```
4. Export to KML: `casparian export --format kml --output safe_routes.kml`
5. Open in Google Earth or TAK
6. Plan route with confidence

**Time saved:** Hours → Minutes. **Lives potentially saved:** Incalculable.

---

## 9. Implementation Roadmap

### Phase 1: CoT Parser (Month 1-2)

- [ ] Implement `defense_cot.py` parser
- [ ] Handle single file and directory of CoT XML
- [ ] Output: `cot_tracks` table
- [ ] Test with ATAK-CIV sample data
- [ ] KML/GeoJSON export

### Phase 1b: KML/KMZ Parser (Month 1-2, parallel)

- [ ] Implement `defense_kml.py` parser
- [ ] Handle both KML and KMZ (compressed) formats
- [ ] Output: `kml_placemarks`, `kml_paths` tables
- [ ] Test with Google Earth exports
- [ ] Path distance calculation

### Phase 2: PCAP Parser (Month 2-3) ⭐ HIGH PRIORITY

- [ ] Implement `defense_pcap.py` parser
- [ ] scapy or pyshark integration
- [ ] Output: `pcap_flows`, `pcap_dns`, `pcap_http` tables
- [ ] Test with public PCAP samples (Wireshark wiki)
- [ ] Flow aggregation and statistics
- [ ] **Bonus:** This parser works for enterprise IT/SOC market too

### Phase 3: NITF Metadata Parser (Month 3-4)

- [ ] Implement `defense_nitf.py` parser
- [ ] GDAL-based header extraction (no pixel read)
- [ ] Output: `nitf_images` table with bounding boxes
- [ ] Test with NASA/USGS sample NITF files
- [ ] Spatial query support (find images covering lat/lon)

### Phase 4: STANAG 4609 KLV Parser (Month 4-5)

- [ ] Implement `defense_klv.py` parser
- [ ] klvdata library integration
- [ ] Output: `fmv_telemetry` table
- [ ] Flight path reconstruction
- [ ] Time-sync with video frames

### Phase 5: Bundle & Air-Gap Support (Month 5-6)

- [ ] `casparian bundle` command
- [ ] Static binary compilation
- [ ] Vendored Python dependencies
- [ ] Offline mode (`--offline` flag)
- [ ] Test on RHEL 7/8

### Phase 6: SBIR Application (Month 6+)

- [ ] Identify relevant SBIR topics
- [ ] Prepare Phase I proposal
- [ ] Develop demo for DoD stakeholders
- [ ] Build relationships with target organizations

### Future Phases

- [ ] VMF/USMTF parsing (complex, requires domain expertise)
- [ ] Link 16 message parsing
- [ ] Real-time CoT stream ingestion
- [ ] TAK Server integration
- [ ] FedRAMP/IL4+ certification

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| SBIR application rejected | High | Multiple submissions, parallel OT track |
| Classification barriers | High | Start with unclassified use cases |
| DoD sales cycle too long | High | Focus on SBIR/OT (faster than traditional) |
| Palantir adds similar features | Medium | Focus on edge/disconnected (not their strength) |
| Format complexity underestimated | Medium | Start simple (CoT), build expertise |
| ATO process delays | Medium | Document security posture early |

---

## 11. Success Metrics

### 11.1 Technical Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| CoT files processed | 1M | 10M |
| NITF files cataloged | 100K | 1M |
| Parse success rate | >99% | >99.5% |
| Bundle size | <100MB | <100MB |

### 11.2 Business Metrics

| Metric | 6-Month Target | 12-Month Target |
|--------|----------------|-----------------|
| SBIR Phase I | 1 application | 1 award |
| DoD pilot users | 5 | 25 |
| Defense MRR | $0 | $10K |
| Government contracts | 0 | 1 |

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Codename | "Casparian Edge" | Avoid conflict with Microsoft Sentinel and internal Sentinel |
| Palantir positioning | Upstream, not competitor | Different value prop (structuring vs. analysis) |
| **Format priority** | **CoT → PCAP → NITF → KLV** | PCAP bridges defense + enterprise IT; dual-use value |
| KML/KMZ | Added Phase 1b | Trivial to implement; universal format; bridges civilian/military |
| PCAP library | scapy or pyshark | Both mature; pyshark wraps Wireshark; scapy more flexible |
| VMF/USMTF | Defer to Phase 3+ | Too complex for initial entry |
| GTM pathway | SBIR primary | Fastest route for small company into DoD |
| International | Defer | ITAR/EAR compliance required |
| NITF library | GDAL | Industry standard, well-documented |

---

## 13. Open Questions

1. **Sample data access:** Where to get realistic (unclassified) CoT/NITF/KLV samples for development?
2. ~~**SBIR topic timing:** When are relevant topics opening for FY2026?~~ **ANSWERED:** Program expired Sept 30, 2025; awaiting reauthorization (expected late Jan 2026). Best-fit topics identified in [dod_sbir_opportunities.md](dod_sbir_opportunities.md).
3. **ATO sponsor:** Who would sponsor an ATO for classified networks?
4. **TAK integration:** Direct plugin vs. file-based integration?
5. **FedRAMP timing:** When does this become a blocker?

---

## 14. Glossary

| Term | Definition |
|------|------------|
| **ATAK** | Android Tactical Assault Kit - mobile situational awareness |
| **ATO** | Authority to Operate - security accreditation |
| **CoT** | Cursor on Target - XML position/track format |
| **DDIL** | Denied, Disrupted, Intermittent, Limited - connectivity constraints |
| **DIU** | Defense Innovation Unit |
| **FMV** | Full Motion Video - drone/aircraft video feeds |
| **GEOINT** | Geospatial Intelligence |
| **IL4/5/6** | Impact Level - DoD security classification tiers |
| **JADC2** | Joint All-Domain Command and Control |
| **KLV** | Key-Length-Value - metadata encoding in video |
| **NITF** | National Imagery Transmission Format |
| **OT** | Other Transaction - flexible procurement vehicle |
| **SBIR** | Small Business Innovation Research |
| **SCIF** | Sensitive Compartmented Information Facility |
| **STANAG** | Standardization Agreement (NATO) |
| **TAK** | Tactical Assault Kit (umbrella for ATAK/iTAK/WinTAK) |
| **VMF** | Variable Message Format |

---

## 15. References

- [GDAL NITF Driver](https://gdal.org/en/stable/drivers/raster/nitf.html)
- [TAK Ecosystem Overview](https://hackaday.com/2022/09/08/the-tak-ecosystem-military-coordination-goes-open-source/)
- [TAK Evolution - Breaking Defense](https://breakingdefense.com/2025/11/evolution-and-future-of-the-tactical-assault-kit-for-soldiers-and-special-operators/)
- [klvdata Python Library](https://pypi.org/project/klvdata/)
- [STANAG 4609 Overview](https://impleotv.com/2025/03/11/stanag-4609-isr-video/)
- [NSA Memory Safety Guidance](https://www.theregister.com/2025/06/27/cisa_nsa_call_formemory_safe_languages/)
- [DoD SBIR Portal](https://www.defensesbirsttr.mil/SBIR-STTR/Opportunities/)
- [Palantir Army Contract](https://www.cnbc.com/2025/08/01/palantir-lands-10-billion-army-software-and-data-contract.html)
- [DDIL Edge Computing](https://fedtechmagazine.com/article/2025/03/ddil-environments-managing-cloud-edge-computing-defense-agencies-perfcon)
- [Variable Message Format](https://en.wikipedia.org/wiki/Variable_Message_Format)
- [scapy - Packet Manipulation](https://scapy.net/)
- [pyshark - Wireshark Python Wrapper](https://github.com/KimiNewt/pyshark)
- [Wireshark Sample Captures](https://wiki.wireshark.org/SampleCaptures)
- [pykml - KML Library](https://pythonhosted.org/pykml/)

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft based on research and analysis |
| 2026-01-08 | 0.2 | Gap analysis integration: Added PCAP parser (Priority 2); Enhanced KML/KMZ section with schemas; Updated format priority and roadmap |
| 2026-01-08 | 0.3 | **SBIR research:** Added detailed SBIR/STTR section with best-fit topics (A254-011, GenAI Tactical Network); Created companion doc [dod_sbir_opportunities.md](dod_sbir_opportunities.md); Noted program expiration and reauthorization status |


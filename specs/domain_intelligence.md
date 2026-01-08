# Domain Intelligence: File Format Catalog

**Status:** Draft
**Purpose:** Guide UI/API design based on where and how data files are stored across verticals
**Version:** 0.2
**Date:** January 8, 2026

---

## 1. Executive Summary

This document catalogs file formats across all target verticals with specific focus on:
- **Where** files live (path patterns, storage types)
- **How** they're named (conventions, timestamps)
- **Value ranking** per vertical (what to prioritize)
- **Detection signatures** (auto-discovery patterns)

**Goal:** Enable smart UI features like:
- "Looks like a hospital IT share - scan for HL7?"
- "Found MTConnect XML - suggest manufacturing parser"
- "QuickBooks IIF detected - process accounting data?"

---

## 2. Storage Archetypes

### 2.1 The "Network Share Graveyard" (Most Common)

| Characteristic | Description |
|----------------|-------------|
| **Protocol** | SMB (Windows), NFS (Linux) |
| **Path Pattern** | `\\server-name\share-name\...` or `/mnt/share/...` |
| **Who Uses** | Healthcare (90%), Manufacturing (80%), Finance (60%), Mid-biz (70%) |
| **Challenges** | Network latency, SMB flakiness, no inotify (polling required) |
| **File Counts** | 100K to millions of small files |

**Detection Hint:** Path contains `\\` or starts with `/mnt/`, `/nas/`, `/share/`

### 2.2 The "Shadow IT Download Dump"

| Characteristic | Description |
|----------------|-------------|
| **Protocol** | Local filesystem |
| **Path Pattern** | `C:\Users\{name}\Downloads\` or `~/Downloads/` or `~/Desktop/` |
| **Who Uses** | Mid-biz (primary), Healthcare analysts, Finance quants |
| **Challenges** | Chaotic naming, mixed formats, duplicates, zip files |
| **File Counts** | 10s to 1000s of files |

**Detection Hint:** Path contains `Downloads`, `Desktop`, or user home directory

### 2.3 The "Disconnected Tactical Drive"

| Characteristic | Description |
|----------------|-------------|
| **Protocol** | Local or USB-mounted |
| **Path Pattern** | `/mnt/mission_data/`, `D:\mission\`, `/media/usb/` |
| **Who Uses** | Defense (100%), Air-gapped manufacturing |
| **Challenges** | No network, intermittent connectivity, sneakernet |
| **File Counts** | 10K to 100K+ files |

**Detection Hint:** Path contains `mission`, `tactical`, `classified`, or mounted USB path

### 2.4 The "Cloud Object Store" (Emerging)

| Characteristic | Description |
|----------------|-------------|
| **Protocol** | S3, Azure Blob, GCS |
| **Path Pattern** | `s3://bucket/prefix/`, `az://container/`, `gs://bucket/` |
| **Who Uses** | Modern finance (20%), Forward-thinking healthcare (10%) |
| **Challenges** | Requires credentials, egress costs, not Day 1 priority |
| **File Counts** | Variable |

**Detection Hint:** URI scheme `s3://`, `az://`, `gs://`

---

## 3. Format Catalog by Vertical

### 3.1 Healthcare IT

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **HL7 v2.x ADT** | `.hl7`, `.txt` | ⭐⭐⭐⭐⭐ | `\\hospital-nas\interface_archives\ADT_*\` | `YYYYMMDD_HH.hl7` | Line starts with `MSH\|` |
| **HL7 v2.x ORU** | `.hl7`, `.txt` | ⭐⭐⭐⭐⭐ | `\\hospital-nas\interface_archives\ORU_*\` | `YYYYMMDD_HH.hl7` | Line starts with `MSH\|`, contains `ORU^R01` |
| **HL7 v2.x ORM** | `.hl7`, `.txt` | ⭐⭐⭐⭐ | `\\hospital-nas\interface_archives\ORM_*\` | `YYYYMMDD_HH.hl7` | Line starts with `MSH\|`, contains `ORM^O01` |
| **Shadow IT Zip** | `.zip` | ⭐⭐⭐ | `\\research-share\Dr_*\` | `data_dump*.zip` | Contains `.hl7` files inside |

**Folder Structure Pattern:**
```
\\hospital-nas-01\interface_archives\
├── ADT_Inbound\
│   └── {YYYY}\{MM}\{YYYYMMDD_HH}.hl7
├── ADT_Outbound\
├── ORU_Inbound\
└── ORU_Outbound\
```

**UI Recommendations:**
- Suggest year/month drill-down navigation
- Show message type distribution (ADT vs ORU vs ORM)
- Handle encoding detection (chardet) due to legacy systems
- Warn about SMB timeout risks

---

### 3.2 Defense/Tactical

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **CoT (Cursor on Target)** | `.cot`, `.xml` | ⭐⭐⭐⭐⭐ | `/mnt/mission_data/tracks/` | `patrol_*.cot` | Root element `<event>` with `uid`, `type` attrs |
| **PCAP (Network Capture)** | `.pcap`, `.pcapng` | ⭐⭐⭐⭐⭐ | `/mnt/mission_data/network/` | `capture_*.pcap` | Magic bytes: `0xd4c3b2a1` (pcap) or `0x0a0d0d0a` (pcapng) |
| **NITF (Imagery)** | `.ntf`, `.nitf` | ⭐⭐⭐⭐⭐ | `/mnt/mission_data/imagery/` | `img_*.ntf` | Magic bytes: `NITF` or `NSIF` at offset 0 |
| **STANAG 4609 Video** | `.ts`, `.mpg` | ⭐⭐⭐⭐ | `/mnt/mission_data/fmv/` | `mission_*.ts` | KLV metadata in MPEG-TS stream |
| **KML/KMZ** | `.kml`, `.kmz` | ⭐⭐⭐⭐ | `/mnt/mission_data/exports/` | `route_*.kml` | Root element `<kml>` or ZIP containing .kml |
| **GeoJSON** | `.geojson`, `.json` | ⭐⭐⭐ | `/mnt/mission_data/exports/` | `tracks_*.geojson` | Contains `"type": "FeatureCollection"` |

**PCAP Special Notes:**
- **Bridges defense + enterprise IT** - Same format used by SOC analysts
- PCAP files accumulate on air-gapped systems for post-mission analysis
- Contains network flows, DNS queries, HTTP traffic
- Wireshark is interactive-only; no good batch pipeline exists

**Folder Structure Pattern:**
```
/mnt/mission_data/
├── imagery/
│   └── {YYYYMMDD}_satellite_pass/
│       └── img_*.ntf
├── tracks/
│   └── patrol_*.cot
├── network/
│   └── capture_*.pcap         # NEW: Network captures
├── fmv/
│   └── mission_*.ts
└── reports/
    └── sitrep_*.txt
```

**UI Recommendations:**
- Mission-centric navigation (group by date/operation)
- Map preview for CoT/NITF (show bounding box)
- Offline-first design (no network calls)
- Security classification indicator (if detectable from metadata)
- PCAP: Show flow counts, top talkers summary

---

### 3.3 Financial Services

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **FIX Protocol Logs** | `.log`, `.fix` | ⭐⭐⭐⭐⭐ | `/var/log/fix/` | `gateway_YYYYMMDD.log` | Contains `8=FIX.4.*\|9=` |
| **SEC EDGAR XBRL** | `.xml`, `.htm` | ⭐⭐⭐⭐⭐ | `~/Downloads/edgar/` or API | `{ticker}-{date}_htm.xml` | Contains XBRL namespace |
| **ISO 20022 (MX)** | `.xml` | ⭐⭐⭐⭐ | Export from payment system | `pacs_*.xml`, `camt_*.xml` | Root element in `urn:iso:std:iso:20022:` namespace |
| **Alternative Data CSV** | `.csv`, `.parquet` | ⭐⭐⭐⭐ | `~/data/vendors/` | `{vendor}_YYYYMMDD.csv` | Vendor-specific columns |
| **Trade Blotter** | `.csv`, `.xlsx` | ⭐⭐⭐ | `\\trading\reports\` | `blotter_YYYYMMDD.xlsx` | Contains trade columns (symbol, qty, price) |

**Folder Structure Pattern:**
```
/var/log/fix/
├── gateway_YYYYMMDD.log        # Inbound FIX
├── execution_YYYYMMDD.log      # Fills
└── drop_copy_YYYYMMDD.log      # Regulatory

~/data/edgar/
├── {CIK}/
│   └── {filing_type}/
│       └── {accession_number}/
```

**UI Recommendations:**
- FIX: Message type breakdown (D=NewOrder, 8=ExecReport, etc.)
- EDGAR: Company picker with CIK lookup
- ISO 20022: Message family selector (pacs, pain, camt)
- Time-range selection critical for FIX logs

---

### 3.4 Manufacturing

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **Historian Export** | `.csv`, `.parquet` | ⭐⭐⭐⭐⭐ | `\\plant-share\engineering\data_exports\` | `line_*_YYYYMM.csv` | Contains tag names like `LI-1234.PV` |
| **MTConnect XML** | `.xml` | ⭐⭐⭐⭐⭐ | `/var/mtconnect/` or HTTP | `streams_*.xml` | Root element `<MTConnectStreams>` |
| **SCADA Alarms** | `.csv` | ⭐⭐⭐⭐ | `/var/scada_exports/` | `alarms_YYYYMMDD.csv` | Columns: timestamp, tag, state, priority |
| **SPC/Quality** | `.csv`, `.xlsx` | ⭐⭐⭐⭐ | `\\quality-share\spc_data\` | `cmm_*_YYYYMMDD.csv` | Columns: actual, nominal, tolerance |
| **Batch Records** | `.xml` | ⭐⭐⭐ | `/var/scada_exports/` | `batch_*.xml` | Contains batch phases, steps |
| **Downtime Logs** | `.csv` | ⭐⭐⭐ | `\\plant-share\downtime\` | `downtime_YYYYMM.csv` | Columns: start, end, reason_code |

**Folder Structure Pattern:**
```
\\plant-share\engineering\data_exports\
├── line_1_temperatures_YYYYMM.csv
├── line_2_pressures_YYYYMM.csv
├── quality_spc_weekly.xlsx
└── oee_report_YYYYMM.parquet

\\quality-share\spc_data\
├── cmm_measurements_YYYYMMDD.csv
├── control_charts_line_*.xlsx
└── capability_studies/
```

**UI Recommendations:**
- Tag browser with alias mapping
- Time-series preview chart
- Quality: Show Cp/Cpk indicators
- Handle cryptic PI tag names (offer alias file upload)

---

### 3.5 Mid-Size Business

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **QuickBooks IIF** | `.iif` | ⭐⭐⭐⭐⭐ | `C:\Users\*\Downloads\` | `chart_of_accounts.iif` | Starts with `!ACCNT` or `!TRNS` |
| **QuickBooks CSV** | `.csv` | ⭐⭐⭐⭐⭐ | `C:\Users\*\Downloads\` | `qb_*.csv` | Contains QB column names |
| **Salesforce Export** | `.csv` | ⭐⭐⭐⭐ | `C:\Users\*\Desktop\CRM_Data\` | `sf_*.csv` | Contains `Id` column (18-char SF ID) |
| **NetSuite Export** | `.csv`, `.xlsx` | ⭐⭐⭐⭐ | `\\server\finance\netsuite\` | `saved_search_*.csv` | Contains NetSuite internal IDs |
| **ADP Payroll** | `.csv` | ⭐⭐⭐⭐ | `\\server\hr\payroll\` | `adp_payroll_YYYYMM.csv` | Contains payroll columns |
| **Generic Excel** | `.xlsx` | ⭐⭐⭐ | Everywhere | Variable | Requires schema inference |

**Folder Structure Pattern:**
```
C:\Users\Controller\Downloads\
├── qb_trial_balance_YYYYMM.xlsx
├── qb_ar_aging_YYYYMM.csv
├── chart_of_accounts.iif
└── journal_entries_YYYYMM.xlsx

\\server\finance\monthly_reports\
├── netsuite\
│   ├── saved_search_*.csv
│   └── financial_report_YYYYMM.xlsx
└── consolidated\
    └── master_workbook.xlsx
```

**UI Recommendations:**
- "What system is this from?" picker (QB, SF, NetSuite, etc.)
- Handle Downloads folder chaos (date-based sorting)
- Multi-company consolidation view
- PII warning for payroll files

---

### 3.6 Legal Tech/eDiscovery

| Format | Extension | Value | Storage Location | Naming Pattern | Detection Signature |
|--------|-----------|-------|------------------|----------------|---------------------|
| **PST (Outlook Archive)** | `.pst`, `.ost` | ⭐⭐⭐⭐⭐ | `\\server\departing_employees\` | `archive*.pst` | Magic bytes: `!BDN` at offset 0 |
| **MBOX (Unix Mail)** | `.mbox`, `.mbx` | ⭐⭐⭐⭐ | `~/mail/` or Google Takeout | `All mail.mbox` | Lines start with `From ` (mbox format) |
| **Load File (.dat)** | `.dat`, `.lfp` | ⭐⭐⭐⭐⭐ | `\\server\productions\` | `production_*.dat` | Uses þ (thorn) as delimiter |
| **OPT File** | `.opt` | ⭐⭐⭐⭐ | `\\server\productions\` | `production_*.opt` | BATES → image path mapping |
| **Slack Export** | `.json` | ⭐⭐⭐⭐ | `~/Downloads/slack_export/` | `channels/*.json` | Contains `"type": "message"` |
| **Teams Export** | `.json` | ⭐⭐⭐ | M365 compliance export | `Messages/*.json` | Contains Teams-specific schema |

**Folder Structure Pattern:**
```
\\fileserver\departing_employees\
├── jsmith_2019/
│   ├── outlook_archive.pst      # 15GB email archive
│   └── desktop_backup.zip
└── mwilliams_2020/
    └── archive_2015_2020.pst

\\server\productions\
├── production_001/
│   ├── DATA/
│   │   └── production_001.dat   # Tab-delimited metadata
│   ├── IMAGES/
│   │   └── 0001/
│   │       └── DOC0001_*.tif
│   ├── NATIVES/
│   │   └── *.pdf, *.docx
│   └── TEXT/
│       └── *.txt

slack_export_YYYYMMDD/
├── channels/
│   └── general/
│       └── YYYY-MM-DD.json
├── users.json
└── channels.json
```

**UI Recommendations:**
- Custodian picker (group by person whose data was collected)
- Email thread visualization
- Attachment extraction preview
- BATES number range display for load files
- Slack: Channel/thread navigation

---

## 4. Value Ranking Summary (Cross-Vertical)

### Tier 1: Killer Apps (Ship Day 1)

| Format | Vertical | Why Critical |
|--------|----------|--------------|
| **FIX Logs** | Finance | Trade Break Workbench; quantifiable ROI |
| **HL7 ADT/ORU** | Healthcare | 95% of hospitals use it; Mirth disruption |
| **CoT XML** | Defense | 500K+ TAK users; simple XML |
| **PCAP** | Defense/Enterprise IT | Bridges two markets; Wireshark pipeline gap |
| **Historian CSV** | Manufacturing | Universal export format; PI disruption |
| **QuickBooks IIF/CSV** | Mid-biz | 80%+ market share; export pain is real |

### Tier 2: High Value (Phase 2)

| Format | Vertical | Why Important |
|--------|----------|---------------|
| **PST/MBOX** | Legal | Universal email archive; $5-15K savings per matter |
| **Load Files (.dat)** | Legal | eDiscovery interchange; platform bridge |
| **SEC EDGAR XBRL** | Finance | Free data; Bloomberg alternative |
| **NITF Metadata** | Defense | Critical for GEOINT; GDAL makes it easy |
| **KML/KMZ** | Defense/General | Universal geospatial; bridges civilian/military |
| **MTConnect XML** | Manufacturing | Open standard; Industry 4.0 |
| **Salesforce CSV** | Mid-biz | 60%+ CRM adoption; common combo with QB |
| **HL7 ORM** | Healthcare | Orders data; links to ADT/ORU |

### Tier 3: Valuable (Phase 3+)

| Format | Vertical | Why Defer |
|--------|----------|-----------|
| **Slack/Teams JSON** | Legal | Growing eDiscovery importance; courts require |
| **ISO 20022 (MX)** | Finance | Complex XML; specialized market |
| **STANAG 4609 KLV** | Defense | Video telemetry; niche |
| **SPC/Quality Excel** | Manufacturing | Highly variable formats |
| **NetSuite/Sage** | Mid-biz | More structured; less pain than QB |

### Tier 4: Future/Complex (Not Day 1)

| Format | Vertical | Why Defer |
|--------|----------|-----------|
| **VMF/USMTF** | Defense | Thousands of message types; classified specs |
| **MDF4/BLF** | Automotive | Niche market; specialized tools exist |
| **ASN.1/CDR** | Telecom | Enterprise market; wrong buyer profile |
| **OPC-UA Real-time** | Manufacturing | Requires OT network access |
| **CAT Audit Trail** | Finance | Regulatory complexity; enterprise only |

---

## 5. Detection Patterns for Auto-Discovery

### 5.1 Content Signatures

```python
CONTENT_SIGNATURES = {
    # Healthcare
    "hl7": {
        "pattern": r"^MSH\|",
        "confidence": 0.99,
        "vertical": "healthcare"
    },

    # Defense
    "cot": {
        "pattern": r'<event[^>]+uid="[^"]+"\s+type="[^"]+"',
        "confidence": 0.95,
        "vertical": "defense"
    },
    "nitf": {
        "magic_bytes": [b"NITF", b"NSIF"],
        "confidence": 0.99,
        "vertical": "defense"
    },
    "pcap": {
        "magic_bytes": [b"\xd4\xc3\xb2\xa1", b"\xa1\xb2\xc3\xd4"],  # pcap little/big endian
        "confidence": 0.99,
        "vertical": "defense"  # Also enterprise_it
    },
    "pcapng": {
        "magic_bytes": [b"\x0a\x0d\x0d\x0a"],
        "confidence": 0.99,
        "vertical": "defense"
    },
    "kml": {
        "pattern": r"<kml\s+xmlns=",
        "confidence": 0.95,
        "vertical": "defense"
    },

    # Finance
    "fix": {
        "pattern": r"8=FIX\.\d+\.\d+\|9=\d+\|",
        "confidence": 0.99,
        "vertical": "finance"
    },
    "xbrl": {
        "pattern": r'xmlns[^=]*="[^"]*xbrl[^"]*"',
        "confidence": 0.90,
        "vertical": "finance"
    },
    "iso20022": {
        "pattern": r'xmlns="urn:iso:std:iso:20022:',
        "confidence": 0.95,
        "vertical": "finance"
    },

    # Manufacturing
    "mtconnect": {
        "pattern": r"<MTConnect(Streams|Devices|Assets)",
        "confidence": 0.99,
        "vertical": "manufacturing"
    },
    "historian_tags": {
        "pattern": r"[A-Z]{2,4}-\d{3,5}\.(PV|SP|CV|OP)",
        "confidence": 0.80,
        "vertical": "manufacturing"
    },

    # Mid-biz
    "quickbooks_iif": {
        "pattern": r"^!(ACCNT|TRNS|CUST|VEND)",
        "confidence": 0.99,
        "vertical": "midsize_business"
    },
    "salesforce_id": {
        "pattern": r"[a-zA-Z0-9]{18}",  # In Id column
        "confidence": 0.70,
        "vertical": "midsize_business"
    },

    # Legal/eDiscovery
    "pst": {
        "magic_bytes": [b"!BDN"],  # PST signature
        "confidence": 0.99,
        "vertical": "legal"
    },
    "mbox": {
        "pattern": r"^From \S+@\S+",  # mbox From line
        "confidence": 0.90,
        "vertical": "legal"
    },
    "loadfile_dat": {
        "pattern": r"þ[A-Z]+þ",  # Thorn delimiter with field names
        "confidence": 0.95,
        "vertical": "legal"
    },
    "slack_message": {
        "pattern": r'"type"\s*:\s*"message".*"user"\s*:',
        "confidence": 0.85,
        "vertical": "legal"
    }
}
```

### 5.2 Path-Based Hints

```python
PATH_HINTS = {
    # Healthcare
    r"interface.*archive": {"vertical": "healthcare", "formats": ["hl7"]},
    r"(ADT|ORU|ORM)_(In|Out)bound": {"vertical": "healthcare", "formats": ["hl7"]},

    # Defense
    r"mission_data": {"vertical": "defense", "formats": ["cot", "nitf", "pcap"]},
    r"imagery.*pass": {"vertical": "defense", "formats": ["nitf"]},
    r"tracks": {"vertical": "defense", "formats": ["cot", "kml"]},
    r"network|capture": {"vertical": "defense", "formats": ["pcap"]},

    # Finance
    r"/var/log/fix": {"vertical": "finance", "formats": ["fix"]},
    r"edgar|sec.*filing": {"vertical": "finance", "formats": ["xbrl"]},
    r"trading|execution": {"vertical": "finance", "formats": ["fix"]},

    # Manufacturing
    r"plant.*share|engineering.*export": {"vertical": "manufacturing", "formats": ["historian"]},
    r"scada.*export": {"vertical": "manufacturing", "formats": ["scada_alarms"]},
    r"quality.*spc|cmm": {"vertical": "manufacturing", "formats": ["spc"]},

    # Mid-biz
    r"quickbooks|qb_": {"vertical": "midsize_business", "formats": ["quickbooks"]},
    r"salesforce|sf_": {"vertical": "midsize_business", "formats": ["salesforce"]},
    r"payroll|adp|gusto": {"vertical": "midsize_business", "formats": ["payroll"]},

    # Legal/eDiscovery
    r"departing_employee|litigation|matter": {"vertical": "legal", "formats": ["pst"]},
    r"production|discovery": {"vertical": "legal", "formats": ["loadfile"]},
    r"slack_export": {"vertical": "legal", "formats": ["slack"]},
    r"teams_export|compliance_export": {"vertical": "legal", "formats": ["teams"]},
}
```

### 5.3 Extension Mapping

```python
EXTENSION_MAP = {
    # Healthcare
    ".hl7": {"vertical": "healthcare", "parser": "hl7_parser"},

    # Defense
    ".cot": {"vertical": "defense", "parser": "cot_parser"},
    ".ntf": {"vertical": "defense", "parser": "nitf_parser"},
    ".nitf": {"vertical": "defense", "parser": "nitf_parser"},
    ".pcap": {"vertical": "defense", "parser": "pcap_parser"},
    ".pcapng": {"vertical": "defense", "parser": "pcap_parser"},
    ".kml": {"vertical": "defense", "parser": "kml_parser"},
    ".kmz": {"vertical": "defense", "parser": "kml_parser"},  # ZIP containing KML

    # Finance
    ".fix": {"vertical": "finance", "parser": "fix_parser"},

    # Mid-biz
    ".iif": {"vertical": "midsize_business", "parser": "quickbooks_parser"},

    # Legal/eDiscovery
    ".pst": {"vertical": "legal", "parser": "pst_parser"},
    ".ost": {"vertical": "legal", "parser": "pst_parser"},
    ".mbox": {"vertical": "legal", "parser": "mbox_parser"},
    ".dat": None,  # Could be load file OR other data
    ".opt": {"vertical": "legal", "parser": "loadfile_parser"},
    ".lfp": {"vertical": "legal", "parser": "loadfile_parser"},

    # Generic extensions need content inspection
    ".xml": None,  # Could be CoT, MTConnect, XBRL, ISO20022, KML
    ".csv": None,  # Could be anything
    ".log": None,  # Could be FIX or generic
    ".json": None,  # Could be Slack, Teams, GeoJSON, etc.
}
```

---

## 6. UI/API Design Implications

### 6.1 Smart Scan Suggestions

When user runs `casparian scan <path>`:

1. **Analyze path** against `PATH_HINTS`
2. **Sample files** for content signatures
3. **Suggest vertical + parsers**:
   ```
   Scanning \\hospital-nas-01\interface_archives\ADT_Inbound...

   Detected: Healthcare environment
   Found: 45,230 files matching HL7 ADT pattern

   Suggested action:
     casparian scan \\hospital-nas-01\interface_archives\ADT_Inbound --tag hl7_adt
     casparian process --tag hl7_adt --parser hl7_adt

   [Accept] [Customize] [Skip]
   ```

### 6.2 TUI Mode Enhancements

| Feature | Implementation |
|---------|----------------|
| **Vertical Picker** | First-run wizard: "What industry?" → pre-filter parsers |
| **Path Templates** | Show common paths for selected vertical |
| **Format Preview** | Before processing, show sample parsed output |
| **Tag Suggestions** | Auto-suggest tags based on detected format |

### 6.3 API Enhancements

```python
# New API: detect_format(path) -> FormatDetection
@dataclass
class FormatDetection:
    vertical: str                    # "healthcare", "defense", etc.
    format: str                      # "hl7_adt", "cot", etc.
    confidence: float                # 0.0 - 1.0
    suggested_parser: str            # Parser name
    suggested_tag: str               # Tag suggestion
    sample_preview: Dict[str, Any]   # First few parsed records

# New API: scan_with_detection(path) -> ScanResult
# Combines file discovery with format detection
```

### 6.4 Onboarding Flow by Vertical

| Vertical | First Question | Quick Win |
|----------|----------------|-----------|
| Healthcare | "Where are your HL7 archives?" | Parse ADT → show patient timeline |
| Defense | "Point to mission data folder" | Parse CoT → show map with tracks |
| Finance | "Where are your FIX logs?" | Parse logs → show execution quality |
| Manufacturing | "Where does PI export to?" | Parse CSV → show time-series chart |
| Mid-biz | "Export from QuickBooks to..." | Parse → show trial balance |

---

## 7. Storage Medium Handling

### 7.1 Network Share Considerations

| Issue | Mitigation |
|-------|------------|
| SMB timeout | Increase read timeout; retry with backoff |
| No inotify | Polling-based watch (configurable interval) |
| Millions of files | Parallel file listing; incremental scan |
| Permission errors | Clear error messages; suggest mount check |

### 7.2 Local/USB Drive Considerations

| Issue | Mitigation |
|-------|------------|
| Drive letter changes | Use volume label or path alias |
| USB disconnect | Graceful handling; resume capability |
| Limited space | Warn before processing large datasets |

### 7.3 Air-Gapped Considerations

| Issue | Mitigation |
|-------|------------|
| No pip install | Bundle mode (`casparian bundle`) |
| No network | `--offline` flag; no telemetry |
| Sneakernet workflow | Export results to portable format |

---

## 8. Naming Convention Patterns

### 8.1 Timestamp Formats by Vertical

| Vertical | Common Pattern | Example |
|----------|----------------|---------|
| Healthcare | `YYYYMMDD_HH` | `20260108_14.hl7` |
| Defense | `YYYYMMDD` | `20260108_satellite_pass/` |
| Finance | `YYYYMMDD` | `gateway_20260108.log` |
| Manufacturing | `YYYYMM` or `YYYYMMDD` | `line_1_202601.csv` |
| Mid-biz | `YYYYMM` (or none) | `qb_trial_balance_202601.xlsx` |

### 8.2 Semantic Naming Patterns

| Pattern | Vertical | Meaning |
|---------|----------|---------|
| `*_Inbound/*_Outbound` | Healthcare | Message direction |
| `patrol_*`, `mission_*` | Defense | Operation name |
| `gateway_*`, `execution_*` | Finance | System component |
| `line_*` | Manufacturing | Production line |
| `qb_*`, `sf_*` | Mid-biz | Source system prefix |

---

## 9. Implementation Priority

### Phase 1: Core Detection (Week 1-2)
- [ ] Content signature matching for Tier 1 formats
- [ ] Path hint detection
- [ ] Extension mapping with fallback to content inspection

### Phase 2: Smart Suggestions (Week 3-4)
- [ ] `detect_format()` API
- [ ] TUI integration with suggestions
- [ ] Vertical picker in onboarding

### Phase 3: Advanced Features (Week 5-6)
- [ ] `scan_with_detection()` API
- [ ] Batch format detection for mixed folders
- [ ] Confidence scoring and manual override

---

## 10. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft aggregating all vertical strategies |
| 2026-01-08 | 0.2 | Gap analysis integration: Added Legal/eDiscovery vertical (PST, load files, Slack); Added PCAP to Defense; Enhanced KML; Updated value rankings and detection patterns |

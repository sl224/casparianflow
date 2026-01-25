# Defense Flight Test Returned Media Strategy

**Status:** Canonical (P2 Vertical)
**Parent:** STRATEGY.md
**Priority:** P2 (after DFIR P0, eDiscovery P1)
**Date:** January 2026

---

## 1. Executive Summary

This strategy defines the **Flight Test Returned Media** wedge for defense vertical entry.

**The Wedge:** Ingest data from removable media/cartridges returned from flight test missions (CH10, TF10, DF10, TMATS files).

**Why This Wedge:**
- Clear file-based workflow (cartridge returns from aircraft)
- Existing pain: manual extraction, no governance, no audit trail
- High value: flight test data is expensive to collect
- Trust primitives map directly: manifest + hash + quarantine + diffs

**Target Persona:** Flight Test Data Processor / Telemetry Data Engineer

---

## 2. The Flight Test Data Problem

### 2.1 How Data Returns from Aircraft

```
Flight Test Mission
        │
        ▼
┌─────────────────────┐
│  Onboard Recorder   │  (IRIG-106 format)
│  - CH10/TF10/DF10   │
│  - TMATS metadata   │
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Removable Media    │  (SSD, cartridge, tape)
│  - Returned to base │
│  - Manual handoff   │
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Data Processing    │  ← WE TARGET HERE
│  Station            │
│  - Extract files    │
│  - Validate         │
│  - Archive          │
└─────────────────────┘
```

### 2.2 Current State (Painful)

| Step | Current Workflow | Pain |
|------|------------------|------|
| 1 | Cartridge arrives at data station | No chain of custody |
| 2 | Manual file copy to network share | No hash verification |
| 3 | Custom scripts parse IRIG data | No schema governance |
| 4 | Results go to analysis systems | No lineage tracking |
| 5 | Audit asks "prove this matches original" | Cannot answer |

### 2.3 Casparian Workflow

| Step | Casparian Workflow | Value |
|------|-------------------|-------|
| 1 | `casparian scan /mnt/cartridge --tag flight_test` | Automatic file discovery |
| 2 | Files hashed, manifested, cataloged | Chain of custody |
| 3 | `casparian run irig106_parser.py` | Schema-governed extraction |
| 4 | Outputs with `_cf_source_hash` lineage | Traceable to source |
| 5 | `casparian manifest export` | Audit-ready proof |

---

## 3. Target Formats

### 3.1 IRIG 106 Chapter 10 (CH10)

**What:** Standard for recording instrumentation data on aircraft, spacecraft, missiles.

**Structure:**
```
CH10 File
├── File Header
├── TMATS (Telemetry Attributes Transfer Standard)
├── Channel 1: PCM Telemetry
├── Channel 2: Video (MPEG)
├── Channel 3: 1553 Bus Data
├── Channel 4: Ethernet
└── ...
```

**Key Tables:**

| Table | Description |
|-------|-------------|
| `ch10_files` | File-level metadata (mission, date, duration) |
| `ch10_channels` | Channel inventory per file |
| `ch10_pcm_frames` | Telemetry frame data |
| `ch10_1553_messages` | MIL-STD-1553 bus traffic |
| `ch10_ethernet_packets` | Network captures |
| `ch10_video_metadata` | Video segment timestamps |

### 3.2 TMATS (Telemetry Attributes Transfer Standard)

**What:** ASCII metadata describing the recording configuration.

**Why Important:** TMATS tells you how to interpret the CH10 data (sample rates, channel assignments, etc.).

**Table:**

| Table | Description |
|-------|-------------|
| `tmats_groups` | Recording groups and attributes |
| `tmats_channels` | Channel configuration |

### 3.3 TF10 / DF10

**What:** Variants of IRIG 106 for specific programs.

- **TF10:** Tactical Fighter (F-22, F-35)
- **DF10:** Data Fusion (multi-platform)

Same parsing approach as CH10 with program-specific extensions.

---

## 4. Trust Primitives Mapping

| Casparian Primitive | Flight Test Value |
|---------------------|-------------------|
| **Manifest** | Complete inventory of cartridge contents |
| **Hash Inventory** | Prove extraction matches original media |
| **Quarantine Rows** | Isolate corrupted or malformed data |
| **"What Changed" Diffs** | Compare runs, detect reprocessing delta |
| **Per-row Lineage** | Trace any analysis result to source recording |

### 4.1 Chain of Custody Story

```
Auditor: "Prove this telemetry reading came from the flight."

Casparian Answer:
1. Row has _cf_source_hash = blake3:abc123...
2. Manifest shows blake3:abc123... = /mnt/cartridge/mission_042.ch10
3. Cartridge hash log shows mission_042.ch10 was on cartridge SN-12345
4. Cartridge logbook shows SN-12345 returned from Flight 042 on 2026-01-15

Chain of custody: complete.
```

---

## 5. Target Persona

### 5.1 Primary: Flight Test Data Processor

| Attribute | Description |
|-----------|-------------|
| **Title** | Telemetry Data Engineer, Flight Test Data Processor, Data Reduction Specialist |
| **Environment** | Data processing lab, ground station |
| **Skills** | Python, MATLAB, domain-specific tools (IENA, decom systems) |
| **Pain** | Manual extraction, no governance, audit burden |
| **Goal** | Reliable, traceable, automated data extraction |

### 5.2 Secondary: Flight Test Engineer

| Attribute | Description |
|-----------|-------------|
| **Title** | Flight Test Engineer, Instrumentation Engineer |
| **Environment** | Analysis workstation |
| **Skills** | MATLAB, analysis tools |
| **Pain** | Waiting for data, uncertainty about data quality |
| **Goal** | Faster access to validated data |

---

## 6. Go-to-Market

### 6.1 Entry Points

| Entry Point | Path |
|-------------|------|
| **SBIR/STTR** | AFWERX, Army Futures Command (telemetry topics) |
| **Prime Contractors** | Boeing, Lockheed, Northrop flight test divisions |
| **Test Ranges** | Edwards AFB, Pax River, China Lake |
| **Defense Tech** | Palantir partners, DIU relationships |

### 6.2 Why After eDiscovery (P2)

| Factor | DFIR (P0) | eDiscovery (P1) | Flight Test (P2) |
|--------|-----------|-----------------|------------------|
| Sales cycle | Short (boutiques) | Medium (firms) | Long (DoD) |
| Warm leads | Some | Yes | No |
| Cash flow | Immediate | Medium | Delayed |
| LTV | Medium | High | Very High |
| Support needs | Low | Medium | High |

Flight test has very high LTV but requires longer sales cycles and more support. Position after establishing cash flow from DFIR and eDiscovery.

### 6.3 Demo Script (90 Seconds)

```
[0:00] "Flight test data arrives on cartridges.
       Here's how to get it into your analysis pipeline with full chain of custody."

[0:10] *Point Casparian at returned media*
       $ casparian scan /mnt/cartridge --tag flight_test

[0:20] "Casparian inventories every file, hashes it for provenance."

[0:30] *Run parser*
       $ casparian run irig106_parser.py --sink duckdb://./flight_data.db

[0:45] "Every row in the database links back to the source file.
       The auditor can verify the hash matches the cartridge."

[0:55] *Show manifest*
       $ casparian manifest export --format json

[1:10] "Source hashes, parser versions, processing timestamps.
       Chain of custody, automated."

[1:20] "That's flight test data with governance built in."
```

---

## 7. Pricing

Reference: [docs/product/pricing.md](../docs/product/pricing.md) Section 5

| Tier | Annual Price | Scope |
|------|--------------|-------|
| **Tactical** | $24,000/year | Per flight test program, 5 users |
| **Mission** | $60,000/year | Multi-program, 15 users, priority support |

### 7.1 Why Annual Per-Program

- Flight test programs have defined timelines (2-5 years)
- Budget allocated per program, not per user
- Procurement expects annual contracts

---

## 8. Competitive Landscape

| Player | What They Do | Gap |
|--------|--------------|-----|
| **IRIG-106.org tools** | Reference implementations | No governance, no lineage |
| **Custom decom systems** | Program-specific extraction | Brittle, single point of failure |
| **Pentek, Curtiss-Wright** | Hardware + software recorders | Recording focus, not governance |
| **Casparian** | Governance layer on any parser | Upstream of all analysis |

### 8.1 Positioning

**Wrong:** "We replace decom systems."

**Right:** "We add governance to your existing workflow."

Casparian doesn't replace recording hardware or analysis tools. It sits between them:

```
Recording → [Extraction] → [CASPARIAN: Governance] → Analysis Tools
```

---

## 9. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Long DoD sales cycle | High | Start with DFIR cash flow |
| Classification barriers | High | Target unclassified programs first |
| Format complexity | Medium | Partner with domain experts |
| Incumbent lock-in | Medium | Position as governance layer, not replacement |

---

## 10. Success Metrics

| Metric | 12-Month Target | 24-Month Target |
|--------|-----------------|-----------------|
| Flight test pilots | 2 | 5 |
| Defense ARR (flight test) | $48K | $200K |
| SBIR awards | 1 Phase I | 1 Phase II |

---

## 11. Relationship to defense_tactical.md

`defense_tactical.md` focuses on **tactical edge** use cases:
- DDIL environments
- CoT/NITF/KLV formats
- Laptop-based analysis in the field

This document focuses on **flight test** use cases:
- Ground-based data processing
- CH10/TF10/TMATS formats
- Returned media workflows

Both are defense verticals but serve different personas and workflows.

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Initial canonical flight test strategy |

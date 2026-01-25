# Legal eDiscovery Production Preflight Strategy

**Status:** Canonical (P1 Vertical)
**Parent:** STRATEGY.md
**Priority:** P1 (after DFIR P0)
**Date:** January 2026

---

## 1. Executive Summary

This strategy defines the **Production Preflight** wedge for legal vertical entry.

**The Wedge:** Validate eDiscovery productions (DAT/OPT/LFP load files) before delivery to opposing counsel or review platforms.

**Why This Wedge (Not PST Parsing):**
- Clear file-based workflow (load files arrive from vendors or review platforms)
- Existing pain: broken family relationships, missing files, incorrect BATES
- Production errors = sanctions, malpractice, delays
- Trust primitives map directly: manifest + hash + quarantine + validation
- Billable: validation is a separate line item on legal invoices

**Why Not PST Focus:**
- PST parsing is commodity (many tools exist)
- Full eDiscovery processing requires extensive GUI/review features
- Competes directly with established platforms
- Higher support burden

**Target Persona:** Litigation Support Technologist / eDiscovery Processing Analyst

---

## 2. The Production Preflight Problem

### 2.1 What Goes Wrong

| Error Type | Description | Consequence |
|------------|-------------|-------------|
| **Missing natives** | DAT references files that don't exist | Production rejected |
| **Broken families** | Attachments not linked to parent | Discovery failure |
| **Wrong BATES** | OPT file points to wrong images | Court sanctions |
| **Encoding errors** | Non-UTF8 text in DAT | Platform import fails |
| **Duplicate docs** | Same document produced multiple times | Cost overruns |
| **Missing text** | Text extraction failed silently | Review incomplete |

### 2.2 Current Workflow (Painful)

```
Production Received
        │
        ▼
┌─────────────────────┐
│  Manual Spot Check  │  ← Hours of clicking through folders
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Load to Platform   │  ← Errors discovered during import
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Reject / Rework    │  ← Days lost, vendor back-and-forth
└─────────────────────┘
```

### 2.3 Casparian Workflow

```
Production Received
        │
        ▼
┌─────────────────────┐
│  casparian scan     │  ← Discover all files, hash everything
│  /production/       │
│  --tag load_files   │
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  casparian run      │  ← Parse DAT, OPT, LFP
│  loadfile_parser.py │     Validate references
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Preflight Report   │  ← Summary: pass/fail, errors by type
│                     │     Quarantine rows with problems
└─────────────────────┘
        │
    ┌───┴───┐
    ▼       ▼
  PASS    FAIL → Send rejection with evidence
```

---

## 3. Target Formats

### 3.1 DAT Files (Primary)

**What:** Tab-delimited metadata files. Standard eDiscovery interchange format.

**Variants:**
- Concordance DAT (thorn delimiter þ)
- Relativity DAT (similar format)
- Custom DAT (vendor-specific columns)

**Schema: `loadfile_documents`**

| Column | Description |
|--------|-------------|
| `doc_id` | Document identifier (BEGDOC) |
| `bates_begin` | Starting BATES number |
| `bates_end` | Ending BATES number |
| `custodian` | Source custodian |
| `date_sent` | Send date (emails) |
| `date_created` | File creation date |
| `author` | Document author |
| `subject` | Subject line |
| `native_path` | Path to native file |
| `text_path` | Path to extracted text |
| `parent_id` | Parent document ID |
| `attachment_range` | Child BATES range |

### 3.2 OPT Files

**What:** Image cross-reference files. Map BATES numbers to TIFF/PDF images.

**Schema: `loadfile_images`**

| Column | Description |
|--------|-------------|
| `bates` | BATES number |
| `volume` | Volume identifier |
| `image_path` | Path to image file |
| `doc_break` | Document boundary flag |
| `folder` | Image folder |
| `box` | Box number |
| `page_count` | Pages in document |

### 3.3 LFP Files (IPRO)

**What:** IPRO load file format. Page-level metadata.

**Schema:** Similar to OPT with IPRO-specific fields.

---

## 4. Trust Primitives Mapping

| Casparian Primitive | Production Preflight Value |
|---------------------|---------------------------|
| **Manifest** | Complete inventory of production contents |
| **Hash Inventory** | Verify files weren't altered in transit |
| **Quarantine Rows** | Isolate documents with validation errors |
| **"What Changed" Diffs** | Compare supplemental to original production |
| **Per-row Lineage** | Track which DAT row came from which file |

### 4.1 Validation Checks

| Check | Description | Severity |
|-------|-------------|----------|
| `native_exists` | Native file path exists | Error |
| `text_exists` | Text file path exists | Warning |
| `image_exists` | OPT image paths exist | Error |
| `family_complete` | Attachments linked correctly | Error |
| `bates_sequence` | BATES numbers in order | Warning |
| `bates_gaps` | No missing BATES numbers | Warning |
| `encoding_valid` | DAT is valid UTF-8 or Latin-1 | Error |
| `date_valid` | Date fields parse correctly | Warning |

---

## 5. Source Locations

### 5.1 SFTP Drop Folders

Most productions arrive via SFTP:

```
sftp://vendor.com/outgoing/
├── MatterXYZ_Production_001/
│   ├── DATA/
│   │   └── production_001.dat
│   ├── IMAGES/
│   ├── NATIVES/
│   └── TEXT/
└── MatterXYZ_Production_002/
```

**Casparian Support:**
```bash
casparian scan sftp://vendor.com/outgoing/MatterXYZ* --tag production
```

### 5.2 UNC Shares

Large productions on network shares:

```
\\fileserver\productions\
├── 2026\
│   ├── Smith_v_Jones\
│   │   └── Production_001\
│   └── ...
```

**Casparian Support:**
```bash
casparian scan "\\fileserver\productions\Smith_v_Jones" --tag production
```

### 5.3 Received Media

Productions on USB, external drives, optical media:

```
/mnt/production_drive/
├── Production_001/
└── ...
```

---

## 6. Target Persona

### 6.1 Primary: Litigation Support Technologist

| Attribute | Description |
|-----------|-------------|
| **Title** | Litigation Support Tech, eDiscovery Analyst, Processing Specialist |
| **Environment** | Law firm, vendor, corporate legal dept |
| **Skills** | Load file formats, Relativity, SQL basics |
| **Pain** | Manual production validation, platform import errors |
| **Goal** | Fast, reliable production QC before handoff |

### 6.2 Secondary: eDiscovery Project Manager

| Attribute | Description |
|-----------|-------------|
| **Title** | eDiscovery PM, Litigation Support Manager |
| **Environment** | Managing multiple matters |
| **Skills** | Process management, vendor coordination |
| **Pain** | Production delays, vendor quality issues |
| **Goal** | Automated QC to reduce back-and-forth |

---

## 7. Go-to-Market

### 7.1 Why P1 (After DFIR)

| Factor | DFIR (P0) | eDiscovery Preflight (P1) |
|--------|-----------|---------------------------|
| Sales cycle | Short (boutiques) | Medium (firms) |
| Warm leads | Some | Yes (legal tech network) |
| Cash flow | Immediate | Medium |
| LTV | Medium | High |
| Support needs | Low | Medium |
| Billable engagement | N/A | Yes (QC is billed) |

eDiscovery has **same trust primitives** as DFIR:
- Manifest = production inventory
- Hash = verify transit integrity
- Quarantine = isolate bad documents
- Diffs = compare productions

### 7.2 Demo Script (90 Seconds)

```
[0:00] "You receive a production from opposing counsel.
       Here's how to validate it before loading to Relativity."

[0:10] *Point Casparian at production*
       $ casparian scan /production/Smith_001 --tag load_files

[0:20] "Casparian inventories every file, hashes for integrity."

[0:30] *Run preflight*
       $ casparian run loadfile_parser.py --preflight

[0:45] "Validation runs: native files exist, families linked,
       BATES in sequence, text extracted."

[0:55] *Show preflight report*
       $ casparian report --preflight

[1:10] "12 documents quarantined: missing natives.
       Send this report back to vendor before you waste hours on import."

[1:20] "That's production preflight. Catch errors before they cost you."
```

### 7.3 Channels

| Channel | Approach |
|---------|----------|
| **Legal ops communities** | ILTA, ACC, ACEDS |
| **Litigation support networks** | LinkedIn groups |
| **Legal tech consultants** | Partner program |
| **eDiscovery vendors** | Integration partnerships |

---

## 8. Pricing

Reference: [docs/product/pricing.md](../docs/product/pricing.md) Section 4

| Tier | Annual Price | Users | Features |
|------|--------------|-------|----------|
| **Solo** | $1,800/year | 1 | CLI + TUI, load file parsers, 25GB/month |
| **Team** | $7,200/year | Up to 5 | + Unlimited volume, SFTP/UNC support |
| **Enterprise Lite** | $18,000/year | Up to 15 | + SSO, priority email, monthly call |

### 8.1 Value Analysis

| Current Cost | Amount | Casparian Savings |
|--------------|--------|-------------------|
| Manual QC (2 hrs @ $150/hr) | $300/production | 90% reduction |
| Platform import failure rework | $500-2,000 | Avoided |
| Production rejection/re-send | $1,000-5,000 | Avoided |
| Annual validation costs (20 matters) | $6,000-40,000 | $5,000-35,000 saved |

---

## 9. Scope Boundaries

### 9.1 What We Do (Preflight)

- Parse DAT/OPT/LFP files
- Validate file references exist
- Check family relationships
- Verify BATES sequences
- Detect encoding issues
- Generate preflight reports
- Hash-based integrity verification

### 9.2 What We Don't Do

- Full eDiscovery processing (PST → review)
- Attorney review interface
- Relativity/Everlaw replacement
- Deduplication across productions
- Privilege logging
- Redaction

**Philosophy:** Minimal scope. Do preflight extremely well. Don't expand into full processing.

---

## 10. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Firms already have Relativity | Medium | Position as complement, not replacement |
| Scope creep into full processing | High | Explicit scope boundaries in product |
| GUI required for adoption | Medium | TUI first, then desktop app |
| Format variations | Medium | Focus on Concordance/Relativity DAT first |

---

## 11. Success Metrics

| Metric | 12-Month Target |
|--------|-----------------|
| Legal vertical ARR | $50K |
| Productions validated | 500 |
| Quarantine catch rate | 95%+ |
| Partner consultants | 10 |

---

## 12. Relationship to Other Docs

| Doc | Purpose |
|-----|---------|
| `strategies/ediscovery.md` | ARCHIVED - old persona segmentation doc |
| `docs/product/pricing.md` | System of record for pricing |
| `STRATEGY.md` | Parent strategy doc |

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 2.0 | Complete rewrite for Production Preflight wedge (P1 vertical) |
| 2026-01-08 | 0.1 | Initial draft (PST-focused, deprecated) |

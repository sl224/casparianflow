# Pharma Clinical Submission Strategy

**Status:** Draft (Deferred - Post-Traction)
**Parent:** STRATEGY.md
**Priority:** Deferred (after DFIR/eDiscovery/Defense traction)
**Date:** January 2026

---

## 1. Executive Summary

This strategy defines the **Clinical Submission Preflight** wedge for pharma vertical entry.

**The Wedge:** Validate clinical submission packages (XPT datasets, define.xml, SDRG/ADRG documents) before FDA/EMA submission.

**Why This Wedge (Not Instrument Data):**
- Instrument data (Mass Spec, HPLC) is addressed by `strategies/pharma.md` (R&D focus)
- Clinical submission has clearer "validate before submit" workflow
- Regulatory deadline pressure = willingness to pay
- Standard formats (CDISC) = predictable parsing

**Why Deferred:**
- Long enterprise sales cycle (6-18 months)
- Requires domain expertise
- No warm leads currently
- DFIR/eDiscovery provide cash flow first

**Target Persona:** Clinical Data Programmer / Regulatory Submissions Analyst

---

## 2. The Clinical Submission Problem

### 2.1 What Gets Submitted

```
Clinical Submission Package (eCTD)
├── Module 5 - Clinical Study Reports
│   ├── Datasets/
│   │   ├── *.xpt                 (SAS Transport files - actual data)
│   │   └── define.xml            (Data dictionary)
│   ├── Analysis/
│   │   ├── ADRG.pdf              (Analysis Data Reviewer's Guide)
│   │   └── analysis_results.pdf
│   └── Reviewer Guides/
│       └── SDRG.pdf              (Study Data Reviewer's Guide)
```

### 2.2 What Goes Wrong

| Error Type | Description | Consequence |
|------------|-------------|-------------|
| **XPT/define mismatch** | Variable in XPT not in define.xml | FDA Refuse to File |
| **Missing datasets** | define.xml references non-existent XPT | Submission rejected |
| **Encoding issues** | Non-ASCII in variable labels | Validation failure |
| **Length violations** | Variable exceeds max length | P21 warnings |
| **Value-level metadata** | Controlled terms mismatch | Reviewer confusion |
| **Broken hyperlinks** | SDRG links to wrong analysis | Review delays |

### 2.3 Current Workflow

```
Submission Package Assembled
        │
        ▼
┌─────────────────────┐
│  Pinnacle 21        │  ← $50K+/year license
│  (P21 Enterprise)   │
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Manual Review      │  ← Days of checking
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Submit to Gateway  │  ← Hope for the best
└─────────────────────┘
```

### 2.4 Casparian Opportunity

Casparian is **not** a P21 replacement. It's a **preflight layer** that:
- Validates file inventory (all referenced files exist)
- Checks define.xml/XPT consistency
- Verifies hyperlinks in reviewer guides
- Provides hash-based integrity for package contents

---

## 3. Target Formats

### 3.1 XPT (SAS Transport)

**What:** SAS V5 transport format. Standard for clinical trial data exchange.

**Schema: `xpt_datasets`**

| Column | Description |
|--------|-------------|
| `dataset_name` | Dataset identifier (DM, AE, LB, etc.) |
| `variable_name` | Variable name |
| `variable_label` | Human-readable label |
| `variable_type` | Char or Num |
| `variable_length` | Max length |
| `row_count` | Number of observations |

### 3.2 define.xml

**What:** XML data dictionary describing all datasets and variables.

**Schema: `define_variables`**

| Column | Description |
|--------|-------------|
| `dataset_name` | Parent dataset |
| `variable_name` | Variable identifier |
| `variable_label` | Label from define |
| `data_type` | Expected type |
| `codelist` | Controlled terminology reference |
| `origin` | Derived, Assigned, Collected |

### 3.3 Reviewer Guides (SDRG/ADRG)

**What:** PDF documents explaining data structure to FDA reviewers.

**Validation:**
- Hyperlinks resolve correctly
- Referenced datasets exist
- Table of contents accurate

---

## 4. Trust Primitives Mapping

| Casparian Primitive | Submission Preflight Value |
|---------------------|---------------------------|
| **Manifest** | Complete package inventory |
| **Hash Inventory** | Prove package integrity post-assembly |
| **Quarantine** | Isolate datasets with validation issues |
| **Diffs** | Compare draft vs final submission |
| **Lineage** | Track which source data created each XPT |

---

## 5. Target Persona

### 5.1 Primary: Clinical Data Programmer

| Attribute | Description |
|-----------|-------------|
| **Title** | SAS Programmer, Clinical Data Programmer, Biostatistician |
| **Environment** | Pharma sponsor or CRO |
| **Skills** | SAS, R, Python, CDISC standards |
| **Pain** | P21 failures at submission time |
| **Goal** | Clean submission package on first try |

### 5.2 Secondary: Regulatory Submissions Manager

| Attribute | Description |
|-----------|-------------|
| **Title** | Regulatory Affairs Manager, Submissions Lead |
| **Environment** | Pharma regulatory affairs |
| **Skills** | eCTD structure, Gateway submission |
| **Pain** | Last-minute rejections, delays |
| **Goal** | Confidence in package quality |

---

## 6. Why Deferred

| Factor | DFIR (P0) | eDiscovery (P1) | Pharma Submission (Deferred) |
|--------|-----------|-----------------|------------------------------|
| Sales cycle | Short | Medium | Long (6-18 months) |
| Warm leads | Some | Yes | No |
| Domain expertise needed | Low | Medium | High |
| Support burden | Low | Medium | High |
| Regulatory requirements | Low | Medium | Very High |
| Cash flow | Immediate | Medium | Delayed |

**Decision:** Establish cash flow with DFIR/eDiscovery before entering pharma.

---

## 7. Competitive Landscape

| Player | What They Do | Gap |
|--------|--------------|-----|
| **Pinnacle 21** | CDISC validation suite | $50K+/year, no lineage |
| **Formedix** | Define.xml authoring | Authoring focus, not validation |
| **SAS** | Data management | Heavyweight, expensive |
| **Custom scripts** | Ad-hoc validation | No governance |
| **Casparian** | Preflight + lineage | Governance layer on existing tools |

---

## 8. Relationship to strategies/pharma.md

| Doc | Focus |
|-----|-------|
| `strategies/pharma.md` | R&D instrument data (Mass Spec, HPLC, plate readers) |
| `strategies/pharma_clinical_submission.md` (this) | Clinical submission packages (XPT, define.xml) |

Different personas, different workflows, different formats.

---

## 9. When to Activate

Activate this strategy when:
1. DFIR ARR > $150K (cash flow established)
2. Warm lead from pharma contact
3. Domain expert hired or partnered
4. P21 integration feasibility confirmed

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Initial draft (deferred status) |

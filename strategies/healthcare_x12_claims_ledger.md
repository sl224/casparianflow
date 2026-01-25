# Healthcare X12 Claims Ledger Strategy

**Status:** Draft (Deferred - Post-Traction)
**Parent:** STRATEGY.md
**Priority:** Deferred (after DFIR/eDiscovery/Defense traction)
**Date:** January 2026

---

## 1. Executive Summary

This strategy defines the **Claims Ledger Reconciliation** wedge for healthcare vertical entry.

**The Wedge:** Reconcile X12 EDI claims (837) with acknowledgments (999/TA1/277CA) and remittances (835) to create a unified claims ledger.

**Why This Wedge (Not HL7):**
- HL7 is addressed by `strategies/healthcare_hl7.md` (clinical messaging focus)
- Claims reconciliation has clearer file-based workflow
- Revenue cycle = money at stake = willingness to pay
- Standard formats (X12) = predictable parsing

**Why Deferred:**
- Long enterprise sales cycle (12-18 months)
- HIPAA compliance requirements
- No warm leads currently
- DFIR/eDiscovery provide cash flow first

**Target Persona:** Revenue Cycle Analyst / Claims Data Engineer

---

## 2. The Claims Reconciliation Problem

### 2.1 The X12 Claims Lifecycle

```
Provider Submits Claim
        │
        ▼
┌─────────────────────┐
│  837 (Claim)        │  → Sent to payer
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  999/TA1 (Ack)      │  ← Technical acknowledgment
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  277CA (Status)     │  ← Claim status update
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  835 (Remittance)   │  ← Payment or denial
└─────────────────────┘
```

### 2.2 What Goes Wrong

| Error Type | Description | Consequence |
|------------|-------------|-------------|
| **Missing 835** | Claim submitted, no remittance received | Revenue leakage |
| **Partial payment** | 835 amount < 837 billed | Denial not appealed |
| **Lost claims** | 837 sent, no 999 received | Never processed |
| **Duplicate payment** | Same service paid twice | Payer audit risk |
| **Unmatched denials** | 277CA with no corresponding 837 | Data integrity issue |

### 2.3 Current Workflow

```
Files Arrive from Clearinghouse
        │
        ▼
┌─────────────────────┐
│  PM/EHR System      │  ← Limited reconciliation
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Spreadsheets       │  ← Manual matching
└─────────────────────┘
        │
        ▼
┌─────────────────────┐
│  Revenue Analysts   │  ← Hunt for missing $$$
└─────────────────────┘
```

### 2.4 Casparian Opportunity

Build a **claims ledger** by joining:
- 837 (what was billed)
- 999/TA1 (was it received?)
- 277CA (what's the status?)
- 835 (what was paid?)

Surface:
- Claims without remittance (revenue leakage)
- Underpayments (appeal candidates)
- Denials by reason code (process improvement)

---

## 3. Target Formats

### 3.1 837 (Healthcare Claim)

**Variants:**
- 837P (Professional)
- 837I (Institutional)
- 837D (Dental)

**Schema: `claims_837`**

| Column | Description |
|--------|-------------|
| `claim_id` | CLM01 identifier |
| `patient_id` | NM1*QC segment |
| `provider_npi` | NM1*85 segment |
| `payer_id` | NM1*PR segment |
| `service_date` | DTP*472 segment |
| `billed_amount` | CLM02 total |
| `diagnosis_codes` | HI segment codes |
| `procedure_codes` | SV1/SV2 segments |
| `place_of_service` | CLM05-1 |

### 3.2 835 (Healthcare Claim Payment)

**Schema: `remittances_835`**

| Column | Description |
|--------|-------------|
| `claim_id` | CLP01 reference |
| `patient_id` | NM1*QC segment |
| `paid_amount` | CLP04 amount |
| `billed_amount` | CLP03 from 837 |
| `claim_status` | CLP02 code |
| `adjustment_codes` | CAS segments |
| `check_number` | TRN segment |
| `payment_date` | DTM*405 segment |

### 3.3 999/TA1 (Acknowledgments)

**Schema: `acknowledgments_999`**

| Column | Description |
|--------|-------------|
| `original_control` | Original ISA13 |
| `ack_status` | AK9 code (A=Accepted, R=Rejected) |
| `error_codes` | AK3/AK4 segment errors |
| `received_date` | ISA09 from 999 |

### 3.4 277CA (Claim Status)

**Schema: `status_277ca`**

| Column | Description |
|--------|-------------|
| `claim_id` | REF*BLT segment |
| `status_code` | STC01 claim status |
| `status_date` | DTP*472 segment |
| `payer_claim_id` | REF*1K segment |

---

## 4. Trust Primitives Mapping

| Casparian Primitive | Claims Ledger Value |
|---------------------|---------------------|
| **Manifest** | Inventory of all EDI files received |
| **Hash Inventory** | Detect file tampering/corruption |
| **Quarantine** | Isolate malformed EDI transactions |
| **Diffs** | Compare ledger snapshots over time |
| **Lineage** | Trace payment back to original claim file |

---

## 5. Target Persona

### 5.1 Primary: Revenue Cycle Analyst

| Attribute | Description |
|-----------|-------------|
| **Title** | Revenue Cycle Analyst, AR Specialist, Claims Analyst |
| **Environment** | Healthcare provider billing office |
| **Skills** | Excel, PM system, EDI basics |
| **Pain** | Manual reconciliation, missing revenue |
| **Goal** | Identify underpayments and denials faster |

### 5.2 Secondary: Claims Data Engineer

| Attribute | Description |
|-----------|-------------|
| **Title** | Healthcare Data Engineer, EDI Analyst |
| **Environment** | Large health system IT, clearinghouse |
| **Skills** | SQL, Python, X12 specification |
| **Pain** | Building reconciliation pipelines |
| **Goal** | Automated claims ledger |

---

## 6. Why Deferred

| Factor | DFIR (P0) | eDiscovery (P1) | Healthcare X12 (Deferred) |
|--------|-----------|-----------------|---------------------------|
| Sales cycle | Short | Medium | Long (12-18 months) |
| Warm leads | Some | Yes | No |
| Compliance requirements | Low | Medium | High (HIPAA) |
| Support burden | Low | Medium | High |
| Cash flow | Immediate | Medium | Delayed |

**Decision:** Establish cash flow with DFIR/eDiscovery before entering healthcare.

---

## 7. Competitive Landscape

| Player | What They Do | Gap |
|--------|--------------|-----|
| **Waystar** | Revenue cycle management | $$$, full platform |
| **Availity** | Clearinghouse + analytics | Payer-centric |
| **Experian Health** | Claims management | Enterprise focus |
| **Custom PM reports** | EHR-based reconciliation | Limited visibility |
| **Casparian** | Claims ledger from EDI files | Governance + flexibility |

---

## 8. Relationship to strategies/healthcare_hl7.md

| Doc | Focus |
|-----|-------|
| `strategies/healthcare_hl7.md` | Clinical messaging (ADT, ORM, ORU) |
| `strategies/healthcare_x12_claims_ledger.md` (this) | Revenue cycle (837/835/999/277CA) |

Different personas, different workflows, different formats.

---

## 9. When to Activate

Activate this strategy when:
1. DFIR ARR > $150K (cash flow established)
2. Warm lead from healthcare contact
3. HIPAA compliance framework in place
4. BAA template ready

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Initial draft (deferred status) |

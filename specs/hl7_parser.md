# HL7 v2.x Parser - Product Subspec

**Status:** Draft (Revised after Review)
**Parent:** spec.md Section 4 (Functional Specifications)
**Version:** 0.2
**Date:** January 8, 2026

---

## 1. Executive Summary

This spec defines a **premade HL7 v2.x parser** that ships with Casparian Flow, enabling healthcare data analysts to transform HL7 message exports into queryable SQL tables without HL7 expertise or custom ETL development.

### 1.1 The Problem

Healthcare organizations sit on vast amounts of "dark data" in HL7 v2.x format:
- EHR system exports (thousands of `.hl7` files)
- Integration engine archives (historical message logs)
- Compliance archives (7+ years of ADT/lab data)

**Current pain points:**
- HL7 is "semi-structured" - not easily loaded into databases
- Analysts must write custom Python scripts for each project
- ETL tools (Mirth, Rhapsody) are expensive and complex
- Knowledge silos: only integration engineers understand HL7

### 1.2 The Solution

Casparian ships with drop-in HL7 parsers that:
1. Parse any HL7 v2.x message (versions 2.1 - 2.8.2)
2. Output normalized, analyst-friendly tables (Patients, Visits, Observations)
3. Handle real-world messiness (lenient parsing, graceful degradation)
4. Integrate with existing Casparian workflow (discover → parse → query)

### 1.3 Target User

**Primary:** Healthcare Data Analyst
- Works at hospital IT, health system, research institution, or payer
- Receives HL7 exports as flat files from EHR/integration teams
- Knows SQL, maybe Python, but **not an HL7 expert**
- Needs queryable data for reports, research, or analytics

**Secondary:** Clinical Informaticist
- Understands HL7 structure
- Needs rapid prototyping for new data feeds
- Values customization options

### 1.4 Success Metrics

| Metric | Target |
|--------|--------|
| Time to first query | < 5 minutes from file drop |
| Message parse success rate | > 95% on real-world data |
| Zero-config usability | Works without editing parser |
| Schema comprehension | Analyst can write SQL without HL7 knowledge |

---

## 2. Market Context

### 2.1 HL7 v2.x Prevalence

- **~95% of US hospitals** use HL7 v2.x for internal data exchange
- **ADT messages** (Admit/Discharge/Transfer) are the most common type
- **ORU messages** (Observation Results) carry lab/clinical data
- HL7 v2.x will remain dominant for 10+ years due to legacy system inertia

### 2.2 Competitive Landscape

| Solution | Cost | Complexity | Use Case |
|----------|------|------------|----------|
| Mirth Connect | $$$$ (commercial since 2025) | High | Enterprise integration |
| Rhapsody | $$$$$ | Very High | Large health systems |
| Qvera QIE | $$$$ | Medium | Mid-market |
| python-hl7 / hl7apy | Free | High (DIY) | Developers |
| **Casparian HL7** | Free | **Low** | **Analytics/Research** |

### 2.3 Why Now

- **Mirth Connect went commercial (March 2025)** - organizations seeking alternatives
- **Analytics demand increasing** - value-based care requires data insights
- **Analyst bottleneck** - more data than analysts can process with current tools

---

## 3. Architecture

### 3.1 Component Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CASPARIAN HL7 INTEGRATION                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ~/.casparian_flow/                                                     │
│  ├── parsers/                                                           │
│  │   ├── hl7_adt.py          # Ships with Casparian (ADT messages)     │
│  │   ├── hl7_oru.py          # Ships with Casparian (ORU messages)     │
│  │   └── hl7_generic.py      # Ships with Casparian (any message)      │
│  │                                                                      │
│  ├── output/                                                            │
│  │   ├── hl7_patients_{job_id}.parquet                                 │
│  │   ├── hl7_visits_{job_id}.parquet                                   │
│  │   ├── hl7_observations_{job_id}.parquet                             │
│  │   └── hl7_messages_{job_id}.parquet   # Raw message metadata        │
│  │                                                                      │
│  └── casparian_flow.sqlite3                                            │
│      └── cf_quarantine (malformed messages)                            │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Parser Hierarchy

```
hl7_generic.py          # Base: parses any HL7 v2.x, outputs raw segments
    │
    ├── hl7_adt.py      # Specialized: ADT messages → Patients + Visits
    │
    └── hl7_oru.py      # Specialized: ORU messages → Observations
```

**Design principle:** Specialized parsers inherit from generic, adding domain-specific extraction logic.

### 3.3 Data Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  HL7 Files   │────►│  Casparian   │────►│   Parser     │────►│   Output     │
│  (.hl7)      │     │   Scout      │     │  (hl7_adt)   │     │  (Parquet)   │
└──────────────┘     └──────────────┘     └──────────────┘     └──────────────┘
                            │                    │                     │
                            ▼                    ▼                     ▼
                     Tag: "hl7_adt"      Yields: patients,      SQL queryable
                                         visits, messages        tables
```

### 3.4 File Format Support

| Format | Description | Support |
|--------|-------------|---------|
| Single message per file | One `.hl7` file = one message | ✅ Primary |
| Batch files | Multiple messages, FHS/BHS envelope | ✅ Supported |
| MLLP-stripped logs | One message per line, no framing | ✅ Supported |
| MLLP with framing | `\x0b`...`\x1c\x0d` delimiters | ✅ Auto-detected |

---

## 4. HL7 v2.x Primer (For Context)

### 4.1 Message Structure

```
MSH|^~\&|EPIC|HOSPITAL|LAB|LAB|20240115120000||ADT^A01|MSG001|P|2.5.1
EVN|A01|20240115120000
PID|1||MRN001^^^HOSPITAL^MR||DOE^JOHN^Q||19800515|M|||123 MAIN ST^^CHICAGO^IL^60601
PV1|1|I|ICU^101^A^HOSPITAL||||1234^SMITH^JANE^M^MD|||MED||||ADM|
```

**Key concepts:**
- **Segments:** Lines starting with 3-letter codes (MSH, PID, PV1)
- **Fields:** Separated by `|` (pipe)
- **Components:** Separated by `^` (caret)
- **Subcomponents:** Separated by `&` (ampersand)
- **Repetitions:** Separated by `~` (tilde)

### 4.2 Critical Segments

| Segment | Name | Contains |
|---------|------|----------|
| MSH | Message Header | Message type, version, timestamps |
| PID | Patient Identification | Demographics, MRN, name, DOB |
| PV1 | Patient Visit | Admission info, location, providers |
| OBR | Observation Request | Order information |
| OBX | Observation Result | Lab values, vital signs |
| DG1 | Diagnosis | ICD codes |
| IN1 | Insurance | Payer information |

### 4.3 Message Types (Trigger Events)

| Type | Event | Description |
|------|-------|-------------|
| ADT^A01 | Admit | Patient admitted |
| ADT^A02 | Transfer | Patient transferred |
| ADT^A03 | Discharge | Patient discharged |
| ADT^A04 | Register | Outpatient registration |
| ADT^A08 | Update | Patient info updated |
| ORU^R01 | Result | Observation result |
| ORM^O01 | Order | New order placed |

---

## 5. Output Schema Specification

### 5.1 Design Philosophy

**Normalized, analyst-friendly tables:**
- No HL7 knowledge required to query
- Standard dimensional model (star schema)
- Preserves Casparian lineage columns
- Handles one-to-many relationships cleanly

**Log vs. Current State (Critical Design Decision):**

HL7 is a *transaction log*, not a database. A single patient may generate dozens of messages (admit, transfer, update, discharge). Analysts typically want *current state*, not the full transaction history.

**Solution:** Dual output mode:

| Table | Contains | Use Case |
|-------|----------|----------|
| `hl7_patients` | Every PID extraction (append-only log) | Audit, lineage, debugging |
| `hl7_patients_current` | Latest record per patient_id | Analytics, reporting |
| `hl7_visits` | Every ADT event (append-only log) | Event timeline |
| `hl7_visits_current` | Latest state per visit_id | Current census |

**Implementation:** `*_current` tables are materialized views (deduplicated by ID, latest `_cf_message_datetime` wins). TUI Inspect mode offers toggle: `[l] Log View` vs `[c] Current State`.

### 5.2 Core Tables

#### 5.2.1 `hl7_patients`

Extracted from PID (Patient Identification) segment.

| Column | Type | HL7 Source | Description |
|--------|------|------------|-------------|
| `patient_id` | String | PID-3.1 | Medical Record Number (primary identifier) |
| `patient_id_type` | String | PID-3.5 | Identifier type (MR, SS, etc.) |
| `patient_id_authority` | String | PID-3.4 | Assigning authority |
| `name_family` | String | PID-5.1 | Last name |
| `name_given` | String | PID-5.2 | First name |
| `name_middle` | String | PID-5.3 | Middle name/initial |
| `name_suffix` | String | PID-5.4 | Suffix (Jr, III, etc.) |
| `name_prefix` | String | PID-5.5 | Prefix (Mr, Dr, etc.) |
| `birth_date` | Date | PID-7 | Date of birth |
| `birth_date_raw` | String | PID-7 | Raw HL7 date string (for debugging) |
| `gender` | String | PID-8 | Administrative gender (M/F/U/O) |
| `race` | String | PID-10 | Race code |
| `address_street` | String | PID-11.1 | Street address |
| `address_city` | String | PID-11.3 | City |
| `address_state` | String | PID-11.4 | State/province |
| `address_zip` | String | PID-11.5 | Postal code |
| `address_country` | String | PID-11.6 | Country |
| `phone_home` | String | PID-13 | Home phone |
| `phone_work` | String | PID-14 | Work phone |
| `language` | String | PID-15 | Primary language |
| `marital_status` | String | PID-16 | Marital status code |
| `ssn` | String | PID-19 | SSN (if present) |
| `drivers_license` | String | PID-20 | Driver's license |
| `ethnic_group` | String | PID-22 | Ethnicity code |
| `birth_place` | String | PID-23 | Birth place |
| `death_indicator` | Boolean | PID-30 | Patient deceased flag |
| `death_datetime` | Timestamp | PID-29 | Death date/time |
| `_cf_message_id` | String | MSH-10 | Source message control ID |
| `_cf_message_datetime` | Timestamp | MSH-7 | Message timestamp |
| `_cf_source_hash` | String | — | Casparian lineage |
| `_cf_job_id` | String | — | Casparian lineage |
| `_cf_processed_at` | Timestamp | — | Casparian lineage |
| `_cf_parser_version` | String | — | Casparian lineage |

**Notes:**
- Multiple messages for same patient_id are **deduplicated** (latest wins)
- Original messages preserved in `hl7_messages` table for audit

#### 5.2.2 `hl7_visits`

Extracted from PV1 (Patient Visit) segment.

| Column | Type | HL7 Source | Description |
|--------|------|------------|-------------|
| `visit_id` | String | PV1-19 | Visit number |
| `patient_id` | String | PID-3.1 | FK to patients |
| `visit_class` | String | PV1-2 | I=Inpatient, O=Outpatient, E=Emergency |
| `location_facility` | String | PV1-3.4 | Facility code |
| `location_building` | String | PV1-3.7 | Building |
| `location_unit` | String | PV1-3.1 | Nursing unit |
| `location_room` | String | PV1-3.2 | Room number |
| `location_bed` | String | PV1-3.3 | Bed |
| `admission_type` | String | PV1-4 | Admission type code |
| `attending_id` | String | PV1-7.1 | Attending physician ID |
| `attending_name` | String | PV1-7.2-3 | Attending physician name |
| `referring_id` | String | PV1-8.1 | Referring physician ID |
| `referring_name` | String | PV1-8.2-3 | Referring physician name |
| `hospital_service` | String | PV1-10 | Hospital service (MED, SURG, etc.) |
| `admit_datetime` | Timestamp | PV1-44 | Admission date/time |
| `admit_datetime_raw` | String | PV1-44 | Raw HL7 datetime (for debugging) |
| `discharge_datetime` | Timestamp | PV1-45 | Discharge date/time |
| `discharge_datetime_raw` | String | PV1-45 | Raw HL7 datetime (for debugging) |
| `discharge_disposition` | String | PV1-36 | Discharge disposition code |
| `admit_source` | String | PV1-14 | Admit source code |
| `vip_indicator` | String | PV1-16 | VIP flag |
| `visit_indicator` | String | PV1-51 | Visit indicator |
| `event_type` | String | MSH-9.2 | Trigger event (A01, A02, A03, etc.) |
| `_cf_message_id` | String | MSH-10 | Source message control ID |
| `_cf_message_datetime` | Timestamp | MSH-7 | Message timestamp |
| `_cf_source_hash` | String | — | Casparian lineage |
| `_cf_job_id` | String | — | Casparian lineage |
| `_cf_processed_at` | Timestamp | — | Casparian lineage |
| `_cf_parser_version` | String | — | Casparian lineage |

**Notes:**
- Each ADT event creates a row (preserves history)
- Use `event_type` to filter: A01=admit, A03=discharge, A08=update
- `visit_id` may be null for pre-registration events

#### 5.2.3 `hl7_orders` (Critical: The Connective Tissue)

Extracted from OBR (Observation Request) segment. **This table links visits to observations.**

Without `hl7_orders`, you cannot answer: "Which CBC order did this hemoglobin result come from?"

| Column | Type | HL7 Source | Description |
|--------|------|------------|-------------|
| `order_id` | String | OBR-2 | Placer order number (primary key) |
| `filler_order_id` | String | OBR-3 | Filler (lab) order number |
| `patient_id` | String | PID-3.1 | FK to patients |
| `visit_id` | String | PV1-19 | FK to visits (nullable) |
| `universal_service_id` | String | OBR-4.1 | Order code (e.g., "CBC") |
| `universal_service_text` | String | OBR-4.2 | Order description |
| `universal_service_system` | String | OBR-4.3 | Code system (LOINC, local) |
| `order_datetime` | Timestamp | OBR-7 | Observation date/time |
| `order_datetime_raw` | String | OBR-7 | Raw HL7 datetime string (for debugging) |
| `observation_end_datetime` | Timestamp | OBR-8 | Observation end time |
| `collection_volume` | String | OBR-9 | Collection volume |
| `collector_id` | String | OBR-10 | Collector identifier |
| `specimen_action_code` | String | OBR-11 | Action code |
| `danger_code` | String | OBR-12 | Danger code |
| `clinical_info` | String | OBR-13 | Relevant clinical info |
| `specimen_received_datetime` | Timestamp | OBR-14 | When specimen received |
| `specimen_source` | String | OBR-15 | Specimen source |
| `ordering_provider_id` | String | OBR-16.1 | Ordering provider ID |
| `ordering_provider_name` | String | OBR-16.2-3 | Ordering provider name |
| `order_callback_phone` | String | OBR-17 | Callback phone |
| `placer_field_1` | String | OBR-18 | Placer field 1 |
| `placer_field_2` | String | OBR-19 | Placer field 2 |
| `filler_field_1` | String | OBR-20 | Filler field 1 |
| `filler_field_2` | String | OBR-21 | Filler field 2 |
| `results_datetime` | Timestamp | OBR-22 | Results report date/time |
| `results_datetime_raw` | String | OBR-22 | Raw datetime (for debugging) |
| `result_status` | String | OBR-25 | Result status (F=Final, P=Preliminary) |
| `parent_order_id` | String | OBR-29 | Parent order (for reflex tests) |
| `reason_for_study` | String | OBR-31 | Reason for study |
| `principal_result_interpreter` | String | OBR-32 | Interpreting physician |
| `technician` | String | OBR-34 | Technician |
| `transcriptionist` | String | OBR-35 | Transcriptionist |
| `scheduled_datetime` | Timestamp | OBR-36 | Scheduled date/time |
| `_cf_message_id` | String | MSH-10 | Source message control ID |
| `_cf_message_datetime` | Timestamp | MSH-7 | Message timestamp |
| `_cf_source_hash` | String | — | Casparian lineage |
| `_cf_job_id` | String | — | Casparian lineage |
| `_cf_processed_at` | Timestamp | — | Casparian lineage |
| `_cf_parser_version` | String | — | Casparian lineage |

**Notes:**
- One ORU message typically has one OBR but can have multiple (panel orders)
- `order_id` is the FK used by `hl7_observations`
- Critical timestamps store both parsed and raw values for debugging

#### 5.2.4 `hl7_observations`

Extracted from OBX (Observation) segments in ORU messages.

| Column | Type | HL7 Source | Description |
|--------|------|------------|-------------|
| `observation_id` | String | Generated | Unique ID (message_id + OBX set_id) |
| `patient_id` | String | PID-3.1 | FK to patients |
| `visit_id` | String | PV1-19 | FK to visits (nullable) |
| `order_id` | String | OBR-2 | Placer order number |
| `filler_order_id` | String | OBR-3 | Filler order number |
| `observation_code` | String | OBX-3.1 | Observation identifier |
| `observation_code_system` | String | OBX-3.3 | Code system (LOINC, local) |
| `observation_code_text` | String | OBX-3.2 | Code display text |
| `value_type` | String | OBX-2 | Value type (NM, ST, CE, etc.) |
| `value_numeric` | Float | OBX-5 | Numeric value (if NM) |
| `value_string` | String | OBX-5 | String value |
| `value_coded` | String | OBX-5.1 | Coded value identifier |
| `value_coded_text` | String | OBX-5.2 | Coded value text |
| `units` | String | OBX-6 | Units of measure |
| `reference_range` | String | OBX-7 | Reference range |
| `abnormal_flags` | String | OBX-8 | Abnormal flag (H, L, A, etc.) |
| `result_status` | String | OBX-11 | Result status (F=Final, P=Preliminary) |
| `observation_datetime` | Timestamp | OBX-14 | Observation date/time |
| `producer_id` | String | OBX-15 | Producer's ID |
| `responsible_observer` | String | OBX-16 | Responsible observer |
| `observation_method` | String | OBX-17 | Observation method |
| `_cf_message_id` | String | MSH-10 | Source message control ID |
| `_cf_message_datetime` | Timestamp | MSH-7 | Message timestamp |
| `_cf_source_hash` | String | — | Casparian lineage |
| `_cf_job_id` | String | — | Casparian lineage |
| `_cf_processed_at` | Timestamp | — | Casparian lineage |
| `_cf_parser_version` | String | — | Casparian lineage |

**Notes:**
- One ORU message can have many OBX segments (one row each)
- `value_numeric` populated only when `value_type = 'NM'`
- LOINC codes enable cross-system analytics

#### 5.2.4 `hl7_messages` (Metadata/Audit)

Raw message metadata for traceability.

| Column | Type | HL7 Source | Description |
|--------|------|------------|-------------|
| `message_id` | String | MSH-10 | Message control ID |
| `message_datetime` | Timestamp | MSH-7 | Message date/time |
| `message_type` | String | MSH-9.1 | Message type (ADT, ORU, ORM) |
| `trigger_event` | String | MSH-9.2 | Trigger event (A01, R01, O01) |
| `message_structure` | String | MSH-9.3 | Message structure |
| `hl7_version` | String | MSH-12 | HL7 version (2.3, 2.5.1, etc.) |
| `sending_application` | String | MSH-3 | Sending application |
| `sending_facility` | String | MSH-4 | Sending facility |
| `receiving_application` | String | MSH-5 | Receiving application |
| `receiving_facility` | String | MSH-6 | Receiving facility |
| `processing_id` | String | MSH-11 | P=Production, T=Test, D=Debug |
| `patient_id` | String | PID-3.1 | Patient identifier (if present) |
| `segment_count` | Int | — | Number of segments in message |
| `raw_message_hash` | String | — | SHA256 of raw message |
| `parse_warnings` | String[] | — | Non-fatal parse issues |
| `source_file` | String | — | Original filename |
| `source_line` | Int | — | Line number (for batch files) |
| `_cf_source_hash` | String | — | Casparian lineage |
| `_cf_job_id` | String | — | Casparian lineage |
| `_cf_processed_at` | Timestamp | — | Casparian lineage |
| `_cf_parser_version` | String | — | Casparian lineage |

### 5.3 Quarantine Table

Malformed messages go to Casparian's standard quarantine.

| Column | Type | Description |
|--------|------|-------------|
| `id` | Int | Auto-increment |
| `source_file` | String | Original filename |
| `source_line` | Int | Line number |
| `raw_content` | String | Raw message (truncated to 10KB) |
| `error_type` | String | PARSE_ERROR, MISSING_MSH, ENCODING_ERROR |
| `error_message` | String | Detailed error |
| `_cf_source_hash` | String | Casparian lineage |
| `_cf_job_id` | String | Casparian lineage |
| `_cf_processed_at` | Timestamp | Casparian lineage |

---

## 6. Parser Implementation

### 6.1 Parser Class Structure

```python
# ~/.casparian_flow/parsers/hl7_adt.py
"""
HL7 v2.x ADT Parser - Extracts patient demographics and visit information.

Supports: ADT^A01 through ADT^A62 (all ADT trigger events)
Output tables: hl7_patients, hl7_visits, hl7_messages
"""

import pyarrow as pa
from hl7apy.parser import parse_message
from hl7apy.exceptions import HL7apyException

class HL7ADTParser:
    name = 'hl7_adt'
    version = '1.0.0'
    topics = ['hl7', 'hl7_adt', 'healthcare']

    outputs = {
        'hl7_patients': pa.schema([
            ('patient_id', pa.string()),
            ('patient_id_type', pa.string()),
            ('patient_id_authority', pa.string()),
            ('name_family', pa.string()),
            ('name_given', pa.string()),
            ('name_middle', pa.string()),
            ('birth_date', pa.date32()),
            ('gender', pa.string()),
            ('address_street', pa.string()),
            ('address_city', pa.string()),
            ('address_state', pa.string()),
            ('address_zip', pa.string()),
            ('phone_home', pa.string()),
            ('ssn', pa.string()),
            ('_cf_message_id', pa.string()),
            ('_cf_message_datetime', pa.timestamp('us')),
            # ... Casparian lineage columns added automatically
        ]),
        'hl7_visits': pa.schema([
            ('visit_id', pa.string()),
            ('patient_id', pa.string()),
            ('visit_class', pa.string()),
            ('location_unit', pa.string()),
            ('location_room', pa.string()),
            ('location_bed', pa.string()),
            ('admit_datetime', pa.timestamp('us')),
            ('discharge_datetime', pa.timestamp('us')),
            ('event_type', pa.string()),
            ('_cf_message_id', pa.string()),
            ('_cf_message_datetime', pa.timestamp('us')),
        ]),
        'hl7_messages': pa.schema([
            ('message_id', pa.string()),
            ('message_datetime', pa.timestamp('us')),
            ('message_type', pa.string()),
            ('trigger_event', pa.string()),
            ('hl7_version', pa.string()),
            ('sending_application', pa.string()),
            ('sending_facility', pa.string()),
            ('patient_id', pa.string()),
            ('parse_warnings', pa.list_(pa.string())),
            ('source_file', pa.string()),
        ]),
    }

    def parse(self, ctx):
        """
        Parse HL7 ADT messages from input file.

        Handles:
        - Single message per file (.hl7)
        - Batch files (multiple messages)
        - MLLP framing (auto-stripped)
        """
        messages = self._read_messages(ctx.input_path)

        patients = []
        visits = []
        message_meta = []

        for msg_text, line_num in messages:
            try:
                parsed = self._parse_message(msg_text)

                # Extract patient
                if parsed.get('patient'):
                    patients.append(parsed['patient'])

                # Extract visit
                if parsed.get('visit'):
                    visits.append(parsed['visit'])

                # Always log message metadata
                message_meta.append(parsed['metadata'])

            except HL7apyException as e:
                # Quarantine malformed messages
                yield ('_quarantine', {
                    'source_file': ctx.input_path,
                    'source_line': line_num,
                    'raw_content': msg_text[:10000],
                    'error_type': 'PARSE_ERROR',
                    'error_message': str(e),
                })

        if patients:
            yield ('hl7_patients', pd.DataFrame(patients))
        if visits:
            yield ('hl7_visits', pd.DataFrame(visits))
        if message_meta:
            yield ('hl7_messages', pd.DataFrame(message_meta))

    def _read_messages(self, path):
        """
        Read and split HL7 messages from file.

        CRITICAL: This MUST be a generator to handle large batch files.
        Hospitals dump 24 hours of data into single 2GB files.
        Loading entire file into memory will cause OOM.

        Yields: (message_text, line_number) tuples
        """
        import chardet

        # Step 1: Detect encoding (sniff first 8KB)
        with open(path, 'rb') as f:
            raw_sample = f.read(8192)
            detected = chardet.detect(raw_sample)
            encoding = detected['encoding'] or 'utf-8'

        # Step 2: Stream messages (never load full file)
        with open(path, 'r', encoding=encoding, errors='replace') as f:
            buffer = []
            line_num = 0
            msg_start_line = 1

            for line in f:
                line_num += 1
                stripped = line.strip()

                # Detect message boundaries
                if stripped.startswith('MSH|'):
                    # Yield previous message if exists
                    if buffer:
                        yield ('\r'.join(buffer), msg_start_line)
                    buffer = [stripped]
                    msg_start_line = line_num
                elif stripped:
                    buffer.append(stripped)

            # Yield final message
            if buffer:
                yield ('\r'.join(buffer), msg_start_line)

    def _parse_message(self, msg_text):
        """Parse single HL7 message, extract structured data."""
        pass
```

### 6.2 Dependencies

```toml
# pyproject.toml for hl7_adt parser
[project]
name = "hl7_adt_parser"
version = "1.0.0"
dependencies = [
    "hl7apy>=1.3.4",
    "pyarrow>=14.0.0",
    "pandas>=2.0.0",
    "chardet>=5.0.0",  # Encoding detection for legacy systems
]
```

**Why hl7apy:**
- Supports HL7 v2.1 through v2.8.2
- TOLERANT mode for real-world data
- Message validation capabilities
- MIT licensed
- Active maintenance

**Why chardet:**
- Older HL7 systems often output `ISO-8859-1` or `Windows-1252`
- Auto-detect encoding before parsing prevents crashes
- Sniff first 8KB of file to guess encoding

### 6.3 Parsing Strategy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         PARSING PIPELINE                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. READ FILE                                                           │
│     ├── Detect encoding (UTF-8, CP1252, Latin-1)                       │
│     ├── Detect format (single, batch, MLLP)                            │
│     └── Split into individual messages                                  │
│                                                                         │
│  2. PARSE MESSAGE (per message)                                         │
│     ├── hl7apy.parse_message(text, validation_level=TOLERANT)          │
│     ├── Extract MSH header → message metadata                          │
│     ├── Route by message type:                                          │
│     │   ├── ADT^* → extract PID, PV1                                   │
│     │   ├── ORU^* → extract PID, OBR, OBX[]                           │
│     │   └── Other → generic segment extraction                         │
│     └── Collect parse warnings (non-fatal issues)                      │
│                                                                         │
│  3. TRANSFORM                                                           │
│     ├── Normalize dates (HL7 YYYYMMDD → ISO 8601)                      │
│     ├── Normalize names (FAMILY^GIVEN^MIDDLE → structured)             │
│     ├── Handle repeating fields (PID-3 identifiers)                    │
│     └── Apply value mappings (gender codes, etc.)                      │
│                                                                         │
│  4. OUTPUT                                                              │
│     ├── Yield (table_name, dataframe) tuples                           │
│     └── Quarantine failures with context                               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.4 Lenient Parsing Rules

Real-world HL7 is messy. The parser prioritizes **data extraction over validation:**

| Issue | Handling | Example |
|-------|----------|---------|
| Unknown segment | Skip with warning | Z-segments (custom) |
| Missing required field | Use null | PV1-19 (visit ID) missing |
| Invalid date format | Try multiple formats, then null | `20240115` vs `2024-01-15` |
| Wrong field length | Truncate/accept | Name > 50 chars |
| Encoding errors | Replace with � | Invalid UTF-8 bytes |
| Duplicate segments | Keep all (array) | Multiple IN1 segments |
| Non-standard delimiters | Detect from MSH-1/2 | `|` vs `\|` |

**Philosophy:** Extract what we can, log what we couldn't, never crash.

---

## 7. User Workflows

### 7.1 Basic Workflow: Parse ADT Files

```
1. User has folder of HL7 files
   ~/exports/hl7_data/
   ├── adt_20240101.hl7
   ├── adt_20240102.hl7
   └── ... (1000 files)

2. User scans folder with Scout
   $ casparian scan ~/exports/hl7_data --tag hl7_adt

3. hl7_adt.py parser auto-binds via topic matching
   (Parser topics: ['hl7', 'hl7_adt', 'healthcare'])

4. User runs parser via TUI or CLI
   $ casparian run hl7_adt.py ~/exports/hl7_data/adt_20240101.hl7

   Or bulk process:
   $ casparian process --tag hl7_adt

5. Output appears in ~/.casparian_flow/output/
   ├── hl7_patients_{job_id}.parquet
   ├── hl7_visits_{job_id}.parquet
   └── hl7_messages_{job_id}.parquet

6. User queries with SQL
   $ casparian query "SELECT * FROM hl7_patients WHERE gender = 'F'"
```

### 7.2 TUI Workflow: Parser Bench

```
┌───────────────────────────────────────────────────────────────────────────────┐
│  PARSER BENCH                                                         Alt+P  │
├────────────────────┬──────────────────────────────────────────────────────────┤
│  PARSERS           │  hl7_adt v1.0.0                                          │
│  ~/.../parsers/    │  ─────────────────────────────────────                   │
│  ────────────────  │  Built-in HL7 v2.x ADT parser                            │
│  ● hl7_adt    1.0  │  Topics: hl7, hl7_adt, healthcare                        │
│  ● hl7_oru    1.0  │                                                          │
│  ○ hl7_generic     │  OUTPUTS                                                 │
│  ────────────────  │  ─────────────────────────────────────                   │
│  ► sales_parser    │  • hl7_patients (23 columns)                             │
│    log_analyzer    │  • hl7_visits (18 columns)                               │
│                    │  • hl7_messages (12 columns)                             │
│                    │                                                          │
│                    │  BOUND FILES (142 matched)                               │
│                    │  ─────────────────────────────────────                   │
│                    │  exports/adt_20240101.hl7     12KB  ○ pending            │
│                    │  exports/adt_20240102.hl7      8KB  ✓ processed          │
│                    │                                                          │
│  ────────────────  │  [Enter] Test with selected file                         │
│  [t] Test parser   │                                                          │
└────────────────────┴──────────────────────────────────────────────────────────┘
```

### 7.3 Workflow: Inspect Parsed Data

```
┌───────────────────────────────────────────────────────────────────────────────┐
│  INSPECT                                                              Alt+I  │
├────────────────────┬──────────────────────────────────────────────────────────┤
│  TABLES            │  hl7_patients                                            │
│  ────────────────  │  ─────────────────────────────────────                   │
│  ► hl7_patients    │  Rows: 12,847    Columns: 23    Size: 2.1 MB            │
│    hl7_visits      │                                                          │
│    hl7_observations│  COLUMN STATS                                            │
│    hl7_messages    │  ─────────────────────────────────────                   │
│                    │  patient_id    12,847 unique   0% null                   │
│                    │  name_family   11,203 unique   0.2% null                 │
│                    │  birth_date    8,941 unique    1.1% null                 │
│                    │  gender        4 unique        0% null                   │
│                    │                M: 6,102 (47.5%)                          │
│                    │                F: 6,701 (52.1%)                          │
│                    │                U: 44 (0.4%)                              │
│                    │                                                          │
│  ────────────────  │  PREVIEW (first 5 rows)                                  │
│  [q] SQL query     │  ─────────────────────────────────────                   │
│  [e] Export        │  │patient_id│name_family│birth_date│gender│             │
│  [Enter] Details   │  │MRN001    │DOE        │1980-05-15│M     │             │
│                    │  │MRN002    │SMITH      │1975-11-22│F     │             │
└────────────────────┴──────────────────────────────────────────────────────────┘
```

### 7.4 Workflow: Handle Parse Errors

```
$ casparian quarantine list --parser hl7_adt

┌─────────────────────────────────────────────────────────────────────────┐
│  QUARANTINE: hl7_adt                                        3 messages  │
├─────────────────────────────────────────────────────────────────────────┤
│  FILE                        ERROR                                      │
│  ────────────────────────────────────────────────────────────────────── │
│  adt_corrupt.hl7:1          PARSE_ERROR: Missing MSH segment            │
│  adt_batch.hl7:47           ENCODING_ERROR: Invalid UTF-8 at pos 234    │
│  adt_old.hl7:1              PARSE_ERROR: Unknown HL7 version '2.0'      │
└─────────────────────────────────────────────────────────────────────────┘

$ casparian quarantine show 1

Raw message (first 500 chars):
PID|1||MRN001^^^HOSPITAL^MR||DOE^JOHN...

Error: PARSE_ERROR: Missing MSH segment
Suggestion: HL7 messages must start with MSH segment. This appears to be
            a truncated message. Check source file for corruption.
```

---

## 8. TUI Integration Features

### 8.1 Rosetta Stone Inspector (Source Segment Highlighting)

**Problem:** HL7 is illegible. Analysts see `DOE^JOHN^Q` and wonder "where did this come from?"

**Solution:** When viewing parsed data in TUI Inspect mode, show the source HL7 segment with field highlighting.

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ INSPECT > hl7_patients > Row 1                                                 │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                │
│ PARSED DATA                                                                    │
│ ───────────────────────────────────────────────────────────────────────────── │
│ patient_id:  MRN001           name_family:  DOE                               │
│ name_given:  JOHN             name_middle:  Q                                 │
│ birth_date:  1980-05-15       gender:       M                                 │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ SOURCE SEGMENT (PID)                                                           │
│ ───────────────────────────────────────────────────────────────────────────── │
│ PID|1||MRN001^^^HOSPITAL^MR||DOE^JOHN^Q||19800515|M|||123 MAIN ST^^CHICAGO   │
│        ^^^^^^                 ^^^^^^^^^^^  ^^^^^^^^ ^                         │
│        PID-3.1                PID-5.1-3    PID-7    PID-8                     │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [←→] Navigate fields   [s] Toggle source   [Esc] Back                         │
└────────────────────────────────────────────────────────────────────────────────┘
```

**Implementation:**
- Store `_cf_message_id` on every row
- Lookup original message from `hl7_messages` table (if `include_raw_message=true`)
- Parse and highlight relevant fields based on column being viewed
- Toggle with `[s]` key in Inspect mode

### 8.2 Z-Segment Discovery (Dark Data Illuminator)

**Problem:** Hospitals put critical data in custom Z-segments (e.g., `ZPV|VIP_LEVEL|COVID_RISK`). These are silently ignored by default parsers.

**Solution:** Detect frequent unmapped segments and offer to generate parser extensions.

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ ⚠ UNMAPPED SEGMENTS DETECTED                                                   │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                │
│ Found 5,247 Z-segments that were not extracted:                               │
│                                                                                │
│ Segment   Count    Sample Content                                              │
│ ───────────────────────────────────────────────────────────────────────────── │
│ ZPV       4,102    ZPV|VIP_LEVEL_3|COVID_RISK_HIGH|ISOLATION_Y                │
│ ZPD       892      ZPD|PREFERRED_PHARMACY|CVS|123 MAIN ST                     │
│ ZIN       253      ZIN|SECONDARY_INS|AETNA|GRP12345                           │
│                                                                                │
│ These may contain valuable data specific to your organization.                │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [g] Generate parser extension   [i] Ignore   [?] What are Z-segments?         │
└────────────────────────────────────────────────────────────────────────────────┘
```

**When user presses `[g]`:**

1. Send to Claude Code sidebar:
   - Sample Z-segment content (10 examples)
   - Current parser schema
   - Prompt: "Generate a parser extension to extract these Z-segments"

2. AI generates code like:
```python
# Extension for ZPV (VIP/Isolation) segment
def extract_zpv(self, message):
    """Extract custom ZPV segment data."""
    zpv = message.segment('ZPV')
    if zpv:
        return {
            'vip_level': zpv[1].value if len(zpv) > 1 else None,
            'covid_risk': zpv[2].value if len(zpv) > 2 else None,
            'isolation_status': zpv[3].value if len(zpv) > 3 else None,
        }
    return {}
```

3. User reviews and applies to their parser copy

### 8.3 Current State Toggle

In TUI Inspect mode, offer view toggle:

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ INSPECT > hl7_patients                            [l] Log  [c] Current (active)│
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                │
│ Showing: CURRENT STATE (deduplicated by patient_id, latest message wins)      │
│ Rows: 12,847 unique patients (from 47,291 total ADT events)                   │
│                                                                                │
```

**Keybindings:**
- `[l]` - Log view: Show all rows (append-only transaction log)
- `[c]` - Current view: Deduplicated, latest-per-ID

**SQL equivalent:**
```sql
-- Current state query (what [c] does)
SELECT DISTINCT ON (patient_id) *
FROM hl7_patients
ORDER BY patient_id, _cf_message_datetime DESC
```

---

## 9. Configuration Options

### 9.1 Parser Configuration

The parser accepts configuration via environment variables or config file:

```yaml
# ~/.casparian_flow/config/hl7.yaml
hl7:
  # Parsing behavior
  validation_level: tolerant  # strict | tolerant
  encoding_fallback: [utf-8, cp1252, latin-1]
  max_message_size: 1048576   # 1MB per message

  # Output options
  deduplicate_patients: true  # Latest message wins per patient_id
  include_raw_message: false  # Store raw text in messages table

  # Date parsing
  date_formats:
    - "%Y%m%d"
    - "%Y%m%d%H%M%S"
    - "%Y-%m-%d"
    - "%Y-%m-%dT%H:%M:%S"

  # Field mappings (customize extraction)
  patient_id_field: "PID-3.1"  # Which component is primary ID

  # Z-segment handling
  z_segments:
    ZPD: ignore   # ignore | preserve | custom
    ZVT: preserve
```

### 9.2 Command Line Options

```bash
# Override validation level
$ casparian run hl7_adt.py input.hl7 --config validation_level=strict

# Include raw messages in output
$ casparian run hl7_adt.py input.hl7 --config include_raw_message=true

# Force specific encoding
$ casparian run hl7_adt.py input.hl7 --config encoding=cp1252
```

---

## 10. Error Handling & Edge Cases

### 10.1 Error Categories

| Category | Examples | Handling |
|----------|----------|----------|
| **Fatal** | File not found, permission denied | Fail job immediately |
| **IO/Network** | SMB timeout, stale NFS handle | Retry 3x with backoff, then fail |
| **Message-level** | Missing MSH, invalid structure | Quarantine message, continue |
| **Field-level** | Invalid date, wrong type | Use null, log warning |
| **Encoding** | Invalid UTF-8 bytes | Try fallbacks, replace with � |

> **Network Share Reality:** 90% of HL7 archives live on NAS (SMB/NFS), not local disk. Parser must tolerate network latency and transient IO failures without crashing.

### 10.2 Common Edge Cases

| Edge Case | Handling |
|-----------|----------|
| Empty file | Skip with warning |
| Binary/non-HL7 file | Detect via MSH check, quarantine |
| Extremely long fields | Truncate to 10,000 chars |
| Duplicate message IDs | Keep all, flag in metadata |
| Future dates | Accept (clock skew common) |
| Dates before 1900 | Accept (historical data) |
| Null bytes in message | Strip and warn |
| Mixed line endings | Normalize to `\r` (HL7 standard) |

### 10.3 Validation Warnings

Non-fatal issues are logged to `parse_warnings` array:

```json
{
  "message_id": "MSG001",
  "parse_warnings": [
    "PID-5: Name contains non-ASCII characters, preserved as-is",
    "PV1-3: Location format non-standard, parsed as single field",
    "ZPD: Unknown Z-segment ignored"
  ]
}
```

---

## 11. Security Considerations

### 11.1 PHI Handling

**CRITICAL:** HL7 messages contain Protected Health Information (PHI).

**Casparian's position:**
- Parser does NOT perform de-identification
- Output files contain PHI (same as input)
- Users must handle PHI according to their organization's policies
- Documentation clearly states: "Input and output may contain PHI"

**Recommendations for users:**
- Process only on authorized systems
- Use encrypted storage for output
- Apply de-identification before sharing
- Maintain audit logs

### 11.2 Input Validation

- Maximum file size: 100MB (configurable)
- Maximum message size: 1MB (configurable)
- No execution of embedded content
- Path traversal prevention

### 11.3 Audit Trail

All processing is logged via Casparian's standard lineage:
- `_cf_source_hash`: Hash of input file
- `_cf_job_id`: Processing job identifier
- `_cf_processed_at`: Processing timestamp
- `_cf_parser_version`: Parser version used

---

## 12. Implementation Phases

> **Implementation Strategy:** Build concrete parsers first (ADT, ORU), then refactor common code into generic base. Do NOT build generic parser first - you will over-engineer for edge cases that don't exist.

### Phase 1: Core ADT Parser (MVP)
- [ ] Implement `hl7_adt.py` parser class
- [ ] Streaming `_read_messages()` generator (critical for large files)
- [ ] Encoding detection with `chardet`
- [ ] PID segment extraction → `hl7_patients`
- [ ] PV1 segment extraction → `hl7_visits`
- [ ] MSH extraction → `hl7_messages`
- [ ] Raw datetime preservation for critical fields
- [ ] Basic quarantine handling
- [ ] Unit tests with sample ADT messages
- [ ] E2E test: folder of ADT files → queryable tables
- [ ] **Ship it** - get user feedback before Phase 2

### Phase 2: ORU Parser (Lab Results)
- [ ] Implement `hl7_oru.py` parser class
- [ ] OBR segment extraction → `hl7_orders` (critical linkage table)
- [ ] OBX segment extraction → `hl7_observations`
- [ ] Link observations to orders via `order_id`
- [ ] Handle multiple OBX per message
- [ ] Numeric vs string value handling
- [ ] Z-segment detection and counting (for discovery feature)
- [ ] Unit tests with sample ORU messages

### Phase 3: Current State Views & Deduplication
- [ ] Implement `hl7_patients_current` materialized view
- [ ] Implement `hl7_visits_current` materialized view
- [ ] Add deduplication logic (latest-per-ID)
- [ ] TUI toggle: Log vs Current state

### Phase 4: TUI Integration
- [ ] Parser auto-discovery in Parser Bench
- [ ] Rosetta Stone inspector (source segment highlighting)
- [ ] Z-segment discovery UI with AI code generation
- [ ] Current/Log toggle in Inspect mode
- [ ] HL7-specific file preview
- [ ] Parse error suggestions

### Phase 5: Refactor to Generic Base
- [ ] Extract common code from ADT/ORU into `hl7_base.py`
- [ ] Implement `hl7_generic.py` for raw segment extraction
- [ ] Configuration file support
- [ ] Batch file envelope handling (FHS/BHS)

### Phase 6: Additional Message Types
- [ ] ORM parser (orders)
- [ ] DFT parser (financial)
- [ ] SIU parser (scheduling)
- [ ] MDM parser (documents)

### Phase 7: Documentation & Polish
- [ ] User guide with examples
- [ ] Sample HL7 files for testing (synthetic, no PHI)
- [ ] Troubleshooting guide
- [ ] Performance benchmarks (target: 10K messages/minute)

---

## 13. Testing Strategy

### 13.1 Test Data

**Sample messages** (synthetic, no real PHI):

```
~/.casparian_flow/samples/hl7/
├── adt_a01_admit.hl7           # Standard admission
├── adt_a03_discharge.hl7       # Discharge
├── adt_a08_update.hl7          # Patient update
├── adt_batch.hl7               # Multiple messages
├── adt_minimal.hl7             # Bare minimum fields
├── adt_maximal.hl7             # All optional fields
├── adt_unicode.hl7             # International characters
├── adt_malformed.hl7           # Various errors
├── oru_r01_lab.hl7             # Lab result
├── oru_r01_vitals.hl7          # Vital signs
└── oru_batch.hl7               # Multiple results
```

### 13.2 Test Categories

| Category | Tests |
|----------|-------|
| **Unit** | Segment parsing, date normalization, encoding |
| **Integration** | Full message → DataFrame |
| **E2E** | Folder → Parquet → SQL query |
| **Edge cases** | Malformed, large, empty files |
| **Performance** | 10K messages in < 60s |

### 13.3 Validation Against Real Data

Partner with healthcare organizations to validate against anonymized real-world data:
- Parse success rate > 95%
- Field extraction accuracy > 99%
- Zero data corruption

---

## 14. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| HL7 version | v2.x only (for now) | 95% of dark data; FHIR is API-focused |
| Initial message types | ADT, ORU | Most common, highest value |
| Output format | Normalized tables | Analyst-friendly, no HL7 knowledge needed |
| Parsing mode | Lenient by default | Real-world data is messy |
| De-identification | Out of scope | Compliance is org-specific |
| Library | hl7apy | Best validation, version support |
| Distribution | Ships with Casparian | Zero setup for users |
| Configuration | Optional YAML | Works without config |
| **Log vs Current state** | Both (dual tables) | Analysts want current; audit needs history |
| **Orders table** | Required (`hl7_orders`) | Links visits to observations; critical for "which order?" |
| **Message reading** | Streaming generator | 2GB batch files are real; prevent OOM |
| **Encoding detection** | chardet sniffing | Legacy systems use ISO-8859-1, CP1252 |
| **Raw date storage** | Critical fields only | Admit/discharge/DOB need debugging; not all dates |
| **Implementation order** | ADT → ORU → refactor | Concrete first, abstract later |
| **Parser distribution** | Copyable file, not baked in | Users can modify; updates via `casparian update-parsers` |

---

## 15. Future Considerations

### 15.1 FHIR Support (Future)

```
~/.casparian_flow/parsers/
├── hl7_adt.py          # HL7 v2.x
├── fhir_patient.py     # FHIR R4 Patient resources
├── fhir_encounter.py   # FHIR R4 Encounter resources
└── fhir_observation.py # FHIR R4 Observation resources
```

FHIR is JSON-based, so parsing is simpler. Output schema would align with HL7 v2.x tables for consistency.

### 15.2 Streaming Mode

For real-time feeds (Kafka, MLLP listeners):

```python
class HL7ADTParser:
    streaming = True  # Enable streaming mode

    def parse_message(self, msg_text):
        """Parse single message (called per message in stream)."""
        pass
```

### 15.3 De-identification Plugin

Optional wrapper parser:

```python
class HL7ADTDeidentified(HL7ADTParser):
    """ADT parser with Safe Harbor de-identification."""

    def transform_patient(self, patient):
        patient['name_family'] = 'REDACTED'
        patient['ssn'] = None
        patient['birth_date'] = shift_date(patient['birth_date'])
        return patient
```

---

## 16. Glossary

| Term | Definition |
|------|------------|
| **ADT** | Admit/Discharge/Transfer - patient movement messages |
| **ORU** | Observation Result Unsolicited - lab/clinical results |
| **ORM** | Order Message - clinical orders |
| **MSH** | Message Header segment - metadata about the message |
| **PID** | Patient Identification segment - demographics |
| **PV1** | Patient Visit segment - encounter information |
| **OBX** | Observation segment - individual result values |
| **MLLP** | Minimal Lower Layer Protocol - TCP framing for HL7 |
| **Trigger Event** | Specific action that caused message (A01=admit) |
| **Z-segment** | Custom/local segment (non-standard) |
| **PHI** | Protected Health Information (HIPAA term) |

---

## 17. References

- [HL7 v2.x Standard](https://www.hl7.org/implement/standards/product_brief.cfm?product_id=185)
- [hl7apy Documentation](https://crs4.github.io/hl7apy/)
- [HL7 Message Types Guide](https://www.interfaceware.com/hl7-standard/hl7-messages.html)
- [HIPAA PHI Guidelines](https://www.hhs.gov/hipaa/for-professionals/privacy/special-topics/de-identification/index.html)

---

## 18. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft |
| 2026-01-08 | 0.2 | **Post-review revision:** Added `hl7_orders` table, current state views, streaming generator, chardet encoding, raw date fields, TUI features (Rosetta Stone, Z-segment discovery), updated implementation phases |


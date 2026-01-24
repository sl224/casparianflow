# Parameterizable Parser Generator (PPG) Specification

**Status:** Reference (post-v1)
**Version:** 1.3.0
**Date:** 2026-01-16
**Parent:** `docs/schema_rfc.md`
**Related:** `specs/parsers/hl7_parser.md`, `strategies/healthcare_hl7.md`

---

## 1. Overview

**Note:** PPG is not in v1 scope; this spec is kept for future design work.

### 1.1 Purpose

The Parameterizable Parser Generator (PPG) enables technical analysts and IT staff to create parsers for structured message formats (HL7, FIX, ISO 20022) via configuration rather than code. It generates RFC-compliant parsers that emit Arrow RecordBatches with proper schema contracts.

### 1.2 Target Users

| User Type | Description | NOT Target |
|-----------|-------------|------------|
| Technical Analyst | SQL-literate, understands data structures | Business users with no technical background |
| Integration Engineer | Familiar with HL7/FIX/EDI formats | Developers who want full code control |
| IT Staff | Can edit JSON/YAML config files | No-code users expecting drag-and-drop UI |

**Positioning:** Low-code for analysts/IT, not no-code for business users.

### 1.3 Design Principles

| Principle | Implementation |
|-----------|----------------|
| RFC-aligned types (v1 subset) | Use dict format (`{"kind": "string"}`), not string shorthand; v1 supports primitives + Decimal + timestamp_tz |
| Arrow-first output | Generated parsers emit `pyarrow.RecordBatch`, never Pandas |
| Child tables for 1:N | Repeating segments become separate outputs with FK joins |
| Soft-fail for quarantine | Row errors can yield nulls + `_cf_row_error`; Rust validation remains authoritative |
| Explicit repeat semantics | Path syntax includes repetition strategy |

### 1.4 v1 Constraints

- Output columns must be flat (no `list`/`struct`) unless a feature flag is enabled.
- Use child tables for repeating segments and 1:N relationships.
- If nested data is unavoidable, encode it as JSON in a String column.

---

## 2. Configuration Schema

### 2.1 Top-Level Structure

```json
{
  "parser_name": "hospital_a_adt",
  "version": "1.0.0",
  "topics": ["hl7_adt"],
  "format": "hl7v2",

  "settings": {
    "delimiter": "|",
    "component_separator": "^",
    "escape_character": "\\",
    "segment_terminator": "\r",
    "default_repeat_strategy": "first"
  },

  "outputs": {
    "patients": { ... },
    "diagnoses": { ... }
  },

  "mappings": [ ... ]
}
```

### 2.2 Settings by Format

| Format | Required Settings | Optional Settings |
|--------|-------------------|-------------------|
| `hl7v2` | `delimiter`, `component_separator` | `escape_character`, `segment_terminator` |
| `fix` | `delimiter` (SOH default) | `checksum_validation` |
| `csv` | `delimiter`, `has_header` | `quote_char`, `escape_char` |
| `json` | (none) | `json_path_root` |
| `xml` | (none) | `namespace_map` |

### 2.3 Output Definitions

Each output maps to one RFC `outputs` entry:

```json
"outputs": {
  "patients": {
    "mode": "strict",
    "description": "One row per ADT message",
    "validation": {
      "list_policy": "full"  // "full" | "sample(N)"
    }
  },
  "diagnoses": {
    "mode": "strict",
    "description": "One row per DG1 segment (child of patients)",
    "parent": "patients",
    "parent_key": "message_id"
  }
}
```

**Validation list_policy values:**

| Value | Behavior |
|-------|----------|
| `"full"` (default) | Validate all list elements; fail if any element is invalid |
| `"sample(N)"` | Validate first N elements; skip validation on remaining |

`sample(N)` is useful for large lists where full validation is too expensive. Example: `"list_policy": "sample(100)"` validates first 100 elements.

| Field | Type | Description |
|-------|------|-------------|
| `mode` | `"strict"` \| `"allow_extra"` \| `"allow_missing_optional"` | Validation mode (RFC 6.3) |
| `description` | string | Human-readable purpose |
| `parent` | string? | Parent output name for child tables |
| `parent_key` | string \| string[]? | Column(s) in parent used for FK join (must be non-nullable) |
| `validation` | object? | Per-output validation settings |

**Mode definitions (RFC 6.3):**
- `strict`: All declared columns must be present with exact types. No extra columns allowed.
- `allow_extra`: Declared columns validated; extra columns preserved but not validated.
- `allow_missing_optional`: Nullable columns can be missing; non-nullable must be present.

**Parent key constraint:** When `parent` is specified, `parent_key` must reference non-nullable column(s) in the parent output that are unique per parent row (natural key or generated ID).

**Composite parent keys:** When MSH-10 (message_id) is not globally unique (e.g., same ID across different source files), use a composite key:

```json
"outputs": {
  "patients": {
    "mode": "strict",
    "description": "One row per ADT message"
  },
  "diagnoses": {
    "mode": "strict",
    "parent": "patients",
    "parent_key": ["source_file_id", "message_id"]  // Composite key
  }
}
```

When `parent_key` is an array:
- Child table includes ALL key columns with `_cf_parent_key_*` naming
- FK join uses tuple equality
- Generated code validates ALL key components are non-null

---

## 3. Mapping Specification

### 3.1 Column Mapping

Basic column extraction:

```json
{
  "output": "patients",
  "column_name": "patient_mrn",
  "path": "PID-3-1",
  "type": {"kind": "string"},
  "nullable": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `output` | string | Yes | Target output name |
| `column_name` | string | Yes | Output column name |
| `path` | string | Yes | Source path expression (see 3.3) |
| `type` | object | Yes | RFC type dict (see 3.2) |
| `nullable` | boolean | Yes | Whether null is allowed |
| `default` | any | No | Default value if path not found |
| `transform` | string | No | Built-in transform function (see 3.4) |

### 3.2 Type Notation (RFC-Aligned)

**CRITICAL:** Use RFC dict format, NOT string shorthand.

```json
// CORRECT - RFC-aligned
{"kind": "string"}
{"kind": "int64"}
{"kind": "decimal", "precision": 18, "scale": 8}
{"kind": "timestamp_tz", "tz": "UTC"}
{"kind": "list", "item": {"kind": "string"}, "item_nullable": true}

// WRONG - string shorthand (not supported)
"string"
"list<string>"
"decimal(18,8)"
```

**v1 constraint:** Only primitives + Decimal + timestamp_tz are approved. `list` and `struct` remain reserved for v1.1 unless feature-flagged.

### 3.3 Path Syntax

#### 3.3.1 Basic Paths

| Format | Syntax | Example | Meaning |
|--------|--------|---------|---------|
| HL7 v2 | `SEG-field-component` | `PID-3-1` | Segment.field.component |
| HL7 v2 | `SEG-field` | `PID-5` | Entire field (all components) |
| FIX | `tag` | `35` | Tag number |
| FIX | `group.tag` | `453.448` | Repeating group tag |
| JSON | JSONPath | `$.patient.name` | JSONPath expression |
| XML | XPath | `/msg/patient/@id` | XPath expression |

**Path complexity limits (XML/JSON):**

XPath and JSONPath expressions can be expensive on large documents. PPG enforces:

| Limit | Value | Rationale |
|-------|-------|-----------|
| Max path depth | 20 levels | Prevents pathological nesting |
| Max wildcards | 3 per path | Prevents `//*//*` explosion |
| Disallowed axes | `ancestor`, `preceding`, `following` | O(n²) complexity |
| Max path length | 500 characters | Prevents abuse |

Config validation rejects paths exceeding these limits.

#### 3.3.2 Repetition Syntax

HL7 and FIX have repeating fields/segments. Use explicit repetition markers:

| Syntax | Meaning | Example |
|--------|---------|---------|
| `SEG-field` | Apply `default_repeat_strategy` | `PID-5` (first name only if default=first) |
| `SEG-field[0]` | First occurrence | `PID-5[0]` |
| `SEG-field[*]` | All occurrences (list or child table) | `PID-5[*]` |
| `SEG-field[-1]` | Last occurrence | `PID-5[-1]` |
| `SEG[*]` | All segment instances | `DG1[*]` (all diagnosis segments) |
| `SEG[0..3]` | Range of occurrences | `OBX[0..3]` (first 3 OBX segments) |

#### 3.3.3 HL7 Path Translation (hl7apy)

PPG uses a user-friendly path syntax that matches HL7 documentation conventions. Generated parsers translate this to hl7apy's attribute-based API:

| PPG Path | hl7apy Translation | Description |
|----------|-------------------|-------------|
| `MSH-10` | `msg.msh.msh_10.value` | Message Control ID |
| `PID-3-1` | `msg.pid.pid_3.pid_3_1.value` | Patient ID, first component |
| `PID-5[0]-1` | `msg.pid.pid_5[0].pid_5_1.value` | First patient name, family name |
| `DG1[*]` | `list(msg.dg1)` | All DG1 segments (iteration) |
| `DG1-3-1` | `segment.dg1_3.dg1_3_1.value` | Diagnosis code (from segment) |
| `OBX-5` | `segment.obx_5.value` | Observation value (entire field) |

**Translation Layer:** Generated parsers include an `_extract()` method that handles this translation automatically. Users never need to know hl7apy's API.

### 3.4 Built-in Transforms

| Transform | Input | Output | Example |
|-----------|-------|--------|---------|
| `trim` | string | string | `"  abc  "` → `"abc"` |
| `upper` | string | string | `"abc"` → `"ABC"` |
| `lower` | string | string | `"ABC"` → `"abc"` |
| `parse_date` | string | date | `"20240115"` → date |
| `parse_datetime` | string | timestamp | `"20240115120000"` → timestamp |
| `parse_hl7_date` | string | date | HL7 date format |
| `parse_hl7_datetime` | string | timestamp_tz | HL7 datetime with TZ |
| `to_int` | string | int64 | `"123"` → 123 |
| `to_decimal` | string | decimal | `"123.45"` → Decimal |
| `join` | list | string | `["a","b"]` → `"a,b"` |
| `first` | list | item | `["a","b"]` → `"a"` |
| `count` | list | int64 | `["a","b"]` → 2 |

**Transform parameterization (`transform_args`):**

Some transforms accept optional arguments for customization:

```json
{
  "output": "patients",
  "column_name": "birth_date",
  "path": "PID-7",
  "type": {"kind": "date"},
  "nullable": true,
  "transform": "parse_date",
  "transform_args": {
    "format": "%Y%m%d"
  }
}
```

| Transform | Supported Args | Default |
|-----------|----------------|---------|
| `parse_date` | `format` (strptime pattern) | `"%Y%m%d"` |
| `parse_datetime` | `format` (strptime pattern) | `"%Y%m%d%H%M%S"` |
| `join` | `separator` | `","` |
| `to_decimal` | `decimal_separator` | `"."` |

Transforms without `transform_args` support ignore any provided args (no error).

**Transform registry:**

PPG validates transforms at config load time against a fixed registry. Unknown transforms are rejected:

```
ERROR: Unknown transform 'parse_custom' in mapping 'patient_dob'.
       Available: trim, upper, lower, parse_date, parse_datetime,
                  parse_hl7_date, parse_hl7_datetime, to_int, to_decimal,
                  join, first, count
```

**Versioning:** Transform implementations are bundled with PPG. The generated parser embeds transform code, ensuring reproducibility regardless of PPG version changes. Config files should record the PPG version used for generation (in `_ppg_version` metadata field).

---

## 4. Child Table Mapping

### 4.1 Purpose

For 1:N relationships (HL7 repeating segments, FIX repeating groups), child tables are **preferred over list columns** for analytics:

| Approach | Pros | Cons |
|----------|------|------|
| List column | Single output | Hard to query in SQL, validation complexity |
| **Child table** | SQL-friendly joins, per-row validation | Multiple outputs to manage |

**Recommendation:** Default to child tables for repeating segments.

### 4.2 Child Table Definition

```json
{
  "output": "diagnoses",
  "kind": "child_table",
  "parent_output": "patients",
  "parent_key": "message_id",
  "path": "DG1[*]",
  "repeat_strategy": "explode",

  "columns": [
    {
      "name": "diag_code",
      "path": "DG1-3-1",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "name": "diag_desc",
      "path": "DG1-3-2",
      "type": {"kind": "string"},
      "nullable": true
    },
    {
      "name": "diag_type",
      "path": "DG1-6",
      "type": {"kind": "string"},
      "nullable": true
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `kind` | `"child_table"` | Marks this as a child table mapping |
| `parent_output` | string | Name of parent output (can be another child table for nesting) |
| `parent_key` | string | Column in parent for join (must exist in parent) |
| `path` | string | Repeating segment selector (e.g., `DG1[*]`) |
| `path_relative_to` | `"root"` \| `"parent"` | How to interpret path (default: `"root"`) |
| `repeat_strategy` | enum | How to handle repeats (see 4.3) |
| `columns` | array | Column definitions relative to each segment |

**Nested Child Tables (Grandchildren):**

For deeply nested structures (e.g., OBX segments within OBR within a message), use `path_relative_to: "parent"`:

```json
{
  "output": "observations",
  "kind": "child_table",
  "parent_output": "orders",
  "parent_key": "order_id",
  "path": "OBX[*]",
  "path_relative_to": "parent",
  "repeat_strategy": "explode",
  "columns": [ ... ]
}
```

| Value | Behavior |
|-------|----------|
| `"root"` (default) | Path is evaluated from the message root |
| `"parent"` | Path is evaluated relative to each parent record's segment |

**Maximum nesting depth:** 10 levels (per RFC Section 5.4.3)

### 4.3 Repeat Strategies

| Strategy | Applies To | Behavior | Output |
|----------|------------|----------|--------|
| `explode` | Child tables | One output row per repeat | N child rows for N repeats |
| `first` | Child tables, columns | Take first occurrence only | 1 child row (or 0 if none) |
| `last` | Child tables, columns | Take last occurrence only | 1 child row (or 0 if none) |
| `all` | **Columns only** | Collect into list column | 1 row with list column |

**Default:** `explode` for child tables.

**IMPORTANT:** `repeat_strategy: all` is incompatible with `kind: "child_table"`. The `all` strategy produces a list column within the parent output, not a separate child table. Use `all` only in column mappings:

```json
// CORRECT: all for column mapping (produces list column in patients)
{
  "output": "patients",
  "column_name": "all_diagnosis_codes",
  "path": "DG1[*]-3-1",
  "type": {"kind": "list", "item": {"kind": "string"}},
  "nullable": true,
  "repeat_strategy": "all"
}

// WRONG: all with child_table (semantic conflict)
{
  "output": "diagnoses",
  "kind": "child_table",
  "repeat_strategy": "all"  // ERROR: Cannot use 'all' with child_table
}
```

Config validation will reject `repeat_strategy: all` on child table mappings.

### 4.4 Generated Lineage Columns

PPG-generated schemas automatically include lineage columns. These columns are split between **schema-declared** (PPG generates) and **runtime-injected** (bridge adds at write time).

#### 4.4.1 Schema-Declared Columns (PPG Generates)

These columns are included in the generated Arrow schema:

| Column | Type | Generated For | Description |
|--------|------|---------------|-------------|
| `_cf_row_id` | String | All outputs | UUID uniquely identifying this output row |
| `_cf_row_error` | String (nullable) | All outputs | Error message if row failed extraction |
| `_cf_segment_index` | Int64 | Child tables only | 0-based index of this segment within its parent |
| `_cf_parent_key` | (matches parent key type) | Child tables only | Copy of parent's join key value |

#### 4.4.2 Runtime-Injected Columns (Bridge Adds)

These columns are NOT in the generated schema but are injected by the Rust bridge at write time:

| Column | Type | Injected By | Description |
|--------|------|-------------|-------------|
| `_cf_source_hash` | String | Bridge | Blake3 hash of the source file |
| `_cf_job_id` | String | Bridge | UUID identifying this processing run |
| `_cf_processed_at` | String (ISO 8601) | Bridge | Timestamp when record was processed |
| `_cf_parser_version` | String | Bridge | Parser version from parser.version attribute |

**Why this split?**
- PPG generates schemas at config time, before runtime context exists
- Bridge has access to job_id, source_hash, timestamp at runtime
- This avoids parsers needing to know about job lifecycle

#### 4.4.3 Quarantine-Specific Columns

When rows fail validation, the bridge creates quarantine files with additional columns (per RFC Section 6.7.2):

| Column | Type | Description |
|--------|------|-------------|
| `_cf_source_row` | Int64 | Source row number (if available) |
| `_cf_output_row` | Int64 | Row index in output batch |
| `_error_msg` | String | Detailed violation message |
| `_violation_type` | String | Enum: `null_not_allowed`, `type_mismatch`, etc. |
| `_raw_data` | String | JSON-serialized original row |

PPG does NOT generate quarantine schemas; the bridge creates them dynamically based on the output schema.

#### 4.4.4 List Element Nullability (`item_nullable`)

When using `repeat_strategy: all` to produce list columns, the `item_nullable` field controls whether list elements can be null:

```json
{
  "column_name": "all_diagnosis_codes",
  "path": "DG1[*]-3-1",
  "type": {
    "kind": "list",
    "item": {"kind": "string"},
    "item_nullable": true  // List can contain null elements
  },
  "nullable": true  // The list itself can be null
}
```

| Field | Meaning |
|-------|---------|
| `nullable: true` | The entire list column can be null (no values extracted) |
| `item_nullable: true` | Individual list elements can be null (extraction failed for some) |

Generated schema includes both nullability flags per RFC 6.2.1.

#### 4.4.5 Multi-Output Schema Fingerprinting

Each PPG output generates a **schema fingerprint** at generation time. This is NOT the final `contract_id` (which is assigned at approval time by the schema subsystem per RFC 6.4).

```json
{
  "outputs": {
    "patients": {
      "mode": "strict",
      "schema_fingerprint": "patients_v1.0.0_abc123"  // Computed at generation
    },
    "diagnoses": {
      "mode": "strict",
      "parent": "patients",
      "parent_schema_fingerprint": "patients_v1.0.0_abc123",  // Must match parent
      "schema_fingerprint": "diagnoses_v1.0.0_def456"
    }
  }
}
```

**Schema fingerprint derivation:** `{output_name}_v{version}_{hash}` where hash is SHA-256 of normalized column definitions.

**Distinction from contract_id:**

| Field | When Assigned | By Whom | Purpose |
|-------|---------------|---------|---------|
| `schema_fingerprint` | PPG generation time | PPG generator | Deterministic schema identity |
| `contract_id` | Schema approval time | Schema subsystem | Immutable contract binding (UUID) |

**Validation at generation:**
1. Parent `schema_fingerprint` exists in outputs
2. Parent key column exists in parent schema
3. Child references correct parent fingerprint

**Validation at approval (runtime):**
- Schema subsystem assigns `contract_id` (UUID) to each output
- Links child contract to parent contract via FK relationship
- Stores in schema registry per RFC 6.9

This enables independent schema evolution while maintaining referential integrity

### 4.5 Example: HL7 ADT with Diagnoses

**Config:**
```json
{
  "parser_name": "hospital_a_adt",
  "version": "1.0.0",
  "topics": ["hl7_adt"],
  "format": "hl7v2",

  "settings": {
    "delimiter": "|",
    "component_separator": "^"
  },

  "outputs": {
    "patients": {
      "mode": "strict",
      "description": "One row per ADT message"
    },
    "diagnoses": {
      "mode": "strict",
      "parent": "patients",
      "parent_key": "message_id"
    }
  },

  "mappings": [
    {
      "output": "patients",
      "column_name": "message_id",
      "path": "MSH-10",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "output": "patients",
      "column_name": "patient_mrn",
      "path": "PID-3-1",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "output": "patients",
      "column_name": "patient_name",
      "path": "PID-5-1",
      "type": {"kind": "string"},
      "nullable": true
    },
    {
      "output": "diagnoses",
      "kind": "child_table",
      "parent_output": "patients",
      "parent_key": "message_id",
      "path": "DG1[*]",
      "repeat_strategy": "explode",
      "columns": [
        {
          "name": "diag_code",
          "path": "DG1-3-1",
          "type": {"kind": "string"},
          "nullable": false
        },
        {
          "name": "diag_desc",
          "path": "DG1-3-2",
          "type": {"kind": "string"},
          "nullable": true
        }
      ]
    }
  ]
}
```

**Generated RFC Outputs:**
```python
outputs = {
    "patients": {
        "mode": "strict",
        "columns": [
            {"name": "message_id", "type": {"kind": "string"}, "nullable": False},
            {"name": "patient_mrn", "type": {"kind": "string"}, "nullable": False},
            {"name": "patient_name", "type": {"kind": "string"}, "nullable": True},
            {"name": "_cf_row_id", "type": {"kind": "string"}, "nullable": False},
            {"name": "_cf_row_error", "type": {"kind": "string"}, "nullable": True}
        ]
    },
    "diagnoses": {
        "mode": "strict",
        "columns": [
            {"name": "message_id", "type": {"kind": "string"}, "nullable": False},  # FK
            {"name": "diag_code", "type": {"kind": "string"}, "nullable": False},
            {"name": "diag_desc", "type": {"kind": "string"}, "nullable": True},
            {"name": "_cf_row_id", "type": {"kind": "string"}, "nullable": False},
            {"name": "_cf_segment_index", "type": {"kind": "int64"}, "nullable": False},
            {"name": "_cf_parent_key", "type": {"kind": "string"}, "nullable": False},  # Copy of parent key
            {"name": "_cf_row_error", "type": {"kind": "string"}, "nullable": True}
        ]
    }
}
```

---

## 5. Code Generation

### 5.1 Output Requirements

Generated parsers MUST:

1. **Emit Arrow RecordBatch** - Never Pandas DataFrames
2. **Preserve Decimal precision** - Use `pyarrow.decimal128`
3. **Include timezone** - Use `pyarrow.timestamp` with explicit tz
4. **Yield multiple outputs** - `yield (output_name, batch)` tuples
5. **Soft-fail row errors** - Catch exceptions, yield nulls + `_cf_row_error`
6. **Stream large files** - Use message/line iterators, not full-file loading (see 5.4)

### 5.1.1 Large File Handling (Streaming)

**CRITICAL:** Generated parsers MUST NOT load entire files into memory. For HL7/FIX message archives that can exceed available RAM:

```python
# WRONG: OOM risk for large files
with open(ctx.input_path, 'r') as f:
    content = f.read()  # Loads entire file
messages = content.split('MSH|')

# RIGHT: Streaming message iterator
def _iter_messages(self, file_path: str) -> Iterator[str]:
    """Stream messages without loading entire file."""
    buffer = []
    with open(file_path, 'r') as f:
        for line in f:
            if line.startswith('MSH|') and buffer:
                yield ''.join(buffer)
                buffer = []
            buffer.append(line)
        if buffer:
            yield ''.join(buffer)
```

**Batch yielding:** For very large files, yield batches periodically (e.g., every 10,000 rows) rather than accumulating all rows before yielding. This bounds memory usage.

| File Size | Approach |
|-----------|----------|
| < 100 MB | In-memory acceptable |
| 100 MB - 1 GB | Streaming required |
| > 1 GB | Streaming + periodic batch yields |

### 5.2 Generated Parser Structure

Generated parsers MUST implement two entry points:
1. `parse(ctx)` - File-based streaming for normal processing
2. `process_message(raw_msg)` - String-based for Quarantine Replay (RFC 6.7.4)

```python
import pyarrow as pa
from decimal import Decimal
from typing import Iterator, Tuple, Dict, List, Optional
import uuid

class GeneratedParser:
    name = 'hospital_a_adt'
    version = '1.0.0'
    topics = ['hl7_adt']

    # Batch size for synchronized flush (bounds memory)
    BATCH_SIZE = 10000

    # Auto-generated from PPG config (full schema with all lineage columns)
    outputs = {
        "patients": {
            "mode": "strict",
            "columns": [
                {"name": "message_id", "type": {"kind": "string"}, "nullable": False},
                {"name": "patient_mrn", "type": {"kind": "string"}, "nullable": False},
                {"name": "patient_name", "type": {"kind": "string"}, "nullable": True},
                {"name": "_cf_row_id", "type": {"kind": "string"}, "nullable": False},
                {"name": "_cf_row_error", "type": {"kind": "string"}, "nullable": True}
            ]
        },
        "diagnoses": {
            "mode": "strict",
            "columns": [
                {"name": "message_id", "type": {"kind": "string"}, "nullable": False},
                {"name": "diag_code", "type": {"kind": "string"}, "nullable": False},
                {"name": "diag_desc", "type": {"kind": "string"}, "nullable": True},
                {"name": "_cf_row_id", "type": {"kind": "string"}, "nullable": False},
                {"name": "_cf_segment_index", "type": {"kind": "int64"}, "nullable": False},
                {"name": "_cf_parent_key", "type": {"kind": "string"}, "nullable": False},
                {"name": "_cf_row_error", "type": {"kind": "string"}, "nullable": True}
            ]
        }
    }

    # Arrow schemas (pre-computed, includes ALL columns)
    _patients_schema = pa.schema([
        pa.field("message_id", pa.string(), nullable=False),
        pa.field("patient_mrn", pa.string(), nullable=False),
        pa.field("patient_name", pa.string(), nullable=True),
        pa.field("_cf_row_id", pa.string(), nullable=False),
        pa.field("_cf_row_error", pa.string(), nullable=True),
    ])

    _diagnoses_schema = pa.schema([
        pa.field("message_id", pa.string(), nullable=False),
        pa.field("diag_code", pa.string(), nullable=False),
        pa.field("diag_desc", pa.string(), nullable=True),
        pa.field("_cf_row_id", pa.string(), nullable=False),
        pa.field("_cf_segment_index", pa.int64(), nullable=False),
        pa.field("_cf_parent_key", pa.string(), nullable=False),
        pa.field("_cf_row_error", pa.string(), nullable=True),
    ])

    # =========================================================================
    # Entry Point 1: File-based Streaming (Normal Processing)
    # =========================================================================
    def parse(self, ctx) -> Iterator[Tuple[str, pa.RecordBatch]]:
        """Parse HL7 file using streaming. NEVER loads entire file into memory."""
        # Initialize buffers for synchronized flush
        buffers: Dict[str, List[dict]] = {
            "patients": [],
            "diagnoses": []
        }

        # Stream messages from file (see _iter_messages)
        for msg_index, raw_msg in enumerate(self._iter_messages(ctx.input_path)):
            # Delegate to pure processing logic
            result = self.process_message(raw_msg, msg_index)

            # Accumulate results
            for output_name, rows in result.items():
                buffers[output_name].extend(rows)

            # SYNCHRONIZED FLUSH: When ANY buffer exceeds limit, flush ALL
            if any(len(buf) >= self.BATCH_SIZE for buf in buffers.values()):
                yield from self._flush_all(buffers)

        # Final flush
        yield from self._flush_all(buffers)

    # =========================================================================
    # Entry Point 2: String-based (Quarantine Replay per RFC 6.7.4)
    # =========================================================================
    def process_message(self, raw_msg: str, msg_index: int = 0) -> Dict[str, List[dict]]:
        """
        Process a single message string. Used by:
        - parse() for normal file processing
        - Quarantine Replay CLI for reprocessing failed rows

        Returns: {'patients': [row_dict], 'diagnoses': [row_dict, ...]}
        """
        from hl7apy.parser import parse_message

        result: Dict[str, List[dict]] = {"patients": [], "diagnoses": []}
        row_id = str(uuid.uuid4())

        # Stage 1: Parse message (soft-fail)
        try:
            msg = parse_message(raw_msg.strip())
        except Exception as e:
            # Parse failure: emit error row, skip children
            result["patients"].append({
                "message_id": None,
                "patient_mrn": None,
                "patient_name": None,
                "_cf_row_id": row_id,
                "_cf_row_error": f"Parse error: {e}",
            })
            return result

        # Stage 2: Extract parent key (CRITICAL for child rows)
        message_id = self._extract(msg, "MSH-10")
        parent_key_valid = message_id is not None

        # Build patient row
        patient_row = {
            "message_id": message_id,
            "patient_mrn": self._extract(msg, "PID-3-1"),
            "patient_name": self._extract(msg, "PID-5-1"),
            "_cf_row_id": row_id,
            "_cf_row_error": None if parent_key_valid else "Parent key (MSH-10) missing",
        }
        result["patients"].append(patient_row)

        # Stage 3: Extract child rows (only if parent key valid)
        if parent_key_valid:
            for idx, dg1 in enumerate(self._get_segments(msg, "DG1")):
                result["diagnoses"].append({
                    "message_id": message_id,  # FK to parent
                    "diag_code": self._extract_from_segment(dg1, "DG1-3-1"),
                    "diag_desc": self._extract_from_segment(dg1, "DG1-3-2"),
                    "_cf_row_id": str(uuid.uuid4()),
                    "_cf_segment_index": idx,
                    "_cf_parent_key": message_id,  # Copy of FK for lineage
                    "_cf_row_error": None,
                })
        else:
            # Parent key missing: mark ALL potential children as errors (orphan prevention)
            for idx, dg1 in enumerate(self._get_segments(msg, "DG1")):
                result["diagnoses"].append({
                    "message_id": None,
                    "diag_code": self._extract_from_segment(dg1, "DG1-3-1"),
                    "diag_desc": self._extract_from_segment(dg1, "DG1-3-2"),
                    "_cf_row_id": str(uuid.uuid4()),
                    "_cf_segment_index": idx,
                    "_cf_parent_key": None,
                    "_cf_row_error": "Parent key missing - orphan row",
                })

        return result

    # =========================================================================
    # Helper Methods
    # =========================================================================
    def _iter_messages(self, file_path: str) -> Iterator[str]:
        """Stream messages from file without loading into memory."""
        buffer = []
        with open(file_path, 'r') as f:
            for line in f:
                if line.startswith('MSH|') and buffer:
                    yield ''.join(buffer)
                    buffer = []
                buffer.append(line)
            if buffer:
                yield ''.join(buffer)

    def _flush_all(self, buffers: Dict[str, List[dict]]) -> Iterator[Tuple[str, pa.RecordBatch]]:
        """Synchronized flush: convert all buffers to batches and clear."""
        schemas = {"patients": self._patients_schema, "diagnoses": self._diagnoses_schema}
        for name, rows in buffers.items():
            if rows:
                yield (name, self._to_batch(rows, schemas[name]))
                rows.clear()

    def _to_batch(self, rows: list, schema: pa.Schema) -> pa.RecordBatch:
        """Convert rows to Arrow RecordBatch."""
        arrays = {}
        for field in schema:
            values = [row.get(field.name) for row in rows]
            arrays[field.name] = pa.array(values, type=field.type)
        return pa.RecordBatch.from_pydict(arrays, schema=schema)

    def _extract(self, msg, path: str) -> Optional[str]:
        """Extract value using PPG path syntax (translation layer)."""
        # ... implementation per Section 3.3.3 ...
        pass

    def _get_segments(self, msg, segment_name: str) -> list:
        """Get all instances of a segment (supports recursive option)."""
        # ... implementation ...
        pass

    def _extract_from_segment(self, segment, path: str) -> Optional[str]:
        """Extract from specific segment instance."""
        # ... implementation ...
        pass
```

**Key features of generated parser:**

| Feature | Purpose |
|---------|---------|
| `parse(ctx)` | File streaming entry point |
| `process_message(raw_msg)` | Quarantine Replay entry point (RFC 6.7.4) |
| Synchronized flush | Memory-bounded multi-output buffering |
| Orphan prevention | Children marked as errors if parent key missing |
| Complete schemas | All lineage columns in both schema and emitted rows |

### 5.3 Error Handling (Two-Stage)

PPG-generated parsers use a **two-stage error handling model** that integrates with RFC quarantine:

#### Stage 1: Parser Extraction Errors (Soft-Fail)

During parsing, extraction failures are captured rather than thrown:

```python
# WRONG: Hard fail aborts batch
try:
    value = parse_value(raw)
except Exception:
    raise  # Entire batch fails

# RIGHT: Soft fail captures error
try:
    value = parse_value(raw)
except Exception as e:
    value = None
    row["_cf_row_error"] = f"Extraction error: {e}"
```

**`_cf_row_error` semantics:**
- `None`: Row parsed successfully (may still fail validation)
- Non-null: Extraction failed; row contains best-effort data + error message

#### Stage 2: Schema Validation (RFC Quarantine)

After parsing, the Rust validator (per RFC 6.7) checks schema compliance:

| Check | Trigger | Result | Violation Type |
|-------|---------|--------|----------------|
| **Extraction error** | `_cf_row_error` is non-null | → Quarantine | `extraction_error` |
| Non-nullable column is null | Value is null | → Quarantine | `null_not_allowed` |
| Type mismatch | Value doesn't match declared type | → Quarantine | `type_mismatch` |
| Extra columns (strict mode) | Undeclared column present | → Quarantine | `extra_column` |

**CRITICAL: Explicit `_cf_row_error` Quarantine Rule**

The validator MUST quarantine any row where `_cf_row_error` is non-null, **regardless of whether the row otherwise passes schema validation**. This prevents the ambiguous case where:
- Only nullable fields failed extraction
- The row technically passes schema validation (nulls are allowed)
- But the row should still be quarantined due to extraction failure

```python
# Validator logic (pseudo-code)
def validate_row(row, schema):
    # FIRST: Check extraction error flag
    if row.get("_cf_row_error") is not None:
        return Quarantine(violation_type="extraction_error",
                          message=row["_cf_row_error"])

    # THEN: Normal schema validation
    for column in schema.columns:
        # ... type checks, nullability, etc.
```

**Key distinction:**
- `_cf_row_error` = Parser-level extraction error → **Always quarantine**
- Schema validation = Validator-level type/null checks → Quarantine on violation

The validator checks `_cf_row_error` FIRST, then performs schema validation. This ensures extraction failures are always quarantined.

---

## 6. CLI Interface

### 6.1 Generate Parser

```bash
# Generate parser from config
casparian ppg generate config.json --output parsers/hospital_a_adt.py

# Validate config without generating
casparian ppg validate config.json

# Preview generated outputs (dry run)
casparian ppg preview config.json --sample data/sample.hl7
```

### 6.2 Test Generated Parser

```bash
# Run parser on sample file
casparian run parsers/hospital_a_adt.py data/sample.hl7

# Run with schema validation
casparian test parsers/hospital_a_adt.py data/sample.hl7 --strict
```

### 6.3 Edit Mode (Future)

```bash
# Interactive config editor (TUI)
casparian ppg edit config.json

# Add mapping via CLI
casparian ppg add-column config.json \
  --output patients \
  --name admit_date \
  --path "PV1-44" \
  --type '{"kind": "date"}'
```

---

## 7. Validation

### 7.1 Config Validation

PPG validates config before generation:

| Check | Level | Error |
|-------|-------|-------|
| Valid JSON/YAML | ERROR | Parse failure |
| Known format type | ERROR | Unknown format: X |
| Required settings present | ERROR | Missing setting: delimiter |
| Valid path syntax | ERROR | Invalid path: PID-3-1-2-3 (too deep) |
| Valid type notation | ERROR | Invalid type: "list<string>" (use dict) |
| Output exists for mapping | ERROR | Unknown output: foo |
| Parent exists for child table | ERROR | Unknown parent: bar |
| Parent key exists in parent | ERROR | Parent key 'msg_id' not in 'patients' |
| Parent key is non-nullable | ERROR | Parent key 'msg_id' must be non-nullable |
| Duplicate column names | WARNING | Duplicate column 'name' in output 'patients' |
| Transform known | ERROR | Unknown transform 'custom_fn' |
| Parent consistency | ERROR | outputs.diagnoses.parent='X' but mapping.parent_output='Y' |

**Parent/Child Consistency Rules:**

When using child tables, the following relationships MUST be consistent:

```
outputs:
  diagnoses:
    parent: "patients"           ← Output-level declaration
    parent_key: "message_id"

mappings:
  - output: "diagnoses"
    kind: "child_table"
    parent_output: "patients"    ← Must match outputs.diagnoses.parent
    parent_key: "message_id"     ← Must match outputs.diagnoses.parent_key
```

PPG rejects configs where:
1. `outputs[child].parent` ≠ `mappings[child].parent_output`
2. `outputs[child].parent_key` ≠ `mappings[child].parent_key`
3. `parent_key` column is not defined in parent output
4. `parent_key` column is nullable in parent output

### 7.2 Runtime Validation

Generated parsers validate at runtime:

| Check | Behavior |
|-------|----------|
| Path not found | Return `None` if nullable, else `_cf_row_error` |
| Type parse failure | Return `None` + `_cf_row_error` |
| Required field missing | `_cf_row_error`, row goes to quarantine |

### 7.3 RFC Conformance Matrix

This section documents PPG's conformance to the Parser Schema Contract RFC (v0.5).

#### 7.3.1 Full Implementation

| RFC Section | Requirement | PPG Implementation |
|-------------|-------------|-------------------|
| **6.1** | Parser Manifest Format | PPG generates `outputs` dict with RFC-compliant structure |
| **6.2.1** | Dict-based Type Notation | PPG config uses `{"kind": "string"}` format; generator emits same |
| **6.2.1** | Decimal Type | `{"kind": "decimal", "precision": P, "scale": S}` -> `pa.decimal128(P, S)` |
| **6.2.1** | TimestampTz Type | `{"kind": "timestamp_tz", "tz": "TZ"}` -> `pa.timestamp('us', tz=TZ)` |
| **6.2.1** | List Type | `{"kind": "list", "item": {...}}` -> `pa.list_(item_type)` |
| **6.2.1** | `item_nullable` | Generated code handles null list elements when `item_nullable: true` |
| **6.3** | Schema Modes | Config `mode` field maps directly to RFC modes |
| **6.7** | Soft-Fail Pattern | Generated `parse()` catches exceptions, populates `_cf_row_error` |
| **6.7.2** | Lineage Columns | Generated schemas include `_cf_row_id`; bridge injects runtime columns |
| **6.8** | Multi-Output | PPG supports multiple outputs with parent/child relationships |

#### 7.3.2 Partial Implementation

| RFC Section | Requirement | PPG Status | Notes |
|-------------|-------------|------------|-------|
| **6.2.1** | Time Type | Supported | `{"kind": "time"}` -> `pa.time64('us')` |
| **6.2.1** | Duration Type | Supported | `{"kind": "duration", "unit": U}` -> `pa.duration(U)` |
| **6.4** | Scope ID Derivation | N/A | Computed by runtime, not PPG |
| **6.7.1** | Quarantine Naming | N/A | Handled by runtime, not generated parser |
| **6.7.3** | QuarantineConfig | Partial | `mode` sets validation strictness; thresholds set at runtime |
| **6.7.4** | Quarantine Replay | N/A | CLI command, not parser concern |

#### 7.3.3 Not Applicable

| RFC Section | Reason |
|-------------|--------|
| **5.4.1** | Process Isolation - Runtime concern, not code generation |
| **6.5** | PreviewResult - TUI concern, not PPG |
| **6.9** | Schema Approval State Machine - TUI concern |

#### 7.3.4 Type System Mapping

Complete mapping from PPG config types to RFC types to Arrow types:

| PPG Config | RFC Type | Arrow Type | Notes |
|------------|----------|------------|-------|
| `{"kind": "string"}` | String | `pa.string()` | |
| `{"kind": "int64"}` | Int64 | `pa.int64()` | |
| `{"kind": "float64"}` | Float64 | `pa.float64()` | |
| `{"kind": "boolean"}` | Boolean | `pa.bool_()` | |
| `{"kind": "date"}` | Date | `pa.date32()` | |
| `{"kind": "time"}` | Time | `pa.time64('us')` | Microsecond precision |
| `{"kind": "duration", "unit": "seconds"}` | Duration | `pa.duration('s')` | Unit required |
| `{"kind": "timestamp"}` | Timestamp | `pa.timestamp('us')` | Naive (no TZ) |
| `{"kind": "timestamp_tz", "tz": "UTC"}` | TimestampTz | `pa.timestamp('us', tz='UTC')` | TZ required |
| `{"kind": "decimal", "precision": 18, "scale": 8}` | Decimal | `pa.decimal128(18, 8)` | Precision <= 38 |
| `{"kind": "binary"}` | Binary | `pa.binary()` | |
| `{"kind": "list", "item": {...}}` | List | `pa.list_(...)` | Recursive |
| `{"kind": "struct", "fields": [...]}` | Struct | `pa.struct(...)` | Recursive |

---

## 8. Format-Specific Notes

### 8.1 HL7 v2.x

**Dependencies:** `hl7apy` (Python library)

**Path resolution:**
- `MSH-10` → Message Control ID
- `PID-3-1` → Patient ID (first component)
- `PID-5[0]-1` → First patient name, family name
- `DG1[*]` → All DG1 segments

**Delimiter detection:** PPG auto-detects delimiters from MSH-1 and MSH-2 if not specified.

**Recursive segment traversal:**

HL7 messages can have complex nested group structures (e.g., ORC/OBR/OBX hierarchies in ORU messages). By default, PPG extracts segments at the message root level only. For deeply nested structures, enable recursive traversal:

```json
{
  "settings": {
    "delimiter": "|",
    "component_separator": "^",
    "segment_traversal": "recursive"  // default: "flat"
  }
}
```

| Value | Behavior |
|-------|----------|
| `"flat"` (default) | Only extract segments at message root |
| `"recursive"` | Walk all nested groups to find matching segments |

**When to use recursive:**
- ORU messages with OBR groups containing OBX segments
- ADT messages with nested procedure/diagnosis groups
- Any message type with segment groups (per HL7 spec)

**Performance note:** Recursive traversal may be slower for complex messages. Use `flat` when segment structure is simple (ADT A01 with top-level DG1 segments).

**Example: ORU with nested OBX**

```json
{
  "output": "observations",
  "kind": "child_table",
  "parent_output": "orders",
  "parent_key": "order_id",
  "path": "OBX[*]",
  "path_relative_to": "parent",  // OBX within each OBR group
  "columns": [...]
}
```

With `segment_traversal: "recursive"`, this correctly finds OBX segments inside OBR groups, not just at the message root.

### 8.2 FIX

**Dependencies:** `quickfix` or custom parser

**Path resolution:**
- `35` → MsgType tag
- `453` → NoPartyIDs (repeating group)
- `453.448` → PartyID within group

**SOH handling:** Default delimiter is `\x01` (SOH).

### 8.3 ISO 20022 (XML)

**Dependencies:** `lxml`

**Path resolution:** XPath expressions
- `/Document/CstmrCdtTrfInitn/GrpHdr/MsgId`
- `//PmtInf/CdtTrfTxInf` (all transactions)

**Namespace handling:** Requires `namespace_map` in settings.

---

## 9. Migration from Existing Parsers

### 9.1 Inferring Config from Parser

```bash
# Analyze existing parser and suggest PPG config
casparian ppg infer parsers/example_parser.py --output suggested_config.json
```

This:
1. Extracts `outputs` manifest from parser
2. Analyzes parse logic for path patterns
3. Suggests PPG config (may need manual adjustment)

### 9.2 Hybrid Approach

For complex parsers, PPG-generated parsers can be extended:

```python
from generated.hospital_a_adt import GeneratedParser

class ExtendedParser(GeneratedParser):
    name = 'hospital_a_adt_extended'
    version = '1.0.1'

    def parse(self, ctx):
        # Call generated parser
        for output_name, batch in super().parse(ctx):
            # Add custom post-processing
            if output_name == 'patients':
                batch = self._add_custom_columns(batch)
            yield (output_name, batch)
```

---

## 10. Implementation Complexity

| Component | Effort | Risk | Notes |
|-----------|--------|------|-------|
| Config schema & validation | Medium | Low | JSON Schema + custom validators |
| Path parser (HL7) | Medium | Medium | Handle all HL7 path variants |
| Path parser (FIX) | Medium | Medium | Repeating groups are tricky |
| Code generator | High | Medium | Template-based, Arrow schema gen |
| Child table logic | Medium | Low | FK propagation |
| Soft-fail error handling | Low | Low | Straightforward pattern |
| CLI commands | Low | Low | Standard CLI patterns |

**Total estimate:** 3-4 weeks for core functionality.

---

## 11. Future Enhancements

### 11.1 Visual Config Editor

TUI or web UI for building PPG configs visually:
- Drag-drop column mapping
- Preview extracted values
- Real-time validation

### 11.2 Schema Inference

Auto-generate PPG config from sample files:
```bash
casparian ppg infer-from-sample sample.hl7 --output config.json
```

### 11.3 Custom Transform Functions

Allow user-defined transforms in config:
```json
{
  "transform": {
    "name": "parse_custom_date",
    "module": "myutils.dates",
    "function": "parse_date"
  }
}
```

---

## 12. Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.3.0 | 2026-01-16 | **Second review fixes:** (1) CRITICAL: Complete rewrite of Section 5.2 with streaming parser, two entry points (`parse(ctx)` for files, `process_message(raw_msg)` for Quarantine Replay per RFC 6.7.4); (2) CRITICAL: Synchronized flush for memory-bounded multi-output buffering; (3) CRITICAL: Orphan prevention (children marked as errors when parent key missing); (4) CRITICAL: All schemas include complete lineage columns; (5) HIGH: Renamed `contract_id` to `schema_fingerprint` with clear distinction from approval-time contract_id; (6) HIGH: Added explicit `_cf_row_error` quarantine rule (validator checks FIRST); (7) MEDIUM: Added composite parent key support (Section 2.3); (8) MEDIUM: Added transform parameterization `transform_args` (Section 3.4); (9) MEDIUM: Defined `validation.list_policy` values (`full`, `sample(N)`); (10) MEDIUM: Added parent/child consistency validation rules (Section 7.1); (11) MEDIUM: Added HL7 recursive segment traversal option `segment_traversal` (Section 8.1). |
| 1.2.0 | 2026-01-16 | **Review fixes:** (1) CRITICAL: Aligned `mode` with RFC (`strict`/`allow_extra`/`allow_missing_optional`, removed `dev`); (2) CRITICAL: Clarified two-stage error handling (parser soft-fail vs RFC quarantine); (3) HIGH: Added parent key constraints (non-nullable, unique); (4) HIGH: Clarified `repeat_strategy: all` incompatible with child tables; (5) HIGH: Added streaming requirements for large files (Section 5.1.1); (6) MEDIUM: Added XPath/JSONPath complexity limits; (7) MEDIUM: Added transform registry validation; (8) MEDIUM: Added `item_nullable` handling; (9) Added multi-output contract linkage (Section 4.4.5); (10) Updated examples to include `_cf_parent_key`. |
| 1.1.0 | 2026-01-16 | **Spec refinement:** Added Section 3.3.3 (HL7 path translation table), Section 7.3 (RFC Conformance Matrix), nested child table support (`path_relative_to` field), updated Section 4.4 with PPG/Bridge lineage column responsibility split. |
| 1.0.0 | 2026-01-16 | Initial PPG specification. RFC-aligned types, child_table support, Arrow output, soft-fail error handling. |

---

## Appendix A: Full Config Example

```json
{
  "parser_name": "hospital_a_adt",
  "version": "1.0.0",
  "topics": ["hl7_adt"],
  "format": "hl7v2",

  "settings": {
    "delimiter": "|",
    "component_separator": "^",
    "escape_character": "\\",
    "segment_terminator": "\r",
    "default_repeat_strategy": "first"
  },

  "outputs": {
    "patients": {
      "mode": "strict",
      "description": "One row per ADT message",
      "validation": {
        "list_policy": "full"
      }
    },
    "diagnoses": {
      "mode": "strict",
      "description": "One row per DG1 segment",
      "parent": "patients",
      "parent_key": "message_id"
    },
    "observations": {
      "mode": "strict",
      "description": "One row per OBX segment",
      "parent": "patients",
      "parent_key": "message_id"
    }
  },

  "mappings": [
    {
      "output": "patients",
      "column_name": "message_id",
      "path": "MSH-10",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "output": "patients",
      "column_name": "message_datetime",
      "path": "MSH-7",
      "type": {"kind": "timestamp_tz", "tz": "UTC"},
      "nullable": false,
      "transform": "parse_hl7_datetime"
    },
    {
      "output": "patients",
      "column_name": "patient_mrn",
      "path": "PID-3-1",
      "type": {"kind": "string"},
      "nullable": false
    },
    {
      "output": "patients",
      "column_name": "patient_last_name",
      "path": "PID-5[0]-1",
      "type": {"kind": "string"},
      "nullable": true
    },
    {
      "output": "patients",
      "column_name": "patient_first_name",
      "path": "PID-5[0]-2",
      "type": {"kind": "string"},
      "nullable": true
    },
    {
      "output": "patients",
      "column_name": "patient_dob",
      "path": "PID-7",
      "type": {"kind": "date"},
      "nullable": true,
      "transform": "parse_hl7_date"
    },
    {
      "output": "patients",
      "column_name": "admit_datetime",
      "path": "PV1-44",
      "type": {"kind": "timestamp_tz", "tz": "America/New_York"},
      "nullable": true,
      "transform": "parse_hl7_datetime"
    },

    {
      "output": "diagnoses",
      "kind": "child_table",
      "parent_output": "patients",
      "parent_key": "message_id",
      "path": "DG1[*]",
      "repeat_strategy": "explode",
      "columns": [
        {
          "name": "set_id",
          "path": "DG1-1",
          "type": {"kind": "int64"},
          "nullable": false,
          "transform": "to_int"
        },
        {
          "name": "diagnosis_code",
          "path": "DG1-3-1",
          "type": {"kind": "string"},
          "nullable": false
        },
        {
          "name": "diagnosis_description",
          "path": "DG1-3-2",
          "type": {"kind": "string"},
          "nullable": true
        },
        {
          "name": "diagnosis_type",
          "path": "DG1-6",
          "type": {"kind": "string"},
          "nullable": true
        },
        {
          "name": "diagnosis_datetime",
          "path": "DG1-5",
          "type": {"kind": "timestamp_tz", "tz": "UTC"},
          "nullable": true,
          "transform": "parse_hl7_datetime"
        }
      ]
    },

    {
      "output": "observations",
      "kind": "child_table",
      "parent_output": "patients",
      "parent_key": "message_id",
      "path": "OBX[*]",
      "repeat_strategy": "explode",
      "columns": [
        {
          "name": "set_id",
          "path": "OBX-1",
          "type": {"kind": "int64"},
          "nullable": false,
          "transform": "to_int"
        },
        {
          "name": "value_type",
          "path": "OBX-2",
          "type": {"kind": "string"},
          "nullable": false
        },
        {
          "name": "observation_id",
          "path": "OBX-3-1",
          "type": {"kind": "string"},
          "nullable": false
        },
        {
          "name": "observation_value",
          "path": "OBX-5",
          "type": {"kind": "string"},
          "nullable": true
        },
        {
          "name": "units",
          "path": "OBX-6-1",
          "type": {"kind": "string"},
          "nullable": true
        },
        {
          "name": "result_status",
          "path": "OBX-11",
          "type": {"kind": "string"},
          "nullable": true
        }
      ]
    }
  ]
}
```

---

## Appendix B: Type Reference

| Kind | Parameters | Arrow Type | Example |
|------|------------|------------|---------|
| `string` | - | `pa.string()` | `{"kind": "string"}` |
| `int64` | - | `pa.int64()` | `{"kind": "int64"}` |
| `float64` | - | `pa.float64()` | `{"kind": "float64"}` |
| `boolean` | - | `pa.bool_()` | `{"kind": "boolean"}` |
| `date` | - | `pa.date32()` | `{"kind": "date"}` |
| `time` | - | `pa.time64('us')` | `{"kind": "time"}` |
| `duration` | `unit` | `pa.duration(unit)` | `{"kind": "duration", "unit": "seconds"}` |
| `timestamp` | - | `pa.timestamp('us')` | `{"kind": "timestamp"}` |
| `timestamp_tz` | `tz` | `pa.timestamp('us', tz)` | `{"kind": "timestamp_tz", "tz": "UTC"}` |
| `decimal` | `precision`, `scale` | `pa.decimal128(p, s)` | `{"kind": "decimal", "precision": 18, "scale": 8}` |
| `binary` | - | `pa.binary()` | `{"kind": "binary"}` |
| `list` | `item`, `item_nullable` | `pa.list_(item)` | `{"kind": "list", "item": {"kind": "string"}}` |
| `struct` | `fields` | `pa.struct(fields)` | See RFC 6.2.1 |

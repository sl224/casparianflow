# RFC: Parser Schema Contract System

**Status:** Draft (Revised after external review)
**Version:** 0.2
**Date:** 2026-01-16
**Authors:** Casparian Flow Team
**Reviewers:** External LLM Review Complete

---

## Purpose of This Document

This RFC proposes a strengthened schema contract system for Casparian Flow. Version 0.2 incorporates feedback from external review, addressing critical gaps in error handling and operational resilience.

**Key changes in v0.2:**
- Added Quarantine pattern for partial failure handling (Section 6.7)
- Changed "hard failures" to "explicit violations" philosophy
- Updated scope_id to use AST-normalized hashing (Section 6.4)
- Resolved all open questions with recommendations (Section 9)
- Upgraded implementation complexity estimates (Section 7)

---

## Table of Contents

1. [Product Context](#1-product-context)
2. [Market & Customer Context](#2-market--customer-context)
3. [Current Architecture](#3-current-architecture)
4. [Problem Statement](#4-problem-statement)
5. [Proposed Solution](#5-proposed-solution)
6. [Detailed Design](#6-detailed-design)
7. [Implementation Complexity](#7-implementation-complexity)
8. [Alternatives Considered](#8-alternatives-considered)
9. [Open Questions (Resolved)](#9-open-questions-resolved)
10. [Future Considerations](#10-future-considerations)
11. [Appendix: Current Code](#appendix-current-code)

---

## 1. Product Context

### 1.1 What is Casparian Flow?

Casparian Flow is a **local-first data platform** that transforms industry-specific file formats into queryable SQL/Parquet datasets. It runs entirely on the user's machine—no cloud required.

**Core value proposition:**
- Premade parsers for arcane formats (FIX trading logs, HL7 healthcare messages, legal PST archives)
- Schema contracts with audit trails for compliance
- Local-first execution for air-gapped/regulated environments
- AI-assisted parser development (human approves, AI proposes)

### 1.2 The "Bronze Layer" Problem

Traditional ETL tools (Fivetran, Airbyte) handle APIs and standard formats. But regulated industries have proprietary formats that require custom parsing:

| Industry | Format Examples | Why Casparian |
|----------|-----------------|---------------|
| Finance | FIX logs, ISO 20022 | Trade break resolution; audit trails |
| Healthcare | HL7 v2.x messages | HIPAA; can't use cloud |
| Legal | PST archives, load files | eDiscovery pre-processing |
| Defense | CoT tracks, KLV telemetry | Air-gapped; classified data |

### 1.3 Core Philosophy

1. **Schema = Contract**: Once approved, a schema is immutable. Violations are **explicit, not silent**—surfaced clearly rather than silently coerced.
2. **Local-First**: Data never leaves the machine unless explicitly exported.
3. **AI Proposes, Human Approves**: AI can help write parsers, but humans approve the output schema.
4. **Governance Built-In**: Audit trails, versioning, and contracts are core features, not enterprise add-ons.
5. **Operational Resilience**: One bad row should not block processing of valid data. Violations are quarantined, not blocking.

---

## 2. Market & Customer Context

### 2.1 Target Customer Profiles (ICPs)

**Primary: Technical teams in regulated industries**

| Segment | Buyer | Pain Point | Why They Care About Schema Contracts |
|---------|-------|------------|-------------------------------------|
| **Financial Services** | Trade ops, quant teams | Trade break resolution from FIX logs | Floats for money = compliance violation; need Decimal |
| **Healthcare IT** | Hospital IT, analysts | Query historical HL7 archives | HIPAA audit trails; deterministic parsing |
| **Legal/eDiscovery** | Litigation support | PST/load file processing | Schema mistakes = costly re-processing |
| **Defense** | In-house dev teams | Tactical data formats | Air-gapped; no cloud; full traceability |

### 2.2 Why Schema Contracts Matter to These Customers

**Finance example (FIX logs):**
```
Trade amounts must be Decimal(18,8), not Float64.
Floats accumulate rounding errors.
A $0.0001 error across 1M trades = regulatory scrutiny.
```

**Healthcare example (HL7 messages):**
```
HL7 v2.x has optional/repeating segments.
Inferred schemas drift when new segment types appear.
Analysts need stable, contract-driven mappings.
```

**Legal example (load files):**
```
Schema approved for production processing.
Parser author changes logic but forgets to update schema.
10TB of data processed with wrong schema = expensive redo.
```

### 2.3 Operational Reality

**Critical insight from external review:**

> "A bank processes a 5GB FIX log file with 10 million trades. Row 9,999,999 has a schema violation (e.g., a null price due to a system glitch). In high-stress environments (e.g., T+1 settlement), blocking the entire batch is unacceptable. Users cannot wait for a developer to fix the parser to see the valid 9,999,998 trades."

This drove the addition of the Quarantine pattern (Section 6.7).

### 2.4 Competitive Positioning

| Solution | Schema Governance | Local-First | Industry Parsers | Error Handling |
|----------|------------------|-------------|------------------|----------------|
| Databricks/Spark | Schema evolution (weak contracts) | Cloud-first | DIY | Silent coercion |
| Fivetran/Airbyte | Source-defined | Cloud-only | Standard APIs | Fail or skip |
| DIY Python | None | Yes | Manual | Ad-hoc |
| **Casparian Flow** | **Explicit contracts** | **Yes** | **Premade** | **Quarantine** |

**Our differentiation**: Schema contracts are a feature, not an afterthought. Violations are explicit and quarantined, not silent or blocking.

---

## 3. Current Architecture

### 3.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CASPARIAN FLOW                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                │
│  │   Scout     │────▶│   Parser    │────▶│   Output    │                │
│  │ (Discovery) │     │   Bench     │     │ (Parquet)   │                │
│  └─────────────┘     └─────────────┘     └─────────────┘                │
│         │                   │                   │                        │
│         ▼                   ▼                   ▼                        │
│  Tag files by         Test parsers,       Query with SQL                │
│  pattern/type         approve schemas     (DuckDB/SQLite)               │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │                    Schema Contracts                      │            │
│  │  • LockedSchema: immutable column definitions           │            │
│  │  • SchemaContract: binds schema to scope                │            │
│  │  • Violations: explicit with quarantine                 │            │
│  └─────────────────────────────────────────────────────────┘            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Parser Structure

Parsers are Python files with class attributes:

```python
# ~/.casparian_flow/parsers/trade_parser.py

class TradeParser:
    name = 'trade_parser'       # Logical name
    version = '1.0.0'           # Semver
    topics = ['fix_logs']       # What file tags this parser handles

    def transform(self, df):
        # Transform input DataFrame
        # Return output DataFrame
        return df[['order_id', 'symbol', 'quantity', 'price']]
```

### 3.3 Current Metadata Extraction

The TUI extracts parser metadata **without executing code** (security requirement):

```python
# Embedded in Rust, executed via subprocess
import ast
import json

def extract_metadata(path):
    """Extract parser metadata via AST parsing (no execution)."""
    source = open(path).read()
    tree = ast.parse(source)

    result = {"name": None, "version": None, "topics": []}

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for item in node.body:
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            try:
                                value = ast.literal_eval(item.value)
                                if target.id == "name":
                                    result["name"] = value
                                elif target.id == "version":
                                    result["version"] = value
                                elif target.id == "topics":
                                    result["topics"] = value
                            except:
                                pass
    return result
```

**Key constraint**: `ast.literal_eval()` only works on Python literals (dicts, lists, strings, numbers). Function calls or variable references fail.

### 3.4 Current Schema Contract System

```rust
// crates/casparian_schema/src/contract.rs

/// The binding agreement between user intent and parser output.
pub struct SchemaContract {
    pub contract_id: Uuid,
    pub scope_id: String,           // What this applies to (currently just a string)
    pub approved_at: DateTime<Utc>,
    pub approved_by: String,
    pub schemas: Vec<LockedSchema>,
    pub version: u32,
}

/// Immutable definition of expected data structure.
pub struct LockedSchema {
    pub schema_id: Uuid,
    pub name: String,               // e.g., "trades"
    pub columns: Vec<LockedColumn>,
    pub source_pattern: Option<String>,
    pub content_hash: String,
}

/// Column definition.
pub struct LockedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub format: Option<String>,
    pub description: Option<String>,
}

/// Supported data types.
pub enum DataType {
    String,
    Int64,
    Float64,    // <-- Problem: Finance needs Decimal
    Boolean,
    Date,
    Timestamp,  // <-- Problem: No timezone specification
    Binary,
}
```

### 3.5 Current Preview Flow

When a user tests a parser in the TUI:

```rust
pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Vec<SchemaColumn>,  // <-- INFERRED from sample data
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub truncated: bool,
}

pub struct SchemaColumn {
    pub name: String,
    pub dtype: String,  // String like "Int64", "Float64"
}
```

**Current flow:**
1. User runs parser on sample file
2. System infers schema from output data
3. User reviews inferred schema
4. User approves → SchemaContract created
5. Future runs validate against contract

---

## 4. Problem Statement

### 4.1 Core Problem

The current schema contract system is **inference-based, not intent-based**. This creates several gaps for regulated industries:

| Gap | Impact | Affected ICP |
|-----|--------|--------------|
| No Decimal type | Can't represent money precisely | Finance |
| No timezone on Timestamp | Audit trail ambiguity | All |
| Schema inferred, not declared | Drift when samples vary | Healthcare (HL7) |
| scope_id is a loose string | No binding to parser identity | All |
| No intent vs observed diff | Can't see what changed | Legal |
| No nested types (List, Struct) | Can't represent HL7 segments | Healthcare |
| Hard failure on violation | One bad row blocks 10M good rows | All |

### 4.2 Specific Failure Scenarios

**Scenario 1: Financial Data Precision Loss**
```
Parser outputs trade prices as Float64 (inferred).
User approves schema without noticing.
Production data: $1,234.56789012 (8 decimal places for FX)
Float64 rounds to: $1,234.5678901199999
Compliance audit flags discrepancy.
```

**Scenario 2: HL7 Schema Drift**
```
Week 1: HL7 messages have segments MSH, PID, PV1.
Schema inferred and approved.
Week 2: New messages include OBX segments (lab results).
Parser output changes. Contract violation or silent schema drift?
```

**Scenario 3: Parser Version Mismatch**
```
trade_parser v1.0.0 approved with schema.
Developer bumps to v1.0.1, changes output columns.
Same scope_id → old contract still applies.
Runtime failure with confusing error.
```

**Scenario 4: T+1 Settlement Blocking (NEW)**
```
5GB FIX log with 10M trades.
Row 9,999,999 has null price (system glitch).
Current: Entire batch fails. User gets 0 rows.
Required: 9,999,999 valid rows written; 1 row quarantined.
```

### 4.3 The Fundamental Gap

**Current state**: "Schema as Inference"
- Schema derived from sample data
- Parser doesn't declare intent
- Contract loosely bound to scope
- Violations block entire batch

**Required state**: "Schema as Contract"
- Parser explicitly declares output schema
- Approval compares intent vs observed
- Contract bound to parser identity + version
- Violations quarantined, valid data flows

---

## 5. Proposed Solution

### 5.1 Solution Overview

1. **Parser Manifest**: Parsers declare expected output schema (`outputs` attribute)
2. **Expanded Type System**: Add Decimal, TimestampTz, List, Struct
3. **Intent vs Observed**: Preview shows declared vs inferred schema with diff
4. **Strong Scope Binding**: Contract tied to parser_id + version + AST-normalized hash
5. **Rust/Arrow Validation**: Validate in Rust layer for performance (not Python)
6. **Quarantine Pattern**: Invalid rows written to separate file, valid rows proceed

### 5.2 Design Principles

| Principle | Implementation |
|-----------|----------------|
| AST-safe extraction | `outputs` must be literal dicts (no function calls) |
| No SDK required | Plain Python dicts work; SDK is optional helper |
| Explicit over implicit | Nullable defaults to false; mode must be specified in production |
| Violations are explicit | Quarantine bad rows; never silently coerce |
| Operational resilience | One bad row doesn't block valid data |
| Validation in Rust | Pre-write validation via Arrow for performance |
| Backward compatible | Parsers without `outputs` continue to work (with warnings) |

### 5.3 High-Level Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    PROPOSED SCHEMA CONTRACT FLOW                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. PARSER AUTHORING                                                     │
│     └─ Developer adds `outputs` manifest to parser                       │
│     └─ Optional: CLI validates manifest syntax                           │
│                                                                          │
│  2. DISCOVERY (AST Extraction)                                           │
│     └─ TUI extracts `outputs` via ast.literal_eval()                    │
│     └─ No code execution; safe for untrusted parsers                    │
│                                                                          │
│  3. PREVIEW (Test in Bench)                                              │
│     └─ Run parser on sample file                                         │
│     └─ Compare: schema_intent (from manifest) vs schema_observed        │
│     └─ Show diff: missing columns, type mismatches, extra columns       │
│     └─ Show confidence: sample size, null %, type coercions             │
│                                                                          │
│  4. APPROVAL (Create Contract)                                           │
│     └─ User reviews diff, approves if acceptable                         │
│     └─ Contract created with scope = parser_id + version + AST hash     │
│     └─ Contract stored in SQLite with audit metadata                     │
│                                                                          │
│  5. RUNTIME (Production Execution)                                       │
│     └─ Worker loads parser, resolves contract by scope                  │
│     └─ Parser yields Arrow batches to Rust                              │
│     └─ Rust validates Arrow schema against contract (fast, zero-copy)   │
│     └─ Valid rows → output.parquet                                      │
│     └─ Invalid rows → output_quarantine.parquet + error metadata        │
│     └─ No contract = fail (unless --dev flag)                            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.4 Validation Architecture

**Critical**: Validation MUST happen in Rust/Arrow layer, not Python.

```
┌──────────────────────────────────────────────────────────────────┐
│                        VALIDATION PATH                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Parser (Python)                                                  │
│       │                                                           │
│       ▼ Arrow IPC batches (via ZMQ)                              │
│       │                                                           │
│  ┌────┴────────────────────────────────────┐                     │
│  │         Rust Validator                   │                     │
│  │  • Receives Arrow RecordBatch           │                     │
│  │  • Validates schema shape (O(columns))  │                     │
│  │  • Validates per-row constraints        │                     │
│  │  • Zero-copy where possible             │                     │
│  └────┬─────────────────────────┬──────────┘                     │
│       │                         │                                 │
│       ▼                         ▼                                 │
│  Valid Rows               Invalid Rows                            │
│  → output.parquet         → output_quarantine.parquet            │
│                              + _error_msg                         │
│                              + _source_row                        │
│                              + _raw_data                          │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

**Why Rust validation?**
- Python loop over 10M rows is too slow
- Arrow provides vectorized validation (O(columns), not O(rows))
- Zero-copy inspection of column types
- Per-value checks (nullability, precision) via Arrow compute kernels

---

## 6. Detailed Design

### 6.1 Parser Manifest Format

The `outputs` attribute is a dict with literal values (AST-extractable):

```python
class TradeParser:
    name = 'trade_parser'
    version = '1.0.0'
    topics = ['fix_logs']

    # NEW: Explicit schema manifest
    outputs = {
        "trades": {
            "mode": "strict",  # "strict" | "allow_extra" | "allow_missing_optional"
            "columns": [
                {
                    "name": "order_id",
                    "type": {"kind": "string"},
                    "nullable": False
                },
                {
                    "name": "quantity",
                    "type": {"kind": "int64"},
                    "nullable": False
                },
                {
                    "name": "price",
                    "type": {"kind": "decimal", "precision": 18, "scale": 8},
                    "nullable": False
                },
                {
                    "name": "exec_time",
                    "type": {"kind": "timestamp_tz", "tz": "UTC"},
                    "nullable": False
                },
                {
                    "name": "tags",
                    "type": {"kind": "list", "item": {"kind": "string"}},
                    "nullable": True
                }
            ]
        }
    }
```

### 6.2 Type System

#### 6.2.1 Manifest Type Notation (Python dict)

```python
# Primitive types
{"kind": "string"}
{"kind": "int64"}
{"kind": "float64"}
{"kind": "boolean"}
{"kind": "date"}
{"kind": "binary"}

# Parameterized types
{"kind": "decimal", "precision": 18, "scale": 8}
{"kind": "timestamp_tz", "tz": "UTC"}           # Explicit timezone REQUIRED
{"kind": "timestamp_tz", "tz": "America/New_York"}
{"kind": "timestamp"}                            # Naive (no timezone) - explicit choice

# Composite types (for HL7, nested data)
{"kind": "list", "item": {"kind": "string"}}
{"kind": "struct", "fields": [
    {"name": "mrn", "type": {"kind": "string"}, "nullable": False},
    {"name": "facility", "type": {"kind": "string"}, "nullable": True}
]}
```

#### 6.2.2 Timezone Handling (RESOLVED)

**Decision**: Explicit timezone handling. No silent UTC default.

| Contract Type | Data Has TZ | Behavior |
|---------------|-------------|----------|
| `timestamp_tz(UTC)` | Yes (UTC) | Accept |
| `timestamp_tz(UTC)` | Yes (other) | Convert to UTC |
| `timestamp_tz(UTC)` | No | **Quarantine** (violation) |
| `timestamp` (naive) | No | Accept |
| `timestamp` (naive) | Yes | Strip timezone, accept |

**Rationale**: "Do not silently default to UTC. That is how medical errors happen (e.g., medication timing across timezones)."

#### 6.2.3 Nested Type Validation (RESOLVED)

**Decision**: Deep validation required.

- List types: Validate element types, not just "is list"
- Struct types: Validate all nested fields recursively
- Rationale: HL7 and ISO 20022 have critical data 4+ levels deep

Arrow provides efficient nested validation via `StructArray` and `ListArray` accessors.

#### 6.2.4 Rust DataType Enum (Updated)

```rust
pub enum DataType {
    // Primitive types
    String,
    Int64,
    Float64,
    Boolean,
    Date,
    Binary,

    // Parameterized types (NEW)
    Decimal { precision: u8, scale: u8 },  // Decimal128 (precision <= 38)
    TimestampTz { tz: String },            // Explicit timezone required
    Timestamp,                              // Naive (no timezone)

    // Composite types (NEW)
    List { item: Box<DataType> },
    Struct { fields: Vec<StructField> },

    // DEFERRED to v2: Union type for dirty legacy data
    // Union { variants: Vec<DataType> },
}

pub struct StructField {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}
```

#### 6.2.5 Decimal Precision (RESOLVED)

**Decision**: Decimal128 only (precision ≤ 38).

- Covers all finance use cases (18-20 precision typical)
- Arrow Decimal128 is well-supported
- Decimal256 deferred unless specific customer need arises

### 6.3 Schema Modes

| Mode | Behavior | Use Case |
|------|----------|----------|
| `strict` | Exact match required. Extra/missing columns = quarantine. | Production, regulated |
| `allow_extra` | Parser may output additional columns (ignored). | Development iteration |
| `allow_missing_optional` | Optional (nullable) columns may be absent. | Flexible schemas |

**Default behavior by context:**

| Context | Default Mode | Rationale |
|---------|--------------|-----------|
| `casparian run` (dev) | `allow_extra` | Fast iteration |
| Parser Bench preview | `allow_extra` + warnings | Show what would fail |
| `casparian worker` (prod) | Require explicit | No implicit leniency |

### 6.4 Scope ID Derivation (UPDATED)

**Change from v0.1**: Use AST-normalized hash instead of file-string hash.

**Problem with file-string hash**: A comment typo changes the hash, breaking the contract even though logic didn't change.

**Solution**: Parse AST, extract only logic-relevant nodes, hash the normalized form.

```python
def compute_logic_hash(source: str) -> str:
    """Hash only logic-relevant parts of the parser."""
    tree = ast.parse(source)

    # Extract only:
    # - Class definitions (name, bases)
    # - Method definitions (name, args, body) - NOT docstrings
    # - Assignments for name, version, topics, outputs

    logic_nodes = []
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            # Include class structure but strip docstrings
            stripped = strip_docstrings(node)
            logic_nodes.append(ast.dump(stripped))

    # Deterministic serialization
    normalized = "\n".join(sorted(logic_nodes))
    return hashlib.blake3(normalized.encode()).hexdigest()
```

**Scope ID formula:**
```
scope_id = sha256(
    parser_name + ":" +
    parser_version + ":" +
    sorted(topics).join(",") + ":" +
    output_name + ":" +
    logic_hash  # <-- AST-normalized, not file-string
)
```

**Benefits:**
- Comment changes don't break contract
- Docstring updates don't break contract
- Whitespace/formatting changes don't break contract
- Only semantic changes trigger re-approval

### 6.5 Preview Result (Extended)

```rust
pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,

    // Schema comparison (NEW)
    pub schema_intent: Option<SchemaIntent>,     // From manifest
    pub schema_observed: Vec<SchemaColumn>,      // From sample
    pub schema_diff: Option<SchemaDiff>,         // Differences
    pub schema_confidence: SchemaConfidence,     // Reliability metrics

    // Existing fields
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub truncated: bool,
}

pub struct SchemaIntent {
    pub output_name: String,
    pub mode: SchemaMode,
    pub columns: Vec<IntentColumn>,
}

pub struct SchemaDiff {
    pub missing_columns: Vec<String>,       // In intent, not observed
    pub extra_columns: Vec<String>,         // In observed, not intent
    pub type_mismatches: Vec<TypeMismatch>,
    pub nullable_mismatches: Vec<NullableMismatch>,
}

pub struct SchemaConfidence {
    pub rows_sampled: usize,
    pub null_percentages: HashMap<String, f64>,
    pub type_coercions: Vec<String>,
    pub level: ConfidenceLevel,  // High, Medium, Low
}
```

### 6.6 Violation Handling

```rust
pub struct SchemaViolation {
    pub parser_id: String,
    pub parser_version: String,
    pub file_path: String,
    pub row: Option<usize>,           // Source row number if available
    pub column: String,
    pub violation_type: ViolationType,
    pub expected: String,
    pub actual: String,
    pub suggestion: Option<String>,
}

pub enum ViolationType {
    ColumnMissing,
    ColumnExtra,
    TypeMismatch,
    NullNotAllowed,
    PrecisionExceeded,
    FormatInvalid,
    TimezoneRequired,
    ContractNotFound,
}
```

### 6.7 Quarantine Pattern (NEW)

**Critical addition based on external review.**

The Quarantine pattern ensures one bad row doesn't block valid data:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     QUARANTINE PATTERN                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Parser Output: 10,000,000 rows                                         │
│                     │                                                    │
│                     ▼                                                    │
│            ┌────────────────┐                                           │
│            │ Rust Validator │                                           │
│            └───────┬────────┘                                           │
│                    │                                                     │
│        ┌───────────┴───────────┐                                        │
│        │                       │                                         │
│        ▼                       ▼                                         │
│  9,999,999 valid          1 invalid                                      │
│        │                       │                                         │
│        ▼                       ▼                                         │
│  ┌──────────────┐    ┌───────────────────────────┐                      │
│  │trades.parquet│    │trades_quarantine.parquet  │                      │
│  │              │    │                           │                      │
│  │ Normal       │    │ All columns as String     │                      │
│  │ schema       │    │ + _source_row: u64        │                      │
│  │              │    │ + _error_msg: String      │                      │
│  │              │    │ + _raw_data: String       │                      │
│  │              │    │ + _cf_job_id: String      │                      │
│  └──────────────┘    └───────────────────────────┘                      │
│                                                                          │
│  UI Message: "Processed 10,000,000 rows with 1 quarantined.             │
│              [View Quarantine]"                                          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 6.7.1 Quarantine File Structure

| Column | Type | Description |
|--------|------|-------------|
| `_source_row` | Int64 | Original row number in source file |
| `_error_msg` | String | Detailed violation message |
| `_raw_data` | String | JSON-serialized original row data |
| `_cf_job_id` | String | Job ID for lineage |
| `*` (all columns) | String | Original columns coerced to String |

#### 6.7.2 Quarantine Threshold

If quarantine exceeds threshold, the job fails entirely:

```rust
pub struct QuarantineConfig {
    /// Maximum percentage of rows that can be quarantined before failing
    pub max_quarantine_pct: f64,  // Default: 10.0 (10%)

    /// Maximum absolute count of quarantined rows before failing
    pub max_quarantine_count: Option<usize>,  // Default: None (no limit)

    /// Whether to emit warnings for quarantine (not just errors)
    pub warn_on_quarantine: bool,  // Default: true
}
```

**Behavior:**
- If 5% of rows quarantined: Job succeeds with warning
- If 15% of rows quarantined: Job fails (exceeds 10% threshold)
- Threshold configurable per contract or globally

#### 6.7.3 Quarantine Lineage

Quarantine files have the same `_cf_job_id` as main output, enabling:

```sql
-- Find all data from a job (including quarantined)
SELECT * FROM trades WHERE _cf_job_id = 'abc123'
UNION ALL
SELECT * FROM trades_quarantine WHERE _cf_job_id = 'abc123'
```

### 6.8 SDK (Optional Authoring Helper)

The SDK is for **validation and codegen**, not inline use:

```python
# WRONG: Function calls not AST-extractable
from casparian import schema
outputs = {"trades": {"columns": [{"type": schema.decimal(18, 8)}]}}  # FAILS

# RIGHT: SDK generates literal dict, user copies to file
$ casparian schema generate trade_parser.py
# Outputs:
outputs = {
    "trades": {
        "mode": "strict",
        "columns": [
            {"name": "price", "type": {"kind": "decimal", "precision": 18, "scale": 8}, "nullable": False}
        ]
    }
}
# Copy this to your parser file.

# SDK for validation
$ casparian parser validate trade_parser.py
✓ outputs manifest is valid
✓ All types recognized
✓ No variable references (AST-safe)
```

---

## 7. Implementation Complexity

### 7.1 Estimated Effort by Component (UPDATED)

| Component | Effort | Risk | Notes |
|-----------|--------|------|-------|
| Extend DataType enum | Medium | Low | Add Decimal, TimestampTz, List, Struct |
| Update AST extractor | Low | Low | Add `outputs` to extraction |
| AST-normalized hashing | Medium | Low | Strip comments/docstrings before hash |
| PreviewResult diff | Medium | Medium | New comparison logic |
| Scope ID derivation | Low | Low | Hash function update |
| **Runtime validation** | **High** | **High** | Pre-write check in Rust; row lineage tracking |
| Quarantine pattern | High | Medium | Separate output path; threshold logic |
| TUI diff view | Medium | Medium | New UI component |
| SDK (validate, generate) | Medium | Low | CLI commands |
| Migration (existing parsers) | Low | Low | Graceful fallback |

**Note**: Runtime validation upgraded to HIGH based on external review. Challenges include:
- Mapping Arrow batch errors back to source row numbers
- Ensuring preview and production validation share identical logic
- Implementing single validation path via PyO3 bindings

### 7.2 Dependencies

```
casparian_schema (Rust)
  └─ Add Decimal, TimestampTz, List, Struct to DataType
  └─ Add serialization for new types
  └─ Add QuarantineConfig

casparian_protocol (Rust)
  └─ Update PreviewResult struct
  └─ Add quarantine output types

casparian/src/cli/tui (Rust)
  └─ Extend METADATA_EXTRACTOR_SCRIPT for `outputs`
  └─ Add diff view component
  └─ Add quarantine view

casparian/src/runner (Rust)
  └─ Add pre-write validation (Rust, not Python)
  └─ Add scope resolution with AST-normalized hash
  └─ Add quarantine output path
  └─ Track source row numbers for error reporting
```

### 7.3 Backward Compatibility

| Scenario | Behavior |
|----------|----------|
| Parser without `outputs` | Continue working; schema inferred; warning in TUI |
| Old contract (pre-Decimal) | Continue working; Float64 still valid |
| New parser, old runtime | Fail at type parsing; require runtime upgrade |

### 7.4 Shared Schema Definitions (Roadmap)

**Problem identified in review**: 50 HL7 parsers sharing PID segment = copy-paste nightmare.

**Deferred solution (v1.1)**: Build-time "bake" step

```bash
# Developer writes (human-friendly):
# parsers/adt_parser.py
from casparian_schemas import PID_SEGMENT  # Import shared definition

class ADTParser:
    outputs = {
        "patients": {"columns": [PID_SEGMENT, ...]}
    }

# CLI bakes to AST-safe form before scan:
$ casparian schema bake parsers/adt_parser.py

# Output: parsers/adt_parser.baked.py
# All imports resolved to literal dicts
# Scanner only sees this file
```

**This preserves AST-safety at scan time while enabling code reuse at development time.**

---

## 8. Alternatives Considered

### 8.1 PyArrow Schema as Primary Format

**Approach**: Use `pa.schema([...])` in parser instead of dict.

```python
import pyarrow as pa

class TradeParser:
    outputs = {
        "trades": pa.schema([
            ("price", pa.decimal128(18, 8)),
        ])
    }
```

**Rejected because**:
- Requires code execution (not AST-safe)
- Violates security model for untrusted parsers
- Creates dependency on PyArrow installation at discovery time

**Trade-off**: PyArrow has rich types and IDE support, but breaks the non-execution constraint.

### 8.2 Sidecar Manifest File

**Approach**: `trade_parser.schema.json` alongside `trade_parser.py`.

```json
{
  "outputs": {
    "trades": {
      "columns": [...]
    }
  }
}
```

**Partially adopted**:
- Sidecar is fallback when AST extraction fails
- Primary is in-parser `outputs` dict
- Sidecar includes `parser_source_hash` for staleness detection

**Trade-off**: Two files to maintain, but provides escape hatch for complex cases.

### 8.3 String-Based Type Notation

**Approach**: `"type": "Decimal(18,8)"` instead of `"type": {"kind": "decimal", ...}`.

**Rejected because**:
- Requires string parsing (regex or parser)
- Error-prone (typos in format string)
- Harder to extend with new parameters

**Trade-off**: More concise but less robust.

### 8.4 Inference-Only with Stronger Validation

**Approach**: Keep inference but add post-inference diff review.

**Rejected because**:
- Doesn't solve "parser intent" problem
- Inference drifts with sample variation
- No source of truth for expected schema

**Trade-off**: Lower migration effort but doesn't meet regulated industry bar.

### 8.5 Hard Fail on Any Violation

**Approach**: Original v0.1 proposal - any schema violation stops the batch.

**Rejected because**:
- Operationally unacceptable for large datasets
- T+1 settlement scenarios can't wait for developer fix
- One bad row shouldn't block 10M good rows

**Trade-off**: Simpler implementation but doesn't meet operational reality.

### 8.6 Union Type for Dirty Data

**Approach**: Allow `Union<Int64, String>` for columns with mixed types.

**Deferred to v2 because**:
- Arrow Union types have limited support
- Parquet Union handling is inconsistent
- Query engines (DuckDB, Pandas) have poor Union support
- Quarantine pattern handles this case adequately for v1

**Trade-off**: Cleaner data model vs. supporting dirty legacy data inline.

---

## 9. Open Questions (RESOLVED)

All open questions have been resolved based on external review:

### 9.1 Nested Types Scope

**Question**: Should List and Struct types be fully validated in v1?

**Resolution**: **Deep validation required.**

Reasoning: HL7 and ISO 20022 have critical data 4+ levels deep. Validating only the outer type is useless for data integrity. Arrow provides efficient nested validation.

### 9.2 Decimal Precision Limits

**Question**: What precision/scale limits should we impose?

**Resolution**: **Decimal128 only (precision ≤ 38).**

Reasoning: Covers all finance use cases. Arrow Decimal128 is well-supported. Decimal256 deferred unless specific need.

### 9.3 Timezone Handling

**Question**: Should we require explicit timezone or allow timezone-naive timestamps?

**Resolution**: **Explicit handling. No silent UTC default.**

- `timestamp_tz(UTC)` + no TZ in data → Quarantine (violation)
- `timestamp` (naive) + no TZ in data → Accept

Reasoning: "Do not silently default to UTC. That is how medical errors happen."

### 9.4 Schema Evolution

**Question**: How should we handle schema changes after initial approval?

**Resolution**: **Manual amendment only (current approach) with version bump.**

Reasoning: Auto-detection of compatible changes is complex and risky for regulated use cases. Explicit version bumps are clearer.

Future consideration: Add tooling to suggest compatible amendments.

### 9.5 Inference Fallback

**Question**: When parser has `outputs` manifest but observed data differs, what happens?

**Resolution**: **Manifest is source of truth. Differences shown in diff view.**

- Preview shows diff between intent and observed
- User reviews and approves (or fixes parser)
- Runtime validates against contract (from manifest)
- Violations go to quarantine

### 9.6 Performance Impact

**Question**: Pre-write validation adds overhead. Is per-row validation acceptable?

**Resolution**: **Validate every row, but in Rust/Arrow layer.**

- "You cannot sample compliance. If you miss one trade break, the bank gets fined."
- Python loop validation is too slow
- Arrow provides vectorized validation
- Rust validator receives Arrow batches via ZMQ (zero-copy)

---

## 10. Future Considerations

### 10.1 Union Type (v2)

For dirty legacy data where a column is "mostly Int, sometimes String":

```python
{"kind": "union", "variants": [{"kind": "int64"}, {"kind": "string"}]}
```

Deferred because:
- Arrow/Parquet/query engine support is limited
- Quarantine pattern handles this for v1

### 10.2 Schema Registry (v1.1)

Shared schema definitions with build-time resolution:

```python
from casparian_schemas import PID_SEGMENT

class ADTParser:
    outputs = {"patients": {"columns": [PID_SEGMENT, ...]}}
```

### 10.3 Multi-Output Handling

**Question not addressed in original review**: Parser with 3 outputs, one fails validation. What happens to the other two?

**Proposed behavior**:
- Each output is independent
- Output A succeeds → written
- Output B fails → quarantined
- Output C succeeds → written
- Job status: "partial success" with details

### 10.4 Streaming Validation

**Question not addressed in original review**: For streaming files (FIX log stream), can we validate and write incrementally?

**Proposed behavior**:
- Parser yields batches incrementally
- Each batch validated and written immediately
- Quarantine file grows incrementally
- Final stats aggregated at end

### 10.5 Migration Path

**Question not addressed in original review**: How do existing parsers transition?

**Proposed path**:
1. Parsers without `outputs` continue to work (inference mode)
2. TUI shows warning: "Parser lacks outputs manifest. Schema inferred."
3. CLI command: `casparian schema infer parser.py` generates manifest from sample
4. User adds generated manifest to parser
5. Next approval creates contract from manifest

---

## Appendix: Current Code

### A.1 Current DataType Enum

```rust
// crates/casparian_schema/src/contract.rs (lines 229-252)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    /// UTF-8 string
    String,

    /// 64-bit signed integer
    Int64,

    /// 64-bit floating point
    Float64,

    /// Boolean (true/false)
    Boolean,

    /// Date (no time component)
    Date,

    /// Timestamp with timezone
    Timestamp,

    /// Binary data
    Binary,
}
```

### A.2 Current SchemaContract

```rust
// crates/casparian_schema/src/contract.rs (lines 14-36)

pub struct SchemaContract {
    pub contract_id: Uuid,
    pub scope_id: String,           // Currently just a string
    pub scope_description: Option<String>,
    pub approved_at: DateTime<Utc>,
    pub approved_by: String,
    pub schemas: Vec<LockedSchema>,
    pub version: u32,
}
```

### A.3 Current Metadata Extractor

```python
# Embedded in crates/casparian/src/cli/tui/app.rs (lines 7869-7942)

import ast
import json
import sys
import os

def extract_metadata(path):
    """Extract parser metadata via AST parsing (no execution)."""
    try:
        source = open(path).read()
        tree = ast.parse(source)
    except SyntaxError as e:
        return {"error": f"Syntax error: {e}"}
    except Exception as e:
        return {"error": str(e)}

    result = {
        "name": None,
        "version": None,
        "topics": [],
        "has_transform": False,
        "has_parse": False,
    }

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for item in node.body:
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            try:
                                value = ast.literal_eval(item.value)
                                if target.id == "name":
                                    result["name"] = value
                                elif target.id == "version":
                                    result["version"] = value
                                elif target.id == "topics":
                                    result["topics"] = value if isinstance(value, list) else [value]
                            except:
                                pass
                elif isinstance(item, ast.FunctionDef):
                    if item.name == "transform":
                        result["has_transform"] = True
                    elif item.name == "parse":
                        result["has_parse"] = True

    if result["name"] is None:
        result["name"] = os.path.splitext(os.path.basename(path))[0]

    return result
```

### A.4 Current PreviewResult

```rust
// From specs/views/parser_bench.md (lines 368-380)

pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Vec<SchemaColumn>,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub error_type: Option<String>,
    pub suggestions: Vec<String>,
    pub truncated: bool,
}

pub struct SchemaColumn {
    pub name: String,
    pub dtype: String,  // String like "Int64", "Float64"
}
```

### A.5 HL7 Message Structure Example

```
MSH|^~\&|EPIC|HOSPITAL|LAB|LAB|20240115120000||ADT^A01|MSG001|P|2.5.1
EVN|A01|20240115120000
PID|1||MRN001^^^HOSPITAL^MR||DOE^JOHN^Q||19800515|M|||123 MAIN ST^^CHICAGO^IL^60601
PV1|1|I|ICU^101^A^HOSPITAL||||1234^SMITH^JANE^M^MD|||MED||||ADM|
OBX|1|NM|8867-4^Heart rate^LN||72|/min|60-100||||F
OBX|2|NM|8480-6^Systolic BP^LN||120|mm[Hg]|90-140||||F

Key structure:
- Segments: Lines (MSH, PID, PV1, OBX)
- Fields: Separated by |
- Components: Separated by ^
- Subcomponents: Separated by &
- Repetitions: Separated by ~

This is inherently nested - a flat schema loses semantics.
```

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-01-16 | Initial RFC for external review |
| 0.2 | 2026-01-16 | Incorporated external review feedback: Added Quarantine pattern (6.7); Changed "hard fail" to "explicit violations" philosophy; Updated scope_id to AST-normalized hash (6.4); Resolved all open questions (9.x); Upgraded runtime validation to HIGH complexity; Added future considerations (10.x); Added multi-output and streaming notes |

---

## Summary of Key Decisions (v0.2)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Violation handling | Quarantine, not hard fail | Operational resilience for large datasets |
| Scope ID hashing | AST-normalized | Comment changes shouldn't break contracts |
| Validation layer | Rust/Arrow | Python too slow for 10M rows |
| Nested validation | Deep | HL7/ISO 20022 have critical nested data |
| Timezone handling | Explicit, no silent UTC | Medical error prevention |
| Decimal precision | Decimal128 (≤38) | Covers finance use cases |
| Union type | Deferred to v2 | Quarantine handles dirty data for now |
| Schema reuse | Deferred to v1.1 | Build-time bake pattern planned |

---

## Feedback Instructions

This RFC has been revised based on external review. Further feedback welcome on:

1. **Quarantine implementation details**: Is the threshold logic correct?
2. **AST-normalized hashing**: Are there edge cases we're missing?
3. **Multi-output handling**: Is independent validation the right choice?
4. **Migration path**: Is the inference → manifest transition smooth enough?

Please provide structured feedback with specific references to sections.

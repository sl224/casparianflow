# RFC: Parser Schema Contract System

**Status:** Working Draft (directional, not binding)
**Version:** 0.3
**Date:** 2026-01-16
**Authors:** Casparian Flow Team
**Reviewers:** External LLM Review + 3-Round Spec Refinement Complete

---

## Purpose of This Document

This RFC proposes a strengthened schema contract system for Casparian Flow. Version 0.3 is the current direction, but it is not a binding contract with the codebase. Expect gaps and update this document as reality evolves.

**Spec posture (v1):**
- Treat this RFC as guidance, not law.
- Prefer real product needs and runtime behavior; update the RFC quickly when it diverges.
- Defer features that are not critical for PMF.

**v1 direction updates (PMF-driven):**
- v1 contracts support primitives + Decimal + timestamp_tz; List/Struct are deferred to v1.1.
- Rust validation is authoritative; parser `_cf_row_error` is optional and additive.
- Job status uses PartialSuccess with per-output status; CompletedWithWarnings is a compat alias if needed.
- Minimum lineage is `_source_row` or `_output_row_index`; `__cf_row_id` is optional; domain provenance is optional.

**Key changes in v0.3:**
- Complete DataType serde with backward compatibility (Section 6.2)
- Simplified logic hash (file-content SHA-256) for advisory change detection (Section 6.4)
- Quarantine file naming convention defined (Section 6.7.1)
- Schema approval state machine for Parser Bench (Section 6.9)
- QuarantineConfig 4-level cascade (Section 6.7.3)
- Multi-output handling with PartialSuccess status (Section 6.8)
- Validation architecture: Rust/Arrow, not Python (Section 5.4)
- Scope ID binding uses explicit parser_id + version + output_name (Section 6.4)
- Per-contract quarantine policy (Section 6.7.3)

**v0.2 changes (incorporated):**
- Added Quarantine pattern for partial failure handling (Section 6.7)
- Changed "hard failures" to "explicit violations" philosophy
- Added file hashing for change detection (Section 6.4)
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
4. **Governance Built-In (Lightweight)**: Approval logs, versioning, and reproducible runs are core features; heavy enterprise workflow is out of scope for v1.
5. **Operational Resilience**: One bad row should not block processing of valid data when policy allows. Violations are quarantined or fail per contract policy, never silent.

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

### 2.5 Detailed Customer Context (Non-superficial)

This section grounds the schema contract design in real user workflows and constraints. It is intended to help reviewers evaluate PMF, gaps, and practical trade-offs.

#### 2.5.1 Finance: Trade Support / Middle Office (FIX Logs)

**Who they are**: Trade support analysts and middle-office ops teams debugging trade breaks under T+1 settlement pressure. They are not operating the production trading system; they analyze FIX logs and execution reports after the fact.

**Daily workflow**:
- Search FIX logs for `ClOrdID` and `ExecID`
- Reconstruct order lifecycle (NewOrderSingle -> ExecutionReport -> Cancel/Reject)
- Validate timestamps, quantities, and prices for reconciliation

**What they care about**:
- Deterministic schemas (Decimal, timestamp_tz) to avoid false mismatches
- Traceability back to FIX identifiers (MsgSeqNum 34, ClOrdID 11, ExecID 17)
- Fast turnaround and minimal operational friction

**What they do NOT need**:
- Heavy enterprise approval workflows
- Deep nested validation (FIX is flat tag/value)

**Definition of success**:
- "I can explain the break in minutes and trust the numbers."

#### 2.5.2 Healthcare: HL7 Analysts (Archive Analytics)

**Who they are**: Hospital data analysts and clinical informaticists querying HL7 archives. They are not replacing interface engines; they need historical access without waiting on integration teams.

**Daily workflow**:
- Pull HL7 files from network shares (thousands to millions of files)
- Extract key segments (MSH, PID, PV1, OBX) into flat tables
- Run SQL for operational/clinical analytics

**What they care about**:
- Stable schemas over time (contracted columns)
- Explicit nullability and timestamp handling (avoid silent defaults)
- Strong privacy posture (PHI handling, minimal raw data retention)

**What they do NOT need (v1)**:
- Fully recursive nested validation if outputs are flattened

**Definition of success**:
- "I can query HL7 archives without custom scripts, and results are trustworthy."

#### 2.5.3 Legal/eDiscovery: Litigation Support

**Who they are**: Litigation support specialists processing PST and load files for discovery prep. Cost-sensitive, often in smaller firms or service providers.

**Daily workflow**:
- Process PST/load files into searchable tables
- Filter by custodian/date/topic
- Export or load into review platforms

**What they care about**:
- Predictable schemas that don't drift across runs
- Clear errors when parsing fails (avoid silent data loss)
- Privacy controls for raw data in quarantine outputs

**Definition of success**:
- "We can process collections in-house without paying vendor fees."

#### 2.5.4 Defense: Tactical Analysts (Air-Gapped)

**Who they are**: Analysts in DDIL environments working on disconnected systems with CoT/telemetry/NITF archives.

**Daily workflow**:
- Load data from external drives
- Run parsers locally with no network access
- Query outputs for situational analysis

**What they care about**:
- Offline determinism and reliability
- Clear failure modes, no dependency on external services
- Minimal operational complexity

**Definition of success**:
- "I can turn raw files into queryable tables on a laptop in the field."

---

## 3. Current Architecture

### 3.1 System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CASPARIAN FLOW                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                │
│  │   Scout     │────▶│   Parser    │────▶│   Output    │                │
│  │ (Discovery) │     │   Bench     │     │ (Parquet)   │                │
│  └─────────────┘     └─────────────┘     └─────────────┘                │
│         │                   │                   │                        │
│         ▼                   ▼                   ▼                        │
│  Tag files by         Test parsers,       Query with SQL                │
│  pattern/type         approve schemas     (DuckDB/SQLite)               │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │                    Schema Contracts                      │            │
│  │  • LockedSchema: immutable column definitions           │            │
│  │  • SchemaContract: binds schema to scope                │            │
│  │  • Violations: explicit with quarantine                 │            │
│  └─────────────────────────────────────────────────────────┘            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Parser Structure

Parsers are Python files with class attributes:

```python
# ~/.casparian_flow/parsers/trade_parser.py

class TradeParser:
    name = 'trade_parser'       # Logical name
    version = '1.0.0'           # Semver
    topics = ['fix_logs']       # What file tags this parser handles

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
    pub approval_note: Option<String>,  // Optional human note (lightweight governance)
    pub schemas: Vec<LockedSchema>,
    pub version: u32,
}

/// Immutable definition of expected data structure.
pub struct LockedSchema {
    pub schema_id: Uuid,
    pub name: String,               // e.g., "trades"
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
    Float64,    // <-- Problem: Finance needs Decimal
    Boolean,
    Date,
    Timestamp,  // <-- Problem: No timezone specification
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
    pub schema: Vec<SchemaColumn>,  // <-- INFERRED from sample data
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub truncated: bool,
}

pub struct SchemaColumn {
    pub name: String,
    pub dtype: String,  // String like "Int64", "Float64"
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
| Unclear lineage for quarantined rows | Misleading audit trail | All |

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
- Violations quarantined or hard-failed per contract policy; valid data flows when allowed

---

## 5. Proposed Solution

### 5.1 Solution Overview

1. **Parser Manifest**: Parsers declare expected output schema (`outputs` attribute)
2. **Expanded Type System**: Add Decimal, TimestampTz, List, Struct
3. **Intent vs Observed**: Preview shows declared vs inferred schema with diff
4. **Strong Scope Binding**: Contract tied to explicit parser_id + version + output_name (file hash is advisory metadata)
5. **Rust/Arrow Validation**: Validate in Rust layer for performance (not Python)
6. **Quarantine Pattern**: Invalid rows written to separate file if explicitly enabled per contract (valid rows proceed)

### 5.2 Design Principles

| Principle | Implementation |
|-----------|----------------|
| AST-safe extraction | `outputs` must be literal dicts (no function calls) |
| No SDK required | Plain Python dicts work; SDK is optional helper |
| Explicit over implicit | Nullable defaults to false; mode must be specified in production |
| Violations are explicit | Quarantine or fail per contract policy; never silently coerce |
| Operational resilience | One bad row doesn't block valid data |
| Validation in Rust | Pre-write validation via Arrow for performance |
| Backward compatible | Parsers without `outputs` continue to work (with warnings) |
| Lightweight governance | Approval log + reproducible runs (not enterprise workflow) |

### 5.3 High-Level Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    PROPOSED SCHEMA CONTRACT FLOW                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. PARSER AUTHORING                                                     │
│     └─ Developer adds `outputs` manifest to parser                       │
│     └─ Optional: CLI validates manifest syntax                           │
│                                                                          │
│  2. DISCOVERY (AST Extraction)                                           │
│     └─ TUI extracts `outputs` via ast.literal_eval()                    │
│     └─ No code execution; safe for untrusted parsers                    │
│                                                                          │
│  3. PREVIEW (Test in Bench)                                              │
│     └─ Run parser on sample file                                         │
│     └─ Compare: schema_intent (from manifest) vs schema_observed        │
│     └─ Show diff: missing columns, type mismatches, extra columns       │
│     └─ Show confidence: sample size, null %, type coercions             │
│                                                                          │
│  4. APPROVAL (Create Contract)                                           │
│     └─ User reviews diff, approves if acceptable                         │
│     └─ Contract created with scope = parser_id + version + output_name  │
│     └─ File hash stored as advisory metadata                            │
│     └─ Contract stored in SQLite with audit metadata                     │
│                                                                          │
│  5. RUNTIME (Production Execution)                                       │
│     └─ Worker loads parser, resolves contract by scope                  │
│     └─ Parser yields Arrow batches to Rust                              │
│     └─ Rust validates Arrow schema against contract (fast, zero-copy)   │
│     └─ Valid rows → output.parquet                                      │
│     └─ Invalid rows → output_quarantine.parquet + error metadata        │
│     └─ No contract = fail (unless --dev flag)                            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.4 Validation Architecture

**Critical**: Validation MUST happen in Rust/Arrow layer, not Python.

```
┌──────────────────────────────────────────────────────────────────┐
│                        VALIDATION PATH                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Parser (Python)                                                  │
│       │                                                           │
│       ▼ Arrow IPC batches (via ZMQ)                              │
│       │                                                           │
│  ┌────┴────────────────────────────────────┐                     │
│  │         Rust Validator                   │                     │
│  │  • Receives Arrow RecordBatch           │                     │
│  │  • Validates schema shape (O(columns))  │                     │
│  │  • Validates per-row constraints (O(rows))                    │
│  │  • Zero-copy where possible             │                     │
│  └────┬─────────────────────────┬──────────┘                     │
│       │                         │                                 │
│       ▼                         ▼                                 │
│  Valid Rows               Invalid Rows                            │
│  → output.parquet         → output_quarantine.parquet            │
│                              + _error_msg                         │
│                              + _source_row                        │
│                              + _raw_data                          │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

**Why Rust validation?**
- Python loop over 10M rows is too slow
- Arrow provides vectorized validation but still incurs O(rows) for per-value checks
- Zero-copy inspection of column types
- Per-value checks (nullability, precision) via Arrow compute kernels

**Performance constraints (NEW):**
- Define explicit budgets (e.g., rows/sec on reference hardware)
- Preview defaults to schema-shape validation only (no full per-row checks)
- Dev mode may sample per-row validation; production validates every row

**IPC cost note (NEW):**
- Arrow IPC over ZMQ/pipe typically involves serialization and copy
- "Zero-copy" only applies when shared memory (e.g., mmap/plasma) is used

---

## 6. Detailed Design

### 6.1 Parser Manifest Format

The `outputs` attribute is a dict with literal values (AST-extractable):

```python
class TradeParser:
    name = 'trade_parser'
    version = '1.0.0'
    parser_id = 'trade_parser'  # Stable identifier; defaults to name if omitted
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

**Safe Read Pattern (NEW):**
- For financial data, avoid implicit float parsing in Python readers.
- Recommended: read as strings, then parse to Decimal/Int explicitly.
- Example (pandas):
```python
df = pd.read_csv(path, dtype=str)
df["price"] = df["price"].apply(Decimal)
```
- Parsers that cast from float to Decimal may already be precision-lost before validation.

**Sink precedence (NEW):**
- CLI `--sink` overrides global config.
- If no CLI sink, use the global config default.
- If none are set, use the standard output directory.

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
{"kind": "timestamp_tz", "tz": "UTC"}           # Explicit timezone REQUIRED
{"kind": "timestamp_tz", "tz": "America/New_York"}
{"kind": "timestamp"}                            # Naive (no timezone) - explicit choice

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

**Implementation requirement (NEW):**
- Rust validation must define a timezone parsing/normalization dependency (e.g., `chrono-tz`)
- Timezone parsing must be benchmarked as part of the validation hot path
- Validation must pin a specific tzdb version for deterministic results across machines

#### 6.2.3 Nested Type Validation (UPDATED)

**Decision (v1 PMF)**: List/Struct are deferred in v1 contracts. Approvals should reject nested types unless a feature flag is enabled.

- Preferred: emit child outputs for 1:N relationships.
- If nested data is unavoidable, encode it as JSON in a String column.
- If List/Struct are enabled, v1 validates only outer shape and top-level fields; deep validation is deferred to v1.1.

Arrow provides efficient nested access via `StructArray` and `ListArray` accessors.

**Limits (future):**
- Define maximum nesting depth and field/element counts when List/Struct are enabled.
- Exceeding limits is a contract violation (quarantine or fail per policy).

#### 6.2.4 Rust DataType Enum (Updated)

**v1 constraint:** Approved contracts should use only primitives + Decimal + timestamp_tz. `List` and `Struct` are defined here but reserved for v1.1 unless a feature flag is enabled.

```rust
/// Canonical data type enum - the SINGLE SOURCE OF TRUTH for data types.
///
/// # Breaking Change: No Longer Implements Copy
///
/// With the addition of composite types (`List`, `Struct`), `DataType` can no longer
/// implement `Copy` because it contains heap-allocated data (`Box<DataType>`, `Vec<StructField>`).
///
/// **Migration Guide:**
/// - Change `fn foo(dt: DataType)` to `fn foo(dt: &DataType)` or use `.clone()`
/// - Use `ref` patterns in match arms: `DataType::Decimal { ref precision, .. }`
///
/// # Serialization Compatibility
///
/// This enum supports TWO serialization formats for backward compatibility:
///
/// **Legacy format (primitive types only):**
/// ```json
/// "float64"
/// ```
///
/// **New format (all types including parameterized):**
/// ```json
/// {"kind": "decimal", "precision": 18, "scale": 8}
/// ```
///
/// Deserialization accepts both formats. Serialization uses the appropriate
/// format based on variant type (primitives use legacy, parameterized use new).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    // Primitive types (serialize as strings for backward compat)
    Null,
    Boolean,
    String,
    Int64,
    Float64,
    Date,
    Timestamp,  // Naive (no timezone)
    Time,
    Duration,
    Binary,

    // Parameterized types (serialize as objects with "kind" tag)
    Decimal { precision: u8, scale: u8 },  // Decimal128 (precision <= 38)
    TimestampTz { tz: String },            // Explicit timezone required

    // Composite types (serialize as objects with "kind" tag)
    List { item: Box<DataType> },
    Struct { fields: Vec<StructField> },

    // DEFERRED to v2: Union type for dirty legacy data
    // Union { variants: Vec<DataType> },
}

/// A field within a Struct type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}
```

**Custom Serde Implementation (backward compatible):**

```rust
// Primitives serialize as strings (legacy format)
serde_json::to_string(&DataType::Float64)
// => "\"float64\""

// Parameterized types serialize as objects
serde_json::to_string(&DataType::Decimal { precision: 18, scale: 8 })
// => "{\"kind\":\"decimal\",\"precision\":18,\"scale\":8}"

// Deserialization accepts BOTH formats
serde_json::from_str::<DataType>("\"float64\"")  // Legacy
serde_json::from_str::<DataType>("{\"kind\": \"float64\"}")  // New format for primitives
serde_json::from_str::<DataType>("{\"kind\": \"decimal\", \"precision\": 18, \"scale\": 8}")
```

**Arrow Type Conversion (via extension trait in casparian_schema):**

```rust
// In crates/casparian_schema/src/arrow_compat.rs
pub trait ToArrow {
    fn to_arrow(&self) -> arrow::datatypes::DataType;
}

impl ToArrow for DataType {
    fn to_arrow(&self) -> arrow::datatypes::DataType {
        match self {
            DataType::Decimal { precision, scale } =>
                ArrowDT::Decimal128(*precision, *scale as i8),
            DataType::TimestampTz { tz } =>
                ArrowDT::Timestamp(TimeUnit::Microsecond, Some(tz.clone().into())),
            DataType::List { item } =>
                ArrowDT::List(Arc::new(Field::new("item", item.to_arrow(), true))),
            // ... other variants
        }
    }
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
| `strict` | Exact match required. Extra/missing columns = violation (quarantine or fail per policy). | Production, regulated |
| `allow_extra` | Parser may output additional columns (ignored). | Development iteration |
| `allow_missing_optional` | Optional (nullable) columns may be absent. | Flexible schemas |

**Default behavior by context:**

| Context | Default Mode | Rationale |
|---------|--------------|-----------|
| `casparian run` (dev) | `allow_extra` | Fast iteration |
| Parser Bench preview | `allow_extra` + warnings | Show what would fail |
| `casparian worker` (prod) | Require explicit | No implicit leniency |

**Audit for allow_extra (NEW):**
- Extra columns must be logged and surfaced in job metadata
- Optional: allow a per-contract flag to retain extra columns as String in output

### 6.4 Scope ID Derivation (UPDATED)

**Change from v0.1**: Use explicit parser identity as the primary key; a simple file-content hash is advisory metadata.

**Problem with file-string hash**: A comment typo changes the hash, breaking the contract even though logic didn't change.

**Solution**: Use explicit identifiers for binding; compute a lightweight file hash as a change-signal only.

#### 6.4.1 File Hash (Advisory)

```python
import hashlib

def compute_file_hash(source: str) -> str:
    return hashlib.sha256(source.encode("utf-8")).hexdigest()
```

**Notes:**
- This hash is advisory only (UI warning if changed without version bump)
- It is not used for contract lookup

**Scope ID formula (UPDATED):**
```
scope_id = sha256(
    parser_id + ":" +
    parser_version + ":" +
    output_name
)
```

**Hash metadata:**
- Stored alongside contract as `logic_hash` (file hash in v1)
- Used to warn when parser logic changes without version bump
- Does not affect contract lookup

**Topic binding (NEW):**
- Topics are used for parser discovery/applicability, not for contract identity

**Note:** Full AST normalization is deferred to v1.1 (see Future Considerations).

### 6.5 Preview Result (Extended)

**v1 note:** CLI preview may return only file metadata + inferred schema. `schema_intent` and `schema_diff` are optional until Parser Bench/approval is fully built.

```rust
pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,

    // Schema comparison (NEW)
    pub schema_intent: Option<SchemaIntent>,     // From manifest
    pub schema_observed: Vec<SchemaColumn>,      // From sample
    pub schema_diff: Option<SchemaDiff>,         // Differences

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
    pub missing_columns: Vec<String>,       // In intent, not observed
    pub extra_columns: Vec<String>,         // In observed, not intent
    pub type_mismatches: Vec<TypeMismatch>,
    pub nullable_mismatches: Vec<NullableMismatch>,
}

pub struct SchemaConfidence {
    pub rows_sampled: usize,
    pub null_percentages: HashMap<String, f64>,
    pub type_coercions: Vec<String>,  // Only if explicit coercions are enabled
    pub level: ConfidenceLevel,  // High, Medium, Low
}

**Coercion policy (NEW):**
- Default: no implicit coercions; mismatches are violations
- If enabled for dev preview, the allowed coercions must be explicit and logged

**Preview UX simplification (NEW):**
- v1 may display only a summary of mismatches (counts + list), not full diff/metrics
- SchemaConfidence is deferred to v1.1 and is not part of the v0.3 PreviewResult
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
    pub provenance: Option<ViolationProvenance>, // Domain-specific lineage hints
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
    LineageUnavailable,
}

/// Optional lineage hints for debugging (FIX/HL7/etc.)
pub struct ViolationProvenance {
    pub source_row: Option<u64>,
    pub output_row_index: Option<u64>,
    pub fix_msg_seqnum: Option<String>,  // FIX tag 34
    pub fix_clordid: Option<String>,     // FIX tag 11
    pub fix_execid: Option<String>,      // FIX tag 17
    pub fix_sending_time: Option<String> // FIX tag 52
}

**Policy note (NEW):**
- Violations are either quarantined or hard-fail depending on resolved `QuarantineConfig`
- v1 quarantine output uses a coarse mapping of violation types (`schema`, `null_not_allowed`, `parser`, `unknown`) derived from `_cf_row_error`; richer ViolationType wiring is deferred.
```

### 6.7 Quarantine Pattern (NEW)

**Critical addition based on external review.**

**Policy (NEW):** Quarantine is opt-in per contract; production defaults to hard-fail unless `allow_quarantine` is explicitly enabled.

The Quarantine pattern ensures one bad row doesn't block valid data:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     QUARANTINE PATTERN                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Parser Output: 10,000,000 rows                                         │
│                     │                                                    │
│                     ▼                                                    │
│            ┌────────────────┐                                           │
│            │ Rust Validator │                                           │
│            └───────┬────────┘                                           │
│                    │                                                     │
│        ┌───────────┴───────────┐                                        │
│        │                       │                                         │
│        ▼                       ▼                                         │
│  9,999,999 valid          1 invalid                                      │
│        │                       │                                         │
│        ▼                       ▼                                         │
│  ┌──────────────┐    ┌───────────────────────────┐                      │
│  │trades.parquet│    │trades_quarantine.parquet  │                      │
│  │              │    │                           │                      │
│  │ Normal       │    │ All columns as String     │                      │
│  │ schema       │    │ + _source_row: u64        │                      │
│  │              │    │ + _error_msg: String      │                      │
│  │              │    │ + _raw_data: String       │                      │
│  │              │    │ + _cf_job_id: String      │                      │
│  └──────────────┘    └───────────────────────────┘                      │
│                                                                          │
│  UI Message: "Processed 10,000,000 rows with 1 quarantined.             │
│              [View Quarantine]"                                          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

#### 6.7.1 Quarantine File Naming Convention

**File naming format:**
```
{output_name}_quarantine_{job_id}.parquet
```

**Examples:**
| Main Output | Quarantine File |
|-------------|-----------------|
| `trades_abc123.parquet` | `trades_quarantine_abc123.parquet` |
| `orders_def456.parquet` | `orders_quarantine_def456.parquet` |

**Location:** Same directory as main output unless `quarantine_dir` is explicitly set.

```
~/.casparian_flow/output/
├── trades_abc123.parquet           # Main output
├── trades_quarantine_abc123.parquet # Quarantine for same job
├── trades_def456.parquet           # Different job
└── trades_quarantine_def456.parquet # Its quarantine
```

**Rationale:**
- Underscore suffix (`_quarantine`) is easier to glob: `trades*.parquet` matches both
- Including job_id enables matching quarantine to its corresponding main output
- Same directory simplifies filesystem layout

**SQLite/DuckDB Special Case:**
For SQLite and DuckDB sinks, quarantine goes to a separate table, not a separate file:
```sql
-- Main output table
CREATE TABLE trades (...);

-- Quarantine table (same database)
CREATE TABLE trades_quarantine (
    _source_row INTEGER, -- present when lineage is available
    _output_row_index INTEGER, -- present when lineage is unavailable
    _error_msg TEXT,
    _violation_type TEXT,
    _raw_data TEXT,
    _cf_job_id TEXT,
    -- All original columns as TEXT
    ...
);
```

#### 6.7.2 Quarantine File Schema

| Column | Type | Description |
|--------|------|-------------|
| `_source_row` | Int64 | Original row number in source file (present when lineage is available) |
| `_output_row_index` | Int64 | Row index in parser output (present when source lineage unavailable) |
| `_error_msg` | String | Detailed violation message (copied from `_cf_row_error` in v1) |
| `_violation_type` | String | Enum (v1): `schema`, `null_not_allowed`, `parser`, `unknown` |
| `_raw_data` | String | JSON-serialized original row data (optional) |
| `_cf_job_id` | String | Job ID for lineage |
| `_cf_source_hash` | String | Source file hash |
| `_cf_processed_at` | Timestamp | When processing occurred |
| `*` (all columns) | String | Original columns coerced to String |

**v1 minimum:** `_error_msg`, `_violation_type`, `_cf_job_id`, and either `_source_row` or `_output_row_index` are required. In v1, the worker emits exactly one of `_source_row` or `_output_row_index` based on lineage availability; the other may be omitted. Other fields are optional until v1.1.

**Lineage requirement (NEW):**
- Parsers SHOULD provide `__cf_row_id` when source-row lineage is required.
- If the parser cannot provide source-row lineage (e.g., aggregation/reordering), quarantine records must use `_output_row_index`.
- Missing/invalid lineage is logged as a warning and recorded in `lineage_unavailable_rows` metrics (policy enforcement deferred).

**FIX-specific provenance (NEW):**
- For FIX logs, prefer capturing `MsgSeqNum (34)`, `ClOrdID (11)`, `ExecID (17)`, and `SendingTime (52)` in quarantine metadata or provenance fields.
- These identifiers are more useful than raw row indexes for trade break investigations.

**Lineage protocol (NEW):**
- Parsers MAY emit a reserved column `__cf_row_id` to preserve source row lineage.
- If present, Rust uses `__cf_row_id` for `_source_row` in quarantine output.
- `__cf_row_id` MUST be an integer type (`Int64` or `UInt64`).
- If `__cf_row_id` has an invalid type, Rust ignores it, uses batch index, emits a lineage warning, and increments `lineage_unavailable_rows` metrics.
- If absent, Rust uses batch index and increments `lineage_unavailable_rows` metrics.

**Privacy & retention (NEW):**
- Quarantine files may contain raw PII/PHI; define retention and encryption-at-rest requirements
- Provide a config option to redact `_raw_data` or disable it entirely in regulated environments

#### 6.7.3 Quarantine Threshold & Configuration

If quarantine exceeds threshold, the job fails entirely:

```rust
pub struct QuarantineConfig {
    /// Maximum percentage of rows that can be quarantined before failing
    pub max_quarantine_pct: f64,  // Default: 10.0 (10%)

    /// Maximum absolute count of quarantined rows before failing
    pub max_quarantine_count: Option<usize>,  // Default: None (no limit)

    /// Whether to emit warnings for quarantine (not just errors)
    pub warn_on_quarantine: bool,  // Default: true

    /// Whether quarantine is allowed or production must hard-fail on any violation
    pub allow_quarantine: bool,  // Default: false in prod, true in dev/preview

    /// Whether to include raw row data in quarantine records
    pub include_raw_data: bool,  // Default: false in prod

    /// Optional directory override for quarantine outputs
    pub quarantine_dir: Option<String>, // Default: None (same as main output)
}
```

**Behavior:**
- If 5% of rows quarantined: Job succeeds with warning
- If 15% of rows quarantined: Job fails (exceeds 10% threshold)
- Threshold configurable per contract or globally
- If `allow_quarantine = false`: any violation fails the job immediately

**Defaults (explicit):**
- `allow_quarantine`: false in prod, true in preview/dev
- `include_raw_data`: false in prod, false in preview/dev unless explicitly enabled
- `quarantine_dir`: None (same directory as main output) unless explicitly set

**4-Level Configuration Cascade:**

```
1. System Default (Rust code constant)
      ↓ overridden by
2. Global Config (cf_config table)
      ↓ overridden by
3. Per-Contract Config (schema_contracts.quarantine_config_json)
      ↓ overridden by
4. CLI Flag (--quarantine-threshold, --allow-quarantine)
```

**Storage locations:**

| Level | Location | Format | Precedence |
|-------|----------|--------|------------|
| System | `QuarantineConfig::default()` | Rust code | Lowest |
| Global | `cf_config` table, key=`quarantine_config` | JSON | Medium |
| Contract | `schema_contracts.quarantine_config_json` | JSON (nullable) | High |
| CLI | `--quarantine-threshold` flag | f64 | Highest |

**Resolution function:**

```rust
pub fn resolve_quarantine_config(
    contract: Option<&SchemaContract>,
    global_config: Option<&QuarantineConfig>,
    cli_override: Option<f64>,
    cli_allow_quarantine: Option<bool>,
) -> QuarantineConfig {
    let mut config = QuarantineConfig::default();
    if let Some(global) = global_config { config.merge(global); }
    if let Some(c) = contract {
        if let Some(contract_config) = &c.quarantine_config { config.merge(contract_config); }
    }
    if let Some(threshold) = cli_override { config.max_quarantine_pct = threshold; }
    if let Some(allow) = cli_allow_quarantine { config.allow_quarantine = allow; }
    config
}
```

**Default policy by context:**
- Parser Bench preview: `allow_quarantine = true` (warn-only)
- `casparian run` (dev): `allow_quarantine = true` (developer iteration)
- `casparian worker` (prod): `allow_quarantine = false` (hard-fail unless explicitly enabled)

**CLI commands:**
```bash
# View current config
casparian config get quarantine_config

# Set global config
casparian config set quarantine_config.max_quarantine_pct 5.0
casparian config set quarantine_config.allow_quarantine false
casparian config set quarantine_config.include_raw_data false
casparian config set quarantine_config.quarantine_dir /var/cf/quarantine

# Run with override
casparian run parser.py input.csv --quarantine-threshold 15.0
casparian run parser.py input.csv --allow-quarantine
```

#### 6.7.4 Quarantine Lineage

Quarantine files have the same `_cf_job_id` as main output, enabling:

```sql
-- Find all data from a job (including quarantined)
SELECT * FROM trades WHERE _cf_job_id = 'abc123'
UNION ALL
SELECT * FROM trades_quarantine WHERE _cf_job_id = 'abc123'
```

### 6.8 Multi-Output Handling

**Problem:** Parser with 3 outputs, one fails validation. What happens?

**Solution:** Per-output status tracking with `PartialSuccess` job status and explicit failure rules.

#### 6.8.1 Job Status Extension

```rust
pub enum JobStatus {
    Success,        // All outputs valid
    PartialSuccess, // Some outputs had quarantine, all under threshold
    Failed,         // At least one output exceeded quarantine threshold
    Rejected,
    Aborted,
}

pub enum OutputStatus {
    Success,      // All rows valid
    Quarantined,  // Some rows quarantined, under threshold
    Failed,       // Exceeded quarantine threshold
}
```

**Compatibility:** `CompletedWithWarnings` may be treated as `PartialSuccess` during transition.

#### 6.8.2 Per-Output Tracking

Each output is tracked independently:

```rust
pub struct OutputResult {
    pub output_name: String,
    pub status: OutputStatus,
    pub valid_rows: u64,
    pub quarantine_rows: u64,
    pub quarantine_pct: f64,
    pub output_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub error_message: Option<String>,
}
```

**Database table:**
```sql
CREATE TABLE IF NOT EXISTS cf_job_outputs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    output_name TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'success', 'quarantined', 'failed'
    valid_rows INTEGER NOT NULL DEFAULT 0,
    quarantine_rows INTEGER NOT NULL DEFAULT 0,
    quarantine_pct REAL NOT NULL DEFAULT 0.0,
    output_path TEXT,
    quarantine_path TEXT,
    error_message TEXT,
    FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id),
    UNIQUE(job_id, output_name)
);
```

#### 6.8.3 Status Resolution

```rust
fn resolve_job_status(outputs: &[OutputResult]) -> JobStatus {
    let any_failed = outputs.iter().any(|o| o.status == OutputStatus::Failed);
    let any_quarantined = outputs.iter().any(|o| o.status == OutputStatus::Quarantined);

    if any_failed { JobStatus::Failed }
    else if any_quarantined { JobStatus::PartialSuccess }
    else { JobStatus::Success }
}
```

**Decision matrix:**

| Output A | Output B | Output C | Job Status |
|----------|----------|----------|------------|
| Success | Success | Success | Success |
| Success | Quarantined | Success | PartialSuccess |
| Quarantined | Quarantined | Success | PartialSuccess |
| Success | Failed | Success | Failed |
| Quarantined (<= threshold, allow_quarantine=true) | Success | Success | PartialSuccess |
| Quarantined (> threshold) | Success | Success | Failed |

**UI/CLI requirement (NEW):**
- `PartialSuccess` must emit a mandatory warning listing affected outputs and quarantine percentages.

#### 6.8.4 Quarantine Files Per-Output

Each output gets its own quarantine file:

| Output | Main File | Quarantine File |
|--------|-----------|-----------------|
| `trades` | `trades_{job_id}.parquet` | `trades_quarantine_{job_id}.parquet` |
| `orders` | `orders_{job_id}.parquet` | `orders_quarantine_{job_id}.parquet` |

**Rationale:** Each output has its own schema contract; quarantine schema differs per output.

---

### 6.9 Schema Approval State Machine (Parser Bench TUI)

**Extension to Parser Bench for schema approval workflow.**

**Simplified v1 flow (NEW):**
- A single "Schema Summary" screen with mismatches + approve/reject
- Full diff view and edit-in-place deferred to v1.1

**v1 vs v1.1 UI roadmap (NEW):**
- v1: Schema Summary + approve/reject only
- v1.1: Side-by-side diff view, edit-in-place, and confidence metrics

**Details moved to Appendix B (v1):**
- State machine, keybindings, and UI mock are in Appendix B to keep the main spec lean

---

### 6.10 Output Sinks (URI-Based)

Outputs can be routed to user-defined sinks using URI syntax. This allows ops teams to control storage, retention, and access without changing parser code.

**URI format (all sinks):**
```
scheme://[user:pass@]host[:port]/path?key=value&key2=value2
```

Note: Sink URIs always use the `scheme://` form. Database connection URLs
use `duckdb:` or `sqlite:` without `//`.

**Supported in v1:**
- `parquet://` (directory or file)
- `csv://` (directory or file)
- `duckdb://` (local DuckDB file)
- `file://` (auto-select format by file extension)

**Examples:**
```
parquet:///var/casparian/output
parquet:///var/casparian/output/trades.parquet
csv:///var/casparian/output
csv:///var/casparian/output/trades.csv
duckdb:///var/casparian/data/cf.duckdb?table=trades
file:///var/casparian/output/trades.parquet
```

**Path semantics:**
- If the path is a directory, output files are named `{output_name}_{job_id}.parquet|csv`
- If the path is a file, it applies only to that output; multi-output jobs must use a directory sink
- `file://` infers format from extension (`.parquet`, `.csv`)

**Per-output vs job-level sinks (rationale):**
- v1 supports a single job-level sink to keep the workflow simple for analysts (one command, one destination).
- Per-output sinks are deferred to v1.1 for advanced workflows.
- Precedence (v1): CLI `--sink` > global config > default output directory.

**Global config + CLI:**
- Global default sink is stored as `output_sink` in config.
- CLI can override with `--sink <uri>`.

**CLI examples:**
```bash
# Job-level sink (all outputs)
casparian run parser.py input.csv --sink parquet:///var/casparian/output
```

**Sink validation (NEW):**
- If a job produces multiple outputs, a file-path sink (e.g., `parquet:///path/file.parquet`) is invalid.
- Directory sinks are always valid for multi-output jobs.
- `duckdb://` requires a `table` query param; omission is a validation error.

**Multiple tables from one input (NEW):**
- One input file may yield multiple outputs; each output maps to a separate table or file within the single job-level sink.
- For DB sinks, the default table name is the `output_name` unless overridden by `?table=`.

**Quarantine routing by sink:**
- Parquet/CSV/File: quarantine files use `{output_name}_quarantine_{job_id}` in the same directory unless `quarantine_dir` is set.
- DuckDB: quarantine table is `{output_name}_quarantine` in the same database file.

**Future sinks (post-v1):**
- `postgresql://` and `mssql://` will use the same URI style.
- Quarantine will use a `{table}_quarantine` table in the same database.

### 6.11 SDK (Optional Authoring Helper)

The SDK is for **validation and codegen**, not inline use:

```python
# WRONG: Function calls not AST-extractable
from casparian import schema
outputs = {"trades": {"columns": [{"type": schema.decimal(18, 8)}]}}  # FAILS

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
| File hashing | Low | Low | Advisory change detection only |
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
  └─ Add scope resolution with explicit parser_id + version + output_name (store file hash as metadata)
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
from casparian_schemas import PID_SEGMENT  # Import shared definition

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

**Resolution**: ✅ **Shallow validation in v1; deep validation deferred to v1.1.**

Reasoning: Most early ICPs consume flattened outputs; deep validation increases complexity and performance cost. Offer strict nested validation as a v1.1 upgrade or opt-in.

**FIX note**: FIX log analysis typically operates on flat tag/value mappings; deep nested validation is not a blocker for trade support workflows.

### 9.2 Decimal Precision Limits

**Question**: What precision/scale limits should we impose?

**Resolution**: ✅ **Decimal128 only (precision ≤ 38).**

Reasoning: Covers all finance use cases. Arrow Decimal128 is well-supported. Decimal256 deferred unless specific need.

### 9.3 Timezone Handling

**Question**: Should we require explicit timezone or allow timezone-naive timestamps?

**Resolution**: ✅ **Explicit handling. No silent UTC default.**

- `timestamp_tz(UTC)` + no TZ in data → Quarantine (violation)
- `timestamp` (naive) + no TZ in data → Accept

Reasoning: "Do not silently default to UTC. That is how medical errors happen."

### 9.4 Schema Evolution

**Question**: How should we handle schema changes after initial approval?

**Resolution**: ✅ **Manual amendment only (current approach) with version bump.**

Reasoning: Auto-detection of compatible changes is complex and risky for regulated use cases. Explicit version bumps are clearer.

Future consideration: Add tooling to suggest compatible amendments.

### 9.5 Inference Fallback

**Question**: When parser has `outputs` manifest but observed data differs, what happens?

**Resolution**: ✅ **Manifest is source of truth. Differences shown in diff view.**

- Preview shows diff between intent and observed
- User reviews and approves (or fixes parser)
- Runtime validates against contract (from manifest)
- Violations go to quarantine

### 9.6 Performance Impact

**Question**: Pre-write validation adds overhead. Is per-row validation acceptable?

**Resolution**: ✅ **Validate every row in production, but acknowledge O(rows) cost; preview/dev may use schema-only validation.**

- "You cannot sample compliance. If you miss one trade break, the bank gets fined."
- Python loop validation is too slow
- Arrow provides vectorized validation but still incurs per-value checks
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

### 10.6 Deep Nested Validation (v1.1)

**Goal**: Full recursive validation of List/Struct contents with configurable depth limits.

**Reason for deferral**: Reduces v1 complexity and performance risk while most early parsers remain flat.

### 10.7 AST Normalizer (v1.1)

**Goal**: Optional AST-normalized hashing for stronger change detection.

**Reason for deferral**: Maintenance overhead outweighs v1 benefit; explicit versioning is primary.


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
    pub scope_id: String,           // Currently just a string
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
    pub dtype: String,  // String like "Int64", "Float64"
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
| 0.2 | 2026-01-16 | Incorporated external review feedback: Added Quarantine pattern (6.7); Changed "hard fail" to "explicit violations" philosophy; Added logic hashing for change detection (6.4); Resolved all open questions (9.x); Upgraded runtime validation to HIGH complexity; Added future considerations (10.x); Added multi-output and streaming notes |
| 0.3 | 2026-01-16 | **Working Draft** after 3-round spec refinement. Key additions: Custom DataType serde for backward compatibility (6.2.4); Simplified file hash for advisory change detection (6.4); Scope ID binding uses explicit parser_id + version + output_name (6.4); Quarantine file naming convention (6.7.1); 4-level QuarantineConfig cascade (6.7.3); Per-contract quarantine policy (6.7.3); Quarantine directory override (6.7.3); Safe Read guidance (6.1); Multi-output handling with PartialSuccess status (6.8); Validation architecture in Rust/Arrow (5.4). PMF-driven updates: v1 subset of types and Rust validation authority. |

---

## Summary of Key Decisions (v0.3)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Violation handling | Quarantine or hard fail per contract | Compliance expectations vary |
| Scope ID hashing | File hash (SHA-256) | Advisory change signal only |
| Scope ID binding | Explicit parser_id + version + output_name | Regulated teams expect explicit versioning |
| Validation layer | Rust/Arrow | Python too slow for 10M rows |
| Nested types (v1) | Deferred (flat only) | PMF focus on FIX/flat outputs |
| Nested validation (v1.1) | Shallow then deep | Introduce nested types safely |
| Nested validation limits | Max depth/width defined | Prevents unbounded perf cost |
| Timezone handling | Explicit, no silent UTC | Medical error prevention |
| Decimal precision | Decimal128 (≤38) | Covers finance use cases |
| Union type | Deferred to v2 | Quarantine handles dirty data for now |
| Schema reuse | Deferred to v1.1 | Build-time bake pattern planned |
| **DataType serde** | Custom impl, dual-format | Backward compatibility with existing contracts |
| **Multi-output status** | PartialSuccess + per-output tracking | Independent failure handling within a single sink |
| **QuarantineConfig** | 4-level cascade | Flexible: system → global → contract → CLI |
| **Quarantine naming** | `{name}_quarantine_{job_id}.parquet` | Same directory, easy to correlate |
| **Quarantine raw data** | Optional, off by default in prod | Reduce PII/PHI exposure |
| **Quarantine directory** | Optional override | Supports retention policies |
| **TUI state machine** | 4 states (REVIEW→CONFIRM→EDIT→DONE) | Clear approval workflow |

---

## Implementation Status

**Status:** DRAFT (stale; reconcile with code)

Note: This section is directional and may be out of date.

| Category | Count | Status |
|----------|-------|--------|
| Initial Gaps | 15 | - |
| Accepted (Ready) | 7 | ✅ Implementation order defined |
| Remaining MEDIUM | 6 | TODOs for implementation |
| Remaining LOW | 5 | TODOs for future |
| CRITICAL/HIGH | 0 | ✅ All resolved |

**Implementation order:**
1. GAP-IMPL-001: DataType extension (Foundation)
2. GAP-IMPL-004: File hashing (Advisory change detection)
3. GAP-IMPL-003: QuarantineConfig (Config before validation)
4. GAP-IMPL-002: Validation (Core quarantine logic)
5. GAP-IMPL-007: Quarantine naming (File conventions)
6. GAP-IMPL-009: Multi-output (Per-output tracking within one sink)
7. GAP-STATE-001: State machine (TUI flow)

**Refinement session artifacts:** See `specs/meta/sessions/parser_schema_contract_rfc_refined/`

---

## Feedback Instructions

This RFC is now implementation-ready. Remaining feedback welcome on:

1. **Implementation details**: See session artifacts for full code proposals
2. **Remaining MEDIUM gaps**: PreviewResult Arrow IPC, SchemaIntent storage, TUI quarantine view
3. **Remaining LOW gaps**: CLI commands (schema infer, schema generate, parser validate), type inference for Decimal/List/Struct

Please provide structured feedback with specific references to sections.


and this critique: 

• Findings

  - Critical (resolved): Per‑row validation cost is now acknowledged as O(rows) with explicit perf constraints.
  - Critical (mitigated): Lineage protocol added (`__cf_row_id`) plus FIX provenance; batch index used when lineage unavailable.
  - High (resolved): AST‑normalized hashing was heavy and incomplete; scope identity is now explicit parser_id + version + output_name, with file hash as advisory metadata.
    parser_schema_contract_rfc_refined/source.md
  - High: Quarantine as default success path may conflict with compliance expectations (some orgs require “fail on any violation”). The spec should require explicit per‑contract
    policy for quarantine vs hard‑fail in production. specs/meta/sessions/parser_schema_contract_rfc_refined/source.md
  - Medium (resolved): Deep nested validation deferred to v1.1; v1 uses shallow validation with limits.
  - Medium (mitigated): Timezone parsing now requires a pinned tzdb dependency and benchmarking.
  - Medium (resolved): Multi‑output status semantics clarified with PartialSuccess rules.

  PMF Alignment

  - Strong alignment on: explicit schema intent, Decimal + TimestampTz types, non‑executing metadata extraction, and manual approval. These fit regulated, local‑first buyers.
  - Potential misalignment if quarantine is default without explicit contract policy, or if performance cost makes large‑file workflows feel slow/fragile.

  Suggestions

  - Treat parser version + output name + topics as the primary scope_id; make AST hash optional metadata or a warning signal. This reduces complexity and puts responsibility where
    regulated teams expect it: explicit versioning.
  - Make quarantine opt‑in at contract level with a clear “strict fail” mode for regulated production; default to strict in prod, lenient in dev/preview.
  - Add explicit lineage requirement if you keep _source_row: either parser must emit a source row column, or quarantine uses “row_index_in_output” only.
  - Add explicit performance budgets (e.g., max rows/sec validation) and/or provide a “validate schema only” mode for preview to avoid full per‑row costs in dev.
  - Document Rust dependencies for timezone handling and Decimal validation to avoid underestimating implementation risk.

Do your own analysis and find issues/gaps/improvements

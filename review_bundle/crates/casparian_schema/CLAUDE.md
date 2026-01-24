# Claude Code Instructions for casparian_schema

## Quick Reference

```bash
cargo test -p casparian_schema              # All tests
cargo test -p casparian_schema --test e2e_contracts  # E2E tests
```

---

## Overview

`casparian_schema` implements the **Schema Contract System** - the core guarantee that user-approved schemas are enforced without silent fallbacks.

### The Philosophy

**Schema = Intent, then Contract.**

1. **Discovery**: Parser analyzes files, proposes schema
2. **Review**: User sees schema, can adjust columns/types
3. **Approval**: User clicks "Approve" - schema becomes CONTRACT
4. **Enforcement**: Parser must conform. Violations are FAILURES.
5. **Amendment**: Controlled evolution when data format changes

Once approved, there are NO silent fallbacks. No guessing. No coercion. If the parser outputs data that doesn't match the contract, it FAILS.

---

## Key Types

### SchemaContract

The binding agreement between user intent and parser output:

```rust
pub struct SchemaContract {
    pub contract_id: ContractId,
    pub scope_id: String,           // What this applies to
    pub scope_description: Option<String>,
    pub approved_at: SchemaTimestamp,
    pub approved_by: String,        // Who approved
    pub schemas: Vec<LockedSchema>, // The locked definitions
    pub version: u32,               // Incremented on re-approval
}
```

### LockedSchema

Immutable definition of expected data structure:

```rust
pub struct LockedSchema {
    pub schema_id: SchemaId,
    pub name: String,               // e.g., "transactions"
    pub columns: Vec<LockedColumn>,
    pub source_pattern: Option<String>,  // e.g., "*.csv"
    pub content_hash: String,       // SHA-256 for comparison
}
```

### LockedColumn

Column definition within a schema:

```rust
pub struct LockedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub format: Option<String>,     // e.g., "%Y-%m-%d" for dates
    pub description: Option<String>,
}
```

### DataType

Supported data types (maps to Arrow/Parquet):

```rust
pub enum DataType {
    String,     // UTF-8
    Int64,      // 64-bit signed integer
    Float64,    // 64-bit floating point
    Boolean,    // true/false
    Date,       // Date (no time)
    Timestamp,  // RFC3339 timestamp with timezone
    Binary,     // Raw bytes
}
```

---

## Schema Lifecycle

### 1. Creation

```rust
use casparian_schema::{LockedSchema, LockedColumn, DataType};

let schema = LockedSchema::new(
    "transactions",
    vec![
        LockedColumn::required("id", DataType::Int64),
        LockedColumn::required("amount", DataType::Float64),
        LockedColumn::optional("description", DataType::String),
        LockedColumn::required("date", DataType::Date)
            .with_format("%Y-%m-%d"),
    ],
);
```

### 2. Contract Approval

```rust
use casparian_schema::SchemaContract;

let contract = SchemaContract::new("parser_123", schema, "user@example.com")
    .with_description("Financial transaction data from CRM");
```

### 3. Validation

```rust
// Validate column names match
schema.validate_columns(&["id", "amount", "description", "date"])?;

// Validate data types
if !DataType::Int64.validate_string("123") {
    return Err(SchemaViolation::type_mismatch(0, DataType::Int64, "abc"));
}
```

### 4. Storage

```rust
use casparian_schema::SchemaStorage;

let storage = SchemaStorage::open("/path/to/db.sqlite").await?;
storage.save_contract(&contract).await?;

// Retrieve later
let contract = storage.get_contract(&contract.contract_id).await?;
let contracts = storage.get_contract_history("parser_123").await?;
```

---

## Schema Violations

Violations are hard failures, not warnings:

```rust
pub struct SchemaViolation {
    pub file_path: Option<String>,
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub expected: String,
    pub got: String,
    pub violation_type: ViolationType,
}

pub enum ViolationType {
    TypeMismatch,        // Value doesn't match type
    NullNotAllowed,      // Null in non-nullable column
    FormatMismatch,      // Wrong format (e.g., date)
    ColumnNameMismatch,  // Column name doesn't match
    ColumnCountMismatch, // Wrong number of columns
    SchemaNotFound,      // No schema for scope
}
```

### Creating Violations

```rust
// Type mismatch
let v = SchemaViolation::type_mismatch(2, DataType::Int64, "abc")
    .with_file("data.csv")
    .with_row(42);

// Null not allowed
let v = SchemaViolation::null_not_allowed(3, "amount");

// Format mismatch
let v = SchemaViolation::format_mismatch(4, "%Y-%m-%d", "01-15-2024");
```

---

## Schema Amendments

When data format legitimately changes, use amendments:

### Amendment Proposal

```rust
use casparian_schema::{SchemaAmendmentProposal, SchemaChange, AmendmentReason};

let proposal = SchemaAmendmentProposal {
    original_contract_id: contract.contract_id,
    proposed_changes: vec![
        SchemaChange::AddColumn {
            schema_name: "transactions".to_string(),
            column: LockedColumn::optional("category", DataType::String),
            position: None,  // Append at end
        },
        SchemaChange::ChangeType {
            schema_name: "transactions".to_string(),
            column_name: "amount".to_string(),
            old_type: DataType::Float64,
            new_type: DataType::String,
        },
    ],
    reason: AmendmentReason::DataFormatChange,
    proposed_by: "user@example.com".to_string(),
};
```

### Amendment Actions

```rust
pub enum AmendmentAction {
    ApproveAsProposed,           // Accept all changes
    ApproveWithModifications(Vec<SchemaChange>),  // Modified changes
    Reject { reason: String },   // Reject with explanation
    CreateSeparateSchema,        // New schema instead of amendment
}
```

### Approval Workflow

```rust
use casparian_schema::approval;

// Review proposed changes
let result = approval::approve_amendment(
    &storage,
    &proposal,
    AmendmentAction::ApproveAsProposed,
    "reviewer@example.com",
)?;

match result.status {
    AmendmentStatus::Approved { new_contract_id, version } => {
        println!("Amendment approved! New contract: {}, v{}", new_contract_id, version);
    }
    AmendmentStatus::Rejected { reason } => {
        println!("Amendment rejected: {}", reason);
    }
}
```

---

## Storage

### SchemaStorage

SQLite-backed persistence for contracts:

```rust
pub struct SchemaStorage {
    conn: DbConnection,
}

impl SchemaStorage {
    /// Create with connection
    pub async fn new(conn: DbConnection) -> Result<Self, StorageError>;

    /// Create with file path
    pub async fn open(path: &str) -> Result<Self, StorageError>;

    /// Create in-memory (for testing)
    pub async fn in_memory() -> Result<Self, StorageError>;

    /// Save a contract
    pub async fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError>;

    /// Get by contract ID
    pub async fn get_contract(
        &self,
        id: &ContractId,
    ) -> Result<Option<SchemaContract>, StorageError>;

    /// Get all contracts for a scope
    pub async fn get_contract_history(
        &self,
        scope_id: &str,
    ) -> Result<Vec<SchemaContract>, StorageError>;

    /// Get latest version for scope
    pub async fn get_contract_for_scope(
        &self,
        scope_id: &str,
    ) -> Result<Option<SchemaContract>, StorageError>;

    /// List all contracts
    pub async fn list_contracts(&self) -> Result<Vec<SchemaContract>, StorageError>;

    /// Delete a contract
    pub async fn delete_contract(&self, id: &ContractId) -> Result<bool, StorageError>;
}
```

---

## Common Tasks

### Add a New Data Type

1. Add variant to `DataType` enum:
```rust
pub enum DataType {
    // ... existing
    Decimal,  // Fixed-point decimal
}
```

2. Implement validation:
```rust
impl DataType {
    pub fn validate_string(&self, value: &str) -> bool {
        match self {
            DataType::Decimal => {
                // Parse as decimal with precision
                value.parse::<rust_decimal::Decimal>().is_ok()
            }
            // ...
        }
    }

    pub fn arrow_type_name(&self) -> &'static str {
        match self {
            DataType::Decimal => "Decimal128(38, 10)",
            // ...
        }
    }
}
```

3. Add E2E test in `tests/e2e_contracts.rs`

### Validate Data Against Schema

```rust
fn validate_row(schema: &LockedSchema, values: &[&str]) -> Result<(), SchemaViolation> {
    // Check column count
    if values.len() != schema.columns.len() {
        return Err(SchemaViolation {
            expected: format!("{} columns", schema.columns.len()),
            got: format!("{} columns", values.len()),
            violation_type: ViolationType::ColumnCountMismatch,
            ..Default::default()
        });
    }

    // Validate each value
    for (i, (col, value)) in schema.columns.iter().zip(values).enumerate() {
        // Check nullability
        if value.is_empty() && !col.nullable {
            return Err(SchemaViolation::null_not_allowed(i, &col.name));
        }

        // Check type
        if !value.is_empty() && !col.data_type.validate_string(value) {
            return Err(SchemaViolation::type_mismatch(i, col.data_type, value));
        }
    }

    Ok(())
}
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_create_schema() {
    let schema = LockedSchema::new(
        "test",
        vec![LockedColumn::required("id", DataType::Int64)],
    );

    assert_eq!(schema.name, "test");
    assert!(!schema.content_hash.is_empty());
}
```

### E2E Tests

```rust
#[test]
fn test_full_schema_lifecycle() {
    let storage = SchemaStorage::in_memory().unwrap();

    // Create schema
    let schema = LockedSchema::new("data", vec![...]);

    // Create contract (approval)
    let contract = SchemaContract::new("scope_1", schema, "user");
    storage.save_contract(&contract).unwrap();

    // Simulate violation
    let violation = SchemaViolation::type_mismatch(0, DataType::Int64, "abc");
    assert_eq!(violation.violation_type, ViolationType::TypeMismatch);

    // Propose amendment
    let proposal = SchemaAmendmentProposal { ... };
    let result = approval::approve_amendment(&storage, &proposal, ...);

    // Verify new version
    let latest = storage.get_latest("scope_1").unwrap().unwrap();
    assert_eq!(latest.version, 2);
}
```

---

## File Structure

```
casparian_schema/
├── CLAUDE.md           # This file
├── Cargo.toml
├── src/
│   ├── lib.rs          # Crate root, exports
│   ├── contract.rs     # LockedSchema, SchemaContract, DataType
│   ├── approval.rs     # Approval workflow
│   ├── amendment.rs    # Schema evolution
│   └── storage.rs      # SQLite persistence
└── tests/
    └── e2e_contracts.rs  # E2E tests (24 tests)
```

---

## Key Principles

1. **Schemas are immutable after approval** - Use amendments for changes
2. **Violations are failures** - No silent coercion or fallbacks
3. **Content hashes for comparison** - Quick schema equality check
4. **Version tracking** - Every amendment increments version
5. **Audit trail** - Track who approved/amended and when

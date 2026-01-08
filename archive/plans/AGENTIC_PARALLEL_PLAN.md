# Casparian Flow: Agentic MCP Server - Parallel Execution Plan

**Goal:** Build the MCP server and supporting infrastructure for Claude Code integration.

**Philosophy:** Foundation workers create infrastructure. Subsequent workers implement features. Final worker integrates everything.

---

## CURRENT STATE ANALYSIS

### What EXISTS (Ready to Build On)

| Component | Location | Status | Notes |
|-----------|----------|--------|-------|
| CLI structure | `crates/casparian/src/cli/` | Working | scan, preview, tag, files, jobs |
| Preview/TypeTracker | `cli/preview.rs` | Basic | Single-file type inference |
| Analyzer | `crates/casparian_worker/src/analyzer.rs` | Working | Heuristic format detection |
| Scout | `crates/casparian_scout/` | Working | File discovery + tagging |
| Sentinel | `crates/casparian_sentinel/` | Working | Job orchestration |
| Worker/Bridge | `crates/casparian_worker/` | Working | Isolated Python execution |
| Parser Lab (UI) | `ui/src-tauri/src/scout.rs` | Working | Single-file validation |
| Database | `~/.casparian_flow/casparian_flow.sqlite3` | Working | Single DB pattern |

### What DOESN'T EXIST (Must Build)

| Component | Priority | Complexity | Notes |
|-----------|----------|------------|-------|
| MCP Server | P0 | High | Complete gap - no implementation |
| Constraint Solver | P0 | High | Only basic TypeTracker exists |
| Schema Contracts | P0 | Medium | Schema = Intent system |
| Backtest Loop | P0 | High | Multi-file validation |
| High-Failure Table | P1 | Medium | Fail-fast optimization |
| Schema Amendment | P1 | Medium | Mid-flight schema changes |
| Streaming Inference | P1 | Medium | Early termination |

---

## FILE STRUCTURE (Target)

```
crates/
├── casparian/                    # Existing - CLI binary
│   └── src/
│       ├── main.rs               # W7 ADDS: mcp-server command
│       └── cli/
│           └── mcp.rs            # W7 CREATES: MCP server launcher
│
├── casparian_mcp/                # W1 CREATES: New crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # W1: MCP server core
│       ├── server.rs             # W1: Server lifecycle
│       ├── protocol.rs           # W1: MCP protocol handling
│       ├── tools/
│       │   ├── mod.rs            # W6: Tool registry
│       │   ├── discovery.rs      # W6: quick_scan, apply_scope
│       │   ├── schema.rs         # W6: discover_schemas, approve_schema
│       │   ├── backtest.rs       # W6: run_backtest, fix_parser
│       │   └── execution.rs      # W6: execute_pipeline, query
│       └── types.rs              # W1: Shared types for tools
│
├── casparian_worker/             # Existing
│   └── src/
│       ├── lib.rs                # W2 MODIFIES: export type_inference
│       ├── type_inference/       # W2 CREATES: New module
│       │   ├── mod.rs            # W2: Module exports
│       │   ├── solver.rs         # W2: ConstraintSolver
│       │   ├── constraints.rs    # W2: Constraint types
│       │   ├── streaming.rs      # W2: Streaming inference
│       │   └── date_formats.rs   # W2: Date format detection
│       └── analyzer.rs           # Existing - format detection
│
├── casparian_schema/             # W3 CREATES: New crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # W3: Schema contract system
│       ├── contract.rs           # W3: SchemaContract, LockedSchema
│       ├── approval.rs           # W4: Approval workflow
│       ├── amendment.rs          # W4: Amendment workflow
│       └── storage.rs            # W3: SQLite persistence
│
├── casparian_backtest/           # W5 CREATES: New crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # W5: Backtest engine
│       ├── loop.rs               # W5: Iteration loop
│       ├── high_failure.rs       # W5: High-failure table
│       ├── failfast.rs           # W5: Fail-fast logic
│       └── metrics.rs            # W5: Iteration metrics
│
└── casparian_scout/              # Existing
    └── src/
        └── db.rs                 # W3 MODIFIES: Add schema tables
```

---

## FILE OWNERSHIP MATRIX

| File | W1 | W2 | W3 | W4 | W5 | W6 | W7 | Notes |
|------|----|----|----|----|----|----|-----|-------|
| **casparian_mcp/** | | | | | | | | |
| Cargo.toml | PRIMARY | - | - | - | - | - | - | Create crate |
| src/lib.rs | PRIMARY | - | - | - | - | - | - | Server core |
| src/server.rs | PRIMARY | - | - | - | - | - | - | Lifecycle |
| src/protocol.rs | PRIMARY | - | - | - | - | - | - | MCP protocol |
| src/types.rs | PRIMARY | - | - | - | - | MODIFY | - | Add tool types |
| src/tools/mod.rs | - | - | - | - | - | PRIMARY | - | Registry |
| src/tools/discovery.rs | - | - | - | - | - | PRIMARY | - | Scan/scope |
| src/tools/schema.rs | - | - | - | - | - | PRIMARY | - | Schema tools |
| src/tools/backtest.rs | - | - | - | - | - | PRIMARY | - | Backtest tools |
| src/tools/execution.rs | - | - | - | - | - | PRIMARY | - | Execute tools |
| **casparian_worker/** | | | | | | | | |
| src/lib.rs | - | MODIFY | - | - | - | - | - | Export module |
| src/type_inference/mod.rs | - | PRIMARY | - | - | - | - | - | Module |
| src/type_inference/solver.rs | - | PRIMARY | - | - | - | - | - | Solver |
| src/type_inference/constraints.rs | - | PRIMARY | - | - | - | - | - | Constraints |
| src/type_inference/streaming.rs | - | PRIMARY | - | - | - | - | - | Streaming |
| src/type_inference/date_formats.rs | - | PRIMARY | - | - | - | - | - | Dates |
| **casparian_schema/** | | | | | | | | |
| Cargo.toml | - | - | PRIMARY | - | - | - | - | Create crate |
| src/lib.rs | - | - | PRIMARY | - | - | - | - | Core |
| src/contract.rs | - | - | PRIMARY | - | - | - | - | Contracts |
| src/approval.rs | - | - | - | PRIMARY | - | - | - | Approval |
| src/amendment.rs | - | - | - | PRIMARY | - | - | - | Amendment |
| src/storage.rs | - | - | PRIMARY | - | - | - | - | SQLite |
| **casparian_backtest/** | | | | | | | | |
| Cargo.toml | - | - | - | - | PRIMARY | - | - | Create crate |
| src/lib.rs | - | - | - | - | PRIMARY | - | - | Core |
| src/loop.rs | - | - | - | - | PRIMARY | - | - | Loop |
| src/high_failure.rs | - | - | - | - | PRIMARY | - | - | Table |
| src/failfast.rs | - | - | - | - | PRIMARY | - | - | Logic |
| src/metrics.rs | - | - | - | - | PRIMARY | - | - | Metrics |
| **casparian_scout/** | | | | | | | | |
| src/db.rs | - | - | MODIFY | - | MODIFY | - | - | Add tables |
| **casparian (CLI)** | | | | | | | | |
| src/main.rs | - | - | - | - | - | - | MODIFY | Add command |
| src/cli/mcp.rs | - | - | - | - | - | - | PRIMARY | Launcher |

**Zero file conflicts by design.** Each worker owns specific files.

---

## PHASE 1: FOUNDATION (Parallel)

### W1: MCP Server Infrastructure

**Branch:** `feat/mcp-server-core`
**Creates:** `crates/casparian_mcp/`

#### Deliverables

1. **Cargo.toml:**
```toml
[package]
name = "casparian_mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
tracing = "0.1"
thiserror = "1"
async-trait = "0.1"

# MCP protocol
# Note: May need to implement from spec if no good crate exists
```

2. **src/lib.rs:**
```rust
//! Casparian MCP Server
//!
//! Model Context Protocol server for Claude Code integration.

pub mod server;
pub mod protocol;
pub mod types;
pub mod tools;

pub use server::McpServer;
pub use types::*;
```

3. **src/server.rs:**
```rust
//! MCP Server lifecycle management

use crate::protocol::McpProtocol;
use crate::tools::ToolRegistry;

pub struct McpServer {
    protocol: McpProtocol,
    tools: ToolRegistry,
}

impl McpServer {
    pub async fn new() -> Result<Self, McpError> { ... }
    pub async fn run(&mut self) -> Result<(), McpError> { ... }
    pub fn register_tool<T: Tool>(&mut self, tool: T) { ... }
}
```

4. **src/protocol.rs:**
```rust
//! MCP Protocol handling (JSON-RPC over stdio)

pub struct McpProtocol { ... }

impl McpProtocol {
    pub async fn read_request(&mut self) -> Result<Request, ProtocolError> { ... }
    pub async fn write_response(&mut self, response: Response) -> Result<(), ProtocolError> { ... }
}
```

5. **src/types.rs:**
```rust
//! Shared types for MCP tools

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// These will be populated by W6, but define the structure now

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestId(pub Uuid);

// Tool trait for registration
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError>;
}
```

6. **src/tools/mod.rs (stub):**
```rust
//! Tool registry (W6 implements tools)

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, tool: Box<dyn Tool>) { ... }
    pub fn get(&self, name: &str) -> Option<&dyn Tool> { ... }
}

// W6 will add:
// pub mod discovery;
// pub mod schema;
// pub mod backtest;
// pub mod execution;
```

#### Done When
- `cargo check -p casparian_mcp` passes
- Server can start and accept JSON-RPC connections
- Tool registration framework works
- Protocol serialization/deserialization works

---

### W2: Constraint-Based Type Inference Engine

**Branch:** `feat/type-inference`
**Creates:** `crates/casparian_worker/src/type_inference/`

#### Deliverables

1. **src/type_inference/mod.rs:**
```rust
//! Constraint-based type inference engine
//!
//! Uses ALL values to eliminate possibilities (not sampling).
//! Each value adds constraints. Intersection = proven type.

pub mod solver;
pub mod constraints;
pub mod streaming;
pub mod date_formats;

pub use solver::ConstraintSolver;
pub use constraints::*;
pub use streaming::*;
```

2. **src/type_inference/constraints.rs:**
```rust
//! Constraint types for type inference

#[derive(Debug, Clone)]
pub enum Constraint {
    CannotBe(DataType),
    MustBe(DataType),
    DateFormatEliminated(String),
    PreferType(DataType),
}

#[derive(Debug, Clone)]
pub struct EliminationEvidence {
    pub eliminated: String,
    pub because_of_value: String,
    pub file_path: String,
    pub row: usize,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct Contradiction {
    pub column: String,
    pub evidence: Vec<ContradictionEvidence>,
    pub distribution: ConflictDistribution,
}

#[derive(Debug, Clone)]
pub enum TypeInferenceResult {
    Resolved {
        data_type: DataType,
        format: Option<String>,
        nullable: bool,
        null_sentinels: Vec<String>,
        evidence: Vec<EliminationEvidence>,
    },
    Ambiguous {
        candidates: HashSet<DataType>,
        format_candidates: HashMap<DataType, HashSet<String>>,
        values_analyzed: usize,
        suggestion: String,
    },
    Contradiction {
        contradictions: Vec<Contradiction>,
        suggestion: String,
    },
    NoValidType {
        reason: String,
    },
}
```

3. **src/type_inference/solver.rs:**
```rust
//! ConstraintSolver - the core inference engine

pub struct ConstraintSolver {
    column_name: String,
    possible_types: HashSet<DataType>,
    format_candidates: HashMap<DataType, HashSet<String>>,
    elimination_evidence: Vec<EliminationEvidence>,
    null_sentinels: HashSet<String>,
    contradictions: Vec<Contradiction>,
    values_seen: usize,
}

impl ConstraintSolver {
    pub fn new(column_name: &str) -> Self { ... }

    pub fn add_value(&mut self, value: &str, file_path: &str, row: usize) {
        // Skip null sentinels
        if self.is_null_sentinel(value) {
            self.null_sentinels.insert(value.to_string());
            return;
        }

        // Derive and apply constraints
        self.apply_numeric_constraints(value, file_path, row);
        self.apply_date_constraints(value, file_path, row);
        self.apply_string_constraints(value, file_path, row);
    }

    fn apply_date_constraints(&mut self, value: &str, file: &str, row: usize) {
        // For each date format candidate
        // Try to parse, check domain constraints (month <= 12, day <= 31)
        // Eliminate formats that don't satisfy constraints
    }

    pub fn is_resolved(&self) -> bool {
        self.possible_types.len() == 1 &&
        self.format_candidates.values().all(|f| f.len() <= 1) &&
        self.contradictions.is_empty()
    }

    pub fn get_result(&self) -> TypeInferenceResult { ... }
}
```

4. **src/type_inference/date_formats.rs:**
```rust
//! Date format detection via constraint elimination

pub const DATE_FORMATS: &[&str] = &[
    "YYYY-MM-DD", "MM/DD/YYYY", "DD/MM/YYYY", "YYYY/MM/DD",
    "MM-DD-YYYY", "DD-MM-YYYY", "YY/MM/DD", "MM/DD/YY", "DD/MM/YY",
];

pub struct ParsedDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

pub fn try_parse_date(value: &str, format: &str) -> Option<ParsedDate> { ... }

pub fn days_in_month(month: u8, year: u16) -> u8 { ... }
```

5. **src/type_inference/streaming.rs:**
```rust
//! Streaming type inference with early termination

pub async fn infer_types_streaming<S>(
    files: S,
    columns: &[String],
) -> HashMap<String, TypeInferenceResult>
where
    S: Stream<Item = FileInfo>,
{
    let mut solvers = create_solvers(columns);
    let mut files_processed = 0;

    pin_mut!(files);
    while let Some(file) = files.next().await {
        files_processed += 1;

        for row in file.rows() {
            for (col, value) in row {
                if let Some(solver) = solvers.get_mut(col) {
                    solver.add_value(value, &file.path, row.index);
                }
            }
        }

        // EARLY TERMINATION: All columns resolved
        if solvers.values().all(|s| s.is_resolved()) {
            tracing::info!("All columns resolved after {} files", files_processed);
            break;
        }
    }

    solvers.into_iter()
        .map(|(name, solver)| (name, solver.get_result()))
        .collect()
}
```

6. **Modify src/lib.rs:**
```rust
// Add export
pub mod type_inference;
```

#### Done When
- `cargo check -p casparian_worker` passes
- ConstraintSolver correctly infers types from values
- Date format elimination works (day > 12 eliminates MM position)
- Early termination stops when all columns resolved
- Unit tests pass for type inference

---

### W3: Schema Contract System (Foundation)

**Branch:** `feat/schema-contracts`
**Creates:** `crates/casparian_schema/`

#### Deliverables

1. **Cargo.toml:**
```toml
[package]
name = "casparian_schema"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.31", features = ["bundled"] }
thiserror = "1"
```

2. **src/lib.rs:**
```rust
//! Schema Contract System
//!
//! Schema = Intent. Once approved, schema is a CONTRACT.
//! Parser must conform. Violations are failures, not fallbacks.

pub mod contract;
pub mod storage;

// W4 adds:
// pub mod approval;
// pub mod amendment;

pub use contract::*;
```

3. **src/contract.rs:**
```rust
//! Schema contract types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaContract {
    pub contract_id: Uuid,
    pub scope_id: Uuid,
    pub approved_at: DateTime<Utc>,
    pub schemas: Vec<LockedSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSchema {
    pub schema_id: Uuid,
    pub name: String,
    pub columns: Vec<LockedColumn>,
    pub source_pattern: String,
    pub estimated_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub format: Option<String>,
    // Contract: parser output MUST match exactly
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    String,
    Int64,
    Float64,
    Boolean,
    Date,
    Timestamp,
}

// Violation types (for backtest)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaViolation {
    pub file_path: String,
    pub row: usize,
    pub column: String,
    pub expected: DataType,
    pub got: String,
    pub violation_type: ViolationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    TypeMismatch,
    NullNotAllowed,
    FormatMismatch,
}
```

4. **src/storage.rs:**
```rust
//! SQLite persistence for schema contracts

pub struct SchemaStorage {
    conn: Connection,
}

impl SchemaStorage {
    pub fn new(conn: Connection) -> Self { ... }

    pub fn init_tables(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS schema_contracts (
                id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                approved_at TEXT NOT NULL,
                schemas_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schema_discovery_results (
                id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                discovered_at TEXT NOT NULL,
                variants_json TEXT NOT NULL,
                questions_json TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_schema_contracts_scope
                ON schema_contracts(scope_id);
        "#)?;
        Ok(())
    }

    pub fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError> { ... }
    pub fn get_contract(&self, id: &Uuid) -> Result<Option<SchemaContract>, StorageError> { ... }
    pub fn get_contract_for_scope(&self, scope_id: &Uuid) -> Result<Option<SchemaContract>, StorageError> { ... }
}
```

5. **Modify casparian_scout/src/db.rs (add tables):**
```rust
// Add to init_schema():

// Schema contracts (W3)
CREATE TABLE IF NOT EXISTS schema_contracts (
    id TEXT PRIMARY KEY,
    scope_id TEXT NOT NULL,
    approved_at TEXT NOT NULL,
    schemas_json TEXT NOT NULL
);

// High-failure files (W5)
CREATE TABLE IF NOT EXISTS high_failure_files (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    scope_id TEXT NOT NULL,
    failure_count INTEGER DEFAULT 0,
    consecutive_failures INTEGER DEFAULT 0,
    first_failure_at TEXT,
    last_failure_at TEXT,
    last_tested_at TEXT,
    failure_history_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_high_failure_scope
    ON high_failure_files(scope_id);
CREATE INDEX IF NOT EXISTS idx_high_failure_consecutive
    ON high_failure_files(consecutive_failures DESC);
```

#### Done When
- `cargo check -p casparian_schema` passes
- Schema contract types compile
- SQLite storage works
- Can save and retrieve contracts

---

## PHASE 2: CORE SYSTEMS (Parallel, after Phase 1)

### W4: Schema Approval & Amendment

**Branch:** `feat/schema-approval`
**Depends on:** W3
**Modifies:** `crates/casparian_schema/`

#### Deliverables

1. **src/approval.rs:**
```rust
//! Schema approval workflow

pub struct SchemaApprovalRequest {
    pub discovery_id: Uuid,
    pub approved_schemas: Vec<ApprovedSchemaVariant>,
    pub question_answers: Vec<QuestionAnswer>,
    pub excluded_files: Vec<String>,
}

pub struct ApprovedSchemaVariant {
    pub variant_id: Uuid,
    pub name: String,
    pub columns: Vec<ApprovedColumn>,
    pub output_table_name: String,
}

pub struct ApprovedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub format: Option<String>,
    pub rename_to: Option<String>,
    pub default_value: Option<String>,
}

pub fn approve_schema(
    storage: &SchemaStorage,
    request: SchemaApprovalRequest,
) -> Result<SchemaContract, ApprovalError> {
    // Validate all variants exist in discovery
    // Lock columns into contract
    // Save contract to storage
    // Return locked contract
}
```

2. **src/amendment.rs:**
```rust
//! Schema amendment workflow

pub struct SchemaAmendmentProposal {
    pub amendment_id: Uuid,
    pub contract_id: Uuid,
    pub reason: AmendmentReason,
    pub current_schema: LockedSchema,
    pub proposed_schema: LockedSchema,
    pub changes: Vec<SchemaChange>,
    pub affected_files: usize,
}

pub enum AmendmentReason {
    NewSchemaVariant {
        variant_description: String,
        file_count: usize,
        file_pattern: Option<String>,
    },
    TypeMismatch {
        column: String,
        expected_type: DataType,
        actual_values: Vec<String>,
    },
    NullabilityChange {
        column: String,
        null_percentage: f32,
    },
}

pub enum SchemaChange {
    AddColumn { column: LockedColumn },
    RemoveColumn { column_name: String },
    ChangeType { column_name: String, from: DataType, to: DataType },
    ChangeNullability { column_name: String, nullable: bool },
    AddDefaultValue { column_name: String, default: String },
}

pub enum AmendmentAction {
    ApproveAsProposed,
    ApproveWithModifications { changes: Vec<SchemaChange> },
    Reject { reason: String },
    CreateSeparateSchema { name: String },
    ExcludeAffectedFiles,
}

pub fn propose_amendment(
    storage: &SchemaStorage,
    contract: &SchemaContract,
    reason: AmendmentReason,
) -> Result<SchemaAmendmentProposal, AmendmentError> { ... }

pub fn approve_amendment(
    storage: &SchemaStorage,
    proposal: &SchemaAmendmentProposal,
    action: AmendmentAction,
) -> Result<SchemaContract, AmendmentError> { ... }
```

#### Done When
- Schema approval creates locked contracts
- Amendments can be proposed and applied
- Contract versioning works

---

### W5: Backtest Engine

**Branch:** `feat/backtest-engine`
**Depends on:** W3 (schema contracts)
**Creates:** `crates/casparian_backtest/`

#### Deliverables

1. **Cargo.toml:**
```toml
[package]
name = "casparian_backtest"
version = "0.1.0"
edition = "2021"

[dependencies]
casparian_schema = { path = "../casparian_schema" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.31", features = ["bundled"] }
tracing = "0.1"
thiserror = "1"
```

2. **src/lib.rs:**
```rust
//! Backtest Engine
//!
//! Multi-file validation with fail-fast optimization.

pub mod loop_;
pub mod high_failure;
pub mod failfast;
pub mod metrics;

pub use loop_::*;
pub use high_failure::*;
```

3. **src/high_failure.rs:**
```rust
//! High-failure file table for fail-fast

pub struct HighFailureFile {
    pub file_id: Uuid,
    pub file_path: String,
    pub scope_id: Uuid,
    pub failure_count: usize,
    pub consecutive_failures: usize,
    pub first_failure_at: DateTime<Utc>,
    pub last_failure_at: DateTime<Utc>,
    pub last_tested_at: DateTime<Utc>,
    pub failure_history: Vec<FailureHistoryEntry>,
}

pub struct FailureHistoryEntry {
    pub iteration: usize,
    pub parser_version: usize,
    pub failure_category: FailureCategory,
    pub error_message: String,
    pub resolved: bool,
    pub resolved_by: Option<String>,
}

pub struct HighFailureTable {
    conn: Connection,
}

impl HighFailureTable {
    pub fn new(conn: Connection) -> Self { ... }

    pub fn record_failure(&self, file_path: &str, scope_id: &Uuid, entry: FailureHistoryEntry) -> Result<(), Error> { ... }
    pub fn record_success(&self, file_path: &str) -> Result<(), Error> { ... }

    pub fn get_active(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, Error> { ... }
    pub fn get_resolved(&self, scope_id: &Uuid) -> Result<Vec<HighFailureFile>, Error> { ... }

    pub fn get_backtest_order(&self, all_files: &[FileInfo], scope_id: &Uuid) -> Vec<FileInfo> {
        // 1. High-failure files (sorted by consecutive failures desc)
        // 2. Resolved files (regression check)
        // 3. Never-tested files
        // 4. Previously-passing files
    }
}
```

4. **src/failfast.rs:**
```rust
//! Fail-fast backtest logic

pub struct FailFastConfig {
    pub high_failure_threshold: f32,  // e.g., 0.8
    pub early_stop_enabled: bool,
}

pub fn backtest_with_failfast(
    parser: &Parser,
    files: Vec<FileInfo>,
    high_failure_table: &HighFailureTable,
    config: &FailFastConfig,
) -> BacktestResult {
    let high_failure_files = high_failure_table.get_active(&scope_id)?;
    let high_failure_count = high_failure_files.len();

    let mut high_failure_passed = 0;
    let mut high_failure_failed = 0;

    // Test high-failure files first
    for file in files.iter().take(high_failure_count) {
        let result = test_file(parser, file);
        match result {
            Ok(_) => high_failure_passed += 1,
            Err(e) => {
                high_failure_failed += 1;
                // Update high-failure table
            }
        }
    }

    let high_failure_pass_rate = high_failure_passed as f32 / high_failure_count as f32;

    // FAIL-FAST: If high-failure files still failing, stop early
    if high_failure_pass_rate < config.high_failure_threshold {
        return BacktestResult::EarlyStop {
            reason: format!(
                "High-failure files still failing ({:.1}% pass rate)",
                high_failure_pass_rate * 100.0
            ),
            high_failure_pass_rate,
            tested: high_failure_count,
        };
    }

    // Continue with remaining files...
}
```

5. **src/loop_.rs:**
```rust
//! Backtest iteration loop

pub struct IterationConfig {
    pub max_iterations: usize,
    pub max_duration_secs: u64,
    pub pass_rate_threshold: f32,
    pub improvement_threshold: f32,
    pub plateau_window: usize,
}

pub struct BacktestIteration {
    pub iteration: usize,
    pub parser_version: usize,
    pub pass_rate: f32,
    pub files_passed: usize,
    pub files_failed: usize,
    pub failure_categories: Vec<FailureCategoryGroup>,
    pub duration_ms: u64,
}

pub enum TerminationReason {
    PassRateAchieved,
    MaxIterations,
    Plateau { no_improvement_for: usize },
    Timeout,
    UserStopped,
}

pub fn run_backtest_loop(
    parser: &mut Parser,
    files: Vec<FileInfo>,
    contract: &SchemaContract,
    high_failure_table: &HighFailureTable,
    config: &IterationConfig,
) -> BacktestLoopResult {
    let mut history: Vec<BacktestIteration> = vec![];

    loop {
        let iteration = history.len() + 1;

        // Run backtest with fail-fast
        let result = backtest_with_failfast(parser, files.clone(), high_failure_table, &failfast_config);

        // Record iteration
        history.push(BacktestIteration { ... });

        // Check termination conditions
        if let Some(reason) = should_terminate(&history, config) {
            return BacktestLoopResult {
                final_pass_rate: result.pass_rate,
                iterations: history,
                termination_reason: reason,
            };
        }

        // Auto-fix parser based on failures
        let fixes = analyze_failures(&result.failures);
        apply_fixes(parser, &fixes);
    }
}

fn should_terminate(history: &[BacktestIteration], config: &IterationConfig) -> Option<TerminationReason> {
    let latest = history.last()?;

    // Success: threshold reached
    if latest.pass_rate >= config.pass_rate_threshold {
        return Some(TerminationReason::PassRateAchieved);
    }

    // Failure: max iterations
    if history.len() >= config.max_iterations {
        return Some(TerminationReason::MaxIterations);
    }

    // Failure: plateau
    if history.len() >= config.plateau_window {
        let recent: Vec<_> = history.iter().rev().take(config.plateau_window).collect();
        let improvement = recent.first().unwrap().pass_rate - recent.last().unwrap().pass_rate;
        if improvement.abs() < config.improvement_threshold {
            return Some(TerminationReason::Plateau {
                no_improvement_for: config.plateau_window,
            });
        }
    }

    None
}
```

6. **src/metrics.rs:**
```rust
//! Iteration metrics and failure analysis

pub struct FailureSummary {
    pub total_failures: usize,
    pub by_category: Vec<FailureCategoryGroup>,
}

pub struct FailureCategoryGroup {
    pub category: FailureCategory,
    pub count: usize,
    pub percentage: f32,
    pub subcategories: Vec<FailureSubcategory>,
}

pub struct FailureSubcategory {
    pub description: String,
    pub count: usize,
    pub sample_files: Vec<String>,
    pub sample_error: String,
    pub column: Option<String>,
    pub file_pattern: Option<String>,
}

pub fn analyze_failures(failures: &[SchemaViolation]) -> FailureSummary {
    // Group by category
    // Cluster by pattern
    // Generate suggestions
}
```

#### Done When
- `cargo check -p casparian_backtest` passes
- High-failure table tracks failures correctly
- Fail-fast stops early when high-failure files still fail
- Iteration loop runs with termination conditions
- Backtest order prioritizes known-bad files

---

## PHASE 3: INTEGRATION (Sequential, after Phase 2)

### W6: MCP Tools Implementation

**Branch:** `feat/mcp-tools`
**Depends on:** W1, W2, W3, W4, W5
**Modifies:** `crates/casparian_mcp/src/tools/`

#### Deliverables

1. **src/tools/mod.rs:**
```rust
//! MCP Tool implementations

pub mod discovery;
pub mod schema;
pub mod backtest;
pub mod execution;

use crate::types::Tool;

pub fn register_all_tools(registry: &mut ToolRegistry) {
    registry.register(Box::new(discovery::QuickScanTool::new()));
    registry.register(Box::new(discovery::ApplyScopeTool::new()));
    registry.register(Box::new(schema::DiscoverSchemasTool::new()));
    registry.register(Box::new(schema::ApproveSchemasTool::new()));
    registry.register(Box::new(schema::ProposeAmendmentTool::new()));
    registry.register(Box::new(backtest::RunBacktestTool::new()));
    registry.register(Box::new(backtest::FixParserTool::new()));
    registry.register(Box::new(execution::ExecutePipelineTool::new()));
    registry.register(Box::new(execution::QueryOutputTool::new()));
}
```

2. **src/tools/discovery.rs:**
```rust
//! Discovery tools: quick_scan, apply_scope

pub struct QuickScanTool { ... }

#[async_trait]
impl Tool for QuickScanTool {
    fn name(&self) -> &str { "quick_scan" }

    fn description(&self) -> &str {
        "Fast metadata scan of a directory (stat only, no content reading)"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory to scan" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path: PathBuf = serde_json::from_value(params["path"].clone())?;

        // Use existing scan logic from CLI
        let result = quick_scan(&path).await?;

        Ok(serde_json::to_value(result)?)
    }
}

pub struct ApplyScopeTool { ... }

// Similar implementation using ScopeRequest/ScopeResult types
```

3. **src/tools/schema.rs:**
```rust
//! Schema tools: discover_schemas, approve_schema, propose_amendment

pub struct DiscoverSchemasTool { ... }

#[async_trait]
impl Tool for DiscoverSchemasTool {
    fn name(&self) -> &str { "discover_schemas" }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let scope_id: Uuid = serde_json::from_value(params["scope_id"].clone())?;
        let sample_size: usize = params["sample_size"].as_u64().unwrap_or(100) as usize;

        // 1. Get files from scope
        // 2. Stratified sample
        // 3. Run constraint-based type inference
        // 4. Cluster into schema variants
        // 5. Generate questions for ambiguous cases

        let result = discover_schemas(scope_id, sample_size).await?;
        Ok(serde_json::to_value(result)?)
    }
}

pub struct ApproveSchemaTool { ... }
pub struct ProposeAmendmentTool { ... }
```

4. **src/tools/backtest.rs:**
```rust
//! Backtest tools: run_backtest, fix_parser

pub struct RunBacktestTool { ... }

#[async_trait]
impl Tool for RunBacktestTool {
    fn name(&self) -> &str { "run_backtest" }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let parser_id: Uuid = serde_json::from_value(params["parser_id"].clone())?;
        let mode: BacktestMode = serde_json::from_value(params["mode"].clone())?;

        // Use backtest engine
        let result = run_backtest(parser_id, mode).await?;
        Ok(serde_json::to_value(result)?)
    }
}

pub struct FixParserTool { ... }
```

5. **src/tools/execution.rs:**
```rust
//! Execution tools: execute_pipeline, query_output

pub struct ExecutePipelineTool { ... }
pub struct QueryOutputTool { ... }
```

#### Done When
- All MCP tools registered and callable
- Tools integrate with backend systems
- End-to-end flow works: scan → discover → approve → backtest → execute

---

### W7: CLI Integration

**Branch:** `feat/mcp-cli`
**Depends on:** W6
**Modifies:** `crates/casparian/src/`

#### Deliverables

1. **Modify src/main.rs:**
```rust
// Add command
Commands::McpServer {
    /// Bind address (default: stdio)
    #[arg(long)]
    addr: Option<String>,
} => cli::mcp::run(cli::mcp::McpArgs { addr }),
```

2. **Create src/cli/mcp.rs:**
```rust
//! MCP Server CLI launcher

use casparian_mcp::McpServer;

pub struct McpArgs {
    pub addr: Option<String>,
}

pub fn run(args: McpArgs) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let mut server = McpServer::new().await?;

        // Register all tools
        casparian_mcp::tools::register_all_tools(&mut server.tools);

        tracing::info!("MCP Server starting...");
        server.run().await
    })
}
```

3. **Update src/cli/mod.rs:**
```rust
pub mod mcp;
```

#### Done When
- `casparian mcp-server` starts the MCP server
- Server can be connected to by Claude Code
- Full workflow accessible via MCP

---

## EXECUTION TIMELINE

```
PHASE 1 (Parallel - Foundation):
  [W1] MCP Server Core ──────────────────────────────────────►
  [W2] Type Inference Engine ────────────────────────────────►
  [W3] Schema Contract Foundation ───────────────────────────►

PHASE 2 (Parallel - Core Systems):
  [W4] Schema Approval/Amendment ────────────────────────────►
  [W5] Backtest Engine ──────────────────────────────────────►

PHASE 3 (Sequential - Integration):
  [W6] MCP Tools Implementation ─────────────────────────────►
  [W7] CLI Integration ──────────────────────────────────────►
```

---

## VALIDATION CHECKLIST

### After Phase 1
```bash
cargo check -p casparian_mcp
cargo check -p casparian_worker
cargo check -p casparian_schema
cargo test -p casparian_worker -- type_inference
```

### After Phase 2
```bash
cargo check -p casparian_backtest
cargo test -p casparian_schema
cargo test -p casparian_backtest
```

### After Phase 3
```bash
cargo build -p casparian
cargo test --workspace

# Full integration test
./target/debug/casparian mcp-server &
# Connect with test client
```

---

## DEPENDENCIES GRAPH

```
                    ┌─────────────────┐
                    │  casparian_mcp  │
                    │      (W1)       │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│casparian_worker │ │casparian_schema │ │casparian_backtest│
│  type_inference │ │     (W3)        │ │      (W5)       │
│      (W2)       │ └────────┬────────┘ └────────┬────────┘
└─────────────────┘          │                   │
                             │                   │
                             ▼                   │
                    ┌─────────────────┐          │
                    │    approval     │          │
                    │   amendment     │          │
                    │      (W4)       │          │
                    └────────┬────────┘          │
                             │                   │
                             └─────────┬─────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │   MCP Tools     │
                              │      (W6)       │
                              └────────┬────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │ CLI Integration │
                              │      (W7)       │
                              └─────────────────┘
```

---

## SUCCESS CRITERIA

1. **MCP server starts and accepts connections**
2. **Type inference proves types** (not guesses) via constraint propagation
3. **Schema approval locks intent** as contract
4. **Backtest catches edge cases** via fail-fast + high-failure table
5. **End-to-end flow works:** scan → discover → approve → backtest → execute
6. **Claude Code can orchestrate** the entire workflow via MCP tools

---

## NOTES

- **W1 is not blocking** on W2/W3 - can be built in parallel
- **W4 and W5 depend on W3** but not on each other
- **W6 depends on everything** but is pure integration
- **W7 is optional** - MCP server works without CLI wrapper
- Each worker should write tests for their components
- Use existing patterns from `cli/preview.rs` for type inference baseline

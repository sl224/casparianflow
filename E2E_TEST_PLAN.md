# MCP E2E Test Plan

## Overview

This document outlines the end-to-end test flow for the Casparian MCP server, including what works today, what's mocked, and gaps to fill.

---

## Implementation Status Summary

| Tool | Status | Real Implementation |
|------|--------|---------------------|
| `quick_scan` | **READY** | Full - scans filesystem |
| `apply_scope` | **READY** | Full - creates file groups |
| `discover_schemas` | **READY** | Full - CSV parsing + type inference |
| `approve_schemas` | **READY** | Full - creates schema contracts |
| `propose_amendment` | **READY** | Full - proposes schema changes |
| `run_backtest` | **PARTIAL** | Mock parser (always passes) |
| `fix_parser` | **PARTIAL** | Generates fixes (no validation) |
| `execute_pipeline` | **PARTIAL** | Mock execution (counts lines) |
| `query_output` | **PARTIAL** | Mock (basic CSV parsing) |

**What works E2E today:**
- Discovery → Schema Discovery → Schema Approval flow

**What's mocked:**
- Parser execution (backtest, execute_pipeline)
- Output writing (no real Parquet/SQLite output)
- SQL queries (no DuckDB/Polars)

---

## Test Data

### Available Sample Files

```
demo/
├── data/sample_data.csv          # 20 rows: id, name, value, category, timestamp
├── scout/sample_data/
│   ├── sales_2024_01.csv         # 5 rows: date, product, quantity, unit_price, total
│   ├── sales_2024_02.csv         # Similar
│   └── inventory.json            # 4 items: sku, name, quantity_on_hand, reorder_point
```

### Sample Data Schema (Expected)

**sample_data.csv:**
| Column | Expected Type | Notes |
|--------|---------------|-------|
| id | Int64 | Sequential integers |
| name | String | Greek letters |
| value | Float64 | Decimal values |
| category | String | A, B, or C |
| timestamp | Timestamp | ISO 8601 format |

**sales_2024_01.csv:**
| Column | Expected Type | Notes |
|--------|---------------|-------|
| date | Date | YYYY-MM-DD format |
| product | String | Widget names |
| quantity | Int64 | Integer quantities |
| unit_price | Float64 | Decimal prices |
| total | Float64 | quantity * unit_price |

---

## E2E Test Flow

### Phase 1: Discovery (READY)

```bash
# Tool: quick_scan
# Input: Directory path
# Output: File listing with metadata

{
  "name": "quick_scan",
  "arguments": {
    "path": "/Users/shan/workspace/casparianflow/demo",
    "extensions": ["csv"],
    "max_depth": 3
  }
}

# Expected: List of CSV files with sizes and modification times
```

### Phase 2: Apply Scope (READY)

```bash
# Tool: apply_scope
# Input: List of files from quick_scan
# Output: Scope ID for further operations

{
  "name": "apply_scope",
  "arguments": {
    "files": [
      "/Users/shan/workspace/casparianflow/demo/data/sample_data.csv",
      "/Users/shan/workspace/casparianflow/demo/scout/sample_data/sales_2024_01.csv"
    ],
    "scope_name": "sales_analysis",
    "tags": ["csv", "financial"]
  }
}

# Expected: { "scope_id": "uuid", "file_count": 2, "total_size": N }
```

### Phase 3: Discover Schemas (READY)

```bash
# Tool: discover_schemas
# Input: Scope ID or file paths
# Output: Inferred schema with types

{
  "name": "discover_schemas",
  "arguments": {
    "source": "/Users/shan/workspace/casparianflow/demo/data/sample_data.csv",
    "sample_rows": 100
  }
}

# Expected: {
#   "schema_name": "sample_data",
#   "columns": [
#     {"name": "id", "type": "int64", "nullable": false},
#     {"name": "name", "type": "string", "nullable": false},
#     {"name": "value", "type": "float64", "nullable": false},
#     {"name": "category", "type": "string", "nullable": false},
#     {"name": "timestamp", "type": "timestamp", "nullable": false}
#   ],
#   "row_count": 20
# }
```

### Phase 4: Approve Schema (READY)

```bash
# Tool: approve_schemas
# Input: Scope ID with discovered schema
# Output: Locked schema contract

{
  "name": "approve_schemas",
  "arguments": {
    "scope_id": "from-phase-2",
    "approved_by": "shan@example.com",
    "schemas": [
      {
        "name": "sample_data",
        "columns": [
          {"name": "id", "type": "int64", "nullable": false},
          {"name": "name", "type": "string", "nullable": false},
          {"name": "value", "type": "float64", "nullable": false},
          {"name": "category", "type": "string", "nullable": false},
          {"name": "timestamp", "type": "timestamp", "nullable": false}
        ]
      }
    ]
  }
}

# Expected: { "contract_id": "uuid", "version": 1, "approved_at": "..." }
```

### Phase 5: Run Backtest (PARTIAL - Mock Parser)

```bash
# Tool: run_backtest
# Input: Scope ID with approved schema
# Output: Pass/fail metrics

{
  "name": "run_backtest",
  "arguments": {
    "scope_id": "from-phase-2",
    "pass_rate_threshold": 0.95,
    "max_iterations": 5
  }
}

# Current behavior: Mock parser always passes
# Expected (future): Real parser execution with schema validation
```

### Phase 6: Execute Pipeline (PARTIAL - Mock)

```bash
# Tool: execute_pipeline
# Input: Scope ID with passing backtest
# Output: Processing results

{
  "name": "execute_pipeline",
  "arguments": {
    "scope_id": "from-phase-2",
    "mode": "full",
    "output_format": "parquet",
    "output_path": "/tmp/output"
  }
}

# Current behavior: Counts lines, doesn't write files
# Expected (future): Real Arrow/Parquet output
```

### Phase 7: Query Output (PARTIAL - Mock)

```bash
# Tool: query_output
# Input: SQL query against output
# Output: Query results as JSON

{
  "name": "query_output",
  "arguments": {
    "scope_id": "from-phase-2",
    "sql": "SELECT * FROM output WHERE value > 200"
  }
}

# Current behavior: Basic CSV parsing, no SQL
# Expected (future): DuckDB/Polars query engine
```

---

## Testing Options

### Option A: Test Working Flow (Quick)

Test only the fully-implemented path:

```bash
# 1. Start MCP server
cargo run -p casparian -- mcp-server

# 2. Send JSON-RPC requests (separate terminal)
# See test harness below
```

### Option B: Integration Test Script

```bash
# Run the existing E2E tests
cargo test --package casparian_mcp --test e2e_tools -- --nocapture
```

### Option C: Manual Claude Code Test

Configure Claude Code to use the MCP server and test interactively.

---

## Test Harness (JSON-RPC Client)

Create a simple test client to send requests to the MCP server:

```typescript
// test-mcp.ts
import { spawn } from 'child_process';
import * as readline from 'readline';

const server = spawn('cargo', ['run', '-p', 'casparian', '--', 'mcp-server']);

// Send initialize
const initRequest = {
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "test-client", version: "1.0.0" }
  }
};
server.stdin.write(JSON.stringify(initRequest) + '\n');

// List tools
const listRequest = {
  jsonrpc: "2.0",
  id: 2,
  method: "tools/list"
};
server.stdin.write(JSON.stringify(listRequest) + '\n');

// Call quick_scan
const scanRequest = {
  jsonrpc: "2.0",
  id: 3,
  method: "tools/call",
  params: {
    name: "quick_scan",
    arguments: {
      path: process.cwd() + "/demo",
      extensions: ["csv"],
      max_depth: 3
    }
  }
};
server.stdin.write(JSON.stringify(scanRequest) + '\n');

// Read responses
server.stdout.on('data', (data) => {
  console.log('Response:', data.toString());
});
```

---

## Gaps to Fill for Production E2E

### Gap 1: Real Parser Execution

**Current:** Mock parser always returns "pass"
**Need:** Integrate `casparian_worker` bridge mode

```rust
// In run_backtest tool
impl ParserRunner for RealParserRunner {
    fn run(&self, file_path: &str) -> FileTestResult {
        // Use bridge mode to execute Python parser
        // Validate output against schema contract
    }
}
```

### Gap 2: Real Output Writing

**Current:** execute_pipeline counts lines
**Need:** Write actual Parquet/CSV/SQLite files

```rust
// In execute_pipeline tool
fn execute(&self, files: &[String], output_path: &str, format: OutputFormat) {
    // Use arrow-rs to write Parquet
    // Or CSV writer for CSV format
}
```

### Gap 3: SQL Query Engine

**Current:** query_output parses CSV manually
**Need:** DuckDB or Polars for SQL queries

```rust
// In query_output tool
fn query(&self, output_path: &str, sql: &str) -> Vec<Row> {
    // Use duckdb-rs or polars for actual SQL
    let conn = duckdb::Connection::open_in_memory()?;
    conn.execute(&format!("CREATE VIEW output AS SELECT * FROM read_parquet('{}')", output_path))?;
    conn.prepare(sql)?.query_map([], |row| ...)?
}
```

### Gap 4: Database Persistence

**Current:** Tools use in-memory SQLite
**Need:** Connect to `~/.casparian_flow/casparian_flow.sqlite3`

---

## Recommended Test Sequence

### Today (With Current Implementation)

1. **Test Discovery Flow:**
   ```
   quick_scan → apply_scope → discover_schemas → approve_schemas
   ```
   This path is fully implemented and will work E2E.

2. **Test Backtest (With Mock):**
   ```
   run_backtest (will always pass)
   fix_parser (will generate fixes)
   ```
   Works but with mock parser.

3. **Test Amendment Flow:**
   ```
   propose_amendment
   ```
   Works for proposing schema changes.

### Future (After Gap Filling)

1. **Full Pipeline:**
   ```
   quick_scan → apply_scope → discover_schemas → approve_schemas →
   run_backtest (real) → fix_parser → run_backtest (pass) →
   execute_pipeline (real output) → query_output (SQL)
   ```

---

## Success Criteria

### Minimum Viable Test (Today)

- [x] MCP server starts and responds to initialize
- [x] `tools/list` returns all 9 tools
- [x] `quick_scan` finds CSV files in demo/
- [x] `apply_scope` creates a scope with files
- [x] `discover_schemas` correctly infers types (int64, float64, timestamp)
- [x] `approve_schemas` creates a locked contract
- [x] `run_backtest` executes (with mock parser)

**Last verified:** 2025-01-06 - All 7 tests passing via `bun run scripts/test-mcp-e2e.ts`

### Full E2E (Future)

- [ ] Backtest runs real Python parser
- [ ] Failed files are categorized correctly
- [ ] fix_parser suggestions are validated
- [ ] execute_pipeline writes real Parquet files
- [ ] query_output executes real SQL queries

---

## Quick Start Commands

```bash
# Build
cargo build -p casparian

# Run MCP server (stdio mode)
cargo run -p casparian -- mcp-server

# Run E2E tests
cargo test --package casparian_mcp --test e2e_tools -- --nocapture

# Test type inference directly
cargo test --package casparian_worker --test e2e_type_inference -- --nocapture
```

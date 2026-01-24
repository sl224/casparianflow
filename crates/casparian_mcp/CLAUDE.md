# Claude Code Instructions for casparian_mcp

## Quick Reference

```bash
cargo test -p casparian_mcp              # All tests
cargo test -p casparian_mcp -- query     # Query tests
cargo test -p casparian_mcp -- redaction # Redaction tests
```

---

## Overview

`casparian_mcp` is the **MCP Server crate** that exposes Casparian's capabilities to AI assistants via the Model Context Protocol (JSON-RPC over stdio).

**Key Responsibilities:**
1. MCP protocol handling (JSON-RPC 2.0)
2. Tool registry and dispatch
3. Security subsystem (path allowlist, output budgets, audit)
4. Job and approval management
5. Direct integration with `casparian_sentinel` storage

---

## Architecture

```
AI Assistant (Claude)
        │
        │ MCP Protocol (JSON-RPC over stdio)
        ▼
┌─────────────────────────────────────────────────────┐
│                 casparian_mcp                        │
│                                                     │
│  ┌───────────────┐  ┌───────────────────────────┐  │
│  │   Server      │  │      Security             │  │
│  │  (stdio)      │  │  • PathAllowlist          │  │
│  │               │  │  • OutputBudget           │  │
│  │               │  │  • AuditLog               │  │
│  └───────────────┘  └───────────────────────────┘  │
│          │                                          │
│          ▼                                          │
│  ┌───────────────────────────────────────────────┐  │
│  │              Tool Registry                    │  │
│  │  • casparian_plugins    • casparian_query    │  │
│  │  • casparian_scan       • casparian_preview  │  │
│  │  • casparian_backtest_start                  │  │
│  │  • casparian_run_request                     │  │
│  │  • casparian_job_*      • casparian_approval_*│  │
│  └───────────────────────────────────────────────┘  │
│          │                                          │
│          ▼                                          │
│  ┌───────────────────────────────────────────────┐  │
│  │        db_store.rs (Bridge Layer)             │  │
│  │  DbJobStore, DbApprovalStore                  │  │
│  └───────────────────────────────────────────────┘  │
│          │                                          │
│          ▼                                          │
│  casparian_sentinel::db::ApiStorage (DuckDB)       │
└─────────────────────────────────────────────────────┘
```

---

## Module Structure

```
crates/casparian_mcp/
├── CLAUDE.md                 # This file
├── Cargo.toml
├── src/
│   ├── lib.rs                # Crate root with re-exports
│   ├── protocol.rs           # JSON-RPC 2.0 types
│   ├── types.rs              # PluginRef, RedactionPolicy, ViolationContext
│   ├── server.rs             # McpServer + McpServerConfig
│   ├── db_store.rs           # Bridge to sentinel's ApiStorage
│   ├── redaction.rs          # Value redaction (hash/truncate/none)
│   ├── security/
│   │   ├── mod.rs            # SecurityConfig, SecurityError
│   │   ├── path_allowlist.rs # Path validation + canonicalization
│   │   ├── output_budget.rs  # Response size limits
│   │   └── audit.rs          # Tool invocation logging
│   ├── jobs/
│   │   ├── mod.rs            # JobId, JobState, JobProgress
│   │   ├── manager.rs        # JobManager lifecycle (DB-backed)
│   │   └── store.rs          # JobStore (legacy JSON store, tests only)
│   ├── approvals/
│   │   ├── mod.rs            # ApprovalId, ApprovalRequest, ApprovalStatus
│   │   ├── manager.rs        # ApprovalManager lifecycle (DB-backed)
│   │   └── store.rs          # ApprovalStore (legacy JSON store, tests only)
│   └── tools/
│       ├── mod.rs            # McpTool trait
│       ├── registry.rs       # ToolRegistry for dispatch
│       ├── plugins.rs        # casparian_plugins
│       ├── scan.rs           # casparian_scan
│       ├── preview.rs        # casparian_preview
│       ├── query.rs          # casparian_query (SQL allowlist)
│       ├── backtest.rs       # casparian_backtest_start
│       ├── run.rs            # casparian_run_request
│       ├── job.rs            # job_status, job_cancel, job_list
│       └── approval.rs       # approval_status, approval_list
└── tests/
    └── (E2E tests in tests/e2e/mcp/)
```

---

## Key Types

### PluginRef

Identifies a parser/plugin:

```rust
pub enum PluginRef {
    Registered {
        plugin: String,
        version: Option<String>,
    },
    Path {
        path: PathBuf,
    },
}

// JSON examples:
// { "plugin": "evtx_native", "version": "0.1.0" }
// { "plugin": "fix_parser" }
// { "path": "./parsers/my_parser.py" }
```

### RedactionPolicy

Controls sensitive data exposure:

```rust
pub struct RedactionPolicy {
    pub mode: RedactionMode,           // none, truncate, hash (default)
    pub max_sample_count: usize,       // Default: 5
    pub max_value_length: usize,       // Default: 100
    pub hash_prefix_length: usize,     // Default: 8
}
```

### SecurityConfig

Security settings for the MCP server:

```rust
pub struct SecurityConfig {
    pub path_allowlist: PathAllowlist,   // Allowed file paths
    pub output_budget: OutputBudget,     // Response size limits
}
```

---

## Tool Implementation

Tools implement the `McpTool` trait:

```rust
#[async_trait::async_trait]
pub trait McpTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> Value;

    async fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        jobs: &Arc<Mutex<JobManager>>,
        approvals: &Arc<Mutex<ApprovalManager>>,
        config: &McpServerConfig,
    ) -> Result<Value>;
}
```

### Adding a New Tool

1. Create `src/tools/my_tool.rs`:

```rust
use super::McpTool;
use serde::{Deserialize, Serialize};

pub struct MyTool;

#[derive(Debug, Deserialize)]
struct MyToolArgs {
    required_field: String,
    #[serde(default)]
    optional_field: bool,
}

#[async_trait::async_trait]
impl McpTool for MyTool {
    fn name(&self) -> &'static str {
        "casparian_my_tool"
    }

    fn description(&self) -> &'static str {
        "Description for Claude"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "required_field": { "type": "string" },
                "optional_field": { "type": "boolean", "default": false }
            },
            "required": ["required_field"]
        })
    }

    async fn execute(&self, args: Value, ...) -> Result<Value> {
        let args: MyToolArgs = serde_json::from_value(args)?;
        // Implementation
        Ok(json!({ "result": "success" }))
    }
}
```

2. Register in `src/tools/registry.rs`:

```rust
pub fn create_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(my_tool::MyTool));
    // ...
    registry
}
```

---

## Query Tool Security

The query tool (`casparian_query`) has multiple security layers:

### 1. SQL Allowlist

```rust
const ALLOWED_PREFIXES: &[&str] = &["SELECT", "WITH", "EXPLAIN"];
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER",
    "TRUNCATE", "COPY", "INSTALL", "LOAD", "ATTACH", "DETACH",
];
```

### 2. Read-Only Connection

```rust
let conn = DbConnection::open_duckdb_readonly(Path::new(&config.db_path))?;
```

### 3. Redaction Applied

```rust
rows = redaction::redact_rows(&rows, &redaction_policy);
```

### 4. Row Limit Enforced

```rust
let limit = args.limit.min(security.output_budget.max_rows());
```

---

## Redaction Module

Located in `src/redaction.rs`:

```rust
// Redact a single value
pub fn redact_value(value: &Value, policy: &RedactionPolicy) -> Value;

// Redact all rows (for query results)
pub fn redact_rows(rows: &[Vec<Value>], policy: &RedactionPolicy) -> Vec<Vec<Value>>;

// Check if a column name suggests sensitive data
pub fn is_sensitive_column(name: &str) -> bool;
```

**Redaction modes:**

| Mode | Output | Example |
|------|--------|---------|
| `none` | Raw value | `"secret123"` |
| `truncate` | First N chars | `"secr..."` |
| `hash` | SHA256 prefix | `"[hash:a1b2c3d4]"` |

---

## Path Allowlist

Located in `src/security/path_allowlist.rs`:

```rust
pub struct PathAllowlist {
    roots: Vec<PathBuf>,  // Canonicalized allowed roots
}

impl PathAllowlist {
    pub fn validate(&self, path: &Path) -> Result<PathBuf, SecurityError>;
    pub fn would_be_allowed(&self, path: &Path) -> bool;
}
```

**Security features:**
- `..` traversal detection
- Symlink resolution and validation
- Canonicalization before comparison
- macOS `/var` → `/private/var` symlink handling

---

## Bridge Layer (db_store.rs)

Bridges MCP types with sentinel's `ApiStorage`:

```rust
pub struct DbJobStore {
    storage: ApiStorage,
}

impl DbJobStore {
    pub fn create_job(&self, plugin_ref: &PluginRef, ...) -> Result<JobId>;
    pub fn get_job(&self, job_id: JobId) -> Result<Option<Job>>;
    pub fn list_jobs(&self, status: Option<HttpJobStatus>, limit: usize) -> Result<Vec<Job>>;
    pub fn update_job_status(&self, job_id: JobId, status: HttpJobStatus) -> Result<()>;
    pub fn cancel_job(&self, job_id: JobId) -> Result<bool>;
}

pub struct DbApprovalStore {
    storage: ApiStorage,
}

impl DbApprovalStore {
    pub fn create_approval(&self, operation: &ApprovalOperation, ...) -> Result<String>;
    pub fn get_approval(&self, approval_id: &str) -> Result<Option<Approval>>;
    pub fn approve(&self, approval_id: &str, decided_by: Option<&str>) -> Result<bool>;
    pub fn reject(&self, approval_id: &str, ...) -> Result<bool>;
}
```

---

## Testing

### Unit Tests

```bash
# All tests
cargo test -p casparian_mcp

# Specific modules
cargo test -p casparian_mcp -- query
cargo test -p casparian_mcp -- redaction
cargo test -p casparian_mcp -- path_allowlist
```

### E2E Tests

Located in `tests/e2e/mcp/`:

```bash
# Smoke test (server starts, tools/list works)
./tests/e2e/mcp/test_mcp_server.sh

# Authoritative E2E via Claude Code CLI
./tests/e2e/mcp/run_with_claude.sh          # Backtest flow
./tests/e2e/mcp/run_with_claude.sh approval # Approval flow
```

**Important:** No mocking in tests. Unit tests use real in-memory DuckDB, E2E tests use real Claude.

---

## Common Tasks

### Debug a Tool Failure

1. Check audit log: `~/.casparian_flow/mcp_audit.log`
2. Run with RUST_LOG: `RUST_LOG=debug casparian mcp serve`
3. Test tool directly:
   ```bash
   echo '{"method":"tools/call","params":{"name":"casparian_query","arguments":{"sql":"SELECT 1"}}}' | cargo run -- mcp serve
   ```

### Add SQL to Allowlist

In `src/tools/query.rs`, modify:
```rust
const ALLOWED_PREFIXES: &[&str] = &["SELECT", "WITH", "EXPLAIN", "NEW_KEYWORD"];
```

### Extend Redaction

In `src/redaction.rs`:
```rust
pub fn is_sensitive_column(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_PATTERNS.iter().any(|p| lower.contains(p))
        || name == "my_new_sensitive_field"  // Add here
}
```

---

## Key Principles

1. **Security first** - Path allowlist, SQL allowlist, redaction are P0
2. **No mocking** - Tests use real databases and real Claude
3. **Direct crate calls** - No HTTP server, MCP calls Rust libraries directly
4. **Job-first architecture** - Long operations return job_id, poll for status
5. **Non-blocking approvals** - Write operations create approval requests

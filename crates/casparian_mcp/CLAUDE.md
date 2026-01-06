# Claude Code Instructions for casparian_mcp

## Quick Reference

```bash
cargo test -p casparian_mcp              # All tests
cargo test -p casparian_mcp --test e2e_tools  # E2E tests
```

---

## Overview

`casparian_mcp` is the **Model Context Protocol (MCP) server** for Claude Code integration. It provides 9 tools that enable AI-assisted data processing workflows.

### What is MCP?

MCP (Model Context Protocol) is a standard for LLM tool integration. It allows Claude Code to:
1. Discover available tools and their schemas
2. Call tools with parameters
3. Receive structured results

### Architecture

```
Claude Code ──JSON-RPC──> MCP Server ──> Tool Registry ──> Tool Implementations
                                              │
                                    ┌─────────┼─────────┐
                                    ▼         ▼         ▼
                               Discovery   Schema   Backtest
                                 Tools     Tools     Tools
```

---

## The 9 MCP Tools

### Discovery Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `quick_scan` | Fast metadata scan of directories | Initial exploration |
| `apply_scope` | Group files into processing scopes | After identifying relevant files |

### Schema Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `discover_schemas` | Infer schema from file contents | Before approval |
| `approve_schemas` | Create locked contracts | When schema looks correct |
| `propose_amendment` | Modify existing contracts | When data format changes |

### Backtest Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `run_backtest` | Validate parser against files | Before deploying parser |
| `fix_parser` | Generate parser fixes | When backtest fails |

### Execution Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `execute_pipeline` | Run full processing | After backtest passes |
| `query_output` | Query processed data | To verify results |

---

## Tool Implementation Pattern

All tools implement the `McpTool` trait:

```rust
pub trait McpTool: Send + Sync {
    /// Tool name (e.g., "quick_scan")
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON Schema for parameters
    fn input_schema(&self) -> ToolInputSchema;

    /// Execute the tool
    fn call(&self, params: serde_json::Value) -> Result<ToolContent, String>;
}
```

### Example Tool Structure

```rust
// tools/quick_scan.rs
pub struct QuickScanTool;

impl McpTool for QuickScanTool {
    fn name(&self) -> &str { "quick_scan" }

    fn description(&self) -> &str {
        "Quickly scan a directory for files matching criteria"
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "path": { "type": "string", "description": "Directory to scan" },
                "extensions": { "type": "array", "items": { "type": "string" } },
                "max_depth": { "type": "integer", "default": 10 }
            }),
            required: vec!["path".to_string()],
        }
    }

    fn call(&self, params: serde_json::Value) -> Result<ToolContent, String> {
        // Implementation
    }
}
```

---

## Key Types

### ToolInputSchema

```rust
pub struct ToolInputSchema {
    pub schema_type: String,      // Always "object"
    pub properties: Value,        // JSON Schema properties
    pub required: Vec<String>,    // Required parameter names
}
```

### ToolContent

Tool results are returned as `ToolContent`:

```rust
pub enum ToolContent {
    Text { text: String },
    // Future: Image, Resource, etc.
}
```

### Tool Registration

```rust
pub fn create_default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Discovery
    registry.register(Box::new(QuickScanTool));
    registry.register(Box::new(ApplyScopeTool));

    // Schema
    registry.register(Box::new(DiscoverSchemasTool));
    registry.register(Box::new(ApproveSchemasTool));
    registry.register(Box::new(ProposeAmendmentTool));

    // Backtest
    registry.register(Box::new(RunBacktestTool));
    registry.register(Box::new(FixParserTool));

    // Execution
    registry.register(Box::new(ExecutePipelineTool));
    registry.register(Box::new(QueryOutputTool));

    registry
}
```

---

## Server Protocol

The MCP server uses JSON-RPC 2.0 over stdio:

### Tool Listing

```json
// Request
{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}

// Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "quick_scan",
        "description": "Quickly scan a directory...",
        "inputSchema": { "type": "object", "properties": {...} }
      }
    ]
  }
}
```

### Tool Invocation

```json
// Request
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "quick_scan",
    "arguments": { "path": "/data", "extensions": ["csv"] }
  }
}

// Response
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [{ "type": "text", "text": "Found 42 files..." }]
  }
}
```

---

## Common Tasks

### Add a New MCP Tool

1. Create `tools/my_tool.rs`:
```rust
pub struct MyTool;

impl McpTool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    // ... implement other methods
}
```

2. Export in `tools/mod.rs`:
```rust
mod my_tool;
pub use my_tool::MyTool;
```

3. Register in `tools/registry.rs`:
```rust
registry.register(Box::new(MyTool));
```

4. Add E2E test in `tests/e2e_tools.rs`:
```rust
#[test]
fn test_my_tool_basic() {
    let registry = create_default_registry();
    let tool = registry.get("my_tool").unwrap();

    let result = tool.call(json!({ "param": "value" })).unwrap();
    // Assert on result
}
```

### Debug Tool Execution

```rust
// In tool implementation
fn call(&self, params: serde_json::Value) -> Result<ToolContent, String> {
    tracing::info!("Tool called with: {:?}", params);

    // Parse parameters with helpful errors
    let path = params.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    // ... implementation
}
```

### Handle Optional Parameters

```rust
fn call(&self, params: serde_json::Value) -> Result<ToolContent, String> {
    // Required
    let path = params["path"].as_str()
        .ok_or("path is required")?;

    // Optional with default
    let max_depth = params["max_depth"].as_i64().unwrap_or(10);

    // Optional array
    let extensions: Vec<&str> = params["extensions"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
}
```

---

## Integration with Other Crates

### Schema Discovery (casparian_schema)

```rust
use casparian_schema::{SchemaContract, LockedSchema};

fn discover_schemas(&self, files: &[&str]) -> Vec<LockedSchema> {
    // Use type inference from casparian_worker
    // Create LockedSchema definitions
}
```

### Backtest Execution (casparian_backtest)

```rust
use casparian_backtest::{BacktestRunner, BacktestConfig};

fn run_backtest(&self, scope_id: &str) -> BacktestResult {
    let config = BacktestConfig {
        pass_threshold: 0.95,
        max_iterations: 10,
    };
    let runner = BacktestRunner::new(config);
    runner.run(scope_id)
}
```

---

## Testing

### E2E Test Pattern

```rust
#[test]
fn test_full_mcp_pipeline() {
    let registry = create_default_registry();

    // 1. Scan for files
    let scan_result = registry.get("quick_scan").unwrap()
        .call(json!({"path": "/tmp/test", "extensions": ["csv"]})).unwrap();

    // 2. Create scope
    let scope_result = registry.get("apply_scope").unwrap()
        .call(json!({"files": [...], "scope_name": "test"})).unwrap();

    // 3. Discover schemas
    let schema_result = registry.get("discover_schemas").unwrap()
        .call(json!({"scope_id": "..."})).unwrap();

    // 4. Approve
    let approve_result = registry.get("approve_schemas").unwrap()
        .call(json!({"scope_id": "...", "approved_by": "test"})).unwrap();

    // 5. Backtest
    let backtest_result = registry.get("run_backtest").unwrap()
        .call(json!({"scope_id": "..."})).unwrap();

    // 6. Execute
    let exec_result = registry.get("execute_pipeline").unwrap()
        .call(json!({"scope_id": "..."})).unwrap();

    // 7. Query
    let query_result = registry.get("query_output").unwrap()
        .call(json!({"scope_id": "...", "sql": "SELECT * FROM output"})).unwrap();
}
```

---

## File Structure

```
casparian_mcp/
├── CLAUDE.md           # This file
├── Cargo.toml
├── src/
│   ├── lib.rs          # Crate root, exports
│   ├── protocol.rs     # MCP protocol types
│   ├── server.rs       # JSON-RPC server
│   ├── tools/
│   │   ├── mod.rs      # Tool exports
│   │   ├── registry.rs # Tool registration
│   │   ├── quick_scan.rs
│   │   ├── apply_scope.rs
│   │   ├── discover_schemas.rs
│   │   ├── approve_schemas.rs
│   │   ├── propose_amendment.rs
│   │   ├── run_backtest.rs
│   │   ├── fix_parser.rs
│   │   ├── execute_pipeline.rs
│   │   └── query_output.rs
│   └── types.rs        # Shared types
└── tests/
    └── e2e_tools.rs    # E2E tests (20 tests)
```

---

## Error Handling

Tools return `Result<ToolContent, String>` where the error string is displayed to the user:

```rust
// Good error messages
Err("File not found: /path/to/file.csv".to_string())
Err("Invalid JSON in request: missing 'path' field".to_string())
Err("Schema violation: column 'amount' expected Int64, got String".to_string())

// Bad error messages (too vague)
Err("Error".to_string())
Err("Invalid input".to_string())
```

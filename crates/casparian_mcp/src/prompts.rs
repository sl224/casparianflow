//! MCP Prompts for LLM context
//!
//! Provides system context for Claude to effectively use the MCP tools.
//! These prompts explain the workflow, constraints, and domain concepts.

use crate::protocol::{PromptContent, PromptDefinition, PromptMessage, PromptsGetResult};

/// All available prompts
pub const PROMPTS: &[(&str, &str)] = &[
    ("workflow-guide", "Complete guide to the data transformation workflow"),
    ("tool-reference", "Quick reference for all 11 MCP tools"),
    ("constraint-reasoning", "How type inference and constraints work"),
    ("approval-criteria", "When to proceed vs escalate to human review"),
];

/// Get a prompt by name
pub fn get_prompt(name: &str) -> Option<PromptsGetResult> {
    match name {
        "workflow-guide" => Some(workflow_guide()),
        "tool-reference" => Some(tool_reference()),
        "constraint-reasoning" => Some(constraint_reasoning()),
        "approval-criteria" => Some(approval_criteria()),
        _ => None,
    }
}

/// List all available prompts
pub fn list_prompts() -> Vec<PromptDefinition> {
    PROMPTS
        .iter()
        .map(|(name, desc)| PromptDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            arguments: None,
        })
        .collect()
}

// =============================================================================
// Prompt Content
// =============================================================================

fn workflow_guide() -> PromptsGetResult {
    PromptsGetResult {
        description: Some("Complete guide to the CasparianFlow data transformation workflow".into()),
        messages: vec![PromptMessage {
            role: "user".into(),
            content: PromptContent::Text {
                text: WORKFLOW_GUIDE.into(),
            },
        }],
    }
}

const WORKFLOW_GUIDE: &str = r#"# CasparianFlow MCP Workflow Guide

## Overview

CasparianFlow transforms "dark data" (messy files) into clean, queryable datasets.
The workflow has 6 phases, each with specific tools and approval gates.

## The 6 Phases

```
1. DISCOVERY ──► 2. SCHEMA INFERENCE ──► 3. SCHEMA APPROVAL
      │                    │                      │
      ▼                    ▼                      ▼
  quick_scan        discover_schemas       approve_schemas
  apply_scope                              propose_amendment
                                                  │
4. PARSER GENERATION ◄────────────────────────────┘
      │
      ▼
  generate_parser
      │
5. BACKTEST ◄─────────────────────────────────────┐
      │                                           │
      ▼                                           │
  run_backtest ──► fix_parser ──► refine_parser ──┘
      │            (if fails)      (max 3 tries)
      │
      ▼ (if passes)
6. EXECUTION
      │
      ▼
  execute_pipeline
  query_output
```

## Phase 1: Discovery

**Goal:** Find and group files for processing.

**Tools:**
- `quick_scan` - Fast metadata scan (file counts, sizes, extensions)
- `apply_scope` - Group files by pattern into a processing scope

**Outputs:**
- ScopeId - Unique identifier for the file group
- File metadata (paths, sizes, extensions)

**When to proceed:** Files are identified and grouped by similar structure.

## Phase 2: Schema Inference

**Goal:** Analyze file structure and infer column types.

**Tool:** `discover_schemas`

**Outputs:**
- Column names and inferred types (int64, float64, string, timestamp, boolean)
- Constraint reasoning (WHY each type was chosen)
- Confidence scores (0.0-1.0)
- Alternative types when ambiguous
- Schema groups for bulk approval

**Key concepts:**
- `ColumnConstraint` - Full reasoning for type inference
- `TypeEvidence` - Sample values and eliminated types
- `EliminatedType` - Why other types were rejected

## Phase 3: Schema Approval

**Goal:** Human confirms or modifies the inferred schema.

**Tools:**
- `approve_schemas` - Lock schema as a contract
- `propose_amendment` - Modify schema before locking

**Outputs:**
- ContractId - Locked schema definition
- Parser constraints

**APPROVAL GATE:** Always pause here for human review. Never auto-approve schemas.

## Phase 4: Parser Generation

**Goal:** Generate Python parser code from the schema contract.

**Tool:** `generate_parser`

**Outputs:**
- Python parser code (polars-based)
- Bridge Protocol format (TOPIC, SINK, parse() function)
- Type conversions and validation

**Bridge Protocol format:**
```python
TOPIC = "schema_name"
SINK = "parquet"  # or "csv", "duckdb"

def parse(file_path: str) -> pl.DataFrame:
    # Read file
    # Convert types
    # Validate required columns
    return df
```

## Phase 5: Backtest

**Goal:** Test parser against multiple files, fix any failures.

**Tools:**
- `run_backtest` - Test parser on files (fail-fast optimization)
- `fix_parser` - Analyze failures and suggest fixes
- `refine_parser` - Apply fixes with bounded iteration (max 3 attempts)

**Bounded iteration:**
- Max 3 refinement attempts before human escalation
- Each attempt targets specific failure categories
- Progress is tracked for visibility

**Failure categories:**
- NullValue - Unexpected nulls
- TypeCast - Type conversion errors
- MissingColumn - Required columns absent
- ParseError - File parsing failures

**When to escalate:** After 3 failed refinement attempts OR when errors are ambiguous.

## Phase 6: Execution

**Goal:** Run the validated parser on all files and store output.

**Tools:**
- `execute_pipeline` - Process files and write to sink
- `query_output` - SQL queries against processed data

**Outputs:**
- Processed data in configured sink (Parquet, CSV, SQLite)
- Execution metrics

## Key Domain Concepts

### ScopeId
Identifies a logical group of files for processing. Created by `apply_scope`.
Files in the same scope share a schema and parser.

### ContractId
Identifies a locked schema definition. Created by `approve_schemas`.
The parser MUST conform to this contract. Cannot be changed without amendment.

### WorkflowMetadata
Every tool response includes workflow metadata:
- `phase` - Current workflow phase
- `needs_approval` - Whether to pause for human
- `pending_decisions` - Questions for human input
- `next_actions` - Suggested tool calls
- `bulk_approval_options` - Grouping hints for efficiency

### Constraint Flow
Constraints flow through the pipeline:
1. Schema inference creates ColumnConstraints with reasoning
2. Schema approval locks constraints into a Contract
3. Parser generation uses constraints for type conversion code
4. Backtest validates output against constraints
5. Execution enforces constraints at runtime

## Best Practices

1. **Always read workflow metadata** - It tells you what to do next
2. **Never skip approval gates** - Schemas and amendments need human review
3. **Trust the constraint reasoning** - If confidence < 0.8, ask for human input
4. **Use bounded iteration** - Don't loop forever on parser fixes
5. **Batch similar operations** - Use schema_groups for bulk approval
6. **Show your reasoning** - Explain constraint decisions to the user
"#;

fn tool_reference() -> PromptsGetResult {
    PromptsGetResult {
        description: Some("Quick reference for all 11 MCP tools".into()),
        messages: vec![PromptMessage {
            role: "user".into(),
            content: PromptContent::Text {
                text: TOOL_REFERENCE.into(),
            },
        }],
    }
}

const TOOL_REFERENCE: &str = r#"# MCP Tool Reference

## Discovery Tools

### quick_scan
**Purpose:** Fast metadata scan of a directory
**When to use:** First step when user provides a directory path
**Input:** `{ path: string, extensions?: string[], recursive?: boolean }`
**Output:** File counts, sizes, extensions, directory structure

### apply_scope
**Purpose:** Group files into a processing scope
**When to use:** After quick_scan identifies target files
**Input:** `{ name: string, files: string[], tags?: string[] }`
**Output:** ScopeId, file list, scope metadata

## Schema Tools

### discover_schemas
**Purpose:** Analyze files and infer schema structure
**When to use:** After apply_scope creates a file group
**Input:** `{ files: string[], sample_rows?: number }`
**Output:** Schemas with columns, types, constraints, schema_groups

### approve_schemas
**Purpose:** Lock schema as a contract
**When to use:** After human reviews inferred schema
**Input:** `{ scope_id: string, schemas: Schema[], approved_by: string }`
**Output:** ContractId, locked schema definition

### propose_amendment
**Purpose:** Modify existing schema contract
**When to use:** When data doesn't match current contract
**Input:** `{ contract_id: string, changes: SchemaChange[], reason: string }`
**Output:** Updated contract, amendment history

## Backtest Tools

### run_backtest
**Purpose:** Test parser against multiple files
**When to use:** After generate_parser creates code
**Input:** `{ parser_code: string, files: string[], contract_id?: string }`
**Output:** Pass rate, failures by category, failing files

### fix_parser
**Purpose:** Analyze failures and suggest fixes
**When to use:** After run_backtest shows failures
**Input:** `{ parser_code: string, failures: Failure[] }`
**Output:** Suggested fixes, failure analysis

## Codegen Tools

### generate_parser
**Purpose:** Generate Python parser from schema
**When to use:** After approve_schemas locks the contract
**Input:** `{ schema: Schema, options?: { sink_type, include_validation } }`
**Output:** Python code in Bridge Protocol format

### refine_parser
**Purpose:** Iteratively fix parser based on errors
**When to use:** After backtest failures, max 3 attempts
**Input:** `{ parser_code: string, errors: Error[], attempt: number }`
**Output:** Refined code, status (retry/success/escalate), changes made

## Execution Tools

### execute_pipeline
**Purpose:** Run parser on files and write output
**When to use:** After backtest passes
**Input:** `{ parser_code: string, files: string[], sink: SinkConfig }`
**Output:** Execution metrics, output location

### query_output
**Purpose:** SQL queries against processed data
**When to use:** After execute_pipeline completes
**Input:** `{ source: string, query?: string, limit?: number }`
**Output:** Query results as records
"#;

fn constraint_reasoning() -> PromptsGetResult {
    PromptsGetResult {
        description: Some("How type inference and constraints work".into()),
        messages: vec![PromptMessage {
            role: "user".into(),
            content: PromptContent::Text {
                text: CONSTRAINT_REASONING.into(),
            },
        }],
    }
}

const CONSTRAINT_REASONING: &str = r#"# Constraint Reasoning

## How Type Inference Works

CasparianFlow uses **elimination-based type inference**:
1. Start with all possible types for a column
2. Test each value against type parsers
3. Eliminate types that fail
4. Keep the most specific type that works for all values

### Type Hierarchy (most to least specific)
```
boolean   (true/false only)
    ↓
int64     (whole numbers)
    ↓
float64   (decimal numbers)
    ↓
timestamp (datetime patterns)
    ↓
string    (anything)
```

### Example: Column "price"
```
Values: ["100", "200.50", "N/A", "350"]

int64:     fails on "200.50" (has decimal) and "N/A" (not numeric)
float64:   fails on "N/A" (not numeric)
timestamp: fails on all (not date patterns)
string:    works for all ✓

Result: string (with note about potential float64 if "N/A" handled)
```

## Understanding ColumnConstraint

Each column gets a `ColumnConstraint` with:

```json
{
  "resolved_type": "float64",
  "confidence": 0.85,
  "evidence": {
    "sample_values": ["100.50", "200.75", "350.00"],
    "eliminated_types": [
      {
        "type_name": "int64",
        "reason": "Contains decimal values",
        "counter_examples": ["100.50", "200.75"]
      }
    ],
    "match_percentage": 95.0
  },
  "assumptions": ["N/A values will be null"],
  "needs_human_decision": false,
  "human_question": null,
  "alternatives": [
    {"type_name": "string", "trade_off": "Loses numeric operations"}
  ]
}
```

## Confidence Scores

- **1.0:** All values match, no ambiguity
- **0.9-0.99:** Minor edge cases (trailing spaces, etc.)
- **0.8-0.9:** Some nulls or special values
- **0.6-0.8:** Ambiguous, may need human input
- **< 0.6:** Low confidence, definitely needs human decision

## When to Ask Humans

Set `needs_human_decision: true` when:
1. Confidence < 0.8
2. Multiple equally-valid type alternatives exist
3. Special values like "N/A", "NULL", "-" appear
4. Date formats are ambiguous (MM/DD vs DD/MM)
5. Boolean representations are unclear (0/1 vs true/false vs Y/N)

## Human Questions

Generate specific questions, not generic ones:

**Good:** "Column 'date' has values like '01/02/2024'. Is this MM/DD/YYYY (US) or DD/MM/YYYY (EU)?"

**Bad:** "What type should 'date' be?"

## Constraint Flow Through Pipeline

```
discover_schemas
    └─► ColumnConstraint (with evidence)
            │
approve_schemas
    └─► ContractId (locks constraints)
            │
generate_parser
    └─► Type conversion code (from constraints)
            │
run_backtest
    └─► Validates output against constraints
            │
execute_pipeline
    └─► Enforces constraints at runtime
```

## Bulk Approval

When multiple columns share constraints (e.g., all string columns in a wide table),
use `schema_groups` and `bulk_approval_options` for efficient approval:

```json
{
  "bulk_approval_options": [
    {
      "group_id": "sales_string",
      "count": 12,
      "description": "12 string columns in schema 'sales'"
    }
  ]
}
```

Approve similar columns together instead of one-by-one.
"#;

fn approval_criteria() -> PromptsGetResult {
    PromptsGetResult {
        description: Some("When to proceed vs escalate to human review".into()),
        messages: vec![PromptMessage {
            role: "user".into(),
            content: PromptContent::Text {
                text: APPROVAL_CRITERIA.into(),
            },
        }],
    }
}

const APPROVAL_CRITERIA: &str = r#"# Approval Criteria

## Required Approval Gates

These ALWAYS require human approval (never auto-proceed):

1. **Schema Approval** (`approve_schemas`)
   - Even with high confidence, humans must review inferred schemas
   - Reason: Schema defines the contract all future data must match

2. **Schema Amendments** (`propose_amendment`)
   - Changes to locked contracts require explicit approval
   - Reason: May affect downstream systems expecting old schema

3. **After Max Refinement Attempts** (`refine_parser` at attempt 3)
   - If 3 parser fixes haven't resolved failures, escalate
   - Reason: Avoid infinite loops, get human insight

## Auto-Proceed Criteria

These CAN proceed without explicit approval:

1. **File Discovery** (`quick_scan`, `apply_scope`)
   - Safe to scan and group files automatically
   - Exception: Very large directories (>10000 files) should warn user

2. **Schema Inference** (`discover_schemas`)
   - Safe to analyze file structure
   - Results shown to user for approval in next step

3. **Parser Generation** (`generate_parser`)
   - Safe to generate from approved schema
   - Code will be tested in backtest phase

4. **Backtest Iterations** (`run_backtest`, `fix_parser`, `refine_parser`)
   - Safe to iterate up to 3 times
   - Each iteration narrows the failure set

5. **Execution** (`execute_pipeline`)
   - Safe after backtest passes with >95% success rate
   - Exception: First-time execution should confirm with user

## Escalation Triggers

Always escalate to human when:

### Type Inference
- Confidence < 0.8 for any column
- Multiple alternative types with < 10% confidence difference
- Date format ambiguity (MM/DD vs DD/MM)
- Special null values detected ("N/A", "-", "NULL", etc.)

### Backtest
- Pass rate < 80%
- Same error recurring after fix attempts
- Errors in > 50% of files
- Unknown error categories

### Parser Refinement
- Attempt count reaches 3
- No improvement between attempts (same error count)
- Refinement introduces new errors

### Execution
- First execution of a new pipeline
- Data volume > 1GB
- Writing to production sinks

## Communication Patterns

### When Escalating
```
"I've attempted to fix the parser 3 times but the error persists.

Error: Type cast failed for column 'amount' - values include 'N/A'

Options:
1. Convert 'N/A' to null (loses information)
2. Keep as string (loses numeric operations)
3. Create separate column for null indicator

Which approach do you prefer?"
```

### When Proceeding Automatically
```
"Backtest passed (98% success rate, 2 files with null values handled).
Proceeding to execute pipeline."
```

### When Showing Progress
```
"Refinement attempt 2/3:
- Fixed: TypeCast error (added null handling)
- Remaining: 3 files with malformed dates

Running backtest..."
```

## WorkflowMetadata Signals

Always check `workflow.needs_approval`:
- `true` → STOP and ask user
- `false` → Can proceed (but still show progress)

Check `workflow.pending_decisions`:
- Non-empty → Present options to user
- Empty → No decisions needed

Check `workflow.next_actions`:
- `auto_suggested: true` → Safe to proceed
- `requires_approval: true` → Must ask first
"#;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_prompts() {
        let prompts = list_prompts();
        assert_eq!(prompts.len(), 4);
        assert!(prompts.iter().any(|p| p.name == "workflow-guide"));
        assert!(prompts.iter().any(|p| p.name == "tool-reference"));
        assert!(prompts.iter().any(|p| p.name == "constraint-reasoning"));
        assert!(prompts.iter().any(|p| p.name == "approval-criteria"));
    }

    #[test]
    fn test_get_workflow_guide() {
        let prompt = get_prompt("workflow-guide").unwrap();
        assert!(prompt.description.is_some());
        assert_eq!(prompt.messages.len(), 1);
        assert_eq!(prompt.messages[0].role, "user");
    }

    #[test]
    fn test_get_unknown_prompt() {
        assert!(get_prompt("unknown").is_none());
    }

    #[test]
    fn test_workflow_guide_content() {
        let prompt = get_prompt("workflow-guide").unwrap();
        match &prompt.messages[0].content {
            PromptContent::Text { text } => {
                assert!(text.contains("Discovery"));
                assert!(text.contains("Schema Approval"));
                assert!(text.contains("Backtest"));
                assert!(text.contains("ContractId"));
                assert!(text.contains("ScopeId"));
            }
        }
    }

    #[test]
    fn test_tool_reference_content() {
        let prompt = get_prompt("tool-reference").unwrap();
        match &prompt.messages[0].content {
            PromptContent::Text { text } => {
                // Verify all 11 tools are documented
                assert!(text.contains("quick_scan"));
                assert!(text.contains("apply_scope"));
                assert!(text.contains("discover_schemas"));
                assert!(text.contains("approve_schemas"));
                assert!(text.contains("propose_amendment"));
                assert!(text.contains("run_backtest"));
                assert!(text.contains("fix_parser"));
                assert!(text.contains("generate_parser"));
                assert!(text.contains("refine_parser"));
                assert!(text.contains("execute_pipeline"));
                assert!(text.contains("query_output"));
            }
        }
    }

    #[test]
    fn test_constraint_reasoning_content() {
        let prompt = get_prompt("constraint-reasoning").unwrap();
        match &prompt.messages[0].content {
            PromptContent::Text { text } => {
                assert!(text.contains("elimination-based"));
                assert!(text.contains("confidence"));
                assert!(text.contains("ColumnConstraint"));
            }
        }
    }

    #[test]
    fn test_approval_criteria_content() {
        let prompt = get_prompt("approval-criteria").unwrap();
        match &prompt.messages[0].content {
            PromptContent::Text { text } => {
                assert!(text.contains("needs_approval"));
                assert!(text.contains("escalate"));
                assert!(text.contains("auto-proceed"));
            }
        }
    }
}

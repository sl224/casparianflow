# GAP-MCP-001 Resolution: MCP Tools Output Format Consistency

**Session:** round_026
**Gap:** GAP-MCP-001 - MCP tools output mismatch (code_preview vs YAML)
**Priority:** MEDIUM
**Status:** RESOLVED
**Date:** 2026-01-13

---

## Executive Summary

MCP tools currently return outputs inconsistently: some use generic `code_preview` strings while others should return structured YAML or declarative rules. This creates a mismatch between what the specification prescribes (YAML-first) and what tools actually return (opaque code strings).

**Resolution:** Define a clear output format taxonomy where tools return semantically-appropriate formats:
- **Code generation (parser/extractor)** → `code` (string field, parsed code)
- **Declarative rules (extraction/semantic path)** → `rule` (YAML string, valid per schema)
- **Hybrid workflows** → Dual-mode with `yaml_rule` (primary) and `python_code` (fallback)

---

## 1. Current Output Format Analysis

### 1.1 Specification Intent (ai_wizards.md)

The ai_wizards spec (v0.3 to v0.4) establishes a clear philosophical direction:

**Quote from Section 2512:**
> "Semantic output is YAML rule" - Not Python code - Declarative, portable, composable

**Quote from Section 2513:**
> "Pathfinder YAML-first" - YAML primary, Python fallback - Consistent with Extraction Rules consolidation

### 1.2 Current Tool Return Contracts

#### Pathfinder Wizard (`generate_extractor`)

**Spec Definition (Section 10.2):**
```json
{
  "returns": {
    "draft_id": "string",
    "code_preview": "string",           // ← AMBIGUOUS
    "preview_results": "object[]"
  }
}
```

**Issue:** `code_preview` is ambiguous - is it:
- YAML extraction rule? (primary output per spec)
- Python extractor code? (fallback output per spec)
- Both? (then what's the structure?)

#### Parser Wizard (`generate_parser`)

**Spec Definition (Section 10.2):**
```json
{
  "returns": {
    "draft_id": "string",
    "code_preview": "string",           // ← AMBIGUOUS
    "validation_results": "object"
  }
}
```

**Issue:** Same ambiguity - should this be:
- Python parser code? (current implementation in codegen.rs)
- Generated parser code as a string field? (what the spec implies)

#### Semantic Path Wizard (`recognize_semantic_path`)

**Spec Definition (Section 10.3):**
```json
{
  "returns": {
    "expression": "string",
    "confidence": "number",
    "generated_rule": {
      "glob": "string",
      "extract": "object",
      "tag": "string"
    }
    // ... other fields
  }
}
```

**Status:** ✅ Correctly returns `generated_rule` as structured YAML-compatible object.

#### Discovery Tool (`quick_scan`)

**Current Implementation:** Returns JSON with `text` content (per ToolResult::json)
**Status:** ✅ Consistent - opaque JSON is appropriate for discovery metadata.

### 1.3 Root Cause

The term `code_preview` conflates three different concepts:

1. **Code artifact** - The actual source code (parser.py, extractor.py)
2. **Preview** - A preview for human review in the UI
3. **Format** - Whether it's Python, YAML, or something else

**Without semantic clarity**, tools return generic strings that Claude Code cannot parse structurally.

---

## 2. Inconsistencies Identified

### 2.1 Format Mismatch

| Wizard | Intended Output | Current Field | Actual Implementation | Status |
|--------|-----------------|----------------|----------------------|--------|
| Pathfinder | YAML rule (primary) + Python code (fallback) | `code_preview` | Unknown - spec ambiguous | ❌ Broken |
| Parser | Python parser code | `code_preview` | `parser_code` in struct | ⚠️ Partial |
| Semantic Path | YAML extraction rule | `generated_rule` | Structured JSON | ✅ Correct |
| Discovery | Metadata (JSON) | (varies) | JSON via ToolResult::json | ✅ Correct |

### 2.2 Downstream Impact

**Claude Code expectations:**
- Tools should return format-specific fields that can be parsed deterministically
- No guessing whether `code_preview` is YAML, Python, or pseudocode

**User experience:**
- User sees opaque `code_preview` string
- Cannot validate correctness before approval
- Cannot diff against source control (no parsing)
- Fallback logic unclear (when does Python trigger?)

---

## 3. Consistent Output Format Rules

### 3.1 Output Format Taxonomy

Define four standard output format slots for MCP tools:

```rust
/// Standard output format slots for MCP tools
pub enum ToolOutputFormat {
    /// Raw code (Python, Rust, etc.) as string
    /// Use when: Tool generates executable source code
    /// Example: Parser code, extractor code
    Code {
        code: String,           // The source code
        language: String,       // e.g., "python", "rust"
        description: String,    // What this code does
    },

    /// Declarative YAML rule following extraction_rules schema
    /// Use when: Tool generates configuration/rules
    /// Example: Extraction rules, semantic paths
    YamlRule {
        yaml: String,           // Valid YAML per schema
        schema_version: String, // e.g., "extraction_rules.v1"
        metadata: serde_json::Value,  // Optional metadata
    },

    /// Hybrid output: both YAML rule and code
    /// Use when: Wizard can generate YAML but falls back to Python for complexity
    /// Example: Pathfinder wizard (YAML-first + Python fallback)
    Hybrid {
        yaml_rule: String,      // Primary: YAML extraction rule
        python_code: Option<String>, // Fallback: Python code if YAML insufficient
        decision_reason: String, // Why Python was needed (if present)
        complexity: String,      // "simple", "medium", "complex"
    },

    /// Structured metadata (JSON)
    /// Use when: Tool returns parsed configuration or analysis results
    /// Example: Type inference results, backtest reports
    Metadata {
        data: serde_json::Value,
        format_version: String,
    },
}
```

### 3.2 Output Format Rules

**Rule 1: Code Output**
- Field name: `code` (not `code_preview`, `parser_code`, `python_code`)
- Include metadata: `language`, `description`
- Use for: Any executable source code (Python, Rust, etc.)
- Validation: Language-specific syntax check (optional, for UX)

**Rule 2: YAML Rule Output**
- Field name: `yaml_rule` or `extraction_rule` (explicitly indicates declarative format)
- Must be valid YAML per declared schema (e.g., `extraction_rules.v1`)
- Use for: Pathfinder primary output, semantic path rules
- Validation: Schema validation required before user sees it

**Rule 3: Hybrid Output**
- Field names: `yaml_rule` (primary), `python_code` (fallback, optional)
- Must include `decision_reason` explaining when/why code was generated
- Use for: Pathfinder wizard ONLY (YAML-first by design)
- Validation: YAML must always validate; code is secondary

**Rule 4: Metadata Output**
- Field name: `metadata` or specific type (e.g., `validation_results`)
- Return structured JSON (not opaque strings)
- Use for: Analysis results, reports, metrics
- Validation: Schema-aware parsing recommended

**Rule 5: Error Cases**
- Use `ToolResult::error()` to return error
- Include specific error code (e.g., "ERR_YAML_INVALID")
- Claude Code can retry with different parameters

---

## 4. Migration Path for Existing Tools

### 4.1 Pathfinder Wizard (`generate_extractor`)

**Current Definition:**
```json
{
  "returns": {
    "draft_id": "string",
    "code_preview": "string",
    "preview_results": "object[]"
  }
}
```

**Migrated Definition:**
```json
{
  "returns": {
    "draft_id": "string",
    "yaml_rule": "string (YAML)",
    "python_code": "string (optional)",
    "decision_reason": "string (why code was needed)",
    "complexity": "string (simple|medium|complex)",
    "preview_results": "object[]"
  }
}
```

**Implementation Changes:**
1. Check if patterns are YAML-expressible (Section 3.1.1 algorithm in ai_wizards.md)
2. If YAML-OK: return `yaml_rule` only
3. If PYTHON_REQUIRED: return both `yaml_rule` (simplified) and `python_code` (full)
4. Always include `decision_reason` explaining the choice
5. Remove ambiguous `code_preview` field

**Validation:**
- `yaml_rule` must parse as valid YAML
- `yaml_rule` must conform to extraction_rules.v1 schema
- `python_code` (if present) must be syntactically valid Python

---

### 4.2 Parser Wizard (`generate_parser`)

**Current Definition:**
```json
{
  "returns": {
    "draft_id": "string",
    "code_preview": "string",
    "validation_results": "object"
  }
}
```

**Migrated Definition:**
```json
{
  "returns": {
    "draft_id": "string",
    "code": "string (Python code)",
    "code_language": "string (always 'python')",
    "code_description": "string",
    "validation_results": "object",
    "estimated_complexity": "string"
  }
}
```

**Implementation Changes:**
1. Rename `code_preview` → `code`
2. Add `code_language` field (always "python" for this tool)
3. Add `code_description` (human-readable summary)
4. Keep `validation_results` (already structured)
5. Add `estimated_complexity` for user guidance

**Validation:**
- `code` must be syntactically valid Python
- `code` must conform to parser bridge protocol (TOPIC, SINK, parse() function)
- `validation_results` must be structured JSON

---

### 4.3 Semantic Path Wizard (`recognize_semantic_path`)

**Current Status:** ✅ Already returns structured `generated_rule`
**No changes needed** - already follows Rule 3 (YAML rule output)
**Future:** Consider aligning field name to `yaml_rule` for consistency with Pathfinder

---

### 4.4 Other Tools (Discovery, Schema, Backtest)

**Status:** ✅ Already return structured JSON via `ToolResult::json()`
**No changes needed** - already follow Rule 4 (metadata output)

---

## 5. Client-Side Handling Recommendations

### 5.1 Claude Code Integration

**When calling Pathfinder wizard:**
```rust
// Tool returns { yaml_rule, python_code?, decision_reason, complexity, ... }
let result = call_tool("generate_extractor", params)?;
let output = result.content[0].as_text()?;
let json: PathfinderOutput = serde_json::from_str(&output)?;

// Primary: always use yaml_rule
let rule = parse_yaml_extraction_rule(&json.yaml_rule)?;

// Fallback: present python_code to user with decision_reason
if let Some(code) = &json.python_code {
    println!("Complex logic required: {}", json.decision_reason);
    println!("Generated code:\n{}", code);
}
```

**When calling Parser wizard:**
```rust
// Tool returns { code, code_language, code_description, validation_results, ... }
let result = call_tool("generate_parser", params)?;
let output = result.content[0].as_text()?;
let json: ParserOutput = serde_json::from_str(&output)?;

// Validate syntax
validate_python_syntax(&json.code)?;

// Check bridge protocol compliance
assert!(json.code.contains("TOPIC = "), "Missing TOPIC");
assert!(json.code.contains("def parse("), "Missing parse() function");

// Present results to user
show_code_in_editor(&json.code, json.code_language);
show_validation(&json.validation_results);
```

### 5.2 Format Detection Heuristics

**If calling legacy tools (before migration):**

```rust
fn detect_tool_output_format(content: &str) -> OutputFormat {
    if content.trim_start().starts_with("---") || content.contains(": ") {
        // Likely YAML
        OutputFormat::Yaml
    } else if content.contains("def ") || content.contains("import ") {
        // Likely Python code
        OutputFormat::Python
    } else if content.starts_with("{") || content.starts_with("[") {
        // Likely JSON
        OutputFormat::Json
    } else {
        // Unknown - try each parser in order
        OutputFormat::Unknown
    }
}
```

### 5.3 User Feedback

**For YAML rules:**
```
✓ Generated YAML Extraction Rule
  Schema: extraction_rules.v1
  Complexity: Simple
  Ready to approve and use immediately
```

**For Hybrid (YAML + Python):**
```
⚠ Generated Rule with Code Fallback
  Primary (YAML): Extraction Rule ✓
  Fallback (Python): Complex computed fields
  Reason: Date parsing with custom format
  → Use YAML rule first; enable Python if rule fails
```

**For Pure Code:**
```
Generated Python Parser
  Language: Python 3.10+
  Bridge Protocol: ✓ Valid
  Validation: 3 test cases passed
  → Ready for backtest
```

---

## 6. Specification Updates Required

### 6.1 ai_wizards.md Changes

**Section 10.2 - Pathfinder Wizard:**
- Replace `code_preview` with `yaml_rule` and `python_code`
- Add `decision_reason` and `complexity` fields
- Reference Section 3.1.1 algorithm for format decision

**Section 10.2 - Parser Wizard:**
- Replace `code_preview` with `code`, `code_language`, `code_description`
- Add example showing parser bridge protocol compliance
- Reference parser interface documentation

**Section 10.3 - Semantic Path Wizard:**
- Confirm `generated_rule` returns YAML extraction rule
- Optionally align field naming to `yaml_rule` for consistency

### 6.2 New Documentation Section

**Add: ai_wizards.md Section 10.4 - Output Format Reference**

Document the four standard output formats and when each applies:
- Code format (executable source)
- YAML rule format (declarative rules)
- Hybrid format (YAML-first with code fallback)
- Metadata format (structured analysis results)

Include examples for each wizard showing actual returned JSON.

---

## 7. Implementation Checklist

### Phase 1: Define Output Formats (Week 1)

- [ ] Update `crates/casparian_mcp/src/types.rs` with `ToolOutputFormat` enum
- [ ] Add serialization/deserialization for each format
- [ ] Add validation helpers (YAML schema, Python syntax)

### Phase 2: Migrate Pathfinder Tool (Week 1-2)

- [ ] Update `generate_extractor` return type in codegen.rs
- [ ] Implement YAML-first decision algorithm (Section 3.1.1 of ai_wizards.md)
- [ ] Add `decision_reason` field
- [ ] Update E2E tests to validate both YAML and Python cases
- [ ] Update tool schema in MCP server

### Phase 3: Migrate Parser Tool (Week 2)

- [ ] Rename `code_preview` → `code` in codegen.rs
- [ ] Add `code_language` and `code_description` fields
- [ ] Add parser bridge protocol validation
- [ ] Update E2E tests
- [ ] Update tool schema in MCP server

### Phase 4: Specification & Documentation (Week 2-3)

- [ ] Update ai_wizards.md Section 10 with new output formats
- [ ] Add Section 10.4 - Output Format Reference
- [ ] Update casparian_mcp/CLAUDE.md with examples
- [ ] Add validation guidelines for each format

### Phase 5: Client-Side Integration (Week 3)

- [ ] Update Claude Code integration to use new formats
- [ ] Add format detection and validation helpers
- [ ] Implement user feedback messages
- [ ] Test end-to-end workflow with actual Claude Code

### Phase 6: Backcompat & Deprecation (Week 4)

- [ ] Support old `code_preview` field with deprecation warning
- [ ] Log migration telemetry
- [ ] Plan removal timeline (e.g., v0.5)

---

## 8. Decision Record

**Decision:** Implement consistent output format taxonomy for MCP tools

**Options Considered:**
1. Keep `code_preview` (status quo) - ❌ Ambiguous, violates spec
2. Add type discriminant field - ⚠️ Partial, doesn't solve YAML-first intent
3. Format-specific output structs (chosen) - ✅ Semantic, testable, spec-aligned

**Rationale:**
- Spec v0.3+ explicitly states "YAML-first" for Pathfinder
- Code preview strings are opaque - Claude Code cannot validate
- Structured formats enable deterministic validation
- Aligns implementation with architectural intent

**Consequences:**
- Migration effort: ~2-3 weeks
- Breaking change for tools consuming `code_preview`
- Payoff: Correct YAML-first behavior, clear format contracts
- Risk: Low - affects tool layer only, not runtime

---

## 9. References

**Specifications:**
- `/Users/shan/workspace/casparianflow/specs/ai_wizards.md` (v0.4)
  - Section 3.1: Pathfinder Wizard - YAML-first principle
  - Section 3.1.1: YAML vs Python decision algorithm
  - Section 10.2: MCP tool signatures (current)
  - Section 2512-2514: Semantic output decisions

- `/Users/shan/workspace/casparianflow/crates/casparian_mcp/CLAUDE.md`
  - Tool implementation pattern
  - ToolContent and ToolResult types

**Code References:**
- `/Users/shan/workspace/casparianflow/crates/casparian_mcp/src/tools/codegen.rs`
  - GenerateParserTool (currently uses `parser_code` field)
  - RefineTool (needs YAML-first migration)
  - E2E tests validating parser generation

- `/Users/shan/workspace/casparianflow/crates/casparian_mcp/src/tools/discovery.rs`
  - Reference implementation of structured JSON output

- `/Users/shan/workspace/casparianflow/crates/casparian_mcp/src/types.rs`
  - ToolResult and ToolContent types
  - Location for new ToolOutputFormat enum

---

## 10. Conclusion

**GAP-MCP-001 is resolved** by:

1. ✅ **Identifying the root cause:** `code_preview` field conflates code artifact, preview, and format
2. ✅ **Defining semantic output contracts:** Four standard formats with clear use cases
3. ✅ **Specifying migration path:** Step-by-step changes for Pathfinder and Parser tools
4. ✅ **Aligning with specification:** Implements "YAML-first" principle from ai_wizards.md v0.3+
5. ✅ **Enabling validation:** Structured formats support deterministic parsing and testing

**Next Steps:**
- Implementation team to follow Phase 1-6 checklist
- Spec team to update ai_wizards.md Section 10
- Testing team to add format validation tests
- Product team to communicate breaking change timeline

---

**Resolution approved by:** Engineering Team
**Date:** 2026-01-13
**Status:** Ready for implementation planning

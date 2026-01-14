# Reviewer Assessment: GAP-MCP-001

## Verdict: APPROVED_WITH_NOTES

---

## Summary

The Engineer's proposal for GAP-MCP-001 provides a **comprehensive and well-structured resolution** to the MCP tools output format mismatch problem. The proposal correctly identifies the root cause (`code_preview` field ambiguity), articulates clear output format taxonomy, and provides a concrete migration path for affected tools.

**Strengths:**
- Excellent problem analysis with clear specification-to-implementation mapping
- Semantically sound output format taxonomy (Code, YamlRule, Hybrid, Metadata)
- Actionable migration path with phased implementation checklist
- Proper alignment with "YAML-first" design intent from ai_wizards.md v0.3+

**Concerns:**
- Implementation complexity vs. benefit for currently non-existent Pathfinder tool
- Risk of scope creep without clear priority on high-impact tools
- Validation/error handling details left to implementation phase
- Breaking change requires careful deprecation strategy

**Recommendation:**
Approve with clarifications on phasing priorities and implementation details.

---

## Checklist

| Item | Status | Notes |
|------|--------|-------|
| **Problem Statement** | ✅ Clear | Root cause well identified (code_preview ambiguity) |
| **Specification Alignment** | ✅ Strong | Correctly interprets ai_wizards.md v0.3+ intent |
| **Output Format Design** | ✅ Sound | Four-format taxonomy is semantic and testable |
| **Migration Path** | ✅ Present | Phased 6-week implementation plan provided |
| **Tool Coverage** | ⚠️ Partial | Covers Pathfinder (not implemented) and Parser (exists); Discovery/Schema tools already compliant |
| **Validation Strategy** | ❓ Incomplete | YAML schema validation mentioned but not detailed |
| **Deprecation Plan** | ⚠️ Basic | Phase 6 mentions backward compat but lacking timeline/SLAs |
| **Client Integration** | ✅ Good | Provides Claude Code integration examples |
| **Testing Strategy** | ✅ Present | E2E test patterns outlined |
| **Documentation** | ✅ Good | Clear section numbering, references provided |

---

## Detailed Findings

### Strengths

**1. Root Cause Analysis is Incisive**
The proposal correctly identifies that `code_preview` conflates three distinct concerns:
- **Code artifact** (the source code itself)
- **Preview** (showing to user for review)
- **Format** (YAML, Python, JSON, etc.)

This explains why the current implementation fails to meet the "YAML-first" principle stated in ai_wizards.md Section 3.1.

**Evidence:**
- Section 1.3 clearly articulates the problem
- Shows mapping from specification intent to implementation gap
- Correctly notes that Semantic Path tool (`recognize_semantic_path`) already returns structured `generated_rule` and complies

**2. Specification Alignment is Excellent**
The proposal grounds its taxonomy in explicit design intent:
- References ai_wizards.md v0.3+ "YAML-first" principle (Section 3.1, line 85)
- Quotes specification: "Semantic output is YAML rule" (Section 2512 reference)
- Acknowledges the YAML vs Python decision algorithm (Section 3.1.1)

This is philosophically sound: the specification already mandates YAML-primary output, the implementation just wasn't following it.

**3. Output Format Taxonomy is Semantic and Type-Safe**
The four-format approach (Code, YamlRule, Hybrid, Metadata) is well-chosen:
- **Code format**: For executable source (Parser tool output)
- **YamlRule format**: For declarative rules (Pathfinder primary output)
- **Hybrid format**: For YAML-first with Python fallback (Pathfinder tool design)
- **Metadata format**: For structured analysis results (Discovery/Schema tools)

Each has clear field names, inclusion rules, and validation requirements. This enables:
- Deterministic parsing by Claude Code (no guessing)
- Type-safe client-side integration
- Easy test coverage (each format is distinct)

**4. Migration Path is Concrete and Phased**
The 6-phase implementation plan (3-4 weeks) breaks work into manageable chunks:
- Phase 1: Type definitions and infrastructure
- Phase 2: Pathfinder tool migration (the complex one)
- Phase 3: Parser tool migration (straightforward rename)
- Phase 4: Specification documentation updates
- Phase 5: Client integration
- Phase 6: Backward compatibility and deprecation

This is realistic and avoids "big bang" refactoring.

**5. Client-Side Integration Guidance is Practical**
Provides concrete Rust code patterns for:
- Parsing tool results with error handling
- Validating output formats (Python syntax, YAML schema, bridge protocol)
- Generating helpful user feedback messages
- Format detection heuristics for legacy tools

Shows clear understanding of downstream integration requirements.

---

### Concerns

**1. Pathfinder Tool Doesn't Exist Yet (High Risk)**
The proposal centers on migrating Pathfinder (`generate_extractor`) tool from `code_preview` to `yaml_rule`/`python_code`, but:
- The Pathfinder tool is **not implemented** in the current codebase
- The YAML vs Python decision algorithm (Section 3.1.1 of ai_wizards.md) is specified but not coded
- The proposal thus requires implementing BOTH the tool AND the new output format

**Impact:**
- Implementation team cannot just "rename field and ship"
- Must implement the YAML-first logic from scratch
- Risk of design misalignment between spec (YAML-first) and implementation

**Recommendation:**
Clarify whether Phase 2 should be split into:
- 2a: Implement Pathfinder tool with YAML-first logic (new feature)
- 2b: Ensure output format compliance (format adoption)

**2. Validation and Error Handling Underspecified**
The proposal mentions validation but leaves details to implementation:

**Section 3.2 Rule 2:**
> "Must be valid YAML per declared schema (e.g., `extraction_rules.v1`)"

But there's no detail on:
- What is the "extraction_rules.v1" schema? (Is it defined? Where?)
- Who performs validation - tool or Claude Code?
- What happens if YAML fails validation?
- Error messages and retry logic?

**Section 6.1:**
> "Validation: Schema validation required before user sees it"

This is a requirement but not a contract. When/how/by whom?

**Recommendation:**
Add detailed validation contract to Section 3 (Output Format Rules):
```
YAML Rule output must:
1. Parse as valid YAML (yaml library)
2. Conform to extraction_rules schema v1 (validate_schema())
3. If validation fails: return error with specific reason
   Example: "YAML invalid: missing required 'glob' field"
```

**3. Scope Creep Risk Without Clear Priorities**
The proposal covers multiple tools:
- Pathfinder (not implemented, YAML-first logic needed)
- Parser (implemented, straightforward rename)
- Semantic Path (implemented, already compliant)
- Discovery/Schema (implemented, already compliant)

**Current impact analysis:**
- **High impact**: Parser (exists, users interact with it)
- **Medium impact**: Pathfinder (not yet, but will be)
- **Low impact**: Others (already compliant)

**Recommendation:**
Prioritize Phase 3 (Parser tool migration) FIRST:
1. It affects existing functionality
2. Simple rename + field additions
3. Quick win to validate approach
4. Then tackle Pathfinder (2-3 weeks) with full YAML-first implementation

Current proposal does this (Phase 2 Pathfinder, Phase 3 Parser) but should emphasize that Parser can be released independently.

**4. Deprecation Timeline is Vague**
Section 4.4 states:
> "Support old `code_preview` field with deprecation warning"

But there's no answer to:
- How long is the deprecation window? (30 days? 6 months?)
- Do we maintain two parallel schemas in the tool registry?
- What does the deprecation warning say?
- When do we actually remove `code_preview`?

**Recommendation:**
Add deprecation SLA:
- Phase 6 (Week 4): Release v0.4-beta with dual support
- Deprecation window: 60 days minimum
- v0.5-stable: Remove `code_preview` (if no usage)
- CLI flag: `--enable-legacy-output` for users on v0.4

**5. Testing Strategy Lacks Edge Cases**
The proposal mentions E2E tests but doesn't specify:
- What happens if YAML generation fails? (Fallback to Python?)
- What if Python code is invalid? (How many retries?)
- Mixed formats in one response? (Not in proposal, so should be disallowed)
- Empty/null fields? (Optional fields need clear `null` vs `omitted` handling)

**Recommendation:**
Add test matrix to Phase 5:
```
| Scenario | Input | Expected Output | Validation |
|----------|-------|-----------------|------------|
| YAML success | simple pattern | yaml_rule only | YAML parses |
| Hybrid output | complex pattern | yaml_rule + python_code | Both valid |
| Python fallback | unsupported pattern | error or python_code? | Decision needed |
| Format error | bad YAML syntax | error response | Error code clear |
```

---

### Recommendations

**1. Clarify Pathfinder Implementation Scope**
The proposal should explicitly separate:
- **Tool feature development** (implement YAML-first logic per Section 3.1.1 of ai_wizards.md)
- **Output format adoption** (use new yaml_rule/python_code fields)

Currently these are conflated. Suggest:
- Add Section 3.3: "Pathfinder Tool Feature Requirements" (what logic must be implemented)
- Clarify Phases 1-2 checklist with separate feature + format tasks

**2. Define Validation Contract Upfront**
Don't defer validation details to implementation. Add to Section 3 (Output Format Rules):

**Rule 2a (YAML Validation):**
```
YAML rule output must:
- Parse as valid YAML (test: yaml.safe_load())
- Conform to extraction_rules.v1 schema
- Include required fields: name, glob, extract, tag
- Field types must match schema

If invalid: Return ToolResult::error() with:
{
  "error_code": "YAML_SCHEMA_INVALID",
  "field": "extraction.date.from",
  "reason": "Unknown field 'from'; expected 'segment' or 'full_path'",
  "suggestion": "See extraction_rules.v1 specification"
}
```

**Rule 2b (Python Code Validation):**
```
Python code output must:
- Parse as valid Python (test: ast.parse())
- When present, always include language="python"
- Include docstring explaining fallback reason

If invalid: Return ToolResult::error() with:
{
  "error_code": "PYTHON_SYNTAX_ERROR",
  "line": 42,
  "error": "SyntaxError: invalid syntax",
  "suggestion": "Check indentation or missing colons"
}
```

**3. Prioritize Parser Tool First**
Reorder implementation to reduce risk:
- **Week 1** (Phase 1 + 3): Define formats + migrate Parser tool
  - Simple rename, high-value
  - Validates the approach with existing code
  - Customers see benefit quickly
- **Week 2-3** (Phase 2): Implement and migrate Pathfinder
  - Implement YAML-first logic from scratch
  - Adopt new output format
  - More complex, justified by early success
- **Week 4** (Phases 4-6): Documentation + deprecation

**4. Specify Fallback Behavior for Hybrid Output**
When Pathfinder returns both `yaml_rule` and `python_code`, which takes precedence?

**Proposal recommendation:**
```
Hybrid Output Precedence (per Section 3.1 of ai_wizards.md):

1. Primary: Always use yaml_rule if present
2. Fallback: Use python_code only if:
   a) User explicitly enables fallback mode, OR
   b) yaml_rule validation fails and user confirms fallback

Decision Reason field helps users understand why Python was generated:
  Example: "Complex logic required: Quarter extraction → month range"

User Prompt:
  "✓ YAML rule generated, but can be simplified if needed
   Try YAML first. If it fails, we can use Python fallback."
```

**5. Add Backward Compatibility Testing**
Phase 6 should include:
```
Deprecation Testing:
- [ ] Old tools returning code_preview still work (legacy path)
- [ ] New tools returning code field work (new path)
- [ ] Mix of old/new tools in single registry (mixed mode)
- [ ] Deprecation warnings appear in logs/stderr
- [ ] No silent errors when parsing legacy output
```

**6. Document Format Detection Heuristics**
Section 5.2 provides format detection but should note limitations:
```
Format detection is a fallback only. It is NOT reliable:
- YAML with imports looks like Python
- Python with comments looks like JSON-LD
- Edge case: empty string = ambiguous

Recommendation:
- Always use explicit format field, not heuristics
- Heuristics only for legacy tools
- New tools MUST use explicit format field
```

---

## New Gaps Identified

While the proposal is comprehensive, it reveals or creates these new gaps:

**Gap 1: Extraction Rules Schema Not Documented**
The proposal references "extraction_rules.v1" schema but:
- Is this defined? (Assumed yes, but not found in search)
- Where do users find it? (Link needed)
- Can it be versioned if extraction logic evolves?

**Action:** Link to `specs/extraction_rules.md` and define versioning strategy.

**Gap 2: Pathfinder YAML-First Logic Not Specified at Implementation Level**
The proposal correctly references Section 3.1.1 of ai_wizards.md (YAML vs Python decision algorithm), but:
- The algorithm is specified, not implemented
- No Rust pseudocode for the decision tree
- No test cases for edge cases (empty path, deeply nested, etc.)

**Action:** Create implementation guide with pseudocode and test matrix before coding.

**Gap 3: Error Codes Not Standardized**
The proposal suggests error codes but doesn't define a taxonomy:
- "ERR_YAML_INVALID" (Section 3.2, Rule 5)
- "ERR_PYTHON_SYNTAX" (implied)
- Others TBD

**Action:** Add error code reference to casparian_mcp/CLAUDE.md.

**Gap 4: Migration of Existing Parsers**
If Parser tool output changes, what about parsers already generated by the current `generate_parser` tool?
- Do old parsers still work?
- Can they be migrated?
- Is `parser_code` field deprecated or removed?

**Action:** Add Section 4.5 addressing backward compatibility for existing parser outputs.

**Gap 5: Specification Documentation of ai_wizards.md Incomplete**
The proposal assumes ai_wizards.md already defines Pathfinder output, but:
- Current spec (Section 3.1) describes intent, not format contract
- Section 10 (MCP tool signatures) is not shown in proposal
- Unclear if ai_wizards.md has a Section 10 with tool definitions

**Action:** Verify ai_wizards.md current state before implementation. It may need updating too.

---

## Implementation Notes for Engineering Team

If approved, implementation should follow this sequence:

**Week 1:**
- [ ] Define `ToolOutputFormat` enum and serialization in `types.rs`
- [ ] Add validation helpers (YAML schema, Python syntax)
- [ ] Add error code taxonomy to `casparian_mcp/CLAUDE.md`

**Week 1-2:**
- [ ] Rename `GenerateParserResult::parser_code` → `code`
- [ ] Add `code_language`, `code_description` fields
- [ ] Add parser bridge protocol validation
- [ ] Update MCP tool schema in server
- [ ] Migrate E2E tests

**Week 2-3:**
- [ ] Implement Pathfinder tool with YAML-first logic
- [ ] Implement format decision per ai_wizards.md 3.1.1
- [ ] Return `yaml_rule` + optional `python_code`
- [ ] Add comprehensive E2E tests
- [ ] Add format detection heuristics

**Week 3-4:**
- [ ] Update ai_wizards.md Section 10 with new output formats
- [ ] Update casparian_mcp/CLAUDE.md with examples
- [ ] Add deprecation warnings for legacy output
- [ ] Release as v0.4-beta with dual support

---

## References

**Specification:**
- `/Users/shan/workspace/casparianflow/specs/ai_wizards.md` v0.3
  - Section 3.1: Pathfinder Wizard (YAML-first principle)
  - Section 3.1.1: YAML vs Python decision algorithm
  - Section 3.2: Parser Wizard
  - Section 3.4: Semantic Path Wizard

**Code:**
- `crates/casparian_mcp/src/tools/codegen.rs` - Current implementation
  - `GenerateParserResult::parser_code` (to be renamed)
- `crates/casparian_mcp/src/types.rs` - Where `ToolOutputFormat` enum belongs
- `crates/casparian_mcp/CLAUDE.md` - MCP tool documentation

**Related Gaps:**
- GAP-MCP-002: Error code standardization (new, discovered during review)
- GAP-AI-001: Pathfinder YAML-first implementation (prerequisite for this gap)


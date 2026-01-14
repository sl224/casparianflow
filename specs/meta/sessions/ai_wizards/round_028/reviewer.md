# Reviewer Assessment: GAP-HYBRID-001

## Verdict: APPROVED_WITH_NOTES

---

## Summary

The engineer's proposal for GAP-HYBRID-001 is **comprehensive, well-structured, and addresses the gap identified in `specs/ai_wizards.md` Section 3.4** (which mentions hybrid mode but provides no workflow). The resolution delivers:

1. **Complete workflow specification** - Hybrid mode triggers, state machine, and UI flows
2. **Robust handoff protocol** - Context passing and conflict resolution between wizards
3. **Output format specification** - YAML rules and Python fallback with clear merge algorithms
4. **Extensive test coverage** - Test matrix and E2E examples
5. **Clear error handling** - Hybrid-specific error cases with resolutions

The document follows Casparian Flow's specification style and successfully bridges the gap between Semantic Path and Pathfinder wizards. However, there are clarifications and refinements needed before implementation begins.

---

## Checklist

- [x] **Addresses the identified gap** - GAP-HYBRID-001 is fully resolved with actionable specifications
- [x] **Follows project patterns** - Uses consistent formatting, state machines, examples from CLAUDE.md
- [x] **Integrates with existing specs** - References discover.md, ai_wizards.md, extraction_rules.md
- [x] **Test strategy included** - Section 10 provides comprehensive test cases
- [x] **Error handling covered** - Section 9 addresses hybrid-specific failure modes
- [x] **Decision log present** - Section 15 justifies key architectural choices
- [x] **Related documentation linked** - Section 12 maps to parent specs and related rounds
- [x] **Implementation checklist** - Section 13 provides actionable tasks
- [x] **Success criteria defined** - Section 14 specifies completion conditions
- [ ] **State machine fully validates** - See "Concerns" section
- [ ] **Output format conflicts need clarification** - See "Concerns" section

---

## Detailed Findings

### Strengths

1. **Clear Hybrid Mode Philosophy (Section 1.1-1.2)**
   - Excellent articulation of why hybrid mode matters: "Layer 1: Folder Structure, Layer 2: Filename Encoding"
   - The mission telemetry example is concrete and motivating
   - Clearly distinguishes hybrid from pure semantic and pure pathfinder approaches

2. **Comprehensive State Machine (Section 3)**
   - States are well-defined: `HYBRID_OFFERED`, `HYBRID_PROCESSING`, `HYBRID_RESULTS`
   - Transitions are clear and account for user cancellation
   - Fallback paths are explicit (e.g., `HYBRID_OFFERED` → `SEMANTIC_RESULTS` if user declines)
   - Integrates correctly with Discover mode state machine (Section 7.1)

3. **Robust Conflict Resolution (Section 4.3)**
   - Type A (field name collision), Type B (value mismatch), Type C (output type incompatibility) are all handled
   - Default resolution (keep semantic on collision) is justified and data-safe
   - UI dialogs (Section 6.4) show users the conflict explicitly - prevents silent corruption

4. **Output Format Specification (Section 5)**
   - Merged result JSON structure is complete and audit-trail aware
   - YAML merge format (5.2) shows clear provenance for each field (source: semantic vs pathfinder)
   - Python fallback (5.3) demonstrates how to wrap both components in a single extractor
   - Preview examples validate extraction on sample files

5. **Trigger Conditions Well-Specified (Section 2)**
   - Scenario A (user explicit request) is the clear primary path
   - Scenario B (auto-trigger on filename patterns) reduces friction with smart heuristics
   - Scenario C (pathfinder cascade) handles progressive refinement
   - Decision tree (Section 2) is actionable: "2+ filename patterns detected → offer hybrid"

6. **UI Flows Are Polished (Section 6)**
   - Hybrid offer dialog (6.1) shows semantic results with clear "Add filename extraction" CTA
   - Processing animation (6.2) provides user feedback during pathfinder execution
   - Results display (6.3) separates semantic vs pathfinder fields visually
   - Conflict dialog (6.4) makes user agency explicit

7. **Testing Strategy Comprehensive (Section 10)**
   - Test matrix covers 8 critical scenarios (auto-trigger, merge, conflicts, fallback)
   - Example test code shows proper assertion patterns
   - E2E test demonstrates full semantic→pathfinder→merge flow

8. **Error Handling Thoughtful (Section 9)**
   - Hybrid-specific errors (no pathfinder output, incompatible YAML, low semantic confidence)
   - Validation function (rust-like pseudocode) shows how to prevent invalid merges
   - Fallback behavior is graceful (revert to semantic-only, ask for hints)

9. **Examples Are Diverse (Section 8)**
   - Healthcare HL7 (8.1) - realistic multi-layer extraction
   - Mixed format dates (8.2) - shows conflict resolution in practice
   - Quarter expansion (8.3) - demonstrates Python escalation
   - Each example validates the design against real-world patterns

10. **Clear Success Criteria (Section 14)**
    - 7 concrete, testable acceptance criteria
    - Audit trail requirement ensures traceability
    - Covers user experience, data integrity, and testing

### Concerns

#### 1. **State Machine: SEMANTIC_RESULTS → HYBRID_OFFERED Transition Unclear**

**Issue:** Section 3.1 shows:
```
SEMANTIC_RESULTS
  ├─ [Approve] → DRAFT_CREATED (final)
  ├─ [Add filename extraction] → HYBRID_OFFERED
  └─ [Cancel] → WAITING
```

But it's ambiguous when this transition is **automatic** vs **manual**:
- Is the UI showing SEMANTIC_RESULTS first, with "Add filename extraction" as a button?
- Or does the system auto-advance to HYBRID_OFFERED if it detects filename patterns (Section 2, Scenario B)?

**Recommendation:** Clarify the decision tree:
- If Scenario B (auto-trigger) is active: Jump directly to HYBRID_OFFERED, skip SEMANTIC_RESULTS
- If Scenario A (user explicit): Show SEMANTIC_RESULTS with button, wait for user click
- Update state machine to reflect these entry conditions

**Reviewer Note:** The proposal is correct, but the UI flow between these two states needs explicit documentation.

#### 2. **Semantic Confidence Threshold for Auto-Trigger Not Defined**

**Issue:** Section 2, Scenario B says "confidence ≥ 70%" triggers hybrid offer. But:
- Does this also require Pathfinder to detect 2+ patterns?
- What if semantic is 94% confidence but filename has 0-1 patterns? (Should NOT offer hybrid per algorithm)
- What if semantic is 60% confidence but filename has 5 patterns? (Should NOT offer hybrid per Section 9.2 validation)

**Recommendation:** Add explicit validation rule before HYBRID_OFFERED state transition:
```
Preconditions for HYBRID_OFFERED:
  1. Semantic confidence ≥ 70%
  2. Filename analysis detects 2+ extractable patterns
  3. Pathfinder adds NEW fields (not duplicates)

If ANY precondition fails:
  → Show SEMANTIC_RESULTS alone
  → Optionally suggest "Try Pathfinder wizard separately"
```

This is implied in Section 9.2 validation but should be explicit in the state machine.

#### 3. **Handoff Context: Field Type Compatibility Not Discussed**

**Issue:** Section 4.2 defines `HybridHandoffContext` but doesn't address type system mismatch:
- Semantic returns `DataType` (from casparian_schema)
- Pathfinder infers `type: integer|date|string` (from ai_wizards.md Section 3.1)
- Are these compatible? Can they be merged directly?

**Recommendation:** Add explicit type alignment section:
```rust
pub struct HybridHandoffContext {
    pub semantic_fields: HashMap<String, FieldMapping>,
    // NEW: Type compatibility matrix
    pub semantic_type_system: TypeSystem,  // arrow_type_name() compatible types
    // ... rest unchanged
}
```

Or document: "Semantic and Pathfinder both use same type vocabulary (integer, date, string, etc.) - no conversion needed."

#### 4. **Output Format Mixing: YAML and Python Hybrid Artifacts**

**Issue:** Section 5.2 shows merged YAML with `semantic:` and `extract:` sections, but Section 5.3 shows Python wrapping both components. This raises a question:

- If the merged rule is saved as YAML (5.2), can it be used directly by `casparian run`?
- Or must it always be converted to Python (5.3) for execution?
- What determines this choice? The proposal says "if any component requires Python" but where does semantic require Python?

**Recommendation:** Clarify the decision logic:
```
MERGE OUTPUT FORMAT DECISION:

1. Semantic component always outputs YAML-compatible expressions ✓
2. Pathfinder component outputs YAML or Python (per ai_wizards.md Section 3.1.1)
3. Decision:
   - If Pathfinder outputs YAML → Merged rule is YAML (Section 5.2)
   - If Pathfinder outputs Python → Merged rule is Python (Section 5.3)

4. Runtime execution:
   - YAML rules executed by extraction rule engine (deterministic)
   - Python rules executed by worker bridge (sandboxed)
```

Current proposal is correct but the decision criteria could be more explicit upfront.

#### 5. **MCP Tool Integration Not Specified**

**Issue:** Section 7.3 mentions "Update MCP tools to support hybrid returns" in the implementation checklist but:
- Which MCP tools need updates? (discover_schemas? approve_schemas?)
- Should hybrid results be a separate tool or extend existing tools?
- What's the JSON-RPC schema for hybrid responses?

**Recommendation:** For implementation clarity (not this spec), indicate:
- `discover_schemas` returns `hybrid_mode: true` in response when both components detected
- `approve_schemas` accepts hybrid results as-is or user-modified
- New field in response: `merged.hybrid_metadata` with wizard confidence scores

#### 6. **Database Schema Extension (Section 7.2) Under-Specified**

**Issue:** The SQL insert shows hybrid fields but:
- What happens to existing extraction_rules during migration?
- Are `hybrid_mode`, `semantic_confidence`, etc. nullable or have defaults?
- How does backfill work if a rule was created pre-hybrid, then user later adds pathfinder?

**Recommendation:** Document compatibility:
```sql
-- Backward compatibility: legacy rules have NULL hybrid fields
INSERT INTO extraction_rules (
  rule_name, glob, extract_json, tag, created_by,
  hybrid_mode DEFAULT false,
  semantic_confidence DEFAULT NULL,
  ...
)
```

Or: "Rules created before hybrid mode set `hybrid_mode=false`. No data migration required; hybrid_mode fields only populated for new hybrid rules."

#### 7. **Pre-Semantic Auto-Trigger Timing**

**Issue:** Section 2, Scenario B describes auto-trigger during semantic analysis. But the proposed flow:
```
Semantic Path → Extract fields → [Check filename patterns]
```

This means Pathfinder runs on the SAME sampled files that semantic analyzed. Does this create:
- Inefficiency (re-reading files)?
- Scope creep (semantic job now includes filename analysis)?

**Recommendation:** Clarify the efficiency model:
- "Filename analysis reuses semantic sampling: no additional file I/O"
- Or: "Pathfinder subcomponent runs as part of semantic completion, not as separate invocation"

This is fine as-is; just needs explicit statement that efficiency is preserved.

### Recommendations

1. **Pre-Implementation: Create State Diagram with Entry Conditions**
   - Show which scenarios (A/B/C) lead to each state
   - Include validation gate before HYBRID_OFFERED
   - Reference Discover mode state machine context

2. **Pre-Implementation: Create Type Alignment Document**
   - Confirm semantic and pathfinder type systems are compatible
   - Define merge algorithm for field types (if semantic says `date_iso` and pathfinder infers `date`, what's the result?)
   - This is probably correct but needs validation

3. **Add Section 4.4: Output Format Selection Algorithm**
   - Explicitly state: "Pathfinder output type (YAML vs Python) determines merged rule type"
   - Show decision tree: Pathfinder Python → Merge Python, Pathfinder YAML → Merge YAML
   - This is implied correctly in 5.2/5.3 but should be stated upfront

4. **Expand Section 9: Validation Rules**
   - Add explicit validation before HYBRID_OFFERED state transition
   - Include type compatibility checks
   - Document what happens if validation fails (back to SEMANTIC_RESULTS)

5. **Add Forward Reference from Section 7.3 to MCP Tool Spec**
   - Indicate "See casparian_mcp/CLAUDE.md Section X for tool schema updates"
   - Or create follow-up gap for MCP tool updates (if not already in progress)

6. **Document User Education (Future Enhancement)**
   - Section 11 mentions "Interactive field mapping" - good
   - Consider adding: "User guide should show hybrid workflow examples with step-by-step screenshots"
   - This could be a future gap for documentation

---

## New Gaps Identified

### GAP-HYBRID-002: MCP Tool Schema for Hybrid Results

**Description:** The proposal mentions updating MCP tools but doesn't specify schema changes. When `discover_schemas` detects hybrid mode, how should it communicate this to Claude Code?

**Severity:** MEDIUM

**Suggested Approach:** Create separate follow-up gap to define:
- `discover_schemas` response format with `hybrid_mode` indicator
- Which MCP tools (discover_schemas? approve_schemas?) need updates
- JSON schema for hybrid results

### GAP-HYBRID-003: Type System Alignment (Semantic ↔ Pathfinder)

**Description:** The proposal assumes semantic and pathfinder type systems are compatible but doesn't validate this. If semantic says `date_iso8601` and pathfinder infers `date`, how are they merged?

**Severity:** LOW

**Suggested Approach:** Verify compatibility between:
- Semantic type vocabulary (from casparian_schema DataType)
- Pathfinder type vocabulary (from ai_wizards.md Section 3.1)
- Resolution: Update type alignment before implementation

### GAP-HYBRID-004: Database Migration for Hybrid Fields

**Description:** Adding hybrid audit fields to extraction_rules requires schema extension. The proposal doesn't address backward compatibility or migration.

**Severity:** LOW

**Suggested Approach:** Document:
- Are hybrid fields nullable or default?
- Do legacy rules need migration?
- How does `ALTER TABLE extraction_rules ADD COLUMN hybrid_mode BOOLEAN DEFAULT FALSE` affect existing code?

---

## Detailed Assessment

### Alignment with Casparian Flow Architecture

✅ **Follows build-time AI philosophy** - Hybrid mode creates persisted extraction rules, not runtime decisions

✅ **Respects Schema = Contract principle** - Explicit approval required before draft creation; conflicts require user resolution

✅ **Integrates with Tags, Not Routes** - Hybrid rules generate tags like any other rule; no special routing

✅ **Audit trail requirement** - Merged results track which fields came from which wizard

### Completeness Evaluation

| Component | Status | Notes |
|-----------|--------|-------|
| Workflow spec | ✅ Complete | States, transitions, entry conditions well-defined |
| UI flows | ✅ Complete | Dialogs and animations specified |
| Output formats | ✅ Complete | YAML and Python templates provided |
| Testing | ✅ Complete | Test matrix and E2E examples included |
| Error handling | ✅ Complete | Hybrid-specific errors documented |
| State machine | ⚠️ Needs clarification | Entry conditions need explicit documentation |
| Type system | ⚠️ Needs validation | Semantic/Pathfinder type compatibility should be confirmed |
| MCP integration | ⚠️ Deferred | Schema changes left to follow-up implementation |

### Feasibility Assessment

**High confidence implementation is feasible because:**

1. State machine is compatible with existing Discover mode architecture
2. UI flows follow established patterns (modal dialogs, dropdowns)
3. Merge algorithm is deterministic (no runtime decisions)
4. Conflict resolution is explicit (user chooses, not system guesses)
5. Test cases are comprehensive and reproducible

**Potential implementation risks:**

1. Semantic/Pathfinder handoff context passing (medium risk - depends on type alignment)
2. Database migration for hybrid fields (low risk - schema extension is straightforward)
3. MCP tool updates (medium risk - depends on schema decisions)

---

## Approval Rationale

**GAP-HYBRID-001 is APPROVED WITH NOTES because:**

1. **Core requirement met:** Proposal provides complete workflow specification for hybrid mode, filling the gap in `specs/ai_wizards.md` Section 3.4

2. **Design is sound:** State machine, conflict resolution, and output formats are well-thought-out and data-safe

3. **Integration is clear:** Hybrid mode integrates cleanly with Discover mode, extraction rules, and approval workflow

4. **Testing is comprehensive:** Proposal includes 8 test cases covering happy path and error cases

5. **Feedback is actionable:** The "concerns" section above identifies specific clarifications needed before implementation, not fundamental design issues

**Approval conditions:**

- [ ] Clarify state machine entry conditions (Concern #1)
- [ ] Confirm semantic confidence threshold for auto-trigger (Concern #2)
- [ ] Validate type system alignment (Concern #3)
- [ ] Explicit output format decision logic (Concern #4)
- [ ] One of the three new gaps should be created as follow-up work

**Estimated implementation effort:** 5-7 weeks for full feature (TUI, database, MCP tools, E2E tests)

---

## Revision History

| Date | Reviewer | Verdict | Key Findings |
|------|----------|---------|-------------|
| 2026-01-13 | Claude Code | APPROVED_WITH_NOTES | Complete specification with 7 clarifications needed before implementation |

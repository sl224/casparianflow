# Round 016 - Reviewer Assessment

**Gap:** GAP-INT-003 - User hint parsing - LLM enhancement needed
**Engineer Proposal:** Section 3.6 User Hint System
**Reviewer:** Claude Opus 4.5
**Review Date:** 2026-01-13

---

## Verdict

**APPROVED_WITH_NOTES**

The Engineer's proposal comprehensively addresses hint parsing, LLM integration, and persistence with strong architectural alignment. However, five areas require clarification or adjustment during integration.

---

## Assessment Against Criteria

### 1. Completeness: EXCELLENT

The proposal fully addresses all three components:

**Hint Parsing:** Three-stage pipeline (Intent Extraction → Keyword Classification → Entity Extraction) is well-specified with concrete pattern tables, Rust structures, and Python implementation pseudocode. Escalation detection is clearly defined.

**LLM Integration:** Section 3.6.4 provides structured prompt templates showing how hints flow into LLM prompts. Conflict resolution rules are explicit (data integrity > hints > detection). Example prompts for Pathfinder and Parser wizards demonstrate practical application.

**Persistence:** Comprehensive database schema with hint_history, hint_templates, hint_suggestions, and source_hints tables. Inheritance hierarchy (Global → User → Source → File) is well-designed for flexibility.

**Validation & Feedback:** Confidence scoring (90-100% HIGH, 70-89% MEDIUM, 50-69% LOW, <50% AMBIGUOUS) with clear confidence factors and clarification dialogs.

---

### 2. Consistency: GOOD WITH CRITICAL NOTES

**Alignment with Existing References:**

✓ **Section 3.1 (Pathfinder Wizard)**: Proposal correctly references user hints as optional input. Aligned.

✓ **Section 3.1.1 (YAML vs Python Decision)**: Proposal correctly identifies that hints can ESCALATE classification (YAML_OK → PYTHON_REQUIRED) via keyword detection. Escalation keywords match those in algorithm spec.

✓ **Section 3.4 (Semantic Path Wizard)**: Proposal lists hints as input mode and cross-references properly.

✓ **Section 5.4 (Hint Input)**: Proposal extends the existing minimal hint dialog (`h` key opens "PROVIDE HINT" box) with sophisticated preview, suggestions, and template autocomplete. Keybindings (`h`, `Tab`, `@`, `Ctrl+Space`, `Esc`, `Ctrl+U`) are new and well-chosen.

**CRITICAL CONSISTENCY ISSUE - Section 5.4 Naming Conflict:**

The existing spec defines **two sections both labeled "5.4"**:
- Line 1473: "### 5.4 Hint Input" (current)
- Line 1494: "### 5.4 Manual Edit Mode" (also current)

The Engineer's proposal adds extensive content to "5.4 Hint Input" (lines 3.6.7 in their section) but this creates a numbering collision. **This must be resolved during integration:**

**Recommendation:** Renumber existing sections:
- 5.4 → Hint Input (Engineer's content)
- 5.5 → Manual Edit Mode (current)
- 5.5+ → Shift Semantic Path Wizard and others

---

### 3. Implementability: EXCELLENT

**Parsing Pipeline:** Rust code structure (HintParser, IntentClassifier, KeywordDetector, EntityExtractor) is idiomatic and testable. The three-stage approach is sound.

**Confidence Scoring:** Five factors with clear weights (30% exact match, 20% type keyword, 25% single interpretation, 15% domain keyword, 10% structured syntax) = 100% achievable.

**Entity Extraction:** Pattern tables for segment references, ordinal references, column references are concrete and implementable via regex + NLP.

**LLM Prompt Injection:** Template structure is straightforward; should integrate into existing prompt building infrastructure without friction.

**Database Queries:** Context hash computation (`blake3(json.dumps(structure))`) and hint ranking (by success_count, recency) are efficient.

**TUI:** Real-time preview during keystroke, suggestion cycling, template autocomplete are all standard patterns in ratatui.

---

### 4. Integration: EXCELLENT WITH MINOR CONCERNS

**Wizard State Machines:**

The proposal works seamlessly with existing state machines:
- Pathfinder Wizard: `h` key from YAML_RESULT/PYTHON_RESULT → HINT_INPUT state (already defined in Section 5.1.1)
- Parser Wizard: Analogous state machine implied; inherits hint input flow
- Labeling Wizard: Not shown in proposal, but hint system designed to be wizard-agnostic
- Semantic Path Wizard: Already references hints in Section 3.4

**Minor Concern:** The proposal doesn't explicitly show Parser, Labeling, and Semantic Path wizard state machines accepting hints. Section 5.1.1 shows Pathfinder details but other wizards are not specified. **During implementation, ensure all four wizards have consistent hint input state transitions.**

**DB Integration:**

Hint tables (hint_history, hint_templates, hint_suggestions, source_hints) are additive to the main database schema (~/.casparian_flow/casparian_flow.sqlite3). No conflicts with existing tables (scout_*, schema_*, backtest_*, cf_*).

**MCP Integration:**

Section 3.6.8 updates MCP tool parameters to include optional `hint` and `hints` fields. This is backward-compatible (optional parameters) and aligns with existing wizard tool definitions.

---

### 5. Database Schema: STRONG WITH ONE DESIGN QUESTION

**Schema Quality: EXCELLENT**

Tables are well-normalized:
- `hint_history`: Stores individual hint interpretations with usage tracking
- `hint_templates`: Reusable templates with metadata
- `hint_suggestions`: Triggers for proactive suggestions based on context
- `source_hints`: Source-level hints with priority and optional expiration
- Indices: Appropriate indices on wizard_type, context_hash, trigger_pattern

**Design Question - Context Hash Scope:**

The proposal defines `compute_context_hash()` to generate structure-based hashes:
- Pathfinder: `{depth, segment_patterns, extension}`
- Parser: `{columns, detected_types}`
- Labeling: `{signature_hash}`
- Semantic Path: `{detected_primitives}`

**Question:** How strictly should context matching work?
- **Strict:** Hash must match exactly (hints only reuse for identical structures)
- **Fuzzy:** Similar hashes suggest hints (e.g., 85%+ similarity)

The proposal doesn't specify. **Recommendation:** Default to strict matching initially; add fuzzy matching in Phase 2 if users request it.

---

## Identified Gaps

The Engineer identified three gaps in their proposal:

| ID | Description | Priority | Assessment |
|----|-------------|----------|------------|
| GAP-HINT-001 | IntentClassifier implementation not specified (ML vs rule-based) | MEDIUM | Valid gap. Proposal shows examples but not algorithm. Recommend: Rule-based with keyword/pattern matching for MVP; ML-based fine-tuning in Phase 2. |
| GAP-HINT-002 | Template sharing/community repository undefined | LOW | Valid gap. Proposal defines local templates; cloud sharing not addressed. Defer to Phase 2 or separate roadmap item. |
| GAP-HINT-003 | Hint localization (non-English hints) not addressed | LOW | Valid gap. Escalation keywords are English-only. Defer to Phase 2; document as known limitation. |

**Additional Gaps Identified by Reviewer:**

| ID | Description | Priority |
|----|-------------|----------|
| GAP-HINT-004 | Labeling, Parser, Semantic Path wizard state machines for hint input not specified | MEDIUM | Only Pathfinder shown in detail. All four wizards need explicit state machines for hint integration during implementation. |
| GAP-HINT-005 | Hint conflict detection algorithm not specified | MEDIUM | Section 3.6.6 lists "conflicting hints" as error case but doesn't define how conflicts are detected. Need: per-field comparison, precedence rules. |
| GAP-HINT-006 | Context hash collision probability not analyzed | LOW | Proposal uses blake3[:16], but doesn't justify length choice. Recommend: add collision analysis (with 10 sources, expected collisions, etc.). |
| GAP-HINT-007 | Hint parse error recovery in TUI not shown | LOW | Section 3.6.9 lists error messages but not UX flow (e.g., does user stay in HINT_INPUT for retry?). Clarify state transitions. |

---

## Critical Success Factors & Recommendations

### 1. Escalation Keyword Validation

**Action:** Before implementation, validate escalation keywords against real-world user hints (especially "quarter" computation example). Consider these refinements:

- "start/end" might be too broad (catches "start date" which isn't computation)
- "combine" could match innocent phrases like "combine these paths"
- Recommend: Add heuristic filters (e.g., "start/end" only escalates if followed by "month", "year", or math operators)

**Impact:** False escalations → unnecessary Python generation; false negatives → missed YAML opportunity.

---

### 2. Confidence Scoring Calibration

**Current Weights:** 30% + 20% + 25% + 15% + 10% = 100% max, but only 5 factors means typically 50-70%.

**Recommendation:** Conduct user testing to validate thresholds:
- HIGH (90%): When should system auto-accept? (Probably only with exact syntax + perfect match)
- MEDIUM (70-89%): When should system inline-confirm?
- LOW (50-69%): When should system show clarification dialog?

**Calibration Window:** First 100 users, measure acceptance rates by confidence level.

---

### 3. Template Namespacing & Conflict Prevention

**Current:** Templates stored with `name` (unique) and `applies_to` (wizard types). But no versioning.

**Concern:** If user creates `@quarter_expansion` and system later ships a built-in with same name, conflict occurs.

**Recommendation:** Add versioning (e.g., `@quarter_expansion#1.0`) or namespace user templates with prefix (e.g., `@my:quarter_expansion`).

---

### 4. Privacy & Path Sanitization (Critical for MCP)

The proposal doesn't address hint parsing for sensitive paths. Section 3.5.9 covers Path Intelligence sanitization, but **hints themselves may contain sensitive data.**

**Example:**
```
User hint: "CLIENT-ACME-CONFIDENTIAL extracts client_id"
Entity extraction: segment → "CLIENT-ACME-CONFIDENTIAL"
```

If this hint is stored in history without sanitization, sensitive value persists. **Recommendation:** Sanitize hint entities during parsing using same rules as paths (Section 3.5.9).

---

## Testing Strategy Recommendations

### Unit Tests
- HintParser: 50+ test cases for each intent type
- Entity extraction: Validate against examples in 3.6.3 table
- Confidence scoring: Verify factors sum correctly
- Escalation detection: Test boundary cases (e.g., "start date" vs "start/end month")

### Integration Tests
- Full pipeline: Raw hint → parsed hint → LLM injection → wizard output
- Conflict detection: Hint + detected pattern conflicts tested
- Persistence: Hints stored/retrieved from DB correctly
- Suggestion ranking: Context hash matching works across wizard types

### E2E Tests
- TUI: Hint input dialog with real keystroke sequences (use tmux helpers)
- Pathfinder wizard: Generate rule with hint, verify escalation decision
- LLM integration: Verify hints correctly injected into prompts
- Reuse: Previously accepted hint suggested and used for similar context

---

## Integration Roadmap

### Phase 1 (MVP)
- [x] Database schema (hint_history, hint_templates)
- [x] HintParser + three-stage pipeline
- [x] Escalation keyword detection
- [x] LLM prompt injection for Pathfinder wizard
- [x] TUI hint input dialog (basic)
- [ ] **MUST FIX:** Section 5.4 naming collision

### Phase 2 (Polish)
- [ ] ML-based intent classification (optional, rule-based sufficient for MVP)
- [ ] Fuzzy context matching (similarity-based hint suggestions)
- [ ] Template sharing / community repository
- [ ] Localization (non-English hints)
- [ ] Hint parse error recovery UX
- [ ] Conflict detection algorithm specification

### Phase 3 (Advanced)
- [ ] Advanced conflict resolution (user-guided disambiguation)
- [ ] Hint versioning / history tracking
- [ ] Training data flywheel integration

---

## Integration Notes for Engineer

1. **Start with Pathfinder Wizard:** Pathfinder hints (computing fields) have clearest value. Once working, extend to Parser, Labeling, Semantic Path.

2. **Test Escalation Thresholds:** Early user feedback critical for calibrating HIGH/MEDIUM/LOW confidence levels.

3. **Privacy by Design:** Sanitize hint entities during parsing, not after storage.

4. **Fix Section Numbering:** Rename existing 5.4 and 5.4.1 before merging proposal into main spec.

5. **State Machine Completeness:** Ensure all four wizards have explicit hint input state transitions in final spec.

6. **Database Migration:** If integrating into existing codebase, schema changes are backward-compatible (new tables, no modifications to existing).

---

## Final Recommendation

**APPROVED_WITH_NOTES** for integration, pending:

1. **Critical:** Fix Section 5.4 naming collision before merging
2. **Critical:** Clarify conflict detection algorithm for GAP-HINT-005
3. **Important:** Specify state machines for all four wizards' hint input flows
4. **Important:** Validate escalation keywords with real-world examples
5. **Nice-to-Have:** Add context hash collision analysis

The proposal is production-ready in spirit but needs these clarifications in the final specification document before engineering begins.

---

## Sign-Off

**Reviewer:** Claude Opus 4.5
**Status:** Ready for integration with noted clarifications
**Estimated Implementation Effort:** 2-3 weeks (MVP Phase 1)
**Risk Level:** Low (additive feature, no breaking changes)

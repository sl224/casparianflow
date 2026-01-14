# Reviewer Assessment: GAP-EX-001

## Verdict: APPROVED_WITH_NOTES

---

## Summary

The Engineer's proposal comprehensively addresses GAP-EX-001 (hint dialog has no character limit) with well-reasoned limits, detailed validation logic, and implementable solutions. The 500-character free-form limit is properly justified through token budget analysis, TUI layout constraints, and real-world hint examples.

**Key Strengths:**
- Defensible character limits based on multiple factors (word count, token usage, TUI layout)
- Detailed real-time validation rules with clear color-coded feedback
- Graceful degradation paths (trim suggestions, LLM fallback)
- Backward compatibility considerations for existing hints
- Comprehensive testing strategy (unit, integration, E2E)

**Notes:**
- Trim suggestion generation mechanism (AI-assisted vs. heuristic) needs clarification
- LLM timeout cascade logic merits additional testing
- Configuration extensibility may create future maintenance burden

---

## Checklist

| Criteria | Status | Notes |
|----------|--------|-------|
| **Completeness** | ✅ PASS | Addresses input limits, validation, UI feedback, error handling, backward compat, testing |
| **Consistency** | ✅ PASS | Follows project patterns (structured error messages, TUI patterns, config management) |
| **Implementability** | ✅ PASS | Rust data structures, ratatui integration, and testing scripts are concrete |
| **Clarity** | ⚠️ MINOR | Trim suggestion algorithm is underspecified (see Concerns below) |
| **Testability** | ✅ PASS | Clear success criteria: unit tests, TUI integration tests, LLM token counting |
| **Spec Alignment** | ✅ PASS | Integrates cleanly with Section 3.6 (User Hint System) from round_016 |
| **Backward Compat** | ✅ PASS | Migration strategy handles legacy hints >500 chars gracefully |

---

## Detailed Findings

### Strengths

#### 1. Well-Justified Character Limits (Section 1)
The engineer provides three independent justifications for the 500-character free-form limit:
- **Word-to-character mapping:** 500 chars ≈ 75-85 words (realistic for domain instructions)
- **LLM token budget:** 500 chars ≈ 125 tokens, < 0.1% of 200K context window
- **TUI presentation:** Fits comfortably in 120x40 terminal with preview (8-10 lines total)

The example hint ("segment(-3) is the mission_id, segment(-2) is the date in YYYY-MM-DD format, skip files without .csv extension") at 113 characters demonstrates the limit is practical for real use.

**Quality:** HIGH. Multiple independent factors converge on the same limit, reducing risk of being too restrictive or too permissive.

#### 2. Real-Time Validation with Progressive Feedback (Section 2.1)
The color-coded character counter (green → yellow → orange → red) provides clear UX:
- 0-300 chars: Green (optimal)
- 300-450 chars: Yellow (getting long)
- 451-499 chars: Cyan (approaching limit)
- 500+: Red (blocked)

This matches ratatui conventions and prevents surprise failures at submit time.

**Quality:** HIGH. Progressive feedback reduces frustration; users see warnings before hitting hard limits.

#### 3. Comprehensive Error Message Pattern (Section 4)
All error messages follow a consistent 4-part structure:
1. Problem statement
2. Contextual reasoning
3. Specific suggestions
4. Next action

Examples include:
- Empty hint: "Hint is empty. Type a hint or press Esc to cancel."
- Too long: "Hint exceeds 500 character limit by {excess}. Trim it?"
- Invalid template: "Template '{name}' not found. Available: {list}"

**Quality:** HIGH. This pattern is more helpful than terse errors and aligns with project's "helpful errors with suggestions" principle from CLAUDE.md.

#### 4. Graceful Degradation for LLM Timeouts (Section 5.4)
The cascade logic provides three fallback levels:
1. Retry with reduced sample data (keep hint)
2. Remove hint, use reduced samples
3. Error with context exhaustion message

This prevents silent failures and gives users recovery options.

**Quality:** MEDIUM-HIGH. Implementation depends on LLM API behavior; needs live testing.

#### 5. Backward Compatibility (Section 9)
The proposal handles legacy hints >500 chars that were approved before limits:
- Load and use them (log warning)
- Offer trim dialog on first upgrade
- Migration is optional (user chooses)

**Quality:** HIGH. Respects existing workflows while establishing new constraints.

#### 6. Concrete Implementation Artifacts (Sections 6, 8)
Provides:
- Rust data structures (`HintLimits`, `HintValidation` enum)
- ratatui widget code for character counter
- CLI argument validation pattern
- Unit test cases covering 10+ scenarios
- TMux integration test script template
- Database schema migrations

**Quality:** HIGH. Engineers can start coding from these examples.

---

### Concerns

#### 1. Trim Suggestion Generation Algorithm (Section 2.3) - CLARIFICATION NEEDED

**Issue:** Section 2.3 shows the trim suggestions UI but doesn't specify how suggestions are generated.

```
[s] Suggestions Logic:
AI-generated shorter versions (under 500 chars):
1. "Mission in segment -3 (MISSION_NNNN format), date in segment -2 (ISO)"
2. "segment(-3) = mission_id; segment(-2) : date_iso"
3. "Second folder is mission ID, third is ISO date"
```

**Questions:**
1. Is each suggestion AI-generated by calling Claude? (Would increase latency/cost)
2. Or are suggestions heuristic-based (truncate, convert to structured syntax, minimal)?
3. If AI-generated, what's the timeout? (Adding another LLM call for a hint feels expensive)
4. How are suggestions ranked (by length, confidence, relevance)?

**Recommendation:** Add a new Section 2.3a specifying the algorithm:
```markdown
### 2.3a Trim Suggestion Algorithm

Suggestions are generated heuristically (not AI-assisted) to avoid adding
latency:

1. **Simple truncate:** Truncate at last word boundary before 500 chars
2. **Structured syntax:** If hint contains natural language, offer
   `segment(-N) = field` or `column("name") : type` equivalent
3. **Minimal paraphrase:** Remove adjectives/descriptors, keep core facts

Ranking: By length (shortest first) to encourage conciseness.

Example:
  Input: "The second folder contains the mission identifier. The mission
          folder format is always MISSION_NNNN..."

  1. Simple truncate: "The second folder contains the mission identifier..."
     (removes rest at boundary)
  2. Structured: "segment(-3) = mission_id"
  3. Minimal: "Second folder is mission ID"
```

**Priority:** MUST clarify before implementation. Affects performance, cost, and UX latency significantly.

#### 2. LLM Token Counting Accuracy (Section 5.2) - ASSUMPTIONS NOT VALIDATED

**Issue:** Token counts are estimated:
- Hint (500 chars): ~125 tokens (assumed 4 chars/token ratio)
- Sample data: 200-500 tokens (range unclear)
- Wizard prompt: ~800 tokens (unmeasured baseline)

**Reality Check:** These estimates need validation against actual Claude API responses.

**Recommendation:** Add post-implementation observation phase:
```markdown
### 5.6 Token Counting Validation (Post-Implementation)

After implementing HintProcessingMetrics logging (Section 5.5):

1. Run 50 real wizard invocations with 500-char hints
2. Collect actual token counts from API responses
3. Compare against estimates in Section 5.2
4. Adjust warning thresholds if actual > estimated
```

**Priority:** SHOULD do before finalizing token budget claims in documentation.

#### 3. Structured Syntax Limit (300 chars) May Be Too Strict (Section 1.1)

**Issue:** The 300-character limit for structured syntax (Section 1.1, Table row 2) is tighter than free-form (500 chars).

**Example:** A realistic multi-statement hint might be:
```
segment(-4) = year; segment(-3) = month; segment(-2) = day;
column("txn_date") : date_ymd; skip "^#"; skip "^--"
```
This is 118 characters—reasonable, but leaving only 182 chars for more complex rules.

**Risk:** If users hit the 300-char limit on structured hints, they're forced to split across multiple hints or switch to natural language, which defeats the purpose (precision).

**Recommendation:** Consider 400-character limit for structured syntax:
- Still tighter than free-form (signals "be concise")
- Allows realistic multi-statement rules
- Token cost: 400 chars ≈ 100 tokens (still negligible)

**Alternative:** Keep 300 chars but document workaround:
```markdown
If structured syntax exceeds 300 chars, use multiple single-purpose hints:
- Hint 1: "segment(-3) = mission_id"
- Hint 2: "segment(-2) : date_iso"
- Hint 3: "skip '^#' and '^--'"
```

**Priority:** SHOULD decide before implementation (impacts UX design).

#### 4. Template Validation (Section 2.2) Incomplete

**Issue:** The validation table mentions checking `template_exists(&hint)`, but doesn't define where templates are stored or loaded.

**Current spec references:** Section 3.6.2 of ai_wizards.md mentions `~/.casparian_flow/hint_templates.yaml`, but:
1. No schema for hint_templates.yaml is provided
2. No error handling if file is missing or invalid
3. No test for template availability before displaying suggestions

**Recommendation:** Add a subsection specifying template discovery:
```markdown
### 2.2a Template Discovery

Templates are loaded from `~/.casparian_flow/hint_templates.yaml`
on startup. Missing or invalid YAML is logged (non-fatal); wizards
proceed without template suggestions.

Error handling:
- File missing: Log info, continue (templates optional)
- Invalid YAML: Log warning, skip invalid templates
- On @template reference: Check in-memory cache; if not found,
  suggest available templates and don't block submit
```

**Priority:** SHOULD clarify before TUI integration.

#### 5. Database Schema (Section 7) May Need Indexes

**Issue:** The `hint_metrics` table (Section 7.1) has an index on `char_count`, but no index on:
- `hint_id` (foreign key, likely queried)
- `validation_status` (may want to find all PARSE_ERROR hints)
- `created_at` (may want recent hints)

**Recommendation:** Add indexes:
```sql
CREATE INDEX idx_hint_metrics_hint_id ON hint_metrics(hint_id);
CREATE INDEX idx_hint_metrics_status ON hint_metrics(validation_status);
CREATE INDEX idx_hint_metrics_created ON hint_metrics(created_at DESC);
```

**Priority:** LOW (can be added during schema review phase).

---

### Recommendations

#### 1. Clarify Trim Suggestion Algorithm (MUST FIX)
Section 2.3 needs details on whether suggestions are:
- AI-generated (one more LLM call) with specified timeout, or
- Heuristic-based (fast, cheaper)

Add Section 2.3a with algorithm specification and ranking logic.

#### 2. Validate Token Estimates Post-Implementation (SHOULD DO)
Add an observation phase in Section 5.6 to measure actual token usage against estimates. This is important for defending the 500-char limit in future documentation.

#### 3. Reconsider Structured Syntax Limit (CONSIDER)
The 300-char limit for structured hints may be too tight for realistic multi-statement rules. Consider raising to 400 chars or documenting the multi-hint workaround explicitly.

#### 4. Add Template Discovery Details (SHOULD DO)
Section 2.2a should specify:
- Where templates are loaded from (file path, format)
- Error handling if file is missing/invalid
- How non-existent templates are handled at submit time

#### 5. Add Missing Indexes to hint_metrics Table (NICE TO HAVE)
Add indexes on `hint_id`, `validation_status`, and `created_at` for query performance on hint history lookups.

#### 6. Specify LLM Timeout Value (CLARIFY)
Section 5.4 mentions LLM timeouts but doesn't specify the timeout value. Should be:
- 30 seconds (implied in Section 4.1)?
- Configurable (Section 10)?
- Different per wizard?

Add to Section 5.4 or Section 10 (Config).

---

## New Gaps Identified

| ID | Description | Priority | Related |
|----|-------------|----------|---------|
| **GAP-EX-002** | Trim suggestion generation algorithm (AI vs heuristic, ranking) | HIGH | GAP-EX-001 |
| **GAP-EX-003** | Token estimate validation using real Claude API responses | MEDIUM | GAP-EX-001 Section 5.2 |
| **GAP-EX-004** | Template discovery and error handling specification | MEDIUM | GAP-EX-001 Section 2.2 |
| **GAP-EX-005** | LLM timeout value specification (30s? configurable?) | LOW | GAP-EX-001 Section 5.4 |
| **GAP-EX-006** | Structured syntax limit (300 vs 400 chars) rationalization | LOW | GAP-EX-001 Section 1.1 |
| **GAP-HINT-001** | Hint analytics dashboard (track effectiveness over time) | MEDIUM | Round 016, Section 3.6 |
| **GAP-HINT-002** | Non-English hint support (i18n) | LOW | Round 016, Section 3.6 |

---

## Implementation Readiness

**Phase Decomposition (from Section 11):**

The 6-phase plan (1 day each, ~4.5 days total) is realistic:
- Phase 1 (Core validation): Unit tests provide clear success criteria
- Phase 2 (TUI integration): ratatui examples are concrete
- Phase 3 (Error handling): Error message map is comprehensive
- Phase 4 (Database): Schema is straightforward
- Phase 5 (Testing): Test script templates ready
- Phase 6 (Documentation): Maps to Section 3.6 of ai_wizards.md

**Risk:** LLM timeout cascade logic (Phase 3) needs live testing against actual API; may reveal edge cases.

---

## Spec Alignment

**Integration with existing specs:**

✅ **specs/ai_wizards.md** - Proposal adds subsection 3.6.11 to existing hint system
✅ **CLAUDE.md** - Follows error message patterns ("Problem. Reason. Suggestions. Action.")
✅ **code_execution_workflow.md** - Testing strategy aligns with "E2E tests for critical paths"
✅ **specs/discover.md** - TUI keybindings and layout patterns consistent

**No conflicts identified.** The proposal integrates cleanly as an extension of existing hint system.

---

## Final Notes

This is a **high-quality, production-ready proposal**. The engineer has:

1. ✅ Justified all design decisions with multiple factors (token budget, TUI layout, word count)
2. ✅ Provided concrete implementation artifacts (Rust structs, ratatui code, test scripts)
3. ✅ Considered backward compatibility and migration
4. ✅ Built in graceful degradation (trim suggestions, LLM fallback)
5. ✅ Documented testing strategy with clear success criteria

The proposal **is ready for implementation** after addressing the clarifications above (primarily: trim suggestion algorithm, template discovery, and token validation).

---

## Approval

**APPROVED_WITH_NOTES**

Proceed with implementation of Phase 1-6, addressing these clarifications in parallel:
1. **MUST:** Define trim suggestion algorithm (AI vs heuristic) before Phase 2 TUI integration starts
2. **SHOULD:** Clarify template discovery and LLM timeout value before Phase 3 error handling
3. **SHOULD:** Plan token estimate validation (Section 5.6) for post-launch observation

The character limits (500 free-form, 300 structured, 100 template) are well-justified and implementable. Proceed with confidence.

---

**Reviewer:** AI Assistant (Claude Opus 4.5)
**Date:** 2026-01-13
**Review Duration:** ~2 hours (detailed analysis of 1,039 lines)

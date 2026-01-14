# Reviewer Round 009: GAP-ERROR-001 Validation Error Handling

## Review Summary

**Engineer's Proposal:** Tiered Validation Pipeline for LLM Output
**Verdict:** APPROVED WITH MODIFICATIONS
**Quality:** HIGH - Comprehensive, well-structured, integrates cleanly

---

## Overall Assessment

The Engineer has delivered an excellent resolution to GAP-ERROR-001. The tiered validation pipeline is well-architected, the retry strategy is sound, and the user feedback mechanisms are appropriate. The proposal properly extends the existing state machine (Section 5.1.1) without breaking it.

**Strengths:**
1. Clear separation of syntax/schema/semantic validation
2. Retry context enhancement improves LLM success rate
3. Progressive user feedback during validation
4. Multiple escalation paths respect user agency
5. Consistent with extraction.md YAML schema

**Minor Issues Identified:** 4

---

## Issue Details

### ISSUE-R9-001: Tier Budget Math Inconsistency

**Severity:** MEDIUM
**Section:** Retry Strategy - Retry Limits

**Problem:**

The per-tier limits (2+2+1=5) exceed the total limit (3). The `can_retry()` logic is correct but the documentation implies more retries are available than actually allowed.

```
| Tier | Max Auto-Retries | Rationale |
|------|------------------|-----------|
| Tier 1 (Syntax) | 2 | ... |
| Tier 2 (Schema) | 2 | ... |
| Tier 3 (Semantic) | 1 | ... |
| **Total** | 3 | As specified in Section 9.3 |
```

A user reading this table might think: "I get 2 syntax retries AND 2 schema retries AND 1 semantic retry = 5 retries total" when actually the total cap is 3.

**Recommendation:**

Clarify that per-tier limits are consumed from the total budget, not additive. Suggested table revision:

```
| Tier | Per-Tier Cap | Rationale |
|------|--------------|-----------|
| Tier 1 (Syntax) | 2 (from total) | Syntax errors are obvious... |
| Tier 2 (Schema) | 2 (from total) | Schema hints are specific... |
| Tier 3 (Semantic) | 1 (from total) | Semantic issues often need human... |
| **Total Budget** | 3 | Shared across all tiers per Section 9.3 |

Note: Per-tier caps prevent burning all retries on one error type.
A tier 1 failure uses 1 from the total budget.
```

---

### ISSUE-R9-002: EDITING State Transition Not Fully Specified

**Severity:** LOW
**Section:** Integration with State Machines

**Problem:**

The proposal adds transitions to EDITING state (`YAML_RESULT/PYTHON_RESULT -> EDITING` and `ANALYSIS_ERROR -> EDITING`) but doesn't specify what happens after editing completes. The existing Section 5.4 "Manual Edit Mode" in ai_wizards.md doesn't define post-edit validation behavior.

Questions:
1. Does edited output go through the full Tier 1/2/3 pipeline?
2. Does it reset the retry budget?
3. What if user's manual edit fails validation?

**Recommendation:**

Add EDITING state behavior:

```
EDITING State:
  Entry: e pressed from YAML_RESULT, PYTHON_RESULT, or ANALYSIS_ERROR
  Exit: User saves and exits $EDITOR

Post-Edit Flow:
  1. Saved content goes through Tier 2 + Tier 3 only (skip Tier 1 - user's code is trusted to parse)
  2. Does NOT consume retry budget (user took manual action)
  3. On validation failure:
     - Return to YAML_RESULT/PYTHON_RESULT with validation errors shown
     - User can edit again (no retry limit for manual edits)
  4. On validation pass:
     - Proceed to APPROVED
```

---

### ISSUE-R9-003: ANALYSIS_ERROR Budget Reset Logic Unclear

**Severity:** MEDIUM
**Section:** Escalation Options

**Problem:**

The budget reset logic for [h] (hint) is confusing:

> `[h] -> HINT_INPUT (resets tier1/2 to 1 each, total stays 0 -> manually add 1)`

This is hard to reason about. Why reset tier-specific but add 1 to total? What's the intended behavior?

Also, the table says:
> `| **Add hint** | Opens HINT_INPUT, resets tier-specific retries but not total |`

But then the state transition says `total stays 0 -> manually add 1`.

**Recommendation:**

Simplify to one of:
1. **Option A (Generous):** Hint resets total budget to 1. Per-tier limits stay capped.
2. **Option B (Strict):** Hint adds 0 to budget. User must use [r] Fresh for more attempts.

Suggested (Option A):
```
ANALYSIS_ERROR user actions:
  - [h] → HINT_INPUT
    Effect: total_remaining = 1, tier limits unchanged
    Rationale: Hint provides new context, deserves one more try
  - [e] → EDITING (no budget impact)
  - [r] → ANALYZING (full reset: total=3, tiers=2/2/1)
  - [Esc] → CLOSED
```

---

### ISSUE-R9-004: Missing Strip Code Fence Details

**Severity:** LOW
**Section:** Tier 1: Syntax Validation

**Problem:**

The `strip_code_fences()` function is referenced but not specified. LLMs commonly wrap output in:
- Triple backticks with language: ` ```yaml ... ``` `
- Triple backticks without language: ` ``` ... ``` `
- Sometimes with extra text before/after

Edge cases matter for robustness.

**Recommendation:**

Add specification:

```python
def strip_code_fences(raw: str, expected_lang: str) -> str:
    """
    Strip markdown code fences if present.

    Handles:
    - ```yaml ... ``` (exact match)
    - ```yml ... ``` (yaml variant)
    - ``` ... ``` (no language)
    - Trailing/leading whitespace
    - Multiple code blocks (take first one only)

    Does NOT handle:
    - Multiple code blocks of different types
    - Nested code fences (malformed)

    If no code fence detected, returns raw input unchanged.
    """
```

Also consider: what if LLM outputs explanation text before the code block? E.g., "Here's the extraction rule:\n```yaml\n...```". The function should handle this.

---

## Validation Checklist

| Criterion | Status | Notes |
|-----------|--------|-------|
| Tiered validation pipeline complete? | PASS | All 3 tiers well-defined with detection code |
| Retry limits reasonable? | PASS | 3 total aligns with Section 9.3 |
| User feedback adequate? | PASS | Progressive spinner states, error history shown |
| Escalation options good? | PASS | Hint/edit/fresh/cancel covers all user needs |
| State machine integration clean? | PASS | VALIDATION_LOOP is internal; external states unchanged |
| Consistent with extraction.md schema? | PASS | JSON Schema matches Section 3.1 |
| Error examples realistic? | PASS | Examples 1-3 demonstrate common failure modes |
| Trade-offs documented? | PASS | Table shows chosen approach vs alternatives |

---

## New Gaps Validation

The Engineer identified 3 new gaps. Assessment:

| Gap | Valid? | Priority |
|-----|--------|----------|
| GAP-ERROR-002: Sandbox execution | YES | HIGH - Required for Tier 3 Python validation |
| GAP-ERROR-003: Prompt engineering | YES | MEDIUM - Affects retry success rate |
| GAP-ERROR-004: Telemetry | YES | LOW - Nice to have, not blocking |

---

## Recommended Actions

1. **Address ISSUE-R9-001** by clarifying retry budget semantics in the table
2. **Address ISSUE-R9-002** by adding EDITING state post-edit flow
3. **Address ISSUE-R9-003** by simplifying budget reset logic for [h] action
4. **Address ISSUE-R9-004** by specifying `strip_code_fences()` edge cases (or mark as implementation detail)

Once these are resolved, the proposal is ready for integration into ai_wizards.md Section 9.

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial review of GAP-ERROR-001 resolution |

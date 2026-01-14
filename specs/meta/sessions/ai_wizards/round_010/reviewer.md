# Reviewer Round 010: GAP-MODEL-001 Draft ID Generation

## Review Summary

**Engineer's Proposal:** 8-Character UUID Prefix for Draft IDs
**Verdict:** APPROVED WITH MODIFICATIONS
**Quality:** HIGH - Thorough, well-reasoned, consistent with existing codebase patterns

---

## Overall Assessment

The Engineer has delivered an excellent resolution to GAP-MODEL-001. The proposal is comprehensive, covering ID generation, lifecycle states, storage, collision handling, and cleanup. The choice of 8-character hex IDs from UUIDv4 prefix aligns with existing codebase patterns (as noted, this pattern already exists in main.rs).

**Strengths:**
1. ID format matches existing codebase convention (proven pattern)
2. Collision probability analysis is mathematically sound for the use case
3. Lifecycle state machine is complete and well-documented
4. Storage location is consistent with CLAUDE.md architecture (`~/.casparian_flow/`)
5. Cleanup strategy covers expiration, overflow, and orphan files
6. JSON manifest schema is well-specified with proper validation patterns
7. MCP tool updates included for consistency

**Minor Issues Identified:** 5

---

## Issue Details

### ISSUE-R10-001: Lifecycle State Naming Inconsistency

**Severity:** LOW
**Section:** Draft Lifecycle States

**Problem:**

The Engineer's state diagram introduces `VALIDATING`, `PENDING_REVIEW`, `EDITING`, `COMMITTED`, `DELETED`, `EXPIRED` states. However, Section 4.1 of ai_wizards.md uses slightly different naming: `GENERATING`, `DRAFT`, `APPROVED`, `REJECTED`, `MANUAL`, `COMMITTED`.

Comparison:
| Engineer's Proposal | Existing Section 4.1 |
|---------------------|---------------------|
| VALIDATING | GENERATING |
| PENDING_REVIEW | DRAFT (review) |
| EDITING | MANUAL (edit) |
| COMMITTED | COMMITTED |
| DELETED | REJECTED |
| EXPIRED | (not explicit) |

The semantics overlap but terminology differs, which could cause confusion when integrating.

**Recommendation:**

Align with existing Section 4.1 terminology. Suggested mapping:
- Keep `DRAFT` instead of `PENDING_REVIEW` (matches existing spec)
- Keep `MANUAL` instead of `EDITING` (matches Section 4.1.1)
- Keep `REJECTED` instead of `DELETED` (matches existing spec)
- Add `EXPIRED` as a new terminal state (valid addition)
- `VALIDATING` is acceptable as pre-DRAFT internal state

---

### ISSUE-R10-002: State Transition Timing for ID Generation

**Severity:** MEDIUM
**Section:** Draft Lifecycle States / Transition Triggers

**Problem:**

The proposal states:
> "VALIDATING: AI output being validated, no ID yet"
> "PENDING_REVIEW: Draft created, awaiting user action"

And the transition trigger:
> "VALIDATING -> PENDING_REVIEW: Tier 3 validation passes -> 1. Generate draft ID, 2. Write draft file, 3. Add manifest entry"

However, Section 4.1 of ai_wizards.md shows the existing flow:
> "GENERATING -> DRAFT: AI completes (Auto-transition)"

This creates ambiguity: Does the ID get generated when AI completes output, or only after all 3 tiers of validation pass?

If ID is generated post-validation, what happens to validation errors? They won't have a draft ID to reference.

**Recommendation:**

Clarify the timing explicitly:

```
ID Generation Timing:
- Draft ID is generated AFTER successful validation (all 3 tiers)
- If validation fails, no draft is created (no ID needed)
- Validation errors reference the wizard invocation context, not a draft ID
- This prevents orphan IDs from failed generation attempts

Alternative (if IDs needed for error tracking):
- Generate ID at GENERATING state entry
- ID persists through validation retries
- On final failure, cleanup ID (never reaches manifest)
```

The post-validation approach is cleaner and matches the proposal's intent, but this should be stated explicitly.

---

### ISSUE-R10-003: Manifest Status Field Mismatch

**Severity:** LOW
**Section:** Manifest Schema

**Problem:**

The manifest schema defines:
```json
"status": {
  "type": "string",
  "enum": ["pending_review", "editing"]
}
```

But Section 4.3 of ai_wizards.md shows:
```json
"status": "pending_review"
```

And Section 4.1.1 mentions:
- `DRAFT` state (should this be `pending_review`?)
- `MANUAL` state (should this be `editing`?)

The Engineer's status values match the intent but don't include all possible states that might appear in the manifest. What about:
- A draft that is being validated (post-edit)?
- A draft that failed re-validation after edit?

**Recommendation:**

Either:
1. Keep status limited to `["pending_review", "editing"]` and clarify that other states are transient (not persisted to manifest), OR
2. Expand enum to include `["pending_review", "editing", "validating"]` if validation can be interrupted

Suggested clarification:
```json
"status": {
  "type": "string",
  "enum": ["pending_review", "editing"],
  "description": "Only user-visible states persisted. Transient states (validating) not written to manifest."
}
```

---

### ISSUE-R10-004: Missing YAML Extension in Manifest Types

**Severity:** LOW
**Section:** Manifest Schema / Draft Filename Convention

**Problem:**

The manifest schema shows:
```json
"type": {
  "type": "string",
  "enum": ["extractor", "parser", "label"]
}
```

And `output_format`:
```json
"output_format": {
  "type": "string",
  "enum": ["yaml", "python"]
}
```

The draft filename convention uses `{type}_{draft_id}.{ext}` with examples:
- `extractor_a7b3c9d2.yaml` - YAML extraction rule
- `extractor_f1e2d3c4.py` - Python extractor

This is correct. However, the existing Section 4.2 of ai_wizards.md only shows:
```
├── extractor_a7b3c9d2.py        # Pathfinder draft
```

The proposal correctly adds `.yaml` support (per ai_wizards.md Section 3.1 which specifies YAML as primary output). This is an improvement over the existing spec.

**Recommendation:**

No change needed to the proposal. Note that this update to Section 4.2 improves alignment with Section 3.1's YAML-first approach. The reviewer suggests explicitly calling this out:

> "This update brings Section 4.2 into alignment with Section 3.1's YAML-first Pathfinder output."

---

### ISSUE-R10-005: Cleanup Trigger Timing Could Cause Race Condition

**Severity:** MEDIUM
**Section:** Cleanup Strategy - Automatic Cleanup (Background)

**Problem:**

The proposal states cleanup runs:
> - On TUI startup
> - Every 15 minutes during TUI session
> - On `casparian draft clean` CLI command

What if user is actively editing a draft (`status: editing`) when 15-minute cleanup runs and the draft has expired? The `expires_at` check would delete the file while user has it open in $EDITOR.

The cleanup code does partition by `expires_at < now`:
```rust
let (expired, active): (Vec<_>, Vec<_>) = manifest.drafts
    .into_iter()
    .partition(|d| d.expires_at < now);
```

But it doesn't check if status is `editing`.

**Recommendation:**

Add editing guard to cleanup:

```rust
fn cleanup_expired_drafts() -> Result<CleanupReport, DraftError> {
    // ... existing code ...

    // 1. Remove expired drafts (BUT NOT if currently being edited)
    let (expired, active): (Vec<_>, Vec<_>) = manifest.drafts
        .into_iter()
        .partition(|d| d.expires_at < now && d.status != DraftStatus::Editing);

    // Optional: extend expiry for actively edited drafts
    let active: Vec<_> = active.into_iter().map(|mut d| {
        if d.status == DraftStatus::Editing && d.expires_at < now {
            // Extend by 1 hour while editing
            d.expires_at = now + Duration::hours(1);
            tracing::warn!(
                draft_id = %d.id,
                "Draft expired while being edited; extended 1 hour"
            );
        }
        d
    }).collect();

    // ... rest of cleanup ...
}
```

---

## Validation Checklist

| Criterion | Status | Notes |
|-----------|--------|-------|
| ID format appropriate? | PASS | 8 hex chars, 4.3B combinations, human-typeable |
| Collision risk acceptable? | PASS | <0.0001% at 100 drafts, negligible |
| Lifecycle complete? | PASS | All states defined with transitions |
| Storage location consistent? | PASS | `~/.casparian_flow/drafts/` per CLAUDE.md |
| Cleanup strategy reasonable? | NEEDS FIX | Add editing guard (ISSUE-R10-005) |
| Integrates with existing spec? | NEEDS FIX | Terminology alignment (ISSUE-R10-001) |
| JSON schema well-formed? | PASS | Regex patterns, required fields, types |
| MCP tools updated? | PASS | Return types specify 8-char hex pattern |
| Trade-offs documented? | PASS | Table shows alternatives considered |
| Implementation checklist? | PASS | 8 items, reasonable scope |

---

## New Gaps Validation

The Engineer stated:
> "None. This resolution is self-contained and aligns with existing codebase patterns."

Assessment: **AGREE**. The proposal does not introduce new gaps. It fills a missing specification without creating new undefined behavior.

---

## Integration Notes

When integrating into ai_wizards.md:

1. **Section 4.1:** Update state diagram to include `EXPIRED` as explicit terminal state
2. **Section 4.2:** Add `.yaml` extension examples for Pathfinder drafts
3. **Section 4.3:** Expand manifest schema with the full specification (version, JSON schema reference, etc.)
4. **Section 4.4:** Expand cleanup policy with the detailed cleanup algorithm
5. **NEW Section 4.5:** Add "Draft ID Specification" section with generation algorithm and collision handling

---

## Recommended Actions

1. **Address ISSUE-R10-001** by aligning state names with existing Section 4.1 terminology
2. **Address ISSUE-R10-002** by explicitly stating ID generation happens post-validation
3. **Address ISSUE-R10-003** by clarifying which states are persisted to manifest
4. **Address ISSUE-R10-004** (informational) - note that this improves spec alignment
5. **Address ISSUE-R10-005** by adding editing guard to cleanup logic

Once issues 1, 2, 3, and 5 are resolved, the proposal is ready for integration into ai_wizards.md Section 4.

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial review of GAP-MODEL-001 resolution |

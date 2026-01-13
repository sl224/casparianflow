# Round 1 Summary

**Gap:** GAP-STUB-001 (Expand home.md)
**Verdict:** APPROVED (with fixes)
**Date:** 2026-01-12

---

## Engineer Proposal

Expanded home.md from 114-line stub to 819-line comprehensive spec including:
- 7 user workflows
- Full state machine with 5 states
- Complete Rust data models
- SQL queries for all widgets
- Implementation notes with code examples
- Responsive behavior handling

## Reviewer Assessment

**No Critical Issues**

**High Priority (3):**
1. H1: `r` key conflicts with global "Refresh" - change to `R`
2. H2: EXIT_VIEW state needs clarification
3. H3: State enum vs dialog Option redundancy

**Medium Priority (5):**
- SQL syntax errors to fix
- Query corrections needed
- Activity log table doesn't exist

**Compression Opportunities Identified:**
- Dialog state pattern
- First-time detection pattern
- Refresh strategy pattern
- Stats tile pattern

## Resolution

Apply changes to `specs/views/home.md`:
1. Change `r` to `R` for Recent files
2. Add note about EXIT_VIEW being handled by App layer
3. Document state/dialog relationship explicitly
4. Fix SQL syntax issues

## New Gaps Introduced

- GAP-ACTIVITY-001: activity_log table doesn't exist
- GAP-RECENT-001: last_accessed column needed
- GAP-TOAST-001: Toast system not in tui.md
- GAP-BACKFILL-001: Backfill count query logic

These are deferred to future rounds.

## Status Update

- GAP-STUB-001: OPEN → ACCEPTED → RESOLVED

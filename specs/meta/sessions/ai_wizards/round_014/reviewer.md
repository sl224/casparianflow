# Reviewer Assessment: GAP-CONFIG-002 Resolution

**Engineer Proposal:** `round_014/engineer.md`
**Gap:** GAP-CONFIG-002 - Training Data Flywheel lacks storage details

---

## Overall Assessment: APPROVED

The Engineer's proposal provides a thorough, well-structured resolution that aligns with existing Casparian patterns. The schema design follows the project's database conventions, privacy handling builds correctly on the ISSUE-R11 work, and export formats are practical for the intended use cases.

---

## Issues Identified

### ISSUE-R14-001: Missing `ai_` Table Prefix Documentation in CLAUDE.md (Low)

**Location:** Section 2.1, Core Tables

**Problem:** The proposal introduces a new table prefix `ai_` but CLAUDE.md's Table Prefixes section (Database Architecture) does not include it. This should be added for consistency.

**Current CLAUDE.md prefixes:**
```
| cf_parsers        | Parser registry...           |
| cf_parser_topics  | Parser -> topic subscriptions |
| scout_*           | sources, files, tagging_rules |
| schema_*          | contracts, amendments         |
| backtest_*        | high_failure_files            |
```

**Recommendation:** Add to CLAUDE.md Table Prefixes section:
```
| ai_*              | Training examples, exports, field mappings | Training flywheel |
```

---

### ISSUE-R14-002: Foreign Key Without Existence Validation (Medium)

**Location:** Section 2.1, `rule_id TEXT NOT NULL -- FK to scout_tagging_rules(id)`

**Problem:** The schema declares `rule_id` as a FK reference but SQLite doesn't enforce FK constraints by default. If a tagging rule is deleted, orphaned training examples remain.

**Two concerns:**
1. `PRAGMA foreign_keys = ON;` must be set per connection
2. No explicit `ON DELETE CASCADE` for training examples when source rule is deleted

**Recommendation:** Either:
- Add `ON DELETE SET NULL` and make `rule_id` nullable (preserves training data even if rule deleted)
- Add `ON DELETE CASCADE` (removes training data when rule deleted - data loss)
- Document that FK enforcement is application-level (current Casparian pattern)

The proposal should explicitly state which approach is intended.

---

### ISSUE-R14-003: Confidence Score Source Undefined (Low)

**Location:** Section 2.1, `confidence_score REAL -- 0.0-1.0, null if unknown`

**Problem:** The schema includes `confidence_score` but the proposal doesn't specify how this value is computed. Questions:
- Is this the AI extraction wizard's confidence?
- Is this computed from validation results?
- Is this user-supplied?

**Recommendation:** Add a brief note in Section 5.1 specifying the source:
```
confidence_score: Set from AIExtractionWizard confidence
                  if available, otherwise NULL
```

---

### ISSUE-R14-004: Privacy Mode Consistency with ISSUE-R11 (Low)

**Location:** Section 3.1, Privacy Modes table

**Problem:** ISSUE-R11 defines privacy modes (strict/standard/permissive) with specific sanitization behaviors. This proposal reuses those names but defines slightly different field value behaviors:

| This Proposal | ISSUE-R11 |
|---------------|-----------|
| strict: All values hashed | strict: Critical+High+Medium redacted |
| permissive: Preserved (local only) | Not explicitly defined |

**Recommendation:** Reference ISSUE-R11 explicitly and clarify that training data modes extend the path sanitization modes:
```
Privacy modes align with GAP-PRIVACY-001 resolution (round_011).
Field value handling is an extension specific to training data.
```

---

### ISSUE-R14-005: Export JSONL Missing Schema Version (Medium)

**Location:** Section 4.2, Export Format (JSONL)

**Problem:** The example JSONL output lacks the `version` field that's shown in Section 2.2 JSON Schema:

```json
{"version":"1.0","type":"training_example"}  // Header has version
{"id":"ex-123","glob":"..."}                 // Records don't have version
```

If the export format evolves, importers need to know which schema version each record uses.

**Recommendation:** Include version in each record:
```json
{"id":"ex-123","version":"1.0","glob":"...","fields":[...]}
```

---

### ISSUE-R14-006: Trust Level Weight Semantics Unclear (Low)

**Location:** Section 4.4, Trust Levels table

**Problem:** The "Weight in Training" column shows 1.0, 0.8, 0.5, 0.2 but doesn't explain how these weights are applied. Is this:
- A multiplier on sample_count?
- A voting weight in ensemble models?
- A probability of inclusion?

**Recommendation:** Add a sentence clarifying:
```
Weight is applied as a multiplier to confidence_score when
using imported examples for pattern suggestion ranking.
```

---

### ISSUE-R14-007: `casparian ai training` Command Namespace (Low)

**Location:** Section 6, CLI Commands

**Problem:** The CLI uses `casparian ai training` as a subcommand. Current CLAUDE.md CLI examples show verb-first patterns:
- `casparian scan`
- `casparian run`
- `casparian backfill`

`casparian ai training list` breaks this pattern (noun-first).

**Recommendation:** Either:
- Accept the namespace pattern for the `ai` subsystem (reasonable for grouping)
- Use `casparian training-list`, `casparian training-export` (verb-first, flat)

The proposal's approach is acceptable if documented as intentional namespace grouping for AI-related commands.

---

### ISSUE-R14-008: Hash Function Not Specified for Field Value Hashing (Low)

**Location:** Section 3.3, `hash_sensitive_value` function

**Problem:** Uses `blake3::hash` which is correct and matches ADR patterns. However, this should reference the installation salt from ISSUE-R11-003 to prevent correlation attacks.

**Recommendation:** Reference ISSUE-R11-003 explicitly:
```rust
pub fn hash_sensitive_value(value: &str, field_name: &str, salt: &str) -> String {
    // Uses installation salt per ISSUE-R11-003
    let input = format!("{}:{}:{}", salt, field_name, value);
    ...
}
```

---

## Positive Observations

1. **Single Database Rule adherence** - Correctly uses `~/.casparian_flow/casparian_flow.sqlite3`
2. **Table prefix convention** - `ai_` prefix is logical and consistent with existing patterns
3. **Privacy-first design** - Sanitization at write time is correct (no raw paths ever stored)
4. **Export format choice** - JSONL is appropriate for streaming, human-readable, git-friendly exports
5. **Trust levels for imports** - Good forward-thinking for community/federated data
6. **Audit trail** - `redactions_applied_json` enables privacy compliance verification
7. **CLI ergonomics** - Commands are intuitive and complete

---

## Required Changes for Approval

| Issue | Severity | Required? |
|-------|----------|-----------|
| ISSUE-R14-001 | Low | No (housekeeping, can be done later) |
| ISSUE-R14-002 | Medium | Yes - FK behavior must be specified |
| ISSUE-R14-003 | Low | No (clarification nice-to-have) |
| ISSUE-R14-004 | Low | No (cross-reference nice-to-have) |
| ISSUE-R14-005 | Medium | Yes - export format should be version-stable |
| ISSUE-R14-006 | Low | No (implementation detail) |
| ISSUE-R14-007 | Low | No (namespace pattern acceptable) |
| ISSUE-R14-008 | Low | Yes - salt integration is security-relevant |

---

## Summary

The proposal is well-designed and ready for implementation with minor clarifications. The schema follows Casparian conventions, privacy handling integrates properly with the ISSUE-R11 work, and the export format is practical.

**Minor fixes needed:**
- ISSUE-R14-002: Specify FK delete behavior
- ISSUE-R14-005: Include version in JSONL records
- ISSUE-R14-008: Reference installation salt

These are low-effort fixes that don't require architectural changes.

**No blocking issues.** Proposal approved for implementation.

---

**Reviewer:** Spec Refinement Workflow
**Date:** 2026-01-13
**Status:** APPROVED (minor clarifications requested)

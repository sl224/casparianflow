# Extraction API Refinement - Session Summary

**Session:** extraction_api
**Rounds:** 2
**Outcome:** APPROVED + SPEC GENERATED

---

## Before vs After

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Spec count | 2 | 1 | -50% |
| Total lines | 3,024 | 580 | -81% |
| Concepts | Many (primitives, expressions, vocabulary, recognition, equivalence) | Few (rules, templates, inference) | Simplified |
| Entry point | Write YAML | Point at file | User-friendly |

---

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Spec structure | Merge into one | Single mental model |
| Single-file handling | Template-first | Inference needs 3+ samples |
| Semantic syntax | Optional for power users | Keep escape hatch |
| Equivalence classes | Keep in v1 | Algorithmic, no AI needed |
| AI features | Defer to v2 | Core works without AI |

---

## What Was Created

**`specs/extraction.md`** (580 lines) - Unified extraction spec with:
- Section 2: Tier 1 Simple API (template matching, algorithmic inference)
- Section 3: Tier 2 Advanced API (YAML, semantic syntax)
- Section 4: Equivalence Classes (cross-source rule sharing)
- Section 5: Inference Engine (algorithms)
- Section 6: Database Schema
- Section 9: Implementation Phases
- Appendices: Template definitions, semantic primitives

---

## Specs Superseded

- `specs/extraction_rules.md` → SUPERSEDED
- `specs/semantic_path_mapping.md` → SUPERSEDED

Both files marked with deprecation notices pointing to `extraction.md`.

---

## Remaining Gaps (for future work)

| Gap | Severity | Notes |
|-----|----------|-------|
| Template coverage validation | Medium | Need real-world testing |
| Equivalence class conflicts | Low | When source matches multiple classes |
| Confidence threshold config | Low | Configurable per-org |
| AI features | Deferred | v2 scope |

---

## Workflow Validation

This session demonstrated:
1. **Spec consolidation** - Two specs merged into one
2. **Simplification focus** - 81% line reduction
3. **User decisions drive direction** - Template-first, semantic optional
4. **2-round resolution** - Quick convergence on approved design

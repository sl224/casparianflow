# UI Specs Refinement - Source Documents

**Session:** ui_specs
**Started:** 2026-01-12
**Workflow:** specs/meta/spec_refinement_workflow_v2.md (v2.1)

---

## Specs Under Refinement

### Master Spec
- `specs/tui.md` (590 lines) - Master TUI specification

### View Specs
| Spec | Lines | Status |
|------|-------|--------|
| `specs/views/home.md` | 114 | Stub |
| `specs/views/discover.md` | 1430 | Comprehensive |
| `specs/views/parser_bench.md` | 720 | Good detail |
| `specs/views/jobs.md` | 123 | Stub |
| `specs/views/sources.md` | 148 | Stub |
| `specs/views/extraction.md` | 190 | Stub |

### Related Specs (context only, not being refined)
- `specs/extraction.md` - Extraction rules API (referenced by views/extraction.md)

---

## Goals

1. **Stub Expansion** - Flesh out stub specs (home, jobs, sources, extraction views)
2. **Structural Consistency** - Ensure all view specs follow same structure
3. **Compression** - Extract common patterns to master spec, eliminate duplication
4. **Cross-Reference Hygiene** - Fix broken/outdated references
5. **State Machine Coverage** - Add state machines where missing
6. **Implementation Alignment** - Sync implementation phases across specs

---

## Known Issues (Pre-Refinement)

1. `discover.md` is 10x larger than other view specs - likely over-scoped
2. Stubs lack state machines, data models, workflows
3. Potential duplicate pattern definitions (dropdowns, dialogs)
4. `views/extraction.md` references deprecated `extraction_rules.md`
5. Inconsistent implementation phase formats

---

## Semantic Compression Notes

Per workflow v2.1, we'll track compression candidates as we go:
- Dropdown pattern appears in tui.md and discover.md
- Dialog pattern appears in tui.md and discover.md
- Keybinding tables have inconsistent formats
- State structs may share common fields

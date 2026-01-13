# UI Specs Refinement - Decisions

**Session:** ui_specs
**Started:** 2026-01-12

---

## Foundational Decisions

### DEC-FOUND-001: Stub Expansion Priority
**Decision:** Home first
**Rationale:** Home is the entry point, sets patterns for other views
**Date:** 2026-01-12

### DEC-FOUND-002: Discover Scope
**Decision:** Move Extractors section to extraction.md
**Rationale:** Keep discover.md focused on file browsing/tagging only. Extractors are backend/API concerns.
**Impact:** GAP-SCOPE-001 resolved, discover.md Section 8 to be refactored
**Date:** 2026-01-12

### DEC-FOUND-003: Compression Timing
**Decision:** After stubs expanded
**Rationale:** Per v2.1 semantic compression rule - wait for 2+ concrete instances before extracting patterns
**Impact:** GAP-COMP-* gaps deferred until stubs complete
**Date:** 2026-01-12

### DEC-FOUND-004: Reference Resolution
**Decision:** Update to new extraction.md
**Rationale:** Semantic path mapping and extraction rules have been consolidated into specs/extraction.md
**Impact:** GAP-REF-001, GAP-REF-002, GAP-REF-003 will point to extraction.md
**Date:** 2026-01-12

---

## Round Decisions

*(To be populated as rounds progress)*

# Jobs Redesign - Decisions Log

**Session:** jobs_redesign
**Started:** 2026-01-13

---

## Decisions

### Round 1: Design Direction
**Date:** 2026-01-13
**Context:** Reviewer found Engineer re-solving problems in crystallized v1.0 spec. User chose direction.

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Focus area | Reopen design decisions | User wants pipeline visualization/data-states model - v1.0 was over-simplified |
| Backtest UI | Jobs view (unified) | Backtest appears alongside Scan/Parse/Export |
| Monitoring | Full monitoring panel | Inline throughput insufficient - need sink metrics, queue depth, trends |

**Impact:** The v1.0 spec needs revision to add:
1. Pipeline/data-state visualization (previously in "What We Explicitly Don't Do")
2. Backtest job type
3. Full monitoring panel (Section 15 from v0.2)

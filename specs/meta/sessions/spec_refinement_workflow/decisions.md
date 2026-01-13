# User Decisions - Spec Refinement Workflow Self-Refinement

**Session:** spec_refinement_workflow (self-referential)
**Started:** 2026-01-09

---

## Round 0 (Initial Setup)

### OQ-006: Instance Model
**Choice:** Separate processes
**Rationale:** True isolation via Task tool. Reviewer can't see Engineer's reasoning, only output.

### OQ-003: Human-in-the-loop
**Choice:** Mediator asks via AskUserQuestion tool
**Rationale:** Interactive, no manual file editing required.

### OQ-002: Async Support
**Choice:** Async optional
**Rationale:** Design for sync, but don't preclude async.

### OQ-005: Round Limit
**Choice:** Hard limit only if no human questions; otherwise human controls termination
**Rationale:** Human remains in control, but prevents runaway loops in automated mode.

### Priority Order
**Choice:** Flow gaps first
**Order:** Process Flow → Open Questions → Roles → Communication → QA → UX → Automation

### OQ-004: Output Format
**Choice:** Flexible markdown only
**Rationale:** Human-readable, no schema enforcement. Maximum flexibility.

### Scope
**Choice:** Full refinement (all 25 gaps)

### Token Optimization
**Choice:** Resume agents
**Rationale:** Keep Engineer/Reviewer agents alive across rounds, pass only deltas. Most token-efficient.

### Chunk Strategy
**Choice:** By dependency
**Rationale:** Resolve foundational gaps first (gap lifecycle, severity defs) that others depend on. Prevents circular dependency pileup.

---

## Round 1

### Engineer Output
- Addressed: GAP-FLOW-001 through GAP-FLOW-007
- Agent ID: a23d36e (for resume)

### Reviewer Output
- Approved: GAP-FLOW-003 (Handoff Mechanics)
- Critical issues found: 7
- Agent ID: a11e6c4 (for resume)

### Key Finding
Circular dependencies detected. Must resolve foundational gaps first:
1. Gap Lifecycle Definition
2. Severity Levels
3. Token Optimization Alignment

### User Decisions (Post-Round 1)

**Token Optimization Clarification:**
**Choice:** Resume means context-primed
**Definition:** Fresh Task each time, but prompt includes prior round summary as delta (not full re-read of all docs)

**Round 2 Scope:**
**Choice:** Foundations only - GAP-FLOW-010 (lifecycle) and GAP-FLOW-012 (severity)

---

## Round 2

*In progress*

---

## Decision Log

| Date | Round | Decision ID | Choice | Rationale |
|------|-------|-------------|--------|-----------|
| 2026-01-09 | 0 | OQ-006 | Separate processes | True isolation |
| 2026-01-09 | 0 | OQ-003 | AskUserQuestion | Interactive |
| 2026-01-09 | 0 | OQ-002 | Async optional | Flexible |
| 2026-01-09 | 0 | OQ-005 | Human-controlled limit | User in control |
| 2026-01-09 | 0 | Priority | Flow first | Unblocks process |
| 2026-01-09 | 0 | OQ-004 | Flexible markdown | Readable |
| 2026-01-09 | 0 | Scope | Full (25 gaps) | Complete refinement |

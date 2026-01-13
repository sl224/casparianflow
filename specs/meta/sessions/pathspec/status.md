# PathSpec Refinement Status

**Started:** 2026-01-09
**Source:** `specs/pathspec_partial.md`
**Target:** Complete, implementable PathSpec specification

---

## Gap Inventory

### Grammar Gaps
- [ ] **GAP-GRAMMAR-001**: Optional segment propagation unclear
- [ ] **GAP-GRAMMAR-002**: OneOf ambiguity resolution undefined
- [ ] **GAP-GRAMMAR-003**: Recursive depth limits unspecified
- [ ] **GAP-GRAMMAR-004**: Negative patterns (exclusions) missing
- [ ] **GAP-GRAMMAR-005**: Multi-root support undefined

### Type System Gaps
- [ ] **GAP-TYPE-001**: Normalization pipeline undefined
- [ ] **GAP-TYPE-002**: Type coercion rules missing
- [ ] **GAP-TYPE-003**: Validation error handling unspecified

### Merge Semantics Gaps
- [ ] **GAP-MERGE-001**: Merge vs Override not distinguished
- [ ] **GAP-MERGE-002**: Array merging undefined
- [ ] **GAP-MERGE-003**: Null handling missing

### Database Gaps
- [ ] **GAP-DB-001**: PathSpec versioning workflow undefined
- [ ] **GAP-DB-002**: Migration strategy for existing data missing
- [ ] **GAP-DB-003**: Index strategy for JSON columns unspecified

### TUI Gaps
- [ ] **GAP-TUI-001**: No wireframes for PathSpec editor
- [ ] **GAP-TUI-002**: No keybindings for view switching
- [ ] **GAP-TUI-003**: No Interactive Breadcrumb spec
- [ ] **GAP-TUI-004**: No anomaly resolution workflow UI
- [ ] **GAP-TUI-005**: No PathSpec validation feedback display

### AI Integration Gaps
- [ ] **GAP-AI-001**: Pathfinder Wizard → PathSpec generation not specified
- [ ] **GAP-AI-002**: No "Mad Libs" template system
- [ ] **GAP-AI-003**: No PathSpec suggestion workflow

---

## Issues

- [ ] **ISSUE-ANOMALY-001** [HIGH]: Proposed actions contradict read-only constraint
- [ ] **ISSUE-ANOMALY-002** [MEDIUM]: Missing Mandatory requires `required` field
- [ ] **ISSUE-ANOMALY-003** [MEDIUM]: Severity consequences undefined

---

## Open Questions

- [ ] **OQ-001**: PathSpec per-source or global?
- [ ] **OQ-002**: OneOf multiple match behavior?
- [ ] **OQ-003**: Should anomalies block processing?
- [ ] **OQ-004**: Inline types or registry only?
- [ ] **OQ-005**: PathSpec file location?

---

## Round Progress

| Round | Gaps In | Resolved | New | Gaps Out | Decisions Made |
|-------|---------|----------|-----|----------|----------------|
| 0 | 18 | - | - | 18 | (initial) |

---

## Blocking Items

These must be resolved before implementation can begin:

1. **GAP-GRAMMAR-001** - Cannot implement optional folders
2. **GAP-GRAMMAR-002** - Non-deterministic matching
3. **ISSUE-ANOMALY-001** - Architecture violation
4. **GAP-MERGE-001** - Unpredictable data behavior

---

## Priority Order (Suggested)

1. Grammar core (GRAMMAR-001, 002) - unblocks implementation
2. Anomaly issues (ANOMALY-001, 002, 003) - architecture integrity
3. Merge semantics (MERGE-001, 002, 003) - data correctness
4. Type system (TYPE-001, 002, 003) - validation behavior
5. Database (DB-001, 002, 003) - persistence layer
6. TUI (TUI-001-005) - user interface
7. AI Integration (AI-001-003) - optional enhancement

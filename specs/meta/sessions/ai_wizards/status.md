# AI Wizards Spec Refinement - Status

**Source:** specs/ai_wizards.md
**Started:** 2026-01-13
**Current Round:** 29 (ALL MEDIUM gaps complete)
**Convergence State:** CONVERGING

---

## Gap Inventory

### CRITICAL (7 gaps, weight 112)

| ID | Description | State | Assigned |
|----|-------------|-------|----------|
| GAP-STATE-001 | Pathfinder Wizard has no state machine | RESOLVED | Round 1 |
| GAP-STATE-002 | Parser Wizard (Parser Lab) has no state machine | RESOLVED | Round 2 |
| GAP-STATE-003 | Labeling Wizard has no state machine (no TUI at all) | RESOLVED | Round 3 |
| GAP-STATE-004 | Semantic Path Wizard has no state machine | RESOLVED | Round 4 |
| GAP-STATE-005 | Draft Lifecycle state machine incomplete (no triggers) | RESOLVED | Round 5 |
| GAP-TRANS-001 | Wizard menu (W) transitions undefined | RESOLVED | Round 6 |
| GAP-DATA-001 | cf_signature_groups table referenced but not defined | RESOLVED | Round 6 |

### HIGH (12 gaps, weight 48)

| ID | Description | State | Assigned |
|----|-------------|-------|----------|
| GAP-INT-001 | Path Intelligence Engine has no TUI integration | RESOLVED | Round 15 |
| GAP-TUI-001 | $EDITOR subprocess handling not specified | RESOLVED | Round 12 |
| GAP-INT-002 | YAML vs Python output decision algorithm undefined | RESOLVED | Round 7 |
| GAP-INT-003 | User hint parsing - LLM enhancement needed | RESOLVED | Round 16 |
| GAP-INT-004 | Complexity thresholds not configurable | RESOLVED | Round 18 |
| GAP-INT-005 | Python extractor validation not specified | RESOLVED | Round 17 |
| GAP-FLOW-001 | Wizard invocation from Files panel underspecified | RESOLVED | Round 8 |
| GAP-FLOW-002 | Semantic Path Wizard invocation unclear | RESOLVED | Round 19 |
| GAP-MODEL-001 | Draft ID generation not specified | RESOLVED | Round 10 |
| GAP-ERROR-001 | Invalid YAML from LLM - no retry mechanism | RESOLVED | Round 9 |
| GAP-PRIVACY-001 | Path normalization may leak sensitive data | RESOLVED | Round 11 |
| GAP-PIE-001 | Path Intelligence phases have no success criteria | RESOLVED | Round 13 |
| GAP-CONFIG-002 | Training data flywheel storage undefined | RESOLVED | Round 14 |

### MEDIUM (11 gaps, weight 22)

| ID | Description | State | Assigned |
|----|-------------|-------|----------|
| GAP-KEY-001 | Keybinding conflict Wizard menu vs Discover | RESOLVED | Round 20 |
| GAP-EX-001 | Hint dialog has no character limit | RESOLVED | Round 21 |
| GAP-EX-002 | Manual edit mode no error handling | RESOLVED | Round 22 |
| GAP-AUDIT-001 | Audit log retention policy undefined | RESOLVED | Round 23 |
| GAP-PIE-002 | Clustering "unclustered" threshold undefined | RESOLVED | Round 24 |
| GAP-PIE-003 | Single-file confidence factors computation unclear | RESOLVED | Round 25 |
| GAP-MCP-001 | MCP tools output mismatch (code_preview vs YAML) | RESOLVED | Round 26 |
| GAP-FLYWHEEL-001 | Training data storage location undefined | RESOLVED | Round 14 |
| GAP-CONFIG-001 | Config defaults vs code defaults unclear | RESOLVED | Round 27 |
| GAP-HYBRID-001 | Hybrid mode (Pathfinder + Semantic) no workflow | RESOLVED | Round 28 |
| GAP-EMBED-001 | Embedding model download/fallback not specified | RESOLVED | Round 29 |

### LOW (7 gaps, weight 7)

| ID | Description | State | Assigned |
|----|-------------|-------|----------|
| GAP-TERM-001 | "Path Intelligence Engine" terminology inconsistent | OPEN | - |
| GAP-VER-001 | Version mismatch header vs revision history | OPEN | - |
| GAP-REF-001 | specs/extraction_rules.md reference broken | OPEN | - |
| GAP-REF-002 | specs/semantic_path_mapping.md reference broken | OPEN | - |
| GAP-EX-003 | Redaction dialog no auto-detection | OPEN | - |
| GAP-EXAMPLE-001 | Parser pattern inconsistency with ADR-015 | OPEN | - |
| GAP-CLI-001 | audit command missing format options | OPEN | - |

---

## Metrics

| Metric | Value |
|--------|-------|
| Total Gaps | 41 |
| Open Gaps | 7 |
| Weighted Open | 7 |
| Rounds Completed | 29 |

---

## Round History

| Round | Gaps Addressed | Resolved | New | Net | State |
|-------|----------------|----------|-----|-----|-------|
| 0 | Initial Analysis | 0 | 34 | -34 | INITIAL |
| 1 | GAP-STATE-001 | 1 | 3 | -2 | CONVERGING |
| 2 | GAP-STATE-002 | 1 | 4 | -3 | CONVERGING |
| 4 | GAP-STATE-004 | 1 | 5 | -4 | CONVERGING |
| 5 | GAP-STATE-005 | 1 | 0 | +1 | CONVERGING |
| 6 | GAP-TRANS-001, GAP-DATA-001 | 2 | 0 | +2 | CONVERGING |
| 7 | GAP-INT-002 | 1 | 3 | -2 | CONVERGING |
| 8 | GAP-FLOW-001 | 1 | 0 | +1 | CONVERGING |
| 9 | GAP-ERROR-001 | 1 | 0 | +1 | CONVERGING |
| 10 | GAP-MODEL-001 | 1 | 0 | +1 | CONVERGING |
| 11 | GAP-PRIVACY-001 | 1 | 0 | +1 | CONVERGING |
| 12 | GAP-TUI-001 | 1 | 0 | +1 | CONVERGING |
| 13 | GAP-PIE-001 | 1 | 0 | +1 | CONVERGING |
| 14 | GAP-CONFIG-002 | 1 | 0 | +1 | CONVERGING |
| 15 | GAP-INT-001 | 1 | 0 | +1 | CONVERGING |
| 16 | GAP-INT-003 | 1 | 0 | +1 | CONVERGING |
| 17 | GAP-INT-005 | 1 | 0 | +1 | CONVERGING |
| 18 | GAP-INT-004 | 1 | 0 | +1 | CONVERGING |
| 19 | GAP-FLOW-002 | 1 | 0 | +1 | CONVERGING |
| 20 | GAP-KEY-001 | 1 | 0 | +1 | CONVERGING |
| 21 | GAP-EX-001 | 1 | 0 | +1 | CONVERGING |
| 22 | GAP-EX-002 | 1 | 0 | +1 | CONVERGING |
| 23 | GAP-AUDIT-001 | 1 | 0 | +1 | CONVERGING |
| 24 | GAP-PIE-002 | 1 | 0 | +1 | CONVERGING |
| 25 | GAP-PIE-003 | 1 | 0 | +1 | CONVERGING |
| 26 | GAP-MCP-001 | 1 | 0 | +1 | CONVERGING |
| 27 | GAP-CONFIG-001 | 1 | 0 | +1 | CONVERGING |
| 28 | GAP-HYBRID-001 | 1 | 0 | +1 | CONVERGING |
| 29 | GAP-EMBED-001 | 1 | 0 | +1 | CONVERGING |

### New Gaps from Round 1
- GAP-TUI-001: $EDITOR subprocess handling (HIGH)
- GAP-YAML-001: Rule file naming convention (MEDIUM)
- GAP-FOCUS-001: Focus management in result state (MEDIUM)

### New Gaps from Round 2
- GAP-SCHEMA-001: Schema Input UI details (MEDIUM)
- GAP-TEST-001: Testing file selection mechanism (MEDIUM)
- GAP-VERSION-001: Parser version conflict handling (HIGH)

### New Gaps from Round 4
- GAP-SEMANTIC-001: Confidence score computation not specified (HIGH)
- GAP-SEMANTIC-002: Similar sources matching algorithm (HIGH)
- GAP-SEMANTIC-003: Alternative generation strategy (MEDIUM)
- GAP-SEMANTIC-004: Low confidence approval confirmation (MEDIUM)
- GAP-SEMANTIC-005: Semantic vocabulary reference (specs/semantic_path_mapping.md) (HIGH)

### New Gaps from Round 7
- GAP-INT-003: User hint parsing - LLM enhancement needed (HIGH)
- GAP-INT-004: Complexity thresholds not configurable (MEDIUM)
- GAP-INT-005: Python extractor validation not specified (HIGH)

---

## Notes

- All four wizards lack TUI state machines (CRITICAL per workflow v2.3 Section 15)
- Path Intelligence Engine (Section 3.5) is technically specified but has no UI
- Two cross-references point to non-existent specs (likely renamed/merged)

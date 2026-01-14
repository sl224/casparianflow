# Reviewer Assessment: Path Intelligence Engine TUI Integration

**Gap:** GAP-INT-001 - Path Intelligence Engine has no TUI integration
**Engineer Submission:** Section 3.5.12 TUI Integration
**Reviewer:** System Reviewer
**Date:** 2026-01-13

---

## Executive Summary

**VERDICT: APPROVED_WITH_NOTES**

The Engineer's proposal is **comprehensive and well-structured**, providing complete TUI integration for the Path Intelligence Engine's clustering workflow. The specification effectively addresses the gap with clear entry points, a well-designed state machine, and proper integration points with existing wizards.

However, there are **4 minor clarifications needed** before full integration. None are blockers, but each requires brief architectural discussion.

---

## Detailed Assessment

### 1. COMPLETENESS: EXCELLENT

**Criterion:** Does it fully address the gap? Are all entry points, states, and transitions defined?

**Findings:**

✓ **All three entry points defined:**
- Files Panel (`C`) - clusters filtered file set
- Sources Manager (`c`) - clusters entire source
- AI Wizards Menu (`W` then `1`) - full wizard flow

✓ **Complete state machine (5 states):**
- `Clustering` → progress UI
- `Overview` → cluster list view
- `Expanded` → single cluster detail
- `Editing` → field modification
- `Accepted` → confirmation + auto-advance

✓ **Comprehensive transition table** with all conditions

✓ **Data model fully specified** with Rust structs

✓ **Implementation phases clearly ordered** with dependencies

**Minor Gap Identified:**

**GAP-1: Cluster Progress Persistence**

The state machine doesn't specify what happens if clustering is **cancelled mid-progress** (user presses `Esc` during `Clustering` state):

```
| Clustering | Esc | Discover | Any | Cancel clustering
```

**Questions:**
- Are partial results saved to database?
- Can user resume clustering later?
- Or does cancellation discard everything?

**Recommendation:** Add brief clarification in Section 3.5.12.3 State Definitions:
```
Clustering:
  - Auto-advances to Overview on complete
  - Esc cancels: discards results, returns to Discover
  - No checkpointing (re-cluster from scratch if needed)
```

---

### 2. CONSISTENCY: VERY GOOD

**Criterion:** Does it align with existing TUI patterns in specs/views/discover.md?

**Alignment Analysis:**

✓ **Dropdown pattern:** Overlay dialogs match Rules Manager and Sources Manager from discover.md Section 3.4-3.5

✓ **Navigation keybindings:** Consistent use of `j`/`k`, letter quick-jump, Tab navigation

✓ **State machine hierarchy:** Overlay modal pattern matches existing dialogs (RULES_MANAGER, SOURCES_MANAGER)

✓ **Preview panel:** Similar to Files Panel preview toggle (`p` key)

✓ **Acceptance flow:** Matches existing "create rule → confirm → files tagged" pattern from tagging rules workflow

**Minor Inconsistency:**

**INCONSISTENCY-1: Keybinding `r` overload**

The proposal uses `r` for two different contexts:
- In Unclustered Files view: `r` = "Re-cluster with hints" (Section 3.5.12.8)
- In Overview: `r` = "Create rule immediately" (Section 3.5.12.10)

In discover.md, there's no current `r` binding, so no collision with existing keys. However, this creates potential confusion if the same modal ever shows both contexts.

**Recommendation:** Change Unclustered handling `r` → `x` (re-cluster) to avoid semantic collision. Rationale:
- `x` suggests "retry/repeat"
- `r` should stay reserved for "create Rule immediately" (clearer semantic)
- Alternative: Use `Ctrl+r` for re-cluster if X is unavailable

**INCONSISTENCY-2: Help text placement**

discover.md has a consistent help footer style:
```
|  [j/k] Navigate  [↑↓]arrows  [Enter]select  [Esc]back   |
```

The proposal uses multi-line help in some views (Expanded Cluster, Editing Mode) which is correct, but should verify that help text is **always placed at bottom**, never mixed within panel content.

**Status:** Minor formatting—no architectural impact.

---

### 3. IMPLEMENTABILITY: VERY GOOD

**Criterion:** Can this be implemented as specified? Are there ambiguities?

**Implementability Review:**

✓ **Data structures are concrete** - Rust types with all fields defined

✓ **State transitions are unambiguous** - Transition table is complete

✓ **Layout specs use ASCII diagrams** - Clear visual reference

✓ **Keybinding table is exhaustive** - No gaps in coverage

**Minor Ambiguities:**

**AMBIGUITY-1: File list scrolling in Expanded view**

Section 3.5.12.4 shows file list pagination `[Showing 1-7 of 247]`. The spec mentions:
- `j`/`k` navigate within file list
- Preview updates on file selection

**Questions:**
1. Does scrolling the file list **auto-update preview panel**? (Likely yes, but implicit)
2. What happens if user navigates past visible files? (Auto-scroll list?)
3. Does preview panel have its own scrolling for large files?

**Recommendation:** Add explicit note to Section 3.5.12.4:

```
**File List Navigation:**
- j/k moves cursor within visible list
- When navigating past visible items, list auto-scrolls
- Preview panel updates immediately on cursor movement
- For files >2000 lines, preview shows first 50 + scrollbar
```

**AMBIGUITY-2: Glob validation in Editing mode**

Section 3.5.12.5 shows "Live: 247 matches" when editing glob pattern. But:
1. **When is validation triggered?** On every keystroke? On Enter? On Tab?
2. **Is glob revalidation against files in current cluster only, or ALL files?**
3. **What if glob matches FEWER files than original cluster?** (User can reduce scope inadvertently)

**Recommendation:** Clarify in Editing Mode section:

```
**Glob Pattern Editing:**
- Validation runs on every keystroke (not Enter)
- Live count shows matches against ALL files in source
  (not just current cluster)
- Warning if final glob matches <50% of original cluster:
  "Reduced from 247 to 89 files. [Restore] or [Continue]?"
```

**AMBIGUITY-3: Field edit interface in Editing mode**

Section 3.5.12.5 shows field navigation but doesn't specify how to edit individual fields:
- Can user edit field **name**? (Likely no - name is derived)
- Can user edit field **source**? (segment(-3) → segment(-2)?)
- Can user edit field **type**? (string → integer?)
- Can user edit field **pattern**?

**Recommendation:** Clarify "Edit field" flow:

```
When user presses Enter on selected field in Fields section:
┌─ EDIT FIELD: department ──────────────────────────────────┐
│                                                            │
│ Source:  segment(-4)  [E] Edit                            │
│ Pattern: [A] Add custom regex pattern                     │
│ Type:    string  [E] Edit type                            │
│                                                            │
│ [Enter] Save  [Esc] Cancel                                │
└────────────────────────────────────────────────────────────┘

- Source: Can't edit (immutable - derived from cluster)
- Type: Can change to: string, integer, date, uuid, float
- Pattern: Can add optional regex refinement
```

---

### 4. INTEGRATION: EXCELLENT

**Criterion:** Does it properly integrate with Pathfinder/Semantic Path Wizards as mentioned in Section 3.5.6?

**Integration Analysis:**

✓ **Section 3.5.12.7 clearly specifies wizard handoff flow** with ASCII diagram

✓ **Three wizard invocations defined:**
- `w` → Pathfinder (generates extractor/YAML rule)
- `s` → Semantic Path (recognizes folder patterns)
- `l` → Labeling (suggests tag names)

✓ **Pre-fill strategy is clear:**
- Pathfinder receives: sample paths (5 from cluster), proposed fields, field types
- Returns: generated rule or Python extractor
- Cluster proposal updates with wizard output

✓ **Wizard menu layout specified** (Section 3.5.12.7)

✓ **Return flow defined:** Wizard output merges back into cluster review context

**Integration Cross-Check:**

Looking at ai_wizards.md Section 3 (The Four Wizards):

- **Pathfinder Wizard (3.1):** Takes sample paths + hints → generates YAML/Python ✓ Spec integrates correctly
- **Semantic Path Wizard (3.4):** Takes sample paths + hints → generates YAML + semantic expression ✓ Spec integrates correctly
- **Labeling Wizard (3.3):** Takes signature group + headers/samples → suggests tag name ✓ Spec integrates correctly

**One Enhancement Opportunity (Not a Gap):**

**ENHANCEMENT-1: Semantic Path pre-selection**

When user invokes Semantic Path wizard (`s`) from a cluster, the spec could **pre-detect if cluster matches a known semantic primitive** and auto-populate the wizard form:

```
Example: Cluster shows pattern like:
  /data/mission_042/2024-01-15/telemetry.csv
  /data/mission_043/2024-01-16/readings.csv

Semantic Path Wizard could pre-fill:
  - Detected primitive: entity_folder(mission)
  - Detected primitive: dated_hierarchy(iso)
  - Suggested tag: mission_data

User can confirm or refine.
```

**Recommendation:** This is a UX enhancement, not required for v1. Could be added in Phase 6 as a "quick suggestion" feature.

---

### 5. STATE MACHINE: EXCELLENT

**Criterion:** Is the state machine complete and consistent?

**State Machine Analysis:**

✓ **5 states clearly defined** with entry/exit conditions

✓ **All transitions accounted for** in transition table

✓ **No ambiguous transitions:**
- Each (state, key) pair has exactly one outcome
- Conditions are explicit (e.g., "Cluster selected", "Next exists")

✓ **State hierarchy is sensible:**
```
Clustering (progress)
    ↓
Overview (list view) ←→ Expanded (detail) ←→ Editing (form)
    ↓
Accepted (confirmation)
    ↓
back to Overview or Discover
```

✓ **Auto-advancement logic is specified:**
- Clustering → Overview (on complete)
- Accepted → next cluster (with `n` key) OR Overview (if none left)

**Potential State Machine Edge Case:**

**EDGE-CASE-1: User edits cluster, then presses Esc**

State machine transition table (Section 3.5.12.3):
```
| Editing | Esc | Overview/Expanded | Any | Discard changes |
```

Question: **Which state does Esc return to?** Overview or Expanded?

**Current spec:** Says "Overview/Expanded" but doesn't specify which.

**Recommendation:** Clarify to:
```
| Editing | Esc | (same state as origin) | Any | Discard changes |
```

Implementation note: Store `editing_from` enum (from_overview | from_expanded) to return correctly.

This matches the discover.md pattern where dialogs return to their parent state, not to a fixed state.

---

### 6. DATA MODEL: EXCELLENT

**Criterion:** Are the Rust structures sufficient and well-designed?

**Data Model Review:**

✓ **DiscoverState extensions are clean:**
- `cluster_overlay_open: bool` - toggle open/closed
- `cluster_state: ClusterReviewState` - current state enum
- `clusters: Vec<PathCluster>` - cluster data
- `selected_cluster: usize` - cursor position
- `unclustered_files: Vec<FileInfo>` - outliers
- `clustering_progress: Option<ClusteringProgress>` - progress tracking

✓ **PathCluster struct is complete:**
- Identifiers: `id`, `name`
- Metrics: `file_count`, `similarity_score`, `confidence`
- Data: `representative_paths`, `file_ids`
- Proposals: `proposed_glob`, `proposed_tag`, `proposed_fields`
- State: `is_expanded`, `is_accepted`, `is_edited`

✓ **ProposedField is well-designed:**
- Covers all field types: segment, filename, full_path, rel_path
- Includes confidence scoring
- Sample values for preview

✓ **ClusteringStage enum enables progress tracking:**
- Loading, Embedding, Clustering, Analyzing, Complete
- Allows UI to show meaningful progress messages

**Minor Data Model Suggestion (Not Required):**

**SUGGESTION-1: Track edit history**

For undo/rollback, consider adding to PathCluster:
```rust
pub original_proposal: ProposedCluster,  // snapshot before edit
pub current_proposal: ProposedCluster,   // live edit
```

**Status:** Enhancement only. Current design (just modify in place) is fine for v1.

---

## Critical Path Testing Requirements

Based on code_execution_workflow.md standards, before implementation:

**Phase 1 (Clustering trigger + progress UI):**
- [ ] Test: Pressing `C` from Files panel opens Clustering state
- [ ] Test: Progress bar updates with realistic timing
- [ ] Test: Esc cancels clustering (confirmed via test)
- [ ] Test: Auto-transition to Overview when complete

**Phase 2 (Cluster list view):**
- [ ] Test: All clusters render correctly
- [ ] Test: j/k navigation moves cursor
- [ ] Test: Letter quick-jump (A-Z) works
- [ ] Test: Cluster counts are accurate

**Phase 3 (Expanded view):**
- [ ] Test: Enter/Space expands cluster
- [ ] Test: File list scrolls (j/k within list)
- [ ] Test: Preview panel updates on file selection
- [ ] Test: Esc returns to Overview

**Phase 4 (Acceptance flow):**
- [ ] Test: `a` creates rule with correct glob/fields/tag
- [ ] Test: Files are tagged in database
- [ ] Test: Auto-advance to next cluster

**Phase 5 (Editing mode):**
- [ ] Test: Tab navigation through sections (Glob → Tag → Fields)
- [ ] Test: Live glob validation updates match count
- [ ] Test: Enter saves, Esc cancels
- [ ] Test: Esc returns to origin state (Overview vs Expanded)

**Phase 6 (Wizard integration):**
- [ ] Test: `w` launches Pathfinder with correct pre-fills
- [ ] Test: Pathfinder output merges back into cluster proposal
- [ ] Test: `s` and `l` work similarly

**Phase 7 (Unclustered handling):**
- [ ] Test: Unclustered section appears with <100% clustered files
- [ ] Test: `h` opens hints dialog
- [ ] Test: Re-clustering with hints triggers new cluster run

---

## Final Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| **Completeness** | ✓ Excellent | All entry points, states, transitions defined. Minor progress persistence question. |
| **Consistency** | ✓ Very Good | Aligns with discover.md patterns. Minor keybinding clarification needed. |
| **Implementability** | ✓ Very Good | Data structures concrete. 3 minor ambiguities (scrolling, validation timing, field editing). |
| **Integration** | ✓ Excellent | Proper handoff with all three wizards (Pathfinder, Semantic, Labeling). |
| **State Machine** | ✓ Excellent | Complete, unambiguous. Minor clarification on Esc return-to-state behavior. |

---

## Summary of Required Clarifications (Before Integration)

### MUST CLARIFY (Blocking):

1. **GAP-1: Cluster progress cancellation** - What happens if user presses Esc during Clustering?
   - Recommendation: Add "No checkpointing, restart from scratch if needed"

2. **AMBIGUITY-2: Glob validation timing** - When does validation trigger during edit?
   - Recommendation: Specify "on every keystroke" + handling for reduced match count

### SHOULD CLARIFY (Non-blocking, UX clarity):

3. **AMBIGUITY-1: File list scrolling behavior** - Auto-scroll, preview update timing?
   - Recommendation: Add explicit note about auto-scroll and preview responsiveness

4. **AMBIGUITY-3: Field edit interface** - Which field properties are editable?
   - Recommendation: Specify source (immutable), type (editable), pattern (editable)

### MINOR STYLE NOTES (No action needed):

5. **INCONSISTENCY-1: Keybinding `r` overload** - Consider `x` for re-cluster to avoid confusion
6. **EDGE-CASE-1: Esc return state in Editing** - Clarify "return to origin state, not fixed state"

---

## Recommendation

**✓ APPROVED_WITH_NOTES**

The specification is **production-ready** with the 4 clarifications above. These are not architectural gaps—they're routine spec refinements that emerge during review.

**Integration Path:**
1. Engineer responds with clarifications for GAPS/AMBIGUITIES 1-4
2. Reviewer confirms responses
3. Proceed to implementation

**Estimated Implementation Effort:**
- Phase 1-3: 2-3 weeks (clustering, list, detail views)
- Phase 4-5: 1-2 weeks (acceptance, editing)
- Phase 6: 1 week (wizard integration)
- Phase 7: 1 week (unclustered handling)
- **Total:** 5-8 weeks with concurrent development

**Risk Assessment: LOW**

No architectural risks identified. This is a well-designed feature that slots cleanly into existing TUI patterns.

---

## Appendix: Cross-Reference Validation

**Checked against:**
- ✓ specs/views/discover.md (Dropdown patterns, state machine, help text)
- ✓ specs/ai_wizards.md (Wizard integration, Pathfinder/Semantic/Labeling outputs)
- ✓ specs/extraction.md (Path extraction rules, field proposals)
- ✓ CLAUDE.md (TUI development patterns, debug workflow)
- ✓ code_execution_workflow.md (Testing standards)

**All cross-references valid. No conflicts detected.**

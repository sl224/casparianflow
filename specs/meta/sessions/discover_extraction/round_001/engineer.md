# Engineer Response: Round 001

**Date:** 2026-01-13
**Focus:** CRITICAL gaps GAP-STATE-001, GAP-TRANS-001
**Engineer Role:** Resolve state machine and transition specification gaps

---

## Gap Resolution: GAP-STATE-001

**Gap:** State machine not updated in Section 13.3 - Phase 18a defines new states (EditRule, Testing, Publishing, Published) but Section 13.3 diagram only shows Explore/Focused. Need unified state diagram.

**Confidence:** HIGH

### Proposed Solution

Replace Section 13.3 with a unified state machine that incorporates all states from Phase 18a while maintaining the existing Browse/Filtering behavior.

#### Unified State Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          GLOB EXPLORER STATE MACHINE                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌──────────────────────────── NAVIGATION LAYER ────────────────────────────┐   │
│  │                                                                           │   │
│  │   ┌─────────────┐    l/Enter     ┌─────────────┐                          │   │
│  │   │   BROWSE    │───────────────►│   BROWSE    │                          │   │
│  │   │  (at root)  │                │ (in folder) │                          │   │
│  │   │             │◄───────────────│             │                          │   │
│  │   └──────┬──────┘   h/Backspace  └──────┬──────┘                          │   │
│  │          │                               │                                │   │
│  │          │ / (start typing)              │ / (start typing)               │   │
│  │          ▼                               ▼                                │   │
│  │   ┌─────────────┐                 ┌─────────────┐                         │   │
│  │   │  FILTERING  │                 │  FILTERING  │                         │   │
│  │   │ (heat map)  │                 │ (in folder) │                         │   │
│  │   │             │◄───────────────►│             │                         │   │
│  │   └──────┬──────┘  l/Enter, h     └──────┬──────┘                         │   │
│  │          │                               │                                │   │
│  │          │ Esc (clear pattern, stay in BROWSE)                            │   │
│  │          ▼                               │                                │   │
│  │   [Return to BROWSE at current prefix]   │                                │   │
│  │                                          │                                │   │
│  └──────────────────────────────────────────┼────────────────────────────────┘   │
│                                              │                                    │
│             e (with matches > 0)             │ e (with matches > 0)              │
│                       │                      │                                    │
│                       └──────────┬───────────┘                                    │
│                                  ▼                                                │
│  ┌───────────────────────── RULE EDITING LAYER ─────────────────────────────┐    │
│  │                                                                           │    │
│  │   ┌─────────────────────────────────────────────────────────────────┐    │    │
│  │   │                         EDIT_RULE                                │    │    │
│  │   │   Glob pattern | Fields | Base tag | Conditions                  │    │    │
│  │   │   (Tab cycles sections, j/k navigates within)                    │    │    │
│  │   └──────────────────────────────┬──────────────────────────────────┘    │    │
│  │                                   │                                       │    │
│  │         ┌─────────────────────────┼─────────────────────────┐            │    │
│  │         │                         │                         │            │    │
│  │         │ t (test)                │ Esc (cancel)            │            │    │
│  │         ▼                         ▼                         │            │    │
│  │   ┌─────────────┐          [Return to BROWSE]               │            │    │
│  │   │   TESTING   │           (preserves prefix)              │            │    │
│  │   │  Running... │                                           │            │    │
│  │   │  Complete   │                                           │            │    │
│  │   └──────┬──────┘                                           │            │    │
│  │          │                                                  │            │    │
│  │          │ p (publish, from Complete)                       │            │    │
│  │          │ e (edit, return to EDIT_RULE)                    │            │    │
│  │          │ Esc (cancel, to BROWSE)                          │            │    │
│  │          ▼                                                  │            │    │
│  │   ┌─────────────┐                                           │            │    │
│  │   │  PUBLISHING │                                           │            │    │
│  │   │ Confirming  │───── Esc ─────────────────────────────────┘            │    │
│  │   │ Saving      │                                                        │    │
│  │   │ Starting    │                                                        │    │
│  │   └──────┬──────┘                                                        │    │
│  │          │                                                               │    │
│  │          │ (auto-transition on success)                                  │    │
│  │          ▼                                                               │    │
│  │   ┌─────────────┐                                                        │    │
│  │   │  PUBLISHED  │                                                        │    │
│  │   │ Complete!   │────── Enter/Esc ──────► [Return to BROWSE at root]     │    │
│  │   │ Job ID: xxx │                                                        │    │
│  │   └─────────────┘                                                        │    │
│  │                                                                          │    │
│  └──────────────────────────────────────────────────────────────────────────┘    │
│                                                                                   │
│   g/Esc from BROWSE/FILTERING → Exit Glob Explorer (return to Discover)          │
│                                                                                   │
└───────────────────────────────────────────────────────────────────────────────────┘
```

#### State Definitions Table (Updated)

| State | Entry Condition | Exit Conditions | Preserves Context |
|-------|-----------------|-----------------|-------------------|
| `Browse` | Default, Esc from Filtering, Enter/Esc from Published | `l`/Enter→drill, `/`→Filtering, `e`→EditRule, `g`/Esc→exit | prefix: Yes |
| `Filtering` | `/` from Browse | Esc→Browse, `l`→drill, `e`→EditRule | prefix: Yes, pattern: Yes |
| `EditRule` | `e` from Browse/Filtering (when matches > 0) | `t`→Testing, Esc→Browse | prefix: Yes, pattern: as glob |
| `Testing` | `t` from EditRule | `p`→Publishing, `e`→EditRule, Esc→Browse | rule draft: Yes |
| `Publishing` | `p` from Testing (Complete) | auto→Published, Esc→EditRule | rule draft: Yes |
| `Published` | auto from Publishing (success) | Enter/Esc→Browse (root) | None (clean slate) |

#### Validation Rules

1. **REACHABILITY**: All states reachable from Browse via documented key sequences
2. **ESCAPABILITY**: All states have path back to Browse (Esc chain)
3. **DETERMINISM**: Each (state, key) pair maps to exactly one action
4. **COMPLETENESS**: All key presses in all states have defined behavior
5. **CONSISTENCY**: Similar keys have similar meanings across states (`Esc` always cancels/goes back)

### Examples

**Example 1: Full rule creation flow**
```
Browse (root)
  → "/" type "**/*.csv"
  → Filtering (showing 847 matches)
  → "l" drill into /data folder
  → Filtering (in folder, 234 matches)
  → "e" (matches > 0, valid trigger)
  → EditRule (glob pre-filled with "data/**/*.csv", fields inferred)
  → "t" test
  → Testing (progress... complete, 95% success)
  → "p" publish
  → Publishing (confirming...)
  → Enter to confirm
  → Publishing (saving... starting job...)
  → Published (Job ID shown)
  → Enter
  → Browse (back at root, fresh start)
```

**Example 2: Cancel mid-edit**
```
Filtering → "e" → EditRule
EditRule → Esc
→ Browse (preserves prefix, clears pattern)
```

**Example 3: Edit after test failure**
```
Testing (70% failed) → "e"
→ EditRule (rule draft preserved, can fix pattern)
→ "t" re-test
→ Testing (now 95% success)
```

### Trade-offs

**Pros:**
- Single unified diagram, no ambiguity
- Clear separation: Navigation Layer vs Rule Editing Layer
- Context preservation rules explicit
- Two-layer model matches mental model (explore vs create)

**Cons:**
- More complex than original Browse/Filtering diagram
- Requires updating Section 13.3 AND removing redundant Phase 18a diagram

### New Gaps Introduced

- **GAP-CTX-001 (LOW)**: Need to specify exactly what "prefix" means when returning to Browse from different states. Is it the path prefix (e.g., `/data/`) or the navigated folder stack?

---

## Gap Resolution: GAP-TRANS-001

**Gap:** Transition triggers incomplete - How does user enter EditRule from Browse? Press `e` on what? Pattern? Selected folder? File?

**Confidence:** HIGH

### Proposed Solution

Define explicit trigger contexts for the `e` key based on current state and selection.

#### Trigger Context Table

| Current State | Selection Context | `e` Key Behavior | Pre-filled Values |
|---------------|-------------------|------------------|-------------------|
| Browse (root) | Any folder selected | DISABLED (no pattern, no matches) | N/A |
| Browse (in folder) | Any item selected | DISABLED (no pattern, no matches) | N/A |
| Filtering | Folder selected (matches > 0) | Enter EditRule | `glob = current_prefix + "/" + pattern` |
| Filtering | File selected (in flat results) | Enter EditRule | `glob = current_prefix + "/" + pattern` |
| Filtering | No selection (matches > 0) | Enter EditRule | `glob = current_prefix + "/" + pattern` |
| Filtering | Pattern has 0 matches | DISABLED (nothing to extract) | N/A |

#### Key Insight: `e` Requires a Pattern

The `e` key creates a rule FROM the current glob pattern. Therefore:
- In Browse state (no pattern): `e` is disabled or shows hint "Press / to filter first"
- In Filtering state (has pattern): `e` creates rule from that pattern

This aligns with the design philosophy: **you explore first, then convert your exploration into a rule**.

#### Visual Feedback

```
Browse state (no pattern):
  ─────────────────────────────────────────────────────────────────────────────────
  [hjkl] Navigate  [l/Enter] Drill  [/] Filter  [g/Esc] Exit

Filtering state (has pattern, matches > 0):
  ─────────────────────────────────────────────────────────────────────────────────
  [hjkl] Navigate  [l/Enter] Drill  [e] Create rule  [Esc] Clear  [g] Exit

Filtering state (has pattern, 0 matches):
  ─────────────────────────────────────────────────────────────────────────────────
  [hjkl] Navigate  [l/Enter] Drill  [Esc] Clear pattern  [g] Exit
  (no [e] shown - nothing to extract)
```

#### Alternative: `e` on Selected File for Template Matching

Per Phase 18g (Template Matching), when a user wants to create a rule from a SINGLE file (not a pattern), they need a different flow:

| Scenario | Entry | Result |
|----------|-------|--------|
| Pattern with matches | `e` from Filtering | EditRule with pattern as glob |
| Single file selected | `Enter` on file in flat results → `e` | Template suggestions dialog |

This means:
1. Navigate to file in flat results
2. `Enter` to select/preview file
3. `e` to "extract from this file" → shows template matches
4. Select template → EditRule with template-suggested fields

#### State Diagram Annotation

```
                    FILTERING state
                    ┌─────────────────────────────────────┐
                    │  Pattern: **/*.csv  [847 matches]   │
                    │                                      │
                    │  📁 data         ▓▓▓▓  423 >        │  ← folder selected
                    │  📁 archive      ▓▓    212 >        │
                    │                                      │
                    │  [e] = Create rule from "**/*.csv"   │
                    └─────────────────────────────────────┘
                                  │
                                  │ e
                                  ▼
                    ┌─────────────────────────────────────┐
                    │           EDIT_RULE state            │
                    │                                      │
                    │  Glob: **/*.csv                [847] │  ← pre-filled
                    │  Fields: (inferring...)              │
                    │  Tag: ___________                    │
                    └─────────────────────────────────────┘
```

### Examples

**Example 1: Valid `e` trigger**
```
1. User in Browse at /data
2. Presses "/" → Filtering state
3. Types "*.csv" → 234 matches shown
4. Presses "e"
5. → EditRule opens with glob = "data/*.csv"
```

**Example 2: Invalid `e` trigger (no pattern)**
```
1. User in Browse at /data (no pattern)
2. Presses "e"
3. → Nothing happens OR status shows: "Press / to filter first, then e to create rule"
```

**Example 3: Invalid `e` trigger (0 matches)**
```
1. User in Filtering with pattern "*.xyz"
2. 0 matches shown
3. Presses "e"
4. → Nothing happens (no files to extract from)
```

**Example 4: Template matching from single file**
```
1. User in Filtering, pattern "mission_*/**/*.csv", 12 matches
2. Flat results show individual files
3. User presses "j" to select "/data/mission_042/2024-01-15/telemetry.csv"
4. Presses "Enter" → file selected/previewed
5. Presses "e" → Template Suggestions dialog opens
6. Selects "#1 Defense Mission (82%)"
7. → EditRule opens with template-suggested fields
```

### Trade-offs

**Pros:**
- Clear, unambiguous: `e` only works when there's something to extract
- Follows "explore first, then formalize" philosophy
- Status bar hints guide users naturally

**Cons:**
- Users might expect `e` to work everywhere (discoverability)
- Two paths to EditRule (pattern-based vs template-based) adds complexity

### New Gaps Introduced

- **GAP-TMPL-001 (MEDIUM)**: Need to clarify the exact UX for template matching flow. Does `e` on a single file show templates inline, or is there a separate "template mode"?

---

## Summary

| Gap | Resolution | Confidence | New Gaps |
|-----|------------|------------|----------|
| GAP-STATE-001 | Unified state diagram with Navigation Layer and Rule Editing Layer | HIGH | GAP-CTX-001 (LOW) |
| GAP-TRANS-001 | `e` triggers only from Filtering state with matches > 0; disabled otherwise with status hint | HIGH | GAP-TMPL-001 (MEDIUM) |

### Recommended Next Steps

1. **Reviewer** should validate the unified state diagram against implementation reality
2. **Round 2** should address HIGH-priority gaps: GAP-FIELD-001, GAP-TEST-001, GAP-DATA-001, GAP-NAV-001
3. New gaps GAP-CTX-001 and GAP-TMPL-001 can be deferred to Round 3 (lower priority)

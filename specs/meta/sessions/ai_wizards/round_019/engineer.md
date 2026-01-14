# Engineer Resolution: GAP-FLOW-002

## Semantic Path Wizard Invocation Specification

**Gap:** The Semantic Path Wizard (Section 3.4) is specified technically but lacks clear UI invocation patterns:
1. How users invoke it from the TUI (entry points)
2. What context is required vs optional
3. How it integrates with Discover mode state machine
4. How it differs from Pathfinder in workflow
5. Pre-detection of semantic primitives

**Confidence:** HIGH

**References:**
- `specs/ai_wizards.md` Section 3.4 (Semantic Path Wizard overview)
- `specs/discover.md` (TUI layout and state machine)
- `specs/semantic_path_mapping.md` (Semantic primitives vocabulary)
- Round 4-17 previous discoveries

---

## 1. Semantic Path Wizard Entry Points

The Semantic Path Wizard has **three primary entry points** in the Discover mode TUI:

### 1.1 From Sources Panel (Primary)

**Key:** `S` (Shift+S) when source row is focused

**Activation Condition:**
- Source must have files (cannot be empty)
- Source must have 3+ files (minimum for meaningful analysis)

**Flow:**
```
Sources Panel (focus on source row)
    ↓ [Press S]
    ↓
Semantic Path Wizard Launches
    ↓ (auto-analyzes all files in source)
    ↓
Dialog shows results
```

**Pre-conditions Met:**
- Source ID (required)
- File list (already scanned)
- No user selection needed (uses all files in source)

**User Experience:**
```
┌─ SOURCES ─────────────────────────────────┐
│ [>] /mnt/mission_data (47 files)          │← Focused here
│     ├─ 0 failed                           │
│     └─ 0 untagged                         │
│                                           │
│ Press [S] for Semantic Path Wizard        │
└───────────────────────────────────────────┘
```

### 1.2 From Wizard Menu (Flexible)

**Key:** `W` (open Wizard menu) then `s` (Semantic Path option)

**When to use:**
- When source selection context is unclear
- When user wants to provide custom hints
- When analyzing a subset of files

**Flow:**
```
Any Discover Mode state
    ↓ [Press W]
    ↓
Wizard Menu appears:
  [p] Pathfinder (Extractor)
  [g] Parser Lab (Generator)
  [l] Labeling (Semantic Tag)
  [s] Semantic Path (Structure)  ← Select this
    ↓
Source Selection Dialog
    ↓ (if not focused on a source)
    ↓
File Sampling / Hint Input
    ↓
Semantic Path Wizard Launches
```

**Menu Behavior:**
- If already focused on a source: skip source selection, proceed directly to analysis
- If not focused on a source: show source picker first
- Maximum 5 random representative files sampled (cost-efficient)

### 1.3 From Files Panel (Context-Aware)

**Key:** `S` (Shift+S) when a file is selected

**Activation Condition:**
- 2+ files selected OR file in a clustered group
- Only works if files are from same source

**Flow:**
```
Files Panel (2+ files selected)
    ↓ [Press S]
    ↓
Detect source from selection
    ↓
Launch Semantic Path Wizard
    ↓ (uses selected files only)
```

**Behavior:**
- If all selected files from different sources: show error "All selected files must be from same source"
- If only 1 file: show warning "Need 3+ files for reliable analysis. Add more?" with options:
  - `[y]` Include entire source (switch to Source panel flow)
  - `[c]` Continue with 1 file (uses AI proposals)
  - `[Esc]` Cancel

---

## 2. Context Requirements

### 2.1 Required Context

**Source ID** (REQUIRED)
- Must identify which source files come from
- Determines file list to analyze
- Used in generated rule

**File List** (REQUIRED)
- Minimum: 1 file (with AI uncertainty tolerance)
- Recommended: 3-10 files (algorithmic detection confidence)
- Maximum: 500 files (limit for cost, use clustering)

If fewer than 3 files:
```
┌─ SEMANTIC PATH WIZARD ──────────────────────────┐
│                                                  │
│  ⚠ Only 1 file available for analysis           │
│                                                  │
│  Detection confidence will be lower. Options:   │
│                                                  │
│  [a] Analyze with 1 file (AI-assisted)          │
│  [s] Switch to Sources panel (include all)      │
│  [Esc] Cancel                                    │
└──────────────────────────────────────────────────┘
```

### 2.2 Optional Context

**User Hints** (OPTIONAL)
- Free-text hint about folder meaning
- Example: "first folder is the mission identifier"
- Helps disambiguate between multiple valid patterns

**Custom File Sample** (OPTIONAL)
- User can override automatic file selection
- Useful if some files are outliers
- UI: "Select files to analyze" modal

### 2.3 Default Behavior When Context Missing

| Missing | Default | Behavior |
|---------|---------|----------|
| Source | Show source picker | Single-source or multi-source selection |
| Files | Auto-sample 5 files | Random stratified sample (balanced depths) |
| Hints | None | Pure algorithmic + AI disambiguation |

---

## 3. Pre-Detection of Semantic Primitives

Before showing the full wizard dialog, **run a fast pre-detection pass** to determine if semantic recognition is even possible:

### 3.1 Pre-Detection Algorithm

```
Input: Sample file paths from source
Output: Recognized primitives OR "not semantic structure" signal

Steps:
1. Normalize paths (strip root, focus on relative parts)
2. Check for known primitive patterns (Section 3.5.2 of ai_wizards.md):
   - dated_hierarchy (ISO, US, custom formats)
   - entity_folder (prefix patterns, ID ranges)
   - document_type (filename extensions + folder keywords)
   - version_hierarchy (v1, v2, release- patterns)
   - geographic_hierarchy (region, country, city folders)

3. Score confidence for each detected primitive
4. Compose expression from detected sequence
5. If confidence < 40%, flag as "ambiguous" (AI will help)
```

### 3.2 Pre-Detection Result States

| State | Confidence | UI Behavior |
|-------|-----------|------------|
| **Clear Detection** | ≥ 80% | Show results immediately, high confidence |
| **Ambiguous** | 40-80% | Show results with "AI analyzing..." progress |
| **Not Semantic** | < 40% | Offer Pathfinder instead |

### 3.3 "Not Semantic" Fallback

If pre-detection score < 40%, offer alternative:

```
┌─ SEMANTIC PATH WIZARD ──────────────────────────┐
│                                                  │
│  Unable to detect semantic structure            │
│                                                  │
│  This folder structure doesn't match known      │
│  semantic patterns (entity_folder, dated,       │
│  version, etc.)                                 │
│                                                  │
│  Available options:                             │
│                                                  │
│  [p] Try Pathfinder (custom patterns)           │
│  [m] Show me what it detected anyway            │
│  [Esc] Cancel                                    │
└──────────────────────────────────────────────────┘
```

If user presses `[m]`: Show partial/ambiguous results with lower confidence scores and offer AI disambiguation option.

---

## 4. Differentiation from Pathfinder

### 4.1 When to Use Each Wizard

| Aspect | Semantic Path Wizard | Pathfinder Wizard |
|--------|---------------------|-------------------|
| **Best for** | Standard folder patterns | Custom/unusual patterns |
| **Input** | File paths only | Paths + optional hints |
| **Output** | YAML rule only | YAML (primary) or Python (fallback) |
| **Abstraction** | High-level (semantic primitives) | Low-level (regex, string ops) |
| **Reusability** | Across sources with same structure | Single source specific |
| **Confidence** | High when semantic detected | Varies by pattern complexity |
| **Speed** | Fast (algorithmic pre-detection) | Slower (requires LLM) |

### 4.2 Auto-Selection Heuristic

When user presses `W` (Wizard menu), show which wizard is recommended:

```
Wizard Menu:

[p] Pathfinder (Custom patterns)
[g] Parser Lab (Generate parser)
[l] Labeling (Tag files)
[s] ← Semantic Path (Standard structure) RECOMMENDED ✓

  (Folder structure detected: entity_folder > dated_hierarchy)
```

**Recommendation Logic:**
```
if pre_detection_score >= 60:
    recommend Semantic
elif has_custom_patterns or user_hints_suggest_complex_logic:
    recommend Pathfinder
else:
    no recommendation (equal preference)
```

### 4.3 Switching Between Wizards

**Mid-Wizard Switch:**

If user is in Semantic Path Wizard and wants to switch:

```
[?] Having trouble? Try:
    [p] Pathfinder (for custom logic)
    [Esc] Cancel
```

Result: Cancel current wizard, launch Pathfinder with same sample paths and user hints pre-filled.

---

## 5. Invocation State Machine

### 5.1 Complete Flow

```
┌────────────────────────────────────────────────────────────────┐
│                    DISCOVER MODE (Active)                      │
└────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │ User Input: S or W│
                    │ (Source/Files)    │
                    └─────────┬─────────┘
                              │
                    ┌─────────▼──────────────────────┐
                    │ Wizard Menu (if W pressed)     │
                    │ Select: [s] Semantic Path      │
                    └─────────┬──────────────────────┘
                              │
                    ┌─────────▼──────────────────────┐
                    │ Source Selection (if needed)   │
                    │ Show source picker             │
                    └─────────┬──────────────────────┘
                              │
                    ┌─────────▼──────────────────────┐
                    │ SAMPLING STATE                 │
                    │ Showing "Sampling files..."    │
                    │ (max 5 files analyzed)         │
                    └─────────┬──────────────────────┘
                              │
                    ┌─────────▼──────────────────────┐
                    │ PRE-DETECTION                  │
                    │ Analyzing for semantic pattern │
                    │ (fast algorithmic check)       │
                    └─────────┬──────────────────────┘
                              │
                    ┌─────────┴──────────────┐
                    │                        │
         ┌──────────▼──────────┐  ┌─────────▼──────────────┐
         │ SEMANTIC DETECTED   │  │ NOT SEMANTIC           │
         │ (confidence ≥ 40%)  │  │ (confidence < 40%)     │
         └──────────┬──────────┘  └─────────┬──────────────┘
                    │                       │
      ┌─────────────┴─┐           ┌────────▼────────────┐
      │               │           │ Show "Not Semantic" │
      │ GENERATING    │           │ fallback dialog     │
      │ (if ambiguous)│           │ [p] Pathfinder      │
      │ Calling LLM   │           │ [m] Show anyway     │
      │               │           │ [Esc] Cancel        │
      └─────────────┬─┘           └────────┬────────────┘
                    │                      │ [p]
         ┌──────────▼──────────┐           │
         │ RESULT               │ ◄────────┘
         │ Show semantic expr   │
         │ + extraction rule    │
         │ + confidence score   │
         │ + similar sources    │
         └──────────┬──────────┘
                    │
         ┌──────────▼──────────────────────┐
         │ User Actions:                    │
         │ [Enter] Approve                  │
         │ [e] Edit rule                    │
         │ [a] Alternatives                 │
         │ [h] Hint (re-analyze with hint)  │
         │ [p] Switch to Pathfinder         │
         │ [Esc] Cancel                     │
         └──────────┬───────────────────────┘
                    │
         ┌──────────▼──────────┐
         │ APPROVED/COMMITTED  │
         │ Rule moved to       │
         │ Layer 1             │
         └─────────────────────┘
```

### 5.2 State Definitions

| State | Entry From | Exit To | Triggers |
|-------|-----------|---------|----------|
| **SAMPLING** | Wizard menu / context | PRE-DETECTION | Sample files completed |
| **PRE-DETECTION** | SAMPLING | SEMANTIC DETECTED / NOT SEMANTIC | Algorithmic check complete |
| **GENERATING** | PRE-DETECTION (if ambiguous) | RESULT / ERROR | LLM returns result |
| **RESULT** | GENERATING / PRE-DETECTION | APPROVED / CANCELED | User reviews |
| **APPROVED** | RESULT (Enter key) | Discover mode | Rule committed |
| **CANCELED** | RESULT (Esc) | Discover mode | User cancels |

---

## 6. Entry Point Keybinding Summary

### 6.1 Keybindings

| Context | Key | Action | Notes |
|---------|-----|--------|-------|
| Sources panel (focused on row) | `S` | Launch Semantic Wizard | Primary entry point |
| Files panel (2+ selected) | `S` | Launch Semantic Wizard | From file selection |
| Anywhere in Discover | `W` then `s` | Show Wizard menu then select Semantic | Alternative entry |
| Semantic Wizard result | `[p]` | Switch to Pathfinder | Mid-wizard switch |
| Semantic Wizard result | `[h]` | Edit hint and re-analyze | Keep wizard open |

### 6.2 Keybinding Conflicts & Resolution

**Potential Conflict:** `S` key for both Semantic Wizard and "Source Manager"

**Resolution:** Context-aware:
- If **focused on source row** → Semantic Path Wizard (primary action)
- If **focused elsewhere** → Show "Hold Shift for Source Manager" tooltip

Implementation:
```rust
// In handle_key_event for Discover mode
if key == Key::Char('S') {
    match current_focus {
        Focus::SourceRow => launch_semantic_wizard(source_id),
        Focus::FilesPanel => launch_semantic_wizard_from_files(file_ids),
        _ => show_source_manager(),  // Secondary binding
    }
}
```

---

## 7. Complete UX Flow Example

### 7.1 Scenario: User Discovers Mission Data

**Setup:**
- User has scanned `/mnt/mission_data` source
- Source shows 47 files with diverse structure
- No tags applied yet

**Sequence:**

```
1. User navigates to Sources panel
   Sees: /mnt/mission_data (47 files, 0 tagged)

2. User presses [S]
   → Wizard samples 5 representative files
   → Pre-detection analyzes structure:
      - segment(-3): "mission_042", "mission_043" → entity_folder(mission)
      - segment(-2): "2024-01-15", "2024-01-16" → dated_hierarchy(iso)
      - files: "*.csv" → files

3. Dialog appears with results:
   ┌─ SEMANTIC PATH WIZARD ─────────────────────────────────────┐
   │  Source: /mnt/mission_data (47 files)                      │
   │  ┌─ Detected Structure ───────────────────────────────────┐ │
   │  │  Semantic: entity_folder(mission) >                    │ │
   │  │            dated_hierarchy(iso) > files                │ │
   │  │  Confidence: ████████████████░░ 94%                    │ │
   │  └────────────────────────────────────────────────────────┘ │
   │                                                              │
   │  ┌─ Generated Rule ───────────────────────────────────────┐ │
   │  │  glob: "**/mission_*/????-??-??/*.csv"                 │ │
   │  │  extract:                                              │ │
   │  │    mission_id: from segment(-3), pattern "mission_(.*)"│ │
   │  │    date: from segment(-2), type date_iso               │ │
   │  │  tag: mission_data                                     │ │
   │  └────────────────────────────────────────────────────────┘ │
   │                                                              │
   │  ┌─ Preview ──────────────────────────────────────────────┐ │
   │  │  ✓ mission_042/2024-01-15/telemetry.csv                │ │
   │  │    → {mission_id: 042, date: 2024-01-15}               │ │
   │  │  ✓ mission_043/2024-01-16/readings.csv                 │ │
   │  │    → {mission_id: 043, date: 2024-01-16}               │ │
   │  └────────────────────────────────────────────────────────┘ │
   │                                                              │
   │  [Enter] Create Rule   [e] Edit   [h] Hint   [p] Pathfinder │
   │  [a] Alternatives      [Esc] Cancel                         │
   └──────────────────────────────────────────────────────────────┘

4. User reviews and presses [Enter]
   → Rule committed to Layer 1
   → 47 files tagged with "mission_data"
   → Return to Discover mode

5. Sources panel now shows:
   /mnt/mission_data (47 files, 47 tagged) ✓
```

---

## 8. Integration with Discover Mode State Machine

### 8.1 State Hierarchy

```
DISCOVER_MODE
├── NORMAL
│   ├── Sources panel focused
│   │   ├── [S] → Semantic Wizard
│   │   └── [M] → Source Manager
│   └── Files panel focused
│       ├── [S] (2+ selected) → Semantic Wizard
│       └── [w] (single file) → Pathfinder Wizard
│
└── WIZARD_ACTIVE
    └── SEMANTIC_PATH_WIZARD
        ├── SAMPLING
        ├── PRE-DETECTION
        ├── GENERATING (if ambiguous)
        ├── RESULT
        └── [APPROVED|CANCELED] → DISCOVER_MODE
```

### 8.2 Focus Management

**Before wizard launches:**
- Save current focus (which panel was active)

**After wizard completes:**
- Restore focus to same panel (if source tagged)
- OR navigate to Files panel (if new rule created)

**Example:**
```rust
// Before launching Semantic Wizard
saved_focus = current_focus;

// After user approves rule
if rule_matched_files > 0 {
    navigate_to(FilesPanel);
    filter_by_tag(rule_tag);
} else {
    navigate_to(saved_focus);
}
```

---

## 9. Error Handling & Edge Cases

### 9.1 Edge Case: Empty Source

**Entry:** User presses `[S]` on empty source

**Behavior:**
```
Error: Source has no files
   Message: "Source /mnt/empty has 0 files. Scan it first?"
   Options:
   [s] Scan now
   [Esc] Cancel
```

### 9.2 Edge Case: All Files Have Same Path

**Entry:** User in source with 5 identical file paths (shouldn't happen, but...)

**Behavior:**
```
Warning: All files are identical
   Message: "All sampled files have the same path structure."
   Options:
   [c] Continue anyway (low confidence)
   [Esc] Cancel
```

### 9.3 Edge Case: Cross-Source Selection

**Entry:** User selects 2+ files from different sources, presses `[S]`

**Behavior:**
```
Error: Mixed sources
   Message: "Selected files are from multiple sources:
              /mnt/mission_data (2 files)
              /archive/old_missions (3 files)

            Analyze separately?"
   Options:
   [m] Mixed (create separate rules)
   [s] Select one source only
   [Esc] Cancel
```

If user selects `[m]`: Create 2 rules, one per source.

---

## 10. Implementation Phases

### Phase 1: Entry Point Keybindings (1 day)
- [ ] Implement `S` keybinding in Sources panel
- [ ] Implement `S` keybinding in Files panel
- [ ] Wire to Semantic Wizard state machine
- [ ] Add context-aware behavior

### Phase 2: Wizard Menu Integration (0.5 day)
- [ ] Add `[s]` option to Wizard menu
- [ ] Implement source picker if needed
- [ ] Auto-sample files

### Phase 3: Pre-Detection Algorithm (1 day)
- [ ] Implement semantic primitive detection
- [ ] Implement confidence scoring
- [ ] Test with sample paths

### Phase 4: UX Dialogs (1 day)
- [ ] Build "Not Semantic" fallback dialog
- [ ] Build source selection dialog
- [ ] Build hint input dialog
- [ ] Build file selection dialog (optional)

### Phase 5: State Machine (1 day)
- [ ] Implement SAMPLING state
- [ ] Implement PRE-DETECTION state
- [ ] Implement state transitions
- [ ] Add error handling for edge cases

### Phase 6: Integration Testing (1 day)
- [ ] E2E test for Sources panel entry
- [ ] E2E test for Files panel entry
- [ ] E2E test for Wizard menu entry
- [ ] E2E test for edge cases

---

## 11. Spec Updates Required

Update `specs/ai_wizards.md` Section 5.6 Keybindings:

**Add:**
```markdown
### 5.6.1 Semantic Path Wizard Invocation

| Entry Point | Key | Condition | Context |
|-------------|-----|-----------|---------|
| Sources panel | `S` | Source focused, 3+ files | Analyzes all files in source |
| Files panel | `S` | 2+ files selected, same source | Analyzes selected files only |
| Wizard menu | `W` then `s` | Any state | Manual selection of source |

**Minimum File Requirement:**
- 1 file: Allowed with AI assistance (lower confidence)
- 3+ files: Recommended for algorithmic detection
- 500+ files: Capped at 5 random samples for cost efficiency

**Pre-Detection Behavior:**
- Confidence ≥ 80%: Show results immediately
- 40-80%: Show results with AI disambiguator
- < 40%: Offer Pathfinder alternative

**Differentiation from Pathfinder:**
- Use Semantic Path for standard folder patterns (dated hierarchies, entity folders)
- Use Pathfinder for custom extraction logic or unusual patterns
- Semantic Path Wizard recommended when confidence ≥ 60%
```

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Primary entry: `S` from Sources | Not menu | Most common use case (analyze entire source) |
| Alternative entry: Files panel `S` | Flexible | Users often explore files first |
| Minimum files: 1 (with AI) | Not hard requirement | Single file + LLM can still generate useful results |
| Pre-detection before LLM | Yes | Fast algorithmic check reduces LLM calls |
| Fallback to Pathfinder if <40% | Yes | Better UX than forcing ambiguous semantic result |
| Max sample: 5 files | Not variable | Cost control + sufficient for detection |
| Keybinding conflict resolution: context-aware | Not separate key | Reduces keybinding surface area |

---

## 13. New Gaps Introduced

None. This resolution is self-contained and defines complete invocation flow.

---

## 14. References

- `specs/ai_wizards.md` Section 3.4 (Semantic Path Wizard overview)
- `specs/ai_wizards.md` Section 5.5 (Semantic Path Wizard dialog)
- `specs/discover.md` (Discover mode state machine)
- `specs/semantic_path_mapping.md` (Semantic primitives vocabulary)
- Round 4 (GAP-STATE-004): Semantic Path Wizard state machine
- Round 15 (GAP-INT-001): Path Intelligence Engine TUI integration

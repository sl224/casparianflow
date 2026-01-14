# Engineer Round 004: GAP-STATE-004

## Gap Resolution: GAP-STATE-004

**Gap:** The Semantic Path Wizard TUI dialog (Section 5.5) shows UI mockups with keybindings [Enter, a, e, h, Esc] but has no state machine.

**Confidence:** HIGH

---

### Proposed Solution

The Semantic Path Wizard is a modal dialog that recognizes semantic folder structure from file paths and generates extraction rules. Unlike Pathfinder (which generates YAML or Python), Semantic Path Wizard always generates YAML extraction rules using semantic primitives. The state machine must handle:

1. Initial analysis/recognition phase
2. Results display with detected semantic structure
3. "Show Alternatives" feature for viewing other interpretations
4. "Similar Sources" display for cross-source pattern matching
5. User refinement via hints or manual editing
6. Regeneration cycles
7. Approval or cancellation

**Key Differences from Pathfinder:**

| Aspect | Pathfinder | Semantic Path Wizard |
|--------|-----------|---------------------|
| Output | YAML rule or Python | Always YAML rule |
| Abstraction | Low-level (regex, segments) | High-level (semantic primitives) |
| Recognition | AI analyzes path directly | Algorithmic + AI disambiguation |
| Alternatives | Not applicable | Multiple interpretations possible |
| Similar Sources | Not applicable | Shows other sources with same pattern |
| Vocabulary | Ad-hoc patterns | Named primitives (entity_folder, dated_hierarchy, etc.) |

---

#### State Diagram

```
                                +-----------------------------------------------------------------+
                                |                    SEMANTIC PATH WIZARD                           |
                                +-----------------------------------------------------------------+

                                                          |
                                                          | User invokes wizard
                                                          | (S on source, W->s menu)
                                                          v
                                                 +-------------------+
                                                 |    RECOGNIZING    |
                                                 |  (entry state)    |
                                                 +---------+---------+
                                                           |
                               +---------------------------+--------------------------+
                               |                           |                          |
                               v                           v                          v
                      +-----------------+        +-----------------+       +-------------------+
                      | RECOGNITION_ERR |        | RESULT_HIGH_    |       | RESULT_LOW_       |
                      |                 |        | CONFIDENCE      |       | CONFIDENCE        |
                      +--------+--------+        | (>= 80%)        |       | (< 80%)           |
                               |                 +--------+--------+       +--------+----------+
                               |                          |                          |
                               |                          +-----------+--------------+
                               |                                      |
                               |                          +-----------+-----------+
                               |                          |                       |
                               |                          v                       v
                               |                 +-------------------+   +-------------------+
                               |                 |    RESULT_SHOWN   |   | ALTERNATIVES_VIEW |
                               |                 |  (abstract parent)|   | (list of interps) |
                               |                 +---------+---------+   +---------+---------+
                               |                           |                       |
          +--------------------+          +----------------+---------------+       |
          |                    |          |        |       |        |      |       |
          v                    |          v        v       v        v      v       v
  +---------------+           |   +----------+ +------+ +--------+ +----------+ +--------+
  |    CLOSED     |<----------+---| APPROVED | | HINT | | EDITING| |REGENERATE| |CANCELED|
  |  (terminal)   |           |   |(terminal)| |INPUT | | (ext.) | |   ING    | |(term.) |
  +---------------+           |   +----------+ +--+---+ +----+---+ +----+-----+ +--------+
                              |                   |          |          |
                              |                   |          |          |
                              |                   +----------+----------+
                              |                              |
                              |                              | (back to RECOGNIZING)
                              |                              |
                              +------------------------------+
```

**Simplified Linear View:**

```
+---------------+      +------------------------+      +---------------+      +---------------+
|  RECOGNIZING  |----->| RESULT_HIGH_CONFIDENCE |      |  HINT_INPUT   |----->| REGENERATING  |
|               |      |          or            |<---->|               |      |               |
+---------------+      | RESULT_LOW_CONFIDENCE  |      +---------------+      +-------+-------+
       |               +-----------+------------+                                     |
       |                           |                                                  |
       v                           |         +----------------------------------------+
+---------------+                  |         |
|RECOGNITION_ERR|                  |         v
+---------------+                  |   +-------------+
       |               +-----------+-->|ALTERNATIVES |
       |               |   (a key)     |    VIEW     |
       v               |               +------+------+
+---------------+      |                      |
|    CLOSED     |      |                      | (Enter selects)
+---------------+      |                      |
                       v                      v
                +---------------+      +---------------+
                |   APPROVED    |      | RESULT_SHOWN  |
                |      or       |      | (with new     |
                |   CANCELED    |      |  selection)   |
                +---------------+      +---------------+
```

---

#### State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| **RECOGNIZING** | Wizard invoked with source ID or paths | Recognition completes or fails | Spinner shown. Analyzes folder structure using semantic vocabulary. Detects primitives (entity_folder, dated_hierarchy, etc.). User cannot interact except Esc to cancel. |
| **RESULT_HIGH_CONFIDENCE** | Recognition succeeded with confidence >= 80% | User action (Enter/a/e/h/Esc) | Shows semantic expression, confidence bar, path breakdown, generated rule, 5-file preview, similar sources. User can approve directly. |
| **RESULT_LOW_CONFIDENCE** | Recognition succeeded with confidence < 80% | User action (a/e/h/Esc) | Same display as HIGH but with amber confidence indicator. "Show Alternatives" (a) is highlighted as suggested action. |
| **RECOGNITION_ERROR** | Algorithm failed to recognize path structure | User action (r/Esc) | Shows error message (no matching primitives, ambiguous structure). User can retry (r) or cancel (Esc). |
| **ALTERNATIVES_VIEW** | User pressed 'a' from result state | User action (Enter/j/k/Esc) | Shows list of alternative interpretations with confidence scores. User can navigate (j/k) and select (Enter). |
| **HINT_INPUT** | User pressed 'h' from result state | Enter submits, Esc cancels | Text input field active. User types hint (e.g., "The first folder is the mission identifier"). Examples shown. |
| **EDITING** | User pressed 'e' from result state | User saves and closes $EDITOR | Draft YAML rule opened in $EDITOR. TUI shows waiting message. Validation runs on return. |
| **REGENERATING** | User submitted hint, saved edit, or selected alternative | Regeneration completes, fails, or user cancels | Spinner shown. Re-analyzes with new context. User can press Esc to cancel. Returns to result state or error. |
| **APPROVED** | User pressed Enter from RESULT_HIGH_CONFIDENCE | Immediate | Rule committed to Layer 1 (extraction_rules/). Tag applied to matching files. Dialog closes. |
| **CANCELED** | User pressed Esc from any non-input state | Immediate | Draft discarded. Dialog closes. |
| **CLOSED** | Recognition error with Esc | Immediate | Dialog closes. No draft created. |

---

#### Transitions

| From | To | Trigger | Guard |
|------|----|---------| ------|
| (external) | RECOGNIZING | User invokes wizard | Source ID or sample paths provided |
| RECOGNIZING | RESULT_HIGH_CONFIDENCE | Recognition completes | confidence >= 0.80 |
| RECOGNIZING | RESULT_LOW_CONFIDENCE | Recognition completes | confidence < 0.80 |
| RECOGNIZING | RECOGNITION_ERROR | Recognition fails | No primitives matched or critical ambiguity |
| RECOGNIZING | CANCELED | Esc | - |
| RESULT_HIGH_CONFIDENCE | APPROVED | Enter | Tag name valid (non-empty) |
| RESULT_HIGH_CONFIDENCE | ALTERNATIVES_VIEW | a | alternatives.len() > 1 |
| RESULT_HIGH_CONFIDENCE | HINT_INPUT | h | - |
| RESULT_HIGH_CONFIDENCE | EDITING | e | $EDITOR available |
| RESULT_HIGH_CONFIDENCE | CANCELED | Esc | - |
| RESULT_LOW_CONFIDENCE | ALTERNATIVES_VIEW | a | alternatives.len() > 1 |
| RESULT_LOW_CONFIDENCE | HINT_INPUT | h | - |
| RESULT_LOW_CONFIDENCE | EDITING | e | $EDITOR available |
| RESULT_LOW_CONFIDENCE | CANCELED | Esc | - |
| RESULT_LOW_CONFIDENCE | APPROVED | Enter | Tag name valid (user accepts low confidence) |
| RECOGNITION_ERROR | REGENERATING | r | retry_count < 3 |
| RECOGNITION_ERROR | CLOSED | Esc | - |
| RECOGNITION_ERROR | CLOSED | r | retry_count >= 3 |
| ALTERNATIVES_VIEW | REGENERATING | Enter | Alternative selected |
| ALTERNATIVES_VIEW | (previous result state) | Esc | - |
| ALTERNATIVES_VIEW | (stay) | j/k | Navigate list |
| HINT_INPUT | REGENERATING | Enter | Hint text non-empty |
| HINT_INPUT | (previous result state) | Esc | - |
| EDITING | REGENERATING | Editor closes | File modified |
| EDITING | (previous result state) | Editor closes | File unmodified |
| REGENERATING | RESULT_HIGH_CONFIDENCE | Recognition completes | confidence >= 0.80 |
| REGENERATING | RESULT_LOW_CONFIDENCE | Recognition completes | confidence < 0.80 |
| REGENERATING | RECOGNITION_ERROR | Recognition fails | - |
| REGENERATING | CANCELED | Esc | - |
| APPROVED | (dialog closes) | - | Commit to Layer 1, apply tag |
| CANCELED | (dialog closes) | - | Discard draft |
| CLOSED | (dialog closes) | - | No draft to discard |

---

#### Keybinding Summary by State

| Key | RECOGNIZING | RESULT_HIGH | RESULT_LOW | RECOGNITION_ERR | ALTERNATIVES | HINT_INPUT | EDITING | REGENERATING |
|-----|-------------|-------------|------------|-----------------|--------------|------------|---------|--------------|
| Enter | - | Approve | Approve* | - | Select alt | Submit hint | - | - |
| Esc | Cancel | Cancel | Cancel | Close | Back | Back | - | Cancel |
| a | - | Show alts | Show alts | - | - | - | - | - |
| h | - | Open hint | Open hint | - | - | - | - | - |
| e | - | Open editor | Open editor | - | - | - | - | - |
| r | - | - | - | Retry | - | - | - | - |
| j/Down | - | - | - | - | Next alt | - | - | - |
| k/Up | - | - | - | - | Prev alt | - | - | - |
| (typing) | - | Edit tag | Edit tag | - | - | Input text | - | - |

*Enter on RESULT_LOW shows confirmation: "Approve with low confidence (72%)? [y/N]"

---

#### Data Model (Rust structs)

```rust
/// Semantic Path Wizard state
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPathState {
    Recognizing,
    ResultHighConfidence(SemanticResultData),
    ResultLowConfidence(SemanticResultData),
    RecognitionError(RecognitionErrorData),
    AlternativesView(AlternativesViewData),
    HintInput(HintInputData),
    Editing(EditingData),
    Regenerating(RegeneratingData),
    Approved,
    Canceled,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticResultData {
    /// Source being analyzed
    pub source_id: Option<String>,
    /// Sample paths analyzed
    pub sample_paths: Vec<PathBuf>,
    /// Number of files in source
    pub file_count: usize,
    /// Detected semantic expression
    pub semantic_expression: String,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// Segment-by-segment breakdown
    pub segment_analysis: Vec<SegmentAnalysis>,
    /// Generated YAML extraction rule
    pub generated_rule: GeneratedRule,
    /// Preview results (5 files)
    pub preview: Vec<SemanticPreviewResult>,
    /// Similar sources with same pattern
    pub similar_sources: Vec<SimilarSource>,
    /// Alternative interpretations
    pub alternatives: Vec<AlternativeInterpretation>,
    /// User-editable tag name
    pub tag_name: String,
    /// Number of regeneration attempts
    pub regeneration_count: u32,
    /// User hints accumulated
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentAnalysis {
    /// Segment position (negative from end, e.g., -3)
    pub position: i32,
    /// Sample value from path
    pub sample_value: String,
    /// Matched semantic primitive
    pub primitive: SemanticPrimitive,
    /// Extracted field name
    pub field_name: String,
    /// Extracted value
    pub extracted_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPrimitive {
    EntityFolder { entity_type: String },
    DatedHierarchy { variant: DateVariant },
    VersionedFolder,
    CategoryFolder,
    NumericSequence,
    Files,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateVariant {
    Iso,        // 2024-01-15
    YearMonth,  // 2024/01 or 2024-01
    YearOnly,   // 2024
    Compact,    // 20240115
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedRule {
    /// Rule name (auto-generated or user-provided)
    pub name: String,
    /// Glob pattern
    pub glob: String,
    /// Extraction fields
    pub extract: HashMap<String, ExtractionField>,
    /// Tag to apply
    pub tag: String,
    /// Semantic source (the expression)
    pub semantic_source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionField {
    pub from: String,        // e.g., "segment(-3)"
    pub pattern: Option<String>,
    pub field_type: Option<String>,
    pub capture: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPreviewResult {
    pub relative_path: String,
    pub extracted: HashMap<String, String>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarSource {
    pub source_id: String,
    pub source_name: String,
    pub semantic_expression: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeInterpretation {
    pub semantic_expression: String,
    pub confidence: f32,
    pub description: String,
    /// Why this interpretation differs
    pub difference_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternativesViewData {
    /// List of alternatives
    pub alternatives: Vec<AlternativeInterpretation>,
    /// Currently selected index
    pub selected_index: usize,
    /// Previous result state to return to
    pub previous_state: Box<SemanticPathState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionErrorData {
    pub error_message: String,
    pub error_type: RecognitionErrorType,
    pub retry_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecognitionErrorType {
    NoPrimitivesMatched,
    AmbiguousStructure,
    InsufficientSamples,
    Timeout,
    ModelUnavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    /// Which result state to return to on Esc
    pub previous_state: Box<SemanticPathState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditingData {
    pub temp_file_path: PathBuf,
    pub original_content: String,
    /// Which result state to return to if unmodified
    pub previous_state: Box<SemanticPathState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegeneratingData {
    /// Accumulated hints
    pub hints: Vec<String>,
    /// Sample paths
    pub sample_paths: Vec<PathBuf>,
    /// Source ID if analyzing a source
    pub source_id: Option<String>,
    /// Selected alternative (if from ALTERNATIVES_VIEW)
    pub selected_alternative: Option<AlternativeInterpretation>,
    /// Manual edits if any
    pub manual_content: Option<String>,
}
```

---

### Alternatives View Sub-State Machine

The ALTERNATIVES_VIEW state has its own internal navigation:

```
+------------------------------------------------------------------------+
|                         ALTERNATIVES_VIEW                                |
|                                                                         |
|  +------------------------------------------------------------------+   |
|  | Alternative Interpretations                          Confidence   |   |
|  +------------------------------------------------------------------+   |
|  | > entity_folder(mission) > dated_hierarchy(iso)      94%         |   |
|  |   entity_folder(project) > dated_hierarchy(iso)      87%         |   |
|  |   category_folder > dated_hierarchy(ymd)             71%         |   |
|  |   literal > literal > dated_hierarchy(iso)           58%         |   |
|  +------------------------------------------------------------------+   |
|                                                                         |
|  j/Down: Next alternative                                               |
|  k/Up: Previous alternative                                             |
|  Enter: Select highlighted alternative                                  |
|  Esc: Cancel, return to result state                                    |
+------------------------------------------------------------------------+
```

---

### Similar Sources Display

The Similar Sources section in RESULT_SHOWN displays sources with the same abstract semantic structure:

```
+- Similar Sources --------------------------------------------------+
|  This structure matches: entity_folder > dated_hierarchy            |
|  - defense_contractor_a (same pattern) - 1,247 files               |
|  - research_lab_data (same pattern) - 892 files                    |
|                                                                     |
|  Press 'S' to open Sources Manager for bulk rule application        |
+---------------------------------------------------------------------+
```

**Note:** The Similar Sources section is display-only in this wizard. Bulk rule application happens through the Sources Manager (separate workflow).

---

### Examples

**Example 1: Happy path - High confidence recognition**

```
1. User selects source "/mnt/mission_data"
2. User presses 'S' (invoke Semantic Path Wizard)
3. State: RECOGNIZING
   - Spinner: "Analyzing folder structure..."
   - Samples 20 files, detects patterns
4. Recognition succeeds with 94% confidence
5. State: RESULT_HIGH_CONFIDENCE
   - Semantic: entity_folder(mission) > dated_hierarchy(iso) > files
   - Confidence bar: [================..] 94%
   - Path breakdown shows segment analysis
   - Generated rule with glob pattern
   - 5-file preview
   - Similar sources: defense_contractor_a, research_lab
6. User edits tag: "mission_data"
7. User presses Enter
8. State: APPROVED
   - Rule written to ~/.casparian_flow/extraction_rules/mission_data.yaml
   - Tag "mission_data" applied to matching files
   - Dialog closes
```

**Example 2: Low confidence - User views alternatives**

```
1. User invokes wizard on source with ambiguous structure
2. State: RECOGNIZING -> RESULT_LOW_CONFIDENCE
   - Semantic: category_folder > files
   - Confidence: 65%
   - Message: "Low confidence. Consider viewing alternatives."
3. User presses 'a'
4. State: ALTERNATIVES_VIEW
   - Shows 4 alternatives:
     > category_folder > files (65%)
       entity_folder(department) > files (61%)
       literal > files (45%)
       unknown > files (30%)
5. User navigates with j/k, selects "entity_folder(department)"
6. User presses Enter
7. State: REGENERATING
   - Re-analyzes with selected interpretation
8. State: RESULT_HIGH_CONFIDENCE (new interpretation)
   - Confidence now 88% (with user confirmation)
9. User approves
```

**Example 3: Hint refinement**

```
1. User has RESULT_LOW_CONFIDENCE
   - Detected: literal > dated_hierarchy
   - First folder "PROJ_A" not recognized as entity
2. User presses 'h'
3. State: HINT_INPUT
   - User types: "PROJ_* folders are project identifiers"
4. User presses Enter
5. State: REGENERATING
   - Re-analyzes with hint context
6. State: RESULT_HIGH_CONFIDENCE
   - Semantic: entity_folder(project) > dated_hierarchy(iso)
   - Now correctly extracts project_id from PROJ_A, PROJ_B, etc.
7. User approves
```

**Example 4: Recognition error - No primitives matched**

```
1. User invokes wizard on source with unusual structure
   - Paths like: /data/abc123-def456/file.csv (hash-based)
2. State: RECOGNIZING
3. Algorithm finds no matching semantic primitives
4. State: RECOGNITION_ERROR
   - Message: "Could not recognize semantic structure."
   - Suggestion: "Try Pathfinder Wizard for custom patterns."
5. User presses Esc
6. State: CLOSED
   - User later uses Pathfinder for custom extraction
```

**Example 5: Manual edit of generated rule**

```
1. User has RESULT_HIGH_CONFIDENCE
   - Generated glob is too broad
2. User presses 'e'
3. State: EDITING
   - Opens /tmp/casparian_draft_xyz789.yaml in $EDITOR
   - User refines glob pattern, adds constraints
4. User saves and closes editor
5. State: REGENERATING
   - Validates edited rule against sample files
6. State: RESULT_HIGH_CONFIDENCE (updated)
   - Preview shows corrected matches
7. User approves
```

---

### Trade-offs

**Pros:**

1. **Confidence-based UX** - HIGH/LOW states guide user behavior (approve vs explore alternatives)
2. **Alternatives view** - Semantic primitives naturally have multiple interpretations; this surfaces them
3. **Similar sources discovery** - Cross-source pattern matching is a key differentiator from Pathfinder
4. **Consistent escape paths** - Same Esc pattern as Pathfinder and Parser Lab
5. **Vocabulary-driven** - Uses named primitives, not ad-hoc patterns; more portable
6. **Always YAML** - Simpler than Pathfinder's dual-output; semantic primitives are inherently declarative

**Cons:**

1. **Confidence threshold (80%)** - Arbitrary cutoff; may need tuning
2. **Alternatives may overwhelm** - 10+ alternatives could be confusing; should limit display
3. **Similar sources is read-only** - No bulk action from this wizard; requires context switch
4. **AI disambiguation hidden** - When AI resolves ambiguity, user doesn't see the reasoning

**Mitigations:**

1. Confidence threshold is configurable; default 80% based on UX research
2. Limit alternatives display to top 5 by confidence; "Show more" expands
3. Accept: Similar sources is discovery-only; bulk application is Sources Manager's job
4. Add "Why this interpretation?" toggle that shows AI reasoning when applicable

---

### New Gaps Introduced

1. **GAP-SEMANTIC-001**: Confidence score computation not specified:
   - How is confidence calculated from primitive matching?
   - Does AI disambiguation affect confidence score?
   - Should confidence include preview success rate?

2. **GAP-SEMANTIC-002**: Similar sources matching algorithm:
   - Is match based on abstract expression (ignoring parameters)?
   - How is "same pattern" defined? (exact expression vs equivalent structure)
   - Performance concerns for large source counts?

3. **GAP-SEMANTIC-003**: Alternative generation strategy:
   - What primitives are considered for each segment?
   - How many alternatives to generate (max)?
   - Should alternatives include combinations not detected initially?

4. **GAP-SEMANTIC-004**: Low confidence approval confirmation:
   - Modal confirmation dialog needed?
   - Or inline [y/N] prompt?
   - What percentage triggers confirmation? (<80%? <50%?)

5. **GAP-SEMANTIC-005**: Semantic vocabulary reference:
   - Wizard references primitives from specs/semantic_path_mapping.md
   - That spec is marked as "reference broken" in status.md
   - Need to define or create the semantic vocabulary spec

---

### Validation Checklist

- [x] Diagram included with all states
- [x] Entry/exit conditions documented for each state
- [x] All keybindings from Section 5.5 appear in transition table: [Enter, a, e, h, Esc]
- [x] Esc behavior is consistent (cancel/back/close)
- [x] No orphan states (all reachable from RECOGNIZING, all can exit)
- [x] Confidence states differentiate high/low for UX guidance
- [x] Alternatives view has navigation (j/k) and selection (Enter)
- [x] Similar sources display specified
- [x] Data model includes semantic primitives and alternatives

---

### References

- `specs/ai_wizards.md` Section 3.4 (Semantic Path Wizard)
- `specs/ai_wizards.md` Section 5.5 (TUI Dialog mockup)
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine - used as template)
- `specs/ai_wizards.md` Section 3.5 (Path Intelligence Engine - related clustering)
- `specs/semantic_path_mapping.md` (Semantic vocabulary - referenced but may not exist)

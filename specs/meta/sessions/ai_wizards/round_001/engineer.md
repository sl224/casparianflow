# Engineer Round 001: GAP-STATE-001

## Gap Resolution: GAP-STATE-001

**Gap:** The Pathfinder Wizard TUI dialog (Section 5.1) shows UI mockups but has no state machine.

**Confidence:** HIGH

---

### Proposed Solution

The Pathfinder Wizard is a modal dialog that guides users through generating extraction rules (YAML) or Python extractors from file paths. The state machine must handle:
1. Initial analysis phase
2. Results display with detected patterns
3. User refinement via hints or manual editing
4. Regeneration cycles
5. Approval or cancellation
6. Both YAML and Python output paths

#### State Diagram

```
                                ┌─────────────────────────────────────────────────────────────────┐
                                │                     PATHFINDER WIZARD                            │
                                └─────────────────────────────────────────────────────────────────┘

                                                          │
                                                          │ User invokes wizard
                                                          │ (w on file, W→p menu)
                                                          ▼
                                                 ┌─────────────────┐
                                                 │    ANALYZING    │
                                                 │  (entry state)  │
                                                 └────────┬────────┘
                                                          │
                               ┌──────────────────────────┼──────────────────────────┐
                               │                          │                          │
                               ▼                          ▼                          ▼
                      ┌─────────────────┐        ┌─────────────────┐       ┌─────────────────┐
                      │  ANALYSIS_ERROR │        │   YAML_RESULT   │       │  PYTHON_RESULT  │
                      │                 │        │   (preferred)   │       │   (fallback)    │
                      └────────┬────────┘        └────────┬────────┘       └────────┬────────┘
                               │                          │                          │
                               │                          └────────────┬─────────────┘
                               │                                       │
                               │                          ┌────────────┴────────────┐
                               │                          │                         │
                               │                          │      RESULT_SHOWN       │
                               │                          │   (abstract parent)     │
                               │                          │                         │
                               │                          └────────────┬────────────┘
                               │                                       │
          ┌────────────────────┤           ┌───────────────────────────┼───────────────────────────┐
          │                    │           │              │            │             │             │
          ▼                    │           ▼              ▼            ▼             ▼             ▼
  ┌───────────────┐           │   ┌───────────────┐ ┌─────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐
  │    CLOSED     │◄──────────┼───│   APPROVED    │ │  HINT   │ │  EDITING  │ │REGENERATE │ │ CANCELED  │
  │  (terminal)   │           │   │  (terminal)   │ │  INPUT  │ │  (ext.)   │ │   ING     │ │(terminal) │
  └───────────────┘           │   └───────────────┘ └────┬────┘ └─────┬─────┘ └─────┬─────┘ └───────────┘
                              │                          │            │             │
                              │                          │            │             │
                              │                          └────────────┴─────────────┘
                              │                                       │
                              │                                       │ (back to ANALYZING)
                              │                                       │
                              └───────────────────────────────────────┘
```

**Simplified Linear View:**

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│  ANALYZING  │─────►│YAML_RESULT  │      │ HINT_INPUT  │─────►│REGENERATING │
│             │      │    or       │◄────►│             │      │             │
└─────────────┘      │PYTHON_RESULT│      └─────────────┘      └──────┬──────┘
      │              └──────┬──────┘                                  │
      │                     │                                         │
      ▼                     │         ┌───────────────────────────────┘
┌─────────────┐             │         │
│ANALYSIS_ERR │             ▼         ▼
└─────────────┘      ┌─────────────┐
      │              │  APPROVED   │
      │              │     or      │
      │              │  CANCELED   │
      ▼              └─────────────┘
┌─────────────┐
│   CLOSED    │
└─────────────┘
```

---

#### State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| **ANALYZING** | Wizard invoked with sample path(s) | Analysis completes or fails | Spinner shown. AI/algorithm analyzes path structure. User cannot interact except Esc to cancel. |
| **YAML_RESULT** | Analysis succeeded, all patterns expressible in YAML | User action (Enter/h/e/r/Esc) | Shows detected patterns, generated YAML rule, preview. User can approve, hint, edit, regenerate, or cancel. |
| **PYTHON_RESULT** | Analysis succeeded, complex patterns require Python | User action (Enter/h/e/r/Esc) | Shows detected patterns with "(requires Python)" note, generated Python code, preview. Same actions as YAML_RESULT. |
| **ANALYSIS_ERROR** | AI/algorithm failed to analyze path | User action (r/Esc) | Shows error message. User can retry (r) or cancel (Esc). |
| **HINT_INPUT** | User pressed 'h' from result state | Enter submits, Esc cancels | Text input field active. User types hint. Examples shown. |
| **EDITING** | User pressed 'e' from result state | User saves and closes $EDITOR | Draft opened in $EDITOR. TUI shows waiting message. Validation runs on return. |
| **REGENERATING** | User submitted hint or saved edit | Regeneration completes, fails, or user cancels | Spinner shown. AI regenerates with new context. User can press Esc to cancel. Returns to result state or error. |
| **APPROVED** | User pressed Enter from result state | Immediate | Draft committed to Layer 1 (extractors/ or extraction_rules/). Dialog closes. |
| **CANCELED** | User pressed Esc from any non-input state | Immediate | Draft discarded. Dialog closes. |
| **CLOSED** | Analysis error with Esc | Immediate | Dialog closes. No draft created. |

---

#### Transitions

| From | To | Trigger | Guard |
|------|----|---------| ------|
| (external) | ANALYZING | User invokes wizard | Sample path(s) provided |
| ANALYZING | YAML_RESULT | Analysis completes | All patterns YAML-expressible |
| ANALYZING | PYTHON_RESULT | Analysis completes | Complex patterns detected |
| ANALYZING | ANALYSIS_ERROR | Analysis fails | Timeout, model unavailable, or parse error |
| ANALYZING | CANCELED | Esc | - |
| YAML_RESULT | APPROVED | Enter | Rule name valid (non-empty) |
| YAML_RESULT | HINT_INPUT | h | - |
| YAML_RESULT | EDITING | e | $EDITOR available |
| YAML_RESULT | REGENERATING | r | - |
| YAML_RESULT | CANCELED | Esc | - |
| PYTHON_RESULT | APPROVED | Enter | Extractor name valid (non-empty) |
| PYTHON_RESULT | HINT_INPUT | h | - |
| PYTHON_RESULT | EDITING | e | $EDITOR available |
| PYTHON_RESULT | REGENERATING | r | - |
| PYTHON_RESULT | CANCELED | Esc | - |
| ANALYSIS_ERROR | REGENERATING | r | retry_count < 3 |
| ANALYSIS_ERROR | CLOSED | Esc | - |
| ANALYSIS_ERROR | CLOSED | r | retry_count >= 3 |
| HINT_INPUT | REGENERATING | Enter | Hint text non-empty |
| HINT_INPUT | (previous result state) | Esc | - |
| EDITING | REGENERATING | Editor closes | File modified |
| EDITING | (previous result state) | Editor closes | File unmodified |
| REGENERATING | YAML_RESULT | Regeneration completes | All patterns YAML-expressible |
| REGENERATING | PYTHON_RESULT | Regeneration completes | Complex patterns detected |
| REGENERATING | ANALYSIS_ERROR | Regeneration fails | - |
| REGENERATING | CANCELED | Esc | - |
| APPROVED | (dialog closes) | - | Commit to Layer 1 |
| CANCELED | (dialog closes) | - | Discard draft |
| CLOSED | (dialog closes) | - | No draft to discard |

---

#### Keybinding Summary by State

| Key | ANALYZING | YAML_RESULT | PYTHON_RESULT | ANALYSIS_ERROR | HINT_INPUT | EDITING | REGENERATING |
|-----|-----------|-------------|---------------|----------------|------------|---------|--------------|
| Enter | - | Approve | Approve | - | Submit hint | - | - |
| Esc | Cancel | Cancel | Cancel | Close | Back to result | - | Cancel |
| h | - | Open hint | Open hint | - | - | - | - |
| e | - | Open editor | Open editor | - | - | - | - |
| r | - | Regenerate | Regenerate | Retry | - | - | - |
| (typing) | - | Edit name field | Edit name field | - | Input text | - | - |

---

#### Data Model (Rust structs)

```rust
/// Pathfinder Wizard state
#[derive(Debug, Clone, PartialEq)]
pub enum PathfinderState {
    Analyzing,
    YamlResult(PathfinderResultData),
    PythonResult(PathfinderResultData),
    AnalysisError(AnalysisErrorData),
    HintInput(HintInputData),
    Editing(EditingData),
    Regenerating(RegeneratingData),
    Approved,
    Canceled,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathfinderResultData {
    /// Sample paths analyzed
    pub sample_paths: Vec<PathBuf>,
    /// Detected patterns with keep/ignore status
    pub patterns: Vec<DetectedPattern>,
    /// Generated rule (YAML) or code (Python)
    pub generated_content: String,
    /// Preview of extraction on sample files
    pub preview: Vec<PreviewResult>,
    /// User-editable name field
    pub name: String,
    /// Why Python was chosen (only for PythonResult)
    pub python_reason: Option<String>,
    /// Number of regeneration attempts
    pub regeneration_count: u32,
    /// User hints accumulated
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedPattern {
    pub segment_value: String,
    pub field_name: String,
    pub field_type: String,
    pub keep: bool,
    pub requires_python: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewResult {
    pub filename: String,
    pub extracted: Option<HashMap<String, String>>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisErrorData {
    pub error_message: String,
    pub error_type: AnalysisErrorType,
    pub retry_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisErrorType {
    Timeout,
    ModelUnavailable,
    ParseError,
    EmptyResponse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    /// Which result state to return to on Esc
    pub previous_state: Box<PathfinderState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditingData {
    pub temp_file_path: PathBuf,
    pub original_content: String,
    /// Which result state to return to if unmodified
    pub previous_state: Box<PathfinderState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegeneratingData {
    /// Accumulated hints including new one
    pub hints: Vec<String>,
    /// Original sample paths
    pub sample_paths: Vec<PathBuf>,
    /// Manual edits if any
    pub manual_content: Option<String>,
}
```

---

### Examples

**Example 1: Happy path - YAML rule approved**

```
1. User selects file /data/ADT_Inbound/2024/01/msg_001.hl7
2. User presses 'w' (invoke Pathfinder)
3. State: ANALYZING
   - Spinner: "Analyzing path structure..."
4. Analysis succeeds, all patterns YAML-expressible
5. State: YAML_RESULT
   - Shows detected patterns
   - Shows generated YAML rule
   - Shows 5-file preview
6. User edits name field: "healthcare_path"
7. User presses Enter
8. State: APPROVED
   - Rule written to ~/.casparian_flow/extraction_rules/healthcare_path.yaml
   - Dialog closes
```

**Example 2: Hint refinement flow**

```
1. User invokes wizard on /data/CLIENT-ABC/2024/Q1/report.csv
2. State: ANALYZING -> YAML_RESULT
   - AI suggests: year, quarter
   - Misses: client_id
3. User presses 'h'
4. State: HINT_INPUT
   - User types: "The CLIENT-XXX folder is the client identifier"
5. User presses Enter
6. State: REGENERATING
   - Spinner: "Regenerating with hint..."
7. State: YAML_RESULT (updated)
   - Now includes client_id extraction
8. User approves
```

**Example 3: Python fallback due to computed fields**

```
1. User invokes wizard with hint: "Quarter should compute start/end month"
2. State: ANALYZING
3. State: PYTHON_RESULT
   - Note: "Computed fields require Python"
   - Shows Python extractor with quarter calculation
4. User approves
   - Extractor written to ~/.casparian_flow/extractors/client_reports.py
```

**Example 4: Error and recovery**

```
1. User invokes wizard
2. State: ANALYZING
3. Ollama not running
4. State: ANALYSIS_ERROR
   - Message: "Model unavailable. Start Ollama: `ollama serve`"
5. User starts Ollama in another terminal
6. User presses 'r' (retry)
7. State: REGENERATING -> YAML_RESULT
```

**Example 5: Manual edit flow**

```
1. User has YAML_RESULT
2. AI got the regex wrong
3. User presses 'e'
4. State: EDITING
   - Opens /tmp/casparian_draft_abc123.yaml in $EDITOR
   - TUI shows: "Editing in vim... Press any key when done"
5. User fixes regex, saves, closes vim
6. State: REGENERATING (validation)
7. State: YAML_RESULT (updated)
   - Preview shows correct extraction
```

---

### Trade-offs

**Pros:**
1. **Clear separation of result types** - YAML_RESULT vs PYTHON_RESULT makes output format explicit
2. **Escapability** - Every state has an exit path (Esc always works)
3. **Iterative refinement** - Hint and edit cycles without losing context
4. **Retry limits** - Prevents infinite regeneration loops (max 3 retries on error)
5. **Previous state tracking** - HintInput and Editing know where to return on cancel

**Cons:**
1. **Complexity of parent-child states** - YAML_RESULT and PYTHON_RESULT share behavior; could use enum with type parameter instead
2. **External editor blocking** - EDITING state requires waiting for subprocess; TUI thread management needed
3. **Hint accumulation** - Multiple hints are accumulated but not shown in UI (may confuse users)

**Mitigation for cons:**
1. Use `PathfinderResultData` struct for shared behavior; only the state name differs
2. Use async subprocess spawn with poll-based checking
3. Add "Previous hints" section to result view when hints.len() > 0

---

### New Gaps Introduced

1. **GAP-TUI-001**: How does the TUI handle $EDITOR subprocess? Need to specify:
   - Suspend TUI or run in background?
   - How to detect editor close?
   - What if editor crashes?

2. **GAP-YAML-001**: YAML rule file naming convention not specified. Currently uses user-provided name, but:
   - What characters are allowed?
   - Name collision handling?
   - Should auto-generate if empty?

3. **GAP-FOCUS-001**: Focus management in result state not fully specified:
   - Can user focus individual detected patterns to toggle keep/ignore?
   - How does Tab cycle between name field and pattern list?

---

### Validation Checklist (per Section 15.5)

- [x] Diagram included with all states
- [x] Entry/exit conditions documented for each state
- [x] All keybindings appear in transition table
- [x] Esc behavior is consistent (cancel/back/close)
- [x] No orphan states (all reachable from ANALYZING, all can exit)

---

### References

- `specs/ai_wizards.md` Section 3.1 (Pathfinder Wizard)
- `specs/ai_wizards.md` Section 5.1 (TUI Dialog mockups)
- `specs/meta/spec_refinement_workflow_v2.md` Section 15 (State Machine Requirements)

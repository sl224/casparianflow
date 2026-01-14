# Engineer Round 003: GAP-STATE-003

## Gap Resolution: GAP-STATE-003

**Gap:** The Labeling Wizard has no TUI dialog mockup in Section 5 and no state machine. Only the functional description in Section 3.3 exists.

**Confidence:** HIGH

---

### Proposed Solution

The Labeling Wizard is a modal dialog that suggests semantic labels for file groups based on content structure (headers, sample values). Unlike other wizards that generate code, the Labeling Wizard outputs a **tag applied to a Signature Group**.

**Key Characteristics:**
1. Input is a file group (multiple files with same fingerprint/structure)
2. Analyzes headers and sample content
3. Proposes semantic labels with confidence scores
4. User can accept, modify, request alternatives, or provide hints
5. Commits label to `cf_signature_groups.label` column
6. Supports both single-group and multi-group labeling

**Key Differences from Pathfinder and Parser Lab:**
- No code generation (outputs a tag/label string)
- Works on Signature Groups, not individual files
- Has "alternatives" navigation (multiple AI suggestions)
- No validation step (label is just a string)
- Multi-group batch mode for efficiency

---

### TUI Dialog Design

#### Single Group Labeling

```
+-- LABELING WIZARD --------------------------------------------------------+
|                                                                            |
|  Signature Group: sg_a7b3c9d2 (47 files)                                  |
|                                                                            |
|  +-- Structure Analysis --------------------------------------------------+|
|  |  Headers: [id, txn_date, amount, customer_email, status]               ||
|  |  Format: CSV (delimiter: ',')                                          ||
|  |  Sample Values:                                                        ||
|  |    id:             [1001, 1002, 1003, ...]                             ||
|  |    txn_date:       [2024-01-15, 2024-01-16, ...]                       ||
|  |    amount:         [$100.00, $250.50, $75.00, ...]                     ||
|  |    customer_email: [a@b.com, c@d.com, ...]                             ||
|  |    status:         [completed, pending, refunded, ...]                 ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  +-- AI Suggestion -------------------------------------------------------+|
|  |                                                                        ||
|  |  Suggested Label: "Sales Transactions"                                 ||
|  |  Confidence: |================    | 89%                                ||
|  |                                                                        ||
|  |  Reasoning: "Contains transaction dates, monetary amounts with         ||
|  |  currency symbols, customer emails, and transaction status fields.    ||
|  |  Pattern consistent with e-commerce or point-of-sale data."           ||
|  |                                                                        ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  +-- Alternatives (Tab to cycle) -----------------------------------------+|
|  |  [ ] Invoice Records      (72%)                                        ||
|  |  [ ] Payment History      (65%)                                        ||
|  |  [ ] Customer Orders      (61%)                                        ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  Label: Sales Transactions_____________                                    |
|                                                                            |
|  [Enter] Apply   [Tab] Alternatives   [h] Hint   [r] Regenerate   [Esc]   |
+----------------------------------------------------------------------------+
```

#### Multi-Group Labeling (Batch Mode)

```
+-- LABELING WIZARD (Batch) ------------------------------------------------+
|                                                                            |
|  Labeling 5 Signature Groups                                               |
|                                                                            |
|  +-- Groups to Label -----------------------------------------------------+|
|  |  #  | Signature    | Files | Headers Preview            | Suggested   ||
|  |----|--------------|-------|----------------------------|-------------||
|  | >1 | sg_a7b3c9d2  |    47 | id, txn_date, amount, ...  | Sales Trans ||
|  |  2 | sg_f8e2d1c0  |    23 | customer_id, name, addr... | Customer Da ||
|  |  3 | sg_b4c5d6e7  |    12 | product_sku, qty, price... | Inventory   ||
|  |  4 | sg_c7d8e9f0  |     8 | emp_id, date, hours, ...   | Timesheet   ||
|  |  5 | sg_d9e0f1a2  |     3 | log_ts, level, message,... | App Logs    ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  +-- Selected Group Detail -----------------------------------------------+|
|  |  Signature: sg_a7b3c9d2 (47 files)                                     ||
|  |  Headers: [id, txn_date, amount, customer_email, status]               ||
|  |                                                                        ||
|  |  Suggested: "Sales Transactions" (89%)                                 ||
|  |  Alternatives: Invoice Records, Payment History, Customer Orders       ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  +-- Actions -------------------------------------------------------------+|
|  |  [Enter] Accept suggestion   [e] Edit label   [Tab] Next alternative   ||
|  |  [j/k] Navigate groups       [a] Accept all   [Esc] Cancel             ||
|  +------------------------------------------------------------------------+|
|                                                                            |
|  Progress: [==          ] 1/5 labeled                                      |
|                                                                            |
+----------------------------------------------------------------------------+
```

#### Hint Input Overlay

```
+-- PROVIDE HINT -----------------------------------------------------------+
|                                                                            |
|  Help the AI understand this data better:                                  |
|                                                                            |
|  > These are financial transactions from our retail POS system____________ |
|                                                                            |
|  Examples:                                                                 |
|    * "This is healthcare data with patient records"                        |
|    * "These are log files from our web server"                             |
|    * "Internal employee timesheet data"                                    |
|    * "E-commerce order fulfillment records"                                |
|                                                                            |
|  [Enter] Submit hint   [Esc] Cancel                                        |
+----------------------------------------------------------------------------+
```

---

### State Machine

#### State Diagram

```
                                +------------------------------------------------+
                                |               LABELING WIZARD                    |
                                +------------------------------------------------+
                                                      |
                                                      | User invokes wizard
                                                      | (l on file group, W->l menu)
                                                      v
                                             +----------------+
                                             |   ANALYZING    |
                                             | (entry state)  |
                                             +-------+--------+
                                                     |
                          +--------------------------+---------------------------+
                          |                          |                           |
                          v                          v                           v
                 +----------------+         +----------------+          +----------------+
                 | ANALYSIS_ERROR |         | SINGLE_RESULT  |          | BATCH_RESULT   |
                 |                |         | (one group)    |          | (multi-group)  |
                 +-------+--------+         +-------+--------+          +--------+-------+
                         |                          |                            |
                         |                          +------------+---------------+
                         |                                       |
                         |                          +------------+------------+
                         |                          |                         |
                         |                          v                         v
                         |                 +----------------+        +----------------+
                         |                 |  RESULT_SHOWN  |        |                |
                         |                 |(abstract parent)|       |                |
                         |                 +-------+--------+        |                |
                         |                         |                 |                |
     +-------------------+                         |                 |                |
     |                   |          +--------------+---------+-------+-------+        |
     |                   |          |              |         |       |       |        |
     v                   |          v              v         v       v       v        |
+--------+              |    +----------+   +--------+ +--------+ +----+ +--------+   |
| CLOSED |<-------------+----| APPROVED |   | HINT   | | EDITING| |REGEN| |CANCELED|<-+
|(term.) |              |    | (term.)  |   | INPUT  | |        | |    | |(term.) |
+--------+              |    +----------+   +---+----+ +---+----+ +--+-+ +--------+
                        |                       |          |        |
                        |                       +----------+--------+
                        |                                  |
                        |                        (back to ANALYZING)
                        |                                  |
                        +----------------------------------+
```

**Simplified Linear View:**

```
+-------------+      +--------------------------------+      +-------------+
|  ANALYZING  |----->| SINGLE_RESULT / BATCH_RESULT   |----->|  APPROVED   |
+------+------+      +---------------+----------------+      +-------------+
       |                             |
       v                   +---------+---------+
+-------------+            |         |         |
|ANALYSIS_ERR |      +-----+---+ +---+---+ +---+------+
+------+------+      |HINT_INP | |EDITING| |REGENERATE|
       |             +---------+ +-------+ +----------+
       v                   |         |         |
+-------------+            +---------+---------+
|   CLOSED    |                      |
+-------------+            (back to ANALYZING for re-analysis)
```

---

#### State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| **ANALYZING** | Wizard invoked with signature group(s) | Analysis completes or fails | Spinner shown. AI analyzes headers and sample values. User can Esc to cancel. |
| **SINGLE_RESULT** | Analysis succeeded for one group | User action (Enter/Tab/h/e/r/Esc) | Shows structure analysis, suggested label with confidence, reasoning, and alternatives. User can accept, cycle alternatives, hint, edit, regenerate, or cancel. |
| **BATCH_RESULT** | Analysis succeeded for multiple groups | User action (Enter/j/k/a/Tab/h/e/r/Esc) | Shows group list with suggestions. User navigates groups (j/k), accepts individual (Enter), accepts all (a), or refines selected group. |
| **ANALYSIS_ERROR** | AI failed to generate suggestions | User action (r/Esc) | Shows error message. User can retry (r) or cancel (Esc). Max 3 retries. |
| **HINT_INPUT** | User pressed 'h' from result state | Enter submits, Esc cancels | Text input field active. User provides domain context. |
| **EDITING** | User pressed 'e' from result state | Enter confirms, Esc cancels | Direct label text editing in TUI (no external editor needed). |
| **REGENERATING** | User submitted hint | Regeneration completes/fails | Spinner shown. AI regenerates with new context. |
| **APPROVED** | User pressed Enter to accept label | Immediate | Label committed to `cf_signature_groups.label`. Tags propagated to files. Dialog closes. |
| **CANCELED** | User pressed Esc from result state | Immediate | No changes made. Dialog closes. |
| **CLOSED** | Esc from ANALYSIS_ERROR | Immediate | Dialog closes. |

---

#### Transitions

| From | To | Trigger | Guard |
|------|----|---------|-------|
| (external) | ANALYZING | User invokes wizard | Signature group(s) provided |
| ANALYZING | SINGLE_RESULT | Analysis completes | Single group provided |
| ANALYZING | BATCH_RESULT | Analysis completes | Multiple groups provided |
| ANALYZING | ANALYSIS_ERROR | Analysis fails | Timeout, model unavailable, empty headers |
| ANALYZING | CANCELED | Esc | - |
| SINGLE_RESULT | APPROVED | Enter | Label non-empty |
| SINGLE_RESULT | HINT_INPUT | h | - |
| SINGLE_RESULT | EDITING | e | - |
| SINGLE_RESULT | REGENERATING | r | - |
| SINGLE_RESULT | SINGLE_RESULT | Tab | Cycle to next alternative (wraps around) |
| SINGLE_RESULT | CANCELED | Esc | - |
| BATCH_RESULT | BATCH_RESULT | j/Down | Move to next group (wraps) |
| BATCH_RESULT | BATCH_RESULT | k/Up | Move to previous group (wraps) |
| BATCH_RESULT | BATCH_RESULT | Enter | Accept current group's suggestion, move to next |
| BATCH_RESULT | BATCH_RESULT | Tab | Cycle alternative for current group |
| BATCH_RESULT | APPROVED | a | Accept all remaining suggestions |
| BATCH_RESULT | APPROVED | (implicit) | All groups labeled via Enter |
| BATCH_RESULT | HINT_INPUT | h | Hint for current group |
| BATCH_RESULT | EDITING | e | Edit label for current group |
| BATCH_RESULT | REGENERATING | r | Regenerate for current group |
| BATCH_RESULT | CANCELED | Esc | - |
| ANALYSIS_ERROR | REGENERATING | r | retry_count < 3 |
| ANALYSIS_ERROR | CLOSED | Esc | - |
| ANALYSIS_ERROR | CLOSED | r | retry_count >= 3 |
| HINT_INPUT | REGENERATING | Enter | Hint text non-empty |
| HINT_INPUT | (previous state) | Esc | - |
| EDITING | (previous state) | Enter | Confirm edited label |
| EDITING | (previous state) | Esc | Cancel edit, restore previous |
| REGENERATING | SINGLE_RESULT | Completes | Single group mode |
| REGENERATING | BATCH_RESULT | Completes | Batch mode (updates current group) |
| REGENERATING | ANALYSIS_ERROR | Fails | - |
| REGENERATING | CANCELED | Esc | - |

---

#### Keybinding Summary by State

| Key | ANALYZING | SINGLE_RESULT | BATCH_RESULT | ANALYSIS_ERROR | HINT_INPUT | EDITING | REGENERATING |
|-----|-----------|---------------|--------------|----------------|------------|---------|--------------|
| Enter | - | Accept label | Accept current, next | - | Submit hint | Confirm edit | - |
| Esc | Cancel | Cancel | Cancel | Close | Back | Cancel edit | Cancel |
| Tab | - | Next alternative | Next alt (current) | - | - | - | - |
| h | - | Open hint | Open hint (current) | - | - | - | - |
| e | - | Edit label | Edit (current) | - | - | - | - |
| r | - | Regenerate | Regenerate (current) | Retry | - | - | - |
| j/Down | - | - | Next group | - | - | - | - |
| k/Up | - | - | Previous group | - | - | - | - |
| a | - | - | Accept all | - | - | - | - |
| (typing) | - | (in label field) | - | - | Input text | Edit text | - |

---

#### Data Model (Rust structs)

```rust
/// Labeling Wizard state
#[derive(Debug, Clone, PartialEq)]
pub enum LabelingState {
    Analyzing,
    SingleResult(LabelingResultData),
    BatchResult(BatchLabelingData),
    AnalysisError(AnalysisErrorData),
    HintInput(HintInputData),
    Editing(EditingData),
    Regenerating(RegeneratingData),
    Approved,
    Canceled,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelingResultData {
    /// Signature group being labeled
    pub signature_group_id: String,
    /// Number of files in this group
    pub file_count: usize,
    /// Content structure analysis
    pub structure: ContentStructure,
    /// AI suggestions with confidence scores
    pub suggestions: Vec<LabelSuggestion>,
    /// Currently selected suggestion index
    pub selected_index: usize,
    /// User-edited label (if different from suggestion)
    pub edited_label: Option<String>,
    /// Hints accumulated
    pub hints: Vec<String>,
    /// Regeneration count
    pub regeneration_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentStructure {
    /// File format (CSV, JSON, etc.)
    pub format: String,
    /// Format-specific details
    pub format_details: HashMap<String, String>,
    /// Column/field headers
    pub headers: Vec<String>,
    /// Sample values per column (up to 5)
    pub sample_values: HashMap<String, Vec<String>>,
    /// Inferred data types per column
    pub inferred_types: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelSuggestion {
    /// Suggested label/tag name
    pub label: String,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
    /// AI reasoning for this suggestion
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchLabelingData {
    /// All groups to label
    pub groups: Vec<LabelingResultData>,
    /// Currently selected group index
    pub current_index: usize,
    /// Groups that have been labeled (accepted)
    pub labeled_indices: HashSet<usize>,
}

impl BatchLabelingData {
    pub fn current_group(&self) -> &LabelingResultData {
        &self.groups[self.current_index]
    }

    pub fn current_group_mut(&mut self) -> &mut LabelingResultData {
        &mut self.groups[self.current_index]
    }

    pub fn remaining_count(&self) -> usize {
        self.groups.len() - self.labeled_indices.len()
    }

    pub fn all_labeled(&self) -> bool {
        self.labeled_indices.len() == self.groups.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    /// Which result state to return to on Esc
    pub previous_state: Box<LabelingState>,
    /// Target group index (for batch mode)
    pub target_group_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditingData {
    /// Current label text being edited
    pub label_text: String,
    /// Cursor position in text
    pub cursor_position: usize,
    /// Original label before editing
    pub original_label: String,
    /// Previous state to return to
    pub previous_state: Box<LabelingState>,
    /// Target group index (for batch mode)
    pub target_group_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegeneratingData {
    /// All hints including the new one
    pub hints: Vec<String>,
    /// Signature group(s) being regenerated
    pub signature_group_ids: Vec<String>,
    /// Is batch mode?
    pub is_batch: bool,
    /// Current group index (for batch mode)
    pub current_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisErrorData {
    pub error_message: String,
    pub error_type: LabelingErrorType,
    pub retry_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LabelingErrorType {
    Timeout,
    ModelUnavailable,
    EmptyHeaders,      // File has no headers to analyze
    EmptyFile,         // File is empty
    UnsupportedFormat, // Cannot parse file format
    EmptyResponse,     // AI returned nothing
}

/// Result of committing a label
#[derive(Debug, Clone)]
pub struct LabelCommitResult {
    pub signature_group_id: String,
    pub label: String,
    pub files_tagged: usize,
    pub labeled_by: String, // "ai" or "user"
}
```

---

### Examples

**Example 1: Happy path - Single group labeled**

```
1. User selects file group with signature sg_a7b3c9d2 (47 files)
2. User presses 'l' (invoke Labeling Wizard)
3. State: ANALYZING
   - Spinner: "Analyzing content structure..."
   - Reads headers: [id, txn_date, amount, customer_email, status]
   - Samples 5 values per column
4. State: SINGLE_RESULT
   - Suggested: "Sales Transactions" (89%)
   - Alternatives: Invoice Records (72%), Payment History (65%)
   - Reasoning shown
5. User reviews, accepts default
6. User presses Enter
7. State: APPROVED
   - cf_signature_groups.label = "Sales Transactions"
   - 47 files tagged with "Sales Transactions"
   - Dialog closes
```

**Example 2: Cycle through alternatives**

```
1. User invokes wizard on customer data group
2. State: SINGLE_RESULT
   - Suggested: "Customer Data" (78%)
   - Alternatives: User Accounts (71%), Contact List (65%)
3. User thinks "Contact List" is more accurate
4. User presses Tab twice
   - Now showing: "Contact List" (65%) as selected
5. User presses Enter
6. State: APPROVED with label "Contact List"
```

**Example 3: Provide domain hint**

```
1. User has SINGLE_RESULT with generic suggestion "Data Records" (52%)
2. AI is uncertain - headers are ambiguous
3. User presses 'h'
4. State: HINT_INPUT
   - User types: "This is healthcare data from our EMR system"
5. User presses Enter
6. State: REGENERATING
   - AI regenerates with healthcare context
7. State: SINGLE_RESULT
   - New suggestion: "Patient Encounters" (87%)
   - Better alternatives: Clinical Notes, Lab Results
8. User approves
```

**Example 4: Manual label edit**

```
1. User has SINGLE_RESULT with "Sales Transactions"
2. User wants more specific label
3. User presses 'e'
4. State: EDITING
   - Label field editable: "Sales Transactions" -> "Q4 2024 Sales Transactions"
5. User presses Enter
6. State: APPROVED with edited label
```

**Example 5: Batch mode - Multiple groups**

```
1. User selects 5 unlabeled file groups from Discover mode
2. User presses 'l' (batch labeling)
3. State: ANALYZING (processes all 5)
4. State: BATCH_RESULT
   - Shows table with 5 groups and suggestions
   - First group highlighted
5. User reviews first group:
   - Suggestion looks good, presses Enter
   - Group 1 marked as labeled, moves to Group 2
6. User navigates with j/k to review each
7. For Group 3, user presses Tab to pick alternative
8. For Group 4, user presses 'h' to add hint, regenerates
9. All 5 reviewed, user presses 'a' to accept remaining
10. State: APPROVED
    - All 5 signature groups labeled
    - All associated files tagged
```

**Example 6: Error recovery**

```
1. User invokes wizard on empty file group
2. State: ANALYZING
3. Files have no headers (binary data)
4. State: ANALYSIS_ERROR
   - Message: "Cannot analyze: No headers found. Only structured data (CSV, JSON) supported."
5. User presses Esc
6. State: CLOSED
```

---

### Trade-offs

**Pros:**

1. **Alternatives cycling** - Tab navigation through AI suggestions is intuitive, no dropdown needed
2. **Confidence display** - User can assess AI certainty before accepting
3. **Batch mode** - Efficient for bulk labeling (common use case after initial scan)
4. **Inline editing** - Edit label in TUI without external editor (simpler than Parser Lab)
5. **Signature Group persistence** - Labels survive file moves/renames (structure-based)
6. **Reasoning transparency** - User understands why AI suggested the label

**Cons:**

1. **Two result modes** - SINGLE_RESULT and BATCH_RESULT have overlapping but different UX
2. **No validation** - Labels are free-form strings; typos possible
3. **Batch complexity** - Need to track labeled vs unlabeled groups
4. **Alt cycling state** - selected_index changes but doesn't persist if user navigates away

**Mitigations:**

1. Share `LabelingResultData` between modes; `BatchLabelingData` is a wrapper with navigation
2. Add label autocomplete from existing labels (future enhancement)
3. Use `labeled_indices: HashSet<usize>` for clear tracking
4. Save selected alternative to `LabelingResultData.edited_label` on Tab

---

### Integration with Layer 1

When a label is approved:

1. **Update Signature Group:**
```sql
UPDATE cf_signature_groups
SET label = :label,
    labeled_by = :labeled_by,  -- 'ai' or 'user'
    labeled_at = CURRENT_TIMESTAMP
WHERE id = :signature_group_id;
```

2. **Propagate Tag to Files:**
```sql
UPDATE scout_files
SET tags = json_insert(tags, '$[#]', :label)
WHERE signature_group_id = :signature_group_id
  AND NOT json_contains(tags, :label);
```

3. **Future Files Auto-Inherit:**
When Fingerprint Engine assigns a new file to a labeled signature group, the file automatically inherits the tag via the existing tag propagation logic in Scout.

---

### New Gaps Introduced

1. **GAP-BATCH-001**: How does batch mode handle partial completion?
   - If user accepts 3/5 groups then presses Esc, are the 3 committed?
   - Should there be "Save progress" vs "Commit all at once"?

2. **GAP-LABEL-001**: Label validation/normalization not specified:
   - Character restrictions? (e.g., no special chars)
   - Case normalization? (Title Case, snake_case)
   - Max length?

3. **GAP-ALT-001**: Alternative generation strategy:
   - How many alternatives to generate? (Currently 3-4)
   - What if AI only has one good suggestion?
   - Should alternatives always be shown even if confidence is very high (95%+)?

4. **GAP-SIGGROUP-001**: `cf_signature_groups` table not yet defined in schema:
   - Referenced in Section 3.3 and Section 8.3 but no CREATE TABLE statement
   - Need to define fingerprint, label, labeled_by, labeled_at columns

5. **GAP-REDACT-002**: Labeling Wizard redaction not specified:
   - Section 7.2 shows redaction for Parser Lab (headers + sample values)
   - Labeling Wizard also sends sample values - should it have redaction?

---

### Validation Checklist

- [x] TUI dialog design included (single and batch modes)
- [x] State diagram included with all states
- [x] Entry/exit conditions documented for each state
- [x] All keybindings documented (Enter, Esc, Tab, h, e, r, j, k, a)
- [x] Esc behavior is consistent (cancel/back/close)
- [x] No orphan states (all reachable, all can exit)
- [x] Batch mode navigation specified (j/k, a for accept all)
- [x] Data model includes signature group reference
- [x] Examples cover single, batch, hint, edit, and error flows

---

### References

- `specs/ai_wizards.md` Section 3.3 (Labeling Wizard functional spec)
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine - template)
- `specs/ai_wizards.md` Section 5.2.1 (Parser Lab state machine - template)
- `specs/ai_wizards.md` Section 8.3 (Committing a Label)
- `roadmap/spec_discovery_intelligence.md` (Signature Groups / Fingerprint Engine)

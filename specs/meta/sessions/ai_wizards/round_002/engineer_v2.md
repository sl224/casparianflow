# Engineer Round 002 (v2): GAP-STATE-002

## Gap Resolution: GAP-STATE-002

**Gap:** The Parser Lab Wizard TUI dialog (Section 5.2) shows UI mockups with keybindings [Enter, t, r, e, h, s, Esc] but has no state machine.

**Confidence:** HIGH

**Revision:** v2 - Addressing reviewer feedback from Round 002

---

### Proposed Solution

The Parser Lab Wizard is a modal dialog that guides users through generating Python parsers from sample files. Unlike Pathfinder (which outputs YAML or Python), Parser Lab always outputs Python parser classes. The state machine must handle:

1. Initial analysis/generation phase
2. Results display with detected structure and generated parser
3. Validation states (success, warnings, failures)
4. Testing against additional files
5. Schema setting for type enforcement
6. User refinement via hints or manual editing
7. Regeneration cycles
8. **[REVISED]** Validation-only cycles (after manual edits)
9. Approval or cancellation

**Key Differences from Pathfinder:**
- Single output type (Python parser) vs two (YAML/Python)
- Validation always runs with pass/warn/fail states
- "Test more" feature to validate against additional files
- "Set schema" feature to lock expected types
- Parser metadata fields (name, version, topic)

---

#### State Diagram

**[REVISED]** Added VALIDATING state to distinguish validation-only (after edit) from full regeneration.

```
                                ┌─────────────────────────────────────────────────────────────────┐
                                │                        PARSER LAB                               │
                                └─────────────────────────────────────────────────────────────────┘

                                                          │
                                                          │ User invokes wizard
                                                          │ (g on file group, W→g menu)
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
                      │  ANALYSIS_ERROR │        │ RESULT_VALIDATED│       │  RESULT_WARNING │
                      │                 │        │    (all pass)   │       │ (warnings only) │
                      └────────┬────────┘        └────────┬────────┘       └────────┬────────┘
                               │                          │                          │
                               │                          └────────────┬─────────────┘
                               │                                       │
                               │                          ┌────────────┴────────────┐
                               │                          ▼                         ▼
                               │                 ┌─────────────────┐       ┌─────────────────┐
                               │                 │  RESULT_FAILED  │       │                 │
                               │                 │(validation fail)│       │  RESULT_SHOWN   │
                               │                 └────────┬────────┘       │(abstract parent)│
                               │                          │               └────────┬────────┘
                               │                          └────────────────────────┤
                               │                                                   │
          ┌────────────────────┤          ┌────────────────────────────────────────┼───────────────────────────────────────────┐
          │                    │          │         │            │           │     │      │            │          │           │
          ▼                    │          ▼         ▼            ▼           ▼     ▼      ▼            ▼          ▼           ▼
  ┌───────────────┐           │   ┌───────────┐ ┌───────┐ ┌──────────┐ ┌────────┐ ┌────────┐ ┌─────────────┐ ┌───────────┐ ┌───────────┐
  │    CLOSED     │◄──────────┼───│ APPROVED  │ │ HINT  │ │ EDITING  │ │TESTING │ │ SCHEMA │ │REGENERATING │ │ VALIDATING│ │ CANCELED  │
  │  (terminal)   │           │   │(terminal) │ │ INPUT │ │  (ext.)  │ │        │ │ INPUT  │ │  (AI regen) │ │(code only)│ │(terminal) │
  └───────────────┘           │   └───────────┘ └───┬───┘ └────┬─────┘ └────┬───┘ └───┬────┘ └──────┬──────┘ └─────┬─────┘ └───────────┘
                              │                     │          │           │         │             │              │
                              │                     │          │           │         │             │              │
                              │                     └──────────┴───────────┴─────────┴─────────────┴──────────────┘
                              │                                            │
                              │                                            │ (back to result state based on outcome)
                              │                                            │
                              └────────────────────────────────────────────┘
```

**Simplified Linear View:**

**[REVISED]** Shows VALIDATING as separate from REGENERATING.

```
┌─────────────┐      ┌─────────────────────────────────────────────────────┐
│  ANALYZING  │─────►│ RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED  │
│             │      └────────────────────────┬────────────────────────────┘
└──────┬──────┘                               │
       │                                      │  ┌─────────────────────────┐
       │                              ┌───────┼──┤  HINT_INPUT             │
       ▼                              │       │  │  SCHEMA_INPUT           │
┌─────────────┐                       │       │  │  TESTING                │
│ANALYSIS_ERR │                       │       │  │  EDITING ───────────────┼──► VALIDATING
└──────┬──────┘                       │       │  └─────────────────────────┘        │
       │                              │       │               │                     │
       ▼                              ▼       ▼               ▼                     │
┌─────────────┐                ┌─────────────────────────────────────┐              │
│   CLOSED    │                │  REGENERATING (AI + validation)     │◄─────────────┘
└─────────────┘                └────────────────┬────────────────────┘   (if validation fails,
                                                │                         user may choose to
                                                ▼                         regenerate)
                               ┌─────────────────────────────────────┐
                               │  APPROVED / CANCELED                │
                               └─────────────────────────────────────┘
```

---

#### State Definitions

**[REVISED]** Added VALIDATING state. Clarified EDITING and TESTING exit behaviors.

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| **ANALYZING** | Wizard invoked with sample file | Analysis completes or fails | Spinner shown. AI detects structure, infers types, generates parser code. Runs initial validation against sample rows. User cannot interact except Esc to cancel. |
| **RESULT_VALIDATED** | Analysis succeeded, validation 100% pass | User action (Enter/t/r/e/h/s/Esc) | Shows detected structure, generated parser, validation section with green checkmarks. All sample rows parsed successfully. |
| **RESULT_WARNING** | Analysis succeeded, validation has warnings | User action (Enter/t/r/e/h/s/Esc) | Same as RESULT_VALIDATED but validation section shows amber warnings (e.g., null values, type coercions). User can still approve. |
| **RESULT_FAILED** | Analysis succeeded, but validation has failures | User action (t/r/e/h/s/Esc) | Same as RESULT_WARNING but validation shows red errors (parse failures, schema violations). Enter is disabled; must fix before approval. |
| **ANALYSIS_ERROR** | AI/algorithm failed to generate parser | User action (r/Esc) | Shows error message (timeout, model unavailable, file format unsupported). User can retry (r) or cancel (Esc). |
| **HINT_INPUT** | User pressed 'h' from any result state | Enter submits, Esc cancels | Text input field active. User types hint (e.g., "Column 3 is DD/MM/YYYY date"). Examples shown. |
| **SCHEMA_INPUT** | User pressed 's' from any result state | Enter submits, Esc cancels | Schema editor overlay. User can modify inferred types, mark columns as required/optional, add constraints. |
| **TESTING** | User pressed 't' from any result state | Testing completes or Esc | File picker shown. User selects additional files. Parser runs against them. **[REVISED]** Results determine new result state (VALIDATED/WARNING/FAILED). |
| **EDITING** | User pressed 'e' from any result state | User saves and closes $EDITOR | Draft opened in $EDITOR. TUI shows waiting message. **[REVISED]** Transitions to VALIDATING on return (not REGENERATING). |
| **VALIDATING** | **[REVISED]** Editor closes with modifications | Validation completes | Spinner shown. Runs validation against user's edited code (NO AI regeneration). Preserves user edits. Returns to result state based on outcome. |
| **REGENERATING** | User submitted hint, set schema, or pressed 'r' | Regeneration completes, fails, or user cancels | Spinner shown. AI regenerates parser with new context. Validation runs. Returns to result state. |
| **APPROVED** | User pressed Enter from RESULT_VALIDATED or RESULT_WARNING | Immediate | Parser committed to Layer 1 (parsers/). Entry created in cf_parsers. Dialog closes. |
| **CANCELED** | User pressed Esc from any non-input state | Immediate | Draft discarded. Dialog closes. |
| **CLOSED** | Analysis error with Esc | Immediate | Dialog closes. No draft created. |

---

#### Transitions

**[REVISED]** Updated EDITING transitions to use VALIDATING. Added explicit TESTING exit transitions.

| From | To | Trigger | Guard |
|------|----|---------| ------|
| (external) | ANALYZING | User invokes wizard | Sample file provided |
| ANALYZING | RESULT_VALIDATED | Analysis + validation completes | All rows pass, no warnings |
| ANALYZING | RESULT_WARNING | Analysis + validation completes | Rows pass but have warnings |
| ANALYZING | RESULT_FAILED | Analysis + validation completes | Some rows fail to parse |
| ANALYZING | ANALYSIS_ERROR | Analysis fails | Timeout, model unavailable, or unsupported format |
| ANALYZING | CANCELED | Esc | - |
| RESULT_VALIDATED | APPROVED | Enter | Parser name valid, version valid |
| RESULT_VALIDATED | HINT_INPUT | h | - |
| RESULT_VALIDATED | SCHEMA_INPUT | s | - |
| RESULT_VALIDATED | TESTING | t | - |
| RESULT_VALIDATED | EDITING | e | $EDITOR available |
| RESULT_VALIDATED | REGENERATING | r | - |
| RESULT_VALIDATED | CANCELED | Esc | - |
| RESULT_WARNING | APPROVED | Enter | Parser name valid, version valid (warnings accepted) |
| RESULT_WARNING | HINT_INPUT | h | - |
| RESULT_WARNING | SCHEMA_INPUT | s | - |
| RESULT_WARNING | TESTING | t | - |
| RESULT_WARNING | EDITING | e | $EDITOR available |
| RESULT_WARNING | REGENERATING | r | - |
| RESULT_WARNING | CANCELED | Esc | - |
| RESULT_FAILED | HINT_INPUT | h | - |
| RESULT_FAILED | SCHEMA_INPUT | s | - |
| RESULT_FAILED | TESTING | t | - |
| RESULT_FAILED | EDITING | e | $EDITOR available |
| RESULT_FAILED | REGENERATING | r | - |
| RESULT_FAILED | CANCELED | Esc | - |
| RESULT_FAILED | (blocked) | Enter | Cannot approve with failures |
| ANALYSIS_ERROR | REGENERATING | r | retry_count < 3 |
| ANALYSIS_ERROR | CLOSED | Esc | - |
| ANALYSIS_ERROR | CLOSED | r | retry_count >= 3 (see footnote 1) |
| HINT_INPUT | REGENERATING | Enter | Hint text non-empty |
| HINT_INPUT | (previous result state) | Esc | - |
| SCHEMA_INPUT | REGENERATING | Enter | Schema modified |
| SCHEMA_INPUT | (previous result state) | Esc | - |
| **[REVISED]** TESTING | RESULT_VALIDATED | Testing completes | All files pass, no warnings (cumulative) |
| **[REVISED]** TESTING | RESULT_WARNING | Testing completes | All files pass, some have warnings (cumulative) |
| **[REVISED]** TESTING | RESULT_FAILED | Testing completes | Any file has parse failures (cumulative) |
| TESTING | (previous result state) | Esc | Cancel file selection |
| **[REVISED]** EDITING | VALIDATING | Editor closes | File modified |
| EDITING | (previous result state) | Editor closes | File unmodified |
| **[REVISED]** VALIDATING | RESULT_VALIDATED | Validation completes | All rows pass, no warnings |
| **[REVISED]** VALIDATING | RESULT_WARNING | Validation completes | Rows pass with warnings |
| **[REVISED]** VALIDATING | RESULT_FAILED | Validation completes | Some rows fail |
| REGENERATING | RESULT_VALIDATED | Regeneration + validation completes | All rows pass |
| REGENERATING | RESULT_WARNING | Regeneration + validation completes | Rows pass with warnings |
| REGENERATING | RESULT_FAILED | Regeneration + validation completes | Some rows fail |
| REGENERATING | ANALYSIS_ERROR | Regeneration fails | - |
| REGENERATING | CANCELED | Esc | - |
| APPROVED | (dialog closes) | - | Commit to Layer 1 |
| CANCELED | (dialog closes) | - | Discard draft |
| CLOSED | (dialog closes) | - | No draft to discard |

**Footnotes:**

1. **[REVISED]** Retry exhaustion: When retry_count >= 3, pressing 'r' transitions to CLOSED with message "Maximum retry attempts reached. Please try again later."

---

#### Keybinding Summary by State

**[REVISED]** Added VALIDATING column. Added footnotes for retry exhaustion and Enter blocking.

| Key | ANALYZING | RESULT_VALIDATED | RESULT_WARNING | RESULT_FAILED | ANALYSIS_ERROR | HINT_INPUT | SCHEMA_INPUT | TESTING | EDITING | VALIDATING | REGENERATING |
|-----|-----------|------------------|----------------|---------------|----------------|------------|--------------|---------|---------|------------|--------------|
| Enter | - | Approve | Approve | (blocked)^2 | - | Submit hint | Submit schema | - | - | - | - |
| Esc | Cancel | Cancel | Cancel | Cancel | Close | Back to result | Back to result | Cancel selection | - | Cancel | Cancel |
| h | - | Open hint | Open hint | Open hint | - | - | - | - | - | - | - |
| s | - | Open schema | Open schema | Open schema | - | - | - | - | - | - | - |
| t | - | Test more | Test more | Test more | - | - | - | - | - | - | - |
| e | - | Open editor | Open editor | Open editor | - | - | - | - | - | - | - |
| r | - | Regenerate | Regenerate | Regenerate | Retry^1 | - | - | - | - | - | - |
| (typing) | - | Edit name/version/topic | Edit name/version/topic | Edit name/version/topic | - | Input text | Edit schema | - | - | - | - |
| Tab | - | Cycle fields^3 | Cycle fields^3 | Cycle fields^3 | - | - | Cycle schema rows | - | - | - | - |

**Footnotes:**

1. **[REVISED]** Retry is limited to 3 attempts. On 4th press, transitions to CLOSED with exhaustion message.
2. **[REVISED]** Enter is blocked when in RESULT_FAILED. UI shows red border with message "Fix errors to approve".
3. Tab cycles through: parser name, parser version, topic fields.

---

#### Data Model (Rust structs)

**[REVISED]** Added ValidatingData. Simplified previous_state storage per M3 feedback.

```rust
/// Parser Lab Wizard state
#[derive(Debug, Clone, PartialEq)]
pub enum ParserLabState {
    Analyzing,
    ResultValidated(ParserLabResultData),
    ResultWarning(ParserLabResultData),
    ResultFailed(ParserLabResultData),
    AnalysisError(AnalysisErrorData),
    HintInput(HintInputData),
    SchemaInput(SchemaInputData),
    Testing(TestingData),
    Editing(EditingData),
    Validating(ValidatingData),  // [REVISED] New state
    Regenerating(RegeneratingData),
    Approved,
    Canceled,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParserLabResultData {
    /// Sample file analyzed
    pub sample_file: PathBuf,
    /// File statistics
    pub file_stats: FileStats,
    /// Detected structure
    pub detected_structure: DetectedStructure,
    /// Generated parser code
    pub generated_code: String,
    /// Validation results
    pub validation: ValidationResult,
    /// User-editable parser metadata
    pub parser_name: String,
    pub parser_version: String,
    pub topic: String,
    /// Number of regeneration attempts
    pub regeneration_count: u32,
    /// User hints accumulated
    pub hints: Vec<String>,
    /// User-defined schema overrides
    pub schema_overrides: Option<Vec<SchemaField>>,
    /// Additional test results from 't' action
    pub additional_tests: Vec<TestFileResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileStats {
    pub row_count: usize,
    pub column_count: usize,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedStructure {
    /// File format (CSV, JSON, Parquet, etc.)
    pub format: String,
    /// Format-specific details
    pub format_details: HashMap<String, String>, // e.g., {"delimiter": ","}
    /// Column headers
    pub headers: Vec<String>,
    /// Inferred types for each column
    pub inferred_types: Vec<InferredType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InferredType {
    pub column_name: String,
    pub arrow_type: String,  // e.g., "Int64", "Date32", "Utf8"
    pub confidence: f32,     // 0.0 to 1.0
    pub sample_values: Vec<String>,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub total_rows: usize,
    pub parsed_rows: usize,
    pub failed_rows: usize,
    pub warnings: Vec<ValidationWarning>,
    pub errors: Vec<ValidationError>,
    pub status: ValidationStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationStatus {
    Passed,      // 100% success, no warnings
    Warning,     // 100% success but has warnings
    Failed,      // Some rows failed to parse
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    pub row_number: Option<usize>,
    pub column_name: String,
    pub message: String,       // e.g., "null values in 'amount' column"
    pub affected_count: usize, // e.g., 2 rows have null
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub row_number: usize,
    pub column_name: Option<String>,
    pub message: String,     // e.g., "ValueError: could not parse '12/31/24' as date"
    pub raw_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    pub column_name: String,
    pub arrow_type: String,
    pub nullable: bool,
    pub constraint: Option<String>, // e.g., "1..100" for integers
}

/// [REVISED] Simplified: Store ValidationStatus + ResultData instead of Box<ParserLabState>
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaInputData {
    /// Current schema fields being edited
    pub fields: Vec<SchemaField>,
    /// Index of currently selected field
    pub selected_index: usize,
    /// Whether editing type, nullable, or constraint
    pub edit_focus: SchemaEditFocus,
    /// [REVISED] Status to return to on cancel
    pub return_to_status: ValidationStatus,
    /// [REVISED] Result data to restore
    pub result_data: ParserLabResultData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaEditFocus {
    FieldList,       // Scrolling through fields
    TypeDropdown,    // Editing type for selected field
    NullableToggle,  // Toggling nullable
    ConstraintInput, // Editing constraint
}

/// [REVISED] Simplified previous_state storage
#[derive(Debug, Clone, PartialEq)]
pub struct TestingData {
    /// Files being tested
    pub test_files: Vec<PathBuf>,
    /// Test results per file
    pub results: Vec<TestFileResult>,
    /// Is file picker open?
    pub selecting_files: bool,
    /// [REVISED] Status to return to on cancel
    pub return_to_status: ValidationStatus,
    /// [REVISED] Result data to restore
    pub result_data: ParserLabResultData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFileResult {
    pub file_path: PathBuf,
    pub rows_tested: usize,
    pub rows_passed: usize,
    pub rows_failed: usize,
    pub warnings: Vec<ValidationWarning>,  // [REVISED] Added warnings
    pub errors: Vec<ValidationError>,
}

/// [REVISED] Simplified previous_state storage
#[derive(Debug, Clone, PartialEq)]
pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    /// [REVISED] Status to return to on Esc
    pub return_to_status: ValidationStatus,
    /// [REVISED] Result data to restore
    pub result_data: ParserLabResultData,
}

/// [REVISED] Simplified previous_state storage
#[derive(Debug, Clone, PartialEq)]
pub struct EditingData {
    pub temp_file_path: PathBuf,
    pub original_content: String,
    /// [REVISED] Status to return to if unmodified
    pub return_to_status: ValidationStatus,
    /// [REVISED] Result data to restore
    pub result_data: ParserLabResultData,
}

/// [REVISED] New state for validation-only (after manual edit)
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatingData {
    /// User's edited parser code
    pub edited_code: String,
    /// Sample file to validate against
    pub sample_file: PathBuf,
    /// Result data (without validation - will be updated)
    pub result_data: ParserLabResultData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegeneratingData {
    /// Accumulated hints
    pub hints: Vec<String>,
    /// Sample file
    pub sample_file: PathBuf,
    /// Schema overrides if set
    pub schema_overrides: Option<Vec<SchemaField>>,
    /// [REVISED] Manual edits are NOT passed here (use VALIDATING instead)
    /// This state is for AI regeneration only
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
    UnsupportedFormat,
    ParseError,
    EmptyFile,
    EmptyResponse,
    RetryExhausted,  // [REVISED] Added for max retry case
}
```

---

### VALIDATING vs REGENERATING Distinction

**[REVISED]** New section clarifying the difference.

| Aspect | VALIDATING | REGENERATING |
|--------|------------|--------------|
| **Trigger** | Editor closes with modifications | Hint submitted, schema changed, or 'r' pressed |
| **AI Involvement** | None | Yes - AI regenerates parser code |
| **Code Source** | User's manually edited code | AI-generated code with new context |
| **Preserves User Edits** | Yes | No - overwrites with AI output |
| **Use Case** | User fixes a bug in generated code | User gives hint to guide AI |
| **Failure Path** | If validation fails, user can edit again or regenerate | If regeneration fails, becomes ANALYSIS_ERROR |

**Rationale:** Users who manually edit code expect their changes to be preserved. They're debugging/fixing, not requesting AI help. VALIDATING respects this by only running validation. If validation fails, user can choose to:
1. Edit again (e -> EDITING -> VALIDATING cycle)
2. Give up on manual fix and regenerate (r -> REGENERATING)

---

### Examples

**Example 1: Happy path - Parser approved with validation passed**

```
1. User selects file group containing sales_2024.csv
2. User presses 'g' (invoke Parser Lab)
3. State: ANALYZING
   - Spinner: "Analyzing file structure..."
   - Reads first 100 rows
   - AI generates parser code
   - Validation runs against sample
4. Validation 100% success
5. State: RESULT_VALIDATED
   - Detected Structure: CSV, headers [id, date, amount, email]
   - Generated Parser shown
   - Validation: "100/100 sample rows parsed successfully"
6. User edits name to "sales_parser", version "1.0.0", topic "sales_data"
7. User presses Enter
8. State: APPROVED
   - Parser written to ~/.casparian_flow/parsers/sales_parser.py
   - Entry created in cf_parsers table
   - Dialog closes
```

**Example 2: Validation warnings - User approves anyway**

```
1. User invokes Parser Lab on customer_data.csv
2. State: ANALYZING -> RESULT_WARNING
   - Validation: "100/100 rows parsed"
   - Warning: "2 rows have null values in 'amount' column"
3. User reviews warning, decides null amounts are acceptable
4. User presses Enter
5. State: APPROVED
   - Parser committed with nullable amount column
```

**Example 3: Validation failed - User provides hint**

```
1. User invokes Parser Lab on european_dates.csv
2. State: ANALYZING -> RESULT_FAILED
   - Validation: "85/100 rows parsed, 15 failed"
   - Error: "Row 12: ValueError: '31/01/24' - day is out of range for month"
   - AI assumed MM/DD/YYYY, but data is DD/MM/YYYY
3. Enter key is blocked (red border, "Fix errors to approve")
4. User presses 'h'
5. State: HINT_INPUT
   - User types: "Date column is DD/MM/YYYY format"
6. User presses Enter
7. State: REGENERATING
   - AI regenerates with European date format
   - Validation re-runs
8. State: RESULT_VALIDATED
   - "100/100 rows parsed successfully"
9. User approves
```

**Example 4: Set schema for strict typing**

```
1. User has RESULT_WARNING (amount inferred as Float64 but has "$" prefix)
2. User presses 's'
3. State: SCHEMA_INPUT
   - Shows table:
     | Column  | Type    | Nullable | Constraint |
     | id      | Int64   | false    | > 0        |
     | date    | Date32  | false    |            |
     | amount  | Float64 | true     |            |  <- user edits
     | email   | Utf8    | false    |            |
   - User changes 'amount' type to "String" (to preserve $)
   - Or adds transform note: "Strip $ prefix, parse as Float64"
4. User presses Enter
5. State: REGENERATING
   - AI regenerates parser with explicit amount handling
6. State: RESULT_VALIDATED
```

**Example 5: Test against additional files**

**[REVISED]** Clarified state transitions based on cumulative test results.

```
1. User has RESULT_VALIDATED on sales_jan.csv
2. User presses 't' to test on more files
3. State: TESTING
   - File picker opens
   - User selects sales_feb.csv, sales_mar.csv
4. Parser runs on additional files
5. Results shown inline:
   - Original: sales_jan.csv: 100/100 passed
   - sales_feb.csv: 150/150 passed
   - sales_mar.csv: 148/150 passed, 2 failed
6. State: RESULT_FAILED (cumulative: 2 failures across all files)
   - Validation section now shows combined results
   - Errors from sales_mar.csv highlighted
7. User must fix before approving (Enter blocked)
8. User presses 'h' to give hint, or 'e' to edit
```

**Example 6: Manual edit to fix generated code**

**[REVISED]** Now shows VALIDATING flow instead of REGENERATING.

```
1. User has RESULT_FAILED
   - AI generated code has bug in date parsing
2. User presses 'e'
3. State: EDITING
   - Opens /tmp/casparian_draft_xyz789.py in $EDITOR
   - User fixes the datetime parsing logic
   - User saves and closes editor
4. State: VALIDATING  [REVISED: was REGENERATING]
   - Spinner: "Validating edited parser..."
   - Runs validation on user's edited code
   - NO AI regeneration - user's code preserved exactly
5. State: RESULT_VALIDATED (if fix worked)
   - Or RESULT_FAILED (if still broken - user can try again)
6. If RESULT_FAILED, user can:
   - Press 'e' again to edit more
   - Press 'r' to give up and let AI regenerate
```

**Example 7: Retry exhaustion**

**[REVISED]** New example demonstrating retry limit behavior.

```
1. User invokes Parser Lab on corrupted_file.csv
2. State: ANALYZING -> ANALYSIS_ERROR
   - Error: "Timeout: AI analysis took too long"
3. User presses 'r' (retry 1)
4. State: REGENERATING -> ANALYSIS_ERROR
5. User presses 'r' (retry 2)
6. State: REGENERATING -> ANALYSIS_ERROR
7. User presses 'r' (retry 3)
8. State: REGENERATING -> ANALYSIS_ERROR
9. User presses 'r' (retry 4)
10. State: CLOSED
    - Message: "Maximum retry attempts (3) reached. Please try again later."
    - Dialog closes
```

---

### Trade-offs

**Pros:**

1. **Tri-state validation** - VALIDATED/WARNING/FAILED gives clear signals about data quality
2. **Enter-blocking on failure** - Prevents committing broken parsers; forces user to address issues
3. **Schema override capability** - User can enforce strict types without regenerating
4. **Test-more workflow** - Validates parser generalizes beyond single sample file
5. **Accumulated context** - Hints, schema, and test results carry through regeneration cycles
6. **Consistent escape paths** - Esc always works; same pattern as Pathfinder
7. **[REVISED] VALIDATING preserves edits** - Manual edits are never overwritten by AI

**Cons:**

1. **Three result states** - More complex than Pathfinder's two (YAML/Python); could use single state with validation_status field
2. **Schema editor complexity** - Full schema editor in TUI is ambitious; may need simplification
3. **Test state file selection** - File picker in TUI needs design work
4. **[REVISED] Two "processing" states** - VALIDATING and REGENERATING may confuse users

**Mitigations:**

1. Use `ParserLabResultData` struct with `ValidationStatus` enum; state name reflects status for UI styling
2. Phase 1: Simple schema editor (type dropdown per column). Phase 2: Full constraints
3. Use glob patterns or directory selection for test files rather than individual file picker
4. **[REVISED]** Clear UI messaging: VALIDATING shows "Validating your code..." vs REGENERATING shows "Regenerating with AI..."

---

### Schema Input Sub-State Machine

The SCHEMA_INPUT state has its own internal navigation:

```
┌────────────────────────────────────────────────────────────────────────┐
│                           SCHEMA_INPUT                                  │
│                                                                        │
│  ┌──────────────┐     Tab     ┌──────────────┐     Tab    ┌──────────┐│
│  │  FIELD_LIST  │◄───────────►│ TYPE_DROPDOWN│◄──────────►│ NULLABLE ││
│  │   (j/k nav)  │             │  (select)    │            │ (toggle) ││
│  └──────────────┘             └──────────────┘            └──────────┘│
│                                                                        │
│  Enter: Submit schema changes                                          │
│  Esc: Cancel, return to result state                                   │
└────────────────────────────────────────────────────────────────────────┘
```

---

### Testing Sub-State Machine

**[REVISED]** Clarified exit transitions based on cumulative results.

The TESTING state has file selection and result display:

```
┌────────────────────────────────────────────────────────────────────────┐
│                             TESTING                                     │
│                                                                        │
│  ┌──────────────┐    Enter    ┌──────────────┐  Completes  ┌─────────┐│
│  │FILE_SELECTION│────────────►│   RUNNING    │────────────►│ RESULTS ││
│  │  (pick files)│             │  (spinner)   │             │ (view)  ││
│  └──────────────┘             └──────────────┘             └─────────┘│
│         │                                                       │      │
│         │ Esc                                              Enter │      │
│         ▼                                                       ▼      │
│  (return to previous         ┌──────────────────────────────────┐     │
│   result state)              │  Compute cumulative result:       │     │
│                              │  - Any failures? -> RESULT_FAILED │     │
│                              │  - Any warnings? -> RESULT_WARNING│     │
│                              │  - All pass?     -> RESULT_VALID  │     │
│                              └──────────────────────────────────┘     │
└────────────────────────────────────────────────────────────────────────┘
```

**Cumulative Result Logic:**

```rust
fn compute_cumulative_status(
    original: &ValidationResult,
    additional_tests: &[TestFileResult],
) -> ValidationStatus {
    // Any parse failure -> FAILED
    if original.failed_rows > 0
       || additional_tests.iter().any(|t| t.rows_failed > 0) {
        return ValidationStatus::Failed;
    }

    // Any warning -> WARNING
    if !original.warnings.is_empty()
       || additional_tests.iter().any(|t| !t.warnings.is_empty()) {
        return ValidationStatus::Warning;
    }

    // All pass, no warnings -> PASSED
    ValidationStatus::Passed
}
```

---

### New Gaps Introduced

1. **GAP-SCHEMA-001**: Schema Input UI not fully specified:
   - What type options are available in dropdown? (Int64, Float64, Utf8, Date32, etc.)
   - How to specify custom date formats?
   - Constraint syntax (e.g., "1..100", "matches /regex/")?

2. **GAP-TEST-001**: Testing file selection mechanism:
   - How does user select multiple files in TUI?
   - Directory selection with glob?
   - Max files to test at once?

3. **[REVISED] GAP-EDIT-002 RESOLVED**: Validation-only after edit vs regeneration:
   - RESOLVED: After edit, run VALIDATING (validation only, no regeneration)
   - User edits are preserved
   - If validation fails, user can edit again or choose to regenerate with 'r'

4. **GAP-VERSION-001**: Parser version handling:
   - What if parser with same name exists?
   - Auto-increment version suggestion?
   - Conflict detection (same name+version, different code)?

---

### Validation Checklist

- [x] Diagram included with all states
- [x] Entry/exit conditions documented for each state
- [x] All keybindings from Section 5.2 appear in transition table: [Enter, t, r, e, h, s, Esc]
- [x] Esc behavior is consistent (cancel/back/close)
- [x] No orphan states (all reachable from ANALYZING, all can exit)
- [x] Validation states clearly differentiate pass/warn/fail
- [x] Enter blocked on RESULT_FAILED to prevent broken parser commits
- [x] Data model includes parser metadata (name, version, topic)
- [x] **[REVISED]** EDITING -> VALIDATING (not REGENERATING) to preserve user edits
- [x] **[REVISED]** TESTING exit transitions explicitly map to result states
- [x] **[REVISED]** Retry exhaustion documented with max=3
- [x] **[REVISED]** VALIDATING vs REGENERATING distinction documented
- [x] **[REVISED]** Simplified data model (no recursive Box<ParserLabState>)

---

### Summary of Revisions (v1 -> v2)

| Issue | Resolution |
|-------|------------|
| **H1: EDITING -> REGENERATING contradiction** | Added VALIDATING state. EDITING -> VALIDATING (preserves edits). REGENERATING is only for AI-driven changes. |
| **H2: TESTING exit underspecified** | Added explicit transitions: TESTING -> RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED based on cumulative test results. |
| **M1: Retry exhaustion in keybindings** | Added footnote 1 to keybinding table. Added Example 7 demonstrating behavior. |
| **M2: Missing VALIDATING state** | Added VALIDATING state with full documentation. |
| **M3: Recursive Box types** | Simplified to `return_to_status: ValidationStatus` + `result_data: ParserLabResultData`. |
| **L4: Enter blocked explanation** | Added footnote 2 to keybinding table describing UX. |

---

### References

- `specs/ai_wizards.md` Section 3.2 (Parser Lab Wizard)
- `specs/ai_wizards.md` Section 5.2 (TUI Dialog mockup)
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine - used as template)
- `CLAUDE.md` ADR-012 (Parser Versioning)

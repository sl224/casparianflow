# Engineer Resolution: GAP-EX-002

## Manual Edit Mode Error Handling Specification

**Gap:** GAP-EX-002 - Manual edit mode has no error handling
- Priority: MEDIUM
- When users manually edit generated YAML/Python in external editors, syntax and validation errors need handling
- Current spec describes EDITING and VALIDATING states but lacks:
  - Error type detection and classification
  - UI feedback patterns for different error categories
  - Validation timing (on-type vs on-save)
  - Recovery options and retry workflows
  - State machine completeness for error flows

**Confidence:** HIGH

**References:**
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine)
- `specs/ai_wizards.md` Section 5.2.1 (Parser Lab state machine)
- `specs/ai_wizards.md` Section 5.1 / 5.2 (TUI dialogs with edit action)
- `specs/ai_wizards.md` Section 4.2 (Validation stages for Python extraction)
- Round 6 (GAP-TUI-001): $EDITOR subprocess handling

---

## 1. Error Categories and Detection

### 1.1 Pathfinder Wizard (YAML Editing)

**YAML Syntax Errors:**

| Error Type | Detection | Severity | Example |
|-----------|-----------|----------|---------|
| Invalid YAML syntax | YAML parser (PyYAML or yaml-rust) | CRITICAL | `name: [unclosed list` |
| Duplicate keys | YAML parser | CRITICAL | Two `extract:` sections at same level |
| Invalid indentation | YAML parser | CRITICAL | Inconsistent spacing (tabs vs spaces) |
| Invalid type for field | Schema validation | HIGH | `type: date_invalid` (not a known type) |
| Missing required field | Schema validation | HIGH | `extract:` present but missing `name:` field |
| Invalid extraction construct | Schema validation | HIGH | `from: invalid_function()` (not recognized function) |
| Circular reference | Schema validation | MEDIUM | `pattern:` references undefined variable |

**Detection Method:**
```
1. YAML Parse Phase:
   └─ yaml.load() - returns parse errors with line/column

2. Schema Validation Phase:
   └─ Validate against extraction rule schema:
      - Required fields present?
      - Field types correct?
      - Values within allowed ranges?
      - Extract constructs recognized?

3. Semantic Validation Phase:
   └─ Check correctness of rule intent:
      - Does glob pattern match sample paths?
      - Do extract patterns produce expected fields?
      - Are field types consistent?
```

### 1.2 Parser Lab (Python Editing)

**Python Syntax Errors:**

| Error Type | Detection | Severity | Example |
|-----------|-----------|----------|---------|
| Syntax error | AST parser (ast.parse) | CRITICAL | `def parse( self` (missing colon) |
| Indentation error | AST parser | CRITICAL | Inconsistent function body indentation |
| Undefined name | AST parser | MEDIUM | Reference to undefined variable |
| Import statement | AST parser validation | MEDIUM | `import os` (disallowed for safety) |
| Missing required method | AST parser + inspection | HIGH | `parse()` method not found or wrong signature |
| Type annotation mismatch | Type checker (mypy-lite) | MEDIUM | Return type doesn't match declared schema |

**Python Runtime Errors (from test execution):**

| Error Type | Detection | Severity | Example |
|-----------|-----------|----------|---------|
| Exception on sample | Sandbox execution | HIGH | `ValueError: invalid literal for int()` |
| Timeout | Sandbox execution | HIGH | Infinite loop or very slow operation |
| Memory exhaustion | Sandbox execution | CRITICAL | Out of memory during parsing |
| Wrong output type | Output validation | HIGH | Returns list instead of dict |
| Missing required field in output | Schema validation | HIGH | Returns `{id: 1}` but needs `{id, amount}` |
| Type mismatch in output | Type validation | HIGH | Returns `id: "123"` but schema says int |

**Detection Method:**
```
1. Syntax Parse Phase:
   └─ ast.parse() - returns SyntaxError with line/column

2. Semantic Analysis Phase:
   └─ Check AST for:
      - Required method exists (parse/transform)
      - Method signature correct
      - No disallowed imports (os, subprocess, etc.)
      - No suspicious patterns (exec, eval, __import__)

3. Sandbox Test Phase (sample data):
   └─ Execute against sample file:
      - Measure execution time (timeout after 30s)
      - Monitor memory usage
      - Catch exceptions with traceback
      - Validate output schema against declared types
      - Check for required fields in returned dict
```

### 1.3 Semantic Path Wizard (YAML Editing)

**Errors identical to Pathfinder YAML** (since output is YAML extraction rule)

---

## 2. Validation Timing Strategy

### 2.1 When to Validate

**Parser Lab (Python):** Three-stage validation

```
Stage 1: On Editor Close (EDITING → VALIDATING)
├─ Syntax check: ast.parse()
├─ Required method signature validation
├─ Disallowed import detection
└─ Takes <100ms
   Result: SYNTAX_ERROR or SEMANTIC_ERROR or PASS

Stage 2: On Sample Test (VALIDATING → RESULT_*)
├─ Execute against sample file (first 10 rows)
├─ Validate output schema
├─ Catch runtime exceptions
└─ Takes 500-2000ms (depends on file size)
   Result: RUNTIME_ERROR or TYPE_ERROR or PASS

Stage 3: On Full Test (if user presses [t])
├─ Execute against all test files
├─ Aggregate results
└─ Takes 1-10s (depends on test file count)
   Result: RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED
```

**Pathfinder (YAML):** Two-stage validation

```
Stage 1: On Editor Close (EDITING → VALIDATING)
├─ YAML syntax check
├─ Schema validation
├─ Glob pattern validation (valid syntax, not empty)
└─ Takes <50ms
   Result: YAML_ERROR or SCHEMA_ERROR or PASS

Stage 2: On Preview Test (VALIDATING → RESULT_*)
├─ Apply rule to sample paths
├─ Verify extracted fields match expected schema
├─ Check no unexpected exceptions
└─ Takes <100ms
   Result: VALIDATION_PASSED or VALIDATION_FAILED
```

**Rationale:**
- We do NOT validate-on-type (would require re-parsing after every keystroke)
- We DO validate-on-save (when editor closes)
- We DO re-test when user approves
- This gives quick feedback without performance penalty

### 2.2 Incremental Feedback

**During validation states, show progress:**

```
┌─ PARSER LAB (VALIDATING) ────────────────────────┐
│                                                   │
│  Validating edited parser...                      │
│                                                   │
│  [⠋] Syntax check                                │
│  [ ] Sample test                                 │
│  [ ] Schema validation                           │
│                                                   │
│  Press [Esc] to cancel                           │
└───────────────────────────────────────────────────┘
```

This prevents the "frozen UI" feeling while validation runs.

---

## 3. Error Detection and Reporting

### 3.1 YAML Error Messages (Pathfinder/Semantic)

**For YAML Syntax Errors:**

```
Error in YAML syntax:
  Line 5, column 3: expected "<block end>"

  │  extract:
  │    direction:
→ │      from: [unclosed list

  Fix: Add closing bracket ]
```

**For Schema Validation Errors:**

```
Schema validation failed:
  - Missing required field: "name"
    Expected: Extract rule must have name field

  - Invalid type in field "extract.year.type": "date_invalid"
    Valid values: date, date_iso, integer, string, float, email

  - Unknown extraction function: "segment_invalid(-1)"
    Valid functions: segment(i), prefix(s), suffix(s),
                     matches(regex), range(start, end)
```

**For Rule Logic Errors:**

```
Rule validation failed:
  - Glob pattern "**/x" matches 0 of 5 sample paths
    Check glob syntax or adjust pattern

  - Field "year" extraction produces invalid dates:
    Sample: /data/2024-13/file.csv → year=2024
    But month 13 is invalid (expected 1-12)
```

### 3.2 Python Error Messages (Parser Lab)

**For Syntax Errors:**

```
Syntax Error in Python code:
  Line 8, column 12: invalid syntax

  │  def parse(self:
→ │              ^

  Fix: Add missing colon or check parentheses
```

**For Semantic Errors:**

```
Python semantic errors found:
  - Missing required method: "parse(self, ctx)"
    Must define: def parse(self, ctx) -> Iterator[tuple[str, Any]]

  - Disallowed import detected: "import subprocess"
    Imports blocked for security: os, subprocess, sys, socket

  - Undefined variable: "pd" (did you mean "pandas"?)
```

**For Runtime Errors (from sandbox test):**

```
Parser failed on sample file:
  Exception: ValueError on row 2
    File parsing failed: invalid literal for int(): "abc"

  Traceback:
    Line 15: int(row['amount'])

  Suggestion: Check if 'amount' column contains valid numbers

  [r] Regenerate with hint
  [e] Edit and retry
  [Esc] Cancel
```

**For Type Mismatches (output validation):**

```
Output schema mismatch:
  Expected: {id: int64, date: date, amount: float64}
  Got:      {id: string, date: string}

  Issues:
    - 'id' has type string but schema says int64
    - 'date' missing required type (expected date)
    - 'amount' field is missing from output
```

---

## 4. UI Feedback Patterns

### 4.1 Error Toast Messages (Pathfinder YAML)

**Critical Errors (stay visible 5s, red background):**

```
┌─────────────────────────────────────────────────┐
│ ✗ YAML syntax error on line 5                  │
│   Press [e] to edit, [r] to regenerate, [Esc] │
└─────────────────────────────────────────────────┘
```

**High Severity Errors (yellow background, 4s):**

```
┌─────────────────────────────────────────────────┐
│ ⚠ Schema validation failed (3 issues)          │
│   Press [e] to edit, [r] to regenerate, [Esc] │
└─────────────────────────────────────────────────┘
```

### 4.2 Error Detail Dialog (Pathfinder YAML)

**Full error view (user presses [e] to see details):**

```
┌─ YAML VALIDATION ERRORS ────────────────────────┐
│                                                  │
│ Error 1: Syntax Error (line 5)                   │
│   Message: expected "<block end>"                │
│   Location: extract section                      │
│   Context:                                       │
│   5 │      from: [unclosed                      │
│     │              ^                             │
│                                                  │
│ Error 2: Schema Error                            │
│   Message: field "type" has invalid value        │
│   Allowed: date, date_iso, integer, string       │
│   Got: date_invalid                              │
│                                                  │
│ Error 3: Logic Error                             │
│   Message: glob pattern matches 0 files          │
│   Pattern: "**/x"                                │
│   Sample files: /data/2024/file.csv              │
│   Suggestion: Adjust pattern or glob syntax      │
│                                                  │
│  [e] Edit and retry   [r] Regenerate   [Esc]   │
└──────────────────────────────────────────────────┘
```

### 4.3 Error Detail Dialog (Parser Lab Python)

```
┌─ PYTHON VALIDATION ERRORS ──────────────────────┐
│                                                  │
│ Error 1: Syntax Error (line 8)                   │
│   Message: invalid syntax                        │
│   Location: method definition                    │
│   Code:                                          │
│   8 │  def parse(self:                           │
│     │                ^                           │
│   Fix: Add missing colon or check syntax         │
│                                                  │
│ Error 2: Runtime Error (sample test)             │
│   Message: ValueError on row 2                   │
│   Exception: invalid literal for int(): "abc"    │
│   Traceback:                                     │
│     Line 15: int(row['amount'])                  │
│   Sample file: sales_2024.csv                    │
│   Affected row: {id: 1, amount: "abc", ...}     │
│                                                  │
│ Severity: The parser cannot process sample      │
│ data. You must fix this before approval.         │
│                                                  │
│  [e] Edit and retry   [t] Test with more files  │
│  [r] Regenerate      [Esc] Discard              │
└──────────────────────────────────────────────────┘
```

### 4.4 Inline Error Highlighting

While user is editing, show errors in-editor context if using TUI editor. If using external editor:

```
┌─ EDITOR: /tmp/draft_extractor_abc123.py ────────┐
│                                                  │
│ Error Summary (after editor close):              │
│   3 syntax errors found                          │
│   Line 8: expected colon                         │
│   Line 15: undefined variable 'pd'               │
│   Line 22: invalid indentation                   │
│                                                  │
│ Press [e] to open editor again                  │
│           [r] to regenerate from scratch         │
│           [Esc] to cancel                        │
└──────────────────────────────────────────────────┘
```

---

## 5. State Machine Updates

### 5.1 Pathfinder Wizard: Enhanced State Machine

```
┌─────────────┐
│  ANALYZING  │ (unchanged)
└──────┬──────┘
       │
       ▼
┌─────────────────┐
│YAML_RESULT /    │ (unchanged)
│PYTHON_RESULT    │
└──────┬──────────┘
       │ [e] Edit
       ▼
┌──────────────────────────────┐
│      EDITING                 │ NEW: Open editor
│  (external $EDITOR)          │
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│    VALIDATING                │ NEW: Validate YAML/Python
│  (syntax + schema)           │      after edit closes
└──────┬──────┬────────────────┘
       │      │
    ✓  │      │ ✗ (errors found)
       │      │
       ▼      ▼
 APPROVED  VALIDATION_ERROR  NEW: State for recoverable errors
           (toast + options)

VALIDATION_ERROR transitions:
  [e] → EDITING (re-open editor)
  [r] → ANALYZING (regenerate fresh)
  [Esc] → (previous state, discard edits)
```

### 5.2 Parser Lab: Enhanced State Machine

```
┌─────────────────┐
│  ANALYZING      │ (unchanged)
│  RESULT_* states│
└──────┬──────────┘
       │ [e] Edit
       ▼
┌──────────────────────────────┐
│      EDITING                 │ NEW: Open editor
│  (external $EDITOR)          │      for Python code
└──────────┬───────────────────┘
           │ (editor closes)
           ▼
┌──────────────────────────────┐
│    VALIDATING                │ NEW: Stage 1 + Stage 2
│  (syntax + sample test)      │      validation pipeline
└──────┬──────┬────────────────┘
       │      │
    ✓  │      │ ✗ (errors found)
       │      │
       ▼      ▼
 RESULT_* VALIDATION_ERROR NEW: Errors from edit

VALIDATION_ERROR transitions:
  [e] → EDITING (re-open editor)
  [r] → ANALYZING (regenerate fresh)
  [t] → TESTING (test on more files)
  [Esc] → CANCELED (discard)
```

### 5.3 Semantic Path Wizard: Enhanced State Machine

```
┌─────────────┐
│ RECOGNIZING │ (unchanged)
└──────┬──────┘
       │
       ▼
┌─────────────────┐
│  RESULT_*       │ (unchanged)
│  (high/low conf)│
└──────┬──────────┘
       │ [e] Edit YAML
       ▼
┌──────────────────────────────┐
│      EDITING                 │ NEW: Open editor
│  (external $EDITOR)          │      for rule YAML
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│    VALIDATING                │ NEW: YAML syntax +
│  (syntax + schema)           │      schema validation
└──────┬──────┬────────────────┘
       │      │
    ✓  │      │ ✗ (errors found)
       │      │
       ▼      ▼
 RESULT_* VALIDATION_ERROR NEW: Errors from edit

VALIDATION_ERROR transitions:
  [e] → EDITING (re-open editor)
  [r] → RECOGNIZING (re-analyze)
  [Esc] → CANCELED (discard)
```

---

## 6. Recovery Options and Retry Workflows

### 6.1 Three-Level Recovery

**Level 1: Quick Edit (Preferred)**
```
Error detected → Show error toast
   ↓
User presses [e]
   ↓
Re-open editor at same location
   ↓
User fixes syntax/schema issue
   ↓
Validate again → Success or back to error
```

**Level 2: Regenerate (AI Help)**
```
Error detected → Show error toast
   ↓
User presses [r]
   ↓
Clear temp file, return to ANALYZING/RECOGNIZING
   ↓
LLM regenerates without manual edits
   ↓
Show new result
```

**Level 3: Discard (Start Over)**
```
Error detected → Show error toast
   ↓
User presses [Esc]
   ↓
Discard all edits, return to previous RESULT state
   ↓
Original generated rule restored
```

### 6.2 Retry Limits

| Scenario | Limit | Behavior |
|----------|-------|----------|
| Edit-validate cycles | Unlimited | User can edit, validate, edit again indefinitely |
| Regenerate after error | 3 max | After 3 regenerate attempts with errors, suggest starting fresh |
| Sandbox timeout | 30s per test | Kill process if exceeds 30 seconds |
| Memory limit | 512MB | Kill sandbox if exceeds memory |

### 6.3 Error Context Preservation

When user hits [e] to edit after validation error:

**Parser Lab Python:**
```
- Line numbers in error message reference editor line numbers
- Temp file preserved with original edits intact
- Error highlights preserved if re-opening same editor
- Traceback shown with exact sample data that failed
```

**Pathfinder/Semantic YAML:**
```
- Error line/column numbers reference YAML line numbers
- Temp file preserved with user's edits intact
- Schema errors list what's expected vs what was provided
- Logic errors show sample paths that didn't match
```

---

## 7. State Transitions Table (Complete)

### 7.1 Pathfinder Wizard: Complete Transitions

| From | To | Trigger | Guard | Action |
|------|----|---------| ------|--------|
| YAML_RESULT | EDITING | e pressed | $EDITOR available | Open temp YAML file |
| PYTHON_RESULT | EDITING | e pressed | $EDITOR available | Open temp Python file |
| EDITING | VALIDATING | Editor closes, exit=0 | File modified | Load edited content, validate |
| VALIDATING | YAML_RESULT | Validation pass | - | Return to result view with original data |
| VALIDATING | VALIDATION_ERROR | YAML syntax error | - | Show error toast + detail dialog |
| VALIDATING | VALIDATION_ERROR | Schema error | - | Show error toast + detail dialog |
| VALIDATING | VALIDATION_ERROR | Logic error | - | Show error toast + detail dialog |
| VALIDATION_ERROR | EDITING | e pressed | $EDITOR available | Re-open editor with error context |
| VALIDATION_ERROR | ANALYZING | r pressed | - | Regenerate from scratch |
| VALIDATION_ERROR | (previous state) | Esc pressed | - | Discard edits, restore original |

### 7.2 Parser Lab: Complete Transitions

| From | To | Trigger | Guard | Action |
|------|----|---------| ------|--------|
| RESULT_* | EDITING | e pressed | $EDITOR available | Open temp Python file |
| EDITING | VALIDATING | Editor closes, exit=0 | File modified | Run Stage 1+2 validation |
| VALIDATING | RESULT_VALIDATED | All tests pass | - | Move to VALIDATED state |
| VALIDATING | RESULT_WARNING | Some rows warn, all pass | - | Move to WARNING state |
| VALIDATING | RESULT_FAILED | Some rows fail | - | Move to FAILED state |
| VALIDATING | VALIDATION_ERROR | Syntax error | - | Show error, syntax context |
| VALIDATING | VALIDATION_ERROR | Semantic error | - | Show error, missing method/import |
| VALIDATING | VALIDATION_ERROR | Runtime error on sample | - | Show error, exception traceback |
| VALIDATING | VALIDATION_ERROR | Type mismatch | - | Show error, schema vs actual |
| VALIDATION_ERROR | EDITING | e pressed | - | Re-open editor |
| VALIDATION_ERROR | ANALYZING | r pressed | - | Regenerate from scratch |
| VALIDATION_ERROR | TESTING | t pressed | - | Run test on more files (preview, no full approval) |
| VALIDATION_ERROR | CANCELED | Esc pressed | - | Discard all edits |

### 7.3 Semantic Path Wizard: Complete Transitions

| From | To | Trigger | Guard | Action |
|------|----|---------| ------|--------|
| RESULT_* | EDITING | e pressed | $EDITOR available | Open temp YAML file |
| EDITING | VALIDATING | Editor closes, exit=0 | File modified | Validate YAML syntax + schema |
| VALIDATING | RESULT_HIGH_CONFIDENCE | Validation pass, conf>=80% | - | Return to result view |
| VALIDATING | RESULT_LOW_CONFIDENCE | Validation pass, conf<80% | - | Return to result view |
| VALIDATING | VALIDATION_ERROR | YAML syntax error | - | Show error toast + detail |
| VALIDATING | VALIDATION_ERROR | Schema error | - | Show error toast + detail |
| VALIDATING | VALIDATION_ERROR | Logic error (glob mismatch) | - | Show error toast + detail |
| VALIDATION_ERROR | EDITING | e pressed | - | Re-open editor |
| VALIDATION_ERROR | RECOGNIZING | r pressed | - | Re-analyze structure |
| VALIDATION_ERROR | CANCELED | Esc pressed | - | Discard edits |

---

## 8. Implementation Checklist

### Phase 1: Error Detection Infrastructure

- [ ] Implement YAML parser wrapper for Pathfinder
  - [ ] Parse errors (syntax)
  - [ ] Schema validation (required fields, types)
  - [ ] Logic validation (glob pattern test)

- [ ] Implement Python AST parser wrapper for Parser Lab
  - [ ] Syntax errors (ast.parse)
  - [ ] Semantic analysis (required methods, imports)
  - [ ] Type signature validation

- [ ] Implement sandbox execution wrapper
  - [ ] Timeout handling (30s limit)
  - [ ] Exception catching with traceback
  - [ ] Output schema validation
  - [ ] Memory monitoring

### Phase 2: Error Messages and Formatting

- [ ] Create error type enum (SyntaxError, SchemaError, RuntimeError, etc.)
- [ ] Implement error message formatter with context
  - [ ] Line/column numbers
  - [ ] Code excerpts with pointer
  - [ ] Suggestions for fixes

- [ ] Implement error detail dialog
  - [ ] Multi-error display
  - [ ] Expandable/collapsible sections

### Phase 3: VALIDATING State Implementation

- [ ] Add VALIDATING state to all three wizards
- [ ] Implement transition logic from EDITING to VALIDATING
- [ ] Implement validation pipeline:
  - [ ] Stage 1: Syntax + schema (all wizards, <100ms)
  - [ ] Stage 2: Sample test (Parser Lab only, <2s)

- [ ] Add VALIDATION_ERROR state
- [ ] Implement transitions from VALIDATION_ERROR:
  - [ ] [e] → EDITING (re-open editor)
  - [ ] [r] → Regenerate (clear cache, re-run LLM)
  - [ ] [Esc] → Cancel (discard edits)

### Phase 4: UI Implementation

- [ ] Error toast messages
  - [ ] Severity levels (critical, high, medium)
  - [ ] Duration (5s, 4s, 3s respectively)

- [ ] Error detail dialog
  - [ ] Multi-error list
  - [ ] Error context with code excerpts
  - [ ] Recovery options as keybindings

- [ ] Inline editor hints (if TUI editor)
  - [ ] Line number mapping
  - [ ] Error indicators

### Phase 5: Testing

- [ ] E2E test: YAML syntax error → edit → fix → validate pass
- [ ] E2E test: Python syntax error → edit → fix → validate pass
- [ ] E2E test: Runtime error → edit → fix → test → pass
- [ ] E2E test: Schema error → regenerate → pass
- [ ] E2E test: Error → discard → original restored
- [ ] E2E test: Retry limits (regenerate after 3 errors)

### Phase 6: Documentation

- [ ] Update `specs/ai_wizards.md` Section 5.1.1 (state machine diagrams)
- [ ] Update `specs/ai_wizards.md` Section 5.2.1 (state machine diagrams)
- [ ] Update `specs/ai_wizards.md` Section 5.5 (Semantic wizard state machine)
- [ ] Add new section to error handling:
  - [ ] Error types
  - [ ] Validation timing
  - [ ] Recovery workflows

---

## 9. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Validation timing | On-save, not on-type | Avoids performance penalty, syntax errors per keystroke |
| Validate after edit closes | Yes | Gives user immediate feedback, before approval |
| Create VALIDATING state | Yes | Clear state for validation-in-progress feedback |
| Create VALIDATION_ERROR state | Yes | Separate from other error states, recoverable |
| Three recovery options | [e] [r] [Esc] | Edit again / regenerate / discard covers most user needs |
| Unlimited edit cycles | Yes | User may need multiple edits to fix complex issues |
| 30s sandbox timeout | Yes | Reasonable for typical file (prevents infinite loops) |
| Show detailed error context | Yes | Helps users understand why validation failed |
| Preserve original on discard | Yes | User can recover if edit goes badly wrong |
| Inline editing (Labeling only) | Not external | YAML/Python edits are complex, need real editor |

---

## 10. Error Message Examples

### Example 1: YAML Syntax Error (Pathfinder)

```
User edits YAML rule, forgets closing bracket:

Extract section:
  year:
    from: segment(-3)
    pattern: [0-9]{4

[Esc] from editor → VALIDATING starts

VALIDATING finds: YAML parse error on line 7
  Message: "expected ',' or ']' before end of document"
  Location: Line 7, col 27

Shows error toast:
┌────────────────────────────────────────────────┐
│ ✗ YAML syntax error on line 7                  │
│   Press [e] to edit, [r] to regenerate, [Esc] │
└────────────────────────────────────────────────┘

User presses [e] → EDITING (re-opens editor at line 7)
User fixes: pattern: "[0-9]{4}"
[Esc] from editor → VALIDATING again

VALIDATING passes → YAML_RESULT returns
```

### Example 2: Python Runtime Error (Parser Lab)

```
User edits parser, introduces bug:

def parse(self, ctx):
    df = pd.read_csv(ctx.input_path)
    df['amount'] = int(df['amount'])  # BUG: can't int() a series
    return df

[Esc] from editor → VALIDATING starts

Stage 1 (syntax): PASS
Stage 2 (sample test): Executes against sample file

Exception caught:
  TypeError: int() argument must be a string or a number
  File "draft_parser_abc.py", line 15
  Traceback shows execution stopped at df['amount'] = int(df['amount'])

Shows error toast:
┌────────────────────────────────────────────────┐
│ ✗ Runtime error on sample file (TypeError)     │
│   Press [e] to edit, [r] to regenerate, [t]...│
└────────────────────────────────────────────────┘

User presses [e] → EDITING (re-opens editor)
User fixes: df['amount'] = df['amount'].astype(int)
[Esc] from editor → VALIDATING again

Stage 1 (syntax): PASS
Stage 2 (sample test): PASS → RESULT_VALIDATED
```

### Example 3: Schema Error (Pathfinder YAML)

```
User edits YAML, uses invalid type:

extract:
  year:
    from: segment(-3)
    type: date_invalid    # Invalid type

[Esc] from editor → VALIDATING starts

Schema validation finds:
  Line 6: "date_invalid" is not a valid type
  Valid types: date, date_iso, integer, string, float, email

Also finds:
  Glob pattern "**/*/????/file.csv" matches 0 of 5 sample paths
  Suggestion: Check glob syntax or adjust pattern

Shows error dialog:
┌─ VALIDATION ERRORS ──────────────────────────────┐
│                                                   │
│ Error 1: Invalid type on line 6                   │
│   Got: date_invalid                               │
│   Valid: date, date_iso, integer, string, float   │
│                                                   │
│ Error 2: Glob pattern mismatch                    │
│   Pattern: "**/*/????/file.csv"                   │
│   Matched: 0 of 5 sample paths                    │
│   Samples:                                        │
│   /data/2024/01/file.csv (no match)              │
│                                                   │
│  [e] Edit   [r] Regenerate   [Esc] Discard      │
└───────────────────────────────────────────────────┘

User presses [e] → EDITING
User fixes: type: date_iso, glob: "**/????/??/*.csv"
[Esc] from editor → VALIDATING

Both errors resolved → YAML_RESULT
```

---

## 11. New Gaps Introduced

None. This resolution is self-contained and fully specifies error handling for manual editing in all three wizards.

---

## 12. References

- `specs/ai_wizards.md` Section 4.2 (Python Extractor Validation)
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine)
- `specs/ai_wizards.md` Section 5.2.1 (Parser Lab state machine)
- `specs/ai_wizards.md` Section 5.5 (Semantic Path Wizard dialog)
- Round 6 (GAP-TUI-001): $EDITOR subprocess handling specification
- Round 12 (GAP-TUI-001 resolution): EDITING state implementation details

---

## 13. Spec Updates Required

### Update `specs/ai_wizards.md` Section 5.1.1 (Pathfinder State Machine)

**Replace existing state machine diagram with:**

```markdown
#### 5.1.1 Pathfinder Wizard State Machine (Updated)

[Diagram showing new EDITING, VALIDATING, VALIDATION_ERROR states]

**New States:**
- **EDITING**: User editing YAML in external editor (new)
- **VALIDATING**: Validating YAML after editor close (new)
- **VALIDATION_ERROR**: YAML validation failed, recovery options shown (new)

**New Transitions:**
| From | To | Trigger | Guard |
|------|----|---------| ------|
| YAML_RESULT | EDITING | e pressed | $EDITOR available |
| EDITING | VALIDATING | Editor closes | File modified |
| VALIDATING | YAML_RESULT | Validation pass | - |
| VALIDATING | VALIDATION_ERROR | Validation fails | - |
| VALIDATION_ERROR | EDITING | e pressed | - |
| VALIDATION_ERROR | ANALYZING | r pressed | - |
| VALIDATION_ERROR | (previous) | Esc pressed | - |
```

### Update `specs/ai_wizards.md` Section 5.2.1 (Parser Lab State Machine)

**Similar updates for Parser Lab state machine and VALIDATING state definition:**

```markdown
#### 5.2.1 Parser Lab State Machine (Updated)

[New diagram with EDITING → VALIDATING → RESULT_* flow]

**New State: VALIDATING**
| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| VALIDATING | Editor closes | Tests complete | Two-stage validation: (1) Syntax+method check (2) Sample execution |
| VALIDATION_ERROR | Validation fails | User recovery | Shows error toast + detail, offers edit/regenerate/discard |
```

### Add New Section: Error Handling and Recovery

**Add Section 4.3 to ai_wizards.md:**

```markdown
### 4.3 Error Handling in Manual Edit Mode

When users manually edit generated YAML or Python code using [e] (Edit),
the following error handling applies:

#### Error Types

**YAML Errors (Pathfinder/Semantic):**
- Syntax errors (invalid YAML)
- Schema errors (invalid fields/types)
- Logic errors (glob pattern mismatch)

**Python Errors (Parser Lab):**
- Syntax errors (ast.parse failures)
- Semantic errors (missing methods, disallowed imports)
- Runtime errors (exceptions during sample test)
- Type errors (output schema mismatch)

[Full error categories table from Section 1.1, 1.2]

#### Validation Timing

Validation occurs in two phases:

1. **On Editor Close (EDITING → VALIDATING)**
   - Fast syntax + schema check (<100ms for YAML, <200ms for Python)
   - Detects syntax errors immediately
   - Shows error toast with recovery options

2. **On Sample Test (Parser Lab only)**
   - Executes against sample data
   - Catches runtime exceptions
   - Validates output schema

#### Recovery Workflows

[Three-level recovery from Section 6.1]

```

---

## 14. Confidence and Sign-Off

**Confidence Level:** HIGH

This resolution:
- Covers all three wizards (Pathfinder YAML, Parser Lab Python, Semantic YAML)
- Provides concrete error types with detection mechanisms
- Specifies clear UI feedback patterns
- Defines complete state transitions
- Includes implementation checklist

No ambiguity about what happens when users edit generated code and errors occur.

**Ready for:** Product implementation phase

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial specification |

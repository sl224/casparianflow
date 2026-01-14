# Engineer Round 009: GAP-ERROR-001

## Gap Resolution: GAP-ERROR-001

**Gap:** Invalid YAML from LLM - no retry mechanism
**Confidence:** HIGH

---

### Problem Statement

The AI Wizards generate YAML extraction rules and Python code via LLM. When the LLM generates invalid output, the current spec only mentions:
- Section 9.1: "Invalid syntax | AI generated invalid code. Retry? | Retry or manual edit"
- Section 9.3: "Max 3 automatic retries per wizard invocation"

What's missing:
1. How to distinguish invalid YAML (syntax error) from valid-but-wrong YAML (schema violation)
2. What validation checks run and in what order
3. How retry context improves subsequent attempts
4. User feedback during the validation/retry cycle
5. Integration with the existing REGENERATING state

---

### Proposed Solution: Tiered Validation Pipeline

LLM output goes through a **three-tier validation pipeline** before reaching the user. Each tier can trigger retries with increasingly specific error context.

```
┌────────────────────────────────────────────────────────────────────────┐
│                     LLM OUTPUT VALIDATION PIPELINE                      │
├────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                 │
│  │   TIER 1    │───►│   TIER 2    │───►│   TIER 3    │───► RESULT     │
│  │   Syntax    │    │   Schema    │    │  Semantic   │                 │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘                 │
│         │                  │                  │                         │
│     FAIL: Parse         FAIL: Wrong       FAIL: Logic                   │
│     error, retry        structure,        error, retry                  │
│     with error          retry with        with sample                   │
│     message             schema hint       output                        │
│                                                                         │
└────────────────────────────────────────────────────────────────────────┘
```

---

### Tier 1: Syntax Validation

**Purpose:** Verify output is parseable YAML/Python.

**Checks:**
| Check | Error Type | Example Error |
|-------|------------|---------------|
| YAML parse | `YAML_SYNTAX_ERROR` | `line 5: mapping values not allowed here` |
| Python parse | `PYTHON_SYNTAX_ERROR` | `line 12: unexpected indent` |
| Encoding | `ENCODING_ERROR` | `invalid UTF-8 sequence at position 234` |

**Detection Code:**

```python
def validate_tier1_yaml(raw_output: str) -> ValidationResult:
    """Tier 1: Is it valid YAML?"""
    try:
        # Strip markdown code fences if present
        content = strip_code_fences(raw_output, "yaml")
        parsed = yaml.safe_load(content)
        return ValidationResult(tier=1, valid=True, parsed=parsed)
    except yaml.YAMLError as e:
        return ValidationResult(
            tier=1,
            valid=False,
            error_type="YAML_SYNTAX_ERROR",
            error_message=str(e),
            error_line=extract_line_number(e),
            raw_output=raw_output
        )

def validate_tier1_python(raw_output: str) -> ValidationResult:
    """Tier 1: Is it valid Python?"""
    try:
        content = strip_code_fences(raw_output, "python")
        ast.parse(content)
        return ValidationResult(tier=1, valid=True, parsed=content)
    except SyntaxError as e:
        return ValidationResult(
            tier=1,
            valid=False,
            error_type="PYTHON_SYNTAX_ERROR",
            error_message=e.msg,
            error_line=e.lineno,
            raw_output=raw_output
        )
```

**Retry Context for Tier 1 Failures:**

```
Previous output had syntax error:
  Line 5: mapping values not allowed here

The problematic section was:
  ```
  extract:
    mission_id
      from: segment(-3)  # <- Error here: missing colon after mission_id
  ```

Please regenerate with correct YAML syntax.
```

---

### Tier 2: Schema Validation

**Purpose:** Verify output conforms to extraction rule schema.

**For YAML Extraction Rules (from specs/extraction.md Section 3.1):**

| Field | Required | Type | Validation |
|-------|----------|------|------------|
| `version` | Optional | int | Must be 1 if present |
| `name` | Required | string | Non-empty, valid identifier |
| `glob` | Required | string | Valid glob pattern |
| `extract` | Optional | object/null | If present, each field must have valid `from` |
| `extract.*.from` | Required per field | string | One of: `segment(N)`, `filename`, `full_path`, `rel_path` |
| `extract.*.type` | Optional | string | One of: `string`, `integer`, `date`, `uuid` |
| `extract.*.pattern` | Optional | string | Valid regex |
| `tag` | Optional | string | Valid tag name |
| `priority` | Optional | int | 0-1000 |

**For Python Extractors:**

| Check | Validation |
|-------|------------|
| Has `extract` function | `def extract(path: str) -> dict:` signature |
| Returns dict | Return type annotation or docstring |
| No dangerous imports | Blocklist: `os.system`, `subprocess`, `eval`, `exec` |
| No syntax errors in f-strings | Common LLM mistake |

**Detection Code:**

```python
YAML_SCHEMA = {
    "type": "object",
    "required": ["name", "glob"],
    "properties": {
        "version": {"type": "integer", "enum": [1]},
        "name": {"type": "string", "minLength": 1, "pattern": "^[a-z][a-z0-9_]*$"},
        "glob": {"type": "string", "minLength": 1},
        "extract": {
            "oneOf": [
                {"type": "null"},
                {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "required": ["from"],
                        "properties": {
                            "from": {
                                "type": "string",
                                "pattern": "^(segment\\(-?\\d+\\)|filename|full_path|rel_path)$"
                            },
                            "pattern": {"type": "string"},
                            "type": {"type": "string", "enum": ["string", "integer", "date", "uuid"]},
                            "normalize": {"type": "string", "enum": ["lowercase", "uppercase", "strip_leading_zeros"]},
                            "default": {}
                        }
                    }
                }
            ]
        },
        "tag": {"type": "string", "pattern": "^[a-z][a-z0-9_]*$"},
        "priority": {"type": "integer", "minimum": 0, "maximum": 1000}
    }
}

def validate_tier2_yaml(parsed: dict) -> ValidationResult:
    """Tier 2: Does it match our extraction rule schema?"""
    try:
        jsonschema.validate(parsed, YAML_SCHEMA)

        # Additional semantic checks not expressible in JSON Schema
        if "extract" in parsed and parsed["extract"]:
            for field_name, field_def in parsed["extract"].items():
                if "pattern" in field_def:
                    try:
                        re.compile(field_def["pattern"])
                    except re.error as e:
                        return ValidationResult(
                            tier=2,
                            valid=False,
                            error_type="INVALID_REGEX",
                            error_message=f"Field '{field_name}' has invalid regex: {e}",
                            field=field_name
                        )

        return ValidationResult(tier=2, valid=True, validated=parsed)

    except jsonschema.ValidationError as e:
        return ValidationResult(
            tier=2,
            valid=False,
            error_type="SCHEMA_VIOLATION",
            error_message=e.message,
            error_path=list(e.absolute_path),
            parsed=parsed
        )
```

**Retry Context for Tier 2 Failures:**

```
Output failed schema validation:
  Field 'extract.mission_id.from' has invalid value "folder(-3)"
  Expected: one of "segment(N)", "filename", "full_path", "rel_path"

The extraction rule schema requires:
  - from: must be segment(N), filename, full_path, or rel_path
  - type: must be string, integer, date, or uuid
  - pattern: must be a valid regex

Please regenerate using correct field syntax.
```

---

### Tier 3: Semantic Validation

**Purpose:** Verify output produces expected results on sample data.

**Checks:**
| Check | Error Type | Description |
|-------|------------|-------------|
| Glob matches sample | `GLOB_MISMATCH` | Generated glob doesn't match input paths |
| Extraction produces output | `EMPTY_EXTRACTION` | Rule matches but extracts nothing |
| Types parse correctly | `TYPE_COERCION_FAILED` | `type: date` but value isn't a date |
| No runtime errors | `RUNTIME_ERROR` | Python extractor throws exception |

**Detection Code:**

```python
def validate_tier3(
    validated_output: dict | str,
    sample_paths: List[str],
    output_type: OutputType
) -> ValidationResult:
    """Tier 3: Does it work on the sample data?"""

    if output_type == OutputType.YAML:
        rule = validated_output

        # Check 1: Glob matches at least one sample path
        matched = [p for p in sample_paths if fnmatch_glob(p, rule["glob"])]
        if not matched:
            return ValidationResult(
                tier=3,
                valid=False,
                error_type="GLOB_MISMATCH",
                error_message=f"Glob '{rule['glob']}' doesn't match any sample paths",
                sample_paths=sample_paths[:3]  # Show first 3
            )

        # Check 2: Extraction produces non-empty output
        if rule.get("extract"):
            extractions = [extract_from_path(p, rule) for p in matched]
            empty_count = sum(1 for e in extractions if not e)
            if empty_count == len(extractions):
                return ValidationResult(
                    tier=3,
                    valid=False,
                    error_type="EMPTY_EXTRACTION",
                    error_message="Rule matches files but extracts no fields",
                    sample_path=matched[0]
                )

        # Check 3: Show preview for user verification
        return ValidationResult(
            tier=3,
            valid=True,
            preview=[
                {"path": p, "extracted": extract_from_path(p, rule)}
                for p in matched[:5]
            ]
        )

    elif output_type == OutputType.PYTHON:
        code = validated_output

        # Execute in sandbox
        try:
            results = execute_python_extractor_sandbox(code, sample_paths[:5])
            if all(not r for r in results):
                return ValidationResult(
                    tier=3,
                    valid=False,
                    error_type="EMPTY_EXTRACTION",
                    error_message="Extractor returns empty dict for all samples"
                )
            return ValidationResult(tier=3, valid=True, preview=results)
        except SandboxExecutionError as e:
            return ValidationResult(
                tier=3,
                valid=False,
                error_type="RUNTIME_ERROR",
                error_message=str(e),
                traceback=e.traceback
            )
```

**Retry Context for Tier 3 Failures:**

```
Output failed semantic validation:
  Glob "**/mission/????-??-??/*.csv" doesn't match sample paths:
    - /data/mission_042/2024-01-15/readings.csv
    - /data/mission_043/2024-01-16/telemetry.csv

The sample paths have structure: /data/mission_{id}/{date}/*.csv
Note: mission has underscore followed by ID, not just "mission/"

Please regenerate with a glob that matches these paths.
```

---

### Retry Strategy

#### Retry Limits

| Tier | Max Auto-Retries | Rationale |
|------|------------------|-----------|
| Tier 1 (Syntax) | 2 | Syntax errors are obvious; if LLM can't fix after 2 tries, likely model issue |
| Tier 2 (Schema) | 2 | Schema hints are specific; should fix quickly |
| Tier 3 (Semantic) | 1 | Semantic issues often need human insight |
| **Total** | 3 | As specified in Section 9.3 |

**Retry Budget Tracking:**

```python
@dataclass
class RetryBudget:
    tier1_remaining: int = 2
    tier2_remaining: int = 2
    tier3_remaining: int = 1
    total_remaining: int = 3

    def can_retry(self, tier: int) -> bool:
        if self.total_remaining <= 0:
            return False
        if tier == 1:
            return self.tier1_remaining > 0
        elif tier == 2:
            return self.tier2_remaining > 0
        elif tier == 3:
            return self.tier3_remaining > 0
        return False

    def consume(self, tier: int):
        self.total_remaining -= 1
        if tier == 1:
            self.tier1_remaining -= 1
        elif tier == 2:
            self.tier2_remaining -= 1
        elif tier == 3:
            self.tier3_remaining -= 1
```

#### Retry Context Enhancement

Each retry includes accumulated context to help the LLM succeed:

```python
def build_retry_prompt(
    original_prompt: str,
    validation_history: List[ValidationResult],
    sample_paths: List[str]
) -> str:
    """Build enhanced retry prompt with failure context."""

    context_parts = [original_prompt]

    context_parts.append("\n---\nPREVIOUS ATTEMPTS AND ERRORS:\n")

    for i, result in enumerate(validation_history, 1):
        context_parts.append(f"\nAttempt {i}: {result.error_type}")
        context_parts.append(f"  Error: {result.error_message}")

        if result.error_line:
            context_parts.append(f"  Line: {result.error_line}")

        if result.error_path:
            context_parts.append(f"  Path: {'.'.join(str(p) for p in result.error_path)}")

    context_parts.append("\n---\nREQUIREMENTS REMINDER:")
    context_parts.append("""
- YAML extraction rules must have: name, glob
- extract fields must use: from (segment(N), filename, full_path, rel_path)
- pattern must be valid regex
- tag must be lowercase with underscores only
""")

    context_parts.append(f"\n---\nSAMPLE PATHS TO MATCH:")
    for p in sample_paths[:3]:
        context_parts.append(f"  - {p}")

    return "\n".join(context_parts)
```

---

### User Feedback During Validation

#### In TUI (Spinner States)

The ANALYZING and REGENERATING states show progressive feedback:

```
┌─ PATHFINDER ────────────────────────────────────────────────────────┐
│                                                                      │
│  ◐ Analyzing paths...                   ← Initial analysis          │
│                                                                      │
│  ──────────────────────────────────────────────────────────────────  │
│  OR                                                                  │
│  ──────────────────────────────────────────────────────────────────  │
│                                                                      │
│  ◐ Validating output... (attempt 2/3)   ← During retry              │
│    Previous: syntax error on line 5                                  │
│                                                                      │
│  ──────────────────────────────────────────────────────────────────  │
│  OR                                                                  │
│  ──────────────────────────────────────────────────────────────────  │
│                                                                      │
│  ◐ Testing on samples... (attempt 3/3)  ← Semantic validation       │
│    Previous: glob didn't match paths                                 │
│                                                                      │
│  [Esc] Cancel                                                        │
└──────────────────────────────────────────────────────────────────────┘
```

#### Spinner Message States

```python
def get_spinner_message(state: WizardState, retry_context: Optional[RetryContext]) -> str:
    if state == WizardState.ANALYZING:
        return "Analyzing paths..."

    if state == WizardState.REGENERATING:
        if retry_context is None:
            return "Regenerating with hint..."

        attempt = 4 - retry_context.budget.total_remaining  # 1, 2, or 3
        max_attempts = 3

        base = f"Validating output... (attempt {attempt}/{max_attempts})"

        if retry_context.last_error:
            return f"{base}\n  Previous: {retry_context.last_error.short_message}"

        return base
```

---

### Escalation When Retries Exhausted

When all 3 retries fail, transition to ANALYSIS_ERROR state with actionable options:

```
┌─ PATHFINDER ────────────────────────────────────────────────────────┐
│                                                                      │
│  ✗ AI couldn't generate valid output after 3 attempts               │
│                                                                      │
│  Last error: Glob pattern doesn't match sample paths                │
│                                                                      │
│  ┌─ Error Details ─────────────────────────────────────────────────┐│
│  │ Attempt 1: YAML syntax error (line 5: mapping values...)        ││
│  │ Attempt 2: Schema error (extract.from: invalid value)           ││
│  │ Attempt 3: Glob mismatch (pattern didn't match samples)         ││
│  └─────────────────────────────────────────────────────────────────┘│
│                                                                      │
│  Options:                                                            │
│    [h] Add hint and retry   - Give AI more context                  │
│    [e] Edit manually        - Open in $EDITOR                       │
│    [r] Retry fresh          - Start over (resets retry count)       │
│    [Esc] Cancel             - Give up                               │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

#### Escalation Options

| Option | Behavior | When to Suggest |
|--------|----------|-----------------|
| **Add hint** | Opens HINT_INPUT, resets tier-specific retries but not total | Semantic errors - user can clarify intent |
| **Edit manually** | Opens $EDITOR with last generated output | Schema errors - user can fix structure |
| **Retry fresh** | Resets ALL retry counts, starts from scratch | Syntax errors - might be LLM glitch |
| **Cancel** | Closes dialog, no draft created | User gives up |

**State Transition on Exhausted Retries:**

```
REGENERATING (retry budget = 0) → ANALYSIS_ERROR

ANALYSIS_ERROR user actions:
  - [h] → HINT_INPUT (resets tier1/2 to 1 each, total stays 0 → manually add 1)
  - [e] → EDITING
  - [r] → ANALYZING (full reset)
  - [Esc] → CLOSED
```

---

### Integration with State Machines

#### Pathfinder State Machine Updates (Section 5.1.1)

Add new internal states for validation tiers:

```
ANALYZING
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│                 VALIDATION_LOOP (internal)              │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐           │
│  │ TIER1_VAL │─►│ TIER2_VAL │─►│ TIER3_VAL │──► EXIT   │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘           │
│        │              │              │                  │
│        ▼              ▼              ▼                  │
│   TIER1_RETRY    TIER2_RETRY   TIER3_RETRY             │
│   (if budget)    (if budget)   (if budget)             │
│        │              │              │                  │
│        └──────────────┴──────────────┘                  │
│                       │                                 │
│                       ▼                                 │
│               VALIDATION_EXHAUSTED                      │
└─────────────────────────────────────────────────────────┘
    │                                    │
    ▼                                    ▼
YAML_RESULT / PYTHON_RESULT         ANALYSIS_ERROR
```

**Note:** The VALIDATION_LOOP is an internal implementation detail. The user-visible states remain as specified in Section 5.1.1. The loop runs within ANALYZING or REGENERATING states.

#### Updated State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| ANALYZING | Wizard invoked | Validation completes/exhausted | Spinner with progress. Shows retry attempt count if retrying. |
| REGENERATING | Hint/edit/refresh | Validation completes/exhausted | Spinner with progress. Shows "attempt N/3" if retrying. |
| ANALYSIS_ERROR | Validation exhausted (total retries = 0) | User action | Shows all error details. Options: hint/edit/fresh/cancel. |

#### Updated Transitions

| From | To | Trigger | Guard |
|------|----|---------| ------|
| ANALYZING | YAML_RESULT | Validation passes | All 3 tiers pass |
| ANALYZING | PYTHON_RESULT | Validation passes | All 3 tiers pass, complex patterns |
| ANALYZING | ANALYSIS_ERROR | Validation exhausted | retry_budget.total_remaining == 0 |
| ANALYSIS_ERROR | HINT_INPUT | h | Always (adds 1 to total budget) |
| ANALYSIS_ERROR | EDITING | e | $EDITOR available |
| ANALYSIS_ERROR | ANALYZING | r | Always (full budget reset) |
| ANALYSIS_ERROR | CLOSED | Esc | Always |

---

### Implementation Data Structures

```python
@dataclass
class ValidationResult:
    tier: int  # 1, 2, or 3
    valid: bool
    error_type: Optional[str] = None  # YAML_SYNTAX_ERROR, SCHEMA_VIOLATION, etc.
    error_message: Optional[str] = None
    error_line: Optional[int] = None
    error_path: Optional[List[str]] = None  # For schema errors
    raw_output: Optional[str] = None  # Original LLM output
    parsed: Optional[dict] = None  # Tier 1 success
    validated: Optional[dict] = None  # Tier 2 success
    preview: Optional[List[dict]] = None  # Tier 3 success

    @property
    def short_message(self) -> str:
        """One-line summary for spinner display."""
        if self.error_type == "YAML_SYNTAX_ERROR":
            return f"syntax error (line {self.error_line})"
        elif self.error_type == "SCHEMA_VIOLATION":
            path = ".".join(str(p) for p in self.error_path) if self.error_path else "root"
            return f"schema error ({path})"
        elif self.error_type == "GLOB_MISMATCH":
            return "glob didn't match samples"
        elif self.error_type == "EMPTY_EXTRACTION":
            return "extracted no fields"
        elif self.error_type == "RUNTIME_ERROR":
            return "execution error"
        return self.error_message or "unknown error"

@dataclass
class RetryContext:
    budget: RetryBudget
    validation_history: List[ValidationResult]
    last_error: Optional[ValidationResult] = None

    def add_failure(self, result: ValidationResult):
        self.validation_history.append(result)
        self.last_error = result
        self.budget.consume(result.tier)
```

---

### Error Examples and Recovery Flows

#### Example 1: Syntax Error Recovery

```
User: Invokes Pathfinder on /data/mission_042/2024-01-15/*.csv

LLM Output (Attempt 1):
  name: mission_data
  glob: **/mission_*/????-??-??/*.csv
  extract:
    mission_id                        # <- Missing colon
      from: segment(-3)

Tier 1 Validation: FAIL
  Error: YAML_SYNTAX_ERROR
  Message: line 5: mapping values not allowed here

Retry Prompt (Attempt 2):
  [Original prompt]
  ---
  PREVIOUS ATTEMPTS AND ERRORS:
  Attempt 1: YAML_SYNTAX_ERROR
    Error: line 5: mapping values not allowed here
  ---
  REQUIREMENTS REMINDER:
  - Field definitions must be: field_name: followed by properties
  - Example: mission_id:
               from: segment(-3)

LLM Output (Attempt 2):
  name: mission_data
  glob: "**/mission_*/????-??-??/*.csv"
  extract:
    mission_id:
      from: segment(-3)
  tag: mission_data

Tier 1: PASS, Tier 2: PASS, Tier 3: PASS
Result: YAML_RESULT state with preview
```

#### Example 2: Schema Error Recovery

```
User: Invokes Pathfinder on /archive/client_ABC/2024/Q1/*.csv

LLM Output (Attempt 1):
  name: client_reports
  glob: "**/client_*/????/Q?/*.csv"
  extract:
    client:
      from: folder(-4)              # <- Wrong! Should be segment(-4)
      pattern: "client_(.*)"

Tier 1: PASS (valid YAML)
Tier 2: FAIL
  Error: SCHEMA_VIOLATION
  Message: 'extract.client.from' has invalid value "folder(-4)"
  Path: ["extract", "client", "from"]

Retry Prompt (Attempt 2):
  [Original prompt]
  ---
  PREVIOUS ATTEMPTS AND ERRORS:
  Attempt 1: SCHEMA_VIOLATION
    Error: 'extract.client.from' has invalid value "folder(-4)"
    Path: extract.client.from
  ---
  REQUIREMENTS REMINDER:
  - from: must be one of: segment(N), filename, full_path, rel_path
  - segment(N) uses negative indexing: segment(-1) = filename's folder

LLM Output (Attempt 2):
  name: client_reports
  glob: "**/client_*/????/Q?/*.csv"
  extract:
    client:
      from: segment(-4)
      pattern: "client_(.*)"

Tier 1: PASS, Tier 2: PASS, Tier 3: PASS
Result: YAML_RESULT
```

#### Example 3: Semantic Error Leading to Manual Edit

```
User: Invokes Pathfinder on complex paths

Attempt 1: YAML_SYNTAX_ERROR (tier 1 retry)
Attempt 2: GLOB_MISMATCH (tier 3, budget exhausted)
Attempt 3: GLOB_MISMATCH (tier 3, total budget = 0)

State: ANALYSIS_ERROR

User presses [e] to edit manually

Editor opens with last LLM output:
  name: complex_rule
  glob: "**/data/*/files/*.csv"  # <- User sees this doesn't match
  extract:
    ...

User edits glob to: "**/data/**/processed/*.csv"
Saves and exits editor

Tier 2: PASS (schema valid)
Tier 3: PASS (glob now matches)

Result: YAML_RESULT with user's edited rule
```

---

### Trade-offs

| Aspect | Chosen Approach | Alternative |
|--------|-----------------|-------------|
| **Retry budget** | Per-tier limits + total limit | Single retry counter for all tiers |
| **Retry context** | Full history in prompt | Only last error |
| **User feedback** | Progressive spinner messages | Silent retries |
| **Escalation** | Multiple options (hint/edit/fresh) | Only "try again" or "cancel" |

**Rationale:**
- Per-tier limits prevent burning all retries on syntax errors (cheap to fix)
- Full history helps LLM avoid repeating same mistakes
- Progressive feedback manages user expectations
- Multiple escalation paths let user choose best recovery strategy

---

### New Gaps Introduced

1. **GAP-ERROR-002: Sandbox execution environment for Python validation**
   - Tier 3 Python validation requires sandboxed execution
   - What isolation mechanism? (subprocess, seccomp, WASM?)
   - Timeout limits for execution?

2. **GAP-ERROR-003: LLM prompt engineering for recovery**
   - Exact prompt format for retry context
   - Should we use structured (JSON) or natural language error descriptions?
   - How much of original output to include in retry prompt?

3. **GAP-ERROR-004: Telemetry for validation failures**
   - Should we track which error types are most common?
   - Can this data improve prompts over time?
   - Privacy implications of logging LLM outputs?

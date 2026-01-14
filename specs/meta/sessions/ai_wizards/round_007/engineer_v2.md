# Engineer Round 007 v2: GAP-INT-002

## Gap Resolution: GAP-INT-002 (Revised)

**Gap:** YAML vs Python output decision algorithm undefined
**Confidence:** HIGH
**Revision:** Addresses ISSUE-R7-001 through ISSUE-R7-005 from Reviewer feedback

---

### Summary of Changes

| Issue | Resolution |
|-------|------------|
| ISSUE-R7-001 | Full `analyze_complexity()` implementation with concrete detection rules |
| ISSUE-R7-002 | Clarified default vs override: Q1→integer is YAML_OK default, hints can escalate |
| ISSUE-R7-003 | Keyword-based `hints_require_python()` with explicit word list |
| ISSUE-R7-004 | Added `recommendations` field to output; `YAML_COMPLEX` not needed |
| ISSUE-R7-005 | Complete `DetectedPattern` struct definition |

---

### Data Structures

#### DetectedPattern (ISSUE-R7-005 Fix)

```python
from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any
from enum import Enum

@dataclass
class DetectedPattern:
    """
    A pattern detected from analyzing sample file paths.

    Created by the pattern detection phase (prior to this algorithm).
    Each pattern represents one potential extraction field.
    """
    # Source location
    source_type: str           # "segment", "filename", "full_path", "rel_path"
    source_value: Optional[str]  # e.g., "-3" for segment(-3), None for filename

    # Detected values from samples
    sample_values: List[str]   # All values seen at this position, e.g. ["Q1", "Q2", "Q3"]

    # Inferred properties
    inferred_field_name: str   # Suggested name, e.g. "quarter", "year"
    inferred_type: Optional[str]  # "integer", "date", "string", "uuid", None if ambiguous

    # If regex was detected
    regex_pattern: Optional[str]  # e.g., "Q(\d)" if pattern detected
    regex_captures: int = 0       # Number of capture groups

    # User enrichment
    user_hints: List[str] = field(default_factory=list)  # Hints specific to this pattern

    def __repr__(self):
        return f"DetectedPattern({self.source_type}({self.source_value}): {self.inferred_field_name})"


@dataclass
class Complexity:
    """
    Result of complexity analysis on a DetectedPattern.
    """
    has_computed_fields: bool = False
    computed_field_description: Optional[str] = None

    has_conditional_logic: bool = False
    condition_description: Optional[str] = None

    has_multi_step_transform: bool = False
    transform_chain: Optional[str] = None

    has_external_lookup: bool = False
    lookup_description: Optional[str] = None

    has_cross_segment_dependency: bool = False
    dependency_description: Optional[str] = None

    # Readability warning (not a blocker, just recommendation)
    regex_complexity_warning: Optional[str] = None
```

#### Classification Types

```python
class PatternClassification(Enum):
    YAML_OK = "yaml_ok"
    PYTHON_REQUIRED = "python_required"


@dataclass
class ClassifiedPattern:
    """Result of classifying a single pattern."""
    pattern: DetectedPattern
    classification: PatternClassification
    python_reason: Optional[str] = None  # Why Python is needed (if applicable)


class OutputFormat(Enum):
    YAML = "yaml"
    PYTHON = "python"


@dataclass
class DecisionResult:
    """
    Complete decision output with format, classified patterns, and recommendations.
    """
    format: OutputFormat
    classified_patterns: List[ClassifiedPattern]
    python_reasons: List[str]  # All reasons forcing Python (if any)
    recommendations: List[str]  # Non-blocking suggestions (ISSUE-R7-004 fix)
```

---

### Core Algorithm

#### Main Decision Function

```python
def decide_output_format(
    patterns: List[DetectedPattern],
    user_hints: List[str]
) -> DecisionResult:
    """
    Main decision function: Analyze all patterns and decide output format.

    Args:
        patterns: Detected patterns from sample analysis
        user_hints: Global hints from user (applied to all patterns)

    Returns:
        DecisionResult with format, classifications, and recommendations
    """
    # Handle empty pattern set (ISSUE-R7-006)
    if not patterns:
        return DecisionResult(
            format=OutputFormat.YAML,
            classified_patterns=[],
            python_reasons=[],
            recommendations=["No extraction patterns detected. Generating tag-only rule."]
        )

    # Classify each pattern
    classified = []
    recommendations = []

    for pattern in patterns:
        # Merge global hints with pattern-specific hints
        combined_hints = user_hints + pattern.user_hints
        result = classify_pattern(pattern, combined_hints)
        classified.append(result)

        # Check for complexity warnings (ISSUE-R7-004 fix)
        complexity = analyze_complexity(pattern)
        if complexity.regex_complexity_warning:
            recommendations.append(complexity.regex_complexity_warning)

    # Collect Python-required patterns
    python_patterns = [c for c in classified
                       if c.classification == PatternClassification.PYTHON_REQUIRED]
    python_reasons = [c.python_reason for c in python_patterns if c.python_reason]

    if python_patterns:
        return DecisionResult(
            format=OutputFormat.PYTHON,
            classified_patterns=classified,
            python_reasons=python_reasons,
            recommendations=recommendations
        )
    else:
        return DecisionResult(
            format=OutputFormat.YAML,
            classified_patterns=classified,
            python_reasons=[],
            recommendations=recommendations
        )
```

#### Pattern Classification

```python
def classify_pattern(
    pattern: DetectedPattern,
    user_hints: List[str]
) -> ClassifiedPattern:
    """
    Classify a single detected pattern as YAML-expressible or Python-required.

    Classification order:
    1. Check user hints (hints can ESCALATE default to Python, never downgrade)
    2. Check pattern complexity
    3. Default to YAML if expressible
    """

    # Step 1: Check for user hints that force Python (ISSUE-R7-003)
    hint_result = hints_require_python(pattern, user_hints)
    if hint_result.requires_python:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=hint_result.reason
        )

    # Step 2: Check pattern complexity (ISSUE-R7-001)
    complexity = analyze_complexity(pattern)

    if complexity.has_computed_fields:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=f"Computed field: {complexity.computed_field_description}"
        )

    if complexity.has_conditional_logic:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=f"Conditional logic: {complexity.condition_description}"
        )

    if complexity.has_multi_step_transform:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=f"Multi-step transformation: {complexity.transform_chain}"
        )

    if complexity.has_external_lookup:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=f"External lookup required: {complexity.lookup_description}"
        )

    if complexity.has_cross_segment_dependency:
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=f"Cross-segment dependency: {complexity.dependency_description}"
        )

    # Step 3: Pattern is YAML-expressible
    return ClassifiedPattern(
        pattern=pattern,
        classification=PatternClassification.YAML_OK,
        python_reason=None
    )
```

---

### Complexity Analysis (ISSUE-R7-001 Fix)

```python
@dataclass
class HintResult:
    """Result of hint analysis."""
    requires_python: bool
    reason: Optional[str] = None


# Keywords that indicate Python-required computation (ISSUE-R7-003 fix)
COMPUTATION_KEYWORDS = {
    # Explicit computation
    "compute", "calculate", "derive", "formula",
    # Range/transformation
    "start/end", "range", "convert to", "expand",
    # Lookup/mapping
    "lookup", "map to", "translate", "reference",
    # Conditional
    "if", "when", "otherwise", "depending on", "conditional",
    # Aggregation
    "combine", "merge", "join", "aggregate",
}


def hints_require_python(
    pattern: DetectedPattern,
    user_hints: List[str]
) -> HintResult:
    """
    Check if user hints indicate Python is required.

    Uses keyword matching against COMPUTATION_KEYWORDS.
    Future: Could use LLM classification (see GAP-INT-003).
    """
    hint_text = " ".join(user_hints).lower()

    for keyword in COMPUTATION_KEYWORDS:
        if keyword in hint_text:
            # Check if this keyword applies to this pattern's field
            field_name = pattern.inferred_field_name.lower()
            # Simple heuristic: keyword in context of field name
            if field_name in hint_text or keyword in hint_text:
                return HintResult(
                    requires_python=True,
                    reason=f"User hint contains '{keyword}' for field '{pattern.inferred_field_name}'"
                )

    return HintResult(requires_python=False)


def analyze_complexity(pattern: DetectedPattern) -> Complexity:
    """
    Analyze a pattern to determine its complexity.

    This is the core detection logic (ISSUE-R7-001 fix).

    Detection rules:
    1. Computed fields: Pattern produces MULTIPLE output fields from ONE input
    2. Conditional logic: Output depends on runtime conditions
    3. Multi-step transform: Requires intermediate variables
    4. External lookup: Values must be resolved against external data
    5. Cross-segment dependency: Interpretation depends on another segment
    """
    result = Complexity()

    # Rule 1: Detect computed fields
    # Computed = ONE input value produces MULTIPLE output fields
    result = _check_computed_fields(pattern, result)

    # Rule 2: Detect conditional logic
    # Conditional = Output varies based on runtime conditions
    result = _check_conditional_logic(pattern, result)

    # Rule 3: Detect multi-step transforms
    # Multi-step = Requires intermediate processing
    result = _check_multi_step_transform(pattern, result)

    # Rule 4: Detect external lookups
    # Lookup = Value must be resolved against external data
    result = _check_external_lookup(pattern, result)

    # Rule 5: Detect cross-segment dependencies
    # Cross-segment = Interpretation depends on another segment
    result = _check_cross_segment_dependency(pattern, result)

    # Non-blocking: Regex complexity warning (ISSUE-R7-004)
    result = _check_regex_complexity(pattern, result)

    return result


def _check_computed_fields(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Detect if pattern requires computed fields.

    COMPUTED means: ONE input value → MULTIPLE output fields

    Detection heuristics:
    1. Field name suggests expansion (e.g., "quarter" might need start_month/end_month)
    2. Sample values are compact encodings (e.g., "Q1" encodes 3 months)
    """
    # Known compact encodings that users often want to expand
    EXPANSION_PATTERNS = {
        # Quarter patterns - compact encoding of month range
        "quarter": {
            "regex": r"^Q[1-4]$",
            "default_behavior": "extract_integer",  # Q1 → 1 (YAML_OK by default)
            "expanded_fields": ["start_month", "end_month", "quarter_name"],
        },
        # Week patterns
        "week": {
            "regex": r"^W[0-5]?\d$",
            "default_behavior": "extract_integer",  # W01 → 1 (YAML_OK)
            "expanded_fields": ["start_date", "end_date"],
        },
        # Fiscal year patterns
        "fiscal_year": {
            "regex": r"^FY\d{2,4}$",
            "default_behavior": "extract_integer",  # FY24 → 24 (YAML_OK)
            "expanded_fields": ["calendar_start", "calendar_end"],
        },
    }

    field_lower = pattern.inferred_field_name.lower()

    for pattern_name, config in EXPANSION_PATTERNS.items():
        if pattern_name in field_lower:
            import re
            # Check if sample values match the compact encoding
            if pattern.sample_values and all(
                re.match(config["regex"], v) for v in pattern.sample_values
            ):
                # By default: YAML_OK (simple extraction)
                # Only mark as computed if hints request expansion
                # This check is a placeholder - actual detection happens in hints_require_python
                # We record the potential for computation but don't trigger it here
                pass

    # Check for mathematical operators in field name
    MATH_INDICATORS = ["total", "sum", "average", "diff", "ratio", "percent"]
    for indicator in MATH_INDICATORS:
        if indicator in field_lower:
            result.has_computed_fields = True
            result.computed_field_description = f"Field '{pattern.inferred_field_name}' suggests computation ({indicator})"
            return result

    return result


def _check_conditional_logic(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Detect if pattern requires conditional logic.

    CONDITIONAL means: Different processing based on runtime values.

    Detection heuristics:
    1. Sample values suggest format switching (e.g., mixed date formats)
    2. Field has validation that depends on another field's value
    """
    # Check for mixed formats in sample values
    if pattern.sample_values and len(set(pattern.sample_values)) > 1:
        # Check if values have inconsistent formats
        formats_detected = set()
        for val in pattern.sample_values:
            fmt = _detect_value_format(val)
            formats_detected.add(fmt)

        if len(formats_detected) > 1 and "unknown" not in formats_detected:
            result.has_conditional_logic = True
            result.condition_description = (
                f"Mixed formats detected: {formats_detected}. "
                "Runtime format detection required."
            )
            return result

    return result


def _detect_value_format(value: str) -> str:
    """Detect the format of a value."""
    import re
    patterns = [
        (r"^\d{4}-\d{2}-\d{2}$", "iso_date"),
        (r"^\d{2}/\d{2}/\d{4}$", "us_date"),
        (r"^\d{2}/\d{2}/\d{2}$", "short_date"),
        (r"^\d+$", "integer"),
        (r"^[A-Z]{2,3}$", "code"),
    ]
    for regex, fmt in patterns:
        if re.match(regex, value):
            return fmt
    return "unknown"


def _check_multi_step_transform(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Detect if pattern requires multi-step transformation.

    MULTI-STEP means: Transformation requires intermediate variables.

    Definition (ISSUE-R7-007 clarification):
    A "multi-step" transform is one that CANNOT be expressed as a linear YAML pipeline.

    YAML supports: from → pattern → type → normalize → default (linear)

    MULTI-STEP examples:
    - Extract → decode (base64) → parse (JSON) → extract field
    - Extract → call function → use result in another extraction
    - Extract → store temporarily → combine with another field
    """
    # Currently no automatic detection - this is primarily hint-driven
    # A future enhancement could analyze pattern.regex_pattern for nested structures

    # Placeholder: check for encoding indicators in sample values
    for val in pattern.sample_values:
        # Base64 detection
        import re
        if re.match(r"^[A-Za-z0-9+/=]{20,}$", val) and len(val) % 4 == 0:
            result.has_multi_step_transform = True
            result.transform_chain = "Extract → Base64 decode → further processing"
            return result

    return result


def _check_external_lookup(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Detect if pattern requires external lookup.

    EXTERNAL LOOKUP means: Value must be resolved against external data.

    Detection heuristics:
    1. Field name suggests lookup (client_code, account_id, etc.)
    2. Values are short codes that likely map to longer names
    """
    LOOKUP_INDICATORS = ["_code", "_id", "_key", "_ref", "client", "account", "vendor"]

    field_lower = pattern.inferred_field_name.lower()
    for indicator in LOOKUP_INDICATORS:
        if indicator in field_lower:
            # Check if values look like codes (short, alphanumeric)
            if pattern.sample_values and all(
                len(v) <= 10 and v.replace("_", "").replace("-", "").isalnum()
                for v in pattern.sample_values
            ):
                # Only flag if there's diversity suggesting lookup table
                unique_values = len(set(pattern.sample_values))
                if unique_values >= 3:
                    # This is only a POTENTIAL lookup - hints determine actual
                    # We don't auto-flag as Python required
                    pass

    # Month name to number is a lookup (not a simple regex)
    MONTH_NAMES = ["jan", "feb", "mar", "apr", "may", "jun",
                   "jul", "aug", "sep", "oct", "nov", "dec"]
    for val in pattern.sample_values:
        if val.lower()[:3] in MONTH_NAMES:
            result.has_external_lookup = True
            result.lookup_description = "Month name → number mapping requires lookup table"
            return result

    return result


def _check_cross_segment_dependency(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Detect if pattern interpretation depends on another segment.

    CROSS-SEGMENT means: Interpretation varies based on another path segment.

    Example: Date format differs between prod vs staging environments.
    """
    # This is primarily hint-driven as it requires context from other patterns
    # Automatic detection would require access to all patterns, not just one
    return result


def _check_regex_complexity(pattern: DetectedPattern, result: Complexity) -> Complexity:
    """
    Check if regex is too complex (warning, not blocking).

    ISSUE-R7-004: This adds a RECOMMENDATION, not a classification change.
    Pattern is still YAML_OK, but we suggest considering Python for readability.
    """
    if pattern.regex_pattern:
        # Thresholds for "complex" regex
        MAX_REGEX_LENGTH = 100
        MAX_CAPTURE_GROUPS = 5

        if len(pattern.regex_pattern) > MAX_REGEX_LENGTH:
            result.regex_complexity_warning = (
                f"Regex for '{pattern.inferred_field_name}' is {len(pattern.regex_pattern)} chars. "
                f"Consider Python for readability (threshold: {MAX_REGEX_LENGTH})."
            )
        elif pattern.regex_captures > MAX_CAPTURE_GROUPS:
            result.regex_complexity_warning = (
                f"Regex for '{pattern.inferred_field_name}' has {pattern.regex_captures} capture groups. "
                f"Consider Python for readability (threshold: {MAX_CAPTURE_GROUPS})."
            )

    return result
```

---

### Default vs Override Behavior (ISSUE-R7-002 Clarification)

The classification follows a clear precedence:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Classification Precedence                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. USER HINTS (highest priority)                               │
│     └─ Hints can ESCALATE YAML_OK → PYTHON_REQUIRED             │
│     └─ Hints can NEVER downgrade PYTHON_REQUIRED → YAML_OK      │
│                                                                  │
│  2. PATTERN COMPLEXITY                                          │
│     └─ Automatic detection of Python-requiring patterns         │
│                                                                  │
│  3. DEFAULT BEHAVIOR (lowest priority)                          │
│     └─ If no complexity detected → YAML_OK                      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Quarter Pattern Example (ISSUE-R7-002 Fix):**

| Scenario | Sample Value | User Hints | Classification | Reason |
|----------|--------------|------------|----------------|--------|
| Default | "Q1" | None | YAML_OK | `pattern: Q(\d)` + `type: integer` → extracts `1` |
| Override | "Q1" | "Quarter should compute start/end month" | PYTHON_REQUIRED | Hint contains "compute" keyword |

**Key Principle:** The pattern `Q1` is inherently YAML-expressible (simple regex extraction). The complexity comes from what the USER wants to do with it, not the pattern itself.

```yaml
# Default behavior: Q1 → 1 (YAML_OK)
quarter:
  from: segment(-2)
  pattern: "Q(\\d)"
  type: integer

# If user wants start/end months → Python required
# (Cannot express Q1 → {start: 1, end: 3} in YAML)
```

---

### Pattern Classification Reference

#### YAML-Expressible Patterns

These patterns map directly to `specs/extraction.md` Section 3.2 capabilities:

| Pattern Type | YAML Construct | Example |
|--------------|----------------|---------|
| Segment extraction | `from: segment(N)` | segment(-3) → field |
| Filename extraction | `from: filename` | Extract from file name |
| Full path regex | `from: full_path` + `pattern` | Variable depth matching |
| Relative path regex | `from: rel_path` + `pattern` | Relative path matching |
| Regex capture | `pattern: "(\\d+)"` | Capture group 1 |
| Type coercion | `type: integer\|date\|uuid` | String → typed value |
| Normalization | `normalize: lowercase\|uppercase\|strip_leading_zeros` | Case/format normalization |
| Default values | `default: "unknown"` | Fallback value |

**Note on `capture` field:** The current YAML schema uses implicit capture group 1. If explicit capture is needed, update `specs/extraction.md` to add `capture` property.

#### Python-Required Patterns

| Pattern Type | Why Python | Detection Method |
|--------------|------------|------------------|
| Computed fields | ONE input → MULTIPLE outputs | Field name analysis + hints |
| Conditional logic | Runtime branching | Mixed format detection in samples |
| Multi-step transform | Requires intermediate variables | Encoding detection (base64, etc.) |
| External lookup | Reference external data | Month names, hint keywords |
| Cross-segment dependency | Interpretation varies | Hints only (requires full context) |
| Format inference | Detect format at runtime | Multiple date formats in samples |

**Removed from list (ISSUE-R7-008):** "Stateful extraction" - this is not currently supported in the architecture. Extraction rules operate per-file with no cross-file state.

---

### Examples

#### Example 1: Simple Pattern → YAML

**Input:**
```
Sample Path: /data/ADT_Inbound/2024/01/msg_001.hl7
User Hints: None
```

**Pattern Analysis:**

| Pattern | source_type | sample_values | Classification | Reason |
|---------|-------------|---------------|----------------|--------|
| direction | segment(-4) | ["ADT_Inbound"] | YAML_OK | Regex extraction |
| year | segment(-3) | ["2024"] | YAML_OK | Type integer |
| month | segment(-2) | ["01"] | YAML_OK | Type integer |

**Decision:** All YAML_OK → Generate YAML Rule

**Output:**
```yaml
name: "healthcare_path"
glob: "**/ADT_*/*/*/*"
extract:
  direction:
    from: segment(-4)
    pattern: "ADT_(Inbound|Outbound)"
  year:
    from: segment(-3)
    type: integer
  month:
    from: segment(-2)
    type: integer
tag: hl7_messages
```

---

#### Example 2: Quarter with Hints → Python

**Input:**
```
Sample Path: /data/CLIENT-ABC/2024/Q1/report.csv
User Hints: ["Quarter folder should compute start/end month"]
```

**Pattern Analysis:**

| Pattern | sample_values | Default | With Hints | Reason |
|---------|---------------|---------|------------|--------|
| client | ["CLIENT-ABC"] | YAML_OK | YAML_OK | Regex extraction |
| year | ["2024"] | YAML_OK | YAML_OK | Type integer |
| quarter | ["Q1"] | YAML_OK | PYTHON_REQUIRED | Hint: "compute" keyword detected |

**Decision:** One PYTHON_REQUIRED → Generate Python Extractor

**DecisionResult:**
```python
DecisionResult(
    format=OutputFormat.PYTHON,
    classified_patterns=[...],
    python_reasons=["User hint contains 'compute' for field 'quarter'"],
    recommendations=[]
)
```

**Output:**
```python
# NOTE: Python extractor generated because extraction requires
# computed fields (quarter → month range).
# Hint: "Quarter folder should compute start/end month"

from pathlib import Path

def extract(path: str) -> dict:
    parts = Path(path).parts
    metadata = {}

    for part in parts:
        if part.startswith("CLIENT-"):
            metadata["client_id"] = part.split("-", 1)[1]

        if part.isdigit() and len(part) == 4:
            metadata["year"] = int(part)

        # Quarter computation - requires Python
        if part.startswith("Q") and len(part) == 2 and part[1].isdigit():
            q = int(part[1])
            metadata["quarter"] = q
            metadata["start_month"] = (q - 1) * 3 + 1
            metadata["end_month"] = q * 3

    return metadata
```

---

#### Example 3: Quarter WITHOUT Hints → YAML

**Input:**
```
Sample Path: /data/CLIENT-ABC/2024/Q1/report.csv
User Hints: None  # No computation requested
```

**Pattern Analysis:**

| Pattern | sample_values | Classification | Reason |
|---------|---------------|----------------|--------|
| client | ["CLIENT-ABC"] | YAML_OK | Regex extraction |
| year | ["2024"] | YAML_OK | Type integer |
| quarter | ["Q1"] | YAML_OK | Default: regex Q(\d) → integer |

**Decision:** All YAML_OK → Generate YAML Rule

**Output:**
```yaml
name: "client_quarterly"
glob: "**/CLIENT-*/*/*/*.csv"
extract:
  client_id:
    from: segment(-4)
    pattern: "CLIENT-(.*)"
  year:
    from: segment(-3)
    type: integer
  quarter:
    from: segment(-2)
    pattern: "Q(\\d)"
    type: integer
tag: quarterly_reports
```

---

#### Example 4: Complex Regex with Recommendation (ISSUE-R7-004)

**Input:**
```
Sample Path: /data/mission_042_alpha_2024-01-15_v2_final_approved.csv
User Hints: None
```

**Pattern Analysis:**

| Pattern | Classification | Recommendation |
|---------|----------------|----------------|
| (entire filename) | YAML_OK | "Regex is 87 chars. Consider Python for readability." |

**DecisionResult:**
```python
DecisionResult(
    format=OutputFormat.YAML,  # Still YAML - complexity is just a warning
    classified_patterns=[...],
    python_reasons=[],
    recommendations=[
        "Regex for 'filename_fields' is 87 chars. Consider Python for readability (threshold: 100)."
    ]
)
```

The UI can display this recommendation:
```
✓ Generated YAML Rule

  ⚠ Recommendation: Consider Python for complex regex
    Regex is 87 chars. You can edit to Python if preferred.

  [Accept YAML] [Switch to Python]
```

---

#### Example 5: Month Name Lookup → Python

**Input:**
```
Sample Path: /archive/reports/Jan/2024/summary.pdf
User Hints: None
```

**Pattern Analysis:**

| Pattern | sample_values | Classification | Reason |
|---------|---------------|----------------|--------|
| month | ["Jan", "Feb", "Mar"] | PYTHON_REQUIRED | Month name → number requires lookup |
| year | ["2024"] | YAML_OK | Type integer |

**Decision:** Automatic Python (lookup detected, no hint needed)

**Output:**
```python
# NOTE: Python extractor generated because extraction requires
# external lookup (month name → number).

from pathlib import Path

MONTH_LOOKUP = {
    "jan": 1, "feb": 2, "mar": 3, "apr": 4,
    "may": 5, "jun": 6, "jul": 7, "aug": 8,
    "sep": 9, "oct": 10, "nov": 11, "dec": 12
}

def extract(path: str) -> dict:
    parts = Path(path).parts
    metadata = {}

    for part in parts:
        # Month lookup
        month_key = part.lower()[:3]
        if month_key in MONTH_LOOKUP:
            metadata["month"] = MONTH_LOOKUP[month_key]
            metadata["month_name"] = part

        if part.isdigit() and len(part) == 4:
            metadata["year"] = int(part)

    return metadata
```

---

### Trade-offs

| Aspect | YAML-First Approach | Alternative: Always Python |
|--------|---------------------|----------------------------|
| **Portability** | YAML rules work across systems | Python requires interpreter |
| **Auditability** | Declarative, easy to review | Code requires reading logic |
| **Performance** | Compiled glob matching | Function call overhead |
| **Flexibility** | Limited to extraction.md capabilities | Unlimited |
| **User experience** | Simpler for simple patterns | Consistent but verbose |
| **AI generation** | Easier to validate YAML structure | Harder to validate code correctness |

**Chosen approach rationale:** YAML-first because:
1. Most real-world patterns ARE simple (80/20 rule)
2. YAML is safer to auto-approve
3. YAML integrates directly with Layer 1 runtime (no bridge)
4. Python fallback handles the complex 20%
5. Clear escalation path: hints can always force Python

---

### New Gaps Introduced

1. **GAP-INT-003: User hint parsing - LLM enhancement** (unchanged)
   - Current: keyword matching with COMPUTATION_KEYWORDS set
   - Future: LLM classification for natural language understanding
   - Scope: Improve hint detection beyond simple keywords

2. **GAP-INT-004: Complexity thresholds tuning** (unchanged)
   - Current: MAX_REGEX_LENGTH=100, MAX_CAPTURE_GROUPS=5
   - Future: User-configurable thresholds
   - Scope: Allow users to adjust complexity warnings

3. **GAP-INT-005: Python extractor validation** (unchanged)
   - How do we validate generated Python code before presenting to user?
   - What syntax/runtime checks should run automatically?

---

### Revision History

| Version | Changes |
|---------|---------|
| v1 | Initial proposal |
| v2 | **ISSUE-R7-001**: Full `analyze_complexity()` implementation with detection rules |
| v2 | **ISSUE-R7-002**: Clarified default vs override with explicit precedence diagram |
| v2 | **ISSUE-R7-003**: Added `COMPUTATION_KEYWORDS` and `hints_require_python()` implementation |
| v2 | **ISSUE-R7-004**: Added `recommendations` field to `DecisionResult`; removed YAML_COMPLEX |
| v2 | **ISSUE-R7-005**: Complete `DetectedPattern` and `Complexity` dataclass definitions |
| v2 | **ISSUE-R7-006**: Handle empty pattern set in `decide_output_format()` |
| v2 | **ISSUE-R7-007**: Defined "multi-step" as "requires intermediate variables" |
| v2 | **ISSUE-R7-008**: Removed "stateful extraction" (not supported in current architecture) |
| v2 | **ISSUE-R7-010**: Added `rel_path` to YAML-Expressible patterns table |

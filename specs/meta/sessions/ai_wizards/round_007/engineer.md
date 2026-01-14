# Engineer Round 007: GAP-INT-002

## Gap Resolution: GAP-INT-002

**Gap:** YAML vs Python output decision algorithm undefined
**Confidence:** HIGH

---

### Proposed Solution

The Pathfinder Wizard should use a **deterministic, pattern-classification decision tree** that examines each detected extraction pattern and classifies it as either YAML-expressible or Python-required. The output format is determined by the **most complex pattern** in the extraction set - if ANY pattern requires Python, the entire output is Python.

This approach:
1. Ensures consistent, predictable behavior
2. Maintains a single coherent output (not hybrid YAML + Python)
3. Can be explained to users ("Python required because: quarter computation")

---

### Decision Tree

```
START: Analyze all detected extraction patterns
       │
       ▼
┌──────────────────────────────────────────────────────────────────┐
│ For each pattern P detected from sample path(s):                 │
│                                                                  │
│   1. Classify P as YAML_OK or PYTHON_REQUIRED                    │
│   2. If PYTHON_REQUIRED, record reason                           │
│                                                                  │
│ Output decision:                                                 │
│   IF all patterns are YAML_OK → Generate YAML Extraction Rule    │
│   IF any pattern is PYTHON_REQUIRED → Generate Python Extractor  │
└──────────────────────────────────────────────────────────────────┘
```

**Detailed Classification Algorithm:**

```python
class PatternClassification(Enum):
    YAML_OK = "yaml_ok"
    PYTHON_REQUIRED = "python_required"

@dataclass
class ClassifiedPattern:
    pattern: DetectedPattern
    classification: PatternClassification
    python_reason: Optional[str]  # Why Python is needed (if applicable)

def classify_pattern(pattern: DetectedPattern, user_hints: List[str]) -> ClassifiedPattern:
    """
    Classify a single detected pattern as YAML-expressible or Python-required.

    YAML-expressible patterns:
    - Direct segment extraction: segment(N) → field
    - Regex capture from segment: segment(N) + pattern → field
    - Full path regex: full_path + pattern → field (for variable depth)
    - Type coercion: integer, date, uuid
    - Normalization: lowercase, uppercase, strip_leading_zeros
    - Default values: static fallback if extraction fails

    Python-required patterns:
    - Computed fields (derived from other extracted values)
    - Conditional logic (if X then Y else Z)
    - Multi-step transformations (extract → transform → transform)
    - External lookups (reference tables, files, APIs)
    - Cross-segment dependencies (value in segment A affects segment B)
    - Stateful extraction (depends on previous files)
    """

    # Check for user hints that force Python
    if hints_require_python(pattern, user_hints):
        return ClassifiedPattern(
            pattern=pattern,
            classification=PatternClassification.PYTHON_REQUIRED,
            python_reason=get_hint_python_reason(pattern, user_hints)
        )

    # Check pattern complexity
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

    # Pattern is YAML-expressible
    return ClassifiedPattern(
        pattern=pattern,
        classification=PatternClassification.YAML_OK,
        python_reason=None
    )

def decide_output_format(
    patterns: List[DetectedPattern],
    user_hints: List[str]
) -> Tuple[OutputFormat, List[ClassifiedPattern]]:
    """
    Main decision function: Analyze all patterns and decide output format.
    """
    classified = [classify_pattern(p, user_hints) for p in patterns]

    python_patterns = [c for c in classified
                       if c.classification == PatternClassification.PYTHON_REQUIRED]

    if python_patterns:
        # Any Python-required pattern → entire output is Python
        reasons = [c.python_reason for c in python_patterns]
        return (OutputFormat.PYTHON, classified)
    else:
        return (OutputFormat.YAML, classified)
```

---

### Pattern Classification

#### YAML-Expressible Patterns (from specs/extraction.md Section 3)

| Pattern Type | YAML Capability | Example |
|--------------|-----------------|---------|
| **Segment extraction** | `from: segment(N)` | Extract folder name at position -3 |
| **Regex capture** | `pattern: "mission_(\\d+)"` | Capture numeric ID from `mission_042` |
| **Full path regex** | `from: full_path` with pattern | Variable depth patterns like `**/mission_*/**` |
| **Type coercion** | `type: integer\|date\|uuid` | Convert "2024" to integer 2024 |
| **Normalization** | `normalize: lowercase\|uppercase\|strip_leading_zeros` | "Inbound" → "inbound" |
| **Default values** | `default: "unknown"` | Fallback if extraction fails |
| **Literal matching** | Fixed segment values | "ADT_Inbound" matches exactly |

#### Python-Required Patterns (NOT in extraction.md)

| Pattern Type | Why Python Required | Example |
|--------------|---------------------|---------|
| **Computed fields** | Output depends on arithmetic/logic on extracted values | `Q1` → `start_month=1, end_month=3` |
| **Conditional assignment** | Different output based on runtime conditions | `if year < 2020: legacy = True` |
| **Multi-step transform** | Chain of transformations | Extract → decode base64 → parse JSON |
| **External lookups** | Reference external data | `client_ABC` → lookup full name in CSV |
| **Cross-segment dependencies** | One field affects interpretation of another | If `env=prod`, parse date differently |
| **Stateful extraction** | Depends on context from other files | Sequence numbers across files |
| **Format inference** | Must detect format at runtime | Auto-detect date format per file |
| **Aggregation** | Combine multiple segments | Join path segments into single field |

---

### Edge Cases

#### Edge Case 1: User Hint Forces Python

**Scenario:** User provides hint that implies computation.

```
Sample Path: /data/CLIENT-ABC/2024/Q1/report.csv
User Hint: "Quarter folder should compute start/end month"
```

**Detection:** Parse user hints for keywords indicating computation:
- "compute", "calculate", "derive"
- "start/end", "range", "convert to"
- "lookup", "map to", "translate"
- "if...then", "when...otherwise"

**Decision:** PYTHON_REQUIRED with reason: "User hint requires computed fields (quarter → month range)"

#### Edge Case 2: Ambiguous Pattern (Could Be Either)

**Scenario:** Pattern looks simple but user might want derived fields.

```
Sample Path: /data/sales/2024-Q1/transactions.csv
Detected: year=2024, quarter=Q1
```

**Decision:** Default to YAML. Show preview:
```
Extracted: { year: 2024, quarter: "Q1" }
```

If user wants `start_month`/`end_month`, they press `h` (hint) and add:
"Also extract start and end month from quarter"

This triggers reclassification → PYTHON_REQUIRED.

#### Edge Case 3: Type Coercion vs Computation

**Scenario:** Distinguishing type coercion (YAML-OK) from computation (Python).

| Input | Desired Output | Classification | Reason |
|-------|----------------|----------------|--------|
| `"2024"` | `2024` (int) | YAML_OK | Simple type coercion |
| `"Q1"` | `1` (quarter number) | YAML_OK | Regex `Q(\d)` + type integer |
| `"Q1"` | `{start: 1, end: 3}` | PYTHON | Computed from input |
| `"jan"` | `1` (month number) | PYTHON | Lookup table required |
| `"2024-01"` | `"January 2024"` | PYTHON | Format transformation |

#### Edge Case 4: Complex Regex vs Simple Python

**Scenario:** Very complex regex could achieve YAML, but Python is clearer.

```
Pattern: mission_042_alpha_2024-01-15_v2_final.csv
Fields needed: mission_id, variant, date, version
```

**Decision:** If regex becomes unreadable (>100 chars or >5 capture groups), recommend Python with comment explaining why. But still classify as YAML_OK - user can choose to accept complex YAML or edit to Python.

#### Edge Case 5: Variable Depth with Extraction

**Scenario:** `**` in glob means segment index is unpredictable.

```
Glob: **/mission_*/2024/**/*.csv
```

**Decision:** YAML_OK - use `from: full_path` with regex instead of `segment(N)`:
```yaml
extract:
  mission_id:
    from: full_path
    pattern: "/mission_([^/]+)/"  # Works regardless of depth
```

#### Edge Case 6: Empty Patterns (Tag-Only Rules)

**Scenario:** Rule only tags, no extraction.

```
Sample: /data/logs/*.log
User: "Just tag these as log_files"
```

**Decision:** YAML_OK (trivially). Generate tag-only rule:
```yaml
glob: "**/logs/*.log"
extract: null
tag: log_files
```

---

### Examples

#### Example 1: Simple Pattern → YAML

**Input:**
```
Sample Path: /data/ADT_Inbound/2024/01/msg_001.hl7
User Hints: None
```

**Pattern Analysis:**
| Segment | Value | Classification | Reason |
|---------|-------|----------------|--------|
| segment(-4) | ADT_Inbound | YAML_OK | Regex `ADT_(Inbound\|Outbound)` |
| segment(-3) | 2024 | YAML_OK | Type integer, validate 1900..2100 |
| segment(-2) | 01 | YAML_OK | Type integer, validate 1..12 |
| filename | msg_001.hl7 | SKIP | Too specific (unique per file) |

**Decision:** All YAML_OK → Generate YAML Rule

**Output:**
```yaml
name: "healthcare_path"
glob: "**/ADT_*/*/*/*"
extract:
  direction:
    from: segment(-4)
    pattern: "ADT_(Inbound|Outbound)"
    capture: 1
  year:
    from: segment(-3)
    type: integer
    validate: "1900..2100"
  month:
    from: segment(-2)
    type: integer
    validate: "1..12"
tag: hl7_messages
```

#### Example 2: Computed Fields → Python

**Input:**
```
Sample Path: /data/CLIENT-ABC/2024/Q1/report.csv
User Hints: ["Quarter folder should compute start/end month"]
```

**Pattern Analysis:**
| Segment | Value | Classification | Reason |
|---------|-------|----------------|--------|
| segment(-4) | CLIENT-ABC | YAML_OK | Regex `CLIENT-(.*)` |
| segment(-3) | 2024 | YAML_OK | Type integer |
| segment(-2) | Q1 | PYTHON_REQUIRED | User hint: compute start/end month |

**Decision:** One PYTHON_REQUIRED → Generate Python Extractor

**Output:**
```python
# NOTE: Python extractor generated because extraction requires
# computed fields (quarter → month range) not expressible
# in declarative YAML rules.

from pathlib import Path

def extract(path: str) -> dict:
    parts = Path(path).parts
    metadata = {}

    for part in parts:
        if part.startswith("CLIENT-"):
            metadata["client_id"] = part.split("-")[1]

        if part.isdigit() and len(part) == 4:
            metadata["year"] = int(part)

        # Quarter computation - requires Python
        if part.startswith("Q") and len(part) == 2:
            q = int(part[1])
            metadata["quarter"] = q
            metadata["start_month"] = (q - 1) * 3 + 1
            metadata["end_month"] = q * 3

    return metadata
```

#### Example 3: External Lookup → Python

**Input:**
```
Sample Path: /archive/clients/acme_corp/invoices/2024/inv_001.pdf
User Hints: ["Lookup client full name from clients.csv"]
```

**Pattern Analysis:**
| Segment | Value | Classification | Reason |
|---------|-------|----------------|--------|
| segment(-4) | acme_corp | PYTHON_REQUIRED | User hint: external lookup |
| segment(-3) | invoices | YAML_OK | Fixed literal |
| segment(-2) | 2024 | YAML_OK | Type integer |

**Decision:** External lookup → Generate Python Extractor

**Output:**
```python
# NOTE: Python extractor generated because extraction requires
# external lookup (client code → full name) not expressible
# in declarative YAML rules.

import csv
from pathlib import Path
from functools import lru_cache

@lru_cache(maxsize=1)
def load_client_lookup():
    lookup = {}
    with open('/path/to/clients.csv', 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            lookup[row['code']] = row['full_name']
    return lookup

def extract(path: str) -> dict:
    parts = Path(path).parts
    metadata = {}

    for part in parts:
        if part in ['invoices', 'contracts', 'correspondence']:
            metadata["doc_type"] = part

        if part.isdigit() and len(part) == 4:
            metadata["year"] = int(part)

    # Client lookup - requires Python
    client_lookup = load_client_lookup()
    for part in parts:
        if part in client_lookup:
            metadata["client_code"] = part
            metadata["client_name"] = client_lookup[part]
            break

    return metadata
```

#### Example 4: Variable Depth with Full Path Regex → YAML

**Input:**
```
Sample Paths:
  /data/mission_042/2024-01-15/readings.csv
  /archive/old/mission_043/2024-01-16/data.csv  (different depth)
```

**Pattern Analysis:**
| Pattern | Classification | Reason |
|---------|----------------|--------|
| mission_id | YAML_OK | `from: full_path`, pattern `/mission_([^/]+)/` |
| date | YAML_OK | `from: full_path`, pattern `/(\\d{4}-\\d{2}-\\d{2})/` |

**Decision:** Variable depth handled by full_path regex → YAML Rule

**Output:**
```yaml
name: "mission_data"
glob: "**/mission_*/????-??-??/*.csv"
extract:
  mission_id:
    from: full_path
    pattern: "/mission_([^/]+)/"
  date:
    from: full_path
    pattern: "/(\\d{4}-\\d{2}-\\d{2})/"
    type: date
tag: mission_data
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

---

### New Gaps Introduced

1. **GAP-INT-003: User hint parsing algorithm undefined**
   - How exactly do we detect computation keywords in natural language hints?
   - Should this use simple keyword matching or LLM classification?

2. **GAP-INT-004: Complexity thresholds undefined**
   - At what point is a regex "too complex" and Python recommended?
   - Should we warn about >N capture groups or >M characters?

3. **GAP-INT-005: Python extractor validation**
   - How do we validate generated Python code before presenting to user?
   - What syntax/runtime checks should run automatically?

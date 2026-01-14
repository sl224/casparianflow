# Round 016 - Engineer Output

**Gap:** GAP-INT-003 - User hint parsing - LLM enhancement needed
**Priority:** HIGH
**Engineer:** Claude Opus 4.5
**Date:** 2026-01-13

---

## Proposed Specification: Section 3.6 User Hint System

The following content is designed to be inserted into `specs/ai_wizards.md` after Section 3.5 (Path Intelligence Engine) and before Section 4 (Draft Lifecycle).

---

### 3.6 User Hint System

**Purpose:** Enable users to guide AI wizards with natural language hints, improving extraction accuracy and reducing iteration cycles.

> **Core Principle:** Hints are *guidance*, not commands. The AI may incorporate, reinterpret, or respectfully decline hints that conflict with detected patterns or data quality requirements.

#### 3.6.1 Overview

User hints appear in multiple wizards:
- **Pathfinder Wizard** (Section 3.1): Guide path segment interpretation
- **Parser Wizard** (Section 3.2): Specify column formats, transformations
- **Labeling Wizard** (Section 3.3): Provide domain context for label suggestions
- **Semantic Path Wizard** (Section 3.4): Clarify folder semantics

The User Hint System provides a unified framework for:
1. Accepting hints in multiple input modes
2. Parsing and validating hints
3. Integrating hints into LLM prompts
4. Providing feedback on hint interpretation
5. Persisting and reusing successful hints

#### 3.6.2 Hint Input Modes

**Mode 1: Free-Form Natural Language (Default)**

Users type hints in plain English. This is the primary mode.

```
Examples:
  "the second folder is always the mission name"
  "Column 3 is a date in DD/MM/YYYY format"
  "ignore lines starting with #"
  "the 'amt' column should be named 'amount'"
  "quarter folder should compute start and end months"
```

**Mode 2: Structured Hints (Power Users)**

For precise control, hints can use a structured syntax:

| Syntax | Meaning | Example |
|--------|---------|---------|
| `segment(N) = field_name` | Assign segment to field | `segment(-3) = mission_id` |
| `segment(N) : type` | Specify segment type | `segment(-2) : date_iso` |
| `column("name") : type` | Specify column type | `column("txn_date") : date_dmy` |
| `column("name") -> new_name` | Rename column | `column("amt") -> amount` |
| `field = value` | Force literal value | `department = "sales"` |
| `skip pattern` | Ignore matching lines | `skip "^#"` |
| `compute field from expr` | Derive field | `compute end_month from quarter * 3` |

**Mode 3: Hint Templates (Reusable Patterns)**

Pre-defined hint templates for common scenarios:

```yaml
# ~/.casparian_flow/hint_templates.yaml

templates:
  quarter_expansion:
    description: "Expand quarter (Q1-Q4) to month range"
    hint: "compute start_month from (quarter - 1) * 3 + 1, end_month from quarter * 3"
    applies_to: ["pathfinder", "parser"]

  european_dates:
    description: "Date columns use DD/MM/YYYY format"
    hint: "all date columns use DD/MM/YYYY format, not MM/DD/YYYY"
    applies_to: ["parser"]

  client_prefix:
    description: "CLIENT-XXX pattern for client identifier"
    hint: "segment matching CLIENT-* extracts client_id from suffix"
    applies_to: ["pathfinder", "semantic_path"]
```

**Template Invocation:**
```
User types: @quarter_expansion
Expands to: "compute start_month from (quarter - 1) * 3 + 1, end_month from quarter * 3"
```

#### 3.6.3 Hint Parsing Pipeline

Hints flow through a three-stage pipeline before reaching the LLM:

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                           HINT PARSING PIPELINE                                │
│                                                                                │
│  Raw Hint Input                                                                │
│       │                                                                        │
│       ▼                                                                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ STAGE 1: INTENT EXTRACTION                                              │   │
│  │                                                                         │   │
│  │  • Classify hint category (format, naming, computation, filter, etc.)   │   │
│  │  • Detect primary action (assign, rename, skip, compute, validate)      │   │
│  │  • Extract confidence level (certain vs speculative)                    │   │
│  └───────────────────────────────────────────────────────────────┬─────────┘   │
│                                                                  │              │
│                                                                  ▼              │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ STAGE 2: KEYWORD CLASSIFICATION (Escalation Check)                      │   │
│  │                                                                         │   │
│  │  Computation keywords → PYTHON_REQUIRED (escalation)                    │   │
│  │  • compute, calculate, derive, formula                                  │   │
│  │  • start/end, range, convert to, expand                                 │   │
│  │  • lookup, map to, translate, reference                                 │   │
│  │  • if, when, otherwise, conditional                                     │   │
│  │  • combine, merge, join, aggregate                                      │   │
│  │                                                                         │   │
│  │  Non-escalating keywords → YAML_OK                                      │   │
│  │  • format, type, name, rename, ignore, skip                             │   │
│  │  • extract, capture, segment, column                                    │   │
│  └───────────────────────────────────────────────────────────────┬─────────┘   │
│                                                                  │              │
│                                                                  ▼              │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ STAGE 3: ENTITY EXTRACTION                                              │   │
│  │                                                                         │   │
│  │  • Segment references: "second folder", "segment -3", "last directory"  │   │
│  │  • Field names: "mission_id", "the client column", "column 3"           │   │
│  │  • Type specifications: "date", "integer", "DD/MM/YYYY", "ISO format"   │   │
│  │  • Patterns: regex patterns, prefix/suffix patterns                     │   │
│  │  • Values: literal values to assign or match                            │   │
│  └───────────────────────────────────────────────────────────────┬─────────┘   │
│                                                                  │              │
│                                                                  ▼              │
│                        Structured Hint Object                                   │
└────────────────────────────────────────────────────────────────────────────────┘
```

**Stage 1: Intent Extraction**

| Category | Keywords/Patterns | Example Hints |
|----------|-------------------|---------------|
| FORMAT | "format", "DD/MM", "ISO", "european" | "date uses European format" |
| NAMING | "name", "rename", "call it", "should be" | "column 'amt' should be 'amount'" |
| COMPUTATION | compute keywords (see Stage 2) | "derive month from quarter" |
| FILTER | "ignore", "skip", "exclude", "only" | "ignore lines starting with #" |
| STRUCTURE | "segment", "folder", "column", "field" | "second folder is mission name" |
| TYPE | "type", "integer", "string", "boolean" | "column 3 is an integer" |

**Stage 2: Keyword Classification**

The parser scans for escalation keywords. If found, the hint triggers `PYTHON_REQUIRED` classification for the entire wizard output (per Section 3.1.1).

```python
ESCALATION_KEYWORDS = {
    # Computation
    "compute", "calculate", "derive", "formula", "generate",
    # Range expansion
    "start", "end", "range", "expand", "convert to",
    # Lookups
    "lookup", "map to", "translate", "reference", "cross-reference",
    # Conditionals
    "if", "when", "otherwise", "conditional", "depends on",
    # Aggregation
    "combine", "merge", "join", "aggregate", "concatenate"
}

def check_escalation(hint: str) -> bool:
    hint_lower = hint.lower()
    return any(kw in hint_lower for kw in ESCALATION_KEYWORDS)
```

**Stage 3: Entity Extraction**

Entities are extracted using pattern matching and NLP:

| Entity Type | Detection Patterns | Normalization |
|-------------|-------------------|---------------|
| Segment reference | "segment N", "folder N", "Nth directory", "-N" (negative) | `segment(N)` |
| Ordinal reference | "first", "second", "third", "last", "second-to-last" | `segment(1)`, `segment(-1)` |
| Column reference | "column N", "column 'name'", "the X column" | `column(N)` or `column("name")` |
| Type specification | "date", "integer", "string", "DD/MM/YYYY" | Enum: `date_dmy`, `integer`, etc. |
| Pattern | Regex literals, "pattern X", "starts with", "ends with" | Regex string |

**Entity Normalization Examples:**

| Raw Hint | Extracted Entities |
|----------|-------------------|
| "the second folder is the mission name" | `{segment: 2, field_name: "mission_name"}` |
| "segment -3 contains the client ID" | `{segment: -3, field_name: "client_id"}` |
| "column 'txn_date' uses DD/MM/YYYY" | `{column: "txn_date", type: "date_dmy"}` |
| "last directory is always 'archive'" | `{segment: -1, literal: "archive"}` |

#### 3.6.4 LLM Prompt Integration

Hints are formatted and injected into LLM prompts using a structured template system.

**Prompt Structure:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         LLM PROMPT STRUCTURE                                │
│                                                                             │
│  1. SYSTEM CONTEXT (fixed per wizard)                                       │
│     └─ Wizard role, output format requirements                              │
│                                                                             │
│  2. SAMPLE DATA (from user input)                                           │
│     └─ Paths, file contents, headers, sample values                         │
│                                                                             │
│  3. DETECTED PATTERNS (from algorithmic analysis)                           │
│     └─ Segments, types, patterns already identified                         │
│                                                                             │
│  4. USER HINTS (from hint pipeline) ◄── INJECTION POINT                     │
│     └─ Structured hint objects, original text                               │
│                                                                             │
│  5. CONFLICT RESOLUTION INSTRUCTIONS                                        │
│     └─ How to handle hint vs detection conflicts                            │
│                                                                             │
│  6. OUTPUT FORMAT REQUIREMENTS                                              │
│     └─ JSON schema, YAML structure, code template                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Hint Injection Template:**

```
## User Hints

The user has provided the following guidance. Incorporate these hints while
maintaining data integrity. If a hint conflicts with detected patterns,
prefer the hint unless it would cause data loss or type errors.

### Structured Hints:
{structured_hints_json}

### Original User Text:
"{original_hint_text}"

### Hint Interpretation:
- Intent: {detected_intent}
- Escalation: {escalation_status}
- Entities: {extracted_entities}

### Conflict Resolution Priority:
1. Data integrity (never lose or corrupt data)
2. User hints (when they don't violate #1)
3. Detected patterns (as fallback)
```

**Example Prompts:**

**Pathfinder with Hint:**

```
You are generating a YAML extraction rule for file paths.

## Sample Paths:
/data/CLIENT-ACME/invoices/2024/Q1/inv_001.pdf
/data/CLIENT-GLOBEX/invoices/2024/Q2/inv_002.pdf

## Detected Patterns:
- segment(-5): Variable, prefix "CLIENT-"
- segment(-4): Fixed "invoices"
- segment(-3): 4-digit year
- segment(-2): Quarter pattern Q1-Q4
- filename: Pattern "inv_NNN.pdf"

## User Hints:

The user has provided the following guidance:

### Structured Hints:
{
  "intent": "COMPUTATION",
  "escalation": true,
  "entities": [
    {"type": "segment", "value": -2, "target_field": "quarter"},
    {"type": "derived", "source": "quarter", "targets": ["start_month", "end_month"]}
  ]
}

### Original User Text:
"quarter folder should compute start and end months"

### Hint Interpretation:
- Intent: COMPUTATION (derive new fields from existing)
- Escalation: PYTHON_REQUIRED (contains "compute")
- Entities: segment(-2) → quarter, derive start_month, end_month

### Conflict Resolution Priority:
1. Data integrity
2. User hints
3. Detected patterns

Because the hint requires computation, generate a Python extractor instead of YAML.
The extractor should extract quarter from segment(-2) and compute:
- start_month = (quarter - 1) * 3 + 1
- end_month = quarter * 3

## Output Format:
Generate a Python extractor function with full implementation.
```

**Parser Wizard with Hint:**

```
You are generating a Python parser class for CSV files.

## Sample Content (first 5 rows):
id,txn_date,amount,customer
1001,15/01/2024,100.50,john@example.com
1002,22/01/2024,250.00,jane@example.com

## Detected Schema:
- id: integer
- txn_date: string (ambiguous date format)
- amount: float
- customer: string (email)

## User Hints:

### Structured Hints:
{
  "intent": "FORMAT",
  "escalation": false,
  "entities": [
    {"type": "column", "name": "txn_date", "format": "date_dmy"}
  ]
}

### Original User Text:
"txn_date column uses DD/MM/YYYY format, not American"

### Hint Interpretation:
- Intent: FORMAT (specify date format)
- Escalation: YAML_OK (no computation)
- Entities: column "txn_date" → date, format DD/MM/YYYY

### Conflict Resolution:
The hint resolves the date format ambiguity. Use DD/MM/YYYY parsing.

## Output Format:
Generate a Python parser class with proper date parsing.
```

**Conflict Resolution Rules:**

| Scenario | Resolution | Example |
|----------|------------|---------|
| Hint specifies type, detection agrees | Use specified type | "txn_date is a date" + detection: date → date |
| Hint specifies type, detection disagrees | Prefer hint with warning | "column 3 is integer" but detection sees floats → integer with warning |
| Hint specifies naming, no conflict | Use hint naming | "call it mission_id" → field name = mission_id |
| Hint contradicts sample data | Reject hint, show feedback | "segment 5 is year" but path has 3 segments → error |
| Hint requests impossible computation | Show limitation, suggest alternative | "lookup from database" → "External lookups require manual configuration" |

#### 3.6.5 Hint Validation and Feedback

Before processing, hints are validated. Invalid or ambiguous hints trigger user feedback.

**Validation Checks:**

| Check | Failure Condition | Feedback |
|-------|-------------------|----------|
| Segment bounds | Reference exceeds path depth | "Segment -5 referenced, but path only has 4 segments. Did you mean -4?" |
| Column existence | Column name not found | "Column 'transaction_date' not found. Available: txn_date, trans_date. Did you mean 'txn_date'?" |
| Type compatibility | Requested type incompatible with values | "Cannot parse '15/01/2024' as integer. Did you mean date?" |
| Ambiguous reference | Multiple interpretations possible | "Segment 'second folder' is ambiguous. The path has: data (1), client (2), year (3). Which did you mean?" |
| Conflicting hints | Multiple hints conflict | "Hints conflict: 'segment -2 is date' vs 'segment -2 is quarter'. Please clarify." |

**Feedback Dialog (TUI):**

```
┌─ HINT CLARIFICATION NEEDED ────────────────────────────────────────────────┐
│                                                                             │
│  Your hint: "the second folder is the mission name"                        │
│                                                                             │
│  ⚠ Ambiguous Reference                                                     │
│                                                                             │
│  Path: /data/missions/mission_042/2024-01-15/readings.csv                  │
│                                                                             │
│  "Second folder" could mean:                                                │
│    [1] "missions"    (segment 2 from root)                                 │
│    [2] "mission_042" (segment -4 from file)                                │
│    [3] "2024-01-15"  (segment -2, second-to-last)                          │
│                                                                             │
│  Select interpretation: [1] [2] [3]                                        │
│                                                                             │
│  Or rephrase: > ___________________________________________________        │
│                                                                             │
│  [Enter] Confirm   [Esc] Cancel                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Confidence Scoring:**

Each hint interpretation receives a confidence score:

| Score Range | Label | Behavior |
|-------------|-------|----------|
| 90-100% | HIGH | Process immediately |
| 70-89% | MEDIUM | Process with inline confirmation |
| 50-69% | LOW | Show clarification dialog |
| <50% | AMBIGUOUS | Require explicit disambiguation |

**Confidence Factors:**

| Factor | Weight | Example |
|--------|--------|---------|
| Exact entity match | +30% | "segment -3" vs "third folder" |
| Type keyword present | +20% | "date format" vs "the date thing" |
| Single interpretation | +25% | Only one possible meaning |
| Domain keyword match | +15% | "mission_id" in defense context |
| Structured syntax used | +10% | `segment(-3) = mission_id` |

#### 3.6.6 Hint Persistence

Successful hints are stored for reuse across similar patterns.

**Storage Schema:**

```sql
-- Add to ~/.casparian_flow/casparian_flow.sqlite3

CREATE TABLE hint_history (
    id TEXT PRIMARY KEY,
    wizard_type TEXT NOT NULL,              -- pathfinder, parser, labeling, semantic_path
    original_text TEXT NOT NULL,            -- Raw user input
    structured_json TEXT NOT NULL,          -- Parsed hint object
    context_hash TEXT NOT NULL,             -- Hash of sample data context
    success_count INTEGER DEFAULT 1,        -- Times hint led to approval
    failure_count INTEGER DEFAULT 0,        -- Times hint was rejected/modified
    created_at INTEGER NOT NULL,            -- Unix millis
    last_used_at INTEGER NOT NULL           -- Unix millis
);

CREATE INDEX idx_hint_wizard ON hint_history(wizard_type);
CREATE INDEX idx_hint_context ON hint_history(context_hash);

CREATE TABLE hint_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,              -- Template identifier
    description TEXT NOT NULL,              -- Human-readable description
    hint_text TEXT NOT NULL,                -- The hint template text
    applies_to TEXT NOT NULL,               -- JSON array of wizard types
    is_builtin INTEGER DEFAULT 0,           -- 1 = shipped with app
    usage_count INTEGER DEFAULT 0,          -- Times used
    created_at INTEGER NOT NULL
);

CREATE TABLE hint_suggestions (
    id TEXT PRIMARY KEY,
    history_id TEXT NOT NULL REFERENCES hint_history(id),
    trigger_pattern TEXT NOT NULL,          -- Context pattern that triggers suggestion
    priority INTEGER DEFAULT 50,            -- Higher = more likely to suggest
    expires_at INTEGER                      -- Optional expiration (Unix millis)
);

CREATE INDEX idx_suggestion_trigger ON hint_suggestions(trigger_pattern);
```

**Hint Reuse Flow:**

```
1. User invokes wizard on new sample
                    │
                    ▼
2. System computes context hash (sample structure)
                    │
                    ▼
3. Query hint_history for matching context_hash
                    │
        ┌───────────┴───────────┐
        │                       │
        ▼                       ▼
    Matches found           No matches
        │                       │
        ▼                       ▼
4. Rank by success_count    Continue with
   and recency              fresh analysis
        │
        ▼
5. Suggest: "Previously you said: '{hint}'. Use again? [Y/n]"
        │
        ▼
6. If accepted, pre-populate hint field
```

**Context Hash Computation:**

```python
def compute_context_hash(wizard_type: str, sample_data: dict) -> str:
    """
    Generate a hash representing the structural context of the sample.
    This enables hint reuse across similar (but not identical) samples.
    """
    if wizard_type == "pathfinder":
        # Hash path structure, not literal values
        structure = {
            "depth": sample_data["path_depth"],
            "segment_patterns": [classify_segment(s) for s in sample_data["segments"]],
            "extension": sample_data.get("extension")
        }
    elif wizard_type == "parser":
        # Hash column structure
        structure = {
            "columns": list(sample_data["headers"]),
            "detected_types": sample_data["inferred_types"]
        }
    elif wizard_type == "labeling":
        # Hash signature
        structure = {
            "signature_hash": sample_data["signature_hash"]
        }
    elif wizard_type == "semantic_path":
        # Hash semantic structure
        structure = {
            "primitives": sample_data["detected_primitives"]
        }

    return hashlib.blake3(json.dumps(structure, sort_keys=True).encode()).hexdigest()[:16]
```

**Hint Inheritance Hierarchy:**

Hints can be defined at multiple levels with inheritance:

```
┌─────────────────────────────────────────────────────────────────┐
│                    HINT INHERITANCE HIERARCHY                    │
│                                                                  │
│  Level 1: Global Templates (lowest priority)                     │
│    └─ Shipped with app, community-contributed                    │
│    └─ e.g., @quarter_expansion, @european_dates                  │
│                                                                  │
│  Level 2: User Templates                                         │
│    └─ User-defined in hint_templates table                       │
│    └─ e.g., @my_client_pattern                                   │
│                                                                  │
│  Level 3: Source-Level Hints                                     │
│    └─ Attached to a specific source                              │
│    └─ Apply to all files from that source                        │
│    └─ e.g., "All files from /data/healthcare use DD/MM/YYYY"     │
│                                                                  │
│  Level 4: File-Level Hints (highest priority)                    │
│    └─ Specific to current file/sample                            │
│    └─ Provided in wizard dialog                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Source-Level Hint Storage:**

```sql
CREATE TABLE source_hints (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    hint_text TEXT NOT NULL,
    wizard_type TEXT,                       -- NULL = applies to all wizards
    priority INTEGER DEFAULT 50,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_source_hints ON source_hints(source_id);
```

**Hint Merging Rules:**

When multiple hints apply:

| Scenario | Resolution |
|----------|------------|
| Same field, same type | Use highest priority (file > source > template) |
| Same field, different types | Conflict error, require disambiguation |
| Different fields | Merge all hints |
| Hint + no-hint default | Hint overrides default |

#### 3.6.7 TUI Integration

**Hint Input States:**

Per Section 5.4, pressing `h` opens the hint dialog. This section extends that specification:

```
┌─ PROVIDE HINT ──────────────────────────────────────────────────────────────┐
│                                                                              │
│  Current context: /data/CLIENT-ACME/invoices/2024/Q1/inv_001.pdf            │
│                                                                              │
│  ┌─ Suggested Hints (from history) ────────────────────────────────────┐    │
│  │  [1] "CLIENT-* extracts client_id from suffix" (used 5 times)       │    │
│  │  [2] "quarter folder computes start/end month" (used 3 times)       │    │
│  │  [Tab] to select, or type new hint below                            │    │
│  └──────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  > The second folder contains the client identifier_____________________    │
│                                                                              │
│  ┌─ Hint Preview ───────────────────────────────────────────────────────┐   │
│  │  Intent: STRUCTURE                                                    │   │
│  │  Escalation: No (YAML OK)                                            │   │
│  │  Entities:                                                            │   │
│  │    • segment(2) → field: client_identifier                           │   │
│  │  Confidence: ████████░░ 82%                                          │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  Templates: @quarter_expansion  @european_dates  @client_prefix             │
│                                                                              │
│  [Enter] Submit   [Tab] Select suggestion   [Esc] Cancel                    │
└──────────────────────────────────────────────────────────────────────────────┘
```

**Real-Time Hint Preview:**

As the user types, the hint is parsed in real-time and a preview is shown:

| User Input | Preview |
|------------|---------|
| "the sec" | `Parsing...` |
| "the second folder" | `segment(2) detected, awaiting field name` |
| "the second folder is the mission" | `segment(2) → mission (confidence: 71%)` |
| "the second folder is the mission_id" | `segment(2) → mission_id (confidence: 89%)` |

**Hint Validation Indicators:**

| Icon | Meaning |
|------|---------|
| `...` | Parsing in progress |
| `[check]` | Hint valid, high confidence |
| `[?]` | Hint valid, needs clarification |
| `[!]` | Hint may cause issues (warning) |
| `[x]` | Hint invalid (error) |

**Keyboard Shortcuts:**

| Key | Action |
|-----|--------|
| `Enter` | Submit hint |
| `Tab` | Cycle through suggestions |
| `@` | Open template autocomplete |
| `Ctrl+Space` | Show entity autocomplete (segments, columns) |
| `Esc` | Cancel hint input |
| `Ctrl+U` | Clear hint field |

#### 3.6.8 CLI Integration

Hints can be provided via CLI for scripting:

```bash
# Single hint
casparian wizard pathfinder /data/sample.csv \
  --hint "the second folder is the mission identifier"

# Multiple hints
casparian wizard pathfinder /data/sample.csv \
  --hint "segment -3 is mission_id" \
  --hint "compute start_month from quarter"

# Template hint
casparian wizard pathfinder /data/sample.csv \
  --hint "@quarter_expansion"

# Hint from file
casparian wizard pathfinder /data/sample.csv \
  --hint-file hints.txt

# Source-level hint (persisted)
casparian source hint add my_source \
  --hint "all dates use DD/MM/YYYY format"

# List source hints
casparian source hint list my_source

# Remove source hint
casparian source hint remove my_source --id hint_abc123
```

**MCP Tool Parameters:**

Update existing MCP tool definitions to include structured hint support:

```json
{
  "name": "wizard_pathfinder",
  "parameters": {
    "sample_paths": { "type": "string[]", "required": true },
    "hint": {
      "type": "string",
      "required": false,
      "description": "Natural language hint or @template reference"
    },
    "hints": {
      "type": "object[]",
      "required": false,
      "description": "Structured hint objects for programmatic use",
      "items": {
        "properties": {
          "type": { "enum": ["segment", "column", "type", "compute", "filter"] },
          "source": { "type": "string" },
          "target": { "type": "string" },
          "value": { "type": "string" }
        }
      }
    },
    "auto_approve": { "type": "boolean", "default": false }
  }
}
```

#### 3.6.9 Error Handling

**Hint Parse Errors:**

| Error | User Message | Recovery |
|-------|--------------|----------|
| Empty hint | "Hint is empty. Press Esc to cancel or type a hint." | Re-prompt |
| Unrecognized syntax | "Could not parse: '{fragment}'. Try natural language or check @template syntax." | Show examples |
| Invalid segment reference | "Segment {N} is out of bounds. Path has {M} segments." | Show segment list |
| Invalid column reference | "Column '{name}' not found. Available: {columns}" | Show column list |
| Type conflict | "Cannot interpret '{value}' as {type}." | Suggest alternatives |
| Template not found | "Template '@{name}' not found. Available: {templates}" | Show template list |

**Hint Processing Errors:**

| Error | User Message | Recovery |
|-------|--------------|----------|
| LLM timeout with hint | "AI took too long. Try simplifying the hint or removing computation keywords." | Retry or simplify |
| Conflicting hints | "Hints conflict: {hint1} vs {hint2}. Please provide one consistent instruction." | Re-prompt |
| Hint ignored by LLM | "AI did not incorporate hint '{hint}'. Reason: {reason}. Try rephrasing?" | Show LLM reasoning |

#### 3.6.10 Implementation Notes

**Performance Considerations:**

- Hint parsing (Stage 1-3) should complete in <50ms
- Context hash computation should complete in <10ms
- Suggestion query should complete in <20ms
- Real-time preview should update within 100ms of keystroke

**Parsing Implementation:**

```rust
pub struct HintParser {
    intent_classifier: IntentClassifier,
    keyword_detector: KeywordDetector,
    entity_extractor: EntityExtractor,
}

impl HintParser {
    pub fn parse(&self, raw_hint: &str) -> Result<ParsedHint, HintParseError> {
        let intent = self.intent_classifier.classify(raw_hint)?;
        let escalation = self.keyword_detector.check_escalation(raw_hint);
        let entities = self.entity_extractor.extract(raw_hint)?;

        Ok(ParsedHint {
            original: raw_hint.to_string(),
            intent,
            escalation,
            entities,
            confidence: self.compute_confidence(&intent, &entities),
        })
    }

    fn compute_confidence(&self, intent: &Intent, entities: &[Entity]) -> f32 {
        let mut score = 0.5; // Base confidence

        // Exact segment reference
        if entities.iter().any(|e| matches!(e, Entity::Segment { .. })) {
            score += 0.30;
        }

        // Type keyword present
        if matches!(intent, Intent::Format | Intent::Type) {
            score += 0.20;
        }

        // Structured syntax used
        if entities.iter().any(|e| e.from_structured_syntax()) {
            score += 0.10;
        }

        score.min(1.0)
    }
}

#[derive(Debug)]
pub struct ParsedHint {
    pub original: String,
    pub intent: Intent,
    pub escalation: Escalation,
    pub entities: Vec<Entity>,
    pub confidence: f32,
}

#[derive(Debug)]
pub enum Intent {
    Format,      // Date format, number format
    Naming,      // Field naming, renaming
    Computation, // Derive new fields
    Filter,      // Skip lines, ignore patterns
    Structure,   // Segment assignment
    Type,        // Type specification
}

#[derive(Debug)]
pub enum Escalation {
    YamlOk,
    PythonRequired { keywords: Vec<String> },
}

#[derive(Debug)]
pub enum Entity {
    Segment { index: i32, field_name: Option<String> },
    Column { reference: ColumnRef, field_name: Option<String> },
    Type { target: String, type_spec: TypeSpec },
    Pattern { target: String, regex: String },
    Literal { target: String, value: String },
    Computation { source: String, targets: Vec<String>, expression: String },
}
```

---

## Summary of Changes

This specification addresses GAP-INT-003 by defining:

1. **Three hint input modes**: Free-form natural language (default), structured syntax (power users), and templates (reusable patterns)

2. **Three-stage parsing pipeline**: Intent extraction, keyword classification (escalation check), and entity extraction

3. **LLM prompt integration**: Structured template showing how hints are formatted and injected, with conflict resolution rules

4. **Validation and feedback**: Confidence scoring, ambiguity detection, and clarification dialogs

5. **Persistence and reuse**: Database schema for hint history, templates, suggestions, and source-level hints with inheritance hierarchy

6. **TUI integration**: Extended hint dialog with suggestions, real-time preview, and template autocomplete

7. **CLI integration**: Command-line flags and MCP tool parameters for programmatic hint provision

8. **Error handling**: Comprehensive error messages and recovery strategies

---

## New Gaps Identified

| ID | Description | Priority |
|----|-------------|----------|
| GAP-HINT-001 | IntentClassifier implementation not specified (ML vs rule-based) | MEDIUM |
| GAP-HINT-002 | Template sharing/community repository undefined | LOW |
| GAP-HINT-003 | Hint localization (non-English hints) not addressed | LOW |

---

## Cross-References

- Section 3.1.1: YAML vs Python Decision Algorithm (escalation keywords)
- Section 3.5: Path Intelligence Engine (field naming uses similar NLP)
- Section 5.4: Hint Input (TUI dialog specification)
- Section 9: Error Handling (retry behavior)
- Section 10: MCP Interface (tool parameters)

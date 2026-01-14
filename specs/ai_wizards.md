# AI Wizards - Layer 2 Specification

**Status:** Draft
**Version:** 0.3
**Parent:** spec.md
**Dependencies:** specs/discover.md (TUI), roadmap/spec_discovery_intelligence.md (Iron Core), specs/semantic_path_mapping.md (Semantic Layer)

---

## 1. Executive Summary

AI Wizards are **build-time assistants** that generate configuration for Casparian Flow's deterministic runtime. They are optional—the system functions fully without them.

### 1.1 The Golden Rule

> "If the user uninstalls the LLM, Casparian Flow must still function as a rigorous, manual data engineering IDE."

### 1.2 The AI Value Add

> "With the LLM installed, Casparian Flow becomes an IDE that writes its own code."

### 1.3 Core Principle: Build-Time, Not Runtime

| Aspect | Runtime AI (REJECTED) | Build-Time AI (ACCEPTED) |
|--------|----------------------|--------------------------|
| When | Every file processed | Once per pattern |
| Output | Ephemeral decisions | Persisted code/config |
| Determinism | Non-deterministic | Code is static |
| Scale | O(files) LLM calls | O(patterns) LLM calls |
| Auditability | "AI decided" | "Rule R17 matched" |
| Debugging | Cannot reproduce | Can reproduce |

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          LAYER 2: AI WIZARDS                                │
│                          (Optional - Build Time)                            │
│                                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐    │
│  │ Pathfinder  │  │   Parser    │  │  Labeling   │  │ Semantic Path   │    │
│  │   Wizard    │  │   Wizard    │  │   Wizard    │  │    Wizard       │    │
│  │             │  │             │  │             │  │                 │    │
│  │ Path → Code │  │ Sample →    │  │ Headers →   │  │ Paths →         │    │
│  │ (Extractor) │  │ Code        │  │ Tag Name    │  │ Semantic +      │    │
│  │             │  │ (Parser)    │  │             │  │ Extraction Rule │    │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘    │
│         │                │                │                   │             │
│         └────────────────┴────────────────┴───────────────────┘             │
│                                    │                                        │
│                                    ▼                                        │
│                             ┌─────────────┐                                 │
│                             │   Drafts    │  Temporary storage              │
│                             │   Store     │  ~/.casparian_flow/drafts/      │
│                             └──────┬──────┘                                 │
│                                    │ User Approves                          │
├────────────────────────────────────┼────────────────────────────────────────┤
│                                    ▼                                        │
│                          LAYER 1: IRON CORE                                │
│                          (Required - Runtime)                              │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Extractors   │ Parsers      │ Extraction Rules │ Semantic Paths      │  │
│  │ (Python)     │ (Python)     │ (Glob + Extract) │ (Vocabulary)        │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Dependency Direction

- Layer 2 depends on Layer 1 (reads samples, writes config)
- Layer 1 NEVER depends on Layer 2 (no AI at runtime)
- Layer 1 is complete without Layer 2

---

## 3. The Four Wizards

### 3.1 Pathfinder Wizard

**Purpose:** Generate Extraction Rules (declarative YAML) from file paths. Falls back to Python extractors only for complex logic.

> **⚠️ Updated v0.3:** Pathfinder now generates **YAML Extraction Rules first** (see `specs/extraction_rules.md`). Python extractors are only generated when the extraction logic cannot be expressed declaratively.

**Input:**
- Sample file path(s)
- Optional: User hints ("the second folder is always the mission name")

**Output:**
- **Primary:** YAML Extraction Rule (declarative, portable)
- **Fallback:** Python Extractor file (for complex/conditional logic)
- Preview of extracted metadata

**Output Selection Logic:**

```
┌─────────────────────────────────────────────────────────────────┐
│                    PATHFINDER OUTPUT DECISION                   │
│                                                                 │
│  Analyze detected patterns:                                     │
│                                                                 │
│  Can ALL patterns be expressed as:                              │
│  • segment(N) + regex capture?                                  │
│  • full_path + regex capture (for variable depth)?              │
│  • known type (date_iso, integer, etc.)?                        │
│  • literal normalization?                                       │
│           │                                                     │
│     ┌─────┴─────┐                                               │
│     ▼           ▼                                               │
│   [YES]       [NO]                                              │
│     │           │                                               │
│     ▼           ▼                                               │
│  Generate    Generate Python                                    │
│  YAML Rule   (with comment noting why)                          │
└─────────────────────────────────────────────────────────────────┘
```

**When to use Python fallback:**
- Conditional logic based on other extracted values
- Multi-step transformations
- Lookups against external data
- Pattern matching that spans multiple segments

**Example 1: Simple pattern → YAML Rule (preferred)**
```
Input Path: /data/ADT_Inbound/2024/01/msg_001.hl7

Generated Extraction Rule (YAML):
┌────────────────────────────────────────────────────────────────┐
│ name: "healthcare_path"                                        │
│ glob: "**/ADT_*/*/*/*"                                        │
│ extract:                                                       │
│   direction:                                                   │
│     from: segment(-4)                                          │
│     pattern: "ADT_(Inbound|Outbound)"                          │
│     capture: 1                                                 │
│   year:                                                        │
│     from: segment(-3)                                          │
│     type: integer                                              │
│     validate: "1900..2100"                                     │
│   month:                                                       │
│     from: segment(-2)                                          │
│     type: integer                                              │
│     validate: "1..12"                                          │
│ tag: hl7_messages                                              │
│ priority: 100                                                  │
└────────────────────────────────────────────────────────────────┘

Preview:
  msg_001.hl7 → {direction: "Inbound", year: 2024, month: 1}
```

**Example 2: Complex logic → Python Extractor (fallback)**
```
Input Path: /data/CLIENT-ABC/2024/Q1/report.csv
User Hint: "Quarter folder should compute start/end month"

Generated Extractor (Python):
┌────────────────────────────────────────────────────────────────┐
│ # NOTE: Python extractor generated because extraction requires │
│ # computed fields (quarter → month range) not expressible     │
│ # in declarative YAML rules.                                   │
│                                                                │
│ from pathlib import Path                                       │
│                                                                │
│ def extract(path: str) -> dict:                                │
│     parts = Path(path).parts                                   │
│     metadata = {}                                              │
│                                                                │
│     for part in parts:                                         │
│         if part.startswith("CLIENT-"):                         │
│             metadata["client_id"] = part.split("-")[1]         │
│                                                                │
│         if part.isdigit() and len(part) == 4:                  │
│             metadata["year"] = int(part)                       │
│                                                                │
│         # Quarter computation - requires Python                │
│         if part.startswith("Q") and len(part) == 2:           │
│             q = int(part[1])                                   │
│             metadata["quarter"] = q                            │
│             metadata["start_month"] = (q - 1) * 3 + 1          │
│             metadata["end_month"] = q * 3                      │
│                                                                │
│     return metadata                                            │
└────────────────────────────────────────────────────────────────┘

Preview:
  report.csv → {client_id: "ABC", year: 2024, quarter: 1,
                start_month: 1, end_month: 3}
```

**Relationship to Semantic Path Wizard:**

| Aspect | Pathfinder | Semantic Path |
|--------|-----------|---------------|
| **Recognition** | AI analyzes path directly | Algorithmic + AI disambiguation |
| **Vocabulary** | Ad-hoc patterns | Named semantic primitives |
| **Reusability** | Single rule | Transferable across sources |
| **Output** | YAML rule (primary) or Python | Always YAML rule |
| **Complexity** | Handles any logic | Standard folder patterns only |

**When to use which:**
- **Semantic Path Wizard**: Source matches known primitives (dated_hierarchy, entity_folder, etc.)
- **Pathfinder Wizard**: Custom patterns, unusual conventions, or complex extraction logic

#### 3.1.1 YAML vs Python Decision Algorithm

The Pathfinder Wizard uses a deterministic algorithm to decide between YAML rules and Python extractors.

**Core Principle:** Each detected extraction pattern is classified as `YAML_OK` or `PYTHON_REQUIRED`. If ANY pattern requires Python, the entire output is Python (no hybrid outputs).

**Classification Precedence:**

```
1. USER HINTS (highest priority)
   └─ Hints can ESCALATE YAML_OK → PYTHON_REQUIRED
   └─ Hints can NEVER downgrade PYTHON_REQUIRED → YAML_OK

2. PATTERN COMPLEXITY
   └─ Automatic detection of Python-requiring patterns

3. DEFAULT BEHAVIOR (lowest priority)
   └─ If no complexity detected → YAML_OK
```

**YAML-Expressible Patterns:**

| Pattern Type | YAML Construct | Example |
|--------------|----------------|---------|
| Segment extraction | `from: segment(N)` | segment(-3) → field |
| Filename extraction | `from: filename` | Extract from file name |
| Full/relative path regex | `from: full_path`, `from: rel_path` | Variable depth matching |
| Regex capture | `pattern: "(\\d+)"` | Capture group 1 |
| Type coercion | `type: integer\|date\|uuid` | String → typed value |
| Normalization | `normalize: lowercase` | Case/format normalization |
| Default values | `default: "unknown"` | Fallback value |

**Python-Required Patterns:**

| Pattern Type | Why Python | Example |
|--------------|------------|---------|
| Computed fields | ONE input → MULTIPLE outputs | `Q1` → `{quarter: 1, start_month: 1, end_month: 3}` |
| Conditional logic | Runtime branching | Mixed date formats in samples |
| Multi-step transform | Requires intermediate variables | Extract → decode base64 → parse JSON |
| External lookup | Reference external data | Month name → month number |
| Cross-segment dependency | Interpretation varies by context | Hint-driven only |

**Hint Detection:**

User hints containing computation keywords trigger Python classification:
- `compute`, `calculate`, `derive`, `formula`
- `start/end`, `range`, `convert to`, `expand`
- `lookup`, `map to`, `translate`, `reference`
- `if`, `when`, `otherwise`, `conditional`
- `combine`, `merge`, `join`, `aggregate`

**Example: Quarter Pattern Behavior**

| Scenario | Sample | User Hints | Classification | Result |
|----------|--------|------------|----------------|--------|
| Default | `Q1` | None | YAML_OK | `pattern: Q(\d)` → extracts `1` |
| Override | `Q1` | "compute start/end month" | PYTHON_REQUIRED | Generates Python with month calculation |

**Recommendations vs Classification:**

Complex but YAML-expressible patterns (regex >100 chars, >5 capture groups) remain classified as `YAML_OK` but generate a recommendation. The UI can present this:
```
✓ Generated YAML Rule
  ⚠ Recommendation: Consider Python for readability
    Regex is 87 chars. You can edit to Python if preferred.
```

#### 3.1.2 Python Extractor Validation

When the Pathfinder Wizard generates Python extractors (fallback from YAML), the code passes through a four-stage validation pipeline before user review.

**Validation Pipeline:**

```
LLM Output → Syntax → Security → Signature → Sandbox → User Review
```

| Stage | Purpose | Failure Action |
|-------|---------|----------------|
| **Syntax** | AST parsing, detect syntax errors | Retry with error context |
| **Security** | Import whitelist, block dangerous operations | Retry with allowed modules list |
| **Signature** | Verify `extract(path) -> dict` | Retry with signature template |
| **Sandbox** | Execute against sample paths | Retry or offer YAML fallback |

**Import Whitelist:**

Extractors may only import: `pathlib`, `os.path`, `re`, `fnmatch`, `datetime`, `time`, `typing`, `collections`, `dataclasses`, `enum`, `json`, `math`, `uuid`, `base64`, `hashlib`, `urllib.parse`

**Blocked Operations:** File I/O, network, subprocess, dynamic code (`exec`, `eval`)

**Sandbox Properties:**
- Timeout: 5 seconds per path
- Isolated subprocess with restricted builtins
- Validates return type is `dict`

**Fallback:** After 3 failed retries, offer simpler YAML extraction (without computed fields).

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_017/engineer.md`

#### 3.1.3 Complexity Configuration

The thresholds that determine when Python is recommended over YAML are fully configurable.

**Configuration File:** `~/.casparian_flow/config.toml`

**Threshold Levels:**

| Level | Regex Chars | Capture Groups | Behavior |
|-------|-------------|----------------|----------|
| **YAML_OK** | ≤100 | ≤5 | Default to YAML, show ✓ |
| **RECOMMEND_PYTHON** | 100-200 | 5-10 | Show ⚠ with recommendation |
| **FORCE_PYTHON** | >200 | >10 | Require Python, show 🔴 |

**Configuration Schema:**

```toml
[complexity]
recommend_python_regex_chars = 100
recommend_python_capture_groups = 5
force_python_regex_chars = 200
force_python_capture_groups = 10
prefer_yaml = true
sensitivity = "strict"  # or "loose"
```

**Override Precedence:** defaults → config.toml → source overrides → CLI flags

**CLI Overrides:** `--prefer-python`, `--prefer-yaml`, `--recommend-regex-chars=N`

**Sensitivity Modes:**
- **strict**: Both regex length and capture groups evaluated
- **loose**: Only force thresholds applied (more permissive)

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_018/engineer.md`

### 3.2 Parser Wizard (Parser Lab)

**Purpose:** Generate Parsers (Python classes) from sample files.

**Input:**
- Sample file (first 100 rows)
- Optional: Target schema
- Optional: User hints ("column 3 is a date in DD/MM/YYYY format")

**Output:**
- Python Parser file (draft)
- Preview of parsed output
- Validation against sample

**Example:**
```
Input File: sales_data.csv (first 10 rows)

Generated Parser:
┌────────────────────────────────────────────────────────────────┐
│ import pandas as pd                                            │
│ import pyarrow as pa                                           │
│                                                                │
│ class SalesParser:                                             │
│     name = "sales_parser"                                      │
│     version = "1.0.0"                                          │
│     topics = ["sales_data"]                                    │
│                                                                │
│     outputs = {                                                │
│         "sales": pa.schema([                                   │
│             ("id", pa.int64()),                                │
│             ("date", pa.date32()),                             │
│             ("amount", pa.float64()),                          │
│         ])                                                     │
│     }                                                          │
│                                                                │
│     def parse(self, ctx):                                      │
│         df = pd.read_csv(ctx.input_path)                       │
│         df["date"] = pd.to_datetime(df["date"])                │
│         yield ("sales", df)                                    │
└────────────────────────────────────────────────────────────────┘

Validation: ✓ 10/10 rows parsed successfully
```

### 3.3 Labeling Wizard

**Purpose:** Suggest semantic labels for file groups based on content structure.

**Input:**
- File group signature (from Fingerprint Engine)
- Headers
- Sample values (first 5 per column)

**Output:**
- Suggested tag name
- Confidence score
- Alternative suggestions

**Label Persistence Strategy:**

> **Key Insight:** Labels derived from content (headers/columns) must persist to **future files with the same structure**, not just files at the same path.

The Labeling Wizard tags the **Signature Group**, not individual files or paths:

| Approach | Mechanism | Future File Handling |
|----------|-----------|---------------------|
| ~~Path-based rule~~ | ~~Create extraction rule with glob~~ | ~~Only matches same path pattern~~ |
| **Signature Group tagging** | Tag the `cf_signature_groups` record | Future files with same structural fingerprint auto-inherit tag |

**Why Signature Groups:**
- Content structure (columns, types) is the actual discriminator
- Same content can exist in different paths (e.g., `/archive/` vs `/incoming/`)
- Fingerprint Engine already groups files by structure

**Example:**
```
Input:
  Headers: [id, txn_date, amount, customer_email]
  Sample: {
    id: ["1001", "1002", "1003"],
    txn_date: ["2024-01-15", "2024-01-16"],
    amount: ["$100.00", "$250.50"],
    customer_email: ["a@b.com", "c@d.com"]
  }

Output:
  Label: "Sales Transactions" (89% confidence)
  Alternatives: ["Invoice Records", "Payment History"]
  Reasoning: "Contains transaction dates, amounts, and customer emails"

Persistence:
  → Signature Group 'abc123' tagged as "Sales Transactions"
  → Future files matching this fingerprint auto-inherit tag
```

### 3.4 Semantic Path Wizard

**Purpose:** Recognize semantic folder structure and generate extraction rules from file paths.

> **Full Specification:** See `specs/semantic_path_mapping.md`

**Input:**
- Sample file paths from a source
- Optional: User hints ("the first folder is the mission identifier")

**Output:**
- Semantic path expression (e.g., `entity_folder(mission) > dated_hierarchy(iso) > files`)
- Generated extraction rule (glob + extract fields)
- Suggested tag name

**What makes it different from Pathfinder Wizard:**

| Aspect | Pathfinder Wizard | Semantic Path Wizard |
|--------|-------------------|---------------------|
| **Output** | Python code (extractor function) | Declarative YAML rule |
| **Abstraction** | Low-level (regex, string ops) | High-level (semantic primitives) |
| **Reusability** | Code-specific | Pattern-transferable |
| **Recognition** | AI generates code | AI recognizes semantics + generates config |
| **Runtime** | Extractor runs on each file | Rule matches during scan |

**When to use which:**
- **Pathfinder**: Complex extraction logic, multiple derived fields, conditional logic
- **Semantic Path**: Standard folder patterns, reusable across sources, simpler output

**Example:**
```
Input Paths:
  /data/mission_042/2024-01-15/telemetry.csv
  /data/mission_043/2024-01-16/readings.csv
  /data/mission_044/2024-01-17/sensor_log.csv

AI Analysis:
  Detected primitives:
    - segment(-3): "mission_*" → entity_folder(mission)
    - segment(-2): "????-??-??" → dated_hierarchy(iso)
    - segment(-1): "*.csv" → files

Output:
  ┌────────────────────────────────────────────────────────────────┐
  │ Semantic Expression:                                            │
  │   entity_folder(mission) > dated_hierarchy(iso) > files         │
  │                                                                 │
  │ Generated Rule:                                                 │
  │   glob: "**/mission_*/????-??-??/*.csv"                        │
  │   extract:                                                      │
  │     mission_id:                                                 │
  │       from: segment(-3)                                         │
  │       pattern: "mission_(.*)"                                   │
  │     date:                                                       │
  │       from: segment(-2)                                         │
  │       type: date_iso                                            │
  │   tag: mission_data                                             │
  │                                                                 │
  │ Similar Sources:                                                │
  │   • defense_contractor_a (same semantic structure)              │
  │   • research_lab (same semantic structure)                      │
  └────────────────────────────────────────────────────────────────┘

Confidence: 94%
```

**Hybrid Mode (Pathfinder + Semantic):**

For complex cases, the wizards can work together:

1. **Semantic Path Wizard** recognizes the folder structure
2. **Pathfinder Wizard** generates extractor for edge cases within that structure

```
Example: Mission data with complex filename encoding

Semantic: entity_folder(mission) > dated_hierarchy(iso) > files
  → Handles: mission_id, date from folder structure

Pathfinder: Extractor for filename
  → Handles: sensor_type, reading_number from "telemetry_001.csv"

Combined Output:
  mission_id: "042" (from semantic)
  date: "2024-01-15" (from semantic)
  sensor_type: "telemetry" (from extractor)
  reading_number: "001" (from extractor)
```

#### 3.4.1 TUI Invocation

The Semantic Path Wizard has three primary entry points in Discover mode:

**Entry Points:**

| Entry Point | Key | Condition | Behavior |
|-------------|-----|-----------|----------|
| **Sources panel** | `S` | Source focused, 3+ files | Analyzes all files in source |
| **Files panel** | `S` | 2+ files selected, same source | Analyzes selected files only |
| **Wizard menu** | `W` then `s` | Any state | Manual source selection |

**Pre-Detection Algorithm:**

Before calling the LLM, a fast algorithmic check determines if semantic structure exists:

| Confidence | Behavior |
|------------|----------|
| ≥80% | Show results immediately (no LLM needed) |
| 40-80% | Show results with AI disambiguator |
| <40% | Offer Pathfinder alternative |

**State Flow:**

```
SAMPLING → PRE-DETECTION → {GENERATING if ambiguous} → RESULT → APPROVED/CANCELED
```

**Edge Cases:**
- Empty source: Error with scan option
- <3 files: Warning with option to include full source
- Cross-source selection: Offer separate rules per source
- >500 files: Auto-sample 5 files (cost control)

**Differentiation from Pathfinder:** Use Semantic Path for standard folder patterns (dated hierarchies, entity folders). Use Pathfinder for custom extraction logic or unusual patterns.

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_019/engineer.md`

#### 3.4.2 Hybrid Mode Workflow

**Purpose:** Combine Semantic Path Wizard (folder structure) with Pathfinder Wizard (filename patterns) for complete metadata extraction in a single workflow.

**When to Use Hybrid Mode:**

Many real-world files have two layers of structure:
```
Layer 1: Folder Structure (Semantic)    → mission_id, date, experiment
Layer 2: Filename Encoding (Pathfinder) → sensor_type, reading_number
```

**Trigger Scenarios:**

| Scenario | Trigger | Behavior |
|----------|---------|----------|
| **User Request** | From Semantic results, user clicks "Add filename extraction" | Handoff to Pathfinder for filename |
| **Auto-Detection** | Semantic ≥70% AND filename has 2+ extractable patterns | Offer "Would you like to extract filename fields?" |
| **Cascade** | After Semantic approval, remaining untagged files | Offer to run Pathfinder on remainder |

**State Flow:**

```
SEMANTIC_RESULTS (≥70% confidence)
    │
    ├─ [Approve] → Draft Created (semantic only)
    │
    └─ [Add filename extraction] → HYBRID_OFFERED
                                      │
                                      ├─ [Yes] → HYBRID_PROCESSING → HYBRID_RESULTS → Draft Created
                                      │
                                      └─ [No] → Back to SEMANTIC_RESULTS
```

**Handoff Context:**

When Semantic hands off to Pathfinder:
- Semantic fields already extracted (e.g., `mission_id`, `date`)
- Folder structure expression (e.g., `entity_folder(mission) > dated_hierarchy(iso)`)
- Filenames only (path stripped, extension included)
- Pathfinder extracts from filename component only

**Conflict Resolution:**

| Conflict | Resolution |
|----------|------------|
| Same field name, different source | Keep semantic (folders more reliable), log warning |
| Same field name, different value | Show dialog: "Field 'date' differs - keep semantic or filename?" |
| YAML + Python mix | Escalate entire rule to Python |

**Combined Output Format:**

```yaml
name: "hybrid_extraction_rule"
source: "hybrid"  # Indicates Semantic + Pathfinder

semantic:
  expression: "entity_folder(mission) > dated_hierarchy(iso) > files"

extract:
  # From Semantic (folder)
  mission_id:
    from: segment(2)
    pattern: "mission_(\\d+)"
  date:
    from: segment(3)
    type: date

  # From Pathfinder (filename)
  sensor_type:
    from: filename
    pattern: "^([a-z_]+?)_\\d+"
  reading_number:
    from: filename
    pattern: "_(\\d+)\\."

tag: "telemetry_data"
```

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_028/engineer.md`

### 3.5 Path Intelligence Engine

**Purpose:** Foundational AI layer that clusters files by path similarity and proposes semantic field names. Powers the Semantic Path Wizard and enhances extraction rule creation.

> **Why this exists:** The algorithmic inference in `extraction.md` works well with 3+ structurally identical files. But real-world data has messy naming, cross-source variations, and ambiguous patterns. The Path Intelligence Engine uses lightweight ML to bridge these gaps.

#### 3.5.1 Core Capabilities

| Capability | Input | Output | Benefit |
|------------|-------|--------|---------|
| **Path Clustering** | Raw file paths | Groups of "same structure" files | Reduce N files → K clusters for rule creation |
| **Field Name Intelligence** | Path segments + patterns | Semantic field names | Better downstream queries (`mission_id` not `segment2`) |
| **Cross-Source Equivalence** | Paths from multiple sources | Unified extraction schema | Query across sources with same fields |
| **Single-File Proposals** | One example path | Candidate extraction fields | Bootstrap without 3+ examples |

#### 3.5.2 Path Clustering

**Problem:** User scans a folder with 500 files. Current options:
- Review all 500 individually (tedious)
- Hope algorithmic inference works (needs similar structure)

**Solution:** Cluster paths by semantic similarity before rule creation.

```
Input: 500 file paths from /data/

Clustering Output:
┌─────────────────────────────────────────────────────────────────────┐
│  Cluster A (247 files) - 94% internal similarity                    │
│    /data/sales/2024/jan/orders_001.csv                              │
│    /data/sales/2024/feb/orders_002.csv                              │
│    /data/sales/2023/dec/orders_847.csv                              │
│    Proposed: { department: "sales", year, month, doc_type: "orders" }│
├─────────────────────────────────────────────────────────────────────┤
│  Cluster B (89 files) - 91% internal similarity                     │
│    /data/reports/client_acme/quarterly_Q1.xlsx                      │
│    /data/reports/client_globex/quarterly_Q2.xlsx                    │
│    Proposed: { doc_type: "reports", client_name, quarter }          │
├─────────────────────────────────────────────────────────────────────┤
│  Cluster C (12 files) - 87% internal similarity                     │
│    /data/misc/backup_2024-01-15.zip                                 │
│    /data/misc/backup_2024-01-16.zip                                 │
│    Proposed: { doc_type: "backup", date }                           │
├─────────────────────────────────────────────────────────────────────┤
│  Unclustered (152 files) - low similarity                           │
│    → Review individually or provide hints                           │
└─────────────────────────────────────────────────────────────────────┘

User creates 3 extraction rules instead of reviewing 500 files.
```

**Implementation Approach:**

| Phase | Technique | Model | Training Required |
|-------|-----------|-------|-------------------|
| Phase 1 | Sentence embeddings + HDBSCAN | `all-MiniLM-L6-v2` (22M params) | None |
| Phase 2 | Fine-tuned embeddings | Custom adapter | User-approved rules as training data |

**Phase 1 Algorithm (No Training):**

```python
from sentence_transformers import SentenceTransformer
import hdbscan

def cluster_paths(paths: List[str]) -> List[Cluster]:
    # Normalize paths for embedding
    normalized = [normalize_for_embedding(p) for p in paths]
    # e.g., "/data/sales/2024/jan/orders.csv"
    #    → "data sales 2024 jan orders csv"

    # Embed with lightweight model (runs on CPU, <100ms for 1000 paths)
    model = SentenceTransformer('all-MiniLM-L6-v2')
    embeddings = model.encode(normalized)

    # Cluster with HDBSCAN (no predefined K)
    clusterer = hdbscan.HDBSCAN(min_cluster_size=5, metric='cosine')
    labels = clusterer.fit_predict(embeddings)

    # Group paths by cluster label
    return group_by_label(paths, labels)
```

**Why Embeddings Beat Algorithmic Inference:**

| Scenario | Algorithmic | Embedding-Based |
|----------|-------------|-----------------|
| `sales_2024_jan.csv` vs `sales_2024_feb.csv` | ✓ Same pattern | ✓ Same cluster |
| `sales_2024_jan.csv` vs `Sales Data Jan 2024.csv` | ✗ Different pattern | ✓ Same cluster |
| `/data/mission_042/` vs `/archive/msn-42/` | ✗ Different pattern | ✓ Same cluster |
| `proj_alpha` vs `project_alpha` | ✗ Different prefix | ✓ Same cluster |

#### 3.5.3 Field Name Intelligence

**Problem:** Algorithmic inference detects a variable segment but can't name it meaningfully.

```
Segment values: ["mission_042", "mission_043", "mission_044"]
Algorithmic: field name = "segment2" or "mission_id" (prefix match)

Segment values: ["proj_alpha", "proj_beta", "proj_gamma"]
Algorithmic: field name = "proj_id" (prefix match) — but user wants "project_name"

Segment values: ["acme_corp", "globex_inc", "initech"]
Algorithmic: field name = "segment1" — no pattern detected
```

**Solution:** LLM proposes semantic field names based on context.

```
Input to LLM:
  Path: /data/clients/acme_corp/invoices/2024/Q1/inv_001.pdf
  Segments: ["data", "clients", "acme_corp", "invoices", "2024", "Q1", "inv_001.pdf"]
  Variable segments: [2, 5, 6] (indices)

LLM Output:
  segment[2] "acme_corp" → field: "client_name" (type: string)
  segment[5] "Q1" → field: "quarter" (type: integer, extract: 1)
  segment[6] "inv_001.pdf" → field: "invoice_number" (type: string, pattern: "inv_(\\d+)")
```

**Model Selection:**

| Model | Size | Latency | Quality | Use Case |
|-------|------|---------|---------|----------|
| Phi-3.5 Mini | 3.8B | ~200ms | Good | Default for field naming |
| Qwen 2.5 3B | 3B | ~150ms | Good | Alternative |
| Few-shot GPT-4 | API | ~500ms | Excellent | High-value / ambiguous cases |

**Prompt Template:**

```
You are a data engineer naming extraction fields for file paths.

Given this path: {path}
And these variable segments: {segments_with_values}

Suggest a field name and type for each variable segment.
Field names should be:
- snake_case
- Descriptive (prefer "client_name" over "segment2")
- Domain-appropriate (use "mission_id" for defense, "patient_mrn" for healthcare)

Context hints: {user_hints}

Output JSON:
{
  "fields": [
    {"segment": 2, "value": "acme_corp", "field_name": "client_name", "type": "string"},
    ...
  ]
}
```

#### 3.5.4 Cross-Source Semantic Equivalence

**Problem:** Same logical structure, different surface syntax across sources.

```
Source A: /data/mission_042/2024-01-15/readings.csv
Source B: /archive/msn-42/20240115/data.csv
Source C: /backup/MISSION.042/2024.01.15/readings.csv
```

Algorithmic equivalence classes (from `extraction.md` Section 4) use structural fingerprinting:
- Folder depth distribution
- Segment patterns (Fixed, Variable, Date, Numeric)

**This misses semantic equivalence.** All three sources have `mission_id` and `date`, but with different encodings.

**Solution:** Embedding similarity across sources.

```
Step 1: Cluster paths within each source (Section 3.5.2)

Step 2: Embed cluster representatives
  Source A representative: "data mission 042 2024-01-15 readings csv"
  Source B representative: "archive msn 42 20240115 data csv"
  Source C representative: "backup MISSION 042 2024.01.15 readings csv"

Step 3: Cross-source similarity matrix
            Source A    Source B    Source C
  Source A     -          0.87        0.91
  Source B    0.87         -          0.84
  Source C    0.91        0.84         -

Step 4: Propose unified schema
  All three sources → { mission_id: string, date: date }
  Source-specific extraction patterns generated for each
```

**Workflow:**

```bash
$ casparian sources --find-equivalents

  Found semantic equivalence (3 sources):

    Source A: /data/missions/        → mission_042/2024-01-15/*
    Source B: /archive/old_missions/ → msn-42/20240115/*
    Source C: /backup/MISSIONS/      → MISSION.042/2024.01.15/*

  Proposed unified fields:
    • mission_id (string) - extracted differently per source
    • date (date) - extracted differently per source

  Create unified extraction rules? [Y/n]:
```

#### 3.5.5 Single-File Proposals

**Problem:** `extraction.md` requires 3+ files for algorithmic inference. But users often start with one example.

**Solution:** LLM proposes fields from single path, user confirms.

```
$ casparian extract /data/CLIENT-ABC/invoices/2024/Q1/inv_001.pdf

  Analyzing 1 file (using AI assistance)...

  Proposed extraction fields:

    Segment          Value          Proposed Field     Confidence
    ─────────────────────────────────────────────────────────────
    segment(-5)      CLIENT-ABC     client_id          ████████░░ 82%
    segment(-4)      invoices       doc_type           █████████░ 91%
    segment(-3)      2024           year               ██████████ 98%
    segment(-2)      Q1             quarter            █████████░ 94%
    filename         inv_001.pdf    invoice_number     ███████░░░ 71%

  [Enter] Accept  [e] Edit  [m] More examples  [Esc] Cancel
```

**Confidence Factors:**

| Factor | Weight | Example |
|--------|--------|---------|
| Known pattern (date, quarter, etc.) | +30% | "2024" → year |
| Prefix match (CLIENT-, inv_) | +20% | "CLIENT-ABC" → client_id |
| Domain keywords in path | +15% | "invoices" in path → doc_type |
| Segment position heuristics | +10% | Last folder often categorical |
| LLM semantic analysis | +25% | Context-aware naming |

#### 3.5.6 Integration with Other Wizards

The Path Intelligence Engine is a **foundation layer** used by:

| Wizard | How It Uses Path Intelligence |
|--------|-------------------------------|
| **Semantic Path Wizard** | Clustering to find representative paths; field naming |
| **Pathfinder Wizard** | Field name suggestions when generating Python extractors |
| **Labeling Wizard** | Pre-cluster files before labeling groups |
| **Parser Wizard** | Suggest parser scope based on file clusters |

**Data Flow:**

```
                          ┌─────────────────────────┐
                          │  Path Intelligence      │
                          │  Engine                 │
                          │                         │
                          │  • Embedding model      │
                          │  • Clustering           │
                          │  • Field naming LLM     │
                          └───────────┬─────────────┘
                                      │
              ┌───────────────────────┼───────────────────────┐
              │                       │                       │
              ▼                       ▼                       ▼
    ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
    │ Semantic Path   │    │ Pathfinder      │    │ Labeling        │
    │ Wizard          │    │ Wizard          │    │ Wizard          │
    │                 │    │                 │    │                 │
    │ Uses: clusters, │    │ Uses: field     │    │ Uses: clusters  │
    │ field names     │    │ names           │    │ for batch label │
    └─────────────────┘    └─────────────────┘    └─────────────────┘
```

#### 3.5.7 Training Data Flywheel

User-approved extraction rules become training data:

```
User approves rule:
  glob: "**/CLIENT-*/invoices/{year}/Q{quarter}/*"
  extract:
    client_id: { from: segment(-5), pattern: "CLIENT-(.*)" }
    year: { from: segment(-3), type: integer }
    quarter: { from: segment(-2), pattern: "Q(\\d)" }

System stores:
  training_example = {
    "paths": ["/data/CLIENT-ABC/invoices/2024/Q1/inv_001.pdf", ...],
    "approved_fields": ["client_id", "year", "quarter"],
    "field_mappings": { "CLIENT-ABC": "client_id", "2024": "year", "Q1": "quarter" }
  }
```

**Flywheel Effect:**

```
More users → More approved rules → Better training data
                                          ↓
                              Fine-tuned embeddings
                                          ↓
                              Better clustering + field proposals
                                          ↓
                              Fewer corrections needed
                                          ↓
                              More users (better UX)
```

#### 3.5.8 Model Configuration

```toml
# ~/.casparian_flow/config.toml

[ai.path_intelligence]
enabled = true

# Embedding model for clustering
embedding_model = "all-MiniLM-L6-v2"  # Default: lightweight, CPU-friendly
# embedding_model = "all-mpnet-base-v2"  # Alternative: higher quality, slower

# Clustering parameters
min_cluster_size = 5          # Minimum files to form a cluster
cluster_selection_epsilon = 0.1

# Field naming model
field_naming_model = "phi3.5:3.8b"   # Ollama model for field proposals
field_naming_timeout = 10            # Seconds

# Cross-source equivalence
equivalence_threshold = 0.75         # Minimum similarity for equivalence
```

#### 3.5.9 Privacy and Path Sanitization

**Three-Layer Sanitization Model:**

Paths are sanitized before sending to embedding models or LLMs:

```
Raw Path → [Layer 1: Auto Detection] → [Layer 2: User Rules] → [Layer 3: Structure] → Sanitized
```

| Layer | Purpose | Example |
|-------|---------|---------|
| **Layer 1** | Automatic sensitive pattern detection | `/home/jsmith/` → `/home/[USER]/` |
| **Layer 2** | User-configured rules | `CLIENT-ACME` → `[CLIENT]` |
| **Layer 3** | Structural preservation | Preserve segment positions for clustering |

**Redaction Severity Levels:**

| Severity | Enforcement | Examples |
|----------|-------------|----------|
| **Critical** | Always enforced, cannot override | PHI (MRN, SSN), home directories, API keys |
| **High** | Default on, user can override | Client names, project codes |
| **Medium** | Suggested redaction | Possible person names, phone numbers |
| **Low** | Informational only | Dates, version strings (usually preserved) |

**Default Redaction Patterns:**

```toml
# ~/.casparian_flow/config.toml
[privacy]
mode = "standard"  # strict | standard | permissive

[privacy.rules.username]
pattern = "/(home|Users)/[^/]+"
replacement = "/[USER_HOME]"
severity = "critical"

[privacy.rules.client]
pattern = "CLIENT-[A-Z0-9]+"
replacement = "[CLIENT]"
severity = "high"

[privacy.rules.mrn]
pattern = "[Mm][Rr][Nn][_-]?\\d+"
replacement = "[MRN]"
severity = "critical"
```

**Example Sanitization:**

| Original Path | Sanitized Path |
|---------------|----------------|
| `/home/jsmith/CLIENT-ACME/data.csv` | `/[USER_HOME]/[CLIENT]/data.csv` |
| `/patients/john_doe_mrn_12345/scan.dcm` | `/patients/[PATIENT]/scan.dcm` |
| `/projects/SECRET-DARPA-X/report.pdf` | `/projects/[PROJECT]/report.pdf` |

**Local vs Cloud Behavior:**

| Mode | Redaction Level | Override Allowed |
|------|-----------------|------------------|
| **Local Ollama** | Standard | User can disable Medium/Low |
| **Cloud API** | Strict | Critical always enforced |
| **Air-gapped** | Permissive | User controls all levels |

**User Preview (TUI):**

Before sending paths to LLM, users can preview what will be sent:
```
┌─ PRIVACY PREVIEW ─────────────────────────────────────────────┐
│ Original: /home/jsmith/CLIENT-ACME/invoices/2024/report.pdf   │
│ Sanitized: /[USER]/[CLIENT]/invoices/2024/report.pdf          │
│                                                                │
│ Redactions applied:                                            │
│   • [USER]: "jsmith" (Critical - username)                     │
│   • [CLIENT]: "ACME" (High - client identifier)                │
│                                                                │
│ [Enter] Send   [e] Edit rules   [Esc] Cancel                   │
└────────────────────────────────────────────────────────────────┘
```

**CLI Commands:**

```bash
casparian privacy test /path/to/file     # Preview sanitization
casparian privacy show                   # Show active rules
casparian privacy rule add "PATTERN"     # Add custom rule
```

#### 3.5.10 Implementation Phases

| Phase | Scope | Success Criteria | Rollback |
|-------|-------|------------------|----------|
| **Phase 1** | Embedding clustering | Cluster purity ≥85%, latency <500ms/1000 paths | Fall back to algorithmic inference |
| **Phase 2** | Field naming (LLM) | Accuracy ≥75%, semantic correctness ≥90% | Disable LLM, use prefix heuristics |
| **Phase 3** | Cross-source equivalence | Precision ≥85%, recall ≥70% | Raise threshold, require manual confirm |
| **Phase 4** | Single-file proposals | Quality ≥70%, bootstrap success ≥80% | Require "More examples" flow |
| **Phase 5** | Training data flywheel | Capture rate ≥95%, privacy compliance 100% | Disable training capture |
| **Phase 6** | Fine-tuned embeddings | ≥10% improvement, no regression >5% | Rollback to base model |

**Gate Criteria (proceed to next phase when ALL pass):**
- Unit tests pass for phase functionality
- Integration tests with TUI pass
- Performance thresholds met
- User study (if applicable) shows improvement

#### 3.5.11 Training Data Storage

Training examples from the flywheel (Section 3.5.7) are stored in the main database:

```sql
CREATE TABLE ai_training_examples (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,              -- Link to committed extraction rule
    sample_paths_json TEXT NOT NULL,    -- Sanitized sample paths (JSON array)
    extraction_config_json TEXT NOT NULL, -- The approved extraction config
    approved_by TEXT,
    approved_at INTEGER NOT NULL,       -- Unix millis
    quality_score REAL,                 -- User rating if provided
    created_at INTEGER NOT NULL
);

CREATE TABLE ai_training_field_mappings (
    id TEXT PRIMARY KEY,
    example_id TEXT NOT NULL REFERENCES ai_training_examples(id),
    segment_value TEXT NOT NULL,        -- Sanitized value (e.g., "[CLIENT]")
    field_name TEXT NOT NULL,           -- Approved field name
    field_type TEXT                     -- integer, date, string, uuid
);
```

**Privacy:** All paths are sanitized per Section 3.5.9 before storage. Raw values are never stored.

**Export:** `casparian ai training export --format jsonl` for sharing anonymized training data.

#### 3.5.12 TUI Integration

The Path Intelligence Engine provides cluster-based file organization through a dedicated **Cluster Review** workflow accessible from Discover mode.

**Entry Points:**

| Entry Point | Key | Behavior |
|-------------|-----|----------|
| **Files Panel** | `C` | Cluster all files in current view |
| **Sources Manager** | `c` | Cluster all files in source |
| **AI Wizards Menu** | `W` then `1` | Open cluster wizard with source selection |

**State Machine (5 states):**

```
Clustering (progress) → Overview (list) ↔ Expanded (detail) ↔ Editing (form)
                                ↓
                         Accepted (rule created)
```

**Key Features:**
- Cluster list with similarity scores and confidence
- File preview within clusters
- Wizard handoff (`w` → Pathfinder, `s` → Semantic Path, `l` → Labeling)
- Unclustered files handling with hint-based re-clustering

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_015/engineer.md`

#### 3.5.13 Unclustered Files Threshold

Files are classified as "unclustered" when they don't fit any cluster reliably.

**Unclustered Conditions (any triggers unclustered):**

| Condition | Threshold | Meaning |
|-----------|-----------|---------|
| HDBSCAN noise | label = -1 | Algorithm couldn't place in any cluster |
| Small cluster | < 5 files | Too few for stable pattern |
| Low confidence | < 70% | Unreliable grouping |

**HDBSCAN Parameters:**
- `min_cluster_size = 5`
- `cluster_selection_epsilon = 0.1`
- `metric = 'cosine'`

**UI for Unclustered Files:**
- Displayed with `[~]` label in cluster list
- 5-option menu: Manual review, Provide hints, Re-cluster, Ignore, Single-file rules
- Hint-based re-clustering uses relaxed thresholds (60% confidence, 3-file minimum)

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_024/engineer.md`

#### 3.5.14 Single-File Confidence Scoring

When analyzing a single file, confidence is computed from five weighted factors.

**Confidence Factors:**

| Factor | Weight | Description |
|--------|--------|-------------|
| **Known Patterns** | 30% | Date formats, quarters, ISO codes |
| **Prefix Match** | 20% | Standard prefixes (CLIENT-, inv_, doc_) |
| **Domain Keywords** | 15% | Finance, healthcare, legal terminology |
| **Segment Position** | 10% | Root=categorical, leaf=ID heuristic |
| **LLM Semantic** | 25% | Language model field naming |

**Confidence Bands:**

| Band | Range | User Action |
|------|-------|-------------|
| Very High | 90-100% | Accept directly |
| High | 75-89% | Quick review |
| Medium | 60-74% | Review before accepting |
| Low | 40-59% | Edit or collect more examples |
| Very Low | 0-39% | Reject or collect more |

**Weight Normalization:** Only active factors contribute; if a factor scores 0, its weight redistributes to remaining factors.

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_025/engineer.md`

### 3.6 User Hint System

User hints allow natural language guidance to AI wizards, improving extraction accuracy and enabling computed fields.

**Hint Input Modes:**

| Mode | Example | Use Case |
|------|---------|----------|
| **Free-form** | "the second folder is the mission name" | Default for all users |
| **Structured** | `segment(-3) = mission_id` | Power users |
| **Templates** | `@quarter_expansion` | Reusable patterns |

**Three-Stage Parsing Pipeline:**

```
Raw Hint → [Intent Extraction] → [Keyword Classification] → [Entity Extraction] → Parsed Hint
```

**Escalation Keywords** (trigger PYTHON_REQUIRED):
- Computation: `compute`, `calculate`, `derive`, `formula`
- Ranges: `start/end`, `range`, `convert to`, `expand`
- Lookups: `lookup`, `map to`, `translate`, `reference`
- Conditionals: `if`, `when`, `otherwise`, `conditional`

**Hint Persistence:**

Successful hints are stored for reuse via context hash matching:
- `hint_history`: Individual hint interpretations
- `hint_templates`: Reusable named patterns
- `source_hints`: Source-level defaults

**Inheritance Hierarchy:** Global Templates < User Templates < Source-Level < File-Level

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_016/engineer.md`

#### 3.6.1 Hint Input Limits

Character limits prevent excessive context consumption and ensure TUI readability.

| Mode | Limit | Rationale |
|------|-------|-----------|
| **Free-form** | 500 chars | ~80 words, fits terminal width |
| **Structured** | 300 chars | Syntax is more compact |
| **Template invocation** | 100 chars | Just the template name |

**Validation Behavior:**
- Real-time character counter with color feedback (green → yellow at 80% → red at 100%)
- Submit disabled when over limit
- AI-assisted trim suggestions when user exceeds limit

**LLM Context Budget:** 500 chars ≈ 125 tokens (~0.06% of 200K context window)

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_021/engineer.md`

---

## 4. Draft Lifecycle

### 4.1 State Machine

```
                    ┌─────────────┐
                    │   START     │
                    └──────┬──────┘
                           │ User invokes wizard
                           ▼
                    ┌─────────────┐
                    │  GENERATING │ ← AI working
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
       ┌──────────┐ ┌──────────┐ ┌──────────┐
       │  DRAFT   │ │  ERROR   │ │ TIMEOUT  │
       │ (review) │ │ (retry?) │ │ (retry?) │
       └────┬─────┘ └────┬─────┘ └────┬─────┘
            │            │            │
            │      ┌─────┴─────┐      │
            │      ▼           ▼      │
            │  [Retry]     [Cancel]   │
            │      │           │      │
            │      └───────────┼──────┘
            │                  │
    ┌───────┼──────────────────┼───────┐
    │       │                  │       │
    ▼       ▼                  ▼       ▼
┌────────┐ ┌────────┐    ┌──────────┐ ┌────────┐
│APPROVED│ │REJECTED│    │  MANUAL  │ │CANCELED│
│(commit)│ │(delete)│    │  (edit)  │ │        │
└───┬────┘ └────────┘    └────┬─────┘ └────────┘
    │                         │
    └───────────┬─────────────┘
                │
                ▼
         ┌─────────────┐
         │  COMMITTED  │ → Moved to Layer 1
         └─────────────┘
```

#### 4.1.1 Transition Triggers

| From | To | Trigger | Notes |
|------|----|---------| ------|
| START | GENERATING | User invokes wizard | Via W menu or keybinding |
| GENERATING | DRAFT | AI completes | Auto-transition |
| GENERATING | ERROR | AI fails | Timeout, model unavailable |
| DRAFT | APPROVED | Enter in wizard | Or 'c' in draft list |
| DRAFT | REJECTED | 'd' in draft list | With confirmation |
| DRAFT | MANUAL | 'e' in wizard/draft list | Opens $EDITOR |
| DRAFT | EXPIRED | 24h timeout | Auto-cleanup |
| ERROR | GENERATING | 'r' (retry) | Max 3 retries |
| ERROR | CANCELED | Esc | No draft created |
| MANUAL | DRAFT | Editor closes | File may be modified |
| APPROVED | COMMITTED | Auto | Moves to Layer 1 |

#### 4.1.2 Timeout Behavior

- **24-hour expiry**: Drafts auto-delete after 24h
- **Max 10 drafts**: Oldest deleted when limit exceeded
- **Warning indicator**: Drafts < 2h remaining show warning
- **CLI cleanup**: `casparian draft clean` removes expired

#### 4.1.3 Draft List Keybindings

| Key | Action |
|-----|--------|
| j/k | Navigate drafts |
| Enter | Open draft preview |
| c | Commit draft to Layer 1 |
| e | Edit draft in $EDITOR |
| d | Delete draft (with confirm) |
| v | Validate draft |
| Esc | Close draft list |

#### 4.1.4 Draft ID Specification

**Format:**

| Property | Value |
|----------|-------|
| Length | 8 characters |
| Character set | Lowercase hexadecimal (`0-9a-f`) |
| Source | First 8 characters of UUIDv4 |
| Example | `a7b3c9d2`, `f8e2d1c0` |

**Generation Algorithm:**

```rust
pub fn generate_draft_id() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}
```

**When Generated:**
- Draft ID is generated **after** validation passes (Tier 3 complete)
- ID is NOT assigned during GENERATING or VALIDATING states
- ID becomes permanent artifact name when committed

**Collision Handling:**
- Check file existence AND manifest before using ID
- Retry up to 3 times if collision detected
- With max 10 drafts, collision probability is ~0.000001%

### 4.2 Storage

```
~/.casparian_flow/
├── drafts/                          # Temporary AI outputs
│   ├── extractor_a7b3c9d2.py        # Pathfinder draft
│   ├── parser_f8e2d1c0.py           # Parser Lab draft
│   └── manifest.json                # Draft metadata
├── extractors/                      # Committed extractors (Layer 1)
│   └── healthcare_path.py
├── parsers/                         # Committed parsers (Layer 1)
│   └── sales_parser.py
└── config.toml                      # Model configuration
```

### 4.3 Draft Manifest

```json
{
  "drafts": [
    {
      "id": "a7b3c9d2",
      "type": "extractor",
      "file": "extractor_a7b3c9d2.py",
      "created_at": "2026-01-08T10:30:00Z",
      "expires_at": "2026-01-09T10:30:00Z",
      "source_context": {
        "sample_paths": ["/data/ADT_Inbound/2024/01/msg_001.hl7"],
        "user_hints": null
      },
      "model": "qwen-2.5-7b",
      "status": "pending_review"
    }
  ]
}
```

### 4.4 Cleanup Policy

| Condition | Action |
|-----------|--------|
| Draft older than 24h | Auto-delete |
| User explicitly rejects | Immediate delete |
| User approves | Move to Layer 1, delete draft |
| More than 10 drafts | Delete oldest |

### 4.5 External Editor Handling

The EDITING state in wizard state machines opens files in the user's editor. This requires careful terminal management.

**Editor Resolution Priority:**
1. `$VISUAL` environment variable
2. `$EDITOR` environment variable
3. Platform fallback: `open -t -W` (macOS), `sensible-editor` (Linux), `notepad` (Windows)

**Terminal Handoff Protocol:**

```rust
// Before spawning editor
terminal.leave_alternate_screen()?;
crossterm::terminal::disable_raw_mode()?;
crossterm::cursor::Show;

// Spawn editor and wait (blocking)
let status = Command::new(&editor.command)
    .args(&editor.args)
    .arg(&temp_file_path)
    .spawn()?
    .wait()?;

// After editor closes
crossterm::terminal::enable_raw_mode()?;
terminal.enter_alternate_screen()?;
crossterm::cursor::Hide;
terminal.clear()?;
```

**Exit Status Handling:**

| Condition | Outcome | Action |
|-----------|---------|--------|
| Exit code 0 | Success | Read modified file, validate, transition |
| Exit code non-zero | Cancelled | Return to previous state, keep original |
| Process killed (SIGKILL) | Crashed | Show error, preserve temp file for recovery |
| Timeout (1 hour) | Timeout | Kill process, show warning |

**GUI Editor Support:**

Auto-detect GUI editors and inject `--wait` flag:
- VS Code: `code --wait`
- Sublime Text: `subl --wait`
- Atom: `atom --wait`

**Fallback When No Editor:**

If no editor found, the `e` key shows error toast: "No editor configured. Set $EDITOR environment variable."

---

## 5. Wizard UX (TUI)

### 5.1 Pathfinder Wizard Dialog

**YAML Rule Output (default):**
```
┌─ PATHFINDER WIZARD ─────────────────────────────────────────────┐
│                                                                  │
│  Sample Path: /data/ADT_Inbound/2024/01/msg_001.hl7             │
│                                                                  │
│  Analyzing path structure...                                     │
│                                                                  │
│  Output: YAML Extraction Rule ✓                                  │
│  (All patterns expressible declaratively)                        │
│                                                                  │
│  ┌─ Detected Patterns ────────────────────────────────────────┐ │
│  │  • "ADT_Inbound" → direction = "Inbound"    [✓ keep]       │ │
│  │  • "2024"        → year = 2024 (integer)    [✓ keep]       │ │
│  │  • "01"          → month = 1 (integer)      [✓ keep]       │ │
│  │  • "msg_001"     → (ignored - too specific)                │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Generated Rule (YAML) ────────────────────────────────────┐ │
│  │  name: "healthcare_path"                                   │ │
│  │  glob: "**/ADT_*/*/*/*"                                   │ │
│  │  extract:                                                  │ │
│  │    direction:                                              │ │
│  │      from: segment(-4)                                     │ │
│  │      pattern: "ADT_(Inbound|Outbound)"                     │ │
│  │    year:                                                   │ │
│  │      from: segment(-3)                                     │ │
│  │      type: integer                                         │ │
│  │    month:                                                  │ │
│  │      from: segment(-2)                                     │ │
│  │      type: integer                                         │ │
│  │  tag: hl7_messages                                         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Preview (5 files) ────────────────────────────────────────┐ │
│  │  ✓ msg_001.hl7 → {direction: Inbound, year: 2024, ...}    │ │
│  │  ✓ msg_002.hl7 → {direction: Inbound, year: 2024, ...}    │ │
│  │  ✓ msg_003.hl7 → {direction: Inbound, year: 2024, ...}    │ │
│  │  ✗ readme.txt  → {} (no patterns matched)                  │ │
│  │  ✓ msg_005.hl7 → {direction: Inbound, year: 2024, ...}    │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  Rule name: healthcare_path_____________                         │
│                                                                  │
│  [Enter] Approve   [r] Regenerate   [e] Edit   [h] Hint   [Esc] │
└──────────────────────────────────────────────────────────────────┘
```

**Python Fallback (when YAML insufficient):**
```
┌─ PATHFINDER WIZARD ─────────────────────────────────────────────┐
│                                                                  │
│  Sample Path: /data/CLIENT-ABC/2024/Q1/report.csv               │
│  Hint: "Quarter folder should compute start/end month"          │
│                                                                  │
│  Output: Python Extractor ⚠                                      │
│  (Reason: Computed fields require Python)                        │
│                                                                  │
│  ┌─ Detected Patterns ────────────────────────────────────────┐ │
│  │  • "CLIENT-ABC" → client_id = "ABC"         [✓ keep]       │ │
│  │  • "2024"       → year = 2024               [✓ keep]       │ │
│  │  • "Q1"         → quarter, start_month, end_month          │ │
│  │                   (computed - requires Python)              │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Generated Code (Python) ──────────────────────────────────┐ │
│  │  # Computed fields require Python - YAML insufficient       │ │
│  │  def extract(path: str) -> dict:                           │ │
│  │      parts = Path(path).parts                              │ │
│  │      metadata = {}                                         │ │
│  │      for part in parts:                                    │ │
│  │          if part.startswith("CLIENT-"):                    │ │
│  │              metadata["client_id"] = part.split("-")[1]    │ │
│  │          if part.startswith("Q"):                          │ │
│  │              q = int(part[1])                              │ │
│  │              metadata["quarter"] = q                       │ │
│  │              metadata["start_month"] = (q - 1) * 3 + 1     │ │
│  │          ...                                               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Preview (5 files) ────────────────────────────────────────┐ │
│  │  ✓ report.csv → {client_id: ABC, quarter: 1, ...}         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  Extractor name: client_reports_____________                     │
│                                                                  │
│  [Enter] Approve   [r] Regenerate   [e] Edit   [h] Hint   [Esc] │
└──────────────────────────────────────────────────────────────────┘
```

#### 5.1.1 Pathfinder Wizard State Machine

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│  ANALYZING  │─────►│YAML_RESULT  │      │ HINT_INPUT  │─────►┌─────────────┐
│  (entry)    │      │    or       │◄────►│             │      │REGENERATING │
└──────┬──────┘      │PYTHON_RESULT│      └─────────────┘      └──────┬──────┘
       │             └──────┬──────┘                                  │
       │                    │         ┌───────────────────────────────┘
       ▼                    ▼         ▼
┌─────────────┐      ┌─────────────┐
│ANALYSIS_ERR │      │  APPROVED   │
└──────┬──────┘      │     or      │
       │             │  CANCELED   │
       ▼             └─────────────┘
┌─────────────┐
│   CLOSED    │
└─────────────┘
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| ANALYZING | Wizard invoked | Analysis completes/fails | Spinner. Esc cancels. |
| YAML_RESULT | Patterns YAML-expressible | User action | Shows patterns, rule, preview. Enter/h/e/r/Esc. |
| PYTHON_RESULT | Complex patterns | User action | Same as YAML_RESULT but with Python code. |
| ANALYSIS_ERROR | Analysis fails | r retries, Esc closes | Error message. Max 3 retries. |
| HINT_INPUT | h pressed | Enter submits, Esc cancels | Text input for refinement hints. |
| REGENERATING | Hint submitted / edit saved | Completes or fails | Spinner. Esc cancels. |
| APPROVED | Enter from result | - | Commits to Layer 1, dialog closes. |
| CANCELED | Esc from result/regenerating | - | Discards draft, dialog closes. |
| CLOSED | Esc from error | - | Dialog closes, no draft. |

**Transitions:**

| From | To | Trigger | Guard |
|------|----|---------| ------|
| (external) | ANALYZING | Wizard invoked | Sample path(s) provided |
| ANALYZING | YAML_RESULT | Analysis completes | All patterns YAML-expressible |
| ANALYZING | PYTHON_RESULT | Analysis completes | Complex patterns detected |
| ANALYZING | ANALYSIS_ERROR | Analysis fails | - |
| ANALYZING | CANCELED | Esc | - |
| YAML_RESULT | APPROVED | Enter | Name valid |
| YAML_RESULT | HINT_INPUT | h | - |
| YAML_RESULT | EDITING | e | $EDITOR available |
| YAML_RESULT | REGENERATING | r | - |
| YAML_RESULT | CANCELED | Esc | - |
| PYTHON_RESULT | (same as YAML_RESULT) | | |
| ANALYSIS_ERROR | REGENERATING | r | retry_count < 3 |
| ANALYSIS_ERROR | CLOSED | Esc | - |
| HINT_INPUT | REGENERATING | Enter | Hint non-empty |
| HINT_INPUT | (previous state) | Esc | - |
| REGENERATING | YAML_RESULT | Completes | YAML-expressible |
| REGENERATING | PYTHON_RESULT | Completes | Complex patterns |
| REGENERATING | ANALYSIS_ERROR | Fails | - |
| REGENERATING | CANCELED | Esc | - |

**Keybindings:**

| Key | ANALYZING | YAML/PYTHON_RESULT | ANALYSIS_ERROR | HINT_INPUT | REGENERATING |
|-----|-----------|-------------------|----------------|------------|--------------|
| Enter | - | Approve | - | Submit | - |
| Esc | Cancel | Cancel | Close | Back | Cancel |
| h | - | Hint | - | - | - |
| e | - | Edit | - | - | - |
| r | - | Regenerate | Retry | - | - |

### 5.2 Parser Wizard Dialog (Parser Lab)

```
┌─ PARSER LAB ────────────────────────────────────────────────────┐
│                                                                  │
│  Sample File: sales_2024.csv (1,234 rows)                       │
│                                                                  │
│  ┌─ Detected Structure ───────────────────────────────────────┐ │
│  │  Format: CSV (delimiter: ',')                              │ │
│  │  Headers: [id, date, amount, customer_email]               │ │
│  │  Types:   [Int64, Date(ISO), Float64, Email]               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Generated Parser ─────────────────────────────────────────┐ │
│  │  class SalesParser:                                        │ │
│  │      name = "sales_parser"                                 │ │
│  │      version = "1.0.0"                                     │ │
│  │      topics = ["sales_data"]                               │ │
│  │                                                            │ │
│  │      def parse(self, ctx):                                 │ │
│  │          df = pd.read_csv(ctx.input_path)                  │ │
│  │          ...                                               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌─ Validation ───────────────────────────────────────────────┐ │
│  │  ✓ 100/100 sample rows parsed successfully                 │ │
│  │  ✓ Output schema matches detected types                    │ │
│  │  ⚠ 2 rows have null values in 'amount' column             │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  Parser name: sales_parser     Version: 1.0.0                   │
│  Topic: sales_data                                               │
│                                                                  │
│  [Enter] Approve   [t] Test more   [r] Regenerate   [e] Edit    │
│  [h] Give hint     [s] Set schema  [Esc] Cancel                 │
└──────────────────────────────────────────────────────────────────┘
```

#### 5.2.1 Parser Lab State Machine

```
┌─────────────┐      ┌─────────────────────────────────────────────┐
│  ANALYZING  │─────►│ RESULT_VALIDATED / WARNING / FAILED        │
│  (entry)    │      └───────────────────────┬─────────────────────┘
└──────┬──────┘                              │
       │                    ┌────────────────┼────────────────┐
       ▼                    │                │                │
┌─────────────┐      ┌──────────┐    ┌───────────┐    ┌───────────┐
│ANALYSIS_ERR │      │HINT_INPUT│    │  EDITING  │───►│ VALIDATING│
└──────┬──────┘      │SCHEMA_INP│    └───────────┘    └─────┬─────┘
       ▼             │ TESTING  │                           │
┌─────────────┐      └────┬─────┘                           │
│   CLOSED    │           │         ┌───────────────────────┘
└─────────────┘           ▼         ▼
                   ┌─────────────────────┐
                   │    REGENERATING     │──► Result states
                   └─────────────────────┘
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| ANALYZING | Wizard invoked | Analysis completes/fails | Spinner. Esc cancels. |
| RESULT_VALIDATED | All rows pass | User action | Green checkmarks. Enter approved. |
| RESULT_WARNING | Rows pass with warnings | User action | Amber warnings. Enter approved. |
| RESULT_FAILED | Some rows fail | User action | Red errors. **Enter blocked.** |
| ANALYSIS_ERROR | Analysis fails | r retries, Esc closes | Max 3 retries. |
| HINT_INPUT | h pressed | Enter/Esc | Text input for hints. |
| SCHEMA_INPUT | s pressed | Enter/Esc | Schema editor for type enforcement. |
| TESTING | t pressed | Test completes/Esc | File picker, run parser on more files. |
| EDITING | e pressed | Editor closes | External $EDITOR, draft file. |
| VALIDATING | Editor closes (modified) | Validation completes | Validation-only, preserves user edits. |
| REGENERATING | Hint/schema submitted, r pressed | Completes/fails | AI regenerates, full validation runs. |
| APPROVED | Enter from VALIDATED/WARNING | - | Commits to Layer 1. |
| CANCELED | Esc | - | Discards draft. |

**Key Design: VALIDATING vs REGENERATING**
- **EDITING → VALIDATING**: Preserves user's manual code changes, runs validation only
- **HINT_INPUT/SCHEMA_INPUT → REGENERATING**: AI regenerates with new context

**Transitions (key):**

| From | To | Trigger | Guard |
|------|----|---------| ------|
| RESULT_FAILED | APPROVED | Enter | **BLOCKED** - must fix first |
| TESTING | RESULT_* | Completes | Cumulative result across all test files |
| EDITING | VALIDATING | Editor closes | File modified |
| VALIDATING | RESULT_* | Completes | Based on validation outcome |

**Keybindings:**

| Key | RESULT_* | HINT/SCHEMA | TESTING | EDITING | VALIDATING |
|-----|----------|-------------|---------|---------|------------|
| Enter | Approve¹ | Submit | - | - | - |
| Esc | Cancel | Back | Cancel | - | - |
| t | Test | - | - | - | - |
| r | Regenerate | - | - | - | - |
| e | Edit | - | - | - | - |
| h | Hint | - | - | - | - |
| s | Schema | - | - | - | - |

¹ Enter blocked on RESULT_FAILED (red border, error message)

### 5.3 Labeling Wizard Dialog

```
┌─ LABELING WIZARD ───────────────────────────────────────────────┐
│                                                                  │
│  Signature Group: abc123 (47 files, same structure)             │
│                                                                  │
│  ┌─ Structure Analysis ─────────────────────────────────────────┐│
│  │  Headers: [id, date, amount, customer_email]                 ││
│  │  Types:   [Int64, Date, Float64, Email]                      ││
│  │  Sample:  [1, 2024-01-15, 99.99, john@example.com]          ││
│  └──────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌─ AI Suggestion ──────────────────────────────────────────────┐│
│  │  Label: "Sales Transactions"                                 ││
│  │  Confidence: ████████░░ 82%                                  ││
│  │  Reasoning: Headers suggest financial transaction data       ││
│  └──────────────────────────────────────────────────────────────┘│
│                                                                  │
│  Alternatives: [Tab to cycle]                                    │
│    • Customer Orders (71%)                                       │
│    • Revenue Data (65%)                                          │
│                                                                  │
│  Label: Sales Transactions_______                                │
│                                                                  │
│  [Enter] Accept   [Tab] Next alt   [h] Hint   [e] Edit   [Esc]  │
└──────────────────────────────────────────────────────────────────┘
```

#### 5.3.1 Labeling Wizard State Machine

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│  ANALYZING  │─────►│SINGLE_RESULT│      │ HINT_INPUT  │
│  (entry)    │      │  or BATCH   │◄────►│             │
└──────┬──────┘      └──────┬──────┘      └─────────────┘
       │                    │
       ▼                    ▼
┌─────────────┐      ┌─────────────┐
│ANALYSIS_ERR │      │  APPROVED   │
└─────────────┘      │ or CANCELED │
                     └─────────────┘
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| ANALYZING | Wizard invoked with file group | Analysis completes/fails | Spinner. Analyzes headers/content. |
| SINGLE_RESULT | One group to label | User action | Shows suggestion, alternatives via Tab. |
| BATCH_RESULT | Multiple groups | User action | Table view, j/k navigation, a=accept all. |
| HINT_INPUT | h pressed | Enter/Esc | Domain hint for better suggestions. |
| EDITING | e pressed | Enter confirms | Inline label editing (no external editor). |
| REGENERATING | Hint submitted | Completes | AI regenerates suggestions. |
| APPROVED | Enter | - | Labels committed to cf_signature_groups. |
| CANCELED | Esc | - | No changes. |

**Keybindings:**

| Key | SINGLE_RESULT | BATCH_RESULT | HINT_INPUT |
|-----|---------------|--------------|------------|
| Enter | Accept label | Accept current | Submit hint |
| Tab | Cycle alternatives | - | - |
| h | Open hint | Open hint | - |
| e | Edit label | Edit label | - |
| j/k | - | Navigate groups | - |
| a | - | Accept all | - |
| Esc | Cancel | Cancel | Back |

### 5.4 Hint Input

When user presses `h` (hint), a sub-dialog appears:

```
┌─ PROVIDE HINT ──────────────────────────────────────────────────┐
│                                                                  │
│  Tell the AI what to fix:                                       │
│                                                                  │
│  > Column 'date' uses DD/MM/YYYY format, not ISO________________ │
│                                                                  │
│  Examples:                                                       │
│    • "Column 3 is a date in European format"                    │
│    • "Ignore lines starting with #"                             │
│    • "The 'amt' column should be named 'amount'"                │
│    • "Split the 'address' column on semicolons"                 │
│                                                                  │
│  [Enter] Submit hint   [Esc] Cancel                             │
└──────────────────────────────────────────────────────────────────┘
```

### 5.4 Manual Edit Mode

When user presses `e` (edit), the code opens in `$EDITOR`:

```
┌─ MANUAL EDIT ───────────────────────────────────────────────────┐
│                                                                  │
│  Opening in $EDITOR (vim)...                                    │
│                                                                  │
│  File: /tmp/casparian_draft_a7b3c9d2.py                         │
│                                                                  │
│  When you save and close:                                       │
│    • Draft will be updated with your changes                    │
│    • Validation will re-run automatically                       │
│                                                                  │
│  Press any key when done editing...                             │
└──────────────────────────────────────────────────────────────────┘
```

#### 5.4.1 Manual Edit Error Handling

When users manually edit YAML or Python, validation errors must be handled gracefully.

**Error Categories:**

| Type | Detection | Example |
|------|-----------|---------|
| **YAML Syntax** | Parser error | Unclosed bracket, duplicate key |
| **YAML Schema** | Schema validation | Missing required field, wrong type |
| **Python Syntax** | AST parse | Invalid indentation, missing colon |
| **Python Runtime** | Sandbox execution | Exception, type mismatch |

**Validation Timing:** On-save (not on-type) to avoid performance penalties

**Error Dialog:**
```
┌─ VALIDATION ERROR ─────────────────────────────────────────────┐
│  ❌ Python syntax error on line 15:                            │
│     IndentationError: unexpected indent                        │
│                                                                │
│  Line 15:                                                      │
│      return result  # <-- expected to be aligned               │
│                                                                │
│  [e] Edit again   [r] Regenerate   [Esc] Discard              │
└────────────────────────────────────────────────────────────────┘
```

**Recovery Options:**
- **Edit (e)**: Re-open editor with cursor at error line
- **Regenerate (r)**: AI generates fresh code (consumes retry budget)
- **Discard (Esc)**: Restore original before edits

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_022/engineer.md`

### 5.5 Semantic Path Wizard Dialog

```
┌─ SEMANTIC PATH WIZARD ──────────────────────────────────────────────────────┐
│                                                                              │
│  Source: /mnt/mission_data (analyzing 47 files)                             │
│                                                                              │
│  ┌─ Detected Structure ─────────────────────────────────────────────────┐   │
│  │                                                                       │   │
│  │  Semantic: entity_folder(mission) > dated_hierarchy(iso) > files     │   │
│  │                                                                       │   │
│  │  Path Breakdown:                                                      │   │
│  │   /mnt/mission_data/mission_042/2024-01-15/telemetry.csv             │   │
│  │   ─────────────────  ──────────  ──────────  ─────────────           │   │
│  │       (root)        mission_id    date        (file)                 │   │
│  │                                                                       │   │
│  │  Confidence: ████████████████░░░░ 94%                                │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  ┌─ Generated Extraction Rule ──────────────────────────────────────────┐   │
│  │  glob: "**/mission_*/????-??-??/*.csv"                               │   │
│  │  extract:                                                            │   │
│  │    mission_id: from segment(-3), pattern "mission_(.*)"              │   │
│  │    date: from segment(-2), type date_iso                             │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  ┌─ Preview (5 files) ──────────────────────────────────────────────────┐   │
│  │  ✓ mission_042/2024-01-15/telemetry.csv                              │   │
│  │    → { mission_id: "042", date: "2024-01-15" }                       │   │
│  │  ✓ mission_043/2024-01-16/readings.csv                               │   │
│  │    → { mission_id: "043", date: "2024-01-16" }                       │   │
│  │  ✓ mission_044/2024-01-17/sensor_log.csv                             │   │
│  │    → { mission_id: "044", date: "2024-01-17" }                       │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  ┌─ Similar Sources ────────────────────────────────────────────────────┐   │
│  │  This structure matches: entity_folder > dated_hierarchy              │   │
│  │  • defense_contractor_a (same pattern)                               │   │
│  │  • research_lab_data (same pattern)                                  │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  Tag: mission_data___________                                                │
│                                                                              │
│  [Enter] Create Rule   [a] Alternatives   [e] Edit   [h] Hint   [Esc]       │
└──────────────────────────────────────────────────────────────────────────────┘
```

#### 5.5.1 Semantic Path Wizard State Machine

```
┌─────────────┐      ┌───────────────────────────┐      ┌───────────────┐
│ RECOGNIZING │─────►│ RESULT_HIGH_CONFIDENCE    │◄────►│ HINT_INPUT    │
│   (entry)   │      │ or RESULT_LOW_CONFIDENCE  │      └───────────────┘
└──────┬──────┘      └────────────┬──────────────┘
       │                         │
       ▼                         ▼
┌─────────────┐      ┌───────────────────────────┐
│RECOG_ERROR  │      │   ALTERNATIVES_VIEW       │
└─────────────┘      └───────────────────────────┘
                              │
                              ▼
                     ┌───────────────┐
                     │ APPROVED      │
                     │ or CANCELED   │
                     └───────────────┘
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| RECOGNIZING | Wizard invoked | Recognition completes/fails | Spinner. Analyzes folder structure with semantic primitives. |
| RESULT_HIGH_CONFIDENCE | Confidence >= 80% | User action | Shows detected structure, rule, preview, similar sources. Enter approves. |
| RESULT_LOW_CONFIDENCE | Confidence < 80% | User action | Same view but "Alternatives" is highlighted. Consider exploring alternatives. |
| ALTERNATIVES_VIEW | a pressed | Enter selects, Esc back | List of alternative interpretations with confidence scores. j/k navigation. |
| HINT_INPUT | h pressed | Enter/Esc | Text input for refinement. |
| EDITING | e pressed | Editor closes | External $EDITOR for manual rule editing. |
| REGENERATING | Hint/alternative/edit | Completes | Re-analyzes with new context. |
| APPROVED | Enter | - | Rule written to extraction_rules/. |
| CANCELED | Esc | - | No rule created. |

**Keybindings:**

| Key | RESULT_* | ALTERNATIVES_VIEW |
|-----|----------|-------------------|
| Enter | Approve rule | Select alternative |
| a | Show alternatives | - |
| e | Edit rule | - |
| h | Give hint | - |
| j/k | - | Navigate list |
| Esc | Cancel | Back to result |

### 5.6 Keybindings

| Key | Context | Action |
|-----|---------|--------|
| `W` | Global (Discover mode) | Open Wizard menu |
| `w` | Files panel, file selected | Launch Pathfinder for file's path |
| `g` | Files panel, group context | Launch Parser Lab for group |
| `l` | Files panel, group context | Launch Labeling Wizard |
| `S` | Source selected | Launch Semantic Path Wizard for source |

**Wizard Menu (`W`):**
```
┌─ WIZARDS ─────────────────────────┐
│                                    │
│  [p] Pathfinder (Extractor)        │
│  [g] Parser Lab (Generator)        │
│  [l] Labeling (Semantic Tag)       │
│  [s] Semantic Path (Structure)     │
│                                    │
│  [Esc] Cancel                      │
└────────────────────────────────────┘
```

#### 5.6.1 Context Determination

Each wizard requires specific context. The algorithm determines context based on current TUI state.

| Wizard | Required Context | Context Source | Always Available? |
|--------|------------------|----------------|-------------------|
| **Pathfinder** | File path(s) | Selected file OR filtered files | No - requires file |
| **Parser Lab** | Signature group | Signature group of selected file | No - requires group |
| **Labeling** | Signature group | Signature group of selected file | No - requires group |
| **Semantic Path** | Source | Currently selected source | Yes - uses default source when sources exist |

**Context Priority (Pathfinder):**
1. Selected file in Files panel → use that file's path
2. Active filter with matches → use all filtered file paths
3. Pending Review > Unmatched Paths → use selected unmatched paths
4. None available → show error "Select a file first"

**Semantic Path Always-Available Behavior:**
Since `selected_source` defaults to the first source when sources exist, Semantic Path wizard is available whenever at least one source exists in Discover mode, even without explicit user selection.

#### 5.6.2 Error Cases

| Wizard | Missing Context | Error Message | Recovery Action |
|--------|----------------|---------------|-----------------|
| Pathfinder | No file selected | "Select a file first" | Focus Files panel |
| Pathfinder | Empty source | "Source has no files" | Scan a directory |
| Pathfinder | Empty file list after filter | "No files match current filter" | Adjust filter |
| Parser Lab | No signature group | "Select a file group" | Select file with known structure |
| Labeling | No signature group | "Select a file group" | Select file with known structure |
| Labeling | No headers detected | "Cannot label files without headers" | Select CSV/tabular file, or use Parser Lab first |
| Semantic Path | No sources | "Add a source first" | Scan a directory |
| Semantic Path | Empty source | "Source has no files" | Scan a directory |

Errors appear as toast messages (3s auto-dismiss), not modal dialogs.

#### 5.6.3 Focus Management

**On wizard open:**
- Discover state is frozen (selections preserved)
- Wizard dialog appears as modal overlay (centered, 80% width)
- All Discover keybindings are suspended

**On wizard close:**

| Outcome | Focus Returns To | State Changes |
|---------|------------------|---------------|
| Approved | Files panel | Artifact committed, success toast |
| Canceled (Esc) | Previous panel | No changes |
| Error | Previous panel | No changes |

#### 5.6.4 Visual Feedback

**Status bar hints (context-aware):**
```
Without context: Wizards: (select a file)
With file:       Wizards: [W]menu [w]Path [g]Parse [l]Label [S]Semantic
```

**Wizard Menu dimmed items:** Unavailable options are dimmed with reason shown on focus.

#### 5.6.5 Entry Point Inventory

| Wizard | Direct Key | Wizard Menu | Pending Review | Other |
|--------|------------|-------------|----------------|-------|
| Pathfinder | `w` (Files) | `W` then `p` | Unmatched Paths | - |
| Parser Lab | `g` (Files) | `W` then `g` | Parser Warnings | - |
| Labeling | `l` (Files) | `W` then `l` | Unlabeled Groups | - |
| Semantic Path | `S` (Global) | `W` then `s` | Unrecognized Sources | Sources Manager |

**Precedence:** Pending Review panel > Wizard Menu > Direct shortcut

#### 5.6.6 Wizard Menu State Machine

The Wizard Menu is a **modal overlay** on Discover mode. While open, Discover keybindings are suspended.

```
┌─────────────────┐     W     ┌─────────────────┐
│ DISCOVER_NORMAL │──────────►│  WIZARD_MENU    │
│                 │◄──────────│   (modal)       │
└─────────────────┘    Esc    └────────┬────────┘
                                       │ p/g/l/s
                                       ▼
                              ┌─────────────────┐
                              │ WIZARD_DIALOG   │
                              │ (Pathfinder/etc)│
                              └────────┬────────┘
                                       │ Approved/Canceled
                                       ▼
                              ┌─────────────────┐
                              │ DISCOVER_NORMAL │
                              └─────────────────┘
```

**Transitions:**

| From | To | Trigger | Guard |
|------|----|---------| ------|
| DISCOVER_NORMAL | WIZARD_MENU | W | - |
| WIZARD_MENU | DISCOVER_NORMAL | Esc | - |
| WIZARD_MENU | PATHFINDER | p | File selected (for path context) |
| WIZARD_MENU | PARSER_LAB | g | Group context available |
| WIZARD_MENU | LABELING | l | Group context available |
| WIZARD_MENU | SEMANTIC_PATH | s | Source selected |
| Any WIZARD | DISCOVER_NORMAL | Approved/Canceled | Wizard completes |

**Context Requirements:**

| Wizard | Required Context | Error if Missing |
|--------|------------------|------------------|
| Pathfinder | At least one file path | "Select a file first" |
| Parser Lab | File group (signature) | "Select a file group" |
| Labeling | File group (signature) | "Select a file group" |
| Semantic Path | Source | "Select a source" |

#### 5.6.7 Keybinding Conflict Resolution

**Design Principle:** Case sensitivity distinguishes global menu access from context-specific shortcuts.

| Key | Scope | Action | Availability |
|-----|-------|--------|--------------|
| `W` (capital) | Global | Open Wizard menu | Always (no dialog open) |
| `w` (lowercase) | Files panel | Launch Pathfinder directly | File selected |
| `S` (capital) | Global | Launch Semantic Path Wizard | Source exists |
| `s` (lowercase) | Global | Scan new directory | Always |
| `g` (lowercase) | Global | Open Glob Explorer | Always |
| `g` (lowercase) | Files panel | Launch Parser Lab | File with group selected |

**Priority Dispatch Order:**

1. **Dialog/Overlay** (highest) - If dialog open, all keys go to dialog handler
2. **State-Specific** - Current panel (Files, Dropdown) handles context keys
3. **Global** - Capital letters and function keys work if no dialog
4. **Fallback** (lowest) - Unrecognized keys ignored

**Case Sensitivity Implementation:**

```rust
match (key.code, key.modifiers) {
    (Char('W'), SHIFT) => Action::OpenWizardMenu,    // Global menu
    (Char('w'), NONE)  => Action::LaunchPathfinder,  // Direct (context)
    (Char('S'), SHIFT) => Action::LaunchSemanticPath,// Global
    (Char('s'), NONE)  => Action::ScanDirectory,     // Global
    _ => Action::Unhandled,
}
```

**Error Messages (Context-Aware):**

| Scenario | Error | Recovery Hint |
|----------|-------|---------------|
| `w` with no file | "Select a file first" | "Press j/k to select, then 'w'" |
| `g` in Files, no group | "File has no group" | "Tag files to create group" |
| `S` with no sources | "Scan a directory first" | "Press 's' to scan" |

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_020/engineer.md`

---

## 6. Model Configuration

### 6.1 Configuration File

```toml
# ~/.casparian_flow/config.toml

[ai]
enabled = true                    # Master switch for all AI features
provider = "ollama"               # ollama | llamacpp | disabled

[ai.models]
# Model for code generation (Pathfinder, Parser Lab)
code_model = "qwen2.5-coder:7b"
code_timeout_seconds = 60

# Model for classification (Labeling)
classify_model = "phi3.5:3.8b"
classify_timeout_seconds = 30

[ai.ollama]
host = "http://localhost:11434"
# If ollama not running, wizards show "AI unavailable" message

[ai.llamacpp]
# Alternative: bundled llama.cpp for air-gapped systems
model_path = "~/.casparian_flow/models/qwen2.5-coder-7b-q4.gguf"
threads = 4
gpu_layers = 0                    # 0 = CPU only
```

### 6.2 Model Selection

| Wizard | Recommended Model | Fallback | Why |
|--------|------------------|----------|-----|
| Pathfinder | Qwen 2.5 Coder 7B | Phi-3.5 Mini | Code generation quality |
| Parser Lab | Qwen 2.5 Coder 7B | Phi-3.5 Mini | Code generation quality |
| Labeling | Phi-3.5 Mini 3.8B | Qwen 2.5 7B | Classification, smaller is fine |

### 6.3 Fallback Behavior

| Condition | Behavior |
|-----------|----------|
| AI disabled in config | Wizard menu shows "AI disabled" |
| Ollama not running | Wizard shows "Start Ollama: `ollama serve`" |
| Model not downloaded | Wizard shows "Download model: `ollama pull qwen2.5-coder:7b`" |
| Generation timeout | Retry prompt with "Simplify" hint |
| Invalid code generated | Show error, offer retry or manual edit |

### 6.4 Configuration Precedence

When the same setting exists in multiple locations, precedence resolves conflicts.

**5-Level Hierarchy (lowest to highest):**

1. **Code defaults** — Hardcoded constants
2. **Config file global** — `[ai.pathfinder]` section
3. **Config file source** — `[sources."name"]` overrides
4. **Environment variables** — `CASPARIAN_*` prefix
5. **CLI flags** — Highest priority

**Example Resolution:**
```
recommend_python_regex_chars:
  Code default:     100
  config.toml:      150  ← used if no CLI flag
  --recommend-regex-chars=200  ← wins if specified
```

**Partial Config Support:** Config files can specify only some settings; missing fields use code defaults.

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_027/engineer.md`

### 6.5 Embedding Model Fallback

Path Intelligence Engine uses embeddings for clustering, with a three-tier fallback hierarchy.

**Fallback Tiers:**

| Tier | Model | Cluster Quality | Network Required |
|------|-------|-----------------|------------------|
| 1 (Primary) | all-MiniLM-L6-v2 | ≥85% | Yes (first download) |
| 2 (Offline) | TF-IDF | 70-75% | No |
| 3 (Minimal) | None (algorithmic only) | N/A | No |

**Download Behavior:**
- Auto-download on first use (~150MB)
- Cache location: `~/.casparian_flow/models/embeddings/`
- Checksum verification after download

**Offline Mode:** Set `offline_mode = true` in config to skip download attempts and use Tier 2.

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_029/engineer.md`

---

## 7. Privacy & Audit

### 7.1 Data Sent to LLM

| Wizard | Data Sent | Data NOT Sent |
|--------|-----------|---------------|
| Pathfinder | Path string only | File contents |
| Parser Lab | Headers + First 10 rows | Full file |
| Labeling | Headers + 5 sample values/column | Full file |

### 7.2 Redaction

Before sending to LLM, user can redact sensitive columns:

```
┌─ REDACT SENSITIVE DATA ─────────────────────────────────────────┐
│                                                                  │
│  The following will be sent to the AI:                          │
│                                                                  │
│  Headers: [id, date, amount, patient_ssn, diagnosis]            │
│                                                                  │
│  Sample values:                                                  │
│    id:          [1001, 1002, 1003]                               │
│    date:        [2024-01-15, 2024-01-16, 2024-01-17]            │
│    amount:      [$100.00, $250.50, $75.00]                       │
│    patient_ssn: [███-██-████, ███-██-████, ███-██-████] REDACTED│
│    diagnosis:   [███████████, ███████████] REDACTED             │
│                                                                  │
│  [Space] Toggle redaction   [Enter] Proceed   [Esc] Cancel      │
└──────────────────────────────────────────────────────────────────┘
```

### 7.3 Audit Log Table

```sql
CREATE TABLE cf_ai_audit_log (
    id TEXT PRIMARY KEY,
    wizard_type TEXT NOT NULL,        -- 'pathfinder', 'parser_lab', 'labeling'
    model_name TEXT NOT NULL,         -- 'qwen2.5-coder:7b'
    input_type TEXT NOT NULL,         -- 'path', 'sample', 'headers'
    input_hash TEXT NOT NULL,         -- blake3(input sent to LLM)
    input_preview TEXT,               -- First 500 chars (for debugging)
    redactions TEXT,                  -- JSON: ["patient_ssn", "diagnosis"]
    output_type TEXT,                 -- 'extractor', 'parser', 'label'
    output_hash TEXT,                 -- blake3(LLM response)
    output_file TEXT,                 -- Draft file path if code generated
    duration_ms INTEGER,
    status TEXT NOT NULL,             -- 'success', 'timeout', 'error'
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_ai_audit_wizard ON cf_ai_audit_log(wizard_type);
CREATE INDEX idx_ai_audit_created ON cf_ai_audit_log(created_at);
```

### 7.4 Audit CLI

```bash
# View recent AI activity
casparian ai audit --last 10

# Export audit log for compliance
casparian ai audit --since 2026-01-01 --format json > ai_audit.json

# Clear audit log (with confirmation)
casparian ai audit --clear --confirm
```

### 7.5 Signature Groups Table (for Labeling Wizard)

The Labeling Wizard stores labels in `cf_signature_groups`, which groups files by structural fingerprint:

```sql
-- Signature groups (files with same structure)
-- Populated by Fingerprint Engine (see roadmap/spec_discovery_intelligence.md)
CREATE TABLE cf_signature_groups (
    id TEXT PRIMARY KEY,                -- Blake3 hash of structural fingerprint
    fingerprint TEXT NOT NULL,          -- JSON: column names, types, row counts
    file_count INTEGER DEFAULT 0,       -- Number of files in this group
    label TEXT,                         -- Semantic label from Labeling Wizard
    labeled_by TEXT,                    -- 'user', 'ai', NULL if unlabeled
    labeled_at TEXT,                    -- ISO timestamp when labeled
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_sig_groups_label ON cf_signature_groups(label);
CREATE INDEX idx_sig_groups_unlabeled ON cf_signature_groups(label) WHERE label IS NULL;

-- Link files to their signature group
ALTER TABLE scout_files ADD COLUMN signature_group_id TEXT
    REFERENCES cf_signature_groups(id);
```

**Labeling Workflow:**

```sql
-- When user approves a label in Labeling Wizard:
UPDATE cf_signature_groups
SET label = 'Sales Transactions',
    labeled_by = 'ai',
    labeled_at = CURRENT_TIMESTAMP
WHERE id = 'abc123';

-- Propagate tag to files in group:
UPDATE scout_files
SET tags = json_insert(tags, '$[#]', 'Sales Transactions')
WHERE signature_group_id = 'abc123';
```

### 7.6 Audit Log Retention Policy

Automated cleanup prevents unbounded storage growth while preserving compliance requirements.

**Time-Based Retention:**

| Status | Retention | Rationale |
|--------|-----------|-----------|
| Success | 90 days | Sufficient for debugging |
| Error | 180 days | Longer for investigation |
| Critical Error | 365 days | Compliance requirement |

**Size-Based Limits:**
- Soft limit: 400 MB (triggers cleanup of oldest entries)
- Hard limit: 500 MB (aggressive cleanup)
- Always retain: Latest 1,000 records

**Compliance Modes:**
- `standard`: Default retention (90/180/365 days)
- `compliant`: Extended retention for regulated industries (∞)
- `permissive`: Minimal retention (30 days)
- `none`: No audit logging (privacy-sensitive environments)

**Cleanup Schedule:** Daily at 02:00 UTC (configurable)

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_023/engineer.md`

---

## 8. Integration with Layer 1

### 8.1 Committing an Extractor

When user approves a Pathfinder draft:

1. Draft file moved from `drafts/` to `extractors/`
2. Entry created in `scout_extractors` table:
   ```sql
   INSERT INTO scout_extractors (
       name, source_path, source_hash, associated_tag, priority, enabled
   ) VALUES (
       'healthcare_path',
       '~/.casparian_flow/extractors/healthcare_path.py',
       'blake3_hash_of_file',
       'hl7',  -- Optional: only run for files with this tag
       100,
       TRUE
   );
   ```
3. Files with `extraction_status = 'PENDING'` are queued for extraction
4. Draft deleted from `drafts/` and `manifest.json`

### 8.2 Committing a Parser

When user approves a Parser Lab draft:

1. Draft file moved to `parsers/`
2. Entry created in `cf_parsers` table
3. If parser with same name exists:
   - Version MUST be higher (wizard auto-suggests next version)
   - User prompted: "This will create sales_parser v1.0.1. Continue?"
4. Backtest automatically queued for files matching parser's `topics`

### 8.3 Committing a Label

When user approves a Labeling suggestion:

1. **Tag the Signature Group** (primary mechanism):
   ```sql
   UPDATE cf_signature_groups
   SET label = 'Sales Transactions',
       labeled_by = 'ai',
       labeled_at = CURRENT_TIMESTAMP
   WHERE id = 'abc123';
   ```

2. **Propagate tag to current files** in the group:
   ```sql
   UPDATE scout_files
   SET tags = json_insert(tags, '$[#]', 'Sales Transactions')
   WHERE signature_group_id = 'abc123';
   ```

3. **Future files auto-inherit**: When Fingerprint Engine assigns a new file to signature group `abc123`, the file automatically inherits the `Sales Transactions` tag.

4. **Optional: Create path-based rule** (if user requests):
   - If AI detects a consistent path pattern across the group, offer to create an Extraction Rule
   - This provides redundant tagging (structure-based + path-based) for robustness

**Why this approach:**
- Labels persist based on **content structure**, not file location
- Moving files between folders doesn't break labeling
- New files with same structure are auto-tagged

---

## 9. Error Handling

### 9.1 Generation Failures

| Error Type | User Message | Recovery |
|------------|--------------|----------|
| Timeout | "AI took too long. Try a simpler sample?" | Retry with hint |
| Invalid syntax | "AI generated invalid code. Retry?" | Retry or manual edit |
| Empty response | "AI returned empty response. Retry?" | Retry |
| Model unavailable | "Model not loaded. Run: `ollama pull ...`" | Instructions shown |

### 9.2 Validation Failures

| Failure | User Message | Recovery |
|---------|--------------|----------|
| Parser crashes on sample | "Parser failed on row 15: ValueError" | Show error, offer hint |
| Extractor returns empty | "Extractor matched 0 files" | Show hint dialog |
| Type mismatch | "Column 'date' expected Date, got String" | Offer hint or manual fix |

### 9.3 Retry Limits

- Max 3 automatic retries per wizard invocation
- After 3 failures: "AI couldn't generate valid code. Edit manually?"
- User can always press `e` to edit manually at any point

### 9.4 LLM Output Validation Pipeline

LLM output goes through a **three-tier validation pipeline** before reaching the user:

```
LLM Output → [Tier 1: Syntax] → [Tier 2: Schema] → [Tier 3: Semantic] → Result
                   ↓                  ↓                   ↓
               Retry with         Retry with          Retry with
               error msg          schema hint         sample output
```

**Tier 1 - Syntax Validation:**
- YAML: Can be parsed by YAML parser
- Python: Can be parsed by `ast.parse()`
- Strip markdown code fences (```yaml, ```python) before validation

**Tier 2 - Schema Validation:**
- YAML: Conforms to extraction rule schema (see specs/extraction.md Section 3.1)
- Python: Has `extract(path: str) -> dict` signature, no dangerous imports

**Tier 3 - Semantic Validation:**
- Glob pattern matches at least some sample files
- Extraction produces non-empty output on sample
- Types match expected (if specified)

**Retry Budget Consumption:**
The 3-retry limit is a total budget consumed by any tier failure:
- First failure (any tier) → 2 retries remaining
- Second failure → 1 retry remaining
- Third failure → 0 retries, escalate to user

**Retry Context Enhancement:**
Each retry includes previous error in the LLM prompt:
```
Previous output had validation error:
  Tier 2 (Schema): Field 'from' value "segment-3" is invalid.
  Valid values: segment(N), filename, full_path, rel_path

Please regenerate with correct syntax.
```

**User Feedback During Validation:**
```
Validating output... (attempt 2/3)
├─ Tier 1 (Syntax): ✓
├─ Tier 2 (Schema): ✓
└─ Tier 3 (Semantic): checking...
```

**Escalation When Retries Exhausted:**

State transitions to ANALYSIS_ERROR with options:
- `[h]` Add hint: Give AI more context, adds 1 retry
- `[e]` Edit manually: Open in $EDITOR, bypasses AI
- `[r]` Retry fresh: Reset to ANALYZING with full budget
- `[Esc]` Cancel: Abandon wizard

---

## 10. MCP Tool Integration

AI wizards are exposed as MCP tools for Claude Code integration:

### 10.1 Primitive Tools (Layer 1)

```json
{
  "name": "read_sample",
  "description": "Read first N rows of a file for AI analysis",
  "parameters": {
    "file_path": "string",
    "rows": "integer (default: 10)",
    "redact_columns": "string[] (optional)"
  }
}
```

```json
{
  "name": "list_pending_review",
  "description": "List files/groups needing human attention",
  "parameters": {
    "type": "'unmatched_files' | 'unlabeled_groups' | 'failed_extractions'"
  }
}
```

### 10.2 Wizard Tools (Layer 2)

```json
{
  "name": "generate_extractor",
  "description": "Use Pathfinder wizard to generate an extractor from paths",
  "parameters": {
    "sample_paths": "string[]",
    "hint": "string (optional)",
    "auto_approve": "boolean (default: false)"
  },
  "returns": {
    "draft_id": "string",
    "code_preview": "string",
    "preview_results": "object[]"
  }
}
```

```json
{
  "name": "generate_parser",
  "description": "Use Parser Lab to generate a parser from sample file",
  "parameters": {
    "file_path": "string",
    "target_schema": "string (optional)",
    "hint": "string (optional)",
    "auto_approve": "boolean (default: false)"
  },
  "returns": {
    "draft_id": "string",
    "code_preview": "string",
    "validation_results": "object"
  }
}
```

```json
{
  "name": "commit_draft",
  "description": "Approve and commit an AI-generated draft",
  "parameters": {
    "draft_id": "string",
    "name": "string (optional - override suggested name)",
    "version": "string (optional - for parsers)"
  }
}
```

### 10.3 Semantic Path Wizard Tools (Layer 2)

```json
{
  "name": "recognize_semantic_path",
  "description": "Recognize semantic folder structure from sample file paths",
  "parameters": {
    "sample_paths": {
      "type": "string[]",
      "required": true,
      "description": "Sample file paths to analyze"
    },
    "source_id": {
      "type": "string",
      "required": false,
      "description": "Source ID if analyzing a specific source"
    },
    "hint": {
      "type": "string",
      "required": false,
      "description": "Optional hint about folder meaning"
    },
    "use_ai": {
      "type": "boolean",
      "default": false,
      "description": "Use AI for disambiguation (requires API key)"
    }
  },
  "returns": {
    "expression": "string - Semantic path expression",
    "confidence": "number - 0.0 to 1.0",
    "alternatives": [{
      "expression": "string",
      "confidence": "number"
    }],
    "segment_analysis": [{
      "position": "number - Segment position (negative from end)",
      "primitive": "string - Matched primitive name",
      "variant": "string - Variant of primitive",
      "sample_values": "string[]",
      "extracted_values": "object"
    }],
    "generated_rule": {
      "glob": "string",
      "extract": "object",
      "tag": "string"
    },
    "similar_sources": [{
      "source_id": "string",
      "semantic_expression": "string"
    }]
  }
}
```

```json
{
  "name": "generate_rule_from_semantic",
  "description": "Generate extraction rule from semantic path expression",
  "parameters": {
    "expression": {
      "type": "string",
      "required": true,
      "description": "Semantic path expression (e.g., 'entity_folder(mission) > dated_hierarchy(iso)')"
    },
    "source_id": {
      "type": "string",
      "required": false,
      "description": "Source ID to apply rule to"
    },
    "tag": {
      "type": "string",
      "required": false,
      "description": "Tag name for matched files"
    },
    "auto_apply": {
      "type": "boolean",
      "default": false,
      "description": "Immediately apply the generated rule"
    }
  },
  "returns": {
    "rule": {
      "name": "string",
      "glob": "string",
      "extract": "object",
      "tag": "string",
      "semantic_source": "string"
    },
    "preview": {
      "matching_files": "number",
      "sample_extractions": [{
        "path": "string",
        "extracted": "object"
      }]
    }
  }
}
```

```json
{
  "name": "list_semantic_primitives",
  "description": "List available semantic primitives in vocabulary",
  "parameters": {
    "domain": {
      "type": "string",
      "required": false,
      "description": "Filter by domain (core, healthcare, defense, user)"
    }
  },
  "returns": {
    "primitives": [{
      "name": "string",
      "description": "string",
      "domain": "string",
      "variants": "string[]",
      "example_patterns": "string[]"
    }]
  }
}
```

```json
{
  "name": "find_similar_sources",
  "description": "Find sources with semantically equivalent folder structure",
  "parameters": {
    "expression": {
      "type": "string",
      "required": true,
      "description": "Semantic path expression to match"
    }
  },
  "returns": {
    "equivalence_class": "string - Name of equivalence class if exists",
    "abstract_structure": "string - Parameterless semantic expression",
    "sources": [{
      "source_id": "string",
      "concrete_expression": "string",
      "file_count": "number"
    }]
  }
}
```

### 10.4 MCP Output Format Reference

**Problem:** The `code_preview` field in MCP tool returns is ambiguous - it could be YAML, Python, or pseudocode.

**Solution:** Standardized output format slots for semantic clarity:

**Format Categories:**

| Format | Field Names | Use Case | Validation |
|--------|-------------|----------|------------|
| **Code** | `code`, `code_language`, `code_description` | Executable source (parser.py) | Syntax check |
| **YAML Rule** | `yaml_rule`, `schema_version` | Declarative extraction rules | Schema validation |
| **Hybrid** | `yaml_rule` + `python_code` | Pathfinder (YAML-first) | Both validate |
| **Metadata** | `metadata`, `format_version` | Analysis results, reports | Schema-aware |

**Pathfinder Tool (`generate_extractor`) - Migrated:**

```json
{
  "returns": {
    "draft_id": "string",
    "yaml_rule": "string (valid YAML)",
    "python_code": "string (optional - if complex)",
    "decision_reason": "string (why code was needed)",
    "complexity": "simple | medium | complex",
    "preview_results": "object[]"
  }
}
```

**Parser Tool (`generate_parser`) - Migrated:**

```json
{
  "returns": {
    "draft_id": "string",
    "code": "string (Python)",
    "code_language": "python",
    "code_description": "string",
    "validation_results": "object",
    "estimated_complexity": "string"
  }
}
```

**Format Decision Logic (Pathfinder):**

```
Analyze extraction patterns:
  ├─ All patterns YAML-expressible? → return yaml_rule only
  └─ Any computed fields needed?    → return yaml_rule + python_code
                                      with decision_reason
```

**Client-Side Handling:**

```rust
// Pathfinder response
let rule = parse_yaml(&response.yaml_rule)?;
if let Some(code) = &response.python_code {
    log::info!("Complex logic needed: {}", response.decision_reason);
}

// Parser response
validate_python(&response.code)?;
assert!(response.code.contains("def parse("));
```

> **Full Specification:** See `specs/meta/sessions/ai_wizards/round_026/engineer.md`

---

## 11. Implementation Phases

### Phase 1: Foundation
- [ ] Create `drafts/` directory structure
- [ ] Implement draft manifest (JSON)
- [ ] Add `cf_ai_audit_log` table
- [ ] Add model configuration parsing

### Phase 2: Pathfinder Wizard
- [ ] Implement path analysis algorithm
- [ ] Add Ollama integration for code generation
- [ ] Create TUI dialog
- [ ] Add preview functionality
- [ ] Implement commit to `scout_extractors`

### Phase 3: Parser Lab Wizard
- [ ] Implement sample extraction
- [ ] Add code generation prompt template
- [ ] Create TUI dialog with validation
- [ ] Integrate with parser versioning
- [ ] Implement commit to `cf_parsers`

### Phase 4: Labeling Wizard
- [ ] Implement header/sample extraction
- [ ] Add classification prompt template
- [ ] Create TUI dialog
- [ ] Implement commit to tagging rules

### Phase 5: Semantic Path Wizard
- [ ] Integrate semantic vocabulary from `specs/semantic_path_mapping.md`
- [ ] Implement recognition algorithm wrapper
- [ ] Create TUI dialog for semantic recognition
- [ ] Add segment visualization
- [ ] Similar sources display
- [ ] MCP tools: `recognize_semantic_path`, `generate_rule_from_semantic`
- [ ] MCP tools: `list_semantic_primitives`, `find_similar_sources`

### Phase 6: Polish
- [ ] Redaction UI
- [ ] Hint system refinement
- [ ] Manual edit integration ($EDITOR)
- [ ] Audit CLI commands
- [ ] Cross-wizard integration (Semantic + Pathfinder hybrid mode)

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Build-time AI only | No runtime LLM calls | Determinism, scale, auditability |
| Drafts in filesystem | `~/.casparian_flow/drafts/` | Simple, inspectable, git-friendly |
| 24h draft expiry | Auto-cleanup old drafts | Prevent clutter |
| Ollama as default | Most common local LLM server | User familiarity |
| Qwen 2.5 for code | Best small coding model | Quality vs size tradeoff |
| Redaction opt-in | User toggles per column | Privacy control |
| Audit log required | Every LLM call logged | Compliance |
| **Semantic Path as fourth wizard** | Separate from Pathfinder | Different output (config vs code), higher abstraction |
| **Algorithmic recognition first** | AI only for disambiguation | Layer 1 compatible, works offline |
| **Semantic output is YAML rule** | Not Python code | Declarative, portable, composable |
| **Pathfinder YAML-first** | YAML primary, Python fallback | Consistent with Extraction Rules consolidation (see extraction_rules.md §1.5) |
| **Python only for computed fields** | When YAML insufficient | Clear boundary: simple extraction vs complex transformation |
| **Embeddings for path clustering** | sentence-transformers + HDBSCAN | No training required; handles messy naming; CPU-friendly |
| **LLM for field naming only** | Phi-3.5 Mini for semantic names | Small model sufficient; deterministic rules are output |
| **Training data flywheel** | User approvals → future training | Self-improving system; no upfront labeling required |
| **Path normalization before embedding** | Strip IDs, keep structure | Privacy-preserving; focuses on patterns not values |

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 0.1 | Initial draft - Wizards architecture |
| 2026-01-12 | 0.2 | **Added Semantic Path Wizard (Section 3.4).** Fourth wizard for recognizing folder semantics and generating extraction rules. TUI dialog (Section 5.5). MCP tools (Section 10.3). Implementation phase 5. Cross-reference to specs/semantic_path_mapping.md. |
| 2026-01-12 | 0.3 | **Pathfinder YAML-first (Section 3.1).** Pathfinder now generates YAML Extraction Rules first, Python only as fallback for complex logic. Updated TUI dialog (Section 5.1) with dual-mode output. Consistent with Extraction Rules consolidation (extraction_rules.md §1.5). |
| 2026-01-13 | 0.4 | **Added Path Intelligence Engine (Section 3.5).** Foundational AI layer for path clustering (embeddings + HDBSCAN), field name intelligence (LLM), cross-source semantic equivalence, and single-file proposals. Powers other wizards. Training data flywheel for self-improvement. Implementation phases 1-6. |

---

**References:**
- `specs/discover.md` (TUI integration)
- `roadmap/spec_discovery_intelligence.md` (Iron Core / Fingerprinting)
- `CLAUDE.md` (Parser interface, MCP tools)

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

#### 3.5.9 Privacy Considerations

| Data | Sent to Embedding Model | Sent to LLM |
|------|-------------------------|-------------|
| Full file paths | Yes (normalized) | Yes (sample only) |
| File contents | No | No |
| Actual field values | No | Only segment values |

**Path Normalization for Embedding:**

```python
def normalize_for_embedding(path: str) -> str:
    """Normalize path for embedding - removes sensitive specifics."""
    # /data/CLIENT-ABC/invoices/2024/Q1/inv_001.pdf
    # → "data CLIENT invoices 2024 Q1 inv pdf"

    parts = Path(path).parts
    tokens = []
    for part in parts:
        # Split on common separators
        subtokens = re.split(r'[-_.]', part)
        # Filter out pure numbers (potential IDs)
        subtokens = [t for t in subtokens if not t.isdigit() or len(t) == 4]  # Keep years
        tokens.extend(subtokens)
    return ' '.join(tokens)
```

#### 3.5.10 Implementation Phases

| Phase | Scope | Time Estimate |
|-------|-------|---------------|
| **Phase 1** | Embedding clustering (no LLM) | 3-4 days |
| **Phase 2** | Field naming with Phi-3.5 | 3-4 days |
| **Phase 3** | Cross-source equivalence | 2-3 days |
| **Phase 4** | Single-file proposals | 2-3 days |
| **Phase 5** | Training data flywheel | 1-2 weeks |
| **Phase 6** | Fine-tuned embeddings | 2-3 weeks (if Phase 1-4 validate) |

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

### 5.3 Hint Input

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

# Extraction API - Unified Specification

**Status:** READY FOR IMPLEMENTATION
**Version:** 1.2
**Parent:** spec.md
**Replaces:** extraction_rules.md, semantic_path_mapping.md
**Related:** specs/views/discover.md Section 13 (Glob Explorer TUI)
**Last Updated:** 2026-01-13

---

## 1. Overview

The Extraction API extracts structured metadata from file paths. It provides:

- **Tier 1 (Simple)**: Point at files, get rules. No YAML required.
- **Tier 2 (Advanced)**: Full YAML for power users.

### 1.1 Design Philosophy

1. **Example-first**: Primary interface is "show me example files"
2. **Template-first for single files**: One file → match templates. Multiple files → full inference.
3. **YAML is the escape hatch**: Advanced users can drop to YAML anytime
4. **Explicit over implicit**: Show confidence, let users decide

### 1.2 Target Users

| Vertical | Example Pattern | Fields Extracted |
|----------|-----------------|------------------|
| Defense | `mission_042/2024-01-15/*.csv` | mission_id, date |
| Healthcare | `ADT_Inbound/2024/01/15/*.hl7` | message_type, direction, date |
| Finance | `FIX_logs/2024/Q1/*.log` | year, quarter |
| Legal | `matter_2024-001/custodian_smith/*` | matter_id, custodian |

---

## 2. Tier 1: Simple API

### 2.1 Single-File Workflow (Template Matching)

When given one file, the system matches against built-in templates:

```bash
$ casparian extract /data/mission_042/2024-01-15/telemetry.csv

Analyzing 1 file...

  #1 Defense Mission (ISO dates)          ████████░░ 82%
     ├─ mission_id: "042" (from folder)
     └─ date: "2024-01-15" (ISO format)

  #2 Generic Project                      █████░░░░░ 48%

Select [1-2] or (m)ore files: 1

✓ Created rule "mission_data" - matches 1,247 files
```

### 2.2 Multi-File Workflow (Algorithmic Inference)

With 3+ files, full algorithmic inference runs:

```bash
$ casparian extract /data/patient_records/

Analyzing 423 files...

  Confidence: ████████████████░ 92%

  Detected:
    Segment 1: Variable → {mrn} (187 unique)
    Segment 2: Category → {type} (labs, imaging, notes)
    Segment 3: ISO Date → {date}

  Generated rule:
    glob: "patients/{mrn}/{type}/{date}_*.pdf"
    extract: { mrn, type, date }

Accept? [Y/n]:
```

### 2.3 Templates

Apply domain templates directly:

```bash
$ casparian extract --template defense /mnt/missions/
$ casparian extract --template healthcare /mnt/hl7_archive/
$ casparian extract --template finance /mnt/fix_logs/
$ casparian extract --template legal /mnt/ediscovery/
```

### 2.4 Built-in Templates

| Template | Glob Pattern | Extracted Fields |
|----------|--------------|------------------|
| `defense` | `**/mission_*/{date}/**/*` | mission_id, date |
| `healthcare` | `**/*_Inbound/{year}/{month}/{day}/*` | message_type, direction, year, month, day |
| `finance` | `**/FIX_logs/{year}/Q{quarter}/*` | year, quarter |
| `legal` | `**/matter_*/{custodian}/*` | matter_id, custodian |
| `generic-dated` | `**/{date}/**/*` | date |
| `generic-entity` | `**/{entity}_*/**/*` | entity_id |

### 2.5 CLI Commands (Tier 1)

```bash
# Point at file(s)
casparian extract /path/to/file.csv
casparian extract /path/a.csv /path/b.csv /path/c.csv

# Apply template
casparian extract --template defense /path/

# Non-interactive
casparian extract /path/file.csv --yes --tag my_data

# Preview only
casparian extract /path/file.csv --dry-run
```

---

## 3. Tier 2: Advanced API

### 3.1 YAML Schema

```yaml
version: 1
rules:
  - name: "Mission Telemetry"
    glob: "**/mission_*/????-??-??/*.csv"
    extract:
      mission_id:
        from: segment(-3)
        pattern: "mission_(\\d+)"
        type: integer
      date:
        from: segment(-2)
        type: date
    tag: mission_data                    # Base tag applied to all matches
    tag_conditions:                      # Conditional tags (optional)
      - if: "mission_id < 100"
        tag: legacy_missions
      - if: "date.year = 2024"
        tag: current_year
    priority: 100
```

### 3.2 Field Definition

| Property | Type | Description |
|----------|------|-------------|
| `from` | string | `segment(N)`, `filename`, `full_path`, `rel_path` |
| `pattern` | regex | Capture groups for extraction |
| `type` | string | `string`, `integer`, `date`, `uuid` |
| `normalize` | string | `lowercase`, `uppercase`, `strip_leading_zeros` |
| `default` | any | Value if extraction fails |

**Note:** Each field extracts a **single scalar value**. If a regex has multiple
capture groups, only group 1 is used. For multiple values, define multiple fields.

### 3.3 Segment Addressing

```
Path: /data/mission_042/2024-01-15/readings.csv
           ───────────  ──────────  ────────────
           segment(-3)  segment(-2)  segment(-1)
```

**When to use `segment()` vs `full_path`:**

| Pattern Type | Recommended `from` | Reason |
|--------------|-------------------|--------|
| Fixed depth (no `**`) | `segment(-N)` | Faster, simpler |
| Variable depth (has `**`) | `full_path` | Segment index varies |

Example: `**/mission_*/**/*.csv` should use `from: full_path` because
the mission folder could be at any depth.

### 3.4 Cross-Platform Path Handling

All paths are **normalized to Unix-style forward slashes** (`/`) before
glob matching or regex extraction. This ensures patterns like
`/mission_([^/]+)/` work on Windows, macOS, and Linux.

### 3.5 Tag-Only Rules

Rules without extraction (just tagging):

```yaml
- glob: "**/README.md"
  extract: null
  tag: documentation

- glob: "**/.gitignore"
  extract: null
  tag: config
```

### 3.6 Optional Semantic Syntax

Power users can write semantic expressions:

```yaml
# Semantic shorthand (power users)
- semantic: "entity_folder(mission) > dated_hierarchy(iso) > files"
  tag: mission_data

# System generates equivalent:
# glob: "**/mission_*/????-??-??/*"
# extract:
#   mission_id: { from: segment(-3), pattern: "mission_(.*)" }
#   date: { from: segment(-2), type: date }
```

**Available primitives:**
- `entity_folder(name)` - Folder with prefix + ID: `mission_042`
- `dated_hierarchy(format)` - Date folders: `iso`, `nested`, `quarter`
- `env_marker` - Environment: `prod`, `staging`, `dev`
- `direction_marker` - Flow: `Inbound`, `Outbound`
- `category_folder(values)` - Fixed set: `logs`, `data`, `config`
- `**` - Recursive descent

### 3.7 CLI Commands (Tier 2)

```bash
# Rule management
casparian rules list --source /path/
casparian rules export my_rule > rule.yaml
casparian rules import rule.yaml
casparian rules validate rule.yaml
casparian rules test rule.yaml /path/file.csv

# Enable/disable
casparian rules enable "Mission Telemetry"
casparian rules disable "Mission Telemetry"

# Semantic primitives
casparian semantic --list
casparian semantic --show entity_folder
```

---

## 4. Equivalence Classes

Sources with identical structure share rules automatically.

### 4.1 Structure Fingerprinting

The system computes fingerprints based on:
- Folder depth distribution
- Segment patterns (Fixed, Variable, Date, Numeric)
- Extension distribution
- Date format detected

### 4.2 Detection

```bash
$ casparian sources --analyze-equivalence

  Class A: "Mission-Dated" (3 sources, 94% similar)
    ├─ /data/mission_alpha
    ├─ /data/mission_bravo
    └─ /data/mission_charlie

  Class B: "Patient Records" (2 sources, 97% similar)
    ├─ /data/clinic_east
    └─ /data/clinic_west
```

### 4.3 Workflow

When adding a new source:

```bash
$ casparian sources add /data/clinic_north

  This source matches "Patient Records" class (96% similar)
  Other sources: clinic_east, clinic_west

  Apply shared rules? [Y/n]:
```

### 4.4 Managing Equivalence Classes

```bash
# Edit class rules (applies to all member sources)
casparian rules --class "Patient Records" --edit

# Remove source from class
casparian sources detach /data/clinic_north --class "Patient Records"
```

---

## 5. Inference Engine

### 5.1 Template Matching (Single File)

```python
def match_templates(path: Path) -> List[TemplateMatch]:
    matches = []
    for template in BUILT_IN_TEMPLATES:
        score = 0.0
        evidence = []

        # Check if extract patterns match
        for field, regex in template.extract_patterns:
            if regex.matches(path):
                score += 0.3
                evidence.append(f"Field '{field}' matched")

        # Check structural similarity
        sim = structural_similarity(path, template.examples)
        if sim > 0.5:
            score += sim * 0.4
            evidence.append(f"Structure {sim*100:.0f}% similar")

        # Domain keywords bonus
        if contains_domain_keywords(path, template.domains):
            score += 0.2

        if score > 0.3:
            matches.append(TemplateMatch(template, score, evidence))

    return sorted(matches, key=lambda m: -m.score)[:3]
```

### 5.2 Algorithmic Inference (3+ Files)

```python
def infer_from_samples(paths: List[Path]) -> InferredRule:
    # Tokenize paths into segments
    tokenized = [tokenize(p) for p in paths]

    # Analyze each segment position
    for position in range(max_depth):
        values = [t[position] for t in tokenized if len(t) > position]

        if all_same(values):
            mark_fixed(position, values[0])
        elif all_match_date(values):
            mark_date(position, detect_format(values))
        elif all_numeric(values):
            mark_numeric(position)
        else:
            mark_variable(position)

    # Generate glob + extract from analysis
    return generate_rule(segment_analysis)
```

### 5.3 Confidence Thresholds

| Confidence | Behavior |
|------------|----------|
| ≥80% | Accept recommended |
| 50-79% | Accept with warning |
| <50% | Prompt for more files or manual input |

### 5.4 AI-Assisted Inference (Optional)

> **Full Specification:** See `archive/specs/ai_wizards.md` Section 3.5 (Path Intelligence Engine)

When algorithmic inference has low confidence or files have inconsistent naming, the optional Path Intelligence Engine provides:

| Capability | When It Helps |
|------------|---------------|
| **Path Clustering** | 500 files → 5 clusters for batch rule creation |
| **Field Name Intelligence** | `segment2` → `client_name` (semantic naming) |
| **Cross-Source Equivalence** | `mission_042` and `msn-42` recognized as same structure |
| **Single-File Proposals** | Bootstrap extraction from 1 example (no 3+ requirement) |

**Key Design Principle:** AI proposes, deterministic rules are the output. The Path Intelligence Engine is Layer 2 (build-time) - it generates extraction rules that become Layer 1 (runtime) configuration.

```
                   AI Layer (optional)              Deterministic Layer
                   ─────────────────────            ──────────────────────
User files ───► Path Intelligence Engine ───► Extraction Rules (YAML)
                   • Embeddings                     • glob patterns
                   • Clustering                     • segment extraction
                   • LLM field naming               • type validation
```

---

## 6. Database Schema

```sql
-- Extraction rules
CREATE TABLE extraction_rules (
    id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES scout_sources(id),
    name TEXT NOT NULL,
    glob_pattern TEXT NOT NULL,
    semantic_source TEXT,          -- Optional semantic expression
    tag TEXT,
    priority INTEGER DEFAULT 100,
    enabled BOOLEAN DEFAULT TRUE,
    created_by TEXT NOT NULL,      -- 'template', 'inferred', 'manual'
    created_at TEXT NOT NULL,
    UNIQUE(source_id, name)
);

-- Extraction fields
CREATE TABLE extraction_fields (
    id TEXT PRIMARY KEY,
    rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    source_type TEXT NOT NULL,     -- 'segment', 'filename', 'full_path'
    source_value TEXT,             -- e.g., "-2" for segment(-2)
    pattern TEXT,
    type_hint TEXT DEFAULT 'string',
    normalizer TEXT,
    default_value TEXT,
    UNIQUE(rule_id, field_name)
);

-- Equivalence classes
CREATE TABLE equivalence_classes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    fingerprint TEXT NOT NULL,     -- JSON structure fingerprint
    created_at TEXT NOT NULL
);

-- Class membership
CREATE TABLE equivalence_members (
    class_id TEXT REFERENCES equivalence_classes(id),
    source_id TEXT REFERENCES scout_sources(id),
    similarity REAL NOT NULL,
    PRIMARY KEY (class_id, source_id)
);

-- Scout files metadata
ALTER TABLE scout_files ADD COLUMN metadata_extracted JSON;
ALTER TABLE scout_files ADD COLUMN matched_rule_id TEXT;

-- Field value aggregation (for metrics/histograms)
-- Pre-computed during extraction for efficient querying
CREATE TABLE extraction_field_values (
    id TEXT PRIMARY KEY,
    rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    field_value TEXT NOT NULL,
    file_count INTEGER NOT NULL DEFAULT 0,
    last_updated TEXT NOT NULL,
    UNIQUE(rule_id, field_name, field_value)
);

CREATE INDEX idx_field_values_rule ON extraction_field_values(rule_id);
CREATE INDEX idx_field_values_field ON extraction_field_values(rule_id, field_name);
CREATE INDEX idx_field_values_count ON extraction_field_values(rule_id, field_name, file_count DESC);

-- Tagging conditions for rules
CREATE TABLE extraction_tag_conditions (
    id TEXT PRIMARY KEY,
    rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    operator TEXT NOT NULL,          -- '=', '!=', '<', '>', '<=', '>=', 'contains', 'matches'
    value TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER DEFAULT 100,    -- Higher priority conditions evaluated first
    UNIQUE(rule_id, field_name, operator, value)
);

CREATE INDEX idx_tag_conditions_rule ON extraction_tag_conditions(rule_id);

-- JSON index for efficient metadata queries (SQLite JSON1 extension)
CREATE INDEX idx_files_metadata ON scout_files(json_extract(metadata_extracted, '$.mission_id'));
-- Note: Create additional indexes for frequently-queried fields
```

---

## 7. Performance

### 7.1 Targets

| File Count | Extraction Time |
|------------|-----------------|
| 10K | < 200ms |
| 100K | < 1s |
| 1M | < 8s |

### 7.2 Implementation

```rust
use globset::{GlobSet, GlobSetBuilder};

// Compile all rules into single DFA
let mut builder = GlobSetBuilder::new();
for rule in rules {
    builder.add(rule.glob.clone());
}
let glob_set = builder.build()?;

// Match all patterns in single pass per file
for file in files.par_iter() {
    let matches = glob_set.matches(&file.path);
    if !matches.is_empty() {
        // First match by priority wins (tie-breaker: name ASC)
        let winner = matches.iter()
            .min_by(|&a, &b| {
                rules[*a].priority.cmp(&rules[*b].priority)
                    .then_with(|| rules[*a].name.cmp(&rules[*b].name))
            })
            .unwrap();
        extract_metadata(file, &rules[*winner]);
    }
}
```

**Priority Collision:** If two rules have the same priority, `name` (alphabetical)
is the tie-breaker. This ensures deterministic behavior.

---

## 8. Error Handling

### 8.1 Extraction Status

| Status | Description |
|--------|-------------|
| `COMPLETE` | All fields extracted successfully |
| `PARTIAL` | Some fields failed (e.g., path too short) |
| `FAILED` | Rule matched but all extractions failed |
| `UNMATCHED` | No rule matched (normal, not an error) |

### 8.2 Status Triggers

| Event | Action |
|-------|--------|
| File discovered (scan) | Extract metadata, set status |
| Rule created | Re-extract matching files |
| Rule updated (glob/extract changed) | Invalidate + re-extract matching files |
| Rule deleted | Clear metadata for previously matched files |
| File modified (mtime change) | Re-extract on next scan |

### 8.3 Failure Logging

```sql
ALTER TABLE scout_files ADD COLUMN extraction_status TEXT;
ALTER TABLE scout_files ADD COLUMN extraction_failures JSON;
```

### 8.4 Coverage Report

```bash
$ casparian rules coverage --source /data/

  Rule "mission_data":
    ✓ Complete: 1,247 files (89%)
    ⚠ Partial: 45 files (3%)
    ○ Unmatched: 112 files (8%)
```

---

## 9. Implementation Phases

### Phase 1: Core Engine
- [ ] `ExtractionRule` and `ExtractionField` structs
- [ ] Glob compilation (globset crate)
- [ ] Segment extraction
- [ ] CLI: `casparian rules validate/test`

### Phase 2: Template System
- [ ] Built-in templates (6 templates)
- [ ] Template matching algorithm
- [ ] CLI: `casparian extract --template`

### Phase 3: Inference Engine
- [ ] Algorithmic inference (3+ files)
- [ ] Confidence scoring
- [ ] CLI: `casparian extract` (main command)

### Phase 4: Database Integration
- [ ] Schema migration
- [ ] Rule CRUD operations
- [ ] Scout scan pipeline integration

### Phase 5: Equivalence Classes
- [ ] Structure fingerprinting
- [ ] Automatic detection
- [ ] CLI: `casparian sources --analyze-equivalence`

### Phase 6: Semantic Syntax (Optional)
- [ ] Expression parser
- [ ] Code generator (semantic → glob + extract)
- [ ] CLI: `casparian semantic --list`

---

## 10. Migration

### From extraction_rules.md
- YAML schema: **Unchanged**
- CLI commands: **Unchanged**
- Database schema: **Extended** (equivalence tables)

### From semantic_path_mapping.md
- Primitives: **Kept** (internal use + optional syntax)
- Recognition: **Kept** (powers inference)
- Equivalence: **Kept** (simplified)
- AI features: **Deferred** to v2

---

## Appendix A: Template Definitions

### defense

```yaml
name: "Defense Mission Data"
glob: "**/[Mm]ission_*/{date}/**/*"
extract:
  mission_id:
    from: full_path
    pattern: "/[Mm]ission_([^/]+)/"
  date:
    from: full_path
    pattern: "/(\\d{4}-\\d{2}-\\d{2})/"
    type: date
```

### healthcare

```yaml
name: "Healthcare ADT Messages"
glob: "**/{type}_{direction}/{year}/{month}/{day}/*"
extract:
  message_type:
    from: full_path
    pattern: "/([A-Z]+)_(Inbound|Outbound)/"
  direction:
    from: full_path
    pattern: "_(Inbound|Outbound)/"
    normalize: lowercase
  year:
    from: full_path
    pattern: "/(\\d{4})/\\d{2}/\\d{2}/"
    type: integer
  month:
    from: full_path
    pattern: "/\\d{4}/(\\d{2})/\\d{2}/"
    type: integer
  day:
    from: full_path
    pattern: "/\\d{4}/\\d{2}/(\\d{2})/"
    type: integer
```

### finance

```yaml
name: "Finance FIX Logs"
glob: "**/FIX_logs/{year}/Q{quarter}/**/*"
extract:
  year:
    from: full_path
    pattern: "/FIX_logs/(\\d{4})/"
    type: integer
  quarter:
    from: full_path
    pattern: "/Q(\\d)/"
    type: integer
```

### legal

```yaml
name: "Legal Matter Documents"
glob: "**/matter_*/{custodian}/**/*"
extract:
  matter_id:
    from: full_path
    pattern: "/matter_([^/]+)/"
  custodian:
    from: full_path
    pattern: "/matter_[^/]+/([^/]+)/"
```

---

## Appendix B: Semantic Primitives

| Primitive | Pattern | Fields |
|-----------|---------|--------|
| `entity_folder(name)` | `{name}_*` | `{name}_id` |
| `dated_hierarchy(iso)` | `????-??-??` | `date` |
| `dated_hierarchy(nested)` | `????/??/??` | `year`, `month`, `day` |
| `dated_hierarchy(quarter)` | `????/Q?` | `year`, `quarter` |
| `env_marker` | `prod\|staging\|dev` | `environment` |
| `direction_marker` | `*_Inbound\|*_Outbound` | `direction`, `type` |
| `category_folder(vals)` | `val1\|val2\|...` | `category` |

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Initial unified spec. Merges extraction_rules.md + semantic_path_mapping.md. Two-tier API (simple + advanced). Template-first inference. Equivalence classes. |
| 2026-01-13 | 1.1 | **Tagging integration**: Added `tag_conditions` to YAML schema for conditional tagging based on extracted fields |
| 2026-01-13 | 1.1 | **Field aggregation**: Added `extraction_field_values` table for efficient field metrics (histograms, unique counts) |
| 2026-01-13 | 1.1 | **Tag conditions table**: Added `extraction_tag_conditions` table for persistent conditional tag rules |
| 2026-01-13 | 1.1 | **JSON indexing**: Added guidance for SQLite JSON1 indexes on frequently-queried metadata fields |
| 2026-01-13 | 1.1 | Cross-reference to Glob Explorer in discover.md for unified rule creation workflow |
| 2026-01-13 | 1.2 | **AI-Assisted Inference (Section 5.4)**: Added cross-reference to Path Intelligence Engine in ai_wizards.md for embedding-based clustering, semantic field naming, cross-source equivalence, and single-file proposals |

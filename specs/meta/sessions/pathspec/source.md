# PathSpec - Partial Specification

**Status:** PARTIAL - Contains unresolved gaps
**Parent:** spec.md
**Version:** 0.1-draft
**Last Updated:** 2026-01-09

---

## Document Purpose

This is a **partial specification** documenting the PathSpec architecture proposal. It transparently identifies:
- **RESOLVED**: Decisions made with clear rationale
- **GAP**: Missing details requiring specification
- **ISSUE**: Identified problems requiring resolution
- **OPEN QUESTION**: Design choices needing user input

---

## 1. Overview

### 1.1 Core Concept

PathSpec is a declarative YAML schema defining expected folder structure. It enables:
- Proactive structure definition (vs reactive regex matching)
- Automatic metadata extraction from path segments
- Anomaly detection when physical files deviate from expected structure
- Virtual "Logical View" organizing files by metadata

### 1.2 Layer Designation

**Layer:** 1 (Iron Core) - Works without AI
**Related:** `specs/ai_wizards.md` (Layer 2 - AI can generate PathSpecs)

---

## 2. Two-Stage Pipeline [RESOLVED]

**Decision:** PathSpec and Extractors are complementary, not competing.

```
┌─────────────────────────────────────────────────────────────────────┐
│                      TWO-STAGE PIPELINE                             │
│                                                                     │
│  Stage 1: PathSpec (Structure)          Stage 2: Extractors         │
│  ─────────────────────────────          ─────────────────────       │
│  • Fast (Rust, pure path parsing)       • Slow (Python, file I/O)   │
│  • Extracts: mission_id, date, type     • Extracts: schema, rows    │
│  • Runs on ALL files                    • Runs on MATCHED files     │
│  • Output: metadata_path JSON           • Output: metadata_content  │
│                                                                     │
│  Example path:                                                      │
│  /data/mission_042/2024-01-15/telemetry.csv                        │
│       ├── mission_id: "042"     (Stage 1)                          │
│       ├── date: "2024-01-15"    (Stage 1)                          │
│       └── columns: [...]        (Stage 2)                          │
└─────────────────────────────────────────────────────────────────────┘
```

**Rationale:** Users with structured folders (mission data, dated archives) get instant metadata from paths. Content extraction remains available for deeper analysis.

---

## 3. Logical View [RESOLVED]

**Decision:** Renamed from "Virtual CAS" to "Logical View"

**Rationale:** "CAS" (Content-Addressable Storage) implies hashing/deduplication. This view is metadata-organized, not content-addressed.

### 3.1 Definition

The Logical View is a **read-only virtual file system** that:
- Organizes files by extracted metadata (not physical location)
- Shows files grouped by mission, date, type, etc.
- Never moves physical files
- Enables "Virtual Repair" via metadata overrides

### 3.2 View Modes

| Mode | Description |
|------|-------------|
| Physical View | Traditional folder tree (current Scout view) |
| Logical View | Files organized by PathSpec-extracted metadata |
| Drift View | Diff showing physical vs logical discrepancies |

---

## 4. PathSpec Grammar [PARTIAL]

### 4.1 Proposed Rust Enum [RESOLVED]

```rust
pub enum PathNode {
    Static { name: String, children: Vec<PathNode>, optional: bool },
    Variable { var_name: String, type_def: SemanticType, children: Vec<PathNode> },
    OneOf { options: Vec<PathNode> },
    File { pattern: GlobPattern, tag: Option<String> },
    Recursive { children: Vec<PathNode> },
}
```

### 4.2 Example PathSpec [RESOLVED]

```yaml
pathspec:
  version: 1
  root:
    - name: data
      children:
        - variable: mission_id
          type: MissionID
          children:
            - variable: date
              type: DateISO
              children:
                - file: "*.csv"
                  tag: telemetry
                - file: "*.json"
                  tag: config
```

### 4.3 GAPS

| Gap ID | Description | Impact |
|--------|-------------|--------|
| **GAP-GRAMMAR-001** | Optional segment propagation unclear. If `optional: true` on a `Static` node, do children become conditional? | Cannot implement optional folders |
| **GAP-GRAMMAR-002** | `OneOf` ambiguity resolution. If multiple options match, which wins? First? Longest? Error? | Non-deterministic matching |
| **GAP-GRAMMAR-003** | `Recursive` depth limits unspecified. Max depth? Performance bounds? | Potential infinite recursion |
| **GAP-GRAMMAR-004** | Negative patterns missing. How to exclude `*.tmp`, `.git/`, etc.? | Cannot ignore unwanted files |
| **GAP-GRAMMAR-005** | Multi-root support. Can one PathSpec define multiple disconnected trees? | Single-source limitation |

---

## 5. Semantic Types [PARTIAL]

### 5.1 Built-in Types [RESOLVED]

| Type | Pattern | Normalization |
|------|---------|---------------|
| Integer | `\d+` | None |
| Alphanumeric | `[a-zA-Z0-9_-]+` | None |
| UUID | `[0-9a-f]{8}-...` | Lowercase |
| DateISO | `\d{4}-\d{2}-\d{2}` | None |
| DateYear | `\d{4}` | None |
| DateMonth | `\d{2}` or month names | Numeric (Jan→1) |
| MissionID | `mission_\d+` or `\d{3,}` | Extract numeric |

### 5.2 Custom Types [RESOLVED]

```yaml
# ~/.casparian_flow/custom_types.yaml
types:
  SatelliteID:
    pattern: "SAT-[A-Z]{2}-\d{4}"
    normalize: "uppercase"
    description: "Satellite identifier"
```

### 5.3 GAPS

| Gap ID | Description | Impact |
|--------|-------------|--------|
| **GAP-TYPE-001** | Normalization pipeline undefined. What order? Can normalizers chain? | Inconsistent normalization |
| **GAP-TYPE-002** | Type coercion rules missing. Can `Integer` auto-coerce to `DateYear`? | Ambiguous type inference |
| **GAP-TYPE-003** | Validation error handling. What happens when value doesn't match type? | Silent failure vs hard error |

---

## 6. Anomaly Detection [PARTIAL]

### 6.1 Anomaly Taxonomy [RESOLVED]

| Anomaly | Detection | Severity | Example |
|---------|-----------|----------|---------|
| **Orphan** | File exists but no PathSpec node matches | Warning | `/data/random.txt` |
| **Pattern Mismatch** | File matches node but fails type validation | Error | `mission_ABC/` when expecting Integer |
| **Unexpected Leaf** | File where folder expected | Error | `/data/mission_042` is a file |
| **Missing Mandatory** | PathSpec expects node but not found | Info | No `telemetry.csv` in mission folder |

### 6.2 ISSUES

| Issue ID | Description | Severity |
|----------|-------------|----------|
| **ISSUE-ANOMALY-001** | Proposed actions (Drag, Rename, Create) contradict read-only NAS constraint. Actions must be metadata overrides only. | HIGH |
| **ISSUE-ANOMALY-002** | "Missing Mandatory" requires knowing what's mandatory. No `required: true` in grammar. | MEDIUM |
| **ISSUE-ANOMALY-003** | Severity levels (Warning/Error/Info) have no defined consequences. Do Errors block processing? | MEDIUM |

---

## 7. Metadata Precedence [RESOLVED]

**Decision:** Three-tier precedence with manual overrides winning.

```
┌─────────────────────────────────────────────────────────┐
│                 METADATA PRECEDENCE                     │
│                                                         │
│  1. metadata_manual   (HIGHEST) - User overrides        │
│  2. metadata_path     - From PathSpec folder structure  │
│  3. metadata_content  (LOWEST) - From file extraction   │
│                                                         │
│  Merge Strategy: Higher tier keys override lower tier   │
│  Conflict Resolution: Last-write-wins within tier       │
└─────────────────────────────────────────────────────────┘
```

### 7.1 GAPS

| Gap ID | Description | Impact |
|--------|-------------|--------|
| **GAP-MERGE-001** | Merge vs Override not distinguished. Does `metadata_path.date` replace or merge with `metadata_content.date`? | Unpredictable results |
| **GAP-MERGE-002** | Array merging undefined. If both tiers have `tags: [...]`, concatenate or replace? | Data loss potential |
| **GAP-MERGE-003** | Null handling missing. Does `null` in higher tier clear lower tier value? | Ambiguous semantics |

---

## 8. Database Schema [PARTIAL]

### 8.1 Proposed Changes [RESOLVED]

```sql
-- Add to scout_files
ALTER TABLE scout_files ADD COLUMN metadata_path JSON;      -- From PathSpec
ALTER TABLE scout_files ADD COLUMN metadata_manual JSON;    -- User overrides
ALTER TABLE scout_files ADD COLUMN pathspec_node_id TEXT;   -- Which node matched
ALTER TABLE scout_files ADD COLUMN anomaly_type TEXT;       -- Orphan, Mismatch, etc.

-- New table for PathSpecs
CREATE TABLE scout_pathspecs (
    id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES scout_sources(id),
    yaml_content TEXT NOT NULL,
    version INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(source_id, version)
);
```

### 8.2 GAPS

| Gap ID | Description | Impact |
|--------|-------------|--------|
| **GAP-DB-001** | PathSpec versioning workflow undefined. How to upgrade? Rollback? | Stuck on bad PathSpec |
| **GAP-DB-002** | Migration strategy for existing `scout_files` rows missing | Breaking existing data |
| **GAP-DB-003** | Index strategy for JSON columns not specified | Query performance |

---

## 9. TUI Integration [GAP]

### 9.1 Missing Specifications

| Gap ID | Description |
|--------|-------------|
| **GAP-TUI-001** | No wireframes for PathSpec editor |
| **GAP-TUI-002** | No keybindings defined for view switching (Physical/Logical/Drift) |
| **GAP-TUI-003** | No specification for "Interactive Breadcrumb" component |
| **GAP-TUI-004** | No anomaly resolution workflow UI |
| **GAP-TUI-005** | No PathSpec validation feedback display |

---

## 10. AI Wizard Integration [GAP]

### 10.1 Missing Specifications

| Gap ID | Description |
|--------|-------------|
| **GAP-AI-001** | Pathfinder Wizard should generate PathSpecs, not just extractors. Not specified. |
| **GAP-AI-002** | No "Mad Libs" template system for partial AI fill-in |
| **GAP-AI-003** | No PathSpec suggestion workflow (AI proposes, user approves) |

---

## 11. Open Questions

These require user input before specification can proceed:

| ID | Question | Options |
|----|----------|---------|
| **OQ-001** | Should PathSpec be per-source or global? | A) Per-source, B) Global with source filters, C) Both |
| **OQ-002** | When `OneOf` has multiple matches, behavior? | A) First wins, B) Longest wins, C) Error |
| **OQ-003** | Should anomalies block file processing? | A) Never, B) Only Errors, C) Configurable |
| **OQ-004** | Allow inline type definitions or require registry? | A) Inline only, B) Registry only, C) Both |
| **OQ-005** | PathSpec file location? | A) In source folder, B) ~/.casparian_flow/, C) Both |

---

## 12. Implementation Phases [TENTATIVE]

These phases assume gap resolution:

### Phase 1: Core Grammar
- [ ] Implement `PathNode` enum
- [ ] YAML parser with validation
- [ ] Static + Variable nodes only
- [ ] Basic type matching (Integer, Alphanumeric, DateISO)

### Phase 2: Metadata Extraction
- [ ] Path → metadata_path JSON conversion
- [ ] Database schema migration
- [ ] Precedence merge logic

### Phase 3: Anomaly Detection
- [ ] Orphan detection
- [ ] Pattern Mismatch detection
- [ ] Anomaly storage in scout_files

### Phase 4: Logical View
- [ ] Virtual filesystem tree construction
- [ ] TUI view switching
- [ ] Drift View computation

### Phase 5: Advanced Grammar
- [ ] OneOf nodes (after OQ-002 resolved)
- [ ] Recursive nodes (after GAP-GRAMMAR-003 resolved)
- [ ] Optional propagation (after GAP-GRAMMAR-001 resolved)

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-09 | 0.1-draft | Initial partial spec from conversation analysis |

---

## Appendix A: Gap Summary

**Total Gaps:** 18
**Total Issues:** 3
**Open Questions:** 5

| Category | Count |
|----------|-------|
| Grammar | 5 |
| Types | 3 |
| Merge | 3 |
| Database | 3 |
| TUI | 5 |
| AI Integration | 3 |

**Blocking Gaps** (must resolve before implementation):
- GAP-GRAMMAR-001 (Optional propagation)
- GAP-GRAMMAR-002 (OneOf ambiguity)
- ISSUE-ANOMALY-001 (Read-only constraint)
- GAP-MERGE-001 (Merge vs Override)

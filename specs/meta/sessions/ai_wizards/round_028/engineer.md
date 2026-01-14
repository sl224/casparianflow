# GAP-HYBRID-001 Resolution: Hybrid Mode (Pathfinder + Semantic) Workflow

**Session:** round_028
**Gap:** GAP-HYBRID-001 - Hybrid mode (Pathfinder + Semantic) no workflow
**Priority:** MEDIUM
**Status:** RESOLVED
**Date:** 2026-01-13

---

## Executive Summary

GAP-HYBRID-001 identified that `specs/ai_wizards.md` Section 3.4 mentions hybrid mode (combining Pathfinder + Semantic Path wizards) but provides no actionable workflow specification. This resolution provides:

1. **Hybrid Mode Triggers** - When and why to use hybrid mode
2. **Workflow State Machine** - Complete state diagram with transitions
3. **Wizard Handoff Protocol** - How Semantic → Pathfinder transfer happens
4. **Combined Output Format** - Merging semantic + pathfinder results
5. **UI Presentation** - How results appear to the user

---

## 1. Understanding Hybrid Mode

### 1.1 Hybrid Mode Philosophy

**Definition:** Hybrid mode enables Semantic Path Wizard to extract fields from folder structure, then Pathfinder Wizard to handle remaining fields from filenames or special cases.

**Why this matters:**

Many real-world files have **two layers of structure**:

```
Layer 1: Folder Structure (Semantic)    → mission_id, date, experiment
Layer 2: Filename Encoding (Custom)     → sensor_type, reading_number, retry_count
```

Example:
```
/data/mission_042/2024-01-15/experiment_x/
  ├─ telemetry_001.csv       ← Filename encodes sensor_type + reading_number
  ├─ telemetry_002.csv
  └─ thermal_scan_001.csv
```

**In hybrid mode:**
1. **Semantic Path** extracts from folder: `mission_id=042`, `date=2024-01-15`, `experiment=x`
2. **Pathfinder** extracts from filename: `sensor_type=telemetry`, `reading_number=001`
3. **Combined Result**: Single extraction rule with all 5 fields

**Without hybrid:**
- Pure Semantic: Would miss filename fields (creates incomplete rule)
- Pure Pathfinder: Would regenerate folder extraction unnecessarily (less portable)

### 1.2 When to Trigger Hybrid Mode

Hybrid mode is triggered in these scenarios:

#### Scenario A: User Explicitly Requests (Primary)

User runs Semantic Path Wizard and sees high-confidence folder results but wants to add filename extraction.

```
Semantic Path Result (confidence 94%):
  ✓ mission_id (from segment 2)
  ✓ date (from segment 3, ISO format detected)
  ✓ experiment (from segment 4)

[✓] Approve as-is
[↓] Add filename extraction (→ hybrid)
[✗] Cancel
```

#### Scenario B: Semantic Detects Incomplete Coverage (Auto-Trigger)

Semantic Path Wizard recognizes folder structure is complete but filename contains additional structure.

**Algorithm:**

```
After semantic extraction, check remaining file content:

1. Sample filename (remove extension)
2. Does filename match any known patterns?
   - Date patterns (YYYY-MM-DD, DDMMMYY, etc.)
   - Numbers with leading zeros (sensor_001, reading_042)
   - Enum-like patterns (v1, v2, retry_a, retry_b)
   - Hash-like patterns (UUID, hex)

3. If filename has 2+ extractable patterns:
   → Show: "Filename contains additional fields. Extract?"
   → Option to launch Pathfinder on filename

4. If 0-1 patterns found:
   → Show results, don't suggest hybrid
```

#### Scenario C: Pathfinder Cascade (User Switches Wizards)

User starts with Semantic Path, approves it, then wants to run Pathfinder on remaining untagged files.

```
After Semantic approval:

Remaining files: 47 (extracted as semantic_pattern_1)
Untagged files: 12 (no folder structure, names only)

[Create more rules] → [Run Pathfinder on remaining]
                     ↓ (launches Pathfinder on remaining 12)
```

---

## 2. Hybrid Mode Triggers (Decision Tree)

```
┌─────────────────────────────────────────────────────────────────┐
│  User Initiates Semantic Path Wizard (via S, W→s, or menu)      │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                    [Run Analysis]
                           │
        ┌──────────────────┴──────────────────┐
        ▼                                     ▼
   CONFIDENCE                         FILENAME ANALYSIS
   ≥ 70%?                              Patterns Detected?
   │                                   │
   ├─ YES → Show semantic results      ├─ YES (2+) → Show "Add filename extraction?"
   │        [Approve] [Modify]          │           [Yes→Hybrid] [No→Accept]
   │        [Cancel]                    │
   │                                    └─ <2 → Skip hybrid offer
   │
   └─ NO → Offer alternatives:
           [Try Pathfinder] [Hints] [Rescan]
           │
           └─ User chooses [Try Pathfinder]
              → Launch Pathfinder (not hybrid, just fallback)
```

**Key principle:** Hybrid is only suggested when BOTH wizards add value.

---

## 3. Hybrid Mode Workflow State Machine

### 3.1 States (Enhanced with Hybrid)

```
WAITING
  ↓ [User triggers Semantic Wizard]
PRE_DETECTION
  ├─ [Confidence ≥70%, no filename patterns]
  │  → SEMANTIC_RESULTS
  │
  ├─ [Confidence ≥70%, 2+ filename patterns]
  │  → HYBRID_OFFERED
  │
  └─ [Confidence <70%]
     → FALLBACK_OPTIONS

SEMANTIC_RESULTS
  ├─ [Approve] → DRAFT_CREATED (final)
  ├─ [Add filename extraction] → HYBRID_OFFERED
  └─ [Cancel] → WAITING

HYBRID_OFFERED
  ├─ [Yes, extract filename] → HYBRID_PROCESSING
  ├─ [No, use semantic only] → SEMANTIC_RESULTS → [Approve]
  └─ [Cancel] → WAITING

HYBRID_PROCESSING
  ├─ Pathfinder extracts filename fields
  ├─ Merge with semantic results
  → HYBRID_RESULTS

HYBRID_RESULTS
  ├─ [Approve] → DRAFT_CREATED (final)
  ├─ [Modify] → HYBRID_EDITING
  ├─ [Edit manually] → YAML_EDITOR
  └─ [Cancel] → WAITING

HYBRID_EDITING
  └─ User reviews/adjusts extraction
     → HYBRID_RESULTS

FALLBACK_OPTIONS
  ├─ [Try Pathfinder] → Launch Pathfinder (non-hybrid)
  ├─ [Provide hints] → PRE_DETECTION (with hints)
  └─ [Rescan] → Increase file sample

DRAFT_CREATED
  └─ [Approval flow begins]
```

### 3.2 State Descriptions

#### HYBRID_OFFERED
- **When**: Semantic results (≥70% confidence) are ready AND filename contains extractable patterns
- **Display**: Results panel shows semantic fields with prompt
- **User action**: Choose to add filename extraction or keep semantic only
- **System action**: Wait for user selection

#### HYBRID_PROCESSING
- **When**: User confirms hybrid mode intent
- **Process**:
  1. Extract filename from all sampled files
  2. Run Pathfinder on filename component only
  3. Merge results (see Section 3.3 below)
- **Duration**: <2s for 5-sample extraction
- **Display**: Progress indicator "Analyzing filename patterns..."

#### HYBRID_RESULTS
- **When**: Pathfinder completes filename extraction
- **Display**: Combined extraction rule with both semantic + filename fields
- **User action**: Approve, modify, or cancel
- **Format**:
  - Primary: Merged YAML rule (if both components are YAML-expressible)
  - Fallback: Python (if filename component requires Python)

---

## 4. Wizard Handoff Protocol

### 4.1 Handoff Sequence (Semantic → Pathfinder)

```
┌─────────────────────────────────────────────────────────────────┐
│ SEMANTIC PATH WIZARD                                            │
│ • Input: folder paths                                           │
│ • Output: Semantic fields + folder structure expression         │
│ • Example: {mission_id, date, experiment} from /mission_*/YYYY-│
└──────────────────┬──────────────────────────────────────────────┘
                   │
        [Extract Semantic Fields]
                   │
        [Merge with sampled files' filenames]
                   │
        [Analyze filename patterns]
                   │
        [Handoff decision: Auto or User-triggered?]
                   │
      ┌────────────┴────────────┐
      ▼                         ▼
   [AUTO-TRIGGER]         [USER-TRIGGERED]
   (Confidence ≥70% +      (User clicks
    2+ filename patterns    "Add extraction")
      detected)
      │                         │
      └────────────┬────────────┘
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│ PATHFINDER WIZARD (Filename Mode)                               │
│ • Input: Extracted filenames (with extension)                   │
│ • Constraints: ONLY extract from filename, ignore folder path   │
│ • Output: Filename fields + extraction pattern                  │
│ • Example: {sensor_type, reading_number} from telemetry_001.csv│
└──────────────────┬──────────────────────────────────────────────┘
                   │
        [Extract Filename Fields]
                   │
        [Validate no conflicts with Semantic fields]
                   │
        [Merge results]
                   │
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│ MERGED RESULT                                                   │
│ • Source: Semantic (4 fields) + Pathfinder (3 fields)           │
│ • Combined extraction rule (YAML or Python fallback)            │
│ • Ready for user approval or manual editing                     │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Handoff Context Passing

When Semantic Path initiates handoff to Pathfinder:

```rust
/// Context passed from Semantic to Pathfinder in hybrid mode
pub struct HybridHandoffContext {
    /// Semantic fields already extracted (e.g., mission_id, date)
    pub semantic_fields: HashMap<String, FieldMapping>,

    /// Folder structure expression (e.g., "entity_folder(mission) > dated_hierarchy(iso)")
    pub semantic_expression: String,

    /// Confidence level of semantic results (70-100)
    pub semantic_confidence: u8,

    /// Sampled filenames (without folder path, with extension)
    /// Example: ["telemetry_001.csv", "sensor_042.csv"]
    pub filenames: Vec<String>,

    /// Source ID for audit trail
    pub source_id: String,

    /// Original file paths (for reference only, Pathfinder ignores)
    pub full_paths: Vec<String>,
}
```

### 4.3 Conflict Resolution

When merging Semantic + Pathfinder results, conflicts can occur:

#### Type A: Field Name Collision
**Problem**: Both wizards extract a field with the same name but different source.
```
Semantic extracts: date = "2024-01-15" (from /2024/01/15/)
Pathfinder tries: date = "2024-01-15" (from filename pattern)
```

**Resolution**: Keep semantic version (folder is more reliable). Log conflict.
```
Merged: date = "2024-01-15" (source: semantic)
Warning: Filename also contains date pattern - keeping semantic version
```

#### Type B: Field Value Mismatch
**Problem**: Both extract same field but different values on same file.
```
File: /mission_042/2024-01-15/telemetry_20240115_001.csv

Semantic: date = "2024-01-15" (from folder)
Pathfinder: date = "20240115" (from filename)
```

**Resolution**: Detect mismatch → Stop hybrid, ask user.
```
⚠ Conflict Detected:

  Field "date" has different values:
  • From folder: "2024-01-15" (ISO format)
  • From filename: "20240115" (YYYYMMDD format)

  [Keep semantic] [Keep filename] [Resolve manually]
```

#### Type C: Incompatible Output Types
**Problem**: Semantic outputs YAML-only, Pathfinder requires Python.
```
Semantic: YAML rule for folder extraction
Pathfinder: Requires Python (e.g., Q1→quarter computation)
```

**Resolution**: Escalate to Python. Keep both, generate unified Python extractor.
```
# Hybrid extractor (Python required due to filename complexity)
def extract(path: str) -> dict:
    # Semantic extraction (folder-based)
    parts = Path(path).parts
    result = {
        'mission_id': extract_mission_id_from_folder(parts),
        'date': extract_date_from_folder(parts),
    }

    # Pathfinder extraction (filename-based, may have complex logic)
    filename = Path(path).name
    result.update(extract_filename_fields(filename))

    return result
```

---

## 5. Combined Output Format

### 5.1 Hybrid Result Structure

After both wizards complete, the combined result has this structure:

```json
{
  "hybrid_mode": true,
  "source_id": "src_12345",
  "draft_id": "draft_abcd1234",

  "semantic": {
    "expression": "entity_folder(mission) > dated_hierarchy(iso) > files",
    "confidence": 94,
    "fields": {
      "mission_id": {
        "from": "segment(2)",
        "pattern": "mission_(\\d+)",
        "capture": 1,
        "type": "integer"
      },
      "date": {
        "from": "segment(3)",
        "type": "date",
        "format": "iso8601"
      },
      "experiment": {
        "from": "segment(4)",
        "type": "string"
      }
    }
  },

  "pathfinder": {
    "component": "filename",
    "fields": {
      "sensor_type": {
        "from": "filename",
        "pattern": "^([a-z_]+?)_\\d+",
        "capture": 1,
        "type": "string"
      },
      "reading_number": {
        "from": "filename",
        "pattern": "_([\\d]+)\\.",
        "capture": 1,
        "type": "integer"
      }
    },
    "requires_python": false
  },

  "merged": {
    "format": "yaml",  // or "python" if any component requires it
    "yaml_rule": "...",  // Full merged YAML rule (see below)
    "python_code": null,  // Only if format == "python"
    "field_count": 5,
    "conflicts": []  // Empty if no conflicts, see section 4.3
  },

  "preview": [
    {
      "filename": "mission_042/2024-01-15/experiment_x/telemetry_001.csv",
      "extracted": {
        "mission_id": 42,
        "date": "2024-01-15",
        "experiment": "x",
        "sensor_type": "telemetry",
        "reading_number": 1
      }
    },
    // ... more samples
  ]
}
```

### 5.2 Merged YAML Rule Format

The merged YAML rule combines both semantic and filename extraction:

```yaml
name: "mission_telemetry_hybrid"
source: "hybrid"  # Indicates Semantic + Pathfinder

# Semantic component (folder-based)
semantic:
  expression: "entity_folder(mission) > dated_hierarchy(iso) > files"

# Extraction rules (combined)
glob: "**/mission_*/[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/*/*.csv"

extract:
  # From Semantic (folder)
  mission_id:
    from: segment(2)
    pattern: "mission_(\\d+)"
    capture: 1
    type: integer

  date:
    from: segment(3)
    type: date
    format: iso8601

  experiment:
    from: segment(4)
    type: string

  # From Pathfinder (filename)
  sensor_type:
    from: filename
    pattern: "^([a-z_]+?)_\\d+"
    capture: 1
    type: string

  reading_number:
    from: filename
    pattern: "_([\\d]+)\\."
    capture: 1
    type: integer

tag: "telemetry_data"
priority: 100

# Audit trail
created_by: "hybrid_mode"
created_at: "2024-01-15T14:32:00Z"
semantic_wizard_confidence: 94
pathfinder_component: "filename"
```

### 5.3 Merged Python Fallback Format

If Pathfinder component requires Python (computed fields, conditional logic):

```python
"""
Hybrid extractor: Semantic (folder) + Pathfinder (filename)
Auto-generated by AI Wizards
Rule ID: rule_hybrid_001
"""

from pathlib import Path
from typing import Dict, Any
import re

def extract(path: str) -> Dict[str, Any]:
    """
    Extract metadata from path: combines folder structure (semantic)
    and filename encoding (pathfinder).
    """
    path_obj = Path(path)
    parts = path_obj.parts
    filename = path_obj.name

    result: Dict[str, Any] = {}

    # ===== SEMANTIC COMPONENT (Folder-based) =====
    # entity_folder(mission) > dated_hierarchy(iso) > files

    try:
        # mission_id from segment 2
        match = re.match(r'mission_(\d+)', parts[2])
        if match:
            result['mission_id'] = int(match.group(1))
    except (IndexError, ValueError):
        pass

    try:
        # date from segment 3 (ISO format)
        result['date'] = parts[3]  # Assume format validated by glob
    except IndexError:
        pass

    try:
        # experiment from segment 4
        result['experiment'] = parts[4]
    except IndexError:
        pass

    # ===== PATHFINDER COMPONENT (Filename-based) =====
    # Note: Python only for complex filename parsing

    # sensor_type from filename prefix
    match = re.match(r'^([a-z_]+?)_\d+', filename)
    if match:
        result['sensor_type'] = match.group(1)

    # reading_number from filename digits
    match = re.search(r'_(\d+)\.', filename)
    if match:
        result['reading_number'] = int(match.group(1))

    return result

# Metadata for runtime (required by casparian run)
name = 'mission_telemetry_hybrid'
version = '1.0.0'
topics = ['telemetry_data']
outputs = {
    'telemetry': ...  # Arrow schema
}
```

---

## 6. UI Presentation

### 6.1 Hybrid Offer Dialog (After Semantic Results)

**When shown**: Semantic Path wizard completes with ≥70% confidence AND filenames contain extractable patterns.

**Location**: Modal overlay on Discover mode

```
┌──────────────────────────────────────────────────────────┐
│  SEMANTIC PATH WIZARD - RESULTS                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ ✓ Folder Structure Detected (94% confidence)             │
│                                                          │
│   Fields extracted from folder:                          │
│   • mission_id   (from folder segment 2)                 │
│   • date         (from folder segment 3)                 │
│   • experiment   (from folder segment 4)                 │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ 💡 Additional Patterns Found in Filenames:               │
│                                                          │
│    Files contain structured names:                       │
│    • telemetry_001.csv → sensor_type, reading_number    │
│    • thermal_scan_042.csv → sensor_type, reading_number  │
│                                                          │
│    Would you like to extract these too?                  │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  [Yes, extract filename fields]  [Keep semantic only]   │
│                     ↓                      ↓              │
│               HYBRID_PROCESSING    Approve & create     │
│               (Pathfinder runs)      draft              │
│                                                          │
│  [Cancel]  [View samples]  [Edit hints]                 │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**Keyboard:**
- `y` or `Enter`: Yes, hybrid mode
- `n` or `→`: Keep semantic only
- `Escape`: Cancel
- `p`: Preview samples
- `h`: Edit hints

### 6.2 Hybrid Processing Animation

**While Pathfinder extracts filename fields:**

```
┌──────────────────────────────────────────────────────────┐
│  SEMANTIC PATH WIZARD - HYBRID PROCESSING                │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ Analyzing filename patterns...                           │
│                                                          │
│  ▁▂▃▄▅ Samples analyzed: 4/5                             │
│                                                          │
│ Pathfinder Extraction:                                   │
│  • sensor_type      from filename prefix                 │
│  • reading_number   from filename digits                 │
│  • retry_count      inferred from pattern                │
│                                                          │
│ Merging results...                                       │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### 6.3 Hybrid Results Display

**After both wizards complete:**

```
┌──────────────────────────────────────────────────────────┐
│  HYBRID EXTRACTION RULE - REVIEW                         │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ Combined Fields (5 total):                               │
│                                                          │
│ SEMANTIC (Folder Structure):                             │
│  ✓ mission_id         integer  (from segment 2)          │
│  ✓ date               date     (ISO format)              │
│  ✓ experiment         string   (from segment 4)          │
│                                                          │
│ PATHFINDER (Filename):                                   │
│  ✓ sensor_type        string   (prefix pattern)          │
│  ✓ reading_number     integer  (digit capture)           │
│                                                          │
│ Preview on sample files:                                 │
│  ✓ mission_042/2024-01-15/exp_x/telemetry_001.csv       │
│    → {mission_id: 42, date: 2024-01-15, ..., reading: 1}│
│                                                          │
│ Output format: YAML (no Python needed)                   │
│ Suggested tag: telemetry_data                            │
│                                                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  [✓ Approve & Create Draft]  [Edit rule]  [Cancel]      │
│                                                          │
│  [Preview full rule]  [Modify fields]                    │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### 6.4 Manual Edit Mode (Conflict Resolution)

**When conflicts are detected:**

```
┌──────────────────────────────────────────────────────────┐
│  HYBRID MODE - CONFLICT RESOLUTION                       │
├──────────────────────────────────────────────────────────┤
│                                                          │
│ ⚠ Field "date" appears in both components:               │
│                                                          │
│  Semantic (folder):  date = 2024-01-15                   │
│  Pathfinder (file):  date = 20240115 (different format!) │
│                                                          │
│  Choose which to use:                                    │
│                                                          │
│  [✓] Keep semantic (more reliable)                       │
│  [ ] Keep pathfinder (different capture)                 │
│  [ ] Keep both, rename pathfinder to: filename_date      │
│  [ ] Edit field manually                                 │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  [Continue]  [Cancel & restart]                          │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

---

## 7. Integration Points

### 7.1 TUI State Machine Integration

Hybrid mode integrates with the Discover mode state machine (specs/discover.md):

```
Current state in Discover:
  SEMANTIC_RESULTS (confidence ≥70%)
    │
    ├─ [Approve] → Draft Created (existing path)
    ├─ [Cancel] → Back to Discover
    │
    └─ [Add filename extraction] → NEW: HYBRID_OFFERED
                                      │
                                      ├─ [Yes] → HYBRID_PROCESSING
                                      │           → HYBRID_RESULTS
                                      │           → Draft Created
                                      │
                                      └─ [No] → SEMANTIC_RESULTS
                                               (back to original approval)
```

### 7.2 Database Integration

Hybrid results are stored with audit trail:

```sql
-- Extract rules table (existing, extended with hybrid fields)
INSERT INTO extraction_rules (
  rule_name,
  glob,
  extract_json,
  tag,
  created_by,
  created_at,

  -- NEW: Hybrid metadata
  hybrid_mode,           -- true/false
  semantic_confidence,   -- 70-100
  pathfinder_component,  -- "filename", "content", etc.
  components_yaml,       -- {"semantic": {...}, "pathfinder": {...}}
  conflicts_resolved,    -- []
  merged_from_draft_ids  -- [draft_semantic, draft_pathfinder]
) VALUES (...)
```

### 7.3 CLI Integration

Users can create hybrid rules via CLI (future enhancement):

```bash
# Manual hybrid creation (not auto-wizard, but composable)
casparian create-rule --hybrid \
  --semantic "entity_folder(mission) > dated_hierarchy(iso)" \
  --pathfinder "{sensor_type, reading_number}" \
  --from-files /data/mission_*/2024*/*.csv

# Approval flow
casparian approve-rule rule_hybrid_001 \
  --for-source mission_telemetry
```

---

## 8. Examples

### 8.1 Example 1: Healthcare HL7 Files

**Scenario**: User has HL7 messages in both folder structure and filenames.

```
/data/ADT_Inbound/2024/01/15/patient_001_msg_A01.hl7
/data/ADT_Inbound/2024/01/15/patient_042_msg_A08.hl7
/data/ADT_Outbound/2024/01/16/response_001_ack.hl7
```

**Semantic detects** (94% confidence):
- `direction` from folder (Inbound/Outbound)
- `date` from folder (2024/01/15)

**Pathfinder detects** (on filename):
- `patient_id` from patient_NNN
- `message_type` from msg_XXX

**Hybrid result:**
```yaml
name: "hl7_hybrid"
glob: "**/ADT_*/[0-9]*/*/*/*"
extract:
  direction:
    from: segment(2)
    pattern: "ADT_(Inbound|Outbound)"
    capture: 1
  date:
    from: segment(3-5)
    type: date_iso
  patient_id:
    from: filename
    pattern: "patient_(\\d+)"
    capture: 1
    type: integer
  message_type:
    from: filename
    pattern: "msg_([A-Z]+)"
    capture: 1
    type: string
```

### 8.2 Example 2: Mixed Format Dates (Conflict)

**Scenario**: Folder uses ISO dates, filename uses YYYYMMDD.

```
/data/2024-01-15/patient_20240115_001.csv
```

**Conflict**:
- Semantic extracts: `date = "2024-01-15"`
- Pathfinder tries: `date = "20240115"`

**User decides**: Keep semantic (ISO is canonical).

**Hybrid result**:
```yaml
extract:
  date:
    from: segment(2)
    type: date
    format: iso8601
    source: semantic
    note: "Filename also contains date in YYYYMMDD - using semantic version"
```

### 8.3 Example 3: Complex Filename → Python Fallback

**Scenario**: Filename contains quarter code that must expand to month range.

```
/missions/mission_042/Q1/telemetry_001_v2.csv
```

**Semantic** (folder):
- `mission_id`, `quarter`

**Pathfinder** (filename):
- `sensor_id`, `version`
- PLUS: User hints "expand Q1 to month range"

**Hybrid result** (escalated to Python):
```python
def extract(path: str) -> dict:
    # Semantic: mission_id, quarter from folders
    # Pathfinder: sensor_id, version from filename
    # Computed: start_month, end_month from quarter
    ...
```

---

## 9. Error Handling

### 9.1 Hybrid-Specific Errors

| Error | Cause | Resolution |
|-------|-------|-----------|
| `Pathfinder produced empty results` | No extractable patterns in filename | Revert to semantic-only, ask user for hints |
| `Conflict: Same field in both components` | Both wizards found same field | Show dialog (Section 6.4) |
| `Merge failed: Incompatible YAML` | Pathfinder output not valid YAML | Generate Python fallback |
| `Semantic incomplete for hybrid` | Semantic confidence <70% | Reject hybrid, offer pure Pathfinder |

### 9.2 Validation Rules

```rust
/// Validation checks before merging
pub fn validate_hybrid_merge(
    semantic: &SemanticResult,
    pathfinder: &PathfinderResult,
) -> Result<HybridResult, HybridError> {
    // Check 1: Semantic confidence threshold
    if semantic.confidence < 70 {
        return Err(HybridError::LowSemanticConfidence(semantic.confidence));
    }

    // Check 2: Pathfinder must add new fields (not duplicates)
    let semantic_fields: HashSet<_> = semantic.fields.keys().collect();
    let pathfinder_only: Vec<_> = pathfinder.fields.keys()
        .filter(|k| !semantic_fields.contains(k))
        .collect();

    if pathfinder_only.is_empty() {
        return Err(HybridError::NoNewFieldsFromPathfinder);
    }

    // Check 3: Detect conflicts and attempt resolution
    let conflicts = detect_conflicts(&semantic, &pathfinder);

    if !conflicts.is_empty() {
        // Log conflicts, require user resolution
        return Err(HybridError::ConflictsDetected(conflicts));
    }

    // Check 4: Both must be valid extraction outputs
    validate_semantic_rule(&semantic)?;
    validate_pathfinder_rule(&pathfinder)?;

    Ok(merge_results(semantic, pathfinder))
}
```

---

## 10. Testing Strategy

### 10.1 Hybrid Mode Test Cases

| Test | Scenario | Expected | Notes |
|------|----------|----------|-------|
| `hybrid_auto_trigger` | Semantic 94% + filename patterns | HYBRID_OFFERED shown | Auto-suggestion works |
| `hybrid_user_trigger` | User clicks "add extraction" | HYBRID_PROCESSING starts | Manual initiation works |
| `hybrid_merge_yaml` | Both components YAML-expressible | Merged YAML generated | No Python needed |
| `hybrid_merge_python` | Pathfinder requires Python | Escalate to Python | Fallback works |
| `hybrid_conflict_field_name` | Same field in both | Conflict dialog shown | User chooses resolution |
| `hybrid_conflict_value` | Different values for same field | Stop, ask user | Prevent silent corruption |
| `hybrid_empty_pathfinder` | No extractable filename patterns | Revert to semantic | Graceful fallback |
| `hybrid_low_semantic` | Semantic <70% confidence | Reject hybrid | Don't offer if semantic weak |

### 10.2 Integration Tests

```rust
#[test]
fn test_hybrid_workflow_semantic_to_pathfinder() {
    // 1. Semantic completes
    let semantic = run_semantic_wizard(files);
    assert_eq!(semantic.confidence, 94);

    // 2. Trigger hybrid
    let hybrid_ctx = HybridHandoffContext::from_semantic(&semantic);

    // 3. Pathfinder processes filename
    let pathfinder = run_pathfinder_wizard(&hybrid_ctx.filenames);

    // 4. Merge
    let merged = merge_hybrid(&semantic, &pathfinder)?;

    // 5. Validate result
    assert_eq!(merged.fields.len(), 5);
    assert!(merged.yaml_rule.contains("semantic:"));
    assert!(merged.preview.iter().all(|p| p.extracted.contains_key("mission_id")));
}

#[test]
fn test_hybrid_conflict_resolution_field_name_collision() {
    let semantic = SemanticResult {
        fields: map! { "date" => FieldMapping::from_segment(3) }
    };

    let pathfinder = PathfinderResult {
        fields: map! { "date" => FieldMapping::from_filename("\\d{8}") }
    };

    let conflicts = detect_conflicts(&semantic, &pathfinder);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].field_name, "date");

    // Resolution: keep semantic
    let merged = resolve_conflicts(&semantic, &pathfinder,
        vec![("date", ConflictResolution::KeepSemantic)]);
    assert_eq!(merged.fields["date"].source, "semantic");
}
```

---

## 11. Future Enhancements

### 11.1 Planned Improvements

1. **Three-way hybrid** (Semantic + Pathfinder + Content Parser)
   - Extract fields from file content (CSV headers, JSON schema)
   - Combine with path-based extraction

2. **Interactive field mapping**
   - Show unified field → extraction source diagram
   - Allow drag-and-drop field reordering

3. **Cross-wizard debugging**
   - Side-by-side comparison of what each wizard extracted
   - Trace each field to its source

4. **Suggestion ranking**
   - When multiple fields could come from either wizard
   - Rank by extraction confidence, user feedback history

---

## 12. Related Documentation

| Document | Section | Relevance |
|----------|---------|-----------|
| `specs/ai_wizards.md` | 3.1 (Pathfinder) | Pathfinder algorithm |
| `specs/ai_wizards.md` | 3.4 (Semantic Path) | Semantic algorithm |
| `specs/discover.md` | State machine | TUI integration |
| `specs/extraction_rules.md` | YAML schema | Rule format |
| Round 1 | Pathfinder state machine | Foundation |
| Round 4 | Semantic state machine | Foundation |
| Round 19 | Semantic invocation | Entry points |
| Round 26 | MCP output formats | Tool integration |

---

## 13. Implementation Checklist

- [ ] Add `HYBRID_OFFERED`, `HYBRID_PROCESSING`, `HYBRID_RESULTS` states to Discover state machine
- [ ] Implement filename pattern detection algorithm (Section 2)
- [ ] Implement `HybridHandoffContext` struct and passing mechanism
- [ ] Implement conflict detection logic (Section 4.3)
- [ ] Implement YAML merge algorithm (Section 5.2)
- [ ] Implement Python fallback generation (Section 5.3)
- [ ] Add hybrid offer dialog (Section 6.1)
- [ ] Add hybrid processing animation (Section 6.2)
- [ ] Add conflict resolution dialog (Section 6.4)
- [ ] Update database schema with hybrid audit fields
- [ ] Implement E2E test suite (Section 10)
- [ ] Update CLI commands for hybrid workflows
- [ ] Update MCP tools to support hybrid returns
- [ ] Document in user guide with examples

---

## 14. Success Criteria

Hybrid mode is successfully implemented when:

1. ✅ Users can trigger hybrid mode from Semantic Path results
2. ✅ Semantic + Pathfinder results merge correctly (no data loss)
3. ✅ Conflicts are detected and require explicit user resolution
4. ✅ Combined output is YAML when possible, Python when needed
5. ✅ Audit trail tracks which fields came from which wizard
6. ✅ All test cases pass (Section 10.2)
7. ✅ User guide includes hybrid workflow examples

---

## 15. Decision Log

| Decision | Rationale | Alternatives Rejected |
|----------|-----------|----------------------|
| Auto-trigger on filename patterns | Reduces clicks for common case | Always require user action (more tedious) |
| Keep semantic on field collision | Folders more reliable than filenames | Always keep pathfinder (risky) |
| Escalate to Python if needed | Handles complex filename logic | Force YAML (loses expressiveness) |
| Separate hybrid state machine | Clear workflow isolation | Merge into single state (confusing) |
| Show conflict dialog explicitly | Prevents silent data loss | Auto-resolve conflicts (risky) |

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial resolution for GAP-HYBRID-001 |


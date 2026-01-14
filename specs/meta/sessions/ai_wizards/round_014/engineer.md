# GAP-CONFIG-002 Resolution: Training Data Flywheel Storage

**Gap**: Section 3.5.7 describes a training data flywheel where user-approved rules become training data, but lacks storage details.

**Status**: Resolved

---

## 1. Storage Location

Training data resides in the main Casparian database following the Single Database Rule:

```
~/.casparian_flow/casparian_flow.sqlite3
```

All training tables use the `ai_` prefix to distinguish from other subsystems (scout_, schema_, backtest_).

---

## 2. Database Schema

### 2.1 Core Tables

```sql
-- Training examples from user-approved extraction rules
CREATE TABLE IF NOT EXISTS ai_training_examples (
    id TEXT PRIMARY KEY,                    -- UUID

    -- Rule reference
    rule_id TEXT NOT NULL,                  -- FK to scout_tagging_rules(id)
    rule_version INTEGER NOT NULL DEFAULT 1, -- Increments when rule is modified

    -- Approved glob pattern and extraction config
    glob_pattern TEXT NOT NULL,             -- e.g., "**/CLIENT-*/invoices/{year}/Q{quarter}/*"
    extraction_config_json TEXT NOT NULL,   -- JSON: field mappings, types, extraction methods

    -- Sample paths (sanitized for privacy)
    sample_paths_json TEXT NOT NULL,        -- JSON array of sanitized representative paths
    sample_count INTEGER NOT NULL,          -- Number of files this rule matched at approval

    -- Approval metadata
    approved_by TEXT,                       -- User identifier (or null if anonymous)
    approved_at INTEGER NOT NULL,           -- Milliseconds since epoch
    approval_source TEXT NOT NULL,          -- 'tui' | 'cli' | 'mcp' | 'api'

    -- Quality signals
    confidence_score REAL,                  -- 0.0-1.0, null if unknown
    user_corrections INTEGER DEFAULT 0,     -- Number of times user edited before approval

    -- Privacy
    privacy_mode TEXT NOT NULL,             -- 'strict' | 'standard' | 'permissive'
    redactions_applied_json TEXT,           -- JSON: list of redaction rules applied

    -- Lifecycle
    is_exported INTEGER DEFAULT 0,          -- Whether included in any export
    is_active INTEGER DEFAULT 1,            -- Soft delete
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Field mappings extracted from paths
CREATE TABLE IF NOT EXISTS ai_training_field_mappings (
    id TEXT PRIMARY KEY,                    -- UUID
    example_id TEXT NOT NULL REFERENCES ai_training_examples(id) ON DELETE CASCADE,

    -- Field definition
    field_name TEXT NOT NULL,               -- e.g., "client_id", "year", "quarter"
    field_type TEXT NOT NULL,               -- 'string' | 'integer' | 'date' | 'enum'

    -- Extraction method
    extraction_method TEXT NOT NULL,        -- 'segment' | 'pattern' | 'literal'
    segment_index INTEGER,                  -- For segment-based: position from end (-1, -2, etc.)
    regex_pattern TEXT,                     -- For pattern-based: capture group regex

    -- Training examples (sanitized values)
    sample_values_json TEXT NOT NULL,       -- JSON array of (original_sanitized, extracted) pairs
    value_count INTEGER NOT NULL,           -- Number of unique values observed

    created_at INTEGER NOT NULL
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_ai_training_examples_rule ON ai_training_examples(rule_id);
CREATE INDEX IF NOT EXISTS idx_ai_training_examples_approved ON ai_training_examples(approved_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_training_examples_active ON ai_training_examples(is_active);
CREATE INDEX IF NOT EXISTS idx_ai_training_field_mappings_example ON ai_training_field_mappings(example_id);
CREATE INDEX IF NOT EXISTS idx_ai_training_field_mappings_field ON ai_training_field_mappings(field_name);

-- Training data exports (for sharing/federation)
CREATE TABLE IF NOT EXISTS ai_training_exports (
    id TEXT PRIMARY KEY,                    -- UUID

    -- Export metadata
    export_format TEXT NOT NULL,            -- 'jsonl' | 'parquet' | 'arrow'
    export_path TEXT,                       -- File path if exported to disk

    -- Content summary
    example_count INTEGER NOT NULL,
    field_count INTEGER NOT NULL,

    -- Privacy certification
    privacy_mode TEXT NOT NULL,             -- Mode used for all included examples
    privacy_certified_at INTEGER,           -- When privacy review completed
    privacy_reviewer TEXT,                  -- Who certified (or 'automated')

    -- Provenance
    source_installation_id TEXT,            -- Anonymous installation UUID
    created_at INTEGER NOT NULL,

    -- Import tracking (for received exports)
    is_imported INTEGER DEFAULT 0,          -- True if this is an import, not local
    imported_from TEXT,                     -- Source identifier if imported
    imported_at INTEGER
);

-- Join table: which examples are in which exports
CREATE TABLE IF NOT EXISTS ai_training_export_examples (
    export_id TEXT NOT NULL REFERENCES ai_training_exports(id) ON DELETE CASCADE,
    example_id TEXT NOT NULL REFERENCES ai_training_examples(id) ON DELETE CASCADE,
    PRIMARY KEY (export_id, example_id)
);
```

### 2.2 Extraction Config JSON Schema

```json
{
  "version": "1.0",
  "fields": [
    {
      "name": "client_id",
      "type": "string",
      "extraction": {
        "method": "segment",
        "segment_index": -5,
        "pattern": "CLIENT-(.*)"
      }
    },
    {
      "name": "year",
      "type": "integer",
      "extraction": {
        "method": "segment",
        "segment_index": -3
      }
    },
    {
      "name": "quarter",
      "type": "integer",
      "extraction": {
        "method": "segment",
        "segment_index": -2,
        "pattern": "Q(\\d)"
      }
    }
  ],
  "glob": "**/CLIENT-*/invoices/{year}/Q{quarter}/*"
}
```

### 2.3 Sample Paths JSON Schema

```json
{
  "version": "1.0",
  "privacy_mode": "standard",
  "paths": [
    {
      "sanitized": "/[USER_HOME]/[CLIENT]/invoices/2024/Q1/inv_001.pdf",
      "redactions": [
        {"placeholder": "[USER_HOME]", "severity": "critical"},
        {"placeholder": "[CLIENT]", "severity": "high"}
      ]
    }
  ],
  "total_matched": 1234,
  "sample_method": "stratified"
}
```

---

## 3. Privacy Handling

### 3.1 Privacy Modes for Training Data

| Mode | Path Sanitization | Field Values | Export Allowed |
|------|-------------------|--------------|----------------|
| **strict** | Critical + High + Medium redacted | All values hashed | No |
| **standard** | Critical + High redacted | Sensitive values hashed | Yes, with review |
| **permissive** | Critical only redacted | Preserved (local only) | No |

### 3.2 Sanitization Before Storage

Training examples are sanitized at write time, not query time:

```rust
pub struct TrainingExampleBuilder {
    rule_id: String,
    paths: Vec<PathBuf>,
    privacy_mode: PrivacyMode,
}

impl TrainingExampleBuilder {
    pub fn build(self, sanitizer: &PathSanitizer) -> TrainingExample {
        let sanitized_paths: Vec<SanitizedPath> = self.paths
            .iter()
            .map(|p| sanitizer.sanitize(p, self.privacy_mode))
            .collect();

        // Sample up to 10 representative paths
        let sample_paths = self.select_representative_sample(&sanitized_paths, 10);

        TrainingExample {
            id: Uuid::new_v4().to_string(),
            rule_id: self.rule_id,
            sample_paths_json: serde_json::to_string(&sample_paths).unwrap(),
            privacy_mode: self.privacy_mode.as_str().to_string(),
            redactions_applied_json: self.collect_redactions(&sample_paths),
            // ...
        }
    }
}
```

### 3.3 Field Value Hashing

For sensitive field values (client IDs, project codes), store semantic hash rather than actual value:

```rust
pub fn hash_sensitive_value(value: &str, field_name: &str) -> String {
    // Deterministic hash preserves "same value = same hash" for pattern learning
    // while preventing value reconstruction
    let input = format!("{}:{}", field_name, value);
    let hash = blake3::hash(input.as_bytes());
    format!("HASH:{}", &hash.to_hex()[..16])  // Truncated for readability
}

// Example:
// "ACME-Corp" -> "HASH:a3b4c5d6e7f8g9h0"
// Training learns: "HASH:* at segment -3 = client_id"
```

### 3.4 No Raw Path Storage

Training data never stores:
- Full absolute paths
- Usernames, home directories
- Client/project identifiers (hashed instead)
- PHI indicators (MRN, SSN patterns)

The `redactions_applied_json` field documents what was redacted for audit purposes.

---

## 4. Export/Import for Sharing

### 4.1 Export Command

```bash
# Export training data for sharing
casparian ai training export \
    --format jsonl \
    --output training_data_2024.jsonl \
    --privacy-mode standard \
    --since 2024-01-01

# Export requires explicit privacy certification
casparian ai training export \
    --format parquet \
    --output training_data.parquet \
    --certify "Reviewed by: jsmith, No PII present"
```

### 4.2 Export Format (JSONL)

```json
{"version":"1.0","type":"training_example"}
{"id":"ex-123","glob":"**/invoices/{year}/**/*.pdf","fields":[{"name":"year","type":"integer","method":"segment","segment_index":-3}],"sample_count":500,"confidence":0.95}
{"id":"ex-124","glob":"**/reports/Q{quarter}_{year}/**","fields":[{"name":"quarter","type":"integer","method":"pattern","regex":"Q(\\d)"},{"name":"year","type":"integer","method":"pattern","regex":"_(\\d{4})"}],"sample_count":120,"confidence":0.88}
```

### 4.3 Import Command

```bash
# Import shared training data
casparian ai training import training_data_2024.jsonl

# Preview before import
casparian ai training import training_data_2024.jsonl --dry-run

# Import with trust level (affects how data influences local models)
casparian ai training import training_data.parquet --trust community
```

### 4.4 Trust Levels for Imported Data

| Trust Level | Weight in Training | Use Case |
|-------------|-------------------|----------|
| **verified** | 1.0 | Official Casparian releases |
| **organization** | 0.8 | Shared within org |
| **community** | 0.5 | Public community contributions |
| **experimental** | 0.2 | Untrusted/testing |

---

## 5. Integration Points

### 5.1 When Training Examples Are Created

```
User approves tagging rule (TUI/CLI/MCP)
           │
           ▼
    ┌─────────────────┐
    │ Rule saved to   │
    │ scout_tagging_  │
    │ rules           │
    └────────┬────────┘
             │
             ▼
    ┌─────────────────┐
    │ Training data   │
    │ extractor runs  │
    │ async           │
    └────────┬────────┘
             │
             ▼
    ┌─────────────────────────────────────┐
    │ 1. Collect matching file paths      │
    │ 2. Apply privacy sanitization       │
    │ 3. Extract field mappings           │
    │ 4. Store in ai_training_examples    │
    └─────────────────────────────────────┘
```

### 5.2 When Training Data Is Used

```
Embedding model fine-tuning (future)
           │
           ▼
    ┌─────────────────┐
    │ Query active    │
    │ training        │
    │ examples        │
    └────────┬────────┘
             │
             ▼
    ┌─────────────────────────────────────┐
    │ Build training corpus:              │
    │ - Glob patterns as "queries"        │
    │ - Field names as "labels"           │
    │ - Sample paths as "context"         │
    └────────┬────────────────────────────┘
             │
             ▼
    ┌─────────────────┐
    │ Fine-tune       │
    │ local embedding │
    │ model           │
    └─────────────────┘
```

---

## 6. CLI Commands

```bash
# List training examples
casparian ai training list
casparian ai training list --since 2024-01-01 --limit 50

# Show details of a training example
casparian ai training show ex-123

# Delete a training example (soft delete)
casparian ai training delete ex-123

# Show training statistics
casparian ai training stats

# Export training data
casparian ai training export --format jsonl --output data.jsonl

# Import training data
casparian ai training import data.jsonl --trust community

# Audit training data privacy
casparian ai training audit
casparian ai training audit --export-report audit_report.json
```

---

## 7. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Store sanitized paths only | Yes | Privacy-first; raw paths never stored |
| Hash sensitive values | Yes | Preserves pattern learning without exposing values |
| Separate tables for fields | Yes | Enables querying "find all year extractions" |
| JSONL export format | Primary | Human-readable, streamable, diff-friendly |
| Trust levels for imports | Yes | Community data should have less weight |
| No automatic export | Yes | Requires explicit user action for privacy |

---

## 8. Future Considerations

1. **Federated Learning**: Training data structure supports future federated learning where models train locally and only gradients are shared.

2. **Differential Privacy**: Export format can be extended to include differential privacy noise parameters.

3. **Schema Evolution**: `version` field in JSON schemas enables forward-compatible format changes.

4. **Embedding Cache**: Consider caching embeddings alongside training examples to avoid recomputation.

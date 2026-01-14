# Review: Round 003

**Date:** 2026-01-13
**Reviewer Role:** Validate engineer proposals against existing specs, assess feasibility, identify issues

---

## GAP-FIELD-001 Review: Field Inference Input

**Verdict:** APPROVED

### Assessment

The engineer's proposal correctly addresses the gap by specifying:

1. **Source of sample_paths**: Files matching the current glob pattern from the folder cache - this aligns with the existing `FolderCache` architecture in discover.md Section 13.2.

2. **Sampling strategy**: Stratified random sampling with hard limits (max 100, min 3) is reasonable:
   - The 100 sample limit is performance-appropriate given the <50ms UI responsiveness target (discover.md Section 13.2: "Load-time: <50ms")
   - Stratified sampling addresses the edge case of skewed distributions well

3. **UI feedback**: Showing "(from 100 of 47,293 files)" provides transparency to users about coverage

### Minor Observations (not blocking)

- The `FieldInferenceConfig` struct is well-designed and provides extensibility for future tuning
- The default of `SamplingStrategy::Stratified` aligns with the design philosophy of "better coverage" at slight performance cost

### Consistency Check

- Integrates with `FolderCache.files_matching(pattern)` as proposed in the implementation
- No conflicts with existing data model in discover.md Section 13.11

---

## GAP-TEST-001 Review: Test Execution Model

**Verdict:** APPROVED

### Assessment

The engineer's proposal provides a comprehensive async test architecture:

1. **Always async**: The rationale is sound - even 100 files with regex can take 500ms+, causing UI freeze. Always-async is simpler than conditional threshold logic.

2. **Cancellation model**: Per-file cancellation via `AtomicBool` is the correct approach:
   - Responsive cancel without complex interrupt handling
   - Clean integration with tokio's cooperative cancellation
   - Partial results on cancel provide useful feedback

3. **Progress display**: The proposed UI matches the patterns in discover.md Section 13.6 (Test State Layout):
   ```
   Progress: [=============>          ] 67%
   Files:    1,247 / 1,859
   Current:  /data/mission_042/2024-01-15/sensor.csv
   ```

4. **TestPhase enum**: The `Running`, `Complete`, `Cancelled`, `Error` states provide clear state machine definition:
   - `Running` includes `files_processed`, `files_total`, `current_file`, `started_at` - comprehensive progress info
   - `Cancelled` preserves `files_processed` for partial result display

### Implementation Consistency

- The use of `tokio::spawn_blocking` for CPU-bound extraction is correct (extraction is CPU-intensive regex work)
- Channel-based progress (`mpsc::Sender<TestProgress>`) aligns with CLAUDE.md ADR guidance: "Channels over locks"

### Minor Observation

The proposal mentions `spawn_blocking` inside an async spawn, which is correct but worth noting for review: the extraction function runs in blocking threads from tokio's blocking pool, preventing async task starvation.

---

## GAP-DATA-001 Review: RuleDraft vs extraction.md Schema Alignment

**Verdict:** APPROVED with minor observation

### Assessment

The engineer's proposal provides a comprehensive schema alignment:

1. **Authoritative source**: DB schema (extraction.md Section 6) as authoritative is the correct choice:
   - Database is the persistence layer
   - YAML is import/export format
   - TUI RuleDraft is the working draft

2. **Unified Rust types**: The proposed `RuleDraft`, `FieldDraft`, `FieldSource`, `FieldType` align with:
   - extraction.md Section 3.2 field definition (from, pattern, type, normalize, default)
   - extraction.md Section 6 database schema (extraction_rules, extraction_fields tables)

3. **Key mappings verified**:

   | RuleDraft field | extraction.md YAML | DB column |
   |-----------------|-------------------|-----------|
   | `glob_pattern` | `glob` | `glob_pattern` |
   | `fields: Vec<FieldDraft>` | `extract: HashMap` | `extraction_fields` table |
   | `base_tag` | `tag` | `tag` |
   | `priority` | `priority` | `priority` |
   | `tag_conditions` | `tag_conditions` | `extraction_tag_conditions` table |

4. **FieldSource enum**: Correctly maps to extraction.md Section 3.2 "from" values:
   - `segment(N)` -> `FieldSource::Segment(i32)`
   - `filename` -> `FieldSource::Filename`
   - `full_path` -> `FieldSource::FullPath`
   - `rel_path` -> `FieldSource::RelPath`

5. **YAML compat layer**: The `RuleYaml` struct provides clean import/export without exposing internal representation

### Verification Against extraction.md Section 6 DB Schema

```sql
-- extraction.md Section 6 defines:
CREATE TABLE extraction_fields (
    id TEXT PRIMARY KEY,
    rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    source_type TEXT NOT NULL,     -- 'segment', 'filename', 'full_path'
    source_value TEXT,             -- e.g., "-2" for segment(-2)
    pattern TEXT,
    type_hint TEXT DEFAULT 'string',
    normalizer TEXT,               -- PRESENT in schema
    default_value TEXT,            -- PRESENT in schema
    UNIQUE(rule_id, field_name)
);
```

The engineer noted `GAP-SCHEMA-001` about missing `normalizer` and `default_value` in the RuleDraft fields - but inspection of extraction.md Section 6 shows these columns ARE present in the `extraction_fields` table. The gap should be:

**Correction**: The `FieldDraft` struct in the engineer's proposal includes `normalizer: Option<Normalizer>` and `default_value: Option<String>` which aligns with the DB schema. The TODO comments in the `load()` function need updating, but this is implementation detail, not a spec gap.

### New Gap Status

- **GAP-SCHEMA-001**: Downgrade from LOW to RESOLVED - the columns exist in extraction.md Section 6

---

## GAP-NAV-001 Review: Return Path from Published State

**Verdict:** APPROVED

### Assessment

The engineer's proposal to return to Browse at root (clean slate) is well-reasoned:

1. **Mental model**: "Publish = done, what's next?" aligns with task completion psychology
2. **Simplicity**: No state to preserve, no edge cases about stale filter context
3. **Discoverability**: User can always access the rule via Rules Manager (`R`) or view job via `j`

### State Transition Table Verification

The proposed transitions:

| From State | Trigger | To State | Context Preserved |
|------------|---------|----------|-------------------|
| Published | Enter | Browse (root) | None - clean slate |
| Published | Esc | Browse (root) | None - clean slate |
| Published | `j` | Job Status | job_id passed to Jobs view |

This matches the state machine pattern in discover.md Section 13.3 where state transitions are explicit and predictable.

### Alternative Analysis

The engineer correctly rejected:
- **Return to Filtering with pattern**: Would show same files that now have a rule - confusing
- **Return to prefix (not root)**: More state to preserve, prefix might be arbitrary

### Implementation Clarity

The proposed `handle_published_key()` implementation is clear:
- Resets all navigation state (`current_prefix`, `pattern`, `selected_index`, etc.)
- Clears rule editing state (`rule_editor`, `test_state`, `publish_state`)
- Calls `refresh_folders()` to reload root view

---

## Overall Verdict

**APPROVED**

### Summary

All four gap resolutions are well-designed, consistent with existing specs, and implementation-ready. The engineer demonstrated thorough cross-referencing with extraction.md and discover.md. One minor correction: GAP-SCHEMA-001 should be marked RESOLVED rather than LOW priority since the `normalizer` and `default_value` columns already exist in the extraction.md Section 6 database schema.

### Recommendations for Spec Update

1. **Phase 18c**: Add sampling strategy specification from GAP-FIELD-001
2. **Phase 18d**: Add async test architecture and cancellation model from GAP-TEST-001
3. **Phase 18f**: Add the unified `RuleDraft` type definitions from GAP-DATA-001
4. **State machine section**: Add Published state transitions from GAP-NAV-001
5. **Remove GAP-SCHEMA-001** from gap inventory (it's resolved)

### Next Steps

With Round 3 approved, the session can proceed to spec integration - updating discover.md Phase 18 with these specifications.

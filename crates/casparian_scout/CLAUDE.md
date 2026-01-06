# Claude Code Instructions for Scout Crate

## Quick Reference

```bash
cargo test -p casparian_scout           # All tests
cargo test -p casparian_scout --test e2e  # E2E tests only
```

---

## Architecture Overview (v6.0 - Tag-Based)

Scout is the **File Discovery + Tagging** layer. It does NOT process files - it only:
1. Watches filesystem locations (sources)
2. Discovers files via polling
3. Assigns tags based on pattern rules
4. Hands off to Sentinel for actual processing

```
Scout (Discovery) → Tag → Sentinel (Processing) → Plugin → Sink
```

---

## Key Paradigm

**Tags, not Routes.**

Old model: `pattern → transform → sink` (Scout did everything)

New model:
- Scout: `pattern → tag`
- Sentinel: `tag → plugin subscription → plugin execution → sink`

This decoupling enables:
- Manual tag override (user clicks file, changes tag)
- Multiple plugins subscribing to same tag
- Tag assignment via pattern OR manual OR API

---

## Database Schema

Scout uses SQLite for state management. Key tables:

```sql
-- Sources: filesystem locations to watch
scout_sources (id, name, source_type, path, poll_interval_secs, enabled)

-- Tagging Rules: pattern → tag mappings
scout_tagging_rules (id, name, source_id, pattern, tag, priority, enabled)

-- Files: discovered files with tags and status
scout_files (id, source_id, path, rel_path, size, mtime, status, tag, error, sentinel_job_id)
```

### Parser Lab Tables (v6.0)

```sql
-- Parsers: top-level entity (no project wrapper)
parser_lab_parsers (
    id, name, file_pattern, pattern_type, source_code,
    validation_status, validation_error, validation_output,
    sink_type, sink_config_json, is_sample, created_at, updated_at
)

-- Test files: belong to parsers
parser_lab_test_files (id, parser_id, file_path, file_name, file_size, created_at)
```

---

## File Status Flow

```
pending → tagged → queued → processing → processed
                                      ↘ failed
```

| Status | Description |
|--------|-------------|
| `pending` | Discovered, awaiting tagging |
| `tagged` | Has tag, ready for processing |
| `queued` | Submitted to Sentinel |
| `processing` | Worker is processing |
| `processed` | Successfully processed |
| `failed` | Processing failed (with error message) |
| `skipped` | User skipped |
| `deleted` | Removed from source |

---

## Key Types

```rust
// From types.rs
pub struct TaggingRule {
    pub id: String,
    pub name: String,
    pub source_id: String,
    pub pattern: String,      // Glob pattern
    pub tag: String,          // Tag to assign
    pub priority: i32,        // Higher = evaluated first
    pub enabled: bool,
}

pub enum FileStatus {
    Pending,
    Tagged,
    Queued,
    Processing,
    Processed,
    Failed,
    Skipped,
    Deleted,
}
```

---

## Removed Code (v6.0)

The following were DELETED because Scout no longer processes files:
- `transform.rs` - Arrow/Parquet transformation
- `sink.rs` - Output writers
- `detect.rs` - Format detection
- `schema.rs` - Type inference

These are now Sentinel/Plugin concerns.

---

## Integration Points

### Integration with Sentinel

When a file is tagged and ready for processing:
1. Scout marks file status as "tagged"
2. UI or API calls to submit file to Sentinel's `cf_processing_queue`
3. Sentinel looks up plugin via `cf_plugin_config.subscription_tags`
4. Worker executes plugin on file
5. Scout's file status updated to "processed" or "failed"

The `sentinel_job_id` field in `scout_files` tracks the Sentinel job.

### Integration with Parser Lab

Parser Lab parsers can match Scout files via:
- `file_pattern` field matches Scout's file tags
- Full validation queries Scout DB for matching files
- Published parsers become Sentinel plugins

```
Scout discovers files    →    Parser Lab matches by tag
   └─ tag: "RFC_DB"      →    file_pattern: "RFC_DB"
   └─ tag: "access_logs" →    file_pattern: "access_logs"
```

---

## Common Tasks

### Add a New File Status

1. Add variant to `FileStatus` enum in `types.rs`
2. Add `as_str()` and `parse()` implementations
3. Update `DbStats` if it should be tracked
4. Update UI to show the new status

### Add a Tagging Rule Field

1. Add field to `TaggingRule` struct in `types.rs`
2. Update `scout_tagging_rules` schema in `db.rs`
3. Update `upsert_tagging_rule` and `row_to_tagging_rule`
4. Update config.rs serialization tests
5. Update Tauri types in `ui/src-tauri/src/scout.rs`
6. Update frontend types in `ui/src/lib/stores/scout.svelte.ts`

### Add a Parser Lab Feature

1. Update schema in `db.rs` (add migration if needed)
2. Add Tauri command in `ui/src-tauri/src/scout.rs`
3. Register in `ui/src-tauri/src/lib.rs`
4. Update UI components
5. Run tests

---

## Testing

```bash
# Run all Scout tests
cargo test -p casparian_scout

# Run specific test
cargo test -p casparian_scout test_file_tagging

# Run e2e tests
cargo test -p casparian_scout --test e2e
```

---

## Current State (January 2025)

### Completed

- Scout crate fully refactored to tag-based model
- All unit tests pass (29 tests)
- All e2e tests pass (11 tests)
- Parser Lab redesigned (parser-centric, no project layer)
- Bundled sample parser for onboarding
- Tauri commands updated for new types
- Frontend UI updated

### Pending

- Scout → Sentinel bridge for full job submission
- Parser Lab → Plugin publishing flow
- Multi-file validation against Scout files

---

## Configuration (TOML)

```toml
database_path = "scout.db"
workers = 4

[[sources]]
id = "sample-data"
name = "Sample Data Source"
path = "data/"
poll_interval_secs = 30
enabled = true

[sources.source_type]
type = "local"

[[tagging_rules]]
id = "csv-files"
name = "CSV Files"
source_id = "sample-data"
pattern = "*.csv"
tag = "csv_data"
priority = 10
enabled = true
```

---

## Edge Cases

### No Plugin for Tag

- File gets tagged with the pattern's tag
- No plugin picks it up from queue
- UI shows "No plugin configured for tag 'X'"
- User can: create plugin subscription, or re-tag the file

### File Matches No Pattern

- File stays untagged (tag = NULL)
- Shown in UI as "Untagged files"
- User can manually tag or add a tagging rule

### Pattern Priority

Higher priority patterns are evaluated first:
```toml
# Priority 20 - matches first
pattern = "sales*.csv"
tag = "sales_data"
priority = 20

# Priority 10 - fallback
pattern = "*.csv"
tag = "csv_data"
priority = 10
```

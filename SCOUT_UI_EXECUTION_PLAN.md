# Scout UI Integration - Execution Plan

## Overview

Integrate Scout (Rust file discovery/transformation) with the Tauri UI to enable:
- Visual folder scanning and file discovery
- Live pattern matching preview
- Pattern suggestions from filenames
- Schema preview before processing
- Aggregate metrics for large file sets
- E2E processing workflow

---

## Architecture Decisions

### 1. Database Strategy

**Decision:** Keep Scout's separate database, open it from Tauri.

```
Tauri App
    ├── cf_* tables (existing Sentinel DB)
    └── scout.db (Scout's separate DB, opened on demand)
```

**Rationale:**
- Scout already works independently with its own DB
- No migration needed for existing Scout users
- Clean separation of concerns
- Jon/Casey: "Don't merge things that work separately"

**Implementation:**
- Tauri will manage a `ScoutDatabase` handle
- Path configurable (default: `scout.db` in app data dir)
- Lazy initialization on first Scout command

### 2. Tauri ↔ Scout Integration

**Decision:** Direct Rust function calls (no IPC, no HTTP).

```rust
// Tauri command calls Scout library directly
#[tauri::command]
fn scout_scan(source_id: String, state: State<ScoutState>) -> Result<ScanStats, String> {
    let db = state.database.lock().unwrap();
    let scanner = Scanner::new(db.clone());
    let source = db.get_source(&source_id)?;
    scanner.scan_source(&source).map_err(|e| e.to_string())
}
```

**Rationale:**
- Scout is a Rust crate, Tauri backend is Rust
- Zero serialization overhead for internal calls
- Type safety across the boundary
- Casey: "Same process, same language, just call the function"

### 3. State Management

**Decision:** Centralized ScoutState in Tauri, reactive ScoutStore in Svelte.

```rust
// Tauri side
struct ScoutState {
    database: Mutex<Option<ScoutDatabase>>,
    db_path: Mutex<PathBuf>,
}

// Svelte side
export const scoutStore = {
    sources: $state<Source[]>([]),
    files: $state<ScannedFile[]>([]),
    routes: $state<Route[]>([]),
    preview: $state<PreviewResult | null>(null),
    // ...
}
```

### 4. Live Preview Architecture

**Challenge:** Pattern matching 100k files on every keystroke.

**Solution:**
1. Debounce input (150ms)
2. Send pattern to Tauri
3. Tauri compiles glob once, iterates files in DB
4. Return aggregates, not full file list

```rust
#[tauri::command]
fn scout_preview_pattern(
    source_id: String,
    pattern: String,
    state: State<ScoutState>
) -> Result<PatternPreview, String> {
    // Returns: { matched_count, matched_bytes, sample_files: Vec<String> }
    // Does NOT return all 100k file paths
}
```

**Performance Target:** <50ms for 100k files

### 5. Large File List Handling

**Decision:** Aggregates by default, pagination for drill-down.

```typescript
interface RoutePreview {
    pattern: string;
    matchedCount: number;
    matchedBytes: number;
    sampleFiles: string[];  // First 5 files
    // Full file list only on demand via pagination
}
```

---

## Sharp Edges & Mitigations

| Sharp Edge | Risk | Mitigation |
|------------|------|------------|
| Blocking Tauri main thread | UI freezes during scan | Use `spawn_blocking` for DB ops |
| 100k files in memory | OOM, slow rendering | Aggregates + pagination |
| Pattern syntax errors | Crash on invalid glob | Validate pattern, return error |
| Schema inference on large file | Slow preview | Sample first N rows only |
| Concurrent scan + process | Race conditions | File-level locking in DB |
| DB path not found | Silent failure | Explicit error on init |

---

## Phase Breakdown

### Phase 1: Tauri Backend Commands

**Goal:** Wire Scout library to Tauri command system.

**Tasks:**
1. Add `casparian_scout` dependency to `ui/src-tauri/Cargo.toml`
2. Create `src-tauri/src/scout.rs` module
3. Implement ScoutState with lazy DB initialization
4. Implement core commands:
   - `scout_init_db(path: Option<String>)` - Initialize/open database
   - `scout_add_source(id, name, path)` - Add a source
   - `scout_list_sources()` - List all sources
   - `scout_remove_source(id)` - Remove a source
   - `scout_scan(source_id)` - Scan a source, return stats
   - `scout_list_files(source_id, status, limit, offset)` - Paginated file list
   - `scout_get_file_stats(source_id)` - Aggregate stats
   - `scout_add_route(id, name, source_id, pattern, output_path)` - Add route
   - `scout_list_routes()` - List all routes
   - `scout_remove_route(id)` - Remove route
   - `scout_preview_pattern(source_id, pattern)` - Live preview
   - `scout_process(source_id)` - Process pending files
   - `scout_get_status()` - Overall status/stats
5. Register commands in `main.rs`
6. Write integration tests (real DB, no mocks)

**Tests:**
- `test_scout_source_crud` - Add, list, remove sources
- `test_scout_scan_discovers_files` - Scan populates file table
- `test_scout_route_crud` - Add, list, remove routes
- `test_scout_preview_pattern` - Pattern matching works
- `test_scout_process_transforms_files` - End-to-end processing

**Review Criteria (Jon/Casey):**
- [ ] No async mutex held across await points
- [ ] Error messages are actionable
- [ ] No unnecessary allocations in hot paths
- [ ] Tests use real database, not mocks

---

### Phase 2: Svelte Scout Store

**Goal:** Reactive state management for Scout data.

**Tasks:**
1. Create `ui/src/lib/stores/scout.svelte.ts`
2. Define TypeScript interfaces matching Rust structs
3. Implement store with reactive state:
   - `sources`, `files`, `routes`, `stats`
   - `selectedSourceId`, `previewPattern`, `previewResult`
4. Create async functions that call Tauri commands
5. Implement debounced pattern preview
6. Error handling with user-friendly messages

**Interfaces:**
```typescript
interface Source {
    id: string;
    name: string;
    path: string;
    pollIntervalSecs: number;
    enabled: boolean;
}

interface ScannedFile {
    id: number;
    sourceId: string;
    path: string;
    relPath: string;
    size: number;
    status: 'pending' | 'processing' | 'processed' | 'failed';
}

interface Route {
    id: string;
    name: string;
    sourceId: string;
    pattern: string;
    outputPath: string;
    enabled: boolean;
    cleanup: 'none' | 'delete' | { archive: { path: string } };
}

interface PatternPreview {
    pattern: string;
    matchedCount: number;
    matchedBytes: number;
    sampleFiles: string[];
}

interface FileStats {
    totalFiles: number;
    totalBytes: number;
    pending: number;
    processed: number;
    failed: number;
    byExtension: Record<string, { count: number; bytes: number }>;
}
```

**Review Criteria (Jon/Casey):**
- [ ] No memory leaks (cleanup on unmount)
- [ ] Debounce prevents excessive API calls
- [ ] Loading states are explicit
- [ ] Errors surface to UI, not swallowed

---

### Phase 3: Basic Scout Tab UI

**Goal:** Functional UI for source management, file viewing, route editing.

**Tasks:**
1. Create `ui/src/lib/components/scout/` directory
2. Implement components:
   - `ScoutTab.svelte` - Main container with tab layout
   - `SourcePicker.svelte` - Dropdown + folder browser
   - `FileList.svelte` - Paginated file table with status colors
   - `RouteEditor.svelte` - Pattern input with live preview
   - `RouteList.svelte` - List of configured routes
   - `ProcessButton.svelte` - Trigger processing with confirmation
3. Add SCOUT tab to main navigation
4. Wire up to scout store
5. Basic styling (match existing cyberpunk theme)

**UI States:**
- Empty state (no sources configured)
- Loading state (scanning in progress)
- Populated state (files discovered)
- Error state (scan failed)

**Review Criteria (Jon/Casey):**
- [ ] Works with 0 files, 10 files, 10,000 files
- [ ] No layout thrashing during updates
- [ ] Keyboard accessible
- [ ] Error states are visible and actionable

---

### Phase 4: Aggregates & Metrics

**Goal:** Handle large file sets gracefully with aggregates.

**Tasks:**
1. Implement `scout_get_file_stats` command with:
   - Count by status (pending, processed, failed)
   - Count by extension
   - Size totals
   - Date range (oldest/newest)
2. Implement `scout_get_route_stats` command:
   - Files matched per route
   - Bytes matched per route
   - Overlap detection (files matching multiple routes)
3. Create `StatsPanel.svelte` component
4. Create `UnmatchedSummary.svelte` component
5. Add processing time estimates

**Metrics to Display:**
```
Source: /data/exports/
Last scan: 5 min ago

Files: 1,247 total (3.2 GB)
├── Pending: 1,195 (3.1 GB)
├── Processed: 50 (100 MB)
└── Failed: 2 (5 MB)

By Extension:
├── .csv: 899 files (2.2 GB)
├── .jsonl: 348 files (980 MB)
└── other: 0 files (0 B)

Routes:
├── R1 (*.csv): 899 files (2.2 GB)
├── R2 (*.jsonl): 348 files (980 MB)
└── Unmatched: 0 files (0 B)

Estimated processing time: ~3 min
```

**Review Criteria (Jon/Casey):**
- [ ] Aggregates computed in SQL, not application code
- [ ] Stats update after scan/process
- [ ] Numbers are human-readable (1.2 GB, not 1234567890)

---

### Phase 5: Pattern Suggestion

**Goal:** Infer useful patterns from discovered filenames.

**Tasks:**
1. Implement `scout_suggest_patterns` command:
   - Group files by extension
   - Find common prefixes within extension groups
   - Return suggested patterns with match counts
2. Algorithm:
   ```
   files: [sales_2024_01.csv, sales_2024_02.csv, report_q4.csv]

   Step 1: Group by extension
     .csv: [sales_2024_01, sales_2024_02, report_q4]

   Step 2: Find prefix groups (LCP or frequency analysis)
     sales_*: 2 files
     report_*: 1 file
     (or just *.csv: 3 files if no clear prefix)

   Step 3: Return suggestions
     [{ pattern: "sales_*.csv", count: 2, bytes: 1.2MB },
      { pattern: "*.csv", count: 3, bytes: 1.5MB }]
   ```
3. Create `SuggestedPatterns.svelte` component
4. One-click to add suggestion as route

**Review Criteria (Jon/Casey):**
- [ ] Suggestions are actually useful (not just `*`)
- [ ] Algorithm handles edge cases (no files, one file, etc.)
- [ ] Performance acceptable for 100k files

---

### Phase 6: Schema Preview & Dry Run

**Goal:** Show what the output will look like before committing.

**Tasks:**
1. Implement `scout_preview_schema` command:
   - Take a pattern, find first matching file
   - Run format detection
   - Run schema inference
   - Return column names, types, sample values
2. Implement `scout_dry_run` command:
   - Transform one file
   - Return record count, output size, schema
   - Don't write to sink
3. Create `SchemaPreview.svelte` component
4. Create `DryRunResult.svelte` component
5. Detect schema variance across files (optional)

**Schema Preview Output:**
```
File: sales_2024_01.csv
Format: CSV (comma-delimited, headers detected)

Column       Type       Sample
─────────────────────────────────
date         Date       2024-01-15
product      String     Widget A
quantity     Int64      10
unit_price   Float64    29.99
total        Float64    299.90

+ Lineage: _source_file, _route_name, _processed_at
```

**Review Criteria (Jon/Casey):**
- [ ] Schema inference matches actual processing
- [ ] Handles malformed files gracefully
- [ ] Dry run uses same code path as real processing

---

### Phase 7: E2E Demo

**Goal:** Demonstrate complete workflow with real data.

**Tasks:**
1. Create demo script that:
   - Launches Tauri app (or uses test harness)
   - Creates sample data folder
   - Adds source via UI
   - Scans folder
   - Adds routes with live preview
   - Runs dry run
   - Processes files
   - Verifies output
2. Record or document the demo flow
3. Create sample data with edge cases:
   - Multiple file types
   - Large files
   - Files with schema variance
   - Files that should be unmatched

**Demo Scenario:**
```
1. User opens Scout tab
2. Clicks "Add Source" → selects demo/scout/sample_data/
3. Clicks "Scan" → sees 4 files discovered
4. Sees suggested patterns: *.csv (2), *.json (1), *.jsonl (1)
5. Clicks to add *.csv route → sees 2 files highlighted
6. Types pattern *.jsonl → sees 1 file highlighted live
7. Adds route for *.jsonl
8. Sees summary: "3 files matched, 1 unmatched (*.json)"
9. Clicks "Preview Schema" → sees inferred columns
10. Clicks "Dry Run" → sees sample transformation
11. Clicks "Process" → files transform to Parquet
12. Verifies output files exist
```

---

## Test Strategy

### Critical Paths (No Mocks)

1. **Scan → DB → Query**: Real filesystem, real SQLite
2. **Pattern → Match → Files**: Real glob matching
3. **Transform → Arrow → Parquet**: Real file I/O
4. **Tauri Command → Scout Library**: Real function calls

### Test Categories

| Category | Location | Description |
|----------|----------|-------------|
| Unit (Scout) | `crates/casparian_scout/src/*/tests.rs` | Already exists (72 tests) |
| Integration (Scout) | `crates/casparian_scout/tests/e2e.rs` | Already exists (20 tests) |
| Integration (Tauri) | `ui/src-tauri/tests/scout_commands.rs` | NEW: Test Tauri commands |
| E2E (UI) | `ui/tests/scout.spec.ts` | NEW: Playwright tests |

### Test Data

```
ui/test-fixtures/scout/
├── small/           # 5 files for basic tests
├── mixed/           # Multiple formats
├── large/           # 1000 files for perf tests
└── edge-cases/      # Empty files, weird names, etc.
```

---

## Review Checklist (Jon Blow / Casey Muratori)

After each phase, verify:

### No Over-Engineering
- [ ] No unnecessary abstractions
- [ ] No "future-proofing" code
- [ ] Minimum viable implementation
- [ ] Could delete code and it would still work?

### Definition of Done
- [ ] Feature works end-to-end
- [ ] Tests cover critical paths
- [ ] Error handling is explicit
- [ ] No silent failures

### Performance
- [ ] Measured, not guessed
- [ ] Hot paths identified and optimized
- [ ] Memory usage is bounded
- [ ] UI remains responsive

### User Experience
- [ ] Immediate feedback on actions
- [ ] Clear error messages
- [ ] No mystery states
- [ ] Works with 0, 1, N items

---

## Execution Order

```
Phase 1: Tauri Commands     ───┐
         ↓ Review              │
Phase 2: Svelte Store       ───┤ Foundation
         ↓ Review              │
Phase 3: Basic UI           ───┘
         ↓ Review
Phase 4: Aggregates         ───┐
         ↓ Review              │
Phase 5: Pattern Suggestion ───┤ Enhancement
         ↓ Review              │
Phase 6: Schema Preview     ───┘
         ↓ Review
Phase 7: E2E Demo           ─── Final Verification
         ↓ Review
SHIP IT
```

---

## Notes

(Space for notes during implementation)


# Casparian Flow MVP - Parallel Execution Plan

**Goal:** Make jobs actually process, show status, capture failures, track versions.

**Philosophy:** Data-oriented. No abstractions. Direct code that does the thing.

---

## COMPACTION-SAFE ORCHESTRATION

**CRITICAL:** This plan uses parallel agents. To survive conversation compaction:

1. **Before spawning workers:** Create `ORCHESTRATION_CHECKPOINT.md`
2. **After each phase:** Update checkpoint with progress
3. **On resume:** Read checkpoint, continue from current_phase
4. **On completion:** Delete checkpoint or mark COMPLETED

See `CLI_PARALLEL_PLAN.md` for detailed checkpoint template.

---

## ORCHESTRATOR PROTOCOL (Main Claude)

**You are the manager. You spawn workers, track progress, ensure quality, merge code.**

### Phase 1: Setup
```bash
# Create worktrees (run these via Bash tool)
cd /Users/shan/workspace/casparianflow
git worktree add ../cf-w1 -b feat/job-loop
git worktree add ../cf-w2 -b feat/status-sync
git worktree add ../cf-w3 -b feat/failure-capture
git worktree add ../cf-w4 -b feat/versioning
```

### Phase 2: Spawn Workers
Spawn 4 Task agents with `run_in_background: true`, `subagent_type: "general-purpose"`.

Each agent gets:
1. Their workstream prompt from WORKER_PROMPTS.md
2. Instruction to work in their specific worktree directory
3. Instruction to commit when done with message "[W#] description"

**Spawn all 4 in a single message (parallel).**

### Phase 3: Monitor Progress
Use `TaskOutput` with `block: false` to check status every 30-60 seconds.

Track in your todo list:
- W1: pending → running → validating → done/failed
- W2: pending → running → validating → done/failed
- W3: pending → running → validating → done/failed
- W4: pending → running → validating → done/failed

### Phase 4: Validate Each Worker
When a worker reports done, verify their work:

**For W1 (job-loop):**
```bash
cd /Users/shan/workspace/cf-w1/ui && cargo check
```

**For W2 (status-sync):**
```bash
cd /Users/shan/workspace/cf-w2/ui && cargo check && bun run check
```

**For W3 (failure-capture):**
```bash
cd /Users/shan/workspace/cf-w3/ui && cargo check && bun run check
```

**For W4 (versioning):**
```bash
cd /Users/shan/workspace/cf-w4/ui && cargo check
```

If validation fails: note the error, consider respawning the agent with the error context.

### Phase 5: Merge (Strict Order)
Only after ALL workers pass validation:

```bash
cd /Users/shan/workspace/casparianflow

# 1. W1 first - foundation
git merge feat/job-loop --no-edit
cargo check -p casparian-deck  # or appropriate package

# 2. W4 next - independent
git merge feat/versioning --no-edit
cargo check -p casparian-deck

# 3. W3 - may conflict with W4 on db.rs
git merge feat/failure-capture --no-edit
# If conflict in db.rs: keep BOTH table creations
cargo check -p casparian-deck

# 4. W2 last - will conflict with W1 on lib.rs
git merge feat/status-sync --no-edit
# If conflict in lib.rs: keep job loop code AND sync command
cargo check -p casparian-deck && bun run check
```

### Phase 6: Final Verification
```bash
cd /Users/shan/workspace/casparianflow/ui
bun run build
bun run test:e2e
```

If tests fail: diagnose, fix directly, don't respawn workers.

### Phase 7: Cleanup
```bash
cd /Users/shan/workspace/casparianflow
git worktree remove ../cf-w1
git worktree remove ../cf-w2
git worktree remove ../cf-w3
git worktree remove ../cf-w4
git branch -d feat/job-loop feat/status-sync feat/failure-capture feat/versioning
```

### Failure Handling

**Worker fails validation:**
1. Read the error output
2. Decide: simple fix (do it yourself) or complex (respawn agent with error context)
3. For respawn: include the error message and what went wrong

**Merge conflict:**
1. Read both versions
2. Keep both changes (they're designed to not overlap logically)
3. If unclear, apply common sense - both features should exist

**Final tests fail:**
1. Read error output
2. Fix directly - you have full context of all changes
3. Don't respawn workers for integration issues

### Progress Reporting
After each phase, briefly report status:
```
=== Progress Update ===
W1 (job-loop): DONE - validated
W2 (status-sync): RUNNING
W3 (failure-capture): DONE - validated
W4 (versioning): DONE - validated
Next: waiting for W2, then merge phase
```

---

## Current State (Read This First)

What works:
- Scout discovers files, assigns tags
- Parser Lab tests parsers (Python subprocess)
- Jobs get created in `cf_processing_queue` with status='QUEUED'
- Worker spawning code exists (`process_job_async`)

What's broken:
- No polling loop picks up QUEUED jobs automatically
- Scout shows "queued" forever (no status sync back)
- Failures give no context (no line numbers, no surrounding code)
- No parser versioning (can't tell which code processed what)

---

## Database: The Actual Data

Single database: `~/.casparian_flow/casparian_flow.sqlite3`

### Existing Tables (Don't Break These)
```sql
cf_processing_queue (id, file_version_id, plugin_name, status, created_at, ...)
cf_plugin_manifest (plugin_name, version, source_code, status, ...)
parser_lab_parsers (id, name, source_code, validation_status, ...)
scout_files (id, source_id, path, tag, status, sentinelJobId, ...)
```

### New/Modified (What We're Adding)
```sql
-- W3: Failure capture
CREATE TABLE IF NOT EXISTS processing_failures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    file_path TEXT,
    line_number INTEGER,
    column_number INTEGER,
    error_type TEXT,
    error_message TEXT,
    context_before TEXT,      -- 5 lines before failure
    context_after TEXT,       -- 5 lines after failure
    stack_trace TEXT,
    raw_input_sample TEXT,    -- first 1KB of problematic input
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
);

-- W4: Parser versioning
ALTER TABLE parser_lab_parsers ADD COLUMN source_hash TEXT;

-- W4: Processing history (which version processed what)
CREATE TABLE IF NOT EXISTS processing_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    file_content_hash TEXT,
    parser_id TEXT,
    parser_source_hash TEXT,
    started_at TEXT,
    finished_at TEXT,
    status TEXT,
    output_path TEXT,
    rows_written INTEGER,
    FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
);
```

---

## Workstreams

### W1: Job Processing Loop
**Branch:** `feat/job-loop`
**Directory:** `../cf-w1`
**Files:** `ui/src-tauri/src/lib.rs`

**What to do:**
1. Add `start_job_processor(pool)` function
2. Spawns background thread with tokio runtime
3. Every 2 seconds: query for oldest QUEUED job
4. If found: UPDATE status to RUNNING, call existing `process_job_async`
5. Call `start_job_processor` from app setup (after pool is ready)

**Key code location:**
- Look at `setup_system_pulse()` around line 1500 for where to add startup
- `process_job_async()` already exists around line 998 - reuse it

**Done when:**
- `cargo check` passes
- Jobs with status='QUEUED' get picked up automatically
- No manual trigger needed

**Does NOT include:**
- Updating status to COMPLETED/FAILED (worker does this)
- Status sync to Scout (that's W2)

---

### W2: Status Sync (Sentinel → Scout)
**Branch:** `feat/status-sync`
**Directory:** `../cf-w2`
**Files:**
- `ui/src-tauri/src/lib.rs` (new Tauri command)
- `ui/src/lib/stores/scout.svelte.ts` (polling)
- `ui/src/lib/components/scout/ScoutTab.svelte` (trigger)

**What to do:**
1. Add Tauri command `sync_scout_file_statuses()`
   - Query `cf_processing_queue` for jobs with `sentinelJobId` in scout_files
   - For each: update scout_files.status based on job status
   - Map: QUEUED→queued, RUNNING→processing, COMPLETED→processed, FAILED→failed
   - If FAILED: copy error_message to scout_files.error

2. In scout store, add `syncStatuses()` method that invokes the command

3. In ScoutTab, call `syncStatuses()`:
   - On mount
   - Every 3 seconds while tab is active
   - After any file operation

**Done when:**
- `bun run check` passes
- Scout files show COMPLETED/FAILED status without manual refresh
- Error messages appear in Scout UI for failed jobs

**Coordinate with W1:**
- W1 changes lib.rs lines 1-600ish (job loop)
- W2 changes lib.rs lines 600+ (new command)
- Merge W1 first, then rebase W2

---

### W3: Failure Capture
**Branch:** `feat/failure-capture`
**Directory:** `../cf-w3`
**Files:**
- `ui/src-tauri/src/db.rs` (new table)
- `ui/src-tauri/src/scout.rs` (capture logic in validation)
- `ui/src/lib/components/scout/FileDetailPane.svelte` (display)

**What to do:**
1. Add `processing_failures` table to db.rs init
   ```sql
   CREATE TABLE IF NOT EXISTS processing_failures (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       job_id INTEGER NOT NULL,
       file_path TEXT,
       line_number INTEGER,
       column_number INTEGER,
       error_type TEXT,
       error_message TEXT,
       context_before TEXT,
       context_after TEXT,
       stack_trace TEXT,
       raw_input_sample TEXT,
       created_at TEXT DEFAULT CURRENT_TIMESTAMP
   );
   ```

2. In `parser_lab_validate_parser` (scout.rs ~line 1408):
   - When Python subprocess fails, parse the error
   - Extract line number from traceback (regex: `line (\d+)`)
   - Read the test file, get 5 lines before/after
   - Store in processing_failures table
   - Return structured error to UI

3. Add Tauri command `get_failure_details(job_id)` that returns failure info

4. In FileDetailPane.svelte:
   - If file.status === 'failed', show expandable failure details
   - Display: line number, error message, context with line highlighted
   - Add "Save as Test Case" button (creates parser_lab_test_file from this)

**Done when:**
- `cargo check` and `bun run check` pass
- Failed validation shows line number + context
- Can see exactly what broke and where

---

### W4: Parser Versioning
**Branch:** `feat/versioning`
**Directory:** `../cf-w4`
**Files:**
- `ui/src-tauri/src/db.rs` (schema changes)
- `ui/src-tauri/src/scout.rs` (hash on save)

**What to do:**
1. Add to db.rs init:
   ```sql
   -- Add column if not exists (SQLite way)
   ALTER TABLE parser_lab_parsers ADD COLUMN source_hash TEXT;

   CREATE TABLE IF NOT EXISTS processing_history (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       job_id INTEGER,
       file_path TEXT NOT NULL,
       file_content_hash TEXT,
       parser_id TEXT,
       parser_source_hash TEXT,
       started_at TEXT,
       finished_at TEXT,
       status TEXT,
       output_path TEXT,
       rows_written INTEGER
   );
   ```
   Note: ALTER TABLE will fail if column exists - wrap in try or check first

2. In `parser_lab_update_parser` (scout.rs):
   - Before saving, compute SHA256 of source_code
   - Store in source_hash column
   ```rust
   use sha2::{Sha256, Digest};
   let hash = format!("{:x}", Sha256::digest(source_code.as_bytes()));
   ```

3. In `parser_lab_validate_parser`:
   - After successful validation, create processing_history entry
   - Include parser_source_hash from the parser

4. Add `parser_lab_get_parser` response to include source_hash

**Done when:**
- `cargo check` passes
- Saving parser computes and stores hash
- processing_history tracks which version processed which file

**Coordinate with W3:**
- Both modify db.rs - coordinate on table creation order
- W3 adds processing_failures, W4 adds processing_history
- Can merge in either order, just resolve conflicts

---

## File Ownership Matrix

| File | W1 | W2 | W3 | W4 | Notes |
|------|----|----|----|----|-------|
| lib.rs:1-700 | PRIMARY | - | - | - | Job loop setup |
| lib.rs:700+ | - | PRIMARY | - | - | Status sync command |
| db.rs | - | - | SHARED | SHARED | W3: failures table, W4: history table |
| scout.rs:1-1250 | - | - | - | PRIMARY | Update parser save |
| scout.rs:1250-1500 | - | - | PRIMARY | - | Validation error capture |
| scout.svelte.ts | - | PRIMARY | - | - | Polling logic |
| ScoutTab.svelte | - | SECONDARY | - | - | Trigger sync |
| FileDetailPane.svelte | - | - | PRIMARY | - | Failure display |

---

## Merge Order

```bash
# After all workers report done:

# 1. W1 first - foundation, no conflicts expected
git checkout main
git merge feat/job-loop
cargo check  # verify

# 2. W4 next - independent, may conflict with W3 on db.rs
git merge feat/versioning
cargo check  # verify

# 3. W3 next - db.rs conflict resolution with W4
git merge feat/failure-capture
# Resolve db.rs: keep both table creations
cargo check  # verify

# 4. W2 last - lib.rs will conflict with W1
git merge feat/status-sync
# Resolve lib.rs: keep job loop + add sync command
cargo check && bun run check  # verify both

# 5. Final verification
bun run build
bun run test:e2e
```

---

## Success Criteria

After all merges:

1. **Jobs auto-process:** Create a job, wait 5 seconds, it's RUNNING/COMPLETED
2. **Scout shows status:** Without refresh, Scout files update to processed/failed
3. **Failures have context:** Failed job shows line number, surrounding code, stack trace
4. **Versions tracked:** Each parser save gets new hash, processing_history links to it

---

## Communication Protocol

No Slack. No Discord. Just this file.

When done with your workstream:
1. Run your checks (`cargo check` or `bun run check`)
2. Commit with message: `[W#] <what you did>`
3. Tell the human: "W# done, checks pass"

When blocked:
1. Update this file with what's blocking you
2. Tell the human

When you find a bug in another workstream's domain:
1. Note it here under "Issues Found"
2. Don't fix it yourself - tell the human

---

## Issues Found

(Workers add issues here as they find them)

---

## Notes

- All workers: Read the existing code before writing. Understand before changing.
- No new dependencies unless absolutely necessary
- No abstractions. Direct code.
- If unsure, ask. Don't guess.

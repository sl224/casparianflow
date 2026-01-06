# Worker Prompts

These are prompts for the 4 background Task agents. Main Claude spawns all 4 in parallel.

**How to spawn:** Main Claude uses Task tool with:
- `subagent_type: "general-purpose"`
- `run_in_background: true`
- `prompt: <the prompt below>`

---

## W1_PROMPT (Job Processing Loop)

```
WORKING DIRECTORY: /Users/shan/workspace/cf-w1/ui
BRANCH: feat/job-loop

You are Worker 1. Your task: Add automatic job processing loop.

FIRST: cd to your working directory and verify you're on the right branch:
cd /Users/shan/workspace/cf-w1/ui && git branch

CONTEXT:
- Jobs get created in cf_processing_queue with status='QUEUED'
- process_job_async(job_id) exists in src-tauri/src/lib.rs - it spawns workers
- But nothing calls it automatically - jobs sit in queue forever

WHAT TO BUILD in src-tauri/src/lib.rs:

1. Add function start_job_processor() that:
   - Spawns a background thread
   - Every 2 seconds: query for oldest QUEUED job
   - If found: UPDATE status='RUNNING', started_at=now
   - Then spawn worker via existing process_job_async logic

2. Call start_job_processor() from app setup (look for setup_system_pulse or similar init code)

IMPLEMENTATION SKETCH:
```rust
fn start_job_processor(pool: Arc<Pool<Sqlite>>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            loop {
                // Query oldest QUEUED job
                let job: Option<(i64,)> = sqlx::query_as(
                    "SELECT id FROM cf_processing_queue WHERE status = 'QUEUED' ORDER BY id LIMIT 1"
                )
                .fetch_optional(&*pool)
                .await
                .ok()
                .flatten();

                if let Some((job_id,)) = job {
                    // Mark as RUNNING
                    let _ = sqlx::query(
                        "UPDATE cf_processing_queue SET status = 'RUNNING', started_at = datetime('now') WHERE id = ?"
                    )
                    .bind(job_id)
                    .execute(&*pool)
                    .await;

                    // Spawn worker - adapt from existing process_job_async
                    // ...
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    });
}
```

CONSTRAINTS:
- Read the existing code first - understand before changing
- Don't modify the existing process_job_async function
- Don't add Scout status sync (that's W2's job)
- No new dependencies unless critical
- Direct code, no abstractions

WHEN DONE:
1. Run: cargo check
2. If passes: git add -A && git commit -m "[W1] Add job processing loop"
3. Report: "W1 DONE - cargo check passes, committed"

If cargo check fails, fix the errors and try again. Report the final status.
```

---

## W2_PROMPT (Status Sync)

```
WORKING DIRECTORY: /Users/shan/workspace/cf-w2/ui
BRANCH: feat/status-sync

You are Worker 2. Your task: Sync job status from Sentinel back to Scout.

FIRST: cd to your working directory and verify you're on the right branch:
cd /Users/shan/workspace/cf-w2/ui && git branch

CONTEXT:
- Jobs complete and update cf_processing_queue.status to COMPLETED/FAILED
- But scout_files.status stays "queued" forever - no sync back
- User has to manually refresh to see anything

WHAT TO BUILD:

1. In src-tauri/src/lib.rs, add Tauri command sync_scout_file_statuses():
   - Query scout_files WHERE status IN ('queued', 'processing') AND sentinelJobId IS NOT NULL
   - For each file, query cf_processing_queue by sentinelJobId
   - Update scout_files.status based on job status:
     * QUEUED → 'queued'
     * RUNNING → 'processing'
     * COMPLETED → 'processed'
     * FAILED → 'failed'
   - If FAILED, also copy error_message to scout_files.error column
   - Return count of updated files

2. Register the command in invoke_handler

3. In src/lib/stores/scout.svelte.ts, add method:
   async syncStatuses() {
     await invoke("sync_scout_file_statuses");
     if (this.currentSourceId) {
       await this.loadFiles(this.currentSourceId);
     }
   }

4. In src/lib/components/scout/ScoutTab.svelte:
   - On mount: call scoutStore.syncStatuses()
   - Set interval: every 3 seconds, call syncStatuses()
   - Clear interval on unmount

CONSTRAINTS:
- W1 is adding job loop code to lib.rs - you add status sync command
- These don't overlap logically but touch same file
- No new npm dependencies
- Read existing code patterns first

WHEN DONE:
1. Run: cargo check && bun run check
2. If passes: git add -A && git commit -m "[W2] Add status sync from Sentinel to Scout"
3. Report: "W2 DONE - cargo check and bun check pass, committed"

If checks fail, fix and retry. Report final status.
```

---

## W3_PROMPT (Failure Capture)

```
WORKING DIRECTORY: /Users/shan/workspace/cf-w3/ui
BRANCH: feat/failure-capture

You are Worker 3. Your task: Capture detailed failure context when parsers fail.

FIRST: cd to your working directory and verify you're on the right branch:
cd /Users/shan/workspace/cf-w3/ui && git branch

CONTEXT:
- When parser validation fails, we just get a generic error
- No line number, no surrounding context, no stack trace
- Users can't debug what broke

WHAT TO BUILD:

1. In src-tauri/src/db.rs, add table (find existing CREATE TABLE statements):
   CREATE TABLE IF NOT EXISTS processing_failures (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       job_id INTEGER,
       parser_id TEXT,
       test_file_id TEXT,
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

2. In src-tauri/src/scout.rs, find parser_lab_validate_parser function.
   When Python subprocess fails (non-zero exit or stderr has error):
   - Parse stderr for line number using regex: r"line (\d+)"
   - Read the test file content
   - Extract 5 lines before and 5 lines after the error line
   - Insert failure record into processing_failures table
   - Return structured error with line number in validation_error

3. Add Tauri command: get_processing_failure(parser_id, test_file_id) -> Option<ProcessingFailure>
   Returns the failure details for display

4. In src/lib/components/scout/FileDetailPane.svelte:
   - If file.status === 'failed', fetch and show failure details
   - Display: line number, error message, context (5 lines before/after)
   - Highlight the error line in the context display
   - Add "Save as Test Case" button

CONSTRAINTS:
- W4 also modifies db.rs - just add your table, no conflicts
- Don't change parser execution logic, only capture errors better
- Use regex crate for line number extraction

WHEN DONE:
1. Run: cargo check && bun run check
2. If passes: git add -A && git commit -m "[W3] Add failure capture with line context"
3. Report: "W3 DONE - checks pass, committed"

If checks fail, fix and retry. Report final status.
```

---

## W4_PROMPT (Parser Versioning)

```
WORKING DIRECTORY: /Users/shan/workspace/cf-w4/ui
BRANCH: feat/versioning

You are Worker 4. Your task: Track parser versions with content hashing.

FIRST: cd to your working directory and verify you're on the right branch:
cd /Users/shan/workspace/cf-w4/ui && git branch

CONTEXT:
- Parser source code changes over time
- No way to know which version processed which file
- Need version tracking for smart reprocessing later

WHAT TO BUILD:

1. Add sha2 to src-tauri/Cargo.toml:
   sha2 = "0.10"

2. In src-tauri/src/db.rs, add schema changes:
   -- Try to add column (ignore if exists)
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

   Note: ALTER TABLE fails if column exists. Wrap in try or check first:
   let _ = sqlx::query("ALTER TABLE parser_lab_parsers ADD COLUMN source_hash TEXT")
       .execute(&pool).await;  // Ignore error

3. In src-tauri/src/scout.rs, find parser_lab_update_parser.
   Before saving, compute hash:
   use sha2::{Sha256, Digest};
   let source_hash = format!("{:x}", Sha256::digest(source_code.as_bytes()));

   Include source_hash in the UPDATE statement.

4. Update ParserLabParser struct to include source_hash field.

5. In parser_lab_validate_parser, after successful validation:
   Insert into processing_history with the parser_source_hash.

CONSTRAINTS:
- W3 also modifies db.rs - just add your changes, they won't conflict
- Keep it simple - SHA256 of source_code string
- No over-engineering

WHEN DONE:
1. Run: cargo check
2. If passes: git add -A && git commit -m "[W4] Add parser versioning with source hash"
3. Report: "W4 DONE - cargo check passes, committed"

If check fails, fix and retry. Report final status.
```

---

## QUICK REFERENCE FOR ORCHESTRATOR

Spawn command pattern:
```
Task(
  description: "W1: Job processing loop",
  subagent_type: "general-purpose",
  run_in_background: true,
  prompt: <W1_PROMPT above>
)
```

Expected completion signals:
- W1: "W1 DONE - cargo check passes, committed"
- W2: "W2 DONE - cargo check and bun check pass, committed"
- W3: "W3 DONE - checks pass, committed"
- W4: "W4 DONE - cargo check passes, committed"

Validation commands:
- W1: `cd /Users/shan/workspace/cf-w1/ui && cargo check`
- W2: `cd /Users/shan/workspace/cf-w2/ui && cargo check && bun run check`
- W3: `cd /Users/shan/workspace/cf-w3/ui && cargo check && bun run check`
- W4: `cd /Users/shan/workspace/cf-w4/ui && cargo check`

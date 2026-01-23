# Casparian MCP E2E Test: Full EVTX DFIR Workflow

You are testing the complete Casparian Flow DFIR workflow for EVTX files. This is a **real integration test** - no mocking, no placeholders. Each step must produce real output that subsequent steps depend on.

## Critical Instructions

**REAL INTEGRATION TEST**: Every tool call must succeed with real data. If any step fails or returns an error, STOP immediately and report the failure. Do NOT continue to subsequent steps.

**FAIL FAST**: If a tool returns `is_error: true` or returns data that cannot be used by the next step, mark the test as FAILED and stop.

**CHAIN DEPENDENCIES**: Steps are chained - you MUST use real values from previous steps:
- Step 4 uses files discovered in Step 1
- Step 6 uses job_id from Step 4
- Step 8 uses approval_id from Step 7
- Step 10/11 query tables created by Step 9

## Important Constraints

1. **Use ONLY MCP tools** - Do not use Bash, Read, Write, or other non-MCP tools
2. **No placeholders** - Every value must be real output from a tool call
3. **Stop on error** - If any step fails, do not continue
4. **Return JSON result** - Your final output MUST be a valid JSON object

## Available MCP Tools

- `casparian_plugins` - List available parsers
- `casparian_scan` - Scan directory for files
- `casparian_preview` - Preview parser output (read-only)
- `casparian_query` - Read-only SQL query
- `casparian_backtest_start` - Start a backtest job
- `casparian_run_request` - Request parser execution (requires approval)
- `casparian_job_status` - Get job status/progress
- `casparian_job_cancel` - Cancel a job
- `casparian_job_list` - List recent jobs
- `casparian_approval_status` - Check approval status
- `casparian_approval_list` - List pending approvals
- `casparian_approval_decide` - Approve or reject a request

## Test Workflow (12 Steps)

### Phase 1: Discovery & Schema Proposal

#### Step 1: Discover EVTX Files
Call `casparian_scan`:
```json
{
  "path": "tests/fixtures/evtx",
  "recursive": true,
  "pattern": "*.evtx"
}
```
**REQUIRED OUTPUT**: Must return at least 1 EVTX file path
**FAIL IF**: No files found or `is_error: true`
**SAVE**: `evtx_files` array for later steps

#### Step 2: List Available Parsers
Call `casparian_plugins`:
```json
{
  "include_dev": true
}
```
**RECORD**: Whether `evtx_native` plugin exists
**Note**: Empty list is acceptable - we'll use path-based reference

#### Step 3: Preview Parser Output
Call `casparian_preview` with the EVTX file from Step 1:
```json
{
  "plugin_ref": {"path": "parsers/evtx_native"},
  "files": ["<first file from Step 1>"],
  "limit": 10
}
```
**REQUIRED OUTPUT**: Must return schema with columns
**FAIL IF**: No schema returned or `is_error: true`
**SAVE**: `schema_columns` for verification

### Phase 2: Backtest Validation

#### Step 4: Start Backtest Job
Call `casparian_backtest_start`:
```json
{
  "plugin_ref": {"path": "parsers/evtx_native"},
  "input_dir": "tests/fixtures/evtx"
}
```
**REQUIRED OUTPUT**: Must return a `job_id`
**FAIL IF**: No job_id returned or `is_error: true`
**SAVE**: `backtest_job_id`

#### Step 5: Poll Backtest Status
Call `casparian_job_status` with the job_id from Step 4:
```json
{
  "job_id": "<job_id from Step 4>"
}
```
Poll every 2 seconds until status is `completed`, `failed`, or `cancelled`.
Max 30 attempts.

**REQUIRED OUTPUT**: Job must reach terminal status
**FAIL IF**: Job never completes (timeout) or status is `failed`
**SAVE**: `backtest_status`, `files_processed`

### Phase 3: Approved Execution

#### Step 6: Request Parser Execution
Call `casparian_run_request`:
```json
{
  "plugin_ref": {"path": "parsers/evtx_native"},
  "input_dir": "tests/fixtures/evtx",
  "output": "./output/evtx_workflow"
}
```
**REQUIRED OUTPUT**: Must return `approval_id` with status `pending_approval`
**FAIL IF**: No approval_id or `is_error: true`
**SAVE**: `approval_id`

#### Step 7: Verify Approval is Pending
Call `casparian_approval_list`:
```json
{
  "status": "pending"
}
```
**REQUIRED OUTPUT**: Must find the approval from Step 6
**FAIL IF**: Approval not in pending list

#### Step 8: Approve the Request
Call `casparian_approval_decide` with approval_id from Step 6:
```json
{
  "approval_id": "<approval_id from Step 6>",
  "decision": "approve"
}
```
**REQUIRED OUTPUT**: Status must become `approved`
**FAIL IF**: Approval fails or `is_error: true`
**SAVE**: `job_id` if returned (job created after approval)

#### Step 9: Poll Run Job Status
If Step 8 returned a `job_id`, poll `casparian_job_status`:
```json
{
  "job_id": "<job_id from Step 8>"
}
```
Poll every 2 seconds until terminal status. Max 30 attempts.

**REQUIRED OUTPUT**: Job must reach terminal status
**SAVE**: `run_status`, `rows_processed`
**Note**: If no job_id was returned, skip to Step 10 but record this

### Phase 4: Timeline Queries

#### Step 10: Query Parsed Events
Call `casparian_query`:
```json
{
  "sql": "SELECT timestamp, event_id, channel, computer FROM evtx_events ORDER BY timestamp LIMIT 10",
  "limit": 10
}
```
**REQUIRED OUTPUT**: Must return rows if job completed, or valid empty response
**FAIL IF**: `is_error: true`
**SAVE**: `query_row_count`, `query_columns`

If table doesn't exist yet (job didn't write), try:
```json
{
  "sql": "SELECT name FROM sqlite_master WHERE type='table'",
  "limit": 100
}
```

#### Step 11: Verify Lineage Columns
If Step 10 returned data, call `casparian_query`:
```json
{
  "sql": "SELECT _cf_job_id, _cf_source_hash FROM evtx_events LIMIT 3",
  "limit": 3
}
```
**RECORD**: Whether lineage columns exist
**Note**: May fail if table doesn't exist - record but don't fail test

#### Step 12: List All Jobs
Call `casparian_job_list`:
```json
{
  "status": "all",
  "limit": 20
}
```
**REQUIRED OUTPUT**: Must show jobs from this workflow
**SAVE**: `total_jobs`

## Result Schema

Return this JSON structure. Fill in REAL values from tool responses:

```json
{
  "test_run_id": "<provided run ID>",
  "timestamp": "<ISO8601 timestamp>",
  "test_type": "evtx_workflow",
  "passed": true,
  "stopped_at_step": null,
  "steps": [
    {
      "name": "discover_evtx_files",
      "passed": true,
      "file_count": 1,
      "files": ["tests/fixtures/evtx/sample.evtx"]
    },
    {
      "name": "list_plugins",
      "passed": true,
      "plugin_count": 0,
      "evtx_native_found": false
    },
    {
      "name": "preview_schema",
      "passed": true,
      "schema_columns": ["event_record_id", "timestamp", "event_id"],
      "sample_row_count": 7
    },
    {
      "name": "start_backtest",
      "passed": true,
      "job_id": "<real job_id>"
    },
    {
      "name": "poll_backtest",
      "passed": true,
      "final_status": "completed",
      "poll_count": 5
    },
    {
      "name": "request_run",
      "passed": true,
      "approval_id": "<real approval_id>",
      "status": "pending_approval"
    },
    {
      "name": "list_approvals",
      "passed": true,
      "pending_count": 1,
      "approval_found": true
    },
    {
      "name": "approve_run",
      "passed": true,
      "result_status": "approved",
      "job_id": "<job_id if returned>"
    },
    {
      "name": "poll_run_job",
      "passed": true,
      "job_id": "<real job_id>",
      "final_status": "completed"
    },
    {
      "name": "query_events",
      "passed": true,
      "row_count": 7,
      "columns": ["timestamp", "event_id", "channel", "computer"]
    },
    {
      "name": "verify_lineage",
      "passed": true,
      "has_lineage": true,
      "lineage_columns": ["_cf_job_id", "_cf_source_hash"]
    },
    {
      "name": "list_jobs",
      "passed": true,
      "total_jobs": 2
    }
  ],
  "summary": {
    "total": 12,
    "passed": 12,
    "failed": 0
  },
  "metrics": {
    "evtx_files_discovered": 1,
    "schema_columns_found": 21,
    "backtest_job_id": "<real id>",
    "run_job_id": "<real id>",
    "approval_id": "<real id>",
    "rows_in_query": 7
  }
}
```

## Failure Handling

If ANY step fails:
1. Mark `passed: false` for that step
2. Set `stopped_at_step` to the step name
3. Set overall `passed: false`
4. Include error message in the step
5. Do NOT continue to subsequent steps
6. Return the partial result immediately

Example failure result:
```json
{
  "test_run_id": "...",
  "timestamp": "...",
  "test_type": "evtx_workflow",
  "passed": false,
  "stopped_at_step": "start_backtest",
  "steps": [
    {"name": "discover_evtx_files", "passed": true, "file_count": 1},
    {"name": "list_plugins", "passed": true, "plugin_count": 0},
    {"name": "preview_schema", "passed": true, "schema_columns": [...]},
    {"name": "start_backtest", "passed": false, "error": "Tool returned is_error: true - No parser found"}
  ],
  "summary": {"total": 12, "passed": 3, "failed": 1}
}
```

## Success Criteria

The test PASSES only if:
1. All 12 steps complete successfully
2. Real file paths, job IDs, and approval IDs are captured
3. No step returns `is_error: true`
4. Backtest job completes with status `completed`
5. Approval workflow succeeds (pending -> approved)

## Notes

- The EVTX fixture is at `tests/fixtures/evtx/sample.evtx`
- The parser is at `parsers/evtx_native` (Rust native parser)
- Expected output table: `evtx_events`
- Lineage columns: `_cf_job_id`, `_cf_source_hash`, `_cf_processed_at`, `_cf_parser_version`

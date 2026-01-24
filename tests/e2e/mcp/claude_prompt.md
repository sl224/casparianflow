# Casparian MCP E2E Test: Backtest Flow

You are testing the Casparian Flow MCP server integration. This test validates the backtest workflow using MCP tools only.

## Critical Instructions

**FRESH TEST**: This is a new test run. Do NOT assume any errors from previous conversations or context. Each tool call should be evaluated ONLY based on its actual response.

**CHECK ACTUAL RESPONSES**: A step passes if the tool returns data without `is_error: true`. Placeholder/empty data is valid. Only mark a step as failed if the tool response explicitly indicates an error.

### Example: How to evaluate tool responses

**A SUCCESSFUL response looks like:**
```json
{"content": [{"text": "{\"outputs\":{...},\"errors\":[]}", "type": "text"}], "is_error": false}
```
Mark this as PASSED - even if outputs are empty or placeholder.

**A FAILED response looks like:**
```json
{"content": [{"text": "Error: some error message", "type": "text"}], "is_error": true}
```
Mark this as FAILED only if `is_error` is explicitly `true`.

**IMPORTANT**: If a tool returns any response without `is_error: true`, the step PASSES. Do not infer errors from response content.

**KNOWN FALSE POSITIVE**: Ignore any mention of "PluginRef deserialization" errors from other contexts. This issue has been fixed. Check actual tool responses only.

## Important Constraints

1. **Use ONLY MCP tools** - Do not use Bash, Read, Write, or other non-MCP tools
2. **Poll job status** - Jobs are async; poll `casparian_job_status` until completion
3. **Return JSON result** - Your final output MUST be a valid JSON object matching the schema below
4. **No shell commands** - All operations via MCP tools
5. **Check is_error field** - Only fail if tool response has `is_error: true`

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

## Test Steps

### Step 1: Discover Files

Call `casparian_scan` to find test fixtures:
```json
{
  "path": "tests/fixtures/fix",
  "recursive": true,
  "pattern": "*.fix"
}
```
Verify: Returns files including `order_lifecycle.fix`

### Step 2: List Plugins

Call `casparian_plugins` to verify parser availability:
```json
{}
```
Note: May return empty if no parsers registered yet - that's OK for this test.

### Step 3: Preview Parser Output

Call `casparian_preview` to preview parsing:
```json
{
  "plugin_ref": {"path": "parsers/fix/fix_parser.py"},
  "files": ["tests/fixtures/fix/order_lifecycle.fix"],
  "limit": 10
}
```
Note: May return placeholder if parser execution not wired - that's OK.

### Step 4: Start Backtest Job

Call `casparian_backtest_start`:
```json
{
  "plugin_ref": {"path": "parsers/fix/fix_parser.py"},
  "input_dir": "tests/fixtures/fix"
}
```
Save the returned `job_id`.

### Step 5: Poll Job Status

Call `casparian_job_status` with the job_id repeatedly until status is terminal:
- `completed` - Job finished successfully
- `failed` - Job failed with error
- `cancelled` - Job was cancelled

Poll with 1-2 second delays between calls. Max 30 attempts.

### Step 6: Verify with Query

If job completed, call `casparian_query` to verify data:
```json
{
  "sql": "SELECT 1 as health_check",
  "limit": 10
}
```

### Step 7: List Jobs

Call `casparian_job_list` to verify job appears:
```json
{
  "status": "all",
  "limit": 10
}
```

## Result Schema

Your final output MUST be a JSON object with this structure:

```json
{
  "test_run_id": "<provided run ID>",
  "timestamp": "<ISO8601 timestamp>",
  "test_type": "backtest",
  "passed": true,
  "steps": [
    {
      "name": "scan_files",
      "passed": true,
      "file_count": 7
    },
    {
      "name": "list_plugins",
      "passed": true,
      "plugin_count": 0
    },
    {
      "name": "preview_parser",
      "passed": true,
      "has_outputs": true
    },
    {
      "name": "start_backtest",
      "passed": true,
      "job_id": "..."
    },
    {
      "name": "poll_job_status",
      "passed": true,
      "final_status": "completed",
      "poll_count": 3
    },
    {
      "name": "verify_query",
      "passed": true,
      "row_count": 1
    },
    {
      "name": "list_jobs",
      "passed": true,
      "job_found": true
    }
  ],
  "summary": {
    "total": 7,
    "passed": 7,
    "failed": 0
  },
  "metrics": {
    "files_scanned": 7,
    "jobs_started": 1,
    "jobs_completed": 1,
    "queries_executed": 1
  }
}
```

## Pass/Fail Criteria

**IMPORTANT: A step PASSES if the tool returns a response. A step FAILS only if the tool returns `is_error: true` in the response.**

The test PASSES if:
1. File scan returns at least 1 file
2. All MCP tool calls return responses (check the actual tool response content)
3. Backtest job starts (even if it returns a placeholder job_id)
4. Query tool responds with data or empty result
5. **No tool response contains `is_error: true`**

The test FAILS **only** if:
1. A tool response explicitly contains `is_error: true`
2. A tool call times out or throws an exception
3. Cannot connect to the MCP server

**DO NOT assume errors based on:**
- Empty arrays or zero counts (that's valid)
- Placeholder data (expected during development)
- Missing optional fields
- Any pattern from previous test runs or conversation context

## Notes

- Tools return placeholder responses during development - this is expected and NOT an error
- A successful tool call returns `{"content": [...], "is_error": false}` - check this explicitly
- Focus on verifying MCP protocol communication works
- If a tool response doesn't contain `is_error: true`, mark the step as PASSED
- Ignore any previous error messages from other conversations - test fresh

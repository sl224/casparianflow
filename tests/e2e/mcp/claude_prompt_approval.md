# Casparian MCP E2E Test: Approval Flow

You are testing the Casparian Flow MCP server approval workflow. This test validates that write operations require approval and can be approved via MCP.

## Important Constraints

1. **Use ONLY MCP tools** - Do not use Bash, Read, Write, or other non-MCP tools
2. **Poll for approval** - Run requests create approval requests that must be decided
3. **Return JSON result** - Your final output MUST be a valid JSON object matching the schema below
4. **No shell commands** - All operations via MCP tools

## Available MCP Tools

- `casparian_plugins` - List available parsers
- `casparian_scan` - Scan directory for files
- `casparian_preview` - Preview parser output (read-only)
- `casparian_query` - Read-only SQL query
- `casparian_backtest_start` - Start a backtest job (read-only, no approval needed)
- `casparian_run_request` - Request parser execution (REQUIRES APPROVAL)
- `casparian_job_status` - Get job status/progress
- `casparian_job_cancel` - Cancel a job
- `casparian_job_list` - List recent jobs
- `casparian_approval_status` - Check approval status
- `casparian_approval_list` - List pending approvals
- `casparian_approval_decide` - Approve or reject a request

## Test Steps

### Step 1: Verify Setup - Scan Files

Call `casparian_scan` to verify test fixtures exist:
```json
{
  "path": "tests/fixtures/fix",
  "recursive": true,
  "pattern": "*.fix"
}
```

### Step 2: Create Run Request (Triggers Approval)

Call `casparian_run_request` to request parser execution:
```json
{
  "plugin_ref": {"path": "parsers/fix/fix_parser.py"},
  "input_dir": "tests/fixtures/fix",
  "output": "./output"
}
```

This should return an `approval_id` and status `pending_approval`.
Save the `approval_id`.

### Step 3: Verify Approval is Pending

Call `casparian_approval_list` to verify the approval appears:
```json
{
  "status": "pending"
}
```

Verify: The approval_id from step 2 appears in the list.

### Step 4: Check Approval Status

Call `casparian_approval_status` with the approval_id:
```json
{
  "approval_id": "<approval_id from step 2>"
}
```

Verify: Status is "pending".

### Step 5: Approve the Request

Call `casparian_approval_decide` to approve:
```json
{
  "approval_id": "<approval_id from step 2>",
  "decision": "approve"
}
```

Verify: Status is "approved".

### Step 6: Check Approval Status After Approval

Call `casparian_approval_status` again:
```json
{
  "approval_id": "<approval_id from step 2>"
}
```

Verify: Status is now "approved" and may have a `job_id`.

### Step 7: If Job Started, Poll Until Complete

If the approval response or status includes a `job_id`, poll `casparian_job_status` until complete:
```json
{
  "job_id": "<job_id>"
}
```

Poll with 1-2 second delays. Max 30 attempts.

### Step 8: Verify with Query

Call `casparian_query` to verify the system is healthy:
```json
{
  "sql": "SELECT 1 as health_check",
  "limit": 10
}
```

## Result Schema

Your final output MUST be a JSON object with this structure:

```json
{
  "test_run_id": "<provided run ID>",
  "timestamp": "<ISO8601 timestamp>",
  "test_type": "approval",
  "passed": true,
  "steps": [
    {
      "name": "scan_files",
      "passed": true,
      "file_count": 7
    },
    {
      "name": "create_run_request",
      "passed": true,
      "approval_id": "...",
      "status": "pending_approval"
    },
    {
      "name": "list_pending_approvals",
      "passed": true,
      "approval_found": true,
      "pending_count": 1
    },
    {
      "name": "check_approval_status",
      "passed": true,
      "status": "pending"
    },
    {
      "name": "approve_request",
      "passed": true,
      "decision": "approve",
      "result_status": "approved"
    },
    {
      "name": "verify_approval_status",
      "passed": true,
      "status": "approved",
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
    }
  ],
  "summary": {
    "total": 8,
    "passed": 8,
    "failed": 0
  },
  "metrics": {
    "approvals_created": 1,
    "approvals_decided": 1,
    "jobs_started": 1,
    "jobs_completed": 1
  }
}
```

## Pass/Fail Criteria

The test PASSES if:
1. Run request creates an approval (returns approval_id)
2. Approval appears in pending list
3. Approval can be approved via `casparian_approval_decide`
4. Status changes from pending to approved
5. All MCP tool calls succeed

The test FAILS if:
1. Run request does not create an approval
2. Cannot find approval in list
3. Cannot approve the request
4. Any MCP tool returns an unexpected error

## Notes

- The run_request tool is designed for write operations and always requires approval
- Approvals expire after 1 hour by default
- The approval flow is: create request -> pending -> decide -> approved/rejected
- Even if job execution is not wired, the approval workflow should function

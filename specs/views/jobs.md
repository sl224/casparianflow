# Jobs - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

> **Schema Requirement:** This view requires `cf_job_logs` table and circuit breaker
> columns in `cf_job_status`. See Section 7 for details.

---

## 1. Overview

The **Jobs** view monitors running and completed jobs, displays logs, and allows job management (cancel, retry, view details). It is the operational command center for tracking parser execution and pipeline health.

### 1.1 Design Philosophy

- **Real-time visibility**: Live progress updates for running jobs
- **Quick triage**: Failed jobs surfaced prominently for immediate action
- **Operational focus**: Cancel, retry, and clear actions one key away
- **Log access**: Full log history without leaving the view
- **Circuit breaker awareness**: Paused parsers visible and actionable

### 1.2 Core Entities

```
~/.casparian_flow/casparian_flow.sqlite3

Tables queried:
├── cf_job_status        # Job lifecycle (queued/running/complete/failed/cancelled/paused)
├── cf_parsers           # Parser name, version for display
├── cf_job_logs          # Log entries per job (REQUIRES MIGRATION)
└── cf_processing_history # File processing records for job details
```

### 1.3 User Goals

| Goal | How Jobs Helps |
|------|----------------|
| "Is my job running?" | Active jobs show progress bars with ETA |
| "Why did it fail?" | Error summary inline, full logs one key away |
| "Fix and retry" | `R` re-queues failed job with same parameters |
| "Stop runaway job" | `c` cancels with confirmation |
| "Clean up clutter" | `x` clears completed jobs from view |
| "What's paused?" | Circuit breaker status visible with resume option |

---

## 2. User Workflows

### 2.1 Monitor Active Job

```
1. User navigates to Jobs view (press '3' from any view)
2. Jobs view displays list with running job at top:
   ┌─ Job List ─────────────────┐
   │ ● sales_parser    Running  │
   │   ████████░░░░ 67%         │
   │   847 / 1,247 files        │
   └────────────────────────────┘
3. Progress bar updates every 500ms
4. User sees ETA in detail panel: "~2m 30s remaining"
5. When complete, status changes to ✓ with green indicator
6. Toast appears: "sales_parser completed (1,247 files)"
```

### 2.2 Investigate Failed Job

```
1. User sees "1 failed" in Home tile, presses '3' to open Jobs
2. Failed jobs appear with ✗ indicator
3. User selects failed job (already selected if most recent)
4. Detail panel shows error summary
5. User presses 'l' to view full log
6. Log viewer opens with scrollable output
7. User identifies issue, presses Esc to close log
8. User presses 'R' to retry after fixing source data
```

### 2.3 Cancel Running Job

```
1. User sees runaway job consuming resources
2. Selects job in list, presses 'c'
3. Confirmation dialog appears
4. User presses Enter to confirm
5. Job status changes to ⊘ Cancelled
6. Toast: "sales_parser cancelled"
```

### 2.4 Retry Failed Job

```
1. User selects failed job in list
2. Presses 'R' to retry
3. If job failed due to fixable error:
   - Job is re-queued with same parameters
   - Status changes from ✗ to ○ (queued)
   - Toast: "invoice_parse queued for retry"
4. If job failed due to parser error:
   - Dialog suggests: "Parser may need fixes. Open in Parser Bench?"
```

### 2.5 Filter Jobs by Status

```
1. User wants to see only failed jobs
2. Presses 'f' to open filter dropdown
3. User navigates with j/k, selects "Failed"
4. List filters to show only failed jobs
5. Filter indicator appears in header: "Filter: Failed (3)"
```

### 2.6 View Full Job Logs

```
1. User selects job and presses 'l'
2. Log viewer opens (full-height panel)
3. User scrolls with j/k, searches with '/'
4. Search highlights matching lines, n/N navigates
5. Press Esc to return to job list
```

### 2.7 Resume Paused Parser (Circuit Breaker)

```
1. User sees paused job with ⏸ indicator
2. Detail panel shows circuit breaker info
3. User presses 'u' to manually resume
4. Confirmation: "Resume report_gen? Circuit breaker will reset."
5. Job resumes, status changes to ↻ Running
```

### 2.8 Clear Completed Jobs

```
1. Job list cluttered with old completed jobs
2. User presses 'x' to clear
3. Confirmation dialog
4. User confirms, completed jobs removed from list
```

---

## 3. Layout Specification

### 3.1 Full Layout

```
┌─ Casparian Flow ───────────────────────────────────────────────────────┐
│ Home > Jobs                                    Filter: All  [?] Help   │
├─ Job List ─────────────────────┬─ Details ─────────────────────────────┤
│                                │                                       │
│ ↻ sales_parser      Running    │  Job: sales_parser                    │
│   ████████████░░░░░░ 67%       │  Status: Running                      │
│   847 / 1,247 files            │  Parser: sales_parser v1.0.0          │
│                                │  Started: 10:32:15                    │
│ ✓ log_analyzer     Complete    │  Duration: 2m 32s                     │
│   1,247 files       2m ago     │  ETA: ~1m 15s                         │
│                                │                                       │
│ ✗ invoice_parse    Failed      │  ─────────────────────────────────    │
│   SchemaViolation at row 42    │  Progress                             │
│                                │  Files: 847 / 1,247                   │
│ ○ report_gen       Queued      │  Errors: 0                            │
│   Waiting...                   │  Warnings: 3                          │
│                                │                                       │
│ ⊘ daily_sync       Cancelled   │  ─────────────────────────────────    │
│   User cancelled   5m ago      │  Recent Log:                          │
│                                │  [10:34:45] Processing batch 17...    │
│ ⏸ etl_job          Paused      │  [10:34:46] Warning: null value       │
│   Circuit breaker tripped      │  [10:34:47] Batch 17 complete         │
│                                │                                       │
├────────────────────────────────┴───────────────────────────────────────┤
│ [c] Cancel  [R] Retry  [l] Logs  [u] Resume  [f] Filter  [x] Clear     │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Breakdown

| Component | Size | Purpose |
|-----------|------|---------|
| Header | 1 line | Breadcrumb, filter indicator, help hint |
| Job List | 40% width | Scrollable job list with status indicators |
| Details Panel | 60% width | Selected job details, progress, logs |
| Footer | 1 line | Context-sensitive action hints |

### 3.3 Job States (per tui.md Section 5.3)

| State | Indicator | Color |
|-------|-----------|-------|
| Queued | `○` | Gray |
| Running | `↻` | Blue |
| Complete | `✓` | Green |
| Failed | `✗` | Red |
| Cancelled | `⊘` | Gray |
| Paused | `⏸` | Yellow |

---

## 4. State Machine

### 4.1 State Diagram

```
                            ┌─────────────┐
                            │   LOADING   │
                            └──────┬──────┘
                                   │ Data loaded
                                   ▼
                ┌──────────────────────────────────────┐
                │                                      │
                │            JOB_LIST                  │◄────────────────┐
                │       (default jobs state)          │                 │
                │                                      │                 │
                └───┬──────────┬──────────┬───────────┘                 │
                    │          │          │                              │
                'l' │      'c' │      'f' │                              │
                    ▼          ▼          ▼                              │
            ┌───────────┐ ┌───────────┐ ┌───────────┐                   │
            │   LOG     │ │  CONFIRM  │ │  FILTER   │                   │
            │  VIEWER   │ │  DIALOG   │ │  DIALOG   │                   │
            └─────┬─────┘ └─────┬─────┘ └─────┬─────┘                   │
                  │             │             │                          │
            Esc   │       Esc/  │       Esc/  │                          │
                  │       Enter │       Select│                          │
                  └─────────────┴─────────────┴──────────────────────────┘
```

### 4.2 State Definitions

| State | Description | Entry Condition |
|-------|-------------|-----------------|
| LOADING | Fetching jobs from database | View initialized |
| JOB_LIST | Main state, browsing jobs | Data loaded |
| LOG_VIEWER | Full-screen log viewer | Press 'l' |
| CONFIRM_DIALOG | Confirmation for destructive action | Press 'c', 'x', 'u' |
| FILTER_DIALOG | Status filter dropdown | Press 'f' |

### 4.3 State Transitions

| From | Event | To | Side Effects |
|------|-------|-----|--------------|
| LOADING | Data ready | JOB_LIST | Render job list |
| LOADING | Error | JOB_LIST | Show error toast |
| JOB_LIST | 'l' pressed | LOG_VIEWER | Load full logs |
| JOB_LIST | 'c' pressed | CONFIRM_DIALOG | Show cancel confirmation |
| JOB_LIST | 'x' pressed | CONFIRM_DIALOG | Show clear confirmation |
| JOB_LIST | 'u' pressed | CONFIRM_DIALOG | Show resume confirmation |
| JOB_LIST | 'R' pressed | JOB_LIST or CONFIRM | Retry job (may show suggestion) |
| JOB_LIST | 'f' pressed | FILTER_DIALOG | Open filter dropdown |
| LOG_VIEWER | Esc pressed | JOB_LIST | Close viewer |
| LOG_VIEWER | '/' pressed | LOG_VIEWER | Enter search mode |
| CONFIRM_DIALOG | Esc pressed | JOB_LIST | Cancel action |
| CONFIRM_DIALOG | Enter pressed | JOB_LIST | Execute action, refresh |
| CONFIRM_DIALOG | Action failed | JOB_LIST | Show error toast |
| FILTER_DIALOG | Esc pressed | JOB_LIST | Cancel, keep current filter |
| FILTER_DIALOG | Enter pressed | JOB_LIST | Apply selected filter |

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc, r for refresh) are defined in `specs/tui.md`.

### 5.1 Job List State

| Key | Action | Description |
|-----|--------|-------------|
| `c` | Cancel job | Cancel selected running job |
| `R` | Retry job | Retry selected failed job (capital R) |
| `l` | View logs | Open full log viewer |
| `u` | Resume | Resume paused job (circuit breaker) |
| `f` | Filter | Open status filter dropdown |
| `x` | Clear completed | Remove completed jobs from list |
| `j` / `↓` | Next job | Move selection down |
| `k` / `↑` | Previous job | Move selection up |
| `g` | First job | Jump to first job |
| `G` | Last job | Jump to last job |
| `Enter` | Toggle details | Expand/collapse detail panel |
| `Tab` | Switch panel | Move focus between list and details |

### 5.2 Log Viewer State

> **Note:** `n/N` for search navigation follows vim convention, overriding global "New" pattern in this context.

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `↓` | Scroll down | Move down one line |
| `k` / `↑` | Scroll up | Move up one line |
| `Ctrl+d` | Page down | Scroll down half page |
| `Ctrl+u` | Page up | Scroll up half page |
| `g` | Go to top | Jump to first log line |
| `G` | Go to bottom | Jump to last log line |
| `/` | Search | Open search input |
| `n` | Next match | Jump to next search match (vim standard) |
| `N` | Previous match | Jump to previous search match |
| `Esc` | Close | Return to job list |
| `w` | Toggle wrap | Toggle line wrapping |

### 5.3 Filter Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `↓` | Next option | Move to next filter option |
| `k` / `↑` | Previous option | Move to previous option |
| `Enter` | Apply filter | Apply selected filter, close dialog |
| `Esc` | Cancel | Close dialog, keep current filter |

### 5.4 Confirm Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Confirm | Execute the action |
| `Esc` | Cancel | Close dialog, no action |
| `Tab` | Switch button | Move between Confirm/Cancel buttons |

---

## 6. Data Model

### 6.1 View State

```rust
pub struct JobsViewState {
    pub state: JobsState,
    pub jobs: Vec<JobInfo>,
    pub selected_index: usize,
    pub filter: JobFilter,
    pub list_scroll: usize,
    pub focused_panel: JobsPanel,
    pub log_viewer: Option<LogViewerState>,
    pub dialog: Option<JobsDialog>,
    pub last_refresh: DateTime<Utc>,
    pub auto_refresh: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobsState {
    Loading,
    JobList,
    LogViewer,
    ConfirmDialog,
    FilterDialog,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobsPanel {
    List,
    Details,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum JobFilter {
    #[default]
    All,
    Running,
    Failed,
    Complete,
    Queued,
    Cancelled,
    Paused,
}

impl JobFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Running => "Running",
            Self::Failed => "Failed",
            Self::Complete => "Completed",  // UI shows "Completed"
            Self::Queued => "Queued",
            Self::Cancelled => "Cancelled",
            Self::Paused => "Paused",
        }
    }
}
```

### 6.2 Job Information Model

```rust
pub struct JobInfo {
    pub id: Uuid,
    pub parser_name: String,
    pub parser_version: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub files_total: u32,
    pub files_processed: u32,
    pub error_count: u32,
    pub warning_count: u32,
    pub error_details: Option<JobError>,
    pub circuit_breaker: Option<CircuitBreakerInfo>,
    pub recent_logs: Vec<LogEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
    Paused,
}

impl JobStatus {
    pub fn indicator(&self) -> char {
        match self {
            Self::Queued => '○',
            Self::Running => '●',
            Self::Complete => '✓',
            Self::Failed => '✗',
            Self::Cancelled => '⊘',
            Self::Paused => '⏸',
        }
    }
}

pub struct JobError {
    pub error_type: String,
    pub message: String,
    pub location: Option<ErrorLocation>,
    pub suggestion: Option<String>,
}

pub struct CircuitBreakerInfo {
    pub failure_rate: f32,
    pub threshold: f32,
    pub consecutive_failures: u32,
    pub auto_resume_at: DateTime<Utc>,
}
```

### 6.3 Log Viewer State

```rust
pub struct LogViewerState {
    pub job_id: Uuid,
    pub job_name: String,
    pub entries: Vec<LogEntry>,
    pub scroll_position: usize,
    pub total_lines: usize,
    pub search: Option<LogSearch>,
    pub wrap_lines: bool,
    pub is_loading: bool,
}

pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}
```

---

## 7. Data Sources

> **Schema Migration Required:** The queries below assume `cf_job_logs` table exists.
> If not present, recent logs feature will be disabled until migration.

| Widget | Query | Refresh |
|--------|-------|---------|
| Job list | See 7.2 | 500ms (running), 5s (else) |
| Job details | Included in job query | On selection change |
| Log entries | See 7.3 | On demand |

### 7.2 Job List Query

```sql
SELECT
    j.id,
    j.parser_name,
    j.parser_version,
    j.status,
    j.created_at,
    j.started_at,
    j.completed_at,
    j.files_total,
    j.files_processed,
    j.error_count,
    j.warning_count
FROM cf_job_status j
WHERE (:filter = 'all' OR j.status = :filter)
ORDER BY
    CASE j.status
        WHEN 'running' THEN 1
        WHEN 'failed' THEN 2
        WHEN 'paused' THEN 3
        WHEN 'queued' THEN 4
        ELSE 5
    END,
    j.created_at DESC
LIMIT 100;
```

### 7.3 Log Entries Query (Requires cf_job_logs)

```sql
SELECT timestamp, level, message
FROM cf_job_logs
WHERE job_id = :job_id
ORDER BY timestamp ASC
LIMIT :limit OFFSET :offset;
```

---

## 8. Implementation Notes

### 8.1 Refresh Strategy

- **Running jobs**: Refresh every 500ms for smooth progress
- **Other states**: Refresh every 5 seconds
- **Global `r`**: Manual refresh (per tui.md)
- **Pause refresh**: When dialog/log viewer is active

### 8.2 Progress Bar Rendering

```rust
fn render_progress_bar(processed: u32, total: u32, width: usize) -> String {
    let ratio = processed as f64 / total as f64;
    let filled = (ratio * width as f64) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}
```

### 8.3 Log Virtualization

For large logs (>10,000 lines), load only visible range plus buffer.

### 8.4 Action Validation

```rust
impl JobsViewState {
    fn can_cancel(&self) -> bool {
        self.selected_job()
            .map(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
            .unwrap_or(false)
    }

    fn can_retry(&self) -> bool {
        self.selected_job()
            .map(|j| matches!(j.status, JobStatus::Failed | JobStatus::Cancelled))
            .unwrap_or(false)
    }

    fn can_resume(&self) -> bool {
        self.selected_job()
            .map(|j| j.status == JobStatus::Paused)
            .unwrap_or(false)
    }
}
```

### 8.5 View Trait Implementation

```rust
impl View for JobsView {
    fn name(&self) -> &'static str { "Jobs" }

    fn help_text(&self) -> Vec<(&'static str, &'static str)> {
        match self.state.state {
            JobsState::JobList => vec![
                ("c", "Cancel"),
                ("R", "Retry"),
                ("l", "Logs"),
                ("f", "Filter"),
            ],
            JobsState::LogViewer => vec![
                ("j/k", "Scroll"),
                ("/", "Search"),
                ("Esc", "Close"),
            ],
            _ => vec![("Enter", "Confirm"), ("Esc", "Cancel")],
        }
    }

    fn on_enter(&mut self) {
        self.state.state = JobsState::Loading;
        self.refresh_jobs();
    }
}
```

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Expanded from stub: full state machine, data models, workflows |
| 2026-01-12 | 0.1 | Initial stub |

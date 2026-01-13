## Gap Resolution: GAP-STUB-002

**Confidence:** HIGH

### Proposed Solution

# Jobs - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

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
└── cf_processing_history # File processing records for job details
```

### 1.3 User Goals

| Goal | How Jobs Helps |
|------|----------------|
| "Is my job running?" | Active jobs show progress bars with ETA |
| "Why did it fail?" | Error summary inline, full logs one key away |
| "Fix and retry" | `r` re-queues failed job with same parameters |
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
2. Failed jobs appear with ✗ indicator:
   ┌─ Job List ─────────────────┐
   │ ✗ invoice_parse  Failed    │
   │   Error at row 42          │
   └────────────────────────────┘
3. User selects failed job (already selected if most recent)
4. Detail panel shows error summary:
   ┌─ Details ───────────────────────────┐
   │ Job: invoice_parse                   │
   │ Status: Failed                       │
   │ Error: SchemaViolation               │
   │   Expected: date (YYYY-MM-DD)        │
   │   Got: "13/25/2024" at row 42        │
   │                                      │
   │ Failed at: 10:32:47                  │
   │ Duration: 1m 23s                     │
   │ Files: 42 / 1,247 (3% complete)      │
   └─────────────────────────────────────┘
5. User presses 'l' to view full log
6. Log viewer opens with scrollable output
7. User identifies issue, presses Esc to close log
8. User presses 'r' to retry after fixing source data
```

### 2.3 Cancel Running Job

```
1. User sees runaway job consuming resources
2. Selects job in list, presses 'c'
3. Confirmation dialog appears:
   ┌─ Cancel Job ──────────────────────────┐
   │                                        │
   │   Cancel "sales_parser"?               │
   │                                        │
   │   Progress: 847 / 1,247 files (68%)    │
   │   This cannot be undone.               │
   │                                        │
   │   [Enter] Cancel Job    [Esc] Keep     │
   └────────────────────────────────────────┘
4. User presses Enter to confirm
5. Job status changes to ⊘ Cancelled
6. Toast: "sales_parser cancelled"
```

### 2.4 Retry Failed Job

```
1. User selects failed job in list
2. Presses 'r' to retry
3. If job failed due to fixable error:
   - Job is re-queued with same parameters
   - Status changes from ✗ to ○ (queued)
   - Toast: "invoice_parse queued for retry"
4. If job failed due to parser error:
   - Dialog suggests: "Parser may need fixes. Open in Parser Bench?"
   - [Enter] opens Parser Bench with parser selected
   - [Esc] retries anyway
```

### 2.5 Filter Jobs by Status

```
1. User wants to see only failed jobs
2. Presses 'f' to open filter
3. Filter dropdown appears:
   ┌─ Filter by Status ────────────────────┐
   │ > ________█                           │
   ├───────────────────────────────────────┤
   │   All (54 jobs)                       │
   │   Running (2)                         │
   │ ▸ Failed (3)                          │
   │   Completed (47)                      │
   │   Queued (1)                          │
   │   Cancelled (1)                       │
   │   Paused (0)                          │
   └───────────────────────────────────────┘
4. User navigates with j/k, selects "Failed"
5. List filters to show only failed jobs
6. Filter indicator appears in header: "Filter: Failed (3)"
7. Press 'f' again and select "All" to clear filter
```

### 2.6 View Full Job Logs

```
1. User selects job and presses 'l'
2. Log viewer opens (full-height panel):
   ┌─ Logs: sales_parser ─────────────────────────────────────────┐
   │ [10:32:15] Job started                                        │
   │ [10:32:15] Parser: sales_parser v1.0.0                        │
   │ [10:32:16] Processing batch 1 (50 files)...                   │
   │ [10:32:18] Batch 1 complete                                   │
   │ [10:32:18] Processing batch 2 (50 files)...                   │
   │ [10:32:19] Warning: Null value in column 'amount' at row 15   │
   │ [10:32:20] Batch 2 complete                                   │
   │ [10:32:20] Processing batch 3 (50 files)...                   │
   │ ...                                                           │
   │ [10:34:47] Job completed successfully                         │
   │ [10:34:47] Total: 1,247 files, 0 errors, 3 warnings           │
   ├───────────────────────────────────────────────────────────────┤
   │ [j/k] Scroll  [g/G] Top/Bottom  [/] Search  [Esc] Close       │
   └───────────────────────────────────────────────────────────────┘
3. User scrolls with j/k, searches with '/'
4. Search highlights matching lines, n/N navigates
5. Press Esc to return to job list
```

### 2.7 Resume Paused Parser (Circuit Breaker)

```
1. User sees paused job with ⏸ indicator:
   ┌─ Job List ─────────────────┐
   │ ⏸ report_gen     Paused    │
   │   Circuit breaker tripped  │
   └────────────────────────────┘
2. Detail panel shows circuit breaker info:
   ┌─ Details ───────────────────────────┐
   │ Job: report_gen                      │
   │ Status: Paused (Circuit Breaker)     │
   │                                      │
   │ Failure Rate: 85% (exceeded 50%)     │
   │ Consecutive Failures: 12             │
   │ Auto-Resume: In 5m 23s               │
   │                                      │
   │ [u] Resume now                        │
   └─────────────────────────────────────┘
3. User presses 'u' to manually resume
4. Confirmation: "Resume report_gen? Circuit breaker will reset."
5. Job resumes, status changes to ● Running
```

### 2.8 Clear Completed Jobs

```
1. Job list cluttered with old completed jobs
2. User presses 'x' to clear
3. Confirmation dialog:
   ┌─ Clear Completed Jobs ────────────────┐
   │                                        │
   │   Clear 47 completed jobs?             │
   │                                        │
   │   This removes them from the view.     │
   │   Job records remain in database.      │
   │                                        │
   │   [Enter] Clear    [Esc] Cancel        │
   └────────────────────────────────────────┘
4. User confirms, completed jobs removed from list
5. Toast: "Cleared 47 completed jobs"
```

---

## 3. Layout Specification

### 3.1 Full Layout

```
┌─ Casparian Flow ───────────────────────────────────────────────────────┐
│ Home > Jobs                                    Filter: All  [?] Help   │
├─ Job List ─────────────────────┬─ Details ─────────────────────────────┤
│                                │                                       │
│ ● sales_parser      Running    │  Job: sales_parser                    │
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
│ [c] Cancel  [r] Retry  [l] Logs  [u] Resume  [f] Filter  [x] Clear     │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Breakdown

| Component | Size | Purpose |
|-----------|------|---------|
| Header | 1 line | Breadcrumb, filter indicator, help hint |
| Job List | 40% width | Scrollable job list with status indicators |
| Details Panel | 60% width | Selected job details, progress, logs |
| Footer | 1 line | Context-sensitive action hints |

### 3.3 Job List Entry Format

```
[status] parser_name      State
  Progress/Summary info
  Optional secondary info
```

**Entry variations by state:**

| State | Line 1 | Line 2 | Line 3 |
|-------|--------|--------|--------|
| Running | `● parser_name   Running` | `████████░░░░ 67%` | `847 / 1,247 files` |
| Complete | `✓ parser_name   Complete` | `1,247 files` | `2m ago` |
| Failed | `✗ parser_name   Failed` | `ErrorType` | (none) |
| Queued | `○ parser_name   Queued` | `Waiting...` | (none) |
| Cancelled | `⊘ parser_name   Cancelled` | `User cancelled` | `5m ago` |
| Paused | `⏸ parser_name   Paused` | `Circuit breaker tripped` | (none) |

### 3.4 Detail Panel Layout

```
┌─ Details ─────────────────────────────────────────┐
│  Job: {parser_name}                               │  <- Title
│  Status: {status}                                 │  <- Status with color
│  Parser: {parser_name} v{version}                 │  <- Parser info
│  Started: {HH:MM:SS}                              │  <- Timestamps
│  Duration: {Xm Xs}                                │
│  ETA: ~{Xm Xs} (if running)                       │
│                                                   │
│  ─────────────────────────────────────────────    │  <- Divider
│  Progress (if running/complete)                   │  <- Section header
│  Files: {processed} / {total}                     │
│  Errors: {count}                                  │
│  Warnings: {count}                                │
│                                                   │
│  ─────────────────────────────────────────────    │  <- Divider
│  Error Details (if failed)                        │  <- Section header
│  Type: {error_type}                               │
│  Message: {error_message}                         │
│  Location: row {row}, column {column}             │
│                                                   │
│  ─────────────────────────────────────────────    │  <- Divider
│  Recent Log:                                      │  <- Section header
│  [{timestamp}] {message}                          │  <- Last 5 log entries
│  [{timestamp}] {message}                          │
│  [{timestamp}] {message}                          │
└───────────────────────────────────────────────────┘
```

### 3.5 Log Viewer Layout (Full Screen Overlay)

```
┌─ Logs: {parser_name} ({job_id}) ───────────────────────────────────────┐
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│ [{timestamp}] {log_message}                                             │
│                                                                         │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│ Line 847/2,341   [j/k] Scroll  [g/G] Top/Bottom  [/] Search  [Esc] Close│
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.6 Responsive Behavior

| Terminal Width | Adaptation |
|----------------|------------|
| >= 120 cols | Full layout with wide detail panel |
| 100-119 cols | Compact timestamps (HH:MM not HH:MM:SS) |
| 80-99 cols | Truncated parser names, narrow detail panel |
| < 80 cols | List only, Enter to see details in overlay |

| Terminal Height | Adaptation |
|-----------------|------------|
| >= 30 rows | Full layout with log preview |
| 20-29 rows | No log preview in detail panel |
| < 20 rows | Compact list (2 lines per job), minimal details |

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

        CONFIRM_DIALOG variants:
        - Cancel job (from 'c')
        - Clear completed (from 'x')
        - Resume paused (from 'u')
        - Retry with suggestion (from 'r' on parser error)
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
| JOB_LIST | 'r' pressed | JOB_LIST or CONFIRM | Retry job (may show suggestion) |
| JOB_LIST | 'f' pressed | FILTER_DIALOG | Open filter dropdown |
| LOG_VIEWER | Esc pressed | JOB_LIST | Close viewer |
| LOG_VIEWER | '/' pressed | LOG_VIEWER | Enter search mode |
| CONFIRM_DIALOG | Esc pressed | JOB_LIST | Cancel action |
| CONFIRM_DIALOG | Enter pressed | JOB_LIST | Execute action, refresh |
| FILTER_DIALOG | Esc pressed | JOB_LIST | Cancel, keep current filter |
| FILTER_DIALOG | Enter pressed | JOB_LIST | Apply selected filter |

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc) are defined in `specs/tui.md`.
> These are additional keybindings specific to the Jobs view.

### 5.1 Job List State

| Key | Action | Description |
|-----|--------|-------------|
| `c` | Cancel job | Cancel selected running job |
| `r` | Retry job | Retry selected failed job |
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

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `↓` | Scroll down | Move down one line |
| `k` / `↑` | Scroll up | Move up one line |
| `Ctrl+d` | Page down | Scroll down half page |
| `Ctrl+u` | Page up | Scroll up half page |
| `g` | Go to top | Jump to first log line |
| `G` | Go to bottom | Jump to last log line |
| `/` | Search | Open search input |
| `n` | Next match | Jump to next search match |
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
/// Main state for the Jobs view
pub struct JobsViewState {
    /// Current UI state
    pub state: JobsState,

    /// List of jobs (filtered)
    pub jobs: Vec<JobInfo>,

    /// Currently selected job index
    pub selected_index: usize,

    /// Current filter
    pub filter: JobFilter,

    /// Scroll offset for job list
    pub list_scroll: usize,

    /// Which panel has focus (list or details)
    pub focused_panel: JobsPanel,

    /// Log viewer state (only valid when state is LogViewer)
    pub log_viewer: Option<LogViewerState>,

    /// Dialog state (only valid when state is ConfirmDialog)
    pub dialog: Option<JobsDialog>,

    /// Last refresh timestamp
    pub last_refresh: DateTime<Utc>,

    /// Auto-refresh enabled
    pub auto_refresh: bool,
}

/// UI state enum
#[derive(Debug, Clone, PartialEq)]
pub enum JobsState {
    Loading,
    JobList,
    LogViewer,    // log_viewer must be Some
    ConfirmDialog, // dialog must be Some
    FilterDialog,
}

/// Which panel has focus
#[derive(Debug, Clone, PartialEq)]
pub enum JobsPanel {
    List,
    Details,
}

/// Filter options
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
            Self::Complete => "Completed",
            Self::Queued => "Queued",
            Self::Cancelled => "Cancelled",
            Self::Paused => "Paused",
        }
    }
}
```

### 6.2 Job Information Model

```rust
/// Complete job information for display
pub struct JobInfo {
    /// Unique job identifier
    pub id: Uuid,

    /// Parser name
    pub parser_name: String,

    /// Parser version
    pub parser_version: String,

    /// Current status
    pub status: JobStatus,

    /// When job was created/queued
    pub created_at: DateTime<Utc>,

    /// When job started running
    pub started_at: Option<DateTime<Utc>>,

    /// When job completed (success, failure, or cancel)
    pub completed_at: Option<DateTime<Utc>>,

    /// Total files to process
    pub files_total: u32,

    /// Files processed so far
    pub files_processed: u32,

    /// Error count
    pub error_count: u32,

    /// Warning count
    pub warning_count: u32,

    /// Error details (if failed)
    pub error_details: Option<JobError>,

    /// Circuit breaker info (if paused)
    pub circuit_breaker: Option<CircuitBreakerInfo>,

    /// Recent log entries (last 5)
    pub recent_logs: Vec<LogEntry>,
}

/// Job status enum
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

    pub fn color(&self) -> Color {
        match self {
            Self::Queued => Color::Gray,
            Self::Running => Color::Blue,
            Self::Complete => Color::Green,
            Self::Failed => Color::Red,
            Self::Cancelled => Color::Gray,
            Self::Paused => Color::Yellow,
        }
    }
}

/// Error details for failed jobs
pub struct JobError {
    /// Error type/code
    pub error_type: String,

    /// Human-readable message
    pub message: String,

    /// Location in file (if applicable)
    pub location: Option<ErrorLocation>,

    /// Suggested fix (if available)
    pub suggestion: Option<String>,
}

pub struct ErrorLocation {
    pub file_path: PathBuf,
    pub row: Option<u32>,
    pub column: Option<String>,
}

/// Circuit breaker information for paused jobs
pub struct CircuitBreakerInfo {
    /// Current failure rate
    pub failure_rate: f32,

    /// Threshold that triggered pause
    pub threshold: f32,

    /// Consecutive failure count
    pub consecutive_failures: u32,

    /// When circuit breaker will auto-reset
    pub auto_resume_at: DateTime<Utc>,
}
```

### 6.3 Log Viewer State

```rust
/// State for the full-screen log viewer
pub struct LogViewerState {
    /// Job being viewed
    pub job_id: Uuid,

    /// Job name for display
    pub job_name: String,

    /// All log entries
    pub entries: Vec<LogEntry>,

    /// Current scroll position (line number)
    pub scroll_position: usize,

    /// Total number of lines
    pub total_lines: usize,

    /// Search state
    pub search: Option<LogSearch>,

    /// Line wrapping enabled
    pub wrap_lines: bool,

    /// Is loading more logs
    pub is_loading: bool,
}

/// A single log entry
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

impl LogLevel {
    pub fn color(&self) -> Color {
        match self {
            Self::Debug => Color::Gray,
            Self::Info => Color::White,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
        }
    }
}

/// Search state within log viewer
pub struct LogSearch {
    /// Search query
    pub query: String,

    /// Matching line numbers
    pub matches: Vec<usize>,

    /// Current match index
    pub current_match: usize,

    /// Is search input active
    pub input_active: bool,
}
```

### 6.4 Dialog State

```rust
/// Dialog variants for confirmations
pub enum JobsDialog {
    CancelJob(CancelDialogState),
    ClearCompleted(ClearDialogState),
    ResumeJob(ResumeDialogState),
    RetryWithSuggestion(RetrySuggestionState),
}

pub struct CancelDialogState {
    pub job_id: Uuid,
    pub job_name: String,
    pub progress: String, // "847 / 1,247 files (68%)"
    pub focused_button: DialogButton,
}

pub struct ClearDialogState {
    pub count: u32,
    pub focused_button: DialogButton,
}

pub struct ResumeDialogState {
    pub job_id: Uuid,
    pub job_name: String,
    pub failure_rate: f32,
    pub focused_button: DialogButton,
}

pub struct RetrySuggestionState {
    pub job_id: Uuid,
    pub job_name: String,
    pub suggestion: String,
    pub focused_button: DialogButton,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DialogButton {
    Confirm,
    Cancel,
}
```

---

## 7. Data Sources

### 7.1 Query Reference

| Widget | Query | Refresh |
|--------|-------|---------|
| Job list | See 7.2 | 500ms (running), 5s (else) |
| Job details | Included in job query | On selection change |
| Log entries | See 7.3 | On demand |
| Log count | `SELECT COUNT(*) FROM cf_job_logs WHERE job_id = ?` | On log open |

### 7.2 Job List Query

```sql
SELECT
    j.id,
    j.parser_name,
    p.version as parser_version,
    j.status,
    j.created_at,
    j.started_at,
    j.completed_at,
    j.files_total,
    j.files_processed,
    j.error_count,
    j.warning_count,
    j.error_type,
    j.error_message,
    j.error_file,
    j.error_row,
    j.error_column,
    j.circuit_breaker_failure_rate,
    j.circuit_breaker_threshold,
    j.circuit_breaker_consecutive_failures,
    j.circuit_breaker_resume_at
FROM cf_job_status j
LEFT JOIN cf_parsers p ON j.parser_name = p.name
WHERE (:filter = 'all' OR j.status = :filter)
ORDER BY
    CASE j.status
        WHEN 'running' THEN 1
        WHEN 'failed' THEN 2
        WHEN 'paused' THEN 3
        WHEN 'queued' THEN 4
        WHEN 'cancelled' THEN 5
        WHEN 'complete' THEN 6
    END,
    j.created_at DESC
LIMIT 100;
```

### 7.3 Log Entries Query

```sql
SELECT
    timestamp,
    level,
    message
FROM cf_job_logs
WHERE job_id = :job_id
ORDER BY timestamp ASC
LIMIT :limit OFFSET :offset;
```

### 7.4 Recent Logs for Detail Panel

```sql
SELECT
    timestamp,
    level,
    message
FROM cf_job_logs
WHERE job_id = :job_id
ORDER BY timestamp DESC
LIMIT 5;
```

### 7.5 Job Counts for Filter Dialog

```sql
SELECT
    status,
    COUNT(*) as count
FROM cf_job_status
WHERE created_at > datetime('now', '-7 days')
GROUP BY status;
```

---

## 8. Implementation Notes

### 8.1 Refresh Strategy

- **Running jobs**: Refresh every 500ms for smooth progress updates
- **Other states**: Refresh every 5 seconds
- **Manual refresh**: `r` key (global keybinding)
- **Pause refresh**: When dialog is open or log viewer is active

```rust
impl JobsView {
    const RUNNING_REFRESH_INTERVAL: Duration = Duration::from_millis(500);
    const IDLE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

    fn refresh_interval(&self) -> Duration {
        if self.has_running_jobs() {
            Self::RUNNING_REFRESH_INTERVAL
        } else {
            Self::IDLE_REFRESH_INTERVAL
        }
    }

    fn has_running_jobs(&self) -> bool {
        self.jobs.iter().any(|j| j.status == JobStatus::Running)
    }
}
```

### 8.2 Progress Bar Rendering

```rust
fn render_progress_bar(processed: u32, total: u32, width: usize) -> String {
    let ratio = processed as f64 / total as f64;
    let filled = (ratio * width as f64) as usize;
    let empty = width - filled;

    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}
```

### 8.3 Log Virtualization

For jobs with large logs (>10,000 lines):

```rust
impl LogViewerState {
    const VIEWPORT_BUFFER: usize = 50; // Lines above/below viewport
    const PAGE_SIZE: usize = 500;

    fn visible_range(&self, viewport_height: usize) -> Range<usize> {
        let start = self.scroll_position.saturating_sub(Self::VIEWPORT_BUFFER);
        let end = (self.scroll_position + viewport_height + Self::VIEWPORT_BUFFER)
            .min(self.total_lines);
        start..end
    }

    async fn load_visible_logs(&mut self, db: &SqlitePool) {
        let range = self.visible_range(self.viewport_height);
        // Load only logs in range
        self.entries = load_log_range(db, self.job_id, range).await;
    }
}
```

### 8.4 ETA Calculation

```rust
fn calculate_eta(job: &JobInfo) -> Option<Duration> {
    let started = job.started_at?;
    let elapsed = Utc::now() - started;

    if job.files_processed == 0 {
        return None;
    }

    let rate = job.files_processed as f64 / elapsed.num_seconds() as f64;
    let remaining = job.files_total - job.files_processed;
    let eta_secs = (remaining as f64 / rate) as i64;

    Some(Duration::seconds(eta_secs))
}

fn format_eta(eta: Duration) -> String {
    if eta.num_minutes() > 0 {
        format!("~{}m {}s", eta.num_minutes(), eta.num_seconds() % 60)
    } else {
        format!("~{}s", eta.num_seconds())
    }
}
```

### 8.5 Circuit Breaker Display

```rust
fn render_circuit_breaker_countdown(info: &CircuitBreakerInfo) -> String {
    let remaining = info.auto_resume_at - Utc::now();
    if remaining.num_seconds() <= 0 {
        "Resuming...".to_string()
    } else {
        format!(
            "Auto-Resume: In {}m {}s",
            remaining.num_minutes(),
            remaining.num_seconds() % 60
        )
    }
}
```

### 8.6 Action Validation

```rust
impl JobsViewState {
    fn can_cancel(&self) -> bool {
        self.selected_job()
            .map(|j| j.status == JobStatus::Running || j.status == JobStatus::Queued)
            .unwrap_or(false)
    }

    fn can_retry(&self) -> bool {
        self.selected_job()
            .map(|j| j.status == JobStatus::Failed || j.status == JobStatus::Cancelled)
            .unwrap_or(false)
    }

    fn can_resume(&self) -> bool {
        self.selected_job()
            .map(|j| j.status == JobStatus::Paused)
            .unwrap_or(false)
    }
}
```

### 8.7 View Trait Implementation

```rust
impl View for JobsView {
    fn name(&self) -> &'static str {
        "Jobs"
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.state.state {
            JobsState::Loading => self.render_loading(frame, area),
            JobsState::JobList => self.render_job_list(frame, area),
            JobsState::LogViewer => self.render_log_viewer(frame, area),
            JobsState::ConfirmDialog => {
                self.render_job_list(frame, area);
                self.render_dialog(frame, area);
            }
            JobsState::FilterDialog => {
                self.render_job_list(frame, area);
                self.render_filter_dialog(frame, area);
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> ViewAction {
        match &self.state.state {
            JobsState::Loading => ViewAction::None,
            JobsState::JobList => self.handle_job_list_event(event),
            JobsState::LogViewer => self.handle_log_viewer_event(event),
            JobsState::ConfirmDialog => self.handle_dialog_event(event),
            JobsState::FilterDialog => self.handle_filter_event(event),
        }
    }

    fn help_text(&self) -> Vec<(&'static str, &'static str)> {
        match self.state.state {
            JobsState::JobList => vec![
                ("c", "Cancel"),
                ("r", "Retry"),
                ("l", "Logs"),
                ("f", "Filter"),
                ("x", "Clear"),
                ("?", "Help"),
            ],
            JobsState::LogViewer => vec![
                ("j/k", "Scroll"),
                ("g/G", "Top/Bottom"),
                ("/", "Search"),
                ("Esc", "Close"),
            ],
            _ => vec![
                ("Enter", "Confirm"),
                ("Esc", "Cancel"),
            ],
        }
    }

    fn on_enter(&mut self) {
        self.state.state = JobsState::Loading;
        self.refresh_jobs();
    }

    fn on_leave(&mut self) {
        // Pause auto-refresh when leaving
        self.state.auto_refresh = false;
    }
}
```

### 8.8 Toast Notifications

After actions complete, show toasts per tui.md Section 9.2:

| Action | Toast Message |
|--------|---------------|
| Job cancelled | "{parser_name} cancelled" |
| Job retried | "{parser_name} queued for retry" |
| Jobs cleared | "Cleared {n} completed jobs" |
| Job resumed | "{parser_name} resumed" |
| Job completed | "{parser_name} completed ({n} files)" |
| Job failed | "{parser_name} failed: {error_type}" |

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Expanded from stub: full state machine, data models, workflows, implementation notes |
| 2026-01-12 | 0.1 | Initial stub |

### Trade-offs

1. **Log virtualization complexity vs. memory**: Chose virtualization over loading all logs to handle jobs with 100k+ log lines. Trade-off is added complexity in scroll handling and cache management.

2. **500ms refresh for running jobs**: Aggressive refresh for smooth progress bars, but increases database load. Could be made configurable if performance becomes an issue.

3. **Filter in dropdown vs. tabs**: Chose dropdown to save vertical space and match the telescope pattern used elsewhere. Tabs would be more discoverable but consume screen real estate.

4. **Recent logs in detail panel**: Limited to 5 entries to avoid overwhelming the detail panel. Full logs require explicit 'l' action. Trade-off is extra keystroke for full log access.

5. **Circuit breaker visibility**: Paused jobs are treated as a first-class status rather than a sub-state of Running. This makes circuit breaker issues immediately visible but adds another status to track.

### New Gaps Introduced

1. **GAP-DB-SCHEMA**: The spec assumes `cf_job_logs` table exists for log storage. Need to verify this table exists or add it to schema.

2. **GAP-CIRCUIT-BREAKER-FIELDS**: The spec assumes circuit breaker fields exist in `cf_job_status`. Need to verify these columns exist:
   - `circuit_breaker_failure_rate`
   - `circuit_breaker_threshold`
   - `circuit_breaker_consecutive_failures`
   - `circuit_breaker_resume_at`

3. **GAP-LOG-SEARCH**: The log search feature requires highlighting matching text. Need to decide if this is a Phase 1 or Phase 2 feature.

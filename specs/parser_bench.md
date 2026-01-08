# Parser Bench - TUI Subspec

**Status:** Approved for Implementation
**Parent:** spec.md Section 5.3 (TUI Specification)
**Replaces:** "Process" mode placeholder

---

## 1. Overview

The **Parser Bench** (formerly "Process") is the TUI mode for parser development, testing, and monitoring. Unlike Discover (file organization) or Jobs (queue management), Parser Bench focuses on the **parser-centric view** of the data pipeline.

### 1.1 Design Philosophy

- **Parser is the protagonist**: All views center around parsers, not files
- **Test-driven development**: Encourage rapid iteration with immediate feedback
- **Zero-friction dry runs**: Test any .py file against any data file with minimal setup
- **Progressive disclosure**: Start simple (parser list), drill into details on demand

### 1.2 Name Rationale

"Parser Bench" evokes:
- A **workbench** where you test and refine tools
- A **test bench** from electronics (probe, measure, iterate)
- Clear distinction from "Process" (which sounds like batch execution)

---

## 2. User Workflows

### 2.1 Primary Workflow: Quick Test (Any Parser File)

The core workflow allows testing ANY .py parser file, not just registered parsers.

```
1. User enters Parser Bench (Alt+P)
2. User presses 'n' for new/quick test
3. System shows recent parser files OR file picker
4. User selects parser .py file (or browses to new one)
5. System shows compatible files (smart sampling: failed files first)
6. User selects data file or accepts suggested sample
7. User presses Enter to execute test
8. Results display inline: schema, preview rows, errors with suggestions
9. User iterates (edit parser in IDE, press 'r' to re-run)
```

### 2.2 Secondary Workflow: Test Registered Parser

```
1. User sees list of registered parsers with health badges
2. User selects a parser (j/k navigation)
3. User presses 't' to enter test mode
4. System shows bound files (smart sampling: high-failure first)
5. User selects sample file or accepts suggestion
6. User presses Enter to execute dry run
7. Results display inline
```

### 2.3 Tertiary Workflow: Monitor Parser Health

```
1. User sees parser list with health indicators
2. Red indicator on paused/unhealthy parsers
3. User selects unhealthy parser
4. Detail pane shows: consecutive failures, last error, success rate
5. User presses 'R' to resume paused parser
```

### 2.4 Background Backtest

```
1. User selects a registered parser
2. User presses 'b' to start backtest
3. Progress bar displays in right panel
4. User can press Esc to send to background and continue working
5. Progress visible in Jobs mode
```

---

## 3. Layout Specification

### 3.1 Three-Panel Design

```
┌───────────────────────────────────────────────────────────────────────────────┐
│  PARSER BENCH                                                      [Alt+P]    │
├────────────────────┬──────────────────────────────────────────────────────────┤
│  PARSERS           │  DETAIL / TEST RESULTS                                   │
│  ────────          │  ──────────────────────                                  │
│  RECENT            │  sales_parser v1.0.2                                     │
│  ► my_parser.py    │  ───────────────────────────────────────                 │
│    invoice.py      │  Topics: [sales_data, invoices]                          │
│                    │  Files:  142 matched, 138 processed                      │
│  REGISTERED        │  Health: ● HEALTHY (98.5% success)                       │
│  ● sales_parser    │                                                          │
│  ○ invoice_parser  │  SCHEMA (6 columns)                                      │
│  ⏸ log_analyzer    │  ─────────────────────────────────────────               │
│                    │  │ Column       │ Type     │ Nullable │                  │
│  ────────          │  ├──────────────┼──────────┼──────────┤                  │
│  [n] New test      │  │ id           │ Int64    │ No       │                  │
│  [t] Test          │  │ customer     │ String   │ Yes      │                  │
│  [f] Files         │  │ amount       │ Float64  │ No       │                  │
│  [b] Backtest      │  │ date         │ Date     │ No       │                  │
│  [R] Resume        │  └──────────────┴──────────┴──────────┘                  │
└────────────────────┴──────────────────────────────────────────────────────────┘
│  [n] New test  [j/k] Navigate  [t] Test  [f] Files  [b] Backtest  [Esc] Home  │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Panel Descriptions

#### Left Panel: Parser List (Two Sections)

**Recent Section**:
- Recently tested parser files (not necessarily registered)
- Stored in local state file (`~/.casparian_flow/recent_parsers.json`)
- Maximum 5 entries, LRU eviction
- Shows filename only (hover/select for full path)

**Registered Section**:
- All parsers from `parser_lab_parsers` + `cf_parsers`
- Columns: Name, Version, Topics (collapsed), Health badge
- Health Icons:
  - `●` = Healthy (green)
  - `○` = Pending validation (yellow)
  - `⚠` = Warning - consecutive failures (orange)
  - `⏸` = Paused - circuit breaker tripped (red)

#### Right Panel: Detail View (Context-Dependent)

**Parser Info View (default when parser selected)**:
- Parser name and version
- Subscribed topics (comma-separated)
- File statistics: matched / processed / failed
- Health status with success rate
- Schema preview (if available)

**Test Results View (after running test)**:
- Success/Failure status with icon
- Execution time
- Rows processed
- Schema (columns with types)
- Output preview (first 5 rows as table)
- On failure: error message + suggestions

**File Picker View (when selecting data file)**:
- Smart sampling: show failed files first, then random
- Filter by path substring
- File info: path, size, status

### 3.3 Quick Actions Bar (Bottom)

Context-sensitive keyboard shortcuts:
- Always: `[n] New test  [Esc] Home  [?] Help`
- Parser selected: `[t] Test  [f] Files  [b] Backtest`
- Paused parser: `[R] Resume`

---

## 4. State Machine

```
                    ┌───────────────────┐
                    │                   │
     ┌─────────────►│   PARSER_LIST     │◄─────────────┐
     │              │    (default)      │              │
     │              │                   │              │
     │              └─────────┬─────────┘              │
     │                        │                        │
     │    ┌───────────────────┼───────────────────┐    │
     │    │         │         │         │         │    │
     │    ▼         ▼         ▼         ▼         ▼    │
     │ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐   │
     │ │PARSER│ │ FILE │ │ TEST │ │FILES │ │BACK- │   │
Esc  │ │PICKER│ │PICKER│ │ MODE │ │ VIEW │ │ TEST │   │ Esc
     │ └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘   │
     │    │        │        │        │        │       │
     │    │ select │ select │ Enter  │ Enter  │ Esc   │
     │    ▼        ▼        ▼        ▼        ▼       │
     │ ┌──────────────────────────────────────────┐   │
     │ │              TEST_RUNNING                │   │
     └─│          (async, can background)         │───┘
       └──────────────────────────────────────────┘
```

### 4.1 State Definitions

| State | Entry | Description | Exit |
|-------|-------|-------------|------|
| `PARSER_LIST` | Default, Esc from any | Show recent + registered parsers | n, t, f, b, Enter |
| `PARSER_PICKER` | 'n' from list | Browse for .py file | Select or Esc |
| `FILE_PICKER` | After parser selected | Choose data file for test | Select or Esc |
| `TEST_MODE` | 't' on registered parser | View bound files, select for test | Enter or Esc |
| `FILES_VIEW` | 'f' on registered parser | Browse all bound files | Enter (dry run) or Esc |
| `BACKTEST` | 'b' on registered parser | Progress view, can background | Esc (backgrounds it) |
| `TEST_RUNNING` | Enter on file | Async execution | Completes or Ctrl+C |

---

## 5. Data Model

### 5.1 ParserBenchState

```rust
pub struct ParserBenchState {
    // View mode
    pub view: ParserBenchView,

    // Parser lists
    pub recent_parsers: Vec<RecentParser>,
    pub registered_parsers: Vec<RegisteredParser>,
    pub selected_index: usize,
    pub in_recent_section: bool,  // true = navigating recent, false = registered

    // File picker state
    pub picker_files: Vec<FileInfo>,
    pub picker_selected: usize,
    pub picker_filter: String,

    // Test state
    pub current_parser: Option<ParserSelection>,
    pub current_data_file: Option<PathBuf>,
    pub test_result: Option<TestResult>,
    pub test_running: bool,

    // Backtest state
    pub backtest: Option<BacktestProgress>,

    // Loaded flags
    pub data_loaded: bool,
}

#[derive(Debug, Clone)]
pub enum ParserBenchView {
    ParserList,
    ParserPicker,
    FilePicker,
    TestMode,
    FilesView,
    Backtest,
}

#[derive(Debug, Clone)]
pub struct RecentParser {
    pub path: PathBuf,
    pub name: String,  // filename without extension
    pub last_used: DateTime<Local>,
}

#[derive(Debug, Clone)]
pub struct RegisteredParser {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub topics: Vec<String>,
    pub health: ParserHealth,
    pub file_count: usize,
    pub processed_count: usize,
    pub failed_count: usize,
    pub schema: Option<Vec<SchemaColumn>>,
}

#[derive(Debug, Clone)]
pub enum ParserHealth {
    Healthy { success_rate: f64 },
    Warning { consecutive_failures: u32, threshold: u32 },
    Paused { reason: String, paused_at: DateTime<Local> },
    Pending,  // Not yet validated
}

#[derive(Debug, Clone)]
pub enum ParserSelection {
    Recent(PathBuf),
    Registered { name: String, source_code: String },
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Option<Vec<SchemaColumn>>,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BacktestProgress {
    pub parser_name: String,
    pub total_files: usize,
    pub processed: usize,
    pub passed: usize,
    pub failed: usize,
    pub current_file: Option<String>,
    pub started_at: DateTime<Local>,
    pub is_background: bool,
}
```

---

## 6. Keybindings

### 6.1 Parser List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Tab` | Toggle between Recent and Registered sections |
| `n` | New quick test (open parser picker) |
| `t` | Test selected parser |
| `f` | View files bound to parser (registered only) |
| `b` | Start backtest (registered only) |
| `R` | Resume paused parser |
| `Enter` | Quick action: test if recent, details if registered |
| `/` | Filter parsers by name |
| `Esc` | Return to Home |
| `?` | Show help overlay |

### 6.2 Parser Picker (File Browser)

| Key | Action |
|-----|--------|
| `j/k` | Navigate files |
| `Enter` | Select .py file |
| `/` | Filter by path |
| `Tab` | Toggle hidden files |
| `Esc` | Cancel, back to list |

### 6.3 File Picker (Data File Selection)

| Key | Action |
|-----|--------|
| `j/k` | Navigate files |
| `Enter` | Select file and run test |
| `/` | Filter by path |
| `1-4` | Filter by status (all/pending/processed/failed) |
| `Esc` | Cancel, back to list |

### 6.4 Test Running / Results View

| Key | Action |
|-----|--------|
| `r` | Re-run test with same files |
| `f` | Pick different data file |
| `Esc` | Back to parser list |

### 6.5 Backtest Progress

| Key | Action |
|-----|--------|
| `Esc` | Send to background, return to list |
| `Ctrl+C` | Cancel backtest |

---

## 7. Smart Sampling Algorithm

When selecting sample files for testing, prioritize in this order:

```rust
fn smart_sample_files(files: &[FileInfo], limit: usize) -> Vec<&FileInfo> {
    let mut result = Vec::new();

    // 1. Files that failed in previous runs (high-failure tracking)
    let failed: Vec<_> = files.iter()
        .filter(|f| f.status == "failed")
        .collect();
    result.extend(failed.iter().take(limit / 2));

    // 2. Files never processed
    let pending: Vec<_> = files.iter()
        .filter(|f| f.status == "pending")
        .collect();
    result.extend(pending.iter().take(limit / 4));

    // 3. Random sample from processed (sanity check)
    let processed: Vec<_> = files.iter()
        .filter(|f| f.status == "processed")
        .collect();
    result.extend(processed.iter().take(limit / 4));

    result.truncate(limit);
    result
}
```

---

## 8. UI Components

### 8.1 Health Badge (Inline in List)

```
● sales_parser v1.2  [sales] 98%     <- Healthy, green
⚠ invoice_parser     [inv]   3/5     <- Warning, 3 consecutive failures
⏸ log_analyzer       [logs]  PAUSED  <- Circuit breaker tripped, red
○ csv_cleaner        [csv]   --      <- Pending validation, gray
```

### 8.2 Test Result: Success

```
┌─ TEST RESULT ─────────────────────────────────────────────────┐
│                                                               │
│  ✓ PASSED                                    45ms  1,234 rows │
│                                                               │
├─ SCHEMA (6 columns) ──────────────────────────────────────────┤
│  id: Int64  customer: String  amount: Float64  date: Date     │
│  status: String  _cf_source_hash: String                      │
├─ PREVIEW ─────────────────────────────────────────────────────┤
│  │ id │ customer  │ amount  │ date       │ status  │          │
│  ├────┼───────────┼─────────┼────────────┼─────────┤          │
│  │ 1  │ Acme Inc  │ 1500.00 │ 2024-01-15 │ pending │          │
│  │ 2  │ Beta LLC  │  750.50 │ 2024-01-16 │ shipped │          │
│  │ 3  │ Gamma Co  │ 2100.00 │ 2024-01-17 │ pending │          │
├───────────────────────────────────────────────────────────────┤
│  [r] Re-run  [f] Different file  [Esc] Back                   │
└───────────────────────────────────────────────────────────────┘
```

### 8.3 Test Result: Failure

```
┌─ TEST RESULT ─────────────────────────────────────────────────┐
│                                                               │
│  ✗ FAILED                                   12ms  SCHEMA_MISMATCH
│                                                               │
├─ ERROR ───────────────────────────────────────────────────────┤
│  KeyError: 'customer_id'                                      │
│                                                               │
│  File "my_parser.py", line 12, in transform                   │
│    return df[['customer_id', 'amount', 'date']]               │
│           ~~~^^^^^^^^^^^^^^^                                  │
├─ SUGGESTIONS ─────────────────────────────────────────────────┤
│  • Column 'customer_id' not found in data                     │
│  • Available columns: id, cust_id, amount, date, status       │
│  • Did you mean 'cust_id'?                                    │
├───────────────────────────────────────────────────────────────┤
│  [r] Re-run  [f] Different file  [Esc] Back                   │
└───────────────────────────────────────────────────────────────┘
```

### 8.4 Backtest Progress

```
┌─ BACKTEST: sales_parser ──────────────────────────────────────┐
│                                                               │
│  [████████████████░░░░░░░░░░░░░░░░░░░░░░░░] 52/142 (36.6%)   │
│                                                               │
│  Passed: 48    Failed: 4    ETA: ~45s                        │
│                                                               │
│  Current: data/invoices/inv_2024_052.csv                      │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│  [Esc] Run in background   [Ctrl+C] Cancel                    │
└───────────────────────────────────────────────────────────────┘
```

---

## 9. Recent Parsers Storage

Recent parsers are stored locally for quick access:

**File**: `~/.casparian_flow/recent_parsers.json`

```json
{
  "version": 1,
  "parsers": [
    {
      "path": "/Users/dev/parsers/my_parser.py",
      "last_used": "2026-01-07T10:30:00Z"
    },
    {
      "path": "/Users/dev/project/invoice_parser.py",
      "last_used": "2026-01-06T15:45:00Z"
    }
  ]
}
```

**Rules**:
- Maximum 5 entries (configurable)
- LRU eviction when full
- Entries removed if file no longer exists (checked on load)
- Updated when test is run (not just selected)

---

## 10. Integration with Existing Code

### 10.1 Reuse `run_parser_test`

The existing `run_parser_test` function in `cli/parser.rs` provides exactly what we need:

```rust
// From parser.rs - already handles:
// - Python subprocess execution
// - Schema inference
// - Preview row extraction
// - Error classification (SCHEMA_MISMATCH, etc.)
// - Structured error codes
fn run_parser_test(
    parser_path: &PathBuf,
    input_path: &PathBuf,
    preview_rows: usize,
) -> Result<(bool, usize, Option<Vec<SchemaColumn>>, Vec<Vec<String>>, Vec<String>, Vec<String>, Option<String>)>
```

### 10.2 Reuse Parser Health Queries

```rust
// From parser.rs cmd_health - query pattern:
let health: Option<(String, i64, i64, i32, Option<String>, Option<String>)> = sqlx::query_as(
    r#"
    SELECT parser_name, total_executions, successful_executions, consecutive_failures,
           last_failure_reason, paused_at
    FROM cf_parser_health
    WHERE parser_name = ?
    "#
)
.bind(name)
.fetch_optional(&pool)
.await?;
```

### 10.3 Reuse Backtest Logic

Trigger CLI backtest in background and poll for progress:

```rust
// Spawn backtest as background process
let child = Command::new("casparian")
    .args(["parser", "backtest", &parser_name, "--json"])
    .stdout(Stdio::piped())
    .spawn()?;

// Track in BacktestProgress, poll stdout for JSON progress updates
```

---

## 11. Implementation Phases

### Phase 1: Core Structure
- [ ] Add `ParserBenchState` to `app.rs`
- [ ] Add `ParserBenchView` enum
- [ ] Stub out `draw_parser_bench_screen` in `ui.rs`
- [ ] Basic navigation between views

### Phase 2: Parser List
- [ ] Load registered parsers from DB
- [ ] Load recent parsers from JSON file
- [ ] Render two-section list with health badges
- [ ] Tab to switch sections
- [ ] j/k navigation, Enter selection

### Phase 3: Quick Test Flow
- [ ] Parser picker (simple file browser for .py)
- [ ] File picker (data file selection with smart sampling)
- [ ] Integration with `run_parser_test`
- [ ] Results display (success/failure)
- [ ] Re-run functionality (`r` key)
- [ ] Update recent parsers on successful test

### Phase 4: Registered Parser Features
- [ ] Test mode for registered parsers
- [ ] Files view (bound files with status filter)
- [ ] Resume paused parser (`R` key)

### Phase 5: Backtest
- [ ] Backtest trigger
- [ ] Progress view with progress bar
- [ ] Background mode (Esc to continue working)
- [ ] Integration with Jobs mode for tracking

### Phase 6: Polish
- [ ] Filter/search (`/` key)
- [ ] Help overlay (`?` key)
- [ ] Error suggestions (column name matching, etc.)
- [ ] Keyboard shortcuts in footer

---

## 12. Decisions Made

Based on requirements gathering:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sample file selection | Smart sampling | Prioritize failed files first for faster iteration |
| Parser input | Recent files + picker | Balance convenience with flexibility |
| Test flow | Quick test any .py file | Zero friction for development workflow |
| List columns | Name + Version + Topics + Health | Complete picture without clutter |
| Backtest mode | Background with progress | User can continue working |
| Result diff | No diff (v1) | Keep simple, show latest result only |
| Error actions | Show + suggestions | IDE is for editing, TUI for running |

---

## 13. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-07 | 0.1 | Initial draft |
| 2026-01-07 | 0.2 | Incorporated decisions: quick test flow, smart sampling, recent files list, background backtest |

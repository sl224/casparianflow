# Parser Bench - TUI View Spec

**Status:** Approved for Implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.2
**Related:** specs/views/jobs.md, specs/views/discover.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Parser Bench** (formerly "Process") is the TUI mode for parser development, testing, and monitoring. It provides a workbench for iterating on parsers with immediate feedback.

### 1.1 Design Philosophy

- **Filesystem-first**: Parsers live in a standard directory (`~/.casparian_flow/parsers/`)
- **Zero ceremony**: Drop a file in the directory, it appears in the list
- **Symlinks for development**: Symlink from your project for live editing
- **Test-driven iteration**: Run, see results, edit in IDE, re-run

### 1.2 Parsers Directory

```
~/.casparian_flow/
├── parsers/                    # <-- Parser plugins directory
│   ├── sales_parser.py         # User copies or symlinks here
│   ├── invoice_parser.py       # Flat structure only
│   └── log_analyzer.py         # No subdirectories
├── casparian_flow.sqlite3
└── output/
```

**Rules**:
- Only `.py` files directly in `parsers/` are discovered
- No subdirectories (flat structure)
- Symlinks are fully supported (encouraged for development)
- Broken symlinks shown with error state (user can delete)
- User manages files manually (cp, ln -s, mv, rm)

---

## 2. User Workflows

### 2.1 Primary Workflow: Test a Parser

```
1. User copies/symlinks parser to ~/.casparian_flow/parsers/
2. User enters Parser Bench (press 2 from any view)
3. Parser appears in list automatically
4. User selects parser, presses 't' to test
5. System shows compatible files (smart sampling: failed first)
6. User selects data file
7. Results display: schema, preview rows, errors with suggestions
8. User edits parser in IDE, presses 'r' to re-run
```

### 2.2 Quick Test (Arbitrary File)

For one-off testing without adding to parsers directory:

```
1. User presses 'n' for new/quick test
2. File picker opens
3. User navigates to any .py file
4. User selects data file
5. Results display
6. (Optional) "Add to parsers directory?" prompt on success
```

### 2.3 Monitor Parser Health

```
1. Parser list shows health badges (●/⚠/⏸/✗)
2. User sees paused parser (circuit breaker tripped)
3. User presses 'R' to resume
```

### 2.4 Background Backtest

```
1. User selects parser, presses 'b'
2. Progress bar displays
3. User presses Esc to background and continue working
4. Results visible in Jobs mode
```

### 2.5 File Watcher Mode (Optional)

```
1. User presses 'w' to enable watch mode on selected parser
2. "Watching..." indicator appears
3. User edits parser in IDE, saves
4. TUI auto-detects save, re-runs last test
5. Results update automatically
6. Press 'w' again to disable
```

---

## 3. Parser Metadata Discovery

**CRITICAL IMPLEMENTATION NOTE**: Metadata extraction MUST happen via Python subprocess, NOT Rust AST parsing. The TUI is Rust; you cannot `import ast` in Rust.

### 3.1 Parser Class Format

```python
class SalesParser:
    name = 'sales_parser'           # Required: logical name
    version = '1.0.0'               # Required: semver
    topics = ['sales_data']         # Required: subscribed topics

    def transform(self, df):
        # ... transformation logic
        return df
```

### 3.2 Metadata Extraction (Batch Processing)

**CRITICAL**: Metadata extraction uses **batch processing** to avoid spawning one Python subprocess per file. The TUI collects all parser paths, then processes them in chunks of 50.

**Implementation**: Embedded Python script in `app.rs` as `METADATA_EXTRACTOR_SCRIPT`. The script reads a JSON array of paths from stdin and outputs a JSON object keyed by path.

```python
#!/usr/bin/env python3
"""Extract parser metadata via AST parsing (no execution).
Batch mode: reads JSON array of paths from stdin."""
import ast
import json
import sys
import os

def extract_metadata(path: str) -> dict:
    try:
        source = open(path).read()
        tree = ast.parse(source)
    except Exception as e:
        return {"error": str(e)}

    result = {
        "name": None,
        "version": None,
        "topics": [],
        "has_transform": False,
        "has_parse": False,
    }

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for item in node.body:
                # Class attributes
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            try:
                                value = ast.literal_eval(item.value)
                                if target.id == "name":
                                    result["name"] = value
                                elif target.id == "version":
                                    result["version"] = value
                                elif target.id == "topics":
                                    result["topics"] = value if isinstance(value, list) else [value]
                            except:
                                pass
                # Methods
                elif isinstance(item, ast.FunctionDef):
                    if item.name == "transform":
                        result["has_transform"] = True
                    elif item.name == "parse":
                        result["has_parse"] = True

    # Fallback: use filename if no name attribute
    if result["name"] is None:
        result["name"] = os.path.splitext(os.path.basename(path))[0]

    return result

if __name__ == "__main__":
    # Batch mode: read JSON array of paths from stdin
    paths = json.load(sys.stdin)
    results = {}
    for path in paths:
        results[path] = extract_metadata(path)
    print(json.dumps(results))
```

**Rust Integration** (in `app.rs`):

```rust
const METADATA_BATCH_SIZE: usize = 50;

fn extract_parser_metadata_batch(
    paths: &[PathBuf],
) -> HashMap<String, (String, Option<String>, Vec<String>)> {
    // Spawn python3 (fallback to python)
    // Write JSON array of paths to stdin
    // Read JSON object from stdout
    // Parse: {"path": {"name": ..., "version": ..., "topics": [...]}, ...}
}

fn load_parsers(&mut self) {
    // 1. Scan directory, collect paths + filesystem metadata
    // 2. Filter out broken symlinks (they get fallback name)
    // 3. Process in chunks of METADATA_BATCH_SIZE
    for chunk in paths.chunks(METADATA_BATCH_SIZE) {
        let batch_results = Self::extract_parser_metadata_batch(chunk);
        all_metadata.extend(batch_results);
    }
    // 4. Build ParserInfo structs
}
```

**Benefits**:
- 222 parsers = 5 subprocess calls (not 222)
- Uses stdin/stdout to avoid command line length limits
- Graceful fallback if any batch fails

### 3.3 Fallback Behavior

If metadata extraction fails or returns partial data:
- **name**: Use filename without extension
- **version**: Display as "—" (unknown)
- **topics**: Empty list (manual file selection only)

---

## 4. State Machine

### 4.1 State Diagram

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                         PARSER BENCH STATE MACHINE                             │
│                                                                                │
│    ┌─────────────────────────────────────────────────────────────────────┐     │
│    │                                                                     │     │
│    │                        ┌─────────────────┐                          │     │
│    │                 ┌──────│   PARSER_LIST   │──────┐                   │     │
│    │                 │      │   (initial)     │      │                   │     │
│    │                 │      └────────┬────────┘      │                   │     │
│    │                 │               │               │                   │     │
│    │         n       │          t    │   f           │  b                │     │
│    │                 ▼               ▼               ▼                   │     │
│    │    ┌────────────────┐  ┌─────────────┐  ┌─────────────┐            │     │
│    │    │ QUICK_TEST_    │  │FILE_PICKER  │  │ FILES_VIEW  │            │     │
│    │    │ PICKER         │  │             │  │             │            │     │
│    │    └───────┬────────┘  └──────┬──────┘  └──────┬──────┘            │     │
│    │            │                  │                │                    │     │
│    │            │ select parser    │ select file    │ Enter              │     │
│    │            │                  │                │ (test file)        │     │
│    │            └──────────────────┼────────────────┘                    │     │
│    │                               │                                     │     │
│    │                               ▼                                     │     │
│    │                       ┌─────────────┐                               │     │
│    │                       │ RESULT_VIEW │◄──────────────────────────────┤     │
│    │                       │             │      completion               │     │
│    │                       └──────┬──────┘                               │     │
│    │                              │                                      │     │
│    │              ┌───────────────┼───────────────┐                      │     │
│    │              │               │               │                      │     │
│    │          r   │           f   │           Esc │                      │     │
│    │       (rerun)│    (diff file)│        (back) │                      │     │
│    │              ▼               ▼               │                      │     │
│    │         [re-run test]   FILE_PICKER         ─┘                      │     │
│    │                                                                     │     │
│    └─────────────────────────────────────────────────────────────────────┘     │
│                                                                                │
│    ┌─────────────────────────────────────────────────────────────────────┐     │
│    │                         BACKTEST (async)                            │     │
│    │                                                                     │     │
│    │     PARSER_LIST ──b──► BACKTEST ──Esc──► [backgrounds to Jobs]      │     │
│    │                            │                                        │     │
│    │                            └─completion─► RESULT_VIEW               │     │
│    └─────────────────────────────────────────────────────────────────────┘     │
│                                                                                │
│    Esc from any picker state returns to PARSER_LIST                            │
│    Alt+P from any state returns to PARSER_LIST (mode re-entry)                 │
│    Alt+H from any state returns to HOME_HUB                                    │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| PARSER_LIST | Mode entry, Esc from child states | n/t/f/b keys, Alt+H | List parsers from `~/.casparian_flow/parsers/`, show health icons |
| QUICK_TEST_PICKER | 'n' from PARSER_LIST | Select file, Esc | File picker for arbitrary .py files (not in parsers dir) |
| FILE_PICKER | 't' from PARSER_LIST, 'f' from RESULT_VIEW | Select file, Esc | Show files bound to parser via topics; smart sampling |
| FILES_VIEW | 'f' from PARSER_LIST | Enter (test), Esc | Browse all files bound to selected parser |
| BACKTEST | 'b' from PARSER_LIST | Esc (backgrounds), completion | Run parser against all bound files; show progress |
| RESULT_VIEW | Test completion, backtest completion | r/f/Esc | Display test results: schema, preview rows, errors |

### 4.3 Transitions

| From | To | Trigger | Guard |
|------|----|---------|-------|
| PARSER_LIST | QUICK_TEST_PICKER | 'n' | — |
| PARSER_LIST | FILE_PICKER | 't' | Parser selected |
| PARSER_LIST | FILES_VIEW | 'f' | Parser selected |
| PARSER_LIST | BACKTEST | 'b' | Parser selected |
| QUICK_TEST_PICKER | FILE_PICKER | Select parser | Valid .py file |
| QUICK_TEST_PICKER | PARSER_LIST | Esc | — |
| FILE_PICKER | RESULT_VIEW | Select file | Test completes |
| FILE_PICKER | PARSER_LIST | Esc | — |
| FILES_VIEW | RESULT_VIEW | Enter | File selected |
| FILES_VIEW | PARSER_LIST | Esc | — |
| BACKTEST | RESULT_VIEW | Completion | All files processed |
| BACKTEST | PARSER_LIST | Esc | Backgrounds job |
| RESULT_VIEW | RESULT_VIEW | 'r' | — (re-run) |
| RESULT_VIEW | FILE_PICKER | 'f' | — |
| RESULT_VIEW | PARSER_LIST | Esc | — |
| any | HOME_HUB | Alt+H | — (global) |

### 4.4 Focus Mode Overlay

Focus mode (`z` key) is an **overlay**, not a separate state. It affects rendering only:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                            FOCUS MODE                                    │
│                                                                          │
│   Can be toggled in: RESULT_VIEW, FILES_VIEW                             │
│                                                                          │
│   When active:                                                           │
│   - Left panel hidden                                                    │
│   - Right panel fullscreen                                               │
│   - All keybindings preserved                                            │
│   - 'z' toggles off                                                      │
└──────────────────────────────────────────────────────────────────────────┘
```

### 4.5 Watch Mode Overlay

Watch mode (`w` key) is also an **overlay** that can be active in any state:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                            WATCH MODE                                    │
│                                                                          │
│   When active:                                                           │
│   - File watcher monitors selected parser                                │
│   - On parser save: auto-trigger last test                               │
│   - "Watching..." indicator in header                                    │
│   - 'w' toggles off                                                      │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Preview Mode Safety

**CRITICAL**: Prevent OOM on large files by limiting rows read.

### 5.1 Protocol Extension

```rust
// In casparian_protocol/src/types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub parser_path: String,
    pub data_path: String,
    pub row_limit: usize,  // CRITICAL: limits pd.read_csv(nrows=...)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Vec<SchemaColumn>,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub error_type: Option<String>,
    pub suggestions: Vec<String>,
    pub truncated: bool,  // True if row_limit was hit
}
```

### 5.2 Python Implementation

The wrapper script in `run_parser_test` MUST pass `nrows`:

```python
# CRITICAL: Limit rows to prevent OOM
ROW_LIMIT = {row_limit}

if USE_POLARS:
    if input_path.endswith('.csv'):
        df = pl.read_csv(input_path, n_rows=ROW_LIMIT)
    # ... etc
else:
    if input_path.endswith('.csv'):
        df = pd.read_csv(input_path, nrows=ROW_LIMIT)
    # ... etc

# Track if we truncated
truncated = len(df) >= ROW_LIMIT
```

### 5.3 Default Limits

| Context | Row Limit | Rationale |
|---------|-----------|-----------|
| Quick test (TUI) | 1,000 | Fast feedback, low memory |
| Backtest preview | 100 | Just checking schema |
| CLI `parser test` | 10,000 | More thorough but bounded |
| Full run | Unlimited | Production execution |

---

## 6. Layout Specification

### 6.1 Two-Panel Design with Focus Mode

Normal mode:
```
┌───────────────────────────────────────────────────────────────────────────────┐
│  PARSER BENCH                                             [w] watching  Alt+P │
├────────────────────┬──────────────────────────────────────────────────────────┤
│  PARSERS           │  sales_parser v1.0.0                                     │
│  ~/.../parsers/    │  ───────────────────────────────────────                 │
│  ────────────────  │  File:    ~/.casparian_flow/parsers/sales_parser.py     │
│  ► sales_parser    │  Topics:  sales_data, invoices                          │
│    invoice_parser  │  Modified: 2 hours ago                                   │
│  ● log_analyzer    │  Health:  ● HEALTHY (142 runs, 98.5%)                   │
│  ⏸ csv_cleaner     │                                                          │
│  ✗ broken_link.py  │  BOUND FILES (12 matched)                                │
│                    │  ─────────────────────────────────────────               │
│                    │  data/sales/jan.csv         2.1KB  ✓ processed          │
│                    │  data/sales/feb.csv         1.8KB  ○ pending            │
│  ────────────────  │  data/invoices/inv_001.csv  3.2KB  ✗ failed             │
│  [n] Quick test    │                                                          │
│  [t] Test parser   │  [Enter] Test with selected file                         │
│  [w] Watch mode    │                                                          │
│  [z] Focus mode    │                                                          │
└────────────────────┴──────────────────────────────────────────────────────────┘
```

Focus mode (`z` pressed - right panel fullscreen):
```
┌───────────────────────────────────────────────────────────────────────────────┐
│  PARSER BENCH > sales_parser                              [z] exit focus Alt+P│
├───────────────────────────────────────────────────────────────────────────────┤
│  TEST RESULT                                                                  │
│  ───────────────────────────────────────                                      │
│  ✗ FAILED                                              12ms   SCHEMA_MISMATCH │
│                                                                               │
│  ERROR                                                                        │
│  ─────                                                                        │
│  KeyError: 'customer_id'                                                      │
│                                                                               │
│  File "sales_parser.py", line 12, in transform                                │
│    return df[['customer_id', 'amount', 'date']]                               │
│               ^^^^^^^^^^^^^^                                                  │
│                                                                               │
│  SUGGESTIONS                                                                  │
│  ───────────                                                                  │
│  • Column 'customer_id' not found in data                                     │
│  • Available columns: id, cust_id, amount, date, status                       │
│  • Did you mean 'cust_id'?                                                    │
│                                                                               │
├───────────────────────────────────────────────────────────────────────────────┤
│  [r] Re-run  [f] Different file  [a] Analyze with AI  [z] Exit focus  [Esc]  │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Parser States

| Icon | State | Description |
|------|-------|-------------|
| `►` | Selected | Currently highlighted |
| `●` | Healthy | Green, success rate > 90% |
| `○` | Unknown | Gray, never run |
| `⚠` | Warning | Yellow, consecutive failures |
| `⏸` | Paused | Red, circuit breaker tripped |
| `✗` | Broken | Red, broken symlink |

### 6.3 Test Result Views

**Success View**:
```
┌─ TEST RESULT ─────────────────────────────────────────────────────────────────┐
│                                                                               │
│  ✓ PASSED                                              45ms    1,234 rows    │
│  (showing first 1,000 rows - file has more)                                  │
│                                                                               │
├─ SCHEMA (6 columns) ──────────────────────────────────────────────────────────┤
│  id: Int64  customer: String  amount: Float64  date: Date  status: String    │
│                                                                               │
├─ PREVIEW (first 5 rows) ──────────────────────────────────────────────────────┤
│  │ id │ customer  │ amount  │ date       │ status  │                         │
│  ├────┼───────────┼─────────┼────────────┼─────────┤                         │
│  │ 1  │ Acme Inc  │ 1500.00 │ 2024-01-15 │ pending │                         │
│  │ 2  │ Beta LLC  │  750.50 │ 2024-01-16 │ shipped │                         │
│  │ 3  │ Gamma Co  │ 2100.00 │ 2024-01-17 │ pending │                         │
│                                                                               │
├───────────────────────────────────────────────────────────────────────────────┤
│  [r] Re-run  [f] Different file  [Esc] Back                                   │
└───────────────────────────────────────────────────────────────────────────────┘
```

**Failure View with AI Analyze Option** (Phase 6):
```
├───────────────────────────────────────────────────────────────────────────────┤
│  [r] Re-run  [f] Different file  [a] Analyze with AI  [Esc] Back             │
└───────────────────────────────────────────────────────────────────────────────┘
```

When `[a]` pressed, sends to Claude Code sidebar:
- Parser source code
- Error traceback
- First 5 lines of input data

---

## 7. Data Model

```rust
pub struct ParserBenchState {
    // View mode
    pub view: ParserBenchView,
    pub focus_mode: bool,  // Right panel fullscreen

    // Parser list (from ~/.casparian_flow/parsers/)
    pub parsers: Vec<ParserInfo>,
    pub selected_parser: usize,
    pub parsers_loaded: bool,

    // Quick test state (for arbitrary files)
    pub quick_test_path: Option<PathBuf>,

    // File picker
    pub picker_files: Vec<FileInfo>,
    pub picker_selected: usize,
    pub picker_filter: String,

    // Test state
    pub test_running: bool,
    pub test_result: Option<TestResult>,
    pub last_test_file: Option<PathBuf>,  // For re-run

    // Watch mode
    pub watch_enabled: bool,
    pub watcher: Option<notify::RecommendedWatcher>,

    // Backtest
    pub backtest: Option<BacktestProgress>,
}

#[derive(Debug, Clone)]
pub enum ParserBenchView {
    ParserList,
    QuickTestPicker,
    FilePicker,
    FilesView,
    Backtest,
    ResultView,
}

#[derive(Debug, Clone)]
pub struct ParserInfo {
    pub path: PathBuf,
    pub name: String,               // From metadata or filename
    pub version: Option<String>,
    pub topics: Vec<String>,
    pub modified: DateTime<Local>,
    pub health: ParserHealth,
    pub is_symlink: bool,
    pub symlink_broken: bool,       // NEW: broken symlink detection
}

#[derive(Debug, Clone)]
pub enum ParserHealth {
    Healthy { success_rate: f64, total_runs: usize },
    Warning { consecutive_failures: u32 },
    Paused { reason: String },
    Unknown,
    BrokenLink,  // NEW: for broken symlinks
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
    pub error_type: Option<String>,
    pub truncated: bool,  // NEW: row limit was hit
}
```

---

## 8. Keybindings

### 8.1 Parser List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `n` | Quick test (pick any .py file) |
| `t` / `Enter` | Test selected parser |
| `f` | View files bound to parser |
| `b` | Start backtest |
| `R` | Resume paused parser |
| `w` | Toggle watch mode |
| `z` | Toggle focus mode |
| `/` | Filter by name |
| `d` | Delete broken symlink |
| `Esc` | Return to Home |
| `?` | Help overlay |

### 8.2 Result View

| Key | Action |
|-----|--------|
| `r` | Re-run test |
| `f` | Pick different file |
| `a` | Analyze with AI (Phase 6) |
| `z` | Toggle focus mode |
| `Esc` | Back to list |

### 8.3 Watch Mode Active

| Key | Action |
|-----|--------|
| `w` | Disable watch mode |
| (auto) | Re-run on file save |

---

## 9. File Watcher Implementation

Use `notify` crate with debouncing:

```rust
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config};
use std::time::Duration;

fn setup_watcher(parser_path: &Path, tx: Sender<()>) -> Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                // Debounce: only trigger on actual content changes
                if event.kind.is_modify() {
                    let _ = tx.send(());
                }
            }
        },
        Config::default()
            .with_poll_interval(Duration::from_millis(500))
    )?;

    watcher.watch(parser_path, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
```

**Debounce Logic**: Editors often write multiple times on save. Use 200ms debounce.

---

## 10. Broken Symlink Detection

```rust
fn detect_symlink_status(path: &Path) -> (bool, bool) {
    let is_symlink = path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    let is_broken = if is_symlink {
        !path.exists()  // Symlink exists but target doesn't
    } else {
        false
    };

    (is_symlink, is_broken)
}
```

---

## 11. Smart Sampling

When selecting files for testing, prioritize:

```rust
fn smart_sample(files: &[FileInfo], limit: usize) -> Vec<&FileInfo> {
    let mut result = Vec::new();

    // 1. Previously failed files (50%)
    let failed: Vec<_> = files.iter()
        .filter(|f| f.status == FileStatus::Failed)
        .take(limit / 2)
        .collect();
    result.extend(failed);

    // 2. Never processed (25%)
    let pending: Vec<_> = files.iter()
        .filter(|f| f.status == FileStatus::Pending)
        .take(limit / 4)
        .collect();
    result.extend(pending);

    // 3. Random from processed (25%)
    let processed: Vec<_> = files.iter()
        .filter(|f| f.status == FileStatus::Processed)
        .take(limit / 4)
        .collect();
    result.extend(processed);

    result.truncate(limit);
    result
}
```

---

## 12. Implementation Phases

### Phase 1: Core Structure ✓
- [x] Add `ParserBenchState` to `app.rs`
- [x] Scan `~/.casparian_flow/parsers/` for .py files
- [x] Detect broken symlinks
- [x] Render parser list with states

### Phase 2: Metadata Extraction ✓
- [x] Embed metadata extraction Python script in `app.rs`
- [x] **Batch processing**: Process up to 50 files per subprocess
- [x] Call from Rust via subprocess with stdin/stdout
- [x] Parse JSON response for name/version/topics
- [x] Fallback to filename

### Phase 3: Test Flow with Safety
- [ ] Add `row_limit` parameter to test execution
- [ ] Modify `run_parser_test` to pass `nrows` to Python
- [ ] File picker with smart sampling
- [ ] Results display with truncation indicator

### Phase 4: Focus Mode
- [ ] `z` key toggles right panel fullscreen
- [ ] Hide left panel when focused
- [ ] Preserve keybindings in focus mode

### Phase 5: Watch Mode
- [ ] Integrate `notify` crate
- [ ] Watch selected parser file
- [ ] Debounce file changes (200ms)
- [ ] Auto re-run on change

### Phase 6: AI Integration
- [ ] `[a]` key in result view
- [ ] Build context: parser code + error + data sample
- [ ] Send to Claude Code sidebar
- [ ] Stream response

### Phase 7: Polish
- [ ] Filter/search (`/` key)
- [ ] Help overlay (`?` key)
- [ ] Delete broken symlinks (`d` key)
- [ ] Error suggestions

### Future (v2)
- [ ] Tabbed result view (Preview / Schema / Logs / Raw)
- [ ] Diff view for regression testing
- [ ] Syntax highlighting in error traces

---

## 13. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Parser location | `~/.casparian_flow/parsers/` | Standard, predictable |
| Directory structure | Flat only | Simpler, no nesting |
| Metadata extraction | Embedded Python script | Can't do AST in Rust |
| Metadata batching | 50 files per subprocess | Avoid spawning N processes |
| Row limit default | 1,000 | Prevent OOM, fast feedback |
| Watch mode | Opt-in with `w` | Some users find auto-run annoying |
| Focus mode | `z` key | Standard vim-like toggle |
| AI analyze | Phase 6 | Infrastructure exists (claude_code.rs) |
| Broken symlinks | Show with ✗, allow delete | Clear error state |

---

## 14. Protocol Additions

Add to `crates/casparian_protocol/src/types.rs`:

> **Note**: `GetMetadataRequest`/`ParserMetadata` are NOT needed - metadata extraction
> is handled by embedded Python script in `app.rs` with batch processing (see Section 3.2).

```rust
/// Preview request with row limit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub parser_path: String,
    pub data_path: String,
    pub row_limit: usize,
}

/// Preview response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResult {
    pub success: bool,
    pub rows_processed: usize,
    pub execution_time_ms: u64,
    pub schema: Vec<SchemaColumn>,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub errors: Vec<String>,
    pub error_type: Option<String>,
    pub suggestions: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaColumn {
    pub name: String,
    pub dtype: String,
}
```

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.2 | Added formal state machine (Section 4) per spec refinement v2.3 |
| 2026-01-07 | 0.1 | Initial draft |
| 2026-01-07 | 0.2 | Quick test flow, smart sampling |
| 2026-01-08 | 0.3 | Plugins directory approach |
| 2026-01-08 | 0.4 | Critical fixes from review: row limits, Python AST extraction, broken symlinks, watch mode, focus mode, AI analyze |
| 2026-01-08 | 0.5 | Phase 1 & 2 implemented; batch metadata extraction (50 files/subprocess); removed GetMetadataRequest protocol types |

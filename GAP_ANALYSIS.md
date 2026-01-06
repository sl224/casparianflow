# Gap Analysis: Current State vs Product North Star

**Generated:** January 2025
**Purpose:** Detailed technical analysis of gaps between current implementation and the vision in `PRODUCT_NORTH_STAR.md`

---

## Executive Summary

The codebase has solid **infrastructure** (Bridge Mode execution, Scout file discovery, Parser Lab basic structure) but is missing the **AI-powered refinement loop** and **output-focused approval UX** that are central to the North Star vision.

**Bottom Line:**
- Infrastructure: 70% complete
- Core UX (output preview, error display): 20% complete
- AI Integration: 5% complete (stub only)
- Approval/Publishing flow: 40% complete

---

## Table of Contents

1. [Parser Lab UI Gaps](#1-parser-lab-ui-gaps)
2. [Tauri Backend Gaps](#2-tauri-backend-gaps)
3. [Scout Discovery Gaps](#3-scout-discovery-gaps)
4. [Bridge Mode Gaps](#4-bridge-mode-gaps)
5. [Scout → Sentinel Integration Gaps](#5-scout--sentinel-integration-gaps)
6. [Plugin Publishing Gaps](#6-plugin-publishing-gaps)
7. [Subtle Issues at Component Boundaries](#7-subtle-issues-at-component-boundaries)
8. [Critical Path: Minimum Viable Changes](#8-critical-path-minimum-viable-changes)
9. [Implementation Order](#9-recommended-implementation-order)
10. [Summary Table](#10-summary-table)

---

## 1. Parser Lab UI Gaps

**Location:** `ui/src/lib/components/parser-lab/`

### Current State

| File | Purpose | Status |
|------|---------|--------|
| `ParserLabTab.svelte` | Parser list + load sample | Working |
| `FileEditor.svelte` | Code editor + test file selection | Working |
| `ParserChat.svelte` | Chat interface for AI | **Stub only** |
| `ParserEditor.svelte` | Monaco-based code editor | Working |
| `SinkConfig.svelte` | Output configuration | Working |

### Gap 1.1: No Output Preview (CRITICAL)

**North Star Requirement:**
```
┌─────────────────────────────────────────────────────────────────┐
│ RAW INPUT                              (first 10 rows)          │
├─────────────────────────────────────────────────────────────────┤
│ txn_id,amount,date,customer                                     │
│ TXN-001,$127.50,01/15/2024,Alice                                │
│ TXN-002,$89.99,01/15/2024,Bob                                   │
│ TXN-003,N/A,01/16/2024,Charlie                                  │
└─────────────────────────────────────────────────────────────────┘

                    [Generate Parser]

┌─────────────────────────────────────────────────────────────────┐
│ PARSED OUTPUT                                                   │
├──────────┬──────────┬─────────────┬───────────┬─────────────────┤
│ txn_id   │ amount   │ date        │ customer  │ _errors         │
│ (string) │ (float)  │ (date)      │ (string)  │                 │
├──────────┼──────────┼─────────────┼───────────┼─────────────────┤
│ TXN-001  │ 127.50   │ 2024-01-15  │ Alice     │                 │
│ TXN-002  │ 89.99    │ 2024-01-15  │ Bob       │                 │
│ TXN-003  │ NULL     │ 2024-01-16  │ Charlie   │ amt: 'N/A'      │
└──────────┴──────────┴─────────────┴───────────┴─────────────────┘
```

**Current State:**

`FileEditor.svelte` shows validation output as a raw JSON/text dump in a monospace panel. There is no:
- Side-by-side input/output comparison
- Table rendering of parsed data
- Error highlighting in context
- Column type indicators

**Missing Components:**
1. `RawFilePreview.svelte` - Show first N rows of input file
2. `ParsedOutputTable.svelte` - Render parsed data as table with types
3. `ErrorHighlighter.svelte` - Show which rows/cells failed

**Severity:** CRITICAL - This is the core "show, don't tell" principle from North Star.

---

### Gap 1.2: ParserChat is Non-Functional (CRITICAL)

**Current Code** (`ParserChat.svelte:54-78`):
```svelte
async function handleSend() {
  if (!inputMessage.trim() || isLoading) return;
  const userMsg = inputMessage.trim();
  inputMessage = "";
  messages.push({ role: "user", content: userMsg });
  isLoading = true;

  try {
    const response = await invoke<string>("parser_lab_chat", {
      parserId,
      message: userMsg,
    });
    messages.push({ role: "assistant", content: response });
  } catch (e) {
    messages.push({ role: "assistant", content: `Error: ${e}` });
  }
}
```

**Problem:** The `parser_lab_chat` Tauri command is not implemented in `scout.rs`. The chat UI exists but does nothing useful.

**North Star Requirement:**
1. AI generates parser code from file sample
2. AI refines based on "what's wrong" feedback
3. Refinement dialog with one-click fixes for common issues

**Missing:**
- `parser_lab_generate_parser` command (AI generates from sample)
- `parser_lab_refine_parser` command (AI refines from feedback)
- LLM integration (Anthropic API, local model, or other)

**Severity:** CRITICAL - This is the core AI value proposition.

---

### Gap 1.3: No Refinement Dialog (HIGH)

**North Star Requirement:**
```
┌─────────────────────────────────────────────────────────────────┐
│ What's wrong?                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ ( ) Column type is wrong                                        │
│     → Click on column header to change type                     │
│                                                                 │
│ ( ) Date format is wrong                                        │
│     → Current: MM/DD/YYYY  Change to: [___________]             │
│                                                                 │
│ ( ) Null values not detected                                    │
│     → Add null values: [N/A, NULL, -, (empty)    ]              │
│                                                                 │
│ ( ) Something else                                              │
│     → Describe: [__________________________________]            │
│                                                                 │
│                         [Regenerate Parser]                     │
└─────────────────────────────────────────────────────────────────┘
```

**Current State:** No refinement UI exists. Users must edit Python code directly.

**Missing:**
- `RefineDialog.svelte` component
- Common fix options (type change, date format, null values)
- Free-form feedback input
- Connection to AI regeneration

**Severity:** HIGH - Users shouldn't need to edit code for common fixes.

---

### Gap 1.4: No Backtest Functionality (HIGH)

**North Star Requirement:**
```
┌─────────────────────────────────────────────────────────────────┐
│ BACKTEST RESULTS                                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ Files: 12 │ Rows: 50,847 │ Time: 2.3s                           │
│                                                                 │
│ ✓ sales_january.csv    - 4,521 rows - 100% success              │
│ ✓ sales_february.csv   - 4,102 rows - 100% success              │
│ ! sales_march.csv      - 4,847 rows - 99.7% (15 errors)         │
│   └─ [View 15 errors]                                           │
│ ✓ sales_april.csv      - 4,201 rows - 100% success              │
│ ...                                                             │
│                                                                 │
│ Summary: 99.97% success rate (15 failures in 50,847 rows)       │
│                                                                 │
│ [View All Errors]       [Publish Plugin]      [Fix Errors First]│
└─────────────────────────────────────────────────────────────────┘
```

**Current State:**

`parser_lab_validate_parser` runs against ONE test file at a time. There is no:
- "Test on All Files" button
- Multi-file execution
- Per-file success/failure breakdown
- Aggregate statistics
- Error pattern detection

**Missing:**
- `parser_lab_backtest` Tauri command
- `BacktestResults.svelte` component
- Query to find Scout files matching parser pattern
- Parallel execution infrastructure

**Severity:** HIGH - Users must know parser works at scale before publishing.

---

## 2. Tauri Backend Gaps

**Location:** `ui/src-tauri/src/scout.rs`

### Current Commands

| Command | Purpose | Status |
|---------|---------|--------|
| `parser_lab_create_parser` | Create new parser | Working |
| `parser_lab_get_parser` | Get parser by ID | Working |
| `parser_lab_update_parser` | Update parser code/config | Working |
| `parser_lab_validate_parser` | Run parser on test file | Working |
| `parser_lab_load_sample` | Load bundled sample | Working |
| `parser_lab_import_plugin` | Import existing .py | Working |
| `deploy_plugin` | Publish parser as plugin | Working |

### Gap 2.1: No AI Generation Command (CRITICAL)

**Missing Command:**
```rust
#[tauri::command]
pub async fn parser_lab_generate_parser(
    state: State<'_, ScoutState>,
    file_path: String,
    user_requirements: Option<String>,
) -> Result<GeneratedParser, String> {
    // 1. Read file sample (first N rows)
    // 2. Call LLM with sample + requirements
    // 3. Return generated Python code
}
```

**Current Workaround:** Users must write parser code manually or import existing files.

**Severity:** CRITICAL - This is the core AI feature.

---

### Gap 2.2: Validation Output Lacks Structure (HIGH)

**Current Implementation** (`scout.rs` validation):
```rust
let output = state.env_manager.lock().await.run_code(
    &source_code,
    "validate",
    &test_file_path,
)?;

// Returns raw stdout/stderr
parser.validation_output = Some(output);
```

**Problem:** Returns unstructured text. The UI cannot:
- Render data as a table
- Show which rows had errors
- Display column types

**North Star Requires:**
```rust
pub struct ValidationResult {
    pub rows: Vec<HashMap<String, Value>>,  // Parsed data
    pub errors: Vec<RowError>,               // Per-row errors
    pub schema: Schema,                      // Inferred types
    pub stats: ValidationStats,              // Row count, success rate
}

pub struct RowError {
    pub row_number: usize,
    pub column: String,
    pub raw_value: String,
    pub error: String,
    pub suggestion: Option<String>,
}
```

**Severity:** HIGH - Foundation for output preview UI.

---

### Gap 2.3: No Backtest Command (HIGH)

**Missing Command:**
```rust
#[tauri::command]
pub async fn parser_lab_backtest(
    state: State<'_, ScoutState>,
    parser_id: String,
    source_id: Option<String>,  // Scout source to test against
) -> Result<BacktestResult, String> {
    // 1. Get parser and its file_pattern
    // 2. Find matching files from Scout (or parser's test files)
    // 3. Run parser against each file
    // 4. Aggregate results
}

pub struct BacktestResult {
    pub total_files: usize,
    pub total_rows: usize,
    pub success_rate: f64,
    pub duration_ms: u64,
    pub per_file: Vec<FileResult>,
}

pub struct FileResult {
    pub file_path: String,
    pub row_count: usize,
    pub error_count: usize,
    pub errors: Vec<RowError>,
}
```

**Severity:** HIGH - Critical for trust before publishing.

---

### Gap 2.4: No Chat/Refinement Command (HIGH)

**Missing Commands:**
```rust
#[tauri::command]
pub async fn parser_lab_chat(
    state: State<'_, ScoutState>,
    parser_id: String,
    message: String,
) -> Result<String, String> {
    // 1. Get parser context (code, file sample, validation results)
    // 2. Send to LLM with conversation history
    // 3. Return response (possibly with code updates)
}

#[tauri::command]
pub async fn parser_lab_apply_fix(
    state: State<'_, ScoutState>,
    parser_id: String,
    fix_type: FixType,  // ChangeColumnType, AddNullValue, ChangeDateFormat, etc.
    params: HashMap<String, String>,
) -> Result<ParserLabParser, String> {
    // Apply structured fix to parser code
}
```

**Severity:** HIGH - Required for refinement loop.

---

## 3. Scout Discovery Gaps

**Location:** `crates/casparian_scout/`

### Current State

| Feature | Status |
|---------|--------|
| File scanning (readdir) | Working |
| Tagging rules (glob patterns) | Working |
| Tag coverage analysis | Working |
| Pattern preview (live) | Working |
| File status flow | Working |

### Gap 3.1: No Schema Similarity Clustering (LOW)

**North Star (Phase 6):**
```yaml
clusters:
  - name: "Monthly Sales CSVs"
    pattern: "sales/YYYY-MM-sales.csv"
    count: 12
    schema_confidence: 0.95
    value_score: 0.92
```

**Current:** Scout discovers files but doesn't analyze content. No:
- Schema fingerprinting
- Similarity detection
- Automatic pattern suggestion
- Value scoring

**Severity:** LOW - This is Phase 6 (Discovery Intelligence) in the roadmap. Not blocking core functionality.

---

### Gap 3.2: No Scout Files → Parser Lab Bridge (MEDIUM)

**Problem:** Parser Lab has its own `parser_lab_test_files` table. Scout has `scout_files`. There's no easy way to:
1. Get Scout files matching a parser's pattern
2. Use Scout-discovered files as test data for Parser Lab
3. Backtest against Scout files

**Missing Query/Command:**
```rust
#[tauri::command]
pub async fn get_scout_files_for_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<Vec<ScoutFile>, String> {
    // 1. Get parser's file_pattern
    // 2. Find Scout files matching that pattern (or tag)
    // 3. Return for backtest or test file selection
}
```

**Severity:** MEDIUM - Required for backtest feature.

---

## 4. Bridge Mode Gaps

**Location:** `crates/casparian_worker/src/bridge.rs`

### Current State

| Feature | Status |
|---------|--------|
| Unix socket IPC | Working |
| Python subprocess spawning via `uv` | Working |
| Arrow IPC streaming | Working |
| Sideband logging | Working |
| Error capture | Working |
| Timeout handling | Working |

### Gap 4.1: No Structured Error Output (MEDIUM)

**Current Output:**
```rust
pub struct BridgeResult {
    pub batches: Vec<RecordBatch>,
    pub logs: String,  // Raw text
}
```

**North Star Requires:**
```rust
pub struct BridgeResult {
    pub batches: Vec<RecordBatch>,
    pub logs: String,
    pub row_errors: Vec<RowError>,  // MISSING
    pub schema: Schema,             // MISSING
    pub metrics: ExecutionMetrics,  // MISSING
}

pub struct RowError {
    pub row_number: usize,
    pub column: String,
    pub raw_value: String,
    pub expected_type: String,
    pub error_message: String,
}
```

**Impact:** Without structured errors, UI cannot show "Row 47: 'N/A' couldn't be parsed as float".

**Severity:** MEDIUM - Important for error drill-down UI.

---

### Gap 4.2: Bridge Shim Doesn't Capture Row Errors (MEDIUM)

**Current** (`bridge_shim.py`):
```python
def execute_plugin(source_code, file_path, ...):
    # Executes plugin, captures output
    # Does NOT track which rows failed or why

    return {
        "rows_published": context.get_row_count(),
        "status": "SUCCESS",
        "output_info": output_info,
    }
```

**Missing:** Error tracking at the row level during parsing.

**Potential Fix:** Modify parser contract to optionally emit error records:
```python
def parse(file_path) -> list[Output]:
    # Parser can emit an _errors Output with row-level issues
    return [
        Output(name="data", data=good_rows, sink="parquet"),
        Output(name="_errors", data=error_rows, sink="parquet"),
    ]
```

**Severity:** MEDIUM - Enables error drill-down.

---

## 5. Scout → Sentinel Integration Gaps

**Location:** `ui/src/lib/stores/scout.svelte.ts`

### Current State

```typescript
async submitTaggedFiles(fileIds: number[]): Promise<SubmitResult> {
  const result = await invoke<SubmitResult>("submit_tagged_files", { fileIds });
  // Spawn worker processes for each job
  for (const [, jobId] of result.jobIds) {
    await invoke("process_job_async", { jobId });
  }
  return result;
}
```

Jobs are created and spawned, but:
- No real-time status updates in UI
- Jobs tab likely shows stale data
- No link from Scout file → Job → Output

### Gap 5.1: No Real-Time Job Status (MEDIUM)

**North Star:** Jobs tab should show live progress as files are processed.

**Current:** Jobs are spawned but UI doesn't poll for updates. Status shown is whatever was in DB at page load.

**Missing:**
- Tauri event for job status changes
- Polling or WebSocket for status updates
- Progress indicator during processing

**Severity:** MEDIUM - Important for UX but not blocking core workflow.

---

### Gap 5.2: No Output Linking (MEDIUM)

**Problem:** After a job completes, user has no easy way to:
1. See where the output went
2. Preview the output data
3. Query the output

**Missing:**
- `job.output_path` displayed in UI
- "Open in Finder" / "Preview" buttons
- Link to output in SQLite/Parquet viewer

**Severity:** MEDIUM - Completes the end-to-end story.

---

## 6. Plugin Publishing Gaps

**Location:** `ui/src-tauri/src/scout.rs` (`deploy_plugin`)

### Current State

```rust
pub async fn deploy_plugin(
    state: State<'_, ScoutState>,
    parser_id: String,
    subscription_tags: Vec<String>,
) -> Result<i64, String> {
    // 1. Get parser from parser_lab_parsers
    // 2. Insert into cf_plugin_manifest with status='ACTIVE'
    // 3. Insert tag subscriptions into cf_topic_config
    // Return manifest_id
}
```

This works but lacks:
- Pre-publish validation
- User confirmation UI
- Backtest requirement
- Version tracking

### Gap 6.1: No Publishing Wizard UI (MEDIUM)

**North Star:**
```
┌─────────────────────────────────────────────────────────────────┐
│ PUBLISH PARSER                                                  │
├─────────────────────────────────────────────────────────────────┤
│ Parser: sales_csv_parser                                        │
│ Version: 1.0.0                                                  │
│                                                                 │
│ Subscribe to tags:                                              │
│ [x] sales_data (47 files, 12.3 MB)                              │
│ [ ] csv_data (120 files, 45.1 MB)                               │
│                                                                 │
│ Backtest: 47/47 files passed (100%)                             │
│                                                                 │
│                     [Cancel]  [Publish]                         │
└─────────────────────────────────────────────────────────────────┘
```

**Current:** Deploy is called programmatically without wizard. User doesn't see:
- What tags the plugin will listen to
- How many files match those tags
- Backtest results before publishing

**Missing:**
- `PublishWizard.svelte` component
- Tag selection with file counts
- Backtest status check
- Version input

**Severity:** MEDIUM - Important for confidence before deploy.

---

### Gap 6.2: No Approval Tracking in Database (MEDIUM)

**Current Schema** (`parser_lab_parsers`):
```sql
validation_status TEXT DEFAULT 'pending',  -- 'pending', 'valid', 'invalid'
```

**North Star Requires:**
```sql
-- Parser approval state
approval_status TEXT DEFAULT 'pending',  -- 'pending', 'approved', 'rejected'
approved_by TEXT,
approved_at TEXT,
auto_approved BOOLEAN DEFAULT FALSE,

-- Plugin audit trail
cf_plugin_manifest:
  approved_by TEXT,
  approved_at TEXT,
  approval_method TEXT,  -- 'manual', 'auto', 'cli'
```

**Impact:** No audit trail of who approved what, when.

**Severity:** MEDIUM - Required for compliance and rollback.

---

### Gap 6.3: No Auto-Approve (Red Button) Infrastructure (LOW)

**North Star Phase 5:**
- Settings toggle to enable auto-approve
- Requires confirmation phrase
- Logged and auditable

**Current:** No auto-approve capability. Every parser requires manual validation.

**Missing:**
- `auto_approve_enabled` user setting
- Confidence threshold logic
- Audit logging for auto-approvals

**Severity:** LOW - Phase 5 feature, not blocking MVP.

---

## 7. Subtle Issues at Component Boundaries

### Issue 7.1: Type Mismatch in Validation Output

**Problem:**
- `scout.rs` returns `validation_output: Option<String>` (raw stdout)
- UI expects structured data to render tables
- The mismatch means validation "works" but output is unusable for good UX

**Fix Required:** Return structured JSON instead of raw text.

---

### Issue 7.2: Scout Files Inaccessible from Parser Lab

**Problem:**
- Parser Lab has `parser_lab_test_files` table
- Scout has `scout_files` table
- No query to "get Scout files matching this parser's pattern"

**Impact:** Cannot backtest against real discovered files.

**Fix Required:** Add query or join capability.

---

### Issue 7.3: Sink Config Not Connected to Output

**Problem:**
- `SinkConfig.svelte` lets users pick Parquet/CSV/SQLite
- Parser stores `sink_type` and `sink_config_json`
- But `bridge_shim.py` and `main.rs` don't read these values
- Output always goes to hardcoded Parquet path

**Code Path:**
```rust
// main.rs:893 - hardcoded parquet output
let output_path = output_dir.join(format!("{}_{}.parquet", plugin_name, job_id));
```

**Fix Required:** Pass sink config through to output writer.

---

### Issue 7.4: File Pattern Mismatch Between Parser Lab and Scout

**Problem:**
- Parser Lab `file_pattern` is meant to match Scout tags
- But no validation that they actually match
- User could create parser with `file_pattern: "invoices"` when no Scout files have that tag

**Fix Required:** Validate pattern against existing tags, or show warning.

---

### Issue 7.5: Test File Path Resolution

**Problem:**
- `parser_lab_test_files` stores absolute paths
- If user moves file or runs on different machine, path breaks
- No fallback or error handling

**Fix Required:** Store relative paths or handle missing files gracefully.

---

## 8. Critical Path: Minimum Viable Changes

### Priority 1: Structured Validation Output

**Why First:** Foundation for all UI improvements.

**Files to Modify:**
- `ui/src-tauri/src/scout.rs` - Change validation to return JSON
- `crates/casparian_worker/shim/bridge_shim.py` - Capture structured output

**Changes:**
```rust
// New return type
pub struct ValidationResult {
    pub success: bool,
    pub row_count: usize,
    pub error_count: usize,
    pub sample_rows: Vec<HashMap<String, serde_json::Value>>,
    pub errors: Vec<RowError>,
    pub schema: HashMap<String, String>,  // column -> type
}
```

**Estimated Effort:** 100-200 lines Rust, 50-100 lines Python

---

### Priority 2: Output Preview UI

**Why Second:** Shows users the value immediately.

**Files to Modify:**
- `ui/src/lib/components/parser-lab/FileEditor.svelte` - Add preview panes

**New Components:**
- `RawPreview.svelte` - Show first N rows of input
- `ParsedPreview.svelte` - Render parsed output as table
- `ErrorList.svelte` - Show errors with row context

**Estimated Effort:** 200-300 lines Svelte

---

### Priority 3: Multi-File Backtest

**Why Third:** Critical for trust before publishing.

**Files to Modify:**
- `ui/src-tauri/src/scout.rs` - Add `parser_lab_backtest` command
- `ui/src/lib/components/parser-lab/FileEditor.svelte` - Add backtest UI

**New Components:**
- `BacktestResults.svelte` - Per-file breakdown

**Estimated Effort:** 300-400 lines total

---

### Priority 4: AI Parser Generation

**Why Fourth:** Core differentiator, but needs foundation first.

**Files to Modify:**
- `ui/src-tauri/src/scout.rs` - Add `parser_lab_generate_parser` command
- `ui/src/lib/components/parser-lab/ParserChat.svelte` - Connect to backend

**New Infrastructure:**
- LLM client (Anthropic API or other)
- Prompt templates for parser generation
- Response parsing

**Estimated Effort:** 500+ lines (depends on LLM integration)

---

### Priority 5: Refinement Loop

**Why Fifth:** Builds on AI generation.

**Files to Modify:**
- `ui/src/lib/components/parser-lab/` - Add `RefineDialog.svelte`
- `ui/src-tauri/src/scout.rs` - Add refinement commands

**Changes:**
- Common fix options (one-click)
- Free-form feedback
- AI regeneration from feedback

**Estimated Effort:** 300-400 lines

---

### Priority 6: Publishing Wizard

**Why Sixth:** Confidence before deploy.

**Files to Modify:**
- `ui/src/lib/components/parser-lab/` - Add `PublishWizard.svelte`

**Changes:**
- Tag selection with file counts
- Backtest status requirement
- Version input
- Confirmation dialog

**Estimated Effort:** 200-300 lines

---

## 9. Recommended Implementation Order

```
Week 1-2: Foundation
├── Structured validation output (backend)
├── Output preview UI (frontend)
└── Basic error display

Week 3-4: Scale Testing
├── Multi-file backtest command
├── Backtest results UI
└── Scout → Parser Lab file bridge

Week 5-6: AI Integration
├── LLM client setup
├── Parser generation command
├── ParserChat connection
└── Basic generate → test → approve flow

Week 7-8: Refinement
├── Refinement dialog
├── Common fix options
├── AI refinement from feedback
└── Iteration loop

Week 9-10: Publishing
├── Publishing wizard
├── Approval tracking in DB
├── Audit logging
└── Version management

Future: Advanced
├── Auto-approve (Red Button)
├── Schema drift detection
├── Discovery intelligence
└── Compliance agent
```

---

## 10. Summary Table

| Feature | North Star | Current State | Gap Severity | Priority |
|---------|------------|---------------|--------------|----------|
| Raw file preview | Required | Missing | CRITICAL | P1 |
| Parsed output as table | Required | Raw text only | CRITICAL | P1 |
| Error highlighting | Required | Missing | HIGH | P1 |
| Structured validation | Required | Unstructured | HIGH | P1 |
| AI parser generation | Core feature | Stub only | CRITICAL | P4 |
| Refinement dialog | Required | Missing | HIGH | P5 |
| Multi-file backtest | Required | Missing | HIGH | P3 |
| Per-file breakdown | Required | Missing | MEDIUM | P3 |
| Scout → Parser bridge | Required | Missing | MEDIUM | P3 |
| Approval tracking | Required | Missing | MEDIUM | P6 |
| Publishing wizard | Required | Basic | MEDIUM | P6 |
| Sink config connection | Required | Disconnected | MEDIUM | P2 |
| Auto-approve (Red Button) | Phase 5 | Missing | LOW | Future |
| Schema drift detection | Phase 7 | Missing | LOW | Future |
| Discovery intelligence | Phase 6 | Missing | LOW | Future |

---

## Appendix: File Reference

### Key Files to Modify (by priority)

**Priority 1-2 (Foundation):**
- `ui/src-tauri/src/scout.rs` - Validation output structure
- `ui/src/lib/components/parser-lab/FileEditor.svelte` - Preview UI
- `crates/casparian_worker/shim/bridge_shim.py` - Error capture

**Priority 3 (Backtest):**
- `ui/src-tauri/src/scout.rs` - Backtest command
- `ui/src/lib/components/parser-lab/FileEditor.svelte` - Backtest UI

**Priority 4-5 (AI):**
- `ui/src-tauri/src/scout.rs` - Generation/refinement commands
- `ui/src/lib/components/parser-lab/ParserChat.svelte` - Chat UI
- New: LLM client module

**Priority 6 (Publishing):**
- `ui/src/lib/components/parser-lab/` - PublishWizard.svelte
- `ui/src-tauri/src/scout.rs` - Approval tracking

---

## Changelog

| Date | Change |
|------|--------|
| 2025-01 | Initial gap analysis from codebase review |

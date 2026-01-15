# Rule Builder - TUI Subspec

**Status:** Approved
**Version:** 3.0
**Date:** 2026-01-14
**Parent:** [specs/views/discover.md](views/discover.md) Section 13
**Session Origin:** specs/meta/sessions/ai_consolidation/

---

## 1. Overview

The **Rule Builder** is a unified split-view interface that consolidates pattern exploration, rule creation, and AI-assisted extraction into a single workflow.

### Key Insight

> **Pattern exploration IS rule creation.** They shouldn't be separate activities.

### What Gets Consolidated

| Before (5 concepts) | After (1 concept) |
|---------------------|-------------------|
| Glob Explorer | Rule Builder |
| Rule Creation Dialog | Rule Builder |
| Pathfinder Wizard | Rule Builder (auto-analysis) |
| Semantic Path Wizard | Rule Builder (auto-analysis) |
| Labeling Wizard | Rule Builder (tag suggestions) |

### What Stays Separate

- **Parser Lab**: Generates Python code, fundamentally different output

---

## 2. Custom Glob Extraction Syntax

Casparian extends standard glob with `<field>` placeholders for inline field extraction.

### Syntax

| Syntax | Meaning | Example |
|--------|---------|---------|
| `*` | Match any characters (not `/`) | `*.csv` |
| `**` | Match any path (including `/`) | `**/*.csv` |
| `?` | Match single character | `file?.txt` |
| `{a,b}` | Alternation (standard glob) | `{src,lib}/*.rs` |
| `<field>` | **Capture as field** | `mission_<id>/*.csv` |
| `<field:type>` | **Capture with type hint** | `<date:date>/*.csv` |

### Examples

```
# Capture mission_id from folder name
**/mission_<mission_id>/**/*.csv
         └─────┬─────┘
         captures "042" as mission_id

# Multiple fields
**/mission_<mission_id>/<date>/*.csv
         └─────┬─────┘  └─┬─┘
         captures 42    captures 2024-01-15

# With type hints (for better inference)
**/<client:string>/<year:int>/invoices/*.pdf

# Standard glob still works (no extraction)
**/*.csv
```

### How It Works

1. Parser extracts `<field>` placeholders from pattern
2. Placeholders replaced with `*` for glob matching
3. After match, captured segments extracted by position
4. Type inference runs on captured values

```
Input:  **/mission_<id>/<date>/*.csv
        ↓
Glob:   **/mission_*/*/*.csv
        ↓
Match:  /data/mission_042/2024-01-15/telemetry.csv
        ↓
Extract: id=042, date=2024-01-15
```

### Type Inference

When no type hint is provided, types are inferred from values:

| Values | Inferred Type |
|--------|---------------|
| `042`, `043`, `100` | integer |
| `2024-01-15`, `2024-02-01` | date |
| `abc123`, `CLIENT-A` | string |
| `550e8400-e29b-...` | uuid |

### 2.1 Parsing Algorithm

The custom glob pattern with `<field>` placeholders is parsed in two passes:

#### Pass 1: Placeholder Extraction

```
Input: "**/mission_<mission_id>/<date:date>/*.csv"

Algorithm:
1. Initialize: placeholders = [], glob_pattern = "", pos = 0
2. Scan character by character:
   - If char == '\\' and next char is '<' or '>':
     a. Append next char literally to glob_pattern
     b. Advance pos by 2
     c. Continue (escaped, not a placeholder)
   - If char == '<':
     a. Find matching '>' (track nesting for error)
     b. If no '>' found → ERROR: "Unclosed placeholder at position {pos}"
     c. Extract content between < and >
     d. Parse content as "field_name" or "field_name:type_hint"
     e. Validate field_name: [a-z_][a-z0-9_]*
        - If invalid → ERROR: "Invalid field name '{name}'"
     f. Add to placeholders: { name, type_hint, segment_index }
     g. Append '*' to glob_pattern
   - Else:
     a. Append char to glob_pattern
3. Return: (glob_pattern, placeholders)

Output:
  glob_pattern: "**/mission_*/*/*.csv"
  placeholders: [
    { name: "mission_id", type_hint: None, segment_index: -3 },
    { name: "date", type_hint: Some("date"), segment_index: -2 }
  ]
```

#### Pass 2: Segment Index Calculation (Post-Match)

**Critical:** Segment indices are calculated on the **matched path**, not the pattern.

```
Pattern: **/mission_<id>/<date>/*.csv
Matched: /data/foo/bar/mission_42/2024-01-15/report.csv

Matched path segments: ["", "data", "foo", "bar", "mission_42", "2024-01-15", "report.csv"]
Indices:                  0     1      2      3        4            5             6
Negative:                -7    -6     -5     -4       -3           -2            -1

<id> is in segment containing "mission_" prefix → segment[-3] = "mission_42" → extract "42"
<date> is full segment → segment[-2] = "2024-01-15"
```

#### Error Handling

| Error Condition | Message | Recovery |
|-----------------|---------|----------|
| Unclosed `<` | "Unclosed placeholder at position {pos}" | Highlight position |
| Invalid field name | "Invalid field name: must be lowercase with underscores" | Show valid example |
| Unknown type hint | "Unknown type '{hint}'. Valid: string, int, integer, date, uuid" | Show dropdown |
| Duplicate field name | "Duplicate field name '{name}'" | Highlight both |
| Nested `<` | "Nested placeholders not supported" | Highlight inner `<` |
| Empty field name | "Empty field name in placeholder" | Show example |

#### Edge Cases

| Input | Handling |
|-------|----------|
| `\<notfield\>` | Escaped `<>` treated as literal characters |
| `mission_<id>_<suffix>` | Multiple fields per segment NOT supported - ERROR |
| `<>` | ERROR: Empty field name |
| `<UPPER_CASE>` | ERROR: Uppercase not allowed |
| `int` vs `integer` | Both accepted as integer type hint |

---

## 3. Layout

```
┌ [1] Sources: sales_data ▾   [2] Tags: All ▾ ────────────────────────────────────┐
├─────────────────────────────────────────┬───────────────────────────────────────┤
│ RULE BUILDER                            │ LIVE FILE RESULTS                     │
│                                         │                                       │
│ ┌ PATTERN ─────────────────────────┐    │ 247 files match        [t] Test      │
│ │ **/sales/*/*/orders_*.csv   [🔍] │    │ ─────────────────────────────────────│
│ └──────────────────────────────────┘    │                                       │
│                                         │ ▸ sales/2024/01/orders_001.csv        │
│ ┌ EXCLUDES (1) ────────────────────┐    │   → {year: 2024, month: 1}            │
│ │ **/legacy/**                [x]  │    │                                       │
│ └──────────────────────────────────┘    │   sales/2024/02/orders_002.csv        │
│                                         │   → {year: 2024, month: 2}            │
│ ┌ TAG ─────────────────────────────┐    │                                       │
│ │ sales_orders                [💡] │    │   sales/2023/12/orders_847.csv        │
│ └──────────────────────────────────┘    │   → {year: 2023, month: 12}           │
│                                         │                                       │
│ ┌ EXTRACTIONS (2) ─────────────────┐    │                                       │
│ │ year   segment(-3)  int    [x]   │    │                                       │
│ │ month  segment(-2)  int    [x]   │    │                                       │
│ └──────────────────────────────────┘    │ [j/k] Navigate  [Space] Select        │
│                                         │ [x] Exclude  [i] Ignore folder        │
│ ┌ OPTIONS ─────────────────────────┐    │                                       │
│ │ [x] Enable rule                  │    │                                       │
│ │ [x] Run job on save              │    │                                       │
│ └──────────────────────────────────┘    │                                       │
│                                         │                                       │
│ [Enter] Save  [Tab] AI  [Esc] Close     │                                       │
└─────────────────────────────────────────┴───────────────────────────────────────┘
```

**Split ratio:** 40% left (Rule Builder) / 60% right (File Results)

### Sections

#### PATTERN
- Custom glob pattern input with live filtering (see Section 2)
- `[🔍]` triggers manual re-analysis
- Errors shown inline: `⚠️ Invalid pattern: unclosed bracket`

#### EXCLUDES
- Patterns to exclude from matching
- Added via `[i]` ignore folder or `[x]` exclude file
- Collapsed when empty

#### TAG
- Tag to apply to matched files
- `[💡]` shows suggestions dropdown

#### EXTRACTIONS
- Auto-populated from pattern analysis (both `<field>` syntax and path heuristics)
- Each field shows: name, source, type, sample values
- Toggle with `[x]` checkbox, remove with `x` key

#### OPTIONS
- `[x] Enable rule` - Rule is active
- `[x] Run job on save` - Execute extraction immediately

---

## 4. File Results Panel (Right Side)

The file results panel is **context-aware** and displays different views based on user activity. This adaptive panel supports exploration, extraction preview, and backtest fixing.

### 4.1 Three Phases

| Phase | Trigger | Purpose | Display |
|-------|---------|---------|---------|
| Exploration | Pattern has no `<field>` placeholders | Find where files are | Folder counts + sample filenames |
| Extraction Preview | Pattern has `<field>` placeholders | Verify extractions work | Per-file with extracted values |
| Backtest Results | User presses `t` | Fix failures | Per-file pass/fail with errors |

The panel automatically transitions between phases based on pattern content and user actions.

### 4.2 Phase 1: Exploration (Folder View)

**Condition:** Pattern like `**/orders_*.csv` (no `<field>` syntax)

```
│ 247 files match                                           0.3s     │
│ ───────────────────────────────────────────────────────────────── │
│ ▸ trades/2024/Q1/           (89)  orders_20240115.csv              │
│ ▸ trades/2024/Q2/           (72)  orders_20240401_amended.csv      │
│ ▸ trades/2023/Q4/           (53)  orders_20231201.csv              │
│ ▸ archive/backfill/         (18)  orders_batch_001.csv             │
│ ▸ test/fixtures/            (15)  orders_sample.csv                │
│ ───────────────────────────────────────────────────────────────── │
│ Hint: orders_<date:YYYYMMDD>.csv                                   │
```

**Row format:** `▸ <folder_path>/  (<count>)  <sample_filename>`

- `folder_path`: Deepest 2 levels (e.g., `trades/2024/Q1/`)
- `count`: Files matching in that folder
- `sample_filename`: First match in folder (reveals naming pattern)
- Sorted by count descending
- Hint: footer shows auto-detected extraction pattern

**Expanded folder (Enter on row):**
```
│ ▾ trades/2024/Q1/           (89)                                   │
│   │ orders_20240115.csv                                            │
│   │ orders_20240116.csv                                            │
│   │ orders_20240117_corrected.csv                                  │
│   └ ... 86 more                                                    │
│ ▸ trades/2024/Q2/           (72)  orders_20240401.csv              │
```

**Selected file footer (cursor on specific file):**
```
│ /data/trades/2024/Q1/orders_20240115.csv                           │
│ Suggested: orders_<date:YYYYMMDD>.csv                              │
```

### 4.3 Phase 2: Extraction Preview

**Condition:** Pattern like `**/orders_<date>.csv` (has `<field>` placeholder)

```
│ 247 files match │ Extracting: date                                 │
│ ───────────────────────────────────────────────────────────────── │
│   trades/2024/Q1/orders_20240115.csv                               │
│   → {date: "20240115"}                                             │
│                                                                    │
│   trades/2024/Q1/orders_20240116.csv                               │
│   → {date: "20240116"}                                             │
│                                                                    │
│   trades/2024/Q2/orders_20240401_amended.csv                       │
│   → {date: "20240401_amended"}  ⚠️                                 │
│                                                                    │
│   archive/backfill/orders_batch_001.csv                            │
│   → {date: "batch_001"}  ⚠️                                        │
```

- Shows individual files with extracted values
- ⚠️ indicates value doesn't match type hint (if `<date:date>` used)
- Helps user spot problems before running full backtest

### 4.4 Phase 3: Backtest Results

**Condition:** User pressed `t` to run backtest

```
│ 245/247 passed (2 failed)                [a] All [p] Pass [f] Fail │
│ ───────────────────────────────────────────────────────────────── │
│ ✓ trades/2024/Q1/orders_20240115.csv                               │
│   → {date: "20240115"}                                             │
│                                                                    │
│ ✗ trades/2024/Q2/orders_20240401_amended.csv                       │
│   ERROR: "20240401_amended" is not valid date (YYYYMMDD)           │
│   [x] Exclude file  [i] Ignore folder                              │
│                                                                    │
│ ✗ archive/backfill/orders_batch_001.csv                            │
│   ERROR: "batch_001" is not valid date (YYYYMMDD)                  │
│   [x] Exclude file  [i] Ignore folder                              │
```

- ✓ / ✗ status per file
- Failed files show error message and quick-fix actions
- Filter keys (`a`, `p`, `f`) to focus on subset

### 4.5 Phase Transitions

```
┌─────────────────┐
│   Exploration   │ ◄── Pattern has no <field>
│ (folder counts) │
└────────┬────────┘
         │ User adds <field> to pattern
         ▼
┌─────────────────┐
│   Extraction    │ ◄── Pattern has <field>
│    Preview      │
└────────┬────────┘
         │ User presses 't'
         ▼
┌─────────────────┐
│    Backtest     │
│    Results      │
└────────┬────────┘
         │ User modifies pattern
         ▼
   (back to Exploration or ExtractionPreview)
```

---

## 5. User Workflow

### Create New Rule

```
1. User in Discover, browsing files
   └── Press 'n' or 'r' (open Rule Builder)

2. Screen splits: Rule Builder (40%) | Files (60%)
   └── Pattern field focused
   └── Right panel shows ALL files (scoped by source/tag)

3. User types pattern: **/mission_<mission_id>/<date>/*.csv
   └── Files filter LIVE (debounced 150ms)
   └── Shows "847 files match"
   └── Extractions auto-detected from <field> placeholders

4. User presses 't' (backtest)
   └── Runs extraction against all matched files
   └── Shows pass/fail per file
   └── "845/847 passed (2 failed)"

5. User presses 'f' (filter to failures)
   └── Only failed files shown
   └── Each shows error reason

6. User presses 'i' on failed file (ignore folder)
   └── "**/legacy/**" added to EXCLUDES
   └── File disappears, count updates

7. User presses Enter (save)
   └── Rule saved with pattern, excludes, tag, extractions
   └── Extraction job starts (if option enabled)

8. User presses Esc (close)
   └── Returns to normal Discover view
```

### AI-Assisted Creation

```
1. Press 'n' to open Rule Builder
2. Optionally type initial pattern or browse files
3. Press Ctrl+Space to invoke AI analysis
4. AI analyzes files and populates:
   - Pattern field with detected glob+extraction syntax
   - Tag field with suggested name
   - Extractions section with detected fields
5. User reviews/edits the populated fields (Tab to navigate)
6. Press Enter to save
```

**Key insights:**
- AI fills in Rule Builder fields directly - user sees familiar interface, not YAML
- `Ctrl+Space` can be pressed multiple times to refine suggestions
- User can skip AI entirely and create rules manually

---

## 6. Exclusion System

### Exclusion Keys

| Key | Context | Action |
|-----|---------|--------|
| `x` | On failed file | Exclude this specific file path |
| `i` | On failed file | Ignore folder (`**/folder_name/**`) |
| `I` | On failed file | Ignore parent tree (shows picker) |

### Ignore Folder Logic

When user presses `i` on `/data/sales/legacy/archive/old_format.csv`:

```
1. Extract immediate folder: "archive"
2. Generate exclude pattern: **/archive/**
3. Add to rule's exclude list
4. Re-run pattern match (instant)
5. File disappears from results
6. Show: "45/45 passed (2 excluded)"
```

### Ignore Parent Tree Picker

When user presses `I`, show picker for which level to exclude:

```
┌ Ignore which folder? ─────────────────────────────┐
│                                                   │
│ ► **/archive/**         (1 file affected)         │
│   **/legacy/**          (3 files affected)        │
│   **/sales/legacy/**    (3 files affected)        │
│                                                   │
│ [Enter] Select  [Esc] Cancel                      │
└───────────────────────────────────────────────────┘
```

---

## 7. Backtest Filtering

### Filter Keys

| Key | Action | Status Line |
|-----|--------|-------------|
| `a` | Show all files | `47 files (45 passed, 2 failed)` |
| `p` | Show passes only | `47 files │ Showing: passes (45)` |
| `f` | Show failures only | `47 files │ Showing: failures (2)` |

### Status Line States

```
Pre-backtest:     247 files match
Backtest all:     45/47 passed (2 failed)
Filtered pass:    45/47 passed │ Showing: passes (45)
Filtered fail:    45/47 passed │ Showing: failures (2)
With exclusions:  45/45 passed (2 excluded)
```

---

## 8. State Model

### 8.1 State Machine Diagram

```
RULE BUILDER STATE MACHINE
==========================

Tab Cycle Order: Pattern → Excludes → Tag → Extractions → Options → FileList → (cycle)

LAYER 0: MAIN PANELS (Tab/Shift+Tab cycling)

┌─────────────┐   Tab    ┌─────────────┐   Tab    ┌─────────────┐
│   PATTERN   │ ───────► │   EXCLUDES  │ ───────► │     TAG     │
│  (default)  │          │             │          │             │
└──────┬──────┘ ◄─────── └──────┬──────┘ ◄─────── └──────┬──────┘
       │       Shift+Tab        │       Shift+Tab        │
       │                        │ n/+                    │ Tab
       │ Tab                    ▼                        ▼
       │                 ┌─────────────┐          ┌─────────────┐
       │                 │EXCLUDE_INPUT│          │ EXTRACTIONS │
       │                 │ (text box)  │          │   (list)    │
       │                 └─────────────┘          └──────┬──────┘
       │                   Esc/Enter ↓                   │ Enter
       │                      (back to Excludes)         ▼
       │                                          ┌─────────────┐
       │                                          │EXTRACTION_  │
       │                                          │  EDIT(idx)  │
       │                                          └──────┬──────┘
       │                                                 │ Esc/Enter
       ▼ Shift+Tab from FileList                         ↓
┌─────────────┐   Tab    ┌─────────────┐        (back to Extractions)
│  FILE_LIST  │ ◄─────── │   OPTIONS   │
│(right panel)│          │ (checkboxes)│
└─────────────┘ ───────► └─────────────┘
              Shift+Tab

OVERLAY LAYER (above any state):
┌─────────────────────────────────────────────────────────────────┐
│                     IGNORE_PICKER                               │
│  Entry: Press 'I' on file in FileList                           │
│  Exit:  Enter (select) or Esc (cancel) → FileList               │
└─────────────────────────────────────────────────────────────────┘
```

### 8.2 State Definitions

| State | Description | Entry | Exit |
|-------|-------------|-------|------|
| `Pattern` | Glob pattern input (default) | Initial, Tab from FileList | Tab → Excludes |
| `Excludes` | Exclusion patterns list | Tab from Pattern | Tab → Tag, n/+ → ExcludeInput |
| `ExcludeInput` | Text input for new exclude | n/+ from Excludes | Enter/Esc → Excludes |
| `Tag` | Tag name input | Tab from Excludes | Tab → Extractions |
| `Extractions` | Extraction fields list | Tab from Tag | Tab → Options, Enter → ExtractionEdit |
| `ExtractionEdit(idx)` | Editing specific field | Enter on item | Enter/Esc → Extractions |
| `Options` | Enable/run checkboxes | Tab from Extractions | Tab → FileList |
| `FileList` | Right panel file list | Tab from Options | Tab → Pattern, I → IgnorePicker |
| `IgnorePicker` | Folder ignore dialog | I on file | Enter/Esc → FileList |

### 8.3 Focus Transitions

| From | Key | To | Side Effect |
|------|-----|-----|-------------|
| Pattern | Tab | Excludes | - |
| Pattern | Shift+Tab | FileList | - |
| Excludes | Tab | Tag | - |
| Excludes | n / + | ExcludeInput | Clear input buffer |
| Excludes | x | Excludes | Remove selected |
| ExcludeInput | Enter | Excludes | Add pattern |
| ExcludeInput | Esc | Excludes | Discard |
| Tag | Tab | Extractions | - |
| Extractions | Tab | Options | - |
| Extractions | Enter | ExtractionEdit(selected) | Load field |
| Extractions | Space | Extractions | Toggle enabled |
| Extractions | x | Extractions | Remove field |
| ExtractionEdit(idx) | Enter | Extractions | Save changes |
| ExtractionEdit(idx) | Esc | Extractions | Discard |
| Options | Tab | FileList | Switch to right panel |
| Options | Space | Options | Toggle option |
| FileList | Tab | Pattern | Switch to left panel |
| FileList | i | FileList | Add folder exclude |
| FileList | I | IgnorePicker | Open picker |
| FileList | x | FileList | Exclude file |
| IgnorePicker | Enter | FileList | Add selected pattern |
| IgnorePicker | Esc | FileList | Cancel |
| IgnorePicker | j/k | IgnorePicker | Navigate options |

### 8.4 DiscoverViewState

```rust
pub enum DiscoverViewState {
    // Normal file browsing (full width)
    #[default]
    Files,

    // Rule Builder mode (split view)
    RuleBuilder,

    // Quick overlays (on top of current view)
    SourcesDropdown,
    TagsDropdown,
    Filtering,

    // Full dialogs
    RulesManager,
    SourcesManager,
    SourceEdit,
    SourceDeleteConfirm,

    // Parser Lab (separate, generates code)
    ParserLab(ParserLabPhase),
}
```

### RuleBuilderState

```rust
/// Focus within Rule Builder
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RuleBuilderFocus {
    #[default]
    Pattern,
    Excludes,
    ExcludeInput,
    Tag,
    Extractions,
    ExtractionEdit(usize),
    Options,
    FileList,
    IgnorePicker,
}

/// Which phase the file results panel is in (Section 4)
#[derive(Debug, Clone, Default, PartialEq)]
pub enum FileResultsPhase {
    #[default]
    Exploration,       // Folder counts + samples (no <field> in pattern)
    ExtractionPreview, // Per-file with extracted values (<field> in pattern)
    BacktestResults,   // Per-file pass/fail (after user presses 't')
}

/// Result filter for backtest view
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ResultFilter {
    #[default]
    All,
    PassOnly,
    FailOnly,
}

/// Backtest result for a single file
#[derive(Debug, Clone)]
pub enum FileTestResult {
    NotTested,
    Pass,
    Fail { error: String, hint: Option<String> },
    Excluded { pattern: String },
}

/// A folder with match count and sample (Phase 1: Exploration)
#[derive(Debug, Clone)]
pub struct FolderMatch {
    pub path: String,              // "trades/2024/Q1/"
    pub count: usize,              // 89
    pub sample_filename: String,   // "orders_20240115.csv"
    pub files: Vec<String>,        // Lazily populated on expand
}

/// File with extraction preview (Phase 2: Extraction Preview)
#[derive(Debug, Clone)]
pub struct ExtractionPreviewFile {
    pub path: String,
    pub relative_path: String,
    pub extractions: HashMap<String, String>,  // field_name -> extracted_value
    pub warnings: Vec<String>,                  // Type mismatch warnings
}

/// Full Rule Builder state
#[derive(Debug, Clone, Default)]
pub struct RuleBuilderState {
    // --- Input fields ---
    pub pattern: String,
    pub pattern_error: Option<String>,
    pub excludes: Vec<String>,
    pub exclude_input: String,
    pub tag: String,
    pub tag_suggestions: Vec<(String, f32)>,
    pub extractions: Vec<ExtractionField>,
    pub enabled: bool,
    pub run_job_on_save: bool,

    // --- Analysis state ---
    pub analysis_state: AnalysisState,
    pub hint: String,

    // --- File Results Phase (Section 4) ---
    pub file_results_phase: FileResultsPhase,

    // --- Phase 1: Exploration ---
    pub folder_matches: Vec<FolderMatch>,
    pub expanded_folder_indices: HashSet<usize>,
    pub detected_patterns: Vec<String>,  // ["orders_<date:YYYYMMDD>.csv"]

    // --- Phase 2: Extraction Preview ---
    pub preview_files: Vec<ExtractionPreviewFile>,

    // --- Phase 3: Backtest Results ---
    pub matched_files: Vec<MatchedFile>,
    pub match_count: usize,
    pub visible_files: Vec<usize>,

    // --- Selection & Navigation ---
    pub selected_index: usize,
    pub multi_selected: HashSet<usize>,
    pub extraction_selected: usize,
    pub exclude_selected: usize,

    // --- Backtest state ---
    pub backtest: BacktestSummary,
    pub result_filter: ResultFilter,

    // --- Ignore picker state ---
    pub ignore_options: Vec<(String, usize)>,
    pub ignore_selected: usize,

    // --- UI state ---
    pub focus: RuleBuilderFocus,

    // --- Debouncing ---
    pub pattern_changed_at: Option<Instant>,

    // --- Streaming state ---
    pub is_streaming: bool,
    pub stream_elapsed_ms: u64,

    // --- Editing existing rule ---
    pub editing_rule_id: Option<String>,
}
```

---

## 9. Keybindings

### Discover Mode (Normal)

| Key | Action |
|-----|--------|
| `n` | Open Rule Builder (new rule) |
| `r` | Open Rule Builder (new rule) |
| `R` | Open Rules Manager |
| `W` | Open Parser Lab |
| `1` | Sources dropdown |
| `2` | Tags dropdown |
| `3` | Focus files |

### Rule Builder - Left Panel

| Key | Context | Action |
|-----|---------|--------|
| `Tab` | Any | Next field (see state machine for cycle order) |
| `Shift+Tab` | Any | Previous field |
| `Ctrl+Space` | Any | Invoke AI analysis |
| `Enter` | Pattern/Tag/Options | Save rule |
| `Enter` | Excludes | Edit selected exclude |
| `h` | Any | Add/edit hint for AI |
| `Esc` | Any | Close builder (cancel input if editing, else close) |
| `j`/`k` | Extractions/Excludes | Navigate items |
| `Space` | Extractions | Toggle field enabled |
| `x` | Extractions | Remove field |
| `x` | Excludes | Remove exclude |
| `n` / `+` | Extractions/Excludes | Add new item |

### Rule Builder - Right Panel (Phase-Specific)

#### Phase 1: Exploration (Folder View)

| Key | Action |
|-----|--------|
| `j`/`k` or arrows | Move cursor down/up |
| `Enter` | Toggle folder expand/collapse |
| `Tab` | Focus left panel (rule fields) |

#### Phase 2: Extraction Preview

| Key | Action |
|-----|--------|
| `j`/`k` or arrows | Move cursor down/up |
| `t` | Run full backtest → transition to Phase 3 |
| `Tab` | Focus left panel |

#### Phase 3: Backtest Results

| Key | Action |
|-----|--------|
| `j`/`k` or arrows | Move cursor down/up |
| `a` | Filter: show all |
| `p` | Filter: show passes only |
| `f` | Filter: show failures only |
| `x` | Exclude this file (on failed file) |
| `i` | Ignore folder `**/folder/**` (on failed file) |
| `I` | Ignore parent tree picker (on failed file) |
| `t` | Re-run backtest |
| `Tab` | Focus left panel |

### Ignore Picker

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate options |
| `Enter` | Select folder level to ignore |
| `Esc` | Cancel |

---

## 10. Processing Pipeline

### Pattern Change Flow (Phase-Aware)

```
User types in pattern
        │
        ▼
Debounce (150ms)
        │
        ▼
Check: Does pattern contain '<' and '>'?
        │
        ├── YES → Set phase = ExtractionPreview
        │         Parse <field> placeholders
        │         Convert to standard glob for matching
        │         Query matched files
        │         Run extraction on matched paths
        │         Update preview_files with extracted values
        │         Detect type warnings (⚠️)
        │
        └── NO  → Set phase = Exploration
                  Spawn streaming folder search
                         │
                         ▼
                  ┌─────────────────────────────────┐
                  │ Background Task (spawn_blocking)│
                  │                                 │
                  │ for (folder, files) in cache:   │
                  │   for file in files:            │
                  │     if glob_matches(pattern):   │
                  │       folder_counts[folder]++   │
                  │       if no sample yet:         │
                  │         sample = file           │
                  │                                 │
                  │   if 100ms elapsed:             │
                  │     send streaming update       │
                  │     (sorted by count desc)      │
                  │                                 │
                  │ send final update               │
                  └─────────────────────────────────┘
                         │
                         ▼
                  UI receives updates via channel
                  Merges into folder_matches
                  Detects suggested patterns
                  Re-renders panel

Cancellation: Any keystroke in pattern field cancels current search,
              restarts debounce.

Data source: FolderCache (already in memory from scout). No disk I/O needed.
```

### Backtest Trigger Flow

```
User presses 't' (in Phase 2: Extraction Preview)
        │
        ▼
Set phase = BacktestResults
        │
        ▼
For each matched file:
├── Run extraction using current pattern
├── Validate extracted values against type hints
├── If all fields pass → FileTestResult::Pass
├── If any field fails → FileTestResult::Fail { error, hint }
└── Update backtest.pass_count / fail_count
        │
        ▼
Update matched_files with test results
        │
        ▼
Apply current result_filter
        │
        ▼
Update visible_files indices
```

### Analysis Pipeline (All Local, <50ms)

```
Input: List of matched file paths

Step 1: Path Segmentation (<1ms)
├── Split paths into segments
├── Identify variable vs fixed segments
└── Output: SegmentAnalysis

Step 2: Pattern Detection (<5ms)
├── Detect dates (YYYY-MM-DD, YYYY/MM, etc.)
├── Detect numeric IDs (001, 002, etc.)
├── Detect entity prefixes (CLIENT-, mission_, etc.)
└── Output: DetectedPatterns

Step 3: Semantic Recognition (<10ms)
├── Match against known primitives:
│   ├── dated_hierarchy
│   ├── entity_folder
│   ├── numeric_sequence
│   └── timestamp patterns
├── Calculate confidence score
└── Output: SemanticMatch (confidence 0-100%)

Step 4: Field Generation (<5ms)
├── If confidence >= 80%: Use semantic field names
├── If confidence < 80%: Use detected pattern names
└── Output: Vec<ExtractionField>

Step 5: Sample Extraction (<20ms)
├── Run extraction on first 10 files
├── Populate sample_values for each field
└── Output: Updated ExtractionField with samples

Total: ~40ms (NO LLM for 95% of cases)
```

---

## 11. Design Decisions

### No YAML Shown to Users

**Old:** Pathfinder showed YAML extraction rules that users had to understand.

**New:** AI populates Rule Builder fields directly. Users see:
- `year: segment(-3)` in the extractions list
- Not `extract:\n  year:\n    from: segment(-3)\n    type: integer`

**Rationale:** Users want to create rules, not read YAML. YAML is an escape hatch, not the primary view.

### AI is a Helper, Not a Wizard

**Old:** "Open Pathfinder Wizard" -> separate modal -> AI does its thing -> user returns.

**New:** Press `Tab` in Rule Builder -> AI fills in fields -> user reviews/edits.

**Rationale:** AI should assist the current task, not be a separate destination.

### Local Analysis First, LLM Only When Needed

**Processing pipeline runs in ~40ms with no LLM:**
- Path segmentation
- Pattern detection
- Semantic recognition
- Field generation

**LLM only invoked for:**
- Ambiguous patterns
- User explicitly requests AI assistance (Tab key)
- Parser Lab (always needs LLM to generate Python)

**Rationale:** Instant feedback is better than accurate-but-slow feedback.

---

## 12. Migration Notes

### Removed Concepts

| Concept | Replacement |
|---------|-------------|
| Glob Explorer | Rule Builder |
| Rule Creation Dialog | Rule Builder |
| Wizard Menu | Parser Lab only (W key) |
| Pathfinder Wizard | Auto-analysis + Tab in Rule Builder |
| Semantic Path Wizard | Auto-analysis in Rule Builder |
| Labeling Wizard | Tag suggestions in Rule Builder |

### Keybinding Changes

| Key | Before | After |
|-----|--------|-------|
| `g` | Glob Explorer | Removed (use `n`/`r` for Rule Builder) |
| `W` | Wizard Menu | Parser Lab directly |
| `n` | Rule Creation Dialog | Rule Builder |

---

## 11. Error Handling

### 11.1 Error Categories

| Category | Severity | Blocks UI? | Example |
|----------|----------|------------|---------|
| Pattern Syntax | Warning | No | Unclosed `<`, invalid field name |
| No Matches | Info | No | Valid pattern, 0 files |
| Analysis Failure | Error | No | LLM timeout, rate limit |
| Database Error | Error | Yes (modal) | Connection failed, constraint violation |

### 11.2 Pattern Syntax Errors

Shown inline below pattern field with red border:
```
┌ PATTERN ─────────────────────────────────────────┐
│ **/mission_<id/<date>/*.csv                      │
└──────────────────────────────────────────────────┘
  ⚠️ Unclosed placeholder at position 11
     Hint: Add matching `>` or escape with `\<`
```

### 11.3 AI Analysis Failure

Status line shows error with retry option:
```
⚠️ AI analysis failed: Rate limit exceeded
   [r] Retry  [Esc] Continue manually
```

### 11.4 Database Save Failure

Modal dialog (blocks UI):
```
┌─────────────────────────────────────────────────┐
│  ⛔ Failed to Save Rule                          │
│  Error: Rule name 'sales_data' already exists    │
│  [e] Edit  [o] Overwrite  [Esc] Cancel           │
└──────────────────────────────────────────────────┘
```

---

## 12. Loading States

### 12.1 Pattern Analysis

Match count shows spinner while analyzing:
```
[⠋] Analyzing...  (previous: 247 files match)
```

### 12.2 AI Analysis (Ctrl+Space)

Spinners appear in tag and extractions sections:
```
┌ TAG ─────────────────────────────────────────────┐
│ [⠋] Analyzing...                            [💡] │
└──────────────────────────────────────────────────┘
```

Press `Esc` to cancel.

### 12.3 Backtest

Progress bar with file count:
```
Testing: [████████░░░░░░] 45/247  (18%)
```

Press `Esc` to cancel (keeps partial results).

### 12.4 Rule Save

Three-phase progress:
```
Saving... [Validating] → [Saving] → [Starting job]
```

---

## 13. Scroll Behavior

### 13.1 File List (Right Panel)

- Viewport: 15-25 files depending on terminal height
- Navigation: `j/k` single, `Ctrl+d/u` half-page, `g/G` top/bottom
- Scroll indicators: `^` / `v` at edges when more content exists
- Status: `Showing 1-20 of 247`

### 13.2 Extractions List

- Viewport: 5 items
- Navigation: `j/k`
- Collapsed when empty

### 13.3 Excludes List

- Viewport: 3 items
- Navigation: `j/k`
- Collapsed when empty

---

## 14. Database Persistence

### 14.1 Table Mapping

| Spec Field | Table | Column |
|------------|-------|--------|
| pattern | `extraction_rules` | `glob_pattern` |
| tag | `extraction_rules` | `base_tag` |
| enabled | `extraction_rules` | `enabled` |
| extractions | `extraction_fields` | (multiple rows) |
| excludes | `scout_tagging_rules` | (separate rules, priority=-1) |

### 14.2 Excludes Storage

Excludes are stored as separate tagging rules with:
- `priority = -1` (negative = exclude)
- `tag = NULL` (schema updated to allow NULL for excludes)
- `pattern` = the exclude glob pattern

This allows excludes to be queried and managed independently.

### 14.3 Save Transaction

```sql
BEGIN TRANSACTION;
-- Upsert extraction_rules
INSERT OR REPLACE INTO extraction_rules (...) VALUES (...);
-- Delete old fields, insert new
DELETE FROM extraction_fields WHERE rule_id = ?;
INSERT INTO extraction_fields (...) VALUES (...);
-- Sync excludes
DELETE FROM scout_tagging_rules WHERE rule_id = ? AND priority < 0;
INSERT INTO scout_tagging_rules (...) VALUES (...);
COMMIT;
-- On any error: ROLLBACK
```

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial design (enhanced Rule Dialog) |
| 2026-01-14 | 2.0 | Redesigned as Rule Builder (unified split view) |
| 2026-01-14 | 2.0 | Added exclusion system and backtest filtering |
| 2026-01-14 | 2.0 | Added ignore folder picker |
| 2026-01-14 | 2.0 | Published from session to main specs |
| 2026-01-14 | 2.1 | **Spec Refinement Session:** |
| | | - Added state machine diagram (Section 8.1-8.3) |
| | | - Added parsing algorithm for `<field>` syntax (Section 2.1) |
| | | - Changed AI invocation from Tab to Ctrl+Space |
| | | - Added error handling (Section 11) |
| | | - Added loading states (Section 12) |
| | | - Added scroll behavior (Section 13) |
| | | - Added database persistence (Section 14) |
| 2026-01-14 | 3.0 | **Three-Phase File Results Panel:** |
| | | - Section 4 redesigned with three phases: Exploration, Extraction Preview, Backtest Results |
| | | - Phase 1 (Exploration): Folder counts + sample filenames, sorted by count |
| | | - Phase 2 (Extraction Preview): Per-file with extracted values and warnings |
| | | - Phase 3 (Backtest Results): Per-file pass/fail with error details |
| | | - Added `FileResultsPhase`, `FolderMatch`, `ExtractionPreviewFile` types |
| | | - Added streaming state fields to `RuleBuilderState` |
| | | - Section 9 updated with phase-specific keybindings |
| | | - Section 10 updated with phase-aware pattern change flow |

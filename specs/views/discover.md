# Discover - TUI View Spec

**Status:** Approved for Implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 3.3
**Related:** specs/extraction.md, docs/decisions/ADR-017-tagging-vs-extraction-rules.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Discover** mode is the TUI mode for file organization - scanning directories, tagging files, and previewing contents. It prepares files for processing by parsers in Parser Bench mode.

### 1.1 Design Philosophy

- **Source-first workflow**: Users must select a source before seeing files
- **Dropdown navigation**: Sources and Tags as filterable dropdowns (telescope.nvim style)
- **Tags, not Rules**: Users browse by category (tag), not mechanism (rule)
- **Live preview**: Navigating sources/tags updates file list in real-time
- **Zero friction**: Immediate filter typing, no mode switches required

### 1.2 Core Entities

```
~/.casparian_flow/casparian_flow.sqlite3

Tables:
├── scout_sources        # Directories being watched
├── scout_files          # Discovered files with tags
└── scout_tagging_rules  # Pattern → tag mappings (background mechanism)
```

**Key Distinction:**
- **Tags** = Categories users browse (what files ARE)
- **Rules** = Mechanisms that apply tags (HOW tags get assigned)

Users interact with Tags in the sidebar. Rules are managed separately via Rules Manager (`R`).

---

## 2. User Workflows

### 2.1 Primary Workflow: Browse by Tag

```
1. User enters Discover mode (press 1 from any view)
2. Sources dropdown shows scanned directories
3. User presses 1 to open Sources dropdown
4. User selects a source, files appear
5. User presses 2 to open Tags dropdown
6. Tags show: "All files", "sales (89)", "logs (34)", "untagged (19)"
7. User navigates with ↑/↓ → files filter LIVE as they browse
8. User presses Enter to confirm selection
```

**Live Preview:** While the Tags dropdown is open, the Files panel updates
instantly as you navigate through tags. This lets you preview what files
are in each category before committing to a selection.

### 2.2 Create Tagging Rule (Primary Flow)

The `n` key opens rule creation from anywhere in Discover mode:

```
1. User presses 'n' to create a new rule
2. Dialog opens with two fields: Pattern and Tag
   - Pattern is prefilled based on context (current filter, file extension)
   - Live preview shows files that will match
3. User enters pattern (e.g., "*.csv") and tag (e.g., "sales")
4. Tab switches between fields
5. Enter creates rule, Esc cancels
6. Rule created, matching files tagged
7. "sales" appears in Tags dropdown
```

**Context-aware prefilling:**
- From Files panel with filter active → Pattern prefilled with filter
- From Files panel with file selected → Pattern prefilled with file extension (e.g., `*.csv`)
- From Tags panel with tag selected → Tag field prefilled

### 2.3 First-Time Wizard (Onboarding)

When entering Discover mode with untagged files, a wizard appears:

```
┌─ Quick Setup ─────────────────────────────────────────┐
│   📁 Source: sales_data                               │
│   142 files discovered, 47 untagged                   │
│                                                       │
│   [n] Create a tagging rule                           │
│   [Enter] Browse files first                          │
│                                                       │
│   [ ] Don't show this again    [Space] toggle         │
└───────────────────────────────────────────────────────┘
```

- Shown once per session when source has untagged files
- User can dismiss permanently with checkbox

### 2.4 Manage Rules (Full Control)

```
1. User presses R to open Rules Manager
2. Dialog shows all rules for current source:
   *.csv → sales
   *.log → logs
   invoice_*.* → invoices
3. User can: [n] New, [e] Edit, [d] Delete, [Esc] Close
```

### 2.4 Tag Files Manually

```
1. User focuses Files panel (press 3)
2. User navigates to file
3. User presses 't' to tag single file
4. User presses 'T' to bulk tag filtered files
```

---

## 3. Layout Specification

### 3.1 Three-Panel Design

```
┌────────────────────┬────────────────────────────────────────┬─────────────────┐
│     SIDEBAR        │              FILES                     │    PREVIEW      │
│  (Sources/Tags)    │                                        │   (toggle 'p')  │
├────────────────────┼────────────────────────────────────────┼─────────────────┤
│ ▼ sales_data (142) │  invoices/jan.csv        [sales]  2KB │                 │
│                    │  invoices/feb.csv        [sales]  3KB │  [file content] │
│ ▼ All files (142)  │  reports/q1.xlsx                 15KB │                 │
│                    │  data/orders.json        [api]   8KB  │                 │
│                    │                                        │                 │
├────────────────────┼────────────────────────────────────────┤                 │
│ [s] Scan           │  Filter: _______                       │                 │
│ [R] Rules          │  [t]ag [T]bulk [↑↓]nav [Enter]detail   │                 │
└────────────────────┴────────────────────────────────────────┴─────────────────┘
```

### 3.2 Sidebar: Dropdown Navigation

The sidebar contains two collapsible, filterable dropdowns:

**Collapsed State (default):**
```
┌─ SOURCES [1] ─────────┐
│ ▼ sales_data (142)    │  <- Selected source + file count
└───────────────────────┘
┌─ TAGS [2] ────────────┐
│ ▼ All files (142)     │  <- Selected tag or "All files"
└───────────────────────┘
```

**Tags Dropdown Expanded:**
```
┌─ TAGS [2] ────────────┐
│ Filter: ___           │  <- Type to filter tags
│ ► All files (142)     │  <- Always first option
│   sales (89)          │  <- Tag with count
│   logs (34)           │
│   invoices (12)       │
│   untagged (7)        │  <- Files without tags
└───────────────────────┘
```

**Indicators:**
- `▼` = Collapsed dropdown (press number key to expand)
- `►` = Currently highlighted item
- `(123)` = File count for source/tag

### 3.3 Dynamic Height Allocation

| Sources | Tags | Sidebar Height |
|---------|------|----------------|
| Collapsed | Collapsed | ~6 lines (minimal) |
| Expanded | Collapsed | Expanded takes available space |
| Collapsed | Expanded | Expanded takes available space |
| Expanded | Expanded | 50%/50% split |

### 3.4 Rules Manager Dialog

Opened with `R` key, appears as overlay:

```
┌─ TAGGING RULES ─────────────────────────────────────────────┐
│                                                             │
│  Pattern              Tag          Priority   Enabled       │
│  ─────────────────────────────────────────────────────────  │
│  ► *.csv              sales        100        ✓             │
│    *.log              logs         90         ✓             │
│    invoice_*.*        invoices     80         ✓             │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  [n] New   [e] Edit   [d] Delete   [Enter] Toggle   [Esc]   │
└─────────────────────────────────────────────────────────────┘
```

### 3.5 Sources Manager Dialog

Opened with `M` key, appears as overlay for full CRUD on sources:

```
┌─ SOURCES MANAGER ───────────────────────────────────────────┐
│                                                             │
│  Name                 Path                        Files     │
│  ─────────────────────────────────────────────────────────  │
│  ► sales_data         /data/sales                 142       │
│    mission_logs       /mnt/missions               847       │
│    sensor_archive     /data/sensors               312       │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│  [n] New   [e] Edit name   [d] Delete   [r] Rescan   [Esc]  │
└─────────────────────────────────────────────────────────────┘
```

**Source Edit Dialog** (opened with `e` in Sources Manager):

```
┌─ EDIT SOURCE ───────────────────────────────────────────────┐
│                                                             │
│  Name: sales_data_______                                    │
│  Path: /data/sales (read-only)                              │
│                                                             │
│  [Enter] Save   [Esc] Cancel                                │
└─────────────────────────────────────────────────────────────┘
```

**Delete Confirmation** (opened with `d` in Sources Manager):

```
┌─ DELETE SOURCE ─────────────────────────────────────────────┐
│                                                             │
│  Delete source "sales_data"?                                │
│                                                             │
│  This will remove the source and all 142 tracked files      │
│  from the database. The actual files on disk will NOT       │
│  be deleted.                                                │
│                                                             │
│  [Enter] Confirm delete   [Esc] Cancel                      │
└─────────────────────────────────────────────────────────────┘
```

### 3.6 Preview Panel

- Toggle with `p` key
- Shows file content for selected file
- Supports text files, CSV preview, JSON pretty-print
- Hidden by default to maximize file list space

---

## 4. State Machine

> **Updated:** 2026-01-14 (aligned with codebase implementation)

The Discover mode uses a state machine with 14 view states organized into 5 categories,
plus a GlobExplorer overlay with 6 phases.

### 4.1 High-Level State Architecture

```
DISCOVER VIEW STATE MACHINE
===========================

LAYER 0: BASE
+------------------+
|      FILES       |  <-- Default state, always return here
|    (default)     |
+--------+---------+
         |
    +----+----+----+----+----+
    |    |    |    |    |    |
    v    v    v    v    v    v

LAYER 1: Dropdowns & Inputs        LAYER 2: Dialogs
+----------+ +----------+          +----------+ +----------+
| Sources  | | Tags     |          | Rules    | | Sources  |
| Dropdown | | Dropdown |          | Manager  | | Manager  |
+----------+ +----------+          +----------+ +----------+

+----------+ +----------+          OVERLAY (independent of view state):
| Filtering| | Entering |          +---------------------------+
|          | | Path     |          | GlobExplorer (optional)   |
+----------+ +----------+          | Phases: Browse, Filtering,|
                                   | EditRule, Testing,        |
+----------+ +----------+          | Publishing, Published     |
| Tagging  | | Creating |          +---------------------------+
|          | | Source   |
+----------+ +----------+

LAYER KEY:
  Layer 0: Base state (Files)
  Layer 1: Overlays - Esc returns directly to Files
  Layer 2: Dialogs - Esc uses previous_view_state
  Overlay:  GlobExplorer can be active alongside any view state
```

### 4.2 State Categories

**Category 1: DEFAULT**
- `Files` - Default state, navigate files. Entry: Default, Esc from overlays.

**Category 2: DROPDOWN MENUS (Layer 1)**
- `SourcesDropdown` - Entry: Press 1. Exit: Enter/Esc -> Files
- `TagsDropdown` - Entry: Press 2. Exit: Enter/Esc -> Files

**Category 3: MODAL INPUT OVERLAYS (Layer 1)**
- `Filtering` - Entry: Press /. Exit: Esc -> Files
- `EnteringPath` - Entry: Press s. Exit: Esc/Enter -> Files/Scanning
- `Tagging` - Entry: Press t. Exit: Esc/Enter -> Files
- `CreatingSource` - Entry: After path entered. Exit: Esc/Enter -> Files
- `BulkTagging` - Entry: Press T. Exit: Esc/Enter -> Files

**Category 4: FULL DIALOGS (Layer 2)**
- `RulesManager` - Entry: Press R. Exit: Esc -> previous_view_state
- `RuleCreation` - Entry: Press n, or Enter in RulesManager. Exit: Esc/Enter -> RulesManager
- `SourcesManager` - Entry: Press M. Exit: Esc -> previous_view_state
- `SourceEdit` - Entry: Press e in SourcesManager. Exit: Esc/Enter -> SourcesManager
- `SourceDeleteConfirm` - Entry: Press d in SourcesManager. Exit: Esc/Enter -> SourcesManager

**Category 5: BACKGROUND STATES (Layer 1)**
- `Scanning` - Entry: Auto after scan starts. Exit: Auto on complete -> Files

### 4.3 GlobExplorer Overlay

GlobExplorer is implemented as an **optional field** (`glob_explorer: Option<GlobExplorerState>`)
rather than a view state variant. This allows it to overlay the Files state.

The phase is tracked in `GlobExplorerState.phase` (see `GlobExplorerPhase` enum).

```
GLOB EXPLORER (Nested Sub-Machine)
==================================
Entry: Press 'g' from Files state
Exit:  Press 'g' or 'Esc' from Browse phase

NAVIGATION LAYER:
+----------------+     l/Enter      +----------------+
|     BROWSE     |----------------->|     BROWSE     |
|    (at root)   |                  |  (in subfolder)|
+-------+--------+<-----------------+-------+--------+
        |           h/Backspace             |
        | / (start typing)                  | /
        v                                   v
+----------------+                  +----------------+
|   FILTERING    |                  |   FILTERING    |
|   (heat map)   |<---------------->|  (in subfolder)|
+-------+--------+     l/h          +-------+--------+
        |
        | e (when matches > 0)
        v
RULE EDITING LAYER:
+----------------+
|   EDIT_RULE    |   4-section editor: Glob|Fields|Tag|Conditions
| (Tab to cycle) |   j/k to navigate within sections
+-------+--------+
        |
        +-- t --> Testing
        +-- Esc --> Browse (cancel)

+----------------+     +----------------+     +----------------+
|    TESTING     | --> |   PUBLISHING   | --> |   PUBLISHED    |
| (auto-progress)|     | (Enter/Esc)    |     | (job_id: xxx)  |
+----------------+     +----------------+     +----------------+
```

### 4.4 State Definitions Table

| # | State | Category | Entry | Exit | Returns To |
|---|-------|----------|-------|------|------------|
| 1 | `Files` | Default | Default, Esc from overlays | 1,2,/,s,t,T,n,R,M,g | - |
| 2 | `SourcesDropdown` | Dropdown | Press 1 | Enter, Esc | Files |
| 3 | `TagsDropdown` | Dropdown | Press 2 | Enter, Esc | Files |
| 4 | `Filtering` | Input | Press / | Esc | Files |
| 5 | `EnteringPath` | Input | Press s | Esc, Enter | Files (or Scanning) |
| 6 | `Tagging` | Input | Press t | Esc, Enter | Files |
| 7 | `CreatingSource` | Input | After path entered | Esc, Enter | Files |
| 8 | `BulkTagging` | Input | Press T | Esc, Enter | Files |
| 9 | `RulesManager` | Dialog | Press R | Esc | previous_view_state |
| 10 | `RuleCreation` | Dialog | Press n, Enter in RulesManager | Esc, Enter | previous_view_state |
| 11 | `SourcesManager` | Dialog | Press M | Esc | previous_view_state |
| 12 | `SourceEdit` | Dialog | Press e in SourcesManager | Esc, Enter | SourcesManager |
| 13 | `SourceDeleteConfirm` | Dialog | Press d in SourcesManager | Esc, Enter | SourcesManager |
| 14 | `Scanning` | Background | Auto (scan starts) | Auto (scan ends) | Files |

**GlobExplorer Overlay** (not a DiscoverViewState variant - tracked via `glob_explorer: Option<GlobExplorerState>`):

| Overlay | Entry | Exit | Returns To |
|---------|-------|------|------------|
| GlobExplorer | Press g from Files | g/Esc from Browse | Files (clears overlay) |

### 4.5 GlobExplorer Phase Definitions

| Phase | Entry | Exit | Preserves |
|-------|-------|------|-----------|
| `Browse` | Default, Esc from Filtering | `/` -> Filtering, `e` -> EditRule, `g`/Esc -> exit | prefix |
| `Filtering` | `/` from Browse | Esc -> Browse, `e` -> EditRule | prefix, pattern |
| `EditRule` | `e` (with matches) | `t` -> Testing, Esc -> Browse | prefix, pattern, draft |
| `Testing` | `t` from EditRule | `p` -> Publishing, `e`/Esc -> EditRule | draft |
| `Publishing` | `p` from Testing | Enter -> Published, Esc -> EditRule | draft |
| `Published` | Auto from Publishing | Enter/Esc -> Browse, `j` -> Jobs | None |

### 4.6 Transition Functions

**Primary transition function** (saves previous_view_state):
```rust
fn transition_discover_state(&mut self, new_state: DiscoverViewState) {
    self.discover.previous_view_state = Some(self.discover.view_state.clone());
    self.discover.view_state = new_state;
}
```

**Return to previous** (for Esc from dialogs):
```rust
fn return_to_previous_discover_state(&mut self) {
    if let Some(prev) = self.discover.previous_view_state.take() {
        self.discover.view_state = prev;
    } else {
        self.discover.view_state = DiscoverViewState::Files;
    }
}
```

**GlobExplorer phase transition** (modifies optional field, does NOT save previous_view_state):
```rust
fn transition_glob_explorer_phase(&mut self, new_phase: GlobExplorerPhase) {
    if let Some(ref mut explorer) = self.discover.glob_explorer {
        explorer.phase = new_phase;
    }
}
```

### 4.7 Esc Behavior Summary

| Context | Esc Behavior |
|---------|--------------|
| Files (base state) | No effect |
| Dropdowns | Close dropdown, return to Files |
| Input overlays | Cancel input, return to Files |
| Scanning | Cancel scan, return to Files |
| Dialogs (RulesManager, SourcesManager) | Return to previous_view_state |
| Nested dialogs (SourceEdit, RuleCreation) | Return to parent dialog |
| GlobExplorer Browse | Exit GlobExplorer entirely (set to None) |
| GlobExplorer Filtering | Clear pattern, return to Browse |
| GlobExplorer EditRule | Cancel, return to Browse |
| GlobExplorer Testing/Publishing | Cancel, return to EditRule |
| GlobExplorer Published | Return to Browse (root) |

### 4.8 Preview vs Selection

Dropdowns have **two-stage selection**:
1. **Preview** (during navigation): Files update as you move
2. **Selection** (on Enter): Dropdown closes, becomes the active choice

---

## 5. Data Model

> **Updated:** 2026-01-14 (aligned with codebase implementation)

### 5.1 State Machine

The Discover mode uses an enum-based state machine. All modal states are represented
in `DiscoverViewState` - there are **no boolean flags** for modal states.

GlobExplorer is implemented as a separate optional field (`glob_explorer: Option<GlobExplorerState>`)
that can overlay the Files state, rather than as an enum variant.

```rust
/// View state machine for Discover mode - matches Section 4
/// Controls which dialog/dropdown/view is currently active
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiscoverViewState {
    #[default]
    Files,              // Default state, navigate files

    // --- Modal input overlays ---
    Filtering,          // Text filter input (press '/')
    EnteringPath,       // Scan path input (press 's')
    Tagging,            // Single file tag input (press 't')
    CreatingSource,     // Source name input (after path entered)
    BulkTagging,        // Bulk tag input (press 'T')

    // --- Dropdown menus ---
    SourcesDropdown,    // Filtering/selecting sources (press '1')
    TagsDropdown,       // Filtering/selecting tags (press '2')

    // --- Full dialogs ---
    RulesManager,       // Dialog for rule CRUD (press 'R')
    RuleCreation,       // Dialog for creating/editing single rule

    // --- Sources Manager (M key) ---
    SourcesManager,     // Dialog for source CRUD
    SourceEdit,         // Nested dialog for editing source name
    SourceDeleteConfirm,// Delete confirmation dialog

    // --- Background operations ---
    Scanning,           // Directory scan in progress (non-blocking)
}
```

### 5.2 Focus Areas

Focus determines which sidebar panel is active (independent of modal state):

```rust
/// Focus areas within Discover mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoverFocus {
    #[default]
    Files,
    Sources,
    Tags,  // Users browse by tag category
}
```

### 5.3 Tag Filtering

```rust
/// Filter applied to file list based on tag selection
#[derive(Debug, Clone, PartialEq)]
pub enum TagFilter {
    Untagged,           // Show files where tag IS NULL
    Tag(String),        // Show files with specific tag
}
```

### 5.4 Main State Struct

```rust
/// State for the Discover mode (File Explorer)
#[derive(Debug, Clone, Default)]
pub struct DiscoverState {
    // --- State machine (per Section 4) ---
    /// Current view state - controls which dialog/dropdown is active
    pub view_state: DiscoverViewState,
    /// Previous state for "return to previous" transitions (Esc from dialogs)
    pub previous_view_state: Option<DiscoverViewState>,
    /// Active tag filter applied to files
    pub tag_filter: Option<TagFilter>,

    // --- File list ---
    pub files: Vec<FileInfo>,
    pub selected: usize,
    /// Text filter for file list (used in Filtering state)
    pub filter: String,
    pub preview_open: bool,
    /// Whether data has been loaded from Scout DB
    pub data_loaded: bool,

    // --- Scan dialog (EnteringPath state) ---
    pub scan_path_input: String,
    pub scan_error: Option<String>,

    // --- Tagging (Tagging state) ---
    pub tag_input: String,
    pub available_tags: Vec<String>,

    // --- Source creation (CreatingSource state) ---
    pub source_name_input: String,
    pub pending_source_path: Option<String>,

    // --- Bulk tagging (BulkTagging state) ---
    pub bulk_tag_input: String,
    pub bulk_tag_save_as_rule: bool,

    // --- Glob Explorer (hierarchical file browsing) ---
    /// GlobExplorer state (Some = explorer active, None = flat file list)
    /// Phase is tracked inside GlobExplorerState.phase
    pub glob_explorer: Option<GlobExplorerState>,

    // --- Sidebar state ---
    pub focus: DiscoverFocus,
    pub sources: Vec<SourceInfo>,
    /// Selected source by ID (stable across list changes)
    pub selected_source_id: Option<SourceId>,
    pub sources_loaded: bool,

    // --- Tags dropdown (TagsDropdown state) ---
    pub tags: Vec<TagInfo>,
    pub selected_tag: Option<usize>,  // None = "All files"
    pub tags_filter: String,
    pub tags_filtering: bool,         // Vim-style modal filtering
    pub preview_tag: Option<usize>,   // Temporary preview while navigating

    // --- Sources dropdown (SourcesDropdown state) ---
    pub sources_filter: String,
    pub sources_filtering: bool,
    pub preview_source: Option<usize>,

    // --- Rules Manager (RulesManager state) ---
    pub rules: Vec<RuleInfo>,
    pub selected_rule: usize,

    // --- Rule dialog (RuleCreation state) ---
    pub rule_tag_input: String,
    pub rule_pattern_input: String,
    pub editing_rule_id: Option<RuleId>,
    pub rule_dialog_focus: RuleDialogFocus,
    pub rule_preview_files: Vec<String>,
    pub rule_preview_count: usize,

    // --- Sources Manager (SourcesManager state) ---
    pub sources_manager_selected: usize,
    pub source_edit_input: String,
    pub editing_source: Option<SourceId>,
    pub source_to_delete: Option<SourceId>,

    // --- Pending DB writes ---
    pub pending_tag_writes: Vec<PendingTagWrite>,
    pub pending_rule_writes: Vec<PendingRuleWrite>,

    // --- Background scanning (Scanning state) ---
    pub scanning_path: Option<String>,
    pub scan_progress: Option<ScoutProgress>,
    pub scan_start_time: Option<std::time::Instant>,

    // --- Directory autocomplete ---
    pub path_suggestions: Vec<String>,
    pub path_suggestion_idx: usize,

    // --- User feedback ---
    pub status_message: Option<(String, bool)>,  // (message, is_error)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleDialogFocus {
    #[default]
    Pattern,
    Tag,
}
```

### 5.5 Supporting Types

```rust
/// Newtype wrappers for type safety
pub struct SourceId(pub i64);
pub struct RuleId(pub i64);

/// Source information for sidebar
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub id: SourceId,
    pub name: String,
    pub path: std::path::PathBuf,
    pub file_count: usize,
}

/// Tag with file count (for Tags dropdown)
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,        // Tag name, "All files", or "untagged"
    pub count: usize,        // Number of files with this tag
    pub is_special: bool,    // True for "All files" and "untagged"
}

/// Tagging rule (for Rules Manager)
#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: RuleId,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
    pub enabled: bool,
}

/// File information for file list
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub rel_path: String,    // Relative path from source root
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Local>,
    pub is_dir: bool,
    pub tags: Vec<String>,
}

/// Pending tag write for persistence
#[derive(Debug, Clone)]
pub struct PendingTagWrite {
    pub file_path: String,
    pub tag: String,
}

/// Pending rule write for persistence
#[derive(Debug, Clone)]
pub struct PendingRuleWrite {
    pub pattern: String,
    pub tag: String,
    pub source_id: SourceId,
}
```

### 5.6 Glob Explorer State

For hierarchical file browsing in large sources. GlobExplorer is stored as an optional
field on DiscoverState (`glob_explorer: Option<GlobExplorerState>`). The phase is
tracked inside the state struct, not as a separate enum variant.

```rust
/// State for Glob Explorer - hierarchical file browsing
/// Stored as discover.glob_explorer: Option<GlobExplorerState>
#[derive(Debug, Clone)]
pub struct GlobExplorerState {
    // --- Input state ---
    pub pattern: String,
    pub nav_history: Vec<(String, String)>,
    pub current_prefix: String,

    // --- Derived state (from DB) ---
    pub folders: Vec<FolderInfo>,
    pub preview_files: Vec<GlobPreviewFile>,
    pub total_count: GlobFileCount,

    // --- O(1) Navigation Cache ---
    pub folder_cache: HashMap<String, Vec<FolderInfo>>,
    pub cache_loaded: bool,
    pub cache_source_id: Option<String>,

    // --- UI state ---
    pub selected_folder: usize,
    pub pattern_cursor: usize,
    pub phase: GlobExplorerPhase,  // Phase is a field, not enum variant

    // --- Rule Editing ---
    pub rule_draft: Option<RuleDraft>,
    pub test_state: Option<TestState>,
    pub publish_state: Option<PublishState>,

    // --- Debouncing ---
    pub pattern_changed_at: Option<std::time::Instant>,
    pub last_searched_pattern: String,
    pub last_searched_prefix: String,

    // --- Rule Builder Enhancements ---
    pub result_filter: ResultFilter,
    pub excludes: Vec<String>,
    pub backtest_summary: BacktestSummary,
}

/// GlobExplorer phase - stored in GlobExplorerState.phase
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GlobExplorerPhase {
    #[default]
    Browse,     // Browsing folders without pattern
    Filtering,  // Pattern active, showing heat map
    EditRule { focus: RuleEditorFocus, selected_index: usize, editing_field: Option<FieldEditFocus> },
    Testing,
    Publishing,
    Published { job_id: String },
}
```

---

## 6. Keybindings

> **Design Pattern:** Following [Lazygit](https://github.com/jesseduffield/lazygit) and
> [K9s](https://k9scli.io) conventions, keys are organized by **function type** rather than
> by keyboard layout. This makes it easy to learn: navigation keys always navigate,
> action keys always perform actions, etc.

### 6.1 Universal Keys (Work Everywhere)

These keys work in **any** context—main view, dropdowns, or dialogs:

| Category | Key | Action |
|----------|-----|--------|
| **Exit** | `Esc` | Close current dialog/dropdown, or return to Home |
| **Confirm** | `Enter` | Confirm/select/save (context-dependent) |
| **Help** | `?` | Show available keys for current context |

### 6.2 Navigation Keys

| Category | Key | Action |
|----------|-----|--------|
| **Vim-style** | `j` / `k` | Move down / up |
| **Arrows** | `↑` / `↓` | Move up / down |
| **Page** | `Ctrl+d` / `Ctrl+u` | Page down / up |
| **Jump** | `g` `g` / `G` | Jump to top / bottom |
| **Panel focus** | `1` / `2` / `3` | Focus Sources / Tags / Files panel |
| **Tab** | `Tab` / `Shift+Tab` | Next / previous field or section |

> **Note:** In Discover mode, `1`, `2`, `3` override global view navigation to control
> panel focus. Use `0`/`H` (Home) or `Esc` to navigate to other views.

### 6.3 Action Keys (Mnemonic)

Single letters that perform actions. Same letter = same action across contexts:

| Key | Action | Mnemonic |
|-----|--------|----------|
| `n` | Create **n**ew item (rule, source, field, condition) | **N**ew |
| `e` | **E**dit selected item | **E**dit |
| `d` | **D**elete selected item | **D**elete |
| `r` | **R**escan / **R**efresh | **R**efresh |
| `t` | **T**ag file(s) or **T**est rule | **T**ag/**T**est |
| `s` | **S**can new directory | **S**can |
| `p` | Toggle **P**review pane | **P**review |
| `y` | Confirm (**Y**es) | **Y**es |

### 6.4 Dialog/Overlay Keys (Shift = Window)

Shift+Letter opens a dialog or overlay window:

| Key | Opens | Mnemonic | Status |
|-----|-------|----------|--------|
| `R` | **R**ules Manager dialog | **R**ules | ✓ Implemented |
| `M` | Sources **M**anager dialog | **M**anager | ✓ Implemented |
| `T` | Bulk **T**ag dialog | **T**ag bulk | ✓ Implemented |
| `S` | Create **S**ource from selected directory | **S**ource | ✓ Implemented |

> **Note:** AI Wizards (Pathfinder, Semantic Path, Parser Lab) were consolidated into the
> GlobExplorer EditRule phase. Rule creation with AI assistance is accessed via `g` -> `e`.

### 6.5 Mode Switch Keys

Keys that change the input mode or context:

| Key | Mode | Description |
|-----|------|-------------|
| `/` | Filter mode | Type to filter current list |
| `g` | Glob Explorer | Open interactive pattern exploration |
| `Esc` | Exit mode | Return to normal navigation |

---

### 6.6 Context-Specific Keys

> **Updated:** 2026-01-14 (spec refinement session - GAP-INT-001)

The following tables show additional keys available in specific contexts.
Universal, Navigation, and Action keys from above also apply.

**Footer content** for each context is shown after the key table.

#### Sources Dropdown

| Key | Action |
|-----|--------|
| `Char(c)` | Append to filter (immediate, no `/` needed) |
| `Backspace` | Remove from filter |
| `Enter` | Select highlighted source |
| `up/down` | Navigate sources |
| `Esc` | Cancel dropdown |

**Footer:** `[Enter] Select  [up/down] Navigate  | [R] Rules [M] Sources | [Esc] Cancel`

#### Tags Dropdown

| Key | Action |
|-----|--------|
| `Char(c)` | Append to filter |
| `Backspace` | Remove from filter / go to "All files" |
| `Enter` | Select highlighted tag |
| `up/down` | Navigate tags |
| `Esc` | Return to "All files" |

**Live Preview:** Files panel updates in real-time as you navigate tags.

**Footer:** `[Enter] Select  [up/down] Navigate  | [R] Rules [M] Sources | [Esc] All files`

#### Files Panel

| Key | Action |
|-----|--------|
| `g` | Open Glob Explorer |
| `/` | Open filter mode |
| `t` | Tag selected file |
| `s` | Scan new directory |
| `R` | Open Rules Manager |
| `M` | Open Sources Manager |
| `1` | Focus Sources dropdown |
| `2` | Focus Tags dropdown |

**Footer (no filter):** `[g] Explorer  [/] Filter  [t] Tag  [s] Scan  | [R] Rules [M] Sources | 1:Source 2:Tags`

**Footer (with filter):** `[t] Tag {N} files  [Esc] Clear  | [R] Rules [M] Sources | 1:Source 2:Tags`

#### Files Panel (Glob Explorer)

| Key | Action |
|-----|--------|
| `l` / `Enter` | Drill into directory |
| `h` / `Backspace` | Go back to parent |
| `n` | Create new rule from current pattern |
| `e` | Enter EditRule phase (when matches > 0) |
| `/` | Enter pattern filter mode |
| `g` / `Esc` | Exit Glob Explorer |

**Footer (Browse):** `[hjkl] Navigate  [l/Enter] In  [h/Bksp] Back  [/] Filter  [n] New Rule  [e] Edit  [g/Esc] Exit`

**Footer (Filtering):** `[Enter] Done  [Esc] Cancel  | Type glob pattern (e.g., *.csv)`

#### Rule Creation Dialog

| Key | Action |
|-----|--------|
| `Tab` | Switch between Pattern and Tag fields |
| `Enter` | Create rule and close |
| `Esc` | Cancel |

**Footer:** `[Enter] Save rule  [Esc] Cancel`

#### Rules Manager Dialog

| Key | Action |
|-----|--------|
| `n` | New rule |
| `e` | Edit selected rule |
| `d` | Delete selected rule |
| `Enter` | Toggle rule enabled/disabled |
| `Esc` | Close dialog |

**Footer:** `[n] New  [e] Edit  [d] Del  [Enter] Toggle  [Esc] Close`

#### Sources Manager Dialog

| Key | Action |
|-----|--------|
| `n` | New source |
| `e` | Edit source name |
| `d` | Delete source |
| `r` | Rescan selected source |
| `Esc` | Close dialog |

**Footer:** `[n] New  [e] Edit  [d] Del  [r] Rescan  [Esc] Close`

#### Source Delete Confirmation

| Key | Action |
|-----|--------|
| `Enter` / `y` | Confirm deletion |
| `Esc` / `n` | Cancel |

**Footer:** `[Enter/y] Confirm delete  [Esc/n] Cancel`

---

## 7. Tag Loading Behavior

### 7.1 Load Tags for Source

Tags are derived from files, not from rules:

```sql
-- Get distinct tags with counts for current source
SELECT
    tag,
    COUNT(*) as count
FROM scout_files
WHERE source_id = ? AND tag IS NOT NULL
GROUP BY tag
ORDER BY count DESC, tag

-- Also count untagged files
SELECT COUNT(*) FROM scout_files
WHERE source_id = ? AND tag IS NULL
```

Result is rendered as:
```
All files (142)     <- sum of all files
sales (89)          <- from query
logs (34)
invoices (12)
untagged (7)        <- from second query
```

### 7.2 Tag Selection (Live Filter)

When navigating tags:

```rust
// User selects a tag
match selected_tag {
    None => {
        // "All files" - show everything
        self.discover.tag_filter = None;
    }
    Some(tag_info) if tag_info.name == "untagged" => {
        // Show files where tag IS NULL
        self.discover.tag_filter = Some(TagFilter::Untagged);
    }
    Some(tag_info) => {
        // Show files with this specific tag
        self.discover.tag_filter = Some(TagFilter::Tag(tag_info.name.clone()));
    }
}
```

### 7.3 Rules Apply in Background

Tagging rules run:
1. When files are first discovered (scan)
2. When a new rule is created (applies to existing files)
3. When user manually triggers re-tagging

Tags dropdown shows the RESULT (what tags exist), not the mechanism (what rules exist).

---

## 8. Extractors (Path Metadata Extraction)

> **⚠️ DEPRECATION NOTICE (v1.5):** Python Extractors for **path parsing** are deprecated in favor of **Extraction Rules** (see `specs/extraction_rules.md`). Extraction Rules provide:
> - Declarative YAML configuration instead of imperative Python
> - DFA-based multi-pattern matching for O(1) performance
> - Semantic Path integration for AI-assisted rule generation
> - Coverage reports with near-miss detection
>
> **Migration path:** Existing Python extractors should be converted to Extraction Rules. Python extractors remain supported for **content-based extraction** (parsing file internals), but path-based metadata extraction should use Extraction Rules.
>
> See `specs/extraction_rules.md` Section 1.5 for the authoritative consolidation decision.

Extractors are Python functions that extract structured metadata from file paths. This enables queryable attributes derived from path conventions (e.g., `ADT_Inbound/2024/01/file.hl7` → `{direction: "Inbound", year: "2024", month: "01"}`).

### 8.1 Problem Statement

Many organizations encode valuable metadata in their folder structures:
- Healthcare: `ADT_Inbound/2024/01/` → direction, year, month
- Defense: `mission_alpha/day_3/` → mission name, day
- Finance: `gateway_prod/2024Q1/` → environment, quarter

This metadata is invisible to queries unless manually tagged. Extractors automate this extraction.

### 8.2 Core Concepts

**Extractor**: A Python function that takes a file path and returns a metadata dictionary:

```python
def healthcare_path_extractor(path: str) -> dict:
    """Extract metadata from healthcare interface paths."""
    parts = Path(path).parts
    metadata = {}

    for part in parts:
        # Direction detection
        if "_Inbound" in part:
            metadata["direction"] = "Inbound"
        elif "_Outbound" in part:
            metadata["direction"] = "Outbound"

        # Year detection (4-digit folder)
        if part.isdigit() and len(part) == 4:
            metadata["year"] = part

        # Month detection (2-digit folder, 01-12)
        if part.isdigit() and len(part) == 2 and 1 <= int(part) <= 12:
            metadata["month"] = part

    return metadata
```

**Inheritance**: Metadata flows down the folder hierarchy:
- Folder `/data/2024/` has `{year: "2024"}`
- File `/data/2024/jan.csv` inherits `{year: "2024"}` automatically
- Child metadata overrides parent metadata (child wins)

**Staleness**: When an extractor is modified, previously-extracted metadata becomes STALE and needs re-extraction.

### 8.3 Data Model

#### 8.3.1 Database Schema

```sql
-- Existing scout_files table gets new columns
ALTER TABLE scout_files ADD COLUMN metadata_raw TEXT;           -- JSON blob
ALTER TABLE scout_files ADD COLUMN extraction_status TEXT;      -- OK, TIMEOUT, CRASH, STALE, PENDING
ALTER TABLE scout_files ADD COLUMN extracted_at TIMESTAMP;      -- When extraction ran
ALTER TABLE scout_files ADD COLUMN extractor_version TEXT;      -- Hash of extractor code

-- Extractor registry
CREATE TABLE scout_extractors (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source_path TEXT NOT NULL,              -- Path to Python file
    source_hash TEXT NOT NULL,              -- blake3 hash of code
    associated_tag TEXT,                    -- Optional: only run for files with this tag
    priority INTEGER DEFAULT 100,           -- Higher = runs first
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Track extraction history for debugging
CREATE TABLE scout_extraction_log (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES scout_files(id),
    extractor_id INTEGER NOT NULL REFERENCES scout_extractors(id),
    status TEXT NOT NULL,                   -- OK, TIMEOUT, CRASH
    error_message TEXT,                     -- If CRASH, the error details
    duration_ms INTEGER,                    -- Extraction time
    extracted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

#### 8.3.2 Extraction Status Enum

| Status | Meaning |
|--------|---------|
| `OK` | Extraction succeeded, metadata is current |
| `PENDING` | File discovered, extraction not yet run |
| `TIMEOUT` | Extractor exceeded time limit (default 5s) |
| `CRASH` | Extractor raised an exception |
| `STALE` | Extractor code changed since extraction |

#### 8.3.3 Rust Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: i64,
    pub path: String,
    pub rel_path: String,
    pub size: u64,
    pub tag: Option<String>,
    pub status: FileStatus,

    // --- Extractor fields ---
    pub metadata_raw: Option<serde_json::Value>,    // Raw JSON from DB
    pub metadata_merged: Option<serde_json::Value>, // After inheritance merge
    pub extraction_status: ExtractionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtractionStatus {
    Ok,
    Pending,
    Timeout,
    Crash,
    Stale,
}

#[derive(Debug, Clone)]
pub struct ExtractorInfo {
    pub id: i64,
    pub name: String,
    pub source_path: String,
    pub source_hash: String,
    pub associated_tag: Option<String>,
    pub priority: i32,
    pub enabled: bool,
}
```

### 8.4 Inheritance Model (Read-Time Merge)

Metadata inheritance is computed at **read time in Rust**, not stored in the database. This avoids complex recursive SQL and enables efficient caching.

#### 8.4.1 Merge Algorithm

```rust
/// Merge metadata from ancestors (folder → subfolder → file)
/// Child values override parent values (last writer wins)
pub fn merge_metadata_chain(
    file_path: &str,
    folder_metadata: &HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    let mut merged = serde_json::Map::new();

    // Walk path from root to file, accumulating metadata
    let path = Path::new(file_path);
    let mut current = PathBuf::new();

    for component in path.parent().unwrap_or(path).components() {
        current.push(component);
        let folder_path = current.to_string_lossy();

        if let Some(folder_meta) = folder_metadata.get(folder_path.as_ref()) {
            if let Some(obj) = folder_meta.as_object() {
                for (k, v) in obj {
                    merged.insert(k.clone(), v.clone()); // Child overwrites parent
                }
            }
        }
    }

    serde_json::Value::Object(merged)
}
```

#### 8.4.2 Caching Strategy

```rust
/// Cache for folder metadata to avoid repeated DB queries
pub struct MetadataCache {
    /// folder_path → metadata JSON
    folder_cache: HashMap<String, serde_json::Value>,
    /// source_id this cache is valid for
    source_id: i64,
    /// When cache was populated
    populated_at: Instant,
}

impl MetadataCache {
    /// Load all folder metadata for a source in one query
    pub async fn load_for_source(pool: &SqlitePool, source_id: i64) -> Self {
        let folders: Vec<(String, String)> = sqlx::query_as(
            "SELECT rel_path, metadata_raw FROM scout_files
             WHERE source_id = ? AND is_directory = TRUE AND metadata_raw IS NOT NULL"
        )
        .bind(source_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let folder_cache = folders.into_iter()
            .filter_map(|(path, json)| {
                serde_json::from_str(&json).ok().map(|v| (path, v))
            })
            .collect();

        Self {
            folder_cache,
            source_id,
            populated_at: Instant::now(),
        }
    }
}
```

### 8.5 Execution Model

#### 8.5.1 When Extractors Run

| Trigger | Behavior |
|---------|----------|
| **Scan** | New files get `extraction_status = PENDING` |
| **Background job** | Picks up PENDING files, runs extractors |
| **Extractor modified** | Marks affected files as STALE |
| **Manual re-extract** | User triggers re-extraction for selected files |

#### 8.5.2 Execution Isolation

Extractors run in isolated Python subprocesses with resource limits:

```rust
pub struct ExtractorRunner {
    timeout: Duration,          // Default 5 seconds
    max_memory_mb: usize,       // Default 256 MB
    python_path: PathBuf,       // Path to Python interpreter
}

impl ExtractorRunner {
    pub async fn run_extractor(
        &self,
        extractor: &ExtractorInfo,
        file_path: &str,
    ) -> ExtractorResult {
        let start = Instant::now();

        // Spawn isolated subprocess
        let result = tokio::time::timeout(
            self.timeout,
            self.spawn_extractor_process(extractor, file_path)
        ).await;

        match result {
            Ok(Ok(metadata)) => ExtractorResult::Ok {
                metadata,
                duration: start.elapsed(),
            },
            Ok(Err(e)) => ExtractorResult::Crash {
                error: e.to_string(),
                duration: start.elapsed(),
            },
            Err(_) => ExtractorResult::Timeout {
                duration: self.timeout,
            },
        }
    }
}
```

#### 8.5.3 Batch Processing

Files are processed in batches with fail-fast semantics:

| Scenario | Behavior |
|----------|----------|
| Single file crashes | Mark that file CRASH, continue with others |
| Extractor itself is broken | After N consecutive crashes, pause extractor |
| Timeout storm | After N consecutive timeouts, increase timeout or pause |

```rust
pub struct BatchExtractor {
    max_consecutive_failures: usize,  // Default 5
    failure_count: usize,
}

impl BatchExtractor {
    pub async fn process_batch(&mut self, files: Vec<FileInfo>) -> BatchResult {
        let mut results = Vec::new();

        for file in files {
            let result = self.runner.run_extractor(&self.extractor, &file.path).await;

            match &result {
                ExtractorResult::Ok { .. } => {
                    self.failure_count = 0;  // Reset on success
                }
                ExtractorResult::Crash { .. } | ExtractorResult::Timeout { .. } => {
                    self.failure_count += 1;
                    if self.failure_count >= self.max_consecutive_failures {
                        return BatchResult::ExtractorPaused {
                            reason: "Too many consecutive failures",
                            processed: results,
                        };
                    }
                }
            }

            results.push((file.id, result));
        }

        BatchResult::Complete { results }
    }
}
```

### 8.6 TUI Integration

#### 8.6.1 Files Panel Enhancement

Files with extraction issues show status indicators:

```
┌─ FILES ──────────────────────────────────────────────────────┐
│  invoices/jan.csv        [sales]  2KB   {year: 2024}         │
│  invoices/feb.csv        [sales]  3KB   {year: 2024}         │
│  reports/q1.xlsx                 15KB   ⚠ STALE              │
│  data/orders.json        [api]   8KB    ❌ CRASH             │
│  logs/app.log                    1MB    ⏱ TIMEOUT            │
└──────────────────────────────────────────────────────────────┘
```

#### 8.6.2 Metadata Filter (Query Builder)

A dedicated metadata filter allows querying by extracted fields:

```
┌─ METADATA FILTER [m] ────────────────────────────────────────┐
│  Filter: year = "2024" AND direction = "Inbound"             │
│                                                               │
│  Available fields:          Operators:                        │
│  ├── year (142 files)       = equals                          │
│  ├── month (142 files)      != not equals                     │
│  ├── direction (89 files)   CONTAINS                          │
│  └── mission (34 files)     EXISTS                            │
│                                                               │
│  [Enter] Apply   [Tab] Field picker   [Esc] Cancel            │
└───────────────────────────────────────────────────────────────┘
```

#### 8.6.3 Problems Tab

A dedicated view for files with extraction issues:

```
┌─ PROBLEMS [!] ───────────────────────────────────────────────┐
│                                                               │
│  ❌ CRASH (3 files)                                           │
│  ├── data/orders.json: ValueError: Invalid JSON              │
│  ├── data/broken.csv: UnicodeDecodeError                     │
│  └── data/huge.xml: MemoryError                              │
│                                                               │
│  ⏱ TIMEOUT (2 files)                                         │
│  ├── logs/app.log: Exceeded 5s limit                         │
│  └── logs/debug.log: Exceeded 5s limit                       │
│                                                               │
│  ⚠ STALE (12 files) - extractor "healthcare" was modified    │
│                                                               │
│  [r] Re-extract selected   [R] Re-extract all   [Esc] Close   │
└───────────────────────────────────────────────────────────────┘
```

### 8.7 Keybindings (Extractor-Related)

| Key | Context | Action |
|-----|---------|--------|
| `m` | Files panel | Open metadata filter dialog |
| `M` | Files panel | Show metadata for selected file |
| `!` | Global | Open Problems tab |
| `e` | File selected | Re-extract metadata for file |
| `E` | Files panel | Re-extract all files in view |

### 8.8 Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| JSON blobs, not dynamic columns | `metadata_raw TEXT` | Avoids schema migrations, flexible, queryable with JSON functions |
| Read-time merge | Rust HashMap + walk | Simpler than SQL recursive CTEs, cacheable, no denormalization |
| Explicit error states | TIMEOUT, CRASH, STALE enum | Actionable by user, no ambiguity about what went wrong |
| Subprocess isolation | Separate Python process | Memory/CPU limits, crash isolation, security |
| Fail-fast batching | Pause after N failures | Prevents runaway broken extractors from burning resources |

### 8.9 Pending Review Panel

Files and groups needing human attention are surfaced in the Pending Review panel (`!` key).

#### 8.9.1 What Triggers Pending Review

| Condition | Category | Action Available |
|-----------|----------|------------------|
| File has no matching extraction rule | Unmatched Paths | Launch Pathfinder Wizard |
| Group has no semantic label | Unlabeled Groups | Launch Labeling Wizard |
| Source has no extraction rules | Unrecognized Sources | Launch Semantic Path Wizard |
| Extraction failed (CRASH/TIMEOUT) | Failed Extractions | Re-extract or inspect |
| Extraction is STALE | Stale Metadata | Re-extract |
| Parser has warnings | Parser Warnings | View warnings, launch Fix wizard |
| Near-miss patterns detected | Coverage Gaps | Review typos, add rules |
| Rule coverage below threshold | Low Coverage | Expand rule patterns |

> **Coverage Report Integration:** The Pending Review panel integrates with the Coverage Report system (see `specs/extraction_rules.md` Section 9.5). Near-miss detection automatically surfaces potential typos in folder names or missing rule patterns.

#### 8.9.2 Pending Review Dialog

```
┌─ PENDING REVIEW [!] ────────────────────────────────────────────┐
│                                                                  │
│  ┌─ Unrecognized Sources (2 sources) ───────────────────────┐   │
│  │  /mnt/new_vendor_data (347 files)                        │   │
│  │    Detected: entity_folder > dated_hierarchy (82%)       │   │
│  │  /mnt/legacy_archive (89 files)                          │   │
│  │    No semantic pattern detected                          │   │
│  │  [S] Launch Semantic Path Wizard                         │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌─ Unmatched Paths (23 files) ─────────────────────────────┐   │
│  │  /data/new_vendor/2024/...  (15 files)                   │   │
│  │  /data/legacy/archive/...   (8 files)                    │   │
│  │  [w] Launch Pathfinder Wizard                            │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌─ Unlabeled Groups (3 groups) ────────────────────────────┐   │
│  │  Group a7b3c9d2: 142 files, CSV [id, date, amount]       │   │
│  │  Group f8e2d1c0: 89 files, JSON {user, event, ts}        │   │
│  │  Group b4c5d6e7: 34 files, TSV [col0, col1, col2]        │   │
│  │  [l] Launch Labeling Wizard                              │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌─ Failed Extractions (5 files) ───────────────────────────┐   │
│  │  ❌ /data/orders.json: CRASH - ValueError                │   │
│  │  ⏱ /data/huge.xml: TIMEOUT - exceeded 5s                 │   │
│  │  [r] Re-extract   [i] Inspect error                      │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌─ Coverage Gaps (14 near-misses) ───────────────────────────┐  │
│  │  ⚠ Rule "Mission Data" has potential typos:               │  │
│  │    • "mision_*" (14 files) - did you mean "mission_*"?    │  │
│  │    • "missin_*" (2 files) - did you mean "mission_*"?     │  │
│  │  [a] Add pattern to rule   [x] Ignore   [c] Coverage report│  │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  [Tab] Switch category   [Enter] Select   [Esc] Close           │
└──────────────────────────────────────────────────────────────────┘
```

#### 8.9.3 Database View for Pending Review

```sql
-- Files needing attention (combined view)
CREATE VIEW v_pending_review AS

-- Unmatched paths: files with no extractor and PENDING status
SELECT
    'unmatched_path' as review_type,
    f.id as item_id,
    f.rel_path as description,
    NULL as error_message,
    f.source_id
FROM scout_files f
WHERE f.extraction_status = 'PENDING'
  AND NOT EXISTS (
    SELECT 1 FROM scout_extractors e
    WHERE (e.associated_tag = f.tag OR e.associated_tag IS NULL)
      AND e.enabled = TRUE
  )

UNION ALL

-- Failed extractions
SELECT
    'failed_extraction' as review_type,
    f.id as item_id,
    f.rel_path as description,
    l.error_message,
    f.source_id
FROM scout_files f
LEFT JOIN scout_extraction_log l ON l.file_id = f.id
WHERE f.extraction_status IN ('CRASH', 'TIMEOUT')

UNION ALL

-- Stale metadata
SELECT
    'stale_metadata' as review_type,
    f.id as item_id,
    f.rel_path as description,
    'Extractor code changed' as error_message,
    f.source_id
FROM scout_files f
WHERE f.extraction_status = 'STALE';
```

#### 8.9.4 Keybindings (Pending Review Panel)

| Key | Action |
|-----|--------|
| `Tab` | Switch between categories |
| `j` / `↓` | Move down in current category |
| `k` / `↑` | Move up in current category |
| `S` | Launch Semantic Path Wizard (Unrecognized Sources) |
| `w` | Launch Pathfinder Wizard (Unmatched Paths) |
| `l` | Launch Labeling Wizard (Unlabeled Groups) |
| `r` | Re-extract selected file |
| `R` | Re-extract all in category |
| `i` | Inspect error details |
| `a` | Add near-miss pattern to rule (Coverage Gaps) |
| `x` | Ignore near-miss (mark as intentional) |
| `c` | Open full coverage report |
| `Enter` | Jump to file in Files panel |
| `Esc` | Close panel |

### 8.10 Semantic Path Integration

> **Full Specification:** See `specs/semantic_path_mapping.md`

Discover mode integrates with the Semantic Path Mapping system to automatically recognize and suggest extraction rules for new sources.

#### 8.10.1 Automatic Recognition on Scan

When scanning a new source, the system automatically runs semantic path recognition:

```
┌─ SCAN COMPLETE ─────────────────────────────────────────────────┐
│                                                                  │
│  Source: /mnt/mission_data                                       │
│  Files discovered: 347                                           │
│                                                                  │
│  ┌─ Semantic Structure Detected ────────────────────────────┐   │
│  │                                                           │   │
│  │  Pattern: entity_folder(mission) > dated_hierarchy(iso)  │   │
│  │  Confidence: 94%                                          │   │
│  │                                                           │   │
│  │  This would extract:                                      │   │
│  │    • mission_id (from folder name)                       │   │
│  │    • date (from date folder)                             │   │
│  │                                                           │   │
│  │  Similar to: defense_contractor_a, research_lab          │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                                                  │
│  [Enter] Create extraction rule   [s] See details   [Esc] Skip  │
└──────────────────────────────────────────────────────────────────┘
```

#### 8.10.2 Source Sidebar Indicator

Sources with detected semantic structure show an indicator:

```
┌─ SOURCES [1] ─────────────┐
│ ▼ mission_data (347)  📐   │  ← 📐 = semantic structure detected
│   invoice_archive (89)     │  ← no indicator = no pattern
│   logs (1,234)        📐   │
└───────────────────────────┘

Legend:
  📐 = Semantic pattern detected, extraction rule available
  (none) = No semantic pattern detected
```

#### 8.10.3 Semantic Info in File Details

When viewing file details (`Enter` on file), semantic metadata is shown:

```
┌─ FILE DETAILS ──────────────────────────────────────────────────┐
│                                                                  │
│  Path: /mnt/mission_data/mission_042/2024-01-15/telemetry.csv   │
│  Size: 1.2 MB                                                    │
│  Modified: 2024-01-15 10:30:00                                  │
│                                                                  │
│  ┌─ Semantic Extraction ────────────────────────────────────┐   │
│  │  Rule: entity_folder(mission) > dated_hierarchy(iso)     │   │
│  │                                                           │   │
│  │  mission_id: "042"                                       │   │
│  │  date: "2024-01-15"                                      │   │
│  │                                                           │   │
│  │  Confidence: 94%                                          │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Tags: [mission_data]                                           │
│                                                                  │
│  [p] Preview content   [e] Edit tags   [Esc] Close              │
└──────────────────────────────────────────────────────────────────┘
```

#### 8.10.4 Cross-Source Discovery

When a source's semantic structure matches another source, suggest rule sharing:

```
┌─ SIMILAR SOURCE DETECTED ───────────────────────────────────────┐
│                                                                  │
│  /mnt/new_vendor_data has the same folder structure as:         │
│                                                                  │
│  • defense_contractor_a                                         │
│    Pattern: entity_folder(mission) > dated_hierarchy(iso)       │
│    47 files, created 2024-01-10                                 │
│                                                                  │
│  Would you like to apply the same extraction rule?              │
│                                                                  │
│  [y] Yes, apply same rule   [n] No, create new   [c] Customize  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 9. Empty States

| Condition | Display |
|-----------|---------|
| No sources | "No sources found. Press 's' to scan a folder." |
| Source selected, no files | "No files in this source." |
| Filter matches nothing | "No files match filter." |
| No tags (all untagged) | Tags dropdown shows only "All files" and "untagged" |
| No rules | Rules Manager shows "No rules. Press 'n' to create one." |

---

## 10. Database Queries

### 10.1 Load Sources

```sql
SELECT s.id, s.name, s.path, COUNT(f.id) as file_count
FROM scout_sources s
LEFT JOIN scout_files f ON f.source_id = s.id
GROUP BY s.id
ORDER BY s.updated_at DESC  -- Most recently used sources first
```

### 10.2 Load Tags for Source

```sql
-- Distinct tags with counts
SELECT tag, COUNT(*) as count
FROM scout_files
WHERE source_id = ? AND tag IS NOT NULL
GROUP BY tag
ORDER BY count DESC, tag

-- Untagged count
SELECT COUNT(*) as count
FROM scout_files
WHERE source_id = ? AND tag IS NULL
```

### 10.3 Load Files for Source (with tag filter)

```sql
-- All files
SELECT id, path, rel_path, size, tag, status
FROM scout_files
WHERE source_id = ?
ORDER BY rel_path

-- Files with specific tag
SELECT id, path, rel_path, size, tag, status
FROM scout_files
WHERE source_id = ? AND tag = ?
ORDER BY rel_path

-- Untagged files
SELECT id, path, rel_path, size, tag, status
FROM scout_files
WHERE source_id = ? AND tag IS NULL
ORDER BY rel_path
```

### 10.4 Load Rules for Source

```sql
SELECT id, pattern, tag, priority, enabled
FROM scout_tagging_rules
WHERE source_id = ?
ORDER BY priority DESC, pattern
```

### 10.5 Source Management Queries

```sql
-- Update source name
UPDATE scout_sources
SET name = ?
WHERE id = ?

-- Delete source (cascades to files via FK)
DELETE FROM scout_sources
WHERE id = ?

-- Get source by ID for confirmation dialog
SELECT id, name, path, (SELECT COUNT(*) FROM scout_files WHERE source_id = s.id) as file_count
FROM scout_sources s
WHERE id = ?
```

### 10.6 Load Files with Extraction Status

```sql
-- Files with metadata and extraction status
SELECT
    id, path, rel_path, size, tag, status,
    metadata_raw, extraction_status, extracted_at
FROM scout_files
WHERE source_id = ?
ORDER BY rel_path
```

### 10.6 Load Folder Metadata for Inheritance Cache

```sql
-- All folder metadata for a source (used to build cache)
SELECT rel_path, metadata_raw
FROM scout_files
WHERE source_id = ? AND is_directory = TRUE AND metadata_raw IS NOT NULL
```

### 10.7 Load Extractors

```sql
SELECT id, name, source_path, source_hash, associated_tag, priority, enabled
FROM scout_extractors
WHERE enabled = TRUE
ORDER BY priority DESC
```

### 10.8 Load Files with Extraction Problems

```sql
-- Files with extraction errors
SELECT
    f.id, f.rel_path, f.extraction_status,
    l.error_message, l.duration_ms, l.extracted_at
FROM scout_files f
LEFT JOIN scout_extraction_log l ON l.file_id = f.id
WHERE f.source_id = ?
  AND f.extraction_status IN ('TIMEOUT', 'CRASH', 'STALE')
ORDER BY f.extraction_status, f.rel_path
```

### 10.9 Mark Files as Stale (When Extractor Changes)

```sql
-- Mark files stale when extractor code changes
UPDATE scout_files
SET extraction_status = 'STALE'
WHERE source_id = ? AND tag = ?
  AND extraction_status = 'OK'
```

### 10.10 AI Audit Log (for AI Wizards - See specs/ai_wizards.md)

```sql
-- Track all AI wizard invocations for compliance/debugging
CREATE TABLE cf_ai_audit_log (
    id TEXT PRIMARY KEY,
    wizard_type TEXT NOT NULL,        -- 'pathfinder', 'parser_lab', 'labeling'
    model_name TEXT NOT NULL,         -- 'qwen2.5-coder:7b', 'phi3.5:3.8b'
    input_type TEXT NOT NULL,         -- 'path', 'sample', 'headers'
    input_hash TEXT NOT NULL,         -- blake3(input sent to LLM)
    input_preview TEXT,               -- First 500 chars (for debugging)
    redactions TEXT,                  -- JSON array: ["patient_ssn", "diagnosis"]
    output_type TEXT,                 -- 'extractor', 'parser', 'label'
    output_hash TEXT,                 -- blake3(LLM response)
    output_file TEXT,                 -- Draft file path if code generated
    duration_ms INTEGER,
    status TEXT NOT NULL,             -- 'success', 'timeout', 'error'
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_ai_audit_wizard ON cf_ai_audit_log(wizard_type);
CREATE INDEX idx_ai_audit_created ON cf_ai_audit_log(created_at);
CREATE INDEX idx_ai_audit_status ON cf_ai_audit_log(status);

-- Query recent AI activity
-- SELECT * FROM cf_ai_audit_log ORDER BY created_at DESC LIMIT 10;

-- Query failed AI invocations
-- SELECT * FROM cf_ai_audit_log WHERE status != 'success';
```

---

## 11. Implementation Phases

### Phase 1: Dropdown Foundation (Complete)
- [x] Add dropdown state fields to `DiscoverState`
- [x] Implement `sources_dropdown_open`, `sources_filter`
- [x] Add `preview_source` for live preview

### Phase 2: Tags Dropdown (Current)
- [ ] Rename `rules` to `tags` in sidebar
- [ ] Change `RuleInfo` to `TagInfo` (name + count)
- [ ] Load tags from `scout_files` (distinct tags)
- [ ] Filter files by tag (not by pattern)
- [ ] Add "All files" and "untagged" special entries

### Phase 3: Rules Manager Dialog
- [ ] Add `rules_manager_open` state
- [ ] Keep `RuleInfo` for rules (pattern, tag, priority, enabled)
- [ ] Add `R` key to open Rules Manager
- [ ] Render rules list with CRUD actions
- [ ] Create/Edit rule dialog

### Phase 4: Quick Rule Creation
- [ ] `Ctrl+S` in Files to save filter as rule
- [ ] Prompt for tag name
- [ ] Apply rule to existing files

### Phase 5: Polish
- [ ] Scan dialog implementation
- [ ] Tag dialog improvements
- [ ] Bulk tag functionality
- [ ] Preview pane content loading
- [ ] Help overlay

### Phase 6: Extractors - Data Model
- [ ] Add `metadata_raw`, `extraction_status`, `extracted_at` columns to `scout_files`
- [ ] Create `scout_extractors` table
- [ ] Create `scout_extraction_log` table
- [ ] Add `ExtractionStatus` enum to Rust types
- [ ] Update `FileInfo` struct with metadata fields

### Phase 7: Extractors - Execution Engine
- [ ] Implement `ExtractorRunner` with subprocess isolation
- [ ] Add timeout handling (default 5s)
- [ ] Add crash isolation and error capture
- [ ] Implement `BatchExtractor` with fail-fast semantics
- [ ] Add consecutive failure pause logic

### Phase 8: Extractors - Metadata Inheritance
- [ ] Implement `MetadataCache` for folder metadata
- [ ] Implement `merge_metadata_chain()` function
- [ ] Add cache invalidation on source change
- [ ] Integrate merged metadata into file loading

### Phase 9: Extractors - TUI Integration
- [ ] Add extraction status indicators to Files panel
- [ ] Implement Metadata Filter dialog (`m` key)
- [ ] Implement Problems tab (`!` key)
- [ ] Add re-extract keybindings (`e`, `E`)
- [ ] Show metadata preview for selected file (`M` key)

### Phase 10: Extractors - Management
- [ ] Extractor registration CLI (`casparian extractor add`)
- [ ] Extractor list/status CLI (`casparian extractor list`)
- [ ] Auto-detect stale files when extractor code changes
- [ ] Background extraction job scheduling

### Phase 11: Semantic Path Integration
- [ ] Automatic semantic recognition on scan
- [ ] Source sidebar indicator for semantic status
- [ ] Semantic info in file details view
- [ ] Pending Review: Unrecognized Sources category
- [ ] Cross-source discovery and rule sharing prompt
- [ ] `S` keybinding for Semantic Path Wizard

---

## 12. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sidebar shows Tags, not Rules | Tags are categories; Rules are mechanisms | Users think "show sales files" not "apply rule #3" |
| Rules managed separately | `R` opens Rules Manager dialog | Keeps sidebar simple, gives rules proper CRUD UI |
| Tags derived from files | Query `DISTINCT tag FROM scout_files` | Shows actual tags, not potential tags from rules |
| "untagged" as special tag | Explicit option in Tags dropdown | Easy to find files needing tagging |
| Rules apply in background | On scan and rule creation | Tags appear automatically, no manual "run rules" step |
| Navigation keys | Arrow keys only in dropdowns | j/k conflict with filter typing |
| Quick rule creation | `Ctrl+S` in Files panel | Natural "save filter" gesture |
| **Extractors: JSON blobs** | `metadata_raw TEXT` column | Avoids schema migrations, flexible, queryable with JSON functions |
| **Extractors: Read-time merge** | Rust HashMap + path walk | Simpler than SQL recursive CTEs, cacheable, no denormalization |
| **Extractors: Explicit error states** | TIMEOUT, CRASH, STALE enum | Actionable by user, no ambiguity about failure reason |
| **Extractors: Subprocess isolation** | Separate Python process | Memory/CPU limits, crash isolation, security boundary |
| **Extractors: Fail-fast batching** | Pause after N consecutive failures | Prevents runaway broken extractors from burning resources |
| **Extractors: Inheritance direction** | Parent → child (child wins) | Intuitive: specific overrides general, like CSS cascade |
| **AI: Build-time only** | No runtime LLM calls | Determinism, scale, auditability (see specs/ai_wizards.md) |
| **AI: Wizards generate code** | AI outputs Python/Regex, not decisions | Layer 1 runtime is dumb/fast, AI is "smart typewriter" |
| **AI: Pending Review queue** | Surface unmatched patterns | Drives users to wizards organically |
| **Semantic: Auto-recognize on scan** | Run semantic recognition on new sources | Proactive assistance, reduce manual rule authoring |
| **Semantic: Sidebar indicator** | Show 📐 for sources with semantic rules | Quick visibility into source status |
| **Semantic: Cross-source discovery** | Suggest rule sharing for similar structures | Knowledge transfer between semantically equivalent sources |

---

## 13. Glob Explorer

The Glob Explorer provides interactive pattern-based file exploration with hierarchical folder drilling and glob pattern matching.

> **Cross-reference:** For extraction rule YAML schema, database tables, and CLI commands,
> see `specs/extraction.md`. The Glob Explorer is the TUI interface for the Extraction API.

### 13.1 Design Philosophy

- **Streaming scan**: Files are persisted in batches during scan, with bounded memory O(batch_size) not O(file_count)
- **On-demand loading**: Folder hierarchy is queried from SQLite on drill-down, not preloaded (<5ms per query)
- **Progressive reveal**: Heat map shows match density; flat results appear below threshold (~200 matches)
- **Full glob syntax**: Supports `**/*.csv`, `logs/**/*.log`, etc. via `globset` crate
- **Vim-style navigation**: `hjkl` for navigation, `l`/`Enter` to drill in, `h`/Backspace to go back
- **Cancel support**: User can abort scan at any time via Esc

> **Architecture:** See `specs/streaming_scanner.md` for full streaming architecture specification.

### 13.2 Folder Storage Architecture

The folder hierarchy is stored in the main SQLite database and updated incrementally during scan.

**Storage:** `scout_folders` table in `~/.casparian_flow/casparian_flow.sqlite3`

**Schema:**

```sql
CREATE TABLE scout_folders (
    id INTEGER PRIMARY KEY,
    source_id TEXT NOT NULL,
    prefix TEXT NOT NULL,      -- "" for root, "logs/" for /logs folder
    name TEXT NOT NULL,        -- folder or file name
    file_count INTEGER NOT NULL,
    is_folder BOOLEAN NOT NULL,
    UNIQUE(source_id, prefix, name)
);

CREATE INDEX idx_scout_folders_lookup
    ON scout_folders(source_id, prefix);
```

**Query for drill-down:**

```rust
pub async fn get_folder_children(
    db: &Database,
    source_id: &str,
    prefix: &str,
) -> Vec<FolderEntry> {
    sqlx::query_as(r#"
        SELECT name, file_count, is_folder
        FROM scout_folders
        WHERE source_id = ? AND prefix = ?
        ORDER BY is_folder DESC, name ASC
    "#)
    .bind(source_id)
    .bind(prefix)
    .fetch_all(db.pool()).await
    .unwrap_or_default()
}
```

**Streaming scan flow:**
```
Scan Job Start (ScanStream::start())
     │
     ▼
Walker threads discover files in parallel
     │
     ├──▶ mpsc::channel(10) ──▶ Batch Writer Task
     │    (bounded backpressure)     │
     │                               ├──▶ INSERT batch into scout_files
     │                               ├──▶ UPSERT ancestors into scout_folders
     │                               └──▶ Emit progress event to TUI
     │
     ▼
User sees live progress, can cancel with Esc
     │
     ▼
Mark scan job complete
```

**Performance:**

| Operation | Latency |
|-----------|---------|
| Drill-down (indexed query) | 1-5ms |
| Source switch | <5ms |
| Memory per source | <1 MB |

### 13.3 State Machine

The Glob Explorer uses a 6-state machine organized into two layers:
- **Navigation Layer**: Browse and Filtering states for exploring files
- **Rule Editing Layer**: EditRule, Testing, Publishing, Published states for extraction rule creation

```
+-----------------------------------------------------------------------------------+
|                          GLOB EXPLORER STATE MACHINE                               |
+-----------------------------------------------------------------------------------+
|                                                                                    |
|  +--------------------------- NAVIGATION LAYER -----------------------------+     |
|  |                                                                           |     |
|  |   +--------------+    l/Enter     +--------------+                        |     |
|  |   |    BROWSE    |--------------->|    BROWSE    |                        |     |
|  |   |   (at root)  |                |  (in folder) |                        |     |
|  |   |              |<---------------|              |                        |     |
|  |   +------+-------+   h/Backspace  +------+-------+                        |     |
|  |          |                               |                                |     |
|  |          | / (start typing)              | / (start typing)               |     |
|  |          v                               v                                |     |
|  |   +--------------+                +--------------+                        |     |
|  |   |  FILTERING   |                |  FILTERING   |                        |     |
|  |   |  (heat map)  |                |  (in folder) |                        |     |
|  |   |              |<-------------->|              |                        |     |
|  |   +------+-------+   l/Enter, h   +------+-------+                        |     |
|  |          |                               |                                |     |
|  |          | Esc (clear pattern, stay in BROWSE)                            |     |
|  |          v                               |                                |     |
|  |   [Return to BROWSE at current prefix]   |                                |     |
|  |                                          |                                |     |
|  +------------------------------------------+--------------------------------+     |
|                                              |                                     |
|             e (with matches > 0)             | e (with matches > 0)               |
|                       |                      |                                     |
|                       +----------+-----------+                                     |
|                                  v                                                 |
|  +--------------------------- RULE EDITING LAYER ----------------------------+    |
|  |                                                                            |    |
|  |   +------------------------------------------------------------------+     |    |
|  |   |                         EDIT_RULE                                 |     |    |
|  |   |   Glob pattern | Fields | Base tag | Conditions                   |     |    |
|  |   |   (Tab cycles sections, j/k navigates within)                     |     |    |
|  |   +-------------------------------+----------------------------------+     |    |
|  |                                   |                                        |    |
|  |         +-----------+-------------+-------------+-----------+              |    |
|  |         |           |                           |           |              |    |
|  |         | t (test)  | Esc (cancel)              |           |              |    |
|  |         v           v                           |           |              |    |
|  |   +--------------+  [Return to BROWSE]          |           |              |    |
|  |   |   TESTING    |  (preserves prefix)          |           |              |    |
|  |   | +----------+ |                              |           |              |    |
|  |   | | Running  | |                              |           |              |    |
|  |   | +----+-----+ |                              |           |              |    |
|  |   |      | auto  |                              |           |              |    |
|  |   |      v       |                              |           |              |    |
|  |   | +----------+ |                              |           |              |    |
|  |   | | Complete | |                              |           |              |    |
|  |   | +----+-----+ |                              |           |              |    |
|  |   +------+-------+                              |           |              |    |
|  |          |                                      |           |              |    |
|  |          | p (publish)    e (edit)   Esc        |           |              |    |
|  |          |                   |         |        |           |              |    |
|  |          |                   +---------+--------+           |              |    |
|  |          |                             |                    |              |    |
|  |          |                             v                    |              |    |
|  |          |                    [Back to EDIT_RULE]           |              |    |
|  |          |                    (draft preserved)             |              |    |
|  |          v                                                  |              |    |
|  |   +----------------+                                        |              |    |
|  |   |   PUBLISHING   |                                        |              |    |
|  |   | +-----------+  |                                        |              |    |
|  |   | | Confirming|--+-- Esc ---------------------------------+              |    |
|  |   | +-----+-----+  |  (back to EditRule)                                   |    |
|  |   |       |        |                                                       |    |
|  |   |       | Enter (confirm)                                                |    |
|  |   |       v        |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   | | Saving    |  |                                                       |    |
|  |   | +-----+-----+  |                                                       |    |
|  |   |       | auto   |                                                       |    |
|  |   |       v        |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   | | Starting  |  |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   +-------+--------+                                                       |    |
|  |           |                                                                |    |
|  |           | (auto-transition on success)                                   |    |
|  |           v                                                                |    |
|  |   +----------------+                                                       |    |
|  |   |   PUBLISHED    |                                                       |    |
|  |   |   Complete!    |                                                       |    |
|  |   |   Job ID: xxx  |                                                       |    |
|  |   +-------+--------+                                                       |    |
|  |           |                                                                |    |
|  |           +-- Enter/Esc --> [Return to BROWSE at root]                     |    |
|  |           |                                                                |    |
|  |           +-- j ----------> [View Job Status screen]                       |    |
|  |                                                                            |    |
|  +----------------------------------------------------------------------------+    |
|                                                                                    |
|   g/Esc from BROWSE/FILTERING --> Exit Glob Explorer (return to Discover)         |
|                                                                                    |
+------------------------------------------------------------------------------------+
```

**State Definitions:**

| State | Entry Condition | Exit Conditions | Preserves Context |
|-------|-----------------|-----------------|-------------------|
| `Browse` | Default, Esc from Filtering, Enter/Esc from Published | `l`/Enter -> drill, `/` -> Filtering, `e` -> EditRule (DISABLED, no pattern), `g`/Esc -> exit | prefix: Yes |
| `Filtering` | `/` from Browse | Esc -> Browse, `l` -> drill, `e` -> EditRule (when matches > 0) | prefix: Yes, pattern: Yes |
| `EditRule` | `e` from Filtering (when matches > 0), `e` from Testing, Esc from Publishing | `t` -> Testing, Esc -> Browse | prefix: Yes, pattern: as glob, rule draft: Yes |
| `Testing` | `t` from EditRule | `p` -> Publishing, `e` -> EditRule, Esc -> EditRule | rule draft: Yes |
| `Publishing` | `p` from Testing (Complete) | Enter -> Saving (then auto -> Published), Esc -> EditRule | rule draft: Yes |
| `Published` | auto from Publishing (success) | Enter/Esc -> Browse (root), `j` -> Job Status screen | None (clean slate) |

**Progressive Reveal (Filtering state):**

| Match Count | Display |
|-------------|---------|
| ≥200 matches | Heat map only - folders with density bars |
| <200 matches | Heat map + flat results list below folders |

**Transition Table:**

| From State | Key/Trigger | To State | Condition | Notes |
|------------|-------------|----------|-----------|-------|
| Browse | `l` / Enter | Browse (deeper) | folder selected | Drill into folder |
| Browse | `h` / Backspace | Browse (parent) | not at root | Go up one level |
| Browse | `/` | Filtering | any | Start pattern typing |
| Browse | `e` | (disabled) | no pattern | Show hint: "Press / to filter first" |
| Browse | `g` / Esc | Exit | any | Return to Discover view |
| Filtering | `l` / Enter | Filtering (deeper) | folder selected | Drill preserving pattern |
| Filtering | `h` | Filtering (parent) | not at root | Go up preserving pattern |
| Filtering | `e` | EditRule | matches > 0 | Pre-fill glob from pattern |
| Filtering | `e` | (disabled) | matches = 0 | Nothing to extract |
| Filtering | Esc | Browse | any | Clear pattern, stay at prefix |
| Filtering | `g` | Exit | any | Return to Discover view |
| EditRule | `t` | Testing | rule valid | Start test run |
| EditRule | Esc | Browse | any | Cancel rule, preserve prefix |
| EditRule | Tab | EditRule | any | Cycle sections |
| EditRule | j/k | EditRule | any | Navigate within section |
| Testing | `p` | Publishing | sub-state = Complete | Begin publish flow |
| Testing | `e` | EditRule | any | Return to edit, draft preserved |
| Testing | Esc | EditRule | any | Cancel test, draft preserved |
| Publishing (Confirming) | Enter | Publishing (Saving) | any | User confirms publish |
| Publishing (Confirming) | Esc | EditRule | any | Cancel publish, draft preserved |
| Publishing (Saving) | (auto) | Publishing (Starting) | save success | Auto-transition |
| Publishing (Starting) | (auto) | Published | job started | Auto-transition |
| Published | Enter | Browse (root) | any | Complete, fresh start |
| Published | Esc | Browse (root) | any | Complete, fresh start |
| Published | `j` | Job Status | any | View job details |

### 13.4 BROWSE State Layout (No Pattern)

```
Pattern: <no filter>  [1191603 files]
────────────────────────────────────────────────────────────────────────────────
FOLDERS (5)
  📁 logs                                                      50,000 files >
  📁 data                                                      30,000 files >
  📁 archive                                                  100,000 files >
  📁 temp                                                       1,000 files >
  📄 README.md                                                      1 files

                         PREVIEW ───────────────────────────────────────────────
                          logs/app.log (1.2 MB)
                          logs/errors/crash.log (892 KB)
                          data/users.csv (2.1 MB)

────────────────────────────────────────────────────────────────────────────────
[hjkl] Navigate  [l/Enter] Drill in  [h/Bksp] Back  [/] Filter  [g/Esc] Exit
```

**Key Behaviors:**

- **Instant navigation**: O(1) HashMap lookup from preloaded cache
- **Folder indicators**: `>` shows folder can be drilled into
- **File preview**: Shows sample files from current folder
- **vim-style keys**: `h`/`l` for back/forward, `j`/`k` for up/down

### 13.5 FILTERING State - Heat Map (Many Matches)

When a glob pattern is entered and matches ≥200 files, show heat map with density bars.

**Density Bar Characters:**
```
▓ = filled (has matches in this subtree)
▒ = empty (no matches in this portion of bar)
```

Bar width: 24 characters, proportional to max matches at current level.

```
Pattern: **/*.csv  [2,847 matches]
────────────────────────────────────────────────────────────────────────────────
FOLDERS (5)                                                             MATCHES
  📁 data          ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓   2,341           30,000 files >
  📁 archive       ▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒     506          100,000 files >
  📁 logs          ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒       0           50,000 files >
  📁 temp          ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒       0            1,000 files >
  📄 config.yaml   ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒       -                1 files

────────────────────────────────────────────────────────────────────────────────
[hjkl] Navigate  [l/Enter] Drill  [h] Back  [Esc] Clear pattern  [g] Exit
```

**Visual treatment:**
- **Selection indicator (►)**: Shows which item is currently selected
- **Bright/highlighted row**: Folders with matches > 0
- **Dimmed row**: Folders with 0 matches (still navigable)
- **MATCHES column**: Count of matching files in that subtree
- **Background highlight**: Selected item has dark gray background when focused

### 13.6 FILTERING State - Heat Map + Flat Results (Few Matches)

When matches drop below threshold (~200), show both heat map AND flat file list.

```
Pattern: **/*.csv  (in data/exports/quarterly/)  [48 matches]
────────────────────────────────────────────────────────────────────────────────
FOLDERS (2)                                                             MATCHES
  📁 2023          ▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒      24               50 files >
  📁 2024          ▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒      24               50 files >

MATCHES (48)                                                    [showing 1-12]
  q1_summary.csv
  q2_summary.csv
  q3_summary.csv
  q4_summary.csv
  2023/jan.csv
  2023/feb.csv
  2023/mar.csv
  2023/apr.csv
  2023/may.csv
  2023/jun.csv
  2024/jan.csv
  2024/feb.csv

────────────────────────────────────────────────────────────────────────────────
[Tab] Switch focus (folders/matches)  [j/k] Scroll  [Enter] Select  [h] Back
```

**Key behaviors:**
- `Tab` switches focus between FOLDERS and MATCHES sections
- When focused on MATCHES, `j`/`k` scrolls through files
- `Enter` on a file selects it for tagging/preview

### 13.7 Scan In Progress State

When a source is still being scanned, folder navigation is disabled.

```
┌ [1] Source ──────────────────────┐
│ ⟳ shan (scanning...)             │
│   1,048,576 files discovered     │
│   Building folder cache...       │
└──────────────────────────────────┘

   Scan in progress. Folder navigation
   will be available when scan completes.

   [Esc] Cancel scan
```

**Behavior:**
- No folder drilling until scan completes
- Progress shows files discovered count
- Cache is built as final step of scan
- On completion, cache loads instantly (<50ms)

---

### 13.8 Extraction Pattern Syntax

Casparian uses an **extended glob syntax** with `<field>` placeholders for inline field extraction.
This is a **superset of standard glob** - all valid globs work, plus extraction markers.

#### Syntax

| Syntax | Meaning | Example |
|--------|---------|---------|
| `*` | Match any characters (not `/`) | `*.csv` |
| `**` | Match any path (including `/`) | `**/*.csv` |
| `?` | Match single character | `file?.txt` |
| `{a,b}` | Alternation (standard glob) | `{src,lib}/*.rs` |
| `<field>` | **Capture as field** | `mission_<id>/*.csv` |
| `<field:type>` | **Capture with type hint** | `<date:date>/*.csv` |

#### Examples

```
# Simple: capture mission_id from folder name
**/mission_<mission_id>/**/*.csv
    └───────┬────────┘
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

#### How It Works

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

#### Type Inference

When no type hint is provided, types are inferred from values:

| Values | Inferred Type |
|--------|---------------|
| `042`, `043`, `100` | integer |
| `2024-01-15`, `2024-02-01` | date |
| `abc123`, `CLIENT-A` | string |
| `550e8400-e29b-...` | uuid |

---

### 13.9 CREATE RULE Dialog (Unified)

> **Design:** One dialog for everything. Live preview replaces the need for a separate
> Test step in most cases. Full Test available via `[t]` for detailed field metrics.

```
┌─ CREATE RULE ──────────────────────────────────────────────────────────────────┐
│                                                                                │
│  PATTERN                                                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │ **/mission_<mission_id>/<date>/*.csv                         [847 files] │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  EXTRACTED FIELDS (auto-detected from pattern)                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  mission_id : integer  (inferred: 042, 043, 044...)                      │ │
│  │  date       : date     (inferred: 2024-01-15, 2024-02-01...)             │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  TAG                                                                           │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │ mission_data                                                             │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  OPTIONS                                                                       │
│  ► [x] Enable rule                  ← selection indicator when focused         │
│    [x] Run extraction job immediately                                          │
│                                                                                │
├─ LIVE PREVIEW ─────────────────────────────────────────────────────────────────┤
│                                                                                │
│  /data/mission_042/2024-01-15/telemetry.csv                                   │
│    → mission_id: 42, date: 2024-01-15                                         │
│                                                                                │
│  /data/mission_043/2024-02-01/readings.csv                                    │
│    → mission_id: 43, date: 2024-02-01                                         │
│                                                                                │
│  /data/mission_044/2024-02-10/sensor.csv                                      │
│    → mission_id: 44, date: 2024-02-10                                         │
│                                                                                │
│  ... and 844 more files                                                        │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [Enter] Save   [t] Full Test (metrics)   [Tab] Next field   [Esc] Cancel      │
└────────────────────────────────────────────────────────────────────────────────┘
```

#### Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Move between Pattern, Tag, Options |
| `Enter` | **Save rule immediately** |
| `t` | Open Full Test view (detailed metrics, histograms) |
| `Esc` | Cancel and return to browse |

#### Live Preview Behavior

- Updates in real-time as you type the pattern (debounced 200ms)
- Shows first 5 matching files with extracted values
- Highlights extraction errors in red
- Shows total match count in pattern field `[847 files]`

#### Auto-Detection

When you type `<field>` placeholders:
1. Fields appear automatically in EXTRACTED FIELDS section
2. Type inference runs on sample values
3. No manual field configuration needed for simple cases

---

### 13.10 FULL TEST View (Optional)

For complex rules, `[t]` opens detailed test results with field metrics.
This runs extraction on ALL matching files **without persisting**.

```
┌─ FULL TEST RESULTS ────────────────────────────────────────────────────────────┐
│                                                                                │
│  Rule: mission_data                                                            │
│  Pattern: **/mission_<mission_id>/<date>/*.csv                                │
│  Files tested: 847                                                             │
│                                                                                │
├─ EXTRACTION STATUS ────────────────────────────────────────────────────────────┤
│                                                                                │
│  ✓ Complete: 812 files (95.9%)                                                │
│  ⚠ Partial:   28 files (3.3%)    [Enter to inspect]                           │
│  ✗ Failed:     7 files (0.8%)    [Enter to inspect]                           │
│                                                                                │
├─ FIELD METRICS ────────────────────────────────────────────────────────────────┤
│                                                                                │
│  FIELD: mission_id                         FIELD: date                         │
│  ─────────────────────────                 ─────────────────────────           │
│  042 ████████████████████ 423              2024-01 ██████████████ 312          │
│  043 ████████████░░░░░░░░ 312              2024-02 ████████░░░░░░ 247          │
│  044 ████████░░░░░░░░░░░░ 112              2023-12 █████░░░░░░░░░ 189          │
│                                            2023-11 ███░░░░░░░░░░░  99          │
│  3 unique values                           4 unique months                     │
│  Range: 042 - 044                          Range: 2023-11 → 2024-02            │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [Enter] Save rule   [e] Edit   [↑↓] Scroll   [v] Value drill-down   [Esc] Back│
└────────────────────────────────────────────────────────────────────────────────┘
```

**When to use Full Test:**
- Complex patterns with multiple `<field>` captures
- Want to see value distributions before committing
- Debugging extraction failures
- Verifying type inference is correct

**Field Metrics Features:**

| Feature | Description |
|---------|-------------|
| Value distribution | Histogram of top values per field |
| Unique count | Number of distinct values |
| Range | Min/Max for numeric and date fields |
| Drill-down | Press `v` on a field to see all values |

---

### 13.8 After Save

```
┌─ PUBLISH COMPLETE ─────────────────────────────────────────────────────────────┐
│                                                                                │
│  ✓ Rule "Mission Telemetry" published                                         │
│                                                                                │
│  Background job started:                                                       │
│    Job ID: cf_extract_a7b3c9d2                                                │
│    Files: 847                                                                  │
│    Status: RUNNING                                                             │
│                                                                                │
│  View progress: casparian jobs status cf_extract_a7b3c9d2                     │
│                                                                                │
│  [Enter] Return to explorer   [j] View job status                             │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 13.11 Glob Explorer Data Model

> **DEPRECATED (v3.1):** The `FolderCache` struct and `.bin.zst` files are deprecated.
> Folder hierarchy is now stored in `scout_folders` SQLite table with on-demand loading.
> See specs/streaming_scanner.md for the new architecture.

```rust
/// [DEPRECATED] Folder cache - replaced by scout_folders table
/// This struct exists only for migration from old .bin.zst files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCache {
    pub segments: Vec<String>,
    pub nodes: Vec<FolderNode>,
    pub root_children: Vec<u16>,
    pub total_files: usize,
    pub built_at: String,
}

/// [DEPRECATED] Node in old trie structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderNode {
    pub segment_idx: u16,
    pub children: Vec<u16>,
    pub file_count: u32,
    pub is_file: bool,
}

/// Entry returned from scout_folders table query
pub struct FolderEntry {
    pub name: String,
    pub file_count: i64,
    pub is_folder: bool,
}

/// Glob Explorer state (extends DiscoverState)
pub struct GlobExplorerState {
    // --- Input state (what user requested) ---
    pub pattern: String,                      // Current glob pattern (e.g., "*.csv", "**/*.json")
    pub nav_history: Vec<(String, String)>,   // History of (prefix, pattern) for back navigation
    pub current_prefix: String,               // Current path prefix (empty = root)

    // --- Derived state (queried on-demand from scout_folders) ---
    pub current_items: Vec<FolderEntry>,      // Children at current prefix (from SQLite)
    pub preview_files: Vec<GlobPreviewFile>,  // Sampled preview files (max 10)
    pub total_count: GlobFileCount,           // Total file count for current prefix + pattern

    // --- On-demand Loading State ---
    pub loading: bool,                        // True while querying scout_folders
    pub scan_in_progress: bool,               // True if source is being scanned
    pub minutes_since_scan: Option<f64>,      // For staleness indicator

    // --- UI state ---
    pub selected_folder: usize,               // Currently selected folder index
    pub phase: GlobExplorerPhase,             // Current phase in state machine

    // --- Debouncing state (performance optimization) ---
    pub pattern_changed_at: Option<Instant>,  // When pattern was last modified
    pub last_searched_pattern: String,        // Last pattern that was searched
    pub last_searched_prefix: String,         // Last prefix that was searched
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum GlobExplorerPhase {
    #[default]
    Explore,     // Browsing/filtering folder hierarchy
    Focused,     // Drilled into a specific folder
}

/// Display info for a folder/file in the current view
#[derive(Debug, Clone)]
pub struct FolderInfo {
    pub name: String,                    // Display name (may include pattern suffix)
    pub path: Option<String>,            // Navigation path (for ** results)
    pub file_count: usize,               // Files in this folder/subtree
    pub is_file: bool,                   // true = file, false = folder
}

impl FolderInfo {
    /// Create a new folder/file info
    pub fn new(name: String, file_count: usize, is_file: bool) -> Self;
    /// Create with explicit navigation path
    pub fn with_path(name: String, path: Option<String>, file_count: usize, is_file: bool) -> Self;
    /// Create a loading placeholder
    pub fn loading(message: &str) -> Self;
    /// Create from cache entry
    pub fn from_cache_entry(name: &str, file_count: usize, is_file: bool) -> Self;
}

/// Preview file for Glob Explorer
#[derive(Debug, Clone)]
pub struct GlobPreviewFile {
    pub rel_path: String,
    pub size: u64,
    pub mtime: i64,
}

/// File count (exact or estimated for large sources)
#[derive(Debug, Clone)]
pub enum GlobFileCount {
    Exact(usize),
    Estimated(usize),
}

#[derive(Debug, Clone)]
pub struct SemanticCluster {
    pub name: String,                    // e.g., "mission_data/"
    pub semantic_pattern: String,        // e.g., "entity_folder(mission) > dated"
    pub example_path: String,            // Sample path for preview
    pub file_count: usize,
    pub suggested_glob: String,          // Pattern if user selects this cluster
}

#[derive(Debug, Clone)]
pub enum FileCount {
    Exact(usize),
    Sampled { estimate: usize, sample_size: usize },
}

#[derive(Debug, Clone)]
pub struct RuleDraft {
    pub name: String,
    pub glob_pattern: String,
    pub fields: Vec<FieldDraft>,
    pub base_tag: String,
    pub tag_conditions: Vec<TagCondition>,
}

#[derive(Debug, Clone)]
pub struct FieldDraft {
    pub name: String,
    pub source: FieldSource,
    pub pattern: Option<String>,
    pub type_hint: FieldType,
}

#[derive(Debug, Clone)]
pub enum FieldSource {
    Segment(i32),      // segment(-2)
    Filename,
    FullPath,
    RelPath,
}

#[derive(Debug, Clone)]
pub struct TagCondition {
    pub field: String,
    pub operator: CompareOp,
    pub value: String,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct TestResults {
    pub total_files: usize,
    pub complete: usize,
    pub partial: usize,
    pub failed: usize,
    pub field_metrics: HashMap<String, FieldMetrics>,
    pub tag_counts: HashMap<String, usize>,
    pub sample_extractions: Vec<SampleExtraction>,
}

#[derive(Debug, Clone)]
pub struct FieldMetrics {
    pub unique_count: usize,
    pub top_values: Vec<(String, usize)>,  // (value, count)
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SampleExtraction {
    pub path: String,
    pub fields: HashMap<String, String>,
    pub tags: Vec<String>,
    pub status: ExtractionStatus,
}
```

### 13.12 Glob Explorer Keybindings

#### 13.12.1 BROWSE State (No Pattern)

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down in folder list |
| `k` / `↑` | Move up in folder list |
| `l` / `→` / `Enter` | Drill into selected folder |
| `h` / `←` / `Backspace` | Go back to parent folder |
| `/` | Enter pattern mode (start typing glob) |
| `g` / `Esc` | Exit Glob Explorer |

#### 13.12.2 FILTERING State (With Pattern)

| Key | Action |
|-----|--------|
| `j` / `↓` | Navigate folders (or matches if Tab-focused) |
| `k` / `↑` | Navigate folders (or matches if Tab-focused) |
| `l` / `Enter` | Drill into folder OR select file |
| `h` / `Backspace` | Go back to parent folder |
| `Tab` | Switch focus between FOLDERS and MATCHES (when <200 matches) |
| `Char(c)` | Append to pattern |
| `Esc` | Clear pattern, return to BROWSE |
| `g` | Exit Glob Explorer |

#### 13.12.3 Pattern Editing

| Key | Action |
|-----|--------|
| `Char(c)` | Append character to pattern |
| `Backspace` | Remove character from pattern (if pattern not empty) |
| `Enter` | Confirm pattern, focus on results |
| `Esc` | Cancel pattern, return to BROWSE |

#### 13.12.4 EDIT RULE State

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: Pattern → Fields → Tagging |
| `Enter` | Edit selected item |
| `+` / `a` | Add field or condition |
| `d` | Delete selected item |
| `↑` / `↓` | Navigate within section |
| `t` | Test rule |
| `Esc` | Cancel, return to browse |

#### 13.12.5 TEST State

| Key | Action |
|-----|--------|
| `p` | Publish rule |
| `e` | Edit rule (return to Edit) |
| `↑` / `↓` | Scroll results |
| `Enter` | Inspect selected file |
| `f` | Filter by extraction status |
| `v` | Value drill-down (see all values for field) |
| `Esc` | Cancel, return to Focused |

#### 13.9.5 PUBLISH State

| Key | Action |
|-----|--------|
| `Enter` | Confirm publish |
| `j` | View job status (after publish) |
| `Esc` | Cancel, return to Test |

### 13.10 Semantic Clustering Algorithm

Files are clustered by detected semantic structure:

```rust
pub fn cluster_files(files: &[FileInfo]) -> Vec<SemanticCluster> {
    let mut structure_map: HashMap<String, Vec<&FileInfo>> = HashMap::new();

    for file in files {
        // Analyze path structure
        let structure = analyze_semantic_structure(&file.path);
        let key = structure.to_fingerprint();

        structure_map.entry(key).or_default().push(file);
    }

    // Convert to clusters, sorted by file count
    let mut clusters: Vec<_> = structure_map.into_iter()
        .map(|(fingerprint, files)| {
            let example = files[0];
            let structure = SemanticStructure::from_fingerprint(&fingerprint);

            SemanticCluster {
                name: derive_cluster_name(&structure, example),
                semantic_pattern: structure.to_display_string(),
                example_path: example.path.clone(),
                file_count: files.len(),
                suggested_glob: structure.to_glob_pattern(),
            }
        })
        .collect();

    clusters.sort_by(|a, b| b.file_count.cmp(&a.file_count));
    clusters
}

/// Semantic primitives detected from path analysis
#[derive(Debug, Clone)]
pub enum SemanticPrimitive {
    EntityFolder { name: String, id_pattern: String },
    DatedHierarchy { format: DateFormat },
    DirectionMarker { direction: String },
    CategoryFolder { values: Vec<String> },
    Flat,
}

#[derive(Debug, Clone)]
pub enum DateFormat {
    Iso,        // 2024-01-15
    Nested,     // 2024/01/15
    Quarter,    // 2024/Q1
}
```

### 13.13 Implementation Phases (Glob Explorer)

#### Phase 12: Streaming Scanner (P0)
> **Spec:** See `specs/streaming_scanner.md` for full architecture.

- [ ] Create `ScanStream` struct with progress channel
- [ ] Modify `parallel_walk()` to send batches via channel
- [ ] Add cancel flag to walker threads (`Arc<AtomicBool>`)
- [ ] Implement `persist_task` that writes batches as they arrive
- [ ] Memory: O(batch_size) not O(file_count)

#### Phase 13: SQLite Folder Table (P1)
- [ ] Add `scout_folders` table to schema
- [ ] Implement `update_folder_counts()` during persist (UPSERT ancestors)
- [ ] Implement `get_folder_children()` query function
- [ ] Remove `FolderCache` file-based serialization
- [ ] Clean up old `.bin.zst` cache files on startup

#### Phase 14: TUI Integration
- [ ] On source selection, query `scout_folders` for root entries
- [ ] If scan in progress, show progress UI with cancel option
- [ ] Populate folder list from query result (not preloaded cache)

#### Phase 14b: Folder Navigation (BROWSE State)
- [ ] Implement folder drilling via `get_folder_children()` query (<5ms)
- [ ] vim-style keybindings: `hjkl`, `l`/Enter, `h`/Backspace
- [ ] Track `current_prefix` for navigation state
- [ ] Show folder/file counts from query result

#### Phase 15: Glob Matching (FILTERING State)
- [ ] Integrate `globset` crate for pattern parsing
- [ ] Implement trie traversal for `**/*.csv` patterns
- [ ] Compute match counts per folder (subtree sums)
- [ ] Cache pattern results for re-use while drilling

#### Phase 16: Heat Map UI
- [ ] Add MATCHES column to folder list
- [ ] Render density bars with ▓/▒ characters (24 char width)
- [ ] Highlight rows with matches > 0, dim rows with 0 matches
- [ ] Scale bar proportionally to max matches at current level

#### Phase 17: Flat Results Below Threshold
- [ ] Detect when match count < 200 (configurable threshold)
- [ ] Show MATCHES section below FOLDERS
- [ ] Implement scrolling within matches list
- [ ] Tab to switch focus between folders/matches
- [ ] Enter on match = select file for preview/tagging

#### Phase 18: Extraction API Integration (Detailed)

This phase connects the Glob Explorer TUI to the Extraction API (`specs/extraction.md`).
The goal is to provide a seamless workflow: browse files → define extraction rules → test → publish.

##### Phase 18a: State Machine Extension

> **Full state diagram**: See Section 13.3 for the unified 6-state machine.

- [ ] Extend `GlobExplorerPhase` enum with new states:
  ```rust
  pub enum GlobExplorerPhase {
      Browse,       // Navigation: root browsing
      Filtering,    // Navigation: browsing with active glob pattern
      EditRule,     // Rule editing: defining extraction rule
      Testing,      // Rule editing: running test extraction
      Publishing,   // Rule editing: creating rule + background job
      Published,    // Rule editing: showing completion status
  }
  ```
- [ ] Add state transitions (see Section 13.3 for complete table):
  - **Entry to Rule Editing Layer**:
    - `Filtering` → `EditRule`: Press `e` (requires matches > 0)
    - `Browse` → `EditRule`: `e` key disabled (show hint: "Press / to filter first")
  - **Within Rule Editing Layer**:
    - `EditRule` → `Testing`: Press `t` to test extraction
    - `EditRule` → `Browse`: Press `Esc` to cancel (preserves prefix)
    - `Testing` → `Publishing`: Press `p` to publish (requires Complete sub-state)
    - `Testing` → `EditRule`: Press `e` or `Esc` (draft preserved)
    - `Publishing` → `EditRule`: Press `Esc` from Confirming sub-state
    - `Publishing (Confirming)` → `Publishing (Saving)`: Press `Enter` to confirm
    - `Publishing (Saving)` → `Publishing (Starting)`: Auto on success
    - `Publishing (Starting)` → `Published`: Auto on success
  - **Exit from Rule Editing Layer**:
    - `Published` → `Browse (root)`: Press `Enter` or `Esc` (clean slate)
    - `Published` → `Job Status`: Press `j` (view job details)

##### Phase 18b: EDIT RULE State Implementation
- [ ] Create `RuleEditorState` struct:
  ```rust
  pub struct RuleEditorState {
      pub rule: RuleDraft,
      pub focus: RuleEditorFocus,        // Glob, Fields, Tag, Conditions
      pub selected_field: usize,
      pub editing_field: Option<usize>,  // Which field is being edited
      pub match_count: usize,            // Live count of matching files
      pub inferred_fields: Vec<FieldDraft>,  // Auto-detected fields
  }

  pub enum RuleEditorFocus {
      GlobPattern,
      FieldList,
      FieldEdit(FieldEditFocus),
      BaseTag,
      Conditions,
  }

  pub enum FieldEditFocus {
      Name,
      Source,
      Pattern,
      Type,
  }
  ```

- [ ] Create `RuleDraft` and supporting types (aligned with DB schema from extraction.md):
  ```rust
  /// TUI working draft - editable in UI
  #[derive(Debug, Clone)]
  pub struct RuleDraft {
      pub id: Option<Uuid>,         // None for new rules, Some for editing existing
      pub source_id: Option<Uuid>,  // Scoped to source, or None for global
      pub name: String,
      pub glob_pattern: String,
      pub fields: Vec<FieldDraft>,
      pub base_tag: Option<String>, // Optional base tag
      pub tag_conditions: Vec<TagConditionDraft>,
      pub priority: i32,            // Default: 100
      pub enabled: bool,            // Default: true
  }

  #[derive(Debug, Clone)]
  pub struct FieldDraft {
      pub name: String,
      pub source: FieldSource,
      pub pattern: Option<String>,  // Regex for extraction
      pub type_hint: FieldType,
      pub normalizer: Option<Normalizer>,
      pub default_value: Option<String>,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum FieldSource {
      Segment(i32),    // segment(-2) -> Segment(-2)
      Filename,        // "filename"
      FullPath,        // "full_path"
      RelPath,         // "rel_path"
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum FieldType {
      String,
      Integer,
      Date,
      Uuid,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum Normalizer {
      Lowercase,
      Uppercase,
      StripLeadingZeros,
  }

  #[derive(Debug, Clone)]
  pub struct TagConditionDraft {
      pub field: String,
      pub operator: CompareOp,
      pub value: String,
      pub tag: String,
      pub priority: i32,  // Default: 100
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum CompareOp {
      Eq,        // =
      NotEq,     // !=
      Lt,        // <
      Gt,        // >
      LtEq,      // <=
      GtEq,      // >=
      Contains,  // contains
      Matches,   // matches (regex)
  }
  ```

  **DB Schema Alignment**: These types map directly to the `extraction_rules`, `extraction_fields`, and `extraction_tag_conditions` tables defined in Phase 18f.

- [ ] Implement field inference from glob pattern:
  - Detect date segments: `2024-01-15` → `date: Date`
  - Detect entity prefixes: `mission_042` → `mission_id: Integer`
  - Detect categories: `Inbound/Outbound` → `direction: String`
- [ ] Implement live match count updates (debounced 200ms)
- [ ] Keybindings for EDIT RULE:
  | Key | Action |
  |-----|--------|
  | `Tab` | Next section (Glob → Fields → Tag → Conditions) |
  | `Shift+Tab` | Previous section |
  | `j`/`k` | Navigate within section |
  | `Enter` | Edit selected item |
  | `+` / `a` | Add field / Add condition |
  | `d` | Delete selected field/condition |
  | `t` | Test rule |
  | `Esc` | Cancel (return to browse) |

##### Phase 18c: Field Inference Engine

**Sample Source**: Files matching the current glob pattern from the folder cache.

**Sampling Strategy Configuration**:
```rust
pub struct FieldInferenceConfig {
    /// Maximum samples to analyze (performance bound)
    pub max_samples: usize,           // Default: 100
    /// Minimum samples needed for reliable inference
    pub min_samples: usize,           // Default: 3
    /// Sampling strategy
    pub strategy: SamplingStrategy,
}

pub enum SamplingStrategy {
    /// Take first N matches (fast, may miss edge cases)
    FirstN,
    /// Random sample across matches (better coverage)
    Random,
    /// Stratified by segment values (best coverage) - DEFAULT
    Stratified,
}
```

**Sampling Rules**:
- Use `SamplingStrategy::Stratified` by default for better edge case coverage
- Maximum 100 samples for real-time UI responsiveness (<50ms inference time)
- Minimum 3 samples required; show warning if fewer matches exist
- When total matches <= 100, use all files (no sampling)

**UI Feedback When Sampling**:
```
INFERRED FIELDS (from 100 of 47,293 files):
  mission_id (high) - 23 unique values in sample
  date (high) - ISO date format detected

  [ ] Show all 47,293 matches   [Sampling: stratified]
```

- [ ] Create `infer_fields_from_pattern(pattern: &str, sample_paths: &[&str]) -> Vec<FieldDraft>`:
  - Parse pattern segments
  - For each variable segment (`*`, `**`, `{name}`):
    - Sample values from `sample_paths` using stratified sampling
    - Detect type (date, integer, uuid, string)
    - Suggest field name from position or pattern
- [ ] Implement pattern primitives detection (from extraction.md Appendix B):
  | Pattern | Detection | Field |
  |---------|-----------|-------|
  | `????-??-??` | ISO date regex | `date: Date` |
  | `????/??/??` | Nested date | `year, month, day: Integer` |
  | `mission_*` | Entity prefix | `mission_id: String` |
  | `Q?` | Quarter | `quarter: Integer` |
  | `*_Inbound` | Direction suffix | `direction: String` |

**Confidence Levels and Thresholds**:

| Level | Score Range | Visual | Description |
|-------|-------------|--------|-------------|
| HIGH | >= 0.85 | `++` / green | High certainty inference |
| MEDIUM | 0.50 - 0.84 | `??` / yellow | Probable but verify |
| LOW | < 0.50 | `??` / gray | Uncertain, may be wrong |

**Confidence Calculation**:
```rust
pub struct InferenceConfidence {
    pub score: f64,           // 0.0 - 1.0
    pub level: ConfidenceLevel,
    pub factors: Vec<ConfidenceFactor>,
}

pub enum ConfidenceLevel {
    High,    // >= 0.85
    Medium,  // 0.50 - 0.84
    Low,     // < 0.50
}

pub enum ConfidenceFactor {
    /// Pattern segment produces consistent type across samples
    TypeConsistency { ratio: f64 },  // % of samples with same type
    /// Named pattern detected (e.g., mission_*, date_*)
    PatternRecognition { pattern: String },  // Bonus for recognized patterns
    /// Value distribution suggests categorical vs continuous
    ValueDistribution { unique_ratio: f64 },  // unique_values / total_samples
    /// Sample size adequacy
    SampleSize { count: usize, min_required: usize },
}
```

**Scoring Algorithm**:
- Base score: 0.5
- Type consistency: `(ratio - 0.5) * 0.6` (100% = +0.3)
- Pattern recognition: +0.25 for date/iso_date, +0.20 for uuid, +0.15 for quarter/year/month
- Value distribution: +0.10 if unique_ratio < 10% (categorical) or > 90% (ID)
- Sample size penalty: `-0.3 * (min_required - count) / min_required` if insufficient

- [ ] Show inferred fields with confidence indicator:
  ```
  INFERRED FIELDS (from 100 of 47,293 files):

    ++ mission_id (HIGH)
    |    Detected: mission_(\d+) prefix pattern
    |    Type: integer (100% consistent)
    |    Unique: 23 values
    |
    ++ date (HIGH)
    |    Detected: ISO date format (????-??-??)
    |    Type: date (100% consistent)
    |    Range: 2023-11 to 2024-02
    |
    ?? category (MEDIUM)
    |    No pattern detected
    |    Type: string (87% consistent, 13% integer-like)
    |    Unique: 4 values

  Legend: ++ = HIGH (>= 0.85)   ?? = MEDIUM/LOW (< 0.85)
  ```

##### Phase 18d: TEST State Implementation

**Execution Model**: Always asynchronous with cancellation support.

Rationale: Even 100 files with regex extraction can take 500ms+, causing perceptible UI freeze. Always-async is simpler than conditional.

- [ ] Create `TestState` struct:
  ```rust
  pub struct TestState {
      pub rule: RuleDraft,
      pub phase: TestPhase,
      pub results: Option<TestResults>,
      pub selected_category: TestCategory,
      pub scroll_offset: usize,
      /// Cancellation token for running test
      pub cancel_token: Option<Arc<AtomicBool>>,
  }

  pub enum TestPhase {
      /// Test running in background
      Running {
          files_processed: usize,
          files_total: usize,
          current_file: Option<String>,  // Currently processing
          started_at: Instant,
      },
      /// Test completed successfully
      Complete,
      /// Test was cancelled by user
      Cancelled { files_processed: usize },
      /// Test encountered fatal error
      Error(String),
  }

  pub enum TestCategory {
      Summary,
      Complete,
      Partial,
      Failed,
      FieldMetrics,
  }
  ```

**Background Task Architecture**:
- Spawn test as `tokio::spawn` task
- Use `tokio::task::spawn_blocking` for CPU-bound extraction work
- Check `cancel_token.load(Ordering::Relaxed)` at each file boundary
- Send progress via `mpsc::Sender<TestProgress>` channel

**Progress Display (Running state)**:
```
+------------------[ TEST RESULTS ]------------------+
|                                                    |
|  Testing rule: csv_data                            |
|                                                    |
|  Progress: [=============>          ] 67%          |
|  Files:    1,247 / 1,859                           |
|  Current:  /data/mission_042/2024-01-15/sensor.csv |
|  Elapsed:  3.2s                                    |
|                                                    |
|  [Esc] Cancel test                                 |
+----------------------------------------------------+
```

**Cancellation UX**:
- User presses Esc during Running phase
- `cancel_token` set to true
- Background task exits at next file boundary
- Phase transitions to `Cancelled { files_processed }`
- UI shows partial results with "(cancelled)" indicator
- User can press `e` to edit rule or `t` to restart test

- [ ] Implement extraction test runner (non-persisting):
  - Load matching files from glob pattern
  - Run extraction for each file in background
  - Collect field values for metrics
  - Track success/partial/failed counts
- [ ] Implement field metrics aggregation:
  ```rust
  pub struct FieldMetrics {
      pub field_name: String,
      pub unique_count: usize,
      pub value_histogram: Vec<(String, usize)>,  // Top values
      pub min_value: Option<String>,
      pub max_value: Option<String>,
      pub null_count: usize,
  }
  ```

**Histogram Rendering Specification**:
```rust
pub struct HistogramConfig {
    /// Maximum bar width in characters (filled + empty)
    pub bar_width: usize,           // Default: 12
    /// Maximum number of values to show per field
    pub max_values: usize,          // Default: 5
    /// Maximum characters for value label before truncation
    pub max_label_width: usize,     // Default: 15
    /// Character for filled portion of bar
    pub filled_char: char,          // Default: '█'
    /// Character for empty portion of bar
    pub empty_char: char,           // Default: '░'
}
```

**Layout Constants**:
```
Field Column Width: 38 characters (fits two columns in 80-char terminal)

Breakdown:
  Value label:    15 chars max (truncated with "...")
  Space:           1 char
  Bar:            12 chars (filled + empty)
  Space:           1 char
  Count:           6 chars (right-aligned, max 999,999)
  Padding:         3 chars
  Total:          38 chars per column
```

**Proportional Scaling Rules**:
- Calculate `filled = (count / max_count) * bar_width`
- At least 1 filled char for non-zero counts
- Full bar (12 chars) for max count

- [ ] Render histogram bars (proportional to max count):
  ```
  FIELD: mission_id                    │ FIELD: date
  ─────────────────────────────────────│─────────────────────────────────────
  042             ████████████    423  │ 2024-01         ██████████░░    312
  043             ████████░░░░    312  │ 2024-02         ████████░░░░    247
  044             █████░░░░░░░    112  │ 2023-12         ██████░░░░░░    189
                                       │ 2023-11         ███░░░░░░░░░     99
  3 unique values                      │ 4 unique months
  Range: 042 - 044                     │ Range: 2023-11 - 2024-02

  ^              ^            ^     ^
  |              |            |     |
  |              |            |     +-- Count (6 chars, right-aligned)
  |              |            +-- Bar (12 chars: 8 filled + 4 empty)
  |              +-- Space separator
  +-- Value label (15 chars, left-aligned, truncated if needed)
  ```

**Histogram Edge Cases**:
| Scenario | Behavior |
|----------|----------|
| Count = 0 | Empty bar: `░░░░░░░░░░░░` |
| Count = max | Full bar: `████████████` |
| Count very small relative to max | At least 1 filled char |
| Value label empty | Show "(empty)" |
| Value label very long | Truncate: `very_long_va...` |
| Fewer than 5 values | Show all values (no padding) |
| Single field | Left-aligned, no right column |
| Odd number of fields | Last field alone in left column |

- [ ] Keybindings for TEST:
  | Key | Action |
  |-----|--------|
  | `Tab` | Cycle category (Summary → Complete → Partial → Failed → Metrics) |
  | `j`/`k` | Scroll within category |
  | `p` | Publish (from Complete sub-state) |
  | `e` | Return to EDIT RULE (draft preserved) |
  | `v` | View all values for selected field |
  | `Esc` | Cancel test / Return to EDIT RULE (draft preserved) |

##### Phase 18e: PUBLISH State Implementation

**Error Types and Handling**:
```rust
#[derive(Debug, Clone)]
pub enum PublishError {
    /// Database connection failed
    DatabaseConnection(String),
    /// Rule name already exists for this source
    RuleNameConflict {
        existing_rule_id: Uuid,
        existing_created_at: String,
    },
    /// Glob pattern conflicts with existing rule (same pattern, same source)
    PatternConflict {
        existing_rule_id: Uuid,
        existing_rule_name: String,
    },
    /// Database write failed (constraint violation, disk full, etc.)
    DatabaseWrite(String),
    /// Job creation failed (job queue full, invalid state)
    JobCreation(String),
    /// User cancelled during save
    Cancelled,
}

pub enum RecoveryOption {
    /// Retry the failed operation
    Retry,
    /// Edit the rule (e.g., change name)
    EditRule,
    /// Overwrite existing rule (for conflicts)
    Overwrite { existing_id: Uuid },
    /// Cancel and return to browse
    Cancel,
}
```

- [ ] Create `PublishState` struct:
  ```rust
  pub struct PublishState {
      pub rule: RuleDraft,
      pub phase: PublishPhase,
      pub job_id: Option<String>,
  }

  pub enum PublishPhase {
      /// Showing confirmation dialog
      Confirming,
      /// Checking for conflicts
      Validating,
      /// Writing rule to database
      Saving,
      /// Creating background job
      StartingJob,
      /// Successfully published
      Complete { job_id: String },
      /// Error occurred with recovery options
      Error {
          error: PublishError,
          recovery: Vec<RecoveryOption>,
      },
  }
  ```

**Error Flow State Machine**:
```
Confirming
    |
    v (Enter)
Validating -----(conflict found)-----> Error(RuleNameConflict)
    |                                       |
    | (no conflicts)                        v
    v                                  [r] Retry (same name)
Saving ---------(write failed)-------> [e] Edit (change name)
    |                                  [o] Overwrite
    | (success)                        [Esc] Cancel
    v
StartingJob ----(job failed)---------> Error(JobCreation)
    |                                       |
    | (success)                             v
    v                                  [r] Retry
Complete                               [Esc] Cancel (rule saved, no job)
```

**Error Display Examples**:

*Name Conflict Error:*
```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Cannot publish: Rule name already exists                 |
|                                                           |
|  Your rule:                                               |
|    Name: "Mission Telemetry"                              |
|    Pattern: **/mission_*/**/*.csv                         |
|                                                           |
|  Conflicting rule:                                        |
|    Name: "Mission Telemetry" (existing)                   |
|    Created: 2024-01-10 14:23                              |
|    ID: abc123-def456                                      |
|                                                           |
|  Options:                                                 |
|    [e] Edit rule name                                     |
|    [o] Overwrite existing rule                            |
|    [Esc] Cancel                                           |
|                                                           |
+===========================================================+
```

*Job Creation Error (Partial Success):*
```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Partial success: Rule saved, but job creation failed     |
|                                                           |
|  Rule "Mission Telemetry" has been saved to database.     |
|                                                           |
|  Job error:                                               |
|  Failed to create extraction job: Job queue full          |
|                                                           |
|  Options:                                                 |
|    [r] Retry job creation                                 |
|    [Enter] Continue without job (extract later manually)  |
|    [Esc] Cancel                                           |
|                                                           |
|  Note: Rule is saved. You can run extraction later:       |
|  casparian extract --rule "Mission Telemetry"             |
|                                                           |
+===========================================================+
```

- [ ] Implement conflict detection:
  - Check name conflict: `SELECT id FROM extraction_rules WHERE source_id = ? AND name = ?`
  - Check pattern conflict: `SELECT id, name FROM extraction_rules WHERE source_id = ? AND glob_pattern = ?`
- [ ] Implement rule persistence:
  - Insert into `extraction_rules` table
  - Insert fields into `extraction_fields` table
  - Insert conditions into `extraction_tag_conditions` table
- [ ] Implement background job creation:
  - Create job in `cf_job_status` table with type = 'extraction'
  - Job processes matching files, extracts metadata, applies tags
  - Updates `scout_files.metadata_extracted` and `matched_rule_id`
- [ ] Keybindings for PUBLISH:
  | Key | Action |
  |-----|--------|
  | `Enter` | Confirm and start job (from Confirming phase) |
  | `Enter` | Return to explorer (from Complete phase) |
  | `j` | View job status (opens Jobs view) |
  | `Esc` | Cancel (from Confirming phase) / Return to EditRule |
  | `r` | Retry (from Error phase, if Retry option available) |
  | `e` | Edit rule (from Error phase, if EditRule option available) |
  | `o` | Overwrite (from Error phase, if Overwrite option available) |

##### Phase 18f: Database Integration
- [ ] Create extraction tables (from extraction.md Section 6):
  ```sql
  CREATE TABLE extraction_rules (
      id TEXT PRIMARY KEY,
      source_id TEXT REFERENCES scout_sources(id),
      name TEXT NOT NULL,
      glob_pattern TEXT NOT NULL,
      tag TEXT,
      priority INTEGER DEFAULT 100,
      enabled BOOLEAN DEFAULT TRUE,
      created_by TEXT NOT NULL,      -- 'inferred', 'manual', 'template'
      created_at TEXT NOT NULL,
      UNIQUE(source_id, name)
  );

  CREATE TABLE extraction_fields (
      id TEXT PRIMARY KEY,
      rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
      field_name TEXT NOT NULL,
      source_type TEXT NOT NULL,     -- 'segment', 'filename', 'full_path'
      source_value TEXT,             -- e.g., "-2" for segment(-2)
      pattern TEXT,
      type_hint TEXT DEFAULT 'string',
      UNIQUE(rule_id, field_name)
  );

  CREATE TABLE extraction_tag_conditions (
      id TEXT PRIMARY KEY,
      rule_id TEXT REFERENCES extraction_rules(id) ON DELETE CASCADE,
      field_name TEXT NOT NULL,
      operator TEXT NOT NULL,
      value TEXT NOT NULL,
      tag TEXT NOT NULL,
      priority INTEGER DEFAULT 100
  );
  ```
- [ ] Add columns to `scout_files`:
  ```sql
  ALTER TABLE scout_files ADD COLUMN metadata_extracted JSON;
  ALTER TABLE scout_files ADD COLUMN matched_rule_id TEXT;
  ALTER TABLE scout_files ADD COLUMN extraction_status TEXT;
  ```
- [ ] Implement CRUD operations in `casparian_scout` crate

##### Phase 18g: Template Matching (Tier 1 Simple API)
- [ ] Implement template matching for single-file workflow:
  - When user selects a single file and presses `e`:
    - Run `match_templates(path)` from extraction.md Section 5.1
    - Show top 3 template matches with confidence scores
    - User selects template → pre-populate EDIT RULE fields
- [ ] Built-in templates (from extraction.md Appendix A):
  | Template | Glob Pattern | Fields |
  |----------|--------------|--------|
  | `defense` | `**/[Mm]ission_*/{date}/**/*` | mission_id, date |
  | `healthcare` | `**/{type}_{direction}/{year}/{month}/{day}/*` | message_type, direction, year, month, day |
  | `finance` | `**/FIX_logs/{year}/Q{quarter}/**/*` | year, quarter |
  | `legal` | `**/matter_*/{custodian}/**/*` | matter_id, custodian |
- [ ] Template selection UI:
  ```
  ┌─ TEMPLATE SUGGESTIONS ──────────────────────────────────────┐
  │  Analyzing: /data/mission_042/2024-01-15/telemetry.csv      │
  │                                                              │
  │  #1 Defense Mission (ISO dates)          ████████░░ 82%      │
  │     ├─ mission_id: "042" (from folder)                       │
  │     └─ date: "2024-01-15" (ISO format)                       │
  │                                                              │
  │  #2 Generic Dated                        █████░░░░░ 52%      │
  │     └─ date: "2024-01-15"                                    │
  │                                                              │
  │  #3 Generic Entity                       ███░░░░░░░ 31%      │
  │                                                              │
  │  [1-3] Select template   [m] More files   [c] Custom rule    │
  └──────────────────────────────────────────────────────────────┘
  ```

##### Phase 18h: Multi-File Inference (Tier 1 Algorithmic)
- [ ] When 3+ files selected, run algorithmic inference:
  - Tokenize all paths into segments
  - Analyze each segment position (fixed, variable, date, numeric)
  - Generate suggested glob + extraction fields
- [ ] Show inference results with confidence:
  ```
  ┌─ PATTERN INFERENCE ─────────────────────────────────────────┐
  │  Analyzed 423 files                                          │
  │                                                              │
  │  Confidence: ████████████████░ 92%                           │
  │                                                              │
  │  Detected segments:                                          │
  │    Segment 1: Variable → {mrn} (187 unique)                  │
  │    Segment 2: Category → {type} (labs, imaging, notes)       │
  │    Segment 3: ISO Date → {date}                              │
  │                                                              │
  │  Generated rule:                                             │
  │    glob: "patients/{mrn}/{type}/{date}_*.pdf"                │
  │    extract: { mrn, type, date }                              │
  │                                                              │
  │  [Enter] Accept   [e] Edit   [Esc] Cancel                    │
  └──────────────────────────────────────────────────────────────┘
  ```

#### Phase 19: Pattern Input Performance (Complete)
- [x] **Debounced pattern input** (150ms delay)
  - Keystrokes update `pattern_changed_at` timestamp only (instant, no blocking work)
  - Actual search triggers in `tick()` after 150ms of no typing
  - Avoids spawning background tasks while user is still typing
- [x] **Cancellable background search**
  - `glob_search_cancelled: Arc<AtomicBool>` cancellation token
  - When new search starts, previous search's token set to `true`
  - Background task checks every 1000 entries: exits early if cancelled
  - Saves CPU cycles when user types quickly
- [x] **Utility function consolidation**
  - `spinner_char(tick)` - reusable spinner animation
  - `FolderInfo::new()`, `::loading()`, `::with_path()` constructors
  - `centered_scroll_offset()` - virtual scroll calculation
  - `render_centered_dialog()` - dialog centering helper

**Pattern Input Flow (Optimized):**
```
Keystroke → pattern_changed_at = now()  [instant, no work]
    ↓
tick() → 150ms elapsed? → update_folders_from_cache()
    ↓
** pattern? → cancel previous → clone cache → spawn_blocking
    ↓
Background: for entry in cache { if cancelled { return } ... }
```

---

## 14. Data Persistence & Scanning

### 14.1 Persistence Architecture

All sources and files are persisted to SQLite, ensuring data survives TUI restarts.

**Database Location:** `~/.casparian_flow/casparian_flow.sqlite3`

**Persistence Flow:**
```
┌─────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│   Add Source    │────►│  Parallel Scan   │────►│  Persist to DB   │
│  (TUI dialog)   │     │   (background)   │     │ (scout_sources,  │
│                 │     │                  │     │  scout_files)    │
└─────────────────┘     └──────────────────┘     └──────────────────┘
         │                                                │
         │                                                │
         ▼                                                ▼
┌─────────────────┐                              ┌──────────────────┐
│  Next Session   │◄─────────────────────────────│   Load from DB   │
│   (TUI start)   │                              │  (on mode entry) │
└─────────────────┘                              └──────────────────┘
```

**What Gets Persisted:**

| Table | Data | When Saved |
|-------|------|------------|
| `scout_sources` | Source path, name, type | On scan start (upsert) |
| `scout_files` | File path, size, mtime, tag | On scan complete (batch insert) |

**Loading Behavior:**
- Sources load from DB when entering Discover mode
- Files load from DB when selecting a source
- File counts derive from `COUNT(*)` queries per source

### 14.2 Unified Parallel Scanner

The scanner uses parallel filesystem walking with configurable options.

**Configuration Options:**

```rust
pub struct ScanConfig {
    pub threads: usize,           // 0 = auto-detect CPU count
    pub batch_size: usize,        // 1000 files per batch (default)
    pub progress_interval: usize, // 5000 files between progress updates
    pub follow_symlinks: bool,    // false (default)
    pub include_hidden: bool,     // true (default)
}
```

**Progress Updates:**

During scanning, progress is reported via channel:

```rust
pub struct ScanProgress {
    pub dirs_scanned: usize,
    pub files_found: usize,
    pub current_dir: Option<String>,
}
```

**TUI Integration:**
- Progress bar shows scan status
- Current directory displayed during scan
- Files/dirs counts update in real-time
- Scan runs in background (non-blocking)

### 14.3 Add Source Dialog with Directory Autocomplete

The Add Source dialog provides directory autocomplete for better path input UX.

**Layout:**

```
┌─ Add Source ─────────────────────────────────────┐
│                                                  │
│  Path: /Users/shan/Do█                           │
│  ┌────────────────────────────────────────────┐  │
│  │ ► Documents/                               │  │
│  │   Downloads/                               │  │
│  │   Desktop/                                 │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│  [Tab] complete  [↑↓] select  [Enter] confirm    │
└──────────────────────────────────────────────────┘
```

**Autocomplete Behavior:**

| Feature | Behavior |
|---------|----------|
| Live suggestions | Updates as user types |
| `~` expansion | Expands to home directory |
| Hidden filtering | Excludes dotfiles/dotfolders |
| Case-insensitive | Matches regardless of case |
| Max suggestions | 8 directories shown |
| Sorted | Alphabetical order |

**Keybindings (EnteringPath state):**

| Key | Action |
|-----|--------|
| `Tab` | Complete to selected suggestion |
| `↑` / `↓` | Navigate through suggestions |
| `Char(c)` | Append to path, refresh suggestions |
| `Backspace` | Remove character, refresh suggestions |
| `Enter` | Confirm path and start scan |
| `Esc` | Cancel dialog |

**State Fields:**

```rust
pub struct DiscoverState {
    // ... existing fields ...

    // --- Directory autocomplete (Add Source dialog) ---
    pub path_suggestions: Vec<String>,    // Available directories
    pub path_suggestion_idx: usize,       // Currently highlighted suggestion
}
```

**Helper Function:**

```rust
fn list_directories(partial_path: &str) -> Vec<String> {
    // 1. Expand ~ to home directory
    // 2. Split into parent dir and prefix
    // 3. Read parent directory
    // 4. Filter: directories only, no hidden, case-insensitive prefix match
    // 5. Sort alphabetically
    // 6. Return up to 8 suggestions with trailing /
}
```

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 1.0 | Initial subspec extracted from spec.md |
| 2026-01-08 | 1.0 | Added dropdown navigation design |
| 2026-01-08 | 1.1 | **Major redesign**: Renamed Rules → Tags in sidebar |
| 2026-01-08 | 1.1 | Added Rules Manager dialog for rule CRUD |
| 2026-01-08 | 1.1 | Tags now derived from files, not rules |
| 2026-01-08 | 1.1 | Added quick rule creation flow (Ctrl+S) |
| 2026-01-13 | 1.6 | **Added Section 13: Glob Explorer** - Interactive pattern-based exploration |
| 2026-01-13 | 1.6 | Glob Explorer: State machine (EXPLORE → FOCUSED → EDIT → TEST → PUBLISH) |
| 2026-01-13 | 1.6 | Glob Explorer: Unified rule model (Glob + Extraction + Tagging) |
| 2026-01-13 | 1.6 | Glob Explorer: Field metrics during test (value distributions, min/max, unique counts) |
| 2026-01-13 | 1.6 | Glob Explorer: Semantic clustering by path structure |
| 2026-01-13 | 1.6 | Glob Explorer: Pattern history navigation (Backspace to go back) |
| 2026-01-13 | 1.6 | Glob Explorer: Background job integration for publish |
| 2026-01-08 | 1.2 | **Added Section 8: Extractors** - Path metadata extraction architecture |
| 2026-01-08 | 1.2 | Extractors: JSON blob storage (`metadata_raw`), read-time merge in Rust |
| 2026-01-08 | 1.2 | Extractors: Explicit error states (OK, PENDING, TIMEOUT, CRASH, STALE) |
| 2026-01-08 | 1.2 | Extractors: Subprocess isolation with fail-fast batch semantics |
| 2026-01-08 | 1.2 | Extractors: TUI integration (metadata filter, Problems tab, status indicators) |
| 2026-01-08 | 1.2 | Added extractor database queries (10.5-10.9) |
| 2026-01-08 | 1.2 | Added implementation phases 6-10 for Extractors |
| 2026-01-08 | 1.3 | **AI Integration**: Added wizard keybindings (W, w, g, l) |
| 2026-01-08 | 1.3 | Added Section 8.9: Pending Review Panel for unmatched files/groups |
| 2026-01-08 | 1.3 | Added cf_ai_audit_log table (10.10) for AI compliance tracking |
| 2026-01-08 | 1.3 | Cross-reference to specs/ai_wizards.md for Layer 2 AI architecture |
| 2026-01-12 | 1.4 | **Semantic Path Integration (Section 8.10)**: Added automatic recognition on scan, source sidebar indicator (📐), semantic info in file details, cross-source discovery. Updated Pending Review with Unrecognized Sources category. Added Phase 11 implementation tasks. Cross-reference to specs/semantic_path_mapping.md. |
| 2026-01-12 | 1.5 | **Consolidation**: Added deprecation notice for Python extractors (path parsing) in favor of Extraction Rules. Added Coverage Gaps category to Pending Review with near-miss detection UI. Added keybindings for coverage gap actions (a, x, c). Cross-reference to extraction_rules.md Section 1.5 and 9.5. |
| 2026-01-13 | 1.7 | **Sources Manager (Section 3.5)**: Added full CRUD dialog for sources (`M` key). States: SourcesManager, SourceEdit, SourceDeleteConfirm. Keybindings: n/e/d/r in manager, text input in edit, y/n/Enter/Esc in delete confirm. Added source management queries (10.5). |
| 2026-01-13 | 1.8 | **Data Persistence & Scanning (Section 14)**: Added comprehensive documentation for persistence architecture, unified parallel scanner, and directory autocomplete. Sources/files now persist to SQLite and survive TUI restarts. Added `ScanConfig` with configurable threads, batch_size, progress_interval, follow_symlinks, include_hidden. Add Source dialog now includes live directory autocomplete with Tab completion, Up/Down navigation, ~ expansion, and case-insensitive matching. |
| 2026-01-13 | 1.9 | **Glob Explorer Redesign (Section 13)**: Scan-time folder cache with trie structure and segment interning (~1MB for 1.2M files). O(1) folder navigation via HashMap lookup. Progressive reveal: heat map (≥200 matches) → heat map + flat results (<200 matches). Density bars with ▓/▒ proportional blocks (24 char width). vim-style navigation (hjkl). Full glob syntax via `globset` crate. Scan-in-progress state blocks navigation until cache is built. Updated data model with `FolderCache`, `FolderNode`, `GlobExplorerState`. New implementation phases 12-18. |
| 2026-01-13 | 2.0 | **Pattern Input Performance (Phase 19)**: Debounced pattern input (150ms delay) - keystrokes instant, search triggers after pause. Cancellable background search via `Arc<AtomicBool>` - cancelled tasks exit early saving CPU. Updated `GlobExplorerState` with debouncing fields (`pattern_changed_at`, `last_searched_pattern`, `last_searched_prefix`). Added `FolderInfo` constructors (`::new()`, `::loading()`, `::with_path()`, `::from_cache_entry()`). Added `GlobPreviewFile` and `GlobFileCount` types. Consolidated utility functions (`spinner_char()`, `centered_scroll_offset()`, `render_centered_dialog()`). Deleted dead code (~200 lines). |
| 2026-01-13 | 2.1 | **Extraction API Integration (Phase 18 Detailed)**: Expanded Phase 18 into 8 sub-phases (18a-18h) for complete Extraction API integration. Phase 18a: Extended `GlobExplorerPhase` enum with EditRule, Testing, Publishing, Published states. Phase 18b: EDIT RULE implementation with `RuleEditorState`, field inference, live match counts. Phase 18c: Field inference engine detecting dates, entity prefixes, categories from patterns. Phase 18d: TEST state with `TestState`, extraction runner, field metrics aggregation, histogram rendering. Phase 18e: PUBLISH state with `PublishState`, rule persistence, background job creation. Phase 18f: Database tables from extraction.md (extraction_rules, extraction_fields, extraction_tag_conditions). Phase 18g: Template matching for single-file workflow (Tier 1 Simple API). Phase 18h: Multi-file algorithmic inference (Tier 1 with confidence scoring). Cross-reference to specs/extraction.md Sections 5, 6, Appendix A/B. |
| 2026-01-13 | 2.2 | **Spec Refinement Integration**: Applied 10 gap resolutions from spec refinement workflow (session: discover_extraction). **Section 13.3**: Unified 6-state machine (Browse, Filtering, EditRule, Testing, Publishing, Published) with Navigation Layer and Rule Editing Layer. `e` key requires Filtering state with matches > 0. Esc from Testing/Publishing returns to EditRule (preserves draft). Publishing requires explicit Enter confirmation. Return to Browse (root) after Published (clean slate). **Phase 18a**: Corrected state transitions with entry/exit conditions. **Phase 18b**: Added `RuleDraft`, `FieldDraft`, `FieldSource`, `FieldType`, `CompareOp`, `Normalizer`, `TagConditionDraft` types aligned with DB schema. **Phase 18c**: Added `FieldInferenceConfig` with stratified sampling (max 100 files), confidence thresholds (HIGH >= 0.85, MEDIUM 0.50-0.84, LOW < 0.50), multi-factor scoring algorithm. **Phase 18d**: Always-async test execution with cancellation via `Arc<AtomicBool>`, `HistogramConfig` (12-char bars, 5 max values, 15-char labels), proportional scaling with min 1 char for non-zero. **Phase 18e**: `PublishError` enum with `RecoveryOption` variants, conflict detection (name, pattern), partial success handling for job creation failures. **Section 13.8**: Definitive EDIT RULE ASCII layout with focus indicators (`+== ... ==+` for focused, `+-- ... --+` for unfocused), section numbers (1/4, 2/4...), section-specific keybindings, field edit sub-focus mode. Session artifacts at `specs/meta/sessions/discover_extraction/`. |
| 2026-01-14 | 2.3 | **UI/UX Improvements**: Fixed Home Hub stats not updating after sources load. Fixed Discover title bar showing "0 files" when glob explorer is active (now shows glob explorer file count). Added selection indicator (►) to FOLDERS panel for clearer navigation feedback. Added selection indicator (►) to OPTIONS checkboxes in CREATE RULE dialog. Updated Section 13.5 Visual treatment with selection indicator documentation. Updated Section 13.9 CREATE RULE dialog ASCII art to show selection indicator. |
| 2026-01-14 | 3.0 | **Rule Builder Consolidation**: Major redesign consolidating 5 concepts into unified Rule Builder. Merged: GlobExplorer, RuleCreation, Pathfinder Wizard, Semantic Path Wizard, Labeling Wizard. Rule Builder provides split-view interface (40% config / 60% live results) with: live pattern filtering, auto-analysis for extractions, inline backtest with pass/fail per file, exclusion system (ignore folders with `i`/`I` keys), result filtering (`a`/`p`/`f` for all/pass/fail). Parser Lab remains separate (generates Python code). Updated state categories: Category 5 now RuleBuilder, Category 6 now ParserLab. Keybinding changes: `n`/`r` opens Rule Builder, `W` opens Parser Lab directly, `g` removed (use Rule Builder). Deprecated: WizardMenu, all Pathfinder states, GlobExplorer phases. Full design: `specs/meta/sessions/ai_consolidation/design.md`. |
| 2026-01-14 | 3.1 | **Streaming Scanner Architecture**: Replaced file-based `FolderCache` (.bin.zst) with SQLite `scout_folders` table. Section 13.1: Updated design philosophy - streaming scan with bounded memory O(batch_size), on-demand SQLite queries for folder drill-down (<5ms), cancel support. Section 13.2: New schema with `scout_folders` table, `get_folder_children()` query. Phases 12-14: Updated for streaming architecture (ScanStream, persist_task, incremental folder updates). Full spec: `specs/streaming_scanner.md`. Session: `specs/meta/sessions/streaming_scanner_2026-01-14/`. |
| 2026-01-14 | 3.2 | **Corpus Alignment**: Updated Section 5.6 `GlobExplorerState` fields to use on-demand loading (`loading`, `scan_in_progress`, `minutes_since_scan`) instead of preloaded `folder_cache`. Updated Section 13.11 data model with deprecated `FolderCache` notice and new `FolderEntry` struct. Aligned with streaming_scanner.md v1.1 gap fixes (batch upserts, staleness detection). |
| 2026-01-14 | 3.3 | **Codebase Alignment**: Removed archived `specs/views/sources.md` reference from Related header. Section 4: Updated state machine from 25 states to 14 view states + GlobExplorer overlay, removed non-existent wizard states (WizardMenu, Pathfinder*, ParserLab*). Section 5: Fixed GlobExplorer as `Option<GlobExplorerState>` field (not enum variant), removed wizard state types. Section 6: Updated `W` key status (removed AI Wizards), removed `w` Pathfinder key, noted AI assistance consolidated into GlobExplorer EditRule. |

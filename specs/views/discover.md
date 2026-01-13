# Discover - TUI View Spec

**Status:** Approved for Implementation
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.8
**Related:** specs/extraction.md (Extraction API), specs/views/sources.md

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

```
                    ┌─────────────────────────────────────┐
                    │                                     │
    ┌───────────────┴───────────────┐                     │
    │                               │                     │
    ▼                               │                     │
┌─────────────┐     1          ┌─────────────┐            │
│   FILES     │◄───────────────│  SOURCES    │            │
│  (default)  │    Enter       │  DROPDOWN   │            │
│             │                │   (open)    │            │
└──────┬──────┘                └──────┬──────┘            │
       │                              │                   │
       │ 2                            │ Esc               │
       ▼                              │                   │
┌─────────────┐                       │                   │
│    TAGS     │───────────────────────┘                   │
│  DROPDOWN   │                                           │
│   (open)    │────────────────────────────────────────────┘
└─────────────┘     Enter

       │ R (from any state)           M (from any state)
       ▼                               ▼
┌─────────────┐                 ┌─────────────┐
│   RULES     │                 │  SOURCES    │
│  MANAGER    │──── Esc ───►    │  MANAGER    │──── Esc ────► (return to previous)
│  (dialog)   │                 │  (dialog)   │
└─────────────┘                 └──────┬──────┘
                                       │ e (edit)
                                       ▼
                                ┌─────────────┐
                                │   SOURCE    │
                                │    EDIT     │──── Esc/Enter ────► SourcesManager
                                │  (dialog)   │
                                └─────────────┘

States:
- FILES: Default state, arrows navigate files
- SOURCES_DROPDOWN: Filter/navigate sources, files preview updates
- TAGS_DROPDOWN: Filter/navigate tags, files filter by tag
- RULES_MANAGER: Dialog overlay for managing tagging rules
- SOURCES_MANAGER: Dialog overlay for managing sources (CRUD)
- SOURCE_EDIT: Nested dialog for editing source name
```

### 4.1 State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| `Files` | Default, Enter from dropdown | Press 1, 2, R, or M | Navigate files, tag, preview |
| `SourcesDropdown` | Press 1 | Enter/Esc | Filter sources, live file preview |
| `TagsDropdown` | Press 2 | Enter/Esc | Filter tags, filter files by tag |
| `RulesManager` | Press R | Esc | CRUD operations on tagging rules |
| `SourcesManager` | Press M | Esc | CRUD operations on sources |
| `SourceEdit` | Press e in SourcesManager | Enter/Esc | Edit source name |
| `SourceDeleteConfirm` | Press d in SourcesManager | Enter/Esc | Confirm source deletion |

### 4.2 Preview vs Selection

Dropdowns have **two-stage selection**:
1. **Preview** (during navigation): Files update as you move
2. **Selection** (on Enter): Dropdown closes, becomes the active choice

---

## 5. Data Model

```rust
pub struct DiscoverState {
    // --- Focus tracking ---
    pub focus: DiscoverFocus,

    // --- Sources ---
    pub sources: Vec<SourceInfo>,
    pub selected_source: usize,
    pub sources_dropdown_open: bool,
    pub sources_filter: String,
    pub preview_source: Option<usize>,

    // --- Tags (replaces Rules in sidebar) ---
    pub tags: Vec<TagInfo>,
    pub selected_tag: Option<usize>,     // None = "All files"
    pub tags_dropdown_open: bool,
    pub tags_filter: String,
    pub preview_tag: Option<usize>,

    // --- Files ---
    pub files: Vec<FileInfo>,
    pub selected_file: usize,
    pub filter: String,                  // Manual filter (separate from tag)

    // --- Preview pane ---
    pub preview_content: Option<String>,
    pub show_preview: bool,

    // --- Rules Manager (dialog) ---
    pub rules_manager_open: bool,
    pub rules: Vec<RuleInfo>,
    pub selected_rule: usize,
    pub rule_edit_mode: Option<RuleEditMode>,

    // --- Loading states ---
    pub loading_files: bool,
    pub loading_sources: bool,

    // --- Directory autocomplete (Add Source dialog) ---
    pub path_suggestions: Vec<String>,    // Available directories matching input
    pub path_suggestion_idx: usize,       // Currently highlighted suggestion
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiscoverFocus {
    Sources,
    Tags,    // Renamed from Rules
    Files,
}

/// Tag with file count (for Tags dropdown)
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,        // Tag name or "All files" or "untagged"
    pub count: usize,        // Number of files with this tag
    pub is_special: bool,    // True for "All files" and "untagged"
}

/// Tagging rule (for Rules Manager)
#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: i64,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub enum RuleEditMode {
    Creating,
    Editing(i64),  // Rule ID being edited
}

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub file_count: usize,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub id: i64,
    pub path: String,
    pub rel_path: String,
    pub size: u64,
    pub tag: Option<String>,
    pub status: FileStatus,

    // --- Extractor fields (see Section 8) ---
    pub metadata_raw: Option<serde_json::Value>,    // Raw JSON from DB
    pub metadata_merged: Option<serde_json::Value>, // After inheritance merge
    pub extraction_status: ExtractionStatus,        // OK, PENDING, TIMEOUT, CRASH, STALE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionStatus {
    Ok,
    Pending,
    Timeout,
    Crash,
    Stale,
}
```

---

## 6. Keybindings

### 6.1 Global (Discover Mode)

| Key | Action |
|-----|--------|
| `1` | Open Sources dropdown |
| `2` | Open Tags dropdown |
| `3` | Focus Files panel |
| `n` | **Create new tagging rule** (opens dialog) |
| `s` | Scan new directory |
| `p` | Toggle preview pane |
| `R` | Open Rules Manager dialog |
| `M` | **Open Sources Manager dialog** |
| `W` | **Open AI Wizards menu** (see Section 8.7) |
| `S` | **Launch Semantic Path Wizard** for current source |
| `!` | **Open Pending Review panel** (files needing attention) |
| `g` | **Open Glob Explorer** (interactive pattern exploration) |
| `Esc` | Close dropdown/dialog or return to Home |

> **Key Override Note:** In Discover mode, `1`, `2`, `3` control panel focus instead of
> global view navigation. This is an intentional override documented in tui.md Section 3.3.
> To navigate to other views from Discover, use `0`/`H` (Home), `4` (Sources), or `Esc` to
> go back to Home first. The override exists because Discover's three-panel layout
> (Sources/Tags/Files) is core to the workflow.

### 6.2 Sources Dropdown (when open)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate sources (triggers file reload) |
| `Char(c)` | Append to filter (including numbers) |
| `Backspace` | Remove from filter |
| `Enter` | Confirm selection, close dropdown, focus Files |
| `Esc` | Close dropdown, revert to previous selection |

### 6.3 Tags Dropdown (when open)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate tags (**live preview**: files filter instantly) |
| `Char(c)` | Append to filter |
| `Backspace` | Remove from filter / go to "All files" |
| `Enter` | Confirm selection, close dropdown, focus Files |
| `Esc` | Close dropdown, reset to "All files" |

**Live Preview Behavior:**
- As you navigate through tags with `↑`/`↓`, the Files panel updates in real-time
- "All files" shows all files (no tag filter)
- "untagged" shows only files without tags
- Specific tags show only files with that tag
- Text filter (`/`) stacks on top of tag filter

### 6.4 Files Panel

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `/` | Enter filter mode (type to filter by path) |
| `t` | Tag selected file (or filtered files if filter active) |
| `T` | Bulk tag filtered files |
| `Enter` | Drill into directory OR show file details |
| `w` | **Launch Pathfinder Wizard** for selected file's path |
| `g` | **Launch Parser Lab** for current file group |
| `l` | **Launch Labeling Wizard** for current group |

### 6.5 Rule Creation Dialog

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch between Pattern and Tag fields |
| `Char(c)` | Type into focused field |
| `Backspace` | Delete from focused field |
| `Enter` | Create rule |
| `Esc` | Cancel and close |

### 6.6 Wizard Dialog (Onboarding)

| Key | Action |
|-----|--------|
| `n` | Create a tagging rule (opens rule dialog) |
| `Enter` | Browse files first (close wizard) |
| `Space` | Toggle "Don't show again" checkbox |
| `Esc` | Close wizard |

### 6.7 Rules Manager Dialog

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `n` | Create new rule |
| `e` | Edit selected rule |
| `d` | Delete selected rule |
| `Enter` | Toggle rule enabled/disabled |
| `Esc` | Close dialog |

### 6.8 Sources Manager Dialog

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `n` | Add new source (opens scan dialog) |
| `e` | Edit selected source name |
| `d` | Delete selected source (with confirmation) |
| `r` | Rescan selected source |
| `Esc` | Close dialog |

### 6.9 Source Edit Dialog

| Key | Action |
|-----|--------|
| `Char(c)` | Type into name field |
| `Backspace` | Delete from name field |
| `Enter` | Save changes |
| `Esc` | Cancel and close |

### 6.10 Source Delete Confirmation

| Key | Action |
|-----|--------|
| `Enter` / `y` | Confirm deletion |
| `Esc` / `n` | Cancel |

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
ORDER BY s.name
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

The Glob Explorer provides interactive pattern-based file exploration with semantic clustering and unified rule creation.

> **Cross-reference:** For extraction rule YAML schema, database tables, and CLI commands,
> see `specs/extraction.md`. The Glob Explorer is the TUI interface for the Extraction API.

### 13.1 Design Philosophy

- **Glob pattern as navigation state**: The pattern IS the drill-down mechanism
- **Semantic clustering**: Group files by meaning, not just path
- **Unified rule model**: Glob + Extraction + Tagging defined together
- **Test vs Publish**: Test shows results without persisting; Publish persists to DB
- **Field metrics during test**: See extracted value distributions before committing

### 13.2 State Machine

```
                    ┌─────────────────────────────────────────────────────┐
                    │                                                     │
    ┌───────────────┴───────────────┐                                     │
    │                               │                                     │
    ▼                               │                                     │
┌─────────────┐                ┌─────────────┐                            │
│   EXPLORE   │───────────────►│   FOCUSED   │                            │
│  (default)  │  Select cluster│   (subset)  │                            │
│             │◄───────────────│             │                            │
└──────┬──────┘   Backspace    └──────┬──────┘                            │
       │                              │                                   │
       │                              │ e (edit rule)                     │
       │                              ▼                                   │
       │                       ┌─────────────┐                            │
       │                       │  EDIT RULE  │                            │
       │                       │  (overlay)  │                            │
       │                       └──────┬──────┘                            │
       │                              │                                   │
       │                              │ t (test)                          │
       │                              ▼                                   │
       │                       ┌─────────────┐                            │
       │                       │    TEST     │◄────────────────────────────┤
       │                       │  (results)  │       Edit rule (e)        │
       │                       └──────┬──────┘                            │
       │                              │                                   │
       │                              │ p (publish)                       │
       │                              ▼                                   │
       │                       ┌─────────────┐                            │
       │                       │   PUBLISH   │───── Job started ──────────┘
       │                       │  (confirm)  │       Return to EXPLORE
       │                       └─────────────┘
       │
       │ Esc (from any state)
       ▼
   [Exit Glob Explorer]
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| `Explore` | Default, Backspace from Focused | Select cluster, Esc | Type glob pattern, see semantic clusters |
| `Focused` | Select cluster in Explore | Backspace, e, Esc | Refined pattern, smaller file set |
| `EditRule` | Press e in Focused | t (test), Esc | Define extraction + tagging |
| `Test` | Press t in EditRule | p (publish), e (edit), Esc | See results with field metrics |
| `Publish` | Press p in Test | Confirm → Explore | Persist rule, start background job |

### 13.3 EXPLORE State Layout

```
┌─ GLOB EXPLORER ────────────────────────────────────────────────────────────────┐
│                                                                                │
│  Pattern: **/*.csv                                                     [1247] │
│           ▔▔▔▔▔▔▔▔▔                                                           │
│                                                                                │
├─ SEMANTIC CLUSTERS ────────────────────────────────────────────────────────────┤
│                                                                                │
│  ► mission_data/         entity_folder(mission) > dated      847 files        │
│    └─ mission_042/2024-01-15/*.csv                                            │
│                                                                                │
│    sensor_readings/      dated_hierarchy(nested) > category  312 files        │
│    └─ 2024/01/15/temperature/*.csv                                            │
│                                                                                │
│    reports/              flat                                 88 files        │
│    └─ quarterly_report_*.csv                                                  │
│                                                                                │
├─ PREVIEW (3 files) ────────────────────────────────────────────────────────────┤
│  /data/mission_042/2024-01-15/telemetry.csv          1.2 MB   Jan 15          │
│  /data/mission_042/2024-01-14/telemetry.csv          1.1 MB   Jan 14          │
│  /data/sensor_readings/2024/01/temp.csv              892 KB   Jan 15          │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [↑↓] Navigate clusters   [Enter] Focus   [Backspace] Back   [e] Edit rule     │
│ [/] Refine pattern       [Tab] Cycle semantic view          [Esc] Exit        │
└────────────────────────────────────────────────────────────────────────────────┘
```

**Key Behaviors:**

- **Pattern updates live**: As user types, clusters update
- **Cluster selection → pattern refinement**: Selecting "mission_data/" updates pattern to `**/mission_*/**/*.csv`
- **Backspace navigation**: Returns to previous pattern in history
- **Count display**: Exact for <1000, "1000+" for large sets with `[c]` to count all

### 13.4 FOCUSED State Layout

```
┌─ GLOB EXPLORER ────────────────────────────────────────────────────────────────┐
│                                                                                │
│  Pattern: **/mission_*/**/*.csv                                         [847] │
│           ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔                                               │
│  History: **/*.csv → **/mission_*/**/*.csv                                    │
│                                                                                │
├─ SUB-CLUSTERS ─────────────────────────────────────────────────────────────────┤
│                                                                                │
│  ► mission_042/          entity_folder(mission) > dated      423 files        │
│    mission_043/          entity_folder(mission) > dated      312 files        │
│    mission_044/          entity_folder(mission) > dated      112 files        │
│                                                                                │
├─ DETECTED STRUCTURE ───────────────────────────────────────────────────────────┤
│                                                                                │
│  Pattern: entity_folder(mission) > dated_hierarchy(iso)                       │
│  Confidence: 94%                                                              │
│                                                                                │
│  Inferred fields:                                                             │
│    • mission_id: from folder name (e.g., "042")                               │
│    • date: from ISO date folder (e.g., "2024-01-15")                          │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [e] Edit rule   [Enter] Drill deeper   [Backspace] Back to **/*.csv           │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 13.5 EDIT RULE State Layout (Unified Rule Model)

The rule combines Glob + Extraction + Tagging in a single definition:

```
┌─ EDIT RULE ────────────────────────────────────────────────────────────────────┐
│                                                                                │
│  ┌─ GLOB PATTERN ──────────────────────────────────────────────────────────┐  │
│  │  **/mission_*/**/*.csv                                           [847]  │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌─ EXTRACTION FIELDS ─────────────────────────────────────────────────────┐  │
│  │                                                                          │  │
│  │  mission_id:                                                             │  │
│  │    from: segment(-3)                                                     │  │
│  │    pattern: mission_(\d+)                                                │  │
│  │    type: integer                                                         │  │
│  │                                                                          │  │
│  │  date:                                                                   │  │
│  │    from: segment(-2)                                                     │  │
│  │    type: date                                                            │  │
│  │                                                                          │  │
│  │  [+] Add field   [d] Delete field   [↑↓] Navigate                        │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌─ TAGGING CONDITIONS ────────────────────────────────────────────────────┐  │
│  │                                                                          │  │
│  │  Base tag: mission_data                                                  │  │
│  │                                                                          │  │
│  │  Conditional tags:                                                       │  │
│  │    IF mission_id < 100 THEN tag = "legacy_missions"                      │  │
│  │    IF date.year = 2024 THEN tag = "current_year"                         │  │
│  │                                                                          │  │
│  │  [+] Add condition                                                       │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [t] Test rule   [Tab] Next section   [Esc] Cancel                             │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 13.6 TEST State Layout (with Field Metrics)

Test runs extraction + tagging on ALL matching files and shows results **without persisting**:

```
┌─ TEST RESULTS ─────────────────────────────────────────────────────────────────┐
│                                                                                │
│  Rule: "Mission Telemetry"                                                     │
│  Pattern: **/mission_*/**/*.csv                                                │
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
├─ TAGGING PREVIEW ──────────────────────────────────────────────────────────────┤
│                                                                                │
│  mission_data:      847 files (base tag)                                       │
│  legacy_missions:    89 files (mission_id < 100)                               │
│  current_year:      559 files (date.year = 2024)                               │
│                                                                                │
├─ SAMPLE FILES ─────────────────────────────────────────────────────────────────┤
│                                                                                │
│  /data/mission_042/2024-01-15/telemetry.csv                                   │
│    → mission_id: 42, date: 2024-01-15                                         │
│    → tags: [mission_data, current_year]                                       │
│                                                                                │
│  /data/mission_043/2024-02-01/readings.csv                                    │
│    → mission_id: 43, date: 2024-02-01                                         │
│    → tags: [mission_data, current_year]                                       │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│ [p] Publish rule   [e] Edit rule   [↑↓] Scroll   [Enter] Inspect file         │
│ [f] Filter by status   [v] Value drill-down      [Esc] Cancel                 │
└────────────────────────────────────────────────────────────────────────────────┘
```

**Field Metrics Features:**

| Feature | Description |
|---------|-------------|
| Value distribution | Histogram of top values per field |
| Unique count | Number of distinct values |
| Range | Min/Max for numeric and date fields |
| Drill-down | Press `v` on a field to see all values |

### 13.7 PUBLISH State

```
┌─ PUBLISH RULE ─────────────────────────────────────────────────────────────────┐
│                                                                                │
│  Rule: "Mission Telemetry"                                                     │
│  Pattern: **/mission_*/**/*.csv                                                │
│  Files: 847                                                                    │
│                                                                                │
│  This will:                                                                    │
│    ✓ Save rule to database                                                    │
│    ✓ Extract metadata for 847 files                                           │
│    ✓ Apply tags (mission_data, legacy_missions, current_year)                 │
│    ✓ Start background job (ID will be shown)                                  │
│                                                                                │
│  ─────────────────────────────────────────────────────────────────────────     │
│                                                                                │
│  [Enter] Confirm and publish   [Esc] Cancel                                   │
│                                                                                │
└────────────────────────────────────────────────────────────────────────────────┘
```

After publish:

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

### 13.8 Glob Explorer Data Model

```rust
/// Glob Explorer state (extends DiscoverState)
pub struct GlobExplorerState {
    // --- Pattern state ---
    pub pattern: String,
    pub pattern_history: Vec<String>,  // For Backspace navigation

    // --- Cluster display ---
    pub clusters: Vec<SemanticCluster>,
    pub selected_cluster: usize,

    // --- File preview ---
    pub preview_files: Vec<FileInfo>,
    pub file_count: FileCount,  // Exact or sampled

    // --- Rule editing ---
    pub rule_draft: Option<RuleDraft>,
    pub edit_focus: RuleEditFocus,

    // --- Test results ---
    pub test_results: Option<TestResults>,

    // --- State machine ---
    pub explorer_state: GlobExplorerPhase,
}

#[derive(Debug, Clone)]
pub enum GlobExplorerPhase {
    Explore,
    Focused,
    EditRule,
    Test,
    Publish,
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

### 13.9 Glob Explorer Keybindings

#### 13.9.1 EXPLORE State

| Key | Action |
|-----|--------|
| `Char(c)` | Append to pattern |
| `Backspace` | Remove from pattern OR go to previous pattern in history |
| `↑` / `↓` | Navigate clusters |
| `Enter` | Focus selected cluster (updates pattern) |
| `/` | Clear pattern and start fresh |
| `Tab` | Cycle semantic grouping view |
| `c` | Count all files (when showing estimate) |
| `Esc` | Exit Glob Explorer |

#### 13.9.2 FOCUSED State

| Key | Action |
|-----|--------|
| `e` | Open Edit Rule overlay |
| `Enter` | Drill into sub-cluster |
| `Backspace` | Go back to previous pattern |
| `↑` / `↓` | Navigate sub-clusters |
| `Esc` | Exit to Explore |

#### 13.9.3 EDIT RULE State

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: Pattern → Fields → Tagging |
| `Enter` | Edit selected item |
| `+` / `a` | Add field or condition |
| `d` | Delete selected item |
| `↑` / `↓` | Navigate within section |
| `t` | Test rule |
| `Esc` | Cancel, return to Focused |

#### 13.9.4 TEST State

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

### 13.11 Implementation Phases (Glob Explorer)

#### Phase 12: Glob Explorer Foundation
- [ ] Add `GlobExplorerState` to `DiscoverState`
- [ ] Implement EXPLORE state with pattern input
- [ ] Basic file listing from glob pattern
- [ ] Sampled counts for large result sets

#### Phase 13: Semantic Clustering
- [ ] Implement `analyze_semantic_structure()`
- [ ] Cluster files by semantic fingerprint
- [ ] Display clusters with file counts
- [ ] Pattern history with Backspace navigation

#### Phase 14: Rule Editing
- [ ] EDIT RULE overlay with three sections
- [ ] Field definition (from, pattern, type)
- [ ] Tag conditions editor
- [ ] Live validation feedback

#### Phase 15: Test State with Field Metrics
- [ ] Run extraction on ALL files
- [ ] Compute field metrics (unique count, distribution)
- [ ] Display histograms for top values
- [ ] Show tagging preview
- [ ] Value drill-down (`v` key)

#### Phase 16: Publish and Background Job
- [ ] Persist rule to database
- [ ] Create background extraction job
- [ ] Return job ID to user
- [ ] Integration with job status system

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

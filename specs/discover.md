# Discover Mode - TUI Subspec

**Status:** Approved for Implementation
**Parent:** spec.md Section 5.3 (TUI Specification)
**Version:** 1.1

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
1. User enters Discover mode (press D from Home)
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

### 3.5 Preview Panel

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

       │ R (from any state)
       ▼
┌─────────────┐
│   RULES     │
│  MANAGER    │──── Esc ────► (return to previous state)
│  (dialog)   │
└─────────────┘

States:
- FILES: Default state, arrows navigate files
- SOURCES_DROPDOWN: Filter/navigate sources, files preview updates
- TAGS_DROPDOWN: Filter/navigate tags, files filter by tag
- RULES_MANAGER: Dialog overlay for managing tagging rules
```

### 4.1 State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| `Files` | Default, Enter from dropdown | Press 1, 2, or R | Navigate files, tag, preview |
| `SourcesDropdown` | Press 1 | Enter/Esc | Filter sources, live file preview |
| `TagsDropdown` | Press 2 | Enter/Esc | Filter tags, filter files by tag |
| `RulesManager` | Press R | Esc | CRUD operations on tagging rules |

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
| `Esc` | Close dropdown/dialog or return to Home |
| `?` | Help overlay |

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

## 8. Empty States

| Condition | Display |
|-----------|---------|
| No sources | "No sources found. Press 's' to scan a folder." |
| Source selected, no files | "No files in this source." |
| Filter matches nothing | "No files match filter." |
| No tags (all untagged) | Tags dropdown shows only "All files" and "untagged" |
| No rules | Rules Manager shows "No rules. Press 'n' to create one." |

---

## 9. Database Queries

### 9.1 Load Sources

```sql
SELECT s.id, s.name, s.path, COUNT(f.id) as file_count
FROM scout_sources s
LEFT JOIN scout_files f ON f.source_id = s.id
GROUP BY s.id
ORDER BY s.name
```

### 9.2 Load Tags for Source

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

### 9.3 Load Files for Source (with tag filter)

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

### 9.4 Load Rules for Source

```sql
SELECT id, pattern, tag, priority, enabled
FROM scout_tagging_rules
WHERE source_id = ?
ORDER BY priority DESC, pattern
```

---

## 10. Implementation Phases

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

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sidebar shows Tags, not Rules | Tags are categories; Rules are mechanisms | Users think "show sales files" not "apply rule #3" |
| Rules managed separately | `R` opens Rules Manager dialog | Keeps sidebar simple, gives rules proper CRUD UI |
| Tags derived from files | Query `DISTINCT tag FROM scout_files` | Shows actual tags, not potential tags from rules |
| "untagged" as special tag | Explicit option in Tags dropdown | Easy to find files needing tagging |
| Rules apply in background | On scan and rule creation | Tags appear automatically, no manual "run rules" step |
| Navigation keys | Arrow keys only in dropdowns | j/k conflict with filter typing |
| Quick rule creation | `Ctrl+S` in Files panel | Natural "save filter" gesture |

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-08 | 1.0 | Initial subspec extracted from spec.md |
| 2026-01-08 | 1.0 | Added dropdown navigation design |
| 2026-01-08 | 1.1 | **Major redesign**: Renamed Rules → Tags in sidebar |
| 2026-01-08 | 1.1 | Added Rules Manager dialog for rule CRUD |
| 2026-01-08 | 1.1 | Tags now derived from files, not rules |
| 2026-01-08 | 1.1 | Added quick rule creation flow (Ctrl+S) |

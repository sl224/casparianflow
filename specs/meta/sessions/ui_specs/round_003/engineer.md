## Gap Resolution: GAP-STUB-003

**Confidence:** HIGH

### Proposed Solution

Below is the complete expanded specification for the Sources view.

---

# Sources - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0
**Related:** specs/extraction.md (Equivalence Classes)

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Sources** view manages data sources (directories), equivalence classes, and source-level configuration. It provides a hierarchical view of all configured sources grouped by their structural similarity.

### 1.1 Design Philosophy

- **Hierarchical organization**: Sources grouped by equivalence class for shared rule management
- **Health visibility**: Source status (file counts, scan recency, errors) at a glance
- **Rule propagation**: Changes to class rules propagate to all member sources
- **Flexible grouping**: Sources can be detached to standalone or moved between classes
- **Scan control**: Manual rescan and watch mode per source

### 1.2 Core Entities

```
~/.casparian_flow/casparian_flow.sqlite3

Tables queried:
├── scout_sources            # Source directories
├── scout_files              # File counts, tag percentages
├── equivalence_classes      # Shared structure groups
├── equivalence_members      # Source-to-class mapping
└── extraction_rules         # Rules per source/class
```

### 1.3 User Goals

| Goal | How Sources Helps |
|------|-------------------|
| "Where is my data?" | Tree shows all configured directories |
| "Which sources are similar?" | Equivalence classes group structurally similar sources |
| "Apply rules across sources" | Edit class rules, applies to all members |
| "Check source health" | File counts, tagged %, last scan, errors visible |
| "Add new data directory" | `n` opens path picker with equivalence detection |
| "Why isn't this scanning?" | Watch mode status and scan errors visible |

---

## 2. User Workflows

### 2.1 Browse Sources and Classes

```
1. User navigates to Sources view (press '4' from any view)
2. Tree view displays:
   - Equivalence classes as expandable parents
   - Sources nested under their class
   - Standalone sources under "Standalone" pseudo-group
3. User navigates with j/k, expands/collapses with Enter or Space
4. Detail panel shows selected source or class info
5. User can Tab between tree and detail panel
```

### 2.2 Add New Source

```
1. User presses 'n' from Sources view
2. Add Source dialog opens:
   ┌─ Add Source ─────────────────────────────────────┐
   │ Path: /Users/data/new_project█                   │
   │ Hint: Enter directory path                       │
   │                                                  │
   │ [ ] Watch for changes                            │
   │                                                  │
   │ [Enter] Add  [Esc] Cancel                        │
   └──────────────────────────────────────────────────┘
3. User enters path and submits
4. System scans directory structure
5. If equivalence match found (>80% similar):
   ┌─ Equivalence Detected ───────────────────────────┐
   │ This source matches "Mission Data" (94% similar) │
   │ Other members: /data/mission_alpha, /data/bravo  │
   │                                                  │
   │ Apply shared rules? [Y/n]                        │
   │                                                  │
   │ [Y] Join class  [n] Keep standalone  [Esc] Cancel│
   └──────────────────────────────────────────────────┘
6. Source added, appears in tree under class or Standalone
7. Toast: "Added /Users/data/new_project (1,247 files)"
```

### 2.3 Remove Source

```
1. User selects source in tree
2. User presses 'd' to remove
3. Confirmation dialog:
   ┌─ Remove Source ──────────────────────────────────┐
   │ Remove "/data/mission_alpha"?                    │
   │                                                  │
   │ This will:                                       │
   │ • Remove 1,247 files from tracking               │
   │ • Delete source-specific rules                   │
   │ • NOT delete files from disk                     │
   │                                                  │
   │ [Enter] Remove  [Esc] Cancel                     │
   └──────────────────────────────────────────────────┘
4. Source removed from tree and database
5. Toast: "Removed /data/mission_alpha"
```

### 2.4 Manual Rescan

```
1. User selects source in tree
2. Presses 's' to scan
3. Progress indicator appears in detail panel:
   Scanning... ████████░░░░ 67% (847 / 1,247 files)
4. Scan completes, stats refresh
5. Toast: "Scanned /data/mission_alpha (23 new, 5 modified)"
```

### 2.5 Toggle Watch Mode

```
1. User selects source in tree
2. Presses 'w' to toggle watch
3. If enabling watch:
   - Watch mode activates
   - Detail panel shows: "Watching for changes"
   - File changes auto-detected
4. If disabling watch:
   - Watch mode deactivates
   - Toast: "Watch mode disabled for /data/mission_alpha"
```

### 2.6 Edit Source Rules

```
1. User selects source in tree
2. Presses 'e' to edit rules
3. If source is in equivalence class:
   ┌─ Edit Rules ─────────────────────────────────────┐
   │ "/data/mission_alpha" is in class "Mission Data" │
   │                                                  │
   │ [c] Edit class rules (applies to 3 sources)      │
   │ [s] Add source-specific rule (this source only)  │
   │ [Esc] Cancel                                     │
   └──────────────────────────────────────────────────┘
4. If standalone: directly opens rule editor
5. User edits rules, changes apply immediately
```

### 2.7 Manage Equivalence Class

```
1. User selects equivalence class in tree (parent node)
2. Presses 'c' for class management
3. Class management dialog:
   ┌─ Manage Class: Mission Data ─────────────────────┐
   │ Members (3 sources):                             │
   │   /data/mission_alpha    (94% similar)           │
   │   /data/mission_bravo    (91% similar)           │
   │   /data/mission_charlie  (89% similar)           │
   │                                                  │
   │ Shared rules: 3                                  │
   │                                                  │
   │ [e] Edit shared rules  [r] Rename class          │
   │ [x] Delete class       [Esc] Close               │
   └──────────────────────────────────────────────────┘
4. User can edit rules, rename, or delete class
5. Delete class converts all members to standalone
```

### 2.8 Detach Source from Class

```
1. User selects source that is in an equivalence class
2. Presses 'D' (Shift+d) to detach
3. Confirmation:
   ┌─ Detach Source ──────────────────────────────────┐
   │ Detach "/data/mission_alpha" from "Mission Data"?│
   │                                                  │
   │ Source will become standalone.                   │
   │ Shared class rules will NOT be copied.           │
   │                                                  │
   │ [Enter] Detach  [Esc] Cancel                     │
   └──────────────────────────────────────────────────┘
4. Source moves to Standalone group in tree
5. Toast: "Detached /data/mission_alpha from Mission Data"
```

### 2.9 Move Source to Different Class

```
1. User selects standalone source or source in a class
2. Presses 'm' to move
3. Class selector dialog:
   ┌─ Move to Class ──────────────────────────────────┐
   │ > Filter classes█                                │
   ├──────────────────────────────────────────────────┤
   │   Mission Data (3 sources, 94% match)            │
   │   Patient Records (2 sources, 67% match)         │
   │   ─────────────────────────────────              │
   │   Create new class...                            │
   │   Keep standalone                                │
   └──────────────────────────────────────────────────┘
4. User selects class, source moves
5. Toast: "Moved /data/new_project to Mission Data"
```

### 2.10 Create New Equivalence Class

```
1. User selects 2+ standalone sources (multi-select with Space)
2. Presses 'C' (Shift+c) to create class
3. Create class dialog:
   ┌─ Create Equivalence Class ───────────────────────┐
   │ Name: New Class█                                 │
   │                                                  │
   │ Selected sources (2):                            │
   │   /data/project_a                                │
   │   /data/project_b                                │
   │                                                  │
   │ [Enter] Create  [Esc] Cancel                     │
   └──────────────────────────────────────────────────┘
4. Class created, sources grouped under it
5. Toast: "Created class 'New Class' with 2 sources"
```

---

## 3. Layout Specification

### 3.1 Full Layout

```
┌─ Casparian Flow ───────────────────────────────────────────────────────┐
│ Home > Sources                                              [?] Help   │
├─ Sources ─────────────────────────┬─ Details ──────────────────────────┤
│                                   │                                    │
│ ▼ Mission Data (3)                │  Source: /data/mission_alpha       │
│   ├─● /data/mission_alpha         │  ────────────────────────────────  │
│   ├─○ /data/mission_bravo         │                                    │
│   └─● /data/mission_charlie       │  Status: Healthy                   │
│                                   │  Last scan: 2 hours ago            │
│ ▼ Patient Records (2)             │  Watch mode: Enabled               │
│   ├─● /data/clinic_east           │                                    │
│   └─⚠ /data/clinic_west           │  ────────────────────────────────  │
│                                   │  Files                             │
│ ▼ Standalone                      │  Total: 1,247                      │
│   └─○ /data/misc_reports          │  Tagged: 89% (1,110)               │
│                                   │  Errors: 0                         │
│                                   │                                    │
│                                   │  ────────────────────────────────  │
│                                   │  Equivalence Class: Mission Data   │
│                                   │  Similarity: 94%                   │
│                                   │  Shared rules: 3                   │
│                                   │                                    │
│                                   │  Rules:                            │
│                                   │  • mission_* -> mission_data       │
│                                   │  • *.log -> logs                   │
│                                   │  • *.csv -> telemetry              │
│                                   │                                    │
├───────────────────────────────────┴────────────────────────────────────┤
│ [n] Add  [d] Remove  [s] Scan  [w] Watch  [e] Edit  [c] Class  [?] Help│
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Breakdown

| Component | Size | Purpose |
|-----------|------|---------|
| Header | 1 line | Breadcrumb, help hint |
| Tree Panel | 40% width | Hierarchical source list |
| Details Panel | 60% width | Selected item details |
| Footer | 1 line | Context-sensitive action hints |

### 3.3 Tree Node Types

| Node Type | Icon | Indicator | Example |
|-----------|------|-----------|---------|
| Equivalence Class | `▼`/`▶` | None | `▼ Mission Data (3)` |
| Source (healthy) | `├─` | `●` Green | `├─● /data/alpha` |
| Source (stale) | `├─` | `○` Gray | `├─○ /data/bravo` |
| Source (warning) | `├─` | `⚠` Yellow | `├─⚠ /data/clinic` |
| Source (error) | `├─` | `✗` Red | `├─✗ /data/broken` |
| Source (watching) | `├─` | `↻` Blue | `├─↻ /data/live` |
| Standalone Group | `▼`/`▶` | None | `▼ Standalone` |

### 3.4 Source Status Indicators

| Indicator | Meaning | Condition |
|-----------|---------|-----------|
| `●` | Healthy | Scanned within 24h, no errors |
| `○` | Stale | Not scanned in >7 days |
| `⚠` | Warning | Some extraction failures |
| `✗` | Error | Scan failed or path inaccessible |
| `↻` | Watching | Watch mode enabled |

### 3.5 Details Panel - Source Selected

```
Source: /data/mission_alpha
────────────────────────────────────────

Status: Healthy
Last scan: 2 hours ago
Watch mode: Enabled

────────────────────────────────────────
Files
Total: 1,247
Tagged: 89% (1,110)
Errors: 0

────────────────────────────────────────
Equivalence Class: Mission Data
Similarity: 94%
Shared rules: 3

Rules:
• mission_* -> mission_data
• *.log -> logs
• *.csv -> telemetry
```

### 3.6 Details Panel - Class Selected

```
Equivalence Class: Mission Data
────────────────────────────────────────

Members: 3 sources
Total files: 3,741

────────────────────────────────────────
Sources (by similarity)
/data/mission_alpha    94%  1,247 files
/data/mission_bravo    91%    892 files
/data/mission_charlie  89%  1,602 files

────────────────────────────────────────
Shared Rules (3)
• mission_* -> mission_data
• *.log -> logs
• *.csv -> telemetry

────────────────────────────────────────
Fingerprint
Avg depth: 4.2
Extensions: .csv (67%), .log (22%), .txt (11%)
Date format: ISO (YYYY-MM-DD)
```

### 3.7 Responsive Behavior

| Terminal Width | Adaptation |
|----------------|------------|
| >= 100 cols | Full layout with details |
| 80-99 cols | Narrower details, truncated paths |
| < 80 cols | Tree only, Enter shows details in overlay |

| Terminal Height | Adaptation |
|-----------------|------------|
| >= 30 rows | Full layout |
| 20-29 rows | Reduced tree items visible |
| < 20 rows | Minimal view, scroll to see all |

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
                │              TREE_VIEW               │◄────────────────┐
                │        (default sources state)       │                 │
                │                                      │                 │
                └───┬──────────┬──────────┬───────────┘                 │
                    │          │          │                              │
               'n'  │     'c'  │     'd'  │                              │
                    ▼          ▼          ▼                              │
            ┌───────────┐ ┌───────────┐ ┌───────────┐                   │
            │   ADD     │ │   CLASS   │ │  CONFIRM  │                   │
            │  DIALOG   │ │  MANAGER  │ │  DIALOG   │                   │
            └─────┬─────┘ └─────┬─────┘ └─────┬─────┘                   │
                  │             │             │                          │
            Esc/  │       Esc   │       Esc/  │                          │
            Done  │             │       Enter │                          │
                  └─────────────┴─────────────┴──────────────────────────┘
```

### 4.2 State Definitions

| State | Description | Entry Condition |
|-------|-------------|-----------------|
| LOADING | Fetching sources and classes | View initialized |
| TREE_VIEW | Main state, browsing tree | Data loaded |
| ADD_DIALOG | Add source dialog open | Press 'n' |
| CLASS_MANAGER | Class management dialog | Press 'c' on class |
| CONFIRM_DIALOG | Confirmation for destructive action | Press 'd', 'D', 'x' |
| EQUIVALENCE_PROMPT | Equivalence detection prompt | After add scan |
| MOVE_DIALOG | Move source to class | Press 'm' |
| RULE_EDITOR | Edit rules for source/class | Press 'e' |

### 4.3 State Transitions

| From | Event | To | Side Effects |
|------|-------|-----|--------------|
| LOADING | Data ready | TREE_VIEW | Render tree |
| LOADING | Error | TREE_VIEW | Show error toast |
| TREE_VIEW | 'n' pressed | ADD_DIALOG | Open add dialog |
| TREE_VIEW | 'c' on class | CLASS_MANAGER | Open class manager |
| TREE_VIEW | 'd' pressed | CONFIRM_DIALOG | Show remove confirmation |
| TREE_VIEW | 'D' pressed | CONFIRM_DIALOG | Show detach confirmation |
| TREE_VIEW | 'm' pressed | MOVE_DIALOG | Open class selector |
| TREE_VIEW | 'e' pressed | RULE_EDITOR | Open rule editor |
| TREE_VIEW | 's' pressed | TREE_VIEW | Start scan (inline progress) |
| TREE_VIEW | 'w' pressed | TREE_VIEW | Toggle watch mode |
| ADD_DIALOG | Esc pressed | TREE_VIEW | Close dialog |
| ADD_DIALOG | Path submitted | EQUIVALENCE_PROMPT or TREE_VIEW | Scan, check equivalence |
| EQUIVALENCE_PROMPT | 'Y' pressed | TREE_VIEW | Join class, add source |
| EQUIVALENCE_PROMPT | 'n' pressed | TREE_VIEW | Add as standalone |
| EQUIVALENCE_PROMPT | Esc pressed | ADD_DIALOG | Back to add dialog |
| CLASS_MANAGER | Esc pressed | TREE_VIEW | Close manager |
| CLASS_MANAGER | 'x' pressed | CONFIRM_DIALOG | Show delete class confirmation |
| CONFIRM_DIALOG | Esc pressed | TREE_VIEW | Cancel action |
| CONFIRM_DIALOG | Enter pressed | TREE_VIEW | Execute action, refresh |
| MOVE_DIALOG | Esc pressed | TREE_VIEW | Cancel move |
| MOVE_DIALOG | Enter pressed | TREE_VIEW | Move source, refresh |
| RULE_EDITOR | Esc pressed | TREE_VIEW | Close editor |

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc, r for refresh) are defined in `specs/tui.md`.

### 5.1 Tree View State

| Key | Action | Description |
|-----|--------|-------------|
| `n` | Add source | Open add source dialog |
| `d` | Remove source | Remove selected source (confirmation) |
| `s` | Scan source | Rescan selected source |
| `w` | Toggle watch | Toggle watch mode for source |
| `e` | Edit rules | Edit source or class rules |
| `c` | Manage class | Open class management (on class node) |
| `m` | Move source | Move source to different class |
| `D` | Detach | Detach source from class (Shift+d) |
| `C` | Create class | Create class from selection (Shift+c) |
| `j` / `↓` | Next item | Move selection down |
| `k` / `↑` | Previous item | Move selection up |
| `Enter` | Expand/collapse | Toggle tree node expansion |
| `Space` | Toggle select | Multi-select for batch operations |
| `Tab` | Switch panel | Move focus between tree and details |
| `g` | First item | Jump to first item |
| `G` | Last item | Jump to last item |
| `/` | Filter | Filter tree by path or class name |

### 5.2 Add Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Next field | Move between path and watch checkbox |
| `Enter` | Submit | Add source |
| `Esc` | Cancel | Close dialog |
| `Space` | Toggle watch | Toggle "Watch for changes" checkbox |

### 5.3 Class Manager State

| Key | Action | Description |
|-----|--------|-------------|
| `e` | Edit rules | Edit shared class rules |
| `r` | Rename | Rename equivalence class |
| `x` | Delete class | Delete class (sources become standalone) |
| `Esc` | Close | Close manager |
| `j` / `↓` | Next member | Move selection in member list |
| `k` / `↑` | Previous member | Move selection up |

### 5.4 Move Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `↓` | Next class | Move selection down |
| `k` / `↑` | Previous class | Move selection up |
| `Enter` | Select class | Move source to selected class |
| `Esc` | Cancel | Close dialog |
| Type | Filter | Filter classes by name |

### 5.5 Confirm Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Confirm | Execute the action |
| `Esc` | Cancel | Close dialog, no action |
| `Tab` | Switch button | Move between Confirm/Cancel buttons |

---

## 6. Data Model

### 6.1 View State

```rust
pub struct SourcesViewState {
    /// Current UI state
    pub state: SourcesState,

    /// Tree structure
    pub tree: SourceTree,

    /// Currently selected node
    pub selected: Option<TreeNodeId>,

    /// Multi-selected nodes (for batch operations)
    pub multi_selected: HashSet<TreeNodeId>,

    /// Scroll offset in tree
    pub tree_scroll: usize,

    /// Which panel has focus
    pub focused_panel: SourcesPanel,

    /// Dialog-specific state
    pub dialog: Option<SourcesDialog>,

    /// Filter text for tree
    pub filter: String,

    /// Last refresh timestamp
    pub last_refresh: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourcesState {
    Loading,
    TreeView,
    AddDialog,
    ClassManager,
    ConfirmDialog,
    EquivalencePrompt,
    MoveDialog,
    RuleEditor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourcesPanel {
    Tree,
    Details,
}

#[derive(Debug)]
pub enum SourcesDialog {
    Add(AddSourceDialogState),
    ClassManager(ClassManagerState),
    Confirm(ConfirmDialogState),
    EquivalencePrompt(EquivalencePromptState),
    Move(MoveDialogState),
    RuleEditor(RuleEditorState),
}
```

### 6.2 Tree Model

```rust
pub struct SourceTree {
    pub roots: Vec<TreeNode>,
}

pub struct TreeNode {
    pub id: TreeNodeId,
    pub node_type: TreeNodeType,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone)]
pub enum TreeNodeId {
    Class(Uuid),
    Source(Uuid),
    StandaloneGroup,
}

#[derive(Debug, Clone)]
pub enum TreeNodeType {
    EquivalenceClass(EquivalenceClassInfo),
    Source(SourceInfo),
    StandaloneGroup,
}

pub struct EquivalenceClassInfo {
    pub id: Uuid,
    pub name: String,
    pub member_count: u32,
    pub total_files: u32,
    pub shared_rules: u32,
}

pub struct SourceInfo {
    pub id: Uuid,
    pub path: PathBuf,
    pub status: SourceStatus,
    pub file_count: u32,
    pub tagged_count: u32,
    pub tagged_percent: u8,
    pub last_scan: Option<DateTime<Utc>>,
    pub watch_enabled: bool,
    pub error_count: u32,
    pub class_id: Option<Uuid>,
    pub similarity: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceStatus {
    Healthy,    // ●  Scanned recently, no errors
    Stale,      // ○  Not scanned in >7 days
    Warning,    // ⚠  Some extraction failures
    Error,      // ✗  Scan failed or path inaccessible
    Scanning,   // ↻  Currently scanning
}

impl SourceStatus {
    pub fn indicator(&self) -> char {
        match self {
            Self::Healthy => '●',
            Self::Stale => '○',
            Self::Warning => '⚠',
            Self::Error => '✗',
            Self::Scanning => '↻',
        }
    }
}
```

### 6.3 Dialog State Models

```rust
pub struct AddSourceDialogState {
    pub path: String,
    pub path_cursor: usize,
    pub watch: bool,
    pub focused_field: AddSourceField,
    pub error: Option<String>,
    pub is_scanning: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddSourceField {
    Path,
    Watch,
}

pub struct ClassManagerState {
    pub class: EquivalenceClassInfo,
    pub members: Vec<SourceInfo>,
    pub selected_member: usize,
    pub rules: Vec<RuleInfo>,
}

pub struct EquivalencePromptState {
    pub new_source_path: PathBuf,
    pub matched_class: EquivalenceClassInfo,
    pub similarity: f32,
    pub other_members: Vec<PathBuf>,
}

pub struct MoveDialogState {
    pub source: SourceInfo,
    pub classes: Vec<EquivalenceClassInfo>,
    pub selected_class: usize,
    pub filter: String,
}

pub struct ConfirmDialogState {
    pub action: ConfirmAction,
    pub message: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    RemoveSource(Uuid),
    DetachSource(Uuid),
    DeleteClass(Uuid),
    ClearWatch(Uuid),
}

pub struct RuleInfo {
    pub name: String,
    pub glob: String,
    pub tag: Option<String>,
    pub is_class_rule: bool,
}
```

---

## 7. Data Sources

| Widget | Query | Refresh |
|--------|-------|---------|
| Source tree | See 7.2 | 5s |
| Source details | See 7.3 | On selection |
| Class details | See 7.4 | On selection |
| File counts | See 7.5 | 5s |
| Rules | See 7.6 | On demand |

### 7.2 Source Tree Query

```sql
-- Sources with their equivalence class
SELECT
    s.id,
    s.path,
    s.last_scan,
    s.watch_enabled,
    ec.id as class_id,
    ec.name as class_name,
    em.similarity
FROM scout_sources s
LEFT JOIN equivalence_members em ON s.id = em.source_id
LEFT JOIN equivalence_classes ec ON em.class_id = ec.id
ORDER BY
    COALESCE(ec.name, 'ZZZZ') ASC,  -- Classes first, then Standalone
    s.path ASC;
```

### 7.3 Source Details Query

```sql
SELECT
    s.id,
    s.path,
    s.last_scan,
    s.watch_enabled,
    s.scan_error,
    COUNT(f.id) as file_count,
    SUM(CASE WHEN f.tag IS NOT NULL THEN 1 ELSE 0 END) as tagged_count,
    SUM(CASE WHEN f.extraction_status = 'FAILED' THEN 1 ELSE 0 END) as error_count
FROM scout_sources s
LEFT JOIN scout_files f ON s.id = f.source_id
WHERE s.id = :source_id
GROUP BY s.id;
```

### 7.4 Class Details Query

```sql
SELECT
    ec.id,
    ec.name,
    ec.fingerprint,
    ec.created_at,
    COUNT(DISTINCT em.source_id) as member_count,
    COUNT(f.id) as total_files,
    (SELECT COUNT(*) FROM extraction_rules er
     WHERE er.source_id IN (SELECT source_id FROM equivalence_members WHERE class_id = ec.id)
       AND er.is_class_rule = TRUE) as shared_rules
FROM equivalence_classes ec
LEFT JOIN equivalence_members em ON ec.id = em.class_id
LEFT JOIN scout_sources s ON em.source_id = s.id
LEFT JOIN scout_files f ON s.id = f.source_id
WHERE ec.id = :class_id
GROUP BY ec.id;
```

### 7.5 File Counts Query

```sql
SELECT
    source_id,
    COUNT(*) as total,
    SUM(CASE WHEN tag IS NOT NULL THEN 1 ELSE 0 END) as tagged,
    SUM(CASE WHEN extraction_status = 'FAILED' THEN 1 ELSE 0 END) as errors
FROM scout_files
GROUP BY source_id;
```

### 7.6 Rules Query

```sql
SELECT
    er.id,
    er.name,
    er.glob_pattern,
    er.tag,
    er.is_class_rule,
    er.priority
FROM extraction_rules er
WHERE er.source_id = :source_id
   OR er.source_id IN (
       SELECT em2.source_id
       FROM equivalence_members em1
       JOIN equivalence_members em2 ON em1.class_id = em2.class_id
       WHERE em1.source_id = :source_id
         AND er.is_class_rule = TRUE
   )
ORDER BY er.priority ASC, er.name ASC;
```

---

## 8. Implementation Notes

### 8.1 Tree Rendering

```rust
fn render_tree_node(node: &TreeNode, indent: usize, frame: &mut Frame, area: Rect) {
    let prefix = "  ".repeat(indent);
    let expand_icon = if node.children.is_empty() {
        " "
    } else if node.expanded {
        "▼"
    } else {
        "▶"
    };

    match &node.node_type {
        TreeNodeType::EquivalenceClass(class) => {
            let line = format!("{}{} {} ({})",
                prefix, expand_icon, class.name, class.member_count);
            // Render with normal style
        }
        TreeNodeType::Source(source) => {
            let connector = if is_last_child { "└─" } else { "├─" };
            let indicator = source.status.indicator();
            let path = source.path.file_name().unwrap_or_default();
            let line = format!("{}{}{} {}", prefix, connector, indicator, path);
            // Render with status-appropriate color
        }
        TreeNodeType::StandaloneGroup => {
            let line = format!("{}{} Standalone", prefix, expand_icon);
            // Render with muted style
        }
    }
}
```

### 8.2 Refresh Strategy

- **Tree structure**: Refresh every 5 seconds
- **File counts**: Included in tree refresh
- **Source details**: Refresh on selection change
- **During scan**: Refresh every 500ms for progress
- **Manual refresh**: `r` key (global keybinding)

### 8.3 Equivalence Detection

When adding a new source, fingerprint comparison happens:

```rust
async fn check_equivalence(new_source: &Path, db: &SqlitePool) -> Option<EquivalenceMatch> {
    let fingerprint = compute_fingerprint(new_source).await?;

    let classes: Vec<EquivalenceClass> = sqlx::query_as(
        "SELECT id, name, fingerprint FROM equivalence_classes"
    ).fetch_all(db).await.ok()?;

    for class in classes {
        let similarity = compare_fingerprints(&fingerprint, &class.fingerprint);
        if similarity >= 0.80 {
            return Some(EquivalenceMatch {
                class_id: class.id,
                class_name: class.name,
                similarity,
            });
        }
    }
    None
}
```

### 8.4 Watch Mode Implementation

Watch mode uses platform-specific file watchers:

```rust
impl SourcesView {
    fn toggle_watch(&mut self, source_id: Uuid) {
        if let Some(source) = self.find_source_mut(source_id) {
            source.watch_enabled = !source.watch_enabled;

            if source.watch_enabled {
                // Start file watcher (background task)
                self.start_watcher(source_id, &source.path);
            } else {
                // Stop file watcher
                self.stop_watcher(source_id);
            }

            // Persist to database
            self.save_watch_state(source_id, source.watch_enabled);
        }
    }
}
```

### 8.5 Multi-Selection

Space key toggles multi-selection for batch operations:

```rust
fn handle_space(&mut self) {
    if let Some(selected) = &self.state.selected {
        if self.state.multi_selected.contains(selected) {
            self.state.multi_selected.remove(selected);
        } else {
            self.state.multi_selected.insert(selected.clone());
        }
    }
}

fn can_create_class(&self) -> bool {
    // Need 2+ sources selected, all must be standalone or same class
    self.state.multi_selected.len() >= 2
        && self.state.multi_selected.iter().all(|id| {
            matches!(id, TreeNodeId::Source(_))
        })
}
```

### 8.6 View Trait Implementation

```rust
impl View for SourcesView {
    fn name(&self) -> &'static str { "Sources" }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.state.state {
            SourcesState::Loading => self.render_loading(frame, area),
            SourcesState::TreeView => self.render_tree_view(frame, area),
            SourcesState::AddDialog => {
                self.render_tree_view(frame, area);
                self.render_add_dialog(frame, area);
            }
            SourcesState::ClassManager => {
                self.render_tree_view(frame, area);
                self.render_class_manager(frame, area);
            }
            // ... other states
        }
    }

    fn handle_event(&mut self, event: Event) -> ViewAction {
        match &self.state.state {
            SourcesState::TreeView => self.handle_tree_view_event(event),
            SourcesState::AddDialog => self.handle_add_dialog_event(event),
            SourcesState::ClassManager => self.handle_class_manager_event(event),
            SourcesState::ConfirmDialog => self.handle_confirm_dialog_event(event),
            _ => ViewAction::None,
        }
    }

    fn help_text(&self) -> Vec<(&'static str, &'static str)> {
        match self.state.state {
            SourcesState::TreeView => vec![
                ("n", "Add"),
                ("d", "Remove"),
                ("s", "Scan"),
                ("e", "Edit rules"),
                ("c", "Class mgmt"),
            ],
            SourcesState::ClassManager => vec![
                ("e", "Edit rules"),
                ("r", "Rename"),
                ("x", "Delete"),
                ("Esc", "Close"),
            ],
            _ => vec![("Enter", "Confirm"), ("Esc", "Cancel")],
        }
    }

    fn on_enter(&mut self) {
        self.state.state = SourcesState::Loading;
        self.refresh_tree();
    }
}
```

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Expanded from stub: full state machine, data models, 10 workflows, implementation notes |
| 2026-01-12 | 0.1 | Initial stub |

---

### Trade-offs

1. **Tree vs Flat List**: Chose hierarchical tree to visualize equivalence relationships clearly, at cost of more complex navigation code.

2. **Multi-select with Space**: Following vim/ranger pattern for batch operations. Trade-off: Space also used for toggle in other contexts (dialogs).

3. **Detach uses `D` (Shift+d)**: Destructive action needs distinct key to prevent accidental use. Trade-off: less discoverable than lowercase.

4. **Class management as dialog vs separate view**: Chose dialog to keep context visible. Trade-off: less screen space for complex operations.

5. **Watch mode toggle vs always-on**: Explicit toggle gives user control over resource usage. Trade-off: requires manual enablement.

### New Gaps Introduced

1. **GAP-SCHEMA-001**: `extraction_rules` table needs `is_class_rule` boolean column to distinguish shared class rules from source-specific rules. Not present in current `specs/extraction.md` schema.

2. **GAP-SCHEMA-002**: `scout_sources` table needs `watch_enabled` boolean and `scan_error` text columns. Need to verify against current schema.

3. **GAP-IMPL-001**: File watcher implementation details not specified - need to determine cross-platform approach (notify crate recommended).

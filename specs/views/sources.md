# Sources - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.1
**Related:** specs/extraction.md (Equivalence Classes)

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Sources** view manages data sources (directories), equivalence classes, and source-level configuration.

### 1.1 Design Philosophy

- **Hierarchical organization**: Sources grouped by equivalence class
- **Health visibility**: Status at a glance (file counts, scan recency, errors)
- **Rule propagation**: Class rules apply to all member sources
- **Flexible grouping**: Sources can be detached or moved between classes

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
| "Which sources are similar?" | Equivalence classes group similar sources |
| "Apply rules across sources" | Edit class rules, applies to all members |
| "Check source health" | File counts, tagged %, last scan visible |
| "Add new data directory" | `n` opens path picker with equivalence detection |

---

## 2. User Workflows

### 2.1 Browse Sources and Classes

```
1. User navigates to Sources view (press '4')
2. Tree displays equivalence classes with nested sources
3. User navigates with j/k, expands/collapses with Enter
4. Detail panel shows selected source or class info
```

### 2.2 Add New Source

```
1. Press 'n' to open Add Source dialog
2. Enter directory path
3. System scans and detects equivalence
4. If match found: "Join Mission Data class? [Y/n]"
5. Source added, appears in tree
```

### 2.3 Remove Source

```
1. Select source, press 'd'
2. Confirmation shows impact (file count, rules)
3. Confirm to remove from tracking
```

### 2.4 Manual Rescan

```
1. Select source, press 's'
2. Progress indicator in detail panel
3. Stats refresh on completion
```

### 2.5 Toggle Watch Mode

```
1. Select source, press 'w'
2. Watch mode enables/disables
3. Detail panel shows watch status
```

### 2.6 Manage Equivalence Class

```
1. Select class node, press 'c'
2. Class manager shows members, shared rules
3. Options: edit rules, rename (F2), delete class
```

### 2.7 Detach/Move Source

```
1. Select source in a class
2. Press 'D' to detach (becomes standalone)
3. Or press 'm' to move to different class
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
│                                   │                                    │
│                                   │  ────────────────────────────────  │
│                                   │  Equivalence Class: Mission Data   │
│                                   │  Similarity: 94%                   │
│                                   │  Shared rules: 3                   │
│                                   │                                    │
├───────────────────────────────────┴────────────────────────────────────┤
│ [n] Add  [d] Remove  [s] Scan  [w] Watch  [e] Edit rules   [?] Help    │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Source Status Indicators

| Indicator | Meaning | Condition |
|-----------|---------|-----------|
| `●` | Healthy | Scanned within 24h, no errors |
| `○` | Stale | Not scanned in >7 days |
| `⚠` | Warning | Some extraction failures |
| `✗` | Error | Scan failed or path inaccessible |
| `↻` | Scanning | Currently scanning |

---

## 4. State Machine

### 4.1 State Diagram

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                          SOURCES VIEW STATE MACHINE                            │
│                                                                                │
│                              ┌─────────────┐                                   │
│                              │   LOADING   │                                   │
│                              │  (initial)  │                                   │
│                              └──────┬──────┘                                   │
│                                     │ data loaded                              │
│                                     ▼                                          │
│    ┌────────────────────────────────────────────────────────────────────┐     │
│    │                           TREE_VIEW                                 │     │
│    │                          (main state)                               │     │
│    │                                                                     │     │
│    │   Navigate with j/k, expand/collapse with Enter                    │     │
│    │   Tab switches between tree and details panel                       │     │
│    └───────┬─────────────┬─────────────┬─────────────┬──────────────────┘     │
│            │             │             │             │                         │
│        n   │         c   │         d   │         m   │                         │
│            ▼             ▼             ▼             ▼                         │
│    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐            │
│    │ ADD_DIALOG  │ │   CLASS_    │ │  CONFIRM_   │ │   MOVE_     │            │
│    │             │ │   MANAGER   │ │   DIALOG    │ │   DIALOG    │            │
│    └──────┬──────┘ └─────────────┘ └─────────────┘ └─────────────┘            │
│           │                              │              │                      │
│           │ path submitted               │ confirm/Esc  │ Esc                  │
│           ▼                              │              │                      │
│    ┌─────────────┐                       │              │                      │
│    │ EQUIVALENCE │                       │              │                      │
│    │   PROMPT    │                       │              │                      │
│    │  [Y/n]?     │                       │              │                      │
│    └──────┬──────┘                       │              │                      │
│           │ accept/decline               │              │                      │
│           └──────────────────────────────┴──────────────┘                      │
│                                          │                                     │
│                                          ▼                                     │
│                                    TREE_VIEW                                   │
│                                                                                │
│    All dialogs return to TREE_VIEW on Esc                                     │
│    Alt+H returns to HOME_HUB (global)                                         │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 State Definitions

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| LOADING | View entry | Data loaded | Show spinner, fetch sources and classes |
| TREE_VIEW | Data loaded, Esc from dialogs | n/c/d/m keys, Alt+H | Browse tree, view details, navigate |
| ADD_DIALOG | 'n' from TREE_VIEW | Submit path, Esc | Path input field, directory picker |
| CLASS_MANAGER | 'c' on class node | Esc | View/edit class rules, rename, delete |
| CONFIRM_DIALOG | 'd' from TREE_VIEW | Confirm/Esc | Show impact, confirm destructive action |
| EQUIVALENCE_PROMPT | Path submitted in ADD_DIALOG | Accept/Decline | "Join class?" prompt with similarity |
| MOVE_DIALOG | 'm' from TREE_VIEW | Select class/Esc | List available classes for move |

### 4.3 Transitions

| From | To | Trigger | Guard |
|------|----|---------|-------|
| LOADING | TREE_VIEW | Data loaded | — |
| TREE_VIEW | ADD_DIALOG | 'n' | — |
| TREE_VIEW | CLASS_MANAGER | 'c' | Class node selected |
| TREE_VIEW | CONFIRM_DIALOG | 'd' | Source selected |
| TREE_VIEW | MOVE_DIALOG | 'm' | Source selected |
| ADD_DIALOG | EQUIVALENCE_PROMPT | Submit path | Match found (≥80% similarity) |
| ADD_DIALOG | TREE_VIEW | Submit path | No match found |
| ADD_DIALOG | TREE_VIEW | Esc | — |
| EQUIVALENCE_PROMPT | TREE_VIEW | Accept/Decline | — |
| CLASS_MANAGER | TREE_VIEW | Esc | — |
| CONFIRM_DIALOG | TREE_VIEW | Confirm/Esc | — |
| MOVE_DIALOG | TREE_VIEW | Select/Esc | — |
| any | HOME_HUB | Alt+H | — (global) |

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc, r for refresh) are defined in `specs/tui.md`.

### 5.1 Tree View State

| Key | Action | Description |
|-----|--------|-------------|
| `n` | Add source | Open add source dialog |
| `d` | Remove source | Remove selected source |
| `s` | Scan source | Rescan selected source |
| `w` | Toggle watch | Toggle watch mode |
| `e` | Edit rules | Edit source or class rules |
| `c` | Manage class | Open class management (on class) |
| `m` | Move source | Move source to different class |
| `D` | Detach | Detach source from class (Shift+d) |
| `C` | Create class | Create class from selection (Shift+c) |
| `j` / `↓` | Next item | Move selection down |
| `k` / `↑` | Previous item | Move selection up |
| `Enter` | Expand/collapse | Toggle tree node |
| `Space` | Toggle select | Multi-select for batch ops |
| `Tab` | Switch panel | Between tree and details |

### 5.2 Class Manager State

| Key | Action | Description |
|-----|--------|-------------|
| `e` | Edit rules | Edit shared class rules |
| `F2` | Rename | Rename equivalence class |
| `x` | Delete class | Delete class (sources → standalone) |
| `Esc` | Close | Close manager |

---

## 6. Data Model

```rust
pub struct SourcesViewState {
    pub state: SourcesState,
    pub tree: SourceTree,
    pub selected: Option<TreeNodeId>,
    pub multi_selected: HashSet<TreeNodeId>,
    pub focused_panel: SourcesPanel,
    pub dialog: Option<SourcesDialog>,
    pub last_refresh: DateTime<Utc>,
}

pub struct SourceInfo {
    pub id: Uuid,
    pub path: PathBuf,
    pub status: SourceStatus,
    pub file_count: u32,
    pub tagged_percent: u8,
    pub last_scan: Option<DateTime<Utc>>,
    pub watch_enabled: bool,
    pub class_id: Option<Uuid>,
    pub similarity: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceStatus {
    Healthy,    // ●
    Stale,      // ○
    Warning,    // ⚠
    Error,      // ✗
    Scanning,   // ↻
}
```

---

## 7. Data Sources

### 7.1 Source Tree Query

```sql
SELECT
    s.id, s.path, s.last_scan, s.watch_enabled,
    ec.id as class_id, ec.name as class_name, em.similarity
FROM scout_sources s
LEFT JOIN equivalence_members em ON s.id = em.source_id
LEFT JOIN equivalence_classes ec ON em.class_id = ec.id
ORDER BY COALESCE(ec.name, 'ZZZZ') ASC, s.path ASC;
```

---

## 8. Implementation Notes

### 8.1 Refresh Strategy

- Tree structure: 5s refresh
- During scan: 500ms for progress
- On selection change: immediate detail refresh

### 8.2 Equivalence Detection

When adding source, compare fingerprint against existing classes. >80% similarity triggers join prompt.

### 8.3 Multi-Selection

Space toggles selection. Used for batch operations like creating a new class from multiple standalone sources.

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.1 | Added ASCII state machine diagram (Section 4.1) per spec refinement v2.3 |
| 2026-01-13 | 1.1 | Expanded state definitions table with Entry/Exit/Behavior columns |
| 2026-01-13 | 1.1 | Added Guards column to transitions table |
| 2026-01-12 | 1.0 | Expanded from stub: state machine, data models, workflows |
| 2026-01-12 | 0.1 | Initial stub |

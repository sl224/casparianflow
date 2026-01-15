# Home - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0
**Related:** specs/views/discover.md, specs/views/jobs.md, specs/views/parser_bench.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Home** view is the navigation hub and status dashboard. Users land here on startup and can jump to any other view. It provides at-a-glance system health, recent activity, and quick access to common actions.

### 1.1 Design Philosophy

- **Landing pad**: First thing users see, orients them to system state
- **Navigation hub**: Fast access to all views via number keys
- **Status dashboard**: Quick health check without drilling down
- **Quick actions**: Common tasks accessible without navigation
- **Zero friction**: No modals on startup, immediate utility

### 1.2 Core Entities

```
~/.casparian_flow/casparian_flow.sqlite3

Tables queried:
├── scout_sources        # Count of configured sources
├── scout_files          # File counts, tag percentages
├── cf_parsers           # Parser registry
└── cf_job_status        # Running, completed, failed jobs
```

**Note:** Activity log is derived from existing tables (see Section 7).

### 1.3 User Goals

| Goal | How Home Helps |
|------|----------------|
| "What's happening?" | Status tiles show job counts, file stats |
| "Where do I go?" | Number keys (1-4) jump to any view |
| "What just happened?" | Recent activity log shows last 10 events |
| "Quick task" | `s` scans, `t` tests, `R` shows recent files |

---

## 2. User Workflows

### 2.1 First-Time User: Orientation

```
1. User launches TUI for first time
2. Home view displays with empty/minimal stats:
   - Sources: 0 configured
   - Files: 0 discovered
   - Parsers: 0 registered
   - Jobs: No activity
3. First-time banner appears:
   ┌─────────────────────────────────────────────────┐
   │  Welcome! Get started:                          │
   │  [s] Scan a directory  [?] Help                 │
   └─────────────────────────────────────────────────┘
4. User presses 's' to scan first source
5. Scan dialog opens (see workflow 2.4)
```

**First-time detection:**
```rust
let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scout_sources")
    .fetch_one(db).await.unwrap_or(0);
let is_first_time = count == 0;
```

### 2.2 Returning User: Navigation Flow

```
1. User launches TUI, Home displays with populated stats
2. User sees: "2 running jobs, 1 failed"
3. User presses '3' to jump to Jobs view
4. After investigating, user presses '0' or 'H' to return Home
5. Home state preserved (scroll position, etc.)
```

### 2.3 Dashboard Check: Quick Health Assessment

```
1. User opens TUI to check system status
2. Home displays:
   - Discover: 12 sources, 1,247 files, 89% tagged
   - Parser Bench: 8 parsers, 3 active jobs
   - Jobs: 2 running, 1 failed
   - Sources: 12 configured, 3 equivalence classes
3. User notices "1 failed" in Jobs tile
4. User presses '3' to investigate
   OR user presses Enter with Jobs tile selected
```

### 2.4 Quick Action: Scan New Source

```
1. User presses 's' from Home
2. Scan Source dialog opens:
   ┌─ Scan Source ─────────────────────────────────┐
   │ Path: /Users/data/reports█                    │
   │ Hint: Enter directory path or drag folder     │
   │                                               │
   │ Tag (optional): ________                      │
   │ Hint: Auto-tag files with this tag           │
   │                                               │
   │ [ ] Watch for changes                         │
   │                                               │
   │ [Enter] Scan  [Esc] Cancel                    │
   └───────────────────────────────────────────────┘
3. User enters path and optional tag
4. Scan executes, progress shown inline
5. Dialog closes, stats refresh
6. Toast: "Scanned 142 files from /Users/data/reports"
```

### 2.5 Quick Action: Test Parser

```
1. User presses 't' from Home
2. Quick Test dialog opens:
   ┌─ Quick Test ──────────────────────────────────┐
   │ Parser: _______________▼                      │
   │         sales_parser (v1.0.0)                 │
   │         invoice_parser (v2.1.0)               │
   │         log_parser (v1.0.0)                   │
   │                                               │
   │ Input: _______________                        │
   │ Hint: Path to test file                       │
   │                                               │
   │ [Enter] Run Test  [Esc] Cancel                │
   └───────────────────────────────────────────────┘
3. User selects parser and input file
4. Test runs, results shown
5. Dialog offers: [v] View full results  [Enter] Done
```

### 2.6 Quick Action: Recent Files

```
1. User presses 'R' from Home
2. Recent Files panel expands (replaces activity log):
   ┌─ Recent Files ────────────────────────────────┐
   │ 10:32  /data/reports/q4.csv         [sales]   │
   │ 10:28  /data/logs/access.log        [logs]    │
   │ 10:15  /data/invoices/jan.pdf       [invoices]│
   │ 10:01  /data/reports/q3.csv         [sales]   │
   │ ...                                            │
   │                                               │
   │ [Enter] Open in Discover  [Esc] Back to Home  │
   └───────────────────────────────────────────────┘
3. User navigates with j/k, Enter opens file in Discover
4. Esc returns to normal Home view
```

### 2.7 Tile Navigation and Selection

```
1. User on Home view, first tile (Discover) selected
2. User presses Tab or arrow keys to move between tiles
3. Selected tile shows highlight/focus indicator
4. User presses Enter to open selected view
   (equivalent to pressing the number key)
5. Alternatively, user presses number key directly
```

---

## 3. Layout Specification

### 3.1 Full Layout

```
┌─ Casparian Flow ───────────────────────────────────────────────────────┐
│ Home                                                        [?] Help   │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│                      Welcome to Casparian Flow                         │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌─ [1] Discover ──────────────┐   ┌─ [2] Parser Bench ─────────────┐ │
│   │  ● 12 sources               │   │  ● 8 parsers                   │ │
│   │    1,247 files discovered   │   │    3 active jobs               │ │
│   │    89% tagged               │   │    2 pending backfill          │ │
│   └─────────────────────────────┘   └────────────────────────────────┘ │
│                                                                        │
│   ┌─ [3] Jobs ──────────────────┐   ┌─ [4] Sources ──────────────────┐ │
│   │  ↻ 2 running                │   │  ● 12 configured               │ │
│   │  ✗ 1 failed                 │   │    3 equivalence classes       │ │
│   │  ✓ 47 completed today       │   │    0 errors                    │ │
│   └─────────────────────────────┘   └────────────────────────────────┘ │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│  Recent Activity                                                       │
│  ───────────────                                                       │
│  10:32  Parser "sales" completed (1,247 files)                     ●   │
│  10:28  Source "/data/reports" scanned                             ●   │
│  10:15  Rule "*.csv -> sales" created                              ●   │
│  09:58  Job #47 failed: schema violation                           ✗   │
│  09:45  Parser "invoice_parser" updated to v2.1.0                  ●   │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│ [s] Scan source  [t] Quick test  [R] Recent files       [1-4] Views   │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Breakdown

| Component | Height | Purpose |
|-----------|--------|---------|
| Header | 1 line | View title, help hint |
| Welcome banner | 3 lines | Branding, first-time guidance |
| Status tiles | 8 lines | 2x2 grid of view summaries |
| Activity log | 6+ lines | Recent events, scrollable |
| Footer | 1 line | Quick action hints |

### 3.3 Status Tile Design

Each tile follows this pattern:

```
┌─ [N] View Name ─────────────────┐
│  ● Primary metric               │  <- Status indicator + main stat
│    Secondary metric             │  <- Supporting detail
│    Tertiary metric              │  <- Additional context
└─────────────────────────────────┘
```

**Tile status indicators:** (per tui.md Section 5.3)
| Indicator | Meaning | Color |
|-----------|---------|-------|
| `●` | Healthy / Active | Green |
| `○` | Empty / Inactive | Gray |
| `↻` | In progress | Blue |
| `✗` | Error / Failed | Red |
| `⚠` | Warning | Yellow |

### 3.4 Activity Log Entry Format

```
HH:MM  Description                                             [indicator]
│      │                                                              │
│      └── Natural language description                               │
└── Timestamp (today only, else date)                          Status icon
```

**Entry types:**
| Type | Icon | Example |
|------|------|---------|
| Success | `✓` | Parser completed, scan finished |
| Failure | `✗` | Job failed, schema violation |
| Info | `○` | Rule created, parser updated |
| Warning | `⚠` | Approaching quota, stale source |

### 3.5 First-Time Banner

Shown when `scout_sources` is empty:

```
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌─ Get Started ────────────────────────────────────────────────────┐ │
│   │                                                                  │ │
│   │   No sources configured yet. Scan a directory to discover files.│ │
│   │                                                                  │ │
│   │   [s] Scan a directory    [?] View help    [q] Quit             │ │
│   │                                                                  │ │
│   └──────────────────────────────────────────────────────────────────┘ │
│                                                                        │
```

### 3.6 Responsive Behavior

| Terminal Width | Adaptation |
|----------------|------------|
| >= 100 cols | Full 2x2 tile grid |
| 80-99 cols | Compact tiles, shorter descriptions |
| < 80 cols | Stacked tiles (1 column), abbreviated stats |

| Terminal Height | Adaptation |
|-----------------|------------|
| >= 30 rows | Full layout with activity log |
| 20-29 rows | Reduced activity log (3 entries) |
| < 20 rows | Tiles only, no activity log |

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
                │              DASHBOARD               │◄────────────┐
                │         (default home state)         │             │
                │                                      │             │
                └───┬──────────┬──────────┬───────────┘             │
                    │          │          │                          │
                's' │      't' │      'R' │                          │
                    ▼          ▼          ▼                          │
            ┌───────────┐ ┌───────────┐ ┌───────────┐               │
            │   SCAN    │ │   TEST    │ │  RECENT   │               │
            │  DIALOG   │ │  DIALOG   │ │   FILES   │               │
            └─────┬─────┘ └─────┬─────┘ └─────┬─────┘               │
                  │             │             │                      │
            Esc/  │       Esc/  │       Esc   │                      │
            Done  │       Done  │             │                      │
                  └─────────────┴─────────────┴──────────────────────┘
```

**Navigation Exit:** When user presses '1-4' or Enter on a tile, the view returns `ViewAction::Navigate(ViewId)`. The App layer handles the actual navigation. Home view resets to DASHBOARD state when `on_enter()` is called upon return.

### 4.2 State Definitions

| State | Description | Entry Condition |
|-------|-------------|-----------------|
| LOADING | Fetching stats from database | View initialized |
| DASHBOARD | Main home state, tiles + activity | Data loaded |
| SCAN_DIALOG | Scan source dialog open | Press 's' |
| TEST_DIALOG | Quick test dialog open | Press 't' |
| RECENT_FILES | Recent files panel expanded | Press 'R' |

**Note:** In LOADING state, all key events are ignored. Per tui.md performance targets, LOADING should complete in <500ms.

### 4.3 State Transitions

| From | Event | To | Side Effects |
|------|-------|-----|--------------|
| LOADING | Data ready | DASHBOARD | Render tiles |
| LOADING | Error | DASHBOARD | Show error toast |
| DASHBOARD | 's' pressed | SCAN_DIALOG | Open dialog |
| DASHBOARD | 't' pressed | TEST_DIALOG | Open dialog |
| DASHBOARD | 'R' pressed | RECENT_FILES | Expand panel |
| DASHBOARD | '1-4' pressed | DASHBOARD | Return Navigate action |
| DASHBOARD | Enter pressed | DASHBOARD | Return Navigate action |
| SCAN_DIALOG | Esc pressed | DASHBOARD | Close dialog |
| SCAN_DIALOG | Scan complete | DASHBOARD | Refresh stats, show toast |
| TEST_DIALOG | Esc pressed | DASHBOARD | Close dialog |
| TEST_DIALOG | Test complete | DASHBOARD | Show results toast |
| RECENT_FILES | Esc pressed | DASHBOARD | Collapse panel |
| RECENT_FILES | Enter pressed | DASHBOARD | Return Navigate action with file context |

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc) are defined in `specs/tui.md`.
> These are additional keybindings specific to the Home view.

### 5.1 Dashboard State

| Key | Action | Description |
|-----|--------|-------------|
| `s` | Scan source | Open scan source dialog |
| `t` | Quick test | Open quick test dialog |
| `R` | Recent files | Expand recent files panel |
| `r` | Refresh | Manual refresh (global, per tui.md) |
| `Tab` | Next tile | Move selection to next tile |
| `Shift+Tab` | Previous tile | Move selection to previous tile |
| `Enter` | Open selected | Navigate to selected tile's view |
| `j` / `↓` | Scroll activity | Scroll activity log down |
| `k` / `↑` | Scroll activity | Scroll activity log up |

### 5.2 Scan Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Next field | Move between path, tag, watch checkbox |
| `Enter` | Submit | Execute scan |
| `Esc` | Cancel | Close dialog, return to dashboard |
| `Space` | Toggle watch | Toggle "Watch for changes" checkbox |

### 5.3 Test Dialog State

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Next field | Move between parser dropdown, input field |
| `Enter` | Run test | Execute parser test |
| `Esc` | Cancel | Close dialog, return to dashboard |
| `↑` / `↓` | Navigate dropdown | Select parser in dropdown |

### 5.4 Recent Files State

| Key | Action | Description |
|-----|--------|-------------|
| `j` / `↓` | Next file | Move to next file in list |
| `k` / `↑` | Previous file | Move to previous file |
| `Enter` | Open in Discover | Navigate to Discover, focused on file |
| `Esc` | Close | Return to dashboard |
| `g` | First file | Jump to first file |
| `G` | Last file | Jump to last file |

---

## 6. Data Model

### 6.1 View State

```rust
/// Main state for the Home view
pub struct HomeViewState {
    /// Current UI state
    pub state: HomeState,

    /// Which tile is currently selected (0-3)
    pub selected_tile: usize,

    /// Dashboard statistics
    pub stats: DashboardStats,

    /// Recent activity entries
    pub activity_log: Vec<ActivityEntry>,

    /// Scroll offset for activity log
    pub activity_scroll: usize,

    /// Whether this is first launch (no sources)
    pub is_first_time: bool,

    /// Dialog-specific state (only valid when state is ScanDialog or TestDialog)
    /// Invariant: dialog.is_some() iff state is ScanDialog or TestDialog
    pub dialog: Option<HomeDialog>,

    /// Recent files list (for 'R' action)
    pub recent_files: Vec<RecentFile>,

    /// Selected index in recent files
    pub recent_files_index: usize,

    /// Last refresh timestamp
    pub last_refresh: DateTime<Utc>,
}

/// UI state enum
#[derive(Debug, Clone, PartialEq)]
pub enum HomeState {
    Loading,
    Dashboard,
    ScanDialog,   // dialog must be Some(Scan(...))
    TestDialog,   // dialog must be Some(Test(...))
    RecentFiles,
}

/// Dialog variants - holds dialog-specific data
#[derive(Debug)]
pub enum HomeDialog {
    Scan(ScanDialogState),
    Test(TestDialogState),
}
```

### 6.2 Statistics Model

```rust
/// Aggregated statistics for dashboard tiles
pub struct DashboardStats {
    pub discover: DiscoverStats,
    pub parser_bench: ParserBenchStats,
    pub jobs: JobStats,
    pub sources: SourceStats,
}

pub struct DiscoverStats {
    pub source_count: u32,
    pub file_count: u32,
    pub tagged_percent: u8,  // 0-100
}

pub struct ParserBenchStats {
    pub parser_count: u32,
    pub active_jobs: u32,
    pub pending_backfill: u32,
}

pub struct JobStats {
    pub running: u32,
    pub failed: u32,
    pub completed_today: u32,
    pub status_indicator: StatusIndicator,
}

pub struct SourceStats {
    pub configured: u32,
    pub equivalence_classes: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum StatusIndicator {
    Healthy,    // ●  Green
    Empty,      // ○  Gray
    InProgress, // ↻  Blue
    Error,      // ✗  Red
    Warning,    // ⚠  Yellow
}
```

### 6.3 Activity Model

```rust
/// A single activity log entry
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub activity_type: ActivityType,
    pub view_link: Option<ViewLink>,
}

#[derive(Debug, Clone)]
pub enum ActivityType {
    Success,  // ●  Job completed, scan finished
    Failure,  // ✗  Job failed, schema violation
    Info,     // ○  Rule created, parser updated
    Warning,  // ⚠  Approaching quota, stale source
}

/// Link to navigate to related view with context
#[derive(Debug, Clone)]
pub struct ViewLink {
    pub view: ViewId,
    pub context: Option<ViewContext>,
}

#[derive(Debug, Clone)]
pub enum ViewContext {
    JobId(Uuid),
    FilePath(PathBuf),
}
```

### 6.4 Dialog State Models

```rust
/// State for the Scan Source dialog
pub struct ScanDialogState {
    pub path: String,
    pub path_cursor: usize,
    pub tag: String,
    pub tag_cursor: usize,
    pub watch: bool,
    pub focused_field: ScanDialogField,
    pub error: Option<String>,
    pub is_scanning: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScanDialogField {
    Path,
    Tag,
    Watch,
}

/// State for the Quick Test dialog
pub struct TestDialogState {
    pub parsers: Vec<ParserInfo>,
    pub selected_parser: usize,
    pub input_path: String,
    pub input_cursor: usize,
    pub focused_field: TestDialogField,
    pub dropdown_open: bool,
    pub error: Option<String>,
    pub is_running: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestDialogField {
    Parser,
    Input,
}

pub struct ParserInfo {
    pub name: String,
    pub version: String,
}
```

### 6.5 Recent Files Model

```rust
/// A recently accessed/processed file
pub struct RecentFile {
    pub path: PathBuf,
    pub accessed_at: DateTime<Utc>,
    pub tag: Option<String>,
    pub source_name: String,
}
```

---

## 7. Data Sources

| Widget | Query | Refresh |
|--------|-------|---------|
| Source count | `SELECT COUNT(*) FROM scout_sources` | 5s |
| File count | `SELECT COUNT(*) FROM scout_files` | 5s |
| Tagged % | See below | 5s |
| Parser count | `SELECT COUNT(DISTINCT name) FROM cf_parsers` | 5s |
| Active jobs | `SELECT COUNT(*) FROM cf_job_status WHERE status = 'running'` | 2s |
| Failed jobs | `SELECT COUNT(*) FROM cf_job_status WHERE status = 'failed' AND DATE(created_at) = DATE('now')` | 5s |
| Completed today | `SELECT COUNT(*) FROM cf_job_status WHERE status = 'complete' AND DATE(created_at) = DATE('now')` | 5s |
| Recent activity | Derived query (see below) | 5s |
| Recent files | `SELECT * FROM scout_files ORDER BY updated_at DESC LIMIT 20` | On request |
| Parser list | `SELECT DISTINCT name, version FROM cf_parsers ORDER BY name` | On dialog open |

**Tagged percentage query:**
```sql
SELECT
    CAST(SUM(CASE WHEN tag IS NOT NULL THEN 1 ELSE 0 END) AS REAL) * 100 / COUNT(*)
FROM scout_files
```

**Recent activity (derived from existing tables - Phase 1):**
```sql
SELECT 'Job completed' as type, parser_name as name, completed_at as ts, 'success' as status
FROM cf_job_status WHERE status = 'complete' AND completed_at > datetime('now', '-1 day')
UNION ALL
SELECT 'Job failed' as type, parser_name as name, completed_at as ts, 'failure' as status
FROM cf_job_status WHERE status = 'failed' AND completed_at > datetime('now', '-1 day')
ORDER BY ts DESC LIMIT 10
```

---

## 8. Implementation Notes

### 8.1 Refresh Strategy

- **Automatic refresh**: Every 5 seconds while Home is visible
- **Manual refresh**: `r` key (global keybinding per tui.md)
- **Event-driven**: Refresh immediately after scan/test completes
- **Debounced**: Multiple rapid refreshes coalesced into one

```rust
impl HomeView {
    const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

    fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= Self::REFRESH_INTERVAL
    }
}
```

### 8.2 First-Time Experience

Detection logic:
```rust
async fn is_first_time(db: &SqlitePool) -> bool {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scout_sources")
        .fetch_one(db)
        .await
        .unwrap_or(0);
    count == 0
}
```

First-time banner replaces the status tiles until user scans first source.

### 8.3 Tile Selection

Tiles are navigable with Tab (forward), Shift+Tab (backward), or arrow keys:

```rust
fn next_tile(&mut self) {
    self.selected_tile = (self.selected_tile + 1) % 4;
}

fn prev_tile(&mut self) {
    self.selected_tile = (self.selected_tile + 3) % 4;
}

fn tile_view_id(&self) -> ViewId {
    match self.selected_tile {
        0 => ViewId::Discover,
        1 => ViewId::ParserBench,
        2 => ViewId::Jobs,
        3 => ViewId::Sources,
        _ => unreachable!(),
    }
}
```

### 8.4 Activity Log Auto-Scroll

- New entries appear at top
- Auto-scroll to top when new entry added (unless user has scrolled)
- Sticky scroll position if user has manually scrolled

```rust
fn add_activity(&mut self, entry: ActivityEntry) {
    let was_at_top = self.activity_scroll == 0;
    self.activity_log.insert(0, entry);
    if self.activity_log.len() > 50 {
        self.activity_log.pop();
    }
    if was_at_top {
        self.activity_scroll = 0; // Stay at top
    }
}
```

### 8.5 Dialog Focus Trapping

When a dialog is open:
- Tab only cycles within dialog fields
- Esc closes dialog
- Background is dimmed and non-interactive
- Number keys (1-4) are disabled

### 8.6 View Trait Implementation

```rust
impl View for HomeView {
    fn name(&self) -> &'static str {
        "Home"
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.state.state {
            HomeState::Loading => self.render_loading(frame, area),
            HomeState::Dashboard => self.render_dashboard(frame, area),
            HomeState::ScanDialog => {
                self.render_dashboard(frame, area);
                self.render_scan_dialog(frame, area);
            }
            HomeState::TestDialog => {
                self.render_dashboard(frame, area);
                self.render_test_dialog(frame, area);
            }
            HomeState::RecentFiles => self.render_recent_files(frame, area),
        }
    }

    fn handle_event(&mut self, event: Event) -> ViewAction {
        match &self.state.state {
            HomeState::Dashboard => self.handle_dashboard_event(event),
            HomeState::ScanDialog => self.handle_scan_dialog_event(event),
            HomeState::TestDialog => self.handle_test_dialog_event(event),
            HomeState::RecentFiles => self.handle_recent_files_event(event),
            HomeState::Loading => ViewAction::None,
        }
    }

    fn help_text(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1-4", "Jump to view"),
            ("s", "Scan source"),
            ("t", "Quick test"),
            ("R", "Recent files"),
            ("Enter", "Open selected"),
            ("?", "Help"),
            ("q", "Quit"),
        ]
    }

    fn on_enter(&mut self) {
        self.state.state = HomeState::Loading;
        self.refresh_stats();
    }

    fn on_leave(&mut self) {
        // State preserved for return
    }
}
```

### 8.7 Toast Notifications

After scan/test completes, show toast per tui.md Section 9.2:
- Auto-dismiss success after 5 seconds
- Errors stay until dismissed with Esc

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Expanded from stub: full state machine, data models, workflows, implementation notes |
| 2026-01-12 | 0.1 | Initial stub |

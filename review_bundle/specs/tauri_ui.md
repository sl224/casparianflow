# Tauri UI Design Specification

**Status:** Draft
**Parent:** [ADR-020](../docs/decisions/ADR-020-tauri-gui.md), [specs/tauri_mvp.md](./tauri_mvp.md)
**Version:** 0.1
**Date:** 2026-01-20

---

## 1. Design Philosophy

### 1.1 From TUI to GUI

The existing TUI has excellent information architecture. The Tauri UI preserves this structure while adapting for mouse-first interaction:

| TUI Concept | Tauri Adaptation |
|-------------|------------------|
| Keybindings (1-4) | Sidebar navigation + keyboard shortcuts |
| Rail (left nav) | Persistent sidebar with icons + labels |
| Inspector (right panel) | Collapsible detail panel |
| Action bar (bottom) | Toolbar (top) + context menu |
| Modal overlays | React modal components |
| Text input mode | Standard HTML inputs |
| Vim-style navigation | Click + scroll + keyboard shortcuts |

### 1.2 Target Persona Adaptation

Trade Support Analysts use Excel and Bloomberg Terminal. The UI should feel familiar:

| Bloomberg/Excel Pattern | Tauri Implementation |
|-------------------------|----------------------|
| Tabbed workspaces | Tab bar for multiple sessions |
| Data grids | TanStack Table with sorting/filtering |
| Right-click context menus | Native context menus |
| Keyboard shortcuts | Cmd/Ctrl + key combinations |
| Status bar | Bottom status bar with job progress |
| Side panels | Collapsible inspector panel |

### 1.3 Core Principles

1. **Output-first**: Show queryable data immediately (Home = Readiness Board)
2. **Progressive disclosure**: Simple by default, powerful on demand
3. **Keyboard accessible**: Power users can navigate without mouse
4. **Local-first visual**: No cloud icons, emphasize "data stays here"
5. **Dark mode default**: Match Bloomberg Terminal aesthetic

---

## 2. Global Shell

All views share a consistent shell layout:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ☰  Casparian Flow              [Search...        🔍]    [⚙️] [?] [—][□][×]│
├────────┬────────────────────────────────────────────────────────────────┤
│        │  [Tab 1: FIX Logs] [Tab 2: HL7 Archive] [+]                    │
│  🏠    ├────────────────────────────────────────────────────────────────┤
│  Home  │                                                                │
│        │                                                                │
│  📁    │                        MAIN CONTENT                            │
│Discover│                                                                │
│        │                         (View-specific)                        │
│  🔧    │                                                                │
│Parsers │                                                                │
│        │                                                                │
│  📊    │                                                                │
│  Jobs  │                                                                │
│        │                                                                │
│  ⚙️    │                                                                │
│Settings│                                                                │
├────────┴────────────────────────────────────────────────────────────────┤
│ ✓ Ready: 3 outputs  │  ↻ Running: 2 jobs  │  ⚠ Warnings: 1  │  Local   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Title Bar (Custom, Frameless)

| Element | Description |
|---------|-------------|
| ☰ Menu | App menu (File, Edit, View, Help) |
| Logo + Name | "Casparian Flow" |
| Global Search | Cmd+K to search files, parsers, jobs |
| Settings gear | Opens settings panel |
| Help button | Opens help/docs |
| Window controls | Minimize, maximize, close |

### 2.2 Sidebar (Left Rail)

| Icon | Label | Shortcut | View |
|------|-------|----------|------|
| 🏠 | Home | Cmd+1 | Readiness Board |
| 📁 | Discover | Cmd+2 | File Browser + Rule Builder |
| 🔧 | Parsers | Cmd+3 | Parser Bench |
| 📊 | Jobs | Cmd+4 | Job Queue Monitor |
| ⚙️ | Settings | Cmd+, | App Configuration |

**Behavior:**
- Click to navigate
- Hover shows tooltip with name + shortcut
- Active view is highlighted
- Sidebar can collapse to icons-only (Cmd+B)

### 2.3 Tab Bar

| Feature | Description |
|---------|-------------|
| Session tabs | Each "workspace" (set of files) is a tab |
| New tab (+) | Opens new empty workspace |
| Close tab (×) | Closes workspace (prompts if unsaved) |
| Tab context menu | Close, Close Others, Duplicate |
| Drag to reorder | Reorder tabs |

### 2.4 Status Bar (Bottom)

| Section | Content |
|---------|---------|
| Ready | "✓ Ready: 3 outputs" (click to view) |
| Running | "↻ Running: 2 jobs" (click to view) |
| Warnings | "⚠ Warnings: 1" (click to view) |
| Mode | "Local" or "Connected: postgres://..." |

---

## 3. Home View (Readiness Board)

The landing page shows output-first status:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│  READY OUTPUTS                                          [View All →]    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 📊 fix_order_lifecycle    1.2M rows    Last updated: 5 min ago  │   │
│  │ 📊 fix_executions         420K rows    Last updated: 5 min ago  │   │
│  │ 📊 hl7_observations       89K rows     Last updated: 2 hrs ago  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ACTIVE RUNS                                            [View All →]    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ ↻ finance_ap_parse    ████████░░ 62%    ETA: 5 min              │   │
│  │ ↻ hl7_daily_scan      █░░░░░░░░░  4%    ETA: 1 hr               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  QUICK ACTIONS                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                  │
│  │   📂 Open    │  │   🔍 Scan    │  │   📋 Query   │                  │
│  │   Files      │  │   Folder     │  │   Output     │                  │
│  └──────────────┘  └──────────────┘  └──────────────┘                  │
│                                                                         │
│  WARNINGS                                               [View All →]    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ ⚠ hl7_observations: 14 quarantined rows (schema violation)      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Ready Outputs Section

- Card list of completed parser outputs
- Click to open in Query view
- Shows: name, row count, last updated
- "View All" links to Discover with filter

### 3.2 Active Runs Section

- Progress bars for running jobs
- Click to view job details
- Shows: name, progress, ETA
- Cancel button on hover

### 3.3 Quick Actions

| Action | Description |
|--------|-------------|
| Open Files | Opens file picker → Discover view |
| Scan Folder | Opens folder picker → scans directory |
| Query Output | Opens SQL query panel |

### 3.4 Warnings Section

- List of quarantined rows, failed jobs
- Click to view details
- Dismissible after acknowledgment

---

## 4. Discover View (File Browser + Rule Builder)

The primary workflow for Trade Support: import files → parse → query.

### 4.1 Default State (No Files)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Source: [None selected ▼]   Tags: [All ▼]   Rules: [All ▼]            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│                                                                         │
│              ┌─────────────────────────────────────────┐                │
│              │                                         │                │
│              │         📂 Drop FIX log files here      │                │
│              │                                         │                │
│              │         or click to browse              │                │
│              │                                         │                │
│              │    Supports: .log, .txt, .fix          │                │
│              │                                         │                │
│              └─────────────────────────────────────────┘                │
│                                                                         │
│              ────────────── OR ──────────────                           │
│                                                                         │
│              [Scan Existing Folder]   [Open Recent ▼]                   │
│                                                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4.2 With Files Loaded (Trade Break Workbench)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Source: [/var/log/fix ▼]   Tags: [All ▼]   Search: [____________ 🔍]  │
├───────────────────────────────────────────────┬─────────────────────────┤
│  ORDER LIFECYCLE                    [SQL] [⚙️] │  ORDER DETAIL           │
├───────────────────────────────────────────────┤                         │
│ ClOrdID    │ Symbol │ Side │ Status  │ Time   │  ClOrdID: DEF456        │
│────────────┼────────┼──────┼─────────┼────────│  Symbol: MSFT           │
│ ABC123     │ AAPL   │ Buy  │ Filled  │ 09:31  │  Side: Sell             │
│▶DEF456     │ MSFT   │ Sell │Rejected │ 09:32  │  Status: REJECTED       │
│ GHI789     │ GOOGL  │ Buy  │ Partial │ 09:33  │                         │
│ JKL012     │ AMZN   │ Sell │ Filled  │ 09:34  │  Reject Reason:         │
│ MNO345     │ META   │ Buy  │ Filled  │ 09:35  │  "Insufficient buying   │
│ PQR678     │ NVDA   │ Sell │ Filled  │ 09:36  │   power"                │
│ STU901     │ TSLA   │ Buy  │Cancelled│ 09:37  │                         │
│ ...        │ ...    │ ...  │ ...     │ ...    │  ─── TIMELINE ───       │
├───────────────────────────────────────────────│                         │
│ Showing 1,234 of 12,410 orders     [< 1 2 3 >]│  09:32:12.001           │
│                                               │  → NewOrderSingle (35=D)│
│ ─── FILTERS ───                              │    Qty: 500  Px: 425.50 │
│ Status: [All ▼] [Rejected ☑] [Filled ☐]     │                         │
│ Symbol: [___________]                        │  09:32:12.015           │
│ Date:   [Today ▼]                            │  ← ExecutionReport (35=8)│
│                                               │    Status: Rejected     │
│                                               │    Reason: Insufficient │
│                                               │            buying power │
│                                               │                         │
│                                               │  [Copy] [Export] [SQL]  │
└───────────────────────────────────────────────┴─────────────────────────┘
```

### 4.3 Components

#### Source Selector (Dropdown)
- Lists configured sources
- "Add Source..." option at bottom
- Shows file count per source

#### Search Bar
- Global search across all columns
- Debounced (300ms)
- Highlights matches in table

#### Order Lifecycle Table
- TanStack Table with virtual scrolling
- Click row to select → shows in detail panel
- Double-click to expand inline
- Right-click for context menu (Copy, Export, View Raw)
- Column sorting (click header)
- Column resize (drag border)

#### Filters Panel (Collapsible)
- Status: Multi-select checkboxes
- Symbol: Text input with autocomplete
- Date: Date range picker
- "Clear All" button

#### Order Detail Panel (Right)
- Shows selected order details
- Timeline of all FIX messages
- Expandable raw message view
- Action buttons: Copy, Export, SQL

### 4.4 Keyboard Shortcuts (Discover)

| Shortcut | Action |
|----------|--------|
| Cmd+O | Open file picker |
| Cmd+F | Focus search |
| Cmd+Shift+F | Open advanced filters |
| ↑/↓ | Navigate table rows |
| Enter | Select row → show detail |
| Cmd+C | Copy selected row |
| Cmd+Shift+C | Copy as SQL |
| Esc | Clear selection / close panel |

---

## 5. Parsers View (Parser Bench)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Search: [____________ 🔍]                    [+ Add Parser] [Refresh]  │
├────────────────────────────────────────────────┬────────────────────────┤
│  PARSERS                                       │  PARSER DETAIL         │
├────────────────────────────────────────────────┤                        │
│ Status │ Name           │ Version │ Topics    │  Name: fix_parser      │
│────────┼────────────────┼─────────┼───────────│  Version: 1.2.0        │
│ ● OK   │ fix_parser     │ 1.2.0   │ fix_logs  │  Path: ~/.casparian/...│
│ ● OK   │ hl7_parser     │ 2.0.1   │ hl7_msgs  │                        │
│ ⚠ Warn │ csv_generic    │ 0.9.0   │ csv_files │  Topics:               │
│ ○ New  │ iso20022       │ 1.0.0   │ payments  │  - fix_logs            │
│ ✗ Fail │ broken_parser  │ 0.1.0   │ test      │                        │
│                                                │  Health: ● Healthy     │
│                                                │  Last Run: 5 min ago   │
│                                                │  Success Rate: 99.2%   │
│                                                │                        │
│                                                │  Output Tables:        │
│                                                │  - fix_messages        │
│                                                │  - fix_orders          │
│                                                │  - fix_executions      │
│                                                │  - fix_order_lifecycle │
│                                                │                        │
│                                                │  [Test] [Edit] [Delete]│
└────────────────────────────────────────────────┴────────────────────────┘
```

### 5.1 Parser List

- Status icon: ● OK, ⚠ Warning, ✗ Failed, ○ Unknown
- Sortable columns
- Filter by status, topic
- Right-click context menu

### 5.2 Parser Detail Panel

- Metadata: name, version, path
- Topics subscribed
- Health metrics
- Output tables
- Action buttons: Test, Edit, Delete

### 5.3 Parser Actions

| Action | Description |
|--------|-------------|
| Test | Run parser on sample file |
| Edit | Open parser file in editor |
| Delete | Remove parser (with confirmation) |
| Add Parser | Open file picker for .py file |

---

## 6. Jobs View (Queue Monitor)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Filter: [All ▼]   Search: [____________ 🔍]              [Refresh]    │
├────────────────────────────────────────────────┬────────────────────────┤
│  ACTIONABLE                                    │  JOB DETAIL            │
├────────────────────────────────────────────────┤                        │
│ Status │ Job            │ Progress │ Started  │  Job: scan_logs_001    │
│────────┼────────────────┼──────────┼──────────│  Type: Scan            │
│ ↻ Run  │ scan_logs_001  │ ████░░ 62%│ 2m ago  │  Status: Running       │
│ ↻ Run  │ parse_hl7_002  │ █░░░░░  4%│ 5m ago  │  Started: 2 min ago    │
│ ○ Pend │ backtest_003   │ ░░░░░░  0%│ —       │  Progress: 62%         │
│ ✗ Fail │ export_csv_004 │ ████░░ 45%│ 10m ago │  ETA: 3 min            │
├────────────────────────────────────────────────│                        │
│  COMPLETED                                     │  Items:                │
├────────────────────────────────────────────────│  - Total: 12,410      │
│ ✓ Done │ scan_inbox_005 │ ██████100%│ 1h ago  │  - Processed: 7,694    │
│ ✓ Done │ parse_fix_006  │ ██████100%│ 2h ago  │  - Failed: 0           │
│ ⚠ Part │ extract_007    │ █████░ 95%│ 3h ago  │                        │
│                                                │  Output:               │
│                                                │  - Path: /data/...     │
│                                                │  - Size: 45 MB         │
│                                                │                        │
│                                                │  [Cancel] [Retry] [Log]│
└────────────────────────────────────────────────┴────────────────────────┘
```

### 6.1 Job List (Split View)

**Actionable (Top)**
- Running, Pending, Failed jobs
- Sorted: Running → Pending → Failed

**Completed (Bottom)**
- Done, Partial Success jobs
- Sorted by completion time (newest first)

### 6.2 Job Detail Panel

- Job metadata
- Progress breakdown
- Output location
- Action buttons: Cancel, Retry, View Log

### 6.3 Job Actions

| Action | Availability |
|--------|--------------|
| Cancel | Running, Pending |
| Retry | Failed |
| View Log | All |
| Open Output | Completed |

---

## 7. Settings View

```
┌─────────────────────────────────────────────────────────────────────────┐
│  SETTINGS                                                               │
├──────────────────────┬──────────────────────────────────────────────────┤
│                      │                                                  │
│  General             │  GENERAL                                         │
│  ────────            │                                                  │
│  ▸ General           │  Default Source Path                             │
│    Appearance        │  [/var/log/fix                              📂]  │
│    Parsers           │                                                  │
│    Database          │  Auto-scan on Startup                            │
│    Keyboard          │  [✓] Scan default source when app opens          │
│    About             │                                                  │
│                      │  Confirm Destructive Actions                     │
│                      │  [✓] Ask before deleting sources, parsers, etc.  │
│                      │                                                  │
│                      │  ─────────────────────────────────────────────── │
│                      │                                                  │
│                      │  APPEARANCE                                      │
│                      │                                                  │
│                      │  Theme                                           │
│                      │  ( ) Light  (•) Dark  ( ) System                 │
│                      │                                                  │
│                      │  Sidebar                                         │
│                      │  [✓] Show labels  [✓] Show icons                 │
│                      │                                                  │
└──────────────────────┴──────────────────────────────────────────────────┘
```

### 7.1 Settings Categories

| Category | Settings |
|----------|----------|
| General | Default path, auto-scan, confirmations |
| Appearance | Theme, sidebar, font size |
| Parsers | Parser directory, auto-reload |
| Database | DB path (read-only), backup |
| Keyboard | Shortcut customization |
| About | Version, license, links |

---

## 8. Common Components

### 8.1 Data Table (TanStack Table)

```tsx
interface TableProps<T> {
  data: T[];
  columns: ColumnDef<T>[];
  onRowSelect?: (row: T) => void;
  onRowDoubleClick?: (row: T) => void;
  enableSorting?: boolean;
  enableFiltering?: boolean;
  enablePagination?: boolean;
  virtualScroll?: boolean;
}
```

**Features:**
- Virtual scrolling for 100K+ rows
- Column sorting (multi-column with Shift+click)
- Column resizing
- Row selection (single or multi)
- Keyboard navigation (↑/↓, Enter, Escape)
- Context menu (right-click)
- Copy to clipboard
- Export to CSV

### 8.2 Detail Panel

```tsx
interface DetailPanelProps {
  title: string;
  isOpen: boolean;
  onClose: () => void;
  width?: number | string;
  children: React.ReactNode;
}
```

**Features:**
- Collapsible (Cmd+I or click toggle)
- Resizable (drag border)
- Sections with expand/collapse
- Action buttons at bottom

### 8.3 Dropdown/Select

```tsx
interface SelectProps<T> {
  value: T;
  options: { label: string; value: T }[];
  onChange: (value: T) => void;
  placeholder?: string;
  searchable?: boolean;
}
```

**Features:**
- Type to filter (searchable)
- Keyboard navigation
- Multi-select variant
- Custom option rendering

### 8.4 Progress Bar

```tsx
interface ProgressProps {
  value: number; // 0-100
  label?: string;
  showPercentage?: boolean;
  variant?: 'default' | 'success' | 'warning' | 'error';
}
```

### 8.5 Status Badge

```tsx
type StatusVariant = 'success' | 'warning' | 'error' | 'info' | 'pending';

interface BadgeProps {
  variant: StatusVariant;
  label: string;
  icon?: React.ReactNode;
}
```

### 8.6 Toast Notifications

```tsx
interface ToastProps {
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message?: string;
  duration?: number;
  action?: { label: string; onClick: () => void };
}
```

---

## 9. Keyboard Shortcuts (Global)

| Shortcut | Action |
|----------|--------|
| Cmd+1 | Go to Home |
| Cmd+2 | Go to Discover |
| Cmd+3 | Go to Parsers |
| Cmd+4 | Go to Jobs |
| Cmd+, | Open Settings |
| Cmd+K | Global search |
| Cmd+B | Toggle sidebar |
| Cmd+I | Toggle inspector/detail panel |
| Cmd+W | Close current tab |
| Cmd+T | New tab |
| Cmd+Shift+T | Reopen closed tab |
| Cmd+Q | Quit app |
| F1 or ? | Help |

---

## 10. Color Palette

### 10.1 Dark Theme (Default)

| Token | Value | Usage |
|-------|-------|-------|
| `--bg-primary` | #1a1a1a | Main background |
| `--bg-secondary` | #252525 | Cards, panels |
| `--bg-tertiary` | #303030 | Inputs, table rows |
| `--text-primary` | #ffffff | Primary text |
| `--text-secondary` | #a0a0a0 | Secondary text |
| `--text-muted` | #666666 | Disabled text |
| `--accent-primary` | #3b82f6 | Primary actions |
| `--accent-success` | #22c55e | Success states |
| `--accent-warning` | #f59e0b | Warning states |
| `--accent-error` | #ef4444 | Error states |
| `--border-default` | #404040 | Borders |

### 10.2 Light Theme

| Token | Value | Usage |
|-------|-------|-------|
| `--bg-primary` | #ffffff | Main background |
| `--bg-secondary` | #f5f5f5 | Cards, panels |
| `--bg-tertiary` | #e5e5e5 | Inputs, table rows |
| `--text-primary` | #1a1a1a | Primary text |
| `--text-secondary` | #666666 | Secondary text |
| `--text-muted` | #a0a0a0 | Disabled text |
| `--accent-primary` | #2563eb | Primary actions |
| `--accent-success` | #16a34a | Success states |
| `--accent-warning` | #d97706 | Warning states |
| `--accent-error` | #dc2626 | Error states |
| `--border-default` | #d4d4d4 | Borders |

---

## 11. Implementation Phases

### Phase 1: Shell + Home (Week 1)

- [ ] Tauri 2.0 project setup
- [ ] React + TypeScript + Tailwind
- [ ] Global shell (sidebar, title bar, status bar)
- [ ] Home view (static mockup)
- [ ] Navigation between views
- [ ] Dark theme implementation

### Phase 2: Discover Core (Week 2)

- [ ] Drag & drop file import
- [ ] Wire Tauri command to Rust parser
- [ ] TanStack Table for order lifecycle
- [ ] Basic search and filtering
- [ ] Row selection → detail panel

### Phase 3: Discover Polish (Week 3)

- [ ] Filter panel (status, symbol, date)
- [ ] Order detail timeline view
- [ ] Copy to clipboard
- [ ] Export to CSV
- [ ] Keyboard shortcuts

### Phase 4: Jobs + Packaging (Week 4)

- [ ] Jobs view (list + detail)
- [ ] Job progress tracking
- [ ] macOS installer (.dmg)
- [ ] Windows installer (.msi)
- [ ] Final polish and testing

### Phase 5: Post-MVP (Future)

- [ ] Parsers view
- [ ] Settings view
- [ ] SQL query panel
- [ ] Rule Builder UI
- [ ] Multi-tab workspaces
- [ ] Auto-update mechanism

---

## 12. File Structure

```
src/
├── main.tsx                    # React entry point
├── App.tsx                     # Root component with routing
├── tauri.ts                    # Tauri API wrapper
│
├── components/
│   ├── shell/
│   │   ├── Sidebar.tsx
│   │   ├── TitleBar.tsx
│   │   ├── StatusBar.tsx
│   │   └── TabBar.tsx
│   ├── common/
│   │   ├── DataTable.tsx
│   │   ├── DetailPanel.tsx
│   │   ├── Select.tsx
│   │   ├── Badge.tsx
│   │   ├── Progress.tsx
│   │   └── Toast.tsx
│   └── views/
│       ├── Home.tsx
│       ├── Discover.tsx
│       ├── Parsers.tsx
│       ├── Jobs.tsx
│       └── Settings.tsx
│
├── hooks/
│   ├── useParser.ts            # Parser invocation
│   ├── useJobs.ts              # Job polling
│   ├── useSources.ts           # Source management
│   └── useKeyboard.ts          # Keyboard shortcuts
│
├── stores/
│   └── appStore.ts             # Zustand store
│
├── styles/
│   ├── globals.css
│   └── theme.css
│
└── types/
    ├── parser.ts
    ├── job.ts
    └── source.ts
```

---

## 13. Tauri Commands (Rust ↔ React)

```rust
// src-tauri/src/commands.rs

#[tauri::command]
async fn parse_fix_file(path: String) -> Result<ParseResult, String>;

#[tauri::command]
async fn query_order_lifecycle(filter: OrderFilter) -> Result<Vec<OrderLifecycle>, String>;

#[tauri::command]
async fn list_sources() -> Result<Vec<Source>, String>;

#[tauri::command]
async fn scan_directory(path: String) -> Result<ScanResult, String>;

#[tauri::command]
async fn list_jobs() -> Result<Vec<Job>, String>;

#[tauri::command]
async fn cancel_job(job_id: String) -> Result<(), String>;

#[tauri::command]
async fn list_parsers() -> Result<Vec<Parser>, String>;
```

---

## 14. Open Questions

1. **Tab state persistence:** Save open tabs between sessions?
2. **Auto-update:** Tauri updater or manual download?
3. **License activation:** How to handle Pro/Enterprise?
4. **Telemetry:** Anonymous usage analytics? (opt-in)
5. **Plugin system:** Allow custom parsers via UI?

---

## 15. References

- [TUI App.rs](../crates/casparian/src/cli/tui/app.rs) - State machine reference
- [TUI Spec - Rule Builder](./rule_builder.md) - Discover mode spec
- [TUI Spec - Home](./views/home.md) - Home view spec
- [TUI Spec - Jobs](./views/jobs.md) - Jobs view spec
- [Tauri 2.0 Docs](https://v2.tauri.app/)
- [TanStack Table](https://tanstack.com/table/latest)
- [Zustand](https://zustand-demo.pmnd.rs/)

---

## 16. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial design based on TUI analysis |

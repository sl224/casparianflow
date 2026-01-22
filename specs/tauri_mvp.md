# Tauri MVP: Trade Break Workbench

> **⚠️ ARCHIVED:** This spec is no longer active. The Tauri GUI approach was explored
> but the project pivoted to CLI/TUI-first architecture. See CLAUDE.md ADR-007.
> Retained for historical reference only.

**Status:** Archived (Cancelled)
**Parent:** [ADR-020](../docs/decisions/ADR-020-tauri-gui.md) (exploration only)
**Original Target:** Mid-February 2026
**Version:** 0.1

---

## 1. Goal

Build the minimum viable Tauri GUI that enables a compelling 60-second demo:

> "Watch me solve a trade break in 60 seconds."

The demo should resonate with Trade Support Analysts who currently spend 30-45 minutes per trade break using grep + Excel.

---

## 2. Target User Workflow

**Before Casparian (current pain):**
1. Receive alert: "Trade break on order ABC123"
2. SSH into log server
3. `grep ABC123 *.log` across multiple files
4. Copy-paste results into Excel
5. Manually reconstruct order lifecycle
6. **30-45 minutes later:** Find the issue

**With Casparian (target state):**
1. Drag & drop FIX log files into app
2. App auto-parses and shows `fix_order_lifecycle` table
3. Search for ClOrdID "ABC123"
4. See complete order lifecycle in one view
5. **5 minutes:** Find the issue

---

## 3. MVP Features (Must Have)

### 3.1 File Import

| Feature | Description | Priority |
|---------|-------------|----------|
| Drag & drop | Drop FIX log files onto window | P0 |
| File browser | "Open File" dialog fallback | P0 |
| Multi-file | Handle multiple log files at once | P0 |
| Progress indicator | Show parsing progress for large files | P0 |

**Accepted formats:** `.log`, `.txt`, `.fix` (pipe-delimited or SOH-delimited)

### 3.2 Auto-Parse

| Feature | Description | Priority |
|---------|-------------|----------|
| Format detection | Detect FIX 4.2/4.4/5.0 automatically | P0 |
| Delimiter detection | Handle `|` (pipe) and `\x01` (SOH) | P0 |
| Error handling | Show parse errors gracefully | P0 |
| Custom tags | Support venue-specific tags (5000+) | P1 |

### 3.3 Results View

| Feature | Description | Priority |
|---------|-------------|----------|
| Table view | Display `fix_order_lifecycle` as table | P0 |
| Column sorting | Click column header to sort | P0 |
| Column resize | Drag to resize columns | P1 |
| Row selection | Click row to see details | P0 |
| Pagination | Handle large result sets (1000+ rows) | P0 |

**Core columns to display:**

| Column | Description |
|--------|-------------|
| `cl_ord_id` | Client Order ID (primary identifier) |
| `symbol` | Instrument symbol |
| `side` | Buy/Sell |
| `order_qty` | Original quantity |
| `cum_qty` | Filled quantity |
| `avg_px` | Average fill price |
| `order_status` | Final status (Filled, Rejected, etc.) |
| `first_seen` | First message timestamp |
| `last_update` | Last message timestamp |
| `message_count` | Number of messages in lifecycle |

### 3.4 Search & Filter

| Feature | Description | Priority |
|---------|-------------|----------|
| Global search | Search across all columns | P0 |
| ClOrdID filter | Filter by specific order ID | P0 |
| Symbol filter | Filter by instrument | P1 |
| Status filter | Filter by order status | P1 |
| Date range | Filter by timestamp range | P1 |

### 3.5 Detail Panel

| Feature | Description | Priority |
|---------|-------------|----------|
| Order detail | Show full order lifecycle on row click | P0 |
| Message timeline | Show all FIX messages for that order | P0 |
| Raw message view | Show raw FIX message with field labels | P1 |
| Copy to clipboard | Copy order details for sharing | P0 |

---

## 4. MVP Features (Nice to Have - Post-Launch)

| Feature | Description | Priority |
|---------|-------------|----------|
| SQL query panel | Run custom SQL queries | P2 |
| Export to CSV | Export filtered results | P2 |
| Export to Parquet | Export for further analysis | P2 |
| Dark mode | Match Bloomberg Terminal aesthetic | P2 |
| Saved filters | Save frequently used filters | P3 |
| Multiple workspaces | Handle multiple log sets | P3 |

---

## 5. Tech Stack

### Frontend

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Framework | React 18 | Widely known, fast iteration |
| Language | TypeScript | Type safety, better DX |
| Styling | Tailwind CSS | Rapid UI development |
| Table | TanStack Table (React Table v8) | Best-in-class table library |
| State | Zustand or React Query | Simple, performant |
| Icons | Lucide React | Clean, consistent |

### Backend (Tauri)

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Runtime | Tauri 2.0 | Rust backend, small binaries |
| Parser | Existing `casparian` Rust code | Reuse core parsing logic |
| IPC | Tauri invoke commands | JSON over IPC |
| Storage | DuckDB (embedded) | Fast queries, no server |

### Build & Distribution

| Platform | Format | Size Target |
|----------|--------|-------------|
| macOS | `.dmg` | <50MB |
| Windows | `.msi` | <50MB |
| Linux | `.AppImage` | <50MB |

---

## 6. UI Wireframe (ASCII)

```
┌─────────────────────────────────────────────────────────────────────┐
│  Casparian Flow - Trade Break Workbench                    [—][□][×]│
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                                                               │ │
│  │     ┌─────────────────────────────────────────┐               │ │
│  │     │                                         │               │ │
│  │     │   Drop FIX log files here               │               │ │
│  │     │                                         │               │ │
│  │     │   or click to browse                    │               │ │
│  │     │                                         │               │ │
│  │     └─────────────────────────────────────────┘               │ │
│  │                                                               │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘

                              ↓ After file drop ↓

┌─────────────────────────────────────────────────────────────────────┐
│  Casparian Flow - Trade Break Workbench                    [—][□][×]│
├─────────────────────────────────────────────────────────────────────┤
│  [Search: _______________]  [Status: All ▼]  [Symbol: All ▼]       │
├─────────────────────────────────────────────────────────────────────┤
│  ClOrdID    │ Symbol │ Side │ Qty    │ Filled │ Status   │ Time    │
│─────────────┼────────┼──────┼────────┼────────┼──────────┼─────────│
│  ABC123     │ AAPL   │ Buy  │ 1000   │ 1000   │ Filled   │ 09:31:05│
│▶ DEF456     │ MSFT   │ Sell │ 500    │ 0      │ Rejected │ 09:32:12│
│  GHI789     │ GOOGL  │ Buy  │ 2000   │ 1500   │ Partial  │ 09:33:45│
│  ...        │ ...    │ ...  │ ...    │ ...    │ ...      │ ...     │
├─────────────────────────────────────────────────────────────────────┤
│                      Order Detail: DEF456                           │
├─────────────────────────────────────────────────────────────────────┤
│  Status: REJECTED                                                   │
│  Reject Reason: "Insufficient buying power"                         │
│                                                                     │
│  Message Timeline:                                                  │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 09:32:12.001  NewOrderSingle (35=D)    Qty: 500  Px: 425.50 │   │
│  │ 09:32:12.015  ExecutionReport (35=8)   Status: Rejected     │   │
│  │               OrdRejReason: Insufficient buying power        │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  [Copy to Clipboard]                                                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 7. Implementation Phases

### Phase 1: Skeleton (Week 1)

- [ ] Initialize Tauri 2.0 project with React + TypeScript
- [ ] Set up Tailwind CSS
- [ ] Create basic window with drag & drop zone
- [ ] Wire up Tauri command to call Rust parser
- [ ] Display raw parse results in console

### Phase 2: Core Table (Week 2)

- [ ] Integrate TanStack Table
- [ ] Display `fix_order_lifecycle` data
- [ ] Add column sorting
- [ ] Add global search
- [ ] Add row selection

### Phase 3: Detail Panel (Week 3)

- [ ] Create split-pane layout (table + detail)
- [ ] Show order details on row click
- [ ] Show message timeline
- [ ] Add "Copy to Clipboard" button

### Phase 4: Polish (Week 4)

- [ ] Add status/symbol filters
- [ ] Handle parse errors gracefully
- [ ] Add loading states
- [ ] Test with real FIX log files
- [ ] Build installers for macOS/Windows

---

## 8. Demo Script (60 seconds)

```
[0:00] "Here's a trade break on order DEF456. Let me show you how fast
       we can debug this."

[0:05] *Drag FIX log file onto Casparian*

[0:10] "Casparian automatically parses the FIX messages and reconstructs
       order lifecycles."

[0:15] *Type "DEF456" in search box*

[0:20] "Found it. Let's see what happened."

[0:25] *Click on the row*

[0:30] "Here's the complete lifecycle. NewOrderSingle at 09:32:12,
       rejected 14 milliseconds later."

[0:40] "Reason: Insufficient buying power. The counterparty rejected it."

[0:50] *Click "Copy to Clipboard"*

[0:55] "Copy the details, paste into your ticket. Done in under a minute."

[1:00] "That used to take 45 minutes with grep and Excel."
```

---

## 9. Success Criteria

| Metric | Target |
|--------|--------|
| Time to first result | <5 seconds for 10MB log file |
| Demo completion | 60 seconds or less |
| Parse accuracy | 99%+ on valid FIX messages |
| Installer size | <50MB |
| Cold start time | <3 seconds |

---

## 10. Open Questions

1. **Custom tag dictionaries:** How do users specify venue-specific tags?
2. **Multi-file correlation:** How to handle logs split across files?
3. **Timestamp parsing:** What formats do different venues use?
4. **Licensing:** How is the product licensed? (seat-based, machine-based?)

---

## 11. References

- [ADR-020: Tauri GUI Decision](../docs/decisions/ADR-020-tauri-gui.md)
- [strategies/finance.md](../strategies/finance.md) - Trade Support persona
- [Tauri 2.0 Documentation](https://v2.tauri.app/)
- [TanStack Table](https://tanstack.com/table/latest)
- [Existing FIX parser](../crates/casparian_worker/) - Rust implementation

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 0.1 | Initial MVP spec |

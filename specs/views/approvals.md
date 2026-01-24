# Approvals View - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0
**Related:** specs/views/sessions.md, docs/execution_plan_mcp.md
**Last Updated:** 2026-01-21

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Approvals** view provides MCP approval management for pending, approved, and rejected requests. This is the human-in-the-loop interface for the AI-assisted workflow, allowing users to review and act on approval requests from the intent pipeline.

### 1.1 Data Source

```
~/.casparian_flow/casparian_flow.duckdb

Tables queried:
├── mcp_approvals        # Pending/approved/rejected requests
├── mcp_sessions         # Session context for approvals
└── cf_parsers           # Parser metadata for context
```

### 1.2 User Goals

| Goal | How Approvals Helps |
|------|---------------------|
| "See what needs my approval" | Pending requests shown first |
| "Review approval details" | Inspector shows full context |
| "Approve/reject quickly" | `a`/`r` keys for direct action |
| "Understand approval history" | Filter by status, see timestamps |

---

## 2. Layout

```
+- Casparian Flow | View: Approvals | Pending: 3 | Approved: 12 | Rejected: 2 --+
+- Rail -----------+- Approvals List -------------------+- Inspector ------------+
| [0] Home         | PENDING (3)                        | Approval #AP-2847      |
| [1] Discover     | > [PEND] Schema: orders_v2  2m ago | Type: Schema           |
| [2] Parser Bench |   [PEND] Run: backfill      5m ago | Session: S-1842        |
| [3] Jobs         |   [PEND] Plugin: email_ingest      | Requester: Claude      |
| [4] Sources      |                                    | Created: 2m ago        |
| [5] Approvals    | APPROVED (12)                      |                        |
| [6] Query        |   [APPR] Schema: trades_v1  1h ago | Request:               |
| [7] Sessions     |   [APPR] Run: daily_parse   2h ago | "Approve schema for    |
|                  |                                    | orders table with 5    |
| Filter: All      | REJECTED (2)                       | columns: id, customer, |
|                  |   [REJ]  Plugin: untrusted  1d ago | product, qty, price"   |
|                  |                                    |                        |
|                  |                                    | [a] to approve         |
|                  |                                    | [r] to reject          |
+------------------+------------------------------------+------------------------+
| [a] Approve  [r] Reject  [Enter] Details  [f] Filter  [I] Inspector  [?] Help  |
+--------------------------------------------------------------------------------+
```

### 2.1 Approvals List Panel

- Groups approvals by status: PENDING, APPROVED, REJECTED
- Pending shown first (actionable items)
- Shows approval type, name, and relative time
- Selection cursor (`>`) indicates current item

### 2.2 Inspector Panel

- Full approval context for selected item
- Shows requester (AI agent or user)
- Displays request message/reason
- Shows session ID for traceability
- Action hints at bottom

---

## 3. State Machine

```
                    +------------------+
                    |   ApprovalList   |
                    | (default state)  |
                    +--------+---------+
                             |
          +------------------+------------------+
          |                  |                  |
    [Enter]                [a]                [r]
          |                  |                  |
          v                  v                  v
+------------------+ +------------------+ +------------------+
| ApprovalDetail   | | ApprovalConfirm  | | ApprovalConfirm  |
| (readonly view)  | | (approve dialog) | | (reject dialog)  |
+--------+---------+ +--------+---------+ +--------+---------+
         |                    |                    |
       [Esc]             [Enter/Esc]          [Enter/Esc]
         |                    |                    |
         v                    v                    v
                    +------------------+
                    |   ApprovalList   |
                    +------------------+
```

### 3.1 State Descriptions

| State | Description |
|-------|-------------|
| `ApprovalList` | Default state, browsing approvals |
| `ApprovalDetail` | Expanded view of single approval |
| `ApprovalConfirm` | Confirmation dialog for approve/reject |

---

## 4. Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `a` | Approve selected | Pending approvals only |
| `r` | Reject selected | Pending approvals only |
| `Enter` | View details | Opens ApprovalDetail |
| `f` | Filter dialog | Filter by type/status |
| `Tab` | Cycle sections | PENDING -> APPROVED -> REJECTED |
| `I` | Toggle inspector | Show/hide details panel |
| `g` / `G` | First / last | List navigation |
| `Esc` | Back / cancel | Close dialog or return |

**List navigation per tui.md Section 3.2**

### 4.1 Approval Confirmation Dialog

```
+-- Approve Request ------------------------------------------------+
|                                                                    |
|   Approve "Schema: orders_v2"?                                     |
|                                                                    |
|   This will:                                                       |
|   * Allow schema to be used in production                          |
|   * Enable runs using this schema contract                         |
|                                                                    |
|   [Enter] Confirm  [Esc] Cancel                                    |
+--------------------------------------------------------------------+
```

### 4.2 Rejection Dialog

```
+-- Reject Request -------------------------------------------------+
|                                                                    |
|   Reject "Schema: orders_v2"?                                      |
|                                                                    |
|   Reason (optional):                                               |
|   +--------------------------------------------------------------+ |
|   | Schema needs additional validation columns                   | |
|   +--------------------------------------------------------------+ |
|                                                                    |
|   [Enter] Confirm  [Esc] Cancel                                    |
+--------------------------------------------------------------------+
```

---

## 5. Data Model

```rust
/// Approval status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

/// Type of approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalType {
    Schema,      // Schema contract approval
    Run,         // Job execution approval
    Plugin,      // Plugin registration approval
    Sink,        // Sink configuration approval
    Query,       // Query execution approval
}

/// Approval request information
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    pub id: String,
    pub approval_type: ApprovalType,
    pub name: String,
    pub description: String,
    pub status: ApprovalStatus,
    pub session_id: Option<String>,
    pub requester: String,
    pub request_message: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub rejection_reason: Option<String>,
}

/// View state for Approvals
#[derive(Debug)]
pub struct ApprovalsViewState {
    pub state: ApprovalsState,
    pub approvals: Vec<ApprovalInfo>,
    pub selected_index: usize,
    pub filter_status: Option<ApprovalStatus>,
    pub filter_type: Option<ApprovalType>,
    pub inspector_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalsState {
    ApprovalList,
    ApprovalDetail,
    ApprovalConfirm { action: ConfirmAction },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    Approve,
    Reject,
}
```

---

## 6. Implementation Notes

### 6.1 Refresh Strategy

- Auto-refresh every 5 seconds for pending approvals
- Pause refresh while confirmation dialog is open
- Manual refresh with `r` key

### 6.2 Status Indicators

| Status | Symbol | Color |
|--------|--------|-------|
| Pending | `[PEND]` | Yellow |
| Approved | `[APPR]` | Green |
| Rejected | `[REJ]` | Red |

### 6.3 Integration Points

- Connects to MCP server for approval actions
- Session ID links to Sessions view
- Approval events logged for audit trail

---

## 7. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-21 | 1.0 | Initial spec for MCP approval management view |

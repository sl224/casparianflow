# Sessions View - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.1
**Related:** specs/views/approvals.md, docs/intent_pipeline_workflow.md, docs/decisions/ADR-021-ai-agentic-iteration-workflow.md
**Last Updated:** 2026-01-25

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Sessions** view provides visibility into intent pipeline workflows. Each session represents an AI-assisted workflow moving through approval gates (G1-G6) from intent to production deployment.

### 1.1 Data Source

```
~/.casparian_flow/sessions/

Session bundle layout:
sessions/{session_id}/
  manifest.json          # includes state = IntentState::as_str()
  proposals/             # selection/tag_rules/path_fields/schema_intent/etc
  approvals.jsonl        # DecisionRecord history
  reports/               # backtest reports
  logs/                  # tool logs
```

Notes:
- `manifest.state` stores the canonical `IntentState` string (e.g., `S0_INTERPRET_INTENT`).
- The TUI derives pending gates from `IntentState::is_gate()` and `gate_number()`.

### 1.2 User Goals

| Goal | How Sessions Helps |
|------|---------------------|
| "See workflow progress" | Visual gate progression G1-G6 |
| "Review pending gates" | Actionable sessions highlighted |
| "Understand AI proposals" | Inspector shows intent details |
| "Approve gate transitions" | Direct gate approval actions |

---

## 2. Layout

```
+- Casparian Flow | View: Sessions | Active: 3 | Awaiting: 2 | Complete: 15 ------+
+- Rail -----------+- Sessions List ---------------------+- Inspector --------------+
| [0] Home         | AWAITING APPROVAL (2)               | Session S-1842           |
| [1] Discover     | > [G3] S-1842 orders_parser  5m ago | Intent: Parse orders     |
| [2] Parser Bench |   [G5] S-1839 trades_ingest  1h ago | Current Gate: G3         |
| [3] Jobs         |                                     | (Schema Approval)        |
| [4] Sources      | ACTIVE (3)                          |                          |
| [5] Approvals    |   [G2] S-1845 email_proc    2m ago  | Progress:                |
| [6] Query        |   [G4] S-1843 hl7_parser   30m ago  | [G1]--[G2]--[G3]--[ ]    |
| [7] Sessions     |   [G1] S-1846 new_source    1m ago  |  ok    ok   WAIT         |
|                  |                                     |                          |
| Filter: All      | COMPLETED (15)                      | Proposal:                |
|                  |   [G6] S-1830 fix_daily    2d ago   | "Create parser for       |
|                  |   [G6] S-1825 citi_trades  3d ago   | orders.csv with schema:  |
|                  |   ...                               | id INT, customer TEXT,   |
|                  |                                     | amount DECIMAL..."       |
|                  |                                     |                          |
|                  |                                     | [Enter] Gate details     |
|                  |                                     | [a] Approve gate         |
+------------------+-------------------------------------+--------------------------+
| [Enter] Details  [a] Approve Gate  [r] Reject  [f] Filter  [I] Inspector  [?]    |
+---------------------------------------------------------------------------------+
```

### 2.1 Sessions List Panel

- Groups sessions by status: AWAITING APPROVAL, ACTIVE, COMPLETED
- Shows current gate (G1-G6) with visual indicator
- Displays session ID, name, and age
- Selection cursor indicates current item

### 2.2 Gate Progress Visualization

```
Progress:
[G1]--[G2]--[G3]--[G4]--[G5]--[G6]
 ok    ok   WAIT   -     -     -

Legend:
 ok   = Gate approved (green)
 WAIT = Awaiting approval (yellow)
 FAIL = Gate rejected (red)
 -    = Not yet reached (gray)
```

### 2.3 Inspector Panel

- Session metadata and intent description
- Current gate status and requirements
- Proposal content (schema, parser code, etc.)
- Action hints for current gate

---

## 3. Gate Definitions (G1-G6)

| Gate | Name | Approval Scope | Auto/Manual |
|------|------|----------------|-------------|
| G1 | Selection Approval | Confirm selection + corpus snapshot | Manual |
| G2 | Tag Rules Approval | Enable persistent tagging rules | Manual |
| G3 | Path Fields Approval | Approve derived fields + naming | Manual |
| G4 | Schema Approval | Approve schema intent | Manual |
| G5 | Publish Approval | Approve publish plan (schema + parser) | Manual |
| G6 | Run Approval | Approve run/backfill scope | Manual |

**Current TUI support:** G1 approval/reject is wired. Other gates are displayed but not yet actionable.

### 3.1 Gate Detail View

```
+-- Gate G3: Schema Approval ---------------------------------------------------+
|                                                                                |
|   Session: S-1842                                                              |
|   Intent: "Parse orders.csv files with customer and amount columns"            |
|                                                                                |
|   Proposed Schema:                                                             |
|   +--------------------------------------------------------------------------+ |
|   | CREATE TABLE orders (                                                    | |
|   |   id INTEGER PRIMARY KEY,                                                | |
|   |   customer_name TEXT NOT NULL,                                           | |
|   |   order_total DECIMAL(10,2),                                             | |
|   |   created_at TIMESTAMP                                                   | |
|   | );                                                                        | |
|   +--------------------------------------------------------------------------+ |
|                                                                                |
|   Backtest Results: 142/150 files passed (94.7%)                               |
|   Quarantine: 8 files with parsing errors                                      |
|                                                                                |
|   [a] Approve  [r] Reject  [v] View failures  [Esc] Back                       |
+--------------------------------------------------------------------------------+
```

---

## 4. State Machine

```
                    +------------------+
                    |   SessionList    |
                    | (default state)  |
                    +--------+---------+
                             |
                           [Enter]
                             |
                +------------+-------------+
                |                          |
                v                          v
       +------------------+       +------------------+
       |  SessionDetail   |       |   GateApproval   |
       | (workflow view)  |       | (G1 approve/rej) |
       +--------+---------+       +--------+---------+
                |                          |
               [w]                    [a]/[r]/[Esc]
                |                          |
                v                          v
       +------------------+        +---------------+
       | WorkflowProgress |        |  SessionList  |
       +--------+---------+        +---------------+
                |
              [Esc]
                |
                v
       +------------------+
       |  SessionDetail   |
       +------------------+
```

### 4.1 State Descriptions

| State | Description |
|-------|-------------|
| `SessionList` | Default state, browsing sessions |
| `SessionDetail` | Expanded view of session workflow + shortcuts |
| `WorkflowProgress` | Gate progression diagram |
| `GateApproval` | Approve/reject pending gate (G1 wired) |
| `ProposalReview` | Reserved for proposal detail (not wired yet) |

---

## 5. Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `Enter` | Open session | If pending gate, opens approval; otherwise detail view |
| `n` | New session | Opens command palette in Intent mode |
| `w` | Workflow diagram | From session detail |
| `a` / `Enter` | Approve gate | Gate approval view |
| `r` | Reject gate | Gate approval view |
| `j` | Jump to Jobs | From session detail |
| `q` | Jump to Query | From session detail |
| `d` | Jump to Discover | From session detail |
| `r` | Refresh list | Session list |
| `Esc` | Back / cancel | Close dialog or return |

**List navigation per tui.md Section 3.2**

### 5.1 Gate Approval Confirmation

```
+-- Approve Gate G3 (Schema Approval) ------------------------------------------+
|                                                                                |
|   Approve schema for session S-1842?                                           |
|                                                                                |
|   This will:                                                                   |
|   * Lock schema contract for production use                                    |
|   * Advance session to G4 (Parser Validation)                                  |
|   * Enable backtest execution                                                  |
|                                                                                |
|   [Enter] Confirm  [Esc] Cancel                                                |
+--------------------------------------------------------------------------------+
```

### 5.2 Gate Rejection Dialog

```
+-- Reject Gate G3 (Schema Approval) -------------------------------------------+
|                                                                                |
|   Reject schema for session S-1842?                                            |
|                                                                                |
|   Reason:                                                                      |
|   +--------------------------------------------------------------------------+ |
|   | Missing customer_id foreign key constraint                               | |
|   +--------------------------------------------------------------------------+ |
|                                                                                |
|   This will:                                                                   |
|   * Return session to G2 for revision                                          |
|   * Notify AI agent with rejection reason                                      |
|                                                                                |
|   [Enter] Confirm  [Esc] Cancel                                                |
+--------------------------------------------------------------------------------+
```

---

## 6. Data Model

```rust
/// Session status based on gate progression
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    AwaitingApproval,  // Blocked on manual gate
    Active,            // In progress (auto gates or working)
    Completed,         // Reached G6
    Failed,            // Gate rejected, not recovered
    Cancelled,         // User cancelled
}

/// Gate identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    G1,  // Intent Confirmation
    G2,  // Schema Proposal
    G3,  // Schema Approval
    G4,  // Parser Validation
    G5,  // Deployment Approval
    G6,  // Production Release
}

/// Gate status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    NotReached,
    Pending,
    Approved,
    Rejected,
}

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub intent: String,
    pub status: SessionStatus,
    pub current_gate: Gate,
    pub gates: Vec<GateInfo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub requester: String,
}

/// Gate information
#[derive(Debug, Clone)]
pub struct GateInfo {
    pub gate: Gate,
    pub status: GateStatus,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub artifacts: Vec<ArtifactRef>,
}

/// Reference to gate artifact
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    pub artifact_type: ArtifactType,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactType {
    Schema,
    Parser,
    BacktestResult,
    Config,
}

/// View state for Sessions
#[derive(Debug)]
pub struct SessionsViewState {
    pub state: SessionsState,
    pub sessions: Vec<SessionInfo>,
    pub selected_index: usize,
    pub filter_status: Option<SessionStatus>,
    pub filter_gate: Option<Gate>,
    pub inspector_visible: bool,
    pub selected_gate: Option<Gate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionsState {
    SessionList,
    SessionDetail,
    GateView,
    GateApproval { action: GateAction },
    FilterDialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAction {
    Approve,
    Reject,
}
```

---

## 7. Implementation Notes

### 7.1 Gate Progression Rules

- Gates must be approved in order (G1 -> G2 -> ... -> G6)
- Rejection returns to previous gate for revision
- G4 (Parser Validation) can auto-approve if backtest passes
- G6 requires explicit confirmation even if all tests pass

### 7.2 Status Indicators

| Status | Symbol | Color |
|--------|--------|-------|
| Awaiting Approval | `[G#]` | Yellow |
| Active | `[G#]` | Blue |
| Completed | `[G6]` | Green |
| Failed | `[FAIL]` | Red |

### 7.3 Refresh Strategy

- Auto-refresh every 5 seconds for active sessions
- Pause refresh while approval dialog is open
- Event-driven refresh on gate transitions

### 7.4 Integration Points

- Links to Approvals view for pending gates
- Links to Jobs view for backtest results
- Links to Parser Bench for parser artifacts
- Connects to MCP server for gate actions

---

## 8. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-21 | 1.0 | Initial spec for intent pipeline sessions view |

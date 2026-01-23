# Query View - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.0
**Related:** specs/views/jobs.md, crates/casparian_db
**Last Updated:** 2026-01-21

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Query** view provides an SQL query console for ad-hoc DuckDB queries. Users can explore data, validate transformations, and inspect pipeline outputs directly from the TUI.

### 1.1 Data Source

```
~/.casparian_flow/casparian_flow.duckdb

Direct SQL access to all tables:
├── cf_*               # Core system tables
├── scout_*            # Discovery tables
├── schema_*           # Schema contract tables
├── mcp_*              # MCP/intent tables
└── <user_outputs>     # Pipeline output tables
```

### 1.2 User Goals

| Goal | How Query Helps |
|------|-----------------|
| "Explore my data" | Direct SQL access to all tables |
| "Validate transformations" | Query output tables directly |
| "Debug pipeline issues" | Inspect system tables |
| "Run quick analysis" | Ad-hoc queries with results |

---

## 2. Layout

```
+- Casparian Flow | View: Query | DB: DuckDB | Tables: 42 | Last: 250ms ----------+
+- Rail -----------+- SQL Editor (60%) ------------------------------------------+
| [0] Home         | SELECT                                                       |
| [1] Discover     |   id,                                                        |
| [2] Parser Bench |   customer_name,                                             |
| [3] Jobs         |   order_total,                                               |
| [4] Sources      |   created_at                                                 |
| [5] Approvals    | FROM orders_v2                                               |
| [6] Query        | WHERE order_total > 1000                                     |
| [7] Sessions     | ORDER BY created_at DESC                                     |
|                  | LIMIT 100;                                                   |
|                  | _                                                            |
| Tables:          +--------------------------------------------------------------+
| > cf_parsers     | Results (234 rows, 250ms)                                    |
|   cf_jobs        +--------------------------------------------------------------+
|   orders_v2      | id   | customer_name | order_total | created_at              |
|   trades_v1      +------+---------------+-------------+-------------------------+
|                  | 1842 | Acme Corp     |    12500.00 | 2026-01-21 10:30:00     |
|                  | 1839 | Beta Inc      |     8750.00 | 2026-01-21 09:15:00     |
|                  | 1835 | Gamma LLC     |     5200.00 | 2026-01-20 16:45:00     |
|                  | ...  | ...           |         ... | ...                     |
+------------------+--------------------------------------------------------------+
| [Ctrl+Enter] Execute  [Ctrl+L] Clear  [Tab] History  [Ctrl+T] Tables  [?] Help  |
+---------------------------------------------------------------------------------+
```

### 2.1 SQL Editor Pane (Top)

- Multi-line SQL editor with syntax highlighting
- Line numbers in gutter
- Cursor position indicator
- Supports standard text editing keybindings

### 2.2 Results Table Pane (Bottom)

- Displays query results in tabular format
- Shows row count and execution time
- Scrollable for large result sets
- Column headers with types

### 2.3 Tables Sidebar (Rail)

- Lists available tables
- Selection inserts table name into editor
- Shows table type (system/user)

---

## 3. State Machine

```
                    +------------------+
                    |     Editing      |
                    | (default state)  |
                    +--------+---------+
                             |
          +------------------+------------------+
          |                  |                  |
    [Ctrl+Enter]           [Tab]          [Ctrl+T]
          |                  |                  |
          v                  v                  v
+------------------+ +------------------+ +------------------+
|    Executing     | |     History      | |   TableBrowser   |
| (query running)  | | (query history)  | | (table list)     |
+--------+---------+ +--------+---------+ +--------+---------+
         |                    |                    |
    [complete]           [Enter/Esc]          [Enter/Esc]
         |                    |                    |
         v                    v                    v
+------------------+         |                    |
|     Viewing      |         |                    |
| (results focus)  |<--------+--------------------+
+--------+---------+
         |
       [Esc]
         |
         v
+------------------+
|     Editing      |
+------------------+
```

### 3.1 State Descriptions

| State | Description |
|-------|-------------|
| `Editing` | Default state, writing SQL in editor |
| `Executing` | Query is running, show spinner |
| `Viewing` | Results displayed, can scroll/copy |
| `History` | Browsing previous queries |
| `TableBrowser` | Exploring available tables |

---

## 4. Keybindings

### 4.1 Editor Mode (Editing State)

| Key | Action | Context |
|-----|--------|---------|
| `Ctrl+Enter` | Execute query | Runs current SQL |
| `Ctrl+L` | Clear editor | Clears SQL text |
| `Tab` | Open history | Browse previous queries |
| `Ctrl+T` | Table browser | List available tables |
| `Ctrl+S` | Save query | Save to query library |
| `Ctrl+O` | Open saved | Load saved query |

### 4.2 Results Mode (Viewing State)

| Key | Action | Context |
|-----|--------|---------|
| `j` / `k` | Scroll rows | Navigate results |
| `h` / `l` | Scroll columns | Wide result sets |
| `g` / `G` | First / last row | Results navigation |
| `y` | Copy cell | Copy selected cell value |
| `Y` | Copy row | Copy entire row as CSV |
| `Ctrl+Y` | Copy all | Copy all results as CSV |
| `e` | Export dialog | Export results to file |
| `Esc` | Return to editor | Back to Editing state |

### 4.3 History Mode

| Key | Action | Context |
|-----|--------|---------|
| `j` / `k` | Navigate history | Select previous query |
| `Enter` | Load query | Insert into editor |
| `/` | Search history | Filter by content |
| `d` | Delete entry | Remove from history |
| `Esc` | Close | Return to previous state |

### 4.4 Text Editing (Standard)

| Key | Action |
|-----|--------|
| `Ctrl+a` | Select all |
| `Ctrl+u` | Clear line |
| `Ctrl+w` | Delete word |
| Arrow keys | Move cursor |
| `Ctrl+Arrow` | Move by word |
| `Home` / `End` | Start/end of line |

---

## 5. Data Model

```rust
/// Query execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryState {
    Editing,
    Executing,
    Viewing,
    History,
    TableBrowser,
}

/// Query result representation
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub execution_time_ms: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub width: usize,
}

/// Query history entry
#[derive(Debug, Clone)]
pub struct QueryHistoryEntry {
    pub id: i64,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub execution_time_ms: u64,
    pub row_count: Option<usize>,
    pub error: Option<String>,
}

/// View state for Query
#[derive(Debug)]
pub struct QueryViewState {
    pub state: QueryState,
    pub editor_content: String,
    pub cursor_position: (usize, usize),  // (line, col)
    pub result: Option<QueryResult>,
    pub error: Option<String>,
    pub history: Vec<QueryHistoryEntry>,
    pub history_index: usize,
    pub tables: Vec<TableInfo>,
    pub selected_table: usize,
    pub result_scroll: (usize, usize),  // (row, col)
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub table_type: TableType,
    pub row_count: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableType {
    System,
    User,
    View,
}
```

---

## 6. Implementation Notes

### 6.1 Query Execution

- Execute queries via DuckDB connection pool
- Limit results to 10,000 rows by default
- Show `LIMIT` warning if truncated
- Timeout after 30 seconds with cancel option

### 6.2 Syntax Highlighting

Keywords highlighted:
- `SELECT`, `FROM`, `WHERE`, `JOIN`, `GROUP BY`, `ORDER BY` - Blue
- `INSERT`, `UPDATE`, `DELETE`, `CREATE`, `DROP` - Red (destructive)
- String literals - Green
- Numbers - Cyan
- Comments - Gray

### 6.3 Query History

- Store last 100 queries
- Persist across sessions
- Include execution stats
- Searchable by content

### 6.4 Export Options

```
+-- Export Results -------------------------------------------------+
|                                                                    |
|   Format:  (*) CSV  ( ) JSON  ( ) Parquet                          |
|                                                                    |
|   Path: ~/exports/query_results.csv                                |
|   +--------------------------------------------------------------+ |
|   | ~/exports/query_results.csv_                                 | |
|   +--------------------------------------------------------------+ |
|                                                                    |
|   [Enter] Export  [Esc] Cancel                                     |
+--------------------------------------------------------------------+
```

### 6.5 Safety Features

- Read-only mode by default
- Require confirmation for DDL/DML statements
- Transaction rollback on error
- Query size limits to prevent memory issues

---

## 7. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-21 | 1.0 | Initial spec for SQL query console view |

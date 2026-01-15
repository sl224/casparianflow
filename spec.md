# Casparian Flow - Product Specification

**Version:** 1.0
**Status:** Approved for Implementation
**Date:** January 7, 2026

---

## 1. Executive Summary

**Casparian Flow** is an AI-native data platform designed to transform "dark data" (unstructured, proprietary, or legacy files) into structured, queryable datasets (Parquet/SQL). It targets technical teams in regulated industries (Defense, Healthcare, Finance) who require air-gapped capability, strict governance, and reproducible data pipelines.

Unlike traditional ETL tools that assume standard API sources, Casparian Flow focuses on the **Bronze Layer** of data engineering: reliably parsing messy files from disk into typed Arrow batches using AI-generated, human-verified Python parsers running in sandboxed environments.

### 1.1 Core Philosophy
1.  **AI Generates, Humans Approve**: AI is a proposal engine. It writes code; humans approve the *output* (schema contracts).
2.  **Schema is Contract**: Once approved, a schema is immutable. Deviations result in hard failures or quarantined rows, never silent data corruption.
3.  **Local-First & Air-Gapped**: The system runs entirely on the user's machine or on-prem server. No data leaves the perimeter.
4.  **Modal Architecture**: Optimized for two distinct distinct modes: **Dev** (low friction, stateless) and **Prod** (high reliability, stateful, reproducible).

---

## 2. User Personas & Workflows

### 2.1 Target User: The Technical Data Steward
*   **Role**: Data Engineer, Analyst, or Developer in a regulated environment.
*   **Pain Point**: Has 50,000 CSV/Log files in a non-standard format. Needs to query them SQL. Can write Python but hates maintaining fragile scripts.
*   **Goal**: Turn files into a database with audit trails and lineage.

### 2.2 Primary Workflows

#### A. The "Dev" Loop (Iteration)
*   **Goal**: Rapidly develop a parser for a new file type.
*   **Interface**: CLI (`casparian run`) or MCP (Claude Code).
*   **Action**: User runs a parser against a sample file.
*   **Behavior**: Stateless execution. Uses local python environment. Pipes output to stdout/terminal for debugging. `pdb` works.
*   **Outcome**: A working Python parser file.

#### B. The "Governance" Loop (Registration)
*   **Goal**: Promote a parser to production.
*   **Interface**: CLI (`casparian parser register`) or TUI.
*   **Action**: User registers the parser.
*   **Behavior**: System bundles source code, locks dependencies (`uv.lock`), creates a ZIP artifact, computes hashes, and enforces the Schema Contract.
*   **Outcome**: An immutable, signed Parser Artifact stored in the database.

#### C. The "Prod" Loop (Execution)
*   **Goal**: Process 10TB of backlogged data.
*   **Interface**: Daemon (`casparian start` / `casparian process`).
*   **Action**: System watches directories or processes queue.
*   **Behavior**: Stateful execution. Uses managed venvs (rebuilt from lockfile). Parallel execution. Validation against contracts. Bad rows quarantined.
*   **Outcome**: Structured Parquet files, SQLite/Postgres records, and a `_quarantine` table for invalid rows.

---

## 3. System Architecture

### 3.1 The Modal Architecture
The system supports two execution modes sharing a single core executor.

| Feature | **Dev Mode** (`DevRunner`) | **Prod Mode** (`QueuedRunner`) |
| :--- | :--- | :--- |
| **Command** | `casparian run` | `casparian worker` / `start` |
| **Source** | File on disk | Bundled ZIP from DB |
| **Environment** | Current Shell / `$VIRTUAL_ENV` | Managed `~/.casparian_flow/venvs/` |
| **State** | Stateless (no DB writes req.) | Stateful (Queue + History) |
| **Logging** | stdout/stderr (raw) | Structured logs to disk |
| **Debugging** | Interactive (`pdb` supported) | Non-interactive |

### 3.2 Component Diagram

```
[ User / Claude ]
      │
      ▼
[ Casparian CLI / MCP Server ]
      │
      ├─► [ Scout ] (File Discovery & Tagging)
      │
      ├─► [ Parser Lab ] (AI Generation & Schema Contracts)
      │
      ▼
[ Runner Abstraction ] ───► [ JobStore / ParserStore ] (SQLite)
      │
      ▼
[ Shared Rust Executor ]
      │
      ├─► [ Validation Engine ] (Schema Enforcement)
      │
      ├─► [ Quarantine Manager ] (Bad Row Isolation)
      │
      ▼
[ Bridge Shim (Python) ] ◄── Unix Socket ──► [ Rust Host ]
      │
      ▼
[ Output Sinks ] (Parquet, SQLite, Arrow IPC)
```

---

## 4. Functional Specifications

### FS-1: Discovery (Scout)
*   **Capability**: High-speed filesystem scanning using `ignore` and `glob` patterns.
*   **Tagging**: Automatic classification of files based on content hash, extension, or heuristic rules.
*   **Output**: Populates `cf_files` table with file metadata (path, size, hash, modified_time).
*   **Constraint**: Must support millions of files with low memory footprint.

### FS-2: Parser Management
*   **Progressive API**:
    *   *Level 1 (Function)*: `def parse(path)` - For quick iteration.
    *   *Level 2 (Class)*: `class Parser` with `name`, `version`, `outputs` - For production registration.
*   **Bundling**:
    *   **Source-Only**: Only bundles source code (`.py`, `.json`, `.yaml`).
    *   **Lockfile Mandatory**: Production parsers MUST have a `uv.lock`.
    *   **Exclusion**: Explicitly excludes `.venv`, `__pycache__`, and native binaries (`.so`, `.dll`) to ensure cross-platform reproducibility.
*   **Artifacts**: Stored as ZIP blobs in SQLite with SHA256 integrity checks.

### FS-3: Execution Engine (The Bridge)
*   **Isolation**: Python code runs in a subprocess (Guest).
*   **Communication**: TCP localhost (`tcp://127.0.0.1:{port}`) using ZMQ with binary protocol `[LENGTH:4][ARROW_IPC_BYTES]`. TCP chosen over Unix sockets for cross-platform consistency.
    *   **IMPLEMENTATION NOTE (Jan 2026)**: Current codebase uses Unix sockets (`AF_UNIX`). Migration to TCP required for Windows support. See `bridge.rs` and `bridge_shim.py`.
*   **Privilege Separation**: Guest has no access to Host credentials.
*   **Dev Mode Debugging**: Uses `Stdio::inherit()` to enable interactive debugging (`pdb`). Ctrl+C handled via process group signaling.
    *   **IMPLEMENTATION NOTE**: Current codebase uses `Stdio::piped()` which breaks pdb. Dev mode MUST switch to `Stdio::inherit()`.
*   **Serialization**:
    *   Shim MUST use `safe_to_arrow` with string fallback for mixed-type columns.
    *   **IMPLEMENTED**: `bridge_shim.py` now has `safe_to_arrow()` that catches `ArrowInvalid`/`ArrowTypeError` and converts problematic columns to strings. Uses single-pass array building on fallback path for efficiency.
    *   Prevents worker crashes on data quality issues; ensures bad data reaches Rust for quarantine.
*   **Memory Safety**: Before `safe_to_arrow` conversion, check available system RAM. If available memory < 3× batch size, abort job with `OOM_RISK` error. User must chunk their data.

### FS-4: Validation & Quarantine
*   **Logic**: Validation happens in **Rust**, not Python.
*   **Schema Contract**: Defined in Rust types (`SchemaContract`).
*   **Flow**:
    1.  Python streams raw Arrow batches (potentially with string-fallback columns).
    2.  Rust validates each row against the Contract.
    3.  **Valid Rows**: Written to primary Output Sink (Parquet/SQL).
    4.  **Invalid Rows**: Written to `cf_quarantine` table with error reason.
*   **Status**: Job marked as `completed_with_warnings` if quarantine > 0.

### FS-5: Reliability & Orchestration
*   **Zombie Detection**:
    *   Uses **Heartbeats** (updated every 60s by worker).
    *   Stale jobs (>5m no heartbeat) are requeued by the Daemon.
    *   On startup, Daemon resets orphaned jobs from previous runs.
*   **Concurrency**: Configurable worker pool (`MAX_CONCURRENT_WORKERS`).
*   **Atomic Claiming**: Uses `UPDATE ... RETURNING` pattern to prevent race conditions.

### FS-6: Backfill & Schema Evolution
*   **Trigger**: Manual only. User runs `casparian backfill <parser>`. No auto-detection on `casparian start`.
*   **Atomic Output**:
    *   Backfill writes to `.staging/{job_id}/` directory.
    *   On successful completion of ALL files, atomic rename to final output path.
    *   On crash/failure, `.staging/` is cleaned up; original output remains untouched.
*   **Version Tracking**: `cf_processing_history` tracks (input_hash, parser_name, parser_version). Backfill reprocesses files where current parser version > processed version.
*   **No Mixed State**: Query layer always sees consistent schema state. Never partially-migrated datasets.

### FS-7: Log Management & Routing
*   **Sideband Protocol**: Guest sends logs via IPC sideband channel (`LOG_SIGNAL = 0xFFFFFFFE`).
    *   Protocol: `[LOG_SIGNAL:4][LEVEL:1][LENGTH:4][MESSAGE]`
    *   Guest knows nothing about Dev vs Prod - just sends to sideband.
*   **Host-Side Routing** (the key mechanism):
    *   **DevRunner**: Prints sideband logs to stdout/stderr immediately. No buffering.
    *   **QueuedRunner**: Writes sideband logs to job log file on disk.
*   **Per-Job Cap**: Maximum 10MB log output per job. Tail truncation after limit.
*   **Rotation**: Global rotation policy: keep last 7 days OR 1GB total, whichever is smaller.
*   **Location**: `~/.casparian_flow/logs/{date}/{job_id}.log`

### FS-8: Error Handling & Retry Semantics
*   **Structured Error Codes** (Python → Rust):
    *   Python shim classifies exceptions into structured `error_code` field in JSON output.
    *   Rust parses `error_code` directly; falls back to string matching for legacy compatibility.
    *   Error code mapping:

    | Exception Type | Error Code | Retryable |
    |----------------|------------|-----------|
    | `KeyError` | `SCHEMA_MISMATCH` | No |
    | `FileNotFoundError` | `FILE_NOT_FOUND` | No |
    | `PermissionError` | `PERMISSION_ERROR` | No |
    | `UnicodeDecodeError` | `ENCODING_ERROR` | No |
    | `MemoryError` | `MEMORY_ERROR` | Yes |
    | `ValueError` (convert) | `INVALID_DATA` | No |
    | Other | `UNKNOWN_ERROR` | No |

*   **Retry Policy**:
    *   Maximum 3 retries per job with exponential backoff (1s, 4s, 16s).
    *   Only **transient** errors trigger retry (timeout, OOM, connection reset).
    *   **Permanent** errors fail immediately (parse errors, validation failures, missing files).
*   **Exit Code Convention** (Shim → Rust):
    *   `exit 0`: Success.
    *   `exit 1`: Permanent failure (no retry).
    *   `exit 2`: Transient failure (retry eligible).
*   **Dead-Letter Queue**:
    *   After max retries, job moves to `cf_dead_letter` table.
    *   Dead-letter jobs are never auto-deleted. Manual inspection via `casparian jobs --dead-letter`.
    *   Replay with `casparian jobs replay <job_id>`.
*   **Circuit Breaker**:
    *   If a parser fails 5 consecutive files, processing for that parser is **paused**.
    *   System logs alert: `[CIRCUIT_BREAKER] Parser '{name}' paused after 5 consecutive failures`.
    *   Resume manually: `casparian parser resume <name>`.
    *   Circuit breaker state stored in `cf_parser_health` table.

*   **Error Fingerprinting** (for deduplication and catalog):
    *   Errors are fingerprinted to group identical root causes across files/runs.
    *   Fingerprint is a 16-character hex hash of 4 components:

    | Component | Source | Purpose |
    |-----------|--------|---------|
    | Error Type | Exception class name | "SchemaValidationError" vs "ParseError" |
    | Error Code | Structured code (e.g., `SCHEMA_MISMATCH`) | Programmatic classification |
    | Normalized Trace | Stack trace with paths/line numbers stripped | WHERE in code error occurs |
    | Context Structure | Hash of error context keys + types (not values) | Distinguishes error scenarios |

    *   **Stack trace normalization**:
        *   `File "/full/path/to/file.py", line 123, in func` → `File "file.py", in func`
        *   Strips absolute paths (fingerprint stable across machines)
        *   Keeps function names (identifies code location)
        *   Truncates traces > 4KB (first frames are most relevant)
    *   **Context structure hashing**:
        *   Hashes keys and value types, NOT actual values
        *   `{"column": "amount", "expected": "float"}` → hash of `["column:string", "expected:string"]`
        *   Enables: same error type with different values = same fingerprint
    *   **Result**: "47 occurrences of ERR-a1b2c3 across 12 files" instead of 47 separate errors.
    *   **CLI**: `casparian errors list`, `casparian errors show <fingerprint>`, `casparian errors files <fingerprint>`
    *   **Storage**: `cf_errors` table (fingerprint PK), `cf_error_occurrences` table (individual instances)

---

## 5. Interface Specifications

### 5.1 CLI Commands

*   **`casparian run <parser> <input>`**:
    *   Executes parser in Dev Mode.
    *   Flags: `--sink`, `--venv`, `--force`, `--whatif`.
*   **`casparian scan <path>`**:
    *   Scans directory for files.
    *   Flags: `--pattern`, `--tag`, `--json`.
*   **`casparian parser register <path>`**:
    *   Bundles and registers a parser for production.
    *   Validates `uv.lock`.
*   **`casparian start`**:
    *   Starts the Daemon (Sentinel + Worker pool).
*   **`casparian process`**:
    *   Runs batch processing on the queue.
*   **`casparian quarantine list/replay`**:
    *   Tools to inspect and fix quarantined rows.
*   **`casparian backfill <parser>`**:
    *   Re-processes files when parser version changes.
    *   Flags: `--execute`, `--limit`, `--force`.
*   **`casparian parser resume <name>`**:
    *   Clears circuit breaker for a paused parser.
*   **`casparian jobs --dead-letter`**:
    *   Lists jobs in dead-letter queue.
*   **`casparian jobs replay <job_id>`**:
    *   Re-queues a dead-letter job for processing.

### 5.2 MCP Tools (for Claude)

1.  **`quick_scan`**: fast filesystem discovery.
2.  **`apply_scope`**: group files for processing.
3.  **`discover_schemas`**: infer schema from file samples.
4.  **`approve_schemas`**: lock a schema contract.
5.  **`run_backtest`**: validate parser against file history.
6.  **`execute_pipeline`**: trigger production processing.
7.  **`query_output`**: SQL interface to processed data.

### 5.3 Terminal UI (TUI) Specification

The TUI (`casparian tui`) is a **user-driven** interface for iterative/exploratory data pipeline development. Users write parser code in their own IDE; TUI handles orchestration, visibility, and AI assistance.

**Design Philosophy:**
*   **User-driven, not agentic**: User initiates actions; AI assists contextually.
*   **Modal workflows**: Distinct modes for different tasks, accessed from a central hub.
*   **IDE-agnostic**: Parsers are registered via CLI, run from TUI. No in-TUI code editing.

#### Workflow Overview

The TUI enforces a clear separation of concerns across four modes:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           DATA PIPELINE FLOW                              │
│                                                                          │
│   DISCOVER       PARSER BENCH        JOBS            INSPECT            │
│   ─────────      ────────────        ────            ───────            │
│   Organize       Test & Dev          Monitor         Analyze            │
│                                                                          │
│   ┌─────────┐      ┌─────────┐      ┌─────────┐      ┌─────────┐        │
│   │  Scan   │      │ Select  │      │  Track  │      │  Query  │        │
│   │   ↓     │  ──► │   ↓     │  ──► │   ↓     │  ──► │   ↓     │        │
│   │  Tag    │      │  Run    │      │  Retry  │      │ Export  │        │
│   └─────────┘      └─────────┘      └─────────┘      └─────────┘        │
│                                                                          │
│   Files → Tags     Tags → Parsers   Jobs → Status   Tables → Insights   │
└──────────────────────────────────────────────────────────────────────────┘
```

#### Home Hub

The landing view is a **card grid with stats** showing the four workflow modes:

| Mode | Purpose | Quick Stats |
|------|---------|-------------|
| **Discover** | Organize: scan, tag, preview | "142 files, 3 sources" |
| **Parser Bench** | Test & develop parsers | "3 parsers, 2 recent tests" |
| **Inspect** | Analyze: query tables, view stats | "3 tables, 12K rows" |
| **Jobs** | Monitor: track status, handle errors | "1 failed, 8 complete" |

**Navigation**: Arrow keys to select card, Enter to launch mode. `q` to quit. `?` for help.

#### Mode: Discover (Alt+d)

**Purpose**: File *organization* - scan, tag, preview. Prepares files for processing.

> **Full Specification**: See [specs/discover.md](specs/discover.md) for complete details.

**Design**: Source-first workflow with dropdown navigation (telescope.nvim style).
- Sources and Rules as collapsible, filterable dropdowns
- Live file preview as you navigate sources/rules
- Enter confirms selection and moves focus to Files

**Layout**: Three-panel design (Sidebar + Files + Preview).

**Key Features**:
*   **Dropdown Navigation**: Press `1` for Sources, `2` for Rules - opens filterable dropdown
*   **Live Preview**: Navigating sources updates file list in real-time
*   **Immediate Filtering**: Type to filter (no `/` required), including numbers
*   **Source-First Loading**: Files only shown for selected source

**Core Workflow**:
1.  Press `1` to open Sources dropdown
2.  Type to filter, arrows to navigate
3.  Files update as you move (live preview)
4.  Press Enter to confirm and focus Files
5.  Tag files with `t` or bulk tag with `T`

**Connection to Parser Bench**: Files tagged in Discover bind to parsers via topic matching.

**NOT in Discover**: Running parsers (that's Parser Bench mode). Discover organizes; Parser Bench tests and executes.

#### Mode: Parser Bench (Alt+p)

**Purpose**: Parser *development and testing* - the workbench for iterating on parsers with immediate feedback.

> **Full Specification**: See [specs/parser_bench.md](specs/parser_bench.md) for complete details.

**Parsers Directory**: `~/.casparian_flow/parsers/`
- Drop `.py` files here (or symlink from your project)
- Flat structure only, no subdirectories
- Metadata (name, version, topics) extracted via AST parsing

**Key Features**:
*   **Filesystem-first**: Parsers in `parsers/` directory appear automatically
*   **Symlinks for dev**: Symlink from project for live editing workflow
*   **Quick Test**: Press `n` to test any `.py` file (not just in parsers dir)
*   **Smart Sampling**: Prioritize failed files first when selecting test data
*   **Health Tracking**: Circuit breaker status, success rates

**Layout**: Parser list (left) + Detail/Results panel (right).

**Core Workflow**:
1.  Copy/symlink parser to `~/.casparian_flow/parsers/`
2.  Select parser in list, press `t` to test
3.  Select data file (failed files shown first)
4.  View results: schema, preview, errors with suggestions
5.  Edit in IDE, press `r` to re-run

**Connection to Discover**: Files tagged in Discover bind to parsers via topic matching.

#### Mode: Inspect (Alt+i)

**Purpose**: Output *analysis* - explore processed data, run queries, validate results.

*   **Layout**: Output tables list → Stats summary → Drill-down view.
*   **Stats Summary**: Column stats (nulls, uniques, min/max, type). Click to see distribution.
*   **Drill-Down**: Select column to see value histogram, sample rows.
*   **Actions**:
    *   `j/k` = navigate tables
    *   `Enter` = expand table details
    *   `q` = open SQL query input
    *   `f` = filter rows
    *   `e` = export to file (parquet/csv)
*   **AI**: "What anomalies do you see in this data?" or "Write a query to find duplicates".
*   **Connection to Jobs**: Links to job that produced each table.

#### Mode: Jobs (Alt+j)

**Purpose**: Queue *management* - monitor execution, handle failures, view logs.

*   **Layout**: Job list (left) + Expandable detail pane (right).
*   **Job List**: Status (running/pending/failed/complete), parser, file count, duration.
*   **Status Filters**: `1` = all, `2` = running, `3` = failed, `4` = pending.
*   **Actions**:
    *   `j/k` = navigate jobs
    *   `Enter` = expand details (logs, error traces, output paths)
    *   `r` = retry failed job
    *   `c` = cancel running job
    *   `d` = view dead-letter queue
*   **Detail Pane**: Full logs, error traces, output paths, lineage info.
*   **AI**: "Why did this job fail?" or "Show me similar failures".

#### AI Assistant (Persistent Chat Sidebar)

*   **Location**: Right sidebar (30% width), toggle visibility with `Alt+a`.
*   **Focus**: `Tab` switches focus between main content and chat sidebar.
*   **Context-Aware**: AI sees current mode, selected file/parser/job. Responses tailored.
*   **Streaming**: Text streams in real-time. Tool calls shown inline.
*   **Input**: Chat input at bottom. `Enter` to send, `Shift+Enter` for newline.
*   **Scrolling**: `Ctrl+Up/Down` or `PageUp/PageDown` to scroll message history.

#### Global Keybindings

| Key | Action |
|-----|--------|
| `Alt+d` | Go to Discover mode |
| `Alt+p` | Go to Process mode |
| `Alt+i` | Go to Inspect mode |
| `Alt+j` | Go to Jobs mode |
| `Alt+h` | Return to Home Hub |
| `Alt+a` | Toggle AI chat sidebar |
| `Tab` | Switch focus (Main ↔ Chat) when sidebar open |
| `Esc` | Unfocus chat → Return to Home Hub (two-stage) |
| `Ctrl+c` | Quit |

#### Global Application State Machine

The TUI operates as a hierarchical state machine with global navigation and mode-specific sub-states.

```
                                    ┌──────────────────────────────────────────────────────────────┐
                                    │                     GLOBAL LAYER                             │
                                    │                                                              │
                                    │  ┌─────────────┐                                            │
                                    │  │ AI SIDEBAR  │◄─── Alt+a (toggle from ANY state)         │
                                    │  │  (overlay)  │                                            │
                                    │  └─────────────┘                                            │
                                    │                                                              │
                                    │  ┌─────────────┐                                            │
                                    │  │    HELP     │◄─── ? (toggle from ANY state)             │
                                    │  │  (overlay)  │                                            │
                                    │  └─────────────┘                                            │
                                    │                                                              │
                                    │  Ctrl+c ──────────────────────────────────► EXIT            │
                                    └──────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                      MODE LAYER                                                    │
│                                                                                                    │
│                                    ┌─────────────┐                                                │
│          ┌─────────────────────────│  HOME HUB   │─────────────────────────┐                      │
│          │                         │  (initial)  │                         │                      │
│          │                         └──────┬──────┘                         │                      │
│          │                                │                                 │                      │
│          │ Alt+d / Enter[1]               │ Alt+p / Enter[2]               │ Alt+i / Enter[3]     │
│          ▼                                ▼                                 ▼                      │
│   ┌─────────────┐                  ┌─────────────┐                  ┌─────────────┐               │
│   │  DISCOVER   │                  │PARSER BENCH │                  │   INSPECT   │               │
│   │   (mode)    │                  │   (mode)    │                  │   (mode)    │               │
│   └──────┬──────┘                  └──────┬──────┘                  └──────┬──────┘               │
│          │                                │                                 │                      │
│          │ Alt+j / [4]                    │                                 │                      │
│          │          ┌─────────────────────┼─────────────────────────────────┘                      │
│          │          │                     │                                                        │
│          ▼          ▼                     ▼                                                        │
│   ┌─────────────────────────────────────────────┐                                                 │
│   │                  JOBS                        │                                                 │
│   │                 (mode)                       │                                                 │
│   └─────────────────────────────────────────────┘                                                 │
│                                                                                                    │
│   ◄─────────────────────────────────────────────────────────────────────────────────────────────► │
│     Esc (from any mode) returns to HOME HUB                                                        │
│     Alt+h returns to HOME HUB from any state                                                       │
│     Alt+{d,p,i,j} navigates directly between modes                                                 │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     MODE SUB-STATES                                                │
│                                                                                                    │
│   DISCOVER                         PARSER BENCH                    INSPECT                        │
│   ────────                         ────────────                    ───────                        │
│   ┌─────────────┐                  ┌─────────────┐                 ┌─────────────┐                │
│   │ FILES_PANEL │◄── default       │ PARSER_LIST │◄── default      │ TABLE_LIST  │◄── default    │
│   └──────┬──────┘                  └──────┬──────┘                 └──────┬──────┘                │
│          │ 1                              │ Enter                         │ Enter                 │
│          ▼                                ▼                               ▼                       │
│   ┌─────────────┐                  ┌─────────────┐                 ┌─────────────┐                │
│   │  SOURCES    │                  │ PARSER_EDIT │                 │TABLE_DETAIL │                │
│   │  DROPDOWN   │                  │   (focus)   │                 │   (panel)   │                │
│   └──────┬──────┘                  └──────┬──────┘                 └──────┬──────┘                │
│          │ 2                              │ t (test)                      │ q (query)             │
│          ▼                                ▼                               ▼                       │
│   ┌─────────────┐                  ┌─────────────┐                 ┌─────────────┐                │
│   │    TAGS     │                  │ TEST_RUNNER │                 │ SQL_EDITOR  │                │
│   │  DROPDOWN   │                  │   (modal)   │                 │  (overlay)  │                │
│   └──────┬──────┘                  └─────────────┘                 └─────────────┘                │
│          │ g                                                                                       │
│          ▼                                                                                         │
│   ┌─────────────┐                  JOBS                                                           │
│   │    GLOB     │                  ────                                                           │
│   │  EXPLORER   │                  ┌─────────────┐                                                │
│   │  (overlay)  │                  │  JOB_LIST   │◄── default                                     │
│   │   ↓         │                  └──────┬──────┘                                                │
│   │ EXPLORE     │                         │ Enter                                                 │
│   │   ↓         │                         ▼                                                       │
│   │ FOCUSED     │                  ┌─────────────┐                                                │
│   │   ↓         │                  │ JOB_DETAIL  │                                                │
│   │ EDIT_RULE   │                  │   (panel)   │                                                │
│   │   ↓         │                  └──────┬──────┘                                                │
│   │ TEST        │                         │ d                                                     │
│   │   ↓         │                         ▼                                                       │
│   │ PUBLISH     │                  ┌─────────────┐                                                │
│   └─────────────┘                  │ DEAD_LETTER │                                                │
│                                    │   (view)    │                                                │
│                                    └─────────────┘                                                │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**State Machine Rules:**

| Rule | Description |
|------|-------------|
| **GLOBAL_OVERLAY** | AI Sidebar and Help overlay on any mode without changing mode state |
| **ESC_CASCADES** | Esc exits innermost state first: Overlay → Sub-state → Mode → Home |
| **ALT_JUMPS** | Alt+{key} always jumps directly, saving current mode state |
| **MODAL_EXCLUSIVE** | Only one overlay (Sidebar/Help) active at a time |
| **STATE_PRESERVED** | Returning to a mode restores its last sub-state |

**Mode Transition Table:**

| From | To | Trigger | Notes |
|------|----|---------|-------|
| Home | Discover | `Alt+d` or `Enter` on card 1 | |
| Home | Parser Bench | `Alt+p` or `Enter` on card 2 | |
| Home | Inspect | `Alt+i` or `Enter` on card 3 | |
| Home | Jobs | `Alt+j` or `Enter` on card 4 | |
| Any Mode | Home | `Alt+h` or `Esc` (if no sub-state) | State preserved |
| Any Mode | Any Mode | `Alt+{d,p,i,j}` | Direct navigation |
| Any State | AI Sidebar | `Alt+a` | Toggle overlay |
| Any State | Help | `?` | Toggle overlay |
| Any State | Exit | `Ctrl+c` | Confirm if unsaved |

> **Full Sub-State Machines:** See individual spec files for complete sub-state documentation:
> - Discover: `specs/views/discover.md` Sections 4 and 13
> - Parser Bench: `specs/parser_bench.md`
> - Jobs: `specs/views/jobs.md` (planned)
> - Inspect: `specs/views/inspect.md` (planned)

#### Mouse Support (Basic)

*   Scroll wheel for lists/content.
*   Click to select items.
*   Click cards in Home Hub to enter mode.

#### Error Display

*   Errors appear as red-styled inline messages.
*   Failed jobs show error icon in list, full trace in detail pane.

---

## 6. Data Strategy

### 6.1 Database Schema (SQLite)

All tables use single database: `~/.casparian_flow/casparian_flow.sqlite3`

**Scout Tables (File Discovery):**
*   **`scout_sources`**: Data sources (directories to watch). Columns: id, name, source_type, path, poll_interval_secs, enabled.
*   **`scout_files`**: Discovered files with tags. Columns: id, source_id (FK), path, rel_path, size, mtime, content_hash, status, tag, tag_source, rule_id.
*   **`scout_tagging_rules`**: Pattern → tag mappings. Columns: id, name, source_id (FK), pattern, tag, priority, enabled.

**Parser Tables (CLI `run` command):**
*   **`cf_parsers`**: Parser registry. Columns: id (UUID), name, version, source_hash, source_code, registered_at.
*   **`cf_parser_topics`**: Parser → topic subscriptions. Columns: parser_id (FK), topic.
*   **`cf_processing_history`**: Dedup tracking. Columns: input_hash, parser_name, parser_version, processed_at, job_id.

**Sentinel Tables (Job Orchestration):**
*   **`cf_processing_queue`**: Job queue. Columns: id, plugin_name, file_version_id, status (QUEUED/RUNNING/COMPLETED/FAILED), priority, retry_count.
*   **`cf_dead_letter`**: Jobs that exhausted retries. Columns: original_job_id, plugin_name, error_message, retry_count, moved_at, reason.
*   **`cf_parser_health`**: Circuit breaker state. Columns: parser_name, consecutive_failures, paused_at, last_failure_reason, total_executions.

**Validation Tables:**
*   **`cf_quarantine`**: Row-level validation errors for inspection.

### 6.2 File Formats
*   **Output**: Apache Parquet (columnar, compressed) or Arrow IPC.
*   **Interchange**: Arrow IPC (zero-copy where possible, streaming).

### 6.3 Environment Management
*   **Tool**: `uv` (managed by Rust `EnvManager`).
*   **Cache**: `~/.casparian_flow/venvs/{lockfile_hash}`.
*   **Offline**: `casparian vendor` command + `--offline` flag.

---

## 7. Security & Compliance

1.  **Sandboxing**: Parsers run in isolated subprocesses.
2.  **Input Validation**: "Gatekeeper" AST analysis scans parser code for dangerous imports (e.g., `socket`, `subprocess` - though `subprocess` is used by the host to spawn the guest, the guest code itself is restricted).
3.  **Air-Gap**: No telemetry. No auto-updates. Offline dependency resolution supported.
4.  **Audit Trail**: Every job execution, schema change, and parser version is logged in SQLite.
5.  **Platform Support**:
    *   **v1 (Current)**: macOS, Linux. Windows support planned but not blocking.
    *   **IMPLEMENTATION NOTE**: Current codebase uses Unix sockets (`AF_UNIX`). Migration to TCP (`tcp://127.0.0.1:port`) required for Windows. See FS-3.

---

## 8. Implementation Roadmap

*   **Phase 1**: Storage Abstraction (Repository Pattern).
*   **Phase 2**: Unified Schema & Bundling (ZIP artifacts).
*   **Phase 3**: Protocol Convergence (Rust Executor + Universal Shim).
*   **Phase 4**: Worker Loop (Heartbeats).
*   **Phase 5**: Quarantine Implementation (Rust-side validation).
*   **Phase 6**: MCP Integration.

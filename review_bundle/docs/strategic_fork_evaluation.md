# Strategic Fork Evaluation: Casparian Flow Go-to-Market

**Date:** January 20, 2026
**Status:** Decision Pending
**Purpose:** Provide complete context for evaluating go-to-market strategy

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Product Overview](#2-product-overview)
3. [Core Technical Architecture](#3-core-technical-architecture)
4. [The Parser Development Loop (Core Differentiator)](#4-the-parser-development-loop-core-differentiator)
5. [Feature Set](#5-feature-set)
6. [Vertical Markets Under Consideration](#6-vertical-markets-under-consideration)
7. [The Trade Desk Opportunity (Deep Dive)](#7-the-trade-desk-opportunity-deep-dive)
8. [Validated Persona Research](#8-validated-persona-research)
9. [Competitive Landscape](#9-competitive-landscape)
10. [The Strategic Fork](#10-the-strategic-fork)
11. [Option A: Trade Desk as Beachhead](#11-option-a-trade-desk-as-beachhead)
12. [Option B: Parser Writers as Primary Target](#12-option-b-parser-writers-as-primary-target)
13. [Option C: Hybrid Approach](#13-option-c-hybrid-approach)
14. [Key Questions for Evaluation](#14-key-questions-for-evaluation)
15. [Founder's Current Thinking](#15-founders-current-thinking)
16. [Request for Input](#16-request-for-input)

---

## 1. Executive Summary

**Casparian Flow** is a local-first data platform that transforms unstructured "dark data" (files on disk in industry-specific formats) into queryable SQL/Parquet datasets. The product provides infrastructure for teams to write, test, and deploy custom parsers with full governance (lineage, versioning, quarantine, backtest).

**The Core Thesis:** Unstructured data requires custom parsing. Therefore, the core business involves enabling teams to write and maintain custom parsers reliably.

**The Strategic Fork:** The founder must decide between two go-to-market approaches:

| Path | Target | Uses Core Value (Parser Dev Loop)? | Time to Revenue |
|------|--------|-----------------------------------|-----------------|
| **A: Trade Desk** | Trade Support Analysts (FIX logs) | NO - they use premade parser | Fast (weeks) |
| **B: Parser Writers** | Technical teams (custom formats) | YES - full platform value | Slower (months) |

**The Tension:** Trade Desk offers faster cash flow and lower-pressure UI feedback, but they don't write parsers—so their feedback won't improve the core platform. Parser writers use the full platform but are harder to reach.

---

## 2. Product Overview

### 2.1 What is Casparian Flow?

Casparian Flow is a **local-first data platform** for transforming industry-specific file formats into queryable datasets.

**The Problem It Solves:**

Teams have "dark data"—files on disk in formats like FIX logs, HL7 messages, satellite telemetry, proprietary exports—that they can't easily query. Current options are:

| Option | Problem |
|--------|---------|
| **Enterprise platforms** (Databricks, Palantir) | $50K-$500K/year; cloud-only; requires data team |
| **DIY Python scripts** | No governance; knowledge lost when author leaves; no audit trail |
| **Vendor services** | $5-15K per engagement; slow; recurring cost |
| **Manual analysis** (grep, Excel) | 30-45 minutes per query; error-prone; doesn't scale |

**Casparian's Solution:**

1. **User writes a parser** (Python) that transforms their format → structured data
2. **Infrastructure handles:** schema validation, lineage tracking, quarantine (bad rows), versioning, backtest
3. **Output:** SQL-queryable tables (DuckDB, Parquet, Postgres)

### 2.2 Key Principles

| Principle | Meaning |
|-----------|---------|
| **Local-first** | Data never leaves the user's machine; works air-gapped |
| **Parsers are the product** | Core value is transforming arcane formats to SQL |
| **Governance built-in** | Schema contracts, audit trails, lineage are core, not add-ons |
| **User writes parsers** | Platform is infrastructure, not a library of premade parsers |

### 2.3 North Star

> "Query your dark data in SQL. Locally. With full audit trails."

---

## 3. Core Technical Architecture

### 3.1 System Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                        USER INTERFACE                               │
│                  (CLI / TUI / Tauri Desktop App)                    │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         SCOUT (Discovery)                           │
│  - Scans directories for files                                      │
│  - Tags files by pattern (*.fix → "fix_logs")                       │
│  - Tracks file metadata, hashes                                     │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      PARSER REGISTRY                                │
│  - Stores parser metadata (name, version, topics)                   │
│  - Content-addressed identity (blake3 hash of parser code)          │
│  - Version conflict detection (same name+version, different code)   │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    WORKER (Bridge Mode Execution)                   │
│  - Spawns isolated Python subprocess per parser                     │
│  - Parser runs in sandboxed venv                                    │
│  - Host handles credentials, sinks, governance                      │
│  - Guest (parser) only sees input file, outputs Arrow batches       │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       OUTPUT SINKS                                  │
│  - DuckDB (default, embedded)                                       │
│  - Parquet files                                                    │
│  - PostgreSQL (enterprise)                                          │
│  - CSV (export)                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Tech Stack

| Component | Technology |
|-----------|------------|
| Core | Rust (Tokio async runtime) |
| Database | DuckDB (embedded), PostgreSQL (enterprise) |
| Data format | Apache Arrow, Parquet |
| Parser runtime | Python (isolated subprocess) |
| IPC | ZeroMQ (Arrow IPC batches) |
| CLI | Clap (Rust) |
| TUI | Ratatui (Rust) |
| Desktop (planned) | Tauri 2.0 (Rust + React) |

### 3.3 Parser Interface

Users write parsers as Python classes:

```python
import pyarrow as pa

class MyParser:
    name = 'my_parser'           # Logical name
    version = '1.0.0'            # Semver version
    topics = ['my_data']         # Topics to subscribe to
    outputs = {
        'parsed_data': pa.schema([
            ('id', pa.int64()),
            ('value', pa.string()),
            ('timestamp', pa.timestamp('us')),
        ])
    }

    def parse(self, ctx):
        # ctx.input_path - path to input file
        # ctx.source_hash - hash of input file
        # ctx.job_id - unique job identifier

        # ... parsing logic ...

        yield ('parsed_data', dataframe)  # Yield (sink_name, data) tuples
```

**Key Properties:**
- Parser declares its outputs (schema)
- Parser yields tuples of (output_name, data)
- Infrastructure handles everything else

---

## 4. The Parser Development Loop (Core Differentiator)

### 4.1 The Problem with DIY Parsers

When teams write parsers as standalone Python scripts:

| Problem | Consequence |
|---------|-------------|
| No backtest | Parser breaks silently on edge cases |
| No versioning | Can't tell which parser version produced which output |
| No lineage | Can't trace output row back to source file |
| No quarantine | Bad rows corrupt entire output |
| No governance | Knowledge lost when author leaves |
| No iteration loop | Fix a bug → re-run everything manually |

### 4.2 Casparian's Parser Development Loop

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PARSER DEVELOPMENT LOOP                          │
│                                                                     │
│   ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐     │
│   │  WRITE  │ ──► │ BACKTEST│ ──► │   FIX   │ ──► │ PUBLISH │     │
│   │ Parser  │     │ Against │     │  Issues │     │ Version │     │
│   │         │     │ Files   │     │         │     │         │     │
│   └─────────┘     └─────────┘     └─────────┘     └─────────┘     │
│        ▲               │                               │           │
│        │               ▼                               ▼           │
│        │    ┌─────────────────────┐    ┌─────────────────────┐    │
│        │    │ FAILURE REPORT      │    │ PRODUCTION RUN      │    │
│        │    │ - Which files failed│    │ - Lineage tracked   │    │
│        │    │ - Why they failed   │    │ - Quarantine active │    │
│        │    │ - Sample bad rows   │    │ - Version recorded  │    │
│        │    └─────────────────────┘    └─────────────────────┘    │
│        │               │                                           │
│        └───────────────┘                                           │
│              (iterate until passing)                               │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.3 Key Platform Features

| Feature | What It Does | Why It Matters |
|---------|--------------|----------------|
| **Backtest** | Run parser against historical files, report failures | Find edge cases before production |
| **High-Failure Tracking** | Prioritize files that failed in previous iterations | Fast iteration (test hard cases first) |
| **Lineage Columns** | Every output row has `_cf_source_hash`, `_cf_parser_version`, `_cf_job_id` | Full traceability |
| **Quarantine** | Bad rows go to quarantine table, not output | Output stays clean |
| **Schema Contracts** | Parser declares output schema; violations are hard failures | No silent data drift |
| **Version Deduplication** | Skip if (input_hash, parser_name, parser_version) already processed | Efficient re-runs |
| **Backfill** | When parser version changes, identify files needing reprocessing | Controlled re-parsing |

### 4.4 This Is The Core Differentiator

The parser development loop is what separates Casparian from:
- **DIY Python scripts** (no governance, no iteration support)
- **Generic ETL tools** (Fivetran, Airbyte - don't handle custom formats)
- **Log viewers** (visualization only, no transformation)

**The thesis:** Teams that parse unstructured data need this loop. It's the infrastructure that makes custom parsing reliable and maintainable.

---

## 5. Feature Set

### 5.1 Current Features (Implemented)

| Feature | Description | Status |
|---------|-------------|--------|
| **Scout (File Discovery)** | Scan directories, tag files by pattern | ✓ Implemented |
| **Parser Registry** | Store parser metadata, version tracking | ✓ Implemented |
| **Worker (Execution)** | Run parsers in isolated subprocess | ✓ Implemented |
| **Backtest** | Validate parser against file set | ✓ Implemented |
| **Lineage Tracking** | Source hash, parser version on every row | ✓ Implemented |
| **Quarantine** | Bad rows separated from output | ✓ Implemented |
| **Schema Contracts** | Declared schemas, violation = failure | ✓ Implemented |
| **DuckDB Sink** | Output to embedded DuckDB | ✓ Implemented |
| **Parquet Sink** | Output to Parquet files | ✓ Implemented |
| **CLI** | Command-line interface | ✓ Implemented |
| **TUI** | Terminal UI with file browser, job monitor | ✓ Implemented |

### 5.2 Planned Features

| Feature | Description | Status |
|---------|-------------|--------|
| **Tauri Desktop App** | Native GUI for non-CLI users | Planned (4 weeks) |
| **Premade Parsers** | Ship FIX, HL7, CoT parsers | Planned |
| **PostgreSQL Sink** | Enterprise database output | Planned |
| **Backtest UI** | Visual backtest results in TUI/Tauri | Partial |

### 5.3 TUI Structure (Existing)

The TUI has 5 views:

| View | Purpose | Key Features |
|------|---------|--------------|
| **Home** | Readiness board | Ready outputs, active runs, warnings |
| **Discover** | File browser + rule builder | Source selection, tagging, rule creation |
| **Parser Bench** | Parser management | List parsers, view metadata, test |
| **Jobs** | Job queue monitor | Running/pending/failed jobs, progress |
| **Settings** | Configuration | Paths, theme, preferences |

---

## 6. Vertical Markets Under Consideration

### 6.1 Overview

| Vertical | Format Examples | Who Writes Parsers | Parser Dev Loop Value | Sales Cycle | Willingness to Pay |
|----------|-----------------|-------------------|----------------------|-------------|-------------------|
| **Financial Services (Trade Desk)** | FIX logs | Nobody (want premade) | LOW | Fast | High |
| **Financial Services (Quant Dev)** | Custom FIX tags | Quant developers | MEDIUM | Medium | High |
| **Healthcare IT** | HL7 variations, EHR exports | Integration engineers | HIGH | Long (12-18mo) | Medium |
| **Defense/Aerospace** | CoT, NITF, PCAP, telemetry | Contractors, analysts | HIGH | Long (SBIR) | Very High |
| **Satellite/Space** | Binary telemetry, CCSDS | Firmware/data engineers | VERY HIGH | Medium | High |
| **IoT/Manufacturing** | Historian exports, MTConnect | Plant engineers | HIGH | Medium | Medium |
| **Data Consultants** | Client-specific formats | Themselves | VERY HIGH | Fast | Medium |
| **DevOps/SRE** | Custom application logs | Platform engineers | MEDIUM | Fast | Medium |

### 6.2 Financial Services - Trade Desk (Deep Dive in Section 7)

### 6.3 Healthcare IT

**Format:** HL7 v2.x messages (ADT, ORM, ORU, etc.)

**The Pain:**
- Every hospital has different HL7 quirks
- Mirth Connect went commercial (March 2025) - $20-30K/year
- Historical archives need analytics (Mirth is real-time routing)
- HIPAA requires data stays local

**Who Writes Parsers:** Integration engineers at hospitals or health IT vendors

**Parser Dev Loop Value:** HIGH - every hospital has variations requiring custom handling

**Challenges:**
- Long sales cycle (12-18 months)
- Compliance requirements (HIPAA)
- Slow procurement

### 6.4 Defense/Aerospace

**Formats:** Cursor on Target (CoT), NITF imagery, STANAG, KLV telemetry, PCAP

**The Pain:**
- Air-gapped environments (can't use cloud)
- Custom formats per system/mission
- Audit trail requirements
- Upstream of Palantir (need to structure data first)

**Who Writes Parsers:** Defense contractors, intelligence analysts, systems integrators

**Parser Dev Loop Value:** VERY HIGH - formats change per mission/system

**Challenges:**
- SBIR/government procurement (slow)
- Security clearances
- Long sales cycles

**Opportunity:** SBIR Phase I could provide $50-250K non-dilutive funding

### 6.5 Satellite/Space

**Formats:** Binary telemetry, CCSDS packets, proprietary spacecraft data

**The Pain:**
- Binary formats unique to each spacecraft/mission
- Ground segment teams parse ad-hoc
- No governance when engineer leaves
- Mission-critical (can't have bad parses)

**Who Writes Parsers:** Firmware engineers, ground segment data teams

**Parser Dev Loop Value:** VERY HIGH - formats change constantly, backtest critical

**Sales Cycle:** Medium (smaller companies are accessible)

### 6.6 Data Consultants

**Formats:** Whatever their clients have

**The Pain:**
- Build custom parsers for every client engagement
- No reusable infrastructure
- Can't demonstrate governance to clients
- Knowledge lost between engagements

**Who Writes Parsers:** Themselves (this is their job)

**Parser Dev Loop Value:** VERY HIGH - they iterate on parsers constantly

**Sales Cycle:** Fast (individuals or small firms)

**Interesting:** This might be the most aligned customer - they use the full platform value and are accessible.

---

## 7. The Trade Desk Opportunity (Deep Dive)

### 7.1 The Pain Point

**T+1 Settlement (Live since May 2024):** US markets moved to T+1 settlement, meaning trade breaks must be resolved in 24 hours instead of 48. This created permanent operational pressure.

**Trade Break Resolution Workflow (Current):**
1. Receive alert: "Trade break on order ABC123"
2. SSH into log server
3. `grep ABC123 *.log` across multiple files
4. Manually piece together order lifecycle (NewOrder → Fills → Final state)
5. Copy-paste into Excel to see timeline
6. **30-45 minutes later:** Find the issue
7. Repeat 10-20 times per day

**Quantified Pain:**
- 40 minutes per trade break × 10 breaks/day = 6+ hours/day
- Fully-loaded analyst cost: $150K/year
- Value of time saved: $50-100K/year per analyst

### 7.2 The Proposed Solution

**Trade Break Workbench:**
1. Drag & drop FIX log files into app
2. Premade FIX parser reconstructs order lifecycles
3. Query: `SELECT * FROM fix_order_lifecycle WHERE cl_ord_id = 'ABC123'`
4. See full order history in seconds
5. **5 minutes instead of 45**

### 7.3 Why Trade Desk Is Attractive

| Factor | Detail |
|--------|--------|
| **Clear pain** | T+1 pressure is real and permanent |
| **Quantifiable ROI** | 6 hours/day saved = $50-100K/year |
| **Fast sales cycle** | Operations budget, not IT budget |
| **Budget authority** | Manager can approve tools <$25K without IT |
| **Accessible** | Can reach via LinkedIn, trading communities |

### 7.4 Why Trade Desk Is Problematic

| Factor | Detail |
|--------|--------|
| **Don't write parsers** | Core platform value (parser dev loop) is unused |
| **Need premade parser** | Casparian must ship FIX parser |
| **"Application" not "Platform"** | They want a product, not infrastructure |
| **Feedback will be domain-specific** | "Show reject reasons by venue" doesn't generalize |
| **Support burden** | You become FIX expert, handle edge cases |

### 7.5 The Parser Question

**Who writes the FIX parser?**

| Option | Implication |
|--------|-------------|
| **Casparian ships it** | You become FIX domain expert; parser is product, not user-written |
| **User writes it** | Trade Support Analysts can't write Python; need different persona |
| **Quant Dev at firm writes it** | Different buyer; longer sales cycle |
| **Consultant writes it** | Services revenue; not scalable |

**Reality:** For Trade Desk, Casparian must ship the premade FIX parser. Users will not write it themselves.

---

## 8. Validated Persona Research

### 8.1 Original Assumption

Trade Support Analysts use command-line tools (grep, awk, sed) for FIX log analysis, so a CLI product would work.

### 8.2 Research Finding (January 2026)

**FALSE.** Trade Support Analysts are **Excel/VBA users**, not command-line users.

| Source | Finding |
|--------|---------|
| [Goodman Masson](https://www.goodmanmasson.com/the-insights-hub/a-day-in-the-life-of-a-trade-support-analyst) | "Strong Excel skills are highly sought after, so if you have experience of Macros (VBA)..." |
| [Velvet Jobs](https://www.velvetjobs.com/job-descriptions/trade-support-analyst) | "Trade support analysts provide ad hoc analysis using SQL, Excel, VBA and internal utilities" |
| [Wall Street Oasis](https://www.wallstreetoasis.com/forum/trading/trading-support-analyst-excel-vba-requirement) | "VBA definitely helps... you are dealing with a lot of spreadsheets" |
| Job postings (multiple) | Excel/VBA required; Unix/Linux NOT mentioned |

### 8.3 Critical Distinction: Two Different Roles

| Role | Department | Tools | CLI Comfort | Our Target? |
|------|------------|-------|-------------|-------------|
| **Trade Support Analyst** | Operations | Excel, VBA, SQL, Bloomberg | LOW | Under consideration |
| **Application Support Analyst** | IT | Unix, shell, Python | HIGH | No (different pain) |
| **Quant Developer** | Technology | Python, FIX expertise | HIGH | Possible alternative |

### 8.4 Implication

If targeting Trade Support Analysts, must build a **GUI (Tauri)** not CLI. They expect Excel-like interfaces.

---

## 9. Competitive Landscape

### 9.1 FIX Log Analysis Specifically

| Product | What It Does | Price | Gap |
|---------|--------------|-------|-----|
| [QuantScope](https://quantscopeapp.com/) | FIX log viewer/visualizer | Free | No SQL output, no batch, no governance |
| [OnixS FIX Analyser](https://www.onixs.biz/fix-analyser.html) | Query builder | Commercial | Testing focus, not operations |
| [B2BITS FIXEye](https://www.b2bits.com/trading_solutions/fix_log_analyzers) | Enterprise log search | Enterprise | Monitoring focus, not break resolution |
| Manual (grep + Excel) | DIY | "Free" | 30-45 min per break |

**Key Finding:** There is NO enterprise "FIX log → SQL with governance" product. The market is:
- Free tools (QuantScope) - no batch, no governance
- Manual (grep + Excel) - slow, no governance
- Generic log tools (Splunk) - not FIX-aware

### 9.2 Trade Reconciliation (Adjacent but Different)

| Product | What It Does | Price |
|---------|--------------|-------|
| [Gresham](https://www.greshamtech.com/) | Trade reconciliation | $100K+ |
| [Broadridge](https://www.broadridge.com/) | Middle office operations | $100K+ |
| [Trintech](https://www.trintech.com/) | Financial close | Enterprise |

**These are NOT competitors.** They work on already-structured data (compare two systems). They don't parse raw FIX logs.

### 9.3 Parser Infrastructure (The Actual Category)

| Approach | What It Does | Gap |
|----------|--------------|-----|
| DIY Python scripts | Custom parsing | No governance, no backtest, no lineage |
| Apache Spark | Batch processing | Requires data engineering team, no governance |
| dbt | Transformation | Works on structured data, not raw files |
| Airbyte/Fivetran | ETL connectors | Standard formats only, not custom parsing |

**Key Finding:** There is no "parser development infrastructure" product. Teams either:
- Write DIY scripts (no governance)
- Build custom infrastructure (expensive)

---

## 10. The Strategic Fork

### 10.1 The Core Tension

```
┌─────────────────────────────────────────────────────────────────────┐
│                     CASPARIAN'S CORE VALUE                          │
│                                                                     │
│    User writes parser → Backtest → Fix → Backtest → Publish        │
│    Infrastructure: lineage, quarantine, versioning, governance      │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  │
            ┌─────────────────────┴─────────────────────┐
            │                                           │
            ▼                                           ▼
┌─────────────────────────┐               ┌─────────────────────────┐
│   PATH A: TRADE DESK    │               │  PATH B: PARSER WRITERS │
├─────────────────────────┤               ├─────────────────────────┤
│ • Don't write parsers   │               │ • Write parsers         │
│ • Use premade FIX parser│               │ • Use full platform     │
│ • Want an APPLICATION   │               │ • Want INFRASTRUCTURE   │
│ • Fast sales cycle      │               │ • Slower sales cycle    │
│ • High willingness pay  │               │ • Variable pricing      │
│ • UI feedback (domain)  │               │ • Platform feedback     │
└─────────────────────────┘               └─────────────────────────┘
            │                                           │
            │                                           │
            ▼                                           ▼
    Feedback: "Add FIX-specific          Feedback: "Backtest UI
     tag grouping, reject reason          needs diff view, lineage
     filtering, venue picker"             graph is confusing"
            │                                           │
            │                                           │
            ▼                                           ▼
    BUILDS: FIX Application              BUILDS: Parser Platform
    (narrow, vertical)                   (broad, horizontal)
```

### 10.2 The Feedback Problem

| Feedback Type | From Trade Desk | From Parser Writers |
|---------------|-----------------|---------------------|
| **UX (generalizes)** | "Table is slow" ✓ | "Table is slow" ✓ |
| **Domain (doesn't generalize)** | "FIX tag 150 should..." ✗ | N/A |
| **Platform (core value)** | Won't mention | "Backtest iteration is slow" ✓ |

**Trade Desk will never ask for:**
- Better backtest iteration UX
- Lineage visualization improvements
- Quarantine rule flexibility
- Parser versioning UI

**Because they don't write parsers.**

### 10.3 The Build Direction Problem

If you optimize for Trade Desk feedback:

```
TRADE DESK REQUESTS                    PLATFORM VALUE
────────────────────                   ──────────────────
"Group orders by venue"         vs     Backtest iteration speed
"Filter by reject reason"       vs     Lineage graph UI
"Show partial fill timeline"    vs     Quarantine management
"FIX tag dictionary picker"     vs     Parser versioning
```

**The risk:** Building for Trade Desk pulls you toward a FIX Application, not a Parser Platform.

---

## 11. Option A: Trade Desk as Beachhead

### 11.1 Strategy

Ship premade FIX parser to Trade Support Analysts. Use them for cash flow and initial UI feedback. Expand to parser-writing customers later.

### 11.2 Execution

1. Build Tauri GUI (Trade Support expects Excel-like interface)
2. Ship premade FIX parser (they won't write it)
3. Target Trade Support Analysts via LinkedIn outreach
4. Pricing: $300/user/month (operations budget, manager can approve)
5. Goal: $10K MRR in 3-6 months

### 11.3 Pros

| Pro | Detail |
|-----|--------|
| **Fast cash flow** | Validates willingness to pay |
| **Clear pain** | T+1 pressure, quantifiable ROI |
| **Accessible** | LinkedIn outreach, short sales cycle |
| **Low technical bar** | They're not picky about internals |
| **UI feedback** | They'll push for better UX |
| **Reference customers** | "Used by trading desks at X" |

### 11.4 Cons

| Con | Detail |
|-----|--------|
| **Don't use core value** | Parser dev loop is unused |
| **Feedback is domain-specific** | Won't improve platform |
| **Risk of becoming "FIX app"** | Gravitational pull toward vertical |
| **You become FIX expert** | Support burden for edge cases |
| **Premade parser is product** | Shifts value from platform to parser |

### 11.5 Mitigation Strategies

| Risk | Mitigation |
|------|------------|
| Becoming "FIX app" | Keep UI generic (table, filter, detail panel); no FIX-specific widgets |
| Domain-specific feedback | Say no to FIX-specific features; treat as "validation of demand" not "platform UX" |
| Not validating platform | Find separate design partners (parser writers) for platform feedback |

---

## 12. Option B: Parser Writers as Primary Target

### 12.1 Strategy

Target technical teams who write custom parsers. They use the full platform value (backtest, lineage, quarantine).

### 12.2 Candidate Personas

| Persona | Formats | Accessibility | Parser Dev Loop Value |
|---------|---------|---------------|----------------------|
| **Data consultants** | Client-specific | HIGH | VERY HIGH |
| **Small FinTech teams** | Custom formats | MEDIUM | HIGH |
| **DevOps/SRE** | Custom app logs | HIGH | MEDIUM |
| **Satellite/space teams** | Binary telemetry | MEDIUM | VERY HIGH |
| **Healthcare integration** | HL7 variations | LOW (slow) | HIGH |
| **Defense contractors** | CoT, NITF | LOW (slow) | VERY HIGH |

### 12.3 Why Data Consultants Are Interesting

| Factor | Detail |
|--------|--------|
| **Write parsers constantly** | It's literally their job |
| **Use full platform value** | Backtest, lineage, versioning |
| **Accessible** | LinkedIn, data communities, conferences |
| **Fast sales cycle** | Individual or small firm decision |
| **Would give platform feedback** | "Backtest needs X", "Lineage UI is confusing" |

### 12.4 Pros

| Pro | Detail |
|-----|--------|
| **Validates core value** | They use parser dev loop |
| **Platform feedback** | Improves backtest, lineage, quarantine |
| **Horizontal product** | Features generalize across formats |
| **No domain lock-in** | Not tied to FIX or any vertical |

### 12.5 Cons

| Con | Detail |
|-----|--------|
| **Slower to revenue** | Longer to find and close |
| **Lower ACV (initially)** | Consultants may pay less than trading desks |
| **Harder to find** | Less obvious outreach channel |
| **More technical users** | Higher expectations for internals |

---

## 13. Option C: Hybrid Approach

### 13.1 Strategy

Pursue Trade Desk for cash flow, but maintain separate "design partners" who are parser writers for platform feedback.

### 13.2 Execution

| Track | Target | Purpose | Feedback You Get |
|-------|--------|---------|------------------|
| **Cash Track** | Trade Desk | Revenue, basic UX | Domain feedback (ignore), UX feedback (use) |
| **Platform Track** | Data consultants, small teams | Design partnership | Platform feedback (prioritize) |

### 13.3 Pros

| Pro | Detail |
|-----|--------|
| **Cash flow from Trade Desk** | Validates willingness to pay |
| **Platform feedback from parser writers** | Improves core value |
| **Balanced roadmap** | Don't over-index on one persona |

### 13.4 Cons

| Con | Detail |
|-----|--------|
| **Split attention** | Serving two masters |
| **Conflicting feedback** | Trade Desk wants X, parser writers want Y |
| **Resource strain** | Small team, two audiences |
| **Message confusion** | What is Casparian? App or Platform? |

---

## 14. Key Questions for Evaluation

### 14.1 Product Identity

1. **Is Casparian a platform or an application?**
   - Platform: Infrastructure for parser development
   - Application: Vertical solution for specific format (e.g., FIX logs)

2. **What is the core value?**
   - The parser dev loop (backtest, lineage, quarantine)?
   - Or the output (dark data → SQL)?

3. **Who is the user?**
   - Someone who writes parsers?
   - Or someone who consumes pre-parsed data?

### 14.2 Go-to-Market

4. **Is faster cash flow worth the feedback mismatch?**
   - Trade Desk: Fast cash, domain feedback
   - Parser writers: Slower cash, platform feedback

5. **Can you resist the gravitational pull of paying customers?**
   - Trade Desk will ask for FIX-specific features
   - Can you say "no" when they're paying?

6. **Is the hybrid approach feasible for a solo founder / small team?**
   - Two audiences = two roadmaps = split focus

### 14.3 Competitive Dynamics

7. **Does "no competition" in FIX log → SQL mean opportunity or lack of demand?**
   - Opportunity: Underserved market
   - Lack of demand: Maybe people don't need it?

8. **If you build the FIX app and it succeeds, does that validate the platform?**
   - Maybe: Proves "dark data → SQL" has value
   - Maybe not: Doesn't prove parser dev loop has value

### 14.4 Long-Term Vision

9. **What do you want to build?**
   - A company that sells FIX log tools to trading desks?
   - A company that sells parser infrastructure to technical teams?

10. **What does success look like in 3 years?**
    - 100 trading desks using Trade Break Workbench?
    - 1,000 teams using Casparian to parse their custom formats?

---

## 15. Founder's Current Thinking

### 15.1 Core Thesis

> "Unstructured data requires custom parsing. Therefore, the core business involves enabling teams to write and maintain custom parsers reliably."

### 15.2 Pragmatic Consideration

> "I can ship the product with a premade parser to Trade Desk to cash flow ASAP and get UI feedback ASAP in a lower-pressure situation than a technical team."

### 15.3 Concern

> "I'm worried I'll be getting feedback from users to implement FIX-specific UI things and filtering, where the core value is tightening up the infra and allowing teams to write their own parsers."

### 15.4 Current Lean

Founder is leaning toward **Trade Desk as beachhead** but worried about:
- Feedback quality (domain vs. platform)
- Risk of becoming "FIX app" instead of "parser platform"
- Not validating the core differentiator

---

## 16. Strategic Evaluation (Completed January 2026)

### 16.1 Core Diagnosis: The "Two-Product" Trap

The Trade Desk option is dangerous because it creates **two different products**:

| Product | Target | Core Activity | Validates |
|---------|--------|---------------|-----------|
| **Casparian Platform** | Parser writers | Writing Python parsers | Infrastructure value |
| **Trade Break Workbench** | Excel/VBA users | Consuming pre-parsed data | Output value |

**Critical Insight:** If we pursue Trade Desk, we become the parser writer. The customer buys the result, not the infrastructure. This turns Casparian into a **services company with a software interface**.

### 16.2 Answers to Key Questions

#### Q1: Is Trade Desk a valid beachhead, or a distraction?

**VERDICT: It is a distraction.**

Trade Desk validates that *structured data* is valuable, but does NOT validate that *our infrastructure* is the best way to get it. It validates the "What" (FIX → SQL), not the "How" (parser dev loop).

**Recommendation:** Only pursue Trade Desk if willing to pivot the entire company to become "The FIX Log Analysis Company" (valid business, but different from platform vision).

#### Q2: Is the hybrid approach realistic for a small team?

**VERDICT: No.**

Building a Tauri GUI for Excel users AND a CLI/Rust platform for Python devs is too much surface area for a solo founder/small team. Both will be done poorly.

**Constraint Check:** Small team + limited runway = cannot split engineering focus.

#### Q3: Are there better beachhead candidates?

**VERDICT: Yes. See Section 17 - "Hidden Gem" Segments.**

Four segments identified that have:
- Technical users (write Python)
- High budget (billable hours or compliance-driven)
- High urgency (deadlines, incidents)
- Mandatory audit trails (lineage is legally required)

#### Q4: How do others handle this tension?

Successful infrastructure companies (dbt, HashiCorp) target the **practitioner** (person doing the typing), not the **manager** (person needing the report).

**Example:** dbt didn't sell "better dashboards" to CFOs. They sold "better SQL workflow" to Analytics Engineers.

**For Casparian:** Sell "better Python workflow" to Data Engineers/Consultants, not "faster reports" to Operations.

#### Q5: What's the risk of the "FIX app" path?

**VERDICT: Vertical Lock-in.**

Once you add "FIX-specific tag grouping," the UI becomes confusing for Satellite Telemetry users. The codebase clutters with domain logic. In 2 years, you own a job (Trade Support Vendor) rather than a scalable platform.

---

## 17. Hidden Gem Segments (Discovered January 2026)

Four customer segments that fit the **Parser Writer** persona (technical, Python-literate) with the **Urgency/Budget** of Trade Desk:

### 17.1 The "Hidden Gem" Matrix

| Segment | Can Write Python? | Budget | Urgency | Audit Trail Required? | Verdict |
|---------|-------------------|--------|---------|----------------------|---------|
| **Trade Desk** | NO | High | High | No | **TRAP (Service)** |
| **Data Consultant** | YES | Medium | Medium | No | **Good Base** |
| **eDiscovery** | **YES** | **VERY HIGH** | **HIGH** | **YES (Legal)** | **GOLD MINE** |
| **DFIR** | **YES** | **HIGH** | **HIGH** | **YES (Chain of Custody)** | **GOLD MINE** |
| **Industrial OT** | YES | High | Medium | No | Good |
| **Bioinformatics** | YES | Low | Low | YES (Reproducibility) | Too Slow |

### 17.2 Segment 1: eDiscovery (Evidence Engineering)

**Persona:** Litigation Support Analyst / eDiscovery Technologist

**The Pain:** Law firm receives 2TB hard drive from client in lawsuit. Contains 10 years of messy data: old PST emails, Slack JSON exports, proprietary chat logs, weird accounting system dumps.

| Factor | Detail |
|--------|--------|
| **Why they fit** | Must parse data to load into review platforms (Relativity) |
| **Parser Dev Loop Value** | If parser skips a row (evidence), they can be sued for spoliation |
| **Backtest Value** | Need to prove to court that parsing was accurate |
| **Lineage Value** | **LEGALLY REQUIRED** - Chain of custody |
| **Urgency** | Court deadlines are non-negotiable |
| **Willingness to Pay** | Extremely high - law firms bill this as "Technical Time" |
| **Local-First** | Often work with privileged/confidential data |

**The Pitch:**
> "Stop writing throwaway scripts for data dumps. Build a defensible parsing pipeline with a full audit trail for the court."

### 17.3 Segment 2: DFIR (Incident Response)

**Persona:** Digital Forensics & Incident Response Consultant

**The Pain:** Client is hacked. Responder gets disk images full of system logs (Windows Event Logs, Shimcache, Amcache, Firewall logs).

| Factor | Detail |
|--------|--------|
| **Why they fit** | Every attacker uses different tools; standard parsers often fail |
| **Parser Dev Loop Value** | Need to iterate fast ("Fix parser → re-run → find attacker") |
| **Backtest Value** | Must handle corrupted/partial logs without crashing |
| **Lineage Value** | **LEGALLY REQUIRED** - Chain of custody for prosecution |
| **Local-First** | Often work on air-gapped evidence machines; cloud is non-starter |
| **Urgency** | Active breach = hours matter |
| **Willingness to Pay** | High - incident response bills $300-500/hour |

**The Pitch:**
> "Your incident response scripts are fragile. Casparian lets you write custom artifact parsers that handle dirty logs without crashing your timeline."

### 17.4 Segment 3: Industrial OT (IIoT Edge)

**Persona:** OT (Operational Technology) Integration Engineer

**The Pain:** Manufacturing plants have machines from 1995 dumping binary data (PLCs, SCADA) to flat files. Need to get into modern MQTT/Kafka stream.

| Factor | Detail |
|--------|--------|
| **Why they fit** | Binary telemetry is their life; highly technical but lack infrastructure |
| **Parser Dev Loop Value** | Formats change per machine/vendor |
| **Quarantine Value** | **CRITICAL** - bad sensor value can't crash whole pipeline |
| **Local-First** | Can't send factory data to cloud (latency, security) |
| **Current Alternative** | Jam into AWS SiteWise (expensive, laggy) or fragile Pi scripts |

### 17.5 Segment 4: Bioinformatics (Lower Priority)

**Persona:** Research Data Engineer / Bioinformatician

**The Pain:** Sequencing data (FASTQ) is standard, but metadata (lab notebooks, machine logs, sample sheets) is disaster of Excel/CSV.

| Factor | Detail |
|--------|--------|
| **Why they fit** | Highly technical (Python/R fluent) |
| **Lineage Value** | **HOLY GRAIL** - Reproducibility crisis; papers retracted without trace |
| **Budget** | Lower (academic) |
| **Urgency** | Lower |

**Verdict:** Good fit for platform value, but slower sales cycle and lower budget.

---

## 18. Why eDiscovery & DFIR Are Platform Route Beachheads

### 18.1 They Are "Billable" Engineers

Unlike corporate data engineers (cost center), these people **bill their time**. If Casparian makes them 20% faster, they make 20% more margin.

### 18.2 Audit Trails Are Mandatory

Lineage isn't "nice to have" - it's a **legal requirement**:
- **eDiscovery:** Chain of custody for evidence
- **DFIR:** Chain of custody for prosecution

### 18.3 The Files Are Terrible

They deal with the **worst "dark data" imaginable**:
- Deleted files, corrupted logs, ancient formats
- Proprietary exports, partial data, encoding issues

They NEED the Parser Dev Loop (Quarantine/Backtest) more than anyone.

### 18.4 The Market Is Underserved

| Current Alternative | Problem |
|---------------------|---------|
| Throwaway Python scripts | No governance, no audit trail |
| Generic forensic tools | Don't handle custom formats |
| Manual review | $500/hour analyst time |

---

## 19. Recommended Strategy: Consultant-First Approach

### 19.1 Strategic Pivot

Instead of selling the **shovel** (Casparian) to the **gold miner** (Trade Desk), sell the shovel to the **shovel operator** (Consultant) who is hired by the gold miner.

### 19.2 Execution Plan

**Phase 1: Kill the General-Purpose GUI (For Now)**

Do NOT build "Trade Break Workbench" GUI for Excel users. It consumes too much dev time and validates the wrong thing.

**Phase 2: Focus on CLI/TUI + Premade Parsers**

- Package FIX parser (and others) as **examples/starter kits**
- Do NOT sell to Trade Support Analysts directly
- Sell to **FinTech Consultants/Integrators** and **eDiscovery Technologists**

**Phase 3: The Pitch to Consultants**

> "You have clients with T+1 issues / discovery deadlines / breach investigations. Use Casparian to deploy a fix in days, not months. You write the last mile of logic; we handle the governance/lineage."

### 19.3 Revenue Model Options

| Model | Description |
|-------|-------------|
| **Platform Seat** | Consultant pays monthly to use the tool |
| **Per-Project** | Consultant pays per client engagement |
| **Revenue Share** | Consultant passes cost to client + markup |

### 19.4 Target Outreach

| Segment | Where to Find | Pitch Angle |
|---------|---------------|-------------|
| **eDiscovery** | ACEDS community, Relativity forums, LinkedIn | "Defensible parsing with audit trail" |
| **DFIR** | SANS community, DFIR Discord, LinkedIn | "Custom artifact parsers that don't crash" |
| **Data Consultants** | LinkedIn, dbt Slack, data communities | "Stop writing throwaway scripts" |

---

## 20. If Survival Mode Requires Trade Desk Cash

If you MUST take Trade Desk money to survive:

### 20.1 Treat as Non-Recurring Engineering (NRE)

- Charge high upfront setup fee ($15K+) to "configure the environment"
- Use cash to fund the Platform
- **IGNORE their feature requests** regarding UI

### 20.2 Don't Replace Excel - Feed It

- Do NOT try to build Excel-killer GUI
- Make output a live-updating Parquet/CSV that Excel reads
- Let them use their familiar tool
- Avoid building complex domain-specific UI

### 20.3 Minimal Viable Delivery

```
Trade Desk gets:
├── Premade FIX parser (you write it)
├── CLI to run parser
├── CSV/Parquet output
└── Excel reads the output

Trade Desk does NOT get:
├── Custom Tauri GUI
├── FIX-specific UI widgets
├── Venue grouping, tag pickers, etc.
└── Ongoing feature development
```

---

## 21. Final Decision (January 2026)

### 21.1 Primary Target: eDiscovery + DFIR

**Rationale:**
- Technical users who write Python
- High budget (billable hours)
- High urgency (deadlines, incidents)
- Audit trails legally required (validates lineage)
- Uses full platform value (backtest, quarantine, lineage)

### 21.2 Secondary Target: Data Consultants

**Rationale:**
- Write parsers constantly
- Fast sales cycle
- Platform feedback
- Bridge to enterprise clients

### 21.3 Deprioritized: Trade Desk

**Rationale:**
- Don't write parsers (core value unused)
- Feedback doesn't improve platform
- Risk of "Service Trap"
- May pursue later with consultant-delivered solution

---

## 22. Next Steps

1. **Research eDiscovery/DFIR communities** - ACEDS, SANS, Relativity forums
2. **Develop outreach messaging** - "Defensible parsing pipeline"
3. **Identify 5-10 potential design partners** - Litigation support analysts, DFIR consultants
4. **Build premade parsers as examples** - PST, Windows Event Logs, common forensic artifacts
5. **Defer Tauri GUI** - Focus on CLI/TUI for technical users

---

## Appendix A: Updated Target Segment Priorities

| Priority | Segment | Rationale |
|----------|---------|-----------|
| **P0** | eDiscovery (Litigation Support) | High budget, mandatory audit trail, writes Python |
| **P0** | DFIR (Incident Response) | High urgency, mandatory chain of custody, writes Python |
| **P1** | Data Consultants | Fast sales cycle, writes parsers, platform feedback |
| **P2** | Industrial OT | Good fit, medium urgency |
| **P3** | Trade Desk (via consultants) | Only if consultant-delivered, not direct |
| **Deprioritized** | Bioinformatics | Lower budget, slower cycle |

---

## Appendix B: Business Details

**Company:** MindMadeSoftware LLC (California)
**Product:** Casparian Flow
**Stage:** Pre-revenue, pre-launch
**Target Launch:** Mid-February 2026

**Pricing (Under Consideration):**

| Tier | Price | Target |
|------|-------|--------|
| Pro | $75-100/user/month | Individual users |
| Team | $300-400/month | Small teams |
| Enterprise | Custom | Large organizations |

---

## Appendix B: Document References

- `STRATEGY.md` - Master business strategy
- `strategies/finance.md` - Financial services vertical strategy
- `docs/decisions/ADR-020-tauri-gui.md` - Decision to build GUI
- `specs/tauri_ui.md` - Tauri UI design specification
- `specs/tauri_mvp.md` - MVP feature set for Tauri

---

## Appendix D: Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-20 | 1.0 | Initial document for strategic evaluation |
| 2026-01-20 | 2.0 | Added external evaluation: "Hidden Gem" segments (eDiscovery, DFIR), Service Trap analysis, Consultant-First strategy recommendation, final decision on target segments |

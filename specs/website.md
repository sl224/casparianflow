# Casparian Flow Website Spec

> **Version:** 3.0
> **Date:** January 19, 2026
> **Status:** Multi-Vertical Structure with Phased Rollout

## Overview

Multi-page marketing site for Casparian Flow - a local-first data platform that transforms industry-specific file formats into queryable SQL datasets.

**Site Structure:**
- `/` - Homepage (vertical selector)
- `/finance` - Trade Break Workbench (LIVE)
- `/healthcare` - HL7 Archive Analytics (Coming Soon - waitlist)
- `/defense` - Air-Gapped Processing (Coming Soon - waitlist)
- `/legal` - eDiscovery Processing (Coming Soon - waitlist)

**Launch Strategy:** DFIR first, expand based on waitlist demand and paying customers. See STRATEGY.md for vertical priority stack.

**Platform:** Carrd.co Pro (supports multiple pages)

**Product Form:** CLI + TUI (Rust binary)

---

## Site Architecture

```
casparian.dev/
│
├── /                    ← Vertical selector (universal value prop)
│
├── /finance             ← LIVE: Full Trade Break Workbench page
│
├── /healthcare          ← COMING SOON: Waitlist + preview
│
├── /defense             ← COMING SOON: Waitlist + preview
│
└── /legal               ← COMING SOON: Waitlist + preview
```

**Routing Logic:**
- Direct traffic (ads, outreach) → vertical-specific pages
- Organic/brand traffic → homepage → self-select vertical
- "Coming Soon" pages capture email for waitlist

---

## Page 1: Homepage (`/`)

### Purpose
Universal entry point. Communicate core value prop, let visitors self-select into their vertical.

### Core Value Proposition

**Headline:** "Query your dark data in SQL. Locally."

**Subhead:** "Built-in parsers and Python plugins. Casparian handles lineage, job management, and backfills. Data never leaves your machine."

### Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Query your dark data in SQL.                                   │
│  Locally. With full lineage.                                    │
│                                                                 │
│  Built-in parsers and Python plugins.                           │
│  Casparian handles jobs and backfills.                          │
│  Data never leaves your machine.                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  What are you working with?                                     │
│                                                                 │
│  ┌───────────────────────┐  ┌───────────────────────┐           │
│  │                       │  │                       │           │
│  │  FIX LOGS             │  │  HL7 MESSAGES         │           │
│  │                       │  │                       │           │
│  │  Trade Support        │  │  Healthcare IT        │           │
│  │  Middle Office        │  │  Integration Teams    │           │
│  │                       │  │                       │           │
│  │  Debug trade breaks   │  │  Query 5 years of     │           │
│  │  in 5 min, not 45.    │  │  archives with SQL.   │           │
│  │                       │  │                       │           │
│  │  ┌─────────────────┐  │  │  ┌─────────────────┐  │           │
│  │  │  AVAILABLE NOW  │  │  │  │  COMING SOON    │  │           │
│  │  │  [Learn More →] │  │  │  │  [Join Waitlist]│  │           │
│  │  └─────────────────┘  │  │  └─────────────────┘  │           │
│  │                       │  │                       │           │
│  └───────────────────────┘  └───────────────────────┘           │
│                                                                 │
│  ┌───────────────────────┐  ┌───────────────────────┐           │
│  │                       │  │                       │           │
│  │  PST / EMAIL          │  │  CoT / NITF           │           │
│  │  ARCHIVES             │  │  TACTICAL DATA        │           │
│  │                       │  │                       │           │
│  │  Legal / eDiscovery   │  │  Defense / Intel      │           │
│  │  Litigation Support   │  │  Air-Gapped Ops       │           │
│  │                       │  │                       │           │
│  │  Process PSTs         │  │  SQL for tactical     │           │
│  │  in-house. $5-15K     │  │  data on your laptop. │           │
│  │  saved per matter.    │  │  Works offline.       │           │
│  │                       │  │                       │           │
│  │  ┌─────────────────┐  │  │  ┌─────────────────┐  │           │
│  │  │  COMING SOON    │  │  │  │  COMING SOON    │  │           │
│  │  │  [Join Waitlist]│  │  │  │  [Join Waitlist]│  │           │
│  │  └─────────────────┘  │  │  └─────────────────┘  │           │
│  │                       │  │                       │           │
│  └───────────────────────┘  └───────────────────────┘           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Why Casparian?                                                 │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │                 │  │                 │  │                 │  │
│  │  LOCAL-FIRST    │  │  LINEAGE        │  │  SQL OUTPUT     │  │
│  │                 │  │                 │  │                 │  │
│  │  Data never     │  │  Every row      │  │  Query with     │  │
│  │  leaves your    │  │  tracks source  │  │  DuckDB, any    │  │
│  │  machine.       │  │  file + plugin  │  │  SQL tool.      │  │
│  │  Air-gapped.    │  │  version.       │  │  Export to      │  │
│  │  Offline-first. │  │  Audit-ready.   │  │  Parquet.       │  │
│  │                 │  │                 │  │                 │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │                 │  │                 │  │                 │  │
│  │  PYTHON         │  │  JOB            │  │  BACKFILL       │  │
│  │  PLUGINS        │  │  MANAGEMENT     │  │  REPLAY         │  │
│  │                 │  │                 │  │                 │  │
│  │  Bring your     │  │  Queue jobs,    │  │  Re-run by      │  │
│  │  own parsers    │  │  monitor runs,  │  │  tag or time    │  │
│  │  or transforms. │  │  retry failures. │  │  range.         │  │
│  │  Runs locally,  │  │  CLI + TUI      │  │  Lineage stays  │  │
│  │  no cloud.      │  │  status views.  │  │  intact.        │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  How it works                                                   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian scan /data/logs --tag my_format            │    │
│  │  ✓ Tagged 50,000 files                                  │    │
│  │                                                         │    │
│  │  $ casparian pipeline run my_pipeline                   │    │
│  │  ✓ Queued jobs + lineage                                │    │
│  │                                                         │    │
│  │  $ casparian jobs --topic my_format                     │    │
│  │  ✓ 49,812 complete, 188 running                         │    │
│  │                                                         │    │
│  │  $ casparian backfill my_parser --execute               │    │
│  │  ✓ Reprocessed 3,820 files                              │    │
│  │                                                         │    │
│  │  $ duckdb -c "SELECT * FROM my_table WHERE ..."         │    │
│  │  → Query results in seconds                             │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  1. Scan: Point at your files                                   │
│  2. Run: Built-in parser or Python plugin (jobs + lineage)      │
│  3. Backfill: Re-run by tag or time window                      │
│  4. Query: SQL on your data, locally                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  support@casparian.dev                      │
│                                                                 │
│  Your data stays yours. Telemetry is opt-in and anonymous.      │
│  No file contents ever leave your machine. Works fully offline. │
│                                                                 │
│  © 2026                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Homepage SEO

- **Title:** "Casparian Flow - SQL for Industry File Formats"
- **Description:** "Transform FIX logs, HL7 messages, PST archives, and tactical data into queryable SQL. Local-first. Python plugins. Full lineage. Data never leaves your machine."

---

## Page 2: Finance (`/finance`)

### Purpose
Primary conversion page for Trade Support / Middle Office buyers. Full product page with pricing.

### Target Persona

| Attribute | Detail |
|-----------|--------|
| **Job Title** | Trade Support Analyst, FIX Connectivity Analyst, Middle Office Analyst |
| **Company** | Broker-dealer, prop trading firm, hedge fund (50-500 employees) |
| **Technical Skills** | SQL, grep, Excel; NOT Python experts |
| **Pain** | T+1 settlement pressure, 30-45 min per trade break |
| **Budget** | Operations budget; can approve tools that reduce risk |
| **Salary** | $73K-$146K |

### Core Value Proposition

**Headline:** "Debug trade breaks in 5 minutes, not 45."

**Subhead:** "FIX log analysis for Trade Support. Local-first. Full audit trail. T+1 ready."

### Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [← All Verticals]                          Casparian Flow      │
│                                                                 │
│  Debug trade breaks in 5 minutes, not 45.                       │
│                                                                 │
│  FIX log analysis for Trade Support. Local-first.               │
│  Full audit trail. T+1 ready.                                   │
│                                                                 │
│  [Download Free]          [Watch Demo →]                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian scan /var/log/fix --tag fix_logs           │    │
│  │  ✓ Tagged 847,291 FIX messages                          │    │
│  │                                                         │    │
│  │  $ casparian process --tag fix_logs                     │    │
│  │  ✓ Output: fix_order_lifecycle (12,847 orders)          │    │
│  │                                                         │    │
│  │  $ duckdb -c "SELECT * FROM fix_order_lifecycle         │    │
│  │     WHERE cl_ord_id = 'ORD12345'"                       │    │
│  │                                                         │    │
│  │  cl_ord_id  | symbol | side | status  | lifecycle_ms    │    │
│  │  ORD12345   | AAPL   | BUY  | FILLED  | 847             │    │
│  │                                                         │    │
│  │  → Full lifecycle: New → PartialFill → Filled           │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  T+1 settlement is live. Your team is still grepping logs.      │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │  30-45 min      │  │  No audit       │  │  Knowledge      │  │
│  │  per break      │  │  trail          │  │  walks out      │  │
│  │                 │  │                 │  │                 │  │
│  │  Grep logs,     │  │  Compliance     │  │  When the one   │  │
│  │  paste into     │  │  asks "show     │  │  person who     │  │
│  │  Excel, repeat  │  │  your work"     │  │  knows leaves   │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
│  10 breaks/day × 40 min = 6+ hours lost. Every day.             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  How it works                                                   │
│                                                                 │
│  1. Scan             2. Parse              3. Query             │
│                                                                 │
│  Point at your       Premade FIX parser    SQL query by         │
│  FIX log             builds the            ClOrdID, symbol,     │
│  directory.          `fix_order_lifecycle` or time range.       │
│                      table automatically.  Full audit trail.    │
│                                                                 │
│  Works with FIX 4.2, 4.4, 5.0. Custom tags supported.           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Built for Trade Support                                        │
│                                                                 │
│  ✓ Trade Lifecycle Reconstruction                               │
│    Full order history in one table: New → Fills → Final State   │
│                                                                 │
│  ✓ Local-First Execution                                        │
│    Data never leaves your machine. Works air-gapped.            │
│                                                                 │
│  ✓ Compliance-Ready Audit Trail                                 │
│    Every query traced. Schema contracts prevent silent drift.   │
│                                                                 │
│  ✓ Multi-Venue Normalization                                    │
│    Consistent schema across execution venues.                   │
│                                                                 │
│  ✓ Custom Tag Support                                           │
│    Handle proprietary FIX extensions (tag 5000+).               │
│                                                                 │
│  ✓ SQL/Parquet Output                                           │
│    Query with DuckDB, load into your existing tools.            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  One table. Complete trade lifecycle.                           │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Column              │ Description                      │    │
│  │  ────────────────────┼──────────────────────────────────│    │
│  │  cl_ord_id           │ Client order ID (lookup key)     │    │
│  │  symbol              │ Instrument                       │    │
│  │  side                │ BUY / SELL                       │    │
│  │  order_qty           │ Original quantity                │    │
│  │  cum_qty             │ Filled quantity                  │    │
│  │  leaves_qty          │ Remaining                        │    │
│  │  avg_px              │ Average fill price               │    │
│  │  order_status        │ Final status                     │    │
│  │  first_seen          │ First message timestamp          │    │
│  │  last_update         │ Last message timestamp           │    │
│  │  lifecycle_ms        │ Total duration                   │    │
│  │  venue               │ Execution venue                  │    │
│  │  reject_reason       │ If rejected, why                 │    │
│  │  lifecycle_json      │ Full message history             │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  Query a single table to see exactly what happened.             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [DEMO VIDEO EMBED - 60 seconds]                                │
│                                                                 │
│  "T+1 is live. Here's a break resolved in 60 seconds."          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Calculate your savings                                         │
│                                                                 │
│  Trade breaks per day:  [  10  ]                                │
│  Minutes per break:     [  40  ]                                │
│  Analyst hourly rate:   [ $75  ]                                │
│                                                                 │
│  ─────────────────────────────────────────────────────────────  │
│                                                                 │
│  Current cost:     $50,000/year                                 │
│  With Casparian:   $12,500/year                                 │
│  You save:         $37,500/year                                 │
│                                                                 │
│  [Start Free Trial →]                                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Pricing                                                        │
│                                                                 │
│  Free              Analyst           Team            Trading    │
│  $0                $300/user/mo      $2,000/mo       Desk       │
│                    $3,000/user/yr    $20,000/yr      $6,000/mo  │
│                                                                 │
│  Evaluate the      Single analyst    Small team      Full desk  │
│  workflow          workflow          (up to 5)       deployment │
│                                                                 │
│  • EDGAR parser    • FIX parser      • All Analyst   • All Team │
│  • 5 files/day     • Unlimited       • 5 users       • Unlimited│
│  • Community         files           • Multi-venue     users    │
│    support         • Email support   • Priority      • Custom   │
│                    • Single venue      support         tags     │
│                                      • 4hr SLA       • SSO/SAML │
│                                                      • Dedicated│
│                                                        success  │
│                                                                 │
│  [Download]        [Start Trial]     [Start Trial]   [Contact]  │
│                                                                 │
│  Enterprise: Multi-desk deployments, custom SLAs → Contact us   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  "We went from 45-minute investigations to 5-minute queries.    │
│   Our compliance team loves the audit trail."                   │
│                                                                 │
│  — Trade Support Lead, [Prop Trading Firm]                      │
│                                                                 │
│  (Placeholder until pilot testimonials available)               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Frequently Asked Questions                                     │
│                                                                 │
│  ▸ Does my data leave my machine?                               │
│    No. Casparian runs entirely locally. Data never touches      │
│    any external server. Works fully air-gapped.                 │
│                                                                 │
│  ▸ What FIX versions do you support?                            │
│    FIX 4.2, 4.4, and 5.0. Custom tags (5000+) are supported.    │
│                                                                 │
│  ▸ How does licensing work?                                     │
│    Purchase online, receive license key via email, activate     │
│    with `casparian activate <key>`. Validates once, then        │
│    works offline.                                               │
│                                                                 │
│  ▸ Can I query the data with my existing tools?                 │
│    Yes. Output is Parquet/DuckDB. Works with any SQL tool,      │
│    Python, Excel, or your existing analytics stack.             │
│                                                                 │
│  ▸ What about compliance/audit requirements?                    │
│    Full lineage tracking: source file, parser version,          │
│    processing timestamp on every row. Schema contracts          │
│    prevent silent data drift.                                   │
│                                                                 │
│  ▸ Do I need Python skills?                                     │
│    No. Built-in parsers work out of the box. Python is only     │
│    needed if you want to build your own plugins.                │
│                                                                 │
│  ▸ How do backfills work?                                       │
│    Re-run by tag or time window with a single command.          │
│    Lineage keeps every run auditable.                           │
│                                                                 │
│  ▸ We already have Databricks. Why this?                        │
│    Different team, different access. Trade Support typically    │
│    doesn't have Databricks credentials. Casparian runs on       │
│    YOUR log server, accessible to YOUR team.                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  support@casparian.dev                      │
│                                                                 │
│  Your data stays yours. Telemetry is opt-in and anonymous.      │
│  No file contents ever leave your machine. Works fully offline. │
│                                                                 │
│  © 2026                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Finance Page SEO

- **Title:** "Casparian Flow - FIX Log Analysis for Trade Support"
- **Description:** "Debug trade breaks in 5 minutes, not 45. Local-first FIX log analysis with full audit trail. SQL for your FIX logs. T+1 ready."
- **Keywords:** FIX log analysis, FIX log parser, trade break resolution, FIX protocol tools, T+1 settlement tools, trade support software

---

## Page 3: Healthcare (`/healthcare`)

### Purpose
Capture interest from Healthcare IT. Waitlist + value preview.

### Target Persona

| Attribute | Detail |
|-----------|--------|
| **Job Title** | HL7 Interface Analyst, Integration Analyst, Mirth Administrator |
| **Company** | Hospital, health system, HIE |
| **Pain** | Archive analysis gap - Mirth routes but doesn't analyze |
| **Timing** | Mirth went commercial March 2025 - want more value from HL7 data |

### Core Value Proposition

**Headline:** "Query 5 years of HL7 archives with SQL."

**Subhead:** "Analytics that Mirth can't do. Local-first. HIPAA-friendly."

### Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [← All Verticals]                          Casparian Flow      │
│                                                                 │
│  Query 5 years of HL7 archives with SQL.                        │
│                                                                 │
│  Analytics that Mirth can't do. Local-first. HIPAA-friendly.    │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                                                         │    │
│  │              COMING Q2 2026                             │    │
│  │                                                         │    │
│  │  We're launching vertical-by-vertical.                  │    │
│  │  Healthcare is next.                                    │    │
│  │                                                         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Join the waitlist for early access                             │
│                                                                 │
│  [Email address                                            ]    │
│  [Organization                                             ]    │
│  [What's your biggest HL7 pain point? (optional)           ]    │
│                                                                 │
│  [Get Early Access →]                                           │
│                                                                 │
│  We'll notify you when Healthcare is live + offer early         │
│  access pricing.                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  What you'll get                                                │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │                 │  │                 │  │                 │  │
│  │  HL7 v2.x       │  │  SQL QUERY      │  │  LOCAL-FIRST    │  │
│  │  PARSER         │  │                 │  │                 │  │
│  │                 │  │  Query ADT,     │  │  Data stays     │  │
│  │  ADT, ORU, ORM  │  │  ORU, ORM by    │  │  on your        │  │
│  │  out of the     │  │  patient, date, │  │  server.        │  │
│  │  box.           │  │  facility.      │  │  HIPAA-         │  │
│  │                 │  │                 │  │  friendly.      │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  The archive analysis gap                                       │
│                                                                 │
│  Mirth Connect routes messages in real-time. But what about     │
│  the archives?                                                  │
│                                                                 │
│  • "How many ADT messages did we process last month?"           │
│  • "Which sending facility had the most errors?"                │
│  • "Show me all messages for patient X across 5 years"          │
│                                                                 │
│  Today: Export from Mirth → manual analysis → hours of work     │
│  With Casparian: SQL query → seconds                            │
│                                                                 │
│  We're complementary to Mirth. We analyze what Mirth archives.  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Example workflow                                               │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian scan /mirth_archives/ADT/ --tag hl7_adt    │    │
│  │  ✓ Tagged 2.4M HL7 messages                             │    │
│  │                                                         │    │
│  │  $ casparian process --tag hl7_adt                      │    │
│  │  ✓ Output: hl7_messages, hl7_segments                   │    │
│  │                                                         │    │
│  │  $ duckdb -c "                                          │    │
│  │      SELECT sending_facility, COUNT(*) as msgs,         │    │
│  │             SUM(CASE WHEN ack='AE' THEN 1 END) as errs  │    │
│  │      FROM hl7_messages                                  │    │
│  │      WHERE msg_type = 'ADT'                             │    │
│  │      GROUP BY sending_facility                          │    │
│  │      ORDER BY errs DESC"                                │    │
│  │                                                         │    │
│  │  sending_facility | msgs    | errs                      │    │
│  │  LAB_WEST         | 847291  | 1247                      │    │
│  │  RADIOLOGY        | 521000  | 89                        │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Can't wait? Finance is live now.                               │
│                                                                 │
│  Our FIX log analysis for Trade Support is available today.     │
│  Same local-first architecture, different format.               │
│                                                                 │
│  [See Finance Solution →]                                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  support@casparian.dev                      │
│                                                                 │
│  © 2026                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Healthcare Page SEO

- **Title:** "Casparian Flow - HL7 Archive Analytics (Coming Soon)"
- **Description:** "Query 5 years of HL7 archives with SQL. Analytics that Mirth can't do. Local-first, HIPAA-friendly. Join the waitlist."

---

## Page 4: Defense (`/defense`)

### Purpose
Capture interest from Defense/Intel analysts. Waitlist + value preview.

### Target Persona

| Attribute | Detail |
|-----------|--------|
| **Job Title** | Intelligence Analyst, SIGINT Analyst, GEOINT Analyst |
| **Company** | DoD, IC agencies, defense contractors |
| **Pain** | DDIL environments, closed systems, no custom algorithms |
| **Clearance** | TS/SCI typical |

### Core Value Proposition

**Headline:** "SQL for tactical data. On your laptop. Air-gapped."

**Subhead:** "CoT, NITF, KLV → queryable tables. Bring your Python plugins. Works offline."

### Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [← All Verticals]                          Casparian Flow      │
│                                                                 │
│  SQL for tactical data. On your laptop. Air-gapped.             │
│                                                                 │
│  CoT, NITF, KLV → queryable tables. Bring your Python plugins.  │
│  Works fully offline.                                           │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                                                         │    │
│  │              COMING 2026                                │    │
│  │                                                         │    │
│  │  We're launching vertical-by-vertical.                  │    │
│  │  Defense/Intel is on the roadmap.                       │    │
│  │                                                         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Join the waitlist for early access                             │
│                                                                 │
│  [Email address (use .mil or contractor domain)            ]    │
│  [Organization                                             ]    │
│  [Primary format: CoT / NITF / KLV / Other                ]    │
│                                                                 │
│  [Get Early Access →]                                           │
│                                                                 │
│  We'll notify you when Defense is ready + discuss pilot         │
│  opportunities.                                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  What you'll get                                                │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │                 │  │                 │  │                 │  │
│  │  TACTICAL       │  │  AIR-GAPPED     │  │  UPSTREAM OF    │  │
│  │  PARSERS        │  │                 │  │  PALANTIR       │  │
│  │                 │  │  No network     │  │                 │  │
│  │  CoT, NITF,     │  │  required.      │  │  We structure   │  │
│  │  KLV out of     │  │  Runs on        │  │  raw files.     │  │
│  │  out of the     │  │  laptop in      │  │  Feed any       │  │
│  │  box.           │  │  DDIL.          │  │  downstream.    │  │
│  │                 │  │                 │  │                 │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  The DDIL problem                                               │
│                                                                 │
│  Disconnected, Denied, Intermittent, Limited.                   │
│                                                                 │
│  Your sensors collect data: CoT tracks, imagery.                │
│  You need to query it NOW. No server. No cloud. No network.     │
│                                                                 │
│  Current tools are closed systems that don't allow custom       │
│  algorithms. Palantir requires structured data as INPUT.        │
│                                                                 │
│  Casparian structures the raw files so you can query them       │
│  with SQL. On your laptop. Works fully offline.                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Example workflow                                               │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian scan /mission_data/tracks --tag cot        │    │
│  │  ✓ Tagged 50,000 CoT messages                           │    │
│  │                                                         │    │
│  │  $ casparian process --tag cot                          │    │
│  │  ✓ Output: cot_tracks, cot_events                       │    │
│  │                                                         │    │
│  │  $ duckdb -c "                                          │    │
│  │      SELECT callsign, lat, lon, timestamp               │    │
│  │      FROM cot_tracks                                    │    │
│  │      WHERE callsign LIKE 'ALPHA%'                       │    │
│  │      AND timestamp > '2026-01-08 06:00:00'"             │    │
│  │                                                         │    │
│  │  callsign  | lat      | lon       | timestamp           │    │
│  │  ALPHA-1   | 34.0522  | -118.2437 | 2026-01-08 06:15:00 │    │
│  │  ALPHA-2   | 34.0530  | -118.2450 | 2026-01-08 06:15:02 │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  SBIR / Government Procurement                                  │
│                                                                 │
│  Interested in piloting Casparian for your program?             │
│  We're pursuing SBIR opportunities and welcome discussions      │
│  with program managers.                                         │
│                                                                 │
│  Contact: defense@casparian.dev                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  support@casparian.dev                      │
│                                                                 │
│  © 2026                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Defense Page SEO

- **Title:** "Casparian Flow - Air-Gapped Data Processing for Defense (Coming Soon)"
- **Description:** "SQL for CoT, NITF, KLV on your laptop. Air-gapped, DDIL-ready. Works offline. Join the waitlist."

---

## Page 5: Legal (`/legal`)

### Purpose
Capture interest from Litigation Support. Waitlist + value preview.

### Target Persona

| Attribute | Detail |
|-----------|--------|
| **Job Title** | Litigation Support Specialist, eDiscovery Analyst |
| **Company** | Law firm (<50 attorneys), corporate legal dept |
| **Pain** | Vendor costs $5-15K per matter; Relativity is $150K+/year |

### Core Value Proposition

**Headline:** "Process PSTs in-house. Stop paying vendors."

**Subhead:** "$5-15K saved per matter. Local processing. Exports to Relativity."

### Page Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [← All Verticals]                          Casparian Flow      │
│                                                                 │
│  Process PSTs in-house. Stop paying vendors.                    │
│                                                                 │
│  $5-15K saved per matter. Local processing.                     │
│  Exports to Relativity load file format.                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                                                         │    │
│  │              COMING Q2 2026                             │    │
│  │                                                         │    │
│  │  We're launching vertical-by-vertical.                  │    │
│  │  Legal/eDiscovery is on the roadmap.                    │    │
│  │                                                         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Join the waitlist for early access                             │
│                                                                 │
│  [Email address                                            ]    │
│  [Firm / Organization                                      ]    │
│  [Average PST volume per matter? (optional)                ]    │
│                                                                 │
│  [Get Early Access →]                                           │
│                                                                 │
│  We'll notify you when Legal is live + offer early              │
│  access pricing.                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  What you'll get                                                │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │                 │  │                 │  │                 │  │
│  │  PST PARSER     │  │  LOCAL          │  │  LOAD FILE      │  │
│  │                 │  │  PROCESSING     │  │  EXPORT         │  │
│  │  Extract        │  │                 │  │                 │  │
│  │  emails,        │  │  No cloud.      │  │  DAT/OPT        │  │
│  │  attachments,   │  │  No vendor.     │  │  format for     │  │
│  │  metadata.      │  │  Your server.   │  │  Relativity.    │  │
│  │                 │  │                 │  │                 │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  The cost problem                                               │
│                                                                 │
│  Relativity: $150K+/year. Overkill for small/mid firms.         │
│  Vendor processing: $5-15K per matter. Adds up fast.            │
│  Per-GB cloud tools: $250-500/GB. 200GB matter = $50-100K.      │
│                                                                 │
│  There are 80,000+ law firms with <10 attorneys.                │
│  They can't justify enterprise pricing.                         │
│                                                                 │
│  Casparian: Flat monthly rate. Process in-house. Keep margin.   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Example workflow                                               │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  $ casparian scan /collections/smith_pst --tag pst      │    │
│  │  ✓ Tagged 47 PST files (189 GB)                         │    │
│  │                                                         │    │
│  │  $ casparian process --tag pst                          │    │
│  │  ✓ Output: pst_emails, pst_attachments                  │    │
│  │                                                         │    │
│  │  $ duckdb -c "                                          │    │
│  │      SELECT * FROM pst_emails                           │    │
│  │      WHERE custodian = 'John Smith'                     │    │
│  │      AND date_sent BETWEEN '2024-01-01' AND '2024-12-31'│    │
│  │      AND body_text LIKE '%contract%'"                   │    │
│  │                                                         │    │
│  │  $ casparian export --format dat --output production.dat│    │
│  │  ✓ Exported 12,847 documents to Relativity load file    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  Casparian Flow                                                 │
│                                                                 │
│  Docs  •  GitHub  •  support@casparian.dev                      │
│                                                                 │
│  © 2026                                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Legal Page SEO

- **Title:** "Casparian Flow - PST Processing for eDiscovery (Coming Soon)"
- **Description:** "Process PSTs in-house. $5-15K saved per matter. Local processing, exports to Relativity. Join the waitlist."

---

## Technical Requirements

### Platform
- **Builder:** Carrd.co Pro ($19/year - supports multiple pages)
- **Domain:** TBD (casparian.dev, casparianflow.com, or casparian.io)
- **Payment:** Stripe (preferred) or Gumroad
- **Waitlist Forms:** Tally (free) or Carrd native forms

### Page Structure in Carrd

```
Carrd Site: casparian.dev
├── Page 1: / (Homepage)
├── Page 2: /finance
├── Page 3: /healthcare
├── Page 4: /defense
└── Page 5: /legal
```

### Distribution
- **Binary hosting:** GitHub Releases
- **Platforms:** macOS (arm64, x86_64), Linux (x86_64), Windows (x86_64)

### License Activation
1. Purchase on Stripe → license key via email
2. `casparian activate <key>` → validates once, cached locally
3. Works offline indefinitely after activation

---

## Waitlist Strategy

### Form Fields by Vertical

| Vertical | Required Fields | Optional Field |
|----------|-----------------|----------------|
| Healthcare | Email, Organization | Biggest HL7 pain point |
| Defense | Email, Organization | Primary format (CoT/NITF/KLV/Other) |
| Legal | Email, Firm | Average PST volume per matter |

### Waitlist Follow-Up Sequence

1. **Immediate:** Thank you email with timeline estimate
2. **Week 2:** "What's your current workflow?" survey (1 question)
3. **Month 1:** Progress update + early access offer if launching soon
4. **Launch:** "We're live" email with early access pricing

### Waitlist → Launch Criteria

| Vertical | Launch When |
|----------|-------------|
| Healthcare | 50+ waitlist signups OR 1 paying pilot |
| Legal | 50+ waitlist signups OR 1 paying pilot |
| Defense | SBIR award OR 3 pilot conversations with .mil/.gov |

---

## Analytics & Tracking

### Per-Page Goals (Plausible)

| Page | Primary Goal | Secondary Goal |
|------|--------------|----------------|
| Homepage | Click to vertical page | — |
| /finance | Download or Start Trial | Watch Demo |
| /healthcare | Waitlist signup | — |
| /defense | Waitlist signup | Contact defense@ |
| /legal | Waitlist signup | — |

### Waitlist Metrics

Track in spreadsheet or Notion:
- Signups by vertical (weekly)
- Organization types
- Pain point themes (from optional fields)
- Conversion to pilot (when launched)

---

## Content Requirements

### Copy Tone
- Technical but accessible
- Pain-first, then solution
- Vertical-specific language (ClOrdID for finance, custodian for legal)
- Respects reader's time

### Required Assets

| Asset | Page | Priority |
|-------|------|----------|
| Terminal screenshot (FIX workflow) | /finance | P0 |
| Demo video (60s trade break) | /finance | P0 |
| Favicon | All | P0 |
| Terminal screenshot (HL7) | /healthcare | P1 (for launch) |
| Terminal screenshot (CoT) | /defense | P1 (for launch) |
| Terminal screenshot (PST) | /legal | P1 (for launch) |

---

## Implementation Checklist

### Phase 1: Launch (Week 1-2)

```
Website Setup
[ ] Purchase domain
[ ] Set up Carrd Pro account
[ ] Create 5-page structure in Carrd

Homepage (/)
[ ] Write copy
[ ] Create vertical cards
[ ] Link to vertical pages
[ ] Test navigation

Finance Page (/finance)
[ ] Full content from spec
[ ] Terminal screenshot (real FIX workflow)
[ ] Demo video embed (or placeholder)
[ ] Pricing section with Stripe embeds
[ ] FAQ section
[ ] Test purchase flow

Coming Soon Pages (/healthcare, /defense, /legal)
[ ] Create Tally forms for each vertical
[ ] Write vertical-specific copy
[ ] Embed forms
[ ] Test form submissions
[ ] Set up email notifications for signups

Analytics
[ ] Set up Plausible
[ ] Configure goals per page
[ ] Test tracking

Launch
[ ] DNS configuration
[ ] SSL verification
[ ] Test all links
[ ] Test all forms
[ ] Announce to pilot prospects
[ ] Go live
```

### Phase 2: Iterate (Ongoing)

```
[ ] Monitor waitlist signups by vertical
[ ] Collect feedback from finance pilots
[ ] Update testimonial section when available
[ ] Build out next vertical when criteria met
```

---

## Success Metrics

### Week 4 Targets

| Metric | Target |
|--------|--------|
| Homepage → Vertical clicks | 60% |
| /finance downloads | 100 |
| /finance paid conversions | 3 |
| Waitlist signups (all verticals) | 50 |

### Week 12 Targets

| Metric | Target |
|--------|--------|
| /finance downloads | 500 |
| /finance paid conversions | 15 |
| Waitlist signups (healthcare) | 50 |
| Waitlist signups (legal) | 30 |
| Waitlist signups (defense) | 20 |

### Revenue Targets

| Timeline | Target |
|----------|--------|
| Month 1 | $0 (pilots) |
| Month 3 | $5K MRR (finance) |
| Month 6 | $15K MRR (finance + 1 other) |
| Month 12 | $50K MRR |

---

## Open Questions

1. **Domain:** casparian.dev, casparianflow.com, or casparian.io?
2. **Waitlist tool:** Tally (free, nice UX) or Carrd native forms?
3. **Defense contact:** defense@casparian.dev or same support@ email?
4. **Finance trial:** 14 days or 30 days?
5. **Waitlist incentive:** Early access pricing (20% off)? Or just early access?

---

## Future Iterations

### When Finance Validates (Month 3+)
- Add customer testimonials to /finance
- Create case study page
- Consider Google Ads for "FIX log analysis" keywords

### When Next Vertical Launches
- Convert waitlist page to full product page
- Email waitlist with launch announcement
- Offer early access pricing for 30 days

### When 3+ Verticals Live
- Add comparison table to homepage
- Consider vertical-specific pricing pages
- Add "Solutions" dropdown navigation

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2025-01 | 1.0 | Initial website spec (generic positioning) |
| 2026-01-19 | 2.0 | Finance vertical focus; Trade Break Workbench positioning |
| 2026-01-19 | 3.0 | **Multi-page structure:** Homepage + 4 vertical pages; Finance live, others as waitlist; Phased rollout strategy; Waitlist capture for demand validation |

---

*This document is the system of record for website content and structure. Strategy lives in STRATEGY.md. Pricing lives in docs/product/pricing.md.*

# Streaming Infrastructure & Redpanda Strategy

**Status:** Draft
**Parent:** STRATEGY.md Section 4 (Product Architecture)
**Related Specs:** specs/profiler.md, ARCHITECTURE.md (Sentinel/Worker)
**Version:** 1.0
**Date:** January 14, 2026

---

## 1. Executive Summary

This document evaluates streaming infrastructure options for Casparian Flow, with particular focus on Redpanda and its new Agentic Data Plane (ADP). The analysis covers current architecture bottlenecks, strategic positioning relative to Redpanda, and a phased adoption roadmap.

**Key Findings:**

1. **Current architecture is polling-driven** - SQLite + ZMQ works well for current scale but has known bottlenecks at 10+ workers
2. **Redpanda is overkill for now** - The operational complexity doesn't justify marginal latency gains at current scale
3. **Strategic opportunity exists** - Casparian and Redpanda ADP are complementary, not competitive
4. **Recommended path:** Differentiate first, complement later, integrate if market demands

**The Core Insight:**
> Redpanda ADP provides **governed AI agent access to existing structured data**.
> Casparian Flow **transforms dark data into structured datasets**.
> These are adjacent, not overlapping. Casparian is upstream of ADP.

---

## 2. Redpanda Ecosystem Overview

### 2.1 The Full Stack (as of January 2026)

```
┌─────────────────────────────────────────────────────────────────────┐
│                    REDPANDA ECOSYSTEM                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐    ┌─────────────────┐    ┌────────────────┐  │
│  │ Redpanda Connect│    │ Redpanda Core   │    │ Iceberg Topics │  │
│  │ 300+ connectors │ →  │ Streaming       │ →  │ Lakehouse      │  │
│  │ YAML pipelines  │    │ (Kafka API)     │    │ (Queryable)    │  │
│  │ Bloblang DSL    │    │                 │    │                │  │
│  └─────────────────┘    └─────────────────┘    └────────────────┘  │
│           │                                            │           │
│           └──────────────┬─────────────────────────────┘           │
│                          ▼                                         │
│              ┌─────────────────────────┐                           │
│              │   Agentic Data Plane    │                           │
│              │   - Agent Tool Server   │                           │
│              │   - Oxla SQL Engine     │                           │
│              │   - Agent governance    │                           │
│              └─────────────────────────┘                           │
│                          ↑                                         │
│                    AI Agents (Claude, GPT)                         │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Agentic Data Plane (ADP) - Announced October 2025

Redpanda acquired Oxla (distributed SQL engine) and launched ADP to target enterprise AI agent deployments.

| Component | Purpose | Status |
|-----------|---------|--------|
| **Streaming layer** | Low-latency events, HITL workflows | GA |
| **SQL query engine** | Unified interface for streams + Iceberg tables | Beta 2026 |
| **300+ connectors** | Pull context from enterprise systems | GA |
| **Governance** | Agent access control, intent logging, audit | Early 2026 |
| **Agent-protocol aware** | Native agent tool protocol support | GA |

**Key capabilities:**
- Full observability (every agent interaction logged, replayable)
- Federated querying across disparate sources
- Agentic Access Control (AAC) - short-lived, scoped tokens per agent

### 2.3 Redpanda vs Kafka

| Aspect | Redpanda | Apache Kafka |
|--------|----------|--------------|
| **Language** | C++ (Seastar framework) | Scala/Java (JVM) |
| **Dependencies** | Single binary | ZooKeeper/KRaft |
| **Latency** | Sub-ms (thread-per-core) | Higher (JVM overhead) |
| **Operations** | Simpler | More complex |
| **Ecosystem** | Growing | Mature |
| **Kafka Streams** | Limited support | Full support |

**Verdict:** Redpanda is the better choice for new deployments prioritizing simplicity and latency. Kafka is better for organizations already invested in the Kafka ecosystem.

---

## 3. Current Casparian Architecture Analysis

### 3.1 Inter-Component Communication

| Component | Current IPC | Mechanism |
|-----------|-------------|-----------|
| Scout → DB | Direct SQLite writes | Async batched inserts |
| Sentinel dispatch | SQLite polling | 100ms timeout loop |
| Sentinel ↔ Worker | ZMQ binary protocol | Unix socket / TCP |
| Job completion | ZMQ CONCLUDE message | Binary with JSON payload |
| Progress updates | mpsc channels | TUI-only |

### 3.2 Identified Bottlenecks

| Bottleneck | Current Behavior | Impact | Threshold |
|------------|------------------|--------|-----------|
| **SQLite polling** | 100ms dispatch loop | 10 queries/sec for job peek | >10 workers |
| **Stale worker cleanup** | 10s sweep interval | Up to 10s orphaned jobs on worker death | High availability |
| **No backpressure** | Scout → Sentinel | Unbounded queue growth possible | Large scans |
| **Single Sentinel** | No multi-instance | Single point of failure | Production deployments |

### 3.3 Data Flow (Current)

```
Scout                    Sentinel                   Worker
─────                    ────────                   ──────
scan_source()
    │
    ├─► batch INSERT ─────► SQLite ◄───── peek_job() (polling)
    │   scout_files              │              │
    │                            │              ▼
    │                       pop_job() ────► DISPATCH (ZMQ)
    │                            │              │
    │                            │              ├─► spawn guest
    │                            │              │   process
    │                            │              ▼
    │                       CONCLUDE ◄──── Arrow IPC batches
    │                       (ZMQ)               │
    │                            │              ▼
    │                       UPDATE          write outputs
    │                       job_status      (parquet/duckdb)
```

---

## 4. Casparian vs Redpanda Feature Comparison

### 4.1 Capability Matrix

| Capability | Redpanda Connect | Casparian Flow |
|------------|------------------|----------------|
| **Input sources** | 300+ connectors (DBs, APIs, queues) | Files on disk (Scout) |
| **Transformation** | Bloblang DSL, YAML processors | Python parsers |
| **Schema handling** | JSON Schema validation | **Schema contracts** (approval workflow) |
| **Type inference** | None | **Constraint-based inference** |
| **Output sinks** | Kafka, Iceberg, DBs, APIs | Parquet, SQLite, CSV |
| **Development UX** | YAML config | **Backtest, fail-fast, TUI** |
| **Agent tool server** | Exposes pipelines as tools | Exposes workflows as tools |
| **Governance** | ADP (agent tokens, audit) | Schema contracts only |
| **Deployment** | Cloud/BYOC/self-managed | **Local-first, air-gapped** |

### 4.2 Key Differentiators (What Redpanda Doesn't Do)

| Gap in Redpanda | Casparian Strength |
|-----------------|-------------------|
| No schema approval workflow | **Schema contracts** with human-in-loop |
| No parser development experience | **Backtest, fail-fast, TUI** |
| No constraint-based type inference | **Elimination-based inference** |
| Streaming focus (not file transformation) | **File-specific transformation** |
| Cloud-first | **Local-first, air-gapped capable** |
| Generic connectors | **Premade parsers for arcane formats** |

### 4.3 Overlap Analysis

```
        Redpanda                    Overlap                 Casparian
        ────────                    ───────                 ─────────
   300+ API connectors          Agent tool server      Premade file parsers
   Stream processing            AI integration         Schema contracts
   Kafka ecosystem              SQL query output       Backtest workflow
   Cloud-native                 Data transformation    Local-first
   Enterprise governance        Lineage tracking       Air-gapped deployment
```

**Core insight:** The overlap (agent tooling + SQL + transformation) is small. The differentiation is large.

---

## 5. Strategic Options

### 5.1 Option A: Compete (Build Full Agent Access Layer)

**What:** Extend Casparian to include governance layer, SQL engine, enterprise connectors.

```
Casparian builds:
├── Governance layer (agent tokens, audit logs)
├── SQL query engine (or integrate DuckDB)
├── More connectors (APIs, databases)
└── Enterprise features (RBAC, SSO)
```

| Pros | Cons |
|------|------|
| Own the full stack | Competing with $1B+ funded company |
| No dependency on Redpanda | Duplicating solved problems |
| Capture more value | Slow to market |

**Verdict:** ❌ High risk, resource intensive. Not recommended.

### 5.2 Option B: Complement (Dark Data Ingestion Layer)

**What:** Position Casparian as the ingestion layer that feeds structured data to platforms like Redpanda ADP.

```
┌─────────────────────────────────────────────────────────────────┐
│                     DATA LAKEHOUSE                              │
│                                                                 │
│   "Dark Data"              "Clean Data"          "AI Access"    │
│   ───────────              ────────────          ──────────     │
│                                                                 │
│   CSV, JSON,    ──────►    Iceberg      ──────►   Redpanda     │
│   logs, XML      Casparian  Tables        ADP     Governed     │
│   on disk        Flow                             Queries      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Value proposition:** *"Before ADP can serve your data, Casparian transforms it."*

| Pros | Cons |
|------|------|
| Clear differentiation | Dependent on Redpanda's success |
| Leverage Redpanda's distribution | May be absorbed into their offering |
| Focus on core strength | Partnership requires negotiation |

**Verdict:** ✅ Strong option for partnership/GTM.

### 5.3 Option C: Integrate (Become a Redpanda Connect Connector)

**What:** Build a Casparian Source Connector for Redpanda Connect.

```yaml
# redpanda-connect.yaml
input:
  casparian:
    source_path: "/data/raw/"
    parser: "sales_parser"
    schema_contract: "sales_v2"

pipeline:
  processors:
    - mapping: |
        root = this
        root.processed_at = now()

output:
  kafka:
    addresses: ["localhost:9092"]
    topic: "processed_sales"
```

| Pros | Cons |
|------|------|
| Access to Redpanda's customer base | Becomes a "component" not a "product" |
| Focus on transformation, not plumbing | Less control over UX |
| Open source visibility | Revenue model unclear |

**Verdict:** ⚠️ Good for adoption, risky for business. Consider as Phase 3.

### 5.4 Option D: Differentiate (Focus on What They Don't Do)

**What:** Double down on capabilities Redpanda doesn't have and can't easily build.

**Position:** *"The parser development platform for data engineers who need to transform files with confidence."*

```
Redpanda: "Connect any system to any system"
Casparian: "Transform messy files into trusted datasets"
```

| Pros | Cons |
|------|------|
| Clear differentiation | Smaller market than "all data integration" |
| Defensible moat | May miss the "agent access" wave |
| Can still integrate later | Requires discipline to stay focused |

**Verdict:** ✅ Best near-term strategy.

---

## 6. Recommended Hybrid Strategy

### 6.1 Phased Approach

| Phase | Timeline | Strategy | Actions |
|-------|----------|----------|---------|
| **Phase 1** | Now - 6 months | Differentiate | Double down on schema contracts, backtest, TUI |
| **Phase 2** | 6-12 months | Complement | Add Iceberg output, Redpanda topic sink |
| **Phase 3** | 12+ months | Evaluate Integration | Build connector if ADP gains traction |

### 6.2 Phase 1: Differentiate (Current Focus)

**Goal:** Establish Casparian as the premier parser development platform.

**Key differentiators to strengthen:**

| Feature | Action |
|---------|--------|
| Schema contracts | Ship approval workflow, amendment tracking |
| Backtest experience | Polish TUI, improve fail-fast feedback |
| Type inference | Document constraint-based approach |
| Local-first | Ensure air-gapped deployment works |
| Premade parsers | Ship FIX, HL7, CoT parsers |

**Success metric:** 10 paying customers who chose Casparian specifically for schema contracts or backtest workflow.

### 6.3 Phase 2: Complement (6-12 Months)

**Goal:** Enable Casparian outputs to flow into data lakehouses and streaming platforms.

**Technical integrations:**

#### 6.3.1 Iceberg Output Sink

```rust
// crates/casparian_worker/src/sinks/iceberg.rs
pub struct IcebergSink {
    catalog: Arc<dyn Catalog>,  // REST, Glue, Unity
    table: TableIdentifier,
    schema: ArrowSchema,
}

impl Sink for IcebergSink {
    async fn write_batch(&self, batch: RecordBatch) -> Result<()> {
        // Write to Iceberg table with lineage metadata
        // Include _cf_source_hash, _cf_job_id, _cf_parser_version
    }
}
```

**Catalog support priority:**
1. Iceberg REST Catalog (open standard)
2. AWS Glue
3. Databricks Unity Catalog

#### 6.3.2 Redpanda/Kafka Topic Sink

```rust
// crates/casparian_worker/src/sinks/redpanda.rs
pub struct RedpandaSink {
    producer: FutureProducer,
    topic: String,
    schema_registry: Option<SchemaRegistry>,
}

impl Sink for RedpandaSink {
    async fn write_batch(&self, batch: RecordBatch) -> Result<()> {
        // Serialize to Avro/JSON, publish to topic
        // Each row becomes a message with lineage headers
    }
}
```

**Value proposition:** *"Process your dark data with Casparian, stream to your lakehouse."*

### 6.4 Phase 3: Evaluate Integration (12+ Months)

**Trigger conditions for building Redpanda Connect connector:**

| Condition | Signal |
|-----------|--------|
| ADP gains significant traction | >1000 enterprise customers |
| Customer requests | >5 customers asking for direct integration |
| Partnership opportunity | Redpanda approaches for partnership |
| Market shift | "Agentic data" becomes mainstream requirement |

**If conditions are met:**
- Build official Casparian Source Connector
- List in Redpanda Connect marketplace
- Co-market as complementary solution

---

## 7. Streaming for Lineage (Internal Architecture)

### 7.1 The Lineage Challenge

Casparian needs to track complete lineage:
- **Input:** Source file → content hash → tags
- **Processing:** Job → parser version → status
- **Output:** Output file → row count → destination

**Current approach:** SQLite tables with foreign keys.

**Question:** Should lineage events stream through Redpanda?

### 7.2 Evaluation: SQL vs Event Stream for Lineage

| Aspect | SQL Only | Redpanda Events → SQL |
|--------|----------|----------------------|
| **Complexity** | Simple | More moving parts |
| **Query capability** | Immediate | Eventual (after materialization) |
| **Immutability** | Requires discipline | Native (append-only log) |
| **Replay** | Not possible | Built-in |
| **Multi-consumer** | Requires polling | Native pub/sub |
| **Scale requirement** | Low | High |

### 7.3 Recommendation: SQL-First with Event-Ready Schema

**Phase 1 (Now):** Store lineage in SQL, but structure as events.

```sql
CREATE TABLE cf_lineage_events (
    id              BIGSERIAL PRIMARY KEY,
    event_type      TEXT NOT NULL,
    event_time      TIMESTAMPTZ DEFAULT NOW(),

    -- Polymorphic payload (denormalized for query performance)
    source_id       UUID,
    file_hash       TEXT,
    file_path       TEXT,
    job_id          UUID,
    parser_name     TEXT,
    parser_version  TEXT,
    output_path     TEXT,
    row_count       BIGINT,

    CONSTRAINT valid_event CHECK (event_type IN (
        'file.discovered',
        'file.tagged',
        'job.queued',
        'job.started',
        'job.completed',
        'job.failed',
        'output.written'
    ))
);

CREATE INDEX idx_lineage_source ON cf_lineage_events(file_hash);
CREATE INDEX idx_lineage_job ON cf_lineage_events(job_id);
CREATE INDEX idx_lineage_type_time ON cf_lineage_events(event_type, event_time);
```

**Benefits:**
- Full lineage queryable via SQL now
- Event-shaped data (easy migration later)
- Append-only pattern (immutable-ish)

**Phase 2 (If needed):** Add Redpanda as event backbone.

```
Scout/Sentinel → Redpanda topics → Consumer → cf_lineage_events
```

The consumer is a simple "event sink" that INSERTs to the existing table. SQL queries don't change.

---

## 8. When to Adopt Streaming Infrastructure

### 8.1 Trigger Conditions

| Trigger | Why Redpanda Makes Sense |
|---------|--------------------------|
| >10 concurrent workers | SQLite polling becomes bottleneck |
| Multi-instance Sentinel | Need distributed coordination |
| Event replay requirement | Redpanda log enables re-processing |
| External integrations | Kafka ecosystem (Connect, Streams) |
| Audit/compliance | Immutable event log for lineage |
| Real-time dashboards | Multiple consumers need same events |

### 8.2 Current Status vs Triggers

| Trigger | Current State | Triggered? |
|---------|---------------|------------|
| >10 workers | Single-digit workers | ❌ No |
| Multi-Sentinel | Single instance | ❌ No |
| Event replay | Not required | ❌ No |
| External integrations | Not requested | ❌ No |
| Audit compliance | Schema contracts sufficient | ❌ No |

**Verdict:** No triggers currently activated. Continue with SQLite + ZMQ.

### 8.3 Alternative Solutions (Before Redpanda)

| Need | Alternative | Complexity |
|------|-------------|------------|
| Reduce polling | PostgreSQL LISTEN/NOTIFY | Low |
| Worker availability events | ZMQ PUB/SUB extension | Low |
| Backpressure | Bounded async channels | Already done |
| Multi-Sentinel | Postgres advisory locks | Medium |

These alternatives provide 80% of the benefit at 20% of the complexity.

---

## 9. Technical Integration Details (For Future Reference)

### 9.1 Redpanda Topic Design (If Adopted)

```
casparian.files.discovered     # Scout publishes file events
casparian.files.tagged         # Tag assignments
casparian.jobs.queued          # Sentinel creates job
casparian.jobs.assigned        # Sentinel → Worker dispatch
casparian.jobs.completed       # Worker completion events
casparian.parsers.deployed     # Plugin registry changes
casparian.lineage.events       # Unified lineage stream
```

### 9.2 Consumer Groups

| Consumer | Topics | Purpose |
|----------|--------|---------|
| `sentinel-dispatch` | files.discovered, files.tagged | Create jobs from file events |
| `lineage-materializer` | lineage.events | Write to SQL for querying |
| `metrics-aggregator` | jobs.* | Compute real-time metrics |
| `external-export` | lineage.events | Feed external systems |

### 9.3 Rust Dependencies (If Adopted)

```toml
[dependencies]
rdkafka = { version = "0.36", features = ["cmake-build"] }
```

**Note:** `rdkafka` is a mature Rust Kafka client that works with Redpanda.

---

## 10. Competitive Intelligence

### 10.1 Redpanda's Trajectory

| Date | Event | Implication |
|------|-------|-------------|
| April 2025 | $100M Series D, $1B valuation | Well-funded competitor |
| October 2025 | Oxla acquisition, ADP launch | Moving into AI agent space |
| 2026 | SQL layer GA (expected) | Complete "agentic" stack |

### 10.2 Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Redpanda builds file transformation | Low | High | Schema contracts moat |
| ADP becomes dominant | Medium | Medium | Complement, don't compete |
| Redpanda acquires similar tool | Low | High | Differentiate on local-first |
| Agent protocol standardization benefits Redpanda | Medium | Low | Both benefit from agent tooling growth |

### 10.3 Partnership Opportunity

**Potential pitch to Redpanda:**

> "Casparian transforms the messy files that your connectors can't handle. We output to Iceberg/Kafka. Our customers become your customers."

**Value to Redpanda:**
- Handles file formats outside their connector scope
- Drives adoption of Iceberg Topics
- Complements rather than competes with ADP

---

## 11. Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Adopt Redpanda now? | **No** | Current scale doesn't justify complexity |
| Strategic position | **Complement (upstream)** | Differentiated value, partnership potential |
| Phase 1 focus | **Differentiate** | Build moat before integration |
| Lineage storage | **SQL with event schema** | Query power now, migration path later |
| Alternative to polling | **PostgreSQL LISTEN/NOTIFY** | If needed before Redpanda |

---

## 12. Open Questions

1. **Partnership timing:** When to approach Redpanda for partnership discussion?
2. **Iceberg priority:** Should Iceberg sink be Phase 2 priority or earlier?
3. **ADP beta access:** Should we apply for ADP SQL layer beta to understand roadmap?
4. **Connector marketplace:** What are requirements to list in Redpanda Connect marketplace?
5. **Customer signal:** Are current prospects asking for streaming/Kafka integration?

---

## 13. Success Metrics

### 13.1 Phase 1 (Differentiate)

| Metric | Target | Timeline |
|--------|--------|----------|
| Customers citing schema contracts | 5+ | 6 months |
| Customers citing backtest workflow | 5+ | 6 months |
| Air-gapped deployments | 2+ | 6 months |

### 13.2 Phase 2 (Complement)

| Metric | Target | Timeline |
|--------|--------|----------|
| Iceberg sink shipped | GA | 12 months |
| Redpanda/Kafka sink shipped | GA | 12 months |
| Customers using lakehouse output | 10+ | 12 months |

### 13.3 Phase 3 (Integration - If Triggered)

| Metric | Target | Timeline |
|--------|--------|----------|
| Redpanda Connect connector | Listed | 18 months |
| Joint customers | 5+ | 18 months |
| Partnership agreement | Signed | 18 months |

---

## 14. References

- [Redpanda Agentic Data Plane](https://www.redpanda.com/agentic-data-plane)
- [Redpanda Oxla Acquisition](https://www.redpanda.com/press/redpanda-acquires-oxla-launches-new-agentic-data-plane-for-enterprise-data)
- [Redpanda ADP Docs](https://docs.redpanda.com/)
- [Redpanda Connect GitHub](https://github.com/redpanda-data/connect)
- [Iceberg Topics Integration](https://docs.redpanda.com/current/manage/iceberg/)
- [Redpanda vs Kafka Benchmarks](https://jack-vanlightly.com/blog/2023/5/15/kafka-vs-redpanda-performance-do-the-claims-add-up)
- [Redpanda $100M Series D](https://www.redpanda.com/press/redpanda-raises-100m-launches-enterprise-agentic-ai-platform)
- [rdkafka Rust Client](https://docs.rs/rdkafka/latest/rdkafka/)

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial draft: Redpanda evaluation, strategic options, phased roadmap |

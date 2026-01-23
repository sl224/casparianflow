# Intent → Pipeline Workflow (Non‑Brittle MCP Orchestration)

**Status**: Design Document  
**Version**: 1.2  
**Date**: 2026-01-21

This document specifies a **non‑brittle**, **supervised** workflow for converting natural language intent (e.g., “process all sales files”) into a deterministic data pipeline using **MCP (JSON‑RPC 2.0)** as the orchestration surface. The agent is a **proposer + orchestrator**. Deterministic evaluators score proposals. Humans approve irreversible actions.

> **Canonicality**: This v1.2 document supersedes any earlier drafts. If you find a copy that disagrees with this spec, treat it as obsolete.

---

## 1. Goals

1. Provide a robust flow from **intent → scan → select files → tagging rules → path‑derived fields → schema intent → parser draft → backtest → promote → publish → run**.
2. Ensure the system remains **deterministic**, **reproducible**, and **auditable**.
3. Keep the surface area small: a minimal set of MCP methods backed by existing CLI primitives.
4. Make the flow resilient to:
   - ambiguous semantics (“what counts as sales?”)
   - schema/type ambiguity
   - parser codegen failures
   - partial runs, crashes, restarts
   - large corpora (millions of files)

---

## 2. Non‑Goals

- Fully autonomous “hands‑off” end‑to‑end execution.
- A new general workflow engine or scheduler (reuse existing job infra).
- A rich GUI—this spec focuses on protocol + artifacts + gates.

---

## 3. Hard Invariants (Non‑Brittle Rules)

### 3.1 No irreversible side effects without explicit human approval
Irreversible actions include:
- enabling persistent tagging rules
- promoting/publishing schema
- publishing parser packs
- running/backfilling at scale

### 3.2 Bounded wire payloads
- **Never** return huge file lists inline over MCP.
- Large collections are represented by **FileSetId**, stored as artifacts and/or DB rows.
- Progress events are capped and monotonic; rich detail goes to artifacts/logs.

### 3.3 No semantic guessing
For intents like “sales files”:
- propose candidates + near‑misses + evidence
- ask targeted questions when confidence is not HIGH

### 3.4 Deterministic truth comes from backtest + validators
- The agent’s “confidence” does not overrule deterministic checks.
- Schema violations are explicit; no silent coercion by default.

### 3.5 No hidden state
Every run writes a **Session Bundle** containing the full decision history and all artifacts needed to reproduce outcomes.

---

## 4. Core Concepts and IDs

### 4.1 Session
A single end‑to‑end workflow instance has:
- `session_id` (UUID)
- `created_at`, `actor` (human identity), `client` (CLI/TUI/API)

### 4.2 FileSetId (critical)
Any set of files (selected candidates, near‑misses, worst offenders, fail‑fast sample, full corpus snapshot) is represented as:

- `file_set_id` (UUID)
- `count`
- `sampling_method` (`all` / `deterministic_sample` / `stratified_sample` / `top_k_failures`)
- `seed` (when sampled)
- `manifest_ref` (path in session bundle or DB pointer)

### 4.3 ProposalId
Any proposed change (selection, rules, path fields, schema intent, publish plan) has:
- `proposal_id`
- `proposal_hash` (canonical JSON, stable ordering)
- `created_at`

### 4.4 ApprovalToken
Approvals are explicit and bound to the *exact* choice:
- `approval_target_hash = hash(proposal_id + choice_payload + session_id + nonce)`
- tokens are **single‑use**; record their consumption in the session bundle.

---

## 5. Session Bundle (No Hidden State)

### 5.1 Canonical path
Default location:
- `~/.casparian_flow/sessions/{session_id}/`

Override allowed via env/config:
- `CASP_SESSION_DIR`

### 5.2 Required layout
```text
sessions/{session_id}/
  manifest.json
  corpora/
    corpus_manifest.jsonl
    filesets/
      {file_set_id}.jsonl
  proposals/
    selection_{proposal_id}.json
    tag_rules_{proposal_id}.json
    path_fields_{proposal_id}.json
    schema_intent_{proposal_id}.json
    publish_plan_{proposal_id}.json
  reports/
    backtest_{job_id}.json
    backtest_iters_{job_id}.jsonl
  approvals.jsonl
  patches/
    schema_patch_{iteration_id}.json
    parser_patch_{iteration_id}.patch
    rule_patch_{iteration_id}.json
  logs/ (optional pointers)
```

### 5.3 Corpus snapshotting (reproducibility)
At the first human approval of selection (Gate G1), persist:
- `corpus_manifest.jsonl` entries: `{path, size, mtime, content_hash}`
This ensures future backtests and regressions compare against the same inputs.

---

## 6. State Machine + Gates

### States
S0 **InterpretIntent**  
S1 **ScanCorpus**  
S2 **ProposeSelection** → SelectionProposal  
**G1** Human approves selection + corpus snapshot  
S3 **ProposeTagRules** → TagRuleProposal  
**G2** Human approves enabling persistent tagging rules  
S4 **ProposePathFields** → PathFieldProposal  
**G3** Human approves derived fields + namespacing + collision resolutions  
S5 **InferSchemaIntent** → SchemaIntentProposal  
**G4** Human resolves ambiguities / approves safe defaults  
S6 **GenerateParserDraft**  
S7 **BacktestFailFast** (loop with patches)  
S8 **PromoteSchema** (ephemeral → schema‑as‑code)  
S9 **PublishPlan** → PublishPlan  
**G5** Human approves publish (schema + parser)  
S10 **PublishExecute**  
S11 **RunPlan** → RunPlan  
**G6** Human approves run/backfill scope  
S12 **RunExecute**

### Loop discipline (S7)
Each iteration applies **exactly one** patch type:
- schema patch OR parser patch OR tag/path patch  
Then re‑backtest on a fixed evaluation set (see §10).

---

## 7. MCP Method Surface (JSON‑RPC 2.0)

### Conventions
- All requests include `session_id`.
- All responses include `request_id` (echo) and stable IDs.
- Methods are pure where possible; mutations require approval tokens.

### 7.1 Session lifecycle
- `casp.session.create` → `{ "session_id": "uuid" }`
- `casp.session.status` → `{ "state": "S*", "pending_questions": [], "active_jobs": [], "artifacts": [] }`
- `casp.session.events.subscribe` (optional) → server notifications  
  If not implemented, clients poll `casp.session.status`.

### 7.2 Scan
- `casp.scan.start` → `{ "scan_job_id": "uuid" }`
- `casp.scan.status` → progress + `scan_result_ref` (artifact)

### 7.3 File selection
- `casp.select.propose` → `SelectionProposal` (references `selected_file_set_id`)
- `casp.fileset.sample` → `{ "examples": [...] }` (bounded)
- `casp.fileset.page` → `{ "items": [...], "next_cursor": "..." }` (bounded paging)

### 7.4 Tag rules
- `casp.tags.propose_rules` → `TagRuleProposal`
- `casp.tags.apply_rules` → requires `approval_token`

### 7.5 Path‑derived fields
- `casp.path_fields.propose` → `PathFieldProposal`
- `casp.path_fields.apply` → requires `approval_token`

### 7.6 Schema intent
- `casp.schema.infer_intent` → `SchemaIntentProposal`
- `casp.schema.resolve_ambiguity` → requires `approval_token` (or returns new proposal for G4)

### 7.7 Parser draft + backtest loop
- `casp.parser.generate_draft` → `ParserDraft`
- `casp.backtest.start` → `{ "backtest_job_id": "uuid" }`
- `casp.backtest.status` → bounded progress envelope
- `casp.backtest.report` → `BacktestReport` (artifact + top‑K summary inline)
- `casp.patch.apply` → applies a single patch (schema/parser/rule) for the loop

### 7.8 Promote + publish
- `casp.schema.promote` → `{ "schema_as_code_ref": "path" }`
- `casp.publish.plan` → `PublishPlan`
- `casp.publish.execute` → requires `approval_token`

### 7.9 Run/backfill
- `casp.run.plan` → `RunPlan`
- `casp.run.execute` → requires `approval_token`

---

## 8. Artifact Types (Language‑Neutral JSON Shapes)

All artifact JSON must be canonicalized for hashing (sorted keys, stable arrays where applicable).

### 8.1 SessionBundleManifest
```json
{
  "session_id": "uuid",
  "created_at": "RFC3339",
  "intent_text": "string",
  "state": "S0..S12",
  "corpus_manifest_ref": "path",
  "artifacts": [ { "kind": "SelectionProposal", "ref": "..." } ]
}
```

### 8.2 SelectionProposal (bounded)
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "selected_file_set_id": "uuid",
  "near_miss_file_set_id": "uuid",
  "evidence": {
    "top_dir_prefixes": [ { "prefix": "...", "count": 123 } ],
    "extensions": [ { "ext": ".csv", "count": 999 } ],
    "semantic_tokens": [ { "token": "sales", "count": 120 } ],
    "collision_with_existing_tags": [ { "tag": "...", "count": 12 } ]
  },
  "confidence": { "score": 0.0, "label": "LOW|MED|HIGH", "reasons": ["..."] },
  "preview": {
    "selected_examples": ["path1", "path2"],
    "near_miss_examples": ["path3", "path4"]
  },
  "next_actions": ["ASK_HUMAN_CONFIRM_SELECTION", "PROPOSE_TAG_RULES"]
}
```

### 8.3 TagRule DSL (constrained)
```json
{
  "rule_id": "string",
  "enabled": false,
  "when": {
    "path_glob": ["**/sales/**"],
    "extension": [".csv", ".parquet"],
    "magic_bytes": [ { "offset": 0, "hex": "..." } ]
  },
  "add_tags": ["sales"],
  "route_to_topic": "sales_ingest"
}
```

### 8.4 RuleEvaluation (reproducible)
```json
{
  "matched_file_set_id": "uuid",
  "negative_sample_file_set_id": "uuid",
  "precision_estimate": 0.0,
  "recall_estimate": 0.0,
  "false_positive_estimate": 0.0,
  "conflicts": [ { "existing_rule_id": "...", "overlap_count": 12 } ],
  "examples": {
    "matches": ["..."],
    "near_misses": ["..."],
    "false_positive_examples": ["..."]
  },
  "sampling": {
    "method": "stratified_sample",
    "seed": 12345,
    "notes": "Negative set sampled from sibling dirs + same extensions"
  }
}
```

### 8.5 TagRuleProposal
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "candidates": [
    {
      "rule": { /* TagRule */ },
      "evaluation": { /* RuleEvaluation */ },
      "confidence": { "score": 0.0, "label": "LOW|MED|HIGH", "reasons": ["..."] }
    }
  ],
  "recommended_rule_id": "string|null",
  "required_human_questions": [ /* HumanQuestion */ ]
}
```

### 8.6 PathFieldProposal
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "input_file_set_id": "uuid",
  "namespacing": { "default_prefix": "_cf_path_", "allow_promote": true },
  "fields": [
    {
      "field_name": "region",
      "dtype": "string|int|date|timestamp",
      "pattern": { "kind": "key_value|regex|segment_position|partition_dir", "value": "..." },
      "source": { "segment_index": 4, "filename_group": null },
      "coverage": { "matched_files": 812, "total_files": 900 },
      "examples": ["us-east-1", "eu-west-1"],
      "confidence": { "score": 0.0, "label": "LOW|MED|HIGH", "reasons": ["..."] }
    }
  ],
  "collisions": {
    "same_name_different_values": [ { "field_name": "dt", "example_paths": ["..."] } ],
    "segment_overlap": [ { "segment_index": 3, "field_names": ["case_id", "incident_id"] } ],
    "with_parsed_columns": [ { "derived_field": "_cf_path_region", "parsed_column": "region" } ]
  },
  "required_human_questions": [ /* HumanQuestion */ ]
}
```

### 8.7 SchemaIntentProposal (ambiguity explicit)
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "input_sources": {
    "parser_output_sample_ref": "path",
    "derived_fields_ref": "path"
  },
  "columns": [
    {
      "name": "amount",
      "source": "parsed|derived",
      "declared_type": "decimal(38,9)|int64|utf8|timestamp(ms,UTC)|...",
      "nullable": true,
      "constraints": { "enum": null, "min": null, "max": null },
      "inference": {
        "method": "constraint_elimination|ambiguous_requires_human",
        "candidates": ["decimal(38,2)", "decimal(38,9)"],
        "evidence": { "null_rate": 0.01, "distinct": 12345, "format_hits": 900 },
        "confidence": { "score": 0.0, "label": "LOW|MED|HIGH" }
      }
    }
  ],
  "column_collisions": [
    { "left": "_cf_path_region", "right": "region", "resolution": "namespace|rename|required_human" }
  ],
  "safe_defaults": {
    "timestamp_timezone": "require_utc",
    "string_truncation": "reject",
    "numeric_overflow": "reject"
  },
  "required_human_questions": [ /* HumanQuestion */ ]
}
```

### 8.8 ParserDraft
```json
{
  "draft_id": "uuid",
  "parser_identity": {
    "name": "sales_csv",
    "version": "0.1.0",
    "topics": ["sales_ingest"],
    "source_hash": "blake3hex"
  },
  "repo_ref": "path",
  "entrypoints": ["..."],
  "tests_ref": "path",
  "build_status": "pass|fail",
  "lint_status": "pass|fail"
}
```

### 8.9 BacktestProgressEnvelope (bounded + monotonic)
```json
{
  "job_id": "uuid",
  "phase": "scan|parse|validate|summarize",
  "elapsed_ms": 123,
  "metrics": {
    "files_processed": 10,
    "files_total_estimate": 100,
    "rows_emitted": 10000,
    "rows_quarantined": 12
  },
  "top_violation_summary": [
    { "violation_type": "TypeMismatch", "count": 120, "top_columns": [ { "name": "amount", "count": 80 } ] }
  ],
  "stalled": false
}
```

### 8.10 BacktestReport (top‑K summary + artifact refs)
```json
{
  "job_id": "uuid",
  "input_file_set_id": "uuid",
  "iterations_ref": "reports/backtest_iters_{job_id}.jsonl",
  "quality": {
    "files_processed": 900,
    "rows_emitted": 1234567,
    "rows_quarantined": 1234,
    "quarantine_pct": 0.099,
    "pass_rate_files": 0.93
  },
  "top_k_violations": [
    {
      "violation_type": "TypeMismatch",
      "count": 123,
      "top_columns": [ { "name": "amount", "count": 80 } ],
      "example_contexts": [
        {
          "column": "amount",
          "expected": "decimal(38,2)",
          "observed_types": { "utf8": 0.7, "float64": 0.3 },
          "sample_values": ["12.34", "N/A"],
          "suggestions": [ { "code": "SUG_CAST_NUMERIC", "confidence": 0.8 } ]
        }
      ]
    }
  ],
  "full_report_ref": "reports/backtest_{job_id}.json"
}
```

### 8.11 PublishPlan
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "schema": {
    "schema_name": "sales",
    "new_version": "1.0.0",
    "schema_as_code_ref": "path",
    "compiled_schema_ref": "path"
  },
  "parser": {
    "name": "sales_csv",
    "new_version": "1.0.0",
    "source_hash": "blake3hex",
    "topics": ["sales_ingest"]
  },
  "invariants": {
    "route_to_topic_in_parser_topics": true,
    "no_same_name_version_different_hash": true,
    "sink_validation_passed": true
  },
  "diff_summary": ["nullable changes: +2 cols", "new cols: 3"],
  "required_human_questions": []
}
```

### 8.12 RunPlan (job‑partitioned outputs)
```json
{
  "proposal_id": "uuid",
  "proposal_hash": "hex",
  "input_file_set_id": "uuid",
  "route_to_topic": "sales_ingest",
  "parser_identity": { "name": "sales_csv", "version": "1.0.0", "source_hash": "..." },
  "sink": {
    "type": "parquet_dir|duckdb",
    "path": "/data/out/sales/",
    "duckdb": { "path": null, "table": null }
  },
  "write_policy": "new_job_partition|error_if_job_exists",
  "job_partitioning": { "mode": "by_job_id", "pattern": "{output}_{job_id}.parquet" },
  "validations": { "sink_valid": true, "topic_mapping_valid": true },
  "estimated_cost": { "files": 1842, "size_bytes": 987654321 }
}
```

### 8.13 HumanQuestion (targeted)
```json
{
  "question_id": "uuid",
  "kind": "CONFIRM_SELECTION|RESOLVE_AMBIGUITY|RESOLVE_COLLISION|CONFIRM_PUBLISH|CONFIRM_RUN",
  "prompt": "string",
  "options": [ { "option_id": "a", "label": "...", "consequence": "...", "default": true } ],
  "evidence_refs": ["proposals/selection_...json"],
  "deadline": null
}
```

### 8.14 DecisionRecord
```json
{
  "timestamp": "RFC3339",
  "actor": "user_id",
  "decision": "approve|reject",
  "target": { "proposal_id": "uuid", "approval_target_hash": "hex" },
  "choice_payload": { "selected_rule_id": "...", "collision_resolutions": [] },
  "notes": "string|null"
}
```

---

## 9. Deterministic Confidence Model

All confidence is computed deterministically from evidence. The agent may explain but not override.

### 9.1 SelectionConfidence
Signals:
- concentration of matches into a few dir prefixes (entropy)
- extension narrowness
- semantic tokens in path
- collision rate with existing tags
- optional bounded header sniff for text formats (top line only, capped bytes)

### 9.2 PathFieldConfidence
Signals:
- explicit `key=value` strongest
- stable segment position across corpus
- validated value formats (date parsing, numeric)
- low collision and overlap

### 9.3 SchemaConfidence
Signals:
- constraint elimination yields a single candidate
- stable null rate / distinct counts
- format checks satisfied (timezone, precision)

### 9.4 ParserConfidence
Signals:
- backtest thresholds met on fixed evaluation set
- no parser-level errors
- violations stable and explainable

---

## 10. Fail‑Fast Backtest + Convergence (Non‑Brittle Loop)

### 10.1 Fixed evaluation sets
On first backtest:
- `fail_fast_file_set_id`: deterministic sample biased to likely failures, frozen
- `worst_offender_file_set_id`: top K failing files from iteration 1, frozen

All subsequent iterations must run at least on:
- worst_offender set (regression guard)
- fail_fast set (speed)

Before declaring convergence, run:
- `full_validation_file_set_id` (either the whole selection or a large deterministic sample)

### 10.2 Convergence criteria (mechanical)
Converged when all hold:
- quarantine_pct ≤ threshold
- no new violation types in last N iterations (default N=2)
- no regressions on worst_offender set
- parser build/tests pass

### 10.3 Stall detection
Stalled if:
- `elapsed_ms` increases AND
- both `files_processed` and `rows_emitted` remain unchanged ≥ threshold window  
Phase-aware grace window applies.

---

## 11. Tag Rule Evaluation: define the negative set

To estimate precision/false positives reproducibly:
- Build `negative_sample_file_set_id` via stratified sampling:
  - same extension(s) as positives
  - sibling directories
  - small random remainder sample
- Record seed + method in RuleEvaluation.

---

## 12. Topic and Mapping Consistency

Required invariant checks (pre‑publish and pre‑run):
- `route_to_topic` is present in `parser_identity.topics`
- Do not overload “topic” for output routing keys.

---

## 13. Implementation Plan (High‑Level)

1) Implement FileSet store (jsonl + metadata)
2) Implement selection proposer + evidence + confidence
3) Implement rule proposer + evaluator + apply gating
4) Implement path field extractor + collision reports + apply gating
5) Implement schema intent inference + ambiguity representation
6) Implement parser draft generator wrapper
7) Implement backtest loop + progress envelopes + bounded violation summary
8) Implement promote/publish plan + invariant checks
9) Implement run plan + sink validation + job partition output policy
10) Implement session status + pending questions + optional event stream

---

## 14. Test Plan

### Unit tests
- path tokenization and pattern extraction
- collision detection
- confidence scoring determinism
- approval token binding and single-use enforcement

### Property tests
- canonical JSON hashing stable under reordering
- file set paging correctness
- sampling reproducibility

### Integration tests
- scan → select.propose → fileset.sample → approve G1
- propose_rules → dry-run evaluation → approve G2
- propose_path_fields → apply → infer_intent → resolve ambiguity
- generate draft → backtest loop → converge → promote → publish.plan → publish.execute → run.plan → run.execute

### Regression tests
- ensure no MCP method returns huge arrays
- ensure examples in this document match actual API shapes

---

## 15. Appendix: Updated Examples (No Inline File Lists)

### A) Selection proposal + sampling
1) `casp.select.propose` returns:
- `selected_file_set_id = "fs-123"`
- `near_miss_file_set_id = "fs-456"`
- preview examples only

2) Client calls:
- `casp.fileset.sample({ "file_set_id": "fs-123", "n": 25 })` for confirmation UI

### B) Applying a chosen tag rule safely
- User chooses `rule_id="sales_rule_v1"` and resolves overlaps
- Client requests an approval token bound to:
  - proposal_id
  - selected_rule_id
  - overlap resolutions
- `casp.tags.apply_rules` requires that token; server verifies and records in `approvals.jsonl`.

# Spec Maintenance Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 2.6
**Purpose:** Periodic audit and cleanup of the entire spec corpus
**Related:** spec_refinement_workflow.md (single-spec refinement)

---

## 1. Overview

This workflow maintains the health of the **entire specification corpus** by identifying misalignments between specs and code, detecting organizational problems, and recommending structural changes.

**Key Difference from Refinement Workflow:**
- **Refinement:** One spec in, refined spec out
- **Maintenance:** All specs in, corpus-level recommendations out

### 1.1 Design Principles

1. **Code is Truth** - Codebase is the source of truth for implemented features
2. **Specs are Intent** - Specs document intended behavior (past, present, or future)
3. **Living Documentation** - Specs should evolve with the codebase
4. **Minimal Spec Surface** - Fewer, focused specs > many overlapping specs
5. **Archive Don't Delete** - Move obsolete specs to archive, preserve history
6. **Graceful Degradation** - One bad file should never block the entire audit

### 1.2 When to Run

| Trigger | Reason |
|---------|--------|
| **Quarterly** | Regular hygiene |
| **Major release** | Verify specs match shipped code |
| **Before new feature spec** | Ensure no overlap with existing specs |
| **After significant refactor** | Codebase may have diverged |
| **When onboarding** | Identify stale docs for new team members |

---

## 2. Execution Model

### 2.1 Single-Instance Architecture

The maintenance workflow uses a **single-instance** execution model with user checkpoints. Unlike the refinement workflow (which uses adversarial Engineer/Reviewer roles), maintenance is primarily observational and doesn't require debate.

```
User initiates maintenance
        │
        ▼
┌───────────────────────────────────────────────────────────────┐
│                    MAINTENANCE AGENT                          │
│           (Single Claude instance, interactive)               │
│                                                               │
│  Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4                  │
│  Inventory   Alignment   Cross-Spec   Recommendations         │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
  ┌─────────────┐
  │ USER REVIEW │  Review recommendations via AskUserQuestion
  └─────────────┘
        │
        ▼ (User approves)
┌───────────────────────────────────────────────────────────────┐
│                    MAINTENANCE AGENT                          │
│                     Phase 5: Execute                          │
└───────────────────────────────────────────────────────────────┘
```

### 2.2 Role: Maintenance Agent

**Responsibilities:**
- Enumerate all specs in the corpus (Phase 1)
- Check alignment between specs and codebase (Phase 2)
- Detect cross-spec issues: overlaps, gaps, fragmentation (Phase 3)
- Generate prioritized recommendations (Phase 4)
- Execute user-approved changes (Phase 5)

**Context Requirements:**

| Source | Purpose | When to Read |
|--------|---------|--------------|
| `CLAUDE.md` | System architecture | Session start |
| `ARCHITECTURE.md` | Detailed design | When checking alignment |
| `specs/*.md` | Spec content | All phases |
| Codebase (`crates/`, `src/`) | Implementation truth | Phase 2 (alignment) |

**Output Files:**

| Phase | File | Content |
|-------|------|---------|
| 1 | `inventory.md` | Spec list with metadata |
| 1 | `reference_graph.json` | Spec relationships (parent/child, inbound/outbound refs) |
| 2 | `alignment_report.md` | Per-spec code alignment status |
| 3 | `cross_spec_report.md` | Overlaps, gaps, fragmentation |
| 3 | `naming_analysis.md` | Naming convention violations and proposed fixes |
| 4 | `recommendations.md` | Prioritized action items |
| 5 | `execution_log.md` | Changes made |
| 5 | `reference_propagation.md` | Reference updates from splits/merges/archives/renames |

### 2.3 User Checkpoints

Two mandatory interaction points ensure user control:

**Checkpoint 1: Pre-Execution Review (after Phase 4)**

Present summary of findings and recommendations. User options:
1. **Review details** - Show full `recommendations.md`
2. **Approve all** - Execute all recommendations
3. **Approve by priority** - Execute HIGH only, defer MEDIUM/LOW
4. **Selective approval** - Choose specific recommendations
5. **Cancel** - End session without changes

**Checkpoint 2: Per-Action Confirmation (during Phase 5)**

| Action Type | Confirmation Level |
|-------------|--------------------|
| ARCHIVE/DELETE | Explicit confirmation required |
| UPDATE | Show diff, confirm |
| MERGE | Show both files, confirm target |
| LINK | Auto-approve (trivial change) |
| WRITE | Show outline, confirm |

### 2.4 Why Not Multi-Instance?

The refinement workflow uses 3 roles (Engineer, Reviewer, Mediator) because:
- Proposals are subjective and benefit from adversarial review
- Multiple rounds iterate toward convergence
- Debates need structured resolution

Maintenance is different:
- Phases 1-3 gather **facts**, not opinions
- Phase 4 recommendations follow **objective criteria** (stale = >90 days)
- Phase 5 changes are **user-approved** before execution

A single instance suffices. Adversarial review adds overhead without benefit.

### 2.5 Handoff to Refinement Workflow

When maintenance identifies a spec needing **significant updates** (>30% sections require changes), don't attempt inline fixes. Instead:

1. Recommend using the Refinement Workflow (`spec_refinement_workflow.md`)
2. Mark spec as `IN_REFINEMENT` in inventory
3. Skip that spec in Phase 5
4. Re-audit after refinement completes

**Threshold:** If alignment report shows >30% of spec sections as SPEC_STALE, recommend refinement.

### 2.6 Parallelization

While phases execute sequentially (each depends on prior output), work **within phases** can be parallelized for large corpora.

#### 2.6.1 Parallelization by Phase

| Phase | Parallel? | Strategy | Aggregation Point |
|-------|-----------|----------|-------------------|
| **Phase 1: Inventory** | Yes | Scan files concurrently | Merge into single inventory.md |
| **Phase 2: Alignment** | Yes | Each spec↔code check independent | Merge into alignment_report.md |
| **Phase 3: Cross-Spec** | Partial | See below | Merge sub-reports |
| **Phase 4: Recommendations** | No | Needs full Phase 3 to prioritize | N/A |
| **Phase 5: Execution** | Careful | See below | Sequential logging |

#### 2.6.2 Phase 1: Inventory Parallelization

```
┌─────────────────────────────────────────────────────────────┐
│                    INVENTORY PHASE                          │
│                                                             │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │
│  │ Worker  │  │ Worker  │  │ Worker  │  │ Worker  │        │
│  │ specs/* │  │ views/* │  │ meta/*  │  │ strat/* │        │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘        │
│       │            │            │            │              │
│       └────────────┴─────┬──────┴────────────┘              │
│                          ▼                                  │
│                   ┌─────────────┐                           │
│                   │   Merge     │                           │
│                   │ inventory   │                           │
│                   └─────────────┘                           │
└─────────────────────────────────────────────────────────────┘
```

- Each worker scans a directory subtree
- Workers report: file path, status, LOC, last modified
- Aggregator merges results, sorts, deduplicates

#### 2.6.3 Phase 2: Alignment Parallelization (Highest Benefit)

Each spec's alignment check is **embarrassingly parallel**:

```
┌─────────────────────────────────────────────────────────────┐
│                   ALIGNMENT PHASE                           │
│                                                             │
│  Inventory: [spec_1, spec_2, spec_3, ..., spec_N]          │
│                         │                                   │
│            ┌────────────┼────────────┐                      │
│            ▼            ▼            ▼                      │
│       ┌─────────┐  ┌─────────┐  ┌─────────┐                │
│       │ Check   │  │ Check   │  │ Check   │   ... N workers│
│       │ spec_1  │  │ spec_2  │  │ spec_3  │                │
│       │ vs code │  │ vs code │  │ vs code │                │
│       └────┬────┘  └────┬────┘  └────┬────┘                │
│            │            │            │                      │
│            └────────────┴─────┬──────┘                      │
│                               ▼                             │
│                    ┌───────────────────┐                    │
│                    │ Aggregate results │                    │
│                    │ Group by category │                    │
│                    └───────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

**Why highest benefit:** Each alignment check involves grep/search operations that are I/O bound. For 50+ specs, parallel execution can reduce Phase 2 time by 5-10x.

**Concurrency limit:** Cap at ~10 parallel workers to avoid overwhelming file system or search tools.

#### 2.6.4 Phase 3: Partial Parallelization

Phase 3 has independent sub-analyses that can run in parallel:

| Sub-Analysis | Parallelizable | Dependencies |
|--------------|----------------|--------------|
| Overlap detection | Yes (pairwise) | Needs inventory |
| Gap detection | Yes | Needs inventory + alignment |
| Fragmentation detection | Yes | Needs inventory |
| Bloat detection | Yes | Needs inventory |
| Naming convention | Yes | Needs inventory |
| Reference health | Partial | Graph traversal needs full inventory |

```
┌─────────────────────────────────────────────────────────────────┐
│                     CROSS-SPEC PHASE                             │
│                                                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ │
│  │ Overlap  │ │   Gap    │ │  Bloat   │ │ Fragment │ │ Naming │ │
│  │ Detector │ │ Detector │ │ Detector │ │ Detector │ │ Check  │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └───┬────┘ │
│       │            │            │            │           │       │
│       └────────────┴─────┬──────┴────────────┴───────────┘       │
│                          ▼                                       │
│                ┌──────────────────┐                              │
│                │ Reference Health │  (sequential - needs         │
│                │     Analysis     │   full graph)                │
│                └────────┬─────────┘                              │
│                         ▼                                        │
│                ┌──────────────────┐                              │
│                │  Merge Reports   │                              │
│                └──────────────────┘                              │
└─────────────────────────────────────────────────────────────┘
```

#### 2.6.5 Phase 5: Careful Parallelization

Execution actions have dependencies and potential conflicts:

| Action Type | Parallelizable | Risk |
|-------------|----------------|------|
| ARCHIVE | Yes | None (independent moves) |
| LINK | Yes | None (adding references) |
| UPDATE | Careful | May conflict if specs reference each other |
| MERGE | No | Must be sequential (file deletion) |
| WRITE | Careful | New specs may reference updated specs |

**Safe parallel pattern:**
```
1. Archives (parallel)     ──► Sync point
2. Merges (sequential)     ──► Sync point
3. Updates (parallel, if independent) ──► Sync point
4. Links (parallel)        ──► Sync point
5. Writes (sequential, may reference updated content)
```

#### 2.6.6 Error Handling in Parallel Context

Parallel execution integrates with graceful degradation (Section 4):

- **Worker failure:** Log error, mark spec as SKIPPED, continue others
- **Aggregation:** Collect all errors into error_report.md
- **No blocking:** One worker's failure doesn't stop other workers
- **Checkpoint:** Record progress per-worker for recovery

```
Worker 1: ✓ spec_1.md
Worker 2: ✗ spec_2.md (FILE_UNREADABLE) → logged, continue
Worker 3: ✓ spec_3.md
Worker 4: ✓ spec_4.md
...
Aggregator: 47/50 succeeded, 3 errors logged
```

#### 2.6.7 When to Parallelize

| Corpus Size | Recommendation |
|-------------|----------------|
| < 20 specs | Sequential (overhead not worth it) |
| 20-50 specs | Parallelize Phase 2 only |
| 50-100 specs | Parallelize Phases 1, 2, 3 |
| > 100 specs | Full parallelization + consider batching |

**Implementation note:** Parallelization is an optimization. The workflow should work correctly in sequential mode first, then add parallelization for performance.

### 2.7 Contract Compliance Mode

In addition to the standard "spec vs code" alignment, this workflow can operate in **Contract Compliance Mode** where specs are checked against a **contract specification** rather than code.

#### 2.7.1 Mode Invocation

```
spec_maintenance_workflow
  --mode contract_compliance
  --corpus "specs/meta/*_workflow.md"
  --contract "specs/meta/workflow_manager.md#section-13"
```

| Parameter | Description |
|-----------|-------------|
| `--mode` | `standard` (default) or `contract_compliance` |
| `--corpus` | Glob pattern for specs to check |
| `--contract` | Contract spec path with optional section anchor |

#### 2.7.2 How It Differs from Standard Mode

| Aspect | Standard Mode | Contract Compliance Mode |
|--------|---------------|-------------------------|
| **Truth source** | Codebase | Contract specification |
| **Alignment check** | Spec sections → grep code | Spec sections → contract requirements |
| **Typical corpus** | All specs | Subset (e.g., workflow specs) |
| **Output** | Alignment to code | Compliance to contract |

#### 2.7.3 Phase Adaptations

**Phase 1: Inventory** - No change. Enumerate specs matching corpus pattern.

**Phase 2: Contract Alignment** - Instead of checking spec↔code, check spec↔contract:

```
FOR each spec IN corpus:
    FOR each requirement IN contract:
        IF spec MISSING requirement:
            RECORD gap: MISSING_REQUIREMENT
        ELIF spec PARTIAL requirement:
            RECORD gap: INCOMPLETE_REQUIREMENT
        ELSE:
            RECORD: COMPLIANT
```

**Phase 3: Cross-Spec Analysis** - Check consistency across specs:
- Do all specs use the same category enum values?
- Are output formats consistent?
- Are there conflicting definitions?

**Phase 4: Recommendations** - Generate compliance gaps as recommendations:

```
RECOMMENDATION: CONTRACT_COMPLIANCE
  Target: memory_audit_workflow.md
  Gap: Missing required section "Output Artifacts" per workflow_manager.md Section 13.2
  Action: Add section with actionable_findings.json output
  Severity: HIGH (blocks Manager integration)
```

**Phase 5: Execution** - Route to spec_refinement_workflow or apply inline if simple.

#### 2.7.4 Contract Specification Format

A contract spec defines requirements that other specs must satisfy:

```markdown
## Contract: Workflow Output Requirements

### Required Sections
- [ ] "Output Artifacts" OR "Output Files" section
- [ ] Lists actionable_findings.json (unless exempt)

### Required Content
- [ ] FindingCategory enum with valid values
- [ ] Output location specification
- [ ] verify_command pattern

### Exemptions
- tui_testing_workflow (reports failures, doesn't prescribe fixes)
```

The contract can be:
1. **Inline in contract spec** - Requirements embedded in prose
2. **Structured YAML block** - Machine-parseable requirements
3. **Reference to schema** - Points to struct definition (e.g., ActionableFinding)

#### 2.7.5 Compliance Report Format

```markdown
## Contract Compliance Report

**Contract:** workflow_manager.md Section 13
**Corpus:** specs/meta/*_workflow.md
**Date:** 2026-01-14

### Summary

| Status | Count |
|--------|-------|
| COMPLIANT | 2 |
| PARTIAL | 2 |
| NON_COMPLIANT | 1 |
| EXEMPT | 1 |

### Per-Spec Status

#### memory_audit_workflow.md - PARTIAL

| Requirement | Status | Notes |
|-------------|--------|-------|
| Output section | MISSING | No "Output Artifacts" section |
| Category enum | COMPLIANT | Defines 6 categories |
| verify_command | COMPLIANT | Documents pattern |

**Gaps:**
- GAP-CONTRACT-001: Add Output Artifacts section listing actionable_findings.json

#### data_model_maintenance_workflow.md - PARTIAL

| Requirement | Status | Notes |
|-------------|--------|-------|
| Output section | PARTIAL | Has section but missing JSON output |
| Category enum | COMPLIANT | Defines 6 categories |
| verify_command | MISSING | No verification pattern |

**Gaps:**
- GAP-CONTRACT-002: Add actionable_findings.json to output
- GAP-CONTRACT-003: Document verify_command pattern

#### tui_testing_workflow.md - EXEMPT

Exempt per contract: "reports failures, doesn't prescribe fixes"
```

#### 2.7.6 Integration with Workflow Manager

The Workflow Manager invokes Contract Compliance Mode:

1. **Periodic:** Weekly/monthly contract compliance check
2. **On-demand:** Before invoking a workflow, verify it's compliant
3. **After contract change:** When workflow_manager.md Section 13 changes

```
Manager detects contract version drift
        │
        ▼
Invoke spec_maintenance --mode contract_compliance
        │
        ▼
Compliance Report with gaps
        │
        ▼
Route gaps to spec_refinement (auto or manual)
        │
        ▼
Re-run compliance check (verify fix)
```

#### 2.7.7 Bootstrapping

**First-time setup:** This section (2.7) must be added to spec_maintenance_workflow.md before the Manager can use Contract Compliance Mode. After this bootstrap:

1. Manager can invoke `--mode contract_compliance`
2. Workflow specs get checked against Manager contract
3. Gaps surface automatically
4. System becomes self-healing

---

## 3. Audit Phases

### Phase 1: Inventory
Enumerate all specs and their metadata.

### Phase 2: Code Alignment
Check each spec against current codebase.

### Phase 3: Cross-Spec Analysis
Detect overlaps, gaps, naming convention violations, and organizational issues.

### Phase 4: Recommendations
Generate actionable cleanup tasks.

### Phase 5: Execution
Apply changes (with user approval).

---

## 4. Error Handling

### 4.1 Design Principle: Graceful Degradation

**One bad file should never block the entire audit.**

The maintenance workflow processes many files. Errors in individual files should be:
1. Logged with context
2. Skipped with warning
3. Aggregated in error report
4. Presented to user for manual resolution

### 4.2 Error Types

| Error Type | Scope | Severity | Example |
|------------|-------|----------|---------|
| **FILE_UNREADABLE** | Single file | LOW | Permission denied, corrupt encoding |
| **FILE_MALFORMED** | Single file | LOW | Binary file in specs/, broken markdown |
| **REFERENCE_BROKEN** | Single reference | MEDIUM | Link to non-existent file/anchor |
| **SEARCH_FAILURE** | Single query | MEDIUM | Grep/glob returns unexpected error |
| **CODE_UNREACHABLE** | Single alignment | MEDIUM | Referenced code path doesn't exist |
| **CIRCULAR_REFERENCE** | Graph subset | HIGH | Spec A refs B refs C refs A |
| **PHASE_TIMEOUT** | Entire phase | HIGH | Phase takes >5 minutes |
| **FILE_TOO_LARGE** | Single file | HIGH | Exceeds LLM token limit (~32K tokens / ~100KB) |
| **STORAGE_FAILURE** | Entire session | CRITICAL | Can't write to output directory |
| **GIT_ERROR** | Execution phase | CRITICAL | Git operation fails during execution |

### 4.3 Error Handling by Phase

#### Phase 1: Inventory Errors

| Error | Action | Continue? |
|-------|--------|-----------|
| FILE_UNREADABLE | Log path + error, mark as `STATUS: ERROR` in inventory | Yes |
| FILE_MALFORMED | Log path + error, mark as `STATUS: MALFORMED` in inventory | Yes |
| FILE_TOO_LARGE | Log path + size, mark as `STATUS: OVERSIZED`, auto-recommend SPLIT | Yes |
| Symlink loop | Detect via visited set, skip with warning | Yes |
| >1000 files | Warn user, suggest scope narrowing | Ask user |

**Error Record Format:**
```markdown
| File | Status | Error |
|------|--------|-------|
| specs/broken.md | ERROR | Permission denied |
| specs/binary.md | MALFORMED | Binary file detected |
```

#### Phase 2: Alignment Errors

| Error | Action | Continue? |
|-------|--------|-----------|
| CODE_UNREACHABLE | Mark alignment as `UNKNOWN`, log missing path | Yes |
| SEARCH_FAILURE | Retry once, then mark as `UNKNOWN` | Yes |
| Ambiguous match (>10 results) | Mark as `NEEDS_MANUAL_REVIEW` | Yes |
| No code found for spec | Valid result: classify as `SPEC_AHEAD` | Yes |

**Error Record Format:**
```markdown
### Alignment Errors

| Spec | Error | Details | Recommendation |
|------|-------|---------|----------------|
| specs/feature.md | CODE_UNREACHABLE | `crates/module/` not found | Verify path or archive spec |
| specs/api.md | AMBIGUOUS_MATCH | 15 matches for "authenticate" | Manual review needed |
```

#### Phase 3: Cross-Spec Errors

| Error | Action | Continue? |
|-------|--------|-----------|
| REFERENCE_BROKEN | Log broken ref, continue analysis | Yes |
| CIRCULAR_REFERENCE | Detect cycle, report all nodes in cycle, skip cycle | Yes |
| PHASE_TIMEOUT | Abort phase, report partial results | Partial |

**Circular Reference Detection:**
```
visited = set()
path = []

def detect_cycle(spec):
    if spec in path:
        report_cycle(path[path.index(spec):])
        return
    if spec in visited:
        return
    visited.add(spec)
    path.append(spec)
    for ref in spec.references:
        detect_cycle(ref)
    path.pop()
```

**Error Record Format:**
```markdown
### Cross-Spec Errors

#### Circular References
- **CYCLE-001:** specs/a.md -> specs/b.md -> specs/c.md -> specs/a.md
  - Recommendation: Break cycle at specs/b.md -> specs/c.md

#### Broken References
| Source | Target | Line | Type |
|--------|--------|------|------|
| specs/a.md | specs/deleted.md | 45 | FILE_MISSING |
| specs/b.md | specs/c.md#anchor | 23 | ANCHOR_MISSING |
```

#### Phase 4: Recommendation Errors

| Error | Action | Continue? |
|-------|--------|-----------|
| Conflicting recommendations | Present both options to user | Yes |
| Missing data for recommendation | Skip recommendation, note as incomplete | Yes |
| Priority calculation error | Default to MEDIUM priority | Yes |

#### Phase 5: Execution Errors

| Error | Action | Continue? |
|-------|--------|-----------|
| FILE_MOVE_FAILED | Log error, skip this action, continue others | Yes |
| MERGE_CONFLICT | Stop, present conflict to user | Ask user |
| GIT_ERROR | Stop execution phase entirely | No |
| PARTIAL_WRITE | Rollback file to backup, log error | Yes |
| RENAME_FAILED | Log error, add to deferred renames list | Yes |
| REFERENCE_UPDATE_FAILED | Log error, mark file for manual review | Yes |

**Critical:** Phase 5 should create backups before any file modification:
```
{session}/backups/
├── specs_extraction_md.backup    # Before modification
├── specs_pricing_md.backup
└── ...
```

### 4.4 Retry Protocol

| Failure Type | Retry Action | Max Retries |
|--------------|--------------|-------------|
| FILE_UNREADABLE | Wait 100ms, retry read | 1 |
| SEARCH_FAILURE | Simplify query, retry | 2 |
| PHASE_TIMEOUT | Split phase into smaller chunks | 1 |
| GIT_ERROR | None (manual intervention required) | 0 |

### 4.5 Error Aggregation

All errors are aggregated into `{session}/error_report.md`:

```markdown
## Error Report - 2026-01-14

### Summary
| Phase | Errors | Warnings | Skipped |
|-------|--------|----------|---------|
| Inventory | 2 | 1 | 3 |
| Alignment | 0 | 5 | 0 |
| Cross-Spec | 1 | 3 | 1 |
| Recommendations | 0 | 0 | 0 |
| Execution | - | - | - |

**Total files processed:** 45
**Files with errors:** 3 (6.7%)
**Audit completion:** 93.3%

### Detailed Errors
[Phase-specific error tables from above]

### User Action Required
1. **CRITICAL:** None
2. **HIGH:** Resolve CYCLE-001 (circular reference)
3. **MEDIUM:** Review 5 ambiguous alignments
4. **LOW:** Fix 3 broken references
```

### 4.6 Escalation Path

After errors are aggregated:

1. **If CRITICAL errors exist:** Stop, present to user immediately
2. **If HIGH errors exist:** Present summary, ask whether to continue
3. **If only MEDIUM/LOW:** Continue, include in final report

**User Options on HIGH Errors:**
```
Errors found during Phase 2 (Alignment):
- 1 circular reference detected
- 2 code paths unreachable

Options:
1. Continue with partial results (skip affected specs)
2. Pause for manual resolution
3. Abort maintenance session
```

### 4.7 Partial Results

When errors prevent complete analysis, clearly mark outputs:

```markdown
## Code Alignment Report (PARTIAL)

**Completion:** 85% (17/20 specs analyzed)

### Skipped Specs
| Spec | Reason |
|------|--------|
| specs/broken.md | FILE_UNREADABLE |
| specs/orphan.md | CODE_UNREACHABLE |
| specs/circular.md | CIRCULAR_REFERENCE |

### ALIGNED (12 specs)
...
```

### 4.8 Recovery from Phase Failures

If an entire phase fails (not just individual files):

| Phase | Recovery |
|-------|----------|
| Inventory | Cannot proceed. Fix file system issues first. |
| Alignment | Skip to Cross-Spec with warning. Alignment data will be incomplete. |
| Cross-Spec | Skip to Recommendations with warning. May miss overlaps/gaps. |
| Recommendations | Generate partial recommendations based on available data. |
| Execution | Rollback all changes, present partial log. |

### 4.9 Session Persistence for Recovery

To enable recovery from crashes/interrupts:

```
{session}/
├── checkpoint.json           # Last successful phase + position
├── inventory.md              # Phase 1 output (even if partial)
├── alignment_partial.md      # In-progress Phase 2
├── error_report.md           # Accumulated errors
└── backups/                  # File backups for rollback
```

**Checkpoint Format:**
```json
{
  "session_id": "2026-01-14-maintenance",
  "last_complete_phase": 2,
  "current_phase": 3,
  "current_position": "spec_15_of_20",
  "errors_so_far": 3,
  "timestamp": "2026-01-14T10:30:00Z"
}
```

On resume:
1. Read checkpoint.json
2. Skip completed phases
3. Resume current phase from last position
4. Continue normally

---

## 5. Decision Escalation

### 5.1 Decision Authority Matrix

| Decision Type | Agent Authority | Escalate When |
|---------------|-----------------|---------------|
| **ALIGNED classification** | Full | Never (low risk) |
| **SPEC_STALE classification** | Full if code diff is clear | Spec has recent commits or >3 sections affected |
| **SPEC_AHEAD classification** | Full if marked PLANNED | No explicit PLANNED marker |
| **CODE_UNDOCUMENTED** | Full | Feature is customer-facing |
| **ABANDONED classification** | Never autonomous | Always escalate |
| **Overlap detection** | Full | Similarity >60% (merge candidate) |
| **Gap identification** | Full | Never (informational only) |
| **Archive recommendation** | Never autonomous | Always escalate |
| **Merge recommendation** | Propose | Always get confirmation |
| **Priority assignment** | Propose | User may reorder |

### 5.2 Escalation Triggers

Escalate immediately when:
1. **Classification confidence <70%** - Present evidence for user judgment
2. **UNKNOWN status** - Cannot determine with available information
3. **Destructive action** - Any Archive, Delete, or Merge recommendation
4. **Business context required** - Spec references strategy, roadmap, or market decisions
5. **Conflicting signals** - Recent spec update but code unchanged, or vice versa

### 5.3 Phase-Specific Decision Points

#### Phase 2: Code Alignment - Decision Points

After alignment classification:

**Auto-proceed (no user input):**
- ALIGNED: No divergence, continue
- SPEC_STALE with clear code diff: Agent proposes update scope

**Checkpoint:**
```markdown
## Alignment Classification Review

### Confident Classifications (12)
[ALIGNED and clear SPEC_STALE cases]

### Needs Your Input (3)

#### ALIGNMENT-001: specs/extraction.md
- **Agent assessment:** SPEC_STALE (60% confidence)
- **Evidence:**
  - Spec describes 4-step wizard (line 45-89)
  - Code has 3 steps (extraction/wizard.rs:28-45)
- **Uncertainty:** Last spec commit was 2 weeks ago, might be intentional redesign
- **Options:**
  1. **SPEC_STALE** - Update spec to match code (agent's recommendation)
  2. **CODE_BUGGY** - Code should have 4 steps (file bug)
  3. **SPEC_AHEAD** - 4 steps is future state, mark PLANNED

[User selects option]
```

#### Phase 3: Cross-Spec Analysis - Decision Points

**Auto-proceed:**
- Fragmentation <50% overlap: Note for later
- Gaps with existing CLAUDE.md coverage: No action needed

**Checkpoint:**
```markdown
## Cross-Spec Analysis Review

### Merges Requiring Decision

#### MERGE-001: specs/pricing.md + specs/pricing_v2_refined.md
- **Overlap:** 65%
- **Agent recommendation:** Merge into specs/pricing.md
- **Risk:** Some v2 content may be intentionally different
- **Options:**
  1. **Merge** - Combine, archive v2 (agent's recommendation)
  2. **Keep separate** - Different purposes, add cross-references
  3. **Review content** - Show detailed diff before deciding

### Orphans Requiring Decision

#### ORPHAN-001: specs/ai_wizards.md
- **No inbound references**
- **Last modified:** 90 days ago
- **Agent assessment:** Cannot determine relevance
- **Options:**
  1. **Archive** - No longer relevant
  2. **Integrate** - Add references from related specs
  3. **Defer** - Keep, revisit next quarter
```

#### Phase 4: Recommendations - Decision Points

Present prioritized recommendations for reordering:

```markdown
## Recommendation Review

### Proposed Order

| Priority | Rec ID | Type | Effort | Agent Confidence |
|----------|--------|------|--------|------------------|
| 1 | REC-001 | ARCHIVE | Trivial | HIGH |
| 2 | REC-002 | UPDATE | Small | HIGH |
| 3 | REC-003 | MERGE | Medium | MEDIUM |
| 4 | REC-004 | WRITE | Large | LOW |

**Adjust priority?** [y/n]
**Remove any recommendations?** [list IDs to skip]
**Add dependencies?** [e.g., "REC-003 before REC-002"]
```

### 5.4 Decision Recording

All user decisions recorded in maintenance session:

```markdown
## decisions.md

### Phase 2 Decisions

#### ALIGNMENT-001: specs/extraction.md
**Presented:** 2026-01-14T10:30:00Z
**Options:** SPEC_STALE, CODE_BUGGY, SPEC_AHEAD
**Selected:** SPEC_STALE
**Rationale:** "We simplified the wizard intentionally"
**Confidence override:** Agent 60% -> User confirmed

### Phase 3 Decisions

#### MERGE-001: pricing specs
**Presented:** 2026-01-14T11:00:00Z
**Options:** Merge, Keep separate, Review content
**Selected:** Review content
**Follow-up:** User reviewed diff, then selected Merge
**Rationale:** "v2 was draft, v1 is canonical"

### Phase 4 Decisions

#### PRIORITY-001: Recommendation reorder
**Original:** REC-001, REC-002, REC-003, REC-004
**User order:** REC-001, REC-003, REC-002 (skipped REC-004)
**Rationale:** "Merge before update to avoid duplicate work"
```

### 5.5 Confidence Scoring

Agent assessments include confidence levels:

| Level | Range | Meaning | Action |
|-------|-------|---------|--------|
| HIGH | 85-100% | Clear evidence, no ambiguity | Auto-proceed |
| MEDIUM | 60-84% | Some uncertainty | Present to user with recommendation |
| LOW | <60% | Insufficient evidence | Require user decision |

**Confidence factors:**
- Code diff clarity (+20% if clear single-location change)
- Spec recency (-10% if modified in last 30 days)
- Cross-references (+10% if other specs corroborate)
- Explicit markers (+30% if spec has PLANNED/DEPRECATED marker)

### 5.6 Blocked Assessments

When agent cannot determine classification:

```markdown
## Blocked Assessments

### BLOCKED-001: specs/domain_intelligence.md
**Reason:** USER_INPUT required
**Blocking question:** Is domain intelligence feature still in roadmap?
**Impact:** Cannot classify as ABANDONED vs SPEC_AHEAD
**Since:** 2026-01-14T10:45:00Z

User may:
1. **Provide context** - Answer question, unblock
2. **Defer** - Skip this spec, continue with others
3. **Force classify** - Accept risk of incorrect classification
```

---

## 6. Phase 1: Spec Inventory

### 6.1 Scan Locations

```
specs/                    # Primary specs
specs/views/              # TUI view specs
specs/meta/               # Meta-specs (like this one)
strategies/               # Business strategy docs
docs/                     # Technical docs
CLAUDE.md                 # Root architecture doc
ARCHITECTURE.md           # System design
STRATEGY.md               # Master strategy document
```

### 6.2 Inventory Output

```markdown
## Spec Inventory

| File | Status | Last Modified | LOC | References |
|------|--------|---------------|-----|------------|
| specs/tui.md | ACTIVE | 2026-01-10 | 450 | 5 inbound, 3 outbound |
| specs/extraction.md | ACTIVE | 2026-01-08 | 320 | 2 inbound, 4 outbound |
| specs/hl7_parser.md | PLANNED | 2026-01-05 | 180 | 0 inbound, 1 outbound |
| specs/old_feature.md | STALE? | 2025-11-02 | 95 | 0 inbound, 0 outbound |

## Strategy Inventory

| File | Status | Last Modified | LOC | Related Spec |
|------|--------|---------------|-----|--------------|
| strategies/healthcare_hl7.md | ACTIVE | 2026-01-08 | 486 | specs/hl7_parser.md |
| strategies/defense_tactical.md | ACTIVE | 2026-01-08 | 783 | (future) |
| strategies/finance.md | ACTIVE | 2026-01-08 | 600 | (none yet) |
```

### 6.3 Status Classification

| Status | Definition |
|--------|------------|
| **ACTIVE** | Spec describes current or near-term features |
| **PLANNED** | Explicitly marked as future work |
| **IMPLEMENTED** | Feature shipped, spec may be archivable |
| **STALE** | Not updated in 90+ days, may be outdated |
| **ORPHAN** | No references to/from other docs |
| **UNKNOWN** | Needs manual classification |

### 6.4 Strategy Document Handling

Strategy documents (`strategies/*.md`) describe business rationale, market positioning, and go-to-market approaches. They are tracked separately from technical specs due to different validation methods.

#### 6.4.1 Strategy Scan Locations

```
strategies/               # Market vertical strategies
STRATEGY.md              # Master strategy document
```

#### 6.4.2 Strategy-Specific Status

| Status | Definition |
|--------|------------|
| **ACTIVE** | Currently guides decisions; market conditions valid |
| **VALIDATED** | Hypotheses confirmed by market feedback |
| **INVALIDATED** | Hypotheses disproven; needs revision or archive |
| **OUTDATED** | Market conditions changed (regulatory, competitive) |
| **SPECULATIVE** | Untested hypothesis; explicitly marked |

#### 6.4.3 Strategy Staleness Signals

A strategy is likely stale if:
- Last modified > 90 days without explicit "no changes needed" note
- References regulatory deadlines that have passed
- Contains unvalidated pricing assumptions > 6 months old
- "Open Questions" unresolved > 6 months
- Referenced competitor has pivoted or exited market

---

## 7. Phase 2: Code Alignment Audit

### 7.1 Alignment Categories

For each spec, determine its relationship to the codebase:

| Category | Code State | Spec State | Action |
|----------|------------|------------|--------|
| **ALIGNED** | Implements spec | Matches code | None |
| **SPEC_STALE** | Code evolved | Spec outdated | Update spec |
| **SPEC_AHEAD** | Not implemented | Describes future | Mark PLANNED |
| **CODE_UNDOCUMENTED** | Feature exists | No spec | Write spec or note in CLAUDE.md |
| **ABANDONED** | Not implemented | Unlikely to be | Archive spec |

### 7.2 Strategy Alignment Categories

Strategies align to market conditions, not code:

| Category | Market State | Strategy State | Action |
|----------|--------------|----------------|--------|
| **MARKET_ALIGNED** | Conditions match assumptions | Active | None |
| **MARKET_SHIFTED** | Conditions changed | Outdated | Revise strategy |
| **HYPOTHESIS_INVALIDATED** | Research disproves | Claims validity | Archive or revise |
| **STRATEGY_AHEAD** | Market not ready | Speculative | Mark SPECULATIVE |

### 7.3 Alignment Check Process

For each spec:

1. **Extract key claims** - What behavior does the spec describe?
2. **Search codebase** - Does code exist that implements this?
3. **Compare** - Do spec and code match?
4. **Classify** - Which alignment category?
5. **Assess confidence** - How certain is the classification? (HIGH/MEDIUM/LOW)
6. **Checkpoint** - If MEDIUM or LOW confidence, queue for user decision

### 7.4 Alignment Report Format

```markdown
## Code Alignment Report

### ALIGNED (12 specs)
- specs/tui.md - TUI architecture matches crates/casparian/src/cli/tui/
- specs/pricing.md - Pricing tiers match billing module
- ...

### SPEC_STALE (3 specs)
- **specs/extraction.md**
  - Spec says: Wizard has 4 steps (line 45)
  - Code has: 3 steps (extraction/wizard.rs:28)
  - Recommendation: Update spec Section 3.2

### SPEC_AHEAD (2 specs)
- **specs/hl7_parser.md** - HL7 parser not yet implemented
  - Recommendation: Mark as PLANNED, no action needed

### ABANDONED (1 spec)
- **specs/old_api.md** - Describes deprecated REST API
  - Last commit to related code: 8 months ago
  - Recommendation: Archive to archive/specs/
```

---

## 8. Phase 3: Cross-Spec Analysis

### 8.1 Overlap Detection

Overlap detection identifies specs describing the same concepts, which are candidates for merging or boundary refactoring.

#### 8.1.1 Overlap Types and Detection Signals

Specs can overlap in multiple dimensions. Each is detected by a weighted signal:

| Type | Weight | Detection Method | Example |
|------|--------|------------------|---------|
| **Structural** | 20% | Heading similarity (Jaccard) | Both have "Error Handling" and "User Workflows" sections |
| **Conceptual** | 40% | Key term extraction + overlap | Both discuss "tagging rules" extensively |
| **Content** | 30% | Representative sentence matching | Copy-pasted sections |
| **Reference** | 10% | Bidirectional reference density | Circular dependency smell |

**Key insight:** Conceptual overlap is most important. Two specs with different structures but describing the same feature should be flagged for merging.

#### 8.1.2 Signal Calculations

**Signal 1: Section Heading Similarity (Structural)**
```
1. Extract all markdown headings (# through ####)
2. Normalize: lowercase, remove punctuation, strip numbers
3. Calculate Jaccard similarity: |A ∩ B| / |A ∪ B|
4. Weight by heading level (# = 1.0, ## = 0.8, ### = 0.6, #### = 0.4)
```

**Signal 2: Key Term Extraction (Conceptual)**
```
1. Extract candidate terms with weights:
   - Words in headings: 3x
   - Words in bold/italic: 2x
   - Words in code blocks/backticks: 2x
   - Capitalized phrases: 1.5x
   - High-frequency nouns: 1x
2. Filter stopwords and common tech terms
3. Take top 20 terms per spec
4. Calculate: term_overlap = |terms_A ∩ terms_B| / min(|terms_A|, |terms_B|)
```

**Why min() not union:** If a small spec is entirely contained in a large spec's topic, that's high overlap even if the large spec covers more.

**Signal 3: Content Fingerprinting**
```
1. Extract first sentence from each section (representative sentences)
2. Check for exact or near-exact matches between specs
3. Calculate: content_overlap = matching_sentences / total_sentences
```

**Signal 4: Reference Density**
```
1. Count references from A to B: refs_A_to_B
2. Count references from B to A: refs_B_to_A
3. Calculate: ref_density = (refs_A_to_B + refs_B_to_A) / max(total_refs_A, total_refs_B, 1)
```

#### 8.1.3 Scoring Formula

```
overlap_score = (
    heading_similarity * 0.20 +
    term_overlap * 0.40 +
    content_overlap * 0.30 +
    ref_density * 0.10
) * 100
```

**Result:** 0-100%

**Weighting rationale:**
- Term overlap (40%): Most indicative of same domain
- Content overlap (30%): Direct evidence of duplication
- Heading similarity (20%): Structural similarity less important than content
- Reference density (10%): Secondary signal, can be intentional

#### 8.1.4 Thresholds and Classifications

| Score Range | Classification | Action |
|-------------|----------------|--------|
| **0-30%** | DISTINCT | No action; specs cover different topics |
| **30-50%** | RELATED | Add cross-references if missing; note in report |
| **50-70%** | OVERLAPPING | Flag for review; recommend refactoring boundaries |
| **70-90%** | MERGE_CANDIDATE | Strongly recommend merging; **checkpoint** |
| **90-100%** | DUPLICATE | Recommend archive one; **checkpoint** |

#### 8.1.5 Confidence Modifiers

Adjust score based on context:

| Factor | Adjustment | Reason |
|--------|------------|--------|
| Same directory (e.g., both in `specs/views/`) | -10% | Expected to share vocabulary |
| Parent/child relationship | -20% | Expected structural similarity |
| Same author/date | +10% | Possible accidental duplication |
| One spec >3x larger | -10% | Subset overlap is expected |

#### 8.1.6 Output Format

```markdown
## Overlap Analysis

### Summary

| Pair Count | Classification |
|------------|----------------|
| 12 | DISTINCT |
| 5 | RELATED |
| 2 | OVERLAPPING |
| 1 | MERGE_CANDIDATE |
| 0 | DUPLICATE |

### MERGE_CANDIDATE (1 pair)

#### OVERLAP-001: specs/pricing.md <-> specs/pricing_v2_refined.md

**Overall Score:** 72%
**Classification:** MERGE_CANDIDATE

| Signal | Score | Evidence |
|--------|-------|----------|
| Heading Similarity | 85% | 6/7 headings match |
| Term Overlap | 68% | Shared: pricing, tier, discount, plan, subscription |
| Content Overlap | 75% | 3 paragraphs near-identical |
| Reference Density | 60% | 4 bidirectional refs |

**Recommendation:** Merge into `specs/pricing.md`
**Rationale:** pricing_v2_refined appears to be an iteration; content should be consolidated
**Risk:** Some v2 content may represent planned changes

---

### OVERLAPPING (2 pairs)

#### OVERLAP-002: specs/tui.md <-> specs/views/discover.md

**Overall Score:** 55%
**Classification:** OVERLAPPING

| Signal | Score | Evidence |
|--------|-------|----------|
| Heading Similarity | 40% | Some shared sections |
| Term Overlap | 65% | Shared: keybinding, panel, navigation, source |
| Content Overlap | 45% | Dialog pattern duplicated |
| Reference Density | 70% | Heavy cross-referencing |

**Recommendation:** Refactor boundaries
**Rationale:** tui.md defines global patterns; discover.md should reference, not duplicate
**Action:** Move duplicate dialog pattern to tui.md Section 4 (Patterns)
```

#### 8.1.7 LLM Execution Protocol

```
OVERLAP DETECTION PROCEDURE

Input: List of spec file paths from Phase 1 inventory

Step 1: Build Comparison Set
  - Filter out: meta specs, archived specs, strategy docs
  - Create all unique pairs: N specs → N*(N-1)/2 pairs
  - For large corpora (>50 specs): batch into groups of 20

Step 2: For Each Pair (A, B)
  2a. Heading Analysis - list headings, calculate Jaccard
  2b. Term Extraction - extract emphasized terms, calculate overlap
  2c. Content Check - extract representative sentences, check matches
  2d. Reference Check - use reference_graph.json from Phase 1
  2e. Aggregate - apply formula from Section 8.1.3
  2f. Classify - apply thresholds from Section 8.1.4

Step 3: Generate Report
  - Sort pairs by score descending
  - Group by classification
  - Format per Section 8.1.6
```

**Parallelization:** Overlap detection is embarrassingly parallel at the pair level. Each worker analyzes one pair independently, results are merged at the end.

### 8.2 Gap Detection

Identify missing specs for implemented features:

```markdown
## Gap Analysis

### GAP-001: Scout module undocumented
- **Code location:** crates/casparian_scout/
- **CLAUDE.md mention:** Brief (10 lines)
- **Dedicated spec:** None
- **Recommendation:** Write specs/scout.md or expand CLAUDE.md

### GAP-002: Bridge mode execution
- **Code location:** crates/casparian_worker/src/bridge.rs
- **Spec coverage:** spec.md mentions briefly
- **Recommendation:** Adequate for now, revisit if complexity grows
```

### 8.3 Fragmentation Detection

Identify specs that should be merged:

```markdown
## Fragmentation Analysis

### FRAG-001: Pricing specs
- **specs/pricing.md** (180 lines)
- **specs/pricing_v2_refined.md** (220 lines)
- **Overlap:** 60%
- **Recommendation:** Merge into single pricing.md, archive v2_refined

### FRAG-002: View specs
- **specs/views/discover.md** (400 lines)
- **specs/views/extraction_rules.md** (350 lines)
- **specs/views/jobs.md** (280 lines)
- **Recommendation:** Keep separate (distinct domains)
```

### 8.4 Bloat Detection

Identify specs that should be split. **Critical consideration: LLM token limits.**

#### 8.4.1 Size Thresholds

| Threshold | Lines | ~Tokens | ~KB | Action |
|-----------|-------|---------|-----|--------|
| **Comfortable** | < 500 | < 8K | < 25KB | No action |
| **Large** | 500-1000 | 8-16K | 25-50KB | Consider splitting |
| **Oversized** | 1000-2000 | 16-32K | 50-100KB | Recommend splitting |
| **Unreadable** | > 2000 | > 32K | > 100KB | **Must split** (exceeds typical LLM context) |

**Why this matters:** If an LLM cannot read a spec in a single context window, it cannot:
- Fully understand the spec during refinement
- Check alignment against code comprehensively
- Detect internal contradictions

#### 8.4.2 Splitting Strategies

When a spec exceeds the Oversized threshold:

| Pattern | When to Use | Example |
|---------|-------------|---------|
| **By Feature** | Spec covers multiple distinct features | spec.md → specs/jobs.md + specs/parsing.md + specs/tui.md |
| **By Audience** | Mixed technical depth | spec.md → specs/overview.md + specs/implementation.md |
| **By Phase** | Phased implementation | spec.md → specs/phase1.md + specs/phase2.md |
| **By Component** | Modular architecture | tui.md → views/discover.md + views/jobs.md |

#### 8.4.3 Splitting Rules

1. **Each child spec should be self-contained** - Readable without requiring parent
2. **Parent becomes index** - Links to children, provides overview
3. **Preserve cross-references** - Update links in other specs
4. **No circular splits** - Children shouldn't reference each other heavily

#### 8.4.4 Bloat Analysis Output

```markdown
## Bloat Analysis

### BLOAT-001: spec.md exceeds token limit (MUST SPLIT)
- **Current size:** 2,400 lines (~38K tokens)
- **Distinct sections:** 15
- **Status:** UNREADABLE by LLM in single pass
- **Recommendation:** Split by feature into:
  - specs/overview.md (Sections 1-3)
  - specs/jobs_lifecycle.md (Sections 4-6, FS-5)
  - specs/parsing.md (Sections 7-9)
  - specs/tui.md (Sections 10-12)
  - specs/error_handling.md (Sections 13-15)

### BLOAT-002: specs/tui.md is large (CONSIDER SPLIT)
- **Current size:** 800 lines (~12K tokens)
- **Distinct sections:** 8
- **Status:** Readable but approaching limit
- **Recommendation:** Monitor; split if grows further
```

### 8.5 Reference Health

Check cross-references are valid:

```markdown
## Reference Health

### Broken References
- specs/extraction.md:45 → specs/discover.md#file-selection (anchor doesn't exist)
- specs/tui.md:120 → specs/views/parser_bench.md (file doesn't exist)

### Orphan Specs (no inbound references)
- specs/ai_wizards.md - Not referenced from any other doc
- specs/domain_intelligence.md - Not referenced from any other doc

### Missing References (should link but don't)
- specs/extraction.md mentions "tagging rules" but doesn't link to specs/views/discover.md
```

### 8.6 Spec-Strategy Cross-References

Technical specs and market strategies should reference each other:

**Expected Headers:**

Strategy documents:
```markdown
**Related Spec:** [specs/xxx.md](../specs/xxx.md)
# or
**Related Spec:** (future) specs/xxx.md
# or
**Related Spec:** N/A (business strategy only)
```

Spec documents (for market-facing features):
```markdown
**Market Strategy:** [strategies/xxx.md](../strategies/xxx.md)
# or (if no strategy needed)
**Market Strategy:** N/A (internal infrastructure)
```

**Cross-Reference Report Format:**

```markdown
## Cross-Reference Analysis

### Complete Bidirectional Links (OK)
- strategies/healthcare_hl7.md <-> specs/hl7_parser.md

### Strategies Without Specs
- strategies/finance.md - No spec exists (GAP: needs specs/fix_parser.md)

### Specs Without Strategies (Review Needed)
- specs/tui.md - Internal infrastructure (EXPECTED)
- specs/domain_intelligence.md - Market-facing? (NEEDS REVIEW)
```

### 8.7 Reference Propagation

When specs are modified, split, merged, or archived, references must be updated across the corpus. This requires building and traversing the reference graph.

#### 8.7.1 Reference Graph Construction

Build a directed graph of spec references during Phase 1:

```
┌─────────────────────────────────────────────────────────────┐
│                    REFERENCE GRAPH                          │
│                                                             │
│    CLAUDE.md ──────► specs/tui.md ──────► specs/views/*.md  │
│        │                  │                     │           │
│        │                  ▼                     ▼           │
│        └──────────► specs/scout.md ◄──── specs/discover.md  │
│                          │                                  │
│                          ▼                                  │
│                    strategies/healthcare.md                 │
└─────────────────────────────────────────────────────────────┘
```

**Graph data structure:**
```
{
  "specs/tui.md": {
    "references_out": ["specs/views/discover.md", "specs/views/jobs.md"],
    "references_in": ["CLAUDE.md", "specs/extraction.md"],
    "parent": null,
    "children": ["specs/views/discover.md", "specs/views/jobs.md"],
    "headings": [
      {"level": 1, "text": "TUI Specification", "slug": "tui-specification", "line": 1},
      {"level": 2, "text": "Layout", "slug": "layout", "line": 45}
    ],
    "parse_warnings": []
  },
  "specs/views/discover.md": {
    "references_out": ["specs/scout.md"],
    "references_in": ["specs/tui.md"],
    "parent": "specs/tui.md",
    "children": [],
    "headings": [...],
    "parse_warnings": []
  }
}
```

#### 8.7.1.1 Reference Parsing Specification

Parsing markdown references involves three phases: exclusion zone identification, link extraction, and path resolution.

**Phase 1: Exclusion Zones**

Before parsing links, identify content regions where links should be ignored:

| Zone Type | Start Pattern | End Pattern | Nested? |
|-----------|---------------|-------------|---------|
| Fenced code block | `` ``` `` or `~~~` at line start | Same delimiter | No |
| Indented code block | 4+ spaces/tab at line start (after blank line) | Non-indented line | No |
| Inline code | Single backtick `` ` `` | Single backtick | No |
| HTML comments | `<!--` | `-->` | No |

**Exclusion detection state machine:**
```
state = NORMAL
for each line in file:
    if state == NORMAL:
        if line matches /^```/ or /^~~~/: state = FENCED_BLOCK
        else if line is blank: state = MAYBE_INDENTED
        else: mark line as PARSEABLE
    elif state == FENCED_BLOCK:
        if line starts with remembered delimiter: state = NORMAL
        # line is in exclusion zone
    elif state == MAYBE_INDENTED:
        if line starts with 4+ spaces or tab: state = INDENTED_BLOCK
        else: state = NORMAL, mark line as PARSEABLE
    elif state == INDENTED_BLOCK:
        if line starts with 4+ spaces/tab or is blank: continue
        else: state = NORMAL, mark line as PARSEABLE
```

For PARSEABLE lines, mask inline code before link extraction:
```
masked_line = line.replace(/`[^`]+`/g, "XXXXXXXX")
# Parse links from masked_line
```

**Phase 2: Link Extraction**

| Link Type | Pattern | Example |
|-----------|---------|---------|
| Inline | `[text](path)` | `[TUI spec](specs/tui.md)` |
| Reference | `[text][ref]` + `[ref]: path` | `[see here][1]` with `[1]: ./doc.md` |
| Anchor-only | `[text](#anchor)` | `[Section 5](#section-5)` |
| Image | `![alt](path)` | `![diagram](images/arch.png)` |
| Related header | `**Related:** path1, path2` | `**Related:** specs/extraction.md, specs/views/sources.md` |

**Regex patterns:**
```
Inline:         /\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g
Reference link: /\[([^\]]+)\]\[([^\]]*)\]/g
Reference def:  /^\[([^\]]+)\]:\s*(\S+)/gm
Related header: /^\*\*Related:\*\*\s*(.+)$/gm  # Then split on comma, strip annotations in ()
```

**Related header parsing:**

The `**Related:**` header uses a comma-separated format with optional annotations:
```markdown
**Related:** specs/extraction.md (Extraction API), specs/views/sources.md, specs/meta/sessions/ai_consolidation/design.md
```

Parsing steps:
1. Match the `**Related:**` line
2. Split on comma (`,`)
3. For each segment, strip parenthetical annotations: `specs/extraction.md (Extraction API)` → `specs/extraction.md`
4. Trim whitespace
5. Validate each path exists in corpus

**Link extraction output:**
```json
{
  "source_file": "specs/tui.md",
  "links": [
    {"line": 45, "text": "discover spec", "raw_path": "../views/discover.md#file-selection", "type": "inline"},
    {"line": 120, "text": "see architecture", "raw_path": "ARCHITECTURE.md", "type": "reference", "ref_id": "arch"}
  ]
}
```

**Phase 3: Path Resolution**

1. Split path on `#` to separate file path and anchor
2. Resolve relative paths from source file's directory
3. Normalize `../` and `./` references
4. Convert to absolute corpus path (relative to repo root)

**Resolution table:**

| Source | Raw Path | Resolved |
|--------|----------|----------|
| `specs/tui.md` | `../CLAUDE.md` | `CLAUDE.md` |
| `specs/tui.md` | `./views/discover.md` | `specs/views/discover.md` |
| `specs/tui.md` | `#layout` | `specs/tui.md#layout` |
| `CLAUDE.md` | `specs/tui.md#overview` | `specs/tui.md#overview` |

**Anchor slug generation (GitHub-Flavored Markdown):**

See Section 8.7.4.1 for the canonical anchor slug generation algorithm.

**LLM Execution Notes:**

- When uncertain, include the link and flag for manual review
- Report parse warnings separately from broken references
- Prefer false positives (extra links) over false negatives (missed links)
- Store line numbers for all references to enable precise updates
- Reference definitions must be collected from the entire file before resolving `[text][ref]` links

**Validation pass:**

After extraction, validate references:
```
for each reference:
    if target_file not in corpus_files:
        mark as BROKEN (FILE_NOT_FOUND)
    if anchor exists and anchor not in target_file_headings:
        mark as BROKEN (ANCHOR_NOT_FOUND)
```

#### 8.7.2 Parent/Child Relationships

Specs can have explicit hierarchical relationships:

| Relationship | Detection Method | Example |
|--------------|------------------|---------|
| **Explicit parent** | `Parent:` header in child spec | `specs/views/discover.md` → `specs/tui.md` |
| **Directory hierarchy** | Subdirectory structure | `specs/views/*.md` children of `specs/` |
| **Index spec** | Spec that primarily links to others | `spec.md` as parent of `specs/*.md` |

**Parent detection heuristics:**
1. Check for `Parent:` or `Related:` header field
2. Check if spec is in subdirectory of another spec's directory
3. Check if another spec has >3 outbound links to this spec

#### 8.7.3 Cascade Update Rules

When a spec changes, propagate updates based on change type:

| Change Type | Affected Specs | Update Action |
|-------------|----------------|---------------|
| **RENAME** | All specs with inbound references | Replace old path with new path |
| **ARCHIVE** | All specs with inbound references | Remove link OR redirect to archive |
| **SPLIT** | All specs with inbound references | Update to reference correct child |
| **MERGE** | All specs referencing merged specs | Update to reference merged target |
| **DELETE** | All specs with inbound references | Remove link, warn user |

#### 8.7.4 Split Propagation (Most Complex)

When `specs/large.md` is split into children:

```
BEFORE:
  specs/other.md:42  →  [specs/large.md#section-5](specs/large.md#section-5)
  specs/another.md:8 →  [specs/large.md](specs/large.md)

AFTER SPLIT:
  specs/large.md (now index)
    └── specs/large/part1.md (sections 1-4)
    └── specs/large/part2.md (sections 5-8)  ← section-5 moved here
    └── specs/large/part3.md (sections 9-12)

REQUIRED UPDATES:
  specs/other.md:42  →  [specs/large/part2.md#section-5](specs/large/part2.md#section-5)
  specs/another.md:8 →  [specs/large.md](specs/large.md)  (keep, now points to index)
```

**Naming Convention for Split Children (Section 8.9 Integration):**

When splitting a spec, child file names MUST follow naming conventions:

| Anti-Pattern | Correct Pattern | Reason |
|--------------|-----------------|--------|
| `large_part1.md` | `large_overview.md` | NAME-004: Descriptive, not generic |
| `large_part2.md` | `large_jobs.md` | NAME-004: Names reflect content |
| `spec_v2_split1.md` | `jobs_lifecycle.md` | NAME-002: No version suffixes |

**Split dry-run should:**
1. Suggest descriptive names based on section content
2. Flag violations of NAME-001 through NAME-005
3. Require user approval for child file names

##### 8.7.4.1 Anchor Slug Generation (Canonical Algorithm)

GitHub-Flavored Markdown generates anchor slugs using these rules:

1. Convert to lowercase
2. Remove punctuation except hyphens, spaces, and underscores
3. Replace spaces with hyphens
4. Remove leading/trailing hyphens
5. For duplicates, append `-1`, `-2`, etc.

**Algorithm:**
```python
def generate_anchor_slug(title: str, existing_anchors: set[str]) -> str:
    slug = title.lower()
    slug = re.sub(r'[^\w\s-]', '', slug)  # Keep alphanumeric, spaces, hyphens
    slug = re.sub(r'[\s_]+', '-', slug)   # Spaces/underscores → hyphens
    slug = slug.strip('-')                 # Remove leading/trailing hyphens

    # Handle duplicates
    base_slug = slug
    counter = 1
    while slug in existing_anchors:
        slug = f"{base_slug}-{counter}"
        counter += 1

    existing_anchors.add(slug)
    return slug
```

**Examples:**

| Title | Generated Anchor |
|-------|------------------|
| `Section 5: Data Processing` | `section-5-data-processing` |
| `2.1 Overview` | `21-overview` |
| `What's New?` | `whats-new` |
| `Error Codes (FS-8)` | `error-codes-fs-8` |
| `Overview` (second occurrence) | `overview-1` |

##### 8.7.4.2 Section Boundary Detection

A **section** is a contiguous block of markdown starting with a heading and extending until:
- The next heading of equal or higher level (fewer `#` characters)
- End of file

**Detection algorithm:**
```python
def detect_sections(markdown_content: str) -> list[Section]:
    lines = markdown_content.split('\n')
    sections = []
    section_stack = []  # Stack of (level, section) for hierarchy
    current_section = None

    HEADING_PATTERN = r'^(#{1,6})\s+(.+)$'

    for line_num, line in enumerate(lines, start=1):
        match = re.match(HEADING_PATTERN, line)
        if match:
            # Close current section
            if current_section:
                current_section.end_line = line_num - 1
                sections.append(current_section)

            # Parse new heading
            hashes, title = match.groups()
            level = len(hashes)
            anchor = generate_anchor_slug(title, existing_anchors)

            # Find parent by popping higher/equal levels
            while section_stack and section_stack[-1][0] >= level:
                section_stack.pop()
            parent = section_stack[-1][1] if section_stack else None

            current_section = Section(level, title.strip(), anchor, line_num, parent)
            section_stack.append((level, current_section))

    # Close final section
    if current_section:
        current_section.end_line = len(lines)
        sections.append(current_section)

    return sections
```

##### 8.7.4.3 Section-to-Child Mapping Table

When splitting, create a mapping table tracking where each section moves:

```json
{
  "split_metadata": {
    "source_file": "specs/large.md",
    "split_timestamp": "2026-01-14T10:30:00Z",
    "split_strategy": "by_feature"
  },
  "children": [
    {
      "path": "specs/large/jobs.md",
      "sections": [
        {"anchor": "job-lifecycle", "source_line": 145},
        {"anchor": "job-states", "source_line": 200}
      ]
    },
    {
      "path": "specs/large/parsing.md",
      "sections": [
        {"anchor": "parser-interface", "source_line": 350},
        {"anchor": "parser-errors", "source_line": 510}
      ]
    }
  ],
  "anchor_redirects": {
    "job-lifecycle": "specs/large/jobs.md#job-lifecycle",
    "parser-interface": "specs/large/parsing.md#parser-interface"
  },
  "unmapped_anchors": []
}
```

##### 8.7.4.4 Reference Update Algorithm

```python
def update_references_after_split(corpus_files, split_mapping):
    updates = []
    source_file = split_mapping.source_file

    for file_path in corpus_files:
        if file_path == source_file:
            continue

        content = read_file(file_path)
        for match in find_links_to(content, source_file):
            anchor = match.anchor  # May be None

            if anchor is None:
                continue  # Keep pointing to parent (now index)

            anchor_slug = anchor[1:]  # Remove leading #
            if anchor_slug in split_mapping.anchor_redirects:
                new_target = split_mapping.anchor_redirects[anchor_slug]
                updates.append(ReferenceUpdate(
                    source_file=file_path,
                    line_number=match.line,
                    old_reference=f"{source_file}{anchor}",
                    new_reference=new_target
                ))
            else:
                updates.append(ReferenceUpdate(
                    source_file=file_path,
                    line_number=match.line,
                    old_reference=f"{source_file}{anchor}",
                    new_reference=None,
                    warning="ANCHOR_NOT_FOUND_IN_SPLIT_MAPPING"
                ))

    return updates
```

##### 8.7.4.5 Unmapped Anchor Resolution

When anchors can't be directly mapped:

| Cause | Resolution Strategy |
|-------|---------------------|
| **Inline content** (moved to subsection) | Map to parent section's child |
| **Deleted section** | Mark as ANCHOR_DELETED |
| **Anchor typo** | Fuzzy match (Levenshtein), suggest correction |
| **Generated anchor** (from HTML) | Preserve in parent index |

**Fuzzy matching:**
```python
def fuzzy_find_anchor(query: str, sections: list[Section], threshold=0.8):
    for section in sections:
        similarity = 1 - (levenshtein(query, section.anchor) /
                         max(len(query), len(section.anchor)))
        if similarity >= threshold:
            return section.anchor
    return None
```

##### 8.7.4.6 Anchor Stability Contract

**Principle:** External references should not break due to internal reorganization.

**Rules:**
1. When splitting, ALL anchors from source MUST be resolvable
2. Parent index SHOULD include redirect comments for major anchors
3. Child specs MUST preserve original anchor slugs (don't rename)

**Redirect comments in parent index:**
```markdown
# Large Spec

This spec has been split into focused documents.

## Quick Navigation

| Topic | Location |
|-------|----------|
| Job Lifecycle | [jobs.md](large/jobs.md#job-lifecycle) |
| Parser Interface | [parsing.md](large/parsing.md#parser-interface) |

<!-- ANCHOR REDIRECTS (for tooling)
#job-lifecycle -> large/jobs.md#job-lifecycle
#parser-interface -> large/parsing.md#parser-interface
-->
```

##### 8.7.4.7 Complete Split Execution

**Split propagation algorithm (detailed):**

```
Phase 1: Parse and Map
  1. Parse source spec into sections using detect_sections()
  2. Build split mapping table with anchor_redirects
  3. Scan corpus for inbound references to source file
  4. Preview reference updates (dry run)
  5. Report unmapped anchors with resolution suggestions

Phase 2: Get User Confirmation
  6. Present unmapped anchors requiring manual resolution
  7. User confirms anchor resolutions

Phase 3: Execute Split
  8. Create child files with preserved anchors
  9. Convert parent to index with redirect comments
  10. Apply reference updates to corpus
  11. Persist mapping table to session folder
  12. Log all changes in execution_log.md
```

**Implementation checklist for LLM execution:**

- [ ] Parse source spec into sections using `detect_sections()`
- [ ] Generate anchor slugs using GFM algorithm (Section 8.7.4.1)
- [ ] Build split mapping table before creating any files
- [ ] Scan corpus for inbound references to source file
- [ ] Preview all reference updates (dry run)
- [ ] Report unmapped anchors with resolution suggestions
- [ ] Get user confirmation for unmapped anchor resolutions
- [ ] Create child files with preserved anchors
- [ ] Convert parent to index with redirect comments
- [ ] Apply reference updates to corpus
- [ ] Persist mapping table to session folder (`splits/` directory)
- [ ] Log all changes in execution_log.md

#### 8.7.5 Traversal for Bulk Updates

When updating references, traverse in dependency order:

```
Topological traversal (leaves first):

Level 0 (no children):     specs/views/discover.md, specs/views/jobs.md
Level 1 (refs level 0):    specs/tui.md
Level 2 (refs level 1):    CLAUDE.md, spec.md

Update order: 0 → 1 → 2
```

**Why leaves first:** If a leaf is renamed/moved, its parent's reference updates before grandparent tries to traverse through parent.

#### 8.7.6 Reference Update Output

```markdown
## Reference Propagation Log

### Changes Triggering Updates
| Spec | Change | Trigger |
|------|--------|---------|
| specs/large.md | SPLIT | Exceeded token limit |

### References Updated
| Source | Line | Old Reference | New Reference |
|--------|------|---------------|---------------|
| specs/other.md | 42 | specs/large.md#section-5 | specs/large/part2.md#section-5 |
| CLAUDE.md | 156 | specs/large.md | specs/large.md (unchanged - now index) |
| specs/extraction.md | 23 | specs/large.md#api | specs/large/part1.md#api |

### Warnings
- specs/orphan.md:15 referenced specs/large.md#unknown-anchor - anchor not found in any child
```

#### 8.7.7 Bidirectional Sync

When updating references, maintain bidirectionality:

```
If specs/a.md references specs/b.md:
  - specs/a.md should have outbound link to b
  - specs/b.md SHOULD have "Referenced by" or inbound awareness

When b.md is renamed to b_new.md:
  1. Update a.md's link: b.md → b_new.md
  2. If b_new.md has "Referenced by" section, verify a.md is listed
```

### 8.8 Decision Checkpoints

After analysis, present to user for mandatory confirmation:

| Trigger | Classification | Required Action |
|---------|----------------|-----------------|
| Overlap score >70% | MERGE_CANDIDATE | Confirm merge or justify separation |
| Overlap score >90% | DUPLICATE | Confirm archive decision |
| Orphan spec >90 days | POTENTIAL_ORPHAN | Confirm archive or integration |
| Spec >1000 lines | OVERSIZED | Confirm split plan |
| Spec >2000 lines | UNREADABLE | **Must split** (exceeds LLM context) |
| Naming convention violation | RENAME_CANDIDATE | Confirm rename and reference updates |

**Checkpoint behavior:**
- MERGE_CANDIDATE/DUPLICATE: Present overlap analysis with per-signal breakdown
- POTENTIAL_ORPHAN: Show inbound reference analysis
- OVERSIZED/UNREADABLE: Present proposed split plan with section mapping preview
- RENAME_CANDIDATE: Show current name, proposed name, and files requiring reference updates
- BULK_RENAME: Show summary table with total impact (see format below)

**Bulk Rename Checkpoint Format:**

```markdown
### Bulk Rename: Directory Restructure

**Total Operations:** 5 renames
**Total Reference Updates:** 23 files

| # | Current Path | New Path | Refs to Update |
|---|--------------|----------|----------------|
| 1 | specs/views/discover.md | specs/tui/views/discover.md | 8 |
| 2 | specs/views/jobs.md | specs/tui/views/jobs.md | 5 |
| 3 | specs/views/parser_bench.md | specs/tui/views/parser_bench.md | 6 |
| 4 | specs/TUI-Spec.md | specs/tui.md | 3 |
| 5 | specs/pricing_v2.md | specs/pricing.md | 1 |

**Execution Order:** (based on dependency analysis)
1. Create directory: specs/tui/views/
2. Rename #1, #2, #3 (parallel - no dependencies)
3. Rename #4 (after views moved)
4. Rename #5 (independent)

Proceed with all renames? [y/n/select]
```

### 8.9 Naming Convention Enforcement

Consistent naming conventions improve discoverability, reduce confusion, and enable automation. This section defines the canonical naming rules and detection of violations.

#### 8.9.1 Naming Convention Rules

**File Naming Rules:**

| Rule ID | Rule | Pattern | Examples |
|---------|------|---------|----------|
| **NAME-001** | Lowercase with underscores | `[a-z][a-z0-9_]*\.md` | `job_lifecycle.md`, `tui_spec.md` |
| **NAME-002** | No version/state suffixes in active specs | Avoid `_v2`, `_v3`, `_refined`, `_redesign`, `_new`, `_old`, `_final` | ❌ `pricing_v2.md`, `jobs_redesign.md` → ✅ `pricing.md`, `jobs.md` |
| **NAME-003** | No date suffixes in active specs | Avoid `_2026`, `_jan` | ❌ `spec_2026.md` → ✅ `spec.md` |
| **NAME-004** | Descriptive names (2-4 words max) | Meaningful, not generic | ❌ `new.md`, `temp.md` → ✅ `parser_errors.md` |
| **NAME-005** | Match directory context | File name relates to parent dir | `views/discover.md` ✅, `views/pricing.md` ❌ |

**Directory Naming Rules:**

| Rule ID | Rule | Pattern | Examples |
|---------|------|---------|----------|
| **DIR-001** | Lowercase with underscores | `[a-z][a-z0-9_]*` | `views/`, `meta/` |
| **DIR-002** | Plural for collections | Contains multiple related items | `specs/`, `strategies/`, `views/` |
| **DIR-003** | Singular for namespaces | Groups by topic | `meta/`, `archive/` |

**Anchor/Heading Rules:**

| Rule ID | Rule | Pattern | Examples |
|---------|------|---------|----------|
| **ANCHOR-001** | Section numbers optional | Can include or omit | `## 3. Overview` or `## Overview` |
| **ANCHOR-002** | Consistent numbering within spec | If numbered, all siblings numbered | All `##` headers numbered or none |
| **ANCHOR-003** | No special chars in headings | Avoid `()`, `[]`, `{}` in titles | ❌ `## Error Codes (FS-8)` → ✅ `## Error Codes - FS-8` |

**Note on Anchor Handling:**

ANCHOR-003 applies to heading *text* only, not anchor links. GitHub-Flavored Markdown (GFM) automatically strips special characters when generating anchor slugs:
- `## Error Codes (FS-8)` → anchor: `#error-codes-fs-8`
- `## Error Codes - FS-8` → anchor: `#error-codes---fs-8`

**Implication:** Changing a heading to comply with ANCHOR-003 may change the anchor slug. Inbound links using the old anchor will break. When ANCHOR-003 violations are fixed:
1. Record the old anchor slug
2. Update inbound references to use new anchor (similar to file rename propagation)
3. Consider adding an invisible anchor for backwards compatibility: `<a id="old-anchor"></a>`

#### 8.9.2 Detection Algorithm

```
NAMING CONVENTION CHECK PROCEDURE

Input: List of spec file paths from Phase 1 inventory

Step 1: File Name Analysis
  For each file:
    1.1 Check NAME-001: lowercase_underscore pattern
    1.2 Check NAME-002: version/state suffix detection (_v\d+, _refined, _redesign, _new, _old, _final)
    1.3 Check NAME-003: date suffix detection (_\d{4}, _jan, _feb, etc.)
    1.4 Check NAME-004: generic name detection (new, temp, test, draft, copy)
    1.5 Check NAME-005: directory context match
        - Extract parent directory name
        - Look up expected content types for that directory (see table below)
        - Flag if filename doesn't match expected context

Step 1.6: NAME-005 Context Mapping

  | Directory | Expected File Content | Example Valid Names |
  |-----------|----------------------|---------------------|
  | views/    | TUI view specs | discover.md, jobs.md, parser_bench.md |
  | meta/     | Workflow/process specs | maintenance.md, refinement.md |
  | strategies/ | Market/GTM strategies | healthcare.md, enterprise.md |
  | sessions/ | Refinement session data | (any - session-specific) |
  | archive/  | Deprecated specs | (any - historical) |

  If directory not in mapping: Flag for manual review (don't auto-fail)

Step 2: Directory Analysis
  For each directory containing specs:
    2.1 Check DIR-001: lowercase pattern
    2.2 Check DIR-002/003: plural/singular appropriateness

Step 3: Anchor Consistency (per-spec)
  For each spec:
    3.1 Extract all headings
    3.2 Check ANCHOR-002: numbering consistency
    3.3 Check ANCHOR-003: special character usage

Step 4: Generate Violations Report
  - Group by rule ID
  - Include current name and suggested fix
  - Calculate severity based on inbound reference count
```

#### 8.9.3 Violation Severity

| Factor | Weight | Rationale |
|--------|--------|-----------|
| Inbound references | 40% | More refs = more files to update |
| File age | 20% | Older files likely more embedded |
| Rule type | 20% | NAME-002/003 more confusing than NAME-001 |
| Directory depth | 20% | Root-level more visible |

**Severity calculation:**
```
severity = (
    (inbound_refs / max_inbound_refs) * 0.4 +
    (file_age_days / 365) * 0.2 +
    rule_weight * 0.2 +
    (1 - depth / max_depth) * 0.2
) * 100

HIGH: >60%
MEDIUM: 30-60%
LOW: <30%
```

#### 8.9.4 Naming Violation Report Format

```markdown
## Naming Convention Analysis

### Summary

| Rule | Violations | Severity |
|------|------------|----------|
| NAME-001 | 2 | LOW |
| NAME-002 | 3 | HIGH |
| NAME-004 | 1 | MEDIUM |

### Violations

#### NAME-002: Version Suffix Violations (HIGH)

| Current | Proposed | Inbound Refs | Files to Update |
|---------|----------|--------------|-----------------|
| specs/pricing_v2_refined.md | specs/pricing.md | 4 | CLAUDE.md, spec.md, strategies/finance.md |
| specs/tui_v3.md | specs/tui.md | 8 | (see full list) |
| docs/api_v2.md | docs/api.md | 2 | specs/integration.md |

**Recommendation:** MERGE pricing_v2_refined.md into pricing.md (see OVERLAP-001)

#### NAME-001: Case/Format Violations (LOW)

| Current | Proposed | Inbound Refs |
|---------|----------|--------------|
| specs/JobLifecycle.md | specs/job_lifecycle.md | 1 |
| specs/TUI-Spec.md | specs/tui_spec.md | 2 |

#### NAME-004: Generic Names (MEDIUM)

| Current | Proposed | Context |
|---------|----------|---------|
| specs/new.md | specs/parser_validation.md | Contains parser validation content |
```

#### 8.9.5 Rename Execution Protocol

When a rename is approved:

```
RENAME EXECUTION PROCEDURE

Input: (old_path, new_path) pair

Step 1: Pre-flight Checks
  1.1 Verify old_path exists
  1.2 Verify new_path doesn't exist (avoid overwrite)
  1.3 Verify new_path follows naming conventions
  1.4 Build list of files with inbound references to old_path

Step 2: Dry Run Preview
  2.1 Calculate impact: 1 file rename + N reference updates
  2.2 List all affected files with line numbers
  2.3 Present to user: "Rename X affects Y files. Proceed? [y/n]"
  2.4 If declined, abort with no changes

Step 3: Create Backup
  3.1 Copy old_path to backups/{old_path_safe_name}.backup
  3.2 For each file with inbound references, create backup

Step 4: Execute Rename
  4.1 If in git repo:
      4.1.1 Run: git mv old_path new_path
      4.1.2 If fails (uncommitted changes): Ask user to commit/stash first
      4.1.3 If fails (index locked): Wait 100ms, retry once
      4.1.4 If still fails: Fall back to mv + git add new_path
  4.2 If not in git repo:
      4.2.1 Run: mv old_path new_path

Step 5: Propagate References (per Section 8.9.6)
  For each file with inbound reference:
    5.1 Read file content
    5.2 Normalize paths and replace all occurrences (see Section 8.9.6)
    5.3 Handle both relative and absolute references
    5.4 Write updated content
    5.5 Log change in reference_propagation.md
    5.6 If write fails: Add to failed_updates list, continue

Step 6: Validate
  6.1 Verify new_path exists
  6.2 Verify old_path doesn't exist
  6.3 Grep corpus for any remaining old_path references
  6.4 Report any missed references as warnings

Step 7: Log
  7.1 Record in execution_log.md
  7.2 Update reference_graph.json
  7.3 If any Step 5 failures: Trigger rollback decision (see 8.9.5.1)
```

##### 8.9.5.1 Rollback Protocol

If reference propagation (Step 5) fails partway through:

```
ROLLBACK DECISION

If failed_updates is non-empty after Step 5:

Option A: Continue with Warnings (DEFAULT)
  - Log all failed updates to error_report.md
  - Mark affected files for manual review
  - Complete rename, accept partial propagation
  - Present warning: "N files could not be updated"

Option B: Full Rollback (on user request)
  1. Restore renamed file:
     mv new_path old_path (or git mv)
  2. For each successfully updated file:
     Restore from backup OR re-update to use old_path
  3. Log rollback in execution_log.md
  4. Mark rename as FAILED in recommendations.md
  5. Present both states for manual resolution

ROLLBACK TRIGGERS (automatic):
  - >50% of reference updates failed
  - Target file itself failed to write
  - Git operation failed after file system changes
```

**Recovery from partial state:**

| State | Detection | Action |
|-------|-----------|--------|
| File renamed, no refs updated | old_path missing, refs point to old | Rollback file rename |
| File renamed, some refs updated | Mixed references | Complete remaining updates manually |
| File not renamed, backups exist | old_path exists, backups present | Clean up backups, retry |

#### 8.9.6 Reference Update Patterns

When updating references, handle all markdown link patterns:

| Pattern | Before | After |
|---------|--------|-------|
| Inline link | `[text](specs/old.md)` | `[text](specs/new.md)` |
| Inline with anchor | `[text](specs/old.md#section)` | `[text](specs/new.md#section)` |
| Reference link | `[text][ref]` + `[ref]: specs/old.md` | `[text][ref]` + `[ref]: specs/new.md` |
| Relative path | `[text](../old.md)` | `[text](../new.md)` |
| Parent reference | `Parent: specs/old.md` | `Parent: specs/new.md` |
| Related spec | `Related Spec: specs/old.md` | `Related Spec: specs/new.md` |

**Regex patterns for replacement:**
```python
# All patterns to search and replace
patterns = [
    # Standard markdown links
    r'\[([^\]]*)\]\(({old_path})(#[^)]+)?\)',
    # Reference definitions
    r'^\[([^\]]+)\]:\s*({old_path})(#\S+)?',
    # Header references (Parent:, Related:, etc.)
    r'^(Parent|Related Spec|Related|See):\s*\[?({old_path})\]?',
]

# Replacement preserves anchor if present
replacement = r'[\1]({new_path}\3)' # for link pattern
```

**Path Normalization Algorithm:**

When matching references, different files may use different path styles:

```python
def normalize_and_replace(source_file, old_path, new_path, content):
    """
    Normalize paths for matching while preserving original style.

    1. Convert all paths to repo-root-relative form for matching
    2. When replacing, calculate new relative path from source file's location
    3. Preserve original style (relative vs absolute)
    """
    # Step 1: Normalize old_path to repo-root-relative
    old_normalized = to_repo_relative(old_path)

    # Step 2: Find all references in content
    for match in find_references(content):
        ref_normalized = to_repo_relative(match, relative_to=source_file)

        if ref_normalized == old_normalized:
            # Step 3: Calculate replacement preserving style
            if is_relative(match):
                # Relative: calculate new relative path from source to new_path
                replacement = relative_path(source_file, new_path)
            else:
                # Absolute: use repo-relative form
                replacement = to_repo_relative(new_path)

            content = content.replace(match, replacement)

    return content
```

**Example:**
- File: `specs/meta/workflow.md` references `../views/discover.md`
- Rename: `specs/views/discover.md` → `specs/tui/views/discover.md`
- New reference: `../tui/views/discover.md` (relative path recalculated)

#### 8.9.7 Bulk Rename Operations

When multiple files need renaming (e.g., directory restructure):

```
BULK RENAME PROCEDURE

Input: List of (old_path, new_path) pairs

Step 1: Dependency Analysis
  1.1 Build rename graph (which renames affect which)
  1.2 Detect cycles (A→B, B→A)
  1.3 Topological sort for execution order

Step 2: Dry Run
  2.1 For each rename, preview reference updates
  2.2 Calculate total files affected
  2.3 Present summary to user

Step 3: Execute in Order
  3.1 Rename leaf nodes first (files with no dependents)
  3.2 Update references after each rename
  3.3 Then rename parent directories

Step 4: Validate Corpus
  4.1 Run reference health check (Section 8.5)
  4.2 Report any broken references
```

**Example bulk rename (directory restructure):**

```markdown
## Bulk Rename: views/ → tui/views/

### Renames Required
1. specs/views/discover.md → specs/tui/views/discover.md
2. specs/views/jobs.md → specs/tui/views/jobs.md
3. specs/views/parser_bench.md → specs/tui/views/parser_bench.md

### Reference Updates Required
| Source File | Updates |
|-------------|---------|
| specs/tui.md | 3 links |
| CLAUDE.md | 2 links |
| spec.md | 1 link |

### Execution Order
1. Create specs/tui/views/ directory
2. Rename specs/views/discover.md (leaf)
3. Rename specs/views/jobs.md (leaf)
4. Rename specs/views/parser_bench.md (leaf)
5. Update all references
6. Remove empty specs/views/ directory
```

---

## 9. Phase 4: Recommendations

### 9.1 Recommendation Types

| Type | Priority | Action | Confidence |
|------|----------|--------|------------|
| **ARCHIVE** | LOW | Move to archive/ | **Requires confirmation** |
| **UPDATE** | MEDIUM | Sync spec with code | Based on diff clarity |
| **MERGE** | MEDIUM | Combine overlapping specs | **Requires confirmation** |
| **SPLIT** | LOW | Break up large specs | Based on bloat severity |
| **RENAME** | LOW-MEDIUM | Fix naming convention violations | Based on violation severity |
| **WRITE** | VARIES | Create missing spec | Based on gap severity |
| **DELETE** | LOW | Remove (rare, prefer archive) | **Requires confirmation** |
| **LINK** | LOW | Add cross-references | HIGH (trivial) |

### 9.2 Recommendation Format

```markdown
## Recommendations

### High Priority (3)

#### REC-001: Update specs/extraction.md
- **Type:** UPDATE
- **Reason:** SPEC_STALE - wizard steps changed
- **Effort:** Small (1-2 sections)
- **Confidence:** HIGH
- **Action:** Sync Section 3.2 with extraction/wizard.rs

#### REC-002: Archive specs/pricing_v2_refined.md
- **Type:** ARCHIVE
- **Reason:** Superseded by specs/pricing.md
- **Effort:** Trivial
- **Confidence:** Requires confirmation
- **Action:** mv specs/pricing_v2_refined.md archive/

#### REC-003: Write specs/scout.md
- **Type:** WRITE
- **Reason:** Undocumented feature
- **Effort:** Medium (new spec)
- **Confidence:** HIGH
- **Action:** Document crates/casparian_scout/ behavior

#### REC-004: Rename specs/JobLifecycle.md
- **Type:** RENAME
- **Reason:** NAME-001 violation (case/format)
- **Effort:** Trivial
- **Confidence:** HIGH
- **Current:** specs/JobLifecycle.md
- **Proposed:** specs/job_lifecycle.md
- **References to update:** 3 files (CLAUDE.md, spec.md, specs/tui.md)
- **Action:** Rename file and update all inbound references

### Medium Priority (5)
...

### Low Priority (8)
...
```

### 9.3 Effort Estimation

| Effort | Definition |
|--------|------------|
| **Trivial** | File move, add link |
| **Small** | Update 1-2 sections |
| **Medium** | Significant rewrite or new small spec |
| **Large** | New comprehensive spec or major restructure |

---

## 10. Phase 5: Execution

### 10.1 User Approval

Before executing, present summary:

```markdown
## Maintenance Plan

**Recommendations:** 18 total
- Archive: 2
- Update: 5
- Merge: 1
- Rename: 3
- Write: 2
- Link: 5

**Estimated effort:** 4-6 hours

Proceed with execution?
```

### 10.2 Execution Order

1. **Archives first** - Remove clutter before updating
   - Propagate: Update/remove inbound references to archived specs
2. **Splits** - Break up oversized specs (see Section 8.4)
   - Propagate: Redirect inbound references to correct children (see Section 8.7.4)
3. **Merges** - Consolidate before updating
   - Propagate: Update references from merged sources to target
4. **Renames** - Fix naming convention violations (see Section 8.9)
   - Propagate: Update all inbound references to use new path (see Section 8.9.5)
   - Execute bulk renames in dependency order (see Section 8.9.7)
5. **Updates** - Sync with code
6. **Links** - Add cross-references
7. **Writes** - New specs last (may reference updated ones)
8. **Reference validation** - Verify all propagated updates are correct

**Critical:** Reference propagation (Section 8.7) must occur immediately after each destructive action (archive, split, merge, rename) to maintain corpus integrity.

#### 10.2.1 Recommendation Conflict Resolution

When a single file has multiple recommendations, apply these precedence rules:

| Conflict | Resolution | Rationale |
|----------|------------|-----------|
| MERGE + RENAME | **MERGE wins** | Rename is implicit in merge target naming |
| ARCHIVE + RENAME | **ARCHIVE wins** | No point renaming before archiving |
| SPLIT + RENAME | **RENAME first** | Rename parent, then split with convention-compliant children |
| ARCHIVE + MERGE | **MERGE wins** | Merge preserves content; archive the merged sources |
| SPLIT + MERGE | **Evaluate overlap** | If >70% overlap, merge; otherwise split then link |

**Detection:** During Phase 4 (Recommendations), flag files with multiple recommendations:

```markdown
### Conflict Detected: specs/pricing_v2_refined.md

Recommendations:
- MERGE into specs/pricing.md (OVERLAP-001)
- RENAME to specs/pricing_refined.md (NAME-002)

**Resolution:** MERGE takes precedence. After merge, only specs/pricing.md exists.
No separate rename needed.
```

### 10.3 Audit Trail

Record all changes:

```markdown
## Maintenance Log - 2026-01-14

| Time | Action | File | Details |
|------|--------|------|---------|
| 10:30 | ARCHIVE | specs/old_api.md | Moved to archive/specs/ |
| 10:32 | MERGE | specs/pricing*.md | Combined into specs/pricing.md |
| 10:45 | UPDATE | specs/extraction.md | Synced wizard steps |
| 11:00 | LINK | specs/tui.md | Added refs to view specs |
```

---

## 11. Output Artifacts

### 11.1 Session Folder Structure

```
specs/meta/maintenance/
├── YYYY-MM-DD/
│   ├── inventory.md              # Phase 1 output
│   ├── reference_graph.json      # Phase 1 output: spec relationships
│   ├── alignment_report.md       # Phase 2 output
│   ├── cross_spec_report.md      # Phase 3 output
│   ├── naming_analysis.md        # Phase 3 output: naming convention violations
│   ├── recommendations.md        # Phase 4 output
│   ├── execution_log.md          # Phase 5 output
│   ├── reference_propagation.md  # Phase 5 output: reference updates
│   ├── summary.md                # Final summary
│   ├── decisions.md              # User decisions
│   ├── error_report.md           # Aggregated errors
│   ├── checkpoint.json           # Recovery checkpoint
│   ├── backups/                  # File backups for rollback
│   ├── splits/                   # Split operation artifacts
│   │   ├── {spec_name}_mapping.json    # Section-to-child mapping table
│   │   ├── {spec_name}_sections.json   # Parsed section metadata
│   │   └── {spec_name}_redirects.json  # Anchor redirect table
│   └── renames/                  # Rename operation artifacts
│       └── rename_log.json       # Old path → new path mapping with affected files
```

### 11.2 Summary Format

```markdown
## Spec Maintenance Summary - 2026-01-14

### Corpus Health

| Metric | Before | After |
|--------|--------|-------|
| Total specs | 18 | 15 |
| Active | 12 | 14 |
| Stale | 4 | 0 |
| Orphan | 2 | 1 |
| Alignment score | 72% | 94% |

### Error Summary

| Phase | Errors | Skipped |
|-------|--------|---------|
| Inventory | 2 | 3 |
| Alignment | 0 | 2 |
| Cross-Spec | 1 | 1 |
| Total | 3 | 6 |

**Audit Completion Rate:** 92%

### Actions Taken

| Action | Count |
|--------|-------|
| Archived | 3 |
| Updated | 4 |
| Merged | 1 |
| Renamed | 2 |
| Links added | 8 |
| References updated | 12 |

### Remaining Issues

- specs/ai_wizards.md - Orphan, unclear if still relevant (needs user decision)
- specs/hl7_parser.md - PLANNED, no implementation timeline

### Next Maintenance

Recommended: 2026-04-14 (quarterly)
```

---

## 12. Automation Hooks

### 12.1 Quick Health Check

Run without full audit:

```bash
# Conceptual - could be a CLI command
casparian specs health

Spec Corpus Health: 85%
- 2 specs stale (>90 days)
- 1 broken reference
- 3 orphan specs

Run full maintenance? (y/n)
```

### 12.2 Pre-Commit Check

Before creating new spec:

```bash
casparian specs check-overlap "new_feature.md"

Potential overlaps found:
- specs/extraction.md (30% keyword overlap)
- specs/tui.md (20% keyword overlap)

Review these before creating new spec.
```

---

## 13. Integration with Refinement Workflow

### 13.1 When Maintenance Finds Gaps

If maintenance identifies a spec needing significant updates:

1. Create refinement session for that spec
2. Use spec_refinement_workflow.md
3. Mark spec as IN_REFINEMENT in maintenance tracker

### 13.2 Post-Refinement

After refinement completes:

1. Re-run alignment check for that spec
2. Update cross-references
3. Verify no new overlaps introduced

---

## 14. Metrics

### 14.1 Corpus Health Score

```
health_score = (
    (aligned_specs / total_specs) * 0.4 +
    (1 - stale_specs / total_specs) * 0.3 +
    (1 - orphan_specs / total_specs) * 0.2 +
    (valid_references / total_references) * 0.1
) * 100
```

### 14.2 Tracking Over Time

```markdown
## Health History

| Date | Score | Specs | Stale | Actions |
|------|-------|-------|-------|---------|
| 2026-01-14 | 94% | 15 | 0 | Full maintenance |
| 2025-10-15 | 72% | 18 | 4 | Initial audit |
```

### 14.3 Strategy Corpus Health Score

```
strategy_health = (
    (market_aligned / total_strategies) * 0.4 +
    (has_spec_reference / total_strategies) * 0.2 +
    (updated_in_90_days / total_strategies) * 0.2 +
    (validated_hypotheses / testable_hypotheses) * 0.2
) * 100
```

### 14.4 Combined Corpus Health

```
combined_health = (spec_health * 0.6) + (strategy_health * 0.4)
```

Weighting reflects that specs are the primary deliverable; strategies inform direction.

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 2.6 | Expanded NAME-002 anti-pattern detection: added `_redesign`, `_new`, `_old` to version/state suffix patterns. Updated detection algorithm (8.9.2 Step 1.2) accordingly. |
| 2026-01-14 | 2.5 | **Spec refinement integration (18 gaps resolved):** (1) Recommendation conflict resolution rules (10.2.1) - MERGE+RENAME, ARCHIVE+RENAME precedence. (2) Rename rollback protocol (8.9.5.1) - partial failure recovery. (3) Dry run preview for renames (8.9.5 Step 2). (4) Git error handling for renames (8.9.5 Step 4.1). (5) Naming analysis in parallelization table (2.6.4). (6) RENAME_FAILED/REFERENCE_UPDATE_FAILED errors (4.3). (7) NAME-005 context mapping table (8.9.2). (8) Bulk rename checkpoint format (8.8). (9) Path normalization algorithm (8.9.6). (10) Anchor handling clarification (8.9.1). (11) Split child naming convention integration (8.7.4). |
| 2026-01-14 | 2.4 | Added Contract Compliance Mode (2.7): mode invocation (2.7.1), phase adaptations (2.7.3), contract specification format (2.7.4), compliance report format (2.7.5), Workflow Manager integration (2.7.6), bootstrapping note (2.7.7). Enables self-healing workflow contract enforcement. |
| 2026-01-14 | 2.3 | Added naming convention enforcement (8.9): file/directory/anchor naming rules (8.9.1), detection algorithm (8.9.2), violation severity calculation (8.9.3), rename execution protocol (8.9.5), reference update patterns (8.9.6), bulk rename operations (8.9.7). Added RENAME recommendation type (9.1), rename to execution order (10.2), naming_analysis.md and renames/ to session folder (11.1). |
| 2026-01-14 | 2.2 | Round 002 integration: Added reference parsing specification (8.7.1.1 - GAP-REF-PARSE-001), complete split propagation algorithm with anchor mapping (8.7.4 - GAP-ANCHOR-MAP-001), multi-signal overlap detection algorithm (8.1 - GAP-OVERLAP-001), updated decision checkpoints (8.8), split artifacts in session folder (11.1) |
| 2026-01-14 | 2.1 | Added parallelization (2.6), token limit handling (8.4), FILE_TOO_LARGE error, reference propagation with parent/child traversal (8.7) |
| 2026-01-14 | 2.0 | Round 001 integration: Added execution model (GAP-ROLE-001), error handling (GAP-ERROR-001), decision escalation (GAP-DECISION-001), strategy document handling (GAP-STRATEGY-001) |
| 2026-01-14 | 1.0 | Initial specification |

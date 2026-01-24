# Memory Audit Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.0
**Category:** Analysis workflow (per workflow_manager.md Section 3.3.1)
**Purpose:** Multi-instance Claude system for codebase memory optimization analysis
**Inspired By:** `spec_refinement_workflow.md`

---

## 1. Overview

This document defines a **3-instance Claude workflow** for analyzing Rust codebases for memory optimization opportunities. The system identifies allocation hotspots, arena candidates, unnecessary clones, and cache locality issues.

### 1.1 Design Principles

1. **Courteous Computing** - Minimize memory footprint; respect the customer's machine
2. **Large File Focus** - Prioritize paths that process user data (10TB+ scenarios)
3. **Safety First** - Never propose unsafe optimizations without explicit justification
4. **Measurable Impact** - Every finding should estimate memory saved or allocations avoided
5. **Report, Don't Rewrite** - Output is findings; humans implement (future: workflow chaining)

### 1.2 Target Discovery (Dynamic)

**The workflow discovers targets at runtime - no hardcoded crate list.**

**Discovery Process:**
1. Scan workspace for `Cargo.toml` files
2. Identify crate boundaries (`crates/*/src/`)
3. Parse `lib.rs`/`main.rs` for module structure
4. Build call graph to identify hot paths

**Heuristics for Priority Assignment:**
```
P0 (Critical):
  - Crates with "worker", "executor", "process" in name
  - Files handling user data (look for: File, Read, Write, Arrow, Batch)
  - Async runtime entry points

P1 (High):
  - Crates with "scan", "discover", "watch" (file system ops)
  - TUI/rendering code (look for: ratatui, Frame, render)
  - Anything with loops over external data

P2 (Medium):
  - Validation, schema, parsing logic
  - Protocol/serialization code

P3 (Low):
  - Auth, config, one-time initialization
  - Test utilities
```

**Dynamic Priority Detection:**
```rust
// Analyst looks for these patterns to assign priority:
- `async fn` + `File::open` → P0 (async file processing)
- `for _ in files` or `for _ in rows` → P0/P1 (iteration over user data)
- `impl Widget` or `fn render` → P1 (TUI rendering)
- `#[derive(Serialize)]` on large structs → P2 (serialization)
```

### 1.3 Scope File (User-Provided Overrides)

User can override auto-discovery via `scope.md`:

```markdown
# Optional: Override automatic target discovery

## Include (even if low auto-priority)
- crates/my_new_crate/src/hot_path.rs

## Exclude (skip even if high auto-priority)
- crates/*/tests/
- crates/*/benches/

## Force Priority
- crates/critical_module: P0
```

---

## 1.4 Runtime Discovery Algorithm

**Phase 1: Workspace Scan**
```bash
# Analyst runs these to find targets
find . -name "Cargo.toml" -not -path "./target/*"
```

**Phase 2: Crate Classification**
For each discovered crate:
1. Read `Cargo.toml` for dependencies (arrow, tokio, ratatui → hot path indicators)
2. Read `lib.rs` or `main.rs` for public API surface
3. Grep for hot-path patterns:
   - `async fn.*File` → file processing
   - `for.*in.*rows` or `iter()` on collections → iteration
   - `impl.*Widget` → TUI rendering
   - `#[derive(Serialize)]` → serialization overhead

**Phase 3: Call Graph (Optional, Deeper Analysis)**
- Use `cargo-call-stack` or manual trace to find:
  - Entry points (main, request handlers, event loops)
  - Paths from entry to allocation-heavy code

**Phase 4: Priority Assignment**
```
Score = (file_io_weight × 3) + (iteration_weight × 2) + (render_weight × 1)

P0: Score >= 5
P1: Score >= 3
P2: Score >= 1
P3: Score < 1
```

**Why Dynamic Discovery Matters:**
- Codebase evolves; new crates added, old ones removed
- Hot paths shift as features change
- Avoids stale hardcoded assumptions

---

## 2. Instance Roles

### 2.1 Analyst Instance (Engineer Equivalent)

**Role:** Memory pattern identifier and optimization proposer

**Responsibilities:**
- Read source code and identify allocation patterns
- Classify allocations by type (heap, stack, arena-candidate)
- Propose specific optimizations with estimated impact
- Identify lifetime scopes for potential arenas
- Find unnecessary clones, redundant allocations

**Persona Prompt:**
```
You are a Staff Engineer specializing in systems programming and memory optimization.
Your role is to analyze Rust code for memory efficiency. You:

- Think in terms of allocator pressure: heap vs stack vs arena
- Know when Vec<T> should be SmallVec, Box<T> should be inline, String should be &str
- Identify clone() calls that could be references or Cow<>
- Recognize arena patterns: batch allocations with shared lifetime
- Consider cache locality: struct layout, hot/cold field separation
- Estimate impact: "This removes ~N allocations per file processed"

Analysis categories:
1. ARENA_CANDIDATE - Batch allocations with scoped lifetime
2. UNNECESSARY_CLONE - Clone where borrow would suffice
3. HEAP_AVOIDABLE - Small fixed-size data on heap
4. LIFETIME_EXTENSION - Data lives longer than necessary
5. CACHE_HOSTILE - Poor struct layout or access patterns
6. ALLOCATION_LOOP - Allocations inside hot loops

For each finding:
- Location: file:line
- Category: one of above
- Current code: snippet
- Issue: what's wrong
- Proposed fix: concrete alternative
- Impact: estimated memory/allocation savings
- Confidence: HIGH/MEDIUM/LOW
- Safety: SAFE/NEEDS_REVIEW/UNSAFE
```

**Output Format:**
```markdown
## Finding: [FINDING-ID]

**Category:** ARENA_CANDIDATE | UNNECESSARY_CLONE | ...
**Location:** `crates/foo/src/bar.rs:123-145`
**Confidence:** HIGH | MEDIUM | LOW
**Safety:** SAFE | NEEDS_REVIEW | UNSAFE

### Current Code
```rust
[problematic code snippet]
```

### Issue
[What's inefficient and why]

### Proposed Alternative
```rust
[optimized code or pattern]
```

### Estimated Impact
- Allocations avoided: ~N per [unit of work]
- Memory saved: ~X bytes per [unit of work]
- Applicable to: [which code paths]

### Trade-offs
- Pro: [benefits]
- Con: [costs, complexity added]

### Prerequisites
- [ ] Requires: [e.g., bumpalo dependency, lifetime annotation changes]
```

---

### 2.2 Validator Instance (Reviewer Equivalent)

**Role:** Safety checker and impact verifier

**Responsibilities:**
- Verify proposed optimizations are sound
- Check for lifetime/ownership violations
- Validate impact estimates are realistic
- Identify risks: race conditions, use-after-free, etc.
- Ensure optimizations don't break existing tests
- Flag when optimization isn't worth complexity

**Persona Prompt:**
```
You are a Principal Engineer known for catching subtle memory bugs. Your role is
to validate memory optimization proposals. You:

- Assume every optimization is wrong until proven safe
- Check lifetime annotations are correct
- Verify arena scopes don't escape
- Question impact estimates (are they realistic?)
- Consider: "What if this is called from multiple threads?"
- Ask: "Does this break any existing invariants?"
- Evaluate: "Is the complexity worth the savings?"

Your validation should:
- Accept findings that are clearly safe and impactful
- Flag findings that need more analysis
- Reject findings that are unsafe or not worth it
- Provide specific counterexamples when rejecting

Never approve without verifying:
1. Lifetime correctness
2. Thread safety
3. Impact estimate plausibility
4. Complexity vs benefit trade-off
```

**Output Format:**
```markdown
## Validation: [FINDING-ID]

**Verdict:** APPROVED | NEEDS_WORK | REJECTED

### Safety Analysis
- Lifetime correctness: [PASS/FAIL/UNCLEAR]
- Thread safety: [PASS/FAIL/N/A]
- No undefined behavior: [PASS/FAIL/UNCLEAR]

### Impact Verification
- Estimate plausible: [YES/NO/NEEDS_MEASUREMENT]
- Actual hot path: [YES/NO/MAYBE]
- Complexity justified: [YES/NO]

### Issues Found
- **[ISSUE-ID]**: [Description]
  - Risk: [What could go wrong]
  - Suggestion: [How to address]

### Conditions for Approval
- [ ] [Condition 1]
- [ ] [Condition 2]

### Counterexample (if rejecting)
[Specific scenario where optimization fails]
```

---

### 2.3 Coordinator Instance (Mediator Equivalent)

**Role:** Synthesis, prioritization, and report generation

**Responsibilities:**
- Aggregate findings across crates
- Prioritize by impact and safety
- Track overall memory profile improvement potential
- Generate actionable reports
- Identify cross-cutting patterns

**Persona Prompt:**
```
You are a Technical Program Manager coordinating a memory optimization effort.
Your role is to:

- Synthesize Analyst findings and Validator feedback
- Prioritize findings by: Impact × Safety × Effort
- Group related findings (e.g., "arena pattern applicable in 5 places")
- Track cumulative impact across the codebase
- Generate reports suitable for engineering planning

Your outputs should:
- Rank findings by ROI (memory saved / implementation effort)
- Highlight quick wins (HIGH impact, SAFE, LOW effort)
- Flag systemic patterns that need architectural decisions
- Provide executive summary for stakeholders

Success metric: Report enables engineer to implement top 5 optimizations
in a single focused session.
```

**Output Format:**
```markdown
## Memory Audit Report - Round [N]

### Executive Summary
- Total findings: X
- Approved (ready to implement): Y
- Needs work: Z
- Rejected: W
- Estimated memory impact: ~X MB saved at peak
- Estimated allocation reduction: ~Y% in hot paths

### Quick Wins (Implement First)
| ID | Location | Category | Impact | Effort |
|----|----------|----------|--------|--------|
| F-001 | worker/bridge.rs | UNNECESSARY_CLONE | High | Low |
| ... | ... | ... | ... | ... |

### Architectural Recommendations
1. **[Pattern]**: Found in N locations. Consider [approach].
   - Locations: [list]
   - Unified solution: [description]

### Findings by Crate

#### casparian_worker (P0)
- F-001: [one-line summary] - APPROVED
- F-002: [one-line summary] - NEEDS_WORK
...

#### casparian_scout (P1)
...

### Deferred (Low ROI)
- F-XXX: [reason for deferral]

### Next Round Focus
[What Analyst should investigate next]
```

---

## 3. Analysis Categories

### 3.1 Arena Candidates (ARENA_CANDIDATE)

**Pattern:** Batch allocations with bounded, shared lifetime

**Indicators:**
- Multiple allocations in a loop that are freed together
- Processing phases with clear start/end boundaries
- Temporary data structures rebuilt per request/file

**Rust Libraries:**
- `bumpalo` - General-purpose bump allocator
- `typed-arena` - Type-safe arenas
- `slotmap` - Arena-like with stable indices

**Example Hot Spots:**
```rust
// Before: N allocations per file
for row in rows {
    let parsed = parse_row(row);        // heap alloc
    let validated = validate(parsed);    // heap alloc
    results.push(validated);             // heap alloc (grow)
}

// After: 1 arena per file
let arena = Bump::new();
for row in rows {
    let parsed = arena.alloc(parse_row(row));      // bump
    let validated = arena.alloc(validate(parsed)); // bump
    // arena dropped at end of file processing
}
```

---

### 3.2 Unnecessary Clones (UNNECESSARY_CLONE)

**Pattern:** `.clone()` where borrow or `Cow<>` would suffice

**Indicators:**
- `clone()` followed by read-only use
- Cloning inside loops
- Cloning to satisfy lifetime (often fixable with better API)

**Fixes:**
- Use `&T` instead of `T`
- Use `Cow<'a, T>` for "usually borrowed, sometimes owned"
- Use `Arc<T>` for shared ownership (but adds atomic overhead)

---

### 3.3 Heap-Avoidable Allocations (HEAP_AVOIDABLE)

**Pattern:** Small, fixed-size data on heap when stack would work

**Indicators:**
- `Box<[T; N]>` where N is small
- `Vec<T>` where length is bounded and small
- `String` where `[u8; N]` or `ArrayString<N>` would work

**Fixes:**
- `SmallVec<[T; N]>` - Stack for small, heap for overflow
- `ArrayString<N>` - Stack-allocated strings
- Inline structs instead of `Box<T>`

---

### 3.4 Lifetime Extension (LIFETIME_EXTENSION)

**Pattern:** Data kept alive longer than necessary

**Indicators:**
- Large structs held across await points
- Caches that grow unbounded
- References that force parent to live too long

**Fixes:**
- Drop explicitly before await
- Add cache eviction
- Clone small data to break lifetime dependency

---

### 3.5 Cache-Hostile Patterns (CACHE_HOSTILE)

**Pattern:** Poor memory layout hurting CPU cache efficiency

**Indicators:**
- Hot and cold fields interleaved in struct
- Array-of-Structs where Struct-of-Arrays would iterate better
- Pointer chasing through many indirections

**Fixes:**
- Separate hot/cold fields into nested structs
- SoA transformation for batch processing
- Flatten nested structures where possible

---

### 3.6 Allocation in Hot Loops (ALLOCATION_LOOP)

**Pattern:** Allocations inside performance-critical loops

**Indicators:**
- `Vec::new()` or `String::new()` inside loop body
- `format!()` inside loops
- `to_string()` or `to_vec()` inside loops

**Fixes:**
- Hoist allocation outside loop
- Reuse buffer with `.clear()`
- Pre-allocate with capacity

---

## 4. Document Structure

```
specs/meta/
├── memory_audit_workflow.md    # THIS FILE (read-only reference)
├── sessions/
│   └── memory_audit_001/       # One folder per audit session
│       ├── round_001/
│       │   ├── analyst.md      # Analyst's findings
│       │   ├── validator.md    # Validator's review
│       │   └── report.md       # Coordinator's synthesis
│       ├── round_002/
│       │   └── ...
│       ├── decisions.md        # User decisions (which to implement)
│       ├── status.md           # Finding counts, progress tracking
│       └── scope.md            # Which crates/files to analyze
```

---

## 5. Process Flow

### 5.1 Round Structure

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ROUND N FLOW                                │
│                                                                     │
│  ┌──────────┐     ┌──────────┐     ┌───────────┐     ┌──────────┐  │
│  │ Analyst  │ ──► │ Validator│ ──► │Coordinator│ ──► │   User   │  │
│  │          │     │          │     │           │     │          │  │
│  │  Find    │     │  Verify  │     │ Synthesize│     │ Decide   │  │
│  │  Issues  │     │  Safety  │     │  Report   │     │ Priority │  │
│  └──────────┘     └──────────┘     └───────────┘     └──────────┘  │
│       │                                                    │        │
│       └───────────────── ROUND N+1 ◄──────────────────────┘        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.2 Step-by-Step Process

**Step 0: Scope Definition**
1. User specifies crates/files to analyze
2. User sets focus: arena-heavy, clone-reduction, or full audit
3. Write `scope.md` with targets

**Step 1: Analyst Phase**
1. Read scope from `scope.md`
2. Read previous findings and decisions
3. Analyze target code for memory patterns
4. Write `round_N/analyst.md` with findings

**Step 2: Validator Phase**
1. Read `round_N/analyst.md`
2. Verify each finding for safety and accuracy
3. Check impact estimates
4. Write `round_N/validator.md` with verdicts

**Step 3: Coordinator Phase**
1. Read Analyst and Validator outputs
2. Synthesize into prioritized report
3. Update `status.md` with finding counts
4. Write `round_N/report.md`

**Step 4: User Phase**
1. Review report
2. Mark findings to implement vs defer
3. Request deeper analysis on specific areas
4. Signal ready for next round (or terminate)

### 5.3 Termination Criteria

**Complete when:**
1. All P0 crates analyzed
2. No new HIGH-impact findings in last round
3. User satisfied with coverage
4. Actionable report generated

**Continue when:**
- User requests deeper analysis
- New code paths identified
- Validator flagged findings needing rework

---

## 6. Integration with Claude Code

### 6.1 Invocation

User triggers via natural language:
```
"Run the memory audit workflow on casparian_worker"
"Start a memory audit session focusing on arena opportunities"
"Continue the memory audit from session memory_audit_001"
```

### 6.2 Task Tool Spawning

```
┌─────────────────────────────────────────────────────────────────────┐
│                    COORDINATOR (Main Context)                        │
│                                                                     │
│  1. Read scope.md, create session folder if needed                  │
│                                                                     │
│  2. Spawn Analyst ──► Task(prompt: analyst_prompt)                  │
│                              │                                      │
│                              ▼                                      │
│                        analyst.md written                           │
│                              │                                      │
│  3. Spawn Validator ──► Task(prompt: validator_prompt)              │
│                              │                                      │
│                              ▼                                      │
│                        validator.md written                         │
│                              │                                      │
│  4. Synthesize, write report.md, update status.md                   │
│                              │                                      │
│  5. Present findings to user via summary                            │
│                              │                                      │
│  6. AskUserQuestion for next steps                                  │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.3 Analyst Prompt Template

```
You are the Analyst instance in a memory audit workflow.

## Scope
- Workspace: {workspace_root}
- Discovery mode: {auto | manual}
- Priority filter: {P0 only | P0-P1 | all}
- Focus: {arena | clones | full}
- Previous findings to ignore: {already_found}

## Discovery (if auto mode)
1. Find all Cargo.toml files to identify crate boundaries
2. For each crate, read lib.rs/main.rs to understand structure
3. Prioritize by heuristics (see workflow spec Section 1.2)
4. Start with highest-priority targets

## Your Task
Analyze the code for memory optimization opportunities.

Categories to check:
1. ARENA_CANDIDATE - Batch allocations with scoped lifetime
2. UNNECESSARY_CLONE - Clone where borrow would suffice
3. HEAP_AVOIDABLE - Small fixed-size data on heap
4. LIFETIME_EXTENSION - Data lives longer than necessary
5. CACHE_HOSTILE - Poor struct layout or access patterns
6. ALLOCATION_LOOP - Allocations inside hot loops

For each finding, provide:
- Location (file:line)
- Category
- Current code snippet
- Issue description
- Proposed fix
- Estimated impact
- Confidence and safety level

Write output to: specs/meta/sessions/{session}/round_{N}/analyst.md

Focus on high-impact findings in hot paths. Quantity < Quality.
```

### 6.4 Validator Prompt Template

```
You are the Validator instance in a memory audit workflow.

## Context
- Session: {session_name}
- Round: {round_number}
- Analyst findings: [attached]

## Your Task
Validate each finding for:
1. Safety - Is the optimization sound? Lifetime correct? Thread safe?
2. Impact - Is the estimate realistic? Is this actually a hot path?
3. Complexity - Is the optimization worth the added complexity?

For each finding:
- APPROVE if safe, impactful, and worth it
- NEEDS_WORK if idea is good but needs refinement
- REJECT if unsafe, low impact, or not worth complexity

Write output to: specs/meta/sessions/{session}/round_{N}/validator.md

Be thorough. Assume optimizations are wrong until proven safe.
```

---

## 7. Example Session

> **Note:** Examples below use concrete paths for illustration. In practice,
> all paths are discovered dynamically at runtime based on actual codebase structure.

### 7.1 Scope Definition

**scope.md** (auto-generated, user can override):
```markdown
# Memory Audit Scope

**Session:** memory_audit_001
**Started:** 2026-01-14
**Auto-discovered:** Yes

## Discovered Targets (auto-prioritized)

### P0 - Critical (processes user data)
- `crates/*/src/` files containing: async file I/O, Arrow batches, row iteration
- Auto-detected hot paths: [dynamically filled]

### P1 - High (scales with input)
- File system scanning, TUI rendering
- Auto-detected: [dynamically filled]

### P2/P3 - Lower priority
- [dynamically filled]

## User Overrides (optional)
<!-- Uncomment to customize -->
<!-- ## Force Include -->
<!-- - path/to/specific/file.rs -->

<!-- ## Force Exclude -->
<!-- - **/tests/** -->

## Focus Areas
- Arena opportunities in file processing
- Clone reduction in hot paths
- Allocation loops in batch processing
```

### 7.2 Round 1 Artifacts (Excerpts)

**analyst.md:**
```markdown
## Finding: F-001

**Category:** ALLOCATION_LOOP
**Location:** `crates/casparian_worker/src/bridge.rs:234-256`
**Confidence:** HIGH
**Safety:** SAFE

### Current Code
```rust
for batch in arrow_batches {
    let serialized = batch.to_ipc()?;  // allocates Vec<u8>
    let message = Message::new(serialized);  // allocates again
    sender.send(message)?;
}
```

### Issue
Each batch creates two allocations: one for IPC serialization, one for message wrapper.
For large files with thousands of batches, this creates significant allocator pressure.

### Proposed Alternative
```rust
let mut buffer = Vec::with_capacity(estimated_batch_size);
for batch in arrow_batches {
    buffer.clear();
    batch.write_ipc_to(&mut buffer)?;  // reuse buffer
    sender.send_borrowed(&buffer)?;    // zero-copy send
}
```

### Estimated Impact
- Allocations avoided: ~2N per file (N = batch count)
- Memory saved: Reduces peak usage by avoiding fragmentation
- Applicable to: All parser executions

### Trade-offs
- Pro: Major reduction in allocator calls
- Con: Requires modifying Message API to support borrowed data
```

**validator.md:**
```markdown
## Validation: F-001

**Verdict:** APPROVED

### Safety Analysis
- Lifetime correctness: PASS - buffer lives for entire loop
- Thread safety: N/A - single-threaded processing
- No undefined behavior: PASS - standard Rust patterns

### Impact Verification
- Estimate plausible: YES - 2 allocs × 1000 batches = 2000 allocs/file
- Actual hot path: YES - bridge.rs is core execution path
- Complexity justified: YES - minimal code change

### Issues Found
None. Clean optimization.

### Conditions for Approval
- [x] Message API supports borrowed data (or can be modified)
- [x] No concurrent access to buffer
```

**report.md:**
```markdown
## Memory Audit Report - Round 1

### Executive Summary
- Total findings: 4
- Approved: 3
- Needs work: 1
- Rejected: 0
- Estimated allocation reduction: ~60% in file processing hot path

### Quick Wins (Implement First)
| ID | Location | Category | Impact | Effort |
|----|----------|----------|--------|--------|
| F-001 | worker/bridge.rs | ALLOCATION_LOOP | High | Medium |
| F-003 | worker/executor.rs | UNNECESSARY_CLONE | Medium | Low |
| F-004 | worker/validation.rs | HEAP_AVOIDABLE | Medium | Low |

### Architectural Recommendations
1. **Buffer Reuse Pattern**: Found in 3 locations. Consider a `BufferPool` utility.

### Next Round Focus
- casparian_scout file metadata storage
- Arena opportunities in batch file iteration
```

---

## 8. Future: Workflow Chaining

This workflow is designed to integrate with a future **Workflow Manager** that:

1. **Chains workflows**: Memory audit → Code generation → Test validation
2. **Passes context**: Findings from audit become inputs to implementation workflow
3. **Tracks lineage**: Which finding led to which code change

**Integration Points:**
- `status.md` uses standard format parseable by manager
- `report.md` includes machine-readable sections
- Findings have stable IDs for cross-workflow reference

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial workflow specification |

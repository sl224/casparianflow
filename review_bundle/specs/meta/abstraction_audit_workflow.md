# Abstraction Audit Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.1
**Category:** Analysis workflow (per workflow_manager.md Section 3.3.1)
**Purpose:** Multi-instance Claude system for detecting platform-specific coupling in the codebase
**Inspired By:** `memory_audit_workflow.md`

---

## 1. Overview

This document defines a **3-instance Claude workflow** for analyzing the codebase for platform-specific coupling that hinders modularity. The system identifies hardcoded database calls, LLM-specific code, and other violations of the abstraction boundaries.

### 1.1 Design Principles

1. **Modular by Default** - Code should be portable across platforms (databases, LLMs, cloud providers)
2. **Trait-First Design** - Platform-specific code behind trait boundaries
3. **Configuration Over Code** - Platform selection via config, not hardcoded
4. **Report, Then Fix** - Output is `actionable_findings.json`; Implementation Protocol handles fixes
5. **Incremental Prevention** - Same patterns power `feature_workflow` validation to prevent new violations

### 1.2 Abstraction Domains

The workflow audits these platform-agnostic boundaries:

| Domain | Abstraction Goal | Common Violations |
|--------|-----------------|-------------------|
| **Database** | Support SQLite, PostgreSQL, MySQL | Direct `sqlx::Sqlite*` types, SQLite-specific SQL |
| **LLM** | Support Claude, GPT, Llama, local models | Hardcoded Anthropic SDK, model-specific prompts |
| **File Storage** | Support local, S3, Azure Blob, GCS | Hardcoded `std::fs::*` without abstraction |
| **Queue/Messaging** | Support in-memory, Redis, RabbitMQ | Direct `tokio::mpsc` in library code |
| **Configuration** | Environment-agnostic config | Hardcoded env vars, paths |
| **Serialization** | Format-agnostic data exchange | Hardcoded JSON assumptions |

### 1.3 Detection Patterns (Shared with feature_workflow)

These patterns are defined once and used by both:
- **abstraction_audit_workflow** - Full codebase scan
- **feature_workflow Phase 3** - Incremental check on changed files

```
ABSTRACTION_PATTERNS = {
    # Database
    "DB_SQLITE_SPECIFIC": {
        "pattern": r"(sqlx::Sqlite|SqlitePool|SqliteRow|sqlx::duckdb::)",
        "severity": "HIGH",
        "suggestion": "Use generic sqlx::Pool<DB> or Database trait"
    },
    "DB_SQLITE_SYNTAX": {
        "pattern": r"(AUTOINCREMENT|INTEGER PRIMARY KEY|duckdb_sequence|pragma)",
        "severity": "MEDIUM",
        "suggestion": "Use database-agnostic SQL or migration abstraction"
    },
    "DB_RAW_QUERY_STRING": {
        "pattern": r'sqlx::query\(\s*"[^"]*"',
        "severity": "LOW",
        "suggestion": "Consider typed queries or query builder for portability"
    },

    # LLM
    "LLM_ANTHROPIC_SPECIFIC": {
        "pattern": r"(anthropic::|Anthropic|ClaudeClient|claude-|anthropic\.com)",
        "severity": "HIGH",
        "suggestion": "Use LlmProvider trait abstraction"
    },
    "LLM_OPENAI_SPECIFIC": {
        "pattern": r"(openai::|OpenAI|gpt-[34]|openai\.com)",
        "severity": "HIGH",
        "suggestion": "Use LlmProvider trait abstraction"
    },
    "LLM_HARDCODED_MODEL": {
        "pattern": r'model\s*[=:]\s*"(claude|gpt|llama)',
        "severity": "MEDIUM",
        "suggestion": "Model selection should come from config"
    },

    # File Storage
    "STORAGE_LOCAL_ONLY": {
        "pattern": r"std::fs::(read|write|create|remove|rename)",
        "context": "library code (not CLI/tools)",
        "severity": "MEDIUM",
        "suggestion": "Use StorageBackend trait for portability"
    },
    "STORAGE_HARDCODED_PATH": {
        "pattern": r'(~/.casparian|/tmp/|C:\\|/var/)',
        "severity": "MEDIUM",
        "suggestion": "Paths should come from config"
    },

    # Configuration
    "CONFIG_HARDCODED_ENV": {
        "pattern": r'std::env::(var|var_os)\s*\(\s*"[A-Z_]+"',
        "severity": "LOW",
        "suggestion": "Use typed config struct with env loading"
    },

    # Queue/Messaging
    "QUEUE_TOKIO_DIRECT": {
        "pattern": r"tokio::sync::(mpsc|broadcast|watch)",
        "context": "library code (not internal runtime)",
        "severity": "LOW",
        "suggestion": "Consider MessageBus trait for external queue support"
    }
}
```

### 1.4 Scope File Schema

User customizes audit via `scope.md`. Below are required and optional fields.

**Required Fields:**
```markdown
# Abstraction Audit Scope

## Focus Domains (REQUIRED)
At least one domain must be selected:
- [x] Database
- [x] LLM
- [ ] File Storage
- [ ] Queue/Messaging
- [ ] Configuration
- [ ] Serialization
```

**Optional Fields:**
```markdown
## Exemptions (OPTIONAL)
# Paths to skip - intentionally platform-specific code
- crates/casparian/src/main.rs  # CLI entry, ok to use std::fs
- crates/*/tests/**             # Tests can use concrete types
- crates/*/benches/**           # Benchmarks can use concrete types

## Priority Overrides (OPTIONAL)
# Override auto-detected priority for specific paths
- crates/casparian_worker/: CRITICAL  # Will need Postgres support

## Severity Filter (OPTIONAL)
# Only report findings at or above this severity
minimum_severity: MEDIUM  # Options: LOW, MEDIUM, HIGH, CRITICAL

## Notes (OPTIONAL)
# Context for the audit
- SQLite → Postgres migration planned Q2 2026
- Multi-provider LLM support needed for enterprise customers
```

**Default Exemptions:**
If no exemptions specified, these are applied automatically:
- `**/tests/**` - Test code
- `**/benches/**` - Benchmark code
- `**/examples/**` - Example code
- `**/main.rs` - CLI entry points

---

## 2. Instance Roles

### 2.1 Analyst Instance (Engineer Equivalent)

**Role:** Platform coupling identifier and abstraction proposer

**Responsibilities:**
- Scan source code for platform-specific patterns
- Classify violations by domain and severity
- Propose specific abstractions with trait definitions
- Identify refactoring dependencies (what must change first)

**Persona Prompt:**
```
You are a Staff Engineer specializing in platform-agnostic architecture.
Your role is to analyze Rust code for platform coupling. You:

- Think in terms of abstraction boundaries: what should be behind a trait?
- Know when direct platform use is appropriate (CLI, tests, internal runtime)
- Identify coupling that blocks future platform support
- Propose trait-based abstractions that enable portability
- Consider migration path: how hard is it to add PostgreSQL/GPT/S3?

Analysis categories:
1. DATABASE_COUPLING - SQLite-specific types or SQL syntax
2. LLM_COUPLING - Vendor-specific LLM client or model references
3. STORAGE_COUPLING - Local filesystem without abstraction
4. CONFIG_COUPLING - Hardcoded environment or paths
5. QUEUE_COUPLING - Direct async channel use in library code
6. SERIALIZATION_COUPLING - Format-specific assumptions

For each finding:
- Location: file:line
- Category: one of above
- Current code: snippet
- Issue: what's coupled and why it matters
- Proposed abstraction: trait definition or pattern
- Migration effort: TRIVIAL/LOW/MEDIUM/HIGH
- Confidence: HIGH/MEDIUM/LOW
```

**Output Format:**
```markdown
## Finding: [FINDING-ID]

**Category:** DATABASE_COUPLING | LLM_COUPLING | ...
**Location:** `crates/foo/src/bar.rs:123-145`
**Severity:** CRITICAL | HIGH | MEDIUM | LOW
**Migration Effort:** TRIVIAL | LOW | MEDIUM | HIGH

### Current Code
```rust
[coupled code snippet]
```

### Issue
[What's platform-specific and why it blocks portability]

### Proposed Abstraction
```rust
// Trait definition
trait Database {
    async fn execute(&self, query: &str) -> Result<()>;
}

// Usage change
fn process(db: &impl Database) {
    db.execute("SELECT * FROM files").await?;
}
```

### Migration Path
1. [Step 1: Define trait]
2. [Step 2: Implement for SQLite]
3. [Step 3: Update callers]
4. [Step 4: Add new implementations later]

### Blocked By
- [Other findings that must be resolved first]
```

---

### 2.2 Validator Instance (Reviewer Equivalent)

**Role:** Abstraction soundness checker and effort verifier

**Responsibilities:**
- Verify proposed abstractions are practical
- Check that abstraction doesn't over-engineer
- Validate migration effort estimates
- Ensure abstractions don't leak platform details
- Flag when coupling is intentional/acceptable

**Persona Prompt:**
```
You are a Principal Engineer known for pragmatic abstractions. Your role is
to validate platform abstraction proposals. You:

- Question if abstraction is worth the complexity
- Check trait definitions don't leak platform details
- Verify migration estimates are realistic
- Identify when "direct use" is actually fine (tests, CLI, internal)
- Ask: "Will we ACTUALLY need this portability?"

Your validation should:
- APPROVE findings that are clearly needed and well-designed
- NEEDS_WORK if abstraction is over-engineered
- REJECT if coupling is intentional or abstraction not worth it
- EXEMPT if code is appropriately platform-specific (tests, CLI)

Never approve without verifying:
1. Abstraction doesn't leak platform details
2. Migration path is realistic
3. We actually need this portability (not speculative)
4. Complexity is justified by concrete benefit
```

**Output Format:**
```markdown
## Validation: [FINDING-ID]

**Verdict:** APPROVED | NEEDS_WORK | REJECTED | EXEMPT

### Abstraction Soundness
- Platform details hidden: [YES/NO/PARTIAL]
- Trait is minimal: [YES/NO - over-designed?]
- Migration path realistic: [YES/NO/UNCLEAR]

### Effort Verification
- Estimate plausible: [YES/NO]
- Dependency order correct: [YES/NO]
- Blocking items identified: [YES/NO]

### Issues Found
- **[ISSUE-ID]**: [Description]
  - Problem: [What's wrong with the proposal]
  - Suggestion: [How to improve]

### Exemption Reason (if EXEMPT)
[Why this coupling is acceptable - e.g., "CLI entry point, std::fs appropriate"]
```

### 2.2.1 EXEMPT Verdict Handling

When Validator marks a finding as EXEMPT:

1. **Not included** in actionable_findings.json (no implementation needed)
2. **Documented** in report.md under "Exempted Findings" section
3. **Exemption reason** must be provided (mandatory for EXEMPT verdict)
4. **Future audits** skip this location if exemption reason still applies

**Valid Exemption Reasons:**
- "CLI entry point - direct platform use appropriate"
- "Test code - concrete types acceptable"
- "Internal implementation detail - not public API"
- "Intentional optimization - abstraction would add overhead"

**Invalid Exemption Reasons (reject and re-analyze):**
- "Too hard to fix" - This is REJECTED, not EXEMPT
- "Not a priority" - Use severity downgrade instead
- No reason provided - EXEMPT requires justification

---

### 2.3 Coordinator Instance (Mediator Equivalent)

**Role:** Synthesis, prioritization, and actionable output generation

**Responsibilities:**
- Aggregate findings across domains
- Prioritize by migration urgency and effort
- Generate `actionable_findings.json` for Implementation Protocol
- Identify cross-cutting abstraction patterns
- Recommend phased migration approach

**Output Format:**
```markdown
## Abstraction Audit Report - Round [N]

### Executive Summary
- Total findings: X
- Approved (ready to implement): Y
- Needs work: Z
- Rejected/Exempt: W
- Domains affected: [Database, LLM, ...]

### Migration Priority Matrix

| Priority | Domain | Finding Count | Effort | Recommended Phase |
|----------|--------|---------------|--------|-------------------|
| P0 | Database | 5 | Medium | Phase 1 |
| P1 | LLM | 3 | Low | Phase 2 |
| P2 | Storage | 2 | High | Phase 3 |

### Quick Wins (Trivial Effort)
| ID | Location | Category | Description |
|----|----------|----------|-------------|
| ABS-001 | schema.rs | DATABASE | Replace SqliteRow with generic Row |

### Architectural Recommendations
1. **Database Abstraction**: Create `trait Database` with X methods
   - Affects: Y files
   - Enables: PostgreSQL, MySQL support

2. **LLM Provider Trait**: Create `trait LlmProvider`
   - Affects: Z files
   - Enables: GPT, Llama, local models

### Phased Migration Plan
**Phase 1 (Database Foundation):**
- [ ] Define Database trait
- [ ] Implement for SQLite
- [ ] Update casparian_schema
- [ ] Update casparian_scout

**Phase 2 (LLM Abstraction):**
- [ ] Define LlmProvider trait
- [ ] Implement for Anthropic

### actionable_findings.json Generated
Location: {session}/actionable_findings.json
Count: Y findings ready for implementation
```

---

## 3. Actionable Findings Output

### 3.1 Finding Categories

```rust
enum AbstractionFindingCategory {
    DatabaseCoupling,      // SQLite-specific types/SQL
    LlmCoupling,           // Vendor-specific LLM code
    StorageCoupling,       // Local-only file operations
    ConfigCoupling,        // Hardcoded env/paths
    QueueCoupling,         // Direct async channel use
    SerializationCoupling, // Format-specific assumptions
}
```

### 3.2 Dependency Resolution

Findings can have dependencies via `blocked_by` and `blocks` fields:

**Cycle Detection:**
Before generating actionable_findings.json, Coordinator detects cycles:
```
1. Build directed graph: finding_id → blocked_by
2. Run topological sort
3. If cycle detected (A blocks B blocks A):
   - Log warning in report.md
   - Break cycle by removing lowest-severity edge
   - Mark affected findings with note: "Dependency cycle broken"
```

**Execution Order:**
Implementation Protocol processes findings in topological order:
1. Findings with no blockers (blocked_by: []) execute first
2. Findings execute only after all blockers are RESOLVED
3. If blocker is REJECTED, blocked finding becomes unblocked

**Example Dependency Chain:**
```
ABS-2026-001 (Database trait) ← ABS-2026-002 (Use trait in storage.rs)
                              ← ABS-2026-003 (Use trait in schema.rs)
```
Execute ABS-2026-001 first, then 002/003 can run in parallel.

### 3.3 Example actionable_findings.json

```json
{
  "workflow": "abstraction_audit_workflow",
  "session": "abstraction_audit_001",
  "round": 1,
  "generated_at": "2026-01-14T10:00:00Z",
  "findings": [
    {
      "id": "ABS-2026-001",
      "source_workflow": "abstraction_audit_workflow",
      "source_round": "round_001",
      "file_path": "crates/casparian_schema/src/storage.rs",
      "line_start": 45,
      "line_end": 52,
      "category": "DatabaseCoupling",
      "severity": "HIGH",
      "confidence": "HIGH",
      "title": "SqlitePool used directly in public API",
      "description": "Function signature uses SqlitePool, blocking PostgreSQL support",
      "current_code": "pub async fn init_db(pool: &SqlitePool) -> Result<()>",
      "suggested_fix": "pub async fn init_db<DB: Database>(pool: &Pool<DB>) -> Result<()>",
      "blocks": [],
      "blocked_by": ["ABS-2026-002"],
      "related_files": [
        "crates/casparian_schema/src/contract.rs",
        "crates/casparian_schema/src/approval.rs"
      ],
      "verify_command": "cargo check -p casparian_schema",
      "expected_outcome": "Compiles with generic DB parameter"
    },
    {
      "id": "ABS-2026-002",
      "source_workflow": "abstraction_audit_workflow",
      "source_round": "round_001",
      "file_path": "crates/casparian_schema/src/lib.rs",
      "line_start": 10,
      "line_end": 15,
      "category": "DatabaseCoupling",
      "severity": "HIGH",
      "confidence": "HIGH",
      "title": "Database trait not defined",
      "description": "No abstraction exists for database operations",
      "current_code": null,
      "suggested_fix": "// Add to lib.rs\npub trait Database: Send + Sync {\n    type Connection;\n    async fn acquire(&self) -> Result<Self::Connection>;\n}\n\nimpl Database for SqlitePool { ... }",
      "blocks": ["ABS-2026-001"],
      "blocked_by": [],
      "related_files": [],
      "verify_command": "cargo check -p casparian_schema",
      "expected_outcome": "New trait compiles"
    }
  ]
}
```

### 3.3 Execution Metrics Output

Per `workflow_manager.md` Section 7.4, emit metrics for Manager learning.

**Output Location:** `{session}/execution_metrics.json`

**Schema:**
```json
{
  "session_id": "abstraction_audit_001",
  "workflow": "abstraction_audit_workflow",
  "started_at": "2026-01-14T10:00:00Z",
  "completed_at": "2026-01-14T11:30:00Z",
  "rounds_executed": 2,
  "domains_audited": ["Database", "LLM"],
  "findings": {
    "total_identified": 15,
    "approved": 10,
    "needs_work": 2,
    "rejected": 1,
    "exempt": 2
  },
  "by_category": {
    "DatabaseCoupling": 8,
    "LlmCoupling": 5,
    "StorageCoupling": 2
  },
  "outcome": "COMPLETE",
  "actionable_findings_generated": true,
  "implementation_requested": false
}
```

---

## 4. Integration with feature_workflow

### 4.1 Shared Pattern Library

Both workflows use `ABSTRACTION_PATTERNS` from Section 1.3. This ensures:
- **Audit** catches existing violations
- **Feature workflow** prevents new violations
- **Single source of truth** for what's considered coupling

### 4.2 feature_workflow Integration Point

In `feature_workflow.md` Section 6 (Validate), add abstraction check:

```
### 6.X Abstraction Check (Incremental)

Run ONLY on changed files, using patterns from abstraction_audit_workflow:

IF changed_files INTERSECTS database_files:
    abstraction_check = check_patterns(changed_files, ABSTRACTION_PATTERNS["DB_*"])

IF changed_files INTERSECTS llm_files:
    abstraction_check = check_patterns(changed_files, ABSTRACTION_PATTERNS["LLM_*"])

Patterns are imported from abstraction_audit_workflow.md Section 1.3
```

### 4.3 Pattern Import Mechanism

The patterns in Section 1.3 are the **single source of truth**. Both workflows reference them:

**Option B (Explicit Reference)** - Currently implemented:
- `abstraction_audit_workflow.md` Section 1.3 defines `ABSTRACTION_PATTERNS`
- `feature_workflow.md` Section 6.6 explicitly states: "Patterns are imported from abstraction_audit_workflow.md Section 1.3"
- Implementers read this spec to get patterns

**Why This Works:**
- Patterns are prose/pseudocode, not executable code
- Human (or LLM) interprets patterns when running workflow
- No runtime import needed - both specs point to same definition

**Pattern Update Protocol:**
When patterns need updating:
1. Edit Section 1.3 of this spec
2. Update version number in Section 10
3. feature_workflow automatically uses new patterns (same reference)
4. Run `spec_maintenance_workflow` to verify cross-references still valid

### 4.4 No Duplication

The audit workflow runs once (or periodically) to **clean up existing debt**.
The feature workflow runs on every change to **prevent new debt**.
Same patterns, different scope:

```
                    ┌───────────────────────────────────────┐
                    │       ABSTRACTION_PATTERNS            │
                    │       (Single Source of Truth)        │
                    └───────────────┬───────────────────────┘
                                    │
           ┌────────────────────────┴────────────────────────┐
           │                                                  │
           ▼                                                  ▼
┌─────────────────────────┐                    ┌─────────────────────────┐
│ abstraction_audit_workflow │                    │    feature_workflow      │
│                           │                    │                         │
│ Full codebase scan        │                    │ Changed files only      │
│ Run once / periodically   │                    │ Run on every feature    │
│ Outputs: actionable_      │                    │ Blocks: new violations  │
│          findings.json    │                    │                         │
│                           │                    │                         │
│ Purpose: FIX EXISTING     │                    │ Purpose: PREVENT NEW    │
└─────────────────────────────┘                    └─────────────────────────┘
```

---

## 5. Process Flow

### 5.1 Full Audit Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ROUND N FLOW                                │
│                                                                     │
│  ┌──────────┐     ┌──────────┐     ┌───────────┐     ┌──────────┐  │
│  │ Analyst  │ ──► │ Validator│ ──► │Coordinator│ ──► │   User   │  │
│  │          │     │          │     │           │     │          │  │
│  │  Find    │     │  Verify  │     │  Generate │     │  Review  │  │
│  │  Coupling│     │  Sound   │     │ Findings  │     │  & Fix   │  │
│  └──────────┘     └──────────┘     └───────────┘     └──────────┘  │
│       │                                                    │        │
│       └───────────────── ROUND N+1 ◄──────────────────────┘        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.2 Step-by-Step Process

**Step 0: Scope Definition**
1. User specifies domains to audit (Database, LLM, etc.)
2. User provides priority overrides or exemptions
3. Write `scope.md` with targets

**Step 1: Analyst Phase**
1. Read scope from `scope.md`
2. Read previous round findings (if any) to avoid duplicates
3. Scan target code using `ABSTRACTION_PATTERNS` from Section 1.3
4. For each match:
   - Verify it's in library code (not tests/CLI)
   - Assess migration effort (TRIVIAL/LOW/MEDIUM/HIGH)
   - Propose trait-based abstraction
5. Write `round_N/analyst.md` with findings

**Step 2: Validator Phase**
1. Read `round_N/analyst.md`
2. For each finding:
   - Verify abstraction is sound (doesn't leak platform details)
   - Check migration effort estimate is realistic
   - Determine if coupling is intentional (EXEMPT) or not
3. Assign verdict: APPROVED | NEEDS_WORK | REJECTED | EXEMPT
4. Write `round_N/validator.md` with verdicts

**Step 3: Coordinator Phase**
1. Read Analyst and Validator outputs
2. Filter findings:
   - APPROVED → Include in actionable_findings.json
   - NEEDS_WORK → Return to Analyst next round (see Section 5.4)
   - REJECTED → Document reason, exclude
   - EXEMPT → Document exemption, exclude
3. Sort approved findings by dependency order (blocked_by first)
4. Generate prioritized `actionable_findings.json`
5. Update `status.md` with counts
6. Write `round_N/report.md`

**Step 4: User Phase**
1. Review report
2. Choose: "Implement all" | "Implement selected" | "Request deeper analysis"
3. If deeper analysis requested → trigger Round N+1
4. If implementing → hand off to Implementation Protocol (workflow_manager.md Section 13)

### 5.3 Typical Session

1. **User triggers audit**: "Run abstraction audit focusing on database"
2. **Analyst scans** codebase for `DB_*` patterns
3. **Validator reviews** each finding for soundness
4. **Coordinator generates** `actionable_findings.json`
5. **User reviews** findings: "Implement these" or "Skip for now"
6. **Implementation Protocol** applies fixes from findings

### 5.4 NEEDS_WORK Handling

When Validator marks a finding as NEEDS_WORK:

1. Finding returns to Analyst queue for next round
2. Analyst refines based on Validator's feedback
3. If still NEEDS_WORK after 2 rounds:
   - Include in actionable_findings.json with `confidence: "MEDIUM"`
   - Add note: "Requires human review before implementation"
4. Track NEEDS_WORK iterations in status.md

### 5.5 Termination Criteria

**Complete when:**
1. All selected domains audited
2. No new findings in last round
3. `actionable_findings.json` generated
4. User satisfied with coverage

---

## 6. Integration with Claude Code

### 6.1 Invocation

User triggers via natural language:
```
"Run the abstraction audit workflow on the database layer"
"Start an abstraction audit focusing on LLM coupling"
"Continue the abstraction audit from session abstraction_audit_001"
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
│  5. Generate actionable_findings.json                               │
│                              │                                      │
│  6. Present findings to user via summary                            │
│                              │                                      │
│  7. AskUserQuestion for next steps                                  │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.3 Analyst Prompt Template

```
You are the Analyst instance in an abstraction audit workflow.

## Scope
- Workspace: {workspace_root}
- Session: {session_name}
- Round: {round_number}
- Domains to audit: {domains}  # e.g., ["Database", "LLM"]
- Priority filter: {priority}  # e.g., "HIGH and above"
- Exemptions: {exemption_paths}  # e.g., ["**/tests/**", "**/main.rs"]
- Previous findings to skip: {already_found_ids}

## Your Task
Analyze the codebase for platform-specific coupling that blocks portability.

Patterns to check (from ABSTRACTION_PATTERNS):
{patterns_for_domains}

For each finding, provide:
- Finding ID: ABS-{year}-{sequence}  # e.g., ABS-2026-001
- Location (file:line)
- Category: DATABASE_COUPLING | LLM_COUPLING | STORAGE_COUPLING | CONFIG_COUPLING | QUEUE_COUPLING
- Current code snippet
- Issue description (what's coupled and why)
- Proposed abstraction (trait definition or pattern)
- Migration effort: TRIVIAL | LOW | MEDIUM | HIGH
- Confidence: HIGH | MEDIUM | LOW
- Blocked by: [list of other finding IDs this depends on]

Write output to: specs/meta/sessions/{session}/round_{round}/analyst.md

Focus on high-impact findings. Quality over quantity.
Skip code in exemption paths - those are intentionally platform-specific.
```

### 6.4 Validator Prompt Template

```
You are the Validator instance in an abstraction audit workflow.

## Context
- Session: {session_name}
- Round: {round_number}
- Analyst findings: [attached or path to analyst.md]

## Your Task
Validate each finding for soundness and practicality.

For each finding, determine:
1. **Safety** - Does the proposed abstraction leak platform details?
2. **Practicality** - Is the migration effort estimate realistic?
3. **Necessity** - Do we actually need this portability, or is it speculative?
4. **Exemption** - Is the coupling intentional and acceptable?

Assign verdict:
- **APPROVED** - Sound abstraction, realistic effort, needed portability
- **NEEDS_WORK** - Good idea but proposal needs refinement
- **REJECTED** - Abstraction not worth complexity, or coupling is fine
- **EXEMPT** - Code is appropriately platform-specific (tests, CLI, internal)

For EXEMPT verdicts, explain why the coupling is acceptable.
For NEEDS_WORK verdicts, specify what needs to change.

Write output to: specs/meta/sessions/{session}/round_{round}/validator.md

Be rigorous but pragmatic. Not every coupling needs an abstraction.
```

### 6.5 Session ID Generation

Session IDs follow the pattern: `abstraction_audit_{NNN}`

Where NNN is a zero-padded sequence number. To generate:
1. List existing session folders in `specs/meta/sessions/`
2. Find highest `abstraction_audit_*` number
3. Increment by 1

Example: If `abstraction_audit_001` exists, next session is `abstraction_audit_002`.

---

## 7. Document Structure

```
specs/meta/
├── abstraction_audit_workflow.md  # THIS FILE (read-only reference)
├── sessions/
│   └── abstraction_audit_001/     # One folder per audit session
│       ├── scope.md               # Which domains/files to analyze
│       ├── round_001/
│       │   ├── analyst.md         # Analyst's findings
│       │   ├── validator.md       # Validator's review
│       │   └── report.md          # Coordinator's synthesis
│       ├── actionable_findings.json  # Ready for Implementation Protocol
│       ├── execution_metrics.json    # For Manager learning
│       └── status.md              # Finding counts, progress tracking
```

### 7.1 status.md Schema

```markdown
# Abstraction Audit Session: {session_id}

**Target:** Codebase platform coupling
**Session Started:** {date}
**Current Round:** {N}
**Status:** IN_PROGRESS | COMPLETE | ABANDONED

---

## Session Overview

| Metric | Value |
|--------|-------|
| Domains Audited | Database, LLM |
| Rounds Completed | {N} |
| Total Findings | {count} |
| Approved | {count} |
| Needs Work | {count} |
| Rejected | {count} |
| Exempt | {count} |

---

## Round History

### Round 1
- **Phase:** {Analyst | Validator | Coordinator | Complete}
- **Findings Identified:** {N}
- **Artifacts:**
  - [x] `round_001/analyst.md`
  - [x] `round_001/validator.md`
  - [x] `round_001/report.md`

### Round 2
...
```

---

## 8. Integration with Workflow Manager

### 8.1 Registration

Add to `workflow_manager.md` Section 3.2:

```
| `abstraction_audit_workflow` | 3-instance (Analyst, Validator, Coordinator) | Platform coupling audit | 2-4 rounds |
```

### 8.2 Routing Keywords

```
ABSTRACTION_KEYWORDS = [
    "abstraction", "platform", "portable", "database", "duckdb", "postgres",
    "llm", "provider", "anthropic", "openai", "coupling", "modular"
]
```

### 8.3 Category Registration

Add to `workflow_manager.md` Section 13.2:

```
| `abstraction_audit_workflow` | DatabaseCoupling, LlmCoupling, StorageCoupling, ConfigCoupling, QueueCoupling | 5-25 |
```

---

## 9. Example Session

### 9.1 Invocation

```
User: "Run abstraction audit on database coupling"

Manager: Creating abstraction_audit_001 session...
         Spawning Analyst for database domain...
```

### 9.2 Analyst Output (Excerpt)

```markdown
## Finding: ABS-2026-001

**Category:** DATABASE_COUPLING
**Location:** `crates/casparian_schema/src/storage.rs:45-52`
**Severity:** HIGH
**Migration Effort:** MEDIUM
**Confidence:** HIGH

### Current Code
```rust
use sqlx::SqlitePool;

pub struct SchemaStorage {
    pool: SqlitePool,  // Direct SQLite reference
}
```

### Issue
`SqlitePool` in public struct prevents PostgreSQL support. Callers are tied to SQLite.

### Proposed Abstraction
```rust
use sqlx::{Pool, Database};

pub struct SchemaStorage<DB: Database> {
    pool: Pool<DB>,
}

impl<DB: Database> SchemaStorage<DB> {
    pub fn new(pool: Pool<DB>) -> Self { ... }
}
```

### Migration Path
1. Add generic parameter to struct
2. Update impl blocks
3. Update callers to specify <Sqlite>
4. Later: Add <Postgres> implementations
```

### 9.3 Validator Output (Excerpt)

```markdown
## Validation: ABS-2026-001

**Verdict:** APPROVED

### Abstraction Soundness
- Platform details hidden: YES - generic Pool<DB> hides SQLite
- Trait is minimal: YES - only adds type parameter
- Migration path realistic: YES - callers specify <Sqlite> initially

### Effort Verification
- Estimate plausible: YES - MEDIUM effort for struct generification
- Dependency order correct: YES - no blockers
- Blocking items identified: N/A

### Issues Found
None. Clean abstraction proposal.
```

### 9.4 Generated actionable_findings.json

After validation, Coordinator generates findings file ready for Implementation Protocol.

---

## 10. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial workflow specification |
| 2026-01-14 | 1.1 | **spec_refinement applied**: Added Section 5.2 step-by-step process, Section 5.4 NEEDS_WORK handling, Section 6 Claude Code integration with prompt templates (6.3-6.4), Section 6.5 session ID generation, Section 3.2 dependency resolution, Section 3.3 execution_metrics output, Section 2.2.1 EXEMPT handling, Section 4.3 pattern sharing mechanism, Section 7.1 status.md schema. Standardized finding IDs to ABS-YYYY-NNN format. Enhanced Section 1.4 with scope file schema (required/optional fields). |

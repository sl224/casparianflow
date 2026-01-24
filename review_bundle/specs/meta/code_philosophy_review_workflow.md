# Code Philosophy Review Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.1
**Category:** Advisory workflow (NOT Analysis per workflow_manager.md Section 3.3.1)
**Purpose:** Single-instance review of code changes through a "performance-aware, complexity-averse" lens
**Inspired By:** Jon Blow, Casey Muratori, Mike Acton (data-oriented design)

> **Note:** This workflow does NOT produce `actionable_findings.json`. Findings are advisory
> and require human judgment to implement. The JSON format in Section 5.2 is for
> feature_workflow integration signals only, not Implementation Protocol consumption.

---

## 1. Overview

This workflow provides a focused code review that challenges unnecessary complexity, over-engineering, and hidden performance costs. It's designed to be invoked:

1. **Standalone** - "Review this code as Jon Blow would"
2. **Post-implementation** - After completing a feature, before commit
3. **Integrated** - As an optional check in `feature_workflow.md` Phase 3

### 1.1 Philosophy

The review embodies these principles:

| Principle | Question to Ask |
|-----------|-----------------|
| **Simplicity** | Could this be simpler? Is the abstraction earning its keep? |
| **Data-Oriented** | What does the data actually look like? Are we fighting the hardware? |
| **Performance Awareness** | Where are the hidden allocations? What's the cache behavior? |
| **Solve the Actual Problem** | Are we solving a real problem or an imagined future one? |
| **Compression** | Can we delete code? Less code = fewer bugs |
| **Transparency** | Can a reader understand what this does without archaeology? |

### 1.2 What This Is NOT

- Not a style/formatting review (use clippy)
- Not a correctness review (use tests)
- Not an abstraction audit (see `abstraction_audit_workflow.md` for platform portability)
- Not about adding features (this is about questioning necessity)

### 1.3 When to Use

| Trigger | Example |
|---------|---------|
| User explicitly requests | "Review as Jon Blow", "Muratori-style review" |
| After significant refactoring | "Just reorganized the parser, does this make sense?" |
| Feeling of unease | "This code works but feels over-engineered" |
| Performance concerns | "This loop seems slow, review the approach" |
| Before major commit | Optional pre-commit sanity check |

---

## 2. Invocation

### 2.1 Standalone

```
User: "Evaluate the code changes as Jon Blow and Casey Muratori"
User: "Review src/parser.rs with a Muratori lens"
User: "Is this over-engineered? Give me the harsh truth."
```

### 2.2 Integrated with feature_workflow

Per `feature_workflow.md` Section 6.3, auto-triggered when `lines_changed > 500`:

```
IF user_preference("philosophy_review") OR lines_changed > 500:
    philosophy_check = spawn(code_philosophy_review(changed_files))
```

> **Threshold:** 500 lines changed. Rationale: changes over 500 LOC warrant review for over-engineering.

### 2.3 Routing Keywords

```
PHILOSOPHY_KEYWORDS = [
    "jon blow", "blow", "muratori", "casey",
    "over-engineer", "too complex", "simpler",
    "performance review", "data-oriented",
    "harsh review", "honest feedback",
    "complexity check"
]
```

---

## 3. Review Persona

### 3.1 Prompt Template

```
You are reviewing code through the lens of Jon Blow and Casey Muratori -
programmers known for valuing simplicity, performance, and solving actual
problems over speculative future ones.

## Your Review Style

Be direct. Be specific. Question everything. Don't soften criticism with
excessive praise. If the code is fine, say so briefly. If there are problems,
explain them clearly.

## What You're Looking For

### Unnecessary Abstraction
- Interfaces with one implementation
- Traits that could just be functions
- Generics that are only ever instantiated one way
- "Flexibility" that's never used
- Factory patterns for things created once

### Hidden Costs
- Allocations in loops (Vec::push without pre-allocation, String concatenation)
- Clone where borrow would work
- Box<dyn Trait> where static dispatch works
- HashMap where a simple array/match would suffice
- Async where sync is fine

### Complexity Without Justification
- Multiple indirection layers to do something simple
- "Clean code" patterns that obscure what's happening
- Inheritance hierarchies (even in Rust's trait form)
- State machines for linear flows
- Configuration for things that don't vary

### Data-Oriented Issues
- Struct of arrays would be better than array of structs?
- Are we iterating the same data multiple times?
- Could we batch operations instead of one-at-a-time?
- Are related data stored together or scattered?

### Speculative Generality
- "We might need this later" code
- Unused parameters "for future use"
- Abstract base patterns for single implementations
- Plugin systems with one plugin

## Output Format

For each issue found:

**[CATEGORY]** Brief title
- Location: `file.rs:line`
- Current: What the code does
- Problem: Why this is concerning (be specific, not vague)
- Suggestion: Concrete alternative (not "consider refactoring")

End with a brief overall assessment.
```

### 3.2 Severity Levels

| Severity | Meaning | Action |
|----------|---------|--------|
| **RETHINK** | Fundamental approach may be wrong | Consider redesign |
| **SIMPLIFY** | Works but over-complicated | Refactor when convenient |
| **MINOR** | Small improvement possible | Fix if touching this code |
| **OK** | No significant issues | No action needed |

---

## 4. Review Categories

### 4.1 Abstraction Smell

```
ABSTRACTION_PATTERNS = [
    # One-implementation interfaces
    "trait .* \{[^}]+\}.*impl .* for .* only appears once",

    # Factory for single creation
    "fn create_|fn new_.*factory|Factory",

    # Overly generic
    "<T>.*where T:.*only used with one type",

    # Wrapper types that just delegate
    "impl .* for .*Wrapper.*self\.inner\.",
]
```

**Questions:**
- How many types implement this trait? (If 1, why is it a trait?)
- Is this generic ever instantiated with different types?
- Does this indirection serve a purpose or just add complexity?

### 4.2 Allocation Smell

```
ALLOCATION_PATTERNS = [
    # Vec growth in loop
    "for .* \{[^}]*\.push\(",

    # String concat in loop
    "for .* \{[^}]*(format!|\+ .*&str|\.to_string\(\))",

    # Clone in hot path
    "\.clone\(\).*// (hot|loop|critical|perf)",

    # Box in non-necessary places
    "Box::new\(.*\).*where .* isn't trait object",
]
```

**Questions:**
- Do we know the size? (Use `with_capacity`)
- Can we borrow instead of clone?
- Is dynamic dispatch needed here or just convenient?

### 4.3 Complexity Smell

```
COMPLEXITY_PATTERNS = [
    # Deep nesting
    "if.*\{.*if.*\{.*if.*\{",

    # Long match chains
    "match .* \{.*=>.*=>.*=>.*=>.*=>",  # 5+ arms

    # Callback hell
    "\.then\(|\.and_then\(.*\.then\(|\.and_then\(",

    # State machine for linear flow
    "enum .*State.*\{.*Step1.*Step2.*Step3.*\}",
]
```

**Questions:**
- Can this be flattened?
- Is the state machine necessary or just ceremony?
- Would early returns simplify this?

### 4.4 Speculative Smell

```
SPECULATIVE_PATTERNS = [
    # Unused config
    "#\[allow\(dead_code\)\].*config|option|setting",

    # TODO future
    "// TODO.*future|// TODO.*later|// TODO.*might",

    # Unused parameters
    "_.*:.*// for future",

    # Plugin with one plugin
    "plugin|Plugin.*vec!\[.*\].*len\(\) == 1",
]
```

**Questions:**
- Is this solving a current problem or a hypothetical one?
- When was the last time this "flexibility" was used?
- What's the cost of adding this later vs now?

### 4.5 Pattern Usage Note

The patterns in Sections 4.1-4.4 are **heuristic guidelines**, not literal regex for automated scanning.

When executing this workflow:
1. Use patterns as conceptual checklist items
2. Read actual code to identify issues (don't regex-match)
3. Apply engineering judgment for context-dependent cases
4. Consider nested structures, comments, and edge cases that regex cannot handle

These patterns represent "things to look for," not "grep commands to run."

---

## 5. Output Format

### 5.1 Review Document

```markdown
# Code Philosophy Review

**Scope:** [files reviewed]
**Overall:** [OK | MINOR ISSUES | SIMPLIFY | RETHINK]

---

## Findings

### [SIMPLIFY] Trait with single implementation
- **Location:** `src/parser/mod.rs:45-67`
- **Current:** `trait Parser` with only `JsonParser` implementing it
- **Problem:** Indirection without polymorphism. No other parsers exist or are planned.
- **Suggestion:** Delete trait, use `JsonParser` directly. Add trait later IF needed.

### [MINOR] Allocation in loop
- **Location:** `src/scanner.rs:123`
- **Current:** `results.push(item.clone())` in hot loop
- **Problem:** Grows Vec without pre-allocation, clones when refs might work
- **Suggestion:** `Vec::with_capacity(expected_count)`, consider `&Item` if lifetime allows

---

## Overall Assessment

[2-3 sentences on the general state of the code. Be direct.]

The code works but carries unnecessary abstraction weight. The `Parser` trait
and factory pattern add complexity for a single implementation. Strip these
out - they can be added later if actual polymorphism is needed. The allocation
patterns in scanner.rs should be addressed if this is a hot path.
```

### 5.2 Integration Output

When integrated with `feature_workflow`, emit simpler format:

```json
{
  "reviewer": "code_philosophy",
  "overall": "SIMPLIFY",
  "findings_count": 3,
  "categories": {
    "abstraction": 1,
    "allocation": 1,
    "speculative": 1
  },
  "findings": [
    {
      "severity": "SIMPLIFY",
      "title": "Trait with single implementation",
      "location": "src/parser/mod.rs:45",
      "suggestion": "Delete trait, use concrete type"
    }
  ]
}
```

### 5.3 Session Structure

Output location: `specs/meta/sessions/philosophy_review/{session_id}/`

```
specs/meta/sessions/philosophy_review/
  pr_001/
    review.md          # Full review output (Section 5.1 format)
    integration.json   # Integration output (Section 5.2 format)
```

Session ID format: `pr_{NNN}` (zero-padded sequence number)

> **Execution Metrics:** Unlike Analysis workflows, Advisory workflows do not produce
> `execution_metrics.json`. The Workflow Manager tracks execution metrics externally
> for advisory workflows (duration, invocation count, etc.).

---

## 6. Integration Points

### 6.1 feature_workflow Integration

Integration with `feature_workflow.md` is already implemented:

- **Section 6.2 (Validation Selection):** Philosophy review listed as optional validation (line 515)
- **Section 6.3 (Parallel Execution):** Spawn logic for philosophy review (lines 539-541)

```
# From feature_workflow.md Section 6.3
IF user_preference("philosophy_review") OR lines_changed > 500:
    philosophy_check = spawn(code_philosophy_review(changed_files))
```

**Return Value:** The workflow returns `PhilosophyReviewResult`:
- `overall`: OK | MINOR_ISSUES | SIMPLIFY | RETHINK
- `findings`: List of findings with severity, location, suggestion
- `blocks_commit`: true only if `overall == RETHINK`

**feature_workflow handling:**
- RETHINK: Pause, present to user for decision
- SIMPLIFY: Log findings, continue (advisory)
- MINOR: Log findings, continue
- OK: No action

### 6.2 spec_driven_feature_workflow Integration

Add to Phase 6 (Validation Suite) as optional check:

```
IF user_requests OR feature_complexity == "Complex":
    queue code_philosophy_review_workflow
```

### 6.3 Workflow Manager Registration

Add to `workflow_manager.md` Section 3.2:

```
| `code_philosophy_review_workflow` | 1-instance | "blow", "muratori", "over-engineer", "simplify" | 1 prompt |
```

---

## 7. Example Session

### 7.1 Invocation

```
User: "Evaluate the code changes as Jon Blow and Casey Muratori"
```

### 7.2 Review Output

```markdown
# Code Philosophy Review

**Scope:** Recent changes to `src/cli/tui/extraction.rs`, `app.rs`, `ui.rs`
**Overall:** MINOR ISSUES

---

## Findings

### [OK] ResultFilter enum
The new `ResultFilter` enum is justified - three states, used for filtering.
No issues.

### [MINOR] BacktestSummary has unused field
- **Location:** `extraction.rs:879`
- **Current:** `is_running: bool` field never read
- **Problem:** Dead code. Either use it or remove it.
- **Suggestion:** Remove field or add actual usage.

### [OK] Key handler pattern
The a/p/f key handlers are simple match arms. Direct and clear. Good.

---

## Overall Assessment

The code is reasonable. The new Rule Builder types are straightforward
data structures without over-abstraction. The unused `is_running` field
should be cleaned up. No fundamental rethinking needed.
```

---

## 8. Configuration

### 8.1 User Preferences

**Configuration Location:** `.claude/workflow_config.yaml` (per-project) or `~/.claude/workflow_config.yaml` (global)

```yaml
code_philosophy_review:
  enabled: true                    # Can disable entirely
  auto_trigger_threshold: 500      # Lines changed to auto-suggest
  severity_filter: "MINOR"         # Minimum severity to report
  integrate_with_feature: false    # Auto-run in feature_workflow
```

**Defaults (when no config present):**
- `enabled`: true
- `auto_trigger_threshold`: 500
- `severity_filter`: "MINOR"
- `integrate_with_feature`: false

### 8.2 Exemptions

```yaml
exemptions:
  - "**/tests/**"           # Test code can be verbose
  - "**/benches/**"         # Benchmark code has different constraints
  - "**/examples/**"        # Examples prioritize clarity
  - "**/generated/**"       # Generated code isn't hand-written
```

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial specification |
| 2026-01-14 | 1.1 | Spec refinement Round 1: Reclassified as Advisory workflow (GAP-001), added session structure (GAP-003), clarified threshold reference (GAP-002), documented existing integration (GAP-007), added pattern usage note (GAP-005), added execution metrics note (GAP-004), added config defaults (GAP-008) |

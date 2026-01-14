# Reviewer Round 007: GAP-INT-002

## Review: GAP-INT-002

### Summary

The Engineer proposes a deterministic, pattern-classification decision tree for the YAML vs Python output decision in Pathfinder Wizard. The algorithm classifies each detected pattern as YAML_OK or PYTHON_REQUIRED, with the "most complex" pattern determining the output format (any PYTHON_REQUIRED forces entire output to Python).

### Critical Issues

**ISSUE-R7-001: `analyze_complexity()` function is undefined**

The core classification logic depends on `analyze_complexity(pattern)` but this function is never defined. The proposal lists what properties it returns (`has_computed_fields`, `has_conditional_logic`, etc.) but not HOW these are determined.

```python
complexity = analyze_complexity(pattern)

if complexity.has_computed_fields:
    # ...
```

This is the heart of the algorithm. Without specifying how `analyze_complexity` works, the proposal is incomplete. How does the system know that `Q1` implies computation vs simple extraction?

**Suggested fix:** Define `analyze_complexity()` with concrete rules:
- What path structures trigger `has_computed_fields`?
- How does it detect "computed" vs "extractable"?
- Provide the `Complexity` dataclass definition with all fields.

---

**ISSUE-R7-002: Conflicting classification for quarter extraction**

The proposal contains inconsistent classification of quarter patterns:

In Section "Edge Case 3: Type Coercion vs Computation":
```
| Input | Desired Output | Classification | Reason |
| "Q1"  | `1` (quarter number) | YAML_OK | Regex `Q(\d)` + type integer |
```

But in Example 2 and the Edge Cases section:
```
User Hint: "Quarter folder should compute start/end month"
...
| segment(-2) | Q1 | PYTHON_REQUIRED | User hint: compute start/end month |
```

The issue: The classification depends on user intent (extracting quarter number vs computing month range), but the algorithm as written classifies based on the **pattern** alone before considering hints. The `classify_pattern()` function checks hints first, but the pattern classification table implies inherent properties.

**Suggested fix:** Clarify that `Q1 -> 1` is YAML_OK by default, but user hints can override. Add explicit statement: "Default classification can be escalated by user hints."

---

### High Priority

**ISSUE-R7-003: Missing definition of `hints_require_python()` and `get_hint_python_reason()`**

The proposal references these functions:
```python
if hints_require_python(pattern, user_hints):
    return ClassifiedPattern(
        pattern=pattern,
        classification=PatternClassification.PYTHON_REQUIRED,
        python_reason=get_hint_python_reason(pattern, user_hints)
    )
```

But only provides example keywords ("compute", "calculate", etc.) without a concrete algorithm. Is this keyword matching, LLM classification, or regex?

Per the proposal:
> "How exactly do we detect computation keywords in natural language hints?" (GAP-INT-003)

This is acknowledged as a new gap, but the proposal should at minimum specify the mechanism (e.g., "keyword matching initially, LLM classification deferred to GAP-INT-003").

---

**ISSUE-R7-004: Edge Case 4 contradicts the decision rule**

Edge Case 4 states:
```
If regex becomes unreadable (>100 chars or >5 capture groups), recommend Python
with comment explaining why. But still classify as YAML_OK - user can choose
to accept complex YAML or edit to Python.
```

This introduces a **recommendation** layer that is separate from classification. However, the main algorithm only returns `ClassifiedPattern` with a binary `YAML_OK`/`PYTHON_REQUIRED` classification. How does the "recommend Python" guidance surface?

**Suggested fix:** Either:
1. Add a `YAML_COMPLEX` classification that triggers a user prompt, or
2. Add a `recommendations: List[str]` field to the decision output

---

**ISSUE-R7-005: `DetectedPattern` struct undefined**

The algorithm operates on `DetectedPattern` objects but never defines what they contain:
```python
def classify_pattern(pattern: DetectedPattern, user_hints: List[str]) -> ClassifiedPattern:
```

What fields does `DetectedPattern` have? Presumably at least:
- Source segment/path
- Raw value(s)
- Inferred type (if any)
- Regex captures

Without this definition, implementers cannot build the system.

---

### Medium Priority

**ISSUE-R7-006: No handling for empty pattern sets**

What happens when no patterns are detected from the sample path? The algorithm assumes `patterns` is non-empty:
```python
classified = [classify_pattern(p, user_hints) for p in patterns]
```

Edge Case 6 covers "tag-only rules" but the decision function should explicitly handle `len(patterns) == 0` and return YAML_OK with empty extract.

---

**ISSUE-R7-007: "Multi-step transformation" threshold unclear**

The proposal lists "Multi-step transformation" as PYTHON_REQUIRED:
```
| Multi-step transform | Extract -> decode base64 -> parse JSON |
```

But many YAML rules have implicit multi-step transforms:
```yaml
direction:
  from: segment(-4)
  pattern: "ADT_(Inbound|Outbound)"  # Step 1: regex capture
  capture: 1                          # Step 2: select group
  normalize: lowercase                # Step 3: lowercase
```

Is this 3 steps? What's the threshold for "multi-step"?

**Suggested fix:** Define "multi-step" as "transformation that cannot be expressed as a linear YAML pipeline" or "requires intermediate variables."

---

**ISSUE-R7-008: Stateful extraction requires clarification**

Listed as PYTHON_REQUIRED:
```
| Stateful extraction | Sequence numbers across files |
```

But extraction rules operate per-file. How would stateful extraction even work in the current architecture? This seems like a feature that doesn't exist yet.

**Suggested fix:** Either remove this case or note it as "future capability, currently not supported."

---

### Low Priority / Nits

**ISSUE-R7-009: Inconsistent naming in examples**

Example 1 output uses `capture: 1`:
```yaml
direction:
  from: segment(-4)
  pattern: "ADT_(Inbound|Outbound)"
  capture: 1
```

But `specs/extraction.md` Section 3.2 doesn't list `capture` as a field property. The spec uses `pattern` with implicit capture group 1.

---

**ISSUE-R7-010: Missing `from: rel_path` in pattern classification**

`specs/extraction.md` Section 3.2 lists valid `from` values:
```
`from` | string | `segment(N)`, `filename`, `full_path`, `rel_path`
```

But the YAML-Expressible Patterns table only covers `segment(N)` and `full_path`. Should clarify that `rel_path` is also YAML_OK.

---

**ISSUE-R7-011: Example 3 hardcodes lookup file path**

```python
with open('/path/to/clients.csv', 'r') as f:
```

This is placeholder text but should be noted as "user must specify" in the generated code comment, or use a config reference.

---

### Verdict

**NEEDS_REVISION**

### Recommendation

The proposal provides a solid conceptual framework but has critical implementation gaps. Before acceptance:

1. **Required:** Define `analyze_complexity()` with concrete rules (ISSUE-R7-001)
2. **Required:** Define `DetectedPattern` struct (ISSUE-R7-005)
3. **Required:** Resolve quarter classification inconsistency (ISSUE-R7-002)
4. **Recommended:** Specify mechanism for hint parsing, even if preliminary (ISSUE-R7-003)
5. **Recommended:** Address the "recommend but classify differently" case (ISSUE-R7-004)

The new gaps introduced (GAP-INT-003, GAP-INT-004, GAP-INT-005) are appropriate - those are genuinely separable concerns. But the core algorithm cannot be implemented without resolving the critical issues above.

---

**Revision requested before next round.**

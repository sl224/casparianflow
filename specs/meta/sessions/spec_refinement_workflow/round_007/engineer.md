# Engineer Proposals - Round 7

**Date:** 2026-01-12
**Focus:** Example Attachment Mechanism (GAP-FLOW-008)
**Priority:** Tier 1 - Unblocking GAP-FLOW-001

**Context:** GAP-FLOW-001 (Error Recovery) was proposed in Round 1 but remains blocked on GAP-FLOW-008. The Round 1 proposal included `attach_example=True` for WRONG_FORMAT retries without defining what examples to attach, where they come from, or how selection works.

**Dependencies Resolved:**
- GAP-FLOW-010 (Gap Lifecycle): 8 states defined
- GAP-FLOW-012 (Severity): CRITICAL/HIGH/MEDIUM/LOW with weights
- GAP-FLOW-004 (Partial Round): Objective completeness checks
- GAP-FLOW-007 (Rollback): Pattern detection heuristics available

---

## Gap Resolution: GAP-FLOW-008

**Gap:** Example attachment mechanism for retries - When retry fails due to wrong format, what example do we attach? Where do examples come from? How are they selected?

**Confidence:** HIGH

### Problem Statement

From Round 1 GAP-FLOW-001 proposal:

```python
if "## Gap Resolution:" not in content:
    return ValidationError("WRONG_FORMAT", retry=True, attach_example=True)
```

This raises questions:
1. **Source:** Where do examples come from?
2. **Format:** What form does the example take?
3. **Selection:** How is the appropriate example chosen?
4. **Size:** How much context is too much? (token limits)
5. **Variation:** Does the example differ by failure type?

---

### Proposed Solution

Define a **hierarchical example system** with three tiers of examples, automatic selection based on failure type, and token-aware truncation.

---

#### 1. Example Sources (Hierarchy)

Examples come from three sources, tried in order:

| Tier | Source | Description | When Available |
|------|--------|-------------|----------------|
| **1** | Workflow Spec | Canonical examples from source.md Section 7 | Always (part of spec) |
| **2** | Session History | Successful outputs from previous rounds | After Round 1 |
| **3** | Generated Templates | Minimal structural templates | Always (fallback) |

**Selection Priority:**
1. If role + failure type has a Tier 1 canonical example, use it
2. Else if successful output exists from this session, use Tier 2
3. Else use Tier 3 generated template

**Rationale:**
- Tier 1 ensures consistency with spec intent
- Tier 2 leverages session-specific context
- Tier 3 guarantees something is always available

---

#### 2. Canonical Examples (Tier 1)

The workflow spec (source.md) defines canonical examples in Section 7. These are the authoritative format references.

**Engineer Canonical Example:**

```markdown
## Gap Resolution: GAP-XXX

**Confidence:** HIGH | MEDIUM | LOW

### Proposed Solution
[1-3 paragraphs describing the solution]

### Examples
[Concrete code/YAML/config examples with explanations]

### Trade-offs
**Pros:**
- [Benefit 1]
- [Benefit 2]

**Cons:**
- [Drawback 1]
- [Drawback 2]

### New Gaps Introduced
- [GAP-YYY: Description] OR "None"
```

**Reviewer Canonical Example:**

```markdown
## Review: [Gap ID or Section]

### Critical Issues
- **ISSUE-RN-001**: [Description]
  - Location: [Exact text or section reference]
  - Impact: [What breaks if not addressed]
  - Suggestion: [How to fix]

### High Priority
[Same format as Critical]

### Medium Priority
[Same format]

### Low Priority / Nits
[Same format]

### Consistency Checks
- [ ] Consistent with related specs?
- [ ] Migration path addressed?
- [ ] Performance implications considered?
```

**Storage:**

Canonical examples are embedded in the workflow spec itself (Section 7) and extracted at runtime:

```python
def extract_canonical_examples(source_md: str) -> dict:
    """
    Extract examples from Section 7 of workflow spec.
    Returns: {"engineer": str, "reviewer": str}
    """
    # Parse Section 7.1 for example session artifacts
    # Extract engineer.md and reviewer.md examples
    # Return as strings with YAML frontmatter stripped
```

---

#### 3. Session History Examples (Tier 2)

Use successful outputs from previous rounds in the same session.

**Selection Criteria for Session Examples:**

| Criterion | Requirement |
|-----------|-------------|
| Validation | Must have passed Mediator validation |
| Relevance | Prefer same gap type if available |
| Recency | Prefer more recent rounds |
| Quality | Prefer rounds that led to ACCEPTED gaps |

**Example Selection Algorithm:**

```python
def select_session_example(role: str, failure_type: str, current_round: int) -> str | None:
    """
    Find best session example for retry prompt.
    """
    candidates = []

    for round_n in range(current_round - 1, 0, -1):  # Most recent first
        output_path = f"round_{round_n:03d}/{role}.md"

        if not exists(output_path):
            continue

        content = read(output_path)

        # Skip rolled-back rounds
        if is_rolled_back(round_n):
            continue

        # Check if this round was validated successfully
        if not was_validated(round_n, role):
            continue

        # Score this candidate
        score = calculate_relevance_score(content, failure_type)
        candidates.append((score, round_n, content))

    if not candidates:
        return None

    # Return highest-scoring candidate
    candidates.sort(reverse=True)
    return candidates[0][2]

def calculate_relevance_score(content: str, failure_type: str) -> int:
    """
    Higher score = better match for retry context.
    """
    score = 0

    # Structural completeness (addresses failure type)
    if failure_type == "WRONG_FORMAT":
        if "## Gap Resolution:" in content:
            score += 10
        if "### Trade-offs" in content:
            score += 5
        if "### Examples" in content:
            score += 5

    elif failure_type == "NO_GAPS_ADDRESSED":
        # Prefer examples that clearly reference gap IDs
        gap_refs = count_gap_references(content)
        score += min(gap_refs * 2, 10)

    # Quality indicators
    if led_to_accepted_gap(content):
        score += 20

    # Length (prefer substantive examples, not too long)
    length = len(content)
    if 500 < length < 5000:
        score += 5
    elif 5000 <= length < 10000:
        score += 2

    return score
```

**Session Example Format:**

When attaching a session example, include provenance:

```markdown
---
Example from this session (Round 3):
This output successfully addressed GAP-FLOW-002 and was approved by Reviewer.
---

## Gap Resolution: GAP-FLOW-002

[... full content from round_003/engineer.md ...]
```

---

#### 4. Generated Templates (Tier 3)

Minimal structural templates for when no canonical or session examples are available.

**Engineer Template:**

```markdown
## Gap Resolution: [GAP-ID]

**Confidence:** [HIGH | MEDIUM | LOW]

### Proposed Solution

[Describe your solution here. Be specific and concrete.]

### Examples

[Provide at least one concrete example showing the solution in action.]

### Trade-offs

**Pros:**
- [List at least 2 benefits]

**Cons:**
- [List at least 2 drawbacks]

### New Gaps Introduced

- [List any new gaps this solution creates, or write "None"]
```

**Reviewer Template:**

```markdown
## Review: [Section or Gap ID being reviewed]

### Critical Issues

[List any spec-breaking issues, or write "None found"]

### High Priority

[List issues that would cause incorrect implementation, or write "None found"]

### Medium Priority

[List suboptimal design issues, or write "None found"]

### Low Priority / Nits

[List polish items, or write "None found"]

### Consistency Checks

- [ ] Consistent with existing architecture?
- [ ] Performance implications addressed?
- [ ] Edge cases covered?
```

**Template Storage:**

Templates are hardcoded in the Mediator implementation (not file-based) to guarantee availability:

```python
TEMPLATES = {
    "engineer": """## Gap Resolution: [GAP-ID]...""",
    "reviewer": """## Review: [Section or Gap ID]..."""
}
```

---

#### 5. Failure Type to Example Mapping

Different failure types need different example emphasis:

| Failure Type | Example Focus | Selection Priority |
|--------------|---------------|-------------------|
| FILE_MISSING | Not applicable (no example helps with this) | N/A |
| EMPTY_OUTPUT | Full example showing expected content volume | Tier 1 > Tier 2 |
| WRONG_FORMAT | Structural example emphasizing headers | Tier 1 > Tier 3 |
| NO_GAPS_ADDRESSED | Session example showing gap references | Tier 2 > Tier 1 |
| INCONSISTENT_REFS | Session example + gap list | Tier 2 + context |

**Example Selection by Failure Type:**

```python
def select_example_for_failure(
    role: str,
    failure_type: str,
    current_round: int,
    gap_list: list[str]
) -> tuple[str, str]:  # (example, source_description)
    """
    Select appropriate example based on failure type.
    Returns tuple of (example_content, source_attribution).
    """

    if failure_type == "FILE_MISSING":
        # No example helps here - this is a Task execution failure
        return ("", "No example attached - file creation issue")

    elif failure_type == "EMPTY_OUTPUT":
        # Need to show expected volume/detail
        canonical = get_canonical_example(role)
        if canonical:
            return (canonical, "Canonical example from workflow spec")
        session = select_session_example(role, failure_type, current_round)
        if session:
            return (session, f"Successful output from previous round")
        return (TEMPLATES[role], "Structural template")

    elif failure_type == "WRONG_FORMAT":
        # Emphasize structure
        canonical = get_canonical_example(role)
        if canonical:
            return (canonical, "Canonical example - note required headers")
        return (TEMPLATES[role], "Structural template - follow this format exactly")

    elif failure_type == "NO_GAPS_ADDRESSED":
        # Session examples better show gap referencing in context
        session = select_session_example(role, failure_type, current_round)
        if session:
            return (session, "Example showing gap references in context")
        canonical = get_canonical_example(role)
        if canonical:
            return (augment_with_gap_list(canonical, gap_list),
                    "Canonical example + gap list for reference")
        return (TEMPLATES[role], "Structural template")

    elif failure_type == "INCONSISTENT_REFS":
        # Session + gap list for valid references
        session = select_session_example(role, failure_type, current_round)
        gap_context = format_gap_list(gap_list)
        if session:
            return (f"{gap_context}\n\n{session}",
                    "Valid gap list + successful example")
        return (f"{gap_context}\n\n{TEMPLATES[role]}",
                "Valid gap list + template")

    # Default fallback
    return (TEMPLATES[role], "Default template")
```

---

#### 6. Token-Aware Truncation

Examples must fit within context limits. Define truncation strategy.

**Token Budget for Examples:**

| Context | Max Example Tokens | Rationale |
|---------|-------------------|-----------|
| Retry prompt | 2000 tokens (~8000 chars) | Leave room for main prompt + gap context |
| Initial prompt | 1000 tokens (~4000 chars) | Examples less critical on first try |

**Truncation Strategy:**

```python
def truncate_example(example: str, max_tokens: int = 2000) -> str:
    """
    Truncate example to fit token budget while preserving structure.
    """
    # Estimate tokens (conservative: 1 token per 4 chars)
    estimated_tokens = len(example) / 4

    if estimated_tokens <= max_tokens:
        return example

    # Try structural truncation first
    truncated = truncate_structurally(example, max_tokens)
    if truncated:
        return truncated

    # Fall back to hard truncation with notice
    char_limit = max_tokens * 4
    return example[:char_limit] + "\n\n[Example truncated for length]"

def truncate_structurally(example: str, max_tokens: int) -> str | None:
    """
    Truncate by removing sections in priority order.
    Preserve: headers, first paragraph of each section.
    Remove: Extended examples, detailed trade-offs.
    """
    sections = parse_sections(example)

    # Priority order for keeping sections (highest priority first)
    priority = [
        "## Gap Resolution",     # Must keep header
        "### Proposed Solution", # Core content
        "### Trade-offs",        # Structure indicator
        "### Examples",          # Abbreviated
        "### New Gaps",          # Brief
    ]

    result = []
    current_tokens = 0

    for section_name in priority:
        if section_name in sections:
            section = sections[section_name]
            section_tokens = len(section) / 4

            if current_tokens + section_tokens <= max_tokens:
                result.append(section)
                current_tokens += section_tokens
            else:
                # Try abbreviated version
                abbreviated = abbreviate_section(section, max_tokens - current_tokens)
                if abbreviated:
                    result.append(abbreviated)
                break

    return "\n\n".join(result) if result else None
```

**Example of Truncated Output:**

```markdown
## Gap Resolution: GAP-FLOW-002

**Confidence:** HIGH

### Proposed Solution

Implement a convergence tracker that measures gap delta per round...

[Solution continues - showing core approach]

### Trade-offs

**Pros:**
- Objective measurement
- Early warning

**Cons:**
- Requires consistent counting

[Example truncated for length - see canonical format for full structure]
```

---

#### 7. Integration with GAP-FLOW-001 (Error Recovery)

Update the Round 1 validation pseudocode to use the example system:

```python
def validate_and_retry(role: str, round_n: int, gap_list: list[str]):
    path = f"round_{round_n:03d}/{role}.md"

    # Validation check
    validation = validate_output(path, role)

    if validation.success:
        return validation

    # Retry with example attachment
    if validation.retry_count < MAX_RETRIES:
        # Select appropriate example
        example, source = select_example_for_failure(
            role=role,
            failure_type=validation.failure_type,
            current_round=round_n,
            gap_list=gap_list
        )

        # Truncate if needed
        example = truncate_example(example, max_tokens=2000)

        # Build retry prompt
        retry_prompt = build_retry_prompt(
            role=role,
            round_n=round_n,
            failure_type=validation.failure_type,
            example=example,
            example_source=source,
            original_prompt=get_original_prompt(role, round_n)
        )

        # Log retry attempt
        log_retry(round_n, role, validation.failure_type, source)

        # Execute retry
        Task(retry_prompt)

        # Recursive validation
        return validate_and_retry(role, round_n, gap_list)

    # Max retries exhausted
    return validation

def build_retry_prompt(
    role: str,
    round_n: int,
    failure_type: str,
    example: str,
    example_source: str,
    original_prompt: str
) -> str:
    """
    Construct retry prompt with example and failure context.
    """
    failure_explanations = {
        "EMPTY_OUTPUT": "Your previous response was too brief. Provide detailed content.",
        "WRONG_FORMAT": "Your previous response did not follow the required format.",
        "NO_GAPS_ADDRESSED": "Your previous response did not reference specific gaps from the gap list.",
        "INCONSISTENT_REFS": "Your previous response referenced gaps that don't exist.",
    }

    return f"""
{original_prompt}

---
RETRY NOTICE (Attempt {validation.retry_count + 1} of {MAX_RETRIES})

Your previous response failed validation: {failure_explanations[failure_type]}

Please review this example and follow its structure:

Source: {example_source}

{example}

---

Now produce your response, following the format shown above.
"""
```

---

#### 8. Recording in status.md

Track example usage for debugging and pattern analysis:

```markdown
## Example Attachment Log

### Round 3 - Engineer Retry 1
- **Failure Type:** WRONG_FORMAT
- **Example Source:** Tier 1 (Canonical from workflow spec)
- **Example Size:** 1,247 chars (312 tokens)
- **Truncated:** No
- **Retry Outcome:** Success

### Round 5 - Reviewer Retry 2
- **Failure Type:** NO_GAPS_ADDRESSED
- **Example Source:** Tier 2 (Round 3 reviewer.md)
- **Example Size:** 3,891 chars (973 tokens)
- **Truncated:** Yes (from 6,234 chars)
- **Retry Outcome:** Success

### Example Source Statistics
| Source | Uses | Success Rate |
|--------|------|--------------|
| Tier 1 (Canonical) | 3 | 100% |
| Tier 2 (Session) | 2 | 100% |
| Tier 3 (Template) | 1 | 50% |
```

---

### Examples

**Example 1: First Round, Wrong Format Failure**

```
Round 1, Engineer validation:
- Content: 200 chars of prose without headers
- Failure: WRONG_FORMAT ("## Gap Resolution:" not found)

Selection process:
1. Check Tier 1: Canonical example exists
2. Return canonical with attribution

Retry prompt includes:
"""
Source: Canonical example from workflow spec

## Gap Resolution: GAP-XXX

**Confidence:** HIGH | MEDIUM | LOW

### Proposed Solution
[1-3 paragraphs describing the solution]
...
"""

Retry outcome: Success (Engineer produces properly formatted output)
```

**Example 2: Later Round, No Gaps Addressed**

```
Round 5, Engineer validation:
- Content: 2000 chars discussing general concepts
- Failure: NO_GAPS_ADDRESSED (no GAP-XXX references found)

Selection process:
1. Check Tier 1: Canonical exists but doesn't emphasize gap references
2. Check Tier 2: Round 3 engineer.md successfully addressed 2 gaps
3. Return Round 3 output as example

Retry prompt includes:
"""
Source: Example showing gap references in context

## Gap Resolution: GAP-FLOW-002

**Confidence:** HIGH

### Proposed Solution
[Content that explicitly references GAP-FLOW-002 throughout]
...
"""

Retry outcome: Success (Engineer references assigned gaps)
```

**Example 3: Token Truncation Needed**

```
Round 7, Reviewer validation:
- Content: 800 chars with headers but sparse
- Failure: EMPTY_OUTPUT (< 1000 chars for reviewer)

Selection process:
1. Check Tier 2: Round 4 reviewer.md is excellent (8,500 chars)
2. Round 4 output exceeds 2000 token budget
3. Truncate structurally: Keep headers + first paragraphs

Original (8,500 chars):
"""
## Review: GAP-FLOW-001 through GAP-FLOW-007

### Critical Issues
- **ISSUE-R1-001**: [500 chars of detail]
- **ISSUE-R1-002**: [500 chars of detail]
... [20 more issues]

### High Priority
[2000 chars]
...
"""

Truncated (6,800 chars):
"""
## Review: GAP-FLOW-001 through GAP-FLOW-007

### Critical Issues
- **ISSUE-R1-001**: [500 chars of detail]
- **ISSUE-R1-002**: [First sentence only]
- [8 more issues summarized]

### High Priority
[First 2 issues only]

[Example truncated - see full format in workflow spec]
"""

Retry outcome: Success
```

**Example 4: Template Fallback (New Session, First Round)**

```
Round 1 of new session, Engineer validation:
- Content: File exists but wrong encoding (garbled)
- Failure: WRONG_FORMAT

Selection process:
1. Check Tier 1: Canonical example extraction fails (source.md not loaded)
2. Check Tier 2: No previous rounds exist (Round 1)
3. Fallback to Tier 3: Return hardcoded template

Retry prompt includes:
"""
Source: Structural template - follow this format exactly

## Gap Resolution: [GAP-ID]

**Confidence:** [HIGH | MEDIUM | LOW]

### Proposed Solution
[Describe your solution here. Be specific and concrete.]
...
"""

Retry outcome: Success (template provides sufficient structure)
```

---

### Trade-offs

**Pros:**
- Three-tier hierarchy ensures examples always available
- Session examples leverage context-specific patterns
- Token-aware truncation prevents context overflow
- Failure-type mapping provides appropriate examples
- Source attribution aids debugging
- Statistics enable pattern analysis

**Cons:**
- Canonical example extraction requires parsing Section 7 (fragile if spec format changes)
- Session example selection adds computational overhead
- Relevance scoring heuristics may miss edge cases
- Truncation may remove valuable context
- Token estimation (4 chars/token) is approximate

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | Session examples filtered by gap state (prefer ACCEPTED outputs) |
| GAP-FLOW-012 (Severity) | N/A (examples don't vary by severity) |
| GAP-FLOW-007 (Rollback) | Rolled-back rounds excluded from session example candidates |
| GAP-FLOW-001 (Error Recovery) | This gap provides the example mechanism for FLOW-001 retries |

---

### Response to Round 1 Handwave

Round 1 GAP-FLOW-001 proposal stated:

> `return ValidationError("WRONG_FORMAT", retry=True, attach_example=True)`

This round defines `attach_example=True` as:

1. **Selection:** `select_example_for_failure(role, failure_type, round, gaps)`
2. **Sources:** Tier 1 canonical > Tier 2 session > Tier 3 template
3. **Format:** Markdown with source attribution header
4. **Size:** Max 2000 tokens, structurally truncated
5. **Integration:** Example injected into retry prompt with failure explanation

**GAP-FLOW-001 is now unblocked.**

---

### New Gaps Introduced

- **GAP-FLOW-018**: Canonical example extraction robustness - What if Section 7 format changes?

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-008 | Proposed | HIGH | 1 (GAP-FLOW-018) |

**This Round:**
- Defined three-tier example source hierarchy
- Specified selection algorithm by failure type
- Defined token-aware truncation strategy
- Provided integration code with GAP-FLOW-001
- Created recording format for status.md

**Unblocked:**
- GAP-FLOW-001 (Error Recovery) - can now be marked RESOLVED after Reviewer approval

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Example Source Hierarchy
```
Tier 1: Canonical (workflow spec Section 7)
Tier 2: Session history (previous successful rounds)
Tier 3: Templates (hardcoded fallback)
```

### Failure Type Mapping
```
FILE_MISSING     -> No example (execution issue)
EMPTY_OUTPUT     -> Full canonical example
WRONG_FORMAT     -> Structural canonical/template
NO_GAPS_ADDRESSED -> Session example with gap refs
INCONSISTENT_REFS -> Gap list + session example
```

### Token Limits
```
Retry prompt:  2000 tokens (~8000 chars)
Initial prompt: 1000 tokens (~4000 chars)
```

### Truncation Priority
```
Keep (highest priority):
1. ## Headers
2. ### Proposed Solution (first paragraph)
3. ### Trade-offs (abbreviated)
4. ### Examples (first only)

Remove first:
- Extended examples
- Detailed trade-off discussions
- Long consistency check lists
```

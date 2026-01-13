# Engineer Proposals - Round 9

**Date:** 2026-01-12
**Focus:** Error Recovery Revision (GAP-FLOW-001)
**Priority:** Tier 1 - Completing Flow Gaps

**Context:** GAP-FLOW-001 (Error Recovery) was proposed in Round 1 but blocked on GAP-FLOW-008 (Example Attachment). Now that GAP-FLOW-008 is resolved (Round 7), along with related gaps GAP-FLOW-004 (Partial Round), GAP-FLOW-007 (Rollback), and GAP-FLOW-013 (Implicit Disagreement), this proposal revises GAP-FLOW-001 to address all Round 1 Reviewer issues.

**Dependencies Resolved:**
- GAP-FLOW-008 (Example Attachment): Three-tier hierarchy, failure-type-aware selection, token truncation
- GAP-FLOW-004 (Partial Round): Bounded retries, escalation options, NO_ISSUES_FOUND
- GAP-FLOW-007 (Rollback): Root cause analysis, pattern detection, auto-retry
- GAP-FLOW-010 (Gap Lifecycle): 8 states defined
- GAP-FLOW-012 (Severity): CRITICAL/HIGH/MEDIUM/LOW with weights

---

## Gap Resolution: GAP-FLOW-001 (Revised)

**Gap:** No error recovery - What if Engineer produces invalid/malformed output?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-001 (Retry prompt modification undefined) | Defined specific prompt modifications per failure type with concrete text |
| ISSUE-R1-002 ("attach_example=True" hand-waved) | Resolved by GAP-FLOW-008; integrated here with explicit references |
| ISSUE-R1-003 ("Proceed with partial round" undefined) | Resolved by GAP-FLOW-004; integrated with explicit semantics |
| ISSUE-R1-004 (100-char threshold arbitrary) | Replaced with structural validation as primary; min length as secondary sanity check |
| ISSUE-R1-005 (Multi-gap round validation parsing undefined) | Defined gap ID regex with format specification |
| ISSUE-R1-006 (Timestamp format undefined) | Specified ISO 8601 format |
| ISSUE-R1-007 (Return types inconsistent) | Standardized to Result type pattern |

---

### Revised Proposal

#### 1. Validation Gate Architecture

The Mediator validates output structure before proceeding to the next phase. Validation is a **two-tier system**: structural validation (primary) and content validation (secondary).

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         VALIDATION GATE FLOW                                 │
└─────────────────────────────────────────────────────────────────────────────┘

           ┌─────────────────────────────────────────┐
           │          Role Output Complete           │
           │    (Engineer or Reviewer writes file)   │
           └──────────────────┬──────────────────────┘
                              │
                              ▼
           ┌─────────────────────────────────────────┐
           │       Tier 1: Structural Validation     │
           │  - File exists at expected path         │
           │  - File is non-empty                    │
           │  - Required headers present             │
           │  - Gap ID format valid                  │
           └──────────────────┬──────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               ▼               ▼
         PASS            FAIL (retriable)   FAIL (fatal)
              │               │               │
              │               ▼               ▼
              │     ┌─────────────────┐    Return error
              │     │  Retry with     │    to user
              │     │  prompt mods    │
              │     │  + example      │
              │     │  (max 2 tries)  │
              │     └────────┬────────┘
              │              │
              │              ▼
              │     [Re-validate on retry]
              │
              ▼
           ┌─────────────────────────────────────────┐
           │       Tier 2: Content Validation        │
           │  - At least one gap addressed           │
           │  - Cross-references valid               │
           │  - Minimum substantive content          │
           └──────────────────┬──────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               ▼               ▼
         PASS            WARN            FAIL (retriable)
              │               │               │
              │               │               ▼
              │               │     [Retry as above]
              │               │
              ▼               ▼
           Proceed to      Proceed with
           next phase      logged warning
```

---

#### 2. Validation Rules (Addressing ISSUE-R1-004)

**Problem:** Round 1 used 100-character minimum as primary check, which is arbitrary and catches garbage.

**Solution:** Structural validation is primary; character count is secondary sanity check.

##### Tier 1: Structural Validation (Required)

| Check | Engineer | Reviewer | Failure Type |
|-------|----------|----------|--------------|
| File exists | `round_N/engineer.md` | `round_N/reviewer.md` | FILE_MISSING |
| File non-empty | `len(content) > 0` | `len(content) > 0` | EMPTY_OUTPUT |
| Primary header present | `## Gap Resolution:` | `## Review:` | WRONG_FORMAT |
| Confidence stated (Engineer) | `**Confidence:**` | N/A | WRONG_FORMAT |
| Severity used (Reviewer) | N/A | Issue severity markers | WRONG_FORMAT |

**Structural Validation Implementation:**

```python
def validate_structure(path: str, role: str) -> ValidationResult:
    """
    Tier 1: Structural validation.
    Returns ValidationResult with success flag and failure details.
    """
    # Check 1: File existence
    if not exists(path):
        return ValidationResult(
            success=False,
            failure_type="FILE_MISSING",
            retriable=True,
            message=f"Expected file not found: {path}"
        )

    content = read(path)

    # Check 2: Non-empty
    if len(content.strip()) == 0:
        return ValidationResult(
            success=False,
            failure_type="EMPTY_OUTPUT",
            retriable=True,
            message="File exists but is empty"
        )

    # Check 3: Role-specific headers
    if role == "engineer":
        if "## Gap Resolution:" not in content:
            return ValidationResult(
                success=False,
                failure_type="WRONG_FORMAT",
                retriable=True,
                message="Missing required header: '## Gap Resolution:'"
            )
        if "**Confidence:**" not in content:
            return ValidationResult(
                success=False,
                failure_type="WRONG_FORMAT",
                retriable=True,
                message="Missing required: '**Confidence:**' statement"
            )

    elif role == "reviewer":
        if "## Review:" not in content:
            return ValidationResult(
                success=False,
                failure_type="WRONG_FORMAT",
                retriable=True,
                message="Missing required header: '## Review:'"
            )
        # Reviewer must use severity sections OR NO_ISSUES_FOUND
        has_severity = any(h in content for h in [
            "### Critical Issues",
            "### High Priority",
            "### Medium Priority",
            "### Low Priority"
        ])
        has_no_issues = "NO_ISSUES_FOUND" in content or "No Issues Found" in content
        if not (has_severity or has_no_issues):
            return ValidationResult(
                success=False,
                failure_type="WRONG_FORMAT",
                retriable=True,
                message="Reviewer must include severity sections or NO_ISSUES_FOUND"
            )

    return ValidationResult(success=True)
```

##### Tier 2: Content Validation (Advisory + Required)

| Check | Threshold | Failure Type | Required? |
|-------|-----------|--------------|-----------|
| Gaps addressed | >= 1 gap ID referenced | NO_GAPS_ADDRESSED | Yes (Engineer) |
| Cross-refs valid | All GAP-XXX refs exist in status.md | INCONSISTENT_REFS | Yes |
| Min substantive content | >= 200 chars per addressed gap | THIN_CONTENT | Warning only |
| Trade-offs present | `### Trade-offs` section | INCOMPLETE_STRUCTURE | Warning only |

**Content Validation Implementation:**

```python
def validate_content(content: str, role: str, gap_list: list[str]) -> ValidationResult:
    """
    Tier 2: Content validation.
    Checks semantic completeness after structural validation passes.
    """
    # Check 1: Gaps addressed (Engineer only)
    if role == "engineer":
        gaps_addressed = extract_gap_ids(content)
        if len(gaps_addressed) == 0:
            return ValidationResult(
                success=False,
                failure_type="NO_GAPS_ADDRESSED",
                retriable=True,
                message="No gap IDs found in output. Expected references like GAP-FLOW-001."
            )

    # Check 2: Cross-references valid
    referenced_gaps = extract_gap_ids(content)
    invalid_refs = [g for g in referenced_gaps if g not in gap_list]
    if invalid_refs:
        return ValidationResult(
            success=False,
            failure_type="INCONSISTENT_REFS",
            retriable=True,
            message=f"Invalid gap references: {invalid_refs}. These do not exist in status.md."
        )

    # Check 3: Min substantive content (warning only)
    warnings = []
    if role == "engineer":
        gaps_addressed = extract_gap_ids(content)
        for gap_id in gaps_addressed:
            section = extract_gap_section(content, gap_id)
            if section and len(section) < 200:
                warnings.append(f"Gap {gap_id} section is thin ({len(section)} chars)")

    # Check 4: Trade-offs present (warning only)
    if role == "engineer" and "### Trade-offs" not in content:
        warnings.append("Missing ### Trade-offs section (recommended)")

    if warnings:
        return ValidationResult(
            success=True,
            warnings=warnings,
            message="Content valid with warnings"
        )

    return ValidationResult(success=True)
```

---

#### 3. Gap ID Format and Parsing (Addressing ISSUE-R1-005)

**Problem:** Round 1 assumed `extract_gap_ids(content)` would work without defining gap ID format or regex.

**Solution:** Define strict format with regex.

**Gap ID Format Specification:**

```
GAP-{CATEGORY}-{NUMBER}

Where:
- CATEGORY: 2-10 uppercase letters (e.g., FLOW, ROLE, COMM, AUTO, QA, UX)
- NUMBER: 3-digit number (001-999)

Examples:
- GAP-FLOW-001  (valid)
- GAP-COMM-012  (valid)
- GAP-UX-999    (valid)
- GAP-flow-001  (INVALID - lowercase)
- GAP-FLOW-1    (INVALID - single digit)
- GAP-A-001     (INVALID - category too short)
```

**Regex Pattern:**

```python
GAP_ID_PATTERN = r"GAP-[A-Z]{2,10}-\d{3}"

def extract_gap_ids(content: str) -> list[str]:
    """
    Extract all gap IDs from content.
    Returns deduplicated list sorted by category then number.
    """
    import re
    matches = re.findall(GAP_ID_PATTERN, content)
    unique = sorted(set(matches))
    return unique

def extract_gap_section(content: str, gap_id: str) -> str | None:
    """
    Extract the section for a specific gap.
    Looks for "## Gap Resolution: {gap_id}" and captures until next ## or end.
    """
    import re
    pattern = rf"## Gap Resolution:\s*{re.escape(gap_id)}(.*?)(?=\n## |$)"
    match = re.search(pattern, content, re.DOTALL)
    return match.group(1).strip() if match else None
```

**Issue ID Format (for Reviewer):**

```
ISSUE-R{ROUND}-{NUMBER}

Where:
- ROUND: Round number (1-99)
- NUMBER: Sequential issue number (001-999)

Examples:
- ISSUE-R1-001  (valid)
- ISSUE-R12-042 (valid)
```

**Issue ID Regex:**

```python
ISSUE_ID_PATTERN = r"ISSUE-R\d{1,2}-\d{3}"
```

---

#### 4. Prompt Modifications Per Failure Type (Addressing ISSUE-R1-001)

**Problem:** Round 1 said "retry with clearer prompt" without defining what changes.

**Solution:** Define specific prompt modifications for each failure type.

**Prompt Modification Table:**

| Failure Type | Prompt Modification | Example Integration |
|--------------|---------------------|---------------------|
| FILE_MISSING | Add explicit file path + Write tool reminder | No example (execution issue) |
| EMPTY_OUTPUT | Provide minimum output template + "produce output even if uncertain" | Full canonical example (per FLOW-008) |
| WRONG_FORMAT | Add structural requirements + required headers list | Canonical or template (per FLOW-008) |
| NO_GAPS_ADDRESSED | Explicit gap assignment + "begin with ## Gap Resolution: GAP-XXX" | Session example with gap refs (per FLOW-008) |
| INCONSISTENT_REFS | Provide valid gap list + "only reference these gaps" | Gap list + session example (per FLOW-008) |
| THIN_CONTENT | Add minimum length requirements per section | N/A (warning, not retry) |

**Concrete Prompt Modifications:**

```python
def build_retry_prompt_modification(failure_type: str, context: dict) -> str:
    """
    Generate specific prompt modification based on failure type.
    Returns text to prepend to original prompt.
    """

    modifications = {
        "FILE_MISSING": f"""
CRITICAL: Your previous attempt did not create the required output file.

You MUST write your output to: {context['expected_path']}

Use the Write tool to create this file. Do not skip this step.
The file must exist at exactly this path when you complete your task.
""",

        "EMPTY_OUTPUT": f"""
ATTENTION: Your previous output was empty or too brief.

You MUST produce substantive content, even if you are uncertain.
Minimum expectations:
- At least 200 characters per gap addressed
- Include concrete examples, not just abstract descriptions
- If uncertain, state your confidence as LOW and explain what information is missing

Do NOT submit an empty file. Write something useful.
""",

        "WRONG_FORMAT": f"""
FORMAT CORRECTION REQUIRED:

Your previous output did not follow the required structure.
You MUST include these elements:

For Engineer:
- Start each gap with: ## Gap Resolution: GAP-XXX
- Include: **Confidence:** HIGH | MEDIUM | LOW
- Include: ### Proposed Solution, ### Examples, ### Trade-offs, ### New Gaps Introduced

For Reviewer:
- Start with: ## Review: [section/gap being reviewed]
- Include severity sections: ### Critical Issues, ### High Priority, etc.
- OR include: NO_ISSUES_FOUND with verification checklist

The example below shows the correct format. Match it exactly.
""",

        "NO_GAPS_ADDRESSED": f"""
GAP REFERENCE REQUIRED:

Your previous output did not reference any specific gaps.
You are assigned to address these gaps (in priority order):

{context['gap_list_formatted']}

Your output MUST:
1. Begin with: ## Gap Resolution: {context['first_gap']}
2. Explicitly reference gap IDs (e.g., GAP-FLOW-001) in your text
3. Address at least one gap from the assigned list

Do not discuss gaps in general terms. Reference them by ID.
""",

        "INCONSISTENT_REFS": f"""
INVALID REFERENCES DETECTED:

Your previous output referenced gaps that do not exist:
{context['invalid_refs']}

Valid gaps for this session are:
{context['valid_gap_list']}

Only reference gaps from this list. If you meant a different gap,
use the correct ID. If the gap doesn't exist, create it as a "New Gap"
in the ### New Gaps Introduced section.
"""
    }

    return modifications.get(failure_type, "Please review your output and try again.")
```

**Complete Retry Prompt Construction:**

```python
def construct_retry_prompt(
    original_prompt: str,
    failure_type: str,
    context: dict,
    attempt: int
) -> str:
    """
    Build complete retry prompt with modifications and example.
    Integrates with GAP-FLOW-008 example attachment.
    """
    # 1. Get prompt modification for failure type
    modification = build_retry_prompt_modification(failure_type, context)

    # 2. Select and truncate example (per GAP-FLOW-008)
    example, source = select_example_for_failure(
        role=context['role'],
        failure_type=failure_type,
        current_round=context['round'],
        gap_list=context['gap_list']
    )
    example = truncate_example(example, max_tokens=2000)

    # 3. Build retry notice
    retry_notice = f"""
═══════════════════════════════════════════════════════════════════════════════
                         RETRY ATTEMPT {attempt} of 2
═══════════════════════════════════════════════════════════════════════════════

{modification}

───────────────────────────────────────────────────────────────────────────────
                              EXAMPLE OUTPUT
Source: {source}
───────────────────────────────────────────────────────────────────────────────

{example}

═══════════════════════════════════════════════════════════════════════════════
"""

    # 4. Combine: original prompt + retry notice
    return f"{retry_notice}\n\n{original_prompt}"
```

---

#### 5. Partial Round Semantics (Addressing ISSUE-R1-003, integrating GAP-FLOW-004)

**Problem:** Round 1 said "proceed with partial round" without defining what that means.

**Solution:** Integrate GAP-FLOW-004 partial round handling.

**Partial Round Definition:**

A partial round occurs when one or both roles fail to produce valid output after max retries.

| Scenario | Definition | What Happens |
|----------|------------|--------------|
| Engineer partial | Engineer addressed some but not all assigned gaps | Reviewer reviews what exists |
| Engineer failed | Engineer produced no valid output after 2 retries | Round recorded as Engineer=SKIP |
| Reviewer partial | Reviewer reviewed some but not all proposals | Continue with reviewed portions |
| Reviewer failed | Reviewer produced no valid output after 2 retries | Engineer proposals marked "unreviewed" |
| Both failed | Neither produced valid output | Escalate to user (per FLOW-004) |

**Integration with GAP-FLOW-004:**

When max retries exhausted, invoke FLOW-004 escalation:

```python
def handle_max_retries_exhausted(role: str, round_n: int, failure_type: str):
    """
    After 2 failed retries, escalate per GAP-FLOW-004.
    """
    # Record failure in status.md
    log_validation_failure(round_n, role, failure_type, "MAX_RETRIES_EXHAUSTED")

    # Present user options (from FLOW-004)
    options = """
{role} could not produce valid output after 2 attempts.

=== Situation ===
Round: {round_n}
Role: {role}
Failure Type: {failure_type}
Attempts: 2 (both failed validation)

=== Options ===
1. Skip {role} this round - Proceed without {role} output
   Effect: Round {round_n} recorded as "{role}: SKIP"

2. Reassign gaps - Choose different gaps for this round
   Effect: Select from non-blocked gaps in status.md

3. Provide context - Add information {role} may be missing
   Effect: Text input added to prompt, one additional retry

4. Narrow scope - Address only the simplest gap
   Effect: Reassign to single lowest-complexity gap

5. Pause session - Save state for later
   Effect: Session saved, can resume with fresh context

Choose an option (1-5): ___
""".format(role=role, round_n=round_n, failure_type=failure_type)

    return AskUserQuestion(options)
```

---

#### 6. Timestamp Format (Addressing ISSUE-R1-006)

**Format:** ISO 8601 with timezone

```
YYYY-MM-DDTHH:MM:SSZ

Example: 2026-01-12T14:30:00Z
```

**Usage in Error Recording:**

```markdown
## Round 3 Errors

| Timestamp | Role | Event | Details |
|-----------|------|-------|---------|
| 2026-01-12T14:30:00Z | Engineer | FILE_MISSING | Retry 1 initiated |
| 2026-01-12T14:31:15Z | Engineer | Retry success | File created on retry |
| 2026-01-12T14:35:00Z | Reviewer | WRONG_FORMAT | Missing severity sections |
| 2026-01-12T14:36:30Z | Reviewer | Retry success | Format corrected |
```

---

#### 7. Return Types (Addressing ISSUE-R1-007)

**Standardized Result Type:**

```python
from dataclasses import dataclass
from typing import Optional, List

@dataclass
class ValidationResult:
    """
    Standardized return type for all validation functions.
    """
    success: bool
    failure_type: Optional[str] = None  # e.g., "FILE_MISSING", "WRONG_FORMAT"
    retriable: bool = False
    message: str = ""
    warnings: List[str] = None
    gaps_addressed: List[str] = None  # For successful Engineer validation

    def __post_init__(self):
        if self.warnings is None:
            self.warnings = []
        if self.gaps_addressed is None:
            self.gaps_addressed = []


# Usage examples:

# Success
ValidationResult(
    success=True,
    gaps_addressed=["GAP-FLOW-001", "GAP-FLOW-002"]
)

# Retriable failure
ValidationResult(
    success=False,
    failure_type="WRONG_FORMAT",
    retriable=True,
    message="Missing required header: '## Gap Resolution:'"
)

# Fatal failure (not retriable)
ValidationResult(
    success=False,
    failure_type="EXECUTION_ERROR",
    retriable=False,
    message="Task tool failed to spawn instance"
)

# Success with warnings
ValidationResult(
    success=True,
    warnings=["Gap GAP-FLOW-003 section is thin (150 chars)"],
    gaps_addressed=["GAP-FLOW-001", "GAP-FLOW-002", "GAP-FLOW-003"]
)
```

---

#### 8. Complete Validation Flow

**Main Validation Function:**

```python
def validate_role_output(
    role: str,
    round_n: int,
    gap_list: list[str],
    max_retries: int = 2
) -> ValidationResult:
    """
    Complete validation flow for a role's output.
    Integrates structural validation, content validation, and retry logic.
    """
    path = f"round_{round_n:03d}/{role}.md"
    attempt = 0

    while attempt <= max_retries:
        # Tier 1: Structural validation
        struct_result = validate_structure(path, role)

        if not struct_result.success:
            if not struct_result.retriable or attempt >= max_retries:
                # Cannot retry or max retries reached
                return struct_result

            # Retry with prompt modifications (per Section 4)
            attempt += 1
            retry_prompt = construct_retry_prompt(
                original_prompt=get_original_prompt(role, round_n),
                failure_type=struct_result.failure_type,
                context={
                    'role': role,
                    'round': round_n,
                    'expected_path': path,
                    'gap_list': gap_list,
                    'gap_list_formatted': format_gap_list(gap_list),
                    'first_gap': gap_list[0] if gap_list else "GAP-XXX",
                },
                attempt=attempt
            )

            # Log retry
            log_retry_attempt(round_n, role, struct_result.failure_type, attempt)

            # Execute retry
            Task(retry_prompt)

            # Loop back to re-validate
            continue

        # Tier 2: Content validation
        content = read(path)
        content_result = validate_content(content, role, gap_list)

        if not content_result.success:
            if not content_result.retriable or attempt >= max_retries:
                return content_result

            # Retry for content issues
            attempt += 1
            retry_prompt = construct_retry_prompt(
                original_prompt=get_original_prompt(role, round_n),
                failure_type=content_result.failure_type,
                context={
                    'role': role,
                    'round': round_n,
                    'gap_list': gap_list,
                    'gap_list_formatted': format_gap_list(gap_list),
                    'first_gap': gap_list[0] if gap_list else "GAP-XXX",
                    'invalid_refs': content_result.message,
                    'valid_gap_list': format_gap_list(gap_list),
                },
                attempt=attempt
            )

            log_retry_attempt(round_n, role, content_result.failure_type, attempt)
            Task(retry_prompt)
            continue

        # Both tiers passed
        return content_result

    # Should not reach here, but handle max retries fallback
    return ValidationResult(
        success=False,
        failure_type="MAX_RETRIES_EXHAUSTED",
        retriable=False,
        message=f"Validation failed after {max_retries} retry attempts"
    )
```

---

#### 9. Error State Recording in status.md

**Format:**

```markdown
## Round N Validation Log

### Validation Summary
| Role | Outcome | Attempts | Final Failure Type |
|------|---------|----------|-------------------|
| Engineer | SUCCESS | 2 | N/A (passed on retry) |
| Reviewer | SUCCESS | 1 | N/A |

### Detailed Log

| Timestamp | Role | Attempt | Validation | Result | Message |
|-----------|------|---------|------------|--------|---------|
| 2026-01-12T14:30:00Z | Engineer | 1 | Structure | FAIL | Missing '## Gap Resolution:' |
| 2026-01-12T14:31:15Z | Engineer | 2 | Structure | PASS | - |
| 2026-01-12T14:31:15Z | Engineer | 2 | Content | PASS | 2 gaps addressed |
| 2026-01-12T14:35:00Z | Reviewer | 1 | Structure | PASS | - |
| 2026-01-12T14:35:00Z | Reviewer | 1 | Content | PASS | - |

### Retry Details

#### Engineer Retry 1 (2026-01-12T14:30:00Z)
- **Failure Type:** WRONG_FORMAT
- **Prompt Modification:** FORMAT CORRECTION REQUIRED (structural headers)
- **Example Attached:** Tier 1 Canonical (1,247 chars)
- **Outcome:** Success on retry
```

---

#### 10. Integration Points

**Integration with Other Resolved Gaps:**

| Gap | Integration |
|-----|-------------|
| GAP-FLOW-008 (Example Attachment) | `select_example_for_failure()` called during retry prompt construction |
| GAP-FLOW-004 (Partial Round) | `handle_max_retries_exhausted()` invokes FLOW-004 escalation options |
| GAP-FLOW-007 (Rollback) | Validation failures can trigger rollback via FLOW-007 flow |
| GAP-FLOW-010 (Lifecycle) | Validation tracks gap state transitions |
| GAP-FLOW-012 (Severity) | Reviewer validation checks for severity sections |

**Mediator Orchestration Integration:**

```python
def run_round(round_n: int, gap_list: list[str]):
    """
    Main round orchestration with integrated validation.
    """
    # Phase 1: Engineer
    engineer_prompt = build_engineer_prompt(round_n, gap_list)
    Task(engineer_prompt)

    engineer_result = validate_role_output("engineer", round_n, gap_list)

    if not engineer_result.success:
        if engineer_result.failure_type == "MAX_RETRIES_EXHAUSTED":
            # Invoke FLOW-004 escalation
            user_choice = handle_max_retries_exhausted("engineer", round_n, engineer_result.failure_type)
            handle_escalation_choice(user_choice, round_n, "engineer")
        else:
            # Fatal error
            return RoundResult(status="FAILED", details=engineer_result.message)

    # Log warnings if any
    if engineer_result.warnings:
        log_warnings(round_n, "engineer", engineer_result.warnings)

    # Phase 2: Reviewer
    reviewer_prompt = build_reviewer_prompt(round_n, gap_list)
    Task(reviewer_prompt)

    reviewer_result = validate_role_output("reviewer", round_n, gap_list)

    if not reviewer_result.success:
        if reviewer_result.failure_type == "MAX_RETRIES_EXHAUSTED":
            user_choice = handle_max_retries_exhausted("reviewer", round_n, reviewer_result.failure_type)
            handle_escalation_choice(user_choice, round_n, "reviewer")
        else:
            return RoundResult(status="FAILED", details=reviewer_result.message)

    # Phase 3: Continue with synthesis
    synthesize_summary(round_n)
    update_status(round_n)

    return RoundResult(status="SUCCESS", gaps_addressed=engineer_result.gaps_addressed)
```

---

### Examples

**Example 1: Successful Validation on First Try**

```
Round 5 Engineer output validation:
─────────────────────────────────────────────────────────
Tier 1 (Structure):
  - File exists: round_005/engineer.md ✓
  - Non-empty: 3,456 chars ✓
  - "## Gap Resolution:" present ✓
  - "**Confidence:**" present ✓
  → PASS

Tier 2 (Content):
  - Gaps addressed: GAP-FLOW-001, GAP-FLOW-002 ✓
  - Cross-refs valid: All 4 GAP-XXX refs exist in status.md ✓
  - Min content: GAP-FLOW-001 (1,234 chars), GAP-FLOW-002 (1,890 chars) ✓
  - Trade-offs present ✓
  → PASS

Result: ValidationResult(success=True, gaps_addressed=["GAP-FLOW-001", "GAP-FLOW-002"])
```

**Example 2: Retry Due to Wrong Format**

```
Round 3 Engineer output validation:
─────────────────────────────────────────────────────────
Attempt 1:

Tier 1 (Structure):
  - File exists: round_003/engineer.md ✓
  - Non-empty: 890 chars ✓
  - "## Gap Resolution:" NOT FOUND ✗
  → FAIL (WRONG_FORMAT)

Retry initiated:
  - Prompt modification: FORMAT CORRECTION REQUIRED
  - Example attached: Tier 1 Canonical (workflow spec Section 7)
  - Example size: 1,247 chars (within 2000 token limit)

[Task executes retry]

Attempt 2:

Tier 1 (Structure):
  - File exists: round_003/engineer.md ✓
  - Non-empty: 2,100 chars ✓
  - "## Gap Resolution:" present ✓
  - "**Confidence:**" present ✓
  → PASS

Tier 2 (Content):
  - Gaps addressed: GAP-FLOW-003 ✓
  - Cross-refs valid ✓
  → PASS

Result: ValidationResult(success=True, gaps_addressed=["GAP-FLOW-003"])

status.md entry:
| Timestamp | Role | Attempt | Validation | Result | Message |
|-----------|------|---------|------------|--------|---------|
| 2026-01-12T14:30:00Z | Engineer | 1 | Structure | FAIL | Missing '## Gap Resolution:' |
| 2026-01-12T14:31:15Z | Engineer | 2 | Structure | PASS | - |
| 2026-01-12T14:31:15Z | Engineer | 2 | Content | PASS | 1 gap addressed |
```

**Example 3: Max Retries Exhausted - Escalation**

```
Round 7 Engineer output validation:
─────────────────────────────────────────────────────────
Attempt 1:
  - FAIL: NO_GAPS_ADDRESSED
  - Retry with gap list + session example

Attempt 2:
  - FAIL: NO_GAPS_ADDRESSED (still no GAP-XXX references)

Max retries exhausted. Invoking GAP-FLOW-004 escalation:

AskUserQuestion:
"Engineer could not produce valid output after 2 attempts.

=== Situation ===
Round: 7
Role: Engineer
Failure Type: NO_GAPS_ADDRESSED
Attempts: 2 (both failed validation)

=== Options ===
1. Skip Engineer this round - Proceed without Engineer output
2. Reassign gaps - Choose different gaps for this round
3. Provide context - Add information Engineer may be missing
4. Narrow scope - Address only the simplest gap
5. Pause session - Save state for later

Choose an option (1-5): ___"

User selects: 4 (Narrow scope)

Mediator:
- Selects GAP-UX-001 (lowest complexity from assigned list)
- Generates new prompt with only GAP-UX-001
- One additional retry with narrowed scope

[Task executes narrowed retry]

Attempt 3 (narrowed):
  - PASS: GAP-UX-001 addressed

Result: Partial success, round continues with single gap addressed
```

**Example 4: Invalid Cross-References**

```
Round 4 Engineer output validation:
─────────────────────────────────────────────────────────
Attempt 1:

Tier 1 (Structure): PASS

Tier 2 (Content):
  - Gaps addressed: GAP-FLOW-099, GAP-FLOW-100 ✓ (syntax valid)
  - Cross-refs check:
    - GAP-FLOW-099: NOT IN status.md ✗
    - GAP-FLOW-100: NOT IN status.md ✗
  → FAIL (INCONSISTENT_REFS)

Retry initiated:
  - Prompt modification: INVALID REFERENCES DETECTED
  - Invalid refs listed: GAP-FLOW-099, GAP-FLOW-100
  - Valid gap list attached
  - Example: Gap list + Round 2 session example

[Task executes retry]

Attempt 2:

Tier 2 (Content):
  - Gaps addressed: GAP-FLOW-001, GAP-FLOW-002 ✓
  - Cross-refs valid ✓
  → PASS

status.md entry:
| Timestamp | Role | Attempt | Validation | Result | Message |
|-----------|------|---------|------------|--------|---------|
| 2026-01-12T14:30:00Z | Engineer | 1 | Content | FAIL | Invalid refs: GAP-FLOW-099, GAP-FLOW-100 |
| 2026-01-12T14:31:30Z | Engineer | 2 | Content | PASS | 2 gaps addressed |
```

---

### Trade-offs

**Pros:**
- Structural validation as primary check is more robust than character count
- Specific prompt modifications per failure type increase retry success rate
- Gap ID regex ensures consistent parsing across rounds
- ISO 8601 timestamps enable reliable sorting and parsing
- Standardized Result type simplifies error handling
- Integration with FLOW-004, FLOW-007, FLOW-008 creates coherent system
- Examples attached per failure type (via FLOW-008) maximize retry effectiveness

**Cons:**
- Structural validation may pass technically correct but semantically poor output
- Two-tier validation adds Mediator complexity
- Retry prompts add token overhead (~2000 tokens per retry)
- Gap ID regex is strict - may reject reasonable variations
- Max 2 retries may be insufficient for complex failures (configurable via FLOW-004 escalation)

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-008 (Example) | `select_example_for_failure()` provides examples for retry prompts |
| GAP-FLOW-004 (Partial) | `handle_max_retries_exhausted()` invokes FLOW-004 escalation |
| GAP-FLOW-007 (Rollback) | Validation failures can trigger rollback analysis |
| GAP-FLOW-010 (Lifecycle) | Validation transitions gap states appropriately |
| GAP-FLOW-012 (Severity) | Reviewer validation checks for severity sections |

---

### Response to Round 1 Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-001 | Section 4 defines specific prompt modifications per failure type with concrete text templates |
| ISSUE-R1-002 | Fully resolved by GAP-FLOW-008; Section 4 integrates via `select_example_for_failure()` |
| ISSUE-R1-003 | Section 5 defines partial round semantics; integrates GAP-FLOW-004 escalation |
| ISSUE-R1-004 | Section 2 makes structural validation primary; character count is secondary sanity check |
| ISSUE-R1-005 | Section 3 defines gap ID format `GAP-{CATEGORY}-{NUMBER}` with regex `GAP-[A-Z]{2,10}-\d{3}` |
| ISSUE-R1-006 | Section 6 specifies ISO 8601 format: `YYYY-MM-DDTHH:MM:SSZ` |
| ISSUE-R1-007 | Section 7 defines standardized `ValidationResult` dataclass |

---

### New Gaps Introduced

None. This proposal completes GAP-FLOW-001 by integrating all resolved dependencies.

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-001 (Revised) | Proposed | HIGH | 0 |

**Changes from Round 1:**
- Structural validation as primary (not char count)
- Specific prompt modifications per failure type with concrete templates
- Gap ID format specification with regex
- ISO 8601 timestamps
- Standardized ValidationResult type
- Full integration with FLOW-004, FLOW-007, FLOW-008

**Dependencies Used:**
- GAP-FLOW-008: Example selection for retry prompts
- GAP-FLOW-004: Escalation when max retries exhausted
- GAP-FLOW-007: Rollback integration for persistent failures
- GAP-FLOW-010: Gap state transitions
- GAP-FLOW-012: Severity section validation for Reviewer

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Gap ID Format
```
Pattern: GAP-{CATEGORY}-{NUMBER}
Regex:   GAP-[A-Z]{2,10}-\d{3}
Example: GAP-FLOW-001
```

### Issue ID Format
```
Pattern: ISSUE-R{ROUND}-{NUMBER}
Regex:   ISSUE-R\d{1,2}-\d{3}
Example: ISSUE-R1-001
```

### Validation Tiers
```
Tier 1 (Structure):
  - File exists
  - File non-empty
  - Required headers present
  - Role-specific markers

Tier 2 (Content):
  - Gaps addressed (Engineer)
  - Cross-references valid
  - Substantive content (warning)
  - Trade-offs present (warning)
```

### Failure Type -> Prompt Modification
```
FILE_MISSING     -> Explicit path + Write tool reminder
EMPTY_OUTPUT     -> Min output template + "produce even if uncertain"
WRONG_FORMAT     -> Structural requirements + header list
NO_GAPS_ADDRESSED -> Explicit gap assignment + "begin with ##"
INCONSISTENT_REFS -> Valid gap list + "only reference these"
```

### Timestamp Format
```
ISO 8601: YYYY-MM-DDTHH:MM:SSZ
Example:  2026-01-12T14:30:00Z
```

### Retry Limits
```
Max retries: 2
On exhaustion: Invoke GAP-FLOW-004 escalation
```

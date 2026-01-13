# Engineer Proposals - Round 10

**Date:** 2026-01-12
**Focus:** Conflict Resolution Revision (GAP-FLOW-006)
**Priority:** Tier 1 - Final Flow Gap

**Context:** GAP-FLOW-006 (Conflict Resolution) was proposed in Round 1 but blocked on GAP-FLOW-013 (Implicit Disagreement Detection). Now that all dependencies are resolved, this proposal provides the complete conflict resolution specification, addressing all Round 1 Reviewer issues.

**Dependencies Resolved:**
- GAP-FLOW-013 (Implicit Disagreement): 5 detection heuristics, escalation thresholds, user actions
- GAP-FLOW-012 (Severity): CRITICAL/HIGH/MEDIUM/LOW weights for prioritization
- GAP-FLOW-005 (Termination): Integration with conflict resolution path via STALL_EXIT
- GAP-FLOW-010 (Gap Lifecycle): 8 states, transition rules for conflict outcomes
- GAP-FLOW-001 (Error Recovery): Validation ensures issue ID formats are parseable

---

## Gap Resolution: GAP-FLOW-006 (Revised)

**Gap:** No conflict resolution - What if Engineer disagrees with Reviewer?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-028 (Implicit disagreement detection hand-waved) | Fully resolved by GAP-FLOW-013; integrated with explicit cross-reference |
| ISSUE-R1-029 (Conflict detection requires issue IDs to match) | Defined mandatory ID reference format with validation; fallback matching |
| ISSUE-R1-030 (Engineer might overuse DISAGREE) | Added DISAGREE rate limiting with thresholds and user notification |
| ISSUE-R1-031 (Conflict presentation table underspecified) | Defined who generates options, option count limits, and synthesis rules |
| ISSUE-R1-032 (Resolution recording doesn't feed back to Engineer) | Added explicit prompt integration for decisions.md consumption |

---

### Revised Proposal

#### 1. Conflict Types and Detection Sources

Conflicts between Engineer and Reviewer arise from two sources: explicit disagreement and implicit disagreement. Both feed into the same resolution workflow.

**Conflict Sources:**

```
                    ┌─────────────────────────────────────────────────────────────┐
                    │                    CONFLICT SOURCES                          │
                    └─────────────────────────────────────────────────────────────┘

                           ┌──────────────────────────────────────┐
                           │      Reviewer Round N Output         │
                           │   (Issues with severity: ISSUE-RN-X) │
                           └──────────────────┬───────────────────┘
                                              │
                                              ▼
                           ┌──────────────────────────────────────┐
                           │      Engineer Round N+1 Output       │
                           └──────────────────┬───────────────────┘
                                              │
                    ┌─────────────────────────┴─────────────────────────┐
                    │                                                    │
                    ▼                                                    ▼
        ┌───────────────────────────┐                    ┌───────────────────────────┐
        │   EXPLICIT DISAGREEMENT   │                    │   IMPLICIT DISAGREEMENT   │
        │   (Engineer states        │                    │   (GAP-FLOW-013 detects   │
        │    "DISAGREE: ISSUE-X")   │                    │    unaddressed issue)     │
        └───────────────┬───────────┘                    └───────────────┬───────────┘
                        │                                                │
                        │                                                │
                        └────────────────────┬───────────────────────────┘
                                             │
                                             ▼
                           ┌──────────────────────────────────────┐
                           │        CONFLICT RESOLUTION           │
                           │           (This Spec)                │
                           └──────────────────────────────────────┘
```

**Conflict Definition:**

A conflict exists when:
1. **Explicit:** Engineer writes "DISAGREE: ISSUE-{X}" with rationale
2. **Implicit (via FLOW-013):** HIGH/CRITICAL issue detected as unaddressed or semantically non-responsive

---

#### 2. Explicit Disagreement Format (Addressing ISSUE-R1-029)

**Problem:** Round 1 allowed Engineer to reference issues informally, making conflict detection unreliable.

**Solution:** Require explicit issue ID reference format; Mediator validates.

**Mandatory Format for DISAGREE:**

```markdown
## DISAGREE: ISSUE-R{N}-{XXX}

**Reviewer Concern:**
> [Exact quote from reviewer.md, preserving original wording]

**Engineer Position:**
[Clear statement of why Engineer believes this issue should not be addressed as requested]

**Rationale:**
[Evidence-based argument. Must include at least one of:]
- Technical reason why Reviewer's approach is problematic
- Alternative interpretation of the requirement
- Trade-off analysis showing Reviewer's priority is incorrect
- External constraint (time, scope, dependency) that precludes addressing

**Alternative Approach (if any):**
[If Engineer proposes different solution to the underlying concern]

**Request:** Escalate to user for resolution
```

**Validation Requirements:**

```python
DISAGREE_PATTERN = r"## DISAGREE:\s*ISSUE-R(\d{1,2})-(\d{3})"

def validate_disagree_block(content: str, known_issues: list[str]) -> ValidationResult:
    """
    Validate DISAGREE blocks in Engineer output.
    Ensures proper format and references valid issues.
    """
    disagree_blocks = re.findall(DISAGREE_PATTERN, content)

    for round_num, issue_num in disagree_blocks:
        issue_id = f"ISSUE-R{round_num}-{issue_num}"

        # Check 1: Issue must exist
        if issue_id not in known_issues:
            return ValidationResult(
                success=False,
                failure_type="INVALID_DISAGREE_REF",
                message=f"DISAGREE references {issue_id} which does not exist"
            )

        # Check 2: Must have Reviewer Concern quote
        block_text = extract_disagree_block(content, issue_id)
        if "**Reviewer Concern:**" not in block_text:
            return ValidationResult(
                success=False,
                failure_type="MALFORMED_DISAGREE",
                message=f"DISAGREE for {issue_id} missing '**Reviewer Concern:**' quote"
            )

        # Check 3: Must have Rationale
        if "**Rationale:**" not in block_text:
            return ValidationResult(
                success=False,
                failure_type="MALFORMED_DISAGREE",
                message=f"DISAGREE for {issue_id} missing '**Rationale:**'"
            )

    return ValidationResult(success=True, disagree_issues=[
        f"ISSUE-R{r}-{n}" for r, n in disagree_blocks
    ])
```

**Fallback Matching (for soft references):**

If Engineer addresses an issue without using exact format, Mediator attempts semantic matching:

```python
def attempt_soft_issue_match(engineer_content: str, issue: Issue) -> MatchResult:
    """
    Attempt to match Engineer content to Reviewer issue without explicit ID.
    Used as fallback, not primary detection.
    """
    # Strategy 1: Look for issue summary keywords
    summary_keywords = extract_keywords(issue.summary)
    content_keywords = extract_keywords(engineer_content)
    keyword_overlap = len(summary_keywords & content_keywords) / len(summary_keywords)

    # Strategy 2: Look for quoted text from issue
    quote_found = issue.key_phrase in engineer_content

    # Strategy 3: Look for location reference
    location_referenced = issue.location in engineer_content

    if keyword_overlap >= 0.5 or quote_found or location_referenced:
        return MatchResult(
            matched=True,
            confidence="MEDIUM",
            reason="Soft match - Engineer may be addressing this issue"
        )

    return MatchResult(matched=False)
```

---

#### 3. DISAGREE Rate Limiting (Addressing ISSUE-R1-030)

**Problem:** Round 1 acknowledged Engineer might overuse DISAGREE to avoid work, but provided no mitigation.

**Solution:** Rate limiting with escalation thresholds.

**Rate Limits:**

| Threshold | Condition | Action |
|-----------|-----------|--------|
| Per-Round Limit | >50% of HIGH/CRITICAL issues get DISAGREE | User warning triggered |
| Session Pattern | DISAGREE rate >40% across 3+ rounds | Systematic alert to user |
| Absolute Limit | >5 DISAGREEs in single round | Block round, require justification |

**Rate Limit Implementation:**

```python
def check_disagree_rate(
    round_n: int,
    disagree_count: int,
    total_high_critical_issues: int,
    session_disagree_history: list[tuple[int, int, int]]  # (round, disagree, total)
) -> RateLimitResult:
    """
    Check if DISAGREE usage exceeds acceptable thresholds.
    """
    # Check 1: Per-round percentage
    if total_high_critical_issues > 0:
        round_rate = disagree_count / total_high_critical_issues
        if round_rate > 0.5:
            return RateLimitResult(
                action="WARN_USER",
                message=f"Engineer disagreed with {round_rate:.0%} of HIGH/CRITICAL issues this round.",
                recommendation="Review if issues are too prescriptive or Engineer needs guidance."
            )

    # Check 2: Absolute limit
    if disagree_count > 5:
        return RateLimitResult(
            action="BLOCK_ROUND",
            message=f"Engineer submitted {disagree_count} DISAGREEs (limit: 5 per round).",
            recommendation="Engineer must reduce disagreements or provide session-level justification."
        )

    # Check 3: Session pattern (3+ rounds)
    session_disagree_history.append((round_n, disagree_count, total_high_critical_issues))
    recent_rounds = session_disagree_history[-3:]

    if len(recent_rounds) >= 3:
        total_disagree = sum(d for _, d, _ in recent_rounds)
        total_issues = sum(t for _, _, t in recent_rounds)

        if total_issues > 0 and total_disagree / total_issues > 0.4:
            return RateLimitResult(
                action="SYSTEMATIC_ALERT",
                message=f"Systematic DISAGREE pattern: {total_disagree}/{total_issues} ({total_disagree/total_issues:.0%}) across last 3 rounds.",
                recommendation="Consider: (1) Reviewer feedback style too rigid, (2) Engineer/Reviewer misaligned on scope, (3) User intervention needed."
            )

    return RateLimitResult(action="ALLOW")
```

**User Warning Presentation:**

```
DISAGREE RATE WARNING

Engineer disagreed with 4 of 5 HIGH/CRITICAL issues this round (80%).

This could indicate:
1. Reviewer feedback is overly prescriptive
2. Engineer is avoiding work
3. Fundamental misalignment on approach

Options:
1. Accept disagreements - Proceed to conflict resolution for each
2. Request Engineer revision - Ask Engineer to reduce disagreements
3. Review Reviewer feedback - Check if issues were reasonable
4. Mediate scope - Clarify session goals before continuing
```

---

#### 4. Conflict Presentation and Option Generation (Addressing ISSUE-R1-031)

**Problem:** Round 1 showed a conflict table with Options A, B, C but didn't specify who generates them.

**Solution:** Mediator generates options following defined rules.

**Option Generation Rules:**

| Option | Generator | Source | Required? |
|--------|-----------|--------|-----------|
| Option A | Mediator | Engineer's position verbatim | Yes (always) |
| Option B | Mediator | Reviewer's suggestion verbatim | Yes (always) |
| Option C | Mediator (synthesized) | Attempt to combine A and B | If synthesis obvious |
| Option D | Mediator | "Neither - user specifies alternative" | If CRITICAL severity |

**Option Count Limits:**

- **Minimum:** 2 (Engineer position + Reviewer position)
- **Maximum:** 4 (A, B, C, D)
- **Synthesis Rule:** Option C generated only if clear middle ground exists (Mediator judgment)

**Conflict Presentation Format:**

```markdown
### Conflict: ISSUE-R{N}-{XXX}

**Subject:** {Issue summary - one line}
**Severity:** {CRITICAL | HIGH | MEDIUM | LOW}
**Gap Affected:** {GAP-XXX}

---

**Reviewer Position (Option A):**
{Exact quote from reviewer.md}

**Impact if ignored:** {From Reviewer's "Impact" field}

---

**Engineer Position (Option B):**
{Exact quote from Engineer's DISAGREE block}

**Rationale:** {From Engineer's "Rationale" field}

---

**Synthesized Option (Option C):** {Only if obvious middle ground}
{Mediator's proposed compromise}

**Trade-off:** {What each side gives up}

---

### Options Summary

| Option | Position | Trade-off | Recommended? |
|--------|----------|-----------|--------------|
| A | Reviewer | {Brief trade-off} | {If severity CRITICAL} |
| B | Engineer | {Brief trade-off} | - |
| C | Synthesis | {Brief trade-off} | {If both concerns addressed} |
| D | User specifies | Maximum flexibility | - |

**Note:** For CRITICAL severity, Option A (Reviewer) is recommended unless Engineer's rationale addresses safety/correctness concerns.
```

**Mediator Option Generation Logic:**

```python
def generate_conflict_options(
    issue: Issue,
    engineer_disagree: DisagreeBlock,
    mediator_analysis: dict
) -> list[ConflictOption]:
    """
    Generate options for user decision.
    """
    options = []

    # Option A: Reviewer position (always present)
    options.append(ConflictOption(
        label="A",
        name="Reviewer Position",
        description=issue.suggestion,
        trade_off="May add complexity or constraints per Engineer concern",
        recommended=issue.severity == "CRITICAL"
    ))

    # Option B: Engineer position (always present)
    options.append(ConflictOption(
        label="B",
        name="Engineer Position",
        description=engineer_disagree.position,
        trade_off=f"May leave unaddressed: {issue.impact}",
        recommended=False
    ))

    # Option C: Synthesis (conditional)
    synthesis = attempt_synthesis(issue, engineer_disagree)
    if synthesis.feasible:
        options.append(ConflictOption(
            label="C",
            name="Synthesis",
            description=synthesis.proposal,
            trade_off=synthesis.trade_off,
            recommended=synthesis.addresses_both
        ))

    # Option D: User specifies (for CRITICAL only)
    if issue.severity == "CRITICAL":
        options.append(ConflictOption(
            label="D",
            name="User Specifies Alternative",
            description="Provide your own resolution approach",
            trade_off="Requires additional user input",
            recommended=False
        ))

    return options


def attempt_synthesis(issue: Issue, disagree: DisagreeBlock) -> SynthesisResult:
    """
    Attempt to find middle ground between Reviewer and Engineer.
    """
    # Pattern 1: Reviewer wants feature, Engineer says too complex
    # Synthesis: Simplified version or configurable option
    if "complexity" in disagree.rationale.lower():
        return SynthesisResult(
            feasible=True,
            proposal=f"Implement {issue.suggestion} as optional/configurable, with simpler default",
            trade_off="Partial complexity for flexibility",
            addresses_both=True
        )

    # Pattern 2: Disagreement on threshold/value
    if "threshold" in issue.summary.lower() or "limit" in issue.summary.lower():
        return SynthesisResult(
            feasible=True,
            proposal=f"Make threshold configurable with default matching Reviewer's suggestion",
            trade_off="Configuration complexity for correctness",
            addresses_both=True
        )

    # Pattern 3: Scope disagreement
    if "out of scope" in disagree.rationale.lower():
        return SynthesisResult(
            feasible=True,
            proposal=f"Defer to v2 with explicit placeholder in spec",
            trade_off="Delayed implementation",
            addresses_both=False  # Doesn't fully address Reviewer concern
        )

    # No obvious synthesis
    return SynthesisResult(feasible=False)
```

---

#### 5. Resolution Recording and Feedback to Engineer (Addressing ISSUE-R1-032)

**Problem:** Round 1 showed resolution recording in decisions.md but no mechanism for Engineer to consume decisions.

**Solution:** Explicit prompt integration and delta mechanism.

**Resolution Recording Format (in decisions.md):**

```markdown
## Round {N} Conflict Resolutions

### ISSUE-R{M}-{XXX}: {Issue Summary}

**Conflict Type:** Explicit DISAGREE | Implicit (converted)
**Gap Affected:** GAP-{CATEGORY}-{NNN}
**Severity:** {CRITICAL | HIGH | MEDIUM | LOW}

**Positions:**
- Reviewer: {One-line summary}
- Engineer: {One-line summary}

**Resolution:**
- **Chosen Option:** {A | B | C | D}
- **Decision:** {Full description of what was decided}
- **Rationale:** {User's explanation for choice}
- **Constraints:** {Any conditions on implementation}

**Action Required:**
- For Engineer: {Specific instruction based on decision}
- For Gap: {State transition, e.g., "GAP-FLOW-006 may now transition to ACCEPTED"}

**Decided By:** User
**Timestamp:** {ISO 8601}
```

**Engineer Prompt Integration:**

The Mediator includes conflict resolution decisions in Engineer's next-round prompt:

```python
def build_engineer_prompt_with_decisions(
    round_n: int,
    assigned_gaps: list[str],
    conflict_resolutions: list[Resolution]
) -> str:
    """
    Build Engineer prompt including recent conflict resolutions.
    """
    base_prompt = build_base_engineer_prompt(round_n, assigned_gaps)

    if not conflict_resolutions:
        return base_prompt

    # Build decisions delta section
    decisions_section = """
═══════════════════════════════════════════════════════════════════════════════
                    CONFLICT RESOLUTIONS FROM PREVIOUS ROUND
═══════════════════════════════════════════════════════════════════════════════

The following conflicts were resolved by the user. You MUST honor these decisions.

"""

    for resolution in conflict_resolutions:
        decisions_section += f"""
### {resolution.issue_id}: {resolution.summary}

**User Decision:** Option {resolution.chosen_option} - {resolution.decision}
**Rationale:** {resolution.user_rationale}

**Your Action Required:**
{resolution.engineer_action}

---
"""

    decisions_section += """
═══════════════════════════════════════════════════════════════════════════════

IMPORTANT: Do not re-argue decided conflicts. Honor the user's decision and
proceed with implementation based on the chosen option.

If you believe the decision creates new issues, note them as NEW gaps, not
as disagreement with the decided conflict.
"""

    return f"{decisions_section}\n\n{base_prompt}"
```

**Validation That Engineer Honors Decisions:**

```python
def validate_decision_compliance(
    engineer_output: str,
    conflict_resolutions: list[Resolution]
) -> ValidationResult:
    """
    Check that Engineer doesn't re-argue decided conflicts.
    """
    for resolution in conflict_resolutions:
        # Check for re-disagreement
        if f"DISAGREE: {resolution.issue_id}" in engineer_output:
            return ValidationResult(
                success=False,
                failure_type="RE_ARGUED_CONFLICT",
                message=f"Engineer re-argued {resolution.issue_id} which was already resolved by user."
            )

        # Check for contradictory proposal (optional, harder to detect)
        if resolution.chosen_option != "B":  # B = Engineer position
            # If user didn't choose Engineer's position, Engineer should not
            # propose the same thing again without new rationale
            if contains_same_proposal(engineer_output, resolution.engineer_position):
                return ValidationResult(
                    success=True,
                    warnings=[f"Engineer may be proposing same position rejected in {resolution.issue_id}"]
                )

    return ValidationResult(success=True)
```

---

#### 6. Integration with GAP-FLOW-013 (Implicit Disagreement)

When GAP-FLOW-013 detects implicit disagreement, it feeds into this conflict resolution flow:

**Implicit -> Explicit Conversion:**

```python
def convert_implicit_to_conflict(
    implicit_disagreement: ImplicitDisagreement,
    round_n: int
) -> Conflict:
    """
    Convert detected implicit disagreement to formal conflict.
    Per GAP-FLOW-013 Action 2.
    """
    return Conflict(
        issue_id=implicit_disagreement.issue_id,
        conflict_type="IMPLICIT_CONVERTED",
        reviewer_position=implicit_disagreement.issue.suggestion,
        engineer_position="Not explicitly stated - Engineer did not address this issue",
        detection_source="GAP-FLOW-013",
        detection_heuristic=implicit_disagreement.detection_method,
        timestamp=now(),

        # For presentation, note the implicit nature
        presentation_note="""
NOTE: This conflict was detected automatically because Engineer did not
explicitly address this HIGH/CRITICAL issue. Engineer may have:
1. Overlooked the issue (oversight)
2. Intentionally not addressed it (implicit disagreement)
3. Addressed it in substance without referencing the issue ID

User should consider requesting clarification before resolving.
"""
    )
```

**Flow Integration:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    GAP-FLOW-013 -> GAP-FLOW-006 INTEGRATION                  │
└─────────────────────────────────────────────────────────────────────────────┘

GAP-FLOW-013 Detection:
┌──────────────────────────────────────────────────────────────────────────┐
│  Heuristic 1: Unaddressed HIGH/CRITICAL ID                               │
│  Heuristic 4: Semantic non-response                                      │
│  Heuristic 5: Deflection pattern                                         │
└─────────────────────────────────┬────────────────────────────────────────┘
                                  │
                                  │ User selects "Convert to DISAGREE"
                                  │ (GAP-FLOW-013 Action 2)
                                  │
                                  ▼
GAP-FLOW-006 Conflict Resolution:
┌──────────────────────────────────────────────────────────────────────────┐
│  1. Create Conflict with type=IMPLICIT_CONVERTED                         │
│  2. Generate options (A: Reviewer, B: "Engineer chose not to address")   │
│  3. Present to user with implicit nature noted                           │
│  4. User decides                                                         │
│  5. Record in decisions.md                                               │
│  6. Feed decision to next Engineer prompt                                │
└──────────────────────────────────────────────────────────────────────────┘
```

---

#### 7. Integration with GAP-FLOW-012 (Severity)

Conflict resolution priority and behavior varies by issue severity:

**Severity-Based Conflict Handling:**

| Severity | Resolution Priority | Blocking? | Recommendation |
|----------|---------------------|-----------|----------------|
| CRITICAL | Immediate | Yes - round cannot complete | Option A (Reviewer) unless safety concern |
| HIGH | High | Yes - blocks gap acceptance | No default recommendation |
| MEDIUM | Normal | No | May batch with other conflicts |
| LOW | Low | No | May auto-resolve to Engineer if pattern |

**Conflict Queue Ordering:**

```python
def prioritize_conflicts(conflicts: list[Conflict]) -> list[Conflict]:
    """
    Order conflicts for user resolution.
    CRITICAL first, then HIGH, then by round (older first).
    """
    severity_order = {"CRITICAL": 0, "HIGH": 1, "MEDIUM": 2, "LOW": 3}

    return sorted(conflicts, key=lambda c: (
        severity_order[c.issue.severity],
        c.issue.round,
        c.issue.number
    ))
```

---

#### 8. Integration with GAP-FLOW-005 (Termination)

Unresolved conflicts affect termination:

**Termination Impact:**

| Conflict Status | Termination Impact |
|-----------------|-------------------|
| CRITICAL conflict unresolved | Blocks all termination except ABANDONED |
| HIGH conflict unresolved | Blocks COMPLETE, GOOD_ENOUGH; allows USER_APPROVED with warning |
| MEDIUM/LOW conflict unresolved | No blocking; documented as known limitation |

**Decision Tree Integration:**

```
From GAP-FLOW-005 Termination Decision Tree:

┌─ Check: Unresolved CRITICAL conflicts?
│   └─ YES ──► Present conflicts, require resolution before termination
│   └─ NO
│       │
│       ▼
┌─ Check: Unresolved HIGH conflicts?
│   └─ YES ──► If USER_APPROVED: Warning + acknowledgment required
│               If other: Must resolve first
│   └─ NO ──► Normal termination flow
```

---

#### 9. Complete Conflict Resolution Workflow

**End-to-End Flow:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    COMPLETE CONFLICT RESOLUTION WORKFLOW                     │
└─────────────────────────────────────────────────────────────────────────────┘

Phase 1: Detection
──────────────────────────────────────────────────────────────────────────────
                    ┌──────────────────────────────────┐
                    │   Round N: Reviewer Output       │
                    │   Issues: ISSUE-RN-001 (HIGH)    │
                    │           ISSUE-RN-002 (CRITICAL)│
                    └─────────────────┬────────────────┘
                                      │
                                      ▼
                    ┌──────────────────────────────────┐
                    │   Round N+1: Engineer Output     │
                    └─────────────────┬────────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    │                                   │
                    ▼                                   ▼
        ┌───────────────────────┐         ┌───────────────────────┐
        │ Contains:             │         │ Missing:              │
        │ "DISAGREE: ISSUE-RN-001"│       │ Reference to ISSUE-RN-002│
        └───────────┬───────────┘         └───────────┬───────────┘
                    │                                 │
                    ▼                                 ▼
        ┌───────────────────────┐         ┌───────────────────────┐
        │ EXPLICIT CONFLICT     │         │ GAP-FLOW-013 flags    │
        │ Created for           │         │ IMPLICIT potential    │
        │ ISSUE-RN-001          │         │ for ISSUE-RN-002      │
        └───────────┬───────────┘         └───────────┬───────────┘
                    │                                 │
                    │                                 │ User: "Convert to DISAGREE"
                    │                                 │
                    └─────────────────┬───────────────┘
                                      │
                                      ▼

Phase 2: Rate Check
──────────────────────────────────────────────────────────────────────────────
                    ┌──────────────────────────────────┐
                    │   DISAGREE Rate Check            │
                    │   2 conflicts / 2 issues = 100%  │
                    │   Threshold: >50% HIGH/CRITICAL  │
                    └─────────────────┬────────────────┘
                                      │
                                      ▼
                    ┌──────────────────────────────────┐
                    │   WARN USER                      │
                    │   "Engineer disagreed with 100%  │
                    │    of HIGH/CRITICAL issues"      │
                    └─────────────────┬────────────────┘
                                      │
                                      │ User: "Accept disagreements"
                                      ▼

Phase 3: Presentation
──────────────────────────────────────────────────────────────────────────────
                    ┌──────────────────────────────────┐
                    │   Prioritize Conflicts           │
                    │   1. ISSUE-RN-002 (CRITICAL)     │
                    │   2. ISSUE-RN-001 (HIGH)         │
                    └─────────────────┬────────────────┘
                                      │
                                      ▼

        ┌─────────────────────────────────────────────────────────────────┐
        │ CONFLICT 1 of 2: ISSUE-RN-002 (CRITICAL)                        │
        │                                                                 │
        │ Subject: Backup files create inconsistency risk                 │
        │ Severity: CRITICAL                                              │
        │ Gap: GAP-FLOW-007                                               │
        │                                                                 │
        │ Option A (Reviewer): Keep last 3 backups                        │
        │ Option B (Engineer): [Implicit - did not address]               │
        │ Option C (Synthesis): Configurable retention (default: 3)       │
        │ Option D: User specifies alternative                            │
        │                                                                 │
        │ NOTE: This conflict was detected implicitly.                    │
        │                                                                 │
        │ Choose option (A/B/C/D): ___                                    │
        └─────────────────────────────────────────────────────────────────┘

                                      │
                                      │ User: "C"
                                      ▼

Phase 4: Recording
──────────────────────────────────────────────────────────────────────────────
                    ┌──────────────────────────────────┐
                    │   Record in decisions.md         │
                    │   - Chosen: Option C             │
                    │   - Rationale: "Best of both"    │
                    │   - Action: Engineer to implement│
                    │     configurable retention       │
                    └─────────────────┬────────────────┘
                                      │
                                      ▼

Phase 5: Next Round Integration
──────────────────────────────────────────────────────────────────────────────
                    ┌──────────────────────────────────┐
                    │   Round N+2 Engineer Prompt      │
                    │                                  │
                    │   "CONFLICT RESOLUTIONS:         │
                    │    ISSUE-RN-002: Option C chosen │
                    │    Your action: Implement        │
                    │    configurable backup retention │
                    │    with default=3"               │
                    └──────────────────────────────────┘
```

---

### Examples

**Example 1: Explicit Disagreement - Clean Resolution**

```
Round 5 Reviewer:
- ISSUE-R5-003 (HIGH): "Retry mechanism should use exponential backoff"
  Suggestion: "Implement 1s, 2s, 4s delays between retries"
  Impact: "Linear retries may overwhelm failed services"

Round 6 Engineer:
## DISAGREE: ISSUE-R5-003

**Reviewer Concern:**
> "Retry mechanism should use exponential backoff. Implement 1s, 2s, 4s delays
> between retries. Linear retries may overwhelm failed services."

**Engineer Position:**
Exponential backoff adds complexity for marginal benefit in this context.

**Rationale:**
The retry target here is local file validation, not remote services. File system
operations complete or fail fast (<100ms). Adding delays would only slow the
workflow without reducing failure likelihood.

If retries are hitting rate limits, the problem is upstream (e.g., slow disk),
not retry frequency.

**Alternative Approach:**
Keep instant retries for file operations. Add note that remote operation retries
(future scope) should use backoff.

**Request:** Escalate to user for resolution

──────────────────────────────────────────────────────────────────────────────

Mediator validates:
- ISSUE-R5-003 exists: YES
- Reviewer Concern quoted: YES
- Rationale provided: YES
- Format valid: YES

Mediator generates options:
- Option A: Exponential backoff (1s, 2s, 4s) for all retries
- Option B: Instant retries for file operations (Engineer's position)
- Option C: Context-aware - instant for local, backoff for remote (synthesis)

Conflict presentation:
### Conflict: ISSUE-R5-003

**Subject:** Retry timing mechanism
**Severity:** HIGH
**Gap Affected:** GAP-FLOW-001

---

**Reviewer Position (Option A):**
"Implement 1s, 2s, 4s delays between retries"

**Impact if ignored:** "Linear retries may overwhelm failed services"

---

**Engineer Position (Option B):**
"Keep instant retries - file operations complete fast, delays only slow workflow"

**Rationale:** Target is local validation, not remote services.

---

**Synthesized Option (Option C):**
Implement context-aware retry timing:
- File operations: Instant retry (no delay)
- Remote operations (future): Exponential backoff
Add configuration flag for override.

**Trade-off:** Documentation overhead + future implementation complexity

---

User selects: Option C

decisions.md entry:
### ISSUE-R5-003: Retry timing mechanism

**Conflict Type:** Explicit DISAGREE
**Gap Affected:** GAP-FLOW-001
**Severity:** HIGH

**Positions:**
- Reviewer: Exponential backoff for all retries
- Engineer: Instant retries for file operations

**Resolution:**
- **Chosen Option:** C (Synthesis)
- **Decision:** Context-aware retry timing. Instant for file ops, note that remote
  operations should use backoff when implemented.
- **Rationale:** Both concerns valid - file ops are fast, remote ops need backoff
- **Constraints:** Must document which operations use which strategy

**Action Required:**
- For Engineer: Update retry implementation to note context-awareness. Add comment
  for future remote operation backoff.
- For Gap: GAP-FLOW-001 may proceed with this approach.

**Decided By:** User
**Timestamp:** 2026-01-12T15:30:00Z
```

**Example 2: Implicit Disagreement - Conversion and Resolution**

```
Round 4 Reviewer:
- ISSUE-R4-007 (CRITICAL): "Schema validation must reject unknown fields"
  Suggestion: "Add strict mode validation that fails on unexpected keys"
  Impact: "Silent data loss if unexpected fields ignored"

Round 5 Engineer output:
- Contains no reference to ISSUE-R4-007
- Schema section unchanged from Round 4

GAP-FLOW-013 Detection:
- Heuristic 1 (Unaddressed ID): TRIGGER
  - ISSUE-R4-007 is CRITICAL
  - Engineer output has no "Response to ISSUE-R4-007" or "DISAGREE: ISSUE-R4-007"
  - Gap GAP-COMM-004 still PROPOSED
- Heuristic 4 (Semantic): TRIGGER
  - Schema section unchanged (0% modification)
- Confidence: HIGH

GAP-FLOW-013 presents to user:
"POTENTIAL IMPLICIT DISAGREEMENT DETECTED

Issue: ISSUE-R4-007 (CRITICAL)
'Schema validation must reject unknown fields'

Evidence:
- Round 5 Engineer output contains no reference to ISSUE-R4-007
- Engineer addressed 3 other issues from Round 4 Reviewer
- Schema section unchanged from Round 4
- Issue severity is CRITICAL

Options:
1. Request Engineer explicitly address this issue
2. Convert to explicit DISAGREE for conflict resolution
3. Let user decide if issue should be waived
4. Ignore this flag (false positive)"

User selects: Option 2 (Convert to DISAGREE)

──────────────────────────────────────────────────────────────────────────────

GAP-FLOW-006 receives converted conflict:

Conflict created:
- issue_id: ISSUE-R4-007
- conflict_type: IMPLICIT_CONVERTED
- reviewer_position: "Add strict mode validation that fails on unexpected keys"
- engineer_position: "Not explicitly stated - Engineer did not address this issue"
- detection_source: GAP-FLOW-013

Mediator generates options:
- Option A: Strict mode validation (Reviewer) [RECOMMENDED for CRITICAL]
- Option B: No change (implicit Engineer position)
- Option C: Configurable strictness with default=strict
- Option D: User specifies alternative

Conflict presentation:
### Conflict: ISSUE-R4-007

**Subject:** Schema validation strictness
**Severity:** CRITICAL
**Gap Affected:** GAP-COMM-004

---

**Reviewer Position (Option A):** [RECOMMENDED]
"Add strict mode validation that fails on unexpected keys"

**Impact if ignored:** "Silent data loss if unexpected fields ignored"

---

**Engineer Position (Option B):**
Not explicitly stated. Engineer did not address this CRITICAL issue in Round 5.

NOTE: This conflict was detected automatically because Engineer did not
explicitly address this HIGH/CRITICAL issue.

---

**Synthesized Option (Option C):**
Configurable strictness flag (--strict/--lenient) with default=strict

**Trade-off:** Flexibility at cost of flag complexity

---

**Option D: User Specifies Alternative**
(Available for CRITICAL severity)

---

User selects: Option A (recommended for CRITICAL)

decisions.md entry:
### ISSUE-R4-007: Schema validation strictness

**Conflict Type:** Implicit (converted via GAP-FLOW-013)
**Gap Affected:** GAP-COMM-004
**Severity:** CRITICAL

**Resolution:**
- **Chosen Option:** A (Reviewer)
- **Decision:** Implement strict mode validation. Fail on unexpected keys.
- **Rationale:** CRITICAL severity - cannot risk silent data loss
- **Constraints:** None

**Action Required:**
- For Engineer: Implement strict validation. Must reject unknown fields.
- For Gap: GAP-COMM-004 must include strict validation before ACCEPTED.

**Decided By:** User
**Timestamp:** 2026-01-12T16:00:00Z

──────────────────────────────────────────────────────────────────────────────

Round 6 Engineer prompt includes:

═══════════════════════════════════════════════════════════════════════════════
                    CONFLICT RESOLUTIONS FROM PREVIOUS ROUND
═══════════════════════════════════════════════════════════════════════════════

### ISSUE-R4-007: Schema validation strictness

**User Decision:** Option A - Implement strict mode validation

**Your Action Required:**
Implement strict validation. Schema parser must reject unknown fields.
This was a CRITICAL issue and the user chose the Reviewer's position.

Do not re-argue this decision. Implement strict validation.

═══════════════════════════════════════════════════════════════════════════════
```

**Example 3: DISAGREE Rate Limit Triggered**

```
Round 7 Engineer output:
- DISAGREE: ISSUE-R7-001 (HIGH)
- DISAGREE: ISSUE-R7-002 (HIGH)
- DISAGREE: ISSUE-R7-003 (HIGH)
- DISAGREE: ISSUE-R7-004 (CRITICAL)
- Response to ISSUE-R7-005 (MEDIUM) - addressed

Rate check:
- Total HIGH/CRITICAL issues in Round 6 Reviewer: 4
- DISAGREEs for HIGH/CRITICAL: 4
- Rate: 100% (exceeds 50% threshold)

Mediator presents warning:
──────────────────────────────────────────────────────────────────────────────
DISAGREE RATE WARNING

Engineer disagreed with 4 of 4 HIGH/CRITICAL issues this round (100%).

Issues disagreed with:
1. ISSUE-R7-001 (HIGH): Timestamp format consistency
2. ISSUE-R7-002 (HIGH): Error message verbosity
3. ISSUE-R7-003 (HIGH): Validation timeout threshold
4. ISSUE-R7-004 (CRITICAL): Recovery data persistence

This could indicate:
1. Reviewer feedback is overly prescriptive
2. Engineer is avoiding work
3. Fundamental misalignment on approach

Options:
1. Accept disagreements - Proceed to conflict resolution for each
2. Request Engineer revision - Ask Engineer to reduce disagreements
3. Review Reviewer feedback - Check if Round 6 Reviewer issues were reasonable
4. Mediate scope - Clarify session goals before continuing
──────────────────────────────────────────────────────────────────────────────

User selects: Option 3 (Review Reviewer feedback)

User reviews Round 6 Reviewer and notes:
"Reviewer was too prescriptive on ISSUE-R7-001 and R7-002. These should have
been suggestions, not requirements. However, R7-003 and R7-004 are valid."

User decision:
- ISSUE-R7-001: Override to LOW severity (not a conflict)
- ISSUE-R7-002: Override to LOW severity (not a conflict)
- ISSUE-R7-003: Proceed to conflict resolution
- ISSUE-R7-004: Proceed to conflict resolution

Effective DISAGREE rate after user intervention: 2/2 = 100%
(But only 2 conflicts to resolve, not 4)
```

---

### Trade-offs

**Pros:**
- Explicit ID reference format ensures reliable conflict detection
- Rate limiting prevents DISAGREE abuse while preserving legitimate disagreement
- Option generation rules are deterministic and auditable
- Feedback loop ensures Engineer honors user decisions
- Integration with FLOW-013 catches both explicit and implicit conflicts
- Severity-based prioritization focuses user attention on critical decisions
- Synthesis option (C) provides middle ground when obvious

**Cons:**
- Mandatory DISAGREE format may feel bureaucratic for minor disagreements
- Rate limits (50%, 5 per round) are heuristic and may need tuning
- Option C synthesis is Mediator judgment - may miss better compromises
- Feedback validation (re-arguing check) has false positive risk
- Implicit conflict conversion requires user action (not automatic resolution)
- CRITICAL conflicts blocking termination may frustrate time-constrained users

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-013 (Implicit) | Section 6: Converted implicit disagreements feed directly into conflict resolution |
| GAP-FLOW-012 (Severity) | Section 7: Conflict priority and blocking behavior by severity level |
| GAP-FLOW-010 (Lifecycle) | Resolution enables gap state transitions (PROPOSED -> ACCEPTED) |
| GAP-FLOW-005 (Termination) | Section 8: Unresolved conflicts affect termination criteria |
| GAP-FLOW-001 (Error Recovery) | Section 2: Uses same issue ID format and validation patterns |

---

### Response to Round 1 Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-028 | Section 6 fully integrates GAP-FLOW-013 implicit disagreement detection. Explicit cross-reference to heuristics and conversion flow. |
| ISSUE-R1-029 | Section 2 defines mandatory `## DISAGREE: ISSUE-R{N}-{XXX}` format with validation. Fallback soft matching for edge cases. |
| ISSUE-R1-030 | Section 3 defines rate limiting: >50% per round = warning, >40% over 3 rounds = systematic alert, >5 per round = block. |
| ISSUE-R1-031 | Section 4 specifies: Mediator generates options, A=Reviewer always, B=Engineer always, C=synthesis if obvious, D=user input for CRITICAL. Min 2, max 4 options. |
| ISSUE-R1-032 | Section 5 defines `build_engineer_prompt_with_decisions()` that includes resolution delta. Validation checks for re-arguing decided conflicts. |

---

### New Gaps Introduced

None. This proposal completes GAP-FLOW-006 as the final Flow gap.

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-006 (Revised) | Proposed | HIGH | 0 |

**Changes from Round 1:**
- Mandatory issue ID reference format with validation
- DISAGREE rate limiting (50% warning, 5 absolute limit)
- Defined option generation rules (who, how many, synthesis criteria)
- Explicit Engineer prompt integration for decisions
- Full integration with GAP-FLOW-013 implicit disagreement flow
- Severity-based conflict prioritization and blocking

**Dependencies Used:**
- GAP-FLOW-013: Implicit disagreement detection and conversion
- GAP-FLOW-012: Severity levels for prioritization and blocking
- GAP-FLOW-005: Termination criteria interaction
- GAP-FLOW-010: Gap state transitions on resolution
- GAP-FLOW-001: Issue ID format consistency

**This is the final Flow gap. All GAP-FLOW-* gaps are now resolved or proposed.**

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### DISAGREE Format
```markdown
## DISAGREE: ISSUE-R{N}-{XXX}

**Reviewer Concern:**
> [Quote]

**Engineer Position:**
[Statement]

**Rationale:**
[Evidence]

**Alternative Approach (if any):**
[Optional]

**Request:** Escalate to user for resolution
```

### Rate Limits
```
Per-round: >50% HIGH/CRITICAL -> User warning
Session:   >40% over 3 rounds -> Systematic alert
Absolute:  >5 per round       -> Block round
```

### Option Generation
```
Option A: Reviewer position (always)
Option B: Engineer position (always)
Option C: Synthesis (if obvious middle ground)
Option D: User specifies (CRITICAL only)
Min: 2, Max: 4
```

### Conflict Priority (by severity)
```
1. CRITICAL - Immediate, blocks termination
2. HIGH     - High priority, blocks COMPLETE/GOOD_ENOUGH
3. MEDIUM   - Normal priority, non-blocking
4. LOW      - Low priority, may batch or auto-resolve
```

### decisions.md Entry Format
```markdown
### ISSUE-R{N}-{XXX}: {Summary}

**Conflict Type:** Explicit | Implicit (converted)
**Gap Affected:** GAP-{CAT}-{NNN}
**Severity:** {CRITICAL | HIGH | MEDIUM | LOW}

**Resolution:**
- **Chosen Option:** {A | B | C | D}
- **Decision:** {Description}
- **Rationale:** {User explanation}
- **Constraints:** {Any conditions}

**Action Required:**
- For Engineer: {Specific instruction}
- For Gap: {State transition}

**Decided By:** User
**Timestamp:** {ISO 8601}
```

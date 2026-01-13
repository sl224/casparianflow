# Engineer Proposals - Round 4

**Date:** 2026-01-12
**Focus:** Termination Criteria Revision (GAP-FLOW-005)
**Priority:** Tier 1 - Unblocked by Round 2 Foundations

**Context:** GAP-FLOW-010 (Gap Lifecycle) and GAP-FLOW-012 (Severity Definitions) were RESOLVED in Round 2. GAP-FLOW-002 (Stall Detection) was APPROVED in Round 3. This proposal revises GAP-FLOW-005 (Termination Criteria) to integrate these foundations and address Reviewer issues from Round 1.

---

## Gap Resolution: GAP-FLOW-005 (Revised)

**Gap:** Termination criteria incomplete - When is "good enough"?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-023 ("Reviewer APPROVE" poorly defined) | Replaced with explicit "Final Review Pass" that confirms no new issues, not approval of proposals |
| ISSUE-R1-024 (USER_APPROVED allows premature closure with CRITICAL gaps) | Added mandatory warning gate with severity-aware confirmation flow |
| ISSUE-R1-025 (Severity definitions missing) | Now uses GAP-FLOW-012 severity levels - RESOLVED |
| ISSUE-R1-026 (10-round limit rationale missing) | Added formula-based limit with rationale; made configurable |
| ISSUE-R1-027 (Duration tracking undefined) | Added explicit timing mechanism with ISO 8601 timestamps |

---

### Revised Proposal

#### Termination Types

| Termination Type | Definition | Requirements |
|------------------|------------|--------------|
| **COMPLETE** | All gaps resolved with confirmation | Zero CRITICAL, Zero HIGH, Zero MEDIUM in terminal states |
| **GOOD_ENOUGH** | Acceptable quality with minor gaps | Zero CRITICAL, Zero HIGH, MEDIUM/LOW acknowledged |
| **USER_APPROVED** | User accepts known limitations | User explicitly acknowledges remaining gaps (with warnings) |
| **MAX_ROUNDS** | Hard limit reached | Round count >= max_rounds (configurable) |
| **STALL_EXIT** | User exits after divergence warning | Via DIVERGENCE_WARNING -> COMPLETE path (GAP-FLOW-002) |
| **ABANDONED** | User cancels session | Explicit user action, no output generated |

---

#### Termination Requirements Matrix (Using GAP-FLOW-012 Severity)

| Termination Type | CRITICAL | HIGH | MEDIUM | LOW | User Action |
|------------------|----------|------|--------|-----|-------------|
| COMPLETE (auto) | 0 | 0 | 0 open, any accepted/deferred | any | None - automatic |
| GOOD_ENOUGH | 0 | 0 | any (acknowledged) | any | Acknowledge MEDIUM gaps |
| USER_APPROVED | 0 (or warning-confirmed) | 0 (or warning-confirmed) | any | any | Explicit confirmation |
| MAX_ROUNDS | any | any | any | any | Warning shown, user forced to choose |
| STALL_EXIT | any | any | any | any | Via divergence flow |
| ABANDONED | N/A | N/A | N/A | N/A | Session discarded |

**Key Principle:** Severity determines automation level. Lower severity = more user control. Higher severity = more guardrails.

---

#### "Good Enough" Determination (Severity-Based)

**Automatic Termination (COMPLETE):**

A session terminates automatically when ALL of the following are true:
1. Zero gaps in OPEN, IN_PROGRESS, PROPOSED, or NEEDS_REVISION state
2. All gaps are in ACCEPTED, RESOLVED, WONT_FIX, or USER_DEFERRED state
3. Zero CRITICAL-severity gaps in USER_DEFERRED state
4. Zero HIGH-severity gaps in USER_DEFERRED state
5. Final Review Pass confirms no new issues

**Weighted Threshold Formula:**
```
# Using GAP-FLOW-012 severity weights
severity_weight = {CRITICAL: 16, HIGH: 4, MEDIUM: 2, LOW: 1}

remaining_open_weight = sum(severity_weight[g.severity] for g in open_gaps)
deferred_weight = sum(severity_weight[g.severity] for g in USER_DEFERRED_gaps)

# COMPLETE requires:
remaining_open_weight == 0
AND deferred_weight <= 4  # At most 2 MEDIUM or 4 LOW deferred
```

**"Good Enough" Termination:**

User may accept GOOD_ENOUGH when:
```
# Zero CRITICAL in any non-terminal state
count(CRITICAL in [OPEN, IN_PROGRESS, PROPOSED, NEEDS_REVISION, USER_DEFERRED]) == 0

# Zero HIGH in any non-terminal state
count(HIGH in [OPEN, IN_PROGRESS, PROPOSED, NEEDS_REVISION, USER_DEFERRED]) == 0

# User acknowledges remaining MEDIUM/LOW gaps
```

---

#### Final Review Pass (Replaces "Reviewer APPROVE")

**Addressing ISSUE-R1-023:** The original "Reviewer APPROVE" was ambiguous. This revision defines an explicit Final Review Pass.

**Definition:** A Final Review Pass is NOT approval of proposals. It is a completeness check confirming:
1. No new gaps discovered during review
2. No existing gaps regressed in state
3. All critical and high-severity issues addressed
4. Spec is internally consistent

**Final Review Pass Protocol:**

```markdown
## Final Review Pass - Round N

### Completeness Check
- [ ] All CRITICAL gaps in terminal state (RESOLVED or WONT_FIX)
- [ ] All HIGH gaps in terminal state or USER_DEFERRED with acknowledgment
- [ ] No new gaps identified during this pass
- [ ] No unaddressed Reviewer issues from previous rounds

### Consistency Check
- [ ] No circular dependencies between resolved gaps
- [ ] No conflicting definitions across gap resolutions
- [ ] All examples are valid per defined rules

### Readiness Assessment
- [ ] Spec can be implemented without ambiguity on core features
- [ ] Known limitations are documented
- [ ] Edge cases have defined behavior (even if "undefined behavior")

### Verdict
[ ] READY_FOR_TERMINATION - Meets COMPLETE or GOOD_ENOUGH criteria
[ ] NOT_READY - Issues found (list below)

### Issues Found (if NOT_READY)
- [Issue description with severity]
```

**Key Distinction:**
- Round N-1 Reviewer: Critiques proposals, raises issues
- Final Review Pass: Confirms readiness, does NOT raise new issues except blockers

---

#### User-Acknowledged Gaps (Addressing ISSUE-R1-024)

**Definition:** "User-acknowledged" means the user has:
1. Seen a clear description of the remaining gap
2. Seen the severity level and its implications
3. Explicitly confirmed acceptance via AskUserQuestion
4. Provided a rationale (required for HIGH, optional for MEDIUM/LOW)

**Operational Flow for USER_APPROVED with HIGH/CRITICAL:**

```
Step 1: Present Warning
─────────────────────────────────────────────────────────
WARNING: You are accepting termination with HIGH-severity gaps.

Remaining HIGH gaps (2):
1. GAP-FLOW-013: Implicit disagreement detection
   Impact: Engineer may ignore Reviewer feedback without detection

2. GAP-COMM-005: Large document handling
   Impact: Sessions with >100 gaps may exceed context limits

Severity Definition Reminder:
HIGH = "Implementation will be incorrect/incomplete without this"

─────────────────────────────────────────────────────────

Step 2: Require Explicit Confirmation
─────────────────────────────────────────────────────────
To accept these gaps as known limitations, you must:

1. Type "I acknowledge" to confirm understanding
2. Provide rationale for each HIGH gap:

GAP-FLOW-013 rationale (required): _______________
GAP-COMM-005 rationale (required): _______________

Options:
[A] I acknowledge - Accept and provide rationale
[B] Continue working - Address HIGH gaps first
[C] Defer decision - Save session for later
─────────────────────────────────────────────────────────

Step 3: Record in decisions.md
─────────────────────────────────────────────────────────
### USER_APPROVED - Round 7

**Termination Type:** USER_APPROVED
**Remaining Gaps:** 2 HIGH, 3 MEDIUM, 1 LOW

**HIGH Gap Acknowledgments:**

#### GAP-FLOW-013 - Implicit Disagreement Detection
- **Severity:** HIGH
- **User Decision:** Accept as known limitation
- **User Rationale:** "Manual detection acceptable for v1. Will address in v2."
- **Confirmed:** 2026-01-12T14:30:00Z

#### GAP-COMM-005 - Large Document Handling
- **Severity:** HIGH
- **User Decision:** Accept as known limitation
- **User Rationale:** "Our specs are <50 gaps. Not a practical concern."
- **Confirmed:** 2026-01-12T14:30:00Z

**Session Complete:** 2026-01-12T14:30:00Z
─────────────────────────────────────────────────────────
```

**CRITICAL Gap Handling:**

If CRITICAL gaps remain, add additional warning tier:

```
DANGER: You are accepting termination with CRITICAL-severity gaps.

CRITICAL = "Spec cannot be implemented without this"

Remaining CRITICAL gaps (1):
1. GAP-FLOW-020: Schema validation undefined
   Impact: Parser cannot validate input. Core functionality broken.

THIS IS STRONGLY DISCOURAGED. The resulting spec may be unusable.

To proceed, type exactly: "I accept CRITICAL gaps knowing the spec may be unusable"

[A] Accept CRITICAL gaps (NOT RECOMMENDED)
[B] Continue working - Address CRITICAL gaps first (RECOMMENDED)
[C] Abandon session
```

---

#### Round Limit (Addressing ISSUE-R1-026)

**Formula-Based Default:**

```
max_rounds = max(10, ceil(initial_gap_count * 0.6))
```

**Rationale:**
- Minimum 10 rounds ensures simple specs have room for iteration
- 0.6 multiplier: Empirical observation that well-scoped specs resolve ~1.5-2 gaps per round
- A 25-gap spec gets max 15 rounds (25 * 0.6 = 15)
- A 50-gap spec gets max 30 rounds (50 * 0.6 = 30)

**Configuration:**

```markdown
## Session Configuration

| Parameter | Default | Formula | Override |
|-----------|---------|---------|----------|
| max_rounds | 10 | max(10, ceil(initial_gaps * 0.6)) | User may set explicitly |
| auto_extend | false | N/A | If true, warn at max but allow continue |
| hard_limit | 50 | N/A | Absolute maximum regardless of formula |
```

**MAX_ROUNDS Behavior:**

When round count reaches max_rounds:

```
SESSION LIMIT REACHED

You have completed {max_rounds} rounds.
Initial gaps: {initial_count}
Current gaps: {current_count}
Net progress: {net}

=== Gap Status ===
CRITICAL: {n} ({list})
HIGH: {n} ({list})
MEDIUM: {n}
LOW: {n}

=== Options ===
1. Extend session - Add 5 more rounds
2. Accept current state - Generate final spec with known gaps
3. Abandon - Discard session

Note: Extending beyond {hard_limit} rounds is not permitted.
```

---

#### Duration Tracking (Addressing ISSUE-R1-027)

**Timing Mechanism:**

| Event | Timestamp Field | Format |
|-------|-----------------|--------|
| Session start | session_started_at | ISO 8601 (2026-01-12T10:00:00Z) |
| Round start | round_{n}_started_at | ISO 8601 |
| Round end | round_{n}_completed_at | ISO 8601 |
| Session end | session_completed_at | ISO 8601 |

**Duration Calculation:**

```
session_duration = session_completed_at - session_started_at
active_duration = sum(round_{n}_completed_at - round_{n}_started_at for all n)
wait_duration = session_duration - active_duration  # Time waiting for user
```

**Recording in status.md:**

```markdown
## Session Timing

| Event | Timestamp |
|-------|-----------|
| Session Started | 2026-01-12T10:00:00Z |
| Round 1 Started | 2026-01-12T10:00:00Z |
| Round 1 Completed | 2026-01-12T10:45:00Z |
| Round 2 Started | 2026-01-12T11:00:00Z |
| Round 2 Completed | 2026-01-12T11:30:00Z |
| ... | ... |
| Session Completed | 2026-01-12T15:30:00Z |

### Duration Summary
- Total Duration: 5h 30m
- Active Duration: 3h 15m (processing)
- Wait Duration: 2h 15m (user decisions)
- Average Round: 39 minutes
```

---

#### Termination Decision Tree (Revised)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        TERMINATION DECISION TREE                           │
└─────────────────────────────────────────────────────────────────────────────┘

┌─ Check: Round count >= max_rounds?
│   └─ YES ─────────────────────────────────────────────────────────────┐
│                                                                        │
│   └─ NO                                                                │
│       │                                                                │
│       ▼                                                                │
│   ┌─ Check: All gaps in terminal state?                                │
│   │   │                                                                │
│   │   └─ YES                                                           │
│   │       │                                                            │
│   │       ▼                                                            │
│   │   ┌─ Check: Final Review Pass = READY?                             │
│   │   │   │                                                            │
│   │   │   └─ YES ──► COMPLETE (automatic)                              │
│   │   │   └─ NO ──► Continue round (address Final Review issues)       │
│   │   │                                                                │
│   │   └─ NO (gaps still open)                                          │
│   │       │                                                            │
│   │       ▼                                                            │
│   │   ┌─ Check: Convergence state = DIVERGENCE_WARNING?                │
│   │   │   │                                                            │
│   │   │   └─ YES ──► User options from GAP-FLOW-002                    │
│   │   │       │       (includes Force Complete path)                   │
│   │   │       │                                                        │
│   │   │   └─ NO (CONVERGING or FLAT)                                   │
│   │   │       │                                                        │
│   │   │       ▼                                                        │
│   │   │   ┌─ Check: User requested early termination?                  │
│   │   │   │   │                                                        │
│   │   │   │   └─ YES                                                   │
│   │   │   │       │                                                    │
│   │   │   │       ▼                                                    │
│   │   │   │   ┌─ Check: CRITICAL gaps remain?                          │
│   │   │   │   │   └─ YES ──► CRITICAL warning flow ──► Confirm? ──┐    │
│   │   │   │   │   └─ NO                                           │    │
│   │   │   │   │       │                                           │    │
│   │   │   │   │       ▼                                           │    │
│   │   │   │   │   ┌─ Check: HIGH gaps remain?                     │    │
│   │   │   │   │   │   └─ YES ──► HIGH warning flow ──► Confirm? ──┤    │
│   │   │   │   │   │   └─ NO                                       │    │
│   │   │   │   │   │       │                                       │    │
│   │   │   │   │   │       ▼                                       │    │
│   │   │   │   │   │   ┌─ Check: MEDIUM gaps remain?               │    │
│   │   │   │   │   │   │   └─ YES ──► GOOD_ENOUGH (acknowledge) ───┤    │
│   │   │   │   │   │   │   └─ NO ──► COMPLETE ──────────────────────┤    │
│   │   │   │   │   │   │                                           │    │
│   │   │   │   └─ NO (user did not request) ──► Continue round     │    │
│   │   │   │                                                       │    │
│   │   │                                                           │    │
│   │                                                               │    │
│                                                                   │    │
│       ┌───────────────────────────────────────────────────────────┘    │
│       │                                                                │
│       ▼                                                                │
│   ┌─ User confirms?                                                    │
│   │   └─ YES ──► USER_APPROVED (record rationale)                      │
│   │   └─ NO ──► Continue round (user changed mind)                     │
│   │                                                                    │
│                                                                        │
└───────────────────────────────────────────────────────────────────────────┐
                                                                            │
┌───────────────────────────────────────────────────────────────────────────┘
│ MAX_ROUNDS reached
│
▼
┌─ Present MAX_ROUNDS options
│   └─ Extend ──► Add rounds, continue
│   └─ Accept ──► USER_APPROVED flow (with severity warnings)
│   └─ Abandon ──► ABANDONED
```

---

#### Integration with GAP-FLOW-002 (Stall Detection)

Termination criteria integrate with the convergence state machine:

| Convergence State | Termination Availability |
|-------------------|--------------------------|
| CONVERGING | COMPLETE (if criteria met), User early termination |
| FLAT | Same as CONVERGING |
| STALLED | N/A (immediately transitions to DIVERGENCE_WARNING) |
| DIVERGENCE_WARNING | Force Complete, Abandon, Pause (via user options) |
| PAUSED | Session saved, termination deferred |
| COMPLETE | Terminal state reached |

**DIVERGENCE_WARNING -> COMPLETE Path:**

When user selects "Force Complete" from DIVERGENCE_WARNING:
1. System applies USER_APPROVED flow
2. Severity warnings shown per remaining gaps
3. User must acknowledge each severity tier
4. Rationale recorded for HIGH/CRITICAL gaps
5. Final spec generated with Known Limitations section

---

#### Final Summary Format (Revised)

```markdown
## Session Complete

**Status:** COMPLETE | GOOD_ENOUGH | USER_APPROVED | MAX_ROUNDS | STALL_EXIT | ABANDONED
**Rounds:** {n}
**Duration:** {total} ({active} active, {wait} waiting)

### Termination Details
- **Type:** {termination_type}
- **Trigger:** {what triggered termination}
- **Final Convergence State:** {CONVERGING | FLAT | etc.}
- **Final Weighted Net:** {last round's weighted_net}

### Gap Summary (Using GAP-FLOW-010 States)

| State | CRITICAL | HIGH | MEDIUM | LOW | Total |
|-------|----------|------|--------|-----|-------|
| RESOLVED | 0 | 8 | 12 | 5 | 25 |
| ACCEPTED | 0 | 0 | 2 | 1 | 3 |
| WONT_FIX | 0 | 0 | 1 | 2 | 3 |
| USER_DEFERRED | 0 | 0 | 1 | 0 | 1 |
| **Terminal Total** | 0 | 8 | 16 | 8 | 32 |

### Known Limitations (if USER_APPROVED or GOOD_ENOUGH)

#### Deferred Gaps
| Gap ID | Severity | Rationale |
|--------|----------|-----------|
| GAP-UX-003 | MEDIUM | "Example session completion deferred to v1.1" |

#### Accepted HIGH Gaps (if any)
| Gap ID | Severity | User Rationale | Acknowledged |
|--------|----------|----------------|--------------|
| GAP-FLOW-013 | HIGH | "Manual detection acceptable for v1" | 2026-01-12T14:30:00Z |

### Convergence History

| Round | Unweighted Net | Weighted Net | State |
|-------|----------------|--------------|-------|
| 1 | +1 | +6 | CONVERGING |
| 2 | 0 | -4 | FLAT |
| 3 | +2 | +8 | CONVERGING |
| ... | ... | ... | ... |
| {n} | {net} | {weighted} | {final_state} |

### Output
Final spec written to: `specs/{name}_v{version}.md`
Session archive: `specs/meta/sessions/{name}/`
```

---

### Examples

**Example 1: Automatic COMPLETE**
```
Round 8 Summary:
- All 25 initial gaps in RESOLVED state
- Zero USER_DEFERRED gaps
- Final Review Pass: READY_FOR_TERMINATION

Mediator: "All gaps resolved. Final Review Pass complete. Generating final spec."

Termination Type: COMPLETE (automatic)
User Action: None required
```

**Example 2: GOOD_ENOUGH with MEDIUM Gaps**
```
Round 10 Summary:
- CRITICAL: 0 remaining
- HIGH: 0 remaining
- MEDIUM: 2 remaining (GAP-UX-003, GAP-COMM-004)
- LOW: 1 remaining

User requests: "I'd like to wrap up"

AskUserQuestion:
─────────────────────────────────────────────────────────
You have requested early termination.

Current Gap Status:
- CRITICAL: 0 (good!)
- HIGH: 0 (good!)
- MEDIUM: 2 (GAP-UX-003, GAP-COMM-004)
- LOW: 1 (GAP-QA-004)

These gaps will be documented as "Known Limitations."

Options:
1. Accept as GOOD_ENOUGH - Document MEDIUM/LOW gaps as known limitations
2. Continue working - Address MEDIUM gaps first
3. Pause - Save for later
─────────────────────────────────────────────────────────

User selects: Option 1

Termination Type: GOOD_ENOUGH
User Action: Acknowledge MEDIUM gaps (rationale optional)
```

**Example 3: USER_APPROVED with HIGH Gaps**
```
Round 7 Summary:
- CRITICAL: 0 remaining
- HIGH: 1 remaining (GAP-FLOW-013: Implicit disagreement detection)
- MEDIUM: 3 remaining
- LOW: 2 remaining

User requests: "Time constraint, need to ship v1"

AskUserQuestion (WARNING tier):
─────────────────────────────────────────────────────────
WARNING: Accepting with HIGH-severity gaps.

HIGH = "Implementation will be incorrect/incomplete without this"

Remaining HIGH gaps (1):
- GAP-FLOW-013: Implicit disagreement detection
  Impact: Engineer may ignore feedback without detection

To accept, you must:
1. Type "I acknowledge"
2. Provide rationale for GAP-FLOW-013: ________________

[A] I acknowledge - Provide rationale and proceed
[B] Continue working - Address HIGH gap first (RECOMMENDED)
[C] Pause - Save session for later
─────────────────────────────────────────────────────────

User selects: A
User rationale: "Will implement manual detection heuristics. Automated v2 goal."

Termination Type: USER_APPROVED
User Action: Explicit acknowledgment with rationale
Record: decisions.md updated with acknowledgment timestamp
```

**Example 4: MAX_ROUNDS Reached**
```
Round 15 (max_rounds reached):
- Initial gaps: 25
- max_rounds: max(10, ceil(25 * 0.6)) = 15
- CRITICAL: 0
- HIGH: 2 remaining
- MEDIUM: 5 remaining

AskUserQuestion:
─────────────────────────────────────────────────────────
SESSION LIMIT REACHED

Completed 15 of 15 rounds.
Initial: 25 gaps | Current: 7 gaps | Net: -18 resolved

=== Gap Status ===
CRITICAL: 0
HIGH: 2 (GAP-FLOW-013, GAP-COMM-005)
MEDIUM: 5
LOW: 0

=== Options ===
1. Extend session - Add 5 more rounds (new limit: 20)
2. Accept current state - USER_APPROVED flow (HIGH warning applies)
3. Abandon - Discard session entirely

Note: Hard limit is 50 rounds.
─────────────────────────────────────────────────────────

User selects: Option 1 (Extend)

max_rounds updated: 20
Continue to Round 16
```

**Example 5: STALL_EXIT via Divergence Warning**
```
Round 6: DIVERGENCE_WARNING triggered (new CRITICAL gap introduced)

User presented with GAP-FLOW-002 options:
1. Narrow scope
2. Accept complexity
3. Prioritize CRITICAL
4. Pause
5. Force complete

User selects: 5 (Force complete)

Flows into USER_APPROVED:
- CRITICAL warning shown (spec may be unusable)
- User types: "I accept CRITICAL gaps knowing the spec may be unusable"
- Rationale required for CRITICAL gap

Termination Type: STALL_EXIT
Record: Triggered via DIVERGENCE_WARNING -> Force Complete path
```

---

### Trade-offs

**Pros:**
- Severity-based termination criteria are objective and computable
- Warning tiers prevent accidental premature closure with CRITICAL/HIGH gaps
- User always has control but system provides guardrails
- Duration tracking enables session analysis and improvement
- Formula-based round limit scales with spec complexity
- Explicit "user-acknowledged" definition prevents ambiguity
- Final Review Pass replaces vague "Reviewer APPROVE"
- Integration with GAP-FLOW-002 provides unified state model

**Cons:**
- Warning flows may feel verbose for experienced users
- Requiring rationale for HIGH gaps adds friction (intentional)
- Formula-based max_rounds (0.6 multiplier) is empirically derived, may need tuning
- Duration tracking requires consistent timestamp recording (bookkeeping)
- Five termination types may be more than necessary (but cover real scenarios)

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | Uses terminal states (RESOLVED, ACCEPTED, WONT_FIX, USER_DEFERRED) for completion checks |
| GAP-FLOW-010 (Lifecycle) | Open gap count uses exact lifecycle formula |
| GAP-FLOW-012 (Severity) | Termination requirements matrix by severity level |
| GAP-FLOW-012 (Severity) | Weighted threshold formula for automatic completion |
| GAP-FLOW-012 (Severity) | Warning tiers for CRITICAL, HIGH acceptance |
| GAP-FLOW-002 (Stall) | Integrates with convergence states; Force Complete path defined |
| GAP-FLOW-002 (Stall) | PAUSED state available as termination alternative |

---

### Response to Reviewer Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-023 | "Reviewer APPROVE" replaced with Final Review Pass protocol. Final Review confirms readiness, not approval. Clear checklist provided. |
| ISSUE-R1-024 | USER_APPROVED requires warning confirmation. CRITICAL: "I accept knowing spec may be unusable". HIGH: "I acknowledge" + rationale. Cannot bypass. |
| ISSUE-R1-025 | Now uses GAP-FLOW-012 severity definitions. Matrix specifies requirements per severity level. (RESOLVED in Round 2) |
| ISSUE-R1-026 | max_rounds formula: `max(10, ceil(initial_gaps * 0.6))`. Rationale: ~1.5-2 gaps resolved per round. Configurable with hard_limit=50. |
| ISSUE-R1-027 | Duration tracked via ISO 8601 timestamps. session_duration, active_duration, wait_duration all defined. Recorded in status.md. |

---

### New Gaps Introduced

None. This revision completes GAP-FLOW-005 using established foundations.

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-005 (Revised) | Proposed | HIGH | 0 |

**Changes from Round 1:**
- Replaced "Reviewer APPROVE" with Final Review Pass protocol
- Added severity-based warning tiers for USER_APPROVED
- Defined "user-acknowledged" operationally (confirmation + rationale)
- Formula-based max_rounds with rationale
- ISO 8601 duration tracking
- Integration with GAP-FLOW-002 convergence states

**Dependencies:**
- Requires GAP-FLOW-010 (RESOLVED Round 2)
- Requires GAP-FLOW-012 (RESOLVED Round 2)
- Integrates with GAP-FLOW-002 (APPROVED Round 3)
- No new dependencies introduced

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Termination Types Summary
```
COMPLETE     - All gaps resolved, automatic
GOOD_ENOUGH  - Zero CRITICAL/HIGH, user acknowledges MEDIUM/LOW
USER_APPROVED - User confirms with warnings, rationale required for HIGH/CRITICAL
MAX_ROUNDS   - Limit reached, user chooses extend/accept/abandon
STALL_EXIT   - Via divergence warning Force Complete
ABANDONED    - User cancels, no output
```

### Severity Requirements Quick Reference
```
COMPLETE:      0 CRITICAL, 0 HIGH, 0 MEDIUM open
GOOD_ENOUGH:   0 CRITICAL, 0 HIGH, MEDIUM acknowledged
USER_APPROVED: CRITICAL/HIGH warning-confirmed, rationale required
```

### Round Limit Formula
```
max_rounds = max(10, ceil(initial_gap_count * 0.6))
hard_limit = 50
```

### Duration Fields
```
session_started_at, session_completed_at
round_{n}_started_at, round_{n}_completed_at
session_duration = end - start
active_duration = sum(round durations)
wait_duration = session - active
```

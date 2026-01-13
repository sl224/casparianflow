# Engineer Proposals - Round 6

**Date:** 2026-01-12
**Focus:** Rollback Mechanism Revision (GAP-FLOW-007)
**Priority:** Tier 1 - Completing Flow Gaps

**Context:** GAP-FLOW-010 (Gap Lifecycle), GAP-FLOW-012 (Severity Definitions), GAP-FLOW-002 (Stall Detection), GAP-FLOW-005 (Termination Criteria), and GAP-FLOW-004 (Partial Round Handling) are now RESOLVED. This proposal revises GAP-FLOW-007 (Rollback Mechanism) to address all Reviewer issues from Round 1.

---

## Gap Resolution: GAP-FLOW-007 (Revised)

**Gap:** No rollback mechanism - How to undo a bad round?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-033 (Rollback restores status.md but NOT decisions.md) | Both status.md AND decisions.md are restored; rolled-back decisions archived with round |
| ISSUE-R1-034 (Backup files create inconsistency - only N-1 kept) | Keep last 3 rounds of backups; cleanup defined |
| ISSUE-R1-035 (Rollback limit rationale missing) | Replaced per-round limit with root cause analysis flow; session limit justified |
| ISSUE-R1-036 (Archive accumulates cruft) | Immediate compression; cleanup at session completion; retention policy defined |
| ISSUE-R1-037 (Rollback UX interrupts rhythm) | Added auto-retry option to reduce user prompts |
| ISSUE-R1-038 (GAP-FLOW-015 analysis deferred but valuable) | Basic rollback analysis defined inline |

---

### Revised Proposal

#### Rollback Scope

Rollback operates at the **round level** with complete state restoration:

| Artifact | On Rollback |
|----------|-------------|
| `round_N/` folder | Archived (not deleted) |
| `status.md` | Restored from `status_backup_round_{N-1}.md` |
| `decisions.md` | Restored from `decisions_backup_round_{N-1}.md` |
| Round counter | Decremented to N-1 |
| Convergence state | Recalculated from restored status.md |

**Key Change from Round 1:** Both `status.md` and `decisions.md` are restored. Decisions made during the rolled-back round are archived with the round folder, not preserved in the active session.

---

#### Decisions.md Restore Behavior (Addressing ISSUE-R1-033)

**Problem:** Round 1 proposal only restored status.md. Decisions from rolled-back rounds persisted, causing state inconsistency.

**Solution:** Full state restoration including decisions.

**Restore Protocol:**

```
Step 1: Identify decisions made during round N
─────────────────────────────────────────────────────────
Parse decisions.md for entries with:
- Round number == N
- Timestamp between round_N_started_at and rollback_timestamp

These are "rolled-back decisions"

Step 2: Archive rolled-back decisions
─────────────────────────────────────────────────────────
Copy to: round_N_rolled_back/decisions_from_round_N.md

Content:
"""
# Decisions Made During Round N (Rolled Back)

**Note:** These decisions were made during Round N, which was subsequently rolled back.
They are preserved for reference but are NOT active in the session.

[Original decision entries from round N]
"""

Step 3: Restore decisions.md
─────────────────────────────────────────────────────────
copy(decisions_backup_round_{N-1}.md, decisions.md)

Append rollback notice:
"""
## Rollback Notice - Round {N}

Round {N} was rolled back at {timestamp}.
Decisions from round {N} have been archived to:
  round_{N:03d}_rolled_back/decisions_from_round_{N}.md

Reason: {rollback_reason}
"""
```

**Decision Recovery:**

If user wants to re-apply a rolled-back decision:

```
User: "I still want to keep the decision about severity weights from round 5"

Mediator: Retrieving from round_005_rolled_back/decisions_from_round_5.md

Found decision:
- ID: DECISION-R5-001
- Choice: "Severity weights remain as defined in GAP-FLOW-012"
- Rationale: "Weights are working well in practice"

Options:
1. Re-apply this decision to current decisions.md
2. Modify and apply (edit rationale/choice)
3. Ignore (decision remains archived only)
```

---

#### Backup Retention Policy (Addressing ISSUE-R1-034)

**Problem:** Round 1 kept only N-1 backup, making multi-round rollback impossible.

**Solution:** Keep last 3 rounds of backups.

**Backup Rotation:**

```
At start of Round N, Mediator maintains:

Required backups:
- status_backup_round_{N-1}.md   (most recent)
- status_backup_round_{N-2}.md   (one round back)
- status_backup_round_{N-3}.md   (two rounds back)
- decisions_backup_round_{N-1}.md
- decisions_backup_round_{N-2}.md
- decisions_backup_round_{N-3}.md

Before creating new backup:
- Create status_backup_round_{N}.md
- Delete status_backup_round_{N-4}.md (if exists)
- Same for decisions backups

Backup creation order:
1. Delete oldest backup (N-4)
2. Create new backup (N)
3. Proceed with round
```

**Multi-Round Rollback:**

With 3 backups retained, user can rollback up to 3 rounds in sequence:

```
Current: Round 8
Available rollback targets:
- Round 7 (rollback 1 round) - uses backup_round_7
- Round 6 (rollback 2 rounds) - uses backup_round_6
- Round 5 (rollback 3 rounds) - uses backup_round_5

Rollback to Round 4 or earlier: NOT AVAILABLE
  (backups have been rotated out)
```

**AskUserQuestion for multi-round rollback:**

```
ROLLBACK OPTIONS

Available rollback targets:
| Target | Gaps at Target | Decisions Since |
|--------|----------------|-----------------|
| Round 7 | 15 open | 2 decisions |
| Round 6 | 17 open | 5 decisions |
| Round 5 | 19 open | 8 decisions |

Note: Earlier rounds cannot be rolled back (backups rotated).

Options:
1. Rollback to Round 7 (undo 1 round)
2. Rollback to Round 6 (undo 2 rounds)
3. Rollback to Round 5 (undo 3 rounds)
4. Cancel rollback (continue from Round 8)
```

---

#### Rollback Limits and Root Cause Analysis (Addressing ISSUE-R1-035)

**Problem:** Round 1 specified "max 2 rollbacks per round, max 5 per session" without justification.

**Solution:** Replace per-round limit with root cause analysis; justify session limit.

**Per-Round Handling (Replacing Arbitrary Limit):**

Instead of hard limit after N rollbacks of same round, Mediator performs root cause analysis:

```
After 2nd rollback of same round:
─────────────────────────────────────────────────────────
REPEATED ROLLBACK DETECTED

Round 5 has been rolled back twice. This suggests a systematic issue.

=== Rollback History ===
Attempt 1: "Proposals too abstract" (user feedback)
Attempt 2: "Still no concrete examples" (user feedback)

=== Possible Root Causes ===
1. Gap is ambiguous - Multiple interpretations possible
   Evidence: Both attempts produced different interpretations

2. Missing context - External information required
   Evidence: Examples reference undefined concepts

3. Scope too large - Gap combines multiple concerns
   Evidence: Attempt 1 addressed X, Attempt 2 addressed Y

=== Recommended Actions ===
Based on analysis, recommended action: Split gap

Current gap: GAP-FLOW-007 "Rollback mechanism"
Suggested split:
  - GAP-FLOW-007a: "Rollback state restoration"
  - GAP-FLOW-007b: "Rollback UX and user prompts"
  - GAP-FLOW-007c: "Rollback limits and policies"

=== Options ===
1. Split gap as suggested (RECOMMENDED)
2. Provide additional context for retry
3. Reassign to different gaps this round
4. Force proceed with best available output
5. Pause session for external consultation

Choose an option: ___
─────────────────────────────────────────────────────────
```

**Root Cause Detection Heuristics:**

| Pattern | Detected When | Likely Cause |
|---------|---------------|--------------|
| Same failure mode | Rollback reasons similar | Prompt issue or gap ambiguity |
| Different failure modes | Rollback reasons divergent | Scope too large |
| Output references unknowns | Content mentions undefined terms | Missing context |
| Output is minimal | <200 chars both attempts | Domain gap |
| Output contradicts itself | Internal inconsistencies | Gap ambiguity |

**Session Limit (Justified):**

```
Session rollback limit: 7

Rationale:
- Average session: 10-15 rounds
- Reasonable rollback rate: 10-20% of rounds
- 7 rollbacks = ~50% of a 15-round session
- Beyond this suggests fundamental session issues, not round-level problems

When limit reached:
─────────────────────────────────────────────────────────
SESSION ROLLBACK LIMIT REACHED

This session has used all 7 rollback attempts.
This suggests systematic issues beyond individual round problems.

Possible causes:
1. Source spec fundamentally ambiguous
2. Session scope too broad
3. Prompt engineering issues
4. Domain expertise mismatch

Options:
1. Pause session for human review
2. Accept current state as GOOD_ENOUGH
3. Abandon session
4. Request Mediator prompt refresh (experimental)

Note: No additional rollbacks available in this session.
─────────────────────────────────────────────────────────
```

**Configurable Limit:**

```markdown
## Session Configuration

| Parameter | Default | Rationale | Override |
|-----------|---------|-----------|----------|
| max_rollbacks_session | 7 | ~50% of typical session | User may increase to 10 for complex specs |
| rollback_analysis_threshold | 2 | After 2 rollbacks of same round, analyze | May set to 1 for strict mode |
| backup_retention_rounds | 3 | Balance storage vs flexibility | May increase to 5 |
```

---

#### Archive Cleanup Strategy (Addressing ISSUE-R1-036)

**Problem:** Rolled-back folders accumulate without cleanup policy.

**Solution:** Immediate compression + session completion cleanup + retention policy.

**Immediate Compression:**

```
On rollback of Round N:
─────────────────────────────────────────────────────────
1. Rename: round_N/ -> round_N_rolled_back/

2. Copy rolled-back decisions to archive:
   round_N_rolled_back/decisions_from_round_N.md

3. Add rollback metadata:
   round_N_rolled_back/rollback_metadata.json:
   {
     "original_round": N,
     "rollback_timestamp": "2026-01-12T14:30:00Z",
     "reason": "User request - proposals too abstract",
     "attempt_number": 1,
     "user_adjustments": ["Focus on concrete examples"]
   }

4. Compress immediately:
   tar -czf round_N_rolled_back.tar.gz round_N_rolled_back/
   rm -rf round_N_rolled_back/

Result: Single .tar.gz file instead of directory
─────────────────────────────────────────────────────────
```

**Naming for Multiple Rollbacks:**

```
First rollback of Round 5:  round_005_rolled_back_1.tar.gz
Second rollback of Round 5: round_005_rolled_back_2.tar.gz
Third rollback of Round 5:  round_005_rolled_back_3.tar.gz
```

**Session Completion Cleanup:**

```
On session termination (any type):
─────────────────────────────────────────────────────────
Archive handling options presented to user:

"Session complete. Rolled-back round archives found:
- round_003_rolled_back_1.tar.gz (2.1 KB)
- round_005_rolled_back_1.tar.gz (1.8 KB)
- round_005_rolled_back_2.tar.gz (2.0 KB)
Total: 5.9 KB

Options:
1. Keep all archives (for debugging/analysis)
2. Delete all archives (clean finish)
3. Keep recent only (last 7 days of this session)
4. Move to cold storage (~/.casparian_flow/archive/)"

Default if no response in automated mode: Option 4 (cold storage)
─────────────────────────────────────────────────────────
```

**Retention Policy:**

| Location | Retention | Cleanup Trigger |
|----------|-----------|-----------------|
| Session folder (active) | Until session completion | Session termination |
| Cold storage | 30 days | Automated cleanup job |
| Explicit keep | Indefinite | Manual deletion only |

**Cold Storage Structure:**

```
~/.casparian_flow/archive/
├── spec_refinement_workflow/
│   ├── 2026-01-12_session_abc123/
│   │   ├── round_003_rolled_back_1.tar.gz
│   │   ├── round_005_rolled_back_1.tar.gz
│   │   └── round_005_rolled_back_2.tar.gz
│   └── manifest.json  # Lists all archives with metadata
└── cleanup_log.txt    # Records automated deletions
```

---

#### Auto-Retry Option (Addressing ISSUE-R1-037)

**Problem:** Rollback UX interrupts rhythm with 4 required options every time.

**Solution:** Add auto-retry option that requires no user input for first retry.

**Auto-Retry Mode:**

```
Rollback behavior modes:

INTERACTIVE (default):
- Every rollback prompts user for adjustments
- Maximum control, more interruptions

AUTO_RETRY:
- First rollback of a round: Auto-retry with Mediator-selected adjustments
- Second rollback of same round: Prompt user (root cause analysis)
- Reduces interruptions by 50% for common cases

STRICT:
- Every rollback prompts user
- Analysis after first rollback (not second)
- For critical/sensitive specs
```

**Auto-Retry Adjustments (Mediator-Selected):**

When auto-retry is enabled and first rollback of a round occurs:

```
Mediator auto-adjustments based on failure pattern:
─────────────────────────────────────────────────────────
Failure: Output too abstract
Auto-adjustment: Add to prompt - "Include at least 3 concrete examples with
                 specific values. Abstract descriptions are insufficient."

Failure: Missing trade-offs
Auto-adjustment: Add to prompt - "You MUST include a ### Trade-offs section
                 with at least 2 pros and 2 cons."

Failure: Gap not addressed
Auto-adjustment: Add to prompt - "Your primary task is to address GAP-XXX.
                 Begin your response with '## Gap Resolution: GAP-XXX'"

Failure: Inconsistent with prior gaps
Auto-adjustment: Attach summary of related resolved gaps to prompt.
─────────────────────────────────────────────────────────
```

**Auto-Retry Protocol:**

```
Step 1: Detect rollback trigger
─────────────────────────────────────────────────────────
Trigger: User says "That round wasn't helpful"
         OR Mediator detects validation failure per GAP-FLOW-001
         OR Divergence warning triggers user selection

Step 2: Check rollback mode and history
─────────────────────────────────────────────────────────
IF mode == AUTO_RETRY AND attempt_count(this_round) == 0:
    # First rollback of this round
    auto_adjust = determine_adjustments(failure_pattern)
    log: "Auto-retrying Round {N} with adjustments: {auto_adjust}"
    execute_rollback(round_N, adjustments=auto_adjust, prompt_user=False)

ELSE:
    # Not auto-retry mode, or second+ attempt
    prompt_user_for_options()

Step 3: Record auto-retry
─────────────────────────────────────────────────────────
Append to status.md:
"""
## Round {N} Auto-Retry

**Attempt:** 1 of 2 before user prompt
**Failure Pattern:** {detected_pattern}
**Auto-Adjustments Applied:**
- {adjustment_1}
- {adjustment_2}

**Note:** User not prompted. Second failure will trigger interactive mode.
"""
```

**User Override:**

```
At any point, user can say:
"Don't auto-retry, ask me"

Mediator: "Auto-retry disabled for this round. What adjustments would you like?"

OR

"Enable auto-retry mode"

Mediator: "Auto-retry enabled. First rollback of each round will auto-adjust."
```

---

#### Rollback Analysis (Addressing ISSUE-R1-038)

**Problem:** GAP-FLOW-015 (analyze rolled-back rounds) was deferred but is valuable for preventing repeated failures.

**Solution:** Define basic analysis inline with rollback operation.

**Rollback Analysis Report:**

Generated automatically after each rollback:

```
## Rollback Analysis - Round {N}

### Failure Summary
- **Rollback Reason:** {user_stated_reason OR validation_failure}
- **Attempt Number:** {n} of this round
- **Timestamp:** {ISO 8601}

### Content Analysis

#### What Was Produced
| Gap | Addressed | Sections Present | Example Count |
|-----|-----------|------------------|---------------|
| GAP-FLOW-007 | Partial | Solution, Examples | 1 |
| GAP-FLOW-008 | No | None | 0 |

#### Structural Completeness
| Check | Status |
|-------|--------|
| Has ## Gap Resolution headers | PASS |
| Has ### Trade-offs | FAIL |
| Has ### Examples | PASS |
| Min length per gap (200 chars) | PASS |
| Valid cross-references | PASS |

### Pattern Detection

#### Detected Patterns
- **ABSTRACT_OUTPUT:** Solution uses vague language ("appropriate", "as needed")
- **MISSING_SECTIONS:** Trade-offs section absent
- **NARROW_EXAMPLES:** Only 1 example, no edge cases

#### Historical Comparison
| Pattern | This Attempt | Previous Attempts |
|---------|--------------|-------------------|
| ABSTRACT_OUTPUT | Yes | N/A (first attempt) |
| MISSING_SECTIONS | Yes | N/A |

### Recommended Adjustments

Based on detected patterns:

1. **For ABSTRACT_OUTPUT:**
   Add to prompt: "Replace vague terms with specific values. Instead of
   'appropriate threshold', say 'threshold of 5 attempts'."

2. **For MISSING_SECTIONS:**
   Add to prompt: "Your response MUST include a ### Trade-offs section."

3. **For NARROW_EXAMPLES:**
   Add to prompt: "Include at least 2 examples: one success case, one edge case."

### Adjustment Confidence
- Adjustment 1: HIGH (pattern clearly detected)
- Adjustment 2: HIGH (structural, easily enforced)
- Adjustment 3: MEDIUM (may require domain knowledge)
```

**Analysis-Driven Retry:**

```
def analyze_rolled_back_round(round_n, reason):
    # 1. Parse the archived round content
    content = decompress_and_read(f"round_{round_n}_rolled_back_{attempt}.tar.gz")

    # 2. Run structural analysis
    structural_issues = check_structure(content)

    # 3. Run pattern detection
    patterns = detect_patterns(content)

    # 4. Compare with historical failures
    history = get_rollback_history(round_n)
    recurring = find_recurring_patterns(patterns, history)

    # 5. Generate adjustment recommendations
    adjustments = recommend_adjustments(patterns, recurring)

    # 6. Produce analysis report
    report = format_analysis_report(
        round_n, reason, structural_issues,
        patterns, recurring, adjustments
    )

    # 7. Store in session metadata
    store_analysis(round_n, attempt, report)

    return report, adjustments
```

**Pattern Definitions:**

| Pattern | Detection Rule | Adjustment |
|---------|---------------|------------|
| ABSTRACT_OUTPUT | >5 instances of vague words per gap | Explicit value requirements |
| MISSING_SECTIONS | Required section headers absent | Structural requirements in prompt |
| NARROW_EXAMPLES | <2 examples per gap | Minimum example requirements |
| INCONSISTENT_REFS | Cross-refs to non-existent gaps | Provide gap list in prompt |
| DOMAIN_CONFUSION | Terms undefined or misused | Attach glossary to prompt |
| SCOPE_DRIFT | Addresses different gap than assigned | Explicit gap assignment |

**Vague Word List:**

```
VAGUE_WORDS = [
    "appropriate", "as needed", "reasonable", "sufficient",
    "adequate", "properly", "correctly", "when necessary",
    "if applicable", "as required", "typically", "generally",
    "various", "certain", "some", "may", "might", "could",
    "should consider", "it depends", "context-dependent"
]
```

---

#### Complete Rollback Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ROLLBACK FLOW (REVISED)                           │
└─────────────────────────────────────────────────────────────────────────────┘

┌─ Rollback Triggered
│   (User request OR Validation failure OR Divergence selection)
│
├─ Check: Session rollback limit reached?
│   └─ YES ──► Present limit-reached options ──► Exit or accept current
│   └─ NO ──► Continue
│
├─ Check: Rollback mode?
│   └─ AUTO_RETRY AND first attempt of this round
│       │
│       ├─ Analyze failure pattern
│       ├─ Apply auto-adjustments
│       ├─ Execute rollback (no user prompt)
│       ├─ Retry round with adjustments
│       └─ [If retry fails] ──► Loop back to trigger ──► Second attempt
│
│   └─ INTERACTIVE OR second+ attempt
│       │
│       ├─ Check: Second+ rollback of same round?
│       │   └─ YES ──► Root cause analysis
│       │       ├─ Present analysis to user
│       │       ├─ Recommend: split gap / add context / reassign
│       │       └─ User selects action
│       │   └─ NO ──► Standard rollback prompt
│       │
│       └─ Present rollback options:
│           1. Retry with same prompt
│           2. Retry with narrowed scope
│           3. Retry with user-provided context
│           4. Rollback multiple rounds (if available)
│           5. Skip to next round
│
├─ Execute Rollback
│   │
│   ├─ Archive round folder with metadata
│   ├─ Compress to .tar.gz immediately
│   ├─ Restore status.md from backup
│   ├─ Restore decisions.md from backup
│   ├─ Archive rolled-back decisions separately
│   ├─ Update status.md with rollback notice
│   ├─ Generate rollback analysis report
│   └─ Decrement round counter
│
├─ Apply Adjustments (if any)
│   │
│   ├─ Auto-adjustments (from analysis) OR
│   ├─ User-provided adjustments OR
│   └─ Mediator-recommended adjustments
│
└─ Retry Round with Adjusted Prompt
```

---

#### Recording in status.md

**Rollback Section Format:**

```markdown
## Rollback History

### Round 5 - Rollback 1
- **Timestamp:** 2026-01-12T14:30:00Z
- **Mode:** AUTO_RETRY
- **Reason:** Proposals too abstract (detected pattern: ABSTRACT_OUTPUT)
- **Adjustments Applied:**
  - Added explicit value requirements to prompt
  - Required minimum 3 examples
- **Archive:** round_005_rolled_back_1.tar.gz
- **Outcome:** Retry successful

### Round 5 - Rollback 2
- **Timestamp:** 2026-01-12T15:45:00Z
- **Mode:** INTERACTIVE (second attempt)
- **Reason:** Still missing concrete trade-offs
- **Root Cause Analysis:** Performed
  - Detected: Gap scope too large
  - Recommendation: Split gap
- **User Decision:** Split gap into 007a, 007b, 007c
- **Archive:** round_005_rolled_back_2.tar.gz
- **Outcome:** Proceeded with split gaps

### Session Rollback Summary
| Metric | Value |
|--------|-------|
| Total Rollbacks | 3 |
| Successful Retries | 2 |
| Root Cause Analyses | 1 |
| Gaps Split | 1 (GAP-FLOW-007 -> 3 sub-gaps) |
| Remaining Rollbacks | 4 of 7 |
```

---

### Examples

**Example 1: Auto-Retry Success**

```
Round 4:
- Engineer produces output for GAP-FLOW-005
- Output flagged: Missing trade-offs section

Mediator (AUTO_RETRY mode):
- Detects pattern: MISSING_SECTIONS
- Auto-adjustment: "You MUST include ### Trade-offs"
- Archives round_004, restores backups
- Retries without prompting user

Round 4 (retry):
- Engineer produces output with trade-offs section
- Validation passes
- Proceeds to Reviewer

status.md entry:
"""
## Round 4 Auto-Retry
**Attempt:** 1 of 2 before user prompt
**Failure Pattern:** MISSING_SECTIONS (Trade-offs absent)
**Auto-Adjustments Applied:**
- Added structural requirement for ### Trade-offs section
**Outcome:** Successful on retry
"""
```

**Example 2: Multi-Round Rollback**

```
User at end of Round 8: "The last 3 rounds went in a bad direction.
                         Can we go back to Round 5?"

Mediator:
─────────────────────────────────────────────────────────
MULTI-ROUND ROLLBACK

Available rollback targets:
| Target | Gaps at Target | Decisions Since | Rounds to Undo |
|--------|----------------|-----------------|----------------|
| Round 7 | 12 open | 1 decision | 1 |
| Round 6 | 14 open | 3 decisions | 2 |
| Round 5 | 16 open | 5 decisions | 3 |

Rollback to Round 5 will:
- Discard rounds 6, 7, 8
- Restore 16 open gaps (currently 10)
- Archive 5 decisions (can be recovered)
- Create 3 archive files

Options:
1. Rollback to Round 5 (3 rounds back)
2. Choose different target
3. Cancel rollback
─────────────────────────────────────────────────────────

User selects: 1

Mediator executes:
- Archives round_008 -> round_008_rolled_back_1.tar.gz
- Archives round_007 -> round_007_rolled_back_1.tar.gz
- Archives round_006 -> round_006_rolled_back_1.tar.gz
- Restores status_backup_round_5.md -> status.md
- Restores decisions_backup_round_5.md -> decisions.md
- Archives decisions from rounds 6-8 separately
- Updates status.md with multi-rollback notice
```

**Example 3: Root Cause Analysis Triggers Gap Split**

```
Round 7 - Second Rollback:
─────────────────────────────────────────────────────────
REPEATED ROLLBACK DETECTED

Round 7 has been rolled back twice.

=== Rollback History ===
Attempt 1: "Rollback mechanics unclear" (user)
Attempt 2: "Now UX is unclear instead" (user)

=== Analysis ===
Pattern: Different sections failed in each attempt
- Attempt 1: Addressed UX, failed on mechanics
- Attempt 2: Addressed mechanics, failed on UX

Root cause: Gap scope too large (HIGH confidence)

=== Recommendation ===
Split GAP-FLOW-007 "Rollback mechanism" into:
- GAP-FLOW-007a: "Rollback state restoration" (what gets rolled back)
- GAP-FLOW-007b: "Rollback user experience" (how user interacts)
- GAP-FLOW-007c: "Rollback policies and limits" (constraints)

=== Options ===
1. Split gap as suggested (RECOMMENDED)
2. Provide additional context for retry
3. Force proceed with current output
4. Pause session

Choose: ___
─────────────────────────────────────────────────────────

User selects: 1 (Split)

Mediator:
- Creates GAP-FLOW-007a, 007b, 007c in status.md
- Links them as sub-gaps of original 007
- Marks original 007 as SUPERSEDED
- Round 7 retry addresses 007a only
```

**Example 4: Session Limit Reached**

```
After 7 rollbacks in session:
─────────────────────────────────────────────────────────
SESSION ROLLBACK LIMIT REACHED

Used: 7 of 7 rollback attempts

Rollback history:
- Round 3: 2 rollbacks (gap split resolved)
- Round 5: 1 rollback (auto-retry successful)
- Round 6: 2 rollbacks (context added)
- Round 7: 2 rollbacks (still failing)

This pattern suggests systematic issues.

Options:
1. Pause for human review - Export session state
2. Accept current state - USER_APPROVED flow
3. Abandon session
4. Prompt refresh (experimental) - Reset Mediator context

Note: Additional rollbacks are not available.
─────────────────────────────────────────────────────────
```

**Example 5: Decision Recovery After Rollback**

```
After rolling back Round 6:

User: "Wait, I still want to keep that decision about error weights"

Mediator:
─────────────────────────────────────────────────────────
DECISION RECOVERY

Round 6 decisions were archived. Found 2 decisions:

1. DECISION-R6-001: Error threshold weights
   Choice: "CRITICAL=16, HIGH=4, MEDIUM=2, LOW=1"
   Rationale: "Matches GAP-FLOW-012 severity definitions"

2. DECISION-R6-002: Stall detection threshold
   Choice: "2 consecutive FLAT rounds"
   Rationale: "Balances sensitivity and false positives"

Options:
1. Re-apply DECISION-R6-001 only
2. Re-apply DECISION-R6-002 only
3. Re-apply both decisions
4. Modify before re-applying
5. Leave archived (don't re-apply)
─────────────────────────────────────────────────────────

User selects: 1

Mediator:
- Appends DECISION-R6-001 to current decisions.md
- Adds note: "Re-applied from rolled-back Round 6"
```

---

### Trade-offs

**Pros:**
- Full state restoration (status.md AND decisions.md) prevents inconsistency
- 3-round backup retention enables multi-round rollback
- Root cause analysis prevents repeated failures
- Auto-retry reduces interruptions by ~50%
- Immediate compression minimizes storage usage
- Configurable limits allow tuning per spec complexity
- Rollback analysis provides actionable insights
- Decision recovery allows selective re-application

**Cons:**
- More complex than Round 1 proposal (necessary complexity)
- 3 backup files per round adds storage (~3x Round 1)
- Root cause analysis requires pattern detection heuristics (may have false positives)
- Auto-retry may miss nuances that user would catch
- Session limit of 7 may be too low for very complex specs (configurable)
- Vague word detection is heuristic, not definitive

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | Gap state restored on rollback; SUPERSEDED state for split gaps |
| GAP-FLOW-012 (Severity) | Analysis recommends severity-appropriate actions |
| GAP-FLOW-002 (Stall) | Rollback can exit DIVERGENCE_WARNING; convergence recalculated post-rollback |
| GAP-FLOW-005 (Termination) | Session limit reached offers termination paths |
| GAP-FLOW-004 (Partial Round) | Rollback may result in partial re-execution |
| GAP-FLOW-001 (Error Recovery) | Validation failures can trigger rollback |

---

### Response to Reviewer Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-033 | Full restore protocol: Both status.md and decisions.md restored. Rolled-back decisions archived separately. Recovery protocol allows selective re-application. |
| ISSUE-R1-034 | 3-round backup retention. Multi-round rollback supported. Backup rotation defined. |
| ISSUE-R1-035 | Per-round limit replaced with root cause analysis after 2 failures. Session limit justified (7 = ~50% of typical session). Limits configurable. |
| ISSUE-R1-036 | Immediate compression to .tar.gz. Session completion cleanup options. Cold storage with 30-day retention. Automated cleanup job. |
| ISSUE-R1-037 | AUTO_RETRY mode: First rollback auto-adjusts without prompting. Second rollback triggers user interaction. Reduces interruptions by ~50%. |
| ISSUE-R1-038 | Rollback analysis defined: pattern detection, historical comparison, adjustment recommendations. Report generated automatically. Patterns and vague word list defined. |

---

### New Gaps Introduced

- **GAP-FLOW-017**: Pattern detection accuracy may need calibration based on real usage

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-007 (Revised) | Proposed | HIGH | 1 (GAP-FLOW-017) |

**Changes from Round 1:**
- Full state restore (status.md + decisions.md) with decision recovery
- 3-round backup retention for multi-round rollback
- Root cause analysis replaces arbitrary per-round limit
- Session limit justified and made configurable
- Immediate compression + cold storage + cleanup policy
- Auto-retry mode reduces user interruptions
- Rollback analysis with pattern detection and recommendations

**Dependencies:**
- Uses GAP-FLOW-010 lifecycle states
- Uses GAP-FLOW-012 severity for recommendations
- Integrates with GAP-FLOW-002 convergence states
- Integrates with GAP-FLOW-005 termination paths
- Uses GAP-FLOW-001 validation for failure detection

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Restore Protocol Summary
```
On rollback of Round N:
1. Archive round_N/ -> round_N_rolled_back_{attempt}.tar.gz
2. Restore status.md from backup_{N-1}
3. Restore decisions.md from backup_{N-1}
4. Archive decisions from round N separately
5. Append rollback notice to status.md
6. Generate analysis report
```

### Backup Retention
```
At Round N, maintain:
- status_backup_round_{N-1}.md
- status_backup_round_{N-2}.md
- status_backup_round_{N-3}.md
- (same for decisions)

Rotate: Delete N-4 before creating N
```

### Rollback Modes
```
AUTO_RETRY:   First attempt auto-adjusts, second prompts user
INTERACTIVE:  Always prompt user
STRICT:       Prompt user, analyze after first failure
```

### Session Limits
```
max_rollbacks_session = 7 (default)
rollback_analysis_threshold = 2 (failures before analysis)
backup_retention_rounds = 3
```

### Pattern Detection
```
ABSTRACT_OUTPUT:    Vague words detected (>5 per gap)
MISSING_SECTIONS:   Required headers absent
NARROW_EXAMPLES:    <2 examples per gap
INCONSISTENT_REFS:  Invalid cross-references
DOMAIN_CONFUSION:   Undefined terms used
SCOPE_DRIFT:        Wrong gap addressed
```

### Archive Cleanup
```
Active session:  Keep archives
Session end:     User chooses: keep / delete / cold storage
Cold storage:    30-day retention, auto-cleanup
```

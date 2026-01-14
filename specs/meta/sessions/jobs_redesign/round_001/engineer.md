# Engineer Response - Round 001

**Date:** 2026-01-13
**Gaps Addressed:** GAP-CORE-001, GAP-MENTAL-001
**Engineer:** Claude (Opus 4.5)

---

## Gap Resolution: GAP-CORE-001

**Confidence:** HIGH

### Analysis

The spec presents 5 UI alternatives without first establishing what success looks like. This is backwards - we're choosing layouts before understanding the job to be done.

Let me work through this systematically by examining what users in each vertical actually DO when they open a "Jobs" view:

**Finance (FIX logs -> Bloomberg TCA):**
- Compliance deadline is 4:30 PM
- User's question: "Is the TCA file ready for upload? Where is it?"
- Secondary: "Why did 12 files fail? Can I still make the deadline?"

**Legal (PST -> Concordance):**
- Production deadline is Friday
- User's question: "How much is done? What's the current Bates range?"
- Secondary: "Are there errors that will delay production?"

**Healthcare (HL7 -> FHIR):**
- Data needs to be in analytics by morning rounds
- User's question: "Did the overnight import complete?"
- Secondary: "Which messages failed parsing?"

**Defense (CoT -> KML):**
- Brief is at 0600
- User's question: "Is the track data exported and ready?"
- Secondary: "Did all sensor feeds get processed?"

The pattern is clear: **Users open Jobs to answer "Is my output ready, and where is it?"**

They are NOT asking:
- "What's the status of Job #47?"
- "How does my pipeline look?"
- "What percentage is complete?"

These are implementation details. The core need is OUTPUT LOCATION + READINESS.

### Proposed Solution

**The ONE thing users need from Jobs view: Find their output and know if it's ready.**

More precisely, the Jobs view should answer these questions in order of priority:

1. **"Where is my output?"** - File path, one click to open
2. **"Is it ready?"** - Yes/No/Partial, with clear status
3. **"If not ready, when?"** - ETA for running jobs
4. **"If something broke, what?"** - Actionable error summary
5. **"Can I fix it?"** - Retry/Skip actions available inline

This means the view is fundamentally OUTPUT-CENTRIC, not JOB-CENTRIC.

The "job" is the mechanism. The "output" is the goal.

### Evidence

1. **User feedback in Appendix A.2**: "I just want to know where my Concordance load file is. I don't care about 'Job #42'."

2. **Vertical analysis**: All four verticals have time-sensitive deliverables (compliance deadlines, production deadlines, briefings). The common thread is DEADLINE + OUTPUT, not JOBS.

3. **Competitive analysis**: Vercel succeeds with "Clean, output-focused" despite being "too minimal for debugging." Users prefer output-focus even at the cost of debugging detail.

4. **Single-source insight**: Section 2.4 notes users typically have ONE source. This means there's usually ONE output they care about. The view should spotlight that ONE thing.

### New Gaps Introduced

- **GAP-OUTPUT-001**: If output-centric, how do we show output that spans multiple jobs? (e.g., Parquet folder built by 10 parse jobs)
- **GAP-OUTPUT-002**: What if user has no completed output yet? Empty state that guides them forward.

---

## Gap Resolution: GAP-MENTAL-001

**Confidence:** MEDIUM

### Analysis

The spec assumes "pipeline thinking" (Scan -> Parse -> Export) is how users conceptualize their work. But let me examine this assumption per vertical:

**Finance professionals' mental model:**
- Think in terms of: DATA STATES
- "Raw logs" -> "Normalized trades" -> "TCA report"
- The transformation is invisible; they care about data AT EACH STATE
- Mental model: **Data lifecycle** (raw -> clean -> output)

**Legal professionals' mental model:**
- Think in terms of: PRODUCTION WORKFLOW
- "Collected" -> "Processed" -> "Produced"
- Bates numbering is the heartbeat - they track progress by Bates range
- Mental model: **Document production funnel** (collected -> produced)

**Healthcare professionals' mental model:**
- Think in terms of: MESSAGE FLOW
- HL7 is event-driven; each message is a unit
- They care about: "Did message X make it through?"
- Mental model: **Message processing** (received -> parsed -> stored)

**Defense professionals' mental model:**
- Think in terms of: MISSION READINESS
- "Is the data ready for the brief?"
- Track data has temporal context (when was this position recorded?)
- Mental model: **Mission timeline** (data collected -> fused -> ready)

**Common thread:** None of these users naturally think "Scan job -> Parse job -> Export job."

They think in terms of:
1. **DATA STATES**: What state is my data in? (raw/processed/exported)
2. **COMPLETION**: Is the final output ready?
3. **PROBLEMS**: What's blocking completion?

### Proposed Solution

**Users think in DATA STATES, not JOBS.**

Reframe the entire view from "Jobs" to "Data Status" or keep "Jobs" as the technical name but design around data states.

The mental model we should design for:

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   SOURCE    │  ->  │  PROCESSED  │  ->  │   OUTPUT    │
│  (Files)    │      │  (Parquet)  │      │  (Export)   │
└─────────────┘      └─────────────┘      └─────────────┘
   1,247 files        1,235 parsed          2 exports
                      (12 failed)           (1 ready)
```

This is NOT a job pipeline. It's a DATA STATE progression.

The difference:
- Job pipeline: "Scan job -> Parse job -> Export job" (3 tasks)
- Data state: "Files -> Processed Data -> Export" (3 states of the same data)

Jobs are the IMPLEMENTATION of state transitions. Users see states, not jobs.

**Vertical adaptations:**

| Vertical | Source State | Processed State | Output State |
|----------|--------------|-----------------|--------------|
| Finance | FIX logs | Order lifecycle | TCA file |
| Legal | PST files | Parsed emails | Concordance load file |
| Healthcare | HL7 messages | FHIR resources | Analytics-ready |
| Defense | CoT tracks | Fused positions | KML brief |

### Evidence

1. **Alternative B (Output-First)** in the spec is closest to this model, and its "cons" include "'Jobs' view that's not really about jobs." This is actually a FEATURE, not a bug. Users don't think about jobs.

2. **Legal workflow specificity**: The spec mentions Bates ranges multiple times. Bates is about DOCUMENTS (data), not jobs. Legal users track "I've produced SMITH000001-030521" not "Export job is 67% complete."

3. **The spec's own insight** (Section 2.3): "Users don't think: 'I have 5 jobs running.' Users think: 'I need to get my FIX logs into Bloomberg TCA format.'" This is data-state thinking, not job thinking.

4. **Pipeline summary in Alternative E**: The one-liner `1,247 discovered -> 1,235 parsed -> 2 exports` is DATA STATE counts, not job counts. The spec is already gravitating toward this.

### New Gaps Introduced

- **GAP-STATE-001**: If we design around data states, how do we handle jobs that don't fit (e.g., Backtest is a validation, not a state transition)?
- **GAP-STATE-002**: How do we show partial states? (e.g., 500 files parsed, 500 pending)

---

## Summary: Foundational Answers

### GAP-CORE-001 Answer
**Users need to find their OUTPUT and know if it's READY.**
The Jobs view is fundamentally about OUTPUT LOCATION + READINESS, not task management.

### GAP-MENTAL-001 Answer
**Users think in DATA STATES, not JOBS.**
Design around the progression: Source -> Processed -> Output. Jobs are the invisible implementation.

### Recommended Design Direction

These two insights together point strongly toward a hybrid of **Alternative B (Output-First)** and **Alternative E (Hybrid)**:

- Lead with DATA STATES (not pipeline boxes, but state summary)
- Show OUTPUT PROMINENCE (file paths, "ready" indicators)
- Jobs are listed BELOW as the "how" not the "what"
- Failed jobs surface as BLOCKERS to state progression

```
┌─ DATA STATUS ───────────────────────────────────────────────────┐
│                                                                  │
│  SOURCE            PROCESSED           OUTPUT                   │
│  1,247 files  ->   1,235 parsed   ->   bloomberg-tca (ready)    │
│                    (12 failed!)        concordance (67%)         │
│                                                                  │
│  ────────────────────────────────────────────────────────────── │
│                                                                  │
│  READY TO USE                                                    │
│  ✓ bloomberg-tca -> ./tca_upload.csv (2.3 MB)    [Open folder]  │
│                                                                  │
│  IN PROGRESS                                                     │
│  ↻ concordance -> production_001/ (67%, ETA 6m)  [Cancel]       │
│                                                                  │
│  NEEDS ATTENTION                                                 │
│  ✗ 12 files failed parsing                       [View] [Retry] │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

This design:
1. Answers "Is my output ready?" immediately (top section + READY TO USE)
2. Uses data-state mental model (SOURCE -> PROCESSED -> OUTPUT)
3. Surfaces blockers prominently (NEEDS ATTENTION)
4. Puts output paths front-and-center (copyable file paths)

---

## New Gaps Introduced This Round

| ID | Description | Priority |
|----|-------------|----------|
| GAP-OUTPUT-001 | How to show output spanning multiple jobs | MEDIUM |
| GAP-OUTPUT-002 | Empty state when no completed output exists | LOW |
| GAP-STATE-001 | How Backtest jobs fit data-state model | MEDIUM |
| GAP-STATE-002 | How to show partial/in-progress states | MEDIUM |

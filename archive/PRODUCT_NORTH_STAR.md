# Casparian Flow - Product North Star

**Last Updated:** January 2025

**Purpose:** Reference document capturing product vision, architecture decisions, UX principles, and feature roadmap. Informed by Jon Blow/Casey Muratori design philosophy.

---

## The One-Liner

**Transform dark data into queryable datasets through AI-generated, human-approved, sandboxed parsers.**

---

## Core Philosophy

### What We Believe

1. **AI generates, humans approve.** AI is a proposal engine, not an autonomous agent. Every AI-generated artifact (parser code) requires human approval of its *output*, not its implementation.

2. **Show output, not code.** Users care about results. "Did my messy CSV become a clean table?" They don't need to read Python to answer that question.

3. **Sandbox everything.** AI-generated code runs in isolation (Bridge Mode). It cannot escape, cannot corrupt, cannot cause damage outside its boundary.

4. **Make the safe path easy.** Manual approval is the default. Auto-approve ("red button") is opt-in, hard to enable, and fully logged.

5. **Deterministic after approval.** Once a parser becomes a signed plugin, execution is deterministic. No AI in the hot path. Just code running on data.

### What We Don't Believe

1. **"Agents all the way down"** - Agents talking to agents is a recipe for unpredictable behavior. AI has ONE job: generate parser code.

2. **"AI can figure it out"** - AI is wrong 10-30% of the time. Always show the user what happened. Always let them correct it.

3. **"Users will read the code"** - They won't. Show them input vs output. That's what they understand.

4. **"More automation is better"** - More automation without visibility is a trust destroyer. Users need to see, understand, and approve.

---

## Architecture

### The Bounded AI Zone

```
┌─────────────────────────────────────────────────────────────────┐
│                     THE BOUNDED AI ZONE                         │
│                        (Parser Lab)                             │
│                                                                 │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐        │
│   │   Sample    │ →  │  AI Gen     │ →  │  Sandbox    │        │
│   │   File      │    │  Parser     │    │  Execute    │        │
│   └─────────────┘    └─────────────┘    └─────────────┘        │
│                                               │                 │
│                                               ▼                 │
│                                    ┌─────────────────┐          │
│                                    │  User Reviews   │          │
│                                    │  OUTPUT         │          │
│                                    │  (not code)     │          │
│                                    └────────┬────────┘          │
│                                             │                   │
│                          ┌──────────────────┴────────────────┐  │
│                          ▼                                   ▼  │
│                    [Approve]                           [Refine] │
│                          │                                   │  │
│                          ▼                                   │  │
│              ┌─────────────────────┐                         │  │
│              │  Signed Plugin      │ ←───────────────────────┘  │
│              │  (immutable)        │                            │
│              └─────────────────────┘                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  DETERMINISTIC EXECUTION                        │
│                                                                 │
│   Scout (discover) → Tag → Plugin (execute) → Sink (write)      │
│                                                                 │
│   No AI here. Just code running in Bridge Mode sandbox.         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Where AI Lives (Bounded)

| Location | AI Role | Boundary |
|----------|---------|----------|
| Parser Lab | Generate parser code | Sandbox execution, user approval |
| Parser Lab | Refine parser from feedback | Same as above |
| Parser Lab | Explain parsing errors | Read-only, informational |

### Where AI Does NOT Live (Deterministic)

| Component | Why No AI |
|-----------|-----------|
| Scout | File discovery is deterministic (readdir, glob match) |
| Sentinel | Job orchestration is deterministic (queue, dispatch) |
| Bridge Mode | Plugin execution is deterministic (run approved code) |
| Sinks | Output writing is deterministic (write Arrow batches) |

---

## The Approval Spectrum

```
CONSERVATIVE                                              AGGRESSIVE
     │                                                         │
     ▼                                                         ▼
┌─────────┐         ┌─────────────────┐         ┌─────────────────┐
│ Manual  │         │ Smart Auto      │         │ Full Auto       │
│ Review  │         │ (>99% backtest) │         │ (Red Button)    │
├─────────┤         ├─────────────────┤         ├─────────────────┤
│ Every   │         │ High-confidence │         │ All parsers     │
│ parser  │         │ auto-approves   │         │ auto-approve    │
│ needs   │         │ Low-confidence  │         │ No human in     │
│ human   │         │ needs review    │         │ loop            │
│ approval│         │                 │         │                 │
└─────────┘         └─────────────────┘         └─────────────────┘
     │                      │                          │
     │                      │                          │
  DEFAULT              POWER USER                 DANGEROUS
                     (opt-in)                   (opt-in + warning)
```

### Red Button Implementation

The "red button" (full auto-approve) should be:

1. **Hard to enable:**
   - Located in Settings, not in main workflow
   - Requires typing confirmation phrase: "I understand the risks"
   - Shows warning about implications

2. **Easy to disable:**
   - One-click toggle off
   - Per-source granularity (can enable for trusted sources only)

3. **Fully logged:**
   - Every auto-approved parser recorded
   - Audit trail of what ran, when, on what data
   - Easy rollback if something goes wrong

4. **Visually distinct:**
   - Banner in UI when auto-approve is active
   - Different color/icon for auto-approved vs manually-approved parsers

---

## Core User Flows

### Flow 1: Parser Lab (Primary)

```
┌─────────────────────────────────────────────────────────────────┐
│ PARSER LAB                                            [x]       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ Source: sales_january.csv                    [Change File]      │
│                                                                 │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ RAW INPUT                              (first 10 rows)      │ │
│ ├─────────────────────────────────────────────────────────────┤ │
│ │ txn_id,amount,date,customer                                 │ │
│ │ TXN-001,$127.50,01/15/2024,Alice                            │ │
│ │ TXN-002,$89.99,01/15/2024,Bob                               │ │
│ │ TXN-003,N/A,01/16/2024,Charlie                              │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                                                 │
│                    [Generate Parser]                            │
│                                                                 │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ PARSED OUTPUT                                               │ │
│ ├──────────┬──────────┬─────────────┬───────────┬─────────────┤ │
│ │ txn_id   │ amount   │ date        │ customer  │ _errors     │ │
│ │ (string) │ (float)  │ (date)      │ (string)  │             │ │
│ ├──────────┼──────────┼─────────────┼───────────┼─────────────┤ │
│ │ TXN-001  │ 127.50   │ 2024-01-15  │ Alice     │             │ │
│ │ TXN-002  │ 89.99    │ 2024-01-15  │ Bob       │             │ │
│ │ TXN-003  │ NULL     │ 2024-01-16  │ Charlie   │ amt: 'N/A'  │ │
│ └──────────┴──────────┴─────────────┴───────────┴─────────────┘ │
│                                                                 │
│ Warning: 1 parse error (row 3: amount)           [View Code]    │
│                                                                 │
│ [Test on All Files]              [Approve & Save]  [Refine...] │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key UX Principles:**
- Show raw input AND parsed output side-by-side
- Errors visible in context, not hidden in logs
- "View Code" is secondary action (for power users)
- Primary actions: Test, Approve, Refine

### Flow 2: Refine Parser

When user clicks [Refine...]:

```
┌─────────────────────────────────────────────────────────────────┐
│ What's wrong?                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ ( ) Column type is wrong                                        │
│     → Click on column header to change type                     │
│                                                                 │
│ ( ) Date format is wrong                                        │
│     → Current: MM/DD/YYYY  Change to: [___________]             │
│                                                                 │
│ ( ) Null values not detected                                    │
│     → Add null values: [N/A, NULL, -, (empty)    ]              │
│                                                                 │
│ ( ) Something else                                              │
│     → Describe: [__________________________________]            │
│       Example: "amount column has (refund) suffix sometimes"    │
│                                                                 │
│                         [Regenerate Parser]                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key UX Principles:**
- Common fixes are one-click (type change, date format, null values)
- Free-form description for complex issues
- User describes problem in plain English, AI regenerates
- No Python editing required (but available via "View Code")

### Flow 3: Backtest

When user clicks [Test on All Files]:

```
┌─────────────────────────────────────────────────────────────────┐
│ BACKTEST RESULTS                                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ Files: 12 │ Rows: 50,847 │ Time: 2.3s                           │
│                                                                 │
│ ✓ sales_january.csv    - 4,521 rows - 100% success              │
│ ✓ sales_february.csv   - 4,102 rows - 100% success              │
│ ! sales_march.csv      - 4,847 rows - 99.7% (15 errors)         │
│   └─ [View 15 errors]                                           │
│ ✓ sales_april.csv      - 4,201 rows - 100% success              │
│ ✓ sales_may.csv        - 4,456 rows - 100% success              │
│ ✓ sales_june.csv       - 4,102 rows - 100% success              │
│ ...                                                             │
│                                                                 │
│ ─────────────────────────────────────────────────────────────── │
│ Summary: 99.97% success rate (15 failures in 50,847 rows)       │
│                                                                 │
│ [View All Errors]       [Publish Plugin]      [Fix Errors First]│
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key UX Principles:**
- Test against ALL files before publishing, not just sample
- Show per-file breakdown with error counts
- Drill-down to specific errors
- User makes informed decision: publish with known errors or fix first

### Flow 4: Error Drill-Down

When user clicks [View 15 errors]:

```
┌─────────────────────────────────────────────────────────────────┐
│ ERRORS: sales_march.csv                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ Row 1847:                                                       │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ Raw:    TXN-1847,VOID,03/15/2024,Diana                      │ │
│ │ Error:  Column 'amount': cannot parse 'VOID' as float       │ │
│ │ Suggestion: Add 'VOID' to null values, or handle as refund  │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                                                 │
│ Row 2901:                                                       │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ Raw:    TXN-2901,$-50.00,03/22/2024,Eve                     │ │
│ │ Error:  Column 'amount': negative value $-50.00             │ │
│ │ Suggestion: Negative amounts may be refunds - consider flag │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                                                 │
│ ... (13 more similar errors)                                    │
│                                                                 │
│ Patterns detected:                                              │
│ • 10 errors: 'VOID' in amount column                            │
│ • 5 errors: Negative amounts                                    │
│                                                                 │
│ [Add 'VOID' to nulls]  [Allow negatives]  [Fix Manually]        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key UX Principles:**
- Show raw data that caused error
- AI suggests likely fix
- Pattern detection groups similar errors
- One-click fixes for common patterns

---

## Feature Roadmap

### Phase 1: Core Parser Lab (NOW)

**Goal:** User can create, test, and approve parsers with AI assistance.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| File preview (raw input) | P0 | Exists | Show first N rows |
| AI parser generation | P0 | Exists | Generate Python from sample |
| Sandbox execution | P0 | Exists | Bridge Mode |
| Parsed output preview | P0 | Exists | Show transformed data |
| Error display in context | P1 | Needed | Show which rows failed, why |
| Manual approval flow | P0 | Exists | User clicks approve |
| Parser save/load | P0 | Exists | Persist to DB |

### Phase 2: Iteration & Refinement

**Goal:** User can easily fix parser issues without editing code.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Refine dialog | P1 | Needed | Common fixes UI |
| Click-to-fix column types | P1 | Needed | Click column, pick type |
| Null value configuration | P1 | Needed | Add/remove null representations |
| Date format picker | P2 | Needed | Common date formats |
| Free-form refinement prompt | P1 | Needed | "Describe what's wrong" |
| Regenerate from feedback | P1 | Needed | AI incorporates feedback |

### Phase 3: Backtest & Confidence

**Goal:** User knows exactly what will happen before publishing.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Backtest against all files | P1 | Needed | Run parser on full dataset |
| Per-file success/failure | P1 | Needed | Breakdown by file |
| Error drill-down | P1 | Needed | See specific failures |
| Error pattern detection | P2 | Needed | Group similar errors |
| One-click pattern fixes | P2 | Needed | "Add 'VOID' to nulls" |
| Confidence score | P2 | Needed | % success, edge case coverage |

### Phase 4: Publishing & Plugins

**Goal:** Approved parsers become signed, immutable plugins.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Publish parser as plugin | P1 | Partial | Sign and deploy |
| Plugin versioning | P1 | Needed | Track which version ran when |
| Plugin audit log | P2 | Needed | Who approved, when |
| Rollback capability | P2 | Needed | Revert to previous version |

### Phase 5: Auto-Approve (Red Button)

**Goal:** Power users can enable automatic approval for trusted patterns.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Auto-approve setting | P2 | Needed | Opt-in in settings |
| Confirmation phrase | P2 | Needed | "I understand the risks" |
| Per-source granularity | P3 | Needed | Enable for specific sources |
| Visual indicator | P2 | Needed | Banner when active |
| Full audit logging | P2 | Needed | Every auto-approval logged |

### Phase 6: Discovery Intelligence

**Goal:** Help users understand their data before parsing.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| File clustering | P2 | Partial | Group similar files |
| Value scoring | P3 | Needed | Which files most valuable |
| Schema similarity detection | P3 | Needed | Files with same structure |
| Suggested parsers | P3 | Needed | "These 50 files look similar" |

### Phase 7: Continuous Monitoring

**Goal:** Detect when source data changes and parsers need updating.

| Feature | Priority | Status | Notes |
|---------|----------|--------|-------|
| Schema drift detection | P3 | Needed | Alert when schema changes |
| Error rate monitoring | P3 | Needed | Alert when failures spike |
| Parser health dashboard | P3 | Needed | Overview of all parsers |
| Suggested updates | P3 | Needed | "New column detected" |

---

## UX Principles (Jon Blow / Casey Muratori)

### Do This

1. **Show, don't tell.** User sees raw input and parsed output side-by-side. No mystery about what happened.

2. **Make the safe path easy.** Default is manual approval. Auto-approve is opt-in and hard to enable.

3. **Tight iteration loops.** Generate → test → see results → refine should be <10 seconds total.

4. **Concrete artifacts.** The parser is Python code. The user can read it if they want. No black boxes.

5. **Fail visibly.** When parsing fails, show exactly which row, which column, what value, and suggest a fix.

6. **Test before commit.** Backtest against ALL files before publishing. No "works on my machine" surprises.

7. **Escape hatches for power users.** "View Code" and "Edit Manually" always available, just not primary.

### Don't Do This

1. **Don't hide errors in logs.** Errors appear in the main UI, in context, with suggestions.

2. **Don't require code editing.** Common fixes are clickable. Free-form feedback for the rest.

3. **Don't auto-approve by default.** Users must explicitly opt in to reduced oversight.

4. **Don't show confidence scores without explanation.** "87% confidence" means nothing. "12 files tested, 1 had errors" means something.

5. **Don't make users wait.** If AI generation takes >5 seconds, show progress. If >30 seconds, something is wrong.

6. **Don't use AI where rules work.** File discovery, pattern matching, job dispatch - these are deterministic. No AI needed.

---

## Anti-Patterns to Avoid

### Architecture Anti-Patterns

| Anti-Pattern | Why It's Bad | What To Do Instead |
|--------------|--------------|---------------------|
| Agents everywhere | Unpredictable, untestable, unexplainable | AI in one place (Parser Lab), deterministic everywhere else |
| AI in hot path | Slow, expensive, unreliable | AI generates code, code executes deterministically |
| Auto-approve by default | Users don't understand what they approved | Manual approval default, auto-approve opt-in |
| Confidence scores without context | "87%" is meaningless | "12 files, 50K rows, 15 errors" is meaningful |

### UX Anti-Patterns

| Anti-Pattern | Why It's Bad | What To Do Instead |
|--------------|--------------|---------------------|
| Show code, not output | Users don't read code | Show input → output transformation |
| Hide errors in logs | Users don't check logs | Errors in main UI, in context |
| Require code editing to fix | Most users can't code | Click-to-fix, describe in English |
| Test on sample only | Sample doesn't catch edge cases | Backtest on ALL files before publish |

### Product Anti-Patterns

| Anti-Pattern | Why It's Bad | What To Do Instead |
|--------------|--------------|---------------------|
| Build for imaginary users | Waste time on unused features | Ship simple thing, watch real usage |
| Optimize before measuring | Premature optimization | Get it working, then profile |
| Abstract before needed | Complexity without benefit | Direct code until pattern emerges |
| "Future-proof" design | YAGNI | Solve today's problem today |

---

## Open Questions

### Technical

1. **Generation latency:** What's acceptable? Target <5 seconds for generate + test.

2. **Backtest scale:** How to backtest 10,000 files quickly? Parallel execution? Sampling?

3. **Schema drift:** How to detect when source files change format? Content hashing? Schema fingerprinting?

4. **Parser failure recovery:** When AI can't generate a working parser, what's the escape hatch?

### Product

1. **Pricing model for AI:** Per-generation? Per-month? Bundled? How to make unit economics work?

2. **Power user vs casual user:** Same UI? Progressive disclosure? Separate "pro mode"?

3. **Multi-user collaboration:** Multiple people editing same parser? Conflict resolution?

4. **Versioning UX:** How to show parser history? Diff between versions?

### Business

1. **Competitive positioning:** vs Fivetran, Airbyte, dbt? What's the wedge?

2. **Target user:** Data engineers? Analysts? Business users? Different needs.

3. **Deployment model:** Desktop app? Cloud? Hybrid?

---

## Glossary

| Term | Definition |
|------|------------|
| **Parser** | Python code that transforms raw files into Arrow batches |
| **Plugin** | Published, signed, immutable parser deployed for execution |
| **Bridge Mode** | Sandboxed execution environment for plugins (host/guest isolation) |
| **Backtest** | Running a parser against all matching files to measure success rate |
| **Red Button** | User opt-in to skip manual approval (auto-approve mode) |
| **Refinement** | Iterating on parser based on user feedback without code editing |
| **Schema Drift** | When source file format changes over time |
| **Bounded AI** | AI that generates proposals (code) but cannot execute or escape sandbox |

---

## References

- `CLAUDE.md` - Main codebase instructions
- `ARCHITECTURE.md` - System design
- `PARALLEL_PLAN.md` - Development orchestration
- `UI_RESTRUCTURE_PLAN.md` - UI consolidation plan

---

## Changelog

| Date | Change |
|------|--------|
| 2025-01 | Initial version from product ideation session |


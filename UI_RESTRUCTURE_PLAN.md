# UI Restructure Plan

**Approach**: Jon Blow / Casey Muratori style - solve real problems, no over-engineering, each phase delivers value.

## Current State (The Problem)

```
5 Tabs:
├── DASHBOARD  → Monitor Sentinel (workers, jobs, throughput)
├── SCOUT      → File discovery, pattern routes, "Process Files" button
├── CONFIG     → RoutingTable (pattern → tag rules)
├── DATA       → Browse completed jobs, query outputs
└── PUBLISH    → Plugin publishing
```

**Problems:**
1. SCOUT and CONFIG both have pattern-based rules (confusing)
2. SCOUT outputs go somewhere, but you view them in DATA (disconnected)
3. "Process Files" is a black box - no schema visibility, no preview
4. User can't see what's IN files, only counts
5. DATA tab shows Sentinel job outputs, not Scout outputs (different systems!)

## Target State

```
3 Tabs:
├── DASHBOARD  → Monitor (unchanged)
├── PIPELINES  → Unified data flow: Sources → Files → Schema → Routes → Outputs
└── PUBLISH    → Plugin publishing (unchanged)
```

## Key Insight

The user's workflow is:
```
Discover files → Understand structure → Define routing → Process → Verify results
     ✓              MISSING              partial         blind      MISSING
```

We're missing the most valuable parts: **seeing what's in files** and **verifying results**.

---

## Phase 1: Schema Visibility (THE CRITICAL FIX)

**Goal**: User can see what's inside their files before processing.

**What to build:**
```
When a file pattern is selected, show:
┌─────────────────────────────────────────┐
│ Pattern: *.csv                          │
│ Matched: 50 files (2.3 MB)              │
│                                         │
│ SCHEMA (inferred from sample):          │
│   id       int64                        │
│   name     utf8                         │
│   amount   float64                      │
│   date     utf8                         │
│                                         │
│ SAMPLE DATA:                            │
│ ┌────┬─────────┬────────┬────────────┐ │
│ │ id │ name    │ amount │ date       │ │
│ ├────┼─────────┼────────┼────────────┤ │
│ │ 1  │ Alice   │ 100.50 │ 2024-01-15 │ │
│ │ 2  │ Bob     │ 250.00 │ 2024-01-16 │ │
│ └────┴─────────┴────────┴────────────┘ │
│                                         │
│ ⚠️ 2 files have different schemas       │
└─────────────────────────────────────────┘
```

**Implementation:**

1. **Backend** (`scout.rs`): Add `scout_preview_schema` command
   - Takes: source_id, pattern (or file_id for single file)
   - Returns: { columns: [{name, type, nullable}], sample_rows: [[values]], schema_conflicts: [{file, difference}] }
   - Uses existing `transform_file` logic to infer schema without writing output

2. **Frontend** (`ScoutTab.svelte`): Add schema panel
   - Shows when pattern is entered or files selected
   - Displays inferred schema in clean table
   - Shows sample data (first 5 rows)
   - Warns about schema conflicts

**Files to modify:**
- `ui/src-tauri/src/scout.rs` - Add schema preview command
- `crates/casparian_scout/src/transform.rs` - Expose schema inference without full transform
- `ui/src/lib/components/scout/ScoutTab.svelte` - Add SchemaPreview section
- `ui/src/lib/stores/scout.svelte.ts` - Add schema state/methods

**Test**: User enters `*.csv`, sees column names and sample data immediately.

---

## Phase 2: Transform Clarity

**Goal**: User knows exactly what "Process" will do before clicking.

**What to build:**
```
Replace vague "Process Files" button with explicit summary:
┌─────────────────────────────────────────┐
│ READY TO TRANSFORM                      │
│                                         │
│ Route: CSV Sales (*.csv)                │
│   50 files → Parquet                    │
│   Output: /output/bronze/sales/         │
│   Schema: 4 columns                     │
│                                         │
│ Route: JSONL Events (*.jsonl)           │
│   10 files → Parquet                    │
│   Output: /output/bronze/events/        │
│   Schema: 3 columns                     │
│                                         │
│ ⚠️ 5 files have no matching route       │
│ ⚠️ 2 files have schema conflicts        │
│                                         │
│ [Transform 60 Files]                    │
└─────────────────────────────────────────┘
```

**Implementation:**

1. Rename button from "Process Files" → "Transform to Parquet" (or dynamic based on sink)
2. Above button, show summary of what each route will do
3. Show warnings prominently (unmatched files, conflicts)
4. Use existing `coverage` data, enhance with schema info from Phase 1

**Files to modify:**
- `ui/src/lib/components/scout/ScoutTab.svelte` - Replace process section with summary

**Test**: User sees exactly what will happen before clicking Transform.

---

## Phase 3: Output Visibility

**Goal**: User can verify transform results without leaving Scout/Pipelines.

**What to build:**
```
After processing, show outputs inline:
┌─────────────────────────────────────────┐
│ OUTPUTS                                 │
│                                         │
│ /output/bronze/sales/                   │
│   └── part-0000.parquet (1.2 MB)        │
│       50 rows, 4 columns                │
│       [Preview] [Query]                 │
│                                         │
│ /output/bronze/events/                  │
│   └── part-0000.parquet (0.8 MB)        │
│       10 rows, 3 columns                │
│       [Preview] [Query]                 │
└─────────────────────────────────────────┘
```

**Implementation:**

1. **Backend**: Add `scout_list_outputs` command
   - Returns: output files created by routes, with metadata

2. **Frontend**: Add Outputs section to ScoutTab
   - List output files per route
   - Preview button shows first N rows (reuse DataGrid)
   - Query button opens simple SQL interface

**Files to modify:**
- `ui/src-tauri/src/scout.rs` - Add output listing
- `ui/src/lib/components/scout/ScoutTab.svelte` - Add outputs section
- Move/reuse `DataGrid.svelte` for preview

**Test**: User processes files, sees output Parquet files immediately, can preview contents.

---

## Phase 4: Tab Consolidation

**Goal**: Remove redundancy, clarify navigation.

**Changes:**

### 4a. Rename SCOUT → PIPELINES
Simple rename. More accurate - it's about data pipelines, not just "scouting".

### 4b. Evaluate CONFIG tab
Two options based on whether RoutingTable is actively used:

**If RoutingTable IS used for Sentinel job routing:**
- Keep CONFIG but rename to "DISPATCH" or "SENTINEL CONFIG"
- Clearly separate from PIPELINES (data flow vs job dispatch)

**If RoutingTable is NOT used / redundant with Scout:**
- Delete CONFIG tab entirely
- Move any needed functionality into PIPELINES

### 4c. Merge DATA into PIPELINES
- DATA tab currently shows Sentinel job outputs
- If Scout outputs are separate, keep DATA for Sentinel, add Outputs to PIPELINES
- If they're the same, merge entirely

**Recommendation:**
- Rename SCOUT → PIPELINES
- Hide CONFIG for now (can restore if needed)
- Add Outputs sub-section to PIPELINES (Phase 3)
- Keep DATA tab but evaluate if still needed after Phase 3

**Files to modify:**
- `ui/src/routes/+page.svelte` - Tab structure
- Navigation labels

---

## Phase 5: Polish (Only If Needed)

After Phases 1-4, evaluate:
- Is DATA tab still needed? (Maybe outputs are fully covered in PIPELINES)
- Is CONFIG tab still needed? (Maybe Scout routes cover everything)
- Do we need sub-tabs within PIPELINES? (Files | Schema | Routes | Outputs)

**Don't over-plan this.** See what feels right after real usage.

---

## Execution Order

```
Phase 1: Schema Visibility     ← HIGHEST VALUE, do first
Phase 2: Transform Clarity     ← Quick win, builds on Phase 1
Phase 3: Output Visibility     ← Completes the loop
Phase 4: Tab Consolidation     ← Cleanup, do last
Phase 5: Polish                ← Only if needed
```

## What We're NOT Doing

1. **Not building elaborate sub-navigation** - Start simple, add if needed
2. **Not merging unrelated systems** - Config (job dispatch) stays separate from Scout (data pipeline) unless proven redundant
3. **Not redesigning everything** - Incremental improvements
4. **Not adding features "just in case"** - Each phase solves a specific user problem

## Success Criteria

After all phases:
- User can see file schemas before processing ✓
- User knows exactly what Transform will do ✓
- User can verify outputs without switching tabs ✓
- No duplicate "routing" concepts visible ✓
- Tab count reduced from 5 to 3 ✓

---

## Jon/Casey Review

**Jon**: "Phase 1 is the only thing that actually matters. Schema visibility is the missing feature. Everything else is shuffling furniture. Do Phase 1, ship it, see if you even need the rest."

**Casey**: "I like that each phase is independent and testable. But I'd push back on Phase 4 - don't reorganize tabs until you've lived with Phases 1-3. The right organization might become obvious."

**Verdict**: Start with Phase 1. It's the highest value, lowest risk change. Everything else can wait.

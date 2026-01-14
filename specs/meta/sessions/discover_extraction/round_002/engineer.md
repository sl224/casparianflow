# Engineer Response: Round 002 (Revision)

**Date:** 2026-01-13
**Focus:** Revised GAP-STATE-001 + GAP-TRANS-001 based on reviewer feedback
**Engineer Role:** Incorporate user decisions and fix critical issues

---

## Revision: GAP-STATE-001 + GAP-TRANS-001

**Changes from Round 1:**

1. **Esc from Testing now goes to EditRule** (not Browse) - preserves rule draft so user can fix issues and re-test
2. **Publishing confirmation flow explicit** - Shows `Enter` as required trigger from Publishing (Confirming) to Publishing (Saving)
3. **Added `j` key to Published state** - View job status transition now shown
4. **Removed template matching scope creep** - Moved to GAP-TMPL-001 deferred section
5. **Clarified Publishing sub-states** - Confirming requires user action, Saving/Starting are auto-transitions

---

### Revised State Diagram

```
+-----------------------------------------------------------------------------------+
|                          GLOB EXPLORER STATE MACHINE                               |
+-----------------------------------------------------------------------------------+
|                                                                                    |
|  +--------------------------- NAVIGATION LAYER -----------------------------+     |
|  |                                                                           |     |
|  |   +--------------+    l/Enter     +--------------+                        |     |
|  |   |    BROWSE    |--------------->|    BROWSE    |                        |     |
|  |   |   (at root)  |                |  (in folder) |                        |     |
|  |   |              |<---------------|              |                        |     |
|  |   +------+-------+   h/Backspace  +------+-------+                        |     |
|  |          |                               |                                |     |
|  |          | / (start typing)              | / (start typing)               |     |
|  |          v                               v                                |     |
|  |   +--------------+                +--------------+                        |     |
|  |   |  FILTERING   |                |  FILTERING   |                        |     |
|  |   |  (heat map)  |                |  (in folder) |                        |     |
|  |   |              |<-------------->|              |                        |     |
|  |   +------+-------+   l/Enter, h   +------+-------+                        |     |
|  |          |                               |                                |     |
|  |          | Esc (clear pattern, stay in BROWSE)                            |     |
|  |          v                               |                                |     |
|  |   [Return to BROWSE at current prefix]   |                                |     |
|  |                                          |                                |     |
|  +------------------------------------------+--------------------------------+     |
|                                              |                                     |
|             e (with matches > 0)             | e (with matches > 0)               |
|                       |                      |                                     |
|                       +----------+-----------+                                     |
|                                  v                                                 |
|  +--------------------------- RULE EDITING LAYER ----------------------------+    |
|  |                                                                            |    |
|  |   +------------------------------------------------------------------+     |    |
|  |   |                         EDIT_RULE                                 |     |    |
|  |   |   Glob pattern | Fields | Base tag | Conditions                   |     |    |
|  |   |   (Tab cycles sections, j/k navigates within)                     |     |    |
|  |   +-------------------------------+----------------------------------+     |    |
|  |                                   |                                        |    |
|  |         +-----------+-------------+-------------+-----------+              |    |
|  |         |           |                           |           |              |    |
|  |         | t (test)  | Esc (cancel)              |           |              |    |
|  |         v           v                           |           |              |    |
|  |   +--------------+  [Return to BROWSE]          |           |              |    |
|  |   |   TESTING    |  (preserves prefix)          |           |              |    |
|  |   | +----------+ |                              |           |              |    |
|  |   | | Running  | |                              |           |              |    |
|  |   | +----+-----+ |                              |           |              |    |
|  |   |      | auto  |                              |           |              |    |
|  |   |      v       |                              |           |              |    |
|  |   | +----------+ |                              |           |              |    |
|  |   | | Complete | |                              |           |              |    |
|  |   | +----+-----+ |                              |           |              |    |
|  |   +------+-------+                              |           |              |    |
|  |          |                                      |           |              |    |
|  |          | p (publish)    e (edit)   Esc        |           |              |    |
|  |          |                   |         |        |           |              |    |
|  |          |                   +---------+--------+           |              |    |
|  |          |                             |                    |              |    |
|  |          |                             v                    |              |    |
|  |          |                    [Back to EDIT_RULE]           |              |    |
|  |          |                    (draft preserved)             |              |    |
|  |          v                                                  |              |    |
|  |   +----------------+                                        |              |    |
|  |   |   PUBLISHING   |                                        |              |    |
|  |   | +-----------+  |                                        |              |    |
|  |   | | Confirming|--+-- Esc ---------------------------------+              |    |
|  |   | +-----+-----+  |  (back to EditRule)                                   |    |
|  |   |       |        |                                                       |    |
|  |   |       | Enter (confirm)                                                |    |
|  |   |       v        |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   | | Saving    |  |                                                       |    |
|  |   | +-----+-----+  |                                                       |    |
|  |   |       | auto   |                                                       |    |
|  |   |       v        |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   | | Starting  |  |                                                       |    |
|  |   | +-----------+  |                                                       |    |
|  |   +-------+--------+                                                       |    |
|  |           |                                                                |    |
|  |           | (auto-transition on success)                                   |    |
|  |           v                                                                |    |
|  |   +----------------+                                                       |    |
|  |   |   PUBLISHED    |                                                       |    |
|  |   |   Complete!    |                                                       |    |
|  |   |   Job ID: xxx  |                                                       |    |
|  |   +-------+--------+                                                       |    |
|  |           |                                                                |    |
|  |           +-- Enter/Esc --> [Return to BROWSE at root]                     |    |
|  |           |                                                                |    |
|  |           +-- j ----------> [View Job Status screen]                       |    |
|  |                                                                            |    |
|  +----------------------------------------------------------------------------+    |
|                                                                                    |
|   g/Esc from BROWSE/FILTERING --> Exit Glob Explorer (return to Discover)         |
|                                                                                    |
+------------------------------------------------------------------------------------+
```

---

### Revised State Definitions Table

| State | Entry Condition | Exit Conditions | Preserves Context |
|-------|-----------------|-----------------|-------------------|
| `Browse` | Default, Esc from Filtering, Enter/Esc from Published | `l`/Enter -> drill, `/` -> Filtering, `e` -> EditRule (DISABLED, no pattern), `g`/Esc -> exit | prefix: Yes |
| `Filtering` | `/` from Browse | Esc -> Browse, `l` -> drill, `e` -> EditRule (when matches > 0) | prefix: Yes, pattern: Yes |
| `EditRule` | `e` from Filtering (when matches > 0), `e` from Testing, Esc from Publishing | `t` -> Testing, Esc -> Browse | prefix: Yes, pattern: as glob, rule draft: Yes |
| `Testing` | `t` from EditRule | `p` -> Publishing, `e` -> EditRule, **Esc -> EditRule** | rule draft: Yes |
| `Publishing` | `p` from Testing (Complete) | **Enter -> Saving (then auto -> Published)**, Esc -> EditRule | rule draft: Yes |
| `Published` | auto from Publishing (success) | Enter/Esc -> Browse (root), **`j` -> Job Status screen** | None (clean slate) |

**Key changes highlighted in bold**

---

### Revised Transition Table

| From State | Key/Trigger | To State | Condition | Notes |
|------------|-------------|----------|-----------|-------|
| Browse | `l` / Enter | Browse (deeper) | folder selected | Drill into folder |
| Browse | `h` / Backspace | Browse (parent) | not at root | Go up one level |
| Browse | `/` | Filtering | any | Start pattern typing |
| Browse | `e` | (disabled) | no pattern | Show hint: "Press / to filter first" |
| Browse | `g` / Esc | Exit | any | Return to Discover view |
| Filtering | `l` / Enter | Filtering (deeper) | folder selected | Drill preserving pattern |
| Filtering | `h` | Filtering (parent) | not at root | Go up preserving pattern |
| Filtering | `e` | EditRule | matches > 0 | Pre-fill glob from pattern |
| Filtering | `e` | (disabled) | matches = 0 | Nothing to extract |
| Filtering | Esc | Browse | any | Clear pattern, stay at prefix |
| Filtering | `g` | Exit | any | Return to Discover view |
| EditRule | `t` | Testing | rule valid | Start test run |
| EditRule | Esc | Browse | any | Cancel rule, preserve prefix |
| EditRule | Tab | EditRule | any | Cycle sections |
| EditRule | j/k | EditRule | any | Navigate within section |
| Testing | `p` | Publishing | sub-state = Complete | Begin publish flow |
| Testing | `e` | EditRule | any | **Return to edit, draft preserved** |
| Testing | Esc | **EditRule** | any | **Cancel test, draft preserved** |
| Publishing (Confirming) | **Enter** | Publishing (Saving) | any | **User confirms publish** |
| Publishing (Confirming) | Esc | EditRule | any | Cancel publish, draft preserved |
| Publishing (Saving) | (auto) | Publishing (Starting) | save success | Auto-transition |
| Publishing (Starting) | (auto) | Published | job started | Auto-transition |
| Published | Enter | Browse (root) | any | Complete, fresh start |
| Published | Esc | Browse (root) | any | Complete, fresh start |
| Published | **`j`** | **Job Status** | any | **View job details** |

---

### Publishing Sub-State Flow (Detail)

```
Testing (Complete)
       |
       | p (publish)
       v
+------------------+
| PUBLISHING       |
|                  |
| Confirming       |   <-- User sees confirmation dialog
|  "Publish rule   |       with rule summary
|   'csv_files'?"  |
|                  |
|  [Enter] Confirm |   <-- EXPLICIT user action required
|  [Esc] Cancel    |
+--------+---------+
         |
         | Enter (user confirms)
         v
+------------------+
| PUBLISHING       |
|                  |
| Saving...        |   <-- Auto-progress, no user input
|  Writing rule    |
|  to database     |
+--------+---------+
         |
         | (auto on success)
         v
+------------------+
| PUBLISHING       |
|                  |
| Starting...      |   <-- Auto-progress, no user input
|  Queuing job     |
|  for tagged      |
|  files           |
+--------+---------+
         |
         | (auto on success)
         v
+------------------+
| PUBLISHED        |
|                  |
| Complete!        |
| Job ID: abc123   |
|                  |
| [Enter] Done     |
| [j] View job     |
+------------------+
```

---

### Validation (Re-check)

#### REACHABILITY: PASS
- [x] Browse - entry point
- [x] Filtering - via `/` from Browse
- [x] EditRule - via `e` from Filtering (matches > 0)
- [x] Testing - via `t` from EditRule
- [x] Publishing - via `p` from Testing (Complete)
- [x] Published - via Enter from Publishing (Confirming) then auto

#### ESCAPABILITY: PASS
- [x] Browse - `g`/Esc to exit
- [x] Filtering - Esc to Browse
- [x] EditRule - Esc to Browse
- [x] Testing - Esc to EditRule (preserves draft)
- [x] Publishing - Esc to EditRule (preserves draft)
- [x] Published - Enter/Esc to Browse (root)

**Esc chain from deepest state:**
```
Published -> Enter/Esc -> Browse (root) -> g/Esc -> Exit
```

**Esc chain preserving work:**
```
Testing -> Esc -> EditRule (draft preserved) -> Esc -> Browse (prefix preserved)
Publishing -> Esc -> EditRule (draft preserved) -> Esc -> Browse (prefix preserved)
```

#### DETERMINISM: PASS
- [x] Each (state, key) pair maps to exactly one action
- [x] No key collisions within any state
- [x] `e` disabled in Browse, enabled in Filtering/Testing

#### COMPLETENESS: PASS
- [x] `j` key in Published state now shown
- [x] `Enter` key in Publishing (Confirming) now shown
- [x] All keybindings documented in transition table

#### CONSISTENCY: PASS
- [x] Esc always cancels/goes back (now consistent - Testing/Publishing go to EditRule)
- [x] Enter always confirms/selects
- [x] `t` always means "test"
- [x] `e` always means "edit" (in contexts where enabled)

---

### Deferred Items

#### GAP-TMPL-001: Template Matching Flow (Moved from Round 1)

**Scope:** Define the UX for creating a rule from a SINGLE file using template matching.

**Original Round 1 proposal (deferred):**
- Navigate to file in flat results
- `Enter` to select/preview file
- `e` to "extract from this file" -> shows template matches
- Select template -> EditRule with template-suggested fields

**Why deferred:**
- Core `e` trigger behavior is now specified (requires Filtering with matches > 0)
- Template matching is a separate flow referenced in Phase 18g
- Adding it to this state machine would conflate pattern-based and template-based rule creation
- Should be addressed as a separate dialog/modal flow, not a state machine extension

**Recommended approach for future:**
- Template matching as a modal dialog from EditRule, not a separate entry path
- User can invoke template suggestions after entering EditRule via normal flow
- This preserves the simple "explore -> filter -> extract" mental model

#### GAP-CTX-001: Prefix Context Definition (From Round 1)

**Status:** LOW priority, deferred to Round 3

**Question:** What exactly is "prefix" when returning to Browse from different states?

**Proposed answer:** The folder path stack (e.g., `/data/mission_042/`) not just a string. Returning to Browse at "prefix" means restoring the navigated folder position.

---

### Examples (Updated)

**Example 1: Full rule creation flow with confirmation**
```
Browse (root)
  -> "/" type "**/*.csv"
  -> Filtering (showing 847 matches)
  -> "l" drill into /data folder
  -> Filtering (in folder, 234 matches)
  -> "e" (matches > 0, valid trigger)
  -> EditRule (glob pre-filled with "data/**/*.csv", fields inferred)
  -> "t" test
  -> Testing (progress... complete, 95% success)
  -> "p" publish
  -> Publishing (Confirming) - shows "Publish rule 'csv_data'? [Enter] Confirm [Esc] Cancel"
  -> Enter                                          <-- EXPLICIT USER ACTION
  -> Publishing (Saving... Starting...)
  -> Published (Job ID shown)
  -> Enter
  -> Browse (back at root, fresh start)
```

**Example 2: Cancel test, fix rule, re-test**
```
EditRule -> "t" test
  -> Testing (Running... Complete: 70% failed)
  -> Esc                                            <-- Go back to edit
  -> EditRule (rule draft PRESERVED)                <-- Fix the pattern
  -> "t" re-test
  -> Testing (now 95% success)
  -> "p" publish
  -> ...
```

**Example 3: Cancel publish, adjust rule**
```
Testing (Complete) -> "p"
  -> Publishing (Confirming)
  -> Esc                                            <-- Changed mind
  -> EditRule (rule draft PRESERVED)                <-- Adjust fields
  -> "t" re-test
  -> ...
```

**Example 4: View job after publish**
```
Published (Job ID: abc123)
  -> "j"                                            <-- View job status
  -> [Job Status screen showing progress]
```

---

## Summary

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-001 (CRITICAL) | Publishing confirmation flow now shows explicit `Enter` trigger between Confirming and Saving sub-states |
| ISSUE-R1-002 (CRITICAL) | Esc from Testing now returns to EditRule (preserves draft), not Browse |
| ISSUE-R1-004 (HIGH) | `Enter` key explicitly shown in Publishing transition |
| ISSUE-R1-006 (MEDIUM) | `j` key added to Published state transitions |
| ISSUE-R1-007 (MEDIUM) | Template matching discussion moved to GAP-TMPL-001 deferred section |

The revised state machine is now complete, consistent with the spec's confirmation requirements, and preserves user work appropriately when escaping from deep states.

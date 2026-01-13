## Review: GAP-STUB-001

**Reviewer:** Claude Opus 4.5
**Date:** 2026-01-12
**Engineer Proposal:** `engineer.md`

---

### Verdict

**APPROVED** (with recommended fixes)

The Engineer has produced a comprehensive, well-structured expansion of the Home view spec. The document follows the established patterns from `tui.md` and `discover.md`, provides detailed state machines, data models, and implementation guidance. While there are issues to address, none are blocking for approval.

---

### Critical Issues

None. The proposal is fundamentally sound and implementable.

---

### High Priority

#### H1. Keybinding Conflict: `r` Key Collision

**Location:** Section 5.1 Dashboard State, Section 8.1

**Issue:** The `r` key is used for "Recent files" in Home view, but `tui.md` Section 3.3 defines `r` as "Refresh / Reload" globally across all views.

**Evidence:**
- `tui.md` line 131: `| r | Refresh / Reload | All views |`
- `engineer.md` line 372: `| r | Recent files | Expand recent files panel |`
- `engineer.md` line 637: mentions "Manual refresh: `r` key (when not in recent files mode)" - acknowledging the conflict

**Resolution Options:**
1. Use `f` for "recent Files" instead (consistent with Files focus)
2. Use `R` (capital) for Recent files
3. Override `r` in Home view only and document the exception explicitly

**Recommendation:** Use `R` for Recent files. Capital letters for less common actions is a TUI convention.

---

#### H2. Missing EXIT_VIEW State Handling

**Location:** Section 4.2, 4.3

**Issue:** The `EXIT_VIEW` state appears in the state diagram but has no transitions OUT of it. What happens after EXIT_VIEW? Does it reset? Is it a terminal state?

**Evidence:**
```
| EXIT_VIEW | Navigating to another view | Press '1-4' or Enter |
```

No "From: EXIT_VIEW" row exists in the transitions table.

**Resolution:** Clarify that EXIT_VIEW triggers `ViewAction::Navigate(ViewId)` which is handled by the App layer, not the Home view. The Home view state should reset to DASHBOARD when `on_enter()` is called upon return.

---

#### H3. Inconsistent State Enum vs Dialog Pattern

**Location:** Section 6.1

**Issue:** The data model has both `state: HomeState` (which includes `ScanDialog`, `TestDialog`) AND `dialog: Option<HomeDialog>`. This is redundant and could lead to inconsistent state.

**Evidence:**
```rust
pub state: HomeState,           // Includes ScanDialog, TestDialog
...
pub dialog: Option<HomeDialog>, // Also tracks dialog state
```

If `state == HomeState::ScanDialog`, should `dialog` be `Some(HomeDialog::Scan(...))` or is it redundant?

**Resolution:** Choose ONE pattern:
- **Option A:** `HomeState` is the authoritative state, `HomeDialog` holds only the dialog-specific data when applicable (current approach seems to be this)
- **Option B:** `HomeState` is always `Dashboard`, dialogs are tracked separately via `dialog: Option<HomeDialog>`

Recommend Option A with explicit documentation that `dialog` contains the data WHEN `state` is `ScanDialog` or `TestDialog`.

---

### Medium Priority

#### M1. SQL Syntax Error in Section 1.2

**Location:** Section 1.2 Core Entities, Section 7 Data Sources

**Issue:** Invalid SQL shown for first-time detection.

**Evidence:**
```
**First-time detection:** `SELECT COUNT(*) FROM scout_sources = 0`
```

This is invalid SQL. Should be:
```sql
SELECT COUNT(*) FROM scout_sources
-- Then check if result = 0 in application code
```

Or if inline comparison is intended:
```sql
SELECT (SELECT COUNT(*) FROM scout_sources) = 0
```

---

#### M2. Query for Tagged Percentage is Incomplete

**Location:** Section 7, Data Sources table

**Issue:** The query shown is pseudocode, not valid SQL.

**Evidence:**
```
| Tagged % | `SELECT COUNT(*) WHERE tag IS NOT NULL / total * 100` | 5s |
```

**Should be:**
```sql
SELECT CAST(SUM(CASE WHEN tag IS NOT NULL THEN 1 ELSE 0 END) AS REAL) * 100 / COUNT(*)
FROM scout_files WHERE source_id = ?
```

Or simpler:
```sql
SELECT
    COUNT(CASE WHEN tag IS NOT NULL THEN 1 END) * 100 / COUNT(*) as tagged_pct
FROM scout_files
```

---

#### M3. Parser Count Query Inconsistency

**Location:** Section 7

**Issue:** The query counts distinct parser names, but the original stub (and typical usage) would count registered parser files.

**Evidence:**
```
| Parser count | `SELECT COUNT(DISTINCT name) FROM cf_parsers` | 5s |
```

The stub said: `| Parser count | Filesystem: ~/.casparian_flow/parsers/*.py |`

**Consideration:** The database approach is correct for the new architecture (parsers are registered, not just files on disk). However, ensure `cf_parsers` contains what users expect to see as "parser count" - unique parsers, not unique (name, version) combinations.

---

#### M4. Missing Loading State Transitions

**Location:** Section 4.3

**Issue:** LOADING state only has two outgoing transitions: "Data ready" and "Error". Both lead to DASHBOARD. But what if data is still loading and user presses a key?

**Resolution:** Add note that all key events are ignored in LOADING state, or that LOADING should be brief enough (<500ms per tui.md perf targets) that this is acceptable.

---

#### M5. Activity Log Table Assumption

**Location:** Section 7, GAP-ACTIVITY-001

**Issue:** The Engineer correctly identified this as a gap but didn't propose a concrete resolution. The review should confirm this is acceptable for Phase 1.

**Recommendation:** For Phase 1, derive activity from existing tables:
```sql
SELECT 'Job completed' as type, name, completed_at as ts
FROM cf_job_status WHERE status = 'complete'
UNION ALL
SELECT 'Source scanned' as type, path, scanned_at as ts
FROM scout_sources
ORDER BY ts DESC LIMIT 10
```

Add `activity_log` as a Phase 2 enhancement for richer activity tracking.

---

### Low Priority / Nits

#### L1. Inconsistent Unicode Indicators

**Location:** Section 3.3

**Issue:** Uses both ASCII (`*`) and Unicode (`✗`, `✓`, `●`) indicators. While tui.md allows this, consistency within a single view is preferred.

**Evidence:**
- Section 3.3 shows `●`, `○`, `↻`, `✗`, `⚠`
- These match tui.md Section 5.3, so this is actually fine.

**Status:** Not an issue upon closer inspection. Retracted.

---

#### L2. First-Time Banner Takes Over Tiles

**Location:** Section 3.5

**Issue:** The first-time banner "replaces" the tiles. This is documented as a trade-off, but consider showing empty tiles WITH the banner overlaid instead, to maintain spatial consistency.

**Evidence:**
```
First-time banner replaces the status tiles until user scans first source.
```

**Status:** Trade-off acknowledged. Acceptable as designed.

---

#### L3. Toast Duration Inconsistency

**Location:** Section 8.7, tui.md Section 9.2

**Issue:** Home spec says "Auto-dismiss success after 3 seconds", tui.md says "Auto-dismiss after 5s".

**Evidence:**
- `engineer.md`: `// Auto-dismiss success after 3 seconds`
- `tui.md` line 395: `Auto-dismiss after 5s (errors stay until dismissed).`

**Resolution:** Align with tui.md (5 seconds) or explicitly document the exception for Home view.

---

#### L4. ViewLink Structure Inconsistency

**Location:** Section 6.3

**Issue:** `ViewLink.context` is `Option<String>`, but in practice this should probably be more typed (job_id: Option<Uuid>, file_path: Option<PathBuf>, etc.).

**Evidence:**
```rust
pub context: Option<String>,  // e.g., job ID, file path
```

**Recommendation:** Consider an enum for type safety:
```rust
pub enum ViewContext {
    JobId(Uuid),
    FilePath(PathBuf),
    None,
}
```

This is a nice-to-have for implementation, not blocking.

---

#### L5. Revision History Minor Issue

**Location:** Section 9

**Issue:** Both entries show the same date "2026-01-12" for v0.1 and v1.0, which is technically correct (same day) but might confuse readers.

**Resolution:** Add time or keep as-is. Minor.

---

### Compression Opportunities

The following patterns from this spec could be extracted to `tui.md` for reuse:

1. **Dialog State Pattern** - The pattern of having a state enum include dialog states plus a separate dialog data struct is reusable. Could document in tui.md Section 7.

2. **First-Time Detection Pattern** - The "is_first_time" boolean and welcome banner pattern could be generalized for any view that needs onboarding.

3. **Refresh Strategy Pattern** - The 5-second automatic refresh with manual override and debouncing is applicable to other views. Consider adding to tui.md Section 7.

4. **Stats Tile Pattern** - The 2x2 grid of tiles with indicators is a reusable layout pattern.

---

### Summary

The Engineer's proposal expands the Home view from a 114-line stub to a comprehensive 819-line specification that:

**Strengths:**
- Follows tui.md patterns consistently
- Provides complete state machine with clear transitions
- Includes detailed Rust data models
- Documents trade-offs explicitly
- Self-identifies gaps (GAP-ACTIVITY-001, GAP-RECENT-001, GAP-TOAST-001, GAP-BACKFILL-001)
- Includes implementation notes with code examples
- Covers responsive behavior
- Defines all keybindings for all states

**Weaknesses:**
- One keybinding conflict (`r`) needs resolution
- Some SQL pseudocode needs correction
- Minor state model redundancy

**Overall Assessment:** This is a well-crafted specification that demonstrates understanding of the existing architecture and patterns. The identified issues are implementation details that can be resolved during development. The self-identified gaps show good engineering judgment.

**Recommendation:** Approve with the understanding that H1 (keybinding conflict) must be resolved before implementation begins. The Engineer should update the spec to:
1. Change `r` (Recent files) to `R` or another key
2. Clarify EXIT_VIEW state handling
3. Fix the SQL syntax issues

These can be addressed in a quick revision round without full re-review.

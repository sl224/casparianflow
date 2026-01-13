# Cross-Cutting Review: TUI View Specs

**Round:** 5 (Cross-Cutting)
**Date:** 2026-01-12
**Specs Analyzed:** tui.md, home.md, jobs.md, sources.md, extraction.md, discover.md, parser_bench.md

---

## 1. Navigation Graph

```
                              ┌─────────────────────────────────────────┐
                              │              HOME [0/H]                  │
                              │          (Navigation Hub)                │
                              └────┬────────┬────────┬────────┬─────────┘
                                   │        │        │        │
                             [1]   │   [2]  │   [3]  │   [4]  │
                                   ▼        ▼        ▼        ▼
                           ┌───────────┐┌───────────┐┌───────────┐┌───────────┐
                           │ DISCOVER  ││  PARSER   ││   JOBS    ││  SOURCES  │
                           │           ││   BENCH   ││           ││           │
                           └─────┬─────┘└───────────┘└───────────┘└─────┬─────┘
                                 │                                      │
                            [e]  │                                 [e] Edit rules
                                 │                                 (on class)
                                 ▼                                      │
                         ┌─────────────┐◄───────────────────────────────┘
                         │ EXTRACTION  │
                         │   RULES     │
                         └─────────────┘

Modal/Dialog States (accessible from parent view):
- Home: SCAN_DIALOG [s], TEST_DIALOG [t], RECENT_FILES [R]
- Discover: SOURCES_DROPDOWN [1], TAGS_DROPDOWN [2], RULES_MANAGER [R]
- Parser Bench: QUICK_TEST_PICKER [n], FILE_PICKER [t], RESULT_VIEW, BACKTEST
- Jobs: LOG_VIEWER [l], CONFIRM_DIALOG [c/x/u], FILTER_DIALOG [f]
- Sources: ADD_DIALOG [n], CLASS_MANAGER [c], CONFIRM_DIALOG [d], MOVE_DIALOG [m]
- Extraction: WIZARD [n], YAML_EDITOR [Y/y], TEST_MODE [t], DELETE_CONFIRM [d],
              PRIORITY_MODE [p], COVERAGE_PANEL [c]
```

---

## 2. Entry/Exit Matrix

| View | Entry From | Entry Key/Action | Entry Context Expected | Exit To | Exit Key |
|------|------------|------------------|------------------------|---------|----------|
| Home | Any view | `0` or `H` | None | Any primary view | `1-4` |
| Home | App startup | Auto | None | Any primary view | `1-4` |
| Discover | Any view | `1` | None (loads own state) | Home or other primary | `Esc`/`0`/`H`/`1-4` |
| Parser Bench | Any view | `2` | None (loads own state) | Home or other primary | `Esc`/`0`/`H`/`1-4` |
| Jobs | Any view | `3` | None (loads own state) | Home or other primary | `Esc`/`0`/`H`/`1-4` |
| Sources | Any view | `4` | None (loads own state) | Home or other primary | `Esc`/`0`/`H`/`1-4` |
| Extraction Rules | Discover | `e` on source/file | source_id, file context | Discover | `Esc` |
| Extraction Rules | Sources | `e` on source/class | source_id or class_id | Sources | `Esc` |

---

## 3. Navigation Issues

### XCUT-NAV-001: Extraction Rules Access Key Undefined (LOW)
- **Location:** `tui.md` vs `extraction.md`
- **Description:** Extraction Rules is a child view accessed via `e` from Discover/Sources, not a numbered key. This is correct but should be explicit in tui.md.
- **Impact:** Minor documentation gap.
- **Suggested Fix:** Add note in tui.md that Extraction is accessed via drill-down, not numbered key.

### XCUT-NAV-002: Discover Entry Key Documented as `D` (MEDIUM)
- **Location:** `discover.md` Section 2.1
- **Description:** Says "User enters Discover mode (press D from Home)" but tui.md defines `1` for Discover.
- **Impact:** Documentation conflict.
- **Suggested Fix:** Update discover.md to reference `1` instead of `D`.

### XCUT-NAV-003: Parser Bench Entry Documented as `Alt+P` (MEDIUM)
- **Location:** `parser_bench.md` Section 2.1
- **Description:** Shows `Alt+P` but tui.md defines `2` for Parser Bench.
- **Impact:** Documentation conflict.
- **Suggested Fix:** Remove `Alt+P`, use `2` consistently.

### XCUT-NAV-004: Home Quick Test vs Parser Bench Test Relationship (LOW)
- **Location:** `home.md` Section 2.5 vs `parser_bench.md`
- **Description:** Home has "Quick Test" dialog (`t`). Unclear if this navigates to Parser Bench or is standalone.
- **Impact:** User confusion about testing workflow.
- **Suggested Fix:** Clarify that Home Quick Test is shortcut dialog; results link to Parser Bench.

### XCUT-NAV-005: Dynamic Breadcrumb for Extraction Rules (LOW)
- **Location:** `extraction.md` Section 3.1
- **Description:** Breadcrumb shows "Home > Discover > Extraction Rules" but can also be accessed from Sources.
- **Impact:** Breadcrumb may not reflect actual navigation path.
- **Suggested Fix:** Document dynamic breadcrumb based on entry path.

### XCUT-NAV-006: Old Spec Files Should Redirect (LOW)
- **Location:** `specs/discover.md`, `specs/parser_bench.md`
- **Description:** Old files have relocation notices but still exist.
- **Impact:** Confusion about which files are authoritative.
- **Suggested Fix:** Either delete old files or add clear redirect.

---

## 4. Data Flow Issues

### XCUT-DATA-001: Extraction Rules Entry Context Undefined (LOW)
- **Location:** `extraction.md` vs `discover.md`
- **Description:** Extraction expects context (source_id) but Discover doesn't define what context is passed.
- **Impact:** Implementation may have mismatch.
- **Suggested Fix:** Add ViewContext definition showing what Discover passes.

### XCUT-DATA-002: Home Recent Files Navigate Context (LOW)
- **Location:** `home.md` Section 4.3
- **Description:** Recent Files passes `ViewContext::FilePath` but Discover's on_enter() expectations not defined.
- **Impact:** Navigation with context may fail if types mismatch.
- **Suggested Fix:** Ensure ViewLink struct matches View trait expectations.

### XCUT-DATA-003: Primary View Navigation Context (LOW)
- **Location:** `home.md` Section 8.3
- **Description:** tile_view_id() returns ViewId but context passing not documented.
- **Impact:** Minor - probably works since context is optional.
- **Suggested Fix:** Document that 1-4 navigation passes no context.

### XCUT-DATA-004: Extraction Rules Class Context Missing (MEDIUM)
- **Location:** `sources.md` vs `extraction.md`
- **Description:** Sources can edit class rules via `e`, but Extraction's data model has no class_id field.
- **Impact:** Class-level rule editing may not work.
- **Suggested Fix:** Add source_context: Option<SourceContext> to ExtractionViewState.

---

## 5. Pattern Audit

### Dialogs (14 instances)

| View | Dialog | Esc | Enter | Tab |
|------|--------|-----|-------|-----|
| Home | Scan Dialog | Cancel | Execute | Next field |
| Home | Test Dialog | Cancel | Run test | Next field |
| Home | Recent Files | Close | Navigate | Not mentioned |
| Discover | Rule Creation | Cancel | Create | Switch fields |
| Discover | Rules Manager | Close | Toggle | Not mentioned |
| Parser Bench | Quick Test Picker | Back | Select | Not mentioned |
| Parser Bench | File Picker | Back | Select | Not mentioned |
| Jobs | Log Viewer | Close | N/A | Not mentioned |
| Jobs | Confirm Dialog | Cancel | Execute | Switch button |
| Jobs | Filter Dialog | Cancel | Apply | Not mentioned |
| Sources | Add Dialog | Close | Submit | Not mentioned |
| Sources | Class Manager | Close | N/A | Not mentioned |
| Extraction | YAML Editor | Cancel | N/A (Ctrl+Enter) | Not mentioned |
| Extraction | Delete Confirm | Cancel | Delete | Not mentioned |

### Status Indicators

| Symbol | tui.md | home.md | jobs.md | sources.md | extraction.md |
|--------|--------|---------|---------|------------|---------------|
| `●` | Active/Healthy | Active | Running | Healthy | N/A |
| `○` | Inactive | Inactive | Queued | Stale | N/A |
| `↻` | In Progress | In Progress | N/A | Scanning | N/A |
| `✓` | N/A | N/A | Complete | N/A | Complete |
| `✗` | Error | Error | Failed | Error | Failed |
| `⚠` | Warning | Warning | N/A | Warning | Partial |

---

## 6. Pattern Inconsistencies

### XCUT-PAT-001: Confirmation Dialog Impact Display (LOW)
- **Location:** Multiple views
- **Description:** Sources/Extraction show impact summary, Jobs doesn't.
- **Suggested Fix:** Standardize: all destructive confirmations show impact.

### XCUT-PAT-002: Tab Behavior Not Consistently Documented (LOW)
- **Location:** Multiple views
- **Description:** Some dialogs specify Tab, others don't.
- **Suggested Fix:** Add Tab behavior to all dialog keybinding tables.

### XCUT-PAT-003: g/G (First/Last) Not Consistently Documented (LOW)
- **Location:** Various views
- **Description:** tui.md defines g/G but many views don't list it.
- **Suggested Fix:** Note "inherits global list navigation" in each view.

### XCUT-PAT-004: `n` Key Meaning Varies (LOW)
- **Location:** Various views
- **Description:** `n` = "new" in some views, "quick action" in others.
- **Suggested Fix:** Accept variance, document clearly per view.

### XCUT-PAT-005: Refresh Key `r` Not Documented in Jobs (LOW)
- **Location:** jobs.md
- **Description:** Uses `R` for Retry but doesn't mention `r` for refresh.
- **Suggested Fix:** Add `r` for refresh if supported.

### XCUT-PAT-006: Capital Letter Convention (LOW)
- **Location:** Multiple views
- **Description:** Capital letters used for view-specific extensions inconsistently.
- **Suggested Fix:** Document convention: capitals are view-specific.

### XCUT-PAT-007: Status Indicator `●` Conflict (HIGH)
- **Location:** tui.md vs jobs.md
- **Description:** `●` means "Complete" in tui.md but "Running" in jobs.md.
- **Impact:** UX confusion.
- **Suggested Fix:** Standardize: `✓` = Complete, `●` = Active/Healthy, `↻` = Running.

### XCUT-PAT-008: Test Mode Consistency (LOW)
- **Location:** parser_bench.md vs extraction.md
- **Description:** Both use `t` for test, slightly different sub-keys.
- **Suggested Fix:** None needed - similar enough.

---

## 7. Keybinding Issues

### XCUT-KEY-001: Global `n` Conflicts with Create Actions (HIGH)
- **Location:** tui.md Section 3.2 vs various views
- **Description:** `n` = "Next search result" globally, but also "Create new" in views.
- **Impact:** Key is overloaded.
- **Suggested Fix:** `n/N` for search only active after `/` initiated.

### XCUT-KEY-002: Discover Overrides 1/2/3 for Panels (MEDIUM)
- **Location:** discover.md Section 6.1
- **Description:** 1/2/3 focus panels instead of navigating views.
- **Impact:** Breaks global key consistency.
- **Suggested Fix:** Document override explicitly.

### XCUT-KEY-003: Jobs Log Viewer n/N Override (NONE)
- **Location:** jobs.md Section 5.2
- **Description:** Properly documented as vim-convention override.
- **Impact:** None - correct pattern.

### XCUT-KEY-004: Home `s` vs Discover `s` (NONE)
- **Description:** Both mean "scan" - consistent.

### XCUT-KEY-005: `c` Key Varies Across Views (MEDIUM)
- **Location:** extraction.md, jobs.md, sources.md
- **Description:** c = Coverage / Cancel / Class Manager.
- **Impact:** May confuse users.
- **Suggested Fix:** Accept as view-specific or find alternatives.

### XCUT-KEY-006: Parser Bench `r` Conflict (MEDIUM)
- **Location:** parser_bench.md
- **Description:** `r` = re-run in ResultView, conflicts with global refresh.
- **Suggested Fix:** Refresh only in ParserList, not ResultView.

### XCUT-KEY-007: Extraction/Sources `e` Consistency (LOW)
- **Description:** Both relate to editing - consistent enough.

### XCUT-KEY-008: `d` Delete Not Universal (LOW)
- **Location:** tui.md vs various views
- **Description:** `d` for delete only where applicable.
- **Suggested Fix:** Clarify tui.md: "d = Delete where applicable".

---

## 8. Summary

| Category | Total | Critical | High | Medium | Low |
|----------|-------|----------|------|--------|-----|
| Navigation (XCUT-NAV-*) | 6 | 0 | 0 | 2 | 4 |
| Data Flow (XCUT-DATA-*) | 4 | 0 | 0 | 1 | 3 |
| Pattern (XCUT-PAT-*) | 8 | 0 | 1 | 0 | 7 |
| Keybinding (XCUT-KEY-*) | 8 | 0 | 1 | 3 | 4 |
| **Total** | **26** | **0** | **2** | **6** | **18** |

### High Priority Issues (2)

1. **XCUT-PAT-007**: Status indicator `●` means "Complete" (tui.md) vs "Running" (jobs.md)
2. **XCUT-KEY-001**: Global `n` for search conflicts with create actions

### Recommended Fixes

1. **Standardize status indicators:**
   - `✓` = Complete/Success
   - `●` = Active/Healthy (not running)
   - `↻` = Running/In Progress
   - `○` = Pending/Inactive
   - `✗` = Failed/Error
   - `⚠` = Warning

2. **Clarify search navigation:**
   - `n/N` only active after `/` search initiated
   - Document in tui.md

3. **Fix entry key documentation:**
   - discover.md: `D` → `1`
   - parser_bench.md: `Alt+P` → `2`

4. **Document key override conventions:**
   - Views may override global keys contextually
   - Document overrides explicitly

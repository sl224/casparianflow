# Extraction Rules - TUI View Spec

**Status:** Draft
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.1
**Related:** specs/extraction.md (Extraction API), docs/decisions/ADR-017-tagging-vs-extraction-rules.md
**Last Updated:** 2026-01-14

> **Note:** For global keybindings, layout patterns, and common UI elements,
> see the master TUI spec at `specs/tui.md`.

---

## 1. Overview

The **Extraction Rules** view manages extraction rules - creating, editing, testing, and monitoring rule coverage. Users access this view by drilling down from the Discover view (press `e` on a source or file pattern) or via the Sources view when managing source-level rules.

### 1.1 Design Philosophy

- **Rules are visible**: Every rule shows its pattern, priority, and coverage at a glance
- **Testing is immediate**: See extraction results before committing changes
- **Tier 1 first**: Wizard-based creation is the default; YAML is the escape hatch
- **Priority is explicit**: Visual ordering matches execution priority
- **Coverage drives decisions**: Know how many files each rule affects

### 1.2 Core Entities

```
~/.casparian_flow/casparian_flow.sqlite3

Tables queried:
├── extraction_rules       # Rule definitions (id, name, glob, priority, tag)
├── extraction_fields      # Field extraction config per rule
├── scout_files            # File counts, extraction status
└── equivalence_classes    # Rule sharing across sources (optional)
```

**Schema Requirement:** This view requires database migrations that may not yet exist:
- Tables `extraction_rules` and `extraction_fields` (see specs/extraction.md Section 6)
- Columns in `scout_files`: `matched_rule_id`, `extraction_status`, `metadata_extracted`, `extraction_failures`
- Current `scout_files` has `rule_id` but not the extraction-specific columns

**Graceful Degradation:** If schema is not migrated, view displays a migration prompt instead of rule list (see Section 7.2).

### 1.3 User Goals

| Goal | How Extraction Rules Helps |
|------|----------------------------|
| "What rules exist?" | Rule list shows all rules with patterns and priorities |
| "How well is this rule working?" | Coverage stats show complete/partial/failed counts |
| "I need to extract metadata from these files" | Wizard infers patterns from example files |
| "This rule isn't matching correctly" | Test mode shows extraction against sample files |
| "I need fine control over extraction" | YAML editor provides full rule syntax |
| "Which rule wins when patterns overlap?" | Priority column; reorder with arrow keys |

---

## 2. User Workflows

### 2.1 Create Rule via Wizard (Tier 1)

Primary workflow for most users. The system infers patterns from example files.

```
1. User presses 'n' from rule list
2. Wizard dialog opens:
   ┌─ Create Extraction Rule ──────────────────────────────────────┐
   │ Step 1: Select Example Files                                   │
   │ ─────────────────────────────────────────────────────────────  │
   │                                                                │
   │ Path: /data/mission_042/2024-01-15/telemetry.csv█              │
   │                                                                │
   │ Selected files (3+ recommended for inference):                 │
   │   ✓ /data/mission_042/2024-01-15/telemetry.csv                 │
   │   ✓ /data/mission_043/2024-01-16/readings.csv                  │
   │   [ ] Add more files...                                        │
   │                                                                │
   │ [Enter] Continue  [Tab] Add file  [Esc] Cancel                 │
   └────────────────────────────────────────────────────────────────┘

3. User adds 1-3 example files (Tab adds more)
4. User presses Enter to continue
5. System runs inference, shows results:
   ┌─ Create Extraction Rule ──────────────────────────────────────┐
   │ Step 2: Review Detected Pattern                                │
   │ ─────────────────────────────────────────────────────────────  │
   │                                                                │
   │ Confidence: ████████████████░░░ 82%                            │
   │                                                                │
   │ Pattern: **/mission_*/????-??-??/*.csv                         │
   │                                                                │
   │ Extracted fields:                                              │
   │   mission_id: segment(-3) pattern="mission_(\\d+)"             │
   │   date:       segment(-2) type=date                            │
   │                                                                │
   │ Matches: 1,247 files                                           │
   │                                                                │
   │ [Enter] Accept  [e] Edit fields  [←] Back  [Esc] Cancel       │
   └────────────────────────────────────────────────────────────────┘

6. User reviews and presses Enter to accept, or 'e' to edit fields
7. Final step - naming and tagging:
   ┌─ Create Extraction Rule ──────────────────────────────────────┐
   │ Step 3: Name and Tag                                           │
   │ ─────────────────────────────────────────────────────────────  │
   │                                                                │
   │ Name: mission_telemetry█                                       │
   │ Tag:  mission_data                                             │
   │                                                                │
   │ Priority: 100 (default)                                        │
   │                                                                │
   │ [Enter] Create Rule  [←] Back  [Esc] Cancel                   │
   └────────────────────────────────────────────────────────────────┘

8. User fills name/tag, presses Enter
9. Rule created, toast shows: "✓ Created rule 'mission_telemetry'"
10. List refreshes with new rule selected
```

### 2.2 Create Rule via YAML (Tier 2)

For power users who want full control over rule syntax.

```
1. User presses 'Y' (Shift+y) from rule list
2. YAML editor opens with template:
   ┌─ New Rule - YAML Editor ──────────────────────────────────────┐
   │                                                                │
   │   1│ - name: "new_rule"                                        │
   │   2│   glob: "**/*"                                            │
   │   3│   extract:                                                │
   │   4│     field_name:                                           │
   │   5│       from: segment(-1)                                   │
   │   6│       pattern: "(.*)"                                     │
   │   7│   tag: "my_tag"                                           │
   │   8│   priority: 100                                           │
   │   9│                                                           │
   │                                                                │
   ├──────────────────────────────────────────────────────────────  │
   │ Validation: ✓ Valid                                            │
   │                                                                │
   │ [Ctrl+S] Validate  [Ctrl+Enter] Save  [Esc] Cancel            │
   └────────────────────────────────────────────────────────────────┘

3. User edits YAML
4. Ctrl+S validates (shows errors inline if invalid):
   │   2│   glob: "**/*["                                           │
   │                  ^─ Error: Invalid glob pattern                │

5. Ctrl+Enter saves and creates rule
6. If validation fails, error shown; save blocked
7. Esc cancels (confirmation dialog if changes made)
```

### 2.3 Test Rule Against Files

Verify that a rule extracts correctly before deploying.

```
1. User selects rule in list, presses 't'
2. Test panel expands below preview:
   ┌─ Test: mission_telemetry ─────────────────────────────────────┐
   │ Testing against 10 random matching files...                    │
   │                                                                │
   │ File                           mission_id  date        Status  │
   │ ──────────────────────────────────────────────────────────────│
   │ /data/mission_042/2024-01-15/t  042        2024-01-15   ✓     │
   │ /data/mission_043/2024-01-16/r  043        2024-01-16   ✓     │
   │ /data/mission_001/2024-01-01/d  001        2024-01-01   ✓     │
   │ /data/mission_invalid/foo/bar   (none)     (none)       ⚠     │
   │   └─ Segment -3 does not match pattern                         │
   │ ...                                                            │
   │                                                                │
   │ Summary: 9/10 passed (90%)                                     │
   │                                                                │
   │ [t] Test more  [f] Test failed only  [Esc] Close test         │
   └────────────────────────────────────────────────────────────────┘

3. User sees extraction results per file
4. Press 't' to test another batch of random files
5. Press 'f' to specifically test files that previously failed
6. Esc closes test panel
```

### 2.4 Edit Existing Rule

Modify an existing rule's pattern, fields, or metadata.

```
1. User selects rule, presses 'e'
2. Edit wizard opens (same as create, but pre-populated):
   ┌─ Edit Rule: mission_telemetry ────────────────────────────────┐
   │                                                                │
   │ Pattern: **/mission_*/????-??-??/*.csv█                        │
   │                                                                │
   │ Fields:                                                        │
   │   mission_id: segment(-3) pattern="mission_(\\d+)"  [x]       │
   │   date:       segment(-2) type=date                 [x]       │
   │   [+] Add field                                                │
   │                                                                │
   │ Tag: mission_data                                              │
   │ Priority: 100                                                  │
   │                                                                │
   │ [Enter] Save  [y] Edit as YAML  [Esc] Cancel                  │
   └────────────────────────────────────────────────────────────────┘

3. User edits fields, presses Enter to save
4. Changes trigger re-extraction for affected files (background job)
5. Toast: "✓ Updated rule 'mission_telemetry' - re-extracting 1,247 files"
```

### 2.5 Edit Rule as YAML

Switch existing rule to YAML editor for advanced editing.

```
1. User selects rule, presses 'y' (lowercase)
2. YAML editor opens with rule's current configuration:
   ┌─ Edit: mission_telemetry - YAML ──────────────────────────────┐
   │                                                                │
   │   1│ - name: "mission_telemetry"                               │
   │   2│   glob: "**/mission_*/????-??-??/*.csv"                   │
   │   3│   extract:                                                │
   │   4│     mission_id:                                           │
   │   5│       from: segment(-3)                                   │
   │   6│       pattern: "mission_(\\d+)"                           │
   │   7│       type: integer                                       │
   │   8│     date:                                                 │
   │   9│       from: segment(-2)                                   │
   │  10│       type: date                                          │
   │  11│   tag: "mission_data"                                     │
   │  12│   priority: 100                                           │
   │                                                                │
   ├──────────────────────────────────────────────────────────────  │
   │ Validation: ✓ Valid                                            │
   │                                                                │
   │ [Ctrl+S] Validate  [Ctrl+Enter] Save  [Esc] Cancel            │
   └────────────────────────────────────────────────────────────────┘

3. User edits, Ctrl+Enter saves
4. If glob/extract changed, re-extraction triggered
```

### 2.6 Delete Rule

Remove a rule and clear its metadata from files.

```
1. User selects rule, presses 'd'
2. Confirmation dialog:
   ┌─ Delete Rule ─────────────────────────────────────────────────┐
   │                                                                │
   │   Delete "mission_telemetry"?                                  │
   │                                                                │
   │   This will:                                                   │
   │   • Remove the rule definition                                 │
   │   • Clear extracted metadata from 1,247 files                  │
   │   • Files will remain (only metadata removed)                  │
   │                                                                │
   │   [Enter] Delete  [Esc] Cancel                                │
   └────────────────────────────────────────────────────────────────┘

3. User confirms with Enter
4. Rule deleted, files' metadata cleared
5. Toast: "✓ Deleted rule 'mission_telemetry'"
```

### 2.7 Change Rule Priority

Reorder rules to control which wins when patterns overlap.

```
1. User selects rule, presses 'p' to enter priority mode
2. Rule row shows priority controls:
   │ ▶ mission_data      P:100  │  <- Selected rule
   │   [↑/↓] Move  [Enter] Set value  [Esc] Done                   │

3. User presses ↑/↓ to swap with adjacent rules
4. Or presses Enter to type exact priority value:
   │ Priority: 95█                                                  │

5. Esc exits priority mode
6. Changes saved immediately
```

### 2.8 View Coverage Report

See detailed statistics about rule effectiveness.

```
1. User selects rule, presses 'c'
2. Coverage panel expands:
   ┌─ Coverage: mission_telemetry ─────────────────────────────────┐
   │                                                                │
   │ Total matching files: 1,247                                    │
   │                                                                │
   │ Extraction Status:                                             │
   │   ████████████████████████░░░░                                 │
   │   ✓ Complete: 1,189 (95.4%)                                    │
   │   ⚠ Partial:    45 (3.6%)   <- Some fields failed              │
   │   ✗ Failed:     13 (1.0%)   <- All fields failed               │
   │                                                                │
   │ Common failures:                                               │
   │   • "Segment -3 does not match pattern" (8 files)              │
   │   • "Date parse failed" (5 files)                              │
   │                                                                │
   │ [f] View failed files  [Esc] Close                            │
   └────────────────────────────────────────────────────────────────┘

3. Press 'f' to navigate to Discover filtered to failed files
4. Esc closes coverage panel
```

### 2.9 Manage Rule Conflicts

When multiple rules match the same files, resolve conflicts.

```
1. System detects overlapping rules during save:
   ┌─ Rule Conflict Detected ──────────────────────────────────────┐
   │                                                                │
   │ "new_rule" (P:100) overlaps with:                              │
   │                                                                │
   │   "mission_telemetry" (P:100) - 847 files in common            │
   │   "log_parser" (P:90) - 12 files in common                     │
   │                                                                │
   │ With equal priority, "mission_telemetry" wins (alphabetically) │
   │                                                                │
   │ Recommendations:                                               │
   │   • Set different priorities to control match order            │
   │   • Make glob patterns more specific                           │
   │                                                                │
   │ [Enter] Save anyway  [p] Adjust priority  [Esc] Cancel        │
   └────────────────────────────────────────────────────────────────┘

2. User can adjust priority inline or proceed
```

---

## 3. Layout Specification

### 3.1 Full Layout

```
┌─ Casparian Flow ────────────────────────────────────────────────────────────┐
│ Home > Discover > Extraction Rules                               [?] Help   │
├─ Rules ──────────────────────────┬─ Preview ────────────────────────────────┤
│                                  │                                          │
│ ▶ mission_data         P:100    │  Rule: mission_data                       │
│   glob: **/mission_*/**         │  ──────────────────────────────────────── │
│   ✓ 1,247 files                 │                                          │
│                                  │  Glob: **/mission_*/????-??-??/*         │
│   sales_reports        P:90     │                                          │
│   glob: **/sales/**/*.csv       │  Extract:                                │
│   ✓ 89 files                    │    mission_id: segment(-3) "mission_(\d+)"│
│                                  │                type: integer             │
│   log_files            P:50     │    date:       segment(-2)               │
│   glob: **/*.log                │                type: date                │
│   ⚠ 34 files (12 partial)       │                                          │
│                                  │  Tag: mission_data                       │
│   healthcare_hl7       P:40     │                                          │
│   glob: **/*_Inbound/**/*.hl7   │  Coverage:                               │
│   ✓ 2,341 files                 │    ✓ Complete: 1,189 (95%)               │
│                                  │    ⚠ Partial: 45 (4%)                    │
│   [+] Create new rule           │    ✗ Failed: 13 (1%)                     │
│                                  │                                          │
│                                  │  Created: 2026-01-10 by inference        │
│                                  │  Last tested: 2026-01-12                 │
│                                  │                                          │
├──────────────────────────────────┴──────────────────────────────────────────┤
│ [n] New rule  [e] Edit  [t] Test  [y] YAML  [d] Delete  [p] Priority  [?]   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Breakdown

| Component | Height | Width | Purpose |
|-----------|--------|-------|---------|
| Header | 1 line | 100% | Breadcrumb, help hint |
| Rules panel | Variable (min 10) | 35% | Scrollable rule list |
| Preview panel | Variable (min 10) | 65% | Selected rule details |
| Footer | 1 line | 100% | Context-sensitive keybindings |

### 3.3 Rule List Entry Format

Each rule in the list follows this pattern:

```
│ [selector] rule_name        P:priority │
│   glob: pattern_preview               │
│   status file_count                   │
```

**Field details:**
| Field | Description |
|-------|-------------|
| `selector` | `▶` for selected, ` ` (space) for unselected |
| `rule_name` | Truncated to 20 chars, full name in preview |
| `P:priority` | Priority value (lower = higher precedence) |
| `glob` | Pattern truncated to fit, full in preview |
| `status` | `✓` complete, `⚠` partial issues, `✗` failures |
| `file_count` | Count with annotation if partial/failed |

**Status indicator rules:**
| Status | Icon | Condition |
|--------|------|-----------|
| Healthy | `✓` | 100% complete extraction |
| Warning | `⚠` | Any partial extractions |
| Error | `✗` | Any complete failures |

### 3.4 Preview Panel Sections

| Section | Content |
|---------|---------|
| Header | Rule name |
| Glob | Full glob pattern |
| Extract | Field definitions with source and type |
| Tag | Tag applied to matching files |
| Coverage | Visual bar + stats |
| Metadata | Created date, creator type, last tested |

### 3.5 Coverage Visualization

```
Coverage:
  ██████████████████████░░░░░░
  ✓ Complete: 1,189 (95%)
  ⚠ Partial: 45 (4%)
  ✗ Failed: 13 (1%)
```

Bar segments:
- Green portion: Complete percentage
- Yellow portion: Partial percentage
- Red portion: Failed percentage
- Gray: Remaining (should be 0%)

### 3.6 Test Panel Layout

When test mode is active, replaces bottom half of preview:

```
├─ Preview ────────────────────────────────────────────────────────┤
│  Rule: mission_data                                              │
│  Glob: **/mission_*/????-??-??/*                                 │
│                                                                  │
├─ Test Results ───────────────────────────────────────────────────┤
│ File                          mission_id  date        Status     │
│ ────────────────────────────────────────────────────────────────│
│ /data/mission_042/.../tel.csv  042        2024-01-15   ✓        │
│ /data/mission_043/.../rea.csv  043        2024-01-16   ✓        │
│ /data/mission_invalid/foo/bar  (none)     (none)       ⚠        │
│   └─ Segment -3 does not match pattern                          │
│                                                                  │
│ Summary: 9/10 passed (90%)                                       │
│                                                                  │
│ [t] Test more  [f] Test failed  [Enter] View file  [Esc] Close  │
└──────────────────────────────────────────────────────────────────┘
```

### 3.7 YAML Editor Layout

Full-screen modal overlay:

```
┌─ Edit: mission_telemetry - YAML ─────────────────────────────────────────────┐
│                                                                              │
│  1│ - name: "mission_telemetry"                                              │
│  2│   glob: "**/mission_*/????-??-??/*.csv"                                  │
│  3│   extract:                                                               │
│  4│     mission_id:                                                          │
│  5│       from: segment(-3)                                                  │
│  6│       pattern: "mission_(\\d+)"                                          │
│  7│       type: integer                                                      │
│  8│     date:                                                                │
│  9│       from: segment(-2)                                                  │
│ 10│       type: date                                                         │
│ 11│   tag: "mission_data"                                                    │
│ 12│   priority: 100                                                          │
│ 13│█                                                                         │
│                                                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ Line 13, Col 1                          Validation: ✓ Valid YAML             │
│                                                                              │
│ [Ctrl+S] Validate  [Ctrl+Enter] Save and Close  [Esc] Cancel                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.8 Responsive Behavior

| Terminal Width | Adaptation |
|----------------|------------|
| >= 120 cols | Full layout as shown |
| 100-119 cols | Narrower preview, shorter field descriptions |
| 80-99 cols | Rule list shows name + priority only; preview stacked below |
| < 80 cols | Single column view, Tab switches between list and preview |

| Terminal Height | Adaptation |
|-----------------|------------|
| >= 30 rows | Full layout with coverage details |
| 20-29 rows | Collapsed coverage (single line) |
| < 20 rows | Rule list only; Enter shows preview in popup |

---

## 4. State Machine

### 4.1 State Diagram

```
                              ┌─────────────┐
                              │   LOADING   │
                              └──────┬──────┘
                                     │ Data loaded
                                     ▼
                    ┌────────────────────────────────────────────┐
                    │                                            │
                    │               RULE_LIST                    │◄────────┐
                    │         (default view state)               │         │
                    │                                            │         │
                    └──┬────┬────┬────┬────┬────┬────┬────┬────┬─┘         │
                       │    │    │    │    │    │    │    │    │           │
                   'n' │'Y' │'e' │'y' │'t' │'d' │'p' │'c' │Esc │           │
                       │    │    │    │    │    │    │    │    │           │
                       ▼    │    ▼    │    ▼    │    │    │    │           │
               ┌───────────┐│┌───────┐│┌───────┐│    │    │    │           │
               │  WIZARD   │││ EDIT  │││ TEST  ││    │    │    │           │
               │  (new)    │││WIZARD │││ MODE  ││    │    │    │           │
               └─────┬─────┘│└───┬───┘│└───┬───┘│    │    │    │           │
                     │      │    │    │    │    │    │    │    │           │
               Done/ │      │Done│    │    │Esc │    │    │    │           │
               Esc   │      │Esc │    │    │    │    │    │    │           │
                     │      │    │    │    │    │    │    │    │           │
                     └──────┴────┴────┴────┴────┴────┴────┴────┴───────────┘
                            │         │              │    │    │
                            ▼         ▼              ▼    ▼    ▼
                    ┌───────────┐┌───────────┐┌──────────┐│┌──────────┐
                    │YAML_EDITOR││YAML_EDITOR││ DELETE   ││ COVERAGE │
                    │  (new)    ││  (edit)   ││ CONFIRM  ││  PANEL   │
                    └─────┬─────┘└─────┬─────┘└────┬─────┘│└────┬─────┘
                          │            │           │      │     │
                    Save/ │      Save/ │     Enter/│      │ Esc │
                    Esc   │      Esc   │     Esc   │      │     │
                          │            │           │      │     │
                          └────────────┴───────────┴──────┴─────┘
                                       │                  │
                                       │              ┌───┴───┐
                                       │              │PRIORITY│
                                       │              │ MODE  │
                                       │              └───┬───┘
                                       │                  │
                                       └────────── Esc ───┘
```

### 4.2 State Definitions

| State | Description | Entry Condition |
|-------|-------------|-----------------|
| LOADING | Fetching rules from database | View initialized |
| SCHEMA_MISSING | Migration required prompt | Schema check failed |
| RULE_LIST | Main browsing state | Data loaded, default |
| WIZARD | Multi-step rule creation wizard | Press 'n' |
| YAML_EDITOR_NEW | YAML editor for new rule | Press 'Y' (Shift+y) |
| EDIT_WIZARD | Edit existing rule wizard | Press 'e' |
| YAML_EDITOR_EDIT | YAML editor for existing rule | Press 'y' on selected rule |
| TEST_MODE | Testing rule against files | Press 't' |
| DELETE_CONFIRM | Confirmation dialog for deletion | Press 'd' |
| PRIORITY_MODE | Inline priority editing | Press 'p' |
| COVERAGE_PANEL | Expanded coverage statistics | Press 'c' |

**Note:** YAML_EDITOR states are modal overlays that capture all input. They contain nested states for cursor position, validation status, and modification tracking.

### 4.3 State Transitions

| From | Event | To | Side Effects |
|------|-------|-----|--------------|
| LOADING | Data ready | RULE_LIST | Render list |
| LOADING | Schema missing | SCHEMA_MISSING | Show migration prompt |
| LOADING | Error | RULE_LIST | Show error toast |
| SCHEMA_MISSING | Esc | RULE_LIST | Navigate back (to parent view) |
| RULE_LIST | 'n' pressed | WIZARD | Open wizard step 1 |
| RULE_LIST | 'Y' pressed | YAML_EDITOR_NEW | Open editor with template |
| RULE_LIST | 'e' pressed | EDIT_WIZARD | Open wizard with rule data |
| RULE_LIST | 'y' pressed | YAML_EDITOR_EDIT | Open editor with rule YAML |
| RULE_LIST | 't' pressed | TEST_MODE | Run tests, show panel |
| RULE_LIST | 'd' pressed | DELETE_CONFIRM | Show confirmation |
| RULE_LIST | 'p' pressed | PRIORITY_MODE | Enable priority controls |
| RULE_LIST | 'c' pressed | COVERAGE_PANEL | Expand coverage |
| RULE_LIST | Esc | RULE_LIST | Navigate back (to Discover) |
| WIZARD | Complete | RULE_LIST | Create rule, refresh, show toast |
| WIZARD | Esc | RULE_LIST | Discard, no changes |
| YAML_EDITOR_* | Ctrl+Enter (valid) | RULE_LIST | Save rule, refresh |
| YAML_EDITOR_* | Ctrl+Enter (invalid) | YAML_EDITOR_* | Show error, stay |
| YAML_EDITOR_* | Esc (no changes) | RULE_LIST | Close |
| YAML_EDITOR_* | Esc (changes) | YAML_EDITOR_* | Show confirm dialog |
| TEST_MODE | Esc | RULE_LIST | Close test panel |
| TEST_MODE | 't' | TEST_MODE | Run more tests |
| TEST_MODE | 'f' | TEST_MODE | Test failed files |
| DELETE_CONFIRM | Enter | RULE_LIST | Delete rule, refresh, toast |
| DELETE_CONFIRM | Esc | RULE_LIST | Cancel |
| PRIORITY_MODE | Esc | RULE_LIST | Save priority changes |
| PRIORITY_MODE | ↑/↓ | PRIORITY_MODE | Swap rule position |
| COVERAGE_PANEL | Esc | RULE_LIST | Close coverage |
| COVERAGE_PANEL | 'f' | RULE_LIST | Navigate to Discover with filter |

### 4.4 YAML Editor Nested States

The YAML editor has internal states:

```rust
pub struct YamlEditorState {
    pub content: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
    pub validation: ValidationState,
    pub is_modified: bool,
    pub original_content: String,  // For change detection
}

pub enum ValidationState {
    Valid,
    Invalid { line: usize, message: String },
    Pending,  // Validation in progress
}
```

**State invariants:**
- `is_modified = (content != original_content)`
- `validation` is recalculated on every edit (debounced 300ms)
- Ctrl+Enter only proceeds if `validation == Valid`

---

## 5. View-Specific Keybindings

> **Note:** Global keybindings (1-4, 0, H, ?, q, Esc) are defined in `specs/tui.md`.
> These are additional keybindings specific to the Extraction Rules view.

### 5.1 Rule List State

| Key | Action | Description |
|-----|--------|-------------|
| `n` | New rule (wizard) | Open wizard to create rule |
| `Y` | New rule (YAML) | Open YAML editor for new rule |
| `e` | Edit rule (wizard) | Open wizard to edit selected |
| `y` | Edit rule (YAML) | Open YAML editor for selected |
| `t` | Test rule | Run extraction test against sample files |
| `d` | Delete rule | Delete selected rule (with confirmation) |
| `p` | Priority mode | Enter priority editing mode |
| `c` | Coverage report | Show detailed coverage statistics |
| `j` / `↓` | Next rule | Move selection down |
| `k` / `↑` | Previous rule | Move selection up |
| `g` | First rule | Jump to first rule |
| `G` | Last rule | Jump to last rule |
| `/` | Filter rules | Filter by name or pattern |
| `Enter` | Expand/collapse | Toggle rule detail expansion |
| `Tab` | Switch panel | Move focus between list and preview |

### 5.2 Wizard State (All Steps)

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Next field | Move to next input field |
| `Shift+Tab` | Previous field | Move to previous input field |
| `Enter` | Continue/Submit | Next step or create rule |
| `←` | Previous step | Go back one step |
| `Esc` | Cancel | Close wizard (confirm if changes) |
| `y` | Switch to YAML | Convert current wizard state to YAML |

### 5.3 YAML Editor State

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+S` | Validate | Validate YAML syntax and schema |
| `Ctrl+Enter` | Save | Save rule and close editor |
| `Esc` | Cancel | Close (confirm if modified) |
| `Ctrl+Z` | Undo | Undo last change |
| `Ctrl+Shift+Z` | Redo | Redo last undone change |
| `Ctrl+A` | Select all | Select entire content |
| `↑/↓` | Move cursor | Navigate lines |
| `←/→` | Move cursor | Navigate characters |
| `Home` | Line start | Move to start of line |
| `End` | Line end | Move to end of line |
| `Ctrl+Home` | Document start | Jump to beginning |
| `Ctrl+End` | Document end | Jump to end |
| `PgUp/PgDn` | Page scroll | Scroll by page |

### 5.4 Test Mode State

| Key | Action | Description |
|-----|--------|-------------|
| `t` | Test more | Run against another batch of files |
| `f` | Test failed | Re-test only previously failed files |
| `j` / `↓` | Next result | Move through test results |
| `k` / `↑` | Previous result | Move through test results |
| `Enter` | View file | Open file in Discover view |
| `Esc` | Close test | Return to rule list |

### 5.5 Priority Mode State

| Key | Action | Description |
|-----|--------|-------------|
| `↑` / `k` | Move up | Swap rule with one above (lower priority) |
| `↓` / `j` | Move down | Swap rule with one below (higher priority) |
| `Enter` | Set exact | Type exact priority value |
| `Esc` | Done | Exit priority mode, save changes |

### 5.6 Coverage Panel State

| Key | Action | Description |
|-----|--------|-------------|
| `f` | View failed | Navigate to Discover with failed files filter |
| `p` | View partial | Navigate to Discover with partial files filter |
| `Esc` | Close | Return to rule list |

---

## 6. Data Model

### 6.1 View State

```rust
/// Main state for the Extraction Rules view
pub struct ExtractionViewState {
    /// Current UI state
    pub state: ExtractionState,

    /// List of all rules
    pub rules: Vec<RuleInfo>,

    /// Currently selected rule index
    pub selected_index: usize,

    /// Scroll offset for rule list
    pub list_scroll: usize,

    /// Expanded rules (showing inline details)
    pub expanded_rules: HashSet<Uuid>,

    /// Filter text for rule search
    pub filter_text: String,

    /// Filtered rule indices
    pub filtered_indices: Vec<usize>,

    /// Which panel has focus
    pub focused_panel: ExtractionPanel,

    /// Test results (only valid in TEST_MODE)
    /// Invariant: test_results.is_some() iff state == TestMode
    pub test_results: Option<TestResults>,

    /// Wizard state (only valid in WIZARD or EDIT_WIZARD)
    /// Invariant: wizard.is_some() iff state in {Wizard, EditWizard}
    pub wizard: Option<WizardState>,

    /// YAML editor state (only valid in YAML_EDITOR_*)
    /// Invariant: yaml_editor.is_some() iff state in {YamlEditorNew, YamlEditorEdit}
    pub yaml_editor: Option<YamlEditorState>,

    /// Coverage data (only valid in COVERAGE_PANEL)
    /// Invariant: coverage_data.is_some() iff state == CoveragePanel
    pub coverage_data: Option<CoverageStats>,

    /// Priority mode active rule
    pub priority_editing: Option<Uuid>,

    /// Last refresh timestamp
    pub last_refresh: DateTime<Utc>,

    /// Error state for display
    pub error: Option<String>,

    /// Entry context: where we came from (for filtering and breadcrumb)
    /// Set via on_enter() when navigating from Sources or Discover
    pub entry_context: Option<EntryContext>,
}

/// Context passed when entering Extraction Rules from another view
#[derive(Debug, Clone)]
pub enum EntryContext {
    /// Entered from Discover view, scoped to a source
    FromDiscover { source_id: Uuid },
    /// Entered from Sources view, scoped to a source
    FromSource { source_id: Uuid },
    /// Entered from Sources view, scoped to an equivalence class
    FromClass { class_id: Uuid, class_name: String },
}

/// UI state enum
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractionState {
    Loading,
    SchemaMissing,  // Migration required
    RuleList,
    Wizard,
    YamlEditorNew,
    EditWizard,
    YamlEditorEdit,
    TestMode,
    DeleteConfirm,
    PriorityMode,
    CoveragePanel,
}

/// Which panel has focus
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtractionPanel {
    RuleList,
    Preview,
    TestResults,
}
```

### 6.2 RuleInfo Struct

```rust
/// Summary information about an extraction rule
#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: Uuid,
    pub name: String,
    pub glob_pattern: String,
    pub tag: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub created_by: RuleCreator,
    pub created_at: DateTime<Utc>,

    /// Extraction fields defined for this rule
    pub fields: Vec<FieldInfo>,

    /// Coverage statistics
    pub coverage: CoverageSummary,
}

/// How the rule was created
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleCreator {
    Template,   // From built-in template
    Inferred,   // From algorithmic inference
    Manual,     // User-written YAML
}

/// Field extraction configuration
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub source_type: FieldSource,
    pub source_value: Option<String>,
    pub pattern: Option<String>,
    pub type_hint: FieldType,
    pub normalizer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldSource {
    Segment,    // segment(-N)
    Filename,   // filename
    FullPath,   // full_path
    RelPath,    // rel_path
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldType {
    String,
    Integer,
    Date,
    Uuid,
}

/// Brief coverage summary for list display
#[derive(Debug, Clone, Default)]
pub struct CoverageSummary {
    pub total_files: u32,
    pub complete: u32,
    pub partial: u32,
    pub failed: u32,
}

impl CoverageSummary {
    pub fn status(&self) -> CoverageStatus {
        if self.failed > 0 {
            CoverageStatus::HasFailures
        } else if self.partial > 0 {
            CoverageStatus::HasPartial
        } else {
            CoverageStatus::AllComplete
        }
    }

    pub fn complete_percent(&self) -> u8 {
        if self.total_files == 0 { 100 }
        else { ((self.complete as f32 / self.total_files as f32) * 100.0) as u8 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoverageStatus {
    AllComplete,
    HasPartial,
    HasFailures,
}
```

### 6.3 CoverageStats Struct

```rust
/// Detailed coverage statistics for coverage panel
#[derive(Debug, Clone)]
pub struct CoverageStats {
    pub rule_id: Uuid,
    pub rule_name: String,

    /// Counts
    pub total_files: u32,
    pub complete_count: u32,
    pub partial_count: u32,
    pub failed_count: u32,

    /// Common failure reasons
    pub failure_reasons: Vec<FailureReason>,

    /// Sample files for each status
    pub complete_samples: Vec<PathBuf>,
    pub partial_samples: Vec<PathBuf>,
    pub failed_samples: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FailureReason {
    pub message: String,
    pub count: u32,
    pub example_files: Vec<PathBuf>,
}
```

### 6.4 TestResults Struct

```rust
/// Results from testing a rule against files
#[derive(Debug, Clone)]
pub struct TestResults {
    pub rule_id: Uuid,
    pub rule_name: String,
    pub samples: Vec<TestSample>,
    pub run_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TestSample {
    pub path: PathBuf,
    pub status: ExtractionStatus,
    /// Extracted field values (field_name -> value)
    pub extracted: HashMap<String, String>,
    /// Errors for failed/partial extractions
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtractionStatus {
    Complete,   // All fields extracted
    Partial,    // Some fields failed
    Failed,     // All fields failed
}

impl TestResults {
    pub fn pass_count(&self) -> usize {
        self.samples.iter().filter(|s| s.status == ExtractionStatus::Complete).count()
    }

    pub fn pass_rate(&self) -> f32 {
        if self.samples.is_empty() { 0.0 }
        else { self.pass_count() as f32 / self.samples.len() as f32 }
    }
}
```

### 6.5 WizardState Struct

```rust
/// State for the rule creation/edit wizard
#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: WizardStep,
    pub mode: WizardMode,

    // Step 1: File selection
    pub selected_files: Vec<PathBuf>,
    pub file_input: String,
    pub file_input_cursor: usize,

    // Step 2: Pattern review
    pub inferred_pattern: Option<InferredPattern>,
    pub confidence: f32,
    pub match_count: u32,

    // Step 3: Name and tag
    pub name: String,
    pub name_cursor: usize,
    pub tag: String,
    pub tag_cursor: usize,
    pub priority: i32,

    // For edit mode: original rule ID
    pub editing_rule_id: Option<Uuid>,

    pub focused_field: WizardField,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardStep {
    SelectFiles,
    ReviewPattern,
    NameAndTag,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardMode {
    Create,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardField {
    FileInput,
    FileList,
    PatternEdit,
    FieldEdit,
    NameInput,
    TagInput,
    PriorityInput,
}

#[derive(Debug, Clone)]
pub struct InferredPattern {
    pub glob: String,
    pub fields: Vec<FieldInfo>,
}
```

### 6.6 YamlEditorState Struct

```rust
/// State for the YAML editor modal
#[derive(Debug, Clone)]
pub struct YamlEditorState {
    /// Current editor content
    pub content: String,

    /// Original content (for change detection)
    pub original_content: String,

    /// Cursor position
    pub cursor_line: usize,
    pub cursor_col: usize,

    /// Scroll position
    pub scroll_offset: usize,
    pub horizontal_scroll: usize,

    /// Current validation state
    pub validation: YamlValidation,

    /// Undo/redo stacks
    pub undo_stack: Vec<String>,
    pub redo_stack: Vec<String>,

    /// For edit mode: original rule ID
    pub editing_rule_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub enum YamlValidation {
    Valid,
    Invalid { line: usize, col: usize, message: String },
    Pending,
}

impl YamlEditorState {
    pub fn is_modified(&self) -> bool {
        self.content != self.original_content
    }

    pub fn can_save(&self) -> bool {
        matches!(self.validation, YamlValidation::Valid) && self.is_modified()
    }
}
```

---

## 7. Data Sources

### 7.1 Main Queries

| Widget | Query | Refresh |
|--------|-------|---------|
| Rule list | See below | 5s or on change |
| Coverage summary | See below | With rule list |
| Test samples | See below | On demand |
| Coverage details | See below | On demand |

**Rule list with coverage query:**
```sql
SELECT
    r.id,
    r.name,
    r.glob_pattern,
    r.tag,
    r.priority,
    r.enabled,
    r.created_by,
    r.created_at,
    COUNT(f.id) as total_files,
    SUM(CASE WHEN f.extraction_status = 'COMPLETE' THEN 1 ELSE 0 END) as complete,
    SUM(CASE WHEN f.extraction_status = 'PARTIAL' THEN 1 ELSE 0 END) as partial,
    SUM(CASE WHEN f.extraction_status = 'FAILED' THEN 1 ELSE 0 END) as failed
FROM extraction_rules r
LEFT JOIN scout_files f ON f.matched_rule_id = r.id
WHERE r.source_id = ?  -- Current source filter, if any
GROUP BY r.id
ORDER BY r.priority ASC, r.name ASC;
```

**Fields for a rule:**
```sql
SELECT
    field_name,
    source_type,
    source_value,
    pattern,
    type_hint,
    normalizer,
    default_value
FROM extraction_fields
WHERE rule_id = ?
ORDER BY field_name ASC;
```

**Test sample files:**
```sql
SELECT path, extraction_status, metadata_extracted, extraction_failures
FROM scout_files
WHERE matched_rule_id = ?
ORDER BY RANDOM()
LIMIT 10;
```

**Failed files only:**
```sql
SELECT path, extraction_status, metadata_extracted, extraction_failures
FROM scout_files
WHERE matched_rule_id = ?
  AND extraction_status IN ('FAILED', 'PARTIAL')
ORDER BY extraction_status DESC, path ASC
LIMIT 10;
```

**Coverage details with failure reasons:**
```sql
SELECT
    json_extract(value, '$.reason') as reason,
    COUNT(*) as count
FROM scout_files, json_each(scout_files.extraction_failures)
WHERE matched_rule_id = ?
GROUP BY json_extract(value, '$.reason')
ORDER BY count DESC
LIMIT 10;
```

### 7.2 Schema Requirements

The following tables and columns are required (per specs/extraction.md Section 6):

| Table | Required Columns | Notes |
|-------|------------------|-------|
| `extraction_rules` | id, source_id, name, glob_pattern, semantic_source, tag, priority, enabled, created_by, created_at | Core rule definition |
| `extraction_fields` | id, rule_id, field_name, source_type, source_value, pattern, type_hint, normalizer, default_value | Field extraction config |
| `extraction_tag_conditions` | id, rule_id, field_name, operator, value, tag, priority | Conditional tagging (v1.2) |
| `extraction_field_values` | id, rule_id, field_name, field_value, file_count, last_updated | Metrics/histograms (v1.2) |
| `equivalence_classes` | id, name, fingerprint, created_at | Rule sharing across sources |
| `equivalence_members` | class_id, source_id, similarity | Class membership |
| `scout_files` | matched_rule_id, extraction_status, metadata_extracted, extraction_failures | ALTER required |

> **Note:** Tables marked (v1.2) were added in extraction.md v1.2. See that spec for full schema details.

**Migration check on view load:**
```rust
async fn check_schema(db: &SqlitePool) -> Result<SchemaStatus, Error> {
    let tables_exist = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='extraction_rules'"
    ).fetch_one(db).await?;

    if tables_exist == 0 {
        return Ok(SchemaStatus::NeedsMigration);
    }
    Ok(SchemaStatus::Ready)
}
```

**Schema Missing UI:**
```
┌─ Extraction Rules ──────────────────────────────────────────────┐
│                                                                  │
│   ┌─ Migration Required ───────────────────────────────────────┐ │
│   │                                                            │ │
│   │   The extraction rules feature requires a database         │ │
│   │   migration that has not yet been applied.                 │ │
│   │                                                            │ │
│   │   Run the following command to migrate:                    │ │
│   │                                                            │ │
│   │   $ casparian migrate                                      │ │
│   │                                                            │ │
│   │   [Esc] Go back                                            │ │
│   │                                                            │ │
│   └────────────────────────────────────────────────────────────┘ │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 7.3 Refresh Strategy

- **Auto-refresh**: Every 5 seconds while view is active
- **Event-driven refresh**: Immediately after rule create/update/delete
- **Background refresh**: Coverage stats update asynchronously
- **Debouncing**: Rapid changes coalesced into single refresh

---

## 8. Implementation Notes

### 8.1 Refresh Strategy

```rust
impl ExtractionView {
    const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
    const COVERAGE_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

    async fn refresh_rules(&mut self, db: &SqlitePool) -> Result<()> {
        let rules = query_rules_with_coverage(db).await?;
        self.state.rules = rules;
        self.state.last_refresh = Utc::now();
        self.apply_filter();
        Ok(())
    }

    fn apply_filter(&mut self) {
        if self.state.filter_text.is_empty() {
            self.state.filtered_indices = (0..self.state.rules.len()).collect();
        } else {
            let pattern = self.state.filter_text.to_lowercase();
            self.state.filtered_indices = self.state.rules
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    r.name.to_lowercase().contains(&pattern)
                    || r.glob_pattern.to_lowercase().contains(&pattern)
                })
                .map(|(i, _)| i)
                .collect();
        }
    }
}
```

### 8.2 YAML Validation

Validation occurs on every edit with debouncing:

```rust
impl YamlEditorState {
    const VALIDATION_DEBOUNCE: Duration = Duration::from_millis(300);

    async fn validate(&mut self) -> YamlValidation {
        // Parse YAML structure
        let parsed: Result<Vec<RuleYaml>, _> = serde_yaml::from_str(&self.content);

        match parsed {
            Err(e) => {
                let (line, col) = extract_position(&e);
                YamlValidation::Invalid {
                    line,
                    col,
                    message: e.to_string(),
                }
            }
            Ok(rules) => {
                // Validate rule semantics
                for rule in &rules {
                    if let Err(e) = validate_rule(rule) {
                        return YamlValidation::Invalid {
                            line: 0,
                            col: 0,
                            message: e.to_string(),
                        };
                    }
                }
                YamlValidation::Valid
            }
        }
    }
}

fn validate_rule(rule: &RuleYaml) -> Result<(), ValidationError> {
    // Check required fields
    if rule.name.is_empty() {
        return Err(ValidationError::MissingField("name"));
    }

    // Validate glob pattern compiles
    globset::Glob::new(&rule.glob)?;

    // Validate each field definition
    for (name, field) in &rule.extract {
        validate_field(name, field)?;
    }

    Ok(())
}
```

### 8.3 Priority Reordering Logic

Rules are ordered by priority (lower = higher precedence):

```rust
impl ExtractionViewState {
    fn swap_priority_up(&mut self, db: &SqlitePool) -> Result<()> {
        let current_idx = self.selected_index;
        if current_idx == 0 { return Ok(()); }

        let current = &self.rules[current_idx];
        let above = &self.rules[current_idx - 1];

        // Swap priorities
        let temp = current.priority;
        update_priority(db, current.id, above.priority).await?;
        update_priority(db, above.id, temp).await?;

        // Re-fetch to maintain sort
        self.refresh_rules(db).await?;
        self.selected_index = current_idx - 1;
        Ok(())
    }

    fn set_exact_priority(&mut self, db: &SqlitePool, new_priority: i32) -> Result<()> {
        let rule = &self.rules[self.selected_index];
        update_priority(db, rule.id, new_priority).await?;
        self.refresh_rules(db).await?;
        Ok(())
    }
}
```

### 8.4 Coverage Calculation

Coverage is calculated from `scout_files` on rule query:

```rust
async fn calculate_coverage(db: &SqlitePool, rule_id: Uuid) -> CoverageSummary {
    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) as "total!",
            SUM(CASE WHEN extraction_status = 'COMPLETE' THEN 1 ELSE 0 END) as "complete!",
            SUM(CASE WHEN extraction_status = 'PARTIAL' THEN 1 ELSE 0 END) as "partial!",
            SUM(CASE WHEN extraction_status = 'FAILED' THEN 1 ELSE 0 END) as "failed!"
        FROM scout_files
        WHERE matched_rule_id = ?
        "#,
        rule_id
    )
    .fetch_one(db)
    .await
    .unwrap_or_default();

    CoverageSummary {
        total_files: row.total as u32,
        complete: row.complete as u32,
        partial: row.partial as u32,
        failed: row.failed as u32,
    }
}
```

### 8.5 View Trait Implementation

```rust
impl View for ExtractionView {
    fn name(&self) -> &'static str {
        "Extraction Rules"
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.state.state {
            ExtractionState::Loading => self.render_loading(frame, area),
            ExtractionState::SchemaMissing => self.render_schema_missing(frame, area),
            ExtractionState::RuleList => self.render_rule_list(frame, area),
            ExtractionState::Wizard | ExtractionState::EditWizard => {
                self.render_rule_list(frame, area);
                self.render_wizard_dialog(frame, area);
            }
            ExtractionState::YamlEditorNew | ExtractionState::YamlEditorEdit => {
                self.render_yaml_editor(frame, area);
            }
            ExtractionState::TestMode => {
                self.render_rule_list_with_test(frame, area);
            }
            ExtractionState::DeleteConfirm => {
                self.render_rule_list(frame, area);
                self.render_delete_dialog(frame, area);
            }
            ExtractionState::PriorityMode => {
                self.render_rule_list_priority(frame, area);
            }
            ExtractionState::CoveragePanel => {
                self.render_rule_list_with_coverage(frame, area);
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> ViewAction {
        match &self.state.state {
            ExtractionState::Loading => ViewAction::None,
            ExtractionState::SchemaMissing => self.handle_schema_missing_event(event),
            ExtractionState::RuleList => self.handle_rule_list_event(event),
            ExtractionState::Wizard | ExtractionState::EditWizard => {
                self.handle_wizard_event(event)
            }
            ExtractionState::YamlEditorNew | ExtractionState::YamlEditorEdit => {
                self.handle_yaml_editor_event(event)
            }
            ExtractionState::TestMode => self.handle_test_mode_event(event),
            ExtractionState::DeleteConfirm => self.handle_delete_event(event),
            ExtractionState::PriorityMode => self.handle_priority_event(event),
            ExtractionState::CoveragePanel => self.handle_coverage_event(event),
        }
    }

    fn help_text(&self) -> Vec<(&'static str, &'static str)> {
        match self.state.state {
            ExtractionState::RuleList => vec![
                ("n", "New rule"),
                ("e", "Edit"),
                ("t", "Test"),
                ("y", "YAML"),
                ("d", "Delete"),
                ("p", "Priority"),
                ("c", "Coverage"),
                ("?", "Help"),
            ],
            ExtractionState::Wizard | ExtractionState::EditWizard => vec![
                ("Enter", "Continue"),
                ("Tab", "Next field"),
                ("Esc", "Cancel"),
                ("y", "Switch to YAML"),
            ],
            ExtractionState::YamlEditorNew | ExtractionState::YamlEditorEdit => vec![
                ("Ctrl+S", "Validate"),
                ("Ctrl+Enter", "Save"),
                ("Esc", "Cancel"),
            ],
            ExtractionState::TestMode => vec![
                ("t", "Test more"),
                ("f", "Test failed"),
                ("Esc", "Close"),
            ],
            ExtractionState::PriorityMode => vec![
                ("Up/Down", "Move rule"),
                ("Enter", "Set value"),
                ("Esc", "Done"),
            ],
            _ => vec![
                ("Enter", "Confirm"),
                ("Esc", "Cancel"),
            ],
        }
    }

    fn on_enter(&mut self) {
        self.state.state = ExtractionState::Loading;
        // Trigger async load
    }

    fn on_leave(&mut self) {
        // Close any open editors without saving
        self.state.yaml_editor = None;
        self.state.wizard = None;
        self.state.test_results = None;
    }
}
```

### 8.6 Conflict Detection

When saving a rule, check for overlapping patterns:

```rust
async fn detect_conflicts(
    db: &SqlitePool,
    new_rule: &RuleInfo,
) -> Vec<RuleConflict> {
    // Get all other rules
    let other_rules = query_rules(db).await
        .into_iter()
        .filter(|r| r.id != new_rule.id)
        .collect::<Vec<_>>();

    let mut conflicts = Vec::new();

    // Get files matching the new rule's pattern
    let new_matches: HashSet<PathBuf> = query_matching_files(db, &new_rule.glob_pattern).await
        .into_iter()
        .collect();

    for other in &other_rules {
        let other_matches: HashSet<PathBuf> = query_matching_files(db, &other.glob_pattern).await
            .into_iter()
            .collect();

        let overlap: Vec<_> = new_matches.intersection(&other_matches).collect();

        if !overlap.is_empty() {
            conflicts.push(RuleConflict {
                rule_id: other.id,
                rule_name: other.name.clone(),
                rule_priority: other.priority,
                overlap_count: overlap.len(),
            });
        }
    }

    conflicts.sort_by_key(|c| std::cmp::Reverse(c.overlap_count));
    conflicts
}
```

### 8.7 Test Mode Implementation

```rust
async fn run_extraction_test(
    db: &SqlitePool,
    rule: &RuleInfo,
    test_failed_only: bool,
) -> TestResults {
    // Get sample files
    let files = if test_failed_only {
        query_failed_files(db, rule.id, 10).await
    } else {
        query_random_files(db, rule.id, 10).await
    };

    // Run extraction on each file
    let mut samples = Vec::new();
    for path in files {
        let result = extract_fields(&rule.glob_pattern, &rule.fields, &path);
        samples.push(TestSample {
            path,
            status: result.status,
            extracted: result.values,
            errors: result.errors,
        });
    }

    TestResults {
        rule_id: rule.id,
        rule_name: rule.name.clone(),
        samples,
        run_at: Utc::now(),
    }
}
```

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-12 | 1.0 | Expanded from stub: comprehensive state machine, full data models, detailed workflows, YAML editor spec, coverage visualization, implementation notes. Reviewer fixes: clarified navigation path (drill-down from Discover, not key 5), added SCHEMA_MISSING state with migration prompt UI, added coverage_data invariant, explicit schema migration requirements |
| 2026-01-12 | 0.1 | Initial stub |

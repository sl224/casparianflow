# UI Specs Refinement - Compression Candidates

**Session:** ui_specs
**Started:** 2026-01-12

Per workflow v2.1 Section 14.5, this file tracks cross-spec redundancy for potential compression.

---

## Identified Candidates

### Candidate: Dropdown Widget Pattern
**Instances:**
- `specs/tui.md:339-354` - Dropdown Pattern (Telescope-style)
- `specs/views/discover.md:145-175` - Sidebar: Dropdown Navigation

**Similarity:** Both describe filterable dropdown with same behavior (type to filter, ↑/↓ to navigate, Enter to select)

**Suggested Action:** Keep in tui.md as canonical, cross-reference from discover.md

**Status:** PENDING

---

### Candidate: Dialog Pattern
**Instances:**
- `specs/tui.md:179-201` - Dialog Pattern
- `specs/views/discover.md:186-201` - Rules Manager Dialog
- `specs/views/discover.md:887-901` - Metadata Filter Dialog
- `specs/views/discover.md:942-983` - Pending Review Dialog

**Similarity:** All use centered overlay with header, content, keybindings footer

**Suggested Action:** Define dialog template in tui.md, view specs only show content

**Status:** PENDING

---

### Candidate: Status Indicators
**Instances:**
- `specs/tui.md:226-237` - Status Indicators (●○⚠✗⏸↻✓)
- `specs/views/parser_bench.md:349-357` - Parser States (►●○⚠⏸✗)
- `specs/views/jobs.md:67-75` - Job States (●○✓✗⊘⏸)

**Similarity:** Same icons with same meanings, minor variations

**Suggested Action:** Consolidate in tui.md, view specs reference

**Status:** PENDING

---

### Candidate: ViewState Base Fields
**Instances:**
- `specs/tui.md:289-295` - ViewState struct
- `specs/views/discover.md:269-305` - DiscoverState struct
- `specs/views/parser_bench.md:398-428` - ParserBenchState struct

**Similarity:** All have selected_index, filter, loading states

**Suggested Action:** Define base ViewState in tui.md, view specs extend it

**Status:** PENDING

---

### Candidate: Keybinding Tables Format
**Instances:**
- All view specs have keybinding tables
- Format varies: some have "Context" column, some don't

**Similarity:** Same information, different presentation

**Suggested Action:** Standardize format in tui.md Appendix B template

**Status:** PENDING

---

## Compression Log

| Date | Candidate | Action | Result |
|------|-----------|--------|--------|
| — | — | — | — |

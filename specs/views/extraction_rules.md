# Extraction Rules - TUI View Spec

**Status:** Obsolete draft (not implemented)
**Parent:** specs/tui.md (Master TUI Spec)
**Version:** 1.2
**Related:** specs/extraction.md, specs/rule_builder.md
**Last Updated:** 2026-01-14

> **Implementation Note:** There is no dedicated Extraction Rules view in the
> current TUI. Rule creation and management are handled via the Rule Builder
> and the Tagging Rules dialog in Discover.
>
> **Deletion Proposal:** This spec overlaps with the Rule Builder spec and
> describes a UI that is not on the current roadmap. Consider deleting or
> archiving it once Rule Builder documentation is finalized.

---

## 1. Current State

- No separate Extraction Rules view exists.
- Rule CRUD is limited to the Rules Manager dialog in Discover.
- YAML editor, coverage panel, and priority management are not implemented.

---

## 2. If Revived (Summary of Intended Design)

This spec originally proposed:
- A full-screen rule list with priority ordering.
- Wizard-based rule creation and YAML editor.
- Test mode with per-file extraction diagnostics.
- Coverage metrics and rule impact summaries.

Refer to earlier revisions or `specs/extraction.md` if this view is resurrected.

---

## 3. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.2 | Marked obsolete; recommend deletion/archival |

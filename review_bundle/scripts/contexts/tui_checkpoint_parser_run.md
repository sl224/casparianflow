# TUI Explore Context: Parser Run + Jobs

Goal: validate running a parser on a tagged set and job tracking.

Expected UI state:
- Ability to select a parser and a tag-based file set.
- Job appears in Jobs view with progress and final status.
- Errors are recorded for failed files.

Explore prompts:
- Enqueue a parse job for a tagged cohort.
- Watch progress; confirm completion or failure status.
- Open job details and inspect failures.

Report issues:
- No job created after run action.
- Jobs view not updating.
- Missing error details or counts.

# TUI Explore Context: Parser Authoring

Goal: validate creating and registering a new parser.

Expected UI state:
- Parser Bench lists available parsers.
- A newly created parser appears after refresh.
- Parser metadata validates (name/version/topics).

Explore prompts:
- Create a new parser from template (CLI or TUI entry).
- Refresh Parser Bench and confirm it appears.
- Open parser details and confirm metadata is correct.

Report issues:
- Parser not listed after creation.
- Validation errors on a fresh template.
- Missing or incorrect metadata.

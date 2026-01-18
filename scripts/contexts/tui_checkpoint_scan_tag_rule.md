<!-- REQUIRE_MOCK_TREE: /tmp/casparian_mock_tree -->
<!-- ENTRY_KEYS: 1,Tab,s,/tmp/casparian_mock_tree,Enter -->

# TUI Explore Context: Scan + Tag + Rule CRUD

Goal: validate scan persistence, manual tagging, and rule CRUD for a newly scanned source.

Expected UI state:
- Discover opens to Rule Builder.
- Scanning a folder creates a new source and file counts reflect the new tree.
- Pattern preview shows matches from the scanned source.
- Manual tagging applies to selected preview items only.
- Rule save persists and Rules Manager shows the saved rule.
- Rule edit updates the rule; delete removes it.

Explore prompts:
- Scan a new folder; confirm the selected source changes and file counts update.
- Enter a pattern; verify preview results are from the scanned source.
- Select 2 files, apply tag, and confirm only those are tagged.
- Save a rule, open Rules Manager, edit it, then delete it.

Report issues:
- Missing persistence (source/files/tags/rules not in DB).
- Rules Manager empty after save.
- Tagging applies to all instead of selection.
- Any unexpected errors or status messages.

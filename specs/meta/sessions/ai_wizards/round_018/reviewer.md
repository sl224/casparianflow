# Reviewer Assessment: GAP-INT-004

## Verdict: APPROVED

## Summary

The engineer's proposal comprehensively addresses GAP-INT-004 by providing a well-architected, multi-layered configuration system that makes complexity thresholds fully customizable. The specification maintains consistency with Casparian Flow's patterns (hierarchical config, CLI overrides, TOML-based), is implementation-ready with clear code examples, and introduces no critical gaps. The only minor improvement needed is a clarification on how `prefer_yaml` interacts with forced Python decisions.

## Checklist

- [x] Gap fully addressed - All three threshold points (regex chars, capture groups, preference bias) are configurable at multiple levels
- [x] Consistent with existing patterns - TOML config, CLI override precedence, source-specific scoping mirrors existing architecture
- [x] Implementation-ready - Pseudocode provided, test cases specified, clear data structures defined
- [x] Testable success criteria - Unit tests, classification tests, and E2E TUI tests all specified with concrete assertions
- [x] No critical gaps introduced - Proposal is self-contained; no unresolved dependencies discovered

## Detailed Findings

### Strengths

1. **Three-Level Hierarchy** (Sections 1-3): The YAML_OK → RECOMMEND_PYTHON → FORCE_PYTHON classification is elegant and mirrors real-world decision-making. Clear enough for users to understand, powerful enough to encode nuanced preferences.

2. **Config Schema (Section 2)**: The TOML structure is minimal, readable, and well-documented. Using `[ai.pathfinder]` namespace and `[sources."source_name"]` per-source overrides aligns perfectly with Casparian Flow's existing config patterns (see CLAUDE.md on configuration organization).

3. **Precedence Clear** (Section 4.2, 5.2): The three-level resolution (defaults → config file → CLI flags) is explicitly documented with a concrete resolution algorithm. The pseudocode in Section 4.1 is accurate and implementable.

4. **Sensitivity Modes** (Section 7): The "strict" vs "loose" dimension is practical—teams uncomfortable with recommendations can suppress them while still enforcing hard limits.

5. **Testing Strategy** (Section 10): Comprehensive coverage of three distinct test categories:
   - Configuration loading (unit)
   - Threshold classification logic (unit)
   - TUI integration (E2E)
   All test cases are concrete and executable.

6. **Backward Compatibility** (Section 9): Clear migration path from hard-coded constants to configurable thresholds, with explicit "no action required" for existing users.

7. **First-Run Experience** (Section 8): Silent defaults with informative logging, optional template generation via `casparian config init`. Respects users' time.

8. **CLI Override Examples** (Section 5.1-5.3): Flag naming is consistent (`--prefer-python`, `--recommend-regex-chars`), validation is specified, and the precedence table is clear.

### Concerns

**MINOR:**

1. **Issue:** Section 6.3 ("User Hint Context") states "Threshold settings override: Ignored." but doesn't explicitly clarify behavior.
   - **Severity:** MINOR
   - **Details:** When user provides hints that require Python (per ai_wizards.md Section 3.1.1), the proposal doesn't clarify whether `prefer_yaml = true` still appears in the UI or is silently suppressed. Since hints force Python, prefer_yaml becomes moot, but the spec should state this explicitly.
   - **Recommended Fix:** Add: "When hints force Python classification, prefer_yaml setting is ignored (no choice to offer)."

2. **Issue:** Default values rationale (Section 2.3) states `recommend_python_regex_chars = 100` is "~2-3 lines of code at typical editor width" but doesn't account for YAML syntax wrapping.
   - **Severity:** MINOR
   - **Details:** A regex in YAML needs escaping/quoting, which can add 10+ characters. The rationale could be slightly more precise. However, 100 is reasonable as a default.
   - **Recommended Fix:** Optional—add parenthetical: "(after YAML syntax, typically spans 2-3 visual lines)"

3. **Issue:** Section 2.4 overrides use `[sources."source_name"]` but spec doesn't clarify what "source_name" means (is it the source ID from scout_sources.id? path? name field?).
   - **Severity:** MINOR
   - **Details:** The implementation section (line 241) shows `format!("sources.{}", source_id)`, suggesting source_id is passed as a parameter, but the config spec should define "source_name" explicitly.
   - **Recommended Fix:** Add to Section 2.4: "source_name must match the `id` field from `scout_sources.id` (e.g., 'my_sales_data', not paths)."

4. **Issue:** Section 10.3 E2E test uses `PathfinderTui::start()` but this type isn't defined in the proposal and assumes the TUI will be refactored to support this API.
   - **Severity:** MINOR
   - **Details:** The pseudocode is correct but may require new TUI infrastructure. However, the goal is clear: verify complexity recommendation displays and user interaction works.
   - **Recommended Fix:** Add note: "E2E test assumes Pathfinder TUI supports programmatic input/capture API (see specs/views/pathfinder.md for TUI architecture details)."

### Recommendations

For **APPROVED** with these additions:

1. **Clarify hint override behavior** (Section 6.3): When user hints force Python, explicitly state that `prefer_yaml` is ignored, and no choice is offered in the UI.

2. **Define source_name** (Section 2.4): Explicitly state whether source_name refers to source ID, path, or name field, and provide example: `[sources."my_sales_data"]` where `my_sales_data` is the scout_sources.id.

3. **Update ai_wizards.md Section 3.1.1** (Section 12): The cross-reference is correct, but ensure the parent spec (ai_wizards.md) includes a link back to this document: `For detailed configuration instructions, see specs/meta/sessions/ai_wizards/round_018/configuration.md` (or whatever the final filename is).

4. **Consider per-threshold granularity** (Future): The current proposal allows per-source overrides at the whole-config level. If a future use case requires overriding only `force_python_regex_chars` for a source while inheriting other values, the merge logic in Section 4.2 already supports this—no changes needed, but document this as intentional.

## New Gaps Identified

**None critical.** The proposal is self-contained. However, there are two **downstream considerations** (not gaps in this spec, but relevant to implementation):

1. **GAP: Config Validation at Load** (MINOR)
   - The spec doesn't mention validation of config file syntax (e.g., `recommend >= 50`, `force >= recommend`).
   - Recommend: Add validation logic to `load_toml_config()` that enforces:
     - `recommend_python_regex_chars >= 50`
     - `force_python_regex_chars >= recommend_python_regex_chars`
     - Same for capture_groups
   - Severity: MINOR (would be caught by tests, but explicit validation prevents confusing user errors)

2. **GAP: Config Reload** (MINOR)
   - Spec doesn't address whether config.toml changes take effect on next command or require TUI restart.
   - Recommend: Clarify in CLAUDE.md: "Config changes take effect on next CLI command. TUI running in interactive mode will use config at startup; users must restart TUI to pick up config.toml changes."
   - Severity: MINOR (UX clarity only, doesn't affect correctness)

## Assessment Conclusion

The proposal is **APPROVED for implementation**. It's ready for engineers to build from. The three minor concerns are clarifications, not blockers. Section 12's promise to update ai_wizards.md Section 3.1.1 should be fulfilled as part of implementation (ensuring bidirectional references).

The design elegantly solves the original gap—users can now adjust complexity thresholds to match their team's Python comfort level and YAML preference—while maintaining the YAML-first philosophy of Pathfinder.

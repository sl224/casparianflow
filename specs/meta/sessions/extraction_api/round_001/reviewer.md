# Reviewer Feedback - Round 1

## Review: GAP-SIMPLE-001

### Critical Issues

- **[CRIT-001]**: Single-File Inference Doesn't Work
  - Location: Tier 1 "point at single file, get inference"
  - Impact: Inference algorithm requires multiple samples to detect varying vs constant segments. n=1 provides no variation data.
  - Suggestion: Require 3+ samples for inference OR fall back to template-matching for single files.

- **[CRIT-002]**: Date/Entity Detection Ambiguous
  - Location: Inference engine assumptions
  - Impact: `mission_042` could be entity OR literal. `20240115` could be date OR version. Without multiple samples, ambiguity is unresolvable.
  - Suggestion: Define explicit fallback behavior when confidence is low. Show users WHAT was inferred and WHY.

### High Priority Issues

- **[HIGH-001]**: Hiding Semantic Layer Removes Power User Value
  - Location: "Semantic layer becomes internal"
  - Impact: Power users who understand `entity_folder(mission) > dated_hierarchy(iso)` lose that authoring path.
  - Suggestion: Keep semantic expressions as optional advanced syntax in YAML.

- **[HIGH-002]**: Tag-Only vs Extraction Rules Need Clarity
  - Location: "Single extraction rule concept"
  - Impact: A tag-only rule (`tag: x` with no `extract:`) is operationally different. Database schema handles this awkwardly.
  - Suggestion: Explicitly document `extract: null` for tag-only rules.

- **[HIGH-003]**: YAML Schema Isn't Actually Simpler
  - Location: Tier 2 YAML
  - Impact: Same complexity as current spec - segment addressing, regex, type hints all required.
  - Suggestion: Add intelligent defaults or acknowledge Tier 2 is for power users only.

### Medium Priority Issues

- **[MED-001]**: Templates May Not Generalize
  - Defense might use `operation_*`, `sortie_*`, not just `mission_*`
  - Suggestion: Expand to 10-15 templates or add template parameters.

- **[MED-002]**: Tier 1 → YAML Export Workflow Unclear
  - What happens to original rule when exported and re-imported?
  - Suggestion: Define explicit upgrade lifecycle.

- **[MED-003]**: CLI vs TUI Mode Confusion
  - `casparian extract` interactive prompting vs batch mode unclear.
  - Suggestion: Define three modes: interactive, batch (`--accept`), TUI.

- **[MED-004]**: Equivalence Classes Worth Keeping
  - Algorithmic, not AI-dependent. Provides immediate cross-source value.
  - Suggestion: Reconsider deferral.

### Low Priority / Nits

- **[LOW-001]**: `extract` vs `rules` namespace inconsistent
- **[LOW-002]**: Partial inference failure UX missing
- **[LOW-003]**: Spec merge mechanics not described

### Verdict
**NEEDS_REVISION**

### Summary

The north star (radical simplicity) is sound. However, single-file inference is technically unsupportable, hiding semantic layer removes value, and YAML isn't actually simpler. Key fixes: require 3+ samples OR template fallback, keep semantic expressions optional, define clear Tier 1 → Tier 2 upgrade path.

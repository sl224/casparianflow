# Engineer Resolution: GAP-INT-004

## Complexity Thresholds Configuration Specification

**Gap:** The YAML vs Python decision algorithm (Section 3.1.1) mentions patterns that generate recommendations ("Regex is 87 chars. Consider Python for readability"), but the complexity thresholds are not configurable:
- Regex >100 chars
- >5 capture groups

Users may want to adjust these thresholds based on their team's Python comfort level or YAML preference.

**Confidence:** HIGH

---

## 1. Complexity Thresholds Overview

The Pathfinder Wizard uses **three decision levels** for YAML vs Python classification, each with configurable thresholds:

```
┌─────────────────────────────────────────────────────────────┐
│  Complexity Classification System                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  YAML_OK:              Stays YAML, no concerns             │
│  ↓ (Below threshold 1)                                      │
│                                                             │
│  RECOMMEND_PYTHON:     YAML works, but recommend Python    │
│  ↓ (Above threshold 1, below threshold 2)                   │
│                                                             │
│  FORCE_PYTHON:         No YAML option, must use Python     │
│  (Above threshold 2)                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Key Principle:** Thresholds are configuration options, not hard-coded constants. Users can adjust based on:
- Team Python skill level
- YAML vs Python preference
- Project conventions
- Regex expertise in the organization

---

## 2. Configuration Schema

### 2.1 Configuration File Location

Configuration lives at: `~/.casparian_flow/config.toml`

**Fallback behavior:**
- If file doesn't exist, use built-in defaults
- File is optional; CLI flags override config

### 2.2 TOML Schema

```toml
# ~/.casparian_flow/config.toml

# Pathfinder Wizard complexity thresholds
[ai.pathfinder]

# Recommendation threshold: Regex becomes "consider Python" above this length
recommend_python_regex_chars = 100

# Recommendation threshold: Capture groups become "consider Python" above this count
recommend_python_capture_groups = 5

# Force threshold: Regex ALWAYS Python above this length (no YAML option)
force_python_regex_chars = 200

# Force threshold: Capture groups ALWAYS Python above this count (no YAML option)
force_python_capture_groups = 10

# Default preference: Biases which language is offered when both are viable
# Values: "yaml" (prefer YAML first) or "python" (prefer Python first)
prefer_yaml = true

# Threshold sensitivity: How strictly to apply thresholds
# Values: "strict" (apply all), "loose" (only force, no recommend)
sensitivity = "strict"
```

### 2.3 Default Values

| Parameter | Default | Rationale |
|-----------|---------|-----------|
| `recommend_python_regex_chars` | 100 | ~2-3 lines of code at typical editor width |
| `recommend_python_capture_groups` | 5 | Beyond 5, regex becomes hard to read |
| `force_python_regex_chars` | 200 | Clearly unreadable as YAML |
| `force_python_capture_groups` | 10 | Unmaintainable without variable names |
| `prefer_yaml` | true | YAML-first aligns with Pathfinder philosophy |
| `sensitivity` | "strict" | Apply recommendations AND force thresholds |

### 2.4 Per-Source Overrides

For users with many sources having different conventions, overrides can be specified per-source:

```toml
# ~/.casparian_flow/config.toml

[sources."my_sales_data"]
# This source has regex-heavy naming, so raise recommend threshold
recommend_python_regex_chars = 150
prefer_yaml = false  # This source prefers Python

[sources."my_logs"]
# Logs are well-structured, keep default thresholds
# (Omitted settings inherit from [ai.pathfinder])
sensitivity = "loose"  # Only force, don't recommend
```

**Override resolution:**
1. Check for source-specific setting in `[sources."source_name"]`
2. Fall back to `[ai.pathfinder]`
3. Fall back to built-in defaults

---

## 3. Threshold Levels: Detailed Classification

### 3.1 YAML_OK Level

**Definition:** Pattern uses YAML-expressible constructs without concern.

**Conditions:**
```
(regex_chars <= recommend_python_regex_chars) AND
(capture_groups <= recommend_python_capture_groups)
```

**UI Presentation:**
```
✓ YAML Extraction Rule
  Pattern is YAML-friendly, no complexity concerns.
```

**User Actions:**
- Use YAML rule as-is
- Switch to Python manually if preferred (via `--prefer-python` flag)

**Examples:**
```
✓ Simple date extraction: `/(\d{4})-(\d{2})-(\d{2})/`
  regex_chars: 24, capture_groups: 3
```

### 3.2 RECOMMEND_PYTHON Level

**Definition:** Pattern technically YAML-expressible, but complexity suggests Python would be clearer.

**Conditions:**
```
(regex_chars > recommend_python_regex_chars OR
 capture_groups > recommend_python_capture_groups) AND
(regex_chars <= force_python_regex_chars AND
 capture_groups <= force_python_capture_groups)
```

**UI Presentation:**
```
⚠ Generated YAML Rule
  Recommendation: Consider Python for readability
  Regex is 115 chars with 6 capture groups.
  Edit to Python? [Yes / No / Use YAML as-is]
```

**User Actions:**
- Accept YAML and proceed
- Switch to Python (triggers Python generation, validation, review)
- Manually edit YAML (advanced)

**Examples:**
```
⚠ Phone number extraction:
   /^(\+\d{1,3})?[\s.-]?(\(\d{3}\)|^(\d{3}))[\s.-]?(\d{3})[\s.-]?(\d{4})$/
   regex_chars: 95, capture_groups: 5
   → Above recommend_python_capture_groups = 5
   → Recommendation shown

⚠ CSV header normalization:
   /^([\w_-]+)\s*=\s*(int|float|str|bool|date)\s*\(default:\s*(.+)\)$/
   regex_chars: 78, capture_groups: 3
   → Below recommend threshold → YAML_OK
```

### 3.3 FORCE_PYTHON Level

**Definition:** Complexity exceeds safe YAML expressibility. Python is mandatory.

**Conditions:**
```
(regex_chars > force_python_regex_chars OR
 capture_groups > force_python_capture_groups)
```

**UI Presentation:**
```
🔴 Python Extractor Required
  Regex complexity exceeds safe YAML limits.
  Regex is 210 chars. Using Python.
  (YAML option not available)
```

**User Actions:**
- Review generated Python (no choice, Python only)
- Accept or provide hints for LLM refinement
- Cannot select YAML

**Examples:**
```
🔴 Complex log format:
   /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.(\d{3})Z)\s+
     \[(\w+)\]\s+(\{[^}]+\})\s+
     \[([A-Z_]+)\]\s+(\w+\.\w+\.\w+):\s+(.+)$/
   regex_chars: 198, capture_groups: 7
   → Exceeds force_python_regex_chars = 200
   → Python required
```

---

## 4. Configuration Resolution Logic

### 4.1 Loading Configuration

```rust
// Pseudocode: Configuration loading with precedence
fn load_complexity_config(source_id: Option<&str>) -> ComplexityThresholds {
    // 1. Start with built-in defaults
    let mut config = ComplexityThresholds::defaults();

    // 2. Load from config file if present
    if let Ok(toml_config) = load_toml_config() {
        // Merge [ai.pathfinder] section
        if let Some(pathfinder_section) = toml_config.get("ai.pathfinder") {
            config.merge_with(pathfinder_section);
        }

        // Merge source-specific overrides if source_id provided
        if let Some(source_id) = source_id {
            let source_key = format!("sources.{}", source_id);
            if let Some(source_section) = toml_config.get(&source_key) {
                config.merge_with(source_section);
            }
        }
    }

    // 3. CLI flags override everything (lowest precedence - applied last)
    // (See Section 5: CLI Overrides)

    config
}
```

### 4.2 Configuration Merging

When merging configurations, **each threshold is independent**:

```rust
struct ComplexityThresholds {
    recommend_python_regex_chars: u32,
    recommend_python_capture_groups: u32,
    force_python_regex_chars: u32,
    force_python_capture_groups: u32,
    prefer_yaml: bool,
    sensitivity: Sensitivity,  // "strict" | "loose"
}

impl ComplexityThresholds {
    fn merge_with(&mut self, other: &ComplexityThresholds) {
        // Each field is independently overridden
        // Only fields explicitly set in 'other' override self

        if let Some(val) = other.recommend_python_regex_chars {
            self.recommend_python_regex_chars = val;
        }
        // ... repeat for each field
    }
}
```

---

## 5. CLI Overrides

### 5.1 Preference Flags

All Pathfinder commands accept preference overrides:

```bash
# Prefer Python output (overrides config)
casparian pathfinder --source sales_data --prefer-python

# Prefer YAML output (overrides config)
casparian pathfinder --source sales_data --prefer-yaml

# Set recommendation threshold
casparian pathfinder --source sales_data --recommend-regex-chars 150

# Set force threshold
casparian pathfinder --source sales_data --force-regex-chars 250
```

### 5.2 Flag Interaction with Config

**Precedence (lowest to highest):**
1. Built-in defaults
2. `config.toml [ai.pathfinder]`
3. `config.toml [sources."source_name"]`
4. CLI flags (highest priority, always override)

**Example Resolution:**

```
config.toml:
  [ai.pathfinder]
  prefer_yaml = true
  recommend_python_regex_chars = 100

CLI:
  casparian pathfinder --source sales_data --prefer-python

Result:
  → prefer_yaml = false  (CLI overrides config)
  → recommend_python_regex_chars = 100 (not specified in CLI, uses config)
```

### 5.3 Flag Details

| Flag | Values | Default | Scope |
|------|--------|---------|-------|
| `--prefer-python` | (boolean) | per config | Single command |
| `--prefer-yaml` | (boolean) | per config | Single command |
| `--recommend-regex-chars N` | N >= 50 | per config | Single command |
| `--recommend-capture-groups N` | N >= 1 | per config | Single command |
| `--force-regex-chars N` | N >= recommend | per config | Single command |
| `--force-capture-groups N` | N >= recommend | per config | Single command |
| `--sensitivity [strict\|loose]` | strict, loose | "strict" | Single command |

**Validation:**
- `--force-regex-chars` must be `>= --recommend-regex-chars` (enforced at parse time)
- `--force-capture-groups` must be `>= --recommend-capture-groups` (enforced at parse time)

---

## 6. TUI Integration

### 6.1 Complexity Indicator Display

When displaying extracted fields in the Discover/Pathfinder TUI, show complexity level inline:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Field: date_range
Pattern: /^(\d{4})-(\d{2})-(\d{2}) to (\d{4})-(\d{2})-(\d{2})$/
Regex Chars: 48    Capture Groups: 6
  ⚠ Recommend Python: 6 groups exceeds threshold of 5
  [Use YAML]  [Switch to Python]  [Dismiss]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Field: order_id
Pattern: /^ORD-(\d{6})-([A-Z]{3})-(\d{2})$/
Regex Chars: 35    Capture Groups: 3
  ✓ YAML-friendly, no complexity concerns
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 6.2 Threshold Configuration in TUI

Add settings panel for complexity thresholds (optional, expert mode):

```
Settings > AI Wizards > Complexity Thresholds

  Recommend Python when regex chars > [100____]
  Recommend Python when capture groups > [5_]
  Force Python when regex chars > [200____]
  Force Python when capture groups > [10_]

  Default preference: (●) YAML first  ( ) Python first
  Sensitivity: (●) Strict  ( ) Loose

  [Reset to Defaults]  [Load from config.toml]  [Save to config.toml]
```

### 6.3 User Hint Context

When user provides hints (Section 3.1 of ai_wizards.md), show current thresholds in context:

```
Field: duration_range
User Hint: "compute start/end dates from quarter"
  ⚠ Hints require Python. Threshold settings override: Ignored.
  Generated Python Extractor:
    [Python code...]
```

---

## 7. Sensitivity Modes

### 7.1 Strict Mode (Default)

Both recommendation AND force thresholds are applied.

```
sensitivity = "strict"

Behavior:
- regex_chars in range [recommend, force) → Show recommendation
- capture_groups in range [recommend, force) → Show recommendation
- regex_chars >= force OR capture_groups >= force → Force Python
```

**Use case:** Teams wanting guidance on code quality

### 7.2 Loose Mode

Only force thresholds applied; recommendations suppressed.

```
sensitivity = "loose"

Behavior:
- All regex_chars < force AND capture_groups < force → YAML_OK (no recommendation)
- regex_chars >= force OR capture_groups >= force → Force Python (mandatory)
```

**Use case:** Teams comfortable with complex YAML, want binary decision only

### 7.3 Example Configuration

```toml
# Production pipeline: Strict guidance
[sources."prod_data"]
sensitivity = "strict"
prefer_yaml = true
recommend_python_regex_chars = 80  # Lower threshold, more recommendations

# Ad-hoc analysis: Loose mode, Python-friendly
[sources."adhoc_analysis"]
sensitivity = "loose"
prefer_yaml = false
force_python_regex_chars = 300  # Only force very complex
```

---

## 8. Default Configuration Bootstrap

### 8.1 First-Run Experience

When a user runs Pathfinder for the first time and `~/.casparian_flow/config.toml` doesn't exist:

1. **No prompt** - Use built-in defaults silently
2. **Log info** - CLI output shows: `Using default complexity thresholds. Customize in ~/.casparian_flow/config.toml`
3. **Provide template** - When user edits config, include well-documented example

### 8.2 Config Template

```bash
# When user runs: casparian config init
# Creates ~/.casparian_flow/config.toml with:

# =============================================================================
# Casparian Flow Configuration
# =============================================================================

# Pathfinder Wizard complexity thresholds
# These settings control when to recommend Python vs using YAML.
[ai.pathfinder]

# Recommend Python when regex exceeds this length (chars)
# Examples:
#   100 (default): "Consider Python at typical code width"
#   150 (lenient): "Only recommend very complex regex"
#   80 (strict): "Recommend Python early, prioritize readability"
recommend_python_regex_chars = 100

# Recommend Python when capture groups exceed this count
# Examples:
#   5 (default): "Standard complexity limit"
#   3 (strict): "Favor Python for regex with 4+ groups"
#   10 (lenient): "Only recommend for really complex patterns"
recommend_python_capture_groups = 5

# Force Python (no YAML option) when regex exceeds this length
force_python_regex_chars = 200

# Force Python (no YAML option) when capture groups exceed this count
force_python_capture_groups = 10

# Default preference when both YAML and Python are viable
# true: Offer YAML first (Pathfinder philosophy: YAML-first)
# false: Offer Python first
prefer_yaml = true

# Sensitivity level for applying thresholds
# "strict": Apply both recommend AND force thresholds
# "loose": Only apply force thresholds, no recommendations
sensitivity = "strict"

# Per-source overrides: uncomment and modify for specific sources
# [[sources."my_source_name"]]
# recommend_python_regex_chars = 150
# prefer_yaml = false
```

---

## 9. Backward Compatibility

### 9.1 Hard-Coded Thresholds → Configurable

This specification formalizes thresholds that currently exist as hard-coded constants in code (e.g., `REGEX_CHAR_LIMIT = 100`).

**Migration:**
1. Extract constants into `ComplexityThresholds` struct
2. Load from config (default to current hard-coded values)
3. No user-facing changes initially; thresholds use current defaults
4. Documentation explains how to customize

### 9.2 Existing Installations

Users with existing `~/.casparian_flow/` directory:
- No action required
- Built-in defaults used until they create `config.toml`
- Behavior unchanged

---

## 10. Testing Strategy

### 10.1 Configuration Loading Tests

```rust
#[test]
fn test_default_thresholds() {
    let config = ComplexityThresholds::defaults();
    assert_eq!(config.recommend_python_regex_chars, 100);
    assert_eq!(config.recommend_python_capture_groups, 5);
}

#[test]
fn test_load_from_toml() {
    let toml_str = r#"
        [ai.pathfinder]
        recommend_python_regex_chars = 150
        sensitivity = "loose"
    "#;
    let config = load_complexity_config_from_toml(toml_str, None);
    assert_eq!(config.recommend_python_regex_chars, 150);
    assert_eq!(config.sensitivity, Sensitivity::Loose);
}

#[test]
fn test_source_override() {
    let toml_str = r#"
        [ai.pathfinder]
        recommend_python_regex_chars = 100

        [sources."test_source"]
        recommend_python_regex_chars = 200
    "#;
    let config = load_complexity_config_from_toml(toml_str, Some("test_source"));
    assert_eq!(config.recommend_python_regex_chars, 200);
}

#[test]
fn test_cli_override_precedence() {
    let config = load_complexity_config_from_toml(toml_str, Some("test_source"));
    let cli_override = CliArgs {
        recommend_python_regex_chars: Some(250),
        ..Default::default()
    };
    let final_config = config.override_with_cli(&cli_override);
    assert_eq!(final_config.recommend_python_regex_chars, 250);
}
```

### 10.2 Threshold Classification Tests

```rust
#[test]
fn test_yaml_ok_classification() {
    let config = ComplexityThresholds::defaults();
    let pattern = Pattern {
        regex: "foo".to_string(),
        regex_chars: 50,
        capture_groups: 2,
    };
    assert_eq!(
        classify_complexity(&pattern, &config),
        ComplexityLevel::YamlOk
    );
}

#[test]
fn test_recommend_python_classification() {
    let config = ComplexityThresholds::defaults();
    let pattern = Pattern {
        regex: "complex".to_string(),
        regex_chars: 120,  // Above 100
        capture_groups: 2,
    };
    assert_eq!(
        classify_complexity(&pattern, &config),
        ComplexityLevel::RecommendPython
    );
}

#[test]
fn test_force_python_classification() {
    let config = ComplexityThresholds::defaults();
    let pattern = Pattern {
        regex: "huge".to_string(),
        regex_chars: 250,  // Above 200
        capture_groups: 2,
    };
    assert_eq!(
        classify_complexity(&pattern, &config),
        ComplexityLevel::ForcePython
    );
}
```

### 10.3 UI Integration Tests

E2E test showing complexity recommendation in TUI:

```rust
#[tokio::test]
async fn test_pathfinder_shows_complexity_recommendation() {
    // Generate pattern that exceeds recommend threshold
    let pattern = generate_pattern_with_6_capture_groups();

    // Start Pathfinder TUI
    let mut tui = PathfinderTui::start().await;
    tui.input_pattern(&pattern).await;

    // Verify recommendation shown
    let screen = tui.capture_screen();
    assert!(screen.contains("Recommend Python"));
    assert!(screen.contains("6 capture groups"));

    // Verify user can accept YAML
    tui.send_key("Y").await;  // "Use YAML"
    let result = tui.get_result().await;
    assert_eq!(result.language, Language::Yaml);
}
```

---

## 11. Implementation Checklist

- [ ] Define `ComplexityThresholds` struct in casparian_mcp crate
- [ ] Implement `load_from_file()` and `merge()` logic
- [ ] Add TOML parsing (use serde with toml crate)
- [ ] Implement `classify_complexity()` function
- [ ] Add CLI flags (`--prefer-python`, `--recommend-regex-chars`, etc.)
- [ ] Update Pathfinder command to use configurable thresholds
- [ ] Implement UI indicators (✓, ⚠, 🔴) in Pathfinder TUI
- [ ] Create config template and `casparian config init` command
- [ ] Add configuration loading tests (unit)
- [ ] Add threshold classification tests (unit)
- [ ] Add E2E TUI test for recommendation display
- [ ] Document in ai_wizards.md Section 3.1.3
- [ ] Update CLAUDE.md with configuration details

---

## 12. Integration with Section 3.1

This specification complements Section 3.1.1 (YAML vs Python Decision Algorithm):

| Aspect | Specification |
|--------|---|
| **Algorithm logic** | Section 3.1.1 (unchanged) |
| **Thresholds** | Section 3.1.3 (this document) |
| **Python validation** | Section 3.1.2 (unchanged) |
| **User interaction** | Pathfinder TUI (specs/views/pathfinder.md) |

**Updated Section 3.1.1 reference:**
```markdown
#### 3.1.1 YAML vs Python Decision Algorithm

... [existing content] ...

**Configurable Thresholds:**

The "recommendation threshold" mentioned above (100 chars, 5 groups) and "force
threshold" (200 chars, 10 groups) are fully configurable. See Section 3.1.3
(Complexity Configuration) for how to customize thresholds via config.toml or CLI flags.
```

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 0.1 | Initial specification: configuration schema, threshold levels, CLI overrides, TUI integration |

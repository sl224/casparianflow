# Engineer Resolution: GAP-CONFIG-001

## Configuration Defaults vs Code Defaults - Clear Precedence Rules

**Gap:** GAP-CONFIG-001 - Config defaults vs code defaults unclear
**Priority:** MEDIUM
**Status:** RESOLVED
**Date:** 2026-01-13

---

## 1. Problem Statement

The Casparian Flow configuration system has two sources of defaults:
- **Code defaults**: Built-in values in Rust/Python code
- **Config file defaults**: Values defined in `~/.casparian_flow/config.toml`

Previously unclear:
- When does each type of default apply?
- What happens when config file is missing?
- How do source-specific overrides interact with global config?
- How do CLI flags interact with the hierarchy?

**Impact:** Developers and users may not understand which default value applies in a given context, leading to unexpected behavior or misconfiguration.

---

## 2. Configuration Precedence Hierarchy

This is the SINGLE AUTHORITATIVE precedence rule for all configuration in Casparian Flow:

```
┌─────────────────────────────────────────────────────────────────┐
│         Configuration Precedence (Lowest to Highest)            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. CODE DEFAULTS                                               │
│     ├─ Built-in constants in Rust (hardcoded)                  │
│     ├─ Built-in constants in Python (hardcoded)                │
│     └─ Example: complexity_thresholds = {100, 5, 200, 10}      │
│        (from casparian_mcp src/tools/ai_wizards.rs)           │
│                                                                 │
│  2. CONFIG FILE [GLOBAL] SECTION                                │
│     ├─ ~/.casparian_flow/config.toml [ai.pathfinder]           │
│     ├─ ~/.casparian_flow/config.toml [complexity]              │
│     ├─ Applied if file exists and section is present           │
│     └─ Overrides code defaults for specified fields only       │
│                                                                 │
│  3. CONFIG FILE [SOURCE-SPECIFIC] SECTION                       │
│     ├─ ~/.casparian_flow/config.toml [sources."source_name"]   │
│     ├─ Only applies if source_id context is provided           │
│     └─ Overrides both code defaults and global config          │
│                                                                 │
│  4. ENVIRONMENT VARIABLES                                       │
│     ├─ CASPARIAN_* prefixed variables                          │
│     ├─ Example: CASPARIAN_PREFER_PYTHON=true                  │
│     └─ Overrides file-based config (optional layer)            │
│                                                                 │
│  5. CLI FLAGS                                                   │
│     ├─ Command-line arguments (highest priority)               │
│     ├─ Example: casparian ... --prefer-python                 │
│     ├─ Any flag provided overrides all above                   │
│     └─ Per-command execution only                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key Principle:** Each level only overrides fields it explicitly specifies. Unspecified fields fall through to the level below.

---

## 3. Default Value Sources by Component

### 3.1 Complexity Thresholds

| Setting | Code Default | Config File | Source Override | CLI Flag |
|---------|--------------|-------------|-----------------|----------|
| `recommend_python_regex_chars` | 100 | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--recommend-regex-chars N` |
| `recommend_python_capture_groups` | 5 | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--recommend-capture-groups N` |
| `force_python_regex_chars` | 200 | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--force-regex-chars N` |
| `force_python_capture_groups` | 10 | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--force-capture-groups N` |
| `prefer_yaml` | true | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--prefer-python` or `--prefer-yaml` |
| `sensitivity` | "strict" | `[ai.pathfinder]` or `[complexity]` | `[sources."name"]` | `--sensitivity [strict\|loose]` |

**Example Resolution Chain for `prefer_yaml`:**

```
User runs: casparian pathfinder --source sales_data

1. Start: prefer_yaml = true (code default)
2. Load config.toml [ai.pathfinder]
   - If "prefer_yaml = false" exists → prefer_yaml = false
   - If not present → prefer_yaml = true (unchanged)
3. Load config.toml [sources."sales_data"]
   - If "prefer_yaml = false" exists → prefer_yaml = false
   - If not present → prefer_yaml = (value from step 2)
4. Apply CLI flags
   - If "--prefer-python" present → prefer_yaml = false
   - If "--prefer-yaml" present → prefer_yaml = true
   - If neither present → prefer_yaml = (value from step 3)

Final result: Whichever was specified at the highest level wins
```

### 3.2 Model Configuration

| Setting | Code Default | Config File | Source Override | CLI Flag |
|---------|--------------|-------------|-----------------|----------|
| `model` | "claude-3-5-sonnet" | `[ai.models]` | `[sources."name".models]` | `--model NAME` |
| `api_provider` | "anthropic" | `[ai.models]` | `[sources."name".models]` | `--provider NAME` |
| `temperature` | 0.7 | `[ai.models]` | `[sources."name".models]` | `--temperature N` |
| `max_tokens` | 4096 | `[ai.models]` | `[sources."name".models]` | `--max-tokens N` |

### 3.3 Security/Privacy Settings

| Setting | Code Default | Config File | Source Override | CLI Flag |
|---------|--------------|-------------|-----------------|----------|
| `redact_level` | "high" | `[security]` | `[sources."name".security]` | `--redact [high\|medium\|low]` |
| `allow_http` | false | `[security]` | `[sources."name".security]` | `--allow-http` |
| `api_key_env` | "ANTHROPIC_API_KEY" | `[security]` | N/A | N/A |

---

## 4. Configuration Resolution Algorithm

### 4.1 Pseudocode

```rust
fn resolve_config_value<T>(
    key: &str,
    code_default: T,
    source_id: Option<&str>,
) -> T {
    // Start with code default
    let mut value = code_default;

    // Layer 1: Global config file section
    if let Ok(config) = load_config_file() {
        if let Some(global_section) = config.get_section("ai") {
            if let Some(file_value) = global_section.get(key) {
                value = file_value;  // Override
            }
        }
    }

    // Layer 2: Source-specific config file section
    if let Some(source_id) = source_id {
        if let Ok(config) = load_config_file() {
            let source_key = format!("sources.{}", source_id);
            if let Some(source_section) = config.get_section(&source_key) {
                if let Some(file_value) = source_section.get(key) {
                    value = file_value;  // Override
                }
            }
        }
    }

    // Layer 3: Environment variable (optional)
    let env_key = format!("CASPARIAN_{}", key.to_uppercase());
    if let Ok(env_value) = std::env::var(&env_key) {
        value = parse_env_value(&env_value);  // Override
    }

    // Layer 4: CLI flag (handled by clap/command parser)
    // This happens BEFORE calling this function; only non-default
    // CLI values are passed to this function as an override parameter

    value
}
```

### 4.2 Implementation in Rust

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityThresholds {
    // Individual fields with Option to track "explicitly set" vs "inherited"
    pub recommend_python_regex_chars: Option<u32>,
    pub recommend_python_capture_groups: Option<u32>,
    pub force_python_regex_chars: Option<u32>,
    pub force_python_capture_groups: Option<u32>,
    pub prefer_yaml: Option<bool>,
    pub sensitivity: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    // Final resolved values (never Option)
    pub recommend_python_regex_chars: u32,
    pub recommend_python_capture_groups: u32,
    pub force_python_regex_chars: u32,
    pub force_python_capture_groups: u32,
    pub prefer_yaml: bool,
    pub sensitivity: Sensitivity,
}

impl ResolvedConfig {
    /// Resolve configuration with full precedence hierarchy
    pub fn resolve(
        source_id: Option<&str>,
        cli_overrides: Option<&ComplexityThresholds>,
    ) -> Self {
        // Start with code defaults
        let mut config = ResolvedConfig::defaults();

        // Load and merge config file
        if let Ok(file_config) = load_config_toml() {
            // Merge global section [ai.pathfinder]
            if let Some(global) = file_config.pathfinder {
                config.merge_from_thresholds(&global);
            }

            // Merge source-specific section [sources."source_id"]
            if let Some(source_id) = source_id {
                if let Some(sources) = file_config.sources {
                    if let Some(source_config) = sources.get(source_id) {
                        config.merge_from_thresholds(source_config);
                    }
                }
            }
        }

        // Merge environment variables
        config.merge_from_env();

        // Merge CLI overrides (highest priority)
        if let Some(cli_overrides) = cli_overrides {
            config.merge_from_thresholds(cli_overrides);
        }

        config
    }

    /// Merge Option<T> fields from another config, only overriding
    /// fields that are explicitly set (Some)
    fn merge_from_thresholds(&mut self, other: &ComplexityThresholds) {
        if let Some(val) = other.recommend_python_regex_chars {
            self.recommend_python_regex_chars = val;
        }
        if let Some(val) = other.recommend_python_capture_groups {
            self.recommend_python_capture_groups = val;
        }
        if let Some(val) = other.force_python_regex_chars {
            self.force_python_regex_chars = val;
        }
        if let Some(val) = other.force_python_capture_groups {
            self.force_python_capture_groups = val;
        }
        if let Some(val) = other.prefer_yaml {
            self.prefer_yaml = val;
        }
        if let Some(ref val) = other.sensitivity {
            self.sensitivity = Sensitivity::from_str(val).unwrap_or(Sensitivity::Strict);
        }
    }

    fn merge_from_env(&mut self) {
        if let Ok(val) = std::env::var("CASPARIAN_PREFER_PYTHON_REGEX_CHARS") {
            if let Ok(num) = val.parse::<u32>() {
                self.recommend_python_regex_chars = num;
            }
        }
        // ... repeat for other env vars
    }

    pub fn defaults() -> Self {
        ResolvedConfig {
            recommend_python_regex_chars: 100,
            recommend_python_capture_groups: 5,
            force_python_regex_chars: 200,
            force_python_capture_groups: 10,
            prefer_yaml: true,
            sensitivity: Sensitivity::Strict,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    Strict,
    Loose,
}

impl Sensitivity {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "strict" => Ok(Sensitivity::Strict),
            "loose" => Ok(Sensitivity::Loose),
            _ => Err(format!("Invalid sensitivity: {}", s)),
        }
    }
}
```

---

## 5. Configuration File Schema (Unified)

Both old `[complexity]` and new `[ai.pathfinder]` sections are supported for backward compatibility.

### 5.1 Canonical Schema

```toml
# ~/.casparian_flow/config.toml

# ============================================================================
# AI WIZARDS CONFIGURATION
# ============================================================================

# [ai.pathfinder] - Recommended location for pathfinder complexity thresholds
[ai.pathfinder]
recommend_python_regex_chars = 100
recommend_python_capture_groups = 5
force_python_regex_chars = 200
force_python_capture_groups = 10
prefer_yaml = true
sensitivity = "strict"

# [ai.models] - Global model configuration
[ai.models]
model = "claude-3-5-sonnet"
provider = "anthropic"
temperature = 0.7
max_tokens = 4096

# [security] - Global security settings
[security]
redact_level = "high"
allow_http = false

# ============================================================================
# SOURCE-SPECIFIC OVERRIDES
# ============================================================================

# [sources."source_name"] - Per-source configuration
# Use source ID from discover mode (from sources table)

[sources."sales_data"]
# Override complexity thresholds for this source
recommend_python_regex_chars = 150
prefer_yaml = false
sensitivity = "loose"

# Override model for this source
[sources."sales_data".models]
model = "claude-3-opus"
temperature = 0.5

# Override security for this source
[sources."sales_data".security]
redact_level = "medium"

[sources."logs"]
# Another source with different settings
sensitivity = "loose"
```

### 5.2 Backward Compatibility

The old `[complexity]` section is deprecated but still supported:

```toml
# OLD (deprecated but still works)
[complexity]
recommend_python_regex_chars = 100

# NEW (recommended)
[ai.pathfinder]
recommend_python_regex_chars = 100
```

**Resolution:** If both exist, `[ai.pathfinder]` takes precedence over `[complexity]`.

---

## 6. Missing Config File Behavior

### 6.1 What Happens When Config File Doesn't Exist

```
~/.casparian_flow/config.toml does NOT exist
        │
        ▼
Silent operation
- Use code defaults for all settings
- Write INFO-level log: "Using default configuration. Customize at ~/.casparian_flow/config.toml"
- NO error, NO failure
- NO prompt to create file
```

### 6.2 First Run Experience

When a user runs Casparian Flow for the first time:

1. **Detect missing config** - On first invocation of any command
2. **Create directory** - `mkdir -p ~/.casparian_flow` (if needed)
3. **Check for config** - Look for `config.toml`
4. **Use defaults** - If not found, use code defaults silently
5. **Log discovery message**:
   ```
   INFO: Using default configuration.
         Create ~/.casparian_flow/config.toml to customize settings.
         Run 'casparian config init' to generate a template.
   ```

### 6.3 Config Initialization

Add a new CLI command to help users create a config file:

```bash
casparian config init
# Creates ~/.casparian_flow/config.toml with well-documented template
# If file exists, shows:
#   "Config already exists at ~/.casparian_flow/config.toml"
#   Use --overwrite to replace
```

---

## 7. Missing Individual Settings Behavior

When config file exists but is incomplete:

### 7.1 Example Scenario

```toml
# ~/.casparian_flow/config.toml (incomplete)

[ai.pathfinder]
prefer_yaml = false
# Note: missing recommend_python_regex_chars, etc.
```

**Result:**
- `prefer_yaml` = false (from config)
- `recommend_python_regex_chars` = 100 (from code default)
- `recommend_python_capture_groups` = 5 (from code default)
- `force_python_regex_chars` = 200 (from code default)
- `force_python_capture_groups` = 10 (from code default)
- `sensitivity` = "strict" (from code default)

**Behavior:** Partial config is valid. Missing fields fall back to code defaults.

### 7.2 Validation

All missing fields should log at DEBUG level with their resolved value:

```
DEBUG: Resolved configuration:
  prefer_yaml: false (from config.toml [ai.pathfinder])
  recommend_python_regex_chars: 100 (from code default)
  recommend_python_capture_groups: 5 (from code default)
  force_python_regex_chars: 200 (from code default)
  force_python_capture_groups: 10 (from code default)
  sensitivity: strict (from code default)
```

---

## 8. Override Rules (Detailed)

### 8.1 Global Config vs Source-Specific Config

```
Scenario: User has global [ai.pathfinder] and source-specific [sources."sales"]

casparian pathfinder --source sales --prefer-python

Resolution:
1. Load code default: prefer_yaml = true
2. Load [ai.pathfinder]: prefer_yaml = false (if present)
3. Load [sources."sales"]: prefer_yaml = true (if present, overrides step 2)
4. Load CLI: --prefer-python → prefer_yaml = false (overrides step 3)

Result: prefer_yaml = false (from CLI flag, wins all conflicts)
```

### 8.2 Conflicting CLI Flags

**Mutual exclusivity:** `--prefer-python` and `--prefer-yaml` cannot both be specified.

```bash
# INVALID - Error at parse time
casparian pathfinder --prefer-python --prefer-yaml

# VALID - One or the other
casparian pathfinder --prefer-python
casparian pathfinder --prefer-yaml
casparian pathfinder  # Neither specified, uses config
```

### 8.3 Threshold Validation

When loading thresholds, enforce constraint: `recommend < force`

```rust
fn validate_thresholds(config: &ResolvedConfig) -> Result<(), String> {
    if config.recommend_python_regex_chars > config.force_python_regex_chars {
        return Err(format!(
            "Invalid config: recommend_python_regex_chars ({}) must be <= force_python_regex_chars ({})",
            config.recommend_python_regex_chars, config.force_python_regex_chars
        ));
    }
    if config.recommend_python_capture_groups > config.force_python_capture_groups {
        return Err(format!(
            "Invalid config: recommend_python_capture_groups ({}) must be <= force_python_capture_groups ({})",
            config.recommend_python_capture_groups, config.force_python_capture_groups
        ));
    }
    Ok(())
}

// Call during initialization
let config = ResolvedConfig::resolve(source_id, cli_overrides);
validate_thresholds(&config)?;
```

---

## 9. Environment Variable Support (Optional Enhancement)

Support environment variables as a middle layer between file and CLI:

| Code Default | Config File | Env Variable | CLI Flag | Precedence |
|--------------|------------|-------------|----------|------------|
| `prefer_yaml = true` | `[ai.pathfinder] prefer_yaml = false` | `CASPARIAN_PREFER_PYTHON=true` | `--prefer-yaml` | CLI > Env > Config > Code |

### 9.1 Environment Variable Naming

```
CASPARIAN_PREFER_PYTHON          # Boolean: override prefer_yaml
CASPARIAN_RECOMMEND_REGEX_CHARS  # Number: override threshold
CASPARIAN_RECOMMEND_CAPTURE_GROUPS
CASPARIAN_FORCE_REGEX_CHARS
CASPARIAN_FORCE_CAPTURE_GROUPS
CASPARIAN_SENSITIVITY           # "strict" or "loose"
CASPARIAN_MODEL                 # Model name override
CASPARIAN_API_PROVIDER          # Provider override
```

### 9.2 Parsing Rules

- Boolean env vars: `true`, `1`, `yes` (case-insensitive) = true; otherwise false
- Number env vars: Must parse as u32; error if invalid
- String env vars: Used as-is; must be valid enum value (e.g., "strict", "loose")

---

## 10. Documentation Rules

### 10.1 Code Comments

Every config-related function must document the precedence:

```rust
/// Load complexity thresholds with full precedence resolution.
///
/// **Precedence (lowest to highest):**
/// 1. Code defaults (hardcoded constants)
/// 2. Global config file section [ai.pathfinder]
/// 3. Source-specific override [sources."source_id"]
/// 4. Environment variables (CASPARIAN_* prefix)
/// 5. CLI flags (highest priority)
///
/// **Example:**
/// ```ignore
/// let config = ResolvedConfig::resolve(Some("sales_data"), Some(&cli_args));
/// // prefer_yaml = true (code) → false (config) → true (source) → false (CLI)
/// ```
pub fn resolve(...) -> ResolvedConfig { ... }
```

### 10.2 User-Facing Documentation

Create a dedicated page in the CLI help:

```bash
casparian config --help
# Output:
#
# Usage: casparian config <COMMAND>
#
# Commands:
#   init    Generate a template ~/.casparian_flow/config.toml
#   show    Display current resolved configuration
#   validate Check config.toml for errors
#
# CONFIGURATION PRECEDENCE:
#
#   Casparian Flow resolves configuration in this order:
#
#   1. CODE DEFAULTS - Built-in values (lowest priority)
#   2. CONFIG FILE - Global settings in ~/.casparian_flow/config.toml
#   3. SOURCE OVERRIDES - Per-source settings in config.toml
#   4. ENVIRONMENT - CASPARIAN_* environment variables
#   5. CLI FLAGS - Command-line arguments (highest priority)
#
#   Example:
#     Code default: prefer_yaml = true
#     Config has: prefer_yaml = false
#     CLI has: --prefer-python
#     Final value: prefer_yaml = false (CLI flag wins)
```

---

## 11. Testing Strategy

### 11.1 Unit Tests for Resolution

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_defaults_only() {
        // No config file, no CLI, no env
        let config = ResolvedConfig::resolve(None, None);
        assert_eq!(config.recommend_python_regex_chars, 100);
        assert_eq!(config.prefer_yaml, true);
    }

    #[test]
    fn test_config_file_overrides_code_default() {
        // Setup: config file has [ai.pathfinder] prefer_yaml = false
        let toml = r#"[ai.pathfinder]\nprefer_yaml = false"#;
        mock_config_file(toml);

        let config = ResolvedConfig::resolve(None, None);
        assert_eq!(config.prefer_yaml, false);
    }

    #[test]
    fn test_source_override_overrides_global_config() {
        let toml = r#"
            [ai.pathfinder]
            prefer_yaml = true

            [sources."sales_data"]
            prefer_yaml = false
        "#;
        mock_config_file(toml);

        let config = ResolvedConfig::resolve(Some("sales_data"), None);
        assert_eq!(config.prefer_yaml, false);
    }

    #[test]
    fn test_cli_overrides_all() {
        let toml = r#"
            [ai.pathfinder]
            prefer_yaml = true

            [sources."sales_data"]
            prefer_yaml = true
        "#;
        mock_config_file(toml);

        let cli_overrides = ComplexityThresholds {
            prefer_yaml: Some(false),
            ..Default::default()
        };

        let config = ResolvedConfig::resolve(
            Some("sales_data"),
            Some(&cli_overrides),
        );
        assert_eq!(config.prefer_yaml, false);
    }

    #[test]
    fn test_missing_config_file_uses_defaults() {
        // No config file exists
        mock_no_config_file();

        let config = ResolvedConfig::resolve(None, None);
        assert_eq!(config.recommend_python_regex_chars, 100);
        assert_eq!(config.prefer_yaml, true);
    }

    #[test]
    fn test_partial_config_merges_with_defaults() {
        // Config file only has one setting
        let toml = r#"[ai.pathfinder]\nprefer_yaml = false"#;
        mock_config_file(toml);

        let config = ResolvedConfig::resolve(None, None);
        assert_eq!(config.prefer_yaml, false);  // From config
        assert_eq!(config.recommend_python_regex_chars, 100);  // From code default
    }

    #[test]
    fn test_threshold_validation() {
        // recommend > force should error
        let config = ResolvedConfig {
            recommend_python_regex_chars: 300,
            force_python_regex_chars: 200,
            ..Default::default()
        };

        let result = validate_thresholds(&config);
        assert!(result.is_err());
    }
}
```

### 11.2 Integration Tests

```rust
#[tokio::test]
async fn test_full_resolution_workflow() {
    // Create temp config file
    let config_content = r#"
        [ai.pathfinder]
        prefer_yaml = false
        recommend_python_regex_chars = 150

        [sources."test_source"]
        prefer_yaml = true
    "#;
    create_temp_config(config_content);

    // Simulate command: casparian pathfinder --source test_source
    let config = load_complexity_config(Some("test_source"));

    assert_eq!(config.prefer_yaml, true);  // source override
    assert_eq!(config.recommend_python_regex_chars, 150);  // global config
}
```

---

## 12. Decision Summary

| Question | Decision | Rationale |
|----------|----------|-----------|
| **How many precedence levels?** | 5 (Code, Global Config, Source Config, Env, CLI) | Covers all use cases without excessive complexity |
| **Behavior when config missing?** | Use code defaults silently | Zero-friction first run; users opt-in to customization |
| **Partial configs allowed?** | Yes, missing fields use code defaults | Flexibility; users only override what matters |
| **Source-specific configs?** | Optional, at same level as global | Useful for teams with heterogeneous data sources |
| **CLI flags highest priority?** | Yes, always | Necessary for one-off overrides and testing |
| **Config file location?** | `~/.casparian_flow/config.toml` | Platform-standard, centralized, matches other settings |
| **Format?** | TOML | Human-readable, typed, standard for Rust projects |
| **Backward compatibility?** | Support both `[ai.pathfinder]` and `[complexity]` | Smooth migration path; no breaking changes |
| **Validation?** | Enforce `recommend < force` | Prevents nonsensical configurations |
| **Documentation?** | Code comments + user guide + help system | Multiple audiences (developers, users, advanced users) |

---

## 13. Implementation Checklist

- [ ] Define `ResolvedConfig` struct with defaults
- [ ] Implement `ComplexityThresholds::merge_with()` method
- [ ] Implement `ResolvedConfig::resolve()` with full precedence
- [ ] Add TOML parsing for config file (use `toml` crate with serde)
- [ ] Implement environment variable support (`CASPARIAN_*` parsing)
- [ ] Add config file validation (threshold constraints)
- [ ] Add `casparian config init` command
- [ ] Add `casparian config show` command to display resolved values
- [ ] Add unit tests for all precedence scenarios
- [ ] Add integration tests for full workflow
- [ ] Update CLI help with precedence documentation
- [ ] Create user guide for config.toml customization
- [ ] Add debug logging for resolution chain (each level)
- [ ] Handle missing config gracefully (INFO log, not error)
- [ ] Test backward compatibility with `[complexity]` section
- [ ] Document in CLAUDE.md and specs

---

## 14. Related Documents

- **Main Spec:** `/Users/shan/workspace/casparianflow/specs/ai_wizards.md` Section 3.1.3
- **Previous Gap Resolution:** `/Users/shan/workspace/casparianflow/specs/meta/sessions/ai_wizards/round_018/engineer.md` (Complexity Thresholds Configuration)
- **Architecture:** `/Users/shan/workspace/casparianflow/ARCHITECTURE.md`

---

## 15. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial resolution: 5-level precedence hierarchy, config file schema, environment variables, validation, testing strategy, implementation checklist |


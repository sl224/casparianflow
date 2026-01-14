# Reviewer Assessment: GAP-CONFIG-001

## Verdict: APPROVED_WITH_NOTES

---

## Summary

The Engineer's proposal for GAP-CONFIG-001 provides a **comprehensive and methodical resolution** to configuration precedence ambiguity in Casparian Flow. The proposal establishes a clear 5-level precedence hierarchy, detailed resolution algorithms, concrete configuration file schema, and a thorough implementation checklist. The approach is well-grounded in established configuration management patterns and includes practical guidance for handling edge cases.

**Strengths:**
- Excellent specification clarity with 5-level precedence hierarchy (Code → Config File → Source Override → Env → CLI)
- Comprehensive implementation guidance with pseudocode and Rust type structures
- Practical examples demonstrating resolution chains at multiple levels
- Strong testing strategy covering unit tests, integration tests, and edge cases
- Graceful handling of missing/incomplete config files ("silent defaults" principle)
- Backward compatibility considerations for legacy `[complexity]` section

**Concerns:**
- Proposal assumes configuration system doesn't yet exist (risk: may already be partially implemented)
- Environment variable support marked "optional" but included in testing—scope clarity needed
- Validation logic and error messages not fully detailed
- No guidance on logging/debugging configuration resolution chains
- Implementation order/prioritization not specified

**Recommendation:**
Approve with minor clarifications on implementation scope and environment variable opt-in status.

---

## Checklist

| Item | Status | Notes |
|------|--------|-------|
| **Problem Statement** | ✅ Clear | Ambiguity between code/config defaults well articulated; examples show real confusion points |
| **Specification Alignment** | ✅ Strong | Aligns with "zero-friction first run" principle in CLAUDE.md |
| **Precedence Model** | ✅ Sound | 5-level hierarchy covers all typical use cases; each level has clear purpose |
| **Config Schema** | ✅ Complete | TOML structure is well-designed; shows both `[ai.pathfinder]` and deprecated `[complexity]` |
| **Resolution Algorithm** | ✅ Present | Pseudocode, Rust implementation, and merge semantics all provided |
| **Missing Config Behavior** | ✅ Good | "Silent defaults" approach is user-friendly; logs discovery message at INFO level |
| **Partial Config Handling** | ✅ Good | Falls through to code defaults for unspecified fields; DEBUG logging shows resolution |
| **Source-Specific Overrides** | ✅ Present | Three-level merge (global config → source config → CLI) demonstrated with examples |
| **Environment Variables** | ⚠️ Inconsistent | Marked "optional enhancement" in intro, but included in precedence diagram and tests |
| **Validation Strategy** | ⚠️ Incomplete | Threshold constraint (`recommend < force`) defined, but error messages not specified |
| **Testing Coverage** | ✅ Strong | Unit tests for all precedence scenarios + integration test provided |
| **Documentation Rules** | ✅ Good | Code comments and user-facing help templates included |
| **CLI Commands** | ✅ Present | `casparian config init` and `casparian config show` proposed |
| **Backward Compatibility** | ✅ Good | Supports both `[ai.pathfinder]` and `[complexity]` sections |
| **Implementation Checklist** | ✅ Complete | 14-item checklist provided; ready for task breakdown |
| **Glossary/References** | ✅ Present | Related documents linked; revision history included |

---

## Detailed Findings

### Strengths

**1. Precedence Hierarchy is Clear and Complete**

The 5-level hierarchy (Section 2) elegantly covers all configuration sources:

```
Code Defaults (hardcoded)
    ↓ overridden by ↓
Global Config File ([ai.pathfinder])
    ↓ overridden by ↓
Source-Specific Config ([sources."name"])
    ↓ overridden by ↓
Environment Variables (CASPARIAN_*)
    ↓ overridden by ↓
CLI Flags (highest priority)
```

**Why this is strong:**
- Each level has a clear purpose (code = defaults, file = customization, env = automation, CLI = one-off overrides)
- Levels are orthogonal—each only overrides what it explicitly specifies
- Examples demonstrate real-world scenarios (Example 3.1.2 shows `prefer_yaml` resolution across 4 levels)
- Pattern matches industry standard (Docker Compose, Kubernetes, Hashicorp Consul)

**Evidence:** Section 2 and Section 3 provide clear examples; Section 3.1.2 demonstrates full resolution chain.

---

**2. Implementation Guidance is Production-Ready**

The pseudocode (Section 4.1) and Rust implementation (Section 4.2) are concrete enough to implement:

- `ComplexityThresholds` struct uses `Option<T>` to distinguish "explicitly set" vs "inherited"
- `ResolvedConfig` struct uses non-Option fields for final resolved values
- `merge_from_thresholds()` method properly implements optional field merging
- `merge_from_env()` and `validate_thresholds()` functions included

**Why this is strong:**
- Type-level distinction between partial and resolved configs prevents mixing concerns
- The Option-based approach avoids "magic values" (e.g., -1 = unset)
- Merge semantics are explicit (if Some, override; otherwise skip)
- Validation logic (`recommend < force` constraint) is testable

**Evidence:** Section 4.2 shows complete Rust types; `merge_from_thresholds()` method at lines 240-259 is clear.

---

**3. Missing/Incomplete Config File Handling is User-Friendly**

Section 6 and 7 demonstrate graceful degradation:

- **Missing config file:** Uses code defaults silently, logs INFO message suggesting `~/.casparian_flow/config.toml`
- **Incomplete config file:** Missing fields fall through to code defaults; DEBUG logs show resolution source
- **First run experience:** Directory created on demand; user not prompted; discovery message on first invocation

**Why this is strong:**
- "Zero-friction first run" principle (from CLAUDE.md) is honored
- Users discover customization optionally ("Create config.toml to customize")
- No breaking changes if config file is deleted (just reverts to defaults)
- Partial configs are valid (users don't need to copy all defaults)

**Evidence:** Section 6.1-6.3 detail behavior; Section 7.2 shows DEBUG logging output.

---

**4. Backward Compatibility Strategy is Thoughtful**

Section 5.2 addresses migration from old `[complexity]` section:

- Both `[ai.pathfinder]` and `[complexity]` sections are supported
- Resolution order: `[ai.pathfinder]` takes precedence if both exist
- No breaking changes; users can migrate at their own pace

**Why this is strong:**
- Protects existing users who may have `[complexity]` in their config
- Clear migration path without forced upgrade
- Avoids "big bang" breaking change
- Test case included (Section 11.1, line 689)

**Evidence:** Section 5.2 and test at line 689-695 show dual support.

---

**5. Testing Strategy Covers All Scenarios**

Section 11 provides 8 unit tests + 1 integration test:

| Test | Scenario | Validates |
|------|----------|-----------|
| test_code_defaults_only | No config, no CLI, no env | Code defaults apply |
| test_config_file_overrides | Config file present | File overrides code |
| test_source_override_overrides_global_config | Source config present | Source overrides global |
| test_cli_overrides_all | All levels present | CLI wins all conflicts |
| test_missing_config_file_uses_defaults | No config file | Graceful fallback |
| test_partial_config_merges_with_defaults | Config has 1 field | Others from code |
| test_threshold_validation | Invalid threshold config | Validation catches error |
| (integration test) | Full workflow with temp config | All layers resolve correctly |

**Why this is strong:**
- Tests cover happy path, error cases, and precedence conflicts
- No mocks—tests use real config files (integration style)
- Validation error cases are tested
- Partial config is explicitly tested

**Evidence:** Section 11.1 and 11.2 provide complete test code.

---

**6. Source-Specific Overrides are Well-Explained**

Section 3 and 8.1 demonstrate per-source configuration:

```toml
[ai.pathfinder]
prefer_yaml = true          # Global: all sources use YAML

[sources."sales_data"]
prefer_yaml = false         # Override: sales_data uses Python
```

The example in Section 8.1 shows full resolution:
1. Code default: `prefer_yaml = true`
2. Load global config: `prefer_yaml = false` (if in file)
3. Load source-specific: `prefer_yaml = true` (if in file, overrides #2)
4. Load CLI: `--prefer-python` (overrides #3)
5. Result: `prefer_yaml = false` (from CLI flag)

**Why this is strong:**
- Teams with heterogeneous data sources can customize per-source
- Useful for multi-tenant or multi-environment deployments
- Example shows real-world conflict resolution
- No ambiguity about which config "wins"

**Evidence:** Section 8.1 demonstrates full example; Table in Section 3.1 shows per-setting override hierarchy.

---

### Concerns

**1. Environment Variable Status is Ambiguous**

The proposal labels Section 9 as "**Optional Enhancement**" but then:

- Includes CASPARIAN_* env vars in the precedence diagram (Section 2, Level 4)
- Adds env variable parsing to `merge_from_env()` (Section 4.2, lines 261-268)
- Includes environment variable tests in the test plan (Section 11.1, implied but not shown)
- Lists environment variable support in the Decision Summary (Section 12, row 1)

**The ambiguity:**
- Is this a required feature or optional?
- If optional, should it be behind a feature flag?
- If required, why is it marked "optional"?

**Impact:**
- Implementation team won't know whether to build env var support or defer it
- Scope of work is unclear (with or without env vars?)

**Recommendation:**
Clarify in Section 9 header:
```markdown
## 9. Environment Variable Support [REQUIRED / OPTIONAL]

[If REQUIRED:]
The configuration system MUST support CASPARIAN_* environment variables
as a middle layer between config file and CLI flags, for automation
and CI/CD use cases. This is NOT optional.

[If OPTIONAL:]
This is a future enhancement. Implementation checklist (Section 13)
should exclude env var tasks if deferring. Mark clearly as "Phase 2".
```

---

**2. Validation Error Messages Not Specified**

Section 8.3 defines the validation function but doesn't specify error messages:

```rust
fn validate_thresholds(config: &ResolvedConfig) -> Result<(), String> {
    if config.recommend_python_regex_chars > config.force_python_regex_chars {
        return Err(format!(
            "Invalid config: recommend_python_regex_chars ({}) must be <= force_python_regex_chars ({})",
            config.recommend_python_regex_chars, config.force_python_regex_chars
        ));
    }
    // ...
}
```

**What's missing:**
- How does the error propagate to the user? (Panic? Log? CLI error?)
- What happens during CLI flag parsing if validation fails?
- Should invalid threshold config prevent the tool from running?

**Scenarios not covered:**
- User has valid config file, then runs `--recommend-regex-chars 300 --force-regex-chars 200` → Which error wins?
- Config file has invalid thresholds → Should startup fail or should it warn and use code defaults?

**Recommendation:**
Add Section 8.4: "Error Handling Strategy"
```markdown
## 8.4 Validation Error Handling

When validation fails:

1. **During config resolution:** Return Result<ResolvedConfig, ConfigError>
2. **Error message format:**
   ```
   Configuration Error: Invalid complexity thresholds
   recommend_python_regex_chars (300) must be <= force_python_regex_chars (200)

   Fix by either:
   - Lower recommend_python_regex_chars to <= 200
   - Raise force_python_regex_chars to >= 300
   - See ~/.casparian_flow/config.toml [ai.pathfinder]
   ```
3. **Startup behavior:** Tool should not proceed with invalid config
4. **CLI override validation:** Validate merged config after CLI flags applied
```

---

**3. Logging/Debugging Configuration Resolution is Underspecified**

Section 7.2 mentions DEBUG logging but doesn't give structure:

```
DEBUG: Resolved configuration:
  prefer_yaml: false (from config.toml [ai.pathfinder])
  recommend_python_regex_chars: 100 (from code default)
  ...
```

**What's missing:**
- Log level strategy: What goes to DEBUG vs INFO vs WARN?
- Log format: Should it be structured (JSON) or human-readable?
- When is logging output? (On every run? Only on first run? Only with `--debug` flag?)
- Can users export resolved config for debugging? (e.g., `casparian config show` command)

**Impact:**
- Users won't know why their config isn't applying
- Debugging "why did the tool pick YAML instead of Python?" becomes hard
- Support tickets will escalate unnecessarily

**Recommendation:**
Add Section 7.3: "Debug Logging Strategy"
```markdown
## 7.3 Debug Logging Strategy

Configuration resolution should emit structured logs at multiple levels:

**INFO level (always):**
- "Using default configuration. Create ~/.casparian_flow/config.toml to customize."
- "Config file loaded from ~/.casparian_flow/config.toml"

**DEBUG level (when --debug flag or RUST_LOG=debug):**
- "Resolution chain for 'prefer_yaml':"
  - "  1. Code default: true"
  - "  2. Global config [ai.pathfinder]: false"
  - "  3. Source config [sources.'sales']: true"
  - "  4. Environment var CASPARIAN_PREFER_PYTHON: not set"
  - "  5. CLI flag --prefer-yaml: not provided"
  - "  Final: true (from source config)"
- "Validation: threshold constraints OK"

**Commands for users:**
- `casparian config show` - Display fully resolved configuration
- `casparian config show --source sales` - Show for specific source
- `RUST_LOG=debug casparian pathfinder ...` - Full debug trace
```

---

**4. No Guidance on Configuration Discovery/Introspection**

The proposal defines `casparian config init` and `casparian config show` commands (Section 6.3) but doesn't specify what they output:

**Missing details:**
- `casparian config init` creates a template, but what does it contain? (Full example? Commented sections?)
- `casparian config show` displays "resolved configuration" but in what format? (Table? JSON? TOML?)
- Should there be a `casparian config validate` command?
- How does a user see where a specific value came from? (e.g., "why is prefer_yaml = false?")

**Impact:**
- Implementation team won't know what the commands should output
- Users won't have good way to debug configuration issues

**Recommendation:**
Add Section 6.4: "Configuration Commands"
```markdown
## 6.4 Configuration Commands

### casparian config init
Creates a template ~/.casparian_flow/config.toml with all available options.

Output: Well-commented TOML file with:
- Section headers explaining each option
- Example values
- Default values shown in comments
- Links to docs for each setting

Example output:
```toml
# ~/.casparian_flow/config.toml
#
# Casparian Flow Configuration
# See: https://docs.casparian.io/config
#
# Code defaults are used for any values not specified here.

# AI Wizards - Pathfinder complexity thresholds
[ai.pathfinder]
# recommend_python_regex_chars = 100     # Default: 100
# prefer_yaml = true                      # Default: true
```

### casparian config show
Display fully resolved configuration with source attribution.

Output: Table or JSON showing:
- Setting name
- Resolved value
- Source (code default, config file, environment, CLI, or source-specific)
- Config section where value came from (e.g., "[ai.pathfinder]")

Example:
```
SETTING                         VALUE    SOURCE              LOCATION
prefer_yaml                     false    config file         [ai.pathfinder]
recommend_python_regex_chars    100      code default        (builtin)
```

### casparian config validate
Check config.toml for errors without running a command.

Output: Pass/fail with detailed error messages for invalid settings.
```

---

**5. No Implementation Order/Prioritization**

Section 13 provides a 14-item implementation checklist but doesn't specify:
- Which items are prerequisites for others?
- Which can be parallelized?
- What's the critical path?

**Example questions:**
- Should TOML parsing be added before or after ResolvedConfig struct?
- Can `casparian config init` be deferred until after core resolution is working?
- Should tests be written before or after code implementation?

**Impact:**
- Implementation team will need to infer ordering from structure
- Risk of blocked work or rework if dependencies are missed

**Recommendation:**
Add Section 13.1: "Implementation Order and Critical Path"
```markdown
## 13.1 Implementation Order and Critical Path

The checklist can be grouped into phases:

**Phase 1: Core Types and Resolution (3-4 days)**
- [ ] Define `ResolvedConfig` struct with defaults
- [ ] Define `ComplexityThresholds` struct (partial config)
- [ ] Implement `ResolvedConfig::defaults()` method
- [ ] Implement `merge_from_thresholds()` method
- [ ] Add unit tests for code defaults only (test_code_defaults_only)

**Phase 2: Config File Loading (2-3 days, depends on Phase 1)**
- [ ] Add TOML parsing (use `toml` crate with serde)
- [ ] Load global config file section [ai.pathfinder]
- [ ] Load source-specific section [sources."name"]
- [ ] Implement `load_config_toml()` function
- [ ] Add unit tests for file loading (test_config_file_overrides*)

**Phase 3: Environment Variables (1-2 days, depends on Phase 1)**
- [ ] Implement environment variable support (CASPARIAN_* parsing)
- [ ] Implement `merge_from_env()` method
- [ ] Add unit tests for env var precedence

**Phase 4: Validation and Error Handling (2 days, depends on Phase 1)**
- [ ] Implement threshold validation (recommend < force)
- [ ] Add error handling with user-friendly messages
- [ ] Add unit test for validation errors

**Phase 5: CLI Commands (2-3 days, depends on Phases 1-2)**
- [ ] Implement `casparian config init` command
- [ ] Implement `casparian config show` command
- [ ] Implement `casparian config validate` command

**Phase 6: Integration and Testing (2-3 days, depends on all)**
- [ ] Write integration test (full workflow)
- [ ] Test backward compatibility with [complexity]
- [ ] Test with real config files
- [ ] Add debug logging for configuration resolution

Critical path: Phase 1 → Phase 2 → Phase 5 (5-10 days)
Optional fast path: Skip Phase 3 (env vars) until later
```

---

**6. No Discussion of Configuration Scope/Applicability**

The proposal focuses on "complexity thresholds" for Pathfinder wizard, but:

**Questions not answered:**
- Are there other configuration settings beyond complexity thresholds?
- Should this same precedence hierarchy apply to model selection (`claude-3-5-sonnet` vs `claude-opus`)?
- What about security settings like `redact_level`?
- Are there per-command configurations (e.g., `casparian scan --exclude-pattern`) that use this system?

**Current proposal scope:** Tables 3.1, 3.2, 3.3 show Complexity, Model, and Security settings, but the core algorithm (Section 4) is written only for `ComplexityThresholds`.

**Impact:**
- Implementation may need to be generalized to handle all config types
- Refactoring risk if structure doesn't work for all settings

**Recommendation:**
Add Section 3.4: "Configuration Scope and Extensibility"
```markdown
## 3.4 Configuration Scope and Extensibility

This proposal establishes the precedence hierarchy for ALL configuration in Casparian Flow,
not just complexity thresholds. Future configuration additions must follow this same hierarchy:

1. Code defaults
2. Global config file [ai.*, security, ...]
3. Source-specific config [sources."name".*, sources."name".security, ...]
4. Environment variables (CASPARIAN_*)
5. CLI flags

Current supported settings:
- Complexity thresholds (Section 3.1): Pathfinder configuration
- Model configuration (Section 3.2): LLM model and parameters
- Security settings (Section 3.3): Redaction and privacy

Generic resolution function: The `ResolvedConfig::resolve()` method should be
generalized to a trait-based system to avoid code duplication:

```rust
trait ConfigResolvable: Clone {
    fn code_defaults() -> Self;
    fn merge_with(&mut self, other: &Self);
    fn from_env_prefix(prefix: &str) -> Self;
}

fn resolve_config<T: ConfigResolvable>(
    source_id: Option<&str>,
    section: &str,
) -> T { ... }
```

This allows ModelConfig, SecurityConfig, etc. to reuse the same resolution logic.
```

---

### Recommendations

**1. Clarify Environment Variable Status**

The proposal should explicitly state whether CASPARIAN_* environment variables are:
- **REQUIRED** (must implement in Phase 1)
- **OPTIONAL** (implement in Phase 2 if time allows)
- **DEFERRED** (plan for future release)

If OPTIONAL, mark Section 9 clearly and remove env vars from:
- Section 2 (precedence diagram)
- Section 4.2 (merge_from_env implementation)
- Section 8.4 (env var validation)
- Section 11 (integration test setup)

**Suggested edit:**
```markdown
## 9. Environment Variable Support (REQUIRED / DEFERRED)

[Choose one and edit accordingly]
```

---

**2. Add Validation Error Messages Section**

Before implementation begins, define what users see when config is invalid.

**Add to Section 8:**
```markdown
## 8.4 Validation Error Messages

When validation fails, the error message should:

1. Clearly state what is wrong (specific values)
2. Suggest how to fix it (remediation)
3. Point to where the setting is configured (file location)

Example 1: Config file error
```
Error: Invalid configuration in ~/.casparian_flow/config.toml

  [ai.pathfinder]
  recommend_python_regex_chars = 300
  force_python_regex_chars = 200    ← INVALID!

Problem: recommend_python_regex_chars (300) must be ≤ force_python_regex_chars (200)

Fix: Either increase force_python_regex_chars to at least 300, or
     decrease recommend_python_regex_chars to at most 200.

Tip: Use `casparian config validate` to check for errors.
```

Example 2: CLI flag error
```
Error: Invalid command-line flags

  casparian pathfinder \
    --recommend-regex-chars 300 \
    --force-regex-chars 200 \
                           ↑ INVALID!

Problem: --recommend-regex-chars (300) must be ≤ --force-regex-chars (200)

Fix: Adjust one or both flags to satisfy the constraint.
```
```

---

**3. Add Configuration Discovery/Introspection Commands**

Expand Section 6 with detailed command specifications:

```markdown
## 6.4 Configuration Commands (Detailed)

### casparian config init [--overwrite]

Generate a template configuration file with all available options.

Behavior:
- If ~/.casparian_flow/config.toml exists: Show path and exit (use --overwrite to replace)
- If ~/.casparian_flow/ doesn't exist: Create directory
- Generate well-commented TOML template
- Show success message with next steps

Output:
```
Configuration template created at ~/.casparian_flow/config.toml
Review and customize settings, then run:

  casparian config show      # See resolved values
  casparian config validate  # Check for errors
```

### casparian config show [--source SOURCE] [--format json|table]

Display resolved configuration with source attribution.

Behavior:
- Load all configuration sources (code defaults, file, env, etc.)
- Resolve with specified source context (if --source provided)
- Display each setting with its source
- Highlight differences from code defaults

Output (table format):
```
SETTING                          VALUE    SOURCE              CONFIG LOCATION
prefer_yaml                      false    config file         [ai.pathfinder]
recommend_python_regex_chars     100      code default        (builtin)
force_python_regex_chars         200      source override     [sources."sales"]
sensitivity                      strict   code default        (builtin)
```

Output (json format):
```json
{
  "prefer_yaml": {
    "value": false,
    "source": "config_file",
    "location": "[ai.pathfinder]"
  },
  ...
}
```

### casparian config validate [--strict]

Check configuration for errors without running a command.

Behavior:
- Load configuration file (if exists)
- Validate all settings (threshold constraints, enum values, etc.)
- Report all errors found
- Suggest fixes

Output:
```
Configuration validation: PASSED

All settings are valid:
- prefer_yaml: false (from [ai.pathfinder])
- recommend_python_regex_chars: 100 (valid: ≤ force)
- ... other settings ...

Tip: Use `casparian config show` to see resolved values.
```

Or on error:
```
Configuration validation: FAILED

Found 1 error:

[ai.pathfinder]
  Line 42: recommend_python_regex_chars = 300
           force_python_regex_chars = 200
           ↑ Invalid: recommend (300) must be ≤ force (200)

Fix: Adjust one of these values to satisfy the constraint.
File: ~/.casparian_flow/config.toml
```
```

---

**4. Add Implementation Dependency Graph**

Specify which items in Section 13 checklist depend on others:

```markdown
## 13.1 Implementation Dependencies

The checklist items have these dependencies:

**Independent (can start immediately):**
- [ ] Add TOML parsing for config file

**Depends on Config Struct:**
- [ ] Implement merge methods (need ComplexityThresholds struct first)
- [ ] Add environment variable support (need merge_with implementation)

**Depends on Parsing + Merging:**
- [ ] Implement ResolvedConfig::resolve() (needs all parsing ready)
- [ ] Add validation (needs fully resolved config)

**Depends on Core Resolution:**
- [ ] CLI commands: casparian config init/show/validate
- [ ] Debug logging for resolution chain
- [ ] Testing (all unit tests need core code)

**Critical Path (minimal dependencies):**
1. Define struct types (ComplexityThresholds, ResolvedConfig)
2. Implement defaults() and merge_with()
3. Add TOML parsing
4. Implement resolve() function
5. Add validation
6. Write tests
7. Add CLI commands

Estimated duration: 5-7 days for critical path.
Optional (can defer): env var support, config commands (Phase 2).
```

---

**5. Specify Configuration File Search/Precedence**

The proposal assumes config file is at `~/.casparian_flow/config.toml` but doesn't address:
- Can users override the config path? (e.g., `CASPARIAN_CONFIG=/etc/casparian.toml`)
- What if config file is in multiple locations? (home, project root, /etc)
- Should there be a `.casparian_flow` directory precedence? (like `.git` search)

**Recommendation:**
Add Section 6.5: "Configuration File Search Path"
```markdown
## 6.5 Configuration File Search Path

The configuration file is resolved in this order (first found wins):

1. Environment variable: CASPARIAN_CONFIG (if set)
   Example: `CASPARIAN_CONFIG=/etc/casparian/config.toml casparian pathfinder`

2. User home: ~/.casparian_flow/config.toml (recommended default)

3. System config (Linux/Mac only): /etc/casparian_flow/config.toml
   (if file exists and user has permission)

4. Not found: Use code defaults (no error)

Example:
```
CASPARIAN_CONFIG=/project/config.toml casparian pathfinder
# Loads /project/config.toml

casparian pathfinder
# Loads ~/.casparian_flow/config.toml (or defaults if missing)
```

Only the first file found is loaded. Do not merge multiple config files.
```

---

**6. Add Caching/Performance Consideration**

The proposal doesn't mention:
- Is config resolution cached? (Load once at startup or on every call?)
- Can config file be hot-reloaded? (Or does restart require new invocation?)
- Performance impact of reading/parsing config on every command?

**Recommendation:**
Add Section 4.3: "Configuration Caching and Performance"
```markdown
## 4.3 Configuration Caching and Performance

Configuration is resolved once per CLI invocation and cached:

- Config is loaded, merged, and validated at tool startup
- Same ResolvedConfig instance is used for entire invocation
- Changes to config.toml take effect on next command invocation
- No file watcher; config is not hot-reloaded during execution

Rationale: Configuration should be stable during a single command run.
If a user changes config.toml and re-runs the same command, they get
the new configuration. This prevents inconsistent behavior within a
single invocation.

Performance: Config file is read exactly once per invocation.
Parsing is cached in memory; no re-reads for multiple commands in
sequence.
```

---

## New Gaps Identified

While the proposal is comprehensive, it reveals or creates these new gaps:

**Gap 1: Configuration System Existing State Unknown**

The proposal assumes the configuration system is being built from scratch, but:
- Section `/Users/shan/workspace/casparianflow/crates/casparian/src/cli/config.rs` already exists
- It currently handles file paths only (database, output, venvs)
- Does it already have settings configuration? (Not found in code review)
- Is there existing TOML parsing infrastructure?

**Action:** Before implementation:
1. Audit `/crates/casparian/src/cli/config.rs` for existing code
2. Check if any TOML crate is already in Cargo.toml
3. Identify what can be reused vs. rebuilt
4. Update proposal scope if existing infrastructure exists

**Gap 2: Configuration Scope Beyond Complexity Thresholds**

The proposal mentions Model Configuration (Section 3.2) and Security Settings (Section 3.3), but:
- Are these actually used/needed?
- Are they part of this gap resolution or separate gaps?
- Do they use the same precedence hierarchy?
- Who will implement them?

**Action:** Clarify whether this proposal covers ALL configuration, or just complexity thresholds:
- If ALL: Update implementation checklist to cover all sections (3.1, 3.2, 3.3)
- If JUST thresholds: Move sections 3.2, 3.3 to "Future Work" and focus on Section 3.1

**Gap 3: No ER Diagram or State Machine for Configuration Resolution**

The proposal is excellent for referential understanding, but lacks:
- Entity-relationship diagram showing config file structure → in-memory types
- State machine showing transition from Code Defaults → Resolved Config
- Dependency graph showing what must load before what

**Action:** Create visual aids for implementation documentation:
```
Code Defaults (ResolvedConfig::defaults())
        ↓
    merge_from_file()
        ↓
    merge_from_env()
        ↓
    merge_from_cli()
        ↓
    validate_thresholds()
        ↓
Final ResolvedConfig (immutable)
```

**Gap 4: No Testing Strategy for Real File Scenarios**

Section 11 tests are thorough, but don't cover:
- What if config file is unreadable (permission denied)?
- What if config file is corrupted TOML?
- What if config directory doesn't exist yet? (First run)
- What if home directory can't be determined?

**Action:** Add error case tests:
```rust
#[test]
fn test_config_file_permission_denied() { ... }

#[test]
fn test_config_file_malformed_toml() { ... }

#[test]
fn test_casparian_home_directory_not_creatable() { ... }
```

**Gap 5: Documentation of TOML Dependencies and Versions**

The proposal mentions "use `toml` crate with serde" (Section 4.2) but:
- Which version of `toml` crate? (0.8? 0.9?)
- Which version of `serde`?
- Are these already in Cargo.toml?
- What about `serde_toml` alternative?

**Action:** Update Cargo.toml specification in proposal:
```toml
[dependencies]
toml = "0.8"           # TOML parsing
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"     # For JSON output of config show
dirs = "5.0"           # For home directory resolution (already used?)
```

**Gap 6: Configuration Precedence for Nested Settings**

The proposal handles flat settings well, but:
- What about nested config? (e.g., `[sources."sales".models]`)
- Does merge logic work for nested structs?
- How does `Some(None)` work? (Explicit null vs. missing?)

**Action:** Add Section on nested configuration precedence:
```markdown
## 8.2 Nested Configuration Precedence

For settings with nested structure (e.g., sources."sales".models):

Merge rules:
- Top-level field: If Some in higher layer, use it; else use lower layer
- Nested fields: Each field merges independently (not all-or-nothing)

Example:
```toml
[ai.models]
model = "claude-3-5-sonnet"
temperature = 0.7

[sources."sales".models]
temperature = 0.5        # Override only temperature, keep model from global
# (model is NOT overridden; it's inherited)
```

In code: Use separate structs for each nesting level, each with Option fields.
```

---

## Implementation Notes for Engineering Team

If approved, implementation should follow this sequence:

**Week 1:**
- [ ] Audit existing `crates/casparian/src/cli/config.rs` for code reuse
- [ ] Add `toml` and `serde` to Cargo.toml (if not present)
- [ ] Define `ResolvedConfig` and `ComplexityThresholds` structs (Section 4.2)
- [ ] Implement `defaults()` and `merge_from_thresholds()` methods
- [ ] Write unit tests for code defaults only (Section 11.1, test #1)

**Week 1-2:**
- [ ] Add TOML parsing with `load_config_toml()` (Section 4.1)
- [ ] Implement global config file loading
- [ ] Implement source-specific config loading
- [ ] Write unit tests for config file precedence (Section 11.1, tests #2-3)

**Week 2:**
- [ ] Implement validation function (Section 8.3)
- [ ] Add detailed error messages (new Section 8.4)
- [ ] Implement environment variable support (Section 9)
- [ ] Write unit tests for validation and env vars

**Week 2-3:**
- [ ] Add `casparian config init` command (generates template)
- [ ] Add `casparian config show` command (displays resolved config)
- [ ] Add `casparian config validate` command (checks for errors)
- [ ] Add debug logging for resolution chain (Section 7.3)

**Week 3:**
- [ ] Write integration test (Section 11.2)
- [ ] Test backward compatibility with `[complexity]` section
- [ ] Test error cases (malformed TOML, missing directory, etc.)
- [ ] Update CLAUDE.md and crate-specific docs

**Critical path:** Phases 1-2 can ship without CLI commands. Can deploy:
1. Core resolution (Week 1-2)
2. Validation (Week 2)
3. Integration tests passing

Then add CLI commands as Phase 2.

---

## References

**Specification:**
- `/Users/shan/workspace/casparianflow/CLAUDE.md` - Project principles ("zero-friction first run")
- `/Users/shan/workspace/casparianflow/specs/ai_wizards.md` - AI Wizards feature
  - Section 3.1: Pathfinder configuration needs
  - Section 3.1.1: Complexity thresholds decision points

**Code:**
- `/Users/shan/workspace/casparianflow/crates/casparian/src/cli/config.rs` - Existing config module
  - Lines 9-30: Path resolution functions
  - Lines 64-112: Current `casparian config` command (shows paths only)

**Related Gaps:**
- (None identified in current round, but may emerge during implementation)

**Standards and Patterns:**
- Docker Compose: Environment variable + file + CLI precedence model
- 12-Factor App: Config precedence (environment over file)
- Hashicorp Consul: Layered configuration merging

---

## References to Previous Review Sessions

- Round 026 Reviewer (GAP-MCP-001): Demonstrates high-quality output format taxonomy review
- Round 025 Reviewer: Shows pattern for approving comprehensive infrastructure changes
- CLAUDE.md: Primary source of truth for project principles and architecture

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-13 | 1.0 | Initial review: Verdict APPROVED_WITH_NOTES on 5-level precedence hierarchy, clear implementation guidance, strong testing strategy, comprehensive configuration file schema. Concerns: env var status ambiguous, validation error messages not specified, logging/debugging not detailed, no implementation prioritization. Recommendations: clarify env vars, add error message specs, add CLI command specs, add dependency graph, add nested config rules. Identified 6 new gaps. |


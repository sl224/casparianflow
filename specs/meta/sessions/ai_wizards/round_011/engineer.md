# Engineer Resolution: GAP-PRIVACY-001

## Path Normalization and Sensitive Data Protection

**Gap:** The Path Intelligence Engine (Section 3.5) performs clustering on file paths and sends paths to LLMs for analysis. Paths may contain sensitive information such as usernames, client names, internal project codes, and PHI indicators.

**Resolution:** Implement a multi-layer path sanitization system with configurable redaction rules and local-only mode support.

---

## 1. Path Sanitization Architecture

### 1.1 Three-Layer Sanitization Model

```
Raw Path
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Automatic Pattern Detection                       │
│  - Usernames (/home/jsmith/, /Users/jsmith/)               │
│  - PHI patterns (MRN, SSN, DOB indicators)                 │
│  - API keys/tokens (base64 strings, hex sequences)         │
│  Output: path with AUTOMATIC_REDACTED markers              │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: User-Configured Rules                             │
│  - Custom patterns (CLIENT-*, PROJECT-*)                   │
│  - Directory exclusions (/confidential/*, /secret/*)       │
│  - Domain-specific rules (healthcare, finance, defense)    │
│  Output: path with USER_REDACTED markers                   │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Structural Preservation                           │
│  - Replace sensitive values with TYPE placeholders         │
│  - Preserve path structure for clustering                  │
│  - Maintain segment positions for field extraction         │
│  Output: sanitized path ready for LLM/embedding            │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
Sanitized Path
```

### 1.2 Core Sanitization Algorithm

```rust
use std::path::Path;
use regex::Regex;

pub struct PathSanitizer {
    auto_rules: Vec<RedactionRule>,
    user_rules: Vec<RedactionRule>,
    mode: SanitizationMode,
}

#[derive(Clone)]
pub struct RedactionRule {
    pub name: String,
    pub pattern: Regex,
    pub replacement: ReplacementType,
    pub severity: Severity,
    pub enabled: bool,
}

#[derive(Clone)]
pub enum ReplacementType {
    /// Replace with generic placeholder: [USERNAME]
    Placeholder(String),
    /// Replace with type indicator: <string>, <date>, <id>
    TypeIndicator(String),
    /// Hash the value (preserves uniqueness, hides content)
    Hash { prefix: String, length: usize },
    /// Remove entirely (segment disappears)
    Remove,
}

#[derive(Clone, PartialEq)]
pub enum Severity {
    /// PHI, PII - must redact, cannot override
    Critical,
    /// Likely sensitive - redact by default, user can override
    High,
    /// Potentially sensitive - suggest redaction
    Medium,
    /// Informational - preserve unless user requests redaction
    Low,
}

pub enum SanitizationMode {
    /// Maximum privacy - all detection rules active, no overrides
    Strict,
    /// Default - auto rules + user rules, user can override Medium/Low
    Standard,
    /// Minimal - only Critical severity rules enforced
    Permissive,
    /// Complete transparency with user about what will be sent
    Interactive,
}

impl PathSanitizer {
    pub fn sanitize(&self, path: &str) -> SanitizedPath {
        let mut result = SanitizedPath {
            original: path.to_string(),
            sanitized: path.to_string(),
            redactions: vec![],
            blocked: false,
        };

        // Apply automatic rules first
        for rule in &self.auto_rules {
            if !rule.enabled {
                continue;
            }
            result = self.apply_rule(result, rule);
        }

        // Apply user-configured rules
        for rule in &self.user_rules {
            if !rule.enabled {
                continue;
            }
            result = self.apply_rule(result, rule);
        }

        // Check for Critical severity - block if found and not sanitized
        if result.redactions.iter().any(|r| r.severity == Severity::Critical && !r.applied) {
            result.blocked = true;
        }

        result
    }

    fn apply_rule(&self, mut result: SanitizedPath, rule: &RedactionRule) -> SanitizedPath {
        if let Some(captures) = rule.pattern.captures(&result.sanitized) {
            let matched = captures.get(0).unwrap();
            let replacement = match &rule.replacement {
                ReplacementType::Placeholder(p) => format!("[{}]", p),
                ReplacementType::TypeIndicator(t) => format!("<{}>", t),
                ReplacementType::Hash { prefix, length } => {
                    let hash = blake3::hash(matched.as_str().as_bytes());
                    format!("{}_{}", prefix, &hash.to_hex()[..*length])
                }
                ReplacementType::Remove => String::new(),
            };

            result.redactions.push(Redaction {
                rule_name: rule.name.clone(),
                original_value: matched.as_str().to_string(),
                replacement: replacement.clone(),
                severity: rule.severity.clone(),
                applied: true,
            });

            result.sanitized = rule.pattern.replace(&result.sanitized, &replacement).to_string();
        }

        result
    }
}

pub struct SanitizedPath {
    pub original: String,
    pub sanitized: String,
    pub redactions: Vec<Redaction>,
    pub blocked: bool,
}

pub struct Redaction {
    pub rule_name: String,
    pub original_value: String,
    pub replacement: String,
    pub severity: Severity,
    pub applied: bool,
}
```

---

## 2. Default Redaction Rules

### 2.1 Critical Severity (PHI/PII - Always Redact)

| Pattern | Example Match | Replacement | Rationale |
|---------|---------------|-------------|-----------|
| `/home/[^/]+/` | `/home/jsmith/` | `/home/[USER]/` | Unix username |
| `/Users/[^/]+/` | `/Users/JohnSmith/` | `/Users/[USER]/` | macOS username |
| `C:\\Users\\[^\\]+\\` | `C:\Users\JSmith\` | `C:\Users\[USER]\` | Windows username |
| `_mrn_\d+` | `_mrn_12345` | `_mrn_[MRN]` | Medical Record Number |
| `_ssn_\d{3}-?\d{2}-?\d{4}` | `_ssn_123-45-6789` | `_ssn_[SSN]` | Social Security Number |
| `_dob_\d{4}-\d{2}-\d{2}` | `_dob_1990-05-15` | `_dob_[DOB]` | Date of Birth |
| `patient[_-]?[a-z]+[_-]?\d*` | `patient_john_doe_123` | `[PATIENT_ID]` | Patient identifiers |
| `\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b` | `john.smith@company.com` | `[EMAIL]` | Email addresses |

### 2.2 High Severity (Likely Sensitive - Default Redact)

| Pattern | Example Match | Replacement | Rationale |
|---------|---------------|-------------|-----------|
| `CLIENT[-_]?[A-Z0-9]+` | `CLIENT-ACME` | `[CLIENT]` | Client identifiers |
| `CUSTOMER[-_]?[A-Z0-9]+` | `CUSTOMER_12345` | `[CUSTOMER]` | Customer identifiers |
| `SECRET[-_]?[A-Z0-9-]+` | `SECRET-DARPA-X` | `[PROJECT]` | Secret project codes |
| `CLASSIFIED[-_]?[A-Z0-9]+` | `CLASSIFIED_001` | `[CLASSIFIED]` | Classified materials |
| `INTERNAL[-_]?[A-Z0-9]+` | `INTERNAL-PROJ-X` | `[INTERNAL]` | Internal projects |
| `CONFIDENTIAL` | `/confidential/` | `[CONFIDENTIAL]` | Confidential markers |
| `api[-_]?key[-_]?[a-zA-Z0-9]+` | `api_key_abc123` | `[API_KEY]` | API keys in paths |
| `token[-_]?[a-zA-Z0-9]{20,}` | `token_abcdef123456789` | `[TOKEN]` | Auth tokens |

### 2.3 Medium Severity (Potentially Sensitive - Suggest Redact)

| Pattern | Example Match | Replacement | Rationale |
|---------|---------------|-------------|-----------|
| `[A-Z][a-z]+[-_][A-Z][a-z]+` | `John_Smith` | `[NAME]` | Possible person names |
| `\d{3}[-.]?\d{3}[-.]?\d{4}` | `555-123-4567` | `[PHONE]` | Phone numbers |
| `v\d+\.\d+\.\d+[-a-z0-9]*` | `v1.2.3-beta` | Keep | Version strings (not sensitive) |
| `20\d{2}[-/]\d{2}[-/]\d{2}` | `2024-01-15` | Keep | Dates (usually not sensitive) |
| `Q[1-4][-_]?20\d{2}` | `Q1_2024` | Keep | Quarter identifiers |

### 2.4 Built-in Rule Definitions

```toml
# Default rules shipped with Casparian
# Location: embedded in binary, overridable via config

[[redaction.rules]]
name = "unix_username"
pattern = "/home/([^/]+)/"
replacement = { type = "placeholder", value = "USER" }
severity = "critical"
enabled = true

[[redaction.rules]]
name = "macos_username"
pattern = "/Users/([^/]+)/"
replacement = { type = "placeholder", value = "USER" }
severity = "critical"
enabled = true

[[redaction.rules]]
name = "windows_username"
pattern = "C:\\\\Users\\\\([^\\\\]+)\\\\"
replacement = { type = "placeholder", value = "USER" }
severity = "critical"
enabled = true

[[redaction.rules]]
name = "mrn_indicator"
pattern = "[_-]mrn[_-]?\\d+"
replacement = { type = "placeholder", value = "MRN" }
severity = "critical"
enabled = true

[[redaction.rules]]
name = "ssn_pattern"
pattern = "[_-]ssn[_-]?\\d{3}-?\\d{2}-?\\d{4}"
replacement = { type = "placeholder", value = "SSN" }
severity = "critical"
enabled = true

[[redaction.rules]]
name = "client_id"
pattern = "CLIENT[-_]?[A-Z0-9]+"
replacement = { type = "hash", prefix = "client", length = 8 }
severity = "high"
enabled = true

[[redaction.rules]]
name = "secret_project"
pattern = "SECRET[-_]?[A-Z0-9-]+"
replacement = { type = "placeholder", value = "PROJECT" }
severity = "high"
enabled = true
```

---

## 3. User Configuration Options

### 3.1 Configuration File

```toml
# ~/.casparian_flow/config.toml

[privacy]
# Master switch for path sanitization
sanitization_enabled = true

# Sanitization mode: strict | standard | permissive | interactive
mode = "standard"

# Local-only mode: never send paths to cloud LLMs
local_only = false

# Show preview before sending to LLM
preview_before_send = true

# Audit all LLM requests
audit_enabled = true

[privacy.redaction]
# Override default rules
# Set enabled = false to disable a built-in rule
[[privacy.redaction.overrides]]
name = "client_id"
enabled = false  # Disable client ID redaction

# Add custom rules
[[privacy.redaction.custom]]
name = "my_project_codes"
pattern = "PROJ[-_]?[A-Z]{3}[-_]?\\d{3}"
replacement = { type = "placeholder", value = "PROJECT" }
severity = "high"
enabled = true

[[privacy.redaction.custom]]
name = "department_codes"
pattern = "/dept[-_]?([A-Z]{2,4})/"
replacement = { type = "hash", prefix = "dept", length = 6 }
severity = "medium"
enabled = true

[privacy.exclusions]
# Directories to never send to LLM (paths matched won't even be sanitized)
blocked_directories = [
    "/home/*/private/*",
    "/Users/*/private/*",
    "*/confidential/*",
    "*/.secret/*",
]

# File patterns to never process
blocked_patterns = [
    "*.key",
    "*.pem",
    "*.env",
    "*credentials*",
]

[privacy.allowlist]
# Directories explicitly allowed (bypass Medium severity redaction)
allowed_directories = [
    "/data/public/*",
    "*/sample_data/*",
]
```

### 3.2 CLI Configuration Commands

```bash
# View current privacy settings
casparian privacy show

# Set sanitization mode
casparian privacy mode strict
casparian privacy mode standard
casparian privacy mode permissive

# Enable/disable local-only mode
casparian privacy local-only true
casparian privacy local-only false

# Add a custom redaction rule
casparian privacy rule add \
    --name "vendor_codes" \
    --pattern "VENDOR[-_]?[A-Z0-9]+" \
    --replacement placeholder:VENDOR \
    --severity high

# Disable a built-in rule
casparian privacy rule disable client_id

# Enable a disabled rule
casparian privacy rule enable client_id

# List all rules (built-in and custom)
casparian privacy rules

# Test sanitization on a path
casparian privacy test "/home/jsmith/CLIENT-ACME/data.csv"
# Output:
#   Original:  /home/jsmith/CLIENT-ACME/data.csv
#   Sanitized: /home/[USER]/[CLIENT]/data.csv
#   Redactions:
#     - unix_username (critical): jsmith -> [USER]
#     - client_id (high): CLIENT-ACME -> [CLIENT]

# Preview what would be sent for a directory
casparian privacy preview /data/clients/
```

### 3.3 Interactive Mode TUI

When `mode = "interactive"` or user invokes a wizard with `--review-privacy`:

```
┌─ PATH PRIVACY REVIEW ───────────────────────────────────────────────────────┐
│                                                                              │
│  The Path Intelligence Engine will analyze these paths:                      │
│                                                                              │
│  ┌─ Paths to Send (47 files) ──────────────────────────────────────────────┐ │
│  │                                                                          │ │
│  │  Original                              Sanitized                         │ │
│  │  ─────────────────────────────────────────────────────────────────────── │ │
│  │  /home/jsmith/data/CLIENT-ACME/...  →  /home/[USER]/data/[CLIENT]/...   │ │
│  │  /home/jsmith/data/CLIENT-BETA/...  →  /home/[USER]/data/[CLIENT]/...   │ │
│  │  /data/patients/john_doe_mrn_123/   →  /data/patients/[PATIENT_ID]/     │ │
│  │  /projects/SECRET-DARPA-X/files/    →  /projects/[PROJECT]/files/       │ │
│  │                                                                          │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Redactions Applied:                                                         │
│    • 47 username redactions (critical)                                       │
│    • 12 client ID redactions (high)                                          │
│    • 3 patient ID redactions (critical)                                      │
│    • 1 project code redaction (high)                                         │
│                                                                              │
│  ┌─ Actions ───────────────────────────────────────────────────────────────┐ │
│  │  [Enter] Proceed with sanitized paths                                    │ │
│  │  [e] Edit redaction rules                                                │ │
│  │  [v] View full path list                                                 │ │
│  │  [x] Exclude specific paths                                              │ │
│  │  [Esc] Cancel operation                                                  │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Local vs Cloud Behavior

### 4.1 Execution Mode Detection

```rust
pub enum ExecutionMode {
    /// Ollama running locally - reduced privacy concern
    LocalOllama { host: String },
    /// llama.cpp with local model - no network
    LocalLlamaCpp { model_path: PathBuf },
    /// Cloud API (OpenAI, Anthropic, etc.) - full sanitization
    CloudApi { provider: String, endpoint: String },
    /// Air-gapped system - no external network
    AirGapped,
}

impl ExecutionMode {
    pub fn privacy_requirements(&self) -> PrivacyRequirements {
        match self {
            ExecutionMode::LocalOllama { .. } => PrivacyRequirements {
                sanitization_required: true,
                critical_rules_enforced: true,
                high_rules_default: true,
                medium_rules_default: false,
                audit_required: false,
                preview_required: false,
            },
            ExecutionMode::LocalLlamaCpp { .. } => PrivacyRequirements {
                sanitization_required: false, // User can disable entirely
                critical_rules_enforced: true,
                high_rules_default: false,
                medium_rules_default: false,
                audit_required: false,
                preview_required: false,
            },
            ExecutionMode::CloudApi { .. } => PrivacyRequirements {
                sanitization_required: true,
                critical_rules_enforced: true,
                high_rules_default: true,
                medium_rules_default: true,
                audit_required: true,
                preview_required: true,
            },
            ExecutionMode::AirGapped => PrivacyRequirements {
                sanitization_required: false,
                critical_rules_enforced: false,
                high_rules_default: false,
                medium_rules_default: false,
                audit_required: false,
                preview_required: false,
            },
        }
    }
}

pub struct PrivacyRequirements {
    pub sanitization_required: bool,
    pub critical_rules_enforced: bool,
    pub high_rules_default: bool,
    pub medium_rules_default: bool,
    pub audit_required: bool,
    pub preview_required: bool,
}
```

### 4.2 Mode-Specific Behavior

| Aspect | Local (Ollama/llama.cpp) | Cloud API | Air-Gapped |
|--------|--------------------------|-----------|------------|
| Critical rules | Enforced | Enforced | Optional |
| High rules | Default on | Enforced | Optional |
| Medium rules | Optional | Default on | Optional |
| Preview dialog | Optional | Required | N/A |
| Audit logging | Optional | Required | N/A |
| User override | Medium/Low | Low only | All |
| Network check | N/A | Verify destination | Block all |

### 4.3 Cloud Provider Detection

```rust
fn detect_cloud_provider(config: &Config) -> Option<String> {
    match &config.ai.provider {
        "ollama" => {
            // Check if Ollama is local or remote
            let host = &config.ai.ollama.host;
            if is_local_host(host) {
                None // Local
            } else {
                Some("Remote Ollama".to_string())
            }
        }
        "openai" => Some("OpenAI".to_string()),
        "anthropic" => Some("Anthropic".to_string()),
        "azure" => Some("Azure OpenAI".to_string()),
        "llamacpp" => None, // Always local
        _ => None,
    }
}

fn is_local_host(host: &str) -> bool {
    let local_patterns = [
        "localhost",
        "127.0.0.1",
        "::1",
        "0.0.0.0",
    ];
    local_patterns.iter().any(|p| host.contains(p))
}
```

### 4.4 Local-Only Mode

When `local_only = true`:

```rust
impl PathIntelligenceEngine {
    pub fn cluster_paths(&self, paths: &[&str]) -> Result<Vec<Cluster>, Error> {
        if self.config.privacy.local_only {
            // Verify execution mode is local
            match self.detect_execution_mode() {
                ExecutionMode::LocalOllama { .. } |
                ExecutionMode::LocalLlamaCpp { .. } |
                ExecutionMode::AirGapped => {
                    // Proceed
                }
                ExecutionMode::CloudApi { provider, .. } => {
                    return Err(Error::LocalOnlyViolation {
                        message: format!(
                            "local_only mode enabled but {} is configured. \
                             Set privacy.local_only = false or use local model.",
                            provider
                        ),
                    });
                }
            }
        }

        // Continue with clustering...
    }
}
```

---

## 5. Examples of Sanitized Paths

### 5.1 Username Redaction

```
Original:  /home/jsmith/documents/reports/sales_2024.csv
Sanitized: /home/[USER]/documents/reports/sales_2024.csv

Original:  /Users/John Smith/Downloads/CLIENT-ACME/data.xlsx
Sanitized: /Users/[USER]/Downloads/[CLIENT]/data.xlsx

Original:  C:\Users\jsmith.CORP\Projects\secret_project\file.csv
Sanitized: C:\Users\[USER]\Projects\[PROJECT]\file.csv
```

### 5.2 PHI/PII Redaction

```
Original:  /data/patients/john_doe_mrn_12345/records/visit_2024.json
Sanitized: /data/patients/[PATIENT_ID]/records/visit_2024.json

Original:  /healthcare/patient_ssn_123-45-6789/claims/
Sanitized: /healthcare/[PATIENT_ID]/claims/

Original:  /records/dob_1990-05-15/medical_history.pdf
Sanitized: /records/[DOB]/medical_history.pdf
```

### 5.3 Client/Project Redaction

```
Original:  /data/CLIENT-ACME-CORP/financials/2024/Q1/
Sanitized: /data/[CLIENT]/financials/2024/Q1/

Original:  /projects/SECRET-DARPA-X/phase2/deliverables/
Sanitized: /projects/[PROJECT]/phase2/deliverables/

Original:  /work/INTERNAL-MERGER-2024/due_diligence/
Sanitized: /work/[INTERNAL]/due_diligence/
```

### 5.4 Hash-Based Redaction (Preserves Uniqueness)

```
Original:  /data/CLIENT-ACME/invoices/2024/
           /data/CLIENT-BETA/invoices/2024/
           /data/CLIENT-GAMMA/invoices/2024/

Sanitized: /data/client_a7b3c2d1/invoices/2024/
           /data/client_e4f5g6h7/invoices/2024/
           /data/client_i8j9k0l1/invoices/2024/

# Hash preserves uniqueness - clustering can still group by structure
# while hiding actual client names
```

### 5.5 Embedding-Safe Normalization

For the embedding model (no LLM, just semantic similarity):

```
Original:  /home/jsmith/data/CLIENT-ACME/invoices/2024/Q1/inv_001.pdf

Step 1 (Standard sanitization):
           /home/[USER]/data/[CLIENT]/invoices/2024/Q1/inv_001.pdf

Step 2 (Embedding normalization):
           "home USER data CLIENT invoices 2024 Q1 inv pdf"

# The embedding sees semantic structure, not sensitive values
```

---

## 6. Audit Trail

### 6.1 Extended Audit Schema

```sql
-- Extend cf_ai_audit_log for path sanitization tracking
ALTER TABLE cf_ai_audit_log ADD COLUMN paths_sent INTEGER;
ALTER TABLE cf_ai_audit_log ADD COLUMN paths_blocked INTEGER;
ALTER TABLE cf_ai_audit_log ADD COLUMN redaction_summary TEXT;  -- JSON

-- Detailed redaction log (optional, for compliance)
CREATE TABLE cf_path_redaction_log (
    id TEXT PRIMARY KEY,
    audit_id TEXT NOT NULL REFERENCES cf_ai_audit_log(id),
    original_path_hash TEXT NOT NULL,  -- blake3 hash of original
    sanitized_path TEXT NOT NULL,      -- What was actually sent
    redactions_applied TEXT NOT NULL,  -- JSON array of rule names
    created_at TEXT NOT NULL
);

CREATE INDEX idx_redaction_audit ON cf_path_redaction_log(audit_id);
```

### 6.2 Audit CLI Extensions

```bash
# View redaction statistics
casparian ai audit --redactions --last 50

# Output:
#   Last 50 AI requests:
#     Total paths processed: 2,847
#     Paths with redactions: 1,923 (67.5%)
#     Paths blocked: 12 (0.4%)
#
#   Redaction breakdown:
#     username (critical):    1,847 (64.9%)
#     client_id (high):         456 (16.0%)
#     patient_id (critical):     89 (3.1%)
#     project_code (high):       34 (1.2%)

# Export redaction audit for compliance
casparian ai audit --redactions --since 2026-01-01 --format json > redaction_audit.json
```

---

## 7. Implementation Checklist

### Phase 1: Core Sanitization (2-3 days)
- [ ] Implement `PathSanitizer` struct with rule engine
- [ ] Add built-in Critical and High severity rules
- [ ] Integrate with `normalize_for_embedding()` in Section 3.5.2
- [ ] Add unit tests for all default patterns

### Phase 2: Configuration (1-2 days)
- [ ] Add `[privacy]` section to config.toml schema
- [ ] Implement rule override/custom rule loading
- [ ] Add CLI commands for privacy management
- [ ] Implement blocked_directories and blocked_patterns

### Phase 3: Mode-Aware Behavior (1-2 days)
- [ ] Implement `ExecutionMode` detection
- [ ] Add local vs cloud behavior differences
- [ ] Implement `local_only` mode enforcement
- [ ] Add preview dialog for cloud mode

### Phase 4: Audit & Compliance (1 day)
- [ ] Extend `cf_ai_audit_log` schema
- [ ] Add `cf_path_redaction_log` table
- [ ] Implement audit CLI extensions
- [ ] Add export functionality

---

## 8. Spec Updates Required

Add to `specs/ai_wizards.md` Section 3.5.9:

```markdown
#### 3.5.9 Privacy Considerations

**Path Sanitization Pipeline:**

All paths are sanitized before being sent to embedding models or LLMs:

1. **Automatic Detection:** Built-in patterns detect usernames, PHI indicators,
   and common sensitive patterns (see `~/.casparian_flow/privacy_rules.toml`)

2. **User Rules:** Custom patterns can be added via config or CLI

3. **Mode-Aware:** Local execution has relaxed defaults; cloud APIs enforce
   all Critical and High severity rules

**Sanitization Example:**

| Original | Sanitized | Rules Applied |
|----------|-----------|---------------|
| `/home/jsmith/CLIENT-ACME/data.csv` | `/home/[USER]/[CLIENT]/data.csv` | unix_username, client_id |
| `/patients/john_doe_mrn_123/` | `/patients/[PATIENT_ID]/` | mrn_indicator |

**Configuration:**

```toml
[privacy]
mode = "standard"        # strict | standard | permissive | interactive
local_only = false       # Never send to cloud LLMs
preview_before_send = true
```

**CLI:**

```bash
# Test sanitization
casparian privacy test "/home/jsmith/data.csv"

# Preview what will be sent
casparian privacy preview /data/clients/

# View/modify rules
casparian privacy rules
casparian privacy rule add --name "my_pattern" --pattern "..."
```

See `specs/meta/sessions/ai_wizards/round_011/engineer.md` for full specification.
```

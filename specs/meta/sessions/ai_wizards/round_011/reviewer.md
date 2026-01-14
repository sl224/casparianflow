# Reviewer Assessment: GAP-PRIVACY-001 Resolution

**Engineer Proposal:** `round_011/engineer.md`
**Gap:** GAP-PRIVACY-001 - Path Intelligence Engine sends paths to LLMs without sanitization

---

## Overall Assessment: CONDITIONAL APPROVAL

The Engineer's proposal is comprehensive and well-structured. The three-layer sanitization model, severity-based rules, and mode-aware behavior are sound architectural choices. However, there are several issues that need addressing before this can be fully approved.

---

## Issues Identified

### ISSUE-R11-001: Incomplete Username Patterns (Medium)

**Location:** Section 2.1, Critical Severity patterns

**Problem:** The username redaction patterns are too narrow. They miss several common scenarios:

1. **WSL (Windows Subsystem for Linux):** `/mnt/c/Users/jsmith/` - the `/mnt/c/` prefix is not handled
2. **Network paths:** `//server/users/jsmith/`, `\\\\server\\users\\jsmith\\`
3. **Environment variable paths:** `$HOME/`, `%USERPROFILE%/` (sometimes appear in logs/configs)
4. **Linux home directories with subdirectories:** `/home/jsmith.backup/` would only partially match
5. **Corporate username formats:** `jsmith.CORP@domain` embedded in paths
6. **Sudo-preserved paths:** `/root/` on Linux systems

**Recommendation:**
```rust
// Add these patterns to Critical severity:
"/mnt/c/Users/([^/]+)/"      // WSL paths
"//[^/]+/users?/([^/]+)/"    // UNC network paths (case-insensitive)
"/root/"                      // Linux root user
"\\$HOME/"                    // Unexpanded env var
"%USERPROFILE%"               // Windows env var
```

---

### ISSUE-R11-002: Regex Order and Greedy Matching (High)

**Location:** Section 1.2, `apply_rule` method

**Problem:** Rules are applied sequentially to an already-modified string. This can cause:

1. **Cascading replacements:** If a rule replaces `/home/jsmith/` with `/home/[USER]/`, a subsequent rule matching `[USER]` would incorrectly trigger
2. **Greedy matching issues:** The patient pattern `patient[_-]?[a-z]+[_-]?\d*` could match unintended strings like `patientcare` or `patiently`
3. **Rule shadowing:** A less specific rule applied first could prevent a more specific rule from matching

**Example of cascading issue:**
```
Original: /home/jsmith/data/[USER_MANUAL].pdf
After rule 1: /home/[USER]/data/[USER_MANUAL].pdf
Rule matching "[USER]" would incorrectly flag "[USER_MANUAL]"
```

**Recommendation:**
1. Apply rules to the original string, collecting all match ranges
2. Sort matches by position and specificity
3. Apply non-overlapping replacements in a single pass
4. Use word boundaries (`\b`) in patterns where appropriate

```rust
// Improved patient pattern
r"\bpatient[_-]?[a-z]+[_-]?\d+\b"  // Require trailing digit, word boundary
```

---

### ISSUE-R11-003: Hash Collision Privacy Leak (Medium)

**Location:** Section 2.4, hash-based replacement; Section 5.4

**Problem:** The hash-based redaction uses truncated hashes (8 characters by default). This creates two issues:

1. **Birthday paradox:** With ~65,000 unique client IDs, there's a 50% chance of collision with 8-character hashes (32 bits)
2. **Correlation attack:** If attacker knows you have clients "ACME" and "BETA", they can hash these and look for matches in audit logs

**Example attack:**
```python
# Attacker knows your client list
known_clients = ["ACME", "BETA", "GAMMA"]
for client in known_clients:
    h = blake3(f"CLIENT-{client}".encode()).hex()[:8]
    print(f"CLIENT-{client} -> client_{h}")
    # Now search sanitized logs for these hashes
```

**Recommendation:**
1. Add a per-installation random salt (generated once, stored in config)
2. Increase default hash length to 16 characters
3. Document that hash-based redaction is weaker than placeholder redaction

```rust
ReplacementType::Hash { prefix, length, salted: bool }

// In apply:
let hash_input = if salted {
    format!("{}{}", self.salt, matched.as_str())
} else {
    matched.as_str().to_string()
};
```

---

### ISSUE-R11-004: Air-Gapped Mode Critical Rules Bypass (High)

**Location:** Section 4.1, `ExecutionMode::AirGapped`

**Problem:** Air-gapped mode disables ALL rules including Critical severity. This is dangerous because:

1. **Logs may still leak:** Even air-gapped systems often have audit trails, exports, or USB-based data transfer
2. **Defense-in-depth violated:** PHI/PII should still be protected regardless of network status
3. **User expectation mismatch:** Users may not realize "air-gapped" means "no protection"

**Recommendation:**
Keep Critical rules enforced even in air-gapped mode. Change to:

```rust
ExecutionMode::AirGapped => PrivacyRequirements {
    sanitization_required: false,  // User can disable entire system
    critical_rules_enforced: true, // But if enabled, Critical always applies
    high_rules_default: false,
    medium_rules_default: false,
    audit_required: false,
    preview_required: false,
}
```

Or add a new mode `ExecutionMode::Unrestricted` that requires explicit opt-in with a warning.

---

### ISSUE-R11-005: Missing Unicode and Encoding Edge Cases (Medium)

**Location:** Section 2.1-2.3, pattern definitions

**Problem:** Patterns assume ASCII paths. Unicode edge cases:

1. **Internationalized usernames:** `/Users/` (Japanese for "user") common on localized systems
2. **Lookalike attacks:** `/Users/jsmitℎ/` (using mathematical h U+210E)
3. **Encoding variations:** Same path could be UTF-8 vs UTF-16 in logs
4. **NFD vs NFC normalization:** `resume.pdf` (composed) vs `resum` + combining e (decomposed)

**Recommendation:**
1. Normalize paths to NFC before pattern matching
2. Add note about unicode-aware patterns
3. Consider ASCII-only mode for systems with guaranteed ASCII paths

```rust
fn sanitize(&self, path: &str) -> SanitizedPath {
    // Normalize to NFC form
    let normalized = path.nfc().collect::<String>();
    // ... rest of sanitization
}
```

---

### ISSUE-R11-006: Structural Preservation vs Privacy Trade-off (Low)

**Location:** Section 1.1, Layer 3

**Problem:** The proposal preserves structure "for clustering" but doesn't quantify the privacy-utility trade-off:

1. **Directory depth preserved:** Reveals organizational hierarchy
2. **Segment positions preserved:** Reveals naming convention patterns
3. **File extensions preserved:** Reveals data types

**Example:**
```
Original:  /home/jsmith/SECRET-PROJECT-X/confidential/2024/Q1/financials.xlsx
Sanitized: /home/[USER]/[PROJECT]/[CONFIDENTIAL]/2024/Q1/financials.xlsx

# Attacker learns: 5-level hierarchy, quarterly structure, Excel financials exist
```

**Recommendation:**
Add an optional `structure_obfuscation` mode for high-security environments:

```rust
pub enum StructurePreservation {
    Full,      // Default: preserve all structure
    Partial,   // Preserve depth, hash segment positions
    Minimal,   // Flatten to "type type type type file.ext"
}
```

---

### ISSUE-R11-007: Blocked Directory Glob Expansion (Low)

**Location:** Section 3.1, `blocked_directories`

**Problem:** Using glob patterns like `/home/*/private/*` for blocking could have performance issues with large filesystems and doesn't handle symlinks.

**Edge cases:**
1. **Symlink escape:** `/home/jsmith/public/link_to_private -> /home/jsmith/private/`
2. **Case sensitivity:** `/home/jsmith/Private/` vs `/home/jsmith/private/`
3. **Trailing slash:** `/home/jsmith/private` (file) vs `/home/jsmith/private/` (dir)

**Recommendation:**
1. Canonicalize paths (resolve symlinks) before blocking check
2. Document case sensitivity behavior per platform
3. Treat both forms (with/without trailing slash) as blocked

---

### ISSUE-R11-008: Audit Log Original Path Hash Reversibility (Medium)

**Location:** Section 6.1, `cf_path_redaction_log`

**Problem:** Storing `original_path_hash` in the audit log enables rainbow table attacks if the hash algorithm is known:

```sql
-- Attacker query
SELECT * FROM cf_path_redaction_log
WHERE original_path_hash = blake3('/home/jsmith/secret/data.csv');
```

**Recommendation:**
1. Use HMAC with installation-specific key instead of plain hash
2. Or don't store the hash at all (sanitized path + rule names sufficient for audit)
3. Document that audit logs are sensitive and should be access-controlled

---

### ISSUE-R11-009: Missing Email Pattern Precision (Low)

**Location:** Section 2.1, email pattern

**Problem:** The email regex `\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b` has issues:

1. **Overly broad:** Matches `version@2.0.1` (version strings)
2. **Missing TLDs:** Won't match `.museum`, `.technology` (though `{2,}` helps)
3. **Path context:** Emails in paths are rare; pattern may cause false positives

**Example false positive:**
```
/data/project@v2.0.csv -> /data/[EMAIL].csv  (wrong!)
```

**Recommendation:**
Tighten the pattern to require more email-like structure:
```rust
r"\b[a-zA-Z][a-zA-Z0-9._%+-]*@[a-zA-Z0-9][-a-zA-Z0-9]*\.[a-zA-Z]{2,}\b"
```

---

### ISSUE-R11-010: Interactive Mode Network Race (Low)

**Location:** Section 3.3, Interactive Mode TUI

**Problem:** If user is in interactive mode reviewing paths, and the network changes (e.g., VPN connects), the execution mode detection might give stale results.

**Recommendation:**
Re-check execution mode immediately before actual send, not just at preview time. Add confirmation:

```
[!] Network status changed since preview.
    Was: Local Ollama
    Now: Cloud API (Azure OpenAI)

[Enter] Re-review with Cloud API rules   [Esc] Cancel
```

---

## Positive Observations

1. **Severity levels are well-thought-out** - Critical/High/Medium/Low provides good granularity
2. **CLI ergonomics are excellent** - `casparian privacy test` and `casparian privacy preview` are very user-friendly
3. **The TUI mockup is clear** - Interactive mode gives users visibility and control
4. **Mode-aware behavior is correct** - Different trust levels for local vs cloud is the right model
5. **TOML configuration is clean** - Override syntax is intuitive

---

## Required Changes for Approval

| Issue | Severity | Required? |
|-------|----------|-----------|
| ISSUE-R11-001 | Medium | Yes - missing username patterns are a real gap |
| ISSUE-R11-002 | High | Yes - cascading replacement is a correctness bug |
| ISSUE-R11-003 | Medium | Yes - hash collision is a real attack vector |
| ISSUE-R11-004 | High | Yes - disabling Critical in air-gapped violates expectations |
| ISSUE-R11-005 | Medium | Document only (note limitation) |
| ISSUE-R11-006 | Low | Optional enhancement, not blocking |
| ISSUE-R11-007 | Low | Document symlink behavior |
| ISSUE-R11-008 | Medium | Yes - use HMAC or remove original hash |
| ISSUE-R11-009 | Low | Yes - easy fix, prevents false positives |
| ISSUE-R11-010 | Low | Document behavior |

---

## Summary

The proposal demonstrates strong understanding of the privacy problem space and provides a practical, layered solution. The severity-based system and mode-aware behavior are architecturally sound.

**Blocking issues to fix:**
- ISSUE-R11-002 (regex application order)
- ISSUE-R11-004 (air-gapped Critical bypass)

**Important issues to address:**
- ISSUE-R11-001 (username patterns)
- ISSUE-R11-003 (hash salting)
- ISSUE-R11-008 (audit hash)
- ISSUE-R11-009 (email pattern)

Once these are addressed, this proposal will be ready for implementation.

---

**Reviewer:** Spec Refinement Workflow
**Date:** 2026-01-13
**Status:** CONDITIONAL APPROVAL (pending fixes)

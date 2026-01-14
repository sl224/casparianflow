# Engineer Resolution: GAP-AUDIT-001

## Audit Log Retention Policy Specification

**Gap:** No retention policy defined for AI wizard audit logs. Spec includes audit log table (Section 7.3) and CLI commands (Section 7.4) but lacks critical operational details around log retention, disk usage management, and privacy considerations.

**Resolution:** Define comprehensive audit log retention strategy addressing time-based retention, size-based limits, privacy implications, and compliance requirements.

---

## 1. Audit Log Scope and Operations

### 1.1 Auditable Operations

Every AI wizard operation generates exactly one audit log entry at completion (success or failure). Audit events are immutable (write-once):

| Operation | Wizard | Trigger | Log Entry |
|-----------|--------|---------|-----------|
| Path clustering + field inference | Path Intelligence Engine | User initiates (manual or auto-called) | LLM call to inference model |
| Extractor generation | Pathfinder Wizard | User opens wizard, provides hints, hits "Generate" | Single LLM call with path + hints |
| Parser template generation | Parser Lab Wizard | User selects file, hits "Generate" | Single LLM call with sample rows |
| Label generation | Labeling Wizard | User opens wizard for signature group | Single LLM call with sample structure |
| Semantic path analysis | Semantic Path Wizard | User selects source/files, hits "Generate" | LLM call if confidence <80% (pre-detection) |
| User redaction decision | All wizards | User enters Redaction dialog | No LLM call, but redaction metadata logged |
| Manual edit via $EDITOR | All wizards | User selects 'e' to edit in $EDITOR | No LLM call, source file path and editor exit code logged |

### 1.2 What Gets Logged

Each operation logs:

```sql
CREATE TABLE cf_ai_audit_log (
    -- Identity
    id TEXT PRIMARY KEY,                    -- uuid v4, generated at time of log creation
    wizard_type TEXT NOT NULL,              -- 'pathfinder', 'parser_lab', 'labeling', 'semantic_path', 'pie'

    -- Input Context
    model_name TEXT NOT NULL,               -- 'qwen2.5-coder:7b', 'ollama:7b', 'gpt-4', etc
    input_type TEXT NOT NULL,               -- 'path', 'headers', 'structure', 'mixed'
    input_hash TEXT NOT NULL,               -- blake3(normalized input sent to LLM)
    input_preview TEXT,                     -- First 500 chars, for debugging (no PII)
    input_byte_count INTEGER,               -- Total bytes of input

    -- Redaction State
    redactions TEXT,                        -- JSON: ["patient_ssn", "diagnosis"]
    redaction_count INTEGER,                -- Number of values redacted before LLM call

    -- Output/Result
    output_type TEXT,                       -- 'extractor', 'parser', 'label', 'semantic_rule', 'cluster_analysis'
    output_hash TEXT,                       -- blake3(LLM response)
    output_byte_count INTEGER,              -- Total bytes of LLM response
    output_file TEXT,                       -- Draft file path if code generated

    -- Execution
    duration_ms INTEGER,                    -- Wall clock time from send to response
    status TEXT NOT NULL,                   -- 'success', 'timeout', 'error', 'user_cancel'
    error_message TEXT,                     -- If status != 'success'
    error_code TEXT,                        -- Structured error (e.g., 'INVALID_YAML', 'TIMEOUT')

    -- User Intent
    user_hint TEXT,                         -- Optional hint provided by user (unparsed)
    user_redaction_fields TEXT,             -- JSON: fields user manually redacted
    user_action_on_draft TEXT,              -- 'approved', 'edited', 'discarded', NULL

    -- Lineage
    created_at TEXT NOT NULL,               -- ISO8601 UTC, immutable
    completed_at TEXT,                      -- ISO8601 UTC when operation finished
    job_id TEXT,                            -- Associated job ID if part of batch

    -- Compliance
    ai_model_endpoint TEXT,                 -- Provider + model (e.g., 'ollama:localhost:11434/qwen2.5')
    privacy_mode TEXT                       -- 'strict', 'standard', 'permissive', 'interactive'
);

CREATE INDEX idx_ai_audit_wizard ON cf_ai_audit_log(wizard_type);
CREATE INDEX idx_ai_audit_created ON cf_ai_audit_log(created_at);
CREATE INDEX idx_ai_audit_status ON cf_ai_audit_log(status);
CREATE INDEX idx_ai_audit_model ON cf_ai_audit_log(model_name);
```

---

## 2. Retention Policy

### 2.1 Time-Based Retention

**Default Policy (production):**

| Log Type | Retention | Rationale |
|----------|-----------|-----------|
| Successful operations | 90 days | Sufficient for audit, reduces storage |
| Failed operations (errors) | 180 days | Support ticket investigation, error pattern analysis |
| Critical errors (timeouts, LLM crashes) | 1 year | Root cause analysis, capacity planning |
| User cancellations | 30 days | Optional, minimal compliance value |

**Justification:**
- 90 days covers typical support SLA (users report issues within 2 weeks, re-investigation within 30 days)
- Error logs kept longer for debugging multi-week issues
- Critical error retention aids incident response and pattern detection
- Balances compliance needs with storage costs

**Implementation:**

```sql
-- Automated retention cleanup (runs daily at 02:00 UTC)
DELETE FROM cf_ai_audit_log
WHERE created_at < datetime('now', '-90 days')
  AND status = 'success'
  AND wizard_type != 'pathfinder';  -- Keep Pathfinder success longer (next clause)

DELETE FROM cf_ai_audit_log
WHERE created_at < datetime('now', '-180 days')
  AND status IN ('error', 'timeout');

DELETE FROM cf_ai_audit_log
WHERE created_at < datetime('now', '-365 days')
  AND status = 'error'
  AND error_code IN ('LLM_CRASH', 'NETWORK_ERROR', 'MEMORY_EXCEEDED');

-- Always keep last 1000 records regardless of age (safety margin)
DELETE FROM cf_ai_audit_log
WHERE id NOT IN (
    SELECT id FROM cf_ai_audit_log
    ORDER BY created_at DESC LIMIT 1000
)
  AND created_at < datetime('now', '-90 days');
```

### 2.2 Size-Based Limits

**Disk Usage Caps:**

```
Total audit log table max size: 500 MB
├─ Soft limit: 400 MB (trigger warning log)
├─ Hard limit: 500 MB (start deletion from oldest)
└─ Recovery: Retain latest 180 days if exceeding hard limit
```

**Calculation:**

Assuming average row size:
- Typical audit log row: ~800 bytes (all text fields, JSON)
- Compression factor (SQLite): ~0.6x with ZSTD
- Effective: 480 bytes/row compressed

```
500 MB limit ÷ 480 bytes/row ≈ 1,048,576 rows
at ~50 rows/day = ~20,971 days retention (57 years)
```

In practice, with proper time-based deletion, size limit is rarely hit.

**Monitoring:**

```sql
-- Check current size
SELECT
    page_count * page_size / (1024*1024) as size_mb
FROM pragma_page_count(), pragma_page_size()
WHERE name = 'cf_ai_audit_log';

-- Alert if >400 MB
SELECT COUNT(*) as row_count,
       ROUND(COUNT(*) * 800 / (1024*1024), 2) as estimated_mb
FROM cf_ai_audit_log;
```

### 2.3 Retention Override Modes

Users can configure retention via config:

```toml
[audit]
# Time-based retention (days)
retention_success = 90          # Default successful operations
retention_error = 180           # Failed operations
retention_critical = 365        # Critical errors only
retention_cancel = 30           # User cancellations

# Size-based limits (MB)
max_log_size_mb = 500
cleanup_at_soft_limit_mb = 400

# Cleanup strategy when size exceeded
cleanup_strategy = "oldest_first"   # 'oldest_first' | 'delete_cancels_first' | 'delete_success_first'

# Compliance modes (override time-based)
# - "compliant": Keep indefinitely (for regulated industries)
# - "permissive": Delete aggressively after 30 days
# - "standard": Default (90/180/365)
compliance_mode = "standard"
```

**Compliance Mode Overrides:**

| Mode | Success | Error | Critical | Use Case |
|------|---------|-------|----------|----------|
| `standard` (default) | 90d | 180d | 365d | General use |
| `compliant` | ∞ | ∞ | ∞ | Healthcare, finance (HIPAA, PCI-DSS) |
| `permissive` | 30d | 60d | 90d | Personal/dev use |
| `none` | 7d | 14d | 30d | Privacy-sensitive (minimal logging) |

---

## 3. Privacy Considerations

### 3.1 What's NOT Logged

To minimize privacy risk, these are explicitly excluded:

| Excluded | Reason | Compliance |
|----------|--------|-----------|
| Full file content | Too large, PII risk | GDPR, CCPA |
| Full LLM response body | May contain extracted PII | HIPAA, GDPR |
| Unredacted paths | User privacy | GDPR Article 32 |
| User email/username | Unnecessary for audit | GDPR minimization |
| Actual LLM API keys | Security risk | Never in logs |

### 3.2 What IS Logged + Privacy Protection

```
Logged Value              Stored As                    Why This Is Safe
─────────────────────────────────────────────────────────────────────────
Input paths               blake3(sanitized_path)       Hash prevents reversal
LLM response              blake3(response_text)        Prevents reconstruction
Input size (bytes)        Integer byte count           No content, just metadata
Field names from sample   Hashed, indexed only         Semantic only, no values
User hints                Plain text (max 200 chars)   User-provided, already safe
Redaction field names     JSON array of field names    Schema only, no values
```

### 3.3 Handling Sensitive Audit Data

**Case: Medical/Financial Data**

User processes HIPAA file with Labeling Wizard:

```json
{
    "id": "audit_uuid",
    "wizard_type": "labeling",
    "input_hash": "a7b3c2d1...",           // Hash of sanitized path
    "input_preview": "[PATIENT]/records",   // Sanitized, not original
    "redactions": ["mrn", "ssn", "dob"],    // Field names only
    "output_hash": "f1e2d3c4...",           // Hash of LLM response
    "status": "success",
    "created_at": "2026-01-13T14:23:45Z"    // Time, but not username
}
```

No PII appears in the audit log. The hashes prevent:
1. Reverse lookup (preimage attack impractical)
2. Correlation across audits (different input each time)
3. Replay attacks (hash changes if input sanitization changes)

### 3.4 User Data Deletion (GDPR Right to Erasure)

When user deletes an audit entry (via CLI below), full deletion is permanent:

```bash
# Delete specific audit record
casparian ai audit --delete abc123def456

# Delete all records matching criteria
casparian ai audit --delete-where "wizard_type='pathfinder' AND status='error'"

# Delete before date
casparian ai audit --delete-before 2025-12-31

# Confirm deletion is permanent
# (system asks for confirmation with 5-char code from record)
```

---

## 4. Log Access and Querying

### 4.1 CLI Commands for Audit Access

```bash
# View recent successful operations (last 10)
casparian ai audit --last 10

# View specific wizard type
casparian ai audit --wizard pathfinder --last 5

# View errors only
casparian ai audit --status error --last 20

# Date-based query
casparian ai audit --since 2026-01-01
casparian ai audit --between 2026-01-01 2026-01-31

# Filter by model
casparian ai audit --model "qwen2.5-coder:7b" --last 10

# Export for compliance
casparian ai audit --since 2026-01-01 --format json > ai_audit_jan.json
casparian ai audit --since 2026-01-01 --format csv > ai_audit_jan.csv

# Statistics/summary
casparian ai audit --stats
# Output:
#   Total operations (all time): 2,847
#   Last 30 days: 547 (19.2 per day)
#   Success rate: 98.2%
#   Average duration: 1,247 ms
#
#   By wizard:
#     pathfinder:    1,045 (36.7%)
#     parser_lab:      892 (31.3%)
#     labeling:        712 (25.0%)
#     semantic_path:   198 (7.0%)
#
#   By status:
#     success:       2,794 (98.1%)
#     error:           45 (1.6%)
#     timeout:          8 (0.3%)

# Clear old logs (respects retention policy)
casparian ai audit --cleanup

# Clear with explicit date
casparian ai audit --delete-before 2025-12-31
```

### 4.2 Viewing Full Audit Entry

```bash
casparian ai audit show abc123def456

# Output:
# ┌─ Audit Log Entry ────────────────────────────────────────────────────┐
# │                                                                       │
# │  ID:                  abc123def456                                   │
# │  Wizard:              pathfinder                                     │
# │  Status:              success                                        │
# │  Model:               qwen2.5-coder:7b                               │
# │  Duration:            1,234 ms                                       │
# │                                                                       │
# │  Input:                                                              │
# │    Type:              path                                           │
# │    Bytes:             342                                            │
# │    Preview:           /home/[USER]/projects/[PROJECT]/data/...      │
# │    Hash:              a7b3c2d1...                                    │
# │                                                                       │
# │  Redactions:          ["client_id", "username"]                      │
# │  User Hints:          "Extract date and transaction ID"              │
# │                                                                       │
# │  Output:                                                             │
# │    Type:              extractor                                      │
# │    Bytes:             2,048                                          │
# │    Hash:              f1e2d3c4...                                    │
# │    Draft File:        ~/drafts/extractor_2024_001.py                 │
# │    User Action:       approved (2026-01-13 14:45:22)                 │
# │                                                                       │
# │  Privacy Mode:        standard                                       │
# │  Created:             2026-01-13 14:23:45 UTC                        │
# │  Completed:           2026-01-13 14:24:19 UTC                        │
# │                                                                       │
# └─────────────────────────────────────────────────────────────────────┘
```

### 4.3 Raw SQL Queries

Users can directly query the audit log for advanced analysis:

```sql
-- Error rate by wizard type (last 30 days)
SELECT
    wizard_type,
    COUNT(*) as attempts,
    SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) as errors,
    ROUND(100.0 * SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) / COUNT(*), 2) as error_pct
FROM cf_ai_audit_log
WHERE created_at > datetime('now', '-30 days')
GROUP BY wizard_type
ORDER BY error_pct DESC;

-- Slowest operations (>5 seconds)
SELECT
    wizard_type,
    model_name,
    duration_ms,
    input_preview,
    status,
    created_at
FROM cf_ai_audit_log
WHERE duration_ms > 5000
ORDER BY duration_ms DESC
LIMIT 20;

-- Most common error types (last 90 days)
SELECT
    error_code,
    COUNT(*) as count,
    ROUND(100.0 * COUNT(*) /
        (SELECT COUNT(*) FROM cf_ai_audit_log
         WHERE created_at > datetime('now', '-90 days')), 2) as pct
FROM cf_ai_audit_log
WHERE created_at > datetime('now', '-90 days')
  AND status = 'error'
GROUP BY error_code
ORDER BY count DESC;

-- Cost analysis (assuming $0.01 per 1M input tokens, $0.03 per 1M output tokens)
SELECT
    wizard_type,
    COUNT(*) as operations,
    SUM(input_byte_count) / 4 as estimated_input_tokens,
    SUM(output_byte_count) / 4 as estimated_output_tokens,
    ROUND((SUM(input_byte_count) / 4 * 0.00001) +
          (SUM(output_byte_count) / 4 * 0.00003), 2) as estimated_cost_usd
FROM cf_ai_audit_log
WHERE created_at > datetime('now', '-30 days')
  AND model_name IN ('gpt-4', 'gpt-3.5-turbo', 'claude-opus')
GROUP BY wizard_type;
```

---

## 5. Retention Schedule and Automation

### 5.1 Automated Cleanup Schedule

```rust
// In casparian_mcp/src/audit.rs or background task
pub struct AuditCleanupTask {
    schedule: "0 2 * * *",  // Daily at 02:00 UTC
    retention_policy: RetentionPolicy,
}

impl AuditCleanupTask {
    pub async fn run(&self) -> Result<CleanupStats, Error> {
        let db = open_audit_db()?;

        // Phase 1: Check current size
        let current_size_mb = self.get_table_size_mb(&db)?;

        // Phase 2: Delete by time (standard policy)
        let (time_deleted, time_reclaimed_mb) = self.delete_by_retention(&db)?;

        // Phase 3: If still over soft limit, delete more aggressively
        let current_size_after = self.get_table_size_mb(&db)?;
        let (aggressive_deleted, aggressive_reclaimed_mb) =
            if current_size_after > 400 {
                self.delete_aggressive(&db)?
            } else {
                (0, 0.0)
            };

        // Phase 4: Vacuum to reclaim space
        db.execute("VACUUM ANALYZE")?;

        Ok(CleanupStats {
            time_deleted,
            aggressive_deleted,
            total_reclaimed_mb: time_reclaimed_mb + aggressive_reclaimed_mb,
            current_size_mb: self.get_table_size_mb(&db)?,
        })
    }

    async fn delete_by_retention(&self, db: &Connection) -> Result<(u64, f64), Error> {
        let mut deleted = 0;
        let size_before = self.get_table_size_mb(db)?;

        // Delete successful operations older than retention_success days
        let days = self.retention_policy.retention_success;
        let n = db.execute(
            &format!("DELETE FROM cf_ai_audit_log
                     WHERE status = 'success'
                       AND created_at < datetime('now', '-{} days')", days),
            params![],
        )?;
        deleted += n as u64;

        // Delete errors older than retention_error days
        let days = self.retention_policy.retention_error;
        let n = db.execute(
            &format!("DELETE FROM cf_ai_audit_log
                     WHERE status IN ('error', 'timeout')
                       AND created_at < datetime('now', '-{} days')", days),
            params![],
        )?;
        deleted += n as u64;

        let size_after = self.get_table_size_mb(db)?;
        let reclaimed = (size_before - size_after).max(0.0);

        Ok((deleted, reclaimed))
    }

    async fn delete_aggressive(&self, db: &Connection) -> Result<(u64, f64), Error> {
        // Keep only latest 90 days regardless of type
        let size_before = self.get_table_size_mb(db)?;

        let deleted = db.execute(
            "DELETE FROM cf_ai_audit_log
             WHERE created_at < datetime('now', '-90 days')",
            params![],
        )? as u64;

        let size_after = self.get_table_size_mb(db)?;
        let reclaimed = (size_before - size_after).max(0.0);

        Ok((deleted, reclaimed))
    }
}
```

### 5.2 Manual Cleanup with Confirmation

```bash
# Check what would be deleted (dry run)
casparian ai audit --cleanup --dry-run

# Output:
# Cleanup Plan (dry run):
#   Successful operations (>90 days old): 847 rows (0.68 MB)
#   Error operations (>180 days old):     34 rows (0.12 MB)
#   Total to delete:                      881 rows (0.80 MB)
#   Retention policy: standard
#
# Execute cleanup with: casparian ai audit --cleanup --confirm

# Execute (requires explicit --confirm)
casparian ai audit --cleanup --confirm

# Shows summary
# Cleanup complete:
#   Deleted:          881 rows
#   Reclaimed:        0.80 MB
#   New table size:   2.34 MB
#   Next cleanup:     2026-01-14 02:00:00 UTC
```

---

## 6. Compliance and Export

### 6.1 Audit Export Formats

```bash
# JSON export (structured, suitable for analysis)
casparian ai audit --since 2026-01-01 --format json > audit.json

# CSV export (spreadsheet-friendly)
casparian ai audit --since 2026-01-01 --format csv > audit.csv

# JSONL export (streaming-friendly for large exports)
casparian ai audit --since 2026-01-01 --format jsonl > audit.jsonl

# Pretty-printed table (terminal)
casparian ai audit --since 2026-01-01 --format table
```

**JSON Export Example:**

```json
[
  {
    "id": "abc123def456",
    "wizard_type": "pathfinder",
    "status": "success",
    "model_name": "qwen2.5-coder:7b",
    "input_type": "path",
    "input_hash": "a7b3c2d1...",
    "input_preview": "/home/[USER]/projects/[PROJECT]/data/orders.csv",
    "redactions": ["username", "client_id"],
    "output_type": "extractor",
    "output_hash": "f1e2d3c4...",
    "output_file": "~/drafts/extractor_2024_001.py",
    "duration_ms": 1234,
    "user_action_on_draft": "approved",
    "privacy_mode": "standard",
    "created_at": "2026-01-13T14:23:45Z",
    "completed_at": "2026-01-13T14:24:19Z"
  }
]
```

### 6.2 Compliance Reporting

**For regulated industries (HIPAA, SOC 2, ISO 27001):**

```bash
# Generate compliance report for audit period
casparian ai audit --compliance-report --since 2025-01-01 --until 2025-12-31

# Output includes:
# - Total operations logged
# - Success vs failure rate
# - All errors with full error messages
# - Longest operations (for performance review)
# - Model usage summary
# - Redaction statistics
# - No users with access to AI features (if applicable)

# Can be signed with organizational key
casparian ai audit --compliance-report \
  --since 2025-01-01 \
  --sign-with /path/to/signing_key.pem \
  --output audit_report_2025.json.sig
```

---

## 7. Deletion and Permanent Records

### 7.1 When Individual Records Can Be Deleted

Users can request deletion of specific audit entries under GDPR Article 17 (right to erasure):

```bash
# Delete specific record by ID
casparian ai audit --delete abc123def456

# System shows 5-char verification code from the record
# User must type it to confirm (prevents accidental deletion)
# Once deleted, permanently gone (not recoverable)
```

### 7.2 When Records MUST Be Retained

In some contexts, audit logs cannot be deleted:

| Scenario | Min Retention | Reason |
|----------|--------------|--------|
| Fraud investigation ongoing | Until closed + 2y | Legal hold |
| User under dispute | Until resolved + 1y | Litigation |
| Data breach assessment | 3 years minimum | Regulatory requirement |
| Compliance audit active | Throughout + 2y | Auditor access |

**Implementation:**

```sql
-- Add hold_reason column for compliance
ALTER TABLE cf_ai_audit_log ADD COLUMN hold_reason TEXT;
-- Examples: 'fraud_investigation', 'legal_hold', 'breach_assessment'

-- Deletion blocked if hold_reason is set
DELETE FROM cf_ai_audit_log WHERE id = ?
  AND hold_reason IS NULL  -- Only allow delete if no hold
```

---

## 8. Database Schema Addition

Complete schema for audit log table:

```sql
CREATE TABLE cf_ai_audit_log (
    id TEXT PRIMARY KEY,
    wizard_type TEXT NOT NULL,
    model_name TEXT NOT NULL,
    input_type TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    input_preview TEXT,
    input_byte_count INTEGER,
    redactions TEXT,
    redaction_count INTEGER,
    output_type TEXT,
    output_hash TEXT,
    output_byte_count INTEGER,
    output_file TEXT,
    duration_ms INTEGER,
    status TEXT NOT NULL,
    error_message TEXT,
    error_code TEXT,
    user_hint TEXT,
    user_redaction_fields TEXT,
    user_action_on_draft TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    job_id TEXT,
    ai_model_endpoint TEXT,
    privacy_mode TEXT,
    hold_reason TEXT,

    CONSTRAINT valid_wizard_type CHECK (wizard_type IN
        ('pathfinder', 'parser_lab', 'labeling', 'semantic_path', 'pie')),
    CONSTRAINT valid_status CHECK (status IN
        ('success', 'timeout', 'error', 'user_cancel')),
    CONSTRAINT valid_privacy_mode CHECK (privacy_mode IN
        ('strict', 'standard', 'permissive', 'interactive'))
);

CREATE INDEX idx_ai_audit_wizard ON cf_ai_audit_log(wizard_type);
CREATE INDEX idx_ai_audit_created ON cf_ai_audit_log(created_at);
CREATE INDEX idx_ai_audit_status ON cf_ai_audit_log(status);
CREATE INDEX idx_ai_audit_model ON cf_ai_audit_log(model_name);
CREATE INDEX idx_ai_audit_error ON cf_ai_audit_log(error_code);
CREATE INDEX idx_ai_audit_hold ON cf_ai_audit_log(hold_reason)
    WHERE hold_reason IS NOT NULL;
```

---

## 9. Implementation Checklist

### Phase 1: Core Cleanup Infrastructure (1-2 days)
- [ ] Implement `AuditCleanupTask` with daily schedule
- [ ] Add time-based retention logic per section 2.1
- [ ] Add size-based monitoring per section 2.2
- [ ] Update `cf_ai_audit_log` schema with new columns
- [ ] Add unit tests for retention calculations

### Phase 2: CLI Commands (1 day)
- [ ] Implement `casparian ai audit --last N`
- [ ] Implement `casparian ai audit --since DATE --until DATE`
- [ ] Implement `casparian ai audit --wizard TYPE`
- [ ] Implement `casparian ai audit --stats`
- [ ] Implement `casparian ai audit show ID`
- [ ] Implement `casparian ai audit --cleanup --dry-run`
- [ ] Implement `casparian ai audit --cleanup --confirm`

### Phase 3: Export and Compliance (1 day)
- [ ] Implement JSON/CSV/JSONL export formats
- [ ] Add `--compliance-report` command
- [ ] Add cryptographic signing capability
- [ ] Add filter combinations (wizard + status + date range)

### Phase 4: Privacy and Deletion (1 day)
- [ ] Add `hold_reason` column and constraints
- [ ] Implement `casparian ai audit --delete ID` with confirmation
- [ ] Add GDPR compliance checks
- [ ] Implement legal hold mechanisms
- [ ] Add deletion logging (meta-audit)

### Phase 5: Configuration and Testing (1-2 days)
- [ ] Add `[audit]` section to config.toml
- [ ] Implement compliance mode overrides
- [ ] Add integration tests with real retention scenarios
- [ ] Document configuration with examples
- [ ] Update user documentation

---

## 10. Spec Updates Required

Add to `specs/ai_wizards.md` Section 7:

```markdown
### 7.6 Audit Log Retention Policy

**Time-Based Retention (default):**
- Successful operations: 90 days
- Failed operations: 180 days
- Critical errors: 365 days
- Always retain at least 1,000 most recent records

**Size-Based Limits:**
- Soft limit: 400 MB (triggers warning)
- Hard limit: 500 MB (begins aggressive deletion)
- Cleanup strategy: Delete oldest-first to keep last 180 days

**Privacy:**
- Input/output stored as hashes (not reversible)
- Redaction field names logged, but not actual values
- User hintsstored as-is (user-provided, already safe)
- No email, username, or API keys in logs

**Compliance Modes:**
```toml
[audit]
compliance_mode = "standard"      # standard | compliant | permissive | none
retention_success = 90            # Days to keep successful operations
retention_error = 180             # Days to keep failed operations
retention_critical = 365          # Days to keep critical errors
max_log_size_mb = 500             # Hard size limit
```

**Cleanup:**
- Automatic daily cleanup at 02:00 UTC
- Manual cleanup: `casparian ai audit --cleanup --dry-run` then `--confirm`
- Deletion: `casparian ai audit --delete ID` (requires verification code)
- Legal holds: Set `hold_reason` to prevent deletion

**Queries:**
- View: `casparian ai audit --last 10 --wizard pathfinder`
- Export: `casparian ai audit --since 2026-01-01 --format json > audit.json`
- Stats: `casparian ai audit --stats`
- Compliance: `casparian ai audit --compliance-report`

See `specs/meta/sessions/ai_wizards/round_023/engineer.md` for complete specification.
```

---

## 11. Summary

**Gap Resolution:**

GAP-AUDIT-001 is resolved with:

1. **Operations Scope** (Section 1): Defined what gets logged from each wizard type
2. **Retention Policy** (Section 2):
   - Time-based: 90d success, 180d error, 365d critical
   - Size-based: 500 MB hard limit with soft limit at 400 MB
   - Safety margin: Always keep latest 1,000 records
3. **Privacy** (Section 3): Hash-based storage, no PII in logs, GDPR-compliant deletion
4. **CLI Access** (Section 4): Rich query interface for users and compliance reporting
5. **Automation** (Section 5): Daily cleanup with manual override capability
6. **Compliance** (Section 6): Export formats, signed reports, regulatory holds
7. **Implementation** (Section 9): Phased rollout with testing strategy

This policy balances:
- **Compliance**: Meets GDPR, HIPAA, SOC 2 requirements
- **Usability**: Simple defaults, powerful customization
- **Storage**: Disk-efficient with automatic cleanup
- **Privacy**: No sensitive data in logs, user control over deletion

---

## References

- `specs/ai_wizards.md` Section 7.3 (Audit Log Table)
- `specs/ai_wizards.md` Section 7.4 (Audit CLI)
- GDPR Article 17 (Right to Erasure)
- HIPAA Audit Controls (§164.312(b))
- SOC 2 Control CC6.1 (Logical Access Controls)

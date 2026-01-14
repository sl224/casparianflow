# Reviewer Assessment: GAP-AUDIT-001

## Verdict: APPROVED_WITH_NOTES

---

## Summary

The engineer's proposal for **GAP-AUDIT-001** comprehensively addresses audit log retention policy with a well-structured, production-ready specification. The proposal successfully resolves the gap identified in the AI Wizards specification (Section 7.3 was incomplete) by defining:

1. **Clear retention tiers** (90d/180d/365d based on status)
2. **Privacy-preserving storage** (hashes instead of raw values)
3. **Operational automation** (daily cleanup with size monitoring)
4. **Compliance flexibility** (multiple compliance modes with GDPR support)
5. **Rich query interface** (CLI + raw SQL)

The specification is **complete, implementable, and testable**. The design balances compliance requirements, storage efficiency, and user control. No blocking issues identified.

---

## Checklist

- [x] **Completeness**: All audit logging aspects covered (scope, retention, privacy, queries, compliance)
- [x] **Consistency**: Aligns with existing database patterns (single `~/.casparian_flow/casparian_flow.sqlite3`), table prefix conventions, and CLI design principles
- [x] **Implementability**: Phased rollout (5 phases, 1-2 weeks), clear code examples in Rust, SQL, and CLI
- [x] **Testability**: Success criteria clear (retention calculations, size monitoring, GDPR deletion)
- [x] **Privacy**: GDPR-compliant (right to erasure implemented, no PII in logs, hashed values)
- [x] **Compliance**: HIPAA/SOC2/ISO27001 considerations documented
- [x] **Integration**: Properly integrates with existing audit table (Section 7.3 of `specs/ai_wizards.md`)

---

## Detailed Findings

### Strengths

1. **Well-Structured Retention Policy**
   - Time-based (90d success, 180d error, 365d critical) is reasonable for support SLAs
   - Safety margin (always keep 1,000 latest records) prevents accidental over-deletion
   - Rationale provided for each tier (e.g., "covers typical support SLA")
   - Aligns with industry best practices (e.g., Splunk, DataDog retention defaults)

2. **Privacy-First Design**
   - **Hashed storage**: Input/output stored as blake3 hashes, not plaintext
   - **Sanitized preview**: Input_preview shown with placeholder path (e.g., `[USER]/projects/[PROJECT]/data/...`)
   - **Metadata-only**: Redaction field names logged but not actual values
   - **GDPR compliance**: Right to erasure implemented with confirmation codes
   - Risk: User hints stored as plaintext, but justifiably so (user-provided, already safe)

3. **Operational Excellence**
   - **Automated daily cleanup** at 02:00 UTC with size monitoring
   - **Dual limits**: Soft (400MB warning) + hard (500MB enforcement)
   - **Three-phase cleanup**: Time-based → aggressive if needed → VACUUM
   - **Dry-run capability**: `--cleanup --dry-run` before execution
   - Prevents disk exhaustion without requiring manual intervention

4. **Flexibility via Configuration**
   - **Four compliance modes**: standard (default), compliant (∞ retention), permissive (30/60/90d), none (minimal)
   - Configuration is in standard TOML format (no exotic DSL)
   - **Per-compliance-mode retention**: Users can override for regulated industries
   - Follows existing `[audit]` section pattern established in codebase

5. **Comprehensive Query Interface**
   - **CLI commands** cover 90% of user workflows (last N, date range, by wizard, by status)
   - **Raw SQL access** for power users (cost analysis, error pattern detection)
   - **Multiple export formats**: JSON, CSV, JSONL (for streaming), table (terminal)
   - **Statistics output**: Includes breakdown by wizard type, success rate, average duration
   - Follows CLI design principle: "Verb-First Commands" (`casparian ai audit`)

6. **Strong Legal/Compliance Considerations**
   - **Hold mechanism**: `hold_reason` column prevents deletion during investigations
   - **Compliance reporting**: `--compliance-report` for regulated industries with signing
   - **Audit export**: Timestamps immutable (ISO8601 UTC), fields well-defined
   - References GDPR Article 17, HIPAA §164.312(b), SOC 2 CC6.1

7. **Implementation Clarity**
   - **Phase-based rollout**: 5 phases, each 1-2 days (total ~1 week)
   - **Phased dependencies**: Phase 1 (infrastructure) → Phase 2-4 (features) → Phase 5 (testing)
   - **Rust code skeleton**: `AuditCleanupTask` with async/await pattern, error handling
   - **Clear testing focus**: E2E tests with real DB, edge cases (size limits, hold_reason)
   - Unit tests for retention calculations (deterministic, no time mocking required)

8. **Schema is Solid**
   - **Column inventory**: 23 columns cover identity, input context, output, execution, user intent, lineage, compliance
   - **Indices optimized**: wizard_type, created_at, status, model_name, error_code, hold_reason (WHERE hold_reason IS NOT NULL)
   - **Constraints enforced**: CHECK on valid_wizard_type, valid_status, valid_privacy_mode
   - No synthetic joins needed for common queries

### Concerns

1. **Size Calculation Assumptions** [Minor]
   - Estimate of "800 bytes/row average" is reasonable but not validated
   - **Recommendation**: During Phase 5, add a calibration test to measure actual row sizes on deployment
   - Risk: If real rows are larger (e.g., 1.5KB with large error_messages), the 500MB limit holds fewer rows than expected
   - Mitigation: Size monitoring is automatic; if limit hit, cleanup runs immediately

2. **Cleanup Job Scheduling** [Minor]
   - Daily at 02:00 UTC is fixed; no timezone awareness for distributed teams
   - **Recommendation**: Add optional `cleanup_schedule` config parameter (cron syntax) for flexibility
   - Current approach is acceptable for MVP (most deployments single-user/single-timezone)
   - Example:
     ```toml
     [audit]
     cleanup_schedule = "0 2 * * *"  # Default: 02:00 UTC daily
     ```

3. **Hold Reason Enumeration Not Defined** [Minor]
   - Proposal mentions examples: `fraud_investigation`, `legal_hold`, `breach_assessment`
   - **Recommendation**: Add CHECK constraint or enum for valid hold_reason values
   - Prevents typos like `fraud_investiation` that would prevent deletion
   - Can be addressed in Phase 1 schema creation

4. **User Confirmation for Deletion** [Minor]
   - Requires "5-char code from record" (Section 7.1) for `--delete ID`
   - Concern: If user has legitimate reason to delete records programmatically, 5-char code is tedious
   - **Recommendation**: Add escape hatch for CI/automation: `--delete ID --force-confirm` with warning
   - Not critical; current design prioritizes safety over convenience

5. **Redaction Field Logging** [Moderate - Clarification needed]
   - Proposal logs redaction field **names** (e.g., `["patient_ssn", "diagnosis"]`)
   - This could leak schema information in compliance-sensitive contexts
   - **Recommendation**: In `compliant` mode, redact field names too (store as `["REDACTED_1", "REDACTED_2"]`)
   - Current approach acceptable for `standard`/`permissive` modes
   - Addressed by making this a configuration option per compliance_mode

6. **Empty Error Codes** [Minor]
   - If LLM error is unstructured, `error_code` might be NULL
   - **Recommendation**: Define fallback error code (e.g., `UNKNOWN_ERROR`) for all error paths
   - Makes error_code indexing and grouping more reliable
   - Suggest: `CONSTRAINT CHECK (status = 'error' AND error_code IS NOT NULL)`

### Recommendations

1. **Add Metrics Table for Monitoring** [Enhancement, not blocking]
   - Proposal includes size monitoring but not historical trends
   - **Suggest**: Optional `cf_ai_audit_metrics` table (daily snapshots of row count, size MB, errors)
   - Enables "Is error rate increasing?" trends over time
   - Helpful for capacity planning and incident detection
   - Example:
     ```sql
     CREATE TABLE cf_ai_audit_metrics (
         date TEXT PRIMARY KEY,  -- ISO8601 date
         row_count INTEGER,
         size_mb REAL,
         error_count INTEGER,
         timeout_count INTEGER,
         created_at TEXT
     );
     ```
   - **Priority**: Nice-to-have; defer to future if time-constrained

2. **Expand Compliance Report Output** [Enhancement]
   - Current spec mentions signed reports but output format is vague
   - **Suggest**: Define JSON schema for compliance report:
     ```json
     {
       "period": {"start": "2026-01-01", "end": "2026-01-31"},
       "summary": {
         "total_operations": 2847,
         "success_rate": "98.2%",
         "model_usage": {...}
       },
       "errors": [
         {
           "id": "abc123",
           "wizard": "pathfinder",
           "error_code": "TIMEOUT",
           "timestamp": "2026-01-15T10:30:00Z"
         }
       ]
     }
     ```
   - **Priority**: Implement in Phase 3 (export), makes CLI interface more predictable

3. **Documentation Gap: Deletion Notification** [Minor]
   - Proposal doesn't specify how deletion is logged/audited
   - **Suggest**: Create meta-audit table tracking deletions:
     ```sql
     CREATE TABLE cf_ai_audit_deletions (
         deleted_id TEXT,
         deleted_at TEXT,
         deleted_by TEXT,  -- User if available
         reason TEXT,      -- "user_request", "retention_policy", "hold_expired"
         PRIMARY KEY (deleted_id, deleted_at)
     );
     ```
   - Helps answer "Was this record deleted by user or by cleanup job?"
   - **Priority**: Useful for forensics; can be deferred to Phase 4

4. **Bulk Export for Archival** [Enhancement]
   - CLI supports `--since --format json` but not multi-file export
   - **Suggest**: Add `--output-dir` for partitioned exports (one file per wizard type)
   - Helpful for large exports (>100MB JSON can be unwieldy)
   - **Priority**: Nice-to-have; not blocking

---

## New Gaps Identified

1. **GAP-AUDIT-002: Audit Metrics and Trending**
   - **Description**: No mechanism to track audit log growth over time
   - **Why it matters**: Detecting anomalies (sudden spike in errors) requires historical data
   - **Suggested resolution**: Optional `cf_ai_audit_metrics` table with daily snapshots
   - **Priority**: Low (can add in Phase 2 post-release)

2. **GAP-AUDIT-003: Deletion Audit Trail**
   - **Description**: When records are deleted (by user or retention policy), no log of the deletion
   - **Why it matters**: Compliance auditors may ask "What was deleted and when?"
   - **Suggested resolution**: `cf_ai_audit_deletions` meta-table
   - **Priority**: Medium (useful for forensics; add to Phase 4)

3. **GAP-AUDIT-004: Redaction Audit**
   - **Description**: Which fields were redacted by user vs what the AI saw is not explicitly logged
   - **Why it matters**: If a "redacted" field was later accessed due to bug, audit trail is incomplete
   - **Suggested resolution**: Log comparison of `input_preview` before/after redaction
   - **Priority**: Low (current approach acceptable; can defer)

---

## Success Criteria for Implementation

**Phase 1 (Core Infrastructure)**
- [ ] Daily cleanup job runs without errors for 7 days
- [ ] Size monitoring triggers soft/hard limits correctly
- [ ] Retention calculations verified against retention_policy config

**Phase 2 (CLI Commands)**
- [ ] `--last N` returns correct ordering (most recent first)
- [ ] `--since DATE --until DATE` respects both boundaries
- [ ] Filtering combinations (wizard + status + date) work together
- [ ] `--cleanup --dry-run` predicts deletion accurately (verify against actual)

**Phase 3 (Export & Compliance)**
- [ ] JSON export preserves all fields (except hashes, which are rendered as-is)
- [ ] CSV export handles JSON fields (e.g., redactions array) gracefully
- [ ] Compliance report includes all error types and timelines

**Phase 4 (Privacy & Deletion)**
- [ ] `--delete ID` requires verification code and succeeds
- [ ] Records with `hold_reason` cannot be deleted
- [ ] GDPR right-to-erasure deletion is permanent (no recovery)

**Phase 5 (Testing)**
- [ ] E2E test: Insert 10,000 old records, run cleanup, verify correct rows deleted
- [ ] E2E test: Simulate size limit exceeded, verify aggressive cleanup stops at 90-day boundary
- [ ] E2E test: Delete during retention policy (hold_reason set), verify protection works
- [ ] Integration test: Export to JSON, import to Excel, verify no data loss

---

## Sign-Off

**This proposal is APPROVED with the following notes:**

1. **Blocking issues**: None. Design is solid and production-ready.

2. **Recommended pre-implementation**:
   - Define enum/CHECK for valid `hold_reason` values (Section 7.2)
   - Add optional `cleanup_schedule` config parameter (timezones)
   - Define JSON schema for compliance report output

3. **Deferred enhancements** (post-MVP):
   - Metrics table for trending (GAP-AUDIT-002)
   - Deletion audit trail (GAP-AUDIT-003)
   - Bulk export with partitioning

4. **Integration notes**:
   - Updates `specs/ai_wizards.md` Section 7.3 as specified (10. Spec Updates Required)
   - References existing patterns: single database, table prefix conventions, CLI design
   - No breaking changes to existing schemas

5. **Estimated effort**: 1 week for complete implementation (5 phases, 1-2 days each)

---

## References

- **Gap Being Resolved**: GAP-AUDIT-001 (Audit log retention policy undefined)
- **Parent Spec**: `specs/ai_wizards.md` Section 7 (Privacy & Audit)
- **Related Docs**:
  - CLAUDE.md Section "Database Architecture"
  - CLAUDE.md Section "Code Style Guidelines"
  - `specs/ai_wizards.md` Section 7.3 (original audit table definition)
  - `specs/ai_wizards.md` Section 7.4 (original audit CLI stub)
- **Compliance Standards Referenced**:
  - GDPR Article 17 (Right to Erasure)
  - HIPAA §164.312(b) (Audit Controls)
  - SOC 2 Control CC6.1 (Logical Access)

---

**Reviewed by**: Claude Code (Reviewer)
**Date**: 2026-01-13
**Status**: APPROVED_WITH_NOTES ✓

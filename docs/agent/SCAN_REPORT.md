# Phase 0 Scan Report

Generated: 2026-01-22

## Async/Tokio Usage
**Command:** `rg -n "(tokio::|async fn|\.await)" crates`
**Total:** 107 occurrences across 25 files

### Files with async/tokio
| File | Occurrences |
|------|-------------|
| crates/casparian_mcp/src/jobs/executor.rs | 22 |
| crates/casparian_mcp/src/server.rs | 12 |
| crates/casparian_mcp/src/tools/intent_publish.rs | 5 |
| crates/casparian_mcp/src/tools/intent_backtest.rs | 5 |
| crates/casparian_mcp/src/tools/approval.rs | 4 |
| crates/casparian_mcp/src/tools/job.rs | 4 |
| crates/casparian_mcp/src/tools/intent_fileset.rs | 3 |
| crates/casparian_mcp/src/tools/intent_session.rs | 3 |
| crates/casparian_mcp/src/tools/backtest.rs | 2 |
| crates/casparian_mcp/src/tools/intent_select.rs | 2 |
| crates/casparian_mcp/src/tools/intent_tags.rs | 2 |
| crates/casparian_mcp/src/tools/intent_path_fields.rs | 2 |
| crates/casparian_mcp/src/tools/intent_schema.rs | 2 |
| crates/casparian_mcp/src/tools/registry.rs | 2 |
| crates/casparian/src/cli/mcp.rs | 2 |
| crates/casparian_mcp/src/tools/mod.rs | 1 |
| crates/casparian_mcp/src/tools/run.rs | 1 |
| crates/casparian_mcp/src/tools/scan.rs | 1 |
| crates/casparian_mcp/src/tools/preview.rs | 1 |
| crates/casparian_mcp/src/tools/query.rs | 1 |
| crates/casparian_mcp/src/tools/plugins.rs | 1 |
| crates/casparian_worker/src/venv_manager.rs | 1 |

**Primary location:** casparian_mcp crate (MCP server tools)

---

## Locks (Mutex/RwLock)
**Command:** `rg -n "(Arc<Mutex<|Mutex<|RwLock<|parking_lot::)" crates`
**Total:** 90 occurrences across 26 files

### Files with locks
| File | Occurrences |
|------|-------------|
| crates/casparian_mcp/src/tools/intent_publish.rs | 10 |
| crates/casparian_mcp/src/tools/intent_backtest.rs | 10 |
| crates/casparian_mcp/src/tools/approval.rs | 6 |
| crates/casparian_mcp/src/tools/job.rs | 6 |
| crates/casparian_mcp/src/tools/intent_fileset.rs | 6 |
| crates/casparian_mcp/src/tools/intent_session.rs | 6 |
| crates/casparian_mcp/src/tools/intent_select.rs | 4 |
| crates/casparian_mcp/src/tools/intent_tags.rs | 4 |
| crates/casparian_mcp/src/tools/intent_schema.rs | 4 |
| crates/casparian_mcp/src/tools/intent_path_fields.rs | 4 |
| crates/casparian_mcp/src/jobs/executor.rs | 3 |
| crates/casparian_mcp/src/server.rs | 2 |
| crates/casparian_mcp/src/tools/backtest.rs | 2 |
| crates/casparian_mcp/src/tools/run.rs | 2 |
| crates/casparian_mcp/src/tools/preview.rs | 2 |
| crates/casparian_mcp/src/tools/query.rs | 2 |
| crates/casparian_mcp/src/tools/registry.rs | 2 |
| crates/casparian_mcp/src/tools/plugins.rs | 2 |
| crates/casparian_mcp/src/tools/scan.rs | 2 |
| crates/casparian_mcp/src/tools/mod.rs | 2 |
| crates/casparian_worker/src/venv_manager.rs | 2 |
| crates/casparian/src/cli/tui/app.rs | 2 |
| crates/casparian_mcp/src/security/audit.rs | 1 |
| crates/casparian/benches/scanner_perf.rs | 1 |
| crates/casparian/src/cli/scan.rs | 1 |

**Primary location:** casparian_mcp crate (Arc<Mutex<...>> pattern for managers)

---

## Silent Corruption: unwrap_or_default()
**Command:** `rg -n "unwrap_or_default\(" crates`
**Total:** 105 occurrences across 32 files

### Top files
| File | Occurrences |
|------|-------------|
| crates/casparian/src/storage/duckdb.rs | 22 |
| crates/casparian/src/cli/tui/app.rs | 11 |
| crates/casparian/src/cli/parser.rs | 9 |
| crates/casparian_sentinel/src/db/queue.rs | 7 |
| crates/casparian/src/cli/rule.rs | 5 |
| crates/casparian/src/cli/topic.rs | 5 |
| crates/casparian/src/cli/tui/ui.rs | 5 |
| crates/casparian/src/cli/worker.rs | 4 |
| crates/casparian/src/scout/db.rs | 4 |

**Risk:** DB reads that default to empty values when NULL encountered

---

## Fallback to Now: Utc::now()
**Command:** `rg -n "Utc::now\(" crates`
**Total:** 70 occurrences across 35 files

### Top files
| File | Occurrences |
|------|-------------|
| crates/casparian/src/scout/db.rs | 8 |
| crates/casparian_mcp/src/jobs/mod.rs | 8 |
| crates/casparian_mcp/src/approvals/mod.rs | 4 |
| crates/casparian/src/ai/draft.rs | 4 |
| crates/casparian_sentinel/src/db/api_storage.rs | 3 |
| crates/casparian_mcp/src/security/audit.rs | 3 |
| crates/casparian_worker/src/venv_manager.rs | 3 |

**Risk:** Using current time as fallback when timestamp parsing fails

---

## Stringly-Typed Status
**Command:** `rg -n "(as_str\(\)|"PENDING"|"SUCCESS"|"FAILED")" crates`
**Total:** 661 occurrences across 77 files

### Top files
| File | Occurrences |
|------|-------------|
| crates/casparian/src/scout/db.rs | 81 |
| crates/casparian_sentinel/src/sentinel.rs | 43 |
| crates/casparian_sentinel/src/db/queue.rs | 44 |
| crates/casparian_sentinel/tests/integration.rs | 40 |
| crates/casparian_protocol/src/types.rs | 40 |

**Risk:** String matching for status values instead of enum patterns

---

## Summary

| Category | Count | Primary Location |
|----------|-------|------------------|
| Async/Tokio | 107 | casparian_mcp |
| Locks | 90 | casparian_mcp |
| unwrap_or_default | 105 | storage/db layers |
| Utc::now fallback | 70 | multiple crates |
| Stringly-typed | 661 | scout/sentinel |

**Priority for Phase 1:** Remove async/tokio from casparian_mcp
**Priority for Phase 2-3:** Fix unwrap_or_default and stringly-typed patterns
**Priority for Phase 4:** Audit and remove locks

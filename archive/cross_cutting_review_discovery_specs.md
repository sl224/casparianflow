# Cross-Cutting Review: Discovery Specs Refinement v2

**Date:** 2026-01-13
**Reviewed Specs:**
1. `specs/discover.md` (v1.2) - Discover TUI View
2. `roadmap/spec_discovery_intelligence.md` (v0.4) - Discovery Intelligence

---

## 1. Summary

Both specs were refined independently in parallel worktrees. This cross-cutting review identifies consistency issues, integration points, and shared patterns that require attention.

**Overall Assessment:** The specs are complementary with no major conflicts. Minor clarifications recommended.

---

## 2. Terminology Consistency

### 2.1 Consistent Usage (Good)
| Term | discover.md | spec_discovery_intelligence.md | Status |
|------|-------------|-------------------------------|--------|
| Source | Directory being watched | File origin for drift detection | Aligned |
| Tag | Category for files | N/A (different scope) | N/A |
| Signature | N/A | Structural fingerprint hash | N/A |

### 2.2 Potential Confusion
| Term | Issue | Recommendation |
|------|-------|----------------|
| `status` | discover.md: `FileStatus` enum (Discovered, Queued, etc.); discovery_intel: outlier `status` (pending, handled, ignored) | These are different concepts. Add clarifying comments in both specs to prevent confusion. |

---

## 3. Type System Alignment

### 3.1 Worker Type Inference Integration

**Issue:** `spec_discovery_intelligence.md` adds `Unknown` type to the Type Lattice (Section 7.2), but `casparian_worker` crate may not have this type yet.

**Cross-cutting action required:**
- [ ] Update `crates/casparian_worker/CLAUDE.md` to document `Unknown` type
- [ ] Add `Unknown` variant to `DataType` enum in worker crate when implementing

### 3.2 FileStatus Extension

**Observation:** discover.md defines `FileStatus`:
```rust
pub enum FileStatus {
    Discovered,
    Queued,
    Processing,
    Complete,
    Failed,
}
```

**Recommendation:** Consider adding `Fingerprinted` status to track files that have been structurally analyzed but not yet processed, bridging the gap between discovery (discover.md) and fingerprinting (spec_discovery_intelligence.md).

---

## 4. Database Schema Integration

### 4.1 Scout Tables (discover.md)
- `scout_sources` - Directories being watched
- `scout_files` - Discovered files with tags
- `scout_tagging_rules` - Pattern → tag mappings

### 4.2 Fingerprint Tables (spec_discovery_intelligence.md)
- `cf_file_signatures` - File structural fingerprints
- `cf_signature_groups` - Grouped signatures
- `cf_structural_outliers` - Detected outliers
- `cf_header_mappings` - Column mappings
- `cf_source_signatures` - Drift tracking

### 4.3 Integration Point

**Recommendation:** Add foreign key relationship between `scout_files` and `cf_file_signatures`:

```sql
-- Add to scout_files table
ALTER TABLE scout_files ADD COLUMN file_hash TEXT;
ALTER TABLE scout_files ADD COLUMN signature_hash TEXT;

-- Foreign keys
FOREIGN KEY (file_hash) REFERENCES cf_file_signatures(file_hash)
FOREIGN KEY (signature_hash) REFERENCES cf_signature_groups(signature_hash)
```

This allows the Discover TUI to show fingerprint status directly in the files panel.

---

## 5. UI/UX Pattern Consistency

### 5.1 Dialog Patterns

Both specs use dialog overlays. Ensure consistent keybindings:

| Pattern | discover.md | Recommendation |
|---------|-------------|----------------|
| Tab navigation | Tab/Shift+Tab for field switching | Standardize for all dialogs |
| Confirmation | Enter to confirm | Standardize for all dialogs |
| Cancellation | Esc to cancel | Standardize for all dialogs |

### 5.2 State Machine Pattern

discover.md introduces `DiscoverViewState` enum pattern:
```rust
pub enum DiscoverViewState {
    Files,
    SourcesDropdown,
    TagsDropdown,
    RulesManager,
    RuleCreation,
    Wizard,
}
```

**Cross-cutting recommendation:** Create a shared pattern document for TUI state machines that can be referenced by future view specs (e.g., `specs/meta/tui_state_machine_patterns.md`).

---

## 6. Cross-Reference Updates

### 6.1 New References Needed

| From | To | Add Reference |
|------|-----|---------------|
| `spec_discovery_intelligence.md` | `specs/discover.md` | "See discover.md for TUI presentation of fingerprinted files" |
| `specs/discover.md` | `roadmap/spec_discovery_intelligence.md` | "See spec_discovery_intelligence.md for fingerprint-based file grouping" |

### 6.2 Existing References (Verified Valid)
- spec_discovery_intelligence.md → spec.md
- spec_discovery_intelligence.md → CLAUDE.md
- spec_discovery_intelligence.md → crates/casparian_worker/CLAUDE.md
- specs/discover.md → spec.md Section 5.3

---

## 7. Shared Enums/Types Candidates

The following types could potentially be shared or defined in a common location:

### 7.1 Candidate: FileStatus + Fingerprint Status

```rust
// Proposed unified enum
pub enum FileLifecycleStatus {
    // Discovery phase (from discover.md)
    Discovered,
    Tagged,

    // Fingerprinting phase (from spec_discovery_intelligence.md)
    Fingerprinted,
    HasOutliers,

    // Processing phase
    Queued,
    Processing,
    Complete,
    Failed,
}
```

**Decision:** Defer to implementation phase. Current separate enums are acceptable.

---

## 8. Open Actions

### 8.1 Immediate (Before Merge)
- [x] Document cross-cutting concerns (this file)
- [ ] Add clarifying comments to `FileStatus` in discover.md
- [ ] Add clarifying comments to outlier `status` in spec_discovery_intelligence.md

### 8.2 Future Work
- [ ] Create `specs/meta/tui_state_machine_patterns.md` shared pattern document
- [ ] Add `Unknown` type to worker crate DataType enum
- [ ] Consider `Fingerprinted` status addition to FileStatus
- [ ] Database migration to link scout_files → cf_file_signatures

---

## 9. Merge Checklist

Before merging the worktree branches:

- [x] W1 committed: `08de8fd` on `feat/spec-refine-discover`
- [x] W2 committed: `6b8299c` on `feat/spec-refine-discovery-intel`
- [x] Cross-cutting review complete (this document)
- [ ] Add cross-references between specs
- [ ] Merge W1 to main
- [ ] Merge W2 to main
- [ ] Clean up worktrees

---

## 10. Revision History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-13 | Claude Opus 4.5 | Initial cross-cutting review |

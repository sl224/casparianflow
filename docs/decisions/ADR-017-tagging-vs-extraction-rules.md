# ADR-017: Tagging Rules vs Extraction Rules

**Status:** Accepted
**Date:** 2026-01-14
**Context:** Spec maintenance audit identified confusion between two rule systems

---

## Decision

Casparian Flow has **two distinct rule systems** that serve different purposes:

### 1. Tagging Rules (Simple)

**Table:** `scout_tagging_rules`
**Purpose:** Assign a single tag to files matching a glob pattern
**Created via:** Discover mode → `n` key
**Spec:** `specs/views/discover.md` Section 2.2

```sql
CREATE TABLE scout_tagging_rules (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    source_id BIGINT NOT NULL,
    pattern TEXT NOT NULL,     -- e.g., "*.csv"
    tag TEXT NOT NULL,          -- e.g., "sales_data"
    priority INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1
);
```

**Use case:** Quick file categorization
- "All `.csv` files in this source → tag as `sales_data`"
- "Files matching `*_report.xlsx` → tag as `reports`"

### 2. Extraction Rules (Advanced)

**Table:** `extraction_rules` + `extraction_fields` + `extraction_tag_conditions`
**Purpose:** Extract structured metadata from file paths with conditional tagging
**Created via:** CLI or Extraction Rules TUI view
**Spec:** `specs/extraction.md` (API), `archive/specs/views/extraction_rules.md` (TUI)

```sql
CREATE TABLE extraction_rules (
    id TEXT PRIMARY KEY,
    source_id BIGINT,
    name TEXT NOT NULL,
    glob_pattern TEXT NOT NULL,  -- e.g., "{client}/{year}/**/*.pdf"
    description TEXT,
    priority INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1
);

CREATE TABLE extraction_fields (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    field_name TEXT NOT NULL,    -- e.g., "client", "year"
    source_type TEXT NOT NULL,   -- "path_segment", "regex", "literal"
    source_value TEXT            -- segment index or regex pattern
);

CREATE TABLE extraction_tag_conditions (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL,
    field_name TEXT NOT NULL,
    operator TEXT NOT NULL,      -- "equals", "contains", "matches"
    value TEXT NOT NULL,
    tag TEXT NOT NULL            -- Conditional tag assignment
);
```

**Use case:** Complex path parsing with metadata extraction
- "Extract `{client}`, `{year}`, `{quarter}` from path segments"
- "If `client` equals 'Acme', tag as `priority_client`"

---

## Relationship

```
Tagging Rules (Simple)          Extraction Rules (Advanced)
────────────────────────       ─────────────────────────────
pattern → tag                   pattern → fields → conditional tags
                                         ↓
                               Superset: Can do everything tagging
                               rules can, plus field extraction
```

**Key insight:** Extraction rules are a superset. A tagging rule is conceptually an extraction rule with zero fields and one unconditional tag.

---

## When to Use Each

| Scenario | Use |
|----------|-----|
| Quick categorization by extension | Tagging Rule |
| Tag files by folder location | Tagging Rule |
| Extract client/date/project from path | Extraction Rule |
| Conditional tagging based on path content | Extraction Rule |
| Batch processing with metadata columns | Extraction Rule |

---

## Future Consideration

**Potential consolidation:** Tagging rules could be migrated to extraction rules with:
- `fields = []` (no extraction)
- `tag_conditions = [unconditional tag]`

This would simplify the codebase to one rule system. However, the simple tagging UI in Discover mode is valuable for quick operations, so both UIs should remain even if the backend consolidates.

---

## Consequences

1. **Documentation:** Specs must clearly state which system they reference
2. **TUI:** Discover mode uses tagging rules; Extraction Rules view uses extraction rules
3. **Migration path:** If consolidation happens, tagging rules can be auto-migrated
4. **No breaking change:** Both systems continue to work independently

---

## Related Specs

- `specs/views/discover.md` - Tagging rules UI
- `specs/extraction.md` - Extraction rules API
- `archive/specs/views/extraction_rules.md` - Extraction rules UI
- `crates/casparian/src/scout/db.rs` - Both table definitions

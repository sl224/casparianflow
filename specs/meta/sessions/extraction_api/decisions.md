# User Decisions - Extraction API Refinement

**Session:** extraction_api
**Started:** 2026-01-12

---

## Round 0 (Initial Setup)

### Spec Scope
**Choice:** Merge into one
**Rationale:** Single spec, simpler mental model, semantic features inline

### Priority
**Choice:** Radically simple for common case + advanced API for complex cases
**Rationale:** User wants simplicity by default but escape hatches for power users

### Verticals
**Choice:** All four (defense, healthcare, finance, legal)
**Rationale:** Comprehensive template coverage despite more validation work

### Authoring Flow
**Choice:** Example-first primary
**Rationale:** TUI/CLI starts from examples, YAML is escape hatch

---

## Implications for Refinement

1. **Merge specs** - `semantic_path_mapping.md` folds into `extraction_rules.md`
2. **Two-tier API** - Simple (example-first) + Advanced (full YAML)
3. **Four templates** - Defense, Healthcare, Finance, Legal all required
4. **Progressive disclosure** - Common case is trivial, complex case is possible

---

## Round 1 Decisions

### CRIT-001: Single-File Inference
**Choice:** Template-first for single file
**Behavior:** Single file → match against templates. Multiple files → full inference

### HIGH-001: Semantic Layer
**Choice:** Keep as optional syntax
**Behavior:** Power users can write `entity_folder(mission) > dated_hierarchy` in YAML

### MED-004: Equivalence Classes
**Choice:** Keep in v1
**Behavior:** Same-structure sources share rules automatically (algorithmic, no AI)

---

## Decision Log

| Date | Round | Decision ID | Choice | Rationale |
|------|-------|-------------|--------|-----------|
| 2026-01-12 | 0 | SCOPE | Merge into one | Simpler mental model |
| 2026-01-12 | 0 | PRIORITY | Radical simple + advanced escape hatch | Best of both |
| 2026-01-12 | 0 | VERTICALS | All four | Comprehensive coverage |
| 2026-01-12 | 0 | AUTHORING | Example-first primary | YAML as escape hatch |
| 2026-01-12 | 1 | CRIT-001 | Template-first for single file | Inference needs variation |
| 2026-01-12 | 1 | HIGH-001 | Keep semantic syntax optional | Power user value |
| 2026-01-12 | 1 | MED-004 | Keep equivalence in v1 | Algorithmic, no AI needed |

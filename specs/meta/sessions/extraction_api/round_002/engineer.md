# Engineer Proposal - Round 2 (Revised)

## Gap Resolution: GAP-SIMPLE-001 (Revised)

**Confidence:** HIGH

### Summary of Changes from Round 1

1. **Template-first inference for single files** - Match against curated templates
2. **Semantic syntax is power-user optional** - `semantic:` field exists but never required
3. **Equivalence classes retained** - Algorithmic structure fingerprinting
4. **Tag-only rules explicitly supported** - `extract: null` is valid

### Tier 1: Simple API

**Inference Strategy:**
- IF N == 1: Template Matching (5+ built-in templates)
- ELSE IF N >= 3: Full Algorithmic Inference with confidence scores
- ELSE (N == 2): Hybrid with lower confidence

**Templates:** defense_mission_dated, healthcare_patient_mrn, finance_daily_reports, legal_case_matter, generic_project_dated

**Confidence Display:** 0-100% with evidence, user correction UI, "scan more files" prompt for low confidence (<50%)

### Tier 2: Advanced API

**Optional Semantic Syntax:**
```yaml
# Power user shorthand
- semantic: "entity_folder(mission) > dated_hierarchy(iso) > **/*.pdf"
  tags: [sitreps]

# Equivalent explicit form
- glob: "{mission}/{date}/**/*.pdf"
  extract:
    mission: "^([^/]+)"
    date: "/(\\d{4}-\\d{2}-\\d{2})/"
  tags: [sitreps]
```

**Tag-Only Rules:**
```yaml
- glob: "**/README.md"
  extract: null
  tags: [documentation]
```

### Equivalence Classes

**Structure Fingerprinting:**
- depth_distribution
- segment_patterns (Fixed, Variable, DatePattern, NumericSequence)
- extension_distribution
- date_format

**Detection:** >90% similarity → same equivalence class
**Workflow:** New source auto-detects match, prompts to apply shared rules

### Remaining Gaps

1. Template curation process
2. Semantic expression discoverability (`casparian semantic --list`)
3. Confidence threshold configuration
4. Equivalence class divergence detection
5. Template versioning/migration

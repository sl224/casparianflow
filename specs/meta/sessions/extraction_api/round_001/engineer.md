# Engineer Proposal - Round 1

## Gap Resolution: GAP-SIMPLE-001

**Confidence:** HIGH

### Proposed Solution

A **Unified Extraction API** that provides radical simplicity for the 80% case (point-and-extract) while preserving full power for edge cases.

**Design Philosophy:**
1. **Example-First, Always**: The primary interface is "show me an example file"
2. **YAML is the Escape Hatch**: Advanced users can drop to YAML at any time
3. **Semantic Layer is Invisible**: The system uses semantic primitives internally but users don't need to know
4. **Single Concept**: "Extraction Rule" - no separate "tagging rules" or "semantic paths"

### Tier 1: Simple API (Example-First)

```bash
# Simplest possible invocation - point at a file
$ casparian extract /mnt/data/mission_042/2024-01-15/telemetry.csv

Analyzing path structure...

  /mnt/data/mission_042/2024-01-15/telemetry.csv
            ───────────  ──────────
            mission: 042  date: 2024-01-15

Would extract:
  • mission_id: "042" (from folder like mission_*)
  • date: "2024-01-15" (ISO date folder)

Files matching: 1,247 in /mnt/data

Create rule? [Y/n] y
```

**CLI Commands:**
- `casparian extract /path/file.csv` - Point at file
- `casparian extract --template healthcare /path/` - Apply template
- `casparian extract --yes --tag my_data /path/file.csv` - Non-interactive

### Tier 2: Advanced API (Full YAML)

```yaml
version: 1
rules:
  - name: "Mission Telemetry"
    glob: "**/mission_*/????-??-??/*.csv"
    extract:
      mission_id:
        from: segment(-3)
        pattern: "mission_(\\d+)"
        type: integer
      date:
        from: segment(-2)
        type: date
    tag: mission_data
    priority: 100
```

### What Gets Cut
- User-facing semantic expressions (internal only)
- Equivalence classes (deferred to v2)
- AI features (deferred to v2)

### Trade-offs
**Pros:** 90% users never see YAML, single concept, clear escape hatch
**Cons:** Power users may miss semantic expressions, AI deferred

### New Gaps Introduced
- GAP-INFER-001: Inference may fail on unusual patterns
- GAP-TEMPLATE-001: Templates need real-world validation

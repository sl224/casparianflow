# Extraction API Refinement - Bootstrap Prompt

**Session Goal:** Refine the Path Metadata Extraction and Semantic Extraction specs into a sharp, user-focused API that delivers real value for business verticals without unnecessary complexity.

---

## 1. What We're Refining

Two interconnected specifications:

| Spec | Purpose | Status |
|------|---------|--------|
| `specs/extraction_rules.md` | Glob-based metadata extraction from file paths | v1.1, Ready |
| `specs/semantic_path_mapping.md` | Meaning layer above globs (primitives, recognition) | v1.2, Draft |

These specs define how users extract structured metadata (mission IDs, dates, environment markers) from file path patterns, using either manual rules or AI assistance.

---

## 2. Target Users (Business Verticals)

| Vertical | Pain Point | Example |
|----------|-----------|---------|
| **Defense** | Terabytes of mission data in dated folders | `/mission_042/2024-01-15/*.csv` |
| **Healthcare** | HL7/FHIR message archives with direction markers | `/ADT_Inbound/2024/01/15/*.hl7` |
| **Finance** | FIX logs and trade data by quarter | `/FIX_logs/2024/Q1/*.log` |
| **Legal** | eDiscovery matters by custodian | `/matter_12345/custodian_smith/emails/` |
| **Manufacturing** | Historian data by production line | `/Line_A/2024-01/sensor_data/` |

**Common Thread:** Structured folder conventions that encode metadata implicitly. Users want that metadata extracted explicitly and queryable.

---

## 3. Core Tension to Resolve

The specs currently have competing complexity:

```
SIMPLE ◄─────────────────────────────────────────────────────► POWERFUL
  │                                                                 │
  │  Manual glob + regex                Semantic primitives         │
  │  "Just tell me what to match"       "Recognize meaning"         │
  │                                                                 │
  │  extraction_rules.md                semantic_path_mapping.md    │
  │  (1343 lines)                       (1681 lines)                │
```

**Question:** Can we deliver 80% of the value with 20% of the complexity?

---

## 4. Refinement Constraints

### 4.1 MUST Have
- **Zero-friction onboarding** - User points at folder, gets useful extraction in <2 minutes
- **Works offline** - No AI required for core functionality (Layer 1)
- **Queryable results** - Extracted metadata in SQLite, immediately useful
- **Vertical templates** - Pre-built patterns for defense, healthcare, finance

### 4.2 SHOULD Have
- **AI enhancement** - Optional AI can suggest rules (Layer 2)
- **Semantic recognition** - System can recognize common patterns algorithmically
- **Cross-source learning** - Similar structures get similar treatment

### 4.3 MUST NOT Have
- **Complexity for complexity's sake** - No features without clear user value
- **AI dependency** - System must work fully air-gapped
- **Configuration overload** - Sane defaults, minimal required config
- **Orphan anxiety** - Unmatched files are normal, not errors

---

## 5. Key Questions to Answer

### 5.1 API Surface
1. What's the minimal CLI to create a useful extraction rule?
2. How many YAML fields are truly required vs optional?
3. Can we have a single-line rule syntax for simple cases?

### 5.2 Semantic Layer
1. Is the semantic layer necessary, or is it premature abstraction?
2. Can we deliver semantic recognition as algorithmic inference without the full vocabulary system?
3. What's the simplest path from "here are sample files" → "here's a rule"?

### 5.3 AI Integration
1. What exactly should AI do vs. what should be algorithmic?
2. How do we prevent AI from being a crutch that makes the system feel broken without it?
3. What's the "AI adds value" vs "AI is required" boundary?

### 5.4 User Experience
1. What does the TUI flow look like from "add source" to "query extracted data"?
2. How do we show users what the system extracted and why?
3. What happens when extraction fails or partially succeeds?

---

## 6. Simplification Proposals (For Discussion)

### Proposal A: Flatten the Semantic Layer
Instead of a full vocabulary system, embed common patterns directly:

```yaml
# Instead of: "entity_folder(mission) > dated_hierarchy(iso) > files"
# Just write:
rules:
  - name: mission_data
    pattern: "**/mission_*/*-*-*/*.csv"
    extract:
      mission_id: segment(-3, "mission_(.*)")
      date: segment(-2, type=date)
    tag: mission
```

The "semantic" is implicit in the extraction fields.

### Proposal B: Example-First Authoring
Start from examples, not patterns:

```bash
$ casparian rules from-example /data/mission_042/2024-01-15/telemetry.csv
Detected:
  mission_id = "042" (from folder "mission_042")
  date = "2024-01-15" (from folder)

Create rule? [Y/n]
```

The pattern is inferred; user just confirms.

### Proposal C: Progressive Disclosure
Three tiers of complexity:

| Tier | User | Interface | Complexity |
|------|------|-----------|------------|
| 1. Auto | Most users | Point at folder, accept suggestions | Zero config |
| 2. Templates | Power users | Pick template, customize parameters | Light config |
| 3. Full | Experts | Full YAML with all options | Full config |

Most users never leave Tier 1.

---

## 7. Existing Gaps to Address

From the current specs, these gaps need resolution:

| Gap | Spec | Impact |
|-----|------|--------|
| Coverage report UX | extraction_rules.md | How do users see what's matched vs missed? |
| Near-miss surfacing | extraction_rules.md | Detection exists, but how is it shown? |
| Partial recognition repair | semantic_path_mapping.md | UI flow for fixing partial matches |
| Link desync handling | semantic_path_mapping.md | What happens when rules drift from semantics? |
| Rule versioning | extraction_rules.md | How do users evolve rules over time? |
| Multi-source patterns | Both | Can one rule apply to multiple sources? |

---

## 8. Success Criteria

The refined spec is successful if:

1. **New user can extract metadata in <5 minutes** - From install to first query
2. **No required reading** - TUI guides user through workflow
3. **Templates cover 80% of verticals** - Defense, healthcare, finance, legal
4. **Spec is <500 lines** - Combined, for the core extraction API
5. **AI is additive, not essential** - All features work without API key

---

## 9. Refinement Process

Use the `spec_refinement_workflow_v2.md` process:

1. **Mediator** identifies gaps in current specs
2. **Engineer** proposes simplifications
3. **Reviewer** validates against user needs
4. **User** approves changes

**Focus:** One gap per round, foundations first, single-gap focus.

---

## 10. Starting Point

Begin with these questions:

1. **Read both specs** - `extraction_rules.md` and `semantic_path_mapping.md`
2. **Identify redundancy** - What's duplicated or overcomplicated?
3. **Map to user journey** - What does the user actually do step by step?
4. **Propose the minimal API** - What's the smallest useful surface?

---

## Appendix: Current Spec Statistics

| Spec | Lines | Sections | Tables | Code Blocks |
|------|-------|----------|--------|-------------|
| extraction_rules.md | 1343 | 15 | 25 | 30 |
| semantic_path_mapping.md | 1681 | 16 | 18 | 45 |
| **Combined** | **3024** | **31** | **43** | **75** |

**Target:** Reduce to <1000 lines combined while preserving all user value.

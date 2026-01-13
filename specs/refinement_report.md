# Spec Refinement: Extraction Robustness & Semantic Vocabulary

**Date:** 2026-01-12
**Status:** Proposal
**Target Specs:** `extraction_rules.md`, `semantic_path_mapping.md`

---

## 1. Executive Summary

This report identifies a critical robustness gap in the current `extraction_rules.md` specification regarding variable-depth folder structures. It proposes a mechanism to allow "Iron Core" (YAML-based) extraction to handle these cases without falling back to Python. Additionally, it suggests expanding the Semantic Vocabulary to handle enumerated folder types.

### 1.1 The Problem: Variable Depth Brittleness

The current specification relies heavily on `segment(N)` (from start) or `segment(-N)` (from end) for extraction. This is brittle when combined with deep wildcards (`**`).

**Scenario:**
- Rule: `**/mission_*/????-??-??/**/*.csv`
- Path A: `/data/mission_001/2024-01-01/logs/data.csv` (Depth: 5)
- Path B: `/data/mission_001/2024-01-01/data.csv` (Depth: 4)

**Failure:**
- `segment(-3)` correctly extracts `mission_001` for Path B.
- `segment(-3)` incorrectly extracts `2024-01-01` (or `logs`) for Path A.

**Current Consequence:**
- The system must fall back to Python extractors (Pathfinder Wizard) for these common patterns, breaking the "Iron Core" philosophy of declarative, safe, fast metadata extraction.

---

## 2. Proposal: Regex-Based Extraction

To fix this, `extraction_rules` must support extracting values via Regex capture groups on the full (or relative) path, not just fixed segment indices.

### 2.1 Schema Update (`extraction_rules.md`)

Add `regex` as a `source_type` alongside `segment`.

```yaml
extract:
  mission_id:
    # OLD way (Brittle)
    # from: segment(-3)
    
    # NEW way (Robust)
    from: full_path  # or rel_path
    pattern: "/mission_([^/]+)/"
    capture: 1
```

### 2.2 Compilation Logic

When a user writes a glob like `**/mission_*/**`, the system (or Wizard) should auto-generate the corresponding regex for robust extraction: `.*/mission_([^/]+)/.*`.

---

## 3. Semantic Vocabulary Expansion

The current `semantic_path_mapping.md` covers specific entities (ID-based) and dates. It lacks a primitive for "Category" or "Enum" folders—folders that classify data but aren't unique entities.

### 3.1 New Primitive: `category_folder` (or `enum_folder`)

**Definition:** A folder segment that matches one of a fixed set of values.

**Usage:**
```yaml
semantic: "entity_folder(mission) > category_folder(report_type) > files"
```

**Parameters:**
- `values`: List of allowed strings (e.g., `['logs', 'metrics', 'imagery']`)
- `name`: Field name to extract (e.g., `report_type`)

**Why needed:**
- `literal_segment` only matches ONE value.
- `wildcard_segment` extracts NOTHING.
- `category_folder` matches a SET and EXTRACTS the value.

**Example:**
- Path: `/data/mission_01/logs/file.log` → `{ report_type: "logs" }`
- Path: `/data/mission_01/metrics/file.csv` → `{ report_type: "metrics" }`

---

## 4. Semantic-to-Rule Compilation Strategy

The Semantic Layer (`semantic_path_mapping.md`) must be the "smart compiler" that protects users from brittle rules.

### 4.1 "Anchoring" Logic

When compiling a semantic expression to an extraction rule:

1.  **If the expression is linear/fixed-depth** (e.g., `entity > date > files`):
    -   Compile to `segment(-N)` for performance.

2.  **If the expression contains deep wildcards** (e.g., `entity > ** > files`):
    -   Compile to `from: full_path` with regex extraction.
    -   Regex: `.*/{entity_pattern}/.*/.*`

This ensures that "High Level" semantic definitions always compile down to "Safe" low-level rules.

---

## 5. UX Enhancements: Drift & Equivalence

### 5.1 Drift vs. Evolution

When the Semantic Recognizer sees a "Drift" (e.g., a folder structure changes slightly), it should offer to:
1.  **Evolve**: Update the Semantic Definition (and re-compile the Rule).
2.  **Fork**: Create a new Variation of the Rule.

### 5.2 Explicit Equivalence Acceptance

"Semantic Equivalence" is powerful but dangerous if wrong.
- **Proposal**: TUI must show a "Diff" of the *Logical View* before applying an equivalent rule.
- "Applying this rule will organize your 50,000 files into 12 Missions. Correct?"

---

## 6. Action Plan

1.  **Update `extraction_rules.md`**: Add `from: full_path` and `pattern` support for extraction.
2.  **Update `semantic_path_mapping.md`**:
    -   Add `category_folder` primitive.
    -   Update "Code Generator" section to use regex for variable-depth paths.
3.  **Update `ai_wizards.md`**:
    -   Update Pathfinder logic to prefer generating Regex-based YAML rules over Python code for variable-depth patterns.

---

## 7. Re-Evaluation Findings (2026-01-12)

### 7.1 Redundancy: Tagging Rules vs. Extraction Rules

**Issue:** `discover.md` defines `scout_tagging_rules` (pattern → tag) while `extraction_rules.md` defines `extraction_rules` (glob → extract + tag).
**Finding:** These are redundant systems. A "Tagging Rule" is semantically identical to an "Extraction Rule" that has no extraction fields.
**Proposal:** Consolidate into `extraction_rules`.
-   Migrate `scout_tagging_rules` table to `extraction_rules` (where `extract` is empty/null).
-   Update Discover TUI to create `ExtractionRule` objects instead of `TaggingRule` objects.

### 7.2 Ambiguity: AI Labeling Persistence

**Issue:** `ai_wizards.md` states the Labeling Wizard creates a "Tag rule" based on content analysis (headers/samples). However, `extraction_rules` operate on *file paths* (globs).
**Finding:** It is unclear how a content-derived label (e.g., "Sales Data") persists to future files if the wizard doesn't also identify a path pattern.
**Proposal:** The Labeling Wizard must either:
1.  **Reverse-engineer a path pattern** (glob) that matches the labeled file group, creating a proper Extraction Rule.
2.  **Tag the Signature Group** (from `roadmap/spec_discovery_intelligence.md`), allowing all future files with that structural signature to inherit the tag automatically. (Preferred for robustness).

### 7.3 Cross-Platform: Regex Path Separators

**Issue:** The proposed `full_path` regex extraction in `extraction_rules` may break on Windows (backslashes) if users write Unix-style regexes (forward slashes).
**Finding:** Raw regex on OS paths is brittle across platforms.
**Proposal:** The Extraction Engine must **normalize paths to Unix-style forward slashes** (`/`) before applying any `full_path` regex or glob matching. This ensures regexes like `/mission_([^/]+)/` work on all platforms.

### 7.4 UX Disconnect: Discover Mode Rule Creation

**Issue:** `discover.md` Section 2.2 describes a simple "Pattern + Tag" dialog. This hides the power of field extraction.
**Finding:** Users creating rules in Discover mode might want to extract metadata, not just tag files.
**Proposal:**
-   Simple "Tag" flow (current): Creates Extraction Rule with no fields.
-   Advanced "Extract" flow: Opens the Semantic Path Wizard or Pathfinder Wizard to define extraction logic.

---

## 8. Extraction Rules Analysis (2026-01-12)

Analysis of `specs/extraction_rules.md`:

### 8.1 Gap: Extraction Status Transitions & Lifecycle

**Issue:** Section 9.3 defines `extraction_status` (COMPLETE, PARTIAL, FAILED) but does not define the transitions or lifecycle.
**Finding:** It is unclear when `extraction_status` is updated. Is it only on initial scan? What happens if a rule is updated? Does it trigger a full re-scan or a targeted update?
**Consequence:** Potential for stale metadata or inconsistent states.
**Proposal:** Define explicit triggers for status updates (e.g., "Rule Created/Updated" -> "Invalidate matching files" -> "Re-extract").

### 8.2 Gap: Multi-Value Extraction

**Issue:** The spec assumes one value per field.
**Finding:** Some patterns might yield multiple values (e.g., repeated segments or regex global matches).
**Consequence:** Loss of data for complex path structures.
**Proposal:** Explicitly state that extraction yields a single scalar value per field, or define syntax for array extraction (e.g., `capture: all`).

### 8.3 Gap: "Dry Run" / Simulation API

**Issue:** Section 8.1 lists `casparian rules test` but it's single-file.
**Finding:** Users editing high-impact rules (priority 100) need to know the *blast radius* before applying.
**Consequence:** Risk of accidental mass-retagging or metadata corruption.
**Proposal:** Add a `simulate` flag or API to `casparian rules update` that returns "files_affected", "metadata_diff_sample".

### 8.4 Issue: Priority Collision Handling

**Issue:** Section 5.1 says "First match wins".
**Finding:** If two rules have `priority: 100`, behavior is undefined (likely DB insertion order, which is unstable).
**Consequence:** Non-deterministic extraction if user isn't careful with priorities.
**Proposal:** Enforce strict deterministic ordering: `ORDER BY priority ASC, name ASC`.

### 8.5 Higher Order Consequence: Indexing Cardinality

**Issue:** Section 6.4 suggests Functional Indexes on JSON fields.
**Finding:** If users extract high-cardinality fields (e.g., `uuid` or `timestamp` with ms precision) as metadata, functional indexes will bloat significantly.
**Consequence:** Database performance degradation.
**Proposal:** Add warnings or limits on indexed field cardinality, or use partial indexes.
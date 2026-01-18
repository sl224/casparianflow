# Spec: Variant Grouping for Structural Suggestions (Revised)

## Context
Users build rules to match and parse files across large folder trees. The UI detects common folder structures and filename patterns. Real-world data often has historical naming variants (e.g., `mission_*` vs `msn_*`) that should be treated as the same data type. Today, the UI shows them separately, making it hard to understand that they are variants of the same intent.

## Goal
Detect same-type structural variants and group them so a user can apply multiple patterns into one rule in a single action. Keep the heuristic conservative to minimize false positives.

---

## Requirements

### Functional
1. Detect variant groups from scanned paths (no ML, local only).
2. Each group includes:
   - Group label (defined algorithm below)
   - Variant list (template + file count + sample)
3. UI shows grouped variants in a Variants view/tab.
4. Users can select/deselect variants in a group before applying.
5. Apply action adds multiple globs (one per variant).
6. Show a "why" explanation: highlight differing literal tokens.

### Non-functional
- Deterministic, local, O(n log n).
- Avoid false positives; conservative grouping.
- No schema changes for persistence (store only resulting globs).

---

## Detection Heuristic

### Input
Each archetype has:
- `template` (e.g., `mission_<id>/<date>/...`)
- `file_count`
- `sample_paths` (1–3 full paths)
- `depth`
- `filename_template` (normalized)
- `extension` (derived, optional)

### Step 1: Normalize tokens
For each segment, tokenize by `_ - .` and map to:
- Variable tokens: `<id>`, `<date>`, `<n>`, `<uuid>`, `<version>`, etc.
- Literal tokens: keep as lowercase (e.g., `mission`, `msn`).

Output:
- `literal_tokens` set (all literal tokens in path)
- `literal_tokens_by_segment` (ordered lists)
- `fingerprint` = sequence of variable/literal classes + filename template class

### Step 2: Candidate pairing (hard gates)
- Same depth (V1)
- Same filename template class
- Same count of literal segments

### Step 3: Similarity score (penalty model + Jaccard)
Start score = **1.0**.

Penalties:
- Literal mismatch (different word, no fuzzy match): **–0.6**
- Literal mismatch (edit distance ≤ 2 + same first letter): **–0.1**
- Extension mismatch: **–0.2**
- Variable token shift (index mismatch): **–0.2**

Bonuses:
- Filename template exact match: **+0.2**
- Variable tokens in same positions: **+0.2**
- File count ratio within 0.5x–2x: **+0.1**

Jaccard bonus/penalty:
Let `J = |A ∩ B| / |A ∪ B|` where `A` and `B` are literal token sets.

- If `J ≥ 0.6` → **+0.2**
- If `0.4 ≤ J < 0.6` → **+0.1**
- If `J < 0.2` → **–0.2**

Group if score ≥ **0.7**.

Why Jaccard helps:
- Captures overlap even if token order differs.
- Adds a robust similarity signal beyond per-segment alignment.
- Penalizes weak overlaps to prevent false positives.

### Step 4: Fuzzy literal matching (no global lexicon)
To reduce domain bias:
- No global synonym dictionary unless user provides it.
- Use fuzzy match only if:
  - same first letter
  - edit distance ≤ 2
  - token length ≤ 6

### Step 5: Clustering rule
If A~B and B~C but A~C is weak:
- Use transitive closure to group, but drop any member whose average score to group < 0.7.
- If ambiguous, prefer smaller groups over large uncertain ones.

---

## Group Label Strategy

1. Pick the variant with highest `file_count`.
2. Extract its primary literal token (first non-generic literal in the path).
3. Title-case + "Data" suffix if token is abstract.
4. If no literal token, fallback to template: "Variant Group".

---

## UI Behavior (TUI)

### Variants View (Right Panel Tab)
```
Variants
 ▸ Mission Data (2 variants)
    [x] mission_<id>/...  (412 files)
    [x] msn_<id>/...      (388 files)

   Sensor Logs (3 variants)
    [x] sensor_<id>/...   (221 files)
    [ ] sns_<id>/...      (198 files)
    [x] s_<id>/...        (144 files)

[Enter] apply checked  [Space] preview  [x] toggle  [?] why  [Esc] clear preview
```

### Selection
- `x` toggles a variant within a group.
- `Enter` applies only checked variants.

### Preview / Why
- `Space` highlights matching folders/files in the Folders/Files tabs.
- `?` shows "why grouped" with literal-token highlighting.

---

## Edge Cases & Limitations

- Different depth (e.g., `mission/2023/data` vs `mission/data`):
  - V1 will not group; explicitly noted limitation.
- Ambiguous grouping:
  - Favor smaller, higher-confidence groups.
- Format changes:
  - `.csv` → `.xlsx` can still group via scoring (not hard gated).

---

## Persistence

- Do not persist groups.
- Persist only resulting glob patterns in the rule.
- Groups are derived on each scan.


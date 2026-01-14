# Engineer Response: Unclustered Threshold Definition

**Gap:** GAP-PIE-002 - Clustering "unclustered" threshold undefined
**Priority:** MEDIUM
**Status:** Complete Specification

---

## Problem Statement

Section 3.5.2 (Path Intelligence Engine - Path Clustering) in `specs/ai_wizards.md` mentions unclustered files but does not define:

1. **When files are considered "unclustered"** - What similarity threshold triggers exclusion from clusters?
2. **Clustering algorithm parameters** - HDBSCAN hyperparameters and their justification
3. **Edge case handling** - What happens with single files, all-unique paths, very small file sets?
4. **UI presentation logic** - How are unclustered files displayed and what options should users have?

This gap creates ambiguity for implementation and testing.

---

## Proposed Section: 3.5.2.1 Unclustered Threshold Definition

### Overview

The clustering algorithm groups semantically similar paths into clusters. Files that do not achieve minimum similarity thresholds are placed in the **Unclustered** group for separate handling (manual review, re-clustering with hints, or exclusion).

### Algorithm Parameters (HDBSCAN Configuration)

The Path Intelligence Engine uses HDBSCAN (Hierarchical Density-Based Spatial Clustering) with these parameters:

```python
clusterer = hdbscan.HDBSCAN(
    min_cluster_size=5,              # Minimum paths per cluster
    cluster_selection_epsilon=0.1,   # Stability threshold
    metric='cosine',                 # Path similarity metric
    allow_single_cluster=False,      # Force unclustered detection
)
```

**Parameter Justification:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `min_cluster_size` | 5 | Minimum viable cluster size; <5 files should be reviewed individually |
| `cluster_selection_epsilon` | 0.1 | Controls cluster stability; avoids spurious micro-clusters |
| `metric` | cosine | Normalized path embeddings; handles varying path lengths well |
| `allow_single_cluster` | False | Force HDBSCAN to identify unclustered points instead of assigning all to one default cluster |

### Unclustered Classification Logic

A file is considered **unclustered** if it meets ANY of these conditions:

#### Condition 1: HDBSCAN Labels as Noise
When HDBSCAN runs with `allow_single_cluster=False`, it returns label `-1` for points that don't belong to any dense cluster. These are automatically unclustered.

#### Condition 2: Cluster Size Below Threshold
If HDBSCAN identifies a cluster with fewer than 5 files, those files are **demoted to unclustered** to ensure clusters are stable enough for pattern extraction.

```python
# After HDBSCAN clustering
for cluster_id in unique_labels:
    if cluster_size[cluster_id] < 5:
        # Move all files to unclustered group
        labels[labels == cluster_id] = -1
```

**Rationale:** Extraction rules should cover at least 5+ files to be statistically meaningful. Smaller groups are edge cases or outliers better handled individually.

#### Condition 3: Cluster Confidence Below 70%
Each cluster receives a confidence score based on:

```
confidence = (mean_intra_cluster_distance) / (mean_inter_cluster_distance)
```

If `confidence < 0.70`, the cluster is considered unreliable and files revert to unclustered.

**Interpretation:**
- `confidence = 1.0`: Perfect cluster (all files similar to each other, different from others)
- `confidence = 0.70`: Acceptable cluster (files reasonably cohesive)
- `confidence < 0.70`: Questionable cluster (recommend individual review or re-clustering)

### Edge Cases

#### Edge Case 1: Total Files < 10

Clustering requires a minimum viable dataset. If the input has fewer than 10 files:

```
Input: 4 files
Output: All 4 files → Unclustered
Action: Show prompt to scan more directories or use single-file proposals
UI Message: "Cannot cluster: Need at least 10 files. Current: 4 files. [s] Scan more  [Esc] Cancel"
```

**Rationale:** With <10 files, clustering is unreliable and algorithmic inference from `extraction.md` is more effective.

#### Edge Case 2: All Unique Paths (No Clusters)

If all embeddings are dissimilar (e.g., completely random file structures), HDBSCAN returns all labels as -1:

```
Input: 250 random files with completely different structures
Output:
  - Clusters: []
  - Unclustered: 250 files
  - Confidence: N/A (no clusters formed)
Action: Show unclustered UI with options to provide hints, scan manually, or create single-file rules
```

**Rationale:** This is a valid outcome indicating highly heterogeneous file structures. Show UI but don't fail.

#### Edge Case 3: Single Large Cluster (>95% of Files)

If HDBSCAN identifies one dominant cluster containing >95% of files and small unclustered remainder:

```
Input: 500 files
Output:
  - Cluster A: 475 files (95%) → confidence 0.88
  - Unclustered: 25 files (5%) → may be legitimate outliers or variations
Action: Accept cluster as primary pattern, offer unclustered handling options
```

**Rationale:** This is common in homogeneous sources. Proceeding with the primary pattern is correct; unclustered files are genuinely different and should be handled separately.

#### Edge Case 4: All Files Below Confidence Threshold

If all identified clusters have `confidence < 0.70`:

```
Input: 200 files with ambiguous structure
Output:
  - Clusters identified: 3, but all have confidence < 0.70
  - Recommendation: All files → Unclustered; offer re-clustering with user hints
Action: Show unclustered UI with emphasis on "Provide hints" option
```

**Rationale:** User provides domain context ("ignore extensions", "focus on folder depth") to improve clustering.

---

## Clustering Parameters Configuration

### Default Configuration

Store clustering parameters in configuration file:

```toml
# ~/.casparian_flow/config.toml

[ai.path_intelligence.clustering]
enabled = true

# HDBSCAN parameters
min_cluster_size = 5
cluster_selection_epsilon = 0.1

# Quality thresholds
min_confidence_score = 0.70      # Below this → unclustered
min_files_per_cluster = 5        # Below this → demoted to unclustered
min_input_files = 10             # Below this → skip clustering, use algorithmic inference

# Similarity metrics
metric = "cosine"
allow_single_cluster = false

# Embedding model (Section 3.5.8)
embedding_model = "all-MiniLM-L6-v2"
```

### Runtime Override

Users can adjust clustering behavior via CLI flags (Phase 2):

```bash
casparian tui
# Then press 'C' to cluster with:

# Option 1: More aggressive clustering (accept lower confidence)
casparian cluster --min-confidence 0.60 --min-cluster-size 3

# Option 2: More conservative clustering (higher bar)
casparian cluster --min-confidence 0.85 --min-cluster-size 8

# Option 3: Retry with hints
casparian cluster --hint "Group files by folder depth, ignore extensions"
```

---

## UI Presentation for Unclustered Files

### When Unclustered Group Appears

The `[~] Unclustered` group appears in Cluster Review overlay (Section 3.5.12.2) when:

1. `num_unclustered_files > 0` AND
2. Total clustering produced at least 1 viable cluster (confidence ≥ 0.70)

### Display Format

**In Overview:**

```
|  >> [A] Sales Reports (247 files) - 94% similarity                 |
|        Confidence: ████████░░ 82%                                  |
|                                                                    |
|     [B] Client Reports (89 files) - 91% similarity                 |
|        Confidence: █████████░ 91%                                  |
|                                                                    |
|     [~] Unclustered (152 files)                                    |
|        Low similarity - review individually or provide hints        |
```

**Visual Conventions:**
- `[~]` label for unclustered group (not A, B, C, etc.)
- No confidence bar (N/A for heterogeneous group)
- Descriptive subtitle explaining why files are unclustered
- File count displayed prominently

### Unclustered Handling Options

When user navigates to `[~] Unclustered` and presses `Enter`, show detailed view:

```
+=========================[ UNCLUSTERED FILES (152) ]=========================+
|                                                                               |
|  These files have low structural similarity and didn't form clusters.        |
|                                                                               |
|  What would you like to do?                                                  |
|                                                                               |
|  Options:                                                                    |
|    [m] Manual review - Browse and tag individually                           |
|    [h] Provide hints - Help AI understand patterns                           |
|    [r] Re-cluster - Try with different parameters                            |
|    [i] Ignore - Skip these files for now                                     |
|    [s] Single-file rules - Create rule for each file                         |
|                                                                               |
+===============================================================================+
```

**Option Definitions:**

| Option | Behavior | When to Use |
|--------|----------|-------------|
| **[m] Manual review** | Show file list; tag individually or create per-file rules | Small unclustered groups (<50 files) |
| **[h] Provide hints** | Accept text hints (e.g., "dates in second folder"); re-cluster | User understands patterns but clustering missed them |
| **[r] Re-cluster** | Run HDBSCAN with relaxed thresholds (min_confidence 0.60, min_size 3) | Try more aggressive clustering before giving up |
| **[i] Ignore** | Leave files untagged; return to overview | Files are truly heterogeneous or not immediately relevant |
| **[s] Single-file rules** | Use Section 3.5.5 (Single-File Proposals) on each unclustered file | Bootstrap extraction rules without clustering patterns |

### Hint-Based Re-Clustering

When user selects `[h] Provide hints`, show prompt:

```
+-- CLUSTERING HINTS -------------------------------------------------+
|                                                                      |
|  Help the AI understand your file organization:                      |
|                                                                      |
|  Hint: [                                                    ]        |
|                                                                      |
|  Examples you can try:                                               |
|    "Files with dates should be grouped together"                     |
|    "Ignore file extensions, focus on folder structure"               |
|    "The second folder is always the project name"                    |
|    "Group by file size patterns"                                     |
|                                                                      |
|  [Enter] Re-cluster with hint  [Esc] Cancel                          |
+----------------------------------------------------------------------+
```

**Hint Processing:**

1. User provides hint text
2. System appends hint to embedding context (e.g., augment normalized paths with hint keywords)
3. Re-run HDBSCAN with relaxed thresholds:
   - `min_confidence_score = 0.60` (instead of 0.70)
   - `min_cluster_size = 3` (instead of 5)
4. Display new clustering results

Example:

```
Original clustering:
  Clusters: 2 (152 unclustered)

User hint: "Files with dates should be grouped together"

Re-clustering with hint:
  Clusters: 4 (38 unclustered)  ← Improved!

Show: "Re-clustering found 4 new clusters. Accept these? [a] [Esc]"
```

### Batch Accept with Unclustered Notice

When user presses `A` (accept all clusters), show confirmation that unclustered files **remain untagged**:

```
+-----------------------------------------------------+
|  Accept all clusters?                              |
|                                                    |
|  [A] Sales Reports        247 files  → sales_data  |
|  [B] Client Reports        89 files  → client_data |
|  [C] Backup Files          12 files  → backups     |
|                                                    |
|  [!] Unclustered (152 files will remain untagged)  |
|                                                    |
|  This will create 3 extraction rules.              |
|                                                    |
|  [Enter] Confirm  [Esc] Cancel                     |
+-----------------------------------------------------+
```

---

## Confidence Score Calculation

### Detailed Algorithm

For each identified cluster, compute:

```
confidence = (cohesion) / (separation)

where:
  cohesion = 1.0 - mean(distance(point, cluster_center))
           for all points in cluster

  separation = mean(distance(cluster_center, nearest_other_cluster))
             / mean(distance(point, cluster_center))
           for all points in cluster

Final confidence = min(1.0, cohesion * separation)
```

**Simplified Interpretation:**

```python
def compute_cluster_confidence(embeddings, cluster_id, clusterer):
    """
    Returns confidence score [0.0, 1.0] for a cluster.
    - 1.0: Perfect cluster (cohesive + isolated)
    - 0.70: Acceptable (clearly distinct from others)
    - <0.70: Questionable (borderline, ambiguous)
    """
    cluster_points = embeddings[clusterer.labels_ == cluster_id]

    # Intra-cluster distances
    centroid = cluster_points.mean(axis=0)
    intra_distances = [cosine_distance(p, centroid) for p in cluster_points]
    cohesion = 1.0 - np.mean(intra_distances)

    # Inter-cluster distances (to nearest other cluster)
    other_clusters = set(clusterer.labels_) - {cluster_id, -1}
    if not other_clusters:
        # Only cluster, or only unclustered + this; assume high confidence
        return 0.85

    min_inter_distance = min(
        np.mean([cosine_distance(p, other_centroid)
                 for other_id in other_clusters
                 for p in embeddings[clusterer.labels_ == other_id]])
    )

    separation = min_inter_distance / (np.mean(intra_distances) + 1e-6)

    return min(1.0, cohesion * separation)
```

### Confidence Display

Show confidence as visual bar + percentage:

```
Confidence: ████████░░ 82%
            └─ 8/10 blocks filled + percentage
```

---

## Implementation Checklist

### Phase 1: Core Clustering (Existing)
- [x] HDBSCAN integration with `min_cluster_size=5`
- [x] Path embedding with `all-MiniLM-L6-v2`
- [x] Cluster identification

### Phase 2: Unclustered Detection (This Gap)
- [ ] Implement unclustered classification logic (Condition 1, 2, 3)
- [ ] Confidence score calculation
- [ ] Cluster size validation and demotion to unclustered
- [ ] Edge case handling: <10 files, all unique, etc.
- [ ] Configuration file update with new parameters

### Phase 3: UI Implementation
- [ ] Render `[~] Unclustered` group in overview
- [ ] Unclustered detail view with 5-option menu
- [ ] Hint-based re-clustering UI and logic
- [ ] Batch accept confirmation with unclustered notice
- [ ] Single-file rules integration (Section 3.5.5)

### Phase 4: Testing
- [ ] Unit tests: confidence calculation with synthetic clusters
- [ ] E2E test: 500-file cluster with edge cases
- [ ] E2E test: hint-based re-clustering flow
- [ ] E2E test: all-unique paths → all unclustered
- [ ] E2E test: <10 files → skip clustering

---

## Edge Case Testing Examples

### Test Case 1: Homogeneous + Few Outliers
```
Input: 500 files
  - 475 files: /data/sales/YYYY/MM/orders_NNN.csv
  - 25 files: /archive/old/misc_files/*

Expected Output:
  Cluster A: 475 files (94% similarity, confidence 0.91)
  Unclustered: 25 files

Behavior: Accept Cluster A, handle 25 outliers via UI options
```

### Test Case 2: Ambiguous Structure
```
Input: 200 files with variable naming patterns
  - Some: /projects/proj_001/data.csv
  - Some: /archive/proj-001/backup.csv
  - Some: /old_projects/001_data.csv

Expected Output:
  Cluster A: 140 files (confidence 0.65 → below threshold → demoted)
  All files: Unclustered

Behavior: Show unclustered UI; user provides hint "focus on project IDs"
```

### Test Case 3: Very Small Input
```
Input: 6 files

Expected Output:
  Error: "Cannot cluster: Need at least 10 files. Current: 6 files."
  Options: [s] Scan more directories  [u] Use single-file rules  [Esc] Cancel
```

---

## Summary

This specification resolves GAP-PIE-002 by defining:

1. **Unclustered Threshold:** Three conditions (HDBSCAN noise label, cluster size <5, confidence <0.70)
2. **Clustering Parameters:** HDBSCAN configuration with justification
3. **Edge Cases:** <10 files, all-unique, single large cluster, all-low-confidence
4. **UI Presentation:** Overview label, detail view with 5 options (manual, hints, re-cluster, ignore, single-file)
5. **Confidence Calculation:** Cohesion/separation formula with visual bar display
6. **Implementation Phases:** 4 phases from core detection to testing

The design follows the existing Cluster Review specification (Section 3.5.12) and integrates with single-file proposals (Section 3.5.5) for comprehensive handling of all clustering outcomes.

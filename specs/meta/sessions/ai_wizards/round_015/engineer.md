# Engineer Response: Path Intelligence Engine TUI Integration

**Gap:** GAP-INT-001 - Path Intelligence Engine has no TUI integration
**Priority:** HIGH
**Status:** Complete Specification

---

## Proposed Section: 3.5.12 TUI Integration

The following specification defines how users interact with the Path Intelligence Engine through the TUI. This integrates with Discover mode (see `specs/views/discover.md`) and enables cluster-based file organization workflows.

---

### 3.5.12 TUI Integration

The Path Intelligence Engine provides cluster-based file organization through a dedicated **Cluster Review** workflow accessible from Discover mode.

#### 3.5.12.1 Entry Points

Users can invoke path clustering from three locations in the TUI:

| Entry Point | Key | Condition | Behavior |
|-------------|-----|-----------|----------|
| **Files Panel** | `C` | Multiple files visible | Cluster all files in current view |
| **Sources Manager** | `c` | Source selected | Cluster all files in source |
| **AI Wizards Menu** | `W` then `1` | Any | Open cluster wizard with source selection |

**Entry Point Details:**

1. **From Files Panel (`C`)**: Clusters the currently filtered file set
   - If tag filter active: Clusters files with that tag
   - If text filter active: Clusters matching files
   - If no filter: Clusters all files in current source

2. **From Sources Manager (`c`)**: Clusters entire source
   - Useful for initial discovery on new sources
   - Shows "Analyzing N files..." progress

3. **From AI Wizards Menu (`W` then `1`)**: Full wizard flow
   - Prompts for source selection first
   - Allows scope refinement before clustering

**Minimum Files Requirement:**

Clustering requires at least 10 files. If fewer files are selected:
```
+-----------------------------------------------------+
|  Cannot cluster: Need at least 10 files             |
|                                                      |
|  Current selection: 4 files                          |
|                                                      |
|  [s] Scan more directories  [Esc] Cancel             |
+-----------------------------------------------------+
```

#### 3.5.12.2 Cluster Review Overlay

When clustering completes, an overlay displays results. The overlay uses full-screen width and 80% of height.

**Cluster Review Layout:**

```
+=========================[ PATH CLUSTERS ]=========================+
|                                                                    |
|  Source: sales_data        Analyzed: 500 files        Time: 1.2s   |
|                                                                    |
+-- CLUSTERS (4 found) -------------------------------------------- +
|                                                                    |
|  >> [A] Sales Reports (247 files) - 94% similarity                 |
|        /data/sales/2024/jan/orders_001.csv                         |
|        /data/sales/2024/feb/orders_002.csv                         |
|        /data/sales/2023/dec/orders_847.csv                         |
|        +2 more representative paths                                |
|        Proposed fields: { department, year, month, doc_type }      |
|        Confidence: ████████░░ 82%                                  |
|                                                                    |
|     [B] Client Reports (89 files) - 91% similarity                 |
|        /data/reports/client_acme/quarterly_Q1.xlsx                 |
|        /data/reports/client_globex/quarterly_Q2.xlsx               |
|        Proposed fields: { doc_type, client_name, quarter }         |
|        Confidence: █████████░ 91%                                  |
|                                                                    |
|     [C] Backup Files (12 files) - 87% similarity                   |
|        /data/misc/backup_2024-01-15.zip                            |
|        /data/misc/backup_2024-01-16.zip                            |
|        Proposed fields: { doc_type, date }                         |
|        Confidence: █████████░ 88%                                  |
|                                                                    |
|     [~] Unclustered (152 files)                                    |
|        Low similarity - review individually                        |
|                                                                    |
+--------------------------------------------------------------------+
|  [j/k] Navigate  [Enter] Expand  [a] Accept  [e] Edit  [r] Create  |
|  [n] Next wizard  [Tab] View files  [Esc] Close                    |
+====================================================================+
```

**Visual Elements:**

| Element | Meaning |
|---------|---------|
| `>>` | Currently selected cluster |
| `[A]`, `[B]`, `[C]` | Cluster labels (for quick-jump with letter keys) |
| `[~]` | Unclustered files (special group) |
| Confidence bar | Visual confidence: `████████░░ 82%` |
| `+N more` | Collapsed representative paths |

#### 3.5.12.3 State Machine

The Cluster Review workflow uses a 5-state machine:

```
+-----------------------------------------------------------------------------------+
|                        CLUSTER REVIEW STATE MACHINE                                |
+-----------------------------------------------------------------------------------+
|                                                                                    |
|   From Discover                                                                    |
|        |                                                                           |
|        v                                                                           |
|   +--------------+                                                                 |
|   |  CLUSTERING  |  <-- AI processing, shows progress                              |
|   |  (progress)  |                                                                 |
|   +------+-------+                                                                 |
|          |                                                                         |
|          | (auto on complete)                                                      |
|          v                                                                         |
|   +--------------+    Enter/Space    +--------------+                              |
|   |   OVERVIEW   | ----------------> |   EXPANDED   |                              |
|   | (cluster     |                   | (single      |                              |
|   |  list)       | <---------------- |  cluster     |                              |
|   |              |    Esc/h          |  detail)     |                              |
|   +------+-------+                   +------+-------+                              |
|          |                                  |                                      |
|          | a (accept)          e (edit)     | a (accept)                           |
|          |                          |       |                                      |
|          v                          v       v                                      |
|   +--------------+           +--------------+                                      |
|   |   ACCEPTED   |           |    EDITING   |                                      |
|   | (rule        |           | (modify      |                                      |
|   |  created)    |           |  fields)     |                                      |
|   +------+-------+           +------+-------+                                      |
|          |                          |                                              |
|          | n (next)                 | Enter (save) / Esc (cancel)                  |
|          v                          v                                              |
|   [Auto-select next cluster]  [Return to Overview/Expanded]                        |
|                                                                                    |
|          | Esc from OVERVIEW (all processed or cancelled)                          |
|          v                                                                         |
|   [Return to Discover]                                                             |
|                                                                                    |
+-----------------------------------------------------------------------------------+
```

**State Definitions:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| `Clustering` | User invokes clustering | Auto on complete | Show progress bar, file count, elapsed time |
| `Overview` | Auto from Clustering, `h`/`Esc` from Expanded | `Enter` to Expanded, `a` to Accepted, `Esc` to Discover | Navigate cluster list, preview selected |
| `Expanded` | `Enter`/`Space` from Overview | `h`/`Esc` to Overview, `a` to Accepted, `e` to Editing | Full cluster detail, file list, proposed rule |
| `Accepted` | `a` from Overview/Expanded | `n` to next cluster, `Esc` if none left | Rule creation in progress, then auto-advance |
| `Editing` | `e` from Overview/Expanded | `Enter` to save, `Esc` to cancel | Modify proposed fields, tag name |

**Transition Table:**

| From | Key | To | Condition | Notes |
|------|-----|----|-----------|-------|
| Clustering | (auto) | Overview | Complete | Show results |
| Clustering | Esc | Discover | Any | Cancel clustering |
| Overview | j/k | Overview | Any | Navigate clusters |
| Overview | A-Z | Overview | Cluster exists | Quick-jump to cluster |
| Overview | Enter/Space | Expanded | Cluster selected | Show detail |
| Overview | a | Accepted | Cluster selected | Accept proposal |
| Overview | e | Editing | Cluster selected | Edit proposal |
| Overview | r | Discover + Rule | Cluster selected | Create rule immediately |
| Overview | Tab | Overview + Files | Any | Toggle file list panel |
| Overview | Esc | Discover | Any | Close overlay |
| Expanded | h/Esc | Overview | Any | Return to list |
| Expanded | a | Accepted | Any | Accept this cluster |
| Expanded | e | Editing | Any | Edit this cluster |
| Expanded | j/k | Expanded | Files visible | Navigate file list |
| Expanded | n | Expanded | Next exists | Jump to next cluster |
| Expanded | p | Expanded | Prev exists | Jump to prev cluster |
| Accepted | n | Overview | Next exists | Select next cluster |
| Accepted | Esc | Overview | None left | All processed |
| Editing | Enter | Overview/Expanded | Valid | Save changes |
| Editing | Esc | Overview/Expanded | Any | Discard changes |
| Editing | Tab | Editing | Any | Cycle field focus |

#### 3.5.12.4 Expanded Cluster View

When a cluster is expanded, show full details with file list:

```
+=========================[ CLUSTER A: Sales Reports ]=========================+
|                                                                               |
|  247 files | 94% internal similarity | Confidence: 82%                        |
|                                                                               |
+-- REPRESENTATIVE PATHS (5) --------------------------------------------------+
|                                                                               |
|    /data/sales/2024/jan/orders_001.csv                                        |
|    /data/sales/2024/feb/orders_002.csv                                        |
|    /data/sales/2024/mar/orders_003.csv                                        |
|    /data/sales/2023/nov/orders_823.csv                                        |
|    /data/sales/2023/dec/orders_847.csv                                        |
|                                                                               |
+-- PROPOSED EXTRACTION -------------------------------------------------------+
|                                                                               |
|  Glob:   **/sales/{year}/{month}/*.csv                                        |
|  Tag:    sales_data                                                           |
|                                                                               |
|  Fields:                                                                      |
|    department    segment(-4)     type: string     "sales"                     |
|    year          segment(-3)     type: integer    2023, 2024                  |
|    month         segment(-2)     type: string     "jan", "feb", ...           |
|    doc_type      filename        type: string     "orders"                    |
|                                                                               |
+-- FILES IN CLUSTER (247) ---------------------+-- PREVIEW -------------------+
|                                                |                              |
|  >> orders_001.csv        2024/jan     2.1KB  |  id,date,amount,product      |
|     orders_002.csv        2024/jan     1.8KB  |  1001,2024-01-15,100.50,A    |
|     orders_003.csv        2024/jan     2.3KB  |  1002,2024-01-16,250.00,B    |
|     orders_004.csv        2024/feb     1.9KB  |  1003,2024-01-17,75.25,A     |
|     orders_005.csv        2024/feb     2.0KB  |  ...                         |
|     orders_823.csv        2023/nov     1.7KB  |                              |
|     orders_847.csv        2023/dec     2.2KB  |                              |
|                                                |                              |
|     [Showing 1-7 of 247]                       |                              |
|                                                |                              |
+------------------------------------------------+------------------------------+
|  [a] Accept  [e] Edit  [h/Esc] Back  [j/k] Navigate  [n/p] Next/Prev cluster  |
+===============================================================================+
```

**File List Features:**

- Scrollable with `j`/`k`
- Shows relative path, parent folder context, and file size
- Preview panel updates on file selection
- `[Showing X-Y of N]` pagination indicator

#### 3.5.12.5 Editing Mode

When editing a cluster proposal, fields become editable:

```
+=========================[ EDIT CLUSTER: Sales Reports ]=======================+
|                                                                               |
+== GLOB PATTERN (1/3) ========================================================+
|>> **/sales/{year}/{month}/*.csv                                               |
|   [Live: 247 matches]                                                         |
+==============================================================================+
|                                                                               |
+-- TAG NAME (2/3) ------------------------------------------------------------+
|   sales_data                                                                  |
+------------------------------------------------------------------------------+
|                                                                               |
+-- FIELDS (3/3) --------------------------------------------------------------+
|                                                                               |
|  >> department                                                                |
|       source: segment(-4)                                                     |
|       type: string                                                            |
|       [sample: "sales"]                                                       |
|                                                                               |
|     year                                                                      |
|       source: segment(-3)                                                     |
|       type: integer                                                           |
|       [sample: 2023, 2024]                                                    |
|                                                                               |
|     month                                                                     |
|       source: segment(-2)                                                     |
|       type: string                                                            |
|       [sample: "jan", "feb", "mar"]                                           |
|                                                                               |
|     doc_type                                                                  |
|       source: filename                                                        |
|       pattern: (\w+)_\d+                                                      |
|       type: string                                                            |
|       [sample: "orders"]                                                      |
|                                                                               |
|  [a] Add field  [d] Delete  [j/k] Navigate  [Enter] Edit field                |
|                                                                               |
+------------------------------------------------------------------------------+
|                                                                               |
|  [Tab] Next section  [Enter] Save  [Esc] Cancel                               |
+===============================================================================+
```

**Section Keybindings (same pattern as Glob Explorer EditRule):**

| Focus Section | Key | Action |
|---------------|-----|--------|
| **Glob Pattern** | Any char | Edit pattern |
| | Backspace | Delete char |
| | Enter | Confirm, move to Tag |
| **Tag Name** | Any char | Edit tag name |
| | Backspace | Delete char |
| | Enter | Confirm, move to Fields |
| **Fields** | j/k | Navigate field list |
| | Enter | Edit selected field |
| | a | Add new field |
| | d | Delete selected field |
| | i | Re-infer from LLM (with updated glob) |

**Global Keybindings:**

| Key | Action |
|-----|--------|
| Tab | Next section |
| Shift+Tab | Previous section |
| Enter (from last section) | Save and exit editing |
| Esc | Cancel editing |

#### 3.5.12.6 Acceptance Flow

When a cluster is accepted (`a` key), the system:

1. **Creates extraction rule** from proposal (or edited version)
2. **Tags matching files** with the proposed tag
3. **Shows confirmation** with summary
4. **Auto-advances** to next cluster (if any)

**Acceptance Confirmation:**

```
+-----------------------------------------------------+
|  Rule Created: sales_data                            |
|                                                      |
|  Glob: **/sales/{year}/{month}/*.csv                 |
|  Fields: department, year, month, doc_type           |
|  Files tagged: 247                                   |
|                                                      |
|  [n] Next cluster  [v] View rule  [Esc] Overview     |
+-----------------------------------------------------+
```

**Batch Accept (`A`):**

Users can accept all clusters at once:

```
+-----------------------------------------------------+
|  Accept all 4 clusters?                              |
|                                                      |
|  [A] Sales Reports        247 files  -> sales_data   |
|  [B] Client Reports        89 files  -> client_data  |
|  [C] Backup Files          12 files  -> backups      |
|  [~] Unclustered (152 files will remain untagged)    |
|                                                      |
|  This will create 3 extraction rules.                |
|                                                      |
|  [Enter] Confirm  [Esc] Cancel                       |
+-----------------------------------------------------+
```

#### 3.5.12.7 Integration with Pathfinder/Semantic Path Wizards

Cluster proposals can be **refined** using other wizards:

| Key | Action | Wizard | Use Case |
|-----|--------|--------|----------|
| `w` | Pathfinder | Pathfinder Wizard | Complex extraction needing Python |
| `s` | Semantic | Semantic Path Wizard | Recognize standard patterns |
| `l` | Label | Labeling Wizard | Get AI-suggested tag names |

**Wizard Handoff Flow:**

When user presses `w` (Pathfinder) on a cluster:

```
Cluster Review                    Pathfinder Wizard
     |                                  |
     | w (invoke)                       |
     |--------------------------------->|
     |                                  |
     | Pre-filled:                      |
     |   - Sample paths (5 from cluster)|
     |   - Proposed fields              |
     |   - Field types                  |
     |                                  |
     |                                  | User edits/confirms
     |                                  |
     |<---------------------------------|
     | Return:                          |
     |   - Generated rule (YAML/Python) |
     |   - Confidence score             |
     |                                  |
     v                                  |
  Update cluster proposal              |
  with wizard output                   |
```

**Wizard Menu (from Expanded view):**

```
+-- AI WIZARDS ---------------------------+
|                                          |
|  [w] Pathfinder - Generate extractor     |
|  [s] Semantic Path - Recognize patterns  |
|  [l] Labeling - Suggest tag names        |
|                                          |
|  [Esc] Cancel                            |
+------------------------------------------+
```

#### 3.5.12.8 Unclustered Files Handling

Files that don't cluster well (low similarity) are grouped in `[~] Unclustered`:

```
+========================[ UNCLUSTERED FILES (152) ]========================+
|                                                                            |
|  These files have low structural similarity and didn't form clusters.      |
|                                                                            |
|  Options:                                                                  |
|    [m] Manual review - Browse and tag individually                         |
|    [h] Provide hints - Help AI understand patterns                         |
|    [r] Re-cluster - Try with different parameters                          |
|    [i] Ignore - Skip these files for now                                   |
|                                                                            |
+-- FILES (showing 1-20 of 152) -------------------------------------------+
|                                                                            |
|    misc_report_2024.pdf                                                    |
|    old_data_backup.tar.gz                                                  |
|    config_template.yaml                                                    |
|    random_notes.txt                                                        |
|    ...                                                                     |
|                                                                            |
+----------------------------------------------------------------------------+
|  [j/k] Navigate  [Enter] Preview  [t] Tag single file  [Esc] Back          |
+============================================================================+
```

**Re-cluster with Hints (`h`):**

```
+-- CLUSTERING HINTS -------------------------------------------------+
|                                                                      |
|  Help the AI understand your file organization:                      |
|                                                                      |
|  Hint: ___________________________________________                   |
|                                                                      |
|  Examples:                                                           |
|    "Files with dates should be grouped together"                     |
|    "Ignore file extensions, focus on folder structure"               |
|    "The second folder is always the project name"                    |
|                                                                      |
|  [Enter] Re-cluster with hint  [Esc] Cancel                          |
+----------------------------------------------------------------------+
```

#### 3.5.12.9 Data Model Extensions

Add to `DiscoverState` (from `specs/views/discover.md` Section 5):

```rust
pub struct DiscoverState {
    // ... existing fields ...

    // --- Cluster Review ---
    pub cluster_overlay_open: bool,
    pub cluster_state: ClusterReviewState,
    pub clusters: Vec<PathCluster>,
    pub selected_cluster: usize,
    pub unclustered_files: Vec<FileInfo>,

    // --- Cluster Progress ---
    pub clustering_progress: Option<ClusteringProgress>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClusterReviewState {
    Clustering,         // AI processing in progress
    Overview,           // Viewing cluster list
    Expanded,           // Single cluster detail view
    Editing,            // Modifying cluster proposal
    Accepted,           // Rule created confirmation
}

#[derive(Debug, Clone)]
pub struct PathCluster {
    pub id: String,                    // "A", "B", "C", etc.
    pub name: String,                  // AI-proposed name
    pub file_count: usize,
    pub similarity_score: f32,         // 0.0 - 1.0
    pub confidence: f32,               // 0.0 - 1.0

    // Representative paths (max 5)
    pub representative_paths: Vec<String>,

    // All file IDs in cluster
    pub file_ids: Vec<i64>,

    // Proposed extraction
    pub proposed_glob: String,
    pub proposed_tag: String,
    pub proposed_fields: Vec<ProposedField>,

    // State tracking
    pub is_expanded: bool,
    pub is_accepted: bool,
    pub is_edited: bool,
}

#[derive(Debug, Clone)]
pub struct ProposedField {
    pub name: String,                  // e.g., "mission_id"
    pub source: FieldSource,
    pub pattern: Option<String>,       // Regex if applicable
    pub field_type: String,            // "string", "integer", "date", "uuid"
    pub sample_values: Vec<String>,    // Up to 5 sample values
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub enum FieldSource {
    Segment(i32),       // segment(-3)
    Filename,           // filename
    FullPath,           // full_path with regex
    RelPath,            // rel_path with regex
}

#[derive(Debug, Clone)]
pub struct ClusteringProgress {
    pub total_files: usize,
    pub processed_files: usize,
    pub elapsed_secs: f32,
    pub stage: ClusteringStage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClusteringStage {
    Loading,            // Reading file paths
    Embedding,          // Creating embeddings
    Clustering,         // Running HDBSCAN
    Analyzing,          // Generating proposals
    Complete,
}
```

#### 3.5.12.10 Keybindings Summary

**Global (Discover Mode) - New Keys:**

| Key | Action |
|-----|--------|
| `C` | **Open Cluster Review** for current file set |

**Sources Manager - New Keys:**

| Key | Action |
|-----|--------|
| `c` | **Cluster source** - run clustering on selected source |

**AI Wizards Menu (`W`) - New Option:**

| Key | Action |
|-----|--------|
| `1` | **Path Clustering** - open cluster wizard |

**Cluster Review Overlay:**

| Key | State | Action |
|-----|-------|--------|
| `j`/`k` | Overview/Expanded | Navigate clusters/files |
| `A`-`Z` | Overview | Quick-jump to cluster |
| `Enter`/`Space` | Overview | Expand selected cluster |
| `h`/`Esc` | Expanded | Return to overview |
| `n`/`p` | Expanded | Next/previous cluster |
| `a` | Overview/Expanded | Accept cluster proposal |
| `A` | Overview | Accept all clusters |
| `e` | Overview/Expanded | Edit cluster proposal |
| `r` | Overview/Expanded | Create rule immediately |
| `w` | Expanded | Invoke Pathfinder Wizard |
| `s` | Expanded | Invoke Semantic Path Wizard |
| `l` | Expanded | Invoke Labeling Wizard |
| `Tab` | Overview | Toggle file list panel |
| `Esc` | Overview | Close overlay |

**Editing Mode:**

| Key | Action |
|-----|--------|
| `Tab` | Next section (Glob -> Tag -> Fields) |
| `Shift+Tab` | Previous section |
| `j`/`k` | Navigate within section |
| `a` | Add field (in Fields section) |
| `d` | Delete field (in Fields section) |
| `i` | Re-infer fields from LLM |
| `Enter` | Save changes |
| `Esc` | Cancel editing |

**Unclustered Files:**

| Key | Action |
|-----|--------|
| `m` | Manual review mode |
| `h` | Provide clustering hints |
| `r` | Re-cluster with current hints |
| `i` | Ignore unclustered files |
| `t` | Tag single file |

#### 3.5.12.11 Implementation Phases

| Phase | Scope | Success Criteria |
|-------|-------|------------------|
| **Phase 1** | Clustering trigger + progress UI | `C` key triggers clustering, progress shows |
| **Phase 2** | Overview state + cluster list | Clusters display with navigation |
| **Phase 3** | Expanded view + file list | Full cluster detail with scrolling |
| **Phase 4** | Accept flow + rule creation | Rules created, files tagged |
| **Phase 5** | Editing mode | Field editing with Tab navigation |
| **Phase 6** | Wizard integration | `w`/`s`/`l` invoke appropriate wizards |
| **Phase 7** | Unclustered handling | Hints, re-clustering, manual review |
| **Phase 8** | Batch operations | `A` for accept-all |

**Phase Dependencies:**

```
Phase 1 ─┬─> Phase 2 ─┬─> Phase 3 ──> Phase 4 ──> Phase 5
         │            │
         │            └─> Phase 6
         │
         └─> Phase 7 ──> Phase 8
```

---

## Summary

This specification adds complete TUI integration for the Path Intelligence Engine:

1. **Three entry points**: Files panel (`C`), Sources Manager (`c`), AI Wizards menu (`W` then `1`)
2. **5-state machine**: Clustering -> Overview -> Expanded -> Editing -> Accepted
3. **Full keybinding coverage**: Navigation, editing, wizard handoff
4. **Integration with existing wizards**: Pathfinder, Semantic Path, Labeling
5. **Unclustered file handling**: Hints, re-clustering, manual review options
6. **Data model extensions**: `PathCluster`, `ClusterReviewState`, progress tracking

The design follows established TUI patterns from `discover.md` (dropdown overlays, state machines) and `ai_wizards.md` (wizard integration, draft workflows).

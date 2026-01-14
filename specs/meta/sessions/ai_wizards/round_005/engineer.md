# Engineer Round 005: GAP-STATE-005

## Gap Resolution: GAP-STATE-005

**Gap:** The Draft Lifecycle state machine (Section 4.1) shows states (GENERATING, DRAFT, APPROVED, REJECTED, MANUAL, CANCELED, COMMITTED) but has no transition triggers, keybindings, or timeout behavior documented.

**Confidence:** HIGH

---

### Proposed Solution

The Draft Lifecycle is a **meta-state machine** that governs all AI-generated artifacts (extractors, parsers, extraction rules, labels). Unlike the individual wizard state machines (Pathfinder, Parser Lab, etc.) which handle user interaction during generation, the Draft Lifecycle manages the **persistence and approval workflow** across all wizard outputs.

**Key Insight:** The existing diagram in Section 4.1 conflates two concerns:
1. **Generation states** (GENERATING, ERROR, TIMEOUT) - handled by individual wizards
2. **Draft management states** (DRAFT, APPROVED, REJECTED, MANUAL, COMMITTED) - cross-cutting lifecycle

This resolution separates these concerns and documents the triggers for draft management.

**Relationship to Wizard State Machines:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        WIZARD STATE MACHINES                                 │
│  (Pathfinder, Parser Lab, Labeling, Semantic Path)                          │
│                                                                             │
│  Entry: User invokes wizard (W menu, 'w'/'g'/'l'/'S' keys)                 │
│  Internal: ANALYZING → RESULT_* → HINT_INPUT → etc.                        │
│  Exit: APPROVED or CANCELED                                                 │
│                                                                             │
│  On APPROVED: Creates draft in Draft Lifecycle                              │
│  On CANCELED: No draft created                                              │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       │ Creates draft
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        DRAFT LIFECYCLE                                       │
│  (Manages all draft artifacts)                                               │
│                                                                             │
│  DRAFT → COMMITTED (on wizard approval)                                     │
│  DRAFT → EXPIRED (24h timeout)                                              │
│  DRAFT → MANUAL_EDIT (user opens in editor outside wizard)                  │
│  MANUAL_EDIT → COMMITTED (user runs 'casparian draft commit')               │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

#### Revised State Diagram

The original diagram combines wizard generation with draft lifecycle. Here is the **Draft Lifecycle only**, which starts after a wizard creates a draft:

```
                    ┌─────────────────────────────────────────────────────────────────────┐
                    │                        DRAFT LIFECYCLE                               │
                    │                  (Post-Wizard Artifact Management)                   │
                    └─────────────────────────────────────────────────────────────────────┘

                                                 │
                                                 │ Wizard creates draft
                                                 │ (Wizard APPROVED state)
                                                 ▼
                                        ┌─────────────────┐
                                        │     PENDING     │◄───────────────────────────────┐
                                        │   (in drafts/)  │                                │
                                        └────────┬────────┘                                │
                                                 │                                         │
                    ┌────────────────────────────┼────────────────────────────┐            │
                    │                            │                            │            │
                    ▼                            ▼                            ▼            │
           ┌───────────────┐            ┌───────────────┐            ┌───────────────┐    │
           │   COMMITTED   │            │    EXPIRED    │            │  MANUAL_EDIT  │    │
           │ (in Layer 1)  │            │  (auto-clean) │            │ (external $EDITOR) │
           └───────────────┘            └───────────────┘            └───────┬───────┘    │
                                                                             │            │
                                                                             │ commit     │
                                                                             └────────────┘

  Transitions:
  ─────────────
  Wizard APPROVED → PENDING     : Draft file created in drafts/, manifest updated
  PENDING → COMMITTED           : User approves (Enter in wizard, or 'casparian draft commit')
  PENDING → EXPIRED             : 24h elapsed, or draft count > 10
  PENDING → MANUAL_EDIT         : User opens draft in external editor ('e' in draft list)
  MANUAL_EDIT → PENDING         : User saves and returns (ready for commit)
  MANUAL_EDIT → COMMITTED       : User runs 'casparian draft commit <id>'
```

**Note:** The original diagram's GENERATING, ERROR, TIMEOUT states are part of individual wizard state machines, not the Draft Lifecycle. REJECTED and CANCELED from the original are wizard states (Esc to cancel), not draft states.

---

#### State Definitions (Enhanced)

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| **PENDING** | Wizard creates draft artifact | User commits, draft expires, or user edits externally | Draft file exists in `~/.casparian_flow/drafts/`. Manifest entry with `status: pending_review`. File can be viewed in TUI draft list. User can commit, edit, or let expire. |
| **COMMITTED** | User approves draft via wizard (Enter) or CLI (`casparian draft commit`) | Terminal state | Artifact moved from `drafts/` to appropriate Layer 1 location (`extractors/`, `parsers/`, etc.). Database entry created. Draft file and manifest entry removed. |
| **EXPIRED** | 24h elapsed since `created_at` OR draft count exceeds limit | Terminal state | Draft file deleted. Manifest entry removed. Cleanup happens on next TUI load or via background job. |
| **MANUAL_EDIT** | User opens draft in external editor (via TUI or CLI) | User returns to TUI or commits via CLI | Draft file is open in $EDITOR. TUI shows "Editing externally..." if user navigates to draft list. Changes are not validated until user commits. |

---

#### Transitions (with Triggers and Guards)

| From | To | Trigger | Guard | Side Effects |
|------|----|---------|-------|--------------|
| (wizard) | PENDING | Wizard emits APPROVED | Draft artifact valid | Create draft file in `drafts/`; Update manifest.json; Log to cf_ai_audit_log |
| PENDING | COMMITTED | `Enter` in wizard result | Validation passes | Move file to Layer 1; Create DB entry; Remove from manifest; Delete draft file |
| PENDING | COMMITTED | `casparian draft commit <id>` | Draft exists, validation passes | Same as above |
| PENDING | COMMITTED | `c` in TUI Draft List | Draft selected, validation passes | Same as above |
| PENDING | EXPIRED | Timer check (on TUI load or background) | `now - created_at > 24h` | Delete draft file; Remove from manifest |
| PENDING | EXPIRED | Draft count check | `draft_count > 10` (oldest first) | Same as above |
| PENDING | MANUAL_EDIT | `e` in TUI Draft List | Draft selected, $EDITOR set | Open file in $EDITOR; TUI waits |
| PENDING | MANUAL_EDIT | `casparian draft edit <id>` | Draft exists, $EDITOR set | Same as above |
| MANUAL_EDIT | PENDING | User closes editor without committing | File saved | TUI returns to draft list; File ready for commit |
| MANUAL_EDIT | COMMITTED | `casparian draft commit <id>` (from another terminal) | Draft valid | Move to Layer 1; Close editor loses changes |

---

#### Keybindings

**Draft List Panel** (accessed via `D` from Discover or any TUI view):

| Key | Action | Guard |
|-----|--------|-------|
| `j` / `↓` | Move down in draft list | - |
| `k` / `↑` | Move up in draft list | - |
| `Enter` / `c` | Commit selected draft | Validation passes |
| `e` | Edit draft in $EDITOR | $EDITOR set |
| `d` | Delete draft (with confirmation) | - |
| `p` | Preview draft content | - |
| `v` | Validate draft (run validation without commit) | - |
| `Esc` | Close draft list, return to previous view | - |

**Draft Preview Panel** (shown when `p` pressed):

| Key | Action |
|-----|--------|
| `↑` / `↓` | Scroll content |
| `Enter` / `c` | Commit from preview |
| `e` | Edit from preview |
| `Esc` | Close preview, return to list |

**Delete Confirmation Dialog**:

| Key | Action |
|-----|--------|
| `Enter` / `y` | Confirm deletion |
| `Esc` / `n` | Cancel deletion |

---

#### CLI Commands (for Draft Lifecycle)

```bash
# List all drafts
casparian draft list
# Output:
#   ID        TYPE       NAME                 CREATED        EXPIRES
#   a7b3c9d2  extractor  healthcare_path      2h ago         22h left
#   f8e2d1c0  parser     sales_parser v1.0.0  5h ago         19h left

# Commit a draft
casparian draft commit a7b3c9d2
# Output: Committed healthcare_path to ~/.casparian_flow/extractors/

# Edit a draft
casparian draft edit a7b3c9d2
# Opens $EDITOR with draft file

# Delete a draft
casparian draft delete a7b3c9d2 --confirm
# Output: Deleted draft a7b3c9d2

# Validate a draft without committing
casparian draft validate a7b3c9d2
# Output: Validation passed (or shows errors)

# Preview draft content
casparian draft show a7b3c9d2
# Output: Prints draft file content

# Clean expired drafts (usually automatic)
casparian draft clean
# Output: Removed 3 expired drafts
```

---

### Timeout Behavior

**24-Hour Expiry:**

Drafts are ephemeral by design. They are working artifacts, not permanent storage.

| Policy | Value | Rationale |
|--------|-------|-----------|
| Default expiry | 24 hours | Long enough for session work; short enough to prevent clutter |
| Max draft count | 10 | Prevents runaway draft accumulation |
| Expiry check frequency | On TUI load + every 5 minutes background | Non-blocking; lazy cleanup |
| Oldest-first deletion | When count > 10 | Preserve recent work |

**Cleanup Algorithm:**

```rust
fn cleanup_expired_drafts(manifest: &mut Manifest) {
    let now = Utc::now();

    // Remove expired by time
    manifest.drafts.retain(|draft| {
        if now > draft.expires_at {
            std::fs::remove_file(&draft.file_path).ok();
            false  // Remove from manifest
        } else {
            true
        }
    });

    // Remove oldest if count exceeds limit
    while manifest.drafts.len() > MAX_DRAFT_COUNT {
        manifest.drafts.sort_by_key(|d| d.created_at);
        if let Some(oldest) = manifest.drafts.first() {
            std::fs::remove_file(&oldest.file_path).ok();
        }
        manifest.drafts.remove(0);
    }

    manifest.save()?;
}
```

**Expiry Warning:**

When a draft is approaching expiry (< 2 hours remaining), the TUI Draft List shows a warning:

```
┌─ DRAFTS ────────────────────────────────────────────────────────┐
│                                                                  │
│  ID        TYPE       NAME                 STATUS    EXPIRES     │
│  ─────────────────────────────────────────────────────────────── │
│  ► a7b3c9  extractor  healthcare_path      pending   22h left   │
│    f8e2d1  parser     sales_parser v1.0.0  pending   ⚠️ 1h left │
│                                                                  │
│  [c] Commit   [e] Edit   [d] Delete   [v] Validate   [Esc] Close │
└──────────────────────────────────────────────────────────────────┘
```

---

### Draft Manifest Schema (Enhanced)

```json
{
  "version": "1.0",
  "last_cleanup": "2026-01-13T10:30:00Z",
  "drafts": [
    {
      "id": "a7b3c9d2",
      "type": "extractor",          // extractor | parser | rule | label
      "subtype": "yaml",            // For extractors: yaml | python
      "file": "extractor_a7b3c9d2.yaml",
      "name": "healthcare_path",    // User-provided name
      "version": null,              // For parsers only
      "created_at": "2026-01-13T08:30:00Z",
      "expires_at": "2026-01-14T08:30:00Z",
      "status": "pending_review",   // pending_review | manual_edit
      "source_context": {
        "sample_paths": ["/data/ADT_Inbound/2024/01/msg_001.hl7"],
        "user_hints": null,
        "wizard": "pathfinder"
      },
      "model": "qwen-2.5-7b",
      "validation": {
        "last_run": "2026-01-13T08:30:15Z",
        "status": "passed",         // passed | warning | failed | not_run
        "errors": [],
        "warnings": []
      }
    }
  ]
}
```

---

### Data Model (Rust structs)

```rust
/// Draft lifecycle states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DraftStatus {
    PendingReview,
    ManualEdit,
    // Note: Committed and Expired are terminal - drafts in these states
    // are removed from manifest, not tracked
}

/// Draft artifact in manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,                       // 8-char hex (blake3 prefix)
    pub draft_type: DraftType,
    pub file_path: PathBuf,               // Relative to drafts/
    pub name: String,                     // User-provided name
    pub version: Option<String>,          // For parsers
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: DraftStatus,
    pub source_context: DraftSourceContext,
    pub model: String,
    pub validation: DraftValidation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DraftType {
    Extractor { subtype: ExtractorSubtype },
    Parser,
    ExtractionRule,
    Label,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExtractorSubtype {
    Yaml,
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftSourceContext {
    pub sample_paths: Vec<PathBuf>,
    pub user_hints: Option<Vec<String>>,
    pub wizard: String,  // "pathfinder" | "parser_lab" | "labeling" | "semantic_path"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftValidation {
    pub last_run: Option<DateTime<Utc>>,
    pub status: ValidationStatus,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationStatus {
    Passed,
    Warning,
    Failed,
    NotRun,
}

/// Manifest file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftManifest {
    pub version: String,
    pub last_cleanup: DateTime<Utc>,
    pub drafts: Vec<Draft>,
}

/// TUI state for draft list
#[derive(Debug, Clone)]
pub struct DraftListState {
    pub drafts: Vec<Draft>,
    pub selected_index: usize,
    pub preview_open: bool,
    pub preview_content: Option<String>,
    pub delete_confirmation: bool,
    pub loading: bool,
    pub error_message: Option<String>,
}
```

---

### Examples

**Example 1: Draft created and committed via wizard**

```
1. User invokes Pathfinder Wizard (w on file)
2. Wizard ANALYZING → RESULT_YAML
3. User presses Enter
4. Wizard state: APPROVED
   └─ Creates draft:
      - File: ~/.casparian_flow/drafts/extractor_a7b3c9d2.yaml
      - Manifest entry added with status: pending_review
5. Draft Lifecycle: PENDING
6. Wizard commits immediately on APPROVED:
   └─ Moves file to ~/.casparian_flow/extraction_rules/healthcare_path.yaml
   └─ Creates entry in scout_extraction_rules table
   └─ Removes from manifest
7. Draft Lifecycle: COMMITTED (terminal)
8. Dialog closes
```

**Example 2: Draft expires after 24 hours**

```
1. User creates draft at 10:00 AM
2. User closes TUI, doesn't return
3. expires_at = 10:00 AM next day
4. User opens TUI at 2:00 PM next day
5. TUI loads, runs cleanup_expired_drafts()
6. Draft file deleted, manifest entry removed
7. Draft Lifecycle: EXPIRED (terminal)
8. User sees empty draft list (or other non-expired drafts)
```

**Example 3: Manual edit workflow**

```
1. User has draft in PENDING state (from yesterday, 20h left)
2. User opens TUI, presses D to view drafts
3. User selects draft, presses 'e'
4. Draft Lifecycle: MANUAL_EDIT
5. $EDITOR opens with draft file
6. User makes changes, saves, closes editor
7. TUI resumes, draft back to PENDING
8. User presses 'v' to validate
   - Validation runs on edited content
   - Shows "Validation passed" or errors
9. User presses 'c' to commit
10. Draft Lifecycle: COMMITTED
```

**Example 4: Draft count limit reached**

```
1. User has 10 drafts in manifest
2. User creates new draft (11th)
3. Cleanup runs: oldest draft deleted (FIFO)
4. User now has 10 drafts, newest one included
```

---

### Trade-offs

**Pros:**

1. **Separation of concerns** - Wizard state machines handle interaction; Draft Lifecycle handles persistence
2. **24h expiry prevents clutter** - Users don't accumulate stale drafts forever
3. **CLI parity** - Every TUI action has a CLI equivalent
4. **Validation before commit** - Users can validate drafts without committing
5. **External edit support** - Power users can use their preferred editor

**Cons:**

1. **24h may be too short** - Users working on complex pipelines over multiple days may lose work
2. **No draft versioning** - If user edits a draft multiple times, no history
3. **No draft sharing** - Drafts are local to user's machine
4. **MANUAL_EDIT state race condition** - User editing in $EDITOR while CLI commits from another terminal

**Mitigations:**

1. Add `casparian draft extend <id>` to reset expiry timer (additional 24h)
2. Keep 1-2 previous versions of draft in manifest (optional, Phase 2)
3. Out of scope for alpha; consider for team features later
4. MANUAL_EDIT state sets a lock file; CLI commit checks lock before proceeding

---

### Integration with Individual Wizards

Each wizard's APPROVED state triggers draft creation:

| Wizard | Draft Type | Layer 1 Destination |
|--------|------------|---------------------|
| Pathfinder (YAML) | `Extractor { subtype: Yaml }` | `extraction_rules/` + `scout_extraction_rules` table |
| Pathfinder (Python) | `Extractor { subtype: Python }` | `extractors/` + `scout_extractors` table |
| Parser Lab | `Parser` | `parsers/` + `cf_parsers` table |
| Labeling | `Label` | `cf_signature_groups` table (no file) |
| Semantic Path | `ExtractionRule` | `extraction_rules/` + `scout_extraction_rules` table |

---

### New Gaps Introduced

1. **GAP-LOCK-001**: Draft locking for MANUAL_EDIT state
   - How to prevent CLI commit while user is editing in $EDITOR?
   - Lock file mechanism? Advisory vs mandatory?

2. **GAP-TUI-002**: Draft List panel integration
   - Where does 'D' key fit in global keybindings?
   - Conflict with existing key assignments?

3. **GAP-EXTEND-001**: Draft expiry extension
   - Should `casparian draft extend` exist?
   - Can users set custom expiry in config?

4. **GAP-VALIDATE-001**: Validation command specifics
   - What does validation check for each draft type?
   - YAML syntax? Python execution? Schema conformance?

---

### Validation Checklist

- [x] Diagram updated to show only Draft Lifecycle (separated from wizard states)
- [x] All states have entry/exit conditions documented
- [x] Triggers for all transitions are explicit
- [x] Keybindings table for TUI Draft List
- [x] CLI commands for parity
- [x] Timeout behavior (24h) fully specified
- [x] Cleanup algorithm documented
- [x] Manifest schema enhanced with validation status
- [x] Rust data model provided
- [x] Integration with individual wizards documented

---

### References

- `specs/ai_wizards.md` Section 4 (Draft Lifecycle)
- `specs/ai_wizards.md` Section 4.3 (Draft Manifest - original)
- `specs/ai_wizards.md` Section 4.4 (Cleanup Policy)
- `specs/ai_wizards.md` Section 5.1.1 (Pathfinder state machine)
- `specs/views/discover.md` Section 6 (Keybindings patterns)
- `CLAUDE.md` (Database architecture, Layer 1 tables)

# Engineer Round 010: GAP-MODEL-001

## Gap Resolution: GAP-MODEL-001

**Gap:** Draft ID generation not specified
**Confidence:** HIGH

---

### Problem Statement

The spec references "draft IDs" for wizard-generated artifacts (extraction rules, parsers) but doesn't specify:

1. ID format and generation algorithm
2. When drafts are created vs when they become permanent
3. Draft storage location and cleanup
4. ID collision handling

Current spec shows examples like:
- Section 4.2: `extractor_a7b3c9d2.py`, `parser_f8e2d1c0.py`
- Section 4.3: `"id": "a7b3c9d2"` in manifest.json
- Section 8 MCP tools: `draft_id` as return value

But no specification of how `a7b3c9d2` is generated or what guarantees it provides.

---

### Proposed Solution: 8-Character UUID Prefix

#### ID Format Specification

| Property | Value |
|----------|-------|
| **Length** | 8 characters |
| **Character set** | Lowercase hexadecimal (`0-9a-f`) |
| **Encoding** | ASCII |
| **Source** | First 8 characters of UUIDv4 |
| **Example** | `a7b3c9d2`, `f8e2d1c0`, `1a2b3c4d` |

**Rationale:**
- **8 characters chosen because:**
  - Already used in codebase (`&uuid::Uuid::new_v4().to_string()[..8]` in main.rs)
  - Human-readable and typeable (short enough for CLI, logs, filenames)
  - 4.3 billion possible values (2^32) - sufficient for draft IDs
  - Collision probability is negligible for <1000 concurrent drafts

- **UUIDv4 prefix chosen because:**
  - No coordination needed (no DB sequence, no central counter)
  - Works offline
  - Timestamp not leaked (unlike ULIDs)
  - Rust ecosystem has strong UUID support

#### Generation Algorithm

```rust
use uuid::Uuid;

/// Generate a new draft ID.
///
/// Returns first 8 hex characters of a UUIDv4.
/// Example: "a7b3c9d2"
pub fn generate_draft_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}
```

**Python equivalent (for wizards running in Python context):**

```python
import uuid

def generate_draft_id() -> str:
    """Generate a new draft ID.

    Returns first 8 hex characters of a UUIDv4.
    Example: "a7b3c9d2"
    """
    return str(uuid.uuid4()).replace('-', '')[:8]
```

---

### Draft Lifecycle States

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         DRAFT LIFECYCLE                                      │
│                                                                              │
│   ┌──────────┐                                                              │
│   │ (none)   │  User invokes wizard                                         │
│   └────┬─────┘                                                              │
│        │                                                                     │
│        │ AI generates output                                                 │
│        │ (ID NOT yet assigned)                                              │
│        ▼                                                                     │
│   ┌──────────┐  Validation passes                                           │
│   │VALIDATING│  ─────────────────────► Draft ID generated                   │
│   └────┬─────┘                          Draft file created                  │
│        │                                Manifest entry added                │
│        │                                                                     │
│        ▼                                                                     │
│   ┌──────────┐                                                              │
│   │ PENDING  │  Draft exists on disk, awaiting user action                  │
│   │ _REVIEW  │                                                              │
│   └────┬─────┘                                                              │
│        │                                                                     │
│   ┌────┴────────────────┬─────────────────┬────────────────┐               │
│   ▼                     ▼                 ▼                ▼               │
│ [Enter]             [d delete]        [e edit]        [24h timeout]        │
│ Approve               Reject           Manual           Expired            │
│   │                     │                │                 │               │
│   ▼                     ▼                ▼                 ▼               │
│ ┌──────────┐       ┌──────────┐    ┌──────────┐     ┌──────────┐          │
│ │COMMITTED │       │ DELETED  │    │ EDITING  │     │ EXPIRED  │          │
│ └──────────┘       └──────────┘    └────┬─────┘     └──────────┘          │
│   │                     │               │                 │               │
│   │                     │               │ Save & exit     │               │
│   │                     │               ▼                 │               │
│   │                     │          ┌──────────┐          │               │
│   │                     │          │ PENDING  │          │               │
│   │                     │          │ _REVIEW  │          │               │
│   │                     │          └──────────┘          │               │
│   │                     │                                  │               │
│   ▼                     └──────────────────────────────────┘               │
│ Move to Layer 1                    Delete draft file                       │
│ (permanent ID)                     Remove manifest entry                   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### State Definitions

| State | Description | Files on Disk | In Manifest |
|-------|-------------|---------------|-------------|
| VALIDATING | AI output being validated, no ID yet | None | No |
| PENDING_REVIEW | Draft created, awaiting user action | Yes (`{type}_{id}.py`) | Yes (status: `pending_review`) |
| EDITING | User editing in $EDITOR | Yes (unchanged) | Yes (status: `editing`) |
| COMMITTED | Approved, moved to Layer 1 | Moved to `extractors/` or `parsers/` | Removed |
| DELETED | User rejected | Deleted | Removed |
| EXPIRED | 24h timeout | Deleted | Removed |

#### Transition Triggers

| From | To | Trigger | Actions |
|------|----|---------| --------|
| VALIDATING | PENDING_REVIEW | Tier 3 validation passes | 1. Generate draft ID<br>2. Write draft file<br>3. Add manifest entry |
| PENDING_REVIEW | COMMITTED | User presses Enter/c | 1. Move file to Layer 1 dir<br>2. Rename to permanent name<br>3. Remove manifest entry |
| PENDING_REVIEW | DELETED | User presses d (+ confirm) | 1. Delete draft file<br>2. Remove manifest entry |
| PENDING_REVIEW | EDITING | User presses e | 1. Update manifest status to `editing`<br>2. Open $EDITOR |
| EDITING | PENDING_REVIEW | Editor closes | 1. Re-validate edited content<br>2. Update manifest status to `pending_review` |
| PENDING_REVIEW | EXPIRED | 24h timer | 1. Delete draft file<br>2. Remove manifest entry |

---

### Storage Specification

#### Directory Structure

```
~/.casparian_flow/
├── drafts/                              # Draft storage (temporary)
│   ├── extractor_a7b3c9d2.py            # Pathfinder draft (Python fallback)
│   ├── extractor_a7b3c9d2.yaml          # Pathfinder draft (YAML rule)
│   ├── parser_f8e2d1c0.py               # Parser Lab draft
│   └── manifest.json                    # Draft metadata + state
│
├── extractors/                          # Committed extractors (Layer 1)
│   ├── healthcare_path.py               # Renamed from draft
│   └── healthcare_path.yaml             # YAML extraction rules
│
├── parsers/                             # Committed parsers (Layer 1)
│   └── sales_parser.py                  # Renamed from draft
│
└── config.toml                          # Model configuration
```

#### Draft Filename Convention

```
{type}_{draft_id}.{ext}
```

| Component | Values | Example |
|-----------|--------|---------|
| `type` | `extractor`, `parser`, `label` | `extractor` |
| `draft_id` | 8 hex chars | `a7b3c9d2` |
| `ext` | `py`, `yaml` | `yaml` |

**Full examples:**
- `extractor_a7b3c9d2.yaml` - YAML extraction rule draft
- `extractor_f1e2d3c4.py` - Python extractor draft (fallback)
- `parser_b5c6d7e8.py` - Parser draft

#### Manifest Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["version", "drafts"],
  "properties": {
    "version": {
      "type": "integer",
      "const": 1
    },
    "drafts": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "type", "file", "created_at", "expires_at", "status"],
        "properties": {
          "id": {
            "type": "string",
            "pattern": "^[0-9a-f]{8}$",
            "description": "8-character hex draft ID"
          },
          "type": {
            "type": "string",
            "enum": ["extractor", "parser", "label"]
          },
          "file": {
            "type": "string",
            "description": "Filename relative to drafts/"
          },
          "created_at": {
            "type": "string",
            "format": "date-time"
          },
          "expires_at": {
            "type": "string",
            "format": "date-time"
          },
          "status": {
            "type": "string",
            "enum": ["pending_review", "editing"]
          },
          "source_context": {
            "type": "object",
            "properties": {
              "sample_paths": {
                "type": "array",
                "items": {"type": "string"}
              },
              "user_hints": {
                "type": ["string", "null"]
              }
            }
          },
          "model": {
            "type": "string",
            "description": "Model used for generation"
          },
          "output_format": {
            "type": "string",
            "enum": ["yaml", "python"],
            "description": "Output format (YAML preferred, Python fallback)"
          },
          "suggested_name": {
            "type": "string",
            "description": "AI-suggested name for committed artifact"
          }
        }
      }
    }
  }
}
```

#### Example Manifest

```json
{
  "version": 1,
  "drafts": [
    {
      "id": "a7b3c9d2",
      "type": "extractor",
      "file": "extractor_a7b3c9d2.yaml",
      "created_at": "2026-01-08T10:30:00Z",
      "expires_at": "2026-01-09T10:30:00Z",
      "status": "pending_review",
      "source_context": {
        "sample_paths": ["/data/ADT_Inbound/2024/01/msg_001.hl7"],
        "user_hints": null
      },
      "model": "qwen-2.5-7b",
      "output_format": "yaml",
      "suggested_name": "healthcare_path"
    },
    {
      "id": "f8e2d1c0",
      "type": "parser",
      "file": "parser_f8e2d1c0.py",
      "created_at": "2026-01-08T11:45:00Z",
      "expires_at": "2026-01-09T11:45:00Z",
      "status": "editing",
      "source_context": {
        "sample_paths": ["/data/sales/2024/january.csv"],
        "user_hints": "Column 3 is the customer ID"
      },
      "model": "qwen-2.5-7b",
      "output_format": "python",
      "suggested_name": "sales_parser"
    }
  ]
}
```

---

### Collision Handling

#### Probability Analysis

With 8 hex characters (32 bits), collision probability follows birthday paradox:

| Draft Count | Collision Probability |
|-------------|----------------------|
| 10 | 0.000001% |
| 100 | 0.0001% |
| 1000 | 0.01% |
| 10000 | 1.2% |

Given max 10 concurrent drafts (Section 4.1.2), collision is effectively impossible.

#### Detection and Handling

Even though collisions are improbable, handle them defensively:

```rust
fn create_draft(draft_type: DraftType, content: &str) -> Result<Draft, DraftError> {
    let drafts_dir = get_drafts_dir()?;
    let manifest = load_manifest(&drafts_dir)?;

    // Generate ID with collision check (max 3 attempts)
    let (id, filename) = (0..3)
        .find_map(|_| {
            let id = generate_draft_id();
            let filename = format!("{}_{}.{}",
                draft_type.prefix(),
                id,
                draft_type.extension()
            );
            let path = drafts_dir.join(&filename);

            // Check both file existence and manifest
            if !path.exists() && !manifest.has_id(&id) {
                Some((id, filename))
            } else {
                None
            }
        })
        .ok_or(DraftError::IdGenerationFailed)?;

    // Write file atomically (temp file + rename)
    let temp_path = drafts_dir.join(format!(".{}.tmp", filename));
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, drafts_dir.join(&filename))?;

    // Add to manifest
    manifest.add_draft(Draft {
        id: id.clone(),
        draft_type,
        file: filename,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
        status: DraftStatus::PendingReview,
        // ... other fields
    })?;

    Ok(draft)
}
```

---

### Cleanup Strategy

#### Automatic Cleanup (Background)

Run cleanup check:
- On TUI startup
- Every 15 minutes during TUI session
- On `casparian draft clean` CLI command

```rust
fn cleanup_expired_drafts() -> Result<CleanupReport, DraftError> {
    let drafts_dir = get_drafts_dir()?;
    let mut manifest = load_manifest(&drafts_dir)?;
    let now = Utc::now();

    let mut report = CleanupReport::default();

    // 1. Remove expired drafts
    let (expired, active): (Vec<_>, Vec<_>) = manifest.drafts
        .into_iter()
        .partition(|d| d.expires_at < now);

    for draft in expired {
        let path = drafts_dir.join(&draft.file);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        report.expired_removed += 1;
    }

    // 2. Enforce max 10 drafts (remove oldest)
    let mut active: Vec<_> = active;
    active.sort_by_key(|d| d.created_at);

    while active.len() > 10 {
        if let Some(oldest) = active.remove(0) {
            let path = drafts_dir.join(&oldest.file);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            report.overflow_removed += 1;
        }
    }

    // 3. Clean orphaned files (file exists but not in manifest)
    for entry in fs::read_dir(&drafts_dir)? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();

        if filename == "manifest.json" || filename.starts_with('.') {
            continue;
        }

        if !active.iter().any(|d| d.file == filename) {
            fs::remove_file(entry.path())?;
            report.orphans_removed += 1;
        }
    }

    // 4. Save updated manifest
    manifest.drafts = active;
    save_manifest(&drafts_dir, &manifest)?;

    Ok(report)
}
```

#### Manual Cleanup CLI

```bash
# Preview what would be cleaned
$ casparian draft clean --dry-run
Would remove 2 expired drafts:
  - extractor_a7b3c9d2.yaml (expired 2h ago)
  - parser_f8e2d1c0.py (expired 5h ago)

# Actually clean
$ casparian draft clean
Removed 2 expired drafts.

# Force clean all drafts
$ casparian draft clean --all
Removed 5 drafts.
```

---

### Permanent ID Assignment (On Commit)

When a draft is committed to Layer 1, it gets a **permanent name** chosen by the user:

```
Draft:     extractor_a7b3c9d2.yaml
             │
             │ User approves with name "healthcare_path"
             ▼
Committed: extractors/healthcare_path.yaml
```

The draft ID (`a7b3c9d2`) is **not preserved** in the committed artifact. It was only for temporary identification.

#### Commit Flow

```rust
fn commit_draft(draft_id: &str, permanent_name: &str) -> Result<(), DraftError> {
    let drafts_dir = get_drafts_dir()?;
    let mut manifest = load_manifest(&drafts_dir)?;

    // Find draft
    let draft = manifest.drafts
        .iter()
        .find(|d| d.id == draft_id)
        .ok_or(DraftError::NotFound(draft_id.to_string()))?
        .clone();

    // Validate permanent name
    if !is_valid_artifact_name(permanent_name) {
        return Err(DraftError::InvalidName(permanent_name.to_string()));
    }

    // Determine destination
    let dest_dir = match draft.draft_type {
        DraftType::Extractor => get_extractors_dir()?,
        DraftType::Parser => get_parsers_dir()?,
        DraftType::Label => get_labels_dir()?,
    };

    let dest_filename = format!("{}.{}", permanent_name, draft.extension());
    let dest_path = dest_dir.join(&dest_filename);

    // Check destination doesn't exist
    if dest_path.exists() {
        return Err(DraftError::DestinationExists(dest_path));
    }

    // Move file
    let src_path = drafts_dir.join(&draft.file);
    fs::rename(&src_path, &dest_path)?;

    // Remove from manifest
    manifest.drafts.retain(|d| d.id != draft_id);
    save_manifest(&drafts_dir, &manifest)?;

    Ok(())
}
```

---

### MCP Tool Updates

Update MCP tool return types to use the specified ID format:

```json
{
  "name": "invoke_pathfinder",
  "returns": {
    "draft_id": {
      "type": "string",
      "pattern": "^[0-9a-f]{8}$",
      "description": "8-character hex ID for the generated draft",
      "example": "a7b3c9d2"
    },
    "code_preview": "string",
    "preview_results": "object[]"
  }
}
```

```json
{
  "name": "commit_draft",
  "parameters": {
    "draft_id": {
      "type": "string",
      "pattern": "^[0-9a-f]{8}$",
      "description": "8-character hex ID from invoke_* response"
    },
    "name": "string (optional - override suggested name)",
    "version": "string (optional - for parsers)"
  }
}
```

---

### Implementation Checklist

- [ ] Add `generate_draft_id()` function to utils
- [ ] Create `Draft` struct with lifecycle states
- [ ] Implement manifest read/write with JSON schema validation
- [ ] Add collision detection loop
- [ ] Implement cleanup function with orphan detection
- [ ] Add `casparian draft` CLI subcommand (list, clean, delete)
- [ ] Wire into wizard state machines (ID generation on validation success)
- [ ] Update MCP tools to use 8-char hex pattern

---

### Trade-offs

| Decision | Chosen | Alternative | Rationale |
|----------|--------|-------------|-----------|
| ID length | 8 chars | 12, 16, or full UUID | 8 matches existing codebase pattern, human-friendly |
| Generation | UUIDv4 prefix | Content hash, sequential | No coordination needed, works offline |
| Storage | Filesystem + JSON manifest | SQLite | Simpler, human-inspectable, no schema migrations |
| Collision handling | Retry 3x | Fail immediately | Defensive without complexity |
| Permanent ID | User-chosen name | Preserve draft ID | Draft ID is temporary by design |

---

### New Gaps Introduced

None. This resolution is self-contained and aligns with existing codebase patterns.

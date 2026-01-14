# Engineer Resolution: GAP-FLOW-001

**Gap:** Wizard invocation from Files panel underspecified
**Status:** Resolved
**Date:** 2026-01-13
**Spec Affected:** specs/ai_wizards.md Section 5.6

---

## 1. Problem Statement

The AI Wizards spec (Section 5.6) defines keybindings for invoking wizards from Discover mode:

| Key | Documented Action |
|-----|------------------|
| `W` | Open Wizard menu |
| `w` | Launch Pathfinder for selected file's path |
| `g` | Launch Parser Lab for file group |
| `l` | Launch Labeling Wizard for group |
| `S` | Launch Semantic Path Wizard for source |

However, the spec leaves critical implementation details ambiguous:

1. **Context determination**: How is file path, group, or source determined?
2. **Focus management**: How does focus transfer on wizard open/close?
3. **Error handling**: What happens when required context is missing?
4. **Visual feedback**: How does the user know if invocation is valid?
5. **Entry point inventory**: Are there other ways to invoke wizards?

---

## 2. Resolution: Context Determination Algorithm

### 2.1 Context Sources by Wizard

Each wizard requires specific context. The algorithm determines context based on current TUI state.

| Wizard | Required Context | Context Source |
|--------|------------------|----------------|
| **Pathfinder** | File path(s) | Selected file OR filtered files |
| **Parser Lab** | File group (signature) | Signature group of selected file |
| **Labeling** | File group (signature) | Signature group of selected file |
| **Semantic Path** | Source | Currently selected source |

### 2.2 Algorithm: Pathfinder (`w` key)

```rust
fn determine_pathfinder_context(state: &DiscoverState) -> PathfinderContext {
    // Priority 1: If in Files panel with file selected
    if state.focus == DiscoverFocus::Files && state.selected_file < state.files.len() {
        let file = &state.files[state.selected_file];
        return PathfinderContext::SingleFile {
            path: file.path.clone(),
            rel_path: file.rel_path.clone(),
        };
    }

    // Priority 2: If filter is active, use all filtered files
    if !state.filter.is_empty() && !state.files.is_empty() {
        let paths: Vec<String> = state.files.iter()
            .map(|f| f.path.clone())
            .collect();
        return PathfinderContext::MultipleFiles { paths };
    }

    // Priority 3: If in Pending Review panel with unmatched paths selected
    if state.pending_review_open && state.pending_review_category == PendingCategory::UnmatchedPaths {
        let paths = state.pending_review_items.iter()
            .map(|item| item.path.clone())
            .collect();
        return PathfinderContext::MultipleFiles { paths };
    }

    // No valid context
    PathfinderContext::None
}
```

**Context determination examples:**

| Scenario | Context Determined | Wizard Opens With |
|----------|-------------------|-------------------|
| File selected in Files panel | `SingleFile` | Selected file's path |
| Filter active with 15 matches | `MultipleFiles` | All 15 filtered paths |
| No file selected, no filter | `None` | Error: "Select a file first" |
| In Pending Review > Unmatched | `MultipleFiles` | Selected unmatched paths |

### 2.3 Algorithm: Parser Lab (`g` key)

```rust
fn determine_parser_lab_context(state: &DiscoverState) -> ParserLabContext {
    // Must have a selected file to determine its signature group
    if state.focus != DiscoverFocus::Files || state.selected_file >= state.files.len() {
        return ParserLabContext::None;
    }

    let file = &state.files[state.selected_file];

    // Priority 1: File has a signature group
    if let Some(sig_group_id) = &file.signature_group_id {
        // Look up all files in this signature group
        let group_files = state.files.iter()
            .filter(|f| f.signature_group_id.as_ref() == Some(sig_group_id))
            .cloned()
            .collect();

        return ParserLabContext::SignatureGroup {
            group_id: sig_group_id.clone(),
            files: group_files,
            fingerprint: file.signature_fingerprint.clone(),
        };
    }

    // Priority 2: Use single file (degenerate case)
    ParserLabContext::SingleFile {
        path: file.path.clone(),
    }
}
```

**Context determination examples:**

| Scenario | Context Determined | Wizard Opens With |
|----------|-------------------|-------------------|
| File in signature group abc123 | `SignatureGroup` | All 47 files in group |
| File without signature group | `SingleFile` | Just the selected file |
| No file selected | `None` | Error: "Select a file group" |

### 2.4 Algorithm: Labeling (`l` key)

```rust
fn determine_labeling_context(state: &DiscoverState) -> LabelingContext {
    // Same as Parser Lab - requires signature group
    if state.focus != DiscoverFocus::Files || state.selected_file >= state.files.len() {
        return LabelingContext::None;
    }

    let file = &state.files[state.selected_file];

    if let Some(sig_group_id) = &file.signature_group_id {
        return LabelingContext::SignatureGroup {
            group_id: sig_group_id.clone(),
            headers: file.headers.clone(),
            sample_values: file.sample_values.clone(),
            file_count: state.files.iter()
                .filter(|f| f.signature_group_id.as_ref() == Some(sig_group_id))
                .count(),
        };
    }

    LabelingContext::None
}
```

### 2.5 Algorithm: Semantic Path (`S` key)

```rust
fn determine_semantic_path_context(state: &DiscoverState) -> SemanticPathContext {
    // Priority 1: Explicitly selected source
    if state.selected_source < state.sources.len() {
        let source = &state.sources[state.selected_source];

        // Gather sample paths from this source
        let sample_paths: Vec<String> = state.files.iter()
            .filter(|f| f.source_id == source.id)
            .take(50)  // Limit samples for performance
            .map(|f| f.path.clone())
            .collect();

        if sample_paths.is_empty() {
            return SemanticPathContext::None;
        }

        return SemanticPathContext::Source {
            source_id: source.id,
            source_name: source.name.clone(),
            source_path: source.path.clone(),
            sample_paths,
        };
    }

    SemanticPathContext::None
}
```

---

## 3. Resolution: Focus Management

### 3.1 Focus Transfer on Wizard Open

When a wizard is invoked, focus transfers according to this hierarchy:

```
DISCOVER_NORMAL ─────► WIZARD_DIALOG
     │                      │
     │ W/w/g/l/S           │
     │                      │
     └──────────────────────┘
```

**Behavior:**

1. Discover mode state is **frozen** (selections preserved)
2. Wizard dialog appears as **modal overlay** (centered, 80% width)
3. All Discover keybindings are **suspended**
4. Only wizard-specific keybindings are active

### 3.2 Focus Transfer on Wizard Close

| Wizard Outcome | Focus Returns To | State Changes |
|---------------|------------------|---------------|
| **Approved** | Files panel | New extractor/parser/label committed |
| **Canceled** (Esc) | Previous panel | No state changes |
| **Error** (closed) | Previous panel | No state changes |

```rust
fn handle_wizard_close(app: &mut App, outcome: WizardOutcome) {
    // Close the wizard dialog
    app.wizard_dialog = None;

    // Return focus to Discover
    match outcome {
        WizardOutcome::Approved { artifact_type, artifact_id } => {
            // Focus Files panel after approval
            app.discover.focus = DiscoverFocus::Files;

            // Show success toast
            app.toast = Some(Toast::success(format!(
                "{} created successfully",
                artifact_type
            )));

            // Refresh relevant data
            match artifact_type {
                ArtifactType::ExtractionRule => app.refresh_files(),
                ArtifactType::Parser => app.refresh_parsers(),
                ArtifactType::Label => app.refresh_tags(),
            }
        }
        WizardOutcome::Canceled | WizardOutcome::Error => {
            // Return to previous focus (preserved in state)
            app.discover.focus = app.wizard_previous_focus.take()
                .unwrap_or(DiscoverFocus::Files);
        }
    }
}
```

### 3.3 State Preservation During Wizard

The wizard must preserve Discover state:

```rust
pub struct WizardDialogState {
    // Previous Discover focus (to restore on close)
    pub previous_focus: DiscoverFocus,

    // Selected items when wizard was opened
    pub context_file_ids: Vec<i64>,
    pub context_source_id: Option<i64>,
    pub context_signature_group_id: Option<String>,

    // Wizard-specific state
    pub wizard_type: WizardType,
    pub wizard_state: Box<dyn WizardState>,
}
```

---

## 4. Resolution: Error Cases (Missing Context)

### 4.1 Error Message Table

| Wizard | Missing Context | Error Message | Recovery Action |
|--------|----------------|---------------|-----------------|
| Pathfinder | No file selected | "Select a file first" | Focus Files panel |
| Pathfinder | Empty source | "Source has no files" | Scan a directory |
| Parser Lab | No signature group | "Select a file group" | Select file with known structure |
| Parser Lab | No file selected | "Select a file first" | Focus Files panel |
| Labeling | No signature group | "Select a file group" | Select file with known structure |
| Labeling | No headers detected | "Cannot label files without headers" | - |
| Semantic Path | No source selected | "Select a source first" | Open Sources dropdown |
| Semantic Path | Empty source | "Source has no files" | Scan a directory |

### 4.2 Error Display

Errors appear as inline toast messages, not modal dialogs:

```
┌────────────────────┬────────────────────────────────────────┬─────────────────┐
│     SIDEBAR        │              FILES                     │    PREVIEW      │
├────────────────────┼────────────────────────────────────────┼─────────────────┤
│ ▼ sales_data (142) │                                        │                 │
│                    │  ⚠ Select a file first                 │                 │
│ ▼ All files (142)  │     ↑ Error toast (3s auto-dismiss)    │                 │
│                    │                                        │                 │
│                    │  invoices/jan.csv        [sales]  2KB  │                 │
│                    │  invoices/feb.csv        [sales]  3KB  │                 │
└────────────────────┴────────────────────────────────────────┴─────────────────┘
```

### 4.3 Implementation

```rust
fn invoke_wizard(app: &mut App, wizard_type: WizardType) {
    let context = match wizard_type {
        WizardType::Pathfinder => determine_pathfinder_context(&app.discover),
        WizardType::ParserLab => determine_parser_lab_context(&app.discover),
        WizardType::Labeling => determine_labeling_context(&app.discover),
        WizardType::SemanticPath => determine_semantic_path_context(&app.discover),
    };

    // Handle missing context
    if context.is_none() {
        let (message, recovery) = match wizard_type {
            WizardType::Pathfinder => ("Select a file first", Some(DiscoverFocus::Files)),
            WizardType::ParserLab => ("Select a file group", Some(DiscoverFocus::Files)),
            WizardType::Labeling => ("Select a file group", Some(DiscoverFocus::Files)),
            WizardType::SemanticPath => ("Select a source first", Some(DiscoverFocus::Sources)),
        };

        app.toast = Some(Toast::warning(message));

        if let Some(focus) = recovery {
            app.discover.focus = focus;
        }

        return;  // Do not open wizard
    }

    // Valid context - open wizard
    app.wizard_previous_focus = Some(app.discover.focus.clone());
    app.wizard_dialog = Some(WizardDialogState::new(wizard_type, context));
}
```

---

## 5. Resolution: Visual Feedback for Valid/Invalid Invocations

### 5.1 Keybinding Hints in Status Bar

The status bar shows available wizards based on current context:

```
┌─ Without valid context ──────────────────────────────────────────────────────┐
│ [1]Sources [2]Tags [3]Files [n]Rule [R]Rules [M]Manage                       │
│ Wizards: (select a file)                                                     │
└──────────────────────────────────────────────────────────────────────────────┘

┌─ With file selected ─────────────────────────────────────────────────────────┐
│ [1]Sources [2]Tags [3]Files [n]Rule [R]Rules [M]Manage                       │
│ Wizards: [W]menu [w]Pathfinder [g]Parser [l]Label [S]Semantic                │
└──────────────────────────────────────────────────────────────────────────────┘

┌─ With file selected but no signature group ──────────────────────────────────┐
│ [1]Sources [2]Tags [3]Files [n]Rule [R]Rules [M]Manage                       │
│ Wizards: [W]menu [w]Pathfinder (g/l need group) [S]Semantic                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Dimmed Keys for Invalid Invocations

In the Wizard Menu (`W`), unavailable options are dimmed:

```
┌─ WIZARDS ─────────────────────────┐
│                                    │
│  [p] Pathfinder (Extractor)        │   <- Normal (file selected)
│  [g] Parser Lab (Generator)        │   <- Dimmed (no group)
│  [l] Labeling (Semantic Tag)       │   <- Dimmed (no group)
│  [s] Semantic Path (Structure)     │   <- Normal (source selected)
│                                    │
│  [Esc] Cancel                      │
└────────────────────────────────────┘
```

Dimmed items show reason on hover/focus:

```
┌─ WIZARDS ─────────────────────────┐
│                                    │
│  [p] Pathfinder (Extractor)        │
│  ► [g] Parser Lab (Generator)      │   <- Focused
│      No signature group            │   <- Reason shown
│  [l] Labeling (Semantic Tag)       │
│  [s] Semantic Path (Structure)     │
│                                    │
│  [Esc] Cancel                      │
└────────────────────────────────────┘
```

### 5.3 Implementation

```rust
fn render_wizard_menu(state: &DiscoverState) -> Vec<MenuItem> {
    let pathfinder_ctx = determine_pathfinder_context(state);
    let parser_lab_ctx = determine_parser_lab_context(state);
    let labeling_ctx = determine_labeling_context(state);
    let semantic_ctx = determine_semantic_path_context(state);

    vec![
        MenuItem {
            key: 'p',
            label: "Pathfinder (Extractor)",
            enabled: pathfinder_ctx.is_some(),
            disabled_reason: if pathfinder_ctx.is_none() {
                Some("Select a file first")
            } else { None },
        },
        MenuItem {
            key: 'g',
            label: "Parser Lab (Generator)",
            enabled: parser_lab_ctx.is_some(),
            disabled_reason: if parser_lab_ctx.is_none() {
                Some("No signature group")
            } else { None },
        },
        MenuItem {
            key: 'l',
            label: "Labeling (Semantic Tag)",
            enabled: labeling_ctx.is_some(),
            disabled_reason: if labeling_ctx.is_none() {
                Some("No signature group")
            } else { None },
        },
        MenuItem {
            key: 's',
            label: "Semantic Path (Structure)",
            enabled: semantic_ctx.is_some(),
            disabled_reason: if semantic_ctx.is_none() {
                Some("Select a source first")
            } else { None },
        },
    ]
}
```

---

## 6. Resolution: Entry Point Inventory

### 6.1 Complete Entry Points by Wizard

**Pathfinder Wizard:**

| Entry Point | Key | Context | Location |
|-------------|-----|---------|----------|
| Wizard menu | `W` then `p` | Selected file | Any Discover state |
| Direct shortcut | `w` | Selected file | Files panel |
| Pending Review | `w` | Unmatched paths | Pending Review panel |
| Right-click (future) | Mouse | Clicked file | Files panel |
| Command palette (future) | `:pathfinder` | Selected file | Any state |

**Parser Lab Wizard:**

| Entry Point | Key | Context | Location |
|-------------|-----|---------|----------|
| Wizard menu | `W` then `g` | Signature group | Any Discover state |
| Direct shortcut | `g` | Signature group | Files panel |
| Pending Review | `g` | Group needing parser | Parser Warnings section |
| Right-click (future) | Mouse | Clicked file's group | Files panel |
| Command palette (future) | `:parser` | Selected file's group | Any state |

**Labeling Wizard:**

| Entry Point | Key | Context | Location |
|-------------|-----|---------|----------|
| Wizard menu | `W` then `l` | Signature group | Any Discover state |
| Direct shortcut | `l` | Signature group | Files panel |
| Pending Review | `l` | Unlabeled groups | Unlabeled Groups section |
| Right-click (future) | Mouse | Clicked file's group | Files panel |
| Command palette (future) | `:label` | Selected file's group | Any state |

**Semantic Path Wizard:**

| Entry Point | Key | Context | Location |
|-------------|-----|---------|----------|
| Wizard menu | `W` then `s` | Source | Any Discover state |
| Direct shortcut | `S` | Source | Any Discover state (global) |
| Pending Review | `S` | Unrecognized source | Unrecognized Sources section |
| Post-scan dialog | `Enter` | Newly scanned source | Scan complete dialog |
| Sources Manager | `S` | Selected source | Sources Manager dialog |
| Right-click (future) | Mouse | Clicked source | Sidebar |
| Command palette (future) | `:semantic` | Selected source | Any state |

### 6.2 Entry Point Precedence

When multiple entry points could apply, use this precedence:

1. **Pending Review panel** (if open) - uses panel's selected item
2. **Wizard Menu** (`W`) - uses current selection in Discover
3. **Direct shortcut** (`w`, `g`, `l`, `S`) - uses current selection
4. **Context menu** (future) - uses clicked item

### 6.3 Future Entry Points (Not Yet Implemented)

These are planned but not in v1.0:

| Entry Point | Description | Implementation Phase |
|-------------|-------------|----------------------|
| Right-click context menu | Mouse-driven wizard invocation | Phase 3 |
| Command palette | `:wizard <name>` | Phase 3 |
| MCP tool | `invoke_wizard` | Phase 2 |
| CLI | `casparian wizard pathfinder <path>` | Phase 2 |

---

## 7. State Machine Update

Add this to specs/ai_wizards.md Section 5.6.1:

```
                              ┌─────────────────────────────┐
                              │      DISCOVER_NORMAL        │
                              │  (Files/Sources/Tags focus) │
                              └──────────────┬──────────────┘
                                             │
              ┌──────────────────────────────┼──────────────────────────────┐
              │                              │                              │
              │ W                            │ w/g/l/S (direct)             │ ! (Pending Review)
              ▼                              │                              ▼
    ┌─────────────────┐                      │                    ┌─────────────────┐
    │  WIZARD_MENU    │──────────────────────┤                    │ PENDING_REVIEW  │
    │  (modal overlay)│   p/g/l/s            │                    │     (panel)     │
    └────────┬────────┘                      │                    └────────┬────────┘
             │                               │                             │ w/g/l/S
             │ (context check)               │ (context check)             │ (context check)
             ▼                               ▼                             ▼
    ┌────────────────────────────────────────────────────────────────────────────────┐
    │                           CONTEXT VALIDATION                                     │
    │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────────┐  │
    │  │ Valid ctx   │    │ Invalid ctx │    │ Valid ctx   │    │ Invalid ctx     │  │
    │  │ Open wizard │    │ Show toast  │    │ Open wizard │    │ Show toast      │  │
    │  │    ↓        │    │ Stay here   │    │    ↓        │    │ Focus recovery  │  │
    │  └──────┬──────┘    └─────────────┘    └──────┬──────┘    └─────────────────┘  │
    └─────────┼────────────────────────────────────┼─────────────────────────────────┘
              │                                     │
              └──────────────────┬──────────────────┘
                                 ▼
                      ┌─────────────────────┐
                      │   WIZARD_DIALOG     │
                      │ (Pathfinder/Parser/ │
                      │  Labeling/Semantic) │
                      └──────────┬──────────┘
                                 │ Approved / Canceled / Error
                                 ▼
                      ┌─────────────────────┐
                      │   DISCOVER_NORMAL   │
                      │ (focus restored)    │
                      └─────────────────────┘
```

---

## 8. Keybinding Updates

### 8.1 Full Keybinding Table for Wizards

| Key | Context | Guard | Action |
|-----|---------|-------|--------|
| `W` | Global (Discover) | None | Open Wizard menu (modal) |
| `w` | Files panel | File selected | Launch Pathfinder if context valid |
| `w` | Pending Review | Unmatched Paths focused | Launch Pathfinder for selected paths |
| `g` | Files panel | Signature group exists | Launch Parser Lab if context valid |
| `g` | Pending Review | Parser Warnings focused | Launch Parser Lab for selected group |
| `l` | Files panel | Signature group exists | Launch Labeling if context valid |
| `l` | Pending Review | Unlabeled Groups focused | Launch Labeling for selected group |
| `S` | Global (Discover) | Source selected | Launch Semantic Path if context valid |
| `S` | Pending Review | Unrecognized Sources focused | Launch Semantic Path for selected source |
| `S` | Sources Manager | Source selected | Launch Semantic Path for selected source |
| `p` | Wizard menu | Pathfinder enabled | Launch Pathfinder |
| `g` | Wizard menu | Parser Lab enabled | Launch Parser Lab |
| `l` | Wizard menu | Labeling enabled | Launch Labeling |
| `s` | Wizard menu | Semantic Path enabled | Launch Semantic Path |
| `Esc` | Wizard menu | None | Close menu, return to previous |
| `Esc` | Any wizard dialog | None | Cancel wizard, return to Discover |

### 8.2 Keybinding Conflicts Resolved

| Key | Default Action | Override Context | Override Action |
|-----|---------------|------------------|-----------------|
| `g` | (none in Discover) | Files panel | Launch Parser Lab |
| `l` | (none in Discover) | Files panel | Launch Labeling |
| `S` | (none in Discover) | Global Discover | Launch Semantic Path |
| `w` | (none in Discover) | Files panel | Launch Pathfinder |

No conflicts with existing Discover keybindings. All wizard keys are new additions.

---

## 9. Summary of Changes to Specs

### 9.1 Changes to specs/ai_wizards.md

**Section 5.6** - Add new subsections:

- 5.6.2 Context Determination (this document Section 2)
- 5.6.3 Focus Management (this document Section 3)
- 5.6.4 Error Handling (this document Section 4)
- 5.6.5 Visual Feedback (this document Section 5)
- 5.6.6 Entry Point Inventory (this document Section 6)

**Section 5.6.1** - Update state machine (this document Section 7)

### 9.2 Changes to specs/views/discover.md

**Section 6.4** - Update Files Panel keybindings to reference wizard context rules

Add note:
> **Wizard Invocation:** Keys `w`, `g`, `l` invoke wizards when context is valid.
> See specs/ai_wizards.md Section 5.6 for context determination rules.

**Section 8.9.4** - Add cross-reference to wizard invocation from Pending Review

---

## 10. Examples

### 10.1 Example: Successful Pathfinder Invocation

```
User state:
  - Files panel focused
  - File selected: /data/ADT_Inbound/2024/01/msg_001.hl7
  - Filter: (empty)

User presses: w

Context check:
  - File selected: YES
  - Path available: YES
  - Context: SingleFile { path: "/data/ADT_Inbound/2024/01/msg_001.hl7" }

Result:
  - Pathfinder wizard opens
  - Wizard analyzes path "/data/ADT_Inbound/2024/01/msg_001.hl7"
  - Shows detected patterns: direction=Inbound, year=2024, month=01
```

### 10.2 Example: Failed Parser Lab Invocation

```
User state:
  - Files panel focused
  - File selected: /data/random_file.txt (no signature group)
  - Filter: (empty)

User presses: g

Context check:
  - File selected: YES
  - Signature group: NO (file.signature_group_id is None)
  - Context: None

Result:
  - Toast appears: "Select a file group"
  - Wizard does NOT open
  - Focus remains on Files panel
```

### 10.3 Example: Wizard Menu with Partial Context

```
User state:
  - Files panel focused
  - File selected: /data/sales/jan.csv (in signature group "abc123")
  - Source selected: sales_data

User presses: W (Wizard menu)

Context checks:
  - Pathfinder: YES (file selected)
  - Parser Lab: YES (signature group exists)
  - Labeling: YES (signature group exists)
  - Semantic Path: YES (source selected)

Wizard menu shows:
  [p] Pathfinder (Extractor)      <- enabled
  [g] Parser Lab (Generator)      <- enabled
  [l] Labeling (Semantic Tag)     <- enabled
  [s] Semantic Path (Structure)   <- enabled
```

### 10.4 Example: Pending Review Entry Point

```
User state:
  - Pending Review panel open (!)
  - Unmatched Paths section focused
  - 23 unmatched files selected

User presses: w

Context check:
  - In Pending Review: YES
  - Category: Unmatched Paths
  - Items selected: 23 file paths
  - Context: MultipleFiles { paths: [...] }

Result:
  - Pathfinder wizard opens
  - Wizard analyzes all 23 paths
  - Clusters paths by similarity
  - Proposes extraction rules for each cluster
```

---

## Appendix A: Data Structures

```rust
/// Context for Pathfinder wizard
#[derive(Debug, Clone)]
pub enum PathfinderContext {
    None,
    SingleFile { path: String, rel_path: String },
    MultipleFiles { paths: Vec<String> },
}

/// Context for Parser Lab wizard
#[derive(Debug, Clone)]
pub enum ParserLabContext {
    None,
    SingleFile { path: String },
    SignatureGroup {
        group_id: String,
        files: Vec<FileInfo>,
        fingerprint: Option<String>,
    },
}

/// Context for Labeling wizard
#[derive(Debug, Clone)]
pub enum LabelingContext {
    None,
    SignatureGroup {
        group_id: String,
        headers: Vec<String>,
        sample_values: HashMap<String, Vec<String>>,
        file_count: usize,
    },
}

/// Context for Semantic Path wizard
#[derive(Debug, Clone)]
pub enum SemanticPathContext {
    None,
    Source {
        source_id: i64,
        source_name: String,
        source_path: String,
        sample_paths: Vec<String>,
    },
}

/// Wizard outcome for focus restoration
#[derive(Debug, Clone)]
pub enum WizardOutcome {
    Approved { artifact_type: ArtifactType, artifact_id: String },
    Canceled,
    Error,
}

/// Type of artifact created by wizard
#[derive(Debug, Clone)]
pub enum ArtifactType {
    ExtractionRule,
    Parser,
    Label,
    SemanticRule,
}
```

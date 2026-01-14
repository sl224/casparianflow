# Engineer Response: Round 003

**Date:** 2026-01-13
**Focus:** 4 HIGH priority gaps (GAP-FIELD-001, GAP-TEST-001, GAP-DATA-001, GAP-NAV-001)
**Engineer Role:** Propose concrete, implementable solutions

---

## Gap Resolution: GAP-FIELD-001

**Field inference input unclear**

**Confidence:** HIGH

### Problem Statement

Phase 18c specifies `infer_fields_from_pattern(pattern: &str, sample_paths: &[&str])` but does not specify:
1. Where `sample_paths` come from
2. How many samples to use
3. What happens when pattern matches 100K+ files

### Proposed Solution

**Source of sample_paths:** Files matching the current glob pattern from the folder cache.

**Sampling strategy:** Stratified random sampling with hard limits:

```rust
pub struct FieldInferenceConfig {
    /// Maximum samples to analyze (performance bound)
    pub max_samples: usize,           // Default: 100
    /// Minimum samples needed for reliable inference
    pub min_samples: usize,           // Default: 3
    /// Sampling strategy
    pub strategy: SamplingStrategy,
}

pub enum SamplingStrategy {
    /// Take first N matches (fast, may miss edge cases)
    FirstN,
    /// Random sample across matches (better coverage)
    Random,
    /// Stratified by segment values (best coverage)
    Stratified,
}
```

**Implementation:**

```rust
/// Gather samples for field inference from matching files
pub fn gather_inference_samples(
    pattern: &str,
    cache: &FolderCache,
    config: &FieldInferenceConfig,
) -> Vec<PathBuf> {
    let matches: Vec<_> = cache.files_matching(pattern).collect();
    let total = matches.len();

    if total <= config.max_samples {
        // Small dataset: use all
        return matches;
    }

    match config.strategy {
        SamplingStrategy::FirstN => {
            matches.into_iter().take(config.max_samples).collect()
        }
        SamplingStrategy::Random => {
            // Fisher-Yates shuffle, take first max_samples
            let mut rng = rand::thread_rng();
            let mut sampled = matches;
            sampled.shuffle(&mut rng);
            sampled.truncate(config.max_samples);
            sampled
        }
        SamplingStrategy::Stratified => {
            // Group by first variable segment, sample proportionally
            stratified_sample(matches, pattern, config.max_samples)
        }
    }
}
```

**Default behavior:**
- Use `SamplingStrategy::Stratified` by default for better edge case coverage
- Maximum 100 samples for real-time UI responsiveness (<50ms inference time)
- Minimum 3 samples required; show warning if fewer matches exist

**UI feedback when sampling:**

```
INFERRED FIELDS (from 100 of 47,293 files):
  mission_id (high) - 23 unique values in sample
  date (high) - ISO date format detected

  [ ] Show all 47,293 matches   [Sampling: stratified]
```

### Examples

**Case 1: Small dataset (< 100 files)**
```
Pattern: *.csv
Matches: 47 files
Sampling: None (use all 47)
Inference: Full coverage, maximum confidence
```

**Case 2: Large dataset (100K files)**
```
Pattern: **/*.log
Matches: 127,493 files
Sampling: Stratified, 100 samples from across directory tree
Inference: Representative sample, show "(sampled)" indicator
```

**Case 3: Very large dataset with skewed distribution**
```
Pattern: **/mission_*/data/*.csv
Matches: 1.2M files (90% from mission_001)
Sampling: Stratified by mission_id, 100 samples spread across missions
Inference: Avoids bias toward dominant mission
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Hard limit (100) | Guaranteed fast UI | May miss rare edge cases |
| Stratified sampling | Better coverage | Slightly slower than FirstN |
| Show sample count | User knows coverage | Extra UI complexity |

### New Gaps Introduced

- None (self-contained solution)

---

## Gap Resolution: GAP-TEST-001

**Test execution model unclear**

**Confidence:** HIGH

### Problem Statement

Phase 18d defines `TestState` with `TestPhase::Running` but does not specify:
1. Whether test runs synchronously (blocking UI) or asynchronously (background)
2. Threshold for async execution
3. How progress is shown
4. Whether user can cancel a running test

### Proposed Solution

**Execution model:** Always asynchronous with cancellation support.

**Rationale:** Even 100 files with regex extraction can take 500ms+, causing perceptible UI freeze. Always-async is simpler than conditional.

```rust
pub struct TestState {
    pub rule: RuleDraft,
    pub phase: TestPhase,
    pub results: Option<TestResults>,
    pub selected_category: TestCategory,
    pub scroll_offset: usize,
    /// Cancellation token for running test
    pub cancel_token: Option<Arc<AtomicBool>>,
}

pub enum TestPhase {
    /// Test running in background
    Running {
        files_processed: usize,
        files_total: usize,
        current_file: Option<String>,  // Currently processing
        started_at: Instant,
    },
    /// Test completed successfully
    Complete,
    /// Test was cancelled by user
    Cancelled { files_processed: usize },
    /// Test encountered fatal error
    Error(String),
}
```

**Background task architecture:**

```rust
impl TestState {
    pub fn start_test(
        &mut self,
        rule: &RuleDraft,
        matching_files: Vec<PathBuf>,
        tx: mpsc::Sender<TestProgress>,
    ) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_token = Some(Arc::clone(&cancel));

        let rule_clone = rule.clone();
        tokio::spawn(async move {
            for (i, path) in matching_files.iter().enumerate() {
                // Check cancellation every file
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(TestProgress::Cancelled { processed: i });
                    return;
                }

                // Send progress update
                let _ = tx.send(TestProgress::Processing {
                    current: i,
                    total: matching_files.len(),
                    path: path.display().to_string(),
                });

                // Run extraction (CPU-bound, use spawn_blocking)
                let result = tokio::task::spawn_blocking(move || {
                    extract_fields(&rule_clone, &path)
                }).await;

                // Collect result...
            }

            let _ = tx.send(TestProgress::Complete(results));
        });
    }

    pub fn cancel(&mut self) {
        if let Some(token) = &self.cancel_token {
            token.store(true, Ordering::Relaxed);
        }
    }
}
```

**Progress display (integrated into TestState render):**

```
+------------------[ TEST RESULTS ]------------------+
|                                                    |
|  Testing rule: csv_data                            |
|                                                    |
|  Progress: [=============>          ] 67%          |
|  Files:    1,247 / 1,859                           |
|  Current:  /data/mission_042/2024-01-15/sensor.csv |
|  Elapsed:  3.2s                                    |
|                                                    |
|  [Esc] Cancel test                                 |
+----------------------------------------------------+
```

**Cancellation UX:**

```
User presses Esc during Running phase:
  -> cancel_token set to true
  -> Background task exits at next file boundary
  -> Phase transitions to Cancelled { files_processed }
  -> UI shows partial results with "(cancelled)" indicator
  -> User can press 'e' to edit rule or 't' to restart test
```

**Post-completion flow:**

```
+------------------[ TEST RESULTS ]------------------+
|                                                    |
|  Rule: csv_data                 Completed in 4.7s  |
|                                                    |
|  Summary:           Field Metrics:                 |
|  - Complete: 1,742   mission_id: 23 unique        |
|  - Partial:    89    date: 2023-01 to 2024-03     |
|  - Failed:     28    category: 4 values           |
|                                                    |
|  [Tab] Cycle views  [p] Publish  [e] Edit  [Esc]   |
+----------------------------------------------------+
```

### Examples

**Example 1: Fast test (< 100 files)**
```
Files: 47
Time: 0.3s
UX: Progress bar flashes briefly, results appear almost instantly
Cancellation: Possible but unlikely needed
```

**Example 2: Medium test (1K files)**
```
Files: 1,247
Time: ~4s
UX: Progress bar updates smoothly, current file shown
Cancellation: User can Esc if wrong pattern detected early
```

**Example 3: Large test with cancellation**
```
Files: 50,000
Time: ~180s estimated
UX: User sees progress at 5%, realizes pattern is wrong
Action: Presses Esc
Result: Cancelled at 2,500 files, partial results shown
Transition: Esc -> EditRule (fix pattern, re-test)
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Always async | Consistent UX, no UI freeze | Slightly more complex impl |
| Per-file cancellation | Responsive cancel | Overhead for each check |
| Partial results on cancel | User sees what worked | May not represent full dataset |

### New Gaps Introduced

- None

---

## Gap Resolution: GAP-DATA-001

**RuleDraft vs extraction.md schema mismatch**

**Confidence:** HIGH

### Problem Statement

Phase 18 defines `RuleDraft` in Rust:
```rust
pub struct RuleDraft {
    pub name: String,
    pub glob_pattern: String,
    pub fields: Vec<FieldDraft>,
    pub base_tag: String,
    pub tag_conditions: Vec<TagCondition>,
}
```

But `extraction.md` Section 3.1 defines different YAML schema:
```yaml
rules:
  - name: "Mission Telemetry"
    glob: "**/mission_*/????-??-??/*.csv"
    extract:
      mission_id:
        from: segment(-3)
        pattern: "mission_(\\d+)"
        type: integer
    tag: mission_data
    priority: 100
```

**Key mismatches:**
1. `glob_pattern` vs `glob`
2. `fields: Vec<FieldDraft>` vs `extract: HashMap<String, FieldDef>`
3. Missing `priority` in RuleDraft
4. `base_tag` vs `tag`
5. Field definitions are structurally different
6. `FieldSource` enum vs string `from` with magic values

### Proposed Solution

**Authoritative schema:** Database schema (extraction.md Section 6) is authoritative because:
1. It's the persistence layer
2. CLI and TUI must both read/write to it
3. YAML is export format, not internal representation

**Unified Rust types aligned with DB schema:**

```rust
/// TUI working draft - editable in UI
#[derive(Debug, Clone)]
pub struct RuleDraft {
    pub id: Option<Uuid>,         // None for new rules, Some for editing existing
    pub source_id: Option<Uuid>,  // Scoped to source, or None for global
    pub name: String,
    pub glob_pattern: String,
    pub fields: Vec<FieldDraft>,
    pub base_tag: Option<String>, // Optional base tag
    pub tag_conditions: Vec<TagConditionDraft>,
    pub priority: i32,            // Default: 100
    pub enabled: bool,            // Default: true
}

#[derive(Debug, Clone)]
pub struct FieldDraft {
    pub name: String,
    pub source: FieldSource,
    pub pattern: Option<String>,  // Regex for extraction
    pub type_hint: FieldType,
    pub normalizer: Option<Normalizer>,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldSource {
    Segment(i32),    // segment(-2) -> Segment(-2)
    Filename,        // "filename"
    FullPath,        // "full_path"
    RelPath,         // "rel_path"
}

impl FieldSource {
    /// Convert to DB string format
    pub fn to_db_format(&self) -> (String, Option<String>) {
        match self {
            FieldSource::Segment(n) => ("segment".to_string(), Some(n.to_string())),
            FieldSource::Filename => ("filename".to_string(), None),
            FieldSource::FullPath => ("full_path".to_string(), None),
            FieldSource::RelPath => ("rel_path".to_string(), None),
        }
    }

    /// Parse from DB format
    pub fn from_db_format(source_type: &str, source_value: Option<&str>) -> Result<Self> {
        match source_type {
            "segment" => {
                let n: i32 = source_value
                    .ok_or(Error::MissingSourceValue)?
                    .parse()?;
                Ok(FieldSource::Segment(n))
            }
            "filename" => Ok(FieldSource::Filename),
            "full_path" => Ok(FieldSource::FullPath),
            "rel_path" => Ok(FieldSource::RelPath),
            _ => Err(Error::UnknownSourceType(source_type.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Integer,
    Date,
    Uuid,
}

impl FieldType {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Date => "date",
            FieldType::Uuid => "uuid",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "integer" => FieldType::Integer,
            "date" => FieldType::Date,
            "uuid" => FieldType::Uuid,
            _ => FieldType::String,  // Default
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Normalizer {
    Lowercase,
    Uppercase,
    StripLeadingZeros,
}

#[derive(Debug, Clone)]
pub struct TagConditionDraft {
    pub field: String,
    pub operator: CompareOp,
    pub value: String,
    pub tag: String,
    pub priority: i32,  // Default: 100
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Eq,        // =
    NotEq,     // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    Contains,  // contains
    Matches,   // matches (regex)
}

impl CompareOp {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            CompareOp::Eq => "=",
            CompareOp::NotEq => "!=",
            CompareOp::Lt => "<",
            CompareOp::Gt => ">",
            CompareOp::LtEq => "<=",
            CompareOp::GtEq => ">=",
            CompareOp::Contains => "contains",
            CompareOp::Matches => "matches",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "=" => CompareOp::Eq,
            "!=" => CompareOp::NotEq,
            "<" => CompareOp::Lt,
            ">" => CompareOp::Gt,
            "<=" => CompareOp::LtEq,
            ">=" => CompareOp::GtEq,
            "contains" => CompareOp::Contains,
            "matches" => CompareOp::Matches,
            _ => CompareOp::Eq,  // Default
        }
    }
}
```

**Mapping to database tables:**

```rust
impl RuleDraft {
    /// Persist to database
    pub async fn save(&self, db: &Database) -> Result<Uuid> {
        let rule_id = self.id.unwrap_or_else(Uuid::new_v4);

        // Insert/update extraction_rules
        sqlx::query!(
            r#"
            INSERT INTO extraction_rules (id, source_id, name, glob_pattern, tag, priority, enabled, created_by, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, 'tui', datetime('now'))
            ON CONFLICT(source_id, name) DO UPDATE SET
                glob_pattern = excluded.glob_pattern,
                tag = excluded.tag,
                priority = excluded.priority,
                enabled = excluded.enabled
            "#,
            rule_id.to_string(),
            self.source_id.map(|id| id.to_string()),
            self.name,
            self.glob_pattern,
            self.base_tag,
            self.priority,
            self.enabled,
        ).execute(db).await?;

        // Delete existing fields, insert new
        sqlx::query!("DELETE FROM extraction_fields WHERE rule_id = ?", rule_id.to_string())
            .execute(db).await?;

        for field in &self.fields {
            let (source_type, source_value) = field.source.to_db_format();
            let field_id = Uuid::new_v4();

            sqlx::query!(
                r#"
                INSERT INTO extraction_fields (id, rule_id, field_name, source_type, source_value, pattern, type_hint)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                field_id.to_string(),
                rule_id.to_string(),
                field.name,
                source_type,
                source_value,
                field.pattern,
                field.type_hint.to_db_string(),
            ).execute(db).await?;
        }

        // Similar for tag_conditions...

        Ok(rule_id)
    }

    /// Load from database
    pub async fn load(db: &Database, rule_id: Uuid) -> Result<Self> {
        let rule_row = sqlx::query!(
            "SELECT * FROM extraction_rules WHERE id = ?",
            rule_id.to_string()
        ).fetch_one(db).await?;

        let field_rows = sqlx::query!(
            "SELECT * FROM extraction_fields WHERE rule_id = ?",
            rule_id.to_string()
        ).fetch_all(db).await?;

        let fields = field_rows.into_iter().map(|row| {
            FieldDraft {
                name: row.field_name,
                source: FieldSource::from_db_format(&row.source_type, row.source_value.as_deref())?,
                pattern: row.pattern,
                type_hint: FieldType::from_db_string(&row.type_hint.unwrap_or_default()),
                normalizer: None,  // TODO: add to schema
                default_value: None,  // TODO: add to schema
            }
        }).collect::<Result<Vec<_>>>()?;

        Ok(RuleDraft {
            id: Some(rule_id),
            source_id: rule_row.source_id.map(|s| Uuid::parse_str(&s)).transpose()?,
            name: rule_row.name,
            glob_pattern: rule_row.glob_pattern,
            fields,
            base_tag: rule_row.tag,
            tag_conditions: vec![],  // Load separately
            priority: rule_row.priority.unwrap_or(100),
            enabled: rule_row.enabled.unwrap_or(true),
        })
    }
}
```

**YAML import/export mapping:**

```rust
/// YAML format for import/export (matches extraction.md Section 3.1)
#[derive(Serialize, Deserialize)]
pub struct RuleYaml {
    pub name: String,
    pub glob: String,
    pub extract: Option<HashMap<String, FieldYaml>>,
    pub tag: Option<String>,
    pub tag_conditions: Option<Vec<TagConditionYaml>>,
    pub priority: Option<i32>,
}

#[derive(Serialize, Deserialize)]
pub struct FieldYaml {
    pub from: String,           // "segment(-2)", "filename", "full_path"
    pub pattern: Option<String>,
    #[serde(rename = "type")]
    pub type_hint: Option<String>,
    pub normalize: Option<String>,
    pub default: Option<String>,
}

impl From<RuleYaml> for RuleDraft {
    fn from(yaml: RuleYaml) -> Self {
        let fields = yaml.extract.unwrap_or_default().into_iter().map(|(name, def)| {
            FieldDraft {
                name,
                source: parse_from_string(&def.from),  // "segment(-2)" -> Segment(-2)
                pattern: def.pattern,
                type_hint: FieldType::from_db_string(&def.type_hint.unwrap_or_default()),
                normalizer: def.normalize.map(|s| parse_normalizer(&s)),
                default_value: def.default,
            }
        }).collect();

        RuleDraft {
            id: None,
            source_id: None,
            name: yaml.name,
            glob_pattern: yaml.glob,
            fields,
            base_tag: yaml.tag,
            tag_conditions: yaml.tag_conditions.unwrap_or_default().into_iter()
                .map(Into::into).collect(),
            priority: yaml.priority.unwrap_or(100),
            enabled: true,
        }
    }
}

fn parse_from_string(s: &str) -> FieldSource {
    if s.starts_with("segment(") && s.ends_with(")") {
        let inner = &s[8..s.len()-1];
        if let Ok(n) = inner.parse::<i32>() {
            return FieldSource::Segment(n);
        }
    }
    match s {
        "filename" => FieldSource::Filename,
        "full_path" => FieldSource::FullPath,
        "rel_path" => FieldSource::RelPath,
        _ => FieldSource::FullPath,  // Default
    }
}
```

### Examples

**Example 1: Create rule in TUI, export to YAML**
```
TUI RuleDraft:
  name: "mission_data"
  glob_pattern: "**/mission_*/????-??-??/*.csv"
  fields: [
    FieldDraft { name: "mission_id", source: Segment(-3), pattern: Some("mission_(\\d+)"), type_hint: Integer }
    FieldDraft { name: "date", source: Segment(-2), pattern: None, type_hint: Date }
  ]
  base_tag: Some("mission_data")
  priority: 100

Exported YAML:
  name: "mission_data"
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

**Example 2: Import YAML to TUI**
```
YAML input (from file):
  name: "Healthcare ADT"
  glob: "**/*_Inbound/*"
  extract:
    direction:
      from: full_path
      pattern: "_(Inbound|Outbound)/"
      normalize: lowercase
  tag: adt_messages

RuleDraft created:
  name: "Healthcare ADT"
  glob_pattern: "**/*_Inbound/*"
  fields: [FieldDraft { name: "direction", source: FullPath, ... }]
  base_tag: Some("adt_messages")
  priority: 100  (default)
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| DB as authoritative | Single source of truth | Must keep all layers in sync |
| Enum-based FieldSource | Type safety | More boilerplate than strings |
| YAML compat layer | Seamless import/export | Maintenance overhead |

### New Gaps Introduced

- **GAP-SCHEMA-001 (LOW):** `normalizer` and `default_value` in extraction_fields table missing from Phase 18f schema. Need to add columns to match extraction.md Section 3.2.

---

## Gap Resolution: GAP-NAV-001

**Return path from Published unclear**

**Confidence:** HIGH

### Problem Statement

After publish completes, user presses Enter. Where do they go?
- Return to Browse at root?
- Return to the prefix where they started?
- Return to Filtering with same pattern?

### Proposed Solution

**Return destination:** Browse at root (clean slate).

**Rationale:**
1. The rule has been published - the work is complete
2. The pattern that led to the rule is now encoded IN the rule itself
3. Returning to the same filter would show the same files, but they now have a rule
4. Starting fresh aligns with the "publish, done, what's next?" mental model
5. User can always use `R` (Rules Manager) to view/edit published rules

**State transition table (clarified):**

| From State | Trigger | To State | Context Preserved |
|------------|---------|----------|-------------------|
| Published | Enter | Browse (root) | None - clean slate |
| Published | Esc | Browse (root) | None - clean slate |
| Published | `j` | Job Status | job_id passed to Jobs view |

**Implementation:**

```rust
impl GlobExplorerApp {
    fn handle_published_key(&mut self, key: KeyEvent) -> AppResult<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                // Return to Browse at root - clean slate
                self.phase = GlobExplorerPhase::Browse;
                self.current_prefix = PathBuf::from("/");
                self.pattern = String::new();
                self.selected_index = 0;
                self.scroll_offset = 0;
                // Clear rule editing state
                self.rule_editor = None;
                self.test_state = None;
                self.publish_state = None;
                // Refresh folder view
                self.refresh_folders()?;
            }
            KeyCode::Char('j') => {
                if let Some(ref publish_state) = self.publish_state {
                    if let Some(job_id) = &publish_state.job_id {
                        // Transition to Jobs view with this job selected
                        self.pending_navigation = Some(Navigation::ViewJob(job_id.clone()));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
```

**Visual confirmation (Published state UI):**

```
+------------------[ PUBLISHED ]------------------+
|                                                 |
|  Rule created successfully!                     |
|                                                 |
|  Rule:       csv_data                           |
|  Matches:    1,247 files                        |
|  Tag:        mission_data                       |
|                                                 |
|  Background job started:                        |
|  Job ID:     abc-123-def                        |
|  Status:     Running (extracting metadata)      |
|                                                 |
|  [Enter] Done   [j] View job   [Esc] Done       |
+-------------------------------------------------+
```

**Alternative considered and rejected:**

*Return to Filtering with pattern:*
- Pro: User could immediately filter by another pattern
- Con: Confusing - shows same files but now they have a rule
- Con: Pattern is somewhat arbitrary (might have been refined in EditRule)
- Con: Breaks "publish = done" mental model

*Return to prefix (not root):*
- Pro: Stays "where user was"
- Con: Prefix might be arbitrary (could have drilled deep during pattern exploration)
- Con: More state to preserve
- Decision: Root is simpler, user can navigate back if needed

### Examples

**Example 1: Normal publish flow**
```
Browse (root) -> "/" -> Filtering ("**/*.csv", 847 matches)
  -> "l" drill to /data
  -> Filtering (/data, "**/*.csv", 234 matches)
  -> "e" -> EditRule
  -> "t" -> Testing (complete)
  -> "p" -> Publishing (Confirming)
  -> Enter -> Publishing (Saving... Starting...)
  -> Published (Job ID: abc123)
  -> Enter
  -> Browse (root, clean slate)  <-- FRESH START
```

**Example 2: View job after publish**
```
Published (Job ID: abc123)
  -> "j"
  -> Jobs View (abc123 selected, showing progress)
```

**Example 3: Multiple rule creation session**
```
Browse -> filter -> edit -> test -> publish -> Published
  -> Enter
  -> Browse (root)  <-- Fresh, ready for next rule
  -> "/" -> filter for different pattern
  -> ...repeat...
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Root return | Simple, clear mental model | User must re-navigate if continuing in same area |
| Clean slate | No stale state issues | Loses navigation context |
| `j` shortcut | Quick access to job | Extra key to remember |

### New Gaps Introduced

- None

---

## Summary

| Gap ID | Resolution | Confidence | New Gaps |
|--------|------------|------------|----------|
| GAP-FIELD-001 | Stratified sampling, max 100, from pattern matches | HIGH | None |
| GAP-TEST-001 | Always async, cancellable, per-file progress | HIGH | None |
| GAP-DATA-001 | DB authoritative, RuleDraft aligned, YAML compat layer | HIGH | GAP-SCHEMA-001 (LOW) |
| GAP-NAV-001 | Return to Browse at root (clean slate) | HIGH | None |

## Next Steps

1. **Reviewer should validate:**
   - Sampling strategy reasonableness (100 samples sufficient?)
   - Async test architecture (tokio spawn_blocking correct?)
   - DB schema alignment completeness
   - Root return UX decision

2. **If approved, update:**
   - `specs/views/discover.md` Phase 18 with these specifications
   - Add `FieldType`, `CompareOp`, `Normalizer` enum definitions to data model section
   - Add sampling config to Phase 18c
   - Add async test architecture to Phase 18d
   - Add GAP-SCHEMA-001 to status.md gap inventory (LOW priority)

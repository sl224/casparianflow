# Engineer Response: Round 004

**Date:** 2026-01-13
**Focus:** 4 MEDIUM priority gaps (GAP-UI-001, GAP-INFER-001, GAP-HIST-001, GAP-ERR-001)
**Engineer Role:** Propose concrete, implementable solutions

---

## Gap Resolution: GAP-UI-001

**EDIT RULE layout undefined**

**Confidence:** HIGH

### Problem Statement

Phase 18b describes `RuleEditorState` struct with `RuleEditorFocus` enum (GlobPattern, FieldList, BaseTag, Conditions), but Section 13.8 provides an ASCII layout that does not precisely match these focus sections. We need a definitive ASCII layout that:
1. Shows all four focus sections clearly
2. Indicates which section has focus via visual highlighting
3. Shows status bar hints and keybindings appropriate for each section
4. Matches the `RuleEditorFocus` enum exactly

### Proposed Solution

**Definitive EDIT RULE ASCII Layout:**

```
+====================[ EDIT RULE: Mission Telemetry ]====================+
|                                                                         |
|  +-- GLOB PATTERN (1/4) -------- [Tab] next section -------------------+|
|  |>> **/mission_*/**/*.csv                                      [847] ||
|  +--------------------------------------------------------------------+|
|                                                                         |
|  +-- FIELDS (2/4) ------------------------------------------------+    |
|  |  mission_id                                                    |    |
|  |    source: segment(-3)                                         |    |
|  |    pattern: mission_(\d+)                                      |    |
|  |    type: integer                                               |    |
|  |                                                                |    |
|  |  date                                                          |    |
|  |    source: segment(-2)                                         |    |
|  |    type: date                                                  |    |
|  |                                                                |    |
|  |  [a] Add   [d] Delete   [j/k] Navigate   [Enter] Edit          |    |
|  +----------------------------------------------------------------+    |
|                                                                         |
|  +-- BASE TAG (3/4) ----------------------------------------------+    |
|  |  mission_data                                                  |    |
|  +----------------------------------------------------------------+    |
|                                                                         |
|  +-- CONDITIONS (4/4) --------------------------------------------+    |
|  |  IF mission_id < 100 THEN tag = "legacy_missions"              |    |
|  |  IF date.year = 2024 THEN tag = "current_year"                 |    |
|  |                                                                |    |
|  |  [a] Add condition   [d] Delete   [j/k] Navigate               |    |
|  +----------------------------------------------------------------+    |
|                                                                         |
+==========================================================================+
| [Tab] Next section   [t] Test rule   [Esc] Cancel                       |
+==========================================================================+
```

**Visual Focus Indicators:**

The currently focused section uses visual highlighting:

```
UNFOCUSED section:
  +-- GLOB PATTERN (1/4) -------------------------------------------+
  |  **/mission_*/**/*.csv                                    [847] |
  +----------------------------------------------------------------+

FOCUSED section:
  +== GLOB PATTERN (1/4) ======== [Tab] next section ==============+
  |>> **/mission_*/**/*.csv                                   [847] |
  +================================================================+

Legend:
  +== ... ==+ : Double-line border (focused section)
  +-- ... --+ : Single-line border (unfocused section)
  >>         : Cursor/selection indicator
  [847]      : Live match count (always visible in GLOB section)
```

**Section-Specific Keybindings:**

| Focus Section | Key | Action |
|---------------|-----|--------|
| **GlobPattern** | Any char | Append to pattern |
| | Backspace | Delete last char |
| | Enter | Confirm pattern, move to Fields |
| **FieldList** | j/k | Navigate field list |
| | Enter | Edit selected field |
| | a | Add new field |
| | d | Delete selected field |
| | i | Infer fields from pattern |
| **BaseTag** | Any char | Edit tag name |
| | Backspace | Delete last char |
| | Enter | Confirm tag |
| **Conditions** | j/k | Navigate condition list |
| | Enter | Edit selected condition |
| | a | Add new condition |
| | d | Delete selected condition |

**Global Keybindings (available in all focus sections):**

| Key | Action |
|-----|--------|
| Tab | Move to next section (wraps: Glob -> Fields -> Tag -> Conditions -> Glob) |
| Shift+Tab | Move to previous section |
| t | Test rule (transition to Testing state) |
| Esc | Cancel and return to Filtering |

**Status Bar Hints by Focus:**

```
GlobPattern focused:
  "[Tab] Fields | Type glob pattern | [t] Test | [Esc] Cancel"

FieldList focused:
  "[Tab] Tag | [a] Add | [d] Delete | [Enter] Edit | [i] Infer | [Esc] Cancel"

BaseTag focused:
  "[Tab] Conditions | Type tag name | [t] Test | [Esc] Cancel"

Conditions focused:
  "[Tab] Glob | [a] Add | [d] Delete | [Enter] Edit | [Esc] Cancel"
```

**Field Edit Sub-Focus:**

When editing a field (Enter on FieldList), show inline editing:

```
+== FIELDS (2/4) ===================================================+
|  mission_id                                                        |
|    source: segment(-3)                                             |
|    pattern: mission_(\d+)                                          |
|    type: integer                                                   |
|                                                                    |
|  date  [EDITING]                                                   |
|    source: >> segment(-2)  <<   [1] segment [2] filename [3] path  |
|    pattern: ____________________                                   |
|    type:    date                [s]tring [i]nteger [d]ate [u]uid   |
|                                                                    |
|  [Enter] Save   [Esc] Cancel                                       |
+====================================================================+
```

### Examples

**Example 1: Fresh rule creation**
```
User presses 'e' in Filtering state with pattern "**/*.csv"
-> EditRule state
-> GlobPattern focus (default)
-> Pattern pre-filled: "**/*.csv"
-> Fields: empty (or auto-inferred if enabled)
-> Base tag: empty
-> Conditions: empty
```

**Example 2: Navigating sections**
```
User is in GlobPattern focus
-> Tab
-> Focus moves to FieldList
-> Status bar updates: "[Tab] Tag | [a] Add..."
-> Tab
-> Focus moves to BaseTag
-> Tab
-> Focus moves to Conditions
-> Tab
-> Focus wraps to GlobPattern
```

**Example 3: Editing a field**
```
User in FieldList focus, selects "date" field
-> Enter
-> Focus changes to FieldEdit(Source)
-> j/k cycles through source options
-> Tab moves to FieldEdit(Pattern)
-> Tab moves to FieldEdit(Type)
-> Enter saves changes, returns to FieldList
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Section numbers (1/4, 2/4...) | Clear progress indicator | Extra UI clutter |
| Double-border for focus | Clear visual differentiation | May not render well in all terminals |
| Inline field editing | No modal popup | More complex state management |

### New Gaps Introduced

- None

---

## Gap Resolution: GAP-INFER-001

**Inference confidence thresholds undefined**

**Confidence:** HIGH

### Problem Statement

Phase 18c mentions HIGH/MEDIUM/LOW confidence indicators for inferred fields, but does not specify:
1. What score corresponds to each level?
2. How is confidence calculated?
3. What visual indicator is used for each level?

### Proposed Solution

**Confidence Levels and Thresholds:**

| Level | Score Range | Visual | Description |
|-------|-------------|--------|-------------|
| HIGH | >= 0.85 | Green checkmark | High certainty inference |
| MEDIUM | 0.50 - 0.84 | Yellow question mark | Probable but verify |
| LOW | < 0.50 | Gray question mark | Uncertain, may be wrong |

**Confidence Calculation Formula:**

Confidence is calculated from multiple factors, each contributing to the final score:

```rust
pub struct InferenceConfidence {
    pub score: f64,           // 0.0 - 1.0
    pub level: ConfidenceLevel,
    pub factors: Vec<ConfidenceFactor>,
}

pub enum ConfidenceLevel {
    High,    // >= 0.85
    Medium,  // 0.50 - 0.84
    Low,     // < 0.50
}

pub enum ConfidenceFactor {
    /// Pattern segment produces consistent type across samples
    TypeConsistency { ratio: f64 },  // % of samples with same type

    /// Named pattern detected (e.g., mission_*, date_*)
    PatternRecognition { pattern: String },  // Bonus for recognized patterns

    /// Value distribution suggests categorical vs continuous
    ValueDistribution { unique_ratio: f64 },  // unique_values / total_samples

    /// Sample size adequacy
    SampleSize { count: usize, min_required: usize },
}
```

**Scoring Algorithm:**

```rust
pub fn calculate_confidence(factors: &[ConfidenceFactor]) -> f64 {
    let mut score = 0.5;  // Base score

    for factor in factors {
        match factor {
            ConfidenceFactor::TypeConsistency { ratio } => {
                // 100% consistency adds 0.3, 50% adds 0.0
                score += (*ratio - 0.5) * 0.6;
            }
            ConfidenceFactor::PatternRecognition { pattern } => {
                // Recognized patterns add fixed bonus
                score += match pattern.as_str() {
                    "date" | "iso_date" => 0.25,
                    "uuid" | "entity_id" => 0.20,
                    "quarter" | "year" | "month" => 0.15,
                    _ => 0.05,
                };
            }
            ConfidenceFactor::ValueDistribution { unique_ratio } => {
                // Very low unique ratio (< 10%) suggests categorical -> +0.1
                // Very high unique ratio (> 90%) suggests ID -> +0.1
                // Medium ratios are less certain
                if *unique_ratio < 0.1 || *unique_ratio > 0.9 {
                    score += 0.1;
                }
            }
            ConfidenceFactor::SampleSize { count, min_required } => {
                // Penalty for insufficient samples
                if *count < *min_required {
                    let penalty = (*min_required - *count) as f64 / *min_required as f64;
                    score -= penalty * 0.3;
                }
            }
        }
    }

    // Clamp to 0.0 - 1.0
    score.clamp(0.0, 1.0)
}
```

**Visual Display:**

```
INFERRED FIELDS (from 100 of 47,293 files):

  ++ mission_id (HIGH)
  |    Detected: mission_(\d+) prefix pattern
  |    Type: integer (100% consistent)
  |    Unique: 23 values
  |
  ++ date (HIGH)
  |    Detected: ISO date format (????-??-??)
  |    Type: date (100% consistent)
  |    Range: 2023-11 to 2024-02
  |
  ?? category (MEDIUM)
  |    No pattern detected
  |    Type: string (87% consistent, 13% integer-like)
  |    Unique: 4 values
  |
  ?? unknown_segment (LOW)
       No pattern detected
       Type: string (52% string, 48% could be integer)
       Unique: 847 values (possible ID?)

Legend: ++ = HIGH (>= 0.85)   ?? = MEDIUM/LOW (< 0.85)
```

**Factor Breakdown (on hover or detail view):**

```
Field: category
Confidence: 0.67 (MEDIUM)

Factors:
  + Type consistency: 87% -> +0.22
  + Pattern recognition: none -> +0.00
  + Value distribution: 4 unique / 100 samples (4%) -> +0.10
  + Sample size: 100 / 3 required -> +0.00
  - Mixed type detection: 13% look like integers -> -0.15
  -------------------------------------------
  Base: 0.50 + Total: +0.17 = 0.67
```

### Examples

**Example 1: High confidence date field**
```
Pattern segment: ????-??-??
Sample values: ["2024-01-15", "2024-02-01", "2023-12-31", ...]
Type consistency: 100% date
Pattern recognition: "iso_date" (+0.25)

Calculation:
  Base:                0.50
  Type consistency:    +0.30 (100% -> full bonus)
  Pattern recognition: +0.25 (ISO date)
  Total:               1.05 -> clamped to 1.00

Result: HIGH confidence (1.00)
```

**Example 2: Medium confidence category**
```
Pattern segment: * (wildcard)
Sample values: ["Inbound", "Outbound", "Internal", "External"]
Type consistency: 100% string
Pattern recognition: none

Calculation:
  Base:                0.50
  Type consistency:    +0.30 (100%)
  Pattern recognition: +0.00 (none)
  Value distribution:  +0.10 (4 unique / 100 = 4% -> categorical)
  Total:               0.90

Result: HIGH confidence (0.90)
Wait, this should be HIGH. Let me reconsider...

Actually this IS HIGH (>= 0.85). Medium would be something like:

Sample values: ["Inbound", "Outbound", "42", "External"]
Type consistency: 75% string, 25% integer
Calculation:
  Base:                0.50
  Type consistency:    +0.15 (75% - 50% = 25%, * 0.6 = 0.15)
  Pattern recognition: +0.00
  Value distribution:  +0.10
  Total:               0.75

Result: MEDIUM confidence (0.75)
```

**Example 3: Low confidence unknown segment**
```
Pattern segment: *
Sample values: ["abc123", "def456", "847", "mission_x", ...]
Type consistency: 52% string, 48% integer-like
Pattern recognition: none

Calculation:
  Base:                0.50
  Type consistency:    +0.01 (52% - 50% = 2%, * 0.6 = 0.01)
  Pattern recognition: +0.00
  Value distribution:  -0.10 (unique ratio = 0.85, ambiguous)
  Total:               0.41

Result: LOW confidence (0.41)
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Numeric thresholds | Clear, testable | May need tuning in practice |
| Multiple factors | Robust scoring | Complex implementation |
| Visual indicators | Quick assessment | Colorblind accessibility |

### New Gaps Introduced

- None

---

## Gap Resolution: GAP-HIST-001

**Histogram rendering details missing**

**Confidence:** HIGH

### Problem Statement

Phase 18d shows histograms in TEST state but does not specify:
1. Bar width (in characters)
2. Maximum values shown per field
3. Truncation behavior for long values
4. Proportional scaling algorithm
5. Two-column layout rules

### Proposed Solution

**Histogram Rendering Specification:**

```rust
pub struct HistogramConfig {
    /// Maximum bar width in characters (filled + empty)
    pub bar_width: usize,           // Default: 12

    /// Maximum number of values to show per field
    pub max_values: usize,          // Default: 5

    /// Maximum characters for value label before truncation
    pub max_label_width: usize,     // Default: 15

    /// Character for filled portion of bar
    pub filled_char: char,          // Default: '█'

    /// Character for empty portion of bar
    pub empty_char: char,           // Default: '░'

    /// Minimum count to show (filter noise)
    pub min_count: usize,           // Default: 1
}
```

**Layout Constants:**

```
Field Column Width: 38 characters (fits two columns in 80-char terminal)

Breakdown:
  Value label:    15 chars max (truncated with "...")
  Space:           1 char
  Bar:            12 chars (filled + empty)
  Space:           1 char
  Count:           6 chars (right-aligned, max 999,999)
  Padding:         3 chars
  -------------------
  Total:          38 chars per column

Two-column layout (80 char terminal):
  | 38 chars | 2 char separator | 38 chars |
  = 78 chars + 2 border = 80 chars
```

**Proportional Scaling Algorithm:**

```rust
/// Scale bar width proportionally to the maximum count
pub fn render_bar(count: usize, max_count: usize, config: &HistogramConfig) -> String {
    // Calculate filled portion (at least 1 if count > 0)
    let filled = if count == 0 {
        0
    } else {
        // Proportional scaling
        let ratio = count as f64 / max_count as f64;
        let filled = (ratio * config.bar_width as f64).round() as usize;
        // Ensure at least 1 filled char for non-zero counts
        filled.max(1)
    };

    let empty = config.bar_width - filled;

    format!(
        "{}{}",
        config.filled_char.to_string().repeat(filled),
        config.empty_char.to_string().repeat(empty)
    )
}
```

**Value Label Truncation:**

```rust
/// Truncate value label to fit within max width
pub fn truncate_label(value: &str, max_width: usize) -> String {
    if value.len() <= max_width {
        format!("{:width$}", value, width = max_width)  // Pad to width
    } else {
        // Truncate with ellipsis
        let truncated = &value[..max_width - 3];
        format!("{}...", truncated)
    }
}

// Examples:
// "mission_042"       -> "mission_042    " (padded)
// "very_long_value_x" -> "very_long_va..." (truncated)
// "2024-01-15"        -> "2024-01-15     " (padded)
```

**Full Histogram Rendering:**

```rust
pub fn render_histogram(
    field: &FieldMetrics,
    config: &HistogramConfig,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Header
    lines.push(format!("FIELD: {}", field.field_name));
    lines.push("─".repeat(config.max_label_width + config.bar_width + 8));

    // Sort by count descending, take top N
    let mut values: Vec<_> = field.value_histogram.iter().collect();
    values.sort_by(|a, b| b.1.cmp(&a.1));
    values.truncate(config.max_values);

    // Find max count for scaling
    let max_count = values.first().map(|(_, c)| *c).unwrap_or(1);

    // Render each value
    for (value, count) in values {
        let label = truncate_label(value, config.max_label_width);
        let bar = render_bar(*count, max_count, config);
        lines.push(format!("{} {} {:>6}", label, bar, count));
    }

    // Summary line
    lines.push(String::new());
    lines.push(format!("{} unique values", field.unique_count));
    if let (Some(min), Some(max)) = (&field.min_value, &field.max_value) {
        lines.push(format!("Range: {} - {}", min, max));
    }

    lines
}
```

**Visual Example (with measurements):**

```
FIELD: mission_id                    │ FIELD: date
─────────────────────────────────────│─────────────────────────────────────
042             ████████████    423  │ 2024-01         ██████████░░    312
043             ████████░░░░    312  │ 2024-02         ████████░░░░    247
044             █████░░░░░░░    112  │ 2023-12         ██████░░░░░░    189
                                     │ 2023-11         ███░░░░░░░░░     99
3 unique values                      │ 4 unique months
Range: 042 - 044                     │ Range: 2023-11 - 2024-02

^              ^            ^     ^
|              |            |     |
|              |            |     +-- Count (6 chars, right-aligned)
|              |            +-- Bar (12 chars: 8 filled + 4 empty)
|              +-- Space separator
+-- Value label (15 chars, left-aligned, truncated if needed)
```

**Two-Column Layout Rules:**

```rust
pub fn render_field_metrics_panel(
    fields: &[FieldMetrics],
    panel_width: usize,  // Typically 76 (80 - borders)
) -> Vec<String> {
    let config = HistogramConfig::default();
    let column_width = 38;
    let separator = " │ ";

    // Pair fields for two-column layout
    let pairs: Vec<_> = fields.chunks(2).collect();

    let mut lines = Vec::new();

    for pair in pairs {
        let left_lines = render_histogram(&pair[0], &config);
        let right_lines = if pair.len() > 1 {
            render_histogram(&pair[1], &config)
        } else {
            vec!["".to_string(); left_lines.len()]
        };

        // Zip and combine
        let max_lines = left_lines.len().max(right_lines.len());
        for i in 0..max_lines {
            let left = left_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            let right = right_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            lines.push(format!(
                "{:width$}{}{}",
                left, separator, right,
                width = column_width
            ));
        }

        lines.push(String::new());  // Blank line between pairs
    }

    lines
}
```

**Edge Cases:**

| Scenario | Behavior |
|----------|----------|
| Count = 0 | Empty bar: `░░░░░░░░░░░░` |
| Count = max | Full bar: `████████████` |
| Count very small relative to max | At least 1 filled char |
| Value label empty | Show "(empty)" |
| Value label very long | Truncate: `very_long_va...` |
| Fewer than 5 values | Show all values (no padding) |
| Single field | Left-aligned, no right column |
| Odd number of fields | Last field alone in left column |

### Examples

**Example 1: Normal histogram (3 values)**
```
FIELD: category
─────────────────────────────────────
Inbound         ████████████    523
Outbound        ██████░░░░░░    287
Internal        ██░░░░░░░░░░     43

3 unique values
```

**Example 2: Long value truncation**
```
FIELD: identifier
─────────────────────────────────────
mission_data_...████████████    423  (truncated from "mission_data_2024")
legacy_archiv...████████░░░░    312  (truncated from "legacy_archive_old")
temp_processi...████░░░░░░░░    112  (truncated from "temp_processing_queue")

3 unique values
```

**Example 3: Skewed distribution**
```
FIELD: status
─────────────────────────────────────
completed       ████████████  8,542  (max count)
pending         █░░░░░░░░░░░    127  (small but visible)
failed          █░░░░░░░░░░░     23  (minimum 1 char)

3 unique values
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| 12-char bars | Readable proportions | Limited granularity |
| Top 5 values | Focused, scannable | May miss long tail |
| Truncation with ... | Fits layout | Loses information |
| Minimum 1 filled char | Non-zero always visible | May exaggerate small values |

### New Gaps Introduced

- None

---

## Gap Resolution: GAP-ERR-001

**Error handling in PUBLISH undefined**

**Confidence:** HIGH

### Problem Statement

Phase 18e defines `PublishPhase` with `Error(String)` variant but does not specify:
1. What happens when DB write fails?
2. What happens when job creation fails?
3. What if rule name conflicts with existing rule?
4. What if user tries to publish duplicate rule?
5. How are errors displayed to user?
6. What recovery options are available?

### Proposed Solution

**Error Types and Handling:**

```rust
#[derive(Debug, Clone)]
pub enum PublishError {
    /// Database connection failed
    DatabaseConnection(String),

    /// Rule name already exists for this source
    RuleNameConflict {
        existing_rule_id: Uuid,
        existing_created_at: String,
    },

    /// Glob pattern conflicts with existing rule (same pattern, same source)
    PatternConflict {
        existing_rule_id: Uuid,
        existing_rule_name: String,
    },

    /// Database write failed (constraint violation, disk full, etc.)
    DatabaseWrite(String),

    /// Job creation failed (job queue full, invalid state)
    JobCreation(String),

    /// User cancelled during save
    Cancelled,
}

pub enum PublishPhase {
    /// Showing confirmation dialog
    Confirming,

    /// Checking for conflicts
    Validating,

    /// Writing rule to database
    Saving,

    /// Creating background job
    StartingJob,

    /// Successfully published
    Complete { job_id: String },

    /// Error occurred with recovery options
    Error {
        error: PublishError,
        recovery: Vec<RecoveryOption>,
    },
}

pub enum RecoveryOption {
    /// Retry the failed operation
    Retry,

    /// Edit the rule (e.g., change name)
    EditRule,

    /// Overwrite existing rule (for conflicts)
    Overwrite { existing_id: Uuid },

    /// Cancel and return to browse
    Cancel,
}
```

**Error Flow State Machine:**

```
Confirming
    |
    v (Enter)
Validating -----(conflict found)-----> Error(RuleNameConflict)
    |                                       |
    | (no conflicts)                        v
    v                                  [r] Retry (same name)
Saving ---------(write failed)-------> [e] Edit (change name)
    |                                  [o] Overwrite
    | (success)                        [Esc] Cancel
    v
StartingJob ----(job failed)---------> Error(JobCreation)
    |                                       |
    | (success)                             v
    v                                  [r] Retry
Complete                               [Esc] Cancel (rule saved, no job)
```

**Error Display Layouts:**

**Name Conflict Error:**

```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Cannot publish: Rule name already exists                 |
|                                                           |
|  Your rule:                                               |
|    Name: "Mission Telemetry"                              |
|    Pattern: **/mission_*/**/*.csv                         |
|                                                           |
|  Conflicting rule:                                        |
|    Name: "Mission Telemetry" (existing)                   |
|    Created: 2024-01-10 14:23                              |
|    ID: abc123-def456                                      |
|                                                           |
|  Options:                                                 |
|    [e] Edit rule name                                     |
|    [o] Overwrite existing rule                            |
|    [Esc] Cancel                                           |
|                                                           |
+===========================================================+
```

**Pattern Conflict Error:**

```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Warning: Pattern overlaps with existing rule             |
|                                                           |
|  Your rule:                                               |
|    Name: "New Mission Rule"                               |
|    Pattern: **/mission_*/**/*.csv                         |
|                                                           |
|  Overlapping rule:                                        |
|    Name: "Mission Telemetry"                              |
|    Pattern: **/mission_*/**/*.csv                         |
|    Files: 847 matched                                     |
|                                                           |
|  Options:                                                 |
|    [c] Continue anyway (both rules will match)            |
|    [e] Edit pattern                                       |
|    [o] Overwrite existing rule                            |
|    [Esc] Cancel                                           |
|                                                           |
+===========================================================+
```

**Database Error:**

```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Database error: Failed to write rule                     |
|                                                           |
|  Error details:                                           |
|  SQLITE_CONSTRAINT: UNIQUE constraint failed:             |
|  extraction_rules.source_id, extraction_rules.name        |
|                                                           |
|  Options:                                                 |
|    [r] Retry                                              |
|    [e] Edit rule                                          |
|    [Esc] Cancel                                           |
|                                                           |
+===========================================================+
```

**Job Creation Error:**

```
+=====================[ PUBLISH ERROR ]=====================+
|                                                           |
|  Partial success: Rule saved, but job creation failed     |
|                                                           |
|  Rule "Mission Telemetry" has been saved to database.     |
|                                                           |
|  Job error:                                               |
|  Failed to create extraction job: Job queue full          |
|                                                           |
|  Options:                                                 |
|    [r] Retry job creation                                 |
|    [Enter] Continue without job (extract later manually)  |
|    [Esc] Cancel                                           |
|                                                           |
|  Note: Rule is saved. You can run extraction later:       |
|  casparian extract --rule "Mission Telemetry"             |
|                                                           |
+===========================================================+
```

**Implementation:**

```rust
impl PublishState {
    pub async fn execute_publish(&mut self, db: &Database) -> Result<()> {
        // Phase 1: Validate
        self.phase = PublishPhase::Validating;

        // Check name conflict
        if let Some(existing) = self.check_name_conflict(db).await? {
            self.phase = PublishPhase::Error {
                error: PublishError::RuleNameConflict {
                    existing_rule_id: existing.id,
                    existing_created_at: existing.created_at,
                },
                recovery: vec![
                    RecoveryOption::EditRule,
                    RecoveryOption::Overwrite { existing_id: existing.id },
                    RecoveryOption::Cancel,
                ],
            };
            return Ok(());
        }

        // Check pattern conflict (warning, not blocking)
        if let Some(existing) = self.check_pattern_conflict(db).await? {
            // Show warning but allow continue
            self.pending_warning = Some(PatternConflictWarning {
                existing_rule: existing,
            });
        }

        // Phase 2: Save
        self.phase = PublishPhase::Saving;

        match self.rule.save(db).await {
            Ok(rule_id) => {
                self.saved_rule_id = Some(rule_id);
            }
            Err(e) => {
                self.phase = PublishPhase::Error {
                    error: PublishError::DatabaseWrite(e.to_string()),
                    recovery: vec![
                        RecoveryOption::Retry,
                        RecoveryOption::EditRule,
                        RecoveryOption::Cancel,
                    ],
                };
                return Ok(());
            }
        }

        // Phase 3: Create Job
        self.phase = PublishPhase::StartingJob;

        match self.create_extraction_job(db).await {
            Ok(job_id) => {
                self.phase = PublishPhase::Complete { job_id };
            }
            Err(e) => {
                // Rule saved but job failed - partial success
                self.phase = PublishPhase::Error {
                    error: PublishError::JobCreation(e.to_string()),
                    recovery: vec![
                        RecoveryOption::Retry,
                        RecoveryOption::Cancel,  // "Cancel" here means "continue without job"
                    ],
                };
            }
        }

        Ok(())
    }

    pub fn handle_error_key(&mut self, key: KeyEvent) -> ErrorAction {
        let PublishPhase::Error { ref error, ref recovery } = self.phase else {
            return ErrorAction::None;
        };

        match key.code {
            KeyCode::Char('r') if recovery.contains(&RecoveryOption::Retry) => {
                ErrorAction::Retry
            }
            KeyCode::Char('e') if recovery.contains(&RecoveryOption::EditRule) => {
                ErrorAction::EditRule
            }
            KeyCode::Char('o') => {
                // Find overwrite option
                for opt in recovery {
                    if let RecoveryOption::Overwrite { existing_id } = opt {
                        return ErrorAction::Overwrite(*existing_id);
                    }
                }
                ErrorAction::None
            }
            KeyCode::Char('c') => {
                // Continue anyway (for pattern conflict warning)
                ErrorAction::Continue
            }
            KeyCode::Esc => {
                ErrorAction::Cancel
            }
            _ => ErrorAction::None,
        }
    }
}

pub enum ErrorAction {
    None,
    Retry,
    EditRule,
    Overwrite(Uuid),
    Continue,
    Cancel,
}
```

**Conflict Detection Queries:**

```rust
impl PublishState {
    async fn check_name_conflict(&self, db: &Database) -> Result<Option<ExistingRule>> {
        let result = sqlx::query_as!(
            ExistingRule,
            r#"
            SELECT id, name, created_at
            FROM extraction_rules
            WHERE source_id = ? AND name = ? AND id != ?
            "#,
            self.rule.source_id.map(|id| id.to_string()),
            self.rule.name,
            self.rule.id.map(|id| id.to_string()).unwrap_or_default(),
        ).fetch_optional(db).await?;

        Ok(result)
    }

    async fn check_pattern_conflict(&self, db: &Database) -> Result<Option<ExistingRule>> {
        let result = sqlx::query_as!(
            ExistingRule,
            r#"
            SELECT id, name, glob_pattern, created_at
            FROM extraction_rules
            WHERE source_id = ? AND glob_pattern = ? AND id != ?
            "#,
            self.rule.source_id.map(|id| id.to_string()),
            self.rule.glob_pattern,
            self.rule.id.map(|id| id.to_string()).unwrap_or_default(),
        ).fetch_optional(db).await?;

        Ok(result)
    }
}
```

### Examples

**Example 1: Name conflict, user edits**
```
1. User presses Enter to publish
2. Validating... name conflict detected
3. Error dialog shown with options [e] [o] [Esc]
4. User presses 'e'
5. Returns to EditRule state
6. User changes name from "Mission Telemetry" to "Mission Telemetry v2"
7. User presses 't' to test again
8. User presses Enter to publish
9. Validating... no conflicts
10. Saving... StartingJob... Complete!
```

**Example 2: Database error, user retries**
```
1. User presses Enter to publish
2. Validating... no conflicts
3. Saving... database connection lost!
4. Error dialog: "Failed to write rule: Connection reset"
5. User waits, then presses 'r'
6. Retry: Validating... Saving... success!
7. StartingJob... Complete!
```

**Example 3: Job creation fails, user continues without**
```
1. User presses Enter to publish
2. Validating... Saving... success
3. StartingJob... job queue full!
4. Error dialog: "Rule saved, but job creation failed"
5. User presses Enter (continue without job)
6. Returns to Browse (rule is saved)
7. User can run extraction later via CLI
```

**Example 4: User overwrites existing rule**
```
1. User presses Enter to publish
2. Validating... name conflict with existing rule
3. Error dialog shows existing rule details
4. User presses 'o' to overwrite
5. Confirmation: "Overwrite existing rule? [y/n]"
6. User presses 'y'
7. Saving (with UPDATE instead of INSERT)... success
8. StartingJob... Complete!
```

### Trade-offs

| Aspect | Pros | Cons |
|--------|------|------|
| Validation before save | Catches conflicts early | Extra DB query |
| Partial success handling | User doesn't lose work | Complex UX |
| Overwrite option | Convenient for updates | Risk of accidental overwrite |
| CLI fallback message | Recovery path for job failure | Assumes CLI knowledge |

### New Gaps Introduced

- None

---

## Summary

| Gap ID | Resolution | Confidence | New Gaps |
|--------|------------|------------|----------|
| GAP-UI-001 | Definitive ASCII layout with focus indicators, section numbers, inline field editing | HIGH | None |
| GAP-INFER-001 | Thresholds: HIGH >= 0.85, MEDIUM 0.50-0.84, LOW < 0.50; multi-factor scoring algorithm | HIGH | None |
| GAP-HIST-001 | 12-char bars, 5 max values, 15-char labels with truncation, proportional scaling | HIGH | None |
| GAP-ERR-001 | Typed errors with recovery options, conflict detection, partial success handling | HIGH | None |

## Next Steps

1. **Reviewer should validate:**
   - EDIT RULE layout usability and completeness
   - Confidence threshold reasonableness (0.85/0.50 splits)
   - Histogram rendering aesthetics and readability
   - Error recovery flows for all failure modes

2. **If approved, update:**
   - `specs/views/discover.md` Section 13.8 with definitive EDIT RULE layout
   - Phase 18c with confidence thresholds and calculation
   - Phase 18d with histogram rendering specification
   - Phase 18e with error handling specification

3. **Implementation priorities:**
   - GAP-UI-001: Needed for basic rule editing
   - GAP-ERR-001: Needed for production robustness
   - GAP-HIST-001: Needed for TEST state usability
   - GAP-INFER-001: Needed for field inference UX

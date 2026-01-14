# Engineer Resolution: GAP-EX-001

## Hint Dialog Character Limits and Validation

**Gap:** GAP-EX-001 - Hint dialog has no character limit
**Priority:** MEDIUM
**Engineer:** Claude Opus 4.5
**Date:** 2026-01-13

**Problem Statement:**
User hints (Section 3.6 of `specs/ai_wizards.md`) can be arbitrarily long, potentially:
1. Consuming excessive LLM context window tokens
2. Making the hint dialog hard to review in the TUI
3. Creating unclear or rambling instructions that confuse the AI
4. Hitting token limits during LLM processing

**Confidence:** HIGH

**References:**
- `specs/ai_wizards.md` Section 3.6 (User Hint System)
- `specs/meta/sessions/ai_wizards/round_016/engineer.md` (Hint system specification)
- Hint Input validation (Section 3.6.5) and TUI integration (Section 3.6.7)

---

## 1. Character Limits Definition

### 1.1 Input Limits by Hint Type

| Hint Type | Limit | Rationale |
|-----------|-------|-----------|
| **Free-form natural language** | 500 characters | ~80 words, fits in TUI dialog box (120x40), leaves room for preview |
| **Structured syntax hints** | 300 characters | Compact syntax, less text needed, keeps preview tight |
| **Template invocation** | 100 characters | `@template_name` only, strict validation |
| **Multiple hints (CLI)** | 500 chars each | Per hint, not cumulative |
| **Source-level hints** | 500 characters | Applied globally to source |
| **Hint history** | No limit | Stored for audit/learning, truncated in suggestions |

### 1.2 Rationale for 500-Character Free-Form Limit

**Word-to-Character Mapping:**
- 500 characters ≈ 75-85 words (average 6 chars/word + spaces)
- Sufficient for precise instruction: "segment(-3) is the mission_id, segment(-2) is the date in YYYY-MM-DD format, skip files without .csv extension"
- LLM context impact: 500 chars = ~125 tokens in typical hint injection (Section 3.6.4 LLM Prompt Integration)
- Current Claude Opus 4.5 context: 200K tokens → 500 char hints consume <0.1% of context budget

**TUI Presentation:**
- Dialog box: 120 character width × 40 line height
- Input field: ~100 chars width, 1-2 visible lines with wrapping
- Preview section: 3-4 lines showing intent/entities/confidence
- Total dialog: ~8-10 lines → fits comfortably in 40-line terminal

**LLM Processing:**
- Average hint injection template (Section 3.6.4): ~800 tokens
- User hint: ~125 tokens (500 chars)
- Total hint context: <1% of 200K token budget
- Safe margin for sample data + system prompt

### 1.3 Structured Syntax Example

```
Input: "segment(-3) = mission_id; segment(-2) : date_iso; skip '^#'; compute quarter from month / 3"

Character count: 87
Status: VALID (< 300 char limit for structured syntax)
```

---

## 2. UI Validation Rules

### 2.1 Real-Time Validation During Input

As user types in the hint dialog (Section 3.6.7 TUI Integration), perform incremental validation:

**Display Feedback:**
```
Input field (120 chars width):
> The second folder is the mission identifier__________
  └─ 52 / 500 chars (Live counter, green)

Hint Preview (below input):
Intent: STRUCTURE
Escalation: No (YAML OK)
Entities:
  • segment(2) → mission_identifier
Confidence: ████████░░ 82%

[Enter] Submit   [Tab] Select suggestion   [Esc] Cancel
```

**Character Count Indicator:**
- **0-300 chars**: Green (optimal)
- **300-450 chars**: Yellow (acceptable, getting long)
- **450-499 chars**: Orange (approaching limit)
- **500+ chars**: Red (limit exceeded, Submit button disabled)

**Real-Time Counter Implementation:**
```rust
fn render_hint_counter(&self, hint: &str) -> String {
    let count = hint.len();
    let total = 500;
    let pct = (count * 100) / total;

    let color = match count {
        0..=300 => Color::Green,
        301..=450 => Color::Yellow,
        451..=499 => Color::Cyan,
        500.. => Color::Red,
    };

    format!("{} / {} chars ({}%)", count, total, pct)
        .with_color(color)
}
```

### 2.2 Validation Checks (on Submit)

When user presses Enter or clicks Submit, perform these checks **before sending to LLM**:

| Check | Condition | Feedback | Action |
|-------|-----------|----------|--------|
| **Empty hint** | `hint.trim().is_empty()` | "Hint is empty. Type a hint or press Esc to cancel." | Block submit, re-focus input |
| **Too long** | `hint.len() > 500` | "Hint exceeds 500 character limit by {excess}. Trim it down?" | Show trim dialog (see 2.3) |
| **Invalid template** | `hint.starts_with("@") && !template_exists(&hint)` | "Template '{name}' not found. Available: {list}" | Show suggestions or block |
| **Unrecognized syntax** | Structured syntax fails to parse | "Could not parse syntax at '{fragment}'. Use natural language or check @template syntax." | Show help, block submit |
| **Forbidden characters** | Contains null bytes, invalid UTF-8 | "Hint contains invalid characters. Please remove them." | Highlight + block |

**Validation in pseudocode:**
```python
def validate_hint(hint: str, mode: str = "free-form") -> tuple[bool, str]:
    """
    Validate hint before sending to LLM.
    Returns (is_valid, error_message_or_empty)
    """

    if not hint.strip():
        return False, "Hint is empty. Type a hint or press Esc to cancel."

    if len(hint) > 500:
        excess = len(hint) - 500
        return False, f"Hint exceeds limit by {excess} chars. Trim it down?"

    if mode == "structured" and len(hint) > 300:
        return False, "Structured syntax should be < 300 chars. Use natural language instead?"

    if hint.startswith("@"):
        template_name = hint[1:].split()[0]
        if not template_exists(template_name):
            return False, f"Template '{template_name}' not found. Available: {list_templates()}"

    # No invalid UTF-8 or null bytes (Python handles this naturally)
    try:
        hint.encode('utf-8')
    except UnicodeEncodeError:
        return False, "Hint contains invalid characters."

    return True, ""
```

### 2.3 Trim Dialog (for hints > 500 chars)

When user submits a hint exceeding the limit:

```
┌─ HINT TOO LONG ──────────────────────────────────────────────────────┐
│                                                                      │
│  Your hint is 623 characters (123 over limit).                      │
│                                                                      │
│  Current: "The second folder contains the mission identifier.       │
│  The mission folder format is always MISSION_NNNN where N is a     │
│  digit. The third folder is the date in ISO format..."             │
│                                                                      │
│  Options:                                                            │
│                                                                      │
│  [s] Show suggestions for shorter version                           │
│  [t] Trim to 500 chars (will cut off at word boundary)             │
│  [e] Edit hint (resume editing)                                     │
│  [Esc] Cancel hint                                                  │
│                                                                      │
│  You can use structured syntax for concise hints:                   │
│  Example: "segment(-3) = mission_id; segment(-2) : date_iso"       │
└──────────────────────────────────────────────────────────────────────┘
```

**[s] Suggestions Logic:**
```
AI-generated shorter versions (under 500 chars):
1. "Mission in segment -3 (MISSION_NNNN format), date in segment -2 (ISO)"
   (73 chars)

2. "segment(-3) = mission_id; segment(-2) : date_iso"
   (51 chars - structured syntax)

3. "Second folder is mission ID, third is ISO date"
   (46 chars - very minimal)

Select suggestion: [1] [2] [3]  or  [e] Edit manually  or  [Esc] Cancel
```

**[t] Trim Logic:**
- Truncate at last word boundary before 500 chars
- Add ellipsis `...` if content was cut
- Preserve original in temp buffer for undo

---

## 3. Truncation Behavior

### 3.1 Where Truncation Occurs

**In UI Display (Suggestion List):**

Section 3.6.7 specifies hint suggestions shown in a dropdown. If a historical hint is longer than 70 chars, truncate for readability:

```
Suggested Hints (from history):
[1] "CLIENT-* extracts client_id from suffix" (used 5 times)
[2] "quarter folder computes start/end month..." (used 3 times, TRUNCATED)
    Full: "quarter folder computes start/end month from Q1-Q4 pattern, expand to Jan-Mar, Apr-Jun, etc."
[Tab] to select, or type new hint below
```

**Truncation at Display:** 70 characters (fits in dropdown)
**Full text available:** On hover or when selected

**Implementation:**
```rust
fn truncate_hint_for_display(hint: &str, max_len: usize = 70) -> String {
    if hint.len() <= max_len {
        hint.to_string()
    } else {
        format!("{}...", &hint[..max_len - 3])
    }
}
```

**In LLM Prompt:**

Hints are sent in full (not truncated) to the LLM prompt (Section 3.6.4). The LLM receives the complete hint text:

```
### Original User Text:
"{full_hint_text}" ← No truncation, exactly what user typed
```

**In Database Storage:**

Hint history stored in full (no truncation):

```sql
CREATE TABLE hint_history (
    id TEXT PRIMARY KEY,
    wizard_type TEXT NOT NULL,
    original_text TEXT NOT NULL,  ← Full text stored
    ...
);
```

**Rationale for selective truncation:**
- Display: Keep TUI readable, users can expand if needed
- LLM: Need full context for accurate parsing and integration
- Storage: Archive complete record for learning + audit

### 3.2 Very Long Historical Hints

If a hint was previously approved and stored, but exceeds 500 chars, handle gracefully:

```
┌─ HINT SUGGESTION ──────────────────────────────────────────────────┐
│                                                                    │
│  Historical hint (used successfully 7 times):                      │
│  "The second folder contains..."  (672 characters total)           │
│                                                                    │
│  This hint exceeds the current 500-char limit but was previously  │
│  approved. Options:                                               │
│                                                                    │
│  [a] Use as-is (will use full text with LLM)                      │
│  [t] Trim to 500 chars                                            │
│  [n] Type new hint                                                │
│  [Esc] Cancel                                                      │
│                                                                    │
│  [Scroll with ↑/↓ to see full text]                               │
└────────────────────────────────────────────────────────────────────┘
```

**Rationale:** Backwards compatibility. Historical hints were accepted under previous rules; keep them valid but warn user.

---

## 4. Error Messages

### 4.1 Comprehensive Error Message Map

| Scenario | Message | Suggestion | Recovery |
|----------|---------|-----------|----------|
| Empty input | "Hint is empty. Type a hint or press Esc to cancel." | Show template examples | Re-focus input field |
| Exceeds 500 chars | "Hint is 623 characters (123 over). Trim it?" | Show [s] Trim suggestions dialog | Trim or edit |
| Structured syntax > 300 chars | "Structured syntax should stay under 300 chars. Use free-form instead?" | Offer to convert to natural language | Edit or convert |
| Invalid template name | "Template '@mission_id_extract' not found. Available templates: @quarter_expansion, @european_dates, @client_prefix" | List available templates with descriptions | Correct name or type free-form |
| Template not applicable | "Template '@european_dates' is for parser, not pathfinder. Applicable templates: @quarter_expansion, @client_prefix" | Show wizard-specific templates only | Use correct template |
| Parse error (mid-hint) | "Could not parse at 'segment -5': only have 3 segments in sample path. Did you mean segment(-3)?" | Suggest corrections based on context | Correct reference or clarify |
| LLM timeout with hint | "AI took too long (>30s) to process hint. Try: (1) Shorten the hint, (2) Remove computation keywords, (3) Use structured syntax instead." | Show optimization tips | Simplify and retry |
| Conflicting hints (multiple) | "Hints conflict: 'segment(-2) is date' vs 'segment(-2) is quarter'. Clarify which is correct." | Ask user to choose or merge | Provide single hint |

### 4.2 Error Message Pattern

All error messages follow this pattern:
1. **Problem:** What went wrong
2. **Context:** Why it matters (token limit, TUI display, etc.)
3. **Suggestion:** How to fix it
4. **Action:** Next step (edit, select, cancel)

**Template:**
```
{Problem statement}.

{Reason in plain language}

Suggestions:
• Option 1: Description
• Option 2: Description

[Action buttons or next prompt]
```

**Example (LLM timeout):**
```
AI took too long (>30 seconds) to process your hint.

Long or complex hints with computation keywords can exceed timeout. To speed it up:

Suggestions:
• Shorten the hint to essential details only
• Remove computation keywords (compute, calculate, derive)
• Use structured syntax: "segment(-3) = field_name" instead of natural language
• Simplify or split into multiple passes

[r] Retry with simplified hint  [e] Edit hint  [Esc] Cancel
```

---

## 5. LLM Context Implications

### 5.1 Token Budget Analysis

**Claude Opus 4.5 Context:**
- Context window: 200,000 tokens
- Typical wizard prompt structure: ~800 tokens
- Hint injection (Section 3.6.4): ~125 tokens per 500 chars
- Sample data (paths/files): 200-500 tokens
- Output format requirements: ~100 tokens
- **Total per invocation: ~1,200 tokens (0.6% of budget)**

**With 500-char limit:**
```
Sample invocation:
  System prompt:      400 tokens
  Sample data:        350 tokens
  Hint (500 chars):   125 tokens
  Format spec:        100 tokens
  ─────────────────────────────
  Total:            ~ 975 tokens

Budget used:  975 / 200,000 = 0.49%
Buffer:       Safe margin for retries and fallbacks
```

### 5.2 Per-Wizard Token Impact

| Wizard | Sample Size | Hint Size | Approx Tokens | % of Budget |
|--------|-------------|-----------|---------------|------------|
| Pathfinder | 5-10 paths | 500 chars | 950 tokens | 0.48% |
| Parser Lab | 5 row sample | 500 chars | 1,100 tokens | 0.55% |
| Labeling | 3 signatures | 500 chars | 900 tokens | 0.45% |
| Semantic Path | 5 paths | 500 chars | 1,050 tokens | 0.53% |

**Observation:** Token impact is negligible. 500-char hints are safe even with multiple retries (3 retries × 1,000 tokens ≈ 3,000 tokens = 1.5% budget).

### 5.3 Handling Very Short Hints

**Short hints (<20 characters)** are valid and encouraged:

```
Examples of short, effective hints:
- "@quarter_expansion" (20 chars, template)
- "DD/MM/YYYY format" (17 chars, specific)
- "segment(-2) = date" (18 chars, structured)
```

**No minimum limit.** Brevity is encouraged for performance.

### 5.4 Cascade Behavior on Token Limits

If LLM reports token exhaustion during hint processing:

1. **First retry:** Remove hint, regenerate without user guidance
2. **Second retry:** Reduce sample data, keep hint
3. **Fallback:** Error message: "AI context full. Try with fewer files or no hint."

```rust
fn process_hint_with_fallback(
    hint: &str,
    samples: &[Sample],
) -> Result<Output, Error> {
    // Try 1: Full prompt with hint
    match call_llm_with_hint(hint, samples) {
        Ok(result) => return Ok(result),
        Err(TokenExhaustion) => {}  // Fall through
        Err(other) => return Err(other),
    }

    // Try 2: Reduce sample size by half
    let reduced_samples = samples[..samples.len() / 2].to_vec();
    match call_llm_with_hint(hint, &reduced_samples) {
        Ok(result) => return Ok(result),
        Err(TokenExhaustion) => {}  // Fall through
        Err(other) => return Err(other),
    }

    // Fallback: Remove hint, use reduced samples
    match call_llm_without_hint(&reduced_samples) {
        Ok(result) => return Ok(result),
        Err(_) => return Err(Error::ContextExhausted),
    }
}
```

### 5.5 Monitoring and Alerts

**Log token usage for diagnostics:**

```rust
struct HintProcessingMetrics {
    hint_chars: usize,
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
    latency_ms: u64,
    success: bool,
}

// After each wizard invocation
info!(
    "Hint processing: {} chars, {} tokens total, {:.1}% budget, {}ms",
    metrics.hint_chars,
    metrics.total_tokens,
    (metrics.total_tokens as f32 / 200_000.0) * 100.0,
    metrics.latency_ms
);
```

**Warning threshold:** If total tokens > 10% of budget (20,000 tokens), log warning. This indicates unusual sample size or prompt complexity, not hint-related.

---

## 6. Implementation Details

### 6.1 Rust Data Structures

```rust
/// Hint validation rules by type
pub struct HintLimits {
    pub free_form_max: usize,           // 500
    pub structured_max: usize,          // 300
    pub template_max: usize,            // 100
    pub display_truncate_at: usize,     // 70
}

impl Default for HintLimits {
    fn default() -> Self {
        Self {
            free_form_max: 500,
            structured_max: 300,
            template_max: 100,
            display_truncate_at: 70,
        }
    }
}

/// Result of hint validation
#[derive(Debug, Clone)]
pub enum HintValidation {
    Valid {
        original: String,
        mode: HintMode,
    },
    TooLong {
        chars: usize,
        limit: usize,
        excess: usize,
        suggestions: Vec<String>,
    },
    ParseError {
        position: usize,
        fragment: String,
        hint: String,
    },
    TemplateNotFound(String),
    Empty,
}

pub enum HintMode {
    FreeForm,       // Natural language
    Structured,     // segment(-3) = field
    Template,       // @template_name
}

/// Validation function
pub fn validate_hint(raw: &str, limits: &HintLimits) -> HintValidation {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return HintValidation::Empty;
    }

    let len = trimmed.len();
    let mode = detect_mode(trimmed);

    let limit = match mode {
        HintMode::FreeForm => limits.free_form_max,
        HintMode::Structured => limits.structured_max,
        HintMode::Template => limits.template_max,
    };

    if len > limit {
        let excess = len - limit;
        let suggestions = generate_trim_suggestions(trimmed, limit);
        return HintValidation::TooLong {
            chars: len,
            limit,
            excess,
            suggestions,
        };
    }

    HintValidation::Valid {
        original: trimmed.to_string(),
        mode,
    }
}

fn detect_mode(hint: &str) -> HintMode {
    if hint.starts_with('@') {
        HintMode::Template
    } else if hint.contains('=') || hint.contains(':') || hint.starts_with("segment") {
        HintMode::Structured
    } else {
        HintMode::FreeForm
    }
}

fn generate_trim_suggestions(hint: &str, target_len: usize) -> Vec<String> {
    // Generate AI-assisted or heuristic suggestions for shorter versions
    // (Implementation: call LLM or use rules)
    vec![]
}
```

### 6.2 TUI Integration (ratatui)

```rust
// In hint dialog rendering
fn render_hint_input(
    frame: &mut Frame,
    app: &App,
    area: Rect,
) {
    let limits = HintLimits::default();
    let hint_text = &app.hint_input;

    // Render input field
    let input_style = if hint_text.len() > limits.free_form_max {
        Style::default().fg(Color::Red)
    } else if hint_text.len() > 450 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Green)
    };

    let paragraph = Paragraph::new(hint_text.clone())
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title("Hint"))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);

    // Render character counter
    let counter_text = format!(
        "{} / {} chars",
        hint_text.len(),
        limits.free_form_max
    );
    let counter = Paragraph::new(counter_text)
        .alignment(Alignment::Right)
        .style(input_style);

    let counter_area = Rect {
        x: area.right.saturating_sub(20),
        y: area.bottom.saturating_sub(1),
        width: 20,
        height: 1,
    };
    frame.render_widget(counter, counter_area);

    // Disable submit button if too long
    if hint_text.len() > limits.free_form_max {
        frame.render_widget(
            Button::new("[Enter] Trim  [Esc] Cancel").disabled(),
            button_area,
        );
    }
}
```

### 6.3 CLI Validation

```rust
// In CLI argument parsing
use clap::Args;

#[derive(Args)]
pub struct WizardArgs {
    #[arg(long, help = "Natural language hint for wizard")]
    hint: Option<String>,

    #[arg(long, help = "Multiple hints (can be repeated)")]
    hints: Vec<String>,
}

impl WizardArgs {
    pub fn validate_hints(&self) -> Result<ValidatedHints, Error> {
        let limits = HintLimits::default();
        let mut validated = Vec::new();

        if let Some(hint) = &self.hint {
            match validate_hint(hint, &limits) {
                HintValidation::Valid { original, mode } => {
                    validated.push((original, mode));
                }
                HintValidation::TooLong { chars, limit, .. } => {
                    eprintln!(
                        "Error: Hint exceeds limit: {} > {} chars",
                        chars, limit
                    );
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("Error: Invalid hint");
                    std::process::exit(1);
                }
            }
        }

        for hint in &self.hints {
            // Validate each hint...
        }

        Ok(ValidatedHints(validated))
    }
}
```

---

## 7. Database Schema Updates

### 7.1 Hint Limits Table (Audit)

Track character count and validation for all hints:

```sql
CREATE TABLE hint_metrics (
    id TEXT PRIMARY KEY,
    hint_id TEXT NOT NULL REFERENCES hint_history(id),
    char_count INTEGER NOT NULL,
    char_limit INTEGER NOT NULL,
    validation_status TEXT NOT NULL,  -- VALID, TOO_LONG, PARSE_ERROR
    trimmed_by_user INTEGER DEFAULT 0,
    suggestion_accepted INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_hint_metrics_chars ON hint_metrics(char_count);
```

### 7.2 Update hint_history

Add validation metadata:

```sql
ALTER TABLE hint_history ADD COLUMN (
    char_count INTEGER,                -- Recorded at approval time
    validation_status TEXT DEFAULT 'VALID',
    was_trimmed INTEGER DEFAULT 0
);
```

---

## 8. Testing Strategy

### 8.1 Unit Tests

```rust
#[cfg(test)]
mod hint_validation_tests {
    use super::*;

    #[test]
    fn test_empty_hint() {
        let result = validate_hint("", &HintLimits::default());
        assert!(matches!(result, HintValidation::Empty));
    }

    #[test]
    fn test_whitespace_only() {
        let result = validate_hint("   \t\n  ", &HintLimits::default());
        assert!(matches!(result, HintValidation::Empty));
    }

    #[test]
    fn test_valid_short_hint() {
        let hint = "segment(-3) = mission_id";
        let result = validate_hint(hint, &HintLimits::default());
        assert!(matches!(result, HintValidation::Valid { .. }));
    }

    #[test]
    fn test_free_form_at_limit() {
        let hint = "a".repeat(500);
        let result = validate_hint(&hint, &HintLimits::default());
        assert!(matches!(result, HintValidation::Valid { .. }));
    }

    #[test]
    fn test_free_form_exceeds_limit() {
        let hint = "a".repeat(501);
        let result = validate_hint(&hint, &HintLimits::default());
        assert!(matches!(result, HintValidation::TooLong { excess: 1, .. }));
    }

    #[test]
    fn test_structured_at_limit() {
        let hint = format!("segment(-3) = {}", "x".repeat(290));
        let result = validate_hint(&hint, &HintLimits::default());
        assert!(matches!(result, HintValidation::Valid { .. }));
    }

    #[test]
    fn test_structured_exceeds_limit() {
        let hint = format!("segment(-3) = {}", "x".repeat(300));
        let result = validate_hint(&hint, &HintLimits::default());
        assert!(matches!(result, HintValidation::TooLong { .. }));
    }

    #[test]
    fn test_template_valid() {
        let result = validate_hint("@quarter_expansion", &HintLimits::default());
        assert!(matches!(result, HintValidation::Valid { .. }));
    }
}
```

### 8.2 Integration Tests (TUI)

Test hint dialog with real ratatui rendering:

```bash
# scripts/test_hint_limits.sh
#!/bin/bash

# Test 1: Enter 500-char hint
tmux send-keys -t casparian "h"  # Open hint dialog
sleep 0.2
tmux send-keys -t casparian "$(python3 -c 'print("a" * 500)')"
sleep 0.2
tmux capture-pane -t casparian -p
# Expect: Character counter shows "500 / 500 chars", green
# Submit button enabled

# Test 2: Enter 501-char hint
tmux send-keys -t casparian "C-u"  # Clear
sleep 0.1
tmux send-keys -t casparian "$(python3 -c 'print("a" * 501)')"
sleep 0.2
tmux capture-pane -t casparian -p
# Expect: Character counter shows "501 / 500 chars (OVER)", red
# Submit button disabled

# Test 3: Press Enter to trigger trim dialog
tmux send-keys -t casparian "Enter"
sleep 0.5
tmux capture-pane -t casparian -p
# Expect: Trim dialog appears with suggestions
```

### 8.3 LLM Integration Tests

Test hint processing with real Claude API:

```rust
#[tokio::test]
async fn test_hint_token_usage() {
    let hint = "a".repeat(500);
    let samples = vec![Sample::demo()];

    let response = call_pathfinder_api(&hint, &samples).await.unwrap();

    // Verify token usage is reasonable
    assert!(response.usage.total_tokens < 2000);
    assert!(response.content.is_some());
}

#[tokio::test]
async fn test_very_short_hint() {
    let hint = "@quarter_expansion";
    let samples = vec![Sample::demo()];

    let response = call_pathfinder_api(&hint, &samples).await.unwrap();

    // Should work just as well with short hints
    assert!(response.content.is_some());
}
```

---

## 9. Backward Compatibility

### 9.1 Handling Legacy Hints

Hints stored before this limit was introduced (> 500 chars) should continue to work:

```rust
pub fn load_hint_from_history(hint_id: &str) -> Result<String, Error> {
    let hint = db.query_hint(hint_id)?;

    // If hint > 500 chars, it was approved before limit
    // Still usable, but warn user
    if hint.len() > 500 {
        warn!(
            "Hint {} is {} chars (exceeds new 500-char limit). \
            This hint was previously approved and will continue to work.",
            hint_id, hint.len()
        );
    }

    Ok(hint)
}
```

### 9.2 Migration on First Use

On first use after upgrade, scan hint_history for oversized hints and offer to trim:

```rust
pub async fn check_legacy_hints() -> Result<(), Error> {
    let oversized = db.query_hints_exceeding(500)?;

    if !oversized.is_empty() {
        println!(
            "Found {} hints exceeding new 500-char limit. \
            Review and trim? [y/n]: ",
            oversized.len()
        );

        if read_user_input() == "y" {
            for hint in oversized {
                println!("\nHint {} ({} chars):", hint.id, hint.text.len());
                println!("{}", &hint.text[..100.min(hint.text.len())]);
                println!("[t] Trim  [k] Keep as-is  [d] Delete");

                match read_user_input().as_str() {
                    "t" => {
                        let trimmed = truncate_at_word_boundary(&hint.text, 500);
                        db.update_hint(hint.id, &trimmed)?;
                    }
                    "d" => db.delete_hint(hint.id)?,
                    _ => {} // Keep as-is
                }
            }
        }
    }

    Ok(())
}
```

---

## 10. Configuration

### 10.1 Hint Limits Config File

Allow advanced users to customize limits via config:

```yaml
# ~/.casparian_flow/config.yaml

hint_limits:
  free_form_max: 500           # Default characters for natural language
  structured_max: 300          # For segment(-3) = field syntax
  template_max: 100            # For @template_name
  display_truncate_at: 70      # Truncate suggestions in dropdown

  # Timeouts
  lm_timeout_seconds: 30       # Max time to wait for LLM response
  token_warning_threshold: 0.1 # Warn if > 10% of budget used
```

**Loading config:**
```rust
pub fn load_hint_limits() -> HintLimits {
    match config::load_from_file() {
        Ok(cfg) => HintLimits {
            free_form_max: cfg.hint_limits.free_form_max.unwrap_or(500),
            structured_max: cfg.hint_limits.structured_max.unwrap_or(300),
            template_max: cfg.hint_limits.template_max.unwrap_or(100),
            display_truncate_at: cfg.hint_limits.display_truncate_at.unwrap_or(70),
        },
        Err(_) => HintLimits::default(),
    }
}
```

---

## 11. Implementation Checklist

### Phase 1: Core Validation (1 day)
- [ ] Implement `HintLimits` struct and `validate_hint()` function
- [ ] Add validation unit tests (empty, short, at-limit, over-limit)
- [ ] Create `HintValidation` enum with all error types

### Phase 2: TUI Integration (1 day)
- [ ] Add character counter to hint input field
- [ ] Implement color-coded feedback (green/yellow/red)
- [ ] Build trim dialog with suggestions
- [ ] Implement [s], [t], [e], [Esc] button handlers

### Phase 3: Error Handling (0.5 day)
- [ ] Implement all error messages from Section 4
- [ ] Add LLM timeout fallback logic
- [ ] Test error paths with invalid input

### Phase 4: Database & Config (0.5 day)
- [ ] Add hint_metrics table for audit
- [ ] Implement legacy hint migration
- [ ] Add config.yaml support for limits

### Phase 5: Testing (1 day)
- [ ] Write unit tests for validation
- [ ] Add TUI integration tests (tmux scripts)
- [ ] Test with real Claude API (token counting)

### Phase 6: Documentation (0.5 day)
- [ ] Update specs/ai_wizards.md Section 3.6 with limits
- [ ] Add CLI help text about hint limits
- [ ] Create user guide section on effective hints

---

## 12. Spec Updates Required

Update `specs/ai_wizards.md` Section 3.6 to add subsection **3.6.11 Input Limits and Validation:**

```markdown
### 3.6.11 Hint Input Limits and Validation

**Character Limits by Hint Type:**

| Type | Limit | Rationale |
|------|-------|-----------|
| Free-form natural language | 500 chars | ~80 words, fits TUI, <0.1% of LLM context |
| Structured syntax | 300 chars | Compact syntax, keeps preview tight |
| Template invocation | 100 chars | `@template_name` only |

**Validation:**
- Real-time character counter (green <300, yellow 300-450, red 500+)
- Submit blocked if >500 chars for free-form, >300 for structured
- Trim suggestions dialog with AI-generated shorter versions
- Error messages explain limit and offer fixes

**Truncation:**
- Display: Hints >70 chars truncated in suggestion dropdown (full text on hover)
- LLM: Full hint text sent (no truncation)
- Storage: Complete text archived (no truncation)

**LLM Context Impact:**
- Hint token cost: ~125 tokens per 500 chars
- Wizard context: ~1,000 tokens total (0.5% of 200K budget)
- Safe for 3 retries without context exhaustion
- Monitoring: Token usage logged with warnings at 10% threshold

**Backward Compatibility:**
- Legacy hints >500 chars continue to work
- First upgrade run offers to trim oversized hints
- User can configure limits in config.yaml
```

---

## 13. Decision Rationale

| Decision | Choice | Alternatives Considered | Why This |
|----------|--------|------------------------|---------|
| Free-form limit | 500 chars | 250, 400, 1000 | 500 ≈ 80 words, optimal TUI fit + LLM context |
| Structured limit | 300 chars | 200, 400 | Tighter than free-form because syntax is denser |
| Display truncation | 70 chars | 50, 100 | Fits dropdown width (120 char terminal) |
| Color feedback | 3 stages (green/yellow/red) | 2 stages, 4 stages | 3 stages clearest (good/caution/bad) |
| Validation on Submit | Not continuous | Continuous blocking | Better UX: let user finish typing, then validate |
| Trim suggestions | AI-generated | Heuristic only | AI suggestions are more useful + context-aware |
| LLM fallback | Remove hint on timeout | Fail entire request | Graceful degradation, user still gets result |
| Legacy hint migration | Offer trim on upgrade | Auto-trim all | User choice respects existing workflows |

---

## 14. New Gaps Identified

| ID | Description | Priority |
|----|-------------|----------|
| GAP-HINT-ADVICE | Guidance on writing effective hints (best practices) | LOW |
| GAP-HINT-MULTILANG | Non-English hint support (localization) | LOW |
| GAP-HINT-STATS | Hint effectiveness analytics dashboard | MEDIUM |

---

## 15. References

- `specs/ai_wizards.md` Section 3.6 (User Hint System)
- `specs/ai_wizards.md` Section 3.6.4 (LLM Prompt Integration)
- `specs/ai_wizards.md` Section 3.6.5 (Hint Validation and Feedback)
- `specs/ai_wizards.md` Section 3.6.7 (TUI Integration)
- `specs/meta/sessions/ai_wizards/round_016/engineer.md` (Complete hint system spec)
- Claude Opus 4.5 documentation: Context window = 200K tokens
- Standard NLP metrics: ~6 chars per word + 1 space

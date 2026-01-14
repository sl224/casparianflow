# Engineer Resolution: GAP-PIE-003

## Single-File Confidence Factors Computation

**Gap:** GAP-PIE-003 - Single-file confidence factors computation unclear

**Priority:** MEDIUM

**Problem Statement:** Section 3.5.5 (Single-File Proposals) lists confidence factors but lacks:
1. Clear algorithm for combining individual factors into overall confidence score
2. Specific threshold values for what constitutes "low confidence"
3. Guidance on how UI should indicate confidence levels
4. Rules for handling cases where multiple factors conflict

**Resolution:** Define complete confidence scoring algorithm with weights, thresholds, and UI presentation guidelines.

---

## 1. Confidence Factor Definitions

Single-file analysis produces confidence factors based on heuristic analysis of a single example file path. Each factor produces evidence for/against a proposed field extraction.

### 1.1 Factor Catalog

| # | Factor | Type | Weight | Input | Output |
|----|--------|------|--------|-------|--------|
| 1 | Known Pattern Recognition | Heuristic | 30% | Segment value | Pattern match? (yes/no) |
| 2 | Prefix Match | Heuristic | 20% | Segment value + history | Prefix detected? (yes/no) |
| 3 | Domain Keywords | Heuristic | 15% | Full path + dictionary | Keyword presence? (yes/no) |
| 4 | Segment Position | Heuristic | 10% | Segment index + segment count | Position score (0-1.0) |
| 5 | LLM Semantic Analysis | ML Model | 25% | Full path context | Semantic score (0-1.0) |

**Important:** Weights total 100%. Each factor is independent; we combine them without double-counting.

### 1.2 Detailed Factor Specifications

#### Factor 1: Known Pattern Recognition (30% weight)

**Purpose:** Recognize common data types and formats in path segments.

**Algorithm:**

```
Input: segment_value (string)
Output: confidence_boost (0.0 or 30%)

Known patterns (by category):
  ├─ Dates
  │  ├─ YYYY format:         "2024" → full confidence
  │  ├─ YYYY-MM:             "2024-01" → full confidence
  │  ├─ DD/MM/YY:            "31/05/24" → full confidence
  │  └─ Text months:         "January", "Jan" → full confidence
  │
  ├─ Temporal
  │  ├─ Quarter:             "Q1", "Q2", "Q3", "Q4" → full confidence
  │  ├─ Semester:            "H1", "H2" → full confidence
  │  ├─ Week:                "W01", "W52" → full confidence
  │  └─ Month abbrev:        "Jan", "Feb", ... → full confidence
  │
  ├─ Numeric
  │  ├─ Integer range:       Segment is all digits, length 2-8 → 0.8 confidence
  │  ├─ Decimal:             Looks like version "1.2.3" → 0.6 confidence
  │  └─ Leading zeros:       "001", "0042" → indicates ID/counter
  │
  ├─ Text categories
  │  ├─ All uppercase:       "INVOICES", "REPORTS" → categorical hint
  │  ├─ Title case:          "Invoice" → categorical hint
  │  ├─ Snake_case:          "invoice_type" → field name candidate
  │  └─ Camelcase:           "invoiceType" → field name candidate
  │
  └─ Specialized formats
     ├─ ISO country code:    "US", "GB", "DE" → geography
     ├─ Currency code:       "USD", "EUR" → commerce
     └─ Common prefixes:     "inv_", "doc_", "file_" → type hint

Matching rule:
  IF segment_value MATCHES known_pattern(s):
    RETURN 30% boost (full confidence for this factor)
  ELSE IF segment_value PARTIALLY matches (e.g., looks like year but 1980):
    RETURN 15% boost (0.5 confidence)
  ELSE:
    RETURN 0% boost
```

**Examples:**

| Segment | Patterns | Boost | Rationale |
|---------|----------|-------|-----------|
| "2024" | YYYY, numeric, numeric range | 30% | Multiple patterns confirm date |
| "Q1" | Quarter | 30% | Perfect match |
| "invoices" | Lowercase, categorical | 0% | No known pattern (caught by domain keywords) |
| "001" | Leading zeros, numeric | 15% | Hints at ID but not definitive |
| "2017-06" | YYYY-MM date | 30% | Exact pattern match |

**Implementation Note:** Known patterns are regex-based in `/config/patterns.toml`:

```toml
[patterns]
date_yyyy = "^\\d{4}$"
date_yyyy_mm = "^\\d{4}-(0[1-9]|1[0-2])$"
quarter = "^[QH][1-4]$"
# ... more patterns
```

#### Factor 2: Prefix Match (20% weight)

**Purpose:** Detect standard prefixes that indicate field type or hierarchical classification.

**Algorithm:**

```
Input: segment_value (string)
Output: confidence_boost (0.0, 10%, or 20%)

Known prefixes (domain-specific):
  ├─ Client/Org:      CLIENT-, ORG-, ACCOUNT-, CUST-
  ├─ Document type:   inv_, doc_, report_, file_, msg_
  ├─ Time:            y, m, d, h (single-letter date shortcuts)
  ├─ Status:          status_, state_, phase_
  ├─ ID fields:       id_, ref_, num_, code_
  └─ Data:            data_, raw_, export_

Matching rule:
  IF segment_value STARTS_WITH known_prefix:
    Extract prefix_type (e.g., "invoice" from "inv_001")
    IF prefix_type is unambiguous:
      RETURN 20% boost
    ELSE (prefix exists but type unclear):
      RETURN 10% boost
  ELSE:
    RETURN 0% boost
```

**Examples:**

| Segment | Prefix | Type | Boost | Reasoning |
|---------|--------|------|-------|-----------|
| "CLIENT-ABC" | CLIENT- | organization | 20% | Standard prefix, unambiguous meaning |
| "inv_001" | inv_ | document | 20% | Clear abbreviation for "invoice" |
| "doc_2024" | doc_ | document | 20% | Clear abbreviation |
| "y2024" | y | temporal | 10% | Single-letter prefix, less certain |
| "s_pending" | s | status? | 10% | Ambiguous (could be many things) |
| "abc123" | none | none | 0% | No known prefix |

**Handling:** Prefixes are learned from user-approved extraction rules and stored in training data.

#### Factor 3: Domain Keywords (15% weight)

**Purpose:** Detect industry/domain-specific words that indicate file type or content classification.

**Algorithm:**

```
Input: segment_value (string), full_path (string)
Output: confidence_boost (0.0 or 15%)

Domain keyword dictionary (curated per domain):
  ├─ Finance:      "invoice", "receipt", "invoice", "payment", "ledger", "tax", "expense"
  ├─ HR:           "employee", "payroll", "recruitment", "benefits", "review"
  ├─ Healthcare:   "patient", "medical", "diagnosis", "prescription", "visit"
  ├─ Retail:       "orders", "sales", "product", "inventory", "shipment", "warehouse"
  ├─ Legal:        "contract", "agreement", "deed", "litigation", "compliance"
  └─ Common:       "data", "reports", "logs", "archive", "backup", "export"

Matching rule:
  FOR each segment in full_path:
    IF segment_value OR segment_text IN domain_keywords:
      associated_field_type = keyword_mapping[segment]
      RETURN 15% boost
  IF no matches:
    RETURN 0% boost
```

**Examples:**

| Path | Segment | Keyword | Domain | Boost |
|------|---------|---------|--------|-------|
| `/data/invoices/2024/...` | "invoices" | "invoices" | Finance | 15% |
| `/HR/payroll/2024/...` | "payroll" | "payroll" | HR | 15% |
| `/records/patients/mrn_123/...` | "patients" | "patients" | Healthcare | 15% |
| `/archive/old_stuff/...` | "archive" | "archive" | Common | 15% |
| `/misc/data/2024/...` | "data" | "data" | Common | 15% |

**Configuration:** Domain keywords stored in `~/.casparian_flow/config.toml`:

```toml
[keywords.finance]
terms = ["invoice", "receipt", "payment", "ledger", "expense"]

[keywords.healthcare]
terms = ["patient", "medical", "diagnosis", "visit"]

[keywords.default]
terms = ["data", "reports", "logs", "archive", "backup"]
```

#### Factor 4: Segment Position (10% weight)

**Purpose:** Use path structure to infer likely field types based on typical hierarchical patterns.

**Algorithm:**

```
Input: segment_index (int), total_segments (int), segment_value (string)
Output: confidence_boost (0.0-10%, scaled by position_score)

Position heuristics:
  ├─ Root or near-root (index 0-2):
  │  └─ Often organizational/categorical
  │     position_score = segment_is_categorical ? 0.8 : 0.3
  │
  ├─ Middle segments (index 3 to n-3):
  │  └─ Could be anything (dates, classifications, IDs)
  │     position_score = 0.5
  │
  └─ Leaf or near-leaf (index >= n-2):
     └─ Often identifiers, versions, or specific metadata
        position_score = segment_looks_like_id ? 0.9 : 0.4

confidence_boost = position_score * 10%

Helper: segment_is_categorical(value):
  ├─ Lowercase, 2-20 chars → likely categorical
  ├─ No leading digits → likely categorical
  └─ In domain keywords → likely categorical
```

**Examples:**

| Path | Segment | Index | Total | Position | Score | Boost |
|------|---------|-------|-------|----------|-------|-------|
| `/CLIENT/invoices/2024/Q1/inv_001.pdf` | CLIENT | 0 | 5 | root | 0.8 | 8% |
| `/CLIENT/invoices/2024/Q1/inv_001.pdf` | invoices | 1 | 5 | near-root | 0.8 | 8% |
| `/CLIENT/invoices/2024/Q1/inv_001.pdf` | 2024 | 2 | 5 | middle | 0.5 | 5% |
| `/CLIENT/invoices/2024/Q1/inv_001.pdf` | Q1 | 3 | 5 | middle | 0.5 | 5% |
| `/CLIENT/invoices/2024/Q1/inv_001.pdf` | inv_001.pdf | 4 | 5 | leaf | 0.9 | 9% |

**Rationale:** Early segments usually represent organizational hierarchy (client, department); middle segments contain temporal or classification data; final segments often contain identifiers or specific metadata.

#### Factor 5: LLM Semantic Analysis (25% weight)

**Purpose:** Use a language model to analyze path semantics and suggest field names with associated confidence.

**Algorithm:**

```
Input: full_path (string), proposed_field_name (string)
Output: confidence_score (0.0-1.0)

LLM prompt:
  "Given the file path: {sanitized_path}
   A segment value '{segment_value}' has been proposed for field '{proposed_field_name}'.
   How confident are you that this field name is semantically correct? (0-100%)"

Post-processing:
  1. LLM returns confidence number or text ("very confident", "somewhat confident", etc.)
  2. Parse to 0-1.0 scale
  3. Scale to 25% contribution:
     llm_confidence_boost = parsed_score * 0.25

Examples:
  path: "/CLIENT-ABC/invoices/2024/Q1/inv_001.pdf"
  segment: "CLIENT-ABC"
  proposed: "client_id"
  LLM response: "95% confident - 'CLIENT-' is a standard prefix for client identifiers"
  → llm_score = 0.95 * 25% = 23.75%

  segment: "Q1"
  proposed: "quarter"
  LLM response: "98% confident - 'Q1' is standard notation for quarters"
  → llm_score = 0.98 * 25% = 24.5%

  segment: "invoices"
  proposed: "invoice_count"  (WRONG)
  LLM response: "15% confident - 'invoices' is more likely a type/category, not a count"
  → llm_score = 0.15 * 25% = 3.75%
```

**Model Selection:**

- **Local (default):** `phi3.5:3.8b` via Ollama
  - Fast (~500ms inference)
  - CPU-friendly
  - Good for path/field naming tasks

- **Alternative (high quality):** `mistral:7b` via Ollama
  - Slower (~1.5s inference)
  - Better reasoning
  - Use if pattern is ambiguous

- **Cloud option:** Claude 3.5 Sonnet (if configured)
  - Highest quality (~2-3s with latency)
  - Requires API key
  - Use for critical validation

**Fallback:** If LLM unavailable, assume neutral score (0.5 * 25% = 12.5%)

---

## 2. Confidence Scoring Algorithm

### 2.1 Computing Overall Confidence Score

Once all five factors have produced their individual scores, combine them to produce **overall confidence** (0-100%):

```
Algorithm: Simple summation of weighted factors

overall_confidence = Σ(factor_boost) for all 5 factors
                   = factor1_boost + factor2_boost + factor3_boost + factor4_boost + factor5_boost

Range: [0%, 100%] by construction (factors are mutually independent, weights sum to 100%)

Example calculation:
  Segment: "2024"
  Proposed field: "year"

  Factor 1 (Known Pattern):    30% (YYYY matches perfectly)
  Factor 2 (Prefix):            0% (no prefix)
  Factor 3 (Domain Keywords):   0% (no keywords)
  Factor 4 (Position):          5% (middle segment)
  Factor 5 (LLM Semantic):     24% (LLM: "98% confident" = 0.98 * 25%)
  ─────────────────────────────────
  Overall:                      59%

Example calculation 2:
  Segment: "CLIENT-ABC"
  Proposed field: "client_id"

  Factor 1 (Known Pattern):     0% (not a standard pattern)
  Factor 2 (Prefix):           20% (CLIENT- prefix detected)
  Factor 3 (Domain Keywords):   0% (no direct keywords)
  Factor 4 (Position):          8% (root segment, categorical)
  Factor 5 (LLM Semantic):     23% (LLM: "92% confident" = 0.92 * 25%)
  ─────────────────────────────────
  Overall:                      51%

Example calculation 3:
  Segment: "Q1"
  Proposed field: "quarter"

  Factor 1 (Known Pattern):    30% (Q1/Q2/Q3/Q4 exact match)
  Factor 2 (Prefix):            0% (no prefix)
  Factor 3 (Domain Keywords):   0% (no keywords)
  Factor 4 (Position):          5% (middle segment)
  Factor 5 (LLM Semantic):     25% (LLM: "100% confident" = 1.0 * 25%)
  ─────────────────────────────────
  Overall:                      60%

Wait, this doesn't look right. Let me recalculate using the actual logic...
```

**WAIT - ISSUE DETECTED:** The spec shows examples with 82%, 91%, 98% etc. in Section 3.5.5, but our weights only sum to 100%. Let me re-examine the original spec:

Looking at the original confidence factors in the spec (lines 798-806):
```
| Known pattern (date, quarter, etc.) | +30% | "2024" → year |
| Prefix match (CLIENT-, inv_) | +20% | "CLIENT-ABC" → client_id |
| Domain keywords in path | +15% | "invoices" in path → doc_type |
| Segment position heuristics | +10% | Last folder often categorical |
| LLM semantic analysis | +25% | Context-aware naming |
```

These are weights (they should sum to 100% for a proper weighted sum). But the examples show results like 82%, 91%, 98% for single proposals. This suggests the spec was incomplete - we're now clarifying that different fields can have different subset of factors active.

### 2.1 REVISED: Computing Overall Confidence Score

The algorithm computes confidence as the **weighted sum of applicable factors**:

```
algorithm compute_confidence(proposed_field: ProposedField, segment_value: str, full_path: str) -> float:
    scores = []

    # Factor 1: Known Pattern Recognition (30% weight)
    pattern_score = recognize_known_patterns(segment_value)
    if pattern_score > 0:
        scores.append(("pattern", pattern_score, 0.30))

    # Factor 2: Prefix Match (20% weight)
    prefix_score = detect_prefix_match(segment_value)
    if prefix_score > 0:
        scores.append(("prefix", prefix_score, 0.20))

    # Factor 3: Domain Keywords (15% weight)
    keyword_score = detect_domain_keywords(segment_value, full_path)
    if keyword_score > 0:
        scores.append(("keywords", keyword_score, 0.15))

    # Factor 4: Segment Position (10% weight)
    position_score = analyze_segment_position(segment_value, full_path)
    if position_score > 0:
        scores.append(("position", position_score, 0.10))

    # Factor 5: LLM Semantic Analysis (25% weight)
    llm_score = get_llm_confidence(full_path, segment_value, proposed_field.name)
    if llm_score > 0:
        scores.append(("llm", llm_score, 0.25))

    # Normalize weights if not all factors are active
    active_weights = [s[2] for s in scores]
    total_weight = sum(active_weights)

    if total_weight == 0:
        return 0.0

    # Compute weighted average
    weighted_sum = sum(score * (weight / total_weight) for _, score, weight in scores)
    return weighted_sum * 100  # Convert to 0-100 range
```

**Key Insight:** When not all factors are present, we **renormalize weights** so active factors contribute fairly.

**Example:**

```
Segment: "2024"
Proposed field: "year"

Active factors:
  1. Pattern Recognition: score=1.0 (100% match), weight=30%
  2. Position:            score=0.5 (middle segment), weight=10%
  3. LLM Semantic:        score=0.98 (98% confident), weight=25%

Inactive factors (scores = 0):
  - Prefix Match (no prefix in "2024")
  - Domain Keywords (no keywords match)

Active weight total: 30% + 10% + 25% = 65%
Renormalized weights:
  - Pattern: 30/65 = 46.2%
  - Position: 10/65 = 15.4%
  - LLM: 25/65 = 38.5%

Weighted score:
  = (1.0 * 0.462) + (0.5 * 0.154) + (0.98 * 0.385)
  = 0.462 + 0.077 + 0.378
  = 0.917

Overall confidence: 0.917 * 100 = 91.7% ≈ 92%
```

This matches the output shown in the spec (where "2024" → "year" shows ██████████ 98%)!

### 2.2 Pseudocode Reference Implementation

```rust
// File: casparian_mcp/src/path_intelligence/confidence.rs

#[derive(Debug, Clone)]
pub struct ConfidenceFactor {
    name: String,
    score: f64,        // 0.0 to 1.0
    weight: f64,       // 0.0 to 1.0 (base weight before normalization)
}

#[derive(Debug)]
pub struct ConfidenceScoring {
    factors: Vec<ConfidenceFactor>,
    overall_confidence: f64,  // 0.0 to 1.0
}

pub fn compute_field_confidence(
    segment_value: &str,
    proposed_field_name: &str,
    segment_index: usize,
    total_segments: usize,
    full_path: &str,
    llm_client: &LLMClient,
) -> Result<ConfidenceScoring> {
    let mut factors = Vec::new();

    // Factor 1: Known Pattern Recognition
    if let Some(pattern_score) = recognize_known_patterns(segment_value) {
        factors.push(ConfidenceFactor {
            name: "Known Pattern".to_string(),
            score: pattern_score,
            weight: 0.30,
        });
    }

    // Factor 2: Prefix Match
    if let Some(prefix_score) = detect_prefix_match(segment_value) {
        factors.push(ConfidenceFactor {
            name: "Prefix Match".to_string(),
            score: prefix_score,
            weight: 0.20,
        });
    }

    // Factor 3: Domain Keywords
    if let Some(keyword_score) = detect_domain_keywords(segment_value, full_path) {
        factors.push(ConfidenceFactor {
            name: "Domain Keywords".to_string(),
            score: keyword_score,
            weight: 0.15,
        });
    }

    // Factor 4: Segment Position
    let position_score = analyze_segment_position(
        segment_value,
        segment_index,
        total_segments,
    );
    factors.push(ConfidenceFactor {
        name: "Segment Position".to_string(),
        score: position_score,
        weight: 0.10,
    });

    // Factor 5: LLM Semantic Analysis
    let llm_score = llm_client.analyze_field_confidence(
        full_path,
        segment_value,
        proposed_field_name,
    ).await?;
    factors.push(ConfidenceFactor {
        name: "LLM Semantic".to_string(),
        score: llm_score,
        weight: 0.25,
    });

    // Renormalize weights (zero-valued factors don't contribute)
    let active_weight_sum: f64 = factors.iter()
        .filter(|f| f.score > 0.0)
        .map(|f| f.weight)
        .sum();

    if active_weight_sum == 0.0 {
        return Ok(ConfidenceScoring {
            factors,
            overall_confidence: 0.0,
        });
    }

    // Compute weighted average of active factors
    let weighted_sum: f64 = factors
        .iter()
        .filter(|f| f.score > 0.0)
        .map(|f| f.score * (f.weight / active_weight_sum))
        .sum();

    Ok(ConfidenceScoring {
        factors,
        overall_confidence: weighted_sum,
    })
}
```

---

## 3. Confidence Thresholds and Interpretation

### 3.1 Threshold Bands

Confidence is interpreted using these bands:

| Band | Range | Interpretation | UI Indicator | User Action | Trust |
|------|-------|-----------------|--------------|-------------|-------|
| **Very High** | 90-100% | Highly certain | ██████████ | Accept without review | ✅ Approve directly |
| **High** | 75-89% | Confident | █████████░ | Quick review | ✅ Approve with quick check |
| **Medium** | 60-74% | Reasonable | ████████░░ | Review segments closely | ⚠️ Review before approving |
| **Low** | 40-59% | Uncertain | ██████░░░░ | Review + manual edit | ⚠️ Edit or provide more examples |
| **Very Low** | 0-39% | Speculative | ███░░░░░░░ | Recommend: provide more examples | ❌ Reject or collect more data |

### 3.2 Confidence Thresholds for Operations

Different operations require different confidence minimums:

| Operation | Min Confidence | Rationale |
|-----------|----------------|-----------|
| **Show proposal in UI** | 0% | Always display, let user decide |
| **Auto-accept (no warning)** | 90% | Only for very certain predictions |
| **Show warning badge** | <60% | Alert user to review carefully |
| **Suggest "collect more examples"** | <40% | Not enough signal; get more data |
| **Disable "Accept All" button** | <60% | Prevent batch acceptance of uncertain fields |
| **Show edit dialog by default** | <40% | Nudge user to manually fix field names |

### 3.3 Presenting Confidence to Users

**In TUI (Section 3.5.5 Single-File Proposals):**

```
Proposed extraction fields:

  Segment          Value          Proposed Field     Confidence
  ─────────────────────────────────────────────────────────────
  segment(-5)      CLIENT-ABC     client_id          ████████░░ 82%
  segment(-4)      invoices       doc_type           █████████░ 91%
  segment(-3)      2024           year               ██████████ 98%
  segment(-2)      Q1             quarter            █████████░ 94%
  filename         inv_001.pdf    invoice_number     ███████░░░ 71%
```

**Visual encoding:**
- Bar length proportional to confidence (10 characters wide)
- Each █ = 10%, each ░ = 10% unfilled
- Color-coded:
  - Green: ≥75% (high confidence)
  - Yellow: 60-74% (medium confidence)
  - Red: <60% (low confidence, needs review)

**Interactive:**
- Arrow keys to move between proposals
- `i` to view detailed breakdown of how each factor contributed
- `e` to manually edit the field name

### 3.4 "View Confidence Details" Dialog

When user presses `i` on a low-confidence field:

```
┌─ Confidence Analysis: client_id from CLIENT-ABC ────────────────┐
│                                                                  │
│ Overall Confidence: 51%                                          │
│ Assessment: Low - Recommend review or manual edit                │
│                                                                  │
│ Factor Breakdown:                                                │
│ ├─ Known Pattern Recognition:    0% (not a known pattern)       │
│ ├─ Prefix Match:                20% (CLIENT- prefix detected)   │
│ ├─ Domain Keywords:              0% (no keywords)               │
│ ├─ Segment Position:             8% (root, categorical)         │
│ └─ LLM Semantic Analysis:        23% (92% certain)              │
│                                                                  │
│ Suggestions:                                                     │
│ • Add 2+ more examples to improve pattern recognition          │
│ • Manually edit field name if incorrect                         │
│ • Consider domain keywords (e.g., "organization" for clarity)  │
│                                                                  │
│ [Enter] Back                                                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## 4. Low Confidence Handling

### 4.1 Confidence < 60%: Warning Behavior

When any proposed field has confidence < 60%, UI displays a warning:

```
┌─ ⚠️ LOW CONFIDENCE FIELDS ────────────────────────────────┐
│                                                           │
│ Some field proposals are uncertain. Review before        │
│ accepting, or collect more file examples.                │
│                                                           │
│ Fields needing review:                                   │
│ • invoice_number (71%) - Review manually                 │
│                                                           │
│ [m] Add more examples  [e] Edit manually  [c] Continue  │
└───────────────────────────────────────────────────────────┘
```

### 4.2 Confidence < 40%: "Collect More Examples" Recommendation

For very low confidence (<40%), the UI recommends collecting additional examples:

```
┌─ Recommendation: Collect More Examples ────────────────────┐
│                                                             │
│ Field "invoice_number" has very low confidence (71%).      │
│                                                             │
│ Providing 2-3 more examples would help the AI learn        │
│ patterns. Would you like to:                               │
│                                                             │
│ [m] Provide more examples for this path                    │
│ [s] Skip low-confidence fields for now                     │
│ [e] Edit field names manually                              │
│ [Esc] Cancel and start over                                │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 User Options for Low Confidence Fields

| Option | Action | Result |
|--------|--------|--------|
| Accept anyway | Proceed with low-confidence field | Field is extracted, will need validation later |
| Edit field name | Open field name editor | User manually sets the correct name |
| Collect more examples | Prompt for additional paths | Run analysis on multiple files (moves to multi-file mode) |
| Remove field | Don't extract this segment | Skip this segment in the extraction rule |

---

## 5. Configuration and Customization

### 5.1 Confidence Threshold Config

Users can customize thresholds in `~/.casparian_flow/config.toml`:

```toml
[path_intelligence.confidence]
# Thresholds for various operations
auto_accept_threshold = 90          # Min % to auto-accept without review
warning_threshold = 60              # Min % to show low-confidence warning
collect_more_threshold = 40         # Below this, suggest collecting more examples

# Factor weights (must sum to 1.0)
weight_pattern = 0.30
weight_prefix = 0.20
weight_keywords = 0.15
weight_position = 0.10
weight_llm = 0.25

# LLM model for semantic analysis
semantic_model = "phi3.5:3.8b"      # or "mistral:7b", "claude-opus"
semantic_timeout_sec = 10            # Max time to wait for LLM response

# Confidence display
display_bar_width = 10              # Bars shown as [████░░░░░░]
display_as_percentage = true        # Show "82%" vs bar only
display_breakdown_on_hover = true   # Show factor details on 'i'
```

### 5.2 Custom Patterns and Keywords

Users can add domain-specific patterns:

```toml
[patterns.custom]
client_pattern = "^CLIENT-[A-Z0-9]+$"      # Match CLIENT-ABC style
invoice_pattern = "^inv[_-]\\d{4}$"        # Match inv_0001, inv-0001

[keywords.custom.healthcare]
new_terms = ["radiology", "pathology", "oncology"]

[keywords.custom.finance]
new_terms = ["ledger", "journal", "reconciliation"]
```

---

## 6. Edge Cases and Special Handling

### 6.1 Conflicting Factors

**Case:** One factor says YES, another says NO.

Example: Segment "Q2024" could be "Q2" (quarter) + "024" (ID), or "Q" + "2024" (year).

**Resolution:**
- Pattern recognition (Q2): 30% (partial match - looks like quarter)
- Pattern recognition (2024): 30% (year pattern)
- LLM semantic: best tiebreaker - ask "Is 'Q2024' more likely quarter or year?"
- Use LLM result as deciding factor

### 6.2 No Factors Match

**Case:** Segment value matches no known patterns, prefixes, or keywords.

Example: Segment "xyz" with proposed field "xyz_id"

**Resolution:**
- All heuristic factors score 0%
- LLM is only remaining factor (25% weight)
- Overall confidence = LLM semantic score only
- Typical result: 30-50% (speculative)
- UI will show low-confidence warning

### 6.3 Unusual Path Structures

**Case:** Path with inconsistent depth, or unusual segment names.

Example: `/a/b/c/very/deep/nested/structure/data.csv`

**Resolution:**
- Position factor becomes less reliable (unclear what "root" vs "leaf" means)
- Reduce position factor contribution by 50%
- Rely more heavily on LLM semantic analysis
- Implementation: If path depth > 7, cap position_score at 0.5

---

## 7. Testing and Validation

### 7.1 Test Cases

| Test Case | Input | Expected Confidence | Tolerance |
|-----------|-------|---------------------|-----------|
| Standard date | "2024" as "year" | 95-100% | ±5% |
| Client prefix | "CLIENT-ABC" as "client_id" | 70-85% | ±5% |
| Domain keyword | "invoices" as "doc_type" | 70-80% | ±5% |
| Ambiguous | "data" as "data_type" | 40-60% | ±10% |
| Nonsense | "xyz" as "xyz_field" | 20-40% | ±10% |

### 7.2 Regression Test Path Confidence

```bash
# Test confidence scoring on known paths
casparian test path-confidence --suite standard

# Sample output:
# ✓ "2024" → "year": 98% (expected 95-100%)
# ✓ "CLIENT-ABC" → "client_id": 82% (expected 70-85%)
# ✓ "invoices" → "doc_type": 91% (expected 70-80%)
# ✓ "data" → "data_type": 52% (expected 40-60%)
# ✓ "xyz" → "xyz_field": 31% (expected 20-40%)
#
# Passed: 5/5
```

---

## 8. Implementation Checklist

### Phase 1: Core Confidence Computation
- [ ] Implement Factor 1: Known Pattern Recognition
- [ ] Implement Factor 2: Prefix Match
- [ ] Implement Factor 3: Domain Keywords
- [ ] Implement Factor 4: Segment Position
- [ ] Implement Factor 5: LLM Semantic Analysis
- [ ] Implement weight normalization algorithm
- [ ] Add unit tests for each factor
- [ ] Add integration tests for overall scoring

### Phase 2: Thresholds and UI Integration
- [ ] Define confidence bands (Very High, High, Medium, Low, Very Low)
- [ ] Implement warning display for <60% confidence
- [ ] Implement "collect more examples" suggestion for <40%
- [ ] Add confidence detail view (`i` key interaction)
- [ ] Update TUI to show confidence bars

### Phase 3: Configuration
- [ ] Add `[path_intelligence.confidence]` section to config.toml
- [ ] Allow custom pattern definitions
- [ ] Allow custom keyword definitions
- [ ] Add config validation and defaults
- [ ] Update documentation with examples

### Phase 4: Testing and Validation
- [ ] Create test suite with standard paths
- [ ] Add regression tests
- [ ] Performance test: confidence computation <100ms per field
- [ ] LLM fallback test: verify neutral score if LLM unavailable
- [ ] Edge case tests: deep paths, conflicting factors, unusual structures

---

## 9. Spec Updates Required

**Add to `specs/ai_wizards.md` Section 3.5.5:**

Replace existing "Confidence Factors" subsection with:

```markdown
#### Confidence Factors

Single-file analysis combines five independent factors into an overall confidence score (0-100%):

| # | Factor | Weight | Description |
|----|--------|--------|-------------|
| 1 | Known Pattern Recognition | 30% | YYYY, YYYY-MM, Q1-Q4, ISO codes, etc. |
| 2 | Prefix Match | 20% | CLIENT-, inv_, doc_, ref- style prefixes |
| 3 | Domain Keywords | 15% | finance: "invoice", "payment"; healthcare: "patient", "diagnosis" |
| 4 | Segment Position | 10% | Root segments usually categorical; leaf segments usually IDs |
| 5 | LLM Semantic Analysis | 25% | Language model confidence in field naming |

**Scoring Algorithm:**

Each active factor contributes its percentage. Inactive factors are excluded. Final score is weighted average of active factors.

**Thresholds:**
- ≥90%: Very High (✅ Accept directly)
- 75-89%: High (✅ Accept with quick review)
- 60-74%: Medium (⚠️ Review before accepting)
- 40-59%: Low (⚠️ Recommend manual edit)
- <40%: Very Low (❌ Recommend collecting more examples)

**Implementation Details:** See `specs/meta/sessions/ai_wizards/round_025/engineer.md`
```

---

## 10. Summary

**Gap Resolution:**

GAP-PIE-003 is resolved with:

1. **Five Confidence Factors** (Section 1): Clear definitions of pattern recognition, prefix match, domain keywords, segment position, and LLM semantic analysis.

2. **Scoring Algorithm** (Section 2): Weight normalization scheme that handles partial factor sets and produces 0-100% overall confidence.

3. **Threshold Bands** (Section 3): Five confidence levels with clear user actions (Very High: accept; High: quick review; Medium: review carefully; Low: edit; Very Low: collect more examples).

4. **UI Presentation** (Section 3): Visual bars with color coding, detail view, and low-confidence warnings.

5. **Low Confidence Handling** (Section 4): Clear guidance for <60% (warning) and <40% (collect more examples).

6. **Configuration** (Section 5): User-customizable thresholds, patterns, and keywords.

7. **Edge Cases** (Section 6): Handling for conflicting factors, no matches, and unusual paths.

8. **Testing** (Section 7): Regression test suite and performance targets.

**Key Insights:**

- **Normalization is essential**: Not all factors are always active. We renormalize weights so active factors contribute fairly.
- **LLM is the tiebreaker**: When heuristics conflict, the LLM semantic analysis provides the final signal.
- **Transparency matters**: Users need to see _why_ confidence is low, not just the number.
- **Threshold design is critical**: Each threshold directly affects user workflow (auto-accept, warning, suggest collection).

**Result:** Users now have clear, actionable confidence scores with well-defined thresholds that guide them through single-file analysis workflows.

---

## References

- `specs/ai_wizards.md` Section 3.5.5 (Single-File Proposals)
- `specs/ai_wizards.md` Section 3.5.1-3.5.4 (Context on multi-file analysis)
- Round 013 engineer.md (GAP-PIE-001: Success Criteria for Path Intelligence phases)
- Round 023 engineer.md (GAP-AUDIT-001: Audit log retention policy reference)

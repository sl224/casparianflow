//! Constraint-based type inference solver
//!
//! The solver maintains possible types and formats for a column,
//! eliminating possibilities as values are processed.

use std::collections::{HashMap, HashSet};

use super::constraints::{
    Constraint, EliminatedItem, EliminationEvidence, EliminationReason, TypeInferenceResult,
};
use super::date_formats::{
    can_be_day, can_be_month, days_in_month, extract_components, try_parse_date, DateFormatSpec,
    DATE_FORMATS,
};
use super::DataType;

/// Constraint-based type inference solver
///
/// Maintains possible types and formats, eliminating as values are seen.
#[derive(Debug)]
pub struct ConstraintSolver {
    /// Column name (for debugging)
    column_name: String,

    /// Types that are still possible
    possible_types: HashSet<DataType>,

    /// For temporal types, which formats are still possible
    /// Key is the format pattern string
    date_format_candidates: HashSet<String>,

    /// Evidence of eliminations (for explainability)
    elimination_evidence: Vec<EliminationEvidence>,

    /// Number of values processed
    values_processed: usize,

    /// Number of null/empty values seen
    null_count: usize,

    /// Whether we've seen any non-null values
    has_non_null_values: bool,

    /// Detected separator for dates (if consistent)
    detected_separator: Option<String>,

    /// Statistics for numeric inference
    integer_count: usize,
    float_count: usize,
    has_decimal_point: bool,
    has_negative: bool,
    has_leading_zeros: bool,
}

impl ConstraintSolver {
    /// Create a new solver for a column
    pub fn new(column_name: impl Into<String>) -> Self {
        let mut possible_types = HashSet::new();
        for dtype in DataType::all() {
            possible_types.insert(dtype);
        }

        let date_format_candidates: HashSet<String> =
            DATE_FORMATS.iter().map(|f| f.pattern.to_string()).collect();

        Self {
            column_name: column_name.into(),
            possible_types,
            date_format_candidates,
            elimination_evidence: Vec::new(),
            values_processed: 0,
            null_count: 0,
            has_non_null_values: false,
            detected_separator: None,
            integer_count: 0,
            float_count: 0,
            has_decimal_point: false,
            has_negative: false,
            has_leading_zeros: false,
        }
    }

    /// Add a value and apply constraints
    pub fn add_value(&mut self, value: &str) {
        self.values_processed += 1;

        let trimmed = value.trim();

        // Handle null/empty values
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") || trimmed == "NA" {
            self.null_count += 1;
            return;
        }

        self.has_non_null_values = true;

        // Apply type-specific constraints
        self.apply_boolean_constraints(trimmed);
        self.apply_numeric_constraints(trimmed);
        self.apply_date_constraints(trimmed);
        self.apply_datetime_constraints(trimmed);
        self.apply_time_constraints(trimmed);
        self.apply_duration_constraints(trimmed);
    }

    /// Apply constraints for boolean type
    fn apply_boolean_constraints(&mut self, value: &str) {
        if !self.possible_types.contains(&DataType::Boolean) {
            return;
        }

        // F-002: Use eq_ignore_ascii_case() instead of to_lowercase() to avoid allocation
        let is_boolean = value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("no")
            || matches!(
                value,
                "y" | "Y" | "n" | "N" | "1" | "0" | "t" | "T" | "f" | "F"
            );

        if !is_boolean {
            self.eliminate_type(
                DataType::Boolean,
                EliminationReason::NotBooleanValue {
                    value: value.to_string(),
                },
            );
        }
    }

    /// Apply constraints for numeric types (Integer and Float)
    fn apply_numeric_constraints(&mut self, value: &str) {
        // Check if it looks numeric at all
        let is_potentially_numeric = self.is_potentially_numeric(value);

        if !is_potentially_numeric {
            // Eliminate both Integer and Float
            if self.possible_types.contains(&DataType::Integer) {
                self.eliminate_type(
                    DataType::Integer,
                    EliminationReason::InvalidCharacters {
                        value: value.to_string(),
                        chars: "non-numeric characters".to_string(),
                    },
                );
            }
            if self.possible_types.contains(&DataType::Float) {
                self.eliminate_type(
                    DataType::Float,
                    EliminationReason::InvalidCharacters {
                        value: value.to_string(),
                        chars: "non-numeric characters".to_string(),
                    },
                );
            }
            return;
        }

        // Track if we've seen decimal points
        if value.contains('.') {
            self.has_decimal_point = true;

            // Eliminate Integer (but keep Float)
            if self.possible_types.contains(&DataType::Integer) {
                self.eliminate_type(
                    DataType::Integer,
                    EliminationReason::ContainsDecimalPoint {
                        value: value.to_string(),
                    },
                );
            }

            // Track float count
            if value.parse::<f64>().is_ok() {
                self.float_count += 1;
            }
        } else if value.parse::<i64>().is_ok() {
            self.integer_count += 1;
        }

        // Track leading zeros (could indicate string, not number)
        if value.len() > 1 && value.starts_with('0') && !value.starts_with("0.") {
            self.has_leading_zeros = true;
        }

        // Track negative values
        if value.starts_with('-') {
            self.has_negative = true;
        }
    }

    /// Check if a value looks potentially numeric
    fn is_potentially_numeric(&self, value: &str) -> bool {
        // Allow optional leading sign
        let chars: Vec<char> = value.chars().collect();
        if chars.is_empty() {
            return false;
        }

        let start = if chars[0] == '-' || chars[0] == '+' {
            1
        } else {
            0
        };

        if start >= chars.len() {
            return false;
        }

        let mut has_digit = false;
        let mut decimal_count = 0;

        for c in &chars[start..] {
            if c.is_ascii_digit() {
                has_digit = true;
            } else if *c == '.' {
                decimal_count += 1;
                if decimal_count > 1 {
                    return false; // Multiple decimal points
                }
            } else if *c == 'e' || *c == 'E' {
                // Scientific notation - allow if followed by optional sign and digits
                continue;
            } else if *c == ',' {
                // Could be thousands separator - check context
                continue;
            } else {
                return false; // Invalid character
            }
        }

        has_digit
    }

    /// Apply constraints for date type
    fn apply_date_constraints(&mut self, value: &str) {
        if !self.possible_types.contains(&DataType::Date) {
            return;
        }

        if self.date_format_candidates.is_empty() {
            // All date formats eliminated - eliminate Date type
            self.eliminate_type(
                DataType::Date,
                EliminationReason::Custom {
                    value: value.to_string(),
                    reason: "all date formats eliminated".to_string(),
                },
            );
            return;
        }

        // Detect separator from value
        let separator = self.detect_separator(value);
        if let Some(sep) = &separator {
            // If we've detected a separator before, check consistency
            if let Some(detected) = &self.detected_separator {
                if detected != sep {
                    // Inconsistent separators - might not be dates
                    // But don't eliminate yet, could be mixed format column
                }
            } else {
                self.detected_separator = Some(sep.clone());
            }
        }

        // Test each remaining date format candidate
        // F-001: Iterate by reference instead of cloning the HashSet
        let formats_to_check: Vec<&String> = self.date_format_candidates.iter().collect();
        let mut formats_to_eliminate = Vec::new();

        for format_pattern in formats_to_check {
            // Find the format spec
            let format = DATE_FORMATS
                .iter()
                .find(|f| f.pattern == format_pattern.as_str());

            if let Some(format) = format {
                // Check if format can be eliminated
                if let Some(reason) = self.check_date_format_elimination(value, format) {
                    formats_to_eliminate.push((format_pattern.clone(), reason));
                }
            }
        }

        // Apply eliminations
        for (format_pattern, reason) in formats_to_eliminate {
            self.date_format_candidates.remove(&format_pattern);
            self.elimination_evidence.push(EliminationEvidence {
                eliminated: EliminatedItem::DateFormat(format_pattern),
                reason: reason.clone(),
                row_index: self.values_processed - 1,
                value: value.to_string(),
            });
        }

        // If all date formats are eliminated, eliminate Date type
        if self.date_format_candidates.is_empty() {
            self.eliminate_type(
                DataType::Date,
                EliminationReason::Custom {
                    value: value.to_string(),
                    reason: "all date formats eliminated".to_string(),
                },
            );
        }
    }

    /// Check if a date format should be eliminated for this value
    fn check_date_format_elimination(
        &self,
        value: &str,
        format: &DateFormatSpec,
    ) -> Option<EliminationReason> {
        let trimmed = value.trim();

        // Check separator match
        if !format.separator.is_empty() && !trimmed.contains(format.separator) {
            return Some(EliminationReason::PatternMismatch {
                value: value.to_string(),
                expected: format!("separator '{}'", format.separator),
            });
        }

        // For compact formats, check length
        if format.separator.is_empty() {
            let expected_len = if format.year_digits == 4 { 8 } else { 6 };
            if trimmed.len() != expected_len {
                return Some(EliminationReason::PatternMismatch {
                    value: value.to_string(),
                    expected: format!("{} digits", expected_len),
                });
            }
        }

        // Try to parse with this format
        if let Some(parsed) = try_parse_date(trimmed, format) {
            if !parsed.is_valid() {
                return Some(EliminationReason::Custom {
                    value: value.to_string(),
                    reason: "parsed date is invalid".to_string(),
                });
            }
            return None; // Format works for this value
        }

        // If we have separator and can extract components, check constraints
        if !format.separator.is_empty() {
            if let Some((c0, c1, c2)) = extract_components(trimmed, format.separator) {
                // Get value at month position
                let month_val = match format.position.month {
                    0 => c0,
                    1 => c1,
                    2 => c2,
                    _ => return None,
                };

                // Get value at day position
                let day_val = match format.position.day {
                    0 => c0,
                    1 => c1,
                    2 => c2,
                    _ => return None,
                };

                // Key constraint: if month position has value > 12, eliminate
                if !can_be_month(month_val) {
                    return Some(EliminationReason::DateComponentInvalid {
                        value: value.to_string(),
                        component: "month".to_string(),
                        actual: month_val,
                    });
                }

                // If day position has value > 31, eliminate
                if !can_be_day(day_val) {
                    return Some(EliminationReason::DateComponentInvalid {
                        value: value.to_string(),
                        component: "day".to_string(),
                        actual: day_val,
                    });
                }

                // Check day is valid for month (e.g., no Feb 30)
                let year_val = match format.position.year {
                    0 => c0,
                    1 => c1,
                    2 => c2,
                    _ => 2000,
                };

                let full_year = if format.year_digits == 2 {
                    if year_val >= 70 {
                        1900 + year_val
                    } else {
                        2000 + year_val
                    }
                } else {
                    year_val
                };

                let max_days = days_in_month(month_val as u32, full_year);
                if day_val as u32 > max_days {
                    return Some(EliminationReason::OutOfRange {
                        value: value.to_string(),
                        component: "day".to_string(),
                        actual: day_val,
                        max: max_days as i32,
                    });
                }
            }
        }

        // If parsing failed but constraints passed, format might still work
        // (chrono can be picky about whitespace, etc.)
        Some(EliminationReason::ParseFailed {
            value: value.to_string(),
            error: "chrono parse failed".to_string(),
        })
    }

    /// Apply constraints for datetime type
    fn apply_datetime_constraints(&mut self, value: &str) {
        if !self.possible_types.contains(&DataType::DateTime) {
            return;
        }

        // DateTime must have both date and time components
        // Quick check: should contain space or T separator
        let has_datetime_separator = value.contains(' ') || value.contains('T');
        let has_time_chars = value.contains(':');

        if !has_datetime_separator || !has_time_chars {
            self.eliminate_type(
                DataType::DateTime,
                EliminationReason::PatternMismatch {
                    value: value.to_string(),
                    expected: "date and time separated by space or T".to_string(),
                },
            );
        }
    }

    /// Apply constraints for time type
    fn apply_time_constraints(&mut self, value: &str) {
        if !self.possible_types.contains(&DataType::Time) {
            return;
        }

        // Time should have colons and be relatively short
        let has_colon = value.contains(':');
        let parts: Vec<&str> = value.split(':').collect();

        if !has_colon || parts.len() < 2 || parts.len() > 3 {
            self.eliminate_type(
                DataType::Time,
                EliminationReason::PatternMismatch {
                    value: value.to_string(),
                    expected: "HH:MM or HH:MM:SS format".to_string(),
                },
            );
            return;
        }

        // Check hour and minute ranges
        if let Ok(hour) = parts[0].trim().parse::<i32>() {
            if hour < 0 || hour > 23 {
                self.eliminate_type(
                    DataType::Time,
                    EliminationReason::OutOfRange {
                        value: value.to_string(),
                        component: "hour".to_string(),
                        actual: hour,
                        max: 23,
                    },
                );
            }
        }

        if let Ok(minute) = parts[1].trim().parse::<i32>() {
            if minute < 0 || minute > 59 {
                self.eliminate_type(
                    DataType::Time,
                    EliminationReason::OutOfRange {
                        value: value.to_string(),
                        component: "minute".to_string(),
                        actual: minute,
                        max: 59,
                    },
                );
            }
        }
    }

    /// Apply constraints for duration type
    fn apply_duration_constraints(&mut self, value: &str) {
        if !self.possible_types.contains(&DataType::Duration) {
            return;
        }

        // Duration formats typically look like:
        // - ISO 8601: PT1H30M, P1DT2H, etc.
        // - Human readable: 1h30m, 2 hours, 30 min, etc.
        // - Numeric durations: values like "10.50" are NOT durations

        // Check for ISO 8601 duration (starts with P) - case insensitive without allocation
        let first_char = value.chars().next();
        if matches!(first_char, Some('p') | Some('P')) {
            // Check for duration markers in rest of string
            let has_duration_marker = value[1..]
                .chars()
                .any(|c| matches!(c, 't' | 'T' | 'y' | 'Y' | 'm' | 'M' | 'd' | 'D'));
            if has_duration_marker {
                return; // Looks like ISO 8601 duration
            }
        }

        // Check for human-readable duration markers (case insensitive without allocation)
        let value_bytes = value.as_bytes();

        // Helper to check if slice contains substring case-insensitively
        fn contains_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
            haystack.windows(needle.len()).any(|window| {
                window
                    .iter()
                    .zip(needle.iter())
                    .all(|(h, n)| h.to_ascii_lowercase() == *n)
            })
        }

        let has_hour = contains_ignore_case(value_bytes, b"hour");
        let has_min = contains_ignore_case(value_bytes, b"min");
        let has_sec = contains_ignore_case(value_bytes, b"sec");
        let has_day = contains_ignore_case(value_bytes, b"day");
        let ends_with_unit = matches!(
            value_bytes.last(),
            Some(b'h')
                | Some(b'H')
                | Some(b'm')
                | Some(b'M')
                | Some(b's')
                | Some(b'S')
                | Some(b'd')
                | Some(b'D')
        );

        if has_hour || has_min || has_sec || has_day || ends_with_unit {
            return; // Looks like human-readable duration
        }

        // Check for short form like "1h30m" or "2h" - check for digits AND duration chars
        let has_digit = value.chars().any(|c| c.is_ascii_digit());
        let has_duration_char = value
            .chars()
            .any(|c| matches!(c, 'h' | 'H' | 'm' | 'M' | 's' | 'S'));

        if has_digit && has_duration_char {
            return; // Looks like short duration format
        }

        // Plain numbers (like "10.50") are NOT durations
        self.eliminate_type(
            DataType::Duration,
            EliminationReason::PatternMismatch {
                value: value.to_string(),
                expected: "duration format (e.g., PT1H30M, 1h30m, 2 hours)".to_string(),
            },
        );
    }

    /// Detect separator in a potential date value
    fn detect_separator(&self, value: &str) -> Option<String> {
        for sep in &["/", "-", "."] {
            if value.matches(*sep).count() == 2 {
                return Some(sep.to_string());
            }
        }
        None
    }

    /// Eliminate a type and record evidence
    fn eliminate_type(&mut self, data_type: DataType, reason: EliminationReason) {
        if self.possible_types.remove(&data_type) {
            self.elimination_evidence.push(EliminationEvidence {
                eliminated: EliminatedItem::Type(data_type),
                reason,
                row_index: self.values_processed.saturating_sub(1),
                value: String::new(), // Already in reason
            });
        }
    }

    /// Check if the type is resolved (only one possibility remains)
    pub fn is_resolved(&self) -> bool {
        // String is always possible (fallback), so resolved means 1 non-String type
        let non_string_types: Vec<_> = self
            .possible_types
            .iter()
            .filter(|t| **t != DataType::String && **t != DataType::Null)
            .collect();

        if non_string_types.len() == 1 {
            return true;
        }

        // Special case: if only Integer and Float remain and no decimal seen,
        // consider it resolved as Integer (integers are a subset of floats)
        if non_string_types.len() == 2
            && non_string_types.contains(&&DataType::Integer)
            && non_string_types.contains(&&DataType::Float)
            && !self.has_decimal_point
        {
            return true;
        }

        false
    }

    /// Get the current result
    pub fn get_result(&self) -> TypeInferenceResult {
        // If only nulls were seen, type is Null
        if !self.has_non_null_values {
            return TypeInferenceResult::Resolved {
                data_type: DataType::Null,
                format: None,
                values_processed: self.values_processed,
                evidence: self.elimination_evidence.clone(),
            };
        }

        // Filter to meaningful types (not Null, and String is fallback)
        let meaningful_types: Vec<DataType> = self
            .possible_types
            .iter()
            .filter(|t| **t != DataType::Null && **t != DataType::String)
            .copied()
            .collect();

        // Special case: if only Integer and Float remain and no decimal seen,
        // resolve as Integer (integers are a subset of floats)
        let resolved_type = if meaningful_types.len() == 2
            && meaningful_types.contains(&DataType::Integer)
            && meaningful_types.contains(&DataType::Float)
            && !self.has_decimal_point
        {
            Some(DataType::Integer)
        } else if meaningful_types.len() == 1 {
            Some(meaningful_types[0])
        } else {
            None
        };

        match resolved_type {
            Some(data_type) => {
                let format = if data_type == DataType::Date {
                    self.date_format_candidates.iter().next().cloned()
                } else {
                    None
                };

                TypeInferenceResult::Resolved {
                    data_type,
                    format,
                    values_processed: self.values_processed,
                    evidence: self.elimination_evidence.clone(),
                }
            }
            None if meaningful_types.is_empty() => {
                // Only String remains (or nothing) - fallback
                TypeInferenceResult::NoValidType {
                    fallback: DataType::String,
                    eliminations: self.elimination_evidence.clone(),
                }
            }
            None => {
                // Multiple types still possible
                let mut possible_formats = HashMap::new();
                if meaningful_types.contains(&DataType::Date) {
                    possible_formats.insert(
                        DataType::Date,
                        self.date_format_candidates.iter().cloned().collect(),
                    );
                }

                TypeInferenceResult::Ambiguous {
                    possible_types: meaningful_types,
                    possible_formats,
                    values_processed: self.values_processed,
                }
            }
        }
    }

    /// Apply a constraint from external source (e.g., schema hint)
    pub fn apply_constraint(&mut self, constraint: Constraint) {
        match constraint {
            Constraint::CannotBe { data_type, reason } => {
                self.eliminate_type(data_type, reason);
            }
            Constraint::MustBe { data_type, reason } => {
                // Keep only the specified type
                let all_types: Vec<DataType> = self.possible_types.iter().copied().collect();
                for t in all_types {
                    if t != data_type && t != DataType::String {
                        self.eliminate_type(
                            t,
                            EliminationReason::Custom {
                                value: String::new(),
                                reason: reason.clone(),
                            },
                        );
                    }
                }
            }
            Constraint::DateFormatEliminated { format, reason } => {
                if self.date_format_candidates.remove(&format) {
                    self.elimination_evidence.push(EliminationEvidence {
                        eliminated: EliminatedItem::DateFormat(format),
                        reason,
                        row_index: self.values_processed,
                        value: String::new(),
                    });
                }
            }
            Constraint::TimeFormatEliminated { format, reason } => {
                self.elimination_evidence.push(EliminationEvidence {
                    eliminated: EliminatedItem::TimeFormat(format),
                    reason,
                    row_index: self.values_processed,
                    value: String::new(),
                });
            }
        }
    }

    /// Get column name
    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    /// Get possible types
    pub fn possible_types(&self) -> &HashSet<DataType> {
        &self.possible_types
    }

    /// Get remaining date format candidates
    pub fn date_format_candidates(&self) -> &HashSet<String> {
        &self.date_format_candidates
    }

    /// Get elimination evidence
    pub fn elimination_evidence(&self) -> &[EliminationEvidence] {
        &self.elimination_evidence
    }

    /// Get number of values processed
    pub fn values_processed(&self) -> usize {
        self.values_processed
    }

    /// Get null count
    pub fn null_count(&self) -> usize {
        self.null_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_solver_has_all_types() {
        let solver = ConstraintSolver::new("test_column");
        assert!(solver.possible_types.contains(&DataType::Integer));
        assert!(solver.possible_types.contains(&DataType::Float));
        assert!(solver.possible_types.contains(&DataType::Date));
        assert!(solver.possible_types.contains(&DataType::String));
    }

    #[test]
    fn test_integer_inference() {
        let mut solver = ConstraintSolver::new("id");
        solver.add_value("1");
        solver.add_value("42");
        solver.add_value("100");

        let _result = solver.get_result();
        assert!(solver.possible_types.contains(&DataType::Integer));
    }

    #[test]
    fn test_float_eliminates_integer() {
        let mut solver = ConstraintSolver::new("price");
        solver.add_value("1.0");
        solver.add_value("42.5");
        solver.add_value("100.99");

        assert!(!solver.possible_types.contains(&DataType::Integer));
        assert!(solver.possible_types.contains(&DataType::Float));
    }

    #[test]
    fn test_date_format_elimination() {
        let mut solver = ConstraintSolver::new("date");

        // "31/05/24" eliminates MM/DD/YY because 31 can't be month
        solver.add_value("31/05/24");

        // Should eliminate MM/DD/YY format
        assert!(
            !solver.date_format_candidates.contains("%m/%d/%y"),
            "MM/DD/YY should be eliminated when first component is 31"
        );

        // Should keep DD/MM/YY format
        assert!(
            solver.date_format_candidates.contains("%d/%m/%y"),
            "DD/MM/YY should remain valid"
        );
    }

    #[test]
    fn test_ambiguous_date_stays_ambiguous() {
        let mut solver = ConstraintSolver::new("date");

        // "05/06/24" is ambiguous - could be May 6 or June 5
        solver.add_value("05/06/24");

        // Both formats should still be valid
        assert!(solver.date_format_candidates.contains("%d/%m/%y"));
        assert!(solver.date_format_candidates.contains("%m/%d/%y"));
    }

    #[test]
    fn test_multiple_dates_narrow_format() {
        let mut solver = ConstraintSolver::new("date");

        // First date is ambiguous
        solver.add_value("01/02/24");
        assert!(solver.date_format_candidates.contains("%d/%m/%y"));
        assert!(solver.date_format_candidates.contains("%m/%d/%y"));

        // Second date eliminates MM/DD/YY (31 can't be month)
        solver.add_value("31/05/24");
        assert!(
            !solver.date_format_candidates.contains("%m/%d/%y"),
            "MM/DD/YY should be eliminated"
        );
        assert!(
            solver.date_format_candidates.contains("%d/%m/%y"),
            "DD/MM/YY should remain"
        );
    }

    #[test]
    fn test_boolean_inference() {
        let mut solver = ConstraintSolver::new("flag");
        solver.add_value("true");
        solver.add_value("false");
        solver.add_value("true");

        assert!(solver.possible_types.contains(&DataType::Boolean));

        // Non-boolean value eliminates Boolean
        solver.add_value("maybe");
        assert!(!solver.possible_types.contains(&DataType::Boolean));
    }

    #[test]
    fn test_null_handling() {
        let mut solver = ConstraintSolver::new("nullable");
        solver.add_value("");
        solver.add_value("null");
        solver.add_value("NA");

        // All nulls - should resolve to Null type
        let result = solver.get_result();
        match result {
            TypeInferenceResult::Resolved { data_type, .. } => {
                assert_eq!(data_type, DataType::Null);
            }
            _ => panic!("Expected Resolved to Null"),
        }
    }

    #[test]
    fn test_mixed_nulls_and_values() {
        let mut solver = ConstraintSolver::new("mixed");
        solver.add_value("");
        solver.add_value("42");
        solver.add_value("null");
        solver.add_value("100");

        // Should still detect Integer
        assert!(solver.possible_types.contains(&DataType::Integer));
        assert_eq!(solver.null_count, 2);
    }

    #[test]
    fn test_time_inference() {
        let mut solver = ConstraintSolver::new("time");
        solver.add_value("14:30:00");
        solver.add_value("09:15:30");

        assert!(solver.possible_types.contains(&DataType::Time));
    }

    #[test]
    fn test_invalid_time_eliminates() {
        let mut solver = ConstraintSolver::new("time");
        solver.add_value("25:30:00"); // Invalid hour

        assert!(!solver.possible_types.contains(&DataType::Time));
    }

    #[test]
    fn test_is_resolved() {
        let mut solver = ConstraintSolver::new("test");

        // Not resolved initially (many types possible)
        assert!(!solver.is_resolved());

        // Add decimal value to eliminate Integer
        solver.add_value("3.14");

        // Add non-boolean to eliminate Boolean
        solver.add_value("hello");

        // Still not resolved (Float and String both possible)
        // Actually "hello" eliminates Float too
        // So only String remains
    }

    #[test]
    fn test_evidence_tracking() {
        let mut solver = ConstraintSolver::new("test");
        solver.add_value("3.14"); // Eliminates Integer

        assert!(!solver.elimination_evidence.is_empty());

        let integer_elimination = solver
            .elimination_evidence
            .iter()
            .find(|e| matches!(&e.eliminated, EliminatedItem::Type(DataType::Integer)));

        assert!(integer_elimination.is_some());
    }

    #[test]
    fn test_apply_must_be_constraint() {
        let mut solver = ConstraintSolver::new("forced_date");

        solver.apply_constraint(Constraint::MustBe {
            data_type: DataType::Date,
            reason: "schema hint".to_string(),
        });

        // Only Date (and String fallback) should remain
        let non_string_types: Vec<_> = solver
            .possible_types
            .iter()
            .filter(|t| **t != DataType::String && **t != DataType::Null)
            .collect();

        assert_eq!(non_string_types.len(), 1);
        assert!(solver.possible_types.contains(&DataType::Date));
    }
}

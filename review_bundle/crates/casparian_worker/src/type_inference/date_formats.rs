//! Date format detection and parsing
//!
//! This module handles date format inference using constraint-based elimination.
//! When a value like "31/05/24" is seen, formats where 31 could be a month are eliminated.

use chrono::NaiveDate;

/// Common date formats to try (ordered by popularity)
pub const DATE_FORMATS: &[DateFormatSpec] = &[
    // ISO formats
    DateFormatSpec {
        pattern: "%Y-%m-%d",
        example: "2024-05-31",
        position: DateComponentPosition {
            year: 0,
            month: 1,
            day: 2,
        },
        separator: "-",
        year_digits: 4,
    },
    DateFormatSpec {
        pattern: "%Y/%m/%d",
        example: "2024/05/31",
        position: DateComponentPosition {
            year: 0,
            month: 1,
            day: 2,
        },
        separator: "/",
        year_digits: 4,
    },
    // European formats (DD/MM/YYYY)
    DateFormatSpec {
        pattern: "%d/%m/%Y",
        example: "31/05/2024",
        position: DateComponentPosition {
            year: 2,
            month: 1,
            day: 0,
        },
        separator: "/",
        year_digits: 4,
    },
    DateFormatSpec {
        pattern: "%d-%m-%Y",
        example: "31-05-2024",
        position: DateComponentPosition {
            year: 2,
            month: 1,
            day: 0,
        },
        separator: "-",
        year_digits: 4,
    },
    DateFormatSpec {
        pattern: "%d.%m.%Y",
        example: "31.05.2024",
        position: DateComponentPosition {
            year: 2,
            month: 1,
            day: 0,
        },
        separator: ".",
        year_digits: 4,
    },
    // US formats (MM/DD/YYYY)
    DateFormatSpec {
        pattern: "%m/%d/%Y",
        example: "05/31/2024",
        position: DateComponentPosition {
            year: 2,
            month: 0,
            day: 1,
        },
        separator: "/",
        year_digits: 4,
    },
    DateFormatSpec {
        pattern: "%m-%d-%Y",
        example: "05-31-2024",
        position: DateComponentPosition {
            year: 2,
            month: 0,
            day: 1,
        },
        separator: "-",
        year_digits: 4,
    },
    // Short year formats (DD/MM/YY and MM/DD/YY)
    DateFormatSpec {
        pattern: "%d/%m/%y",
        example: "31/05/24",
        position: DateComponentPosition {
            year: 2,
            month: 1,
            day: 0,
        },
        separator: "/",
        year_digits: 2,
    },
    DateFormatSpec {
        pattern: "%m/%d/%y",
        example: "05/31/24",
        position: DateComponentPosition {
            year: 2,
            month: 0,
            day: 1,
        },
        separator: "/",
        year_digits: 2,
    },
    DateFormatSpec {
        pattern: "%d-%m-%y",
        example: "31-05-24",
        position: DateComponentPosition {
            year: 2,
            month: 1,
            day: 0,
        },
        separator: "-",
        year_digits: 2,
    },
    DateFormatSpec {
        pattern: "%m-%d-%y",
        example: "05-31-24",
        position: DateComponentPosition {
            year: 2,
            month: 0,
            day: 1,
        },
        separator: "-",
        year_digits: 2,
    },
    // Year first, short
    DateFormatSpec {
        pattern: "%y/%m/%d",
        example: "24/05/31",
        position: DateComponentPosition {
            year: 0,
            month: 1,
            day: 2,
        },
        separator: "/",
        year_digits: 2,
    },
    // Compact formats (no separator)
    DateFormatSpec {
        pattern: "%Y%m%d",
        example: "20240531",
        position: DateComponentPosition {
            year: 0,
            month: 1,
            day: 2,
        },
        separator: "",
        year_digits: 4,
    },
    DateFormatSpec {
        pattern: "%y%m%d",
        example: "240531",
        position: DateComponentPosition {
            year: 0,
            month: 1,
            day: 2,
        },
        separator: "",
        year_digits: 2,
    },
];

/// Date format specification with component positions
#[derive(Debug, Clone)]
pub struct DateFormatSpec {
    /// strftime pattern
    pub pattern: &'static str,
    /// Example value
    pub example: &'static str,
    /// Position of each component (0, 1, or 2)
    pub position: DateComponentPosition,
    /// Separator character
    pub separator: &'static str,
    /// Number of year digits (2 or 4)
    pub year_digits: u8,
}

/// Position of date components in the format
#[derive(Debug, Clone, Copy)]
pub struct DateComponentPosition {
    /// Position of year component (0, 1, or 2)
    pub year: u8,
    /// Position of month component (0, 1, or 2)
    pub month: u8,
    /// Position of day component (0, 1, or 2)
    pub day: u8,
}

/// A parsed date with its components
#[derive(Debug, Clone)]
pub struct ParsedDate {
    /// Year (full 4-digit year)
    pub year: i32,
    /// Month (1-12)
    pub month: u32,
    /// Day (1-31)
    pub day: u32,
    /// The format that was used to parse
    pub format: String,
}

impl ParsedDate {
    /// Check if this represents a valid date
    pub fn is_valid(&self) -> bool {
        if self.month < 1 || self.month > 12 {
            return false;
        }
        if self.day < 1 || self.day > days_in_month(self.month, self.year) {
            return false;
        }
        true
    }

    /// Convert to NaiveDate
    pub fn to_naive_date(&self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, self.month, self.day)
    }
}

/// Try to parse a date value with a specific format
pub fn try_parse_date(value: &str, format: &DateFormatSpec) -> Option<ParsedDate> {
    let trimmed = value.trim();

    // Try chrono parsing first
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, format.pattern) {
        return Some(ParsedDate {
            year: date.year(),
            month: date.month(),
            day: date.day(),
            format: format.pattern.to_string(),
        });
    }

    // For compact formats, try manual parsing
    if format.separator.is_empty() {
        return try_parse_compact_date(trimmed, format);
    }

    None
}

/// Parse compact date format (no separator)
fn try_parse_compact_date(value: &str, format: &DateFormatSpec) -> Option<ParsedDate> {
    // YYYYMMDD
    if format.year_digits == 4 && value.len() == 8 {
        let year: i32 = value[0..4].parse().ok()?;
        let month: u32 = value[4..6].parse().ok()?;
        let day: u32 = value[6..8].parse().ok()?;
        return Some(ParsedDate {
            year,
            month,
            day,
            format: format.pattern.to_string(),
        });
    }

    // YYMMDD
    if format.year_digits == 2 && value.len() == 6 {
        let year_short: i32 = value[0..2].parse().ok()?;
        let year = if year_short >= 70 {
            1900 + year_short
        } else {
            2000 + year_short
        };
        let month: u32 = value[2..4].parse().ok()?;
        let day: u32 = value[4..6].parse().ok()?;
        return Some(ParsedDate {
            year,
            month,
            day,
            format: format.pattern.to_string(),
        });
    }

    None
}

/// Calculate days in a month (accounting for leap years)
pub fn days_in_month(month: u32, year: i32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0, // Invalid month
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Extract date components from a value given a separator
pub fn extract_components(value: &str, separator: &str) -> Option<(i32, i32, i32)> {
    let parts: Vec<&str> = if separator.is_empty() {
        // Handle compact formats
        return None;
    } else {
        value.split(separator).collect()
    };

    if parts.len() != 3 {
        return None;
    }

    let c1: i32 = parts[0].trim().parse().ok()?;
    let c2: i32 = parts[1].trim().parse().ok()?;
    let c3: i32 = parts[2].trim().parse().ok()?;

    Some((c1, c2, c3))
}

/// Check if a component value can be a month (1-12)
pub fn can_be_month(value: i32) -> bool {
    value >= 1 && value <= 12
}

/// Check if a component value can be a day (1-31)
pub fn can_be_day(value: i32) -> bool {
    value >= 1 && value <= 31
}

/// Check if a component value can be a year
pub fn can_be_year(value: i32, year_digits: u8) -> bool {
    match year_digits {
        2 => value >= 0 && value <= 99,
        4 => value >= 1900 && value <= 2100,
        _ => false,
    }
}

/// Eliminate date formats based on component constraints
///
/// This is the core of constraint-based date format inference.
/// Given a value like "31/05/24", we can eliminate formats where
/// 31 would be in the month position (since 31 > 12).
pub fn eliminate_formats_for_value(
    value: &str,
    possible_formats: &[&DateFormatSpec],
) -> Vec<(&'static DateFormatSpec, &'static str)> {
    let mut eliminated = Vec::new();

    for format in possible_formats {
        if let Some(reason) = check_format_elimination(value, format) {
            // Find the matching static format
            for static_format in DATE_FORMATS {
                if static_format.pattern == format.pattern {
                    eliminated.push((static_format, reason));
                    break;
                }
            }
        }
    }

    eliminated
}

/// Check if a format should be eliminated for a value
fn check_format_elimination(value: &str, format: &DateFormatSpec) -> Option<&'static str> {
    let trimmed = value.trim();

    // Check separator matches
    if !format.separator.is_empty() {
        if !trimmed.contains(format.separator) {
            return Some("separator mismatch");
        }
    }

    // Extract components
    if format.separator.is_empty() {
        // Compact format - check length
        if format.year_digits == 4 && trimmed.len() != 8 {
            return Some("length mismatch for YYYYMMDD");
        }
        if format.year_digits == 2 && trimmed.len() != 6 {
            return Some("length mismatch for YYMMDD");
        }
        // Parse and validate
        if let Some(parsed) = try_parse_compact_date(trimmed, format) {
            if !parsed.is_valid() {
                return Some("invalid date values");
            }
        } else {
            return Some("parse failed");
        }
        return None;
    }

    // Split by separator
    let parts: Vec<&str> = trimmed.split(format.separator).collect();
    if parts.len() != 3 {
        return Some("wrong number of components");
    }

    // Parse each component
    let components: Vec<i32> = parts.iter().filter_map(|p| p.trim().parse().ok()).collect();

    if components.len() != 3 {
        return Some("non-numeric components");
    }

    let (c0, c1, c2) = (components[0], components[1], components[2]);

    // Get the value at each position
    let month_val = match format.position.month {
        0 => c0,
        1 => c1,
        2 => c2,
        _ => return Some("invalid position"),
    };

    let day_val = match format.position.day {
        0 => c0,
        1 => c1,
        2 => c2,
        _ => return Some("invalid position"),
    };

    let year_val = match format.position.year {
        0 => c0,
        1 => c1,
        2 => c2,
        _ => return Some("invalid position"),
    };

    // Check constraints
    if !can_be_month(month_val) {
        return Some("month value out of range");
    }

    if !can_be_day(day_val) {
        return Some("day value out of range");
    }

    if !can_be_year(year_val, format.year_digits) {
        return Some("year value out of range");
    }

    // Check day is valid for the month
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
        return Some("day exceeds month maximum");
    }

    None
}

/// Get all formats with a specific separator
pub fn formats_with_separator(separator: &str) -> Vec<&'static DateFormatSpec> {
    DATE_FORMATS
        .iter()
        .filter(|f| f.separator == separator)
        .collect()
}

/// Get all formats where month is at a specific position
pub fn formats_with_month_position(position: u8) -> Vec<&'static DateFormatSpec> {
    DATE_FORMATS
        .iter()
        .filter(|f| f.position.month == position)
        .collect()
}

use chrono::Datelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_in_month() {
        // Normal months
        assert_eq!(days_in_month(1, 2024), 31);
        assert_eq!(days_in_month(4, 2024), 30);

        // February in leap year
        assert_eq!(days_in_month(2, 2024), 29);
        assert_eq!(days_in_month(2, 2023), 28);

        // Century leap year rules
        assert_eq!(days_in_month(2, 2000), 29); // Divisible by 400
        assert_eq!(days_in_month(2, 1900), 28); // Divisible by 100 but not 400
    }

    #[test]
    fn test_can_be_month() {
        assert!(can_be_month(1));
        assert!(can_be_month(12));
        assert!(!can_be_month(0));
        assert!(!can_be_month(13));
        assert!(!can_be_month(31)); // This is the key constraint!
    }

    #[test]
    fn test_can_be_day() {
        assert!(can_be_day(1));
        assert!(can_be_day(31));
        assert!(!can_be_day(0));
        assert!(!can_be_day(32));
    }

    #[test]
    fn test_parse_date_iso() {
        let format = &DATE_FORMATS[0]; // %Y-%m-%d
        let parsed = try_parse_date("2024-05-31", format).unwrap();
        assert_eq!(parsed.year, 2024);
        assert_eq!(parsed.month, 5);
        assert_eq!(parsed.day, 31);
    }

    #[test]
    fn test_parse_date_european() {
        let format = &DATE_FORMATS[2]; // %d/%m/%Y
        let parsed = try_parse_date("31/05/2024", format).unwrap();
        assert_eq!(parsed.year, 2024);
        assert_eq!(parsed.month, 5);
        assert_eq!(parsed.day, 31);
    }

    #[test]
    fn test_eliminate_mm_dd_when_first_component_exceeds_12() {
        // "31/05/24" cannot be MM/DD/YY because 31 > 12
        let formats: Vec<&DateFormatSpec> = DATE_FORMATS.iter().collect();
        let eliminated = eliminate_formats_for_value("31/05/24", &formats);

        // Should eliminate MM/DD/YY format
        let mm_dd_yy_eliminated = eliminated.iter().any(|(f, _)| f.pattern == "%m/%d/%y");
        assert!(
            mm_dd_yy_eliminated,
            "Should eliminate MM/DD/YY when first component is 31"
        );

        // Should NOT eliminate DD/MM/YY format
        let dd_mm_yy_eliminated = eliminated.iter().any(|(f, _)| f.pattern == "%d/%m/%y");
        assert!(
            !dd_mm_yy_eliminated,
            "Should NOT eliminate DD/MM/YY when first component is 31"
        );
    }

    #[test]
    fn test_ambiguous_date() {
        // "05/06/24" is ambiguous - could be May 6 or June 5
        let formats: Vec<&DateFormatSpec> = DATE_FORMATS.iter().collect();
        let eliminated = eliminate_formats_for_value("05/06/24", &formats);

        // Neither DD/MM/YY nor MM/DD/YY should be eliminated
        let dd_mm_yy_eliminated = eliminated.iter().any(|(f, _)| f.pattern == "%d/%m/%y");
        let mm_dd_yy_eliminated = eliminated.iter().any(|(f, _)| f.pattern == "%m/%d/%y");

        assert!(
            !dd_mm_yy_eliminated && !mm_dd_yy_eliminated,
            "Neither format should be eliminated for ambiguous date"
        );
    }

    #[test]
    fn test_parsed_date_validation() {
        // Valid date
        let valid = ParsedDate {
            year: 2024,
            month: 2,
            day: 29,
            format: "%Y-%m-%d".to_string(),
        };
        assert!(valid.is_valid());

        // Invalid day for month
        let invalid = ParsedDate {
            year: 2023,
            month: 2,
            day: 29, // 2023 is not a leap year
            format: "%Y-%m-%d".to_string(),
        };
        assert!(!invalid.is_valid());

        // Invalid month
        let invalid_month = ParsedDate {
            year: 2024,
            month: 13,
            day: 1,
            format: "%Y-%m-%d".to_string(),
        };
        assert!(!invalid_month.is_valid());
    }

    #[test]
    fn test_compact_date_parsing() {
        let format = DATE_FORMATS.iter().find(|f| f.pattern == "%Y%m%d").unwrap();
        let parsed = try_parse_date("20240531", format).unwrap();
        assert_eq!(parsed.year, 2024);
        assert_eq!(parsed.month, 5);
        assert_eq!(parsed.day, 31);
    }
}

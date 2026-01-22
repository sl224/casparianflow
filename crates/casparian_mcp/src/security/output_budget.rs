//! Output Budget - Response Size Limits
//!
//! Prevents OOM and excessive context usage by limiting response sizes.
//! Large responses are truncated with a clear indicator.
//!
//! # Defaults
//!
//! - Max response size: 1MB
//! - Max rows returned: 10,000

use super::SecurityError;

/// Output budget for limiting response sizes
#[derive(Debug, Clone)]
pub struct OutputBudget {
    /// Maximum response size in bytes
    max_bytes: usize,

    /// Maximum number of rows
    max_rows: usize,
}

impl OutputBudget {
    /// Create a new output budget
    pub fn new(max_bytes: usize, max_rows: usize) -> Self {
        Self { max_bytes, max_rows }
    }

    /// Default budget (1MB, 10K rows)
    pub fn default_budget() -> Self {
        Self {
            max_bytes: 1024 * 1024,
            max_rows: 10_000,
        }
    }

    /// Get max bytes limit
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Get max rows limit
    pub fn max_rows(&self) -> usize {
        self.max_rows
    }

    /// Check if response size is within budget
    pub fn check_size(&self, size: usize) -> Result<(), SecurityError> {
        if size > self.max_bytes {
            Err(SecurityError::OutputTooLarge {
                size,
                max: self.max_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Check if row count is within budget
    pub fn check_rows(&self, count: usize) -> Result<(), SecurityError> {
        if count > self.max_rows {
            Err(SecurityError::TooManyRows {
                count,
                max: self.max_rows,
            })
        } else {
            Ok(())
        }
    }

    /// Enforce size limit, returning truncated content if needed
    ///
    /// Returns (content, was_truncated)
    pub fn enforce_size(&self, content: &str) -> (String, bool) {
        if content.len() <= self.max_bytes {
            (content.to_string(), false)
        } else {
            // Truncate at a reasonable boundary (not mid-UTF8)
            let truncated = content
                .char_indices()
                .take_while(|(i, _)| *i < self.max_bytes - 100) // Leave room for truncation message
                .map(|(_, c)| c)
                .collect::<String>();

            (truncated, true)
        }
    }

    /// Enforce row limit on a vector
    ///
    /// Returns (rows, was_truncated)
    pub fn enforce_rows<T>(&self, rows: Vec<T>) -> (Vec<T>, bool) {
        if rows.len() <= self.max_rows {
            (rows, false)
        } else {
            let truncated: Vec<T> = rows.into_iter().take(self.max_rows).collect();
            (truncated, true)
        }
    }
}

impl Default for OutputBudget {
    fn default() -> Self {
        Self::default_budget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_within_budget() {
        let budget = OutputBudget::new(1000, 100);

        assert!(budget.check_size(500).is_ok());
        assert!(budget.check_rows(50).is_ok());
    }

    #[test]
    fn test_exceeds_size_budget() {
        let budget = OutputBudget::new(1000, 100);

        let result = budget.check_size(2000);
        assert!(matches!(
            result,
            Err(SecurityError::OutputTooLarge { size: 2000, max: 1000 })
        ));
    }

    #[test]
    fn test_exceeds_row_budget() {
        let budget = OutputBudget::new(1000, 100);

        let result = budget.check_rows(200);
        assert!(matches!(
            result,
            Err(SecurityError::TooManyRows { count: 200, max: 100 })
        ));
    }

    #[test]
    fn test_enforce_size() {
        let budget = OutputBudget::new(100, 10);

        // Within budget
        let (content, truncated) = budget.enforce_size("short");
        assert_eq!(content, "short");
        assert!(!truncated);

        // Exceeds budget
        let long = "x".repeat(500);
        let (content, truncated) = budget.enforce_size(&long);
        assert!(content.len() < 100);
        assert!(truncated);
    }

    #[test]
    fn test_enforce_rows() {
        let budget = OutputBudget::new(1000, 5);

        // Within budget
        let (rows, truncated): (Vec<i32>, bool) = budget.enforce_rows(vec![1, 2, 3]);
        assert_eq!(rows, vec![1, 2, 3]);
        assert!(!truncated);

        // Exceeds budget
        let (rows, truncated): (Vec<i32>, bool) = budget.enforce_rows(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(rows.len(), 5);
        assert!(truncated);
    }
}

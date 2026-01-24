//! Canonical metric keys for job receipts and telemetry.
//!
//! Use these constants/helpers everywhere to avoid stringly-typed drift.

/// Total rows processed across outputs.
pub const ROWS: &str = "rows";
/// Total quarantined rows across outputs.
pub const QUARANTINE_ROWS: &str = "quarantine_rows";
/// Total rows with lineage unavailable across outputs.
pub const LINEAGE_UNAVAILABLE_ROWS: &str = "lineage_unavailable_rows";
/// Number of outputs produced by the job.
pub const OUTPUT_COUNT: &str = "output_count";
/// Error classification flag (1 = transient, 0 = permanent).
pub const IS_TRANSIENT: &str = "is_transient";
/// Quarantine policy rejection flag.
pub const QUARANTINE_REJECTED: &str = "quarantine_rejected";

/// Per-output rows prefix.
pub const ROWS_BY_OUTPUT_PREFIX: &str = "rows.";
/// Per-output status prefix.
pub const STATUS_BY_OUTPUT_PREFIX: &str = "status.";
/// Per-output quarantined rows prefix.
pub const QUARANTINE_ROWS_BY_OUTPUT_PREFIX: &str = "quarantine_rows.";
/// Per-output lineage-unavailable rows prefix.
pub const LINEAGE_UNAVAILABLE_ROWS_BY_OUTPUT_PREFIX: &str = "lineage_unavailable_rows.";

/// Build a per-output rows key.
pub fn rows_by_output_key(output: &str) -> String {
    let mut key = String::with_capacity(ROWS_BY_OUTPUT_PREFIX.len() + output.len());
    key.push_str(ROWS_BY_OUTPUT_PREFIX);
    key.push_str(output);
    key
}

/// Build a per-output status key.
pub fn status_by_output_key(output: &str) -> String {
    let mut key = String::with_capacity(STATUS_BY_OUTPUT_PREFIX.len() + output.len());
    key.push_str(STATUS_BY_OUTPUT_PREFIX);
    key.push_str(output);
    key
}

/// Build a per-output quarantined rows key.
pub fn quarantine_rows_by_output_key(output: &str) -> String {
    let mut key = String::with_capacity(QUARANTINE_ROWS_BY_OUTPUT_PREFIX.len() + output.len());
    key.push_str(QUARANTINE_ROWS_BY_OUTPUT_PREFIX);
    key.push_str(output);
    key
}

/// Build a per-output lineage-unavailable rows key.
pub fn lineage_unavailable_rows_by_output_key(output: &str) -> String {
    let mut key =
        String::with_capacity(LINEAGE_UNAVAILABLE_ROWS_BY_OUTPUT_PREFIX.len() + output.len());
    key.push_str(LINEAGE_UNAVAILABLE_ROWS_BY_OUTPUT_PREFIX);
    key.push_str(output);
    key
}

/// Parse a per-output rows key, returning the output name.
pub fn parse_rows_by_output(key: &str) -> Option<&str> {
    key.strip_prefix(ROWS_BY_OUTPUT_PREFIX)
}

/// Parse a per-output status key, returning the output name.
pub fn parse_status_by_output(key: &str) -> Option<&str> {
    key.strip_prefix(STATUS_BY_OUTPUT_PREFIX)
}

/// Parse a per-output quarantined rows key, returning the output name.
pub fn parse_quarantine_rows_by_output(key: &str) -> Option<&str> {
    key.strip_prefix(QUARANTINE_ROWS_BY_OUTPUT_PREFIX)
}

/// Parse a per-output lineage-unavailable rows key, returning the output name.
pub fn parse_lineage_unavailable_rows_by_output(key: &str) -> Option<&str> {
    key.strip_prefix(LINEAGE_UNAVAILABLE_ROWS_BY_OUTPUT_PREFIX)
}

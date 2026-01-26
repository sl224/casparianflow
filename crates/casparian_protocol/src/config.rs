//! System configuration shared across control/data plane.

use std::path::PathBuf;

/// Canonical system configuration used by the launcher and Sentinel.
#[derive(Debug, Clone)]
pub struct SystemConfig {
    /// State store URL (sqlite:/... | postgres://... | sqlserver://...)
    pub state_store_url: String,
    /// Control API bind address
    pub control_addr: String,
    /// Local output root (still used for parquet sinks)
    pub output_root: PathBuf,
    /// DuckDB query catalog path (local SQL over Parquet)
    pub query_catalog_path: PathBuf,
    /// Default sink URI (parquet://... or postgres://...)
    pub default_sink_uri: String,
}

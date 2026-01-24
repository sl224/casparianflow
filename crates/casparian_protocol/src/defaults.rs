//! Canonical default values shared across control/data plane.

pub const DEFAULT_SENTINEL_BIND_ADDR: &str = "tcp://127.0.0.1:5555";
pub const DEFAULT_CONTROL_ADDR: &str = "tcp://127.0.0.1:5556";
pub const DEFAULT_DB_URL: &str = "duckdb:casparian_flow.duckdb";
pub const DEFAULT_SINK_TOPIC: &str = "output";
pub const DEFAULT_SINK_URI: &str = "parquet://./output/";
pub const CANCELLED_BY_USER_MESSAGE: &str = "Cancelled by user";

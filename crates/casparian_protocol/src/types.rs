//! Protocol payload types (Pydantic model equivalents)

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de;
use serde::ser::SerializeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::fmt;
use std::str::FromStr;
use url::form_urlencoded;

// ============================================================================
// Canonical Enums (used across all crates)
// ============================================================================

/// Sink write mode - how to handle existing data.
/// This is the CANONICAL definition - use this everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SinkMode {
    /// Append to existing data (default)
    #[default]
    Append,
    /// Replace/overwrite existing data
    Replace,
    /// Error if data already exists
    Error,
}

impl SinkMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SinkMode::Append => "append",
            SinkMode::Replace => "replace",
            SinkMode::Error => "error",
        }
    }
}

impl fmt::Display for SinkMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for SinkMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "append" => Ok(SinkMode::Append),
            "replace" => Ok(SinkMode::Replace),
            "error" => Ok(SinkMode::Error),
            _ => Err(format!("Invalid sink mode: '{}'. Expected: append, replace, or error", s)),
        }
    }
}


/// Processing job status - lifecycle of a job in the queue.
/// This is the CANONICAL definition - use this everywhere for job queue status.
/// Different from JobStatus (protocol) which is for Worker→Sentinel completion messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessingStatus {
    /// Job created but not yet ready for processing
    #[default]
    Pending,
    /// Job is queued and ready for a worker
    Queued,
    /// Job is currently being processed by a worker
    Running,
    /// Job data written but awaiting finalization (used by `casparian run`)
    Staged,
    /// Job completed successfully
    Completed,
    /// Job failed with an error
    Failed,
    /// Job was skipped (e.g., deduplication)
    Skipped,
}

impl ProcessingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessingStatus::Pending => "PENDING",
            ProcessingStatus::Queued => "QUEUED",
            ProcessingStatus::Running => "RUNNING",
            ProcessingStatus::Staged => "STAGED",
            ProcessingStatus::Completed => "COMPLETED",
            ProcessingStatus::Failed => "FAILED",
            ProcessingStatus::Skipped => "SKIPPED",
        }
    }

    /// For database compatibility - returns lowercase version
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ProcessingStatus::Pending => "pending",
            ProcessingStatus::Queued => "queued",
            ProcessingStatus::Running => "running",
            ProcessingStatus::Staged => "staged",
            ProcessingStatus::Completed => "complete",
            ProcessingStatus::Failed => "failed",
            ProcessingStatus::Skipped => "skipped",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, ProcessingStatus::Completed | ProcessingStatus::Failed | ProcessingStatus::Skipped)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, ProcessingStatus::Running | ProcessingStatus::Staged)
    }
}

impl fmt::Display for ProcessingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ProcessingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "PENDING" => Ok(ProcessingStatus::Pending),
            "QUEUED" => Ok(ProcessingStatus::Queued),
            "RUNNING" => Ok(ProcessingStatus::Running),
            "STAGED" => Ok(ProcessingStatus::Staged),
            "COMPLETED" | "COMPLETE" => Ok(ProcessingStatus::Completed),
            "FAILED" => Ok(ProcessingStatus::Failed),
            "SKIPPED" => Ok(ProcessingStatus::Skipped),
            _ => Err(format!("Invalid processing status: '{}'", s)),
        }
    }
}

/// Worker status for heartbeats and tracking.
/// This is the CANONICAL definition - use this everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerStatus {
    /// Worker is idle, ready for jobs
    #[default]
    Idle,
    /// Worker is busy processing jobs
    Busy,
    /// Worker is alive but status unknown
    Alive,
    /// Worker is draining (finishing current work, not accepting new jobs)
    Draining,
    /// Worker is shutting down
    ShuttingDown,
    /// Worker is offline/dead
    Offline,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Idle => "IDLE",
            WorkerStatus::Busy => "BUSY",
            WorkerStatus::Alive => "ALIVE",
            WorkerStatus::Draining => "DRAINING",
            WorkerStatus::ShuttingDown => "SHUTTING_DOWN",
            WorkerStatus::Offline => "OFFLINE",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, WorkerStatus::Idle)
    }
}

impl fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for WorkerStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "IDLE" => Ok(WorkerStatus::Idle),
            "BUSY" => Ok(WorkerStatus::Busy),
            "ALIVE" => Ok(WorkerStatus::Alive),
            "DRAINING" => Ok(WorkerStatus::Draining),
            "SHUTTING_DOWN" => Ok(WorkerStatus::ShuttingDown),
            "OFFLINE" => Ok(WorkerStatus::Offline),
            _ => Err(format!("Invalid worker status: '{}'", s)),
        }
    }
}

// ============================================================================
// Data Types (Canonical Definition)
// ============================================================================

/// Canonical data type enum - the SINGLE SOURCE OF TRUTH for data types.
///
/// # Layered Design
///
/// This crate defines the canonical data types. Other crates define
/// domain-specific subsets that convert to this canonical type:
///
/// - `casparian_schema::DataType` - User-facing subset for schema approval
///   (excludes Null, Time, Duration - internal/uncommon types)
/// - `casparian_worker::type_inference::DataType` - Inference-friendly names
///   (Integer vs Int64, Float vs Float64, DateTime vs Timestamp)
///
/// All domain-specific types convert to this canonical type via `From` impls.
///
/// # Arrow Mapping
///
/// Each variant maps to an Arrow/Parquet type for output storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum DataType {
    /// Null/empty value (used during type inference)
    Null,

    /// Boolean (true/false, yes/no, 1/0)
    Boolean,

    /// 64-bit signed integer
    Int64,

    /// 64-bit floating point
    Float64,

    /// Date (no time component)
    Date,

    /// Timestamp without timezone (naive)
    Timestamp,

    /// Time only (no date component)
    Time,

    /// Duration/interval
    Duration,

    /// UTF-8 string (default/fallback)
    #[default]
    String,

    /// Binary data (raw bytes)
    Binary,

    /// Decimal128 (precision <= 38)
    Decimal { precision: u8, scale: u8 },

    /// Timestamp with explicit timezone
    TimestampTz { tz: String },

    /// List/array of a single item type
    List { item: Box<DataType> },

    /// Struct with named fields
    Struct { fields: Vec<StructField> },
}

/// A field within a Struct type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
    pub nullable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DataTypeRepr {
    Legacy(String),
    Modern(DataTypeObject),
}

#[derive(Debug, Deserialize)]
struct DataTypeObject {
    pub kind: String,
    #[serde(default)]
    pub precision: Option<u8>,
    #[serde(default)]
    pub scale: Option<u8>,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub item: Option<Box<DataType>>,
    #[serde(default)]
    pub fields: Option<Vec<StructField>>,
}

impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            DataType::Null => serializer.serialize_str("null"),
            DataType::Boolean => serializer.serialize_str("boolean"),
            DataType::Int64 => serializer.serialize_str("int64"),
            DataType::Float64 => serializer.serialize_str("float64"),
            DataType::Date => serializer.serialize_str("date"),
            DataType::Timestamp => serializer.serialize_str("timestamp"),
            DataType::Time => serializer.serialize_str("time"),
            DataType::Duration => serializer.serialize_str("duration"),
            DataType::String => serializer.serialize_str("string"),
            DataType::Binary => serializer.serialize_str("binary"),
            DataType::Decimal { precision, scale } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("kind", "decimal")?;
                map.serialize_entry("precision", precision)?;
                map.serialize_entry("scale", scale)?;
                map.end()
            }
            DataType::TimestampTz { tz } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "timestamp_tz")?;
                map.serialize_entry("tz", tz)?;
                map.end()
            }
            DataType::List { item } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "list")?;
                map.serialize_entry("item", item)?;
                map.end()
            }
            DataType::Struct { fields } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "struct")?;
                map.serialize_entry("fields", fields)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = DataTypeRepr::deserialize(deserializer)?;
        match repr {
            DataTypeRepr::Legacy(raw) => DataType::from_str(&raw).map_err(de::Error::custom),
            DataTypeRepr::Modern(obj) => DataType::from_object(obj).map_err(de::Error::custom),
        }
    }
}

impl DataType {
    /// Return the Arrow type name for this data type.
    pub fn arrow_type_name(&self) -> String {
        match self {
            DataType::Null => "Null".to_string(),
            DataType::Boolean => "Boolean".to_string(),
            DataType::Int64 => "Int64".to_string(),
            DataType::Float64 => "Float64".to_string(),
            DataType::Date => "Date32".to_string(),
            DataType::Timestamp => "Timestamp(Microsecond, None)".to_string(),
            DataType::TimestampTz { tz } => {
                format!("Timestamp(Microsecond, Some(\"{}\"))", tz)
            }
            DataType::Time => "Time64(Microsecond)".to_string(),
            DataType::Duration => "Duration(Microsecond)".to_string(),
            DataType::String => "Utf8".to_string(),
            DataType::Binary => "Binary".to_string(),
            DataType::Decimal { precision, scale } => {
                format!("Decimal128({}, {})", precision, scale)
            }
            DataType::List { .. } => "List".to_string(),
            DataType::Struct { .. } => "Struct".to_string(),
        }
    }

    /// Returns all primitive data types.
    pub fn all() -> Vec<DataType> {
        vec![
            DataType::Null,
            DataType::Boolean,
            DataType::Int64,
            DataType::Float64,
            DataType::Date,
            DataType::Timestamp,
            DataType::Time,
            DataType::Duration,
            DataType::String,
            DataType::Binary,
        ]
    }

    /// Returns primitive numeric types.
    pub fn numeric() -> Vec<DataType> {
        vec![DataType::Int64, DataType::Float64]
    }

    /// Returns primitive temporal types.
    pub fn temporal() -> Vec<DataType> {
        vec![DataType::Date, DataType::Timestamp, DataType::Time, DataType::Duration]
    }

    /// Returns true if this type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Int64 | DataType::Float64 | DataType::Decimal { .. }
        )
    }

    /// Returns true if this type is temporal
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            DataType::Date
                | DataType::Timestamp
                | DataType::TimestampTz { .. }
                | DataType::Time
                | DataType::Duration
        )
    }

    /// Check if a string value can be parsed as this type
    pub fn validate_string(&self, value: &str) -> bool {
        if value.is_empty() {
            return true; // Empty handled by nullable check
        }

        match self {
            DataType::Null => value.is_empty(),
            DataType::Boolean => matches!(
                value.to_lowercase().as_str(),
                "true" | "false" | "1" | "0" | "yes" | "no" | "t" | "f"
            ),
            DataType::Int64 => value.parse::<i64>().is_ok(),
            DataType::Float64 => value.parse::<f64>().is_ok(),
            DataType::Date => {
                // Common date formats
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
                    || chrono::NaiveDate::parse_from_str(value, "%m/%d/%Y").is_ok()
                    || chrono::NaiveDate::parse_from_str(value, "%d/%m/%Y").is_ok()
            }
            DataType::Timestamp => {
                chrono::DateTime::parse_from_rfc3339(value).is_ok()
                    || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok()
            }
            DataType::TimestampTz { .. } => {
                // TimestampTz requires an explicit timezone; RFC3339 enforces this.
                chrono::DateTime::parse_from_rfc3339(value).is_ok()
            }
            DataType::Time => {
                chrono::NaiveTime::parse_from_str(value, "%H:%M:%S").is_ok()
                    || chrono::NaiveTime::parse_from_str(value, "%H:%M").is_ok()
            }
            DataType::Duration => {
                // Simple duration parsing (e.g., "1h30m", "PT1H30M")
                value.starts_with("PT") || value.contains('h') || value.contains('m') || value.contains('s')
            }
            DataType::String => true,
            DataType::Binary => true, // Base64 or hex - assume valid
            DataType::Decimal { precision, scale } => {
                decimal_precision_scale(value)
                    .map(|(digits, value_scale)| digits <= *precision as usize && value_scale <= *scale as usize)
                    .unwrap_or(false)
            }
            DataType::List { .. } | DataType::Struct { .. } => false,
        }
    }

    fn from_object(obj: DataTypeObject) -> Result<Self, String> {
        match obj.kind.to_lowercase().as_str() {
            "null" => Ok(DataType::Null),
            "boolean" => Ok(DataType::Boolean),
            "int64" => Ok(DataType::Int64),
            "float64" => Ok(DataType::Float64),
            "date" => Ok(DataType::Date),
            "timestamp" => Ok(DataType::Timestamp),
            "time" => Ok(DataType::Time),
            "duration" => Ok(DataType::Duration),
            "string" => Ok(DataType::String),
            "binary" => Ok(DataType::Binary),
            "decimal" => {
                let precision = obj
                    .precision
                    .ok_or_else(|| "decimal.precision is required".to_string())?;
                let scale = obj.scale.ok_or_else(|| "decimal.scale is required".to_string())?;
                if precision == 0 || precision > 38 {
                    return Err("decimal.precision must be between 1 and 38".to_string());
                }
                if scale > precision {
                    return Err("decimal.scale must be <= precision".to_string());
                }
                Ok(DataType::Decimal { precision, scale })
            }
            "timestamp_tz" => {
                let tz = obj.tz.ok_or_else(|| "timestamp_tz.tz is required".to_string())?;
                if tz.is_empty() {
                    return Err("timestamp_tz.tz must be non-empty".to_string());
                }
                if !is_valid_timezone(&tz) {
                    return Err(format!(
                        "timestamp_tz.tz '{}' is not a valid IANA timezone",
                        tz
                    ));
                }
                Ok(DataType::TimestampTz { tz })
            }
            "list" => {
                let item = obj.item.ok_or_else(|| "list.item is required".to_string())?;
                Ok(DataType::List { item })
            }
            "struct" => {
                let fields = obj
                    .fields
                    .ok_or_else(|| "struct.fields is required".to_string())?;
                Ok(DataType::Struct { fields })
            }
            other => Err(format!("Invalid data type kind: '{}'", other)),
        }
    }

    /// Alias for Int64 (for backwards compatibility with type inference)
    pub fn integer() -> Self {
        DataType::Int64
    }

    /// Alias for Float64 (for backwards compatibility with type inference)
    pub fn float() -> Self {
        DataType::Float64
    }

    /// Alias for Timestamp (for backwards compatibility)
    pub fn datetime() -> Self {
        DataType::Timestamp
    }
}

fn decimal_precision_scale(value: &str) -> Option<(usize, usize)> {
    let mut total_digits = 0usize;
    let mut scale = 0usize;
    let mut saw_dot = false;
    let mut saw_digit = false;

    for (idx, ch) in value.chars().enumerate() {
        if ch == '+' || ch == '-' {
            if idx != 0 {
                return None;
            }
            continue;
        }
        if ch == '.' {
            if saw_dot {
                return None;
            }
            saw_dot = true;
            continue;
        }
        if ch.is_ascii_digit() {
            saw_digit = true;
            total_digits += 1;
            if saw_dot {
                scale += 1;
            }
            continue;
        }
        return None;
    }

    if !saw_digit {
        None
    } else {
        Some((total_digits, scale))
    }
}

fn is_valid_timezone(tz: &str) -> bool {
    if tz.eq_ignore_ascii_case("utc") {
        return true;
    }
    tz.parse::<chrono_tz::Tz>().is_ok()
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Null => write!(f, "null"),
            DataType::Boolean => write!(f, "boolean"),
            DataType::Int64 => write!(f, "int64"),
            DataType::Float64 => write!(f, "float64"),
            DataType::Date => write!(f, "date"),
            DataType::Timestamp => write!(f, "timestamp"),
            DataType::TimestampTz { tz } => write!(f, "timestamp_tz({})", tz),
            DataType::Time => write!(f, "time"),
            DataType::Duration => write!(f, "duration"),
            DataType::String => write!(f, "string"),
            DataType::Binary => write!(f, "binary"),
            DataType::Decimal { precision, scale } => write!(f, "decimal({},{})", precision, scale),
            DataType::List { item } => write!(f, "list<{}>", item),
            DataType::Struct { fields } => {
                write!(f, "struct{{")?;
                for (idx, field) in fields.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}:{}", field.name, field.data_type)?;
                    if field.nullable {
                        write!(f, "?")?;
                    }
                }
                write!(f, "}}")
            }
        }
    }
}

impl FromStr for DataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "null" => Ok(DataType::Null),
            "boolean" | "bool" => Ok(DataType::Boolean),
            "int64" | "integer" | "int" => Ok(DataType::Int64),
            "float64" | "float" | "double" => Ok(DataType::Float64),
            "date" => Ok(DataType::Date),
            "timestamp" | "datetime" => Ok(DataType::Timestamp),
            "time" => Ok(DataType::Time),
            "duration" | "interval" => Ok(DataType::Duration),
            "string" | "utf8" | "text" => Ok(DataType::String),
            "binary" | "bytes" => Ok(DataType::Binary),
            _ => Err(format!(
                "Invalid data type: '{}'. Expected: null, boolean, int64, float64, date, timestamp, time, duration, string, binary. Use object format for decimal, timestamp_tz, list, struct.",
                s
            )),
        }
    }
}

// ============================================================================
// Sink Configuration
// ============================================================================

/// Configuration for a single data sink.
/// Worker will use this to instantiate the appropriate sink.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SinkConfig {
    pub topic: String,
    pub uri: String,
    #[serde(default)]
    pub mode: SinkMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_def: Option<String>, // Renamed from schema_json to avoid conflicts
}

/// Supported sink URI schemes (job-level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkScheme {
    Parquet,
    Csv,
    Duckdb,
    File,
}

impl SinkScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            SinkScheme::Parquet => "parquet",
            SinkScheme::Csv => "csv",
            SinkScheme::Duckdb => "duckdb",
            SinkScheme::File => "file",
        }
    }
}

impl FromStr for SinkScheme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "parquet" => Ok(SinkScheme::Parquet),
            "csv" => Ok(SinkScheme::Csv),
            "duckdb" => Ok(SinkScheme::Duckdb),
            "file" => Ok(SinkScheme::File),
            other => Err(format!("Unsupported sink scheme: '{}'", other)),
        }
    }
}

/// Parsed sink URI used for local validation and sink creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSinkUri {
    pub scheme: SinkScheme,
    pub path: PathBuf,
    pub query: HashMap<String, String>,
    pub original: String,
}

impl ParsedSinkUri {
    pub fn parse(uri: &str) -> Result<Self, String> {
        let (scheme_str, rest) = uri
            .split_once("://")
            .ok_or_else(|| format!("Sink URI '{}' is missing scheme delimiter", uri))?;
        let scheme = SinkScheme::from_str(scheme_str)?;

        let (path_part, query_part) = rest.split_once('?').unwrap_or((rest, ""));
        if path_part.is_empty() {
            return Err(format!("Sink URI '{}' is missing a path", uri));
        }

        let mut query = HashMap::new();
        if !query_part.is_empty() {
            for (k, v) in form_urlencoded::parse(query_part.as_bytes()) {
                query.insert(k.into_owned(), v.into_owned());
            }
        }

        Ok(Self {
            scheme,
            path: PathBuf::from(path_part),
            query,
            original: uri.to_string(),
        })
    }
}

// ============================================================================
// OpCode.DISPATCH (Sentinel -> Worker)
// ============================================================================

/// Payload for OpCode.DISPATCH.
/// Sentinel -> Worker: "Process this file in isolated venv with Bridge Mode."
///
/// v5.0: Bridge Mode is now mandatory. All execution happens in isolated subprocesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchCommand {
    pub plugin_name: String,
    pub file_path: String,
    pub sinks: Vec<SinkConfig>,
    pub file_id: i64, // Required for lineage restoration

    // Bridge Mode fields (now required)
    pub env_hash: String, // SHA256 of lockfile - links to PluginEnvironment
    pub source_code: String, // Plugin source code for subprocess execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>, // For signature verification (optional for legacy manifests)
}

// ============================================================================
// OpCode.CONCLUDE (Worker -> Sentinel)
// ============================================================================

/// Job completion status - type-safe enum instead of stringly-typed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Success,
    CompletedWithWarnings,
    Failed,
    Rejected,  // Worker at capacity
    Aborted,   // Cancelled by sentinel
}

impl JobStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, JobStatus::Success | JobStatus::CompletedWithWarnings)
    }

    pub fn is_failure(&self) -> bool {
        !self.is_success()
    }
}

/// Payload for OpCode.CONCLUDE.
/// Worker -> Sentinel: "Job finished. Here are the results."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobReceipt {
    pub status: JobStatus,
    pub metrics: HashMap<String, i64>, // e.g., {"rows": 1500, "size_bytes": 42000}
    pub artifacts: Vec<HashMap<String, String>>, // e.g., [{"topic": "output", "uri": "s3://..."}]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>, // Populated if status is failure
}

// ============================================================================
// OpCode.IDENTIFY (Worker -> Sentinel)
// ============================================================================

/// Payload for OpCode.IDENTIFY.
/// Worker -> Sentinel: Handshake with capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyPayload {
    pub capabilities: Vec<String>, // List of plugin names this worker can execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>, // Optional stable worker ID
}

// ============================================================================
// OpCode.HEARTBEAT (Worker -> Sentinel)
// ============================================================================

/// Worker heartbeat status - type-safe enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HeartbeatStatus {
    /// Worker is idle, ready for jobs
    Idle,
    /// Worker is busy processing jobs
    Busy,
    /// Worker is alive (generic keepalive)
    Alive,
}

impl HeartbeatStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, HeartbeatStatus::Idle)
    }
}

/// Payload for OpCode.HEARTBEAT.
/// Worker -> Sentinel: Status update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub status: HeartbeatStatus,
    /// First active job ID (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<i64>,
    /// Number of currently active jobs (0 to MAX_CONCURRENT_JOBS)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub active_job_count: usize,
    /// All active job IDs (for monitoring/debugging)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_job_ids: Vec<i64>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

// ============================================================================
// OpCode.ERR (Bidirectional)
// ============================================================================

/// Payload for OpCode.ERR.
/// Bidirectional: Error notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceback: Option<String>,
}

// ============================================================================
// v5.0 Bridge Mode: Environment Provisioning
// ============================================================================

/// Payload for OpCode.PREPARE_ENV.
/// Sentinel -> Worker: "Provision this environment before execution."
///
/// Enables Eager Provisioning to avoid network blocking during job execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareEnvCommand {
    pub env_hash: String, // SHA256 of lockfile content
    pub lockfile_content: String, // Raw TOML content (uv.lock)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_version: Option<String>, // e.g., "3.11"
}

/// Payload for OpCode.ENV_READY.
/// Worker -> Sentinel: "Environment is ready."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvReadyPayload {
    pub env_hash: String,
    pub interpreter_path: String, // Path to Python interpreter in venv
    #[serde(default)]
    pub cached: bool, // True if environment was already cached
}

// ============================================================================
// v5.0 Bridge Mode: Artifact Deployment
// ============================================================================

/// Payload for OpCode.DEPLOY.
/// CLI -> Sentinel: "Deploy this artifact to the registry."
///
/// Part of the Publisher workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployCommand {
    pub plugin_name: String,
    pub version: String,
    pub source_code: String,
    pub lockfile_content: String, // uv.lock content (empty string for legacy mode)
    pub env_hash: String,         // SHA256(lockfile_content)
    pub artifact_hash: String,    // SHA256(source_code + lockfile_content)
    pub publisher_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azure_oid: Option<String>, // For enterprise mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_requirements: Option<Vec<String>>, // e.g., ["glibc_2.31"]
}

/// Response to a DEPLOY command.
/// Sentinel -> CLI: "Deploy succeeded/failed."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<i64>,
}

// ============================================================================
// Shredder Types (v6.0)
// ============================================================================

/// How to split a multiplexed file into homogeneous shards
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShredStrategy {
    /// Regex with named capture group for shard key
    Regex {
        pattern: String,
        key_group: String, // Named group, e.g., "msg_type"
    },
    /// CSV column value determines shard
    CsvColumn {
        delimiter: u8,
        col_index: usize,
        has_header: bool,
    },
    /// JSON key path (streaming parser)
    JsonKey {
        key_path: String, // e.g., "event.type"
    },
    /// No shredding needed (homogeneous file)
    Passthrough,
}

impl Default for ShredStrategy {
    fn default() -> Self {
        ShredStrategy::Passthrough
    }
}

/// Confidence level of format detection
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DetectionConfidence {
    /// Heuristic is certain (e.g., valid CSV with consistent columns)
    High,
    /// Likely correct but user should verify
    Medium,
    /// Guessing - user MUST review
    Low,
    /// Need LLM assistance or manual specification
    Unknown,
}

impl Default for DetectionConfidence {
    fn default() -> Self {
        DetectionConfidence::Unknown
    }
}

/// Result of analyzing file head
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisResult {
    pub strategy: ShredStrategy,
    pub confidence: DetectionConfidence,
    /// First N unique shard keys found in sample
    pub sample_keys: Vec<String>,
    /// Distinct keys found in sample
    pub estimated_shard_count: usize,
    /// How much of the file we read for analysis
    pub head_bytes: usize,
    /// Human-readable explanation of detection
    pub reasoning: String,
    /// Warning message if high cardinality detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Configuration for shredding operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredConfig {
    pub strategy: ShredStrategy,
    pub output_dir: PathBuf,
    /// Maximum number of open file handles (default: 200)
    #[serde(default = "default_max_handles")]
    pub max_handles: usize,
    /// Number of top keys to get dedicated files; rest go to _MISC (default: 5)
    #[serde(default = "default_top_n_shards")]
    pub top_n_shards: usize,
    /// Buffer size for I/O (default: 64KB)
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Threshold to promote key from _MISC to dedicated file (default: 1000)
    #[serde(default = "default_promotion_threshold")]
    pub promotion_threshold: u64,
}

fn default_max_handles() -> usize {
    200
}
fn default_top_n_shards() -> usize {
    5
}
fn default_buffer_size() -> usize {
    65536 // 64KB
}
fn default_promotion_threshold() -> u64 {
    1000
}

impl Default for ShredConfig {
    fn default() -> Self {
        Self {
            strategy: ShredStrategy::default(),
            output_dir: PathBuf::from("output"),
            max_handles: default_max_handles(),
            top_n_shards: default_top_n_shards(),
            buffer_size: default_buffer_size(),
            promotion_threshold: default_promotion_threshold(),
        }
    }
}

/// Metadata about a generated shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMeta {
    pub path: PathBuf,
    /// The shard key value
    pub key: String,
    pub row_count: u64,
    pub byte_size: u64,
    /// Did we clone header to this shard?
    pub has_header: bool,
    pub first_source_offset: u64,
    pub last_source_offset: u64,
}

/// Result of shredding operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredResult {
    pub shards: Vec<ShardMeta>,
    /// Path to _MISC file for rare types (if created)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freezer_path: Option<PathBuf>,
    /// How many distinct keys ended up in freezer
    pub freezer_key_count: usize,
    pub total_rows: u64,
    pub duration_ms: u64,
    /// Path to sidecar lineage index file
    pub lineage_index_path: PathBuf,
}

/// Block-based lineage record (10KB blocks for efficiency)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageBlock {
    pub block_id: u64,
    pub source_offset_start: u64,
    pub source_offset_end: u64,
    pub shard_key: String,
    pub row_count_in_block: u32,
    pub first_row_number_in_shard: u64,
}

/// Hop in a lineage chain (for multi-hop tracing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageHop {
    pub file_path: PathBuf,
    pub file_type: LineageFileType,
    pub offset: u64,
    pub row_number: u64,
}

/// Type of file in lineage chain
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageFileType {
    Original,
    Shard,
    Freezer,
    ExtractedShard,
}

/// Full lineage chain from output back to source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageChain {
    pub hops: Vec<LineageHop>,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LlmProvider {
    Anthropic,
    OpenAi,
    Ollama { endpoint: String },
    /// Manual parser writing only (no LLM)
    None,
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::None
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    #[serde(flatten)]
    pub provider: LlmProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_max_tokens() -> usize {
    4096
}
fn default_temperature() -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sink_config_serialization() {
        let sink = SinkConfig {
            topic: "output".to_string(),
            uri: "s3://bucket/key".to_string(),
            mode: SinkMode::Append,
            schema_def: None,
        };

        let json = serde_json::to_string(&sink).unwrap();
        assert!(json.contains("\"mode\":\"append\"")); // Serializes to lowercase
        let deserialized: SinkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(sink, deserialized);
    }

    #[test]
    fn test_sink_mode_from_str() {
        assert_eq!("append".parse::<SinkMode>().unwrap(), SinkMode::Append);
        assert_eq!("REPLACE".parse::<SinkMode>().unwrap(), SinkMode::Replace);
        assert_eq!("Error".parse::<SinkMode>().unwrap(), SinkMode::Error);
        assert!("invalid".parse::<SinkMode>().is_err());
    }

    #[test]
    fn test_worker_status_from_str() {
        assert_eq!("IDLE".parse::<WorkerStatus>().unwrap(), WorkerStatus::Idle);
        assert_eq!("busy".parse::<WorkerStatus>().unwrap(), WorkerStatus::Busy);
        assert!(WorkerStatus::Idle.is_available());
        assert!(!WorkerStatus::Busy.is_available());
    }

    #[test]
    fn test_identify_payload_serialization() {
        let payload = IdentifyPayload {
            capabilities: vec!["plugin_a".to_string(), "plugin_b".to_string()],
            worker_id: Some("worker-001".to_string()),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: IdentifyPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.capabilities, deserialized.capabilities);
        assert_eq!(payload.worker_id, deserialized.worker_id);
    }

    #[test]
    fn test_heartbeat_payload_serialization() {
        let payload = HeartbeatPayload {
            status: HeartbeatStatus::Busy,
            current_job_id: Some(12345),
            active_job_count: 3,
            active_job_ids: vec![12345, 12346, 12347],
        };

        let json = serde_json::to_string(&payload).expect("serialize heartbeat");
        let deserialized: HeartbeatPayload = serde_json::from_str(&json).expect("deserialize heartbeat");
        assert_eq!(payload.status, deserialized.status);
        assert_eq!(payload.current_job_id, deserialized.current_job_id);
        assert_eq!(payload.active_job_count, deserialized.active_job_count);
        assert_eq!(payload.active_job_ids, deserialized.active_job_ids);
    }

    #[test]
    fn test_heartbeat_status_serialization() {
        // Test that HeartbeatStatus serializes to SCREAMING_SNAKE_CASE
        assert_eq!(
            serde_json::to_string(&HeartbeatStatus::Idle).unwrap(),
            "\"IDLE\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatStatus::Busy).unwrap(),
            "\"BUSY\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatStatus::Alive).unwrap(),
            "\"ALIVE\""
        );

        // Test deserialization
        assert_eq!(
            serde_json::from_str::<HeartbeatStatus>("\"IDLE\"").unwrap(),
            HeartbeatStatus::Idle
        );
    }

    #[test]
    fn test_heartbeat_payload_backward_compat() {
        // Old payload without new fields should deserialize with defaults
        let old_json = r#"{"status":"ALIVE","current_job_id":123}"#;
        let payload: HeartbeatPayload = serde_json::from_str(old_json).expect("deserialize old heartbeat");
        assert_eq!(payload.status, HeartbeatStatus::Alive);
        assert_eq!(payload.current_job_id, Some(123));
        assert_eq!(payload.active_job_count, 0);
        assert!(payload.active_job_ids.is_empty());
    }

    #[test]
    fn test_job_receipt_serialization() {
        let mut metrics = HashMap::new();
        metrics.insert("rows".to_string(), 1500);
        metrics.insert("size_bytes".to_string(), 42000);

        let receipt = JobReceipt {
            status: JobStatus::Success,
            metrics,
            artifacts: vec![],
            error_message: None,
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: JobReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt.status, deserialized.status);
        assert_eq!(receipt.metrics, deserialized.metrics);
    }

    #[test]
    fn test_job_status_serialization() {
        // Test that JobStatus serializes to SCREAMING_SNAKE_CASE
        assert_eq!(
            serde_json::to_string(&JobStatus::Success).unwrap(),
            "\"SUCCESS\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::CompletedWithWarnings).unwrap(),
            "\"COMPLETED_WITH_WARNINGS\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Failed).unwrap(),
            "\"FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Rejected).unwrap(),
            "\"REJECTED\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Aborted).unwrap(),
            "\"ABORTED\""
        );

        // Test deserialization
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"SUCCESS\"").unwrap(),
            JobStatus::Success
        );
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"COMPLETED_WITH_WARNINGS\"").unwrap(),
            JobStatus::CompletedWithWarnings
        );
        assert_eq!(
            serde_json::from_str::<JobStatus>("\"FAILED\"").unwrap(),
            JobStatus::Failed
        );
    }

    #[test]
    fn test_job_status_methods() {
        assert!(JobStatus::Success.is_success());
        assert!(!JobStatus::Success.is_failure());

        assert!(JobStatus::CompletedWithWarnings.is_success());
        assert!(!JobStatus::CompletedWithWarnings.is_failure());

        assert!(!JobStatus::Failed.is_success());
        assert!(JobStatus::Failed.is_failure());

        assert!(!JobStatus::Rejected.is_success());
        assert!(JobStatus::Rejected.is_failure());

        assert!(!JobStatus::Aborted.is_success());
        assert!(JobStatus::Aborted.is_failure());
    }

    #[test]
    fn test_datatype_from_str() {
        assert_eq!("int64".parse::<DataType>().unwrap(), DataType::Int64);
        assert_eq!("integer".parse::<DataType>().unwrap(), DataType::Int64);
        assert_eq!("INT".parse::<DataType>().unwrap(), DataType::Int64);
        assert_eq!("float64".parse::<DataType>().unwrap(), DataType::Float64);
        assert_eq!("float".parse::<DataType>().unwrap(), DataType::Float64);
        assert_eq!("double".parse::<DataType>().unwrap(), DataType::Float64);
        assert_eq!("string".parse::<DataType>().unwrap(), DataType::String);
        assert_eq!("utf8".parse::<DataType>().unwrap(), DataType::String);
        assert_eq!("date".parse::<DataType>().unwrap(), DataType::Date);
        assert_eq!("timestamp".parse::<DataType>().unwrap(), DataType::Timestamp);
        assert_eq!("datetime".parse::<DataType>().unwrap(), DataType::Timestamp);
        assert_eq!("boolean".parse::<DataType>().unwrap(), DataType::Boolean);
        assert_eq!("bool".parse::<DataType>().unwrap(), DataType::Boolean);
        assert_eq!("binary".parse::<DataType>().unwrap(), DataType::Binary);
        assert!("invalid".parse::<DataType>().is_err());
    }

    #[test]
    fn test_datatype_is_numeric() {
        assert!(DataType::Int64.is_numeric());
        assert!(DataType::Float64.is_numeric());
        assert!(DataType::Decimal { precision: 8, scale: 2 }.is_numeric());
        assert!(!DataType::String.is_numeric());
        assert!(!DataType::Date.is_numeric());
        assert!(!DataType::Boolean.is_numeric());
    }

    #[test]
    fn test_datatype_is_temporal() {
        assert!(DataType::Date.is_temporal());
        assert!(DataType::Timestamp.is_temporal());
        assert!(DataType::TimestampTz { tz: "UTC".to_string() }.is_temporal());
        assert!(DataType::Time.is_temporal());
        assert!(DataType::Duration.is_temporal());
        assert!(!DataType::String.is_temporal());
        assert!(!DataType::Int64.is_temporal());
    }

    #[test]
    fn test_datatype_validate_string() {
        // Int64
        assert!(DataType::Int64.validate_string("123"));
        assert!(DataType::Int64.validate_string("-456"));
        assert!(!DataType::Int64.validate_string("12.34"));
        assert!(!DataType::Int64.validate_string("abc"));

        // Float64
        assert!(DataType::Float64.validate_string("12.34"));
        assert!(DataType::Float64.validate_string("123"));
        assert!(!DataType::Float64.validate_string("abc"));

        // Boolean
        assert!(DataType::Boolean.validate_string("true"));
        assert!(DataType::Boolean.validate_string("false"));
        assert!(DataType::Boolean.validate_string("1"));
        assert!(DataType::Boolean.validate_string("0"));
        assert!(DataType::Boolean.validate_string("yes"));
        assert!(DataType::Boolean.validate_string("no"));
        assert!(!DataType::Boolean.validate_string("maybe"));

        // Date
        assert!(DataType::Date.validate_string("2024-01-15"));
        assert!(DataType::Date.validate_string("01/15/2024"));
        assert!(!DataType::Date.validate_string("not-a-date"));

        // Timestamp
        assert!(DataType::Timestamp.validate_string("2024-01-15T10:30:00Z"));
        assert!(DataType::Timestamp.validate_string("2024-01-15 10:30:00"));

        // TimestampTz (requires explicit timezone)
        assert!(DataType::TimestampTz { tz: "UTC".to_string() }
            .validate_string("2024-01-15T10:30:00Z"));
        assert!(!DataType::TimestampTz { tz: "UTC".to_string() }
            .validate_string("2024-01-15 10:30:00"));

        // Decimal
        assert!(DataType::Decimal { precision: 5, scale: 2 }
            .validate_string("123.45"));
        assert!(!DataType::Decimal { precision: 5, scale: 2 }
            .validate_string("1234.56"));
        assert!(!DataType::Decimal { precision: 5, scale: 2 }
            .validate_string("12.345"));

        // String always valid
        assert!(DataType::String.validate_string("anything"));
        assert!(DataType::String.validate_string(""));

        // Empty is valid for all types (nullability check elsewhere)
        assert!(DataType::Int64.validate_string(""));
    }

    #[test]
    fn test_datatype_arrow_type_name() {
        assert_eq!(DataType::Int64.arrow_type_name(), "Int64");
        assert_eq!(DataType::Float64.arrow_type_name(), "Float64");
        assert_eq!(DataType::String.arrow_type_name(), "Utf8");
        assert_eq!(DataType::Date.arrow_type_name(), "Date32");
        assert_eq!(DataType::Boolean.arrow_type_name(), "Boolean");
        assert_eq!(DataType::Binary.arrow_type_name(), "Binary");
    }

    #[test]
    fn test_datatype_serialization() {
        // Test that primitive DataType serializes to legacy strings
        assert_eq!(
            serde_json::to_string(&DataType::Int64).unwrap(),
            "\"int64\""
        );
        assert_eq!(
            serde_json::to_string(&DataType::Float64).unwrap(),
            "\"float64\""
        );
        assert_eq!(
            serde_json::to_string(&DataType::String).unwrap(),
            "\"string\""
        );

        // Test deserialization
        assert_eq!(
            serde_json::from_str::<DataType>("\"int64\"").unwrap(),
            DataType::Int64
        );
        assert_eq!(
            serde_json::from_str::<DataType>("\"timestamp\"").unwrap(),
            DataType::Timestamp
        );

        // Object format for primitives
        assert_eq!(
            serde_json::from_str::<DataType>("{\"kind\":\"string\"}").unwrap(),
            DataType::String
        );

        // Extended types
        assert_eq!(
            serde_json::to_string(&DataType::Decimal { precision: 18, scale: 8 }).unwrap(),
            "{\"kind\":\"decimal\",\"precision\":18,\"scale\":8}"
        );
        assert_eq!(
            serde_json::to_string(&DataType::TimestampTz { tz: "UTC".to_string() }).unwrap(),
            "{\"kind\":\"timestamp_tz\",\"tz\":\"UTC\"}"
        );
        assert_eq!(
            serde_json::to_string(&DataType::List { item: Box::new(DataType::String) }).unwrap(),
            "{\"kind\":\"list\",\"item\":\"string\"}"
        );
        assert_eq!(
            serde_json::to_string(&DataType::Struct {
                fields: vec![StructField {
                    name: "id".to_string(),
                    data_type: DataType::Int64,
                    nullable: false,
                }],
            }).unwrap(),
            "{\"kind\":\"struct\",\"fields\":[{\"name\":\"id\",\"type\":\"int64\",\"nullable\":false}]}"
        );

        assert_eq!(
            serde_json::from_str::<DataType>("{\"kind\":\"decimal\",\"precision\":18,\"scale\":8}").unwrap(),
            DataType::Decimal { precision: 18, scale: 8 }
        );
        assert_eq!(
            serde_json::from_str::<DataType>("{\"kind\":\"timestamp_tz\",\"tz\":\"UTC\"}").unwrap(),
            DataType::TimestampTz { tz: "UTC".to_string() }
        );

        assert!(serde_json::from_str::<DataType>(
            "{\"kind\":\"timestamp_tz\",\"tz\":\"Not/AZone\"}"
        )
        .is_err());
    }

    #[test]
    fn test_datatype_all() {
        let all = DataType::all();
        assert!(all.contains(&DataType::Int64));
        assert!(all.contains(&DataType::String));
        assert!(all.contains(&DataType::Date));
        assert!(all.contains(&DataType::Binary));
        assert_eq!(all.len(), 10); // All 10 primitive variants
    }

    #[test]
    fn test_datatype_aliases() {
        assert_eq!(DataType::integer(), DataType::Int64);
        assert_eq!(DataType::float(), DataType::Float64);
        assert_eq!(DataType::datetime(), DataType::Timestamp);
    }
}

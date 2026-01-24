//! Stable identifiers and timestamps for schema contracts.

use serde::{Deserialize, Serialize};
use std::fmt;

pub use casparian_ids::{
    AmendmentId, ContractId, DiscoveryId, IdParseError, SchemaId, SchemaVariantId,
};

/// Error returned when parsing timestamps fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaTimestampError {
    message: String,
}

impl SchemaTimestampError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SchemaTimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SchemaTimestampError {}

/// RFC3339 timestamp wrapper for contracts and amendments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaTimestamp(String);

impl SchemaTimestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now().to_rfc3339())
    }

    pub fn parse(value: &str) -> Result<Self, SchemaTimestampError> {
        chrono::DateTime::parse_from_rfc3339(value)
            .map_err(|e| SchemaTimestampError::new(format!("Invalid timestamp: {}", e)))?;
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SchemaTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

//! Shared identifier wrappers for Casparian Flow.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Error returned when parsing a UUID-backed identifier fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdParseError {
    message: String,
}

impl IdParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IdParseError {}

macro_rules! define_uuid_id {
    ($name:ident, $label:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            pub fn parse(value: &str) -> Result<Self, IdParseError> {
                Uuid::parse_str(value)
                    .map_err(|e| IdParseError::new(format!("Invalid {}: {}", $label, e)))?;
                Ok(Self(value.to_string()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }
    };
}

define_uuid_id!(ContractId, "contract ID");
define_uuid_id!(SchemaId, "schema ID");
define_uuid_id!(DiscoveryId, "discovery ID");
define_uuid_id!(SchemaVariantId, "schema variant ID");
define_uuid_id!(AmendmentId, "amendment ID");
define_uuid_id!(ScopeId, "scope ID");
define_uuid_id!(FileId, "file ID");
define_uuid_id!(BacktestId, "backtest ID");

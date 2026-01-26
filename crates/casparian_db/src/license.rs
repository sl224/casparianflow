//! License validation for enterprise database features.
//!
//! Enterprise features (PostgreSQL, MSSQL) require a valid license file.
//! The license is validated at runtime when creating database connections.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

use crate::DatabaseType;

/// License validation errors.
#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("License file not found: {0}")]
    NotFound(String),

    #[error("Invalid license format: {0}")]
    InvalidFormat(String),

    #[error("License expired")]
    Expired,

    #[error("License does not include {0} support")]
    FeatureNotLicensed(String),

    #[error("License signature invalid")]
    InvalidSignature,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// License tier determines which features are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseTier {
    /// Open source tier
    Community,
    /// Professional tier
    Professional,
    /// Enterprise tier
    Enterprise,
}

impl LicenseTier {
    /// Check if this tier allows the given database type.
    pub fn allows(&self, db_type: DatabaseType) -> bool {
        match db_type {
            DatabaseType::DuckDb => true,
            DatabaseType::Sqlite => true,
        }
    }
}

/// Validated license information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// License holder organization
    pub organization: String,
    /// License tier
    pub tier: LicenseTier,
    /// Expiration timestamp (Unix epoch seconds), None = perpetual
    pub expires_at: Option<i64>,
    /// License ID for tracking
    pub license_id: String,
}

impl License {
    /// Load and validate a license from a file path.
    ///
    /// License file format: JSON with signature
    /// ```json
    /// {
    ///   "organization": "Acme Corp",
    ///   "tier": "Professional",
    ///   "expires_at": 1735689600,
    ///   "license_id": "lic_abc123",
    ///   "signature": "base64..."
    /// }
    /// ```
    pub fn load(path: &Path) -> Result<Self, LicenseError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parse license from JSON string.
    pub fn parse(content: &str) -> Result<Self, LicenseError> {
        #[derive(Deserialize)]
        struct LicenseFile {
            organization: String,
            tier: LicenseTier,
            expires_at: Option<i64>,
            license_id: String,
            #[allow(dead_code)]
            signature: String,
        }

        let file: LicenseFile = serde_json::from_str(content)
            .map_err(|e| LicenseError::InvalidFormat(e.to_string()))?;

        // TODO: Verify signature using Ed25519 public key
        // For now, just validate the structure

        let license = License {
            organization: file.organization,
            tier: file.tier,
            expires_at: file.expires_at,
            license_id: file.license_id,
        };

        // Check expiration
        if let Some(expires_at) = license.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            if now > expires_at {
                return Err(LicenseError::Expired);
            }
        }

        Ok(license)
    }

    /// Check if this license allows the given database type.
    pub fn allows(&self, db_type: DatabaseType) -> Result<(), LicenseError> {
        if self.tier.allows(db_type) {
            Ok(())
        } else {
            Err(LicenseError::FeatureNotLicensed(db_type.name().to_string()))
        }
    }

    /// Create a community (open source) license.
    ///
    /// This is the default when no license file is present.
    pub fn community() -> Self {
        Self {
            organization: "Community".to_string(),
            tier: LicenseTier::Community,
            expires_at: None,
            license_id: "community".to_string(),
        }
    }
}

impl Default for License {
    fn default() -> Self {
        Self::community()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_community_license() {
        let license = License::community();
        assert_eq!(license.tier, LicenseTier::Community);

        assert!(license.allows(DatabaseType::DuckDb).is_ok());
    }

    #[test]
    fn test_professional_license() {
        let license = License {
            organization: "Test".to_string(),
            tier: LicenseTier::Professional,
            expires_at: None,
            license_id: "test".to_string(),
        };

        assert!(license.allows(DatabaseType::DuckDb).is_ok());
    }

    #[test]
    fn test_parse_license() {
        let json = r#"{
            "organization": "Acme Corp",
            "tier": "Professional",
            "expires_at": null,
            "license_id": "lic_test",
            "signature": "dummy"
        }"#;

        let license = License::parse(json).unwrap();
        assert_eq!(license.organization, "Acme Corp");
        assert_eq!(license.tier, LicenseTier::Professional);
        assert_eq!(license.license_id, "lic_test");
    }
}

//! Trust configuration parsing
//!
//! Reads trust-related settings from `~/.casparian_flow/config.toml`

use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Error type for config operations
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Config not found at: {0}")]
    NotFound(String),

    #[error("Config validation error: {0}")]
    InvalidConfig(String),
}

/// Result type for config operations
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Trusted signer identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignerId(String);

impl SignerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SignerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for SignerId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(de::Error::custom("signer id cannot be empty"));
        }
        if trimmed != raw {
            return Err(de::Error::custom(
                "signer id cannot contain leading/trailing whitespace",
            ));
        }
        Ok(SignerId(raw))
    }
}

/// Base64-encoded Ed25519 public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyBase64(String);

impl PublicKeyBase64 {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PublicKeyBase64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for PublicKeyBase64 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(de::Error::custom("public key cannot be empty"));
        }
        if trimmed != raw {
            return Err(de::Error::custom(
                "public key cannot contain leading/trailing whitespace",
            ));
        }
        Ok(PublicKeyBase64(raw))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustMode {
    VaultSignedOnly,
}

impl Default for TrustMode {
    fn default() -> Self {
        TrustMode::VaultSignedOnly
    }
}

impl TrustMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustMode::VaultSignedOnly => "vault_signed_only",
        }
    }
}

/// Trust configuration section from config.toml
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustConfigRaw {
    /// Trust mode (default: vault_signed_only)
    #[serde(default)]
    mode: TrustMode,

    /// Allowed signer IDs (must exist in `keys` if provided)
    #[serde(default)]
    allowed_signers: Vec<SignerId>,

    /// Trusted public keys keyed by signer id
    #[serde(default)]
    keys: BTreeMap<SignerId, PublicKeyBase64>,

    /// Dev override: allow unsigned native executables
    #[serde(default)]
    allow_unsigned_native: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustConfig {
    pub mode: TrustMode,
    pub allowed_signers: Vec<SignerId>,
    pub keys: BTreeMap<SignerId, PublicKeyBase64>,
    pub allow_unsigned_native: bool,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            mode: TrustMode::default(),
            allowed_signers: Vec::new(),
            keys: BTreeMap::new(),
            allow_unsigned_native: false,
        }
    }
}

impl TrustConfig {
    fn from_raw(raw: TrustConfigRaw) -> Result<Self> {
        if !raw.allowed_signers.is_empty() {
            for signer in &raw.allowed_signers {
                if !raw.keys.contains_key(signer) {
                    return Err(ConfigError::InvalidConfig(format!(
                        "allowed_signer '{}' missing from trust.keys",
                        signer
                    )));
                }
            }
        }

        Ok(Self {
            mode: raw.mode,
            allowed_signers: raw.allowed_signers,
            keys: raw.keys,
            allow_unsigned_native: raw.allow_unsigned_native,
        })
    }
}

/// Root config structure that may contain a [trust] section
#[derive(Debug, Clone, Deserialize, Default)]
struct RootConfig {
    #[serde(default)]
    trust: Option<TrustConfigRaw>,
}

/// Load trust configuration from a file
pub fn load_trust_config(config_path: &Path) -> Result<TrustConfig> {
    if !config_path.exists() {
        return Ok(TrustConfig::default());
    }

    let content = std::fs::read_to_string(config_path)?;
    let root: RootConfig = toml::from_str(&content)?;

    match root.trust {
        Some(trust) => TrustConfig::from_raw(trust),
        None => Ok(TrustConfig::default()),
    }
}

/// Load trust configuration from the default location
pub fn load_default_trust_config() -> Result<TrustConfig> {
    let config_path = if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        std::path::PathBuf::from(override_path).join("config.toml")
    } else {
        let home = dirs::home_dir()
            .ok_or_else(|| ConfigError::NotFound("Could not find home directory".to_string()))?;
        home.join(".casparian_flow").join("config.toml")
    };
    load_trust_config(&config_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = TrustConfig::default();
        assert_eq!(config.mode, TrustMode::VaultSignedOnly);
        assert!(config.allowed_signers.is_empty());
        assert!(config.keys.is_empty());
        assert!(!config.allow_unsigned_native);
    }

    #[test]
    fn test_load_empty_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let config = load_trust_config(&config_path).unwrap();
        assert_eq!(config.mode, TrustMode::VaultSignedOnly);
        assert!(config.allowed_signers.is_empty());
        assert!(config.keys.is_empty());
        assert!(!config.allow_unsigned_native);
    }

    #[test]
    fn test_load_trust_config_full() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [trust]
            mode = "vault_signed_only"
            allowed_signers = ["casparian_root_2026"]
            allow_unsigned_native = true

            [trust.keys]
            casparian_root_2026 = "BASE64_ED25519_PUB"
            "#,
        )
        .unwrap();

        let config = load_trust_config(&config_path).unwrap();
        assert_eq!(config.mode, TrustMode::VaultSignedOnly);
        assert_eq!(config.allowed_signers.len(), 1);
        assert_eq!(config.allowed_signers[0].as_str(), "casparian_root_2026");
        assert_eq!(config.keys.len(), 1);
        assert_eq!(
            config
                .keys
                .get(&SignerId("casparian_root_2026".to_string()))
                .unwrap()
                .as_str(),
            "BASE64_ED25519_PUB"
        );
        assert!(config.allow_unsigned_native);
    }

    #[test]
    fn test_unknown_field_rejected() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [trust]
            mode = "vault_signed_only"
            unexpected = 1
            "#,
        )
        .unwrap();

        let err = load_trust_config(&config_path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown field"));
    }

    #[test]
    fn test_empty_signer_rejected() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [trust]
            allowed_signers = [""]
            "#,
        )
        .unwrap();

        let err = load_trust_config(&config_path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("signer id cannot be empty"));
    }

    #[test]
    fn test_missing_key_for_allowed_signer() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [trust]
            allowed_signers = ["casparian_root_2026"]
            "#,
        )
        .unwrap();

        let err = load_trust_config(&config_path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing from trust.keys"));
    }

    #[test]
    fn test_empty_key_value_rejected() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [trust]
            allowed_signers = ["casparian_root_2026"]

            [trust.keys]
            casparian_root_2026 = ""
            "#,
        )
        .unwrap();

        let err = load_trust_config(&config_path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("public key cannot be empty"));
    }

    #[test]
    fn test_nonexistent_file_returns_default() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("nonexistent.toml");

        let config = load_trust_config(&config_path).unwrap();
        assert_eq!(config.mode, TrustMode::VaultSignedOnly);
        assert!(config.allowed_signers.is_empty());
        assert!(config.keys.is_empty());
        assert!(!config.allow_unsigned_native);
    }
}

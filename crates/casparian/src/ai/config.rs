//! AI configuration parsing
//!
//! Reads AI-related settings from `~/.casparian_flow/config.toml`

use serde::Deserialize;
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
}

/// Result type for config operations
pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Llamacpp,
    Disabled,
}

impl Default for AiProvider {
    fn default() -> Self {
        AiProvider::Llamacpp
    }
}

impl AiProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiProvider::Llamacpp => "llamacpp",
            AiProvider::Disabled => "disabled",
        }
    }
}

/// AI configuration section from config.toml
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    /// Master switch for all AI features
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// LLM provider: "llamacpp" (default) or "disabled"
    #[serde(default)]
    pub provider: AiProvider,
}

fn default_enabled() -> bool {
    true
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            provider: AiProvider::default(),
        }
    }
}

/// Root config structure that may contain an [ai] section
#[derive(Debug, Clone, Deserialize, Default)]
struct RootConfig {
    #[serde(default)]
    ai: Option<AiConfig>,
}

/// Load AI configuration from a file
pub fn load_ai_config(config_path: &Path) -> Result<AiConfig> {
    if !config_path.exists() {
        return Ok(AiConfig::default());
    }

    let content = std::fs::read_to_string(config_path)?;
    let root: RootConfig = toml::from_str(&content)?;

    Ok(root.ai.unwrap_or_default())
}

/// Load AI configuration from the default location
pub fn load_default_ai_config() -> Result<AiConfig> {
    let home = dirs::home_dir().ok_or_else(|| {
        ConfigError::NotFound("Could not find home directory".to_string())
    })?;
    let config_path = home.join(".casparian_flow").join("config.toml");
    load_ai_config(&config_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert!(config.enabled);
        assert_eq!(config.provider, AiProvider::Llamacpp);
    }

    #[test]
    fn test_load_empty_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let config = load_ai_config(&config_path).unwrap();
        assert!(config.enabled); // Default
    }

    #[test]
    fn test_load_partial_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [ai]
            enabled = false
            provider = "llamacpp"
            "#,
        )
        .unwrap();

        let config = load_ai_config(&config_path).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.provider, AiProvider::Llamacpp);
    }

    #[test]
    fn test_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("nonexistent.toml");

        let config = load_ai_config(&config_path).unwrap();
        // Should return defaults
        assert!(config.enabled);
    }
}

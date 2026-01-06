//! Configuration for the Scout system

use crate::types::{Source, TaggingRule};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration for Scout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutConfig {
    /// Path to the SQLite database
    #[serde(default = "default_database_path")]
    pub database_path: String,

    /// Number of worker threads for parallel processing
    #[serde(default = "default_workers")]
    pub workers: usize,

    /// Default polling interval in seconds
    #[serde(default = "default_poll_interval")]
    pub default_poll_interval_secs: u64,

    /// Maximum concurrent scans
    #[serde(default = "default_max_concurrent_scans")]
    pub max_concurrent_scans: usize,

    /// Sources to watch
    #[serde(default)]
    pub sources: Vec<Source>,

    /// Tagging rules for pattern → tag mapping
    #[serde(default)]
    pub tagging_rules: Vec<TaggingRule>,
}

fn default_database_path() -> String {
    // SINGLE DATABASE: always use absolute path to casparian_flow.sqlite3
    dirs::home_dir()
        .map(|h| h.join(".casparian_flow").join("casparian_flow.sqlite3"))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "casparian_flow.sqlite3".to_string())
}

fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

fn default_poll_interval() -> u64 {
    30
}

fn default_max_concurrent_scans() -> usize {
    4
}

impl Default for ScoutConfig {
    fn default() -> Self {
        Self {
            database_path: default_database_path(),
            workers: default_workers(),
            default_poll_interval_secs: default_poll_interval(),
            max_concurrent_scans: default_max_concurrent_scans(),
            sources: Vec::new(),
            tagging_rules: Vec::new(),
        }
    }
}

impl ScoutConfig {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: ScoutConfig = toml::from_str(&content)
            .map_err(|e| crate::ScoutError::Config(e.to_string()))?;
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::ScoutError::Config(e.to_string()))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SourceType;

    #[test]
    fn test_default_config() {
        let config = ScoutConfig::default();
        // SINGLE DATABASE: must use casparian_flow.sqlite3, not scout.db
        assert!(
            config.database_path.contains("casparian_flow.sqlite3"),
            "Database path should use casparian_flow.sqlite3, got: {}",
            config.database_path
        );
        assert!(config.workers > 0);
        assert_eq!(config.default_poll_interval_secs, 30);
    }

    #[test]
    fn test_config_serialization() {
        let config = ScoutConfig {
            database_path: "test.db".to_string(),
            workers: 4,
            default_poll_interval_secs: 60,
            max_concurrent_scans: 2,
            sources: vec![Source {
                id: "src-1".to_string(),
                name: "Local Data".to_string(),
                source_type: SourceType::Local,
                path: "/data".to_string(),
                poll_interval_secs: 30,
                enabled: true,
            }],
            tagging_rules: vec![TaggingRule {
                id: "rule-1".to_string(),
                name: "CSV Files".to_string(),
                source_id: "src-1".to_string(),
                pattern: "**/*.csv".to_string(),
                tag: "csv_data".to_string(),
                priority: 10,
                enabled: true,
            }],
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: ScoutConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.database_path, config.database_path);
        assert_eq!(parsed.sources.len(), 1);
        assert_eq!(parsed.tagging_rules.len(), 1);
    }
}

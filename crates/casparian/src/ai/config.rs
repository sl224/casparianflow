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

/// AI configuration section from config.toml
#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    /// Master switch for all AI features
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// LLM provider: "llamacpp" (default) or "disabled"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Model configuration
    #[serde(default)]
    pub models: AiModels,

    /// llama.cpp-specific settings
    #[serde(default)]
    pub llamacpp: LlamaCppConfig,

    /// Complexity thresholds for YAML vs Python decision
    #[serde(default)]
    pub complexity: ComplexityConfig,

    /// Redaction settings
    #[serde(default)]
    pub redaction: RedactionConfig,

    /// Audit log settings
    #[serde(default)]
    pub audit: AuditConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            provider: default_provider(),
            models: AiModels::default(),
            llamacpp: LlamaCppConfig::default(),
            complexity: ComplexityConfig::default(),
            redaction: RedactionConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

/// Model configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AiModels {
    /// Model for code generation (Pathfinder, Parser Lab)
    #[serde(default = "default_code_model")]
    pub code_model: String,

    /// Timeout for code generation (seconds)
    #[serde(default = "default_code_timeout")]
    pub code_timeout_seconds: u64,

    /// Model for classification (Labeling)
    #[serde(default = "default_classify_model")]
    pub classify_model: String,

    /// Timeout for classification (seconds)
    #[serde(default = "default_classify_timeout")]
    pub classify_timeout_seconds: u64,

    /// Model for semantic analysis (Semantic Path)
    #[serde(default = "default_semantic_model")]
    pub semantic_model: String,

    /// Timeout for semantic analysis (seconds)
    #[serde(default = "default_semantic_timeout")]
    pub semantic_timeout_seconds: u64,
}

impl Default for AiModels {
    fn default() -> Self {
        Self {
            code_model: default_code_model(),
            code_timeout_seconds: default_code_timeout(),
            classify_model: default_classify_model(),
            classify_timeout_seconds: default_classify_timeout(),
            semantic_model: default_semantic_model(),
            semantic_timeout_seconds: default_semantic_timeout(),
        }
    }
}

/// llama.cpp-specific configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LlamaCppConfig {
    /// HuggingFace repo ID for the model
    #[serde(default = "default_llamacpp_model_repo")]
    pub model_repo: String,

    /// Filename of the GGUF model within the repo
    #[serde(default = "default_llamacpp_model_file")]
    pub model_file: String,

    /// Number of GPU layers to offload (0 = CPU only)
    #[serde(default = "default_llamacpp_n_gpu_layers")]
    pub n_gpu_layers: u32,

    /// Context size (max tokens)
    #[serde(default = "default_llamacpp_n_ctx")]
    pub n_ctx: u32,

    /// Number of threads for CPU inference
    #[serde(default = "default_llamacpp_n_threads")]
    pub n_threads: u32,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_repo: default_llamacpp_model_repo(),
            model_file: default_llamacpp_model_file(),
            n_gpu_layers: default_llamacpp_n_gpu_layers(),
            n_ctx: default_llamacpp_n_ctx(),
            n_threads: default_llamacpp_n_threads(),
        }
    }
}

/// Complexity thresholds for YAML vs Python decision
#[derive(Debug, Clone, Deserialize)]
pub struct ComplexityConfig {
    /// Maximum regex pattern length for YAML
    #[serde(default = "default_yaml_max_regex_chars")]
    pub yaml_max_regex_chars: usize,

    /// Maximum capture groups for YAML
    #[serde(default = "default_yaml_max_capture_groups")]
    pub yaml_max_capture_groups: usize,

    /// Regex length threshold to recommend Python
    #[serde(default = "default_python_recommend_regex_chars")]
    pub python_recommend_regex_chars: usize,

    /// Capture groups threshold to recommend Python
    #[serde(default = "default_python_recommend_capture_groups")]
    pub python_recommend_capture_groups: usize,

    /// Regex length threshold to force Python
    #[serde(default = "default_python_force_regex_chars")]
    pub python_force_regex_chars: usize,

    /// Capture groups threshold to force Python
    #[serde(default = "default_python_force_capture_groups")]
    pub python_force_capture_groups: usize,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            yaml_max_regex_chars: default_yaml_max_regex_chars(),
            yaml_max_capture_groups: default_yaml_max_capture_groups(),
            python_recommend_regex_chars: default_python_recommend_regex_chars(),
            python_recommend_capture_groups: default_python_recommend_capture_groups(),
            python_force_regex_chars: default_python_force_regex_chars(),
            python_force_capture_groups: default_python_force_capture_groups(),
        }
    }
}

/// Redaction settings
#[derive(Debug, Clone, Deserialize)]
pub struct RedactionConfig {
    /// Auto-detect sensitive columns
    #[serde(default = "default_auto_detect")]
    pub auto_detect: bool,

    /// Patterns that trigger redaction warnings
    #[serde(default = "default_sensitive_patterns")]
    pub sensitive_patterns: Vec<String>,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            auto_detect: default_auto_detect(),
            sensitive_patterns: default_sensitive_patterns(),
        }
    }
}

/// Audit log settings
#[derive(Debug, Clone, Deserialize)]
pub struct AuditConfig {
    /// Days to retain success entries
    #[serde(default = "default_success_retention_days")]
    pub success_retention_days: u32,

    /// Days to retain error entries
    #[serde(default = "default_error_retention_days")]
    pub error_retention_days: u32,

    /// Compliance mode: "standard", "compliant", "permissive", "none"
    #[serde(default = "default_compliance_mode")]
    pub compliance_mode: String,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            success_retention_days: default_success_retention_days(),
            error_retention_days: default_error_retention_days(),
            compliance_mode: default_compliance_mode(),
        }
    }
}

// Default value functions
fn default_enabled() -> bool { true }
fn default_provider() -> String { "llamacpp".to_string() }
fn default_code_model() -> String { "qwen2.5-coder:7b".to_string() }
fn default_code_timeout() -> u64 { 60 }
fn default_classify_model() -> String { "phi3.5:3.8b".to_string() }
fn default_classify_timeout() -> u64 { 30 }
fn default_semantic_model() -> String { "phi3.5:3.8b".to_string() }
fn default_semantic_timeout() -> u64 { 30 }

// llama.cpp defaults
fn default_llamacpp_model_repo() -> String { "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF".to_string() }
fn default_llamacpp_model_file() -> String { "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf".to_string() }
fn default_llamacpp_n_gpu_layers() -> u32 { 0 } // CPU by default for compatibility
fn default_llamacpp_n_ctx() -> u32 { 4096 }
fn default_llamacpp_n_threads() -> u32 { 4 }
fn default_yaml_max_regex_chars() -> usize { 100 }
fn default_yaml_max_capture_groups() -> usize { 5 }
fn default_python_recommend_regex_chars() -> usize { 100 }
fn default_python_recommend_capture_groups() -> usize { 5 }
fn default_python_force_regex_chars() -> usize { 200 }
fn default_python_force_capture_groups() -> usize { 10 }
fn default_auto_detect() -> bool { true }
fn default_sensitive_patterns() -> Vec<String> {
    vec![
        "password".to_string(),
        "secret".to_string(),
        "api_key".to_string(),
        "token".to_string(),
        "ssn".to_string(),
        "social_security".to_string(),
        "credit_card".to_string(),
    ]
}
fn default_success_retention_days() -> u32 { 90 }
fn default_error_retention_days() -> u32 { 180 }
fn default_compliance_mode() -> String { "standard".to_string() }

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
        assert_eq!(config.provider, "llamacpp");
        assert_eq!(config.models.code_model, "qwen2.5-coder:7b");
        // llama.cpp defaults
        assert!(config.llamacpp.model_repo.contains("Qwen"));
        assert!(config.llamacpp.model_file.contains(".gguf"));
        assert_eq!(config.llamacpp.n_gpu_layers, 0);
        assert_eq!(config.llamacpp.n_ctx, 4096);
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
        assert_eq!(config.provider, "llamacpp");
        // Defaults should still work
        assert_eq!(config.models.code_model, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_load_full_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [ai]
            enabled = true
            provider = "llamacpp"

            [ai.models]
            code_model = "codellama:13b"
            code_timeout_seconds = 120

            [ai.llamacpp]
            model_repo = "codellama/CodeLlama-7b-Instruct-GGUF"
            model_file = "codellama-7b-instruct.Q4_K_M.gguf"
            n_gpu_layers = 32

            [ai.complexity]
            yaml_max_regex_chars = 150
            python_force_regex_chars = 300

            [ai.audit]
            success_retention_days = 30
            compliance_mode = "compliant"
            "#,
        )
        .unwrap();

        let config = load_ai_config(&config_path).unwrap();
        assert!(config.enabled);
        assert_eq!(config.models.code_model, "codellama:13b");
        assert_eq!(config.models.code_timeout_seconds, 120);
        assert!(config.llamacpp.model_repo.contains("codellama"));
        assert_eq!(config.llamacpp.n_gpu_layers, 32);
        assert_eq!(config.complexity.yaml_max_regex_chars, 150);
        assert_eq!(config.audit.success_retention_days, 30);
        assert_eq!(config.audit.compliance_mode, "compliant");
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

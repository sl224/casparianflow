//! llama.cpp LLM Provider
//!
//! Direct integration with llama.cpp via the llama-cpp-2 Rust bindings.
//! This provides local LLM inference without requiring a separate server.
//!
//! ## Features
//!
//! - Direct llama.cpp integration (no HTTP overhead)
//! - Automatic model download from HuggingFace
//! - GPU acceleration support (CUDA/Metal/Vulkan)
//! - Streaming token generation
//!
//! ## Model Storage
//!
//! Models are cached in `~/.casparian_flow/models/`

#![cfg(feature = "local-llm")]

use async_trait::async_trait;
use futures::stream::BoxStream;
use hf_hub::api::sync::Api;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::{LlmConfig, LlmError, LlmProvider, Message, Role, StreamChunk, ToolDefinition};

// =============================================================================
// Default Models
// =============================================================================

/// Default model for code generation tasks (Qwen2.5-Coder 1.5B)
pub const DEFAULT_CODE_MODEL: &str = "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF";
pub const DEFAULT_CODE_MODEL_FILE: &str = "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf";

/// Default model for classification tasks (smaller, faster)
pub const DEFAULT_CLASSIFY_MODEL: &str = "microsoft/Phi-3-mini-4k-instruct-gguf";
pub const DEFAULT_CLASSIFY_MODEL_FILE: &str = "Phi-3-mini-4k-instruct-q4.gguf";

// =============================================================================
// Provider Configuration
// =============================================================================

/// Configuration for the llama.cpp provider
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Directory to store downloaded models
    pub models_dir: PathBuf,
    /// HuggingFace repo ID for the model
    pub model_repo: String,
    /// Filename of the GGUF model within the repo
    pub model_file: String,
    /// Number of GPU layers to offload (0 = CPU only)
    pub n_gpu_layers: u32,
    /// Context size (max tokens)
    pub n_ctx: u32,
    /// Number of threads for CPU inference
    pub n_threads: u32,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Temperature for sampling
    pub temperature: f32,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        let models_dir = dirs::home_dir()
            .map(|h| h.join(".casparian_flow").join("models"))
            .unwrap_or_else(|| PathBuf::from("./models"));

        Self {
            models_dir,
            model_repo: DEFAULT_CODE_MODEL.to_string(),
            model_file: DEFAULT_CODE_MODEL_FILE.to_string(),
            n_gpu_layers: 0, // CPU by default for compatibility
            n_ctx: 4096,
            n_threads: 4,
            max_tokens: 2048,
            temperature: 0.7,
        }
    }
}

impl LlamaCppConfig {
    /// Get the local path where the model should be stored
    pub fn model_path(&self) -> PathBuf {
        self.models_dir.join(&self.model_file)
    }

    /// Create config for code generation tasks
    pub fn for_code() -> Self {
        Self {
            model_repo: DEFAULT_CODE_MODEL.to_string(),
            model_file: DEFAULT_CODE_MODEL_FILE.to_string(),
            temperature: 0.2, // Lower temp for more deterministic code
            ..Default::default()
        }
    }

    /// Create config for classification tasks
    pub fn for_classify() -> Self {
        Self {
            model_repo: DEFAULT_CLASSIFY_MODEL.to_string(),
            model_file: DEFAULT_CLASSIFY_MODEL_FILE.to_string(),
            max_tokens: 256, // Classification needs fewer tokens
            ..Default::default()
        }
    }
}

// =============================================================================
// Model Manager
// =============================================================================

/// Manages model downloads and caching
pub struct ModelManager {
    config: LlamaCppConfig,
}

impl ModelManager {
    /// Create a new model manager
    pub fn new(config: LlamaCppConfig) -> Self {
        Self { config }
    }

    /// Ensure the model is downloaded and return its path
    pub fn ensure_model(&self) -> Result<PathBuf, LlmError> {
        let model_path = self.config.model_path();

        // Check if already downloaded
        if model_path.exists() {
            return Ok(model_path);
        }

        // Create models directory
        std::fs::create_dir_all(&self.config.models_dir).map_err(|e| {
            LlmError::Internal(format!("Failed to create models directory: {}", e))
        })?;

        // Download from HuggingFace
        tracing::info!(
            "Downloading model {} from {}...",
            self.config.model_file,
            self.config.model_repo
        );

        let api = Api::new().map_err(|e| {
            LlmError::Provider {
                provider: "hf-hub".to_string(),
                message: format!("Failed to initialize HuggingFace API: {}", e),
            }
        })?;

        let repo = api.model(self.config.model_repo.clone());

        let downloaded_path = repo.get(&self.config.model_file).map_err(|e| {
            LlmError::Provider {
                provider: "hf-hub".to_string(),
                message: format!("Failed to download model: {}", e),
            }
        })?;

        // Copy to our models directory (hf-hub caches in its own location)
        std::fs::copy(&downloaded_path, &model_path).map_err(|e| {
            LlmError::Internal(format!("Failed to copy model to cache: {}", e))
        })?;

        tracing::info!("Model downloaded to {:?}", model_path);
        Ok(model_path)
    }
}

// =============================================================================
// llama.cpp Provider
// =============================================================================

/// llama.cpp LLM Provider
///
/// Provides local LLM inference using llama.cpp directly.
pub struct LlamaCppProvider {
    /// Provider configuration
    config: LlamaCppConfig,
    /// Loaded model (lazy-loaded on first use)
    model: Option<Arc<LoadedModel>>,
}

/// A loaded llama.cpp model ready for inference
struct LoadedModel {
    backend: LlamaBackend,
    model: LlamaModel,
    model_path: PathBuf,
}

impl LlamaCppProvider {
    /// Create a new provider with default configuration
    pub fn new() -> Self {
        Self {
            config: LlamaCppConfig::default(),
            model: None,
        }
    }

    /// Create a provider with custom configuration
    pub fn with_config(config: LlamaCppConfig) -> Self {
        Self {
            config,
            model: None,
        }
    }

    /// Create a provider configured for code generation
    pub fn for_code() -> Self {
        Self::with_config(LlamaCppConfig::for_code())
    }

    /// Create a provider configured for classification
    pub fn for_classify() -> Self {
        Self::with_config(LlamaCppConfig::for_classify())
    }

    /// Ensure model is loaded, downloading if necessary
    fn ensure_loaded(&mut self) -> Result<Arc<LoadedModel>, LlmError> {
        if let Some(ref model) = self.model {
            return Ok(Arc::clone(model));
        }

        // Download model if needed
        let manager = ModelManager::new(self.config.clone());
        let model_path = manager.ensure_model()?;

        // Initialize backend
        let backend = LlamaBackend::init().map_err(|e| {
            LlmError::Internal(format!("Failed to initialize llama.cpp backend: {}", e))
        })?;

        // Load model
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(self.config.n_gpu_layers);

        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params).map_err(
            |e| LlmError::Internal(format!("Failed to load model: {}", e)),
        )?;

        let loaded = Arc::new(LoadedModel {
            backend,
            model,
            model_path,
        });

        self.model = Some(Arc::clone(&loaded));
        Ok(loaded)
    }

    /// Build a prompt from messages
    fn build_prompt(&self, messages: &[Message], tools: &[ToolDefinition]) -> String {
        let mut prompt = String::new();

        // System prompt for code tasks
        prompt.push_str("<|im_start|>system\n");
        prompt.push_str("You are a helpful coding assistant for Casparian Flow, a data processing platform. ");
        prompt.push_str("Generate clean, working code with minimal explanation.\n");

        // Add tool descriptions if any
        if !tools.is_empty() {
            prompt.push_str("\nAvailable tools:\n");
            for tool in tools {
                prompt.push_str(&format!("- {}: {}\n", tool.name, tool.description));
            }
        }
        prompt.push_str("<|im_end|>\n");

        // Add conversation history
        for msg in messages {
            match msg.role {
                Role::User => {
                    prompt.push_str("<|im_start|>user\n");
                    prompt.push_str(&msg.text());
                    prompt.push_str("<|im_end|>\n");
                }
                Role::Assistant => {
                    prompt.push_str("<|im_start|>assistant\n");
                    prompt.push_str(&msg.text());
                    prompt.push_str("<|im_end|>\n");
                }
                Role::System => {
                    // Already handled above
                }
            }
        }

        // Start assistant response
        prompt.push_str("<|im_start|>assistant\n");
        prompt
    }

    /// Generate text (blocking, for use in spawn_blocking)
    fn generate_blocking(
        &self,
        loaded: Arc<LoadedModel>,
        prompt: String,
        max_tokens: u32,
        temperature: f32,
        tx: mpsc::Sender<StreamChunk>,
    ) {
        // Create context
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.config.n_ctx))
            .with_n_threads(self.config.n_threads as i32)
            .with_n_threads_batch(self.config.n_threads as i32);

        let mut ctx = match loaded.model.new_context(&loaded.backend, ctx_params) {
            Ok(ctx) => ctx,
            Err(e) => {
                let _ = tx.blocking_send(StreamChunk::Error(format!(
                    "Failed to create context: {}",
                    e
                )));
                return;
            }
        };

        // Tokenize prompt
        let tokens = match loaded
            .model
            .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
        {
            Ok(t) => t,
            Err(e) => {
                let _ = tx.blocking_send(StreamChunk::Error(format!(
                    "Failed to tokenize: {}",
                    e
                )));
                return;
            }
        };

        // Create batch for prompt
        let mut batch = LlamaBatch::new(self.config.n_ctx as usize, 1);

        // Add prompt tokens
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            if let Err(e) = batch.add(*token, i as i32, &[0], is_last) {
                let _ = tx.blocking_send(StreamChunk::Error(format!(
                    "Failed to add token to batch: {}",
                    e
                )));
                return;
            }
        }

        // Decode prompt
        if let Err(e) = ctx.decode(&mut batch) {
            let _ = tx.blocking_send(StreamChunk::Error(format!(
                "Failed to decode prompt: {}",
                e
            )));
            return;
        }

        // Set up sampler
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(42), // Random seed
        ]);

        // Generate tokens
        let mut n_decoded = tokens.len();
        let eos_token = loaded.model.token_eos();

        for _ in 0..max_tokens {
            // Sample next token
            let token = sampler.sample(&ctx, -1);

            // Check for EOS
            if token == eos_token {
                break;
            }

            // Check for end of response marker
            let token_str = loaded
                .model
                .token_to_str(token, llama_cpp_2::model::Special::Tokenize)
                .unwrap_or_default();

            if token_str.contains("<|im_end|>") || token_str.contains("<|endoftext|>") {
                break;
            }

            // Send token
            if !token_str.is_empty() {
                if tx.blocking_send(StreamChunk::Text(token_str)).is_err() {
                    return; // Receiver dropped
                }
            }

            // Prepare next batch
            batch.clear();
            if let Err(e) = batch.add(token, n_decoded as i32, &[0], true) {
                let _ = tx.blocking_send(StreamChunk::Error(format!(
                    "Failed to add token: {}",
                    e
                )));
                return;
            }

            // Decode
            if let Err(e) = ctx.decode(&mut batch) {
                let _ = tx.blocking_send(StreamChunk::Error(format!(
                    "Failed to decode: {}",
                    e
                )));
                return;
            }

            n_decoded += 1;
        }

        // Send done
        let _ = tx.blocking_send(StreamChunk::Done {
            stop_reason: Some("end_turn".to_string()),
        });
    }
}

impl Default for LlamaCppProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    fn model(&self) -> &str {
        &self.config.model_file
    }

    fn is_ready(&self) -> bool {
        // Check if model exists or can be downloaded
        self.config.model_path().exists() || !self.config.model_repo.is_empty()
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        // Clone what we need for the blocking task
        let prompt = self.build_prompt(messages, tools);
        let max_tokens = config.map(|c| c.max_tokens).unwrap_or(self.config.max_tokens);
        let temperature = config
            .and_then(|c| c.temperature)
            .unwrap_or(self.config.temperature);

        // Ensure model is loaded (this is sync, but fast if already loaded)
        let mut provider_clone = Self::with_config(self.config.clone());
        let loaded = provider_clone.ensure_loaded()?;

        let (tx, rx) = mpsc::channel::<StreamChunk>(32);

        // Spawn blocking task for inference
        let config_clone = self.config.clone();
        tokio::task::spawn_blocking(move || {
            let provider = LlamaCppProvider::with_config(config_clone);
            provider.generate_blocking(loaded, prompt, max_tokens, temperature, tx);
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = LlamaCppConfig::default();
        assert!(config.models_dir.to_string_lossy().contains("models"));
        assert_eq!(config.n_gpu_layers, 0);
        assert_eq!(config.n_ctx, 4096);
    }

    #[test]
    fn test_config_for_code() {
        let config = LlamaCppConfig::for_code();
        assert!(config.model_repo.contains("Coder"));
        assert!(config.temperature < 0.5); // Lower temp for code
    }

    #[test]
    fn test_config_for_classify() {
        let config = LlamaCppConfig::for_classify();
        assert!(config.model_repo.contains("Phi"));
        assert_eq!(config.max_tokens, 256); // Fewer tokens needed
    }

    #[test]
    fn test_provider_creation() {
        let provider = LlamaCppProvider::new();
        assert_eq!(provider.name(), "llama.cpp");
    }

    #[test]
    fn test_build_prompt() {
        let provider = LlamaCppProvider::new();

        let messages = vec![
            Message::user("Generate a YAML extraction rule"),
        ];

        let prompt = provider.build_prompt(&messages, &[]);

        assert!(prompt.contains("<|im_start|>system"));
        assert!(prompt.contains("<|im_start|>user"));
        assert!(prompt.contains("Generate a YAML extraction rule"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn test_build_prompt_with_tools() {
        let provider = LlamaCppProvider::new();

        let messages = vec![Message::user("Use the scan tool")];
        let tools = vec![ToolDefinition {
            name: "quick_scan".to_string(),
            description: "Scan a directory".to_string(),
            input_schema: super::super::ToolSchema::default(),
        }];

        let prompt = provider.build_prompt(&messages, &tools);

        assert!(prompt.contains("quick_scan"));
        assert!(prompt.contains("Scan a directory"));
    }

    // Integration test - only runs when model is available
    #[tokio::test]
    #[ignore = "Requires model download (~1GB)"]
    async fn test_inference() {
        use futures::StreamExt;

        let provider = LlamaCppProvider::for_code();

        let messages = vec![Message::user("Write a Python function that adds two numbers")];

        match provider.chat_stream(&messages, &[], None).await {
            Ok(mut stream) => {
                let mut response = String::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        StreamChunk::Text(t) => response.push_str(&t),
                        StreamChunk::Done { .. } => break,
                        StreamChunk::Error(e) => panic!("Error: {}", e),
                        _ => {}
                    }
                }
                println!("Response: {}", response);
                assert!(response.contains("def") || response.contains("function"));
            }
            Err(e) => {
                panic!("Failed to start inference: {}", e);
            }
        }
    }
}

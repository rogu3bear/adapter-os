//! Local inference engine for CLI chat without server
//!
//! Provides direct model inference using MLX FFI backend,
//! bypassing the full Worker infrastructure.

use adapteros_core::{AosError, Result};
use std::path::Path;

#[cfg(feature = "multi-backend")]
use adapteros_core::B3Hash;
#[cfg(feature = "multi-backend")]
use adapteros_lora_mlx_ffi::{GenerationConfig, MLXFFIModel, MLXGenerator, MLXTokenizer};

/// Local inference engine wrapping MLX directly
pub struct LocalInferenceEngine {
    #[cfg(feature = "multi-backend")]
    tokenizer: MLXTokenizer,
    #[cfg(feature = "multi-backend")]
    model: MLXFFIModel,
    #[cfg(feature = "multi-backend")]
    base_seed: B3Hash,
    model_path: std::path::PathBuf,
}

impl LocalInferenceEngine {
    /// Create a new local inference engine
    ///
    /// # Arguments
    /// * `model_path` - Path to the model directory (containing tokenizer.json, config.json, etc.)
    #[allow(unused_variables)]
    pub fn new(model_path: &Path) -> Result<Self> {
        // Validate model path exists
        if !model_path.exists() {
            return Err(AosError::Config(format!(
                "Model path does not exist: {}",
                model_path.display()
            )));
        }

        // Check for tokenizer
        let tokenizer_path = model_path.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(AosError::Config(format!(
                "Tokenizer not found at: {}",
                tokenizer_path.display()
            )));
        }

        #[cfg(not(feature = "multi-backend"))]
        {
            return Err(AosError::Config(
                "Local inference requires 'multi-backend' feature. Rebuild with: cargo build --features multi-backend".to_string()
            ));
        }

        #[cfg(feature = "multi-backend")]
        {
            // Initialize MLX runtime
            if let Err(e) = adapteros_lora_mlx_ffi::mlx_runtime_init() {
                tracing::warn!(
                    "MLX runtime init warning (may already be initialized): {}",
                    e
                );
            }

            // Load tokenizer
            let tokenizer = MLXTokenizer::from_file(&tokenizer_path)
                .map_err(|e| AosError::Config(format!("Failed to load tokenizer: {}", e)))?;

            // Load model
            let model = MLXFFIModel::load(model_path)
                .map_err(|e| AosError::Config(format!("Failed to load model: {}", e)))?;

            // Deterministic seed for all generation
            let base_seed = B3Hash::hash(b"local-inference-cli");

            tracing::info!(
                model_path = %model_path.display(),
                vocab_size = tokenizer.vocab_size(),
                eos_token = tokenizer.eos_token_id(),
                num_layers = model.config.num_hidden_layers,
                "Local inference engine initialized with model"
            );

            Ok(Self {
                tokenizer,
                model,
                base_seed,
                model_path: model_path.to_path_buf(),
            })
        }
    }

    /// Get the model path
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Get vocabulary size (for diagnostics)
    #[cfg(feature = "multi-backend")]
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.vocab_size()
    }

    /// Get vocabulary size (stub when feature disabled)
    #[cfg(not(feature = "multi-backend"))]
    pub fn vocab_size(&self) -> usize {
        0
    }

    /// Generate text (non-streaming)
    #[cfg(feature = "multi-backend")]
    pub fn generate(&self, prompt: &str, max_tokens: usize, temperature: f32) -> Result<String> {
        // Encode prompt tokens
        let prompt_tokens = self
            .tokenizer
            .encode(prompt)
            .map_err(|e| AosError::Internal(format!("Tokenization failed: {}", e)))?;

        tracing::debug!(
            prompt_tokens = prompt_tokens.len(),
            max_tokens = max_tokens,
            temperature = temperature,
            "Starting generation"
        );

        // Create generation config for this request
        let gen_config = GenerationConfig {
            max_tokens,
            temperature,
            top_k: Some(50),
            top_p: Some(0.9),
            repetition_penalty: 1.1,
            eos_token: self.tokenizer.eos_token_id(),
            use_cache: true,
            kv_num_layers: Some(self.model.config.num_hidden_layers),
        };

        // Create generator for this request
        let mut generator = MLXGenerator::new(self.base_seed, gen_config)
            .map_err(|e| AosError::Internal(format!("Failed to create generator: {}", e)))?;

        // Generate tokens
        let generated_tokens = generator
            .generate(&self.model, prompt_tokens)
            .map_err(|e| AosError::Internal(format!("Generation failed: {}", e)))?;

        // Decode to text
        let output = self
            .tokenizer
            .decode(&generated_tokens)
            .map_err(|e| AosError::Internal(format!("Decoding failed: {}", e)))?;

        tracing::debug!(
            output_tokens = generated_tokens.len(),
            output_len = output.len(),
            "Generation complete"
        );

        Ok(output)
    }

    #[cfg(not(feature = "multi-backend"))]
    pub fn generate(&self, _prompt: &str, _max_tokens: usize, _temperature: f32) -> Result<String> {
        Err(AosError::Config(
            "Local inference requires 'multi-backend' feature".to_string(),
        ))
    }

    /// Generate text with streaming output
    #[cfg(feature = "multi-backend")]
    pub fn generate_stream<F>(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        mut callback: F,
    ) -> Result<()>
    where
        F: FnMut(&str) -> bool,
    {
        // Encode prompt tokens
        let prompt_tokens = self
            .tokenizer
            .encode(prompt)
            .map_err(|e| AosError::Internal(format!("Tokenization failed: {}", e)))?;

        let prompt_len = prompt_tokens.len();

        tracing::debug!(
            prompt_tokens = prompt_len,
            max_tokens = max_tokens,
            temperature = temperature,
            "Starting streaming generation"
        );

        // Create generation config for this request
        let gen_config = GenerationConfig {
            max_tokens,
            temperature,
            top_k: Some(50),
            top_p: Some(0.9),
            repetition_penalty: 1.1,
            eos_token: self.tokenizer.eos_token_id(),
            use_cache: true,
            kv_num_layers: Some(self.model.config.num_hidden_layers),
        };

        // Create generator for this request
        let mut generator = MLXGenerator::new(self.base_seed, gen_config)
            .map_err(|e| AosError::Internal(format!("Failed to create generator: {}", e)))?;

        // Generate with streaming callback
        let _tokens = generator
            .generate_streaming(&self.model, prompt_tokens, |token, _position| {
                // Decode single token to text
                let text = self
                    .tokenizer
                    .decode(&[token])
                    .unwrap_or_else(|_| String::new());

                // Call user callback, return whether to continue
                let should_continue = callback(&text);
                Ok(should_continue)
            })
            .map_err(|e| AosError::Internal(format!("Streaming generation failed: {}", e)))?;

        Ok(())
    }

    #[cfg(not(feature = "multi-backend"))]
    pub fn generate_stream<F>(
        &self,
        _prompt: &str,
        _max_tokens: usize,
        _temperature: f32,
        _callback: F,
    ) -> Result<()>
    where
        F: FnMut(&str) -> bool,
    {
        Err(AosError::Config(
            "Local inference requires 'multi-backend' feature".to_string(),
        ))
    }

    /// Apply chat template to a prompt
    #[cfg(feature = "multi-backend")]
    pub fn apply_chat_template(&self, prompt: &str) -> String {
        self.tokenizer.apply_chat_template(prompt)
    }

    #[cfg(not(feature = "multi-backend"))]
    pub fn apply_chat_template(&self, prompt: &str) -> String {
        // Simple fallback template when MLX is not available
        format!(
            "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            prompt
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_model_path() {
        let result = LocalInferenceEngine::new(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("does not exist"),
            "Error should mention path doesn't exist: {}",
            err
        );
    }

    #[test]
    fn test_missing_tokenizer() {
        // Create a temp directory without tokenizer.json
        let temp_dir = std::env::temp_dir().join("aos_test_no_tokenizer");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = LocalInferenceEngine::new(&temp_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Tokenizer not found") || err.contains("multi-backend"),
            "Error should mention missing tokenizer or feature: {}",
            err
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

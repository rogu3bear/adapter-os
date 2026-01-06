//! Local inference engine for CLI chat without server
//!
//! Provides direct model inference using MLX FFI backend,
//! bypassing the full Worker infrastructure.

use adapteros_core::{AosError, Result};
use std::path::Path;

#[cfg(feature = "multi-backend")]
use adapteros_lora_mlx_ffi::MLXTokenizer;

/// Local inference engine wrapping MLX directly
#[derive(Debug)]
pub struct LocalInferenceEngine {
    #[cfg(feature = "multi-backend")]
    tokenizer: MLXTokenizer,
    model_path: std::path::PathBuf,
    #[cfg(feature = "multi-backend")]
    _initialized: bool,
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

            tracing::info!(
                model_path = %model_path.display(),
                vocab_size = tokenizer.vocab_size(),
                "Local inference engine initialized"
            );

            Ok(Self {
                tokenizer,
                model_path: model_path.to_path_buf(),
                _initialized: true,
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
        // For MVP, we use the tokenizer to validate input and return a placeholder
        // Full generation will be implemented when MLX model loading is wired up

        let tokens = self
            .tokenizer
            .encode(prompt)
            .map_err(|e| AosError::Internal(format!("Tokenization failed: {}", e)))?;

        tracing::debug!(
            prompt_tokens = tokens.len(),
            max_tokens = max_tokens,
            temperature = temperature,
            "Generate request (MVP stub)"
        );

        // TODO: Wire up actual MLX model generation
        // For now return a placeholder that confirms the engine is working
        Ok(format!(
            "[Local inference MVP - tokenized {} tokens, would generate up to {}]",
            tokens.len(),
            max_tokens
        ))
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
        // For MVP, tokenize and call callback with placeholder tokens
        let tokens = self
            .tokenizer
            .encode(prompt)
            .map_err(|e| AosError::Internal(format!("Tokenization failed: {}", e)))?;

        tracing::debug!(
            prompt_tokens = tokens.len(),
            max_tokens = max_tokens,
            temperature = temperature,
            "Stream generate request (MVP stub)"
        );

        // TODO: Wire up actual streaming generation
        // For now, simulate streaming with placeholder text
        let words = ["[Local", " inference", " MVP", " -", " streaming", " works!]"];
        for word in words {
            if !callback(word) {
                break;
            }
            // Small delay to simulate streaming
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

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

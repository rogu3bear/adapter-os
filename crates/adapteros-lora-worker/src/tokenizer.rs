//! Tokenizer wrapper for Qwen2.5 model
//!
//! Provides encoding/decoding and chat template formatting for Qwen2.5-Instruct models.

use adapteros_core::{AosError, Result};
use adapteros_secure_fs::{content::validate_tokenizer_config_json, traversal::normalize_path};
use std::path::Path;
use tokenizers::Tokenizer;

/// Tokenizer for Qwen2.5 models
pub struct QwenTokenizer {
    tokenizer: Tokenizer,
    eos_token_id: u32,
    _im_start_id: u32,
    _im_end_id: u32,
}

impl QwenTokenizer {
    /// Load tokenizer from model directory
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Canonicalize path for security validation
        let canonical_path = normalize_path(path.as_ref()).map_err(|e| {
            AosError::Worker(format!(
                "Path security validation failed for tokenizer: {}",
                e
            ))
        })?;

        // Read and validate JSON content before loading tokenizer
        let tokenizer_content = std::fs::read_to_string(&canonical_path)
            .map_err(|e| AosError::Worker(format!("Failed to read tokenizer file: {}", e)))?;

        // Perform semantic validation for tokenizer config
        validate_tokenizer_config_json(&tokenizer_content)
            .map_err(|e| AosError::Worker(format!("Tokenizer config validation failed: {}", e)))?;

        let tokenizer = Tokenizer::from_file(&canonical_path)
            .map_err(|e| AosError::Worker(format!("Failed to load tokenizer: {}", e)))?;

        // Qwen2.5 special tokens
        Ok(Self {
            tokenizer,
            eos_token_id: 151645, // <|im_end|>
            _im_start_id: 151644, // <|im_start|>
            _im_end_id: 151645,   // <|im_end|>
        })
    }

    /// Encode text to token IDs
    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| AosError::Worker(format!("Encoding failed: {}", e)))?;
        Ok(encoding.get_ids().to_vec())
    }

    /// Decode token IDs to text
    pub fn decode(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| AosError::Worker(format!("Decoding failed: {}", e)))
    }

    /// Apply chat template for Qwen2.5-Instruct
    pub fn apply_chat_template(&self, prompt: &str) -> String {
        format!(
            "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            prompt
        )
    }

    /// Get EOS token ID
    pub fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }

    /// Create a tokenizer from an existing tokenizers::Tokenizer instance.
    ///
    /// This is primarily intended for tests where we want to avoid loading
    /// tokenizer JSON files from disk. The token IDs for special tokens
    /// mirror the defaults used in `from_file`.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn from_tokenizer_instance(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            eos_token_id: 151645,
            _im_start_id: 151644,
            _im_end_id: 151645,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires model files
    fn test_tokenizer_round_trip() {
        let tokenizer = QwenTokenizer::from_file("models/qwen2.5-7b-mlx/tokenizer.json")
            .expect("Test tokenizer loading should succeed");
        let text = "Hello, world!";
        let ids = tokenizer
            .encode(text)
            .expect("Test encoding should succeed");
        let decoded = tokenizer
            .decode(&ids)
            .expect("Test decoding should succeed");
        assert!(decoded.contains("Hello"));
    }

    #[test]
    fn test_chat_template() {
        // Create a mock tokenizer for testing since we don't have actual tokenizer files
        // This follows the pattern from the main implementation
        let tokenizer = QwenTokenizer {
            tokenizer: Tokenizer::new(tokenizers::models::bpe::BPE::default()),
            eos_token_id: 151645,
            _im_start_id: 151644,
            _im_end_id: 151645,
        };

        let formatted = tokenizer.apply_chat_template("What is 2+2?");
        assert!(formatted.contains("system"));
        assert!(formatted.contains("user"));
        assert!(formatted.contains("assistant"));
    }
}

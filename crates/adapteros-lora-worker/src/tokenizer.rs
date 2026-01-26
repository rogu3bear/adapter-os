//! Tokenizer wrapper for Qwen2.5 model
//!
//! Provides encoding/decoding and chat template formatting for Qwen2.5-Instruct models.
//!
//! # Special Token Loading (PRD-RECT-003)
//!
//! Special token IDs are loaded dynamically from the model directory using
//! `SpecialTokenMap`. No hardcoded fallback values are used.

use adapteros_core::tokenizer_config::SpecialTokenMap;
use adapteros_core::{AosError, Result};
use adapteros_storage::secure_fs::{content::validate_tokenizer_config_json, traversal::normalize_path};
use std::path::Path;
use tokenizers::Tokenizer;

/// Tokenizer for Qwen2.5 models
pub struct QwenTokenizer {
    tokenizer: Tokenizer,
    /// Special token map loaded from model directory (PRD-RECT-003)
    special_tokens: SpecialTokenMap,
}

impl QwenTokenizer {
    /// Load tokenizer from model directory
    ///
    /// Loads both the tokenizer and special token IDs from the model directory.
    /// Special tokens are resolved via `SpecialTokenMap` without hardcoded fallbacks.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let display_path = path_ref.to_string_lossy();
        if path_ref.to_str().is_none() {
            return Err(AosError::Validation(format!(
                "Tokenizer path is not valid UTF-8: {}",
                display_path
            )));
        }

        // Canonicalize path for security validation
        let canonical_path = normalize_path(path_ref).map_err(|e| {
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

        // Load special tokens from model directory (PRD-RECT-003: no hardcoded fallbacks)
        let model_dir = canonical_path.parent().ok_or_else(|| {
            AosError::Worker("Tokenizer path has no parent directory".to_string())
        })?;
        let special_tokens = SpecialTokenMap::from_model_dir(model_dir)?;

        Ok(Self {
            tokenizer,
            special_tokens,
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
        self.special_tokens.eos_token_id
    }

    /// Get pad token ID (if configured).
    pub fn pad_token_id(&self) -> Option<u32> {
        self.special_tokens.pad_token_id
    }

    /// Get vocab size (optionally including added tokens).
    pub fn vocab_size(&self, include_added_tokens: bool) -> usize {
        self.tokenizer.get_vocab_size(include_added_tokens)
    }

    /// Get the full special token map
    pub fn special_tokens(&self) -> &SpecialTokenMap {
        &self.special_tokens
    }

    /// Create a tokenizer from an existing tokenizers::Tokenizer instance with explicit tokens.
    ///
    /// This is primarily intended for tests where we want to avoid loading
    /// tokenizer JSON files from disk.
    #[cfg(test)]
    pub(crate) fn from_tokenizer_with_tokens(
        tokenizer: Tokenizer,
        special_tokens: SpecialTokenMap,
    ) -> Self {
        Self {
            tokenizer,
            special_tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_config::{DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT};
    use adapteros_core::tokenizer_config::TokenMapSource;

    #[test]
    #[ignore = "Requires tokenizer model files - run with: cargo test --release -- --ignored [tracking: STAB-IGN-0044]"]
    fn test_tokenizer_round_trip() {
        let tokenizer_path = std::env::var("AOS_TOKENIZER_PATH")
            .or_else(|_| std::env::var("AOS_MODEL_PATH").map(|p| format!("{}/tokenizer.json", p)))
            .unwrap_or_else(|_| {
                format!(
                    "{}/{}/tokenizer.json",
                    DEFAULT_MODEL_CACHE_ROOT, DEFAULT_BASE_MODEL_ID
                )
            });
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path)
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
        // Uses explicit SpecialTokenMap instead of hardcoded values
        let special_tokens = SpecialTokenMap {
            eos_token_id: 151645,
            bos_token_id: None,
            pad_token_id: None,
            unk_token_id: None,
            im_start_id: Some(151644),
            im_end_id: Some(151645),
            source: TokenMapSource::Unknown,
        };
        let tokenizer = QwenTokenizer::from_tokenizer_with_tokens(
            Tokenizer::new(tokenizers::models::bpe::BPE::default()),
            special_tokens,
        );

        let formatted = tokenizer.apply_chat_template("What is 2+2?");
        assert!(formatted.contains("system"));
        assert!(formatted.contains("user"));
        assert!(formatted.contains("assistant"));
    }
}

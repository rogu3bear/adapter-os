//! Tokenizer integration for MLX backend
//!
//! Provides tokenization/detokenization support with:
//! - Text encoding to token IDs
//! - Token ID decoding to text
//! - Chat template formatting
//! - BOS/EOS token handling
//! - UTF-8 validation

use adapteros_core::{AosError, Result};
use std::path::Path;
use tokenizers::Tokenizer;

/// Tokenizer wrapper for LLM models
///
/// Supports encoding text to tokens and decoding tokens back to text.
/// Implements proper BOS/EOS token handling and chat template support.
#[derive(Clone)]
pub struct MLXTokenizer {
    /// Underlying tokenizers library instance
    tokenizer: Tokenizer,
    /// Beginning of sequence token ID (usually 0 or <bos>)
    bos_token_id: Option<u32>,
    /// End of sequence token ID (usually 151645 for Qwen or 2 for Llama)
    eos_token_id: u32,
}

impl MLXTokenizer {
    /// Load tokenizer from a tokenizer.json file
    ///
    /// # Arguments
    /// * `path` - Path to tokenizer.json file
    ///
    /// # Returns
    /// Loaded tokenizer ready for encoding/decoding
    ///
    /// # Notes
    /// Attempts to auto-detect BOS/EOS tokens from the tokenizer configuration.
    /// Falls back to common defaults if not found.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let tokenizer = Tokenizer::from_file(path).map_err(|e| {
            AosError::Worker(format!(
                "Failed to load tokenizer from {}: {}",
                path.display(),
                e
            ))
        })?;

        // Try to extract special token IDs from the tokenizer
        let mut bos_token_id = None;
        let mut eos_token_id = 151645; // Default Qwen2.5 EOS

        // Check for common EOS tokens
        let eos_candidates = [
            "<|endoftext|>",
            "<|im_end|>",
            "</s>",
            "<eos>",
            "[EOS]",
            "<|end|>",
        ];
        for token in &eos_candidates {
            if let Some(id) = tokenizer.token_to_id(token) {
                eos_token_id = id;
                tracing::debug!(token = token, id = id, "Found EOS token");
                break;
            }
        }

        // Check for common BOS tokens
        let bos_candidates = ["<|startoftext|>", "<|im_start|>", "<s>", "<bos>", "[BOS]"];
        for token in &bos_candidates {
            if let Some(id) = tokenizer.token_to_id(token) {
                bos_token_id = Some(id);
                tracing::debug!(token = token, id = id, "Found BOS token");
                break;
            }
        }

        tracing::info!(
            vocab_size = tokenizer.get_vocab_size(false),
            eos_token_id = eos_token_id,
            bos_token_id = ?bos_token_id,
            path = %path.display(),
            "Tokenizer loaded successfully"
        );

        Ok(Self {
            tokenizer,
            bos_token_id,
            eos_token_id,
        })
    }

    /// Load tokenizer from a model directory
    ///
    /// Looks for tokenizer.json in the given directory.
    ///
    /// # Arguments
    /// * `model_dir` - Path to model directory containing tokenizer.json
    ///
    /// # Returns
    /// Loaded tokenizer ready for encoding/decoding
    pub fn from_model_dir<P: AsRef<Path>>(model_dir: P) -> Result<Self> {
        let model_dir = model_dir.as_ref();
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !tokenizer_path.exists() {
            return Err(AosError::NotFound(format!(
                "tokenizer.json not found in model directory: {}",
                model_dir.display()
            )));
        }

        Self::from_file(&tokenizer_path)
    }

    /// Create tokenizer with custom EOS token ID
    ///
    /// # Arguments
    /// * `tokenizer` - Loaded tokenizers::Tokenizer instance
    /// * `eos_token_id` - End of sequence token ID
    ///
    /// # Returns
    /// Configured tokenizer
    pub fn new(tokenizer: Tokenizer, eos_token_id: u32) -> Self {
        Self {
            tokenizer,
            bos_token_id: None,
            eos_token_id,
        }
    }

    /// Set BOS token ID
    pub fn with_bos_token(mut self, bos_token_id: u32) -> Self {
        self.bos_token_id = Some(bos_token_id);
        self
    }

    /// Encode text to token IDs
    ///
    /// # Arguments
    /// * `text` - Text to encode
    ///
    /// # Returns
    /// Vector of token IDs
    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| AosError::Worker(format!("Encoding failed: {}", e)))?;

        Ok(encoding.get_ids().to_vec())
    }

    /// Encode text with BOS token prepended
    ///
    /// # Arguments
    /// * `text` - Text to encode
    ///
    /// # Returns
    /// Vector of token IDs starting with BOS token (if configured)
    pub fn encode_with_bos(&self, text: &str) -> Result<Vec<u32>> {
        let mut tokens = self.encode(text)?;

        // Prepend BOS token if configured
        if let Some(bos_id) = self.bos_token_id {
            tokens.insert(0, bos_id);
        }

        Ok(tokens)
    }

    /// Decode token IDs to text
    ///
    /// # Arguments
    /// * `ids` - Token IDs to decode
    ///
    /// # Returns
    /// Decoded text
    pub fn decode(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| AosError::Worker(format!("Decoding failed: {}", e)))
    }

    /// Decode token IDs without skipping special tokens
    ///
    /// Useful for debugging or when special tokens should be preserved in output.
    pub fn decode_no_skip(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, false)
            .map_err(|e| AosError::Worker(format!("Decoding failed: {}", e)))
    }

    /// Get EOS token ID
    pub fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }

    /// Get BOS token ID if configured
    pub fn bos_token_id(&self) -> Option<u32> {
        self.bos_token_id
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.get_vocab_size(false)
    }

    /// Apply chat template for instruction-following models
    ///
    /// Default implementation for Qwen2.5-Instruct format.
    /// Can be overridden for other model families.
    ///
    /// # Arguments
    /// * `prompt` - User prompt text
    ///
    /// # Returns
    /// Formatted prompt ready for encoding
    pub fn apply_chat_template(&self, prompt: &str) -> String {
        format!(
            "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            prompt
        )
    }

    /// Apply chat template for models using chat_template in tokenizer.json
    ///
    /// This attempts to use the model's official chat template if available.
    /// Falls back to manual template if not available.
    ///
    /// # Arguments
    /// * `prompt` - User prompt text
    ///
    /// # Returns
    /// Formatted prompt ready for encoding
    pub fn apply_chat_template_auto(&self, prompt: &str) -> String {
        // Try to apply template through tokenizers library
        // For now, fall back to Qwen format
        self.apply_chat_template(prompt)
    }

    /// Get underlying tokenizers library instance
    ///
    /// Useful for creating specialized decoders or advanced tokenization operations.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }
}

/// Simple token decoder for streaming
///
/// Decodes individual tokens or small sequences.
/// Useful for streaming applications where tokens arrive incrementally.
pub struct StreamingTokenDecoder {
    tokenizer: Tokenizer,
    /// Cached state for proper decoding of partial tokens
    cached_ids: Vec<u32>,
}

impl StreamingTokenDecoder {
    /// Create new streaming decoder
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            cached_ids: Vec::new(),
        }
    }

    /// Add a token ID and decode as much as possible
    ///
    /// Returns decoded text (may be empty if token represents partial character)
    pub fn push_token(&mut self, token_id: u32) -> Result<String> {
        self.cached_ids.push(token_id);

        // Try to decode from cached IDs
        match self
            .tokenizer
            .decode(&self.cached_ids, true)
            .map_err(|e| AosError::Worker(format!("Decoding failed: {}", e)))
        {
            Ok(decoded) => {
                // Clear cache and return decoded text
                self.cached_ids.clear();
                Ok(decoded)
            }
            Err(_) => {
                // Keep buffering - probably incomplete character
                Ok(String::new())
            }
        }
    }

    /// Flush remaining cached tokens
    ///
    /// Call at end of generation to decode any remaining tokens.
    pub fn flush(&mut self) -> Result<String> {
        if self.cached_ids.is_empty() {
            return Ok(String::new());
        }

        let decoded = self
            .tokenizer
            .decode(&self.cached_ids, true)
            .map_err(|e| AosError::Worker(format!("Decoding failed: {}", e)))?;

        self.cached_ids.clear();
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_creation() {
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let mlx_tokenizer = MLXTokenizer::new(tokenizer, 151645);

        assert_eq!(mlx_tokenizer.eos_token_id(), 151645);
        assert_eq!(mlx_tokenizer.bos_token_id(), None);
    }

    #[test]
    fn test_tokenizer_with_bos() {
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let mlx_tokenizer = MLXTokenizer::new(tokenizer, 151645).with_bos_token(1);

        assert_eq!(mlx_tokenizer.bos_token_id(), Some(1));
    }

    #[test]
    fn test_chat_template_formatting() {
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let mlx_tokenizer = MLXTokenizer::new(tokenizer, 151645);

        let formatted = mlx_tokenizer.apply_chat_template("What is 2+2?");
        assert!(formatted.contains("<|im_start|>system"));
        assert!(formatted.contains("user"));
        assert!(formatted.contains("What is 2+2?"));
        assert!(formatted.contains("assistant"));
    }

    #[test]
    fn test_streaming_decoder_creation() {
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let _decoder = StreamingTokenDecoder::new(tokenizer);
        // Decoder should initialize successfully
    }
}

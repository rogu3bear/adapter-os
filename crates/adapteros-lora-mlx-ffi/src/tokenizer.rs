//! Tokenizer integration for MLX backend
//!
//! Provides tokenization/detokenization support with:
//! - Text encoding to token IDs
//! - Token ID decoding to text
//! - Chat template formatting
//! - BOS/EOS token handling (via SpecialTokenMap)
//! - UTF-8 validation
//!
//! # Token Loading
//!
//! Special token IDs are loaded from the model directory using `SpecialTokenMap`,
//! which reads from `tokenizer_config.json` or `tokenizer.json`. NO hardcoded
//! fallback values are used - loading fails if EOS token cannot be resolved.

use adapteros_core::{AosError, Result, SpecialTokenMap};
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
    /// Special token map loaded from model directory
    special_tokens: SpecialTokenMap,
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
    /// # Errors
    /// Returns error if:
    /// - Tokenizer file cannot be read or parsed
    /// - EOS token cannot be resolved from tokenizer config
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let tokenizer = Tokenizer::from_file(path).map_err(|e| {
            AosError::Worker(format!(
                "Failed to load tokenizer from {}: {}",
                path.display(),
                e
            ))
        })?;

        // Load special tokens from model directory (parent of tokenizer.json)
        // NO hardcoded fallbacks - fail if EOS cannot be resolved
        let model_dir = path.parent().unwrap_or(Path::new("."));
        let special_tokens = SpecialTokenMap::from_model_dir(model_dir)?;

        tracing::info!(
            vocab_size = tokenizer.get_vocab_size(false),
            eos_token_id = special_tokens.eos_token_id,
            bos_token_id = ?special_tokens.bos_token_id,
            im_start_id = ?special_tokens.im_start_id,
            im_end_id = ?special_tokens.im_end_id,
            path = %path.display(),
            "Tokenizer loaded successfully with SpecialTokenMap"
        );

        Ok(Self {
            tokenizer,
            special_tokens,
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

    /// Create tokenizer with custom special tokens
    ///
    /// # Arguments
    /// * `tokenizer` - Loaded tokenizers::Tokenizer instance
    /// * `special_tokens` - Special token configuration
    ///
    /// # Returns
    /// Configured tokenizer
    pub fn new(tokenizer: Tokenizer, special_tokens: SpecialTokenMap) -> Self {
        Self {
            tokenizer,
            special_tokens,
        }
    }

    /// Create tokenizer with just EOS token (for testing)
    #[cfg(test)]
    pub(crate) fn new_with_eos(tokenizer: Tokenizer, eos_token_id: u32) -> Self {
        use adapteros_core::tokenizer_config::TokenMapSource;
        Self {
            tokenizer,
            special_tokens: SpecialTokenMap {
                eos_token_id,
                bos_token_id: None,
                pad_token_id: None,
                unk_token_id: None,
                im_start_id: None,
                im_end_id: Some(eos_token_id),
                source: TokenMapSource::Unknown,
            },
        }
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
        if let Some(bos_id) = self.special_tokens.bos_token_id {
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
        self.special_tokens.eos_token_id
    }

    /// Get BOS token ID if configured
    pub fn bos_token_id(&self) -> Option<u32> {
        self.special_tokens.bos_token_id
    }

    /// Get the special token map
    pub fn special_tokens(&self) -> &SpecialTokenMap {
        &self.special_tokens
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.get_vocab_size(false)
    }

    /// Apply chat template for instruction-following models
    ///
    /// Selects template based on available special tokens:
    /// - ChatML (`<|im_start|>` / `<|im_end|>`) for Qwen, Yi, etc.
    /// - Mistral (`[INST]` / `[/INST]`) when BOS is available but no im_start
    ///
    /// # Arguments
    /// * `prompt` - User prompt text
    ///
    /// # Returns
    /// Formatted prompt ready for encoding
    pub fn apply_chat_template(&self, prompt: &str) -> String {
        if self.special_tokens.im_start_id.is_some() {
            // ChatML format (Qwen, Yi, etc.)
            format!(
                "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                prompt
            )
        } else if self.special_tokens.bos_token_id.is_some() {
            // Mistral/Llama instruct format
            format!("<s> [INST] {} [/INST]", prompt)
        } else {
            // Fallback: raw prompt
            prompt.to_string()
        }
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
        let mlx_tokenizer = MLXTokenizer::new_with_eos(tokenizer, 151645);

        assert_eq!(mlx_tokenizer.eos_token_id(), 151645);
        assert_eq!(mlx_tokenizer.bos_token_id(), None);
    }

    #[test]
    fn test_tokenizer_with_special_tokens() {
        use adapteros_core::tokenizer_config::TokenMapSource;
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let special_tokens = SpecialTokenMap {
            eos_token_id: 151645,
            bos_token_id: Some(1),
            pad_token_id: None,
            unk_token_id: None,
            im_start_id: Some(151644),
            im_end_id: Some(151645),
            source: TokenMapSource::Unknown,
        };
        let mlx_tokenizer = MLXTokenizer::new(tokenizer, special_tokens);

        assert_eq!(mlx_tokenizer.eos_token_id(), 151645);
        assert_eq!(mlx_tokenizer.bos_token_id(), Some(1));
    }

    #[test]
    fn test_chat_template_formatting() {
        use adapteros_core::tokenizer_config::TokenMapSource;

        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        // Provide ChatML markers so apply_chat_template() selects the ChatML format.
        // new_with_eos() intentionally leaves these unset and would fall back to the raw prompt.
        let mlx_tokenizer = MLXTokenizer::new(
            tokenizer,
            SpecialTokenMap {
                eos_token_id: 151645,
                bos_token_id: None,
                pad_token_id: None,
                unk_token_id: None,
                im_start_id: Some(151644),
                im_end_id: Some(151645),
                source: TokenMapSource::Unknown,
            },
        );

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

//! Tokenizer Configuration (PRD-RECT-003)
//!
//! Provides a single source of truth for special token IDs.
//! Eliminates hardcoded EOS tokens across backends.
//!
//! # Loading Priority
//!
//! 1. `tokenizer_config.json` - explicit token IDs if present
//! 2. `tokenizer.json` - token-to-id lookup for special tokens
//! 3. Error if EOS token cannot be resolved (NO hardcoded fallback)
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_core::tokenizer_config::SpecialTokenMap;
//!
//! let tokens = SpecialTokenMap::from_model_dir("/path/to/model")?;
//! println!("EOS token ID: {}", tokens.eos_token_id);
//! ```

use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Well-known EOS token strings to search for in tokenizer vocabulary.
const EOS_TOKEN_CANDIDATES: &[&str] = &[
    "<|endoftext|>",
    "<|im_end|>",
    "</s>",
    "<eos>",
    "[EOS]",
    "<|end|>",
    "<|eot_id|>",
];

/// Well-known BOS token strings to search for in tokenizer vocabulary.
const BOS_TOKEN_CANDIDATES: &[&str] = &[
    "<|startoftext|>",
    "<|im_start|>",
    "<s>",
    "<bos>",
    "[BOS]",
    "<|begin_of_text|>",
];

/// FIM (Fill-in-the-Middle) token strings for code completion models.
///
/// Qwen2.5 and other code models use these tokens natively for infill generation.
const FIM_PREFIX_TOKEN: &str = "<|fim_prefix|>";
const FIM_SUFFIX_TOKEN: &str = "<|fim_suffix|>";
const FIM_MIDDLE_TOKEN: &str = "<|fim_middle|>";

/// Special token ID map loaded from tokenizer configuration.
///
/// This struct provides the canonical source of truth for special token IDs
/// used during inference. It is loaded once per model and shared across
/// all inference requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialTokenMap {
    /// End-of-sequence token ID (required)
    pub eos_token_id: u32,
    /// Beginning-of-sequence token ID (optional)
    pub bos_token_id: Option<u32>,
    /// Padding token ID (optional)
    pub pad_token_id: Option<u32>,
    /// Unknown token ID (optional)
    pub unk_token_id: Option<u32>,
    /// Instruction start marker (Qwen/ChatML style)
    pub im_start_id: Option<u32>,
    /// Instruction end marker (Qwen/ChatML style)
    pub im_end_id: Option<u32>,
    /// Fill-in-the-Middle prefix token (`<|fim_prefix|>`)
    pub fim_prefix_id: Option<u32>,
    /// Fill-in-the-Middle suffix token (`<|fim_suffix|>`)
    pub fim_suffix_id: Option<u32>,
    /// Fill-in-the-Middle middle/infill token (`<|fim_middle|>`)
    pub fim_middle_id: Option<u32>,
    /// Source of the token IDs for debugging
    #[serde(skip)]
    pub source: TokenMapSource,
}

/// Minimal metadata returned by tokenizer validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerMetadata {
    pub hash: B3Hash,
    pub vocab_size: usize,
    pub added_tokens: usize,
    /// Normalizer type if present (useful for UI/debugging)
    pub normalizer: Option<String>,
}

/// Source of token ID resolution.
#[derive(Debug, Clone, Default)]
pub enum TokenMapSource {
    /// Loaded from tokenizer_config.json explicit fields
    TokenizerConfig,
    /// Looked up from tokenizer.json vocabulary
    VocabLookup,
    /// Default/fallback (should not be used in production)
    #[default]
    Unknown,
}

/// Resolved FIM (Fill-in-the-Middle) token IDs for code completion.
///
/// All three tokens must be present for FIM to work. Use
/// [`SpecialTokenMap::fim_tokens()`] to extract this from a loaded token map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FIMTokens {
    /// `<|fim_prefix|>` token ID
    pub prefix_id: u32,
    /// `<|fim_suffix|>` token ID
    pub suffix_id: u32,
    /// `<|fim_middle|>` token ID
    pub middle_id: u32,
}

/// Raw tokenizer_config.json structure for deserialization.
#[derive(Debug, Deserialize)]
struct TokenizerConfigJson {
    eos_token: Option<TokenOrId>,
    bos_token: Option<TokenOrId>,
    pad_token: Option<TokenOrId>,
    unk_token: Option<TokenOrId>,
    eos_token_id: Option<u32>,
    bos_token_id: Option<u32>,
    pad_token_id: Option<u32>,
    unk_token_id: Option<u32>,
}

/// Token can be either a string or a numeric ID.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenOrId {
    String(String),
    Id(u32),
    Object { content: String },
}

impl SpecialTokenMap {
    /// Load special tokens from a model directory.
    ///
    /// Priority:
    /// 1. Try `tokenizer_config.json` for explicit IDs
    /// 2. Fall back to vocabulary lookup in `tokenizer.json`
    /// 3. Return error if EOS cannot be resolved
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Neither tokenizer file exists
    /// - EOS token cannot be found or resolved
    pub fn from_model_dir(model_dir: &Path) -> Result<Self> {
        let config_path = model_dir.join("tokenizer_config.json");
        let tokenizer_path = model_dir.join("tokenizer.json");

        // Try tokenizer_config.json first
        if config_path.exists() {
            match Self::from_tokenizer_config(&config_path, &tokenizer_path) {
                Ok(map) => return Ok(map),
                Err(e) => {
                    tracing::debug!(
                        path = %config_path.display(),
                        error = %e,
                        "tokenizer_config.json parsing failed, falling back to tokenizer.json"
                    );
                }
            }
        }

        // Fall back to tokenizer.json vocabulary lookup
        if tokenizer_path.exists() {
            return Self::from_tokenizer_vocab(&tokenizer_path);
        }

        Err(AosError::Validation(format!(
            "Neither tokenizer_config.json nor tokenizer.json found in {}",
            model_dir.display()
        )))
    }

    /// Load from tokenizer_config.json with optional vocab lookup.
    fn from_tokenizer_config(config_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path).map_err(|e| {
            AosError::Validation(format!("Failed to read tokenizer_config.json: {}", e))
        })?;

        let config: TokenizerConfigJson = serde_json::from_str(&content).map_err(|e| {
            AosError::Validation(format!("Failed to parse tokenizer_config.json: {}", e))
        })?;

        // Try to get EOS token ID directly
        let eos_token_id = config.eos_token_id.or_else(|| {
            // Handle different token representations
            match &config.eos_token {
                Some(TokenOrId::Id(id)) => Some(*id),
                Some(TokenOrId::String(token)) => {
                    Self::lookup_token_in_vocab(tokenizer_path, token)
                }
                Some(TokenOrId::Object { content }) => {
                    Self::lookup_token_in_vocab(tokenizer_path, content)
                }
                None => None,
            }
        });

        let eos_token_id = eos_token_id.ok_or_else(|| {
            AosError::Validation(
                "EOS token ID not found in tokenizer_config.json and cannot be resolved"
                    .to_string(),
            )
        })?;

        // Resolve other token IDs
        let bos_token_id = config.bos_token_id.or_else(|| match &config.bos_token {
            Some(TokenOrId::Id(id)) => Some(*id),
            Some(TokenOrId::String(token)) => Self::lookup_token_in_vocab(tokenizer_path, token),
            Some(TokenOrId::Object { content }) => {
                Self::lookup_token_in_vocab(tokenizer_path, content)
            }
            None => None,
        });

        let pad_token_id = config.pad_token_id.or_else(|| match &config.pad_token {
            Some(TokenOrId::Id(id)) => Some(*id),
            Some(TokenOrId::String(token)) => Self::lookup_token_in_vocab(tokenizer_path, token),
            Some(TokenOrId::Object { content }) => {
                Self::lookup_token_in_vocab(tokenizer_path, content)
            }
            None => None,
        });

        let unk_token_id = config.unk_token_id.or_else(|| match &config.unk_token {
            Some(TokenOrId::Id(id)) => Some(*id),
            Some(TokenOrId::String(token)) => Self::lookup_token_in_vocab(tokenizer_path, token),
            Some(TokenOrId::Object { content }) => {
                Self::lookup_token_in_vocab(tokenizer_path, content)
            }
            None => None,
        });

        // Check for ChatML-style tokens
        let im_start_id = Self::lookup_token_in_vocab(tokenizer_path, "<|im_start|>");
        let im_end_id = Self::lookup_token_in_vocab(tokenizer_path, "<|im_end|>");

        // Check for FIM tokens (code completion models)
        let fim_prefix_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_PREFIX_TOKEN);
        let fim_suffix_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_SUFFIX_TOKEN);
        let fim_middle_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_MIDDLE_TOKEN);

        Ok(Self {
            eos_token_id,
            bos_token_id,
            pad_token_id,
            unk_token_id,
            im_start_id,
            im_end_id,
            fim_prefix_id,
            fim_suffix_id,
            fim_middle_id,
            source: TokenMapSource::TokenizerConfig,
        })
    }

    /// Load from tokenizer.json vocabulary by searching for known tokens.
    fn from_tokenizer_vocab(tokenizer_path: &Path) -> Result<Self> {
        // Find EOS token
        let eos_token_id = EOS_TOKEN_CANDIDATES
            .iter()
            .find_map(|token| Self::lookup_token_in_vocab(tokenizer_path, token))
            .ok_or_else(|| {
                AosError::Validation(format!(
                    "EOS token not found in tokenizer vocabulary. Searched: {:?}",
                    EOS_TOKEN_CANDIDATES
                ))
            })?;

        // Find BOS token (optional)
        let bos_token_id = BOS_TOKEN_CANDIDATES
            .iter()
            .find_map(|token| Self::lookup_token_in_vocab(tokenizer_path, token));

        // Check for ChatML-style tokens
        let im_start_id = Self::lookup_token_in_vocab(tokenizer_path, "<|im_start|>");
        let im_end_id = Self::lookup_token_in_vocab(tokenizer_path, "<|im_end|>");

        // Check for FIM tokens (code completion models)
        let fim_prefix_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_PREFIX_TOKEN);
        let fim_suffix_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_SUFFIX_TOKEN);
        let fim_middle_id = Self::lookup_token_in_vocab(tokenizer_path, FIM_MIDDLE_TOKEN);

        Ok(Self {
            eos_token_id,
            bos_token_id,
            pad_token_id: None,
            unk_token_id: None,
            im_start_id,
            im_end_id,
            fim_prefix_id,
            fim_suffix_id,
            fim_middle_id,
            source: TokenMapSource::VocabLookup,
        })
    }

    /// Look up a token string in the tokenizer vocabulary.
    fn lookup_token_in_vocab(tokenizer_path: &Path, token: &str) -> Option<u32> {
        // Load tokenizer using the tokenizers crate
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path).ok()?;
        tokenizer.token_to_id(token)
    }

    /// Validate tokenizer schema and return metadata (hash, vocab size, etc.).
    ///
    /// Performs light-weight structural checks before the MLX backend attempts
    /// to load the tokenizer. This prevents late runtime failures when an
    /// incomplete tokenizer.json slips through packaging.
    pub fn validate_tokenizer(
        tokenizer_path: &Path,
        expected_vocab_size: Option<usize>,
    ) -> Result<TokenizerMetadata> {
        let raw = std::fs::read(tokenizer_path)
            .map_err(|e| AosError::Validation(format!("Failed to read tokenizer.json: {}", e)))?;

        let json: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| {
            AosError::Validation(format!("tokenizer.json is not valid JSON: {}", e))
        })?;

        // Basic structural guards
        let model = json
            .get("model")
            .and_then(|m| m.get("type").or_else(|| m.get("name")));
        if model.is_none() {
            return Err(AosError::Validation(
                "tokenizer.json missing model.type/name field".to_string(),
            ));
        }

        // Use tokenizers crate to ensure the file is fully parsable and
        // discover the vocabulary size (includes added tokens to match model embedding size).
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path).map_err(|e| {
            AosError::Validation(format!(
                "Failed to parse tokenizer.json: {} (unsupported schema; update tokenizers crate)",
                e
            ))
        })?;
        let vocab_size = tokenizer.get_vocab_size(true);

        if let Some(expected) = expected_vocab_size {
            // Some packaged models (and/or tokenizers) report a tokenizer vocab size that is
            // smaller than the model's embedding vocab size (e.g., reserved/unused IDs).
            // This is safe as long as the tokenizer never emits IDs >= expected.
            //
            // Guard the dangerous direction (tokenizer > model vocab), but allow
            // tokenizer < model vocab to proceed.
            if vocab_size > expected {
                return Err(AosError::Validation(format!(
                    "Tokenizer vocab_size {} exceeds manifest/base config {}",
                    vocab_size, expected
                )));
            }
        }

        let added_tokens = json
            .get("added_tokens")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        let normalizer = json
            .get("normalizer")
            .and_then(|n| n.get("type").or_else(|| n.get("name")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let hash = B3Hash::hash(&raw);

        Ok(TokenizerMetadata {
            hash,
            vocab_size,
            added_tokens,
            normalizer,
        })
    }

    /// Check whether all three FIM tokens are available.
    pub fn has_fim_support(&self) -> bool {
        self.fim_prefix_id.is_some() && self.fim_suffix_id.is_some() && self.fim_middle_id.is_some()
    }

    /// Extract resolved FIM token IDs, returning `None` if any are missing.
    pub fn fim_tokens(&self) -> Option<FIMTokens> {
        Some(FIMTokens {
            prefix_id: self.fim_prefix_id?,
            suffix_id: self.fim_suffix_id?,
            middle_id: self.fim_middle_id?,
        })
    }

    /// Compute BLAKE3 digest of the tokenizer file for receipts.
    pub fn compute_tokenizer_digest(tokenizer_path: &Path) -> Result<[u8; 32]> {
        let content = std::fs::read(tokenizer_path).map_err(|e| {
            AosError::Validation(format!("Failed to read tokenizer for hashing: {}", e))
        })?;
        let hash = blake3::hash(&content);
        Ok(*hash.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_eos_token_candidates_not_empty() {
        assert!(!EOS_TOKEN_CANDIDATES.is_empty());
    }

    #[test]
    fn test_bos_token_candidates_not_empty() {
        assert!(!BOS_TOKEN_CANDIDATES.is_empty());
    }

    #[test]
    fn test_special_token_map_serialization() {
        let map = SpecialTokenMap {
            eos_token_id: 151645,
            bos_token_id: Some(151644),
            pad_token_id: None,
            unk_token_id: None,
            im_start_id: Some(151644),
            im_end_id: Some(151645),
            fim_prefix_id: None,
            fim_suffix_id: None,
            fim_middle_id: None,
            source: TokenMapSource::TokenizerConfig,
        };
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"eos_token_id\":151645"));
    }

    #[test]
    fn test_fim_tokens_extraction() {
        let map = SpecialTokenMap {
            eos_token_id: 151645,
            bos_token_id: None,
            pad_token_id: None,
            unk_token_id: None,
            im_start_id: None,
            im_end_id: None,
            fim_prefix_id: Some(151659),
            fim_suffix_id: Some(151661),
            fim_middle_id: Some(151660),
            source: TokenMapSource::Unknown,
        };
        assert!(map.has_fim_support());
        let fim = map.fim_tokens().unwrap();
        assert_eq!(fim.prefix_id, 151659);
        assert_eq!(fim.suffix_id, 151661);
        assert_eq!(fim.middle_id, 151660);
    }

    #[test]
    fn test_fim_tokens_partial_returns_none() {
        let map = SpecialTokenMap {
            eos_token_id: 151645,
            bos_token_id: None,
            pad_token_id: None,
            unk_token_id: None,
            im_start_id: None,
            im_end_id: None,
            fim_prefix_id: Some(151659),
            fim_suffix_id: None, // missing
            fim_middle_id: Some(151660),
            source: TokenMapSource::Unknown,
        };
        assert!(!map.has_fim_support());
        assert!(map.fim_tokens().is_none());
    }

    #[test]
    fn test_validate_tokenizer_catches_missing_model() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokenizer.json");
        let mut f = File::create(&path).unwrap();
        writeln!(f, "{{}}\n").unwrap();

        let err = SpecialTokenMap::validate_tokenizer(&path, None).unwrap_err();
        assert!(format!("{}", err).contains("missing model"));
    }

    #[test]
    fn test_validate_tokenizer_checks_vocab_size() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokenizer.json");
        // Minimal valid tokenizer: WordLevel with two tokens
        let json = r#"{
            "model": {"type": "WordLevel", "vocab": {"hello":0, "world":1}, "unk_token": "[UNK]"},
            "pre_tokenizer": {"type": "Whitespace"}
        }"#;
        std::fs::write(&path, json).unwrap();

        // Tokenizer smaller than expected is allowed (reserved/unused IDs in embedding space).
        let meta = SpecialTokenMap::validate_tokenizer(&path, Some(3)).unwrap();
        assert_eq!(meta.vocab_size, 2);

        // Matching vocab size succeeds
        let meta = SpecialTokenMap::validate_tokenizer(&path, Some(2)).unwrap();
        assert_eq!(meta.vocab_size, 2);

        // Dangerous direction should error (tokenizer > expected)
        let err = SpecialTokenMap::validate_tokenizer(&path, Some(1)).unwrap_err();
        assert!(format!("{}", err).contains("vocab_size"));
    }
}

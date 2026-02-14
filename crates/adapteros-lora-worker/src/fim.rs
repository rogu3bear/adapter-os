//! Fill-in-the-Middle (FIM) prompt builder and stop conditions.
//!
//! Constructs FIM token sequences for code completion and provides
//! FIM-specific stop token detection. The FIM format is:
//!
//! ```text
//! <|fim_prefix|>{prefix}<|fim_suffix|>{suffix}<|fim_middle|>{generated}
//! ```
//!
//! The model generates tokens after `<|fim_middle|>` until a stop condition
//! is met (EOS, FIM boundary token, or max tokens).

use adapteros_core::tokenizer_config::FIMTokens;
use adapteros_core::{AosError, Result};

/// Build a FIM prompt token sequence.
///
/// Encodes prefix and suffix text using the provided tokenizer, then wraps
/// them with the FIM special tokens in the standard PSM (prefix-suffix-middle)
/// order used by Qwen2.5 and compatible models.
///
/// # Arguments
/// * `tokenizer` - Tokenizer for encoding text to token IDs
/// * `fim` - Resolved FIM token IDs
/// * `prefix` - Code before the cursor
/// * `suffix` - Code after the cursor
///
/// # Returns
/// Token ID sequence ready for model input: `[fim_prefix, ...prefix_tokens, fim_suffix, ...suffix_tokens, fim_middle]`
pub fn build_fim_prompt(
    tokenizer: &tokenizers::Tokenizer,
    fim: &FIMTokens,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<u32>> {
    let prefix_encoding = tokenizer
        .encode(prefix, false)
        .map_err(|e| AosError::Worker(format!("FIM prefix encoding failed: {}", e)))?;
    let suffix_encoding = tokenizer
        .encode(suffix, false)
        .map_err(|e| AosError::Worker(format!("FIM suffix encoding failed: {}", e)))?;

    let prefix_ids = prefix_encoding.get_ids();
    let suffix_ids = suffix_encoding.get_ids();

    let mut tokens = Vec::with_capacity(prefix_ids.len() + suffix_ids.len() + 3);
    tokens.push(fim.prefix_id);
    tokens.extend_from_slice(prefix_ids);
    tokens.push(fim.suffix_id);
    tokens.extend_from_slice(suffix_ids);
    tokens.push(fim.middle_id);

    Ok(tokens)
}

/// FIM-specific stop token IDs.
///
/// In addition to the model's EOS token, FIM generation should stop when it
/// encounters the start of a new FIM block (`<|fim_prefix|>`) or any other
/// FIM boundary marker. This prevents the model from generating into the
/// next fill-in-the-middle block.
pub fn fim_stop_tokens(fim: &FIMTokens, eos_token_id: u32) -> Vec<u32> {
    vec![
        eos_token_id,
        fim.prefix_id, // start of next FIM block
    ]
}

/// Check whether a generated token should stop FIM generation.
pub fn is_fim_stop_token(token_id: u32, fim: &FIMTokens, eos_token_id: u32) -> bool {
    token_id == eos_token_id || token_id == fim.prefix_id
}

/// Validate that FIM special token strings encode to the expected single token IDs.
///
/// This catches model/tokenizer mismatches where the tokenizer doesn't have FIM
/// tokens as added tokens and would shred `<|fim_prefix|>` into sub-word pieces.
/// Should be called once at tokenizer load time when FIM support is detected.
pub fn validate_fim_token_encoding(
    tokenizer: &tokenizers::Tokenizer,
    fim: &FIMTokens,
) -> Result<()> {
    let checks = [
        ("<|fim_prefix|>", fim.prefix_id),
        ("<|fim_suffix|>", fim.suffix_id),
        ("<|fim_middle|>", fim.middle_id),
    ];

    for (token_str, expected_id) in checks {
        let encoding = tokenizer
            .encode(token_str, false)
            .map_err(|e| AosError::Worker(format!("FIM validation encoding failed: {}", e)))?;

        let ids = encoding.get_ids();
        if ids.len() != 1 || ids[0] != expected_id {
            return Err(AosError::Validation(format!(
                "FIM token '{}' does not encode to single ID {}; got {:?}. \
                 Model tokenizer may not support FIM.",
                token_str, expected_id, ids
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokenizers::Tokenizer;

    fn make_test_fim() -> FIMTokens {
        FIMTokens {
            prefix_id: 151659,
            suffix_id: 151661,
            middle_id: 151660,
        }
    }

    #[test]
    fn test_build_fim_prompt_structure() {
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let fim = make_test_fim();

        // Empty prefix and suffix should produce just the 3 special tokens
        let tokens = build_fim_prompt(&tokenizer, &fim, "", "").unwrap();
        assert_eq!(tokens, vec![151659, 151661, 151660]);
    }

    #[test]
    fn test_build_fim_prompt_ordering() {
        // The sequence should always be: prefix_id, ..., suffix_id, ..., middle_id
        let tokenizer = Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let fim = make_test_fim();

        let tokens = build_fim_prompt(&tokenizer, &fim, "hello", "world").unwrap();

        assert_eq!(tokens[0], fim.prefix_id);
        assert_eq!(tokens[tokens.len() - 1], fim.middle_id);

        // suffix_id should appear somewhere in the middle
        let suffix_pos = tokens.iter().position(|&t| t == fim.suffix_id).unwrap();
        assert!(suffix_pos > 0);
        assert!(suffix_pos < tokens.len() - 1);
    }

    #[test]
    fn test_fim_stop_tokens_includes_eos_and_prefix() {
        let fim = make_test_fim();
        let stops = fim_stop_tokens(&fim, 151645);
        assert!(stops.contains(&151645)); // EOS
        assert!(stops.contains(&151659)); // fim_prefix (block boundary)
    }

    #[test]
    fn test_is_fim_stop_token() {
        let fim = make_test_fim();
        assert!(is_fim_stop_token(151645, &fim, 151645)); // EOS
        assert!(is_fim_stop_token(151659, &fim, 151645)); // fim_prefix
        assert!(!is_fim_stop_token(42, &fim, 151645)); // random token
    }
}

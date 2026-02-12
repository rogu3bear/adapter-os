//! Prefix KV cache key computation
//!
//! This module provides cryptographic key computation for prefix KV caching.
//! The prefix KV key uniquely identifies a cached prefix based on:
//! - Context digest (tenant/adapter context)
//! - Prefix token IDs (the actual tokenized prefix)
//! - Tokenizer manifest hash (tokenizer identity)
//! - Model cache identity V2 bytes (kernel/quant/fusion/tokenizer binding)
//!
//! See PRD: PrefixKvCache v1

use crate::{AosError, B3Hash, Result};

/// Compute the cryptographic prefix KV cache key.
///
/// Per PRD spec:
/// ```text
/// prefix_kv_key_b3 = BLAKE3(
///     context_digest ||
///     prefix_token_ids_bytes ||
///     tokenizer_manifest_hash ||
///     model_cache_identity_v2_bytes
/// )
/// ```
///
/// # Arguments
/// * `context_digest` - BLAKE3 hash of the context manifest (tenant + adapter stack)
/// * `prefix_token_ids` - Tokenized prefix (sequence of token IDs)
/// * `tokenizer_manifest_hash` - Combined hash of tokenizer files
/// * `model_cache_identity_v2_bytes` - Canonical bytes of ModelCacheIdentityV2
///
/// # Returns
/// BLAKE3 hash that uniquely identifies this prefix KV cache entry.
///
/// # Determinism
/// This function is fully deterministic. Identical inputs always produce
/// identical outputs across all platforms and Rust versions.
pub fn compute_prefix_kv_key(
    context_digest: &B3Hash,
    prefix_token_ids: &[u32],
    tokenizer_manifest_hash: &B3Hash,
    model_cache_identity_v2_bytes: &[u8],
) -> B3Hash {
    let mut hasher = blake3::Hasher::new();

    // Context digest (32 bytes)
    hasher.update(context_digest.as_bytes());

    // Prefix token IDs (canonical encoding)
    let token_bytes = encode_prefix_tokens(prefix_token_ids);
    hasher.update(&token_bytes);

    // Tokenizer manifest hash (32 bytes)
    hasher.update(tokenizer_manifest_hash.as_bytes());

    // Model cache identity V2 canonical bytes
    hasher.update(model_cache_identity_v2_bytes);

    B3Hash::from_bytes(*hasher.finalize().as_bytes())
}

/// Encode prefix token IDs into canonical bytes.
///
/// Format:
/// - 4 bytes: token count (u32 little-endian)
/// - For each token: 4 bytes (u32 little-endian)
///
/// This encoding is platform-independent and produces identical
/// output for identical input sequences.
pub fn encode_prefix_tokens(tokens: &[u32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + tokens.len() * 4);

    // Token count
    buf.extend_from_slice(&(tokens.len() as u32).to_le_bytes());

    // Each token as u32 LE
    for token in tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }

    buf
}

/// Compute a combined tokenizer manifest hash from component hashes.
///
/// This combines the tokenizer.json hash and tokenizer_config.json hash
/// into a single manifest hash for cache key computation.
///
/// Format: BLAKE3(tokenizer_hash || tokenizer_cfg_hash)
pub fn compute_tokenizer_manifest_hash(
    tokenizer_hash: &B3Hash,
    tokenizer_cfg_hash: &B3Hash,
) -> B3Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(tokenizer_hash.as_bytes());
    hasher.update(tokenizer_cfg_hash.as_bytes());
    B3Hash::from_bytes(*hasher.finalize().as_bytes())
}

/// Builder for prefix KV key computation.
///
/// Provides a fluent API for constructing prefix KV keys with validation.
#[derive(Debug, Clone)]
pub struct PrefixKvKeyBuilder {
    context_digest: Option<B3Hash>,
    prefix_token_ids: Option<Vec<u32>>,
    tokenizer_manifest_hash: Option<B3Hash>,
    model_cache_identity_bytes: Option<Vec<u8>>,
}

impl Default for PrefixKvKeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefixKvKeyBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            context_digest: None,
            prefix_token_ids: None,
            tokenizer_manifest_hash: None,
            model_cache_identity_bytes: None,
        }
    }

    /// Set the context digest
    pub fn context_digest(mut self, digest: B3Hash) -> Self {
        self.context_digest = Some(digest);
        self
    }

    /// Set the prefix token IDs
    pub fn prefix_tokens(mut self, tokens: Vec<u32>) -> Self {
        self.prefix_token_ids = Some(tokens);
        self
    }

    /// Set the tokenizer manifest hash
    pub fn tokenizer_manifest_hash(mut self, hash: B3Hash) -> Self {
        self.tokenizer_manifest_hash = Some(hash);
        self
    }

    /// Set the tokenizer hashes and compute the manifest hash
    pub fn tokenizer_hashes(mut self, tokenizer_hash: B3Hash, tokenizer_cfg_hash: B3Hash) -> Self {
        self.tokenizer_manifest_hash = Some(compute_tokenizer_manifest_hash(
            &tokenizer_hash,
            &tokenizer_cfg_hash,
        ));
        self
    }

    /// Set the model cache identity V2 bytes
    pub fn model_cache_identity_bytes(mut self, bytes: Vec<u8>) -> Self {
        self.model_cache_identity_bytes = Some(bytes);
        self
    }

    /// Build the prefix KV key
    ///
    /// # Errors
    /// Returns `AosError::Validation` if any required field is not set.
    pub fn build(self) -> Result<B3Hash> {
        let context_digest = self.context_digest.ok_or_else(|| {
            AosError::Validation("context_digest is required for prefix KV key".into())
        })?;
        let prefix_token_ids = self.prefix_token_ids.ok_or_else(|| {
            AosError::Validation("prefix_token_ids is required for prefix KV key".into())
        })?;
        let tokenizer_manifest_hash = self.tokenizer_manifest_hash.ok_or_else(|| {
            AosError::Validation("tokenizer_manifest_hash is required for prefix KV key".into())
        })?;
        let model_cache_identity_bytes = self.model_cache_identity_bytes.ok_or_else(|| {
            AosError::Validation("model_cache_identity_bytes is required for prefix KV key".into())
        })?;

        Ok(compute_prefix_kv_key(
            &context_digest,
            &prefix_token_ids,
            &tokenizer_manifest_hash,
            &model_cache_identity_bytes,
        ))
    }

    /// Try to build the prefix KV key, returning None if any field is missing
    pub fn try_build(self) -> Option<B3Hash> {
        let context_digest = self.context_digest?;
        let prefix_token_ids = self.prefix_token_ids?;
        let tokenizer_manifest_hash = self.tokenizer_manifest_hash?;
        let model_cache_identity_bytes = self.model_cache_identity_bytes?;

        Some(compute_prefix_kv_key(
            &context_digest,
            &prefix_token_ids,
            &tokenizer_manifest_hash,
            &model_cache_identity_bytes,
        ))
    }

    /// Build the prefix KV key, returning an error if any field is missing.
    ///
    /// This is the recommended method for production code as it provides
    /// detailed error information about which field is missing.
    ///
    /// # Errors
    /// Returns `AosError::Validation` if any required field is not set,
    /// with a message indicating which field is missing.
    ///
    /// # Example
    /// ```
    /// use adapteros_core::prefix_kv_key::PrefixKvKeyBuilder;
    /// use adapteros_core::B3Hash;
    ///
    /// let result = PrefixKvKeyBuilder::new()
    ///     .context_digest(B3Hash::hash(b"ctx"))
    ///     .prefix_tokens(vec![1, 2, 3])
    ///     .tokenizer_manifest_hash(B3Hash::hash(b"tok"))
    ///     .model_cache_identity_bytes(vec![1, 2, 3, 4])
    ///     .build_result();
    ///
    /// assert!(result.is_ok());
    /// ```
    pub fn build_result(self) -> Result<B3Hash> {
        let context_digest = self.context_digest.ok_or_else(|| {
            AosError::Validation("context_digest is required for prefix KV key".to_string())
        })?;
        let prefix_token_ids = self.prefix_token_ids.ok_or_else(|| {
            AosError::Validation("prefix_token_ids is required for prefix KV key".to_string())
        })?;
        let tokenizer_manifest_hash = self.tokenizer_manifest_hash.ok_or_else(|| {
            AosError::Validation(
                "tokenizer_manifest_hash is required for prefix KV key".to_string(),
            )
        })?;
        let model_cache_identity_bytes = self.model_cache_identity_bytes.ok_or_else(|| {
            AosError::Validation(
                "model_cache_identity_bytes is required for prefix KV key".to_string(),
            )
        })?;

        Ok(compute_prefix_kv_key(
            &context_digest,
            &prefix_token_ids,
            &tokenizer_manifest_hash,
            &model_cache_identity_bytes,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_prefix_tokens_empty() {
        let tokens: Vec<u32> = vec![];
        let encoded = encode_prefix_tokens(&tokens);

        assert_eq!(encoded.len(), 4); // Just the count
        assert_eq!(&encoded[..4], &0u32.to_le_bytes());
    }

    #[test]
    fn test_encode_prefix_tokens_single() {
        let tokens = vec![42u32];
        let encoded = encode_prefix_tokens(&tokens);

        assert_eq!(encoded.len(), 8); // count + 1 token
        assert_eq!(&encoded[..4], &1u32.to_le_bytes()); // count = 1
        assert_eq!(&encoded[4..8], &42u32.to_le_bytes()); // token = 42
    }

    #[test]
    fn test_encode_prefix_tokens_multiple() {
        let tokens = vec![1u32, 2, 3, 4, 5];
        let encoded = encode_prefix_tokens(&tokens);

        assert_eq!(encoded.len(), 24); // count + 5 tokens
        assert_eq!(&encoded[..4], &5u32.to_le_bytes());
    }

    #[test]
    fn test_encode_prefix_tokens_deterministic() {
        let tokens = vec![100u32, 200, 300];
        let encoded1 = encode_prefix_tokens(&tokens);
        let encoded2 = encode_prefix_tokens(&tokens);

        assert_eq!(encoded1, encoded2, "encoding must be deterministic");
    }

    #[test]
    fn test_compute_prefix_kv_key_deterministic() {
        let context = B3Hash::hash(b"test_context");
        let tokens = vec![1u32, 2, 3];
        let tokenizer = B3Hash::hash(b"tokenizer");
        let identity = vec![1u8, 2, 3, 4];

        let key1 = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity);
        let key2 = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity);

        assert_eq!(key1, key2, "prefix KV key must be deterministic");
    }

    #[test]
    fn test_compute_prefix_kv_key_different_context() {
        let context1 = B3Hash::hash(b"context_1");
        let context2 = B3Hash::hash(b"context_2");
        let tokens = vec![1u32, 2, 3];
        let tokenizer = B3Hash::hash(b"tokenizer");
        let identity = vec![1u8, 2, 3, 4];

        let key1 = compute_prefix_kv_key(&context1, &tokens, &tokenizer, &identity);
        let key2 = compute_prefix_kv_key(&context2, &tokens, &tokenizer, &identity);

        assert_ne!(key1, key2, "different contexts must produce different keys");
    }

    #[test]
    fn test_compute_prefix_kv_key_different_tokens() {
        let context = B3Hash::hash(b"context");
        let tokens1 = vec![1u32, 2, 3];
        let tokens2 = vec![1u32, 2, 4];
        let tokenizer = B3Hash::hash(b"tokenizer");
        let identity = vec![1u8, 2, 3, 4];

        let key1 = compute_prefix_kv_key(&context, &tokens1, &tokenizer, &identity);
        let key2 = compute_prefix_kv_key(&context, &tokens2, &tokenizer, &identity);

        assert_ne!(key1, key2, "different tokens must produce different keys");
    }

    #[test]
    fn test_compute_prefix_kv_key_different_tokenizer() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let tokenizer1 = B3Hash::hash(b"tokenizer_v1");
        let tokenizer2 = B3Hash::hash(b"tokenizer_v2");
        let identity = vec![1u8, 2, 3, 4];

        let key1 = compute_prefix_kv_key(&context, &tokens, &tokenizer1, &identity);
        let key2 = compute_prefix_kv_key(&context, &tokens, &tokenizer2, &identity);

        assert_ne!(
            key1, key2,
            "different tokenizers must produce different keys"
        );
    }

    #[test]
    fn test_compute_prefix_kv_key_different_identity() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let tokenizer = B3Hash::hash(b"tokenizer");
        let identity1 = vec![1u8, 2, 3, 4];
        let identity2 = vec![1u8, 2, 3, 5];

        let key1 = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity1);
        let key2 = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity2);

        assert_ne!(
            key1, key2,
            "different identities must produce different keys"
        );
    }

    #[test]
    fn test_tokenizer_manifest_hash_deterministic() {
        let tok = B3Hash::hash(b"tokenizer.json");
        let cfg = B3Hash::hash(b"tokenizer_config.json");

        let hash1 = compute_tokenizer_manifest_hash(&tok, &cfg);
        let hash2 = compute_tokenizer_manifest_hash(&tok, &cfg);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_tokenizer_manifest_hash_different_inputs() {
        let tok1 = B3Hash::hash(b"tokenizer_v1.json");
        let tok2 = B3Hash::hash(b"tokenizer_v2.json");
        let cfg = B3Hash::hash(b"tokenizer_config.json");

        let hash1 = compute_tokenizer_manifest_hash(&tok1, &cfg);
        let hash2 = compute_tokenizer_manifest_hash(&tok2, &cfg);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_builder_build() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let tok = B3Hash::hash(b"tokenizer.json");
        let cfg = B3Hash::hash(b"tokenizer_config.json");
        let identity = vec![1u8, 2, 3, 4];

        let key = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens.clone())
            .tokenizer_hashes(tok, cfg)
            .model_cache_identity_bytes(identity.clone())
            .build()
            .expect("all fields set");

        // Verify it matches direct computation
        let manifest_hash = compute_tokenizer_manifest_hash(&tok, &cfg);
        let expected = compute_prefix_kv_key(&context, &tokens, &manifest_hash, &identity);

        assert_eq!(key, expected);
    }

    #[test]
    fn test_builder_try_build_success() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let manifest = B3Hash::hash(b"manifest");
        let identity = vec![1u8];

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens)
            .tokenizer_manifest_hash(manifest)
            .model_cache_identity_bytes(identity)
            .try_build();

        assert!(result.is_some());
    }

    #[test]
    fn test_builder_try_build_missing_field() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        // Missing tokenizer_manifest_hash and identity

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens)
            .try_build();

        assert!(result.is_none());
    }

    #[test]
    fn test_builder_build_result_success() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let manifest = B3Hash::hash(b"manifest");
        let identity = vec![1u8];

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens)
            .tokenizer_manifest_hash(manifest)
            .model_cache_identity_bytes(identity)
            .build_result();

        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_build_result_missing_context() {
        let tokens = vec![1u32, 2, 3];
        let manifest = B3Hash::hash(b"manifest");
        let identity = vec![1u8];

        let result = PrefixKvKeyBuilder::new()
            .prefix_tokens(tokens)
            .tokenizer_manifest_hash(manifest)
            .model_cache_identity_bytes(identity)
            .build_result();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("context_digest"));
    }

    #[test]
    fn test_builder_build_result_missing_tokens() {
        let context = B3Hash::hash(b"context");
        let manifest = B3Hash::hash(b"manifest");
        let identity = vec![1u8];

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .tokenizer_manifest_hash(manifest)
            .model_cache_identity_bytes(identity)
            .build_result();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("prefix_token_ids"));
    }

    #[test]
    fn test_builder_build_result_missing_tokenizer() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let identity = vec![1u8];

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens)
            .model_cache_identity_bytes(identity)
            .build_result();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("tokenizer_manifest_hash"));
    }

    #[test]
    fn test_builder_build_result_missing_identity() {
        let context = B3Hash::hash(b"context");
        let tokens = vec![1u32, 2, 3];
        let manifest = B3Hash::hash(b"manifest");

        let result = PrefixKvKeyBuilder::new()
            .context_digest(context)
            .prefix_tokens(tokens)
            .tokenizer_manifest_hash(manifest)
            .build_result();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("model_cache_identity_bytes"));
    }
}

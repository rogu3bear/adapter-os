//! Cache Prefix Lookup for KV Cache Reuse
//!
//! This module implements deterministic cache prefix lookup for reusing
//! pre-computed KV cache states. The lookup result binds into the receipt
//! for accurate usage attribution and third-party verification.
//!
//! ## Receipt Binding
//!
//! The lookup produces values that flow directly into `RunReceipt`:
//! - `cache_id` → `prefix_kv_key_b3` (deterministic BLAKE3 hash)
//! - `cached_token_count` → `prefix_cached_token_count` (u32)
//! - `cached_kv_bytes` → `prefix_kv_bytes` (u64)
//! - `cache_hit` → `prefix_cache_hit` (bool)
//!
//! All receipt-bound values are integers or hashes. The floating-point KV
//! tensors themselves are intermediate computation cache, not receipt material.
//!
//! ## Verification
//!
//! A third-party verifier with only the receipt can:
//! 1. Recompute `prefix_kv_key_b3` from (context_digest, tokens, tokenizer_hash, model_identity)
//! 2. Verify `billed_input_tokens = logical_prompt_tokens - prefix_cached_token_count`
//!
//! The verifier cannot verify KV tensor correctness (would require recomputation),
//! but the deterministic key proves the claimed cache entry identity.
//!
//! ## Stop Conditions (complete partition)
//!
//! - `EmptyInput` → input tokens empty
//! - `NoMatchingPrefix` → no cache entry matches
//! - `IntegrityValidationFailed` → entry found but corrupted (invalidate & miss)
//! - `MaxPrefixLengthReached` → search truncated by config
//!
//! ## Determinism
//!
//! This module is fully deterministic. Identical inputs produce identical
//! outputs. The cache key computation uses BLAKE3 with canonical byte encoding.
//!
//! **Deterministic Tie-Breaking**: When multiple cache entries have equal match
//! lengths during longest-prefix search, tie-breaking uses lexicographic key
//! ordering (smallest key wins). The underlying `PrefixKvCache` uses `BTreeMap`
//! for deterministic iteration and logical ticks (not wall-clock time) for LRU
//! eviction ordering. This ensures reproducible behavior across process restarts.
//!
//! See PRD: PrefixKvCache v1

use crate::prefix_kv_cache::{PrefixKvCache, PrefixKvEntry, PrefixMatch};
use adapteros_core::cache_attestation::{CacheAttestation, CacheAttestationBuilder};
use adapteros_core::prefix_kv_key::compute_prefix_kv_key;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =============================================================================
// Cache Lookup Result (Receipt-Ready)
// =============================================================================

/// Result of a cache prefix lookup operation.
///
/// This struct captures exactly what's needed for receipt attribution.
/// All fields are either integers or hashes—no floating-point values
/// that would require Q15 quantization for deterministic storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLookupResult {
    /// Whether a cache entry was found.
    /// Maps to `RunReceipt::prefix_cache_hit`.
    pub cache_hit: bool,

    /// The cache_id (prefix_kv_key_b3) of the matched entry.
    /// Maps to `RunReceipt::prefix_kv_key_b3`.
    /// Deterministically computed from (context, tokens, tokenizer, model_identity).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_id: Option<B3Hash>,

    /// Number of tokens satisfied from the cache.
    /// Maps to `RunReceipt::prefix_cached_token_count`.
    pub cached_token_count: u32,

    /// Number of tokens requiring computation (total - cached).
    /// This becomes `billed_input_tokens` in the receipt.
    pub tokens_to_compute: u32,

    /// Total input tokens in the sequence.
    /// For receipt: `logical_prompt_tokens`.
    pub total_input_tokens: u32,

    /// Bytes of cached KV tensors (for memory accounting).
    /// Maps to `RunReceipt::prefix_kv_bytes`.
    pub cached_kv_bytes: u64,

    /// Whether this was an exact key match or partial prefix match.
    pub is_exact_match: bool,

    /// Reason for cache miss, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miss_reason: Option<CacheMissReason>,

    /// Cryptographic attestation for cache credits (P0-1: billing fraud prevention).
    /// Present only on cache hits. Must be verified by control plane before accepting
    /// cache credits in billing calculations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<CacheAttestation>,
}

impl CacheLookupResult {
    /// Create a cache miss result.
    pub fn miss(total_input_tokens: u32, reason: CacheMissReason) -> Self {
        Self {
            cache_hit: false,
            cache_id: None,
            cached_token_count: 0,
            tokens_to_compute: total_input_tokens,
            total_input_tokens,
            cached_kv_bytes: 0,
            is_exact_match: false,
            miss_reason: Some(reason),
            attestation: None,
        }
    }

    /// Create a cache hit result without attestation.
    ///
    /// For cache hits with billing credits, use `hit_with_attestation` instead
    /// to generate the cryptographic proof required by P0-1.
    pub fn hit(
        cache_id: B3Hash,
        cached_token_count: u32,
        total_input_tokens: u32,
        cached_kv_bytes: u64,
        is_exact_match: bool,
    ) -> Self {
        Self {
            cache_hit: true,
            cache_id: Some(cache_id),
            cached_token_count,
            tokens_to_compute: total_input_tokens.saturating_sub(cached_token_count),
            total_input_tokens,
            cached_kv_bytes,
            is_exact_match,
            miss_reason: None,
            attestation: None,
        }
    }

    /// Create a cache hit result with cryptographic attestation.
    ///
    /// This generates a signed attestation proving the cache hit is genuine,
    /// required for billing fraud prevention (P0-1).
    ///
    /// # Arguments
    ///
    /// * `cache_id` - BLAKE3 hash of the cache lookup key
    /// * `cached_token_count` - Number of tokens satisfied from cache
    /// * `total_input_tokens` - Total input tokens in the sequence
    /// * `cached_kv_bytes` - Bytes of cached KV tensors
    /// * `is_exact_match` - Whether this was exact or partial match
    /// * `worker_id` - Worker identifier for attestation
    /// * `timestamp_tick` - Logical tick (not wall time)
    /// * `signing_key` - Worker's Ed25519 signing key (32-byte seed)
    #[allow(clippy::too_many_arguments)]
    pub fn hit_with_attestation(
        cache_id: B3Hash,
        cached_token_count: u32,
        total_input_tokens: u32,
        cached_kv_bytes: u64,
        is_exact_match: bool,
        worker_id: &str,
        timestamp_tick: u64,
        signing_key: &[u8; 32],
    ) -> Result<Self> {
        // Generate attestation for the cache hit
        let attestation = CacheAttestationBuilder::new()
            .cache_key_b3(&cache_id)
            .token_count(cached_token_count)
            .worker_id(worker_id)
            .timestamp_tick(timestamp_tick)
            .build_and_sign(signing_key)?;

        Ok(Self {
            cache_hit: true,
            cache_id: Some(cache_id),
            cached_token_count,
            tokens_to_compute: total_input_tokens.saturating_sub(cached_token_count),
            total_input_tokens,
            cached_kv_bytes,
            is_exact_match,
            miss_reason: None,
            attestation: Some(attestation),
        })
    }

    /// Attach an attestation to this result.
    ///
    /// Use this to add an attestation after creating a hit result,
    /// for example when attestation parameters aren't available at hit time.
    pub fn with_attestation(mut self, attestation: CacheAttestation) -> Self {
        self.attestation = Some(attestation);
        self
    }

    /// Get billed input tokens (tokens requiring computation).
    ///
    /// This is `logical_prompt_tokens - prefix_cached_token_count` for receipts.
    #[inline]
    pub fn billed_input_tokens(&self) -> u32 {
        self.tokens_to_compute
    }
}

impl Default for CacheLookupResult {
    fn default() -> Self {
        Self::miss(0, CacheMissReason::EmptyInput)
    }
}

/// Reasons for cache miss (complete partition of miss states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMissReason {
    /// No cache entry matches any prefix (normal miss)
    NoMatchingPrefix,
    /// Cache entry failed integrity check (invalidated)
    IntegrityValidationFailed,
    /// Search stopped at configured max_prefix_length
    MaxPrefixLengthReached,
    /// Input token sequence was empty
    EmptyInput,
}

impl std::fmt::Display for CacheMissReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatchingPrefix => write!(f, "no matching prefix in cache"),
            Self::IntegrityValidationFailed => write!(f, "cache entry integrity failed"),
            Self::MaxPrefixLengthReached => write!(f, "max prefix length reached"),
            Self::EmptyInput => write!(f, "empty input tokens"),
        }
    }
}

// =============================================================================
// Lookup Configuration
// =============================================================================

/// Configuration for cache prefix lookup.
#[derive(Debug, Clone)]
pub struct CacheLookupConfig {
    /// Minimum tokens for a valid match. Default: 1
    pub min_match_tokens: u32,
    /// Maximum prefix length to search. Default: u32::MAX
    pub max_prefix_length: u32,
}

impl Default for CacheLookupConfig {
    fn default() -> Self {
        Self {
            min_match_tokens: 1,
            max_prefix_length: u32::MAX,
        }
    }
}

// =============================================================================
// Main Cache Prefix Lookup
// =============================================================================

/// Perform cache prefix lookup for a token sequence.
///
/// Returns a `CacheLookupResult` with all fields needed for receipt attribution.
///
/// # Determinism
///
/// This function is fully deterministic. The cache key is computed via
/// `compute_prefix_kv_key` using canonical BLAKE3 hashing.
///
/// # Arguments
/// * `cache` - The prefix KV cache
/// * `input_tokens` - Input token sequence
/// * `context_digest` - BLAKE3 hash of context manifest
/// * `tokenizer_hash` - Tokenizer manifest hash
/// * `model_identity_hash` - Model identity hash (for entry validation)
/// * `model_identity_bytes` - Canonical bytes (for key computation)
/// * `config` - Lookup configuration
pub fn cache_prefix_lookup(
    cache: &PrefixKvCache,
    input_tokens: &[u32],
    context_digest: &B3Hash,
    tokenizer_hash: &B3Hash,
    model_identity_hash: &B3Hash,
    model_identity_bytes: &[u8],
    config: &CacheLookupConfig,
) -> CacheLookupResult {
    let total_input_tokens = input_tokens.len() as u32;

    // Stop condition: Empty input
    if input_tokens.is_empty() {
        return CacheLookupResult::miss(0, CacheMissReason::EmptyInput);
    }

    // Compute deterministic cache key per PRD spec
    let exact_key = compute_prefix_kv_key(
        context_digest,
        input_tokens,
        tokenizer_hash,
        model_identity_bytes,
    );

    // Fast path: exact key match
    if let Some(entry) = cache.get(&exact_key) {
        // Sanity check: entry metadata should be consistent
        if !entry_metadata_valid(&entry, context_digest, tokenizer_hash, model_identity_hash) {
            tracing::warn!(
                cache_key = %exact_key.to_hex()[..16],
                "Cache entry metadata inconsistent, invalidating"
            );
            cache.remove(&exact_key);
            return CacheLookupResult::miss(
                total_input_tokens,
                CacheMissReason::IntegrityValidationFailed,
            );
        }

        return CacheLookupResult::hit(
            exact_key,
            entry.prefix_cached_token_count,
            total_input_tokens,
            entry.kv_bytes,
            true,
        );
    }

    // Slow path: longest-prefix matching
    let effective_max = std::cmp::min(config.max_prefix_length, total_input_tokens);
    let search_tokens = if effective_max < total_input_tokens {
        &input_tokens[..effective_max as usize]
    } else {
        input_tokens
    };

    match cache.find_longest_prefix_match(
        search_tokens,
        context_digest,
        tokenizer_hash,
        model_identity_hash,
        config.min_match_tokens,
    ) {
        Some(prefix_match) => {
            // Sanity check metadata
            if !entry_metadata_valid(
                &prefix_match.entry,
                context_digest,
                tokenizer_hash,
                model_identity_hash,
            ) {
                cache.remove(&prefix_match.cache_key);
                return CacheLookupResult::miss(
                    total_input_tokens,
                    CacheMissReason::IntegrityValidationFailed,
                );
            }

            CacheLookupResult::hit(
                prefix_match.cache_key,
                prefix_match.matched_token_count,
                total_input_tokens,
                prefix_match.entry.kv_bytes,
                prefix_match.is_full_match(),
            )
        }
        None => {
            let reason = if effective_max < total_input_tokens {
                CacheMissReason::MaxPrefixLengthReached
            } else {
                CacheMissReason::NoMatchingPrefix
            };
            CacheLookupResult::miss(total_input_tokens, reason)
        }
    }
}

/// Sanity check that entry metadata is consistent.
///
/// This is NOT integrity verification of KV tensors (impossible without recompute).
/// It only checks that stored metadata matches expected values, catching corruption.
#[inline]
fn entry_metadata_valid(
    entry: &PrefixKvEntry,
    expected_context: &B3Hash,
    expected_tokenizer: &B3Hash,
    expected_model: &B3Hash,
) -> bool {
    entry.context_digest == *expected_context
        && entry.tokenizer_hash == *expected_tokenizer
        && entry.model_identity_hash == *expected_model
        && entry.keys.len() == entry.values.len()
        && !entry.keys.is_empty()
}

// =============================================================================
// Attested Cache Lookup (P0-1: Billing Fraud Prevention)
// =============================================================================

/// Configuration for attested cache lookups.
#[derive(Debug, Clone)]
pub struct AttestedLookupConfig {
    /// Base lookup configuration
    pub lookup: CacheLookupConfig,
    /// Worker identifier for attestation
    pub worker_id: String,
    /// Worker's Ed25519 signing key (32-byte seed)
    pub signing_key: [u8; 32],
}

/// Perform cache prefix lookup with cryptographic attestation.
///
/// This function extends `cache_prefix_lookup` by generating a signed
/// attestation for cache hits. The attestation proves the cache hit is
/// genuine and must be verified by the control plane before accepting
/// cache credits in billing (P0-1: billing fraud prevention).
///
/// # Arguments
/// * `cache` - The prefix KV cache
/// * `input_tokens` - Input token sequence
/// * `context_digest` - BLAKE3 hash of context manifest
/// * `tokenizer_hash` - Tokenizer manifest hash
/// * `model_identity_hash` - Model identity hash (for entry validation)
/// * `model_identity_bytes` - Canonical bytes (for key computation)
/// * `config` - Attested lookup configuration (includes worker_id and signing_key)
/// * `timestamp_tick` - Logical tick for replay prevention
///
/// # Returns
///
/// A `CacheLookupResult` with attestation on cache hits.
#[allow(clippy::too_many_arguments)]
pub fn cache_prefix_lookup_attested(
    cache: &PrefixKvCache,
    input_tokens: &[u32],
    context_digest: &B3Hash,
    tokenizer_hash: &B3Hash,
    model_identity_hash: &B3Hash,
    model_identity_bytes: &[u8],
    config: &AttestedLookupConfig,
    timestamp_tick: u64,
) -> Result<CacheLookupResult> {
    let total_input_tokens = input_tokens.len() as u32;

    // Stop condition: Empty input
    if input_tokens.is_empty() {
        return Ok(CacheLookupResult::miss(0, CacheMissReason::EmptyInput));
    }

    // Compute deterministic cache key per PRD spec
    let exact_key = compute_prefix_kv_key(
        context_digest,
        input_tokens,
        tokenizer_hash,
        model_identity_bytes,
    );

    // Fast path: exact key match
    if let Some(entry) = cache.get(&exact_key) {
        // Sanity check: entry metadata should be consistent
        if !entry_metadata_valid(&entry, context_digest, tokenizer_hash, model_identity_hash) {
            tracing::warn!(
                cache_key = %exact_key.to_hex()[..16],
                "Cache entry metadata inconsistent, invalidating"
            );
            cache.remove(&exact_key);
            return Ok(CacheLookupResult::miss(
                total_input_tokens,
                CacheMissReason::IntegrityValidationFailed,
            ));
        }

        // Generate attestation for cache hit (P0-1)
        return CacheLookupResult::hit_with_attestation(
            exact_key,
            entry.prefix_cached_token_count,
            total_input_tokens,
            entry.kv_bytes,
            true,
            &config.worker_id,
            timestamp_tick,
            &config.signing_key,
        );
    }

    // Slow path: longest-prefix matching
    let effective_max = std::cmp::min(config.lookup.max_prefix_length, total_input_tokens);
    let search_tokens = if effective_max < total_input_tokens {
        &input_tokens[..effective_max as usize]
    } else {
        input_tokens
    };

    match cache.find_longest_prefix_match(
        search_tokens,
        context_digest,
        tokenizer_hash,
        model_identity_hash,
        config.lookup.min_match_tokens,
    ) {
        Some(prefix_match) => {
            // Sanity check metadata
            if !entry_metadata_valid(
                &prefix_match.entry,
                context_digest,
                tokenizer_hash,
                model_identity_hash,
            ) {
                cache.remove(&prefix_match.cache_key);
                return Ok(CacheLookupResult::miss(
                    total_input_tokens,
                    CacheMissReason::IntegrityValidationFailed,
                ));
            }

            // Generate attestation for cache hit (P0-1)
            CacheLookupResult::hit_with_attestation(
                prefix_match.cache_key,
                prefix_match.matched_token_count,
                total_input_tokens,
                prefix_match.entry.kv_bytes,
                prefix_match.is_full_match(),
                &config.worker_id,
                timestamp_tick,
                &config.signing_key,
            )
        }
        None => {
            let reason = if effective_max < total_input_tokens {
                CacheMissReason::MaxPrefixLengthReached
            } else {
                CacheMissReason::NoMatchingPrefix
            };
            Ok(CacheLookupResult::miss(total_input_tokens, reason))
        }
    }
}

// =============================================================================
// Cache Entry Handle (for deferred tensor loading)
// =============================================================================

/// Handle to a cache entry for deferred KV tensor loading.
///
/// Holds an Arc reference to the entry and provides methods to:
/// - Access receipt-relevant metadata
/// - Load KV tensors when ready for inference
/// - Manage refcount for eviction protection
#[derive(Debug, Clone)]
pub struct CacheEntryHandle {
    entry: Arc<PrefixKvEntry>,
    cache_key: B3Hash,
    matched_token_count: u32,
}

impl CacheEntryHandle {
    /// Create handle from a prefix match result.
    pub fn from_prefix_match(prefix_match: &PrefixMatch) -> Self {
        Self {
            entry: Arc::clone(&prefix_match.entry),
            cache_key: prefix_match.cache_key,
            matched_token_count: prefix_match.matched_token_count,
        }
    }

    /// Create handle from an exact match.
    pub fn from_exact_match(entry: Arc<PrefixKvEntry>, cache_key: B3Hash) -> Self {
        let matched_token_count = entry.prefix_cached_token_count;
        Self {
            entry,
            cache_key,
            matched_token_count,
        }
    }

    /// Get the cache key (for receipt).
    pub fn cache_key(&self) -> &B3Hash {
        &self.cache_key
    }

    /// Get matched token count (for receipt).
    pub fn matched_token_count(&self) -> u32 {
        self.matched_token_count
    }

    /// Get KV bytes (for receipt).
    pub fn kv_bytes(&self) -> u64 {
        self.entry.kv_bytes
    }

    /// Load KV tensors for inference.
    ///
    /// Returns (keys, values, token_count) where keys/values are per-layer.
    /// The f32 tensors are NOT receipt-bound—only the counts and key are.
    pub fn load_kv_tensors(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>, u32) {
        (
            self.entry.keys.clone(),
            self.entry.values.clone(),
            self.entry.prefix_cached_token_count,
        )
    }

    /// Acquire reference (eviction protection).
    pub fn acquire(&self) -> u32 {
        self.entry.acquire()
    }

    /// Release reference.
    pub fn release(&self) -> u32 {
        self.entry.release()
    }
}

// =============================================================================
// Combined Lookup + Handle
// =============================================================================

/// Result of lookup with optional entry handle for tensor loading.
pub struct CacheLookupWithTensors {
    /// The lookup result (receipt-ready)
    pub result: CacheLookupResult,
    /// Handle for loading tensors, if hit
    pub entry_handle: Option<CacheEntryHandle>,
}

/// Perform lookup and return handle for deferred tensor loading.
pub fn cache_prefix_lookup_with_tensors(
    cache: &PrefixKvCache,
    input_tokens: &[u32],
    context_digest: &B3Hash,
    tokenizer_hash: &B3Hash,
    model_identity_hash: &B3Hash,
    model_identity_bytes: &[u8],
    config: &CacheLookupConfig,
) -> CacheLookupWithTensors {
    let total_input_tokens = input_tokens.len() as u32;

    if input_tokens.is_empty() {
        return CacheLookupWithTensors {
            result: CacheLookupResult::miss(0, CacheMissReason::EmptyInput),
            entry_handle: None,
        };
    }

    let exact_key = compute_prefix_kv_key(
        context_digest,
        input_tokens,
        tokenizer_hash,
        model_identity_bytes,
    );

    // Try exact match
    if let Some(entry) = cache.get(&exact_key) {
        if entry_metadata_valid(&entry, context_digest, tokenizer_hash, model_identity_hash) {
            let handle = CacheEntryHandle::from_exact_match(Arc::clone(&entry), exact_key);
            return CacheLookupWithTensors {
                result: CacheLookupResult::hit(
                    exact_key,
                    entry.prefix_cached_token_count,
                    total_input_tokens,
                    entry.kv_bytes,
                    true,
                ),
                entry_handle: Some(handle),
            };
        } else {
            cache.remove(&exact_key);
            return CacheLookupWithTensors {
                result: CacheLookupResult::miss(
                    total_input_tokens,
                    CacheMissReason::IntegrityValidationFailed,
                ),
                entry_handle: None,
            };
        }
    }

    // Try longest-prefix
    let effective_max = std::cmp::min(config.max_prefix_length, total_input_tokens);
    let search_tokens = if effective_max < total_input_tokens {
        &input_tokens[..effective_max as usize]
    } else {
        input_tokens
    };

    match cache.find_longest_prefix_match(
        search_tokens,
        context_digest,
        tokenizer_hash,
        model_identity_hash,
        config.min_match_tokens,
    ) {
        Some(prefix_match) => {
            if entry_metadata_valid(
                &prefix_match.entry,
                context_digest,
                tokenizer_hash,
                model_identity_hash,
            ) {
                let handle = CacheEntryHandle::from_prefix_match(&prefix_match);
                CacheLookupWithTensors {
                    result: CacheLookupResult::hit(
                        prefix_match.cache_key,
                        prefix_match.matched_token_count,
                        total_input_tokens,
                        prefix_match.entry.kv_bytes,
                        prefix_match.is_full_match(),
                    ),
                    entry_handle: Some(handle),
                }
            } else {
                cache.remove(&prefix_match.cache_key);
                CacheLookupWithTensors {
                    result: CacheLookupResult::miss(
                        total_input_tokens,
                        CacheMissReason::IntegrityValidationFailed,
                    ),
                    entry_handle: None,
                }
            }
        }
        None => {
            let reason = if effective_max < total_input_tokens {
                CacheMissReason::MaxPrefixLengthReached
            } else {
                CacheMissReason::NoMatchingPrefix
            };
            CacheLookupWithTensors {
                result: CacheLookupResult::miss(total_input_tokens, reason),
                entry_handle: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::prefix_kv_key::compute_prefix_kv_key;

    fn make_test_entry(
        prefix_tokens: Vec<u32>,
        layers: usize,
        size_per_layer: usize,
        context_digest: B3Hash,
        tokenizer_hash: B3Hash,
        model_identity_hash: B3Hash,
    ) -> PrefixKvEntry {
        let keys: Vec<Vec<f32>> = (0..layers).map(|_| vec![1.0; size_per_layer]).collect();
        let values: Vec<Vec<f32>> = (0..layers).map(|_| vec![2.0; size_per_layer]).collect();
        PrefixKvEntry::new_with_tokens(
            keys,
            values,
            "test_tenant".to_string(),
            prefix_tokens,
            context_digest,
            tokenizer_hash,
            model_identity_hash,
        )
    }

    #[test]
    fn test_cache_lookup_miss_empty() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();

        let result = cache_prefix_lookup(
            &cache,
            &[],
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );

        assert!(!result.cache_hit);
        assert_eq!(result.miss_reason, Some(CacheMissReason::EmptyInput));
        assert_eq!(result.billed_input_tokens(), 0);
    }

    #[test]
    fn test_cache_lookup_miss_no_entry() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();

        let result = cache_prefix_lookup(
            &cache,
            &[1, 2, 3],
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );

        assert!(!result.cache_hit);
        assert_eq!(result.miss_reason, Some(CacheMissReason::NoMatchingPrefix));
        assert_eq!(result.total_input_tokens, 3);
        assert_eq!(result.billed_input_tokens(), 3);
    }

    #[test]
    fn test_cache_lookup_hit_exact() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();

        let tokens = vec![1, 2, 3, 4, 5];

        // Insert entry with PRD-compliant key
        let exact_key = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity_bytes);
        let entry = make_test_entry(tokens.clone(), 2, 128, context, tokenizer, model);
        cache.insert(exact_key, entry).unwrap();

        let result = cache_prefix_lookup(
            &cache,
            &tokens,
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );

        assert!(result.cache_hit);
        assert!(result.is_exact_match);
        assert_eq!(result.cached_token_count, 5);
        assert_eq!(result.billed_input_tokens(), 0);
        assert_eq!(result.cache_id, Some(exact_key));
    }

    #[test]
    fn test_cache_lookup_hit_partial() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();

        // Insert shorter prefix
        let cached_tokens = vec![1, 2, 3];
        let cached_key =
            compute_prefix_kv_key(&context, &cached_tokens, &tokenizer, &identity_bytes);
        let entry = make_test_entry(cached_tokens, 2, 128, context, tokenizer, model);
        cache.insert(cached_key, entry).unwrap();

        // Lookup longer sequence sharing prefix
        let input_tokens = vec![1, 2, 3, 4, 5];
        let result = cache_prefix_lookup(
            &cache,
            &input_tokens,
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );

        assert!(result.cache_hit);
        assert!(!result.is_exact_match);
        assert_eq!(result.cached_token_count, 3);
        assert_eq!(result.billed_input_tokens(), 2);
        assert_eq!(result.total_input_tokens, 5);
    }

    #[test]
    fn test_receipt_field_mapping() {
        // Verify CacheLookupResult fields map correctly to RunReceipt
        let result = CacheLookupResult::hit(B3Hash::hash(b"test"), 100, 150, 1024, false);

        // These should map to receipt fields:
        assert!(result.cache_hit); // → prefix_cache_hit
        assert_eq!(result.cache_id.unwrap().as_bytes().len(), 32); // → prefix_kv_key_b3
        assert_eq!(result.cached_token_count, 100); // → prefix_cached_token_count
        assert_eq!(result.cached_kv_bytes, 1024); // → prefix_kv_bytes
        assert_eq!(result.billed_input_tokens(), 50); // → billed_input_tokens
    }

    #[test]
    fn test_stop_conditions_partition() {
        // Verify stop conditions form complete partition
        let reasons = [
            CacheMissReason::EmptyInput,
            CacheMissReason::NoMatchingPrefix,
            CacheMissReason::IntegrityValidationFailed,
            CacheMissReason::MaxPrefixLengthReached,
        ];

        // Each has distinct Display output
        let displays: Vec<_> = reasons.iter().map(|r| r.to_string()).collect();
        for i in 0..displays.len() {
            for j in (i + 1)..displays.len() {
                assert_ne!(displays[i], displays[j]);
            }
        }
    }

    #[test]
    fn test_entry_handle_receipt_fields() {
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");

        let entry = Arc::new(make_test_entry(
            vec![1, 2, 3],
            2,
            128,
            context,
            tokenizer,
            model,
        ));
        let cache_key = B3Hash::hash(b"cache_key");

        let handle = CacheEntryHandle::from_exact_match(entry, cache_key);

        // Receipt-relevant accessors
        assert_eq!(handle.cache_key(), &cache_key);
        assert_eq!(handle.matched_token_count(), 3);
        assert!(handle.kv_bytes() > 0);

        // Tensor loading (NOT receipt-bound)
        let (keys, values, count) = handle.load_kv_tensors();
        assert_eq!(keys.len(), 2);
        assert_eq!(values.len(), 2);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_determinism_identical_inputs() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();
        let tokens = vec![1, 2, 3, 4, 5];

        // Insert entry
        let key = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity_bytes);
        let entry = make_test_entry(tokens.clone(), 2, 128, context, tokenizer, model);
        cache.insert(key, entry).unwrap();

        // Two lookups with identical inputs
        let result1 = cache_prefix_lookup(
            &cache,
            &tokens,
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );
        let result2 = cache_prefix_lookup(
            &cache,
            &tokens,
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &CacheLookupConfig::default(),
        );

        // Must be identical
        assert_eq!(result1.cache_hit, result2.cache_hit);
        assert_eq!(result1.cache_id, result2.cache_id);
        assert_eq!(result1.cached_token_count, result2.cached_token_count);
        assert_eq!(result1.cached_kv_bytes, result2.cached_kv_bytes);
    }

    // =========================================================================
    // P0-1: Cache Attestation Tests
    // =========================================================================

    #[test]
    fn test_hit_with_attestation() {
        let signing_key = [0x42u8; 32];
        let cache_id = B3Hash::hash(b"cache_key");

        let result = CacheLookupResult::hit_with_attestation(
            cache_id,
            100,
            150,
            1024,
            true,
            "worker-001",
            42,
            &signing_key,
        )
        .expect("should create attested result");

        assert!(result.cache_hit);
        assert!(result.attestation.is_some());

        let attestation = result.attestation.unwrap();
        assert_eq!(attestation.token_count, 100);
        assert_eq!(attestation.worker_id, "worker-001");
        assert_eq!(attestation.timestamp_tick, 42);
        assert_eq!(attestation.cache_key_hash, *cache_id.as_bytes());
    }

    #[test]
    fn test_attested_lookup_hit() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();
        let signing_key = [0x42u8; 32];

        let tokens = vec![1, 2, 3, 4, 5];

        // Insert entry
        let exact_key = compute_prefix_kv_key(&context, &tokens, &tokenizer, &identity_bytes);
        let entry = make_test_entry(tokens.clone(), 2, 128, context, tokenizer, model);
        cache.insert(exact_key, entry).unwrap();

        let config = AttestedLookupConfig {
            lookup: CacheLookupConfig::default(),
            worker_id: "worker-test".to_string(),
            signing_key,
        };

        let result = cache_prefix_lookup_attested(
            &cache,
            &tokens,
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &config,
            999,
        )
        .expect("should succeed");

        assert!(result.cache_hit);
        assert!(result.attestation.is_some());

        let attestation = result.attestation.unwrap();
        assert_eq!(attestation.worker_id, "worker-test");
        assert_eq!(attestation.timestamp_tick, 999);
        assert_eq!(attestation.token_count, 5);
    }

    #[test]
    fn test_attested_lookup_miss_no_attestation() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();
        let signing_key = [0x42u8; 32];

        let config = AttestedLookupConfig {
            lookup: CacheLookupConfig::default(),
            worker_id: "worker-test".to_string(),
            signing_key,
        };

        // Empty cache, should miss
        let result = cache_prefix_lookup_attested(
            &cache,
            &[1, 2, 3],
            &context,
            &tokenizer,
            &model,
            &identity_bytes,
            &config,
            100,
        )
        .expect("should succeed");

        assert!(!result.cache_hit);
        assert!(result.attestation.is_none()); // No attestation on miss
    }

    #[test]
    fn test_attestation_verifiable() {
        use ed25519_dalek::SigningKey;

        let signing_key_bytes = [0x42u8; 32];
        let signing_key = SigningKey::from_bytes(&signing_key_bytes);
        let public_key = signing_key.verifying_key().to_bytes();

        let cache_id = B3Hash::hash(b"cache_key");
        let result = CacheLookupResult::hit_with_attestation(
            cache_id,
            100,
            150,
            1024,
            true,
            "worker-001",
            42,
            &signing_key_bytes,
        )
        .expect("should create attested result");

        let attestation = result.attestation.unwrap();

        // Verify with correct public key should succeed
        assert!(attestation.verify(&public_key).is_ok());

        // Verify with wrong public key should fail
        let wrong_key = [0x99u8; 32];
        assert!(attestation.verify(&wrong_key).is_err());
    }

    /// Test that cache prefix lookup is deterministic across multiple runs.
    ///
    /// This verifies that the BTreeMap-based cache produces identical results
    /// for identical operation sequences.
    #[test]
    fn test_cache_prefix_lookup_determinism() {
        let context = B3Hash::hash(b"context");
        let tokenizer = B3Hash::hash(b"tokenizer");
        let model = B3Hash::hash(b"model");
        let identity_bytes = b"identity".to_vec();

        /// Run a sequence of cache operations and return the results
        fn run_sequence(
            context: &B3Hash,
            tokenizer: &B3Hash,
            model: &B3Hash,
            identity_bytes: &[u8],
        ) -> Vec<(Option<B3Hash>, u32, bool)> {
            let cache = PrefixKvCache::new(1024 * 1024);
            let config = CacheLookupConfig::default();

            // Insert multiple entries with different prefixes
            let tokens1 = vec![1, 2, 3];
            let tokens2 = vec![1, 2, 3, 4, 5];
            let tokens3 = vec![10, 20, 30];

            let key1 = compute_prefix_kv_key(context, &tokens1, tokenizer, identity_bytes);
            let key2 = compute_prefix_kv_key(context, &tokens2, tokenizer, identity_bytes);
            let key3 = compute_prefix_kv_key(context, &tokens3, tokenizer, identity_bytes);

            cache
                .insert(
                    key1,
                    make_test_entry(tokens1.clone(), 2, 64, *context, *tokenizer, *model),
                )
                .unwrap();
            cache
                .insert(
                    key2,
                    make_test_entry(tokens2.clone(), 2, 64, *context, *tokenizer, *model),
                )
                .unwrap();
            cache
                .insert(
                    key3,
                    make_test_entry(tokens3.clone(), 2, 64, *context, *tokenizer, *model),
                )
                .unwrap();

            // Perform a series of lookups
            let queries = vec![
                vec![1, 2, 3, 4, 5, 6, 7], // Should match tokens2 (partial)
                vec![1, 2, 3],             // Should match tokens1 (exact)
                vec![10, 20, 30, 40],      // Should match tokens3 (partial)
                vec![99, 98, 97],          // Should miss
                vec![1, 2, 3, 4, 5],       // Should match tokens2 (exact)
            ];

            queries
                .into_iter()
                .map(|q| {
                    let result = cache_prefix_lookup(
                        &cache,
                        &q,
                        context,
                        tokenizer,
                        model,
                        identity_bytes,
                        &config,
                    );
                    (result.cache_id, result.cached_token_count, result.cache_hit)
                })
                .collect()
        }

        // Run the same sequence multiple times
        let results1 = run_sequence(&context, &tokenizer, &model, &identity_bytes);
        let results2 = run_sequence(&context, &tokenizer, &model, &identity_bytes);
        let results3 = run_sequence(&context, &tokenizer, &model, &identity_bytes);

        // All runs should produce identical results
        assert_eq!(
            results1, results2,
            "Cache lookup results should be deterministic (run 1 vs 2)"
        );
        assert_eq!(
            results2, results3,
            "Cache lookup results should be deterministic (run 2 vs 3)"
        );

        // Verify expected behavior
        assert!(results1[0].2, "First query should hit (partial match)");
        assert!(results1[1].2, "Second query should hit (exact match)");
        assert!(results1[2].2, "Third query should hit (partial match)");
        assert!(!results1[3].2, "Fourth query should miss");
        assert!(results1[4].2, "Fifth query should hit (exact match)");
    }
}

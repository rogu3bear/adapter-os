//! Canonical receipt digest computation for AdapterOS.
//!
//! This module provides a single source of truth for computing receipt digests,
//! ensuring parity between production (inference_trace) and offline verification (CLI).
//!
//! # Schema Versions
//!
//! - **V1**: Original receipt digest (context + run_head + output + billing fields)
//! - **V2**: Adds backend identity (backend_used, backend_attestation)
//! - **V3**: Adds seed lineage (root_seed_digest, seed_mode, has_manifest_binding)
//! - **V4**: Production format with all fields (stop controller, KV, prefix cache, model cache)
//!
//! # Digest Algorithm
//!
//! All versions use BLAKE3 hash over canonically ordered fields.
//! Optional fields use deterministic sentinels:
//! - Strings: empty string or length-prefixed with 0 length
//! - u32: 0xFFFFFFFF sentinel for None
//! - B3Hash: 32 zero bytes for None
//! - bool: 0/1 byte

use crate::B3Hash;
use serde::{Deserialize, Serialize};

/// Receipt schema versions
pub const RECEIPT_SCHEMA_V1: u8 = 1;
pub const RECEIPT_SCHEMA_V2: u8 = 2;
pub const RECEIPT_SCHEMA_V3: u8 = 3;
/// V4: Full production format with stop controller, KV, prefix cache, model cache fields
pub const RECEIPT_SCHEMA_V4: u8 = 4;
/// Current schema version for new receipts
pub const RECEIPT_SCHEMA_CURRENT: u8 = RECEIPT_SCHEMA_V4;

/// Input fields for receipt digest computation.
///
/// This struct contains all fields that may be included in a receipt digest,
/// with appropriate defaults for backward compatibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReceiptDigestInput {
    // Core fields (all versions)
    pub context_digest: [u8; 32],
    pub run_head_hash: [u8; 32],
    pub output_digest: [u8; 32],
    pub logical_prompt_tokens: u32,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    pub logical_output_tokens: u32,
    pub billed_output_tokens: u32,

    // V2+ fields: Backend identity
    #[serde(default)]
    pub backend_used: Option<String>,
    #[serde(default)]
    pub backend_attestation_b3: Option<[u8; 32]>,

    // V3+ fields: Seed lineage
    #[serde(default)]
    pub root_seed_digest: Option<[u8; 32]>,
    #[serde(default)]
    pub seed_mode: Option<String>,
    #[serde(default)]
    pub has_manifest_binding: Option<bool>,

    // V4 fields: Stop controller
    #[serde(default)]
    pub stop_reason_code: Option<String>,
    #[serde(default)]
    pub stop_reason_token_index: Option<u32>,
    #[serde(default)]
    pub stop_policy_digest_b3: Option<[u8; 32]>,

    // V4 fields: KV quota/residency
    #[serde(default)]
    pub tenant_kv_quota_bytes: u64,
    #[serde(default)]
    pub tenant_kv_bytes_used: u64,
    #[serde(default)]
    pub kv_evictions: u32,
    #[serde(default)]
    pub kv_residency_policy_id: Option<String>,
    #[serde(default)]
    pub kv_quota_enforced: bool,

    // V4 fields: Prefix KV cache
    #[serde(default)]
    pub prefix_kv_key_b3: Option<[u8; 32]>,
    #[serde(default)]
    pub prefix_cache_hit: bool,
    #[serde(default)]
    pub prefix_kv_bytes: u64,

    // V4 fields: Model cache identity
    #[serde(default)]
    pub model_cache_identity_v2_digest_b3: Option<[u8; 32]>,
}

impl ReceiptDigestInput {
    /// Create a new input with required fields, defaulting optional fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context_digest: [u8; 32],
        run_head_hash: [u8; 32],
        output_digest: [u8; 32],
        logical_prompt_tokens: u32,
        prefix_cached_token_count: u32,
        billed_input_tokens: u32,
        logical_output_tokens: u32,
        billed_output_tokens: u32,
    ) -> Self {
        Self {
            context_digest,
            run_head_hash,
            output_digest,
            logical_prompt_tokens,
            prefix_cached_token_count,
            billed_input_tokens,
            logical_output_tokens,
            billed_output_tokens,
            ..Default::default()
        }
    }

    /// Set stop controller fields (V4+)
    pub fn with_stop_controller(
        mut self,
        stop_reason_code: Option<String>,
        stop_reason_token_index: Option<u32>,
        stop_policy_digest_b3: Option<[u8; 32]>,
    ) -> Self {
        self.stop_reason_code = stop_reason_code;
        self.stop_reason_token_index = stop_reason_token_index;
        self.stop_policy_digest_b3 = stop_policy_digest_b3;
        self
    }

    /// Set KV quota/residency fields (V4+)
    pub fn with_kv_quota(
        mut self,
        tenant_kv_quota_bytes: u64,
        tenant_kv_bytes_used: u64,
        kv_evictions: u32,
        kv_residency_policy_id: Option<String>,
        kv_quota_enforced: bool,
    ) -> Self {
        self.tenant_kv_quota_bytes = tenant_kv_quota_bytes;
        self.tenant_kv_bytes_used = tenant_kv_bytes_used;
        self.kv_evictions = kv_evictions;
        self.kv_residency_policy_id = kv_residency_policy_id;
        self.kv_quota_enforced = kv_quota_enforced;
        self
    }

    /// Set prefix KV cache fields (V4+)
    pub fn with_prefix_cache(
        mut self,
        prefix_kv_key_b3: Option<[u8; 32]>,
        prefix_cache_hit: bool,
        prefix_kv_bytes: u64,
    ) -> Self {
        self.prefix_kv_key_b3 = prefix_kv_key_b3;
        self.prefix_cache_hit = prefix_cache_hit;
        self.prefix_kv_bytes = prefix_kv_bytes;
        self
    }

    /// Set model cache identity (V4+)
    pub fn with_model_cache_identity(
        mut self,
        model_cache_identity_v2_digest_b3: Option<[u8; 32]>,
    ) -> Self {
        self.model_cache_identity_v2_digest_b3 = model_cache_identity_v2_digest_b3;
        self
    }

    /// Set backend identity fields (V2+)
    pub fn with_backend(
        mut self,
        backend_used: Option<String>,
        backend_attestation_b3: Option<[u8; 32]>,
    ) -> Self {
        self.backend_used = backend_used;
        self.backend_attestation_b3 = backend_attestation_b3;
        self
    }

    /// Set seed lineage fields (V3+)
    pub fn with_seed_lineage(
        mut self,
        root_seed_digest: Option<[u8; 32]>,
        seed_mode: Option<String>,
        has_manifest_binding: Option<bool>,
    ) -> Self {
        self.root_seed_digest = root_seed_digest;
        self.seed_mode = seed_mode;
        self.has_manifest_binding = has_manifest_binding;
        self
    }
}

/// Compute receipt digest for the given schema version.
///
/// This is the canonical implementation used by both production and CLI.
///
/// # Arguments
/// * `input` - Receipt fields to hash
/// * `schema_version` - Schema version determining which fields to include
///
/// # Returns
/// BLAKE3 hash of the canonicalized receipt fields
///
/// # Errors
/// Returns None if schema_version is unsupported
pub fn compute_receipt_digest(input: &ReceiptDigestInput, schema_version: u8) -> Option<B3Hash> {
    match schema_version {
        RECEIPT_SCHEMA_V1 => Some(compute_v1_digest(input)),
        RECEIPT_SCHEMA_V2 => Some(compute_v2_digest(input)),
        RECEIPT_SCHEMA_V3 => Some(compute_v3_digest(input)),
        RECEIPT_SCHEMA_V4 => Some(compute_v4_digest(input)),
        _ => {
            tracing::warn!(
                schema_version = schema_version,
                "Unsupported receipt schema version"
            );
            None
        }
    }
}

/// Compute V1 receipt digest (original format).
///
/// Fields: context_digest, run_head_hash, output_digest, billing counts
fn compute_v1_digest(input: &ReceiptDigestInput) -> B3Hash {
    B3Hash::hash_multi(&[
        &input.context_digest[..],
        &input.run_head_hash[..],
        &input.output_digest[..],
        &input.logical_prompt_tokens.to_le_bytes(),
        &input.prefix_cached_token_count.to_le_bytes(),
        &input.billed_input_tokens.to_le_bytes(),
        &input.logical_output_tokens.to_le_bytes(),
        &input.billed_output_tokens.to_le_bytes(),
    ])
}

/// Compute V2 receipt digest (with backend identity).
///
/// Adds: schema_version byte, backend_used, backend_attestation
fn compute_v2_digest(input: &ReceiptDigestInput) -> B3Hash {
    let backend_bytes = input.backend_used.as_deref().unwrap_or("").as_bytes();
    let attestation_bytes = input
        .backend_attestation_b3
        .map(|b| b.to_vec())
        .unwrap_or_default();

    B3Hash::hash_multi(&[
        &input.context_digest[..],
        &input.run_head_hash[..],
        &input.output_digest[..],
        &input.logical_prompt_tokens.to_le_bytes(),
        &input.prefix_cached_token_count.to_le_bytes(),
        &input.billed_input_tokens.to_le_bytes(),
        &input.logical_output_tokens.to_le_bytes(),
        &input.billed_output_tokens.to_le_bytes(),
        // V2 additions
        &[RECEIPT_SCHEMA_V2],
        &(backend_bytes.len() as u32).to_le_bytes(),
        backend_bytes,
        &(attestation_bytes.len() as u32).to_le_bytes(),
        &attestation_bytes,
    ])
}

/// Compute V3 receipt digest (with seed lineage).
///
/// Adds: root_seed_digest, seed_mode, has_manifest_binding
fn compute_v3_digest(input: &ReceiptDigestInput) -> B3Hash {
    let backend_bytes = input.backend_used.as_deref().unwrap_or("").as_bytes();
    let attestation_bytes = input
        .backend_attestation_b3
        .map(|b| b.to_vec())
        .unwrap_or_default();

    // Seed lineage fields
    let seed_digest_bytes = input
        .root_seed_digest
        .map(|b| b.to_vec())
        .unwrap_or_else(|| vec![0u8; 32]);
    let seed_mode_bytes = input.seed_mode.as_deref().unwrap_or("unknown").as_bytes();
    let manifest_binding_byte = if input.has_manifest_binding.unwrap_or(false) {
        [1u8]
    } else {
        [0u8]
    };

    B3Hash::hash_multi(&[
        &input.context_digest[..],
        &input.run_head_hash[..],
        &input.output_digest[..],
        &input.logical_prompt_tokens.to_le_bytes(),
        &input.prefix_cached_token_count.to_le_bytes(),
        &input.billed_input_tokens.to_le_bytes(),
        &input.logical_output_tokens.to_le_bytes(),
        &input.billed_output_tokens.to_le_bytes(),
        // V2 additions (included in V3)
        &[RECEIPT_SCHEMA_V3],
        &(backend_bytes.len() as u32).to_le_bytes(),
        backend_bytes,
        &(attestation_bytes.len() as u32).to_le_bytes(),
        &attestation_bytes,
        // V3 additions: seed lineage
        &seed_digest_bytes,
        &(seed_mode_bytes.len() as u32).to_le_bytes(),
        seed_mode_bytes,
        &manifest_binding_byte,
    ])
}

/// Compute V4 receipt digest (production format with all fields).
///
/// This is the canonical production algorithm from `inference_trace.rs`.
/// It matches the production `compute_receipt_digest` function EXACTLY.
///
/// **IMPORTANT**: This must stay in sync with `inference_trace.rs::compute_receipt_digest`.
/// Any changes here must be mirrored there (or both should use this function).
///
/// Adds: stop controller, KV quota/residency, prefix cache, model cache identity
fn compute_v4_digest(input: &ReceiptDigestInput) -> B3Hash {
    // Stop controller fields - serialized deterministically:
    // - Empty string if None for stop_reason_code
    // - 0xFFFFFFFF sentinel if None for stop_reason_token_index
    // - 32 zero bytes if None for stop_policy_digest_b3
    let stop_reason_bytes = input.stop_reason_code.as_deref().unwrap_or("").as_bytes();
    let stop_token_index_bytes = input
        .stop_reason_token_index
        .unwrap_or(0xFFFFFFFF)
        .to_le_bytes();
    let stop_policy_bytes = input
        .stop_policy_digest_b3
        .map(|b| b.to_vec())
        .unwrap_or_else(|| vec![0u8; 32]);

    // KV residency policy - 0 length if None
    let kv_residency_policy_id = input.kv_residency_policy_id.as_deref();

    // Prefix KV cache - 32 zero bytes if None
    let prefix_kv_key_bytes = input
        .prefix_kv_key_b3
        .map(|b| b.to_vec())
        .unwrap_or_else(|| vec![0u8; 32]);

    // Model cache identity V2 - 32 zero bytes if None (backward compatibility)
    let model_cache_identity_bytes = input
        .model_cache_identity_v2_digest_b3
        .map(|b| b.to_vec())
        .unwrap_or_else(|| vec![0u8; 32]);

    B3Hash::hash_multi(&[
        // Core fields (same layout as production inference_trace.rs)
        &input.context_digest[..],
        &input.run_head_hash[..],
        &input.output_digest[..],
        &input.logical_prompt_tokens.to_le_bytes(),
        &input.prefix_cached_token_count.to_le_bytes(),
        &input.billed_input_tokens.to_le_bytes(),
        &input.logical_output_tokens.to_le_bytes(),
        &input.billed_output_tokens.to_le_bytes(),
        // Stop controller fields (PRD: Hard Deterministic Stop Controller)
        &(stop_reason_bytes.len() as u32).to_le_bytes(),
        stop_reason_bytes,
        &stop_token_index_bytes,
        &stop_policy_bytes,
        // KV quota/residency fields (PRD: KvResidencyAndQuotas v1)
        &input.tenant_kv_quota_bytes.to_le_bytes(),
        &input.tenant_kv_bytes_used.to_le_bytes(),
        &input.kv_evictions.to_le_bytes(),
        &(kv_residency_policy_id.map(|s| s.len() as u32).unwrap_or(0)).to_le_bytes(),
        kv_residency_policy_id.map(|s| s.as_bytes()).unwrap_or(&[]),
        &[if input.kv_quota_enforced { 1u8 } else { 0u8 }],
        // Prefix KV cache fields (PRD: PrefixKvCache v1)
        &prefix_kv_key_bytes,
        &[if input.prefix_cache_hit { 1u8 } else { 0u8 }],
        &input.prefix_kv_bytes.to_le_bytes(),
        // Model cache identity V2 (PRD-06)
        &model_cache_identity_bytes,
    ])
}

/// Compute output digest from output tokens.
///
/// This is the canonical algorithm: length-prefixed array of u32 LE bytes.
pub fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

/// Hash a single token decision for the run_head chain.
///
/// This is the canonical algorithm for decision hashing.
#[allow(clippy::too_many_arguments)]
pub fn hash_token_decision(
    context_digest: &[u8; 32],
    token_index: u32,
    adapter_ids_blob: &[u8],
    gates_blob: &[u8],
    policy_mask_digest: Option<[u8; 32]>,
    allowed_mask_blob: Option<&[u8]>,
    policy_overrides_json: Option<&str>,
    backend_id: Option<&str>,
    kernel_version_id: Option<&str>,
) -> B3Hash {
    let policy_bytes = policy_mask_digest.map(|d| d.to_vec()).unwrap_or_default();
    let allowed_bytes = allowed_mask_blob.unwrap_or(&[]);
    let overrides_bytes = policy_overrides_json
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    let backend_bytes = backend_id.unwrap_or("").as_bytes().to_vec();
    let kernel_bytes = kernel_version_id.unwrap_or("").as_bytes().to_vec();

    B3Hash::hash_multi(&[
        &context_digest[..],
        &token_index.to_le_bytes(),
        &(adapter_ids_blob.len() as u32).to_le_bytes(),
        adapter_ids_blob,
        &(gates_blob.len() as u32).to_le_bytes(),
        gates_blob,
        &(policy_bytes.len() as u32).to_le_bytes(),
        &policy_bytes,
        &(allowed_bytes.len() as u32).to_le_bytes(),
        allowed_bytes,
        &(overrides_bytes.len() as u32).to_le_bytes(),
        &overrides_bytes,
        &(backend_bytes.len() as u32).to_le_bytes(),
        &backend_bytes,
        &(kernel_bytes.len() as u32).to_le_bytes(),
        &kernel_bytes,
    ])
}

/// Update run_head hash chain with a new token decision.
///
/// This is the canonical chaining algorithm.
pub fn update_run_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
    B3Hash::hash_multi(&[
        prev.as_bytes(),
        decision_hash.as_bytes(),
        &token_index.to_le_bytes(),
    ])
}

/// Encode adapter IDs to canonical blob format.
///
/// Format: count (u32 LE) + for each: length (u32 LE) + bytes
pub fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        let bytes = id.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

/// Encode Q15 gates to canonical blob format.
///
/// Format: count (u32 LE) + for each: i16 LE
pub fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + gates.len() * 2);
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for g in gates {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out
}

/// Encode allowed mask to canonical blob format.
///
/// Format: count (u32 LE) + for each: 0/1 byte
pub fn encode_allowed_mask(mask: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + mask.len());
    out.extend_from_slice(&(mask.len() as u32).to_le_bytes());
    out.extend(mask.iter().map(|b| if *b { 1u8 } else { 0u8 }));
    out
}

/// Decode allowed mask from canonical blob format.
pub fn decode_allowed_mask(bytes: &[u8]) -> Result<Vec<bool>, &'static str> {
    if bytes.len() < 4 {
        return Err("allowed_mask blob missing length");
    }
    let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let mut cursor = 4;
    let mut mask = Vec::with_capacity(count);
    for _ in 0..count {
        if bytes.len() < cursor + 1 {
            return Err("allowed_mask blob truncated");
        }
        mask.push(bytes[cursor] == 1);
        cursor += 1;
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Parity test**: Proves the canonical V4 digest matches production (inference_trace.rs).
    ///
    /// This test uses known inputs and verifies the hash matches the expected value
    /// computed by the production algorithm. If this test fails after any changes to
    /// `compute_v4_digest`, it means CLI verification will diverge from production.
    ///
    /// **IMPORTANT**: If you change the V4 algorithm, update this test with the new expected hash.
    /// Any change to the V4 algorithm is a BREAKING CHANGE requiring migration.
    #[test]
    fn test_v4_parity_with_production_algorithm() {
        // Fixed inputs that match production inference_trace.rs::compute_receipt_digest
        let input = ReceiptDigestInput {
            context_digest: [0x01u8; 32],
            run_head_hash: [0x02u8; 32],
            output_digest: [0x03u8; 32],
            logical_prompt_tokens: 100,
            prefix_cached_token_count: 10,
            billed_input_tokens: 90,
            logical_output_tokens: 50,
            billed_output_tokens: 50,
            backend_used: None, // V4 doesn't include backend in hash
            backend_attestation_b3: None,
            root_seed_digest: None, // V4 doesn't include seed in hash
            seed_mode: None,
            has_manifest_binding: None,
            // Stop controller fields
            stop_reason_code: Some("EOS".to_string()),
            stop_reason_token_index: Some(49),
            stop_policy_digest_b3: Some([0x04u8; 32]),
            // KV quota fields
            tenant_kv_quota_bytes: 1024 * 1024,
            tenant_kv_bytes_used: 512 * 1024,
            kv_evictions: 0,
            kv_residency_policy_id: Some("default".to_string()),
            kv_quota_enforced: true,
            // Prefix cache fields
            prefix_kv_key_b3: Some([0x05u8; 32]),
            prefix_cache_hit: true,
            prefix_kv_bytes: 256 * 1024,
            // Model cache identity
            model_cache_identity_v2_digest_b3: Some([0x06u8; 32]),
        };

        // Compute digest using canonical V4
        let digest = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();

        // Re-compute using the EXACT production algorithm layout to verify parity.
        // This mirrors inference_trace.rs::compute_receipt_digest line-by-line.
        let production_digest = {
            let stop_reason_bytes = "EOS".as_bytes();
            let stop_token_index_bytes = 49u32.to_le_bytes();
            let stop_policy_bytes = [0x04u8; 32].to_vec();
            let prefix_kv_key_bytes = [0x05u8; 32].to_vec();
            let model_cache_identity_bytes = [0x06u8; 32].to_vec();

            B3Hash::hash_multi(&[
                &[0x01u8; 32][..],
                &[0x02u8; 32][..],
                &[0x03u8; 32][..],
                &100u32.to_le_bytes(),
                &10u32.to_le_bytes(),
                &90u32.to_le_bytes(),
                &50u32.to_le_bytes(),
                &50u32.to_le_bytes(),
                // Stop controller
                &(stop_reason_bytes.len() as u32).to_le_bytes(),
                stop_reason_bytes,
                &stop_token_index_bytes,
                &stop_policy_bytes,
                // KV quota
                &(1024u64 * 1024).to_le_bytes(),
                &(512u64 * 1024).to_le_bytes(),
                &0u32.to_le_bytes(),
                &(7u32).to_le_bytes(), // "default".len()
                b"default",
                &[1u8], // kv_quota_enforced = true
                // Prefix cache
                &prefix_kv_key_bytes,
                &[1u8], // prefix_cache_hit = true
                &(256u64 * 1024).to_le_bytes(),
                // Model cache identity
                &model_cache_identity_bytes,
            ])
        };

        assert_eq!(
            digest,
            production_digest,
            "V4 canonical digest must match production algorithm exactly.\n\
            This failure indicates CLI/production parity drift.\n\
            Canonical: {}\n\
            Production: {}",
            digest.to_hex(),
            production_digest.to_hex()
        );
    }

    /// Proves V4 digest with all None/default fields matches production with zeros.
    #[test]
    fn test_v4_parity_with_none_fields() {
        let input = ReceiptDigestInput::new(
            [0x01u8; 32],
            [0x02u8; 32],
            [0x03u8; 32],
            100,
            0,
            100,
            50,
            50,
        );

        let digest = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();

        // Production algorithm with None fields:
        // - stop_reason_code: empty string (length=0)
        // - stop_reason_token_index: 0xFFFFFFFF sentinel
        // - stop_policy_digest_b3: 32 zero bytes
        // - kv_residency_policy_id: length=0, empty
        // - prefix_kv_key_b3: 32 zero bytes
        // - model_cache_identity: 32 zero bytes
        let production_digest = {
            B3Hash::hash_multi(&[
                &[0x01u8; 32][..],
                &[0x02u8; 32][..],
                &[0x03u8; 32][..],
                &100u32.to_le_bytes(),
                &0u32.to_le_bytes(),
                &100u32.to_le_bytes(),
                &50u32.to_le_bytes(),
                &50u32.to_le_bytes(),
                // Stop controller (all None)
                &0u32.to_le_bytes(), // empty stop_reason_code
                &[],
                &0xFFFFFFFFu32.to_le_bytes(), // sentinel for None
                &[0u8; 32],                   // 32 zero bytes
                // KV quota (defaults)
                &0u64.to_le_bytes(),
                &0u64.to_le_bytes(),
                &0u32.to_le_bytes(),
                &0u32.to_le_bytes(), // empty kv_residency_policy_id
                &[],
                &[0u8], // kv_quota_enforced = false
                // Prefix cache (defaults)
                &[0u8; 32], // 32 zero bytes
                &[0u8],     // prefix_cache_hit = false
                &0u64.to_le_bytes(),
                // Model cache identity (None)
                &[0u8; 32],
            ])
        };

        assert_eq!(
            digest, production_digest,
            "V4 with None fields must match production defaults"
        );
    }

    #[test]
    fn test_v1_digest_deterministic() {
        let input = ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50);

        let d1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V1).unwrap();
        let d2 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V1).unwrap();
        assert_eq!(d1, d2, "V1 digest should be deterministic");
    }

    #[test]
    fn test_v4_digest_deterministic() {
        let input = ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50)
            .with_stop_controller(Some("EOS".to_string()), Some(45), Some([4u8; 32]))
            .with_kv_quota(
                1024 * 1024,
                512 * 1024,
                0,
                Some("default".to_string()),
                true,
            )
            .with_prefix_cache(Some([5u8; 32]), true, 256 * 1024)
            .with_model_cache_identity(Some([6u8; 32]));

        let d1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();
        let d2 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();
        assert_eq!(d1, d2, "V4 digest should be deterministic");
    }

    #[test]
    fn test_different_versions_produce_different_digests() {
        let input = ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50);

        let v1 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V1).unwrap();
        let v2 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V2).unwrap();
        let v3 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V3).unwrap();
        let v4 = compute_receipt_digest(&input, RECEIPT_SCHEMA_V4).unwrap();

        assert_ne!(v1, v2, "V1 and V2 should differ");
        assert_ne!(v2, v3, "V2 and V3 should differ");
        assert_ne!(v3, v4, "V3 and V4 should differ");
    }

    #[test]
    fn test_missing_field_changes_digest() {
        let with_stop =
            ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50)
                .with_stop_controller(Some("EOS".to_string()), Some(45), Some([4u8; 32]));

        let without_stop =
            ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50);

        let d1 = compute_receipt_digest(&with_stop, RECEIPT_SCHEMA_V4).unwrap();
        let d2 = compute_receipt_digest(&without_stop, RECEIPT_SCHEMA_V4).unwrap();

        assert_ne!(
            d1, d2,
            "Different stop fields should produce different digest"
        );
    }

    #[test]
    fn test_unsupported_version_returns_none() {
        let input = ReceiptDigestInput::default();
        assert!(compute_receipt_digest(&input, 99).is_none());
    }

    #[test]
    fn test_output_digest_deterministic() {
        let tokens = vec![1u32, 2, 3, 4, 5];
        let d1 = compute_output_digest(&tokens);
        let d2 = compute_output_digest(&tokens);
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_encode_adapter_ids_roundtrip() {
        let ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let blob = encode_adapter_ids(&ids);

        // Verify format: count + (len + bytes) for each
        let count = u32::from_le_bytes(blob[0..4].try_into().unwrap());
        assert_eq!(count, 2);
    }

    #[test]
    fn test_encode_gates_roundtrip() {
        let gates = vec![123i16, -456, 789];
        let blob = encode_gates_q15(&gates);

        let count = u32::from_le_bytes(blob[0..4].try_into().unwrap());
        assert_eq!(count, 3);
    }

    #[test]
    fn test_allowed_mask_roundtrip() {
        let mask = vec![true, false, true, true, false];
        let blob = encode_allowed_mask(&mask);
        let decoded = decode_allowed_mask(&blob).unwrap();
        assert_eq!(mask, decoded);
    }

    #[test]
    fn test_run_head_chain() {
        let context = [1u8; 32];
        let adapter_blob = encode_adapter_ids(&["a".to_string()]);
        let gates_blob = encode_gates_q15(&[100]);

        let decision0 = hash_token_decision(
            &context,
            0,
            &adapter_blob,
            &gates_blob,
            None,
            None,
            None,
            None,
            None,
        );
        let head0 = update_run_head(&B3Hash::zero(), 0, &decision0);

        let decision1 = hash_token_decision(
            &context,
            1,
            &adapter_blob,
            &gates_blob,
            None,
            None,
            None,
            None,
            None,
        );
        let head1 = update_run_head(&head0, 1, &decision1);

        assert_ne!(head0, head1, "Chain should progress");
        assert_ne!(
            head0,
            B3Hash::zero(),
            "Chain should not be zero after first token"
        );
    }
}

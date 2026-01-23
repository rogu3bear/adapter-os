//! Third-Party Receipt Verification
//!
//! Enables verification of inference receipts without system access.
//! A third party can verify that a claimed output was produced by the
//! system under claimed conditions using only the receipt and claimed values.
//!
//! # Trust Model
//!
//! This module provides **binding verification**: it proves that a receipt
//! cryptographically commits to specific input/output values. It answers:
//!
//! > "Does this receipt actually bind to these claimed tokens?"
//!
//! ## What IS Verified
//!
//! - **Output binding**: The receipt commits to the claimed output tokens
//! - **Context binding**: The receipt commits to the claimed input context
//!   (tenant, stack config, prompt tokens)
//! - **Token accounting**: Billing counts match the receipt
//! - **Receipt integrity**: All components hash correctly to the receipt_digest
//!
//! ## What is NOT Verified (Trust Assumptions)
//!
//! - **`run_head_hash`**: The hash chain over per-token routing decisions is taken
//!   on faith. Verifying this requires the full token decision trace, which only
//!   the system (or a full bundle) has. This is an intentional design boundary—
//!   the third party trusts the system recorded decisions honestly.
//!
//! - **Model behavior**: This doesn't prove the model "should have" produced this
//!   output—only that the system claims it did and the receipt is internally consistent.
//!
//! - **Signature validity**: Use [`EvidenceVerifier`] for signature verification.
//!   This module verifies digest computation, not cryptographic signatures.
//!
//! # Verification Hierarchy
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ Full Bundle Verification (verify_receipt.rs CLI)                │
//! │ - Has: Complete token decision trace                            │
//! │ - Verifies: Everything, including run_head_hash recomputation   │
//! │ - Use: Audit, forensics, debugging                              │
//! └─────────────────────────────────────────────────────────────────┘
//!                              ↑
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ Third-Party Verification (this module)                          │
//! │ - Has: Receipt + claimed raw input/output                       │
//! │ - Verifies: Receipt binds to claimed values                     │
//! │ - Trusts: run_head_hash is accurate                             │
//! │ - Use: External citation verification                           │
//! └─────────────────────────────────────────────────────────────────┘
//!                              ↑
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ Evidence Chain Verification (EvidenceVerifier)                  │
//! │ - Has: Signed evidence envelope                                 │
//! │ - Verifies: Signature, chain linking, schema                    │
//! │ - Trusts: All digest values in the envelope                     │
//! │ - Use: Tamper detection in stored records                       │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example: Citation Verification
//!
//! A publisher claims: "I asked the model X and it responded Y."
//! They provide the receipt as proof. A third party can verify:
//!
//! ```ignore
//! use adapteros_core::third_party_verification::{verify_receipt, ClaimedValues, ClaimedConfig};
//! use adapteros_core::B3Hash;
//!
//! // Publisher's claim
//! let prompt_tokens = vec![/* tokenized input */];
//! let output_tokens = vec![/* tokenized output */];
//! let receipt_digest = B3Hash::from_hex("...")?;
//!
//! // Third party verifies the receipt binds to these values
//! let claimed = ClaimedValues::new(
//!     output_tokens,
//!     run_head_hash,  // From receipt metadata
//!     ClaimedConfig {
//!         tenant_namespace: "tenant-1".to_string(),
//!         stack_hash: B3Hash::hash(b"stack-config"),
//!         prompt_tokens,
//!         ..Default::default()
//!     },
//! );
//!
//! let result = verify_receipt(&receipt_digest, &claimed, 1)?;
//! if result.verified {
//!     // The receipt genuinely commits to these input/output tokens
//! }
//! ```

use crate::evidence_envelope::InferenceReceiptRef;
use crate::receipt_digest::{
    compute_output_digest, compute_receipt_digest, ReceiptDigestInput, RECEIPT_SCHEMA_V5,
};
use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

// =============================================================================
// Claimed Values Types
// =============================================================================

/// Configuration claimed to have been used during inference.
///
/// These values are combined to compute the context digest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaimedConfig {
    /// Tenant namespace identifier
    pub tenant_namespace: String,

    /// Hash of the adapter stack configuration
    pub stack_hash: B3Hash,

    /// Prompt tokens (input tokens) for context digest computation
    pub prompt_tokens: Vec<u32>,

    // --- Optional fields for V4+ receipts ---
    /// Stop reason code (V4+)
    #[serde(default)]
    pub stop_reason_code: Option<String>,

    /// Token index where stop occurred (V4+)
    #[serde(default)]
    pub stop_reason_token_index: Option<u32>,

    /// BLAKE3 digest of stop policy specification (V4+)
    #[serde(default)]
    pub stop_policy_digest_b3: Option<[u8; 32]>,

    /// Tenant KV cache quota in bytes (V4+)
    #[serde(default)]
    pub tenant_kv_quota_bytes: u64,

    /// Tenant KV cache bytes used (V4+)
    #[serde(default)]
    pub tenant_kv_bytes_used: u64,

    /// Number of KV cache evictions (V4+)
    #[serde(default)]
    pub kv_evictions: u32,

    /// KV residency policy ID (V4+)
    #[serde(default)]
    pub kv_residency_policy_id: Option<String>,

    /// Whether KV quota was enforced (V4+)
    #[serde(default)]
    pub kv_quota_enforced: bool,

    /// Prefix KV cache key hash (V4+)
    #[serde(default)]
    pub prefix_kv_key_b3: Option<[u8; 32]>,

    /// Whether prefix cache hit occurred (V4+)
    #[serde(default)]
    pub prefix_cache_hit: bool,

    /// Prefix KV cache bytes (V4+)
    #[serde(default)]
    pub prefix_kv_bytes: u64,

    /// Model cache identity V2 digest (V4+)
    #[serde(default)]
    pub model_cache_identity_v2_digest_b3: Option<[u8; 32]>,

    // --- V5 fields: Equipment profile and citation binding ---
    /// Equipment profile digest (V5+)
    #[serde(default)]
    pub equipment_profile_digest_b3: Option<[u8; 32]>,

    /// Processor ID (V5+)
    #[serde(default)]
    pub processor_id: Option<String>,

    /// MLX version (V5+)
    #[serde(default)]
    pub mlx_version: Option<String>,

    /// ANE version (V5+)
    #[serde(default)]
    pub ane_version: Option<String>,

    /// Merkle root of citations (V5+)
    #[serde(default)]
    pub citations_merkle_root_b3: Option<[u8; 32]>,

    /// Citation count (V5+)
    #[serde(default)]
    pub citation_count: u32,
}

/// Values claimed by a third party for verification.
///
/// Contains all data needed to recompute and verify a receipt digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedValues {
    /// Output tokens produced by inference
    pub output_tokens: Vec<u32>,

    /// Run head hash from the inference trace
    /// (hash chain over per-token decisions)
    pub run_head_hash: B3Hash,

    /// Configuration used during inference
    pub config: ClaimedConfig,

    /// Token accounting fields
    #[serde(default)]
    pub logical_prompt_tokens: u32,

    #[serde(default)]
    pub prefix_cached_token_count: u32,

    #[serde(default)]
    pub billed_input_tokens: u32,

    #[serde(default)]
    pub logical_output_tokens: u32,

    #[serde(default)]
    pub billed_output_tokens: u32,
}

impl ClaimedValues {
    /// Create new claimed values with minimal required fields.
    pub fn new(
        output_tokens: Vec<u32>,
        run_head_hash: B3Hash,
        config: ClaimedConfig,
    ) -> Self {
        let logical_prompt_tokens = config.prompt_tokens.len() as u32;
        let logical_output_tokens = output_tokens.len() as u32;

        Self {
            output_tokens,
            run_head_hash,
            logical_prompt_tokens,
            prefix_cached_token_count: 0,
            billed_input_tokens: logical_prompt_tokens,
            logical_output_tokens,
            billed_output_tokens: logical_output_tokens,
            config,
        }
    }

    /// Set token accounting fields explicitly.
    pub fn with_token_accounting(
        mut self,
        logical_prompt_tokens: u32,
        prefix_cached_token_count: u32,
        billed_input_tokens: u32,
        logical_output_tokens: u32,
        billed_output_tokens: u32,
    ) -> Self {
        self.logical_prompt_tokens = logical_prompt_tokens;
        self.prefix_cached_token_count = prefix_cached_token_count;
        self.billed_input_tokens = billed_input_tokens;
        self.logical_output_tokens = logical_output_tokens;
        self.billed_output_tokens = billed_output_tokens;
        self
    }
}

// =============================================================================
// Verification Result
// =============================================================================

/// Reason for verification failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum MismatchReason {
    /// Context digest does not match
    ContextDigestMismatch {
        computed: String,
        expected: Option<String>,
    },

    /// Output digest does not match
    OutputDigestMismatch {
        computed: String,
        expected: Option<String>,
    },

    /// Receipt digest does not match (final verification failure)
    ReceiptDigestMismatch { computed: String, claimed: String },

    /// Schema version not supported
    UnsupportedSchemaVersion { version: u8 },

    /// Failed to parse claimed data
    ParseError { message: String },
}

impl std::fmt::Display for MismatchReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContextDigestMismatch { computed, expected } => {
                write!(
                    f,
                    "Context digest mismatch: computed {} vs expected {:?}",
                    computed, expected
                )
            }
            Self::OutputDigestMismatch { computed, expected } => {
                write!(
                    f,
                    "Output digest mismatch: computed {} vs expected {:?}",
                    computed, expected
                )
            }
            Self::ReceiptDigestMismatch { computed, claimed } => {
                write!(
                    f,
                    "Receipt digest mismatch: computed {} vs claimed {}",
                    computed, claimed
                )
            }
            Self::UnsupportedSchemaVersion { version } => {
                write!(f, "Unsupported schema version: {}", version)
            }
            Self::ParseError { message } => {
                write!(f, "Parse error: {}", message)
            }
        }
    }
}

impl std::error::Error for MismatchReason {}

/// Result of third-party receipt verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether verification succeeded
    pub verified: bool,

    /// Reason for failure (None if verified)
    pub mismatch_reason: Option<MismatchReason>,

    /// Computed context digest
    pub computed_context_digest: B3Hash,

    /// Computed output digest
    pub computed_output_digest: B3Hash,

    /// Computed receipt digest
    pub computed_receipt_digest: B3Hash,

    /// Schema version used for verification
    pub schema_version: u8,
}

impl VerificationResult {
    /// Create a successful verification result.
    fn success(
        context_digest: B3Hash,
        output_digest: B3Hash,
        receipt_digest: B3Hash,
        schema_version: u8,
    ) -> Self {
        Self {
            verified: true,
            mismatch_reason: None,
            computed_context_digest: context_digest,
            computed_output_digest: output_digest,
            computed_receipt_digest: receipt_digest,
            schema_version,
        }
    }

    /// Create a failed verification result.
    fn failure(
        reason: MismatchReason,
        context_digest: B3Hash,
        output_digest: B3Hash,
        receipt_digest: B3Hash,
        schema_version: u8,
    ) -> Self {
        Self {
            verified: false,
            mismatch_reason: Some(reason),
            computed_context_digest: context_digest,
            computed_output_digest: output_digest,
            computed_receipt_digest: receipt_digest,
            schema_version,
        }
    }
}

// =============================================================================
// Core Verification Functions
// =============================================================================

/// Compute context digest from claimed configuration.
///
/// This is the canonical algorithm for context digest computation:
/// BLAKE3(tenant_namespace || stack_hash || prompt_token_count || prompt_tokens...)
fn compute_context_digest(config: &ClaimedConfig) -> B3Hash {
    let mut buf = Vec::with_capacity(
        config.tenant_namespace.len()
            + 32
            + 4
            + (config.prompt_tokens.len() * 4),
    );

    buf.extend_from_slice(config.tenant_namespace.as_bytes());
    buf.extend_from_slice(config.stack_hash.as_bytes());
    buf.extend_from_slice(&(config.prompt_tokens.len() as u32).to_le_bytes());

    for token in &config.prompt_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }

    B3Hash::hash(&buf)
}

/// Build ReceiptDigestInput from claimed values.
fn build_receipt_input(
    claimed: &ClaimedValues,
    context_digest: &B3Hash,
    output_digest: &B3Hash,
) -> ReceiptDigestInput {
    let mut input = ReceiptDigestInput::new(
        *context_digest.as_bytes(),
        *claimed.run_head_hash.as_bytes(),
        *output_digest.as_bytes(),
        claimed.logical_prompt_tokens,
        claimed.prefix_cached_token_count,
        claimed.billed_input_tokens,
        claimed.logical_output_tokens,
        claimed.billed_output_tokens,
    );

    // V4 fields: Stop controller
    input = input.with_stop_controller(
        claimed.config.stop_reason_code.clone(),
        claimed.config.stop_reason_token_index,
        claimed.config.stop_policy_digest_b3,
    );

    // V4 fields: KV quota/residency
    input = input.with_kv_quota(
        claimed.config.tenant_kv_quota_bytes,
        claimed.config.tenant_kv_bytes_used,
        claimed.config.kv_evictions,
        claimed.config.kv_residency_policy_id.clone(),
        claimed.config.kv_quota_enforced,
    );

    // V4 fields: Prefix cache
    input = input.with_prefix_cache(
        claimed.config.prefix_kv_key_b3,
        claimed.config.prefix_cache_hit,
        claimed.config.prefix_kv_bytes,
    );

    // V4 fields: Model cache identity
    input = input.with_model_cache_identity(claimed.config.model_cache_identity_v2_digest_b3);

    // V5 fields: Equipment profile
    input = input.with_equipment_profile(
        claimed.config.equipment_profile_digest_b3,
        claimed.config.processor_id.clone(),
        claimed.config.mlx_version.clone(),
        claimed.config.ane_version.clone(),
    );

    // V5 fields: Citation binding
    input = input.with_citations(
        claimed.config.citations_merkle_root_b3,
        claimed.config.citation_count,
    );

    input
}

/// Verify a receipt against claimed values.
///
/// This is the main entry point for third-party verification. It:
/// 1. Recomputes the context digest from claimed config
/// 2. Recomputes the output digest from claimed output tokens
/// 3. Recomputes the receipt digest from all components
/// 4. Compares the recomputed receipt digest against the provided one
///
/// # Arguments
/// * `receipt_digest` - The receipt digest to verify against
/// * `claimed` - The claimed values (input, output, config)
/// * `schema_version` - Receipt schema version (1-5)
///
/// # Returns
/// * `Ok(VerificationResult)` - Verification completed (check `verified` field)
/// * `Err(AosError)` - Verification could not be performed (invalid schema, etc.)
///
/// # Stop Conditions
/// - Digest comparison complete → returns result
/// - Claimed data fails to parse → returns ParseError mismatch
///
/// # Next Conditions
/// - On match → verification succeeds, output is authentic
/// - On mismatch → verification fails, output or claims are modified
pub fn verify_receipt(
    receipt_digest: &B3Hash,
    claimed: &ClaimedValues,
    schema_version: u8,
) -> Result<VerificationResult> {
    // Validate schema version
    if schema_version > RECEIPT_SCHEMA_V5 {
        return Ok(VerificationResult::failure(
            MismatchReason::UnsupportedSchemaVersion {
                version: schema_version,
            },
            B3Hash::zero(),
            B3Hash::zero(),
            B3Hash::zero(),
            schema_version,
        ));
    }

    // Step 1: Recompute context digest from claimed config
    let context_digest = compute_context_digest(&claimed.config);

    // Step 2: Recompute output digest from claimed output tokens
    let output_digest = compute_output_digest(&claimed.output_tokens);

    // Step 3: Build receipt input and compute receipt digest
    let receipt_input = build_receipt_input(claimed, &context_digest, &output_digest);

    let computed_receipt_digest = match compute_receipt_digest(&receipt_input, schema_version) {
        Some(digest) => digest,
        None => {
            return Ok(VerificationResult::failure(
                MismatchReason::UnsupportedSchemaVersion {
                    version: schema_version,
                },
                context_digest,
                output_digest,
                B3Hash::zero(),
                schema_version,
            ));
        }
    };

    // Step 4: Compare recomputed receipt digest against provided receipt digest
    if &computed_receipt_digest == receipt_digest {
        Ok(VerificationResult::success(
            context_digest,
            output_digest,
            computed_receipt_digest,
            schema_version,
        ))
    } else {
        Ok(VerificationResult::failure(
            MismatchReason::ReceiptDigestMismatch {
                computed: computed_receipt_digest.to_hex(),
                claimed: receipt_digest.to_hex(),
            },
            context_digest,
            output_digest,
            computed_receipt_digest,
            schema_version,
        ))
    }
}

/// Verify a receipt from hex-encoded receipt digest string.
///
/// Convenience wrapper around [`verify_receipt`] that accepts hex input.
pub fn verify_receipt_hex(
    receipt_digest_hex: &str,
    claimed: &ClaimedValues,
    schema_version: u8,
) -> Result<VerificationResult> {
    let receipt_digest = B3Hash::from_hex(receipt_digest_hex).map_err(|e| {
        AosError::Validation(format!("Invalid receipt digest hex: {}", e))
    })?;

    verify_receipt(&receipt_digest, claimed, schema_version)
}

/// Verify with pre-computed digests for cases where caller already has hashes.
///
/// This variant is useful when the third party has the individual digests
/// from the receipt bundle rather than raw data.
#[allow(clippy::too_many_arguments)]
pub fn verify_receipt_with_precomputed(
    receipt_digest: &B3Hash,
    context_digest: &B3Hash,
    run_head_hash: &B3Hash,
    output_digest: &B3Hash,
    logical_prompt_tokens: u32,
    prefix_cached_token_count: u32,
    billed_input_tokens: u32,
    logical_output_tokens: u32,
    billed_output_tokens: u32,
    schema_version: u8,
) -> Result<VerificationResult> {
    // Validate schema version
    if schema_version > RECEIPT_SCHEMA_V5 {
        return Ok(VerificationResult::failure(
            MismatchReason::UnsupportedSchemaVersion {
                version: schema_version,
            },
            *context_digest,
            *output_digest,
            B3Hash::zero(),
            schema_version,
        ));
    }

    // Build minimal receipt input with precomputed digests
    let receipt_input = ReceiptDigestInput::new(
        *context_digest.as_bytes(),
        *run_head_hash.as_bytes(),
        *output_digest.as_bytes(),
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
    );

    let computed_receipt_digest = match compute_receipt_digest(&receipt_input, schema_version) {
        Some(digest) => digest,
        None => {
            return Ok(VerificationResult::failure(
                MismatchReason::UnsupportedSchemaVersion {
                    version: schema_version,
                },
                *context_digest,
                *output_digest,
                B3Hash::zero(),
                schema_version,
            ));
        }
    };

    if &computed_receipt_digest == receipt_digest {
        Ok(VerificationResult::success(
            *context_digest,
            *output_digest,
            computed_receipt_digest,
            schema_version,
        ))
    } else {
        Ok(VerificationResult::failure(
            MismatchReason::ReceiptDigestMismatch {
                computed: computed_receipt_digest.to_hex(),
                claimed: receipt_digest.to_hex(),
            },
            *context_digest,
            *output_digest,
            computed_receipt_digest,
            schema_version,
        ))
    }
}

// =============================================================================
// Evidence Chain Integration
// =============================================================================

/// Verify that an `InferenceReceiptRef` from the evidence chain is internally consistent.
///
/// This verifies that the `receipt_digest` in the evidence envelope was correctly
/// computed from the other digest fields. It does NOT verify:
/// - That output_digest matches specific output tokens (you don't have them)
/// - That run_head_hash matches specific decisions (you don't have them)
/// - The cryptographic signature (use `EvidenceVerifier` for that)
///
/// # Use Case
///
/// When you receive an `InferenceReceiptRef` from an evidence chain and want to
/// verify its internal consistency before trusting the digest values.
///
/// # Arguments
/// * `receipt_ref` - The inference receipt reference from an evidence envelope
/// * `context_digest` - The context digest (if you have it) or None to skip context verification
/// * `schema_version` - Receipt schema version
///
/// # Returns
/// * `VerificationResult` indicating whether the receipt_digest matches the computed value
pub fn verify_evidence_receipt_consistency(
    receipt_ref: &InferenceReceiptRef,
    context_digest: Option<&B3Hash>,
    schema_version: u8,
) -> Result<VerificationResult> {
    // If caller provided context_digest, use it; otherwise use a placeholder
    // (we can't verify context binding without knowing what context to expect)
    let ctx_digest = context_digest.copied().unwrap_or_else(B3Hash::zero);

    // Build receipt input from the evidence reference
    let mut input = ReceiptDigestInput::new(
        *ctx_digest.as_bytes(),
        *receipt_ref.run_head_hash.as_bytes(),
        *receipt_ref.output_digest.as_bytes(),
        receipt_ref.logical_prompt_tokens,
        receipt_ref.prefix_cached_token_count,
        receipt_ref.billed_input_tokens,
        receipt_ref.logical_output_tokens,
        receipt_ref.billed_output_tokens,
    );

    // Add stop controller fields
    input = input.with_stop_controller(
        receipt_ref.stop_reason_code.clone(),
        receipt_ref.stop_reason_token_index,
        receipt_ref.stop_policy_digest_b3.map(|h| *h.as_bytes()),
    );

    // Add model cache identity
    input = input.with_model_cache_identity(
        receipt_ref.model_cache_identity_v2_digest_b3.map(|h| *h.as_bytes()),
    );

    // Compute expected receipt digest
    let computed = match compute_receipt_digest(&input, schema_version) {
        Some(d) => d,
        None => {
            return Ok(VerificationResult::failure(
                MismatchReason::UnsupportedSchemaVersion {
                    version: schema_version,
                },
                ctx_digest,
                receipt_ref.output_digest,
                B3Hash::zero(),
                schema_version,
            ));
        }
    };

    if computed == receipt_ref.receipt_digest {
        Ok(VerificationResult::success(
            ctx_digest,
            receipt_ref.output_digest,
            computed,
            schema_version,
        ))
    } else {
        Ok(VerificationResult::failure(
            MismatchReason::ReceiptDigestMismatch {
                computed: computed.to_hex(),
                claimed: receipt_ref.receipt_digest.to_hex(),
            },
            ctx_digest,
            receipt_ref.output_digest,
            computed,
            schema_version,
        ))
    }
}

/// Verify claimed output tokens against an `InferenceReceiptRef`.
///
/// This is the most common third-party verification: you have a receipt from
/// the evidence chain AND the claimed raw output tokens, and want to verify
/// the receipt actually commits to those tokens.
///
/// # Arguments
/// * `receipt_ref` - The inference receipt reference from an evidence envelope
/// * `claimed_output_tokens` - The output tokens the publisher claims were produced
/// * `context_digest` - The context digest (compute from tenant + stack + prompt tokens)
/// * `schema_version` - Receipt schema version
///
/// # Returns
/// * `VerificationResult` with `verified=true` if output_digest matches AND
///   receipt_digest is internally consistent
pub fn verify_output_against_receipt(
    receipt_ref: &InferenceReceiptRef,
    claimed_output_tokens: &[u32],
    context_digest: &B3Hash,
    schema_version: u8,
) -> Result<VerificationResult> {
    // Step 1: Verify claimed output tokens hash to the receipt's output_digest
    let computed_output_digest = compute_output_digest(claimed_output_tokens);

    if computed_output_digest != receipt_ref.output_digest {
        return Ok(VerificationResult::failure(
            MismatchReason::OutputDigestMismatch {
                computed: computed_output_digest.to_hex(),
                expected: Some(receipt_ref.output_digest.to_hex()),
            },
            *context_digest,
            computed_output_digest,
            B3Hash::zero(),
            schema_version,
        ));
    }

    // Step 2: Verify the receipt is internally consistent
    verify_evidence_receipt_consistency(receipt_ref, Some(context_digest), schema_version)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt_digest::{RECEIPT_SCHEMA_V1, RECEIPT_SCHEMA_V4};

    fn sample_config() -> ClaimedConfig {
        ClaimedConfig {
            tenant_namespace: "tenant-reference".to_string(),
            stack_hash: B3Hash::hash(b"stack-hash-123"),
            prompt_tokens: vec![11, 22, 33],
            ..Default::default()
        }
    }

    fn sample_claimed_values() -> ClaimedValues {
        ClaimedValues::new(
            vec![101, 102, 103],
            B3Hash::hash(b"run-head-hash"),
            sample_config(),
        )
    }

    #[test]
    fn test_context_digest_computation() {
        let config = sample_config();
        let digest = compute_context_digest(&config);

        // Verify determinism
        let digest2 = compute_context_digest(&config);
        assert_eq!(digest, digest2, "Context digest must be deterministic");

        // Verify different config produces different digest
        let mut config2 = config.clone();
        config2.tenant_namespace = "tenant-other".to_string();
        let digest3 = compute_context_digest(&config2);
        assert_ne!(digest, digest3, "Different config should produce different digest");
    }

    #[test]
    fn test_output_digest_computation() {
        let tokens = vec![101u32, 102, 103];
        let digest = compute_output_digest(&tokens);

        // Verify determinism
        let digest2 = compute_output_digest(&tokens);
        assert_eq!(digest, digest2, "Output digest must be deterministic");

        // Verify different tokens produce different digest
        let tokens2 = vec![101u32, 102, 104];
        let digest3 = compute_output_digest(&tokens2);
        assert_ne!(digest, digest3, "Different tokens should produce different digest");
    }

    #[test]
    fn test_verification_with_matching_receipt() {
        let claimed = sample_claimed_values();

        // Compute the actual receipt digest
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let expected_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();

        // Verify with correct receipt digest
        let result = verify_receipt(&expected_receipt, &claimed, RECEIPT_SCHEMA_V1).unwrap();

        assert!(result.verified, "Verification should succeed with matching receipt");
        assert!(result.mismatch_reason.is_none());
        assert_eq!(result.computed_context_digest, context_digest);
        assert_eq!(result.computed_output_digest, output_digest);
        assert_eq!(result.computed_receipt_digest, expected_receipt);
    }

    #[test]
    fn test_verification_with_wrong_receipt() {
        let claimed = sample_claimed_values();
        let wrong_receipt = B3Hash::hash(b"wrong-receipt-digest");

        let result = verify_receipt(&wrong_receipt, &claimed, RECEIPT_SCHEMA_V1).unwrap();

        assert!(!result.verified, "Verification should fail with wrong receipt");
        assert!(matches!(
            result.mismatch_reason,
            Some(MismatchReason::ReceiptDigestMismatch { .. })
        ));
    }

    #[test]
    fn test_verification_with_tampered_output() {
        let claimed = sample_claimed_values();

        // Compute legitimate receipt
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let legitimate_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();

        // Tamper with output tokens
        let mut tampered = claimed.clone();
        tampered.output_tokens = vec![999, 998, 997];

        // Verification should fail
        let result = verify_receipt(&legitimate_receipt, &tampered, RECEIPT_SCHEMA_V1).unwrap();

        assert!(!result.verified, "Tampered output should fail verification");
        assert!(matches!(
            result.mismatch_reason,
            Some(MismatchReason::ReceiptDigestMismatch { .. })
        ));
    }

    #[test]
    fn test_verification_with_tampered_config() {
        let claimed = sample_claimed_values();

        // Compute legitimate receipt
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let legitimate_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();

        // Tamper with config
        let mut tampered = claimed.clone();
        tampered.config.tenant_namespace = "tampered-tenant".to_string();

        // Verification should fail
        let result = verify_receipt(&legitimate_receipt, &tampered, RECEIPT_SCHEMA_V1).unwrap();

        assert!(!result.verified, "Tampered config should fail verification");
    }

    #[test]
    fn test_verification_with_unsupported_schema() {
        let claimed = sample_claimed_values();
        let receipt = B3Hash::hash(b"some-receipt");

        let result = verify_receipt(&receipt, &claimed, 99).unwrap();

        assert!(!result.verified);
        assert!(matches!(
            result.mismatch_reason,
            Some(MismatchReason::UnsupportedSchemaVersion { version: 99 })
        ));
    }

    #[test]
    fn test_verify_receipt_hex() {
        let claimed = sample_claimed_values();

        // Compute the actual receipt digest
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let expected_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();

        // Verify using hex string
        let result = verify_receipt_hex(
            &expected_receipt.to_hex(),
            &claimed,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(result.verified, "Hex verification should succeed");
    }

    #[test]
    fn test_verify_receipt_hex_invalid() {
        let claimed = sample_claimed_values();

        let result = verify_receipt_hex("not-valid-hex", &claimed, RECEIPT_SCHEMA_V1);

        assert!(result.is_err(), "Invalid hex should return error");
    }

    #[test]
    fn test_verification_with_precomputed_digests() {
        let context_digest = B3Hash::hash(b"context");
        let run_head_hash = B3Hash::hash(b"run-head");
        let output_digest = B3Hash::hash(b"output");

        // Compute expected receipt digest
        let receipt_input = ReceiptDigestInput::new(
            *context_digest.as_bytes(),
            *run_head_hash.as_bytes(),
            *output_digest.as_bytes(),
            10,  // logical_prompt_tokens
            0,   // prefix_cached_token_count
            10,  // billed_input_tokens
            5,   // logical_output_tokens
            5,   // billed_output_tokens
        );
        let expected_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();

        // Verify with precomputed values
        let result = verify_receipt_with_precomputed(
            &expected_receipt,
            &context_digest,
            &run_head_hash,
            &output_digest,
            10, 0, 10, 5, 5,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(result.verified, "Precomputed verification should succeed");
    }

    #[test]
    fn test_v4_verification() {
        let mut claimed = sample_claimed_values();

        // Add V4 fields
        claimed.config.stop_reason_code = Some("EOS".to_string());
        claimed.config.stop_reason_token_index = Some(2);
        claimed.config.kv_quota_enforced = true;

        // Compute V4 receipt
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let expected_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V4).unwrap();

        // Verify
        let result = verify_receipt(&expected_receipt, &claimed, RECEIPT_SCHEMA_V4).unwrap();

        assert!(result.verified, "V4 verification should succeed");
        assert_eq!(result.schema_version, RECEIPT_SCHEMA_V4);
    }

    #[test]
    fn test_v5_verification_with_equipment_profile() {
        let mut claimed = sample_claimed_values();

        // Add V5 fields
        claimed.config.equipment_profile_digest_b3 = Some([0x42u8; 32]);
        claimed.config.processor_id = Some("Apple M4 Max".to_string());
        claimed.config.mlx_version = Some("0.21.0".to_string());
        claimed.config.citations_merkle_root_b3 = Some([0x43u8; 32]);
        claimed.config.citation_count = 5;

        // Compute V5 receipt
        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);
        let expected_receipt = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V5).unwrap();

        // Verify
        let result = verify_receipt(&expected_receipt, &claimed, RECEIPT_SCHEMA_V5).unwrap();

        assert!(result.verified, "V5 verification should succeed");
        assert_eq!(result.schema_version, RECEIPT_SCHEMA_V5);
    }

    #[test]
    fn test_mismatch_reason_display() {
        let reason = MismatchReason::ReceiptDigestMismatch {
            computed: "abc123".to_string(),
            claimed: "def456".to_string(),
        };

        let display = format!("{}", reason);
        assert!(display.contains("abc123"));
        assert!(display.contains("def456"));
    }

    #[test]
    fn test_different_schema_versions_produce_different_digests() {
        let claimed = sample_claimed_values();

        let context_digest = compute_context_digest(&claimed.config);
        let output_digest = compute_output_digest(&claimed.output_tokens);
        let receipt_input = build_receipt_input(&claimed, &context_digest, &output_digest);

        let v1 = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V1).unwrap();
        let v4 = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V4).unwrap();
        let v5 = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V5).unwrap();

        assert_ne!(v1, v4, "V1 and V4 should differ");
        assert_ne!(v4, v5, "V4 and V5 should differ");
    }

    #[test]
    fn test_claimed_values_builder() {
        let config = sample_config();
        let claimed = ClaimedValues::new(
            vec![1, 2, 3],
            B3Hash::hash(b"run-head"),
            config,
        ).with_token_accounting(100, 10, 90, 50, 50);

        assert_eq!(claimed.logical_prompt_tokens, 100);
        assert_eq!(claimed.prefix_cached_token_count, 10);
        assert_eq!(claimed.billed_input_tokens, 90);
        assert_eq!(claimed.logical_output_tokens, 50);
        assert_eq!(claimed.billed_output_tokens, 50);
    }

    #[test]
    fn test_verification_result_json_serialization() {
        let result = VerificationResult::success(
            B3Hash::hash(b"context"),
            B3Hash::hash(b"output"),
            B3Hash::hash(b"receipt"),
            RECEIPT_SCHEMA_V1,
        );

        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: VerificationResult = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.verified, result.verified);
        assert_eq!(parsed.schema_version, result.schema_version);
    }

    // =========================================================================
    // Evidence Chain Integration Tests
    // =========================================================================

    fn sample_inference_receipt_ref() -> InferenceReceiptRef {
        // Create a receipt ref with consistent values
        let context_digest = B3Hash::hash(b"context");
        let run_head_hash = B3Hash::hash(b"run-head");
        let output_digest = B3Hash::hash(b"output");

        // Compute the V1 receipt digest manually
        let receipt_input = ReceiptDigestInput::new(
            *context_digest.as_bytes(),
            *run_head_hash.as_bytes(),
            *output_digest.as_bytes(),
            10, 0, 10, 5, 5,
        );
        let receipt_digest = crate::receipt_digest::compute_receipt_digest(
            &receipt_input,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        InferenceReceiptRef {
            trace_id: "trace-test".to_string(),
            run_head_hash,
            output_digest,
            receipt_digest,
            logical_prompt_tokens: 10,
            prefix_cached_token_count: 0,
            billed_input_tokens: 10,
            logical_output_tokens: 5,
            billed_output_tokens: 5,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            model_cache_identity_v2_digest_b3: None,
            backend_used: String::new(),
            backend_attestation_b3: None,
            seed_lineage_hash: None,
            adapter_training_lineage_digest: None,
        }
    }

    #[test]
    fn test_verify_evidence_receipt_consistency_valid() {
        let receipt_ref = sample_inference_receipt_ref();
        let context_digest = B3Hash::hash(b"context");

        let result = verify_evidence_receipt_consistency(
            &receipt_ref,
            Some(&context_digest),
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(result.verified, "Consistent receipt should verify");
    }

    #[test]
    fn test_verify_evidence_receipt_consistency_tampered() {
        let mut receipt_ref = sample_inference_receipt_ref();
        // Tamper with the receipt digest
        receipt_ref.receipt_digest = B3Hash::hash(b"tampered");

        let context_digest = B3Hash::hash(b"context");

        let result = verify_evidence_receipt_consistency(
            &receipt_ref,
            Some(&context_digest),
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(!result.verified, "Tampered receipt should fail");
        assert!(matches!(
            result.mismatch_reason,
            Some(MismatchReason::ReceiptDigestMismatch { .. })
        ));
    }

    #[test]
    fn test_verify_output_against_receipt() {
        // Create output tokens and their digest
        let output_tokens = vec![100u32, 101, 102, 103, 104];
        let output_digest = compute_output_digest(&output_tokens);
        let context_digest = B3Hash::hash(b"context");
        let run_head_hash = B3Hash::hash(b"run-head");

        // Build a receipt ref with matching output digest
        let receipt_input = ReceiptDigestInput::new(
            *context_digest.as_bytes(),
            *run_head_hash.as_bytes(),
            *output_digest.as_bytes(),
            10, 0, 10,
            output_tokens.len() as u32,
            output_tokens.len() as u32,
        );
        let receipt_digest = crate::receipt_digest::compute_receipt_digest(
            &receipt_input,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        let receipt_ref = InferenceReceiptRef {
            trace_id: "trace-output".to_string(),
            run_head_hash,
            output_digest,
            receipt_digest,
            logical_prompt_tokens: 10,
            prefix_cached_token_count: 0,
            billed_input_tokens: 10,
            logical_output_tokens: output_tokens.len() as u32,
            billed_output_tokens: output_tokens.len() as u32,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            model_cache_identity_v2_digest_b3: None,
            backend_used: String::new(),
            backend_attestation_b3: None,
            seed_lineage_hash: None,
            adapter_training_lineage_digest: None,
        };

        // Verify with correct output tokens
        let result = verify_output_against_receipt(
            &receipt_ref,
            &output_tokens,
            &context_digest,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(result.verified, "Matching output should verify");
    }

    #[test]
    fn test_verify_output_against_receipt_wrong_tokens() {
        let receipt_ref = sample_inference_receipt_ref();
        let context_digest = B3Hash::hash(b"context");

        // Claim different output tokens than what's in the receipt
        let wrong_output_tokens = vec![999u32, 998, 997];

        let result = verify_output_against_receipt(
            &receipt_ref,
            &wrong_output_tokens,
            &context_digest,
            RECEIPT_SCHEMA_V1,
        ).unwrap();

        assert!(!result.verified, "Wrong output tokens should fail");
        assert!(matches!(
            result.mismatch_reason,
            Some(MismatchReason::OutputDigestMismatch { .. })
        ));
    }
}

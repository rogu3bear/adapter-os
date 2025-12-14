//! Unified verifier for evidence envelopes.
//!
//! Provides cryptographic verification of evidence envelopes including:
//! - Schema version validation
//! - Root hash computation and verification
//! - Chain linkage verification
//! - Ed25519 signature verification (requires `evidence-signing` feature)
//!
//! # Example
//!
//! ```ignore
//! use adapteros_core::evidence_verifier::{EvidenceVerifier, EnvelopeVerificationResult};
//!
//! let verifier = EvidenceVerifier::new();
//! let result = verifier.verify_envelope(&envelope, None)?;
//! assert!(result.is_valid);
//! ```

use crate::evidence_envelope::{
    EvidenceEnvelopeV1, EvidenceScope, EVIDENCE_ENVELOPE_SCHEMA_VERSION,
};
use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error code for evidence chain divergence
pub const EVIDENCE_CHAIN_DIVERGED_CODE: &str = "EVIDENCE_CHAIN_DIVERGED";

/// Create a chain divergence error
pub fn evidence_chain_divergence(msg: impl Into<String>) -> AosError {
    AosError::Validation(format!("{}: {}", EVIDENCE_CHAIN_DIVERGED_CODE, msg.into()))
}

/// Check if an error is a chain divergence error
pub fn is_evidence_chain_divergence(err: &AosError) -> bool {
    matches!(err, AosError::Validation(msg) if msg.contains(EVIDENCE_CHAIN_DIVERGED_CODE))
}

/// Result of single envelope verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeVerificationResult {
    /// Overall validity of the envelope
    pub is_valid: bool,
    /// Schema version is supported
    pub schema_version_ok: bool,
    /// Signature is valid (only if signature present and verification enabled)
    pub signature_valid: bool,
    /// Root hash matches computed value
    pub root_matches: bool,
    /// Chain linkage is valid (previous_root matches expected)
    pub chain_link_valid: bool,
    /// Payload reference matches scope
    pub payload_valid: bool,
    /// Description of the first validation failure (if any)
    pub error_message: Option<String>,
}

impl Default for EnvelopeVerificationResult {
    fn default() -> Self {
        Self {
            is_valid: false,
            schema_version_ok: false,
            signature_valid: false,
            root_matches: false,
            chain_link_valid: false,
            payload_valid: false,
            error_message: None,
        }
    }
}

/// Result of chain verification
///
/// Contains detailed information about the integrity of an evidence chain,
/// including whether the chain is valid and where any issues were detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Overall validity of the chain
    pub is_valid: bool,
    /// Number of envelopes checked during verification
    pub envelopes_checked: usize,
    /// Index of first invalid envelope (if any)
    pub first_invalid_index: Option<usize>,
    /// Whether a chain divergence was detected
    pub divergence_detected: bool,
    /// Description of the first validation failure
    pub error_message: Option<String>,
}

impl Default for ChainVerificationResult {
    fn default() -> Self {
        Self {
            is_valid: true,
            envelopes_checked: 0,
            first_invalid_index: None,
            divergence_detected: false,
            error_message: None,
        }
    }
}

/// Unified verifier for all evidence envelope types
///
/// Supports verification of:
/// - Telemetry bundle envelopes
/// - Policy audit envelopes
/// - Inference trace envelopes
///
/// All three types use the same verification logic and chain linking.
pub struct EvidenceVerifier {
    /// Trusted public keys for signature verification (key_id -> pubkey hex)
    trusted_keys: HashMap<String, String>,
}

impl Default for EvidenceVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl EvidenceVerifier {
    /// Create a new verifier with no trusted keys
    pub fn new() -> Self {
        Self {
            trusted_keys: HashMap::new(),
        }
    }

    /// Register a trusted public key
    ///
    /// # Arguments
    /// * `key_id` - Key identifier (e.g., "key-abc12345")
    /// * `pubkey_hex` - Hex-encoded 32-byte public key
    pub fn add_trusted_key(&mut self, key_id: &str, pubkey_hex: &str) {
        self.trusted_keys
            .insert(key_id.to_string(), pubkey_hex.to_string());
    }

    /// Verify a single envelope
    ///
    /// # Arguments
    /// * `envelope` - The envelope to verify
    /// * `expected_previous_root` - Expected previous root for chain linkage
    ///   - `None` if this is the first envelope in the chain
    ///   - `Some(hash)` if this should link to a previous envelope
    ///
    /// # Returns
    /// Verification result with detailed status for each check
    pub fn verify_envelope(
        &self,
        envelope: &EvidenceEnvelopeV1,
        expected_previous_root: Option<&B3Hash>,
    ) -> Result<EnvelopeVerificationResult> {
        let mut result = EnvelopeVerificationResult::default();

        // 1. Verify schema version
        if envelope.schema_version != EVIDENCE_ENVELOPE_SCHEMA_VERSION {
            result.error_message = Some(format!(
                "Schema version mismatch: expected {}, got {}",
                EVIDENCE_ENVELOPE_SCHEMA_VERSION, envelope.schema_version
            ));
            return Ok(result);
        }
        result.schema_version_ok = true;

        // 2. Verify payload matches scope
        if let Err(e) = envelope.validate() {
            result.error_message = Some(format!("Payload validation failed: {}", e));
            return Ok(result);
        }
        result.payload_valid = true;

        // 3. Verify root matches computed value
        let computed_root = self.compute_envelope_root(envelope)?;
        if computed_root != envelope.root {
            result.error_message = Some(format!(
                "Root hash mismatch: expected {}, got {}",
                computed_root.to_short_hex(),
                envelope.root.to_short_hex()
            ));
            return Ok(result);
        }
        result.root_matches = true;

        // 4. Verify chain linkage
        match (&envelope.previous_root, expected_previous_root) {
            (None, None) => {
                // First in chain, valid
                result.chain_link_valid = true;
            }
            (Some(prev), Some(expected)) if prev == expected => {
                // Links correctly to previous
                result.chain_link_valid = true;
            }
            (Some(prev), Some(expected)) => {
                // Mismatch - chain divergence
                result.error_message = Some(format!(
                    "Chain link mismatch: expected {}, got {}",
                    expected.to_short_hex(),
                    prev.to_short_hex()
                ));
                return Ok(result);
            }
            (Some(_), None) => {
                // Claims previous but we expected first
                // This might be valid if we're verifying a partial chain
                result.chain_link_valid = true;
            }
            (None, Some(expected)) => {
                // Claims first but we expected a link
                result.error_message = Some(format!(
                    "Unexpected chain break: expected link to {}, got None",
                    expected.to_short_hex()
                ));
                return Ok(result);
            }
        }

        // 5. Verify signature (if present and keys available)
        if envelope.is_signed() {
            result.signature_valid = self.verify_signature(envelope)?;
            if !result.signature_valid {
                result.error_message = Some("Signature verification failed".to_string());
                return Ok(result);
            }
        } else {
            // No signature - mark as valid (unsigned envelopes allowed)
            result.signature_valid = true;
        }

        result.is_valid = true;
        Ok(result)
    }

    /// Verify an evidence chain
    ///
    /// Verifies each envelope in sequence and checks chain linkage.
    ///
    /// # Arguments
    /// * `envelopes` - Ordered list of envelopes (oldest first)
    ///
    /// # Returns
    /// Chain verification result
    pub fn verify_chain(
        &self,
        envelopes: &[EvidenceEnvelopeV1],
    ) -> Result<ChainVerificationResult> {
        if envelopes.is_empty() {
            return Ok(ChainVerificationResult::default());
        }

        let mut prev_root: Option<B3Hash> = None;

        for (i, envelope) in envelopes.iter().enumerate() {
            let result = self.verify_envelope(envelope, prev_root.as_ref())?;

            if !result.is_valid {
                return Ok(ChainVerificationResult {
                    is_valid: false,
                    envelopes_checked: i + 1,
                    first_invalid_index: Some(i),
                    divergence_detected: !result.chain_link_valid,
                    error_message: result.error_message,
                });
            }

            prev_root = Some(envelope.root);
        }

        Ok(ChainVerificationResult {
            is_valid: true,
            envelopes_checked: envelopes.len(),
            first_invalid_index: None,
            divergence_detected: false,
            error_message: None,
        })
    }

    /// Compute the root hash for an envelope's payload
    fn compute_envelope_root(&self, envelope: &EvidenceEnvelopeV1) -> Result<B3Hash> {
        match envelope.scope {
            EvidenceScope::Telemetry => {
                let ref_data = envelope.bundle_metadata_ref.as_ref().ok_or_else(|| {
                    AosError::Validation("Missing bundle_metadata_ref".to_string())
                })?;
                // Root = hash of bundle_hash + merkle_root
                Ok(B3Hash::hash_multi(&[
                    ref_data.bundle_hash.as_bytes(),
                    ref_data.merkle_root.as_bytes(),
                ]))
            }
            EvidenceScope::Policy => {
                let ref_data = envelope
                    .policy_audit_ref
                    .as_ref()
                    .ok_or_else(|| AosError::Validation("Missing policy_audit_ref".to_string()))?;
                // Root = entry_hash from policy decision
                Ok(ref_data.entry_hash)
            }
            EvidenceScope::Inference => {
                let ref_data = envelope.inference_receipt_ref.as_ref().ok_or_else(|| {
                    AosError::Validation("Missing inference_receipt_ref".to_string())
                })?;
                // Root = receipt_digest from trace receipt
                Ok(ref_data.receipt_digest)
            }
        }
    }

    /// Verify envelope signature
    #[cfg(feature = "evidence-signing")]
    fn verify_signature(&self, envelope: &EvidenceEnvelopeV1) -> Result<bool> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        // Get public key for this key_id
        let pubkey_hex = match self.trusted_keys.get(&envelope.key_id) {
            Some(key) => key,
            None => {
                // Key not trusted - use the one in envelope if present
                if envelope.public_key.is_empty() {
                    return Ok(false);
                }
                &envelope.public_key
            }
        };

        // Decode public key
        let pubkey_bytes = hex::decode(pubkey_hex)
            .map_err(|e| AosError::Crypto(format!("Invalid pubkey hex: {}", e)))?;

        if pubkey_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid pubkey length: expected 32, got {}",
                pubkey_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&pubkey_bytes);

        let verifying_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| AosError::Crypto(format!("Invalid public key: {}", e)))?;

        // Decode signature
        let sig_bytes = hex::decode(&envelope.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: expected 64, got {}",
                sig_bytes.len()
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array);

        // Verify signature over canonical bytes
        let canonical_bytes = envelope.to_canonical_bytes();
        Ok(verifying_key.verify(&canonical_bytes, &signature).is_ok())
    }

    /// Verify envelope signature (stub when feature disabled)
    #[cfg(not(feature = "evidence-signing"))]
    fn verify_signature(&self, _envelope: &EvidenceEnvelopeV1) -> Result<bool> {
        // Without the feature, skip signature verification
        Ok(true)
    }
}

/// Sign an envelope with Ed25519 (requires `evidence-signing` feature)
#[cfg(feature = "evidence-signing")]
pub fn sign_envelope(envelope: &mut EvidenceEnvelopeV1, signing_key_hex: &str) -> Result<()> {
    use crate::evidence_envelope::compute_key_id;
    use ed25519_dalek::{Signer, SigningKey};

    // Decode signing key
    let key_bytes = hex::decode(signing_key_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid signing key hex: {}", e)))?;

    if key_bytes.len() != 32 {
        return Err(AosError::Crypto(format!(
            "Invalid signing key length: expected 32, got {}",
            key_bytes.len()
        )));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    let signing_key = SigningKey::from_bytes(&key_array);

    // Get public key
    let public_key = signing_key.verifying_key();
    let pubkey_hex = hex::encode(public_key.as_bytes());

    // Compute key ID
    let key_id = compute_key_id(public_key.as_bytes());

    // Sign canonical bytes
    let canonical_bytes = envelope.to_canonical_bytes();
    let signature = signing_key.sign(&canonical_bytes);

    // Update envelope
    envelope.signature = hex::encode(signature.to_bytes());
    envelope.public_key = pubkey_hex;
    envelope.key_id = key_id;
    envelope.signed_at_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence_envelope::{BundleMetadataRef, InferenceReceiptRef, PolicyAuditRef};

    fn sample_bundle_ref() -> BundleMetadataRef {
        BundleMetadataRef {
            bundle_hash: B3Hash::hash(b"bundle-content"),
            merkle_root: B3Hash::hash(b"merkle-root"),
            event_count: 100,
            cpid: Some("cp-001".to_string()),
            sequence_no: Some(42),
        }
    }

    fn sample_policy_ref() -> PolicyAuditRef {
        PolicyAuditRef {
            decision_id: "dec-001".to_string(),
            entry_hash: B3Hash::hash(b"policy-entry"),
            chain_sequence: 1,
            policy_pack_id: "pack-egress".to_string(),
            hook: "OnBeforeInference".to_string(),
            decision: "allow".to_string(),
        }
    }

    fn sample_inference_ref() -> InferenceReceiptRef {
        InferenceReceiptRef {
            trace_id: "trace-001".to_string(),
            run_head_hash: B3Hash::hash(b"run-head"),
            output_digest: B3Hash::hash(b"output"),
            receipt_digest: B3Hash::hash(b"receipt"),
            logical_prompt_tokens: 100,
            prefix_cached_token_count: 20,
            billed_input_tokens: 80,
            logical_output_tokens: 50,
            billed_output_tokens: 50,
            stop_reason_code: Some("end_turn".to_string()),
            stop_reason_token_index: Some(49),
            stop_policy_digest_b3: Some(B3Hash::hash(b"stop-policy")),
            model_cache_identity_v2_digest_b3: Some(B3Hash::hash(b"model-cache-id")),
        }
    }

    #[test]
    fn test_verify_telemetry_envelope() {
        let verifier = EvidenceVerifier::new();
        let env =
            EvidenceEnvelopeV1::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let result = verifier.verify_envelope(&env, None).unwrap();
        assert!(
            result.is_valid,
            "Telemetry envelope should verify: {:?}",
            result.error_message
        );
        assert!(result.schema_version_ok);
        assert!(result.root_matches);
        assert!(result.chain_link_valid);
    }

    #[test]
    fn test_verify_policy_envelope() {
        let verifier = EvidenceVerifier::new();
        let env = EvidenceEnvelopeV1::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

        let result = verifier.verify_envelope(&env, None).unwrap();
        assert!(
            result.is_valid,
            "Policy envelope should verify: {:?}",
            result.error_message
        );
    }

    #[test]
    fn test_verify_inference_envelope() {
        let verifier = EvidenceVerifier::new();
        let env =
            EvidenceEnvelopeV1::new_inference("tenant-1".to_string(), sample_inference_ref(), None);

        let result = verifier.verify_envelope(&env, None).unwrap();
        assert!(
            result.is_valid,
            "Inference envelope should verify: {:?}",
            result.error_message
        );
    }

    /// AC-1: test_evidence_envelope_verifies_telemetry_policy_inference
    #[test]
    fn test_evidence_envelope_verifies_telemetry_policy_inference() {
        let verifier = EvidenceVerifier::new();

        // Telemetry
        let telemetry_env =
            EvidenceEnvelopeV1::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let result = verifier.verify_envelope(&telemetry_env, None).unwrap();
        assert!(result.is_valid, "Telemetry envelope should verify");

        // Policy
        let policy_env =
            EvidenceEnvelopeV1::new_policy("tenant-1".to_string(), sample_policy_ref(), None);
        let result = verifier.verify_envelope(&policy_env, None).unwrap();
        assert!(result.is_valid, "Policy envelope should verify");

        // Inference
        let inference_env =
            EvidenceEnvelopeV1::new_inference("tenant-1".to_string(), sample_inference_ref(), None);
        let result = verifier.verify_envelope(&inference_env, None).unwrap();
        assert!(result.is_valid, "Inference envelope should verify");
    }

    /// AC-2: test_evidence_chain_linking_break_emits_divergence
    #[test]
    fn test_evidence_chain_linking_break_emits_divergence() {
        let verifier = EvidenceVerifier::new();

        // Create chain of 3 envelopes
        let env1 =
            EvidenceEnvelopeV1::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let env2 = EvidenceEnvelopeV1::new_telemetry(
            "tenant-1".to_string(),
            BundleMetadataRef {
                bundle_hash: B3Hash::hash(b"bundle-2"),
                merkle_root: B3Hash::hash(b"merkle-2"),
                event_count: 50,
                cpid: Some("cp-001".to_string()),
                sequence_no: Some(43),
            },
            Some(env1.root),
        );

        // Create env3 with WRONG previous_root (chain break)
        let env3 = EvidenceEnvelopeV1::new_telemetry(
            "tenant-1".to_string(),
            BundleMetadataRef {
                bundle_hash: B3Hash::hash(b"bundle-3"),
                merkle_root: B3Hash::hash(b"merkle-3"),
                event_count: 25,
                cpid: Some("cp-001".to_string()),
                sequence_no: Some(44),
            },
            Some(B3Hash::hash(b"wrong-previous")), // Wrong!
        );

        let result = verifier.verify_chain(&[env1, env2, env3]).unwrap();

        assert!(!result.is_valid, "Chain with break should be invalid");
        assert!(result.divergence_detected, "Should detect divergence");
        assert_eq!(
            result.first_invalid_index,
            Some(2),
            "Third envelope should be invalid"
        );
    }

    /// AC-3: test_inference_envelope_contains_run_receipt_fields
    #[test]
    fn test_inference_envelope_contains_run_receipt_fields() {
        let receipt_ref = sample_inference_ref();
        let env =
            EvidenceEnvelopeV1::new_inference("tenant-1".to_string(), receipt_ref.clone(), None);

        let ref_data = env.inference_receipt_ref.as_ref().unwrap();

        // Verify all RunReceipt fields are present
        assert_eq!(ref_data.logical_prompt_tokens, 100);
        assert_eq!(ref_data.prefix_cached_token_count, 20);
        assert_eq!(ref_data.billed_input_tokens, 80);
        assert_eq!(ref_data.logical_output_tokens, 50);
        assert_eq!(ref_data.billed_output_tokens, 50);
        assert_eq!(ref_data.stop_reason_code, Some("end_turn".to_string()));
        assert_eq!(ref_data.stop_reason_token_index, Some(49));
        assert!(ref_data.stop_policy_digest_b3.is_some());
        assert!(ref_data.model_cache_identity_v2_digest_b3.is_some());

        // Verify envelope validates
        let verifier = EvidenceVerifier::new();
        let result = verifier.verify_envelope(&env, None).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    fn test_valid_chain_verification() {
        let verifier = EvidenceVerifier::new();

        let env1 =
            EvidenceEnvelopeV1::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

        let env2 = EvidenceEnvelopeV1::new_policy(
            "tenant-1".to_string(),
            PolicyAuditRef {
                decision_id: "dec-002".to_string(),
                entry_hash: B3Hash::hash(b"policy-entry-2"),
                chain_sequence: 2,
                policy_pack_id: "pack-egress".to_string(),
                hook: "OnAfterInference".to_string(),
                decision: "allow".to_string(),
            },
            Some(env1.root),
        );

        let result = verifier.verify_chain(&[env1, env2]).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.envelopes_checked, 2);
        assert!(!result.divergence_detected);
    }

    #[test]
    fn test_empty_chain_is_valid() {
        let verifier = EvidenceVerifier::new();
        let result = verifier.verify_chain(&[]).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.envelopes_checked, 0);
    }

    #[test]
    fn test_schema_version_mismatch() {
        let verifier = EvidenceVerifier::new();
        let mut env =
            EvidenceEnvelopeV1::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        env.schema_version = 99;

        let result = verifier.verify_envelope(&env, None).unwrap();
        assert!(!result.is_valid);
        assert!(!result.schema_version_ok);
        assert!(result.error_message.unwrap().contains("Schema version"));
    }

    #[test]
    fn test_is_evidence_chain_divergence() {
        let err = evidence_chain_divergence("test divergence");
        assert!(is_evidence_chain_divergence(&err));

        let other_err = AosError::Validation("other error".to_string());
        assert!(!is_evidence_chain_divergence(&other_err));
    }
}

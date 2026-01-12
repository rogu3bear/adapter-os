//! Unified verifier for evidence envelopes.
//!
//! Provides cryptographic verification of evidence envelopes including:
//! - Schema version validation
//! - Root hash computation and verification
//! - Chain linkage verification
//! - Ed25519 signature verification (requires `evidence-signing` feature)
//! - Ingestion validation (PR-005)
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

use crate::evidence_envelope::{EvidenceEnvelope, EvidenceScope, EVIDENCE_ENVELOPE_SCHEMA_VERSION};
use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Ingestion Validation Types and Functions (PR-005)
// =============================================================================

/// Result of evidence envelope ingestion validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// List of validation errors (empty if valid)
    pub errors: Vec<IngestionError>,
    /// Non-fatal warnings
    pub warnings: Vec<String>,
    /// Computed root hash (if computable)
    pub computed_root: Option<B3Hash>,
}

impl IngestionValidationResult {
    /// Create a successful validation result.
    pub fn success(computed_root: Option<B3Hash>) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            computed_root,
        }
    }

    /// Create a failed validation result with the given errors.
    pub fn failure(errors: Vec<IngestionError>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: Vec::new(),
            computed_root: None,
        }
    }
}

/// Specific error types for ingestion validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum IngestionError {
    /// Schema version not supported
    UnsupportedSchema { version: u8, max_supported: u8 },

    /// Root hash mismatch (tampered payload)
    RootMismatch { claimed: String, computed: String },

    /// Signature validation failed
    SignatureInvalid { reason: String },

    /// Signature missing when required
    SignatureMissing,

    /// Payload reference missing for claimed scope
    PayloadMissing { scope: String },

    /// Payload reference present for wrong scope
    PayloadScopeMismatch { claimed: String, present: String },

    /// Chain linkage invalid
    ChainLinkageInvalid { reason: String },

    /// Previous root mismatch
    PreviousRootMismatch {
        expected: Option<String>,
        claimed: Option<String>,
    },

    /// Tenant ID mismatch with payload
    TenantMismatch { envelope: String, payload: String },

    /// Timestamp in future (beyond allowed skew)
    TimestampInFuture { timestamp: String },

    /// Key ID doesn't match public key
    KeyIdMismatch { computed: String, claimed: String },

    /// Empty tenant ID
    EmptyTenantId,

    /// Multiple payload refs populated
    MultiplePayloads { count: usize },
}

impl std::fmt::Display for IngestionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema {
                version,
                max_supported,
            } => {
                write!(
                    f,
                    "Schema version {} not supported (max: {})",
                    version, max_supported
                )
            }
            Self::RootMismatch { claimed, computed } => {
                write!(
                    f,
                    "Root mismatch: claimed {} != computed {}",
                    claimed, computed
                )
            }
            Self::SignatureInvalid { reason } => write!(f, "Invalid signature: {}", reason),
            Self::SignatureMissing => write!(f, "Signature required but missing"),
            Self::PayloadMissing { scope } => write!(f, "Payload missing for scope {}", scope),
            Self::PayloadScopeMismatch { claimed, present } => {
                write!(
                    f,
                    "Scope mismatch: claimed {} but payload is {}",
                    claimed, present
                )
            }
            Self::ChainLinkageInvalid { reason } => {
                write!(f, "Chain linkage invalid: {}", reason)
            }
            Self::PreviousRootMismatch { expected, claimed } => {
                write!(
                    f,
                    "Previous root mismatch: expected {:?}, claimed {:?}",
                    expected, claimed
                )
            }
            Self::TenantMismatch { envelope, payload } => {
                write!(
                    f,
                    "Tenant mismatch: envelope={} payload={}",
                    envelope, payload
                )
            }
            Self::TimestampInFuture { timestamp } => {
                write!(f, "Timestamp in future: {}", timestamp)
            }
            Self::KeyIdMismatch { computed, claimed } => {
                write!(
                    f,
                    "Key ID mismatch: computed {} != claimed {}",
                    computed, claimed
                )
            }
            Self::EmptyTenantId => write!(f, "Tenant ID is empty"),
            Self::MultiplePayloads { count } => {
                write!(
                    f,
                    "Multiple payload refs populated (expected 1, found {})",
                    count
                )
            }
        }
    }
}

impl std::error::Error for IngestionError {}

/// Validate an evidence envelope for ingestion (PR-005).
///
/// Performs comprehensive validation including:
/// 1. Schema version check
/// 2. Tenant ID presence
/// 3. Payload presence and scope matching
/// 4. Root hash recomputation and verification
/// 5. Signature validation (if required)
/// 6. Key ID verification (if public key present)
/// 7. Timestamp sanity check
///
/// Does NOT check chain linkage (requires DB state) - use `ingestion_validate_chain_linkage`
/// separately for that.
pub fn validate_for_ingestion(
    envelope: &EvidenceEnvelope,
    require_signature: bool,
    max_timestamp_skew_secs: i64,
) -> IngestionValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Schema version check
    if envelope.schema_version > EVIDENCE_ENVELOPE_SCHEMA_VERSION {
        errors.push(IngestionError::UnsupportedSchema {
            version: envelope.schema_version,
            max_supported: EVIDENCE_ENVELOPE_SCHEMA_VERSION,
        });
    }

    // 2. Tenant ID presence
    if envelope.tenant_id.is_empty() {
        errors.push(IngestionError::EmptyTenantId);
    }

    // 3. Verify exactly one payload ref is populated
    let payload_count = [
        envelope.bundle_metadata_ref.is_some(),
        envelope.policy_audit_ref.is_some(),
        envelope.inference_receipt_ref.is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if payload_count != 1 {
        if payload_count == 0 {
            errors.push(IngestionError::PayloadMissing {
                scope: envelope.scope.as_str().to_string(),
            });
        } else {
            errors.push(IngestionError::MultiplePayloads {
                count: payload_count,
            });
        }
    }

    // 4. Verify payload matches scope
    let payload_scope = detect_payload_scope(envelope);
    match payload_scope {
        Some(scope) if scope != envelope.scope => {
            errors.push(IngestionError::PayloadScopeMismatch {
                claimed: envelope.scope.as_str().to_string(),
                present: scope.as_str().to_string(),
            });
        }
        None if payload_count > 0 => {
            // Payload exists but scope detection failed - shouldn't happen
            warnings.push("Payload present but scope detection failed".to_string());
        }
        _ => {}
    }

    // 5. Recompute and verify root hash
    let computed_root = compute_envelope_root(envelope);
    if let Some(ref computed) = computed_root {
        if computed != &envelope.root {
            errors.push(IngestionError::RootMismatch {
                claimed: envelope.root.to_hex(),
                computed: computed.to_hex(),
            });
        }
    }

    // 6. Signature validation
    if require_signature {
        if envelope.signature.is_empty() {
            errors.push(IngestionError::SignatureMissing);
        } else {
            // Signature verification requires ed25519 library
            // For now, just verify signature is valid hex of correct length
            match hex::decode(&envelope.signature) {
                Ok(sig_bytes) if sig_bytes.len() != 64 => {
                    errors.push(IngestionError::SignatureInvalid {
                        reason: format!("Signature must be 64 bytes, got {}", sig_bytes.len()),
                    });
                }
                Err(e) => {
                    errors.push(IngestionError::SignatureInvalid {
                        reason: format!("Invalid hex encoding: {}", e),
                    });
                }
                _ => {
                    // Valid format; cryptographic verification would go here
                    // For now, emit warning that crypto verification is not yet implemented
                    warnings.push(
                        "Signature format valid; cryptographic verification pending".to_string(),
                    );
                }
            }
        }
    }

    // 7. Key ID verification (if public key present)
    if !envelope.public_key.is_empty() && !envelope.key_id.is_empty() {
        let computed_key_id = compute_key_id_from_pubkey_hex(&envelope.public_key);
        if computed_key_id != envelope.key_id {
            errors.push(IngestionError::KeyIdMismatch {
                computed: computed_key_id,
                claimed: envelope.key_id.clone(),
            });
        }
    }

    // 8. Timestamp sanity (not too far in future)
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&envelope.created_at) {
        let now = chrono::Utc::now();
        let max_future = now + chrono::Duration::seconds(max_timestamp_skew_secs);
        if ts > max_future {
            errors.push(IngestionError::TimestampInFuture {
                timestamp: envelope.created_at.clone(),
            });
        }
    }

    IngestionValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
        computed_root,
    }
}

/// Detect which payload reference is present in the envelope.
fn detect_payload_scope(envelope: &EvidenceEnvelope) -> Option<EvidenceScope> {
    if envelope.bundle_metadata_ref.is_some() {
        Some(EvidenceScope::Telemetry)
    } else if envelope.policy_audit_ref.is_some() {
        Some(EvidenceScope::Policy)
    } else if envelope.inference_receipt_ref.is_some() {
        Some(EvidenceScope::Inference)
    } else {
        None
    }
}

/// Compute envelope root from payload reference (standalone function).
pub fn compute_envelope_root(envelope: &EvidenceEnvelope) -> Option<B3Hash> {
    match envelope.scope {
        EvidenceScope::Telemetry => envelope
            .bundle_metadata_ref
            .as_ref()
            .map(|r| B3Hash::hash_multi(&[r.bundle_hash.as_bytes(), r.merkle_root.as_bytes()])),
        EvidenceScope::Policy => envelope.policy_audit_ref.as_ref().map(|r| r.entry_hash),
        EvidenceScope::Inference => envelope
            .inference_receipt_ref
            .as_ref()
            .map(|r| r.receipt_digest),
    }
}

/// Compute key ID from public key hex string.
///
/// Key ID is "key-" + first 32 hex characters of BLAKE3(pubkey).
fn compute_key_id_from_pubkey_hex(pubkey_hex: &str) -> String {
    if let Ok(bytes) = hex::decode(pubkey_hex) {
        let hash = B3Hash::hash(&bytes);
        // Key ID is "key-" + first 32 hex chars (16 bytes / 128 bits)
        format!("key-{}", &hash.to_hex()[..32])
    } else {
        String::new()
    }
}

/// Validate chain linkage against known chain tail (PR-005).
///
/// This is separate from `validate_for_ingestion` because it requires
/// database state (the current chain tail).
///
/// # Arguments
/// * `envelope` - The envelope being validated
/// * `chain_tail` - The current chain tail (root, sequence) or None if chain is empty
///
/// # Returns
/// * `None` if linkage is valid
/// * `Some(error)` if linkage is invalid
pub fn ingestion_validate_chain_linkage(
    envelope: &EvidenceEnvelope,
    chain_tail: Option<(B3Hash, i64)>,
) -> Option<IngestionError> {
    match (&chain_tail, &envelope.previous_root) {
        // First envelope: must have no previous_root
        (None, Some(claimed)) => Some(IngestionError::ChainLinkageInvalid {
            reason: format!(
                "First envelope in chain cannot have previous_root (claimed: {})",
                claimed.to_hex()
            ),
        }),

        // Subsequent envelope: must reference tail root
        (Some((tail_root, _)), None) => Some(IngestionError::PreviousRootMismatch {
            expected: Some(tail_root.to_hex()),
            claimed: None,
        }),

        (Some((tail_root, _)), Some(claimed)) if tail_root != claimed => {
            Some(IngestionError::PreviousRootMismatch {
                expected: Some(tail_root.to_hex()),
                claimed: Some(claimed.to_hex()),
            })
        }

        // Valid: first envelope with no previous, or correct linkage
        _ => None,
    }
}

/// Categorize errors for metrics labeling.
pub fn categorize_ingestion_errors(errors: &[IngestionError]) -> &'static str {
    if errors
        .iter()
        .any(|e| matches!(e, IngestionError::RootMismatch { .. }))
    {
        "root_mismatch"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::SignatureInvalid { .. }))
    {
        "signature_invalid"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::SignatureMissing))
    {
        "signature_missing"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::PayloadMissing { .. }))
    {
        "payload_missing"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::PayloadScopeMismatch { .. }))
    {
        "scope_mismatch"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::ChainLinkageInvalid { .. }))
    {
        "chain_linkage"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::PreviousRootMismatch { .. }))
    {
        "previous_root"
    } else if errors
        .iter()
        .any(|e| matches!(e, IngestionError::UnsupportedSchema { .. }))
    {
        "unsupported_schema"
    } else {
        "other"
    }
}

// =============================================================================
// Original Evidence Verifier Types
// =============================================================================

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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        envelope: &EvidenceEnvelope,
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
    pub fn verify_chain(&self, envelopes: &[EvidenceEnvelope]) -> Result<ChainVerificationResult> {
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
    fn compute_envelope_root(&self, envelope: &EvidenceEnvelope) -> Result<B3Hash> {
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
    fn verify_signature(&self, envelope: &EvidenceEnvelope) -> Result<bool> {
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
    fn verify_signature(&self, _envelope: &EvidenceEnvelope) -> Result<bool> {
        // Without the feature, skip signature verification
        Ok(true)
    }
}

/// Sign an envelope with Ed25519 (requires `evidence-signing` feature)
#[cfg(feature = "evidence-signing")]
pub fn sign_envelope(envelope: &mut EvidenceEnvelope, signing_key_hex: &str) -> Result<()> {
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
            backend_used: "metal".to_string(),
            backend_attestation_b3: Some(B3Hash::hash(b"metal-attestation")),
            seed_lineage_hash: None, // PRD-DET-001: PR-A
            adapter_training_lineage_digest: None,
        }
    }

    #[test]
    fn test_verify_telemetry_envelope() {
        let verifier = EvidenceVerifier::new();
        let env =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

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
        let env = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

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
            EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(), None);

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
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let result = verifier.verify_envelope(&telemetry_env, None).unwrap();
        assert!(result.is_valid, "Telemetry envelope should verify");

        // Policy
        let policy_env =
            EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);
        let result = verifier.verify_envelope(&policy_env, None).unwrap();
        assert!(result.is_valid, "Policy envelope should verify");

        // Inference
        let inference_env =
            EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(), None);
        let result = verifier.verify_envelope(&inference_env, None).unwrap();
        assert!(result.is_valid, "Inference envelope should verify");
    }

    /// AC-2: test_evidence_chain_linking_break_emits_divergence
    #[test]
    fn test_evidence_chain_linking_break_emits_divergence() {
        let verifier = EvidenceVerifier::new();

        // Create chain of 3 envelopes
        let env1 =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let env2 = EvidenceEnvelope::new_telemetry(
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
        let env3 = EvidenceEnvelope::new_telemetry(
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
            EvidenceEnvelope::new_inference("tenant-1".to_string(), receipt_ref.clone(), None);

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

        let env1 = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

        let env2 = EvidenceEnvelope::new_policy(
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
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
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

    // =========================================================================
    // PR-005 Ingestion Validation Tests
    // =========================================================================

    #[test]
    fn test_ingestion_valid_telemetry_envelope() {
        let envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(
            result.is_valid,
            "Valid envelope should pass: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
        assert!(result.computed_root.is_some());
    }

    #[test]
    fn test_ingestion_valid_policy_envelope() {
        let envelope =
            EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(
            result.is_valid,
            "Valid envelope should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_ingestion_valid_inference_envelope() {
        let envelope =
            EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(), None);

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(
            result.is_valid,
            "Valid envelope should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_ingestion_root_mismatch() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.root = B3Hash::hash(b"wrong-root");

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::RootMismatch { .. })));
    }

    #[test]
    fn test_ingestion_missing_payload() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.bundle_metadata_ref = None;

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::PayloadMissing { .. })));
    }

    #[test]
    fn test_ingestion_scope_mismatch() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.scope = EvidenceScope::Inference; // Wrong scope

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::PayloadScopeMismatch { .. })));
    }

    #[test]
    fn test_ingestion_empty_tenant_id() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.tenant_id = String::new();

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::EmptyTenantId)));
    }

    #[test]
    fn test_ingestion_signature_required_missing() {
        let envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let result = validate_for_ingestion(&envelope, true, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::SignatureMissing)));
    }

    #[test]
    fn test_ingestion_unsupported_schema() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.schema_version = 99;

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::UnsupportedSchema { .. })));
    }

    #[test]
    fn test_ingestion_timestamp_in_future() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let future = chrono::Utc::now() + chrono::Duration::hours(1);
        envelope.created_at = future.to_rfc3339();

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::TimestampInFuture { .. })));
    }

    #[test]
    fn test_ingestion_key_id_mismatch() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.public_key = hex::encode([1u8; 32]);
        envelope.key_id = "key-wrong".to_string();

        let result = validate_for_ingestion(&envelope, false, 300);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, IngestionError::KeyIdMismatch { .. })));
    }

    #[test]
    fn test_ingestion_chain_linkage_first_envelope_valid() {
        let envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let error = ingestion_validate_chain_linkage(&envelope, None);
        assert!(error.is_none(), "First envelope should be valid");
    }

    #[test]
    fn test_ingestion_chain_linkage_first_envelope_with_previous_invalid() {
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.previous_root = Some(B3Hash::hash(b"unexpected"));

        let error = ingestion_validate_chain_linkage(&envelope, None);
        assert!(matches!(
            error,
            Some(IngestionError::ChainLinkageInvalid { .. })
        ));
    }

    #[test]
    fn test_ingestion_chain_linkage_subsequent_envelope_valid() {
        let tail_root = B3Hash::hash(b"tail");
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.previous_root = Some(tail_root);

        let error = ingestion_validate_chain_linkage(&envelope, Some((tail_root, 1)));
        assert!(error.is_none(), "Correct linkage should be valid");
    }

    #[test]
    fn test_ingestion_chain_linkage_subsequent_envelope_missing_previous() {
        let tail_root = B3Hash::hash(b"tail");
        let envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        let error = ingestion_validate_chain_linkage(&envelope, Some((tail_root, 1)));
        assert!(matches!(
            error,
            Some(IngestionError::PreviousRootMismatch { .. })
        ));
    }

    #[test]
    fn test_ingestion_chain_linkage_subsequent_envelope_wrong_previous() {
        let tail_root = B3Hash::hash(b"tail");
        let mut envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        envelope.previous_root = Some(B3Hash::hash(b"wrong"));

        let error = ingestion_validate_chain_linkage(&envelope, Some((tail_root, 1)));
        assert!(matches!(
            error,
            Some(IngestionError::PreviousRootMismatch { .. })
        ));
    }

    #[test]
    fn test_categorize_ingestion_errors() {
        assert_eq!(
            categorize_ingestion_errors(&[IngestionError::RootMismatch {
                claimed: "a".into(),
                computed: "b".into()
            }]),
            "root_mismatch"
        );
        assert_eq!(
            categorize_ingestion_errors(&[IngestionError::SignatureInvalid {
                reason: "test".into()
            }]),
            "signature_invalid"
        );
        assert_eq!(
            categorize_ingestion_errors(&[IngestionError::SignatureMissing]),
            "signature_missing"
        );
        assert_eq!(
            categorize_ingestion_errors(&[IngestionError::PayloadMissing {
                scope: "telemetry".into()
            }]),
            "payload_missing"
        );
        assert_eq!(
            categorize_ingestion_errors(&[IngestionError::EmptyTenantId]),
            "other"
        );
    }

    #[test]
    fn test_compute_envelope_root_telemetry() {
        let envelope =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let computed = compute_envelope_root(&envelope);
        assert!(computed.is_some());
        assert_eq!(computed.unwrap(), envelope.root);
    }

    #[test]
    fn test_compute_envelope_root_policy() {
        let envelope =
            EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);
        let computed = compute_envelope_root(&envelope);
        assert!(computed.is_some());
        assert_eq!(computed.unwrap(), envelope.root);
    }

    #[test]
    fn test_compute_envelope_root_inference() {
        let envelope =
            EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(), None);
        let computed = compute_envelope_root(&envelope);
        assert!(computed.is_some());
        assert_eq!(computed.unwrap(), envelope.root);
    }

    #[test]
    fn test_ingestion_error_display() {
        let err = IngestionError::RootMismatch {
            claimed: "abc".to_string(),
            computed: "xyz".to_string(),
        };
        assert!(err.to_string().contains("Root mismatch"));

        let err = IngestionError::SignatureMissing;
        assert!(err.to_string().contains("Signature required"));
    }
}

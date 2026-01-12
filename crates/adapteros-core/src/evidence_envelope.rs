//! Unified evidence envelope for telemetry, policy audit, and inference traces.
//!
//! EvidenceEnvelope provides a canonical, chain-linked, signed container
//! that wraps all evidence types with a single verifiable format.
//!
//! # Design
//!
//! - **Digest-only payloads**: Envelopes contain references (hashes) to underlying data,
//!   not the data itself. This keeps envelopes small and streaming-friendly.
//! - **Chain linking**: Each envelope references the previous envelope's root via `previous_root`,
//!   forming a tamper-evident audit chain per tenant and scope.
//! - **Canonical bytes**: Deterministic serialization using big-endian integers and
//!   length-prefixed UTF-8 strings for consistent hashing.
//!
//! # Example
//!
//! ```ignore
//! use adapteros_core::evidence_envelope::{EvidenceEnvelope, EvidenceScope, InferenceReceiptRef};
//!
//! let receipt_ref = InferenceReceiptRef { /* ... */ };
//! let envelope = EvidenceEnvelope::new_inference(
//!     "tenant-1".to_string(),
//!     receipt_ref,
//!     None, // first in chain
//! );
//! let canonical = envelope.to_canonical_bytes();
//! let digest = envelope.digest();
//! ```

use crate::{B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

/// Schema version for forward compatibility
///
/// Version history:
/// - v1: Initial schema with telemetry, policy, inference scopes
/// - v2: Added backend_used and backend_attestation_b3 to InferenceReceiptRef (PRD-DET-001)
/// - v3: Added seed_lineage_hash to InferenceReceiptRef (PRD-DET-001: determinism hardening)
/// - v4: Added adapter_training_lineage_digest for training data provenance (patent rectification)
pub const EVIDENCE_ENVELOPE_SCHEMA_VERSION: u8 = 4;

/// Evidence scope discriminator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceScope {
    /// Telemetry bundle evidence (from BundleMetadata)
    Telemetry,
    /// Policy audit chain evidence (from PolicyAuditDecision)
    Policy,
    /// Inference trace receipt evidence (from TraceReceipt)
    Inference,
}

impl EvidenceScope {
    /// Convert scope to string for storage
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceScope::Telemetry => "telemetry",
            EvidenceScope::Policy => "policy",
            EvidenceScope::Inference => "inference",
        }
    }

    /// Parse scope from string
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "telemetry" => Some(EvidenceScope::Telemetry),
            "policy" => Some(EvidenceScope::Policy),
            "inference" => Some(EvidenceScope::Inference),
            _ => None,
        }
    }

    /// Scope tag byte for canonical encoding
    fn tag(&self) -> u8 {
        match self {
            EvidenceScope::Telemetry => 0,
            EvidenceScope::Policy => 1,
            EvidenceScope::Inference => 2,
        }
    }
}

/// Reference to telemetry bundle (digest-only)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleMetadataRef {
    /// Hash of the bundle file
    pub bundle_hash: B3Hash,
    /// Merkle root over all events in the bundle
    pub merkle_root: B3Hash,
    /// Number of events in the bundle
    pub event_count: u32,
    /// Control plane ID (optional)
    pub cpid: Option<String>,
    /// Bundle sequence number within the CPID
    pub sequence_no: Option<u64>,
}

/// Reference to policy audit decision (digest-only)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyAuditRef {
    /// Decision ID
    pub decision_id: String,
    /// Entry hash (BLAKE3 of decision fields)
    pub entry_hash: B3Hash,
    /// Chain sequence number
    pub chain_sequence: i64,
    /// Policy pack ID
    pub policy_pack_id: String,
    /// Hook point (e.g., "OnBeforeInference")
    pub hook: String,
    /// Decision result ("allow" or "deny")
    pub decision: String,
}

/// Reference to inference trace receipt (digest-only)
///
/// Contains all fields from RunReceipt for complete evidence binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferenceReceiptRef {
    /// Trace ID
    pub trace_id: String,
    /// Hash chain head over per-token decisions
    pub run_head_hash: B3Hash,
    /// Hash of output token sequence
    pub output_digest: B3Hash,
    /// Final receipt digest (hash of all receipt fields)
    pub receipt_digest: B3Hash,

    // --- Token accounting ---
    /// Total logical prompt tokens before cache reuse
    pub logical_prompt_tokens: u32,
    /// Tokens satisfied by prefix cache reuse
    pub prefix_cached_token_count: u32,
    /// Billed input tokens (logical - cached, floored at 0)
    pub billed_input_tokens: u32,
    /// Tokens produced logically (excludes eos)
    pub logical_output_tokens: u32,
    /// Billed output tokens (v1 = logical output tokens)
    pub billed_output_tokens: u32,

    // --- Stop controller ---
    /// Stop reason code explaining why generation terminated
    pub stop_reason_code: Option<String>,
    /// Token index at which the stop decision was made
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used
    pub stop_policy_digest_b3: Option<B3Hash>,

    // --- Model identity ---
    /// BLAKE3 digest of ModelCacheIdentityV2 canonical bytes
    pub model_cache_identity_v2_digest_b3: Option<B3Hash>,

    // --- Backend identity (PRD-DET-001: Determinism hardening) ---
    /// Backend used for inference (e.g., "metal", "coreml", "mlx").
    ///
    /// This field binds the receipt to the specific backend that executed
    /// the inference, ensuring backend substitution is detectable in the
    /// tamper-evident evidence chain.
    #[serde(default)]
    pub backend_used: String,

    /// BLAKE3 hash of backend attestation report for integrity verification.
    ///
    /// Computed from the `DeterminismReport` canonical bytes, including:
    /// - metallib_hash (for Metal backend)
    /// - rng_seed_method
    /// - floating_point_mode
    /// - determinism_level
    /// - compiler_flags
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_attestation_b3: Option<B3Hash>,

    // --- Seed lineage (PRD-DET-001: Determinism hardening v3) ---
    /// BLAKE3 hash of seed lineage for replay verification.
    ///
    /// Computed from `SeedLineage::to_binding_hash()`, which includes:
    /// - HKDF algorithm version
    /// - Root seed digest
    /// - Seed mode (Strict/BestEffort/NonDeterministic)
    /// - Manifest binding flag
    ///
    /// This field enables detection of seed manipulation during replay:
    /// replay with different seed → different receipt digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_lineage_hash: Option<B3Hash>,

    // --- Adapter training lineage (Patent rectification: v4) ---
    /// BLAKE3 digest of adapter training lineage for data provenance verification.
    ///
    /// Computed from the combined hash of all adapters used in this inference,
    /// including each adapter's training_dataset_hash_b3. Enables proof that
    /// "this output came from adapter trained on dataset X".
    ///
    /// Formula: BLAKE3(adapter_id_1 || training_hash_1 || adapter_id_2 || training_hash_2 || ...)
    /// Adapters are sorted by adapter_id for determinism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_training_lineage_digest: Option<B3Hash>,
}

/// Unified evidence envelope with cryptographic chain linking
///
/// All evidence types (telemetry, policy, inference) are wrapped in this
/// canonical envelope format for unified verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEnvelope {
    /// Schema version for forward compatibility (currently 1)
    pub schema_version: u8,

    /// Tenant ID for isolation (required)
    pub tenant_id: String,

    /// Evidence scope discriminator
    pub scope: EvidenceScope,

    /// Reference to previous envelope's root (chain linking)
    /// None for first envelope in chain
    pub previous_root: Option<B3Hash>,

    /// Root hash for this envelope (computed from payload)
    pub root: B3Hash,

    /// Ed25519 signature over canonical envelope bytes (64 bytes, hex-encoded)
    pub signature: String,

    /// Public key used for signing (hex-encoded, 64 chars)
    pub public_key: String,

    /// Key ID: first 8 bytes of blake3(pubkey) hex-encoded
    pub key_id: String,

    /// Reference to external attestation (optional, e.g., Secure Enclave)
    pub attestation_ref: Option<String>,

    /// Envelope creation timestamp (RFC3339)
    pub created_at: String,

    /// Signature timestamp (microseconds since epoch)
    pub signed_at_us: u64,

    // --- Scope-specific payload references (exactly one populated) ---
    /// For Telemetry scope: reference to BundleMetadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_metadata_ref: Option<BundleMetadataRef>,

    /// For Policy scope: reference to PolicyAuditDecision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_audit_ref: Option<PolicyAuditRef>,

    /// For Inference scope: reference to TraceReceipt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_receipt_ref: Option<InferenceReceiptRef>,
}

impl EvidenceEnvelope {
    /// Create a new telemetry evidence envelope (unsigned)
    pub fn new_telemetry(
        tenant_id: String,
        bundle_ref: BundleMetadataRef,
        previous_root: Option<B3Hash>,
    ) -> Self {
        let root = Self::compute_telemetry_root(&bundle_ref);
        Self {
            schema_version: EVIDENCE_ENVELOPE_SCHEMA_VERSION,
            tenant_id,
            scope: EvidenceScope::Telemetry,
            previous_root,
            root,
            signature: String::new(),
            public_key: String::new(),
            key_id: String::new(),
            attestation_ref: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            signed_at_us: 0,
            bundle_metadata_ref: Some(bundle_ref),
            policy_audit_ref: None,
            inference_receipt_ref: None,
        }
    }

    /// Create a new policy evidence envelope (unsigned)
    pub fn new_policy(
        tenant_id: String,
        policy_ref: PolicyAuditRef,
        previous_root: Option<B3Hash>,
    ) -> Self {
        let root = policy_ref.entry_hash;
        Self {
            schema_version: EVIDENCE_ENVELOPE_SCHEMA_VERSION,
            tenant_id,
            scope: EvidenceScope::Policy,
            previous_root,
            root,
            signature: String::new(),
            public_key: String::new(),
            key_id: String::new(),
            attestation_ref: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            signed_at_us: 0,
            bundle_metadata_ref: None,
            policy_audit_ref: Some(policy_ref),
            inference_receipt_ref: None,
        }
    }

    /// Create a new inference evidence envelope (unsigned)
    pub fn new_inference(
        tenant_id: String,
        receipt_ref: InferenceReceiptRef,
        previous_root: Option<B3Hash>,
    ) -> Self {
        let root = receipt_ref.receipt_digest;
        Self {
            schema_version: EVIDENCE_ENVELOPE_SCHEMA_VERSION,
            tenant_id,
            scope: EvidenceScope::Inference,
            previous_root,
            root,
            signature: String::new(),
            public_key: String::new(),
            key_id: String::new(),
            attestation_ref: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            signed_at_us: 0,
            bundle_metadata_ref: None,
            policy_audit_ref: None,
            inference_receipt_ref: Some(receipt_ref),
        }
    }

    /// Compute root hash for telemetry bundle
    fn compute_telemetry_root(bundle_ref: &BundleMetadataRef) -> B3Hash {
        B3Hash::hash_multi(&[
            bundle_ref.bundle_hash.as_bytes(),
            bundle_ref.merkle_root.as_bytes(),
        ])
    }

    /// Serialize into canonical bytes for hashing and signing.
    ///
    /// Uses big-endian integers and length-prefixed UTF-8 strings
    /// following the pattern from `ContextManifestV1::to_bytes()`.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(512);

        // Schema version (1 byte)
        bytes.push(self.schema_version);

        // Tenant ID (length-prefixed string)
        encode_str(&mut bytes, &self.tenant_id);

        // Scope (1 byte enum tag)
        bytes.push(self.scope.tag());

        // Previous root (32 bytes or 32 zero bytes if None)
        match &self.previous_root {
            Some(hash) => bytes.extend_from_slice(hash.as_bytes()),
            None => bytes.extend_from_slice(&[0u8; 32]),
        }

        // Root (32 bytes)
        bytes.extend_from_slice(self.root.as_bytes());

        // Created at timestamp (length-prefixed)
        encode_str(&mut bytes, &self.created_at);

        // Signed at (8 bytes big-endian)
        bytes.extend_from_slice(&self.signed_at_us.to_be_bytes());

        // Attestation ref (length-prefixed, empty string if None)
        encode_str(&mut bytes, self.attestation_ref.as_deref().unwrap_or(""));

        // Scope-specific payload bytes
        self.encode_scope_payload(&mut bytes);

        bytes
    }

    fn encode_scope_payload(&self, bytes: &mut Vec<u8>) {
        match self.scope {
            EvidenceScope::Telemetry => {
                if let Some(ref r) = self.bundle_metadata_ref {
                    bytes.extend_from_slice(r.bundle_hash.as_bytes());
                    bytes.extend_from_slice(r.merkle_root.as_bytes());
                    encode_u32(bytes, r.event_count);
                    encode_str(bytes, r.cpid.as_deref().unwrap_or(""));
                    encode_u64(bytes, r.sequence_no.unwrap_or(0));
                }
            }
            EvidenceScope::Policy => {
                if let Some(ref r) = self.policy_audit_ref {
                    encode_str(bytes, &r.decision_id);
                    bytes.extend_from_slice(r.entry_hash.as_bytes());
                    encode_i64(bytes, r.chain_sequence);
                    encode_str(bytes, &r.policy_pack_id);
                    encode_str(bytes, &r.hook);
                    encode_str(bytes, &r.decision);
                }
            }
            EvidenceScope::Inference => {
                if let Some(ref r) = self.inference_receipt_ref {
                    encode_str(bytes, &r.trace_id);
                    bytes.extend_from_slice(r.run_head_hash.as_bytes());
                    bytes.extend_from_slice(r.output_digest.as_bytes());
                    bytes.extend_from_slice(r.receipt_digest.as_bytes());

                    // Token accounting
                    encode_u32(bytes, r.logical_prompt_tokens);
                    encode_u32(bytes, r.prefix_cached_token_count);
                    encode_u32(bytes, r.billed_input_tokens);
                    encode_u32(bytes, r.logical_output_tokens);
                    encode_u32(bytes, r.billed_output_tokens);

                    // Stop controller
                    encode_str(bytes, r.stop_reason_code.as_deref().unwrap_or(""));
                    encode_u32(bytes, r.stop_reason_token_index.unwrap_or(u32::MAX));
                    match &r.stop_policy_digest_b3 {
                        Some(h) => bytes.extend_from_slice(h.as_bytes()),
                        None => bytes.extend_from_slice(&[0u8; 32]),
                    }

                    // Model identity
                    match &r.model_cache_identity_v2_digest_b3 {
                        Some(h) => bytes.extend_from_slice(h.as_bytes()),
                        None => bytes.extend_from_slice(&[0u8; 32]),
                    }

                    // Backend identity (PRD-DET-001: v2 schema addition)
                    encode_str(bytes, &r.backend_used);
                    match &r.backend_attestation_b3 {
                        Some(h) => {
                            bytes.push(1); // present marker
                            bytes.extend_from_slice(h.as_bytes());
                        }
                        None => {
                            bytes.push(0); // absent marker
                        }
                    }

                    // Seed lineage (PRD-DET-001: v3 schema addition)
                    match &r.seed_lineage_hash {
                        Some(h) => {
                            bytes.push(1); // present marker
                            bytes.extend_from_slice(h.as_bytes());
                        }
                        None => {
                            bytes.push(0); // absent marker
                        }
                    }
                }
            }
        }
    }

    /// Compute BLAKE3 digest of canonical bytes (excludes signature fields)
    pub fn digest(&self) -> B3Hash {
        B3Hash::hash(&self.to_canonical_bytes())
    }

    /// Check if envelope has been signed
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty() && !self.public_key.is_empty()
    }

    /// Validate envelope structure (not cryptographic verification)
    pub fn validate(&self) -> Result<()> {
        use crate::AosError;

        if self.schema_version != EVIDENCE_ENVELOPE_SCHEMA_VERSION {
            return Err(AosError::Validation(format!(
                "Unsupported schema version: expected {}, got {}",
                EVIDENCE_ENVELOPE_SCHEMA_VERSION, self.schema_version
            )));
        }

        if self.tenant_id.is_empty() {
            return Err(AosError::Validation("tenant_id is required".to_string()));
        }

        // Verify exactly one payload ref is populated
        let payload_count = [
            self.bundle_metadata_ref.is_some(),
            self.policy_audit_ref.is_some(),
            self.inference_receipt_ref.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if payload_count != 1 {
            return Err(AosError::Validation(format!(
                "Exactly one payload ref must be populated, found {}",
                payload_count
            )));
        }

        // Verify scope matches payload
        match self.scope {
            EvidenceScope::Telemetry if self.bundle_metadata_ref.is_none() => {
                return Err(AosError::Validation(
                    "Telemetry scope requires bundle_metadata_ref".to_string(),
                ));
            }
            EvidenceScope::Policy if self.policy_audit_ref.is_none() => {
                return Err(AosError::Validation(
                    "Policy scope requires policy_audit_ref".to_string(),
                ));
            }
            EvidenceScope::Inference if self.inference_receipt_ref.is_none() => {
                return Err(AosError::Validation(
                    "Inference scope requires inference_receipt_ref".to_string(),
                ));
            }
            _ => {}
        }

        Ok(())
    }
}

/// Compute key ID from public key bytes
///
/// Key ID is the first 32 hex characters (16 bytes / 128 bits) of blake3(pubkey).
/// This provides sufficient entropy to avoid birthday-bound collisions (~2^64 keys).
pub fn compute_key_id(pubkey_bytes: &[u8]) -> String {
    let hash = B3Hash::hash(pubkey_bytes);
    format!("key-{}", &hash.to_hex()[..32])
}

// --- Canonical encoding helpers (following context_manifest.rs pattern) ---

fn encode_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn encode_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn encode_i64(buf: &mut Vec<u8>, value: i64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn encode_str(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let len = u32::try_from(bytes.len()).expect("string length fits in u32");
    encode_u32(buf, len);
    buf.extend_from_slice(bytes);
}

// =============================================================================
// EP-5: Receipt Finalization Completeness Check (PRD-DET-001)
// =============================================================================

/// Report of missing fields in inference receipt.
///
/// Used by EP-5 enforcement point to report incomplete receipts.
#[derive(Debug, Clone)]
pub struct ReceiptCompletenessReport {
    /// Fields that are missing
    pub missing_fields: Vec<String>,
    /// Whether the receipt is complete for determinism
    pub is_complete: bool,
}

impl InferenceReceiptRef {
    /// Validate receipt completeness for determinism (PRD-DET-001: EP-5).
    ///
    /// This method checks that all determinism-critical fields are populated
    /// before the receipt is finalized. It is called at enforcement point EP-5.
    ///
    /// # Required Fields for Determinism
    ///
    /// - `backend_used`: Must be non-empty
    /// - `backend_attestation_b3`: Must be present for verified determinism
    /// - `seed_lineage_hash`: Must be present for replay verification
    ///
    /// # Enforcement Point: EP-5
    ///
    /// Location: `adapteros-core/src/evidence_envelope.rs:validate_completeness`
    /// Action: Emit incomplete_receipt telemetry event if fields missing
    ///
    /// # Returns
    ///
    /// `ReceiptCompletenessReport` with list of missing fields
    pub fn validate_completeness(&self) -> ReceiptCompletenessReport {
        let mut missing_fields = Vec::new();

        // Check backend_used (required since v2)
        if self.backend_used.is_empty() {
            missing_fields.push("backend_used".to_string());
        }

        // Check backend_attestation_b3 (required for verified determinism)
        if self.backend_attestation_b3.is_none() {
            missing_fields.push("backend_attestation_b3".to_string());
        }

        // Check seed_lineage_hash (required since v3 for replay verification)
        if self.seed_lineage_hash.is_none() {
            missing_fields.push("seed_lineage_hash".to_string());
        }

        // Check output_digest (always required)
        if self.output_digest.as_bytes() == &[0u8; 32] {
            missing_fields.push("output_digest".to_string());
        }

        // Check receipt_digest (always required)
        if self.receipt_digest.as_bytes() == &[0u8; 32] {
            missing_fields.push("receipt_digest".to_string());
        }

        ReceiptCompletenessReport {
            is_complete: missing_fields.is_empty(),
            missing_fields,
        }
    }

    /// Validate receipt for strict determinism mode (PRD-DET-001: EP-5).
    ///
    /// In strict mode, incomplete receipts are a hard failure.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if receipt is complete
    /// * `Err(AosError::DeterminismViolation)` if fields are missing
    pub fn validate_for_strict_mode(&self) -> Result<()> {
        use crate::AosError;

        let report = self.validate_completeness();
        if !report.is_complete {
            return Err(AosError::DeterminismViolation(format!(
                "EP-5: Receipt incomplete for strict mode. Missing fields: {}",
                report.missing_fields.join(", ")
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_telemetry_envelope_creation() {
        let env =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        assert_eq!(env.scope, EvidenceScope::Telemetry);
        assert_eq!(env.tenant_id, "tenant-1");
        assert!(env.bundle_metadata_ref.is_some());
        assert!(env.policy_audit_ref.is_none());
        assert!(env.inference_receipt_ref.is_none());
        assert!(env.validate().is_ok());
    }

    #[test]
    fn test_policy_envelope_creation() {
        let env = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(), None);

        assert_eq!(env.scope, EvidenceScope::Policy);
        assert!(env.policy_audit_ref.is_some());
        assert!(env.validate().is_ok());
    }

    #[test]
    fn test_inference_envelope_creation() {
        let env =
            EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(), None);

        assert_eq!(env.scope, EvidenceScope::Inference);
        assert!(env.inference_receipt_ref.is_some());
        assert!(env.validate().is_ok());
    }

    #[test]
    fn test_canonical_bytes_determinism() {
        let env1 =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);

        // Create another envelope with same data
        let mut env2 =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        env2.created_at = env1.created_at.clone();

        assert_eq!(env1.to_canonical_bytes(), env2.to_canonical_bytes());
        assert_eq!(env1.digest(), env2.digest());
    }

    #[test]
    fn test_tenant_id_changes_digest() {
        let env1 =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let mut env2 =
            EvidenceEnvelope::new_telemetry("tenant-2".to_string(), sample_bundle_ref(), None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(env1.digest(), env2.digest());
    }

    #[test]
    fn test_previous_root_changes_digest() {
        let env1 =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        let mut env2 = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(),
            Some(B3Hash::hash(b"previous")),
        );
        env2.created_at = env1.created_at.clone();

        assert_ne!(env1.digest(), env2.digest());
    }

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
    }

    #[test]
    fn test_scope_as_str() {
        assert_eq!(EvidenceScope::Telemetry.as_str(), "telemetry");
        assert_eq!(EvidenceScope::Policy.as_str(), "policy");
        assert_eq!(EvidenceScope::Inference.as_str(), "inference");
    }

    #[test]
    fn test_scope_parse() {
        assert_eq!(
            EvidenceScope::parse("telemetry"),
            Some(EvidenceScope::Telemetry)
        );
        assert_eq!(EvidenceScope::parse("policy"), Some(EvidenceScope::Policy));
        assert_eq!(
            EvidenceScope::parse("inference"),
            Some(EvidenceScope::Inference)
        );
        assert_eq!(EvidenceScope::parse("invalid"), None);
    }

    #[test]
    fn test_compute_key_id() {
        let pubkey = [0u8; 32];
        let key_id = compute_key_id(&pubkey);
        assert!(key_id.starts_with("key-"));
        // "key-" (4 chars) + 32 hex chars (16 bytes / 128 bits)
        assert_eq!(key_id.len(), 4 + 32);
    }

    #[test]
    fn test_validation_rejects_empty_tenant() {
        let env = EvidenceEnvelope::new_telemetry("".to_string(), sample_bundle_ref(), None);

        assert!(env.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_wrong_schema_version() {
        let mut env =
            EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(), None);
        env.schema_version = 99;

        assert!(env.validate().is_err());
    }

    #[test]
    fn test_json_roundtrip() {
        let env = EvidenceEnvelope::new_inference(
            "tenant-1".to_string(),
            sample_inference_ref(),
            Some(B3Hash::hash(b"previous")),
        );

        let json = serde_json::to_string(&env).expect("serialize");
        let parsed: EvidenceEnvelope = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.tenant_id, env.tenant_id);
        assert_eq!(parsed.scope, env.scope);
        assert_eq!(parsed.root, env.root);
        assert_eq!(parsed.previous_root, env.previous_root);
    }

    // ==========================================================================
    // PRD-DET-001: Backend identity binding tests
    // ==========================================================================

    #[test]
    fn test_backend_used_changes_receipt_digest() {
        // PRD-DET-001: Different backend_used must produce different envelope digest
        let mut ref1 = sample_inference_ref();
        ref1.backend_used = "metal".to_string();

        let mut ref2 = sample_inference_ref();
        ref2.backend_used = "coreml".to_string();

        let env1 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref1, None);
        let mut env2 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref2, None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(
            env1.digest(),
            env2.digest(),
            "Different backend_used must produce different digest"
        );
    }

    #[test]
    fn test_backend_attestation_changes_receipt_digest() {
        // PRD-DET-001: Different backend attestation must produce different digest
        let mut ref1 = sample_inference_ref();
        ref1.backend_attestation_b3 = Some(B3Hash::hash(b"attestation-1"));

        let mut ref2 = sample_inference_ref();
        ref2.backend_attestation_b3 = Some(B3Hash::hash(b"attestation-2"));

        let env1 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref1, None);
        let mut env2 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref2, None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(
            env1.digest(),
            env2.digest(),
            "Different backend_attestation_b3 must produce different digest"
        );
    }

    #[test]
    fn test_backend_attestation_none_vs_some_changes_digest() {
        // PRD-DET-001: Presence vs absence of attestation must be distinguishable
        let mut ref1 = sample_inference_ref();
        ref1.backend_attestation_b3 = None;

        let mut ref2 = sample_inference_ref();
        ref2.backend_attestation_b3 = Some(B3Hash::hash(b"attestation"));

        let env1 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref1, None);
        let mut env2 = EvidenceEnvelope::new_inference("tenant-1".to_string(), ref2, None);
        env2.created_at = env1.created_at.clone();

        assert_ne!(
            env1.digest(),
            env2.digest(),
            "None vs Some attestation must produce different digest"
        );
    }

    #[test]
    fn test_v1_schema_backward_compat_deserialization() {
        // PRD-DET-001: Old v1 receipts without backend fields should deserialize
        // with empty defaults
        let json = r#"{
            "trace_id": "trace-old",
            "run_head_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "output_digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "receipt_digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "logical_prompt_tokens": 10,
            "prefix_cached_token_count": 0,
            "billed_input_tokens": 10,
            "logical_output_tokens": 5,
            "billed_output_tokens": 5
        }"#;

        let parsed: InferenceReceiptRef =
            serde_json::from_str(json).expect("v1 schema should deserialize");

        assert_eq!(parsed.backend_used, "", "backend_used defaults to empty");
        assert!(
            parsed.backend_attestation_b3.is_none(),
            "backend_attestation defaults to None"
        );
    }

    #[test]
    fn test_inference_receipt_includes_backend_fields() {
        // PRD-DET-001: Verify backend fields are present in receipt ref
        let receipt_ref = sample_inference_ref();
        let env =
            EvidenceEnvelope::new_inference("tenant-1".to_string(), receipt_ref.clone(), None);

        let ref_data = env.inference_receipt_ref.as_ref().unwrap();

        assert_eq!(ref_data.backend_used, "metal");
        assert!(ref_data.backend_attestation_b3.is_some());
    }
}

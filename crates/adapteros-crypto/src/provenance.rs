//! Provenance certificates for adapter lineage verification.
//!
//! A [`ProvenanceCertificate`] is a cryptographically signed record proving an
//! adapter's complete chain of custody — from training data through checkpoint,
//! promotion, and serving policy. All hashes are BLAKE3, all signatures Ed25519.
//!
//! # Certificate Lifecycle
//!
//! 1. Build with [`ProvenanceCertificateBuilder`], populating known lineage fields.
//! 2. Call [`ProvenanceCertificateBuilder::sign`] to finalize and sign.
//! 3. Distribute the certificate alongside the adapter bundle.
//! 4. Verifiers call [`ProvenanceCertificate::verify`] to check integrity.
//!
//! # Content Hash
//!
//! The `content_hash` is computed by serializing all certificate fields (except
//! `content_hash`, `signature`, and `signer_public_key`) to JCS (RFC 8785
//! canonical JSON), then BLAKE3 hashing the result. This ensures deterministic
//! verification regardless of JSON field ordering.

use crate::compute_key_id;
use crate::signature::{Keypair, PublicKey, Signature};
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Schema version for provenance certificates.
pub const PROVENANCE_SCHEMA_VERSION: u8 = 1;

/// A cryptographically signed certificate proving an adapter's complete lineage
/// from training data through to serving.
///
/// Each field captures a link in the chain of custody:
/// - Training data -> config -> checkpoint -> promotion -> serving policy
/// - All hashes are BLAKE3, all signatures are Ed25519
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceCertificate {
    /// Schema version for forward compatibility
    pub schema_version: u8,

    /// Unique certificate ID (`cert-{hex}`)
    pub certificate_id: String,

    /// Adapter this certificate covers
    pub adapter_id: String,

    /// Adapter version ID
    pub version_id: String,

    /// Tenant that owns this adapter
    pub tenant_id: String,

    // --- Training provenance ---
    /// BLAKE3 hash of the training dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_data_hash: Option<String>,

    /// BLAKE3 hash of the training configuration JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config_hash: Option<String>,

    /// Training job ID that produced this adapter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_job_id: Option<String>,

    /// Final training loss
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_final_loss: Option<f64>,

    /// Number of training epochs completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_epochs: Option<u32>,

    // --- Checkpoint provenance ---
    /// BLAKE3 hash of the trained weights checkpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_hash: Option<String>,

    /// Ed25519 signature of the checkpoint (hex-encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_signature: Option<String>,

    /// Public key that signed the checkpoint (hex-encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_signer_key: Option<String>,

    // --- Promotion provenance ---
    /// Human review ID that approved promotion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_review_id: Option<String>,

    /// Who approved the promotion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_by: Option<String>,

    /// When promotion was approved (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_at: Option<String>,

    /// State the adapter was promoted from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_from_state: Option<String>,

    /// State the adapter was promoted to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_to_state: Option<String>,

    // --- Serving provenance ---
    /// BLAKE3 hash of the policy pack applied during serving
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_pack_hash: Option<String>,

    /// Name/ID of the policy pack
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_pack_id: Option<String>,

    /// Base model ID this adapter targets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,

    // --- Egress attestation ---
    /// Whether egress was verified blocked at certificate generation time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub egress_blocked: Option<bool>,

    /// BLAKE3 fingerprint of firewall rules at cert time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub egress_rules_fingerprint: Option<String>,

    // --- Certificate metadata ---
    /// When this certificate was generated (ISO 8601)
    pub generated_at: String,

    /// BLAKE3 hash of all fields above (computed before signing)
    pub content_hash: String,

    /// Ed25519 signature over content_hash (hex-encoded)
    pub signature: String,

    /// Public key that signed this certificate (hex-encoded)
    pub signer_public_key: String,
}

/// Intermediate structure for canonical hashing.
///
/// Contains all certificate fields *except* `content_hash`, `signature`, and
/// `signer_public_key`, which are computed after hashing.
#[derive(Serialize)]
struct CertificateContent<'a> {
    schema_version: u8,
    certificate_id: &'a str,
    adapter_id: &'a str,
    version_id: &'a str,
    tenant_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    training_data_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    training_config_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    training_job_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    training_final_loss: &'a Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    training_epochs: &'a Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_signature: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_signer_key: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promotion_review_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_by: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_at: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_from_state: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promoted_to_state: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_pack_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_pack_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_model_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    egress_blocked: &'a Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    egress_rules_fingerprint: &'a Option<String>,
    generated_at: &'a str,
}

impl ProvenanceCertificate {
    /// Verify this certificate's Ed25519 signature over the content hash.
    ///
    /// Re-serializes the content fields to JCS canonical JSON, recomputes the
    /// BLAKE3 hash, checks it matches `content_hash`, then verifies the
    /// Ed25519 signature.
    pub fn verify(&self) -> Result<bool> {
        // Schema version gate
        if self.schema_version != PROVENANCE_SCHEMA_VERSION {
            return Err(AosError::Crypto(format!(
                "Unsupported provenance schema version: expected {}, got {}",
                PROVENANCE_SCHEMA_VERSION, self.schema_version
            )));
        }

        // Recompute content hash from certificate fields
        let recomputed = self.compute_content_hash()?;
        if recomputed != self.content_hash {
            return Ok(false);
        }

        // Decode signer public key
        let pk_bytes = hex::decode(&self.signer_public_key)
            .map_err(|e| AosError::Crypto(format!("Invalid signer public key hex: {}", e)))?;
        if pk_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid signer public key length: {}",
                pk_bytes.len()
            )));
        }
        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(&pk_bytes);
        let public_key = PublicKey::from_bytes(&pk_arr)?;

        // Decode signature
        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;
        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: {}",
                sig_bytes.len()
            )));
        }
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr)?;

        // Verify Ed25519 signature over the content hash bytes
        let hash_bytes = hex::decode(&self.content_hash)
            .map_err(|e| AosError::Crypto(format!("Invalid content hash hex: {}", e)))?;

        match public_key.verify(&hash_bytes, &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify and return a structured report.
    pub fn verify_report(&self) -> Result<ProvenanceVerifyReport> {
        let valid = self.verify()?;

        let signer_key_id = if valid {
            // Decode public key to compute key ID
            let pk_bytes = hex::decode(&self.signer_public_key)
                .map_err(|e| AosError::Crypto(format!("Invalid signer public key hex: {}", e)))?;
            let mut pk_arr = [0u8; 32];
            pk_arr.copy_from_slice(&pk_bytes);
            let pk = PublicKey::from_bytes(&pk_arr)?;
            compute_key_id(&pk)
        } else {
            String::from("invalid")
        };

        Ok(ProvenanceVerifyReport {
            valid,
            certificate_id: self.certificate_id.clone(),
            adapter_id: self.adapter_id.clone(),
            signer_key_id,
            generated_at: self.generated_at.clone(),
            chain_completeness: self.chain_completeness(),
        })
    }

    /// Compute chain completeness for this certificate.
    pub fn chain_completeness(&self) -> ChainCompleteness {
        let has_training_data = self.training_data_hash.is_some()
            || self.training_config_hash.is_some()
            || self.training_job_id.is_some();
        let has_checkpoint = self.checkpoint_hash.is_some();
        let has_promotion = self.promotion_review_id.is_some() || self.promoted_by.is_some();
        let has_policy = self.policy_pack_hash.is_some() || self.policy_pack_id.is_some();
        let has_egress_attestation = self.egress_blocked.is_some();

        let present = [
            has_training_data,
            has_checkpoint,
            has_promotion,
            has_policy,
            has_egress_attestation,
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        ChainCompleteness {
            has_training_data,
            has_checkpoint,
            has_promotion,
            has_policy,
            has_egress_attestation,
            completeness_score: present as f32 / 5.0,
        }
    }

    /// Recompute the canonical content hash from certificate fields.
    fn compute_content_hash(&self) -> Result<String> {
        let content = CertificateContent {
            schema_version: self.schema_version,
            certificate_id: &self.certificate_id,
            adapter_id: &self.adapter_id,
            version_id: &self.version_id,
            tenant_id: &self.tenant_id,
            training_data_hash: &self.training_data_hash,
            training_config_hash: &self.training_config_hash,
            training_job_id: &self.training_job_id,
            training_final_loss: &self.training_final_loss,
            training_epochs: &self.training_epochs,
            checkpoint_hash: &self.checkpoint_hash,
            checkpoint_signature: &self.checkpoint_signature,
            checkpoint_signer_key: &self.checkpoint_signer_key,
            promotion_review_id: &self.promotion_review_id,
            promoted_by: &self.promoted_by,
            promoted_at: &self.promoted_at,
            promoted_from_state: &self.promoted_from_state,
            promoted_to_state: &self.promoted_to_state,
            policy_pack_hash: &self.policy_pack_hash,
            policy_pack_id: &self.policy_pack_id,
            base_model_id: &self.base_model_id,
            egress_blocked: &self.egress_blocked,
            egress_rules_fingerprint: &self.egress_rules_fingerprint,
            generated_at: &self.generated_at,
        };

        let canonical = serde_jcs::to_vec(&content)
            .map_err(|e| AosError::Crypto(format!("JCS serialization failed: {}", e)))?;

        let hash = B3Hash::hash(&canonical);
        Ok(hash.to_hex())
    }
}

/// Structured verification report for a provenance certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceVerifyReport {
    /// Whether the certificate signature is valid
    pub valid: bool,
    /// Certificate ID
    pub certificate_id: String,
    /// Adapter ID covered by this certificate
    pub adapter_id: String,
    /// Key ID of the signer (`kid-{blake3(pubkey)[..32]}`)
    pub signer_key_id: String,
    /// When the certificate was generated (ISO 8601)
    pub generated_at: String,
    /// Completeness of the provenance chain
    pub chain_completeness: ChainCompleteness,
}

/// How complete the provenance chain is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCompleteness {
    pub has_training_data: bool,
    pub has_checkpoint: bool,
    pub has_promotion: bool,
    pub has_policy: bool,
    pub has_egress_attestation: bool,
    /// 0.0 (no provenance) to 1.0 (full chain)
    pub completeness_score: f32,
}

/// Builder for constructing and signing provenance certificates.
pub struct ProvenanceCertificateBuilder {
    adapter_id: String,
    version_id: String,
    tenant_id: String,
    training_data_hash: Option<String>,
    training_config_hash: Option<String>,
    training_job_id: Option<String>,
    training_final_loss: Option<f64>,
    training_epochs: Option<u32>,
    checkpoint_hash: Option<String>,
    checkpoint_signature: Option<String>,
    checkpoint_signer_key: Option<String>,
    promotion_review_id: Option<String>,
    promoted_by: Option<String>,
    promoted_at: Option<String>,
    promoted_from_state: Option<String>,
    promoted_to_state: Option<String>,
    policy_pack_hash: Option<String>,
    policy_pack_id: Option<String>,
    base_model_id: Option<String>,
    egress_blocked: Option<bool>,
    egress_rules_fingerprint: Option<String>,
}

impl ProvenanceCertificateBuilder {
    /// Create a new builder with the required identity fields.
    pub fn new(
        adapter_id: impl Into<String>,
        version_id: impl Into<String>,
        tenant_id: impl Into<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            version_id: version_id.into(),
            tenant_id: tenant_id.into(),
            training_data_hash: None,
            training_config_hash: None,
            training_job_id: None,
            training_final_loss: None,
            training_epochs: None,
            checkpoint_hash: None,
            checkpoint_signature: None,
            checkpoint_signer_key: None,
            promotion_review_id: None,
            promoted_by: None,
            promoted_at: None,
            promoted_from_state: None,
            promoted_to_state: None,
            policy_pack_hash: None,
            policy_pack_id: None,
            base_model_id: None,
            egress_blocked: None,
            egress_rules_fingerprint: None,
        }
    }

    // --- Training provenance ---

    pub fn training_data_hash(mut self, hash: impl Into<String>) -> Self {
        self.training_data_hash = Some(hash.into());
        self
    }

    pub fn training_config_hash(mut self, hash: impl Into<String>) -> Self {
        self.training_config_hash = Some(hash.into());
        self
    }

    pub fn training_job_id(mut self, id: impl Into<String>) -> Self {
        self.training_job_id = Some(id.into());
        self
    }

    pub fn training_final_loss(mut self, loss: f64) -> Self {
        self.training_final_loss = Some(loss);
        self
    }

    pub fn training_epochs(mut self, epochs: u32) -> Self {
        self.training_epochs = Some(epochs);
        self
    }

    // --- Checkpoint provenance ---

    pub fn checkpoint_hash(mut self, hash: impl Into<String>) -> Self {
        self.checkpoint_hash = Some(hash.into());
        self
    }

    pub fn checkpoint_signature(mut self, sig: impl Into<String>) -> Self {
        self.checkpoint_signature = Some(sig.into());
        self
    }

    pub fn checkpoint_signer_key(mut self, key: impl Into<String>) -> Self {
        self.checkpoint_signer_key = Some(key.into());
        self
    }

    // --- Promotion provenance ---

    pub fn promotion_review_id(mut self, id: impl Into<String>) -> Self {
        self.promotion_review_id = Some(id.into());
        self
    }

    pub fn promoted_by(mut self, by: impl Into<String>) -> Self {
        self.promoted_by = Some(by.into());
        self
    }

    pub fn promoted_at(mut self, at: impl Into<String>) -> Self {
        self.promoted_at = Some(at.into());
        self
    }

    pub fn promoted_from_state(mut self, state: impl Into<String>) -> Self {
        self.promoted_from_state = Some(state.into());
        self
    }

    pub fn promoted_to_state(mut self, state: impl Into<String>) -> Self {
        self.promoted_to_state = Some(state.into());
        self
    }

    // --- Serving provenance ---

    pub fn policy_pack_hash(mut self, hash: impl Into<String>) -> Self {
        self.policy_pack_hash = Some(hash.into());
        self
    }

    pub fn policy_pack_id(mut self, id: impl Into<String>) -> Self {
        self.policy_pack_id = Some(id.into());
        self
    }

    pub fn base_model_id(mut self, id: impl Into<String>) -> Self {
        self.base_model_id = Some(id.into());
        self
    }

    // --- Egress attestation ---

    pub fn egress_blocked(mut self, blocked: bool) -> Self {
        self.egress_blocked = Some(blocked);
        self
    }

    pub fn egress_rules_fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.egress_rules_fingerprint = Some(fp.into());
        self
    }

    /// Finalize and sign the certificate.
    ///
    /// Generates a unique certificate ID from BLAKE3(adapter_id || version_id || timestamp),
    /// computes the canonical content hash via JCS, and signs with the provided keypair.
    pub fn sign(self, keypair: &Keypair) -> Result<ProvenanceCertificate> {
        let generated_at = chrono::Utc::now().to_rfc3339();

        // Generate deterministic certificate ID from identity + timestamp
        let id_input = format!("{}:{}:{}", self.adapter_id, self.version_id, generated_at);
        let id_hash = B3Hash::hash(id_input.as_bytes());
        let certificate_id = format!("cert-{}", &id_hash.to_hex()[..32]);

        // Build the unsigned certificate (content_hash/signature/signer_public_key are placeholders)
        let mut cert = ProvenanceCertificate {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            certificate_id,
            adapter_id: self.adapter_id,
            version_id: self.version_id,
            tenant_id: self.tenant_id,
            training_data_hash: self.training_data_hash,
            training_config_hash: self.training_config_hash,
            training_job_id: self.training_job_id,
            training_final_loss: self.training_final_loss,
            training_epochs: self.training_epochs,
            checkpoint_hash: self.checkpoint_hash,
            checkpoint_signature: self.checkpoint_signature,
            checkpoint_signer_key: self.checkpoint_signer_key,
            promotion_review_id: self.promotion_review_id,
            promoted_by: self.promoted_by,
            promoted_at: self.promoted_at,
            promoted_from_state: self.promoted_from_state,
            promoted_to_state: self.promoted_to_state,
            policy_pack_hash: self.policy_pack_hash,
            policy_pack_id: self.policy_pack_id,
            base_model_id: self.base_model_id,
            egress_blocked: self.egress_blocked,
            egress_rules_fingerprint: self.egress_rules_fingerprint,
            generated_at,
            // Placeholders — filled below
            content_hash: String::new(),
            signature: String::new(),
            signer_public_key: String::new(),
        };

        // Compute content hash
        let content_hash = cert.compute_content_hash()?;

        // Sign the content hash bytes
        let hash_bytes = hex::decode(&content_hash)
            .map_err(|e| AosError::Crypto(format!("Failed to decode content hash: {}", e)))?;
        let sig = keypair.sign(&hash_bytes);

        cert.content_hash = content_hash;
        cert.signature = hex::encode(sig.to_bytes());
        cert.signer_public_key = hex::encode(keypair.public_key().to_bytes());

        Ok(cert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_full_cert(keypair: &Keypair) -> ProvenanceCertificate {
        ProvenanceCertificateBuilder::new("adapter-001", "v1.0.0", "tenant-abc")
            .training_data_hash("b3deadbeef")
            .training_config_hash("b3cafebabe")
            .training_job_id("job-123")
            .training_final_loss(0.042)
            .training_epochs(10)
            .checkpoint_hash("b3checkpoint")
            .checkpoint_signature("sig-hex")
            .checkpoint_signer_key("key-hex")
            .promotion_review_id("review-456")
            .promoted_by("admin@example.com")
            .promoted_at("2026-01-15T12:00:00Z")
            .promoted_from_state("shadow")
            .promoted_to_state("active")
            .policy_pack_hash("b3policy")
            .policy_pack_id("pp-default-v2")
            .base_model_id("Llama-3.2-3B-Instruct-4bit")
            .egress_blocked(true)
            .egress_rules_fingerprint("b3egress")
            .sign(keypair)
            .expect("signing should succeed")
    }

    fn make_minimal_cert(keypair: &Keypair) -> ProvenanceCertificate {
        ProvenanceCertificateBuilder::new("adapter-002", "v0.1.0", "tenant-xyz")
            .sign(keypair)
            .expect("signing should succeed")
    }

    #[test]
    fn builder_sign_verify_roundtrip() {
        let keypair = Keypair::generate();
        let cert = make_full_cert(&keypair);

        assert!(cert.certificate_id.starts_with("cert-"));
        assert_eq!(cert.schema_version, PROVENANCE_SCHEMA_VERSION);
        assert_eq!(cert.adapter_id, "adapter-001");
        assert_eq!(cert.version_id, "v1.0.0");
        assert_eq!(cert.tenant_id, "tenant-abc");
        assert!(cert.verify().expect("verify should not error"));
    }

    #[test]
    fn minimal_cert_verifies() {
        let keypair = Keypair::generate();
        let cert = make_minimal_cert(&keypair);

        assert!(cert.verify().expect("verify should not error"));
        assert_eq!(cert.adapter_id, "adapter-002");
        assert!(cert.training_data_hash.is_none());
        assert!(cert.checkpoint_hash.is_none());
    }

    #[test]
    fn tampered_adapter_id_detected() {
        let keypair = Keypair::generate();
        let mut cert = make_full_cert(&keypair);
        cert.adapter_id = "tampered-adapter".to_string();

        // Content hash will no longer match
        assert!(!cert.verify().expect("verify should not error"));
    }

    #[test]
    fn tampered_training_hash_detected() {
        let keypair = Keypair::generate();
        let mut cert = make_full_cert(&keypair);
        cert.training_data_hash = Some("tampered-hash".to_string());

        assert!(!cert.verify().expect("verify should not error"));
    }

    #[test]
    fn tampered_content_hash_detected() {
        let keypair = Keypair::generate();
        let mut cert = make_full_cert(&keypair);
        // Tamper content_hash directly — signature won't match
        cert.content_hash = "ff".repeat(32);

        assert!(!cert.verify().expect("verify should not error"));
    }

    #[test]
    fn tampered_signature_detected() {
        let keypair = Keypair::generate();
        let mut cert = make_full_cert(&keypair);
        // Flip a byte in the signature
        let mut sig_bytes = hex::decode(&cert.signature).unwrap();
        sig_bytes[0] ^= 0xff;
        cert.signature = hex::encode(sig_bytes);

        assert!(!cert.verify().expect("verify should not error"));
    }

    #[test]
    fn wrong_key_detected() {
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        let mut cert = make_full_cert(&keypair1);
        // Replace signer key with a different key
        cert.signer_public_key = hex::encode(keypair2.public_key().to_bytes());

        assert!(!cert.verify().expect("verify should not error"));
    }

    #[test]
    fn chain_completeness_full() {
        let keypair = Keypair::generate();
        let cert = make_full_cert(&keypair);
        let report = cert.verify_report().expect("report should succeed");

        assert!(report.valid);
        assert!(report.signer_key_id.starts_with("kid-"));
        assert!(report.chain_completeness.has_training_data);
        assert!(report.chain_completeness.has_checkpoint);
        assert!(report.chain_completeness.has_promotion);
        assert!(report.chain_completeness.has_policy);
        assert!(report.chain_completeness.has_egress_attestation);
        assert!((report.chain_completeness.completeness_score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn chain_completeness_empty() {
        let keypair = Keypair::generate();
        let cert = make_minimal_cert(&keypair);
        let report = cert.verify_report().expect("report should succeed");

        assert!(report.valid);
        assert!(!report.chain_completeness.has_training_data);
        assert!(!report.chain_completeness.has_checkpoint);
        assert!(!report.chain_completeness.has_promotion);
        assert!(!report.chain_completeness.has_policy);
        assert!(!report.chain_completeness.has_egress_attestation);
        assert!(report.chain_completeness.completeness_score.abs() < f32::EPSILON);
    }

    #[test]
    fn chain_completeness_partial() {
        let keypair = Keypair::generate();
        let cert = ProvenanceCertificateBuilder::new("a", "v1", "t")
            .training_data_hash("hash")
            .checkpoint_hash("hash")
            .sign(&keypair)
            .expect("signing should succeed");

        let completeness = cert.chain_completeness();
        assert!(completeness.has_training_data);
        assert!(completeness.has_checkpoint);
        assert!(!completeness.has_promotion);
        assert!(!completeness.has_policy);
        assert!(!completeness.has_egress_attestation);
        assert!((completeness.completeness_score - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn schema_version_mismatch_errors() {
        let keypair = Keypair::generate();
        let mut cert = make_minimal_cert(&keypair);
        cert.schema_version = 99;

        let err = cert.verify().unwrap_err();
        assert!(err.to_string().contains("schema version"));
    }

    #[test]
    fn certificate_id_format() {
        let keypair = Keypair::generate();
        let cert = make_minimal_cert(&keypair);
        assert!(cert.certificate_id.starts_with("cert-"));
        // cert- prefix + 32 hex chars = 37 total
        assert_eq!(cert.certificate_id.len(), 37);
    }

    #[test]
    fn serde_roundtrip() {
        let keypair = Keypair::generate();
        let cert = make_full_cert(&keypair);

        let json = serde_json::to_string_pretty(&cert).expect("serialize should succeed");
        let deserialized: ProvenanceCertificate =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert!(deserialized.verify().expect("verify should not error"));
        assert_eq!(deserialized.certificate_id, cert.certificate_id);
        assert_eq!(deserialized.adapter_id, cert.adapter_id);
    }

    #[test]
    fn content_hash_is_deterministic() {
        // Two certs with identical fields should have the same content hash
        // (we test by building the same data twice with a fixed timestamp)
        let keypair = Keypair::generate();
        let cert1 = make_full_cert(&keypair);

        // Recompute the content hash — should match what's stored
        let recomputed = cert1.compute_content_hash().expect("hash should succeed");
        assert_eq!(recomputed, cert1.content_hash);
    }

    #[test]
    fn verify_report_invalid_cert() {
        let keypair = Keypair::generate();
        let mut cert = make_full_cert(&keypair);
        cert.adapter_id = "tampered".to_string();

        let report = cert.verify_report().expect("report should succeed");
        assert!(!report.valid);
        assert_eq!(report.signer_key_id, "invalid");
    }
}

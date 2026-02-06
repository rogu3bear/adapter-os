//! Boot Evidence Chain for cryptographic boot attestation.
//!
//! This module provides types and traits for building a tamper-evident chain of boot
//! phase evidence, culminating in a signed `BootAttestation`. The design follows the
//! patterns established in `adapteros-crypto::decision_chain` for Merkle tree construction
//! and hash chain linking.
//!
//! # Design Principles
//!
//! 1. **Evidence-Enforced Invariants**: No evidence = didn't run = boot fails
//! 2. **Hash Chain Linking**: Each phase's evidence includes the previous phase's hash
//! 3. **Merkle Tree Finalization**: Final attestation is a signed Merkle root
//! 4. **Timeout Produces Evidence**: Phase timeouts create evidence (not silent hangs)
//!
//! # Example
//!
//! ```rust,no_run
//! use adapteros_boot::evidence::{
//!     BootEvidenceChainBuilder, BootPhaseEvidence, PhaseOutcome, InvariantCheck
//! };
//! use adapteros_core::B3Hash;
//! use std::time::Duration;
//!
//! let mut chain = BootEvidenceChainBuilder::new(
//!     Duration::from_secs(30),  // phase timeout
//!     Duration::from_secs(300), // boot timeout
//! );
//!
//! // Each phase produces evidence
//! let mut evidence = BootPhaseEvidence::new(
//!     "config-loading",
//!     1704067200_000_000, // started_at_us
//!     1704067200_100_000, // completed_at_us
//!     PhaseOutcome::Success,
//!     B3Hash::hash(b"config_payload"),
//!     None, // first phase has no previous
//! );
//! evidence.add_check("SEC-001", true, true);
//! chain.push_evidence(evidence);
//!
//! // Verify chain and compute attestation
//! assert!(chain.verify_chain());
//! let merkle_root = chain.compute_merkle_root();
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Schema version for boot evidence format.
/// Increment when making breaking changes to canonical encoding.
pub const BOOT_EVIDENCE_SCHEMA_VERSION: u8 = 1;

/// Evidence produced by a single boot phase.
///
/// Each phase must produce evidence to prove it ran. Missing evidence
/// for any required phase causes boot failure (fail-closed semantics).
///
/// # Canonical Encoding
///
/// Evidence is canonically encoded using JCS (JSON Canonical Serialization)
/// for deterministic hashing, matching the pattern in `RouterEventDigest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootPhaseEvidence {
    /// Schema version for forward compatibility
    pub schema_version: u8,

    /// Phase name (e.g., "db-connecting", "loading-policies")
    pub phase_name: String,

    /// Timestamp when phase started (microseconds since Unix epoch)
    pub started_at_us: u64,

    /// Timestamp when phase completed (microseconds since Unix epoch)
    pub completed_at_us: u64,

    /// Phase outcome
    pub outcome: PhaseOutcome,

    /// Invariant checks performed during this phase
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks_performed: Vec<InvariantCheck>,

    /// Hash of previous phase evidence (chain linking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<B3Hash>,

    /// BLAKE3 hash of phase-specific payload (e.g., config hash, key fingerprint)
    pub payload_digest: B3Hash,
}

/// Outcome of a boot phase execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseOutcome {
    /// Phase completed successfully
    Success,
    /// Phase completed with warnings (non-fatal violations)
    SuccessWithWarnings,
    /// Phase timed out (produces evidence but blocks boot)
    TimedOut,
    /// Phase failed (produces evidence, blocks boot)
    Failed,
    /// Phase was skipped (produces evidence with skip reason)
    Skipped,
}

impl PhaseOutcome {
    /// Whether this outcome blocks boot from completing.
    pub fn blocks_boot(&self) -> bool {
        matches!(self, Self::TimedOut | Self::Failed)
    }

    /// Whether this outcome indicates a warning condition.
    pub fn has_warnings(&self) -> bool {
        matches!(self, Self::SuccessWithWarnings)
    }
}

/// Record of an invariant check performed during a phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvariantCheck {
    /// Invariant ID (e.g., "SEC-001", "DAT-002")
    pub id: String,
    /// Whether the check passed
    pub passed: bool,
    /// Whether failure is fatal (blocks boot)
    pub fatal_on_failure: bool,
}

impl InvariantCheck {
    /// Create a new invariant check record.
    pub fn new(id: impl Into<String>, passed: bool, fatal_on_failure: bool) -> Self {
        Self {
            id: id.into(),
            passed,
            fatal_on_failure,
        }
    }

    /// Create a passing check.
    pub fn pass(id: impl Into<String>, fatal_on_failure: bool) -> Self {
        Self::new(id, true, fatal_on_failure)
    }

    /// Create a failing check.
    pub fn fail(id: impl Into<String>, fatal_on_failure: bool) -> Self {
        Self::new(id, false, fatal_on_failure)
    }
}

impl BootPhaseEvidence {
    /// Create new evidence for a phase.
    pub fn new(
        phase_name: impl Into<String>,
        started_at_us: u64,
        completed_at_us: u64,
        outcome: PhaseOutcome,
        payload_digest: B3Hash,
        previous_hash: Option<B3Hash>,
    ) -> Self {
        Self {
            schema_version: BOOT_EVIDENCE_SCHEMA_VERSION,
            phase_name: phase_name.into(),
            started_at_us,
            completed_at_us,
            outcome,
            checks_performed: Vec::new(),
            previous_hash,
            payload_digest,
        }
    }

    /// Add an invariant check result.
    pub fn add_check(&mut self, id: impl Into<String>, passed: bool, fatal_on_failure: bool) {
        self.checks_performed
            .push(InvariantCheck::new(id, passed, fatal_on_failure));
    }

    /// Add a pre-built invariant check.
    pub fn push_check(&mut self, check: InvariantCheck) {
        self.checks_performed.push(check);
    }

    /// Compute canonical bytes for hashing using JCS.
    ///
    /// # Panics
    ///
    /// Panics if JCS serialization fails. This should never happen with the
    /// simple primitive types in BootPhaseEvidence and would indicate a bug.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_jcs::to_vec(self).unwrap_or_else(|e| {
            tracing::error!(
                error = %e,
                phase_name = %self.phase_name,
                "CRITICAL: BootPhaseEvidence serialization failed"
            );
            panic!("BootPhaseEvidence serialization failed: {}", e)
        })
    }

    /// Compute BLAKE3 hash of this evidence.
    pub fn hash(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }

    /// Duration of this phase in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        (self.completed_at_us.saturating_sub(self.started_at_us)) / 1000
    }

    /// Whether this phase has any fatal failures that block boot.
    pub fn has_fatal_failures(&self) -> bool {
        self.checks_performed
            .iter()
            .any(|c| !c.passed && c.fatal_on_failure)
    }

    /// Whether this phase blocks boot from completing.
    pub fn blocks_boot(&self) -> bool {
        self.outcome.blocks_boot() || self.has_fatal_failures()
    }

    /// Count of checks that passed.
    pub fn passed_count(&self) -> usize {
        self.checks_performed.iter().filter(|c| c.passed).count()
    }

    /// Count of checks that failed.
    pub fn failed_count(&self) -> usize {
        self.checks_performed.iter().filter(|c| !c.passed).count()
    }
}

/// Builder for constructing a chain of boot phase evidence.
///
/// Follows the pattern from `DecisionChainBuilder` in `adapteros-crypto`
/// but for boot phases. Each phase's evidence includes the hash of the
/// previous phase, forming a tamper-evident chain.
#[derive(Debug)]
pub struct BootEvidenceChainBuilder {
    /// Collected phase evidence
    phases: Vec<BootPhaseEvidence>,
    /// Hash of last phase (for chain linking)
    last_hash: Option<B3Hash>,
    /// Boot start time (monotonic)
    boot_started_at: Instant,
    /// Boot start timestamp (microseconds since Unix epoch)
    boot_started_at_us: u64,
    /// Per-phase timeout duration
    phase_timeout: Duration,
    /// Overall boot timeout
    boot_timeout: Duration,
}

impl BootEvidenceChainBuilder {
    /// Create a new builder with the specified timeouts.
    pub fn new(phase_timeout: Duration, boot_timeout: Duration) -> Self {
        let now = std::time::SystemTime::now();
        let boot_started_at_us = now
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        Self {
            phases: Vec::new(),
            last_hash: None,
            boot_started_at: Instant::now(),
            boot_started_at_us,
            phase_timeout,
            boot_timeout,
        }
    }

    /// Create a builder with default timeouts (30s per phase, 300s total).
    pub fn with_defaults() -> Self {
        Self::new(Duration::from_secs(30), Duration::from_secs(300))
    }

    /// Check if the overall boot timeout has been exceeded.
    pub fn is_boot_timed_out(&self) -> bool {
        self.boot_started_at.elapsed() > self.boot_timeout
    }

    /// Get the per-phase timeout.
    pub fn phase_timeout(&self) -> Duration {
        self.phase_timeout
    }

    /// Get the overall boot timeout.
    pub fn boot_timeout(&self) -> Duration {
        self.boot_timeout
    }

    /// Get boot start timestamp in microseconds since Unix epoch.
    pub fn boot_started_at_us(&self) -> u64 {
        self.boot_started_at_us
    }

    /// Add completed phase evidence to the chain.
    ///
    /// The evidence's `previous_hash` will be set to the last phase's hash.
    pub fn push_evidence(&mut self, mut evidence: BootPhaseEvidence) {
        evidence.previous_hash = self.last_hash;
        self.last_hash = Some(evidence.hash());
        self.phases.push(evidence);
    }

    /// Get all phases in the chain.
    pub fn phases(&self) -> &[BootPhaseEvidence] {
        &self.phases
    }

    /// Get the number of phases in the chain.
    pub fn len(&self) -> usize {
        self.phases.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.phases.is_empty()
    }

    /// Get the last phase's hash.
    pub fn last_hash(&self) -> Option<B3Hash> {
        self.last_hash
    }

    /// Check if any phase blocked boot.
    pub fn has_blocking_phase(&self) -> bool {
        self.phases.iter().any(|p| p.blocks_boot())
    }

    /// Get the first blocking phase, if any.
    pub fn first_blocking_phase(&self) -> Option<&BootPhaseEvidence> {
        self.phases.iter().find(|p| p.blocks_boot())
    }

    /// Compute the Merkle root over all phase evidence.
    ///
    /// Returns `BLAKE3(MerkleRoot(phase_hashes))`.
    ///
    /// If the chain is empty, returns a hash of the sentinel value
    /// "empty_boot_evidence_chain" for deterministic behavior.
    pub fn compute_merkle_root(&self) -> B3Hash {
        if self.phases.is_empty() {
            return B3Hash::hash(b"empty_boot_evidence_chain");
        }

        // Compute leaf hashes
        let mut leaves: Vec<B3Hash> = self.phases.iter().map(|p| p.hash()).collect();

        // Build Merkle tree bottom-up (same algorithm as DecisionChainBuilder)
        while leaves.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in leaves.chunks(2) {
                let hash = if chunk.len() == 2 {
                    // Parent = BLAKE3(left || right)
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[1].as_bytes());
                    B3Hash::hash(&combined)
                } else {
                    // Odd node: duplicate for pairing
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[0].as_bytes());
                    B3Hash::hash(&combined)
                };
                next_level.push(hash);
            }

            leaves = next_level;
        }

        // Final boot evidence chain hash = BLAKE3(merkle_root)
        B3Hash::hash(leaves[0].as_bytes())
    }

    /// Verify the internal hash chain integrity.
    ///
    /// Returns `true` if all phases properly link to their predecessors.
    pub fn verify_chain(&self) -> bool {
        let mut expected_prev: Option<B3Hash> = None;

        for phase in &self.phases {
            if phase.previous_hash != expected_prev {
                return false;
            }
            expected_prev = Some(phase.hash());
        }

        true
    }

    /// Get boot duration so far.
    pub fn elapsed(&self) -> Duration {
        self.boot_started_at.elapsed()
    }

    /// Get total boot time in milliseconds (sum of phase durations).
    pub fn total_boot_time_ms(&self) -> u64 {
        self.phases.iter().map(|p| p.duration_ms()).sum()
    }

    /// Get count of passed invariant checks across all phases.
    pub fn total_passed_checks(&self) -> usize {
        self.phases.iter().map(|p| p.passed_count()).sum()
    }

    /// Get count of failed invariant checks across all phases.
    pub fn total_failed_checks(&self) -> usize {
        self.phases.iter().map(|p| p.failed_count()).sum()
    }
}

/// Signed attestation of a complete boot sequence.
///
/// This is the cryptographic proof that a boot completed successfully
/// with all phases producing evidence. It can be:
/// - Verified remotely without access to the full evidence chain
/// - Stored for compliance auditing
/// - Used to detect if boot was tampered with
///
/// # Verification
///
/// 1. Verify signature over canonical bytes using public key
/// 2. Check Merkle root matches expected (if you have the chain)
/// 3. Check boot_id is unique (replay prevention)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootAttestation {
    /// Schema version
    pub schema_version: u8,

    /// Unique boot identifier (UUID or timestamp-based)
    pub boot_id: String,

    /// Merkle root of all phase evidence
    pub merkle_root: B3Hash,

    /// Number of phases in the chain
    pub phase_count: u32,

    /// Total boot time in milliseconds
    pub total_boot_time_ms: u64,

    /// Whether boot completed successfully (all phases passed)
    pub boot_successful: bool,

    /// Number of invariant checks passed
    pub checks_passed: u32,

    /// Number of invariant checks failed
    pub checks_failed: u32,

    /// Git commit hash (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,

    /// Build timestamp (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_timestamp: Option<u64>,

    /// Attestation timestamp (microseconds since Unix epoch)
    pub attested_at_us: u64,

    /// Ed25519 signature over canonical bytes (hex-encoded, 128 chars)
    #[serde(default)]
    pub signature: String,

    /// Public key used for signing (hex-encoded, 64 chars)
    #[serde(default)]
    pub public_key: String,

    /// Key ID (first 16 bytes of BLAKE3(pubkey), hex-encoded)
    #[serde(default)]
    pub key_id: String,
}

impl BootAttestation {
    /// Create an unsigned attestation from a completed evidence chain.
    pub fn from_chain(chain: &BootEvidenceChainBuilder, boot_id: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now();
        let attested_at_us = now
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        Self {
            schema_version: BOOT_EVIDENCE_SCHEMA_VERSION,
            boot_id: boot_id.into(),
            merkle_root: chain.compute_merkle_root(),
            phase_count: chain.len() as u32,
            total_boot_time_ms: chain.total_boot_time_ms(),
            boot_successful: !chain.has_blocking_phase(),
            checks_passed: chain.total_passed_checks() as u32,
            checks_failed: chain.total_failed_checks() as u32,
            git_commit: Some(adapteros_core::version::GIT_COMMIT_HASH.to_string())
                .filter(|s| s != "unknown"),
            build_timestamp: {
                let ts = adapteros_core::version::BUILD_TIMESTAMP;
                if ts != "unknown" {
                    ts.parse().ok()
                } else {
                    None
                }
            },
            attested_at_us,
            signature: String::new(),
            public_key: String::new(),
            key_id: String::new(),
        }
    }

    /// Compute canonical bytes for signing using JCS.
    ///
    /// Note: signature, public_key, and key_id are excluded from canonical
    /// bytes since they're set after signing.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // Create a version without signature fields for canonical encoding
        let canonical = serde_json::json!({
            "schema_version": self.schema_version,
            "boot_id": self.boot_id,
            "merkle_root": self.merkle_root,
            "phase_count": self.phase_count,
            "total_boot_time_ms": self.total_boot_time_ms,
            "boot_successful": self.boot_successful,
            "checks_passed": self.checks_passed,
            "checks_failed": self.checks_failed,
            "git_commit": self.git_commit,
            "build_timestamp": self.build_timestamp,
            "attested_at_us": self.attested_at_us,
        });
        serde_jcs::to_vec(&canonical).unwrap_or_else(|e| {
            tracing::error!(
                error = %e,
                boot_id = %self.boot_id,
                "CRITICAL: BootAttestation serialization failed"
            );
            panic!("BootAttestation serialization failed: {}", e)
        })
    }

    /// Compute digest of canonical bytes.
    pub fn digest(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }

    /// Sign this attestation with an Ed25519 signing key.
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let canonical = self.canonical_bytes();
        let signature = signing_key.sign(&canonical);
        let verifying_key = signing_key.verifying_key();

        self.signature = hex::encode(signature.to_bytes());
        self.public_key = hex::encode(verifying_key.to_bytes());

        // Key ID: first 16 bytes of BLAKE3(pubkey), hex-encoded (32 chars)
        let key_hash = B3Hash::hash(&verifying_key.to_bytes());
        self.key_id = key_hash.to_hex()[..32].to_string();
    }

    /// Verify the signature.
    pub fn verify(&self) -> Result<()> {
        if self.signature.is_empty() || self.public_key.is_empty() {
            return Err(AosError::Crypto("Attestation not signed".to_string()));
        }

        let pubkey_bytes = hex::decode(&self.public_key)
            .map_err(|e| AosError::Crypto(format!("Invalid public key hex: {}", e)))?;

        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if pubkey_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid public key length: expected 32, got {}",
                pubkey_bytes.len()
            )));
        }

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: expected 64, got {}",
                sig_bytes.len()
            )));
        }

        let verifying_key = VerifyingKey::from_bytes(
            &pubkey_bytes
                .try_into()
                .expect("length already verified to be 32"),
        )
        .map_err(|e| AosError::Crypto(format!("Invalid public key: {}", e)))?;

        let signature = Signature::from_bytes(
            &sig_bytes
                .try_into()
                .expect("length already verified to be 64"),
        );

        let canonical = self.canonical_bytes();
        verifying_key
            .verify(&canonical, &signature)
            .map_err(|e| AosError::Crypto(format!("Signature verification failed: {}", e)))
    }

    /// Check if attestation is signed.
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty() && !self.public_key.is_empty()
    }

    /// Verify against a known public key (for key pinning).
    pub fn verify_with_key(&self, expected_public_key: &VerifyingKey) -> Result<()> {
        if !self.is_signed() {
            return Err(AosError::Crypto("Attestation not signed".to_string()));
        }

        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: expected 64, got {}",
                sig_bytes.len()
            )));
        }

        let signature = Signature::from_bytes(
            &sig_bytes
                .try_into()
                .expect("length already verified to be 64"),
        );

        let canonical = self.canonical_bytes();
        expected_public_key
            .verify(&canonical, &signature)
            .map_err(|e| AosError::Crypto(format!("Signature verification failed: {}", e)))
    }
}

/// Helper to get current timestamp in microseconds since Unix epoch.
pub fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_evidence(
        name: &str,
        start: u64,
        end: u64,
        outcome: PhaseOutcome,
        prev: Option<B3Hash>,
    ) -> BootPhaseEvidence {
        BootPhaseEvidence::new(
            name,
            start,
            end,
            outcome,
            B3Hash::hash(format!("payload_{}", name).as_bytes()),
            prev,
        )
    }

    #[test]
    fn test_evidence_chain_empty() {
        let chain = BootEvidenceChainBuilder::with_defaults();
        let root = chain.compute_merkle_root();
        assert_eq!(root, B3Hash::hash(b"empty_boot_evidence_chain"));
        assert!(chain.verify_chain());
    }

    #[test]
    fn test_evidence_chain_single_phase() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let mut evidence = create_test_evidence("config", 1000, 2000, PhaseOutcome::Success, None);
        evidence.add_check("SEC-001", true, true);
        chain.push_evidence(evidence);

        assert_eq!(chain.len(), 1);
        assert!(chain.last_hash().is_some());
        assert!(chain.verify_chain());
        assert!(!chain.has_blocking_phase());
    }

    #[test]
    fn test_evidence_chain_multiple_phases() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let phases = [
            ("config", PhaseOutcome::Success),
            ("security", PhaseOutcome::Success),
            ("database", PhaseOutcome::Success),
            ("ready", PhaseOutcome::Success),
        ];

        for (i, (name, outcome)) in phases.iter().enumerate() {
            let evidence = create_test_evidence(
                name,
                (i * 1000) as u64,
                ((i + 1) * 1000) as u64,
                *outcome,
                chain.last_hash(),
            );
            chain.push_evidence(evidence);
        }

        assert_eq!(chain.len(), 4);
        assert!(chain.verify_chain());
        assert!(!chain.has_blocking_phase());

        // Verify chain linking
        let phases = chain.phases();
        assert!(phases[0].previous_hash.is_none());
        for i in 1..phases.len() {
            assert_eq!(phases[i].previous_hash, Some(phases[i - 1].hash()));
        }
    }

    #[test]
    fn test_evidence_chain_deterministic() {
        let build_chain = || {
            let mut chain = BootEvidenceChainBuilder::with_defaults();
            for i in 0..5 {
                let evidence = create_test_evidence(
                    &format!("phase_{}", i),
                    i * 1000,
                    (i + 1) * 1000,
                    PhaseOutcome::Success,
                    chain.last_hash(),
                );
                chain.push_evidence(evidence);
            }
            chain.compute_merkle_root()
        };

        let root1 = build_chain();
        let root2 = build_chain();
        assert_eq!(root1, root2, "Merkle root should be deterministic");
    }

    #[test]
    fn test_evidence_with_failed_checks() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let mut evidence =
            create_test_evidence("security", 1000, 2000, PhaseOutcome::Success, None);
        evidence.add_check("SEC-001", true, true);
        evidence.add_check("SEC-002", false, true); // Fatal failure
        chain.push_evidence(evidence);

        assert!(chain.has_blocking_phase());
        assert!(chain.phases()[0].has_fatal_failures());
    }

    #[test]
    fn test_evidence_with_timeout() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let evidence = create_test_evidence("database", 1000, 31000, PhaseOutcome::TimedOut, None);
        chain.push_evidence(evidence);

        assert!(chain.has_blocking_phase());
        assert!(chain.phases()[0].outcome.blocks_boot());
    }

    #[test]
    fn test_attestation_signing_and_verification() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let mut evidence = create_test_evidence("config", 1000, 2000, PhaseOutcome::Success, None);
        evidence.add_check("SEC-001", true, true);
        chain.push_evidence(evidence);

        let mut attestation = BootAttestation::from_chain(&chain, "test-boot-001");
        assert!(!attestation.is_signed());

        // Sign
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        attestation.sign(&signing_key);
        assert!(attestation.is_signed());

        // Verify
        assert!(attestation.verify().is_ok());

        // Verify with specific key
        let verifying_key = signing_key.verifying_key();
        assert!(attestation.verify_with_key(&verifying_key).is_ok());

        // Verify fails with wrong key
        let wrong_key = SigningKey::generate(&mut rand::thread_rng()).verifying_key();
        assert!(attestation.verify_with_key(&wrong_key).is_err());
    }

    #[test]
    fn test_attestation_tamper_detection() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        let evidence = create_test_evidence("config", 1000, 2000, PhaseOutcome::Success, None);
        chain.push_evidence(evidence);

        let mut attestation = BootAttestation::from_chain(&chain, "test-boot-001");
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        attestation.sign(&signing_key);

        // Tamper with attestation
        attestation.boot_successful = false;

        // Verification should fail
        assert!(attestation.verify().is_err());
    }

    #[test]
    fn test_invariant_check_helpers() {
        let pass = InvariantCheck::pass("SEC-001", true);
        assert!(pass.passed);
        assert!(pass.fatal_on_failure);

        let fail = InvariantCheck::fail("DAT-001", false);
        assert!(!fail.passed);
        assert!(!fail.fatal_on_failure);
    }

    #[test]
    fn test_phase_outcome_methods() {
        assert!(PhaseOutcome::Failed.blocks_boot());
        assert!(PhaseOutcome::TimedOut.blocks_boot());
        assert!(!PhaseOutcome::Success.blocks_boot());
        assert!(!PhaseOutcome::SuccessWithWarnings.blocks_boot());
        assert!(!PhaseOutcome::Skipped.blocks_boot());

        assert!(PhaseOutcome::SuccessWithWarnings.has_warnings());
        assert!(!PhaseOutcome::Success.has_warnings());
    }

    #[test]
    fn test_evidence_duration() {
        let evidence =
            create_test_evidence("test", 1_000_000, 2_500_000, PhaseOutcome::Success, None);
        assert_eq!(evidence.duration_ms(), 1500);
    }

    #[test]
    fn test_chain_statistics() {
        let mut chain = BootEvidenceChainBuilder::with_defaults();

        // Timestamps are in microseconds, so 1_000_000us = 1000ms
        let mut e1 = create_test_evidence("phase1", 0, 1_000_000, PhaseOutcome::Success, None);
        e1.add_check("A", true, true);
        e1.add_check("B", false, false); // Non-fatal fail
        chain.push_evidence(e1);

        let mut e2 = create_test_evidence(
            "phase2",
            1_000_000,
            2_000_000,
            PhaseOutcome::Success,
            chain.last_hash(),
        );
        e2.add_check("C", true, true);
        e2.add_check("D", true, false);
        chain.push_evidence(e2);

        assert_eq!(chain.total_passed_checks(), 3);
        assert_eq!(chain.total_failed_checks(), 1);
        assert_eq!(chain.total_boot_time_ms(), 2000);
    }
}

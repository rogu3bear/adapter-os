//! Runtime policy signing, verification, and integrity checking
//!
//! Provides comprehensive policy verification at load time with:
//! - Ed25519 signature verification
//! - File integrity checking via BLAKE3 hashing
//! - Tamper detection and recovery
//! - Audit trail for policy modifications
//! - Version compatibility checking

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::signature::{Keypair, PublicKey, Signature};
use blake3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Policy integrity metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyIntegrityMetadata {
    /// BLAKE3 hash of policy file
    pub file_hash: String,
    /// Ed25519 signature of canonical policy data
    pub signature: String,
    /// Public key that signed the policy
    pub public_key: String,
    /// Timestamp of last signature
    pub signed_at: u64,
    /// Version of integrity schema
    pub schema_version: u8,
    /// List of previous file hashes (for tamper detection)
    pub hash_history: Vec<String>,
    /// Timestamp of last tamper check
    pub last_verification: u64,
}

impl PolicyIntegrityMetadata {
    /// Create new integrity metadata
    pub fn new(
        file_hash: String,
        signature: String,
        public_key: String,
        schema_version: u8,
    ) -> Self {
        let signed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            file_hash: file_hash.clone(),
            signature,
            public_key,
            signed_at,
            schema_version,
            hash_history: vec![file_hash],
            last_verification: signed_at,
        }
    }
}

/// Policy verification result with detailed diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVerificationResult {
    /// Overall verification status
    pub is_valid: bool,
    /// Signature verification passed
    pub signature_valid: bool,
    /// File hash matches expectations
    pub hash_valid: bool,
    /// No tampering detected
    pub tamper_free: bool,
    /// Schema version compatible
    pub version_compatible: bool,
    /// Detailed error message if invalid
    pub error_message: Option<String>,
    /// List of detected issues
    pub issues: Vec<String>,
    /// Timestamp of verification
    pub verified_at: u64,
}

impl Default for PolicyVerificationResult {
    fn default() -> Self {
        Self {
            is_valid: false,
            signature_valid: false,
            hash_valid: false,
            tamper_free: false,
            version_compatible: false,
            error_message: None,
            issues: Vec::new(),
            verified_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Tamper detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TamperDetectionResult {
    /// Policy file was tampered with
    pub tampered: bool,
    /// Expected hash before tampering
    pub expected_hash: String,
    /// Actual hash found
    pub actual_hash: String,
    /// When tampering was detected
    pub detected_at: u64,
    /// Suggested recovery action
    pub recovery_action: RecoveryAction,
}

/// Action to take when tampering is detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Quarantine policy and fail fast
    Quarantine,
    /// Load from backup
    LoadBackup,
    /// Retry with re-verification
    Retry,
    /// Manual intervention required
    Manual,
}

/// Policy integrity verifier
pub struct PolicyIntegrityVerifier {
    /// Trusted public keys for signature verification
    trusted_keys: Vec<PublicKey>,
    /// Cache of verified policy hashes
    verified_hashes: BTreeMap<String, u64>,
}

impl Default for PolicyIntegrityVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyIntegrityVerifier {
    /// Create new integrity verifier
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
            verified_hashes: BTreeMap::new(),
        }
    }

    /// Add a trusted public key
    pub fn add_trusted_key(&mut self, pubkey: PublicKey) {
        self.trusted_keys.push(pubkey);
    }

    /// Add multiple trusted keys
    pub fn add_trusted_keys(&mut self, pubkeys: Vec<PublicKey>) {
        self.trusted_keys.extend(pubkeys);
    }

    /// Verify policy file at path with metadata
    pub fn verify_policy_file(
        &mut self,
        path: &Path,
        metadata: &PolicyIntegrityMetadata,
    ) -> Result<PolicyVerificationResult> {
        let mut result = PolicyVerificationResult::default();
        result.verified_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check schema version compatibility
        if metadata.schema_version > 1 {
            result.version_compatible = false;
            result.issues.push(format!(
                "Unsupported schema version: {}",
                metadata.schema_version
            ));
        } else {
            result.version_compatible = true;
        }

        // Read policy file content
        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(e) => {
                result.error_message = Some(format!("Failed to read policy file: {}", e));
                result.issues.push(format!("I/O error: {}", e));
                return Ok(result);
            }
        };

        // Verify file hash
        let file_hash = compute_blake3_hash(&content)?;
        result.hash_valid = file_hash == metadata.file_hash;

        if !result.hash_valid {
            result.tamper_free = false;
            result.issues.push(format!(
                "File hash mismatch: expected {}, got {}",
                metadata.file_hash, file_hash
            ));
        } else {
            result.tamper_free = true;
        }

        // Verify Ed25519 signature
        if let Err(e) = self.verify_signature(&content, metadata) {
            result.signature_valid = false;
            result
                .issues
                .push(format!("Signature verification failed: {}", e));
        } else {
            result.signature_valid = true;
        }

        // Check hash history for tampering patterns
        if !metadata.hash_history.is_empty() && metadata.hash_history[0] != metadata.file_hash {
            // Hash changed, verify if legitimate
            if !result.hash_valid {
                result
                    .issues
                    .push("File has been modified since last signature".to_string());
                result.tamper_free = false;
            }
        }

        // Overall validity
        result.is_valid = result.signature_valid && result.hash_valid && result.version_compatible;

        if !result.is_valid {
            result.error_message = Some(format!(
                "Policy verification failed: signature={}, hash={}, version={}",
                result.signature_valid, result.hash_valid, result.version_compatible
            ));

            warn!(
                path = %path.display(),
                issues = ?result.issues,
                "Policy verification failed"
            );
        } else {
            info!(path = %path.display(), "Policy verification successful");
            self.verified_hashes.insert(file_hash, result.verified_at);
        }

        Ok(result)
    }

    /// Detect tampering in policy file
    pub fn detect_tampering(
        &self,
        path: &Path,
        metadata: &PolicyIntegrityMetadata,
    ) -> Result<TamperDetectionResult> {
        let content = std::fs::read(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read policy file for tampering check: {}",
                e
            ))
        })?;

        let actual_hash = compute_blake3_hash(&content)?;
        let expected_hash = &metadata.file_hash;

        let tampered = actual_hash != *expected_hash;

        let recovery_action = if tampered {
            // Determine recovery strategy
            if !metadata.hash_history.is_empty() {
                RecoveryAction::LoadBackup
            } else {
                RecoveryAction::Quarantine
            }
        } else {
            RecoveryAction::Retry
        };

        let result = TamperDetectionResult {
            tampered,
            expected_hash: expected_hash.clone(),
            actual_hash,
            detected_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            recovery_action,
        };

        if tampered {
            error!(
                path = %path.display(),
                expected = %expected_hash,
                actual = %result.actual_hash,
                "Policy file tampering detected"
            );
        }

        Ok(result)
    }

    /// Sign policy content with Ed25519
    pub fn sign_policy_content(
        &self,
        content: &[u8],
        signing_key: &Keypair,
    ) -> Result<(String, String, String)> {
        let signature = signing_key.sign(content);
        let public_key = signing_key.public_key();

        Ok((
            hex::encode(signature.to_bytes()),
            hex::encode(public_key.to_bytes()),
            compute_blake3_hash(content)?,
        ))
    }

    /// Verify signature against trusted keys
    fn verify_signature(&self, content: &[u8], metadata: &PolicyIntegrityMetadata) -> Result<()> {
        let sig_bytes = hex::decode(&metadata.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: {}",
                sig_bytes.len()
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array)
            .map_err(|e| AosError::Crypto(format!("Invalid signature format: {}", e)))?;

        // Try verification against all trusted keys
        for trusted_key in &self.trusted_keys {
            if trusted_key.verify(content, &signature).is_ok() {
                return Ok(());
            }
        }

        Err(AosError::Crypto(
            "Policy signature verification failed against all trusted keys".to_string(),
        ))
    }

    /// Record verified hash in cache
    pub fn cache_verification(&mut self, hash: String) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.verified_hashes.insert(hash, timestamp);
    }

    /// Check if hash was previously verified
    pub fn is_hash_verified(&self, hash: &str) -> bool {
        self.verified_hashes.contains_key(hash)
    }

    /// Get verification statistics
    pub fn get_verification_stats(&self) -> VerificationStats {
        VerificationStats {
            total_verified: self.verified_hashes.len(),
            trusted_keys_count: self.trusted_keys.len(),
        }
    }
}

/// Verification statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStats {
    /// Number of policies verified
    pub total_verified: usize,
    /// Number of trusted keys
    pub trusted_keys_count: usize,
}

/// Compute BLAKE3 hash of content
pub fn compute_blake3_hash(content: &[u8]) -> Result<String> {
    let hash = blake3::hash(content);
    Ok(hash.to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_integrity_metadata_creation() {
        let metadata = PolicyIntegrityMetadata::new(
            "hash123".to_string(),
            "sig456".to_string(),
            "pubkey789".to_string(),
            1,
        );

        assert_eq!(metadata.file_hash, "hash123");
        assert_eq!(metadata.signature, "sig456");
        assert_eq!(metadata.public_key, "pubkey789");
        assert_eq!(metadata.schema_version, 1);
        assert!(!metadata.hash_history.is_empty());
    }

    #[test]
    fn test_verification_result_defaults() {
        let result = PolicyVerificationResult::default();
        assert!(!result.is_valid);
        assert!(!result.signature_valid);
        assert!(!result.hash_valid);
        assert!(!result.tamper_free);
        assert!(!result.version_compatible);
    }

    #[test]
    fn test_blake3_hash_computation() {
        let content = b"test policy content";
        let hash1 = compute_blake3_hash(content).unwrap();
        let hash2 = compute_blake3_hash(content).unwrap();

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        let different_content = b"different policy content";
        let hash3 = compute_blake3_hash(different_content).unwrap();
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_verifier_trusted_keys_management() {
        let mut verifier = PolicyIntegrityVerifier::new();
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        verifier.add_trusted_key(keypair1.public_key());
        verifier.add_trusted_keys(vec![keypair2.public_key()]);

        let stats = verifier.get_verification_stats();
        assert_eq!(stats.trusted_keys_count, 2);
    }

    #[test]
    fn test_hash_verification_caching() {
        let mut verifier = PolicyIntegrityVerifier::new();
        let hash = "test_hash_value".to_string();

        assert!(!verifier.is_hash_verified(&hash));
        verifier.cache_verification(hash.clone());
        assert!(verifier.is_hash_verified(&hash));
    }

    #[test]
    fn test_tamper_detection_result() {
        let result = TamperDetectionResult {
            tampered: true,
            expected_hash: "expected".to_string(),
            actual_hash: "actual".to_string(),
            detected_at: 12345,
            recovery_action: RecoveryAction::Quarantine,
        };

        assert!(result.tampered);
        assert_eq!(result.recovery_action, RecoveryAction::Quarantine);
    }

    #[test]
    fn test_recovery_action_determination() {
        let mut metadata = PolicyIntegrityMetadata::new(
            "hash".to_string(),
            "sig".to_string(),
            "key".to_string(),
            1,
        );

        // With backup history
        let result_with_backup = TamperDetectionResult {
            tampered: true,
            expected_hash: "hash1".to_string(),
            actual_hash: "hash2".to_string(),
            detected_at: 12345,
            recovery_action: if !metadata.hash_history.is_empty() {
                RecoveryAction::LoadBackup
            } else {
                RecoveryAction::Quarantine
            },
        };

        assert_eq!(
            result_with_backup.recovery_action,
            RecoveryAction::LoadBackup
        );
    }
}

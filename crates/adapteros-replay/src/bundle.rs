//! Replay bundle management with double-sign protection
//!
//! Implements immutable replay bundles with protection against double-signing
//! to ensure audit trail integrity.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::signature::{Keypair, Signature};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Replay bundle with signature protection
pub struct ReplayBundle {
    bundle_path: PathBuf,
    signature_path: PathBuf,
    is_signed: bool,
    merkle_root: Option<B3Hash>,
}

impl ReplayBundle {
    /// Create a new replay bundle
    pub fn new<P: AsRef<Path>>(bundle_path: P) -> Self {
        let bundle_path = bundle_path.as_ref().to_path_buf();
        let signature_path = bundle_path.with_extension("ndjson.sig");

        Self {
            bundle_path,
            signature_path,
            is_signed: false,
            merkle_root: None,
        }
    }

    /// Check if bundle is already signed
    pub fn is_signed(&self) -> bool {
        self.is_signed || self.signature_path.exists()
    }

    /// Sign the replay bundle with double-sign protection
    pub fn sign(&mut self, signing_key: &Keypair) -> Result<()> {
        if self.is_signed() {
            return Err(AosError::Replay("Replay bundle already signed".to_string()));
        }

        // Compute Merkle root of bundle content
        let bundle_content = std::fs::read(&self.bundle_path)
            .map_err(|e| AosError::Io(format!("Failed to read bundle: {}", e)))?;

        let merkle_root = B3Hash::hash(&bundle_content);
        self.merkle_root = Some(merkle_root);

        // Sign the Merkle root
        let signature = signing_key.sign(merkle_root.as_bytes());

        // Create signature metadata
        let sig_metadata = ReplaySignatureMetadata {
            merkle_root: merkle_root.to_hex(),
            signature: hex::encode(signature.to_bytes()),
            public_key: hex::encode(signing_key.public_key().to_bytes()),
            bundle_path: self.bundle_path.to_string_lossy().to_string(),
            signed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
        };

        // Write signature file
        let sig_json =
            serde_json::to_string_pretty(&sig_metadata).map_err(AosError::Serialization)?;

        std::fs::write(&self.signature_path, sig_json)
            .map_err(|e| AosError::Io(format!("Failed to write signature: {}", e)))?;

        self.is_signed = true;
        Ok(())
    }

    /// Verify bundle signature
    pub fn verify_signature(&self, trusted_pubkey: &adapteros_crypto::PublicKey) -> Result<()> {
        if !self.is_signed() {
            return Err(AosError::Replay("Bundle is not signed".to_string()));
        }

        // Load signature metadata
        let sig_content = std::fs::read_to_string(&self.signature_path)
            .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;

        let sig_metadata: ReplaySignatureMetadata =
            serde_json::from_str(&sig_content).map_err(AosError::Serialization)?;

        // Decode signature
        let sig_bytes = hex::decode(&sig_metadata.signature)
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

        let expected_merkle_root = B3Hash::from_hex(&sig_metadata.merkle_root)
            .map_err(|e| AosError::Crypto(format!("Invalid merkle root hex: {}", e)))?;

        let bundle_content = std::fs::read(&self.bundle_path)
            .map_err(|e| AosError::Io(format!("Failed to read bundle: {}", e)))?;
        let actual_merkle_root = B3Hash::hash(&bundle_content);

        if expected_merkle_root != actual_merkle_root {
            return Err(AosError::Crypto(
                "Replay bundle merkle root mismatch".to_string(),
            ));
        }

        // Verify signature against Merkle root
        trusted_pubkey
            .verify(actual_merkle_root.as_bytes(), &signature)
            .map_err(|e| {
                AosError::Crypto(format!(
                    "Replay bundle signature verification failed: {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Get bundle path
    pub fn bundle_path(&self) -> &Path {
        &self.bundle_path
    }

    /// Get signature path
    pub fn signature_path(&self) -> &Path {
        &self.signature_path
    }

    /// Get Merkle root if available
    pub fn merkle_root(&self) -> Option<&B3Hash> {
        self.merkle_root.as_ref()
    }
}

/// Signature metadata for replay bundles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySignatureMetadata {
    pub merkle_root: String,
    pub signature: String,
    pub public_key: String,
    pub bundle_path: String,
    pub signed_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_double_sign_protection() {
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join("test_bundle.ndjson");

        // Create a test bundle
        std::fs::write(&bundle_path, b"test bundle content").unwrap();

        let mut replay_bundle = ReplayBundle::new(&bundle_path);
        let keypair = Keypair::generate();

        // First signing should succeed
        replay_bundle.sign(&keypair).unwrap();
        assert!(replay_bundle.is_signed());

        // Second signing should fail
        let result = replay_bundle.sign(&keypair);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already signed"));
    }

    #[test]
    fn test_replay_bundle_verification() {
        let temp_dir = tempdir().unwrap();
        let bundle_path = temp_dir.path().join("test_bundle.ndjson");

        // Create a test bundle
        std::fs::write(&bundle_path, b"test bundle content").unwrap();

        let mut replay_bundle = ReplayBundle::new(&bundle_path);
        let keypair = Keypair::generate();

        // Sign the bundle
        replay_bundle.sign(&keypair).unwrap();

        // Verify signature
        let public_key = keypair.public_key();
        replay_bundle.verify_signature(&public_key).unwrap();
    }
}

//! Bundle signing and verification for telemetry bundles
//!
//! Per Artifacts Ruleset #13: All bundles must be signed with Ed25519
//! Provides cryptographic chain-of-custody for all telemetry data

use crate::{Keypair, PublicKey, Signature};
use adapteros_core::{AosError, Result, B3Hash};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Bundle signature metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSignature {
    /// BLAKE3 hash of the bundle
    pub bundle_hash: B3Hash,
    /// Merkle root of events in the bundle
    pub merkle_root: B3Hash,
    /// Ed25519 signature over bundle_hash
    pub signature: Signature,
    /// Public key used for signing (for verification)
    pub public_key: PublicKey,
    /// Schema version
    pub schema_ver: u32,
    /// Timestamp when signed (microseconds since epoch)
    pub signed_at_us: u64,
    /// Key ID (deterministic: kid = blake3(pubkey))
    pub key_id: String,
}

impl BundleSignature {
    /// Create new bundle signature
    pub fn new(
        bundle_hash: B3Hash,
        merkle_root: B3Hash,
        signature: Signature,
        public_key: PublicKey,
    ) -> Self {
        let key_id = compute_key_id(&public_key);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            bundle_hash,
            merkle_root,
            signature,
            public_key,
            schema_ver: 1,
            signed_at_us: timestamp,
            key_id,
        }
    }

    /// Verify the signature
    pub fn verify(&self) -> Result<()> {
        self.public_key
            .verify(self.bundle_hash.as_bytes(), &self.signature)
            .map_err(|e| AosError::Crypto(format!("Bundle signature verification failed: {}", e)))
    }

    /// Save signature to file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load signature from file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let sig: BundleSignature = serde_json::from_str(&json)?;
        Ok(sig)
    }
}

/// Sign a bundle with the provided keypair
///
/// Per Artifacts Ruleset #13: Sign bundle_hash with Ed25519
/// Returns signature metadata including Merkle root
pub fn sign_bundle(
    bundle_hash: &B3Hash,
    merkle_root: &B3Hash,
    keypair: &Keypair,
) -> Result<BundleSignature> {
    // Sign the bundle hash
    let signature = keypair.sign(bundle_hash.as_bytes());
    let public_key = keypair.public_key();

    Ok(BundleSignature::new(
        *bundle_hash,
        *merkle_root,
        signature,
        public_key,
    ))
}

/// Sign a bundle and save signature to var/signatures/<bundle_hash>.sig
pub fn sign_and_save_bundle(
    bundle_hash: &B3Hash,
    merkle_root: &B3Hash,
    keypair: &Keypair,
    signatures_dir: &Path,
) -> Result<BundleSignature> {
    let signature = sign_bundle(bundle_hash, merkle_root, keypair)?;

    // Create signatures directory if needed
    fs::create_dir_all(signatures_dir)?;

    // Save signature
    let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));
    signature.save_to_file(&sig_path)?;

    tracing::info!(
        bundle_hash = %bundle_hash.to_hex(),
        sig_path = %sig_path.display(),
        key_id = %signature.key_id,
        "Bundle signed and saved"
    );

    Ok(signature)
}

/// Verify bundle signature from file
pub fn verify_bundle_from_file(
    bundle_hash: &B3Hash,
    signatures_dir: &Path,
) -> Result<BundleSignature> {
    let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));

    if !sig_path.exists() {
        return Err(AosError::Crypto(format!(
            "Signature file not found: {}",
            sig_path.display()
        )));
    }

    let signature = BundleSignature::load_from_file(&sig_path)?;

    // Verify bundle hash matches
    if signature.bundle_hash != *bundle_hash {
        return Err(AosError::Crypto(format!(
            "Bundle hash mismatch: expected {}, got {}",
            bundle_hash.to_hex(),
            signature.bundle_hash.to_hex()
        )));
    }

    // Verify signature
    signature.verify()?;

    tracing::info!(
        bundle_hash = %bundle_hash.to_hex(),
        key_id = %signature.key_id,
        "Bundle signature verified"
    );

    Ok(signature)
}

/// Compute deterministic key ID from public key
///
/// Per Secrets Ruleset #14: kid = blake3(pubkey)
pub fn compute_key_id(public_key: &PublicKey) -> String {
    let key_bytes = public_key.to_bytes();
    let hash = B3Hash::hash(&key_bytes);
    format!("kid-{}", &hash.to_hex()[..16])
}

/// Load signing keypair from file
///
/// In production, this would integrate with Secure Enclave
/// Per Secrets Ruleset #14: key material stored securely
pub fn load_signing_key(key_path: &Path) -> Result<Keypair> {
    if !key_path.exists() {
        return Err(AosError::Crypto(format!(
            "Signing key not found: {}",
            key_path.display()
        )));
    }

    let key_bytes = fs::read(key_path)?;

    if key_bytes.len() != 32 {
        return Err(AosError::Crypto(format!(
            "Invalid key length: {} (expected 32)",
            key_bytes.len()
        )));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);

    Ok(Keypair::from_bytes(&key_array))
}

/// Generate and save a new signing keypair
pub fn generate_signing_key(key_path: &Path) -> Result<Keypair> {
    let keypair = Keypair::generate();

    // Create parent directory if needed
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Save private key
    let key_bytes = keypair.to_bytes();
    fs::write(key_path, key_bytes)?;

    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(key_path)?.permissions();
        perms.set_mode(0o600); // Owner read/write only
        fs::set_permissions(key_path, perms)?;
    }

    let key_id = compute_key_id(&keypair.public_key());
    tracing::info!(
        key_path = %key_path.display(),
        key_id = %key_id,
        "Generated new signing keypair"
    );

    Ok(keypair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bundle_signing() {
        let keypair = Keypair::generate();
        let bundle_hash = B3Hash::hash(b"test_bundle");
        let merkle_root = B3Hash::hash(b"merkle_root");

        let signature = sign_bundle(&bundle_hash, &merkle_root, &keypair).unwrap();

        // Verify signature
        assert!(signature.verify().is_ok());
        assert_eq!(signature.bundle_hash, bundle_hash);
        assert_eq!(signature.merkle_root, merkle_root);
    }

    #[test]
    fn test_bundle_sign_and_save() {
        let temp_dir = TempDir::new().unwrap();
        let signatures_dir = temp_dir.path().join("signatures");

        let keypair = Keypair::generate();
        let bundle_hash = B3Hash::hash(b"test_bundle");
        let merkle_root = B3Hash::hash(b"merkle_root");

        // Sign and save
        let signature =
            sign_and_save_bundle(&bundle_hash, &merkle_root, &keypair, &signatures_dir).unwrap();

        // Verify file was created
        let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));
        assert!(sig_path.exists());

        // Verify from file
        let verified = verify_bundle_from_file(&bundle_hash, &signatures_dir).unwrap();
        assert_eq!(verified.bundle_hash, signature.bundle_hash);
        assert_eq!(verified.merkle_root, signature.merkle_root);
    }

    #[test]
    fn test_invalid_signature() {
        let keypair = Keypair::generate();
        let bundle_hash = B3Hash::hash(b"test_bundle");
        let merkle_root = B3Hash::hash(b"merkle_root");

        let mut signature = sign_bundle(&bundle_hash, &merkle_root, &keypair).unwrap();

        // Tamper with bundle hash
        signature.bundle_hash = B3Hash::hash(b"tampered");

        // Verification should fail
        assert!(signature.verify().is_err());
    }

    #[test]
    fn test_key_id_deterministic() {
        let keypair = Keypair::generate();
        let public_key = keypair.public_key();

        let key_id1 = compute_key_id(&public_key);
        let key_id2 = compute_key_id(&public_key);

        assert_eq!(key_id1, key_id2);
        assert!(key_id1.starts_with("kid-"));
    }

    #[test]
    fn test_generate_and_load_key() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("signing.key");

        // Generate key
        let keypair1 = generate_signing_key(&key_path).unwrap();
        assert!(key_path.exists());

        // Load key
        let keypair2 = load_signing_key(&key_path).unwrap();

        // Public keys should match
        assert_eq!(
            keypair1.public_key().to_bytes(),
            keypair2.public_key().to_bytes()
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_key_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("signing.key");

        generate_signing_key(&key_path).unwrap();

        let metadata = fs::metadata(&key_path).unwrap();
        let permissions = metadata.permissions();

        // Should be owner read/write only (0o600)
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}

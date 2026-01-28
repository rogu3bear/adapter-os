//! Bundle signing and verification for telemetry bundles
//!
//! Per Artifacts Ruleset #13: All bundles must be signed with Ed25519
//! Provides cryptographic chain-of-custody for all telemetry data

use crate::{Keypair, PublicKey, Signature};
use adapteros_core::{AosError, B3Hash, Result};
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
            .unwrap_or_default()
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
            .map_err(|e| {
                tracing::warn!(
                    target: "security.bundle",
                    bundle_id = %self.bundle_hash.to_hex(),
                    key_id = %self.key_id,
                    "Bundle signature verification failed - possible tampering"
                );
                AosError::Crypto(format!("Bundle signature verification failed: {}", e))
            })
    }

    /// Save signature to file
    ///
    /// Uses atomic write (temp-then-rename) and sets 0644 permissions on Unix.
    /// Signature files are public (contain public key), so they should be
    /// world-readable but not writable.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        let json = serde_json::to_string_pretty(self)?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Generate unique temp file name
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = path.with_extension(format!("sig.tmp.{}", nanos));

        // Write to temp file with proper permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644) // World-readable, owner-writable
                .open(&temp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        #[cfg(not(unix))]
        {
            fs::write(&temp_path, &json)?;
        }

        // Atomic rename
        fs::rename(&temp_path, path).inspect_err(|_e| {
            let _ = fs::remove_file(&temp_path);
        })?;

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

/// Check if signature bypass is enabled via environment variable.
///
/// SECURITY: This is ONLY available in debug builds. In release builds,
/// this function always returns false regardless of environment variables.
/// Set `AOS_DEV_SIGNATURE_BYPASS=1` to enable bypass mode in debug builds.
/// Even with bypass enabled, warnings are always logged.
#[cfg(debug_assertions)]
fn is_signature_bypass_enabled() -> bool {
    use std::sync::OnceLock;
    static BYPASS: OnceLock<bool> = OnceLock::new();
    *BYPASS.get_or_init(|| {
        let enabled = std::env::var("AOS_DEV_SIGNATURE_BYPASS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if enabled {
            tracing::warn!(
                target: "security.bundle",
                "AOS_DEV_SIGNATURE_BYPASS enabled (debug build only)"
            );
        }
        enabled
    })
}

/// In release builds, signature bypass is NEVER enabled.
/// The environment variable is ignored entirely for security.
#[cfg(not(debug_assertions))]
fn is_signature_bypass_enabled() -> bool {
    use std::sync::OnceLock;
    static LOGGED: OnceLock<()> = OnceLock::new();
    LOGGED.get_or_init(|| {
        if std::env::var("AOS_DEV_SIGNATURE_BYPASS").is_ok() {
            tracing::error!(
                target: "security.bundle",
                "AOS_DEV_SIGNATURE_BYPASS detected in RELEASE build - IGNORED for security"
            );
        }
    });
    false // Always false in release builds
}

/// Verify bundle signature from file
///
/// Signature verification is strict by default. All signature failures result in errors.
///
/// For development/testing only: Set `AOS_DEV_SIGNATURE_BYPASS=1` environment variable
/// to allow verification to pass with warnings instead of hard failures.
/// This is a runtime flag, not a compile-time flag, to ensure explicit opt-in.
pub fn verify_bundle_from_file(
    bundle_hash: &B3Hash,
    signatures_dir: &Path,
) -> Result<BundleSignature> {
    let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));
    let bypass_enabled = is_signature_bypass_enabled();

    if !sig_path.exists() {
        tracing::warn!(
            target: "security.bundle",
            bundle_hash = %bundle_hash.to_hex(),
            sig_path = %sig_path.display(),
            bypass_enabled = bypass_enabled,
            "Bundle signature file not found - SECURITY VIOLATION"
        );

        if bypass_enabled {
            tracing::error!(
                target: "security.bundle",
                "AOS_DEV_SIGNATURE_BYPASS enabled - returning placeholder (DO NOT USE IN PRODUCTION)"
            );
            // Return a placeholder signature for dev mode ONLY when explicitly enabled
            let placeholder_keypair = crate::Keypair::generate();
            let placeholder_sig = BundleSignature {
                bundle_hash: *bundle_hash,
                merkle_root: B3Hash::hash(b"dev-mode-placeholder"),
                signature: placeholder_keypair.sign(b"dev-mode-placeholder"),
                public_key: placeholder_keypair.public_key(),
                schema_ver: 1,
                signed_at_us: 0,
                key_id: "dev-mode-bypass".to_string(),
            };
            return Ok(placeholder_sig);
        }

        return Err(AosError::Crypto(format!(
            "Signature file not found: {}",
            sig_path.display()
        )));
    }

    let signature = BundleSignature::load_from_file(&sig_path)?;

    // Verify bundle hash matches
    if signature.bundle_hash != *bundle_hash {
        tracing::warn!(
            target: "security.bundle",
            bundle_hash = %bundle_hash.to_hex(),
            expected = %bundle_hash.to_hex(),
            got = %signature.bundle_hash.to_hex(),
            bypass_enabled = bypass_enabled,
            "Bundle hash mismatch - POSSIBLE TAMPERING"
        );

        if !bypass_enabled {
            return Err(AosError::Crypto(format!(
                "Bundle hash mismatch: expected {}, got {}",
                bundle_hash.to_hex(),
                signature.bundle_hash.to_hex()
            )));
        }

        tracing::error!(
            target: "security.bundle",
            "AOS_DEV_SIGNATURE_BYPASS enabled - continuing despite hash mismatch (DO NOT USE IN PRODUCTION)"
        );
    }

    // Verify signature
    match signature.verify() {
        Ok(_) => {
            tracing::info!(
                bundle_hash = %bundle_hash.to_hex(),
                key_id = %signature.key_id,
                "Bundle signature verified"
            );
            Ok(signature)
        }
        Err(e) => {
            tracing::warn!(
                target: "security.bundle",
                bundle_hash = %bundle_hash.to_hex(),
                error = %e,
                bypass_enabled = bypass_enabled,
                "Bundle signature verification FAILED - POSSIBLE TAMPERING"
            );

            if bypass_enabled {
                tracing::error!(
                    target: "security.bundle",
                    "AOS_DEV_SIGNATURE_BYPASS enabled - returning invalid signature (DO NOT USE IN PRODUCTION)"
                );
                Ok(signature)
            } else {
                Err(e)
            }
        }
    }
}

/// Compute deterministic key ID from public key
///
/// Per Secrets Ruleset #14: kid = blake3(pubkey)
/// Uses 32 hex characters (128 bits) to avoid birthday-bound collisions.
pub fn compute_key_id(public_key: &PublicKey) -> String {
    let key_bytes = public_key.to_bytes();
    let hash = B3Hash::hash(&key_bytes);
    format!("kid-{}", &hash.to_hex()[..32])
}

/// Load signing keypair from file
///
/// In production, this would integrate with Secure Enclave
/// Per Secrets Ruleset #14: key material stored securely
///
/// This function validates file permissions on Unix systems and warns if
/// the private key is world-readable (security risk).
pub fn load_signing_key(key_path: &Path) -> Result<Keypair> {
    if !key_path.exists() {
        return Err(AosError::Crypto(format!(
            "Signing key not found: {}",
            key_path.display()
        )));
    }

    // Validate file permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(key_path) {
            let mode = metadata.permissions().mode();
            // Check if world or group readable/writable (bits 077)
            if mode & 0o077 != 0 {
                tracing::warn!(
                    path = %key_path.display(),
                    mode = format!("{:o}", mode & 0o777),
                    "Private key file has loose permissions (should be 0600). \
                     This is a security risk - the key may be readable by other users."
                );
            }
        }
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

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("TempDir::with_prefix() failed: could not create temporary directory for test. This indicates insufficient disk space, permission issues in system temp directory (check TMPDIR env var), or OS resource limits exceeded.")
    }

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
        let temp_dir = new_test_tempdir();
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
        let temp_dir = new_test_tempdir();
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

        let temp_dir = new_test_tempdir();
        let key_path = temp_dir.path().join("signing.key");

        generate_signing_key(&key_path).unwrap();

        let metadata = fs::metadata(&key_path).unwrap();
        let permissions = metadata.permissions();

        // Should be owner read/write only (0o600)
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}

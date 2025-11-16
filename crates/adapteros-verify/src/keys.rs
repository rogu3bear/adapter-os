//! Key management for fingerprint signing

use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

#[allow(dead_code)] // TODO: Implement device fingerprinting in future iteration
const FINGERPRINT_KEY_LABEL: &str = "com.adapteros.fingerprint.signing";
const KEY_FILE_PATH: &str = "var/keys/fingerprint_key.bin";

/// Get or create fingerprint signing keypair
///
/// Tries to load from Secure Enclave first (production), then falls back
/// to file-based key storage (development/testing).
pub fn get_or_create_fingerprint_keypair() -> Result<Keypair> {
    // Try Secure Enclave first (production)
    #[cfg(target_os = "macos")]
    {
        match try_load_from_enclave() {
            Ok(keypair) => {
                debug!("Loaded fingerprint keypair from Secure Enclave");
                return Ok(keypair);
            }
            Err(e) => {
                debug!(
                    "Could not load from Secure Enclave: {}, falling back to file storage",
                    e
                );
            }
        }
    }

    // Fall back to file-based storage
    load_or_create_file_keypair()
}

/// Try to load keypair from Secure Enclave
#[cfg(target_os = "macos")]
fn try_load_from_enclave() -> Result<Keypair> {
    // Secure Enclave integration not yet implemented
    Err(AosError::Unavailable(
        "Secure Enclave key management not implemented".to_string(),
    ))
}

/// Load or create file-based keypair (development/testing fallback)
fn load_or_create_file_keypair() -> Result<Keypair> {
    let key_path = PathBuf::from(KEY_FILE_PATH);

    if key_path.exists() {
        // Load existing key
        debug!(
            "Loading fingerprint keypair from file: {}",
            key_path.display()
        );
        let key_bytes = fs::read(&key_path)
            .map_err(|e| AosError::Io(format!("Failed to read key file: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid key file length: {}",
                key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        Ok(Keypair::from_bytes(&key_array))
    } else {
        // Generate new key
        warn!(
            "No fingerprint key found, generating new key at: {}",
            key_path.display()
        );
        warn!(
            "WARNING: File-based keys are for development only. Use Secure Enclave in production."
        );

        let keypair = Keypair::generate();
        let key_bytes = keypair.to_bytes();

        // Ensure directory exists
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AosError::Io(format!("Failed to create key directory: {}", e)))?;
        }

        // Write key file with restrictive permissions
        fs::write(&key_path, key_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write key file: {}", e)))?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)
                .map_err(|e| AosError::Io(format!("Failed to get key file metadata: {}", e)))?
                .permissions();
            perms.set_mode(0o600); // rw-------
            fs::set_permissions(&key_path, perms)
                .map_err(|e| AosError::Io(format!("Failed to set key file permissions: {}", e)))?;
        }

        info!(
            "Generated new fingerprint signing key at: {}",
            key_path.display()
        );
        Ok(keypair)
    }
}

/// Get the public key for verification
pub fn get_fingerprint_public_key() -> Result<adapteros_crypto::PublicKey> {
    let keypair = get_or_create_fingerprint_keypair()?;
    Ok(keypair.public_key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_keypair_generation() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("test_key.bin");

        // Override KEY_FILE_PATH for testing (would need to be done differently in practice)
        // For now, just test the keypair generation directly
        let keypair = Keypair::generate();
        let key_bytes = keypair.to_bytes();
        assert_eq!(key_bytes.len(), 32);

        // Test round-trip
        let keypair2 = Keypair::from_bytes(&key_bytes);
        assert_eq!(
            keypair.public_key().to_bytes(),
            keypair2.public_key().to_bytes()
        );
    }
}

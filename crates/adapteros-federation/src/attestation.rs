//! Secure Enclave Attestation for Federation Bundles
//!
//! Provides hardware-backed signing with Secure Enclave integration.
//! Per Secrets Ruleset (#14): keys must be backed by Secure Enclave.

use adapteros_core::{AosError, Result};
use adapteros_crypto::Signature;
use serde::{Deserialize, Serialize};
use hex;

#[cfg(target_os = "macos")]
use adapteros_secd::EnclaveManager;

/// Attestation metadata for federation bundles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationInfo {
    /// Whether hardware attestation was used
    pub hardware_backed: bool,
    /// Enclave identifier (if hardware-backed)
    pub enclave_id: Option<String>,
    /// Attestation timestamp
    pub attested_at: String,
    /// Signature algorithm
    pub algorithm: String,
}

/// Attest and sign a federation bundle with Secure Enclave
///
/// This function attempts to use the Secure Enclave for signing if available.
/// Falls back to software signing if hardware is not available.
///
/// # Arguments
///
/// * `payload` - The data to sign
///
/// # Returns
///
/// A tuple of (Signature, AttestationInfo)
#[cfg(target_os = "macos")]
pub fn attest_bundle(payload: &[u8]) -> Result<(Signature, AttestationInfo)> {
    use tracing::{debug, info};

    // Attempt to use Secure Enclave
    match EnclaveManager::new() {
        Ok(mut enclave) => {
            debug!("Using Secure Enclave for federation bundle signing");

            // Sign the payload using Secure Enclave
            let signature_bytes = enclave
                .sign_bundle(payload)
                .map_err(|e| AosError::Crypto(format!("Secure Enclave signing failed: {}", e)))?;

            // Convert to adapteros_crypto::Signature format (requires exactly 64 bytes)
            if signature_bytes.len() != 64 {
                return Err(AosError::Crypto(format!(
                    "Secure Enclave signature has unexpected length: {} (expected 64)",
                    signature_bytes.len()
                )));
            }
            let mut sig_array = [0u8; 64];
            sig_array.copy_from_slice(&signature_bytes);
            let signature = Signature::from_bytes(&sig_array)
                .map_err(|e| AosError::Crypto(format!("Invalid signature format: {}", e)))?;

            // Generate enclave identifier from public key
            let public_key = enclave
                .get_public_key("aos_bundle_signing")
                .map_err(|e| AosError::Crypto(format!("Failed to get public key: {}", e)))?;
            let enclave_id = hex::encode(&public_key[..8]); // Use first 8 bytes as ID

            let attestation = AttestationInfo {
                hardware_backed: true,
                enclave_id: Some(enclave_id.clone()),
                attested_at: chrono::Utc::now().to_rfc3339(),
                algorithm: "ECDSA-P256-SecureEnclave".to_string(),
            };

            info!(
                enclave_id = %enclave_id,
                "Federation bundle signed with Secure Enclave"
            );

            Ok((signature, attestation))
        }
        Err(e) => {
            debug!(
                error = %e,
                "Secure Enclave unavailable, falling back to software signing"
            );
            
            // Fallback to software signing
            use adapteros_crypto::Keypair;
            let keypair = Keypair::generate();
            let signature = keypair.sign(payload);

            let attestation = AttestationInfo {
                hardware_backed: false,
                enclave_id: None,
                attested_at: chrono::Utc::now().to_rfc3339(),
                algorithm: "Ed25519-Software".to_string(),
            };

            Ok((signature, attestation))
        }
    }
}

/// Non-macOS platforms: always use software signing
#[cfg(not(target_os = "macos"))]
pub fn attest_bundle(payload: &[u8]) -> Result<(Signature, AttestationInfo)> {
    use adapteros_crypto::Keypair;
    use tracing::debug;

    debug!("Secure Enclave not available on this platform, using software signing");

    let keypair = Keypair::generate();
    let signature = keypair.sign(payload);

    let attestation = AttestationInfo {
        hardware_backed: false,
        enclave_id: None,
        attested_at: chrono::Utc::now().to_rfc3339(),
        algorithm: "Ed25519-Software".to_string(),
    };

    Ok((signature, attestation))
}

/// Verify that a bundle was attested with hardware backing
pub fn verify_hardware_attestation(attestation: &AttestationInfo) -> Result<()> {
    if !attestation.hardware_backed {
        return Err(AosError::PolicyViolation(
            "Bundle not hardware-attested (Secrets Ruleset #14)".to_string(),
        ));
    }

    if attestation.enclave_id.is_none() {
        return Err(AosError::PolicyViolation(
            "Hardware attestation missing enclave ID".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attest_bundle() {
        let payload = b"test federation bundle";
        let result = attest_bundle(payload);
        
        assert!(result.is_ok());
        
        let (_signature, attestation) = result.unwrap();
        assert!(!attestation.attested_at.is_empty());
        assert!(!attestation.algorithm.is_empty());
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_software_fallback() {
        let payload = b"test bundle";
        let (_sig, attestation) = attest_bundle(payload).unwrap();
        
        assert!(!attestation.hardware_backed);
        assert!(attestation.enclave_id.is_none());
        assert_eq!(attestation.algorithm, "Ed25519-Software");
    }

    #[test]
    fn test_verify_hardware_attestation() {
        let hw_attestation = AttestationInfo {
            hardware_backed: true,
            enclave_id: Some("test-enclave".to_string()),
            attested_at: chrono::Utc::now().to_rfc3339(),
            algorithm: "Ed25519-SecureEnclave".to_string(),
        };

        assert!(verify_hardware_attestation(&hw_attestation).is_ok());

        let sw_attestation = AttestationInfo {
            hardware_backed: false,
            enclave_id: None,
            attested_at: chrono::Utc::now().to_rfc3339(),
            algorithm: "Ed25519-Software".to_string(),
        };

        assert!(verify_hardware_attestation(&sw_attestation).is_err());
    }
}


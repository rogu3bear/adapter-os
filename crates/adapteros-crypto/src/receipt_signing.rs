//! Receipt Signing with Fail-Closed Semantics
//!
//! Provides canonical receipt signing for inference traces. In production mode,
//! receipts MUST be signed; unsigned receipts are rejected.
//!
//! # Signing Boundary Hardening (PRD-06)
//!
//! This module enforces the principle that **no receipt escapes unsigned**:
//!
//! 1. `sign_receipt_digest`: Signs a receipt digest with Ed25519
//! 2. `SigningMode::Production`: Requires signing, fails if no keypair
//! 3. `SigningMode::Development`: Allows unsigned receipts (dev/test only)
//!
//! # Example
//!
//! ```ignore
//! use adapteros_crypto::receipt_signing::{sign_receipt_digest, SigningMode};
//!
//! let digest = [0u8; 32];
//! let keypair = Keypair::generate();
//!
//! // Production: signing required
//! let signed = sign_receipt_digest(&digest, Some(&keypair), SigningMode::Production)?;
//! assert!(signed.signature.is_some());
//!
//! // Development: signing optional
//! let unsigned = sign_receipt_digest(&digest, None, SigningMode::Development)?;
//! assert!(unsigned.signature.is_none());
//! ```

use crate::signature::{Keypair, Signature};
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Signing mode for receipt operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SigningMode {
    /// Production mode: signing REQUIRED. Unsigned receipts are rejected.
    /// This is the default for safety (fail-closed).
    #[default]
    Production,
    /// Development mode: signing optional. Use for testing only.
    Development,
}

impl SigningMode {
    /// Check if this mode requires signing
    pub fn requires_signing(&self) -> bool {
        matches!(self, Self::Production)
    }

    /// Parse from environment variable or config string
    pub fn parse_mode(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "development" | "dev" | "test" => Self::Development,
            _ => Self::Production,
        }
    }

    /// Get mode from environment variable AOS_SIGNING_MODE
    pub fn from_env() -> Self {
        std::env::var("AOS_SIGNING_MODE")
            .map(|v| Self::parse_mode(&v))
            .unwrap_or(Self::Production)
    }
}

/// A signed receipt containing the digest and optional signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedReceipt {
    /// The receipt digest (BLAKE3 hash, 32 bytes)
    pub digest: B3Hash,
    /// Ed25519 signature over the digest (64 bytes, base64-encoded for JSON)
    /// None only in Development mode
    pub signature: Option<Signature>,
    /// Public key used for signing (32 bytes, hex-encoded)
    pub public_key_hex: Option<String>,
    /// Signing mode used
    pub mode: SigningMode,
}

impl SignedReceipt {
    /// Check if this receipt is signed
    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Verify the signature (if present) against the digest
    pub fn verify(&self) -> Result<bool> {
        let Some(ref sig) = self.signature else {
            return Ok(false);
        };
        let Some(ref pk_hex) = self.public_key_hex else {
            return Err(AosError::crypto("Signature present but public key missing"));
        };

        let pk_bytes = hex::decode(pk_hex)
            .map_err(|e| AosError::crypto(format!("Invalid public key hex: {}", e)))?;

        let pk_array: [u8; 32] = pk_bytes
            .try_into()
            .map_err(|_| AosError::crypto("Public key wrong length: expected 32 bytes"))?;

        let pk = crate::signature::PublicKey::from_bytes(&pk_array)?;
        Ok(pk.verify(self.digest.as_bytes(), sig).is_ok())
    }

    /// Get signature as base64 string
    pub fn signature_b64(&self) -> Option<String> {
        self.signature.as_ref().map(|s| {
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, s.to_bytes())
        })
    }

    /// Get signature as raw bytes
    pub fn signature_bytes(&self) -> Option<Vec<u8>> {
        self.signature.as_ref().map(|s| s.to_bytes().to_vec())
    }
}

/// Sign a receipt digest with the provided keypair.
///
/// # Arguments
/// * `digest` - The 32-byte BLAKE3 receipt digest
/// * `keypair` - Optional signing keypair
/// * `mode` - Signing mode (Production requires keypair)
///
/// # Returns
/// `SignedReceipt` containing the digest and signature
///
/// # Errors
/// * In Production mode, returns error if keypair is None
/// * Returns error if signing fails
///
/// # Security
/// - Production mode enforces fail-closed: no keypair = error
/// - Development mode allows unsigned for testing
/// - Signature covers only the digest (not raw receipt data)
pub fn sign_receipt_digest(
    digest: &B3Hash,
    keypair: Option<&Keypair>,
    mode: SigningMode,
) -> Result<SignedReceipt> {
    // Fail-closed in production: no keypair = error
    if mode.requires_signing() && keypair.is_none() {
        tracing::error!("Receipt signing REQUIRED in production mode but no keypair provided");
        // Emit telemetry event for observability
        let event = adapteros_core::telemetry::strict_mode_failure_event(
            "Receipt signing required but no keypair available",
            Some("receipt.signing".to_string()),
            false,
            None,
            None,
        );
        adapteros_core::telemetry::emit_observability_event(&event);

        return Err(AosError::DeterminismViolation(
            "Receipt signing REQUIRED in production mode. \
             Configure AOS_SIGNING_KEY_PATH or set AOS_SIGNING_MODE=development for testing."
                .to_string(),
        ));
    }

    match keypair {
        Some(kp) => {
            let signature = kp.sign(digest.as_bytes());
            let public_key_hex = hex::encode(kp.public_key().to_bytes());

            tracing::debug!(
                public_key_prefix = %public_key_hex.get(..16).unwrap_or(&public_key_hex),
                "Receipt signed successfully"
            );

            Ok(SignedReceipt {
                digest: *digest,
                signature: Some(signature),
                public_key_hex: Some(public_key_hex),
                mode,
            })
        }
        None => {
            // Development mode only - allow unsigned
            tracing::warn!(
                mode = ?mode,
                "Receipt generated without signature (development mode)"
            );

            Ok(SignedReceipt {
                digest: *digest,
                signature: None,
                public_key_hex: None,
                mode,
            })
        }
    }
}

/// Sign a receipt digest using raw bytes
pub fn sign_receipt_digest_bytes(
    digest_bytes: &[u8; 32],
    keypair: Option<&Keypair>,
    mode: SigningMode,
) -> Result<SignedReceipt> {
    let digest = B3Hash::from_bytes(*digest_bytes);
    sign_receipt_digest(&digest, keypair, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_mode_requires_keypair() {
        let digest = B3Hash::hash(b"test receipt");

        // Production without keypair should fail
        let result = sign_receipt_digest(&digest, None, SigningMode::Production);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Receipt signing REQUIRED"));
    }

    #[test]
    fn test_development_mode_allows_unsigned() {
        let digest = B3Hash::hash(b"test receipt");

        // Development without keypair should succeed
        let result = sign_receipt_digest(&digest, None, SigningMode::Development);
        assert!(result.is_ok());
        let signed = result.unwrap();
        assert!(!signed.is_signed());
    }

    #[test]
    fn test_production_mode_signs_with_keypair() {
        let digest = B3Hash::hash(b"test receipt");
        let keypair = Keypair::generate();

        let result = sign_receipt_digest(&digest, Some(&keypair), SigningMode::Production);
        assert!(result.is_ok());
        let signed = result.unwrap();
        assert!(signed.is_signed());
        assert!(signed.signature.is_some());
        assert!(signed.public_key_hex.is_some());
    }

    #[test]
    fn test_signature_verification() {
        let digest = B3Hash::hash(b"test receipt");
        let keypair = Keypair::generate();

        let signed = sign_receipt_digest(&digest, Some(&keypair), SigningMode::Production).unwrap();

        // Verification should pass
        assert!(signed.verify().unwrap());
    }

    #[test]
    fn test_signing_mode_from_env() {
        // Default should be Production
        std::env::remove_var("AOS_SIGNING_MODE");
        assert_eq!(SigningMode::from_env(), SigningMode::Production);

        // Set to development
        std::env::set_var("AOS_SIGNING_MODE", "development");
        assert_eq!(SigningMode::from_env(), SigningMode::Development);

        // Clean up
        std::env::remove_var("AOS_SIGNING_MODE");
    }

    #[test]
    fn test_signing_mode_default() {
        // Default should be Production (fail-closed)
        assert_eq!(SigningMode::default(), SigningMode::Production);
    }
}

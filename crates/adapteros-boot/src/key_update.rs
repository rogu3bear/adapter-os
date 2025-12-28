//! Key update message types for control plane to worker key distribution.
//!
//! This module provides message types for securely pushing key updates
//! from the control plane to workers during key rotation.
//!
//! ## Security Model
//!
//! Key updates are signed by the OLD key to prove authenticity:
//! 1. Control plane rotates its signing key
//! 2. Control plane signs the update message with the OLD key
//! 3. Workers validate the signature using their currently trusted key
//! 4. Workers add the new key to their key ring
//! 5. Old key remains valid for the grace period
//!
//! ## Replay Protection
//!
//! Each update includes:
//! - `nonce`: Unique identifier to prevent replay
//! - `issued_at`: Timestamp to reject stale updates
//!
//! Workers must track seen nonces and reject duplicates.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::error::WorkerAuthError;

/// Maximum age of a key update request in seconds.
/// Updates older than this are rejected.
pub const KEY_UPDATE_MAX_AGE_SECS: i64 = 300;

/// Current protocol version for key updates.
pub const KEY_UPDATE_PROTOCOL_VERSION: u8 = 1;

/// Request to update a worker's verifying keys.
///
/// This message is signed by the OLD key to prove authenticity.
/// Workers validate the signature before accepting the new key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUpdateRequest {
    /// Protocol version (currently 1)
    pub version: u8,

    /// The new public key (32 bytes, base64-encoded)
    pub new_public_key: String,

    /// Key ID of the new key
    pub new_kid: String,

    /// Key ID of the old/current key (used for signature verification)
    pub old_kid: String,

    /// Grace period in seconds for the old key
    pub grace_period_secs: u64,

    /// Unix timestamp when this update was issued
    pub issued_at: i64,

    /// Nonce for replay protection (UUID)
    pub nonce: String,

    /// Signature of the message signed by OLD key (base64-encoded)
    pub signature: String,
}

/// Response from key update endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUpdateResponse {
    /// Whether the update was applied successfully
    pub success: bool,

    /// New primary key ID (if success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_kid: Option<String>,

    /// Number of keys now in the ring
    pub key_count: usize,

    /// Error message (if failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl KeyUpdateRequest {
    /// Create a new key update request.
    ///
    /// # Arguments
    ///
    /// * `old_signing_key` - The old (current) signing key to sign with
    /// * `old_kid` - Key ID of the old key
    /// * `new_verifying_key` - The new public key to distribute
    /// * `new_kid` - Key ID of the new key
    /// * `grace_period_secs` - How long the old key remains valid
    pub fn new(
        old_signing_key: &SigningKey,
        old_kid: &str,
        new_verifying_key: &VerifyingKey,
        new_kid: &str,
        grace_period_secs: u64,
    ) -> Result<Self, WorkerAuthError> {
        let now = chrono::Utc::now().timestamp();
        // Generate a random 16-byte nonce as base64 string
        let nonce_bytes: [u8; 16] = rand::thread_rng().gen();
        let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);

        // Encode new public key as base64
        let new_public_key = URL_SAFE_NO_PAD.encode(new_verifying_key.as_bytes());

        // Create the message to sign (deterministic JSON)
        let message = serde_json::json!({
            "version": KEY_UPDATE_PROTOCOL_VERSION,
            "new_public_key": new_public_key,
            "new_kid": new_kid,
            "old_kid": old_kid,
            "grace_period_secs": grace_period_secs,
            "issued_at": now,
            "nonce": nonce,
        });

        let message_bytes = serde_json::to_vec(&message)
            .map_err(|e| WorkerAuthError::Serialization(e.to_string()))?;

        // Sign with old key
        let signature = old_signing_key.sign(&message_bytes);
        let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(Self {
            version: KEY_UPDATE_PROTOCOL_VERSION,
            new_public_key,
            new_kid: new_kid.to_string(),
            old_kid: old_kid.to_string(),
            grace_period_secs,
            issued_at: now,
            nonce,
            signature: signature_b64,
        })
    }

    /// Verify the signature of this request using the old key.
    ///
    /// # Arguments
    ///
    /// * `old_verifying_key` - The old public key to verify against
    pub fn verify_signature(
        &self,
        old_verifying_key: &VerifyingKey,
    ) -> Result<(), WorkerAuthError> {
        // Reconstruct the signed message
        let message = serde_json::json!({
            "version": self.version,
            "new_public_key": self.new_public_key,
            "new_kid": self.new_kid,
            "old_kid": self.old_kid,
            "grace_period_secs": self.grace_period_secs,
            "issued_at": self.issued_at,
            "nonce": self.nonce,
        });

        let message_bytes = serde_json::to_vec(&message)
            .map_err(|e| WorkerAuthError::Serialization(e.to_string()))?;

        // Decode signature
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(&self.signature)
            .map_err(|e| WorkerAuthError::Base64Decode(e.to_string()))?;

        let signature_array: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| WorkerAuthError::InvalidSignature)?;

        let signature = Signature::from_bytes(&signature_array);

        // Verify
        old_verifying_key
            .verify(&message_bytes, &signature)
            .map_err(|_| WorkerAuthError::InvalidSignature)
    }

    /// Check if this request is within the acceptable time window.
    pub fn is_valid_time(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        (now - self.issued_at).abs() <= KEY_UPDATE_MAX_AGE_SECS
    }

    /// Decode the new public key from this request.
    pub fn decode_new_public_key(&self) -> Result<VerifyingKey, WorkerAuthError> {
        let key_bytes = URL_SAFE_NO_PAD
            .decode(&self.new_public_key)
            .map_err(|e| WorkerAuthError::Base64Decode(e.to_string()))?;

        let key_array: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| WorkerAuthError::InvalidKeyFormat("public key must be 32 bytes".into()))?;

        VerifyingKey::from_bytes(&key_array)
            .map_err(|e| WorkerAuthError::InvalidKeyFormat(e.to_string()))
    }
}

impl KeyUpdateResponse {
    /// Create a success response.
    pub fn success(new_kid: String, key_count: usize) -> Self {
        Self {
            success: true,
            new_kid: Some(new_kid),
            key_count,
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(error: String, key_count: usize) -> Self {
        Self {
            success: false,
            new_kid: None,
            key_count,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_update_request_creation_and_verification() {
        // Generate old and new keys
        let old_signing_key = SigningKey::generate(&mut rand::thread_rng());
        let new_signing_key = SigningKey::generate(&mut rand::thread_rng());

        let old_kid = crate::derive_kid_from_verifying_key(&old_signing_key.verifying_key());
        let new_kid = crate::derive_kid_from_verifying_key(&new_signing_key.verifying_key());

        // Create update request
        let request = KeyUpdateRequest::new(
            &old_signing_key,
            &old_kid,
            &new_signing_key.verifying_key(),
            &new_kid,
            300,
        )
        .unwrap();

        // Verify signature
        assert!(request
            .verify_signature(&old_signing_key.verifying_key())
            .is_ok());

        // Verify time is valid
        assert!(request.is_valid_time());

        // Decode new public key
        let decoded_key = request.decode_new_public_key().unwrap();
        assert_eq!(decoded_key, new_signing_key.verifying_key());
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let old_signing_key = SigningKey::generate(&mut rand::thread_rng());
        let new_signing_key = SigningKey::generate(&mut rand::thread_rng());
        let wrong_key = SigningKey::generate(&mut rand::thread_rng());

        let old_kid = crate::derive_kid_from_verifying_key(&old_signing_key.verifying_key());
        let new_kid = crate::derive_kid_from_verifying_key(&new_signing_key.verifying_key());

        let request = KeyUpdateRequest::new(
            &old_signing_key,
            &old_kid,
            &new_signing_key.verifying_key(),
            &new_kid,
            300,
        )
        .unwrap();

        // Verification with wrong key should fail
        assert!(request
            .verify_signature(&wrong_key.verifying_key())
            .is_err());
    }

    #[test]
    fn test_response_types() {
        let success = KeyUpdateResponse::success("worker-new-kid".into(), 2);
        assert!(success.success);
        assert_eq!(success.new_kid, Some("worker-new-kid".into()));
        assert!(success.error.is_none());

        let error = KeyUpdateResponse::error("something went wrong".into(), 1);
        assert!(!error.success);
        assert!(error.new_kid.is_none());
        assert_eq!(error.error, Some("something went wrong".into()));
    }
}

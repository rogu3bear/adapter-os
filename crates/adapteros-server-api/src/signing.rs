//! Ed25519 signing for promotion records

use anyhow::Result;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey};
use std::env;

/// Sign a promotion record with Ed25519
pub fn sign_promotion(
    cpid: &str,
    promoted_by: &str,
    quality_json: &str,
) -> Result<(String, String)> {
    // Load signing key from environment
    let key_hex = env::var("PROMOTION_SIGNING_KEY").unwrap_or_else(|_| {
        // Development fallback - deterministic key
        "0000000000000000000000000000000000000000000000000000000000000001".to_string()
    });

    let key_bytes =
        hex::decode(&key_hex).map_err(|e| anyhow::anyhow!("Invalid signing key hex: {}", e))?;

    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("Signing key must be 32 bytes"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    let signing_key = SigningKey::from_bytes(&key_array);

    // Create message to sign
    let message = format!("{}:{}:{}", cpid, promoted_by, quality_json);

    // Sign
    let signature: Signature = signing_key.sign(message.as_bytes());

    // Base64 encode signature
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    // Generate key ID (first 8 chars of public key hex)
    let public_key = signing_key.verifying_key();
    let key_id = format!("key-{}", &hex::encode(public_key.as_bytes())[..8]);

    Ok((signature_b64, key_id))
}

/// Verify a promotion signature
pub fn verify_promotion_signature(
    cpid: &str,
    promoted_by: &str,
    quality_json: &str,
    signature_b64: &str,
    _key_id: &str,
) -> Result<bool> {
    use ed25519_dalek::Verifier;

    // Load signing key to get public key
    let key_hex = env::var("PROMOTION_SIGNING_KEY").unwrap_or_else(|_| {
        "0000000000000000000000000000000000000000000000000000000000000001".to_string()
    });

    let key_bytes = hex::decode(&key_hex)?;
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    let signing_key = SigningKey::from_bytes(&key_array);
    let public_key = signing_key.verifying_key();

    // Decode signature
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| anyhow::anyhow!("Invalid base64 signature: {}", e))?;

    let signature = Signature::from_bytes(
        sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
    );

    // Recreate message
    let message = format!("{}:{}:{}", cpid, promoted_by, quality_json);

    // Verify
    Ok(public_key.verify(message.as_bytes(), &signature).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let cpid = "cp-test-001";
        let promoted_by = "user@example.com";
        let quality_json = r#"{"arr":0.95,"ecs5":0.80,"hlr":0.02,"cr":0.01}"#;

        let (signature, key_id) =
            sign_promotion(cpid, promoted_by, quality_json).expect("Signing failed");

        assert!(!signature.is_empty());
        assert!(key_id.starts_with("key-"));

        let verified =
            verify_promotion_signature(cpid, promoted_by, quality_json, &signature, &key_id)
                .expect("Verification failed");

        assert!(verified, "Signature verification failed");
    }

    #[test]
    fn test_verify_tampered_fails() {
        let cpid = "cp-test-001";
        let promoted_by = "user@example.com";
        let quality_json = r#"{"arr":0.95,"ecs5":0.80,"hlr":0.02,"cr":0.01}"#;

        let (signature, key_id) =
            sign_promotion(cpid, promoted_by, quality_json).expect("Signing failed");

        // Tamper with data
        let tampered_json = r#"{"arr":0.50,"ecs5":0.50,"hlr":0.50,"cr":0.50}"#;

        let verified =
            verify_promotion_signature(cpid, promoted_by, tampered_json, &signature, &key_id)
                .expect("Verification failed");

        assert!(!verified, "Tampered signature should not verify");
    }
}

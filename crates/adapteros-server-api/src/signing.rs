//! Ed25519 signing for promotion records

use anyhow::Result;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey};
use std::env;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const PROMOTION_KEY_PATH: &str = "var/keys/promotion_signing.key";

/// Cached key to avoid repeated file I/O
static CACHED_KEY: OnceLock<String> = OnceLock::new();

/// Get or create the promotion signing key.
///
/// Priority:
/// 1. `PROMOTION_SIGNING_KEY` environment variable
/// 2. Persisted key at `var/keys/promotion_signing.key`
/// 3. Generate new key and persist it
fn get_or_create_promotion_key() -> Result<String> {
    // Check cache first (after first successful load)
    if let Some(key) = CACHED_KEY.get() {
        return Ok(key.clone());
    }

    // 1. Check environment variable first (highest priority)
    if let Ok(key) = env::var("PROMOTION_SIGNING_KEY") {
        tracing::debug!("Using PROMOTION_SIGNING_KEY from environment");
        let _ = CACHED_KEY.set(key.clone());
        return Ok(key);
    }

    let key_path = Path::new(PROMOTION_KEY_PATH);

    // 2. Check for persisted key
    if key_path.exists() {
        let key_hex = fs::read_to_string(key_path)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read promotion key from {}: {}",
                    PROMOTION_KEY_PATH,
                    e
                )
            })?
            .trim()
            .to_string();

        // Validate the key format
        if key_hex.len() == 64 && hex::decode(&key_hex).is_ok() {
            tracing::debug!("Loaded promotion signing key from {}", PROMOTION_KEY_PATH);
            let _ = CACHED_KEY.set(key_hex.clone());
            return Ok(key_hex);
        } else {
            tracing::warn!("Invalid key format in {}, regenerating", PROMOTION_KEY_PATH);
        }
    }

    // 3. Generate new key and persist
    tracing::info!(
        "Generating new promotion signing key at {}",
        PROMOTION_KEY_PATH
    );

    // Ensure directory exists
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("Failed to create keys directory: {}", e))?;
    }

    // Generate secure random key
    use rand::RngCore;
    let mut key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key_bytes);
    let key_hex = hex::encode(key_bytes);

    // Write with restrictive permissions (0600)
    fs::write(key_path, &key_hex)
        .map_err(|e| anyhow::anyhow!("Failed to write promotion key: {}", e))?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(key_path)
            .map_err(|e| anyhow::anyhow!("Failed to get key file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(key_path, perms)
            .map_err(|e| anyhow::anyhow!("Failed to set key file permissions: {}", e))?;
    }

    let _ = CACHED_KEY.set(key_hex.clone());
    Ok(key_hex)
}

/// Sign a promotion record with Ed25519
pub fn sign_promotion(
    cpid: &str,
    promoted_by: &str,
    quality_json: &str,
) -> Result<(String, String)> {
    let key_hex = get_or_create_promotion_key()?;

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

    let key_hex = get_or_create_promotion_key()?;

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
    use std::sync::Once;

    // Test key - deterministic for reproducible tests
    const TEST_KEY: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

    static INIT: Once = Once::new();

    fn setup_test_key() {
        INIT.call_once(|| {
            env::set_var("PROMOTION_SIGNING_KEY", TEST_KEY);
        });
    }

    #[test]
    fn test_sign_and_verify() {
        setup_test_key();

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
        setup_test_key();

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

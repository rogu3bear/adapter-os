//! GCP Cloud KMS provider implementation
//!
//! Feature-gated by `gcp-kms`.
//!
//! This implementation is designed for use with the GCP KMS emulator for
//! local testing. For production use with real GCP KMS, OAuth2 authentication
//! and authenticated API calls would need to be implemented.
//!
//! SECURITY: This provider is emulator-only today and will fail closed when
//! configured with the production Cloud KMS endpoint.

use crate::key_provider::{KeyAlgorithm, KeyHandle};
use crate::providers::kms::{KmsConfig, KmsCredentials, KmsProvider, KmsProviderType};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Metadata for a GCP KMS key
#[derive(Clone, Debug)]
struct GcpKeyMetadata {
    /// Full resource name of the key
    key_name: String,
    /// Algorithm type
    algorithm: KeyAlgorithm,
    /// Current primary version number
    version: i32,
    /// Key material (for emulator mock)
    key_material: Vec<u8>,
    /// Cached public key (for asymmetric keys)
    public_key: Option<Vec<u8>>,
}

/// GCP Cloud KMS provider implementation
///
/// This is a stub implementation that simulates GCP KMS behavior locally.
/// It's designed for testing with the GCP KMS emulator or as a local mock.
#[allow(dead_code)]
pub struct GcpKmsProvider {
    /// Provider configuration
    config: KmsConfig,
    /// GCP project ID
    project_id: String,
    /// Location (region)
    location: String,
    /// Key ring name
    key_ring: String,
    /// Local key storage (simulates KMS for emulator testing)
    keys: Arc<RwLock<HashMap<String, GcpKeyMetadata>>>,
    /// Maximum retries for operations
    max_retries: u32,
}

impl GcpKmsProvider {
    /// Create a new GCP KMS provider with async initialization
    ///
    /// This implementation parses configuration and prepares for emulator use.
    /// Actual GCP API calls would require additional OAuth2 setup.
    pub async fn new_async(config: KmsConfig) -> Result<Self> {
        // Extract credentials
        let credentials_json = match &config.credentials {
            KmsCredentials::GcpServiceAccount { credentials_json } => {
                String::from_utf8_lossy(credentials_json.as_bytes()).to_string()
            }
            _ => {
                return Err(AosError::Crypto(
                    "GCP KMS requires GcpServiceAccount credentials".to_string(),
                ))
            }
        };

        // Parse project_id from credentials JSON
        let creds_json: serde_json::Value = serde_json::from_str(&credentials_json)
            .map_err(|e| AosError::Crypto(format!("Invalid GCP credentials JSON: {}", e)))?;

        let project_id = creds_json
            .get("project_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AosError::Crypto("GCP credentials missing project_id".to_string()))?
            .to_string();

        let endpoint_lc = config.endpoint.trim().to_ascii_lowercase();
        if endpoint_lc.contains("cloudkms.googleapis.com") || endpoint_lc.starts_with("https://") {
            return Err(AosError::Crypto(format!(
                "GCP KMS provider is emulator-only; refusing non-emulator endpoint '{}'",
                config.endpoint
            )));
        }

        // Get location from config or default
        let location = config
            .region
            .clone()
            .unwrap_or_else(|| "us-central1".to_string());

        // Get key ring from config or default
        let key_ring = config
            .key_namespace
            .clone()
            .unwrap_or_else(|| "adapteros-keys".to_string());

        let max_retries = config.max_retries;

        info!(
            project_id = %project_id,
            location = %location,
            key_ring = %key_ring,
            endpoint = %config.endpoint,
            "GCP KMS provider initialized (emulator mode)"
        );

        Ok(Self {
            config,
            project_id,
            location,
            key_ring,
            keys: Arc::new(RwLock::new(HashMap::new())),
            max_retries,
        })
    }

    /// Build the key ring resource name
    fn key_ring_name(&self) -> String {
        format!(
            "projects/{}/locations/{}/keyRings/{}",
            self.project_id, self.location, self.key_ring
        )
    }

    /// Build a crypto key resource name
    fn crypto_key_name(&self, key_id: &str) -> String {
        format!("{}/cryptoKeys/{}", self.key_ring_name(), key_id)
    }

    /// Build a crypto key version resource name
    #[allow(dead_code)]
    fn crypto_key_version_name(&self, key_id: &str, version: i32) -> String {
        format!(
            "{}/cryptoKeyVersions/{}",
            self.crypto_key_name(key_id),
            version
        )
    }

    /// Generate deterministic key material for testing
    fn generate_key_material(key_id: &str, alg: &KeyAlgorithm) -> Vec<u8> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key_id.hash(&mut hasher);
        format!("{:?}", alg).hash(&mut hasher);
        "gcp-kms-emulator".hash(&mut hasher);

        let hash = hasher.finish();
        let mut key = vec![0u8; 32];
        for (i, byte) in hash.to_le_bytes().iter().cycle().take(32).enumerate() {
            key[i] = *byte;
        }
        key
    }

    /// Derive mock public key from private key material
    fn derive_public_key(private_key: &[u8]) -> Vec<u8> {
        // Mock derivation: XOR with constant pattern
        private_key.iter().map(|b| b.wrapping_add(0x42)).collect()
    }
}

#[async_trait]
impl KmsProvider for GcpKmsProvider {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let mut keys = self.keys.write().await;

        if keys.contains_key(key_id) {
            return Err(AosError::Crypto(format!("Key already exists: {}", key_id)));
        }

        let key_material = Self::generate_key_material(key_id, &alg);
        let public_key = if alg == KeyAlgorithm::Ed25519 {
            Some(Self::derive_public_key(&key_material))
        } else {
            None
        };

        let key_name = self.crypto_key_version_name(key_id, 1);

        let metadata = GcpKeyMetadata {
            key_name: key_name.clone(),
            algorithm: alg.clone(),
            version: 1,
            key_material,
            public_key: public_key.clone(),
        };

        keys.insert(key_id.to_string(), metadata);

        debug!(key_id = %key_id, algorithm = %alg, "GCP KMS (emulator): generated key");

        Ok(KeyHandle::with_public_key(
            key_name,
            alg,
            public_key.unwrap_or_default(),
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        if key.algorithm != KeyAlgorithm::Ed25519 {
            return Err(AosError::Crypto(format!(
                "Key {} is not a signing key (algorithm: {})",
                key_id, key.algorithm
            )));
        }

        // Mock signature using HMAC-like construction
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.key_material.hash(&mut hasher);
        data.hash(&mut hasher);
        let hash = hasher.finish();

        // Create a 64-byte signature (Ed25519 signature size)
        let mut signature = Vec::with_capacity(64);
        for _ in 0..8 {
            signature.extend_from_slice(&hash.to_le_bytes());
        }

        debug!(key_id = %key_id, data_len = %data.len(), "GCP KMS (emulator): signed data");

        Ok(signature)
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        match key.algorithm {
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {}
            _ => {
                return Err(AosError::Crypto(format!(
                    "Key {} is not an encryption key (algorithm: {})",
                    key_id, key.algorithm
                )));
            }
        }

        // Mock encryption: XOR with key material (NOT cryptographically secure)
        let mut ciphertext = plaintext.to_vec();
        for (i, byte) in ciphertext.iter_mut().enumerate() {
            *byte ^= key.key_material[i % key.key_material.len()];
        }

        debug!(key_id = %key_id, plaintext_len = %plaintext.len(), "GCP KMS (emulator): encrypted data");

        Ok(ciphertext)
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        match key.algorithm {
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {}
            _ => {
                return Err(AosError::Crypto(format!(
                    "Key {} is not an encryption key (algorithm: {})",
                    key_id, key.algorithm
                )));
            }
        }

        // Mock decryption: XOR with key material (inverse of encrypt)
        let mut plaintext = ciphertext.to_vec();
        for (i, byte) in plaintext.iter_mut().enumerate() {
            *byte ^= key.key_material[i % key.key_material.len()];
        }

        debug!(key_id = %key_id, ciphertext_len = %ciphertext.len(), "GCP KMS (emulator): decrypted data");

        Ok(plaintext)
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let mut keys = self.keys.write().await;

        let key = keys
            .get_mut(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Increment version
        key.version += 1;

        // Generate new key material for the new version
        let new_key_id = format!("{}-v{}", key_id, key.version);
        key.key_material = Self::generate_key_material(&new_key_id, &key.algorithm);

        if key.algorithm == KeyAlgorithm::Ed25519 {
            key.public_key = Some(Self::derive_public_key(&key.key_material));
        }

        let new_version_name = self.crypto_key_version_name(key_id, key.version);
        key.key_name = new_version_name.clone();

        info!(key_id = %key_id, version = %key.version, "GCP KMS (emulator): rotated key");

        Ok(KeyHandle::with_public_key(
            new_version_name,
            key.algorithm.clone(),
            key.public_key.clone().unwrap_or_default(),
        ))
    }

    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        key.public_key.clone().ok_or_else(|| {
            AosError::Crypto(format!(
                "Key {} does not have a public key (algorithm: {})",
                key_id, key.algorithm
            ))
        })
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let keys = self.keys.read().await;
        Ok(keys.contains_key(key_id))
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let mut keys = self.keys.write().await;

        if keys.remove(key_id).is_some() {
            info!(key_id = %key_id, "GCP KMS (emulator): deleted key");
            Ok(())
        } else {
            Err(AosError::Crypto(format!("Key not found: {}", key_id)))
        }
    }

    fn provider_type(&self) -> KmsProviderType {
        KmsProviderType::GcpKms
    }

    fn fingerprint(&self) -> String {
        format!(
            "gcp-kms-emulator:{}:{}:{}",
            self.project_id, self.location, self.key_ring
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_async_rejects_production_endpoint() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: None,
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: r#"{"project_id":"test-project"}"#.into(),
            },
            timeout_secs: 30,
            max_retries: 1,
            key_namespace: None,
        };

        let err = match GcpKmsProvider::new_async(config).await {
            Ok(_) => panic!("expected production endpoint to be rejected"),
            Err(e) => e,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("emulator-only"),
            "expected emulator-only rejection, got: {msg}"
        );
    }
}

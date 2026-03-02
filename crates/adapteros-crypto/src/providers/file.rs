//! File-based key provider for development and testing
//!
//! **WARNING:** This provider stores keys in plaintext on the filesystem.
//! It is intended for development and testing only, NOT for production use.
//!
//! ## Security Considerations
//! - Keys are stored unencrypted on disk
//! - File permissions are set to 0600 (owner read/write only)
//! - Requires --allow-insecure-keys flag in production mode
//! - Logs warnings when used
//!
//! ## Usage
//! ```no_run
//! use adapteros_crypto::providers::file::FileProvider;
//! use std::path::PathBuf;
//!
//! let provider = FileProvider::new(
//!     PathBuf::from("/path/to/keyfile"),
//!     true  // allow_insecure
//! ).expect("Failed to create file provider");
//! ```

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, ProviderAttestation, RotationReceipt,
};
use crate::signature::Keypair;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// File-based key storage format
#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeyStore {
    /// Schema version for future compatibility
    version: u32,
    /// Map of key_id to key data
    keys: HashMap<String, StoredKey>,
}

/// Individual key entry
#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredKey {
    /// Key algorithm
    algorithm: KeyAlgorithm,
    /// Raw key bytes (sensitive!)
    key_bytes: Vec<u8>,
    /// Public key bytes (if applicable)
    public_key: Option<Vec<u8>>,
    /// Creation timestamp
    created_at: u64,
    /// Last rotation timestamp
    rotated_at: Option<u64>,
}

/// File-based key provider
///
/// Stores keys in a JSON file on the filesystem. Suitable for development
/// and testing only. Production use requires --allow-insecure-keys flag.
#[derive(Debug)]
pub struct FileProvider {
    /// Path to the key file
    key_file: PathBuf,
    /// Allow insecure keys flag
    allow_insecure: bool,
    /// In-memory key store
    store: Arc<RwLock<KeyStore>>,
}

impl FileProvider {
    /// Create a new file provider
    ///
    /// # Arguments
    /// - `key_file`: Path to the key file (will be created if it doesn't exist)
    /// - `allow_insecure`: Allow insecure file-based keys (required for production)
    ///
    /// # Security
    /// - Sets file permissions to 0600 (owner read/write only)
    /// - Warns in logs when created
    /// - Rejects creation without allow_insecure flag
    pub fn new(key_file: PathBuf, allow_insecure: bool) -> Result<Self> {
        warn!(
            path = %key_file.display(),
            allow_insecure = allow_insecure,
            "Creating file-based key provider - NOT SUITABLE FOR PRODUCTION"
        );

        if !allow_insecure {
            return Err(AosError::Config(
                "File-based keys require allow_insecure=true".to_string(),
            ));
        }

        // Load or create the key store
        let store = if key_file.exists() {
            debug!(path = %key_file.display(), "Loading existing key file");
            Self::load_keystore(&key_file)?
        } else {
            debug!(path = %key_file.display(), "Creating new key file");
            let store = KeyStore {
                version: 1,
                keys: HashMap::new(),
            };

            // Create parent directory if needed
            if let Some(parent) = key_file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AosError::Io(format!("Failed to create key directory: {}", e)))?;
            }

            // Save empty store
            Self::save_keystore(&key_file, &store)?;
            store
        };

        // Set file permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_file)
                .map_err(|e| AosError::Io(format!("Failed to get file metadata: {}", e)))?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_file, perms)
                .map_err(|e| AosError::Io(format!("Failed to set file permissions: {}", e)))?;
            info!(path = %key_file.display(), "Set file permissions to 0600");
        }

        Ok(Self {
            key_file,
            allow_insecure,
            store: Arc::new(RwLock::new(store)),
        })
    }

    /// Load keystore from file
    fn load_keystore(path: &PathBuf) -> Result<KeyStore> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read key file: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| AosError::Config(format!("Failed to parse key file: {}", e)))
    }

    /// Save keystore to file
    fn save_keystore(path: &PathBuf, store: &KeyStore) -> Result<()> {
        let content = serde_json::to_string_pretty(store)
            .map_err(|e| AosError::Config(format!("Failed to serialize keystore: {}", e)))?;

        std::fs::write(path, content)
            .map_err(|e| AosError::Io(format!("Failed to write key file: {}", e)))?;

        Ok(())
    }

    /// Persist the current store to disk
    async fn persist(&self) -> Result<()> {
        let store = self.store.read().await;
        Self::save_keystore(&self.key_file, &store)
    }

    /// Get the current timestamp
    fn now() -> u64 {
        adapteros_core::time::unix_timestamp_secs()
    }
}

#[async_trait::async_trait]
impl KeyProvider for FileProvider {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        info!(key_id = %key_id, algorithm = ?alg, "Generating new key");

        let mut store = self.store.write().await;

        // Check if key already exists
        if store.keys.contains_key(key_id) {
            return Err(AosError::Config(format!("Key '{}' already exists", key_id)));
        }

        // Generate key based on algorithm
        let (key_bytes, public_key) = match alg {
            KeyAlgorithm::Ed25519 => {
                let keypair = Keypair::generate();
                let key_bytes = keypair.to_bytes().to_vec();
                let public_key = keypair.public_key().to_bytes().to_vec();
                (key_bytes, Some(public_key))
            }
            KeyAlgorithm::Aes256Gcm => {
                use rand::RngCore;
                let mut key = vec![0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                (key, None)
            }
            KeyAlgorithm::ChaCha20Poly1305 => {
                use rand::RngCore;
                let mut key = vec![0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                (key, None)
            }
        };

        // Store the key
        let stored_key = StoredKey {
            algorithm: alg.clone(),
            key_bytes: key_bytes.clone(),
            public_key: public_key.clone(),
            created_at: Self::now(),
            rotated_at: None,
        };

        store.keys.insert(key_id.to_string(), stored_key);

        // Persist to disk
        drop(store);
        self.persist().await?;

        Ok(KeyHandle::new(key_id.to_string(), alg))
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, msg_len = msg.len(), "Signing message");

        let store = self.store.read().await;

        let stored_key = store
            .keys
            .get(key_id)
            .ok_or_else(|| AosError::Config(format!("Key '{}' not found", key_id)))?;

        if stored_key.algorithm != KeyAlgorithm::Ed25519 {
            return Err(AosError::Config(format!(
                "Key '{}' is not an Ed25519 signing key",
                key_id
            )));
        }

        // Convert to signing key
        if stored_key.key_bytes.len() != 32 {
            return Err(AosError::Config(format!(
                "Invalid key length for Ed25519: {}",
                stored_key.key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&stored_key.key_bytes);
        let keypair = Keypair::from_bytes(&key_array);

        let signature = keypair.sign(msg);
        Ok(signature.to_bytes().to_vec())
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        debug!(
            key_id = %key_id,
            plaintext_len = plaintext.len(),
            "Sealing plaintext"
        );

        let store = self.store.read().await;

        let stored_key = store
            .keys
            .get(key_id)
            .ok_or_else(|| AosError::Config(format!("Key '{}' not found", key_id)))?;

        match stored_key.algorithm {
            KeyAlgorithm::Aes256Gcm => {
                use aes_gcm::{
                    aead::{Aead, KeyInit},
                    Aes256Gcm, Nonce,
                };
                use rand::RngCore;

                if stored_key.key_bytes.len() != 32 {
                    return Err(AosError::Config(format!(
                        "Invalid key length for AES-256-GCM: {}",
                        stored_key.key_bytes.len()
                    )));
                }

                let cipher = Aes256Gcm::new_from_slice(&stored_key.key_bytes)
                    .map_err(|e| AosError::Config(format!("Failed to create cipher: {}", e)))?;

                // Generate random nonce
                let mut nonce_bytes = [0u8; 12];
                rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
                let nonce = Nonce::from_slice(&nonce_bytes);

                // Encrypt
                let ciphertext = cipher
                    .encrypt(nonce, plaintext)
                    .map_err(|e| AosError::Config(format!("Encryption failed: {}", e)))?;

                // Prepend nonce to ciphertext
                let mut result = nonce_bytes.to_vec();
                result.extend_from_slice(&ciphertext);

                Ok(result)
            }
            _ => Err(AosError::Config(format!(
                "Key '{}' is not an encryption key",
                key_id
            ))),
        }
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        debug!(
            key_id = %key_id,
            ciphertext_len = ciphertext.len(),
            "Unsealing ciphertext"
        );

        if ciphertext.len() < 12 {
            return Err(AosError::Config(
                "Ciphertext too short (missing nonce)".to_string(),
            ));
        }

        let store = self.store.read().await;

        let stored_key = store
            .keys
            .get(key_id)
            .ok_or_else(|| AosError::Config(format!("Key '{}' not found", key_id)))?;

        match stored_key.algorithm {
            KeyAlgorithm::Aes256Gcm => {
                use aes_gcm::{
                    aead::{Aead, KeyInit},
                    Aes256Gcm, Nonce,
                };

                if stored_key.key_bytes.len() != 32 {
                    return Err(AosError::Config(format!(
                        "Invalid key length for AES-256-GCM: {}",
                        stored_key.key_bytes.len()
                    )));
                }

                let cipher = Aes256Gcm::new_from_slice(&stored_key.key_bytes)
                    .map_err(|e| AosError::Config(format!("Failed to create cipher: {}", e)))?;

                // Extract nonce and ciphertext
                let nonce = Nonce::from_slice(&ciphertext[..12]);
                let ct = &ciphertext[12..];

                // Decrypt
                let plaintext = cipher
                    .decrypt(nonce, ct)
                    .map_err(|e| AosError::Config(format!("Decryption failed: {}", e)))?;

                Ok(plaintext)
            }
            _ => Err(AosError::Config(format!(
                "Key '{}' is not an encryption key",
                key_id
            ))),
        }
    }

    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt> {
        info!(key_id = %key_id, "Rotating key");

        let mut store = self.store.write().await;

        let old_key = store
            .keys
            .get(key_id)
            .ok_or_else(|| AosError::Config(format!("Key '{}' not found", key_id)))?;

        let old_handle = KeyHandle {
            provider_id: key_id.to_string(),
            algorithm: old_key.algorithm.clone(),
            public_key: old_key.public_key.clone(),
        };

        // Generate new key with same algorithm
        let (new_key_bytes, new_public_key) = match old_key.algorithm {
            KeyAlgorithm::Ed25519 => {
                let keypair = Keypair::generate();
                let key_bytes = keypair.to_bytes().to_vec();
                let public_key = keypair.public_key().to_bytes().to_vec();
                (key_bytes, Some(public_key))
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                use rand::RngCore;
                let mut key = vec![0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                (key, None)
            }
        };

        let new_stored_key = StoredKey {
            algorithm: old_key.algorithm.clone(),
            key_bytes: new_key_bytes,
            public_key: new_public_key.clone(),
            created_at: old_key.created_at,
            rotated_at: Some(Self::now()),
        };

        let new_handle = KeyHandle {
            provider_id: key_id.to_string(),
            algorithm: old_key.algorithm.clone(),
            public_key: new_public_key,
        };

        // Update the key
        store.keys.insert(key_id.to_string(), new_stored_key);

        // Persist to disk
        drop(store);
        self.persist().await?;

        // Create rotation receipt
        let timestamp = Self::now();
        let receipt_data = format!("{}:{}:{}", key_id, old_handle.provider_id, timestamp);
        let signature = self.sign(key_id, receipt_data.as_bytes()).await?;

        Ok(RotationReceipt::new(
            key_id.to_string(),
            old_handle,
            new_handle,
            timestamp,
            signature,
        ))
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        debug!("Generating provider attestation");

        let store = self.store.read().await;

        // Compute fingerprint of all keys
        let mut key_data = String::new();
        for (key_id, key) in store.keys.iter() {
            key_data.push_str(&format!(
                "{}:{}:{}",
                key_id,
                key.algorithm,
                hex::encode(&key.key_bytes)
            ));
        }

        let fingerprint_hash = B3Hash::hash(key_data.as_bytes());
        let fingerprint = hex::encode(fingerprint_hash.as_bytes());

        // Compute policy hash (for file provider, this is just version)
        let policy_data = format!(
            "file-provider:v{}:insecure={}",
            store.version, self.allow_insecure
        );
        let policy_hash_bytes = B3Hash::hash(policy_data.as_bytes());
        let policy_hash = hex::encode(policy_hash_bytes.as_bytes());

        let timestamp = Self::now();

        // Create attestation data
        let attestation_data = format!("file:{}:{}:{}", fingerprint, policy_hash, timestamp);

        // Sign with first available Ed25519 key
        let key_id_opt = store
            .keys
            .iter()
            .find(|(_, k)| k.algorithm == KeyAlgorithm::Ed25519)
            .map(|(key_id, _)| key_id.clone());
        drop(store);

        let signature = if let Some(key_id) = key_id_opt {
            self.sign(&key_id, attestation_data.as_bytes()).await?
        } else {
            vec![]
        };

        Ok(ProviderAttestation::new(
            "file".to_string(),
            fingerprint,
            policy_hash,
            timestamp,
            signature,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        adapteros_core::tempdir_in_var("aos-test-").expect("create temp dir")
    }

    #[tokio::test]
    async fn test_file_provider_create() {
        let temp_dir = new_test_tempdir();
        let key_file = temp_dir.path().join("test_keys.json");

        let _provider = FileProvider::new(key_file.clone(), true).unwrap();
        assert!(key_file.exists());
    }

    #[tokio::test]
    async fn test_file_provider_generate_and_sign() {
        let temp_dir = new_test_tempdir();
        let key_file = temp_dir.path().join("test_keys.json");

        let provider = FileProvider::new(key_file, true).unwrap();

        // Generate a key
        let handle = provider
            .generate("test-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);

        // Sign a message
        let msg = b"test message";
        let signature = provider.sign("test-key", msg).await.unwrap();
        assert_eq!(signature.len(), 64); // Ed25519 signature length
    }

    #[tokio::test]
    async fn test_file_provider_seal_unseal() {
        let temp_dir = new_test_tempdir();
        let key_file = temp_dir.path().join("test_keys.json");

        let provider = FileProvider::new(key_file, true).unwrap();

        // Generate an encryption key
        provider
            .generate("enc-key", KeyAlgorithm::Aes256Gcm)
            .await
            .unwrap();

        // Seal plaintext
        let plaintext = b"secret data";
        let ciphertext = provider.seal("enc-key", plaintext).await.unwrap();

        // Unseal ciphertext
        let recovered = provider.unseal("enc-key", &ciphertext).await.unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[tokio::test]
    async fn test_file_provider_rotate() {
        let temp_dir = new_test_tempdir();
        let key_file = temp_dir.path().join("test_keys.json");

        let provider = FileProvider::new(key_file, true).unwrap();

        // Generate initial key
        provider
            .generate("rotate-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        // Sign with original key
        let msg = b"test";
        let sig1 = provider.sign("rotate-key", msg).await.unwrap();

        // Rotate key
        let receipt = provider.rotate("rotate-key").await.unwrap();
        assert_eq!(receipt.key_id, "rotate-key");

        // Sign with rotated key (should produce different signature)
        let sig2 = provider.sign("rotate-key", msg).await.unwrap();
        assert_ne!(sig1, sig2);
    }

    #[tokio::test]
    async fn test_file_provider_rejects_without_allow_insecure() {
        let temp_dir = new_test_tempdir();
        let key_file = temp_dir.path().join("test_keys.json");

        let result = FileProvider::new(key_file, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("allow_insecure=true"));
    }
}

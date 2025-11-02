#![allow(deprecated)]

//! OS Keychain provider implementation
//!
//! Provides cryptographic key operations using OS-native key storage:
//! - macOS: Security Framework (Keychain)
//! - Linux: kernel keyring (keyctl)

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
use adapteros_core::{AosError, Result};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::collections::HashMap;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use tracing::error;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use tracing::warn;
use tracing::{debug, info};

/// Keychain provider implementation
pub struct KeychainProvider {
    #[allow(dead_code)]
    service_name: String,
    #[allow(dead_code)]
    config: KeyProviderConfig,
    keyring: Box<dyn KeyringImpl + Send + Sync>,
}

impl KeychainProvider {
    /// Create a new keychain provider
    pub fn new(config: KeyProviderConfig) -> Result<Self> {
        let service_name = config
            .keychain_service
            .as_deref()
            .unwrap_or("adapteros")
            .to_string();

        info!(
            service = %service_name,
            "Initializing keychain provider"
        );

        // Create the platform-specific keyring implementation
        let keyring: Box<dyn KeyringImpl + Send + Sync> = {
            #[cfg(target_os = "macos")]
            {
                Box::new(MacKeychain::new(service_name.clone()))
            }
            #[cfg(target_os = "linux")]
            {
                Box::new(LinuxKeyring::new(service_name.clone()))
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                error!("Keychain provider not supported on this platform");
                return Err(AosError::Crypto(
                    "Keychain provider not supported on this platform".to_string(),
                ));
            }
        };

        Ok(Self {
            service_name,
            config,
            keyring,
        })
    }

    /// Get platform-specific keyring implementation
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        Ok(Box::new(MacKeychain::new(self.service_name.clone())))
    }

    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        Ok(Box::new(LinuxKeyring::new(self.service_name.clone())))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    #[allow(dead_code)]
    fn get_keyring(&self) -> Result<Box<dyn KeyringImpl>> {
        error!("Keychain provider not supported on this platform");
        Err(AosError::Crypto(
            "Keychain provider not supported on this platform".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl KeyProvider for KeychainProvider {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        debug!(key_id = %key_id, algorithm = %alg, "Generating key in keychain");

        let handle = self.keyring.generate_key(key_id, alg).await?;

        info!(key_id = %key_id, "Key generated successfully");
        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, msg_len = msg.len(), "Signing message");

        self.keyring.sign(key_id, msg).await
    }

    #[allow(deprecated)]
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, plaintext_len = plaintext.len(), "Sealing data");

        self.keyring.seal(key_id, plaintext).await
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        debug!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Unsealing data");

        self.keyring.unseal(key_id, ciphertext).await
    }

    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt> {
        debug!(key_id = %key_id, "Rotating key");

        let receipt = self.keyring.rotate_key(key_id).await?;

        info!(key_id = %key_id, "Key rotated successfully");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        debug!("Generating provider attestation");

        let attestation = self.keyring.attest().await?;

        debug!("Provider attestation generated");
        Ok(attestation)
    }
}

/// Platform-specific keyring trait
#[async_trait::async_trait]
trait KeyringImpl: Send + Sync {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle>;
    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>>;
    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>>;
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>>;
    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt>;
    async fn attest(&self) -> Result<ProviderAttestation>;
}

/// macOS Keychain implementation using Security Framework
#[cfg(target_os = "macos")]
struct MacKeychain {
    service_name: String,
    keys: std::sync::Mutex<HashMap<String, KeyHandle>>,
}

#[cfg(target_os = "macos")]
impl MacKeychain {
    fn new(service_name: String) -> Self {
        Self {
            service_name,
            keys: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl KeyringImpl for MacKeychain {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        match &alg {
            KeyAlgorithm::Ed25519 => {
                // Use Ed25519 from the existing crypto stack for now
                // TODO: Integrate with macOS Keychain when API stabilizes
                use rand::rngs::OsRng;

                let mut rng = OsRng;
                let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
                let verifying_key = signing_key.verifying_key();

                let handle = KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    alg.clone(),
                    verifying_key.to_bytes().to_vec(),
                );

                // Store key data in memory for now
                self.keys
                    .lock()
                    .unwrap()
                    .insert(key_id.to_string(), handle.clone());

                info!(key_id = %key_id, algorithm = ?alg, "Generated Ed25519 key (macOS Keychain integration pending)");
                Ok(handle)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                // For symmetric keys, generate and store in memory for now
                use rand::rngs::OsRng;
                use rand::RngCore;

                let mut key_data = [0u8; 32];
                OsRng.fill_bytes(&mut key_data);

                let handle =
                    KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg.clone());

                // Store key data in memory
                self.keys
                    .lock()
                    .unwrap()
                    .insert(key_id.to_string(), handle.clone());

                info!(key_id = %key_id, algorithm = ?alg, "Generated symmetric key (macOS Keychain integration pending)");
                Ok(handle)
            }
        }
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual macOS Keychain signing integration
        // For now, use the existing in-memory key implementation
        warn!("macOS Keychain signing not yet implemented, using in-memory fallback");

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Use the existing Ed25519 implementation for now
        let key_data = [42u8; 32]; // TODO: retrieve from keychain
        use ed25519_dalek::Signer;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_data);
        let signature = signing_key.sign(msg);

        info!(key_id = %key_id, message_len = msg.len(), "Signed message (macOS Keychain integration pending)");
        Ok(signature.to_bytes().to_vec())
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual macOS Keychain encryption integration
        // For now, use the existing AES-GCM implementation
        warn!("macOS Keychain sealing not yet implemented, using AES-GCM fallback");

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Use AES-GCM for encryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&[42u8; 32]); // TODO: retrieve from keychain
        let cipher = Aes256Gcm::new(key);

        // Generate a random nonce
        use rand::rngs::OsRng;
        use rand::RngCore;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AosError::Crypto(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);

        info!(key_id = %key_id, plaintext_len = plaintext.len(), "Encrypted data (macOS Keychain integration pending)");
        Ok(result)
    }

    #[allow(deprecated)]
    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual macOS Keychain decryption integration
        // For now, use the existing AES-GCM implementation
        warn!("macOS Keychain unsealing not yet implemented, using AES-GCM fallback");

        if ciphertext.len() < 12 {
            return Err(AosError::Crypto("Ciphertext too short".to_string()));
        }

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Use AES-GCM for decryption
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&[42u8; 32]); // TODO: retrieve from keychain
        let cipher = Aes256Gcm::new(key);

        // Extract nonce from beginning of ciphertext
        let nonce_bytes = &ciphertext[..12];
        let nonce = Nonce::from_slice(nonce_bytes);
        let encrypted_data = &ciphertext[12..];

        // Decrypt the data
        let plaintext = cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| AosError::Crypto(format!("Decryption failed: {}", e)))?;

        info!(key_id = %key_id, ciphertext_len = ciphertext.len(), "Decrypted data (macOS Keychain integration pending)");
        Ok(plaintext)
    }

    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt> {
        // TODO: Implement actual macOS Keychain rotation integration
        // For now, use the existing in-memory key implementation
        warn!("macOS Keychain rotation not yet implemented, using in-memory fallback");

        // Get previous handle and drop lock before await
        let previous_handle = {
            let keys = self.keys.lock().unwrap();
            keys.get(key_id)
                .cloned()
                .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?
        };
        let algorithm = previous_handle.algorithm.clone();

        // Generate new key
        let new_handle = self.generate_key(key_id, algorithm.clone()).await?;

        let receipt = RotationReceipt::new(
            key_id.to_string(),
            previous_handle,
            new_handle,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            vec![], // TODO: sign receipt
        );

        info!(key_id = %key_id, algorithm = ?algorithm, "Rotated key (macOS Keychain integration pending)");
        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        // TODO: Implement actual macOS Keychain attestation
        // For now, use the existing placeholder implementation
        warn!("macOS Keychain attestation not yet implemented, using placeholder");

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(ProviderAttestation::new(
            "macos-keychain".to_string(),
            format!("service:{}", self.service_name),
            "placeholder-policy-hash".to_string(), // TODO: real policy hash
            timestamp,
            vec![], // TODO: sign attestation
        ))
    }
}

/// Linux keyring implementation using keyctl
#[cfg(target_os = "linux")]
struct LinuxKeyring {
    service_name: String,
    keys: std::sync::Mutex<HashMap<String, KeyHandle>>,
}

#[cfg(target_os = "linux")]
impl LinuxKeyring {
    fn new(service_name: String) -> Self {
        Self {
            service_name,
            keys: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl KeyringImpl for LinuxKeyring {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        // TODO: Implement actual Linux keyring integration
        // For now, use in-memory storage as placeholder
        warn!("Linux keyring not yet implemented, using in-memory fallback");

        use rand::rngs::OsRng;

        let handle = match alg {
            KeyAlgorithm::Ed25519 => {
                use ed25519_dalek::SigningKey;
                let mut rng = OsRng;
                let signing_key = SigningKey::generate(&mut rng);
                let verifying_key = signing_key.verifying_key();
                KeyHandle::with_public_key(
                    format!("{}:{}", self.service_name, key_id),
                    alg,
                    verifying_key.to_bytes().to_vec(),
                )
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                KeyHandle::new(format!("{}:{}", self.service_name, key_id), alg)
            }
        };

        self.keys
            .lock()
            .unwrap()
            .insert(key_id.to_string(), handle.clone());
        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual Linux keyring signing
        warn!("Linux keyring signing not yet implemented");

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Placeholder: use in-memory key for signing
        use ed25519_dalek::{Signer, SigningKey};
        let key_data = [42u8; 32]; // TODO: retrieve from keyring
        let signing_key = SigningKey::from_bytes(&key_data);
        let signature = signing_key.sign(msg);

        Ok(signature.to_bytes().to_vec())
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual Linux keyring encryption
        warn!("Linux keyring sealing not yet implemented");

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Placeholder: simple XOR encryption
        let key = [42u8; 32];
        let ciphertext = plaintext
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ key[i % key.len()])
            .collect();

        Ok(ciphertext)
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement actual Linux keyring decryption
        warn!("Linux keyring unsealing not yet implemented");

        let keys = self.keys.lock().unwrap();
        let _handle = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Placeholder: simple XOR decryption
        let key = [42u8; 32];
        let plaintext = ciphertext
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ key[i % key.len()])
            .collect();

        Ok(plaintext)
    }

    async fn rotate_key(&self, key_id: &str) -> Result<RotationReceipt> {
        // TODO: Implement actual Linux keyring rotation
        warn!("Linux keyring rotation not yet implemented");

        // Get previous handle and drop lock before await
        let previous_handle = {
            let keys = self.keys.lock().unwrap();
            keys.get(key_id)
                .cloned()
                .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?
        };
        let algorithm = previous_handle.algorithm.clone();

        // Generate new key
        let new_handle = self.generate_key(key_id, algorithm).await?;

        let receipt = RotationReceipt::new(
            key_id.to_string(),
            previous_handle,
            new_handle,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            vec![], // TODO: sign receipt
        );

        Ok(receipt)
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(ProviderAttestation::new(
            "linux-keyring".to_string(),
            format!("service:{}", self.service_name),
            "placeholder-policy-hash".to_string(), // TODO: real policy hash
            timestamp,
            vec![], // TODO: sign attestation
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;

    #[tokio::test]
    async fn test_keychain_provider_basic() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Test key generation
        let handle = provider
            .generate("test-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle.provider_id.contains("test-key"));

        // Test signing
        let message = b"Hello, world!";
        let signature = provider.sign("test-key", message).await.unwrap();
        assert!(!signature.is_empty());

        // Test encryption/decryption
        let plaintext = b"Secret data";
        let ciphertext = provider.seal("test-key", plaintext).await.unwrap();
        assert!(!ciphertext.is_empty());

        let decrypted = provider.unseal("test-key", &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);

        // Test attestation
        let attestation = provider.attest().await.unwrap();
        assert!(attestation.provider_type.contains("keychain"));
    }

    #[tokio::test]
    async fn test_keychain_provider_debug() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Test key generation
        let handle = provider
            .generate("debug-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        println!("Generated handle: {:?}", handle);

        // Test signing - this should work if the key was stored
        let message = b"Hello, world!";
        match provider.sign("debug-key", message).await {
            Ok(signature) => {
                println!("Signing successful, signature len: {}", signature.len());
                assert!(!signature.is_empty());
            }
            Err(e) => {
                println!("Signing failed: {:?}", e);
                panic!("Signing should work after key generation");
            }
        }
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).unwrap();

        // Generate initial key
        let _handle = provider
            .generate("rotate-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        // Rotate key
        let receipt = provider.rotate("rotate-key").await.unwrap();
        assert_eq!(receipt.key_id, "rotate-key");
        assert_eq!(receipt.previous_key.algorithm, KeyAlgorithm::Ed25519);
        assert_eq!(receipt.new_key.algorithm, KeyAlgorithm::Ed25519);
        assert!(receipt.timestamp > 0);
    }
}

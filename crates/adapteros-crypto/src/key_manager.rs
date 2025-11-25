//! KeyManager abstraction for centralized key management
//!
//! Provides a unified facade for key operations with support for multiple backends:
//! - Environment variables (highest precedence)
//! - File-based keys (development mode)
//! - OS Keychain (macOS/Linux)
//! - KMS/HSM services
//!
//! Key precedence: Environment variable > File path > OS Keychain > KMS

use crate::key_provider::{KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderMode};
use crate::providers::file::FileProvider;
use crate::providers::keychain::KeychainProvider;
use crate::providers::kms::KmsProvider;
use crate::signature::{Keypair, SigningKey};
use adapteros_core::{AosError, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Central key management facade
///
/// Provides a unified interface for all key operations with automatic
/// provider selection based on configuration and environment.
pub struct KeyManager {
    /// Active key provider
    provider: Arc<RwLock<Box<dyn KeyProvider>>>,
    /// Provider mode
    mode: KeyProviderMode,
    /// Configuration
    config: KeyManagerConfig,
}

impl std::fmt::Debug for KeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyManager")
            .field("mode", &self.mode)
            .field("config", &self.config)
            .finish()
    }
}

/// Configuration for KeyManager
#[derive(Clone, Debug)]
pub struct KeyManagerConfig {
    /// Provider mode
    pub mode: KeyProviderMode,
    /// File path for file-based provider
    pub file_path: Option<PathBuf>,
    /// Keychain service name
    pub keychain_service: Option<String>,
    /// KMS endpoint
    pub kms_endpoint: Option<String>,
    /// Allow insecure file-based keys
    pub allow_insecure_keys: bool,
    /// Production mode (enforces stricter security)
    pub production_mode: bool,
}

impl Default for KeyManagerConfig {
    fn default() -> Self {
        Self {
            mode: KeyProviderMode::Keychain,
            file_path: None,
            keychain_service: Some("adapteros".to_string()),
            kms_endpoint: None,
            allow_insecure_keys: false,
            production_mode: false,
        }
    }
}

impl KeyManager {
    /// Create a new KeyManager with the given configuration
    ///
    /// # Key Precedence
    /// 1. Environment variable (AOS_SIGNING_KEY)
    /// 2. File path (if allow_insecure_keys is true)
    /// 3. OS Keychain
    /// 4. KMS (if configured)
    ///
    /// # Errors
    /// - Returns error if file-based keys are used in production without --allow-insecure-keys
    /// - Returns error if no valid provider can be initialized
    pub async fn new(config: KeyManagerConfig) -> Result<Self> {
        // Check for environment variable first (highest precedence)
        if let Ok(env_key) = std::env::var("AOS_SIGNING_KEY") {
            info!("Using signing key from AOS_SIGNING_KEY environment variable");
            warn!("Environment-based keys should only be used in development");

            // Parse the key and create a file provider backed by environment
            let provider = Self::create_env_provider(env_key)?;
            return Ok(Self {
                provider: Arc::new(RwLock::new(provider)),
                mode: KeyProviderMode::File,
                config,
            });
        }

        // Check for file-based keys (second precedence)
        if let Some(ref file_path) = config.file_path {
            if config.production_mode && !config.allow_insecure_keys {
                return Err(AosError::Config(
                    "File-based keys require --allow-insecure-keys flag in production".to_string(),
                ));
            }

            if config.allow_insecure_keys || !config.production_mode {
                warn!(
                    path = %file_path.display(),
                    "Using file-based key provider - not suitable for production"
                );

                let provider = FileProvider::new(file_path.clone(), config.allow_insecure_keys)?;
                return Ok(Self {
                    provider: Arc::new(RwLock::new(Box::new(provider))),
                    mode: KeyProviderMode::File,
                    config,
                });
            }
        }

        // Select provider based on mode
        let (provider, mode) = match config.mode {
            KeyProviderMode::Keychain => {
                info!("Initializing OS Keychain provider");
                let provider_config = crate::key_provider::KeyProviderConfig {
                    mode: KeyProviderMode::Keychain,
                    keychain_service: config.keychain_service.clone(),
                    kms_endpoint: None,
                    file_path: None,
                    rotation_interval_secs: Some(86400), // 24 hours
                };
                let provider = KeychainProvider::new(provider_config)?;
                (
                    Box::new(provider) as Box<dyn KeyProvider>,
                    KeyProviderMode::Keychain,
                )
            }
            KeyProviderMode::Kms => {
                info!("Initializing KMS provider");
                if config.kms_endpoint.is_none() {
                    return Err(AosError::Config(
                        "KMS endpoint required for KMS mode".to_string(),
                    ));
                }
                let provider_config = crate::key_provider::KeyProviderConfig {
                    mode: KeyProviderMode::Kms,
                    keychain_service: None,
                    kms_endpoint: config.kms_endpoint.clone(),
                    file_path: None,
                    rotation_interval_secs: Some(86400),
                };
                let provider = KmsProvider::new(provider_config)?;
                (
                    Box::new(provider) as Box<dyn KeyProvider>,
                    KeyProviderMode::Kms,
                )
            }
            KeyProviderMode::File => {
                return Err(AosError::Config(
                    "File mode requires file_path to be set".to_string(),
                ));
            }
        };

        Ok(Self {
            provider: Arc::new(RwLock::new(provider)),
            mode,
            config,
        })
    }

    /// Create a provider from environment variable
    fn create_env_provider(env_key: String) -> Result<Box<dyn KeyProvider>> {
        // Decode the key from hex or base64
        let key_bytes = if env_key.len() == 64 {
            // Assume hex encoding
            hex::decode(&env_key).map_err(|e| {
                AosError::Config(format!("Invalid hex key in AOS_SIGNING_KEY: {}", e))
            })?
        } else {
            // Assume base64 encoding
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(&env_key)
                .map_err(|e| {
                    AosError::Config(format!("Invalid base64 key in AOS_SIGNING_KEY: {}", e))
                })?
        };

        if key_bytes.len() != 32 {
            return Err(AosError::Config(format!(
                "Invalid key length in AOS_SIGNING_KEY: expected 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        // Create a temporary file provider with the key
        let temp_dir = std::env::temp_dir();
        let key_file = temp_dir.join("aos_env_signing_key");
        std::fs::write(&key_file, &key_bytes).map_err(|e| {
            AosError::Io(format!(
                "Failed to write environment key to temp file: {}",
                e
            ))
        })?;

        Ok(Box::new(FileProvider::new(key_file, true)?))
    }

    /// Get the signing key for JWT or bundle signing
    ///
    /// Returns the Ed25519 signing key for the default key ID.
    pub async fn get_signing_key(&self) -> Result<SigningKey> {
        self.get_signing_key_by_id("default").await
    }

    /// Get a signing key by key ID
    pub async fn get_signing_key_by_id(&self, key_id: &str) -> Result<SigningKey> {
        debug!(key_id = %key_id, "Retrieving signing key");

        let provider = self.provider.read().await;

        // First, try to sign a test message to verify the key exists
        let test_msg = b"test";
        let _signature = provider.sign(key_id, test_msg).await?;

        // If signing succeeded, we can use the key
        // Note: This is a workaround since KeyProvider doesn't expose raw key material
        // In production, we'd need to extend KeyProvider to support key export
        drop(provider);

        // For now, return an error indicating this needs implementation
        Err(AosError::Config(
            "Direct signing key export not yet implemented - use sign() method instead".to_string(),
        ))
    }

    /// Get the JWT signing key
    ///
    /// Alias for get_signing_key() for semantic clarity.
    pub async fn get_jwt_key(&self) -> Result<SigningKey> {
        self.get_signing_key().await
    }

    /// Sign data with the default signing key
    pub async fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.sign_with_key("default", data).await
    }

    /// Sign data with a specific key ID
    pub async fn sign_with_key(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        debug!(
            key_id = %key_id,
            data_len = data.len(),
            "Signing data"
        );

        let provider = self.provider.read().await;
        provider.sign(key_id, data).await
    }

    /// Rotate the specified key
    ///
    /// Returns a signed receipt documenting the rotation.
    pub async fn rotate_key(&self, key_id: &str) -> Result<crate::key_provider::RotationReceipt> {
        info!(key_id = %key_id, mode = %self.mode, "Rotating key");

        let provider = self.provider.read().await;
        provider.rotate(key_id).await
    }

    /// Get the fingerprint of the current key
    ///
    /// Returns a BLAKE3 hash of the public key for identification.
    pub async fn key_fingerprint(&self, key_id: &str) -> Result<String> {
        debug!(key_id = %key_id, "Computing key fingerprint");

        // Generate a test signature to get the public key
        let provider = self.provider.read().await;
        let test_msg = b"fingerprint-test";
        let _signature = provider.sign(key_id, test_msg).await?;

        // Get attestation which includes provider fingerprint
        let attestation = provider.attest().await?;
        Ok(attestation.fingerprint)
    }

    /// Get the current provider mode
    pub fn mode(&self) -> &KeyProviderMode {
        &self.mode
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.config.production_mode
    }

    /// Generate a new key with the specified algorithm
    pub async fn generate_key(&self, key_id: &str, algorithm: KeyAlgorithm) -> Result<KeyHandle> {
        info!(
            key_id = %key_id,
            algorithm = ?algorithm,
            "Generating new key"
        );

        let provider = self.provider.read().await;
        provider.generate(key_id, algorithm).await
    }

    /// Encrypt data with the specified key (AEAD)
    pub async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        debug!(
            key_id = %key_id,
            plaintext_len = plaintext.len(),
            "Sealing data"
        );

        let provider = self.provider.read().await;
        provider.seal(key_id, plaintext).await
    }

    /// Decrypt data with the specified key (AEAD)
    pub async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        debug!(
            key_id = %key_id,
            ciphertext_len = ciphertext.len(),
            "Unsealing data"
        );

        let provider = self.provider.read().await;
        provider.unseal(key_id, ciphertext).await
    }

    /// Get provider attestation
    pub async fn attest(&self) -> Result<crate::key_provider::ProviderAttestation> {
        debug!("Generating provider attestation");

        let provider = self.provider.read().await;
        provider.attest().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_key_manager_file_provider() {
        let temp_dir = TempDir::new().unwrap();
        let key_file = temp_dir.path().join("test_keys.json");

        let config = KeyManagerConfig {
            mode: KeyProviderMode::File,
            file_path: Some(key_file.clone()),
            allow_insecure_keys: true,
            production_mode: false,
            ..Default::default()
        };

        let manager = KeyManager::new(config).await.unwrap();
        assert_eq!(manager.mode(), &KeyProviderMode::File);

        // Generate a key first
        manager
            .generate_key("test-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        // Test signing
        let data = b"test data";
        let signature = manager.sign_with_key("test-key", data).await.unwrap();
        assert_eq!(signature.len(), 64); // Ed25519 signature length
    }

    #[tokio::test]
    async fn test_key_manager_rejects_file_in_production() {
        let temp_dir = TempDir::new().unwrap();
        let key_file = temp_dir.path().join("test_keys.json");

        let config = KeyManagerConfig {
            mode: KeyProviderMode::File,
            file_path: Some(key_file),
            allow_insecure_keys: false,
            production_mode: true,
            ..Default::default()
        };

        let result = KeyManager::new(config).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--allow-insecure-keys"));
    }
}

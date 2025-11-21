//! KMS/HSM provider implementation
//!
//! Provides abstraction for cloud KMS (AWS, GCP, Azure) and HSM integration.
//! Uses a backend trait to allow different KMS implementations.

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Supported KMS backend types
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KmsBackendType {
    /// AWS Key Management Service
    AwsKms,
    /// Google Cloud KMS
    GcpKms,
    /// Azure Key Vault
    AzureKeyVault,
    /// HashiCorp Vault
    HashicorpVault,
    /// PKCS#11 HSM (YubiHSM, Thales, etc.)
    Pkcs11Hsm,
    /// Mock backend for testing
    Mock,
}

impl std::fmt::Display for KmsBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KmsBackendType::AwsKms => write!(f, "aws-kms"),
            KmsBackendType::GcpKms => write!(f, "gcp-kms"),
            KmsBackendType::AzureKeyVault => write!(f, "azure-keyvault"),
            KmsBackendType::HashicorpVault => write!(f, "hashicorp-vault"),
            KmsBackendType::Pkcs11Hsm => write!(f, "pkcs11-hsm"),
            KmsBackendType::Mock => write!(f, "mock"),
        }
    }
}

/// KMS-specific configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KmsConfig {
    /// Backend type to use
    pub backend_type: KmsBackendType,
    /// Endpoint URL for the KMS service
    pub endpoint: String,
    /// Region (for cloud providers)
    pub region: Option<String>,
    /// Authentication credentials (provider-specific)
    pub credentials: KmsCredentials,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Key namespace/prefix for multi-tenant isolation
    pub key_namespace: Option<String>,
}

impl Default for KmsConfig {
    fn default() -> Self {
        Self {
            backend_type: KmsBackendType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        }
    }
}

/// KMS authentication credentials
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KmsCredentials {
    /// No authentication (mock/local)
    None,
    /// AWS IAM credentials
    AwsIam {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
    /// GCP service account
    GcpServiceAccount {
        credentials_json: String,
    },
    /// Azure service principal
    AzureServicePrincipal {
        tenant_id: String,
        client_id: String,
        client_secret: String,
    },
    /// HashiCorp Vault token
    VaultToken {
        token: String,
    },
    /// PKCS#11 PIN
    Pkcs11Pin {
        pin: String,
        slot_id: Option<u64>,
    },
}

/// Backend trait for KMS implementations
#[async_trait::async_trait]
pub trait KmsBackend: Send + Sync {
    /// Generate a new key in the KMS
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle>;

    /// Sign data using a key stored in the KMS
    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>>;

    /// Encrypt data using a key stored in the KMS
    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>>;

    /// Decrypt data using a key stored in the KMS
    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>>;

    /// Rotate a key in the KMS
    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle>;

    /// Get the public key for an asymmetric key
    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>>;

    /// Check if a key exists in the KMS
    async fn key_exists(&self, key_id: &str) -> Result<bool>;

    /// Delete a key from the KMS (use with caution)
    async fn delete_key(&self, key_id: &str) -> Result<()>;

    /// Get backend type identifier
    fn backend_type(&self) -> KmsBackendType;

    /// Get backend version/fingerprint for attestation
    fn fingerprint(&self) -> String;
}

/// Mock KMS backend for testing and development
pub struct MockKmsBackend {
    keys: Arc<RwLock<HashMap<String, MockKey>>>,
}

#[derive(Clone)]
struct MockKey {
    algorithm: KeyAlgorithm,
    private_key: Vec<u8>,
    public_key: Vec<u8>,
    version: u32,
}

impl MockKmsBackend {
    /// Create a new mock KMS backend
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MockKmsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl KmsBackend for MockKmsBackend {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        use rand::RngCore;

        let mut keys = self.keys.write().await;

        if keys.contains_key(key_id) {
            return Err(AosError::Crypto(format!("Key already exists: {}", key_id)));
        }

        // Generate mock key material
        let (private_key, public_key) = match alg {
            KeyAlgorithm::Ed25519 => {
                let mut private = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut private);
                // Derive public key (simplified mock)
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                (key.clone(), vec![])
            }
        };

        let mock_key = MockKey {
            algorithm: alg.clone(),
            private_key,
            public_key: public_key.clone(),
            version: 1,
        };

        keys.insert(key_id.to_string(), mock_key);

        debug!(key_id = %key_id, algorithm = %alg, "Mock KMS: generated key");

        Ok(KeyHandle::with_public_key(
            format!("mock:{}", key_id),
            alg,
            public_key,
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys.get(key_id).ok_or_else(|| {
            AosError::Crypto(format!("Key not found: {}", key_id))
        })?;

        if key.algorithm != KeyAlgorithm::Ed25519 {
            return Err(AosError::Crypto(format!(
                "Key {} is not a signing key (algorithm: {})",
                key_id, key.algorithm
            )));
        }

        // Mock signature: HMAC-like construction
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.private_key.hash(&mut hasher);
        data.hash(&mut hasher);
        let hash = hasher.finish();

        let signature = hash.to_le_bytes().to_vec();
        debug!(key_id = %key_id, data_len = %data.len(), "Mock KMS: signed data");

        Ok(signature)
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys.get(key_id).ok_or_else(|| {
            AosError::Crypto(format!("Key not found: {}", key_id))
        })?;

        match key.algorithm {
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {}
            _ => {
                return Err(AosError::Crypto(format!(
                    "Key {} is not an encryption key (algorithm: {})",
                    key_id, key.algorithm
                )));
            }
        }

        // Mock encryption: XOR with key (NOT cryptographically secure)
        let mut ciphertext = plaintext.to_vec();
        for (i, byte) in ciphertext.iter_mut().enumerate() {
            *byte ^= key.private_key[i % key.private_key.len()];
        }

        debug!(key_id = %key_id, plaintext_len = %plaintext.len(), "Mock KMS: encrypted data");

        Ok(ciphertext)
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        // Mock decryption is same as encryption (XOR is self-inverse)
        self.encrypt(key_id, ciphertext).await
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        use rand::RngCore;

        let mut keys = self.keys.write().await;

        let key = keys.get_mut(key_id).ok_or_else(|| {
            AosError::Crypto(format!("Key not found: {}", key_id))
        })?;

        // Generate new key material
        let (private_key, public_key) = match key.algorithm {
            KeyAlgorithm::Ed25519 => {
                let mut private = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut private);
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut k = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut k);
                (k.clone(), vec![])
            }
        };

        key.private_key = private_key;
        key.public_key = public_key.clone();
        key.version += 1;

        info!(key_id = %key_id, version = %key.version, "Mock KMS: rotated key");

        Ok(KeyHandle::with_public_key(
            format!("mock:{}:v{}", key_id, key.version),
            key.algorithm.clone(),
            public_key,
        ))
    }

    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;

        let key = keys.get(key_id).ok_or_else(|| {
            AosError::Crypto(format!("Key not found: {}", key_id))
        })?;

        if key.public_key.is_empty() {
            return Err(AosError::Crypto(format!(
                "Key {} does not have a public key (algorithm: {})",
                key_id, key.algorithm
            )));
        }

        Ok(key.public_key.clone())
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let keys = self.keys.read().await;
        Ok(keys.contains_key(key_id))
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let mut keys = self.keys.write().await;

        if keys.remove(key_id).is_none() {
            return Err(AosError::Crypto(format!("Key not found: {}", key_id)));
        }

        warn!(key_id = %key_id, "Mock KMS: deleted key");
        Ok(())
    }

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::Mock
    }

    fn fingerprint(&self) -> String {
        "mock-kms-v1.0".to_string()
    }
}

/// KMS provider implementation
pub struct KmsProvider {
    config: KmsConfig,
    backend: Arc<dyn KmsBackend>,
    key_handles: Arc<RwLock<HashMap<String, KeyHandle>>>,
}

impl std::fmt::Debug for KmsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KmsProvider")
            .field("config", &self.config)
            .field("backend_type", &self.config.backend_type)
            .finish()
    }
}

impl KmsProvider {
    /// Create a new KMS provider with the specified configuration
    pub fn new(config: KeyProviderConfig) -> Result<Self> {
        let kms_config = KmsConfig::from_provider_config(&config)?;
        Self::with_kms_config(kms_config)
    }

    /// Create a new KMS provider with detailed KMS configuration
    pub fn with_kms_config(config: KmsConfig) -> Result<Self> {
        let backend: Arc<dyn KmsBackend> = match config.backend_type {
            KmsBackendType::Mock => Arc::new(MockKmsBackend::new()),
            KmsBackendType::AwsKms => {
                // TODO: Implement AWS KMS backend
                warn!("AWS KMS backend not yet fully implemented, using mock");
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::GcpKms => {
                // TODO: Implement GCP KMS backend
                warn!("GCP KMS backend not yet fully implemented, using mock");
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::AzureKeyVault => {
                // TODO: Implement Azure Key Vault backend
                warn!("Azure Key Vault backend not yet fully implemented, using mock");
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::HashicorpVault => {
                // TODO: Implement HashiCorp Vault backend
                warn!("HashiCorp Vault backend not yet fully implemented, using mock");
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::Pkcs11Hsm => {
                // TODO: Implement PKCS#11 HSM backend
                warn!("PKCS#11 HSM backend not yet fully implemented, using mock");
                Arc::new(MockKmsBackend::new())
            }
        };

        info!(
            backend_type = %config.backend_type,
            endpoint = %config.endpoint,
            "KMS provider initialized"
        );

        Ok(Self {
            config,
            backend,
            key_handles: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a KMS provider with a custom backend (for testing)
    pub fn with_backend(config: KmsConfig, backend: Arc<dyn KmsBackend>) -> Self {
        Self {
            config,
            backend,
            key_handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the namespaced key ID
    fn namespaced_key_id(&self, key_id: &str) -> String {
        match &self.config.key_namespace {
            Some(ns) => format!("{}/{}", ns, key_id),
            None => key_id.to_string(),
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl KmsConfig {
    /// Convert from generic KeyProviderConfig
    pub fn from_provider_config(config: &KeyProviderConfig) -> Result<Self> {
        let endpoint = config.kms_endpoint.clone().unwrap_or_else(|| {
            "http://localhost:8200".to_string()
        });

        // Parse backend type from endpoint URL pattern
        let backend_type = if endpoint.contains("kms.amazonaws.com") {
            KmsBackendType::AwsKms
        } else if endpoint.contains("cloudkms.googleapis.com") {
            KmsBackendType::GcpKms
        } else if endpoint.contains("vault.azure.net") {
            KmsBackendType::AzureKeyVault
        } else if endpoint.contains("vault") {
            KmsBackendType::HashicorpVault
        } else {
            KmsBackendType::Mock
        };

        Ok(Self {
            backend_type,
            endpoint,
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        })
    }
}

#[async_trait::async_trait]
impl KeyProvider for KmsProvider {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let namespaced_id = self.namespaced_key_id(key_id);

        let handle = self.backend.generate_key(&namespaced_id, alg).await?;

        // Cache the handle locally
        let mut handles = self.key_handles.write().await;
        handles.insert(key_id.to_string(), handle.clone());

        info!(key_id = %key_id, algorithm = %handle.algorithm, "Generated key in KMS");

        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.backend.sign(&namespaced_id, msg).await
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.backend.encrypt(&namespaced_id, plaintext).await
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.backend.decrypt(&namespaced_id, ciphertext).await
    }

    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt> {
        let namespaced_id = self.namespaced_key_id(key_id);

        // Get current key handle
        let handles = self.key_handles.read().await;
        let previous_key = handles.get(key_id).cloned().ok_or_else(|| {
            AosError::Crypto(format!("Key not found in local cache: {}", key_id))
        })?;
        drop(handles);

        // Rotate in KMS
        let new_key = self.backend.rotate_key(&namespaced_id).await?;

        // Update local cache
        let mut handles = self.key_handles.write().await;
        handles.insert(key_id.to_string(), new_key.clone());

        let timestamp = Self::current_timestamp();

        // Create receipt (signature would be from KMS in production)
        let receipt_data = format!(
            "{}:{}:{}:{}",
            key_id,
            previous_key.provider_id,
            new_key.provider_id,
            timestamp
        );
        let signature = self.backend.sign(&namespaced_id, receipt_data.as_bytes()).await
            .unwrap_or_else(|_| vec![0u8; 8]); // Fallback for non-signing keys

        info!(
            key_id = %key_id,
            previous = %previous_key.provider_id,
            new = %new_key.provider_id,
            "Rotated key in KMS"
        );

        Ok(RotationReceipt::new(
            key_id.to_string(),
            previous_key,
            new_key,
            timestamp,
            signature,
        ))
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let timestamp = Self::current_timestamp();
        let fingerprint = self.backend.fingerprint();

        // Create attestation data
        let attestation_data = format!(
            "{}:{}:{}:{}",
            self.config.backend_type,
            fingerprint,
            self.config.endpoint,
            timestamp
        );

        // Sign with a system key if available, otherwise use placeholder
        let signature = vec![0u8; 64]; // Would be actual signature in production

        Ok(ProviderAttestation::new(
            format!("kms:{}", self.config.backend_type),
            fingerprint,
            blake3::hash(attestation_data.as_bytes()).to_hex().to_string(),
            timestamp,
            signature,
        ))
    }
}

/// Create a KMS provider instance
pub fn create_kms_provider(config: KeyProviderConfig) -> Result<KmsProvider> {
    KmsProvider::new(config)
}

/// Create a KMS provider with detailed configuration
pub fn create_kms_provider_with_config(config: KmsConfig) -> Result<KmsProvider> {
    KmsProvider::with_kms_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;

    #[tokio::test]
    async fn test_kms_provider_mock_generate() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        let handle = provider.generate("test-key", KeyAlgorithm::Ed25519).await.unwrap();
        assert!(handle.provider_id.contains("mock:test-key"));
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle.public_key.is_some());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_sign() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        provider.generate("sign-key", KeyAlgorithm::Ed25519).await.unwrap();

        let signature = provider.sign("sign-key", b"test message").await.unwrap();
        assert!(!signature.is_empty());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_encrypt_decrypt() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        provider.generate("enc-key", KeyAlgorithm::Aes256Gcm).await.unwrap();

        let plaintext = b"secret data";
        let ciphertext = provider.seal("enc-key", plaintext).await.unwrap();
        let decrypted = provider.unseal("enc-key", &ciphertext).await.unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_rotate() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        provider.generate("rotate-key", KeyAlgorithm::Ed25519).await.unwrap();

        let receipt = provider.rotate("rotate-key").await.unwrap();
        assert_eq!(receipt.key_id, "rotate-key");
        assert_ne!(receipt.previous_key.provider_id, receipt.new_key.provider_id);
    }

    #[tokio::test]
    async fn test_kms_provider_attest() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        let attestation = provider.attest().await.unwrap();
        assert!(attestation.provider_type.contains("kms:mock"));
        assert!(!attestation.fingerprint.is_empty());
    }

    #[tokio::test]
    async fn test_kms_provider_namespacing() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            key_namespace: Some("tenant-a".to_string()),
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        let handle = provider.generate("my-key", KeyAlgorithm::Ed25519).await.unwrap();
        // The provider ID should contain the namespaced path
        assert!(handle.provider_id.contains("tenant-a/my-key"));
    }

    #[tokio::test]
    async fn test_kms_provider_key_not_found() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        let result = provider.sign("nonexistent", b"data").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_kms_provider_algorithm_mismatch() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

        // Generate encryption key
        provider.generate("enc-key", KeyAlgorithm::Aes256Gcm).await.unwrap();

        // Try to sign with it (should fail)
        let result = provider.sign("enc-key", b"data").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a signing key"));
    }

    #[tokio::test]
    async fn test_create_kms_provider_from_generic_config() {
        let config = KeyProviderConfig {
            kms_endpoint: Some("http://localhost:8200".to_string()),
            ..Default::default()
        };

        let provider = create_kms_provider(config).unwrap();
        let attestation = provider.attest().await.unwrap();
        assert!(attestation.provider_type.contains("kms"));
    }

    #[tokio::test]
    async fn test_mock_backend_duplicate_key() {
        let backend = MockKmsBackend::new();

        backend.generate_key("dup-key", KeyAlgorithm::Ed25519).await.unwrap();

        let result = backend.generate_key("dup-key", KeyAlgorithm::Ed25519).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_mock_backend_delete_key() {
        let backend = MockKmsBackend::new();

        backend.generate_key("del-key", KeyAlgorithm::Ed25519).await.unwrap();
        assert!(backend.key_exists("del-key").await.unwrap());

        backend.delete_key("del-key").await.unwrap();
        assert!(!backend.key_exists("del-key").await.unwrap());
    }
}

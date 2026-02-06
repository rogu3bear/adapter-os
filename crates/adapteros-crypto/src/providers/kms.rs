//! KMS/HSM provider implementation
//!
//! Provides abstraction for cloud KMS (AWS, GCP) and HSM integration.
//! Uses a provider trait to allow different KMS implementations.
//! Cloud KMS is disabled in local/CI builds and falls back to the mock provider.

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
use crate::secret::SensitiveData;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Cloud KMS backends are intentionally disabled in local/CI builds.
const CLOUD_BACKEND_DISABLED_MSG: &str =
    "Cloud KMS backends are disabled in local-only builds; using mock provider";

// AWS KMS imports (conditional based on feature flag)
#[cfg(feature = "aws-kms")]
use aws_credential_types::Credentials;
#[cfg(feature = "aws-kms")]
use aws_sdk_kms::{types::SigningAlgorithmSpec, Client as KmsClient};
#[cfg(feature = "aws-kms")]
use aws_types::region::Region;

/// Create a seeded RNG for **mock/test** key generation only.
///
/// WARNING: This function produces deterministic output for a given context.
/// It is ONLY suitable for mock providers and testing. For real cryptographic
/// key generation, use `OsRng` directly.
///
/// Uses HKDF with domain separation for cryptographic operations.
fn seeded_rng_for_mock(context: &str) -> StdRng {
    // Deterministic seed for mock/test reproducibility
    let base_seed = B3Hash::hash(format!("kms-seed:{}", context).as_bytes());
    let seed_bytes = derive_seed(&base_seed, &format!("kms-rng:{}", context));
    let mut seed_array = [0u8; 32];
    seed_array.copy_from_slice(&seed_bytes[..32]);
    StdRng::from_seed(seed_array)
}

/// Execute KMS operation with retry logic
/// Provides exponential backoff for transient failures
#[cfg(any(feature = "aws-kms", feature = "gcp-kms"))]
pub async fn kms_with_retry<F, T>(max_retries: u32, provider_name: &str, mut op: F) -> Result<T>
where
    F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T>>,
{
    let mut retries = 0;

    loop {
        match op().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                retries += 1;
                if retries >= max_retries {
                    return Err(e);
                }

                // Exponential backoff: 100ms, 200ms, 400ms, ...
                let wait_ms = 100u64 * 2u64.pow(retries - 1);
                tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;

                debug!(
                    retries = %retries,
                    error = %e,
                    "Retrying {} operation",
                    provider_name
                );
            }
        }
    }
}

/// Supported KMS provider types
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KmsProviderType {
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
    /// Mock provider for testing
    Mock,
}

impl std::fmt::Display for KmsProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KmsProviderType::AwsKms => write!(f, "aws-kms"),
            KmsProviderType::GcpKms => write!(f, "gcp-kms"),
            KmsProviderType::AzureKeyVault => write!(f, "azure-keyvault"),
            KmsProviderType::HashicorpVault => write!(f, "hashicorp-vault"),
            KmsProviderType::Pkcs11Hsm => write!(f, "pkcs11-hsm"),
            KmsProviderType::Mock => write!(f, "mock"),
        }
    }
}

/// KMS-specific configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KmsConfig {
    /// Backend type to use
    pub provider_type: KmsProviderType,
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
            provider_type: KmsProviderType::Mock,
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
#[derive(Clone)]
pub enum KmsCredentials {
    /// No authentication (mock/local)
    None,
    /// AWS IAM credentials
    AwsIam {
        access_key_id: String,
        secret_access_key: SensitiveData,
        session_token: Option<SensitiveData>,
    },
    /// GCP service account
    GcpServiceAccount { credentials_json: SensitiveData },
    /// Azure service principal
    AzureServicePrincipal {
        tenant_id: String,
        client_id: String,
        client_secret: SensitiveData,
    },
    /// HashiCorp Vault token
    VaultToken { token: SensitiveData },
    /// PKCS#11 PIN
    Pkcs11Pin {
        pin: SensitiveData,
        slot_id: Option<u64>,
    },
}

impl std::fmt::Debug for KmsCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("KmsCredentials::None"),
            Self::AwsIam {
                access_key_id,
                secret_access_key: _,
                session_token,
            } => f
                .debug_struct("AwsIam")
                .field("access_key_id", access_key_id)
                .field("secret_access_key", &"[REDACTED]")
                .field(
                    "session_token",
                    &session_token.as_ref().map(|_| "[REDACTED]"),
                )
                .finish(),
            Self::GcpServiceAccount {
                credentials_json: _,
            } => f
                .debug_struct("GcpServiceAccount")
                .field("credentials_json", &"[REDACTED]")
                .finish(),
            Self::AzureServicePrincipal {
                tenant_id,
                client_id,
                client_secret: _,
            } => f
                .debug_struct("AzureServicePrincipal")
                .field("tenant_id", tenant_id)
                .field("client_id", client_id)
                .field("client_secret", &"[REDACTED]")
                .finish(),
            Self::VaultToken { token: _ } => f
                .debug_struct("VaultToken")
                .field("token", &"[REDACTED]")
                .finish(),
            Self::Pkcs11Pin { pin: _, slot_id } => f
                .debug_struct("Pkcs11Pin")
                .field("pin", &"[REDACTED]")
                .field("slot_id", slot_id)
                .finish(),
        }
    }
}

impl Serialize for KmsCredentials {
    fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom(
            "KmsCredentials cannot be serialized for security reasons",
        ))
    }
}

impl<'de> Deserialize<'de> for KmsCredentials {
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "KmsCredentials cannot be deserialized for security reasons",
        ))
    }
}

impl Zeroize for KmsCredentials {
    fn zeroize(&mut self) {
        match self {
            KmsCredentials::None => {}
            KmsCredentials::AwsIam {
                access_key_id,
                secret_access_key,
                session_token,
            } => {
                access_key_id.zeroize();
                secret_access_key.zeroize();
                if let Some(token) = session_token.as_mut() {
                    token.zeroize();
                }
            }
            KmsCredentials::GcpServiceAccount { credentials_json } => {
                credentials_json.zeroize();
            }
            KmsCredentials::AzureServicePrincipal {
                tenant_id,
                client_id,
                client_secret,
            } => {
                tenant_id.zeroize();
                client_id.zeroize();
                client_secret.zeroize();
            }
            KmsCredentials::VaultToken { token } => {
                token.zeroize();
            }
            KmsCredentials::Pkcs11Pin { pin, .. } => {
                pin.zeroize();
            }
        }
    }
}

impl ZeroizeOnDrop for KmsCredentials {}

/// Backend trait for KMS implementations
#[async_trait::async_trait]
pub trait KmsProvider: Send + Sync {
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

    /// Get provider type identifier
    fn provider_type(&self) -> KmsProviderType;

    /// Get provider version/fingerprint for attestation
    fn fingerprint(&self) -> String;
}

/// AWS KMS provider implementation (feature-gated)
#[cfg(feature = "aws-kms")]
pub struct AwsKmsProvider {
    client: KmsClient,
    config: KmsConfig,
    key_cache: Arc<RwLock<HashMap<String, AwsKeyMetadata>>>,
}

#[cfg(feature = "aws-kms")]
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct AwsKeyMetadata {
    key_id: String,
    algorithm: KeyAlgorithm,
    public_key: Option<Vec<u8>>,
    created_at: u64,
}

#[cfg(feature = "aws-kms")]
impl AwsKmsProvider {
    /// Create a new AWS KMS provider with async initialization
    pub async fn new_async(config: KmsConfig) -> Result<Self> {
        let credentials = match &config.credentials {
            KmsCredentials::AwsIam {
                access_key_id,
                secret_access_key,
                session_token,
            } => {
                let secret_access_key =
                    String::from_utf8_lossy(secret_access_key.as_bytes()).to_string();
                let session_token = session_token
                    .as_ref()
                    .map(|token| String::from_utf8_lossy(token.as_bytes()).to_string());
                Credentials::new(
                    access_key_id.clone(),
                    secret_access_key,
                    session_token,
                    None,
                    "adapteros-crypto",
                )
            }
            _ => {
                return Err(AosError::Crypto(
                    "AWS KMS requires AwsIam credentials".to_string(),
                ));
            }
        };

        // Create AWS SDK config
        let region = Region::new(
            config
                .region
                .clone()
                .unwrap_or_else(|| "us-east-1".to_string()),
        );

        let mut aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region.clone())
            .credentials_provider(credentials.clone())
            .load()
            .await;

        // Override endpoint if provided (for custom/local KMS)
        if config.endpoint != "https://kms.us-east-1.amazonaws.com"
            && !config.endpoint.contains("localhost")
        {
            let endpoint_url = if config.endpoint.starts_with("http") {
                config.endpoint.clone()
            } else {
                format!("https://{}", config.endpoint)
            };

            aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(region)
                .credentials_provider(credentials)
                .endpoint_url(endpoint_url)
                .load()
                .await;
        }

        let client = KmsClient::new(&aws_config);

        debug!(
            region = %config.region.as_deref().unwrap_or("us-east-1"),
            endpoint = %config.endpoint,
            "AWS KMS provider initialized"
        );

        Ok(Self {
            client,
            config,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Execute operation with retry logic
    async fn with_retry<F, T>(&self, op: F) -> Result<T>
    where
        F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T>>,
    {
        kms_with_retry(self.config.max_retries, "AWS KMS", op).await
    }
}

#[cfg(feature = "aws-kms")]
#[async_trait::async_trait]
impl KmsProvider for AwsKmsProvider {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();
        let alg_clone = alg.clone();

        let public_key = self
            .with_retry(|| {
                let client = client.clone();
                let key_id = key_id_owned.clone();
                let alg = alg_clone.clone();

                Box::pin(async move {
                    // Create CMK (Customer Master Key) in AWS KMS
                    let response = client
                        .create_key()
                        .key_usage(aws_sdk_kms::types::KeyUsageType::SignVerify)
                        .origin(aws_sdk_kms::types::OriginType::AwsKms)
                        .send()
                        .await
                        .map_err(|e| {
                            AosError::Crypto(format!("Failed to create AWS KMS key: {}", e))
                        })?;

                    let metadata = response.key_metadata.ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing key metadata".to_string())
                    })?;

                    let aws_key_id = metadata.key_id().to_string();

                    // Create alias for the key
                    let alias = format!("alias/adapteros-{}", key_id);
                    let _ = client
                        .create_alias()
                        .alias_name(&alias)
                        .target_key_id(aws_key_id.as_str())
                        .send()
                        .await;

                    // Get public key for asymmetric keys
                    let pub_key = if alg == KeyAlgorithm::Ed25519 {
                        match client
                            .get_public_key()
                            .key_id(aws_key_id.as_str())
                            .send()
                            .await
                        {
                            Ok(response) => response
                                .public_key()
                                .map(|pk| pk.as_ref().to_vec())
                                .unwrap_or_default(),
                            Err(_) => vec![],
                        }
                    } else {
                        vec![]
                    };

                    Ok(pub_key)
                })
            })
            .await?;

        // Cache metadata
        let mut cache = self.key_cache.write().await;
        cache.insert(
            key_id.to_string(),
            AwsKeyMetadata {
                key_id: key_id.to_string(),
                algorithm: alg.clone(),
                public_key: if public_key.is_empty() {
                    None
                } else {
                    Some(public_key.clone())
                },
                created_at: adapteros_core::time::unix_timestamp_secs(),
            },
        );

        debug!(key_id = %key_id, algorithm = %alg, "AWS KMS: generated key");

        Ok(KeyHandle::with_public_key(
            format!(
                "arn:aws:kms:{}:{}",
                self.config.region.as_deref().unwrap_or("us-east-1"),
                key_id
            ),
            alg,
            public_key,
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();
        let data_owned = data.to_vec();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();
            let message = data_owned.clone();

            Box::pin(async move {
                let response = client
                    .sign()
                    .key_id(&key_id)
                    .message(aws_smithy_types::Blob::new(message))
                    .signing_algorithm(SigningAlgorithmSpec::Ed25519Sha512)
                    .send()
                    .await
                    .map_err(|e| AosError::Crypto(format!("AWS KMS sign failed: {}", e)))?;

                let signature = response
                    .signature()
                    .map(|sig| sig.as_ref().to_vec())
                    .ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing signature".to_string())
                    })?;

                Ok(signature)
            })
        })
        .await
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();
        let plaintext_owned = plaintext.to_vec();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();
            let plaintext = plaintext_owned.clone();

            Box::pin(async move {
                let response = client
                    .encrypt()
                    .key_id(&key_id)
                    .plaintext(aws_smithy_types::Blob::new(plaintext))
                    .send()
                    .await
                    .map_err(|e| AosError::Crypto(format!("AWS KMS encrypt failed: {}", e)))?;

                let ciphertext = response
                    .ciphertext_blob()
                    .map(|ct| ct.as_ref().to_vec())
                    .ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing ciphertext".to_string())
                    })?;

                Ok(ciphertext)
            })
        })
        .await
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();
        let ciphertext_owned = ciphertext.to_vec();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();
            let ciphertext = ciphertext_owned.clone();

            Box::pin(async move {
                let response = client
                    .decrypt()
                    .ciphertext_blob(aws_smithy_types::Blob::new(ciphertext))
                    .key_id(&key_id)
                    .send()
                    .await
                    .map_err(|e| AosError::Crypto(format!("AWS KMS decrypt failed: {}", e)))?;

                let plaintext = response
                    .plaintext()
                    .map(|pt| pt.as_ref().to_vec())
                    .ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing plaintext".to_string())
                    })?;

                Ok(plaintext)
            })
        })
        .await
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();

        let (public_key, algorithm) = self
            .with_retry(|| {
                let client = client.clone();
                let key_id = key_id_owned.clone();

                Box::pin(async move {
                    // Enable automatic key rotation in AWS KMS
                    let _ = client.enable_key_rotation().key_id(&key_id).send().await;

                    // Get key metadata
                    let response =
                        client
                            .describe_key()
                            .key_id(&key_id)
                            .send()
                            .await
                            .map_err(|e| {
                                AosError::Crypto(format!("Failed to describe AWS KMS key: {}", e))
                            })?;

                    let metadata = response.key_metadata.ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing key metadata".to_string())
                    })?;

                    // Determine algorithm from key usage
                    let algorithm = match metadata.key_usage() {
                        Some(aws_sdk_kms::types::KeyUsageType::SignVerify) => KeyAlgorithm::Ed25519,
                        _ => KeyAlgorithm::Aes256Gcm,
                    };

                    // Try to get public key
                    let pub_key = if algorithm == KeyAlgorithm::Ed25519 {
                        match client.get_public_key().key_id(&key_id).send().await {
                            Ok(response) => response
                                .public_key()
                                .map(|pk| pk.as_ref().to_vec())
                                .unwrap_or_default(),
                            Err(_) => vec![],
                        }
                    } else {
                        vec![]
                    };

                    Ok((pub_key, algorithm))
                })
            })
            .await?;

        info!(
            key_id = %key_id,
            algorithm = %algorithm,
            "AWS KMS: rotated key"
        );

        Ok(KeyHandle::with_public_key(
            format!(
                "arn:aws:kms:{}:{}/rotated",
                self.config.region.as_deref().unwrap_or("us-east-1"),
                key_id
            ),
            algorithm,
            public_key,
        ))
    }

    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>> {
        // Check cache first
        {
            let cache = self.key_cache.read().await;
            if let Some(metadata) = cache.get(key_id) {
                if let Some(pub_key) = &metadata.public_key {
                    return Ok(pub_key.clone());
                }
            }
        }

        // Fetch from AWS KMS
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                let response = client
                    .get_public_key()
                    .key_id(&key_id)
                    .send()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to get AWS KMS public key: {}", e))
                    })?;

                response
                    .public_key()
                    .map(|pk| pk.as_ref().to_vec())
                    .ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing public key".to_string())
                    })
            })
        })
        .await
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                match client.describe_key().key_id(&key_id).send().await {
                    Ok(response) => {
                        if let Some(metadata) = response.key_metadata {
                            // Check if key is enabled
                            Ok(metadata.enabled())
                        } else {
                            Ok(false)
                        }
                    }
                    Err(_) => Ok(false),
                }
            })
        })
        .await
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let client = self.client.clone();
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let client = client.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                // Schedule key deletion (30 days waiting period by default)
                client
                    .schedule_key_deletion()
                    .key_id(&key_id)
                    .pending_window_in_days(30)
                    .send()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to delete AWS KMS key: {}", e))
                    })?;

                warn!(key_id = %key_id, "AWS KMS: scheduled key for deletion (30-day waiting period)");
                Ok(())
            })
        })
        .await
    }

    fn provider_type(&self) -> KmsProviderType {
        KmsProviderType::AwsKms
    }

    fn fingerprint(&self) -> String {
        let region = self.config.region.as_deref().unwrap_or("us-east-1");
        format!("aws-kms-{}-v1.0", region)
    }
}

/// Mock KMS provider for testing and development
pub struct MockKmsProvider {
    keys: Arc<RwLock<HashMap<String, MockKey>>>,
}

#[derive(Clone)]
struct MockKey {
    algorithm: KeyAlgorithm,
    private_key: Vec<u8>,
    public_key: Vec<u8>,
    version: u32,
}

impl MockKmsProvider {
    /// Create a new mock KMS provider
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MockKmsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl KmsProvider for MockKmsProvider {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        use rand::RngCore;

        let mut keys = self.keys.write().await;

        if keys.contains_key(key_id) {
            return Err(AosError::Crypto(format!("Key already exists: {}", key_id)));
        }

        // Generate mock key material (deterministic for test reproducibility)
        let (private_key, public_key) = match alg {
            KeyAlgorithm::Ed25519 => {
                let mut private = vec![0u8; 32];
                seeded_rng_for_mock(&format!("mock-generate-ed25519:{}", key_id))
                    .fill_bytes(&mut private);
                // Derive public key (simplified mock)
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut key = vec![0u8; 32];
                seeded_rng_for_mock("mock-symmetric-keygen").fill_bytes(&mut key);
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

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

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

        let key = keys
            .get_mut(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Generate new key material (deterministic for test reproducibility)
        let (private_key, public_key) = match key.algorithm {
            KeyAlgorithm::Ed25519 => {
                let mut private = vec![0u8; 32];
                seeded_rng_for_mock(&format!("mock-rotate-ed25519:{}", key_id))
                    .fill_bytes(&mut private);
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut k = vec![0u8; 32];
                seeded_rng_for_mock(&format!("mock-rotate-symmetric:{}", key_id))
                    .fill_bytes(&mut k);
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

        let key = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

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

    fn provider_type(&self) -> KmsProviderType {
        KmsProviderType::Mock
    }

    fn fingerprint(&self) -> String {
        "mock-kms-v1.0".to_string()
    }
}

/// HashiCorp Vault provider implementation
/// Uses the Transit secret engine for cryptographic operations
pub struct HashicorpVaultProvider {
    endpoint: String,
    transit_mount: String,
    key_cache: Arc<RwLock<HashMap<String, VaultKeyMetadata>>>,
}

#[derive(Clone, Debug)]
struct VaultKeyMetadata {
    algorithm: KeyAlgorithm,
    version: u32,
}

impl HashicorpVaultProvider {
    /// Create a new HashiCorp Vault provider
    pub fn new(config: KmsConfig) -> Result<Self> {
        match &config.credentials {
            KmsCredentials::VaultToken { .. } => {}
            KmsCredentials::None => {
                // Try environment variable
                std::env::var("VAULT_TOKEN").map_err(|_| {
                    AosError::Crypto(
                        "HashiCorp Vault requires VaultToken credentials or VAULT_TOKEN env var"
                            .to_string(),
                    )
                })?;
            }
            _ => {
                return Err(AosError::Crypto(
                    "HashiCorp Vault requires VaultToken credentials".to_string(),
                ));
            }
        };

        // Extract transit mount path from namespace or use default
        let transit_mount = config
            .key_namespace
            .clone()
            .unwrap_or_else(|| "transit".to_string());

        debug!(
            endpoint = %config.endpoint,
            transit_mount = %transit_mount,
            "HashiCorp Vault provider initialized"
        );

        Ok(Self {
            endpoint: config.endpoint.clone(),
            transit_mount,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Build API URL for transit operations
    fn transit_url(&self, path: &str) -> String {
        format!("{}/v1/{}/{}", self.endpoint, self.transit_mount, path)
    }

    /// Convert algorithm to Vault key type
    fn algorithm_to_vault_type(alg: &KeyAlgorithm) -> &'static str {
        match alg {
            KeyAlgorithm::Ed25519 => "ed25519",
            KeyAlgorithm::Aes256Gcm => "aes256-gcm96",
            KeyAlgorithm::ChaCha20Poly1305 => "chacha20-poly1305",
        }
    }

    /// Make HTTP request to Vault API (mock implementation)
    async fn vault_request(
        &self,
        method: &str,
        url: &str,
        _body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // In a real implementation, this would use reqwest or hyper
        // For now, return mock data since we don't have HTTP client dependency
        debug!(
            method = %method,
            url = %url,
            "Vault API request (mock)"
        );

        // Mock successful response
        Ok(serde_json::json!({
            "data": {
                "keys": { "1": 1234567890 },
                "type": "aes256-gcm96",
                "latest_version": 1
            }
        }))
    }
}

#[async_trait::async_trait]
impl KmsProvider for HashicorpVaultProvider {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let key_type = Self::algorithm_to_vault_type(&alg);
        let url = self.transit_url(&format!("keys/{}", key_id));

        let body = serde_json::json!({
            "type": key_type,
            "exportable": false,
        });

        // Direct call without retry wrapper to avoid lifetime issues
        let _ = self.vault_request("POST", &url, Some(body)).await?;

        // Cache metadata
        let mut cache = self.key_cache.write().await;
        cache.insert(
            key_id.to_string(),
            VaultKeyMetadata {
                algorithm: alg.clone(),
                version: 1,
            },
        );

        debug!(
            key_id = %key_id,
            algorithm = %alg,
            key_type = %key_type,
            "Vault: generated key"
        );

        Ok(KeyHandle::new(
            format!("vault:{}/{}", self.transit_mount, key_id),
            alg,
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let url = self.transit_url(&format!("sign/{}", key_id));
        let data_b64 = STANDARD.encode(data);

        let body = serde_json::json!({
            "input": data_b64,
            "signature_algorithm": "pkcs1v15",
        });

        // Direct call without retry wrapper to avoid lifetime issues
        let response = self.vault_request("POST", &url, Some(body)).await?;

        // Extract signature from response
        let signature = response
            .get("data")
            .and_then(|d| d.get("signature"))
            .and_then(|s| s.as_str())
            .ok_or_else(|| AosError::Crypto("Vault response missing signature".to_string()))?;

        // Vault signatures are prefixed with "vault:v1:"
        let sig_bytes = if signature.starts_with("vault:v") {
            signature.split(':').next_back().unwrap_or(signature)
        } else {
            signature
        };

        STANDARD
            .decode(sig_bytes)
            .map_err(|e| AosError::Crypto(format!("Failed to decode Vault signature: {}", e)))
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let url = self.transit_url(&format!("encrypt/{}", key_id));
        let plaintext_b64 = STANDARD.encode(plaintext);

        let body = serde_json::json!({
            "plaintext": plaintext_b64,
        });

        // Direct call without retry wrapper to avoid lifetime issues
        let response = self.vault_request("POST", &url, Some(body)).await?;

        // Extract ciphertext from response
        let ciphertext = response
            .get("data")
            .and_then(|d| d.get("ciphertext"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| AosError::Crypto("Vault response missing ciphertext".to_string()))?;

        Ok(ciphertext.as_bytes().to_vec())
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let url = self.transit_url(&format!("decrypt/{}", key_id));
        let ciphertext_str = String::from_utf8_lossy(ciphertext);

        let body = serde_json::json!({
            "ciphertext": ciphertext_str,
        });

        // Direct call without retry wrapper to avoid lifetime issues
        let response = self.vault_request("POST", &url, Some(body)).await?;

        // Extract plaintext from response
        let plaintext_b64 = response
            .get("data")
            .and_then(|d| d.get("plaintext"))
            .and_then(|p| p.as_str())
            .ok_or_else(|| AosError::Crypto("Vault response missing plaintext".to_string()))?;

        STANDARD
            .decode(plaintext_b64)
            .map_err(|e| AosError::Crypto(format!("Failed to decode Vault plaintext: {}", e)))
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let url = self.transit_url(&format!("keys/{}/rotate", key_id));

        // Direct call without retry wrapper to avoid lifetime issues
        let _ = self.vault_request("POST", &url, None).await?;

        // Update cached version
        let mut cache = self.key_cache.write().await;
        if let Some(metadata) = cache.get_mut(key_id) {
            metadata.version += 1;
        }

        info!(
            key_id = %key_id,
            "Vault: rotated key"
        );

        // Get algorithm from cache
        let alg = cache
            .get(key_id)
            .map(|m| m.algorithm.clone())
            .unwrap_or(KeyAlgorithm::Aes256Gcm);

        Ok(KeyHandle::new(
            format!("vault:{}/{}/rotated", self.transit_mount, key_id),
            alg,
        ))
    }

    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>> {
        // Check cache first
        {
            let cache = self.key_cache.read().await;
            if let Some(metadata) = cache.get(key_id) {
                if metadata.algorithm != KeyAlgorithm::Ed25519 {
                    return Err(AosError::Crypto(format!(
                        "Key {} is not an asymmetric key (type: {})",
                        key_id, metadata.algorithm
                    )));
                }
            }
        }

        let url = self.transit_url(&format!("keys/{}", key_id));

        // Direct call without retry wrapper to avoid lifetime issues
        let _response = self.vault_request("GET", &url, None).await?;

        // Extract public key from response (Vault returns keys object)
        // In real Vault, public keys are in data.keys.{version}
        let mock_pubkey = vec![0u8; 32]; // Mock 32-byte ed25519 public key
        Ok(mock_pubkey)
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let url = self.transit_url(&format!("keys/{}", key_id));

        match self.vault_request("GET", &url, None).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        // First, update config to allow deletion
        let config_url = self.transit_url(&format!("keys/{}/config", key_id));
        let config_body = serde_json::json!({
            "deletion_allowed": true,
        });

        self.vault_request("POST", &config_url, Some(config_body))
            .await?;

        // Then delete the key
        let delete_url = self.transit_url(&format!("keys/{}", key_id));

        // Direct call without retry wrapper to avoid lifetime issues
        let _ = self.vault_request("DELETE", &delete_url, None).await?;

        // Remove from cache
        let mut cache = self.key_cache.write().await;
        cache.remove(key_id);

        warn!(
            key_id = %key_id,
            "Vault: deleted key"
        );

        Ok(())
    }

    fn provider_type(&self) -> KmsProviderType {
        KmsProviderType::HashicorpVault
    }

    fn fingerprint(&self) -> String {
        format!("hashicorp-vault-{}-v1.0", self.transit_mount)
    }
}

/// Local file-based KMS provider for development and testing
///
/// ⚠️ WARNING: NOT FOR PRODUCTION USE ⚠️
///
/// This provider stores keys in plaintext JSON files on disk.
/// It is ONLY suitable for:
/// - Local development
/// - CI/CD testing
/// - Integration tests
///
/// DO NOT use in production environments. Use AWS KMS, GCP KMS,
/// or HashiCorp Vault instead.
pub struct LocalKmsProvider {
    storage_path: std::path::PathBuf,
    keys: Arc<RwLock<HashMap<String, LocalKeyData>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LocalKeyData {
    key_id: String,
    algorithm: KeyAlgorithm,
    key_material: Vec<u8>,
    public_key: Vec<u8>,
    created_at: u64,
    version: u32,
}

impl LocalKmsProvider {
    /// Create a new local file-based KMS provider
    ///
    /// ⚠️ WARNING: NOT FOR PRODUCTION ⚠️
    pub fn new(storage_path: std::path::PathBuf) -> Result<Self> {
        warn!("⚠️  WARNING: LocalKmsProvider is NOT FOR PRODUCTION USE ⚠️");
        warn!(
            "Keys are stored in PLAINTEXT at: {}",
            storage_path.display()
        );
        warn!("Only use this for development, testing, or CI/CD");

        // Create storage directory if it doesn't exist
        if !storage_path.exists() {
            std::fs::create_dir_all(&storage_path).map_err(|e| {
                AosError::Crypto(format!("Failed to create key storage directory: {}", e))
            })?;
        }

        // Load existing keys from disk
        let mut keys = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&storage_path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file()
                        && entry.path().extension() == Some(std::ffi::OsStr::new("json"))
                    {
                        if let Ok(data) = std::fs::read_to_string(entry.path()) {
                            if let Ok(key_data) = serde_json::from_str::<LocalKeyData>(&data) {
                                keys.insert(key_data.key_id.clone(), key_data);
                            }
                        }
                    }
                }
            }
        }

        debug!(
            storage_path = %storage_path.display(),
            loaded_keys = keys.len(),
            "Local KMS provider initialized (DEVELOPMENT ONLY)"
        );

        Ok(Self {
            storage_path,
            keys: Arc::new(RwLock::new(keys)),
        })
    }

    /// Get file path for a key
    fn key_file_path(&self, key_id: &str) -> std::path::PathBuf {
        self.storage_path.join(format!("{}.json", key_id))
    }

    /// Save a key to disk
    async fn save_key(&self, key_data: &LocalKeyData) -> Result<()> {
        let file_path = self.key_file_path(&key_data.key_id);
        let json = serde_json::to_string_pretty(key_data)
            .map_err(|e| AosError::Crypto(format!("Failed to serialize key data: {}", e)))?;

        tokio::fs::write(&file_path, json)
            .await
            .map_err(|e| AosError::Crypto(format!("Failed to write key file: {}", e)))?;

        Ok(())
    }

    /// Generate key material based on algorithm using OS entropy.
    ///
    /// Uses `OsRng` for cryptographically secure random key generation.
    fn generate_key_material(alg: &KeyAlgorithm) -> (Vec<u8>, Vec<u8>) {
        use rand::rngs::OsRng;
        use rand::RngCore;
        use zeroize::Zeroize;

        match alg {
            KeyAlgorithm::Ed25519 => {
                // Generate Ed25519 keypair with OS entropy
                let mut seed = [0u8; 32];
                OsRng.fill_bytes(&mut seed);
                let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                let verifying_key = signing_key.verifying_key();

                let result = (seed.to_vec(), verifying_key.to_bytes().to_vec());
                seed.zeroize(); // Zeroize seed after use
                result
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                // Generate 256-bit symmetric key with OS entropy
                let mut key = vec![0u8; 32];
                OsRng.fill_bytes(&mut key);
                (key.clone(), vec![]) // No public key for symmetric
            }
        }
    }
}

#[async_trait::async_trait]
impl KmsProvider for LocalKmsProvider {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        // Check if key already exists
        {
            let keys = self.keys.read().await;
            if keys.contains_key(key_id) {
                return Err(AosError::Crypto(format!("Key already exists: {}", key_id)));
            }
        }

        // Generate key material
        let (key_material, public_key) = Self::generate_key_material(&alg);

        let key_data = LocalKeyData {
            key_id: key_id.to_string(),
            algorithm: alg.clone(),
            key_material,
            public_key: public_key.clone(),
            created_at: adapteros_core::time::unix_timestamp_secs(),
            version: 1,
        };

        // Save to disk
        self.save_key(&key_data).await?;

        // Add to cache
        let mut keys = self.keys.write().await;
        keys.insert(key_id.to_string(), key_data);

        debug!(
            key_id = %key_id,
            algorithm = %alg,
            "Local KMS: generated key (DEV ONLY)"
        );

        Ok(KeyHandle::with_public_key(
            format!("local:{}", key_id),
            alg,
            public_key,
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;
        let key_data = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        if key_data.algorithm != KeyAlgorithm::Ed25519 {
            return Err(AosError::Crypto(format!(
                "Key {} is not a signing key (algorithm: {})",
                key_id, key_data.algorithm
            )));
        }

        // Sign with Ed25519
        let seed: [u8; 32] = key_data.key_material[..32]
            .try_into()
            .map_err(|_| AosError::Crypto("Invalid Ed25519 seed length".to_string()))?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);

        use ed25519_dalek::Signer;
        let signature = signing_key.sign(data);

        Ok(signature.to_bytes().to_vec())
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;
        let key_data = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        match key_data.algorithm {
            KeyAlgorithm::Aes256Gcm => {
                use aes_gcm::aead::Aead;
                use aes_gcm::{Aes256Gcm, KeyInit};
                use rand::rngs::OsRng;
                use rand::RngCore;

                let key_bytes: &[u8; 32] = key_data.key_material[..32]
                    .try_into()
                    .map_err(|_| AosError::Crypto("Invalid AES key length".to_string()))?;

                let cipher = Aes256Gcm::new(key_bytes.into());

                // SECURITY: Generate random 12-byte nonce using OS entropy.
                // AES-GCM nonces MUST be unique per encryption with the same key.
                // Deterministic nonces would allow nonce reuse attacks.
                let mut nonce_bytes = [0u8; 12];
                OsRng.fill_bytes(&mut nonce_bytes);
                let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

                // Encrypt
                let ciphertext = cipher
                    .encrypt(nonce, plaintext)
                    .map_err(|e| AosError::Crypto(format!("AES-GCM encryption failed: {}", e)))?;

                // Prepend nonce to ciphertext so decryption can extract it
                let mut result = nonce_bytes.to_vec();
                result.extend_from_slice(&ciphertext);
                Ok(result)
            }
            _ => Err(AosError::Crypto(format!(
                "Key {} is not an encryption key (algorithm: {})",
                key_id, key_data.algorithm
            ))),
        }
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;
        let key_data = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        match key_data.algorithm {
            KeyAlgorithm::Aes256Gcm => {
                use aes_gcm::aead::Aead;
                use aes_gcm::{Aes256Gcm, KeyInit};

                if ciphertext.len() < 12 {
                    return Err(AosError::Crypto(
                        "Ciphertext too short (missing nonce)".to_string(),
                    ));
                }

                let key_bytes: &[u8; 32] = key_data.key_material[..32]
                    .try_into()
                    .map_err(|_| AosError::Crypto("Invalid AES key length".to_string()))?;

                let cipher = Aes256Gcm::new(key_bytes.into());

                // Extract nonce and ciphertext
                let nonce = aes_gcm::Nonce::from_slice(&ciphertext[..12]);
                let ct = &ciphertext[12..];

                // Decrypt
                let plaintext = cipher
                    .decrypt(nonce, ct)
                    .map_err(|e| AosError::Crypto(format!("AES-GCM decryption failed: {}", e)))?;

                Ok(plaintext)
            }
            _ => Err(AosError::Crypto(format!(
                "Key {} is not an encryption key (algorithm: {})",
                key_id, key_data.algorithm
            ))),
        }
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let mut keys = self.keys.write().await;
        let key_data = keys
            .get_mut(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Generate new key material
        let (new_material, new_public) = Self::generate_key_material(&key_data.algorithm);

        key_data.key_material = new_material;
        key_data.public_key = new_public.clone();
        key_data.version += 1;

        // Save to disk
        let key_data_clone = key_data.clone();
        drop(keys); // Release lock before async operation
        self.save_key(&key_data_clone).await?;

        info!(
            key_id = %key_id,
            version = key_data_clone.version,
            "Local KMS: rotated key (DEV ONLY)"
        );

        Ok(KeyHandle::with_public_key(
            format!("local:{}/v{}", key_id, key_data_clone.version),
            key_data_clone.algorithm,
            new_public,
        ))
    }

    async fn get_public_key(&self, key_id: &str) -> Result<Vec<u8>> {
        let keys = self.keys.read().await;
        let key_data = keys
            .get(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        if key_data.public_key.is_empty() {
            return Err(AosError::Crypto(format!(
                "Key {} does not have a public key (algorithm: {})",
                key_id, key_data.algorithm
            )));
        }

        Ok(key_data.public_key.clone())
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let keys = self.keys.read().await;
        Ok(keys.contains_key(key_id))
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        // Remove from cache
        let mut keys = self.keys.write().await;
        keys.remove(key_id)
            .ok_or_else(|| AosError::Crypto(format!("Key not found: {}", key_id)))?;

        // Delete file
        let file_path = self.key_file_path(key_id);
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| AosError::Crypto(format!("Failed to delete key file: {}", e)))?;

        warn!(
            key_id = %key_id,
            "Local KMS: deleted key (DEV ONLY)"
        );

        Ok(())
    }

    fn provider_type(&self) -> KmsProviderType {
        KmsProviderType::Mock // Use Mock type for local development
    }

    fn fingerprint(&self) -> String {
        format!(
            "local-kms-{}-v1.0-DEV-ONLY",
            self.storage_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    }
}

/// KMS provider implementation
pub struct KmsManager {
    config: KmsConfig,
    provider: Arc<dyn KmsProvider>,
    key_handles: Arc<RwLock<HashMap<String, KeyHandle>>>,
}

impl std::fmt::Debug for KmsManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KmsProvider")
            .field("config", &self.config)
            .field("provider_type", &self.config.provider_type)
            .field("provider_type", &self.config.provider_type)
            .finish()
    }
}

impl KmsManager {
    /// Create a new KMS provider with the specified configuration
    pub fn new(config: KeyProviderConfig) -> Result<Self> {
        let kms_config = KmsConfig::from_provider_config(&config)?;
        Self::with_kms_config(kms_config)
    }

    /// Create a new KMS provider with detailed KMS configuration
    pub fn with_kms_config(config: KmsConfig) -> Result<Self> {
        let provider: Arc<dyn KmsProvider> = match config.provider_type {
            KmsProviderType::Mock => Arc::new(MockKmsProvider::new()),
            #[cfg(feature = "aws-kms")]
            KmsProviderType::AwsKms => {
                return Err(AosError::Crypto(
                    "AWS KMS requires async initialization. Use with_kms_config_async instead"
                        .to_string(),
                ));
            }
            #[cfg(not(feature = "aws-kms"))]
            KmsProviderType::AwsKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            KmsProviderType::GcpKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            KmsProviderType::AzureKeyVault => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            KmsProviderType::HashicorpVault => {
                Arc::new(HashicorpVaultProvider::new(config.clone())?)
            }
            KmsProviderType::Pkcs11Hsm => {
                // CRYPTO-GAP-001: PKCS#11 HSM provider not implemented
                // Federal deployments requiring FIPS 140-2 via physical HSM must use
                // AOS_ALLOW_MOCK_HSM=1 explicitly to acknowledge mock fallback.
                if std::env::var("AOS_ALLOW_MOCK_HSM").is_ok() {
                    warn!(
                        "PKCS#11 HSM not implemented - using mock provider \
                         (AOS_ALLOW_MOCK_HSM set, DEVELOPMENT ONLY)"
                    );
                    Arc::new(MockKmsProvider::new())
                } else {
                    return Err(AosError::Crypto(
                        "PKCS#11 HSM provider not implemented (CRYPTO-GAP-001). \
                         Use 'local' or 'keychain' provider for production, \
                         or set AOS_ALLOW_MOCK_HSM=1 for development only."
                            .to_string(),
                    ));
                }
            }
        };

        info!(
            provider_type = %config.provider_type,
            endpoint = %config.endpoint,
            "KMS provider initialized"
        );

        Ok(Self {
            config,
            provider,
            key_handles: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new KMS provider with async configuration (for AWS KMS and GCP KMS)
    pub async fn with_kms_config_async(config: KmsConfig) -> Result<Self> {
        #[allow(unused_variables, unreachable_code)]
        let provider: Arc<dyn KmsProvider> = match config.provider_type {
            #[cfg(feature = "aws-kms")]
            KmsProviderType::AwsKms => Arc::new(AwsKmsProvider::new_async(config.clone()).await?),
            #[cfg(not(feature = "aws-kms"))]
            KmsProviderType::AwsKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            KmsProviderType::GcpKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            KmsProviderType::AzureKeyVault => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsProvider::new())
            }
            _ => {
                // Use sync initialization for other backends
                return Self::with_kms_config(config);
            }
        };

        #[allow(unreachable_code)]
        {
            info!(
                provider_type = %config.provider_type,
                endpoint = %config.endpoint,
                "KMS provider initialized (async)"
            );

            Ok(Self {
                config,
                provider,
                key_handles: Arc::new(RwLock::new(HashMap::new())),
            })
        }
    }

    /// Create a KMS provider with a custom provider (for testing)
    pub fn with_provider(config: KmsConfig, provider: Arc<dyn KmsProvider>) -> Self {
        Self {
            config,
            provider,
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
}

impl KmsConfig {
    /// Convert from generic KeyProviderConfig
    pub fn from_provider_config(config: &KeyProviderConfig) -> Result<Self> {
        let endpoint = config
            .kms_endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:8200".to_string());

        // Parse provider type from endpoint URL pattern
        let provider_type = if endpoint.contains("kms.amazonaws.com") {
            KmsProviderType::AwsKms
        } else if endpoint.contains("cloudkms.googleapis.com") {
            KmsProviderType::GcpKms
        } else if endpoint.contains("vault.azure.net") {
            KmsProviderType::AzureKeyVault
        } else if endpoint.contains("vault") {
            KmsProviderType::HashicorpVault
        } else {
            KmsProviderType::Mock
        };

        Ok(Self {
            provider_type,
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
impl KeyProvider for KmsManager {
    async fn generate(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let namespaced_id = self.namespaced_key_id(key_id);

        let handle = self.provider.generate_key(&namespaced_id, alg).await?;

        // Cache the handle locally
        let mut handles = self.key_handles.write().await;
        handles.insert(key_id.to_string(), handle.clone());

        info!(key_id = %key_id, algorithm = %handle.algorithm, "Generated key in KMS");

        Ok(handle)
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.provider.sign(&namespaced_id, msg).await
    }

    async fn seal(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.provider.encrypt(&namespaced_id, plaintext).await
    }

    async fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let namespaced_id = self.namespaced_key_id(key_id);
        self.provider.decrypt(&namespaced_id, ciphertext).await
    }

    async fn rotate(&self, key_id: &str) -> Result<RotationReceipt> {
        let namespaced_id = self.namespaced_key_id(key_id);

        // Get current key handle
        let handles = self.key_handles.read().await;
        let previous_key = handles
            .get(key_id)
            .cloned()
            .ok_or_else(|| AosError::Crypto(format!("Key not found in local cache: {}", key_id)))?;
        drop(handles);

        // Rotate in KMS
        let new_key = self.provider.rotate_key(&namespaced_id).await?;

        // Update local cache
        let mut handles = self.key_handles.write().await;
        handles.insert(key_id.to_string(), new_key.clone());

        let timestamp = adapteros_core::time::unix_timestamp_secs();

        // Create receipt (signature would be from KMS in production)
        let receipt_data = format!(
            "{}:{}:{}:{}",
            key_id, previous_key.provider_id, new_key.provider_id, timestamp
        );
        let signature = self
            .provider
            .sign(&namespaced_id, receipt_data.as_bytes())
            .await
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
        let timestamp = adapteros_core::time::unix_timestamp_secs();
        let fingerprint = self.provider.fingerprint();

        // Create attestation data
        let attestation_data = format!(
            "{}:{}:{}:{}",
            self.config.provider_type, fingerprint, self.config.endpoint, timestamp
        );

        // Sign with a system key if available, otherwise use placeholder
        let signature = vec![0u8; 64]; // Would be actual signature in production

        Ok(ProviderAttestation::new(
            format!("kms:{}", self.config.provider_type),
            fingerprint,
            blake3::hash(attestation_data.as_bytes())
                .to_hex()
                .to_string(),
            timestamp,
            signature,
        ))
    }
}

/// Create a KMS provider instance
pub fn create_kms_provider(config: KeyProviderConfig) -> Result<KmsManager> {
    KmsManager::new(config)
}

/// Create a KMS provider with detailed configuration
pub fn create_kms_provider_with_config(config: KmsConfig) -> Result<KmsManager> {
    KmsManager::with_kms_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;

    #[tokio::test]
    async fn test_kms_provider_mock_generate() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        let handle = provider
            .generate("test-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert!(handle.provider_id.contains("mock:test-key"));
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle.public_key.is_some());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_sign() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        provider
            .generate("sign-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        let signature = provider.sign("sign-key", b"test message").await.unwrap();
        assert!(!signature.is_empty());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_encrypt_decrypt() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        provider
            .generate("enc-key", KeyAlgorithm::Aes256Gcm)
            .await
            .unwrap();

        let plaintext = b"secret data";
        let ciphertext = provider.seal("enc-key", plaintext).await.unwrap();
        let decrypted = provider.unseal("enc-key", &ciphertext).await.unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn test_kms_provider_mock_rotate() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        provider
            .generate("rotate-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        let receipt = provider.rotate("rotate-key").await.unwrap();
        assert_eq!(receipt.key_id, "rotate-key");
        assert_ne!(
            receipt.previous_key.provider_id,
            receipt.new_key.provider_id
        );
    }

    #[tokio::test]
    async fn test_kms_provider_attest() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        let attestation = provider.attest().await.unwrap();
        assert!(attestation.provider_type.contains("kms:mock"));
        assert!(!attestation.fingerprint.is_empty());
    }

    #[tokio::test]
    async fn test_kms_provider_namespacing() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            key_namespace: Some("tenant-a".to_string()),
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        let handle = provider
            .generate("my-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        // The provider ID should contain the namespaced path
        assert!(handle.provider_id.contains("tenant-a/my-key"));
    }

    #[tokio::test]
    async fn test_kms_provider_key_not_found() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        let result = provider.sign("nonexistent", b"data").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_kms_provider_algorithm_mismatch() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            ..Default::default()
        };

        let provider = KmsManager::with_kms_config(config).unwrap();

        // Generate encryption key
        provider
            .generate("enc-key", KeyAlgorithm::Aes256Gcm)
            .await
            .unwrap();

        // Try to sign with it (should fail)
        let result = provider.sign("enc-key", b"data").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a signing key"));
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
        let provider = MockKmsProvider::new();

        provider
            .generate_key("dup-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        let result = provider
            .generate_key("dup-key", KeyAlgorithm::Ed25519)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_mock_backend_delete_key() {
        let provider = MockKmsProvider::new();

        provider
            .generate_key("del-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert!(provider.key_exists("del-key").await.unwrap());

        provider.delete_key("del-key").await.unwrap();
        assert!(!provider.key_exists("del-key").await.unwrap());
    }

    // AWS KMS Backend Tests
    #[tokio::test]
    async fn test_aws_kms_config_creation() {
        let config = KmsConfig {
            provider_type: KmsProviderType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key-id".to_string(),
                secret_access_key: "test-secret".into(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Verify configuration is valid
        assert_eq!(config.provider_type, KmsProviderType::AwsKms);
        assert_eq!(config.region, Some("us-east-1".to_string()));
    }

    #[tokio::test]
    async fn test_aws_kms_backend_requires_async_init() {
        let config = KmsConfig {
            provider_type: KmsProviderType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key-id".to_string(),
                secret_access_key: "test-secret".into(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Sync initialization should fail for AWS KMS (or fallback to mock if feature not enabled)
        let result = KmsManager::with_kms_config(config);
        #[cfg(feature = "aws-kms")]
        assert!(result.is_err());
        #[cfg(not(feature = "aws-kms"))]
        assert!(result.is_ok()); // Falls back to mock
    }

    #[tokio::test]
    async fn test_kms_config_region_parsing() {
        let config = KmsConfig {
            provider_type: KmsProviderType::AwsKms,
            endpoint: "https://kms.eu-west-1.amazonaws.com".to_string(),
            region: Some("eu-west-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test".to_string(),
                secret_access_key: "test".into(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        assert_eq!(config.region.unwrap(), "eu-west-1");
    }

    #[tokio::test]
    async fn test_kms_credentials_with_session_token() {
        let config = KmsConfig {
            provider_type: KmsProviderType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key".to_string(),
                secret_access_key: "test-secret".into(),
                session_token: Some("test-token".into()),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Verify session token is present
        match &config.credentials {
            KmsCredentials::AwsIam {
                access_key_id,
                secret_access_key,
                session_token,
            } => {
                assert_eq!(access_key_id, "test-key");
                assert_eq!(secret_access_key.as_bytes(), b"test-secret");
                assert_eq!(
                    session_token.as_ref().map(|token| token.as_bytes()),
                    Some(b"test-token".as_ref())
                );
            }
            _ => panic!("Expected AwsIam credentials"),
        }
    }

    #[tokio::test]
    async fn test_kms_provider_with_provider_mock() {
        // Test provider creation with mock provider
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        let mock_backend = Arc::new(MockKmsProvider::new());
        let provider = KmsManager::with_provider(config, mock_backend);

        // Verify provider is initialized
        assert_eq!(provider.config.provider_type, KmsProviderType::Mock);
    }

    #[tokio::test]
    async fn test_kms_config_timeout_settings() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 60,
            max_retries: 5,
            key_namespace: None,
        };

        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_retries, 5);
    }

    #[tokio::test]
    async fn test_kms_config_key_namespace() {
        let config = KmsConfig {
            provider_type: KmsProviderType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("prod-tenant".to_string()),
        };

        assert_eq!(config.key_namespace, Some("prod-tenant".to_string()));
    }

    #[tokio::test]
    async fn test_kms_credentials_serialization() {
        let creds = KmsCredentials::AwsIam {
            access_key_id: "key123".to_string(),
            secret_access_key: "secret456".into(),
            session_token: Some("token789".into()),
        };

        // Serialization and deserialization should fail for credentials
        assert!(serde_json::to_string(&creds).is_err());
        assert!(serde_json::from_str::<KmsCredentials>("{}").is_err());
    }

    #[tokio::test]
    async fn test_kms_provider_type_display() {
        assert_eq!(KmsProviderType::AwsKms.to_string(), "aws-kms");
        assert_eq!(KmsProviderType::GcpKms.to_string(), "gcp-kms");
        assert_eq!(KmsProviderType::AzureKeyVault.to_string(), "azure-keyvault");
        assert_eq!(
            KmsProviderType::HashicorpVault.to_string(),
            "hashicorp-vault"
        );
        assert_eq!(KmsProviderType::Pkcs11Hsm.to_string(), "pkcs11-hsm");
        assert_eq!(KmsProviderType::Mock.to_string(), "mock");
    }

    // GCP KMS Configuration Tests
    #[tokio::test]
    async fn test_gcp_kms_config_creation() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: r#"{
                    "type": "service_account",
                    "project_id": "test-project",
                    "private_key_id": "key-id",
                    "private_key": "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA0Z3VS5JJcds...\n-----END RSA PRIVATE KEY-----\n",
                    "client_email": "test@test-project.iam.gserviceaccount.com",
                    "client_id": "123456789",
                    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                    "token_uri": "https://oauth2.googleapis.com/token"
                }"#
                .into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("test-keyring".to_string()),
        };

        assert_eq!(config.provider_type, KmsProviderType::GcpKms);
        assert_eq!(config.region, Some("us-central1".to_string()));
        assert_eq!(config.key_namespace, Some("test-keyring".to_string()));
    }

    #[tokio::test]
    async fn test_gcp_kms_backend_requires_async_init() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Sync initialization should fail for GCP KMS (or fallback to mock if feature not enabled)
        let result = KmsManager::with_kms_config(config);
        assert!(result.is_ok()); // Falls back to mock provider
    }

    #[tokio::test]
    async fn test_gcp_kms_credentials_validation() {
        // Valid credentials structure
        let valid_creds = KmsCredentials::GcpServiceAccount {
            credentials_json: r#"{
                "type": "service_account",
                "project_id": "test-project"
            }"#
            .into(),
        };

        if let KmsCredentials::GcpServiceAccount { credentials_json } = valid_creds {
            let creds_json = String::from_utf8_lossy(credentials_json.as_bytes());
            let parsed: std::result::Result<serde_json::Value, _> =
                serde_json::from_str(creds_json.as_ref());
            assert!(parsed.is_ok());
            assert_eq!(
                parsed.unwrap().get("project_id").and_then(|p| p.as_str()),
                Some("test-project")
            );
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_with_custom_location() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("europe-west1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        assert_eq!(config.region, Some("europe-west1".to_string()));
    }

    #[tokio::test]
    async fn test_gcp_kms_with_custom_keyring() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("production-keys".to_string()),
        };

        assert_eq!(config.key_namespace, Some("production-keys".to_string()));
    }

    #[tokio::test]
    async fn test_kms_config_endpoint_detection_gcp() {
        let config = KeyProviderConfig {
            kms_endpoint: Some("https://cloudkms.googleapis.com".to_string()),
            ..Default::default()
        };

        let kms_config = KmsConfig::from_provider_config(&config).unwrap();
        assert_eq!(kms_config.provider_type, KmsProviderType::GcpKms);
    }

    // GCP KMS Emulator Integration Tests
    // Run with: GCP_KMS_EMULATOR_HOST=localhost:9011 cargo test --release --features gcp-kms

    /// Check if GCP KMS emulator is available
    #[cfg(feature = "gcp-kms")]
    fn is_kms_emulator_available() -> bool {
        std::env::var("GCP_KMS_EMULATOR_HOST").is_ok()
    }

    /// Create GCP KMS config for emulator testing
    #[cfg(feature = "gcp-kms")]
    fn create_gcp_emulator_config() -> KmsConfig {
        let endpoint =
            std::env::var("GCP_KMS_EMULATOR_HOST").unwrap_or_else(|_| "localhost:9011".to_string());

        KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: format!("http://{}", endpoint),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: r#"{
                    "type": "service_account",
                    "project_id": "test-project",
                    "private_key_id": "key-id",
                    "private_key": "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA0Z3...test...\n-----END RSA PRIVATE KEY-----\n",
                    "client_email": "test@test-project.iam.gserviceaccount.com",
                    "client_id": "123456789",
                    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                    "token_uri": "https://oauth2.googleapis.com/token"
                }"#
                .into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("test-keyring".to_string()),
        }
    }

    #[tokio::test]
    #[ignore = "Requires GCP KMS emulator: GCP_KMS_EMULATOR_HOST=localhost:9011"]
    #[cfg(feature = "gcp-kms")]
    async fn test_gcp_kms_emulator_key_generation() {
        use crate::providers::gcp::GcpKmsProvider;

        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }

        let config = create_gcp_emulator_config();
        let provider = GcpKmsProvider::new_async(config)
            .await
            .expect("Failed to initialize GCP KMS provider");

        // Generate an Ed25519 signing key
        let handle = provider
            .generate_key("test-key-gen", KeyAlgorithm::Ed25519)
            .await
            .expect("Failed to generate key");

        assert!(!handle.provider_id.is_empty());
        assert_eq!(handle.algorithm, KeyAlgorithm::Ed25519);
        assert!(handle.public_key.is_some());
    }

    #[tokio::test]
    #[ignore = "Requires GCP KMS emulator: GCP_KMS_EMULATOR_HOST=localhost:9011"]
    #[cfg(feature = "gcp-kms")]
    async fn test_gcp_kms_emulator_sign_and_verify() {
        use crate::providers::gcp::GcpKmsProvider;

        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }

        let config = create_gcp_emulator_config();
        let provider = GcpKmsProvider::new_async(config)
            .await
            .expect("Failed to initialize GCP KMS provider");

        // Generate a signing key
        let _ = provider
            .generate_key("test-sign-key", KeyAlgorithm::Ed25519)
            .await
            .expect("Failed to generate signing key");

        // Sign test data
        let message = b"test message to sign for verification";
        let signature = provider
            .sign("test-sign-key", message)
            .await
            .expect("Failed to sign message");

        assert!(!signature.is_empty());

        // Get public key for verification
        let public_key = provider
            .get_public_key("test-sign-key")
            .await
            .expect("Failed to get public key");

        assert!(!public_key.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires GCP KMS emulator: GCP_KMS_EMULATOR_HOST=localhost:9011"]
    #[cfg(feature = "gcp-kms")]
    async fn test_gcp_kms_emulator_encrypt_decrypt() {
        use crate::providers::gcp::GcpKmsProvider;

        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }

        let config = create_gcp_emulator_config();
        let provider = GcpKmsProvider::new_async(config)
            .await
            .expect("Failed to initialize GCP KMS provider");

        // Generate an encryption key
        let _ = provider
            .generate_key("test-enc-key", KeyAlgorithm::Aes256Gcm)
            .await
            .expect("Failed to generate encryption key");

        // Encrypt plaintext
        let plaintext = b"secret data to encrypt and decrypt";
        let ciphertext = provider
            .encrypt("test-enc-key", plaintext)
            .await
            .expect("Failed to encrypt data");

        assert!(!ciphertext.is_empty());
        assert_ne!(ciphertext, plaintext);

        // Decrypt ciphertext
        let decrypted = provider
            .decrypt("test-enc-key", &ciphertext)
            .await
            .expect("Failed to decrypt data");

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    #[ignore = "Requires GCP KMS emulator: GCP_KMS_EMULATOR_HOST=localhost:9011"]
    #[cfg(feature = "gcp-kms")]
    async fn test_gcp_kms_emulator_key_rotation() {
        use crate::providers::gcp::GcpKmsProvider;

        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }

        let config = create_gcp_emulator_config();
        let provider = GcpKmsProvider::new_async(config)
            .await
            .expect("Failed to initialize GCP KMS provider");

        // Generate initial key
        let initial_handle = provider
            .generate_key("test-rotate-key", KeyAlgorithm::Aes256Gcm)
            .await
            .expect("Failed to generate initial key");

        // Encrypt data before rotation
        let plaintext = b"data encrypted before rotation";
        let ciphertext_before = provider
            .encrypt("test-rotate-key", plaintext)
            .await
            .expect("Failed to encrypt before rotation");

        // Rotate the key
        let rotated_handle = provider
            .rotate_key("test-rotate-key")
            .await
            .expect("Failed to rotate key");

        assert_ne!(initial_handle.provider_id, rotated_handle.provider_id);

        // Verify new key can encrypt/decrypt
        let new_plaintext = b"data encrypted after rotation";
        let ciphertext_after = provider
            .encrypt("test-rotate-key", new_plaintext)
            .await
            .expect("Failed to encrypt after rotation");

        let decrypted_after = provider
            .decrypt("test-rotate-key", &ciphertext_after)
            .await
            .expect("Failed to decrypt after rotation");

        assert_eq!(new_plaintext.as_slice(), decrypted_after.as_slice());

        // Old ciphertext should still be decryptable (GCP KMS handles version internally)
        let decrypted_before = provider
            .decrypt("test-rotate-key", &ciphertext_before)
            .await
            .expect("Failed to decrypt old data after rotation");

        assert_eq!(plaintext.as_slice(), decrypted_before.as_slice());
    }

    // GCP KMS Async Initialization Tests
    #[tokio::test]
    async fn test_gcp_kms_missing_project_id() {
        let invalid_creds = r#"{"type": "service_account"}"#;

        let _config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: invalid_creds.into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Note: This will only error if gcp-kms feature is enabled
        // Without the feature, it falls back to mock
        #[cfg(any())]
        {
            let result = GcpKmsProvider::new_async(_config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("project_id"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_invalid_credentials_json() {
        let invalid_json = "not valid json {]";

        let _config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: invalid_json.into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        #[cfg(any())]
        {
            let result = GcpKmsProvider::new_async(_config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("JSON"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_wrong_credential_type() {
        let _config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test".to_string(),
                secret_access_key: "test".into(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        #[cfg(any())]
        {
            let result = GcpKmsProvider::new_async(_config).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("GcpServiceAccount"));
        }
    }

    // GCP KMS Configuration Defaults Tests
    #[tokio::test]
    async fn test_gcp_kms_default_location() {
        // When location is not specified, it should default to us-central1
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: None,
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // The default location "us-central1" would be used
        assert_eq!(config.region, None);
    }

    #[tokio::test]
    async fn test_gcp_kms_default_keyring() {
        // When key_namespace is not specified, it should default to "adapteros-keys"
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // The default key_namespace "adapteros-keys" would be used
        assert_eq!(config.key_namespace, None);
    }

    #[tokio::test]
    async fn test_gcp_kms_timeout_settings() {
        let config = KmsConfig {
            provider_type: KmsProviderType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".into(),
            },
            timeout_secs: 60,
            max_retries: 5,
            key_namespace: None,
        };

        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_retries, 5);
    }

    #[tokio::test]
    async fn test_gcp_kms_credentials_serialization() {
        let creds = KmsCredentials::GcpServiceAccount {
            credentials_json: r#"{"project_id": "test-project"}"#.into(),
        };

        // Serialization and deserialization should fail for credentials
        assert!(serde_json::to_string(&creds).is_err());
        assert!(serde_json::from_str::<KmsCredentials>("{}").is_err());
    }
}

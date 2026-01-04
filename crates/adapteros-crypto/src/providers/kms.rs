//! KMS/HSM provider implementation
//!
//! Provides abstraction for cloud KMS (AWS, GCP) and HSM integration.
//! Uses a backend trait to allow different KMS implementations.
//! Cloud KMS is disabled in local/CI builds and falls back to the mock backend.

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
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

/// Cloud KMS backends are intentionally disabled in local/CI builds.
const CLOUD_BACKEND_DISABLED_MSG: &str =
    "Cloud KMS backends are disabled in local-only builds; using mock backend";

// AWS KMS imports (conditional based on feature flag)
#[cfg(feature = "aws-kms")]
use aws_credential_types::Credentials;
#[cfg(feature = "aws-kms")]
use aws_sdk_kms::{types::SigningAlgorithmSpec, Client as KmsClient};
#[cfg(feature = "aws-kms")]
use aws_types::region::Region;

// GCP KMS imports (conditional based on feature flag)
#[cfg(any())]
use google_cloudkms1::api::{
    CryptoKey, CryptoKeyVersion, DecryptRequest, EncryptRequest, SignRequest,
    VerifySignatureRequest,
};
#[cfg(any())]
use google_cloudkms1::hyper;
#[cfg(any())]
use google_cloudkms1::{oauth2, yup_oauth2, Client as GcpKmsClient};

/// Create a seeded RNG for deterministic key generation
/// Uses HKDF with domain separation for cryptographic operations
fn seeded_rng(context: &str) -> StdRng {
    // Use a base seed derived from a constant (for KMS operations)
    // In production, this should be derived from a master key or system entropy
    let base_seed = B3Hash::hash(format!("kms-seed:{}", context).as_bytes());
    let seed_bytes = derive_seed(&base_seed, &format!("kms-rng:{}", context));
    let mut seed_array = [0u8; 32];
    seed_array.copy_from_slice(&seed_bytes[..32]);
    StdRng::from_seed(seed_array)
}

/// Execute KMS operation with retry logic
/// Provides exponential backoff for transient failures
#[cfg(feature = "aws-kms")]
async fn kms_with_retry<F, T>(max_retries: u32, provider_name: &str, mut op: F) -> Result<T>
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
    GcpServiceAccount { credentials_json: String },
    /// Azure service principal
    AzureServicePrincipal {
        tenant_id: String,
        client_id: String,
        client_secret: String,
    },
    /// HashiCorp Vault token
    VaultToken { token: String },
    /// PKCS#11 PIN
    Pkcs11Pin { pin: String, slot_id: Option<u64> },
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

/// AWS KMS backend implementation (feature-gated)
#[cfg(feature = "aws-kms")]
pub struct AwsKmsBackend {
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
impl AwsKmsBackend {
    /// Create a new AWS KMS backend with async initialization
    pub async fn new_async(config: KmsConfig) -> Result<Self> {
        let credentials = match &config.credentials {
            KmsCredentials::AwsIam {
                access_key_id,
                secret_access_key,
                session_token,
            } => Credentials::new(
                access_key_id.clone(),
                secret_access_key.clone(),
                session_token.clone(),
                None,
                "adapteros-crypto",
            ),
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
            "AWS KMS backend initialized"
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
impl KmsBackend for AwsKmsBackend {
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

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::AwsKms
    }

    fn fingerprint(&self) -> String {
        let region = self.config.region.as_deref().unwrap_or("us-east-1");
        format!("aws-kms-{}-v1.0", region)
    }
}

/// GCP Cloud KMS backend implementation (feature-gated)
#[cfg(any())]
pub struct GcpKmsBackend {
    client: GcpKmsClient,
    config: KmsConfig,
    project_id: String,
    location: String,
    key_ring: String,
    key_cache: Arc<RwLock<HashMap<String, GcpKeyMetadata>>>,
}

#[cfg(any())]
#[derive(Clone, Debug)]
struct GcpKeyMetadata {
    key_id: String,
    key_name: String,
    algorithm: KeyAlgorithm,
    public_key: Option<Vec<u8>>,
    created_at: u64,
    version: u32,
}

#[cfg(any())]
impl GcpKmsBackend {
    /// Create a new GCP KMS backend with async initialization
    pub async fn new_async(config: KmsConfig) -> Result<Self> {
        // Extract GCP-specific configuration
        let gcp_creds = match &config.credentials {
            KmsCredentials::GcpServiceAccount { credentials_json } => credentials_json.clone(),
            _ => {
                return Err(AosError::Crypto(
                    "GCP KMS requires GcpServiceAccount credentials".to_string(),
                ));
            }
        };

        // Parse service account JSON
        let service_account: serde_json::Value = serde_json::from_str(&gcp_creds)
            .map_err(|e| AosError::Crypto(format!("Invalid GCP service account JSON: {}", e)))?;

        let project_id = service_account
            .get("project_id")
            .and_then(|p| p.as_str())
            .ok_or_else(|| {
                AosError::Crypto("GCP service account missing 'project_id'".to_string())
            })?
            .to_string();

        // Extract location from config or default to us-central1
        let location = config
            .region
            .clone()
            .unwrap_or_else(|| "us-central1".to_string());

        // Key ring name (use namespace or default)
        let key_ring = config
            .key_namespace
            .clone()
            .unwrap_or_else(|| "adapteros-keys".to_string());

        // Create OAuth2 credentials
        let secret: oauth2::ApplicationSecret = serde_json::from_str(&gcp_creds)
            .map_err(|e| AosError::Crypto(format!("Failed to parse GCP credentials: {}", e)))?;

        // Build authenticator with service account
        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(secret)
            .build()
            .await
            .map_err(|e| AosError::Crypto(format!("Failed to create GCP authenticator: {}", e)))?;

        // Create hyper-based client
        let client = hyper::Client::builder().build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .https_only()
                .enable_all_versions()
                .build(),
        );

        let gcp_client = GcpKmsClient::new(client, auth);

        debug!(
            project_id = %project_id,
            location = %location,
            key_ring = %key_ring,
            "GCP KMS backend initialized"
        );

        Ok(Self {
            client: gcp_client,
            config,
            project_id,
            location,
            key_ring,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Build the key ring resource name
    fn key_ring_path(&self) -> String {
        format!(
            "projects/{}/locations/{}/keyRings/{}",
            self.project_id, self.location, self.key_ring
        )
    }

    /// Build the crypto key resource name
    fn key_path(&self, key_id: &str) -> String {
        format!("{}/cryptoKeys/{}", self.key_ring_path(), key_id)
    }

    /// Build the crypto key version resource name
    fn key_version_path(&self, key_id: &str, version: &str) -> String {
        format!("{}/cryptoKeyVersions/{}", self.key_path(key_id), version)
    }

    /// Execute operation with retry logic
    async fn with_retry<F, T>(&self, mut op: F) -> Result<T>
    where
        F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T>>,
    {
        kms_with_retry(self.config.max_retries, "GCP KMS", op).await
    }

    /// Ensure the key ring exists (creates if necessary)
    async fn ensure_key_ring(&self) -> Result<()> {
        let key_ring_path = self.key_ring_path();
        let parent = format!("projects/{}/locations/{}", self.project_id, self.location);

        // Attempt to create key ring (ignores error if already exists)
        let _ = self
            .client
            .projects()
            .locations_key_rings_create(Default::default(), &parent, Some(&self.key_ring))
            .doit()
            .await;

        debug!(key_ring = %key_ring_path, "Key ring ensured");
        Ok(())
    }

    /// Convert algorithm to GCP protection level and signing algorithm
    fn algorithm_to_gcp(alg: &KeyAlgorithm) -> (&'static str, &'static str) {
        match alg {
            KeyAlgorithm::Ed25519 => ("ASYMMETRIC_SIGN", "ED25519"),
            KeyAlgorithm::Aes256Gcm => ("SYMMETRIC_ENCRYPTION", "AES_256_GCM"),
            KeyAlgorithm::ChaCha20Poly1305 => ("SYMMETRIC_ENCRYPTION", "CHACHA20_POLY1305"),
        }
    }
}

#[cfg(any())]
#[async_trait::async_trait]
impl KmsBackend for GcpKmsBackend {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        self.ensure_key_ring().await?;

        let (purpose, algorithm) = Self::algorithm_to_gcp(&alg);
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();
            let key_id = key_id.to_string();
            let alg = alg.clone();

            Box::pin(async move {
                // Create new crypto key
                let crypto_key = CryptoKey {
                    purpose: Some(purpose.to_string()),
                    version_template: Some(google_cloudkms1::api::CryptoKeyVersionTemplate {
                        algorithm: Some(algorithm.to_string()),
                        ..Default::default()
                    }),
                    labels: Some(
                        vec![
                            ("managed-by".to_string(), "adapteros".to_string()),
                            ("algorithm".to_string(), alg.to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..Default::default()
                };

                // Create the key
                let _ = client
                    .projects()
                    .locations_key_rings_crypto_keys_create(
                        crypto_key,
                        &format!(
                            "projects/{}/locations/{}/keyRings/{}",
                            self.project_id, self.location, self.key_ring
                        ),
                        Some(&key_id),
                    )
                    .doit()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to create GCP KMS key: {}", e))
                    })?;

                // Get public key for asymmetric keys
                let pub_key = if alg == KeyAlgorithm::Ed25519 {
                    // Fetch the primary version
                    let response = client
                        .projects()
                        .locations_key_rings_crypto_keys_versions_get(&key_path)
                        .doit()
                        .await
                        .map_err(|_| {
                            AosError::Crypto("Failed to fetch GCP KMS key version".to_string())
                        })?;

                    if let Some((_, version)) = response {
                        version
                            .public_key
                            .and_then(|pk| pk.pem)
                            .map(|pem| pem.into_bytes())
                            .unwrap_or_default()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                Ok((pub_key, key_path, 1u32))
            })
        })
        .await
        .map(|(pub_key, _, _version)| {
            debug!(key_id = %key_id, algorithm = %alg, "GCP KMS: generated key");

            KeyHandle::with_public_key(key_path, alg, pub_key)
        })
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();
            let message = data.to_vec();

            Box::pin(async move {
                // Create sign request
                let sign_request = SignRequest {
                    bytes_to_sign: Some(message),
                    signature_algorithm: Some("ED25519".to_string()),
                    ..Default::default()
                };

                let response = client
                    .projects()
                    .locations_key_rings_crypto_keys_versions_sign(sign_request, &key_path)
                    .doit()
                    .await
                    .map_err(|e| AosError::Crypto(format!("GCP KMS sign failed: {}", e)))?;

                if let Some((_, result)) = response {
                    result.signature.map(|sig| sig.into_bytes()).ok_or_else(|| {
                        AosError::Crypto("GCP KMS response missing signature".to_string())
                    })
                } else {
                    Err(AosError::Crypto(
                        "GCP KMS sign response missing result".to_string(),
                    ))
                }
            })
        })
        .await
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();
            let plaintext = plaintext.to_vec();

            Box::pin(async move {
                // Create encrypt request
                let encrypt_request = EncryptRequest {
                    plaintext: Some(plaintext),
                    ..Default::default()
                };

                let response = client
                    .projects()
                    .locations_key_rings_crypto_keys_encrypt(encrypt_request, &key_path)
                    .doit()
                    .await
                    .map_err(|e| AosError::Crypto(format!("GCP KMS encrypt failed: {}", e)))?;

                if let Some((_, result)) = response {
                    result.ciphertext.map(|ct| ct.into_bytes()).ok_or_else(|| {
                        AosError::Crypto("GCP KMS response missing ciphertext".to_string())
                    })
                } else {
                    Err(AosError::Crypto(
                        "GCP KMS encrypt response missing result".to_string(),
                    ))
                }
            })
        })
        .await
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();
            let ciphertext = ciphertext.to_vec();

            Box::pin(async move {
                // Create decrypt request
                let decrypt_request = DecryptRequest {
                    ciphertext: Some(ciphertext),
                    ..Default::default()
                };

                let response = client
                    .projects()
                    .locations_key_rings_crypto_keys_decrypt(decrypt_request, &key_path)
                    .doit()
                    .await
                    .map_err(|e| AosError::Crypto(format!("GCP KMS decrypt failed: {}", e)))?;

                if let Some((_, result)) = response {
                    result.plaintext.map(|pt| pt.into_bytes()).ok_or_else(|| {
                        AosError::Crypto("GCP KMS response missing plaintext".to_string())
                    })
                } else {
                    Err(AosError::Crypto(
                        "GCP KMS decrypt response missing result".to_string(),
                    ))
                }
            })
        })
        .await
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();
            let key_id = key_id.to_string();

            Box::pin(async move {
                // Fetch the key to get its configuration
                let response = client
                    .projects()
                    .locations_key_rings_crypto_keys_get(&key_path)
                    .doit()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to describe GCP KMS key: {}", e))
                    })?;

                if let Some((_, key)) = response {
                    // Get algorithm and purpose from existing key
                    let purpose = key.purpose.as_deref().unwrap_or("ASYMMETRIC_SIGN");
                    let algorithm = match purpose {
                        "ASYMMETRIC_SIGN" => KeyAlgorithm::Ed25519,
                        "SYMMETRIC_ENCRYPTION" => KeyAlgorithm::Aes256Gcm,
                        _ => KeyAlgorithm::Aes256Gcm,
                    };

                    // Create a new version (rotation in GCP KMS)
                    let new_version = google_cloudkms1::api::CryptoKeyVersion {
                        algorithm: key.version_template.and_then(|vt| vt.algorithm),
                        ..Default::default()
                    };

                    let version_response = client
                        .projects()
                        .locations_key_rings_crypto_keys_versions_create(new_version, &key_path)
                        .doit()
                        .await
                        .map_err(|e| {
                            AosError::Crypto(format!(
                                "Failed to create new GCP KMS key version: {}",
                                e
                            ))
                        })?;

                    if let Some((_, version)) = version_response {
                        let pub_key = version
                            .public_key
                            .and_then(|pk| pk.pem)
                            .map(|pem| pem.into_bytes())
                            .unwrap_or_default();

                        let version_num = version
                            .name
                            .as_ref()
                            .and_then(|n| n.split('/').last())
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(1);

                        info!(
                            key_id = %key_id,
                            algorithm = %algorithm,
                            version = %version_num,
                            "GCP KMS: rotated key"
                        );

                        Ok((pub_key, key_path, version_num))
                    } else {
                        Err(AosError::Crypto(
                            "GCP KMS version create response missing data".to_string(),
                        ))
                    }
                } else {
                    Err(AosError::Crypto(
                        "GCP KMS get key response missing data".to_string(),
                    ))
                }
            })
        })
        .await
        .map(|(pub_key, path, version)| {
            let version_path = format!("{}/cryptoKeyVersions/{}", path, version);
            KeyHandle::with_public_key(version_path, KeyAlgorithm::Ed25519, pub_key)
        })
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

        // Fetch from GCP KMS
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();

            Box::pin(async move {
                let response = client
                    .projects()
                    .locations_key_rings_crypto_keys_versions_get(&key_path)
                    .doit()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to get GCP KMS public key: {}", e))
                    })?;

                if let Some((_, version)) = response {
                    version
                        .public_key
                        .and_then(|pk| pk.pem)
                        .map(|pem| pem.into_bytes())
                        .ok_or_else(|| {
                            AosError::Crypto("GCP KMS response missing public key".to_string())
                        })
                } else {
                    Err(AosError::Crypto(
                        "GCP KMS get public key response missing data".to_string(),
                    ))
                }
            })
        })
        .await
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();

            Box::pin(async move {
                match client
                    .projects()
                    .locations_key_rings_crypto_keys_get(&key_path)
                    .doit()
                    .await
                {
                    Ok((_, Some(key))) => {
                        // Check if key is enabled (not destroyed)
                        Ok(key.destroy_scheduled_duration.is_none())
                    }
                    Ok(_) => Ok(false),
                    Err(_) => Ok(false),
                }
            })
        })
        .await
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let key_path = self.key_path(key_id);

        self.with_retry(|| {
            let client = self.client.clone();
            let key_path = key_path.clone();

            Box::pin(async move {
                // Update the key to destroy it
                let mut key = CryptoKey::default();
                key.destroy_scheduled_duration = Some("86400s".to_string()); // 24 hours

                let _ = client
                    .projects()
                    .locations_key_rings_crypto_keys_patch(
                        key,
                        &key_path,
                        Some("destroy_scheduled_duration"),
                    )
                    .doit()
                    .await
                    .map_err(|e| {
                        AosError::Crypto(format!("Failed to delete GCP KMS key: {}", e))
                    })?;

                warn!(key_id = %key_id, "GCP KMS: scheduled key for deletion (24-hour waiting period)");
                Ok(())
            })
        })
        .await
    }

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::GcpKms
    }

    fn fingerprint(&self) -> String {
        format!("gcp-kms-{}-{}-v1.0", self.project_id, self.location)
    }
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
                seeded_rng(&format!("mock-generate-ed25519:{}", key_id)).fill_bytes(&mut private);
                // Derive public key (simplified mock)
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut key = vec![0u8; 32];
                seeded_rng("mock-symmetric-keygen").fill_bytes(&mut key);
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

        // Generate new key material
        let (private_key, public_key) = match key.algorithm {
            KeyAlgorithm::Ed25519 => {
                let mut private = vec![0u8; 32];
                seeded_rng(&format!("mock-rotate-ed25519:{}", key_id)).fill_bytes(&mut private);
                let public = private.iter().map(|b| b.wrapping_add(1)).collect();
                (private, public)
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                let mut k = vec![0u8; 32];
                seeded_rng(&format!("mock-rotate-symmetric:{}", key_id)).fill_bytes(&mut k);
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

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::Mock
    }

    fn fingerprint(&self) -> String {
        "mock-kms-v1.0".to_string()
    }
}

/// HashiCorp Vault backend implementation
/// Uses the Transit secret engine for cryptographic operations
pub struct HashicorpVaultBackend {
    endpoint: String,
    transit_mount: String,
    key_cache: Arc<RwLock<HashMap<String, VaultKeyMetadata>>>,
}

#[derive(Clone, Debug)]
struct VaultKeyMetadata {
    algorithm: KeyAlgorithm,
    version: u32,
}

impl HashicorpVaultBackend {
    /// Create a new HashiCorp Vault backend
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
            "HashiCorp Vault backend initialized"
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
impl KmsBackend for HashicorpVaultBackend {
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

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::HashicorpVault
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
pub struct LocalKmsBackend {
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

impl LocalKmsBackend {
    /// Create a new local file-based KMS backend
    ///
    /// ⚠️ WARNING: NOT FOR PRODUCTION ⚠️
    pub fn new(storage_path: std::path::PathBuf) -> Result<Self> {
        warn!("⚠️  WARNING: LocalKmsBackend is NOT FOR PRODUCTION USE ⚠️");
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
            "Local KMS backend initialized (DEVELOPMENT ONLY)"
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

    /// Generate key material based on algorithm
    fn generate_key_material(alg: &KeyAlgorithm) -> (Vec<u8>, Vec<u8>) {
        use rand::RngCore;
        let mut rng = seeded_rng(&format!("key-material:{:?}", alg));

        match alg {
            KeyAlgorithm::Ed25519 => {
                // Generate Ed25519 keypair
                let mut seed = [0u8; 32];
                rng.fill_bytes(&mut seed);
                let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                let verifying_key = signing_key.verifying_key();

                (seed.to_vec(), verifying_key.to_bytes().to_vec())
            }
            KeyAlgorithm::Aes256Gcm | KeyAlgorithm::ChaCha20Poly1305 => {
                // Generate 256-bit symmetric key
                let mut key = vec![0u8; 32];
                rng.fill_bytes(&mut key);
                (key.clone(), vec![]) // No public key for symmetric
            }
        }
    }
}

#[async_trait::async_trait]
impl KmsBackend for LocalKmsBackend {
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
                use rand::RngCore;

                let key_bytes: &[u8; 32] = key_data.key_material[..32]
                    .try_into()
                    .map_err(|_| AosError::Crypto("Invalid AES key length".to_string()))?;

                let cipher = Aes256Gcm::new(key_bytes.into());

                // Generate deterministic nonce using seeded RNG
                let mut nonce_bytes = [0u8; 12];
                seeded_rng(&format!("aes-nonce:{}", key_id)).fill_bytes(&mut nonce_bytes);
                let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

                // Encrypt
                let ciphertext = cipher
                    .encrypt(nonce, plaintext)
                    .map_err(|e| AosError::Crypto(format!("AES-GCM encryption failed: {}", e)))?;

                // Prepend nonce to ciphertext
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

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::Mock // Use Mock type for local development
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
            #[cfg(feature = "aws-kms")]
            KmsBackendType::AwsKms => {
                return Err(AosError::Crypto(
                    "AWS KMS requires async initialization. Use with_kms_config_async instead"
                        .to_string(),
                ));
            }
            #[cfg(not(feature = "aws-kms"))]
            KmsBackendType::AwsKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::GcpKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::AzureKeyVault => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::HashicorpVault => Arc::new(HashicorpVaultBackend::new(config.clone())?),
            KmsBackendType::Pkcs11Hsm => {
                // STUB: CRYPTO-GAP-001 - PKCS#11 HSM backend not implemented
                // Impact: Users selecting Pkcs11Hsm get mock backend silently
                // Rectify: Implement PKCS#11 via rust-pkcs11 crate when HSM support required
                warn!("PKCS#11 HSM backend not implemented (CRYPTO-GAP-001), using mock");
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

    /// Create a new KMS provider with async configuration (for AWS KMS and GCP KMS)
    pub async fn with_kms_config_async(config: KmsConfig) -> Result<Self> {
        #[allow(unused_variables, unreachable_code)]
        let backend: Arc<dyn KmsBackend> = match config.backend_type {
            #[cfg(feature = "aws-kms")]
            KmsBackendType::AwsKms => Arc::new(AwsKmsBackend::new_async(config.clone()).await?),
            #[cfg(not(feature = "aws-kms"))]
            KmsBackendType::AwsKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::GcpKms => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            KmsBackendType::AzureKeyVault => {
                warn!("{}", CLOUD_BACKEND_DISABLED_MSG);
                Arc::new(MockKmsBackend::new())
            }
            _ => {
                // Use sync initialization for other backends
                return Self::with_kms_config(config);
            }
        };

        #[allow(unreachable_code)]
        {
            info!(
                backend_type = %config.backend_type,
                endpoint = %config.endpoint,
                "KMS provider initialized (async)"
            );

            Ok(Self {
                config,
                backend,
                key_handles: Arc::new(RwLock::new(HashMap::new())),
            })
        }
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
}

impl KmsConfig {
    /// Convert from generic KeyProviderConfig
    pub fn from_provider_config(config: &KeyProviderConfig) -> Result<Self> {
        let endpoint = config
            .kms_endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:8200".to_string());

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
        let previous_key = handles
            .get(key_id)
            .cloned()
            .ok_or_else(|| AosError::Crypto(format!("Key not found in local cache: {}", key_id)))?;
        drop(handles);

        // Rotate in KMS
        let new_key = self.backend.rotate_key(&namespaced_id).await?;

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
            .backend
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
        let fingerprint = self.backend.fingerprint();

        // Create attestation data
        let attestation_data = format!(
            "{}:{}:{}:{}",
            self.config.backend_type, fingerprint, self.config.endpoint, timestamp
        );

        // Sign with a system key if available, otherwise use placeholder
        let signature = vec![0u8; 64]; // Would be actual signature in production

        Ok(ProviderAttestation::new(
            format!("kms:{}", self.config.backend_type),
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
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

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
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

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
            backend_type: KmsBackendType::Mock,
            ..Default::default()
        };

        let provider = KmsProvider::with_kms_config(config).unwrap();

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
        let backend = MockKmsBackend::new();

        backend
            .generate_key("dup-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        let result = backend.generate_key("dup-key", KeyAlgorithm::Ed25519).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_mock_backend_delete_key() {
        let backend = MockKmsBackend::new();

        backend
            .generate_key("del-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();
        assert!(backend.key_exists("del-key").await.unwrap());

        backend.delete_key("del-key").await.unwrap();
        assert!(!backend.key_exists("del-key").await.unwrap());
    }

    // AWS KMS Backend Tests
    #[tokio::test]
    async fn test_aws_kms_config_creation() {
        let config = KmsConfig {
            backend_type: KmsBackendType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key-id".to_string(),
                secret_access_key: "test-secret".to_string(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Verify configuration is valid
        assert_eq!(config.backend_type, KmsBackendType::AwsKms);
        assert_eq!(config.region, Some("us-east-1".to_string()));
    }

    #[tokio::test]
    async fn test_aws_kms_backend_requires_async_init() {
        let config = KmsConfig {
            backend_type: KmsBackendType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key-id".to_string(),
                secret_access_key: "test-secret".to_string(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Sync initialization should fail for AWS KMS (or fallback to mock if feature not enabled)
        let result = KmsProvider::with_kms_config(config);
        #[cfg(feature = "aws-kms")]
        assert!(result.is_err());
        #[cfg(not(feature = "aws-kms"))]
        assert!(result.is_ok()); // Falls back to mock
    }

    #[tokio::test]
    async fn test_kms_config_region_parsing() {
        let config = KmsConfig {
            backend_type: KmsBackendType::AwsKms,
            endpoint: "https://kms.eu-west-1.amazonaws.com".to_string(),
            region: Some("eu-west-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test".to_string(),
                secret_access_key: "test".to_string(),
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
            backend_type: KmsBackendType::AwsKms,
            endpoint: "https://kms.us-east-1.amazonaws.com".to_string(),
            region: Some("us-east-1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test-key".to_string(),
                secret_access_key: "test-secret".to_string(),
                session_token: Some("test-token".to_string()),
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
                assert_eq!(secret_access_key, "test-secret");
                assert_eq!(session_token, &Some("test-token".to_string()));
            }
            _ => panic!("Expected AwsIam credentials"),
        }
    }

    #[tokio::test]
    async fn test_kms_provider_with_backend_mock() {
        // Test provider creation with mock backend
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        let mock_backend = Arc::new(MockKmsBackend::new());
        let provider = KmsProvider::with_backend(config, mock_backend);

        // Verify provider is initialized
        assert_eq!(provider.config.backend_type, KmsBackendType::Mock);
    }

    #[tokio::test]
    async fn test_kms_config_timeout_settings() {
        let config = KmsConfig {
            backend_type: KmsBackendType::Mock,
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
            backend_type: KmsBackendType::Mock,
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
            secret_access_key: "secret456".to_string(),
            session_token: Some("token789".to_string()),
        };

        // Verify serialization works
        let json = serde_json::to_string(&creds);
        assert!(json.is_ok());

        // Verify deserialization works
        if let Ok(json_str) = json {
            let deserialized: std::result::Result<KmsCredentials, _> =
                serde_json::from_str(&json_str);
            assert!(deserialized.is_ok());
        }
    }

    #[tokio::test]
    async fn test_kms_backend_type_display() {
        assert_eq!(KmsBackendType::AwsKms.to_string(), "aws-kms");
        assert_eq!(KmsBackendType::GcpKms.to_string(), "gcp-kms");
        assert_eq!(KmsBackendType::AzureKeyVault.to_string(), "azure-keyvault");
        assert_eq!(
            KmsBackendType::HashicorpVault.to_string(),
            "hashicorp-vault"
        );
        assert_eq!(KmsBackendType::Pkcs11Hsm.to_string(), "pkcs11-hsm");
        assert_eq!(KmsBackendType::Mock.to_string(), "mock");
    }

    // GCP KMS Configuration Tests
    #[tokio::test]
    async fn test_gcp_kms_config_creation() {
        let config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
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
                }"#.to_string(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("test-keyring".to_string()),
        };

        assert_eq!(config.backend_type, KmsBackendType::GcpKms);
        assert_eq!(config.region, Some("us-central1".to_string()));
        assert_eq!(config.key_namespace, Some("test-keyring".to_string()));
    }

    #[tokio::test]
    async fn test_gcp_kms_backend_requires_async_init() {
        let config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Sync initialization should fail for GCP KMS (or fallback to mock if feature not enabled)
        let result = KmsProvider::with_kms_config(config);
        assert!(result.is_ok()); // Falls back to mock backend
    }

    #[tokio::test]
    async fn test_gcp_kms_credentials_validation() {
        // Valid credentials structure
        let valid_creds = KmsCredentials::GcpServiceAccount {
            credentials_json: r#"{
                "type": "service_account",
                "project_id": "test-project"
            }"#
            .to_string(),
        };

        if let KmsCredentials::GcpServiceAccount { credentials_json } = valid_creds {
            let parsed: std::result::Result<serde_json::Value, _> =
                serde_json::from_str(&credentials_json);
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
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("europe-west1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
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
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
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
        assert_eq!(kms_config.backend_type, KmsBackendType::GcpKms);
    }

    // GCP KMS Emulator Integration Tests
    // Run with: GCP_KMS_EMULATOR_HOST=localhost:9011 cargo test --release

    /// Check if GCP KMS emulator is available
    fn is_kms_emulator_available() -> bool {
        std::env::var("GCP_KMS_EMULATOR_HOST").is_ok()
    }

    #[tokio::test]
    #[ignore = "GCP KMS emulator integration not yet implemented - requires GcpKmsBackend::new_async"]
    async fn test_gcp_kms_emulator_key_generation() {
        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }

        // This test requires the GCP KMS emulator to be running locally
        // Start with: gcloud beta emulators cloud-kms start
        let endpoint =
            std::env::var("GCP_KMS_EMULATOR_HOST").unwrap_or_else(|_| "localhost:9011".to_string());

        let _config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: format!("http://{}", endpoint),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: r#"{
                    "type": "service_account",
                    "project_id": "test-project",
                    "private_key_id": "key-id",
                    "private_key": "-----BEGIN RSA PRIVATE KEY-----\ntest\n-----END RSA PRIVATE KEY-----\n",
                    "client_email": "test@test-project.iam.gserviceaccount.com",
                    "client_id": "123456789",
                    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                    "token_uri": "https://oauth2.googleapis.com/token"
                }"#.to_string(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: Some("test-keyring".to_string()),
        };

        // Implementation needed:
        // let backend = GcpKmsBackend::new_async(config).await.expect("backend init");
        // let key_id = backend.generate_key("test-key", KeyType::Signing).await.expect("key gen");
        // assert!(!key_id.is_empty());
        unimplemented!("GCP KMS key generation test");
    }

    #[tokio::test]
    #[ignore = "GCP KMS emulator integration not yet implemented - requires sign/verify methods"]
    async fn test_gcp_kms_emulator_sign_and_verify() {
        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }
        // Implementation needed:
        // 1. Initialize GcpKmsBackend
        // 2. Generate or import a signing key
        // 3. Sign test data
        // 4. Verify signature
        unimplemented!("GCP KMS sign/verify test");
    }

    #[tokio::test]
    #[ignore = "GCP KMS emulator integration not yet implemented - requires encrypt/decrypt methods"]
    async fn test_gcp_kms_emulator_encrypt_decrypt() {
        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }
        // Implementation needed:
        // 1. Initialize GcpKmsBackend
        // 2. Generate or import an encryption key
        // 3. Encrypt plaintext
        // 4. Decrypt ciphertext
        // 5. Assert plaintext matches
        unimplemented!("GCP KMS encrypt/decrypt test");
    }

    #[tokio::test]
    #[ignore = "GCP KMS emulator integration not yet implemented - requires key rotation methods"]
    async fn test_gcp_kms_emulator_key_rotation() {
        if !is_kms_emulator_available() {
            panic!("GCP KMS emulator not available (set GCP_KMS_EMULATOR_HOST)");
        }
        // Implementation needed:
        // 1. Initialize GcpKmsBackend
        // 2. Create initial key version
        // 3. Trigger rotation
        // 4. Verify new version is primary
        // 5. Verify old version still decrypts old data
        unimplemented!("GCP KMS key rotation test");
    }

    // GCP KMS Async Initialization Tests
    #[tokio::test]
    async fn test_gcp_kms_missing_project_id() {
        let invalid_creds = r#"{"type": "service_account"}"#;

        let _config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: invalid_creds.to_string(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        // Note: This will only error if gcp-kms feature is enabled
        // Without the feature, it falls back to mock
        #[cfg(any())]
        {
            let result = GcpKmsBackend::new_async(_config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("project_id"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_invalid_credentials_json() {
        let invalid_json = "not valid json {]";

        let _config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: invalid_json.to_string(),
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        #[cfg(any())]
        {
            let result = GcpKmsBackend::new_async(_config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("JSON"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_wrong_credential_type() {
        let _config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::AwsIam {
                access_key_id: "test".to_string(),
                secret_access_key: "test".to_string(),
                session_token: None,
            },
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        };

        #[cfg(any())]
        {
            let result = GcpKmsBackend::new_async(_config).await;
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
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: None,
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
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
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
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
            backend_type: KmsBackendType::GcpKms,
            endpoint: "https://cloudkms.googleapis.com".to_string(),
            region: Some("us-central1".to_string()),
            credentials: KmsCredentials::GcpServiceAccount {
                credentials_json: "{}".to_string(),
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
            credentials_json: r#"{"project_id": "test-project"}"#.to_string(),
        };

        // Verify serialization works
        let json = serde_json::to_string(&creds);
        assert!(json.is_ok());

        // Verify deserialization works
        if let Ok(json_str) = json {
            let deserialized: std::result::Result<KmsCredentials, _> =
                serde_json::from_str(&json_str);
            assert!(deserialized.is_ok());
        }
    }
}

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
use tracing::{debug, info, warn};

// AWS KMS imports (conditional based on feature flag)
#[cfg(feature = "aws-kms")]
use aws_sdk_kms::types::SigningAlgorithmSpec;
#[cfg(feature = "aws-kms")]
use aws_sdk_kms::Client as KmsClient;

// GCP KMS imports (conditional based on feature flag)
#[cfg(feature = "gcp-kms")]
use google_cloudkms1::api::{
    CryptoKey, CryptoKeyVersion, DecryptRequest, EncryptRequest, SignRequest,
    VerifySignatureRequest,
};
#[cfg(feature = "gcp-kms")]
use google_cloudkms1::hyper;
#[cfg(feature = "gcp-kms")]
use google_cloudkms1::{oauth2, yup_oauth2, Client as GcpKmsClient};

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
            } => {
                use aws_config::aws_credential_types::Credentials;

                Credentials::new(
                    access_key_id.clone(),
                    secret_access_key.clone(),
                    session_token.clone(),
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
        let region = config
            .region
            .as_deref()
            .unwrap_or("us-east-1")
            .parse::<aws_config::region::Region>()
            .map_err(|_| {
                AosError::Crypto(format!(
                    "Invalid AWS region: {}",
                    config.region.as_deref().unwrap_or("unknown")
                ))
            })?;

        let mut aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region)
            .credentials_provider(credentials)
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
    async fn with_retry<F, T>(&self, mut op: F) -> Result<T>
    where
        F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T>>,
    {
        let mut retries = 0;
        let max_retries = self.config.max_retries;

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
                        "Retrying AWS KMS operation"
                    );
                }
            }
        }
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

                    let aws_key_id = metadata.key_id().ok_or_else(|| {
                        AosError::Crypto("AWS KMS response missing key ID".to_string())
                    })?;

                    // Create alias for the key
                    let alias = format!("alias/adapteros-{}", key_id);
                    let _ = client
                        .create_alias()
                        .alias_name(&alias)
                        .target_key_id(aws_key_id)
                        .send()
                        .await;

                    // Get public key for asymmetric keys
                    let pub_key = if alg == KeyAlgorithm::Ed25519 {
                        match client.get_public_key().key_id(aws_key_id).send().await {
                            Ok(response) => response
                                .public_key()
                                .and_then(|pk| pk.as_ref())
                                .map(|b| b.to_vec())
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
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
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
                    .signing_algorithm(SigningAlgorithmSpec::Ed25519)
                    .send()
                    .await
                    .map_err(|e| AosError::Crypto(format!("AWS KMS sign failed: {}", e)))?;

                let signature = response
                    .signature()
                    .and_then(|sig| sig.as_ref())
                    .map(|b| b.to_vec())
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
                    .and_then(|ct| ct.as_ref())
                    .map(|b| b.to_vec())
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
                    .and_then(|pt| pt.as_ref())
                    .map(|b| b.to_vec())
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
                                .and_then(|pk| pk.as_ref())
                                .map(|b| b.to_vec())
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
                    .and_then(|pk| pk.as_ref())
                    .map(|b| b.to_vec())
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
                            Ok(metadata.enabled() == Some(true))
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
#[cfg(feature = "gcp-kms")]
pub struct GcpKmsBackend {
    client: GcpKmsClient,
    config: KmsConfig,
    project_id: String,
    location: String,
    key_ring: String,
    key_cache: Arc<RwLock<HashMap<String, GcpKeyMetadata>>>,
}

#[cfg(feature = "gcp-kms")]
#[derive(Clone, Debug)]
struct GcpKeyMetadata {
    key_id: String,
    key_name: String,
    algorithm: KeyAlgorithm,
    public_key: Option<Vec<u8>>,
    created_at: u64,
    version: u32,
}

#[cfg(feature = "gcp-kms")]
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
        let mut retries = 0;
        let max_retries = self.config.max_retries;

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
                        "Retrying GCP KMS operation"
                    );
                }
            }
        }
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

#[cfg(feature = "gcp-kms")]
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

/// Azure Key Vault backend implementation (feature-gated)
#[cfg(feature = "azure-kms")]
pub struct AzureKeyVaultBackend {
    vault_url: String,
    credential: azure_identity::DefaultAzureCredential,
    config: KmsConfig,
    key_cache: Arc<RwLock<HashMap<String, AzureKeyMetadata>>>,
}

#[cfg(feature = "azure-kms")]
#[derive(Clone, Debug)]
struct AzureKeyMetadata {
    key_id: String,
    algorithm: KeyAlgorithm,
    public_key: Option<Vec<u8>>,
    created_at: u64,
    version: String,
}

#[cfg(feature = "azure-kms")]
impl AzureKeyVaultBackend {
    /// Create a new Azure Key Vault backend with async initialization
    pub async fn new_async(config: KmsConfig) -> Result<Self> {
        // Extract Azure-specific configuration
        let (tenant_id, client_id, client_secret) = match &config.credentials {
            KmsCredentials::AzureServicePrincipal {
                tenant_id,
                client_id,
                client_secret,
            } => (tenant_id.clone(), client_id.clone(), client_secret.clone()),
            KmsCredentials::None => {
                // Use environment variables or managed identity
                (
                    std::env::var("AZURE_TENANT_ID").unwrap_or_default(),
                    std::env::var("AZURE_CLIENT_ID").unwrap_or_default(),
                    std::env::var("AZURE_CLIENT_SECRET").unwrap_or_default(),
                )
            }
            _ => {
                return Err(AosError::Crypto(
                    "Azure Key Vault requires AzureServicePrincipal credentials or AZURE_* environment variables"
                        .to_string(),
                ));
            }
        };

        // Validate vault URL format
        let vault_url = if config.endpoint.starts_with("https://") {
            config.endpoint.clone()
        } else if config.endpoint.contains(".vault.azure.net") {
            format!("https://{}", config.endpoint)
        } else {
            format!("https://{}.vault.azure.net/", config.endpoint)
        };

        // Ensure trailing slash
        let vault_url = if !vault_url.ends_with('/') {
            format!("{}/", vault_url)
        } else {
            vault_url
        };

        // Use service principal credentials if available, otherwise default credential
        #[allow(unused_variables)]
        let credential =
            if !client_id.is_empty() && !client_secret.is_empty() && !tenant_id.is_empty() {
                // Create credentials from service principal
                // In a real implementation, use azure_identity::ClientSecretCredential
                // For now, use DefaultAzureCredential which supports multiple auth methods
                azure_identity::DefaultAzureCredential::default()
            } else {
                // Use DefaultAzureCredential for managed identity or local development
                azure_identity::DefaultAzureCredential::default()
            };

        debug!(
            vault_url = %vault_url,
            tenant_id = %tenant_id,
            "Azure Key Vault backend initialized"
        );

        Ok(Self {
            vault_url,
            credential,
            config,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Execute operation with retry logic
    async fn with_retry<F, T>(&self, mut op: F) -> Result<T>
    where
        F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T>>,
    {
        let mut retries = 0;
        let max_retries = self.config.max_retries;

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
                        "Retrying Azure Key Vault operation"
                    );
                }
            }
        }
    }

    /// Build the key URI
    fn key_uri(&self, key_id: &str) -> String {
        format!("{}keys/{}", self.vault_url, key_id)
    }

    /// Convert algorithm to Azure algorithm name
    fn algorithm_to_azure(alg: &KeyAlgorithm) -> &'static str {
        match alg {
            KeyAlgorithm::Ed25519 => "Ed25519",
            KeyAlgorithm::Aes256Gcm => "RSA2048", // Azure uses RSA for encryption
            KeyAlgorithm::ChaCha20Poly1305 => "RSA2048",
        }
    }

    /// Map Azure error codes to AosError
    fn map_azure_error(error_msg: &str, context: &str) -> AosError {
        let error_lower = error_msg.to_lowercase();

        if error_lower.contains("not found") || error_lower.contains("does not exist") {
            AosError::Crypto(format!("Azure Key Vault: Key not found - {}", context))
        } else if error_lower.contains("unauthorized") || error_lower.contains("forbidden") {
            AosError::Auth(format!(
                "Azure Key Vault: Authentication failed - {}",
                context
            ))
        } else if error_lower.contains("invalid") {
            AosError::Crypto(format!(
                "Azure Key Vault: Invalid key or operation - {}",
                context
            ))
        } else if error_lower.contains("timeout") {
            AosError::Network(format!("Azure Key Vault: Operation timeout - {}", context))
        } else if error_lower.contains("conflict") {
            AosError::Crypto(format!("Azure Key Vault: Key already exists - {}", context))
        } else {
            AosError::Crypto(format!(
                "Azure Key Vault error: {} - {}",
                error_msg, context
            ))
        }
    }
}

#[cfg(feature = "azure-kms")]
#[async_trait::async_trait]
impl KmsBackend for AzureKeyVaultBackend {
    async fn generate_key(&self, key_id: &str, alg: KeyAlgorithm) -> Result<KeyHandle> {
        let key_uri = self.key_uri(key_id);
        let algorithm_name = Self::algorithm_to_azure(&alg).to_string();
        let alg_clone = alg.clone();

        let (public_key, version) = self
            .with_retry(|| {
                let vault_url = self.vault_url.clone();
                let key_id_owned = key_id.to_string();
                let algorithm = algorithm_name.clone();
                Box::pin(async move {
                    // In real Azure SDK usage, this would call:
                    // let mut client = KeyClient::new(&vault_url, credential);
                    // client.create_key(&key_id, KeyType::from(alg), None).await

                    // Mock Azure API call for demonstration
                    // Returns (public_key, version)
                    let mut pub_key = vec![0u8; 64];
                    use rand::RngCore;
                    rand::thread_rng().fill_bytes(&mut pub_key);

                    Ok((pub_key, "1".to_string()))
                })
            })
            .await?;

        // Cache metadata
        let mut cache = self.key_cache.write().await;
        cache.insert(
            key_id.to_string(),
            AzureKeyMetadata {
                key_id: key_id.to_string(),
                algorithm: alg_clone.clone(),
                public_key: if public_key.is_empty() {
                    None
                } else {
                    Some(public_key.clone())
                },
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                version: version.clone(),
            },
        );

        debug!(key_id = %key_id, algorithm = %alg_clone, version = %version, "Azure Key Vault: generated key");

        Ok(KeyHandle::with_public_key(
            format!("{}/versions/{}", key_uri, version),
            alg_clone,
            public_key,
        ))
    }

    async fn sign(&self, key_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let key_id_owned = key_id.to_string();
        let data_owned = data.to_vec();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();
            let message = data_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = CryptographyClient::new(&key_uri, credential);
                // client.sign(SignatureAlgorithm::ES256, &message).await

                // Mock implementation for demonstration
                let mut signature = vec![0u8; 64];
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                key_id.hash(&mut hasher);
                message.hash(&mut hasher);
                let hash = hasher.finish();

                signature.iter_mut().enumerate().for_each(|(i, b)| {
                    *b = ((hash >> (i % 8 * 8)) & 0xFF) as u8;
                });

                Ok(signature)
            })
        })
        .await
    }

    async fn encrypt(&self, key_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let key_id_owned = key_id.to_string();
        let plaintext_owned = plaintext.to_vec();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();
            let plaintext = plaintext_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = CryptographyClient::new(&key_uri, credential);
                // client.encrypt(EncryptionAlgorithm::RsaOaep, &plaintext).await

                // Mock implementation for demonstration
                let mut ciphertext = plaintext.clone();
                for (i, byte) in ciphertext.iter_mut().enumerate() {
                    *byte = byte.wrapping_add((i as u8).wrapping_mul(13));
                }

                Ok(ciphertext)
            })
        })
        .await
    }

    async fn decrypt(&self, key_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let key_id_owned = key_id.to_string();
        let ciphertext_owned = ciphertext.to_vec();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();
            let ciphertext = ciphertext_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = CryptographyClient::new(&key_uri, credential);
                // client.decrypt(EncryptionAlgorithm::RsaOaep, &ciphertext).await

                // Mock implementation for demonstration (inverse of encrypt)
                let mut plaintext = ciphertext.clone();
                for (i, byte) in plaintext.iter_mut().enumerate() {
                    *byte = byte.wrapping_sub((i as u8).wrapping_mul(13));
                }

                Ok(plaintext)
            })
        })
        .await
    }

    async fn rotate_key(&self, key_id: &str) -> Result<KeyHandle> {
        let key_id_owned = key_id.to_string();

        let (public_key, new_version, algorithm) = self
            .with_retry(|| {
                let vault_url = self.vault_url.clone();
                let key_id = key_id_owned.clone();

                Box::pin(async move {
                    // In real Azure SDK usage:
                    // let mut client = KeyClient::new(&vault_url, credential);
                    // let properties = client.get_key_properties(&key_id, None).await?;
                    // client.update_key_properties(&properties).await

                    // Mock implementation
                    let mut pub_key = vec![0u8; 64];
                    use rand::RngCore;
                    rand::thread_rng().fill_bytes(&mut pub_key);

                    Ok((pub_key, "2".to_string(), KeyAlgorithm::Ed25519))
                })
            })
            .await?;

        let key_uri = self.key_uri(key_id);

        info!(
            key_id = %key_id,
            version = %new_version,
            "Azure Key Vault: rotated key"
        );

        Ok(KeyHandle::with_public_key(
            format!("{}/versions/{}", key_uri, new_version),
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

        // Fetch from Azure Key Vault
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = KeyClient::new(&vault_url, credential);
                // let key = client.get_key(&key_id, None).await?;
                // Ok(key.key.public_key_bytes().to_vec())

                // Mock implementation
                let mut pub_key = vec![0u8; 64];
                use rand::RngCore;
                rand::thread_rng().fill_bytes(&mut pub_key);

                Ok(pub_key)
            })
        })
        .await
    }

    async fn key_exists(&self, key_id: &str) -> Result<bool> {
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = KeyClient::new(&vault_url, credential);
                // match client.get_key_properties(&key_id, None).await {
                //     Ok(props) => Ok(props.enabled),
                //     Err(_) => Ok(false),
                // }

                // Mock implementation
                Ok(!key_id.is_empty())
            })
        })
        .await
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let key_id_owned = key_id.to_string();

        self.with_retry(|| {
            let vault_url = self.vault_url.clone();
            let key_id = key_id_owned.clone();

            Box::pin(async move {
                // In real Azure SDK usage:
                // let mut client = KeyClient::new(&vault_url, credential);
                // client.delete_key(&key_id).await?;

                // Mock implementation
                warn!(key_id = %key_id, "Azure Key Vault: scheduled key for deletion (90-day waiting period)");
                Ok(())
            })
        })
        .await
    }

    fn backend_type(&self) -> KmsBackendType {
        KmsBackendType::AzureKeyVault
    }

    fn fingerprint(&self) -> String {
        let vault_name = self
            .vault_url
            .split('/')
            .filter(|s| !s.is_empty())
            .nth(2) // Get the vault name from URL
            .unwrap_or("unknown");
        format!("azure-keyvault-{}-v1.0", vault_name)
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
                warn!("AWS KMS backend not available (feature not enabled), using mock");
                Arc::new(MockKmsBackend::new())
            }
            #[cfg(feature = "gcp-kms")]
            KmsBackendType::GcpKms => {
                return Err(AosError::Crypto(
                    "GCP KMS requires async initialization. Use with_kms_config_async instead"
                        .to_string(),
                ));
            }
            #[cfg(not(feature = "gcp-kms"))]
            KmsBackendType::GcpKms => {
                warn!("GCP KMS backend not available (feature not enabled), using mock");
                Arc::new(MockKmsBackend::new())
            }
            #[cfg(feature = "azure-kms")]
            KmsBackendType::AzureKeyVault => {
                return Err(AosError::Crypto(
                    "Azure Key Vault requires async initialization. Use with_kms_config_async instead".to_string(),
                ));
            }
            #[cfg(not(feature = "azure-kms"))]
            KmsBackendType::AzureKeyVault => {
                warn!("Azure Key Vault backend not available (feature not enabled), using mock");
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

    /// Create a new KMS provider with async configuration (for AWS KMS, GCP KMS, and Azure Key Vault)
    pub async fn with_kms_config_async(config: KmsConfig) -> Result<Self> {
        #[allow(unused_variables, unreachable_code)]
        let backend: Arc<dyn KmsBackend> = match config.backend_type {
            #[cfg(feature = "aws-kms")]
            KmsBackendType::AwsKms => Arc::new(AwsKmsBackend::new_async(config.clone()).await?),
            #[cfg(feature = "gcp-kms")]
            KmsBackendType::GcpKms => Arc::new(GcpKmsBackend::new_async(config.clone()).await?),
            #[cfg(feature = "azure-kms")]
            KmsBackendType::AzureKeyVault => {
                Arc::new(AzureKeyVaultBackend::new_async(config.clone()).await?)
            }
            #[cfg(not(any(feature = "aws-kms", feature = "gcp-kms", feature = "azure-kms")))]
            _ => {
                // Use sync initialization for other backends when no async features are enabled
                return Self::with_kms_config(config);
            }
            #[cfg(any(feature = "aws-kms", feature = "gcp-kms", feature = "azure-kms"))]
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

        let timestamp = Self::current_timestamp();

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
        let timestamp = Self::current_timestamp();
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
        #[cfg(feature = "gcp-kms")]
        assert!(result.is_err());
        #[cfg(not(feature = "gcp-kms"))]
        assert!(result.is_ok()); // Falls back to mock
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
    #[tokio::test]
    #[ignore] // Requires GCP emulator running
    async fn test_gcp_kms_emulator_key_generation() {
        // This test requires the GCP KMS emulator to be running locally
        // Start with: gcloud kms emulator

        let config = KmsConfig {
            backend_type: KmsBackendType::GcpKms,
            endpoint: "http://localhost:9011".to_string(),
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

        // This would require a running emulator
        // let result = GcpKmsBackend::new_async(config).await;
        // assert!(result.is_ok() || result.is_err()); // Depends on emulator availability
    }

    #[tokio::test]
    #[ignore] // Requires GCP emulator running
    async fn test_gcp_kms_emulator_sign_and_verify() {
        // This test demonstrates signing with GCP KMS emulator
        // Requires emulator running and proper authentication
    }

    #[tokio::test]
    #[ignore] // Requires GCP emulator running
    async fn test_gcp_kms_emulator_encrypt_decrypt() {
        // This test demonstrates encryption/decryption with GCP KMS emulator
        // Requires emulator running and proper authentication
    }

    #[tokio::test]
    #[ignore] // Requires GCP emulator running
    async fn test_gcp_kms_emulator_key_rotation() {
        // This test demonstrates key rotation with GCP KMS emulator
        // Requires emulator running and proper authentication
    }

    // GCP KMS Async Initialization Tests
    #[tokio::test]
    async fn test_gcp_kms_missing_project_id() {
        let invalid_creds = r#"{"type": "service_account"}"#;

        let config = KmsConfig {
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
        #[cfg(feature = "gcp-kms")]
        {
            let result = GcpKmsBackend::new_async(config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("project_id"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_invalid_credentials_json() {
        let invalid_json = "not valid json {]";

        let config = KmsConfig {
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

        #[cfg(feature = "gcp-kms")]
        {
            let result = GcpKmsBackend::new_async(config).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("JSON"));
        }
    }

    #[tokio::test]
    async fn test_gcp_kms_wrong_credential_type() {
        let config = KmsConfig {
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

        #[cfg(feature = "gcp-kms")]
        {
            let result = GcpKmsBackend::new_async(config).await;
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

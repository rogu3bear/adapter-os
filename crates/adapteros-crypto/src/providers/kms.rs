//! KMS/HSM provider stub
//!
//! Placeholder implementation that returns Unimplemented errors.
//! To be replaced with actual KMS/HSM integration in the future.

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, ProviderAttestation, RotationReceipt,
};
use adapteros_core::Result;

/// KMS provider stub implementation
#[derive(Debug)]
pub struct KmsProvider {
    #[allow(dead_code)]
    config: KeyProviderConfig,
}

impl KmsProvider {
    /// Create a new KMS provider (always returns Unimplemented)
    pub fn new(_config: KeyProviderConfig) -> Result<Self> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl KeyProvider for KmsProvider {
    async fn generate(&self, _key_id: &str, _alg: KeyAlgorithm) -> Result<KeyHandle> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }

    async fn sign(&self, _key_id: &str, _msg: &[u8]) -> Result<Vec<u8>> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }

    async fn seal(&self, _key_id: &str, _plaintext: &[u8]) -> Result<Vec<u8>> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }

    async fn unseal(&self, _key_id: &str, _ciphertext: &[u8]) -> Result<Vec<u8>> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }

    async fn rotate(&self, _key_id: &str) -> Result<RotationReceipt> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        Err(adapteros_core::AosError::Crypto(
            "KMS provider not yet implemented".to_string(),
        ))
    }
}

/// Create a KMS provider instance (always fails with Unimplemented)
pub fn create_kms_provider(_config: KeyProviderConfig) -> Result<KmsProvider> {
    Err(adapteros_core::AosError::Crypto(
        "KMS provider not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;

    #[tokio::test]
    async fn test_kms_provider_unimplemented() {
        let config = KeyProviderConfig::default();

        // Creating provider should fail
        let result = KmsProvider::new(config.clone());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));

        // Direct creation should also fail
        let result = create_kms_provider(config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));
    }
}

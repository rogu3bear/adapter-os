use super::{EnclaveError, Result};

/// Placeholder implementation for platforms without Secure Enclave support.
#[derive(Debug)]
pub struct EnclaveManager;

impl EnclaveManager {
    pub fn new() -> Result<Self> {
        Err(EnclaveError::OperationFailed(
            "Secure Enclave not available on this platform".to_string(),
        ))
    }

    pub fn sign_bundle(&mut self, _bundle_hash: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(
            "Secure Enclave signing is not supported on this platform".to_string(),
        ))
    }

    pub fn seal_lora_delta(&mut self, _delta: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(
            "Secure Enclave encryption is not supported on this platform".to_string(),
        ))
    }

    pub fn unseal_lora_delta(&mut self, _sealed_delta: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(
            "Secure Enclave encryption is not supported on this platform".to_string(),
        ))
    }

    pub fn get_public_key(&mut self, _label: &str) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(
            "Secure Enclave is not supported on this platform".to_string(),
        ))
    }

    pub fn seal_with_label(&mut self, label: &str, _data: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(format!(
            "Secure Enclave encryption is not supported on this platform (label: {})",
            label
        )))
    }

    pub fn unseal_with_label(&mut self, label: &str, _sealed: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(format!(
            "Secure Enclave encryption is not supported on this platform (label: {})",
            label
        )))
    }

    pub fn sign_with_label(&mut self, label: &str, _data: &[u8]) -> Result<Vec<u8>> {
        Err(EnclaveError::OperationFailed(format!(
            "Secure Enclave signing is not supported on this platform (label: {})",
            label
        )))
    }
}

impl Default for EnclaveManager {
    fn default() -> Self {
        // Return a valid stub instance. All methods will return appropriate errors
        // when called, so this is safe to construct even on unsupported platforms.
        Self
    }
}

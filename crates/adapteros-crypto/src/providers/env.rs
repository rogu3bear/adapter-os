//! Environment-based key provider (development-only)
//!
//! Loads key material from process environment and keeps it in memory.
//! This provider is intended for local development and testing only.

use crate::key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, ProviderAttestation, RotationReceipt,
};
use adapteros_core::{AosError, B3Hash, Result};
use ed25519_dalek::Signer;

/// In-memory key provider backed by `AOS_SIGNING_KEY`.
///
/// Supports Ed25519 signing for the fixed key id `"default"`.
#[derive(Clone)]
pub struct EnvProvider {
    signing_key: crate::signature::SigningKey,
}

impl EnvProvider {
    pub fn new(signing_key: crate::signature::SigningKey) -> Self {
        Self { signing_key }
    }

    fn require_default_key(&self, key_id: &str) -> Result<()> {
        if key_id != "default" {
            return Err(AosError::Config(format!(
                "EnvProvider only supports key_id=\"default\" (got {key_id})"
            )));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl KeyProvider for EnvProvider {
    async fn generate(&self, _key_id: &str, _alg: KeyAlgorithm) -> Result<KeyHandle> {
        Err(AosError::Config(
            "EnvProvider is read-only and does not support key generation".to_string(),
        ))
    }

    async fn sign(&self, key_id: &str, msg: &[u8]) -> Result<Vec<u8>> {
        self.require_default_key(key_id)?;
        let sig = self.signing_key.sign(msg);
        Ok(sig.to_bytes().to_vec())
    }

    async fn seal(&self, _key_id: &str, _plaintext: &[u8]) -> Result<Vec<u8>> {
        Err(AosError::Config(
            "EnvProvider does not support encryption (seal)".to_string(),
        ))
    }

    async fn unseal(&self, _key_id: &str, _ciphertext: &[u8]) -> Result<Vec<u8>> {
        Err(AosError::Config(
            "EnvProvider does not support decryption (unseal)".to_string(),
        ))
    }

    async fn rotate(&self, _key_id: &str) -> Result<RotationReceipt> {
        Err(AosError::Config(
            "EnvProvider does not support key rotation".to_string(),
        ))
    }

    async fn attest(&self) -> Result<ProviderAttestation> {
        let public_key = self.signing_key.verifying_key();
        let public_key_bytes = public_key.to_bytes();

        let provider_type = "env".to_string();
        let fingerprint = B3Hash::hash(&public_key_bytes).to_hex();
        let policy_hash = B3Hash::hash(b"env-provider").to_hex();
        let timestamp = adapteros_core::time::unix_timestamp_secs();

        let msg = format!("{provider_type}|{fingerprint}|{policy_hash}|{timestamp}");
        let signature = self.signing_key.sign(msg.as_bytes()).to_bytes().to_vec();

        Ok(ProviderAttestation {
            provider_type,
            fingerprint,
            policy_hash,
            timestamp,
            signature,
        })
    }
}

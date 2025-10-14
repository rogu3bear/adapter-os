//! Policy pack signing and verification
//!
//! Implements Ed25519 signing and verification for policy packs to ensure
//! integrity and authenticity of policy configurations.

use adapteros_core::{AosError, Result};
use adapteros_crypto::signature::{Keypair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Policy pack with signature metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPolicyPack {
    pub policy_id: String,
    pub version: String,
    pub policy_data: BTreeMap<String, serde_json::Value>,
    pub signature: String,
    pub public_key: String,
    pub schema_version: u8,
}

impl SignedPolicyPack {
    /// Sign a policy pack with Ed25519
    pub fn sign(
        policy_id: &str,
        version: &str,
        policy_data: BTreeMap<String, serde_json::Value>,
        signing_key: &Keypair,
    ) -> Result<Self> {
        let policy_bytes = serde_json::to_vec(&policy_data).map_err(AosError::Serialization)?;

        let signature = signing_key.sign(&policy_bytes);
        let public_key = signing_key.public_key();

        Ok(Self {
            policy_id: policy_id.to_string(),
            version: version.to_string(),
            policy_data,
            signature: hex::encode(signature.to_bytes()),
            public_key: hex::encode(public_key.to_bytes()),
            schema_version: 1,
        })
    }

    /// Verify policy pack signature
    pub fn verify_signature(&self, trusted_pubkey: &PublicKey) -> Result<()> {
        // Verify signature against canonical JSON
        let policy_bytes =
            serde_json::to_vec(&self.policy_data).map_err(AosError::Serialization)?;

        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: {}",
                sig_bytes.len()
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array)
            .map_err(|e| AosError::Crypto(format!("Invalid signature format: {}", e)))?;

        trusted_pubkey
            .verify(&policy_bytes, &signature)
            .map_err(|e| {
                AosError::Crypto(format!("Policy pack signature verification failed: {}", e))
            })?;

        Ok(())
    }

    /// Verify schema version compatibility
    pub fn verify_schema_version(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(AosError::Crypto(format!(
                "Unsupported policy pack schema version: {}",
                self.schema_version
            )));
        }
        Ok(())
    }
}

/// Policy pack registry with signature verification
pub struct PolicyPackRegistry {
    trusted_keys: Vec<PublicKey>,
    signed_packs: BTreeMap<String, SignedPolicyPack>,
}

impl Default for PolicyPackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyPackRegistry {
    /// Create a new policy pack registry
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
            signed_packs: BTreeMap::new(),
        }
    }

    /// Add a trusted public key for verification
    pub fn add_trusted_key(&mut self, pubkey: PublicKey) {
        self.trusted_keys.push(pubkey);
    }

    /// Register a signed policy pack
    pub fn register_pack(&mut self, pack: SignedPolicyPack) -> Result<()> {
        // Verify schema version
        pack.verify_schema_version()?;

        // Verify signature against trusted keys
        let mut verified = false;
        for trusted_key in &self.trusted_keys {
            if pack.verify_signature(trusted_key).is_ok() {
                verified = true;
                break;
            }
        }

        if !verified {
            return Err(AosError::Crypto(
                "Policy pack signature verification failed against all trusted keys".to_string(),
            ));
        }

        self.signed_packs.insert(pack.policy_id.clone(), pack);
        Ok(())
    }

    /// Get a policy pack by ID
    pub fn get_pack(&self, policy_id: &str) -> Option<&SignedPolicyPack> {
        self.signed_packs.get(policy_id)
    }

    /// List all registered policy packs
    pub fn list_packs(&self) -> Vec<&SignedPolicyPack> {
        self.signed_packs.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_pack_signing() {
        let keypair = Keypair::generate();
        let mut policy_data = BTreeMap::new();
        policy_data.insert(
            "test_key".to_string(),
            serde_json::Value::String("test_value".to_string()),
        );

        let signed_pack =
            SignedPolicyPack::sign("test_policy", "1.0", policy_data.clone(), &keypair).unwrap();

        assert_eq!(signed_pack.policy_id, "test_policy");
        assert_eq!(signed_pack.version, "1.0");
        assert_eq!(signed_pack.schema_version, 1);

        // Verify signature
        let public_key = keypair.public_key();
        signed_pack.verify_signature(&public_key).unwrap();
    }

    #[test]
    fn test_policy_pack_registry() {
        let keypair = Keypair::generate();
        let mut policy_data = BTreeMap::new();
        policy_data.insert(
            "test_key".to_string(),
            serde_json::Value::String("test_value".to_string()),
        );

        let signed_pack =
            SignedPolicyPack::sign("test_policy", "1.0", policy_data, &keypair).unwrap();

        let mut registry = PolicyPackRegistry::new();
        registry.add_trusted_key(keypair.public_key());

        registry.register_pack(signed_pack).unwrap();

        let retrieved_pack = registry.get_pack("test_policy").unwrap();
        assert_eq!(retrieved_pack.policy_id, "test_policy");
    }
}

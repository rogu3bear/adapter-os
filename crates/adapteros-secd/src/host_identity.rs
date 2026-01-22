//! Host identity management with Secure Enclave integration
//!
//! Provides hardware-rooted Ed25519 signing keys for host attestation and
//! telemetry bundle signing (Secrets Ruleset #14).

use adapteros_core::Result;
use adapteros_crypto::{Keypair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Detect macOS hardware model identifier using system_profiler
fn detect_hardware_model() -> String {
    let output = Command::new("system_profiler")
        .args(["SPHardwareDataType", "-json"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let json_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(model) = json
                    .get("SPHardwareDataType")
                    .and_then(|arr| arr.get(0))
                    .and_then(|hw| hw.get("machine_model"))
                    .and_then(|v| v.as_str())
                {
                    return model.to_string();
                }
                // Fallback to model_name if machine_model not found
                if let Some(model) = json
                    .get("SPHardwareDataType")
                    .and_then(|arr| arr.get(0))
                    .and_then(|hw| hw.get("model_name"))
                    .and_then(|v| v.as_str())
                {
                    return model.to_string();
                }
            }
        }
        Ok(output) => {
            warn!(
                "system_profiler failed with status: {:?}",
                output.status.code()
            );
        }
        Err(e) => {
            warn!("Failed to execute system_profiler: {}", e);
        }
    }

    // Fallback: use sysctl for hw.model
    let sysctl_output = Command::new("sysctl").args(["-n", "hw.model"]).output();

    match sysctl_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown-hardware".to_string(),
    }
}

/// Detect Secure Enclave version/capability using ioreg
fn detect_secure_enclave_version() -> String {
    // Check if Secure Enclave is available (Apple Silicon or T2 chip)
    let output = Command::new("ioreg")
        .args(["-c", "AppleSEPManager", "-d", "1"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains("AppleSEPManager") {
                // Check for specific SEP properties
                let version_output = Command::new("ioreg")
                    .args(["-c", "AppleSEPManager", "-r", "-d", "1"])
                    .output();

                if let Ok(ver_out) = version_output {
                    let ver_str = String::from_utf8_lossy(&ver_out.stdout);

                    // Extract SEP version if available
                    for line in ver_str.lines() {
                        if line.contains("\"seprom-version\"") || line.contains("\"sep-version\"") {
                            if let Some(version) = line.split('=').nth(1) {
                                let cleaned = version
                                    .trim()
                                    .trim_matches('"')
                                    .trim_matches('<')
                                    .trim_matches('>')
                                    .to_string();
                                if !cleaned.is_empty() {
                                    return format!("SEP-{}", cleaned);
                                }
                            }
                        }
                    }

                    // Check chip generation based on AppleSEPManager presence
                    if ver_str.contains("apple,") {
                        // Apple Silicon (M1/M2/M3)
                        return "SEP-AppleSilicon".to_string();
                    }

                    return "SEP-v1".to_string();
                }

                return "SEP-available".to_string();
            }
        }
        Ok(_) => {
            debug!("AppleSEPManager not found in ioreg");
        }
        Err(e) => {
            warn!("Failed to execute ioreg: {}", e);
        }
    }

    // Check for T2 chip as fallback
    let t2_output = Command::new("ioreg")
        .args(["-c", "AppleT2Controller"])
        .output();

    if let Ok(output) = t2_output {
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("AppleT2Controller")
        {
            return "SEP-T2".to_string();
        }
    }

    "SEP-unavailable".to_string()
}

/// Host identity with hardware-backed signing key
#[derive(Debug, Clone)]
pub struct HostIdentity {
    /// Host ID (derived from public key)
    pub host_id: String,
    /// Public key
    pub pubkey: PublicKey,
    /// Key alias in Secure Enclave
    key_alias: String,
    /// Secure Enclave connection
    connection: Arc<SecureEnclaveConnection>,
}

/// Attestation metadata from Secure Enclave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationMetadata {
    /// Public key bytes
    pub pubkey: Vec<u8>,
    /// Attestation data (hardware-signed)
    pub attestation_data: Vec<u8>,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
    /// Hardware model identifier
    pub hardware_model: String,
    /// Secure Enclave version
    pub secure_enclave_version: String,
}

/// Attestation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    /// Public key
    pub pubkey: Vec<u8>,
    /// Attestation metadata
    pub attestation_metadata: AttestationMetadata,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
}

/// Secure Enclave connection (mock implementation for now)
pub struct SecureEnclaveConnection {
    /// Mock keypair (in production, this would be a handle to Secure Enclave)
    _mock_keypair: Keypair,
}

impl std::fmt::Debug for SecureEnclaveConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureEnclaveConnection")
            .field("mock_keypair", &"[REDACTED]")
            .finish()
    }
}

impl SecureEnclaveConnection {
    /// Create a new Secure Enclave connection
    pub fn new() -> Result<Self> {
        info!("Initializing Secure Enclave connection");

        // In production, this would establish connection to Secure Enclave
        // For now, we use a mock implementation
        let mock_keypair = Keypair::generate();

        Ok(Self {
            _mock_keypair: mock_keypair,
        })
    }

    /// Generate a new keypair in Secure Enclave
    pub fn generate_keypair(&self, _alias: &str) -> Result<PublicKey> {
        // In production, this would generate key in Secure Enclave
        let keypair = Keypair::generate();
        Ok(keypair.public_key())
    }

    /// Sign data with Secure Enclave key
    pub fn sign(&self, _alias: &str, data: &[u8]) -> Result<Signature> {
        // In production, this would sign using Secure Enclave
        let keypair = Keypair::generate();
        Ok(keypair.sign(data))
    }

    /// Get public key from Secure Enclave
    pub fn get_public_key(&self, _alias: &str) -> Result<PublicKey> {
        // In production, this would retrieve key from Secure Enclave
        let keypair = Keypair::generate();
        Ok(keypair.public_key())
    }

    /// Attest key in Secure Enclave
    pub fn attest_key(&self, _alias: &str) -> Result<Vec<u8>> {
        // In production, this would request hardware attestation
        Ok(vec![0u8; 64]) // Mock attestation data
    }
}

// NOTE: Intentionally no Default impl for SecureEnclaveConnection.
// Security-critical components should fail explicitly at construction time,
// not panic during default initialization. Use SecureEnclaveConnection::new()
// directly and handle errors appropriately.

/// Host identity manager
pub struct HostIdentityManager {
    /// Secure Enclave connection
    connection: Arc<SecureEnclaveConnection>,
    /// Key alias
    key_alias: String,
}

impl HostIdentityManager {
    /// Create a new host identity manager
    pub fn new(key_alias: String) -> Result<Self> {
        let connection = Arc::new(SecureEnclaveConnection::new()?);
        Ok(Self {
            connection,
            key_alias,
        })
    }

    /// Generate host signing key in Secure Enclave
    pub fn generate_host_key(&self, alias: &str) -> Result<PublicKey> {
        info!("Generating host key in Secure Enclave: {}", alias);

        let pubkey = self.connection.generate_keypair(alias)?;

        debug!("Generated host key: {}", hex::encode(pubkey.to_bytes()));

        Ok(pubkey)
    }

    /// Sign data with host key (private key never leaves Secure Enclave)
    pub fn sign_with_host_key(&self, data: &[u8]) -> Result<Signature> {
        self.connection.sign(&self.key_alias, data)
    }

    /// Get host public key
    pub fn get_host_public_key(&self) -> Result<PublicKey> {
        self.connection.get_public_key(&self.key_alias)
    }

    /// Attest host identity (returns hardware attestation)
    pub fn attest_host_identity(&self) -> Result<AttestationReport> {
        use adapteros_core::AosError;

        let pubkey = self.get_host_public_key()?;
        let attestation_data = self.connection.attest_key(&self.key_alias)?;

        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AosError::Crypto(format!("system time before UNIX epoch: {}", e)))?
            .as_micros() as u64;

        let attestation_metadata = AttestationMetadata {
            pubkey: pubkey.to_bytes().to_vec(),
            attestation_data,
            timestamp_us,
            hardware_model: detect_hardware_model(),
            secure_enclave_version: detect_secure_enclave_version(),
        };

        Ok(AttestationReport {
            pubkey: pubkey.to_bytes().to_vec(),
            attestation_metadata,
            timestamp_us,
        })
    }

    /// Create host identity
    pub fn create_host_identity(&self) -> Result<HostIdentity> {
        let pubkey = self.get_host_public_key()?;
        let host_id = Self::derive_host_id(&pubkey);

        Ok(HostIdentity {
            host_id,
            pubkey,
            key_alias: self.key_alias.clone(),
            connection: self.connection.clone(),
        })
    }

    /// Derive host ID from public key
    fn derive_host_id(pubkey: &PublicKey) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&pubkey.to_bytes());
        let hash = hasher.finalize();
        format!("host-{}", hex::encode(&hash.as_bytes()[..16]))
    }
}

impl HostIdentity {
    /// Sign data with this host identity
    pub fn sign(&self, data: &[u8]) -> Result<Signature> {
        self.connection.sign(&self.key_alias, data)
    }

    /// Get attestation report
    pub fn attest(&self) -> Result<AttestationReport> {
        use adapteros_core::AosError;

        let attestation_data = self.connection.attest_key(&self.key_alias)?;

        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AosError::Crypto(format!("system time before UNIX epoch: {}", e)))?
            .as_micros() as u64;

        let attestation_metadata = AttestationMetadata {
            pubkey: self.pubkey.to_bytes().to_vec(),
            attestation_data,
            timestamp_us,
            hardware_model: detect_hardware_model(),
            secure_enclave_version: detect_secure_enclave_version(),
        };

        Ok(AttestationReport {
            pubkey: self.pubkey.to_bytes().to_vec(),
            attestation_metadata,
            timestamp_us,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_identity_generation() {
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        assert!(!identity.host_id.is_empty());
        assert!(identity.host_id.starts_with("host-"));
    }

    #[test]
    fn test_signing() {
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        let data = b"test message";
        let signature = identity.sign(data).unwrap();

        // Verify signature
        assert!(identity.pubkey.verify(data, &signature).is_ok());
    }

    #[test]
    fn test_attestation() {
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        let report = identity.attest().unwrap();

        assert!(!report.pubkey.is_empty());
        assert!(report.timestamp_us > 0);
    }
}

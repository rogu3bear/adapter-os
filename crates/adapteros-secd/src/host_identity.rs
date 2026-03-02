//! Host identity management with Secure Enclave integration
//!
//! Provides hardware-rooted Ed25519 signing keys for host attestation and
//! telemetry bundle signing (Secrets Ruleset #14).
//!
//! ## Stub Implementation: SecureEnclaveConnection
//!
//! This module contains a **stub implementation** of [`SecureEnclaveConnection`] that
//! uses software cryptography instead of hardware Secure Enclave operations.
//!
//! ### Why This Is a Stub
//!
//! The Secure Enclave Processor (SEP) requires:
//! - Apple Silicon (M1/M2/M3) or Intel Mac with T2 chip
//! - macOS with proper entitlements for Secure Enclave access
//! - The `secure-enclave` feature flag enabled
//!
//! This module provides a mock implementation for:
//! - Cross-platform development and testing
//! - CI/CD environments without Apple hardware
//! - Rapid prototyping before hardware integration
//!
//! ### What Would Be Needed for Full Implementation
//!
//! 1. **Hardware-backed key generation**:
//!    - Use `SecKeyCreateRandomKey` with `kSecAttrTokenIDSecureEnclave`
//!    - Keys never leave the hardware
//!
//! 2. **Hardware-backed signing**:
//!    - Use `SecKeyCreateSignature` with SEP-resident key
//!    - Private key operations happen in hardware
//!
//! 3. **Real attestation**:
//!    - Use `SecKeyCopyAttestation` for hardware attestation
//!    - Provides cryptographic proof key resides in SEP
//!
//! ### Current Stub Behavior
//!
//! | Operation | Stub Behavior |
//! |-----------|---------------|
//! | `generate_keypair()` | Generates new Ed25519 keypair in memory |
//! | `sign()` | Signs with in-memory mock keypair (per-process stable) |
//! | `get_public_key()` | Returns in-memory mock public key |
//! | `attest_key()` | Returns 64 zero bytes (mock attestation) |
//!
//! **Warning**: This remains a development stub. Keys are software-only and
//! process-local, so they provide no hardware trust guarantees.
//!
//! ### Hardware Detection
//!
//! The module includes real hardware detection functions:
//! - [`detect_hardware_model()`]: Uses `system_profiler` to identify Mac model
//! - [`detect_secure_enclave_version()`]: Uses `ioreg` to check SEP availability
//!
//! These work correctly and populate [`AttestationMetadata`] with real hardware info,
//! even though the cryptographic operations are stubbed.

use adapteros_core::Result;
use adapteros_crypto::{Keypair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

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
    /// Attestation mode: hardware-backed or synthetic fallback
    pub attestation_kind: String,
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

/// Secure Enclave connection for host identity operations.
///
/// # Stub Implementation
///
/// This is a **mock/stub implementation** that uses software cryptography instead of
/// the actual macOS Secure Enclave. It exists to provide API compatibility for:
/// - Cross-platform development
/// - CI/CD testing without Apple hardware
/// - Prototyping before hardware integration
///
/// ## Current Limitations
///
/// - **No hardware persistence**: Keys are process-local and software-backed
/// - **No hardware binding**: Keys exist only in process memory
/// - **Mock attestation**: Returns placeholder data (64 zero bytes)
/// - **No reboot durability**: Key material is regenerated on process start
///
/// ## Full Implementation Requirements
///
/// A production implementation would need:
/// 1. `SecKeyCreateRandomKey` with `kSecAttrTokenIDSecureEnclave` for key generation
/// 2. `SecKeyCreateSignature` for hardware-backed signing
/// 3. `SecKeyCopyAttestation` for hardware attestation (macOS 13.0+)
/// 4. Proper entitlements in the app's code signature
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
    /// Create a new Secure Enclave connection.
    ///
    /// # Stub Behavior
    ///
    /// This stub generates a mock Ed25519 keypair in memory. The keypair is used
    /// by `sign()` and `get_public_key()` for per-process stable behavior.
    pub fn new() -> Result<Self> {
        info!("Initializing Secure Enclave connection");

        // STUB: In production, this would establish connection to Secure Enclave
        // via Security.framework and verify hardware availability.
        let mock_keypair = Keypair::generate();

        Ok(Self {
            _mock_keypair: mock_keypair,
        })
    }

    /// Generate a new keypair in Secure Enclave.
    ///
    /// # Stub Behavior
    ///
    /// Generates a fresh Ed25519 keypair in memory and returns its public key.
    /// The private key is immediately discarded (not stored).
    ///
    /// # Production Implementation
    ///
    /// Would use `SecKeyCreateRandomKey` with:
    /// - `kSecAttrTokenIDSecureEnclave` for hardware binding
    /// - `kSecAttrKeyTypeECSECPrimeRandom` for ECDSA P-256
    /// - `kSecAttrIsPermanent = true` for keychain persistence
    pub fn generate_keypair(&self, _alias: &str) -> Result<PublicKey> {
        // STUB: Generates ephemeral keypair, discards private key
        let keypair = Keypair::generate();
        Ok(keypair.public_key())
    }

    /// Sign data with Secure Enclave key.
    ///
    /// # Stub Behavior
    ///
    /// Uses the process-local mock keypair, so signatures can be verified with
    /// `get_public_key()` for testing workflows.
    ///
    /// # Production Implementation
    ///
    /// Would use `SecKeyCreateSignature` with the key handle retrieved by alias,
    /// keeping the private key in hardware.
    pub fn sign(&self, _alias: &str, data: &[u8]) -> Result<Signature> {
        // STUB: signs with a process-local software keypair.
        Ok(self._mock_keypair.sign(data))
    }

    /// Get public key from Secure Enclave.
    ///
    /// # Stub Behavior
    ///
    /// Returns the process-local mock public key associated with this connection.
    ///
    /// # Production Implementation
    ///
    /// Would use `SecItemCopyMatching` to retrieve the key by alias and
    /// `SecKeyCopyPublicKey` to extract the public component.
    pub fn get_public_key(&self, _alias: &str) -> Result<PublicKey> {
        // STUB: returns public key for the process-local software keypair.
        Ok(self._mock_keypair.public_key())
    }

    /// Attest key in Secure Enclave.
    ///
    /// # Stub Behavior
    ///
    /// Returns 64 zero bytes as mock attestation data. This provides no
    /// cryptographic guarantees and should not be trusted.
    ///
    /// # Production Implementation
    ///
    /// Would use `SecKeyCopyAttestation` (macOS 13.0+) to get hardware-signed
    /// attestation proving the key resides in the Secure Enclave. The attestation
    /// can be verified against Apple's attestation root certificate.
    pub fn attest_key(&self, _alias: &str) -> Result<Vec<u8>> {
        // STUB: Returns placeholder data - no hardware attestation
        Ok(vec![0u8; 64])
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
        let attestation_kind = infer_attestation_kind(&attestation_data);
        let require_hardware = require_hardware_attestation();
        emit_attestation_visibility_event(
            "host_identity_manager.attest_host_identity",
            &self.key_alias,
            &attestation_kind,
            require_hardware,
            attestation_data.len(),
        );
        if require_hardware && attestation_kind != "hardware" {
            error!(
                target: "security.attestation",
                event = "hardware_attestation_gate_rejected",
                source = "host_identity_manager.attest_host_identity",
                key_alias = %self.key_alias,
                attestation_kind = %attestation_kind,
                "Hardware attestation required and synthetic attestation was rejected"
            );
            error!(
                target: "security.audit",
                event = "attestation_gate_rejected",
                source = "host_identity_manager.attest_host_identity",
                key_alias = %self.key_alias,
                gate = "AOS_REQUIRE_HARDWARE_ATTESTATION",
                "Attestation gate blocked synthetic attestation"
            );
            return Err(AosError::Crypto(
                "hardware attestation required but synthetic attestation was produced".to_string(),
            ));
        }

        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AosError::Crypto(format!("system time before UNIX epoch: {}", e)))?
            .as_micros() as u64;

        let attestation_metadata = AttestationMetadata {
            pubkey: pubkey.to_bytes().to_vec(),
            attestation_data,
            attestation_kind,
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
        let attestation_kind = infer_attestation_kind(&attestation_data);
        let require_hardware = require_hardware_attestation();
        emit_attestation_visibility_event(
            "host_identity.attest",
            &self.key_alias,
            &attestation_kind,
            require_hardware,
            attestation_data.len(),
        );
        if require_hardware && attestation_kind != "hardware" {
            error!(
                target: "security.attestation",
                event = "hardware_attestation_gate_rejected",
                source = "host_identity.attest",
                key_alias = %self.key_alias,
                attestation_kind = %attestation_kind,
                "Hardware attestation required and synthetic attestation was rejected"
            );
            error!(
                target: "security.audit",
                event = "attestation_gate_rejected",
                source = "host_identity.attest",
                key_alias = %self.key_alias,
                gate = "AOS_REQUIRE_HARDWARE_ATTESTATION",
                "Attestation gate blocked synthetic attestation"
            );
            return Err(AosError::Crypto(
                "hardware attestation required but synthetic attestation was produced".to_string(),
            ));
        }

        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AosError::Crypto(format!("system time before UNIX epoch: {}", e)))?
            .as_micros() as u64;

        let attestation_metadata = AttestationMetadata {
            pubkey: self.pubkey.to_bytes().to_vec(),
            attestation_data,
            attestation_kind,
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

fn infer_attestation_kind(attestation_data: &[u8]) -> String {
    if attestation_data.iter().any(|byte| *byte != 0) {
        "hardware".to_string()
    } else {
        "synthetic".to_string()
    }
}

fn emit_attestation_visibility_event(
    source: &'static str,
    key_alias: &str,
    attestation_kind: &str,
    require_hardware: bool,
    attestation_len: usize,
) {
    let trust_downgraded = attestation_kind != "hardware";
    if trust_downgraded {
        warn!(
            target: "security.attestation",
            event = "attestation_mode",
            source,
            key_alias = %key_alias,
            attestation_kind = %attestation_kind,
            trust_downgraded = true,
            require_hardware_attestation = require_hardware,
            attestation_len,
            "Synthetic attestation selected"
        );
        warn!(
            target: "security.audit",
            event = "attestation_trust_downgrade",
            source,
            key_alias = %key_alias,
            attestation_kind = %attestation_kind,
            require_hardware_attestation = require_hardware,
            "Attestation trust downgraded to synthetic"
        );
    } else {
        info!(
            target: "security.attestation",
            event = "attestation_mode",
            source,
            key_alias = %key_alias,
            attestation_kind = %attestation_kind,
            trust_downgraded = false,
            require_hardware_attestation = require_hardware,
            attestation_len,
            "Hardware attestation selected"
        );
    }
}

fn require_hardware_attestation() -> bool {
    match std::env::var("AOS_REQUIRE_HARDWARE_ATTESTATION") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
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

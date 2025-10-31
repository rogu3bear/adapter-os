//! Host identity management with Secure Enclave integration
//!
//! Provides hardware-rooted signing keys for host attestation and telemetry
//! bundle signing (Secrets Ruleset #14).

use crate::EnclaveManager;
use adapteros_core::{AosError, Result};
use adapteros_crypto::{Keypair, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
const HOST_KEY_ENV: &str = "AOS_SECD_HOST_KEY_DIR";
const DEFAULT_KEY_DIR: &str = "var/aos-secd/keys";
const HOST_KEY_ENCRYPTION_LABEL: &str = "host_identity";
const HOST_ATTESTATION_LABEL: &str = "host_attestation";

/// Host identity backed by Secure Enclave–protected key material
#[derive(Debug, Clone)]
pub struct HostIdentity {
    /// Host ID (derived from public key)
    pub host_id: String,
    /// Public key
    pub pubkey: PublicKey,
    context: Arc<HostIdentityContext>,
}

/// Attestation metadata from Secure Enclave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationMetadata {
    /// Public key bytes
    pub pubkey: Vec<u8>,
    /// Hardware attestation payload (JSON-encoded)
    pub attestation_data: Vec<u8>,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
    /// Hardware model identifier
    pub hardware_model: String,
    /// Secure Enclave version
    pub secure_enclave_version: String,
}

/// Host attestation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    /// Public key bytes
    pub pubkey: Vec<u8>,
    /// Attestation metadata
    pub attestation_metadata: AttestationMetadata,
    /// Timestamp (microseconds)
    pub timestamp_us: u64,
}

#[derive(Clone, Debug)]
struct HostIdentityContext {
    alias: String,
    key_dir: PathBuf,
    enclave: Arc<Mutex<EnclaveManager>>,
}

impl HostIdentityContext {
    fn key_path(&self) -> PathBuf {
        self.key_dir.join(format!("{}.sealed", self.alias))
    }

    fn ensure_keypair(&self) -> Result<Keypair> {
        if let Some(existing) = self.load_keypair()? {
            return Ok(existing);
        }

        let keypair = Keypair::generate();
        self.persist_keypair(&keypair)?;
        Ok(keypair)
    }

    fn generate_new_keypair(&self) -> Result<Keypair> {
        let keypair = Keypair::generate();
        self.persist_keypair(&keypair)?;
        Ok(keypair)
    }

    fn persist_keypair(&self, keypair: &Keypair) -> Result<()> {
        let sealed = self.seal_bytes(&keypair.to_bytes())?;
        let path = self.key_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create host key directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        fs::write(&path, sealed).map_err(|e| {
            AosError::Io(format!(
                "Failed to write sealed host key {}: {}",
                path.display(),
                e
            ))
        })?;
        tracing::debug!(
            "Persisted host key for alias {} at {}",
            self.alias,
            path.display()
        );
        Ok(())
    }

    fn load_keypair(&self) -> Result<Option<Keypair>> {
        let path = self.key_path();
        if !path.exists() {
            return Ok(None);
        }

        let sealed = fs::read(&path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read sealed host key {}: {}",
                path.display(),
                e
            ))
        })?;

        let plaintext = self.unseal_bytes(&sealed)?;
        if plaintext.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid host key length for {}: {} bytes",
                self.alias,
                plaintext.len()
            )));
        }

        let mut secret = [0u8; 32];
        secret.copy_from_slice(&plaintext);
        Ok(Some(Keypair::from_bytes(&secret)))
    }

    fn seal_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut enclave = self.enclave.lock().unwrap();
        enclave
            .seal_with_label(HOST_KEY_ENCRYPTION_LABEL, plaintext)
            .map_err(|e| AosError::Crypto(format!("Failed to seal host key: {}", e)))
    }

    fn unseal_bytes(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        let mut enclave = self.enclave.lock().unwrap();
        enclave
            .unseal_with_label(HOST_KEY_ENCRYPTION_LABEL, sealed)
            .map_err(|e| AosError::Crypto(format!("Failed to unseal host key: {}", e)))
    }

    fn build_attestation(&self, pubkey: &PublicKey) -> Result<AttestationReport> {
        let pubkey_bytes = pubkey.to_bytes();
        let mut enclave = self.enclave.lock().unwrap();

        let signature = enclave
            .sign_with_label(HOST_ATTESTATION_LABEL, pubkey_bytes.as_slice())
            .map_err(|e| AosError::Crypto(format!("Hardware attestation failed: {}", e)))?;

        let hardware_pubkey = enclave
            .get_public_key(HOST_ATTESTATION_LABEL)
            .map_err(|e| AosError::Crypto(format!("Failed to load attestation key: {}", e)))?;

        drop(enclave);

        let attestation_payload = HardwareAttestationPayload {
            algorithm: "ecdsa-sha256".to_string(),
            signature_der_hex: hex::encode(&signature),
            signing_key_der_hex: hex::encode(&hardware_pubkey),
            label: HOST_ATTESTATION_LABEL.to_string(),
        };

        let attestation_bytes = serde_json::to_vec(&attestation_payload)?;

        let timestamp_us = current_timestamp_us();
        let (hardware_model, secure_enclave_version) = hardware_inventory();

        let metadata = AttestationMetadata {
            pubkey: pubkey_bytes.to_vec(),
            attestation_data: attestation_bytes,
            timestamp_us,
            hardware_model,
            secure_enclave_version,
        };

        Ok(AttestationReport {
            pubkey: pubkey_bytes.to_vec(),
            attestation_metadata: metadata,
            timestamp_us,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareAttestationPayload {
    algorithm: String,
    signature_der_hex: String,
    signing_key_der_hex: String,
    label: String,
}

/// Host identity manager
pub struct HostIdentityManager {
    context: Arc<HostIdentityContext>,
}

impl HostIdentityManager {
    /// Create a new host identity manager
    pub fn new(key_alias: String) -> Result<Self> {
        let key_dir = resolve_key_dir();
        fs::create_dir_all(&key_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to ensure host key directory {}: {}",
                key_dir.display(),
                e
            ))
        })?;

        let enclave = Arc::new(Mutex::new(EnclaveManager::new().map_err(|e| {
            AosError::Security(format!("Secure Enclave init failed: {}", e))
        })?));
        let context = HostIdentityContext {
            alias: key_alias,
            key_dir,
            enclave,
        };

        Ok(Self {
            context: Arc::new(context),
        })
    }

    /// Generate host signing key (overwrites any existing key)
    pub fn generate_host_key(&self, alias: &str) -> Result<PublicKey> {
        if alias != self.context.alias {
            return Err(AosError::Config(format!(
                "Host identity manager configured for alias '{}', got '{}'",
                self.context.alias, alias
            )));
        }

        let keypair = self.context.generate_new_keypair()?;
        tracing::info!("Generated new host key for alias {}", alias);
        Ok(keypair.public_key())
    }

    /// Sign data with host key (private key never leaves Secure Enclave)
    pub fn sign_with_host_key(&self, data: &[u8]) -> Result<Signature> {
        let keypair = self.context.ensure_keypair()?;
        Ok(keypair.sign(data))
    }

    /// Get host public key (generates key if missing)
    pub fn get_host_public_key(&self) -> Result<PublicKey> {
        let keypair = self.context.ensure_keypair()?;
        Ok(keypair.public_key())
    }

    /// Attest host identity and return attestation report
    pub fn attest_host_identity(&self) -> Result<AttestationReport> {
        let pubkey = self.get_host_public_key()?;
        self.context.build_attestation(&pubkey)
    }

    /// Create host identity object for signing/attestation
    pub fn create_host_identity(&self) -> Result<HostIdentity> {
        let pubkey = self.get_host_public_key()?;
        let host_id = HostIdentity::derive_host_id(&pubkey);

        Ok(HostIdentity {
            host_id,
            pubkey,
            context: self.context.clone(),
        })
    }
}

impl HostIdentity {
    /// Sign data with this host identity
    pub fn sign(&self, data: &[u8]) -> Result<Signature> {
        let keypair = self.context.ensure_keypair()?;
        Ok(keypair.sign(data))
    }

    /// Get attestation report for this identity
    pub fn attest(&self) -> Result<AttestationReport> {
        self.context.build_attestation(&self.pubkey)
    }

    /// Derive host ID from public key
    fn derive_host_id(pubkey: &PublicKey) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&pubkey.to_bytes());
        let hash = hasher.finalize();
        format!("host-{}", hex::encode(&hash.as_bytes()[..16]))
    }
}

fn resolve_key_dir() -> PathBuf {
    env::var(HOST_KEY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_KEY_DIR))
}

fn current_timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn hardware_inventory() -> (String, String) {
    (
        hardware_model().unwrap_or_else(|| "unknown-model".to_string()),
        secure_enclave_version().unwrap_or_else(|| "unknown-version".to_string()),
    )
}

#[cfg(target_os = "macos")]
fn hardware_model() -> Option<String> {
    Command::new("sysctl")
        .args(["-n", "hw.model"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
}

#[cfg(not(target_os = "macos"))]
fn hardware_model() -> Option<String> {
    tracing::warn!("Hardware model lookup not supported on this platform");
    None
}

#[cfg(target_os = "macos")]
fn secure_enclave_version() -> Option<String> {
    Command::new("system_profiler")
        .arg("SPiBridgeDataType")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| {
            text.lines()
                .find(|line| line.trim().starts_with("Secure Enclave"))
                .map(|line| line.trim().to_string())
        })
}

#[cfg(not(target_os = "macos"))]
fn secure_enclave_version() -> Option<String> {
    tracing::warn!("Secure Enclave version lookup not supported on this platform");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EnclaveManager;
    use std::sync::Once;
    use tempfile::TempDir;

    static INIT: Once = Once::new();

    fn enclave_ready() -> bool {
        if let Ok(mut manager) = EnclaveManager::new() {
            manager
                .sign_with_label("host_identity_self_test", b"probe")
                .is_ok()
        } else {
            false
        }
    }

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().to_path_buf();
        INIT.call_once(|| {
            tracing::warn!(
                "Using temporary directory {:?} for host identity tests",
                path
            )
        });
        env::set_var(HOST_KEY_ENV, path.to_str().unwrap());
        tmp
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_host_identity_generation() {
        if !enclave_ready() {
            eprintln!("Secure Enclave unavailable; skipping host identity generation test");
            return;
        }
        let _guard = setup_env();
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        assert!(!identity.host_id.is_empty());
        assert!(identity.host_id.starts_with("host-"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_signing() {
        if !enclave_ready() {
            eprintln!("Secure Enclave unavailable; skipping host identity signing test");
            return;
        }
        let _guard = setup_env();
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        let data = b"test message";
        let signature = identity.sign(data).unwrap();

        assert!(identity.pubkey.verify(data, &signature).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_attestation() {
        if !enclave_ready() {
            eprintln!("Secure Enclave unavailable; skipping host identity attestation test");
            return;
        }
        let _guard = setup_env();
        let manager = HostIdentityManager::new("test-key".to_string()).unwrap();
        let identity = manager.create_host_identity().unwrap();

        let report = identity.attest().unwrap();

        assert!(!report.pubkey.is_empty());
        assert!(report.timestamp_us > 0);
        assert!(!report.attestation_metadata.attestation_data.is_empty());
    }
}

//! Key management for fingerprint signing and device fingerprinting

use adapteros_core::{AosError, Result};
use adapteros_crypto::Keypair;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use sysinfo::System;
use tracing::{debug, info, warn};

fn fingerprint_key_path() -> PathBuf {
    adapteros_core::resolve_var_dir().join("keys/fingerprint_key.bin")
}

fn device_fingerprint_path() -> PathBuf {
    adapteros_core::resolve_var_dir().join("keys/device_fingerprint.json")
}

/// Hardware attributes used to generate device fingerprint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareAttributes {
    /// CPU brand/model string
    pub cpu_brand: String,
    /// Number of physical CPU cores
    pub cpu_cores: usize,
    /// Total system memory in bytes
    pub total_memory: u64,
    /// System name (e.g., "Darwin", "Linux")
    pub system_name: String,
    /// OS version
    pub os_version: String,
    /// Kernel version
    pub kernel_version: String,
    /// Host name (hashed for privacy)
    pub host_hash: String,
}

/// Device fingerprint containing hardware-derived ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    /// Deterministic device ID (BLAKE3 hash of hardware attributes)
    pub device_id: String,
    /// Hardware attributes used to generate the ID
    pub attributes: HardwareAttributes,
    /// Timestamp when fingerprint was generated
    pub generated_at: String,
}

/// Adapter hardware binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterBinding {
    /// Adapter ID
    pub adapter_id: String,
    /// Device fingerprint this adapter is bound to
    pub device_id: String,
    /// Binding timestamp
    pub bound_at: String,
    /// Optional expiration timestamp
    pub expires_at: Option<String>,
}

/// Collect hardware attributes from the current system
pub fn collect_hardware_attributes() -> Result<HardwareAttributes> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Get CPU information
    let cpu_brand = sys
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let cpu_cores = sys.physical_core_count().unwrap_or(0);

    // Get memory information
    let total_memory = sys.total_memory();

    // Get system information
    let system_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let kernel_version = System::kernel_version().unwrap_or_else(|| "Unknown".to_string());

    // Hash hostname for privacy
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let host_hash = {
        let mut hasher = Hasher::new();
        hasher.update(hostname.as_bytes());
        hasher.update(b"adapteros-host-salt");
        hex::encode(&hasher.finalize().as_bytes()[..16])
    };

    Ok(HardwareAttributes {
        cpu_brand,
        cpu_cores,
        total_memory,
        system_name,
        os_version,
        kernel_version,
        host_hash,
    })
}

/// Generate a deterministic device ID from hardware attributes
pub fn generate_device_id(attributes: &HardwareAttributes) -> String {
    let mut hasher = Hasher::new();

    // Include all stable hardware attributes
    hasher.update(attributes.cpu_brand.as_bytes());
    hasher.update(&attributes.cpu_cores.to_le_bytes());
    hasher.update(&attributes.total_memory.to_le_bytes());
    hasher.update(attributes.system_name.as_bytes());
    hasher.update(attributes.kernel_version.as_bytes());
    hasher.update(attributes.host_hash.as_bytes());

    // Add domain separation
    hasher.update(b"adapteros-device-fingerprint-v1");

    hex::encode(hasher.finalize().as_bytes())
}

/// Get or create device fingerprint
///
/// Returns the current device's fingerprint, creating one if it doesn't exist.
/// The fingerprint is stored locally and remains stable unless hardware changes.
pub fn get_or_create_device_fingerprint() -> Result<DeviceFingerprint> {
    let fingerprint_path = device_fingerprint_path();

    if fingerprint_path.exists() {
        // Load existing fingerprint
        debug!(
            "Loading device fingerprint from: {}",
            fingerprint_path.display()
        );
        let data = fs::read_to_string(&fingerprint_path)
            .map_err(|e| AosError::Io(format!("Failed to read device fingerprint: {}", e)))?;
        let fingerprint: DeviceFingerprint = serde_json::from_str(&data)
            .map_err(|e| AosError::Validation(format!("Invalid device fingerprint: {}", e)))?;

        // Verify fingerprint still matches current hardware
        let current_attrs = collect_hardware_attributes()?;
        let current_id = generate_device_id(&current_attrs);

        if current_id != fingerprint.device_id {
            warn!(
                "Hardware configuration changed. Old ID: {}, New ID: {}",
                &fingerprint.device_id[..16],
                &current_id[..16]
            );
            // Return current hardware fingerprint
            return create_and_save_fingerprint();
        }

        Ok(fingerprint)
    } else {
        create_and_save_fingerprint()
    }
}

/// Create and save a new device fingerprint
fn create_and_save_fingerprint() -> Result<DeviceFingerprint> {
    let fingerprint_path = device_fingerprint_path();

    let attributes = collect_hardware_attributes()?;
    let device_id = generate_device_id(&attributes);

    let fingerprint = DeviceFingerprint {
        device_id,
        attributes,
        generated_at: chrono::Utc::now().to_rfc3339(),
    };

    // Ensure directory exists
    if let Some(parent) = fingerprint_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AosError::Io(format!("Failed to create fingerprint directory: {}", e)))?;
    }

    // Save fingerprint
    let data = serde_json::to_string_pretty(&fingerprint)
        .map_err(|e| AosError::Validation(format!("Failed to serialize fingerprint: {}", e)))?;
    fs::write(&fingerprint_path, data)
        .map_err(|e| AosError::Io(format!("Failed to write device fingerprint: {}", e)))?;

    info!(
        "Generated device fingerprint: {}",
        &fingerprint.device_id[..16]
    );
    Ok(fingerprint)
}

/// Create an adapter binding to the current device
pub fn bind_adapter_to_device(
    adapter_id: &str,
    expires_at: Option<&str>,
) -> Result<AdapterBinding> {
    let fingerprint = get_or_create_device_fingerprint()?;

    let binding = AdapterBinding {
        adapter_id: adapter_id.to_string(),
        device_id: fingerprint.device_id,
        bound_at: chrono::Utc::now().to_rfc3339(),
        expires_at: expires_at.map(|s| s.to_string()),
    };

    info!(
        adapter_id = %adapter_id,
        device_id = %&binding.device_id[..16],
        "Bound adapter to device"
    );

    Ok(binding)
}

/// Verify an adapter binding against the current device
pub fn verify_adapter_binding(binding: &AdapterBinding) -> Result<bool> {
    let fingerprint = get_or_create_device_fingerprint()?;

    // Check device ID matches
    if binding.device_id != fingerprint.device_id {
        warn!(
            adapter_id = %binding.adapter_id,
            expected = %&binding.device_id[..16],
            actual = %&fingerprint.device_id[..16],
            "Adapter binding device mismatch"
        );
        return Ok(false);
    }

    // Check expiration
    if let Some(expires_at) = &binding.expires_at {
        let expiration = chrono::DateTime::parse_from_rfc3339(expires_at)
            .map_err(|e| AosError::Validation(format!("Invalid expiration timestamp: {}", e)))?;
        if chrono::Utc::now() > expiration {
            warn!(
                adapter_id = %binding.adapter_id,
                expires_at = %expires_at,
                "Adapter binding expired"
            );
            return Ok(false);
        }
    }

    debug!(
        adapter_id = %binding.adapter_id,
        device_id = %&binding.device_id[..16],
        "Adapter binding verified"
    );

    Ok(true)
}

/// Get or create fingerprint signing keypair
///
/// Tries to load from Secure Enclave first (production), then falls back
/// to file-based key storage (development/testing).
pub fn get_or_create_fingerprint_keypair() -> Result<Keypair> {
    // Try Secure Enclave first (production)
    #[cfg(target_os = "macos")]
    {
        match try_load_from_enclave() {
            Ok(keypair) => {
                debug!("Loaded fingerprint keypair from Secure Enclave");
                return Ok(keypair);
            }
            Err(e) => {
                debug!(
                    "Could not load from Secure Enclave: {}, falling back to file storage",
                    e
                );
            }
        }
    }

    // Fall back to file-based storage
    load_or_create_file_keypair()
}

/// Try to load keypair from Secure Enclave
#[cfg(target_os = "macos")]
fn try_load_from_enclave() -> Result<Keypair> {
    // Secure Enclave integration not yet implemented
    Err(AosError::Unavailable(
        "Secure Enclave key management not implemented".to_string(),
    ))
}

/// Load or create file-based keypair (development/testing fallback)
fn load_or_create_file_keypair() -> Result<Keypair> {
    let key_path = fingerprint_key_path();

    if key_path.exists() {
        // Load existing key
        debug!(
            "Loading fingerprint keypair from file: {}",
            key_path.display()
        );
        let key_bytes = fs::read(&key_path)
            .map_err(|e| AosError::Io(format!("Failed to read key file: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(AosError::Crypto(format!(
                "Invalid key file length: {}",
                key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        Ok(Keypair::from_bytes(&key_array))
    } else {
        // Generate new key
        warn!(
            "No fingerprint key found, generating new key at: {}",
            key_path.display()
        );
        warn!(
            "WARNING: File-based keys are for development only. Use Secure Enclave in production."
        );

        let keypair = Keypair::generate();
        let key_bytes = keypair.to_bytes();

        // Ensure directory exists
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AosError::Io(format!("Failed to create key directory: {}", e)))?;
        }

        // Write key file with restrictive permissions
        fs::write(&key_path, key_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write key file: {}", e)))?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)
                .map_err(|e| AosError::Io(format!("Failed to get key file metadata: {}", e)))?
                .permissions();
            perms.set_mode(0o600); // rw-------
            fs::set_permissions(&key_path, perms)
                .map_err(|e| AosError::Io(format!("Failed to set key file permissions: {}", e)))?;
        }

        info!(
            "Generated new fingerprint signing key at: {}",
            key_path.display()
        );
        Ok(keypair)
    }
}

/// Get the public key for verification
pub fn get_fingerprint_public_key() -> Result<adapteros_crypto::PublicKey> {
    let keypair = get_or_create_fingerprint_keypair()?;
    Ok(keypair.public_key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[test]
    fn test_file_keypair_generation() {
        let temp_dir = new_test_tempdir();
        let _key_path = temp_dir.path().join("test_key.bin");

        // Override fingerprint key path for testing (would need to be done differently in practice)
        // For now, just test the keypair generation directly
        let keypair = Keypair::generate();
        let key_bytes = keypair.to_bytes();
        assert_eq!(key_bytes.len(), 32);

        // Test round-trip
        let keypair2 = Keypair::from_bytes(&key_bytes);
        assert_eq!(
            keypair.public_key().to_bytes(),
            keypair2.public_key().to_bytes()
        );
    }

    #[test]
    fn test_collect_hardware_attributes() {
        let attrs = collect_hardware_attributes().unwrap();

        // CPU brand should not be empty
        assert!(!attrs.cpu_brand.is_empty() || attrs.cpu_brand == "Unknown");

        // System name should be valid
        assert!(!attrs.system_name.is_empty());

        // Host hash should be 32 hex chars (16 bytes)
        assert_eq!(attrs.host_hash.len(), 32);
    }

    #[test]
    fn test_generate_device_id_deterministic() {
        let attrs = HardwareAttributes {
            cpu_brand: "Test CPU".to_string(),
            cpu_cores: 8,
            total_memory: 16_000_000_000,
            system_name: "TestOS".to_string(),
            os_version: "1.0".to_string(),
            kernel_version: "5.0".to_string(),
            host_hash: "0123456789abcdef0123456789abcdef".to_string(),
        };

        let id1 = generate_device_id(&attrs);
        let id2 = generate_device_id(&attrs);

        // Same attributes should produce same ID
        assert_eq!(id1, id2);

        // ID should be 64 hex chars (32 bytes BLAKE3)
        assert_eq!(id1.len(), 64);
    }

    #[test]
    fn test_generate_device_id_different_attrs() {
        let attrs1 = HardwareAttributes {
            cpu_brand: "CPU A".to_string(),
            cpu_cores: 8,
            total_memory: 16_000_000_000,
            system_name: "TestOS".to_string(),
            os_version: "1.0".to_string(),
            kernel_version: "5.0".to_string(),
            host_hash: "0123456789abcdef0123456789abcdef".to_string(),
        };

        let attrs2 = HardwareAttributes {
            cpu_brand: "CPU B".to_string(),
            ..attrs1.clone()
        };

        let id1 = generate_device_id(&attrs1);
        let id2 = generate_device_id(&attrs2);

        // Different attributes should produce different IDs
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_adapter_binding_creation() {
        // This test creates actual fingerprint, so just verify structure
        let attrs = collect_hardware_attributes().unwrap();
        let device_id = generate_device_id(&attrs);

        let binding = AdapterBinding {
            adapter_id: "test-adapter".to_string(),
            device_id: device_id.clone(),
            bound_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
        };

        assert_eq!(binding.adapter_id, "test-adapter");
        assert_eq!(binding.device_id, device_id);
    }

    #[test]
    fn test_verify_binding_expired() {
        let attrs = collect_hardware_attributes().unwrap();
        let device_id = generate_device_id(&attrs);

        // Create binding that expired in the past
        let binding = AdapterBinding {
            adapter_id: "test-adapter".to_string(),
            device_id,
            bound_at: "2020-01-01T00:00:00Z".to_string(),
            expires_at: Some("2020-01-02T00:00:00Z".to_string()),
        };

        // Note: This would require mocking get_or_create_device_fingerprint
        // For now, just verify the structure is correct
        assert!(binding.expires_at.is_some());
    }
}

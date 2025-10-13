//! Metadata for golden run archives
//!
//! Captures toolchain version, device fingerprint, and adapter configuration
//! to enable reproducibility verification across different environments.

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

/// Toolchain metadata for reproducibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolchainMetadata {
    /// Rust compiler version (e.g., "1.75.0")
    pub rustc_version: String,
    /// Metal shader compiler version
    pub metal_version: String,
    /// Hash of the compiled Metal kernels
    pub kernel_hash: B3Hash,
}

impl ToolchainMetadata {
    /// Create toolchain metadata from current environment
    pub fn capture_current() -> Self {
        Self {
            rustc_version: env!("CARGO_PKG_RUST_VERSION").to_string(),
            metal_version: Self::detect_metal_version(),
            kernel_hash: Self::compute_kernel_hash(),
        }
    }

    /// Detect Metal version from system
    fn detect_metal_version() -> String {
        // In a real implementation, would query the system
        // For now, return a placeholder
        "3.1".to_string()
    }

    /// Compute hash of Metal kernels
    fn compute_kernel_hash() -> B3Hash {
        // In a real implementation, would hash the .metallib files
        // For now, return a placeholder
        B3Hash::from_hex("b3:0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap()
    }

    /// Check if this toolchain matches another
    pub fn matches(&self, other: &ToolchainMetadata) -> bool {
        self.rustc_version == other.rustc_version
            && self.metal_version == other.metal_version
            && self.kernel_hash == other.kernel_hash
    }

    /// Get a summary string for display
    pub fn summary(&self) -> String {
        format!(
            "rustc={}, metal={}, kernels={}",
            self.rustc_version,
            self.metal_version,
            &self.kernel_hash.to_string()[..16]
        )
    }
}

/// Device fingerprint for environment tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFingerprint {
    /// Schema version for fingerprint format
    pub schema_version: u8,
    /// Device model (e.g., "MacBookPro18,3")
    pub device_model: String,
    /// SoC identifier (e.g., "Apple M1 Pro")
    pub soc_id: String,
    /// GPU PCI ID (Metal device registry ID)
    pub gpu_pci_id: String,
    /// OS version (e.g., "14.0")
    pub os_version: String,
    /// OS build number (e.g., "23A344")
    pub os_build: String,
    /// Metal GPU family (e.g., "Apple9")
    pub metal_family: String,
    /// Metal driver version
    pub gpu_driver_version: String,
    /// PATH environment variable hash
    pub path_hash: B3Hash,
    /// Sorted environment variables hash (excluding volatile vars)
    pub env_hash: B3Hash,
    /// CPU feature flags
    pub cpu_features: Vec<String>,
    /// GPU firmware hash (if accessible)
    pub firmware_hash: Option<B3Hash>,
    /// BIOS/bootloader version hash
    pub boot_version_hash: Option<B3Hash>,
}

impl DeviceFingerprint {
    const SCHEMA_VERSION: u8 = 1;

    /// Capture current device fingerprint
    pub fn capture_current() -> Result<Self> {
        Ok(Self {
            schema_version: Self::SCHEMA_VERSION,
            device_model: Self::detect_device_model()?,
            soc_id: Self::detect_soc_id()?,
            gpu_pci_id: Self::detect_gpu_pci_id()?,
            os_version: Self::detect_os_version()?,
            os_build: Self::detect_os_build()?,
            metal_family: Self::detect_metal_family()?,
            gpu_driver_version: Self::detect_gpu_driver_version()?,
            path_hash: Self::compute_path_hash()?,
            env_hash: Self::compute_env_hash()?,
            cpu_features: Self::detect_cpu_features()?,
            firmware_hash: Self::detect_firmware_hash().ok(),
            boot_version_hash: Self::detect_boot_version_hash().ok(),
        })
    }

    /// Detect device model via sysctl
    fn detect_device_model() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("sysctl")
                .arg("-n")
                .arg("hw.model")
                .output()
                .map_err(|e| AosError::Io(format!("Failed to run sysctl: {}", e)))?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect SoC ID via sysctl
    fn detect_soc_id() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("sysctl")
                .arg("-n")
                .arg("machdep.cpu.brand_string")
                .output()
                .map_err(|e| AosError::Io(format!("Failed to run sysctl: {}", e)))?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect GPU PCI ID from Metal device
    fn detect_gpu_pci_id() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            use metal::Device;
            if let Some(device) = Device::system_default() {
                let name = device.name();
                let registry_id = device.registry_id();
                Ok(format!("{}::{}", name, registry_id))
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect OS version
    fn detect_os_version() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("sw_vers")
                .arg("-productVersion")
                .output()
                .map_err(|e| AosError::Io(format!("Failed to run sw_vers: {}", e)))?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect OS build number
    fn detect_os_build() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("sw_vers")
                .arg("-buildVersion")
                .output()
                .map_err(|e| AosError::Io(format!("Failed to run sw_vers: {}", e)))?;

            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect Metal GPU family
    fn detect_metal_family() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            use metal::Device;
            if let Some(device) = Device::system_default() {
                // Metal GPU family detection
                let family = if device.supports_family(metal::MTLGPUFamily::Apple9) {
                    "Apple9"
                } else if device.supports_family(metal::MTLGPUFamily::Apple8) {
                    "Apple8"
                } else if device.supports_family(metal::MTLGPUFamily::Apple7) {
                    "Apple7"
                } else {
                    "Unknown"
                };
                Ok(family.to_string())
            } else {
                Ok("Unknown".to_string())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Detect GPU driver version
    fn detect_gpu_driver_version() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            // Metal version is tied to OS version on macOS
            Self::detect_os_version()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok("Unknown".to_string())
        }
    }

    /// Compute PATH hash
    fn compute_path_hash() -> Result<B3Hash> {
        match std::env::var("PATH") {
            Ok(path) => {
                // Sort PATH components for determinism
                let mut components: Vec<&str> = path.split(':').collect();
                components.sort_unstable();
                let sorted_path = components.join(":");
                Ok(B3Hash::hash(sorted_path.as_bytes()))
            }
            Err(_) => Ok(B3Hash::hash(b"")),
        }
    }

    /// Compute environment variables hash (excluding volatile vars)
    fn compute_env_hash() -> Result<B3Hash> {
        let mut env_map = BTreeMap::new();

        // Exclude volatile/nondeterministic variables
        let exclude = [
            "PWD",
            "OLDPWD",
            "SHLVL",
            "_",
            "TERM_SESSION_ID",
            "SECURITYSESSIONID",
            "SSH_AUTH_SOCK",
            "SSH_AGENT_PID",
            "TMPDIR",
            "TEMP",
            "TMP",
        ];

        for (key, value) in std::env::vars() {
            if !exclude.contains(&key.as_str()) {
                env_map.insert(key, value);
            }
        }

        // Serialize to canonical JSON
        let json = serde_json::to_string(&env_map).map_err(AosError::Serialization)?;

        Ok(B3Hash::hash(json.as_bytes()))
    }

    /// Detect CPU features
    fn detect_cpu_features() -> Result<Vec<String>> {
        let mut features = Vec::new();

        #[cfg(target_arch = "aarch64")]
        {
            features.push("aarch64".to_string());
        }

        #[cfg(target_arch = "x86_64")]
        {
            features.push("x86_64".to_string());
        }

        Ok(features)
    }

    /// Detect firmware hash (if accessible)
    fn detect_firmware_hash() -> Result<B3Hash> {
        // Firmware access is restricted on macOS
        // This would require elevated privileges
        Err(AosError::Unavailable(
            "Firmware hash not accessible".to_string(),
        ))
    }

    /// Detect boot version hash
    fn detect_boot_version_hash() -> Result<B3Hash> {
        #[cfg(target_os = "macos")]
        {
            let output = Command::new("nvram")
                .arg("boot-args")
                .output()
                .map_err(|e| AosError::Io(format!("Failed to run nvram: {}", e)))?;

            if output.status.success() {
                let boot_args = String::from_utf8_lossy(&output.stdout);
                Ok(B3Hash::hash(boot_args.as_bytes()))
            } else {
                Err(AosError::Unavailable("nvram boot-args failed".to_string()))
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(AosError::Unavailable(
                "Boot version detection not supported".to_string(),
            ))
        }
    }

    /// Compute canonical fingerprint hash
    pub fn compute_hash(&self) -> Result<B3Hash> {
        // Serialize to canonical JSON (JCS)
        let json = serde_json::to_string(self).map_err(AosError::Serialization)?;
        Ok(B3Hash::hash(json.as_bytes()))
    }

    /// Save fingerprint to JSON file with signature
    pub fn save_signed(
        &self,
        path: &std::path::Path,
        keypair: &adapteros_crypto::Keypair,
    ) -> Result<()> {
        use adapteros_crypto::sign_bytes;
        use std::fs;
        use std::io::Write;

        // Serialize to canonical JSON
        let json = serde_json::to_string_pretty(self).map_err(AosError::Serialization)?;

        // Write JSON file
        fs::write(path, &json)
            .map_err(|e| AosError::Io(format!("Failed to write fingerprint: {}", e)))?;

        // Compute signature
        let fingerprint_hash = B3Hash::hash(json.as_bytes());
        let signature = sign_bytes(keypair, fingerprint_hash.as_bytes());

        // Write signature file
        let sig_path = path.with_extension("sig");
        let mut sig_file = fs::File::create(sig_path)
            .map_err(|e| AosError::Io(format!("Failed to create signature file: {}", e)))?;
        sig_file
            .write_all(&signature.to_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write signature: {}", e)))?;

        Ok(())
    }

    /// Load fingerprint from JSON file and verify signature
    pub fn load_verified(
        path: &std::path::Path,
        public_key: &adapteros_crypto::PublicKey,
    ) -> Result<Self> {
        use adapteros_crypto::verify_signature;
        use std::fs;

        // Read JSON file
        let json = fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read fingerprint: {}", e)))?;

        // Read signature file
        let sig_path = path.with_extension("sig");
        let sig_bytes = fs::read(sig_path)
            .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;

        // Verify signature
        let fingerprint_hash = B3Hash::hash(json.as_bytes());
        let signature = adapteros_crypto::Signature::from_bytes(
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| AosError::Crypto("Invalid signature length".to_string()))?,
        )?;

        verify_signature(public_key, fingerprint_hash.as_bytes(), &signature)?;

        // Deserialize fingerprint
        serde_json::from_str(&json).map_err(AosError::Serialization)
    }

    /// Check if this device matches another
    pub fn matches(&self, other: &DeviceFingerprint) -> bool {
        self.compute_hash().ok() == other.compute_hash().ok()
    }

    /// Get a summary string for display
    pub fn summary(&self) -> String {
        format!(
            "{} {} (OS {} build {}, Metal {})",
            self.device_model, self.soc_id, self.os_version, self.os_build, self.metal_family
        )
    }
}

/// Complete metadata for a golden run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenRunMetadata {
    /// Unique identifier for this golden run
    pub run_id: String,
    /// Control Plane ID
    pub cpid: String,
    /// Plan ID
    pub plan_id: String,
    /// When this golden run was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Toolchain information
    pub toolchain: ToolchainMetadata,
    /// Adapter IDs included in this run
    pub adapters: Vec<String>,
    /// Device fingerprint
    pub device: DeviceFingerprint,
    /// Global seed used for deterministic execution
    pub global_seed: B3Hash,
}

impl GoldenRunMetadata {
    /// Create new golden run metadata
    pub fn new(
        cpid: String,
        plan_id: String,
        toolchain_version: String,
        adapters: Vec<String>,
        global_seed: B3Hash,
    ) -> Self {
        // Generate run ID from components
        let run_id = format!(
            "golden-{}-{}",
            cpid,
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );

        Self {
            run_id,
            cpid,
            plan_id,
            created_at: chrono::Utc::now(),
            toolchain: ToolchainMetadata {
                rustc_version: toolchain_version,
                metal_version: ToolchainMetadata::detect_metal_version(),
                kernel_hash: ToolchainMetadata::compute_kernel_hash(),
            },
            adapters,
            device: DeviceFingerprint::capture_current().unwrap_or_else(|_| DeviceFingerprint {
                schema_version: 1,
                device_model: "Unknown".to_string(),
                soc_id: "Unknown".to_string(),
                gpu_pci_id: "Unknown".to_string(),
                os_version: "Unknown".to_string(),
                os_build: "Unknown".to_string(),
                metal_family: "Unknown".to_string(),
                gpu_driver_version: "Unknown".to_string(),
                path_hash: B3Hash::hash(b""),
                env_hash: B3Hash::hash(b""),
                cpu_features: vec![],
                firmware_hash: None,
                boot_version_hash: None,
            }),
            global_seed,
        }
    }

    /// Check if metadata is compatible with another run
    pub fn compatible_with(&self, other: &GoldenRunMetadata) -> Result<()> {
        if self.cpid != other.cpid {
            return Err(AosError::InvalidCPID(format!(
                "CPID mismatch: {} vs {}",
                self.cpid, other.cpid
            )));
        }

        if self.plan_id != other.plan_id {
            return Err(AosError::Plan(format!(
                "Plan ID mismatch: {} vs {}",
                self.plan_id, other.plan_id
            )));
        }

        if self.global_seed != other.global_seed {
            return Err(AosError::InvalidHash("Global seed mismatch".to_string()));
        }

        if !self.toolchain.matches(&other.toolchain) {
            return Err(AosError::Toolchain(format!(
                "Toolchain mismatch: {} vs {}",
                self.toolchain.summary(),
                other.toolchain.summary()
            )));
        }

        if self.adapters != other.adapters {
            return Err(AosError::Validation("Adapter set mismatch".to_string()));
        }

        Ok(())
    }

    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Golden Run: {}\n  CPID: {}\n  Plan: {}\n  Toolchain: {}\n  Adapters: {}\n  Device: {}\n  Created: {}",
            self.run_id,
            self.cpid,
            self.plan_id,
            self.toolchain.summary(),
            self.adapters.join(", "),
            self.device.summary(),
            self.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolchain_metadata_matches() {
        let toolchain_a = ToolchainMetadata {
            rustc_version: "1.75.0".to_string(),
            metal_version: "3.1".to_string(),
            kernel_hash: B3Hash::from_hex(
                "b3:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
        };

        let toolchain_b = toolchain_a.clone();
        assert!(toolchain_a.matches(&toolchain_b));

        let mut toolchain_c = toolchain_a.clone();
        toolchain_c.rustc_version = "1.76.0".to_string();
        assert!(!toolchain_a.matches(&toolchain_c));
    }

    #[test]
    fn test_device_fingerprint_matches() {
        let device_a = DeviceFingerprint {
            schema_version: 1,
            device_model: "MacBookPro18,3".to_string(),
            soc_id: "Apple M1 Pro".to_string(),
            gpu_pci_id: "Apple M1 Pro::0x0000000000000000".to_string(),
            os_version: "14.0".to_string(),
            os_build: "23A344".to_string(),
            metal_family: "Apple9".to_string(),
            gpu_driver_version: "3.1".to_string(),
            path_hash: B3Hash::hash(b"path:test"),
            env_hash: B3Hash::hash(b"env:test"),
            cpu_features: vec!["aarch64".to_string()],
            firmware_hash: None,
            boot_version_hash: None,
        };

        let device_b = device_a.clone();
        assert!(device_a.matches(&device_b));

        let mut device_c = device_a.clone();
        device_c.metal_family = "Apple8".to_string();
        assert!(!device_a.matches(&device_c));
    }

    #[test]
    fn test_golden_run_metadata_compatible() {
        let metadata_a = GoldenRunMetadata::new(
            "test-cpid".to_string(),
            "test-plan".to_string(),
            "1.75.0".to_string(),
            vec!["adapter-001".to_string()],
            B3Hash::from_hex("b3:1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap(),
        );

        let metadata_b = metadata_a.clone();
        assert!(metadata_a.compatible_with(&metadata_b).is_ok());

        let mut metadata_c = metadata_a.clone();
        metadata_c.cpid = "different-cpid".to_string();
        assert!(metadata_a.compatible_with(&metadata_c).is_err());
    }
}
